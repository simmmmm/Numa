# Splitting window.rs

*Written 21 September 2026, at the photographer's request: "a faster plan for
splitting the big files, so more agents can work at once and contributors can
find their way."*

**Phase A is done** (21 September, branch `split-window`): 38 moves and the
test modules, one commit each, every one of them passed by `dev/moved.py`, the
compiler and `dev/check.sh`. `window.rs` is 984 lines. The screenshot rig gives
the same page measurements as before, and the same pixels except for the
marching ants and the filmstrip, which move by themselves. The scripts
learned four things on the way that the first row did not show: tuple
struct fields, `pub` items reached through `ui::window::`, `thread_local!`
statics, and `include_str!` paths.

**B2 is done too** (the same day): five agents, one area each in a worktree
of its own, split all 21 functions over 150 lines. Their 21 commits applied to
main without a conflict. The whole passes `dev/check.sh`. The develop pipeline
and skin smoothing give the same output hashes before and after, and the
screenshot rig gives the same pixels and measurements on ten pages. The
ratchet's baseline is down from 24 debts to 3: `App` (116 fields), `Basic` and
`Sliders` (39 each). A first "before" set of screenshots, taken while the five
agents were compiling, measured pages the editor had not finished building;
take rig screenshots on a quiet machine.

**B1 is done as well**: `dev/regroup.py` moved thirteen groups of `App`'s
fields into a `State` in the module that uses them, one commit each, and the
compiler checked every rewritten `state.<area>.<field>`. `App` went from 116
fields to 30. Each `State` has a `new()` of its own, which also keeps
`build_window` under 150 lines. The baseline is down to two structs of 39
fields: `Basic` in numa-core, which is the document format and wants its own
plan, and `Sliders`.

**And the last two, the same day.** `Basic` (39 fields, the document format)
is seven groups now — tone, presence, detail, effects, calibration, optics,
balance — each `#[serde(flatten)]` and in the order the fields were declared
in, so what is stored is the same flat object with its keys in the same
order. Proof: the `basic_json` test, against three strings the flat struct
wrote before it was split; and every one of the 122 edits stored in the
photographer's libraries, read and written back by the old code and by the
new, byte for byte the same. The 450-odd uses were rewritten where the
compiler pointed, not by pattern, since `exposure` is a field of other
structs too. `Basic::with(|b| b.tone.exposure = 1.0)` replaces the fifty
literals that named a slider or two. `Sliders` mirrors the seven groups, so
`read` and `write` go group to group; every pairing was checked by name.
**The baseline has no debts left.**

The rig can mislead in one more way: it reads the photographer's own catalog,
so a photograph he edits while it runs looks like a change in the code.
Compare before and after in pairs, one straight after the other.

## Where it stood

One file is the problem. `src/ui/window.rs` is **14 552 lines**, 10 138 of them
code, against a limit of 2 000. Every other file in the tree is under it. The
same file holds `App` (116 fields against a limit of 30) and 11 of the 21
functions over 150 lines.

The scout rule (`AGENT_GUIDE.md`) was the only way it shrank: move one piece
out when you touch it, never open a branch just to make things smaller. In a
month that took it from 10 197 to 10 138 code lines. At that rate it is under
the limit in about ten years.

Every change of any size touches this file. Two agents working at once
conflict in it, and a contributor looking for the mask canvas has to find it
between the People dialog and the AppImage menu entry.

## The idea

**Moving code is mechanical, so a script does it. Changing code is not, and
that is where agents help.**

- **Phase A: move, don't change.** One session, done one move after another,
  by script. Every function goes to a file named for what it is about, byte
  for byte, with `pub(super)` added and nothing else. The compiler and a
  pure-move check prove it. Done in parallel, these moves would all cut from
  the same file and conflict at every seam. By script the whole phase is an
  afternoon.
- **Phase B: refactor, in parallel.** Once every long function and every area
  of `App` lives in its own file, agents can take one each without meeting.

## Phase A: move, don't change

### A0: two tools

