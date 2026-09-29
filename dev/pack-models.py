#!/usr/bin/env python3
"""START-017: pack a model file for the mirror as `<name>.zst` (lossless) or
`<name>.f16.zst` (float32 weights stored as float16), the container
`numa_io::models::unpack` reads. Needs numpy and the `zstd` command.

    dev/pack-models.py [--level 19] [--f16] [--out dir] model.onnx [...]
    dev/pack-models.py --stats model.onnx      # float bytes by consumer

The container, decompressed: b"NUMAPACK", a u32 count, that many
(u64 offset, u64 length, u8 kind) ranges of the original file (little endian,
sorted, not overlapping), then the original with each range in its packed
form. Kind 1: float32 with its bytes grouped by position (every first byte,
then every second ...); kind 2: float16, the same; kind 3: float32 stored as
float16 (round to nearest even), grouped, half the length. Grouped, the
exponent bytes sit together and zstd finds what they repeat (the HDF5/Blosc
shuffle).

The ranges are the tensors' own bytes, found by reading the ONNX protobuf; a
`.onnx.data` file takes them from the `.onnx` beside it (external data).
Streamed: the file is mapped, not read, and one tensor at a time is in memory.
Prints name, size, SHA-256, and the size and SHA-256 of what it unpacks to.
"""
import argparse, hashlib, mmap, os, struct, subprocess
import numpy as np

FLOAT, FLOAT16 = 1, 10
MAGIC = b"NUMAPACK"
# --f16: only these ops' weight inputs, and only tensors this large. Scalars,
# epsilons, shape and index arithmetic stay exactly as they are.
WEIGHT_OPS = {"Conv": (1,), "ConvTranspose": (1,), "MatMul": (0, 1), "Gemm": (0, 1)}
MIN_ELEMENTS = 1024


def varint(buf, pos):
    shift = result = 0
    while True:
        byte = buf[pos]
        pos += 1
        result |= (byte & 0x7F) << shift
        if byte < 0x80:
            return result, pos
        shift += 7


def fields(buf, start, end):
    """(field, wire type, value or (offset, length)) of one message."""
    pos = start
    while pos < end:
        key, pos = varint(buf, pos)
        field, wire = key >> 3, key & 7
        if wire == 0:
            value, pos = varint(buf, pos)
            yield field, wire, value
        elif wire == 1:
            yield field, wire, buf[pos:pos + 8]
            pos += 8
        elif wire == 5:
            yield field, wire, buf[pos:pos + 4]
            pos += 4
        elif wire == 2:
            length, pos = varint(buf, pos)
            yield field, wire, (pos, length)
            pos += length
        else:
            raise ValueError(f"wire type {wire} at {pos}")


def text(buf, span):
    return bytes(buf[span[0]:span[0] + span[1]]).decode()


def tensor(buf, span):
    """TensorProto: name, type, element count, where its data is."""
    name, kind, count, data, external = "", 0, 1, None, {}
    for field, wire, value in fields(buf, span[0], span[0] + span[1]):
        if field == 1:
            if wire == 2:  # packed dims
                pos, end = value[0], value[0] + value[1]
                while pos < end:
                    dim, pos = varint(buf, pos)
                    count *= dim
            else:
                count *= value
        elif field == 2:
            kind = value
        elif field == 8:
            name = text(buf, value)
        elif field in (9, 4) and wire == 2:  # raw_data, or packed float_data
            data = value
        elif field == 13:  # external_data: key, value
            entry = dict((f, text(buf, v)) for f, _, v in fields(buf, value[0], value[0] + value[1]))
            external[entry.get(1)] = entry.get(2)
    return name, kind, count, data, external


def walk_graph(buf, span, tensors, consumers):
    for field, wire, value in fields(buf, span[0], span[0] + span[1]):
        if field == 5 and wire == 2:  # initializer
            tensors.append(tensor(buf, value))
        elif field == 1 and wire == 2:  # node
            op, inputs, outputs = "", [], []
            attributes = []
            for f, w, v in fields(buf, value[0], value[0] + value[1]):
                if f == 1:
                    inputs.append(text(buf, v))
                elif f == 2:
                    outputs.append(text(buf, v))
                elif f == 4:
                    op = text(buf, v)
                elif f == 5:
                    attributes.append(v)
            for index, name in enumerate(inputs):
                consumers.setdefault(name, []).append((op, index, outputs))
            for attribute in attributes:
                for f, w, v in fields(buf, attribute[0], attribute[0] + attribute[1]):
                    if f in (5, 10):  # t, tensors (a Constant's value, named by its output)
                        found = tensor(buf, v)
                        tensors.append((outputs[0] if op == "Constant" and outputs else found[0],) + found[1:])
                    elif f in (6, 11):  # g, graphs
                        walk_graph(buf, v, tensors, consumers)


