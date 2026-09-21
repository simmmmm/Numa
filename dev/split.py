#!/usr/bin/env python3
"""SPLIT_PLAN A0: move top-level items out of window.rs, unchanged.

    dev/split.py <module> <first> [<last>]    items <first>..<last> into window/<module>.rs
    dev/split.py --test <name>                a `#[cfg(test)] mod <name> { … }` into window/<name>.rs

The range runs from the first item called <first> to the last item called
<last>, so a struct and its impl blocks go together, and everything between
them goes too: doc comments, attributes, blank lines. Moved items, struct
fields and inherent impl methods that have no visibility get `pub(super)`,
because a child module cannot otherwise be seen by its parent; the child sees
the parent through `use super::*`, private fields included, so no call site
changes. `mod <module>; use <module>::*;` go beside the others.

It stops rather than guess: a name that is not a top-level item, or a block
whose braces do not balance, is an error. `dev/moved.py` proves the result.
"""
import pathlib, re, sys

WINDOW = pathlib.Path("src/ui/window.rs")
CHILDREN = pathlib.Path("src/ui/window")

ITEM = re.compile(
    r"^(pub(\([\w:]+\))?\s+)?((async|const|unsafe|extern\s+\"[^\"]*\")\s+)*"
    r"(fn|struct|enum|impl|const|static|type|mod|trait|union)\b"
)
CLOSER = re.compile(r"^[}\])];?\s*$")


def name_of(line):
    """The name a top-level item line declares, or None."""
    if not ITEM.match(line):
        return None
    if re.match(r"^impl\b", line):
        # `impl<T> Name<T>`, `impl Trait for Name`: the type being implemented.
        body = re.sub(r"^impl(<[^>]*>)?\s+", "", line)
        target = body.split(" for ")[-1]
        m = re.match(r"([A-Za-z_]\w*)", target)
        return m.group(1) if m else None
    m = re.search(r"\b(fn|struct|enum|const|static|type|mod|trait|union)\s+([A-Za-z_]\w*)", line)
    return m.group(2) if m else None


def items(lines):
    """Every top-level item as (name, start, end), inclusive, with the doc
    comments and attributes directly above it counted in."""
    out = []
    i = 0
    while i < len(lines):
        line = lines[i]
        name = name_of(line)
        if name is None:
            i += 1
            continue
        start = i
        while start > 0 and re.match(r"^(///|//|#\[)", lines[start - 1]):
            start -= 1
        if line.rstrip().endswith(";") and "{" not in line:
            end = i
        else:
            end = i + 1
            while end < len(lines) and not CLOSER.match(lines[end]):
                end += 1
            if end == len(lines):
                sys.exit(f"{name} at {i + 1}: no closing line at column 0")
        out.append((name, start, end))
        i = end + 1
    return out


def balanced(block):
    """Braces balance, counted outside strings, chars and comments."""
    text, i, depth = "\n".join(block), 0, 0
    while i < len(text):
        c = text[i]
        if text.startswith("//", i):
            i = text.find("\n", i) if text.find("\n", i) >= 0 else len(text)
        elif text.startswith("/*", i):
            i = text.find("*/", i + 2) + 2
        elif re.match(r'b?r#*"', text[i:i + 8]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r'b?r(#*)"', text[i:])
            close = '"' + m.group(1)
            i = text.find(close, i + m.end()) + len(close)
        elif c == '"':
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
            i += 1
        elif c == "'":
            # A char literal, or a lifetime: 'a' is one, 'a is the other.
            m = re.match(r"'(\\.[^']*|[^'\\])'", text[i:])
            i += m.end() if m else 1
        else:
            depth += (c == "{") - (c == "}")
            i += 1
    return depth == 0


