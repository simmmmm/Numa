#!/usr/bin/env python3
"""SPLIT_PLAN B1: move a group of `App`'s fields into a `State` of their own.

    dev/regroup.py <module> [--as <name>] <field>[=<new name>] ...

The fields leave `struct App` and its literal in `build_window`, doc comments
and initialisers word for word, and become `<module>::State` — a struct at the
end of `src/ui/window/<module>.rs` — held by `App` as `<name>` (the module's
own name unless `--as` says otherwise). Every `state.<field>` under
`src/ui/window*` becomes `state.<name>.<new name>`, in one pass, so a field
that is renamed to what another one was called is not renamed twice.

Nothing is checked here that the compiler checks better: a use that was not
rewritten, or one rewritten that was not `App`'s, is a field that does not
exist, and the build says where.
"""
import pathlib, re, sys

WINDOW = pathlib.Path("src/ui/window.rs")
CHILDREN = pathlib.Path("src/ui/window")


def block(lines, start, closer):
    """The index of the line that closes the block opened at `start`."""
    for i in range(start + 1, len(lines)):
        if lines[i] == closer:
            return i
    sys.exit(f"no {closer!r} after line {start + 1}")


def entries(lines, first, last, indent):
    """The entries between `first` and `last` (exclusive) at `indent`: each is
    (name, start, end) with its comments counted in, inclusive."""
    head = re.compile(r"^" + " " * indent + r"([a-z_][a-z0-9_]*)(:|,)")
    starts = [i for i in range(first, last) if head.match(lines[i])]
    out = []
    for n, i in enumerate(starts):
        start = i
        while start > first and re.match(r"^\s*(//|#\[)", lines[start - 1]):
            start -= 1
        end = (starts[n + 1] if n + 1 < len(starts) else last) - 1
        # The next entry's comments are the next entry's.
        while end > i and (re.match(r"^\s*(//|#\[)", lines[end]) or lines[end].strip() == ""):
            end -= 1
        out.append((head.match(lines[i]).group(1), start, end))
    return out


def main(args):
    module, name = args[0], args[0]
    args = args[1:]
    if args[:1] == ["--as"]:
        name, args = args[1], args[2:]
    renames = dict((a.split("=") + [a])[:2] for a in args)

    lines = WINDOW.read_text().split("\n")
    s_open = lines.index("struct App {")
    s_close = block(lines, s_open, "}")
    l_open = lines.index("    let state = App {")
    l_close = block(lines, l_open, "    };")

    fields = {n: (a, b) for n, a, b in entries(lines, s_open + 1, s_close, 4)}
    values = {n: (a, b) for n, a, b in entries(lines, l_open + 1, l_close, 8)}
    for field in renames:
        if field not in fields or field not in values:
            sys.exit(f"App has no field {field} (or no initialiser for it)")
    if name in fields and name not in renames:
        sys.exit(f"App already has a field called {name}")
    target = CHILDREN / f"{module}.rs"
    if re.search(r"^pub\(super\) struct State\b", target.read_text(), re.M):
        sys.exit(f"{target} already has a State")

    # The new struct, from the fields' own lines.
    body = []
    for field, new in renames.items():
        a, b = fields[field]
        chunk = lines[a:b + 1]
        chunk = [re.sub(r"^    " + field + r":", f"    pub(super) {new}:", l) for l in chunk]
        body += chunk
    literal = []
    for field, new in renames.items():
        a, b = values[field]
        chunk = ["    " + l if l else l for l in lines[a:b + 1]]
        head = re.compile(r"^            " + field + r"(:|,)")
        chunk = [head.sub(lambda m: f"            {new}{':' if m.group(1) == ':' else ': ' + field + ','}", l)
                 if head.match(l) else l for l in chunk]
        literal += chunk

    # Out of the literal first (it is further down), then out of the struct,
    # each replaced where its first field was.
    def replace(ranges, insert):
        first = min(a for a, _ in ranges)
        for a, b in sorted(ranges, reverse=True):
            del lines[a:b + 1]
        lines[first:first] = insert

    replace([values[f] for f in renames],
            [f"        {name}: {module}::State {{"] + literal + ["        },"])
    replace([fields[f] for f in renames],
            [f"    /// SPLIT_PLAN B1: see `{module}::State`.", f"    {name}: {module}::State,"])
    WINDOW.write_text("\n".join(lines))

    text = target.read_text().rstrip("\n")
    text += (
        f"\n\n/// SPLIT_PLAN B1: this module's part of `App`, moved out of it whole —\n"
        f"/// each field with what it said about itself — and held there as `{name}`.\n"
        f"#[derive(Clone)]\npub(super) struct State {{\n" + "\n".join(body) + "\n}\n"
    )
    target.write_text(text)

    # Every use, in one pass.
    # `state` and its field may be a line apart, the way rustfmt breaks a chain.
    pattern = re.compile(r"\bstate(\s*)\.(" + "|".join(map(re.escape, renames)) + r")\b(?!\s*\()")
    touched = 0
    for path in [WINDOW, *sorted(CHILDREN.glob("*.rs"))]:
        before = path.read_text()
        after = pattern.sub(lambda m: f"state{m.group(1)}.{name}.{renames[m.group(2)]}", before)
        if after != before:
            path.write_text(after)
            touched += 1
    print(f"{module}::State: {len(renames)} fields as state.{name}, uses rewritten in {touched} files")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1:])
