# Next release: 0.41.0

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md), and after the
bump `data/THIRD_PARTY_LICENSES.txt` is generated again and `dev/check.sh` run.

0.40.0 went out on 15 October.
What went into it is in this file's history (`git log -p docs/NEXT_RELEASE.md`).

The version is 0.41.0 in `Cargo.toml` and the metainfo (16 October): Linux
(HOME-006) and Apple (HOME-022) changed since 0.40.0 as well as Numa Home, so
this is a train, not Home alone. Licences generated again, `dev/check.sh` green.

## In

- ~~**Numa Home, round four (HOME-021, 023, 024)**~~ — on `main` 16 October:
  libraries chosen like photographs with a bar to combine, remove and delete
  them (the archive's own trash, 30 days); For a Book; every place in one
  list, a library, album or person as a page, a selection bar that holds on
  a phone. The server's own third-party licences (HOME-001) merged too.
- ~~**Send Home signed out (HOME-006)**~~ — on `main`: Numa Home in Linux's
  export dialog when signed out, greyed, with Sign In.
- ~~**Numa Home on Apple (HOME-022)**~~ — on Numa-mac `mac`: albums and
  thumbnails on the iPhone, iPad and Mac, Connected after signing in. Swift
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
