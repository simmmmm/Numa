#!/usr/bin/env python3
"""The parity ratchet: differences between clients may only become fewer.

    dev/parity.py                 check; exit 1 when a count is above dev/parity-baseline.txt
    dev/parity.py --apple DIR     also check the Apple app (a Numa-mac checkout's app/Numa)
    dev/parity.py --baseline      write the counts; refused when any would grow (no override)
    dev/parity.py --report        what is missing where, no verdict

Two counts per client, and one for the list:
    words   a word of dev/parity-words.txt that the client's sources do not contain
    listed  the rows under "Differences" in docs/PARITY.md (code, flow, design)
Apple is counted only when --apple is given (Numa-mac's own check passes it).
"""
import pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / "dev/parity-baseline.txt"
SOURCES = {
    "linux": ["src/ui/**/*.rs"],
    "home": ["crates/numa-server/src/web/*.js", "crates/numa-server/src/web/*.html"],
}

def words():
    return [w for w in (ROOT / "dev/parity-words.txt").read_text().splitlines() if w.strip() and not w.startswith(("#", "["))]

def text(base, globs):
    return "\n".join(p.read_text(errors="ignore") for g in globs for p in base.glob(g))

def listed():
    page = ROOT / "docs/PARITY.md"
    if not page.exists():  # the public copy has no docs/PARITY.md
        return None
    part = page.read_text().split("## Differences", 1)[1].split("\n## ", 1)[0]
    return sum(1 for line in part.splitlines() if line.startswith("| ") and not line.startswith(("| Screen", "|---")))

def counts(apple):
    sources = {client: text(ROOT, globs) for client, globs in SOURCES.items()}
    if apple:
        sources["apple"] = text(pathlib.Path(apple), ["*.swift", "Localizable.xcstrings"])
    missing = {client: [w for w in words() if w not in body] for client, body in sources.items()}
    found = {f"words {client}": len(m) for client, m in missing.items()}
    if (n := listed()) is not None:
        found["listed"] = n
    return found, missing

def read_baseline():
    rows = [l.split() for l in BASELINE.read_text().splitlines() if l.strip() and not l.startswith("#")] if BASELINE.exists() else []
    return {" ".join(r[:-1]): int(r[-1]) for r in rows}

def main(args):
    apple = args[args.index("--apple") + 1] if "--apple" in args else None
    found, missing = counts(apple)
    if "--report" in args:
        for client, m in missing.items():
            print(f"{client}: {len(m)} missing" + "".join(f"\n  {w}" for w in m))
        print(f"listed: {found.get('listed')}")
        return 0
    base = read_baseline()
    grown = [k for k, n in found.items() if k in base and n > base[k]] + [k for k, n in found.items() if k not in base and n > 0]
    if "--baseline" in args:
        if grown and BASELINE.exists():  # the first baseline is today's count
            print("parity baseline would grow (" + ", ".join(grown) + "): fix the difference instead", file=sys.stderr)
            return 1
        merged = {**base, **found}
        BASELINE.write_text("# parity baseline: differences per client and listed. May only shrink.\n" + "".join(f"{k} {n}\n" for k, n in sorted(merged.items())))
        print("baseline written: " + ", ".join(f"{k} {n}" for k, n in sorted(merged.items())))
        return 0
    if grown:
        for k in grown:
            print(f"parity: {k} is {found[k]}, the baseline allows {base.get(k, 0)}", file=sys.stderr)
            client = k.split()[-1]
            for w in missing.get(client, []):
                print(f"  missing on {client}: {w}", file=sys.stderr)
        print("A difference between clients was added. Fix it (docs/PARITY.md, numa-brain: A feature works the same on every client).", file=sys.stderr)
        return 1
    shrunk = [k for k, n in found.items() if n < base.get(k, n)]
    print("parity: " + ", ".join(f"{k} {n}" for k, n in sorted(found.items())) + (f"; {len(shrunk)} shrank: run --baseline to lock them in" if shrunk else ""))
    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
