# Next release: 0.39.0

What goes into the next general build and is not in `main` or Numa-mac `mac`
yet. Before `dev/publish.sh` each item is merged, checked, and its line struck
here; the version bump comes after that (docs/AGENT_GUIDE.md), and after the
bump `data/THIRD_PARTY_LICENSES.txt` is generated again and `dev/check.sh` run.

0.38.0 went out on 15 October (with Numa Home's first image on ghcr.io).
What went into it is in this file's history (`git log -p docs/NEXT_RELEASE.md`).

The version is 0.39.0 in `Cargo.toml` and the metainfo — never released, so
no bump for what joins it.

## In

- **Numa Home, round two (HOME-010, 011, 013–015)** — on `main` and Numa-mac
  `mac` 15 October: Activity and Cameras never blank, the header under
  900 px, the scrubber's oldest months, Open in Numa (Linux, web, Apple),
  the Live Activity and the share sheet (Apple, uncompiled). Before the
  first Apple build the photographer registered the App Group
  `group.com.tijmen.Numa` (15 October).

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
