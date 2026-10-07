# Plan: Auto — crop, light and level that read the photograph

30 September 2026, corrected 1 October 2026. Research and reading only:
nothing was downloaded or run and no photograph was measured for this memo.
The code was read at `b71a447` (Numa 0.33.0), the public export of
`eb67920` with the comments taken out, and every function, constant and line
number below was checked against it and refers to that export, in the code and
in `docs/FEATURES.md` alike — not to this repository, where the comments make
the lines fall elsewhere; names marked *new* do not
exist yet. P0 has since been built (Status), so §1 describes the code before
it. Every speed, memory and energy figure is either measured in
`docs/ENGINEERING.md` or `docs/FEATURES.md` (the line is named), published (the
source is named), computed from the code's own tone arithmetic in a scratch
script rather than on photographs (marked *computed*), or an estimate (it says
so). The research notes behind it could read GitHub, raw GitHub and Apple's
developer pages in full; most publisher sites, arXiv, Springer and the patent
mirrors were blocked, so a claim that rests on a search-result extract is
marked *unverified* where it is made.

The design was argued out as three proposals (one built on craft, one on
surprise, one on systems and evidence) and judged twice, once with a
photographer's eye and once with an engineer's. Both judges chose the craft
proposal; this plan is that proposal with the others' best parts grafted on
and everything both judges cut left out. It was then checked by three
reviewers: one against the code, one against the outside sources, and one
looking for the ways it would damage a photograph. Their corrections are in
this version. The larger ones changed the design and not only the wording:
Auto no longer overwrites what the photographer set (§1 bug 7, §3.8), the
chosen subject is stored rather than asked for again (§3.4), backlight is read
on its own (§3.5), levelling is a crop and passes the crop's gates (§4.3),
batch Auto lays no subject masks (§6.5), and counsel is a gate for the Apple
build (§8).

This is not legal advice. It follows the rule of
`docs/PLAN_DEPTH_PANO_GENFILL.md`: the paid Apple build needs weights **and**
training data that allow commercial use; the free GPL-3 Linux build may offer
non-commercial models only as optional downloads, flagged where they are
offered. Every model the Apple Auto would rest on has training data that fails
or may fail that rule (§8), so **counsel is a gate before P1 for the Apple
build**, and a fallback on Apple's own Vision requests is written down now in
case the answer is no.

| Part | Recommendation | Apple | Linux | Effort |
|---|---|---|---|---|
| Corpus | Auto-60 and four refusal strata of 30 from the photographer's library, consented portraits, twin documents, the roll test frames | yes | yes | 5–8 days, its own task |
| P0 fixes | **Built 1 October** (see "Status" below). Measure on the frame that is shown (crop, angle, quarter turn, mirror, keystone); write a global slider, HDR included, only where the photographer has not; solve through the document's look; masks start from `Basic::local()`; stop deleting the photographer's Subject mask; Blacks still inside the camera's band; levelling says so and runs on the worker | yes (same Rust) | yes | done |
| Scene read | One read per photograph in the uncropped frame, cached on open: classes, faces, the camera's fields, edges, two luma planes, and the level evidence. The matte runs on the press, never on every open | yes, after counsel (§8) | yes | in P1 |
| Level | Sea horizon from the segmentation; roll and keystone from one vanishing-point fit on buildings; the camera's own level as a bound once test frames have verified it; slopes and Dutch angles refused; applied only when the corners it costs pass the crop's gates | yes, plus `VNDetectHorizonRequest` as a second opinion (outside the parity run) | yes | P1, about 20 days with the read and a minimal reveal |
| Light | Keep the measured ends rule, with a ramp, solved against the document's own look; read the photographer's intent from the file; rescue for a subject in shadow and for a backlit one, neither tied to skin tone; emphasis from Radials, "Around" off until it wins blind; sky held; a verify loop that drops what it cannot defend, halos and a flattened range included | yes | yes | P2, about 20 days |
| Crop | Offered, never applied: three proposals with reasons, "as shot" must be beaten by 0.08, face, joint and whole-animal gates, a floor in megapixels rather than in area. The birder's portrait card ships early, in P2, behind the same gates | yes, plus the aesthetics request as a tie-breaker | yes | P3, about 14 days |
| The wow | The sharpest frame of the burst on the subject; the heron cut as a portrait; a plan card that says no in plain words; the printer's marks only once a blind test shows people keep what Auto found | yes (SwiftUI and a touch design, 2–3 weeks) | yes | across P1–P4 |
| Depth | Depth Anything V2 Small (already planned) as a check on the subject and for fall-off by distance | yes | yes | P4, 4 days after `PLAN_DEPTH_PANO_GENFILL.md` §1 |
| Like you | The photographer's own targets learned from their edits, with abstention and a reset | yes (Vision FeaturePrint) | yes | P5, about 10 days |
| Corner fill | Not part of Auto. Later at most an opt-in, labelled action of its own | later | later | not planned |

---

## Status

**P0 built, 1 October 2026.** What it does, and where it went its own way:

- **Measured on the frame that is shown.** `auto::framed` passes the working
  image through `mask_geometry`, the geometry the subject's mask frame is made
  with. A test finds a dark subject through a crop, a straighten, a quarter
  turn, a mirror and a keystone (the old path misread four of the five), and a
  lamp cropped away no longer pulls the exposure down.
- **What the photographer set, Auto leaves.** `Document.auto` (`AutoRecord`)
  keeps what Auto last wrote; a slider is Auto's only at rest or at that value.
  HDR is under the same rule rather than taken away (bug 2, corrected above).
  The toast names what was left, HDR apart.
- **The endpoints through the look, the exposure not.** Whites, Blacks and
  Highlights are solved through the document's contrast, tone sliders and
  composite curve. The exposure was at first as well, and the review found
  what that does: a Highlights −100 recovering a sky read as a dim frame and
  took +¾ EV, a Contrast +45 read as blown and took −¾. The exposure is decided
  on the camera's rendering again, as ADJ-001 measured it, and an end the
  photographer has shaped (their Highlights, Whites or Blacks, or a curve that
  rolls off the white or fades the black) is left to them. The subject lift is
  not solved through the look yet (§3.5 is P2), and neither are HDR and
  Clarity, which are local.
- **Masks.** Built as `Mask.auto: Option<Basic>`, the adjustment Auto gave
  the mask, with `Mask::as_auto_left_it` true only while the whole mask is
  what `Mask::auto_subject` made: any edit, a new name or a softer edge
  included, or another mask subtracting it, makes it the photographer's. This
  needs no hook at each of the forty places a mask is edited. Auto replaces
  its own mask where it stands, with its id; never removes the photographer's;
  lifts nothing on top of a subject mask that is theirs, and leaves the HDR
  under it. The lift the old Auto left is recognised (`Mask::from_old_auto`)
  and replaced on the next press. Nothing is selected afterwards; a selected
  mask stays selected by its id.
- **History.** `EditState` carries the record, so undo, redo and a history
  jump keep it in step with the sliders. Which sliders are Auto's is decided
  at the press and checked again when the answer lands.
- **Blacks.** Re-computed through the render's tone curve, as P0 asked: a dark
  end at 0.06–0.09 solved to Blacks −21 to −53. Rather than wait for P2, Blacks
  now moves only outside 0.035–0.09, from Auto's black point to the top of the
  0.06–0.09 band ADJ-001 measured. That is a computation, not a measurement on
  photographs, and the corpus can overturn it.
- **Level and Perspective** run on the worker with a spinner in their buttons,
  levelling says what it did, and an answer is dropped if the crop changed or
  the Crop tool was left while it measured. Auto straighten has an entry now,
  GEOM-005.
- **The harness** is `what_auto_now_does` with `FRAMES_LIST` and `OUT_CSV`
  rather than a new `auto_scorecard`; it writes the slider answers and the
  percentiles, not yet the reasons or a contact sheet.
- **Not done here:** the baseline CSV on real frames. This container could not
  reach raw.pixls.us, Hugging Face or the model release, so there were no raws
  and no models to measure with. The harness is ready
  (`FRAMES_LIST=$PWD/frames.txt OUT_CSV=$PWD/auto.csv cargo test -p
  numa-render --release what_auto_now_does -- --ignored --nocapture`, with
  `CROP`, `TURN`, `MIRROR` and `KEYSTONE` to frame the document first; FEATURES
  ADJ-001 has the details) and takes the application's path.

**P1 built in part, 1 October 2026** (branch `auto-p1`). What it does, and
where it went its own way:

- **The scene read** (`auto::scene`), in the frame as shot, kept in
  `auto_read` beside `found` and sharing its grid blob. **Read on the press,
  not on open or on a dwell:** `write_rest_of_panel` took the models off the
  open path so stepping through a shoot runs nothing, and a read is 0.6 s on
  the processor, so the first press pays it and every later one reads a row.
  Its key carries the models only where they are installed, so a read made
  without the segmenter is made again once it is there. The planes, the
  intrusions and the subject are not in it yet (P2, P3); the live
  segmentation is not kept beside it.
- **Level** (§4.2–4.3): E2 as specified; E3 as the vanishing-point fit;
  **E4 changed twice.** A histogram's peak of the central half read three
  posts on one side of a jetty, leaning only by the camera's pitch, as a 4.2°
  roll on a level lake, so E4 is the same vanishing-point fit as E3, over 80 %
  of the width, with the peak test on each edge's lean taken back to the
  centre and at least three structures placed where their lines cross the
  middle row. Even so it agreed on tilts that were not there on hills and a
  stream, so **E4 alone is offered, never applied** — a stricter reading of
  "E4 σ 0.6, refuse above 0.5" than refusing it outright, which would have
  made "by the verticals" unreachable. E1 is read (`raw::roll_angle`, and DR
  mode and exposure program beside it) and unused: an X-T5 portrait frame
  reads 88.6–92.9°, so the turn is in the value.
- **The corners** (§4.3 step 7, §4.5): `auto::corners`, with `into_crop`
  (`source_map` backwards) and `levelled` in numa-core; the re-centring is a
  search over smaller rectangles inside the largest, within the 5 %. Joints
  and the whole-animal gate wait for P3; a person or animal is the
  segmentation's grid dilated by a cell.
- **Upright** is offered on the card, never applied by the press, and only
  where the buildings' lean is sure to 0.5°: distant villages read −6 to −12.
- **The press**: the Light page's button and the A key; "Auto · level" and
  "Auto · light" as named steps; `AutoRecord` gains `angle` and `vertical`.
  The card is at the top **left** (the histogram has the top right), a row per
  part with Level / Square Up where offered; the sea drawn with its angle;
  Shift+Space before Auto. An offer taken re-measures the light.
- **Measured** (`the_level_on_real_frames`, 91 of the photographer's frames,
  before the last E4 change): sloping land levelled 0 of 16, the sea within
  0.014° median and 0.13° p90 on turned frames, the buildings 0.08° and 0.21°.
  Sloping land is 16 frames, not the 30 of §9.1; the Andes and vineyard frames
  were not run.
- **The burst's sharpest frame** (§6.5 #1) is a row with **Open**, from
  CULL-002's best of the burst: the faces' sharpness where there are faces,
  the frame's elsewhere — not yet on the subject's box. Nothing without
  Analyse having run.
- **The subject's outline** is drawn while the card is up (`draw_ants`, still,
  not traced in), once Auto's mask has its pixels.
- **Compare** holds the photograph before Auto in the reference pane
  (`reference::hold_frame`, split out of `set_reference`).
- **A switch on an applied level** turns it off and on, each a named step,
  and the light is measured again; offers are buttons. Other parts do not
  switch yet (Ctrl+Z peels the light).
- **Measured again on 2 October**, a13e7ca, 204 of the photographer's frames
  (172 from Japan 2026, 32 from Chile and Argentina, Italy and Maastricht),
  each judged from its preview: sloping land and deliberate tilts levelled 0
  of 11 and 0 of all turned reads; levels and offers on architecture right;
  buildings on turned reads within 0.10° median, 0.42° p90 (0.07° and 0.21°
  where the untouched frame was answered); 638 ms a read. **No sea reading on
  any of the 24 sea, lake and river frames**, rightly by the judge's eye —
  bays, far shores and ponds, no open horizon — except one hazy Sorrento
  horizon missed. **A known failure:** on the same harbour, turned, the haze
  hid the horizon and the breakwater, at 4°, was read as the sea at σ 0.09 and
  would have been applied; one line alone has nothing to disagree with it.
  36 frames with visible verticals were refused at σ above 0.5; against their
  own untouched reading, σ 0.3–0.5 reads agree to 0.29° p90, and there are
  too few above 0.5 to move the threshold on. Upright offers: 32 of 35
  plausible; wrong on a miniature city seen from above (−37), doubtful on a
  gate shot from below (+40). Every level over about 1.05° on a 3:2 frame is
  an offer under the 5 % rule, as designed.
- **The person gate was too strict for crowds:** a level of a few tenths moves
  the edge by less than a cell, and every passer-by already cut by the edge
  refused it. Each person or animal is now its own joined set of cells: one
  whole in the frame stays whole (an animal a cell wider), one the frame
  already cuts may lose what lies within a cell and a half of the old edge. 8
  of the 9 street frames it blocked now level; the ninth has feet just above
  the bottom edge and stays an offer.
- **The blind test is ready to judge** (§9.4, P1's arms): 40 of the
  photographer's frames (20 where the read levels, 20 spread over the rest of
  the Japan set), each rendered untouched, by the old Auto (8e38d88, before
  P0) and by this one (`auto_blind_renders`: the level as the app decides it,
  then the light on the app's path), at 1 400 px; the untouched renders of
  the two codes are identical to the pixel. 120 pairs on a local page,
  shuffled and sided per judge, answers downloaded as JSON and scored against
  the gate by `score.py` — all in `numa-scratch/auto-blind/`. Lightroom and
  the camera JPEG are not arms yet.
- **The sloping-land stratum is 30** (2 October, 9f02bd2, from eight trips:
  reservoir shores, mountain flanks, vineyards, tea terraces, dunes with
  posts, cliffs): **levelled 0 of 30**, untouched or turned ±2°; 8 of 8
  deliberate tilts refused — all as "nothing to go by", none read as a slope
  and then rejected. The P1 level gate is met.
- **The subject is lit by the exposure** (2 October, the photographer's
  choice, §3.5 above): no masks, no HDR, the ends may clip.
- **Light strata, 2 October** (`numa-scratch/auto-light-strata/`, 30 night
  or low-key frames — ten shot at −1 EV or lower — and 30 already good). The
  first run of the subject exposure failed both gates: 12 of 30 nights
  brightened (candle-lit dinners to flat daylight, a blue hour lost), 8 of 30
  good frames moved, mostly up for a dark dress, a back turned or dark hair.
  Three changes: a person sets the exposure by their **face** and not at all
  without one, an animal whole (`lit_part`); the exposure goes up only for a
  subject a stop or more darker than the frame's upper middle (in shadow or
  against the light, not a dark room); and a **low-key** frame — its middle
  under 0.15 or its darkest tenth under 0.04, a black sky over a floodlit
  facade — is not brought up by the ends nor its black lifted. Again: **nights
  brightened 0 of 30**; good frames moved 5 of 30 against a gate of 2, all
  judged neutral or better — two hazy or sunlit frames brought down 0.75 by
  the ends to give their sky back, a deer in the woods up 0.75, a girl in her
  own shade up 0.92 by her face. The gate assumes a move is harm; here it was
  not, and it is left as measured rather than tuned to the number.
