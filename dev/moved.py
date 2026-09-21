#!/usr/bin/env python3
"""SPLIT_PLAN A0: prove a change only moves code.

    dev/moved.py              the staged change (run `git add -A` first)
    dev/moved.py <rev>        that commit
    dev/moved.py --self-test

Every line removed must be added somewhere and every line added must have been
removed, compared without leading or trailing space. The only lines allowed on
one side alone are the boilerplate of a move: `use super::*;`, `mod x;` with
its `use x::*;`, a `pub use x::name;` for what was public, and — when a test module moves to a file of its own — the
`mod x {` it loses and one `}` for each. A `pub(super) ` anywhere in an added
line is not counted as a difference. Exit 1, and the lines, if anything else
differs.
"""
import collections, re, subprocess, sys


def lines_of(diff):
    removed, added = [], []
    for line in diff.splitlines():
        if line.startswith("---") or line.startswith("+++"):
            continue
        if line.startswith("-"):
            removed.append(line[1:].strip())
        elif line.startswith("+"):
            added.append(line[1:].strip())
    return removed, added


def check(removed, added):
    """The lines that make this more than a move; empty when it is one."""
    # A file included by relative path is further away from a file that moved
    # down a directory; how far is not a difference.
    near = lambda line: re.sub(r'(include_(?:str|bytes)!\(")(\.\./)+', r"\1", line.strip())
    removed = [near(line) for line in removed]
    added = [near(line).replace("pub(super) ", "") for line in added]
    only_removed = collections.Counter(removed) - collections.Counter(added)
    only_added = collections.Counter(added) - collections.Counter(removed)

    boilerplate = re.compile(r"^(use super::\*;|mod \w+;|use \w+::\*;|pub(\(crate\))? use \w+::\w+;|)$")
    wrappers = sum(count for line, count in only_removed.items() if re.match(r"^mod \w+ \{$", line))
    problems = []
    for line, count in only_removed.items():
        if re.match(r"^mod \w+ \{$", line) or boilerplate.match(line):
            continue
        if line == "}":
            count -= wrappers
        if count > 0:
            problems.append(f"- {line}" + (f"  (x{count})" if count > 1 else ""))
    for line, count in only_added.items():
        if boilerplate.match(line):
            continue
        problems.append(f"+ {line}" + (f"  (x{count})" if count > 1 else ""))
    return problems


def self_test():
    moved = ["fn a() {", "    1", "}"]
    assert check(moved, ["use super::*;", "pub(super) fn a() {", "1", "}", "mod x;", "use x::*;"]) == []
    assert check(moved, ["pub(super) fn a() {", "2", "}"]), "a changed line passed"
    assert check(moved, ["pub(super) fn a() {", "}"]), "a dropped line passed"
    assert check(moved[:2], moved), "an added line passed"
    assert check(["mod t {", "fn b() {}", "}"], ["mod t;", "fn b() {}"]) == []
    assert check(["fn c() {}", "}"], ["fn c() {}"]), "a stray brace passed"
    assert check(["struct C(u8);"], ["pub(super) struct C(pub(super) u8);"]) == []
    print("moved.py: self-test passed")


def main(args):
    if args == ["--self-test"]:
        return self_test()
    command = ["git", "show", "--format=", "--unified=0", args[0]] if args else ["git", "diff", "--cached", "--unified=0"]
    diff = subprocess.run(command, capture_output=True, text=True, check=True).stdout
    removed, added = lines_of(diff)
    problems = check(removed, added)
    if problems:
        print("not a pure move:")
        print("\n".join(problems[:40]))
        sys.exit(1)
    print(f"a pure move: {len(removed)} lines out, {len(added)} in")


if __name__ == "__main__":
    main(sys.argv[1:])
