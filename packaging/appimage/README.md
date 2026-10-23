# AppImage

**There is a Flatpak too** — 18 MB against 113, and it takes its GNOME stack
from a runtime instead of carrying one. See `packaging/flatpak/`. This one is
for machines without flatpak, and since the stack below it is no longer the
older one it is not the lesser build any more.

```sh
./packaging/appimage/build.sh          # writes dist/Numa-<version>-x86_64.AppImage
                                       # and its .zsync
```

Needs Docker. Takes a few minutes; most of it is the release build
(`--profile dist`, fat LTO: about three minutes on its own).

## Why it builds in a container

glibc is backwards compatible and not forwards. A binary linked against a
current distribution's glibc refuses to start on anything older. The build runs
on Ubuntu 22.04, the oldest Ubuntu LTS still in standard support, so every ELF
file in the AppImage needs glibc 2.35 at most.

**Runs on:** anything with glibc 2.35 or newer and a graphical session: Ubuntu
22.04+, Debian 12+, Fedora 36+, any current rolling release.

From the host it takes glibc, libstdc++, fontconfig and freetype, the display
stack (X11, XCB, Wayland, EGL/GL), and FUSE to mount itself (`fusermount3`).
Everything else is in the file.

## Why the GNOME stack is built from source

Ubuntu 22.04 carries libadwaita 1.1 and GTK 4.6; built against those, the
application would wear a stylesheet years older than the one it was designed
against, and the Flatpak would look current while this did not.

glibc is the only thing that has to be old, and it comes from the base image;
everything above it can be built and carried along. `stack.sh` builds
libadwaita 1.9.3 and what it needs into `/usr/local`, where pkg-config finds it
first. What 22.04 is short of:

| | distribution | built |
|---|---|---|
| wayland | 1.20 | 1.24.0 |
| wayland-protocols | 1.25 | 1.45 (build only) |
| glib | 2.72 | 2.84.4 |
| cairo | 1.16.0 | 1.18.4 |
| harfbuzz | 2.7.4 | 10.4.0 |
| pango | 1.50 | 1.56.4 |
| gtk4 | 4.6 | 4.22.4 |
| libadwaita | 1.1 | 1.9.3 |
| libshumate | none | 1.7.0 (Places' map; libsoup 3, json-glib, protobuf-c are 22.04's) |
| Vulkan headers | 1.3.204 | 1.3.275 (build only) |
| glslc (shaderc) | none | 2026.4 (build only) |

pango 1.56 asks for fontconfig 2.15 for two property names, `FC_ORDER` and
`FC_FONT_WRAPPER`, which are strings in a header rather than functions. 22.04
has fontconfig 2.13, and fontconfig is the host's at run time, so `stack.sh`
defines the two names and lets pango build against 2.13. A newer host's
fontconfig knows them and pango uses them; on 2.13 they match nothing, as for
pango before 1.56.

One package per Docker layer, on purpose: the first version was a single script,
libadwaita failed at the end of it, and a `RUN` that fails caches nothing — so
twenty minutes of glib, cairo, harfbuzz, pango and GTK were thrown away to
discover a missing apt package.

The list is split across two files for the same reason. GTK and libadwaita are
the two that get rebuilt when something about the renderer changes, and they are
last, so `stack-top.sh` is copied in after the five below it — touching it
leaves those cached.

Vulkan is on, and was off at first on the reasoning that keeps fontconfig and
the display stack out of the bundle: a carried library that talks to the host's
drivers is a coupling waiting to break. That reasoning does not hold here. The
Vulkan *loader* is not a driver — it reads the host's ICD manifests and dlopens
the host's driver, and that indirection is exactly what makes carrying it safe.
Without it GTK falls back to the GL renderer, which is what the editor felt
like. linuxdeploy excludes the loader by default, so `bundle.sh` asks for it by
name; without that the AppImage would refuse to start anywhere the loader is
missing, because GTK links it.

