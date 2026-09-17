#!/usr/bin/env python3
"""Slanted-edge MTF50 (ISO 12233 style), for DETAIL-008.

    python3 mtf50.py export.png x0 y0 x1 y1      # ROI containing one near-vertical edge
    python3 mtf50.py --selftest

Prints MTF50 in cycles/pixel. Compare exports from Numa, RawTherapee,
Lightroom and Capture One on the *same* ROI of the *same* frame at full size.
Works on 8-bit or 16-bit PNG/TIFF; luma is taken in linear light (sRGB
decoded), because contrast measured on encoded values is not contrast.
Requires numpy and Pillow only.
"""
import sys, numpy as np
from PIL import Image

def srgb_to_linear(v):
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)

def load_luma(path):
    im = Image.open(path)
    arr = np.asarray(im).astype(np.float64)
    if arr.ndim == 3:
        arr = arr[..., :3]
    maxv = 65535.0 if arr.max() > 255 else 255.0
    arr = srgb_to_linear(arr / maxv)
    if arr.ndim == 3:
        arr = 0.2126 * arr[..., 0] + 0.7152 * arr[..., 1] + 0.0722 * arr[..., 2]
    return arr

def mtf50(roi, oversample=4):
    """roi: 2-D array with one near-vertical edge, dark left / bright right or reverse."""
    roi = roi - roi.min()
    roi = roi / (roi.max() + 1e-12)
    h, w = roi.shape
    # 1. edge position per row: centroid of the derivative
    d = np.abs(np.diff(roi, axis=1))
    d = np.where(d > 0.2 * d.max(axis=1, keepdims=True), d, 0.0)   # keep the edge, drop the noise
    x = np.arange(w - 1) + 0.5
    pos = (d * x).sum(axis=1) / (d.sum(axis=1) + 1e-12)
    # 2. straight line through the edge positions (the edge is assumed straight)
    rows = np.arange(h)
    slope, icpt = np.polyfit(rows, pos, 1)
    # 3. project every pixel onto its distance from the edge -> oversampled ESF
    dist = (np.arange(w)[None, :] - (slope * rows[:, None] + icpt))
    bins = np.round(dist * oversample).astype(int)
    lo, hi = bins.min(), bins.max()
    esf = np.zeros(hi - lo + 1); cnt = np.zeros_like(esf)
    np.add.at(esf, (bins - lo).ravel(), roi.ravel()); np.add.at(cnt, (bins - lo).ravel(), 1)
    ok = cnt > 0
    esf = np.interp(np.arange(len(esf)), np.flatnonzero(ok), esf[ok] / cnt[ok])
    # 4. LSF, Hann window, |FFT| -> MTF, normalised at DC
    lsf = np.gradient(esf)
    lsf *= np.hanning(len(lsf))
    mtf = np.abs(np.fft.rfft(lsf)); mtf /= mtf[0]
    f = np.fft.rfftfreq(len(lsf), d=1.0 / oversample)   # cycles per original pixel
    # 5. first crossing of 0.5
    i = np.argmax(mtf < 0.5)
    f50 = np.interp(0.5, [mtf[i], mtf[i - 1]], [f[i], f[i - 1]])
    return f50, f, mtf

def selftest():
    # A slanted edge blurred by a Gaussian of known sigma has MTF50 = sqrt(ln2 / (2 pi^2 sigma^2)).
    from math import log, pi, sqrt
    rng = np.random.default_rng(0)
    for sigma in (0.7, 1.0, 1.5, 2.5):
        h, w = 200, 120
        yy, xx = np.mgrid[0:h, 0:w]
        edge = xx - (w / 2 + 0.08 * (yy - h / 2))      # ~4.6 degrees of slant
        from scipy.special import erf
        roi = 0.5 * (1 + erf(edge / (sigma * sqrt(2)))) * 0.8 + 0.1
        roi += rng.normal(0, 0.005, roi.shape)
        got, _, _ = mtf50(roi)
        want = sqrt(log(2) / (2 * pi ** 2 * sigma ** 2))
        print(f"sigma {sigma:>4}: expected {want:.4f}  measured {got:.4f}  error {100*(got/want-1):+.1f} %")

if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest(); sys.exit()
    path, x0, y0, x1, y1 = sys.argv[1], *map(int, sys.argv[2:6])
    luma = load_luma(path)
    f50, _, _ = mtf50(luma[y0:y1, x0:x1])
    print(f"MTF50 = {f50:.4f} cycles/pixel   ({f50*2:.3f} of Nyquist)")
