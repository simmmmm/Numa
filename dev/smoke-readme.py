#!/usr/bin/env python3
"""Every job of the smoke test finds its install lines in the README.

The smoke test (.github/workflows/smoke.yml, packaging/smoke/run.sh) runs the
README's sh block under each job's heading; a README edit that renames a
heading broke four jobs of 0.45.0's run unnoticed. Run by dev/check.sh.
"""
import re
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
workflow = (root / ".github/workflows/smoke.yml").read_text()
readme = (root / "README.md").read_text().splitlines()


def block(heading):
    """The first sh block after the line that starts with `heading`, as run.sh's awk finds it."""
    found = inside = False
    lines = []
    for line in readme:
        if not found:
            found = line.startswith(heading)
        elif not inside:
            inside = line == "```sh"
        elif line == "```":
            break
        else:
            lines.append(line)
    return lines


jobs = re.findall(r'\{ name: "([^"]+)".*?heading: "([^"]+)"', workflow)
missing = [f"{name}: no sh block under {heading}" for name, heading in jobs if not block(heading)]
if not jobs or missing:
    print("\n".join(missing or ["smoke.yml: no jobs found"]), file=sys.stderr)
    sys.exit(1)
