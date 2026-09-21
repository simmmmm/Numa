#!/usr/bin/env python3
"""The shape ratchet: nothing may get bigger than it already is.

    dev/shape.py            check against dev/shape-baseline.txt; exit 1 on new debt
    dev/shape.py --baseline rewrite the baseline from the tree (only allowed to shrink;
                            pass --force to accept a larger one, and say why in the commit)
    dev/shape.py --report   the tables, no verdict

What it measures, over crates/*/src and src (whichever exist), tests excluded
(`#[cfg(test)] mod tests` blocks and tests/ are skipped):

    file     lines of code (not blank, not `//`) — limit 2000
    fn       lines per function, closures included — limit 150
    struct   fields per struct — limit 30

A "debt" is one file, function or struct over its limit. The baseline lists
every debt that existed when it was written. The check fails when a debt
exists that the baseline does not name, or when a named debt has grown.
Debts that shrank or vanished are fine and are reported so the baseline can
be rewritten smaller. Counting is Claude Code's own from FT-017
(evidence/ft017-*.py), so the numbers here match those tables.
"""
import pathlib, re, sys

LIMITS = {"file": 2000, "fn": 150, "struct": 30}
BASELINE = pathlib.Path(__file__).with_name("shape-baseline.txt")
ROOTS = [p for p in (pathlib.Path("crates"), pathlib.Path("src")) if p.exists()]

fn_start = re.compile(r'^(\s*)(pub(\([\w:]+\))?\s+)?(async\s+|const\s+|unsafe\s+|extern\s+"[^"]*"\s+)*fn\s+(\w+)')
struct_start = re.compile(r'^\s*(pub(\([\w:]+\))?\s+)?struct\s+(\w+)[^;{]*\{')
strip = re.compile(r'"(\\.|[^"\\])*"|\'(\\.|[^\'\\])\'|//.*$')
test_mod = re.compile(r'^\s*#\[cfg\(test\)\]')

def rust_files():
    for root in ROOTS:
        for f in sorted(root.rglob("*.rs")):
            if "tests" in f.parts and f.parts.index("tests") == 0:
                continue
            yield f

def without_tests(lines):
    """Drop `#[cfg(test)]` items (the mod tests block) by brace depth."""
    out, i = [], 0
    while i < len(lines):
        if test_mod.match(lines[i]):
            depth, opened = 0, False
            while i < len(lines):
                clean = strip.sub("", lines[i])
                depth += clean.count("{") - clean.count("}")
                if "{" in clean: opened = True
                i += 1
                if opened and depth <= 0: break
            continue
        out.append(lines[i]); i += 1
    return out

def measure():
    debts = {}  # key -> size
    for f in rust_files():
        lines = without_tests(f.read_text(errors="replace").splitlines())
        code = sum(1 for l in lines if l.strip() and not l.strip().startswith("//"))
        debts.setdefault("file", {})[str(f)] = code
        i = 0
        while i < len(lines):
            m = fn_start.match(lines[i]); s = struct_start.match(lines[i])
            if not (m or s):
                i += 1; continue
            depth, j, opened = 0, i, False
            fields = 0
            while j < len(lines):
                clean = strip.sub("", lines[j])
                if s and depth == 1 and re.match(r'\s*(pub(\([\w:]+\))?\s+)?\w+\s*:', clean): fields += 1
                depth += clean.count("{") - clean.count("}")
                if "{" in clean: opened = True
                if opened and depth <= 0: break
                j += 1
            if opened:
                if m: debts.setdefault("fn", {})[f"{f}::{m.group(5)}"] = j - i + 1
                else: debts.setdefault("struct", {})[f"{f}::{s.group(3)}"] = fields
                i = j + 1
            else:
                i += 1
    return {kind: {k: v for k, v in d.items() if v > LIMITS[kind]} for kind, d in debts.items()}

def read_baseline():
    base = {}
    if BASELINE.exists():
        for line in BASELINE.read_text().splitlines():
            if not line.strip() or line.startswith("#"): continue
            kind, size, key = line.split("\t", 2)
            base.setdefault(kind, {})[key] = int(size)
    return base

def write_baseline(debts):
    rows = ["# shape baseline — every file/function/struct over its limit. May only shrink.",
            "# kind\tsize\tkey"]
    for kind in ("file", "fn", "struct"):
        for key, size in sorted(debts.get(kind, {}).items(), key=lambda kv: -kv[1]):
            rows.append(f"{kind}\t{size}\t{key}")
    BASELINE.write_text("\n".join(rows) + "\n")

def main():
    debts = measure()
    if "--report" in sys.argv:
        for kind in ("file", "fn", "struct"):
            print(f"\n{kind} over {LIMITS[kind]}: {len(debts.get(kind, {}))}")
            for key, size in sorted(debts.get(kind, {}).items(), key=lambda kv: -kv[1])[:15]:
                print(f"  {size:>6}  {key}")
        return 0
    base = read_baseline()
    if "--baseline" in sys.argv:
        total_now = sum(len(d) for d in debts.values()); total_base = sum(len(d) for d in base.values())
        grown = any(size > base.get(kind, {}).get(key, 10**9) for kind, d in debts.items() for key, size in d.items())
        if base and (total_now > total_base or grown) and "--force" not in sys.argv:
            print("baseline would grow; refusing without --force"); return 1
        write_baseline(debts); print(f"baseline written: {total_now} debts"); return 0
    new, grown, shrunk = [], [], []
    for kind, d in debts.items():
        for key, size in d.items():
            was = base.get(kind, {}).get(key)
            if was is None: new.append((kind, size, key))
            elif size > was: grown.append((kind, was, size, key))
    for kind, d in base.items():
        for key, was in d.items():
            now = debts.get(kind, {}).get(key)
            if now is None or now < was: shrunk.append((kind, was, now, key))
    for kind, size, key in new: print(f"NEW    {kind:6} {size:>6}  {key}")
    for kind, was, size, key in grown: print(f"GREW   {kind:6} {was:>6} -> {size:<6} {key}")
    for kind, was, now, key in shrunk: print(f"less   {kind:6} {was:>6} -> {now if now is not None else 'gone':<6} {key}")
    if new or grown:
        print(f"\n{len(new)} new, {len(grown)} grown: the shape got worse. Move one piece out before your change.")
        return 1
    print(f"\nno new debt; {len(shrunk)} debts shrank — run --baseline to lock them in")
    return 0

if __name__ == "__main__":
    sys.exit(main())
