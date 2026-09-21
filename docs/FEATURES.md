# Feature index

Every feature has an ID. Reference it in commit messages, PR titles and TODO
comments — `// TODO MASK-002: brush falloff` — so a line of code can be traced
back to the thing it was for. `docs/AGENT_GUIDE.md` has the rules.

**This file is the single source of truth for status.** The README explains how
the interesting parts work and what was measured; it does not keep a second
list.

| | Meaning |
|---|---|
| ✅ | Built and in use |
| 🟡 | Partly built — the entry says which part |
| ◻️ | Planned |
| ❌ | Withdrawn, with the reason |

---

## Groups

| Prefix | Area |
|---|---|
| `APP` | Application shell and GNOME integration |
| `IO` | Import, export, catalog persistence |
| `LIB` | Library browsing, rating, filtering, batch |
| `DOC` | Document model, operation stack, history |
| `CANVAS` | Canvas rendering and navigation |
| `RENDER` | The render pipeline and colour science |
| `OPTICS` | Lens corrections |
| `TOOL` | Tools with their own canvas interaction |
| `GEOM` | Geometry: rotation, perspective, aspect |
| `ADJ` | Adjustment operations |
| `FILTER` | Filters and effects |
| `DETAIL` | Sharpening, noise reduction, artefacts |
| `HDR` | High dynamic range and tone mapping |
| `MASK` | Masking and local adjustment |
| `RETOUCH` | Healing, cloning, removal |
| `PERF` | Performance and robustness |
| `UX` | User experience and accessibility |
| `START` | Onboarding: from receiving the AppImage to a first edited photograph |

---

### START — Onboarding

Everything a new user runs into between receiving the AppImage and exporting
a first photograph. Today every step of it assumes the person who built the
program: the models come from a shell script in the source tree, the menu
entry does not exist, and the messages that say what is missing name
`dev/fetch-models.sh`, which an AppImage user does not have.

- ✅ **START-001**: Running the AppImage. The instructions a download page
  needs: make the file executable, and where FUSE is missing either install it
  or run with `--appimage-extract-and-run`. A double-click in Files does nothing
  on a file that is not executable, and says nothing either, so this is the
  first place a new user gives up. The README's "Installing" section has them.
  `libfuse2` turned out not to be the requirement: appimagetool embeds the
  static type2-runtime, which carries its own libfuse and only needs a setuid
  `fusermount3` or `fusermount` on the host — package `fuse3` where it is
  missing. Built on Ubuntu 24.04, so the floor is glibc 2.39.
- ✅ **START-002**: A menu entry and an icon. An AppImage does not integrate
  itself: without one Numa is a file in Downloads that has to be found again
  every time. On first start, offer to add it to the applications menu — write
  the `.desktop` file and icon to `~/.local/share`, pointing at where the
  AppImage actually is — and to take it out again. Say that moving the AppImage
  afterwards breaks the entry, or notice and repair it on the next start.

  Built: when running from an AppImage (`$APPIMAGE`), the first start offers
  it in a toast rather than another dialog. The entry is the shipped one with
  `Exec` quoted as the desktop entry spec wants and `TryExec` on the file, so
  a moved AppImage is hidden rather than broken, and marked as Numa's own; the
  icon is compiled into the binary. On every start an entry of ours that points
  elsewhere is rewritten to this file, and Preferences has a switch that adds
  or removes it. Tried with a path containing a space and with a moved file.
- ✅ **START-003**: A first start that explains itself. The empty library says
  "Add a folder" today; it should also say the two things people worry about:
  photographs are never moved, copied or changed, and Numa keeps its catalog in
  a hidden `.numa` folder inside the folder that is added (IO-014), so the
  folder has to be writable. A read-only folder or a network share that refuses
  writes needs its own clear message rather than a failed add.

  Built: before there is a library the page is a status page — "Add a Folder
  of Photographs", those two things said plainly, and an Add Folder button —
  with the filter bar hidden, since there is nothing to filter. A folder Numa
  cannot create `.numa` in is refused when it is added, with a dialog that
  says why and what to do instead, and is not listed; before, it was listed
  and failed every time it was opened. Tried with an empty `XDG_DATA_HOME`,
  which is what a new user has.
