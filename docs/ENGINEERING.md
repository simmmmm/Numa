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

**A table's value axis is read at sensor white.** A table with more than one
value row is indexed by brightness, and the DNG convention — Adobe's, and
RawTherapee's, whose profiles are fitted in RawTherapee — has sensor white at
1.0. Numa's frame has middle grey at 0.18 and sensor white two or three stops
up, so a look table was handed values two to five times what it was made for
and read its top rows for anything bright. RawTherapee's A6000 profile carries
a 90×30×30 look; on a blue sky it took red to nothing:

| DSC08326's sky, linear sRGB | R | G | B |
|---|---|---|---|
| RawTherapee, same profile | 0.137 | 0.257 | 0.513 |
| Numa, before | 0.006 | 0.274 | 0.742 |
| Numa, at sensor white | 0.136 | 0.258 | 0.508 |

Matrix and hue/sat map already agreed with RawTherapee to the third decimal;
only the look was off, because only it has value rows. `Rendering` divides by
the frame's `clip` for the lookup and nothing else. The tables Numa fits itself
(`RENDER-012`) learned the frame as Numa holds it, so they say so:
`ProfileCalibrationSignature` reads `numa.photo scene-referred`, `dcp::write`
puts it there, and those are looked up as before.

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

### Every body (RENDER-023)

`fit_camera_profile_from_samples` fits one body from a folder of raw.pixls.us
CC0 raws; 29 September it ran over 220 bodies of up to four raws each. What the
run taught the fitter:

- **A held-out frame of the same scene measures nothing.** Most bodies' raws
  are one shot in several compressions, seconds apart; holding one of those out
  tests the fit on a copy of what it was fitted on. The fitter now holds out a
  frame taken more than half an hour from every other, and says in its RESULT
  line whether it found one. 176 of the 220 beat the matrix (and RawTherapee's
  profile) on their held-out frame; only 20 of those did so on a scene of its
  own, and only those ship.
- **The preview does not always follow the colour-space setting.** Of 129
  frames whose camera was set to Adobe RGB, 101 previews read nearer the matrix
  as sRGB. The fitter reads such a JPEG both ways and keeps the nearer.
- **Some frames are not a fit's business**: a Fujifilm film simulation (39
  frames; 16 of 29 Fujifilm bodies had no Provia frame at Color 0), a black and
  white JPEG, a JPEG cropped to another aspect than the raw (50 frames, mostly
  CHDK DNGs and 16:9 settings), an Olympus High Res Shot, whose decode comes out
  in the wrong colours (matrix error 2.0 against 0.01-0.12; FT-030), and 108
  raws with no camera JPEG to fit against or no decode.
- **More profiles cost the lookup almost nothing.** The first lookup of a body
  reads every installed profile's header (PERF-023). Best of three rounds:
  3.2 ms with the six, 3.6 ms with 26, 3.9 ms with 225 of Numa's own beside
  RawTherapee's 161; a later body 0.8 → 1.6 ms.
- **The first six were measured the same way, and five did not survive it.**
  Each had been held out on a frame 1 to 8 minutes from its fitted ones. Only
  the A7R III and the X-T5 have a raw of another scene: the A7R III's lost to
  the matrix there (0.0105 against 0.0101), the X-T5's won on all five frames
  (0.0375 → 0.0308, Adobe Standard 0.0372). The A7R III's went, and so did the
  EOS R5's, D850's, X-S10's and X-T4's, which nothing can measure honestly yet.

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

**27 September: the match read the frame before its white balance.** The raw's
median was taken as Rec. 709 luminance of camera RGB, where red and blue sit a
stop or so under green until the multipliers are applied, so it read low and
the lift came out high. Every frame of every make landed above the camera it
was matching — a photographer's A6000 church as a cyan sky nobody had asked for.
It is now measured through the as-shot matrix, which is what the render does to
those pixels, and the corpus test holds each make to within 0.15 EV:

| 18 frames, 8 makes | Before | After |
|---|---|---|
| Midtones against the camera's JPEG, mean | +0.21 EV | −0.02 EV |
| Furthest out | +0.32 EV (A6000, K-70) | −0.11 EV (DC-G9) |

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
BiRefNet for the subject and its edge (it replaced IS-Net, which replaced MODNet), YuNet for finding faces, SFace
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
| BiRefNet, the subject (since 25 September) | 6.2 s, 8 GB | 0.36–0.47 s | 13× — on, rewritten |
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

