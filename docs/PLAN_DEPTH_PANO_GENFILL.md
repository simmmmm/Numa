# Plan: Depth mask, Panorama, Generative fill

27 September 2026. Research and reading only: no model was downloaded or run
for this memo, so every speed and memory figure below is either a published
number (the source is named) or an estimate (it says so). This is not legal
advice. It follows the rules in `Numa-mac-next/docs/LICENCES_AUDIT.md`: the
paid Apple build needs weights **and** training data that allow commercial
use; the free Linux build (PolyForm Noncommercial) can take NC models.

| Feature | Recommendation | Apple | Linux | Effort |
|---|---|---|---|---|
| Depth mask | Depth Anything V2 **Small** (Apache-2.0) on every platform; test DA3MONO-LARGE (Apache-2.0) as the Linux/iPad-M upgrade | yes | yes | 3–4 days |
| Panorama | Numa's own Rust stitcher in `numa-render`, beside `bracket.rs`; no OpenCV | yes (capped size on iPhone) | yes | 8–12 days |
| Generative fill | v1.1: a better **Remove** everywhere (LaMa, no prompt) + a **ComfyUI hook** on Linux. No prompt-based generation in the paid app yet | better Remove only | better Remove + ComfyUI | 3–5 + 4–6 days |

---

## 1. Depth mask

### What it is

A new `Shape::Depth { near: f32, far: f32, feather: f32, picked: bool }` in
`crates/numa-core/src/mask.rs`, beside `LuminanceRange`, which it copies: the
document stores the range, never the pixels. The pixels come from a depth map
worked out from the photograph and kept like the segmentation
(`numa-render/src/segment.rs`), then upsampled to the frame with the guided
refine that `segment::refine(guide, coarse)` already does. Combined with the
existing Add/Subtract of another mask (commit 3ce26d1), "Background ∩ far"
and "Subject minus near" come for free.

Monocular models give **relative** (affine-invariant) inverse depth, not
metres. So the range is on each photo's own 0..1: normalise the model's
output between its 1st and 99th percentile, 0 = nearest. The sliders then mean
the same thing on every photo, and a click on the photo ("pick", as the range
masks do) sets the band around the depth under the finger.

### Candidates

| Model | Weights licence | Training data | Size | Quality for photos | ONNX | Verdict |
|---|---|---|---|---|---|---|
| **Depth Anything V2 Small** | **Apache-2.0** | teacher: synthetic only (BlendedMVS, Hypersim, IRS, TartanAir, VKITTI 2); student: 62 M pseudo-labelled real images (BDD100K, Google Landmarks, ImageNet-21K, LSUN, Objects365, Open Images V7, Places365, SA-1B) | 25 M params; ~50 MB fp16, ~99 MB fp32 | good, soft edges at 518 px | yes (onnx-community) | **Pick, all platforms** |
| Depth Anything V2 Base / Large / Giant | **CC BY-NC-4.0** | same | 97 M / 335 M / 1.3 B | better | yes | Linux only; superseded by DA3MONO-LARGE below |
| Depth Anything 3 Small / Base | Apache-2.0 | "public academic datasets only" | 80 M / 120 M | any-view model, used as monocular; no photo comparison vs V2 published | third-party export scripts | watch |
| **DA3MONO-LARGE** (monocular relative) | **Apache-2.0** | public academic datasets | 350 M (~0.7 GB fp16) | the best permissive option: predicts depth, not disparity | third-party export scripts | **Evaluate for Linux (GPU) and iPad M** |
| DA3-Large / Giant | CC BY-NC-4.0 | — | 0.35 / 1.15 B | — | — | no need, DA3MONO-LARGE is Apache |
| Apple Depth Pro | Apple's sample-code licence (use, modify, redistribute; no trademarks). Issue #66 asks about commercial use, unanswered | Apple's mix, not itemised | ~950 M params, 1536 px | sharp, metric | community exports | too heavy for a phone; not worth the licence doubt |
| MiDaS 3.1 / DPT | MIT | a dozen mixed sets incl. 3D-movie frames (not re-verified) | 20 M–345 M | clearly below DA V2 | yes | no |
| Marigold v1-1 | depth-v1-1 **OpenRAIL++-M** (fine-tuned from SD 2); depth-hr-v1-1 Apache-2.0 | synthetic (Hypersim, VKITTI) on an SD 2 base (LAION) | ~1 B (diffusion) | very sharp | awkward (multi-step diffusion) | no: seconds on a GPU, minutes on a phone |
| **AVDepthData** (Portrait / auto-depth HEICs) | Apple's, free | — | 0 | hardware depth, but low-res and subject-centred; only iPhone photos with a person/cat/dog (iPhone 15+ auto) or Portrait mode | n/a (ImageIO auxiliary disparity) | later, optional: use it when present |
| Vision framework | — | — | — | Apple has no public monocular depth request (checked WWDC26 material) | — | n/a |

