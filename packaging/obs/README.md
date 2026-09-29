# Distribution packages (openSUSE Build Service)

One OBS package builds Numa for Fedora, openSUSE, Debian, Ubuntu and Arch, and
keeps a repository per distribution that updates with the system. OBS takes
only open source, which is why Numa has been GPL-3.0-or-later since 29
September 2026.

| File | For |
|---|---|
| `numa.spec` | Fedora, openSUSE |
| `numa.dsc`, `debian.*` | Debian, Ubuntu (OBS's debtransform makes the source package) |
| `PKGBUILD` | Arch |
| `build.sh` | the build and install steps all three share |
| `_constraints` | a worker with room for the fat-LTO link |

## A release

```sh
dev/publish.sh                   # the public copy, ../Numa-public
packaging/obs/sources.sh         # target/obs: the tarball and the recipes
packaging/obs/test.sh            # optional: all five built in containers
```

OBS builds with no network, so the tarball carries the crates (`cargo vendor`)
and Microsoft's ONNX Runtime 1.28.0. ONNX Runtime goes into `/usr/lib/numa`,
found through the binary's rpath, so it cannot clash with a distribution's own.
Numa's camera profiles and the LensFun database go into `/usr/share/numa`,
where Numa looks from its binary.

Then into the OBS checkout, with [osc](https://github.com/openSUSE/osc):

```sh
cd ~/obs/home:<user>/numa
rm -f * && cp ../../../path/to/Numa/target/obs/* .
osc addremove && osc commit -m "Numa <version>"
```

## Setting it up once

1. An openSUSE account at build.opensuse.org, which gives `home:<user>`.
2. There, **Create Package** `numa`, and under **Repositories** add Fedora
   (current), openSUSE Tumbleweed, Debian Testing, Ubuntu 26.04 and Arch, each
   for x86_64.
3. `osc checkout home:<user>/numa` asks for the password once.

The package page then has **Download package**, with the lines to add the
repository on each distribution.

## Not yet

- **Debian 13 and Ubuntu 24.04.** Their Rust is too old (1.85 and 1.91; the
  gtk4-rs crates want 1.92). Debian 13 has 1.94 in trixie-backports, which OBS
  can use once the project lists that repository; Ubuntu 24.04 has nothing
  newer. The Flatpak and the AppImage cover both meanwhile.
- **aarch64.** Microsoft publishes ONNX Runtime for it too; the spec is
  `ExclusiveArch: x86_64` until someone runs it on one.
