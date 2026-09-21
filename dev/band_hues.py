#!/usr/bin/env python3
"""Where the eight sRGB band hues land in ProPhoto HSV (ADJ-005 / BANDS).
Encoded sRGB (the hue a colour picker shows) -> decoded to linear with the
sRGB curve -> XYZ -> Bradford to D50 -> ProPhoto linear -> HSV hue.
The decode step matters only for bands with a 0.5 component (orange, purple);
skipping it was this script's bug, found by FT-001. Matches
src/core/profile.rs::prophoto_hue_of_srgb.
Run: python3 band_hues.py   Prints the table; compare with BANDS in the source."""
import numpy as np, colorsys
M_srgb = np.array([[0.4124564,0.3575761,0.1804375],[0.2126729,0.7151522,0.0721750],[0.0193339,0.1191920,0.9503041]])
BRADFORD_D65_D50 = np.array([[1.0478112,0.0228866,-0.0501270],[0.0295424,0.9904844,-0.0170491],[-0.0092345,0.0150436,0.7521316]])
M_xyz_to_pp = np.array([[1.3459433,-0.2556075,-0.0511118],[-0.5445989,1.5081673,0.0205351],[0,0,1.2118128]])
M = M_xyz_to_pp @ BRADFORD_D65_D50 @ M_srgb
def srgb_decode(v):
    v = np.asarray(v, float)
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)
def hue(c):
    return colorsys.rgb_to_hsv(*np.clip(c, 0, None))[0] * 360
for name, h in [("Red",0),("Orange",30),("Yellow",60),("Green",120),("Aqua",180),("Blue",240),("Purple",270),("Magenta",300)]:
    linear = srgb_decode(colorsys.hsv_to_rgb(h/360, 1, 1))
    print(f"{name:8s} sRGB {h:3d}  ProPhoto {hue(M @ linear):6.1f}")
