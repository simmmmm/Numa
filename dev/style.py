#!/usr/bin/env python3
"""The style ratchet: what the design system replaces may only become rarer.

    dev/style.py            check against dev/style-baseline.txt; exit 1 on new debt
    dev/style.py --baseline rewrite the baseline from the tree (only allowed to shrink;
                            pass --force to accept a larger one, and say why in the commit)
    dev/style.py --report   the counts per check and the worst files, no verdict
    dev/style.py --root DIR measure another tree (a checkout of main, say)

Modelled on dev/shape.py. What it counts, per file under src/ui (tests.rs
left out):

    colour     a hex or rgb()/rgba() colour, a cairo set_source_rgb(a) or a
               gdk::RGBA::new with numbers in it — anywhere but tokens.css,
               which is where the design system's colours live
    suggested  `suggested-action` in Rust: Numa's primary is white
               (`primary_button`); blue stays on the platform's own surfaces,
               which set it themselves (AdwAlertDialog's responses)
    scale      a gtk::Scale made outside slider.rs, the slider's home
    size       a CSS font-size, border-radius, padding or margin off the scale:
               spacing 0 4 8 12 16 24 32, radius 3 6 8 14 22 999 (and 50 %),
               font 10 12 13 15 20 26 px — an em or rem is off the scale too
    sans       cairo text set in "Sans" rather than the system face
    busy       a spinner or progress bar made outside busy.rs, whose three
               forms are the only ones: the loader toast, the progress toast
               with Stop, and Waiting in place (UX-015, UX-025)

A count per file and check is a debt. The check fails when a file has a
debt the baseline does not name, or more of one than the baseline says.
"""
import pathlib, re, sys

BASELINE = pathlib.Path(__file__).with_name("style-baseline.txt")
CHECKS = ("colour", "suggested", "scale", "size", "sans", "busy")

SPACING = {0, 4, 8, 12, 16, 24, 32}
RADIUS = {0, 3, 6, 8, 14, 22, 999}
FONT = {10, 12, 13, 15, 20, 26}

hex_colour = re.compile(r'(?<![\w&])#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3}(?:[0-9a-fA-F]{2})?)?\b')
rgb_colour = re.compile(r'\brgba?\(')
cairo_colour = re.compile(r'\b(?:set_source_rgba?|RGBA::new)\(\s*-?[0-9.]')
css_size = re.compile(r'^\s*(font-size|border-radius|padding(?:-\w+)?|margin(?:-\w+)?)\s*:\s*([^;]+);', re.M)
busy_widget = re.compile(r'\b(?:gtk|adw)::(?:Spinner|ProgressBar)::(?:new|builder)\b')
number = re.compile(r'(-?[0-9.]+)(px|em|rem|%)?')


def off_scale(prop, value):
    """How many of the values in one declaration are off their scale."""
    allowed = FONT if prop == "font-size" else RADIUS if prop == "border-radius" else SPACING
    off = 0
    for amount, unit in number.findall(value):
        if unit == "%" and prop == "border-radius" and float(amount) == 50:
            continue
        if unit in ("em", "rem", "%") or float(amount) not in allowed:
            off += 1
    return off


def strip_comments(text, css):
    if css:
        return re.sub(r'/\*.*?\*/', lambda m: "\n" * m.group().count("\n"), text, flags=re.S)
    return "\n".join(line.split("//")[0] if not line.lstrip().startswith("///") else "" for line in text.splitlines())


def measure(root):
    debts = {check: {} for check in CHECKS}
    ui = root / "src/ui"
    for f in sorted(ui.rglob("*")):
        if f.suffix not in (".rs", ".css") or f.name in ("tokens.css", "tests.rs"):
            continue
        css = f.suffix == ".css"
        text = strip_comments(f.read_text(errors="replace"), css)
        key = str(f.relative_to(root))
        found = {
            "colour": len(hex_colour.findall(text)) + len(rgb_colour.findall(text)) + (0 if css else len(cairo_colour.findall(text))),
            "suggested": 0 if css else text.count("suggested-action"),
            "scale": 0 if css or f.name == "slider.rs" else text.count("gtk::Scale::"),
            "size": sum(off_scale(prop, value) for prop, value in css_size.findall(text)) if css else 0,
            "sans": 0 if css else text.count('"Sans"'),
            "busy": 0 if css or f.name == "busy.rs" else len(busy_widget.findall(text)),
        }
        for check, count in found.items():
            if count:
                debts[check][key] = count
    return debts


def read_baseline():
    base = {check: {} for check in CHECKS}
    if BASELINE.exists():
        for line in BASELINE.read_text().splitlines():
            if not line.strip() or line.startswith("#"):
                continue
            check, count, key = line.split("\t", 2)
            base[check][key] = int(count)
    return base


def write_baseline(debts):
    rows = ["# style baseline — what the design system replaces, per file. May only shrink.",
            "# check\tcount\tfile"]
    for check in CHECKS:
        for key, count in sorted(debts[check].items(), key=lambda kv: (-kv[1], kv[0])):
            rows.append(f"{check}\t{count}\t{key}")
    BASELINE.write_text("\n".join(rows) + "\n")


def totals(debts):
    return {check: sum(d.values()) for check, d in debts.items()}


def main():
    args = sys.argv[1:]
    root = pathlib.Path(args[args.index("--root") + 1]) if "--root" in args else pathlib.Path(__file__).resolve().parent.parent
    debts = measure(root)
    if "--report" in args:
        for check in CHECKS:
            d = debts[check]
            print(f"\n{check}: {sum(d.values())} in {len(d)} files")
            for key, count in sorted(d.items(), key=lambda kv: -kv[1])[:8]:
                print(f"  {count:>5}  {key}")
        return 0
    base = read_baseline()
    grown = [(c, k, base[c].get(k, 0), n) for c in CHECKS for k, n in debts[c].items() if n > base[c].get(k, 0)]
    if "--baseline" in args:
        if BASELINE.exists() and grown and "--force" not in args:
            for check, key, was, now in grown:
                print(f"GREW   {check:9} {was:>4} -> {now:<4} {key}")
            print("baseline would grow; refusing without --force")
            return 1
        write_baseline(debts)
        print("baseline written: " + ", ".join(f"{c} {n}" for c, n in totals(debts).items()))
        return 0
    shrunk = [(c, k, n, debts[c].get(k, 0)) for c in CHECKS for k, n in base[c].items() if debts[c].get(k, 0) < n]
    for check, key, was, now in grown:
        print(f"{'NEW' if was == 0 else 'GREW':6} {check:9} {was:>4} -> {now:<4} {key}")
    for check, key, was, now in shrunk:
        print(f"less   {check:9} {was:>4} -> {now:<4} {key}")
    if grown:
        print(f"\n{len(grown)} grown: use the design system's token, part or scale instead (data/design, kit.rs).")
        return 1
    print("\nstyle: " + ", ".join(f"{c} {n}" for c, n in totals(debts).items())
          + (f"; {len(shrunk)} shrank — run --baseline to lock them in" if shrunk else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