- **The camera's level without a tripod** (2 October; the photographer has
  none): the X-T5's RollAngle beside the read on 320 of his Japan frames,
  34 of them with the sea or buildings sure to 0.3°. The sign is the read's
  (straighten = +roll, the turn taken off, landscape and portrait alike),
  steps of a tenth, nought within a degree; but it is 0.34–0.47° off the read
  at the median and 1.35–1.43° at the 90th percentile — a bound, not a level.
  Used as §4.3 step 2 has it: a camera that read nought refuses trunks and
  posts claiming more than 1.5°. "Your camera knew" (§6.5 #9) does not hold
  on this body.
- **The AF point chooses the subject** (§3.4): Fujifilm's single point, in the
  frame on screen; off the matte, the photographer focused on something else
  and the subject sets no exposure; on it, among several faces the nearest
  is the one measured. Zone and wide AF, a turn or a mirror: not used.
  On the night, good and blind sets (100 frames) it changed nothing: the
  subjects left were already under the photographer's point; it is a guard
  for the passer-by.
- **The subject's own white holds**: up no further than its brightest two
  per cent stay under white (§11, white plumage, a white shirt).
- **One light for a burst** (§6.5 #7, from P4; the photographer's call on 2
  October, with Evoto's Match as the model): a Same Light row on the card,
  `auto::key` and `auto::match_light` — the targets shared, not the sliders.
  Levels are not shared (no tripod check yet). Seen on the demo burst.
- **P3, first version** (GEOM-006, the photographer's call on 2 October):
  `auto::crop::proposals` and a Crop row with a toggle group and the
  proposals dashed on the canvas. From §5: the aspects, the birder's
  orientation card, the 8 MP floor, an 11 × 11 grid at 4 % steps of area, the
  τ 0.08 against "as shot", NMS at IoU 0.7 with two aspects, and the reasons.
  Not yet: joints (no waist crops), look room, intrusions and the bright
  mass, per-proposal tone, hover previews, the chooser at the foot of the
  viewport.
- **Against the photographer's own crops** (2 October,
  `the_crops_against_the_photographers`, `numa-scratch/auto-crops/`): his 16
  catalogs hold 96 crops, most of them perspective fixes; 26 plain crops
  left, 10 of them real (under 0.7 of the frame). First version: 2 of the 10
  within IoU 0.7, and one hard-gate miss — a 1:1 through a gull's wingtip.
  Changed: no trims (a proposal keeps at most nine tenths, the trims crowded
  out his 4:5), a content term (all that is not sky, so empty sky goes before
  a dome's top), weights nearer §5.4 (area 0.15, margins 0.10, content 0.15),
  the birder's card a place of its own, and every animal's box a quarter
  wider and a tenth taller kept whole. Now **3 of the 10 within 0.7, 8 within
  0.5** — under the gate of half. What is left: he crops animals tighter than
  "whole, and no deeper" allows (a deer's rump the frame already cut, kites,
  a sea lion, with look room); a pagoda, a train, a baptistery are subjects
  the grid does not name; and the gull is still cut — the segmentation takes
  gull and the hand holding it for one person, so the animal's rule never
  applies. Saliency for non-living subjects and YOLOX's boxes are the next
  step, then look room.
- **YOLOX's boxes in the read** (2 October): `distractions::objects` gives all
  80 COCO classes, the read keeps them (`Scene.objects`). An animal's box,
  1.5 % of the short side wider and not at the frame's edge, is kept whole;
  that is the "gull", in fact a kite the grid took for a person, whose wingtip
  the portrait card cut (DSCF2934), now whole. A thing's box is not a gate —
  a train's box half the frame wide left DSCF2459 no crop at all — but with
  no person or animal in the grid YOLOX's largest animal or thing is the
  subject. A passer-by the frame already cuts may now go altogether (never
  the largest part). Healed spots: no crops (§5.3). Still 3 of 10 within 0.7,
  8 within 0.5; a crowded street (DSCF4311, 20 people, no face) gets no
  crop, the grid joining the crowd into one subject; YOLOX does not name a
  pagoda or a baptistery.
- **P2, 4 October** (`auto-p1`, after main's 0.35.0 merged in): the
  subject's light as the photographer's 2 October choice left it — one
  exposure, no masks. Built: §3.3's intent from the file (`auto::Intent`:
  compensation, DR, the Color setting), §3.2's lights out of the top end
  (`auto::LIGHTS`, and on a night the bright spots under 0.05 %), §3.2's fog
  read on Blacks, §3.5 step 2's silhouette, Vibrance capped at 12 over faces
  and none on monochrome, and the card naming what the exposure was set for.
  **Changed from the plan, measured:** compensation is read in manual
  exposure too (his M frames with −1⅓ have Auto ISO, 125 to 1 600 under it);
  +1 EV is not held (a hall at +1 that still came out dim stayed dim);
  §3.2's ramp from `DIM` to 0.86 was built and taken out — on 149 random
  frames it moved twenty more by a tenth of a stop or more, many by the
  whole ¾ with Whites +80, and the daylight ones looked at came out lighter
  and flatter, the hazy ones washed; point lights by day were left out at
  first and took two already-good frames up ¾ (sky between branches), so
  they count only on a night. **The sclera** (§3.5) was built as the
  reference no skin moves and measured on 97 of his faces large enough:
  42 readings, two stops under the skin's lit side at the median and 2.8
  stops between the quartiles — lids and lashes at the proxy's size — and
  taken out; the skin-tone spread of §9.3 stays open. **Measured against
  P1** (`numa-scratch/auto-light-strata/`, `night7`, `good7`, `random7`
  against `night4`, `good4`, `randomref`): night brightened 0 of 30;
  already-good moved by more than 0.1 EV 5 of 30, as in P1, two of them now
  with Whites at 0 under DR200; 149 random frames, 16 changed and none in
  exposure — eleven Whites at 0 under DR200 (no visible change on the four
  looked at), two lamps, two Blacks left on a flat frame (a red desert, a
  cream statue: a little flatter, a matter of taste), one Vibrance under
  faces. The silhouette did not fire on any of the 209 frames; its test is
  synthetic. **Not done in P2:** the wildlife admission rate, the groups and
  wrong-subject strata of §9.3, backlight against today's Auto, and a blind
  arm for P2.
- **P3, second round** (4 October, branch `auto-p3`, `numa-scratch/auto-p3/`):
  - **What stands out where nothing is named:** with no person or animal in
    the grid and no animal or thing from YOLOX, the read asks BiRefNet (the
    Subject chip's model) for the frame's foreground and keeps it on a grid
    48 cells on the long side (`Scene.salient`, read VERSION 2, the matte in
    its key); that is the subject. Asked only then: it costs as much as the
    rest of the read.
  - **The joints** (§5.3 rule 2): bands in face heights below the chin, from
    Drillis and Contini's proportions with a YuNet box about nine tenths of
    the head — neck 0–1.0, waist 1.6–2.7, knees 4.7–5.7, ankles 6.8–8.1. A
    person lost through the bottom edge below their face, with that edge
    between the bands, no longer refuses a crop, so head-and-shoulders,
    chest and waist crops can be offered. A figure is taken as standing only
    when its feet are in the frame 6.5 face heights or more below the chin;
    otherwise only the chest (a woman seated on stone steps had her raised
    knees cut by the first bands). The bands hold against a trim too: a
    frame that already cuts at the neck may not be trimmed there. Elbows
    held away from the body and wrists are not placed (hands move); no pose
    model. A level keeps everyone whole, as before.
  - **Look room:** YuNet's five points kept (`Scene.landmarks`); the yaw is
    the nose's offset from the eyes' midpoint over the face's width, and at
    0.1 or more §5.4 L asks for twice the room in front as behind. Mirroring
    mirrors the crop (a unit test). On 60 of the photographer's frames with
    people, 8 faces are turned that far and 3 frames' proposals moved, each
    towards where the face looks. Animals get none: neither the grid nor
    YOLOX's box says which way one faces.
  - **The edges** (§5.4 E, 0.20): a YOLOX box that is not the subject's and
    under a fifth of the frame costs what the border cuts off it (a sliver
    most), and the outer twentieth's share near white as rendered — not sky —
    three times over (`Scene.bright`). With neither in the frame the term
    gives its weight to the others. Counting a white sky moved a kite closer
    to his crop but cut a building's sky off, so the sky is left out.
  - **On the card:** the pointer over 1, 2 or 3 draws that crop and the
    others faint.
  - **Measured** after each of the four steps, on his ten real crops: **3
    within IoU 0.7, 8 within 0.5, every time** — the P3 gate (half within
    0.7) is still not met. Cut gates, judged on the drawn proposals for his
    26 crops and 60 frames with people (Japan, Mallorca, Italy): no face cut,
    nobody cut at a joint (after the second set of bands), no animal cut.
  - **Where it misses, and why:** the deer (DSCF9580, 0.38) — his crop cuts
    its hindquarters and legs, which rule 1 forbids by design; the crowded
    street (DSCF4311, no crop) — the grid joins twenty people into one
    subject and YOLOX's boxes do not split it yet, the next step, with a
    rule for which person a crowd's subject is; the kite (DSCF2943, 0.59) — a
    4:5 portrait round a bird whose box is 1.4 times taller than wide, under
    §5.2's 1.6 for the birder's card; Cinque Terre (DSCF2476, 0.64 → 0.56) — a
    portrait of a village from a landscape frame, with a boat for the subject
    and his crop through a small boat the edge term avoids; the sea lion,
    the pagoda and the alley (0.59, 0.67, 0.61) — he crops to 0.38–0.56 of
    the frame, tighter than the area term lets into the top three. Ten
    frames do not carry a constant fitted to them; nothing was.
- **P3, third round** (4 October, `auto-p3` after `auto-p1` 72ffc2e, the
  photographer's go on crowds):
  - **A crowd split into its people** (`corners::people`): each YOLOX person
    box, and each face no box holds grown to a figure, takes the joined
    cells of the grid it holds — the smallest box, for one in front of
    another. A box mostly inside a larger one, with no face of its own, is
    that person again (YOLOX found a man's torso as well as the man,
    DSCF2413). What no box holds stays a part for the gates, but is no one
    to choose.
  - **The subject, §3.4 steps 4–5** (`crop::choose`): +2 under the camera's
    single AF point (now kept in the read, `Camera.focus`, VERSION 3; a cell
    within 2 %, or an animal's YOLOX box — a kite is a few cells of the
    grid), +2 holding a face, −2 touching two edges or over 60 %. The
    highest, then the largest; those within a point and of a size with it
    (§3.4's 1.5×) are one subject together; together over 40 % of the
    frame, **no one stands out and no crop is offered** rather than a guess.
    An AF point on none of them says the photograph is of something else, as
    `lit_part` reads it. Not built: §3.4's +1 for a sharp box (the read keeps
    no sharpness) and its face-size clause beyond the 1.5× of size.
  - **Who a crop may leave out:** anyone who is not its subject and shows no
    face, whole — never a sliver (§5.3 cuts nobody that way; the face gate
    still keeps every face in). Before, only someone the frame already cut.
  - **Measured:** his ten real crops, **3 within 0.7, 8 within 0.5, as
    before**. The crowded street (DSCF4311) had no crop and has three, round
    the man in the folding chair the AF point is on, the best at 0.50 (a 1:1
    leaving both walkers out; his crop keeps a sliver of one, which the
    gates do not); the alley (DSCF4218) 0.61 → 0.56 (its AF point is on no
    one, so no person is its subject). Cut gates on the drawn proposals for
    his 26 crops and the 60 frames with people: no face, joint or animal
    cut; two frames that had no crop now have one (a stage crowd, a
    commuter train).
  - **The kite's portrait card, left at 1.6.** §5.2 puts a box 0.7–1.4 wide
    for its height in the compact range (1:1) and only one 1.6 times taller
    than wide among the upright (the standing heron). The kite (DSCF2943),
    1.4 times taller, is compact by the plan's own ranges; lowering the
    card's threshold to it would be fitting to the frame.
- **5 October** (`auto-p1`, main merged in at 6ef7193; the photographer
  asleep, "je gaat gewoon door en je doet gewoon je best"):
  - **A blown top end keeps the exposure** (ADJ-001). `exposure_from_the_ends`
    no longer comes down for the brightest half per cent; Whites and
    Highlights bring it in, and only a subject that itself burns out takes
    the exposure down (`exposure_for_the_subject`). Found on the blind set:
    seven daylight frames had been brought down the whole ¾ with Whites,
    Highlights at 0 on all forty, and the subject-and-clipping choice of 2
    October says the opposite. Strata against P2: already-good moved 5 of 30
    → **2 of 30** (the gate); night brightened **0 of 30** as before, its six
    frames that came down ¾ now as shot with the top held; random:
    11 of 149 frames change, exactly the eleven that had come down ¾; none comes down now and 23 go up, as before; Whites at −100 on two, Highlights past −80 on one. A lamp far past white is beyond every slider and is left.
  - **A stand-in blind judge** (Claude, sides shuffled, the key read after):
    the forty frames re-rendered on this code, 80 pairs, then the 14 whose
    render the change above moved judged again. New over old **60 %** of
    decided pairs, old over new 40 %; untouched over new **53 %** (58 %
    before the change). The gate (old ≤ 10 %, untouched ≤ 15 %) is not met
    on this judge. It is weak evidence: on 50 of the first 80 pairs the mean
    brightness differs by less than 2 of 255 (Auto left the light, or only
    levelled a few tenths), and choices there followed framing or nothing;
    where the light differs, the brighter side won 13 of 23. Where old Auto
    lost it was the cut-out subject lifts (a van, passers-by gone grey). The
    photographer's own test stays the gate (`numa-scratch/auto-blind/`;
    this judge's sheets in `judge/` and `judge2/`).
  - **The crop gate by weights, leave-one-out** (GEOM-006). Area and content
    at 0.15, 0.08 and 0, 16:9 without a horizon, and a second place for a
    crop tighter than the first by §5.5's 0.7, all 36 combinations on his 26
    crops: at best 4 of the 10 real ones within 0.7, chosen on nine and
    tried on the tenth **1 of 10**. Seven of his ten keep 0.3–0.57 of the
    frame, where the proposals sit at 0.6–0.9; a tighter second place alone
    went to 2 of 10 (it displaced the third that matched). Nothing kept; the
    proposals stay ideas, and learning his crops is P5's.
  - **The hover** renders the crop (§5.5): 160 ms on a number and the canvas
    shows the photograph as that crop leaves it (`preview_document`, from
    the presets' hover), the light as it is; taking it measures again. The
    outlines step aside while the preview shows.
  - **Wildlife** (§9.3, reported, not a gate): YOLOX run over 8 533 of his frames (`animals_in_frames`) found an animal of 0.5 % of the frame or more in 451; 200 of them at random: a subject measured on **54 (27 %)** — the others too small for the 2 % the segmentation needs, or not named by it; the exposure up on 60 (27 by the subject, the rest with nothing near white), down on one (a subject burning out). Ten of the 60 looked at, all neutral or better: deer under trees, a kite against cloud, a seal through glass, a rooster. `numa-scratch/auto-wildlife/`.
- **Not done:** the photographer's judging; per-proposal tone measured before
  the crop is taken, and the chooser at the foot of the viewport (the card's
  row is the chooser); groups and the wrong-subject strata; backlight
  against today's Auto.

---

## 1. Why the current Auto is not it

**The exposure is usually right already, so a better Exposure number is not
the answer.** RENDER-008 matches every raw's base exposure to the camera's own
JPEG, held to 0.15 EV by the corpus test (`docs/FEATURES.md:1466`, the
matching in `match_camera_exposure`, `crates/numa-io/src/raw.rs:1796`). That
rendering is what the photographer saw in the viewfinder, and it already
includes the exposure compensation they dialled in. ADJ-001 then measured that
metering on the middle is wrong: on a bird against cloud it made the picture a
full stop darker, and over 24 frames it moved the exposure on all 24 where the
ends rule moves it on one (`docs/FEATURES.md:2131-2148`). What is missing is
everything a printer does after the exposure: find what the photograph is
about, give it the light, hold the sky, level by the true horizon, frame for
the subject, and show the marks.

What the code did before P0, in order of harm.

**Bugs, where the code contradicts its own stated intent**

1. **Tone is measured on pixels the photographer cropped, turned or mirrored
   away.** `auto_tone` passes the uncropped, unrotated `photo.working` to
   `render::auto::tone` (`src/ui/window/geometry.rs:246`), while the subject
   alpha comes from the mask frame (`mask_frame_job`,
   `src/ui/window/mask_list.rs:418-435`), which `mask_geometry`
   (`crates/numa-render/src/lib.rs:414-422`) builds with the crop, the angle,
   the quarter turn, the mirror and the keystone applied. `luminances` maps the
   alpha proportionally onto the working image
   (`crates/numa-render/src/auto.rs:239-242`). After a crop to the right half a
   subject at 0.5 is read at 0.5 of the whole frame, not at 0.75; on a
   quarter-turned or mirrored document the alpha lands transposed or flipped,
   on a different region altogether. The 0.5 % and 99.5 % ends include
   whatever was cropped out, so a blown lamp the photographer cropped away
   still pulls the exposure down. No test or harness uses a cropped, turned,
   mirrored or keystoned document.
2. **Auto's biggest move lands on a slider nobody can see.** `tone` writes HDR
   up to 40 in proportion to the lift (`auto.rs:144`). HDR is `all[8]` in the
   slider list (`src/ui/window/panel_sliders.rs:302`); the Light page lays out
   `all[2..8]` (`src/ui/window/light.rs:66`) and no other page lays out index
   8. HDR is the adjustment that flattens the frame: on ADJ-001's bird, HDR 100
   alone takes the brightest pixel from 0.936 to 0.805, and the +2 EV mask with
   HDR 40 that Auto writes takes it to 0.896 (`docs/FEATURES.md:2158-2162`).
   *Corrected while building P0:* the missing row is not a bug. HDR-001 records
   that the row left the panel on purpose (PANEL_PLAN P1) because HDR "is the
   one of them that Auto sets for itself". The fault is the one in bug 7: Auto
   overwrote an HDR it had not set, a merge's 50 among them.
3. **Auto deletes the photographer's own Subject mask.** The Subject chip
   makes a `Shape::Subject` mask with `name: None` (the chip's name equals the
   default, `mask_list.rs:247`, `:675-677`), and `Auto::apply` removes every
   non-inverted Subject mask with no name (`auto.rs:56`), with its curve and
   grade. `Mask` has no field that says who made it
   (`crates/numa-core/src/mask.rs:531-610`).
4. **The subject is sharpened and colour-denoised twice.** The lift is built
   with `Basic::with` (`auto.rs:140`), which starts from `Basic::default()`
   and so carries `sharpen: 25` and `denoise_colour: 25`
   (`crates/numa-core/src/document.rs:152`, `:161`), and a mask applies its
   own detail settings (`lib.rs:1094-1107`). A mask should start from
   `Basic::local()` (`document.rs:277`), as `Mask::new` does.
5. **Levelling succeeds silently, on the main thread.** `auto_level` toasts
   only "none found" and "already level"; on success it sets the slider and
   says nothing (`geometry.rs:181`). It and `auto_perspective`
   (`geometry.rs:141`) render and filter on the GTK thread.
6. **After a lift the Auto button disappears.** The new mask is selected
   (`geometry.rs:277`), and the button is `global_only` (`light.rs:63`), so
   pressing Auto again needs a deselect first.
7. **Auto overwrites what the photographer set.** `Auto::apply`
   (`auto.rs:44-53`) writes exposure, whites, blacks, highlights, HDR and
   vibrance unconditionally, whatever the photographer had there. A bracket
   merge sets HDR 50 (`src/ui/window/merge.rs:76`) on the slider that has no
   row, and pressing Auto silently resets it to 0. The solves also ignore the
   document's own look: `display_at` and the Highlights solve build
   `Basic::with(blacks, whites)` from a default (`auto.rs:181-186`,
   `:125-129`), so after a preset, a curve or Contrast +30 the ends do not land
   where they were aimed.

**Limits of the design**

8. **A dead band and a jump at `DIM`** (*computed*). Exposure moves only when
   the top 0.5 % displays above `BLOWN = 0.988` or below `DIM = 0.74`
   (`auto.rs:12-13`, `:100-110`). Whites at +100 cannot lift a top end between
   0.74 and about 0.86 to 0.94, so `solve` returns 0 and a foggy or overcast
   frame gets Blacks and a little Vibrance:

   | p99.5 displays at | Exposure today | Top end after |
   |---|---|---|
   | 0.60 | +0.75 (the cap) | 0.739 |
   | 0.73 | +0.75 | 0.851 |
   | 0.75 | 0 | 0.750 |
   | 0.80 | 0 (Whites +100 reaches 0.813, short of 0.94) | 0.800 |

9. **The subject is lifted toward one fixed grey whatever it is.** The median
   of the whole subject is aimed at display 0.55 (`SUBJECT_TARGET`,
   `auto.rs:22`) by up to +2 EV, so a black dog turns grey and dark skin is
   pushed toward a standard value. Face-priority exposure that ignores skin
   tone is the pattern Google's Real Tone work and El-Yamany et al. (2019,
   *unverified*) were written against. YuNet's face boxes are not used.
10. **The gate and the region are two different models.** A person or animal
    named anywhere by EfficientViT opens the gate (`subject_mask`,
    `auto.rs:66-81`; `named_something` is a mean alpha above 0.001), and the
    region lifted is BiRefNet's "whatever stands out" (`the_subject`,
    `crates/numa-render/src/lib.rs:340`). A hiker at 0.1 % of a landscape
    opens the gate and the lighthouse gets the stop.
11. **Separation is a hard-edged multiply.** The lift is a plain exposure
    inside a 12 px feather; ENGINEERING documents the "hard edge around her"
    once it is applied. Auto never lowers the surroundings or holds the sky.
12. **Blacks probably moves on most frames now** (*computed*). With the FT-028
    Blacks and `BLACK_POINT = 0.035` (`auto.rs:15`), a frame whose darkest
    half per cent sits at the "well-rendered" 0.06–0.09 that ADJ-001 quotes
    solves to Blacks −21 to −53. FEATURES still says "Blacks moves on none"
    (`docs/FEATURES.md:2334`). That is hidden contrast, worst on fog and haze
    where flatness is the mood, and it has to be re-measured before anything is
    built on it.
13. **Level reads line statistics, not the scene.** `level` votes coherent
    Sobel edges on linear luma, with a floor at 0.12 of the single strongest
    gradient (`EDGE_FLOOR`, `auto.rs:257`), so a sun or a specular point raises
    the floor and a soft sea horizon at dusk drops out. Horizontal and vertical
    lines vote into one histogram (`auto.rs:312-329`); a receding pier at 8°
    competes with the sea, and a sloping ridge counts as level. It never uses
    the sky/sea boundary the segmentation already found.
14. **Perspective throws away half of its own fit.** `robust_slope` fits lean
    against position and returns only the slope (`auto.rs:305`); the
    intercept, which GEOM-002 says is the straighten angle
    (`docs/FEATURES.md:2062-2071`), is discarded. Under a keystone that
    intercept is biased anyway (§4.2, E3). Level measures at the current
    keystone and Perspective at the current angle, so the answer depends on
    which button was pressed first.
15. **There is no crop at all**, and the three Autos sit in two panels:
    Straighten and Perspective only exist while the Crop tool is open
    (`src/ui/window/masks.rs:224`).

**The docs have drifted before this work starts.** ADJ-001 says "The button is
off the page for now" (`docs/FEATURES.md:2213`); `light.rs:37-64` builds and
shows it. It says the mask is `Segment`; the code makes `Shape::Subject`
(`auto.rs:57`). It says the mask is named; it is not. GEOM-002 describes an
intercept the code discards, and `auto::level` has no FEATURES entry. P0
corrects all of these.

---

## 2. The scene read

### What it is

Auto is cut into a module,
`crates/numa-render/src/auto/{scene,level,crop,tone,reasons}.rs`. `level`,
`perspective`, `robust_slope`, `coherent_edges`, `solve`, `display_at` and
`luminances` move there (`display_at` and the solves change as §3.1 says). One
read of the photograph serves every answer, and each answer is a pure function
of the read, the document and the working image. **Determinism is defined as:
on the same machine with the same model files, two presses give the same
document hash.** The model files and the execution provider are part of the
read's key, and the chosen subject is stored with the read, so a second press
reuses it rather than asking the matte again. That matters: a float16
`birefnet.onnx` still on disk answers differently from the float32 file that
replaced it, with subjects as far apart as IoU 0.26
(`docs/ENGINEERING.md:5076-5085`), and `matte.rs` may decline on one press and
not the next on an iPad short of memory.

```rust
// new: crates/numa-render/src/auto/scene.rs
pub struct Scene {
    pub asked: Asked,             // source hash, canonical framing, model files, execution provider, read version
    pub planes: Planes,           // 512 px log2 luminance, mean-pooled and max-pooled
    pub classes: Classes,         // 128 x 128 winner grid, shares, sky/water band
    pub faces: Vec<Face>,         // YuNet boxes and 5 landmarks (Vision on Apple), canonical coordinates
    pub camera: Camera,           // AF point and zone, bias, exposure program, colour and monochrome film, DR mode, roll, shutter, focal35
    pub edges: Vec<Vote>,         // coherent_edges votes, with the class under each
    pub level: Vec<Evidence>,     // (angle, sigma, source, support) per kind of evidence, the sea fit's row
    pub upright: Option<Lean>,    // vertical vanishing point on buildings: roll, keystone, bootstrap sigma
    pub intrusions: Vec<Intrusion>, // border-touching boxes, clutter, bright blobs
    pub subject: Option<Subject>, // on the press: anchor, component box, 256 px alpha, name, why chosen
}
```

**It is read in the canonical frame:** `geometry_only`
(`crates/numa-render/src/lib.rs:1537`) with the quarter turn, mirror and lens
correction, but no crop, angle 0 and an identity keystone. The existing
`found` cache has the opposite property: its key is the framing, and
`forget_model_frames` (`geometry.rs:52`) drops the segmentation on every crop
change. Every region in the read lives in normalised canonical coordinates and
is carried into a level, a keystone or a crop candidate with `crop_in_view`
and `between_frames` (`crates/numa-core/src/image.rs:304`, `:319`).
`source_map` (`image.rs:279`) is `pub(crate)` today and becomes public for
this. Nothing is re-run per candidate. When the document has no crop, angle or
keystone, the mask frame is the canonical frame and `photo.segmentation` is
reused.

**Panoramas and other merges are not read for level, emphasis or crop.**
`segment::of` squashes a 4:1 frame four times over and the classes fall apart;
a merge carries no maker note (no AF point, roll or compensation); and its
horizon is curved by the projection. Auto sets their global tone only and says
why. Segmenting in overlapping square tiles when the aspect exceeds 2:1 is
later work.

**What each field comes from:**

| Field | Source | Notes |
|---|---|---|
| planes | `photo.working`: linear, white balance applied, before any slider | The max-pooled plane keeps the brightest half per cent honest inside any crop; the mean plane gives the middle. Tone can be re-measured inside a crop candidate in milliseconds without a render (estimate: about 5 ms) |
| classes | `segment::of` (`crates/numa-render/src/segment.rs:307`), `present`, `winners`, `alpha` | EfficientViT-Seg B2, ADE20K 150 classes. The sea fit (§4.2, E2) needs the sky and water probabilities, which are the private `probability` field of `Segmentation` (`segment.rs:53`), and the guide, also private (`segment.rs:58`); only `coarse()` (`segment.rs:198`) is public, and `refine` is `pub(crate)`. *New* accessors expose the probability planes and the guide. The guide is display-encoded luma (sRGB byte / 255) of the rendered mask frame at 2048 (`luminance`, `segment.rs:374-405`), not log luminance; E2 takes its log, or recomputes log luminance from the linear working image |
| faces | `numa_cull::faces::detect` (`crates/numa-cull/src/faces.rs:69`) on the canonical proxy; on Apple, Vision's face landmarks if counsel says no to YuNet (§8) | Today `ensure_faces` runs on the cropped frame and stores into `document.faces`, which is `#[serde(skip)]` |
| camera | `raw::af_point` (`crates/numa-io/src/raw.rs:1684`, Fujifilm RAF only, with its `zone` flag, `:1681`, `:1718`); `Summary.exposure_bias` (`raw.rs:1863`, filled at `:1933` on the EXIF path and `:1981` through rawler); `film_mode` (`raw.rs:1459`, colour simulations only); `raw::colour_setting` (`raw.rs:1669`, tag 0x1003, where Fujifilm records Monochrome, Sepia and Acros; nothing calls it today); `raw::shot` (`raw.rs:1721`); *new* reads of the Fujifilm roll and DR mode through `makernote_tag` (`raw.rs:1487`), and of ExposureProgram | No part of rendering or Auto uses the compensation today; its only consumer is the info panel's "Compensation" row (`src/ui/window/info.rs:308`). `Summary.film_mode` is filled only when the make is Fujifilm (`raw.rs:1984`) |
| edges | `coherent_edges` (`auto.rs:374`) on log luma at 900 px | Each vote keeps the ADE20K class under it |
| intrusions | YOLOX through the private `distractions::detect` (`crates/numa-render/src/distractions.rs:29`), opened to all 80 COCO classes; clutter classes touching the border; bright blobs from the max plane | `people` reads only class 0 today (`distractions.rs:61`). The named COCO animal classes also serve the subject's admission (§3.4) |
| subject | §3.4, on the press | The only part that needs the matte |

### When it is computed, and where it is kept

- **On open, the cheap parts.** From `write_rest_of_panel`
  (`src/ui/window/open.rs:374`) when the models are installed and
  `power::frugal()` is false, under `power::background`: segmentation of the
  canonical frame, YuNet, edges, the maker note, the planes, and the level
  evidence including the sea fit while the live `Segmentation` is in hand.
  Today that function only segments when masks are pending or the Masks tab is
  showing (`open.rs:400-404`).
- **On the press, the matte.** BiRefNet runs when Auto is pressed, or after
  the photographer has stayed on the photograph for about 2 seconds (estimate,
  to tune), never on every open. Its cost is the reason (below).
- **In memory:** `OpenPhoto` gains `scene: Option<Arc<Scene>>`, keyed by
  `Asked`, so a crop does not invalidate it. The live `Segmentation` of the
  canonical frame is kept beside it while the photograph is open.
- **On disk, in a table of its own.** The existing `found` table holds one row
  per photograph: `photo_id` is its primary key
  (`crates/numa-io/src/catalog/schema.rs:344-349`) and `save_found` upserts on
  it (`crates/numa-io/src/catalog.rs:1900-1901`), so a row for the uncropped
  framing would overwrite the cropped-frame row the mask chips depend on. It
  also keeps only the winning class per cell, one byte each
  (`crates/numa-io/src/masks.rs:96`), not the probabilities. So the read goes
  into a *new* table `auto_read(photo_id, asked, json)`, keyed on both: the
  class grid, faces, the camera's fields, the level evidence with the sea
  fit's answer and row, and the chosen subject (anchor, component box and a
  256 px 8-bit alpha, about 10–20 kB; the rest a few kB a photograph,
  estimates measured in P1). The sea fit runs once, from the live
  segmentation, when the read is made; a reopened photograph uses the stored
  answer, and a stale read (a new model file or read version) runs the model
  again. Storing the sky and water probability planes as well (two 128 × 128
  planes, about 32 kB at 8 bits, estimate) is the alternative if a re-read on
  reopening proves slow; it is measured in P1. The luminance planes and the
  full matte are recomputed, never stored.
- **A whole-library pass is deferred.** Bumping `cull::VERSION`
  (`crates/numa-cull/src/lib.rs:7`) would re-queue every photograph, and
  `measure_photo` (`crates/numa-io/src/analysis.rs:14`) reads a scaled
  rendering rather than the linear working image, so its planes would be the
  wrong ones. Later, when Analyse runs anyway, the class grid, the faces and
  the camera's fields can be filled in at its 640 px without bumping the
  version, so that Auto is instant across a library; the planes always come
  from the working image.
- **Nothing is applied until the press.** README says Auto "is a button, never
  a default" (`README.md:219-221`), and a precomputed answer stays an answer
  nobody asked for until then.

### What it costs

| Step | On the card | On the processor | Source |
|---|---|---|---|
| EfficientViT-Seg B2 at 1024, float32 as shipped | 38 ms in one table, 301 ms and 10.4 J in another; re-measure before any gate | 224 ms | `docs/ENGINEERING.md:1870`, `:4890-4898` (that table's 216 ms and 6.9 J are a float16 conversion that overflows and was not taken) |
| YuNet | runs on the processor | 1.3 ms | `ENGINEERING.md:1875` |
| YOLOX-s | runs on the processor | about 64 ms | FEATURES RETOUCH-007 |
| PP-ResNet50 (naming) | runs on the processor | 6.8 ms | `ENGINEERING.md:1877` |
| Edges at 900 px, planes at 512 px | — | not measured; 30–60 ms (estimate) | — |
| Maker note | — | under 1 ms (estimate) | — |
| BiRefNet float32, on the press (shipped since 29 September) | 566 ms, 0.2 + 111 J, +8.1 GB of VRAM a run | 3.3 s, 36 cpu-s, 168 J, 4.4 GB | `ENGINEERING.md:5034`, `:5079-5081` |
| A subject recipe resolved again at export (today's path) | about 0.9 s and 120 J (sum of the above, estimate) | about 3.5 s (estimate) | `lib.rs:451-478`: EfficientViT and BiRefNet on the cropped 2400 px frame |

The cheap read is about 0.15–0.4 s and about 10 J on the card (EfficientViT's
10.4 J dominates, `ENGINEERING.md:4898`; the 38 ms table at `:1870` would put
it lower), and about 0.35 s on the processor (estimate). BiRefNet at 111 J is
ten times that, which is why it does not run on every open.

**Export does not pay the matte again.** Today a subject recipe is resolved
again at every export, on the cropped frame, through `with_masks_resolved`
(`lib.rs:451-478`). Batch-exporting a shoot would pay about 3.5 s on the
processor or about 110 J on the card per photograph (estimate), and BiRefNet
on the cropped frame could answer differently from the canonical alpha Auto
measured. Auto's subject mask therefore resolves from the stored alpha in the
canonical frame, upsampled with the guided refine as the segmentation already
is, and is mapped through `between_frames` (§3.4, step 9).

**There is no fast processor fallback for the matte.** All three proposals
budgeted IS-Net (196 ms on the processor) for machines without a card. In
practice it is not there: `matte.rs` deletes `isnet.onnx` as soon as BiRefNet
has loaded (`crates/numa-render/src/matte.rs:129-130`), IS-Net is not in
`MODELS` (`crates/numa-io/src/models.rs:5`), and `ISNET_ALLOWED` is false on
Apple (`matte.rs:38`). A processor-only Linux machine pays 3.3 s for the matte.
Auto therefore shows its evidence progressively (§6). BiRefNet lite is used
only on `target_os = "ios"` builds (`matte.rs:26-30`), that is iPhone and iPad;
a native macOS build gets the full 973 MB model (unless the Mac app is built as
Catalyst, which counts as iOS). Only on iOS can `matte.rs` decline for lack of
room (`room()` returns None elsewhere, `matte.rs:83-86`); then the light part
runs without a subject and says so. BiRefNet lite was measured only on a Linux
processor (5 GB peak without the arena, `ENGINEERING.md:1961-1963`); its time on
an iPad is to be measured.

From a cached read the answers take: level under 5 ms, crop under 30 ms on
256 px integral images, tone under 120 ms including a 512 px verify render
(estimates).

### The read is the parity contract

The Apple app compiles the same Rust crates, so the read, the answers and
their reasons are written once in `numa-render` and port without a rewrite.
`Scene` derives `Serialize`; its JSON is the contract between the builds. Three
things are not free.

- **The models run on the iPad's processor.** `numa-infer` has the CoreML
  execution provider (`crates/numa-infer/src/lib.rs:401`), but on iOS only
  SCUNet, Restormer, RealPLKSR and LaMa are routed to it (`ON_CORE_ML`,
  `lib.rs:452`). EfficientViT, BiRefNet lite, YuNet and YOLOX run on the
  processor, where their time and energy have not been measured, and
  EfficientViT overflows float16 (`ENGINEERING.md:4898`), so moving it to the
  Neural Engine is not free. **The read is measured on an iPad's processor
  before P1.**
- **Parity is defined on decisions, not numbers.** On Auto-60 the two builds
  must agree on whether to lift, on the subject (IoU at least 0.8), and on the
  level (within 0.2°). Evidence only Apple has is switched off for the parity
  run: `VNDetectHorizonRequest`'s angle as extra level evidence,
  `CalculateImageAestheticsScoresRequest`'s score as a crop tie-breaker, EXIF
  SubjectArea in place of the Fujifilm AF point, and Apple's maker-note
  AccelerationVector as a camera level on iPhone (§4.4). If counsel moves the
  Apple build onto Vision (§8), its answers will differ more, and parity is
  re-defined then.
- **The interface.** SwiftUI for the plan card, the outline and the chooser,
  and a touch design: hover, the A key and Space do not exist on a touch
  screen, so a tap on a proposal previews it, a second tap commits, and a
  press and hold shows "before Auto". 2–3 weeks.

The paid app's main input is the iPhone's own HEIC and ProRAW, which Apple has
already tone-mapped locally. A subject lift on top may be applied twice, so
until a stratum of at least ten iPhone frames says otherwise (§9.1) those files
are treated like a finished JPEG: rescue capped at +1 EV. On Apple, Vision's
`VNGenerateForegroundInstanceMaskRequest` gives subject instances with no
training-data question for the app, and is the first candidate if counsel says
no to BiRefNet.

---

## 3. Light

### 3.1 What stays, and what goes

The ends rule stays: ADJ-001 measured it and the middle rule was wrong. The
Whites, Blacks and Highlights solves against the real `ToneCurve` stay, but
**they solve against the document's own look**: the document's current `Basic`
and curves are held fixed and only the slider being solved varies, so a
preset, a profile, a film simulation or Contrast +30 is part of the
measurement. The verify render checks where the ends landed (§3.7).

Vibrance stays, capped at 12 when faces cover more than 2 % of the frame,
and skipped on a monochrome frame. Fujifilm records black and white not in the
FilmMode tag 0x1401 that `film_mode` reads (that table maps colour simulations
only, `raw.rs:1469-1482`), but in tag 0x1003: Monochrome, its colour filters
and Sepia at 0x300–0x310, Acros and its filters at 0x500–0x503 (ExifTool's
FujiFilm.pm). Auto reads it with `raw::colour_setting` (`raw.rs:1669`).

**Auto keeps setting HDR, and only its own.** HDR stays the slider Auto sets
for itself (HDR-001), under the same rule as the others below, so a bracket
merge's HDR 50 (`src/ui/window/merge.rs:76`) or a preset's survives Auto. (This
section first said Auto should stop writing HDR altogether; that would have
undone a deliberate panel decision and lowered ADJ-001's backlit bird from
0.410, so P0 did not.) Whether the lift wants HDR at all is still re-run
mask-only against mask-plus-HDR on the ADJ-001 table (§3.5).

**A global slider is written only where the photographer has not set it**:
while it is at its default or still equals the value Auto wrote last time
(`AutoRecord`, §3.8). Otherwise it is left and the plan card says so:
"Exposure is yours (+½ stop) — left".

Contrast, Saturation, Shadows (globally), Clarity, Texture and white balance
stay refused. The tests `auto_refuses_everything_that_is_a_matter_of_taste`
(`auto.rs:909`) and
`the_exposure_answers_to_the_ends_and_not_to_the_middle` (`auto.rs:815`) keep
their meaning for the global sliders; the local answers get tests of their
own.

### 3.2 Global exposure: the ends rule with a ramp

The switch at `DIM` becomes a ramp. With `t` the top end (p99.5 of non-light
pixels on the max plane, in the final framing):

```
lift      = log2(scene(0.94) / t)                 // as today
r         = 1 at display(t) <= 0.74, 0 at >= 0.86, linear between
exposure  = clamp(r * lift, -0.75, +0.75)         // and down when display(t) > 0.988, as today
```

This closes the dead band and the jump without metering the middle. The limit
rises to +1.5 EV only when the frame is not low-key: no night reading, and
Reinhard's automatic key
`a = 0.18·4^((2·log2 L̄ − log2 Lmin − log2 Lmax)/(log2 Lmax − log2 Lmin))`
at or above 0.12, with Lmin and Lmax at the 1st and 99th percentiles.

**Light sources are not the top end.** Pixels of lamp (36), light (82),
chandelier (85), streetlight (87), sconce (134) and traffic light (136), and
bright connected blobs under 0.05 % of the frame at 256 px, are left out
of `t`. A lamp is allowed to be white. Because the measurement is on the final
framing, a lamp cropped out no longer counts at all.

**Night is never brightened, and night is read from the content, not the
shutter.** A night reading is a dark sky (sky median below a display of 0.15,
estimate) with light-source pixels or bright point blobs present, or an
as-shot white balance under about 3500 K with point lights. It caps the global
move at 0 upward. A long exposure is not on its own night: a 30 s seascape
through an ND filter is daylight and is treated as such (a stratum of its own,
§9.1).

**Fog, haze and high key keep their flatness.** A cheap read of the spread
(the p5–p95 display range below 0.35, or the dark channel's median above 0.3;
both starting values) marks a hazy or high-key frame. There the Blacks solve
is switched off, so Auto does not add to those frames the hidden contrast of
limit 12. The ramp may still brighten them; the fog gate is "brighter, with
the display IQR changed by at most 5 %".

### 3.3 Reading the photographer's intent from the file

None of the three proposals read what the file already says, and an Auto that
undoes a deliberate choice loses the photographer in one session. RENDER-008's
base already includes the exposure compensation, so Auto never adds it again as
an offset; what it reads from the file sets only the direction Auto may move.

| In the file | Read from | What Auto does |
|---|---|---|
| Exposure compensation of −1 EV or less | `Summary.exposure_bias` (`raw.rs:1863`), and a *new* read of ExposureProgram | Low-key on purpose: global exposure may only fall. Not read as intent when it is the body's habit (the same value on 60 % or more of that body's frames in the catalog: many Fujifilm shooters keep −⅓ to −⅔ dialled in to protect highlights) or in manual exposure (ExposureProgram 1), where the field says nothing. The plan card says "−1 EV dialled in — kept low-key" |
| Exposure compensation of +1 EV or more | the same, with the same habit and manual rules | High key on purpose: global exposure falls only if the top end is blown |
| Fujifilm DR200 or DR400, or D-Range Priority | *new* maker-note reads, per ExifTool's FujiFilm.pm: DynamicRangeSetting 0x1402; when it is Manual, DevelopmentDynamicRange 0x1403 (valid only for manual DR); when it is Auto, AutoDynamicRange 0x140b, which records what the camera used; and DRangePriority 0x1443 (with 0x1444 and 0x1445) counted as headroom bought. All to verify on the photographer's RAFs | The photographer bought highlight headroom: Auto adds no positive Whites, and the sky hold triggers at a sky median of 0.75 instead of 0.80 (starting value) |
| Film simulation | colour from `film_mode` (`raw.rs:1459`, tag 0x1401); monochrome from `raw::colour_setting` (`raw.rs:1669`, tag 0x1003: 0x300–0x310 or 0x500–0x503) | Colour is never touched beyond Vibrance; on Monochrome, Sepia or Acros, not even that |
| A finished JPEG or HEIF | `image.display_referred` | The existing limits stay (Whites only up, Blacks only down); rescue is capped at +1 EV because 8-bit shadows band; a stratum of its own in the test |
| An iPhone HEIC or ProRAW (Apple build) | the make and the container | Treated like a finished file until the iPhone stratum shows a lift is not applied twice (§2) |

The base the ramp measures from is RENDER-008's camera-matched rendering, so
"exposure 0" means "what the camera showed". Any move away from it is written
in the plan card with its reason.

### 3.4 Choosing the subject

The subject is chosen once per read, on the press, and the rule is FT-025's:
**in v1 Auto lifts only what a model can name.** A wrong subject lifted a stop
is worse than a photograph left alone.

1. **Candidates:** the components of the matte, through a *new*
   `matte::components`. Today `matte::subject` (`matte.rs:588`) returns one
   merged alpha: `keep_subjects` (`matte.rs:600-661`) keeps every component of
   at least 5 % of the largest (`SCRAP`, `matte.rs:13`) and gives back no
   components. Beside them: `Segmentation::alpha(&MATTEABLE)`, person 12 and
   animal 126 (`segment.rs:407`); YuNet faces; and YOLOX boxes of the COCO
   classes that name a subject (person, bird, cat, dog, horse, sheep, cow,
   elephant, bear, zebra, giraffe).
2. **Admission:** a matte component is admitted only if it overlaps the
   person/animal alpha with an IoU of at least 0.5, or its box overlaps a
   YOLOX box of a named class with a box IoU of at least 0.5, or it contains a
   face centre. The unnamed components BiRefNet offers (the lighthouse, the
   wave) are not admitted. YOLOX is there because the semantic model alone
   names too little: FT-025 measured 14 of 30 frames where it named no person
   or animal, a kitesurfer among them, and ADE20K has no bird class of its
   own. **The gate's hit rate is measured on the photographer's last 200
   wildlife frames before P2**; if it fires on a minority, §6's headline is
   rewritten before it ships.
3. **The hiker case:** when the segmentation names a person or animal but no
   matte component reaches IoU 0.5, `matte::refine(photo, &seg_alpha)`
   (`matte.rs:172`) is asked on that region instead. Its answer is used only
   if it keeps at least half of what it was handed, the FT-025 rule for a
   declining refinement. Otherwise there is no lift, and the toast says "The
   matte and the person disagree — no subject lift".
4. **Choosing among admitted candidates**, by score: +2 under the Fujifilm AF
   point (`raw::af_point`, which already works on the photographer's RAFs),
   **only when its `zone` flag is false**: in zone, wide and tracking modes,
   which is how birds in flight are shot, the point is the zone's centre, not
   where the camera focused. On Apple, EXIF SubjectArea takes its place where
   the camera writes it; other makes have nothing. +2 containing a face
   centre, +1 if `numa_cull::measure::sharpness`
   (`crates/numa-cull/src/measure.rs:20`) on its box is at least 1.5× the
   frame's median, −2 if it touches two or more frame edges or covers more
   than 60 %. The AF point chooses; it never admits.
5. **Two subjects or more.** If two or more admitted candidates score within 1
   point of each other, or two or more faces are within 1.5× of each other's
   size, Auto lights their union box as one subject, or, when that union covers
   more than 40 %, refuses emphasis: "Two subjects — light left even". With
   more than three faces there is no emphasis and no subject mask at all, only
   the global tone. Couples, families, weddings, two birds on a branch and
   street photographs with many faces are a refusal stratum (§9.1): Auto must
   never light one member and burn the others.
6. **Accept** a coverage between 0.3 % and 60 %. Otherwise Auto still
   levels, offers crops and sets the global tone, and says "Could not tell
   what the photograph is about — light set for the whole frame".
7. **Name:** "Portrait" for a face; `classify::animal`
   (`crates/numa-render/src/classify.rs:63`) at confidence 0.6 or more gives
   one of ten groups ("Bird", "Dog", …); else "the subject". PP-ResNet's finer
   ImageNet classes (herons, egrets, cranes) could name the bird more exactly;
   that is a later, measured change. SFace names are not used by Auto: its
   weights are Apache-2.0 but its training data is research-only, and it is
   Linux-only.
8. **Anchor and facing.** With a face, the anchor is the eye midpoint
   (landmarks 0 and 1) and the facing is
   `yaw = (nose_x − eye_mid_x) / eye_distance`, turned when its magnitude
   exceeds 0.15. Without a face, when the subject's box is upright (taller
   than wide) the anchor is the centroid of the top 30 % of the matte's mass,
   the head of an upright bird or animal; when the box is wide (a bird in
   flight, whose top 30 % is its wingtips) it is the centroid. An animal's
   facing is the sign of (anchor_x − body centroid_x) when that offset exceeds
   10 % of the box width; otherwise there is no facing and the look-room term
   of the crop is dropped. A principal axis has no sign and is not used.
9. **What lands is the choice, not the question.** A `Shape::Subject` recipe
   does not store a choice. Each time it is resolved again (after a crop
   through `forget_model_frames`, on reopen, and at export through
   `with_masks_resolved`, `lib.rs:451-478`) it goes through `the_subject` to
   `matte::subject` and gets BiRefNet's merged "whatever stands out", the
   lighthouse included. That would bring back the fault ADJ-001 fixed, a mask
   measured on one region and applied to another ("+2.00 EV that moved its
   subject by four thousandths"). So Auto's subject mask is a *new*
   `Shape::SubjectAt { anchor: [f32; 2], classes: Vec<u16> }`, with the anchor
   in canonical coordinates. It resolves to the stored alpha of the chosen
   component (§2), or, when that is missing or stale, to the matte component
   that contains the anchor, and it is mapped through `between_frames`. The
   Subject chip keeps `Shape::Subject`. A regression test runs Auto, then a
   crop, then an export, and asks for a subject IoU of at least 0.95 between
   what was measured and what was exported.

Depth Anything V2 Small (P4) adds a second opinion that BiRefNet's "what
stands out" cannot give: the subject is near and in focus. Admitting unnamed
subjects through it is re-measured on FT-025's thirty frames first.

> **Superseded on 2 October** by the photographer's choice: "ik vind het
> oke als er highlights of shadows clippen als het betekent dat we het
> subject wel goed uitlichten (zonder maskers)". The subject is lit by the
> frame's exposure (its lit side, the 90th percentile, brought to 0.72 when
> it sits below 0.55, up to +2 EV; down up to a stop when the subject itself
> burns out), the ends may clip, and Auto lays no masks and no HDR — so the
> rescue masks of §3.5, the radials and "Around" of §3.6 and the sky hold go,
> and P2 becomes the subject's *choice* (§3.4: which subject, named, the AF
> point, groups) and its measure (backlight, the sclera, skin tone), feeding
> one exposure. Built on `auto-p1`; see Status.

### 3.5 Rescue: a subject in shadow, a subject against the light

The measurements, on the final framing: `key_S` the subject's log-average
luminance (alpha above 0.6, eroded as today); `key_R` the ring, the subject
dilated by 0.5·√area minus the subject minus the sky; `key_F` the frame's
non-sky, non-light log-average; `key_B` the background directly behind the
subject (the subject's box widened by a quarter, minus the subject, sky
included); `rim` and `core` the subject's inner boundary band and its interior
(band width 2 % of the subject's short side, estimate).
`sep = log2(key_S / key_R)`.

1. **Highlights first.** If the subject's p99 displays above 0.95 (white
   plumage, snow, a wedding dress), the subject mask gets Highlights solved to
   bring it to 0.94 (never above 0, never below −60) before anything is
   lifted, and no step below may clip a subject pixel that was not clipped.
2. **Silhouette kept.** If the subject displays below 0.05 and the sky behind
   it is more than 3 EV brighter, and the photographer's compensation or the
   frame's key says it was meant (a sunset, a low-key reading), it is a
   silhouette and is left: "Silhouette kept". Otherwise it goes to step 4.
3. **In shadow.** When `key_R ≤ key_F · 2^−0.7` (the subject's place is darker
   than the scene, measured on its surroundings), the lift is
   `L = ½ · log2(key_F / key_R)`, clamped to ⅓–1½ EV. That is half the depth
   of the shadow, measured independently of the subject's own reflectance: a
   black dog or a dark-skinned face goes back into the light and is not pushed
   toward a standard grey. Shadows +15 inside the mask when `L ≥ 1`. If the
   subject's p25 still displays below 0.10, up to +0.3 EV more, inside the 1½
   cap.
4. **Against the light.** The ring cannot tell backlight from a dark subject:
   a backlit person in front of a bright wall, foliage or a window has a
   bright ring, and a bird against cloud has a ring that is all sky. So
   backlight is read on its own: the rim is at least 1.5 EV brighter than the
   core, or `key_B` is at least 2 EV brighter than the subject's p75. Then the
   lift is `L = ½ · log2(key_B / key_S)`, clamped to ⅓–2 EV (today's cap),
   and the sky hold keeps the background at least 0.3 EV above the subject
   afterwards. For a face large enough to measure (each eye at least 12 px
   across at 2048, estimate) the reference that does not depend on skin tone
   is the sclera: patches beside YuNet's eye landmarks, whose brightest decile
   is aimed at a display of about 0.80 (estimate), within the same 2 EV.
5. **A dark subject in good light** (`sep < −1`, neither step 3 nor step 4
   read): dodge only until its p75 displays 0.30, at most +0.3 EV. A black
   dog stays black.
6. **A subject covering more than 40 %** (a close portrait, a macro) keeps a
   subject mask, and the background is held by Highlights on an inverted copy
   of it, instead of a global move the ends rule would block when the window
   behind is already blown.

**Measured before P2, not assumed.** On ADJ-001's backlit bird today's Auto
lands the subject at a display of 0.410 with a +2 EV mask and HDR 40
(`docs/FEATURES.md:2158-2162`). Without HDR, a mask alone lands it lower: in
sRGB-like arithmetic (not Numa's tone curve, so an estimate) a subject at
0.040 lifted 1½ EV displays at about 0.1. The rules above are run on
ADJ-001's table frames and FT-025's thirty before P2, and the subject display
values are published beside today's. If the backlit frames land darker than
today, Shadows inside the subject mask (up to +40) and then HDR inside the
subject mask (a mask's own HDR is applied, `lib.rs:1126`, but it needs a
visible row first) are the arms to test. P2 does not ship a backlight rescue
that does less than today's Auto on its own example.

### 3.6 Emphasis: light without a hard edge

When no rescue fired, the subject covers 1–40 % of the frame and is not
already brighter than everything outside it, the gap is `g = 0.35 − sep`. If
`g ≤ 0.1` the subject already leads and the answer is "already leads".
Otherwise **analytic Radial masks** carry the emphasis. A Radial has no hard
edge, so it reads as light falling off, not as a cutout; the matte is used only
for rescue. A Radial can still show: a +¼ Radial on a flat sky, water or
backdrop is a visible low-frequency bump, a glow around the bird. So "Light
on" is skipped when more than half of its feather band is sky or water, and
the halo check of §3.7 runs on every mask Auto lays.

| Mask | Shape | Amount | In the plan card |
|---|---|---|---|
| "Light on the bird" | `Shape::Radial`, centred on the face (eye midpoint plus 0.3 face heights down) or the matte's luminance-weighted centroid; radius 1.1× the subject's half-extents; feather 1.0 | `+min(g/2, ¼)` EV | on |
| "Around the bird" | the same Radial at 1.6×, `inverted` | `−min(g/2 + 1/12, ⅓)` EV | **off** until it wins the blind test: an inverted Radial is an off-centre vignette, which is a matter of taste |
| "Bird" (rescue, dodge or highlight protection only) | `Shape::SubjectAt` (§3.4), matte on, feather 24 | `L` or the dodge from §3.5; Highlights when its first step applies | on |
| "Sky" | `Shape::Segment { classes: [2] }`, `minus: [12, 126]` | −⅓ EV, Highlights −20 | on; its boundary is checked for halos along tree and mountain skylines |

- Amounts are rounded toward zero to twelfths of a stop, so every mark reads
  as a fraction a printer would write (⅙, ¼, ⅓, ½) and two presses give the
  same numbers.
- The caps are starting values. Miangoleh et al. trained their realism
  discriminator on "real" samples made by scaling a region's encoded (sRGB)
  values by ×0.85–1.15, and "fake" ones by ×0.5–0.75 or ×1.5–2.0
  (`train_realismnet.py`, `utils/applyedits.py`, on COCO instance mattes).
  That range is an assumption of their training, not a measured threshold of
  realism. Applied to encoded values it is about −½ to +0.44 EV in linear
  light (γ ≈ 2.2), so +¼ and −⅓ sit inside it. Their code is licensed for
  academic use only, so nothing of it is used beyond this reading. The blind
  test decides; the fallback is +⅙ and −¼.
- "Around" is skipped when the outer 10 % band is mostly sky or the subject
  touches the frame edge. When the subject already leads, "Around" alone burns
  the corners by up to −¼, and only if they are brighter than the subject:
  bright corners pull the eye out of the frame. Both are off by default with
  the rest of "Around".
- Luminance only. Colour moves inside emphasis (Saturation or Vibrance around
  the subject) are deferred: luminance carries the separation, and Cajar and
  Laubrock (2026, *unverified*) report that peripheral colour contrast is used
  only together with luminance contrast. The deferral does not rest on that
  paper alone: a colour move is a matter of taste by this Auto's own rule.
- **Sky hold** applies when sky covers at least 8 %, its median displays
  above 0.80 (0.75 with DR200, DR400 or D-Range Priority) and its p99 is not
  blown. In a backlit frame it keeps `key_sky ≥ key_S · 2^0.3`: the sky stays
  brighter than the subject.
- **Backlit and sunset frames** (sky key more than 2 EV above the ground,
  warm) cap the global key at a display of 0.42 so a sunset stays a sunset.

### 3.7 Verify, then keep only what holds

A 512 px render with `apply_stack` (`lib.rs:490`) is checked:

1. **Order:** the subject against the sky, and the subject against the
   brightest light source, must not change places.
2. **Clipping:** no new clipped subject pixels.
3. **Halo, on every Auto mask:** gradient energy in a band 2 % of the frame
   wide around each mask's edge (the subject mask and the sky's boundary)
   rises by at most 25 %; and inside each Radial's feather band, on smooth
   sky or water, the band-passed luminance (scales of 1/32 to 1/8 of the short
   side, estimate) rises by no more than the frame's own texture there
   (threshold set on the corpus).
4. **Budget:** the local change at any pixel stays within 0.7 EV, rescue
   excluded. **And a global budget**, so that rescue, "Around" and the sky
   hold do not rebuild with masks the HDR look: the
   display range p1–p99 in EV after Auto is at least 0.85× the range before,
   and the subject-to-sky separation shrinks by at most 30 %.
5. **The ends landed:** the top end and the black point are within
   `CLOSE_ENOUGH` (0.012, `auto.rs:18`) of their targets; otherwise the
   global answer is dropped.
6. **The mask that lands is the mask that was measured:** the IoU between the
   alpha Auto measured and the mask as `with_masks_resolved` resolves it is at
   least 0.95; otherwise that mask is dropped.

A failed check 1–4 halves the emphasis (or the rescue) once and checks again;
a second failure drops that mask, keeps the corrections, and names it:
"Separation skipped — it would show an edge".

### 3.8 What lands

- Global answers in the ordinary sliders, only where the photographer has not
  set them (§3.1). HDR only where Auto set it.
- At most four masks, each with a name, carrying what Auto gave them
  (`Mask.auto`, built in P0 in place of the `by_auto: bool` first planned),
  starting from `Basic::local()`. No Texture or
  Clarity on them: that is taste, and it would stack on Detail sharpening on
  feathers and fur. **A mask is Auto's only while `Mask::as_auto_left_it`
  holds**, so a mask they have tuned (+¼ to +½) is theirs, and a second press
  or a re-plan from the plan card never replaces it.
- **Nothing is selected afterwards**, so the panel stays on the photograph and
  the Auto button stays visible.
- `Document.auto: Option<AutoRecord>`, built in P0 for the six global
  sliders, grows to record the angle, rect and keystone, the mask ids, the read
  hash and the reasons. Auto may set a global slider, Straighten, the keystone
  or the inscribed crop only while it is at its default or still equals the
  record, and replaces only masks for which `as_auto_left_it` holds. It never
  touches the photographer's own.

**Refusals, each in words** (numbers go in the tooltip, not the sentence):
"Light left alone — the bird already leads", "Silhouette kept", "Exposure
already right — the brightest highlights sit just below white", "−1 EV dialled
in — kept low-key", "Exposure is yours (+½ stop) — left", "Two subjects —
light left even", "Could not tell what the photograph is about — light set for
the whole frame".

### 3.9 An experiment, not v1: key bands and Contrast

The key-band rule from the craft proposal stays a harness arm: a band for the
frame key by kind of scene (night 0.18–0.36, high key 0.50–0.66, backlit
0.25–0.42, otherwise 0.36–0.54), anchored on the camera-matched base, which
already includes the exposure compensation; inside the band nothing, outside
halfway to its edge. It must move the exposure on at most 3 of ADJ-001's 24
frames, and it ships only if it beats the ramp blind. Contrast (+10 to +20 on
frames whose display IQR is below 0.20) is the same kind of experiment, and is
refused outright on haze, fog, snow and high key, where flatness is the mood;
the haze read of §3.2 is what tells them apart.

---

## 4. Level

### 4.1 Order

**Upright first, then level, then crop, then light.** The building fit (E3)
gives roll and keystone together; level is decided with that keystone applied,
which removes today's order dependence. Merged panoramas are not levelled
(§2).

### 4.2 The evidence

| | Evidence | σ (roll) | When |
|---|---|---|---|
| E2 | Sea horizon from the segmentation; lake and other water as a weaker reading | from the residuals, floored at 0.05° | Sky (2) directly above sea (26), water (21) or lake (128) across at least 40 % of the width. Only sea near the pitch-implied horizon may win a conflict (§4.3) |
| E3 | The vertical vanishing point on buildings: roll and keystone in one fit | from a bootstrap of the residuals (0.3° as a starting value) | Wall 0, building 1, windowpane 8, door 14, house 25, column 42, skyscraper 48, hovel 79 or tower 84 cover at least 8 % |
| E1 | The camera's own level | 0.5°, and a non-zero Fujifilm reading only | Only after the test frames in §4.4 |
| E4 | Today's histogram, cleaned; verticals only unless E2 is present | 0.6° | Fallback |

**E2, the sea horizon.** Rivers (60) and pools are left out: they bring banks.

1. On the live segmentation's sky and water probability planes (the *new*
   accessors of §2), walk the columns. Find where `p_sky − p_water` crosses
   from above +0.3 to below −0.3 within three cells, interpolated; weight the
   column by the steepness. Skip columns where person (12), animal (126), boat
   (76), ship (103) or pier (140) wins within two cells. The subject is
   excluded with the cheap read's person and animal alpha, never the matte, so
   the level does not depend on whether the matte has arrived.
2. **Undo the squash.** `segment::of` resizes to a 1024 square ignoring
   aspect, so each cell is anisotropic. Map cells to normalised (u, v) and
   then to 2048-guide pixels before fitting; fitting in cells biases the angle
   by the aspect ratio.
3. **Refine** each column on the log of the guide's display luma (or on log
   luminance recomputed from the linear working image): the largest vertical
   gradient within ±2 cells (about ±32 guide pixels, the boundary error of a
   stride-8 segmentation), chosen with a continuity path across columns
   (dynamic programming) so one column cannot jump to a wave crest, with a
   parabolic sub-pixel peak, and kept only where the gradient is above the
   noise floor.
4. **Fit** with deterministic RANSAC (every pair among 64 evenly spaced
   columns), then Tukey IRLS total least squares. The canonical frame is
   already lens-corrected; columns within 10 % of each frame end are
   down-weighted by half (estimate), because an uncorrected wide-angle horizon
   curves most there.
5. **Accept** only if the inliers span at least 40 % of the width, at least
   60 % of columns are inliers, and the RMS residual is at most 0.25 % of the
   height. A curved or broken line is a shore, not the sea. A sky boundary
   with mountain 16, hill 68, field 29, sand 46, earth 13, rock 34 or land 94
   is never used: land slopes. Calm water that mirrors the sky can confuse the
   crossing; the straightness and residual gates catch most of it, and mirror
   frames are part of the sea stratum.

The fit also returns the horizon's row, which the crop's horizon term uses.
With a thousand columns and a pixel of noise the slope's standard error is
about 0.006°; the practical floor is lens distortion and the refinement.

**Only the horizon at infinity is level.** A lake's far shore at finite
distance, seen from an oblique bank, projects as a sloped line and still
passes the straightness gates, and a far shore thinner than one cell (10–16
guide pixels at 128 cells) is invisible to the classes. That is why lake and
water readings need a second source that agrees (§4.3).

**E3, roll and keystone in one fit.** `coherent_edges` votes are kept only
where a building class has alpha above 0.5. Under a keystone, a line's lean
depends on its height in the frame as well as its position: fitting lean
against x alone and reading the intercept as the roll (what GEOM-002 describes,
and what `robust_slope`'s discarded intercept would give) returns roughly
tanθ·D/(D+y). With the vanishing point two frame-heights away and the edges in
the upper or lower half, the roll comes out 20–33 % wrong, 0.4–0.7° on a 2°
roll (*computed* by a reviewer, not on photographs), and `WORTH_IT` applies the
keystone exactly when the vanishing point is near and this bias is largest. So
E3 fits the **vertical vanishing point** directly: for a vote at (x, y) with
lean l, `x − l·y = vx − l·vy`, which is linear in (vx, vy). Tukey IRLS on the
building-gated votes gives the vanishing point; the roll is its direction, the
keystone its distance, and σ comes from a bootstrap of the residuals. It is
tested on synthetic renders with roll and pitch together, not on roll alone.

The keystone is applied at 100 %, as today, and `WORTH_IT = 2` still gates
it. The photographer's review preferred 80 %, which leaves a trace of looking
up as architectural printers do; the engineering review called it untested
taste. It goes into the blind test as an arm (80, 90, 100 %) and changes only
if it wins. Where a maker note gives pitch and `focal35` is known (Nikon,
Pentax, Ricoh, recent Canon bodies, OM System with the level gauge on; Fujifilm
writes no pitch), the pitch is a prior on the keystone, once that brand has a
maker-note reader (§4.4). Trees shot looking up never vote: the gate is
buildings at 8 %.

**E4, the histogram, cleaned.** Log luma; the floor at 0.12 of the 99th
percentile of the gradient instead of the single strongest; separate
histograms for near-horizontal and near-vertical lines. **Without E2 or E3,
E4 counts only near-vertical votes**, because verticals (trunks, poles,
standing people) follow gravity and horizontal lines in nature rarely are
level, and it needs at least three separate vertical structures. Horizontal
votes count only when E2 is present, as a check on it. Vertical votes come
only from the central 50 % of the width, so a keystone does not bias the
angle. There are no votes on sloping classes (16, 68, 46, 29, 13, 34, 94),
receding ones (road 6, sidewalk 11, path 52, pier 140), the lines that follow
a slope (fence 32, railing 38, bannister 95, stairs 53, stairway 59), or the
cheap read's person and animal alpha. Standing alone it needs a peak of 3.5×
the median bin (2.5× today).

### 4.3 Deciding, and refusing

1. **Consistency.** Two readings agree when `|a − b| ≤ 3·√(σa² + σb²)`. On a
   conflict, E2 wins only when it is sea (26) and its row lies near the
   horizon line implied by the pitch from E3 or a maker note (within 2 % of
   the height, estimate). A lake or other water line, or a sea with no pitch
   to check it against, needs a second source that agrees; alone in a
   conflict it is refused. Any other conflict is refused: "The lines disagree
   — left alone".
2. **Camera bound.** Once verified, a Fujifilm reading of 0 means
   `|roll| < 1°`. If only E4 claims more than 1.5°, Auto refuses.
3. **Combine** the agreeing readings by inverse variance. If the best σ is
   above 0.5°, refuse.
4. **Already level:** less than 0.15° from the current value. "Already level".
5. **Deliberate:** more than 8° is a Dutch angle and is refused, with or
   without a sea: "Tilt of 12° looks deliberate — left alone". With a sea
   horizon it is offered instead in the plan card: "The sea is tilted 10° —
   level it?".
6. **Out of range:** beyond the slider's ±15° (`crop.rs:41`), refuse.
7. **The corners it costs.** Levelling is a crop: `commit_crop` runs
   `crop_inside`, which at 3:2 takes 5.0 % of the area at 1° and 9.6 % at 2°
   (*computed*). The crop's hard gates of §5.3 (face box, joints, the whole
   animal, the horizon near the edge) run on the inscribed rectangle after
   levelling. Level is applied only when they pass and the area lost is under
   5 % (about 1° at 3:2). Otherwise it is offered in the plan card, not
   applied: "Level 2.1° would cut the hand at the left edge — apply?". The row
   always says what is kept: "1.3° by the sea · keeps 94 %".
8. **Apply** to Straighten on the worker through `busy_in`, with
   `geometry_now` before and `carry_masks` after (`geometry.rs:45`, `:63`) so
   existing masks follow the new angle and rectangle.
9. **The keystone cannot be carried yet.** `geometry_now` stores the
   rectangle, angle and rotation but not the keystone, and `between_frames`
   takes one perspective and returns an affine map, so a keystone change (a
   projective one) cannot be carried; `apply_perspective` carries nothing.
   Until masks and spots have a projective remap (a homography in
   `Mask::remap` and for spots), Auto refuses Upright on a document with local
   masks or spots and says why: "Verticals not corrected — your masks would
   move". Retouch spots are not remapped by any geometry change (§5.3), so
   Level, like crop, is refused on a document with healed spots until they
   are: "No level: healed spots would move".

The toast names the source: "Levelled 1.3° by the sea — the slider is yours to
change", "by the verticals", later "by the camera's level". Portraits and
close-ups with neither geometry nor a camera reading are refused, and say so.

### 4.4 The camera's level, verified before it is trusted

Fujifilm writes `RollAngle` at maker-note tag 0x144d as a signed rational, and
`makernote_tag` already decodes kind 10 (`raw.rs:1527`, `:1550-1554`). Nothing
reads it today. The FocusPoints compatibility notes name the X-T5 and later as
the bodies that write it (the photographer's X-T5 is covered), confirm that
anything within ±1° is stored as 0, and warn that the recorded roll "does not
always seem to correspond reasonably to the actual tilt angle". The sign
convention after the orientation flip is unverified. **Before E1 is used even
as a bound**, the photographer shoots test frames: a tripod and a levelled
head, rolls of 0, ±0.5, ±1, ±2, ±5 and ±10°, in landscape and both portrait
orientations, on each body in use. About 33 frames a body, read back in a unit
test.

**Other brands need a maker-note reader of their own.** The vendored rawler
0.8.0 parses none of their level tags (its only level entry is a Pentax-internal
`LevelInfo = 0x022b`, `vendor/rawler/src/decoders/pef.rs:501`), and Numa's
`makernote_tag` parses Fujifilm only (`raw.rs:1487-1488`). Each brand is a
reader like `makernote_tag`, in Numa or as a rawler patch, with the same test
frames, per ExifTool:

| Brand | Tag | Encoding | Caveat |
|---|---|---|---|
| Canon | `LevelInfo` 0x4059 | int32 / 10, negated, wrapping at 1800 | recent bodies only: R5 from firmware 1.5, R5 II, R6 II, R6 III, R7, R8, R10 |
| OM System | 0x0903 roll, 0x0904 pitch, in the CameraSettings sub-IFD 0x2020 | 0.1° | "n/a" when the level gauge was off |
| Panasonic | 0x90 roll, 0x91 pitch | int16 / 10 | |
| Nikon | OrientationInfo `RollAngle`, `PitchAngle` | degrees | D5, D500 and later |
| Pentax | `LevelInfo` | 0.5° steps | K-7 and later |
| Ricoh | GR III level | — | to read in Pentax.pm |
| Apple (iPhone) | maker-note `AccelerationVector` | a gravity vector | Apple build only, outside the parity run |

Whenever E2 and E1 disagree, the gap is logged per body serial. Nothing is
corrected from that log until it is measured.

### 4.5 Keeping the corners

After levelling, `crop_inside` (`image.rs:370`) keeps the crop on the
photograph, at the cost in §4.3 step 7. Auto keeps the aspect. `fit`
(`image.rs:377`) first searches for the largest zoom at which any feasible
centre exists (`image.rs:416-429`), and only then takes the centre nearest the
wanted one at that zoom (`:431-447`). At that zoom the feasible set shrinks to
a point along the binding axis (the vertical one for 3:2 at 1–2°), so
`crop_inside` keeps the largest rectangle and moves it only within its small
leftover slack; it cannot centre the crop on the subject. Spending the wedge
where it hurts least needs a *new* call that trades area for position, a zoom
a little below the maximum, bounded by the 5 % rule of §4.3. It is part of
P1's level work.

**Corner fill is not part of Auto.** Both judges cut it. It invents pixels in
nature and documentary frames, where competition rules forbid that. Remove's
LaMa path makes disc holes and caches each fill per render region
(`crates/numa-render/src/remove.rs`), so wedges along a whole edge would be
re-filled per zoom or tile, with seams and a preview that differs from the
export. LaMa at proxy scale will not match a 40 MP grain, and the frame edge is
its weakest case. What it saves at 1–2° is small. If it is ever built it is
its own opt-in action, never a step of Auto: full-resolution tiles with a grain
match, a visible "Filled corners" row, and IPTC DigitalSourceType written on
export.

---

## 5. Crop

### 5.1 Principles

The default is no crop. A crop is offered and never applied unseen. At most,
once the photographer has picked the top proposal at least 60 % of the time
in logged local use, proposal 1 is pre-highlighted in the chooser; it is still
not applied. "As shot" is always candidate 0 and must be beaten. Every
proposal says why. This follows Twitter's lesson: its saliency crop showed a
demographic skew and was withdrawn on the conclusion that how to crop "is a
decision best made by people".

### 5.2 Candidates

- **Frame:** the levelled canonical frame; every candidate is tested with
  `crop_fits` (`image.rs:360`) at the current angle and keystone, and shrunk
  to validity with `crop_inside`.
- **Aspects:** the original; 5:4 or 4:5 in the same orientation; 1:1 only when
  the subject is compact (box aspect 0.7–1.4); 16:9 only for seascapes and
  landscapes with a horizon. If the photographer has locked a preset
  (`state.crop.ratio`), only that one.
- **One change of orientation:** when the subject is upright and tall (box
  height at least 1.6× its width) in a landscape frame, or the reverse, one
  card offers the other orientation: the standing heron cut as a portrait from
  a landscape frame. This card, the birder's, ships first, in P2, behind the
  gates of §5.3, because it is where a birder will say "I did not expect
  this".
- **Scale: a floor in megapixels, not in area.** The proposals' area floors of
  0.45–0.5 forbid the crop a birder actually makes when the bird fills 3 %
  of a 40 MP frame. The floor is an output of at least 8 MP (a preference,
  8–12): on a 40 MP X-T5 frame that allows a crop of 0.2 of the area, on a
  24 MP frame 0.33. Scales step down from the largest valid rectangle by 4 %
  of area to that floor.
- **Positions:** an 11 × 11 grid over the free travel, then ±1 % around
  the best. Up to about 10,000 candidates on a 40 MP frame, each scored in
  constant time on 256 px integral images of the subject alpha, face boxes,
  intrusions and the bright mass.

### 5.3 Hard gates

A candidate is rejected when it:

1. **cuts an animal at all.** An animal is kept whole: the matte dilated by
   1.5 % of the short side and OR'ed with the YOLOX box of its named class.
   The dilation and the box are there because a matte drops exactly the thin
   parts a crop must not cut (legs, wingtips, a tail; float16 BiRefNet lost a
   palm frond at IoU 0.26, `ENGINEERING.md:5084`).
2. **cuts a person at a joint.** A crop through a person is allowed, so a
   waist crop or a head-and-shoulders from a full-length frame can be
   offered, but never at the neck, elbow, wrist, knee or ankle. The joints are
   placed from the face height and the person box with standard figure
   proportions (bands are estimates, calibrated on the photographer's own
   crops); no pose model is needed. Mass lost is allowed for people.
3. cuts a face box, or leaves less than 0.3 face heights above one;
4. puts the horizon within 3 % of the top or bottom;
5. cuts deeper into a person than the original frame did.

The earlier "thin run" test (a crossing narrower than a quarter of the
subject's width is a limb) is dropped: it rejected a standard waist crop
whenever the arms hang apart from the torso, it trusted a matte that drops
thin structures, and with a 2 % mass gate beside it a torso cut could never
pass anyway.

**Retouch spots are not remapped when the geometry changes** (`carry_masks`
remaps masks only, `geometry.rs:63-100`). Until spots remap through
`between_frames`, Auto offers no crop, no level and no Upright on a document
with spots, and says so.

### 5.4 Score

`S = Σ wᵢ · termᵢ`, each term in [0, 1] and logged per candidate. The weights
are starting values, calibrated on the photographer's own crops; a term with
no input spreads its weight over the others.

| Term | w | Rule |
|---|---|---|
| E, edges | 0.20 | 1 − Σ salience × (1 if the crop cuts the intrusion, 0.3 if it keeps it whole). Intrusions: YOLOX boxes touching the border and not overlapping the subject; clutter classes car 20, signboard 43, boat 76, streetlight 87, pole 93, poster 100, ashcan 138, flag 149 touching the border; bright blobs (max plane above 0.92) in the outer 5 % band, weighted ×3 |
| B, balance | 0.15 | exp(−d²/2σ²), σ = 0.15, d the distance of the centre of mass (2× subject plus bright mass) from the crop centre after shifting it by the look-room offset |
| L, look room | 0.15 | r = space in front / space behind; `L = exp(−ln²(r/2) / 0.5)`. The 2:1 ratio is a starting value; the literature ratio is unverified. With no facing (§3.4 step 8) the term is dropped. Mirroring the photograph mirrors the answer (a unit test) |
| H, horizon | 0.15 | Near the upper third: 1.0 (σ 0.05 of the height). The lower third instead when the sky carries at least 1.5× the texture and colour variance of the ground: sunsets. Centred: 0.6, or 1.0 when the water mirrors the sky (correlation of the flipped bands at least 0.6). Svobodova et al. (2014, *unverified*) are reported to have found the lower third rated lowest on average, which is why it is the exception, not a flat penalty |
| A, area | 0.15 | √(area fraction), with the megapixel floor above as the hard limit |
| P, placement | 0.10 | max(thirds, c × centre), Gaussian σ 0.07 of the diagonal. c = 1 for a frontal face (\|yaw\| < 0.1) or a mirror-symmetric frame (64 px luma mirror correlation above 0.8), else 0.8. For portraits the eye line aims at 0.30–0.40 of the height from the top. Low weight: thirds predicts preference weakly (Amirshahi et al.) |
| M, margins | 0.10 | smoothstep(0.02, 0.06, subject margin / short side), only on sides the subject did not already touch |

### 5.5 Proposals

- The best must beat "as shot" by τ = 0.08. Otherwise the toast says "Nothing
  in the frame asks for a crop", and the proposals are still offered as ideas.
- Non-maximum suppression drops a candidate whose IoU with a better one
  exceeds 0.7. Three are kept, with at least two aspects.
- **Each proposal carries its own tone.** The exposure ramp, the Radials and
  the sky hold are re-measured inside that rectangle from the planes, so
  cropping out a blown sign changes the light with it.
- **Each says why**, built from its two largest weighted gains over "as shot":
  "4:5 · bird on the left third, room to look right"; "16:9 · horizon on the
  upper third, drops the half cyclist at the right edge"; "Portrait 4:5 · the
  standing bird, 9.6 MP".
- **On the canvas:** three dashed, numbered rectangles on the overlay that
  carries the evidence (§6.3), placed with `content_rect`
  (`src/ui/window/overlays.rs:413`) in the style of `draw_crop`
  (`overlays.rs:496`), and an `.osd` chooser at the bottom of the viewport
  ("As shot · 1 · 2 · 3", the `build_drawing` pattern,
  `src/ui/window/mask_toolbar.rs:465`). **The chooser is a focusable toggle
  group**: the arrow keys move between choices and Enter commits; Tab keeps its
  GTK meaning of moving focus; the digits stay ratings; Esc keeps "as shot".
  Each proposal is also a row in the plan card, so it is reachable through
  AT-SPI. Hovering a choice renders that crop with its tone after 160 ms (the
  `HOVER_REST` pattern, `src/ui/window/presets.rs:200`); committing goes
  through `commit_crop` (`geometry.rs:370`) and `carry_masks`. Thumbnail cards
  through a third `presets::Kind` are deferred: the rectangles carry the idea
  for a fraction of the work.
- **Learned scorers:** none in either build's defaults. AVA (from
  DPChallenge), AADB and GAICD (from Flickr) carry no image licence, so GAIC,
  CACNet and SAMP-Net are out. Cropper (CVPR 2025) is training-free, but it
  needs a large vision-language model and in-context exemplars drawn from
  those same sets. Venus (CVPR 2026) trains on its own AesGuide set under a
  signed release agreement plus post-processed open datasets on a Qwen-VL
  backbone, and its repository states no licence. GAIC as an optional Linux
  download is cut as a model to maintain for a tie-breaker. On Apple,
  `CalculateImageAestheticsScoresRequest` breaks ties between proposals within
  0.02 of each other (estimate), once its sensitivity to crop differences is
  measured.

---

## 6. The wow

### 6.1 Where it comes from

The research is consistent about what earned "I did not expect this" in
2023–2026: the tool understood the content (Pixel's Auto frame, which reframes,
straightens and fills; Apple's Clean Up, reported to find shadows with their
objects, *unverified*), it showed what it had found (Clean Up's glow), it
offered choices (Auto frame, Pixelmator's hover previews), and it was personal
(Imagen's profiles). Auto buttons disappoint where they guess without
understanding: Lightroom's Auto Tone brightening raws, and reviews of
Luminar's horizon tool ignoring an obvious horizon (*unverified*). Exposure
alone can no longer delight; it has become an expectation (Kano). Showing the
work honestly raises its value (Buell and Norton), and being able to change an
algorithm's answer is what keeps people using it after it errs (Dietvorst et
al. 2018).

So the wow is not a bigger change, and it is not a show around a quarter-stop
move either. When the result looks like "a slightly brighter bird", a long
reveal around it backfires: the labour illusion works only when the labour
produced something. For this photographer "I did not expect this" lives in
what Numa already measures and nobody else connects: **the sharpest frame of
the burst on the bird, offered before they edit the wrong one; the standing
heron cut as a 9.6 MP portrait from a landscape frame; the sea levelled when
the pier is not; the bird found through their own focus point (Fujifilm,
single-point AF); and a plain no when no is right.** The printer's marks on a
work print are the way to show that, but they are built only after a blind
test shows that people keep what Auto found (§6.3).

### 6.2 The gesture

One Auto: the button on the Light page and a *new* key, **A**, which is free
in `editor_key` (`src/ui/window/rating.rs:236`) and goes into
`shortcuts_dialog` (`src/ui/window/preferences.rs:396`) as well, because the
shortcut list is the code that reads the keys (UX-013). Straighten › Auto and
Perspective › Auto stay as precision tools that read the same `Scene`, so
their answers are identical. On Apple, a toolbar button and the touch design
of §2.

### 6.3 Pressing it, moment by moment

**What P1 ships is the minimal reveal:** the evidence line along the horizon,
the subject's outline, and the plan card. No crossfade, no marks on the
picture. The marks and the crossfade are built only after the P2 blind test
asks "did it do something you would not have thought of, and did you keep
it?" and the answer is yes on at least 30 % of pairs. The table shows the
full version, with the parts that wait marked.

With the cheap read cached and the matte starting on the press, on the card:

| t | Canvas | Panel |
|---|---|---|
| 0 ms | The frame on screen (`render.on_screen`, `src/ui/window/render_loop.rs:463`) is kept as "Before Auto". BiRefNet starts on the worker | No spinner: `Waiting` shows nothing before 400 ms (`BUSY_AFTER`) |
| 0–300 ms | A line draws itself along the sea, labelled "1.3° · sea", through `guide_to_widget` (`overlays.rs:299`). The picture turns to meet it as a view transform of the displayed texture; the geometry is committed once, at the end, and no draft render starts until the matte has returned, so the card is not shared with BiRefNet's 8 GB | Straighten shows −1.3 |
| 300–450 ms | Eye dots and an eye line on faces; a small square on the AF point if it chose the subject | |
| about 600 ms or later, when the matte arrives (566 ms warm on the card, `ENGINEERING.md:5079`) | The subject's outline traces itself (`mask::outline`, `crates/numa-core/src/mask.rs:2008`, and `draw_ants`, `src/ui/window/mask_overlay.rs:510`), with a pill "Bird" in the face-name style (`build_face_names_overlay`, `overlays.rs:3`) | |
| the next 400 ms (after the blind test) | The printer's marks: "+¼" circled on the bird, "−⅓ held" across the sky. The photograph crossfades from before to after, on the `slide_away` pattern (`src/ui/window/loupe.rs:288`) | The touched rows flash with a *new* `auto-flash` class copied from `.point-section.point-flash` (`src/ui/style.css:637`). The named masks appear in the list, none selected |
| about 1–1.5 s | The marks fade over 500 ms. If crops are offered, the three dashed rectangles and the chooser appear. The plan card opens at the top right | |
| end | Toast: "Auto: levelled, light set — see the plan" | |

The evidence and the marks are drawn on a *new* non-targetable `DrawingArea`
in `build_canvas_overlay` (`src/ui/window/editor_page.rs:40`). A `DrawingArea`
is invisible to AT-SPI, so **every mark is also a row in the plan card**,
reachable from the keyboard and by a screen reader; clicking a mark on the
canvas selects the same thing as its row (the "+¼" its mask, the horizon label
Straighten).

**Other paths.** Without a card the evidence arrives as the models answer: the
horizon at once from the cached read, the subject after BiRefNet's 3.3 s on
the processor, so the wait becomes the explanation. Any key or click jumps to
the end state. With `gtk-enable-animations` off the end state and the evidence
show still for 1.5 s. The full reveal plays once per photograph at most, and
after the first three presses in a session only the evidence lines show;
"Show what Auto saw" in the plan card replays it.

**Pressing again** on the same photograph, on the same machine with the same
model files, gives the identical plan and skips the reveal: "Auto's answer
stands".

**While the plan card is open, Shift+Space shows "before Auto".** Space keeps
its as-shot meaning (`hold_for_before`, `rating.rs:193`), so muscle memory is
not overturned. "Before Auto" is a cached document rendered the way
`show_baseline` renders. "Compare" needs a *new* variant of `set_reference`:
today `set_reference(state)` takes no frame and re-renders the open document
through `with_masks_resolved` and `apply_stack`
(`src/ui/window/reference.rs:90-118`), which after Auto is the after state.
The variant takes the kept "Before Auto" frame and sets
`state.reference.texture` directly.

### 6.4 The plan card

An `.osd` card, one row per part, each with a switch, and refusals that say
why in plain words. Numbers in display units go in the tooltip.

```
Level      1.3° by the sea · keeps 94 %                   [on]
Light      on the bird +¼                                 [on]
Around     −⅓ around the bird                             [off]
Sky        held −⅓                                        [on]
Exposure   already right: the brightest highlights sit just below white
Contrast   left: a matter of taste
Crop       three ideas on the photograph, or as shot
```

Turning a part on or off re-plans the rest, because level feeds the crop and
the crop feeds the tone; the parts cannot be toggled in any order
independently. A re-plan never overwrites a value or a mask the photographer
has touched (§3.8). Each switch is its own history step. Undo has three named
steps through `push_named` (`crates/numa-io/src/history.rs:148`): "Auto ·
level", "Auto · light", and "Auto · crop" when a proposal is chosen; Ctrl+Z
peels them off in reverse order. Each part can also be taken back by hand: a
double-click resets Straighten, "As shot" undoes a crop, each mask row has its
eye and its delete.

### 6.5 What makes people say "I did not expect this", ranked

Ranked by surprise for the risk taken, with the phase it ships in.

| # | Idea | What the photographer sees | Risk | Phase |
|---|---|---|---|---|
| 1 | The sharpest frame of the burst | "Frame 5 of this burst is the sharpest on the bird — use it?", from CULL-001's sharpness measured on the subject's box and CULL-002's bursts. For faces, closed eyes are a question, never a decision: CULL-003's eye classifier is right about a closed eye about three times in ten (`docs/FEATURES.md:1219-1225`) | Low: it only offers, from shipped models | P1 |
| 2 | The birder's portrait | "Portrait 4:5 · the standing heron, 9.6 MP", cut from a landscape frame | Medium, contained by the gates and never applying | P2 |
| 3 | Levelled by meaning | "1.3° · sea" laid along the horizon; the pier and the sloping shore ignored; "Tilt 12° looks deliberate — left alone" | Low | P1 |
| 4 | It says no, in plain words | "Exposure already right", "−1 EV dialled in — kept low-key", "Silhouette kept", "Two subjects — light left even" | None | P1–P2 |
| 5 | It found the subject and lit it | The outline traces the bird under the AF point, not the reeds; "+¼" on it, no hard edge anywhere. The AF square only on Fujifilm single-point AF; the hit rate is measured first (§3.4) | Low with the admission gate, the stored choice and the halo checks | P2 |
| 6 | Crops that explain themselves | Three rectangles with reasons; look room flips when the photograph is mirrored | Medium, contained by gates and never applying | P3 |
| 7 | One look for a burst | Auto on one of eight heron frames offers "The same light for the 8 frames of this burst"; the eight look the same | Low | P4 |
| 8 | Fall-off by distance | The background falls away by depth like real light, not by a cutout (Depth Anything V2 Small) | Medium: halos at the skyline | P4, experiment |
| 9 | Your camera knew | "Levelled 2.1° by the camera's level" in fog, at night, on a bird against sky | Low once verified | after §4.4's test frames |
| 10 | It edits like you | "Like 7 of your backlit portraits" | Medium | P5 |

**Bursts.** `numa_cull::bursts` (`crates/numa-cull/src/lib.rs:80`) already
groups frames by hash and time. Auto on one frame of a burst offers to carry
the same answer to the others. **What is shared is the targets, not the slider
values:** the top end, the subject's key, the sky relative to the subject.
RENDER-008's base differs per frame when the camera's metering moved between
frames, so the same slider would not give the same look; each frame is solved
to the shared targets, and the gate is the spread of the output in display
space, at most 0.05 EV. Roll is shared only across frames that register to
within 1 px (a tripod); a handheld burst rolls ±0.5–1° from frame to frame,
so each frame is levelled on its own evidence or refused. Crops are proposed
in subject-relative coordinates so the subject sits in the same place.

**Auto on a selection** goes in the library's photo menu (`photo_menu_model`,
`src/ui/window/library_page.rs:211`). **In v1 it writes global tone and level
only, with no subject masks**: a subject mask nobody has looked at removes the
main containment of §11, the outline drawn before the light, and on a
processor 24 photographs would cost about 80 s of BiRefNet. Subject light on
many photographs comes later as a contact sheet of outlines to confirm. No
reveal and no crop, on the `analyse_pending` loop
(`src/ui/window/culling.rs:35`) with its progress toast, ending "Auto on 24
photographs: 19 changed, 5 left alone". It is not Copy Settings: `EditParts`
has no masks (`document.rs:631`).

**Deferred** until P1–P2's refusal rates and blind tests show where the gap
is: the face light (CAPE's fix for a side-lit face, a Radial toward the dark
side), the edge-distraction chip ("Half a cyclist at the right edge — Crop
past it · Remove · Keep"), GeoCalib, a per-body level bias, and the
attention-share measure in the harness.

**Not doing:** corner fill as part of Auto; diffusion relighting (IC-Light
regenerates the subject and carries the SD 1.5 licence chain); sky
replacement; a crop, a level beyond the gates of §4.3, or a removal applied
unseen; subject masks laid by batch Auto in v1; names from SFace in Auto's
toasts; an attention heatmap in the product (no licence-clean fixation model;
MSI-Net stays in the harness).

---

## 7. Edits like you (P5, later)

Both judges kept this for later, after the rules have been measured and their
refusal rates are known. The research is consistent about how to do it: learn
the photographer's **targets**, not their sliders, and learn the difference
from a sensible default.

- **What is learned.** For every photograph the photographer finished,
  re-render the final edit at thumbnail size and measure the achieved targets:
  top end, black point, subject median, sky relative to subject, the
  separation, crop aspect and tightness, subject placement, horizon row.
  Sliders are redundant (Contrast, Whites and the curve reach the same place
  in several ways); targets port unchanged to the Apple build and survive a
  process change. Auto's existing solvers still turn targets into sliders.
- **Not learning Auto from Auto.** An edit made after pressing Auto is
  anchored to Auto's answer. Edits that stay within tolerance of what Auto
  proposed are excluded (or strongly down-weighted), or the learner would
  converge on Auto's own defaults.
- **Features**, all measured on the neutral rendering, never on the edit:
  log-luminance percentiles, clipped fraction, colourfulness, as-shot white
  balance, exposure time, ISO, focal length, the EfficientViT class shares
  compressed to about 20 groups with the median luminance of each.
- **Model tiers**, each falling back to the one below: under 30 finished
  photographs nothing personal (the same `MIN_RATED = 30` as
  `crates/numa-cull/src/learn.rs`); then global offsets where the photographer
  is consistent; then kernel regression over the history with shoot-size
  weighting (a 900-frame synced wedding counts as one voice) and
  leave-one-shoot-out tuning. An embedding (DINOv2 ViT-S/14, Apache-2.0, as an
  optional Linux download; Vision FeaturePrint on Apple) is added only if it
  measurably helps.
- **Abstention.** A target is personalised only when the effective neighbour
  count is at least 3 and their spread is within tolerance (0.2 EV, for
  example); a photograph unlike anything in the history gets the rule and says
  "new kind of photograph for you: standard Auto".
- **Adoption** follows `learn.rs`: at least 30 examples, a held-out check,
  adopted only when it beats the rule, and the UI says which is answering.
- **Crops stay three ideas.** One of the three crop proposals always stays
  outside the personal mode, so the proposals do not collapse into one habit.
- **Cold start.** `crates/numa-io/src/foreign.rs` already parses Lightroom
  `crs:` values, but as a preset translator: `is_foreign` accepts
  `.lrtemplate`, `.xmp` and `.costyle` files (`foreign.rs:28-30`) and
  `translate` returns a `Preset` (`:11-15`, `:36`), used by preset import
  (`crates/numa-io/src/presets.rs:256`, `:291`) and DNG export
  (`export.rs:376`). Per-photograph sidecar import is new work: find the
  `<raw>.xmp` beside each file, match it to the photo id, and read crop and
  angle (`crs:Crop*` and `CropAngle`, not read today). And Lightroom's
  Exposure2012 and the rest live in Adobe's rendering, which Numa cannot
  re-render, so the slider values themselves are not targets. The cold start
  measures targets on Lightroom-rendered exports or previews matched to the
  raws, and takes only crop and angle from the sidecars.
- **What the photographer sees:** "Like 7 of your backlit portraits" in the
  plan card, with the neighbours on hover, and a visible **"Forget my style"**
  that resets the learner.

---

## 8. Models and licences

| Model | Use in Auto | Weights licence | Training data | Size | Apple | Linux |
|---|---|---|---|---|---|---|
| EfficientViT-Seg B2, ADE20K 1024 | classes, the sea horizon, clutter, the sky | Apache-2.0 | ADE20K, ImageNet-pretrained backbone; research terms: **flag to counsel** | 61 MB | after counsel (existing decision, flagged); fallback Vision classification for sky and water presence, and no sea fit | yes |
| BiRefNet / BiRefNet lite 512 | the subject matte, on the press | MIT | DIS5K-TR, DIS-TEs, DUTS, HRSOD, UHRSD, HRS10K, P3M-10k, TR-humans (the model zoo lists the same sets for the general and lite weights). DIS5K's terms forbid commercial use "even after copying, editing, processing": **flag to counsel** | 973 MB / 192 MB | lite on iPhone and iPad (`target_os = "ios"`), full on a native Mac build; after counsel; fallback `VNGenerateForegroundInstanceMaskRequest` | full |
| YuNet 2023mar | faces, eye line, yaw, sclera, crop face gates | MIT | WIDER FACE, CC BY-NC-ND 4.0: **flag to counsel** | 0.2 MB | after counsel; fallback `VNDetectFaceLandmarksRequest` | yes |
| PP-ResNet50 | naming the animal | Apache-2.0 | ImageNet-1k, non-commercial access terms: **flag to counsel** | 103 MB | after counsel (existing decision, flagged); fallback Vision classification | yes |
| YOLOX-s | edge intrusions and naming for admission, all 80 COCO classes | Apache-2.0 | COCO: Flickr images under mixed licences, the same question: **flag to counsel** | 36 MB | after counsel (existing decision); fallback Vision person and animal requests | yes |
| Depth Anything V2 Small (P4, already planned) | subject check, fall-off by distance | Apache-2.0 | synthetic teacher; 62 M pseudo-labelled real images including research-only sets (BDD100K, Places365; VKITTI 2 is CC BY-NC-SA): the "low residual risk" class of LaMa | about 50 MB fp16 | yes | yes |
| Apple Vision: `VNDetectHorizonRequest`, `VNDetectFaceLandmarksRequest`, `VNGenerateForegroundInstanceMaskRequest`, person segmentation, image classification, `CalculateImageAestheticsScoresRequest`, FeaturePrint; Core Image `autoAdjustmentFilters` | second opinion on level, the fallbacks above, crop tie-breaker, "like you" embedding, a baseline | the OS | Apple's | 0 | yes | n/a |
| DINOv2 ViT-S/14 (P5, optional) | "like you" embedding | Apache-2.0 | LVD-142M, an undisclosed web crawl | about 84 MB fp32 | no (FeaturePrint instead) | optional download |
| GeoCalib (only if refusals on horizonless frames exceed 30 %) | roll and pitch with a covariance | Apache-2.0 code, CC BY 4.0 weights | OpenPano: PolyHaven (CC0), HDRMaps, Laval Indoor HDR (non-commercial; the README thanks its authors "for allowing" the release): **flag to counsel** | about 116 MB checkpoint | after counsel, with attribution | optional |
| MSI-Net | harness only: attention share before and after | MIT code | SALICON (annotations CC BY 4.0) on COCO images | about 25 M parameters | not shipped | not shipped |
| IS-Net | not used: deleted once BiRefNet loads, not a download | Apache-2.0 | DIS5K | 178 MB | no (`ISNET_ALLOWED`) | no |
| LaMa | not in Auto (Remove only) | Apache-2.0 | Places2, research-only, accepted as low residual risk | 208 MB | — | — |
| SFace | not in Auto | Apache-2.0 | research-only | 37 MB | no | not in Auto |
| ViTMatte-S | not in Auto; replaced by BiRefNet lite's matting weights on 26 September (`docs/FEATURES.md:3390`) | non-commercial | Adobe's composition set | 104 MB | no | no |
| GAIC, CACNet, SAMP-Net | cut | MIT code (GAIC) | AVA (DPChallenge), AADB and GAICD (Flickr): no image licence | — | no | no |
| Cropper, Venus and the 2026 VLM croppers | cut | Cropper: training-free on a large VLM; Venus: no licence stated | Cropper's exemplars come from the sets above; Venus: AesGuide under a signed release agreement | large | no | no |
| Perspective Fields | no | Adobe Research Licence: non-commercial, no redistribution | 360cities, Street View | — | no | no |
| RTMPose | not needed: the joints are placed from the face and the person box (§5.3); considered only if those bands prove too crude | Apache-2.0 | COCO | 3.3 M parameters | no | no |

**Counsel is a gate before P1 for the Apple build.** Every model the Apple
Auto would rest on has training data that fails or may fail the rule:
BiRefNet (DIS5K), YuNet (WIDER FACE), EfficientViT (ADE20K), PP-ResNet
(ImageNet) and YOLOX (COCO). Counsel may record that the existing "low residual
risk" acceptance that covers LaMa's Places2 covers these uses too; if not, the
fallback is already written: on Apple the read takes its faces from
`VNDetectFaceLandmarksRequest`, its subject from
`VNGenerateForegroundInstanceMaskRequest` and person segmentation, sky and
water presence and the animal's name from Vision's image classification, and
the horizon from `VNDetectHorizonRequest`, none of which carries a
training-data question for the app. The sea fit and the clutter classes then
have no Apple source; Apple's level falls back to E3, E4 and Vision's horizon
until counsel answers. On Linux the models stay as they are offered today;
whether the Downloads page should flag BiRefNet and YuNet's training data is
the same question to counsel.

P0 to P3 add no new weights. P4 adds Depth Anything V2 Small, which the depth
plan adds anyway. Upright stays roll plus a vertical keystone. Adobe's
energy-minimising homography (Lee et al. 2012) is reported to be covered by
US9582855B2 and US9519954 (*unverified*: their status and claims are to be
checked before this is treated as a legal limit rather than a choice). Full
homography is left out in any case, because masks cannot follow a projective
change yet (§4.3 step 9).

---

## 9. How it will be measured

### 9.1 The frames

**The corpus is its own task, 5–8 days, with consent.** Raw.pixls.us has
almost no portraits, so the eight portraits across skin tones need consent from
the people in them, and every Auto-60 frame needs a hand-made twin document.

**Auto-60, frozen before any tuning,** from the photographer's own RAFs, with
raw.pixls.us frames from `raws-cc0` where a stratum is short. Twenty of the
sixty are held out and never used for tuning. Every frame also exists as a twin
document with a crop and a straighten, and across the set quarter turns,
mirrors and keystones: the regression test for bug 1.

| Stratum | n | What must happen |
|---|---|---|
| Backlit or dark subject | 8 | Lifted, no halo; backlit subjects at least as bright as today's Auto leaves them |
| Portraits across skin tones (consented, or CC0) | 8 | Per-face spread within 0.15 EV of the photographer's own edits |
| Sea or lake horizon, including calm water mirroring the sky | 8 | Levelled |
| Architecture | 6 | Upright and level |
| High key, snow, white plumage | 6 | Not greyed; no subject highlight newly clipped |
| Fog, haze, flat light | 6 | Brighter, display IQR changed by at most 5 % |
| Daylight ND long exposures | 6 | Treated as daylight, not night |
| iPhone HEIC and ProRAW (Apple build) | 10 | Not lifted twice; the `finished` limits hold until shown otherwise |
| Finished JPEGs | 2 | The `finished` limits hold |

**Refusal strata, 30 frames each, from the photographer's library.** Their
labels are cheap ("must not move"), and 0 failures in 6 bounds the true failure
rate only below about 39 % at 95 % confidence, where 0 in 30 bounds it below
about 10 %.

| Refusal stratum | n | What must happen |
|---|---|---|
| **Sloping land** (hills, dunes, shores, fences and terraces along a slope) | 30 | **Not levelled** |
| **Night or low-key**, including frames shot at −1 EV on purpose and frames from a body whose habit is −⅓ | 30 | **Never brightened** (the habit frames may be) |
| **Already good** | 30 | **Left alone** |
| **Groups and couples** (two or more people or animals, street frames with many faces) | 30 | **No single member emphasised** |

Beside them:

- **FT-025's thirty frames**, the subject-choice regression: a wrong subject
  lifted is a hard failure.
- **ADJ-001's table frames**, for the backlight rescue against today's values.
- **The photographer's last 200 wildlife frames**, for the hit rate of the
  admission gate (§3.4).
- **Three bursts of eight** for consistency, and one tripod series for level.
- **Synthetic rotations** of level frames by ±0.3, 0.5, 1, 2, 4 and 6°, **each
  cropped to its inscribed rectangle before measuring**: otherwise the corner
  wedges, whose edges vote at exactly the rotation angle, inflate the
  accuracy. Synthetic roll-plus-pitch renders for E3, and a set of deliberate
  tilts.
- **The Fujifilm roll test frames** of §4.4.
- **The photographer's finished edits** (`Catalog::load_edits`,
  `catalog.rs:1827`) as "what you did".

### 9.2 Harnesses

A *new* ignored test, `auto_scorecard`, built on the `run()` helper
(`auto.rs:621`) once it is reworked. Today `run()` takes a different path from
the app: it resolves a plain `Shape::Subject` probe with the matte on
(`auto.rs:632-635`), which goes straight to `the_subject` and `matte::subject`
(`lib.rs:340-345`) and skips the `named_something` gate in `subject_mask`
(`auto.rs:66-81`) that the app uses (`geometry.rs:236`); and it segments the
uncropped frame, so it cannot reproduce bug 1. Reworked, it calls
`auto::subject_mask` (and later the choice of §3.4) and takes a cropped,
straightened, turned, mirrored or keystoned `Document`, so the "wrong subject
0" gate is measured on the path the app takes.

Its input is `FRAMES_LIST`, its output one CSV row per frame and part (every
answer, every refusal, every reason) plus a before / after / "what Auto saw"
contact sheet. `what_auto_now_does` (`auto.rs:525`) prints the `Scene`;
`auto_before_and_after` (`auto.rs:481`) also writes the evidence.
`auto_against_your_edits` reads the catalog. Unit tests assert that two presses
give the same document hash on five frames, that mirroring a frame mirrors its
crop, and that Auto, then a crop, then an export keeps the subject at IoU 0.95
or more.

### 9.3 Metrics and gates

| Part | Metric | Gate |
|---|---|---|
| Level | Median / p90 error on synthetic rotations (cropped to the inscribed rectangle) and hand-marked horizons | ≤ 0.15° / ≤ 0.4°; sea ≤ 0.1° |
| Level | Wrong-direction corrections; deliberate tilts refused; false levels on level frames | 0; ≥ 90 %; ≤ 3 % |
| Level | **Sloping land levelled**; sea frames answered | **0 of 30**; ≥ 7 of 8 |
| Level | Levels applied whose inscribed rectangle fails a crop gate or loses 5 % or more | 0 |
| Level | Refusal rate per lens | reported; a lens lensfun does not correct shows up here |
| Light | **Night frames brightened more than 0.2 EV**; already-good frames moved more than 0.1 EV | **0 of 30**; ≤ 2 of 30 |
| Light | Order flips; new clipped subject pixels; halo band rise on every Auto mask | 0; 0; ≤ 25 % |
| Light | Display range p1–p99 after against before; subject-to-sky separation lost | ≥ 0.85×; ≤ 30 % |
| Light | Ends landed in the verify render | within `CLOSE_ENOUGH` |
| Light | Wrong subject lifted, Auto-60 and FT-025's thirty; a single member emphasised in a group | **0**; **0 of 30** |
| Light | Measured mask against the mask exported after a crop | IoU ≥ 0.95 |
| Light | Backlit subject display against today's Auto, ADJ-001's frames | not darker; values published |
| Light | Admission gate hit rate on 200 wildlife frames | reported before P2 |
| Light | Fog frames: display IQR change | ≤ 5 % |
| Light | Per-face spread across skin tones at equal need | ≤ 0.15 EV |
| Light | Closer to the photographer's final edit than no Auto, per region in EV | ≥ 70 % of frames; further than as-shot ≤ 5 % |
| Crop | The photographer's own crop among the three (IoU ≥ 0.7) | ≥ 50 % |
| Crop | Faces cut, people cut at a joint, animals cut; "as shot" right on frames they left uncropped | 0; 0; 0; ≥ 70 % |
| Burst | Output spread in display space across a burst; level across a tripod series | ≤ 0.05 EV; ≤ 0.1° |
| Runtime | Press to first evidence with the read cached | ≤ 100 ms |
| Runtime | Cheap read, cold, on Linux and on an iPad's processor | set after EfficientViT is re-measured and the iPad is measured; energy reported per photograph |
| Parity | Apple against Linux on Auto-60, Apple-only evidence off | the same lift-or-not; the same subject at IoU ≥ 0.8; level within 0.2° |
| Determinism | Two presses on the same machine with the same model files | the same document hash |
| Surprise | "Did it do something you would not have thought of, and did you keep it?" | ≥ 30 % before the marks and the crossfade are built |

### 9.4 The blind test

Three judges, the photographer and two others, see pairs with sides randomised
and answer better, worse or the same, and for new Auto also the surprise
question of §9.3. **Arms:** the untouched camera-matched rendering, old Numa
Auto, new Numa Auto, Lightroom's Auto (with Upright Auto, and the Adaptive
Subject and Sky presets where they apply) and the camera's JPEG, because those
are what the photographer compares against every day. Lightroom does not run
on Linux, so its arm needs the photographer to supply Lightroom exports of the
frames; without them the arm is dropped. On Apple, Core Image's `.level` and
`.crop` auto-adjustments are a further arm for level and crop. The experiments
of §3.9, the keystone percentage of §4.2 and "Around" on against off are arms
of their own.

The judging is budgeted apart from the days in §10: about 240 pairs a phase,
roughly 1½–2 hours a judge a phase (estimate).

**Gate per phase:** the new Auto preferred to the old on at least 60 % of
decided pairs and worse on at most 10 %; the untouched frame preferred to it on
at most 15 %; no stratum loses more than 2 frames. Against Lightroom's Auto and
the camera JPEG the result is reported in P1, and from P2 the goal is preferred
or equal on at least half (a goal, not a gate, until the first run shows where
it stands).

### 9.5 In the app, locally

In the pattern of CULL-012's `decisions` table, and never leaving the catalog:

- **Did it respect the photograph:** per part, the share of presses where the
  photographer's next action on that photograph is Ctrl+Z, a reset of a slider
  Auto set, hiding or deleting an Auto mask, or "As shot". This is the
  sharpest harm signal: it must fall below old Auto's and stay under 10 %
  per part (estimate, set after the first month).
- Slider travel within 60 s after Auto, 30 % less than with old Auto.
- Which crop proposal was picked: the 60 % rule of §5.1, for the
  pre-highlight only.
- Whether the burst offer of §6.5 is taken.

---

## 10. Phases and effort

| Phase | Days | Contents | Gate before the next |
|---|---|---|---|
| **Corpus** | 5–8, alongside P0 | Auto-60 with consented portraits; the four refusal strata of 30; the twin documents (cropped, straightened, turned, mirrored, keystoned); the roll test frames (about 33 a body of the photographer's time); Lightroom exports if the photographer supplies them | Frozen before any tuning |
| **P0 Honest Auto** | 6, built 1 October | Measure tone on `geometry_only` of the shown framing (crop, angle, quarter turn, mirror, keystone); lift from `Basic::local()`; a global slider, HDR included, written only at its default or Auto's own last value (`AutoRecord`, carried by the history); the endpoints solved through the document's look, the exposure decided on the camera's rendering, ends the photographer shaped left to them; `Mask.auto` with `as_auto_left_it`, replaced where it stands (named "Subject · Auto" until P2 names them by what was found); do not select the mask; a toast when levelling succeeds; level and perspective on `busy_in`. `run()` on the application's path with framing, `FRAMES_LIST` and `OUT_CSV`; Blacks re-computed and held to its band. ADJ-001 and GEOM-002 corrected, GEOM-005 added | The baseline CSV exists (still to be run on the photographer's frames); the cropped, turned, mirrored and keystoned regressions pass; a merge's HDR 50 survives Auto; no existing test regresses |
| **Counsel** | — | The training-data questions of §8 for the Apple build | Before P1 on Apple; the Vision fallback if the answer is no |
| **P1 Level, the read and the minimal reveal** | about 20 | The scene read, its cache and the `auto_read` table, the new `Segmentation` accessors (4); E2 with the continuity refine, E3 as a vanishing-point fit, E4 verticals-only, fusion and refusals, Upright refused with masks or spots (4); the corner gates, the 5 % rule and the new re-centring call (2); the Fujifilm roll read and its test frames, as a bound only after they pass (1); `AutoRecord`, named undo steps, toasts with sources, the A key (2); the burst's sharpest frame offer (1.5); the minimal reveal: the horizon line as a view transform, the subject trace, the plan card with refusals and accessible rows, Shift+Space and Compare (4); harness and blind test (1.5) | Level gates met, sloping land 0 of 30, no level applied through a failing gate; blind gate |
| **P2 Subject light** | about 20 | Measure first: the admission gate's hit rate on 200 wildlife frames and the backlight values on ADJ-001's frames. Then subject choice with `matte::components`, YOLOX naming, the AF point in single-point mode only, the hiker fallback, groups, `Shape::SubjectAt` and the stored alpha (4); the ramp, intent from the file (−1 EV, habit, manual, the DR tags, monochrome from 0x1003), light-source exclusion, night read from the content, the haze read, Blacks per the P0 measurement (3.5); rescue in shadow and against the light, the sclera reference, highlight protection, the silhouette gate, large subjects (3.5); the Radials with "Around" off, and the sky hold (1.5); the verify loop: halos on every mask, the global budget, the ends landed, the resolved-mask IoU (2.5); the birder's portrait card behind the crop gates (2.5); plan-card switches (1); blind test with the surprise question (1.5) | **Blind gate**; night and already-good untouched (0 of 30); wrong subject 0; groups 0 of 30; backlight not darker than today |
| **P3 Crop proposals** | about 14 | Candidates and gates: the whole animal, the joints from the face, the megapixel floor (4); score, reasons, per-proposal tone (3); dashed rectangles, the toggle-group chooser, hover preview (3); spot remap or refusal (1); measurement (1); the printer's marks and the crossfade, only if the surprise question passed (2.5) | Top-3 ≥ 50 %, cuts 0; a pre-highlight only after a 60 % pick rate |
| **P4 Depth and many photographs** | 8 | Depth Anything V2 in Auto as subject check and the fall-off experiment (4, after the depth plan's 3–4 days); bursts with shared targets and Auto on a selection without subject masks (4) | Each beats its rule baseline on the held-out 20 |
| **P5 Like you** | about 10 | Targets, kernel regression, abstention, the Auto-anchored filter, per-photograph sidecar import (crop and angle), "Forget my style" | Beats the rule on leave-one-shoot-out |
| **Apple** | 10–15 | First, before P1: the read measured on an iPad's processor. Then SwiftUI for the plan card, the outline and the chooser; the touch design; FFI for the Vision fields; the Vision fallbacks if counsel says no (more days then) | Parity on decisions on Auto-60 |

**What ships first:** P0 as the next patch, because each of its bugs is
confirmed in code and each undermines the trust the rest depends on. Then P1,
which needs no new model and makes every later part legible: each new answer
only adds a row and a line of evidence, and the burst offer gives the first
real surprise from models Numa already ships. Level and light carry the largest
perceptual gain at the lowest risk; crop, the most personal answer, waits
until it meets its targets on the corpus, apart from the birder's card.

P0 to P2 is about 46 working days, plus the 5–8 days of the corpus. With the
judging sessions, the photographer's shooting and review time, and the
measurements that come before P2, that is **roughly 12–15 weeks**, plus **2–3
weeks** for the Apple interface. P0 to P5 is about 78 working days, about 85
with the corpus (estimate).

---

## 11. Risks and refusals

| Risk | Containment |
|---|---|
| A wrong subject lifted (the hiker and the lighthouse, the kitesurfer and the wave) | Named classes, YOLOX names and faces only; the IoU gate at 0.5 with the hiker fallback; the AF point chooses but never admits, and only in single-point AF; the outline is drawn before the light, so a wrong choice is visible; a hard gate of 0 on FT-025's thirty frames |
| The chosen subject lost at export (the recipe asks the matte again) | `Shape::SubjectAt` with the stored alpha; IoU ≥ 0.95 between the measured and the resolved mask, or the mask is dropped; the Auto → crop → export test |
| One member of a group lit and the rest burned | Close candidates or similar faces lit as a union, or emphasis refused; no emphasis above three faces; a group stratum of 30 with a gate of 0 |
| The headline fires on few frames | The admission gate's hit rate measured on 200 wildlife frames before P2; YOLOX names birds where ADE20K cannot |
| Halos, a cutout look, the HDR look rebuilt with masks | Radials with no hard edge, skipped over sky and water; the halo check on every Auto mask, the sky boundary included; a global range budget; "Around" off until it wins blind |
| A backlit subject given less than today | Backlight read on its own (rim against core, background against subject); the sclera for faces; measured against today's values before P2, and not shipped if darker |
| Skin-tone bias | No absolute skin or median target; the shadow lift measures the surroundings, the backlight lift the background or the sclera; a portrait stratum across skin tones with a 0.15 EV spread gate |
| A deliberate low-key frame "fixed" | Compensation of −1 EV or less read as intent, a body's habit and manual exposure ignored; the night read; "kept low-key" in the plan card |
| An ND long exposure taken for night | Night read from the content, not the shutter; an ND stratum |
| Fog given contrast by the Blacks solve | The haze read switches the Blacks solve off; the fog gate on the IQR |
| White plumage, snow or a dress blown | Subject highlights solved before any lift; no new clipped subject pixel as a check |
| A slope, a shore, a far waterline or a pier taken for the horizon | E2 needs sky directly above water over 40 % of the width with a small residual; only sea near the pitch-implied row wins a conflict, lake and water need a second source; E4 counts verticals only without E2 or E3; sloping, receding and slope-following classes never vote; 0 of 30 is a gate |
| A Dutch angle "corrected" | Refused above 8° always; with a sea it is offered, not applied |
| The roll biased by a keystone | E3 fits the vanishing point, not an intercept; tested on roll plus pitch |
| The Fujifilm roll's sign, dead band or bodies wrong; other brands unread | Unused until the test frames pass; a 0 is only ever a bound; each other brand needs its own reader and the same test |
| A horizon curved by an uncorrected lens | The fit runs after lens correction, frame ends down-weighted, refusal rates reported per lens |
| Levelling crops a hand, a foot or a wingtip unseen | The crop gates run on the levelled rectangle; applied only when they pass and under 5 % is lost, otherwise offered; "keeps 94 %" in the row |
| A crop that cuts a head, a limb or a wingtip | Animals kept whole with a dilated matte and the YOLOX box; people never cut at a joint; offered, never applied; "as shot" must be beaten by 0.08 |
| Crop feels presumptuous | Never applied; at most a pre-highlight after a 60 % pick rate; never ranks people by saliency |
| The photographer's decisions overwritten | `AutoRecord` on the global sliders (HDR included) and the geometry; a mask is Auto's only while `as_auto_left_it`; a re-plan never overwrites a touched value |
| Masks or spots misplaced by a geometry change | `geometry_now` and `carry_masks` around level and crop; Upright refused with masks or spots until a projective remap exists; no level, crop or Upright with spots until spots remap |
| A burst that flickers, or levelled wrongly | Targets shared, not sliders, with an output spread gate of 0.05 EV; roll shared only across frames that register to a pixel |
| Batch Auto lifts subjects nobody has seen | v1 batch writes global tone and level only |
| Matte cost on the processor, energy on the card, cost at export | The matte only on the press or a dwell; evidence progressive; the stored alpha at export; the cheap read's energy reported; no IS-Net assumption |
| A float16 BiRefNet still on disk, or a decline on one press | Model files and execution provider in `Asked`; the chosen subject stored and reused; determinism defined per machine and model files |
| EfficientViT's two timings disagree (38 ms against 301 ms, both float32) | Re-measured before any runtime gate is set |
| The reveal is theatre around a small move | P1 ships only the evidence line, the outline and the plan card; the marks and the crossfade wait for the surprise question to pass 30 % |
| The reveal grows tiresome or stutters | About a second, once per photograph, skippable, reduced motion respected, evidence lines only after three presses; a view transform, one commit, no draft renders while BiRefNet holds the card |
| Keys and accessibility | The chooser a focusable toggle group on the arrow keys; Tab left to GTK; Shift+Space for before Auto; every mark a plan-card row |
| Determinism breaks between worker and main thread | One pure function per part on the same read; RANSAC over all pairs, no seeds; the hash test |
| The Apple build drifts, or is slow on the processor | The read's schema is the contract; parity on decisions; the read measured on an iPad's processor before P1 |
| An iPhone HEIC or ProRAW lifted twice | Treated as a finished file until an iPhone stratum of 10 says otherwise |
| Panoramas and merges squashed by the segmentation | Global tone only, level, emphasis and crop refused, and said |
| Licences | Counsel a gate before P1 for the Apple build on BiRefNet, YuNet, EfficientViT, PP-ResNet and YOLOX data; the Vision fallback written; GeoCalib only after counsel; no new weights through P3 |
| The patented Upright homography | Roll plus a vertical keystone only; the patents' status to be checked |
| The learner learns Auto from Auto | Edits within tolerance of Auto's answer excluded; one crop proposal kept outside the personal mode; "Forget my style" |
| Docs drift | ADJ-001, GEOM-002 and an AUTO entry updated in each phase; the two rewritten tests say why they changed |

**Refusals Auto says out loud:** "No horizon or upright lines clear enough to
level by", "The lines disagree — left alone", "Tilt of 12° looks deliberate —
left alone", "Already level", "Verticals not corrected — your masks would
move", "No level: healed spots would move", "Not levelled or cropped: a merged
panorama", "Light left alone — the bird already leads", "Silhouette kept",
"Exposure already right — the brightest highlights sit just below white", "−1
EV dialled in — kept low-key", "Exposure is yours (+½ stop) — left", "Two
subjects — light left even", "The matte and the person disagree — no subject
lift", "Could not tell what the photograph is about — light set for the whole
frame", "Separation skipped — it would show an edge", "Nothing in the frame
asks for a crop", "No crop offered: healed spots would move". And two offers
rather than refusals: "Level 2.1° would cut the hand at the left edge —
apply?", "The sea is tilted 10° — level it?". Each blames the model rather
than the photograph (UX-016).

---

## 12. Sources

Claims marked *unverified* in the text rest on the sources below that could not
be read from here (Springer, ResearchGate, library.imaging.org, the patent
mirrors, PetaPixel, Tom's Guide, PhotoWorkout).

Level and horizon
- GeoCalib (code, weights, OpenPano, AUC figures): https://github.com/cvg/GeoCalib
- Perspective Fields licence: https://github.com/jinlinyi/PerspectiveFields/blob/main/LICENSE
- Apple horizon: https://developer.apple.com/documentation/vision/vndetecthorizonrequest · Core Image level option: https://developer.apple.com/documentation/coreimage/ciimageautoadjustmentoption/level · auto-adjustment keys: https://developer.apple.com/documentation/coreimage/autoadjustment-keys
- Maker-note level tags: https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Canon.pm · https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Olympus.pm · https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Panasonic.pm · https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Nikon.pm · https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Pentax.pm · https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/Apple.pm
- Fujifilm DR, film and monochrome tags (0x1003, 0x1401, 0x1402, 0x1403, 0x140b, 0x1443–0x1445): https://github.com/exiftool/exiftool/blob/master/lib/Image/ExifTool/FujiFilm.pm
- Fujifilm roll dead band and brand coverage: https://github.com/FocusPointsLrC/Focus-Points/blob/master/docs/Compatibility.md · https://github.com/FocusPointsLrC/Focus-Points/issues/365
- Lee et al., Automatic Upright Adjustment (CVPR 2012): https://cg.postech.ac.kr/papers/06_Automatic-Upright-Adjustment-of-Photographs-with-Robust-Camera-Calibration.pdf · patents (*unverified*) https://patents.google.com/patent/US9582855B2/en , https://patents.justia.com/patent/9519954 · Lightroom Upright: https://helpx.adobe.com/lightroom-classic/desktop/help/upright-automatic-perspective-correction.html
- Content-Aware Rotation (ICCV 2013): https://dl.acm.org/doi/10.1109/ICCV.2013.74
- Maritime horizon by segmentation and line fit: https://ietresearch.onlinelibrary.wiley.com/doi/full/10.1049/el.2018.0989
- HLW / DeepHorizon licence: http://hlw.csr.uky.edu/
- Camera Raw Generative Expand for Upright corners: https://helpx.adobe.com/camera-raw/using/generative-expand.html

Crop and composition
- GAIC: https://arxiv.org/html/1909.08989 · https://github.com/HuiZeng/Grid-Anchor-based-Image-Cropping-Pytorch · https://www4.comp.polyu.edu.hk/~cslzhang/paper/GAIC-PAMI.pdf
- AVA's source (DPChallenge): https://github.com/imfing/ava_downloader
- Autocropping: a closer look: https://link.springer.com/chapter/10.1007/978-3-030-30645-8_29
- Cropper (CVPR 2025): https://cvpr.thecvf.com/virtual/2025/poster/34238 · notes: https://github.com/zhaoyang97/Paper-Notes-en/blob/main/docs/CVPR2025/multimodal_vlm/cropper_vision-language_model_for_image_cropping_through_in-context_learning.md · CROP (2026): https://arxiv.org/abs/2605.12545 · ShotCrop³ (2026): https://arxiv.org/html/2606.05635 · COMEX (2026): https://arxiv.org/abs/2608.07570 · Venus (CVPR 2026): https://github.com/PKU-ICST-MIPL/Venus_CVPR2026
- Rule of thirds, weak: https://pdfs.semanticscholar.org/c060/3b5133ff5c534e0e504af36b9422c47a65f9.pdf
- Horizon placement (Svobodova et al. 2014, *unverified*): https://www.researchgate.net/publication/260008070
- Centre and balance: https://www.ncbi.nlm.nih.gov/pmc/articles/PMC4707557/ · https://www.semanticscholar.org/paper/668b0bf51ece5151e65cad6911327b2d675db2e1
- Look room and inward bias: https://doi.org/10.1177/0301006617694189 · https://link.springer.com/article/10.3758/s13414-021-02289-y
- Twitter's crop: https://blog.x.com/engineering/en_us/topics/insights/2021/sharing-learnings-about-our-image-cropping-algorithm · https://github.com/twitter-research/image-crop-analysis
- Products: Pixelmator ML Crop https://www.macstories.net/news/pixelmator-pro-2-1-adds-ml-crop-quick-fill-color-and-text-tool-updates/ · Apple Photos Extend and Spatial Reframing https://www.macrumors.com/2026/06/08/apple-ai-reframing-and-editing-tools-in-photos/ · Pixel Auto frame https://www.androidpolice.com/pixel-auto-frame-guide/ · Lightroom without auto crop (*unverified*) https://petapixel.com/2025/11/03/lightrooms-new-features-ai-culling-auto-dust-removal-color-variance-slider-and-more/ · smartcrop.js https://github.com/jwagner/smartcrop.js
- Apple aesthetics and saliency: https://developer.apple.com/documentation/vision/calculateimageaestheticsscoresrequest · https://developer.apple.com/documentation/vision/cropping-images-using-saliency

Light and subject
- Miangoleh et al., realistic saliency-guided enhancement (CVPR 2023): https://arxiv.org/html/2306.06092 · https://github.com/compphoto/RealisticImageEnhancement · the training range: https://github.com/compphoto/RealisticImageEnhancement/blob/main/train_realismnet.py · https://github.com/compphoto/RealisticImageEnhancement/blob/main/utils/applyedits.py · https://github.com/compphoto/RealisticImageEnhancement/blob/main/dataloader/cocodataset.py
- GazeShiftNet (ECCV 2020): https://arxiv.org/pdf/2008.05413
- CAPE (Kaufman, Lischinski, Werman 2012): https://onlinelibrary.wiley.com/doi/10.1111/j.1467-8659.2012.03225.x
- Colour contrast only with luminance contrast (Cajar and Laubrock 2026, *unverified*): https://link.springer.com/article/10.3758/s13414-026-03226-7 · contrast gradients add (Engmann et al. 2009): https://link.springer.com/article/10.3758/APP.71.6.1337 · objects predict fixations (Einhäuser, Spain, Perona 2008): https://jov.arvojournals.org/article.aspx?articleid=2193226
- Skin tone: El-Yamany et al. 2019 (*unverified*) https://library.imaging.org/admin/apis/public/api/ist/website/downloadArticle/ei/31/4/art00004 · Google Real Tone https://blog.google/products-and-platforms/devices/pixel/image-equity-real-tone-pixel-6-photos/
- Reinhard's automatic key: https://www.researchgate.net/publication/255682296_Parameter_Estimation_for_Photographic_Tone_Reproduction · lightness order error: https://www.researchgate.net/publication/236675436_Naturalness_Preserved_Enhancement_Algorithm_for_Non-Uniform_Illumination_Images
- Products: Lightroom Auto (2017) https://blog.adobe.com/en/publish/2017/12/12/announcing-december-update-lightroom · Lightroom Auto too bright on raws https://www.lightroomqueen.com/community/threads/tone-auto-way-to-bright-for-raw-files.24159/ · Adaptive Color https://gregbenzphotography.com/lightroom-acr/acr-17-ai-adobe-adaptive-profiles-non-destructive-denoise-generative-expand/ · Capture One Smart Adjustments https://support.captureone.com/hc/en-us/articles/7129920998045-Smart-Adjustments · darktable spot exposure https://docs.darktable.org/usermanual/4.8/en/module-reference/processing-modules/exposure/
- Fixation model for the harness: https://github.com/alexanderkroner/saliency
- Depth Anything V2: https://github.com/DepthAnything/Depth-Anything-V2

Models and training data
- BiRefNet model zoo and training sets: https://github.com/ZhengPeng7/BiRefNet · DIS5K terms of use: https://github.com/xuebinqin/DIS/blob/main/DIS5K-Dataset-Terms-of-Use.pdf
- YuNet: https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet · trained on WIDER FACE: https://github.com/ShiqiYu/libfacedetection.train · WIDER FACE licence: https://huggingface.co/datasets/CUHK-CSE/wider_face
- SFace licence: https://github.com/opencv/opencv_zoo/tree/main/models/face_recognition_sface
- Apple Vision fallbacks: https://developer.apple.com/documentation/vision/vndetectfacelandmarksrequest · https://developer.apple.com/documentation/vision/vngenerateforegroundinstancemaskrequest

The wow
- Labour illusion (Buell and Norton 2011): https://doi.org/10.1287/mnsc.1110.1376 · algorithm aversion (Dietvorst et al. 2015, 2018): https://doi.org/10.1037/xge0000033 , https://doi.org/10.1287/mnsc.2016.2643 · IKEA effect: https://doi.org/10.1016/j.jcps.2011.08.002 · choice overload: https://doi.org/10.1037/0022-3514.79.6.995 · response times: https://www.nngroup.com/articles/response-times-3-important-limits/
- Apple Clean Up's reveal (*unverified*): https://www.tomsguide.com/phones/iphones/i-tried-apple-intelligences-photos-clean-up-feature-and-it-feels-kind-of-magical · Google Photos Help me edit: https://9to5google.com/2025/09/23/google-photos-help-me-edit/ · Luminar's composition and horizon reviews (*unverified*): https://www.photoworkout.com/luminar-neo-review/

Like you
- Personalisation of image enhancement (Kang, Kapoor, Lischinski 2010): https://www.microsoft.com/en-us/research/publication/personalization-image-enhancement/
- FiveK licences: https://github.com/yuukicammy/mit-adobe-fivek-dataset · PPR10K: https://github.com/csjliang/PPR10K
- Imagen's personal profiles: https://www.intelligenthq.com/imagen-ai-personal-profile-creation-3000-photos-vs-5000-photos/
- DINOv2: https://github.com/facebookresearch/dinov2 · Vision FeaturePrint: https://developer.apple.com/documentation/vision/generateimagefeatureprintrequest
- Unsplash Lite terms: https://github.com/unsplash/datasets/blob/master/TERMS.md · pyiqa licence: https://github.com/chaofengc/IQA-PyTorch/blob/main/LICENSE