def expose(block):
    """`pub(super)` on what has no visibility: items, struct fields, and the
    methods of inherent impls. Trait impls may not have it."""
    out, inside = [], None
    for line in block:
        if re.match(r"^thread_local!\s*\{", line):
            inside = "thread_local"
        elif inside == "thread_local" and re.match(r"^    static\b", line):
            line = "    pub(super) " + line[4:]
        elif ITEM.match(line):
            # A tuple struct's fields, on its one line: `struct X(T, U);`.
            tuple = re.match(r"^(.*\bstruct \w+\()(.*)(\);\s*)$", line)
            if tuple:
                if "<" in tuple.group(2) and "," in tuple.group(2):
                    sys.exit(f"move this one by hand: {line}")
                fields = [f.strip() for f in tuple.group(2).split(",") if f.strip()]
                fields = [f if f.startswith("pub") else "pub(super) " + f for f in fields]
                line = tuple.group(1) + ", ".join(fields) + tuple.group(3)
            if re.match(r"^impl\b", line):
                inside = "trait" if " for " in line else "impl"
            elif re.match(r"^(pub(\([\w:]+\))?\s+)?struct\b", line) and line.rstrip().endswith("{"):
                inside = "struct"
            else:
                inside = None
            if not line.startswith("pub") and not line.startswith("impl"):
                line = "pub(super) " + line
        elif CLOSER.match(line):
            inside = None
        elif inside == "impl" and re.match(r"^    ((async|const|unsafe)\s+)*fn\b|^    const\b", line):
            line = "    pub(super) " + line[4:]
        elif inside == "struct" and re.match(r"^    [a-z_][a-z0-9_]*\s*:", line):
            line = "    pub(super) " + line[4:]
        # A file included by relative path is one directory further away.
        line = re.sub(r'(include_(?:str|bytes)!\(")(?!/)', r"\1../", line)
        out.append(line)
    return out


def declare(lines, module, public):
    """`mod module;` after the last `mod x;`, `use module::*;` after the last
    glob — and a `pub use` for each item that was public beyond this file,
    which the private glob would otherwise hide from the rest of the crate."""
    mods = [i for i, l in enumerate(lines) if re.match(r"^mod \w+;$", l)]
    globs = [i for i, l in enumerate(lines) if re.match(r"^use \w+::\*;$", l)]
    if f"mod {module};" in lines:
        sys.exit(f"mod {module} is already declared")
    for visibility, name in reversed(public):
        lines.insert(globs[-1] + 1, f"{visibility} use {module}::{name};")
    lines.insert(globs[-1] + 1, f"use {module}::*;")
    lines.insert(mods[-1] + 1, f"mod {module};")


def main(args):
    lines = WINDOW.read_text().split("\n")
    found = items(lines)

    if args[0] == "--test":
        name = args[1]
        mods = [(s, e) for n, s, e in found if n == name and any(l.startswith(f"mod {name} {{") for l in lines[s:e + 1])]
        if len(mods) != 1:
            sys.exit(f"{name}: {len(mods)} test modules of that name")
        s, e = mods[0]
        head = next(i for i in range(s, e + 1) if lines[i].startswith(f"mod {name} {{"))
        body = [l[4:] if l.startswith("    ") else l for l in lines[head + 1:e]]
        target = CHILDREN / f"{name}.rs"
        if target.exists():
            sys.exit(f"{target} exists")
        target.write_text("\n".join(body).rstrip("\n") + "\n")
        lines[head:e + 1] = [f"mod {name};"]
        WINDOW.write_text("\n".join(lines))
        print(f"{name}: {e - head - 1} lines to {target}")
        return

    module, first = args[0], args[1]
    last = args[2] if len(args) > 2 else first
    starts = [s for n, s, e in found if n == first]
    ends = [e for n, s, e in found if n == last]
    if not starts or not ends:
        sys.exit(f"not a top-level item: {first if not starts else last}")
    start, end = starts[0], ends[-1]
    if end < start:
        sys.exit(f"{last} comes before {first}")
    block = lines[start:end + 1]
    if not balanced(block):
        sys.exit(f"{first}..{last}: braces do not balance, not moving it")

    target = CHILDREN / f"{module}.rs"
    if target.exists():
        sys.exit(f"{target} exists")
    target.write_text("use super::*;\n\n" + "\n".join(expose(block)).rstrip("\n") + "\n")

    public = [(m.group(1), name_of(line)) for line in block
              for m in [re.match(r"^(pub|pub\(crate\))\s", line)] if m and ITEM.match(line) and name_of(line)
              and not line.startswith("impl")]

    # The block goes, and the blank line it leaves behind with it.
    del lines[start:end + 1]
    while start < len(lines) and start > 0 and lines[start] == "" and lines[start - 1] == "":
        del lines[start]
    declare(lines, module, public)
    WINDOW.write_text("\n".join(lines))
    print(f"{module}: {end - start + 1} lines, {first}..{last}")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1:])
