# Contributing

## Reporting a problem

Open an issue with the **Report a Problem** form. Include the debug
information from **About → Troubleshooting → Debug Information** (the
version, the graphics card and the distribution) and, for a RAW file that opens wrongly, the
camera model. A sample file helps most if you can share it, for example on
[raw.pixls.us](https://raw.pixls.us).

## Building and testing

```sh
cargo run --release           # the desktop app
cargo test --workspace        # the tests
dev/check.sh                  # the tests and the checks CI runs
```

Tests that use real camera files read raw.pixls.us's CC0 samples from
`NUMA_CC0_RAWS` (default: a folder `raws-cc0` beside the repository) and are
skipped when it is missing. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
explains where things are in the code.

`dev/check.sh` fails on new code that is too long: files over 2000 lines,
functions over 150 lines, structs over 30 fields. Split the code rather than
raising the limit.

## Code

- Interface colours and sizes come from `src/ui/tokens.css`; do not hard-code
  them.
- New user-visible behaviour belongs in the [user guide](https://guide.numa.photo).

This repository is updated from the development tree with each release, so a
change you contribute appears in the next release.

## Packaging

Numa is packaged on the openSUSE Build Service (`packaging/obs/`) and as a
Flatpak and an AppImage (`packaging/flatpak/`, `packaging/appimage/`). It is
not yet in any distribution's own repositories, Flathub, the AUR or the Snap
Store; if you want to maintain a package, open an issue. Flathub does not
accept manifests written with AI, which the one in this repository was, so a
Flathub manifest has to be written by hand.
