#!/usr/bin/env python3
"""The design system's tokens, written for the two clients that read CSS.

    dev/tokens.py           write src/ui/tokens.css and the web's tokens.css
    dev/tokens.py --check   exit 1 if either is not what it would write

    src/ui/tokens.css                          GTK named colours (Linux)
    crates/numa-server/src/web/tokens.css      Numa Home's custom properties

tokens.json is the design system's own file (claude.ai artifact
AJwfFQrTHrAG3x5NCYHL5Z, project/tokens.json), copied in unchanged. Each colour
token becomes `@define-color numa_<name>` with its dark value; below the
`/* light */` line come the tokens whose light value differs. Those also get a
`numa_photo_<name>` that keeps the dark value in both schemes: what sits on
the photograph is photo-side, and photo-side is dark (the README's Colour). `load_css`
(src/ui/window.rs) reads the dark half always and the light half on top of it
while the style manager says the scheme is light — GTK 4.14 has no media
query for that.

Numa Home is dark only (the README's Web column), so its file is one `:root`
with every token's dark value as `--<token-name>`: the colours, the spacing,
radius, size, shadow and opacity tokens, and the type families as
`--font-<family>`. `app.css` and the scripts' style strings read these and
nothing else (dev/style_web.py counts what does not).
"""
import json, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "data/design/tokens.json"
OUT = ROOT / "src/ui/tokens.css"
WEB = ROOT / "crates/numa-server/src/web/tokens.css"


def render():
    tokens = json.loads(SOURCE.read_text())["color"]["tokens"]
    name = lambda token: "numa_" + token["name"].replace("-", "_")
    dark, light, photo = [], [], []
    for token in tokens:
        value = token["value"]
        if isinstance(value, str):
            dark.append(f"@define-color {name(token)} {value};")
            continue
        dark.append(f"@define-color {name(token)} {value['dark']};")
        if value["light"] != value["dark"]:
            light.append(f"@define-color {name(token)} {value['light']};")
            photo.append(f"@define-color {name(token).replace('numa_', 'numa_photo_', 1)} {value['dark']};")
    head = "/* Written by dev/tokens.py from data/design/tokens.json. Do not edit. */"
    return "\n".join([head, *dark, *photo, "/* light */", *light]) + "\n"


def render_web():
    tokens = json.loads(SOURCE.read_text())
    dark = lambda value: value if isinstance(value, str) else value["dark"]
    rows = []
    for token in tokens["color"]["tokens"]:
        rows.append(f"  --{token['name']}: {dark(token['value'])};")
    for group in ("spacing", "radius", "size", "shadow", "opacity"):
        rows.append("")
        for token in tokens[group]["tokens"]:
            rows.append(f"  --{token['name']}: {dark(token['value'])};")
    rows.append("")
    for family, stack in tokens["type"]["families"].items():
        rows.append(f"  --font-{family}: {stack};")
    head = "/* Written by dev/tokens.py from data/design/tokens.json. Do not edit. */"
    return "\n".join([head, ":root {", *rows, "}"]) + "\n"


if __name__ == "__main__":
    outputs = [(OUT, render(), "src/ui/tokens.css"), (WEB, render_web(), "crates/numa-server/src/web/tokens.css")]
    if "--check" in sys.argv:
        stale = [name for path, text, name in outputs if not path.exists() or path.read_text() != text]
        for name in stale:
            print(f"{name} is out of date: python3 dev/tokens.py", file=sys.stderr)
        sys.exit(1 if stale else 0)
    for path, text, _ in outputs:
        path.write_text(text)
