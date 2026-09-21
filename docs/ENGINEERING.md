# Engineering notes

How the interesting parts of Numa work, and the measurement that settled each
one. Moved out of the README, which is for people who want to use the thing.

Every number here was measured on a real frame rather than reasoned about.
Several of these sections exist because the reasoning was wrong and the
measurement said so — the demosaic that was silently falling back, the camera
profile that made colour *worse*, the tone curve that was a display encoding
rather than a rendering. `docs/FEATURES.md` is the index of what is built;
this is why it is built that way.

The photograph's path through the code is summarised in "The render pipeline"
below; the sections before it take its stages one at a time, and the sections
after the rule are about the library, the catalog and the models around it.

---

## White balance and colour

`decode_linear` deliberately skips rawler's `WhiteBalance`, `Calibrate` and
`SRgb` steps, so the pixels stay **camera-native**. White balance is then ours
to change, and it is applied where it physically belongs — as per-channel sensor
gains, before the colour matrix. Doing it after the matrix, on display RGB,
shifts hues that a real RAW editor leaves alone.

Temperature and tint become channel multipliers through the Planckian locus and
the camera's own XYZ matrix. The reverse — reading the camera's coefficients
back as a Kelvin value — is a joint fit over both parameters, since each moves
red and blue.

Verified against rawler on a real X-T5 frame: at as-shot settings our path
reproduces rawler's own `Calibrate` output to within **1/255** across 99.7 % of
pixels. The remaining 0.3 % are blown highlights, where we deliberately differ:
rawler clips right after the matrix, we carry over-range values through to the
display transform so highlight recovery has something to recover. Clipping
happens once, at the end, rolling off towards white rather than per channel so a
blown sky does not turn cyan.

