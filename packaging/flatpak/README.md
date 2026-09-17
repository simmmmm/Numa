# Flatpak

```sh
flatpak run org.flatpak.Builder --force-clean --repo=repo build \
    packaging/flatpak/com.tijmen.Numa.yml
flatpak build-bundle repo dist/Numa.flatpak com.tijmen.Numa master
```

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
flatpak install --user Numa.flatpak
flatpak run com.tijmen.Numa
```

The GNOME 50 runtime is downloaded automatically the first time, about a
gigabyte, and is then shared with every other flatpak that uses it.

## Notes

`--filesystem=host`: a photo editor reads photographs from wherever they are
kept, routinely an external drive. The narrower permission breaks the first
library anyone adds.

The catalog lives in `~/.var/app/com.tijmen.Numa/data/numa`, so a
flatpak install starts empty rather than picking up a native build's library.
Camera profiles you supply yourself go in `.../data/numa/profiles`; the
159 that ship with the build are in `/app/share/numa/profiles` and are
found without any of that.

The build fetches crates from the network, which is why Flathub would not accept
this manifest as it stands — vendoring is the fix when that matters, and for a
build handed to testers it is not worth the tarball.
