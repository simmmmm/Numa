# Next release: 0.35.0

**Ready to publish (3 October):** the version is 0.35.0 in `Cargo.toml` and
the metainfo; Linux main holds UI cohesion and the face-retouching fix, and
Apple `mac` holds UI cohesion with the iPhone's mask mode (merged and pushed
by the photographer). Rapid is on hold and not in it. Before `dev/publish.sh`,
on the Mac: `BehaviourTests.test3_maskOutlastsATab` on an iPhone simulator.

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md).

## Apple

- **UI cohesion** (Numa-mac `ui-cohesion`, 2 October). Contains `ux-study`
  and `iphone-mask-mode` below, so merging it into `mac` merges all three.
  Compiled for iOS and macOS by build.yml (run 37029274157, BUILD SUCCEEDED);
  the merge into `mac` waits for the photographer. One Export and one Before
  (held on every touch screen) in every editor, the iPhone's top bar in the
  corners with Before, Undo and Redo at the photograph's foot, Looks as
  Modern's first tab, a tile's marks on the tile, one stack and the mark on
  every Libraries page, the iPhone's filter chips on the iPad and the Mac,
  Grade's All first, Original in every crop, no spring that overshoots and
  Reduce Motion honoured, and `dev/style.py` in `dev/check-apple.sh`.
  - Behaviour changes to say in the notes: the iPhone's Stars chip is now
    ★ 3+; Before on the iPad is held, not tapped; an export failure is white,
    not red.
  - Release note: "Numa looks and moves the same on the iPhone, the iPad and
    the Mac: one white button where you finish, Before held wherever you
    touch, and on the iPhone your thumb reaches Before, Undo and Redo."

- **The iPhone's mask mode** (Numa-mac `iphone-mask-mode`, FEEDBACK #75).
  The photographer, 0.33.0 on the iPhone: "maskers verdwijnen als je er uit
  klikt. omdat we de tabs onderaan niet wisselen zoals de andere varianten."
  A mask chosen brings MASK-022's bar at the top and turns the tabs along
  the foot into the mask's until its Done, as on the iPad, the Mac and Linux.
  - Branched from `ux-study`: merge `ux-study` into `mac` first, or both at
    once.
  - Compiled as part of `ui-cohesion` (above); still to run:
    `BehaviourTests.test3_maskOutlastsATab` on an iPhone simulator.
  - Release note: "On the iPhone, editing a mask works as everywhere else:
    its bar at the top, the tabs along the bottom the mask's own until
    Done."

## Linux

- **UI cohesion** (UX-031..UX-042) is on `main` and pushed (30c4eb0): one
  white primary per screen, white for what is chosen inside Numa's content,
  chips, one ground on the photograph, the grid's marks on the filmstrip,
  stacks and the mark, the quick filter chips, the value HUD for keys and
  scroll, motion, and the `dev/style.py` ratchet. guide.numa.photo carries
  it (numa-site 185830c, pushed).
  - Release note: "Numa on Linux follows one design: a single white button
    to finish, white for whatever you chose, chips for every small choice,
    and calmer overlays on the photograph."

- **Rapid** (FLOW-018, branch `workflows`, worktree Numa-workflows) is ON
  HOLD and not in 0.35.0: the photographer, 3 October, "rapid nog even on
  hold houden tot we het echt goed voor elkaar hebben, de rest mag wel
  door". Linux only, Apple not started; `dev/check.sh` and `dev/style.py`
  green on the branch. A second view of
  a library beside the grid: moments with a kind of shoot's four or five
  sliders, Numa's work done ahead with a switch per part (light, Even
  Exposure, straighten, verticals, frame) and Undo Numa's Work, notes and a
  learned style, stacks of the same picture with Numa's few kept and a stack
  seen whole, chapters with Numa's offers, First Look and the teaser, the
  check through the picks and Deliver by chapter and by person, by likeness
  for products and property, its own size; the editor's filmstrip groups
  bursts. Not tried in the rig: Deliver by person (no faces in the CC0
  wedding).
  - With it: numa-site `rapid` (11d965a, not pushed) — the guide's Rapid
    chapter, chapters 05–20 renumbered.
  - Release note: "Rapid: look at a shoot by its moments, with only the few
    sliders your kind of shoot needs, Numa's work already done and a switch
    for every part of it, the bursts in stacks, the day in chapters, and a
    last look through your picks before it goes out."

- **Face retouching in the export** (FACE-001, from branch
  `claude/sad-newton-0be11e`, on main). A photograph exported from the grid,
  or given its edited thumbnail, came out without the skin, eye and teeth
  retouching: the faces are never stored, and only the editor looked for
  them. `render::with_masks_resolved` now finds them for a stored stack that
  has retouching waiting. Test: `FACES=<a portrait> cargo test -p numa-render
  --release a_stored_face`.
  - Release note: "Face retouching is in every export, including photographs
    exported from the grid without opening them."

## Guide

- **numa-site `iphone-mask-mode`**: the iPhone's paragraph on the Masks
  page. Push with the release. The screenshot `shots/iphone/editor-mask-chosen`
  still shows the old toolbar; the screenshot round after the build replaces
  it.