**`dev/split.py <module> <first> [<last>]`** cuts the top-level items from
`<first>` through `<last>` out of `window.rs` and puts them in
`src/ui/window/<module>.rs`. Items are found by name, so the command still
works after earlier moves have shifted the lines. The range starts at the
first item called `<first>` and ends at the last item called `<last>`, so a
struct and its `impl` blocks move together. It stops if either name is not a
top-level item. It works like this:
- It takes each item's doc comment and attributes along with it.
- It creates the file with `use super::*;`.
- It adds `pub(super)` to every moved item, struct field and impl method that
  has no visibility yet. A struct that moves to a child module would otherwise
  hide its fields from the parent.
- It adds `mod <module>; use <module>::*;` next to the existing pairs.

That is the pattern every module under `window/` already follows. A child
module can see everything in `window.rs`, including `App`'s private fields,
so no call site changes.

**`dev/moved.py [<rev>]`** proves that a commit only moves code:
- every line it removes is added somewhere;
- every line it adds was removed somewhere.

It ignores only four things: a `pub(super) ` prefix, `use super::*;`, the
`mod`/`use` pair, and blank lines. It prints any line that breaks this. This
is the P0b proof ("53 of 54 files differ only in a path"), turned into a
command.

### A1: the moves

For each row: run `split.py`, then `cargo build` (dev profile, under a
second). Only two kinds of fix are allowed:
1. a visibility the script missed;
2. a name that two glob imports now both provide, fixed by writing its path.

Then run `moved.py` and commit, one commit per row. Run `dev/check.sh` and the
screenshot rig once at the end.

| # | module | first item … last item | lines |
|---|---|---|---|
| 1 | `busy` | `BUSY_AFTER` … `timed` | 150 |
| 2 | `header` | `build_header` … `debug_info` | 300 |
| 3 | `libraries_dialog` | `libraries_dialog` | 200 |
| 4 | `people` | `people_dialog` … `fetch_portraits` | 360 |
| 5 | `desktop` | `appimage` … `integrate_appimage` | 170 |
| 6 | `preferences` | `preferences_dialog` … `shortcuts_dialog` | 235 |
| 7 | `add_library` | `add_library_dialog` … `open_path` | 145 |
| 8 | `updates` | `UPDATE_CHECK` … `key_hint` | 95 |
| 9 | `libraries` | `describe_new_library` … `copy_into_library` | 280 |
| 10 | `library_page` | `build_library_page` … `confirm_delete` | 505 |
| 11 | `mask_overlay` | `build_mask_overlay` … `draw_mask` | 895 |
| 12 | `mask_list` | `refresh_masks` … `add_mask_reordering` | 855 |
| 13 | `mask_parts` | `refresh_mask_parts` … `set_mask_visible` | 225 |
| 14 | `pipettes` | `arm_band_pipette` … `rounded` | 190 |
| 15 | `mask_edits` | `pick_range` … `tint_mixer` | 485 |
| 16 | `retouch_overlay` | `build_retouch_overlay` … `toggle_retouch` | 505 |
| 17 | `panel_values` | `tint_grading_hue` … `pick_point` | 245 |
| 18 | `copy_paste` | `fill_presets_menu` … `reload_open_document` | 365 |
| 19 | `filmstrip` | `build_filmstrip` … `mark_filmstrip` | 170 |
| 20 | `culling` | `cull_note` … `regroup_bursts` | 340 |
| 21 | `picker` | `shelve_libraries` … `selected_ids` | 200 |
| 22 | `albums` | `fill_albums_menu` … `albums_dialog` | 195 |
| 23 | `grid` | `reload_grid` | 105 |
| 24 | `merge` | `merge_selection` … `open_merged` | 150 |
| 25 | `loupe` | `LOUPE_EDGE` … `refresh_loupe_bar` | 335 |
| 26 | `badges` | `build_card` … `strip_badge_text` | 145 |
| 27 | `rating` | `rate_open_photo` … `SeenFace` | 365 |
| 28 | `history` | `EditState` … `masks_changed` | 710 |
| 29 | `panel_sliders` | `Sliders` … `Readout` | 355 |
| 30 | `editor_page` | `build_editor_page` … `build_editor_bar` | 615 |
| 31 | `export_ui` | `ExportJob` … `export_dialog` | 350 |
| 32 | `panel` | `build_adjustment_panel` … `narrow_dropdown` | 390 |
| 33 | `render_loop` | `adjustments_changed` … `show` | 495 |
| 34 | `zooming` | `set_zoom` … `write_zoom_label` | 280 |
| 35 | `geometry` | `leave_crop` … `commit_crop` | 390 |
| 36 | `overlays` | `build_face_names_overlay` … `show_baseline` | 620 |
| 37 | `info` | `build_histogram` … `refresh_info` | 360 |
| 38 | `open` | `begin_open` … `close_editor` | 420 |
| 39 | test modules | `tests`, `shelves`, `icon_proof`, `crop_aspect`, `brush_scale`, `mask_names`, each to its own file as `#[cfg(test)] mod x;` | 480 |

