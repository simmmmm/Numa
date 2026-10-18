#!/usr/bin/env python3
"""The web style ratchet: what the design system's tokens replace in Numa
Home's pages may only become rarer.

    dev/style_web.py            check against dev/style-web-baseline.txt; exit 1 on new debt
    dev/style_web.py --baseline rewrite the baseline from the tree (only allowed to shrink;
                                pass --force to accept a larger one, and say why in the commit)
    dev/style_web.py --report   the counts per check and the worst files, no verdict
    dev/style_web.py --root DIR measure another tree (a checkout of main, say)

The twin of dev/style.py for crates/numa-server/src/web: app.css, and the
style strings in the scripts (`style: { … }` objects, `.style.x = …`,
`style.setProperty(…)`, `cssText`, `style="…"`). tokens.css, which
dev/tokens.py writes from data/design/tokens.json, is where the literals
belong and is not counted. What it counts, per file:

    colour    a hex colour, an rgb(), rgba(), hsl() or hsla(), or a hex colour
              written %23… inside a data URI: use a token (`var(--accent)`), or
              `color-mix(in srgb, var(--ink) 40%, transparent)` for a tint of one
    size      a px length off the scale in font-size, padding, margin or gap:
              spacing 0 2 4 6 8 10 12 14 16 18 20 30 44 46 (the tokens), font
              10 11 12 12.5 13 15 17 20 26 32 (the type styles)
    radius    a px border-radius off the tokens' 1 4 6 7 8 10 14 16 18 22
              (0, 50 % and 999px, the capsule, pass)
    duration  a literal time in a transition or an animation off the motion
              times: 0, 120, 150, 160, 200, 300, 700 and 900 ms, 1.4 s; use
              var(--quick), var(--standard), var(--screen)

A count per file and check is a debt. The check fails when a file has a debt
the baseline does not name, or more of one than the baseline says.
"""
import json, pathlib, re, sys

BASELINE = pathlib.Path(__file__).with_name("style-web-baseline.txt")
CHECKS = ("colour", "size", "radius", "duration")
WEB = "crates/numa-server/src/web"

RADIUS = {0, 999}
DURATION_MS = {0, 120, 150, 160, 200, 300, 700, 900, 1400}


def scales(root):
    """The spacing, radius and font scales, from the design system's own file."""
    tokens = json.loads((root / "data/design/tokens.json").read_text())
    px = lambda value: float(str(value).removesuffix("px"))
    spacing = {0.0} | {px(t["value"]) for t in tokens["spacing"]["tokens"]}
    radius = {float(r) for r in RADIUS} | {px(t["value"]) for t in tokens["radius"]["tokens"]}
    font = {px(s["fontSize"]) for g in tokens["type"]["groups"] for s in g["styles"] if s["fontSize"].endswith("px") and g["name"] == "Interface"}
    return spacing, radius, font


hex_colour = re.compile(r'(?<![\w&])#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3}(?:[0-9a-fA-F]{2})?)?\b|%23[0-9a-fA-F]{3,8}\b')
func_colour = re.compile(r'\b(?:rgba?|hsla?)\(')
px_length = re.compile(r'(?<![\w.-])(-?\d+(?:\.\d+)?)px\b')
time_value = re.compile(r'(?<![\w.-])(\d+(?:\.\d+)?)(ms|s)\b')

SIZE_PROP = re.compile(r'^(font-size|padding(?:-\w+)*|margin(?:-\w+)*|gap|row-gap|column-gap)$')
TIME_PROP = re.compile(r'^(transition(?:-duration|-delay)?|animation(?:-duration|-delay)?)$')


def strip_css_comments(text):
    return re.sub(r'/\*.*?\*/', lambda m: "\n" * m.group().count("\n"), text, flags=re.S)


def declarations(css):
    """Every `property: value` of a stylesheet, whatever its nesting, with url(…) bodies blanked."""
    css = re.sub(r'url\((?:"[^"]*"|\'[^\']*\'|[^)]*)\)', 'url()', css)
    found = []
    previous = None
    while previous != css:
        previous = css
        for body in re.findall(r'\{([^{}]*)\}', css):
            for part in body.split(';'):
                prop, colon, value = part.partition(':')
                if colon and re.fullmatch(r'-{0,2}[a-z][\w-]*', prop.strip()):
                    found.append((prop.strip(), value.strip()))
        css = re.sub(r'\{[^{}]*\}', ' ', css)
    return found


def count_values(decls, spacing, radius, font):
    size = rad = dur = 0
    for prop, value in decls:
        if prop.startswith('--'):
            continue
        if SIZE_PROP.match(prop):
            allowed = font if prop == 'font-size' else spacing
            size += sum(1 for amount in px_length.findall(value) if float(amount) not in allowed)
        elif prop == 'border-radius' or re.fullmatch(r'border(?:-\w+)*-radius', prop):
            rad += sum(1 for amount in px_length.findall(value) if float(amount) not in radius)
        elif TIME_PROP.match(prop):
            for amount, unit in time_value.findall(value):
                if float(amount) * (1 if unit == 'ms' else 1000) not in DURATION_MS:
                    dur += 1
    return size, rad, dur


# camelCase to kebab, so a style string reads as the property it sets.
kebab = lambda name: re.sub(r'(?<=[a-z])(?=[A-Z])', '-', name).lower()


def style_strings(js):
    """The declarations written as strings in a script: [(property, value)], and the raw text for colours."""
    decls, raw = [], []
    for body in re.findall(r'\bstyle:\s*\{([^{}]*)\}', js):
        raw.append(body)
        for key, value in re.findall(r'(\w+)\s*:\s*([^,]+)', body):
            decls.append((kebab(key), value))
    for key, value in re.findall(r'\.style\.(\w+)\s*=\s*([^;\n]+)', js):
        if key == 'cssText':
            continue
        raw.append(value)
        decls.append((kebab(key), value))
    for key, value in re.findall(r"style\.setProperty\(\s*'([^']+)'\s*,\s*([^)]+)\)", js):
        raw.append(value)
        decls.append((key, value))
    texts = [text for _, text in re.findall(r"cssText\s*=\s*(['\"`])(.*?)\1", js)] + re.findall(r'style="([^"]*)"', js)
    for text in texts:
        raw.append(text)
        decls += [(prop.strip(), value) for prop, _, value in (part.partition(':') for part in text.split(';')) if value]
    return decls, "\n".join(raw)


def measure(root):
    debts = {check: {} for check in CHECKS}
    spacing, radius, font = scales(root)
    web = root / WEB
    for f in sorted(web.iterdir()):
        if f.suffix not in (".css", ".js") or f.name == "tokens.css":
            continue
        text = f.read_text(errors="replace")
        if f.suffix == ".css":
            text = strip_css_comments(text)
            decls = declarations(text)
            colour_text = text
        else:
            decls, colour_text = style_strings(text)
        size, rad, dur = count_values(decls, spacing, radius, font)
        found = {
            "colour": len(hex_colour.findall(colour_text)) + len(func_colour.findall(colour_text)),
            "size": size, "radius": rad, "duration": dur,
        }
        key = str(f.relative_to(root))
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
    rows = ["# web style baseline — what the design system's tokens replace in Numa Home's pages, per file. May only shrink.",
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
        print(f"\n{len(grown)} grown: use the design system's token or scale instead (data/design/tokens.json, tokens.css).")
        return 1
    print("\nweb style: " + ", ".join(f"{c} {n}" for c, n in totals(debts).items())
          + (f"; {len(shrunk)} shrank — run --baseline to lock them in" if shrunk else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
