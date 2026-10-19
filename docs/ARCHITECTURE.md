# Architecture

Numa is a Cargo workspace. The desktop app is the root package; the work is
done in the crates under `crates/`.

| Crate | What it does |
|---|---|
| `numa-core` | Types shared by everything: the edit document, colour and tone maths, masks, lens geometry, image buffers. |
| `numa-io` | Reading and writing: RAW decoding, demosaicking, lens corrections, the catalogs, import, export, XMP, ICC and DCP profiles. |
| `numa-render` | Turns a document and a decoded image into pixels: the editing pipeline, masks, retouching, AI tools. |
| `numa-gpu` | The decode steps on the graphics card, through wgpu compute shaders. |
| `numa-cull` | Culling aids: sharpness, clipping, faces, closed eyes, bursts, scores learned from your ratings. |
| `numa-infer` | Runs the ONNX models through ONNX Runtime. |
| `numa-server` | Numa Home: a server for a NAS with a browser interface and an API for the apps. |
| root (`src/`) | The GTK 4 / libadwaita desktop app. |

`vendor/rawler` is the RAW decoder, vendored with a few changes listed in
`vendor/rawler/NUMA-CHANGES.md`.

## Opening a RAW file

`crates/numa-io/src/raw.rs` turns a file into a `LinearImage`: scene-linear
f32 RGB in the camera's own colour space, with the camera profile attached.

1. **Decode.** rawler reads the mosaic and the metadata.
2. **Scale.** Black level is subtracted and the values divided by the white
   level, giving 0..1.
3. **Demosaic.**
   - Bayer: RCD (`raw/rcd.rs`). The mosaic is multiplied by the as-shot white
     balance first and divided again afterwards, so the rest of the pipeline
     still sees camera RGB. The draft decode uses PPG.
   - Fujifilm X-Trans: Markesteijn, followed by a false-colour suppression
     step.
   - Anything else (four-colour sensors, linear DNG, monochrome sensors):
     rawler's own develop.
4. **Exposure.** A baseline lift, then the frame is scaled so that its median
   lands where the camera's embedded JPEG puts its median. This sets the
   starting exposure only.
5. **Lens.** Vignetting, distortion and lateral chromatic aberration are
   corrected from a table. For Fujifilm the table is the one the camera writes
   into the RAF; for other cameras it is sampled from the LensFun database,
   matched by camera and lens. The correction code is the same for both
   sources (`numa-core/src/lens.rs`, `numa-io/src/lensfun.rs`).
6. **Orientation** from the EXIF tag.

The editor does not wait for all of this. Its proxy is averaged straight from
the mosaic at the size of the screen (`raw/proxy.rs`). For 1:1 the frame is
demosaicked once and only the part on screen is developed further
(`raw/region.rs`); export develops the whole frame.

When a graphics card is available, `numa-gpu` performs steps 2–5 and the
downscale on the card (`crates/numa-gpu/src/*.wgsl`). The processor path is
the reference; the two are tested to agree within rounding, and the processor
takes over when there is no card or the frame does not fit in its memory.
`NUMA_GPU=0` disables the card.

## Rendering

An edit is a `Document` (`numa-core/src/document.rs`): an ordered set of
settings, stored as JSON in the library's catalog and versioned by
`document::SCHEMA`. `numa_render::develop` applies a document to a
`LinearImage`. Preview and export call the same function; only the input
differs (a proxy or the full frame).

```
LinearImage (scene-linear, camera RGB)
  ├─ colour       white balance in camera space, the camera matrix or a DCP profile
  ├─ geometry     quarter turns, crop, straighten, perspective
  └─ pixels       heal and clone, face retouching
                  noise reduction, sharpening
                  exposure, contrast, tone regions, vibrance, saturation
                  colour mixer
                  local tone mapping (HDR, clarity, texture)
                  masks: the same stack, blended in per mask
                  colour grading
                  tone curve (the camera's look or AgX), your curves
                  output colour space, 8 or 16 bits
```

Values stay scene-linear until the tone curve. While a slider moves, the
editor renders a draft and replaces it with the full render when the slider
stops.

## Library and catalog

Each library is a folder with a hidden `.numa/` inside it holding a SQLite
catalog (`numa-io/src/catalog.rs`, `catalog/schema.rs`): photographs, ratings,
flags, edit documents, history, analysis and the names given to faces, plus
the grid's thumbnails. Paths are stored relative to the folder, so a library
can be moved or opened on another computer. A small application catalog in
`~/.local/share/numa/` lists the libraries and holds the settings and albums.
A library whose drive is not connected can still be browsed from a copy of its
list.

## Machine learning

Every model is an ONNX file run by `numa-infer`. Models are optional and
downloaded on first use; each is checked against its SHA-256. The list, with
authors and licences, is in `data/MODELS-LICENSES.txt`.

## The desktop app

`src/ui/` is the interface: `window.rs` builds the main window and
`src/ui/window/` holds one module per area (library grid, loupe, editor
panels, masks, export, preferences). Colours and sizes come from
`src/ui/tokens.css`, generated from `data/design/tokens.json` by
`dev/tokens.py`.

## Numa Home

`crates/numa-server` is one binary (axum and tokio): the HTTP API used by the
apps, the browser interface (`src/web/`) and background jobs such as
thumbnails and face detection. It uses the same catalog and renderer as the
desktop app. `packaging/docker/` builds its image.

## Tests and checks

- `cargo test --workspace --release` runs the unit and integration tests.
  Tests that need real camera files read raw.pixls.us's CC0 samples from
  `NUMA_CC0_RAWS` (default: `raws-cc0` beside the repository) and skip when
  they are not there.
- `dev/check.sh` runs the tests and the repository's checks: no file longer
  than 2000 lines, no function longer than 150, no struct with more than 30
  fields (`dev/shape.py`, against `dev/shape-baseline.txt`), the colour and
  size rules of the interface (`dev/style.py`), and the third-party licence
  lists in `data/`.
- CI runs `dev/check.sh` on Ubuntu 24.04, the oldest platform Numa builds on.