- ✅ **START-004**: Models downloaded from inside the application. About
  420 MB across six models (EfficientViT-Seg 61 MB, IS-Net 179, PP-ResNet 103,
  SFace 39, SlimSAM 40, YuNet 0.2), and for an AppImage user there is no
  `dev/fetch-models.sh`. A page that lists each model with its size, what it
  enables and its licence, downloads the ones chosen with progress, resumes an
  interrupted download and checks a hash. Every message that names the script
  today (the mask panel, the Click mask, the segmentation toast) offers the
  download instead. Nothing is fetched without being asked: 420 MB on a metered
  connection is not a default.

  Built so far: **Preferences** (`Ctrl+,`, in the main menu) lists the six
  models with what each enables, its licence and size, and whether its files
  are there, with a button to the models folder; the messages that named the
  script point there now. Each model downloads from there — or all of them —
  with `curl` into a `.part` file that resumes and is renamed when complete, and
  a failed download offers the link in a browser; a file fetched by hand works
  under the name its link gives it. On a first start with no models, a dialog
  says there are additional files — once — as the features they bring, each
  with an icon and its size, the models themselves folded away underneath, and
  one Download button. It downloads in the standing progress toast, which can
  be stopped, and a file that fails does not stop the others. EfficientViT is
  the one model with no upstream ONNX, so it is published as the `models`
  release of this repository; the link answered 404 until that release was
  made on 2026-09-17. Every model is on that release now, with
  `MODELS-LICENSES.txt` (Apache-2.0, and MIT for YuNet and rembg's export), so
  the downloads keep working whatever happens upstream: the mirror is asked
  first and the original source second. Without the models the features that need one are not offered:
  People is hidden, the face tools are hidden, Click is insensitive with the
  reason in its tooltip, and "In this photograph" is gone. `NUMA_MODELS=<empty
  folder>` runs the application as a new user sees it. Not yet: a checksum.
- ✅ **START-005**: What Numa can do with this camera, said when a library is
  added. A dialog after the scan lists the cameras in a sample of up to 300 of
  its files — each with whether a camera profile was found, and if not, that
  colour comes from the matrix in the file, which is fine — and how many RAW
  files could not be read.
- ✅ **START-006**: The first Analyse, offered. The same dialog says what
  Analyse measures, what depends on it (suggested ratings, best of burst,
  People), that it runs while browsing and can be stopped, and offers to start
  it. *No per-machine time estimate: "some minutes" rather than a number that
  was never measured here.*
- ✅ **START-007**: Faces, explained before they are used. The first time People
  is opened: recognition runs on this computer, nothing is uploaded, and names
  live in the library's `.numa` folder, so they travel with it. Once.
- ✅ **START-008**: The keys, shown once. A banner under the library's filter bar
  (0–5, P, X, U, Ctrl+/) and one under the editor's bar (Space to compare,
  double-click to reset, I for the camera's details), each gone for good after
  "Got it".
- ✅ **START-009**: Knowing a newer version exists. Asked once — half a minute
  after a start with a library, so it is never a second dialog on top of the
  first start's — whether to look; if yes, the releases on GitHub are fetched
  at most once a day with `curl`, as the models are, and a toast names a newer
  version with a button to its page. Only numbered tags count: the models
  release is tagged `models`. A switch in Preferences changes the answer.
  *Not AppImageUpdate: that needs update information embedded at packaging.*
- ✅ **START-010**: Where everything is, and how to remove it. One place —
  the About dialog or a small preferences page — listing the settings file,
  models, added camera profiles and thumbnail cache with their sizes and a
  button to open each folder, plus what uninstalling means: delete the
  AppImage and those folders, and the `.numa` folder in each library if the
  ratings and edits should go too.

  Built so far, in the same Preferences dialog: why models and camera profiles
  are delivered separately, every folder profiles are read from with how many
  are in each (the user's own always listed, so there is somewhere to put one),
  and the settings and thumbnail folders — each with a button that opens it in
  the file manager. The folders come from the platform, not from a written
  path, so they are right inside a Flatpak too. Not yet: sizes and the
  uninstall text.
  Now also the models and presets folders, every folder's size counted off the
  main thread, and a "Removing Numa" section saying what deleting what does.
- ✅ **START-011**: Reporting a problem. The About dialog's Troubleshooting page
  holds the debug information as one block to copy or save: Numa, GTK and
  libadwaita versions, the renderer, whether it runs as an AppImage, which
  models are installed, the open library's size and the cameras in a sample of
  it, and how to get a fuller log with `RUST_LOG`. "Report an Issue" opens the
  issue form. Building it found a crash: rawler panics on a Sigma X3F, and in
  a GTK callback a panic aborts — so reading a file's summary is caught now.
- ✅ **START-012**: The first scan, visible. A new library of thousands of RAW
  files spends its first minutes building thumbnails. Once that has taken
  longer than the loader's 400 ms, the toast Analyse uses (UX-019) says
  "Making thumbnails — 38 of 42", so a half-empty grid reads as work in
  progress rather than a fault. Counted per run of the queue, because only
  what is on screen is made and scrolling asks for more; a run that only read
  the cache says nothing. No Stop, since the cards on screen would stay blank.
- ◻️ **START-013**: Crash reports, sent only when the photographer says so.
  - **Rust panics.** A panic hook writes a report into the data folder. It
    holds the version, how Numa runs (Flatpak, AppImage or a build), GTK,
    the GPU line, and the panic with its backtrace. Paths become `~`, and
    file names become `<photo>`.
  - **Native crashes** (a driver, ONNX Runtime, Dawn). The panic hook never
    runs for these, so a marker the way the GPU guard works: written at start,
    removed on a clean exit.
  - **At the next start** a toast says Numa closed unexpectedly. "Send
    report…" shows the whole text before anything leaves. "Open on GitHub"
    pre-fills an issue; Copy is there for anyone without an account.
  - No server and no cost. A crash server (GlitchTip) waits until there are
    more reports than one person reads. See ENGINEERING, *Hearing back*.
- ◻️ **START-014**: What gets used, if the photographer agrees. Off until
  asked, asked once, the way the update check is.
  - **What is sent:** a short list of counts and nothing else — which tools
    were used, how long the slow ones took and on which device, how Numa runs,
    the camera make.
  - **What never is:** no photographs, paths, names, places, EXIF, faces or
    library names, and no identifier that ties one session to the next.
  - **Seeing and stopping it:** Preferences shows exactly what would go, and
    the switch stops it.
  - **Where it goes:** Aptabase — privacy-first analytics made for desktop
    apps, open source, EU hosting, free for 20,000 events a month — over the
    `curl` Numa already calls for the update check. See ENGINEERING,
    *Hearing back*.
- ◻️ **START-015**: Ideas and tips from the photographer. "Send feedback…" in
  the main menu: a text field and, ticked off by default, the debug information
  of START-011. It opens GitHub Discussions (an Ideas category and issue forms,
  both free), pre-filled. Mail is the way in for anyone without a GitHub
  account.

### APP — Application shell

- ✅ **APP-001**: App actions (`app.quit`, `app.about`) and accelerators.
- ✅ **APP-002**: About dialog with version and third-party attribution.
- ❌ **APP-003**: ~~Keyboard shortcuts window.~~ *Built as UX-013, which
  also lists what the keys actually are rather than what they should be.*
- ✅ **APP-004**: Settings that survive a restart — the window's size and
  whether it was maximised, what the grid was narrowed and sorted by, and what
  the last export was set to.

  Three things and not more. A setting is a promise to remember something
  correctly for ever, and the ones worth making are the ones a person would
  otherwise set again on every launch.

  **Not GSettings**, which is what this entry used to say. The catalog already
  had a key/value table with the last-opened library in it, and a second store
  would mean a schema to compile and install before the application would run
  out of its own source tree at all, plus two places for the answer to "what
  did it remember" to disagree. One table, as JSON, moving with the catalog.

  Nothing here is load-bearing: missing or unreadable reads as absent and the
  caller uses its default, because failing to start over a remembered window
  size would be a worse fault than forgetting it. The grid's filter is written
  in `reload_grid` rather than at each of the five controls that can change it,
  which is the only arrangement where the two cannot drift apart, and
  everything is read before a single widget is built so the controls come up
  showing what is actually in force.
- ✅ **APP-005**: Photographs dragged onto the library from the file manager are
  copied into its folder, and a dragged folder as a folder. Nothing already
  there is overwritten; each copy keeps its modified time, which the grid sorts
  by, and is written under a `.part` name until it is whole. What was copied,
  what was already there and what failed is said afterwards. Not a way to open
  a single file outside a library, which is IO-001.

  **Import from a card or a camera** (the brief's C7, extended by the
  photographer on 21 September: a connected camera, and "a folder that seems
  logical — a little smart is fine"). A card in a reader and a camera on a
  cable both arrive as a GIO mount — the camera through GVFS, read by its FUSE
  path — and either is offered by a toast the moment it appears; the camera
  button in the header opens the same dialog on whatever is there, or any
  folder (`importing.rs`, `numa::io::import`). The card is read from its
  `DCIM`, at the top or one folder down as a camera lists it. What a library
  already has — the same name, size and first and last 64 kB — is left on the
  card and said once ("3 already in Mallorca"). The rest is split into shoots
  where the camera was put down for two days, and each is offered a place: the
  library whose dates it falls in or beside (two days either way), else a new
  folder where new ones go, read off the libraries there are — a year folder
  when most of them sit in one (`Fotos/2026/`), their common folder otherwise —
  named for its first day and renamable in the dialog; "Change…" puts it in
  any folder. An optional pattern renames (`{date} {time} {n} {name}`, one
  count per frame so a RAW and its JPEG keep one stem). The copy is APP-005's —
  `.part` until whole, modified time kept, nothing overwritten; a name taken by
  a different photograph gets `-2` rather than losing either — with a count
  and a Stop on the toast; new folders become libraries, the rest are
  rescanned, and the first opens. Checked on a card made of Mallorca frames:
  three found already there, two put into Mallorca by their date, three into
  a new library named in the dialog, modified times kept and no `.part` left.

  **The libraries as folders** (the photographer's, 21 September: "a page
  before it, like the thumbnails in the drop-down, as a kind of folders"): the
  page before a library, as the editor is the page after one — "Libraries",
  the first step of the path at the top of the grid and of the editor, leads
  there (the photographer's, in place of a back arrow of its own); the
  library's own step shows its chevron only while it is pointed at or open.
  Every library as a stack of
  three prints at 132 px, its name and "362 photos · Sep 2025 – Oct 2025",
  shelved by year folder and in the order they were shot, "All libraries"
  first; a press opens it (`folders.rs`, on `places`' fan, now drawn at any
  size). Adding a folder and importing are in its header too. The prints on a stack —
  here and in the picker — are the library's best, not its newest (the
  photographer's, the same day): the highest rated, then the picked, then the
  edited, then Analyse's suggestion, the newest last; never a rejected frame,
  and one of each burst, chosen by that same order — a RAW and its HEIF are
  one burst whose sharpest is often the HEIF nobody edited.
- ✅ **APP-006**: Recent files integration. A photograph opened from outside and
  every exported file are added to the desktop's recent files, so the file
  manager's Recent and any file dialog offer them again.

### IO — Import, export, catalog

- ✅ **IO-001**: Open a single image outside any library — Open… (Ctrl+O) in the
  menu, `numa photo.RAF`, or Open With from a file manager. The photograph opens
  in the library that holds it; when none does, its folder is offered as a new
  library, since edits live in a library's catalog and an edit held only in
  memory is work lost on closing with nothing to say so.

  It lands in the **loupe** (LIB-004) rather than the editor: a photograph
  double-clicked in a file manager is being looked at, not yet worked on, and
  the grid behind the loupe is the folder it came from — so the next frame is
  one arrow key away and the editor is one press of Enter. If the grid is
  narrowed to something that does not include it, the editor takes it instead.

  The desktop entry claims every type Numa reads: the RAW formats
  shared-mime-info names (DNG, CR2/CR3/CRW, NEF/NRW, ARW/SR2/SRF, RAF, ORF,
  RW2, PEF, SRW, X3F, 3FR/FFF, IIQ, ERF, MOS, MRW, DCR/KDC), HEIF, JPEG, PNG,
  TIFF, WebP and BMP. **Open photographs with Numa** in Preferences makes it
  the default for all of them and switches back to whatever the desktop would
  have chosen; it goes through GIO rather than writing `mimeapps.list`, which
  belongs to the desktop, has three locations and a precedence between them.
  For a machine where Numa is not installed at all,
  `packaging/set-default.sh [AppImage]` does the same from a terminal.
- ✅ **IO-002**: Export to `<library>/edited/`, never overwriting.
- ✅ **IO-003**: Autosave. Every path that changes the document already says so
  — that is what the history push is — so the same call schedules a write, two
  seconds after the last one. Long enough that dragging a slider is one write
  and short enough that a crash costs a sentence rather than a session. No
  recovery file to write and find again afterwards: the catalog is where edits
  live, so a stack that reached it survives anything. What autosave writes is
  tested in full, masks and their clicks and strokes included, because it is now
  the only thing between a session and a power cut.
- ❌ **IO-004**: ~~Per-file `.numa` sidecars.~~ *Replaced by IO-007;
  sidecars litter the user's folders and rewrite whole files for one rating.*
- ✅ **IO-005**: Metadata. Orientation, exposure, lens and film mode are read,
  and an exported JPEG carries the camera's own EXIF: a RAW keeps a JPEG of the
  frame inside it, and that JPEG's APP1 segment is lifted whole into ours.
  Found by its signature — `FF D8 FF E1`, a length, `Exif\0\0`, a TIFF header —
  rather than at the offset a RAF's header names, which was the first version
  and quietly exported every other make's photographs bare. Every tag the camera
  wrote, not the handful this application parses, and the shortest way to it.
  Two things are patched: the orientation, because the rotation is already in
  the pixels and a viewer applying it again would turn the photograph on its
  side, and the pointer to the thumbnail, which is of the unedited frame.

  **IPTC, and the location left behind** (21 September). What the camera does
  not write goes in an XMP packet — IPTC Core, which Lightroom, Bridge,
  exiftool, Windows and every photo site read, where the old IIM block is kept
  only for software older than XMP: the creator and copyright typed in the
  export dialog, and from the catalogue the stars, the people in the
  photograph (IPTC's persons shown) and the albums it is in, people and albums
  together as keywords. In a JPEG as its own APP1, in a TIFF as tag 700, in a
  JPEG XL as its `xml ` box; an AVIF has no box for it yet. *Remove location*
  rebuilds the camera's EXIF without its GPS directory, in every format that
  carries one.
  The capture time is read too, kept beside the file's own date, and it is what
  the grid orders on. The file date alone was the first version, on the
  reasoning that a camera writes it at the shutter — true of a card straight out
  of one, and wrong for every archive that has ever been copied: `cp` without
  `-p` stamps the day of the copy, and a parallel copy stamps it in no
  particular order. A real 11 509-frame archive read back shuffled, and CULL-002
  cut bursts out of that order which were never one moment. A photograph with no
  capture time — a scan, an export stripped of its tags — falls back to the file
  date rather than to the beginning of time.

  A TIFF-based RAW — ARW, NEF, CR2, DNG — keeps its metadata in the file's own
  IFDs and its preview carries no APP1 at all, so those exported bare. They now
  get a block that is written rather than copied: eleven tags out of IFD0 and
  the Exif IFD, back through rawler's TIFF writer, 924 bytes against a RAF's 65
  kB. Whitelisted rather than filtered, because MakerNote is full of offsets
  into a file that no longer exists and there is no reading it without knowing
  the make. Verified with exiftool on exports from both kinds: a RAF and a Sony
  ARW both come out naming make, model, lens, aperture, shutter, ISO and date,
  upright, with no thumbnail and no warnings. PNG gets none of this — nothing
  reads camera data out of one.
- ✅ **IO-006**: Clipboard copy/paste. Ctrl+Shift+C in the editor puts the edited
  picture on the clipboard, at the editing proxy's 2400 pixels, rendered off
  the main thread with masks worked out — for pasting into a message or a
  document without writing a file. Copying and pasting *settings* is LIB-005.
- ✅ **IO-007**: SQLite catalog holding photos, ratings, flags and edit stacks.
  Split per library by IO-014.
- ✅ **IO-008**: RAW support; thumbnails from the embedded JPEG preview. Which
  formats those are is asked of the decoder rather than written down: rawler
  reads twenty-nine and the list here was nine, so a Leica RWL, a Hasselblad
  3FR, a Phase One IIQ and a Sigma X3F were never so much as looked at.

  Verified end to end on Fujifilm RAF and on a Sony ARW: an ILCE-6000 frame
  decodes in 248 ms, finds its camera profile, keeps its lens name, renders
  true and exports with its metadata. A TIFF-based RAW from an actual camera
  goes through this application without a complaint.

  A DNG written by rawler's *own* converter from one of those RAFs does not: it
  decodes, previews, matches a profile, develops and exports, and renders
  magenta. The fault is upstream — the raw values are byte for byte the RAF's
  while the CFA pattern comes back with its rows reversed after the first, so
  most photosites are given the wrong colour, and rawler's own complete
  pipeline renders it the same way. Worth knowing when a converted DNG turns
  up; not worth working around here.

  A sample frame per make is still the only thing that turns "every camera"
  from what the decoder promises into what has been seen.
- ✅ **IO-009**: Library roots — add, rescan, originals left where they are.
- ✅ **IO-010**: Demosaic backend settled: rawler, no libraw.
- ✅ **IO-013**: Markesteijn demosaic for X-Trans at full size; bilinear for the
  proxy, where the difference is 3.6 % rather than 81 %.

  **FT-021 B1, 20 September: the editor's proxy comes off Markesteijn too.**
  The 3.6 % that justified bilinear for the proxy was measured on the decode
  alone; what it costs in the finished picture at fit zoom is **15 %** of
  acutance against what the export produces, because the box filter down to
  2400 px is applied to a worse demosaic. Capture sharpening, which the proxy
  skips at that scale, turns out to be worth only 1.4-1.6 % after the same
  filter — so the demosaic was fourteen of the fifteen points. `decode_for_
  editing` picks Best for a raw; the fitted view is now within 1.2-1.4 % of
  its own ground truth and agrees with 1:1, which already used that decode.
  0.34 s on opening, nothing on a slider tick. Thumbnails keep the draft
  decode: a 320-pixel card cannot show it, and a library has hundreds.

  **Brief A9 and FT-022, 20 September: the false-colour step.** Markesteijn
  invents coloured pixels where its guess at the two thirds of the mosaic that
  has no red and no blue sample goes wrong, and rawler ships no step to remove
  them. One 3x3 median of the channel ratios, run **after the lens geometry**:
  green is left exactly as it was and R and B are rebuilt as `G x median`.
  On FT-019's patch of DSCF9580 it takes 64 % of the high-pass a\* and 63 %
  of the b\* out at sharpening 0, colour 0, for no measurable time at all.

  Where it runs is most of what it is worth. At the demosaic, where it first
  landed, it removed the same noise and then `correct_geometry` put two thirds
  of it back — that pass resamples R, G and B on three different radii to undo
  lateral colour, which is about a pixel apart near the frame edge, so
  luminance texture returns as ratio noise after the step that cleaned it.
  Moved after the geometry, 16 % / 35 % becomes 65 % / 64 %.

  It does not cost detail, and after the move that needs saying per channel
  rather than for green alone: over the stag's head on DSCF9580, red's acutance
  rises 1.5 % and blue's 5.0 % while green's does not move, because the
  bilinear resampling that undoes lateral CA softens R and B relative to green
  and pulling their ratios back onto green's structure puts that back.

  X-Trans only: Bayer never enters this path and the bilinear draft decode is
  untouched. 150 frames across 2022 to 2026 decode, render and export through
  it without a failure.
- ✅ **IO-014**: Each library keeps its own catalog, in a hidden `.numa`
  folder inside it: photographs, ratings, flags, edit stacks, analysis and the
  names on faces. The folder is the library — moved to another drive or
  computer and added from there, or removed and added back, it comes back with
  everything done in it. Paths are stored relative to the folder. The
  application's own catalog only lists where the libraries are and what to
  remember between runs. A library on a drive that is not connected is shown as
  not connected rather than as empty, and nothing is created at its mount point.

  A copy of each library's catalog is taken once a week into `.numa/backups`,
  named by date, and the last four are kept; restoring one is closing the
  application and copying it over `.numa/catalog.db`. The catalog uses a
  rollback journal rather than WAL, so a folder of photographs holds one file
  while the application is open, not three, for tools that copy folders without
  knowing what SQLite is.

  The single catalog from before is taken apart at the first start: a copy of
  the whole file is kept as `catalog-before-libraries.db` beside it, each
  reachable library gets its rows with their ids, and a library on a drive
  that is not connected keeps its rows at home until it is. Names on faces
  are kept per library and put together by name across every library that can
  be reached.
- ✅ **IO-015**: HEIF and HEIC. iPhones write them and Fujifilm bodies can (as
  `.HIF`), and Numa did not recognise them at all. Decoded by `heic-rs`, pure
  Rust under MIT/Apache — the photographer's choice over libheif, which is LGPL
  and C. Its steps are called one by one for one reason: a file with no nclx
  `colr`, which is every iPhone photograph, was converted as BT.709 limited
  range where the stream says BT.601 full range. With libheif's fallback
  instead it matches libheif to 47–57 dB on 18 iPhone files, 50–70 ms each.
  A RAW and a HEIF of the same frame sit side by side, as a JPEG does (LIB-018).

  **A Fujifilm .HIF opens at its preview size, and says so.** Raised on
  2026-09-17 by a Mallorca shoot: every `.HIF` in it failed with "picture is
  not fully covered by slices", one line per file. The file is a 2×2 grid of
  HEVC **Range Extensions** tiles — 4:2:2 at ten bits, which is what `ffprobe`
  calls profile Rext — and the decoder gets part of the way through a tile
  before the CABAC state drifts and the slice ends early. It is not a missing
  feature that can be switched on: heic-rs implements the RExt flags it parses
  and rejects the ones it does not, so this is a bug in it on this stream, and
  there is no newer release.

  What the file also carries is three previews of its own — 1920×1280, 640×480
  and 160×120 — written as **JPEG** items rather than HEVC ones, which is why
  they were never the problem. So when the primary image will not decode, the
  largest preview that does stands in: 1280×1920 in 21 ms, turned by the
  primary's own `irot`, identical to ffmpeg's decode of the same frame. A
  photograph at 1920 across is a library, a loupe, a cull and a print at
  postcard size; failing is none of those. It is logged once per file, and the
  info panel then shows the preview's size, which is the honest number for what
  is on screen — the RAF beside it is the file to develop. `irot` turns
  counter-clockwise where `image`'s rotations turn clockwise, which is asserted
  rather than assumed.

  `tests/heif_probe.rs` is the tool for the next one: it prints every item, its
  type and size, and whether it decodes.
- ✅ **IO-016**: Thumbnails kept in the library's `.numa` folder instead of the
  user's cache, so a library moved to another computer or drive shows at once
  instead of decoding every frame again — the same reason the catalog moved
  there (IO-014). The photographer's preference, 2026-09-17. Only the grid's
  default size (320 pixels, tens of kB a frame) goes there; the larger sizes
  LIB-004 asks for stay in the cache, so the library does not grow by a megabyte
  a frame per size. Keyed by the path within the library, which is what survives
  the move. The cache is still read first-time-round, and a library that cannot
  be written to keeps its thumbnails there. Not pruned when a file changes or
  goes, any more than the cache is.
- ✅ **IO-011**: JPEG, PNG and — the brief's A3 — TIFF 16-bit, the round-trip
  format for every other editor. The extension follows the format rather than
  the name template, so the two cannot disagree. The TIFF is developed at
  sixteen bits from the same stack (`render::develop16`; only the final
  rounding differs, and the eight-bit frame is the sixteen brought down to
  within one step — a test), written by rawler's TIFF writer rather than the
  `image` crate's, which would be a decoder in the binary for every user:
  RGB, Deflate over horizontal differences, the ICC of its space, and the
  camera's EXIF as a real Exif directory without the MakerNote. A 40 MP X-T5
  frame is 191 MB (239 uncompressed; LZW was 236, and without the predictor
  304 — larger than nothing); exiftool reads make, model, lens, exposure and
  date off it, ImageMagick reads it as 16-bit sRGB, and the q100 JPEG of the
  same stack is 0.0034 RMSE from it.

  **AVIF, JPEG XL and DNG** (21 September). All three from the sixteen-bit
  develop. *AVIF* is one AV1 still frame at ten bits, 4:4:4, from rav1e in pure
  Rust — without its assembly, which would need nasm to build: 6.3 s for a 20 MP
  frame on eight cores, 651 kB where the JPEG is 8 MB. rav1e and avif-serialize
  directly rather than through ravif, which describes every file as sRGB; the
  colour box and the AV1 header both name the primaries. AVIF has codes for
  sRGB and Display P3 only, so Adobe RGB and ProPhoto are written as P3 and the
  dialog says so. *JPEG XL* goes through the libjxl already on the computer,
  loaded when it is first asked for: the only lossy encoder in Rust is under the
  AGPL. Quality maps to libjxl's distance as `cjxl` does (90 is 1.0, "visually
  lossless"), 100 is lossless, and the file carries the ICC, the camera's Exif
  and an XMP box; where there is no libjxl the format is not offered. *DNG* is
  Lightroom's DNG export: the sensor's own values, losslessly compressed by
  rawler's writer, a 2048-pixel preview of the photograph as edited — turned
  back to the way the sensor lay, since the orientation tag applies to previews
  too — and the edit as Camera Raw settings (`foreign::to_lightroom`, the import
  run backwards: exposure to grain, the mixer, black and white, grading, white
  balance), so Camera Raw opens it looking roughly as it does here. The crop,
  curves, masks and retouching stay behind. A test writes each format from a
  corpus raw and reads it back; avifdec, jxlinfo and exiftool read all four.
- ✅ **IO-012**: Size on the way out — full, or a long edge of 4096 down to
  1080, Lanczos, never enlarging — and the edge that takes off, put back.
  Reducing a 40 MP frame to 2048 averages fourteen pixels into one, and the
  softness that follows is the resampling's rather than the photograph's. The
  same unsharp mask DETAIL-001 uses, at less than a pixel of radius and in
  linear light, only when something was actually reduced: acutance 18.3 to 23.0
  on a garden frame, 282 ms to 364 ms, no haloes at the stone edges. And a
  folder to choose, remembered like every other answer, with the library's own
  `edited/` as the default because it is right often enough to be one. One dialog for one photograph and for a
  hundred, and what it was told is remembered for the next time: Export is a
  pair of linked buttons, where the button itself repeats the last answer and
  the arrow to its right asks again. A pair rather than a split button, because
  the arrow's whole job is to open one dialog and a menu of one entry is a click
  in the way of it. Exporting a shoot is the same question forty times, and
  answering it once is what remembering it is for. The button's tooltip says
  what it will do, since it will not ask.

  **Presets, a watermark and the file name** (21 September). The dialog is a
  dialog of its own now, 480 wide, with the settings in groups and Metadata and
  Watermark folded away. Presets are named sets of every answer but the
  folder, chosen from the first row, saved and forgotten beside it; choosing
  one fills the rows, which can still be changed after. The watermark is a line
  of text in a corner, set in the system's sans at a size in thousandths of the
  long edge, white at seven tenths over a faint shadow so it reads on sky and
  shadow alike, drawn over the finished frame at its final size by Pango — the
  copyright line when no text is given. The name template, which the
  preferences had and the dialog did not, is a row. Colour space's notes were
  cut to a few words so the value beside them is no longer cut to "s…".

### LIB — Library

- ✅ **LIB-001**: Rating (0–5) and flag/reject.
- ✅ **LIB-002**: Filter and sort the grid.
- ✅ **LIB-003**: Keyboard culling (`0`–`5`, `P`, `X`, `U`) in the grid *and* in
  the editor, a star row in the editor's toolbar, the rating on each filmstrip
  frame, and the same on a right-click menu — which is how anyone finds out the
  keys exist. Deleting moves the file to the desktop's trash and forgets the
  catalog row; it says so before it does it. `Delete` in the grid
  asks that same question: the dialog and the trashing were already there
  behind the menu, and the key every file manager uses for them was the one
  gesture a cull reaches for that did nothing. It is bound in the grid's own
  key controller rather than as an application shortcut, because that
  controller sits on the window and bubbles — a focused entry has consumed the
  key first, so Delete still deletes text in the search and rename fields,
  which an accel would have taken before them.
- ✅ **LIB-004**: Thumbnail size control and a single-photo loupe. The size is a
  button at the end of the filter bar holding two sliders, how tall a
  row aims to be and how much space sits between the photographs, both
  remembered. They move in marked steps and show no pixel figure: the rows
  stretch to the width and the cards have padding, so a number would be a
  promise the layout does not keep. The thumbnails follow the size: the grid
  asks for 320, 640, 960, 1280 or 1920 pixels, whichever is sharp at the row
  height on this screen's scale — a fixed 320 was soft even at the old size on
  a HiDPI display — and the bands of cards held ready (PERF-005) shrink as
  the thumbnails grow, so the memory they take stays about the same. Changing either keeps the photograph at the top
  of the view where it was. The space is on top of each card's own 6 px of
  padding, which is where the selection outline is drawn.

  The loupe is Space: one photograph as large as the window allows, over the
  grid rather than in place of it, so the filter bar, the header and the scroll
  position are all still there when Space or Escape puts it away. It fades in
  and out over 160 ms, because a photograph that replaces a wall of
  photographs between one frame and the next reads as a new window rather than
  a closer look at this one. The arrows step through the shoot inside it and
  Enter takes the frame into the editor. The culling keys are not repeated for
  it: the loupe moves the grid's cursor with it, so 0–5, P, X and U already act
  on what is on screen.

  Under the photograph is a bar of everything that can be done to it and
  nothing that cannot: previous and next, five stars, pick, reject, Edit, and
  back to the grid. The keys do all of it and are faster, so every button names
  its key — a key nobody knows about is a feature nobody has. The stars are
  pressable here where the grid's are a readout: in the loupe what is being
  judged is the thing on screen, and there is no selection to be unsure about.
  Pressing the star already given clears the rating, which is how a rating is
  taken back without knowing that 0 is a key. What the bar shows is read from
  the card's own badge, so it says what was just typed rather than what the
  catalog said when the grid was filled.

  The pane is hidden as well as unrevealed once the fade is over: an overlay
  stretches its children, so a revealer left visible is a full-size widget over
  the grid that shows nothing and takes every click — which reads as a grid
  that has stopped responding. It shows the 1920-pixel thumbnail, which is a cache
  read rather than a decode, and so the as-shot frame rather than the edited
  one: this is the pass where a frame is kept or dropped, and the editor is
  where it is developed. Its keys run before the grid's own, since the grid
  keeps the focus underneath it and its arrows would otherwise move the cursor
  without the loupe following.

  **Compare** (the brief's C4) is C: two to four selected photographs side by
  side, over the grid the way the loupe is and on the loupe's thumbnails
  (`compare.rs`). One zoom and one centre for all of them — the wheel zooms
  about the point under the pointer, a drag pans, a double click goes to one
  thumbnail pixel a screen pixel there and back — so the eye in one frame is
  the eye in the next; above 1:1 it is nearest-neighbour, as the canvas is.
  The culling keys act on the frame under the pointer, whose name is the one
  in bold, through the same `apply_to_ids` the grid's keys use, so a rating
  given here is the same `photos` row (checked in the catalogue: 4 and pick on
  DSCF1192). Enter edits that frame, Escape or C closes. Opening is half a
  millisecond and cached frames are in within 8 ms; one not yet cached at
  1920 took 235. The loupe stays: it is the other half of the same look.

  UX-009's strip follows the same rule as of today: a frame is as tall as the
  strip and as wide as the photograph's shape, from the thumbnail cache, where
  it was a 64-pixel square left over from the square grid. It shows the grid's
  own 320-pixel thumbnail, so the strip decodes nothing of its own and stays
  sharp on a HiDPI screen.
- ✅ **LIB-013**: A mark in the grid and the filmstrip on photographs that have
  adjustments. Opening one is not editing it: an untouched document is stored as
  nothing at all, so the mark means what it says — which is the question a shoot
  raises on the second pass through it.
- ✅ **LIB-014**: Names on faces. The info page lists the faces in the open
  photograph, each with a picture of it and a name: typed once, and every other
  photograph with a face like it offers that name, saying it is a guess — Enter
  confirms, typing corrects. Only what was typed is stored; the name reaches
  photographs by likeness, so one named today reaches the ones taken last year.
  SFace (OpenCV Zoo, Apache-2.0, 37 MB) aligned on YuNet's five points; see
  ENGINEERING.md, "Faces and names", for the threshold and how it was measured.
  Everyone with a name is a place in the library picker (LIB-017) — worked out
  from the faces Analyse keeps, so a library needs a pass of Analyse first (the
  measures are at version 2 for it).
  **People**, a button in the grid's filter bar, is the place to do it in
  bulk: everyone named, with a picture, how many photographs and a Show that
  goes to them; renaming one there renames them everywhere, and
  giving two the same name makes them one person. Below, the faces nobody is
  yet, grouped so each group is probably one person — "In 38 photographs — who
  is this?" — named with one Enter. A group can be set aside as nobody to name
  (a stranger who keeps turning up, a statue), which, like a name, reaches faces
  like it; "Ask again" takes that back. A name can be forgotten, which gives its
  faces back to the groups. Faces seen in only one photograph are left to the
  info page. Pictures are kept by Analyse; a library analysed before that gets
  them fetched once, the first time the dialog opens.

  On the canvas too, from a switch under the People rows: a thin box around
  each face and the name in a dark pill under it — under, because a label
  across the eyes is exactly where nobody wants one, and kept inside the frame,
  because a face at its edge is common and a name running off the canvas is not
  a name. A guess carries a question mark, as the row does; a face nobody has
  named is not drawn at all, since an empty box over every stranger in a crowd
  is noise. Off unless asked for: a photograph is looked at for what it is
  until who is in it is the question. The names are worked out when the faces
  land or a name is given rather than on every draw, which would be a catalog
  read per frame, and each face carries where it is, so a face whose embedding
  the model could not produce cannot shift the names onto the wrong people.
- ✅ **LIB-015**: The library as rows — every thumbnail at
  its own shape, the rows filling the width (justified, Google Photos style,
  rather than masonry columns: rows keep the filter's order left to right, and
  columns do not). Selecting, the culling keys, the menu, export and
  opening work as they did in the grid. The shape comes from the thumbnail cache, so the
  rows are right when they appear; a photograph never shown before takes the
  camera's 3:2 until its thumbnail lands, and the view holds still when it
  does. See ENGINEERING.md, "Rows at the photograph's shape".

  Fixed 2026-09-17: scrolling the library sometimes ran away upwards and
  selected a stretch of photographs. The rubber band scrolls the view while it
  is dragged past an edge, and it stopped only on drag-end; a drag the toolkit
  cancels instead (the scroller claiming it, a touchpad click during a scroll)
  never sends one, so the band stayed and the scroll kept going. A cancel now
  ends the band too, and the scroll stops by itself once the gesture is no
  longer active, whichever signal was missed.
- ✅ **LIB-016**: The rows are the library; the square grid is gone, and with
  it the pair of buttons that switched between them. Asked directly, the
  photographer did not use the grid.
- ✅ **LIB-017**: People as places to go, in the library picker rather than as
  an "Everyone" picker in the filter bar. A person is not inside a library —
  a name reaches faces by likeness, across all of them — so everyone with a
  name is listed after the libraries, under a heading of their own (a section
  of the list, so the heading is not part of a row), and picking one shows
  every photograph they are in from every library that can be reached, in the
  filter's order. The rating, flag and sort filters still apply. The People
  dialog stays the place to name them, and works on the library last picked.
  Libraries sharing a parent folder are listed under its name, in the same kind
  of section: a library is a folder, so one trip photographed across several
  years is several of them, and a flat list of "2015, 2019, 2023" says nothing
  about where any of them was. A folder becomes a heading only when the
  libraries do not all sit in the same one — a single heading over the whole
  list is what the section already is — and never for a folder holding one
  library, which says nothing its own row does not. The cost of that first
  rule, named where it is made: one trip and nothing else reads as a bare
  "2015" and "2023" until a second trip is added.
  A sidebar on the left, the size of the editor's, holding all of it is for
  later — for now the photographer finds it more than the list needs.
- ✅ **LIB-005**: Copy and paste adjustments between photos, with a checklist of
  what travels: white balance, tone, colour, tone curve, detail, and crop. The
  crop is off by default — it is drawn against one photograph's content and
  rarely means the same thing on another. Pasting replaces rather than merges,
  so pasting an untouched edit resets those parts; anything else would make
  "make these match" depend on what the target already had. Copy from the
  editor (Ctrl+C), paste onto a grid selection or the open photo (Ctrl+V, or the
  right-click menu for the checklist). The rules live in `Document::copy_from`
  and are tested there rather than in the UI.
- ✅ **LIB-006**: Presets — create, apply, import, export; scoped to parts of
  the stack the same way LIB-005 is. A Presets submenu on the grid's right-click
  and a Presets tab (the star) in the editor's panel, each a list to search and
  click, applying to the selection or the open photograph — the camera's facts
  that tab used to hold are a popover on the editor bar; save the open
  (or first selected) photograph's edit with the same checklist as pasting;
  import files or whole folders. A preset is one JSON file in the data folder,
  in a subfolder per group, holding only the parts it carries — no path, spots
  or faces — so exporting, renaming and deleting one are the file manager's,
  reached from "Open presets folder". A name already taken is refused, not
  replaced. The presets are not rows in the menu itself: a GTK menu builds
  every row at once, and 3500 imported ones held the window back for over
  forty seconds.

  **Presets do not stack.** Picking a second one is changing your mind about
  the first, not asking for both — so a preset is applied to the stack as it
  was *before* the one on it now, which the open photograph remembers from the
  moment the first one lands. Before this, a second preset settled on top of
  the first wherever it had nothing to say: a tone-and-colour look followed by
  a colour-only one left half of each, with nothing to say which half. The
  memory is cleared the moment the photographer changes anything by hand —
  after that the picture is theirs, and the next preset builds on it. A paste
  is unchanged and still lands on what is there: a paste is something you
  built and aimed, a preset is a look.

  **Resting on one shows it.** The canvas renders the open photograph with
  that preset, from the same proxy the ordinary preview uses, so what hovering
  shows is what clicking gives — including when another preset is already on,
  because the preview reads the same baseline. Nothing is written. A hover
  books a render 160 ms later rather than starting one, and only the newest
  booking counts: a pointer crossing the list walks every row on the way.
- ✅ **LIB-020**: Lightroom and Capture One presets, imported and translated —
  Lightroom Classic `.lrtemplate`, Lightroom `.xmp`, Capture One `.costyle` and
  `.costylepack`. Tone, presence, dehaze, vignette, grain, vibrance and
  saturation, the eight-band HSL mixer, calibration, colour grading or split
  toning, white balance, sharpening and noise, point curves per channel, and
  Lightroom's parametric curve laid over them as points; Capture One's
  gradation curves, colour balance per range as grading, film grain and
  recovery sliders. Each preset lands in a group named for its folder or
  pack. What has no counterpart — local adjustments, camera profiles, lens
  corrections, a black-and-white mix, Capture One's advanced colour editor —
  is listed after the import with how many presets used it. Close rather than
  the same: nothing here was fitted against Lightroom's own render. Of a
  collection of 4383, 3844 translate; the rest are brush presets or hold only
  local, lens or reset settings. The whole collection imports in 0.2 s.
- ✅ **LIB-021**: Presets as thumbnails of the photograph itself, two to a
  row, rendered on its proxy and cached under `~/.cache/numa/` keyed on the
  pair. A list of names asks the photographer to remember what forty names
  look like on this frame; a grid of the frame answers it. PANEL_PLAN P3.

  Both halves are built. Resting on a preset renders it onto the canvas (see
  LIB-006), which answers the question for the one under the pointer; the tab
  itself is a grid of two columns where every card is this photograph with
  that preset on it.

  A card is developed from a 360-pixel copy of the open proxy rather than
  straight at 130: a thumbnail rendered at 130 sizes every detail pass against
  a frame a twentieth of the real one, and what a preset does to sharpening
  would be invisible in the one place it is being judged. **6 ms a card**,
  measured over 65 of them, so the phase's two seconds for forty is 0.24 s.
  One card per idle, only for the rows on screen, so scrolling never waits for
  a render.

  The cache is in memory rather than on disk, which is a departure from the
  plan and cheap to revisit: at 6 ms the rebuild costs less than reading forty
  files would. It is thrown away when the Presets tab is shown and the
  photograph or its stack has changed since — not when a slider moves, because
  re-rendering forty frames on every tick of a drag is work nobody is looking
  at. And the stack a card is laid over is the one a *click* would land on —
  the stack as it was before the preset now on the photograph — so a card is a
  promise the click keeps.
- ✅ **LIB-022**: A photograph that has been worked on shows the edit in the
  grid and the filmstrip, rather than the camera's own picture — seeing the
  render is what says it has been edited, and by then the original is the
  less interesting of the two. Kept beside the ordinary thumbnail and keyed on
  the stored stack as well, so changing an adjustment makes a new picture and
  nothing else does; a photograph with no edits keeps the thumbnail it had and
  costs nothing. The camera's picture is shown first and replaced when the
  render lands, so no card is blank while one is running, and only one renders
  at a time — it is a decode of the whole raw, about 0.7 s for a 40 MP frame
  against 40 ms for the camera's preview. The loupe keeps showing the original.
- ✅ **LIB-007**: Batch export — pick a shoot in the grid and it goes out in
  one go, from the same pair of buttons the editor has: the button repeats the
  last answer, the arrow asks again, and the label counts what is selected,
  because "Export 32" is also the confirmation that there are 32 of them. A
  shoot is edited one frame at a time and written out all at once, so the
  button belongs in both places. The stacks are read from the catalog rather
  than from anything on screen.

  A stack read out of the catalog carries no mask pixels, because those are
  never stored, so they are worked out again before the frame is developed:
  without it the masks on an exported photograph did nothing at all, and an
  inverted painted one did its adjustment to the whole frame. The editor's own
  export was always right, which is how two buttons came to disagree about the
  same file. Both call one function now.

  One photograph at a time rather than in parallel: a full-resolution decode
  and render already uses every core, and forty at once is a machine out of
  memory rather than a faster export. The rest of the batch is elsewhere and
  done: applying settings to a selection is LIB-005's paste, and rating one is
  LIB-003 — 0–5, P, X and U act on everything selected, as does the
  right-click menu's Rating.
- ✅ **LIB-008**: Albums, independent of folders. An album can hold photographs
  from any library. Albums are listed in the library picker under a heading of
  their own, between the libraries and the people, and picking one shows its
  photographs with the filter's rating, flag, file type and sort still applying.
  The grid's right-click menu adds the selection to an album or a new one, and
  removes it from the album on screen; Albums… in the main menu renames and
  deletes them, asking first. The names live in the application's own catalog;
  which photographs are in an album lives in each library's catalog (IO-014),
  so it moves with the folder and a photograph gone from disk leaves its albums
  with it. Library ids, library paths and photo row numbers all proved
  unstable as references across libraries — SQLite reuses ids, drives remount
  elsewhere — which is why membership is kept on the library's side.
- ✅ **LIB-009**: Manage the libraries themselves — a list with photo counts,
  add, remove, rename, and the one that was open last reopening on start.
  Removing one only stops showing it: every rating, flag and edit stack is in
  the folder's own catalog (IO-014), so adding the folder back brings all of it
  back, which is what the confirmation says. Still missing: reaching a
  subfolder without adding it as its own library, which is LIB-010.

  A folder moved or renamed outside Numa reads as "Not connected", and that row
  alone offers **Find this folder where it is now**: a folder picker, and the
  catalog's note of where to look is changed. Nothing on disk is touched, which
  is why it cannot lose anything — and it beats the only way out of this
  before, which was removing the row and adding the folder again. A folder
  another row already holds is refused, since the same library twice is one
  catalog under two ids. Tested in `tests/smoke.rs`: a folder renamed under
  Numa's feet, relocated, with its five-star rating still on its photograph.

  **A row's title is Pango markup, and a path is not.** A folder called
  "Chili & Argentinie" made GTK parse an entity that never ends, so it dropped
  the whole title and the row showed nothing at all — the photographer found it
  in the log, with the line repeated for every refresh. Titles that carry a
  path, a file name or something typed now turn markup off rather than escape
  it, so what is shown is exactly what was passed and a name read back off the
  row has no `&amp;` in it; subtitles, which nothing reads back, are escaped,
  since this libadwaita has no switch for them. Toasts are markup too and are
  escaped in the one function every one of the eighty of them goes through.
- ✅ **LIB-010**: Folders inside a library, navigable as a tree. A folder picker
  in the filter bar lists every folder under the library that holds photographs,
  and the ones above them, indented by depth — so a shoot is picked by itself
  or by its year — and narrows the grid to it and everything under it. Hidden
  when the library has no subfolders and in views that span libraries; a
  folder of one library is forgotten on switching to another.
- ✅ **LIB-011**: One view across every library. "All libraries" heads the
  picker when there is more than one, and shows every reachable library's
  photographs as one grid, narrowed and sorted as one library is. Rating, flags,
  opening, export, pasting and presets work there as anywhere. Rescan walks
  every reachable library; photographs dropped onto the window are copied in
  only once a single library is picked. Unplugged drives are left out.
- ✅ **LIB-012**: Notice files appearing and disappearing without a manual
  reload. The open library is walked again whenever the window comes back to
  the front — which is what follows copying a card in a file manager — and
  once a minute besides. The walk runs off the main thread, so a network share
  does not hold the window; the grid is rebuilt only when something changed,
  at the same scroll position, and in the editor it waits until the grid is
  shown again.

  A folder that is not there is a state rather than an event, and is said once.
  Renaming folders outside Numa left it walking paths that had gone, failing
  and reporting it twice over — once from the walk and once from applying it —
  on every return to the window and every tick of the minute timer, which is a
  log nobody can read. Now the walk is skipped while the folder is missing, the
  line is logged the first time only, and the library list says "Not
  connected"; if the folder comes back and goes again, it is said again.

### CULL — Assisted culling

Ten thousand frames from a trip is the case this exists for. The principle for
the whole group: **a suggestion is never an edit.** Scores and flags live in
their own columns and drive a sort and a filter — the stars stay whatever the
photographer put there, and nothing here ever writes one.

Most of culling is not machine learning, and doing the cheap deterministic part
first is what makes the rest worth having: a model that re-discovers "this one
is out of focus" is a slow way to compute a number we can get exactly.

- ✅ **LIB-018**: Photographs shot as RAW and JPEG (or HEIF) at once. A camera set
  to write both leaves two files per frame, and the library shows both, side by
  side, as two photographs. Raised on 2026-09-17 with a newly added folder. The
  photographer's choice: both stay cards of their own, and a file-type filter
  beside the flag filter narrows the grid to the RAWs or to everything else.
  A rating or an edit is on the file it was given to, as for any card.
- ✅ **LIB-019**: Renaming a library the way it was meant. The Libraries dialog
  renamed what the picker shows (LIB-009), which is not what was wanted for a
  folder that was named wrongly on disk. Now the rename is of the folder itself,
  `.numa` and all, after a question that says other programs see it too. Within
  the same parent only, so it is one atomic rename; a name already taken is
  refused. The display name it replaces is cleared.
- ✅ **CULL-001**: Sharpness and clipping per frame, measured not inferred.
  Laplacian energy over a standardised luma at a fixed size, so the number is
  independent of exposure, contrast and megapixels — without that it ranks
  contrast at least as strongly as focus. Thresholds measured against a 10 729
  frame library rather than taken from a paper: soft below 0.25, which was about
  one frame in twelve there, where the first value tried would have flagged 45 %.
  That intent — the softest one in twelve — is what the threshold now is, read
  off the library being looked at rather than fixed. Held against two real
  libraries the fixed number flagged 10 % and 13 %, because both hold about two
  thirds of the reference's fine detail; on their own scale both come back to
  8 %, which is 226 fewer photographs handed to the photographer to look at
  again in the larger of the two. A library with too little measured to have a
  distribution keeps the fixed number.
- ✅ **CULL-002**: Burst grouping by perceptual hash, and the best frame of
  each burst by CULL-001. Fifteen frames of the same scene is where the time
  actually goes. The hash is a 64-bit difference hash written here rather than
  pulled in — it is sixty-four gradient comparisons, which is less code than
  the dependency line. Which frame is the best of its run is worked out when the
  grid is read rather than stored: a flag written once goes stale the moment the
  rule behind it changes, and it did — an earlier version called the only frame
  of a run of one that run's best, which put the label on 1936 of 2337
  photographs. A run is a stretch of the order the catalog hands over, and
  since IO-005 that is capture order rather than file order: an archive copied
  without `-p` arrives in whatever order the copy wrote it, and runs were being
  cut out of an order nothing had ever been photographed in. Two bodies
  shooting one event still interleave, but that is the interleaving of two
  clocks now rather than of two copy operations.
- 🟡 **CULL-003**: Faces, from YuNet (OpenCV Zoo, Apache-2.0, 232 kB) through
  `tract`. Turns the frame measure into the one that matters for a portrait: a
  crisp background with a soft face scores well on CULL-001 and is a reject.
  Measured on real frames — one at frame 0.53 with a face at 0.20, another the
  other way round at 0.22 and 0.41. Run `dev/fetch-models.sh`; without the model
  the columns stay null, which the grid reads as "not looked for" rather than
  "nobody here". Eyes open/closed is **not** built: YuNet gives eye positions,
  not eyelids, and that needs a second classifier there is no trustworthy small
  one of.
- 🟡 **CULL-004**: A suggested rating, 0–5, shown beside the photograph and
  sortable. It is a **rule, not a model**, and that is a deliberate retreat from
  the original plan. The plan was a NIMA-class network; the finding is that no
  small, general, well-provenanced ONNX one exists to depend on — what is
  published is anime-tuned, unlicensed, or a diffusion model, and scoring
  someone's photographs with a black box of unknown training is worse than not
  scoring them. So: the sharpness that counts (the face's when there is one),
  less up to two stars for blown highlights, plus a nudge for the pick of a
  run. The five stars are stretched between **this library's own** fifth and
  ninety-fifth percentiles of sharpness, not a reference library's: fixed ends
  scored a photographer whose frames hold less fine detail than that reference
  — wide apertures, fog, a lot of smooth sky — a star or two low across
  everything they own. Under fifty measured frames, or ends too close together
  to divide by, the reference scale stands. Every term can be argued with, and
  the tooltip shows the working. The real answer is CULL-005.
- 🟡 **CULL-005**: A score learned from this photographer's own ratings. No
  general image embedding exists in the pipeline, so it is a ridge regression
  over what Analyse measures — the rule itself, frame and face sharpness, blown
  highlights, faces, best of burst — and the frame's brightness, contrast and
  colourfulness, now measured too. Stars are the labels, a reject is 0, unrated
  is left out. Nothing is fitted below 30 ratings or three distinct verdicts;
  past that a quarter of the rated bursts are set aside, and the learned score
  replaces the rule only when it ranks them better. On a synthetic taste: ρ 0.95
  against the rule's -0.05; stars that follow the rule keep the rule. Not yet
  measured on a real library — the ones here hold eight ratings between them.
  Measuring tone bumped the analysis version, so the next Analyse runs afresh.

**Which library.** `ort` for inference, which is ONNX Runtime linked
statically, so the AppImage is still a single file. It began as `tract-onnx`,
pure Rust, and moved when the models were measured to be six to ten times
slower there and one of them would not load at all — `docs/ENGINEERING.md`,
"Inference runtime", has the table and the way back. `image-hasher` for the perceptual hashing in CULL-002 (the
`img_hash` it forked from is unmaintained). Nothing in CULL-001 needs a
dependency at all.

### DOC — Document and history

- ✅ **DOC-001**: Document state and camera profile.
- ✅ **DOC-002**: Non-destructive operation stack, stored in the catalog.
- ✅ **DOC-003**: Undo/redo, covering adjustments, white balance and geometry.
- ✅ **DOC-004**: Every step, kept with the photograph in its library's
  catalog — up to a hundred, the oldest going first — so closing it and coming
  back carries the history on rather than starting at "Opened" with the crop
  already done. An edit changed in the grid meanwhile, by a paste or a preset,
  is a step of its own on top; a photograph edited before histories were kept
  starts at "Original". Every step, in a popover beside the two arrows that walk it —
  which is where it belongs: the list is read when something went wrong three
  edits ago, not while working, and a seventh tab across a panel that fits six
  by the pixel is not free. Each step is named after what it changed rather
  than numbered, because "Step 7" tells a photographer nothing and the
  difference between two snapshots is exactly the thing they did: "Exposure",
  "Crop", "Drew on Sky 1", "Camera profile", "Exposure and Shadows". Newest
  first, the one on screen marked, the ones after it dimmed but still there —
  an undo that can be undone is the difference between a history and a warning.
  Clicking one goes straight to it.
- ✅ **DOC-005**: Snapshots — named versions of one photograph's edit, kept in
  its library's catalog beside the history and gone with the photograph. They
  live in the history popover, under the steps, because both answer "take me
  back to how it was": save the edit on screen under a name ("Snapshot 1" when
  none is given), and each one is a row with its date. Clicking one puts it
  back as an ordinary edit — one step, named after the snapshot, that undo
  takes off again. A name already taken for that photograph is refused, not
  replaced, as a preset's is. Deleting is at once, with Undo on the toast. The
  step's name lasts the session; reopened, it is named by what it changed.

### CANVAS — Rendering and navigation

- ✅ **CANVAS-001**: Render the active document.
- ✅ **CANVAS-002**: Zoom as one control rather than four. It was minus, plus,
  a readout, and then two more buttons for Fit and 1:1 — five things in three
  groups, all saying "zoom" and none of them saying which. The readout is the
  control now: it is the button, it shows where you are, and the places worth
  going are in the menu behind it. Minus and plus flank it, and the three read
  as one because they are.

  Old **CANVAS-002**: Pan and zoom, wheel-proportional, floored at fit.
- ❌ **CANVAS-003**: ~~Checkerboard transparency background.~~ *Dropped: nothing
  here is ever transparent. A photograph is opaque, the crop shows the frame
  beyond its edge rather than a hole, and there are no layers with gaps. A
  checkerboard would be a pattern behind something that always covers it.*
- ✅ **CANVAS-004**: Guide overlays — none, thirds or an eight-part grid, from a
  button on the editor bar or G. Drawn over the photograph's own rectangle,
  dark under light so they read on sky and shadow alike, and taking no clicks,
  so every tool works through them. *No rulers or draggable guides: a photo
  editor's question is a horizon or a composition, which these answer.*
- ✅ **CANVAS-005**: High-DPI correctness. A zoom is photograph pixels per
  *screen* pixel, so 100 % is one to one at any display scale: it was one per
  layout pixel, which at a scale of two drew every pixel four times and made
  "100 %" useless for judging focus. The canvas is sized in layout pixels from
  that, Fit reports the real ratio (30 % where it said 15 %), zooming keeps its
  centre, and the render resolution was already asked in screen pixels. The
  grid's thumbnails follow the scale too (LIB-004). Tried with GDK_SCALE=2.
- ✅ **CANVAS-006**: Reference view — a second photograph beside the one being
  worked on, which is how a set is matched to the frame that is already right.
  Reference in the editor bar keeps the frame on screen, developed as it is at
  that moment, in a pane sharing the canvas's room; stepping through the
  filmstrip then compares each frame against it, and the pane says which frame
  it is holding. Off again, or leaving the editor, puts it away.

  It costs one render. The reference is not decoded or re-read: it is
  `apply_stack` over the proxy already in memory, the same 30 ms a render tick
  pays, and after that comparing is free — a texture beside the canvas, with no
  work per step. Masks are resolved into it first, so a frame with local
  adjustments is the reference as it looks rather than as it would look without
  them.

  Not the same thing as UX-007's Before, which is this photograph as it came
  off the camera. Equal halves rather than natural widths: a 2400-pixel picture
  in a box that hands out natural sizes first took nearly the whole room and
  left the canvas a strip.

  **It follows the canvas.** It did not, at first — the pane fitted its whole
  frame and stood still whatever the canvas did, so zooming in to compare two
  frames' detail compared a magnified crop against a postage stamp. Now every
  pan and zoom re-places it: the pane is a window at the canvas's magnification
  and the reference is laid into it. Because it is a *different* photograph,
  "the same edge" is meaningless and the same **fraction of the frame** is what
  a comparison wants — so its whole frame is laid out at the size the canvas
  gives its own, and the fractions then line up whatever the two resolutions or
  shapes are. Measured at 1:1: correlation 0.9995 between the two halves, scale
  1.002.

  It stays a proxy render, so above about 31 % it is the reference magnified
  rather than the reference at full size. Deliberately: the full-resolution
  frame the canvas keeps for 1:1 (PERF-002) belongs to the photograph that is
  *open*, and there is nothing of the reference in it — matching it would mean
  a second 1.2 s decode and half a gigabyte for a picture nobody is editing.
- ✅ **CANVAS-007**: Nearest-neighbour scaling above 1:1, so magnified pixels
  read as pixels rather than as a soft photograph.
- ✅ **CANVAS-009**: Double-click the photograph for 1:1, double-click again
  for where you came from. Judging sharpness means 100 %, and the ways there
  were the wheel and the plus button — both of which leave you somewhere
  between the two, and neither of which comes back. The point under the
  pointer is what lands in the middle, because "that bit, at full size" is the
  question a double-click on a photograph asks.

  The gesture sits on the canvas, under the tool overlays, so a crop handle or
  a mask stroke never becomes a zoom, and it stands down while a pipette is
  armed — two clicks would otherwise pick a colour twice on the way. "Already
  at 1:1" has a little room in it: the wheel lands on 0.9998 as easily as on
  1.0, and a double-click there means "take me back" rather than "stay".

- ✅ **CANVAS-008**: Under a tile there is now the whole frame at proxy size, so
  a view the renderer has not caught up with is a soft photograph rather than a
  cropped one — which was the complaint, and the right one: a crop says
  something about the picture, blur says something about the renderer.

  It costs nothing to produce. Whenever the canvas shows a fragment, the
  histogram already renders the whole frame at proxy size and throws it away,
  because a histogram describes the photograph rather than the part on screen.
  That render is the backdrop. Measured on a 40 MP frame at 300 % with the tile
  live: 22.3 ms a tick before, 22.9 ms after — the texture is wrapped without a
  copy and uploaded by the time it is drawn.

  Seen rather than reasoned about: rendering the paintable on its own produces a
  900 x 1350 node with the backdrop and a 306 x 459 one without, which is the
  bug exactly — the node was only ever as big as the tile.
- ✅ **CANVAS-010**: The camera's own rendering as a second source for the
  Reference pane. "Camera" beside "Reference" in the editor bar, one pane, one
  picture at a time.

  **It is not UX-007.** "Before" is Numa's render of an empty document — the
  base curve, the film simulation, sharpen 25 — so it cannot answer "is my
  render softer than the camera's": it *is* my render, at a different
  resolution and off a different demosaic. This is the JPEG the camera embedded
  in the raw file, shown as the camera wrote it: no `apply_stack`, no tone, no
  film simulation, no sharpening, sRGB as Fuji stores it, and never through the
  proxy's box filter. It is the only thing on screen that is not ours.

  It follows the canvas's zoom and pan, so at 1:1 the same edge is on screen in
  both — through the same placement CANVAS-006 now uses, because a pane that
  syncs for one of its two sources and not the other is a difference nobody can
  explain. The pane is treated as a window at the canvas's magnification and
  the camera's picture is laid into that window — that way round, because what
  Fujifilm embeds is a *preview*, not the JPEG it writes to
  the card: 2944 x 4416 of an X-T5's 5152 x 7728, 1920 x 1280 on an X-T20. So
  the camera's side is enlarged x1.75 or x3.12 to meet the canvas, and the zoom
  readout says which, the way it already says "soft". At fit both simply show
  the whole frame, since a viewport there would cut a strip off for the
  caption's sake at a magnification nobody judges sharpness at.

  Measured at 1:1 on 1920x1080, by cross-correlating a column profile of the
  two halves of the screenshot: scale 1.002 and correlation 0.997 on the X-T5,
  1.004 and 0.981 on the X-T20, with the centres 13-14 px apart against the 12
  px predicted by the pane being 744 px wide where the canvas viewport is 768.
  The same edge, at the same size, in both.

  It costs one JPEG decode per photograph — 125 ms on an X-T5 file, off the
  main thread, and thrown away when the pane is put away. Panning and zooming
  cost nothing after that: the frame is one texture and only its placement is
  recomputed.

### RENDER — Pipeline and colour

- ✅ **RENDER-001**: Proxy pipeline — edit at canvas resolution, export at full.
- ✅ **RENDER-002**: Scene-referred linear working space.
- ✅ **RENDER-003**: One operation stack shared by preview and export.
- ✅ **RENDER-004**: Base tone curve, fitted against the camera's own rendering.
- ✅ **RENDER-005**: DNG camera profiles (`.dcp`) — forward matrix, hue/sat map,
  look table. Exact model matches only; see the README for why. Which profile is
  a choice rather than something decided for the photographer: the Colour tab
  lists every profile installed for the body, plus "none", which is the colour
  matrix on its own. The list is the same scan the automatic match uses, so it
  can never offer something the renderer would refuse.

  ❌ *The dropdown leaves the panel in PANEL_PLAN P1. Automatic is right on
  every frame the photographer has, and "none" is a diagnostic — FT-012 used
  it and found the profile moves the baseline by 0.003 EV. A stack that
  carries `None` still renders as `None`, and the history line still says so.*

  A profile's look table is applied now, where it used to be thrown away on the
  grounds that RENDER-006 owned the look — which left every profile rendering
  without the half of itself that carries its character, for as long as
  RENDER-006 stayed switched off. It is what makes Adobe's *Camera Matching*
  profiles render as the camera's own modes rather than as Adobe Standard under
  another name, which is the film-simulation problem solved from the other end.
  Choosing a film simulation still wins: two looks over one another is neither
  of them. Measured on the X-T5's Adobe Standard, whose look is a correction
  rather than a simulation: up to 6 of 255 on greenery, nothing on a neutral.
  72 of RawTherapee's 161 bundled profiles carry one. `None` in the document
  means the automatic match, and a name that no longer resolves falls back to
  it rather than rendering something else in silence.
- ❌ **RENDER-006**: ~~Film simulations from third-party look tables.~~
  *Withdrawn.* It sat behind a switch for weeks: the tables did not match what
  the camera produces closely enough to trust, and a look that is nearly right
  makes every other colour judgement suspect. They were also CC BY-NC-SA, a
  constraint on the whole program for a feature that was off. The picker, the
  loader and the fetch script are gone. What stays is the reading half — the
  `FilmMode` code agrees with exiftool on all 10 729 frames of the reference
  library, and the info panel reports what the camera was set to — and the
  `film_simulation` field on the document, so stored edits still load. A
  camera profile's own look table (RENDER-005) still applies; that is the
  profile's rendering, not a simulation over it. If simulations come back it
  will be from tables fitted here against the camera's own JPEGs, which is the
  method every other colour decision in this program was settled by.
- ❌ **RENDER-007**: ~~Classic Negative, Nostalgic Neg and Bleach Bypass as
  cube LUTs.~~ *Withdrawn with RENDER-006.*
- ✅ **RENDER-008**: Per-image exposure matched to the camera's own rendering.
- ✅ **RENDER-012**: Fujifilm X-T5 profiles fitted here, against the camera's
  own JPEGs. `tests/camera_profile_fit.rs` takes X-T5 frames of one film
  simulation at Color 0, pairs Numa's matrix-only rendering with the embedded
  JPEG (at 300 px, tone curve inverted, clipped, dark and edge pixels skipped),
  fits a smoothed, identity-damped hue/saturation/value map (36×8×4) and writes
  it as "Numa X-T5 <film>.dcp" — Numa writes DCPs now as well as reading them.
  A quarter of the frames are held out. Held-out mean chroma error against the
  camera JPEG, 12 frames each: Classic Chrome 0.0687 matrix → 0.0310 fitted
  (Adobe Standard 0.0581), Eterna 0.0344 → 0.0126 (Adobe Standard 0.0303),
  better on every held-out frame — optimistic, since those frames share trips
  with the fitted ones. Looks at Color 0, not a neutral profile: that still
  needs Provia reference frames. Classic Negative, Reala ACE and Nostalgic Neg
  have no Color 0 frames in the library. Run with `RAF_DIR` and `FILM`, and
  `OUT` to install.

  Two SILKYPIX Provia TIFFs were measured on 2026-09-17. They showed that its
  saturation is a multiplier (1.00 neutral, 0 is monochrome) separate from
  Fujifilm's Color −4..+4, and that the converter starts from the recipe the
  frame was shot with. Chroma error against the neutral one (DSCF2167): matrix
  0.093, Adobe Standard 0.081. Two frames are not a result. The reference set
  is being shot instead: RAW+JPEG on Provia with every tone and colour setting
  at neutral (Color 0, Highlight and Shadow 0, DR100, Clarity 0, grain and
  Color Chrome off, sRGB), across daylight, shade, golden hour, tungsten, LED
  and fluorescent light, with skin, sky, foliage, saturated reds, oranges,
  yellows, blues and purples, and neutral greys and whites. Profiles fitted from our own frames can ship. This
  is also how RENDER-006's film simulations would come back. *Parked by the
  photographer on 2026-09-17, to return to.*
- ✅ **RENDER-014** (the brief's A1): photographs in the screen's own colour.
  colord's default profile for the primary display — the automatic EDID one
  GNOME makes, or the one chosen in Settings › Colour — read at start and
  again when colord or Mutter says a screen changed (`ui::display`); its
  primaries and curves turn every frame's sRGB into the screen's numbers at
  the moment it becomes a texture (`render::display`): the canvas, the
  thumbnails, the loupe and compare, the reference. After the histogram,
  which describes the photograph and not the screen; not the clipboard or
  any file, which stay sRGB. On this machine's Acer X32Q FS, whose red is a
  third further out than sRGB's, sRGB red goes out as 214, 63, 38 and a skin
  tone of 220, 170, 140 as 205, 170, 143; grey and white stay put. 5 ms a
  4 MP frame.

  Nothing is converted with no colord, no profile, one that is sRGB (every
  code within one of itself — so byte for byte what it was), one made of
  lookup tables (which Preferences says), or when Mutter is doing it itself:
  its sdr-native and HDR colour modes map every window to the screen, and the
  default mode does not — GNOME applies a profile itself only when it is a
  measured calibration one, and then colord reports sRGB. A test gives a
  Display P3 and an Adobe RGB profile as the screen's and gets the colour
  spaces' own conversion of eight patches within one code value. Preferences
  › Colour says which: "Display: Acer Technologies 32" (automatic)".

  Screenshots taken headless under Xvfb are converted too, since the profile
  is the real screen's.
- ✅ **RENDER-013**: Camera profiles in every package, and a predictable
  Automatic. The AppImage bundles RawTherapee's profiles; the Flatpak does not,
  and its sandbox cannot see `/usr/share/rawtherapee` or `~/.local/share/numa`,
  so there it finds none at all. And with more than one profile installed for a
  body, Automatic takes whichever file the directory lists first — so a special
  purpose profile dropped in (an infrared one, say) can silently become every
  photograph's colour. Automatic should prefer the standard profile for the
  body and leave the others to be chosen.

  Half built: RawTherapee 5.13's 161 profiles are on the `models` release as
  one archive with the GPL-3.0 text and a README, downloadable from Preferences
  and offered on first start when no profiles came with Numa or are installed.
  They unpack into their own folder, apart from the user's, which a Flatpak
  can read as well, so it needs nothing bundled.

  Automatic is predictable now: it takes Adobe Standard, or what RawTherapee
  ships for the body whatever that profile is called ("X-Mod" for the X-E2),
  or a profile in the user's own folder named after the body — and never a
  look, an infrared profile or anything else put beside them, which stay in the
  picker to be chosen. Measured on every installed profile: no body lost its
  automatic one. The same profile in two folders is listed once.
- ✅ **RENDER-011**: Clipped highlights stay neutral. White balance scales red
  and blue up and leaves green alone, so three channels that all stopped at the
  sensor's ceiling leave it as (1.8, 1.0, 1.9) — magenta. It hid at normal
  exposure, where everything above white encodes to white anyway, and appeared
  the moment the exposure came down, which is exactly when a photographer is
  looking at a blown sky. It was visible at 0 EV too: a pink sky. A channel that
  ran out is a lower bound rather than a measurement, so it is lifted to the
  dimmest channel still measuring — per channel, since green saturates first on
  this sensor. A saturated red whose red channel clipped is already its own
  brightest, so it does not move; that is what lets this run without turning
  deep colour white.

  That was the first version and it was half a fix: red and blue are already
  *above* the other channels when they clip, so holding them up to the dimmest
  one did nothing to them, and a sky that clips one channel at a time came back
  in bands of cyan and pink instead of in magenta. What works is the other
  direction — a blown pixel is held *down* to the lowest of the three ceilings,
  which costs the three quarters of a stop of red and blue that was only ever
  an artefact of the white balance, and leaves saturated colour alone because
  its other channels are nowhere near the ceiling to be held to it. Measured on
  a blown frame at −5 EV: 15.4 % of it carried a cast in the highlights, now
  0.0 %, with frames that clip nothing untouched to the last bit. Estimating
  the light that was really there is reconstruction, and not this.
- ✅ **RENDER-009**: Colour space — four of them, chosen for the file. sRGB,
  Display P3, Adobe RGB and ProPhoto RGB.

  **The export renders in the space it writes** (P4, 21 September). With the
  working-space picker gone from the panel every photograph was edited in
  sRGB, and the colour stage clips to the working space — so a Display P3 file
  was sRGB's colours written in P3's numbers, 0.0 % of any corpus frame
  outside sRGB. Now a photograph edited in sRGB is rendered in the export's
  own space (`Document::set_output_space`): on the camera corpus 0.1–3.9 % of
  the frame lands outside sRGB, which is the saturated colour the sensor saw.
  What sRGB can hold barely moves — a median ΔE of 0.15–0.6 in P3 and Adobe
  RGB (p95 0.6–2.4), 0.5–1.6 in ProPhoto — because the look is a per-channel
  tone curve, and the same curve in wider primaries is very nearly the same
  picture. The screen, and an sRGB file, are the arithmetic they always were.
  The mixer's table is read in the pixels' own space, so its bands name the
  same colours in any of them. A stack stored with a space of its own, from
  when the picker was on the Colour page, still renders in it; the picker's
  dead code is gone.

  ❌ *The picker leaves the panel in PANEL_PLAN P1 for the export dialog, where
  the other decision about the file already lives, and where it can be tagged
  with an ICC profile (brief A2). The working space stays with the edit; what
  goes is the row, not the choice.*

  Two separate decisions and they are made in two places, because they are not
  the same question. The **working space** sits beside the camera profile on
  the Colour page, because both decide what the numbers after the camera matrix
  mean rather than what to do with them; it is stored with the edit, since a
  stack read back in a year has to be rendered in the space it was made in. The
  **output space** is in the export dialog, where the question is what the file
  is for.

  One rule held the whole design: **sRGB in and sRGB out is the arithmetic it
  always was**, and there is a test that asserts it byte for byte. A colour
  space setting that quietly changed the default render would be a change to
  every photograph already edited, dressed up as a feature — and every constant
  in this pipeline was fitted against the camera in sRGB. So the conversion is
  one matrix after the colour stage rather than folded into the profile, and
  `convert_to` answers `None` for a space to itself, so the default path does
  no multiply at all.

  The negative that used to be clamped at the matrix is kept until after that
  conversion, which is the point of a wider space: a colour the sensor saw and
  sRGB cannot hold now survives into one that can.

  Writing a file is three steps and not one, because the base tone curve is a
  *rendering* rather than a transfer function — it was fitted against the
  camera's own JPEGs, so what comes out of it carries sRGB's encoding in the
  working space's primaries. Undo that encoding, turn the primaries, put the
  target's own encoding on. sRGB to sRGB cancels exactly.

  **Still half built**, for two reasons. The luminance weights each space
  carries are used by the tone regions but not yet by the detail passes, which
  still use sRGB's — a second-order error, largest on ProPhoto, and listed here
  rather than hidden. And the working space has no picker since P1 moved the
  row out of the panel: a stack keeps the space it was made in, and a new one
  is edited in sRGB.

  Every export carries an ICC profile for its space, sRGB included. They
  went out untagged, which every viewer reads as sRGB — the drained wide-gamut
  photograph this section warns about, happening to the files it was meant to
  protect. The profiles are written by `io::icc` from the same primaries and
  curves the render uses (ICC 2.1, matrix and TRC), and littleCMS converting
  (200, 100, 50) out of each of the four agrees with Numa's own matrices to the
  code value.
- ◻️ **RENDER-010**: GPU pipeline, if the CPU one ever stops being enough.

### OPTICS — Lens corrections

- ✅ **OPTICS-001**: Vignetting from the camera's own lens profile.
- ✅ **OPTICS-002**: Distortion and chromatic aberration, from the camera's own
  tables — the same FujiIFD OPTICS-001 reads, so all three corrections are one
  profile and one file read. Both are applied in one resampling pass: every
  destination pixel reads from a radius of its own and the three channels read
  from three slightly different ones, which is the same arithmetic twice if they
  are done separately.

  The three tables do not always agree about where they were sampled. X-Trans IV
  and V state nine radii for all three; X-Trans III states eleven for falloff and
  distortion and ten of its own for aberration. The reader asked that the
  aberration table match the falloff table's radii, which meant every X-T20 frame
  had its aberration table read, rejected and dropped without a log line — 60 of
  60 sampled in each of two 2022 folders. It is now resampled onto the profile's
  radii instead, and 60 of 60 keep it. What it corrects is small: on the X-T20 the
  correction is 0.33 px outward for red and 0.66 px inward for blue at the very
  corner, measured on a rendered frame as 0.16 and 0.41 px at r = 0.93 against a
  green channel that does not move. Nothing on an X-Trans IV or V frame changes —
  the rendered bytes are identical.

  Which way the correction goes was measured against the camera's own corrected
  JPEG rather than read off: each direction given its own best global scale,
  because that JPEG is cropped against the raw frame, and then judged on how
  closely the edges agree. The table's sign as stored more than halves the
  difference on all three lenses tried; reversing it is worse than doing
  nothing. Pincushion reads from outside the frame at the corners, so the map is
  pulled in until it does not — the same slight crop the camera takes — and the
  corrected frame then matches the camera's field of view at a scale of 1.000.

  The colour half is smaller and less certain: the largest value in 3925 frames
  is 0.0011, five pixels at the corner of a 40 MP frame, read as a fraction
  because as a percentage it would be a twentieth of a pixel. The direction is
  confirmed against the uncorrected frames; the magnitude was not resolvable
  with the lenses to hand. See the README.
- ✅ **OPTICS-003**: Defringe — take the purple and green off a high-contrast
  edge.

  Chromatic aberration is the one lens fault a lens profile cannot fix: the
  profile knows where the three channels land, not that one of them arrived
  the wrong colour. A fast lens wide open does not focus every wavelength in
  the same plane, so a branch against a bright sky comes back with a violet rim
  on one side and a green one on the other.

  Two questions, and a pixel has to answer both. *Is there an edge here*,
  measured on brightness in log so a coloured rim cannot declare its own edge
  and so a step in the shadows counts as much as one in the highlights. And
  *is this a colour a lens invents rather than one a photograph contains* —
  purple, meaning red and blue both above green, or green, meaning the reverse.
  Both together is what separates a fringe from a violet flower: the flower is
  violet everywhere, the fringe only on the edge.

  Where both hold, the offending channels are pulled towards the ones that are
  not, and the pixel's brightness is put back afterwards — a defringed rim that
  went dark is a different artefact rather than none. Off by default: every
  frame has noise and every demosaic softens, but only some lenses fringe, and
  on a frame that does not this can only take colour out of something real.

  Measured through the whole render on six frames from a real library, at full
  travel: it moves between **0.03 % and 15.7 %** of subpixels, by up to 101 of
  255 where it does. Which is to say it works, and is invisible at fit zoom
  because it acts on scattered edge pixels and nothing else — the first thing
  said about it was that it looked like it did nothing, and the answer was that
  it was doing exactly what it should. The panel says so now.
- ✅ **OPTICS-004**: Manual overrides, for when a profile is wrong or absent.
  Distortion and Vignetting in the Detail page's Lens section, on top of
  whatever profile applied: each is turned into a lens profile of the same
  shape the camera's tables have — distortion growing with the square of the
  radius, 10 % at the corner at 100; half the light at the corner — so the same
  correction code does it. Applied to the frame before the turns and the crop,
  because a lens's centre is the frame's, and part of what a cached view was
  cut under. *Adds to a profile; turning a wrong profile off is not built.*

  PANEL_PLAN P4 and its rule 5 — things appear only when there is nothing to
  reveal by hand: the Lens section carries one line naming the profile that was
  applied ("Corrected for XF70-300mmF4-5.6 R LM OIS WR"), and the two sliders
  are there only when none was. A profile that applied has already taken both
  faults out, so a second pair of controls over them is two answers to one
  question. When nothing matched the line says so, and the sliders under it are
  what that sentence is for.

  "Matched" means the profile *corrects something*, not merely that one was
  found — falloff or geometry, `LensProfile::corrects_anything`. A camera can
  record a table of zeroes and lensfun can hold a model that works out to
  nothing at the focal length and aperture used; behind the narrower test a
  photographer would be left with a bent frame, a line saying it had been
  corrected, and nothing to straighten it with. The two questions are not the
  same one: falloff bends no geometry, so `bends_anything` alone would hide the
  sliders from a frame whose only correction was light in the corners. Sampled
  over the reference library — four frames from each of sixteen folders — every
  RAF gets a profile that corrects something, on Fujifilm's own tables and on
  lensfun's for a Sigma on a Sony; the unmatched state is a non-raw frame.

  The sliders come back on a photograph whose stack already moves them even
  when a profile did apply. A preset or a paste can put a value there, it still
  renders, and a correction nobody can see or take off is worse than a section
  with five rows in it.
- ✅ **OPTICS-005**: Lens corrections for everything that is not a Fujifilm RAF,
  out of the lensfun database. The camera's own table stays first where there is
  one: it describes the lens that was actually mounted at the settings actually
  used, which is why a Sigma nobody calibrated is still corrected on a Sony.
  Lensfun is the fallback, and it is a fallback for RAF too, for the frames
  whose own table will not read.

  The `lensfun` crate rather than the XML: it is a pure-Rust port that carries
  the database with it, so nothing has to be installed, and it already contains
  the part that is genuinely hard. That part is the radius. The database is not
  written in fractions of the frame — lensfun normalises by millimetres on the
  sensor over the focal length, so the scale moves with the body, the crop
  factor *and* the focal length, and distortion and vignetting do not even share
  a coordinate system. Fitting it by hand against a Fujifilm frame produced 1.86,
  1.85 and 2.04 for three lenses that should have agreed, which is what a wrong
  model looks like when it is fed enough parameters.

  Nothing here corrects a pixel. The database is sampled onto the nine radii an
  X-Trans IV or V RAF carries and handed to the code that has been bending
  Fujifilm frames since OPTICS-002 — one correction, two sources.

  Checked against the one reference that already agrees with a camera: Fujifilm's
  own tables. On the 16-80 the two curves track each other to within a constant
  2.3 percentage points at 16 mm and 0.3 at 80 mm, that constant being lensfun's
  choice to pin the corner rather than the centre, which is a magnification and
  not a distortion. Both call 16 mm barrel and 80 mm pincushion, which is the
  sign that matters: read backwards, a correction doubles what it should remove.

  Falloff agrees far less well — up to half a stop at the corner, in both
  directions. That is what a community database is: a different copy of the lens,
  on a different body, pointed at a different chart. Half a stop of error beats
  the eight tenths of uncorrected falloff it replaces, and only applies where
  there is nothing better to use.

  The search is held to the body's lens mount, because a 30 mm f/1.4 exists for
  most systems and a correction from the wrong one cannot be noticed downstream.
  A lens the database has never seen — the Viltrox 23 mm in the reference
  library — is left alone rather than approximated.

### TOOL — Tools

- ❌ **TOOL-001**: ~~Tool system — one active tool, consistent event handling.~~
  *Dropped: the tools were built without one and do not want one. Crop, the
  masks' brush and lasso, the pipette, the retouch spot and the straighten
  drag each own the canvas while their panel is open, and which one is active
  is which page the panel is on — one state, already visible, with no mode to
  remember or a toolbar to hold it. An abstraction over five handlers that
  agree on nothing but taking a drag would be a layer to read past.*
- ✅ **TOOL-002**: Crop — handles, aspect presets, straighten, thirds overlay.

  The aspect presets are toggles that stay chosen, not buttons that fire once.
  As buttons they reshaped the rectangle and then let the next drag of a corner
  undo it, which is the opposite of what picking 4:5 means: the reason to pick
  it is that the crop should *be* 4:5 when you are done moving it about. While
  one is held, a dragged corner keeps the shape, keeps the corner opposite it —
  that is what makes it feel like resizing rather than the rectangle jumping —
  and shrinks to fit rather than sliding off the frame when the shape it wants
  runs out of room. The larger of the two edges decides the size, so it follows
  the hand rather than one axis of it.

  And there are Reset and Done. The crop was committed on every release of a
  handle, which is right for the undo history and wrong as the only feedback
  there is: nothing on screen ever said the rectangle had been accepted, so the
  tool looked like it was still waiting for something. Reset takes the lock off
  with the rectangle, because the whole frame is not 4:5.

  **The photograph is the largest frame** (the photographer's, 21 September:
  "as Auto does — the photograph's edges are the maximum"). Straightened,
  turned or dragged, the crop stays on the photograph: a crop that fits is
  left alone, one that does not shrinks about its centre only as far as it
  has to, and nothing grows back when the angle returns (`crop_inside`). A
  dragged corner stops at the edge and, free of a ratio, slides along it. Each
  preset is a long and a short side that reads lying down or standing up —
  5:4 · 3:2 · 16:9, or 4:5 · 2:3 · 9:16 — as the crop already lies when the
  tool opens, and pressing the chosen one again turns both the ratios and the
  rectangle (Free turns the free one), and straight back gives back the same
  rectangle. **Custom** is a ratio of the photographer's own, typed the way it
  should lie. And one quarter-turn button rather than a left and a right (the
  photographer's: lying down or standing up needs one button; three presses
  go the other way).

  **The tool cuts what it draws.** The renderer turns the frame about the crop
  rectangle's centre in the source, so off the middle and straightened, the
  crop it cut was not the rectangle the tool drew — for the left half of a
  40 MP frame at five degrees, 168 pixels off. The tool now works in the view it shows
  and crosses to the document's crop when it commits (`crop_in_view` and back;
  a test holds the two to the crop's own pixels). No stored crop is read
  differently: only what the tool shows moved.

  **Masks stay on their pixels while it is open.** A mask is fractions of the
  crop it was made on, and the tool shows the whole frame — so it was laid
  across the whole frame and slid off its subject, further the tighter the
  crop. It is now mapped from the view into the crop the tool opened on,
  straightening included (`view_to_crop`), until Done makes it again for the
  new one. Measured on the rig: the face under a person mask held at 253 while
  the rectangle went from the whole frame to a 5:7 around it, where it fell to
  215 before.
- ✅ **TOOL-003**: Orientation from EXIF, plus manual quarter turns.
- ✅ **TOOL-004**: White balance eyedropper. "Pick a neutral" under the white
  balance sliders, then a click on something that should be grey: a few pixels
  of the unadjusted frame, skipping clipped ones, are taken back through the
  camera matrix to what the balanced sensor saw, and the temperature and tint
  that would make that grey are solved the way the camera's own balance is. A
  grey card under 3200 K and 7500 K comes back within 2 %. Both pipettes show
  a drawn pipette cursor: the theme has no colour-picker cursor, and the
  crosshair it fell back to was a coarse plus.

  **It no longer overshoots** (FT-028 #5). The frame it samples carries the
  base tone curve per channel and no sRGB encoding after it, and it was read
  back as plain sRGB — which bent the channels' ratios: a 3200 K card came
  back as 2791 K, an 8000 K one as 10 777 K. It goes back through the curve
  now (`tone::scene_value_for`), and the card it is pointed at renders grey
  to within three codes.
- ✅ **TOOL-005**: Colour picker for Point Colour (ADJ-006). The pipette under
  the mixer arms the canvas with the drawn pipette cursor, and a click adds the
  colour there as a new point: a 5×5 average of the unadjusted frame. Only one
  pipette is armed at a time across the mixer, white balance and Point Colour.

### GEOM — Geometry

- ✅ **GEOM-001**: Perspective — vertical, horizontal and aspect, beside the
  straighten that was already there. A camera pointed up at a building makes
  its verticals converge, and no amount of rotation reaches that: it is not a
  turn, it is a projection — and it is computed as one, a division by depth, so
  straight lines stay straight at any strength. It was a first-order
  approximation until a -40/+40 correction bent a bicycle out of shape.

  Stored in the crop operation and applied in the same resampling pass, which
  is the point of putting it there — separately it would interpolate the frame
  twice and every interpolation costs sharpness that nothing gives back. The
  backward map gains one term: how far out to reach, as a number that depends
  on where in the frame the pixel is. At the centre it is one and nothing
  moves, which is what holds the middle still while the edges are pulled about.

  Halved on the way in, so the coefficient never reaches one: at one the far
  edge of the frame maps to a point and the arithmetic divides by nothing. Half
  is already a threefold stretch from one edge of the frame to the other.

  Aspect is what the other two cost — correcting a keystone squashes the frame
  along the axis it fixed — and is exponential rather than linear, so −50 and
  +50 are the same correction either way round.

  The crop is the largest of its shape that the photograph fills, moved as
  well as shrunk: pulling it in about its own centre stopped at the first
  corner to touch and gave up a band of photograph on the opposite sides. A
  keystone reaches further out at one edge than at the other, and what is out
  there is nothing: the sampler clamps, so the corners came back as the edge
  pixel smeared into a streak hundreds of pixels long. That is what a
  corrected frame looked like, and it is worse than the slight crop taken
  instead — which is the same crop OPTICS-002 takes for a pincushion and the
  same one the camera's own engine takes.

  It is a crop, so it is the crop rectangle: a slider or Auto writes it there
  rather than zooming the render. It used to be the render, which meant the
  crop tool showed a frame already cut down, with nothing darkened and the
  handles on the edge with nowhere to go — what the correction took could not
  be seen, let alone taken back. Now the tool shows the whole corrected frame
  with the outside darkened, as after any crop by hand, and a corner can be
  pulled back out over what lies beyond — black, not the smear it once was —
  if that is what is wanted. For that to mean
  anything the keystone is measured from the frame's centre rather than the
  crop's, so a crop of a corrected frame is that frame cut down and not a
  different correction.

  Found by bisection on the size over the convex quadrilateral the photograph
  covers, moving the crop as well as shrinking it, so it is the largest of its
  shape the photograph fills. A frame with no correction on it is never
  quietly zoomed into, and a straighten alone leaves black corners for the
  photographer to crop away.

  Like a straighten it stops the renderer tiling: the region a pixel came from
  is a quadrilateral now, and a different one at every corner.

  **A correction taken back takes its crop back** (FT-028 #3). The fit is
  made from the rectangle the photographer left, not from the last fit, as
  long as nobody has moved it since — so Vertical +40 and back to 0, or a
  straighten and back, gives the whole frame again. Keystones and angles go
  through one fit (`crop_inside`), which also no longer grows a crop past the
  size it was drawn at.
- ✅ **GEOM-002**: Auto perspective, from the photograph's own lines.

  In a frame with converging verticals, how far a line leans depends on where
  it is: lines left of centre lean one way, lines right of it the other, and
  the lean grows with the distance from the middle. So the fit is *lean against
  position* — one weighted least squares over the edge pixels, with no Hough
  transform to tune and no vanishing point to find. Its slope is the keystone;
  a lean every line shares whatever its position is not a keystone at all but a
  camera that was not level, and that is the fit's intercept, so the straighten
  angle falls out of the same arithmetic.

  Which edges get a vote is decided by their gradient: an edge runs at a right
  angle to its own, so a vertical line has a horizontal one. Anything within a
  third of a right angle of upright votes on the vertical, anything that near
  horizontal votes on the other, and a diagonal votes on neither because a
  diagonal has nothing to say about which way is up.

  It declines. Below four hundred edge pixels there is nothing to fit, and a
  fit to nothing is a confident wrong answer; below two on the slider there is
  nothing worth correcting, which is what a frame of foliage measures — no
  architecture, but a great many short edges that fit to something very small.
  A photograph it cannot read is left alone and says so.

  Pressed twice it gives the same answer: the frame it measures is the one
  without any correction on it, so it replaces rather than compounds.
- ✅ **DOC-006**: Undo and redo — and a snapshot put back is now the whole
  snapshot.

  `apply_history` wrote every field of a step back into the document except
  `basic`. Everything else was there, so an undo restored the crop, the curve,
  the masks and the grade — and then `select_mask` read the document's own
  untouched `basic` back into the sliders and `sync_document` wrote it straight
  back down. Exposure, contrast, whites, every one of the eighteen simply would
  not undo. It looked like nothing had happened, which is the worst kind of
  fault in a history, because the answer is to press it again.

  Putting a snapshot back is one function now rather than a list written out at
  the call site, and a test holds it to the *whole* of `EditState` — so a field
  added to the snapshot and forgotten in the restore fails there instead of
  quietly ceasing to be undoable. Verified by removing the line again: the test
  fails, with the wrong exposure named.
- ✅ **GEOM-003**: Guided perspective — the photographer draws the lines.
  "Guided" beside Auto on the crop page: while it is on, a drag on the crop
  draws a guide along something that should be upright or level, up to four,
  and a press near a guide's end moves it. On every release the keystones and
  the straighten angle that make every guide true are solved for and applied
  as the sliders are, so the fitted crop follows and every slider stays yours.
  A guide nearer upright than level is meant vertical. One guide sets the
  keystone along its own axis — or, when no keystone can make it true, as for a
  line through the middle, the angle. Two or more set the keystone of every
  axis they speak for, and the angle; the aspect stays. Where the guides pin
  down less than that, the smallest correction that squares them wins. Guides
  are kept in the photograph's coordinates, so they stay on their edges as the
  correction moves; the solve is a coarse-to-fine search through the exact
  inverse of the resampling map, and synthetic keystoned lines come back within
  0.2 on the sliders and 0.02° on the angle. A tool, not an edit.
- ✅ **GEOM-004**: Flip horizontal and vertical, two buttons beside the quarter
  turns. Stored as one flag on the turn — a left-to-right mirror before it —
  since a vertical flip is that and half a turn, so the eight ways a frame can
  face are one flag and an angle. What is flipped is the frame on screen,
  whichever way it is turned, and the crop, its straighten angle and the
  perspective are mirrored with it. In the history as "Flip". *Masks and
  healed spots stay where they were drawn, as they do after a quarter turn.*

### ADJ — Adjustments

- ✅ **ADJ-001**: Exposure, contrast, highlights, shadows, whites, blacks — and
  Auto, which is a button and never a default.

  **The exposure is measured on the ends, not the middle**, and the first
  version was measured on the middle and was wrong for it. Putting the median
  on middle grey is what every auto-exposure does, and here it is wrong twice
  over. RENDER-008 has already asked *this camera* what exposure it meant for
  *this frame* and solved it from the camera's own JPEG, so the median is not
  adrift — it is where the photographer and the camera agreed. And a median is
  only a good estimate of "correctly lit" on an average scene: on a bird
  against bright cloud the median **is** the cloud.

  Measured on exactly that photograph: the median displayed at 0.685, Auto
  pulled it to 0.461, and the picture came back a full stop darker than anyone
  intended. Over twenty-four frames the old rule moved the exposure on all
  twenty-four; the new one moves it on one — the only frame where an end had
  actually run out. It comes down when the brightest half per cent is blown and
  goes up when nothing is near white at all, and between those the camera's own
  answer stands.

  **It looks at the subject and builds the photograph around it**, and it does
  that with a mask rather than with a slider. A frame with every tone in its
  place and the subject sitting among them is a photograph nobody looks at
  twice. What anyone wants from Auto is the thing they pointed the camera at,
  clearly lit, and the rest left where the photographer put it.

  Every global tool was tried first and each one fails in its own way. Measured
  on a backlit bird against cloud, reading the rendered pixels rather than the
  histogram:

  | | subject | frame | brightest |
  |---|---|---|---|
  | nothing | 0.040 | 0.682 | 0.936 |
  | Shadows 100 | 0.088 | 0.682 | 0.936 |
  | HDR 100 | 0.322 | 0.661 | 0.805 |
  | **a mask, +2 EV, with HDR 40** | **0.410** | 0.674 | 0.896 |

  Shadows at its full travel barely doubles the subject — a control pinned at
  its limit having achieved nothing, which is the exact failure this feature
  already refuses to write down at the endpoints. HDR-001 does far better,
  because it is not a curve at all but a measurement of what is bright *near*
  each pixel, and a dark bird surrounded by cloud is the case it was built for.
  It is kept for that reason, in proportion to the rescue. But it is still an
  answer about every dark pixel in the frame when the question was about one
  bird. Exposure inside a mask asks the right question: two stops on the
  subject, nothing at all on the sky behind it.

  So Auto's answer arrives as a **mask in the mask list**, named, with its own
  exposure slider. That is also the only honest way to show the work — an
  automatic correction you cannot see the workings of is one you cannot
  disagree with — and taking it back is one click.

  The mask is `Segment` rather than the pixels already in hand, so it survives
  being saved: a catalog row keeps which classes a mask is made of, not a
  megabyte of alpha.

  **The subject is measured on the pixels its own mask covers**, which took two
  goes. The first version measured with the matting model and adjusted a mask
  made from the semantic one, and those are the same region only when the
  semantic model found nothing. Measured on a night frame, that was +2.00 EV
  that moved its subject by four thousandths — a mask doing nothing, the same
  fault in a new place. Resolving the mask first and measuring inside it makes
  the two the same region by construction. Over ten frames, every subject now
  lands within 0.04 of where it was aimed.

  The subject is measured well inside itself rather than up to its edge. A
  subject's soft border is half background, and counting that band would
  measure the sky the bird is against as part of the bird.

  Pressed twice, one subject: the previous mask is replaced rather than
  stacked, or the exposure doubles every time the button is hit.

  **Auto lifts what the model can name, and nothing else** (FT-025). Measured
  over thirty photographs along the path the application actually takes:
  fourteen of them never reach the semantic model's opinion — no person, no
  animal — and the mask then comes from the matting model, which answers "what
  stands out here". That is the kitesurfer plus a piece of wave and the person
  fused with the lifeguard hut behind her, and a wrong subject lifted a stop is
  worse than a photograph left alone. So `auto::subject_mask` asks only the
  model that can name what it found; on the other fourteen Auto does the
  frame's half of the answer, as it already does on a landscape. The fallback
  is not disabled, only unasked — the Subject chip still reaches it, because a
  photographer choosing the frame is what makes "what stands out" the right
  question.

  **The button is off the page for now**, at the photographer's call, until
  the subject mask is reliably the shape of the subject. Everything behind it
  stays built and measured; showing it again is two lines in `window.rs`.

  **A refinement that keeps a sixteenth of the mask is declining** (FT-025).
  `matte::refine` is asked where an edge is; four of the sixteen frames where
  the semantic model found the person answered the other question instead —
  whether there is a subject at all — and came back with almost nothing, which
  was then used: 35.63 % of the frame became 0.85 %, 9.67 % became 0.24 %. An
  answer below half of what it was handed is not that subject's edge, so the
  mask that was named is kept and the fact is logged. Seven of the thirty
  recover to every pixel that was named; twenty-three are unchanged to two
  decimals.

  **And the model is no longer shown a squashed subject** (FT-025). A standing
  figure's box is 756 × 1460 on DSCF7577; pressed into the 1024 square the
  model answers on, all 756 columns arrive and 1024 of the 1460 rows do — three
  rows in ten thrown away on the axis where hair is. Letterboxing is worse and
  the arithmetic says so: it scales both sides by 0.70, leaving the rows
  unchanged and taking the columns to 529. A crop more than 1.3 times longer
  than it is wide is read as two squares of three fifths of its long side,
  overlapping by a fifth, each scaled *up* into the square, cross-faded on the
  distance to each piece's own edge. Never for `matte::subject` itself, which
  asks about the whole photograph on purpose. Two frames of thirty move: one
  ramp with no inside becomes a mask, one gets slightly softer.

  **The matting model is IS-Net, and it replaced MODNet for a measured reason.**
  MODNet is a 2020 portrait model that answers at 512. Asked about the whole
  frame, a bird across a sixth of it is seventy pixels wide and its wing came
  back as a block; asked again on a crop it drew every feather one time and a
  block the next — two copies of the same frame that differed by a third of a
  level per pixel got a different wing. IS-Net (the model behind `rembg`,
  Apache-2.0, 178 MB) answers at 1024 over the whole frame in one pass of
  2.5 seconds, drew every primary and every scallop of the leading edge, and
  gave the same answer on both copies to the cell. BiRefNet's Swin-tiny
  variant was measured as well: no better on this bird and eight times
  slower. The photograph every mask is cut from is reduced with Lanczos rather
  than a triangle filter: from a full frame that is a threefold reduction, and
  a triangle reads four of every nine pixels, so a feather against sky came
  out ragged before any model saw it.

  **Asked again on 20 September, with a graphics card and BiRefNet's full
  973 MB model rather than its lite one, the answer did not change.** On the
  same bird, IS-Net and BiRefNet draw the same wing — the same primaries, the
  same gaps — and BiRefNet's edge is the harder of the two (mean step across
  the soft cells 0.212 against 0.121), which is what a model trained for
  dichotomous segmentation rather than for matting gives. It is also 4 to 6
  seconds against IS-Net's 0.28, and it **cannot run on WebGPU at all**: its
  decoder asks for 17 storage buffers in one shader where the standard allows
  16, so the provider refuses the graph. MODNet, measured beside them, still
  turns the far wing into a half-transparent smear.

  What the shootout did find is that **IS-Net belongs on the card**: 282 ms on
  the processor against 149 ms on it, three runs each, and the two alphas are
  identical to the code value over the whole 1024 square. It had never been
  tried there, because the line about small models paying more to move tensors
  than to compute was written about models that are small, and 171 MB is not. Over ten frames, IS-Net finds a subject on one more (a dark one
  MODNet missed) and invents none.

  **The mask is the shape of the bird, which took a second look.** Asked about
  the whole frame at its 512 pixels, a bird across a sixth of it is seventy
  pixels wide, and at seventy pixels a wing is a solid block — the space
  between the primaries filled in, the tips gone. The first pass now only
  says where to look; a second pass on that crop, with the bird filling the
  model's input, draws every feather. Two things then had to get out of its
  way. The second pass was gated by the first — and a gate drawn from a pass
  that could not see the wingtips cut them off the pass that could. And with
  "Refine edge" on, `resolve_mask` ran the model a third time over an answer
  that was already the model's, which only trimmed the tail. Measured on
  DSCF2934: 2.83 % as a blob, 3.48 % as a bird. The same fix carries the
  Animal and Person presets whenever the semantic model comes up empty.

  **A little colour, because the rest of it takes colour away.** Lifting a
  subject out of shadow and pulling the range together both wash saturation
  out. Vibrance is measured against what a well-lit frame off this camera
  already has, only ever added, never more than a fifth of the slider, and
  never to a frame that already has colour in it. Vibrance and not saturation,
  so the colours that are already there are left alone.

  The frame's own sliders are written to the photograph and never to a mask.
  `sync_document` puts the sliders into whichever mask is selected, which is
  right for a hand on a slider and wrong for this.

  It sets three things and refuses the rest. The two endpoints, each measured at half a per cent in so a dozen
  blown specular highlights on a bumper do not decide where white is for the
  whole frame. And Highlights, only where the exposure could not be had
  without it.

  The endpoints are **solved against the arithmetic that will actually run**,
  not against a formula written beside it. A formula was the first attempt and
  it pinned both ends at ±100 on every photograph — which is not an automatic
  exposure, it is a sledgehammer. Two things were wrong. These sliders do not
  gain the whole frame: each is masked to the outer quarter of the range, and
  at the darkest half per cent of a real frame that mask measures **0.03**, so
  at full travel the control moves that pixel by two per cent and no target
  down there is reachable at all. And the value then goes through the base
  curve, which is a sigmoid rather than a gamma, so answering in stops
  over-asked by two or three times.

  The targets were wrong as well, and measured rather than chosen now. A
  well-rendered frame off this camera already puts its darkest half per cent at
  0.06 to 0.09 and its brightest at 0.93 to 0.94; aiming at 0.004 and 0.985
  describes a histogram that has been clipped, not one that is right.

  So a slider is allowed to say nothing. Already at its target, or at its
  extreme having moved the photograph by less than anyone could see: both come
  out as zero rather than as a confident number. Over forty frames spread
  through a real library, exposure moves on all of them, Whites moves on
  about half — down to −100 with Highlights behind it on a blown frame, up on
  a dull one, and nothing at all on one whose brightest pixel sits below where
  the slider can reach — and Blacks moves on none, because on this camera's
  rendering that control genuinely has no purchase where the measurement is
  taken. Saying so is the point.

  Contrast, vibrance, saturation and the tone regions it does not touch. Those
  are questions without answers, and an automatic answer to one of them is a
  slider that has to be put back on every frame.

  The answer lands in the ordinary sliders rather than anywhere of its own, so
  it can be seen, nudged, or taken straight back out one piece at a time. That
  is the whole reason it is a button: applied on open it would be a correction
  nobody asked for on every photograph it was wrong about. UX-017 is what makes
  this readable — the sliders it moved are the ones that light up.

  **The four tone regions are one curve that never turns over** (FT-028 #1,
  #2). Each region's gain depends on luminance alone, so together they are a
  curve of it — and Highlights or Whites pulled below about −60 turned it
  over near white: scene 0.52 → 1.0 rendered 178 → 146 at Highlights −100. The
  curve is now worked out once per render over log2 luminance and held to a
  quarter of the slope it had at the least, from the bottom up, so every tone
  below where it would have turned keeps exactly what the sliders asked and
  the ones above stay in order. Per pixel it is a lookup where it was five
  `powf`s.

  **Contrast and Blacks, as Lightroom has them** (FT-028 #9, #10). Contrast
  stretches tones out from middle grey or presses them towards it, the one the
  mirror of the other: +100 doubles the slope as before, −100 halves it — it
  was a slope of nothing, one flat grey. Blacks sets the black point, as
  Adobe describes it: to the left the darkest tones clip to black
  progressively (at −100 the point is about the twentieth code of a frame
  rendered as the camera would), to the right they come up by as much as two
  stops without clipping, and neither reaches scene value 0.1. It was one
  stop over the bottom quarter, where the base curve had already crushed
  everything it could have moved. Auto predicts both with the render's own
  curve.
- ✅ **ADJ-002**: Tone curve. Point curve on the composite channel, with the
  histogram behind it and the black and white points draggable. Monotonic cubic
  (Fritsch–Carlson), so a steep segment cannot make the curve turn back on
  itself — a plain spline overshoots there, and on a tone curve that is a range
  of input tones which get darker as they get brighter. Applied in display
  values, between the camera's base curve and the eight-bit quantisation, so
  the shape drawn is the shape applied. Red, green and blue each have a curve
  of their own, picked with RGB · R · G · B above the graph and applied after
  the composite, on the picture as that curve left it. The one being edited is
  drawn in its colour; the others that do something stay faintly behind it,
  so a red curve is not forgotten while the composite is shaped. They travel
  with "tone curve" when settings are pasted, and undo like the rest.

  The curve is a 256-entry table, baked once — a spline evaluation per subpixel
  would be forty million of them. It used to be read at the nearest entry, which
  gives 256 possible answers, and a steep stretch of curve spreads those over
  more than 256 code values: the output then skips codes. That is banding in a
  sky and a comb in the histogram, and it is worst exactly where a curve is
  usually steepest, so every shadow lift drew one. Reading between the two
  nearest entries costs one multiply and makes the output continuous again. The
  test builds a steep curve, encodes a long ramp through it and fails if the
  codes it covers are not nearly all used; without the interpolation it fails.

  Measured on a frame, since the shape of the fault matters more than the fact
  of it. A shadow lift of (0.25 → 0.45) used 198 of 256 codes with 53 empty bins
  inside its own range, and the gaps sat where the curve was steep — half the
  bins below 64 empty, none above 160. An S curve moved them to the midtones
  instead, which is where an S curve is steep. Interpolated, the same photograph
  and the same curve use 252 with no gaps at all. Nothing upstream was ever
  combed: the scene-linear data fills every twentieth of a code at full size, at
  proxy size and in working space.
- ✅ **ADJ-003**: Vibrance and saturation.
- ✅ **ADJ-004**: White balance in Kelvin and tint, applied in camera space.

  **Tint reads as Lightroom's: + is magenta** (FT-028 #4; the photographer's,
  21 September: "take what Lightroom decided"). The units were already
  Adobe's — a hundred and fifty of tint is 0.05 of CIE 1960 v, the DNG SDK's
  scale — and only the sign was the other way. Every stored edit is in the old
  sign, so the document keeps it and the sign is turned where a person or a
  file meets it: the Tint slider, the mask's, and a Lightroom preset's Tint on
  import (which had been read the wrong way round).
- ✅ **ADJ-005**: HSL / colour mixer — and a pipette, because eight named dots
  are a fine list and a poor question. The sky in front of you is either Aqua
  or Blue and the only way to find out from the list is to try both. So the
  pipette beside them arms the canvas, and a click picks the band nearest the
  colour under it — nearest *round the wheel*, which is not the nearest number:
  red sits at 0 and magenta at 300, and a hue of 350 belongs to red.

  Sampled from the frame before any adjustment, the same one MASK-006 reads, so
  picking twice in a row gives the same answer even after the mixer has moved
  the hue it was pointed at. The question is which band of the photograph to
  work on, and that does not change because the work has started. — eight colours, and hue, saturation and
  luminance for whichever one is picked.

  It was arranged the other way for a while: a tab per channel over eight named
  bands, so the first thing asked of you was which *operation* you wanted. Nobody
  thinks "I would like to change some hues". They think "the sky is too cyan",
  which is a colour and then an amount, in that order. The colours are swatches
  rather than names — a row of words is a row of words to read, and these are
  the thing itself.

  Each of the three tracks is painted for the colour it is about: the hue track
  runs thirty degrees either side, saturation from grey to the colour, luminance
  from near-black through it to near-white. A slider reading "Saturation −40"
  over a grey bar is a number; the same slider running from grey to that blue is
  the answer before it is moved. Every gradient is generated from the band's own
  hue in `BANDS`, so the swatch, the track and the arithmetic cannot drift
  apart. Built on the
  same `HsvTable` machinery a DNG profile's hue/saturation map uses, so it costs
  one more pass over a table rather than a new stage.

  The bands sit at the ProPhoto hues their sRGB names map to, not at the sRGB
  numbers. The tables operate in ProPhoto where the DNG specification put them,
  but "green" means 120 degrees on the wheel a photographer has seen — and that
  lands at 103 in ProPhoto, with purple at 255 against 270, a gap of up to 17
  degrees. Placed at their sRGB numbers the "Green" slider would act on
  something closer to yellow-green. The mapping is computed from the matrices
  rather than written down, and tested against measured values.
- ❌ **Point Colour as its own section** leaves the panel in PANEL_PLAN P1:
  merged into the Mixer, whose pipette becomes the one pipette and whose Range
  appears after a pick. Two pipettes for "point at a colour" was the same
  question asked twice. The feature itself is ADJ-006 below and does not change.

  **Luminance leaves grey alone** (FT-028 #6). The table had one saturation
  row, so a band's luminance scaled every pixel whose hue rounded into it,
  greys included: Magenta −100 took a middle-grey frame from 118 to 1. It has
  two rows now, grey and full colour, and the luminance fades out towards
  grey; hue and saturation are the same in both rows and act as before.
- ✅ **ADJ-006**: Point Colour — pick a colour from the image, adjust that range,
  see the affected region. The mixer's eight bands split the wheel by hue
  alone, so skin and brick are both Orange to it. A point is the colour itself:
  its hue, chroma and lightness, matched in OkLCh with a range around all three
  that fades smoothly rather than ending at an edge, and greys stay out because
  their hue means nothing. Each point has its own hue, saturation and
  luminance. It sits right after the mixer in every working space, and a
  photograph with no points renders exactly as before. "Show affected area"
  greys out everything the selected point does not reach, only on screen.
  Points are in history and copied with colour settings. *Picked from the
  unadjusted frame and matched after exposure, so a large exposure change can
  make a point miss; the lightness tolerance is wide for that.*

  **Luminance leaves grey alone** (FT-028 #7). A point picked on orange
  weighed a neutral grey at 0.75 through the chroma term alone, and its
  Luminance −100 took middle grey from 118 to 28. The luminance change now
  fades out below the chroma at which the weight starts to count hue.
- ✅ **ADJ-007**: Colour grading — shadows, midtones, highlights and global,
  with blending and balance. The grade that gets asked for is warm highlights
  against cool shadows, and a white balance cannot say it: that moves the whole
  photograph, which is the thing being corrected rather than the thing being
  said.

  The ranges are decided on the *displayed* brightness, through the same tone
  curve the export goes through. Scene-linear is where the arithmetic belongs
  but not where the vocabulary does — middle grey sits at 0.18 there, so half of
  what a photographer calls a midtone would be graded as a shadow.

  The three always sum to one, at any blending and any balance, which is what
  keeps a grade from changing the overall brightness as the balance moves. Black
  is entirely shadow and white entirely highlight whatever else is set. Blending
  is the exponent that decides how far the ends reach inward; balance bends the
  brightness axis rather than sliding a threshold along it, so at +100 a pixel
  has to be half as bright to count as a highlight and everything in between
  moves with it.

  A tint is the hue's own chroma rather than the hue normalised to a luminance
  of one: pure blue carries 7 % of the luminance, so making it weigh as much as
  white means multiplying the blue channel by fourteen, which is a filter and
  not a tint. Measured on a neutral: under a fifth of a stop of brightness
  change at full saturation. The brightness dial is in stops, one per hundred.

  It runs last, after the masks, because a grade is a statement about the
  finished photograph — put earlier, every local adjustment would shift its idea
  of which pixels are shadows. It is in the history snapshot and travels with a
  pasted look, both of which are the kind of omission that is only found when an
  undo silently does nothing.

  **What the page shows is what is kept** (FT-028 #13): a hue chosen before
  its saturation, the blending and the balance used to be dropped with a
  grade that changed no pixel yet, and stepping away and back lost them. And
  the range it opens on is **All** (the photographer's, 21 September): a grade
  usually starts as one tint over the whole photograph.

  The per-range slider is **Luminance**, Lightroom's word and the mixer's; it
  was Brightness (FT-028 #15).
- ✅ **ADJ-008**: Calibration — hue and saturation of the red, green and blue
  primaries, and a green–magenta tint in the shadows. A matrix, as in a camera
  profile: each primary's colour is turned about the neutral axis (up to 30°,
  towards the next primary as Lightroom's sliders go) and scaled, acting only
  on the colour over a pixel's grey, so grey stays grey. Applied
  where a profile acts, before anything reads colour. Global only.

  ❌ *The Calibration section leaves the panel in PANEL_PLAN P1. It is a
  profile-maker's control, not an editing one — the place it belongs is a
  camera profile, which RENDER-005 already installs. The value stays in
  `Basic` and an imported Lightroom stack that carries calibration renders
  unchanged.*

  **A primary stays its own** (FT-028 #8). White was held by taking its drift
  back out of every primary in proportion to its brightness, which carried
  Red −100 into aqua further than into red. The grey in a pixel is now left
  as it is and the matrix acts only on what is over it — at most the two
  primaries the colour is made of — so grey and white stay put by
  construction and the complement is not touched.
- ✅ **ADJ-009**: Black & white (brief B2) — a switch at the head of the
  Colour section, as Lightroom's treatment is. On, the mixer's eight dots
  become the black-and-white mix: one Luminance per colour, which sets how
  light that colour's grey comes out, up to two stops either way for a fully
  saturated colour and less as the colour pales, so a grey never moves.
  Vibrance and Saturation dim, having nothing left to act on. The colour
  bands and the grey mix are kept apart in the document, and switching back
  loses neither.

  Where it converts is Lightroom's place: after the masks, so a mask's
  warmth is a colour the mix can still lighten or darken, and before the
  grade, so a grade tones the grey — split toning is Colour grading on a
  black-and-white photograph. The grey is the working space's luminance.
  Lightroom presets and styles that were black and white (`ConvertToGrayscale`
  with `GrayMixer*`, Capture One's `BwEnabled`) now arrive as that instead of
  as Saturation −100 with a note that the mix was left behind.

### FILTER — Effects

- ✅ **FILTER-004**: Clarity (local contrast, shares HDR-001's machinery).
- ❌ **FILTER-001**: ~~Gaussian blur.~~ *Dropped: a photograph is not blurred as
  a whole. Where softening is wanted it is local — a mask over a background, a
  retouch spot — and negative Clarity and Sharpening already reach for it
  there, with the mask machinery deciding where. FILTER-008 dropped the depth
  version for the same reason.*
- ❌ **FILTER-002**: ~~Unsharp mask.~~ *Dropped: DETAIL-001 is the sharpening,
  and it is an unsharp mask — radius, amount, detail and a masking threshold,
  with the halo control this item would have lacked. A second, blunter one
  beside it would only be a way to get a worse result.*
- ✅ **FILTER-003**: Vignette — amount, midpoint, roundness, feather. Up to two
  stops at the corners, in stops so a sky and a shadow darken alike; roundness
  runs from the frame's rectangle through its ellipse to a circle. Placed by
  where a pixel is in the cropped frame, so a 1:1 tile and the export agree.
  Last but grain in the pipeline. Lightroom's defaults (midpoint and feather
  50), so a preset that names only the amount means what it meant there.

  **Squaring it no longer weakens it** (FT-028 #11). Below zero Roundness
  raises the distance's power, and the scale meant to keep the corner at √2
  divided where it should multiply: at −100 the corner sat at 0.84 of the
  way out and darkened 7 codes against 88. It multiplies now; the corners
  darken as far at every roundness.
- ❌ **FILTER-005**: ~~Texture.~~ *Built as DETAIL-004, where it belongs: it
  is a band of the same local tone mapping as clarity, not a filter of its
  own.*
- ✅ **FILTER-006**: Dehaze, -100..100, beside Clarity and Texture. The dark
  channel prior on a grid of patches a sixty-fourth of the long edge, the
  transmission smoothed and unmixed per pixel. The airlight is taken as grey
  at the haziest patches' brightness: on a drone frame a coloured one turned
  +70 cyan and -70 pink. Negative mixes that grey back in evenly. Measures the
  whole frame, so it keeps a render off tiles.
- ✅ **FILTER-007**: Grain — amount, size, roughness. Two octaves of value
  noise on lattices turned against the pixels and each other — an unturned one
  showed its grid — pinned to the full-resolution frame, so zooming shows the
  same grain larger and a tile its own part of it. Strongest in the midtones
  and nothing in black or white.
- ❌ **FILTER-008**: ~~Lens blur with a depth map.~~ *Dropped: a faked blur is
  not something the photographer wants.*

### DETAIL — Sharpening and noise

- ✅ **DETAIL-001**: Sharpening — amount, radius, masking. An unsharp mask on
  log2 luminance, applied as a gain so colour ratios survive and an edge grows
  no fringe. **On by default at 25**, because demosaicing is a low-pass
  operation and a develop without it is unfinished rather than neutral: measured
  against RawTherapee on the same file, its default render has an acutance of
  0.082 where ours at zero has 0.068 and ours at 25 has 0.081. The radius is in
  full-resolution pixels and scales with the preview, so below about half a
  pixel both detail passes decline — which is why the sliders do nothing at fit
  zoom and have to be judged at 1:1.

  ponytail: a single box blur, and one unsharp mask. RawTherapee's default is
  deconvolution-based and pulls further ahead on very high-detail frames (0.47
  against our 0.20 on the sharpest frame in the reference library). Deconvolution
  is the upgrade path if that gap ever matters.

  **Every tenth of Radius does something** (FT-028 #12). Lightroom's runs 0.5
  to 3.0 in tenths; one box blur at the rounded radius made 23 of those 25
  steps the same picture. Between two whole radii the two blurs are mixed by
  how far along it is, so a whole radius costs what it did. Below half a pixel
  at the magnification on screen it still does nothing, as FT-021 decided.
- ✅ **DETAIL-002**: Noise reduction — luminance and colour. Luminance is the
  guided filter HDR-001 already uses, at a small radius, with the slider moving
  the variance that counts as noise rather than a blend weight. Colour takes the
  hue from a heavily blurred copy and the brightness from the original, which is
  what makes it noise reduction and not a blur. Luminance defaults to off and colour noise to 25:
  the right amount of luminance smoothing depends on the ISO and guessing it is
  worse than leaving it, while colour noise is never wanted (DETAIL-002 below
  says why).

  ❌ *"Contrast" leaves the panel in PANEL_PLAN P1, and on the measurement
  rather than for room. FT-015: at noise reduction 40 and Contrast 50 — a
  setting someone would type — an ISO 6400 frame at 1:1 moves by one code value
  on no pixels; with both sliders at maximum, two code values on 0.29 % of
  them. What a guided filter at radius 2 takes out of a real frame is the grain
  itself, and blurring grain at radius 4 leaves nothing to give back. Nobody
  had set it in 69 edit stacks. The value stays in `Basic`.*

  Under the luminance slider, Detail and Contrast. Detail moves the line
  between grain and structure — a quarter to four times the variance that
  counts as noise, with the middle being the floor the slider always had, so a
  stack from before keeps its look. Contrast gives back the coarse part of what
  the filter took: what was removed is blurred at twice the grain's width,
  where the grain averages out and the slow wobble under it does not, and that
  much is added back. It is the difference between a denoised face and a
  plastic one. The test builds a ripple under a checkerboard of grain and fails
  if either slider is disconnected.

  **A10, 20 September: the colour half now runs after dehaze**, which means
  dehaze moved ahead of the whole detail group rather than the pass moving
  past the sharpening. Dehaze is an affine stretch per channel, so it
  multiplies channel differences — chroma noise among them — by one over its
  own contrast term, and it was undoing part of what this pass had just done.
  Measured on FT-019's patch: with dehaze at +40 the high-pass b\* was 4.253
  against a 3.695 floor, +15 %; it is 4.027 now, +9 %. Not all of it, because
  the pass is a spatial filter rather than a scaling and the two do not simply
  commute — but a third of the penalty is gone.

  Dehaze was the one that moved because `blur_colour` blurs absolute channel
  values and rescales each pixel to its own brightness, so it does not commute
  with a spatially varying gain: moving it past the sharpening would have
  changed every photograph that has sharpening on, which is every photograph.
  This way a stack with dehaze at zero is untouched to the byte, and three
  reference frames prove it.
- ✅ **DETAIL-003**: AI denoise, SCUNet through ONNX Runtime. A switch in the
  Detail page's Noise section, with an amount.

  SCUNet is Swin-Conv-UNet, Zhang et al. 2022, trained for blind real-world
  denoising; the `real_psnr` checkpoint, the one that stays faithful rather
  than inventing texture, from an ONNX export on Hugging Face — 77 MB as an
  `.onnx` and its `.onnx.data` sibling (the project is Apache-2.0, the export
  carries MIT). It is in Preferences with the other models.

  At about half a second a 256-pixel tile it is not a slider. Turning it on
  runs it once over the full-resolution decode, in the background, in the
  standing progress toast with Stop: a 7728×5152 X-T5 frame is 805 tiles and
  516 s. The model is shown what it was trained on, the frame at the camera's
  own balance and matrix and sRGB-encoded; tiles overlap by 32 pixels and are
  feathered together. Its answer is kept in the user's cache, keyed by path and
  modification time, as a 12-bit PNG — 69 MB for that frame.

  What goes back into the pipeline is the difference it made, through the
  inverse of that matrix, at the start of the colour stage. So white balance
  and the profile still apply afterwards, a highlight the model could only see
  clipped comes through untouched, and the proxy, the 1:1 view and the export
  all start from the same frame. The amount is a blend there: reading the kept
  frame costs 0.5 s once and 5 ms after. Stored with the edit, in the history,
  copied with Detail. *Not yet: grid thumbnails without it, one photograph at a
  time, and a measurement against DETAIL-002's guided filter.*

  **Half precision on the card, and each device its own tile** (21 September).
  Asked for something cheaper, NAFNet — darktable 5.6's denoiser, MIT,
  trained on SIDD — was measured beside SCUNet on an ISO 12800 X-T5 frame: 11×
  faster on the card (a 40 MP frame about 20 s against 200), and visibly less
  clean, with grain left in flat stone and the whole frame a little darker even
  fed through darktable's own shadow boost. Faster, not better, so SCUNet
  stays. What does make SCUNet cheaper: its weights in float16 on the card
  (`dev/export-scunet-fp16.sh`, 40 MB) and a 512-pixel tile there — 108 s
  against 200 for a 40 MP frame, 99.9 % of it within three 8-bit codes of the
  float32 answer — while the processor keeps float32, which half precision
  does not speed up, at a 320 tile, 7.5 % faster than 256. The half-precision
  file comes with the GPU download.

- ✅ **DETAIL-004**: Texture. The same idea as clarity at a different scale, and
  choosing between them is most of the skill: clarity works at a twenty-eighth
  of the frame, which is the size of a cheek or a cloud, and texture at a
  two-hundred-and-twentieth, which is the weave in a jacket and the bark on a
  tree.

  A third band in the tone mapping rather than a pass of its own — the guided
  filter is already solving for one base, and a second radius gives the middle
  band for the cost of one more solve. With texture at zero the two bands add
  back up to exactly what they were, so a photograph that has not asked pays
  nothing and the second filter is never built.

  Negative is the useful half nobody expects: it takes detail *out* of a band
  narrow enough that skin flattens and eyes do not, which sharpening cannot do
  and blurring cannot do without taking everything.

- ✅ **DETAIL-005**: Clarity below zero spreads light instead of removing
  detail. It scaled the detail band down, which is the symmetrical thing to do
  and the wrong one: at the far end the texture goes to exactly zero and
  everything away from a highlight darkens. That is a blur, and it is not what
  the control is for.

  What people reach for is the opposite in character — the photograph keeps its
  detail and gains a glow. So a plain blur rather than the guided base, because
  this one has to cross edges: bleeding across them is the entire point, and an
  edge-preserving filter refuses to. Only where the surroundings are brighter
  than here, so light spills into shadow and a highlight is never made brighter
  still.

  Measured on a bright disc in the dark: the shadow beside it doubles, the disc
  itself does not move, the far corner does not move, and the fine texture keeps
  three fifths of its contrast instead of all of it going.

- ✅ **DETAIL-006**: Moiré — the false colour off fine repeating detail.

  A sensor samples on a grid, and a fabric, a roof of tiles or a screen
  photographed from across a room carries detail finer than that grid. What
  comes back is not the pattern but a beat against it: bands of colour that are
  not in the scene and that no white balance moves, because they are an
  artefact of sampling rather than of light.

  The whole of it is one distinction, because without it this is a
  desaturation with extra steps. A coloured edge **moves**: one colour, then
  another, and it stays there. Moiré **wiggles**: it alternates over a few
  pixels and nets to nothing. Both read as "the colour changed a lot here" to
  anything that only measures how much, so the test is the wobble at the
  finest scale against the transition underneath it — a red flower on green
  leaves is all transition, a jacket's weave is all wobble.

  Worked in chroma with the brightness divided out and multiplied back after,
  so it can only change what colour a pixel is and never how bright. That
  matters: the luminance detail under moiré is usually real, and softening it
  to remove the colour trades one artefact for a worse one. The test asserts
  it — every pixel's luminance is unchanged to a thousandth — alongside the
  wobble going from 0.30 to 0.06 and a green edge keeping its green.

  Off by default, like OPTICS-003 and for the same reason.
- ❌ **DETAIL-007**: ~~Defringe.~~ *Built as OPTICS-003, which is where it
  belongs: it is a lens fault, not a detail pass, even though it runs among
  them.*
- ✅ **DETAIL-010**: AI sharpen — the smear of a hand that moved, undone.
  Restormer (Zamir et al. 2022, MIT), its motion-deblurring checkpoint,
  exported to ONNX here (`dev/export-restormer.sh`) because it is published as
  PyTorch only. A switch and an amount under Sharpening, run exactly as AI
  denoise is: once over the full-resolution frame in the background, 256-pixel
  tiles, kept in the cache and mixed in by the amount. When AI denoise is on it
  works on the denoised frame, and the cache knows which, so sharpening never
  brings the noise back. On a crop smeared nine pixels sideways it goes from
  32.8 to 40.2 dB against the unsmeared original, the same number through
  Numa's runtime as through PyTorch; on a sharp crop it leaves the photograph
  within 39–45 dB of itself. 249 ms a tile on the card, about 3½ minutes for
  40 MP; 12–20 on the processor. *Not for a lens that missed focus: Restormer's
  defocus checkpoint made sharp crops worse (30–34 dB) and a blurred one
  sometimes worse than the blur, so it is not offered. Lightroom's AI Sharpen
  is Topaz's model, which is not open.*
- ✅ **DETAIL-009**: Super Resolution — an export at twice the size each way,
  with detail an interpolation cannot make. RealPLKSR (Lee et al. 2024, MIT),
  the ×2 checkpoint darktable 5.6 ships for the same job: trained with
  real-world degradations and stopped at its MS-SSIM stage, before the GAN
  training that makes other upscalers invent texture — faithful, as
  Lightroom's is. A size in the export dialog, offered once the 30 MB model is
  downloaded, and run over the finished frame so the edit is what gets
  enlarged: 512-pixel tiles, 32 pixels of overlap feathered together, an HDR
  file's gain map enlarged with it. 680 ms a tile on the card, a 40 MP frame in
  about 2½ minutes (160 MP out); about 12 on the processor. Beside Lanczos on
  the M10's brickwork it is crisper at the edges and in the texture, and
  invents nothing in the sky. *Lightroom runs its Super Resolution over the
  raw and hands back a new DNG to edit; this enlarges the edit, as Topaz does.*
- ◻️ **DETAIL-008**: Settle whether a developed frame is as sharp as Lightroom's
  and Capture One's, with numbers rather than an impression.

  The impression is that it is not, and the honest answer is that nobody here
  knows yet, because the comparison has never been made like for like. What is
  already known, and has to be ruled out before anything is changed:

  - The editor edits a 2400-pixel proxy, and DETAIL-001 and DETAIL-002 both
    decline below half a pixel of radius — which is deliberate, and means the
    sharpening genuinely is not applied to what is on screen at fit zoom. So the
    first question is not "is it sharp" but "is the *export* sharp", which is a
    different file and a different answer.
  - Capture sharpening defaults to 25 at a radius of 1.0, fitted against
    RawTherapee's default render by acutance. Lightroom's default is 40, and it
    has a Detail parameter — a deconvolution-ish stage that recovers the lens
    rather than raising local contrast — which is not built here at all.
  - The demosaic matters more than the sharpening on an X-Trans sensor, and by a
    lot. Whether ours holds fine detail against a known-good one is measurable
    on a resolution target and has not been measured.
  - A comparison against Capture One on an iPad is not a comparison of
    developers: that is a retina panel rendering at full resolution, with the
    device's own display pipeline on top.

  The work is the measurement, not the fix: one frame, exported at full size
  from each, MTF50 across the same edge. A slider moved before that is guessing.
### HDR — Dynamic range

- ✅ **HDR-001**: Local tone mapping via an edge-aware guided filter.

  ❌ *Its panel row leaves in PANEL_PLAN P1: the Light tab is six sliders and a
  curve without it, and HDR is the one of them that Auto sets for itself
  (ADJ-001 sets `hdr` on half the frames it touches). The value stays in
  `Basic`, a stack that carries it renders exactly as before, and a preset can
  still set it.*
- ✅ **HDR-002**: Bracket merge.
- 🟡 **HDR-003**: HDR output — HDR files built, the HDR screen deferred.

  **An HDR JPEG with a gain map** (21 September). The file every viewer shows
  as the SDR photograph it always was, and behind it a greyscale gain map that
  a screen with room above white uses to show the highlights brighter: the
  Ultra HDR layout Android, Chrome and Adobe read — an XMP container directory
  in the primary naming a second image, an MPF index giving where it is, and
  Adobe's `hdrgm` description in the gain map's own XMP. The HDR rendition is
  the scene's own light wherever the base curve had to compress it and the SDR
  frame everywhere else, by luminance so colour does not move: below middle
  grey the curve deepens shadows on purpose and just above it lifts them, so
  only from about 0.8 of scene white does a gain rise above one, capped three
  stops over paper white. On the corpus 1.5–11 % of a frame is lifted, by up to
  2.5 stops. The gain map is half size each way and is resized as stops: the
  `image` crate's resize clamps a float channel to 0..1, which the first
  version found by writing maps that lifted nothing at all.

  *Deferred: showing HDR on the screen (PQ/HLG) needs an HDR display and a
  compositor to verify against. `gdk::ColorState::rec2100_pq` is the hook.*
- ✅ **HDR-004**: Bracket alignment (median threshold bitmap), which is what
  makes HDR-002 usable handheld. Every merge lines its frames up with the
  middle exposure first: whole-pixel shifts of up to 63 pixels, so nothing is
  resampled, and a frame that moved simply has no say beyond its own edge.
  0.3 s for a 40 MP bracket of three. *Translation only; rotation is not
  searched.*

### MASK — Local adjustment

- ✅ **MASK-001**: Linear and radial gradient masks, drawn and dragged on the
  canvas. Coordinates are fractions of the displayed frame, never pixels — which
  is what lets a mask drawn on the fit view mean the same thing on a
  full-resolution tile and in the export, and is tested by rendering a region on
  its own and checking it agrees with the whole frame.

  A gradient is finished the moment it is drawn, and the panel now says so. It
  has handles, and that is the whole of it — so painting on one, tracing its
  edge with a matting model and feathering an edge that is nothing but a feather
  are three controls with nothing to act on, and what was left was Name and
  Strength. Clicking is off as well: every drag of a handle ends in a release,
  and a release used to add whatever the model saw under the cursor, which
  quietly turned a radial into a radial plus a bush.
- ✅ **MASK-002**: The brush and the lasso — and they belong to every mask
  rather than to one of their own. A found sky that took in a roofline is fixed
  by rubbing the roofline out, and that is the same tool as drawing a mask from
  nothing, so both apply to a gradient, a found mask and an empty one alike.
  Shape::Painted is the case where there was nothing underneath to start from.

  Strokes are stored as the points, radius and feather they were drawn with —
  fractions of the frame, like every other mask measurement — and replayed into
  the pixels. That keeps a catalog row small, survives a change of resolution,
  and is what lets a painted-on gradient still be dragged: the ramp moves and
  the strokes go back on top of it. While a stroke is being drawn it is stamped
  into the mask's own pixels segment by segment instead, because replaying four
  hundred points on every motion event is a brush that falls behind the hand.

  Distance to the segment rather than to its ends, so a fast hand draws a line
  and not a row of dots; coverage composites over rather than adding, so a
  second pass cannot go past solid.

  A lasso is the same record with its path closed and filled instead of drawn,
  which is what keeps it one type and one list: the brush and the lasso stay in
  the order they were used without a second list to interleave. Scanline,
  even-odd, the row sampled twice and each span measured against the pixels it
  partly covers — anti-aliasing out of arithmetic that was happening anyway.
  Shift takes away, for the lasso, the brush and a click alike, which is what
  replaced the Erase button: one rule across the three is one thing to learn.

  Three things made it unusable and all three were the same mistake — doing the
  expensive thing on every motion event. The canvas scroller drags to pan and is
  an ancestor of the overlay, so a stroke slid the photograph under the brush at
  the same time; the press is claimed now, which is also what stops a radial's
  handle from panning. The mask's coverage wash was repainted whole, six million
  pixels, for a mark a few pixels across; only the touched rectangle is
  repainted now. And the proxy was re-rendered per event, thirty milliseconds of
  work arriving faster than thirty milliseconds — the photograph catches up when
  the hand stops, and the wash is what shows the stroke while it is being drawn.
  With the wash switched off the stroke is drawn on the overlay instead, at the
  width it is being laid down at, because otherwise there would be nothing to
  see until the release.

  The size slider is squared: its travel is 0 to 1 and the radius is the square
  of it, between a pixel and a half of the mask's raster and a quarter of the
  frame. Linear, everything small enough to fix an edge with lived in the first
  tenth of the slider and the smallest brush on offer was still too big for a
  wingtip.
- ✅ **MASK-003**: Masks made of what is in the photograph. The presets are off
  while the model is still looking: the answer is what decides which of them
  are worth pressing, so before it lands none of them is. Pressed early they
  were all live, and one pressed early made a mask, ran the model, found
  nothing, took the mask back and said so — a round trip that looked like a
  fault and was only a question asked too soon. EfficientViT-Seg-B2
  on ADE20K through `infer`, the same way CULL-003 runs its face detector:
  61 MB, Apache-2.0, fetched by `dev/fetch-models.sh` into the user's own data
  directory. 195 ms per frame, once, off the main thread, and the whole answer
  is kept rather than the one mask that was asked for — adding a class has to
  be instant.

  **Which model, measured.** It began as SegFormer-B0, whose weights are under
  NVIDIA's non-commercial licence and could never ship with a sold copy. There
  is no published ONNX of EfficientViT's ADE20K weights, so
  `dev/export-efficientvit.sh` makes one, and it records the four things that
  had to be fixed in the upstream repository to get there. Held against
  SegFormer on four frames the two name the same classes in the same
  proportions; on a person at equal grid EfficientViT's raw answer is a little
  cleaner; and after the refinement below the sky masks on a bird, a ridge and
  a row of branches cannot be told apart by eye. Asked at 2048 rather than
  1024 it gets worse, not finer — trained at 512, it sees pieces of a person —
  which is the same limit MASK-008's matting model showed. Neither model calls
  a kite in flight an animal: ADE20K has almost none, and that is the dataset,
  not the network; MASK-008's fallback carries it.

  The model names 150 classes, which is the reason it was chosen over a sky
  detector: a named mask is a *set* of them (Greenery is tree, grass, plant,
  flower and palm), and anything the groups do not name is one click on
  the photograph away, because the class under the cursor is a lookup.

  **The panel shows what was found, not what could be asked for.** "Find in
  the photograph" was six fixed buttons — Sky, Buildings, Person, Animal,
  Greenery, Ground, Water — greyed out when the thing was not here. Now the
  row is what the model found in *this* photograph, largest first.

  **In groups, not in nouns.** Foreground and Background first, then the groups
  that are here: Person, Animal, Sky, Water, Greenery, Ground, Buildings — and
  nothing for a group under half a per cent of the frame, which is a smudge.
  There was a chip per class as well, and it made the panel a list of nouns —
  Windowpane, Curtain, Chair, Signboard — where half of them are things this
  model is not good enough at for the chip to be worth pressing, and the ones
  that are were already inside a group. Nothing is lost: a class-sized thing is
  selected by clicking on it, which picks *that* one rather than every thing
  like it in the frame, and the class under the cursor is a lookup.

  Foreground is whatever the photograph is of — the person or animal when the
  semantic model named one, and MASK-008's matting model when it named nothing,
  because a kite in flight is nobody's ADE20K class and the matte is not asked
  *which* thing it is, only which pixels are it. Background is that mask
  inverted, which is what the invert was always for, and it keeps the matte's
  clean edge. The pair is offered whenever the matting model is installed: "all
  of it except the subject" is a thing a photographer asks for on any frame.
  The row says "Looking at the photograph…" until the answer lands.
  `Segmentation::found` is the testable half.

  **Hovering a chip traces the thing on the photograph** with MASK-010's
  marching ants, before anything is made: what "Greenery" means here is seen
  first, and the wrong chip costs nothing. Traced from the model's own grid
  rather than the refined mask, because the outline is drawn at 640 across
  anyway and a preview that takes a tenth of a second is one nobody waits for.
  Leaving the chip puts back whatever was there.

  A click is a *point*, not a class. What it resolves to is whatever the model
  found connected under it — flood-filled across the grid from the cell that
  was clicked — so the melon it reads as a person comes out of the mask without
  the two people coming out with it: measured, 0.98 to 0.02 at the click and
  1.00 left standing on the other figure. Shift takes away instead of adding,
  which is the rule the brush and the lasso follow too. Points are stored as
  points, like everything else here, so they survive a better model; they work
  on a gradient as readily as on a found mask, and one placed before the model
  has answered is kept until it can be resolved.

  The model is asked at 1024 rather than at the 512 it was exported at, which
  it allows because SegFormer carries no positional encoding. The grid is a
  quarter of the input either way, and at 512 a walking figure was rounded off
  into the umbrella they were holding: 0.3 s against 1.9 s, once per photograph
  and on another thread, and the mask is the product. What comes back is still
  one answer per thirty pixels of a 40 MP frame, so it is refined against the
  photograph with the guided filter HDR-001 already had, used the other way
  round: there, smooth the image and
  keep its edges; here, take the mask's soft grid and give it the image's. That
  is the difference between a staircase across a branch and an edge that
  follows it.

  Three numbers decide how that edge reads, and the first two were wrong at
  first. The filter looks two grid cells out, not eight — at eight it spread
  the edge over a quarter of the shoulder it was meant to find. It reads the
  photograph at 2048 rather than 1024, not because the model has more to say at
  that size but because the *photograph* does: the edge of a shoulder is three
  pixels wide at 1024 and half averaged away before the filter sees it. And the
  probability is steepened afterwards, which is the part that actually read as
  "too much feather": the model is genuinely unsure over a wide band — some
  seventeen cells of it at the edge of a walking figure — and a mask that fades
  across seventeen cells has been applied to the background at a third
  strength. Measured on a night street: 2.6 % of the mask is now a middle
  value, and what softness is left is the softness the photograph has. The document stores the classes and never the pixels, so a
  catalog row stays a few bytes and a better model later improves every mask
  already made.

  The six buttons say what is in front of them. The model has answered by the
  time they can be pressed and it knows how much of the frame each of its
  hundred and fifty things covers, so a button that cannot produce a mask is
  switched off with a tooltip saying why, and one that can says roughly how much
  of the photograph it is. They all stay live until the answer lands — six dead
  buttons for the two seconds the model takes reads as broken.

  And a preset that still finds nothing — pressed before the answer arrived —
  takes its mask back rather than leaving one behind. A name, a row in the list,
  an entry in the history and no pixels looks exactly like a mask that works and
  is being ignored, and a message at the bottom of the window is not where
  anyone is looking when they expected a mask on the photograph. You asked for
  the sky and there is no sky, so there is no mask. Only masks nobody has
  touched: a click or a stroke makes it the photographer's rather than the
  model's, and theirs is not ours to remove.


- ✅ **MASK-004**: Per-mask adjustment stacks. A mask carries a `Basic`, the
  same struct the panel edits globally, so a local adjustment is the same
  adjustment faded in by a shape rather than a new kind of edit. Selecting a
  mask points the Light and Colour sliders at it instead of at the photograph —
  a second set would have doubled a panel that is already too long, and
  switching what the existing ones mean is what every editor with local
  adjustments does. The panel says which, because a mode nobody can see is a
  mode that confuses — and with one selected the panel hides everything a mask
  has no say over: the crop, the tone curve, the colour mixer, the AI denoise
  switch, and the lens corrections.

  **B3 widened what a mask carries, and this entry used to say otherwise.** It
  is no longer "exactly what `apply_basic` reads": `apply_masks` runs the same
  passes the photograph gets, in the photograph's own order, over a copy of
  the pixels. White balance as a *difference* from the photograph's — its own
  Kelvin pair on the Colour page, ±2000 K, see ENGINEERING's "What a mask
  carries"; Dehaze, Clarity, Texture and HDR, with `tiles_cleanly` asking the
  masks as well as the document; and the four detail passes — Sharpening,
  Noise reduction, Colour noise, Defringe and Moiré — on the mask's own copy,
  so softening a sky stops at the skyline instead of smearing the roof into
  it.

  A mask rests at `Basic::local`, not `Basic::default`. Two of a photograph's
  defaults are deliberately not zero — `sharpen: 25` and `denoise_colour: 25`,
  undoing its own demosaic — and a mask sits on top of a photograph that has
  already had both, so a mask carrying them would sharpen its area twice with
  nobody asking.

  **A mask gets only its own values** (FT-028 #0). Six things write the
  photograph's values into the sliders — opening, a history step, a paste, a
  merge — and if a mask was still selected, the next slider event wrote the
  photograph's exposure, sharpening and colour noise into it with nothing
  touched. The panel now remembers whose values it holds, and the document
  is written only there; a mismatch reloads the right values and says so in
  the log.
- ✅ **MASK-005**: Masks as a list. Every row carries the grip that says it can
  be dragged, an invert and a delete, and dragging one onto another moves it —
  the order is a decision rather than decoration, because masks are applied in
  it. Under the selected mask is what it is *made of*: every class clicked into
  it and every stroke painted on it, each removable on its own, because one
  click too many on a photograph should not mean starting the mask again.

  Above those parts is the mask itself: a name, a strength and a duplicate. The
  name is what it is called rather than what it is, because three radial masks
  on one frame are "Radial 1/2/3" until someone says which one is the face;
  blank puts the automatic name back. The number only appears when there is
  something to tell apart: one radial mask is "Radial", and it counts masks of
  that name rather than places in the list, so deleting the first of three
  leaves "Radial 1" and "Radial 2" instead of "Radial 2" and "Radial 3". The strength fades everything the mask
  does at once, which is how "slightly less of that" is actually asked — the
  alternative is walking back seven sliders by eye. Duplicating carries the
  pixels across, so a found mask does not pay for the segmentation model twice.

  Visibility is a row of its own, an eye beside each mask, because a mask is
  usually a question — is this better with the sky pulled down — and the answer
  needs both sides of it. Deleting and rebuilding is not an answer.

  All four live in `Mask::weight`, which is the one function every reader goes
  through: the preview, a tile, the export and the overlay that draws it.
  Hidden means covering nothing there, including when inverted, where the other
  branch would put the adjustment over the whole photograph. Strength applies
  after the inversion, so half of an inverted mask is the other half of the
  frame at half strength rather than the same half at the opposite one.

  A stack written before any of this reads back visible, full strength and
  unnamed, which is asserted rather than assumed.

  Where all of it *sits* was the second half of the problem. The first version
  put the name, the strength and the duplicate in a group of their own above the
  parts, on a page that also held the brush, the list of every mask and the
  buttons that add one — so the panel answered "what am I editing" in one place,
  "what can I draw with" in another and "what is this made of" in a third, with
  the tab strip still offering six other scopes on top.

  Now the panel has two states. Outside a mask, the mask page is the masks:
  every one of them, and the ways to add another. Inside one, the tabs go —
  there is a single scope and nothing to choose between — and the page is that
  mask: the histogram above it, a back arrow and an eye in the banner, the
  sliders that write to it, the brush and lasso that extend it, and what it is
  made of. The tools live under the sliders rather than on the mask page,
  because those sliders are what a selected mask is.

  The tab strip is a sibling of the panel stack rather than a child, so hiding
  it costs no width — which is exactly what made hiding the widgets *inside* the
  stack a mistake worth a commit of its own.

  Each part of a mask also has a dot on the photograph, at the middle of what it
  covers: a found class at the centre of the cells it won, and a click where it
  was clicked. Not strokes — a click is a *place*, where you pointed at
  something and the model answered, and a dot belongs there; a stroke is an area
  you painted, and the middle of a squiggle is not a place anything was decided.
  For a lasso it can fall outside the shape altogether. Marking them made
  drawing on a mask look like it was adding entries to a list, when what it does
  — and always did — is extend the mask.

  The list said the same thing a second way. Painting is continuous — twenty
  passes to clean one edge is an ordinary thing to do — and twenty rows saying
  "Painted" is a list nobody reads. A run of consecutive strokes of the same
  kind is one row now, counted: "Painted ×12". They are still twelve strokes
  underneath, and the row's cross takes out the run rather than the first of it,
  because taking one pass out of twelve looks like nothing happened. Clicking one switches that part
  off — distinct from the cross that removes it, because a click that turned out
  wrong is worth muting to see the difference before deciding, and a muted class
  comes back without finding the thing in the photograph again. The list and the
  dots are the same switch: filled means on in both.

  The dots can be turned off as a set, from the banner. They are working marks
  rather than the photograph, and judging a grade means seeing it without them.

  A click on a dot is tested before a click on the picture, because a dot sits
  inside the thing it marks — otherwise switching a region off would add the
  region it already is.
- ✅ **MASK-006**: Colour range — every pixel of a chosen colour, wherever it
  is in the frame.
- ✅ **MASK-007**: Luminance range — every pixel between two brightnesses.

  Every other mask answers *where*: a gradient by its geometry, a brush by
  where the hand went, a found mask by what the model recognised. These two
  answer *which*. That is the tool for a sky seen through branches, or the one
  red coat in a crowd, where no shape anyone can draw encloses the thing and
  nothing else — and unlike MASK-003 it needs no model at all, because it is
  arithmetic on the pixels.

  Measured on the photograph as it is framed and before any adjustment, which
  is the same frame the model is shown and for the same reason: a mask that
  moved when the exposure did would be arguing with the edit it exists to
  carry. When the model has run its own copy of that frame is reused, so the
  common case costs nothing.

  **Nothing is selected until the photograph has been asked.** A range mask
  made with a default hue would be selecting whatever that default happened to
  land on, over a photograph nobody pointed at — so it starts empty and says
  so, and the three numbers do not appear until there is something for them to
  adjust.

  **The click is the control**, and it says so. The pointer becomes a pipette
  over the photograph — the theme's own where it has one, a crosshair where it
  does not, never silently an arrow, because a cursor that says nothing says
  the click means nothing. And what was picked is *drawn*: the range sits on
  the thing it is a range of, a colour wheel for a hue and black to white for a
  brightness, with everything outside it dimmed and the chosen point marked.
  Three numbers on three rows do not add up to a picture of what is selected,
  and what is selected is the only thing anyone wants to know. A hue range that
  runs off one end of the bar comes back on the other, because a wheel does.
 Hue, spread and a saturation floor are the
  right parameters and the wrong question: a photographer knows which thing in
  the frame they mean and has no idea what number its colour is. Nobody thinks
  "hue 30 degrees"; they think "that red coat", and the coat is on screen. So
  clicking the photograph takes the range from it — the coat's hue, or a range
  centred on how dark the shadow was — and the numbers below are for adjusting
  what was picked. A click on something grey says so rather than selecting
  whatever the shadows rounded to. Picking never changes the spread, or the
  second click would undo the width the first one was set to.

  Three numbers each. Colour is a hue, how far either side of it still counts,
  and a saturation floor — grey has a hue, arithmetically, and it means
  nothing, so without that floor the mask fills with whatever the shadows
  rounded to. Hue is compared round the wheel rather than as a number: the
  distance from 350° to 10° is twenty degrees, and treating it as three hundred
  and forty is how a red mask misses half the reds. Brightness is two bounds
  and a softness, read on the 0..1 the screen shows rather than the
  scene-linear one underneath, where "half way" is a quarter of the way up
  anything a person would call brightness.

  They carry the whole adjustment stack like any other mask, take a brush or a
  lasso on top like any other mask, and MASK-011's feather and edge apply to
  what comes out.
- 🟡 **MASK-008**: Background and people masks, and **Person and Animal find a
  subject the semantic model has never heard of**.

  A kite in flight is not one of ADE20K's hundred and fifty nouns. Asked about
  such a frame SegFormer answers 89 % sky, 7 % mountain and **0.000 % animal**,
  so pressing Animal made a mask of nothing that was taken away again with a
  message — and there was no way left to select the bird at all.

  But the matting model is not asked *which* thing. It is asked which pixels
  are foreground, and on a bird against cloud it answers well: **2.8 %** of the
  frame, the body solid and the wingtips in the soft band, against 3.5 % for a
  click on the bird through the promptable model. So when the noun finds
  nothing and the class is one of the two that model was trained for, the other
  model is asked instead.

  It needs the largest connected region afterwards, because run over a whole
  frame it also picks up a few scraps of cloud at the bottom edge, and a
  subject mask with three bits of sky in it is not one. Labelled on a slightly
  spread copy rather than on the matte itself: an outstretched wing is a row of
  separate feathers with sky between them, and asked directly this kept the
  body and threw the wingtips away — which is worse than keeping a scrap of
  cloud.

  The preset buttons stay pressable on the same reading. Switching Person and
  Animal off because the noun found nothing is what left a photograph of one
  bird offering no way to select the bird.

  "Refine edge" is only offered
  where the model has something to say. The matting model is asked "how much
  of this pixel is foreground", and about a building or a sky it returns a
  confident-looking nothing. A switch on every mask promises
  something on all of them. The two classes it works on were already written
  down as the ones that turn it on by themselves, so the same list decides
  whether to offer it at all; a painted or clicked mask keeps the offer,
  because a click can land on a person ADE20K called something else. The matte is read back off the
  model between its four nearest samples rather than at the nearest one. It
  answers on a 512-square whatever size the subject is, so on anything filling
  much of the frame its grid is coarser than the mask's — and read at the
  nearest sample, that grid was what came out: a stair-stepped edge in place
  of the one it had been asked to improve, which is most of why the switch
  looked like it was doing nothing. It does least on a subject that fills the
  frame and most on one that does not, and that is the model's size rather
  than anything here. Person and Animal are separate
  presets, and the panel now offers the inversion as a chip of its own:
  **Background** is the foreground mask inverted, so "everything except the
  subject" is one press rather than two. People *individually*, and parts of
  them, is not done.

  They were one button called "Subject" until the photographer pointed out that
  his subject usually is not one: on a street photograph the subject is the
  building. A button named for what it is *for* is a promise the list cannot
  keep; two named for what they find keep it. The matte below is gated on the
  classes rather than on the presets, so a mask built by clicking a person and
  then a dog still gets a real edge — and is named "person + animal", which is
  what it is.

  The edge of a subject no longer comes from the guided filter. That filter is
  given a decision made on a grid a quarter of the model's input and asked to
  find the photograph's edges in it, which it does — just not always the right
  ones. On a person it took bites out of a dark shirt, welded a hat into a blob
  and left a blocky outline. It was recovering detail that was never computed.

  The matting model is asked a different question: not "which of a hundred and
  fifty things is this" but "how much of this pixel is foreground", and it
  answers in alpha.
  Hair comes back as hair. It runs on a crop around what the semantic model
  found, padded by a third — on a whole frame a person at 2.5 % coverage is a
  few pixels after the reduction to 512, and the model tops out at 0.30 instead
  of reaching 1.0.

  Trained on people. Measured, it also holds a bird against sky: the task
  transfers further than the training set suggests, which is why the gate is the
  Subject preset's own classes rather than person alone. It is allowed to
  decline — under 0.9 anywhere means no subject found — and the guided filter is
  then still the answer.

  What it does *not* do is find anything. A kite in flight is 0.1 % person to
  the semantic model and nothing to the animal class. Selecting that bird is a
  promptable model's job, not this one's — but once it *is* selected, the matte
  is exactly what the edge wants, and for a while it could not be reached.

  It ran on the found region, before the clicks and strokes were composited
  onto it, and only when every class in the mask was one of the two it was
  trained for. So a bird selected by clicking got nothing: no class to qualify,
  and the part of the mask that mattered was not the part being refined. It runs
  on the *finished* mask now, and it is a switch — "Refine edge" — rather than a
  guess about whether this mask is a subject. It costs half a second, it has
  nothing to say about a building, and the photographer is the one who knows
  which this is. The two presets it was trained for turn it on themselves, so
  they behave as they did when it was automatic.

  **It is a button now, and it searches from where Edge put the border.** A
  found mask often comes back a little wide, and the matte searched from
  outside the real border found an edge out there; pulling it in with Edge
  afterwards only moved that wrong edge. So the order is rasterise → Edge →
  matte → Feather: the photographer pulls the mask in, presses Refine edge, and
  the model looks for the border from the tighter start. Pressing it again
  searches again from wherever Edge is then. It runs off the main thread and
  the mask updates when it comes back.

  The document keeps the Edge the search started from, not the answer, and
  only what Edge has moved since is applied after — so the slider is still a
  blur, and the export repeats the same search. A stack saved when this was a
  switch has no such Edge, which is zero, which is the old order exactly.

  Measured on the kite, one click and nothing else: 398 ms without, 866 ms with,
  and the difference is a staircase against separated primary feathers, a
  serrated trailing edge, the beak and the talons.

  The matte is held to what was selected by a gate built from the coarse mask,
  or a second person in the same crop would quietly join the first. The gate was
  a distance scan at first — for each cell, how far the nearest selected cell
  was — which on a subject filling half a 2048-wide frame is twenty-six thousand
  samples per cell and tens of billions in total. It did not return. It is two
  passes of a separable box blur now, and there is a test that fails if it takes
  more than half a second on a real frame.

  Cost: 0.74 s for a subject mask against 0.16 s, measured end to end.

  **Refine edge looks again, with a matting model** (21 September). IS-Net is a
  model of what stands out, and on DSCF1264 — a woman in front of a wall of
  bags — the loose hair beside her head came back as one smooth shape with a
  glow round it, because a lock with pink behind it does not stand out from
  her. Asked again from closer it said the same; a pixel's colour against the
  colours either side could not tell a strand from a pink bag. ViTMatte-S
  (Apache-2.0, `vitmatte_small.onnx`) is asked a different question: the
  photograph and a trimap — certainly her, certainly not, decide the band — and
  it answers with coverage per pixel. The trimap is the first matte's: its
  solid part less a fiftieth of the subject is certain, the empty background
  beyond it is certain, and the band between, with any loose coverage near her,
  is asked. From a frame of the original at the mask's raster size, in tiles of
  1024 because its attention over a whole frame would want tens of gigabytes;
  on the card 1.8 s for her border, 6.7 s without one. On DSCF1264 the glow is
  gone and the strands are strands on both sides of her head. Only the largest
  subject is looked at again — a man in a hat behind her, whom the first look
  gave a third, keeps his third.

  It is a recipe (`Mask::fine`), so it is done again wherever the mask is
  resolved: after every rebuild in the editor, off the main thread with the
  loader up at once, the original decoded once per photograph; and in the
  export, from the original. Not in a thumbnail, whose source is the proxy.
  Known: her ear came back at about nine tenths rather than whole.

  And a matte starts without Feather. The photographer's first look at it in
  the app was a smooth shape again: every new mask starts at Feather 12, and a
  blur along the whole border is exactly what the closer look is there to
  undo. Person, Animal and Auto's subject now start at nought
  (`Mask::set_matte`), and Refine edge sets it to nought the first time it is
  pressed on a mask — the slider moves with it, and raising it again is the
  photographer's to do.

- ✅ **MASK-009**: Click a thing, get that thing. The semantic model knows a
  hundred and fifty nouns and nothing else — asked for the kite in a photograph
  of a kite it says 0.1 % person and no animal at all, so the thing the
  photograph is *of* cannot be selected. ADE20K has no class for it, and no
  amount of tuning makes one.

  SAM has no classes. Given the photograph once and a point, it answers "the
  thing under there", which is the question a click actually asks. SlimSAM-77
  through the same pure-Rust runtime as the rest: encoder 6.0 s once per
  photograph, every click after it 0.13 s. It answers three times over, because
  a click on a sleeve could mean the sleeve, the coat or the person, and scores
  each; the highest is taken, which is the whole disambiguation.

  The encoder runs on the first click that needs it rather than when the
  photograph opens. Three times the semantic model's cost is not something to
  pay for opening a frame to move the exposure slider.

  One point per call, so every click stays a part of the mask that can be
  switched off on its own — the dots in MASK-005 would have nothing to point at
  otherwise. The answer is 256 × 256, the same coarse grid as everything else
  here, so it still goes through the guided filter and through the matting model
  when what was clicked is a subject. SAM decides *what*; the others decide
  where its edge is.

  What it is not is a better Buildings button. A facade is not one object to it:
  clicking a wall gives a window, a sign, a run of brick. Measured on a street
  photograph, and the semantic preset is still the right tool for a whole
  building. Where it wins is what has no name — that kite — and the parts of
  things.

  Optional, and the largest of the models at 37 MB. Without it a click is the
  semantic flood fill it always was.

- ✅ **MASK-010**: Three ways to see where a mask is, and a switch for each.
  They answer different questions. The wash tints the covered area and is the
  only one that shows a soft edge as soft — and it hides the photograph to do
  it. The outline is a moving dashed line along the edge, over a picture you can
  still see. The dots say what the mask is made of and where each piece was
  decided. Which you want depends on what you are asking, so none of it is
  decided here: the wash used to be forced back on by every edit and the outline
  was never off.

  The outline is marching squares on a reduced copy — it is drawn at screen size
  either way, so tracing four megapixels buys a precision nobody can see. The
  segments are chained into paths rather than left loose, because cairo restarts
  a dash pattern at every `move_to`: as loose segments the line would not march,
  it would twinkle. Two shapes stay two paths, so the dashes do not run across
  the gap between them. Black under white, offset half a period, so it reads on
  a white sky and in a black shadow without a halo. 20 ms on a ragged
  four-megapixel mask, with a test that fails past 120.

  A fourth, **Matte**, landed 21 September: the mask by itself, white on black.
  The wash tops out at a light tint — times the mask's Strength — so a strand
  of hair at three tenths is a tint of a tint, and next to the outline, which is
  a line at one half by construction, a matte read as a flat blue shape with a
  hard line round it. The photographer took that for the mask. The matte view
  shows what the render multiplies by, shade for shade.

  It appeared only after hiding a mask and showing it again for a while.
  `rebuild_mask_map` held a mutable borrow of the open photograph across the
  trace, and the trace reads the mask back through the same cell — so every
  ordinary path failed and the one that worked came in from elsewhere.


  A gradient needed a fourth thing, which was pixels. The wash and the traced
  edge both read an array of coverage, and a gradient has none — its shape *is*
  the formula, evaluated wherever the render happens to be working — so two of
  the three switches did nothing at all on a linear or a radial. The overlay now
  makes one at its own size when it needs one, thrown away with the selection
  rather than kept on the mask, where it would go stale the moment a handle
  moved. The third switch, the dots, is gone on a gradient: it marks the parts a
  mask was built from, and a gradient is one shape with its handles already on
  the canvas.

  Selecting a gradient cost 55 ms on the main thread, 37 of them working out
  its eleven million cells one at a time for the overlay; across the cores,
  with the wash painted the same way, it is 17 ms.
- ✅ **MASK-011**: Feather, and moving the edge in or out. Both were a remap
  of the coverage values and nothing else, and no remap can widen an edge that
  has no width. Measured on a hard edge, which is what a found mask and a
  lasso both have: feather at 100 gave a transition **nought pixels** across,
  and Edge at either extreme moved the border **nought pixels**. They did
  nothing at all on the masks people actually draw.

  Softening an edge is a question about a pixel's neighbours, so it takes a
  blur. That blur lives in `core::plane` now rather than beside the guided
  filter that was its first caller, because `core` may not reach into
  `render` and this is the same blur. On a mask kept at 2048:

  | Feather | Transition | | Edge | Border moves |
  |---|---|---|---|---|
  | 12 (default) | 2 px | | +100 | 64 px out |
  | 25 | 12 px | | −100 | 64 px in |
  | 50 | 92 px | | | |
  | 100 | 376 px | | | |

  Squared, the way the brush size is: linear, everything worth having lived in
  the first tenth of the travel. The default of 12 is two pixels — enough that
  an edge is not a staircase, little enough that MASK-008's traced hair
  survives it.

  Both are the *last* thing done to a mask — except that on a refined one,
  MASK-008 searches from the Edge that was set when Refine edge was pressed,
  and only Edge moved since then comes after. They used to sit on the far
  side of everything before them: moving Feather re-rasterised the shape, went
  back to the segmentation for its classes, replayed every stroke and ran
  MASK-008's half second of matting — to change a blur radius. Five ticks of a
  slider queued five matting passes and the editor stopped answering. The
  coverage is kept as it was before the border was shaped, so shaping it again
  is one blur and one remap over one buffer. An extra raster per mask against
  half a second per tick.

  Edge is a bias rather than a moved midpoint. A midpoint that slides towards
  either end of the range stops being pinned there, and at full Edge every
  pixel with any coverage at all landed on it: a border came out as a wash
  across a fifth of the frame. The curve used instead passes through 0 and 1
  whatever it is asked for, so a soft edge stays soft while it moves. Everything else
  about a mask decides *where* it is; these decide what its border looks like —
  once, on the finished thing, so softening a mask made of a found region and
  four lassoes softens the mask rather than each of the five separately.

  The edge control exists because a found mask comes back generous. A model
  trained to say "this is a bird" has no reason to be exact about the last two
  pixels of it, so a little of the sky comes with it, and that reads as a halo
  the moment the mask is brightened. Pulling the edge in by a few per cent is
  the fix and it is not something a model can decide: how much of the border
  belongs to the bird is a judgement about the photograph.

  Feather 0 is a step and 100 is a ramp across the whole soft band the parts
  left behind, on a reciprocal so the low end stays usefully sharp rather than
  spending half the slider on nothing. The default is what it always did before
  there was a slider, and a stack saved before this reads back unchanged —
  asserted, because a mask that silently hardened or shrank is an edit nobody
  made.
- ✅ **MASK-012**: The animal chip says what the animal is, coarsely — "Bird",
  "Fish", "Dog", "Cat" — instead of "Animal" or "Subject", when an ImageNet
  classifier is sure of it. A crop around the subject is classified and the
  thousand classes are summed into groups a photographer names; below 60 % for
  every group the chip keeps its generic name, because "Animal" on a cat is
  true and "Dog" on it is not. The tooltip says how sure: "Probably a bird —
  100 %". Runs off the main thread once the segmentation lands and renames the
  chip when it answers. Optional (PP-ResNet50, `dev/fetch-models.sh`); without
  it the chips are what they were. The mask the chip makes is called what the
  chip said — "Bird", not "Animal" — and the rename field shows that word, so
  blanking it still puts the class name back. A second click on the same chip
  points at the mask the first one made rather than adding an empty copy of it,
  as long as that mask is still untouched; a double-click therefore makes one
  mask, not two. Measured on 35 frames, see ENGINEERING, "Naming the
  animal": no wrong name, and deer, which ImageNet has no word for, stay
  "Animal".

  Later, if it is felt: on a photograph with nobody and no animal in it — a
  landscape, a building — naming the Subject chip runs the subject matte on
  opening, about half a second of background work that only renames a chip.
  It could wait for the chip to be hovered instead.

- ✅ **MASK-013**: Editing a mask is a state the whole panel enters, not a
  section inside Light. The histogram becomes the mask's own header — its
  thumbnail, its name, which of them it is — the panel tints, the canvas says
  which mask is being edited, and the rail drops to the five tabs a mask
  carries. Esc, Done and the back arrow all leave. Before this a selected mask
  was a handful of rows under the sliders, with nothing on screen saying the
  sliders now meant something else. PANEL_PLAN P2, landed 19/20 September.

  The Mask tab holds the shape: Show (Wash / Outline / Points / Matte), Strength,
  Feather and Edge as ordinary slider rows, Draw on it (Brush / Lasso and
  Size), the row of verbs — Refine edge, Invert, Duplicate, Delete — and then
  what the mask is made of. The verbs sit *above* that list, because under it
  they fell below the fold on any mask with three parts, and Delete is not a
  control to go looking for.

  **P2b, mask mode revised (21 September).** The photographer's three
  objections to P2 as built: a mask's own tools do not belong in a panel tab;
  nothing blue may lie over the photograph; the panel's tint and border go. So:

  - The **filmstrip becomes the mask's toolbar** while a mask is edited, one
    row of 56 px under the canvas (`mask_toolbar.rs`): EDITING MASK · the
    chip (the mask small, its name, "1 of 1 · 1 part"; its popover switches
    mask and holds the name, the parts with eye and ×, and a range mask's
    numbers) · Tool (Brush, Lasso, Click, Linear, Radial) · Show
    (any of Wash, Outline, Points, Matte, and the button says
    which) · Shape (Strength, Feather, Edge; the button reads Feather) · ⋯
    (Invert, Refine edge where the model has something to say, Duplicate,
    Delete) · Done. The panel runs to the foot of the window beside it.
  - **The Mask tab is gone**; the rail in a mask is Light, Colour, Effects and
    Detail, with one quiet line above it — "These four tabs edit **Person**"
    and the eye. The path ends in the mask's name, in its accent.
  - **Nothing on the photograph**: no badge, no frame; the wash is white at a
    tenth. What says "you are in a mask" is a one-pixel accent border round
    the viewport, where the photograph is shown — the photographer's
    addition, first round the whole window and then, once seen, round the
    viewport only; transparent outside a mask so entering one moves nothing.
  - **The tool's own row floats** just above the bar, over the canvas column,
    in the OSD's glass — the photographer's, 21 September: Add | Subtract for
    Brush, Lasso and Click, and Size and Softness only for the first two;
    nothing for a gradient, whose tool is its handles. On the bar the sliders
    were its widest thing and folded behind a button wherever it was short of
    room. The toolbar's popovers open over the row.
  - **The tool in hand is plain**: the Tool button sits in a light accent,
    and its menu is the five side by side, without a heading, with the one in
    hand in the full accent. Every menu of the bar opens upwards, over the
    photograph.
  - **Softness** is the brush's hardness, stored per stroke and not the mask's
    Feather; fifty, the value every stroke had while it was fixed, is where it
    starts. A **lasso** has it too (`Stroke::soft_lasso`): a band of Size ×
    Softness, centred on the drawn line, made by filling hard and blurring the
    bounding box — 31–39 ms for a 600-point lasso on a 4096 map. A lasso with
    no softness, which is every one stored before this, fills bit for bit as
    it did; so do brush strokes, checked on forty random ones against the old
    code.
  - **Add | Subtract** is a switch of two — a track, the pressed one a pill
    of accent in it — so which is on is plain, as the photographer asked. It
    stays pressed, and Shift is the other one for as long as it is held.
    Under 750 px they are their signs; the accent stays.
  - **Linear and Radial are tools too.** The artboard took them off the Masks
    tab; they stayed there, by the photographer's rule for the artboards —
    where one leaves out something that was there, keep it. A mask cannot hold
    a gradient as one part among others, so picking one in the toolbar turns
    an *empty* mask into it (or a gradient into the other kind); on a mask
    with parts in it the two are greyed with the reason.
  - **Labels are one line**, ellipsised where the room runs out — the
    artboard set some in two ("1 of 1 · / 1 part", "Wash, / Outline"), and
    the photographer asked for one.
  - **It narrows by itself.** Written out, the bar is 960 px and the row
    above it 708; the canvas column is about 980 at 1366 × 768 and never
    under 680. An `adw::BreakpointBin` measures the bar's own width, which is
    the row's too: under 975 the words that repeat a control go, under 750
    Add and Subtract are their signs, the chip keeps only the name and the
    tool is its icon — each step measured. A control is never dropped.
    Checked at windows of 1366 and 1920.
  - Leaving — Done, Esc, the header's back — lands on the Masks tab with the
    list scrolled to the mask that was left.
### RETOUCH — Repair

- ✅ **RETOUCH-001**: Heal — the source's texture under the destination's tone.
  The honest way to do that is to solve for a surface whose gradients are the
  source's and whose edges match the destination's, which is a Poisson equation
  over the patch. This measures how much brighter the destination's
  surroundings are than the source's and scales by that: one ratio per channel,
  taken from a ring of the same shape around both, so it asks the same question
  of the same neighbourhood twice. On a dust spot in a sky or a blemish on skin
  the two agree; on a patch straddling a hard edge they do not, and that patch
  wants the clone tool.

  Measured: healing a blemish from a source six times brighter lands within 3 %
  of the destination's own tone, where cloning the same patch brings the
  brightness with it.
- ✅ **RETOUCH-002**: Clone, with a visible source. Solid ring for the
  destination, dashed for the source, a line between them. Which is which has
  to be visible without clicking anything — a clone source that cannot be seen
  gets dragged onto the thing it was covering.

  Both run first in the pixel pass, before the noise reduction and the
  sharpening. A spot removed afterwards would have been sharpened as a spot
  before it was removed as one, and the patch replacing it would carry the
  sharpening of where it came from.

  The source is read into its own buffer before anything is written, because
  source and destination are allowed to overlap: dragging a clone source across
  its own patch is an ordinary thing to do, and reading a buffer while writing
  it smears the patch along the drag. Spots are stored against the frame and
  moved into a tile's coordinates when one is rendered, so a retouch stays where
  it was put when the photograph is zoomed into.

  Not carried by copy and paste, unlike every other edit. A spot is a position
  on one photograph; pasting it onto another puts a patch of wall over somebody's
  face.

  A tile is cut out of the working image *before* the pixel pass runs, so a spot
  whose source lies outside the cut has nothing to copy — the sampler clamps to
  the edge and the patch is made of whatever sat at the boundary, silently. The
  retouch would then look one way fitted, another at 1:1, and a third in the
  export, and 1:1 is exactly where a retouch is judged. So the region is asked
  in advance whether it holds every spot's source and destination, radius
  included, and one that does not falls back to rendering the whole frame — the
  same escape HDR and a straighten angle already take. A photograph with nothing
  retouched keeps its tiling, which is asserted.
- ✅ **RETOUCH-003**: Brush controls — size, feather, opacity. The Heal and
  clone page's sliders, per spot.
- ✅ **RETOUCH-004**: Remove — what is under a circle, gone, and the picture
  around it carried in. A third tool on the Spots page, beside Heal and Clone,
  with no source to choose: LaMa (Suvorov et al. 2022, Samsung Research,
  Apache-2.0), the large-mask inpainter darktable offers for the same job,
  whose Fourier convolutions see the whole window from the first layer and so
  continue a wall, a fence or a horizon through the hole where healing smears.
  Each spot is filled from a window five of its radii across, resampled to the
  model's 512 and back, marked eight per cent wider than drawn so its edge is
  not copied back, and composited with a narrow feather — a wide one let the
  removed thing show through it. The fill is kept, per spot and per render
  size, as a ratio to the ring around it, so exposure and white balance move it
  afterwards with its surroundings instead of asking the model again; a tile
  at 1:1 that cannot see the whole window renders the frame instead. 225 ms a
  window on the card, 760 ms on the processor; a 208 MB download. On DSCF1290
  a lamp post comes out of a cloudy sky with the clouds and the far hotels
  continued. *A circle, not a brush: a long thin thing takes several. Not
  generative in Lightroom's sense — nothing is invented that the window does
  not suggest, which is also why a large hole comes out soft.*
- ✅ **RETOUCH-005**: Sensor dust, found. *Find dust* on the Spots page heals
  every speck a dirty sensor leaves, from the frame the masks read. A speck is
  out of focus by the filter stack in front of it — a soft, round, slightly
  darker disc that only shows where the picture is smooth — and that
  description is the detector: a difference of blurs at four scales (six to
  fifty pixels across on the proxy), in stops so a speck is as dark in a
  bright sky as in a dim one; smooth at the finest scale and at the speck's
  own; alone, the difference around it averaging under a quarter of its
  depth, which is what tells it from a window in a blurred town or a pore;
  never deeper than 0.3 stop; and not on skin, where a darker spot is a
  freckle and the Face section's Spots are the tool. Each find is a heal,
  sourced from the smoothest of eight places three sizes away that is not
  dust itself; spots already there are left alone. On the Mallorca frames with
  sky it finds its specks in the sky and nowhere else, where the first version
  found sixty on a face, a town and an arm. 230 ms on a 2400-pixel frame.
- ✅ **RETOUCH-007**: Remove people — Lightroom's Distraction Removal for the
  passers-by. *Remove people* on the Spots page finds everyone in the frame
  with YOLOX-s (Megvii, Apache-2.0, 36 MB), takes the largest to be the
  subject, and turns every other person a quarter of the subject's size or
  less into a Remove spot (RETOUCH-004) round their box — unless they stand in
  the middle of the subject's box, which is someone in front of them. The
  masks' segmentation was tried first and could not do it: on its 128-cell
  grid three figures forty pixels tall behind a woman on a beach were smudges
  below even odds, joined to her by the smudges between. The detector found
  all three in 64 ms, and LaMa took them out with the water and the beach
  carried through and her face and hand untouched. A figure that would need a
  circle more than a tenth of the frame across is left: that is a different
  photograph. *Reflections are not built: there is no open model for them.*
- ✅ **RETOUCH-006**: Pet eye — Lightroom's tool for the glow an animal's eye
  throws back at a flash. A third tool beside Heal and Clone: a circle on the
  pupil, no source. Under it every pixel brighter than a twentieth of what
  the ring around the eye measures is scaled down to that, and turned most of
  the way to grey on the way, so a green glow ends as a dark pupil rather than
  a dark green disc; a pupil the flash missed stays as it was. Size, feather
  and strength are the spot's as for any other.

### FACE — Retouching a face

- ✅ **FACE-001**: Skin, evenness, red eye and teeth — the ones that only make
  sense once a face has been found, and are hidden until one has been. Not
  disabled: "whiten the teeth" on a photograph of a building is a slider that
  can only do harm, and greyed rows are rows to read past on every frame that is
  not a portrait.

  Skin is not smoothed by blurring it. Blurring takes the pores with the
  blotches and leaves a mannequin, which is the most recognisable way for a
  photograph to look retouched. Two guided filters instead: a wide one that
  flattens the unevenness and a narrow one that says what counted as texture,
  with the texture put back on top. What is lost is the band between the two
  radii, which is the blotchy scale and nothing else. A single filter cannot do
  this — there is no radius at which it removes a blemish and keeps a pore.

  Evenness is the same on colour alone, at a wider radius. Blotchiness is mostly
  a chroma problem — the red under one eye, the patch on a cheek — and colour
  smooths much harder than brightness before anyone can tell.

  Where the skin is: an ellipse over the detector's box with the eyes, brows and
  mouth taken back out, then multiplied by whether the pixel is *actually* skin.
  The colour test is on ratios rather than values, so a face in shadow is still
  a face; every tone tried sits above a third of its own sum in red and below a
  third in blue. The mouth comes out as one shape — taking out the two corners
  the detector gives leaves the middle of it in, which is lips, and lips
  smoothed like a cheek is a mouth made of plastic.

  Red eye only fires where the red is genuinely out of proportion: twice the
  next channel, which no iris reaches. It brings red down to the other two
  rather than greying the pixel out, so the pupil stays dark instead of becoming
  a smudge. A brown iris is untouched, which is a test.

  Teeth are the bright, *neutral*-yellow part of a mouth. The first version
  asked whether red and green together beat blue, which red lips answer yes to —
  they are the reddest thing in the frame and have no blue either. Green beside
  red is what separates a tooth from a lip: within a tenth of each other on
  teeth, under half on lips. A shut mouth has nothing bright and neutral in it,
  so nothing happens, which is what makes this safe to leave on.

  The mouth is cut out of the skin softly. It was cut with a ramp a hundredth
  of its own width, which is a step in all but name, and subtracting a step
  from the skin mask draws a seam: smoothed cheek hard against unsmoothed lip,
  which is how a face comes to look like something worn over a face. Measured
  across one pixel at the size a mask is worked out at, the mouth's edge moved
  by **0.63** of its whole range where the eyes moved by 0.04 and the nose by
  0.05. It is 0.04 now, and a test walks three lines across the face and one
  down it to keep every feature's edge that way.

- ✅ **FACE-002**: Spots — take out what is small and temporary.

  A different question from skin smoothing, which is why it is a different
  slider. Smoothing asks "is this skin uneven" and answers over a whole cheek; a
  pimple is not unevenness, it is a *thing*, and to a guided filter it is an
  edge — exactly what that filter exists to leave alone. Turning skin up until a
  spot disappears is how a face becomes a mannequin.

  So: a band filter at the scale of a spot. Two blurs per channel, one at
  0.25 % of the face's larger side and one at 3 %; what lies between them is
  what is spot-sized and almost nothing else. Where it fires, that band is
  subtracted, which leaves the surrounding tone with the pixel's own finest
  detail still on it — skin, rather than a smooth patch.

  All of it on the square root of the data, not the data. Every judgement here
  is about how different two things *look*, and these pixels are scene-linear: a
  blemish that is a tenth on screen is more than twice that here, which is how
  the first version decided every spot was too deep to be a spot. A square root
  is near enough to a display curve and is a square to undo.

  Finding one is two tests, either of which is enough. Darker than its
  surroundings, as a fraction of them. And *redder* than them, as a ratio on
  each side rather than a difference — on light skin a spot is mostly red and
  barely dark at all, and a brightness-only filter finds every mole and walks
  past every pimple. The ratio matters: an absolute chroma difference makes
  anything bright look red, and the shine on a cheekbone is the brightest skin
  in the frame.

  Four things keep it off what is not a blemish, and each one was put there by a
  photograph it had already ruined:

  - One-sided, so a catchlight and the shine on a nose survive. Taking those out
    is how a face stops looking lit.
  - Scaled, so a shadow under a nose is broader than the band and passes through
    untouched.
  - Vetoed on brightness, in either direction: redness is what finds a spot,
    brightness is what refuses one. An eyelash and a nostril are half as bright
    as their surroundings where a blemish is a twentieth. Without this the first
    version drew a bright halo along the jaw.
  - Capped, at a tenth of the surrounding brightness per channel. The size of
    the correction is itself a test, and no threshold on the way in separates
    brown hair from skin as well as this one does on the way out — a strand
    across a cheek is warm, red-dominant and inside the ellipse, and the version
    without this cap turned it grey.

  And a fifth that is nearly free: the face's own average brightness, from the
  skin mask, as the floor under all of it. Hair against a dark background is not
  much darker than what surrounds it, so the veto misses it — but it is two
  thirds of what the cheek is worth, and that is the thing to measure against.

  It runs before the smoothing, and the guide is rebuilt after it: the passes
  that follow decide what an edge is, and by then the spots are not one.

  Where the faces are is derived data, not an edit: worked out by the detector,
  never stored, and found again when the photograph opens — the same
  arrangement a mask's pixels have. The stack says what to do to a face; this
  says where one is. It is not carried by copy and paste, because the next frame
  has a different face or none.

  Measured on a portrait: detection 106 ms, and 138 ms to render with all four
  on. The detector is a quarter of a megabyte — this is not the six-second one.

### PERF — Performance and robustness

- ✅ **PERF-001**: Disk-cached thumbnails, versioned so a rendering change
  invalidates them.
- ✅ **PERF-002**: Full-resolution view wherever the proxy would be stretched,
  rendered one viewport at a time.
  The threshold is the magnification at which the proxy runs out — 2400 pixels
  of 7728, so about 31 % — not a flat 100 %, which called two thirds of that
  range fine while the proxy was already being upscaled. The visible region is
  cut from the full image and reduced to the pixels the screen can show, then
  cached: a slider tick costs 26 ms at 35 %, 45 ms at 81 % and 5 ms at 311 %,
  against 243 ms for the whole frame. Falls back to the whole frame when the stack cannot be split — a
  quarter turn, a straighten angle, or local tone mapping, which measures the
  frame to decide how to compress it. Memory is still the whole frame: the
  colour stage is cached at full resolution, and tiling that too would mean
  re-running it per pan.
- ✅ **PERF-003**: The long jobs can be stopped, and say what they kept.

  Honest about what it can do, which is the whole of the design. A single call
  into a model or a decoder cannot be interrupted part-way: there is no safe
  point inside it and inventing one would mean owning the loop. What *can* be
  stopped is a job made of many such calls, and both of the ones that run for
  minutes are exactly that — measuring a library, exporting a shoot. They check
  between items, which is the granularity the work actually has, and it is also
  where stopping costs nothing: every chunk already measured is in the catalog
  and every file already written stays written.

  So the button appears only where it will be honoured. A Stop that is ignored
  is worse than no Stop, because the next thing the person does is press it
  again. On an export the button goes on the progress toast that is already up
  rather than on a second one beside it.

  Both say what they did rather than just stopping: *stopped after 340 of 2336,
  what was measured is kept*. A job that halts without accounting for itself is
  a job the photographer has to redo from the beginning to be sure.
- ✅ **PERF-004**: Deterministic exports — the same stack on proxy and full.
- ✅ **PERF-008**: Picking a person in the filter bar froze the window until
  the desktop offered to force-quit it. Not slow work: the grid reloaded
  forever, 65 ms a time. Reloading gives the people picker a new model, and
  GTK delivers the resulting change of selection after the guard meant to
  ignore it is already down, so every reload asked for another. The picker now
  does nothing when the person it lands on is the one already chosen. The
  choice was written down before the reload, which is why a restart showed it.
- 🟡 **PERF-005**: The grid decodes what is on screen. `GtkFlowBox` realises
  every child, and every card was holding a 320-pixel texture — 300 kB on the
  graphics card each, which on a folder of 2336 photographs measured as
  **844 MB** of resident memory with nothing scrolled and nothing clicked. A
  screenful is twelve.

  So a card starts empty and `sweep_thumbnails` hands pixels to the band a
  person can see, sixty cards either side, and takes them back from anything
  more than 240 away. Two numbers rather than one, so scrolling a row and back
  does not decode anything twice. Which cards are visible is found by bisection
  over their bounds — they are laid out in order, so eleven measurements answer
  for two thousand — and the sweep is debounced, because a scroll says so on
  every frame and the answer changes by a row.

  Measured after: **215 MB** with the folder open, and **236 MB** after
  scrolling the whole library end to end and back, where it stays. The
  filmstrip is the same list swept by the same code: 2336 frames, 137 decoded.

  Nothing is cancelled when a band moves, which was the first version's
  mistake: a card whose queued decode was thrown away had already been marked
  as asked-for, so it stayed blank for as long as it was on screen. A decode
  against the cache is a couple of milliseconds, so the answer is allowed to
  arrive late and is checked against the card when it does. Only a rebuild of
  the list a job belongs to may cancel it. The widgets
  themselves are still all built; that half is measured and is not the problem —
  2336 cards take 53 ms — so a `GtkGridView` and its factory can wait until a
  library is large enough to make 53 ms into something.

- ✅ **PERF-006**: A drag draws a draft, and the picture sharpens when the hand
  stops. The operation stack over a 2400-pixel proxy costs 30.5 ms, which is 33
  frames a second — a slider that moves in steps rather than smoothly, and one
  core pinned for as long as the drag lasts. The comment above `PROXY_EDGE` said
  8 ms, and was right when it was written; denoise, sharpening, the look table,
  the mixer, masks, grading, retouch and the local tone mapping have been added
  to that stack since.

  | proxy | stack |
  |---|---|
  | 2400 | 30.5 ms |
  | 1800 | 16.1 ms |
  | 1400 | 10.1 ms |
  | 1200 | 7.1 ms |

  So the working image is kept at half its edge as well, which is a quarter of
  the pixels and a quarter of the cost, and every change draws that one. Nothing
  decides whether a drag is happening: the changes stopping *is* the drag
  ending, and 130 ms of quiet triggers the full-size render.

  Lowering the proxy instead would have been one line and the wrong line — it
  costs sharpness at rest, which is when the photograph is actually being
  judged.
- ✅ **PERF-007**: The same pixels, sooner. Measured first, on the 2400-pixel
  proxy of a 40 MP frame, and every change below gives the same answer to the
  bit — the blur has a test that says so against the version it replaced.

  Four things. The column blur ran one column per task, a strided read of one
  float per cache line; it runs in strips of sixty-four columns now, each row
  read once and contiguously, with the running sums in the order they were.
  Every blur in the application goes through it — denoise, sharpening, the
  guided filter, the matte's gate. The guided filter, when it filters an image
  by itself, blurred the cross term and the target's mean, which are the
  squared term and the guide's mean; two of six blurs gone, and its arithmetic
  runs in parallel. The four tone regions cost a `powf` per pixel to decide a
  gain that is exactly one when all four sliders are at rest; they are asked
  first. And a mask's field and the decoder's two whole-frame multiplies were
  each on one thread.

  | | Before | After |
  |---|---|---|
  | Exposure tick, proxy | 44.8 ms | 23.3 ms |
  | Exposure + HDR + clarity + texture | 95.2 ms | 43.8 ms |
  | One radial mask | 58.3 ms | 28.8 ms |
  | Three radial masks | 104.3 ms | 41.4 ms |
  | Guided filter, r = 10 | 36.1 ms | 7.8 ms |
  | Luminance denoise at 1:1 scale | 36.2 ms | 12.7 ms |
  | Full-resolution stack, exposure | 507 ms | 291 ms |

  What is left, in order of what it would buy: the detail passes — retouch,
  faces, denoise, sharpening — are recomputed on every slider tick although
  none of their inputs moved, and colour denoise is on by default; caching
  their output under a key is about 10 ms of the 23. The colour stage's two
  HSV tables each do the RGB→HSV→RGB round trip, 44 ms per white-balance tick
  on the proxy, and could share one. `correct_geometry` is 161 ms of a 616 ms
  decode. Each of those changes pixels in the last bit or needs plumbing, so
  each wants its own pixel-diff test first.
- ✅ **PERF-009**: The renderer stops copying frames nobody was going to read
  again. What freezes a laptop is not a saturated CPU — the scheduler handles
  that — but memory, and a 40 MP Fuji frame is 456 MB of f32 per copy.

  Both halves of a render began by cloning the whole frame: `to_working_space`
  so it could write the colour stage into it, and `apply_pixels` so it could
  write the operation stack into it. Neither had any way of knowing whether the
  caller still needed what it had been handed, so both always assumed it did.
  Now they take a `Cow`, which is that question asked out loud. An `&image` call
  site means exactly what it meant before and pays for its copy; a caller that
  is finished with the frame hands it over and the pixels are worked on where
  they already are.

  Measured on an X-T5 40 MP frame (`DSCF9580.RAF`, 5152 × 7728), peak RSS of the
  render stage alone — `VmHWM` with the mark reset after the decode, release
  build:

  | | Before | After |
  |---|---|---|
  | Export / thumbnail (`develop`, frame handed over) | 2.25 GB | 1.37 GB |
  | Full-resolution colour stage (editor at 1:1) | 0.93 GB | 0.48 GB |
  | The stack over a kept `full_working` | 1.81 GB | 1.81 GB |

  The last row is unchanged on purpose. The editor keeps its working image
  across slider ticks — that *is* PERF-006 — so it lends rather than hands over,
  and the copy is the price of the thing that makes a drag cheap.

  Not done, and bigger than any of this: the decode itself peaks at 1.97 GB to
  produce a 456 MB frame, four times its own answer. That is where the next
  gigabyte is. Taken in PERF-011.

- ✅ **PERF-010**: Work that nobody is waiting for any more is dropped rather
  than finished. A queued thumbnail render used to run whatever happened: a
  card scrolled past was marked unwanted, but the job was already in the queue
  and paid its second and a half of raw decode before the *answer* was thrown
  away. On a shoot with ninety-two edited frames that is four hundred CPU
  seconds spent on pictures that had left the screen. The job carries the same
  question its answer was already checked against, asked before the work
  instead of after it, and a dropped job still counts as done so the progress
  toast does not stall on a total it will never reach.

  With it, the whole frame rendered behind a tile at 1:1 is kept between ticks
  instead of made fresh — half of the 82 ms a pan used to cost. Panning moves
  the tile, not the photograph, so it is held against the same colour key that
  decides when `working` is stale.

- ✅ **PERF-011**: The decode stops holding frames it has finished with. The
  gigabyte PERF-009 pointed at was not four passes each needing a buffer; it
  was one pass needing a buffer and three bindings that had gone quiet without
  going out of scope. `decode_with` is one long scope, so the mosaic and the
  mapped file stayed on the heap to the end of it, `flatten` copied the
  demosaic's output and left the original alive, shadowing kept the bent frame
  beside the straightened one, and `oriented` took `&self` — which also meant a
  456 MB clone of an identical frame for every photograph taken level.

  Scoping, not cleverness: a block around the part that needs rawler,
  `into_flatten` where `flatten` copied, one `drop` where shadowing hid one,
  and `into_oriented` taking the frame by value. Two places that held a second
  copy while making the first were fixed too — the Markesteijn path takes the
  float buffer `apply_scaling` has already made instead of copying it, and the
  default crop shifts rows down inside their own buffer rather than gathering
  them into a new one.

  Peak RSS of one decode, `VmHWM` in a fresh process, release build:

  | | Before | After |
  |---|---|---|
  | X-T5 40 MP, proxy demosaic (`DSCF9580.RAF`) | 2.02 GB | 1.27 GB |
  | X-T5 40 MP, Markesteijn (editor and export) | 2.03 GB | 1.27 GB |
  | X-T20 24 MP, Markesteijn (`DSCF2455.RAF`) | 1.24 GB | 0.79 GB |
  | Sony A7 24 MP, Bayer (`DSC07881.ARW`) | 1.24 GB | 0.77 GB |

  Every pass after the demosaic now runs flat at one frame. The floor is
  rawler's own demosaic, which holds the mosaic, a cropped copy of it, a
  per-pixel bounds table and the output at once.

- ✅ **PERF-012**: Optional GPU acceleration for the three heavy models.
  ONNX Runtime's WebGPU execution provider is a 6.6 MB download beside the
  models — 16 MB once unpacked — offered in Preferences like one; with it the masks and the denoise
  run on the graphics card, measured on an RX 9070 XT through Mesa's RADV
  against Numa's own session settings:

  | | CPU | GPU | |
  |---|---|---|---|
  | EfficientViT, the found masks | 224 ms | 38 ms | 5.9× |
  | SAM encoder, click to select | 671 ms | 160 ms | 4.2× |
  | SCUNet, AI denoise | 484 ms/tile | 181 ms/tile | 2.7× |

  The card gives the same photograph, not just a faster one: the same tile
  through SCUNet on both devices differs by at most 1.4e-6, and after the
  twelve bits the denoised frame is kept at, 174 of 196 608 codes differ and
  never by more than one. Turning it on is a speed decision, not a picture
  decision.

  The other four models stay on the processor because the GPU is slower for
  them — SFace by a factor of seven — and which models ask is a property of
  the model in `numa-infer`, not a flag at the call sites.

  It is switched on the moment the file is there and off again by a switch
  that leaves the file alone, because a driver that starts misbehaving after
  an update is a thing to turn off in a second. A session that will not build
  on the card falls back to the processor and says so on the Info page. And a
  marker written just before ONNX Runtime is let near Vulkan, removed as soon
  as a session has stood, means a crash in the graphics driver costs one
  slower mask rather than an application that will not open: the next start
  finds the marker, stays on the processor, and says where the switch is.
  Built as a plugin ONNX Runtime loads at run time rather than the `ort`
  crate's `webgpu` feature, which links Dawn as a hard dependency and, on
  every machine where WebGPU finds no adapter, segfaults at exit.

- ✅ **PERF-013**: A shoot spends the encode decoding the next frame. The three
  parts of an export do not behave alike — measured per frame on a 24 MP X-T20
  file, decode is 700 ms and gets 5.7× faster on sixteen threads than on one,
  develop is 456 ms and 6.4×, and writing the JPEG is 340 ms and **1.0×**: 345
  ms on one thread and 347 ms on sixteen. So a fifth of every exported frame
  was one core writing a file while fifteen waited.

  The encode is started without waiting for it and collected on the next turn
  of the loop, so it runs against the next frame's decode and develop. Eight
  frames, three runs: 12.0 s in line, 9.8 s one behind — 1.50 s to 1.22 s each,
  which on a hundred frames is half a minute. One encode in flight rather than
  a queue of them, because two would hold two finished frames as well as the
  one being developed.

  The name is chosen on the writing side, not beside the render: `next_path`
  returns the first name no file has yet, and the previous frame's file does
  not exist until its encode has finished.

- ✅ **START-010**: The editor is not built while nobody is looking at it. It
  is 60 ms of widgets, and it was assembled before the window was shown on a
  start whose whole job is to put the library on screen. Built on the first
  idle instead — the gap between showing the window and the first frame, where
  the main loop is waiting on the display server anyway. Measured to the first
  frame, five runs each: a 440-photograph library goes 120 ms → 76 ms, a
  2 319-photograph one 284 ms → 248 ms.

  What the start actually costs, marked from process start with 2 319
  photographs: 44 ms for GTK and libadwaita, 68 ms to the library page, 8 ms
  for the catalog, 134 ms to build 2 319 cards, and the window is on screen at
  342 ms. The 0.78 s that used to be quoted is the time until the main loop
  goes idle, which is work behind a window that is already there.

### UX — Experience

- ✅ **UX-001**: Toast notifications for actions and errors.
- ✅ **UX-002**: Empty state on the canvas and in the library.
- ✅ **UX-003**: Adjustment panel grouped into sections, and a slider that has
  been moved off its neutral is the accent colour where one still sitting on it
  is grey. The question a photograph raises on the second pass through it is
  "what did I do to this one", and seventeen identical sliders do not answer it.
  White balance counts as moved against what the camera chose rather than
  against zero, and the sharpening's neutral is what a RAW is developed at
  rather than nothing — so a default is not an edit.

  Two corrections from an outside review (FT-006): the marks were put on the
  panel's own sliders only, so a mixer band, a grade, a point colour or a
  spot's radius showed nothing however far it had been moved — every slider
  goes through one row builder, which now registers it, and the marks run over
  all of them. And a double-click reset went to the readout's idea of neutral
  rather than the slider's own, so Sharpening and Colour noise landed on 0
  where their neutral is 25; each slider's neutral is registered where it is
  built and both the mark and the reset read it.
- ✅ **UX-004**: Keyboard-first navigation and accessible names. Every button
  that is only an icon takes its tooltip as its accessible name, set once over
  the whole window rather than at each of the places one is made. The editor's
  panel tabs are Alt+1 to Alt+9, beside the culling, compare, guides and
  history keys; the grid was already driven from the keyboard (LIB-015).
- ✅ **UX-005**: Never silently discard work. Leaving the editor saved the
  stack and so did stepping to the next frame, but closing the window did not —
  which made the one door everyone uses the one that dropped the work behind
  it. Ctrl+Q was worse: `app.quit` leaves the main loop where it stands and asks
  nobody anything, so it now closes the window instead and the window's
  close-request writes the open photograph out. A merged bracket still warns
  before it is lost. No prompt on quit, and none wanted: there is nothing to
  prompt about once everything is already saved.
- ✅ **UX-006**: RGB histogram with clipping indicators and overlay, outside the
  scrolled area so it stays put. It is the one thing in the panel about the
  photograph rather than about an adjustment, and it is what you watch while
  moving a slider.
- ✅ **UX-007**: Before/after against the as-shot rendering. Held on Space,
  which the window asks before anything with focus hears it (the
  photographer's, 21 September: "hold it long, but not too long"). A focused
  button — Before itself, once clicked — took the press, clicked itself a
  quarter second later and was left down after the key came up: the canvas
  then stayed on the original while the histogram followed the sliders,
  which read as edits that no longer rendered. Measured under Xvfb with focus
  on Before: shown at 0.45 s and stuck after release before; shown at 0.2 s
  and gone on release now. Text fields keep their spaces, and a window that
  loses the keyboard lets Before go, since the release never arrives.
- ✅ **UX-011**: Logging — warnings visible by default, `RUST_LOG` to widen.
- ✅ **UX-020**: The readout stops lying about what is on screen, and says
  why when the answer is not the original.

  "Sometimes the tile does not trigger and I cannot work out why" is a
  sentence nobody should have to write about their own editor, and the reason
  it could be written is that the application was insisting nothing was wrong.
  The magnification in the toolbar was written by `apply_zoom` and by nothing
  else — so it was right when the zoom changed and stale ever after. A render
  that fell back to the proxy, because a colour change had invalidated the
  original or because it was still decoding, left **233 %** standing over
  proxy pixels. The suffix that exists for exactly this said nothing. It is
  written after every render now rather than only on a zoom.

  And the panel says which of the four it is: waiting on the decode, the
  decode failed, the colour stage moved since the decode, or there is no
  original loaded at all. Every one of those was knowable and none of them was
  said.

  Writing it after every render is also how it came to be written *during*
  one, underneath the mutable borrow that render holds — and reading the open
  photograph there is a panic rather than a wrong answer. Zoomed in, that was
  every frame. It goes after the borrow is let go.

  Two more, on the same path. A decode that lands after the view has gone back
  to fit is no longer wanted: `set_zoom` drops the original on exactly that
  reading, and installing it anyway put the half gigabyte straight back. And
  the flag that says a request is outstanding is cleared when the image in
  hand turns out to be the one that was asked for, rather than being left set
  to send every later decode chasing an answered question.

  A failed decode is remembered against the colour stage it failed for rather
  than as a flag. As a flag, one failure refused every later request for the
  rest of the photograph's life on screen — including requests for a different
  colour stage, which is a different question.
- 🟡 **UX-021**: The rail — the panel's tabs as a vertical column of nine
  icons with their names under them, on the left of the page, and Alt+1…9.
  Six unlabelled icons across a narrow panel was a guess every time (FT-011
  found two of them nobody could name); nine labelled ones down the side fit.
  A 5 px dot marks a tab whose values are not all at neutral, read off the
  page itself rather than from a table of which slider belongs where — P1
  moved nine sliders between tabs and P2 will move more.

  The panel is **372 px** where it was 240, of which the rail takes 66 — so
  the page itself gained 66. Measured at 1920×1080 maximised, the stack gives
  a page **795 px**, and every tab fits in it without a scrollbar:

  | Tab | Content | | Tab | Content |
  |---|---|---|---|---|
  | Light | 774 | | Grade | 333 |
  | Detail | 693 | | Masks | 287 |
  | Effects | 607 | | Retouch | 279 |
  | Colour | 563 | | Crop, Presets | built on demand |

  Two of those were over before P1 moved anything: Light at 830 and Detail at
  797. Light lost 66 when the tone curve stopped measuring itself against the
  panel's full width — the rail is not part of the page it sits beside — and
  Detail lost 104 when two wrapped paragraphs of prose became tooltips, which
  is rule 7 of the plan and was going to happen in P4 anyway.

  **Inside a mask the room is 631, not 795**, because the Done button takes
  the foot of the panel. Every number reported from mask mode before 20
  September was optimistic by 164 px: the measurement was read in the same
  frame that showed the button, before GTK had laid it out. Under the honest
  number the Mask tab was 804 and scrolled; the verbs moved above the list of
  parts so that what has to be reachable is reachable, and the page scrolls
  below that. The other four tabs a mask carries are well inside it — Light
  345, Colour 284, Effects 201, Detail 367 — because everything the mask has
  no say over is gone rather than greyed.

  Two widths GTK had been complaining about in the logs all along, both the
  same arithmetic: a homogeneous row of buttons is as wide as its widest
  button times their number. The mask's four verbs asked for 349 in a 306-wide
  page, so GTK widened the whole panel and cut every other page's readout off
  at the window's edge; the four ways to make a mask asked for 331. The verbs
  size themselves now and the make-a-mask row got the 6 px padding
  `.aspect-ratios` already carried for the same reason. The artboard draws
  both rows even; GTK says four even buttons do not fit 306, and GTK wins.

  The nine icons are traced from the artboard rather than taken from the
  theme. Five themed symbolics beside four drawn here put three families in
  one column — a filled star next to a hairline sparkle next to a
  toggle-shaped mask — and no amount of optical sizing makes those a set.
- ✅ **UX-010**: Info page — camera, lens, exposure, film mode and dimensions,
  as three `AdwPreferencesGroup`s of rows with the name on the left and the
  answer dimmed on the right, which is how the rest of this desktop writes a
  fact. It was one label of hand-spaced markup before. The
  render diagnostics — what is loaded, how big the canvas widget is, what the
  last render came from — are the same page's second half, behind a disclosure:
  they answer "why does this look soft", which is not the question the page is
  opened for. The colour profile left this page for the Colour tab, where it is
  now a choice rather than a note (RENDER-005).
- ✅ **UX-008**: Panel sections. Tabs rather than collapsible sections, which
  answers the same question — one thing on screen at a time — without a
  disclosure to remember the state of. The one disclosure left is on the Info
  page, over the render diagnostics, which are read once when something looks
  wrong rather than while working.
- ✅ **UX-012**: Go through the whole right-hand panel and rework it — what is
  grouped with what, what is shown by default, what belongs behind a disclosure.
  It has grown a section at a time (masks, tone curve, colour mixer, detail) and
  the order is now the order things were built in rather than the order anyone
  works in. Overlaps with UX-008 and should be done as one job.

  Done so far: the histogram sits outside the scrolled area so it stays put; the
  mask control moved to the toolbar beside the crop tool, where a tool that
  changes what the canvas is for belongs; and the panel is the end pane of a
  `GtkPaned`, so its width is whatever the user drags it to and nothing inside
  can change it. That last one was a real defect rather than a preference — a
  `set_size_request` is a *minimum*, so one long entry in a dropdown widened the
  panel and shoved the canvas sideways. Dropdowns now ellipsise instead of
  reporting their longest row as the width they want, and the pane's position is
  floored at whatever the panel measures — asked of GTK rather than assumed,
  because a number written down here would be right until the next section was
  added. Rating moved from five buttons to one, the same way masks did: five
  buttons is five times the width for a value that is one number. And the panel
  now has one scope at a time — the photograph, one of its masks, or its crop —
  with a banner saying which and the way out under it, rather than showing every
  section and leaving the reader to work out which apply.

  And the grouping is done: six tabs under the histogram — Light, Colour,
  Detail, Masks, Crop, Info — replacing one column that held every section in
  the order they happened to be built. Icons rather than words, because six
  labels do not fit across a panel this narrow. Picking a mask goes to the Light
  tab, since that is where its sliders are: they exist once, so they cannot also
  live on the Masks page.

  A mask's own coverage is laid over the photograph while the mask is being
  built — selected, clicked, painted — and goes the moment a slider moves. The
  question has changed by then from "what did I select" to "what did that do",
  and a blue wash over the answer is in the way of it. Touching the mask again
  brings it back.

  And then the toolbar gave up its copies. The crop and mask buttons were the
  same two tools the tabs already are, so the tabs became the state rather than
  a second thing to keep in step with it — `is_cropping` asks which tab is
  showing.

  Where you are is two questions, and they are answered in two places. **Which
  photograph**, in the header, as *library › frame* — replacing the library
  picker, because choosing a library is not something anyone does while editing
  a photograph out of one, and the picker comes back the moment the grid does.
  **Which mask**, in the panel, as a tinted row above the tabs holding the way
  out and the list of the others. Above the tabs rather than on the Light page:
  a mask's tone is on Light, its colour on Colour and its sharpening on Detail,
  and "you are editing the sky" is true on all three. That row is what the mask
  scope was missing entirely — the banner meant to say it was an empty box,
  built, made visible, and never filled.

  The tab strip is homogeneous now, and the panel no longer re-measures itself:
  the pane's floor was recomputed on every position change from what the panel
  measured *at that moment*, and what it measures changes when a section is
  hidden — so selecting a mask or opening the crop tab moved the panel and the
  tabs in it sideways.
- ✅ **UX-009**: Filmstrip in the editor, to move between photos without
  returning to the grid. It scrolls to the open frame by waiting for the strip
  to be measured rather than for one frame of it: the thumbnails arrive from
  another thread, so on the frame after a rebuild the scroller still described
  the strip that was there before and every open-from-the-grid clamped to
  position zero.

  That wait was guarded against a newer photograph having been opened in the
  meantime, and the guard asked `state.open` — which the decode fills
  *asynchronously*, so it was always still empty and the wait broke out of
  itself on its first frame. The strip did not move at all, which is what was
  reported. The guard asks the strip now: the mark is put on synchronously and
  a newer one takes it off again, so it answers the question actually being
  asked. Measured: opening frame 800 of 2336 leaves the scroller at 61796 of
  182218, which is that frame centred to the pixel.

  The strip had the grid's fault in one row: a frame *and* a decode for every
  photograph in the library, queued the moment the editor opened and competing
  with the photograph being opened. It is swept by the same code now — 2336
  frames, 137 of them decoded, centred on the one being edited.
- ✅ **UX-013**: A "Keyboard shortcuts" window, reached from the hamburger menu
  next to Libraries and bound to Ctrl+/. Culling, comparing and moving between
  frames are all done with a hand on the keyboard, and none of them are written
  anywhere but the source — so a binding was only as real as remembering it was
  there. The window lists every one that exists, grouped as the app is:
  Application, Library and Editor, each key combination dimmed the way UX-010's
  fact pages already write one. Nothing here is aspirational; the list is the
  code that reads the keys, not a plan for what should.
- ✅ **UX-014**: The panel, quieter. Seven section headings at full weight, a
  drop shadow under every one of eighteen slider handles, and a bar of eight
  touching saturated rectangles for the colour mixer — each defensible alone,
  and together the panel was the loudest thing in a window whose subject is a
  photograph. Headings drop to a label's weight and a third of the opacity,
  handles lose the shadow and three pixels, tracks lose a pixel, and the mixer's
  colours become spaced dots that dim until chosen. The segmented rows — aspect,
  grading range, mask view — are drawn as a choice rather than as four raised
  buttons, because the only thing on the panel that should look like a button is
  something that does something when it is pressed. Nothing moved and nothing
  was removed; the colour that carries meaning, on the hue and mixer tracks,
  is all still there.

  And every tab opens with a heading, which only Colour and Crop did. A page
  that starts with a bare slider reads as the continuation of something that
  scrolled off the top — so Light opens on Tone, Detail on Sharpening with Noise
  under it, and the rest name themselves.
- ✅ **UX-017**: Hierarchy from state, not from folding. The panel had become
  loud — dozens of full tracks and handles, most of them on sliders that were
  doing nothing — and the two usual answers are both wrong for this program:
  hiding the professional half behind a disclosure is a click on every visit,
  and a Simple/Pro switch is the same thing with a worse name. So nothing
  folds. A slider at rest paints a name, a value and a hairline; hovering it
  brings the track and the handle back, and one that has been moved keeps
  them, in the accent colour, with its value at full brightness. Sizes never
  change between the states, so a hand passing over the page moves nothing.
  One control leads each section at full weight — Exposure on Light — which is
  the whole of the beginner's guidance: the eye starts there. Temperature and
  Vibrance lead on Colour, where colour stays only where it says what the
  slider does: the hue tracks. The mixer's dots sit at a third of their
  strength with the chosen one at full, its saturation and luminance tracks
  are grey until hovered, and the grading wheel is grey while the saturation
  that would apply it is zero — a rainbow on a control that does nothing was
  the loudest thing on the page. Detail, Masks, Retouch and Crop follow the
  same rule, led by Sharpening, Size, Spots and Straighten.
- ✅ **UX-018**: Shift with an arrow key moves a control ten times as far.
  GTK already gives an arrow one step and a page key ten of them, which is the
  right pair of sizes behind the wrong key — nobody reaches for Page Up to
  nudge a slider, and on a laptop it is a chord anyway. Every slider on the
  panel and both of MASK-011's spin rows take it, so Feather and Edge, which
  get aimed more than anything else here, are one with an arrow and ten with
  Shift.
- ✅ **UX-019**: A long job says how far it has got. Analyse puts up one toast
  that stays for the whole pass — "Analysing the library — 128 of 2319 · 6 %"
  and a bar — with Stop on it, counted per photograph as the workers finish
  them. It was a toast per chunk of sixty-four, up after 400 ms and gone with
  the chunk, so minutes of work showed as a flicker with no number in it.
- ✅ **UX-016**: A fourth kind of mask, Click, beside Linear, Radial and
  Brush. A found mask that found nothing is taken back — right, but it was
  also the only way to a mask made by clicking, so when the model missed the
  dog there was no route left to the dog. Click makes the empty mask and warms
  both models for the first click. And the model is asked when the photograph
  opens rather than when a preset is pressed, so the buttons for things that
  are not there are off before anyone reaches them. The toast when a preset
  finds nothing says "could not find", not "there is no": the model missed
  it, the photographer did not imagine it.
- ✅ **UX-015**: One loader for everything that waits. Eight things run off the
  main thread — the two models, the face detector, the culling measures, a
  bracket merge, an export, the full-resolution decode and opening a
  photograph — and each said nothing, or said something of its own. They go
  through one function now: nothing for the first 400 ms, because a chip that
  flashes up for a tenth of a second is noise, then a toast with a spinner
  naming what is being waited for, dismissed when the work lands.

  Work that stays on the main thread cannot have a loader *animate* — it
  blocks the thread the loader would draw on — but where the wait is expected
  the loader can be up before the work starts. Adding and removing a mask show
  the toast at once, defer the work until the frame that draws it has gone
  out, and drop a second click while it runs: the double mask a laggy "add"
  used to produce was two clicks, not one. Both also say so in the log when
  they cross the same 400 ms. Measured headless, everything on that path is small: a gradient's raster
  9 ms, its outline 9 ms, its wash 2 ms, an empty painted mask nothing. What
  was found was repetition: adding a mask traced the outline and built the
  wash four times and rebuilt the layer list twice, and removing one rebuilt
  the list and rendered twice. Once each now. If it still crosses the line the
  log names the function, which is what the next round needs.

---

## Priorities

The status markers above are the roadmap; the tier lists that stood here went
stale as items were built, so this list is generated from them instead. What
is not yet built, in the order the groups appear:

- partly built — **CULL-003** Faces, from YuNet
- partly built — **CULL-004** A suggested rating, 0–5, shown beside the photograph and sortable
- partly built — **CULL-005** A score learned from this photographer's own ratings
- planned — **RENDER-010** GPU pipeline, if the CPU one ever stops being enough
- planned — **DETAIL-008** Settle whether a developed frame is as sharp as Lightroom's and Capture One's, with numbers rather than an impression
- partly built — **HDR-003** HDR output — HDR files built, the HDR screen deferred
- partly built — **MASK-008** Background and people masks, and Person and Animal find a subject the semantic model has never heard of
- partly built — **PERF-005** The grid decodes what is on screen
- partly built — **UX-021** The rail

---

## Design principles

From the roadmap, and binding on anything added here.

**Simple by default, professional when needed.** A beginner sees the controls
that matter; a professional can open every layer of the pipeline.

**Do not simplify by removing.** Simplify by hiding well until it is wanted:

```
LIGHT                     COLOUR
Exposure                  Vibrance
Contrast                  Saturation
Highlights
Shadows                   Advanced ▾
Whites                      HSL
Blacks                      Point Colour
                            Colour Grading
Advanced ▾
  Curve
  HDR
```

**A profile is not a preset.** A film simulation or camera profile decides how
the RAW is *rendered*; a preset is a set of *adjustments* on top. They live in
different places in the pipeline and must stay separable.

**Non-destructive.** The RAW is never written to. Pixels are baked on export and
nowhere else.

**Contextual.** Tools appear where they are used. No modal detours.

**Fast.** Every change is visible immediately. A proxy exists so that this stays
true; anything that breaks it needs a measurement to justify itself.

**Fuji-first.** Fujifilm RAW rendering and film simulations are the quality
differentiator, not an afterthought.

---

## Positioning

> A native, extremely fast desktop RAW editor for Linux, with excellent
> Fujifilm support.

Not a Lightroom clone. The things worth being better at are startup speed,
scrolling a large library, and being right about Fujifilm colour.