**The pipette** (`TOOL-004`) is the same fit asked a different question. A few
pixels of the unadjusted frame — rendered at the camera's own balance, clipped
ones skipped — go back through the inverse of the camera matrix to what the
balanced sensor saw; the multipliers that would make that grey are the camera's
times its reciprocal, and `white_balance_for` turns those into a temperature and
tint exactly as it does for the as-shot coefficients. One solver, so the pipette
and the camera cannot disagree about what a Kelvin is. A grey card under known
light comes back within 2 % in a test; in the window a click on a chart's grey
moved 4702 K / −48 to 5201 K / −41 (in the stored sign; the panel now shows
Adobe's, +48 and +41 — see "Lightroom's answers" below). The frame it samples
carries the base tone curve per channel and no sRGB encoding after it, so the
codes go back through `tone::scene_value_for`; read as plain sRGB, as they first
were, a 3200 K card came back as 2791 K (FT-028 #5). Both pipettes draw their own cursor, a
pipette with its tip as the hotspot: the theme has no colour-picker cursor, and
the crosshair it fell back to was a coarse plus that hid the pixel being picked.

## The base tone curve

Applying only the sRGB gamma to scene-linear data is a display *encoding*, not a
rendering — and it looks it. Measured against the camera's own JPEG of the same
frame, a bare gamma gave a mean of 90 against 124, a contrast of 42 against 62,
and blacks sitting at 30 instead of 17: flat, dark and milky, with a grey veil
over everything. Saturation was identical, so it was never a colour problem.

`RENDER-004` adds the missing rendering intent as a sigmoid in log2 exposure,
plus a baseline exposure applied at decode time. Both constants are **fitted
against the camera**, not chosen: the embedded JPEG in a RAF is the
manufacturer's own rendering of that exposure, so `tests/colour_ab.rs` sweeps
both parameters across eight frames and minimises the difference in their tonal
percentiles.

| | Camera | Bare gamma | Fitted curve |
|---|---|---|---|
| Mean | 124.5 | 90.5 | 126.3 |
| Contrast (σ) | 62.4 | 41.9 | 61.2 |
| Blacks (p1) | 17.4 | 29.9 | 18.2 |
| Midtones (p50) | 108.8 | 76.3 | 113.7 |
| Highlights (p99) | 246.4 | 205.1 | 238.4 |

Middle grey is pinned, so 0.18 still lands on 118. The baseline exposure lives
in `io::raw` rather than in the curve so that everything downstream — the
contrast pivot, the tone-region masks — can take 0.18 to mean middle grey.

Sensor white is deliberately **not** display white: the gap is headroom the
curve rolls off into. Being per-channel, the sigmoid also desaturates blown
highlights towards white on its own, which replaced a hand-rolled clipping step.

The numbers are one camera's calibration (`BASELINE_EV = 1.241`,
`CURVE_CONTRAST = 0.939`). A second body needs a per-camera table.

## Camera profiles

A 3×3 matrix is a linear fit to a sensor that is not linear. It is
systematically wrong, worst on saturated colour, and no tone curve reaches it.
Every real raw converter ships a *camera profile* instead: the matrix, plus a
three-dimensional HSV table that cancels its error, plus optionally a second
table carrying a look. That is what "nicer colour out of the box" actually is.

`RENDER-005` reads DNG camera profiles. A `.dcp` is a TIFF holding the DNG
profile tags — with a deliberately different magic number so it is not mistaken
for an image — so rawler's TIFF reader parses it once the magic is patched back.
Supported: `ForwardMatrix1/2`, `ColorMatrix1/2`, `ProfileHueSatMapDims/Data1/2`,
`ProfileLookTableDims/Data`, `ProfileToneCurve`, and the two calibration
illuminants.

The tables are applied in ProPhoto RGB's HSV, where the specification defines
them and where the shipped tables were fitted: hue shifts wrap, saturation and
value clamp, and the two calibrations are blended in mired space by the chosen
colour temperature.

**Where profiles come from.** `dcp::search_paths` looks, nearest first, in
`$XDG_DATA_HOME/numa/profiles/` for anything the photographer puts there; in
`$XDG_DATA_HOME/numa/rawtherapee-dcpprofiles/`, where Numa unpacks RawTherapee's
set when it is downloaded from Preferences; in `share/numa/profiles/` beside the
binary, which is where the AppImage carries the same set; and in
`/usr/share/rawtherapee/dcpprofiles/` on a desktop that has RawTherapee
installed. The set can be carried at all because RawTherapee is GPL-3.0 in its
entirety and the profiles live in its repository: the repository's licence is
the grant, and the `ProfileCopyright` tag inside each file (46 say public
domain, 9 CC0, 63 name Maciej Dworak) is an authorship statement rather than
permission withheld — which was the first reading, and was wrong. What it
obliges is shipping the licence and the attribution beside them, which both the
AppImage and the download do. The Flatpak bundles RawTherapee's set and reads
the host's `~/.local/share/numa` (see *The Flatpak* below). The panel names whichever profile is in use.

Matching is on the profile's own `UniqueCameraModel` tag, not its file name.
Adobe writes `Fujifilm X-T5 Adobe Standard.dcp` and RawTherapee writes
`FUJIFILM X-T4.dcp`; both record the camera inside, which is what the DNG spec
provides that tag for. So any `.dcp` dropped into the profiles directory is
found whatever it is called — including the ones an installed Lightroom already
has. A match on the model alone counts only when what precedes it in the tag is
this camera's make: `olympusem10` ends in `m10`, and until that check a Leica
M10 took the Olympus E-M10's profile.

**Exact model matches only.** A near-match used to be offered — an X-T5 reaching
for an X-T4, on the reasoning that a correction fitted to a similar sensor beats
none. Measured against the camera's own rendering across eight frames it lost
every time, sometimes by more than double the colour error:

| Frame | Matrix only | + X-T4 profile |
|---|---|---|
| DSCF2059 | 0.0820 | 0.1825 |
| DSCF2159 | 0.0911 | 0.1210 |
| DSCF2482 | 0.0679 | 0.0921 |

A hue/saturation map fitted to a different sensor is not an approximation of
this one's; it is a different answer to a different question. A body with no
profile now gets the bare matrix, which is what every converter does in the same
situation. Copying a neighbouring body's profile in under your own camera's name
still works, and needs no code.

**Automatic takes the standard profile, never a look** (`RENDER-013`). With
several folders searched, one body can have a dozen matches — Adobe Standard,
Camera Velvia, an infrared profile — and taking whichever the folder listed
first would make a look every photograph's colour without anyone choosing it.
So `dcp::standard_rank` orders them: Adobe Standard first; then whatever
RawTherapee shipped, since it ships one plain profile per body whatever that
file is called; then, in the photographer's own folder, a profile named after
the body. Everything else stays in the picker. Checked against every installed
profile: no body lost its automatic choice.

The white-balance stage is split from the operation stack (`to_working_space`
and `apply_stack`) because the profile's tables depend only on the white point.
The editor caches the working-space image under `colour_key` — the white
balance, the profile and the AI denoise amount, the three things that stage
reads — so a slider drag still re-runs only the ~30 ms stack rather than the
~43 ms colour stage.

## Fitting a profile against the camera

`RENDER-012`. Where no profile is good enough, the camera's own JPEG says what
the colour should be, frame by frame. `tests/camera_profile_fit.rs` (ignored;
run with `RAF_DIR` and `FILM`) takes X-T5 frames of one film simulation at
Color 0, pairs Numa's matrix-only rendering with the embedded JPEG at 300 pixels
— the tone curve inverted, clipped, dark and edge pixels skipped — and fits a
DNG hue/saturation/value map as a regularised least squares over the table's
own interpolation weights: neighbouring cells tied together, a small pull
towards identity, grey's value held. The result is written by `dcp::write`, the
reverse of `read` — one little-endian IFD with the spec's own tags — so what
comes out is an ordinary `.dcp` that Numa, or any DNG reader, can use.

The last quarter of the frames are never seen by the fit, and the grid and the
smoothness are chosen on a quarter of the rest. Mean chroma error on the
held-out frames:

| Film simulation | Matrix only | Adobe Standard | Fitted |
|---|---|---|---|
| Classic Chrome | 0.0687 | 0.0581 | **0.0310**, better on 12 of 12 |
| Eterna | 0.0344 | 0.0303 | **0.0126**, better on 12 of 12 |

Optimistic, since the held-out frames share trips with the fitted ones — and
these are looks at Color 0, not a neutral profile, which needs frames shot for
the purpose.

## Orientation

A sensor always reads out the same way round, so a portrait frame is stored
landscape with a tag saying how to present it. In this library that is **1165 of
1589 frames** — ignoring the tag does not mean a few sideways photos, it means
most of them.

`TOOL-003` applies it on both paths: the preview that fills the library, and the
demosaiced image the canvas and the export use. Both are checked against each
other, because a thumbnail whose shape disagrees with what opening it shows is
its own kind of wrong.

The tag comes from EXIF rather than `RawImage::orientation` — rawler's RAF
decoder leaves that field at `Normal`, which reads as "no rotation" for every
portrait frame and fails silently. The thumbnail cache carries a version so the
sideways ones already on disk are ignored rather than served, and because the
thumbnails are written upright their JPEG headers are what the library's rows
read a photograph's shape from (see "Rows at the photograph's shape").

## Seeing real pixels, and cropping

`PERF-002`. A proxy exists so a slider drag is cheap; it is exactly the wrong
thing to magnify. At 1:1 the editor decodes the original and keeps it past the
colour stage — the expensive half, and the half a tone slider does not change.

| | Proxy | Full resolution |
|---|---|---|
| First view | — | 403 ms decode + 470 ms colour |
| Every slider tick | 21 ms | 225 ms |
| Held in memory | 44 MiB | 455 MiB |

Half a gigabyte is not worth keeping for a view nobody is looking at, so it is
dropped the moment the view returns to fit. Until it is ready the readout says
**"100% soft"** rather than claiming a magnification it is not delivering — a
100% that is really 31% is the exact lie this feature exists to stop telling.

`TOOL-002` is crop and straighten. Both happen in one pass: each output pixel is
mapped back through the rotation to where it came from and sampled bilinearly,
because going forwards leaves holes wherever the rotation stretches. Outside the
frame it is black. It used to extend the edge, on the reasoning that a crop
takes those pixels away; but the crop tool shows the whole corrected frame, and
a strong keystone stretched a row of edge pixels into streaks across a third of
it, which read as the photograph breaking. Within half a pixel of the edge the
sampler still clamps, so the frame's own border is not darkened.

Geometry runs **before** the operation stack, so local tone mapping and the
histogram describe the photograph actually being kept rather than the one before
its edges came off. The rectangle is stored in 0..1 of the image, which is what
lets it survive zoom, storage, and a quarter turn.

The overlay maps widget coordinates through the letterbox bars rather than
treating the widget as the image — otherwise the crop lands somewhere other than
where it was drawn, by the width of those bars.

## Perspective, and the largest crop it leaves

`GEOM-001`. A camera pointed up at a building makes its verticals converge, and
no rotation reaches that: it is a projection. `core::image::source_map` is the
one function that says where an output pixel comes from, and it is shared by
the resampling loop and by the fit below, because the fit has to ask exactly the
question the renderer answers. It turns the pixel's offset by the straighten
angle, scales it by the aspect, and then divides by the depth at that point in
the frame:

    reach = 1 / (1 − v·y/half_height − h·x/half_width)

At the centre the depth is one and nothing moves, which holds the middle still
while the edges are pulled about. It was `1 + k` — the same to first order, and
fine for a few degrees — but at −40/+40 it bent every line in the frame and
pulled a bicycle in a corner into a curve. A division makes the map a
homography, so a straight line stays straight; a test checks that with both
corrections at once, and a re-render of a strong correction has its door frame
and skirting straight. The slider values are halved on the way in
(`Perspective::coefficients`), because at a coefficient of one the far edge maps
to a point and the arithmetic divides by nothing; half is already a threefold
stretch from one edge to the other. The aspect is exponential, so −50 and +50
are the same correction either way round.

The keystone is measured from the frame's centre and against the frame's edges,
not the crop's. The correction belongs to the lens, so a crop of a corrected
frame has to be that frame cut down rather than a different correction — which
is what the crop tool shows, and what a crop drawn on it has to mean.

**The fitted crop.** A keystone reaches past the photograph at one edge, and
what is out there is black. The crop that avoids it is written into the crop
rectangle rather than hidden inside the render, so the crop tool shows the whole
corrected frame with what was cut away darkened and a corner can be pulled back
out. The fit (`image::fit`, behind `crop_inside`) finds the largest crop of the
chosen shape that the photograph fills. It first shrank the rectangle about its own centre until one
corner touched, which gave up a band of photograph on the opposite sides for
nothing. Now it uses the fact that the map is a projection: what the photograph
covers, taken back through the keystone, is a convex quadrilateral, and a
rectangle lies inside it exactly when its four corners do. For a given size the
centres that work are that quadrilateral intersected with itself moved by each
corner — a convex polygon, clipped with Sutherland–Hodgman — which is either
empty or not. So the size is found by bisection (24 passes, up to the whole
frame), the centre is the point of that polygon nearest where the crop was, and
the answer is checked against `source_map` itself (`crop_fits`, asking of the
corner pixels' centres exactly what `cropped` will sample) and shrunk by a hair
if the two disagree. A test on the frame the fault was reported on finds a crop
larger than the centred one, with nothing larger fitting beside it. A frame with
no correction is never quietly zoomed into.

**Two rectangles, one crop.** `source_map` turns the frame about the crop
rectangle's own centre *in the source*, so the crop's frame is an upright
rectangle of the crop tool's view — the whole corrected frame — whose centre is
the rectangle's, turned back through the straighten angle and the aspect
stretch (`crop_in_view`). Off the middle and straightened those differ: the
left half of a 40 MP frame at five degrees lands 168 pixels off what was drawn. The tool draws and
drags in the view and crosses to the stored rectangle when it commits
(`crop_from_view`), so no crop already stored renders differently. The masks,
which are fractions of the crop they were made on, are mapped from the view
the same way while the tool is open (`view_to_crop`, an affine map, since a
straighten since then turns one frame against the other); the tests hold both
to `cropped`'s own pixels.

**The photograph is the largest frame.** `crop_inside` is the fit capped at the
crop's own size, so it never grows: a crop that fits is left alone, one that
does not shrinks to the largest of its shape that fits, as near where it was as
that allows. It is the only fit there is — the keystone's grew the crop up to
the whole frame, which enlarged a crop drawn small — and every commit goes
through it, so straightening, a keystone, a quarter turn and a drag all end on
the photograph; a drag also stops at the edge while it is happening, each axis
on its own when no ratio holds them together. The fit is made from the
rectangle the photographer left rather than from the last fit, as long as
nobody has moved it since, so a keystone or an angle taken back to nothing
gives the crop back as it was (FT-028 #3).

**Flips** (`GEOM-004`) are one flag on the turn: a left-to-right mirror applied
before the quarter turns, so a vertical flip is that mirror and half a turn, and
the eight ways a frame can face are one flag and an angle. Flipping what is on
screen accounts for a sideways frame, and the crop, its straighten angle and the
perspective are mirrored along with it.

**Manual lens corrections** (`OPTICS-004`) are two sliders turned into a lens
profile of the same shape the camera's tables have, and handed to the same
correction code as `OPTICS-001/002`. They run first in `geometry_of`, before the
flip, the turns and the crop, because a lens's distortion is about the distance
from the lens's centre, which a crop moves. They are part of the key a cached
view was cut under, and a test checks the corners change and the centre does
not.

So `render::geometry_of` runs, in order: manual lens corrections, the flip, the
quarter turns, and then crop, straighten and perspective in one resampling pass.

## Two silent failures, one lesson

The 1:1 view was soft for three rounds of "it still is not sharp", and both
causes were things that failed without saying so.

**The demosaic was falling back.** rawler's Markesteijn indexes its CFA pattern
with deliberately wrapping arithmetic — `(row + 48) % 48` on a `usize` that is
sometimes a wrapped negative. Release builds turn overflow checks off and it
works; debug builds leave them on and it panics. So every `cargo run` fell back
to bilinear while every `--release` benchmark reported the thing was fine, and a
scan of eighty frames found nothing because the scan was a benchmark.

`opt-level` does not imply `overflow-checks`; they are separate settings, which
is why the profile entry added for image decoding did not cover this. rawler now
has its own entry with both.

**And nothing said so**, because `log::warn!` had nowhere to go: no logger was
ever initialised, so every warning in the codebase was dead code. `UX-011` fixes
that — warnings on by default, `RUST_LOG` to widen. The first thing it printed
was the name of the file that had been failing all along.

The lesson is narrower than "test what you ship": a benchmark that runs in a
different profile from the application is not measuring the application.

## Magnified pixels should look like pixels

`Gtk.Picture` scales its texture with a linear filter. That is right when
shrinking and wrong when magnifying: at 500 % each source pixel becomes a smooth
gradient across a 5 × 5 block, so detail that is genuinely there reads as detail
that is missing. It is what turned a correct full-resolution view into "the
detail is not really there".

`CANVAS-007` is a small `gdk::Paintable` that snapshots with
`ScalingFilter::Nearest` above 1:1 and `Linear` at or below it — the same rule
every other editor uses. It plugs into the existing `Gtk.Picture`, so none of
the layout changed.

**And 100 % has to mean one photograph pixel per screen pixel** (`CANVAS-005`).
The canvas was sized at one photograph pixel per *layout* pixel, which at a
display scale of two draws every pixel four times — a "100 %" that could not
show focus. A zoom is now photograph pixels per screen pixel: the widget is
sized from it divided by the scale factor, Fit reports the real ratio, and
zooming about the centre converts back to layout pixels. The render resolution
was already asked for in screen pixels, so nothing below the canvas changed.
Checked with `GDK_SCALE=2` under Xvfb, where Fit reads 30 % for what used to
read 15 %.

The guides over the canvas (`CANVAS-004`, thirds or a grid) are drawn in a layer
of their own above the photograph's rectangle and below the tools, which takes
no clicks — so turning them on changes nothing about what a click on the canvas
does.

## Saying what is on screen

`UX-010` puts the camera, lens, exposure and film mode in a popover on the
editor bar, and one line more that matters as much: **which resolution the
canvas is actually showing**. A proxy magnified past 100 % looks like a soft
photograph rather than a magnified small one, and the difference is not
something anyone should have to guess at.

That line exists because of a bug it would have caught immediately. The colour
stage is cached under a key, and two copies of that key had drifted: the
full-resolution cache included the film simulation while the renderer's did not.
Selecting a look meant the 1:1 image was decoded and then silently ignored —
the readout said "soft" and nothing said why. One `colour_key` function now
serves both, so they cannot diverge again, and it is a type as well as a
function: adding something to the colour stage is a compile error at both call
sites rather than a silent mismatch at one.

## Demosaic: where the detail went

X-Trans reconstruction is the one place a cheap algorithm shows. Bilinear on a
6×6 mosaic is visibly mushy — a crane's lattice becomes a smear — and that was
the answer to "why does RawTherapee show so much more at the same zoom".

The original reasoning was that a threefold proxy downscale averages bilinear's
artefacts away before anyone sees them. That held, and it does not generalise:
at 1:1 there is no downscale left to hide behind.

Measured on a 40 MP X-T5 frame, as acutance normalised by the image's own mean —
the division matters, because without it a brighter render scores higher for
being brighter and says nothing about detail:

| | Bilinear | Markesteijn | |
|---|---|---|---|
| Full resolution | 0.0287 | 0.0519 | **+81 %** |
| At proxy size | 0.0698 | 0.0723 | +3.6 % |
| Cost | 330 ms | 980 ms | |

So it is not a quality setting but a question of whether the difference is
visible: at 1:1 and in an export it plainly is, on a proxy it is not.
`io::raw::Demosaic` picks accordingly, and rawler's own pipeline hardcodes
bilinear for X-Trans, so `markesteijn_develop` repeats the geometry around it —
rescale, region of interest, default crop — and swaps the one step that matters.

### The colour it invents, and the step that takes it back

Two thirds of an X-Trans frame has no red and no blue sample, so where the
interpolation guesses wrong it writes a coloured pixel the scene never had.
FT-019 measured the cause twice, each time with one variable: inside
RawTherapee, turning `CcSteps` on took 81 % of the high-pass a\* and 71 % of
the b\* out of a flat patch; and on rawler's own Markesteijn output, a 3×3
median of the log2 channel ratios took 63 % / 68 %. Three Markesteijn passes
took 1.5 % for 0.9 s, which is why there is one pass and a median rather than
three passes.

`suppress_false_colour` is that median, and two things about it are worth
keeping.

**It medians the ratio, not the colour.** Green is never written; R and B come
back as `G × median(R/G)` and `G × median(B/G)`. A median commutes with a
monotonic transform, so the median of `log2(R/G)` is the log2 of the median of
`R/G` — the plain ratio is computed and 240 million logarithms are skipped for
the same answer. Because green is untouched, the luminance the sensor actually
resolved cannot be blurred by a colour repair: the green-channel acutance is
the same number before and after, to five digits.

**A sorting network, not a quickselect.** `select_nth_unstable_by` on a
nine-element array, eighty million times, cost **0.59 s** on a 40 MP frame.
Smith's 19-comparison median-of-nine network, branchless on `f32::min`/`max`,
costs **0.14 s** for the identical output — every reference hash matches
between the two. A false-colour step that cost two thirds of the `passes: 3`
FT-019 rejected would have been arguing with its own ticket.

**It runs after the lens geometry, and that is most of what it is worth.**
For one commit it ran at the demosaic, where FT-019 measured it, and two
thirds of the benefit was thrown away downstream. `correct_geometry` samples
R, G and B at three different radii to undo lateral colour
(`LensProfile::source_radius`); on DSCF9580's lens those radii differ by
3.4e-4, which near the frame edge is about a pixel — enough that R is no
longer read from where G was read, so the frame's own luminance texture comes
back as ratio noise *after* the step that cleaned it. Measured at the end of
`decode_with` on FT-019's patch:

| | hp log2(R/G) | hp log2(B/G) |
|---|---|---|
| no step | 0.0585 | 0.0965 |
| at the demosaic | 0.0492 (−16 %) | 0.0623 (−35 %) |
| after the geometry | 0.0207 (−65 %) | 0.0351 (−64 %) |

The rig's own at-demosaic figures are 0.0184 and 0.0308, so after the geometry
the finished photograph lands within a hair of what the measurement promised.
In CIELAB on the same patch at sharpening 0 and colour 0, high-pass a\* goes
2.364 → 0.843 and b\* 5.085 → 1.864. It costs nothing to move: 3 847 ms
against 3 850 ms for a decode and a full develop, the same work in a different
place.

`correct_vignetting` is the same gain on all three channels and cannot move a
ratio, so which side of it this falls on does not matter. What does matter is
the black floor: `apply_scaling` leaves the pixels at 0..1 and the baseline
multiply has moved them by the time the late call sees them, so the threshold
travels with them as `BLACK_FLOOR * baseline`.

**After the geometry, green's acutance stops being the whole answer.** At the
demosaic the step could not touch luminance detail and one number said so:
green is never written, and its acutance was identical to five digits. That
argument does not survive the move, because `correct_geometry` has resampled R
and B onto green's grid by then, so a median of the ratios *can* reach real
red and blue detail. The honest measurement is one acutance per channel over
a detailed patch — the stag's head on DSCF9580:

| | R | G | B |
|---|---|---|---|
| at the demosaic | 0.060394 | 0.056129 | 0.072289 |
| after the geometry | 0.061307 (+1.5 %) | 0.056129 (=) | 0.075907 (+5.0 %) |

Red and blue come out **sharper**. The resampling that undoes lateral CA is
bilinear, which softens R and B relative to green; pulling their ratios back
onto green's structure puts that back. Meanwhile high-pass L\* on that patch
falls 3.2 %, which reads like the opposite until you notice the two metrics
measure different things: high-pass L\* is an RMS residual after a 3×3 mean
and is dominated by noise, acutance is mean gradient magnitude and is
dominated by structure. Noise leaving while structure holds moves them in
opposite directions, and that is the whole of it.

**Two ways this step is easy to misjudge, both arithmetic.** They are worth
writing down because the acceptance for A9 was stated in exactly these terms.

*High-pass L\* falls on a flat patch even when nothing was blurred.* About
28 % of the luminance comes from R and B, so removing their noise removes some
of the luminance noise with it: 0.721 → 0.695 on FT-019's out-of-focus grass.
The test that separates noise from detail is to measure a patch that **has**
detail — the stag's head at (1350, 3650) — where the same change moves
high-pass L\* 1.473 → 1.468, three tenths of a per cent, while high-pass a\*
falls 34 % and b\* 44 %. RawTherapee's own `CcSteps` moved the flat patch's
high-pass L\* by −8.4 % and FT-019 called it unchanged; this is −3.6 %.

*A mean of `sqrt(a*² + b*²)` is biased upwards by chroma noise*, because a
magnitude cannot cancel the way a signed mean can. On a bright, saturated edge
that hardly matters: the red lettering on a pale wall in DSCF0567 has noise a
tenth of its chroma, and the band's mean C\* moves −0.5 %. On a dark, noisy
one it dominates: the bamboo culm against leaf litter in DSCF0247 has noise
half its chroma, and its mean C\* reads −7.3 % — while the mean a\*/b\*
*vector* over the same band grows 1.2 % longer, and subtracting the measured
noise power from the magnitude gives the same +1.2 %. The edge is not being
desaturated; the noise that was inflating the average has gone. Any future
saturation test on a noisy edge wants the signed means printed beside the
magnitude, which `tests/demosaic_probe.rs::coloured_edge` now does.

## Matching the camera's own rendering

Two things were making frames look wrong next to another converter, and neither
was a matter of taste.

**Lens falloff.** A Fujifilm RAF records the vignetting its own JPEG engine
corrects for — a handful of image radii and the percentage of light reaching
each. On an 18 mm shot at f/2.8 the corner keeps **47 %**, a full stop. Ignoring
it means the corners really are a stop darker than in any converter that reads
it. `OPTICS-001` applies it. The data lives in the FujiIFD, a sub-IFD the RAF
header points at from a fixed offset — not in the MakerNote where the film
simulation is.

Verified against the camera's own JPEG, corner versus centre in linear light:
uncorrected **−0.74 EV**, corrected **+0.07 EV**.

**Lens geometry.** The same sub-IFD carries two more tables on the same nine
radii: how far the lens bends the frame (`0xf00b`, per cent of the radius) and
how far red and blue land from green (`0xf00f`, a fraction of it). `OPTICS-002`
applies both in one resampling pass, because they are the same arithmetic —
every destination pixel reads from a radius of its own, and the three channels
read from three slightly different ones. Doing them separately would interpolate
the frame twice for nothing.

Which way the correction goes was settled by measurement rather than by reading.
Each direction was given its own best global scale — the camera's JPEG is
cropped against the raw frame, so a plain comparison measures that crop instead
— and then judged on how closely the two frames' edges agree:

| Frame | Distortion at the corner | Uncorrected | As stored | Reversed |
|---|---|---|---|---|
| DSCF4225 | −5.01 % | 9.31 | **4.60** | 10.66 |
| DSCF8224 | −2.12 % | 13.04 | **9.24** | 15.43 |
| DSCF5591 | +2.93 % | 7.51 | **4.58** | 9.52 |

So `r_source = r_destination × (1 + k/100)`, with the table's sign as it stands:
negative is barrel and reads from closer in, positive is pincushion and reads
from further out. Pincushion would read from outside the frame at the corners,
so the whole map is pulled in until it does not — the same slight crop the
camera takes, and the alternative is a smeared edge where there was no light.
Afterwards the pincushion frame lines up with the camera's at a scale of 1.000.

The colour half is smaller and less certain: the largest value anywhere in a
library of 3925 frames is 0.0011, which is five pixels at the corner of a
40 MP frame. As a *percentage* it would be a twentieth of a pixel, which is not
worth nine samples to describe, so it is read as a fraction. The direction is
confirmed — blue really does sit further out than red in the uncorrected frame,
which is what the table says — but the magnitude was not resolvable with the
frames to hand.

Files that are not RAFs get the same two corrections from lensfun's database
(`OPTICS-005`), sampled onto the same nine radii and handed to the same code.
Both happen at decode time, before the colour stage, so everything after them
sees the frame the lens should have drawn.

**Brightness.** A single calibrated baseline exposure cannot match a
scene-adaptive JPEG engine. On a bright frame the two agreed; on a dim museum
interior they were more than a stop apart. `RENDER-008` asks the camera instead:
the embedded JPEG is that engine's own answer for this exact scene, so its
median run backwards through our tone curve says where our midtones belong. One
number per frame, solved rather than searched, because the curve is monotonic —
which is why `core::tone` owns both directions.

| DSCF3678, a dim interior | Camera | Fixed baseline | Matched |
|---|---|---|---|
| Midtones | 102.5 | 47.8 | 116.5 |
| Mean RGB error | — | 44.6 | 17.8 |

The bright frame the baseline was originally fitted against stays where it was,
so this is a refinement rather than a replacement: `BASELINE_EV` still stands in
for files with no usable preview. The match asks `embedded_preview` for the
camera's JPEG and nothing else — see the next section for why that distinction
had to be made.

## Film simulations

The camera records which simulation a frame was shot with, and the panel says
so. The `FilmMode` tag lives in Fujifilm's MakerNote, which rawler does not
surface, so `io::raw::film_mode` walks it directly: a little-endian IFD
introduced by the ASCII marker `FUJIFILM` and its own offset. The codes were
confirmed against a library of real frames with exiftool as the reference —
`0x0600` Classic Chrome, `0x0700` Eterna, `0x0800` Classic Negative.

Reproducing the look is not done, and the attempt was taken out (`RENDER-006`).
The third-party HSV tables that came closest did not match the camera closely
enough to trust, and a look that is nearly right makes every other colour
judgement suspect; they were also licensed non-commercially, which is not a
constraint this program wants to carry. A camera profile's own look table
(`RENDER-005`) is still applied, because that is part of the profile's
rendering rather than a simulation on top of it. Fitting looks against the
camera's own JPEGs (`RENDER-012`, above) is the route back.

## Files that are not shaped like a RAF

Most of the pipeline is format-agnostic, and the places where it was not were
all found by a file from another camera.

**No embedded preview** (`IO-008`). rawler offers no preview for ORF, SRW, IIQ
and phone DNGs, so their cards stayed empty. `raw::preview` now falls back to
`developed_preview`: a draft decode, reduced to 1920 pixels and rendered with
the default document. That is a second or two once, and the thumbnail keeps it.
It runs one at a time behind a lock, because a full decode of a 150 MP back is
gigabytes and the thumbnail workers would otherwise run eight at once and
exhaust memory. And it exposed a recursion: the exposure match (`RENDER-008`)
asked `preview` for the camera's JPEG, which for these files now developed the
raw to measure the raw, which asked again, until memory ran out. The match asks
`embedded_preview` instead, which answers only with what the camera wrote.
Checked on 41 sample files.

**Matrices not labelled D65.** `camera_profile` took only a D65 colour matrix,
so a DNG calibrated at A and D50 — a Leica Q2 — got no conversion at all and its
raw channels were shown as if they were sRGB, in a strong teal. It now takes the
matrix nearest daylight the file has, and any matrix before none. Checked on a
Q2 frame against its embedded preview.

**HEIF and HEIC** (`IO-015`) are decoded by `heic-rs`, pure Rust, with its steps
called one at a time rather than through `heic_rs::decode`. The reason is its
colour fallback: a file with no nclx `colr` box — every iPhone photograph, which
carries an ICC profile instead — was converted as BT.709 limited range where the
stream is BT.601 full range, stretching the contrast and shifting the colours:
28 dB from libheif across 18 iPhone files. With libheif's fallback in its place
the same files match libheif to 47–57 dB, at 50–70 ms each. A phone's frame is
a grid of 512-pixel tiles, decoded in parallel and composed.

**A decoder that panics.** `raw::summary` reads a frame's metadata for the info
popover and for the camera sample a new library is described with. On a Sigma
X3F, a format rawler lists as supported, it panicked — and inside a GTK
callback a panic cannot unwind, so the application aborted. `summary` now runs
behind `catch_unwind`, and a file it cannot describe is a file with no summary.
The Markesteijn demosaic has had the same net since "Two silent failures".

## AI denoise, worked out once and kept

`DETAIL-003` runs SCUNet — the real-world, PSNR-trained checkpoint, which stays
faithful to the photograph rather than inventing texture as the GAN one does —
through ONNX Runtime. A 256-pixel tile takes about half a second on the CPU: a
whole 7728×5152 X-T5 frame at ISO 3200 was 805 tiles in 516 s. That makes it a
pass run once per photograph in the background, never something a slider waits
on, so the design is about what is kept and where it re-enters the pipeline.

**What the model is shown** is what a blind real-world denoiser was trained on:
the frame at the camera's own white balance and matrix, sRGB-encoded. Tiles of
256 overlap by 32 and are feathered together, each pixel the weighted mean of
the tiles that covered it, with a tile's edge pixels — the ones with least
context — counting least. A tile running off the frame repeats its last row, so
every tile is the same shape and the runtime plans for one.

**What is kept** is the model's answer as a PNG in the user's cache, keyed by
path, modification time, model and a version, with FNV-1a rather than
`DefaultHasher`, whose output may change between Rust releases: a thumbnail lost
to that costs a decode, this costs minutes. The cache rather than the library's
`.numa` folder, because it is tens of megabytes a photograph, can always be
worked out again, and a library folder is the thing that gets synced and backed
up. Twelve bits in a sixteen-bit sample: the model's low bits are noise of its
own, which PNG cannot compress. On an ISO 3200 crop, sixteen bits at the
encoder's fastest setting took 31.6 bits a pixel, twelve bits 29.3, and twelve
bits at the default level with adaptive filtering 14.5 — 69 MB for the 40 MP
frame above. Written under a `.part` name and renamed, so a half-written file is
never read.

**What goes back** is the *difference* the model made, carried into camera space
through the inverse of the matrix it was shown through, at the very start of the
colour stage (`ai_denoise::for_render`). So the white balance and the profile
still apply afterwards and can change; a highlight above white, which the model
could only see clipped, comes through untouched; and the proxy, the 1:1 view and
the export all start from the same frame without knowing it is there. The
amount is a blend at that point, which is why it is part of `colour_key`. Read
back, the kept frame costs 0.57 s at full size and 0.53 s at the proxy's, then
5 ms from memory: answers up to 4096 pixels on the long edge are held, because
the proxy is re-developed on every white-balance tick, while the full-size frame
is developed once per zoom or export and half a gigabyte is not worth holding
for that.

## The effects a preset leans on, and Point Colour

Calibration, dehaze, vignette and grain (`ADJ-008`, `FILTER-006`, `FILTER-003`,
`FILTER-007`) arrived together because a Lightroom preset uses them and the panel
had none. All four work on the linear working-space buffer in
`render::effects`, and where each sits in `apply_pixels` follows from what it is
a property of.

**Calibration** is a matrix, as in a camera profile: each primary's colour — its
distance from the grey of the same brightness — is turned about the neutral axis
(up to 30°, towards the next primary, the way Lightroom's sliders go) and scaled.
It acts on what is over a pixel's grey — `min(r, g, b)` taken off, which leaves at
most the two primaries the colour is made of — so grey and white stay put by
construction. It first held white by taking the drift back out of every primary
in proportion to its brightness, which carried Red −100 into aqua further than
into red (FT-028 #8). A shadow tint moves green in the shadows only.
It runs where a profile acts: after the detail passes and before anything reads
colour.

**Dehaze** is the dark channel prior. In a clear photograph almost every patch
has some channel near black, so how far a patch's darkest channel sits above
black is how much airlight is in front of it. Patches are a sixty-fourth of the
long edge; the transmission is worked out per patch, smoothed, and each pixel is
unmixed from the airlight by it. The airlight is taken as *grey* at the haziest
patches' brightness, because those patches are a roof or a field as often as
sky, and unmixing a coloured airlight turned a drone frame cyan at +70 and pink
at −70. It runs on the scene before any tone is laid over it — haze is a
property of the light, and a curve first would bend what it measures — and
because it measures the whole frame it keeps a render off tiles.

**Vignette** works in stops, so it darkens a bright sky and a shadow in the same
proportion, as a lens does; roundness runs from the frame's rectangle through
its ellipse to a circle. **Grain** is two octaves of value noise, each on a
lattice turned against the pixels and against the other, strongest in the
midtones and nothing in black or white. Both run last before the display
transform — a vignette is the frame's, and grain is the last thing on the print
— and both are placed by where a pixel sits in the *whole* frame, which the
`region` argument gives. Grain is pinned to full-resolution coordinates, so
zooming shows the same grain larger and a 1:1 tile shows exactly the grain of
its part of the export.

**Point Colour** (`ADJ-006`) is the mixer's question asked of one colour rather
than a band. The mixer divides the wheel by hue alone, so skin and brick are
both Orange to it. A point is a hue, a chroma and a lightness picked off the
unadjusted frame, and a pixel's weight is its distance from that in **OkLCh** —
an ellipsoid flat to half its size and falling off smoothly to one and a half,
so there is no edge to find in a gradient. OkLCh because equal steps there look
equal; in HSV a dark blue and a bright one share a hue and nothing else about
them compares. Hue is discounted on colours with little chroma, since a grey's
hue means nothing. The points are defined on sRGB, so pixels in another working
space visit it and come back, and the pass runs straight after the mixer, at the
same point and for the same reason. "Show affected area" is a flag on the
render's copy of the document only, never serialised.

## Lightroom's answers, and the controls that did something else

`docs/CONTROL_AUDIT.md` measured every control against what it says it does
(FT-028). What it found, and the rule for fixing it — the photographer's, 21
September: where Lightroom has an answer, take it.

**The four tone regions are one curve.** Each region's gain depends on
luminance alone, so their product is a curve of it, and pulled down hard
enough near white it turned over: Highlights −100 rendered scene 0.52 → 1.0 as
178 → 146. `ToneCurve` works it out once per render over log2 luminance in
hundredths of a stop and holds it to a quarter of its slope from the bottom
up, so every tone below where it would have turned keeps exactly what the
sliders asked and the ones above stay in order. Per pixel it is a log2, a lerp
and an exp2 where it was five `powf`s. Auto solves its endpoints against the
same curve, so it cannot disagree with the render about what a slider does.

**Contrast and Blacks as Adobe describes them.** Lightroom's Contrast stretches
tones from the middle or presses them towards it, one the mirror of the other,
so −c is the reciprocal of +c: −100 halves the slope where it used to make it
zero. Blacks sets the black point — to the left the darkest tones clip
progressively (about the twentieth code at −100), to the right they rise up to
two stops without clipping — and reaches nothing above scene 0.1. Adobe
publishes no curves, and Lightroom was not to hand, so those three numbers are
chosen to give the described behaviour rather than fitted to Lightroom's
output; FT-003's machine is where they would be checked.

**Tint's sign.** The units were Adobe's all along — a hundred and fifty of tint
is 0.05 of CIE 1960 v, the DNG SDK's `kTintScale` — but + was greener, the
opposite of Adobe's. Every stored edit is in that sign, so `WhiteBalance` keeps
it and the sign is turned where a person or a file meets it: the two Tint
sliders and Lightroom's `crs:Tint` on import, which had been read the wrong way
round. No stored photograph renders differently.

**Luminance that left grey alone.** The mixer's table had one saturation row,
so a band's luminance scaled every pixel whose hue rounded into it, greys
included — Magenta −100 took middle grey from 118 to 1. Two rows now, grey and
full colour, the luminance fading between them; hue and saturation are the same
in both. Point Colour's luminance fades the same way below the chroma at which
its weight starts to count hue (−100 on an orange point had taken grey from 118
to 28).

**Calibration over the grey.** White was held by taking its drift back out of
every primary in proportion to brightness, which carried Red −100 into aqua
further than into red. The matrix now acts on what is over a pixel's grey —
`min(r, g, b)` taken off, leaving at most the two primaries the colour is made
of — so grey and white stay by construction and the complement is untouched.

**Radius in tenths.** One box blur at the rounded radius made 23 of Lightroom's
25 steps the same picture. The two whole-pixel blurs either side are mixed by
the fraction; a whole radius costs what it did, and below half a pixel on
screen the pass still does nothing (FT-021). On a soft edge a tenth is under
one eight-bit code, so the test measures at sixteen bits.

**The vignette's squaring** divided by the scale meant to keep the corner at √2
where it should multiply, so Roundness −100 left the corners at 0.84 of the way
out. And a grade is kept whenever anything moved, not only when it changes a
pixel, so a hue chosen before its saturation survives stepping away.

## What a mask carries

A mask is the same edit, faded in by a shape. Everything a photograph's
`Basic` holds, a mask's holds too, and `apply_masks` runs the same passes
over a copy of the pixels before blending it back by the mask's own weight.

**Temperature and Tint inside a mask are relative to the photograph's white
balance, not absolute Kelvin.** This is the one place a reader will guess
wrong. By the time pixels reach a mask they have already been balanced — in
camera space, before the colour matrix, which is the only place white balance
can honestly be applied — so their white is neutral by construction. A mask
asking for 5 000 K would be asking about a photograph that no longer exists.
What it asks for is the difference: `temperature: -5.0` means five Kelvin
cooler than whatever the photograph was set to, and the gains that produces
are computed in the working space from the two white points' ratio,
normalised on green so the mask does not also change how bright it is.

**A mask rests where a photograph does not.** Two of `Basic::default`'s
values are deliberately not zero: `sharpen: 25` and `denoise_colour: 25`,
because a demosaic produces both softness and colour speckle and every
photograph starts by undoing its own decoding. A mask is applied *on top of* a
photograph that has already had both, so a mask carrying them would sharpen
its own area twice for nobody's asking. `Basic::local` is where a mask rests,
`Mask::is_idle` compares against it, and `#[serde(default = "Basic::local")]`
makes a stack written before the detail passes reached masks read back as a
mask that asks for neither — which is exactly what it did.

The four detail passes run inside a mask too, on its own copy and in the
photograph's own order: smooth before sharpen, or the sharpening sharpens what
the smoothing missed; defringe after sharpening, because sharpening an edge
sharpens the fringe on it; moiré last, because it asks about colour that
wobbles where brightness does not and both passes above change one of those.
On the mask's copy rather than the frame's, so softening a sky stops at the
skyline instead of smearing the roof into it — which is the whole reason to
ask for it locally. Five of them reach the panel inside a mask: Sharpening,
Noise reduction, Colour noise, Defringe and Moiré. Radius and Masking shape a
sharpening the mask takes from the photograph, Detail and Contrast shape its
noise reduction, and a lens correction is a fact about the whole frame.

The four frame-local fields — HDR, Clarity, Texture and Dehaze — work inside
a mask as well, and `tiles_cleanly` now asks the masks as well as the
document. All four measure the frame to decide what to do with it: the
airlight in the whole photograph, the range the local tone mapping has to
compress. On a tile they would measure the tile, which is a different answer
on every tile and a different one again on the export. So a stack that
carries them anywhere — in the document or in a mask — is rendered whole.

## The machine this is for

A few-year-old laptop, and it has to feel good on one. That is the
photographer's own constraint — "naast eenvoud wil ik ook graag dat zoveel
mogelijk mensen het kunnen gebruiken" — and it decides arguments that would
otherwise be a matter of taste.

What it rules out is not expensive work. It is expensive work **in front of
the person waiting**. The distinction matters, and a measurement pass on 20
September settled which is which:

- **A saturated CPU is not a freeze.** A probe doing a compositor's job — wake
  every 16.7 ms, 2 ms of work — missed zero frames out of 840 with rayon on
  all eight cores, and zero on two cores. Linux serves a short periodic task
  ahead of eight CPU-bound workers perfectly well. Numa may use the whole
  machine.
- **The main thread is the scarce thing.** `render_current` runs in a
  frame-clock callback, so anything it does there is time the window is not
  drawing. Before PERF-006 was widened, one tick of Temperature was 55-65 ms
  on eight cores, 104 on four and 269 on two — a slider that made the window
  draw four times a second on the machine this is meant for.
- **Memory is what freezes the whole computer.** One 40 MP photograph taken to
  1:1 peaked at 4.58 GB when that was first measured. On this machine, with
  30 GB, that is invisible; on a 16 GB laptop with a browser open it is a
  system-wide stall, and it is the only mechanism measured that stops more than
  Numa itself.

  **Re-measured 20 September, after PERF-009, PERF-011 and PERF-014: it is
  1.85 GB.** The library alone is 452 MB; opening a 40 MP frame takes it to
  1.72 GB and 1:1 to 1.85 GB. A full-size export peaks at 1.30 GB.

  What is left is not ours to take. The decode alone peaks at **1.27 GB** and
  settles at 497 MB, and that peak is inside rawler's Markesteijn — the mosaic
  floats, the three-channel output and the demosaic's own tile buffers, all
  alive at once. PERF-011 took what could be taken around it. The rest needs a
  fork of the dependency, and 1.85 GB on a 16 GB laptop is no longer the thing
  that stalls it.

So the order to think in: do not do the work at all if something already has
the answer; do it off the main thread if it must be done; do it at a lower
priority if nobody is waiting for it; and count the buffers, because a
gigabyte held is worse than a second spent.

The three numbers to keep a change honest are the ones above: a tick of the
thing being dragged, the longest block of the main thread, and peak RSS. Two
of them come out of the measuring hooks in `window/measure.rs`; the third
wants `/usr/bin/time -v`.

**And a fourth thing, found on 20 September: look for the part that does not
scale.** Two of the routes on that list turned out to be nothing, and the one
that paid did so because a single step was serial while the others were not.

*The export.* Per frame on a 24 MP file, decode is 700 ms and 5.7× faster on
sixteen threads than on one; develop is 456 ms and 6.4×; writing the JPEG is
340 ms and **1.0×** — 345 ms on one thread, 347 ms on sixteen. A fifth of
every frame was one core working and fifteen idle. Overlapping that encode
with the next frame's decode took a shoot from 1.50 s a frame to 1.22 s
(PERF-013), and no amount of making the parallel parts faster would have
found it.

*The startup.* The 0.78 s that was quoted is the time until the main loop
goes idle, not a wait: the window is on screen at 342 ms with 2 319
photographs and 205 ms with 440. What is left in front of the person is 44 ms
of GTK, 68 ms to the library page, and 134 ms building cards — and only the
last of those grows with the shoot.

*The grid's sweep*, suspected of being O(all cards), is 442 µs for 2 319 of
them: 190 ns each for the 2 298 that are not on screen, on a debounced
callback. Real, lineair, and nothing.

*The AI denoise* was the largest single number on the list — 805 tiles at
roughly half a second — and it has no knob on the processor. Tried both:

| tile | per tile | per megapixel | a 40 MP frame |
|---|---|---|---|
| 128 | 127 ms | 8 s | 554 s |
| 192 | 238 ms | 6 s | 385 s |
| **256 (shipped)** | **500 ms** | **8 s** | **403 s** |
| 320 | 792 ms | 8 s | 385 s |
| 512 | 2 s | 9 s | 463 s |
| 640 | 4 s | 10 s | 478 s |

Flat, and the best of them is 4 % off what is shipped — less than the spread
between runs. And running tiles side by side does nothing at all: twelve tiles
take 5.93 s one at a time, 5.94 s two at once, 5.93 s four at once. That is
worth knowing for its own sake, because SCUNet uses only **4.4 of 16 cores**
(205 s of user time against 46 s of wall clock over the whole sweep) — the
eleven idle ones are not available to it, and ONNX Runtime will not hand them
out by being asked twice.

So on the processor the denoise costs what it costs. **On the GPU the tile is
a lever**, and it is the one place the shape of the answer changes:

| tile | processor, 40 MP | WebGPU, 40 MP |
|---|---|---|
| 256 (shipped) | 403 s | 190 s |
| 320 | 385 s | 176 s |
| 512 | 463 s | 159 s |

Two things follow. The card is worth **2.1×** on this model, which is the
weakest of the three it is used for — the masks are 5.9× and the SAM encoder
4.2×, because SCUNet is heavier on memory than on arithmetic and a tile is
never big enough to hide the transfer. And the two devices want different
tiles: 512 is 16 % better on the card and 15 % worse on the processor, while
**320 is better than 256 on both** (−4.5 % and −7.5 %).

**And the card gives the same photograph, which is the part that had to be
checked before any of this mattered.** The same tile through SCUNet on both
devices: 180 046 of 196 608 values differ, and the largest difference is
**1.4e-6** — 0.0003 of an 8-bit code value. The denoised frame is kept at
twelve bits, and at that precision 174 of 196 608 codes differ, always by one.
0.09 %, in the last bit of a twelve-bit value. So turning the plugin on is a
speed decision and not a picture decision, which is what makes it safe to
offer as a switch at all.

That makes 320 the only change worth making, because a tile size that depends
on the device would mean the same photograph denoises differently depending on
whether the plugin is installed, and that is the inconsistency `RenderInputs`
exists to prevent. It is not made here: changing the tile moves every seam, so
it is a pixel change to a feature the photographer turns on deliberately, and
it wants his word rather than an afternoon's.

*The cold start* was the last open question and it is answered: the loop that
reads 2 319 thumbnails' dimensions takes **52 ms cold and 0.7 ms warm**, with
the whole thumbnail cache pushed out of the page cache on purpose
(`posix_fadvise(DONTNEED)` over all 40 093 files). The comment beside it
quotes 330 ms, and that is the single-threaded figure it is there to explain —
the `par_iter` is what turns a third of a second into fifty milliseconds, and
a cold start is the one a photographer gets in the morning.

The lesson for the next pass is the shape of the question. "Where does the
time go" found two non-problems; "which step does not get faster when the
machine does" found the one that did.

## What resolution a mask is actually made at

Three model families make masks, and every one of them answers at a size that
is not the photograph's. What a mask edge looks like at 1:1 is decided by the
coarsest link in that chain, not by the model:

| step | answers at | to reach a 40 MP frame |
|---|---|---|
| semantic (`segment.rs`, EfficientViT) | 1024 | 7.5× |
| **promptable (`sam.rs`, the decoder)** | **256** | **30×** |
| matting (`matte.rs`, IS-Net) | 1024, on a crop | depends on the crop |
| the raster masks live on (`MASK_RASTER`) | 2048 | 3.8× |

SAM's decoder answering at a quarter of its encoder's edge is the family's
own convention, not a choice made here — but it means a mask built by clicking
starts life 30 times smaller than the frame it will be applied to. The matting
pass is what rescues it (`search_edge` hands the coarse alpha to
`matte::refine`), which is why that pass exists and why its own edge quality
is the thing worth measuring.

`MASK_RASTER` is a choice made here, and it is the last multiplication before
the render. Nothing above it can produce an edge finer than 2048 across.

**And how often the subject is found at all, measured 20 September over 30
photographs spread across the whole library: 23.** Seven got no mask — the
model reached no confident opinion or called the whole frame the subject — and
three of the twenty-three came back covering 45 to 53 % of the frame, which is
the same failure wearing a different face.

So roughly a quarter of photographs get nothing, and that is a **detection**
failure rather than an edge one. The two are worth keeping apart: a better
edge model does nothing for a photograph the model never saw a subject in, and
the shootout above measured only the edge, on a bird it already found.
`tests/matte_survey.rs` is the rig, and the seven it missed are the test set a
detection question should be asked on.

**The failure the photographer actually sees is a third kind: the mask runs on
into whatever touches the subject.** Reviewed over 72 panels on 20 September:
the kitesurfers plus a piece of the wave behind them; a person fused with the
lifeguard hut she stands in front of; a woman plus a length of fence. The
subject is found and the edge is not stepped — the mask simply does not stop
where the object does.

That is what a salient-object model is: IS-Net answers "what stands out here",
and a person in front of a hut is one salient blob. It has no notion of where
one object ends. No matting model fixes this, which is why the BiRefNet
shootout could not have found it, and why `matte::subject` asking one model
about the whole frame is the shape of the problem. The model that does know a
person from a building is the semantic one already installed beside it.

So a complaint about a stair-stepped edge is a question about this table
before it is a question about models. A better model answering at the same
size cannot fix the multiplication that comes after it.

## The render pipeline

**A render is a function of what it is handed.** Two exports of the same
document five minutes apart should never differ — and until `RenderInputs`
they could, because the pipeline reached for two things on disk while it ran:
the kept denoised frame, which the background job might have finished writing
in between, and the camera profile the document names, which was looked up
behind a memo. Neither is a property of the document, and both changed the
picture. That is a correctness bug rather than a question of layering, and it
is the reason the renderer is now given `RenderInputs` by its caller and opens
no file itself. The layering follows from it: a crate that takes everything as
an argument is one a test can run in CI with no disk, no cache and no model
folder, and one whose tile, proxy and export paths cannot disagree by accident.

`Default` is "nothing handed in", which renders exactly as an empty cache and
an unnamed profile did. The canvas may use it — it shows a proxy while the job
runs. An export may not: it calls `io::denoised::ensure` first and then builds
its inputs, because an export that quietly shipped the undenoised picture
would be the same bug wearing the fix's clothes.

```
RAW file
  └─ decode (io::raw::decode_with) ─────> linear RGB, f32, camera-native
       │  rawler's develop without WhiteBalance, Calibrate and SRgb,
       │    or Markesteijn for X-Trans at full size        (IO-013)
       │    ──> a 3x3 median of the channel ratios, X-Trans only  (IO-013)
       │  × baseline exposure                              (RENDER-004)
       │  lens falloff ──> lens distortion and lateral colour  (OPTICS-001/002/005)
       │  × the camera's own exposure for this scene       (RENDER-008)
       │  the sensor's ceiling noted ──> EXIF orientation   (RENDER-011, TOOL-003)
       │
       ├─ colour stage (render::to_working_space; cached, redone only when the
       │    white balance, the profile or the AI denoise amount moves)
       │    AI denoise's kept answer, blended in camera space   (DETAIL-003)
       │    white balance in camera space, blown pixels held neutral
       │      ──> camera matrix, or the profile's matrix, hue/saturation map and look
       │      ──> working colour space                          (RENDER-009)
       │
       ├─ geometry (render::geometry_of)
       │    manual lens corrections                   (OPTICS-004)
       │    flip ──> quarter turns                    (GEOM-004, TOOL-003)
       │    crop, straighten and perspective, one pass (TOOL-002, GEOM-001)
       │
       └─ pixel stack (render::apply_pixels)
            spot healing ──> face retouching          (RETOUCH-001/002, FACE-001/002)
            noise reduction ──> sharpening            (DETAIL-002, DETAIL-001)
            defringe ──> moiré                        (OPTICS-003, DETAIL-006)
            calibration ──> dehaze                    (ADJ-008, FILTER-006)
            exposure, contrast, the four tone regions, vibrance, saturation
            colour mixer ──> point colour             (ADJ-005, ADJ-006)
            local tone mapping                        (HDR, clarity, texture)
            masks                                     (each: the same sliders, faded in)
            colour grading                            (ADJ-007)
            vignette ──> grain                        (FILTER-003, FILTER-007)
            base tone curve ──> your tone curve ──> red, green, blue curves
              ──> output colour space ──> 8-bit
```

Non-RAW files enter through `decode_linear_any`, which undoes sRGB's encoding
and joins the same path at the colour stage with no profile.

**Who owns the frame, and why the renderer asks.** Each of the two boxes above
that writes pixels — the colour stage and the pixel stack — needs a buffer it
may scribble on, and each used to get one the same way: clone the frame it was
handed. On a 40 MP Fuji that is 456 MB apiece, and a full-resolution develop
therefore held three copies of the same photograph at once before the 8-bit
result was even allocated. Measured with `VmHWM`, the render stage of one
develop peaked at 2.25 GB.

Neither function could do better, because neither knew anything about its
caller. Some callers genuinely need the frame afterwards: the editor keeps
`photo.working` and `photo.full_working` between slider ticks, which is the
whole basis of `PERF-006` — a drag re-runs only the stack. Others never look
at it again: an export decodes a frame, develops it once and drops it.

So `to_working_space`, `apply_stack`, `apply_pixels` and `develop` take
`impl Into<Cow<'_, LinearImage>>`. Passing `&image` is the old behaviour to the
byte, and every call site that wants it kept compiling unchanged. Passing
`image` hands the frame over, and the pass writes into the buffer it was given
instead of a copy of it (`std::mem::take` on the `Vec`, so the frame's width,
height, clip and film simulation stay readable beside it). `apply_stack` also
drops a handed-over frame the moment the geometry pass has replaced it, rather
than at the end of the function.

Export and thumbnails went from 2.25 GB to 1.37 GB that way, and the editor's
full-resolution colour stage from 0.93 GB to 0.48 GB. The editor's stack did
not move, and should not: it is the caller that keeps its frame on purpose.

The two `From` impls that make `&image` and `image` both convert to a `Cow`
live next to `LinearImage` in `numa-core`; `std` writes them out for `str` and
`[T]` but has no blanket one for an arbitrary `T`.

**Nothing the decode has finished with lives past the line that finished with
it.** PERF-009 left a bigger number behind: `decode_with` peaked at 2.02 GB to
produce a 456 MB frame, four times its own answer. Measured stage by stage with
`VmHWM` — on an X-T5 RAF, where every full frame is 456 MB — the four multiples
were not four passes each needing a buffer. They were one pass needing a buffer
and three bindings that had gone quiet but not out of scope:

| after | RSS, before | RSS, after |
|---|---|---|
| the demosaic returns | 651 MB | 490 MB |
| the frame is flattened to interleaved f32 | 1106 MB | 490 MB |
| the geometry correction | 1562 MB | 490 MB |
| the frame is turned upright | 2023 MB | 495 MB |

Rust drops a binding at the end of its scope, and `decode_with` is one long
scope. So the mosaic and the mapped file stayed on the heap through every pass
below them; `Color2D::flatten` copied the demosaic's output into a second
buffer and left the first alive; shadowing `image` with the straightened frame
kept the bent one; and `oriented` took `&self`, so the sideways frame outlived
the upright one it had produced. Each is 456 MB of a photograph nothing was
ever going to read again.

The fix is scoping, not cleverness. The rawler objects live in a block that
ends where the mosaic is no longer needed. `into_flatten` consumes what
`flatten` copied. The geometry arm drops its source the moment the pass
returns. `into_oriented` takes the frame by value — and hands it straight back
untouched when the camera was held level, which is most photographs and used to
be a 456 MB clone of something identical.

Two places were also holding a second copy while making the first: rawler's
`apply_scaling` has already turned the mosaic into floats, so `markesteijn_develop`
takes that buffer rather than asking `as_f32` for a copy of it; and the default
crop is only ever a shift towards the origin, so the rows move down inside the
buffer they are in instead of being gathered into a new one.

A decode now peaks at 1.27 GB, and every pass after the demosaic runs flat at
one frame. What is left is rawler's own demosaic, which holds the mosaic, a
cropped copy of it, a per-pixel bounds table and the output at once; that is a
1.27 GB floor nothing this side of the crate boundary can move.

**How far a fingerprint can be trusted.** `tests/reference_render.rs` renders
every reference frame on both decode routes, and it is what every claim of "no
photograph changed" in this file rests on. One thing has to be known about it
before relying on one: **the render is not bit-stable between processes, and a
hash comparison therefore cannot prove that nothing changed.**

Measured on 20 September, ten runs of the same binary on the same frame: eight
gave one hash and two gave another on the draft route, seven and three on the
Markesteijn route. Comparing the renditions byte for byte rather than by hash
says what the difference actually is:

    FRAME DSCF9580.RAF  moved 1876 bytes, worst 1
    BEST  DSCF9580.RAF  moved    7 bytes, worst 1
    FRAME DSCF9580.RAF  moved 1863 bytes, worst 1
    BEST  DSCF9580.RAF  moved  936 bytes, worst 1
    ...

Between 7 and 3 400 bytes of 119 443 968, and **never more than one code value
of 255**. In the floating-point pipeline it is at most 122 ULP and 3.6e-6
absolute. It is a rounding difference, not a difference in the photograph.

Three things are known about where it comes from. The decode is exactly
reproducible — `decode_linear` and `decode_linear_best` both hash the same on
every run. `RAYON_NUM_THREADS=1` and `=2` are exactly reproducible end to end;
from three threads up it wobbles, which is the signature of work-stealing
changing how buffers and iterations line up rather than of a race writing the
wrong pixel. And the first step whose output moves is `detail::denoise_colour`,
whose `blur` is itself reproducible on synthetic data — so the wobble is in how
the same arithmetic gets laid out, not in the algorithm.

Two earlier statements in this file were wrong and are withdrawn. An early
report that `main` gave four hashes in six runs was recorded here as "has not
reproduced": it reproduces, and the fourteen matching runs that were used to
dismiss it were luck. And the wobble was recorded as invisible after the
eight-bit quantisation and absent from the bilinear draft: it survives the
quantisation, and the draft route moves *more* bytes than the Markesteijn one.

**So "byte-identical" means zero code values moved, not one hash.** Set
`REF_DIR` and the harness writes each rendition the first time and compares it
byte for byte afterwards. The floor to judge against is the one above: a couple
of thousand bytes at worst 1. A real change is nothing like it — A9's median
moved every Fuji Markesteijn rendition by millions of bytes. The two are three
orders of magnitude apart, so the test still answers the question it is for; it
just has to be asked in bytes.

**What it cannot see.** rawler's X-Trans Markesteijn splits the frame into
64-pixel tiles and writes them in parallel through a shared raw pointer, on the
stated assumption that two tiles never write the same pixel (`markesteijn.rs`,
`SharedColor2D` and the `unsafe` write inside `process_tile`). Measured, two
runs of the same commit differ in about four channels out of 119 million, by
around 4e-6 — so the assumption is very nearly true and not quite. That is a
second, smaller source on top of the one above, and the same conclusion applies
to it: worth knowing about, not worth acting on until something measures it
larger.

**What gets rendered, and at what size.** Interactive editing does *not* simply
run on a proxy. It renders the region on screen at the resolution the screen can
show it, which below about 31 % magnification happens to be the proxy — that is
where a 2400-pixel proxy stops having a pixel per screen pixel on a 7728-pixel
frame. Above it the full-resolution decode is cut to the visible region and
reduced to fit, which is what keeps a slider at 300 % costing 5 ms rather than
243 (`PERF-002`).

The operation stack is the same code on every path, so a preview and an export
differ only in how many pixels went through it (`PERF-004`). Two things are
scaled to match: the detail radii, which are stated in full-resolution pixels
and shrink with the preview, and everything placed in the frame — mask
coordinates, the vignette, the grain — which is in fractions of the frame and
never pixels at all. A region that cannot be cut out in advance falls back to
rendering the frame entire. `tile_in_source` declines anything that is not a
plain rectangle map: a quarter turn, a flip, a straighten angle, a perspective
correction or a manual lens correction. `tiles_cleanly` declines what measures
the whole frame: local tone mapping and texture, dehaze, and face retouching,
whose radii are a fraction of the face. `spots_within` declines a healed spot
whose patch reaches past the cut.

---

## The build directory

`target` reached 21 GB once and 16 GB two days after it was cleaned. A fresh
build is 2 GB; the rest is what cargo keeps and never deletes: every edit to
`Cargo.toml` rehashes the tree, so the old copies of our binaries stay (numa
was there eight times and each of the five test binaries five times, at about
100 MB a copy), each of those variants keeps its own incremental
cache (82 of them, 3.8 GB), a renamed package leaves its old name behind for
good, and a release build is a second full tree. Run `dev/clean.sh`: it drops
the incremental caches, our own crate's artifacts and the release profile, keeps
the compiled dependencies, and the next `cargo run` is a ten-second rebuild.
What it leaves is stale copies of third-party crates under old hashes, about
half a gigabyte a day; when the tree is still large after the script, `cargo
clean` takes it back to 2 GB for a two-minute rebuild.

**Debug builds optimise Numa's own code** (`opt-level = 2` in `[profile.dev]`).
The dependencies were already at 3, but every pixel loop in this crate was not:
`./dev/run.sh` opened a 40 MP RAF in 4.2 s against a release build's 1.1 s, and
1.5 s after the change, with overflow checks and debug assertions still on. It
costs rebuild time — about 5 s after a change to the UI instead of 1, and 9.5 s
after one in the library crate instead of 2 — which is the trade a build that is
used to judge photographs is worth making.

## Rows at the photograph's shape

LIB-015 was asked for as masonry — columns of equal width, Pinterest's wall —
and is justified rows instead. A column wall places each photograph in
whichever column is shortest, so the next frame of a burst can land to the
left of and above the one before it; rows read in the order the filter put
them, which is the order the filmstrip, the next and previous keys and a
shift-click run already use. Rows also keep every card's top edge rising with
its place in the list, which is what lets PERF-005 find the visible band by
bisection rather than by measuring 2319 cards.

`GtkFlowBox` cannot lay out either: it lines children up in columns across
lines, and it cannot be subclassed. So `ui::justified` is its own container and
does the flow box's work for itself — click, Ctrl, Shift and rubber-band
selection with scrolling at the edge, arrows, Page Up/Down, Home/End, Ctrl+A,
Enter and double-click. The rows were a second view beside the square grid at
first; the grid has since gone, and the rows are the library.

The catalog does not know a photograph's shape, and decoding a RAW to find out
is what the thumbnail cache exists to avoid. The cache knows: its thumbnails
are written upright from the preview the library shows, and the JPEG frame
header says their size in the first few hundred bytes. On the 2319-photograph
trip that is 2 ms for all of them when the files are in memory, and 330 ms in
one thread when they are not (evicted with `posix_fadvise`), 45 ms across
threads. A photograph with no thumbnail yet takes 3:2 and is corrected when its
thumbnail lands; the view is held on the card at its top edge while the rows
below that change, which on an emptied cache kept it within a pixel with
thumbnails landing all around.

Measured in a debug build, 2319 photographs, from the start of the reload,
against the square grid the rows replaced:

| | built | first frame painted |
|---|---|---|
| grid, before | 53 ms | 350 ms |
| grid, after | 53 ms | 349 ms |
| rows | 52 ms | 308 ms |

Breaking 2319 cards into rows is 30 µs at any width; a resize that reflows
them is 10 ms, nearly all of it allocating every card, as the flow box does.

## Thumbnails

`io::thumbs` keeps each thumbnail as a small JPEG keyed by a hash of the cache
version, the path, the modification time and the edge — so re-saving a
photograph invalidates it for free. Decoding a RAF's embedded JPEG is about
56 ms, which is fine once and far too much for every card on every launch.

**Where they are kept** (`IO-016`). The grid's default size, 320 pixels, is
kept in the library's own `.numa/thumbs/`, keyed by the path *within* the
library, which is what survives the folder being moved: a library carried to
another drive or computer shows at once rather than decoding every frame again,
for the same reason the catalog moved there. The library root is found as the
nearest folder above the photograph holding a `.numa`, remembered per
directory. Larger sizes stay in the user's cache, which is also read for
thumbnails made before this and written to when the library cannot be — a
read-only share — so the library does not grow by a megabyte a frame per size.
Neither place is pruned when a file changes or goes.

**How big** (`LIB-004`). A fixed 320-pixel thumbnail was stretched at every row
height on a HiDPI display. The edge now follows the row height, the stretch a
row takes to fill the width, a landscape frame's aspect and the display scale,
rounded up to one of 320, 640, 960, 1280 and 1920 — steps, so a slider moved one
mark does not throw away every cached thumbnail. Cards in view fetch the
sharper size when the rows grow. The bands of cards kept ready and kept at all
(`PERF-005`: sixty and 240 either side of the visible band, at 320 pixels) shrink
by the square of the edge, so memory stays about where it was: at 1920 it is 36
cards.

**Saying how far it has got** (`START-012`). A new library's first minutes are
thumbnails being made, and a wall of blank cards looked broken. The thumbnail
queue counts each run — asked for, done, and whether anything had to be decoded
rather than read — and once a run that decodes has lasted past 400 ms, the same
standing toast Analyse uses says "Making thumbnails — 38 of 42". Counted in runs,
because only what is on screen is made and scrolling asks for more; and there is
no Stop, because the cards on screen would only stay blank.

**Showing the edit** (`LIB-022`). A photograph the photographer has worked on
shows his render in the grid and the filmstrip, not the camera's: seeing the
edit is what says he has been here, and the original is the less interesting of
the two by then. The camera's embedded preview cannot be adjusted into it — the
stack is defined against scene-referred camera-native pixels and a preview is
already developed — so `thumbs::edited` decodes the raw as opening it would,
runs the stack at 1024 pixels and shrinks the result. Measured in a release
build over the seven edited frames of a Zwitserland shoot: 0.50 to 1.78 s each,
median 0.65 s, against 17–45 ms for the camera's preview — and 0.3–0.5 ms to
read one back once it is kept.

That cost shapes the rest of it. The entry is keyed on the stored edit JSON
along with the path, mtime and edge, so a slider moved one step is a different
picture rather than a stale one, and a photograph with no edits keeps exactly
the name its thumbnail had before any of this — no render, no entry, no cost.
The stack is read from the catalog when the card is asked for rather than when
it was built, so the picture is of the edit as it now stands. Two jobs are
queued for such a card, the camera's thumbnail first: it fills only a picture
that is still empty, so a card is never blank while a render is running and the
render cannot be painted over by the original it replaces. And renders get a
lane of their own in the loader — one at a time, since a decode is half a
gigabyte for a 40 MP frame and eight of them would hold every worker while the
rest of the screen waited on memory it did not need. Saving an edit marks that
card so the next sweep asks again; the loupe is left alone, since a render at
its size is a decode in the middle of stepping through a shoot. The renderer is
handed the profile the document names, as the canvas and the export are, but
not the kept denoised frame: that is a full-size buffer to read from disk, as
dear as the decode, and what it changes cannot be seen in 320 pixels.

## A catalog per library

Each library's catalog is in `.numa/catalog.db` inside the library's folder
(IO-014); the application's own catalog, in its data directory, holds only the
list of libraries and the settings. It was one file in the data directory,
which put every rating and edit on the system drive — the one that filled up —
and none of them where the photographs were, so a folder copied to another
drive arrived without any of its work.

Photograph ids are still one integer to the rest of the application: the
library's id in the upper 32 bits and the row in that library's catalog in the
lower. The grid, the filmstrip and the editor key on that integer, and two
catalogs each counting from one would otherwise hand out the same id twice.

A library catalog uses a rollback journal, not WAL, because it sits in a folder
that backup and sync tools copy without knowing what it is: one file, with a
journal only for the length of a write, rather than three files for as long as
the application is open. Its paths are relative, so the folder can move. A copy
is taken with `VACUUM INTO` once a week — a consistent copy even of a catalog
being written — and four are kept.

Measured on the real catalog (two libraries, 3908 photographs, 69 edit stacks),
split into two library folders: 19 ms, every count the same afterwards, and
every edit stack loading.

**History and snapshots live there too.** A photograph's undo history
(`DOC-004`) is saved with its edits in a `history` table of the library's
catalog: the states as one JSON array of edit stacks, oldest first, and which
of them is on screen. A table of its own because the photographs' rows are read
for every grid and this is read only when one is opened. A hundred steps are
kept, the oldest going first. Reopening carries the history on rather than
starting at "Opened" with a crop already baked in: an edit changed in the grid
meanwhile — by a paste or a preset — becomes a step of its own on top, and a
photograph edited before histories were kept starts at the untouched original,
so there is always a way back to it. Snapshots (`DOC-005`) are named copies of
the same edit stack `edits` holds, in a `snapshots` table keyed by photograph
and name, so a name already taken is refused by the key itself and the rows go
with the photograph. Putting one back is an ordinary history step named after
the snapshot, which undo takes off again; the step's name is kept for the
session only, and reopened it is named by what it changed, like any other.

**Renaming a library renames its folder** (`LIB-019`), `.numa` and all, since
that is what a photographer means by the name. Only within the same parent,
which makes it one atomic `rename`: a name with a separator would be a move, and
a move across drives is a copy that can stop halfway. A name already taken on
disk is refused rather than letting `rename` replace an empty folder silently,
and if the catalog cannot be updated afterwards the folder is renamed back, so
the list of libraries never names a folder that is not there.

## When a photograph was taken

`IO-005`. `Sort::Captured` read the file's modified time, on the reasoning that
a camera writes it at the shutter — true of a card straight out of one, and
wrong for every archive that has ever been copied. `cp` without `-p` stamps the
day of the copy, and a copy that walks a folder in parallel stamps it in no
particular order. A real 11 509-frame library carried the afternoon it was
moved on every folder, and one trip's files ran *backwards* against the
shutter: a minute apart on disk and a week apart in life.

The grid's order was the smaller half of the damage. `analysed()` hands CULL-002
its frames in that same order and it cuts bursts out of the seams, so a shuffled
archive produced runs that were never one moment, and a best-of-burst chosen
from them — a wrong label on a card with nothing on screen to explain it.

So the capture time is a column of its own beside the file's date, and
everything that orders on time takes `COALESCE(taken, mtime)`: the grid's
capture sort, the tie-break that the rating, sharpness and suggestion sorts
fall back on, and the queries that hand the frames to Analyse and to the
grouping. Sorting by name is the one that does not, since it never asked.

Both dates are kept because either can be the only one there is: a walk of the
folder always has the file's date, and a photograph with no EXIF — a scan of a
negative, an export that dropped its tags — has no other. So the fallback is to
the file's date rather than to the beginning of time, which is where a null
would have sorted it. A library catalogued before this gets the column by
migration, null on every row, and the next scan fills them in.

**Three ways to read one tag** (`io::exif`). The container's own EXIF block
first, for JPEG and HEIF — Fujifilm's HIF is ordinary HEIF, which is most of
this archive once the RAFs are counted out, and rawler cannot read any of these
formats. Then the RAF's embedded TIFF, walked by hand at the offsets
`raw::lens_model` already uses for the lens: the main TIFF structure twelve
bytes past the pointer at offset 84, with the Exif IFD hanging off it, and only
the tag differs. Then rawler's decoder, for every other RAW and for the RAF the
shortcut could not read, which makes a file the fast path misses slow rather
than undated.

The middle path is what makes the whole thing possible. rawler maps the file
with `populate` and `WillNeed`, so reading one tag out of a 50 MB RAF first
faults in all 50 MB: over the same twenty files that is 33 ms a frame against
56 µs for the hand-walk, and the archive's 8538 RAFs go by in 1.7 seconds where
the decoder would have spent five minutes reading 400 GB to find 8538
timestamps. That is the difference between reading the date inside the scan
loop, beside the `stat` that is happening anyway, and building a second
background pass around it with its own work list and its own progress bar.
Measured on a real library: 4653 files scanned in 945 ms, every one of them
with a capture time. `tests/bench.rs` keeps that measurement runnable, since
the design rests on it, and an ignored test beside the reader holds the
hand-walked path against the decoder over twenty RAFs — hand-walked offsets
into a layout nobody documents for us are only honest while the thing that does
understand the format agrees they are.

It is read in `catalog::scan`, which already runs off the main thread, and not
in `apply_scan`, which does not. EXIF records local clock time with no zone, so
there is nothing to convert from and it is read as if it were UTC: wrong by the
traveller's offset, and it does not matter, because every frame is wrong by its
own offset in the same direction and the only thing ever asked of the number is
which of two frames came first. A camera with no clock set writes
`0000:00:00 00:00:00`, and a date in year zero sorted ahead of everything real
is worse than no date at all, so that one is refused. The date arithmetic is
Howard Hinnant's `days_from_civil`, which is the whole of what this needs and
therefore the whole reason there is no date crate in the dependency list.

## Keeping up with the folder

**Rescanning without being asked** (`LIB-012`). The walk of a library's folder
is split from bringing the catalog in line with it: `catalog::scan` needs no
catalog and runs off the main thread — on a network share or a card of thousands
it is the slow half, and it used to hold the window still — and `apply_scan` is
the quick half, on the main thread, reporting what it added, updated and
removed. The open library is walked when the window becomes active again, which
is what follows copying a card in a file manager, and once a minute besides; the
grid is rebuilt only when the scan changed something, at the same scroll
position. The rule that makes a rescan safe is unchanged: rows are removed only
when the walk saw every directory, and never when it found nothing at all,
because an unmounted drive is an empty folder at its mount point. Tried under
Xvfb: a file copied into the folder was in the catalog a minute later with
nothing clicked.

**Dropping files on the library** (`APP-005`) copies them into its folder, off
the main thread, and then rescans. Only what the library would show is copied,
and a dropped folder arrives as a folder. Nothing already there is overwritten.
Each file is written under a `.part` name and renamed when whole, so a scan
running meanwhile never records half a photograph, and keeps its modification
time, because the grid sorts by it when a camera wrote no capture date. A
library dropped into itself is skipped, since it would copy forever.

**Opening a photograph from outside** (`IO-001`) — `numa photo.RAF`, Open With,
or Ctrl+O — goes through one function. Edits live in a library's catalog, so a
photograph is always opened as part of one: the innermost library that already
holds it, or, after asking, its folder added as a new one. The alternative, an
edit held only in memory, is work that is gone when the window closes with
nothing on screen to say so. Opened photographs and exported files are added to
the desktop's recent files.

**RAW and JPEG of the same frame** (`LIB-018`) stay two cards, and the filter
narrows the grid to the RAWs or to everything else. The filter asks
`raw::is_raw`, the same decoder-derived list the scanner uses, so a HEIF beside
its RAW is "everything else" like a JPEG.


## Folders, albums and every library at once

A library is a folder, and a folder of years holds shoots. The folder picker in
the filter bar (LIB-010) lists every folder under the library that holds a
photograph and every folder above one, indented by depth, and narrows the grid
by path prefix after the catalog query. It needs nothing in the catalog: the
paths are already there, relative to the library, and filtering a list the grid
is about to show costs less than a column that has to be kept in step.

Albums (LIB-008) span libraries, and that decided where they live. The names are
in the home catalog, the only one that sees every library. Membership is in each
library's own catalog, as a row per photograph, because nothing the home catalog
could point at is stable: SQLite hands a removed library's id to the next one
added, a drive remounts at another path, and a photo's row number can be given
out again after a rescan forgets it. Kept beside the photographs, membership
moves with the folder and goes when the file does. An album's key is made from
the clock rather than its name, so renaming one while a drive is unplugged does
not strand that drive's photographs.

"All libraries" (LIB-011), an album and a person are the same view to the grid:
`photos_everywhere` with the filter, each reachable library queried and the
results sorted as one query would. Photo ids carry their library in their upper
half, so rating, flags, opening, export and presets need nothing new there.
Actions that need one library — dropping files in — ask for one to be picked.

## Guided perspective

Guides (GEOM-003) are drawn on the crop tool's view of the corrected frame but
stored in the photograph's own coordinates, mapped back through `source_map`,
so they stay on the edge they were drawn along while the correction moves that
edge. The inverse map is the closed form the fitted crop uses, `u / (1 + a·u)`,
with the stretch and rotation undone, and a test holds it to `source_map` within
a hundredth of a pixel.

The solve is a coarse-to-fine grid search, eleven points an axis shrinking to
0.4 each pass, minimising the sum of sin² of each guide's error. Which unknowns
are free follows the guides: one guide moves only the keystone of its own axis,
or the angle when no keystone can square it (a line through the middle of the
frame is a tilt, not a keystone); two or more free both keystones they speak for
and the angle. When the guides pin down fewer numbers than are free — one
upright and one level — a small penalty on the size of the correction picks the
smallest that squares them. Synthetic keystoned lines come back within 0.2 on
the sliders and 0.02° on the angle, and solving from an already-corrected frame
gives the same answer as from an uncorrected one.

## Presets, and other applications' presets

**A preset is a file** (`LIB-006`): one JSON file in the data folder's
`presets/`, holding an `EditParts` checklist and the document it applies, and
only the parts it carries — no path, no spots, no faces. That makes export,
rename and delete the file manager's, and a subfolder is a group. Applying a
preset and pasting settings are one path with one checklist, so the two cannot
disagree about what "the colour settings" means. A name already taken is
refused rather than replaced.

**Translating Lightroom and Capture One** (`LIB-020`). Lightroom Classic's
`.lrtemplate` is a Lua table, Lightroom's `.xmp` is Camera Raw settings as XML
attributes and sequences, Capture One's `.costyle` is a list of keys and values,
and a `.costylepack` is a zip of those. None needs a real parser: each is read by
a scanner that knows only its own shape into one flat list of named settings,
and then mapped onto Numa's — tone, presence, the four effects above, the HSL
mixer, calibration, split toning or colour balance as grading, white balance,
detail and curves. Lightroom's parametric curve is laid over its point curve as
points, by an approximation fitted by eye to the shape Lightroom draws, not to
its output; tone from before process version 2012 is left alone, because its
scales were different. What has no counterpart — local adjustments, camera
profiles, a black-and-white mix, Capture One's advanced colour editor — is
counted and listed after the import, so the photographer knows what did not come
across. The result is close rather than the same: nothing here was fitted
against a Lightroom render. Each translated preset is grouped by the folder or
pack it came from. Of a 4383-file collection 3844 translate, in 0.2 s; the rest
are brush presets or hold only local, lens or reset settings.

**Why a picker and not a menu.** The presets were a submenu first. A GTK menu
builds every row at once, and 3500 imported presets held the window back for
over forty seconds. They are a searchable list now — the editor panel's last
tab, and the same list from the grid — rebuilt each time it is shown, which is
cheap because it is a list of file names.

## Faces and names

LIB-014 puts a name on a face by likeness. SFace turns an aligned face into 128
numbers of length one, and two faces are as alike as the dot product of theirs.
A name typed on one face is offered on another when the nearest face with that
name is at least **0.5** alike, and no other name is within 0.05 of it.

The threshold was read off the photographer's own library rather than a
benchmark: the 145 faces in 140 frames of a trip, embedded, grouped greedily,
and the groups looked at as contact sheets. No group held two people. One woman
came out as four groups — sunglasses, none, a hat — and the likeness *between*
those groups peaked at 0.62–0.84; between different people it never passed
0.38, nor between a person and the Buddha statues the detector also finds.
Measured on the 640-pixel preview the grid's measure pass decodes, and again at
2400: the same split, the gap a little wider at 2400.

Aligned the way OpenCV's `alignCrop` does it — a similarity transform fitted to
YuNet's five points onto the ArcFace template, 112 pixels square — because a
face put there any other way is one the network was not trained on.

Checked end to end in the application, on a copy of the catalog: named on one
frame, offered at 0.67 on another without sunglasses and at 0.55 on one with
them, and nothing offered on the men in the same trip.

**Who is in which photograph** is worked out when the grid is read, not
stored: Analyse keeps every face it finds as an embedding (`faces`), and a
person is every photograph with a face that would be offered their name. On the
same trip, one face named once brought in 59 photographs, and a contact sheet of
the faces that did it was the one woman 59 times — the ones missing were behind
sunglasses, which a second named face brings in.

## A suggested rating learned from the photographer's stars

`CULL-005`. The suggested rating was `CULL-004`'s rule — frame and face
sharpness, less for blown highlights, a nudge for the best of a burst — which is
one opinion of a keeper and nobody's in particular. Every star already in the
catalog is a label of exactly the taste the suggestion is for, so at the end of
Analyse `cull::learn` fits a ridge regression on them.

**The ends the rule divides by** are read off the library being analysed.
`CULL-004` stretched its five stars between fixed numbers — 0.15 and 1.15, the
fifth and ninety-fifth percentiles of sharpness in the 10 729-frame reference
library, rounded — and those are someone else's cameras, lenses and subjects. A
photographer whose frames hold less fine detail than that reference, which is
what wide apertures, fog, long exposures and a lot of smooth sky produce, had
every photograph they own scored a star or two low: a scale that is wrong about
a whole library at once. `cull::Scale::of` now takes the two percentiles of
what this Analyse actually measured, of the sharpness that is scored — the
face's where there is a face — so the ends describe the same quantity the score
divides. The number then means "against your own work", which is the only
comparison a cull is ever making. It is not a more objective number for it: a
percentile is a rank, so a library of uniformly soft frames still has a
ninety-fifth percentile and still hands out fives. Under fifty measured frames
a percentile is one photograph's opinion rather than a distribution, and ends
closer together than 0.10 would stretch a sliver over five stars; in both cases
the reference scale stands. The scale is per library, which is per trip only as
long as a library is one folder — a library holding a studio session and a week
outdoors lets the studio's detail pull the top end up and costs everything else
half a star — and the marker in `regroup_bursts` names grouping on the
containing folder as the way out on the day subfolders are used for separate
shoots.

The nudge on top of it — four tenths of a star for the pick of a run — is
chosen the same way. `best_of_each` ranks a burst on the face's sharpness where
there is one, because ranking on the frame picks the portrait with the crisp
background and the soft eyes, which is the one judgement the score exists to
avoid making.

The features are the eleven numbers Analyse already has: the rule's own score
(so the model starts from it and learns what to add), the logarithm of frame
and face sharpness (sharpness matters in ratios), blown highlights, the number
of faces and whether one was found, best of burst, and the frame's brightness —
squared as well, so a straight line can say "neither too dark nor too bright" —
contrast and colourfulness, which Analyse now measures (`cull::VERSION` 3, so a
library is measured again). There is no general image embedding in the pipeline
to put a head on, and adding a large model to get one is not what ranking
someone's own frames needs. Stars 1–5 are labels, a reject is 0, and an unrated
frame is left out rather than read as a zero. The ridge penalty, on standardised
features, keeps the several sharpness terms, which move together, from
cancelling each other out with large opposite weights on thirty points.

It has to earn its place. Below thirty ratings, or fewer than three different
verdicts, nothing is fitted. Past that a quarter of the rated *bursts* are set
aside — whole bursts, because fifteen near-copies of one moment would test the
model on what it had seen — and the learned score replaces the rule only when it
ranks those better, by Spearman correlation; the toast says which one the
suggestions are and both correlations. On a synthetic taste in two features the
model reached ρ 0.95 against the rule's −0.05, with a mean error of 0.26 stars
against a constant's 0.69; given stars that follow the rule, the rule was kept
(0.97 against 0.08). It has not yet been measured on a real library's ratings.

## Lining up a handheld bracket

`HDR-004`. The merge assumed a tripod, so a bracket shot by hand came out
doubled. Frames a stop or two apart cannot be compared by their values, and
edges found by a gradient move with exposure too. Ward's median threshold bitmap
sidesteps both: split each frame at its own median, and the bright half of one
exposure is the bright half of every other, because exposure is a monotone
change that leaves the ranking of pixels alone. Aligning two frames is then
counting the pixels on which their halves disagree. Pixels too close to the
median to call are fenced off, since noise decides their side differently in
each frame and a dark frame's shadows would otherwise vote against the right
offset.

`render::align` searches a six-level pyramid, each level refining the one below
by a pixel, which finds shifts of up to 63 pixels with nine comparisons a level.
The centre is tried first and only a strictly better neighbour replaces it, so
frames that already line up, and featureless ones, stay exactly where they are.
Every frame is aligned to the middle exposure. Offsets are whole pixels, so
nothing is resampled; beyond a moved frame's edge it simply does not vote in the
merge. Tests recover (7, −3) and (−12, 5) at a quarter and four times the
exposure, and the handheld merge matches the tripod one wherever every frame
saw. Three 40 MP frames align in 0.3 s and merge in 0.43 s in release. Rotation
is not searched: from a handheld bracket it is usually well under a pixel across
the frame.

## Inference runtime

Every model is an ONNX file, and every model module (`render::segment`,
`render::sam`, `render::matte`, `render::classify`, `render::ai_denoise`,
`cull::faces`, `cull::people`) reaches the runtime through one door:
`crates/numa-infer`. A `Model` is built from a path and handed arrays; nothing else
in the code knows what is behind it.

The models are EfficientViT-Seg B2 for the found masks (it replaced SegFormer-B0,
whose weights are under NVIDIA's non-commercial licence), SlimSAM for a click,
IS-Net for a subject's edge (it replaced MODNet), YuNet for finding faces, SFace
for recognising them, PP-ResNet50 for naming an animal, and SCUNet for AI
denoise. None is inside the application; see "What is delivered separately".

Behind the door is **ONNX Runtime**, through the `ort` crate. It replaced
**`tract`**, which is pure Rust. The decision was measured, on this machine,
same files:

| model | tract | ONNX Runtime |
|---|---|---|
| SegFormer-B0, 512 | 300 ms | 52 ms |
| EfficientViT-B2, 1024 | 1.7 s | 195 ms |
| IS-Net, 1024 | 2.5 s | 240 ms |
| BiRefNet Swin-tiny, 1024 | 20 s | 2.8 s |
| SCUNet denoise, 256 tile | would not load | 440 ms |

Both runtimes gave the same answers where it was checked: the subject mask on
DSCF2934 covers 3.50 % either way, Auto makes the same decision to the third
decimal on ten frames, SAM's click on the bird is 3.52 % against 3.5 %, and the
face counts on three frames match what the catalog recorded under `tract`.

**The locale.** Those comparisons ran in tests, where nobody calls
`setlocale`. The application does — GTK sets every category from the
environment at startup — and under a numeric locale with a decimal comma
(`nl_NL`) ONNX Runtime ran EfficientViT to a confident wrong answer: every cell
of every photograph floor. Same model, same 1600×2400 frame, mean 101.4 in both:
64 % tree, 13 % path, 6 % person in a test; 100 % floor in the window, and again
100 % floor in a test once it set `LC_NUMERIC=nl_NL.UTF-8` itself. So `main`
puts `LC_NUMERIC` back to `C` at startup, before any model loads. Any check of a
model's answers has to be made under the locale the application runs in.

**The GPU** (PERF-012) goes through the same door and nowhere else. ONNX
Runtime's WebGPU execution provider is not part of this build: it is a 15 MB
`libonnxruntime_providers_webgpu.so` that Microsoft publishes for linux x64,
downloaded beside the models as a 7 MB Python wheel and registered at run time
with `Environment::register_ep_library`. `ort`'s own `webgpu` feature was the
obvious road and was rejected after measuring it: it links Dawn as a
`DT_NEEDED`, so the binary will not start without the library, and on every
machine where WebGPU finds no adapter — which is most of them — the process
**segfaults at exit**, five runs out of five. The plugin does neither, and a
missing file is an ordinary `Err`.

Which models ask for it is a property of the model in `numa-infer`, a list of
file names, so no call site carries a flag and no call site can be wrong about
it. Measured on an RX 9070 XT through Mesa's RADV, against Numa's own session
settings on both sides (Level3, every thread), which is a fairer CPU baseline
than FT-009 used:

| model | CPU | WebGPU | |
|---|---|---|---|
| EfficientViT, found masks | 224 ms | 38 ms | 5.9× — on |
| SAM encoder, click to select | 671 ms | 160 ms | 4.2× — on |
| SCUNet, AI denoise | 484 ms/tile | 181 ms/tile | 2.7× — on |
| IS-Net, subject edge | 196 ms | 141 ms | 1.4× — CPU |
| YuNet, faces | 1.3 ms | 3.0 ms | slower — CPU |
| SFace, recognising | 2.9 ms | 21.7 ms | slower — CPU |
| PP-ResNet50, naming | 6.8 ms | 10.0 ms | slower — CPU |

A session built without `with_devices` stays on the processor and is not
disturbed by the plugin being registered: 206 ms before any GPU work, 207 ms
with the plugin registered and unused, 211 ms after a GPU session in the same
process. There is no cheap probe — `Environment::devices` lists a WebGPU
device whether or not an adapter is behind it — so the probe is to build the
session and fall back on error, about 41 ms when the answer is no.

**GTK's Vulkan renderer and the provider's own Vulkan instance** share the
process, which nothing had tested. They coexist: Numa under `GSK_RENDERER=vulkan`
on the real session, with the provider registered, ran the segmentation on the
card and quit with status 0 five times out of five. The `radv is not a
conformant Vulkan implementation` line appears twice in the log — once per
instance — and nothing else.

**One crash must not cost the application.** The provider is registered lazily,
and just before it is, `numa-infer` writes a marker beside the settings and
removes it as soon as a session has stood. A start that finds the marker still
there knows the last start died inside the graphics driver: it stays on the
processor, switches the preference off and says so. It is darktable's trick
with OpenCL, and it is the difference between a slower mask and a Numa that
will not open. Two Numas at once (only `NUMA_OPEN`, `NUMA_COMPARE` and
`NUMA_IMPORT` make the application non-unique) could have the second read the first's marker mid-registration and
switch itself off for nothing; that is the safe side of the mistake and one
click to undo.

**What it costs.** `ort` downloads a prebuilt ONNX Runtime at build time (the
`download-binaries` feature, on by default) and links it statically: `ldd` on
the binary shows no `libonnxruntime`, so the AppImage stays one file and the
Flatpak needs nothing beyond the network it already uses for cargo. The debug
binary is larger. The crate is a 2.0 release candidate and its API still moves
between candidates; the version is pinned in `Cargo.toml`.

**The way back to `tract`**, should it be wanted — a smaller binary, a build
with no network, a platform ONNX Runtime does not cover:

1. `git revert` the commit titled "Inference goes through ONNX Runtime", or by
   hand:
2. `Cargo.toml`: remove `ort` and `ndarray`, restore
   `tract-onnx = { version = "0.23.7", default-features = false }`.
3. Delete `crates/numa-infer` and every dependency on it (it was `src/infer.rs`
   when this was written).
4. In each of the model modules listed above: `use tract_onnx::prelude::*;`
   back; `type Plan = TypedRunnableModel;`; the `plan()` function builds with
   `tract_onnx::onnx().model_for_path(&path)` then `.with_input_fact(0,
   f32::fact([1, 3, EDGE, EDGE]).into())`, `.into_optimized()`,
   `.into_runnable()`, held in a `OnceLock<Option<Arc<Plan>>>`; inputs are
   `tract_ndarray::Array4` run as `plan.run(tvec!(input.into_tensor().into()))`;
   outputs are read with `outputs[i].to_plain_array_view::<f32>()`. SAM's
   decoder needs four input facts (`[1,1,1,2]` f32, `[1,1,1]` i64, and two
   `[1,256,64,64]` f32) and keeps the encoder's outputs as `TValue`.
   `render::classify`, `cull::people` and `render::ai_denoise` were written
   after the move and have no `tract` version to go back to.
5. `tract` fixes the input shape at plan time, so a model asked at another
   size needs another plan — and the ONNX must have been exported for that
   size (the `tract` version of EfficientViT failed at 1024 on an export made
   at 512). ONNX Runtime takes the shape from the tensor.
6. Expect the numbers in the table, and DETAIL-003 to be blocked again.

**Threads.** One run uses every core (`with_intra_threads`); runs on one model
are serialised behind a mutex. The runtime is parallel inside a run, so the
lock only costs anything when two callers want the same model at once. The mask
and face models are loaded once and kept for the session; SCUNet is loaded for
a denoise run and dropped after it, because it runs once per photograph and
77 MB of weights plus the runtime's arena is a lot to hold on to for that.

## Naming the animal

MASK-012. The found-mask chips said "Animal" for whatever ADE20K's one animal
class covered and "Subject" for what only the matting model could find, which
on the kite in DSCF2934 is a bird and nothing else. An ImageNet classifier is
asked about a square crop around that region (the matte's subject, or
ADE20K's animal cells against their own peak), padded by a quarter. Its
thousand probabilities are summed into groups a photographer names — Bird,
Dog, Cat, Horse, Cow, Sheep, Bear, Rabbit, Fish, Insect — and the chip is
renamed when one group holds 60 % of the answer. Wolves, foxes and big cats
belong to no group: a wolf is not a dog, and "Cat" on a lion is the wrong word.

Neither OpenCV Zoo graph ends in a softmax; both answer with logits, so it is
taken in `classify`. The zoo resizes to 256 and centre-crops 224; a crop that
is already square and padded is resized to 224 directly.

Both zoo models were held against 35 frames from the catalog: the frames of a
1 305-frame sample where ADE20K found an animal, the kite and its neighbours,
and a random sample of Japan frames where the chip row would offer a subject.
Birds and fish are what the catalog has; there is no dog or cat in it.

| | frames | PP-ResNet50 (98 MB) | MobileNetV2 (14 MB) |
|---|---|---|---|
| birds (kites, a gull, a sandpiper) | 9 | Bird, 0.76–1.00 | Bird, 0.93–1.00 |
| fish (koi, carp, an aquarium) | 5 | Fish, 0.66–1.00 | Fish on four, 0.68–1.00; *platypus* 0.96 on a carp |
| Nara deer, and something small in driftwood | 4 | no group above 0.09 | Cow 0.33 on one |
| statues ADE20K calls animal: a centaur, Buddhas, a fountain | 7 | no group above 0.03 | **Dog 0.63 on the bronze centaur** (Great Dane) |
| subject-chip frames with no animal: a car, a stupa, a vase, a valley | 7 | no group above 0.02 | none above 0.01 |
| people, the matte forced on them | 3 | no group above 0.04 | none above 0.01 |

MobileNetV2 names a bronze centaur a dog, above any threshold that also keeps
the fish; that is the mistake this feature is not allowed to make, so the
larger model it is. PP-ResNet50 puts the lowest right answer at 0.66 (a shoal
of fish in a dark tank) and the highest wrong group at 0.09, and 0.6 sits
where a wrong name needs six times what any frame gave it. 5–13 ms a crop on
the CPU; the matte that finds the kite's crop is the half second, and that
only runs when ADE20K named neither a person nor an animal.

These ran in tests, under the C locale. The application puts `LC_NUMERIC` back
to C before any model loads (see "Inference runtime"), and in the window the
kite's chip reads Bird, 100 %, the same as the test.

## What is delivered separately

The models and the camera profiles are not in the download: together they are
several hundred megabytes, their licences are their own, and a photographer who
never makes a mask should not carry a segmentation network. Every feature that
needs one steps aside without it — Click is insensitive and the found-mask
section is not shown — rather than failing when pressed. `NUMA_MODELS` points
the models folder somewhere else, which is how the application is tried as a new
user sees it: an empty folder, and every feature has to say so.

**Downloads** (`START-004`, `RENDER-013`) happen from Preferences and the
first-start dialog, one model or all of them. Each file is asked of a mirror
first — the `models` release of Numa's own repository, which holds every model
and RawTherapee's profiles with their licences — and of the place it came from
second, so the downloads keep working whatever happens to either. They go
through `curl` rather than an HTTP library in the binary: it is on every
desktop this runs on, it resumes an interrupted download with `--continue-at`,
and it follows the redirects GitHub and Hugging Face both answer with. Each file
is written to a `.part` name and renamed when complete, so a half-downloaded
model is never loaded; a failed download offers its link in a browser, and a
file fetched that way is found under the name its link gives it. Every file is
checked against its SHA-256 before it is renamed (`E3`, 21 September) —
`sha256sum` for the reason `curl` fetches it — and one that does not match is
removed and the next link tried; the digests are GitHub's own for the mirror's
assets and Hugging Face's LFS ids, matched against the files installed here
before they were written down. The profiles arrive as
one archive and are unpacked into a folder of their own, apart from the
photographer's, so neither can be mistaken for the other. The GPU provider
(`PERF-012`) travels the same road with one difference: Microsoft publishes it
only as a Python wheel, so the download is a `.whl` and three files are taken
out of it into the models folder — the provider, ONNX Runtime's MIT licence
and its third-party notices, each under a name that says what it belongs to.
It is deliberately not part of "Download all": on a machine whose driver
cannot run it, it is 7 MB and a probe bought for nothing.

**Looking for a new version** (`START-009`). An AppImage does not update
itself. Numa asks once, and only after a library has been added so the first
start is about that and nothing else; if the answer is yes, GitHub's releases
list is fetched with `curl` at most once a day and a newer numbered release is
announced in a toast with a link. Tags that are not versions — the models
release is tagged `models` — are not releases of Numa. The part that reads the
answer (`io::update`) is tested, and the live API answers as the parser expects.

**What a bug report should carry** (`START-011`). About's troubleshooting page
lists the versions, the renderer, the installed models, the open library's size
and a sample of its cameras, and how to widen the log. About moved into the
window so that it can describe what is open — and sampling cameras is what found
the X3F panic described under "Files that are not shaped like a RAF".

**Accessible names** (`UX-004`) are given in one pass over the window after it
is built: an icon-only button's tooltip is already the sentence a sighted person
reads, so it becomes the accessible label, for every such button at once rather
than at each of the hundred places one is made.

---

## Review of the numbers — 2026-09-17

An outside review recomputed what could be derived from this document and the
feature list. What it found, and what was done about it.

### Confirmed

| Claim | Recomputed |
|---|---|
| 0.18 lands on 118 | sRGB-encoded 0.18 × 255 = 117.6 |
| Proxy 44 MiB, full frame 455 MiB | 2400×1600×12 B = 43.9 MiB; 7728×5152×12 B = 455.6 MiB |
| Markesteijn +81 % full, +3.6 % proxy | 0.0519/0.0287 = 1.808; 0.0723/0.0698 = 1.036 |
| Corner keeps 47 % = a full stop | log₂ 0.47 = −1.09 EV |
| CA 0.0011 = five pixels at the corner | 4644 px × 0.0011 = 5.1 px |
| Proxy runs out at about 31 % | 2400/7728 = 31.1 % |
| One model answer per thirty pixels of a 40 MP frame | 7728/256 = 30.2 px |
| Blue at 7 % luminance → ×14 to weigh as white | 1/0.0722 = 13.9 |
| Keystone coefficient ½ is a threefold stretch | (1/(1−½)) / (1/(1+½)) = 3 |
| sRGB green sits at 103° in ProPhoto | 103.1° |

### Wrong, and corrected

- **Reducing a 40 MP frame to 2048** averages fourteen pixels into one, not four
  hundred: (7728/2048)² = 14.2 (IO-012).
- **The runtime comparison** said eight to twelve times; the table in *Inference
  runtime* gives 5.8× to 10.4×. The summaries say six to ten now.
- **Memory figures** in *Seeing real pixels* are MiB, and say so.

### Checked, and the document was right

- **Purple at 255° in ProPhoto** (ADJ-005). The reviewer's script fed
  gamma-encoded HSV straight into the matrices; `prophoto_hue_of_srgb` decodes
  to linear light first. Only orange and purple have a component at 0.5, so
  only they differed: orange 26.2° (script 44.6°), purple 255.0° (script 264.7°).

### Measured, and not reproduced

- **The base tone curve.** Refitted from the eight original RAF frames with the
  real curve ported into `dev/fit_tone_curve.py` and percentiles dumped by an
  ignored test in `tests/colour_ab.rs`, the fit gives **1.104 EV / 0.948**
  against the shipped 1.241 / 0.939 (0.943 in an older version of this
  document). The repository's own fitter on the same pixels gives 1.094 / 0.971,
  so the port and the dump agree with it: the same frames now decode about
  0.14 EV brighter than when the constants were fitted, most likely through the
  lens vignetting correction or the camera profile that came since. Not yet
  isolated, and no constant has been changed.
- **Sharpness against Lightroom and Capture One** (DETAIL-008) still needs
  exports from those two; see `dev/mtf50.py`.


### The subject mask, as the photographer sees it — 20 September

Reviewed over 72 panels. Three faults, in the order they matter:

1. **The subject chosen is not the subject.** Not a boundary problem — the
   model picks the wrong thing to be salient about.
2. **It runs over, and it falls short.** The kitesurfers plus a piece of wave;
   a person fused with the lifeguard hut behind her; a woman plus a length of
   fence — and parts of the subject missing too. Both directions.
3. **The edge still shows at 1:1**, though "much better than it was".

Only (3) is what the matting shootout measured, and (3) is the one he calls
improved.

**But the panels he judged were made by the wrong rig, and that has to be said
before any of this is acted on.** They called `matte::subject` directly. The
application does not: `auto.rs` builds a `Shape::Segment` mask over
`MATTEABLE` — person and animal — with `matte = true`, and `resolve_mask`
reaches for the matting model *only when the semantic model finds neither*.
So what those 72 panels showed is the fallback in isolation, not the path a
photographer takes.

That makes (1) and (2) a question about **when the fallback fires**, not about
the matting model as such: where the semantic model names a person, it has
said something about that person and the mask stops at her. Where it does not
— a kitesurfer too small or too far to be a `person`, say — IS-Net answers
"what stands out", and what stands out includes the wave. Re-measuring along
the real path is the first thing, because it decides whether there is a
boundary problem at all or only a detection one wearing its coat.

### The real path, measured — 20 September, later

`tests/matte_survey.rs` gained `where_the_real_path_fails`, which walks the
route the editor takes rather than asking the matting model on its own: the
proxy, `apply_stack`, `MASK_RASTER`, a `Segment` mask over `MATTEABLE`, and
`resolve_mask`. Thirty photographs, every 284th of the 8 537 in the library.

**The fallback is not an edge case.** Sixteen of the thirty were answered by
the semantic model and **fourteen fell back to matting** — so the 72 panels
were measuring something the photographer meets on nearly half his frames,
even if they were not measuring the path he takes to it.

And on the semantic route, where the matting model is only supposed to sharpen
the edge, it does one of two things and neither is what it was asked for:

| | named by the model | delivered | soft band | edge step |
|---|---|---|---|---|
| DSCF6390 | 35.63 % | **0.85 %** | 1.76 % | 0.007 |
| DSCF2310 | 9.67 % | **0.24 %** | 0.32 % | 0.000 |
| DSCF7577 | 30.15 % | 30.19 % | **0.87 %** | **0.078** |
| DSCF6674 | 36.48 % | 24.83 % | **24.57 %** | 0.007 |

Either the mask collapses to a fortieth of what the model named, or it comes
back as one long ramp with no inside, or it keeps its area and delivers a
**cut-out**: 0.87 % of the frame in the soft band around a mask covering 30 %.
Rendered as a picture, DSCF7577's subject mask is a silhouette with no hair in
it at all — which is what a photographer sees as "a hard edge around her" the
moment a lift is applied inside it.

**`CERTAINTY` has no say in any of this.** The constant that the comment beside
it calls "the part that reads as too much feather" is measured, at 1.0 and at
3.0, to make no difference at all to the delivered mask: the numbers above are
the same to three decimals. It only bites with the matting model out of the
way (`MATTE=0` in the rig), where it does exactly what it claims — the edge
step on DSCF7577 goes 0.035 to 0.118. On the path the application takes,
`matte::refine`'s answer **replaces** the alpha it was handed, so the whole
semantic refinement — guided filter, radius, steepening — is thrown away
wherever the matting model commits.

Which puts the edge where `matte.rs` decides it: IS-Net at **1024 on a crop
resized to a square**. For the portrait above the subject's box is about
700 × 1500, so it is squashed to 1024 × 1024 — two thirds of the vertical
detail gone before the model sees it — and interpolated back up onto a 2048
raster that is then stretched again to the frame. Hair is one to three pixels
at that raster. No stage ever samples it, so no stage can return it.

Three things it is *not*, each struck off by measurement rather than by
argument:

- **Not the graphics card.** The same seven portraits on the processor and on
  WebGPU agree to two decimals in coverage and soft band. `c32aa45` did not
  change the mask.
- **Not the smaller proxy** from `41eadf9`. At 2400, 1920 and 1400 the edge
  step is identical to three decimals; the mask is built at `REFINE` = 2048
  either way.
- **Not `PERF-014` or the B3 panel work.** Neither touches the alpha chain, and
  `git log -S` on `search_edge`, `matte::refine` and `!matted` reaches back to
  MASK-008 and no further.

So the hard edge is not something that broke today. It is what this chain has
always produced on hair, first looked at on hair. The repair is not a better
matting model — BiRefNet drew the same wing with a harder edge — it is to stop
stretching a 2048 alpha to a 40 megapixel frame and instead upsample it
edge-aware against the photograph itself, which is the guided filter that is
already in `local.rs`, on the other side of the multiplication.

### What was done about it — 21 September

Three changes, each measured on the same thirty frames, and one road measured
and not taken.

**Auto stopped guessing.** The subject lift now asks only the semantic model:
sixteen of the thirty lift, fourteen do not, which is that column exactly. The
matting fallback is still there and still reachable by pressing the Subject
chip — a photographer choosing the frame is what makes "what stands out here"
the right question to answer.

**A refinement below half is a refusal.** `matte::refine` was allowed to
replace the alpha it was handed with anything, including a fortieth of it.
Seven frames of thirty recover every pixel the model named:

| | before | after |
|---|---|---|
| DSCF6390 | 0.85 % | 35.63 % |
| DSCF2310 | 0.24 % | 9.67 % |
| DSCF3760 | 0.50 % | 7.98 % |
| DSCF3169 | 1.06 % | 2.95 % |
| DSCF9990 | 0.31 % | 0.68 % |
| DSCF3474 | 0.00 % | 0.15 % |
| DSCF7083 | 0.01 % | 0.08 % |

Three more than expected. DSCF3169 and DSCF9990 were losing two thirds and a
half rather than everything; DSCF7083 is a frame where the fallback fired and
the matting model never committed, so the semantic mask was the base and the
refinement ate that instead.

**The model stopped being shown a squashed subject.** A tall crop is read as
two overlapping squares rather than pressed into one. On DSCF7577 the box is
756 × 1460: all 756 columns reached the model before and 1024 of the 1460
rows; now both reach it whole. Two frames of thirty move — one of the two
masks that came back as one long ramp with no inside becomes a proper mask
(DSCF8091: 20.79 % coverage with 25.19 % soft, to 35.62 % with 8.95 %) and the
other gets slightly softer. It costs a second run of the model on a tall
subject, 520 ms to 720.

And DSCF7577 itself, the frame all of this was measured for, does not move:
30.19 % to 30.18 %, the edge step 0.078 to 0.076. **So the squashing was real
and it is not what makes that edge a cut-out.**

### The edge-aware upsample, measured and dropped

The remaining idea was to stop stretching a 2048 alpha onto a 7728-pixel frame
and instead pull it onto the photograph's own edges at the size it is drawn,
with the guided filter that already exists in `local.rs`. It was built, and it
does not pay.

A mask that only changes exposure is a multiply, so the rendered frame divided
by the unrendered one gives the border back exactly at full size. Measured
there, over the 420-pixel panel at the border's hardest point:

| | edge step | follows the photograph | a full frame |
|---|---|---|---|
| stretched, as it ships | 0.0110 | 0.874 | 512 ms |
| edge-aware, best of four settings | 0.0109 | **0.899** | 650 ms |

Two and a half per cent of agreement, for between a quarter and three quarters
more time — against a budget of fifteen. A 1:1 pan would go from 50 ms to 80.

The reason is worth keeping, because it says where the fault is not. By the
time the alpha reaches the frame it has been interpolated twice and is
already smooth: there is no staircase left at that point to remove. And a
filter cannot pull the border onto hair that the alpha never had — the detail
was lost at 1024, on a crop, and nothing downstream can return what was never
sampled. The border's problem is not the last multiplication.

### The raster, and the drag that was holding it down — 21 September

FT-026 closed the modelling road: there is no matting model in reach that
answers at the size hair exists at, and asking this one about native-resolution
tiles of a border makes it decline on most of them — 61 of 62 on a person
against a wall — and draw blobs on the rest. A salient-object model needs an
object, and a border has none.

So the remaining ground was the raster, and the cost that decides it is not the
render. `Mask::field` samples the raster per output pixel and does not care how
big it is, so an export and a 1:1 pan cost the same at 4096 as at 2048. The
cost is the **drag**: Feather and Edge reshape the whole raster on every tick.

| raster | border follows the photograph | a drag tick | a settle | memory per mask |
|---|---|---|---|---|
| 2048 | 0.321 | 2 ms | — | 22 MB |
| 3072 | 0.381 | 5 ms | — | 50 MB |
| **4096 with PERF-015** | **0.458** | **3 ms** | 11 ms | 89 MB |

Eleven milliseconds is inside a frame on this machine and outside one on a
laptop three times slower, which is what put the raster at 3072 for an hour.
That was the wrong move: the trade did not want splitting, it wanted
dissolving, and the pattern was already in the file. PERF-006 draws a draft
while the hand is moving and the real thing when it stops; the edge does the
same now. A drag shapes a half-size copy — a quarter of the cells, and the same
border, because `shape_edge` takes its blur radius as a fraction of the alpha's
own long side — and the settle shapes the real one.

**The general shape of that, because it comes up again**: the way to have the
better answer without paying for it everywhere is rarely to let someone choose
between fast and good. It is to find the moment where the expensive step does
not matter — a frame that is about to be replaced, a hand that is still moving
— and be cheap only there. That costs no setting, no explanation, and everyone
gets the same photograph. A quality setting would have meant the same
photograph rendering differently on two machines, which is the inconsistency
`RenderInputs` exists to prevent and the reason the denoise tile was left alone
on 20 September.

### What the raster actually bought, which was mostly the filter — 21 September

The two changes of that evening were measured one at a time, at 4096, and they
are not independent:

| | soft band | follows the photograph |
|---|---|---|
| 2048, with the staircase filter | 0.93 % | 0.321 |
| 4096, **without** the filter | 0.92 % | 0.330 |
| 4096, with it | 1.46 % | **0.458** |

A finer raster on its own is worth almost nothing — 0.330 against 0.321. What
it does is give the guided filter more cells and a finer guide to work with,
and *that* pair is worth 43 %. The filter is not smoothing the border; it is
moving it onto the photograph, by 116 % on DSCF2195 and 39 % on DSCF7577.

Worth keeping in mind when reading either number alone, and worth the habit:
two changes shipped together should be measured apart at least once, or the
credit lands on whichever was committed last.

The price is half a per cent of the frame in the transition, which is what a
border that follows a subject costs here.

---

## Wide-gamut exports follow the file — 21 September

With the working-space picker gone from the panel (PANEL_PLAN P1) every
photograph was edited in sRGB, and the colour stage clips to the working
space. So a Display P3 export held sRGB's colours in P3's numbers: on the
camera corpus 0.0 % of every frame lay outside sRGB. Extended-range sRGB —
keeping the negatives through the stack — was looked at and set aside:
exposure clamps at zero and contrast is a per-channel `powf`, so it would have
been a change to every operation and to the byte-for-byte sRGB path.

What the export does instead (`Document::set_output_space`) is render a
photograph edited in sRGB in the file's own space. Measured on the corpus,
comparing each export back in Lab against the sRGB render, on the pixels both
can hold:

| space | outside sRGB | median ΔE | p95 ΔE |
|---|---|---|---|
| Display P3 | 0.0–3.0 % | 0.15–0.44 | 0.6–2.4 |
| Adobe RGB | 0.0–2.3 % | 0.16–0.59 | 1.4–1.9 |
| ProPhoto | 0.1–3.9 % | 0.5–1.6 | 2.3–3.9 |

The look is a per-channel tone curve, and the same curve in wider primaries is
very nearly the same picture; ProPhoto drifts most, and Lightroom renders in
ProPhoto primaries too. The mixer's table is read in the pixels' own space
(`Look::in_space`) — without it a band acted on a P3 pixel as if it were sRGB,
which the test that fails without it shows.

## Export formats, and whose encoder — 21 September

**AVIF** is rav1e and avif-serialize directly: ravif writes every file as
sRGB, and a P3 file described as sRGB drains. Without rav1e's assembly (it
wants nasm to build) a 20 MP frame is 6.3 s on eight cores and 651 kB where
the JPEG is 8 MB. Its colour box has codes for sRGB and P3 only, so Adobe RGB
and ProPhoto are written as P3. **JPEG XL** loads the system's libjxl when
asked: the only lossy encoder in Rust is AGPL, and a library missing from a
desktop should cost one entry in a list, not the start. Only the handle API
and the two structs libjxl keeps padded for this are touched, so 0.7 onwards
will do. **DNG** is rawler's writer driven as its converter drives it, with
Numa's render as the preview — turned back to the sensor's orientation, since
the tag applies to previews too — and the edit as Camera Raw settings, the
import (`foreign`) run backwards and checked by translating it back.

**HDR** is an Ultra HDR JPEG. The HDR rendition is the scene's own luminance
wherever the base curve compressed it, and the SDR frame elsewhere: the curve
takes scene 0.05 to 0.016 on purpose and lifts 0.3 to 0.356, so a gain above
one only starts past about 0.8 of scene white, capped three stops over paper
white. The first maps lifted nothing: `image`'s resize clamps a float channel
to 0..1, and every gain worth having is above one. They are resized as stops
now, and the test asks a frame a stop up for more than a stop of gain.

## A cheaper AI denoise, and why it is still SCUNet — 21 September

Asked for something cheaper, darktable 5.6's NAFNet (SIDD, MIT) was run beside
SCUNet on an ISO 12800 X-T5 frame through the same tiler: 11× faster on the
card (a 40 MP frame about 20 s against 200) and 4.7× on the processor — and
visibly less clean, grain left in flat stone and the frame a little darker,
even through darktable's own shadow boost (square root in, square out). The
rule given was faster *and* better; it was only the first. SCUNet in float16
on the card with a 512 tile is 108 s against 200, 99.9 % of the frame within
three 8-bit codes of float32; on the processor half precision buys nothing,
so it keeps float32 at a 320 tile.

## AI sharpen, and the checkpoint that was left out — 21 September

Restormer publishes six checkpoints as PyTorch; two are about blur. Measured on
crops of three corpus cameras, each smeared nine pixels sideways and, apart,
blurred by a 1.6-pixel Gaussian, against the untouched crop: the
motion-deblurring checkpoint took the smears from 32.8–36.1 dB to 39.1–40.2
and left sharp crops at 39–45 dB of themselves, and did nothing for the
Gaussian; the defocus checkpoint made the sharp crops worse (30–34 dB) and a
blurred one worse than its blur (35.9 to 27.5). So AI sharpen is the motion
one, said to be for a hand that moved. Exported to ONNX with dynamic sides, it
gives the same 40.2 dB through Numa's runtime and tiler as through PyTorch; 256
tiles beat 512 on both devices. When denoise is on it sharpens the denoised
frame — the cache key says which — because sharpening the noisy one and then
mixing the two back would bring the noise back.

## Taking things out — 21 September

**Remove** (`RETOUCH-004`) runs LaMa where the spot is rendered and keeps the
fill per spot and render size, as a ratio to the ring around it — so exposure
and white balance move it afterwards without the model, and dragging the spot
is what asks again. **Remove people** (`RETOUCH-007`) was built first on the
masks' segmentation and found nobody on a beach frame with three people behind
the subject: on a 128-cell grid they scored below even odds and joined the
subject through the cells between. YOLOX-s found all three in 64 ms. The
subject is the largest box, and a figure stays only if it stands in the middle
of that box — the first rule, "not touching it", kept all three, because an
arm held out stretched the subject's box across the frame.

**Find dust** (`RETOUCH-005`) went from sixty finds on a face, a blurred town
and an arm to five in a sky by four rules, each added for a false find it
explained: smooth at the speck's own scale, not only the finest (a blurred
field of leaves); alone, the difference around it under a quarter of its
depth (bokeh, pores, edges); never deeper than 0.3 stop (windows); and not on
skin, where a darker spot is a freckle.

## CI, and one camera per make — 21 September

`dev/check.sh` runs on every push (`E4`) on Ubuntu 24.04, the oldest desktop
the GTK and libadwaita features allow, and with it FT-010's corpus (`A5`):
nine CC0 files from raw.pixls.us, fetched against their SHA-256 and cached on
the manifest's hash, each read as the application reads it and asserted on
its size, on finite values and on developing to neither black nor white. The
whole suite passes headless with an empty models folder, which is what a
runner is.

## The Flatpak — 21 September

The manifest from 17 September built as it was; what had to change was the
application. A Flatpak is given its own `XDG_DATA_HOME` and `XDG_CACHE_HOME`
under `~/.var/app`, so it would have started with no libraries, none of the
3 366 presets and none of two gigabytes of models. `numa_core::paths` asks
for the host's folders instead when `FLATPAK_ID` is set (`HOST_XDG_*_HOME`,
else `~/.local/share` and `~/.cache`), which `--filesystem=host` can reach
anyway. The Flatpak, an AppImage and a development build now share one
catalogue, as the AppImage and a development build already did; moving to
the Flatpak costs nothing. The edits were never at risk — they live in each
library's own `.numa` — but the list of libraries and the presets were.

Three things the sandbox changes:

- **The screen's profile** (A1) comes from colord on the system bus:
  `--system-talk-name=org.freedesktop.ColorManager`.
- **"Open photographs with Numa"** would write into the sandbox's own
  `mimeapps.list`, which the file manager never reads, and report success.
  Preferences says to use Open With instead; the exported desktop entry lists
  every type, so Numa is offered there.
- **Light or dark** reaches a sandboxed libadwaita through the settings
  portal, not through dconf. Under a test bus without one the editor came up
  light — and `style.css` has 80 colours written for a dark panel, so the
  slider names were white on grey. With the portal it follows the desktop, as
  it should. The light editor is a fault of every build on a light desktop,
  not of the Flatpak; it waits for a decision on whether the editor is always
  dark.

What else was checked rather than assumed: GNOME 50 ships libjxl 0.11, which
the `dlopen` in `jxl.rs` finds; ONNX Runtime is linked statically, so there is
nothing to bundle; and the WebGPU plugin, unpacked from its wheel into the
shared models folder, loads on radv inside the sandbox with `--device=dri`
(Dawn's start-up warnings are in the log).

The manifest's source is the working tree, and the tree holds `presets data`:
1.3 GB of commercial presets kept for the importer's tests. It is on the skip
list with `.claude`, `dist`, `fable-tickets*` and the camera corpus, and the
builder's own state, build and repo go under `target/flatpak`, which is
skipped too. The release build takes 52 s.

Tested headless with `flatpak build` against a copy of the catalogue, pointed
at through `HOST_XDG_DATA_HOME` — the Flatpak reads the real one otherwise.

**Towards Flathub.** Flathub builds without a network and from source, and
`ort` downloads a prebuilt ONNX Runtime. Staying with `ort` does not need a
change to the application: `ort-sys` links an ONNX Runtime found through
`ORT_LIB_PATH` (or `ORT_LIB_LOCATION`) before it considers downloading one,
statically or, with `ORT_PREFER_DYNAMIC_LINK`, as a shared library — so a
package can build ONNX Runtime 1.28 from source and point `ort` at it. The
WebGPU plugin comes out of the same source tree; `numa_infer::gpu_plugin`
looks for it in the `lib` beside the program's `bin` (`/app/lib` in a
Flatpak) before the models folder, because a plugin built with the ONNX
Runtime it loads into is known to agree with it and Microsoft's wheel is only
known to agree with the one `ort` downloads. Checked by running the binary as
`<dir>/bin/numa` with the plugin only in `<dir>/lib`: the masks went to the
card, and with the plugin taken away they did not. The Flathub manifest
itself is the photographer's to write — Flathub does not accept one written
with AI.

## Hearing back: crashes, usage and ideas — 22 September

Numa knows nothing about how it fares on anyone else's computer: a crash on
another machine is invisible, and so is which tools matter. START-013, 014
and 015 plan three ways to hear back. Nothing is built yet; this is the
decision and why.

**What holds for all three.**
- Off until the photographer says yes, and asked once, the way the update
  check is (START-009).
- What would be sent can be read before it goes, and it is never a photograph
  or anything that says whose.
  - Never paths, file names, EXIF, GPS, faces, names or library names.
  - No identifier that ties one session to the next.
- One switch in Preferences stops it.
- It works inside the Flatpak, which has the network for downloads already.

This is also what the GDPR asks for: consent, and no more data than the
purpose needs.

**Crashes (START-013): no server at first.**
- **A panic** runs a hook, which writes the report into the data folder. A
  crash below Rust — a driver, ONNX Runtime, Dawn — does not. For those, a
  marker the GPU guard's way (written at start, removed on a clean exit) at
  least says it happened.
- **At the next start**, "Send report…" shows the text. "Open on GitHub"
  pre-fills an issue; Copy is for anyone without an account. That is
  START-011's route, one step further, and it costs nothing.
- **When there are more reports than one person reads:**
  [GlitchTip](https://glitchtip.com), with the MIT-licensed `sentry` crate.
  GlitchTip is open source (MIT) and accepts Sentry's SDKs. Self-hosted it is
  four containers in under 512 MB; hosted, the first 1,000 events a month are
  free.
- **Not these:**
  - [Bugsink](https://www.bugsink.com) takes the same SDKs, but it is
    source-available (PolyForm Shield), not open source.
  - Sentry itself is not open source, and self-hosted it is some forty
    containers.
- **The Flatpak strips its binary.** Its symbols go to the `.Debug` extension,
  so a backtrace from it has addresses rather than lines. The report has to
  carry the build ID so it can be symbolised against that extension.

**Usage (START-014):**
- **What.** A handful of counts per session:
  - which tools were used;
  - how long AI denoise, AI sharpen and Super Resolution took, and on which
    device;
  - Flatpak, AppImage or a build;
  - the camera make, which decides which camera goes into the corpus next.
- **Where to.** [Aptabase](https://aptabase.com) is made for exactly this:
  analytics for desktop and mobile apps, with no unique identifiers, and
  GDPR-compliant by design.
  - The server is AGPLv3, so it can be self-hosted; the SDKs are MIT.
  - Hosted, in the EU if chosen, 20,000 events a month are free.
- **How.** There is no Rust SDK, and none is needed: an event is one HTTPS
  POST, and Numa already runs `curl` for the update check. The events wait in
  the data folder and go once per session, so a session without a network
  loses nothing.
- **Considered and passed over:**
  - [Umami](https://umami.is) (MIT) and Plausible are web analytics first.
  - PostHog is far more than this needs.
  - KDE's KUserFeedback is Qt.

**Ideas and tips (START-015).** GitHub Discussions with an Ideas category,
and issue forms for bugs, both free. "Send feedback…" in the main menu
pre-fills one, with START-011's debug information only if the photographer
ticks it. Most photographers do not have a GitHub account, so mail is the
second way in. It shows their address to one person, which is their choice
to make.

Sources: [GlitchTip review and pricing, 2026](https://cubeapm.com/blog/glitchtip-pricing-and-review/),
[Self-host Sentry or GlitchTip, 2026](https://danubedata.ro/blog/self-host-sentry-glitchtip-error-tracking-2026),
[Aptabase on GitHub](https://github.com/aptabase/aptabase),
[Bugsink](https://www.bugsink.com/).