Line counts are from 21 September and include comments. The ranges follow the
order the file already has, which groups things well. The few that sit in an
odd place, such as `auto_tone` in `geometry` and `key_hint` in `updates`, move
with their neighbours. Putting them right is a one-line follow-up that
`moved.py` can also verify.

Some names are chosen to avoid existing modules: `zooming` because `zoom`
exists, `export_ui` because of `exporting`, `badges` because of `cards`,
`geometry` because of `crop`, and `mask_*` because of `masks`. Merging each
pair is a later, optional pure move.

### A2: what stays in window.rs

The imports and the list of modules, the constants, `App` and `impl App`,
`load_css`, `build_window`, `render_inputs`, `OpenPhoto` and `Source`. That is
about 900 lines. The file debt disappears, and the baseline is rewritten
smaller in the last commit.

### A3: where things live

`AGENT_GUIDE.md` gets a table with one line per module: what it is about, and
which feature labels (MASK-, LIB-, CULL-, …) it carries. That table is the map
for a contributor and the list of tasks for agents.

## Phase B: refactor, in parallel

Each task happens in its own worktree, one agent per row. The proof is
`dev/check.sh` plus the screenshot rig: the same pages before and after, and
the same measurements.

**B1: `App`, 116 fields.** Split it into one `State` per area, the way P0b did
for `masks::State`. Candidate areas:
- library and grid;
- loupe;
- canvas and render;
- panel and sliders;
- export;
- people.

For each area, its fields move into `<area>::State`, and `state.f` becomes
`state.<area>.f` (one `sed` per field; the compiler finds any that are
missed). After Phase A those accesses sit almost entirely in that area's own
files. The only place where two tasks meet is the definition of `App` and the
literal in `build_window`, so the tasks are merged one after another, each
rebasing onto the last. Each conflict is a few lines.

**B2: the long functions.** There are 15 over 150 lines. After Phase A each
sits in a file of its own, so one agent per file never meets another:
- `build_mask_overlay` (301), `fill_people` (246), `build_window` (245),
  `install_photo_menu` (238), `build_editor_page` (217);
- `render_current` (198), `export_dialog` (195), `libraries_dialog` (194),
  `analyse_library` (193), `build_editor_bar` (193);
- `build_header` (191), `refresh_mask_parts` (172), `install_rating_shortcuts`
  (171), `open_photo` (168), `refresh_masks` (166).

Also outside window.rs: `build_filter_bar` (283), `smooth_skin` (222),
`build_crop_controls` (205), `apply_pixels` (178), `from_lightroom` (159),
`slider_row` (156). Break out named helpers and change no behaviour.

## Rules while this plan runs

- **Branches that only move code are allowed** for the rows in this plan, as
  an exception to the scout rule. Everything else in `AGENT_GUIDE.md` still
  applies.
- **A move commit changes nothing else.** If `moved.py` complains, the fix goes
  in a separate commit.
- **Phase A runs start to finish without other work in `window.rs`.** Anything
  that has to happen meanwhile waits, or goes after the last move.

## Cost

- **A0:** the two scripts, with a test that fails if `moved.py` accepts a
  change that is not a move. About an hour.
- **A1 and A2:** 39 moves at a few minutes each, most of it the compiler.
  Half a day, one session.
- **B1:** one to two hours per area; six areas, in sequence or two or three
  at a time.
- **B2:** an hour or two per function, all at once.