**BiRefNet, rewritten for the card** (MASK-008, 25 September). As published
it does not build on WebGPU — Splits of 16 and 32 outputs ask for 17 storage
buffers where the provider allows 16 — and with those cut into Slices 320
nodes still went to the processor: the twenty deformable convolutions' index
arithmetic, in int64, and their four-input Sums. Copying across cost 1.5 s of
2.3. So `numa_infer::rewrite` rewrites the downloaded file once, before its
first use: every Split of more than eight outputs becomes Slices, the index
arithmetic stays in the model's float type from the Cast after Floor to a Cast
put back before each GatherND (whole numbers under 2048, which float16 holds
exactly), and each Sum becomes Adds. Then every one of the 13 142 kernels of a
run is on the card (ONNX Runtime's profile) and a photograph takes 0.36–0.47 s
through the model, 0.39–0.64 s through `matte::subject`; rewriting takes a
quarter of a second, building the session three.

The rewrite edits the protobuf on the wire — fields copied as they were, the
changed nodes re-encoded — rather than decoding ONNX into generated types: no
crate added, and nothing it does not know about can be dropped. It is written
beside the file and renamed over it, so a crash leaves the download as it was,
and it ends in a `metadata_props` entry that `rewrite::rewritten` finds by
reading the last 20 bytes. On the full model its output gives the same answer
as the Python reference rewrite to the bit on the card; on the lite model, on
the processor, IoU 0.9999–1.0 against the unmodified model over six
photographs (`rewrite::tests::birefnet_rewritten`).

On the processor BiRefNet is heavy: the lite model passed 9 GB by its second
photograph with ONNX Runtime's arena, which keeps every run's high-water mark,
and peaks at 5 GB without it, no slower — so these two files run without it
(`LEAN`). The full model is 6.2 s and about 8 GB there, which is what a Linux
machine without the card pays.

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

## Slow sliders, measured — 23 and 24 September

The photographer's report was that editing got slower and slower, the
processor ran all over the place, and sliders were nowhere near the 60 or 144
Hz the screen could show. Two evenings went into it, and the rule that came
out of both is: measure on the photographer's machine before changing
anything. The Xvfb rig has no card, and its guesses were wrong twice.

**The instrument.** `NUMA_TIMING=1` (with `RUST_LOG=info`) prints every render
with its size, time and path — proxy, tile or full, draft or not, and
`recut` when a tile was cut out of the full frame again — beside what the open
photograph holds and what the process uses. And a line per render with what
each pass of the stack cost. Almost every finding below was one of those lines
read off the photographer's terminal.

**What was wrong, in the order it was found.**

1. *The models were on the processor.* ONNX Runtime was handed every WebGPU
   device, and with two (the card and the processor's Radeon) it refused and
   fell back without a word to the user. That was the processor running all
   over the place, and most of the memory: 5.4 GB resident against 63 MB of
   image buffers, the rest being model arenas. One device, a card before a
   software renderer: segmentation 250 → 51 ms, and the editor at 0.7–1.8 GB.
2. *A rating rebuilt the grid* when the library was filtered on flags, which
   flickered it and cancelled the thumbnails still loading.
3. *Masks cost the whole frame each* (PERF-016): 190 → 290 ms for the same
   render as masks were added.
4. *The draft only existed on the proxy* (PERF-006): at 1:1 it was the same six
   megapixels as the finished render, and on a frame that could only be
   rendered whole it was thirty-eight.
5. *The sharp render started at every pause.* 130 ms of quiet counted as the
   end of a drag, and on a careful one that was every other moment: a second
   and a half on the main thread, which is why none of the draft's gains were
   felt. It waits for the mouse button now — watched on the window, after a
   gesture on the slider (never hears the release) and the pointer's modifier
   state (always nought) both failed, the first of them shipped for an hour
   as 0.19.17 with no sharp frame at all.
6. *A turned frame and a frame with Clarity rendered whole at 1:1* (PERF-017,
   PERF-018): 1.4 s and 1 s after every drag.

**What held and what did not.** The proxy as the source of the tone map's
measurement was the obvious move and would have been a mistake: up to 26
levels off along a hard edge, exactly where a halo is judged at 1:1. Measured
before it went in, it was kept for drafts and the full-resolution measurement,
skipping what the base cannot see, took the sharp frame. Two of the evening's
own changes were wrong and put right: PERF-016 handed a mask with Clarity a
strip of the frame to measure, and 0.19.17 waited for a release it never heard.

**Along the way.** The backdrop behind a tile, and the histogram read off it,
were kept per colour stage rather than per edit, so at 1:1 the histogram
showed the photograph from before the last slider. A lasso on a refined mask
was reasoned away by the matting model and the hair trace ate the rest
(MASK-008). A quarter turn or a crop drew every mask again from the models,
with the click model still looking at the old orientation (MASK-014). Auto
froze the window for 0.9 s.

**Where it stands.** At fit: 5–8 ms a draft, 30 ms sharp, flat however many
masks. At 1:1: about 60 ms a draft and 0.2–0.5 s once after letting go. The
draft at 1:1 still carries the tile's panning margin — four times the pixels
on screen — and dropping it while a slider is held is the next 6×.

## The subject path, re-tuned for BiRefNet — 25 September

BiRefNet was chosen on the raw model over whole frames. The editor does not ask
it that way: `resolve_mask` takes the semantic model's person or animal when
there is one and crops round it, gates the answer to it, and filters the edge;
with nothing named it asks about the whole frame and keeps the largest region.
Every one of those steps was tuned against IS-Net. `tests/subject_sheets.rs`
puts the model alone beside the path and Refine edge on 150 of the
photographer's frames, and agreement with the model alone (intersection over
union at a half) is the number below — it is not a quality measure where the
route rightly chose a person, so every change was also looked at.

| | frames changed | agreement, mean over 150 |
|---|---|---|
| as it was | — | 0.719 |
| the gate grown, not blurred | 20 of 55 better, none worse | |
| no guided filter over the matte | 12 of 55 better, none worse | |
| "nothing named" by the panel's floor | 9 of 9 better | |
| scraps under a twentieth dropped, not everything but the largest | 8 of 9 better, none worse | 0.811 (55 better, none worse) |

**The gate was a blur.** It is meant to be one wherever the coarse mask is and
fall off within the slack, and over anything narrower than the blur — a head,
an arm, a brim — it came out under a half *inside* the mask. On DSCF2857 it
was 0.26 at the crown of a hiker's head, which is where the straight line
across his hair came from. Grown by half the slack and then blurred by the
other half, it is solid wherever the mask is.

**The staircase filter goes.** It was written against IS-Net's grid on a 2048
raster. BiRefNet has no staircase to take out, and the filter, guided by
luminance, turned dark hair on a dark hillside into a grey half and put specks
along a light shirt. The measure it was kept for — the border's gradient
following the photograph's, the `the_border_at_full_size` survey — came out a
draw on ten frames: 0.697 with it, 0.707 without, five each way.

**Refine edge's band is narrower inside.** As wide inside the border as out,
anything narrower than it was all band, and ViTMatte in the dark decided it
was not there: of what the first look was sure of, 64 % of DSCF3952's poles
came back under a half. An eighth as wide inside, 0.8 % on the poles and under
0.2 % on the other 33 frames; the hair outside the border is decided as
before.

**What is left is a question of which mask, not of its edge.** On 24 of the
69 frames where the semantic model found a person, the person it found is not
what the photograph is of: faces on billboards, a knee beside two koi, the
driver behind a tram's windscreen, the hands of a Buddha. Subject and Person
are the same mask — `Segment` over the matteable classes — so the path cannot
answer "the subject" without also changing what Person means.

So Subject got a shape of its own, `Shape::Subject`, resolved by
`matte::subject` on the whole frame, and Background is it inverted. Subject's
agreement with the model alone rose from 0.811 to 0.956 over the 150 (40
better, none worse); of the 24, 22 now select what the photograph is of. The
Person chip, walked through the same harness, came out identical on all 69
frames it applies to. Stacks saved before read back as the new shape:
`Mask::upgrade` runs on every read (a `remote = "Self"` derive, so the field
list is still written once), and it recognises the old chip by what it was —
person and animal classes, named "Subject", or "Background" inside out — which
nothing else is. Auto's subject lift, which was the same `Segment` named
"Subject", is the same shape now, and still only where a person or animal was
named.

## Switching photographs, measured — 27 September

The photographer: switching to another photograph takes about half a second,
a second with a lens profile, and should feel instant. Measured on ten frames
(5D Mark III, R5, 5DS, A6000, A7R III, Z 6, Z 7, two E-M1 II with the 12-40,
X-T5), release build under Xvfb, each run on a quiet machine, before and after
back to back, two passes, medians. `NUMA_TIMING=1` now prints a line per decode
with every stage and how many cores it kept busy (PERF-020), one for the main
thread's part of an opening, and when the first and the first sharp frame went
up after the photograph was asked for.

**Where the time went** (cold opening, ms, before → after):

| stage | cores | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| read | 1 | 25 | 21 | 21 | 25 | 22 | 24 | 21 | 28 | 26 |
| rawler decode | 1–1.5 (CR3 4, ARW/RAF 10) | 100 | 190 | 240 | 8 | 13 | 88 | 169 | 140 | 130 |
| camera profile (first of a body) | 1 | 54 | 53 | 55 | 56 | 54 | 55 | 53 | 56 | 54 |
| demosaic (incl. black/white) | 11 | 109 → 74 | 254 → 151 | 275 → 191 | 123 → 82 | 238 → 158 | 119 → 82 | 250 → 161 | 107 → 68 | 668 |
| flatten + baseline | 1 → 14 | 29 → 8 | 84 → 21 | 84 → 24 | 43 → 9 | 81 → 20 | 36 → 10 | 74 → 21 | 36 → 7 | 74 → 17 |
| lens lookup (first in a session) | 1 | 24 | 25 | 26 | 26 | 35 | 27 | 29 | 30 | — |
| vignetting | 15 | 10 | 21 | 24 | 11 | 19 | 11 | 22 | 9 | 19 |
| geometry (distortion + TCA) | 15 | 75 | 153 | 172 | 83 | 149 | 82 | 165 | 70 | 139 |
| false colour | 15 | — | — | — | — | — | — | — | — | 147 |
| exposure match | 1 → 10 | 63 → 12 | 130 → 26 | 151 → 29 | 32 → 13 | 49 → 24 | 64 → 13 | 130 → 27 | — | 86 → 23 |
| downscale to the proxy | 14 | 12 | 24 | 27 | 13 | 23 | 14 | 26 | 11 | 27 |
| colour stage (main thread) | 1 | 3 | 11 | 3 | 24 | 25 | 11 | 12 | 28 | 4 |
| panel (main thread) | 1 | 58 → 3 | 56 → 3 | 54 → 3 | 55 → 3 | 54 → 3 | 57 → 3 | 56 → 3 | 58 → 3 | 55 → 3 |
| first render, draft then sharp | 16 | +141 → 0 | +149 → 0 | +146 → 0 | +138 → 0 | +143 → 0 | +143 → 0 | +147 → 0 | +140 → 0 | +144 → 0 |

The texture upload is under 20 ms from the render being handed over to the
frame being painted (cairo under Xvfb), and the catalog read is under 1 ms.
Nothing in the pixel pipeline runs on the GPU; the models are the only thing
that does.

**What was wrong.** The critical path was the decode, and inside it two kinds
of waste. *Copies:* rawler's `develop_intermediate` borrows the frame, so it
cloned the whole mosaic, copied the scaled mosaic again to demosaic it and
gathered the crop into a third buffer on one thread, and `into_flatten` copied
the result once more, serially — a third of the demosaic and three quarters of
the "baseline" line. *Waiting:* the camera's JPEG, which the exposure match
needs one number from, was decoded on one core after the raw pipeline had
finished, and its median was a sort of a million samples. Then two things in
front of the first frame that were not the decode at all: the profile
picker's scan of every installed `.dcp` (80 MB) on the main thread, repeating
the decode's own scan; and the first render always being the half-size draft,
because the panel's writers asked for renders of their own and the last
request fell inside the settle time that tells a drag from a click — the
sharp frame followed 140 ms later on every photograph.

**What changed**, each kept to float-for-float the same output — the decode,
the proxy and the default render hash identically on all ten frames before and
after, and a screenshot of a frame reached through the new path differs from
the old one in 0 pixels:

- `ppg_develop` runs rawler's own steps for a Bayer frame without the copies,
  and `flat` reinterprets the pixels in place;
- the camera's JPEG is decoded on a scoped thread beside the raw, and the
  median is selected rather than sorted;
- the `.dcp` scan is made once per body and shared;
- an opening starts a fresh run of render requests, so its first frame is the
  sharp one;
- the next photograph in the direction of travel is decoded as soon as this
  one is on screen (`prefetch`), and a photograph that was not gets its cached
  thumbnail on the canvas while it decodes.

**Where it stands** (first frame / sharp frame after the photograph was asked
for, ms):

| | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | X-T5 |
|---|---|---|---|---|---|---|---|---|---|
| step, before | — | 1015 / 1161 | 1175 / 1321 | 463 / 605 | 763 / 907 | 555 / 698 | 990 / 1135 | 546 / 695 | 1413 / 1554 |
| step, after (decoded ahead) | — | 43 | 37 | 56 | 55 | 45 | 44 | 60 | 37 |
| jump, before | 600 / 741 | 1050 / 1199 | 1173 / 1319 | 541 / 679 | 821 / 964 | 629 / 772 | 1053 / 1200 | 616 / 756 | 1451 / 1595 |
| jump, after | 13 / 427 | 13 / 741 | 13 / 917 | 13 / 389 | 13 / 598 | 13 / 462 | 13 / 750 | 13 / 502 | 13 / 1302 |

A jump's 13 ms is the cached thumbnail; its second number is the sharp frame.
The lens does not double the time: lookup, falloff and geometry are 110–120 ms on
a 24 MP frame and 200–215 on a 45 MP one — a quarter of the decode. The doubling
the photographer saw is a 45 MP body against a 24 MP one.

**What it costs.** A photograph opened and not stepped on from pays one decode
it did not need, in the background — 0.4 to 1.3 s of every core, once. If the
photographer jumps elsewhere while it runs, two decodes overlap: about a
gigabyte more at the peak on a 45 MP frame. The thumbnail standing in is the
camera's picture for an untouched frame, before lens correction, so the swap
to the render is a small shift in framing and colour, as in Lightroom.

**What is left.** On a jump, the decode itself: rawler's CR2, NEF and ORF
decoders are one thread (90–250 ms), and the X-T5's Markesteijn is 680 ms on
eleven cores. The camera-profile scan is still 54 ms the first time a body is
seen in a session, and the lens database 25 ms the first time anything is. On
a step, the colour stage (3–28 ms) still runs on the main thread; it could be
made ahead too, with the neighbour's stored document.

**The GPU**, written up rather than started. What a GPU could take is
everything after the decoder: demosaic, baseline, falloff, geometry, false
colour, downscale — 450 of the 5DS's 917 ms and 1 010 of the X-T5's 1 302,
all memory-bound passes over a frame that a mid-range card does in 3–5 ms
each. Uploading a 50 MP mosaic is about 100 MB. Expected: a jump to a 5DS
frame at about 500 ms, an X-T5 at about 320, and a 1:1 full-resolution decode
faster by the same amount. Against that: stepping is already under 60 ms,
which the GPU does not improve; a shader cannot be float-for-float the CPU's
answer, so every stage needs a tolerance and a reference comparison instead of
a hash; and the laptop this is for has to be measured first — the rule of the
sliders' evening. Worth it for the jump and for 1:1 once the decoder's own
single thread is looked at, not before.

## Saying Numa is busy — 28 September

The photographer: "Ik denk dat ondanks het allemaal sneller kan we voor de ux
toch overal loaders toe moeten voegen zodat de gebruiker ziet dat Numa bezig
is en even wacht." UX-015 had one loader for everything — a toast with a
spinner after 400 ms. That stays the loader for work that belongs to no place.
UX-025 adds the rules below, and a spinner in the place a wait belongs to.

**The rules.**

- Nothing for the first 400 ms (`BUSY_AFTER`), as before: most waits are
  shorter, and are getting shorter.
- Once up, up for at least 200 ms (`LINGER`), toast or spinner. A wait of
  450 ms used to put a toast up for 50.
- In place when the wait has a place: the photograph, the Masks tab's chip
  row, the button that was pressed, the loupe's caption, the card being read.
  The toast for what has none, and for the long passes that are started from
  a menu and are worth naming (Refine edge, Refine hair, a merge).
- A count and a bar when the total is known — export, import, thumbnails,
  Analyse, downloads, AI denoise — with Stop where the job can stop between
  items (PERF-003).
- A spinner turns only while it is mapped: tied to map and unmap rather than
  to being asked for, so one in a queued toast, on a page not shown or in a
  row scrolled away costs nothing. Each has an accessible label, and the
  region it speaks for is marked busy while it is up.
- Nothing polls to drive feedback that was not polling already: a place's
  spinner is two one-shot timers per wait, the thumbnail count is told by
  the queue when it moves, the import's by the copy as each file lands.
- Nothing moves the layout: Auto's spinner takes the word's place in a stack
  as wide as the word, and the loupe's has a slot kept on both sides of the
  caption. The badge is an overlay.

**How a place is held.** `Waiting` is the spinner and a count of holds. A
wait takes a `Hold`, and the place stays busy until the last is dropped — so
two mask jobs over the photograph keep it up until both are done, and a job
that ends any way at all, an early return or a panic in the worker, lets go.
The photograph's is in the zoom badge, which is where the design system has
the canvas say "loading"; the opening holds it until its own sharp frame is up
(`render.coming`), the original at 1:1 until it has been rendered, not only
decoded.

**What waits, and what it shows.** Release build, the ten frames of the
switching run and a 2 000-photograph library of hard links, under Xvfb;
"two cores" is the same build under `taskset -c 0,1`, standing in for the
laptop.

| wait | how long | before | now |
|---|---|---|---|
| opening a photograph, not decoded ahead | 5DS 830 ms, 2.07 s on two cores; the thumbnail stands in at 10–15 ms | thumbnail, "Opening…" toast | thumbnail, spinner in the badge until the sharp frame |
| stepping to the one decoded ahead | 37–60 ms | nothing | nothing |
| 1:1, the original | 5DS 3.5 s on two cores | "…", and "Decoding the original…" | "…" and the spinner, until the full frame is up |
| a slider | draft 7–30 ms, sharp 44–200 ms on two cores | nothing | nothing |
| Masks: what is in the photograph | under a second, 1–2.5 s on two cores | the chip "Looking at the photograph…", and a toast saying it again | the chip with a spinner in it; on the photograph too only if a mask there waits |
| a found mask's pixels, a click, a closer look, the click model | 0.5–2.6 s | a toast each | the badge |
| Auto | ~0.5 s; its frame 824 ms on the main thread on two cores with a decode running | a toast at once, the window frozen | the spinner in the button; the frame on the worker |
| Looks cards | up to 814 ms a card on two cores, on the main thread | empty cards, the window held 1.4 s | cards fill in one by one from a worker |
| thumbnails of a new library | 10 raws ~3 s on two cores; an edit's render 0.5–1.8 s | "Making thumbnails — n of N" from a 100 ms poll | the same toast, told by the queue; the grid marked busy |
| export | ~1 s a file | "Exporting 3 of 10…" as text, and a loader per file never seen | count, bar and Stop; one plain file the loader after 400 ms |
| import from a card | 30 files, 1 GB, 1.5 s | text read every 250 ms; the walk after it on the main thread | count and bar moved by the copy; the walk beside it; "Reading the card…" turns |
| Move to Trash | a rename locally; a copy per file on a share | on the main thread, nothing shown | beside it, the loader after 400 ms |
| adding a folder, opening a file from outside, dropping files in | a new folder of 2 001: 3.9 s, reading every file's date | on the main thread, the dialog frozen on screen | beside it, "Reading the folder…" |
| the loupe at 1:1 | 0.8–1.5 s on two cores | "developing at full size…" | the same, with a spinner |
| AI denoise, downloads, Analyse | minutes | count, bar, Stop | unchanged |

**What is still on the main thread**, found with a 10 ms watchdog on the main
loop (not committed; it is a dozen lines):

- The first frame a mask, a pipette or a retouch spot needs (`mask_frame`) is
  still made where it is asked for — the colour stage and the geometry over
  the proxy. Tens of milliseconds on sixteen quiet cores; on two with the
  next photograph decoding, 824 ms, because it calls into the rayon pool the
  decode is using. Auto's is on its worker now; the pipettes', retouch's and
  a new mask's are not.
- The colour stage of an opening (`to_working_space`), 3–74 ms, for the same
  reason worth watching on a laptop.
- Building the grid: 55 ms for 2 000 photographs, and 180 ms from building the
  window to its first frame. On a network drive the thumbnail sizes it reads
  are the network's.
- Adding and removing a mask (`busy_sync`): a raster and an outline of about
  9 ms each (UX-015's measurement), behind a toast put up first.

**Kept as they were, and why.** The half-second tickers of AI denoise and an
export's model passes, and the quarter-second ones of Analyse and downloads,
run only during a job that holds every core or the network, which they cost
nothing beside; downloads read curl's file as it grows, which has nothing to
push. No spinner per thumbnail card: sixty spinners turning over a grid is
the cost this round is against, and the count says more. No spinner per
preset card, for the same reason. libadwaita 1.6's `AdwSpinner` does what
`spinner()` does by itself, but the floor is 1.5 (`v1_5`, deliberately); the
Flatpak's runtime and the AppImage both carry 1.9, so it is a one-line change
the day that floor moves.

**The Apple clients, planned, not built.** Read on 28 September from
`Numa-mac-next` (`next-numa`) and `Numa-mac-port` (`mac`); the Mac draws the
iPad's views, so the plan for one is the plan for both. Deployment target iOS
and macOS 26, so every `ProgressView` style is there. One modifier in
`Theme.swift`, the Linux rules in SwiftUI — a `ProgressView` in place after
400 ms, up at least 200, `.accessibilityLabel` on it — used everywhere below
instead of the delays each view has now (the mask loader's 300 ms, nothing at
all elsewhere).

- *Opening* (`EditorView` 870): a large spinner on black at once, and a step
  in the filmstrip makes a new model, so every step is blank again. The
  grid's thumbnail stands in, and the spinner goes to the zoom badge's place
  at the top centre on `ground-hud`, until the sharp render — as here.
- *Masks after an opening, a tile at zoom, Before*: nothing says a render is
  coming (`rendering` is private), and Before shows the edited picture under
  its badge until the original renders. The same badge; the loupe's
  "Developing at full size" already does this for the tile.
- *Auto* (`AutoNotice`, at once): in the Auto chip, in the word's place.
- *Dust, Remove people*: a note and a live button; a spinner in the row and
  the button off while it runs. *Photo info* says "No camera metadata in this
  file." while it loads: a spinner, not a false empty state. The profile
  picker is empty until its choices arrive: a spinner in its row.
- *Counts*: a batch export has N and stops between items, so "n of N" with a
  bar and Stop; a plain export's bar moves only during AI passes, so it stays
  indeterminate but should not offer a Stop it ignores. "Saving to Photos"
  has a `Work` it does not show: its fraction and Cancel. Copying into a
  library: the file count. Downloads: Cancel.
- *On the main actor*: `autoLevel`, `autoPerspective`, `pickNeutral`,
  `nameAt`, `readValues`, `carryMasks`; paste, preset, Move to Trash and batch
  from the grid (a catalog round trip each); the Mac's Share save copying
  large TIFFs; `Catalog.openDefault` at launch. To detached tasks, with the
  spinner of the button that started them. Two causes are in the FFI rather
  than the views: the catalog is one worker thread, so a main-actor call
  waits behind a rescan; and `masks` is locked for the whole of a model run,
  so `name_at` on a click waits seconds for it.

## Opening photographs faster, and with less energy — 28 September

Picked up from "What is left" above, overnight, CPU side only (the GPU is a
branch of its own). The photographer asked for speed and then, the same
night, for no hot phones: "zorg dat het dus echt extreem efficiënt is". So
every change was judged twice — wall time, and the processor time it costs
(user + system, from `getrusage`; RAPL's joules are root-only on this
machine) — and preferred when it does less work rather than the same work on
more cores. The output is the rule it always is: decode, proxy and default
render hash the same on the ten frames before and after, and every decoder
change hashes the same mosaic on the 502 raws of every make from
raw.pixls.us (`dev/fetch-corpus.sh`'s source, in `raws-cc0`).

**Where the time went now** — rawler's decode alone, one frame, medians of
three runs of five, wall / processor ms, before → after:

| | 5D3 CR2 | 5DS CR2 | Z 6 NEF | Z 7 NEF | E-M1 ORF |
|---|---|---|---|---|---|
| rawler 0.8.0 | 99 / 98 | 239 / 239 | 88 / 87 | 168 / 168 | 138 / 138 |
| one thread, PERF-021 | 76 / 76 | 177 / 176 | 54 / 54 | 106 / 106 | 104 / 104 |
| four threads, PERF-022 | 20 / 76 | 53 / 174 | 23 / 85 | 48 / 169 | — |

**The decoders (PERF-021, PERF-022, PERF-027).** rawler is carried in the tree
now (`vendor/rawler`, LGPL-2.1, every change in its `NUMA-CHANGES.md`), as a
path in the workspace's dependency table rather than a `[patch]`, so the Apple
workspace, which reads that table, builds the same decoders. On one thread:
the bit reader refills eight bytes at a time instead of four through an
iterator, and the Huffman cache is a `u32` an entry over 12 bits — 16 KB
where it was 48, measured against 11 and 13 bits. On four: CR2 and NEF have
no restart markers, but a Huffman code resynchronises within a few codes, so
the stream is cut into four parts, each read from its own first byte, and a
join is where one part's reading meets a code boundary the next noted. From a
shared boundary both readings are the same, so the codes are the single
reader's exactly; a stream that does not join falls back to one reader. Two
things decided the shape. The parts run on a pool of their own: in the global
one every split woke all sixteen threads and the idle ones spun, a third more
processor time for the same work. And four parts, not sixteen: eight were no
faster and cost 40 % more, sixteen saved 5 ms for 2.3 times the processor
time — past four the parts only compete for memory. A CR2 is decoded straight
into its vertical fields, which drops a 100 MB frame and a serial copy of it;
a NEF's dithering random number is a multiply-with-carry generator, which is a
multiplication modulo 15700·2¹⁶ − 1, so each row's starting value is a power
away. ORF's predictor and the PPG demosaic choose between candidates with
selects rather than branches the noise keeps mispredicting (ORF 133 → 104 ms,
PPG 15 % less processor time). A closed form for ORF's code-length loop was
tried and was slower.

**The profile scan (PERF-023).** Automatic's profile was found by reading
every installed `.dcp` whole — 160 MB with RawTherapee's set both downloaded
and installed, 105 ms on the first photograph of each body, measured again
tonight at 104–141 ms (the note above had 54). The header names the camera
and the profile in a few hundred bytes; only a file whose header could be the
one is read whole, and the choice among those is made as before. 1–3 ms, so
warming it at library open is no longer worth a thread.

**The rest of the decode (PERF-028, PERF-029).** The lens geometry read its
three tables at every pixel, finding the same window three times, and a pixel
and its mirror across the centre line — exactly the same distance out — did
it again: found once each now, geometry 20 % faster and falloff 40 %.
Markesteijn works in tiles that compute a 24-pixel margin they throw away;
at 64 pixels that was 2.6 times the pixels kept, at 128 it is 1.5, and a
pixel depends only on its neighbourhood so the picture does not change
(identical on 70 RAFs). Its green bounds, a single-threaded pass over the
whole frame, now go row by row on all threads. The X-T5's demosaic 634 → 426
ms.

**Stepping (PERF-025, PERF-026).** The colour stage is made off the main
thread on every opening, and by the decode ahead with the neighbour's stored
edits; the opening takes it when the edits are still what they were (compared
as the catalog holds them). The main thread's part of a step is the panel, 12
ms. LensFun's database is read at startup, off the main thread.

**Where it stands** (first frame / sharp frame after the photograph was asked
for, ms, and the processor time of the whole process meanwhile; release build
under Xvfb, one quiet pass, before = 030913c):

| | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| jump, before | 436 | 717 | 812 | 367 | 544 | 442 | 706 | 480 | 477 | 1239 |
| jump, after | 292 | 623 | 537 | 294 | 451 | 311 | 517 | 352 | 351 | 958 |
| its processor time, after (s) | 2.7 | 5.9 | 5.9 | 3.2 | 5.0 | 3.1 | 5.6 | 2.7 | 2.8 | 12.5 |
| decode + proxy + render, processor s, before → after | 2.5 → 2.2 | 5.6 → 5.1 | 5.9 → 5.3 | 2.6 → 2.3 | 4.8 → 4.3 | 2.7 → 2.5 | 5.3 → 4.8 | 2.2 → 1.9 | 2.2 → 1.9 | 13.3 → 11.7 |
| step, before | — | 48 | 35 | 57 | 48 | 44 | 45 | 64 | 67 | 47 |
| step, after | — | 49 | 48 | 44 | 50 | 45 | 50 | 45 | 46 | 46 |

First and sharp frame are the same frame on every opening (PERF-020), so one
number each. A step is the photograph after in capture order, decoded ahead;
its time is the render and the texture, 44–50 ms either way (the 35 of the
5DS before is one lucky run), and its processor time 0.2 s. The main thread's
part of a step went from 15–40 ms to 12 (the colour stage left it). The row
of processor-seconds is the same work outside the app (`rawbench full`),
since the build before did not print it. A screenshot of a step reached
through the new path and of a jump to the X-T5 differ from the old ones in
nothing on the canvas; one card in the filmstrip differs by a fraction of a
level at its focus ring, which is the animation's moment, not the picture.

**The energy side (PERF-024).** `numa_core::power` holds one flag, `frugal`:
on Linux the power-saver profile (GIO's monitor, the portal in a Flatpak) or
running on battery (UPower, where the system bus is reachable); on Apple the
bridge calls `set_frugal`, meant for Low Power Mode and a serious or critical
thermal state. Frugal, nothing is decoded ahead until two steps the same way,
nothing is warmed up (the lens database waits for the first raw), and
background work runs on a quarter of the cores. The GPU branch and the
"doing less" branch read the same flag.

A decode ahead is speculative work, and what it costs when nobody steps to
it is the number to watch: 2.0 processor-seconds for an E-M1 frame, 2.5–2.7
for a 24 MP one, 4.5–5.7 for a 42–50 MP one, 12 for an X-T5. Before tonight a replaced decode ran on to the
end beside the one that replaced it; now it is stopped at its next stage
boundary (the longest stage is Markesteijn, 0.4 s) when another photograph is
opened, when a newer one replaces it, or when the editor is left, and it
runs on its own pool at nice 10 so nothing the photographer waits for queues
behind it — which is what the loaders branch found on two cores, where the
first mask of a photograph waited 824 ms behind the neighbour's decode.
Pinned to two cores (`taskset -c 0,1`), opening the 5D3 and stepping: 1039
ms and 195 ms before, 860 and 87 after; the Z 7 and a step to the Z 6: 1902
and 578 before, 1590 and 84 after. The frugal path itself — the run of two
steps, the quarter pool — is covered by a unit test for the run and was not
run under a real power-saver profile tonight.

What a decode ahead is worth in a session, from those numbers: stepping
straight through a shoot wastes one decode at the end of the run; every jump
that is not followed by a step wastes one; a turn back wastes one. Frugal,
the jumps and turns cost nothing extra and the first two steps of every run
decode as a jump would.

**For the Apple apps.** Everything in the shared crates reaches them: the
decoders through the workspace table (their `Cargo.lock` will see rawler move
from the registry to a path, so `dev/check-apple.sh`'s `--locked` wants one
unlocked build first, and the relink kit now copies `numa/vendor/rawler`,
which is the modified source the LGPL asks for), the profile scan, the lens
and demosaic work. The switch needs one line in the bridge —
`#[uniffi::export] fn set_frugal(on: bool) { numa_core::power::set_frugal(on) }`
— fed from `ProcessInfo.isLowPowerModeEnabled` and a `thermalState` of
`.serious` or `.critical`, both of which post a notification when they
change. The background pool's lower priority is Linux-only; on Apple the
counterpart is a utility quality of service for those threads.

**What is left**, with numbers:

- *The global pool.* Sixteen threads on eight cores: the 5D3's whole decode
  is 202 ms and 2.5 processor-seconds; on four threads 394 ms and 1.5, on
  eight 243 ms and 1.7. The extra is the second thread of each core and idle
  threads spinning between the many short parallel passes (the decoder's own
  pool showed the second: a third more for the same work). A pool per stage, or the
  global pool at the number of physical cores, would trade a little wall time
  for a lot of processor time — the photographer's call, and to be measured
  on the laptop, since joules are what matter and a sibling thread costs far
  less than a core.
- *The X-T5.* Its decode is still 0.87 s and 11.7 processor-seconds: the
  Fuji decompressor 124 ms at ×9.8 (to try first: its `read_code` sits
  behind `multiversion`, so it is dispatched per sample and never inlined),
  Markesteijn 426 ms,
  false colour 145 ms, geometry 110 ms.
- *ORF thumbnails.* rawler reads no preview from an ORF, so every ORF
  thumbnail is a full decode, lens correction included, at 2–11
  processor-seconds each (two ran at every start of the harness until the
  cache was warmed). The camera's JPEG is in the Olympus makernote
  (CameraSettings 0x0101/0x0102). Reading it would change what an ORF's card
  shows (the camera's rendering instead of Numa's), so it is left for the
  photographer to decide.
- *The step's render* is 42 ms and 0.2 processor-seconds for a 1920-pixel
  proxy — worth a look with the same eye as the decode.

## The decode on the graphics card — 28 September

RENDER-010, started from the brief the section above ends with. Everything
between rawler handing over the sensor's counts and the editor getting its
proxy now runs on the card: black and white, the demosaic (PPG for Bayer,
Markesteijn for X-Trans), the baseline lift, the lens's falloff and
geometry, X-Trans false colour, the samples the exposure match reads, the
turn and the shrink to the proxy. 1:1 and export get the whole frame the
same way. It is `crates/numa-gpu` (wgpu 30, Vulkan; WGSL shaders), behind
`numa-io`'s `gpu` feature, which only the Linux app turns on — the shared
crates build for Apple unchanged, and nothing of wgpu is linked there.

**The shape.** The mosaic goes up once, as the sensor's own 16-bit counts
(half the bytes of floats); a pass scales it by a table of rawler's own
black-and-white arithmetic, the demosaic writes three planes, the lens
bends them into a second buffer, false colour cleans them back into the
first, and only then does anything come back: every 37th pixel for the
exposure match (the camera's JPEG is still read on the processor, beside
the raw), then — lifted by the factor that match makes — the proxy, turned
upright and box-averaged over exactly the boxes `downscaled` uses, or the
whole frame. A 50 MP or 40 MP frame is about 1.8 GB on the card at the peak; between
photographs the process holds 27.5 MB of it (amdgpu's fdinfo), 14 of which
are the device and its compiled pipelines.

**Measured** as the section above was: release build, the same ten frames,
Xvfb, an isolated copy of the catalogue, the same binary with `NUMA_GPU=0`
for the processor's path, interleaved runs, two passes, medians — on the
RX 9070 XT (RADV, Mesa 26.2) this machine has, while two other sessions
built and measured under the same machine lock.

**Where the time went** (cold opening, ms, processor → card; the card's
column is the whole develop, lock, upload and read back included):

| stage | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | X-T5 |
|---|---|---|---|---|---|---|---|---|---|
| demosaic (incl. black/white) | 74 | 146 | 169 | 76 | 143 | 79 | 151 | 67 | 641 |
| flatten + baseline | 8 | 20 | 24 | 10 | 19 | 9 | 21 | 8 | 19 |
| vignetting | 10 | 21 | 24 | 11 | 19 | 11 | 21 | 9 | 18 |
| geometry (distortion + TCA) | 71 | 142 | 164 | 78 | 136 | 78 | 146 | 64 | 128 |
| false colour | — | — | — | — | — | — | — | — | 140 |
| exposure match | 12 | 26 | 30 | 13 | 25 | 13 | 26 | — | 23 |
| downscale to the proxy | 10 | 16 | 24 | 11 | 16 | 11 | 20 | 8 | 16 |
| **all of the above** | **184 → 20** | **372 → 30** | **433 → 32** | **198 → 20** | **358 → 32** | **200 → 20** | **386 → 30** | **156 → 19** | **985 → 62** |
| of which upload | 6.2 | 7.3 | 8.7 | 6.3 | 8.3 | 6.2 | 8.6 | 5.2 | 8.2 |
| of which the passes | 8.6 | 15.1 | 15.0 | 8.7 | 14.8 | 8.8 | 13.7 | 8.7 | 45.1 |
| of which samples + match | 2.0 | 3.6 | 4.0 | 1.7 | 3.7 | 1.8 | 3.6 | 0.9 | 3.8 |
| of which proxy + read back | 3.1 | 4.5 | 4.4 | 3.2 | 4.3 | 3.6 | 4.1 | 3.8 | 4.2 |
| rawler decode (unchanged) | 100 | 189 | 239 | 8 | 13 | 88 | 169 | 141 | 130 |
| **the whole decode** | **408 → 223** | **666 → 323** | **808 → 373** | **323 → 130** | **528 → 154** | **420 → 213** | **665 → 308** | **426 → 284** | **1209 → 266** |

Inside the passes (`NUMA_GPU_PROFILE=1`, each waited for): a 5DS is
scale 4.2, PPG 1.4 + 2.4, geometry 2.5, samples 1.1, proxy 2.2 ms; an X-T5
is scale 4.2, Markesteijn 3.8 + 5.8 + 6.3 + 5.4 + 6.8 + 1.7 + 4.0 (green,
solitary greens, red/blue, 2×2 greens, Lab gradients, votes, the average),
geometry 1.9, false colour 2.7, samples 0.9, proxy 2.0.

**Where it stands** (first frame / sharp frame after the photograph was
asked for, ms; the two are one frame since PERF-020):

| | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | X-T5 |
|---|---|---|---|---|---|---|---|---|---|
| jump, processor | 453 | 719 | 852 | 385 | 595 | 471 | 720 | 494 | 1260 |
| jump, card | **262** | **374** | **414** | **193** | **219** | **261** | **361** | **352** | **316** |
| step (decoded ahead), processor | — | 46 | 35 | 59 | 50 | 46 | 44 | 64 | 50 |
| step (decoded ahead), card | — | 47 | 37 | 56 | 49 | 43 | 48 | 65 | 48 |

The brief's guess was a 5DS at about 500 ms and an X-T5 at about 320: 414
and 316. Stepping is the colour stage and a render, as predicted, and does
not move. What is left of a jump is now mostly rawler's decode — one
thread on the CR2, NEF and ORF bitstreams — and, the first time a body is
seen in a session, the camera profile's scan (54 ms).

**1:1 and export** (`decode_linear_best`, the whole frame, outside the
app, medians of three, ms): 5D3 277 → 159, R5 534 → 280, 5DS 650 → 358,
A6000 200 → 72, A7R3 367 → 124, Z 6 284 → 154, Z 7 539 → 281, E-M1 292 →
193, X-T5 1098 → 253. Here the read back is the largest piece on the card
— 62 of a 5DS's 114 ms, 600 MB through a 64 MB window — because the frame
has to become a vector of floats on the processor's side.

**The same picture, within a tolerance.** A shader is not the processor's
arithmetic, so each stage is compared rather than hashed:
`the_card_develops_what_the_processor_does` runs both paths to the same
point — the demosaic alone, then with the falloff, the geometry, the false
colour, turned, and the proxy — and renders both proxies through the
renderer's defaults. Over the ten frames and the nine makes of the corpus:

| | Bayer (8 bodies, 9 corpus makes) | X-T5 |
|---|---|---|
| demosaic alone, max / mean | ≤ 4.8e-7 / ≤ 3e-10 (a few units in the last place) | 4.5e-2 / 6.8e-7 |
| finished frame, max / mean | 4.4e-5 – 2.2e-3 / ≤ 1e-6 | 1.9e-2 / 1.7e-6 |
| values more than 1e-3 apart | ≤ 0.0001 % | 0.027 % |
| rendered proxy, 8-bit | at most 1 level, on 0.007–0.03 % of values | at most 2, on 0.048 % |

(Scene-linear, where 1.0 is the sensor's white lifted by the baseline; a
maximum of 2e-3 is one pixel of the geometry's bilinear weights rounding
differently on a hard edge.)

The tolerances the test holds: a mean difference under 1e-5 of the lifted
white, under 0.1 % of values more than 1e-3 apart, and rendered proxies at
most 2 levels apart on under 0.5 % of values. A screenshot of the editor on
the same photograph through each path (X-T5, 5DS, A6000, 1920×1080):
the photograph differs by at most 1 level, on 0.02–0.06 % of the screen's
pixels; the histogram above it by up to 10 at a few bar edges, where a bin
count one sample different moves an anti-aliased edge by a pixel.

Two things made PPG the processor's to a few units in the last place, and
both were found by the comparison rather than foreseen. PPG decides between
directions on exact ties — flat areas of integer counts are full of them —
so one bit of difference in a green is a different green. The first bit
came from the black-and-white division, which the card does not round
correctly: the counts go through a table of rawler's own arithmetic made on
the processor (Markstein's correction, exact on the processor for every
count and 197 million pairs of levels, was not on the card: the driver may
split an `fma`). The second came from `a * 3.0 + b`, which the compiler
fuses into one rounding where the processor rounds twice; `a + a + a` is
the same number and cannot be fused. Markesteijn is not bit-exact — its
homogeneity vote compares Lab gradients, and a near-tie can tip — but a
tipped pixel is a single pixel, and at most one in six thousand values
moves by more than 1e-3.

**What stayed on the processor, and why.** rawler's decode itself — a
bitstream, sequential by nature, and the other half of every jump now
(90–240 ms on the CR2, CR3, NEF and ORF decoders); the camera profile, the
lens lookup and the metadata, which are file parsing; the camera's JPEG and
the median of the 1.3 million samples the exposure match takes (3–5 ms);
thumbnails, culling and HDR merges, which decode in the background on the
draft route — none is waited on, and one holding the card would send the
photograph being opened to the processor; and anything the card does not
take: a linear DNG, a four-colour or SuperCCD sensor, a frame that needs
more than 3 GB of the card. Every pass in between moved, and none lost:
the slowest, Markesteijn, is 45 ms of the card against 644 of eleven cores.

**Keeping the desktop and the models whole.** The card that develops is the
card that draws the photographer's desktop, so nothing is asked of it in
one long piece: every pass runs over tiles of rows, one submission a tile,
about 8 MP each (1 MP on an integrated GPU) — a few milliseconds.
Markesteijn works in bands of about 8 MP with rawler's 12-row margins. Every loop a
table drives is bounded; a frame that needs more than 3 GB of the card, or
a larger buffer than it binds, stays on the processor; `develop` runs
inside an out-of-memory and a validation error scope, and anything they
catch, a panic, a ten-second wait, or a lost device (after which the card
is not asked again that session) is the processor for that photograph. The
develop takes the models' lock (`numa_infer::try_card`) without waiting, so
it and a model are never on the card together — the rule the models keep
with each other since two of them met in RADV on 21 September — and two
develops queue rather than one falling to the processor. The device is made
at start-up off the main thread with darktable's marker around it, the one
the models use: a start that dies in the driver switches GPU acceleration
off for the next, which says so, and Preferences' switch — now there
without the models' download — turns both back on. No shader or GPU fault
was logged by the kernel in any run tonight.

**Energy.** The photographer, 28 September: speed, but no hot laptops —
and energy is a goal equal to speed. Measured per jump over the ten
frames, above idle: the card's board power (`power1_average`, sampled every
50 ms) and the processor package's (the socket power the APU's SMU
reports in `gpu_metrics`; RAPL needs root here, and this reads 33 W idle
and 105–148 W under an all-core decode, as a 9800X3D does). Back to back
is a photographer jumping through a shoot; 1.5 s apart is one jump at a
time, where the card spends the second after it clocking down.

| per jump | back to back | one at a time |
|---|---|---|
| processor path | 29.0 J (package), 4.9 core-seconds | 27.9 J |
| discrete card (RX 9070 XT) | 3.4 J package + 5.6 J card = **9.0 J**, 0.42 core-seconds | 3.8 + 12.5 = **16.3 J** |
| integrated GPU (frugal) | **7.5 J** package (the iGPU is in it) | **8.1 J** |

Both paths on the card cost less energy than the processor's, so the card
is the default. Frugal takes the integrated GPU: a third of the energy of
the processor and not slower — jumps of 155–607 ms against 301–778 on the
Bayer frames, the X-T5 level at 1.18 s, where Markesteijn's 954 ms on two
compute units is most of it. A frugal machine without an integrated GPU,
or a discrete card only, decodes on the processor; no discrete card is
ever woken for frugal. The power switch that says frugal is not on this
branch: `numa_gpu::warm_up` and `develop` take the flag, and until the
switch is merged `NUMA_GPU=low` asks for it by hand. Nothing is submitted
unless a photograph is being developed; the waits are the driver's fence,
not a loop.

**What the Apple apps would need.** wgpu's Metal backend and the same
WGSL (naga translates it to MSL): the `metal` feature for Apple targets in
`numa-gpu`, and numa-ffi turning `numa-io/gpu` on. Four things to settle
there before it is the default, each a measurement on the device: Metal's
largest buffer (`maxBufferLength`) is smaller on iPhones than a 50 MP
frame's three planes (600 MB), so the planes need binding separately or
storing as half floats; the 3 GB budget becomes a fraction of the device's
memory, because iOS ends an app for its footprint; unified memory makes
the upload and the read back unnecessary (the mosaic can be mapped where
the card reads it — worth doing there, where it was not here); and a
develop asked for while the app is in the background fails, which the
fallback already turns into the processor. The comparison test runs on the
self-hosted MacBook as it is. The licence question in the next paragraph
applies to the closed Apple apps with more weight than to the open Linux
one.

**A licence question for the photographer.** `demosaic.wgsl` translates
rawler's PPG and Markesteijn step for step — its borders, its tile-width
quirk — because that is how the card's picture came to match the
processor's. The algorithms are Chuan-kai Lin's and Frank Markesteijn's as
dcraw published them, but a translation that close is rawler's code in
another language, so the file says LGPL-2.1, as rawler does. Numa links
rawler under the same licence already; whether a shader file changes what
the licence audit has to say is for the audit (the Mac repository's
`docs/LICENCES_AUDIT.md`).

## Developing at the size that is shown — 28 September

The photographer, the night after the switching measurements: faster, and
"echt extreem efficiënt" — no hot iPhone, no drained battery. The switching
work of the 27th made each stage cheaper; this looked for work that did not
need doing at all. Doing less is the one speed-up that is also an energy
saving.

**The finding.** Every opening developed the whole frame — 24 to 50
megapixels through demosaic, baseline, falloff, geometry, false colour and
the exposure match — and then averaged it down to a proxy of 1920 × 1280 and
threw the frame away. Nothing kept it: `prefetch::prepare` returned the proxy
and the full size, and everything that wants the photosites (1:1, the
export, the loupe's 1:1, tracing hair, AI denoise, an HDR merge) decodes
again, on demand, as it always did. So each opening paid for twelve to twenty
developed pixels per pixel it kept.

**`raw::proxy_from_mosaic` (PERF-030; `decode_proxy` on its branch)**
averages the mosaic straight into the proxy. For each proxy pixel and each
colour:

- the footprint is `LinearImage::downscaled`'s own — `[floor(x·r),
  ceil((x+1)·r))`, whole photosites, so the proxy has the old size to the
  pixel and its box is the old box;
- it is read from where the lens profile says that colour landed:
  `correct_geometry`'s pull-in (`lens::geometry_fit`, moved out for this),
  and `source_radius` per channel, so lateral CA still comes out;
- the falloff is the profile's gain at that radius;
- and every photosite is spread by a tent before the box weighs it — two
  photosites for red and blue, one for green.

The tent is the part that took measuring. A bare box — the mean of the
photosites of one colour in a footprint exactly `r` wide, the first
version — came out 4 to 26 % *more* acute
than the old proxy, and the extra was noise: on a flat grey wall the
difference image was all grain. A demosaic interpolates each missing colour
from its neighbours before the box ever sees it, so its box averages over
more than the footprint. Swept on five bodies:

| tent red/blue, green | acutance, new / old (5DS, E-M1, Z 7, A6000, X-T5) |
|---|---|
| none (box) | 1.16, 1.26, 1.12, 1.09, 1.11 |
| 1, 0.75 | 1.02, 1.03, 1.02, 1.01, 1.01 |
| **2, 1** | **0.99, 0.99, 1.00, 0.99, 1.00** |
| 2, 1.5 | 0.97, 0.94, 0.98, 0.97, 0.98 |
| 3, 2 | 0.94, 0.88, 0.95, 0.93, 0.95 |

Two and one it is, for Bayer and X-Trans alike (acutance as FT-021 measured
it: mean gradient magnitude of the rendered luminance over its mean). Nothing
else is Bayer- or X-Trans-specific: the CFA's period and which of its sites
are which colour are read from the file, so any three-colour mosaic goes
this way; four colours, a linear DNG, a SuperCCD or a frame no larger than
the proxy is handed back and developed as before. X-Trans loses nothing it
had: Markesteijn and the false-colour step exist for what interpolation
invents, and nothing is interpolated here.

**The picture** (`examples/proxy_ab.rs` in the scratch folder: old =
`decode_for_editing` + downscale, new = `decode_proxy`, both through the
default render at the editor's detail scale, 1920 proxy):

| 1920 proxy | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| mean difference, 8-bit levels | 0.46 | 0.33 | 0.86 | 1.06 | 0.33 | 0.22 | 0.96 | 0.74 | 0.93 | 0.68 |
| 99.9th percentile, levels | 8 | 5 | 6 | 12 | 11 | 4 | 22 | 9 | 14 | 10 |
| ΔE mean / 99th percentile | 0.67 / 2.4 | 0.48 / 1.9 | 1.22 / 4.1 | 1.35 / 5.4 | 0.45 / 3.7 | 0.33 / 1.5 | 0.94 / 7.7 | 1.13 / 4.7 | 1.47 / 5.4 | 0.83 / 4.5 |
| the same, both blurred 4× | 0.24 / 1.2 | 0.22 / 1.0 | 0.40 / 1.6 | 0.50 / 1.5 | 0.23 / 1.7 | 0.14 / 0.9 | 0.63 / 3.0 | 0.42 / 1.6 | 0.53 / 2.0 | 0.47 / 1.7 |
| acutance, new / old | 1.008 | 0.998 | 0.993 | 0.988 | 1.002 | 1.001 | 1.002 | 1.002 | 1.004 | 0.994 |
| linear level, green, new / old | 0.999 | 0.998 | 0.999 | 0.995 | 1.000 | 1.000 | 0.989 | 1.000 | 1.000 | 0.993 |

At a 2400 proxy (a HiDPI screen) the same picture: acutance 0.982–1.012,
mean difference 0.24–1.26 levels. And on the screen itself — the editor
under Xvfb, fit view, before and after, cropped to the canvas — E-M1, X-T5,
Z 7 and 5DS differ by 0.62, 0.52, 0.78 and 0.62 levels on average, ΔE 0.95,
0.64, 0.74 and 0.89 (99th percentile 3.8, 3.5, 5.6, 3.2); the largest pixel
difference on the Z 7, 51 levels, is one grain of a noisy patch that looks
the same at four times on either side. Outside the canvas only the
histogram differs.

The mean differences are almost all noise texture — two filters over the
same grain agree on its average and not on its pixels — which is why the
4×-blurred difference is a fifth to a half of the sharp one. The one
systematic part is the exposure match. `match_camera_exposure` takes the
median of every 37th pixel, and on the proxy those are smoother pixels: the
Z 7's factor came out 0.9656 instead of 0.9775, and that −1.2 % is its whole
linear difference; the E-M1, which has no camera JPEG to match, differs by
0.0000. A dark, noisy frame moves most. Under a fiftieth of a stop, and the
export and 1:1 keep their own factor, as before.

**What did not change.** `decode_for_editing`, `decode_linear_best` and
`decode_linear` hash as before on all ten frames — full frame, proxy and
default render — so 1:1, the export and the loupe are the same bytes. The
old route stays in `prefetch::prepare` behind `PROXY_FROM_MOSAIC`.

**Where the time went** (1920 proxy, one decode, ms and processor-seconds):

| 1920 proxy | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| decode + proxy, before | 288 / 2.6 | 557 / 5.8 | 689 / 6.1 | 209 / 2.8 | 392 / 4.9 | 296 / 2.8 | 575 / 5.5 | 306 / 2.3 | 307 / 2.3 | 1 141 / 13.6 |
| after | 147 / 0.85 | 224 / 1.5 | 298 / 1.2 | 63 / 0.87 | 80 / 0.94 | 143 / 0.90 | 228 / 1.1 | 203 / 0.99 | 203 / 0.99 | 195 / 2.2 |

(ms / processor-seconds, medians of three, file in the page cache, the
camera profile and lens database already read — what an opening costs once
a session has seen the body.) What is left of the "after" is mostly rawler's
decoder — 100–240 ms of one or two cores on the CR2s, NEFs and ORFs — and
the develop-small stage itself, 45–65 ms on sixteen threads. The stages it
replaces were 180 ms on the A6000, 410 on the 5DS and 990 on the X-T5.

The RAPL energy counters are not readable on this machine (root only), so
processor-seconds stand in for energy; on a laptop or a phone the two move
together.

**Not everywhere.** The stage costs per pixel of proxy, not per photosite,
so it wins by how much smaller the proxy is than the frame. At a 4096-pixel
long edge — what a 2048-pixel export develops at — it costs as much
processor time as the full PPG develop on the Bayer bodies (the 5D3's 2.9 s
became 3.3), and only the X-T5 still gains (13.8 → 5.1 s). So it is the
editor's, the grid's and the thumbnails', and not the export's.

The stage itself took three rounds, from 66–100 ms on sixteen threads to
45–65. A `%` by the CFA's period — known only at run time, so a division —
ran for every row and site; the period's phase is now found once per box and
stepped (78 → 66 ms on the A6000). The tent weights were worked out afresh
for every pixel and colour; the weights of every box a frame can ask for are
now built once, at a sixteenth of a photosite (a fiftieth of a proxy pixel),
and looked up (66 → 49). What is left is the averaging itself: with the
tents every photosite is read three to five times by overlapping
footprints, which only a two-pass separable warp would take out.

It also goes where a full develop was made to be shrunk: the grid's preview
of a raw that carries no camera JPEG (every ORF — the E-M1's grid thumbnails
were a full develop each) and the thumbnail of an edited photograph.

**Going back (PERF-031).** A step back after a step forward, or two frames
of a burst compared back and forth, was a whole decode of a photograph
decoded seconds earlier. The last three openings are kept — the proxy the
open photograph already holds, and two more: 60 to 90 MB — keyed on the
file, the proxy's size and what Automatic means (RENDER-016). `prefetch::take`
answers from them before it looks at the decode ahead; the decode ahead
skips a neighbour that is kept, and one that finished but was passed over
by a change of direction is kept too. With the arrow keys under Xvfb (open,
→, ←, →, ←), A7R III ⇄ 5DS went 623 / 46 / 439 / 711 / 443 ms →
238 / 46 / 41 / 41 / 41, four decodes where there were seven, and the whole
16-second session 44.8 → 6.3 processor-seconds; 5DS ⇄ Z 7 went
807 / 50 / 713 / 610 / 712 → 432 / 47 / 41 / 39 / 38, 46.9 → 5.9.

**Going back in (PERF-032).** Zooming back to fit dropped the decoded
original at once, so every look at 1:1 somewhere else decoded it again —
0.3 to 1.2 s and 2.6 to 13.6 processor-seconds each time. What was rendered
from it still goes at once; the decode is kept for thirty seconds after the
last look, unless another zoom in or another photograph has come since. On
a 5DS frame (double-clicks: 1:1, fit, 1:1) the second 1:1 went straight to
its tile instead of decoding again for 659 ms and 6.0 processor-seconds; the
price is 576 MB held for the half minute.

**Nothing speculative when frugal.** The decode ahead is the one piece of
work nobody asked for. `prefetch::frugal()` is its switch, `false` for now:
it is wired to `numa_core::power::frugal()` when speed-night is merged.

**In the editor** (release build under Xvfb, open a photograph from the
grid and step once with the decode ahead — the harness of the 27th; three
passes each, before and after back to back on a quiet machine, medians):

| | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| open: first frame, ms | 451 → 296 | 733 → 400 | 818 → 435 | 369 → 225 | 616 → 239 | 491 → 291 | 712 → 381 | 476 → 380 | 489 → 378 | 1243 → 324 |
| open: decode, processor-s | 2.6 → 1.0 | 5.8 → 1.7 | 6.0 → 1.3 | 2.8 → 1.1 | 4.8 → 1.1 | 2.9 → 1.1 | 5.4 → 1.2 | 2.3 → 1.2 | 2.3 → 1.1 | 13.5 → 2.4 |
| step: first frame, ms | — | 49 → 47 | 42 → 43 | 58 → 60 | 47 → 47 | 46 → 46 | 44 → 46 | 64 → 63 | 65 → 65 | 46 → 50 |
| step: its decode ahead, processor-s | — | 5.6 → 1.6 | 5.9 → 1.3 | 2.7 → 0.9 | 4.8 → 1.0 | 2.8 → 0.9 | 5.3 → 1.1 | 2.2 → 1.0 | 2.2 → 1.0 | 13.4 → 2.3 |
| the whole session, processor-s | 9.0 → 4.0 | 20.7 → 5.1 | 15.6 → 4.3 | 9.0 → 4.5 | 17.6 → 4.4 | 23.5 → 6.0 | 15.4 → 4.7 | 11.2 → 4.6 | 14.8 → 4.6 | 14.4 → 3.0 |

A step was already a decode made ahead and stays at 40–60 ms; what it costs
now is a third to a fifth of the processor behind it. The session row is the
whole 12-second run — starting the application, the grid, the opening, the
step and the next decode ahead.

**What the Apple apps get.** `numa-ffi`'s `Editor::open` does what Linux
did: `decode_for_editing`, downscale, drop. With `decode_proxy(path,
max_edge)` it gets the same saving, and the peak goes with it — the full
develop of a 45 MP frame is 540 MB of floats before the proxy exists, the
proxy path's is the mosaic (100 MB) and the proxy. On an iPhone that is the
difference the memory notes of 26 September were about.

**1:1 develops what is on screen (PERF-033).** A zoom past the proxy
developed the whole original — 0.6 to 1.4 s and 5 to 17 processor-seconds
with its colour stage — to show two or three megapixels of it. The idea is
Core Image's region of interest, and darktable's tiles: ask each stage what
it reads of the stage before for the part that is wanted, and develop only
that. `raw::region::Regions` opens the raw once — the same read, profile,
orientation and demosaic as `decode_beside` — and develops any rectangle of
it: baseline, falloff and geometry through `lens::Centre` and
`lens::bilinear` (moved out of `correct_vignetting` and `correct_geometry`
so both run the same arithmetic), FT-022's false colour through the same
median of nine, the factor, the orientation. The one step that is not local
is the exposure match's median, over every 37th pixel of the whole frame;
those pixels are developed one by one — 17 to 35 ms on the Bayer bodies,
168 on the X-T5, whose false colour needs nine developed neighbours each.

Checked float for float against `decode_linear_best` on all ten bodies — a
tile, the four corners and edges, odd sizes, the whole frame — and in a test
on the corpus's Bayer and X-Trans frames. In the editor the part the view
reads is `render::tile_box` (the tile's rectangle, or `cut_turned_tile`'s
box, grown by what the bilinear sample reaches; tested by cutting from a
frame that holds only that box), plus 128 pixels, developed into a working
frame of the whole size whose other pages are never touched. The 1:1 canvas
differs from main's in 0 pixels, both zoom-ins, on a 5DS and an X-T5.
Anything that needs the whole frame — HDR, Clarity, Texture, Dehaze, AI
frames, a spot whose source is elsewhere — still gets all of it.

| first zoom-in | 5DS | A7R III | X-T5 |
|---|---|---|---|
| before: decode + colour stage of the whole frame | ~0.8 s, 6.5 s of processor | ~0.6 s | ~1.4 s, 16.8 |
| after: open + the part on screen | 453 + 24 ms, ~3 s | 194 + 31 ms, ~2.1 | 984 + 38 ms, ~11 |
| a pan to a part not developed yet | 24–44 ms | 31–62 ms | 38–69 ms |
| held after the colour stage | 576 → 19–38 MB | | |

**Previews kept beside the photographs (PERF-034).** With PERF-030 an
opening is 0.2–0.45 s and a processor-second; kept, it is a 14 MB read. The
proxy goes into the library's own `.numa/previews` — Lightroom keeps
`Previews.lrdata` beside its catalogue, darktable its mipmaps in a cache of
levels; here the level is the one proxy the screen asks for — with a
`CACHEDIR.TAG` so backup and sync tools skip it. Half floats (`half`,
already in the build): a rendered proxy moves by one level on 1–1.6 % of its
values. Keyed on the path within the library, mtime and length, the proxy's
size, what Automatic means and Numa's version; least recently used goes
first. Nothing outside a library, on a drive that is away or read-only, or
on one where the session's first read is slower than the last decode — a
network share or an old stick, where 14 MB at 30 MB/s is half a second and
decoding again is quicker.

| | decode (page cache emptied) | read back |
|---|---|---|
| ten bodies | 189–444 ms, 1.0–2.3 processor-s | 10–19 ms, 7–15 processor-ms |
| after a restart, first frame | 5DS 460 ms, X-T5 326 | 163, 143 |

How much room each library gives it is Lightroom's "Camera Raw cache size"
as a stock slider in Preferences › Storage: Off, 0.5, 1, 2 (default), 5,
10, 20 GB, per library, trimmed at once when lowered.

**The proxy's exposure (PERF-035).** The one systematic difference PERF-030
left — the exposure match's median over the proxy's smoother pixels, up to
−1.2 % on the Z 7 — is now taken over the full frame's one-in-37 pixels,
each demosaicked on its own at the photosite the geometry reads: within
0.36 % (Z 7) and 0.16 % (A6000), under 0.1 % elsewhere, for 5–15 ms.

**Grid thumbnails for less: measured, not done.** The camera's JPEG is what
the grid shows (Photo Mechanic's whole idea), but rawler hands out only the
largest one a file carries: 5760–8688 pixels on the Canons and Nikons,
34–82 ms of one core, to keep 320–1920 of it. The files carry smaller ones
— a CR3's 1620 × 1080 `PRVW`, a NEF's and a CR2's small previews — and a
JPEG can be decoded at an eighth in the DCT. Either needs the JPEG's bytes
out of rawler (speed-night vendors it; that is where it belongs) and, for
the second, a decoder that scales (`jpeg-decoder`'s `scale()`; zune-jpeg,
which `image` uses, has none). For a thousand-frame import: about a minute
of one core, once.

**Measured, and left as proposals.**

- *A reduced-size export from the mosaic.* Above: at the 4096 pixels a
  2048-pixel export develops at, only the X-T5 gains (13.8 → 5.1
  processor-seconds). It would also change the export's pixels, which this
  work was not allowed to do; an X-Trans-only switch is the photographer's
  call.
- *A two-pass separable warp* for the develop-small stage, reading each
  photosite once per pass: perhaps 45–65 ms → 15–20.

## Four branches, one decode — 28 September

The four sections above were written on four branches the same night, and
merged in that order on `speed-merge`: loaders-night (UX-025), speed-night
(PERF-021…029), gpu-night (RENDER-010), smart-night (PERF-030…035). Two of
them had a `raw::decode_proxy` that meant different things, three had a
switch for saving power, and two had a decode ahead. This is how they were
made one, and what that measured.

**Two routes to the editor's proxy, and a rule.** gpu-night's is the whole
frame developed on the card and shrunk there — the processor's full develop
within RENDER-010's tolerances; smart-night's averages the mosaic straight
into the proxy on the processor (PERF-030). They are
`raw::proxy_from_card` and `raw::proxy_from_mosaic` now, and the editor asks
`raw::editor_proxy`, which is the rule: the card where one is ready for the
power state and Numa is not frugal; otherwise the mosaic. A card that
declines a frame (a model holding it, a sensor it does not do, more than
3 GB) hands it to the mosaic, not to the processor's full develop. Grid
previews and edited thumbnails take the mosaic and never the card, as
gpu-night kept background decodes off it. 1:1 and the export are what they
were on each branch: the card where it is ready — frugal, the integrated
one — and the processor's full develop otherwise. One code path carries
both: `Demosaic::Proxy(edge)` is the proxy, and `decode_beside` offers it to
the card only when the caller allows.

The three against each other, on the ten frames of the switching run (warm
file cache, release build, ms; the card is the RX 9070 XT):

| | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| processor, full develop | 259 | 530 | 458 | 192 | 363 | 226 | 410 | 251 | 248 | 872 |
| card, whole frame (1:1, export) | 83 | 277 | 192 | 70 | 122 | 90 | 162 | 157 | 157 | 257 |
| card, proxy | 47 | 195 | 105 | 27 | 47 | 45 | 86 | 123 | 123 | 185 |
| mosaic, proxy | 71 | 236 | 116 | 61 | 79 | 82 | 111 | 162 | 164 | 199 |

What each gives, against the processor's full develop: the card's whole
frame a mean difference of 1.5e-7–1.2e-6 on the Bayer bodies and 2.5e-6 on
the X-T5, where 0.023 % of values are more than 1e-3 apart (none on the
others); its proxy, rendered, at most one level, on 0.007–0.077 % of values
— all inside `the_card_develops_what_the_processor_does`. A 5DS exported
from the editor as a JPEG, card against processor: 0.03 levels on average,
0.17 % of values more than two apart, the largest 15 where a JPEG block
quantises one step differently. The mosaic's proxy, rendered as
smart-night rendered it, 0.21–1.03 levels on average and 4–22 at the 99.9th
percentile: smart-night's own table, and with PERF-035 the Z 7's 0.96 is
0.60 — the merge changed nothing in it.

**Frugal: the mosaic, measured.** On a frugal machine the question was the
low-power card's full develop against the mosaic on the processor. Measured
on this machine's integrated GPU (the 9800X3D's two-CU Radeon), the power
switch on, the package's power from the APU's `gpu_metrics` and the
discrete card's from `power1_average` sampled every 50 ms, per jump above
idle over the ten frames:

| per proxy, frugal | card (integrated) | mosaic (processor) |
|---|---|---|
| back to back, as an opening runs | 7.0–7.2 J, 0.44 core-s | 8.1 J, 1.15 core-s |
| one every 1.5 s | 7.8 J, 0.42 core-s | 8.1 J, 1.12 core-s |
| as a decode ahead (the frugal pool, nice 10) | 7.2 J, 0.38 core-s | 7.6 J, 0.80 core-s |
| time, per frame | 164–1 159 ms (X-T5 1.15 s) | 60–231 ms (X-T5 0.19 s) |
| the discrete card | 0.00–0.02 J | 0.00–0.01 J |

The card saves 0.3–1.1 J a photograph above idle — 4 to 14 per cent — and
takes two and a half to six times as long. Over that extra wait the rest of
the machine is not idle for free: a quarter of a second more per jump, on
average, at this package's 33 W idle is about 8 J, and on a laptop at a few
watts still more than the card saved. So the mosaic is the cheaper by
energy as well as by time, and it is the rule. A laptop's integrated GPU
has six times these compute units; if the photographer's measures otherwise,
`card_makes_proxies` in `raw.rs` is the one place to change. The discrete
card stayed at idle through all of it.

**One power switch.** `numa_core::power::frugal()` is the only flag:
gpu-night's (`NUMA_GPU=low`, kept for measuring) and smart-night's
(`prefetch::frugal()`, a stub) read it now. It is read before the card is
chosen — `power::watch` runs before `start_gpu`, and UPower's `OnBattery` is
read synchronously, where the branch had it arrive later — so a laptop
started on its battery makes a device on its integrated GPU only. A flip at
run time makes the new state's device in the background
(`raw::gpu_follows_power`); the photograph under way keeps the one it
started on, and the next one asks again. `NUMA_FRUGAL=1` forces frugal, for
measuring on a machine that is neither on battery nor saving power.

**One decode ahead.** speed-night's: its own pool at nice 10, stopped at the
next stage when it is let go, and frugal only after two steps the same way —
smart-night's "nothing ahead when frugal" gave way to it. From smart-night:
a neighbour already among the three kept is not decoded again, and a
finished decode that a change of direction passed over is kept — now also
when an opening elsewhere lets it go, which is where speed-night stops a
running one.

**1:1 and the previews.** PERF-033 came in after the rule: where a card
is ready the original at 1:1 is its whole-frame develop and the part on
screen is cut from it for the colour stage; elsewhere PERF-033's regions
develop the part from the raw. PERF-034's previews keep whatever
`editor_proxy` made, under the same key, so a frugal session reads what a
full-speed one developed rather than decoding again.

**In the app.** Release build under Xvfb, an isolated copy of the ten-frame
library, the harness of the 27th (open from the grid, step once, the decode
ahead); main and the three states of the merge back to back per frame, on a
machine shared with other sessions (so main is slower here than in the
sections above). First and sharp frame are one frame; ms:

| jump | 5D3 | R5 | 5DS | A6000 | A7R3 | Z 6 | Z 7 | E-M1 | E-M1 b | X-T5 |
|---|---|---|---|---|---|---|---|---|---|---|
| main | 636 | 1185 | 1207 | 607 | 766 | 630 | 1113 | 679 | 697 | 1615 |
| card | 144 | 410 | 329 | 281 | 311 | 306 | 338 | 350 | 377 | 419 |
| no card (`NUMA_GPU=0`, the mosaic) | 313 | 439 | 409 | 346 | 341 | 344 | 369 | 390 | 392 | 438 |
| frugal (`NUMA_FRUGAL=1`, the mosaic) | 333 | 479 | 426 | 351 | 368 | 363 | 402 | 390 | 396 | 456 |

A step to the photograph decoded ahead is 45–54 ms on the card and without
it (main 39–113); frugal, where nothing is decoded ahead before two steps
the same way, the first step is a decode — 133–298 ms, the thumbnail
standing in meanwhile. No stand-in and no blank frame on any of the 36
steps with the card or without it. The whole 12-second session (start, grid,
opening, step, the next decode ahead) took 16–31 processor-seconds on main,
6.7–8.6 with the card, 8.0–10.9 without, 7.7–10.1 frugal.

1:1 by double-click, until the tile is up, and the second zoom-in within
half a minute (PERF-032), ms; the export's decode after it:

| | 5DS 1:1 / again | X-T5 1:1 / again | export decode 5DS / X-T5 | session processor-s 5DS / X-T5 |
|---|---|---|---|---|
| main | 1282 / 1230 | 2111 / 2007 | 662 / 1198 | 47.7 / 79.9 |
| card | 701 / 461 | 801 / 480 | 219 / 251 | 18.8 / 22.5 |
| no card (PERF-033's part) | 800 / 493 | 1265 / 526 | 490 / 872 | 26.6 / 41.5 |
| frugal (integrated card) | 1106 / 459 | 1749 / 475 | 451 / 1217 | 20.1 / 24.1 |

The export with `NUMA_GPU=0` is main's file byte for byte on both; the
discrete and the integrated card write the same file as each other, and
against the processor's a 5DS differs by 0.03 levels on average. Frugal,
the process held a file on the integrated GPU's render node only
(`/proc/<pid>/fdinfo`), full speed on the discrete one only. No amdgpu
fault, reset or timeout in the kernel's log through any of it.

**What is left.**

- The integrated GPU measured here is the smallest there is. The rule
  should be measured once on the photographer's laptop (the same two
  scripts; `NUMA_FRUGAL=1`). On this one it is also slower at 1:1 than
  PERF-033's part on the processor (an X-T5 1.75 s against 1.27), for a
  fifth of the processor time; whether that is the better trade on a
  battery is the same measurement.
- Vulkan lists every adapter when the device is made, and opening a
  discrete card's render node to list it wakes it for a moment on a hybrid
  laptop, even though no device is made on it. Not measurable here.
- A flip from full speed to frugal leaves the discrete card's device made
  (idle, its buffers handed back) until Numa restarts.

## Built as one unit, and four ideas for the decoders — 28 September

The build, runtime and decoder findings of `docs/EXPLORE_BUILD.md` (branch
`explore-build`), carried out on `build-now` (PERF-060…065). Measured with
that branch's `xbench` on the ten frames (medians; wall, processor time =
user + sys, and a hash of every output), rawler alone over the 528 raws of
raw.pixls.us in `raws-cc0` (the mosaic hashed, with its levels and
geometry), and the APU's socket power above idle for energy. Every output
hashes as it did before, everywhere below.

**PERF-060: `[profile.dist]`.** Fat LTO and one codegen unit, for what is
handed over: the Flatpak manifest, the AppImage's `bundle.sh` and
`stage.sh` (`target/dist/numa`), and `publish.sh`'s check that the public
copy builds. `packaging/flathub-reference` is the photographer's and still
says `--release`; the public copy carries the profile, so switching it there
is `--profile dist` and `target/dist/numa`. The same source, `release`
against `dist`, two interleaved rounds:

| path | processor time | wall | energy |
|---|---|---|---|
| raw decode | 0 % | 0 % | — |
| jump (decode + proxy + render) | −1.5 to −1.8 % | −2.8 to −3.0 % | within noise (600 → 628 J for 20) |
| slider tick | −6.4 % | −5 % | 1081 → 998 J for 400 (−8 %) |
| full-size export | −3 % | +2 to +2.7 % | — |

The export's wall time went the other way in both rounds, by little; its
processor time did not. From nothing, as `flatpak-builder` runs it (all 16
jobs, the app and `xbench`): 185 s, 557 processor-seconds, 4.9 GB peak for
the whole build's cgroup and 2.2 GB for the largest process, the one link
that does all the optimising — a machine with 8 GB free builds it. `release` from nothing
at 8 jobs: 60 s, 3.9 GB. The Apple workspace (Numa-mac) reads its own
profiles, so the same four lines go in its `Cargo.toml` and `app/build.sh`
builds `--profile dist` (not done here: that repository is the Apple
side's).

**PERF-061: Fuji's `read_code` inlined.** It carried `#[multiversion]` for
LZCNT; for a function with arguments that means a dispatch on every call and
a clone that cannot be inlined into its caller, so every sample of an
X-Trans frame paid a call. Inlined down to the strip instead. The explore
branch's variant B also multiversioned the whole strip; measured again here
it was slower than plain inlining on one core (933 against 890 ms) and on
all (1086 against 1040 processor-ms), so A it is. X-T5, medians of three:
one core 971 → 890 ms, sixteen threads 125 → 115 ms and 1151 → 1040
processor-ms. All 70 RAFs hash the same.

**PERF-062: a code's entry in the decode cache when its difference does not
fit.** PERF-021's cache already held the whole decoded difference where code
and difference fit in 12 bits — half of rawspeed's prefix-code decoder. The
other half: where the code fits and its difference does not, the entry holds
the code's length, shift and difference length, and only the difference is
read. Before, such a code went back through `hufftable`, 64 K entries of
three bytes for a 16-bit table, out of the first-level cache. Over the 184
Huffman-coded raws of the corpus: processor time −2.3 % in all, −20 to −34 %
on frames with long differences (PowerShot and Rebel CR2s, D3/D3S/D4S/D300S
NEFs, Leica M DNGs), the 5D Mark III on one core 71.7 → 70.4 ms. The other
rawspeed gap the exploration named, CR2 on one thread (52 against our 76 ms),
is in the JPEG bit reader; CR2 and NEF are read on four threads now
(PERF-022), where that reader is not used, so it was left.

**PERF-063: uncompressed frames in two runs.** rawspeed's A7R3 took 9 ms to
our 24 because it copies an uncompressed frame on one thread; rawler made a
rayon task of every row. Measured on the A7R3's `raw_image`, sixteen
threads: a task per row 10.8 ms and 78 processor-ms; one run 10.4 / 10.3;
two 8.8 / 12.3; four 8.4 / 18; eight 9.2 / 35. Two it is, for the plain
unpackers only (`packed.rs`); the decoders that do real work per row keep a
task per row. Over the 123 uncompressed and 16-bit raws: processor time
−24 %, wall within the noise. What is left of the 24 ms is the file's
mapping, the data-now agent's part (`rawsource.rs`).

**PERF-064: quality of service on Apple.** A thread made by
`pthread_create` starts at the default QoS whatever Swift asked for, so
rayon's threads never told the scheduler what they were. `power::background`
now sets `QOS_CLASS_BACKGROUND` in its start handler, and
`power::start_threads` builds the global pool with `QOS_CLASS_USER_INITIATED`
— for the app to call first. Background, not utility: measured on the
photographer's M3 Pro (`docs/MAC_BENCH.md`, branch mac-bench), utility still
ran on the performance cores, and background kept work on the efficiency
cores at 3.5–4 times less energy; waited-for work was fastest and cheapest at
user-initiated on all cores. Linux keeps nice 10. Compiled for
`aarch64-apple-ios` (std built from source) and not run.

**PERF-065: the develop-small stage, and why not a two-pass warp.** The
proposal was a separable warp, reading each photosite once per pass, for
45–65 → 15–20 ms. Measured first, the premise does not hold: the stage costs
the same per proxy pixel whatever the frame (single thread: 5D3, 22 MP, 429
ms; X-T5, 40 MP, 647 ms), and a variant that reads at most one photosite per
row and site was no cheaper than the real one. The time is in each box's
setup — two weight lookups, the rows and sites, per pixel and colour, 220
cycles or so a box — and the lens arithmetic around it (a sixth). A two-pass
scheme only saves that setup where the geometry is separable, which with a
lens profile it is not: its first pass would have to run unwarped (the
mosaic box-filtered to an intermediate grid, then warped), which is a
different filter, with its tents to be tuned again for the acutance of
PERF-030's table. Estimated from the operation counts at 1.6× with an
intermediate at twice the proxy, 3× at the proxy's own size and blurrier —
a proposal, with its pixels to measure, not done. Done: the site's first
column found by a subtraction instead of a `%` (−6 %, the same bits);
removing `floor`/`ceil`/`round`'s libm calls and the weights' copies was
measured and bought nothing, and was not kept. Sixteen threads buy little
over eight here (55 → 46 ms on the 5D3 for 70 % more processor time).

**Before and after, the ten frames** (base = speed-merge with smart-night's
PERF-035, `release`; after = this branch at PERF-062, `dist`): raw decode −3.5 %
processor time (X-T5 −10 %), jump −2 %, slider tick −6.7 %, export −2.5 %.
## The render on the graphics card — 28 September

RENDER-017 (the brief called it RENDER-011, which was taken), from
explore-render's prototype: the editor's render of its proxy runs on the
card that developed it, and the frame goes to GTK where it lies. Built,
held to the processor's render on ten bodies — and **off by default**,
because its first run in the app made the card log page faults in the
hand-over to GTK. `NUMA_GPU_RENDER=1` asks for the render on the card,
`NUMA_GPU_DMABUF=1` for the dmabuf; without them nothing changes.

**The shape.** `numa_render::card::plan` turns a document into the numbers
the card needs, each made by the processor's own code — the profile's
tables resolved for the white point (`Rendering::resolve`), `ToneCurve`'s
stops, the mixer's `Look`, the curves' lookups, the base curve at its
quarter stops — or names the first stage the card does not have, and then
the whole render stays on the processor: no stage goes back and forth.
`numa-gpu`'s `render.wgsl` runs it in up to four passes over the proxy:
the colour stage; the mirror, quarter turn, crop, straighten and
perspective (`geometry_of`, with `LinearImage::cropped`'s bilinear sample);
colour noise reduction's box along the rows; and everything after — the
column box and the recolouring, exposure, contrast, the four tone regions,
saturation and vibrance, the mixer in ProPhoto, the base curve or sRGB's
encoding for a finished picture, the curves, eight bits, the histogram of
every fourth pixel and the clipping overlay. The develop leaves the proxy
it made on the card (`render::keep`; four at most, about 28–46 MB each,
known by their size and every 997th value, so a copy of a proxy finds it
too) and a proxy from anywhere else — the mosaic, the previews on disk, a
JPEG — goes up once. Out comes a frame of RGBA and 770 counts: read back,
or copied into a GBM buffer that GTK 4.14's `GdkDmabufTexture` samples
where it is (`display.rs`; libgbm opened at run time, a pool of three).

What stays on the processor, by the name `plan` gives it: spot removal,
face retouching, dehaze, luminance noise reduction, sharpening where the
proxy shows it (a body under about 4 800 pixels on its long edge; at fit
the others' radius rounds to nothing), defringe and moiré, calibration,
HDR, Clarity and Texture, point colour, masks, black and white, the grade,
vignette and grain, a LUT, a working or output space other than sRGB,
manual lens corrections, AI denoise, and a finished picture's white
balance (TOOL-004). And 1:1 and the export, which render the full frame.
(Since 29 September the card has the grade, point colour, black and white,
the vignette, masks, luminance noise reduction and HDR, Clarity and Texture
too: "The card's stages".)

**In the app.** `card_render.rs` asks for the card in the render's plan
and runs it on the render's worker; `Picture` carries either the
processor's pixels, the card's RGBA or its dmabuf to `present` and
`show`, which put the clipping overlay on the processor's pixels as before. While the hand moves, the card's frame
needs no colour stage on the processor; the settled render still makes
it, because the masks, Auto, the reference pane and the thumbnail read
the working frame. The models' card lock became a read-write lock: the
develop and the render share the card (one wgpu device) and neither ever
runs beside a model; as a mutex, a render and a develop that met would
have sent one of them to the processor for nothing.

**The same picture, within a tolerance.**
`the_card_renders_what_the_processor_does` renders twenty-one edits — each
stage alone, then everything together, turns, crops, a straighten and a
perspective among them — on the raw, and six of them on a finished
picture made of it, through `apply_stack` and through the card, at the
2 400 pixels of a 1440p screen (the ten bodies were run before the
geometry was added, the corpus's three after):

| | 8-bit, largest step | values that moved | histogram counts off |
|---|---|---|---|
| untouched, ten bodies | 1 level | 0.0008–0.0040 % | ≤ 0.006 % |
| every case, ten bodies | 1 level | ≤ 0.017 % (X-T5, everything) | ≤ 0.016 % |
| geometry, three corpus bodies | 1 level | ≤ 0.029 % (A6000, keystone) | ≤ 0.031 % |

The test holds at most 2 levels on under 0.5 % of values and the
histogram within 1 %, as RENDER-010's develop is held. The clipping
overlay is held to `mark_clipping` over the card's own frame, exactly —
against the processor's frame a level at 0 or 255 is a pixel painted or
not, which is what the first version of the test tripped over. The
integrated GPU passes the small frame; the whole comparison there was not
run (below).

**The page fault.** The first run in the app (Xvfb, cairo, the Z 6, an
Exposure drag, the dmabuf on) logged forty lines of amdgpu `[gfxhub] page
fault`, client CB, writes, from the render's worker — no ring timeout, no
reset, the desktop untouched. The copy of the frame into the imported
buffer: RADV does it through the colour block in whole tiles, and a
buffer allocated at exactly 1920 × 1277 was written three rows past its
end (the faulting pages are the 23 KB after it; explore-render's
prototype used 1280 rows and never saw it). The compute passes and the
read back had run some seven hundred renders in the tests without a
fault. As the round's rules say, GPU work stopped there for the session:
the buffer is now allocated padded to 64 pixels both ways, GTK told the
frame's own size, **but that is not yet measured on the card**, so both
switches are off. Two other things the run showed: under Xvfb GTK's cairo
renderer accepts a dmabuf and copies it down on the processor, 220 ms a
frame, so a dmabuf is now offered only where GSK draws with the card;
and while the card faults, every render waits on it (220 ms).

**Drags, the processor's side** (Xvfb, cairo, the Z 6 at 1920 × 1277,
120 ticks 16 ms apart, `NUMA_GPU=0`, above idle, two runs):

| drag | frames put up | processor | energy |
|---|---|---|---|
| Exposure | 118 of 120 | 2.2 cores, 4.5 CPU-s | 17.5–21.3 J/s, 0.30–0.36 J a frame |
| Temperature | 117 of 120 | 5.3 cores, 10.7 CPU-s | 20.3–28.3 J/s, 0.35–0.49 J a frame |

The card's side is explore-render's prototype, the same kernels for an
untouched photograph: 1.5–1.9 ms and about 0.05–0.1 J a render with the
proxy resident and the frame read back, the integrated GPU 6–15 ms and
0.1 J; with a dmabuf the frame's hand-over was 0.16 ms of processor time
against 1.28 for a read back and an upload. So the expectation is a
Temperature drag at about a twentieth of the energy and none of the five
cores — to be measured, not quoted, when the switches go on
(`numa-scratch/speed-night/gpu-render/`: `xbatch.sh`, `run.sh`,
`joules.py`, and `NUMA_DRAG=120 NUMA_DRAG_WHAT=temperature`, which now
prints `DRAGCOST`: the window, CPU-s, paints, renders, and how many the
card made).

**Frugal.** The render takes the device the develop takes for the power
state (`raw::card_frugal`): the integrated GPU, or none, never a discrete
card woken. There the proxy comes from the mosaic and goes up once; the
render must not cost more energy per frame than the processor's, which is
the measurement above on `NUMA_FRUGAL=1` — not taken, for the same
reason.

**Where the frame can go to GTK as a dmabuf.** GTK 4.14 or later
(`gtk4-rs` now builds against `v4_14`): the Flatpak's GNOME 50 runtime has
4.22.5, and libgbm.so.1 is in both the runtime and its GL extension; the
AppImage's Ubuntu 24.04 base has 4.14. The display has to list linear
ABGR8888 among its dmabuf formats and GSK has to draw with the card;
otherwise, and wherever GTK refuses the texture once, the frame is read
back and handed over as a four-byte `MemoryTexture`. A hybrid laptop whose
integrated GPU develops and draws imports its own buffer; a discrete card
that renders while the integrated one draws would cross devices, which is
GTK's to accept or refuse.

**What the Apple apps get, and what changes there.** `card::plan` is
portable Rust and ships there as it is; the WGSL goes through naga to MSL
on wgpu's Metal backend like the develop's. Three things differ:

- Display: no dmabuf and no GBM. The render writes into an `MTLTexture`
  backed by an `IOSurface` (made once per size, a pool of three, like the
  GBM buffers), and the view shows it as a `CALayer`'s contents or a
  `CAMetalLayer`'s drawable — the Apple twin of `GdkDmabufTexture`,
  through `wgpu::hal::metal` as `texture_from_dmabuf_fd` is here. SwiftUI
  hosts it in the `UIViewRepresentable`/`NSViewRepresentable` the canvas
  already is.
- Unified memory: nothing needs a read back or an upload. The proxy can be
  a buffer the processor wrote in place (`MAPPABLE_PRIMARY_BUFFERS`), the
  histogram read where the card wrote it, and a read-back fallback is a
  pointer, not a copy. MAC_BENCH measured 5–10 % from mapped buffers on
  the develop.
- Correctness: compile in safe math. MAC_BENCH (run 2) found Metal's
  default fast math failing the develop's tolerance on five of nine bodies;
  this shader compares for equality in `rgb_to_hsv` (which channel is the
  largest) and leans on `pow` and `log2`, so the same patch — wgpu-hal's
  `MTLCompileOptions.mathMode`, vendored on mac-bench — goes with it, and
  `the_card_renders_what_the_processor_does` runs on the MacBook as it is.
  One submission a render: a proxy is at most 3.8 M pixels, under the 8 M
  MAC_BENCH found best per submission on the M3 Pro.

**Next, in order.** Measure the padded buffer on the card (the tests,
then the app on a headless Mutter with Vulkan GSK, where the dmabuf is
real) and turn the switches on if the kernel's log stays clean — or, if it
does not, write the frame from the compute pass straight into the imported
image as a storage texture, which never goes through the colour block.
Then the drags and frugal above. Then the stages in the order they are
used: vignette, black and white, the grade and calibration (per pixel,
small), Clarity/Texture/HDR and dehaze (a guided filter over the frame),
sharpening and luminance noise reduction, point colour, masks.

## Metal for the Apple apps — 28 September

RENDER-018, from MAC_BENCH's measurements on the photographer's M3 Pro
(`docs/MAC_BENCH.md`): Metal's full develop took a third of the
processor's energy, and its proxy the least energy of any route, but
Metal was only correct in safe math, and an iPhone's memory, not its
energy, decides what may run there.

**What was built** (branch `metal-now`, tested on Linux; the MacBook has
not run it yet):

- **Safe math.** wgpu 30 has no switch for Metal's math mode: not in the
  shader module's descriptor, not in naga's MSL options, and an MSL
  passthrough module is compiled with default options too. So
  `vendor/wgpu-hal` is wgpu-hal 30.0.1 with one hunk in the Metal backend
  (`mathMode = .safe`), trimmed to the backends Numa builds;
  `vendor/wgpu-hal/NUMA-CHANGES.md` says how to carry it to the next wgpu.
  The render on the card (RENDER-017) gets it too.
- **Apple's defaults.** Unified-memory buffers (the mosaic written where
  the card reads it, the frame read where it wrote it), 8 M-pixel
  submissions, one device whatever the power state (frugal keeps Metal:
  there is no second card), `numa_gpu::release` for a memory warning and
  the background, and the card's limits as one line for the app's log.
- **Frames that fit a phone** (`crates/numa-gpu/src/buffers.rs`). Each of
  the six planes is a buffer of its own (16 bindings, Metal allows 29); a
  plane takes a buffer the passes before it are done with — `lin` the
  mosaic's once `scale` has read it, `bent` the demosaic's; the frame
  comes back through one 64 MB buffer a chunk of rows at a time instead of
  a whole frame of floats beside the planes; Markesteijn's bands are held
  to the largest buffer and a quarter of the budget. The budget is 3 GB,
  or on iOS a third of `os_proc_available_memory()`, read at each develop.
  Where f32 still does not fit and the caller may round, the planes are
  f16. What the nine MAC_BENCH bodies come to under the guessed iPhone
  limits (256 MB a buffer, 1 GB in all; computed from the code's sizes,
  not measured on a device — 832 to 1702 MB with a 274–575 MB largest
  buffer before):

  | | 5D3 | E-M1 | A6000 | Z 6 | X-T5 | A7R III | R5 | Z 7 | 5DS |
  |---|---|---|---|---|---|---|---|---|---|
  | f32, MB (largest) | 620 (84) | 570 (76) | 667 (91) | 675 (92) | 1168 | 1123 | 1188 | 1205 | 1328 |
  | f16, MB (largest) | | | | | 788 (180) | 721 (160) | 761 (170) | 772 (173) | 848 (191) |

  So every 20–24 MP body fits in f32, and the 40–50 MP ones only in f16.
- **f16 rounds**, and is kept to what is shown: 1:1 on a device where f32
  does not fit, never an export or the proxy. Measured on Linux
  (`NUMA_GPU_HALF=1`, the tolerance test on the nine bodies): the linear
  mean difference 1.0e-5 to 7.5e-5 (f32: under 1e-5), up to 0.43 % of
  values more than 1e-3 apart (5DS), the rendered proxy 1 level apart (2
  on the X-T5) on 0.32–0.61 % of its values (f32: 0.007–0.05 %). The test
  holds f16 to its own bounds (1.5e-4, 1 %, 1.5 %); f32 keeps RENDER-010's.
- **The policy per device** (`raw::card_makes_proxies`, one place): on a
  Mac or iPad the editor's proxy on Metal, frugal or not; on an iPhone
  (`numa_core::power::set_phone`) the mosaic's. A frame the device has no
  room for is refused by the card and goes to the mosaic's proxy or the
  processor. 1:1 (`raw::region::Regions`, what the Apple app zooms with)
  is cut from a frame the card developed whole where the card takes it,
  float for float the card's `decode_linear_best`, and demosaicked on the
  processor otherwise. Export is Metal where it fits in f32.
- **Linux is unchanged.** The card's whole frame and proxy hash bit for bit
  the same as main's on the nine bodies (RADV, the RX 9070 XT), and the
  tolerance test passes as before; f16 and the budget never apply there.

**Left for later:**

- The MacBook: the tolerance test in safe math on this code, the timing
  and energy matrix Metal against the mosaic, and the `iphone` phase
  (MAC_BENCH's workflow). Until then the Apple app builds without Metal
  (numa-ffi's `metal` feature is off).
- The real limits of an iPhone and an iPad (the app logs them once
  Metal is on), and what `os_proc_available_memory` says at an open.
- Geometry folded into the passes that read it, which would take the
  third `bent` plane off a Bayer frame and bring the 42–46 MP bodies into
  1 GB in f32. A zero-copy upload (`newBufferWithBytesNoCopy`).
- Upstream: a math-mode option on wgpu's Metal shader modules, which would
  retire the vendored wgpu-hal (a suggestion; nothing was filed).
## Idle wake-ups, and a grid of fifty thousand — 28 September

EXPLORE_DATA F5 and F4, the photographer's "do it now". Branch `grid-now`.

**The wake-ups (PERF-050, landed).** When the editor opened before the grid
had ever been laid out — a photograph handed over at start-up, or `NUMA_OPEN`
— `cards::sweep` found the grid's cards unplaced and re-armed a tick
callback, which scheduled a sweep 60 ms later, which re-armed the tick, for
as long as the editor stayed open. A tick callback comes to any *realized*
widget, mapped or not, so the frame clock ran the whole time. The re-arm now
happens only for a mapped list, and the filmstrip, like the grid, is swept
when it is mapped. Every other tick callback and repeating timer was read for
the same pattern: the panel split's (breaks once allocated), the mask ants
(check `is_mapped`), the loupe's band autoscroll, the render booking and
the filmstrip's centring (one-shot or bounded), and the progress tickers of
AI denoise, downloads, Analyse and export (removed when their work ends).
None repeats for a widget that is not on screen.

Measured with a per-thread census on Xvfb (`threads.py`, 30 s windows,
release builds, lib10k):

| | before | after |
|---|---|---|
| sweeps re-armed for an unmapped grid, editor opened at start-up | ≥ 900 in 60 s | 0 |
| main-thread wake-ups/s, editor opened at start-up | 236 | 179 |
| editor opened from the grid | 174 | 165 |
| library, idle | 62–104 | 60–104 |

The last three rows do not reach EXPLORE_DATA's 0.1/s because this rig has a
floor of its own: with no dialog over the window, the main thread's `ppoll`
is interrupted by a signal (ERESTARTNOHAND, gdb `catch syscall ppoll`)
100–180 times a second in *every* build, the explore branch's binary
included, with the frame clock doing nothing (0 update, layout or paint
phases in 10 s, counted on the clock itself). x-data's 0.1/s was measured
with the models dialog open over the grid. So the editor opened at start-up
is now at the library's own level on this rig; the tick loop itself is gone.

**The recycling grid and filmstrip (PERF-051, not landed).** Built on branch
`grid-recycling` (f912a1e, 1b3e461, on top of this branch): the justified
wall is its own `GtkScrollable` that recycles card widgets the way
`GtkListView` does — the layout stays numbers for every photograph, only the
cards in view and half a screen either side are widgets — and the filmstrip
is a horizontal `GtkListView` over the grid's order. Each photograph's shape
is a new catalog column (`photos.aspect`, a migration, filled from the
thumbnail cache the first time a grid meets it and from every thumbnail that
lands). Stars, flags and pixels moved from widgets into the grid's rows and
`LazyThumb.texture`; the selection is by place. A `GtkListView` of justified
rows was the first idea; it only measures rows it has seen and estimates the
rest, so the scrollbar, the rubber band and the view held still under a
resize would all have become approximate.

First numbers, lib10k: filling the grid 250 ms → 1 ms on the main thread,
shown 1.7 s → 0.4 s after start, wheel scrolling 149 frames with none over
25 ms. The same screenful looks the same (`numa-scratch/speed-night/grid-now/
runs/shot-diag-idle.png` before, `shot-diag-grid1.png` after).

Left for later, in this order:

- The gestures on the recycling grid are not yet driven one by one: click,
  Shift and Ctrl, rubber band with autoscroll, arrow keys, stars and flags,
  the right-click menu, the loupe and cull keys, compare, drag in, the
  "Making thumbnails" loader. `interact.sh` in the scratch folder does them
  with XTest and a screenshot after each.
- The filmstrip's visible range was wrong in the first cut (a `GtkListView`
  keeps frames bound that are not in sight; the sweep then asked for every
  thumbnail, 217 processor-seconds in 30 s). Fixed in 1b3e461, not yet
  measured again.
- The before/after table at 1k, 10k and 50k (time to show, scroll frames,
  main thread, RSS; today 50k: 1.2 s grid + 1.5 s filmstrip, 1.9 GB) —
  `measure.sh` in the scratch folder.
- The rig's own signal floor above, to rule out that it is Numa's.
## The Masks tab stops running models — 28 September

Branch `masks-now` (MASK-015 to MASK-019), from the explore-data findings
(`docs/EXPLORE_DATA.md` on `explore-data`, F7). Release builds, every run
under `machine.lock` and `dev/capped.sh`, processor-seconds from `getrusage`,
energy above idle from the APU's socket power and the card's board power.
Probe: `tests/masks_probe.rs`; logs in `numa-scratch/speed-night/masks-now/`.

**A step with the Masks tab open (MASK-015).** Twelve of his frames, nine with
nobody in them:

| | per step | processor | energy |
|---|---|---|---|
| before, processor | 5.1 s | 50.7 cpu-s | ~213 J |
| before, card | 0.92 s | 0.49 cpu-s | ~31 J |
| after, a photograph seen before | 0.03 ms | — | — |
| after, a new photograph, processor | 0.51 s | 2.1 cpu-s | |

Most of "before" was not the segmenter (0.51 s, 2.1 cpu-s) but the subject
matte that `classify::animal` ran on every frame with nobody in it — 5.8 s and
64 cpu-s each on the processor, 0.58 s on the card — to name a chip that has
not been renamed since 22 September (only the Animal group takes the name).
It is gone. What the segmenter named — the groups, the animal's name and the
model's grid of classes, one byte a cell, deflated — is kept in the library
catalog's `found` table, filed under the framing and the model file, and read
back on arrival: the chips, and the hover outlines from the grid. A chip
pressed runs the model for the mask's pixels as before. The Apple apps get the
same calls: `Catalog::found(photo_id, &Chips::asked(&framing))`, and after a
run `Chips::of(&segmentation)` into `Catalog::save_found`; `framing` is the
string both clients already key masks with.

**SAM's embedding kept (MASK-016).** `numa_io::previews::embedding(path,
framing, frame)`: the image embedding in half floats, 2 097 188 bytes, as
`<hash>.sam` beside the previews, in their budget and their least-recently-used
trim. The positional grid is the same for every photograph and is kept once per
model in the cache. Encoding 1 001 ms, 11.0 cpu-s; reading back 41 ms,
0.04 cpu-s (12 frames, processor; on the card the encoder is 196 ms). Held
against a fresh encode, 108 clicks: mean IoU 0.99955, worst 0.9875, eight
identical — the half floats move a handful of pixels across one half.

**Models let go (MASK-017).** Loaded again after a release, on the processor:
EfficientViT +53 ms, SAM encoder +275 ms, SAM decoder +51 ms, PP-ResNet 97 ms,
BiRefNet 1.75 s (on the card 52, 218, 50 ms and 2.0 s). Released, glibc kept
what the sessions freed — RSS did not move — so the release trims the arenas:
397 + 262 + 222 + 578 + 275 MB back. The decoder loads with the embedding,
off the main thread, so the first click after a release is 23 ms, not 74.

**EfficientViT at 512 (MASK-018), not taken.** Exported with
`dev/export-efficientvit.sh b2 512` (its repository now also imports triton
from `nn/__init__.py`; wrapped the same way). On 108 frames, processor: 167 ms,
0.35 cpu-s against 469 ms, 2.09 cpu-s — but 32 frames offer other chips, and
the group masks agree at IoU 0.84 median, 0.68 mean (Sky 0.99, Greenery 0.90,
Water 0.89, Person 0.86, Buildings 0.81, Ground 0.77).

**A cheaper click encoder (MASK-019) — left for later.** Encoder alone, twelve
frames, processor:

| encoder | per photograph | processor | energy | file |
|---|---|---|---|---|
| SlimSAM-77 (shipped) | 1 037 ms | 11.1 cpu-s | 33 J | 23 + 17 MB |
| MobileSAM (TinyViT, Apache-2.0) | 261 ms | 1.8 cpu-s | 8 J | 28 + 16 MB |
| EfficientViT-SAM L0 (Apache-2.0) | 252 ms | 1.3 cpu-s | 7 J | 123 + 16 MB |

MobileSAM and EfficientViT-SAM L0 (Apache-2.0, weights and code) are exported
in SlimSAM's I/O by `numa-scratch/speed-night/masks-now/py/export_sam.py` —
the prompt path as segment_anything's own ONNX export, checked equal to the
model's own to the bit; L0 sees the frame at 512 through a 2 × 2 average, its
prompts stay in the 1024 square. The click path takes SAM 2's and
EfficientSAM's I/O too (from `sam-shootout` 48d8fa5). What is left, with
everything in place: the 108-frame click survey (`tests/click_survey.rs`,
`masks-now/sam/survey.sh <model>`) for slimsam, mobilesam, evsam-l0, esam and
sam21-tiny, each held against SAM 2.1 Large (downloaded, Apache-2.0) and
against the survey's own measures (hit, specks, holes, edge); then the chosen
one on the card and through Core ML. From 26 September's shootout, on the
older click code: SAM 2.1 tiny 0.63 s and EfficientSAM-Ti 0.43 s against
SlimSAM's 1.30 s, with 15–17 clicks outside their own mask against 4. Nothing
ships differently until the survey says so. EdgeSAM stays out (S-Lab,
non-commercial).

The card was only used where it was free. At 18:51, during the card run of the
reload probe, the kernel logged `amdgpu` page faults from another session's
`numa-a` (pid 399818, `gpu-render`/`x-render`), which was on the card beside
it without the lock; no GPU work was done here after that.
## Reading the head of a raw, not the file — 28 September

IO-020 to IO-026, from `docs/EXPLORE_DATA.md` (explore-data) and the build
explorer's lazy map. Release build, 9800X3D, NVMe; each file evicted from the
page cache before it is read (`posix_fadvise(DONTNEED)`), bytes from
`/proc/self/io`. Corpus: 560 files — every raw.pixls.us make (503), the ten
test raws, the photographer's own (CR2, RAF, HIF, one JPEG). Probe and logs:
`numa-scratch/speed-night/data-now/` (`tests/data_now_probe.rs` kept there,
not committed).

| per file, cold, 560 files | before | after |
|---|---|---|
| capture date (the first scan) | 34.7 MB, 14.1 ms | **5.4 MB, 2.3 ms** |
| embedded preview | 44.2 MB, 49 ms, 0.034 cpu-s | **2.8 MB, 24 ms, 0.022 cpu-s** |
| summary (info page, START-005's sample) | 44.1 MB, 29 ms, 0.024 cpu-s | **10.7 MB, 3.8 ms, 0.001 cpu-s** |
| grid thumbnail, nothing cached | 44.2 MB, 66 ms, 0.110 cpu-s | **12.1 MB, 32 ms, 0.042 cpu-s** |

The averages keep what could not shrink: TIFFs and HIFs are read whole
because they are the picture, and a date a head does not hold falls back to
the old paths. Per test raw (summary / preview, MB read): 5DS CR2 60.9 →
0.5 / 4.5, R5 CR3 40.6 → 1.0 / 3.9, A7R3 ARW 85.6 → 0.4 / 0.8, Z 7 NEF 47.3
→ 0.7 / 4.5, E-M1 ORF 17.3 → 1.8 / 1.8, X-T5 RAF 28.0 → 2.6 / 3.9, 645D PEF
62.8 → 0.3 / 0.3. On a 40 MB/s share that is the difference between a
second and a hundredth per file.

**IO-020, dates.** `exif::from_head`: 256 KB, kamadak-exif's `read_raw` with
`continue_on_error`, DateTimeOriginal by its number in any directory (a CR3's
CMT2 box; ORF and RW2 with their magic set to TIFF's; an RW2 by the Exif of
its embedded JPEG). The old chain stays behind it. All 558 dates the old paths
found are the same; two RW2s (DC-FZ45, DMC-GM1S) that had none now have
exiftool's. `the_head_agrees_with_the_whole_file` is the check.

**IO-021, rawler reads lazily** (`vendor/rawler/NUMA-CHANGES.md`): no
`populate`; padded views of data that runs to the end of the file (NEF, ORF,
RAF, RW2) no longer copy it for a dummy decode; a dummy DNG decode does not
decode; ARW's `as_vec` copies are slices. Previews: every one of the 491 the
corpus had is the same, pixel for pixel (hashed). Summaries: all identical,
and a Hasselblad L2D-20c DNG whose full decode fails now has one. The film
mode is looked for only in a Fujifilm (a 2 MB read in every other raw).

**IO-022, ORF.** The maker note's JPEG (`raw/orf.rs`): 23 of 25 ORFs, the two
SP-5xxUZ without. Grid thumbnail: 221 ms, 1.28 cpu-s, 27 MB → 30 ms, 0.09
cpu-s, 2.8 MB. It is `embedded_preview`, so the reference pane has a camera
view for an ORF and the exposure match (RENDER-008) runs for the E-M1 as for
every other make — that moves the E-M1's pixels. The median of the editor's
decode, against a copy whose maker-note JPEG is disabled: the two E-M1 test
frames **+0.28 and +0.27 EV**; the other 21 bodies −1.16 to +2.46 EV, most
within ±0.5 (E-450 −1.16, E-510 −1.10, PEN-F +1.49, the E-M5 II high-res
frame +2.46 — the 2.5 EV limit, worth a look). `previews::VERSION` 1 → 2, so no
proxy kept from before the match is read as one made with it.

**IO-023, the rescan.** 50 000 photographs, nothing changed: `apply_scan` on
the main thread 65 → 0 ms (the comparison is on the worker, `Scan::unchanged`);
`known_files` 12 ms stays. The minute timer is gone. A library on a local
disk is watched (`gio::FileMonitor` per folder, 257 at 50 000); an event is a
walk three seconds later. Coming back to the window stats the folders (257)
rather than walking the files (50 000), and walks only if one moved. A remote
or `/run/media` library has no monitors, only that. A drive plugged in or
pulled out is followed through the volume monitor.

**IO-024, JPEG export** (jpeg-encoder 0.6, MIT/Apache + IJG's notice; AVX2
found at run time): 24 MP 293 → 171 ms, 0.284 → 0.171 cpu-s, 2.017 → 2.027
MB; 50 MP 603 → 389 ms, 5.817 → 5.822 MB. Same quality tables, 4:4:4.

**IO-025, JPEG XL** effort 7 → 3 (lossy only): 24 MP 730 → 161 ms, 6.00 →
0.76 cpu-s, 1.064 → 1.026 MB; 50 MP 1 549 → 363 ms, 12.5 → 2.0 cpu-s, 3.39 →
3.01 MB.

**IO-026, the smallest preview.** `raw/embedded.rs`, thumbnails only
(`load_thumbnail`; the models and measures keep `load_scaled`): a CR3's
PRVW, or among two or more JPEGs in a TIFF-shaped raw the smallest framed as
the largest is. Per format, thumbnail ms / cpu-s: CR3 92 → 27 / 0.18 → 0.13,
NEF 85 → 18 / 0.073 → 0.017, DNG 80 → 37. Measured before the rule "two or
more, framed alike" was added — without it a PEF took its full JPEG over the
720-pixel one in its maker note and a Sigma DNG a 4:3 preview.

**Apple.** Everything above is in `numa-io` and `vendor/rawler`, which the
Apple workspace builds: its thumbnails (`thumbs::load`), rescans
(`sync_library`), dates, summaries, the ORF previews and the JPEG export get
the same. No JPEG XL there. jpeg-encoder runs its scalar code on ARM;
ImageIO (`CGImageDestination`) is the system's own encoder if that is not
enough — not measured.

### Left for later

- SSIMULACRA2 for IO-024/025 (a scorer is set up in
  `data-now/ssim/`, not run); the sizes say the same quality settings.
- Whether the match suits the older Olympus bodies and the E-M5 II's
  high-res frames (above); a per-body switch if not.
- IO-026's rule re-measured over the corpus, and the GUI rig for IO-023 (a
  file copied into a watched folder while the window is open).
- A cold decode from a USB disk with the lazy map (the explorer measured
  NVMe only); `WillNeed` over the data range if it is slower.
- CR2 has no mid-sized JPEG: jpeg-decoder's DCT scaling halves its thumbnail
  (5DS 119 → 58 ms) — a new crate for one format.
- `packaging/flathub-reference/cargo-sources.json` needs regenerating for
  jpeg-encoder (the photographer's, not an agent's).
## The render's last steps and the hand-over to GTK — 28 September

Branch `render-now`, from explore-render's findings (`docs/EXPLORE_RENDER.md`
on `explore-render`). Release builds, the ten raws of the switching set,
under the machine lock; in the app a headless Mutter 50.5 with GTK's Vulkan
renderer on the RX 9070 XT (`numa-scratch/speed-night/render-now/wl.sh`),
isolated catalogue. Every change here is byte-identical: `render_bench HASH=1`
gives the same 50 frames and histograms (plain, draft, edited, edited draft,
warm white balance) and the same frames through an AdobeRGB display profile
before and after, and after the merge of main the same as main's own.

**Four bytes a pixel to GTK (PERF-040).** Vulkan has no 24-bit texture, so
GTK converted every `R8g8b8` frame on the processor — 13 ms of CPU for
1920 × 1280, 20 at 2400 × 1600, measured in explore-render's `gsk_upload`
test; four bytes upload in 1 ms. `render::display::to_bgra` writes GDK's own
B8G8R8A8 (native to Vulkan, GL and cairo) with the display profile in the
same pass, so the one place a frame becomes a texture got cheaper rather than
gaining a pass. The card's read-back path (RENDER-017) already hands four
bytes.

**The encode off a table (PERF-041).** Scene-linear to eight bits was a `log2`
and two table reads per value, 7 of a 10–17 ms render. A table over the
float's own bits — 1 024 buckets an octave from 2^-24 to 2^8 — holds a code
wherever a whole bucket gives one, and says "work it out" for the one bucket
in thirty that holds a step; below the first bucket and for a NaN it is
worked out too. Exact by construction, and checked
(`the_table_encode_is_the_exact_encode`: every 97th float from 2^-26 to 2^9,
a plain and a steep curve, scene- and display-referred). Built once per set
of curves and kept.

| `render_bench`, 1920 px, ms / cores | before | after |
|---|---|---|
| first render of a photograph | 16.7–18.2 / 10 | 9.5–9.8 / 10 |
| Exposure tick | 10.5–10.9 / 12.5 | 4.5–4.6 / 10.3 |
| draft tick | 2.7–2.8 / 12.5 | 1.1–1.2 / 11 |
| edited (Clarity, mask, vignette) tick | 31–35 / 12 | 24.2–24.5 / 12.8 |
| a step's render, energy above idle | 2.6 J, 0.40 core-s | 2.0 J, 0.31 core-s |

In the app, an Exposure drag at fit (240 ticks, 4 s, Vulkan GSK, run back to
back): **18.8 CPU-s before, 9.4 after**, 238 of 240 renders put up either
way; of that the RGBA hand-over alone was 18.8 → 17.0.

**Leaving an edited photograph (PERF-042).** `save_edits` rendered the whole
proxy again on the main thread for the card's picture, then shrank it and
wrote the JPEG there: 36 ms in front of the next photograph. The last sharp
whole frame from the proxy is kept with its document (`render.on_screen`);
if the document is still that one it is the card's picture — at 1:1 the
frame behind the tile is, keyed the same way — and the shrink, the JPEG and
any render still needed run on a worker, the card asked for its picture once
it is written. Stepping on from a Z 6 with stored edits: the next
photograph's first frame 107–109 → 34–39 ms; the card's JPEG byte-identical.
And `tone::curve(+inf)` indexed one past its table (an infinity's index is
`usize::MAX`) — white now, asked before the index is made.

**The worker in the tick (PERF-043).** `gio::spawn_blocking` sat inside the
future, which the main loop runs after GTK's layout and paint of the frame
that asked; called in the tick, the wait before the worker starts went from
4.0–4.8 ms to nothing on eight steps (first frame 34–40 → 32–36 ms; the paint
still waits for the next vblank).

**What the Apple apps get.** The table encode is shared Rust: iPhone, iPad
and Mac renders get it unchanged, a render's last step at a fifth of its
cost; the `tone::curve` fix too. The rest is the GTK side.

### Left for later (PERF-044…049)

Stopped here on the photographer's word; measured, not done:

- **Render ahead (PERF-044).** A step's render is 20–23 ms of stack after
  the decode ahead and its colour stage (PERF-025); render the first frame in
  `prefetch` beside them — not when frugal, stopped with them — and the step
  is a swap and a paint. The frame has to match the rendered document
  (fingerprint, no faces, no masks map) and the working frame it was made on.
- **Temperature at 1:1 (PERF-045):** 12.3 cores and 40 of 120 frames today
  (explore-render); colour only the tile while the hand is down.
- **Histogram and backdrop from a small render at 1:1 (PERF-046):** the
  backdrop is 1.5 of 2.7 ms a 1:1 drag frame.
- **Display profile (PERF-047):** 3.3 ms and ~14 cores a frame through an
  AdobeRGB profile (`render_bench ICC=`), per pixel a `roundf` call on the
  x86-64 baseline; fold into the encode or make the pixel loop cheaper.
  Mutter maps windows itself only in its sdr-native and HDR modes, where Numa
  already converts nothing.
- **A cache point at the dragged stage, the prefix cache keyed on frame
  identity (PERF-048):** edited tick 24 ms when a late slider moves; the
  prefix's hash and copy are ~2 of a 4.5 ms tick.
- **f16 intermediates and AVX2 clones without FMA (PERF-049):** explore-build
  measured −12 % CPU on a slider with an all-v3 build; per-function clones via
  `multiversion`, no FMA so the bits match the Apple apps.

## The card's stages — 29 September

RENDER-019, RENDER-020, RENDER-021: the render on the card (RENDER-017)
sent the whole render to the processor for any stage it lacked, and the
photographer's log said so for point colour and masks. Which stages his
edits need was read from copies of the sixteen catalogues under
`Fotos/**/.numa/catalog.db` — 167 edited photographs, every one a Fuji RAF —
and they were built in that order:

| his edited photographs | on the card |
|---|---|
| before (0.30.2) | 46 of 167 |
| + the grade, point colour, black and white, vignette (RENDER-019) and masks (RENDER-020) | 118 |
| + luminance noise reduction and HDR, Clarity, Texture (RENDER-021) | **140** |

What still sends one of the other 27 to the processor: a mask's HDR,
Clarity or Texture (7), dehaze (8), spot removal (5), AI denoise (5), grain
(4), defringe and moiré (3), calibration, another working space, and
sharpening where the proxy shows it (at fit a 40 MP X-T5's radius rounds
to nothing, so that is one photograph).

**How.** Every number the card reads is still made by the processor's code,
now including `Grading::tints`/`shape`, `effects::vignette_shape`,
`local::ToneShape` and `detail::LumaShape`, which `apply_stack` itself uses
— so the two cannot drift apart, and the processor's output is bit for bit
main's (hashed over the refactored stages on the X-T5 and the E-M1 II).
The per-pixel stages go into `finish`. A mask is one pass: the processor
makes its field (`field_among`), the card keeps it in a slot while only the
adjustments move (`Mask::field_key`: the mask with its adjustments at rest,
and its pixels by address and every 997th cell), and the pass runs the
mask's white balance, colour noise reduction, basic adjustments, mixer,
point colours, curves (through the display and back, `scene_value_for` by
halving the base curve's stops), black and white, grade and Color on its
copy and fades it in. Which mask a pass is — and, for the planes, what it
reads and writes — is a record per dispatch behind a dynamic offset.
Luminance noise reduction and the local tone map read neighbourhoods, so
they work on planes of one value: log luminance, box blurs along the rows
and down the columns (a column's box slides down 64 rows), a guided filter's
a and b, the quarter-size plane, the glow, Texture's band, and the pivot as
two reductions.

**The same picture.** `the_card_renders_what_the_processor_does` has 42
edits now, 22 of them new, on three bodies raw and 13 of them finished: at
most 1 level in 8 bits, on at most 0.056 % of values (HDR, Clarity, Texture
and luminance noise reduction together), histograms within 0.033 %.

**Found on the way, not changed** (the processor stays bit for bit):
`LinearImage::oriented`, `cropped` and `downscaled` leave `white_point`
behind, so a mask's Temperature and Tint do nothing on a turned, mirrored or
cropped photograph, and nothing on the half-size draft while any slider is
dragged. The card does what the processor does (`card.rs`, a ponytail note);
the fix is one line in each of the three, and a changed export for those
photographs. (Fixed later that day: "Which device renders a drag".)

**A drag, measured** (`what_a_drag_costs`: the X-T5 at 2 400 px, 120 frames
at 60 a second, joules above idle for the card and the package; the
processor's frame is the half-size draft the app renders while a hand
moves, the cards' the whole proxy):

| edit | processor (draft) | RX 9070 | integrated GPU |
|---|---|---|---|
| untouched | 1.9 ms, 0.016 CPU-s, 0.08 J | 2.3 ms, 0.005 CPU-s, 1.25 J | 9.0 ms, 0.13 J |
| grade | 7.7 ms, 0.10 CPU-s, 0.61 J | 2.3 ms, 1.21 J | 10.1 ms, 0.12 J |
| point colour | 8.6 ms, 0.12 CPU-s, 0.59 J | 2.3 ms, 1.23 J | 12.0 ms, 0.17 J |
| a painted mask | 4.4 ms, 0.05 CPU-s, 0.24 J | 2.5 ms, 1.27 J | 11.1 ms, 0.16 J |
| three masks, one with colour NR | 139 ms, 2.16 CPU-s, 11.2 J | 3.2 ms, 1.41 J | 20.3 ms, 0.25 J |
| luminance NR 30 | 2.2 ms, 0.02 CPU-s, 0.17 J | 2.9 ms, 1.32 J | 16.4 ms, 0.21 J |
| HDR +50 | 4.2 ms, 0.04 CPU-s, 0.30 J | 2.9 ms, 1.27 J | 15.1 ms, 0.19 J |
| Clarity +30, Texture +20 | 6.6 ms, 0.07 CPU-s, 0.50 J | 3.6 ms, 1.34 J | 27.8 ms, 0.35 J |

Before the field was kept, one mask cost 11.5 ms on the RX 9070 and three
36 ms (packing a field to sixteen bits was 6.6 ms a mask).

In the app (Xvfb, cairo, 600 ticks, the steady last 8 s): DSCF3204 — two
found masks, HDR 18, luminance NR 48, a grade — **8.5 cores on the
processor, 2.3 on the card** (about 1 once the models had let go of it), and
0.79 against 0.80 J a frame; an untouched photograph's Exposure drag 1.0
core against 0.3, but **0.16 J a frame against 0.47**.

**The energy, plainly.** At 60 renders a second the RX 9070 does not clock
down between frames: it sits at 95 W in the bench and 31–48 W in the app
against 8–18 W idle, whatever the edit, so about 0.5–1.3 J a frame is its
floor. The processor's draft is cheaper than that for anything light, the
card only pays for itself in energy on a heavy stack (masks with noise
reduction: 11 J against 1.4), and the integrated GPU is the cheapest of the
three throughout — at 9–28 ms a frame. What the card always buys is a whole
proxy instead of a half one and the processor's cores back. Which device a
drag should use is the photographer's decision; nothing here changed it (he
made it the same day: "Which device renders a drag").

**The card itself** (RADV `shaderstats`, Mesa 26.2.3, GFX1201): every
render entry point uses 12–36 VGPRs and 108 SGPRs, no spills, no scratch,
32 subgroups a SIMD — the most there is; nothing to win in registers.
Timestamp queries per pass (a scratch patch, not kept) showed where the
time went instead: on the RX 9070 an untouched frame's passes were 0.69 of
2.4 ms, the rest the read back; on the integrated GPU 12.0 of 13.2 ms, and
the colour stage — camera values through the profile's two tables, which
only the white balance and the profile move — 5.7 ms of it, every frame.
It is now kept on the card with a key, as the processor has kept its since
PERF-006: 0.69 → 0.41 ms and 13.2 → 7.9 ms a frame. A mask's curves walked
the base curve's 53 stops per channel (16 ms of an integrated-GPU frame; 6.8
by halving), and Texture's column boxes summed 21 rows for every pixel (11.3
→ 1.6 ms a pass by sliding). Half precision for the frames between passes
(`enable f16`, where the device has it) was tried and dropped: all of them
in f16 moved 0.62 % of an untouched frame's values, only the blur's rows
0.61 % of the X-T5's "everything" — over the 0.5 % the test holds, for at
most half a millisecond on the integrated GPU.

**What is left, in the order his edits need it** (estimates for one agent):

- **A mask's HDR, Clarity and Texture** (7 masks, 5 photographs): the mask
  pass split in two around the local tone map's passes, pointed at the
  mask's copy — half a day.
- **Dehaze** (8): the dark channel and means per cell as reductions, the
  airlight from the two darkest cells, the small transmission grid's blur
  and the per-pixel stretch — half a day; a mask's dehaze with it.
- **Spot removal** (5): the patches as a list the pass looks up — a day,
  more if a heal blends.
- **AI denoise** (5): the denoised proxy is already made on the processor;
  hand the card that frame instead of the raw proxy — an hour or two.
- **Grain** (4): value noise over a 64-bit hash, emulated in 32-bit halves;
  a lattice cell chosen differently from the processor's is a grain of a
  different value, so it needs the coordinates computed exactly as there —
  half a day, and it may not hold the tolerance.
- **Sharpening where the proxy shows it, defringe, moiré, calibration** (1–3
  each): the plane passes carry sharpening in an hour or two once colour
  noise reduction's recolouring is its own pass; the others half a day each.
- **Rows by shared memory** on the integrated GPU and Apple: Texture's row
  boxes are 4.7–5.0 ms of its 27.8 ms frame.

## The processor's render, the rest of it — 29 September

Branch `cpu-render`, PERF-044…049 as `render-now` left them, and PERF-067.
The processor's render is still what an iPhone runs, what a laptop without a
usable card runs, what frugal mode runs, and what any stage the card does not
have yet runs, so everything here was measured with `NUMA_GPU=0`. Release
builds, the ten raws of the switching set, under the machine lock: stages in
`render_bench` (1920 px), drags and steps in the app under Xvfb (cairo) on an
isolated catalogue, CPU-s from the process clock, energy from the APU's
socket power (`gpu_metrics`) above an idle window taken just before.
Scripts and logs: `numa-scratch/research-2/cpu-render/`.

**Nothing on the settled screen or in a file moved.** `render_bench HASH=1`
gives the same 90 frames and histograms before and after (plain, draft,
edited, a warm balance, a late slider twice, an early one after it, the
edited frame again — the new ones rendered through the kept stages), the
same frames through an AdobeRGB display profile, and the colour stage's own
floats bit for bit. The app's window at fit and at 1:1, settled, is the same
screenshot to the pixel (`magick compare -metric AE`: 0). What changes is only
what is on screen while a hand is down, measured below.

**The display profile (PERF-047).** Its pixel was `(v * 4095).round() as
usize` per channel: a libm call on the x86-64 baseline and a float-to-`usize`
that is a branch. The whole part as `i32` and one more where the remainder is
half or over is exact (the remainder is exact by Sterbenz; tested over every
seventh float in 0..1 and both sides of every half). The BGRA hand-over with
a profile 3.0 → 2.0 ms a frame, one thread 23.3 → 13.9 ms. It was not folded
into the encode: the cost was the transform's own arithmetic, not a pass of
memory, and the histogram, the clipping overlay and PERF-042's kept frame all
read the sRGB frame the fold would have skipped.

**The stages kept by the frame (PERF-048, `numa-render/src/kept.rs`).** A kept
prefix was found by hashing the frame: a copy of the 30 MB, the hash, then a
copy of the answer over the copy. `apply_stack_kept` and `apply_pixels_kept`
take the frame's `Arc` and keep a `Weak` beside the answer — while it exists
the address is that frame's and the frame cannot change in place — so a hit
is one copy, and the geometry pass is skipped with it. A second point sits in
front of the grade (darktable's in-focus cache line, Core Image's
intermediates), kept once the same frame and settings are asked for twice in
a row, so an early slider never pays for a copy nobody reads. Its key is the
document with the late settings left out rather than the early ones named, so
anything added to the stack later is in it; mask pixels by address. The
editor's proxy, draft, backdrop and tile go through it; the Apple apps get it
by calling the same two functions.

| `render_bench`, ms a call | main | cpu-render |
|---|---|---|
| Exposure tick, sharp / draft | 4.4 / 1.2 | 3.2 / 0.9 |
| a late slider (vignette) on an edited frame | 24.5 | 5.5 |
| first render of a photograph | 10.1 | 8.9 |
| edited frame, Exposure tick | 24.4 | 23.0 |
| histogram (one core) | 0.9 | 0.6 |
| the BGRA hand-over, AdobeRGB profile | 3.0 | 2.0 |
| a step's render in an 8 s loop (colour, stack, histogram) | 0.28 core-s, 1.8–1.9 J | 0.26 core-s, 1.5 J |

The same at 2400 px, the proxy of the photographer's 1440p desktop (median
of the ten; ms a call / CPU-ms):

| `render_bench EDGE=2400` | main | cpu-render |
|---|---|---|
| Exposure tick, sharp | 7.7 / 73 | 5.1 / 60 |
| Exposure tick, draft | 1.7 / 19 | 1.3 / 16 |
| a late slider on an edited frame | 41.0 / 489 | 9.0 / 115 |
| first render | 20.4 / 217 | 17.2 / 175 |
| the BGRA hand-over, AdobeRGB profile | 4.5 / 67 | 2.9 / 42 |
| histogram | 1.5 | 0.9 |

**A kept frame's buffer (PERF-068).** At 2400 px a frame is 46 MB of floats,
over glibc's 32 MB ceiling for reusing freed memory, so the buffer each kept
render copied its stage into was a fresh mapping the kernel zeroed and faulted
in on every tick. The last render's buffer is kept as a spare (one; let go
with `forget_kept`): at 2400 an Exposure tick 6.2 → 5.0–5.2 ms and a late
slider 9.3 → 8.1 (twice each, alternating); at 1920 nothing moves. It holds
one frame's buffer while editing. Raising glibc's mmap threshold instead
(`GLIBC_TUNABLES=glibc.malloc.mmap_threshold`) changed nothing measurable:
it is that one buffer, not the passes' temporaries.

**1:1 while the hand moves (PERF-046, PERF-045).** The backdrop behind a
tile was meant to come off the draft during a drag, but at 1:1 nothing made
a draft — the tile is not the proxy's path — so every tick rendered the
whole proxy behind the tile, 7 of its 9 ms; the draft is made there now, as
at fit. And a colour change at 1:1 asked the tile's develop again for every
tick of Temperature — a part of the raw and its colour stage, 0.8 CPU-s on a
Z 6 — with the stretched proxy on screen meanwhile. Now the part on screen is
cut from the original before its colour stage once, at the draft's size, and
coloured afresh each tick; the develop is asked once, when the hand stops,
and the settled view is the same path as before. What was left of such a
drag was the draft's own colour stage, which at 1:1 is only the backdrop and
the histogram, so there it is a quarter of the proxy's edge (darktable's
preview pipe): the histogram while the hand moves has 3.4–4.6 % of its counts
in another bin than the half-edge draft's would — the same order as the
half-edge draft against the proxy, which every drag at fit shows, 2.0–6.2 %.
Colouring after the
reduction instead of before (`render_bench RECOLOUR=1`, the middle
1400 × 900 of each raw at 700): mean 0.002–0.22 codes, at most 0.95 % of
values over one code apart and 0.04 % over three, the worst single value 37
(Z 7, a clipped edge) — on frames that were the soft proxy before.

**The next photograph's first frame (PERF-044).** Where the processor renders
and nothing asks it to go easy, the decode ahead renders the neighbour's first
frame and histogram after its colour stage (stopped with it; not with a mask,
whose pixels the opening works out). The opening's first render takes it if it
is still that picture — the rendered document's fingerprint, the colour key,
the working frame it was made with, fit — and puts it up on a high-priority
idle that the opening's other requests fold into, not on the next tick. The
render is not saved, it is moved: it is wasted on a turn back, as the decode is.

**A colour drag's half proxy (PERF-067).** Every tick of Temperature, Tint or a
profile reduced the 30 MB proxy to half before its colour stage — the same half
each time. It is made once per proxy and kept (by the proxy's `Arc`).

In the app (Z 6 unless said; 120 ticks 16 ms apart, CPU-s over the drag):

| | main | cpu-render |
|---|---|---|
| Exposure at fit | 2.09 | 1.78 |
| Exposure at 1:1 | 8.52 (4.3 cores) | 3.32 (1.7) |
| Temperature at fit | 8.36 | 7.09 |
| Temperature at fit, A6000 (DCP) | 13.9 | 12.6 |
| **Temperature at 1:1** | **28.3 (14 cores), 43 of 120 frames** | **4.8 (2.4), 118 of 120** |
| Temperature at 1:1, A6000 | 28.4, 25 of 120 frames | 7.7, 118 of 120 |
| a step: first frame after the key | 31–39 ms | 12–13 ms |
| a step: the process's CPU until then | 115–163 ms | 14–23 ms |

Energy above idle over 600-tick drags (10 s; the first 3 s left out, the SMU's
socket power is a running average), three runs each: Temperature at 1:1
700–719 → 99 J, Exposure at 1:1 160–221 → 91–117 J. Below about four cores the
socket power did not repeat from run to run (Temperature at fit: 176–211 J on
main, 32–341 J here, at 4.1 against 3.5 cores), so those are not quoted; the
CPU-s are.

**AVX2 without FMA, and f16 (PERF-049).** A whole-binary AVX2 build (no FMA;
hashes identical) is the ceiling a clone per pass could reach, and on this
branch it is 0–5 %: a tick 3.2/3.1 ms, Clarity 7.7/7.7, Contrast 6.0/5.9, the
tone sliders 7.3/7.3, a DCP colour stage −3 %, and only the histogram
(0.9 → 0.6 ms) and Vibrance (−0.3 ms) worth naming. explore-build's −12.8 %
was before the table encode took the encode's `log2` out. What a tick has left
is `powf`, `log2` and `exp2` a pixel (Contrast, the tone regions, a DCP's
value axis), which a clone does not vectorise and a vector version would give
other bits — so no `multiversion` clones. Two portable changes instead, exact,
the iPhone's too: the HSV lookup's `rem_euclid(6.0)` (`fmodf`, a library call
on every platform) and `floor` (one on the x86-64 baseline) worked out
directly where the answer is the library's bit for bit (tested over the whole
range, zeros' signs and NaNs included) — a DCP colour stage 1–4 % quicker —
and the histogram counted into two sets of bins in turn, so a count no longer
waits on the one before (0.9 → 0.6 ms of a core a frame, the same counts).
f16 was not used: every intermediate on the processor feeds a 1:1 tile or an
export that has to stay bit for bit, and the kept stages would then differ
between a first render and the next; on the card it is another question
(RENDER-017's buffers).

**What the Apple apps get.** PERF-048's kept stages and PERF-068's spare
buffer by calling `apply_stack_kept`/`apply_pixels_kept` where they call
`apply_stack` now, and `forget_kept` when the editor closes; PERF-049's colour
stage and histogram with the shared code, as they are. The rest is the GTK side, and the same ideas carry: the frame made
ahead (044), the view coloured alone at 1:1 (045), the draft behind (046), the
kept half proxy (067).

### Left

- The step's first frame now waits on the opening's own hop to a worker and
  back (`off thread` 7 ms for nothing when the decode ahead is ready); taking
  a finished decode ahead on the main thread would halve it again.
- ~~A mask's own Temperature and Tint do nothing on a cropped, turned or
  flipped frame, nor on a draft made by `downscaled`~~ — fixed, "Which device
  renders a drag".
- The app's drags were measured at 1920 (Xvfb's screen); at 2400 the
  per-stage table above is the evidence.

## Which device renders a drag — 29 September

RENDER-022, and MASK-004's white point. Branch `render-policy`; scripts and
logs in `numa-scratch/research-2/render-policy/`.

**The photographer's decision** ("slim kiezen per bewerking"): the card's
stages above showed a discrete card costing three times the processor's
energy for a light drag and the same for a heavy one, with a fraction of the
cores. So a discrete card now renders only what would cost the processor
more than it costs the card; the rest renders on the processor, as before
RENDER-017. Integrated GPUs and Apple (unified memory: `numa_gpu::discrete`
is false) keep the card for everything it has, and frugal mode takes the
integrated lane, which is not discrete, so it keeps its choice too. The
develop (RENDER-010) is not touched.

**Where.** `card::Plan::heavy` — the rule, portable, beside the plan it
reads — and one check in `card_render.rs` once the plan is made: a discrete
card and not heavy, and the render goes to the processor. The same device
settles the frame as dragged it, so the picture does not change by a level
when the hand stops.

**What a stage costs the processor.** `what_a_drag_costs` now drags the way
the app does — the half-size draft through `apply_stack_kept`, Exposure a
hundredth of a stop a frame, so the stages before the operations are kept —
over the stages his edits use. The X-T5's 2 400-pixel proxy, 1 200-pixel
draft, CPU-s a frame at 60 a second:

| edit | CPU-s a frame | | edit | CPU-s a frame |
|---|---|---|---|---|
| untouched | 0.017 | | a mask: exposure | 0.028 |
| tone regions / whites, blacks | 0.043 | | a mask: gradient (tone, contrast) | 0.059 |
| contrast | 0.033 | | a mask: colour NR | 0.051 |
| vibrance, saturation | 0.022 | | a mask: grade | 0.096 |
| curves | 0.018 | | a mask: B&W, grade, Color | 0.180 |
| mixer | 0.076 | | a mask: mixer, point colour | 0.188 |
| black and white | 0.071 | | **a mask: one curve** | **1.565** |
| vignette | 0.030 | | two masks | 0.094 |
| grade | 0.105 | | luminance NR (kept) | 0.017 |
| point colour | 0.119 | | HDR +50 / Clarity +30 / Texture +20 | 0.041 / 0.040 / 0.070 |
| tone, contrast, vibrance, curves, mixer | 0.133 | | two masks, HDR 18, LNR 48, grade | **0.207** |

They add up: the last row, his DSCF3204's kind, is 0.206 summed from its
parts. So the rule sums them — CPU-ns a draft pixel per stage, a mask 12
plus its own adjustments at the photograph's rates — rather than naming a
stage: of his DSCF3204's four stages none is heavy alone. A mask's curves
are the outlier: `finish_mask` takes each value through the display and back,
and the way back is `scene_value_for`'s 48 halvings — 100 ms of wall a draft
frame on sixteen threads, where the card searches the base curve's 53 stops.

**The line.** At 60 frames a second the RX 9070 never clocks down: 0.47 J a
frame in the app untouched, 0.80 with DSCF3204's masks, HDR and grade (card
31–48 W above 8 W idle). The processor's package in the same app ran 0.16 J
a frame at one core and 0.79 at 8.5, about 5 W a core; the two meet at about
six cores, 0.1 CPU-s a draft frame — `CARD_FRAME_NS`. Below it the processor
is cheaper; at and above it the energy is about even and the card gives the
cores back and shows the whole proxy instead of half. The line is in CPU-s a
frame, not per pixel: a smaller proxy (a 1080p screen's 1 920) makes the
processor's draft cheaper while the card's frame costs what it did, so more
stays on the processor there. It is one processor's numbers (sixteen
threads, desktop); a laptop's discrete card would want its own.

**Before and after, in the app** (Xvfb, cairo, isolated copies of his
catalogues, 600 Exposure ticks 16 ms apart, the steady last 8 s, joules above
idle from the card's `power1_average` and the APU's socket power):

| | before (card) | after |
|---|---|---|
| DSCF1960, untouched | 0.46 J a frame, 0.3 cores | **processor: 0.15 J a frame, 0.9 cores** (card 8.9 W, idle 8.0) |
| DSCF3204, two masks, HDR, LNR, grade | 0.74 J a frame, 1.5 cores | card: 0.75 J a frame, 0.4 cores |

(Before, DSCF3204's first 66 frames went to the processor while a model
held the card; after, all 599 on the card.)

**A mask's Temperature and Tint on a turned, cropped or drafted frame.**
`LinearImage::oriented`, `into_oriented`, `cropped` and `downscaled` now
carry `white_point` on, as `lens::correct_geometry` and the 1:1 parts
(`zooming.rs`) already did; `apply_masks` reads it only for a mask's
Temperature and Tint, so nothing else moves. The card's plan dropped its
copy of the bug. Exports of a mirrored, turned or cropped photograph with a
mask that has Temperature or Tint change — the mask now warms or cools as it
does on an untouched frame. No test in the tree held the old pixels: the
fingerprint tests store no hashes, and `render_bench HASH=1`'s documents have
no mask white balance. `a_masks_temperature_moves_a_turned_cropped_frame`
fails on the old code (red over blue 116 → 116) and holds processor and card
within a level; `the_card_renders_what_the_processor_does`'s five masks,
turned and cropped, now warm on both, within 1 level on ≤ 0.020 % of values.

### Left

- On a discrete card the frame made ahead for the next photograph (PERF-044)
  is still off, as it is wherever a card may render; a light edit's step
  could now take it.
- A mask's curves on the processor: 1.5 CPU-s a draft frame, 48 halvings a
  value. Searching the 53 stops first, as the card does, and halving only
  within one would be the same answer in a fraction of the steps — if it can
  be shown bit for bit.

## The click encoder on the card, and half precision — 29 September

Branch `models-f16` (MASK-019, MASK-020, PERF-069). Release builds, every run
under the machine lock and `dev/capped.sh`; processor-seconds from
`getrusage`, energy above idle from the APU's socket power and the card's
board power, VRAM from `mem_info_vram_used`. The clicks are
`tests/click_survey.rs`'s: 219 of them on 60 frames — one raw from each of 50
raw.pixls.us bodies (CC0, drawn with a fixed seed) and the ten of the
switching set — each answered the editor's way, closer look and guided edge
included. The reference is SAM 2.1 Large (onnx-community's export,
Apache-2.0). Scripts, masks, sheets and logs:
`numa-scratch/research-2/models-f16/` (`py/iou.py`, `paired.py`, `bycat.py`,
`sameobj.py`, `layerdiff.py`; `PROGRESS.md` has every number).

**The card was answering another question (MASK-020).** Run on the card, the
survey's 219 masks agreed with the processor's at a mean IoU of 0.73; against
SAM 2.1 Large the card's median was 0.78 where the processor's is 0.91. Every
output of the encoder held against the processor's, layer by layer
(`py/layerdiff.py`), parts at the first attention's relative-position index:
the `Add` before it agrees to 8e-8, the `Cast` to int64 after it by 6 %.
SAM works those indices out from the input's shape — `(q − k) + (k − 1) ·
max(q / k, 1)`, whole numbers, cast to int64 — and with the batch dimension
left symbolic ONNX Runtime could not fold that chain when the session was
built, so it ran on the card, whose division is not exact: 14 / 14 came out a
hair under one, and 13 was cut to 12. Every attention then looked up its
neighbours' positions. Card sessions now fix `batch_size` and `batch` at one
(`numa_infer::build`); the chain is folded on the processor, and the card's
embedding is the processor's to 1.5e-4 of its mean. All 219 masks agree at IoU
≥ 0.999 (mean 0.99999), and the card got cheaper with it:

| SlimSAM encoder on the card, 12 frames | per photograph | processor | energy (socket + card) | VRAM peak |
|---|---|---|---|---|
| before | 225 ms | 0.35 cpu-s | 2.8 + 28.0 J | +11.0 GB |
| after | 217 ms | 0.32 cpu-s | 3.3 + 14.5 J | +5.1 GB |

`masks_probe::the_card_encodes_what_the_processor_does` holds the card
against the processor (run once without `GPU`, once with): 7× the mean apart
before, 1.5e-4 after. Kept embeddings (`NUMASAM2`) and kept click masks
(`sam::ANSWERS`, sam-5) from before are made again, since one from the card
cannot be told from one from the processor. ViTMatte has the same code with
its height and width open as well: 0.0058 apart on a 1024 tile, 8e-6 with
all four fixed — the "attention summed in another order" in "Inference
runtime" is this. Its tiles are not all one size, so it is left for its own
fix. SCUNet, EfficientViT and BiRefNet are unaffected (SCUNet's card answer
is the processor's to 1e-6; the other two are exported at a fixed size).

**A cheaper click encoder (MASK-019), not taken.** MobileSAM and
EfficientViT-SAM L0 (both Apache-2.0, weights and code) exported in SlimSAM's
I/O again — speed-night's scripts had gone: the prompt path is
segment_anything's own ONNX export with HF's padding point, held against the
model's own to 0.0; L0 sees the frame at 512 through a 2 × 2 average.

| against SAM 2.1 Large, 219 clicks | mean IoU | median | < 0.5 | hit | specks | holes | on edge |
|---|---|---|---|---|---|---|---|
| SlimSAM (shipped) | 0.734 | 0.912 | 54 | 213 | 13 | 11 | 1.390 |
| MobileSAM | 0.727 | 0.913 | 56 | 215 | 13 | 6 | 1.352 |
| EfficientViT-SAM L0 | 0.682 | 0.910 | 70 | 215 | 20 | 13 | 1.385 |

| encoder, 12 frames | processor | | | card | | | file |
|---|---|---|---|---|---|---|---|
| SlimSAM | 958 ms | 12.2 cpu-s | 36 J | 217 ms | 3.3 + 14.5 J | +5.1 GB | 23 + 17 MB |
| MobileSAM | 223 ms | 1.6 cpu-s | 11 J | 138 ms | 2.8 + 6.9 J | +0.9 GB | 28 + 16 MB |
| EfficientViT-SAM L0 | 237 ms | 1.3 cpu-s | 7 J | 121 ms | 1.9 + 4.8 J | +0.5 GB | 123 + 16 MB |

The photographer's bar is equal or better. L0 is worse beyond noise on small
things (−0.110 IoU, 95 % interval −0.198 to −0.027) and on buildings and
structure (−0.087). MobileSAM ties on IoU with the reference (−0.006, −0.043
to +0.030; every kind of click within noise) and has fewer holes, but where
it and SlimSAM chose the same thing (129 clicks) its border lies on the
photograph's edges less (on-edge −0.020, −0.036 to −0.004), and it takes the
larger thing more often (9.6 % of the frame on average, against 6.7 % and the
reference's 6.1): a car's bonnet for the car, a bicycle with the rider's arm
(`sheets/vehicles.jpg`). So SlimSAM stays, and the 4× on the processor is
left on the table. EfficientSAM and SAM 2.1 tiny were not surveyed again.

**Half precision on the card (PERF-069), measured and not taken.** The
WebGPU plugin has no precision option; it runs a model in the types the file
has, and RADV gives Dawn `shader-f16` (the RX 9070 has `shaderFloat16`), so a
float16 file runs in float16 throughout. Converted as SCUNet's was
(`onnxconverter-common`, input and output kept float32):

| on the card | time | energy (card) | VRAM | against float32 |
|---|---|---|---|---|
| EfficientViT-Seg B2 | 301 → 216 ms | 10.4 → 6.9 J | | "Buildings 100 %" on 26 of 30 frames: overflow |
| the same, its linear attention kept float32 | → 242 ms | → 7.8 J | | still 16 of 30; the rest group IoU 0.82–0.999 |
| SlimSAM encoder | 217 → 173 ms | 14.5 → 10.6 J | +5.1 → +1.4 GB | click IoU mean 0.979, ≥ 0.999 on 70 of 219 |
| BiRefNet (float16 since 25 Sep) | 887 → 435 ms | ~185 → 96 J | +12.3 → +8.0 GB | matte IoU 0.22–0.9997, median 0.996 |
| SCUNet (float16 since 21 Sep) | 200 → 108 s a 40 MP frame | | | 512 crop: max 2 codes, PSNR 60 dB |

The bar for half precision is indistinguishable from float32 (IoU ≥ 0.999,
nothing on the sheets). EfficientViT overflows float16 — its ReLU attention
sums over thousands of positions, and something past the attention too — and
SAM's click masks move on a fifth of the clicks, so neither is taken. SCUNet
passes and stays. BiRefNet does not pass (a palm frond and a bare tree
smaller in float16, `sheets/birefnet-f16-f32.jpg`), but float32 on the card
takes twice the time and energy and 15.7 of the card's 16 GB at its peak with
the desktop's share, which is no option; it stays float16, for the
photographer to know. (The same day float32 did fit, and took its place:
"BiRefNet in float32, a kernel row at a time" below.)

### Left

- ViTMatte on the card, the same index chain with its tile's size open —
  fixed the same day (MASK-021, "The proxy's edge, ViTMatte on the card, a
  mask's curves", below).
- Two raws of the corpus panic in `editor_proxy` (`proxy.rs:260`, a slice
  whose end is before its start): the ILME-FX2's and the GH5S's.
- SlimSAM's 5 GB on the card is its four global attentions; the head-at-a-time
  rewrite the iPhone gets would bring that down (at 197 → 235 ms, measured
  26 September) if a card with less memory needs it.

## The proxy's edge, ViTMatte on the card, a mask's curves — 29 September

Branch `fixes-30` (PERF-030, MASK-021, PERF-070): the three things the two
sections above left. Release builds, every run under the machine lock and
`dev/capped.sh`; processor-seconds from `getrusage` or render_bench's
cores × wall, energy above idle from the APU's socket power. Notes and logs:
`numa-scratch/research-2/fixes-30/` (`PROGRESS.md`, `proto/inv.rs`).

**The proxy's panic was a lens, not a body.** `editor_proxy`'s mosaic route
panicked at `proxy.rs:260` on the GH5S frame (raw.pixls.us 2603, the M.Zuiko
7-14 at 14 mm): a slice whose first column was past its last. The lens
profile's correction bulges mid-radius, and `geometry_fit` only keeps the
sources inside the circle through the corners, so a pixel near the middle of
a side reads from past the frame's side; the box around it, cut to the
frame, turned inside out. `Frame::mean`, which every proxy pixel goes
through, now holds the box's centre inside the frame, as the full develop's
bilinear holds the edge. The ILME-FX2 frame (8807) never panicked: rawler has
no entry for the body, so it does not decode at all — an error, and a camera
to add to rawler with a colour matrix one day.

The whole corpus through the editor's route, card off (`NUMA_GPU=0`) and
Numa frugal, at 2400, 1920 and 1024 (`raw::proxy`'s
`every_frame_in_a_list_makes_a_proxy`, the 549 files of `raws-cc0` and the
ten of the switching set, 176 s): 8 panics before, on three frames, all the
same cause (2603 and 2607, GH5S with the 7-14; 5146, Z 9 with the Z 14-30 at
18 mm, at 2400 and 1920); none after. 1 590 proxies made; 87 errors, all
rawler's (bodies it does not know, NEF High Efficiency, truncated files). The
ten's proxies and renders hash as before (`render_bench HASH=1`, 110 lines):
none of them has a source outside its frame.

**ViTMatte on the card (MASK-021).** The same fault MASK-020 fixed for SAM's
batch: ViTMatte's relative positions come from its input's height and width
through a division and a cast to whole numbers, and with those left open the
chain ran on the card. One 1024 tile (models-f16's `cardcheck.py`): card
against processor mean |d| 1.6e-3, max 0.38; with height and width fixed,
5.9e-8 and 1.2e-5. Refine edge's tiles are 1024 unless the band's frame is
smaller on a side, so `numa_infer::Model::load_sized` builds a card session
for one height and width (on the processor the size stays open and nothing
moves), and `matte::vitmatte` builds it again when a band wants another size.

`masks_probe::the_card_refines_what_the_processor_does`, the ten frames
whole (2400 px, tiles of 1024) and at 700 px (tiles of 448–512 by 672), an
ellipse for the first look, all 20 kept from the processor and held against
the card: before, 1.2e-3 to 4.0e-3 of alpha on average and up to 0.997; after,
1.1e-8 to 5.6e-8 and up to 6.3e-5. The processor's 20 mattes are bit for bit
the old code's. A band of another size costs one session build on the card
(the 20 mattes with three builds, 7.8 s). Refined mattes are never kept on
disk, so nothing old is read back.

Two other ways were measured beside it, on one photograph's tiles of 1024²,
1024 × 736, 512 × 1024 and 320 × 640 (Python onnxruntime 1.30 and the
plugin; `numa-scratch/research-2/vitmatte-card/`). A Round before each of
the 25 casts from float to int64 (of 162 to int64) is as exact — 7.5e-6 to
1.4e-5, like the fixed size — and one session would take every size, but
the file carries no types, so a rewrite would have to infer them, and the
chain stays on the card: a 1024 tile 180–194 ms. Padding a smaller tile to
1024 moves the processor's own answer by up to 0.99 (a 512 × 1024 tile
against itself padded), so it is out. The fixed size is also the cheapest,
because the folded chain no longer runs at all: a 1024 tile on the card
151–172 ms and about 30 J above idle open, 128–138 ms and 25 J fixed
(interleaved, 12 and 20 runs), 0.016 against 0.010 processor-seconds.

**A mask's curves (PERF-070).** `finish_mask` takes every value of the frame
up to the display, through the curves, and back with `scene_value_for`,
which halved 48 times over the whole curve. The X-T5's 2400 proxy, its 1200
draft, an Exposure drag with render_bench's `curved` sky (a curve and a red
curve on the edited frame's graduated mask):

| | ms a frame | CPU-s a frame | J a frame (socket, above idle) |
|---|---|---|---|
| before | 145.7 | 2.15 | 11.2–12.5 |
| after | 30.0 | 0.43 | 1.8–2.0 |
| the same frame, no curve | 9.5 | 0.12 | |

The inverse has to be the halving's answer bit for bit — it decides the
export — and the halving's answer is a float, not the real inverse. Where the
curve is steep enough, it is the first float at which `at_stops` reaches the
value, and the table says where that is: the closed-form inverse of the
quarter-stop table (or of the toe's power law), then halvings over the floats
in order from four either side of that guess, widened where the guess was
further out (near zero, where floats are dense). Above `CAMERAS[48]` (0.986
on the display, four stops over grey) the table's last steps rise less per
float than a float's rounding, `at_stops` steps back here and there, and
which crossing the halving lands on depends on the path it took — 15 080
answers differed there. So the halving runs its whole path, but asks the
curve only where the table's guesses for a few floats either side cannot say
which way it goes. `at_stops` itself lost its `floor` and `fract` calls to
libm: for a positive `at` the cast is the floor, and `fract` is defined as
`at - trunc(at)`. Single-threaded: 590 → 50 ns a value below 0.986, 561 →
114 above.

Checked on every float `scene_value_for` can be handed — the 112 083 548 in
[1e-4, 0.9999], and a NaN — against a verbatim copy of the old code, and
`at_stops` on every float in its table's range: 0 differ
(`the_inverse_is_the_halvings_on_every_float`, 7 s on sixteen threads; every
61st float in the ordinary run). `render_bench HASH=1` on the ten, a curved
frame included, hashes as before.

### Left

- ~~The ILME-FX2 in rawler (no camera entry).~~ Done: "The Sony ILME-FX2
  in rawler" below.
- A mask's curves still cost 0.30 CPU-s a draft frame over the rest of the
  mask: the round trip's `log2` and `exp2` and the guess. `Plan::heavy` now
  counts them at 320 ns a draft pixel rather than 1 600, which still sends
  a drag over any whole 3:2 proxy (1 400 px and up) to a discrete card; a
  narrow crop's now stays on the processor.

## BiRefNet in float32, a kernel row at a time — 29 September

Branch `birefnet-f16` (PERF-071, MASK-008). Seventy-five frames: the 21 CC0
JPEGs of models-f16 (the palm and the bare tree among them), 20 of the
photographer's, 24 raw.pixls.us raws (half size through LibRaw) and 10
portraits, each held against the model in float32 on the processor — its
logits (`py/run.py`) and its subject through `matte::subject`, as the
Subject chip asks it. Release builds, every run under the machine lock and
`dev/capped.sh`; time, processor-seconds and energy above idle (APU socket,
card board power) from `masks_probe::subject_cost`, VRAM from
`mem_info_vram_used`. Scripts, sheets and every number:
`numa-scratch/research-2/birefnet-f16/` (`PROGRESS.md`).

**Why float16 is not the model's answer on the card.** Held against float32
module by module (63 outputs, `py/tapcmp.py`), float16 drifts to 2 % through
the backbone and then jumps where a convolution sums over many channels: 2 →
8 % at the squeeze module's 3 × 3 over the 5 760-channel pyramid, 18 % after
decoder_block1's 1 × 1 over 1 280. The WebGPU plugin adds up in float16 too:
one Conv on its own, from the same float16 inputs, comes out 1.0e-3 off at 64
channels, 4.6e-3 at 1 280 and 2.8e-2 at 5 760 × 3 × 3, where adding up in
float32 would leave 1.8e-4 (`py/accum.py`). CUDA's float16 adds up in
float32; this does not. Nothing mixed passes: the backbone in float32 changes
nothing, the decoder's convolutions in float32 lift the worst frame to 0.93,
the whole decoder to 0.92 with 25 of 75 still under 0.999, and even the
float16 file on the processor, which adds up in float32, misses on five of
six hard frames (0.9745 at worst). Mixed files also answered nothing at all
through ONNX Runtime 1.28, the one Numa links, where Python's 1.30 ran them.

**What makes float32 fit.** Its trouble was memory — 12.3 GB, 15.7 of the
card's 16 with the desktop — and most of that is the exporter's deformable
convolution, which gathers all K × K taps at once: in decoder_block1
twenty-odd tensors of [1, 64, 49, 256, 256], 822 MB each. `numa_infer::rewrite`
now works each of the ten (3 × 3 and 7 × 7) out a kernel row at a time:
slices of the same indices and weights, put pixel-major first (they are the
small tensors), so the gathered row is already [H·W, K·C] and meets
W[:, :, row, :] in one MatMul; the rows are added. The same arithmetic in
another order — on the processor the logits move by 1e-4 at most — and a
miniature in `rewrite::tests` holds it to the exporter's to 1e-5. Not on
Apple, where the lite model runs on the processor and it was not measured.

| 75 frames, per photograph | time | processor | energy (socket + card) | VRAM | subject against float32 |
|---|---|---|---|---|---|
| float16 as shipped 25–29 Sep, card | 415 ms | 0.10 cpu-s | 0.2 + 75 J | +8.2 GB | ≥ 0.999 on 33, min 0.26 |
| float16 by row, card | 370 ms | 0.11 cpu-s | 0.5 + 66 J | +5.6 GB | the same (min 0.27) |
| float32 as exported, card (models-f16, 21 frames) | 887 ms | | ~185 J | +12.3 GB | |
| **float32 by row, card** | **566 ms** | 0.11 cpu-s | 0.2 + 111 J | +8.1 GB | **1.0000 on all 75** |
| float16, processor (6 frames) | 7.5 s | 43 cpu-s | | 6.0 GB | |
| float32 by row, processor (6 frames) | 3.3 s | 36 cpu-s | 168 J | 4.4 GB | |

On the card float32 by row is the processor's answer — one 8-bit code apart
at most. Float16's subject lost part of a palm frond (IoU 0.26), a bare tree's
branches (0.85), the end of a fence (0.86) (`sheets/f16-vs-f32.jpg`). So the
Linux download is onnx-community's `model.onnx`, 973 MB, MIT as before, saved
as `birefnet_f32.onnx`: a new name, so subject masks kept from float16 are
made again. The float16 `birefnet.onnx` answers until the new file is fetched
(Preferences › Downloads lists BiRefNet as not installed) and is deleted once
it has. The price on the card is 150 ms and 36 J a Subject, with the card's
memory as it was; on the processor float32 is the faster of the two.

### Left

- The mirror has the float16 file only; `birefnet_f32.onnx` is to be put
  beside it (until then it comes from Hugging Face).
- Nothing tells a photographer who has the float16 file that there is a new
  one but the Downloads page.
- Mixed float16/float32 files answer nothing through ONNX Runtime 1.28 on the
  card; not chased, since nothing mixed passes.

## Smaller model downloads — 29 September

Branch `model-compress` (START-017). Copies of all 17 model files, every
SHA-256 the one `models.rs` names; zstd 1.5.7 for packing, `ruzstd` 0.9 in
Numa for unpacking; every run under the machine lock and `dev/capped.sh`.
Scripts and numbers: `numa-scratch/research-2/model-compress/`.

**Lossless.** Model weights are floats, and zstd finds little in a float's
four bytes as they come: the exponent byte repeats, the low mantissa bytes
are noise, and interleaved each hides the other. Grouped by position — every
first byte of a tensor, then every second, … (HDF5's and Blosc's shuffle) —
the exponents sit together. `dev/pack-models.py` finds each float tensor's
bytes by reading the ONNX protobuf (a `.onnx.data` file by the `.onnx`
beside it), writes a header of those ranges and the file with them grouped,
through zstd. `numa_io::models::unpack` undoes it a tensor at a time.

| all 17 files | zstd 3 | zstd 9 | zstd 19 |
|---|---|---|---|
| as they are (2 008.9 MB) | 1 748.1 | 1 746.8 | 1 747.2 |
| **bytes grouped** | 1 616.0 | 1 605.9 | **1 592.3** |

Per model grouped at 19: BiRefNet 972.7 → 742.4 MB (−24 %), lite 191.8 →
149.5, LaMa 208.0 → 173.0, Restormer 107.1 → 88.6, ViTMatte 103.9 → 87.5,
PP-ResNet 102.6 → 85.9, the rest −15 to −22 %. Unpacking all 17 in Numa:
4.2 s on one core, BiRefNet 1.13 s; each lands on the original's digest.

**Float16 weights, float32 arithmetic.** A `.f16.zst` keeps the weights of
the convolutions and matrix products (≥ 1 024 elements; everything else,
index arithmetic included, stays float32) as float16, and unpack widens them
back: the file on disk is an ordinary float32 model with those values, so the
card, the processor and BiRefNet's rewrite work as before. Half the bytes —
but the answer moves by what rounding the weights moves it, and it was taken
only where that is less than the card already moves it from the processor
and every decision is the same. Measured against the originals on the
processor:

| model | float16 weights against float32 | the card against the processor |
|---|---|---|
| BiRefNet (75 frames) | IoU 0.9923 and 0.9961 on two frames | 1.0000 on all 75 (PERF-071) |
| SlimSAM (219 clicks) | two clicks 0.91 and 0.95 | 219 ≥ 0.999 (MASK-020) |
| EfficientViT (75 frames) | 3 found chips differ, worst IoU 0.92 | none, 0.9999 |
| LaMa (16 fills) | up to 19 codes inside the hole | 1 code |
| ViTMatte (12 edge tiles) | 1 code, 4.5e-3 | 1.2e-5 (MASK-021) |
| **SCUNet** (16 crops) | **1 code on 1.1 %, 84 dB** | its float16 file: 2 codes, 60 dB |
| Restormer, RealPLKSR | 1 code, 89 / 98 dB | not measured (the card was busy) |
| YuNet, SFace, eye, PP-ResNet, YOLOX | same faces, matches, eyes, animals, boxes | never on the card |
| BiRefNet lite, 512 (iPad) | ≥ 0.999 on all 75, 14 codes at most | processor only |

So SCUNet's weights come as float16 (73.1 → 31.7 MB) and everything else
whole. Weights of BiRefNet's matrix products alone or convolutions alone
were no better (0.9899, 0.9978 on the same frame).

### Left

- The 17 packs are for the mirror; until they are on it every download
  falls back to the original, as before.
- The iPad fetches through `numa-ffi` in Numa-mac, which still asks for the
  originals; `models::packed` and `models::unpack` are there for it.
- Restormer and RealPLKSR could take float16 weights if the card turns out
  to move them by a code as well.

## Sentences in parts, for other languages — 29 September

APP-007. The Apple app puts every word through a String Catalog, so a
language is added there rather than in code; the photographer's, 29
September: "alles van die i18n strings maken en voor nu alles Engels houden".
What the crates said came across as finished English sentences — "Exposure
and 3 more", "Added Sky 2", "person + animal", a tooltip of numbers — and a
catalog can only look up a sentence it knows whole.

So the crates say those things in parts, and the English is the parts'
`Display`: `history::Step` and `Change` (a word, or what was done to which
mask), `masks::MaskName` and `MaskLabel`, `segment::Named`, and
`notes::DetailLine` with its numbers. `steps()`, `mask_label`, `name_for` and
`cull_detail` are unchanged to their callers and format from the parts, so
the two cannot drift: the Linux interface reads exactly what it did, and the
tests that pin its wording (`src/ui/window/tests.rs`, `segment`, `masks`)
pass as they were. The tooltip was checked against the function it replaced
over 108 000 combinations of measurements before that function went: the
same text for every one.

The words themselves — "Exposure", "Sky", "soft" — stay English in the
crates. They are fixed, so the app looks each one up under its English, and
Numa-mac's `numa-ffi` has a test that fails when the panel, Storage, the
models, the masks or the profile picker hand over a word its catalog does not
hold.

### Left

- Errors are sentences, often with a system's message inside; they stay
  English.
- Analyse's closing line is assembled in `analysis::summary` from parts that
  are already values (`learn::Outcome`); the Apple app words it from those.

## The Sony ILME-FX2 in rawler — 29 September

Branch `sony-fx2` (IO-008). dnglab has no entry for the body, on `main`
or on any branch, so `vendor/rawler` got one (`NUMA-CHANGES.md`): the
matrices of rawler's own ILCE-7CM2, which has the same 33 MP sensor. Its D65
is the matrix LibRaw and rawspeed give the FX2. Release builds, every run
under the machine lock and `dev/capped.sh`. Notes and logs:
`numa-scratch/research-2/fx2/`.

- **All four FX2 frames of raw.pixls.us** (8807 uncompressed, 8808
  compressed, 8809 lossless, 8810 lossless at 4608 × 3072) decode, make a
  proxy at 2400, 1920 and 1024, and render (`render_bench HASH=1`). Beside
  the camera's JPEG the matrix is right (chroma error 0.055 on three frames,
  0.086 on the fourth).
- **Nothing else moves.** `render_bench HASH=1` on the ten test raws, two
  7M4s, a 7CM2, an FX3 and an FX30, before and after: 165 lines, identical.
- **The corpus sweep** (`every_frame_in_a_list_makes_a_proxy`, `NUMA_GPU=0`,
  fixes-30's 559 files plus 8809 and 8810, one thread): 1 590 proxies and 93
  errors before, 1 602 and 81 after, no panics either way. The twelve are the
  FX2's; no new error.
- **No profile of Numa's own yet (RENDER-023).** The fitter fits it (matrix
  0.0864 → 0.0845 on the held-out frame), but the four frames are one scene
  within 35 minutes, so the held-out one measures a scene the fit saw. It
  waits for a second scene, as 156 other bodies do.

Still failing as a camera rawler does not know, of the sweep's bodies: the
ILCE-7M5 (its ARW6 compression has no decoder in rawler, dnglab issue #681),
the Coolpix P7700, the Pentax K2000, the Panasonic DC-FZ45 and DMC-GM1S (the
body is there under another name or aspect), and four Hasselblads (X1D II
50C, CFV-50c twice, CFV 100C: model strings rawler does not match). Nothing
for them upstream to take.