Verified both ways: `GskVulkanRenderer` on a machine with a driver, and a clean
start on a container with zero ICD manifests, where GTK falls back to GL on its
own.

GTK's Vulkan renderer compiles its own shaders and wants shaderc's `glslc` —
`glslang-tools` is a different compiler under a different name and does not
satisfy the check. Ubuntu 22.04 does not package it, so `stack.sh` builds the
compiler alone (glslang, SPIRV-Tools and SPIRV-Headers at the revisions shaderc
pins). It is a build tool and does not ship.

Printing is off because the application cannot print; that leaves GTK with no
modules at all and therefore no `$libdir/gtk-4.0`, which linuxdeploy's plugin
copies without checking, so `bundle.sh` creates the empty directory the plugin
needs to find nothing in.

## Two lines removed from the plugin's startup hook

linuxdeploy's GTK plugin writes an AppRun hook, and two of its exports have to
go. Both are in `bundle.sh`, with a check that fails the build if they return.

`GTK_THEME=Adwaita:<variant>` is why the first build still looked like
libadwaita 1.5 after 1.9.3 had been bundled and was demonstrably the library
being loaded — proved by `adw_bottom_sheet_new`, `adw_spinner_new` and
`adw_toggle_group_new` being present in it, none of which exist in 1.5. Setting
`GTK_THEME` makes GTK load the stylesheet of the *theme* named Adwaita, which is
GTK's own and has not tracked libadwaita for years. libadwaita installs its
stylesheet itself and has to be left to. The variable also pinned light or dark
at launch, so the application stopped following the desktop when that changed.

`GDK_BACKEND=x11` is a workaround for a Wayland crash GTK4 does not have, and
its cost is XWayland: no fractional scaling, blurry on a HiDPI screen. Without
it, measured on a Wayland session: native, and no crash.

## What is bundled, and what deliberately is not

GTK4, libadwaita, pango, the GSettings schemas, the gdk-pixbuf loaders and the
icon theme are all in there — that is the GTK plugin's job.

The symbolic icons Numa names are copied from Adwaita 46, fetched as source in
the Dockerfile rather than taken from 22.04's package (version 41, in which
nearly all of them are drawn differently). They go into hicolor, so a desktop
whose icon theme is not Adwaita still finds them.

The schemas are copied from `/usr/local/share` as well as `/usr/share` before
the plugin runs, because the plugin looks in the usual places and this stack is
not in them. GTK reads `org.gtk.Settings.*` at startup
and aborts when they are not registered.

ONNX Runtime is Microsoft's release (1.28.0), linked as a shared library and
carried in `usr/lib`, as the distribution packages do (`packaging/obs`). The
static library the `ort` crate downloads by itself needs glibc 2.38 and GCC 13's
libstdc++ and does not link on 22.04; Microsoft's needs glibc 2.27.

Two libraries are added past linuxdeploy's exclude list: `libfribidi` and
`libharfbuzz`. Pure text shaping, no configuration, nothing coupled to the
machine, and a bare system does not have them.

**Not bundled, after trying:** fontconfig and freetype. They are on that exclude
list for a reason. A bundled fontconfig reads the *host's* `/etc/fonts`, and
that configuration is written for the host's version — an Ubuntu 24.04
fontconfig meeting a 2026 Arch config produced a hundred lines of "invalid
constant used" on the first machine it ran on. The display stack (`libX11`,
`libxcb`, `libwayland-client`) is left alone for the same class of reason: it
has to match what the machine is actually running.

## Sharing a build

Two things the binary carries that constrain who you can hand it to.

**`rawler` (LGPL-2.1) is linked into it statically**, because that is what
Rust does. Its licence requires whoever
receives the binary to be able to relink it against a *modified* copy of that
library. Dynamic linking satisfies that on its own — the library is a separate
file to swap. Static linking does not, so a bare binary handed to someone is
short of the obligation. The cheap way to satisfy it is to pass the source along
with the AppImage, or a pointer to the tagged public source.

