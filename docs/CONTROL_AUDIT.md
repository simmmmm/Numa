# Control audit — does every slider and option do what it says?

Asked by the photographer on 21 September: walk every control on every page
of the panel and prove it does what its name promises. Not a redesign —
nothing is moved, renamed or added to the panel by this audit. What it
leaves behind is tests, and a list of what is wrong.

## What "does what it says" means

Four questions, asked of every control:

1. **Wired** — moving it writes the field its name promises, and only that
   field; in a mask it writes the mask's field, not the photograph's.
   Double-click (reset) puts back the neutral value, and the render is then
   byte-identical to never having touched it. Stepping to another
   photograph and back shows that photograph's value, not the last one's.
2. **Direction** — it moves the picture the way its name says: +Exposure
   brighter, +Temperature warmer, −Saturation greyer, −Vignette darker
   corners and + lighter (Lightroom's sign, which the preset import relies
   on), and so on.
3. **Where** — it moves only what it names: Highlights not the shadows,
   the red band not the blues, Shadows grading not the highlights, the
   vignette not the centre, a mask's slider only inside the mask.
4. **How much** — the ends of its range are usable: no NaN, no whole frame
   clipped to black or white, no jump at the neutral point, and the middle
   of the range does something visible (a slider whose first 90 % does
   nothing is a finding, as MASK-002 was for brush size).

Neutral is `Default` for every field; FT-005 lists the neutral of every
control and is the starting inventory — but the panel has moved since, so
the inventory is rebuilt from the code that builds the pages.

## Two layers, two kinds of test

**Layer 1 — wiring, through the real panel.** A rig in the app, under an
environment variable like the others in `measure.rs` (`NUMA_AUDIT=1`), that
walks every registered slider, switch and dropdown of every page: sets a
non-neutral value, diffs the document's JSON before and after, puts it back,
and prints one line per control — tab, label, fields written, whether the
reset was exact. Then again inside a mask, for the four mask tabs. The
diffing is generic, so a control that writes nothing, writes two fields, or
writes the photograph's field in a mask shows up without a list of
expectations written by hand; the names are then read against the fields.
Runs under Xvfb on a **copy** of the catalogue, never the photographer's own
— and a copy of `~/.local/share/numa/catalog.db` is not one: edits, history
and ratings live in each library's own `.numa/catalog.db`, which the global
catalogue only points at. The copy has its library repointed at a folder of
symlinks to the photographs with a copied `.numa/catalog.db` in it.

**Layer 2 — effect, in the render.** Property tests in
`crates/numa-render/tests/audit_<group>.rs` on synthetic frames (ramps,
patches of known colour, a grey card, a checkerboard for detail), through
`apply_stack`. For each control: identity at neutral, direction, locality,
and the ends of the range, as above. One test per control, named for what
it proves.

A control found wrong gets its test written anyway, as it *should* behave,
marked `#[ignore = "AUDIT: <what is wrong>"]` so `dev/check.sh` stays green
and the finding is one `--ignored` away. Fixing is a separate step, after
the photographer has seen the list — some findings will be taste.

## Who does what

| Group | Pages | Layer |
|---|---|---|
| Light & Colour | Light (tone, curve), Colour (white balance, HSL/bands, profile, calibration) | 2 |
| Effects, Grade & Detail | Effects (presence, dehaze, vignette, grain), Grade, Detail (sharpen, noise, moiré, defringe, lens) | 2 |
| Wiring | every page, and the four in mask scope | 1 |

Masks' own shapes, Retouch and Crop come after, with what the first three
find about how the tests are best written; Presets is still being built (P3).

## What comes back

From each group: the tests, and a table — control, verdict (fine / wrong
direction / leaks / dead range / not wired / other), one line of evidence.
Collected into `fable-tickets/` for the photographer, fixed afterwards.
