# Flatpak

```sh
flatpak run org.flatpak.Builder --force-clean --disable-rofiles-fuse \
    --state-dir=target/flatpak/state --repo=target/flatpak/repo \
    target/flatpak/build packaging/flatpak/com.tijmen.Numa.yml
flatpak build-bundle target/flatpak/repo dist/Numa-<version>-x86_64.flatpak com.tijmen.Numa master
```

Everything it makes goes under `target/`, which the manifest skips; built in
the source folder instead, each build would copy the last one's output in.

Needs `org.flatpak.Builder`, `org.gnome.Sdk//50` and
`org.freedesktop.Sdk.Extension.rust-stable//25.08`, all from Flathub.

## Why this and not only the AppImage

The AppImage has to carry GTK and libadwaita, so it carries whatever the build
base had. Built on Ubuntu 24.04 for the sake of a usable glibc floor, that is
libadwaita 1.5 — the GNOME 46 stylesheet, which does not look like a 2026
desktop. A runtime removes the problem rather than trading against it: the
platform brings its own libadwaita to every machine, and the host's glibc stops
mattering.

Runtime 50 rather than 49, checked rather than assumed: 49 carries libadwaita
1.8.7 and GTK 4.20, 50 carries 1.9.3 and 4.22.

| | AppImage | Flatpak |
|---|---|---|
| size | 113 MB | 18 MB |
| libadwaita | 1.5 | 1.9.3 |
| runs on | glibc 2.39+ | anything with flatpak |
| recipient needs | nothing | flatpak, and the runtime once |

## Installing a bundle

```sh
flatpak install --user Numa-<version>-x86_64.flatpak
flatpak run com.tijmen.Numa
```

The GNOME 50 runtime is downloaded automatically the first time, about a
gigabyte, and is then shared with every other flatpak that uses it.

## Notes

`--filesystem=host`: a photo editor reads photographs from wherever they are
kept, routinely an external drive. The narrower permission breaks the first
library anyone adds.

The catalogue, presets, models and thumbnails are the host's own
`~/.local/share/numa` and `~/.cache/numa`, not the `~/.var/app` folders a
Flatpak is given (`numa_core::paths`): a Flatpak, an AppImage and a build of
your own share them, so moving to the Flatpak loses nothing and downloads no
model twice. Camera profiles you supply yourself go in
`~/.local/share/numa/profiles`; the ones that ship with the build are in
`/app/share/numa/profiles`.

`--system-talk-name=org.freedesktop.ColorManager`: the screen's colour profile
(A1) comes from colord on the system bus.

Preferences leaves out the "Open photographs with Numa" switch: GIO inside the
sandbox would write the choice where the file manager never reads it. It says
to use the file manager's Open With instead; the Flatpak's desktop entry lists
every photograph type, so Numa is offered there.

The build fetches crates from the network, which is why Flathub would not accept
this manifest as it stands — vendoring is the fix when that matters, and for a
build handed to testers it is not worth the tarball.

## Moving from the AppImage

An AppImage's menu entry has the same name, `com.tijmen.Numa.desktop`, and
sits in `~/.local/share/applications`, ahead of the Flatpak's. While the
AppImage is there, the menu starts the AppImage; once it is deleted, its entry
hides itself and the Flatpak's with it. Any Numa that is not an AppImage now
removes such an entry at start when the file it points at is gone. With the
AppImage still in place, switch "Show Numa in the applications menu" off in
its Preferences first.
