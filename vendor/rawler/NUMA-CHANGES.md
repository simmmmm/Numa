# rawler, as Numa carries it

This is rawler 0.8.0 as published on crates.io (dnglab, git `ae01bcb2`),
LGPL-2.1 (`LICENSE`), with the changes listed below. Numa carries it so that
its raw decoders can be made faster; every change keeps the decoded frame
bit for bit what 0.8.0 produces, checked by hashing the mosaic of every test
raw before and after.

Left out of the published crate: `src/bin`, `tests`, `benches`,
`data/testdata`, and the targets and dev-dependencies in `Cargo.toml` that
named them. Nothing else was removed.

## Changes

### PERF-021: bits and codes, faster on one thread (28 September 2026)

- `pumps.rs`: `BitPumpMSB` keeps its bits at the top of a 64-bit word and
  refills eight bytes at a time, where it took one 32-bit chunk through an
  iterator. The same bits in the same order; a refill with nothing left
  still panics. Every decoder that reads a plain most-significant-first
  stream uses it (NEF, ORF, ARW, PEF, SRW, RAF, …).
- `decompressors/ljpeg/huffman.rs`: the decode cache holds one `u32` an entry
  (bits to consume, difference) where an `Option<(u8, i16)>` took six bytes,
  and covers 12 bits rather than 13, so it is 16 KB instead of 48 and stays in
  the first-level cache. A code that reads no bits is left to the slow path,
  which reads it the same way. The decode functions take their pump as a
  generic rather than `dyn`.

Measured, one thread: a 22 MP CR2 99 → 76 ms, 50 MP CR2 240 → 177 ms, 24 MP
NEF 87 → 54 ms, 45 MP NEF 168 → 106 ms, ORF 138 → 134 ms.

### PERF-022: one stream on several threads (28 September 2026)

- `decompressors/ljpeg/parallel.rs` (new): a Huffman-coded stream cut into
  up to four parts, each read from its own first byte on a pool of its own
  (not the global one, whose idle threads spin), the joins found where the
  reading of one part meets a code boundary noted at the start of the next.
  Huffman codes resynchronise within a few codes, and from a boundary both
  readings agree, so the result is exactly the single reader's; a stream
  whose parts do not join is read in one go as before. Also the JPEG
  unstuffing (the zero after each 0xFF) done side by side.
- `decompressors/ljpeg/decompressors.rs`: `decode_ljpeg` takes that path for
  a frame of predictor 1 whose components share one table.
- `decoders/cr2.rs`: a plain raw frame in vertical fields is decoded straight
  into its place in the picture, without the frame in between and the copy
  field by field (`fields_in_place`); the end of `raw_image` moved into
  `finish` so both paths share it. sRAW and the hinted models go as before.
- `decoders/nef.rs`: a lossless NEF with one table throughout takes the same
  path (`decode_parallel`); the dither's random number, a multiply-with-carry
  generator, is advanced to each row's start as a power modulo
  15700 · 2¹⁶ − 1.
- `pumps.rs`: `BitPumpMSB::new_zero_padded` and `bit_pos`;
  `decompressors/ljpeg/huffman.rs`: `HuffTable::same_code`.

Measured, wall / processor time: 22 MP CR2 99/98 → 20/76 ms, 50 MP CR2
239/239 → 53/174 ms, 24 MP NEF 88/87 → 23/85 ms, 45 MP NEF 168/168 → 48/169
ms. The mosaic hashes the same on 502 raws of every make from raw.pixls.us.

### PERF-027: ORF's prediction without branches (28 September 2026)

- `decoders/orf.rs`: the predictor's choice between its three candidates is
  worked out as selects rather than nested branches, which the noise in a
  picture kept mispredicting. The same arithmetic, wrapping where the
  release build always wrapped. (Replacing the loop that finds `nbits` with a
  closed form was tried and was slower.)

Measured, one thread: an E-M1 Mark II ORF 133 → 104 ms. The mosaic hashes the
same on all 23 ORFs from raw.pixls.us and both test frames.
- `imgop/sensor/bayer/ppg.rs`: the same for PPG — the green pass works out
  all four directional candidates and keeps the first with the least
  gradient, `hue_transit` and the red/blue pass work out both and keep one.
  Each candidate is the expression it was.

Measured, four threads: the Bayer demosaic stage of a 22 MP CR2 161 → 135 ms,
of a 45 MP NEF 327 → 277 ms. Decode, proxy and default render hash the same
on the ten test raws.

### PERF-029: Markesteijn in larger tiles (28 September 2026)

- `imgop/sensor/xtrans/markesteijn.rs`: tiles of 128 pixels rather than 64.
  Each tile computes a margin it does not keep; at 64 that was 2.6 times the
  pixels written, at 128 it is 1.5 times. A pixel's value depends only on its
  neighbourhood within the margin, so the output does not change: the whole
  decode, proxy and render hash the same on all 70 RAFs from raw.pixls.us.

Measured on the X-T5, four threads: the demosaic stage 1180 → 970 ms, the
whole decode 7.4 → 6.5 processor-seconds; sixteen threads 634 → 551 ms.
- The same file: `compute_green_bounds` row by row on all threads, where it
  ran over the whole frame on one. Each pixel's bounds are its own
  neighbours'; identical on the 70 RAFs. X-T5, sixteen threads: the demosaic
  stage 546 → 426 ms.

### IO-021: a look at a raw reads what it looks at (28 September 2026)

- `rawsource.rs`: `RawSource::new` maps the file without `populate` and
  without the `WillNeed`/`Sequential` advice, so the kernel reads the pages a
  reader touches instead of the whole file before anything is parsed. A
  decode touches them all and is no slower (explore-build measured it cold:
  5DS 561 → 528 ms, X-T5 944 → 904). The same SIGBUS an mmap always had if
  the file is cut short under it; `populate` never prevented that either.
