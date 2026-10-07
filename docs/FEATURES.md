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
| `FLOW` | Workflows: Rapid, a library by its moments with a kind of shoot's few controls |

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
- ✅ **START-013**: Crash reports, sent only when the photographer says so.
  Linux; on Apple the system collects crashes itself (TestFlight and App
  Store Connect), for those who share analytics with developers.
  - **What a crash leaves.** A signal handler writes `crash.txt` into the data
    folder when Numa dies of SIGSEGV, SIGABRT, SIGBUS, SIGILL or SIGFPE — a
    driver, ONNX Runtime, Dawn, or a panic in a GTK callback, which aborts. It
    holds the version, how Numa runs (Flatpak, AppImage, a distribution's
    package or built from source), GTK, the renderer, and the build ID with
    where the program was loaded, so a stripped binary's addresses can be
    looked up. The panic hook keeps the last panic with its backtrace beside
    it; in its message every path but Numa's own folders becomes `<path>`,
    and a photograph's name `<photo>`.
  - **Only a crash.** Killed, logged out or the terminal closed leaves
    nothing, and neither does a caught panic; a clean exit clears both files.
  - **At the next start** a toast says Numa closed unexpectedly. "Send
    Report…" shows the whole text, with the graphics card, before anything
    leaves. "Open on GitHub" fills in the problem form; Copy is there for
    anyone without an account, to mail.
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
- ✅ **START-015**: Ideas and tips from the photographer. "Send Feedback…" in
  the main menu (on Apple, the ⋯ menu on the Libraries page): a text field and,
  off unless ticked, the debug information of START-011. "Post on GitHub"
  opens a new discussion in the Ideas category, pre-filled; "Send by Mail" the
  same in a mail to support@numa.photo, for anyone without a GitHub account.
  Problems have an issue form, which About's "Report an Issue" opens.

- ✅ **START-016**: (A second START-010 until 28 September.) The editor is
  not built while nobody is looking at it. It
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

- ✅ **START-017**: Smaller model downloads, the same models. The mirror
  keeps every model file packed beside the original: zstd over the file with
  its float tensors' bytes grouped by position (the HDF5/Blosc shuffle),
  unpacked once as it is fetched and checked against both digests. 2 009 MB
  of models are 1 563 MB to fetch (BiRefNet 973 → 742). SCUNet's weights
  come as float16 and are widened back, the rest byte for byte; float16
  weights were measured for every model and taken only where the answer moves
  less than the card already moves it. A mirror without the pack, or a Numa
  from before, fetches the original as before.

- ✅ **START-018**: A card or camera plugged in is a banner under the header,
  "EOS_DIGITAL · 312 new photographs", with one button, Import…, which opens
  the Import dialog on that card. The card is read beside the window first,
  and nothing is said when none of it is new; the banner goes when the card
  is pulled out. It was a toast that stayed until closed.

- ✅ **START-019**: A quiet first start. Nothing opens by itself: the
  Additional files dialog is a banner over the libraries, once, "Some tools
  need extra files: Select, Faces, AI Denoise", whose Download… opens it; the
  update question, which came as an alert half a minute in, is a banner too
  and only for the AppImage — a Flatpak or a distribution's package is
  updated by what installed it. The empty page says "No Libraries Yet", "Add
  a folder of photographs to start. Numa never moves or changes them.", with
  Add Folder… and Import from Card…, under the mark (the app icon in the
  light theme). No banner shows in the editor.

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
  already has — the same name, size and first and last 64 kB; off a camera on
  its cable, where GVFS fetches a whole file to read any of it (2 s a RAF), the
  same name, size and time, whole quarter hours apart within a day for the
  time zones — is left on the card and said once ("3 already in Mallorca"). The rest is split into shoots
  where the camera was put down for two days, and each is offered a place: the
  library whose dates it falls in or beside (two days either way), else a new
  folder where new ones go, read off the libraries there are — a year folder
  when most of them sit in one (`Fotos/2026/`), their common folder otherwise —
  named for its first day and renamable in the dialog; "Change…" puts it in
  any folder. An optional pattern renames (`{date} {time} {n} {name}`, one
  count per frame so a RAW and its JPEG keep one stem). The copy was APP-005's (since 4 October IO-027's: hashed, synced and read back) —
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
- 🟡 **APP-007**: Other languages. The photographer's, 29 September: the Apple
  app only, every word through its String Catalog, English for now (Numa-mac,
  `docs/APPLE_PLAN.md`). The crates hand over what they say in parts — a
  history step (`history::Step`), a mask's label (`masks::MaskLabel`), what a
  found mask holds (`segment::Named`), the numbers behind a cull note
  (`notes::DetailLine`) — and Linux's English is those parts' `Display`, word
  for word what it was; Analyse's closing line the app words from
  `learn::Outcome`. Linux itself stays English. Open: errors are still
  sentences.

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
  JPEG XL as its `xml ` box. An AVIF has no box for it — the container writer
  makes an Exif item and nothing else — so there the same four are EXIF's own
  tags: Artist, Copyright, Rating and XPKeywords (26 September). *Remove location*
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

  The Sony ILME-FX2, which rawler did not know, has an entry in Numa's copy
  (29 September): the 7CM2's matrices, the same sensor, the matrix LibRaw
  gives the FX2. All four of its raw.pixls.us frames open; nothing else
  renders differently (`docs/ENGINEERING.md`, "The Sony ILME-FX2 in rawler").

  How many cameras that is, as the README, the metainfo and numa.photo say it
  (850 on 29 September): each name a camera goes by, counted once — its
  clean make and model and each of its aliases (the Canon 450D is also the
  Rebel XSi), not its modes (sRAW, compressed) and not the raw make strings,
  which name one maker several ways. Counted from rawler's camera files:

  ```sh
  python3 -c 'import tomllib,glob; print(len({(c["clean_make"],m) for f in glob.glob("vendor/rawler/data/cameras/**/*.toml",recursive=True) for c in [tomllib.load(open(f,"rb"))] for m in [c["clean_model"]]+[a[1] for a in c.get("model_aliases",[])]}))'
  ```
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

  A library whose drive is not connected can still be looked through and
  culled: "ik wil voor alle numa's nog dat je library ook kan bestaan uit
  mappen waar je nu geen access toe hebt". Beside the application's catalog,
  `offline/<id>.db` lists each library's photographs — path, dates and the
  name of its biggest thumbnail in the user's cache — and nothing of its
  ratings, flags or edits, which stay in the folder. Kept when the library is
  opened (at most daily), after a scan that changed something, on switching
  library and on closing. With the drive away the grid is that list, with the
  cached thumbnails, a "source not available" mark on each card and a banner
  saying so. Stars and flags work and go into `offline/<id>-marks.db` with
  when; editing, export, pasting, the trash, Analyse and Rescan wait. When the
  folder is back (on opening it, on the rescan triggers) the marks go into its
  catalog — unless it holds a later decision about that photograph — the file
  is deleted and a toast says how many came in. Core: `Catalog::is_offline`,
  `reconnect`, `keep_for_offline`.

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
  postcard size; failing is none of those. It is logged once a session —
  it had been every opening of every file, one frame seven times in a row
  while tethered (7 October) — and a file found to need its preview goes
  straight to it the next time, without the decode that fails: 17.6 ms an
  opening before, 7.4 ms after, on a Fujifilm HEIC of that day. The
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
- ✅ **IO-017**: A bigger thumbnail answers a smaller request. Grid
  thumbnails come in a short fixed ladder — 640, 960, 1280, 1920 — whose step
  follows the widest monitor, and a cached one at a larger step is scaled down
  for a smaller one, so a new screen costs at most one pass and a smaller one
  nothing. The library's own 320 stays apart. (Added to the index 28
  September.)
- ✅ **IO-020**: A capture date is read from the first 256 KB of a raw, not
  the whole file (kamadak-exif on the head; the old paths as the fallback).
  Same date on all 560 files of the corpus; two RW2s gain one.
- ✅ **IO-021**: A look at a raw — a thumbnail, a summary — reads the pages it
  touches: rawler maps lazily, a dummy decode reads no image data, ARW's
  whole-file copies are gone, the film mode is looked for in Fujifilms only.
- ✅ **IO-022**: ORF previews from the JPEG in the Olympus maker note — the
  grid, the camera view and the E-M1's exposure match, as for every other
  make. The SP-5xxUZ have none and keep the develop.
- ✅ **IO-023**: The library follows its folders without polling: a local one
  is watched (GIO file monitors), any one is compared by its folders' times
  when the window comes to the front, a rescan that found nothing changes
  nothing on the main thread, and the once-a-minute walk is gone. A walk asked
  for while another runs waits for it (30 September): dropped, a library
  opened during a start was neither walked nor watched.
- ✅ **IO-024**: JPEG export through jpeg-encoder (pure Rust): the same
  baseline 4:4:4 JPEG in 60 % of the processor time.
- ✅ **IO-025**: JPEG XL lossy export at effort 3 instead of 7: an eighth of
  the processor, the file a little smaller. Lossless keeps 7.
