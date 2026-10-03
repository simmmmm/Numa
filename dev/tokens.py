#!/usr/bin/env python3
"""The design system's colours as GTK named colours.

    dev/tokens.py           write src/ui/tokens.css from data/design/tokens.json
    dev/tokens.py --check   exit 1 if src/ui/tokens.css is not what it would write

tokens.json is the design system's own file (claude.ai artifact
AJwfFQrTHrAG3x5NCYHL5Z, project/tokens.json), copied in unchanged. Each colour
token becomes `@define-color numa_<name>` with its dark value; below the
`/* light */` line come the tokens whose light value differs. Those also get a
`numa_photo_<name>` that keeps the dark value in both schemes: what sits on
the photograph is photo-side, and photo-side is dark (the README's Colour). `load_css`
(src/ui/window.rs) reads the dark half always and the light half on top of it
while the style manager says the scheme is light — GTK 4.14 has no media
query for that.
"""
import json, pathlib, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "data/design/tokens.json"
OUT = ROOT / "src/ui/tokens.css"


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


if __name__ == "__main__":
    text = render()
    if "--check" in sys.argv:
        if OUT.read_text() != text:
            print("src/ui/tokens.css is out of date: python3 dev/tokens.py", file=sys.stderr)
            sys.exit(1)
    else:
        OUT.write_text(text)
