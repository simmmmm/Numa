#!/usr/bin/env python3
"""Fit BASELINE_EV and CURVE_CONTRAST for one camera body (RENDER-004 / brief A4).

Input: a CSV with one row per frame, columns
    frame, p1_lin, p5_lin, p25_lin, p50_lin, p75_lin, p95_lin, p99_lin,
           p1_jpg, p5_jpg, p25_jpg, p50_jpg, p75_jpg, p95_jpg, p99_jpg
where *_lin are luma percentiles of the scene-linear render (after the colour
stage, before any tone curve, NOT baseline-exposed) and *_jpg the same
percentiles of the camera's embedded JPEG, 0..255. tests/colour_ab.rs already
computes both; have it dump them.

    python3 fit_tone_curve.py frames.csv
    python3 fit_tone_curve.py --selftest

THE ONE THING TO CHECK: curve() below is a guess at core::tone. Port the real
function here before trusting a fit — a fit to the wrong curve gives confident
wrong constants (see "Two silent failures"). The fitter and the residual report
do not care what the curve is.
"""
import sys, numpy as np
from scipy.optimize import minimize

MID_GREY = 0.18

def srgb_encode(v):
    v = np.clip(v, 0, 1)
    return np.where(v <= 0.0031308, 12.92 * v, 1.055 * v ** (1 / 2.4) - 0.055)

def curve(lin, baseline_ev, contrast):
    """Port of core::tone::curve (src/core/tone.rs), with BASELINE_EV applied as
    the multiply io::raw does at decode. The sigmoid's output is already the
    display code value: no sRGB encode goes on top of it."""
    grey_display = 0.46137  # tone.rs GREY_DISPLAY, sRGB's encoding of 0.18
    offset = -np.log(1 / grey_display - 1)
    stops = np.log2(np.maximum(lin * 2 ** baseline_ev, 1e-6) / MID_GREY)
    return np.clip(1 / (1 + np.exp(-(contrast * stops + offset))), 0, 1) * 255

def residual(params, lin, jpg):
    b, c = params
    pred = curve(lin, b, c)
    return np.mean((pred - jpg) ** 2)

def fit(lin, jpg, start=(1.0, 1.0)):
    r = minimize(residual, start, args=(lin, jpg), method="Nelder-Mead",
                 options=dict(xatol=1e-4, fatol=1e-4))
    return r.x, np.sqrt(r.fun)

def report(lin, jpg, params, names):
    pred = curve(lin, *params)
    print(f"BASELINE_EV = {params[0]:.3f}   CURVE_CONTRAST = {params[1]:.3f}")
    print(f"RMS error over all percentiles: {np.sqrt(np.mean((pred-jpg)**2)):.2f} / 255")
    print("per percentile (mean error, camera minus fit):")
    for j, n in enumerate(names):
        print(f"  {n:>4}: {np.mean(jpg[:, j] - pred[:, j]):+6.2f}")

def selftest():
    rng = np.random.default_rng(1)
    true = (1.241, 0.943)
    lin = np.sort(10 ** rng.uniform(-3, 0.3, (8, 7)), axis=1)
    jpg = curve(lin, *true) + rng.normal(0, 1.5, lin.shape)
    est, rms = fit(lin, jpg)
    print(f"true {true}  ->  fitted ({est[0]:.3f}, {est[1]:.3f}), rms {rms:.2f}")
    assert abs(est[0] - true[0]) < 0.05 and abs(est[1] - true[1]) < 0.05, "fitter does not recover known constants"
    print("selftest ok")

if __name__ == "__main__":
    if "--selftest" in sys.argv:
        selftest(); sys.exit()
    import csv
    rows = list(csv.DictReader(open(sys.argv[1])))
    names = ["p1", "p5", "p25", "p50", "p75", "p95", "p99"]
    lin = np.array([[float(r[n + "_lin"]) for n in names] for r in rows])
    jpg = np.array([[float(r[n + "_jpg"]) for n in names] for r in rows])
    params, _ = fit(lin, jpg)
    report(lin, jpg, params, names)
    # hold-out: fit on all but one, report on that one, for every frame
    print("\nleave-one-out RMS per frame:")
    for i, r in enumerate(rows):
        m = np.arange(len(rows)) != i
        p, _ = fit(lin[m], jpg[m])
        e = np.sqrt(np.mean((curve(lin[i], *p) - jpg[i]) ** 2))
        print(f"  {r['frame']:>14}: {e:5.2f}")
