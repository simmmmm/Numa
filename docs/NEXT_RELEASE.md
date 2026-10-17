# Next release: 0.42.0

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md), and after the
bump `data/THIRD_PARTY_LICENSES.txt` is generated again and `dev/check.sh` run.

0.41.0 went out on 16 October.
What went into it is in this file's history (`git log -p docs/NEXT_RELEASE.md`).

The version is 0.42.0 in `Cargo.toml` and the metainfo (17 October): Linux
(HOME-030) and Apple (HOME-027, 028, 030) changed since 0.41.0 as well as Numa
Home, so this is a train, not Home alone. Licences generated again,
`dev/check.sh` green. `main`, Numa-mac `mac` and numa-site `main` pushed.

## In

- ~~**Numa Home, round five (HOME-025, 026, 029)**~~ — on `main` 17 October:
  a drag with the mouse chooses, a right-click opens Numa's menu, the
  selection bar's star and ⋯ menus open; By names only the people with
  photographs where you are; a trip named after where it was from its GPS
  in Import and Rename (GeoNames in the binary), and a trip without GPS
  borrows the household's places from the same hours.
- ~~**Libraries on Apple, round two (HOME-027, HOME-028)**~~ — on Numa-mac
  `mac` e9de0d28 (17 October), the mark no button any more with it; the
  guide on numa-site `main` 359fc22 (live). The photographer: "ik zou graag
  alle libraries willen zien niet alleen alle fotos … albums willen kunnen
  syncen of sturen naar numa home vanaf mijn ipad of telefoon als ik on the
  go ben". Numa Home's libraries on the Libraries page; every shelf one row
  that scrolls sideways with See All (iPhone, iPad; the Mac keeps the grid);
  Keep at Numa Home on a library's or a Photos album's stack, up only.
  Swift compiled first by TestFlight.
- ~~**Numa Home offered once (HOME-030)**~~ — merged 17 October: Linux on
  `main`, Apple on Numa-mac `mac` 06181cd3, guide on numa-site `main`. The
  photographer, 17 October: "als een gebruiker nog geen home gebruikt wil ik
  dit wel als een kleine upsell laten zien in alle apps". A card where Numa
  Home's row would be, no taller than a row: three prints with the house,
  NUMA HOME, "Keep them at home", Set Up… (Preferences on Linux, Settings on
  Apple) and ×. Closed, or once signed in, it never comes back. Swift
  compiled first by TestFlight.

## Earlier, in 0.38.0

- ~~**All EXIF (UX-010)**~~ — merged 8 October: Linux `main`, the guide on
  numa-site `main`, the Mac and the iPad's panel on Numa-mac `mac` 3d059828
  (numa-ffi `all_exif`; the iPhone's sheet keeps to the facts). The photographer: "meer
  advanced exif die we niet standaard hoeven te tonen maar wel allemaal uit
  kunnen lezen". A fold at the foot of the info panel lists every tag the
  file holds; read only while open, open until shut. Checked on 4 files each
  of RAF, ARW, CR3, CR2, NEF, DNG, ORF, RW2 and PEF, and on HIF and JPEG
  (`NUMA_ALL_EXIF=<list> cargo test -p numa-io --release --lib
  every_format_lists -- --ignored`).
  - Release note: "All EXIF, at the foot of the photo info panel: every tag
    the file holds, from the camera's settings to the lens serial and the
    GPS, folded away until you open it."
  - The Swift is compiled first by the TestFlight build.

- ~~**Numa Home (HOME-001–012)**~~ — merged 15 October: the server
  (`crates/numa-server`, image on ghcr.io/simmmmm/numa-home through
  `dev/publish.sh`, built on this PC), Send Home and Numa Home libraries in
  Linux Numa; the Apple side (HOME-007, Send Home) on Numa-mac `home`, merged
  into `mac`, Swift compiled first by TestFlight. Running on the
  photographer's UGREEN since 15 October. The page: github.com/simmmmm/numa-home.
  - Release note: in the metainfo.

## Guide

- The iPhone's mask screenshot (`shots/iphone/editor-mask-chosen`) still shows
  the old toolbar; the screenshot round after 0.35.0 replaces it.