Signals worth noting: Apple itself distributes Depth Anything V2 Small as a
Core ML model on its models page (`apple/coreml-depth-anything-v2-small`),
33.9 ms on an iPhone 15 Pro Max on the Neural Engine (Apple's number). The
same residual dataset risk as the audit's "low residual risk" class applies:
VKITTI 2 is CC BY-NC-SA 3.0, BDD100K and Places365 are research-only; GitHub
issue #320 asks about this and has no answer. That is the same position as
BiRefNet or LaMa today.

### Per platform

- **Apple:** DA V2 Small through ORT (CoreML EP, as the others), input 518 px
  on the long edge. Expected well under 0.5 s with the CoreML EP (estimate:
  ORT's EP is slower than Apple's native 34 ms). Measure the peak and add a
  `ROOM` constant as `matte.rs` does; the model is a fifth of BiRefNet lite, so
  expect a few hundred MB.
- **Linux:** the same file, on the GPU list in `numa-infer` (`ON_GPU`) if it
  measures faster. Then run DA3MONO-LARGE on the 150-photo set, as BiRefNet
  was chosen; if it wins, ship it on Linux (and iPad M later) under the same
  mask shape. Its Apache licence means no Linux/Apple split is forced.
- **Add-ons entry:** "Depth Anything V2 Small · Depth masks · Apache-2.0 ·
  50 MB", mirrored on the `models` release (fp16 export), plus its line in
  `MODELS-LICENSES.txt` and About.

### Effort

~400 lines: `numa-render/src/depth.rs` (run, normalise, cache, refine: ~150),
the shape and its alpha in `mask.rs` (~80), the models entry, Linux mask panel
(Near/Far/Feather sliders + pick, ~100), iOS panel (~80 Swift + FFI). **3–4 days.**

### Risks

- Relative depth jumps between photos of a series; "sync to others" copies
  the range, which is fine because both are normalised per photo, but a sky
  (infinite) can steal the far end. Clamp the sky: EfficientViT's sky class
  is already there, force it to 1.0.
- Edges at 518 px are soft; the guided refine fixes most of it; hair against
  a far background stays hard. Pair with Subject when it matters.
- A better model later changes existing masks slightly (they are re-derived).
  The same trade Segment already made on purpose.

---

## 2. Panorama

### Where it fits

Exactly where the HDR bracket merge sits:

- core: `crates/numa-render/src/bracket.rs` (merge) and `align.rs` (Ward MTB,
  translation only). A panorama is a new `numa-render/src/pano.rs`.
- Linux: `src/ui/window/merge.rs` (`merge_selection` → `open_merged`) and the
  button in `src/ui/window.rs:726`. Add "Merge to Panorama" beside it; the
  frames go through `raw::decode_with_exposure` as now.
- Apple: `numa-ffi/src/ai.rs` `Editor::merge` + `merged_edits`, and the
  `bracket: [String]?` session in `LibrariesView.swift`/`PhoneEditor.swift`.
  Generalise to `merge: (kind, paths)` and add `Editor::panorama`.
- The merge today lives only in memory ("merged:" path, no file). For a
  panorama that is worse (minutes to rebuild, huge). **Decision needed:** write
  the result to disk (16-bit linear TIFF, or a linear DNG via rawler's
  `DngWriter`) so it joins the catalogue like any photo. Recommended.

### Pipeline (own code, ~1,200–1,800 lines of Rust)

1. **Decode small.** Each frame decoded (Draft demosaic, as the merge does),
   lens-corrected, downscaled to ~1.5 MP grey, then dropped.
2. **Features.** AKAZE or ORB on the grey images. SIFT's US patent expired in
   March 2020 (it is in OpenCV main since 4.4), so SIFT is legally fine too,
   but ORB/AKAZE are faster and enough for rotation-only handheld frames. The
   pure-Rust `akaze` crate (rust-cv, MIT) exists but was last released June
   2020; try it, and if its dependency versions fight ours, write ORB
   (FAST + oriented BRIEF, ~400 lines).
3. **Matching + RANSAC.** Hamming brute force with a ratio test; RANSAC on a
   **rotation-only** camera model (focal from EXIF's 35 mm-equivalent focal
   length). `arrsac` (MIT, maintained, 2025) or ~80 lines of our own.
4. **Bundle adjustment.** Rotations (3 per frame) + one shared focal,
   Levenberg–Marquardt on dense normal equations: with N ≤ 20 frames that is a
   61×61 system; no solver crate needed.
5. **Projection.** Planar for 2 narrow frames, cylindrical by default, spherical
   when the frames form more than one row. Auto "straighten" by the median up
   vector (wave correction).
6. **Exposure.** The data is scene-linear, so compensation is one gain per
   frame from the overlap means (least squares), no curves. Vignetting is
   already the lens correction's job.
7. **Seams.** A dynamic-programming minimum-difference seam per overlap
   (~60 lines): cheap and it routes around a walking person. Skip graph cut.
8. **Blending.** Multi-band (Burt–Adelson, 5 levels) **only inside the overlap
   strips**, plain copy elsewhere: this keeps the pyramids small.
9. **Crop.** Numa has no alpha, so set the document's crop to the largest
   inscribed rectangle and let the photographer widen it with the existing crop
   tool (the undefined corners filled with the edge colour, or later with LaMa).
10. Later: bracketed panoramas (group by exposure, `bracket::merge` each
    position, then stitch).

### Why not OpenCV (Apache-2.0) or others

OpenCV's `stitching` module is Apache-2.0 since 4.5 and does all of the above,
but: the official iOS framework is ~180 MB, a minimal arm64 build still tens of
MB; it adds a C++ toolchain to the Rust/UniFFI build of both apps; and the
Rust `opencv` crate needs a system OpenCV (fine on Linux, a pain for iOS and
Flathub). Hugin/libpano13 are GPL (no for the paid app). Apple's Vision
`VNHomographicImageRegistrationRequest` gives a pairwise homography, but it is
made for stacking (large overlap), gives no bundle adjustment, and would split
the result between platforms. One Rust stitcher, same result everywhere.

### Memory (5 × 24 MP, estimate)

- A decoded frame is `LinearImage`, interleaved f32 RGB: 24 MP × 12 B =
  **288 MB**. Five at once = 1.44 GB: never hold them together; decode one at a
  time for the features, and again one at a time for the warp.
- A 5-frame portrait-orientation sweep with ~30 % overlap is ~80–90 MP out.
  As f32 RGB that is ~1.0 GB, plus a weight plane, plus the frame being warped:
  **~1.7 GB peak**, which does not fit the iPhone's ~2 GB of headroom next to
  the app (see `matte.rs` ROOM).
- So: on iPhone, cap the output (≈40 MP, ~0.5 GB, and a `ROOM` check); iPad M
  and Mac a higher cap; Linux none. Accumulate in f16 if the cap hurts.
  Time on iPhone: decoding raw twice dominates (seconds per frame), so
  ~30–60 s for 5 raws (estimate).

### Effort

pano.rs ~1,200–1,800 lines incl. tests (a synthetic test: crop one image into
3 overlapping, rotated views and stitch back), Linux button + progress (~100),
FFI + Swift (~150), writing the result to disk (~100). **8–12 days.**

### Risks

- Parallax (near foreground, handheld without a nodal point) makes ghosting
  the DP seam cannot hide. Say so in the UI; no mesh warping in v1.
- Too little overlap / featureless sky: fail with a clear message, not a bad
  stitch (require ≥ N inliers per link, a connected graph).
- Raw frames with different white balance or exposure: stitch in the camera's
  native space as the bracket merge does, gains fix exposure.

---

## 3. Generative fill

### The candidates

| Model | Licence (weights) | Data | Size / needs | Apple paid app? | Notes |
|---|---|---|---|---|---|
| **LaMa (big-lama)** | Apache-2.0 | Places2 (research-only; low residual risk, already accepted) | 208 MB, on device today | **yes (already)** | no prompt; blurry on very large holes |
| MAT (Places 512) | **CC BY-NC-4.0**, "research purposes only" | Places | ~60 M | no | Linux only, and not better enough |
| MI-GAN (Picsart) | MIT | Places2/CelebA-HQ, **distilled from Co-Mod-GAN, NVIDIA Source Code Licence-NC** (issue #25 open) | ~30 MB, fast on phones | no (NC inheritance risk) | only if Picsart answers #25 |
| SD 1.5 inpainting | CreativeML OpenRAIL-M (commercial allowed, use restrictions must be passed on) | LAION-5B / laion-aesthetics (Stanford found CSAM in LAION-5B, Dec 2023; the runwayml repo is gone, now mirrored under `stable-diffusion-v1-5`) | ~1 GB 6-bit Core ML; 512 px; 20–40 s on an iPhone (estimate) | legally possible, reputationally poor | Apple's `ml-stable-diffusion` has no first-class inpainting (issue #148); via ControlNet-inpaint |
| SD 2 / SDXL inpainting 0.1 | CreativeML OpenRAIL++-M | LAION | 1024 px SDXL: ~7 GB fp16 | not on a phone | Linux via ComfyUI |
| SD 3.5 Medium/Large | Stability Community Licence: free under **US$1 M annual revenue**, Enterprise above; pass the licence on | undisclosed | 2.5–8 B | possible under the threshold, but a revenue-tied clause in a paid app | inpainting via community ControlNets |
| FLUX.1 Fill [dev] | **FLUX.1 [dev] Non-Commercial** (outputs usable commercially; the model not) | undisclosed | 12 B, ~24 GB (fp8 ~12 GB) | no | best fill quality; Linux via ComfyUI, user brings it |
| FLUX.1 schnell | Apache-2.0 | undisclosed | 12 B | licence yes | no Fill variant from BFL |
| **FLUX.2 [klein] 4B** | **Apache-2.0** (9B variant: non-commercial) | undisclosed | 4 B + a multi-GB text encoder; ~13 GB VRAM | licence yes; hardware: Mac 16 GB+, maybe iPad M 16 GB; not iPhone | editing by prompt/reference, **no native mask**: crop, edit, composite through the mask |
| Kandinsky 2.2 inpaint | Apache-2.0 (per HF card, not re-verified) | LAION-derived | ~2–3 GB | licence yes | dated quality |
| Apple Image Playground (iOS 27) | Apple's | — | system | — | photorealistic since WWDC26, `sourceImage` is only "inspiration"; **no mask/inpainting**; `ImageCreator` (non-UI) deprecated. Not a fill tool |
| Apple Photos Clean Up | — | — | — | — | no public API |

### Recommendation for v1.1

1. **All platforms: a better Remove, no prompt, no new licence.** LaMa stays.
   What makes it weak on big objects is Numa's side: the window is 5 radii
   resampled to 512 (`remove.rs`, `WINDOW_RADII`), so a large object is filled
   at low resolution. Do:
   - **Remove object by click**: SlimSAM (already there) gives the object,
     dilate ~1–2 % of the frame edge to take the outline and a shadow margin.
   - **Coarse-to-fine**: fill the whole hole at 512 with context, then for
     holes larger than the window, fill 512 tiles at full resolution with
     overlap, each seeing the coarse fill's neighbours as known context
     (outside-in order), blend the overlaps.
   - **Grain match**: add noise measured in the ring (the fill is already a
     ratio to the ring; add the ring's high-pass statistics) so the fill does
     not look smoother than the photo.
   - Effort **3–5 days**; same memory as today.
2. **Linux: a ComfyUI hook** (user runs ComfyUI; Numa ships no generative
   weights). Preferences: server URL (default `http://127.0.0.1:8188`) and a
   workflow file (Numa bundles templates for FLUX.1 Fill, SDXL inpaint and
   klein 4B; the user can point at their own with `{image}`, `{mask}`,
   `{prompt}` placeholders). Flow: `POST /upload/image` for the window and the
   mask, `POST /prompt` with the workflow, poll `/history/{id}` (or the
   websocket), fetch `/view`. The result is not deterministic, so store the
   filled pixels on disk per spot, the way a model-made mask is already kept
   (commit 2b1d025), not re-run like LaMa. Licence: ComfyUI is GPL-3, but it is
   a separate process spoken to over HTTP; the model's licence is the user's
   business, and Linux Numa is non-commercial anyway. Effort **4–6 days**.
   Risks: users without a GPU or without ComfyUI see nothing (hide it until a
   server answers); workflow JSON breaks when custom nodes change.
3. **Apple: no prompt-based generation in v1.1.** Reasons: the only on-device
   option that fits an iPhone is SD 1.5-class inpainting (LAION provenance,
   OpenRAIL use restrictions to pass into the EULA, 512 px fills that look soft
   on 24 MP photos, 20–40 s and ~1.5–2 GB on a 6 GB phone); prompting also
   makes Numa a generator of arbitrary content, with App Store review and
   age-rating consequences to work through first. Revisit for a later version
   with **FLUX.2 klein 4B** (Apache-2.0) on Mac and 16 GB iPads only, once a
   Core ML conversion of it exists and is measured. The paid app gets item 1.

### Risks

- Enthusiasts compare with Photoshop/Lightroom Generative Remove: the better
  Remove closes much of the gap for "a person/bin/sign on a plain background",
  not for "a car across a busy street". Say "Remove", not "Generative".
- Dataset provenance of every diffusion model is undisclosed or LAION; keep
  them out of the paid build until there is a model with documented clean data.

---

## Sources

Depth
- Depth Anything V2 (licences per size, data): https://github.com/DepthAnything/Depth-Anything-V2 · paper https://arxiv.org/abs/2406.09414 · dataset-licence question, unanswered: https://github.com/DepthAnything/Depth-Anything-V2/issues/320 · Base licence: https://github.com/DepthAnything/Depth-Anything-V2/issues/162
- ONNX: https://huggingface.co/onnx-community/depth-anything-v2-small · Apple Core ML + iPhone timings: https://huggingface.co/apple/coreml-depth-anything-v2-small
- Depth Anything 3 model zoo and licences: https://github.com/ByteDance-Seed/Depth-Anything-3 · https://huggingface.co/depth-anything/DA3-BASE · https://huggingface.co/depth-anything/DA3MONO-LARGE · paper https://arxiv.org/abs/2511.10647
- Depth Pro: https://github.com/apple/ml-depth-pro/blob/main/LICENSE · https://github.com/apple/ml-depth-pro/issues/66
- Marigold: https://huggingface.co/prs-eth/marigold-depth-v1-1 · https://huggingface.co/prs-eth/marigold-depth-hr-v1-1
- iPhone auto depth: https://www.macrumors.com/how-to/iphone-15-enable-portrait-mode-after-shooting/ · https://developer.apple.com/documentation/avfoundation/capturing-photos-with-depth

Panorama
- OpenCV stitching pipeline: https://docs.opencv.org/4.x/d9/dd8/samples_2cpp_2stitching_detailed_8cpp-example.html
- OpenCV iOS size: https://github.com/opencv/opencv/issues/13439 · https://github.com/miguelps/opencv-mobile
- rust-cv / akaze: https://github.com/rust-cv/cv · https://crates.io/crates/akaze · https://crates.io/crates/arrsac
- Vision registration: https://developer.apple.com/documentation/vision/vnhomographicimageregistrationrequest

Generative fill
- LaMa: https://github.com/advimman/lama · MAT: https://github.com/fenglinglwb/MAT · MI-GAN: https://github.com/Picsart-AI-Research/MI-GAN , https://github.com/Picsart-AI-Research/MI-GAN/issues/25
- SD 1.5 inpainting: https://huggingface.co/stable-diffusion-v1-5/stable-diffusion-inpainting · Apple Core ML SD: https://github.com/apple/ml-stable-diffusion , https://github.com/apple/ml-stable-diffusion/issues/148
- Stability Community Licence: https://stability.ai/news/license-update · https://huggingface.co/stabilityai/stable-diffusion-3.5-large/blob/main/LICENSE.md
- FLUX.1 [dev] licence: https://github.com/black-forest-labs/flux/blob/main/model_licenses/LICENSE-FLUX1-dev · FLUX.2 klein: https://huggingface.co/black-forest-labs/FLUX.2-klein-4B , https://bfl.ai/blog/flux2-klein-towards-interactive-visual-intelligence
- Image Playground (WWDC26): https://developer.apple.com/videos/play/wwdc2026/375/