- `rawsource.rs`: `subview_padded_or_dummy` and
  `subview_until_eof_padded_or_dummy`. Padding a view that ends at the end of
  the file copied all of it — the raw data of a NEF, ORF, RAF or RW2 — even
  for a dummy decode (the geometry only), which reads none of it; a dummy one
  now gets zeros of the same length, allocated but neither read nor written.
  Used by `decoders/nef.rs`, `orf.rs`, `raf.rs` and `rw2.rs` where they took
  the padded views.
- `decoders/dng.rs`: a dummy decode no longer decodes the frame
  (`plain_image_from_ifd`) for its size.
- `decoders/arw.rs`, `decoders/mod.rs`: `file.buf()` where `file.as_vec()`
  copied the whole file into memory to read a few kilobytes of it (the SR2
  private data, the key tables, a file size).

Measured in Numa, each file evicted from the page cache first, on 560
files (every raw.pixls.us make, the ten test raws, the photographer's own):
an embedded preview 44.2 → 2.8 MB read a file, a summary 44.1 → 10.7 MB and
29 → 3.8 ms; per test raw a summary reads 0.3–2.6 MB where it read 17–86.
Every preview and every summary the same (Numa's `ENGINEERING.md`, 28
September); one DNG that failed its full decode now has a summary.

### PERF-061: Fuji's samples without a dispatch each (28 September 2026)

- `decoders/raf/fuji_decompressor.rs`: `read_code` is `#[inline(always)]`
  instead of `#[multiversion]`, and so are the sample and block functions
  above it, down to `decompress_strip`. multiversion gives a function with
  arguments a dispatcher that runs on every call, and the clone it picks is
  a `#[target_feature]` function, which cannot be inlined into its caller:
  every sample of an X-Trans frame paid a call and a dispatch. The LZCNT the
  clone was for bought less than that cost; multiversioning the whole strip
  instead was measured and was slower than plain inlining.

Measured, X-T5, medians of three runs: one core 971 → 890 ms, sixteen
threads 125 → 115 ms wall and 1151 → 1040 processor-ms. The mosaic hashes
the same on all 70 RAFs from raw.pixls.us and the test frame.

### PERF-062: a code's entry in the decode cache when its difference does not fit (28 September 2026)

- `decompressors/ljpeg/huffman.rs`: where a code ends within the cache's
  12 bits but its difference does not, the cache entry now holds the code's
  length, shift and difference length (`CODE_ONLY`), so only the difference
  is left to read. Before, such a code went to the slow path, which peeked
  the table's full width again and looked it up in `hufftable` — 64 K
  entries of three bytes for a 16-bit table, which does not stay in the
  first-level cache. The difference is read by the same `huff_diff` as
  before. The idea is rawspeed's prefix-code decoder (LGPL-2.1, read, not
  copied): a table entry that is either the whole decoded difference or
  the code with the difference still to read.

Measured over the 184 lossless-JPEG and Huffman-coded raws of raw.pixls.us
(CR2, NEF, DNG, PEF, 3FR…), sixteen threads: processor time −2.3 % in all,
−20 to −34 % on the frames whose differences are long — older Canon CR2s,
lossless NEFs from the D3/D3S/D4S/D300S, Leica M DNGs; the 5D Mark III one
core 71.7 → 70.4 ms. The mosaic hashes the same on all 528 raws.

### PERF-063: uncompressed frames unpacked in two runs, not a task per row (28 September 2026)

- `decompressors/mod.rs`: `unpack_lines_fn` (new), and `decompress_lines`,
  split the frame into two runs of rows where they made a rayon task of
  every row; `decompressors/packed.rs` unpacks with it. Unpacking is moving
  memory: on sixteen threads a task per row was no faster than one thread
  and cost seven times the processor time. The idea is rawspeed's, which
  copies an uncompressed frame on one thread (read, not copied). The
  decoders that do real work per row (ARW2, RW2, NEF, IIQ) keep
  `decompress_lines_fn`.

Measured, a 42 MP uncompressed ARW (A7R3), sixteen threads: 10.8 ms and 78
processor-ms → 8.8 ms and 12.3; one run was 10.4 / 10.3, four 8.4 / 18.
Over the 123 uncompressed and 16-bit raws of raw.pixls.us: processor time
−24 %, wall time within the noise; an uncompressed X-H2S RAF 52 → 6
processor-ms. The mosaic hashes the same on all 528 raws.

### IO-008: the Sony ILME-FX2 (29 September 2026)

- `data/cameras/sony/fx2.toml` (new): the body, which 0.8.0 and dnglab's
  `main` (still `ae01bcb2` that day) do not know, so its ARWs failed as an
  unknown camera. Nobody has published a matrix of the FX2's own; it has
  the 33 MP sensor of the ILCE-7M4 and ILCE-7CM2, and the entry takes both
  matrices of rawler's own `a7cm2.toml` (Adobe DNG Converter's). Its D65 is
  the one LibRaw's `adobe_coeff` (`src/tables/colordata.cpp`) and rawspeed's
  `cameras.xml` give the ILME-FX2: 7460 −2365 −588 / −5687 13442 2474 /
  −624 1156 6584. Levels and crop come from the file, as for the 7M4.
  (rawler's `a7m4.toml` carries the ILCE-7M3's D65 instead of that one;
  left as it is, like every body already known.)

Checked: the four FX2 frames of raw.pixls.us (8807–8810: uncompressed,
compressed, lossless and lossless medium) decode, make their proxies and
render; the renders of fifteen frames of bodies it knew (the ten test raws,
two 7M4s, a 7CM2, an FX3 and an FX30) hash as before.
