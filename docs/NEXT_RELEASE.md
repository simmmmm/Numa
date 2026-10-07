# Next release: 0.36.0

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md), and after the
bump `data/THIRD_PARTY_LICENSES.txt` is generated again and `dev/check.sh` run.

0.35.0 went out on 3 October: GitHub (v0.35.0, Flatpak and AppImage), OBS and
TestFlight (mac).

**Ready to publish (7 October):** the version is 0.36.0 in `Cargo.toml` and
the metainfo — never released, so no bump for what joined it. The
photographer, 7 October: "ik wil alles op main van alle chats en nieuwe
publieke builds" — every chat's branch is on Linux `main` and Numa-mac `mac`
(only the `explore-*` experiments stay apart). Before `dev/publish.sh`: the
Swift of Send Feedback, LIB-025, Modern's tidy, RENDER-025 and the Mac's
tethering has not been compiled — the TestFlight build is its first compile.

## In

- ~~**Send Feedback… (START-015)**~~ — merged 4 October: Linux `main`, Numa-mac
  `mac`, the guide on numa-site `main`. Discussions is on for `simmmmm/Numa`
  (Ideas category); the issue form goes public with the publish itself.
- ~~**Stars to click on a hovered card (LIB-025)**~~ — merged 4 October:
  Linux `main` (tested in the README rig), Numa-mac `mac`, the guide on
  numa-site `main`. The Mac's Swift has not been compiled yet — the
  TestFlight build is its first compile, as for Send Feedback.

- ~~**Crash reports (START-013)**~~ — merged 4 October: Linux `main`, the
  guide on numa-site `main`. Linux only: on Apple the system collects crashes
  itself. Tested in the README rig (SIGSEGV, SIGABRT, a panic in a GTK
  callback, Copy, Open on GitHub, a clean exit).
- ~~**Every mask on the card (RENDER-020)**~~ — merged 4 October: Linux
  `main`, tried in the app by the photographer ("het voelde hartstikke
  goed"). Nothing on screen changes, so no guide chapter. The shared core
  reaches the Apple apps with the publish; their render on the card is off
  (RENDER-018), and the processor's output is unchanged.
- ~~**Mist (FILTER-010)**~~ — merged 4 October: Linux `main`, Numa-mac
  `mac`, the guide on numa-site `main`. Effects → Mist, one slider from Black
  Mist through neutral to White Mist; the lamps glow more than a wall by
  themselves. Tried in the app by the photographer ("heel vet"). Apple: the
  slider moves the photograph once the publish points its core at this
  `main`, and does nothing before that. No Swift changed, so nothing new
  to compile.

- ~~**Auto: level, light and crops (PLAN_AUTO P1–P3; ADJ-001, GEOM-005,
  GEOM-006)**~~ — merged 5 October: Linux `main`, the guide on numa-site
  `main`. The photographer asked to finish P1–P3 and, asleep, to go on and do
  his best; his own blind test (`numa-scratch/auto-blind/blind.html`) is
  still the gate and still open. A stand-in judge preferred it to the old
  Auto on 60 % of decided pairs, and untouched to it on 53 %; the crop gate
  is unmet (3 of his 10). Linux only: the Apple apps keep their Auto until
  the core is pointed at this `main`, and its card is not built there.
  Tried in the app (Auto rig): the card, a crop shown on a rest.
- ~~**Space at start (LIB-003)**~~ — merged 5 October: Linux `main`. A fresh
  start gave the keyboard to Add Folder, so Space opened the file chooser
  until a card was clicked; the grid has it from the start now — not on No
  Libraries Yet, where Add Folder is the way in. Tested in an isolated rig:
  Space, the arrows, Open With, the editor and back, a first start. The
  guide already says it, so no chapter changes.

- ~~**Everything from every chat (7 October)**~~ — merged into Linux `main`
  through `buren-samen`, `tether` and `import-lezen`, and the branches beside
  them; the Apple half into Numa-mac `mac`; the guide's branches into
  numa-site `main`. Linux unless said:
  - Import: two copies, both checked, a receipt, Safe to Format and Eject
    (IO-027…031); previews first, the raw later (IO-032); the camera's
    frames already in a library known without fetching them (APP-005); Only
    the Raws; what comes in analysed and read for words (LIB-028, CULL-001).
  - The camera on its cable (FLOW-013): its name in the header bar, Shoot
    Tethered…, a camera in its card mode told which setting to choose; the
    Mac's half through ImageCaptureCore.
  - Search by words and Things Numa Saw (LIB-028/029), on the quiet lane
    (PERF-072) and never on battery (PERF-024).
  - Moments (FLOW-018, Rapid until 6 October; the hold of 3 October lifted by
    the line above): a moment's layers (FLOW-019), Frame (FLOW-008),
    delivery per person (FLOW-014), the tape and Review (CULL-013/014), the
    choosing screen (CULL-015), the chosen moment on a fill.
  - As Shot (RENDER-024); a profile for your own camera (RENDER-025, every
    client); camera clocks (LIB-026/027); HEIF and HEIC (IO-015); Proof for
    Print (IO-034); For a Book (IO-035); Tonight (LIB-031); the client's picks
    (LIB-032); How It Was Made (DOC-008); Modern's tidy text on the iPad.

## Linux

- ~~**Face retouching in the export** (branch `claude/sad-newton-0be11e`,
  FACE-001)~~ — merged 7 October. A photograph exported from the grid, or given its edited
  thumbnail, came out without the skin, eye and teeth retouching: the faces
  are never stored, and only the editor looked for them.
  `render::with_masks_resolved` now finds them for a stored stack that has
  retouching waiting. Test: `FACES=<a portrait> cargo test -p numa-render
  --release a_stored_face`.
  - Release note: "Face retouching is in every export, including photographs
    exported from the grid without opening them."

## Guide

- The iPhone's mask screenshot (`shots/iphone/editor-mask-chosen`) still shows
  the old toolbar; the screenshot round after 0.35.0 replaces it.
