#!/usr/bin/env python3
"""MASK-003: contact sheets and counts for what `tests/group_survey.rs` wrote.

    python3 dev/group_sheets.py OUT [SHEETS]

One sheet per photograph: the photograph, then every group the panel would
offer, each with the photograph dimmed outside the mask and a thin yellow line
where it crosses one half. And per group, the numbers the failure modes are
counted with:

  specks  pieces of the mask under 0.05 % of the frame, apart from its body
  holes   pieces of not-mask under 0.5 % of the frame, enclosed by it
  soft    how much of the frame is neither in nor out (0.1..0.9)
  edge    how much steeper the photograph is under the mask's border than on
          average: a border that follows the photograph's edges reads high,
          one that cuts across flat sky or a wall reads near 1
"""
import collections
import os
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

PANEL = 600
# Components are counted at this width: a speck is still a pixel here, and
# pure-Python labelling stays under a second a sheet.
COUNT = 400


def components(binary):
    """4-connected pieces of a boolean grid: (sizes, touches-the-border)."""
    h, w = binary.shape
    label = np.full((h, w), -1, np.int32)
    sizes, border = [], []
    flat = binary.ravel()
    lab = label.ravel()
    for start in np.flatnonzero(flat):
        if lab[start] >= 0:
            continue
        n = len(sizes)
        lab[start] = n
        stack, size, edge = [start], 0, False
        while stack:
            i = stack.pop()
            size += 1
            y, x = divmod(i, w)
            if x == 0 or y == 0 or x == w - 1 or y == h - 1:
                edge = True
            for j in (i - 1 if x > 0 else -1, i + 1 if x < w - 1 else -1,
                      i - w if y > 0 else -1, i + w if y < h - 1 else -1):
                if j >= 0 and flat[j] and lab[j] < 0:
                    lab[j] = n
                    stack.append(j)
        sizes.append(size)
        border.append(edge)
    return sizes, border


def measure(alpha, luma):
    small = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).resize(
        (COUNT, max(1, COUNT * alpha.shape[0] // alpha.shape[1])), Image.BILINEAR)) / 255.0
    cells = small.size
    inside = small >= 0.5
    sizes, _ = components(inside)
    body = max(sizes) if sizes else 0
    specks = sum(1 for s in sizes if s < cells * 0.0005 and s != body)
    hole_sizes, hole_border = components(~inside)
    holes = sum(1 for s, b in zip(hole_sizes, hole_border) if not b and s < cells * 0.005)

    gy, gx = np.gradient(luma)
    grad = np.hypot(gx, gy)
    hard = alpha >= 0.5
    line = np.zeros_like(hard)
    line[:, 1:] |= hard[:, 1:] != hard[:, :-1]
    line[1:, :] |= hard[1:, :] != hard[:-1, :]
    # A pixel either side, because the photograph's step and the mask's are
    # not on the same pixel after two resamplings.
    near = line.copy()
    near[:, 1:] |= line[:, :-1]
    near[:, :-1] |= line[:, 1:]
    near[1:, :] |= line[:-1, :]
    near[:-1, :] |= line[1:, :]
    edge = (grad * near).sum() / max(near.sum(), 1) / max(grad.mean(), 1e-6) if line.any() else 0.0
    return {
        "share": alpha.mean(),
        "specks": specks,
        "holes": holes,
        "soft": ((alpha > 0.1) & (alpha < 0.9)).mean(),
        "edge": edge,
    }


def panel(photo, alpha, text, font):
    w = PANEL
    h = PANEL * photo.height // photo.width
    rgb = np.asarray(photo.resize((w, h), Image.LANCZOS)).astype(np.float32)
    a = np.asarray(Image.fromarray((alpha * 255).astype(np.uint8)).resize((w, h), Image.BILINEAR)) / 255.0
    out = rgb * (0.22 + 0.78 * a[..., None])
    hard = a >= 0.5
    line = np.zeros_like(hard)
    line[:, 1:] |= hard[:, 1:] != hard[:, :-1]
    line[1:, :] |= hard[1:, :] != hard[:-1, :]
    out[line] = (255, 225, 0)
    image = Image.fromarray(out.clip(0, 255).astype(np.uint8))
    draw = ImageDraw.Draw(image)
    draw.rectangle((0, 0, draw.textlength(text, font=font) + 10, 22), fill=(0, 0, 0))
    draw.text((5, 3), text, fill=(255, 255, 255), font=font)
    return image


def main():
    out = sys.argv[1]
    sheets = sys.argv[2] if len(sys.argv) > 2 else os.path.join(out, "sheets")
    os.makedirs(sheets, exist_ok=True)
    try:
        font = ImageFont.truetype("DejaVuSans.ttf", 15)
    except OSError:
        font = ImageFont.load_default()

    rows = collections.defaultdict(list)
    seen = {}
    with open(os.path.join(out, "groups.tsv")) as table:
        next(table)
        for line in table:
            photo, group, share, classes, what = line.rstrip("\n").split("\t")
            rows[photo].append((group, classes))
            seen[photo] = what

    print("photo\tgroup\tshare\tspecks\tholes\tsoft\tedge")
    for stem, groups in rows.items():
        photo = Image.open(os.path.join(out, stem, "photo.png")).convert("RGB")
        luma = np.asarray(photo.convert("L")).astype(np.float32) / 255.0
        panels = [panel(photo, np.ones(luma.shape), stem, font)]
        for group, _ in groups:
            alpha = np.asarray(Image.open(os.path.join(out, stem, group + ".png"))).astype(np.float32) / 255.0
            m = measure(alpha, luma)
            print(f"{stem}\t{group}\t{m['share']:.4f}\t{m['specks']}\t{m['holes']}\t{m['soft']:.4f}\t{m['edge']:.2f}")
            panels.append(panel(photo, alpha, f"{group} {m['share'] * 100:.1f}%", font))
        columns = 4
        h = panels[0].height
        sheet = Image.new("RGB", (PANEL * columns, h * ((len(panels) + columns - 1) // columns) + 24), (20, 20, 20))
        for i, p in enumerate(panels):
            sheet.paste(p, ((i % columns) * PANEL, (i // columns) * h))
        ImageDraw.Draw(sheet).text((5, sheet.height - 20), seen[stem], fill=(200, 200, 200), font=font)
        sheet.save(os.path.join(sheets, stem + ".jpg"), quality=88)


if __name__ == "__main__":
    main()
