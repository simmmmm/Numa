# Distribution packages (openSUSE Build Service)

One OBS package, [`home:Numa/numa`](https://build.opensuse.org/package/show/home:Numa/numa),
builds Numa for Fedora, openSUSE, Debian, Ubuntu and Arch, and keeps a
repository per distribution that updates with the system. Users add it from the
[download page](https://software.opensuse.org/download.html?project=home:Numa&package=numa). OBS takes
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

`dev/publish.sh` does all of it, with the other platforms. By hand:

```sh
dev/publish-dev.sh copy          # ../Numa-public, and from it target/obs
packaging/obs/test.sh            # optional: all five built in containers
```

OBS builds with no network, so the tarball carries the crates (`cargo vendor`)
and Microsoft's ONNX Runtime 1.28.0. ONNX Runtime goes into `/usr/lib/numa`,
found through the binary's rpath, so it cannot clash with a distribution's own.
Numa's camera profiles and the LensFun database go into `/usr/share/numa`,
where Numa looks from its binary.

Then into the OBS checkout, with [osc](https://github.com/openSUSE/osc):

```sh
osc checkout home:Numa numa -o <somewhere>     # once
cd <somewhere> && rm -f * && cp <Numa>/target/obs/* .
osc addremove && osc commit -m "Numa <version>"
```

`osc` is on PyPI (`pipx install osc`, with `pipx inject osc keyring` so the
password stays in the keyring). The account is `Numa`, with a capital N.

## The project

`home:Numa` builds for Fedora_44, openSUSE_Tumbleweed, Debian_Testing,
xUbuntu_26.04 and Arch, all x86_64 (`osc meta prj home:Numa`). Its project
config says `Prefer: hdf5` for Arch, where something deep in the dependencies
could be either hdf5 or hdf5-openmpi and OBS will not choose.

## Not yet

- **Debian 13, Ubuntu 24.04 and Fedora 43.** Their Rust is too old (1.85,
  1.91, and 1.90 on OBS, which has Fedora 43 without its updates; the gtk4-rs
  crates want 1.92). Debian 13 has 1.94 in trixie-backports, which OBS
  can use once the project lists that repository; Ubuntu 24.04 has nothing
  newer. The Flatpak and the AppImage cover both meanwhile.
- **aarch64.** Microsoft publishes ONNX Runtime for it too; the spec is
  `ExclusiveArch: x86_64` until someone runs it on one.