**The application itself is GPL-3.0-or-later.** Its licence is in
`usr/share/licenses/com.tijmen.Numa/LICENSE`. Whoever passes the AppImage on
passes on the source with it (or a written offer of it), and the tag the
release was built from is that source; it covers rawler's relinking too.

RawTherapee's camera profiles are GPL-3 and travel with their licence text and
attribution; they are data files read at runtime rather than part of the
program, which is aggregation under section 5. The GNOME stack is LGPL and
dynamically linked, with the licences below.

### Licences in the bundle

linuxdeploy deploys a copyright file for every *distribution* library it
bundles, by reading dpkg's metadata — 69 of them. The seven built from source
have no dpkg metadata and so arrived with none, which is the largest part of the
bundle shipping without its licence. `bundle.sh` fetches them.

Two of the seven keep a pointer in `COPYING` rather than the text: glib's is
thirty bytes naming a file under `LICENSES/`, and cairo's is a paragraph saying
the terms are LGPL-2.1 or MPL-1.1 and to see the two files beside it. The first
version fetched `COPYING` and asked only whether curl had succeeded, which a
thirty-byte signpost answers yes to. There is a size check now: under a kilobyte
fails the build.

## Camera profiles

RawTherapee's 159 DNG camera profiles are bundled, which is 62 MB of the 113 MB
and the reason the file is that size. RawTherapee is GPL-3.0 in its entirety and
the profiles live in its repository, so they travel under those terms with the
licence text and the attribution beside them in
`usr/share/numa/profiles/`. The per-profile `ProfileCopyright` tag names
the author; it is not a separate grant.

**There is no X-T5 profile in that set** and no way to fix that by bundling:
RawTherapee does not ship one, and the Adobe Standard profile for that body is
Adobe's. Put your own in `~/.local/share/numa/profiles`, which is
searched first. Without any profile the panel says "No camera profile — colour
matrix only" and the colour is visibly flatter.

## Updates

The AppImage carries update information,
`gh-releases-zsync|simmmmm|Numa|latest|Numa-*-x86_64.AppImage.zsync`, and
`appimagetool` writes `Numa-<version>-x86_64.AppImage.zsync` beside it.
Each GitHub release carries both. AppImage updaters read the newest release's
`.zsync` and download only the blocks that changed.

## What a tester still needs

Nothing to run it. Optionally **the face model** for CULL-003, via
`dev/fetch-models.sh`; without it the face columns stay empty and everything
else works.

Film simulations are reported, not applied (RENDER-006); there is nothing to
fetch for them.

## Checking a build

Starting it on the machine that built it proves very little — that machine has
GTK4 already. What the two useful checks are:

```sh
# 1. Does it run where nothing is installed, on the oldest system it is for?
#    "Failed to open display" is the pass: every library loaded and GTK got as
#    far as looking for a screen.
docker run --rm -v "$PWD/dist":/img:ro ubuntu:22.04 bash -c '
  apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    libx11-6 libxcb1 libwayland-client0 libfontconfig1 libfreetype6 fontconfig-config
  cd /tmp && cp /img/*.AppImage . && chmod +x *.AppImage
  APPIMAGE_EXTRACT_AND_RUN=1 ./Numa-*-x86_64.AppImage'

# 2. Does it run cleanly on a *newer* system than it was built on? This is what
#    caught the fontconfig problem.
./dist/Numa-*-x86_64.AppImage
```

AppImageHub's test (`code/worker.sh` in AppImage/appimage.github.io) is
stricter than check 1. On the oldest Ubuntu LTS in standard support, it starts
the AppImage without network under Xvfb (800×600, X11, `LANG=C`, no dark-mode
preference) and needs a window within 30 seconds that is not empty, shows no
error text, is English and is light. It also runs `desktop-file-validate`,
`appdir-lint.sh` and `appstreamcli validate` on the AppDir, and reads the
newest glibc symbol any ELF file in the AppImage needs. The main window it
photographs is the listing's screenshot.