- ✅ **IO-026**: A thumbnail from the smallest embedded JPEG that is still
  big enough (a CR3's PRVW, a NEF's second JPEG, a DNG's preview), not the
  largest.
- ✅ **IO-027**: Two copies, both checked. The Import dialog's Copies group:
  Second Copy (a folder on another drive, asked for once with Add a Second
  Copy…, remembered on this machine, a switch to leave it out) and Check Both
  Copies (on by default). Each file is hashed (xxh3-128) as it is read off the
  card, written under `.part`, synced and renamed; then read back from the
  disk — its pages dropped from the cache first — and compared. The second
  place gets the library's checked copy on a thread of its own, so a slow
  drive trails without holding the card up, and is read back the same way.
  "Safe to Format" only when there are two places, every copy of every file
  matched, every raw decoded (IO-028) and every frame on the card has a
  checked copy; otherwise the done page says what is missing, lists every
  problem by name, and Try Again copies those over the same files. Most cards
  are never formatted and carry frames a library already has: the done page
  has a row for them ("13 older frames · Already in Spring Walks", with how
  long reading them off the card takes) and Check Them Too, which hashes each
  card file beside its library copy and the second place's where there is
  one, writes a receipt for what matched, and then says Safe to Format — or
  names the frame whose copy is not the same and keeps it off (no Try Again
  there: which side is right is the photographer's to say). A
  copy that read back different is not left behind. Without a second copy:
  "Copied and Checked — add a second copy to be safe to format." The library
  is catalogued and opened as soon as it has every file; the done page comes
  when the rest is done, with Eject Card where the card is a drive of its own
  (never a folder on the photographer's disk). Tried in the rig on CC0 RAFs:
  both places, receipts checked by `xxh128sum -c`, Try Again; Eject Card and a
  real card reader not (`offload.rs`, `import_done.rs`, `import_verdict.rs`).
  Check Them Too tried on a card of 13 older frames and one new: Safe to
  Format; then with one library copy and one library-and-second copy
  damaged: both named, not safe.
- ✅ **IO-028**: The card checked while it can be reshot. Every raw of an
  import is decoded once in full (the whole mosaic, nothing developed), from
  the checked copy, four at a time beside the copy, and a frame that does not
  decode is named: "DSCF4204.RAF could not be read — reshoot while you can."
  Tried with a RAF cut short at 15 MB.
- ✅ **IO-029**: A receipt for every import, in the plain format `xxh128sum`
  writes — a hash and a path per file under a few comment lines (date, card,
  counts, places, the command that checks it) — in the library's
  `.numa/receipts/` and in the second place's `.numa-receipts/`; Receipt ·
  Show on the done page. Check Copies Again (Preferences › Imports, and on
  the done page; on by default): once a month, ten minutes after Numa opens
  and not on battery, every file a receipt lists that is still there is read
  again at the idle priority, and only one that no longer matches is said, in
  a toast.
- ✅ **IO-030**: Look first, on the card. The Import dialog shows the new
  frames' embedded previews in a strip; Look at the Card… (or a frame in it)
  steps through them full size — ← →, X leaves a frame on the card, P picks,
  0–5 rate, U clears. Frames left on the card are not copied ("2 left on the
  card", "Import 10"); the marks come into the library with the rest.
- ✅ **IO-031**: Ratings from the camera. The stars a camera wrote
  (`xmp:Rating`, which Fujifilm, Canon, Nikon and Sony put in the head of
  their raws, or EXIF's Rating) go into the catalog on import when the
  photograph has none and was not marked on the card; the done page says
  "2 rated in the camera". Tried on RAFs rated with exiftool; no camera-rated
  CR3, NEF or ARW was at hand.
- ✅ **IO-032**: Previews first, the raw later. JPEGs a camera's phone app
  brought across (Fujifilm XApp, Sony Creators' App, SnapBridge) are found
  in the libraries when the card's raws are read — the same base name, the
  same moment to the second, and the same body serial and firmware where both
  files carry them — and the Import dialog offers "Already Here as JPEGs:
  14 from the X-T5 app · the raws take their place · stars, flags, albums,
  names and crops stay", a switch, on. On import each raw becomes that
  photograph: its stars and flag (where the card gave none) and its albums
  move to the raw, the names given to faces, a track's place and the shape
  of its edit (crop, straightening, perspective, turn and flip) are given to
  it; colour and tone set on the JPEG stay behind, and the done page says
  what was carried. The JPEG stays in view as the raw's pair, as LIB-018
  shows a RAW+JPEG shot together — unmarked, with its own edit, and set
  aside by RAW Only like any pair — so one frame is never picked or exported
  twice. A renamed JPEG, one saved again by another program, or one beside
  its own raw already is not touched. Tried in the rig
  with CC0 RAFs and 1600 px JPEGs made from them with the camera's EXIF;
  measured on 2 313 RAFs of the archive (ENGINEERING); no real app transfer
  was at hand.
- ✅ **IO-033**: Places from a track. The Import dialog's Places group, From
  a Track…, takes a GPX file or a Google Timeline export (Takeout's
  Records.json, the monthly Semantic Location History, or the on-device
  Timeline.json from Android or iOS), places each frame at its moment in UTC
  (its clock less the zone it wrote) between the fixes either side, and says
  "286 of 310 placed · 24 outside the track · 2 placed by the camera". A gap
  over a quarter of an hour between two places is not guessed across; a long
  stay in one place is. The catalog keeps the place and an export writes it
  as EXIF GPS where the camera wrote none (Remove location leaves it out);
  nothing is written into the raw. Later, from the library's main menu, Add
  Places from a Track… does the same for the selection or the grid, to
  photographs with no place yet, with Undo. Tried in the rig with a GPX and
  an Android Timeline export over CC0 frames.
- ✅ **IO-034**: Proof for Print — the photograph shown as a printer or lab's
  ICC profile would print it (perceptual or relative colorimetric, the
  paper's white shown), Out of Range as a neutral hatch, a state pill at the
  photograph's top centre; profiles added from a file and kept in Numa's
  data folder, sRGB always there. Only the view changes. The export's Proof
  With converts JPEG, PNG and TIFF to an RGB profile and tags them with it.
- ✅ **IO-035**: For a Book — the picks or the selection, in capture order,
  as numbered folders a chapter each (days, or albums and Other) with
  numbered files, sized at 300 ppi for a photo book (CEWE, Albelli, Popsa,
  Blurb or a custom size), sRGB or a lab's profile, through the export.

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
- ✅ **LIB-002**: Filter and sort the grid. A filter that narrows the grid is
  in the accent colour; the order and its arrow are one linked control, and the
  arrow — a plain one of Numa's own, Adwaita having only chevrons — points the
  way the grid runs (22 September). A rating given while the grid is filtered
  on flags no longer rebuilds it: only the filter on what was changed can drop
  a photograph, and the rebuild flickered the library at every star and
  cancelled the thumbnails still loading.
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

  FT-029 makes it the cull. Up picks and Down rejects, and both go on to the
  next frame at once: setting rather than toggling, unlike the bar's buttons,
  because a second Up has already moved on and a key that sometimes takes the
  mark away has to be looked at before it is pressed. On the last frame the
  mark lands and a toast says why the loupe stayed. Backspace takes the last
  mark back — whichever key or button gave it, and a star as much as a flag —
  and returns to the frame it was given to, with no question asked: the mark
  is one key to give again. What it can take back is kept only while the loupe
  is open. The grid's other keys are held off while it is: Up, Down, Home, End
  and Ctrl+A used to reach the grid underneath and move the selection while
  the loupe went on showing the frame it had, so the next P landed on a
  photograph nobody was looking at. Delete is held off too — beside Backspace,
  the key that takes a reject back, and a cull must never be one slip away
  from losing a file; a reject never removes anything. A mark that takes a
  frame out of the grid's filter no longer closes the loupe: the grid is
  rebuilt when the loupe closes, as it is when the editor does, and into the
  editor straight from the loupe it is the editor's to do.

  Bursts (CULL-002) are visible in it. Under the name, a bracket with one mark
  per frame of the burst — open, picked or rejected, this one larger — says
  which frame this is, which the grid calls best, and why: the sharpest face
  where there is one, else the sharpest frame, which is what `best_of_each`
  ranks on (eyes are not measured, so it does not say eyes). Ctrl+Left and
  Ctrl+Right go by burst and land on its best frame, the one to judge the rest
  against; the arrows alone still go frame by frame, into a burst as into
  anything else. Shift+Down rejects every frame of the burst not yet marked and
  goes on to the next — Up never does that by itself, since a pick says "this
  one", not "only this one" — and one Backspace takes all of it back. A burst
  is a stretch of the grid in its own order, so a sort that scatters one shows
  it as frames on their own.

  The frames before and after sit dimmed in the space either side of the
  photograph — about 8 % a side for a 3:2 frame on a 16:9 screen, most of the
  screen beside a portrait one — worked out from the photograph's own shape
  each time it is laid out, and left empty where the space is too narrow to be
  more than a stripe. A click on one goes there; F puts them away for the frame
  alone and is remembered. They are also what makes stepping immediate: the
  loupe holds the frame on screen, the one before and the two after, loaded
  ahead of the grid's own queue, so a step shows a frame already in hand rather
  than going blank while it loads. A picked frame leaves upwards and a rejected
  one downwards, 180 ms over the next frame that is already in its place — a
  layer of its own, so the next photograph never waits for it, and an Adwaita
  animation, so it is not there at all where the desktop has animations off.
  Nothing on the library side is styled for any of it: the dimming is the
  widget's opacity.

  And 1:1 is in the loupe too, with the editor's gestures: a double-click goes
  to 1:1 about the point clicked and a second one back, the wheel zooms about
  the middle, and a drag pans. What is shown at 1:1 is the frame developed at
  full size — the best demosaic and Numa's untouched rendering, about 1.9 s for
  a 40 MP RAF — since the camera's embedded preview is 4416 pixels across a
  7728-pixel frame and too small to judge focus on. The view zooms at once on
  the frame already on screen, says "developing at full size…" under it, and
  sharpens in place when the development arrives. A step keeps the
  magnification and the place in the frame, so a burst is judged on the same
  eye in every frame, and while zoomed the next frame is developed ahead, one
  at a time — each is half a gigabyte while it runs — so after the first the
  step is sharp at once. Out to the whole frame lets them go. A click on the
  photograph no longer closes the loupe: the first click of every double-click
  did. Zoomed in, the neighbours and the slide are not shown — there is no side,
  and the eye is on the same corner of the next frame.

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

  30 September: the loupe's five stars, Pick and Reject are in the header's
  start while it is up, where the editor has them since UX-024 and drawn the
  same way; adding and importing step aside meanwhile. The bar under the
  photograph keeps the arrows, the pick target, Edit and the grid.
- ✅ **LIB-013**: A mark in the grid and the filmstrip on photographs that have
  adjustments. Opening one is not editing it: an untouched document is stored as
  nothing at all, so the mark means what it says — which is the question a shoot
  raises on the second pass through it.
- ✅ **LIB-014**: Names on faces. The info page lists the faces in the open
  photograph, each with a picture of it and a name: typed once, and every other
  photograph with a face like it offers that name, saying it is a guess — Enter
  confirms, typing corrects. Only what was typed is stored; the name reaches
  photographs by likeness, so one named today reaches the ones taken last year.
  SFace (OpenCV Zoo; weights Apache-2.0, training data research-only, so for
  non-commercial use only; 37 MB) aligned on YuNet's five points; see
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
  Since 0.34.0 the Libraries page has shelves for Albums and People too,
  after the libraries', each a stack like theirs.
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
  reached from "Open Presets Folder". A name already taken is refused, not
  replaced. On the Looks tab (UX study, 30 September) the four full-width
  buttons under the cards are a + (Save Current as Preset…) and a ⋯ (Import
  Presets…, Import a Folder of Presets…, Open Presets Folder) at the end of
  the PRESETS label, so more cards fit. The presets are not rows in the menu itself: a GTK menu builds
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

  **Numa comes with twelve looks**, in a group "Numa": Soft Film, Vivid, Warm,
  Cool Fade, Moody, Golden Hour, Matte, Teal & Orange, Cross Process, Vintage,
  Silver and Noir. They are JSON in `crates/numa-io/looks/`, built in, and
  `presets::seed_looks` writes each into the presets folder the first time a
  version with it starts — Linux in `main`, the Apple clients through the
  bridge — so a deleted one stays deleted (`.looks-seeded` remembers) and a
  new one arrives with its version. Tone, curve, mixer, grade, grain and
  vignette only: never white balance, detail or the crop, so they read the
  same on a finished JPEG as on a raw. `tests/looks_sheet.rs` draws them side
  by side, a raw and its JPEG each, for judging by eye.
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
- ✅ **LIB-023**: One Filter button and a bar of chips, in place of the second
  row of drop-downs under the header. The header's end in the grid is the
  sizes, Analyse, Filter and Export. Filter's popover holds every facet as a
  section: Rating (Any, 1+ to 5), Flag (All, Picked, Unflagged, Rejected),
  Analyse (Only Questionable, Only Best of Each Burst — out of Analyse's
  menu, so Analyse is a plain button now), Type, and Folder when the library
  has folders. While anything narrows, a slim bar under the header has a
  chip per facet with its own cross, "5 of 72" and Clear, and the button
  says how many ("Filter · 2"); with nothing narrowing the bar is gone. The
  order moved to the main menu's first section, Sort By, five radio items
  and Reverse Order. Rescan left the header for the main menu, "Rescan
  Library" (F5), since the folders are watched; People left the filter row
  for the main menu too. Each library keeps its filter as before.
- ✅ **LIB-024**: A bar for the selection. With one or more photographs
  selected, a bar slides up at the foot of the library: a cross that clears
  the selection, "4 Selected", Rate (No Rating, 1 Star … 5 Stars), Pick,
  Reject, Paste Settings, Add to Album (the albums and New Album…), Merge…
  (the HDR merge, only with two or more; it left the header) and, at the
  end, Export 4… for the export dialog. The keys are in the tooltips. It
  replaces the floating line of keys ("0–5 rate · P pick · X reject") and
  stays away while the loupe is up.
- ✅ **LIB-025**: Rate from the grid with the pointer. The five stars a
  hovered card shows are the editor bar's in small: the star under the
  pointer fills the ones before it, a click gives that rating to this
  photograph alone (not the selection, and without selecting or opening
  it), and clicking the rating it already has takes it off. Linux and the
  Mac — on a touch screen there is no hover, so a tap stays the card's
  (the photographer, 4 October).
- ✅ **LIB-031**: Tonight. From the selection bar's ⋯ and the library's main
  menu, a dialog with the day's picks (or the selection): "24 picks from
  today, with their look", Size (Long edge 2048 by default), Into (a folder
  per evening, ~/Pictures/Tonight/<library> · <day>, Change…) and Every Day
  of the Trip; Make 24 writes sRGB JPEGs without a watermark through the
  ordinary export, and a toast says "24 in Tonight" with Open Folder — Linux
  has no share sheet, so the folder is the hand-off. With Every Day of the
  Trip on, an import that brings in a day with picks makes that day's pack
  by itself, once a day, its toast staying up until seen. Linux; the iPhone's
  version (S4b, a shared album) is not built.
- ✅ **LIB-026**: Camera clocks. A second body, a second shooter's camera
  or a phone a few minutes off no longer puts its frames out of order: each
  library keeps an offset in seconds per camera (make, model and body
  serial; make and model where there is no serial), in its own catalog so
  it travels with the folder. Everything ordered or grouped by capture time
  — the grid's date order, bursts, the same scene again, the library
  picker's dates, Rapid's moments — uses the corrected time, and a frame
  scanned in later lands on the same timeline. A burst stays one camera's
  sequence: lined-up frames of two bodies meet in the same scene again, never
  inside one burst. Camera Clocks… in the main
  menu lists each camera with its photographs and its offset, with a switch
  to apply it; every change re-sorts at once and has Undo. The Info page
  shows the corrected time and, where an offset applies, the camera's own
  clock beside it.
- ✅ **LIB-027**: Finding a camera's offset, offered and never applied
  silently. From one moment two cameras both saw: near-identical frames of
  two bodies within twenty minutes of each other (the burst hashes of
  CULL-002), at least three of them agreeing within five seconds. From the
  shooting rhythm, for shooters who share no framing: everyone fires at the
  ring, the kiss, the goal, so the two bodies' shots second by second,
  cross-correlated within twenty minutes on the days both shot, peak at the
  offset — taken only when the peak stands at least twice as far above the
  usual as the next best lag and as where the clocks are now, and only
  where it agrees with any look-alike frames there are. From a
  clock photo: Clock Photo… shows the time with a QR code of it, redrawn ten
  times a second, and a frame of that screen found when the library is
  analysed gives that camera's offset; such a frame is a slate, left out of
  bursts and suggestions, and its Info says so. Or set by hand. What is
  found is offered in a banner over the library with the frames that show
  it — "Two cameras saw the same moment · The A7 IV's clock runs 3 min 12 s
  ahead of the X-T5" — with Line Up (Undo in its toast) and Not Now, which
  is remembered. One camera, or matches that do not agree: nothing is
  offered.
- ✅ **LIB-028**: Search the library by what is in the photographs, in
  words, English or Dutch — "bride with bouquet", "reiger", "kitchen" — on
  this computer. Ctrl+F or the magnifier at the start of the library's
  header opens a field under the header; the answer is one more filter,
  within the others, best first, and the chip row says "Words: …" with its
  ✕ after All · Picks · ★ 3+, the count on the right. A model (SigLIP 2,
  Apache-2.0, 412 MB) is asked for, with its size, when the field is first
  opened, and the libraries the grid shows are then read in the background,
  one at a time, on the quiet lane (PERF-072), never on battery or power
  saving, and silently: UX-019's toast for a long run only while a search
  waits on it. The numbers are kept per library in
  `.numa/words.bin` beside its catalog, not in it, and a photograph whose
  file changes is read again. People are found by name, which the model
  cannot do (the photographer, 7 October: "als ik zoek op frederique krijg
  ik nu random foto's te zien"): a word that is someone's first name, or
  words that are a whole name, accents or not, narrow to the photographs
  that person is in as the People place has them (LIB-014), every named
  person at once for "tijmen en frederique"; the words left over ("op het
  strand") go to the model and order those, and names alone need neither the
  model nor its reading. An import, or a tethered session, has what
  came in read for words as it lands (the photographer, 7 October: "met de
  import zelf ook de zoekwoorden … en de foto geanalyseerd"), as a search
  would — once the model is here, never downloading it unasked — and the
  library it opens is analysed as Analyse would. Linux; the core is shared.
- ✅ **LIB-029**: Things Numa Saw. A section in the Filter menu, after the
  others: a fixed list of 150 everyday things a photographer points a camera
  at (bride, bouquet, beach, dog, mountain, church, car, food, sunset, …),
  each a chip only where some photograph has it, the most frequent first —
  outlined and in italic, so they read as Numa's rather than the
  photographer's, and Hide turns the section off. A chip narrows the grid
  like any filter. Worked out from LIB-028's numbers, never written into
  keywords, the catalog or an exported file.
- ✅ **LIB-032**: The client's picks, back from a proofing gallery. Pixieset,
  Pic-Time, ShootProof, Picdrop and SmugMug hand the photographer a list of
  the file names the client marked; Paste the Client's Picks… in the
  library's main menu takes it as it comes — commas, spaces, semicolons or a
  name a line, quoted or not, with or without the extension, Lightroom's
  "A OR B", a gallery's copies ("DSCF2210-2", "DSCF2210 (1)") and the names
  Numa's export gave ("DSCF2210 - edited #007", any saved template), in any
  case. Matched against the whole library on screen (every library when the
  grid spans them), not what the filter shows; a RAW and its JPEG are one
  name and both are marked. Under the list as it is pasted: "41 of 42
  found", how many in each folder, and every name not found, by name. Make
  Them: Picks, 5 Stars or Add to Album… (an album by that name, made if it
  is new); one Undo in the toast takes all of it back. The other way, Copy
  File Names (main menu, and the photograph menu) puts the selection's
  names on the clipboard, each frame once, for a gallery's search.
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
  A rating or an edit is on the file it was given to, as for any card. A
  pair made afterwards — the JPEG a camera's app brought and the raw that
  took its place on import (IO-032) — is shown the same way: two cards side
  by side in capture order, the marks on the raw.
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

  FT-029: a frame also joins its run when it comes within two seconds of the
  frame before and looks like that one — twelve bits. The anchor alone cut a
  burst with a moving subject into pieces: nineteen frames in twelve seconds,
  one to nine bits apart frame to frame, made three runs, because a few
  seconds in the subject had moved too far from where it began. On the Japan
  raws this merged 95 runs into their neighbours, and every one looked at was
  one scene shot a frame or two a second; a pan over minutes still ends its
  run.

  FT-029: the camera's own JPEG or HIF of a raw frame — the same name and the
  same capture second, in whatever folder it was filed — is in no run. It sits
  right beside its raw in capture order and looks the same, so every shot was a
  burst of two: in the 4 644-file Japan library 1 586 of 1 969 runs were one
  RAF and its HIF, and "best of burst" was on 1 888 cards, often the hidden
  copy. Grouped without the twins it is 297: one per real run of more than
  one frame.
  The name alone would not do: a Fujifilm counts to DSCF9999 and starts again.
  A twin takes its raw's suggestion and learns nothing twice. With "RAW only"
  the grid and the loupe are the raws and nothing else, as fast as before —
  the loupe was showing the raw's own preview already.
- ✅ **CULL-006**: Frames with next to nothing in them (FT-029 C7) — a lens
  cap, a flash into the dark, a frame gone white. The spread of the luma that
  CULL-005 already measures, under 0.01 of its 0..0.5: across the fifteen
  libraries here that is seven raws, of which five are blanks, a black and a
  white, and two are frescoes shot so far under that the preview is black. So
  the card says what the frame looks like — "nearly black", "nearly white",
  "nearly blank" — rather than calling it a mistake, and "Only questionable"
  includes them. Its suggestion is 0: a lens cap's frame is sensor noise, all
  fine detail, and measured sharp it came out five stars. The first real
  photograph above the line, an aquarium in the dark, is at 0.012 — and a
  nearly black frame just above it still measures its noise as sharpness,
  which is C3's to fix, not this line's.
- ✅ **CULL-007**: Culling to a number (FT-029 C9). The loupe's bar counts the
  picks in the library — the whole shoot, whatever the grid is filtered to — and,
  once one is set, against how many it is being culled down to: "⚑ 73 / 50",
  in Adwaita's warning colour while there are more. Pressed, it asks for the
  number; 0 is none. Kept in the library's own catalog, in a small settings
  table of its own, so it goes where the folder goes. Per library rather than
  per folder inside one, which is the same thing as long as a library is one
  shoot. While that field has the keys, the loupe's own are left alone: Up
  there is one more, not a pick, and Escape closes the field.
- ✅ **CULL-008**: The same scene again, later (FT-029 C8). Not a burst: a
  composition tried again minutes or days on. The difference hash alone could
  not say it — across six libraries it put ten pairs of different photographs
  within burst distance for every two real ones — and a perceptual hash of the
  frame's low frequencies alone did no better. The two summed did: nothing but
  real pairs up to 24 bits (a garden four minutes apart, a statue by day and
  lit, a mountain road, a staircase, a beach with kites); at 25 the first
  wrong one. So Analyse measures both, and each frame is linked to the nearest
  frame of the same scene in another run at least two minutes away — blank
  frames (CULL-006) and a raw's own JPEG or HIF left out. The loupe says it
  under the name: "same scene as DSCF3274, 4 min later". Measuring the second
  hash bumped the analysis version, so the next Analyse measures afresh.
- 🟡 **CULL-009**: Where the camera focused (FT-029 C1). The loupe draws the
  AF point as a box — a single point small, a zone or wide area larger — in
  Adwaita's accent colour over a dark line, and a double-click inside it goes
  to 1:1 on the AF point itself, which is the look that answers "is it sharp
  where it focused". Read from the file when the frame is shown, so it needs
  no Analyse: Fujifilm's `FocusPixel` (0x1023), in the pixels of the RAF's
  embedded JPEG before it is turned upright, found by walking from the RAF
  header to the JPEG's frame header; the orientation from that JPEG's EXIF.
  F puts it away with the neighbours. **No verdict yet.** Comparing edge
  sharpness at the AF point with the sharpest part of the frame was measured
  on 154 frames: the eight lowest were all an AF point on something with
  nothing to judge — a hazy skyline, a white curtain, a dark wall — and no
  missed focus among them, so a "focus missed?" note would mostly fire on sky.
  That waits for frames known to be missed. Other makes keep their AF point in
  MakerNotes of their own shape (Nikon and Canon AFInfo2, Sony FocusLocation)
  and are not read yet.
- ✅ **CULL-010**: The shutter as a reason (FT-029 C6). Analyse keeps the
  exposure time and the 35 mm focal length from the EXIF, and a frame that is
  soft *and* more than five stops slower than 1/focal-length says "soft · slow
  shutter", with the stops in the tooltip. Five, not the rule as taught: on
  2 313 raws from a stabilised X-T5, frames up to 32 times slower than 1/f
  measure as sharp as the rest (median 0.41–0.43 against 0.42) and only past
  that does the median fall (0.25) — the 1/f rule would have blamed the
  shutter for 454 frames it had nothing to do with. Past the line are light
  trails on purpose as well as shaken frames, and the shutter cannot tell them
  apart, so it is a reason given beside "soft", never a note of its own and
  never a verdict; a sharp long exposure says nothing. Telling shake from a
  moving subject from missed focus in the pixels needs a sharpness map per
  region, which does not exist yet (C1, C3). No 35 mm focal length in Canon's
  CR2 and CR3, Olympus's ORF or the Leica DNG, and Panasonic's RW2 has one the
  EXIF reader cannot open: no reason given for those.
- ✅ **CULL-011**: Clipping on the raw (FT-029 C2). Analyse reads the raw's
  own values — not demosaiced, 15–45 ms for a 40 MP RAF — and counts the 6x6
  blocks of photosites with one within 1 % of the white level, and those with
  none above 1 % of the range over black. "Blown", its filter and the
  suggestion's penalty follow the raw where there is one, the camera's JPEG
  where there is not. The JPEG is a rendering, and its highlights go white
  well before the sensor's: of the 51 frames of a trip it showed over a tenth
  blown, 23 held under a tenth in the raw and 10 none at all — a white Osaka
  sky at 36 % in the JPEG whose brightest photosites stop two thirds of a stop
  short of full. The tooltip gives both, and says "the raw holds it" where
  that is so, and how much of the raw is deep shadow.
- ✅ **CULL-012**: What was decided, kept (FT-029 C10). Every mark given in
  the loupe — pick, reject, clear, a rating, Backspace's undo — is written to
  the library's own catalog with its time, and so is every frame looked at and
  left without one: "passed", with how long it was looked at. That is the
  training data CULL-005 lacks: the stars say what was kept, not what was
  seen and passed over. Nothing learns from it yet. A general aesthetic score
  (NIMA, MUSIQ) as a tie-breaker is not built: there is no small, general,
  well-provenanced ONNX one (CULL-004), and within a burst of technically equal
  frames the choice is expression and gesture, which such a score is least
  able to judge.
- ✅ **CULL-013**: The shoot as a tape, under the loupe's photograph (Resolve's
  Source Tape, Final Cut's skimmer; drawing O6-Band). Every photograph of the
  grid the loupe was opened from is a 5 px mark at its capture time, 7 apart
  within two seconds; a jump in time adds a gap that grows by the doubling up
  to a fixed 30 px from five minutes on, where the clock is printed (the day
  where it changes). The frame on screen is the tall white mark; picked is
  white, rejected dimmed, and each star makes a mark 2 px taller — the tile's
  judgement at five pixels wide. A raw's own JPEG or HIF (CULL-002's twin) is
  its raw's mark and is marked with it. The pointer skims: the loupe shows the
  frame under it, selection and stars included, and goes back when it leaves;
  a key while skimming makes that frame the one, a click goes there. A drag
  chooses frames, Shift+click chooses up to a frame; the line above says
  "Frames 27–38 · 12 photographs" with Reject 12, Pick 12 and Rate, and X, P,
  U and 0–5 act on all of them as one mark: one Backspace, or the toast's
  Undo, takes it back. Escape lets them go. Skimmed frames are not "passed"
  in CULL-012's log: they were glanced at, not looked at. In time order
  whatever the grid's sort. Pointer only: choosing frames has no keys yet.
  One strip under the photograph (UX-029): the loupe's name, burst line and
  bar (previous, next, the pick counter, Edit, back to the grid) moved into
  the tape's line beside Review, the chosen frames in its middle.
- ✅ **CULL-014**: Review — the shoot run through at full size in the loupe
  (Resolve's Fast Review): Review in the tape's line or Shift+Space, from the
  frame on screen to the end, or through the frames chosen on the tape. About
  six frames a second; half a second on a frame with nothing within three
  seconds of it; ten a second inside a stretch of eight or more frames each
  within two seconds of the next. "Reviewing · 6 a second" at the top centre
  of the photograph while it runs. Any key stops it on the frame on screen
  and then does what it does there — X rejects that frame, Up picks it and
  goes on, Left goes back one for a key pressed a frame late; Space and
  Escape only stop. What is seen is what is marked: the key is not moved back
  to a frame shown a moment earlier. It shows the loupe's own picture, the
  camera's preview from the cache, and holds the next twelve frames ahead;
  measured on a 56-frame demo shoot, cold and warm, it keeps the pace exactly
  with no frame waited for, and unpaced it shows 18–27 frames a second
  (`docs/ENGINEERING.md`, "Review, and the tape it runs on"). Frames shown by
  Review are not "passed" in CULL-012's log either.
- 🟡 **CULL-015**: The choosing screen — the loupe grown into the one place a
  shoot is chosen in (the night study of 5 October, "Kiezen": nine places with
  five ways of deciding become one; the photographer: "ik wil dit wel
  hebben"). A frame in a burst opens the burst side by side, up to twelve at
  once in as many columns as make each frame the largest, the frame with the
  keys ringed as the grid rings a selection, a rejected one dimmed: choosing
  within a series is comparing (Chang et al. 2016; Mantiuk et al. 2012), and
  one frame and a bracket of pips asked the photographer to remember the
  others. Stepping into a burst from outside it lands on its sharpest frame,
  so the first Up keeps the frame Numa measured best with the rest on screen
  to say whether it is; Shift+Down puts the rest out and goes on. Two keys a
  burst. P and X now go on as Up and Down do — one rule wherever one
  photograph has the keys. Z, or a double click on a frame, looks at one frame
  alone, where 1:1 and the AF box are; the next burst opens side by side again.
  What Numa saw is one pill at the photograph's top left, where the design
  system keeps what Numa did: "Soft · slow shutter", "Eyes closed?", "Nearly
  black", "Same scene as DSCF3274, 4 min later", the first of them and "+2",
  all of them in its tooltip; nothing at all when there is nothing to say. In
  a burst each frame says its own in one word, what every frame shares is said
  once over the burst ("Soft · all 6"), and the sharpest says so. While the
  loupe is up the library's chips, its key hint and the header's end (size,
  Analyse, Filter, Export) step aside: the audit counted 34 things on screen
  while a frame was judged. The dimmed neighbours are off unless F turns them
  on (the tape says what came before and after); the AF box no longer goes
  with them. Rapid's stacks open here too (FLOW-018, "A burst in one look").
  Not yet: faces of a burst side by side, Compare folded in, Backspace beyond
  the loupe.
- 🟡 **CULL-003**: Faces, from YuNet (OpenCV Zoo, MIT, 232 kB) through
  `tract`. Turns the frame measure into the one that matters for a portrait: a
  crisp background with a soft face scores well on CULL-001 and is a reject.
  Measured on real frames — one at frame 0.53 with a face at 0.20, another the
  other way round at 0.22 and 0.41. Run `dev/fetch-models.sh`; without the model
  the columns stay null, which the grid reads as "not looked for" rather than
  "nobody here". FT-029 C4: whether the eyes are closed is asked of OpenVINO's
  `open-closed-eye-0001` (46 kB, Apache-2.0, in the Faces download), on a crop
  of each eye from the preview at full size. Trained on drivers in infrared, it
  is right about a closed eye about three times in ten on photographs; asking
  for both eyes closed, the face turned to the camera and the eyes no darker
  than the face (sunglasses), five of six were right — the sixth a stone statue.
  So the card asks "eyes closed?", and the frame is never marked for it.
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
- ✅ **DOC-008**: How It Was Made — the photograph's own edit played back on
  the photograph, from the camera's starting point to how it looks now, for
  learning from an edit (one's own, a look's, a shared look's). In the editor's
  main menu, the photograph's section. A card at the top left of the photograph,
  the place of what Numa did, on the photo-side ground: `HOW IT WAS MADE` and a
  ×, then a row for each step — what changed with its value and the tab it is
  on ("Exposure +0.35 · Light"), and under it one line of why when a look
  wrote one. Rows already in are white, the coming ones dimmed; Back, Pause or
  Play and Forward at the foot, and the look's line beside them ("Look “Golden
  Hour” · by Numa", or "3 of 8" for a step of one's own). About a second a step,
  the photograph crossfading in 300 ms; with Reduce Motion it is only stepped.
  The steps are the history Numa already keeps, read through `made.rs` (here
  rather than in the interface, so the iPad and Mac read the same words):
  consecutive moves of one slider are one step, and a look, a paste or a reset
  is split into its settings, each with the photograph as it stood once that
  one was in; more than eight settings at once that are not a look read as one
  row. Playback never touches the edit or the history: the canvas is handed the
  state on show through `rendered_document`, and the card closes the moment
  anything is done to the photograph, or another is opened.

  A look file may carry notes: an optional `by` and `notes`, one line of why
  under each setting's word (`Highlights`, `Colour grading`), in the same JSON
  as the edit. A Numa before this ignores them and one after keeps them
  through an import, so a shared look keeps its notes. Numa's own twelve looks
  carry theirs, and a test holds each one to say why of every setting it
  changes and of none it does not. Which look a step came with is kept in the
  library's catalog (`made_looks`) by a fingerprint of the edit it left, with
  the look's name and notes as they were, so it survives a look's file being
  edited or deleted, the history's hundred-step limit and an Apple client that
  saves the history without knowing of it. Linux; the iPad and Mac read the
  same steps from `numa-io` and draw their own card.

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
  A table's value axis is read with sensor white at 1.0, as the DNG spec and
  RawTherapee have it; Numa's own fitted tables carry a signature and keep
  Numa's scale. Before, RawTherapee's A6000 look turned blue skies cyan.

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
  *4 October:* RENDER-024 follows each photograph's own camera JPEG instead
  of a table per simulation, and so carries the recipe a table cannot.
- ❌ **RENDER-007**: ~~Classic Negative, Nostalgic Neg and Bleach Bypass as
  cube LUTs.~~ *Withdrawn with RENDER-006.*
- ✅ **RENDER-008**: Per-image exposure matched to the camera's own rendering.
  27 September: measured after the white balance, not before — every make
  had landed 0.07-0.32 EV above its camera. The corpus test holds each to 0.15.
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
- ✅ **RENDER-010**: The decode's passes on the graphics card. After rawler
  has read the file, everything that was a pass over the whole frame on the
  processor — black and white, the demosaic (PPG, Markesteijn for X-Trans),
  the lift, the lens's falloff and geometry, false colour, the turn and the
  shrink to the proxy — runs on the card through wgpu (Vulkan), and only the
  proxy comes back; 1:1 and export get the whole frame the same way. A jump
  to a photograph not decoded ahead is 0.19–0.41 s where it was 0.39–1.26
  (an X-T5 316 ms from 1260), for a third to three fifths of the energy,
  and the picture is the processor's to within a level in 8 bits (two on
  a few X-Trans pixels). The processor stays the reference and the
  fallback — no card, a card still starting, a model on the card, an error
  or too little memory, and Preferences' "Use GPU acceleration" off all mean
  the processor, as before; `NUMA_GPU=0` forces it. On the Apple apps the
  same passes run on Metal (RENDER-018). Frugal
  (PERF-024), 1:1 and the export take the integrated GPU and never wake a
  discrete one, and the editor's proxy comes from the mosaic (PERF-030),
  which measured as cheap as the integrated card and several times faster
  (`docs/ENGINEERING.md`, "The decode on the graphics card" and "Four
  branches, one decode").
- ◻️ **RENDER-015**: Numa's own camera profiles for every camera, fitted as
  RENDER-012 does — from CC0 raws first (raw.pixls.us, the embedded JPEG as
  the target), so no profile Numa offers is GPL. Later a call to action in
  Numa asking photographers to send a neutral raw of their camera (with its
  JPEG, or a grey or colour card); the sending itself happens outside Numa —
  "dat hoeft voor mij niet via numa te gaan behalve de cta" (the
  photographer, 27 September). After v1. Since RENDER-025 (4 October) a
  photographer fits their own camera in Numa instead, so the bodies still
  waiting for a second CC0 scene need not ship.

  Built for v1: six, the pilot's that beat the matrix on held-out frames
  (EOS R5, D850, A7R III, E-M1 II, X-S10, X-T4; not the G9, Z 7 or K-1, and
  not the D-LUX, whose one frame was also its training frame). Of those and
  the X-T5 only the X-T5's is left since RENDER-023: the others were held out
  on a frame of the session they were fitted on. Twenty more
  from every body the CC0 raws allow: RENDER-023. Fitted from
  CC0 raw.pixls.us frames only, never the DPReview studio frames.
  The photographer's own ("die zijn van mij nu, bundel ze met de apps maar
  publiceer ze niet los", 27 September): inside the apps only — Flatpak and
  AppImage in `share/numa/profiles/numa`, Apple in the bundle's
  `NumaProfiles/` — from `data/private-profiles`, which the public copy
  leaves out; no download. Offered, and since RENDER-016 Automatic as well.
  The Camera profile picker (on Looks since
  UX-023) is back for them, every entry saying where it comes from (Numa, RawTherapee,
  yours, the camera's matrix).
  Real camera profiles only ("echte profielen"): a Fujifilm film simulation
  — Adobe's "Camera CLASSIC CHROME", RENDER-012's "Numa X-T5 Eterna" — is
  neither listed nor automatic (`dcp::is_film_simulation`).
- ✅ **RENDER-016**: Automatic takes Numa's own profile, and what Automatic
  means is one choice for the whole library. Picking a profile photograph by
  photograph was the only way to Numa's own, and pasting Colour onto a
  selection to get it there took the mixer, the grade and the LUT along
  ("is het gek dat je niet alle colour profiles in bulk meteen kunt
  aanpassen voor een hele library?", the photographer, 27 September).
  Preferences › General › Colour › Camera profile: Numa's own (the default),
  RawTherapee or Adobe (Adobe Standard first), or Camera matrix; a camera without
  the one chosen gets the next in that list (`dcp::Automatic`). A photograph
  given a profile of its own keeps it. The choice is part of the decode
  cache's key and of an edited thumbnail's, so changing it re-renders what
  was made under the old one; the open photograph is opened again.
  With it: a profile is only used on the camera it was made for
  (`inputs::chosen_profile`). Pasted or preset from another body — "Numa
  X-T5" onto an A7R III — it had coloured that sensor with the X-T5's
  correction; now the photograph falls back to Automatic, and the picker
  says so.

- 🟡 **RENDER-017**: The editor's render on the graphics card. The
  proxy's render — colour stage, colour noise reduction, exposure, contrast,
  the tone regions, saturation, vibrance, the mixer, the curves, crop,
  straighten, perspective and turns — runs on the card that developed the
  proxy, within one level in 8 bits of the processor's on ten bodies, and
  the frame goes to GTK as a dmabuf where the display can take one. A stage
  the card does not have yet keeps the whole render on the processor. The
  render on the card is on since 29 September, tried by the photographer
  (about 2.5 ms a drag frame; `NUMA_GPU_RENDER=0` turns it off); the dmabuf
  hand-over to GTK, where its first run logged page faults, stays behind
  `NUMA_GPU_DMABUF=1` until it is tried on the card (`docs/ENGINEERING.md`,
  "The render on the graphics card"). On Apple since 29 September (Numa-mac
  `27ed0c1`, TestFlight 43), behind RENDER-018's switch: on the MacBook nine
  bodies at most 1 level from the processor (mac-bench run 6).
- ✅ **RENDER-019**: The render on the card takes the grade, point colour
  (and Show affected area), black and white and the vignette, from the
  numbers the processor's own code makes; within one level in 8 bits of the
  processor's (`docs/ENGINEERING.md`, "The card's stages — 29 September").
- ✅ **RENDER-020**: The render on the card takes masks: the processor makes
  each mask's field, the card keeps it while only the adjustments move and
  runs the mask's white balance, colour noise reduction, basic adjustments,
  mixer, point colours, curves, black and white, grade and Color on its copy.
  Since 4 October everything else a mask has too — dehaze, luminance noise
  reduction, sharpening, defringe, moiré, HDR/Clarity/Texture and grain — on
  a copy of the frame of its own, as the processor's `apply_masks` has it;
  and the photograph's own dehaze, sharpening, defringe, moiré and grain
  with them. What the stages before the operations leave is kept on the
  card between renders, as on the processor, so a drag after them does not
  run them again. Of the photographer's 280 edited photographs 268 render on
  the card now (253 before); what is left is AI denoise, spot removal,
  another working space and calibration (`docs/ENGINEERING.md`, "Every mask
  on the card — 4 October").
- ✅ **RENDER-021**: The render on the card takes luminance noise reduction
  and HDR, Clarity and Texture: guided filters and box blurs over planes of
  log luminance on the card, the local tone map's pivot as a reduction.
- ✅ **RENDER-022**: Which device renders an edit. A discrete card never
  clocks down while a drag asks for 60 frames a second, so a frame costs it
  about the same whatever the edit; it takes a frame only when the edit would
  cost the processor more (`card::Plan::heavy`, measured stage by stage) —
  on a 2 400-pixel proxy a mask's curves, a grade, point colour, the mixer
  with the tone sliders, masks with HDR and a grade. An untouched photograph's Exposure drag went back to the processor,
  0.46 → 0.15 J a frame. Integrated GPUs, Apple and frugal mode keep what they
  did (`docs/ENGINEERING.md`, "Which device renders a drag — 29 September").
- ✅ **RENDER-018**: The decode's passes on Metal, for the iPhone, the iPad
  and the Mac. Safe math (a patched wgpu-hal: fast math failed the tolerance
  on five of nine bodies), unified-memory buffers, 8 M-pixel submissions;
  every plane its own buffer and in a buffer the passes before it are done
  with, so a 24 MP frame fits an iPhone's guessed 256 MB / 1 GB in f32 and a
  50 MP one in f16 (1:1 only, never an export). The editor's proxy on Metal
  on a Mac or iPad, frugal or not; the mosaic's on an iPhone; 1:1 cut from a
  frame Metal developed whole. Linux's card output is bit for bit main's.
  On in the Apple app since 28 September (Numa-mac `0508fbf`, TestFlight
  41): the MacBook passed the tolerance test in safe math on 0.29.0 — nine
  bodies, at most 1 level in f32 (2 on the X-T5) and 1–2 in f16 — and an
  iPhone's guessed limits put 20–24 MP frames on Metal in f32, 40–50 MP in
  f16 and the rest on the processor (mac-bench run 5, Numa-mac
  `docs/APPLE_PLAN.md`, "Metal"). Add-ons › Speed › Graphics Processor
  (Metal) turns it off. Not measured on a real iPhone or iPad
  (`docs/ENGINEERING.md`, "Metal for the Apple apps").
- ✅ **RENDER-023**: Numa's own profile for every body the CC0 raws allow.
  The raw.pixls.us corpus grew to up to four CC0 raws a body (663 more,
  20.2 GB; Fujifilm, Sony, Canon and Nikon first), and every body with two
  usable raws was fitted as RENDER-015's were, one frame held out: 220
  bodies. The fitter now takes a Fujifilm frame only on Provia at Color 0
  ("echte profielen"), skips a JPEG without colour, one not the frame's
  shape, and a frame whose decode and JPEG disagree beyond any colour error
  (Olympus High Res Shots, FT-030), reads a JPEG tagged Adobe RGB both ways
  and keeps the reading nearer the matrix (101 of 129 such previews are
  sRGB anyway), and holds out a frame of a scene no fitted frame shows.
  Shipped when that frame beat the matrix, and RawTherapee's profile where
  one is installed: 20 (Sony 3, Canon 3, Nikon 5, Fujifilm 0, others 9),
  held-out chroma error 0.0488 → 0.0412 on average. 156 more beat both but
  only on a frame of a scene they were fitted on — most bodies' raws are
  one scene in several compressions — and wait for a second scene. With 220
  more profiles installed a body's first lookup costs about 1 ms more
  (`docs/ENGINEERING.md`, "Fitting a profile against the camera").
  The same rule, applied to the profiles already shipped, took five out
  (29 September): the EOS R5, D850, A7R III, X-S10 and X-T4 had each been
  held out on a frame of their own session. On another scene the A7R III's
  lost to the matrix (0.0105 against 0.0101); the other four have no second
  scene on raw.pixls.us. The X-T5's held (0.0375 → 0.0308 on five frames of
  other scenes, Adobe Standard 0.0372). 21 ship.
- 🟡 **RENDER-025**: A camera profile for the photographer's own camera,
  from their own neutral photographs. A row at the end of Preferences ›
  Add-ons › Camera Profiles ("een klein beetje verstopt", 5 October) asks
  which camera — the bodies behind the libraries' raws, the open
  photograph's first — and says what it takes: at least eight
  raws from that body in its standard colour (Fujifilm: Provia at Color 0;
  Sony: Creative Style or Look Standard; Canon: Picture Style Standard, not
  Auto; Nikon: Picture Control Standard; saturation 0), from more than one
  day. Then it finds them in every library by the style in their maker
  notes, fits a profile against the camera's own JPEGs as RENDER-023 does,
  and tests it on a whole session the fit never saw. Kept only when it comes
  out nearer the camera than what Automatic gave before; then it is "Your
  <model>" in the photographer's profiles folder, and Automatic takes it
  first. A make whose style Numa cannot read counts every frame, and the
  dialog asks for neutral ones. The photographer, 4 and 5 October: "dan
  hoeven we het niet perse mee te leveren maar kunnen we mensen hun eigen
  camera gewoon laten fitten"; "eigenlijk alleen met neutrale foto's";
  "Sony canon en nikon zijn volgens mij allemaal groter dan Fuji dus die
  mensen moeten het ook kunnen doen." His X-T20 (81 Provia frames, 2022):
  −12 % on another day's frames, 11 s (`docs/ENGINEERING.md`, "The
  photographer's own"). Linux only so far.

- ✅ **RENDER-024**: As Shot — a look that follows the camera's own JPEG of
  this very photograph. Every raw carries the camera's rendering of it: the
  film simulation, the recipe (Colour, Highlight and Shadow tone, DR, the WB
  shift, Clarity) and the camera's choices for that frame, which no table per
  simulation can carry (RENDER-006). Fitted per photo in a third of a second
  (`numa_io::camera_look`): Numa's untouched render and the embedded JPEG at
  1104 px, lined up for scale and shift, the JPEG's clipped pixels and the
  edge left out, averaged 4×4; a third-order polynomial in display RGB, 20
  terms a channel, held toward identity by anchors over the cube; above the
  brightest value the JPEG showed unclipped it fades back to Numa's own, so
  the raw's highlight headroom stays. The 61 numbers are kept in the document
  and render identically anywhere; applied as a 33³ table where the LUT is,
  before it, with a Strength, so Light and Colour still act first. Copied
  with Colour, in history and presets; pasted, it follows each photo's own
  JPEG (`fill`). Linux: Looks › From the Camera, under the profile, with what
  the camera was set to (the Fujifilm MakerNote's recipe; "The camera's own
  rendering" elsewhere). Refused where there is no JPEG, for a JPEG or HEIF
  original, a merge, or a JPEG that does not line up (correlation under 0.8).
  `tests/camera_look_fit.rs`, held-out 8×8 blocks: 389 CC0 raws of eight
  makes, ΔE2000 2.85 untouched → 0.96 (p90 5.06 → 2.31); 64 X-T5 frames,
  2.61 → 0.67, where a table per film simulation gets 1.37
  (`docs/ENGINEERING.md`, "As Shot — 4 October"). The Apple apps have the
  document field and the render, not the tile; their export does not fill a
  pasted As Shot yet.

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

  Since 0.24 (licences, for the App Store): Numa's own code in place of the
  crate, which is LGPL. Written clean-room from LensFun's documentation and the
  database, and held to the crate's answers by a comparison over every lens in
  the database (`crates/numa-io/tests/lensfun_compare.rs`). The database ships
  beside the binary as its XML files (`data/lensfun`, CC BY-SA 3.0), not
  compiled in.

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

  Coverage for v1 (27 September, `tests/lens_colour_coverage.rs` over CC0
  raw.pixls.us samples of 42 bodies and ~100 lens names as cameras write
  them): LensFun is keyed on the make and model the file writes ("PENTAX
  K-1", "Canon EOS R6m2", "E-M10MarkIV"), which rawler tidies into names it
  often lacks — so those are asked first, and the K-1, K-3 III, R6 II and
  E-M10 IV are found now. A lens rawler's own list does not know keeps its
  EXIF name. A compact's single lens is taken when its name finds nothing
  (Leica Q, GR IIIx, D-LUX, X100V), never one calibrated through a converter.
  A name of only numbers ("24-70mm", what Canon writes for a Sigma) matches
  only exactly — it took Canon's own 24-70 — and a calibration with an
  extender needs a "+" in the name. 101 of 102 bodies are in the database
  (the GR II is not); a Nikon High Efficiency NEF (Z f, Z 8, Z 9) does not
  decode — rawler has no TicoRAW — while their lossless ones do.

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

  **Original** comes first (UX study, 30 September): the photograph's own
  shape, as it is turned, then Free, 1:1, 5:4 / 3:2, 16:9, Custom — seven in
  two linked rows so none is cut short. While the angle moves, a value HUD
  floats at the top centre of the photograph — STRAIGHTEN and "−1.4°" on the
  HUD's dark ground, 22 px round — and leaves 0.9 s after the last change.
  Done is white, as the mask bar's is.

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
  go the other way). The crop turns with the photograph, as Lightroom's does,
  and a held ratio with it — 3:2 becomes 2:3 (26 September; it used to go back
  to the whole frame with 3:2 still lit).

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
- ✅ **GEOM-002**: Auto perspective, from the buildings' vanishing point
  (1 October; it was a fit of lean against position, whose intercept a
  keystone biases). Perspective › Auto in the Crop tool and **Square Up** on
  Auto's card both read GEOM-005's scene read: the vertical keystone that
  sends the buildings' vertical vanishing point to infinity, `−200·p`, as the
  Vertical slider has it — pressed twice it says the same thing. Only where
  buildings fill 8 % of the frame and the fit's lean is sure to 0.5°: on the
  demo library's distant villages and a hut among palms it came out at −6 to
  −12, unsure by 0.8–2.6°. Refused on a photograph with masks or healed spots,
  which nothing carries across a keystone yet: "Verticals not corrected — your
  masks or spots would move". Square Up takes the roll from the same fit.
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
- ✅ **GEOM-005**: Auto straighten, from what the photograph shows (P1 of
  `docs/PLAN_AUTO.md`, 1 October; the histogram of every line's angle it
  replaced counted a sloping ridge and a receding pier as level). A **scene
  read** (`auto::scene`), made once in the frame as shot — the turn and the
  mirror, no crop, angle or keystone — and kept in the catalog (`auto_read`),
  holds three kinds of reading, each kept as geometry so a keystone changed
  later is taken into account:
  - **The sea** (`auto::sea`): where the segmentation has sky directly on sea
    or water across 40 % of the width, nothing standing on it and no land;
    each column moved onto the strongest change of log luminance within two
    cells along a continuous path, fitted with a deterministic RANSAC over
    every pair of 64 columns and Tukey total least squares, and refused when
    curved, broken or short. A lake's line needs a second reading beside it.
  - **The buildings' vanishing point** (`auto::evidence::vanishing`), fitted
    directly as `l = t + p·(l·y − x)`, so the roll is not biased by the
    keystone; σ by resampling strips of the frame.
  - **Trunks and posts** where there are no buildings: the same fit, at least
    three structures apart that crowd round it, never on sloping, receding or
    slope-following classes, people or animals. **Offered, never applied
    alone**: on the demo library's hills and streams they agreed on tilts that
    were not there.

  Readings that disagree are refused unless it is the sea where the camera's
  pitch puts the horizon; σ above 0.5° is refused; under 0.15° is "Already
  level"; past 8° is "looks deliberate", offered when it is a sea. Auto applies
  a level only where the crop it costs (`auto::corners`) loses under 5 % of the
  area, cuts no face, cuts no person or animal that was whole and none the
  frame already cut by more than a cell and a half, and keeps the horizon off
  the edge, moving the crop within the 5 % if that is what it takes;
  otherwise it offers it. The straighten is Auto's only at 0 or at what
  Auto wrote (`AutoRecord.angle`). Straighten › Auto in the Crop tool reads the
  same scene and applies its answer as asked, naming what it went by.
  Measured on 91 of the photographer's frames (release, 16 threads; a first
  run, with trunks and posts still able to level alone, and without eight sea
  frames that hit an indexing bug fixed since): no level on sloping land or a
  deliberate tilt; turned by known angles, the sea comes back within 0.014°
  median and 0.13° at the 90th percentile, the buildings within 0.08° and
  0.21°; a read takes 0.6 s on the processor. Again on 204 frames from
  Japan, South America, Italy and Maastricht on 2 October: no level on
  sloping land or a deliberate tilt, buildings within 0.10° median on turned
  reads, and one failure — a breakwater in haze read as the sea's horizon, on
  a turned read (`docs/PLAN_AUTO.md`, Status). The camera's
  own roll (Fujifilm, maker-note 0x144d), checked against the read on 34 of
  the photographer's frames: the read's sign, but 0.4° off it at the median
  and 1.4° at the 90th percentile, so it is a bound — a camera that read
  level refuses trunks and posts claiming more than 1.5° — and never a level
  on its own.

- 🟡 **GEOM-006**: Crops offered by Auto, never applied unasked (P3 of
  `docs/PLAN_AUTO.md`, first version 2 October, second round 4 October).
  Auto's card gets a Crop row: up to three proposals, each saying why ("1:1 ·
  the subject on a third · 13 MP"), and a toggle group "As shot · 1 · 2 · 3"
  — each choice its own named step, the light measured again — with the
  proposals dashed over the photograph while "as shot" is chosen; the
  pointer over a number draws that crop and the others faint. Candidates in
  the frame's own shape, 4:5 or 5:4, 1:1 for a compact subject, 16:9 with a
  horizon, and the other orientation for a tall subject in a landscape frame
  (the birder's card), scaled down to 8 MP of the original; scored on the
  subject on a third or centred, room in front of a face turned aside, the
  horizon on a third, area, room round the subject, and the edges — no
  sliver of a passer-by or a thing at the border, no bright mass along it;
  "as shot" has to be beaten by 0.08 or the row says "Nothing in the frame
  asks for a crop". No face cut or crowded at the top; a person may be cut
  at the chest, the hips or mid-calf below their face — never at the neck,
  waist, knees or ankles, placed from the face's height — and otherwise kept
  whole, none cut deeper; an animal whole, by the grid and by YOLOX's box;
  the horizon off the edges. Anyone who is not the subject and shows no
  face may be cropped out altogether, never sliced. A crowd the
  segmentation joins is split into its people by YOLOX's boxes and the
  faces; the subject is the one under the camera's single AF point or with
  a face, standing a point clear of the rest — people alike are one subject
  together, and when together they fill more than 40 % of the frame no one
  stands out and no crop is offered. With nobody in the frame, YOLOX's
  largest animal or thing (a train, a boat) is the subject; with nothing
  named, what the matting model sees stand out (a pagoda). Healed spots on
  the photograph: no crops offered, they would move. Against the
  photographer's own ten real crops: 3 within IoU 0.7, 8 within 0.5, none
  cut through a face, a joint or an animal — the plan's gate is half within
  0.7. Resting the pointer on a number shows the photograph as that crop
  would leave it (5 October), with the light as it is; taking it measures
  the light again.

  The gate is not met and weights do not meet it (5 October): the area and
  content weights at 0.15, 0.08 and 0, 16:9 offered without a horizon, and a
  second place kept for a crop tighter than the first, every combination on
  his 26 crops — at best 4 of the 10, and chosen leave-one-out (on nine,
  tried on the tenth) 1 of 10. His crops are choices the rules cannot
  predict; learning them is P5's, "Edits like you". The proposals stay ideas,
  never applied.

### ADJ — Adjustments

- ✅ **ADJ-001**: Exposure, contrast, highlights, shadows, whites, blacks — and
  Auto, which is a button and never a default.

  **One Auto levels first, then sets the light, and says what it did on a
  card** (P1 of `docs/PLAN_AUTO.md`, 1 October). The button on the Light page
  and the **A** key read the photograph once (GEOM-005's scene read), level it
  where the corners allow, and then measure the light on the frame as
  levelled. Each is its own named step — "Auto · level", "Auto · light" — so
  Ctrl+Z takes them back one at a time. A card at the top left of the
  photograph has a row per part: the level and what said so ("0.5° by the sea
  · keeps 97 %"), or why not ("No horizon or upright lines clear enough to
  level by", "Straighten is yours (+1.0°) — left", "No level: healed spots
  would move"); the buildings' keystone, refused with masks or spots on the
  photograph; and the light ("Exposure and the endpoints set", or "already
  right" when no slider moved). Each row is the part's name over one line of
  why, with a switch: on where Auto applied it, off where it only asks, and
  turning one measures the light again on the frame as it then is. At the
  foot, **Compare** and **Undo All**; no toast beside the card. The sea it levelled by is drawn along
  the horizon with its angle while the card is up, and **Shift+Space** shows
  the photograph as it was before the press. In a burst whose sharpest frame
  is another one (CULL-002's, the faces' sharpness where there are faces),
  the card says which — "Frame 1 of 2 is the sharpest of this burst" — with
  **Open**; and **Same Light** gives the burst's other frames this one's light,
  the way Evoto's Match holds a shoot to a reference: Auto's ends as written
  here, and each frame an exposure of its own so its middle lands where this
  one's does (within 0.05 EV on a synthetic burst metered half a stop apart),
  sliders set by hand on a frame left, levels each frame's own. Where Auto lit a subject, its mask's edge is drawn while the card
  is up, the way a selected mask's is, so a wrong subject shows before the
  light is judged. **Compare** on the card puts the photograph before Auto in
  the reference pane beside it (CANVAS-006's, `hold_frame`). A merge is not
  read; its light only. Not yet: sharpness measured on the subject's box
  rather than the frame.

  **The whole photograph first, a subject's mask only for backlight** (30
  September; the mask's last rule, replaced on 2 October — below). The
  photographer: "het moet de foto netjes belichten over de hele foto heen …
  als een masker daarvoor beter werkt gebruik je wel een masker, maar het
  moet niet zomaar hetzelfde riedeltje doen steeds". A
  subject below 0.40 was lifted to 0.55 in a mask of its own, and a dark
  jumper in a dim room was enough; now the subject has to sit a full stop
  below the frame's own middle as well, and is lifted to the frame's level
  rather than above it, and only a silhouette — under 0.22 on screen and a
  stop and a half below the frame: dark shirts on a sunny pitch were lifted
  1.7 stops by the first rule.

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
  actually run out. It goes up when nothing is near white at all, and
  otherwise the camera's own answer stands. It used to come down too, up to
  ¾ of a stop, when the brightest half per cent was blown; since 5 October a
  blown top end keeps the exposure and Whites and Highlights bring it in,
  because coming down took the facades and faces with the sky (of seven
  daylight frames brought down the whole ¾ on the blind set, four looked
  worse than untouched and one better). Only a subject that itself burns out
  still takes the exposure down. Already-good frames moved: 5 of 30 before,
  2 of 30 after.

  **Since 2 October the subject is lit by the exposure, not by a mask.** The
  photographer: "ik vind het oke als er highlights of shadows clippen als het
  betekent dat we het subject wel goed uitlichten (zonder maskers)". Where
  the semantic model names a person or an animal, the matte has its shape
and it fills 2 % of the frame (a cyclist of a few pixels took a blue-hour
  street to daylight), Auto measures what of it is lit — a person's face,
  and nothing for a person with none to see; an animal whole; with
  Fujifilm's single AF point off the subject, nothing, and on it the face
  nearest it — at its 90th
  percentile, and when that sits below 0.55 on display and a stop or more
  below the frame's upper middle (in shadow or against the light, not a dark
  room), sets the frame's exposure — never so far that the subject's own
  brightest two per cent pass white —
  to bring it to 0.72, up to two stops (one on a finished picture); when the
  subject's brightest two per cent are past white, it brings the exposure
  down until they are not, up to a stop. Highlights then recovers what it can
  of what went up with it, and the card says what it cost. A frame shot a
  stop or more down, or low-key in itself (its middle under 0.15, or a tenth
  of it near black), is not brought up, nor its black lifted ("Exposure set for
  the face (+1.3); the brightest parts may clip"). Auto lays no mask and
  no HDR; one it laid before goes with the next press, and a subject mask of
  the photographer's means the ends set the exposure. The paragraphs below
  describe the mask this replaced.

  **What the file says, and the lights** (P2, 4 October). A stop or more of
  compensation down holds the frame low-key in manual exposure too: the
  photographer's M frames with compensation have Auto ISO moving under them.
  Compensation up is not held — a hall shot at +1 that still came out dim
  wanted lighter. DR200 and DR400 keep their headroom (no positive Whites); a
  frame shot as Monochrome, Sepia or Acros gets no Vibrance, and faces over 2 %
  of the frame cap it at 12. The top end leaves out what the segmentation calls
  a light — lamp, light, chandelier, streetlight, sconce, traffic light — and on
  a night every bright spot under 0.05 % of the frame, so a street of lamps no
  longer takes a night down ¾ of a stop; by day the spots stay (sky between
  branches). Fog, haze and high key (the middle nine tenths within 0.35 of
  display) get no Blacks. A subject all but black against a frame three stops
  brighter, under a warm light or with a stop down dialled in, is kept as a
  silhouette. Measured against P1 on 30 night frames (none brightened), 30
  already-good frames (2 changed, Whites only) and 149 frames from every trip
  (16 changed, none in exposure).

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

  The mask is a recipe rather than the pixels already in hand, so it survives
  being saved: a catalog row keeps which classes a mask is made of, not a
  megabyte of alpha. (This said `Segment`; the code makes `Shape::Subject` with
  the matte on, which is a recipe as well. Corrected 1 October.)

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

  **Only Auto's own mask, and only while it is as Auto left it** (1 October).
  Auto used to delete every Subject mask without a name, and the Subject chip
  makes its mask without one, so pressing Auto threw the photographer's own
  mask away with its curve and its grade. Auto's mask now carries the
  adjustment Auto gave it (`Mask.auto`) and the name "Subject · Auto", and
  `Mask::as_auto_left_it` holds only while the whole mask is still what
  `Mask::auto_subject` made: a tuned slider, a new name, a softer edge, a
  curve, hiding it, inverting it or painting on it makes it the photographer's,
  and so does another mask subtracting it. Auto neither removes such a mask nor
  lifts the subject again on top of it; the toast says the light was set around
  their own subject mask, and the HDR under it stays. Auto's own mask is
  replaced where it stands, with its id, so nothing that refers to it breaks.
  The lift the old Auto left (unnamed, with the photograph's Sharpening and
  Colour noise in it and the edge `set_matte` gave it, `Mask::from_old_auto`)
  is recognised and replaced on the next press; a mask from the builds before
  the Subject chip made local masks could look the same, and would be replaced
  too. A new Auto mask never takes an id that a deleted mask left behind in
  another mask's subtractions. Duplicating Auto's mask makes a copy that is the
  photographer's. Auto never selects its new mask, so the panel stays on the
  photograph and the button stays in reach. A mask selected while Auto measured
  stays selected, and if it is Auto's own and the answer replaces it, its
  sliders are read again, so the next nudge cannot write the old lift back.

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

  **The button is on the page again**: `light.rs` builds and shows it. It had
  been taken off, at the photographer's call, until the subject mask was
  reliably the shape of the subject.

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

  **On 25 September BiRefNet replaced it after all** — the full model on
  Linux, the lite one on the iPad, the photographer's choice after 150 of his
  own photographs side by side with IS-Net, BiRefNet lite and BEN2: it finds
  subjects IS-Net missed, leaves a photograph with no subject empty instead of
  scribbling in it, has the cleaner edge, and stray specks on 26 % of them
  against IS-Net's 46 % — "the most beautiful results". The 17 storage buffers
  above are why Numa rewrites the file once it is downloaded, after which all
  of it runs on the card: 0.4–0.6 s a photograph (ENGINEERING, "Inference
  runtime"). IS-Net keeps answering for anyone who has not fetched BiRefNet
  yet, and is deleted once BiRefNet has loaded.

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

  **What the photographer set, Auto leaves** (1 October). Auto wrote its six
  sliders unconditionally, whatever was there: a bracket merge's HDR 50 went to
  0 on the slider that has no row, and an exposure, a Whites or a preset's
  Vibrance set by hand went to Auto's number. Now the document keeps what Auto
  last wrote (`Document.auto`, an `AutoRecord`), and a slider is Auto's to set
  only while it rests at 0 or still holds that value (`AutoRecord::may_set`).
  Anything else stays, and is named in the toast ("exposure left as it was"),
  HDR apart, which has no row. HDR keeps its place as the one slider Auto sets
  for itself (HDR-001), on the same terms. Pressed again after a crop, Auto
  replaces its own answers and nothing else. The record travels with undo and
  the history (`EditState.auto`), so an undone answer is still Auto's; a step
  that only changes the record is called "Auto". Which
  sliders are Auto's is decided when the button is pressed and checked again
  when the answer lands, so one reset in between stays reset.

  **And the endpoints see the photographer's look.** `display_at` used to
  build its Basic from nothing, so after Contrast +40, a tone slider or a
  curve the ends landed somewhere other than where they were aimed. Whites,
  Blacks and Highlights are now solved through the document's own contrast,
  tone sliders and composite curve (`Look` in `auto.rs`), with the sliders
  Auto may set at rest; a test aims the top end through Contrast +40 and shows
  the old solve missing it. The exposure is not: it answers to the capture,
  decided on the camera's rendering as before, because through the look a
  Highlights −100 that recovers a sky reads as a dim frame and Contrast +45 as
  a blown one, and Auto would move the whole photograph by ¾ of a stop against
  the photographer's own choice. An end the photographer has shaped is theirs:
  with their Highlights or Whites, or a curve that rolls off the white, Auto
  leaves the top end alone, its own earlier answer there included; with their
  Blacks, or a curve that fades the black, the bottom. HDR and Clarity are
  local and not in `Look`, so through a kept HDR the top end lands a little
  short of where it was aimed.

  **Measured on the frame that is shown** — its crop, angle, turn, mirror and
  keystone; the manual Lens sliders are left out, as they are from the mask
  frame (1 October). `auto_tone` measured the
  uncropped, unturned working image while the subject's alpha came from the
  mask frame, which is cropped, straightened, turned, mirrored and keystoned.
  After a crop to the right half a subject at the middle of the picture was
  read at the middle of the whole frame; on a turned or mirrored photograph the
  alpha fell on another region altogether; and a blown lamp cropped away still
  pulled the exposure down. `auto::framed` gives `tone` the working image
  through the same geometry the mask frame is made with. A test finds a dark
  subject through five framings — the old path misread four of them — and
  another crops a lamp away and sees the exposure stop answering to it.

  **The lift is a local adjustment and nothing else.** It was built with
  `Basic::with`, which starts from the photograph's defaults — Sharpening 25
  and Colour noise 25 — so the subject was sharpened and denoised a second
  time inside its mask. It starts from `Basic::local()` now, as every mask
  does.

  **Blacks has purchase since FT-028, and Auto now leaves a dark end where
  this camera renders it** (1 October). "Blacks moves on none" stopped being
  true when FT-028 rebuilt the control, and nobody re-measured: solved against
  `BLACK_POINT` = 0.035 through the render's tone curve, a darkest half per
  cent displayed at 0.06, 0.07, 0.08 or 0.09 — the band this entry measured for
  a well-rendered frame off this camera — came out as Blacks −21, −30, −41 and
  −53. Hidden contrast on almost every frame. Blacks now moves only outside
  0.035–0.09, from Auto's own black point to the top of that band: up to 0.035
  when the end is crushed below it, down to 0.09 (`DEEPEST_RENDERED`) when it
  sits above, and not at all between.
  Computed, not measured on photographs; `docs/PLAN_AUTO.md` §9 is the corpus
  that decides whether the band stays.

  The harness takes the path the application takes now: `run()` renders the
  mask frame, asks `subject_mask` (the named gate the button uses, which the
  harness used to skip) and measures `framed`. `CROP`, `TURN`, `MIRROR` and
  `KEYSTONE` frame the document first (`CROP=x,y,width,height[,angle]` in
  fractions of the frame, `TURN=90`, `180` or `270`, `MIRROR=1`, `KEYSTONE` the
  Vertical amount), `FRAMES_LIST` names the frames, one path a line, and
  `OUT_CSV` appends one row per frame. Cargo runs the test inside
  `crates/numa-render`, so give absolute paths:
  `FRAMES_LIST=$PWD/frames.txt OUT_CSV=$PWD/auto.csv cargo test -p numa-render
  --release what_auto_now_does -- --ignored --nocapture`.
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

  **A JPEG or HEIF has them too** (28 September). A finished picture was
  balanced when it was made, so its Temperature and Tint are a shift from
  that: they rest at 5500 K and no tint, and moving them applies the same
  relative gains a mask's Temperature uses (`to_working_space`). They used to
  be greyed out on Linux and, on the iPhone, movable and doing nothing — the
  photographer: "temperatuur en tint sliders doen het niet", on the phone's
  own photographs. A preset's white balance, a raw's Kelvin, stays off a
  finished picture, as Lightroom leaves one (`presets::at_strength`); the
  neutral pipette still needs a camera's.
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
- ✅ **FILTER-009**: LUTs — `.cube` (1D and 3D, up to 65, `DOMAIN_MIN/MAX`)
  and `.3dl`, imported into the data folder's `luts/` and chosen from a list
  on the Looks tab (Effects' foot until UX-023), with an Amount 0–100; Import
  LUTs… and Open LUTs Folder are the ⋯ at the end of the LUTS label. Kept apart from the
  camera profile, as the design principles require: a profile is how the raw
  is rendered, a LUT is a look over the rendered photograph. Applied where a
  LUT made for an sRGB / Rec.709 screen expects to be — on the display values
  after the tone curve and the curves, before quantisation — with tetrahedral
  interpolation, which keeps a grey ramp grey. The document keeps the file's
  name and the amount, not the table; it travels with the colour when copied
  and in a preset, fades with a preset's strength, and is a step in history
  ("LUT"). A name whose file is gone renders as no LUT. In a wide export the
  table reads that space's values as if they were sRGB's (a `ponytail:` in
  `encode`).
- ✅ **FILTER-010**: Mist — one slider, Black ↔ White, -100..100 through a
  neutral zero, after a Tiffen Pro-Mist in front of the lens. A share of the
  light is moved sideways in linear light, colour by colour, so a sodium lamp's
  halo is orange and the frame keeps its brightness: what a pixel gives up its
  neighbours receive. Two widths, a near halo (1/90 of the long edge) and a far
  one (1/16), each three box passes so a point of light comes out round.
  Below zero Black Mist: only what is over about half a stop above middle grey
  scatters, so the lamps glow and the blacks stay deep. Above zero White Mist:
  everything scatters, a fifth of it over the whole frame, which is the milky
  veil that lifts the blacks. Placed after the tone map, before the masks and
  the grade, where the glass would be. The frame's, not a mask's; copied with
  the tone, a step in history ("Mist"). Worked out at a quarter size: about
  2 ms on a 900-pixel proxy.

  And the light sources glow more than a wall does. The sensor clips a lamp at
  the same white as a lit wall though it was far brighter, and the glass
  spreads light in proportion to how bright it really was; so the light
  sources get twice the scatter, the extra added as glow rather than taken
  from the lamp. It was a second slider, Light sources, for one evening; the
  photographer could not tell what it was for, so it is its resting value and
  always on. A source is found as a morphological
  top-hat in stops (opened over a square 1/30 of the long edge: smaller and at
  least 1–3 stops brighter than its surroundings) that is also 2.5–4 stops over
  the frame's log average — which is what keeps a white car on daylit tarmac
  from glowing like a headlight, the first try's mistake. Tried on night
  streets in Osaka (lamps, the tower's lights, lanterns and signs found; the
  people and the facades not) and a daylit crossing (nothing found).

  ponytail: a frame with mist is rendered whole at 1:1, as Dehaze is, and the
  card hands it to the processor. Handing tiles the frame's halo, as
  `measure_tone` does for the tone map, and a card pass are the upgrades if
  either is felt.

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

  **In groups, not in nouns.** Subject and Background first, then Sky and
  Water, then the groups that are here: Person, Animal, Greenery, Ground,
  Mountains, Buildings — and nothing for a group under half a per cent of the frame, which
  is a smudge. That order is the photographer's (23 September): subject,
  background, sky and water are what a photograph is edited by, and the rest is
  occasionally useful.
  There was a chip per class as well, and it made the panel a list of nouns —
  Windowpane, Curtain, Chair, Signboard — where half of them are things this
  model is not good enough at for the chip to be worth pressing, and the ones
  that are were already inside a group. Nothing is lost: a class-sized thing is
  selected by clicking on it, which picks *that* one rather than every thing
  like it in the frame, and the class under the cursor is a lookup.

  Subject is whatever the photograph is of: MASK-008's matting model over the
  whole frame (`Shape::Subject`), because a kite in flight is nobody's ADE20K
  class and the matte is not asked *which* thing it is, only which pixels are
  it. Until 25 September it was the person or animal when the semantic model
  named one — the Person chip's mask under another name — and on 24 of 150 of
  the photographer's frames that person was a passer-by, a face on a
  billboard or a knee beside two koi while the model had the tram, the
  Buddha or the manta ray. Measured against the model's own answer, Subject
  went from 0.81 to 0.96 agreement over the 150, 40 better and none worse;
  22 of the 24 now select what the photograph is of. Of the other two, the
  model declines a dark doorway (DSCF4009, where the semantic person is kept)
  and on a night street under Tokyo Tower finds only a cyclist (DSCF3532).
  Person and Animal are unchanged, to the hundredth of a per cent on 69
  frames. A stack saved with the old chips opens as the new shape. Background is that mask
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

  Since 29 September a mask's Temperature and Tint work on a mirrored,
  turned or cropped photograph and on the draft a drag renders; the turn, the
  crop and the downscale used to drop the white point they are relative to,
  and there they did nothing. Exports of such photographs change with it.

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

  **Measured on the photographer's own frames** (25 September):
  `tests/group_survey.rs` walks the export's path over 130 RAWs from
  `Fotos/` and `dev/group_sheets.py` makes a contact sheet and counts per
  photograph. Three things came out of looking at them. What a group left
  out was a hole in it: windows, doors, signs, columns, awnings and ceilings
  are Buildings now, rock is Ground, and mountain and hill — in no group at
  all, three per cent or more of fifteen frames and nearly all of a cliff —
  are a chip of their own, **Mountains**; the part of a frame no group
  claims went from 22.6 % to 15.1 %. The sky stopped at the outline of every
  tree, because the model calls a crown "tree" sky and twigs together; each
  pixel it gave a see-through class with some sky in it is now placed
  between the sky's colour and the branches' colour nearby, so the gaps are
  Sky and leave Greenery, and wires and hanging fronds leave the sky. And
  islands of the grid the model barely believed — under five cells, never
  0.7 — are dropped before the edge is found: specks in Buildings 131 to 77,
  Ground 104 to 66. Tried and left out: filling small holes the same way,
  steps and stairs in Ground, a colour guided filter, a wider filter.

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

  It needs its scraps taken out afterwards, because run over a whole frame it
  also picks up a few bits of cloud at the bottom edge, and a subject mask
  with three bits of sky in it is not one. That was the largest connected
  region alone until BiRefNet (25 September), and then it threw away the
  second of three cars, the barrels round a jar, a second boat; a region is
  kept now if it is a twentieth of the largest — BiRefNet's scraps measured
  three and a half per cent at most on 59 frames. Labelled on a slightly
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
  (code MIT, weights for non-commercial use only, `vitmatte_small.onnx`) is
  asked a different question: the
  photograph and a trimap — certainly her, certainly not, decide the band — and
  it answers with coverage per pixel. The trimap is the first matte's: its
  solid part less a fiftieth of the subject is certain, the empty background
  beyond it is certain, and the band between, with any loose coverage near her,
  is asked. From a frame of the original at the mask's raster size, in tiles of
  1024 because its attention over a whole frame would want tens of gigabytes;
  on the card 1.8 s for her border, 6.7 s without one. On DSCF1264 the glow is
  gone and the strands are strands on both sides of her head. Only the
  subjects the first look was sure of are looked at again — a man in a hat
  behind her, whom the first look gave a third, keeps his third. The band is
  an eighth as wide inside the border as outside (25 September): as wide both
  ways, a leg, a telegraph pole or a glass's stem was all band, and in the
  dark ViTMatte decided it was not there.

  On 26 September ViTMatte-S was replaced by BiRefNet lite's matting weights
  (MIT) everywhere, because ViTMatte's weights were trained on Adobe's
  Composition-1k, whose licence says models trained on it "may not be sold".
  On 27 September Linux went back to ViTMatte: the Linux version was then
  free under PolyForm Noncommercial (since 29 September it is GPL-3.0-or-later,
  and Add-ons marks ViTMatte-S for non-commercial use only), and the
  replacement was clearly weaker on dense
  fur (a mean error of 0.16 in the fur of a made-up head against ViTMatte's
  0.04 and the first look's 0.18). The paid Apple app, which is sold, has
  closed-form matting instead (Levin et al. 2008, patent
  expired; `numa_render::closed_form`): the same trimap, solved from the
  photograph's colours, no model and nothing to download. Measured on 24
  portraits with loose hair from AIM-500 and P3M-500 against their true
  mattes, from the iPad's first look, the mean error in the hair was 0.165
  against the first look's 0.170 and BiRefNet lite matting's 0.166, in 0.2 s
  and 0.5 GB rather than 1.9 s and 1.9 GB on this machine's processor; the
  full BiRefNet matting model at 512 was 0.119 against 0.129 for the lite
  one from Linux's first look, at twice the memory (4.2 GB), and not taken.

  It is a recipe (`Mask::fine`), done again when the export resolves the mask,
  from the original; not in a thumbnail, whose source is the proxy. Known: her
  ear came back at about nine tenths rather than whole.

  **Both closer looks are asked for (23–24 September).** Refine edge ran the
  hair trace straight after the edge search, and on a kite against the sky it
  ate the wing tips the edge search had just got right; then every rebuild ran
  them both again. A lasso stroke was added and at once reasoned away — the
  matting model answers "the subject", never "the subject plus the polygon
  just drawn" — and the hair trace ate what was left. So Refine is a split
  button in the mask bar (Edge, and Hair, fur and feathers), ⟳ runs the edge
  search again, and a rebuild — a stroke, a click, a gradient moved — drops
  both flags (`Mask::matte`, `Mask::fine`): they were passes over pixels that
  are no longer there, and neither starts again by itself. The export then
  repeats exactly what the screen shows.

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
  here, and it says it went through the guided filter: until 25 September it
  did not, and a click's border was the grid's, ten pixels a step. SAM decides
  *what*; the others decide where its edge is.

  **25 September: what a click gets, made right on his own photographs.**
  `tests/click_survey.rs` clicks the middle of every distinct thing the
  semantic model finds, and one small thing, on 108 frames from every trip —
  414 clicks, contact sheets looked at one by one. Five changes, each kept
  only where the sheets and the numbers agreed:

  | | before | after |
  |---|---|---|
  | masks with specks elsewhere | 165 | 30 |
  | masks with pinholes | 125 | 18 |
  | click outside its own mask | 13 | 8 |
  | border gradient against its surroundings | 1.06 | 1.34 |
  | border width, px of the proxy | 2.5 | 1.8 |
  | window held per click, median / worst | 342 / 1758 ms | 100 / 402 ms |

  - The guided filter against the photograph, as the semantic masks have.
  - Specks off the clicked piece smaller than a fifth of it go, and closed
    gaps under a hundredth of it fill. A second large piece stays: a person
    behind a lamp post is two pieces.
  - Of the three answers, only those that contain the click, and none that is
    unsure of its own edge (SAM's stability under 0.6) while another is not:
    the spray of letters on a business card became the card, one barrel
    rather than the row, the car without the kerb stripes. Ranked by that
    measure instead of vetoed by it, it chose parts — a knee for a statue.
  - Something small — under a fifth of the long edge — is encoded again from
    a crop two and a half times its size, and the answer that agrees with the
    first is taken, because from close by the model offers the stripe on the
    van rather than the van. A fingernail, a weathervane, a boat at sea get
    their own outline instead of an octagon. It costs one more encoder run on
    that click, 1.3 s on the processor here, and it runs beside the editor:
    the first answer is on the canvas at once and the closer one replaces it
    when it lands. Each click's answers are kept, so a click added to a mask
    does not ask again about the ones before it.
  - A click on the rim of a thing counts as a click on it: a steady answer
    the click misses by a hair wins on its score over a part that takes it.
  - The encoder without ONNX Runtime's arena: 1.8 GB at most instead of 3.7,
    no slower.
  - On the iPhone and the iPad, its attention a head at a time, rewritten
    into the downloaded file once (`numa_infer::rewrite`): 0.5 GB at most
    instead of 1.8, and faster on the processor (0.58 s against 0.94). The
    same embedding to the bit, so the same masks: all 410 of the survey's
    identical (26 September). Not on Linux, whose card it slows by 38 ms.
  - A mask kept on disk from the old answers is not read back.

  What stays wrong: a facade is still not one object to it (a strip of wall
  and window), and where the model's three answers are all a different thing
  from the one meant, no choice among them fixes it — that is a second click.

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

- ✅ **MASK-014**: A mask survives a change of framing. A quarter turn, a
  crop or a straighten threw every mask's pixels away and asked the models
  again about the new frame — and the answer about a turned frame is a
  different shape, so a subject clicked and brushed into shape came back as
  something else; the click model's embedding was not even thrown away with
  it, so its points asked about places in the old orientation. Now the
  framing's change is a map: `image::between_frames` takes one framing's
  fractions to another's, and `Mask::remap` carries the clicks, the strokes, a
  gradient's handles and the raster through it (`carry_masks`, when the crop
  tool closes). A quarter turn is exact and turns them instead (`Mask::turn`).
  What the models were shown is dropped either way (`forget_model_frames`).
  Tested: four turns come back to the start, and a mask carried into a crop of
  the middle half keeps its clicks on the same part of the photograph. Known:
  a crop that changes the aspect scales a stroke's width by one figure for
  both axes.

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
    numbers) · Tool (Look, Brush, Lasso, Click, Linear, Radial) · Show
    (any of Wash, Outline, Points, Matte, and the button says
    which) · Shape (Strength, Feather, Edge; the button reads Feather) ·
    Refine ▾ and ⟳ where the model has something to say (MASK-008) · ⋯
    (Invert, Duplicate, Delete) · Done. **Look** (23 September) puts the tool
    down: the overlay lets the canvas have the pointer, so the photograph can
    be double-clicked to 1:1 and panned with the mask open. Picking the tool in
    hand again puts it down too. The panel runs to the foot of the window beside it.
  - **The Mask tab is gone**; the rail in a mask is Light, Colour, Effects and
    Detail, with one quiet line above it — "These four tabs edit **Person**"
    and the eye. The path ends in the mask's name, in its accent.
  - **Nothing on the photograph**: no badge, no frame; the wash is white at a
    tenth. What said "you are in a mask" was a one-pixel accent border round
    the viewport; MASK-022 took it away (nothing blue round the photograph)
    — the bar under it and the banner say it.
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

- ✅ **MASK-015**: Stepping through a shoot with the Masks tab open reads the
  chips; it does not run the models. What the found-masks model named in a
  photograph — the chips, the animal's name and the grid their hover
  outlines are traced from — is kept in the library's catalog, filed under
  the framing and the model (`masks::Chips`, `Catalog::found`). The model
  runs when a chip is pressed, for the mask's pixels; masks made before come
  from the mask store. And naming the animal no longer runs the subject
  matte on a frame with nobody in it — only the Animal chip is renamed, and
  it exists only when an animal was found. Per step on the processor: 5.1 s,
  51 cpu-s, ~213 J before (12 frames, 9 of them with nobody); a catalog read
  after. See ENGINEERING, "The Masks tab stops running models".
- ✅ **MASK-016**: A photograph clicked on before is clicked on again without
  the encoder. SAM's embedding (256 × 64 × 64, half floats, 2.1 MB) is kept in
  the library's `.numa/previews` under the previews' budget and trimmed with
  them; keyed on the file, its size and time, the framing and the encoder.
  1.0 s and 11 cpu-s become a 41 ms read; the masks agree at IoU 0.9996.
- ✅ **MASK-017**: Models are let go after three idle minutes (half a minute
  when frugal), at once when the machine turns frugal, and their memory goes
  back to the system: 1.7 GB with the five mask models. The next use pays
  the load (50–300 ms; BiRefNet 1.8 s); a click does not, the decoder is
  loaded with the embedding. `numa_infer::release_all` is what the Apple apps
  call on a memory warning.
- ❌ **MASK-018**: EfficientViT at 512 for the chips. Measured on 108 frames:
  6× less processor (0.35 against 2.09 cpu-s), but only 76 offer the same
  chips and the group masks agree at IoU 0.84 median. Not taken; the
  segmenter now takes the size its file was exported at.
- ❌ **MASK-019**: A cheaper click encoder. MobileSAM and EfficientViT-SAM L0
  (Apache-2.0) in SlimSAM's I/O, held against SAM 2.1 Large over 219 clicks
  on 60 frames (50 raw.pixls.us bodies and the switching set): L0 is worse
  on small things and buildings beyond noise; MobileSAM ties on IoU (−0.006)
  but its border lies on the photograph's edges less and it takes the larger
  thing more often. Neither is equal or better, so SlimSAM stays — and with
  it 958 ms, 12 cpu-s and 36 J a photograph on the processor, against
  MobileSAM's 223 ms, 1.6 cpu-s and 11 J. See ENGINEERING, "The click encoder
  on the card, and half precision".
- ✅ **MASK-020**: A click on the card gives the processor's mask. SAM's
  relative-position indices were worked out on the card, whose division is
  not exact, and 13 became 12: over 219 clicks the card's masks agreed with
  the processor's at IoU 0.73, and were worse against SAM 2.1 Large. Card
  sessions now fix the batch at one, so that chain is folded on the
  processor: all 219 at IoU ≥ 0.999, and an encode costs the card half the
  energy (28 → 14.5 J) and half the memory (+11 → +5 GB). Embeddings and
  click masks kept from before are made again.
- ✅ **MASK-021**: Refine edge on the card gives the processor's hair.
  ViTMatte works out its relative positions from its tile's height and
  width, and on the card, whose division is not exact, some came out one
  short: its mattes were 0.001–0.004 of alpha from the processor's on
  average and up to 0.997 in places. The card's session is now built for the
  tile's size, and built again for a frame under 1024 on a side: 6e-8 on
  average, 6e-5 at most. The processor's mattes do not move.
- ✅ **MASK-022**: The mask bar, fewer parts and in sight (UX study, 30
  September; reworked after the photographer's test of 0.34.0: "de inhoud
  was net beter alleen moet het wel duidelijker zijn wat er gebeurt op je
  scherm"). Entering a mask, the bar slides down at the top of the
  photograph's column, under the header where the eye already is, and the
  filmstrip goes; leaving, it goes back. Left, the chip — the mask's
  picture, "EDITING MASK" over its name (in the chip so it never has to go
  for room), no "1 of 1 · …". Its popover holds the mask: MASKS with New
  Mask, THIS MASK's parts, Strength, Feather and Edge, and Invert,
  Duplicate, Delete Mask. Then the tools as one group — Look, Brush, Lasso,
  Click, Linear, Radial — with the tool's own controls after them while
  Brush or Lasso (Add | Subtract for Click too) is in hand. Right, Refine
  as a split button (Edge, Hair, fur and feathers, Search Again), what is
  shown in one place — the wash's eye with Outline, Points and Matte in its
  arrow — and Done in white, at the top right. Round the viewport a white
  edge while a mask is edited: the photographer's border of 21 September, in
  Numa's accent since the design system made that white. Nothing is cut off
  and a 1 280 window keeps one line: under 1 160 px of bar the tools and Add
  | Subtract are set tighter, under 1 000 the row of tools is one menu with
  the one in hand as its label, and under 700 the tools and their controls
  take a second line. Menus open downwards.
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

  Moments the same since 7 October: its tiles start empty and a sweep gives
  pixels to the tiles within a screen of the view and takes them back past
  three. Every tile of a 2346-photograph shoot had been given a texture at
  the grid's size as Moments opened, and the window's thread was busy for
  10.7 s as they came in (`numa-scratch/kiezen/rig`, Xvfb and cairo); after,
  0.8 s, then idle (the photographer: "het lagged nu nog een beetje als je
  het opent").

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

  **23–24 September: every path drafts, and the sharp frame waits for the
  hand.** Measured on the photographer's machine with `NUMA_TIMING=1`, which
  prints each render's size, cost and path, and each pass of the stack:

  - Zoomed in, the draft and the finished render were the same six megapixels
    — the halving only ever reached the proxy — so a drag at 1:1 cost 200 ms a
    frame. The edge a tile is cut at is halved while drafting, and the draft
    keeps a tile of its own (`draft_view`), so it is cut out of the full frame
    once per zoom rather than at every tick: 200 ms became 52.
  - A frame that can only be rendered whole — HDR, Clarity, Texture or Dehaze,
    until PERF-018 — drafted at thirty-eight megapixels, 560 ms a frame. While
    the hand moves, a whole-frame region now comes from the proxy.
  - The 130 ms of quiet was the real stall: on a careful drag the full-size
    render started at every pause, a second and a half on the main thread,
    which hid every gain in the draft. The sharp frame now waits for the mouse
    button to come up. Watched on the window's raw events
    (`watch_the_button`): a gesture on a slider never hears its release,
    because the scale claims the sequence, and the pointer's own modifier
    state reads nought — both were tried, and the first left the photograph on
    the draft for good.

  At fit, afterwards: 5–8 ms a draft and 30 ms sharp, flat however many masks
  are added.
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

  **One device (23 September).** The session was handed every WebGPU device
  the environment listed, and on a machine with two — the RX 9070 XT and the
  processor's own Radeon — the factory refused with "currently only supports
  one device at a time", and every model ran on the processor, quietly, with a
  warning in the log. It is handed one now: a card before a software renderer.
  Segmentation on the card: 51 ms, against 250 on the processor. It was also
  most of the memory: sessions on the processor held about 5 GB of arenas
  after a few masks; on the card the editor sat at 0.7–1.8 GB.

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

- ✅ **PERF-014**: The blur stops building a second plane to copy it: strips
  of columns go out a batch at a time, so a 40 MP export peaks at 1.30 GB
  rather than 1.41, and the renditions are unchanged. (Added to the index 28
  September; the code has used the ID since 0.14.)

- ✅ **PERF-015**: A mask's edge is drafted while the hand moves. Feather and
  Edge reshape a half-size copy of the mask on every tick of the slider and
  the full raster when it settles, as PERF-006 does for the picture — 3 ms a
  tick instead of 11, which is what let the mask raster go to 4096. (Added
  to the index 28 September; in the code since 0.18.)

- ✅ **PERF-016**: A mask costs what it covers. Every mask cloned the whole
  buffer, ran its detail passes over all of it and blended all of it back, and
  worked its coverage out for every pixel — for a bird a twentieth of the
  frame. On the photographer's log the same render went from 190 ms to 290 as
  masks were added. A raster now keeps the box it covers from when it is built,
  an ellipse is its centre and radius, and both the coverage and the passes run
  over the rows the mask reaches, with a margin of 64 for the blurs. A mask
  covering nothing costs nothing. A gradient and an inverted mask reach
  everywhere and are left as they were; so is a mask with HDR, Clarity, Texture
  or Dehaze, which measure the whole of what they are given — handed a strip
  they came out differently, which the first version of this did from 0.19.10
  to 0.19.20 (`rows_to_work`, tested directly).

- ✅ **PERF-017**: A turned, mirrored or straightened frame at 1:1 renders the
  part on screen. The region is a rectangle in the frame and a rotated one in
  the photograph, so `tile_in_source` said "not a rectangle" and the whole
  frame was rendered at full size. `cut_turned_tile` cuts the smallest upright
  box around the region out of the working image, turns only that, and lets
  `cropped` do the straighten angle over it. Tested against cutting the region
  out of the whole frame after `geometry_of` for all sixteen combinations of
  quarter turn, mirror and angle — within 1e-4 — and the test fails with the
  turn or the angle's sign wrong. At 1:1 on a turned photograph the sharp frame
  went from about 1.4 s to 185 ms.

- ✅ **PERF-018**: HDR, Clarity and Texture at 1:1 without the whole frame.
  Their base layer, their glow and the average they compress about are the
  frame's, so a tile tone-mapped on its own came out differently — 17 levels
  off at worst, 9 on average — and a frame with any of them on was always
  rendered whole. The tone map now hands its measurement over and takes one
  in (`local::Tone`, `measure_tone`, `apply_pixels_guided`):

  - while a slider moves, measured on the draft: milliseconds, and a little
    off along a hard edge, where nobody judges a halo;
  - once it stops, measured on the full-resolution frame without the passes
    its quarter-size base cannot see — sharpening, noise reduction, spots —
    and nothing after the tone map; kept, so a pan measures nothing.

  Measured on a scene built for it: the full-resolution measurement is within
  0.05 of a level on average of the whole-frame render, and the proxy's was up
  to 26 levels off along a hard edge, which is why the proxy only ever serves a
  draft. In the editor, at 1:1 on the kite with Clarity at −45: 1058 ms became
  541, and the screenshots differ in one pixel of 1.3 million by more than 1 %
  (`NUMA_WHOLE_FRAME=1` renders the old way, to compare). Dehaze, and a mask
  carrying any of the four, still render the whole frame.

- ✅ **PERF-019**: A render off the main thread, the latest change winning.
  A render is planned on GTK's thread, its pixels made on a worker and put up
  when done; a change while one runs asks for one more, so nothing queues and
  the window takes input throughout. With it, each slider's track has a
  stylesheet of its own rather than one on the display, which restyled every
  widget in the window per tick: 156 ms frames with 3 300 photographs in the
  filmstrip became 16.6. (Added to the index 28 September.)

- ✅ **PERF-020**: Switching photographs. The photograph after this one, in
  the direction the photographer is stepping, is decoded as soon as this one
  is on screen, so a step is the colour stage and one render: 37–60 ms to the
  sharp frame, where it was 0.45 s for a 24 MP frame and 1.3 s for a 50 MP
  one. A jump to a photograph that was not decoded ahead shows its cached
  thumbnail within 13 ms and its sharp frame 25–30 % sooner than before —
  the decode lost rawler's copies of the frame, decodes the camera's JPEG
  beside the raw, and the first frame is no longer a draft followed 140 ms
  later by the real one. Same pixels, hash for hash. `NUMA_TIMING=1` prints
  every stage of an opening with the cores it used (`docs/ENGINEERING.md`,
  "Switching photographs, measured").

- ✅ **PERF-021**: The raw decoders read faster on one thread. rawler 0.8.0
  is carried in the tree (`vendor/rawler`, LGPL-2.1, changes listed in its
  `NUMA-CHANGES.md`); its bit reader refills eight bytes at a time and its
  Huffman cache is a third of the size. One thread: a 50 MP CR2 240 → 177 ms,
  a 45 MP NEF 168 → 106 ms. Mosaics identical on 502 raws of every make.

- ✅ **PERF-022**: A CR2's and a NEF's stream is decoded on four threads,
  though neither has restart markers — each part is read from its own byte
  and the joins found where Huffman codes resynchronise, so the result is the
  single reader's exactly. A CR2 goes straight into its fields. 5DS 240 → 53
  ms, Z 7 168 → 48 ms, at no more processor time.

- ✅ **PERF-023**: The camera-profile scan reads each `.dcp`'s header rather
  than all 160 MB of them: the first photograph of a body 105 → 2 ms.

- ✅ **PERF-024**: A power switch (`numa_core::power::frugal`), set on Linux
  by the power-saver profile or the battery, and by the Apple apps through
  `set_frugal`. Frugal, nothing is decoded ahead until two steps the same
  way, and background work runs on a quarter of the cores. The decode ahead
  always runs on its own low-priority pool and stops when it is let go; the
  flag stays with that decode, not with the thread rayon runs other jobs on
  while it waits.

- ✅ **PERF-025**: An opening's colour stage is made off the main thread, and
  by the decode ahead with the neighbour's stored edits.

- ✅ **PERF-026**: The lens database is read at startup, off the main thread,
  unless frugal: the first raw opened no longer waits 25 ms for it.

- ✅ **PERF-027**: ORF's predictor and the PPG demosaic choose with selects
  instead of mispredicted branches: ORF 133 → 104 ms, the Bayer demosaic
  15 % less processor time. Output identical.

- ✅ **PERF-028**: The lens corrections find each radius's interpolation once
  for all three tables and once for a pixel and its mirror: geometry 20 %
  faster, falloff 40 %. Output identical.

- ✅ **PERF-029**: Markesteijn in 128-pixel tiles (1.5 times the pixels it
  keeps rather than 2.6) with its green bounds on all threads: the X-T5's
  demosaic 634 → 426 ms. Identical on 70 RAFs.

- ✅ **PERF-030**: The editor's proxy is developed at the size it is shown.
  An opening developed every photosite of a 24–50 MP frame — demosaic,
  falloff, geometry, false colour, exposure match — and then averaged twelve
  to twenty of them into each pixel of the proxy and threw the rest away.
  `raw::proxy_from_mosaic` averages the mosaic straight into the proxy instead:
  per pixel and colour, the photosites under the downscale's own footprint,
  read from where the lens profile says that colour landed, spread by the
  tent a demosaic would have spread them by. The editor's proxy wherever no
  card is ready or Numa is frugal (elsewhere the card's develop, RENDER-010),
  a grid preview of a raw with no camera JPEG and an edited photograph's
  thumbnail go this way; 1:1, the export and the loupe decode in full as
  before, hash for hash. On ten bodies the stages after rawler's decoder fell from
  270–1 000 ms to 70–100, and the processor time of a whole opening by
  three to five times. The fit view is the old one within a level on average
  (`docs/ENGINEERING.md`, "Developing at the size that is shown"). Where a
  wide lens's profile reads past the frame's side the box holds the edge:
  three raw.pixls.us frames panicked there until 29 September.

- ✅ **PERF-031**: Going back to a photograph needs no decode. The last
  three opened are kept decoded — the open photograph's proxy and two more,
  60–90 MB — keyed on the file, the proxy's size and what Automatic means,
  and a finished decode ahead that a change of direction left unused is kept
  behind them. Stepping back and forth between two frames of a burst went
  from 440–710 ms a step to 38–42, and the decode ahead no longer decodes a
  neighbour that is already kept: on A7R III ⇄ 5DS, four decodes where there
  were seven, 6.3 processor-seconds where there were 45. Nothing speculative
  runs on a frugal device before two steps the same way (PERF-024's rule;
  the branch's own hook went at the merge).

- ✅ **PERF-032**: Going back in to 1:1 needs no decode. Zooming out to fit
  drops what was rendered from the original at once and the decoded original
  thirty seconds later, so a second look at focus somewhere else is the
  colour stage and a tile rather than the whole original decoded again: on a
  5DS frame, 0.66 s and 6 processor-seconds saved per look, for 576 MB held
  half a minute longer.

- ✅ **PERF-033**: 1:1 develops what is on screen. A zoom past the proxy
  decoded and developed the whole original — falloff, geometry, false colour,
  exposure match and the colour stage over 24–50 MP — to show two or three.
  The raw is now opened and demosaicked once (`raw::region::Regions`), and the
  tile on screen is developed from it, float for float what the full develop
  gives there: the lens corrections through the same per-pixel arithmetic, and
  the exposure match's median from the same one-in-37 pixels, each developed
  on its own. A pan to somewhere not developed yet is a new part, 25–70 ms;
  anything that needs the whole frame (HDR, Clarity, Texture, Dehaze, AI
  denoise, a spot whose source is elsewhere) still gets all of it. The 1:1
  view is the same pixels as before, screenshot for screenshot; the first
  zoom-in is 0.48 s instead of 0.8 on a 5DS and 0.23 instead of 0.6 on an
  A7R III, and the full-resolution frame held is 19–38 MB instead of 576.
  Where a card is ready (RENDER-010) it develops the whole original instead,
  and the part on screen is cut from that for the colour stage.

- ✅ **PERF-034**: A photograph opened before opens from its developed
  preview. The editor's proxy is kept in the library's own
  `.numa/previews` folder — on the drive with the photographs, as Lightroom
  keeps its previews beside its catalogue, with a `CACHEDIR.TAG` for backup
  tools — as half floats, 14 MB each, keyed on the file, the proxy's size,
  what Automatic means and Numa's version. Read back in 10–19 ms and 7–15
  processor-milliseconds where a decode is 190–440 ms and 1–2.3
  processor-seconds; written on a thread of its own, never on the way to the
  screen. Least recently used goes first. Nothing outside a library, on a
  drive that is away or read-only, or on one the first read of a session
  finds slower than decoding. How much room each library may give it is a
  slider in Preferences › Storage (Off to 20 GB, 2 by default).

- ✅ **PERF-035**: The proxy's brightness is the export's. The exposure
  match lifts a frame's median onto the camera JPEG's; a proxy made from the
  mosaic (PERF-030) took that median over its own smoother pixels, and a
  dark, noisy frame came out up to 1.2 % darker than the export (a Z 7). It is
  now taken over the pixels the full develop reads — every 37th of the whole
  frame, after the falloff and the geometry — each demosaicked on its own from
  the mosaic: the gap is 0.36 % at most and under 0.1 % on eight of ten
  bodies, for 5–15 ms.

- ✅ **PERF-050**: An editor opened before the grid was ever on screen (Open
  With, `numa file.RAF`) no longer keeps the machine awake. The thumbnail
  sweep asked again every 60 ms, through a tick callback, for a grid that was
  not laid out — and a tick comes to any realized widget, mapped or not — so
  the frame clock ran for as long as the editor stayed open (EXPLORE_DATA F5,
  92 wake-ups a second). A list is asked again only while it is mapped, and
  swept when it is.

- ✅ **PERF-051**: The grid and the filmstrip make widgets only for the cards
  on screen, with each photograph's shape kept in the catalog so a grid of
  fifty thousand is laid out without opening a file. Fifty thousand shown
  0.78 s after start, scrolling at a 6.9 ms median frame with next to nothing
  on the main thread; tried by the photographer on 29 September ("dat werkt
  echt super snel"). See ENGINEERING, 28 September.
- ✅ **PERF-060**: What is handed over is compiled as one unit. The Flatpak,
  the AppImage and the public copy's build are `cargo build --profile dist`
  (fat LTO, one codegen unit): a slider tick 6 % less processor time and 8 %
  less energy, a jump 2 %, an export 3 %, every output the same byte for
  byte. Development stays on `release`, which builds in a third of the time.
- ✅ **PERF-061**: A Fuji frame decodes 9 % faster: its per-sample code is
  inlined instead of chosen at run time for every sample.
- ✅ **PERF-062**: Lossless-JPEG and NEF codes whose difference is long no
  longer take the slow path: 20–34 % less processor time on the frames that
  have many of them (older Canons, D3-era Nikons, Leica DNGs), 2 % over all.
- ✅ **PERF-063**: An uncompressed raw is unpacked in two runs instead of a
  task per row: the same speed for a sixth to a seventh of the processor time
  (a 42 MP ARW 78 → 12 processor-ms).
- ✅ **PERF-064**: On the iPhone, iPad and Mac the core's threads say how
  much they are waited for: the photograph on screen at user-initiated, the
  decode ahead at background, which keeps it on the efficiency cores. The
  Apple app calls `startThreads()` first, in `NumaApp`'s `init`, since 0.29.0
  (Numa-mac `7ba3fd2`, TestFlight 41). Measured on the MacBook's bench
  (`docs/MAC_BENCH.md`, run 3: background 3.5–4× less energy), not yet in
  the app on a device.
- 🟡 **PERF-065**: The editor's proxy averaged from the mosaic, 6 % faster
  and the same to the bit. The two-pass separable warp proposed for it was
  measured against the stage's real cost and left as a proposal: the time
  goes to each box's setup, not to photosites read twice.
- ✅ **PERF-066**: A photograph opened while a mask model is on the card
  waits for the card, up to a second and a half, instead of decoding on the
  processor: an X-T5 costs 0.8 processor-seconds there against about 12. The
  photographer saw the processor busy while masks were found (29 September).
- ✅ **PERF-040**: Frames go to GTK as four bytes a pixel. GTK's Vulkan
  renderer, its default, has no three-byte texture and converted every frame
  on the processor (13 ms of CPU for 1920 × 1280); now `ui::display::texture`
  hands it B8G8R8A8, with the display profile applied in the same pass —
  canvas, thumbnails, loupe, reference, previews.
- ✅ **PERF-041**: The eight-bit encode is read off a table over the float's
  bits, exactly: 1 024 buckets an octave, and a value whose bucket holds a
  step is worked out as before. Byte-identical (test, and 50 frames of ten
  bodies); a render's last step 7 → 1.5 ms.
- ✅ **PERF-042**: Leaving an edited photograph hands its card the frame that
  is on screen (or the one behind the 1:1 tile) instead of rendering the
  proxy again on the main thread; the shrink and the JPEG go to a worker. And
  `tone::curve(+inf)` is white instead of a panic.
- ✅ **PERF-043**: The render's worker starts in the frame's tick, not after
  GTK has laid out and painted it (−4 ms a step).
- ✅ **PERF-044**: The next photograph's first frame is rendered ahead with
  its decode (not when frugal, not with the card on, not with a mask; stopped
  with it) and put up at once when it is still that picture: a step's first
  frame 31–39 → 12–13 ms after the key.
- ✅ **PERF-045**: A colour drag at 1:1 colours only the view, cut once from
  the original before its colour stage, while the hand is down; the tile's
  develop follows when it stops (Temperature at 1:1: 14 → 2.4 cores, 43 → 118
  of 120 frames).
- ✅ **PERF-046**: At 1:1, the backdrop and the histogram come off the draft
  while a slider moves, as at fit — a quarter of the proxy's edge during a
  colour drag (Exposure at 1:1: 4.3 → 1.7 cores).
- ✅ **PERF-047**: The display profile's pixel without `roundf` and a
  float-to-`usize` (3.0 → 2.0 ms a frame, byte-identical). Not folded into
  the encode: its cost was the arithmetic, not the pass.
- ✅ **PERF-048**: The stack's stages kept by the frame's `Arc` instead of a
  30 MB hash, and a second point in front of the grade for late sliders
  (`apply_stack_kept`): a tick 4.4 → 3.2 ms, a late slider on an edited frame
  24.5 → 5.5, byte-identical.
- 🟡 **PERF-049**: Measured: AVX2 without FMA is worth 0–5 % on this render
  (hashes identical), so no clones; the DCP hue lookup without `fmodf` and
  `floorf`, and the histogram in two sets of bins (0.9 → 0.6 ms), exact. f16
  not used on the processor: every intermediate feeds a 1:1 tile or an export.
- ✅ **PERF-067**: A colour drag's draft is coloured from a half proxy made
  once and kept, not reduced from the proxy on every tick.
- ✅ **PERF-068**: A kept frame's render works in the last one's buffer: at
  2400 px a frame is over glibc's reuse ceiling, and each tick faulted in a
  fresh 46 MB (tick 6.2 → 5.1 ms there).
- ❌ **PERF-069**: Half precision for the models on the card. The WebGPU
  plugin runs a float16 file in float16 (RADV has `shader-f16`), but only
  what is indistinguishable from float32 is taken: EfficientViT overflows,
  SAM's clicks move on a fifth of them (IoU 0.979), so both stay float32.
  SCUNet (float16 since DETAIL-003) passes. BiRefNet's float16 differs
  visibly from float32 on some frames; it is float32 now (PERF-071).
- ✅ **PERF-070**: A mask's curves cost the processor a fifth of what they
  did. The way back from the curve to scene light halved 48 times over the
  whole curve for every value: 2.15 CPU-s and about 12 J a draft frame. The
  table's closed form says where the answer is, and a few halvings there find
  the same float — every one of the 112 million floats it can be handed
  checked against the old answer, bit for bit: 0.43 CPU-s and about 2 J.
- ✅ **PERF-071**: BiRefNet in float32 on the card, its deformable
  convolutions worked out a kernel row at a time: the subject is the
  processor's (IoU 1.0000 on 75 frames, where float16's was off on 42, down
  to 0.26), 566 ms and +8.1 GB of the card against float16's 415 ms and
  +8.2 GB (float32 as exported: 887 ms, +12.3 GB). The download is
  `birefnet_f32.onnx`, 973 MB; the float16 file answers until it is fetched.
- ✅ **PERF-072**: A quiet lane for work nobody asked for (`power::quietly`):
  the photographs read for search by words, one job at a time across Numa,
  on two threads at nice 19, each job followed by a rest twice its length;
  the words model is loaded on two threads for it. The photographer, 7
  October: on a library opened for the first time words and Numa's work
  ahead in Moments started at once, words at every core and the work ahead
  at full priority with the card, "wat meteen weer mijn computer laat
  loeien"; words were to go "op een laag pitje". The work ahead is no longer
  unasked (FLOW-018, Let Numa Do…). It was on the lane for a day, where it
  seemed to come back with no light; measured again on 7 October
  (`numa-scratch/kiezen/rig-pool/probe.sh`), the lane, `power::background`
  and a plain thread give the same proxy, frame and light bit for bit on all
  22 Garden moments — the first four have none, and the slower lanes had not
  reached the rest. Asked-for work runs on neither because it is waited on.
  Linux; the lane is in the shared core.

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
  moved nine sliders between tabs and P2 will move more. What is missing:
  the page shows Grade one range at a time, All by default, so a grade on
  Shadow, Mid or High while another range is chosen leaves Grade without its
  dot (`refresh_rail_dots` reads only the sliders on the page; seen in the
  inventory's screenshots 10 and 17).

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
  30 September: bound to Ctrl+? as well, as GNOME's own applications have it,
  and the rows that were missing added — C to compare, Ctrl+O to open a
  photograph, Ctrl+A and Ctrl+Shift+A for the selection, F5 to rescan (LIB-023),
  Esc to leave a mask. Wide enough that every row is one line.
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
- ✅ **UX-015**: One loader for everything that waits. Auto joined them on 24
  September: its models and measurement were 0.9 s on the main thread. Eight things run off the
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

  28 September (UX-025): the toast is now the loader for waits that belong to
  no place, and it stays up at least 200 ms once it is up. A wait that has a
  place shows there instead, by the same 400 ms rule.
- ✅ **UX-022**: Waveform and RGB parade, in the histogram's place, chosen
  with a right click on it (a tap on the iPad) — Histogram, Waveform or
  Parade — and remembered. A histogram says
  how much of the photograph is at each level; a waveform keeps where: across
  is the frame's own columns, up is 0–100 IRE, brightness is how many pixels
  of that column sit there. The parade is the same per channel, red, green and
  blue side by side, which is where a cast shows. Worked out in
  `render::scope` off the same whole frame as the histogram, on the render
  worker, and handed to every client as a small picture to stretch (360×128,
  every other row sampled). Linux under the histogram, the iPad's panel and
  the iPhone's Light as chips under theirs; Modern has no histogram, so none.
- ✅ **UX-023**: The editor, quieter, and a Looks tab. The photographer, 27
  September, over a screenshot of Colour: "er gebeurt teveel". One bar instead
  of two: the header carries the crumbs, Undo, Redo, Before, Export and the
  main menu, and the fifteen-control row under it is gone. What it held is the
  photograph's section at the top of that menu, each with its key beside it —
  Copy Settings, Rating (which says the rating), Zoom, Guides, Keep as
  Reference, Compare with Camera, History, Photo Info — so the menu teaches
  the keys the row of icons never did. The zoom is a badge at the top centre
  of the photograph, shown while it says something (zoomed, soft, still
  loading), not at Fit.

  The Presets tab is **Looks**, the iPhone's word, on every client: the
  camera profile on top and always in sight, then one choice, **Presets |
  LUTs**, which always opens on Presets — "als luts populairder blijkt ruilen
  we die om". How the raw becomes a picture and a look over it are the two
  questions before any slider moves, and the photographer who never opens
  another tab now meets both. The profile is still not a look (film
  simulations stay out of its list) and the LUT is still applied last in the
  pipeline; they only sit together. Colour opens on White balance with its
  pipette as an icon at the end of the label, like the mixer's; the profile's
  source and folder are its tooltip. Effects ends at Grain. Two hairlines in
  the rail group the nine: Looks, the adjustments, the tools. On the iPhone
  the profile is on top of Looks too, shown only for a photograph that did
  not come from an iPhone, and there are no LUTs there.
- ✅ **UX-024**: The rating, the history and the camera's facts back in
  sight. The photographer, 28 September, over the one bar: "misschien wel te"
  quiet — rating, history and photo info are "essentiële zaken om snel in te
  kunnen zien". The left of the bar, which held only Back, has the History
  popover and then the rating as the loupe gives it (RatingAndFlags): five
  Adwaita stars, Pick and Reject, each pressed again to take it back. Hovering
  a star fills the ones before it; set stars are white, a rejected frame's
  stars and its cross the reject's red. The
  camera's facts are an info icon behind the photograph's name in the path;
  `I` still opens them. Keep as Reference and Compare with Camera left the
  menu for an arrow linked to Before: all three are comparisons, and one
  control for comparing reads like Export and its arrow. The menu's own
  section is now Copy Settings, Zoom and Guides.
- ✅ **UX-025**: (UX-024 on its branch, loaders-night.) Numa says it is busy
  where it is busy. The photographer, 28
  September: "Ik denk dat ondanks het allemaal sneller kan we voor de ux toch
  overal loaders toe moeten voegen zodat de gebruiker ziet dat Numa bezig is
  en even wacht." A wait with a place shows there, as a small spinner, by
  UX-015's rules — nothing for 400 ms, and once up, up for at least 200: in
  the zoom badge over the photograph while it opens, while the original
  decodes for 1:1 and while a mask's pixels are found; in the Masks tab's
  "Looking at the photograph…"; in the Auto button, in the word's place; by
  the loupe's "developing at full size…"; and by "Reading the card…". What
  has a total says how far it has got with a bar: an export (with Stop), an
  import, the thumbnails. The toast stays for what has no place.

  Freezes went with it: the Looks cards, drawn on the main thread (1.4 s on
  two cores), now drawn on a worker one at a time; Auto's frame, made on the
  main thread before it started (1.7 s on two cores with a decode running),
  now made on its worker; a library's walk when a folder is added, a file
  opened from outside or files dropped in (3.9 s for a new folder of 2 001
  on sixteen cores), Move to Trash and the walk after an import, which on a
  share or a slow drive were as long as the drive — all beside the window
  now. A spinner turns only while it is on screen and is
  labelled for assistive technology, and the thumbnail count is told by the
  queue rather than asking it ten times a second. The inventory, the
  numbers and what is still on the main thread are in `ENGINEERING.md`
  ("Saying Numa is busy").
- ✅ **UX-026**: Quiet rows. A card in the grid is only the photograph: the
  lines under it (the cull note, the file name, five stars) are gone, so a
  row is as tall as its pictures and more of them fit. What has been set is
  said on the picture in small dark pills — "★ 3" and the pick flag at the
  foot on the left in white, a reject as a dimmed frame with a cross in the
  reject colour, a white dot on the right for an edited frame, the drive icon
  for one that is offline, and "×N" at the top on the best frame of a burst.
  Analyse's suggestion shows only on a frame nobody has rated or flagged, as
  a dimmer hollow star ("☆ 3", and "soft"). With the pointer on a card a
  gradient at its foot gives the stars where the pill was and the file name
  on the right. The tooltip is the name in bold, what stands out in a word
  or two (Sharp or Soft, Blown highlights, Deep shadows, Almost empty,
  faces, Eyes closed?, Slow for the lens) and how it was taken with
  Analyse's suggestion — not every measure with its threshold, which read
  as a report. Selected is a ring in Numa's accent. From the start, and back
  from the editor or the Libraries page, the keyboard is the grid's, so Space
  opens the loupe rather than pressing Add Folder.
- ✅ **UX-027**: The editor's bar, grouped by meaning (UX study, 30
  September, drawing "Editor · de balk", A). The verdict on the left — Back,
  the five stars, Pick and Reject; which photograph in the middle — the path
  and its ⓘ; what is done to it on the right — Undo, Redo and History as one
  linked group (History is the list Undo and Redo step through, so it moved
  from the left to beside them), Before with its arrow, Export, the menu.
- ✅ **UX-030**: White is Numa's accent inside its own content on Linux, as
  the design system's second signature says. A moved slider's fill and dot,
  the selected rail tab (white, its icon and name dark), the rail's dots, the
  chosen chip in the panel (RGB | R | G | B, Presets | LUTs, the grade's
  ranges, the crop's aspects, a mask's ranges), a pipette that is down, held
  Before and the filmstrip's current frame are the foreground ink — white on
  dark, near-black on the light scheme. The mask being edited, the step on
  screen and a picked point colour lose their blue tint for a neutral one.
  Adwaita's chrome and its accent are untouched; Export went white with
  UX-032.
- ✅ **UX-031**: One design system in the Linux code (UI cohesion, 2
  October). The design system's tokens.json is in the repository
  (`data/design/tokens.json`) and `dev/tokens.py` turns its colours into GTK
  named colours (`src/ui/tokens.css`, `@numa_*`), loaded ahead of the
  stylesheet with the light scheme's values on top while the scheme is light.
  The parts more than one place builds live in one module (`kit.rs`).
  `dev/style.py`, run by `dev/check.sh`, is a ratchet like the shape one:
  colour literals outside the tokens, stray `suggested-action`, stock
  sliders outside `slider.rs`, CSS sizes off the scale, cairo "Sans", and
  spinners or progress bars made outside `busy.rs` (whose three forms —
  loader toast, progress toast, `Waiting` in place — are the only waits)
  may only become fewer. The slim tracks' corners are written as a capsule rather
  than 1 px (the same on a 2 px track), and the notes, the mask's parts and
  the loupe's and reference's captions are set in px on the type scale
  rather than in em (12 or 13 px, within half a pixel of what they were at
  the default font).
- ✅ **UX-032**: One primary command per screen, and it is white — Numa's
  accent, near-black on the light scheme — in the header bar as well: the
  Export pair in the grid and the editor, Export in the export dialog, Add
  Folder… on the first page, Import, Download, Save and Paste in the settings
  dialogs, and Done on the mask bar and the crop. The system's blue stays
  only on what the platform draws itself (an alert's response, the file
  chooser). AI Denoise and AI Sharpen on the Detail tab are plain buttons:
  a command in the panel is not the screen's primary.
- ✅ **UX-033**: Chosen is the accent, everywhere in Numa's own content. A
  choice among a few is a row of chips, the chosen one white with dark on it
  (near-black with white on the light scheme): the curve's channels, Presets
  | LUTs, the grade's ranges, the crop's aspects, retouch's tools, the
  Filter popover's rows, and on the mask bar the tools and Add | Subtract,
  which lose their grey track. Inside the editor, the grid, the loupe,
  compare and Numa's popovers the accent Adwaita draws with (a check, a
  switch, a focus ring) is Numa's too; the chrome keeps the system's. On
  GTK before 4.16 (Ubuntu 24.04's AppImage and OBS builds), which ignores
  the accent properties, switches, checks and radio buttons there are set
  white by name all the same; only the focus ring stays the system's. The loupe's AF point and the picked frame are white,
  not the system blue; the grid's selection ring, set stars in the bar, the
  rail and Before take the same accent.
- ✅ **UX-034**: The mask bar's Size and Softness are Numa's slider, as on
  the panel: a hairline filled from where each rests (the default brush, a
  half-soft edge) rather than from the left end, the dot dim until moved and
  then in the accent, the name and number coming up to full strength, and a
  double or right click to put it back. The panel's rest tick follows the
  light scheme now (it was white on white there).
- ✅ **UX-035**: One ground on the photograph, in three forms: #141414 at 70 %
  (GTK draws no blur, so the ground does the reading alone), dark in both
  schemes. The histogram sits on a card (14 px corners), the zoom and a
  face's name on a pill (12 px, semibold, the system face — the names were
  square black boxes in cairo's "Sans"), and the Straighten value HUD keeps
  its own, as the design system draws it.
- ✅ **UX-036**: One tile grammar. A filmstrip frame carries the marks its
  grid card does, drawn the same way: the rating and pick as a dark pill at
  the foot on the left ("★ 4  ⚑", where it said "4★" in small type at the
  top right), the edited mark as the card's white dot at the foot on the
  right (it was a pencil with no ground), and a reject dimmed with the cross
  in the reject red. One reject red: #ff7a66 on the photograph in both
  schemes, the design system's light value in the bar on the light scheme.
- ✅ **UX-037**: One small heading. The mask chip's popover headings (MASKS,
  THIS MASK, SHOW) and the bar's EDITING MASK are the section label every
  other popover and the panel use — 10 px, semibold, spaced, in the design
  system's grey — where they were 11 px bold and 9 px bold white; the
  Libraries page's shelves are headed by it too, instead of a large title.
  The section label's grey is the design system's `label` token, which
  follows the light scheme.
- ✅ **UX-038**: Stacks of prints as the design system draws them, on the
  Libraries page and in the picker: three prints turned −8°, 6° and 0°,
  small corners, the back two darkened to 0.8 and 0.9, each lifted by a
  shadow — no longer Adwaita cards with a rim and 12 px corners. Each album
  in the Albums dialog leads with its 36 px stack, as in the picker. The eclipse
  mark heads the Libraries page (the app icon on the light scheme, where the
  mark's light disc would vanish); never in the editor.
- ✅ **UX-039**: The filter's quick chips. The bar under the library's
  header is always there, led by three chips — All, Picks, ★ 3+ — the
  narrowings a shoot is gone through with, one press each and the chosen
  ones white. What else narrows (from the Filter popover) follows them as
  removable chips, with how many are left and Clear while anything narrows
  (LIB-023).
- ✅ **UX-040**: Copy and icons, one way each. Commands, menu items and
  titles are in Title Case — the presets menu said "Import presets…" in one
  place and "Import Presets…" in another; "Ask Again", "Find Dust", "Remove
  People", "Show Affected Area", "Download All", "Got It", "Fit to Window",
  "Pet Eye"; empty pages "No Albums Yet", "No Presets Yet", "No Folders
  Yet", "No Faces Yet" — and explanations stay in sentence case. The two
  dialogs both called "Save as preset" are "Save Settings as Preset" and
  "Save Export Preset". An icon means one thing: Clear Selection no longer
  shares Reject's cross (a clear-all icon), a mask's Invert is its own icon
  (a square with a hole) rather than Flip's, and the arrow beside Export,
  which opens its settings, is a gear rather than the crumbs' arrow.
- ✅ **UX-041**: Motion, one way. Opening and closing a photograph, and
  going to the Libraries page, cross-fade over 300 ms (with the header)
  instead of cutting. The mask bar slides in over 200 ms. A slider's
  double-click or right-click reset glides back over about 200 ms, easing
  out, on the panel and the mask bar alike; the filmstrip follows the
  photograph the same way, and the panel scrolls to a picked point colour
  so too. That point colour's flash comes on at once and fades over 700 ms
  without the 900 ms hold. Nothing bounces, and with animations turned off
  in the desktop's settings each lands at once.
- ✅ **UX-042**: The floating value, for every slider. A value changed away
  from the drag on its row — an arrow, Page or Home/End key on a focused
  slider (Shift for ten steps), a touchpad's sideways swipe over it (the
  wheel scrolls the panel instead), on the panel and on the mask bar's Size
  and Softness, where the wheel moves them too — shows its name
  and number at the top centre of the photograph on the HUD's dark pill,
  at once, and leaves 0.9 s after the last change. It was Straighten's
  alone (which keeps it while the angle moves at all, the level line drawn
  on the photograph included). A drag on the row does not call it — the
  row already says it — nor do undo, redo, a preset or opening a
  photograph.
- ✅ **UX-028**: The histogram on the photograph. It left the top of the
  panel for a small graph (about 200×70) at the photograph's top right, 14 px
  in, on the photo-side card (UX-035) — beside the
  panel and its sliders (at the top left it hid, the photographer said on
  testing 0.34.0), where the eyes are while a slider moves, and the panel's first line is the tabs, or
  the mask's banner while a mask is edited. Shown only while Light or Colour
  is open; Crop, Retouch and the rest keep the photograph clear. The clipping
  lights are its two top corners; a right click still chooses Histogram,
  Waveform or Parade (UX-022); Histogram, a check in the photograph's section
  of the menu, hides it (not remembered between launches).
- ✅ **UX-029**: Less furniture round the photograph. Tone's Auto sits at
  the end of the section's label, small, as White balance's pipette does,
  rather than on a row of its own. The editor's filmstrip is as tall as the
  photographer drags it: the line above it is its handle, 36 to 200 px
  (64 until dragged), kept between launches — the slim 48 of the first test
  was "nog veel kleiner". Measured in the window's coordinates: in the
  handle's own the strip shook and the window stopped answering.

### FLOW — Workflows: Rapid

The research of 30 September (report and canvas "Numa Workflows") and the step
back of 1 October (canvas pages "Een stap terug" to "De review, getekend";
report "Numa als je eigen editor"): Numa stays the whole editor, and a
photographer who needs speed gets a way of looking at a library with only the
few things a kind of shoot is adjusted by — "zonder dat ze het hele edit
programma door hoeven als ze toch maar 4-5 dingen aan hoeven te passen", and
"eerder een alternatieve view van de library", not a route walked in and out
of ("je gaat een route in of uit ipv 'laat me dit vanaf deze kant
benaderen'"). On Linux, on the `workflows` branch, on hold (3 October).

- 🟡 **FLOW-018**: Rapid — the library by its moments, named **Moments** on
  screen since 6 October, once choosing had moved into the loupe (the
  photographer: "het woord rapid ook niet echt meer klopt"; Rapid stays its
  name in the code). View at the left of the library's bar chooses Grid or
  Moments; it is remembered, and nothing is
  entered or left. A moment is a run of frames without a pause of five
  minutes, and one longer than 48 is cut where it pauses longest (across
  8 563 frames of sixteen libraries, five minutes made 756 scenes of a median
  five). Its photographs lie in justified rows at their own shape, as in the
  grid (LIB-015), First Look's larger, and they are the grid's own cards:
  selected with its white ring, marked by its pills (the pick's ⚑, the stars,
  the white dot of a frame the photographer edited — quieter than the grid:
  no Analyse suggestion, and no dot for Numa's own work, which the panel
  says), Rapid adding no
  colour of its own — the design system's parts throughout (2 October: the
  sliders are Numa's, "Numa did" a section, Deliver… the page's one white
  button, Numa's work ahead a banner, keys in tooltips and the shortcuts
  window). A burst is a stack — its frames'
  edges behind the best, "×6", and "Pick a Few" until something in it is
  picked; a click or ↵ culls it (below). The editor's filmstrip holds a
  burst together — its frames side by side closer, square where they meet,
  one line under them, "Pick a Few" on the first, then "2 of 6 Picked" — and
  only the look: every frame is still its own to click and step through ("ik
  wilde de burst gecombineerd hebben in de thumbnails in de editor"). A frame
  with
  nothing in it, eyes shut, or soft where the shutter cannot explain it *and*
  the rest of its moment is sharper is dimmed with the reason — soft against
  the library alone marked whole moments of shallow focus. P, X, U and 0–5
  mark the tile with the keys and move on; the arrows move; E, ↵ or a double
  click open it in the editor, and Back returns to Rapid. Space puts it in the
  loupe, as in the grid — a stack's frames side by side, as a click does —
  and back in Rapid the keys are on the frame the looking ended on. One bar
  over Rapid, the library's chips; Deliver… in the header where the grid
  has Export, and the ways of looking (by time or likeness, First Look,
  Clipping) under the size button's two sliders — Rapid had a second bar of
  buttons, and the photographer, 5 October: "er zijn twee toolbars met
  buttons wat wil je nou van me". First of all Rapid asks the
  library's kind of shoot, big, in place of the moments until it is
  answered ("de allerbelangrijkste dropdown rechtsboven vraagt je aandacht
  helemaal niet"), and under the size button it can be changed —
  Weddings and Events, Sports, Portraits, Property and Interiors, Products
  and Food, Wildlife and Landscape, Travel and Street,
  kept in its own `.numa`. The panel on the right is a moment's, and only
  when one is chosen: a click on a moment's head sets the whole moment on a
  quiet fill (an outline doubled a selected photograph's ring) and opens
  the panel for it, until its ✕, Escape or the head again; the
  arrows and a click on a photograph leave it where it is, and nothing in it
  is about one photograph (7 October: "welk moment heb ik geselecteerd dan?
  hoe weet ik dat? … wat als die groep 250 foto's is?" — it followed the
  tile with the keys, often off screen, and said This Photo too). What is
  one photograph's own — Match the Moment to This Photo, Detach from Moment
  — is on its right-click menu. In the panel: the moment's time, how many,
  how many picked, and four or five sliders set for the whole moment at
  once, on its Even layer (FLOW-019; Events
  and Property: exposure, warmth, highlights, shadows; Sports: exposure,
  warmth, contrast, noise; Portraits: exposure, warmth, shadows, vibrance;
  Products: exposure, whites, warmth, contrast; Wildlife: exposure,
  highlights, contrast, noise; Travel: exposure, warmth, contrast, vibrance).
  Warmth makes a moment one balance: the temperature, with the camera's own
  tint of its first frame. Reset puts the moment's sliders back; All Tools is
  the editor; Next Moment (Page Down) goes to the next one not yet seen, and a
  moment left is marked Seen. Tiles show their stars, and a dot where the
  photographer edited a frame.
  **Let Numa Do…**, only when asked (the photographer, 7 October: "Alleen
  op verzoek en dan met alle mogelijkheden die Numa kan doen, misschien wil
  je alleen alles recht laten zetten" — it went through every moment by
  itself as Moments opened, with a bar across the top, and the fans ran):
  a button under What Numa did opens a dialog with a switch for each part —
  Light, Even Exposure, Straighten, Square the Verticals, and Frame where the
  kind has one — the last choice remembered, then This Moment or Every
  Moment, the latter with UX-019's toast and its Stop and at the end how many
  photographs changed ("Nothing in this moment needed it" for one). Asked
  for, every frame of the moment is done, the ones worked on too; a part
  asked again replaces only itself, so straightening later keeps the frame
  Numa placed. The parts: one light per moment from
  its middle frame (where Auto per photograph made 18 of 40 bursts flicker),
  plus what it learned of the photographer's style in that light; and the
  shape the kind wants — Weddings and Travel straightened by the horizon and
  the uprights (from 0.2°), Property straightened with its verticals squared,
  Sports framed 4:5 and Products 1:1 on the subject (FLOW-008's placing). The
  level and the keystone are Auto's (GEOM-005's scene read): only a level its
  readings agree on, and a keystone of two degrees or more, as the editor's.
  Portraits get no skin: a document does not keep its faces, so evened skin
  would show in the editor and nowhere else. Each frame is kept as it was, and
  what Numa put on it part by part, in the library's catalog.
  **Even Exposure**: within a moment, each frame is brought in line by what
  the camera let in — shutter × ISO ÷ f², against the moment's middle — so a
  frame auto ISO gave a stop more is given that stop back, and the moment's
  exposure slider sets them all on top of their own correction. By the
  camera's numbers, not measured brightness, which would grey a close-up of
  the dress; frames with the flash, without EXIF, or more than 1½ stops out
  (the light changed) are left.
  **What Numa did**, in the moment's panel: Light (in numbers), Even Exposure, Straightened,
  Verticals and Framed, each with a switch for the whole moment. A switch puts
  the before's value back only where the slider is still Numa's — what the
  photographer moved since stays. A moment Numa left has Let Numa Do This
  Moment; Undo Numa's Work… in the library's menu takes all of it back.
  (Light follows the moment, First Frame | Last Frame, is replaced by Key
  Frames…, FLOW-021.)
  **Note…** (N): what is off, in the photographer's words ("te warm", "too
  dark", "the sky is blown"); before anything happens Numa says how it reads
  it ("Warmth −300 K"), or that it does not know the words. For This Photo,
  This Moment, or My Style — this moment and every one like it from now on.
  **Your Style…** (panel and library menu): what Numa learned for this kind
  per light (warm, mixed, daylight by the camera's balance) — the mean of the
  last 20 changes, from sliders set after Numa (one per minute per slider) and
  notes kept as style — with Forget; and the latest changes, dated, with
  Undo. A moment's two ends are not learned from.
  **Moments by hand**: S starts a new moment at the tile with the keys, M
  merges the moment with the one before; kept with the library.
  **Clipping** (J, as in Lightroom; remembered): every tile, the burst pair
  and the check painted where they clip as set now — red where a channel is
  at its top, blue where all three are at the bottom.
  **Chapters** (Evoto's Storyline Mode): a moment's head says "Start Chapter
  Here…" — the kind's chapters (Weddings: Getting Ready, First Look,
  Ceremony, Portraits, Reception, Party), one's own name, or none — and the
  chapter holds the moments after it until the next. Each is a heading a
  click renames; the one scrolled through keeps its name at the top of the
  list, and that name is a menu of them all, each with its photographs, a
  click going to one (7 October, of a bar of chapter chips under the filter
  chips and the name kept under both: "filters met pillen navigatie met
  pillen nog een keer de title sticky — eenvoud").
  For Weddings and Events and Travel, Numa offers the first ("Tell the day
  in chapters?") and the next where the day pauses 20 minutes or more ("New
  chapter here? A 36-minute pause" — Start First Look, or Not Here, which
  it remembers), or where the camera's own balance jumps 1 500 K from one
  moment to the next ("The light went from 3 200 K to 5 600 K"). Deliver counts the picks per chapter and writes them a
  folder each, numbered in the day's order ("02 First Look/…").
  **Stacks of the same picture**: a moment's frames are stacked where they
  are the same picture — a burst, or within Analyse's "same scene" of the
  stack's first frame, in whatever order they came (`workflows::same_picture`;
  "als je 50x net niet dezelfde foto neemt wil je er toch maar een of twee").
  **Numa keeps a few** (Evoto's "per cluster"): a stack says "Numa keeps
  2" — the best of each stretch of the burst by sharpness and Analyse,
  never eyes shut or blank, never the whole burst; how many is the kind's
  (Products 3; Sports, Property, Travel 1; else 2) until set under Stacks
  in the panel; P on a stack picks them.
  **A burst in one look** (Evoto's Survey): a click on a stack, ↵ or Space
  opens its frames side by side in the choosing screen (CULL-015) — the one
  place a burst is chosen, where Rapid had a page of its own for it — with
  the keys on Numa's sharpest: Up keeps, Shift+Down puts the rest out and goes
  back to Rapid, Esc goes back with nothing more done; the keys are on the
  stack in its place.
  **By likeness** (Evoto's General Mode): Time | Likeness under the size
  button;
  Likeness joins moments Analyse found to be the same scene again ("Setup
  1 · 10:05, 13:40 and 16:12"), the default for Products and Property.
  **Its own size**: the library's size button sets Rapid's own photo size
  and space between while Rapid is on screen ("rapid eigen maat"), kept
  apart from the grid's. Its tiles load at the grid's thumbnail size, chosen
  from the monitor (LIB-004): a fixed 360 pixels was stretched nearly four
  times at the largest size on a 2× screen.
  **Bursts**: a stack clicked (or ↵) opened the loupe on its frames only —
  cull mode, with the note "A burst of 6 — pick the few worth keeping (↑ or
  P), pass the rest (→). 2 picked. Space when you are done." — and back in
  Rapid what was picked stands on its own in its place, burst or not, the
  stack keeping the rest (the photographer, 2 October: "als je dan klaar
  bent zijn het losse foto's ook al was het onderdeel van een burst").
  **First Look**, under the size button: one frame from each moment as it
  will go out;
  a moment that is off is set in its panel, P puts a frame in the teaser, and
  Share Teaser… writes it out (FLOW-009).
  **Deliver…**: how many are picked across how many moments, the teaser, and
  the moments with no pick; Check the Picks First goes through every pick
  large, one at a time, with its moment's other picks beside it to see they
  belong together — → next, ← back, X unpicks, E edits and Back returns to
  the check — and at the end Deliver again, with Export N Picks… and Export
  Teaser… through the export dialog (FLOW-011). Linux; Apple not yet.
- ✅ **FLOW-019**: Three layers per moment, live (DaVinci Resolve's colour
  groups: Group Pre-Clip · Clip · Group Post-Clip). A moment's photographs
  share **Even** under each one's own edits — the moment's sliders, its
  white balance, each frame's evening by the camera's numbers — and **Look**
  over them: As Shot, a preset or a LUT, with a Strength. Between them is
  **This Photo**. The panel: THIS MOMENT · its name, then Even, This Photo
  (the sliders the photograph with the keys moved of its own, a double click
  back to the moment's) and Look, numbered 1 to 3 down in the order they are
  laid on (top first as layers lie read 3 to 1: "stappen van 3 tot 1 ipv 1
  tot 3"), each with its switch or count. The moment's values live once, in the library's own
  catalog; a frame names them, and they are put round its own edits wherever
  it is read — tiles, the editor, export, a library's prints — so a change is
  every frame at once and nothing goes stale. An edit in the editor or a
  paste is written back as the frame's own. Detach from Moment keeps a frame
  as it looks and lets the moment's changes pass it by; Join the Moment puts
  it back, its own kept. A look moves sliders by what it moves an untouched
  frame by, so a frame's exposure stays its own; its grade, curve and LUT go
  where the frame has none. Numa's work ahead puts its light, warmth and
  evening on the layers, the shape on each frame — a frame you had worked on
  takes the light off its own again and looks as it did; a switch still moves only
  what is still Numa's, and Undo Numa's Work takes both back. A library from
  before is read as it was, and a moment is given layers the first time one
  of them changes, the same picture to the pixel (tried: 0 pixels over 1 %).
  Linux.
- 🟡 **FLOW-020**: Match to This Photo. On the photograph with the keys, the
  moment's Even set so every frame comes out as it does: what of its exposure
  and balance is its own becomes the moment's; exposure by each frame's
  camera numbers against it (as Even Exposure, 1½ stops at most, flash
  left); colour per kind of camera — its own camera's frames take its
  balance, another camera is moved by how far its frames' light reads from
  the first camera's, each through its own matrix. The light's colour is read
  from the pixels near grey under the camera's own balance (a field of red
  tulips is not red light). Tried on four bodies of four scenes; two cameras
  at one scene not yet (no CC0 pair).
- ✅ **FLOW-021**: Key Frames… — where the light changes through a moment (a
  ceremony into sunset, golden hour): Add This Photo on two or three frames,
  set each one's Exposure and Warmth, and Numa ramps both between them by
  capture time (the LRTimelapse idea), the balance in mireds; before the
  first key the first, after the last the last. The frames say Key Frame,
  and Even says how many it ramps between. Replaces Light follows the moment.
  Linux.
- ❌ **FLOW-001**: ~~A template: a name, its steps in order and a look.~~
  *Replaced by FLOW-018's kinds of shoot: names photographers use ("The
  Celebration" was the research's word, and nobody looks for it), a few
  controls, no steps.*
- ❌ **FLOW-002**: ~~Chosen when a folder is added.~~ *The kind is chosen in
  Rapid's panel, when it is first wanted; adding a folder asks Analyse Now as
  it did before.*
- ❌ **FLOW-003**: ~~Continue Flow and the steps as pills.~~ *Rapid is a view
  of the library: there is no route to be on.*
- ❌ **FLOW-004**: ~~Numa sorts first.~~ *Withdrawn: in 220 reviews of
  culling and editing tools no one had stopped checking an AI cull, and a
  verdict is what pros abandon tools over; evidence is what they keep. Rapid
  stacks bursts and says why a frame is weak, and the photographer chooses.*
- ❌ **FLOW-016**: ~~The flow mode, where a route is walked.~~ *Replaced by
  FLOW-018 ("te overnemend": locked into a process).*
- ❌ **FLOW-005**: ~~Receive, a look given to one frame put on every frame
  that arrives.~~ *Not in Rapid yet. What it found stays: a walk under way is
  waited for rather than taken as the next library's (IO-023).*
- ❌ **FLOW-006**: ~~Save as New Template.~~ *The kinds are Numa's own until
  one's own are asked for.*
- ❌ **FLOW-017**: ~~The review — Sort, Look and Finish as one pass.~~
  *Replaced by FLOW-018: one photograph at a time was the same work
  rearranged, not less of it. Kept from it: what Numa did on record in the
  library's catalog, and Undo Numa's Work.*
- 🟡 **FLOW-007**: Match — moments made alike: white balance from the
  resolved as-shot value and exposure from the EXIF's total exposure, only
  where the light did not change (round 2). In Rapid: one warmth per
  moment, and Even Exposure from shutter × ISO ÷ f², frames more than 1½
  stops out left as the light's own (FLOW-018). Across moments not yet.
- 🟡 **FLOW-008**: Frame — one aspect for a moment (4:5 and 1:1 as they are,
  3:2 and 16:9 turning with the photograph), on the subject or the faces. The
  placing is in the shared core and tested, from the review it was first built
  for: the subject is the largest person or animal the segmentation model
  finds, not a crowd; 6 % of room on every side the camera did not cut, nobody
  else of a size halved, the rule of thirds, and never a zoom — the largest
  frame of its shape the photograph holds. On screen as Numa's work ahead
  for Sports (4:5) and Products (1:1), with its switch; not yet a choice of
  aspect per moment.
- 🟡 **FLOW-009**: Teaser — a small set first, with a look, shared the same
  night (round 2). Built as First Look in Rapid (FLOW-018); a look of its own
  for the teaser not yet.
- 🟡 **FLOW-010**: Chapters within a shoot, and Sort in rounds (round 2).
  Moments split and joined by hand in Rapid (S, J); rounds not yet.
- 🟡 **FLOW-011**: Deliver in layers, the export recipes as one template's
  deliveries (round 2). Rapid's Deliver: the teaser and the picks, with a
  check through the picks first; recipes per kind not yet.
- ◻️ **FLOW-012**: The iPad as the first desk: safe copy, 100 % at the focus
  point, the first round (round 3).
- 🟡 **FLOW-013**: Receive without a second app — the camera on its cable,
  each photograph in Numa the moment it is taken. A camera plugged in shows
  in the library's header bar by name: the import button's camera gains the
  camera's name, a stock menu button with Shoot Tethered… and Import… (the
  photographer, 7 October: "net als Lightroom … meteen gevonden", and one
  camera icon, not two), and goes back when it is pulled out — seen from the device nodes under /dev/bus/usb, so
  nothing polls and the Flatpak sees it too; in a session it only names the
  camera. Pressed, or Shoot Tethered… (the main menu, and the Import dialog's
  From), it asks which camera (the first on a
  cable, found by libgphoto2; "No camera found yet" with Look Again, and a
  line on the camera's own setting: USB Tether Shooting on a Fujifilm, PC
  Remote on a Sony), Into — the library whose days these are, or a new one
  named for today where new ones go, as the Import suggests (Change…) — and
  Same Edit as the Last (remembered). Start begins even before the camera is
  there. While it runs a banner over the library says so ("Tethered to
  Fujifilm X-T5 · 12 photographs into 2026-10-04") with Stop, and the main
  menu has Stop Tethering; in the editor, where no banner is, a camera that
  stops answering is a toast. Each frame is fetched whole into the library's
  folder under the camera's name (`-2`, `-3` when taken, never over another
  file), with only what Numa opens taken — not voice memos or films. It
  opens in the loupe, the culling view (the photographer, 4 October:
  "tethered meteen in culling modus"), and the next follows it there as long
  as the photographer is still on the one before — gone back to an earlier
  frame, or left for the grid, they are left there; in the editor it opens
  in the editor, for someone adjusting the next frame. Numa reads each frame
  as it comes in — Analyse on that frame alone — and the loupe says what the
  card says, in a word or two beside the name ("DSCF0202.JPG · soft", "eyes
  closed?"); evidence, never a reject of its own — in the research's 220
  reviews a verdict is what photographers leave a tool over. With Same Edit
  as the Last each new frame takes the edit of the one before it, crop
  included — Lightroom's "Same as Previous", Capture One's "Copy from Last".
  A camera switched off, asleep or unplugged is waited for and taken up
  again ("Fujifilm X-T5 is back"); one set to give its card rather than to
  shoot — an X-T5 in USB Card Reader, which libgphoto2 finds all the same —
  is said in the banner ("Fujifilm X-T5 is set to read its card — choose USB
  Tether Shooting in its connection settings"), known by its capture
  settings being empty, and taken up when it is switched; one the file manager holds (GVFS mounts a
  camera when it is plugged in) is asked for back. Linux, through libgphoto2,
  opened when tethering is first wanted and never linked, and never in the
  shared core (LGPL); the Flatpak carries it, the distribution packages
  recommend it. Tried with a stand-in for the camera (`NUMA_TETHER_FROM`),
  not yet with one on a cable. The Mac, through ImageCaptureCore, is on
  Numa-mac's `tether` branch, not yet compiled.
- 🟡 **FLOW-014**: Delivery per person by face, and a list of who was
  already sent theirs (round 3). Rapid's Deliver has Export for N People…:
  the picks each named person (People…) is in, a folder each, a photograph
  of two in both. Deliver lists each with when their folder was written,
  and offers only those who have none yet (or everyone again once all have).
- ◻️ **FLOW-015**: One's own kinds of shoot and their controls, from the six
  questions (round 3).

---

## Priorities

The status markers above are the roadmap; the tier lists that stood here went
stale as items were built, so this list is generated from them instead. What
is not yet built, in the order the groups appear:

- planned — **START-014** What gets used, if the photographer agrees
- partly built — **APP-007** Other languages — the Apple app's String Catalog, English for now
- partly built — **CULL-009** Where the camera focused — the AF box built, no "focus missed?" verdict yet, Fujifilm only
- partly built — **CULL-003** Faces, from YuNet
- partly built — **CULL-004** A suggested rating, 0–5, shown beside the photograph and sortable
- partly built — **CULL-005** A score learned from this photographer's own ratings
- planned — **RENDER-015** Numa's own camera profiles for every camera, and a call to send a neutral raw
- partly built — **RENDER-017** The editor's render on the graphics card — on since 29 September; the dmabuf hand-over still off
- planned — **DETAIL-008** Settle whether a developed frame is as sharp as Lightroom's and Capture One's, with numbers rather than an impression
- partly built — **HDR-003** HDR output — HDR files built, the HDR screen deferred
- partly built — **MASK-008** Background and people masks, and Person and Animal find a subject the semantic model has never heard of
- partly built — **PERF-005** The grid decodes what is on screen
- partly built — **PERF-065** The editor's proxy averaged from the mosaic — the two-pass warp left as a proposal
- partly built — **PERF-049** AVX2, the DCP hue lookup and the histogram, measured
- partly built — **UX-021** The rail — Grade's dot misses a range the page is not showing
- done — **PERF-051** The grid and the filmstrip make widgets only for the cards on screen
- partly built — **FLOW-013** Tethering — Linux built, the Mac (ImageCaptureCore) written and not yet compiled

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