def model_tensors(path):
    with open(path, "rb") as file:
        buf = memoryview(mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ))
    tensors, consumers = [], {}
    for field, wire, value in fields(buf, 0, len(buf)):
        if field == 7 and wire == 2:
            walk_graph(buf, value, tensors, consumers)
    return tensors, consumers


def ranges(path, f16):
    """(offset, length, kind) of every float tensor's bytes in `path`."""
    onnx = path[:-len(".data")] if path.endswith(".onnx.data") else path
    tensors, consumers = model_tensors(onnx)
    found = []
    for name, kind, count, data, external in tensors:
        if kind not in (FLOAT, FLOAT16):
            continue
        if path != onnx:
            if not external or os.path.basename(path) != external.get("location"):
                continue
            span = (int(external.get("offset", 0)), int(external["length"]))
        elif data is None or external:
            continue
        else:
            span = data
        width = 4 if kind == FLOAT else 2
        if span[1] == 0 or span[1] % width:
            continue
        packed = 2 if kind == FLOAT16 else 1
        if f16 and kind == FLOAT and count >= MIN_ELEMENTS and weight(name, consumers):
            packed = 3
        found.append((span[0], span[1], packed))
    found.sort()
    return found


def weight(name, consumers):
    """Whether every use of `name` is as a weight, through Identity copies."""
    uses = consumers.get(name, [])
    return bool(uses) and all(
        weight(outputs[0], consumers) if op == "Identity" else index in WEIGHT_OPS.get(op, ())
        for op, index, outputs in uses)


def pack_range(data, kind):
    if kind == 1:
        return np.frombuffer(data, np.uint8).reshape(-1, 4).T.tobytes()
    if kind == 2:
        return np.frombuffer(data, np.uint8).reshape(-1, 2).T.tobytes()
    return np.frombuffer(data, "<f4").astype("<f2").view(np.uint8).reshape(-1, 2).T.tobytes()


def fits_half(data):
    with np.errstate(over="ignore"):
        return bool(np.isfinite(np.frombuffer(data, "<f4").astype("<f2")).all())


def pack(path, found, out, level):
    """Write the container through `zstd` a range at a time; the SHA-256 of
    the file `unpack` will make of it, and how many bytes went to float16."""
    with open(path, "rb") as file:
        original = memoryview(mmap.mmap(file.fileno(), 0, access=mmap.ACCESS_READ))
    # A float32 range that does not fit in float16 stays float32.
    kept = [(o, n, 1 if k == 3 and not fits_half(original[o:o + n]) else k) for o, n, k in found]
    zstd = subprocess.Popen(["zstd", "-q", "-f", "-T0", f"-{level}", "-o", out], stdin=subprocess.PIPE)
    zstd.stdin.write(MAGIC + struct.pack("<I", len(kept)) + b"".join(struct.pack("<QQB", *r) for r in kept))
    digest, pos = hashlib.sha256(), 0
    for offset, length, kind in kept + [(len(original), 0, 0)]:
        zstd.stdin.write(original[pos:offset])
        digest.update(original[pos:offset])
        if length:
            data = original[offset:offset + length]
            zstd.stdin.write(pack_range(data, kind))
            digest.update(np.frombuffer(data, "<f4").astype("<f2").astype("<f4").tobytes() if kind == 3 else data)
        pos = offset + length
    zstd.stdin.close()
    if zstd.wait():
        raise SystemExit(f"zstd failed on {path}")
    return digest.hexdigest(), sum(length for _, length, kind in kept if kind == 3)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("files", nargs="+")
    parser.add_argument("--level", type=int, default=19)
    parser.add_argument("--f16", action="store_true", help="float32 weights stored as float16 (lossy)")
    parser.add_argument("--out", default=".")
    parser.add_argument("--stats", action="store_true")
    args = parser.parse_args()
    for path in args.files:
        found = ranges(path, args.f16)
        if args.stats:
            by_kind = {}
            for _, length, kind in found:
                by_kind[kind] = by_kind.get(kind, 0) + length
            print(f"{os.path.basename(path)}: {os.path.getsize(path)} B, ranges {len(found)}, by kind {by_kind}")
            continue
        name = os.path.basename(path) + (".f16" if args.f16 else "") + ".zst"
        out = os.path.join(args.out, name)
        unpacked, lossy = pack(path, found, out, args.level)
        with open(out, "rb") as file:
            packed = hashlib.file_digest(file, "sha256").hexdigest()
        print(f"{name}\t{os.path.getsize(out)}\t{packed}\t{os.path.getsize(path)}\t{unpacked}\tf16 {lossy} B")


if __name__ == "__main__":
    main()
