#!/usr/bin/env python3
"""Every Rust crate linked into Numa, with its licence texts, as one file.

    dev/third-party-licences.py [--manifest-path Cargo.toml] [--target T] [-p crate] > THIRD_PARTY_LICENSES.txt

MIT, BSD and Apache ask for their text to travel with every copy, and a link
is not the text (LICENCES_AUDIT.md). cargo-about does this; it is not installed
here, and `cargo metadata` plus the licence files in each crate's source is all
it takes. Only what is built into the app: normal and build dependencies of
`-p` (Numa's own crates are left out), for `--target` when given. Identical
texts are printed once, under every crate that carries them. Fails when a
crate ships no licence file and no other crate carries its licence's text,
so a gap is caught rather than shipped.
"""

import argparse
import json
import os
import re
import subprocess
import sys

NAMES = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright)", re.I)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-path", default="Cargo.toml")
    parser.add_argument("--target")
    parser.add_argument("-p", dest="roots", action="append", default=[])
    args = parser.parse_args()

    command = ["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", args.manifest_path]
    if args.target:
        command += ["--filter-platform", args.target]
    meta = json.loads(subprocess.check_output(command))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    roots = [i for i in members if not args.roots or packages[i]["name"] in args.roots]

    # What is linked: no dev-dependencies, below any of the roots.
    seen, todo = set(), list(roots)
    while todo:
        at = todo.pop()
        if at in seen:
            continue
        seen.add(at)
        for dep in nodes[at]["deps"]:
            if any(kind["kind"] in (None, "build") for kind in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    # Numa's own crates: the workspace's members, and this repository's crates/
    # when another workspace (the Apple app's) builds them. A path crate that is
    # neither is third-party code carried in the tree (vendor/, PERF-021) and is
    # listed like any other.
    ours = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "crates") + os.sep
    own = {i for i in seen if i in members or packages[i]["manifest_path"].startswith(ours)}
    crates = sorted((packages[i] for i in seen if i not in own), key=lambda p: (p["name"], p["version"]))

    def files_of(package):
        folder = os.path.dirname(package["manifest_path"])
        found = []
        for name in sorted(os.listdir(folder)):
            path = os.path.join(folder, name)
            if NAMES.match(name) and os.path.isfile(path):
                found.append(open(path, encoding="utf-8", errors="replace").read().strip())
        if package.get("license_file"):
            path = os.path.join(folder, package["license_file"])
            if os.path.isfile(path):
                text = open(path, encoding="utf-8", errors="replace").read().strip()
                if text not in found:
                    found.append(text)
        return found

    texts = {c["id"]: files_of(c) for c in crates}

    # A crate that ships no file: its licence's text as another crate ships it,
    # under its own authors.
    canonical = {}
    for c in crates:
        if len(texts[c["id"]]) == 1 and c.get("license") and " " not in c["license"]:
            canonical.setdefault(c["license"], texts[c["id"]][0])
    # An own crate with a part under another licence than Numa's — numa-gpu's
    # demosaic.wgsl, rawler's LGPL-2.1 — is listed under that licence's text.
    for i in sorted(own):
        if set(re.findall(r"[A-Za-z0-9.\-]+", packages[i].get("license") or "")) - {"GPL-3.0-or-later", "AND", "OR", "WITH"}:
            crates.append(packages[i])
            texts[i] = []
    missing = []
    for c in crates:
        if texts[c["id"]]:
            continue
        spdx = re.findall(r"[A-Za-z0-9.\-]+", c.get("license") or "")
        spdx = [s for s in spdx if s not in ("OR", "AND", "WITH")]
        if c["id"] in own:
            spdx = [s for s in spdx if s != "GPL-3.0-or-later"]
        # LGPL-2.1-only is the newer name of LGPL-2.1.
        chosen = [canonical.get(s) or canonical.get(s.removesuffix("-only")) for s in spdx]
        chosen = [text for text in chosen if text][:1]
        if not chosen:
            missing.append(f"{c['name']} {c['version']} ({c.get('license')})")
            continue
        authors = ", ".join(c.get("authors") or []) or f"the {c['name']} authors"
        # An own crate's part shares the text it came with (numa-gpu beside rawler).
        texts[c["id"]] = [chosen[0] if c["id"] in own else
                          f"{c.get('license')} — the crate ships no licence file; its authors: {authors}.\n\n{chosen[0]}"]
    if missing:
        sys.exit("no licence text for: " + "; ".join(missing))

    # Each distinct text once, under every crate that carries it.
    groups = {}
    for c in crates:
        for text in texts[c["id"]]:
            groups.setdefault(text, []).append(f"{c['name']} {c['version']} ({c.get('license') or 'see text'})")

    out = sys.stdout
    out.write("Third-party software in Numa\n============================\n\n")
    out.write(f"The {len(crates)} Rust crates built into Numa{' for ' + args.target if args.target else ''}, each with its licence as the crate itself ships it.\n")
    out.write("Where a crate offers a choice of licences, every text it ships is here; Numa takes the permissive one.\n")
    out.write("Generated by dev/third-party-licences.py from cargo metadata.\n\n")
    for c in crates:
        out.write(f"  {c['name']} {c['version']} — {c.get('license') or 'see text'} — {c.get('repository') or ''}\n")
    for text, names in groups.items():
        out.write("\n" + "-" * 78 + "\n")
        out.write("\n".join(names) + "\n")
        out.write("-" * 78 + "\n\n")
        out.write(text + "\n")


if __name__ == "__main__":
    main()
