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
moved 4702 K / −48 to 5201 K / −41. Both pipettes draw their own cursor, a
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
`CURVE_CONTRAST = 0.943`). A second body needs a per-camera table.

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
AppImage and the download do. The Flatpak's sandbox sees none of those folders,
which is why the download exists. The panel names whichever profile is in use.

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
out. `fitted_crop` finds the largest crop of the chosen shape that the
photograph fills. It first shrank the rectangle about its own centre until one
corner touched, which gave up a band of photograph on the opposite sides for
nothing. Now it uses the fact that the map is a projection: what the photograph
covers, taken back through the keystone, is a convex quadrilateral, and a
rectangle lies inside it exactly when its four corners do. For a given size the
centres that work are that quadrilateral intersected with itself moved by each
corner — a convex polygon, clipped with Sutherland–Hodgman — which is either
empty or not. So the size is found by bisection (24 passes, up to the whole
frame), the centre is the point of that polygon nearest where the crop was, and
the answer is checked against `source_map` itself and shrunk by a hair if the
two disagree. A test on the frame the fault was reported on finds a crop larger
than the centred one, with nothing larger fitting beside it. A frame with no
perspective correction is never quietly zoomed into.

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
(up to 30°, towards the next primary, the way Lightroom's sliders go) and scaled,
and the drift from white is taken back out in proportion to each primary's share
of brightness, so grey stays grey. A shadow tint moves green in the shadows only.
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

## The render pipeline

```
RAW file
  └─ decode (io::raw::decode_with) ─────> linear RGB, f32, camera-native
       │  rawler's develop without WhiteBalance, Calibrate and SRgb,
       │    or Markesteijn for X-Trans at full size        (IO-013)
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
`src/infer.rs`. A `Model` is built from a path and handed arrays; nothing else
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
3. Delete `src/infer.rs` and its `pub mod infer;` line in `src/lib.rs`.
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

**GPU.** Not wired. ONNX Runtime has execution providers for it; the one for
this machine's Radeon is ROCm or the newer Vulkan path, neither measured. Add
one in `src/infer.rs` when a photograph needs it, and nowhere else.

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
file fetched that way is found under the name its link gives it. There is no
checksum yet: a truncated file fails to load and says so. The profiles arrive as
one archive and are unpacked into a folder of their own, apart from the
photographer's, so neither can be mistaken for the other.

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

