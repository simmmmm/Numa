# AppImage

**There is a Flatpak too** — 18 MB against 113, and it takes its GNOME stack
from a runtime instead of carrying one. See `packaging/flatpak/`. This one is
for machines without flatpak, and since the stack below it is no longer the
older one it is not the lesser build any more.

```sh
./packaging/appimage/build.sh          # writes dist/Numa-<version>-x86_64.AppImage
```

Needs Docker. Takes a few minutes; most of it is the release build.

## Why it builds in a container

glibc is backwards compatible and not forwards. A binary linked against this
machine's 2.44 refuses to start on anything older, and in 2026 that is almost
every machine you would hand a test build to — Ubuntu 24.04 is on 2.39, Fedora
42 on 2.41. Building on Ubuntu 24.04 sets the floor there instead.

**Runs on:** anything with glibc 2.39 or newer and a graphical session. Ubuntu
24.04+, Fedora 40+, Debian 13+, any current rolling release.

## Why the GNOME stack is built from source

Ubuntu 24.04 carries libadwaita 1.5, so the first version of this wore a
stylesheet three years older than the one the application was designed against
— the Flatpak looked current and this did not.

That looked like a trade against reach and is not one. glibc is the only thing
that has to be old, and it comes from the base image; everything above it can
be built and carried along. `stack.sh` builds libadwaita 1.9.3 and what it needs
into `/usr/local`, where pkg-config finds it first. What 24.04 is short of,
measured rather than assumed:

| | distribution | built |
|---|---|---|
| wayland | 1.22 | 1.24.0 |
| glib | 2.80 | 2.84.4 |
| cairo | 1.18.0 | 1.18.4 |
| harfbuzz | 8.3 | 10.4.0 |
| pango | 1.52 | 1.56.4 |
| gtk4 | 4.14 | 4.22.4 |
| libadwaita | 1.5 | 1.9.3 |

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
satisfy the check. It is installed in a layer above the stack, so finding that
out cost two layers rather than the whole thing.

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
icon theme are all in there — that is the GTK plugin's job and the reason the
file is 47 MB. The schemas are copied from `/usr/local/share` as well as
`/usr/share` before the plugin runs, because the plugin looks in the usual
places and this stack is not in them. GTK reads `org.gtk.Settings.*` at startup
and aborts when they are not registered.

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

**`rawler` (LGPL-2.1) and `lensfun` (LGPL-3.0-or-later) are linked into it
statically**, because that is what Rust does. Both licences require whoever
receives the binary to be able to relink it against a *modified* copy of those
libraries. Dynamic linking satisfies that on its own — the library is a separate
file to swap. Static linking does not, so a bare binary handed to someone is
short of the obligation. The cheap way to satisfy it is to pass the source along
with the AppImage; it is your code and any licence will do.

**The application itself declares `LicenseRef-UNLICENSED`.** That is fine for
giving a file to somebody — it only means they have no right to pass it on
further. Publishing means choosing a licence.

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
# 1. Does it run where nothing is installed? "Failed to open display" is the
#    pass: every library loaded and GTK got as far as looking for a screen.
docker run --rm -v "$PWD/dist":/img:ro ubuntu:24.04 bash -c '
  apt-get update -qq && apt-get install -y -qq --no-install-recommends \
    libx11-6 libxcb1 libwayland-client0 libfontconfig1 libfreetype6 fontconfig-config
  cd /tmp && cp /img/*.AppImage . && chmod +x *.AppImage
  APPIMAGE_EXTRACT_AND_RUN=1 ./Numa-*-x86_64.AppImage'

# 2. Does it run cleanly on a *newer* system than it was built on? This is what
#    caught the fontconfig problem.
./dist/Numa-*-x86_64.AppImage
```
