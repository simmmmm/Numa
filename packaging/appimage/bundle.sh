#!/usr/bin/env bash
set -euo pipefail

export CARGO_TARGET_DIR=/tmp/target
export PATH="/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0:$PATH"

cd /src
cargo build --release

APPDIR=/tmp/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
         "$APPDIR/usr/share/icons/hicolor/scalable/apps"

install -m 0755 "$CARGO_TARGET_DIR/release/numa" "$APPDIR/usr/bin/numa"
install -m 0644 data/com.tijmen.Numa.desktop \
        "$APPDIR/usr/share/applications/com.tijmen.Numa.desktop"

VERSION="$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)"
if [[ -z "$VERSION" ]]; then
    echo "no version in Cargo.toml" >&2
    exit 1
fi
printf 'X-AppImage-Version=%s\n' "$VERSION" \
    >> "$APPDIR/usr/share/applications/com.tijmen.Numa.desktop"
echo "building version $VERSION"
install -Dm0644 data/icons/hicolor/256x256/apps/com.tijmen.Numa.png \
        "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png"
install -Dm0644 data/icons/hicolor/256x256/apps/com.tijmen.Numa-dark.png \
        "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.tijmen.Numa-dark.png"
install -Dm644 data/icons/hicolor/scalable/actions/numa-sliders-symbolic.svg \
        "$APPDIR/usr/share/icons/hicolor/scalable/actions/numa-sliders-symbolic.svg"

ICONS="$APPDIR/usr/share/icons/hicolor"
install -Dm644 /usr/share/icons/hicolor/index.theme "$ICONS/index.theme"
for name in $(grep -rhoE '"[a-z0-9-]+-symbolic"' /src/src | tr -d '"' | sort -u); do
    found="$(find /usr/share/icons/Adwaita -name "$name.svg" -path '*symbolic*' | head -1)"
    if [[ -n "$found" ]]; then
        install -Dm644 "$found" "$ICONS/scalable/actions/$name.svg"
    elif [[ ! -e "$ICONS/scalable/actions/$name.svg" ]]; then
        echo "no icon named $name in Adwaita" >&2
    fi
done
install -Dm644 /usr/share/doc/adwaita-icon-theme/copyright \
        "$APPDIR/usr/share/doc/adwaita-icon-theme/copyright"
echo "bundled $(ls "$ICONS/scalable/actions" | wc -l) symbolic icons"

EXTRA=()
for soname in libfribidi.so.0 libharfbuzz.so.0 libvulkan.so.1 libgpg-error.so.0 libcom_err.so.2; do
    for dir in /usr/local/lib/x86_64-linux-gnu /usr/lib/x86_64-linux-gnu; do
        if [[ -e "$dir/$soname" ]]; then
            EXTRA+=(--library "$dir/$soname")
            break
        fi
    done
done

PROFILES="$APPDIR/usr/share/numa/profiles"
mkdir -p "$PROFILES"
install -m 0644 /opt/profiles/rt/rtdata/dcpprofiles/*.dcp "$PROFILES/"
install -m 0644 /opt/profiles/rt/LICENSE "$PROFILES/LICENSE.GPL-3.0.txt"
echo "bundled $(ls "$PROFILES"/*.dcp | wc -l) camera profiles"

cat > "$PROFILES/README.txt" <<'NOTICE'
DNG camera profiles from RawTherapee
https://github.com/Beep6581/RawTherapee  (rtdata/dcpprofiles, tag 5.11)

RawTherapee is free software released under the GNU General Public License
version 3, reproduced here as LICENSE.GPL-3.0.txt. These profiles are part of
that distribution and are included under the same terms. Most were made by
Maciej Dworak; individual profiles name their author in the DNG
ProfileCopyright tag.

Numa reads these at runtime and never modifies them. It also looks in
/usr/share/rawtherapee/dcpprofiles and ~/.local/share/numa/profiles, so
a profile installed on the machine is found without being copied.

There is no profile here for the Fujifilm X-T5. RawTherapee does not ship one,
and the Adobe Standard profile for that body is Adobe's and cannot be
redistributed. Put your own in ~/.local/share/numa/profiles.
NOTICE

mkdir -p "$APPDIR/usr/share/glib-2.0/schemas"
for schemas in /usr/local/share/glib-2.0/schemas /usr/share/glib-2.0/schemas; do
    [[ -d "$schemas" ]] || continue
    find "$schemas" -maxdepth 1 \( -name '*.xml' -o -name '*.gschema.override' \) \
        -exec cp -n {} "$APPDIR/usr/share/glib-2.0/schemas/" \;
done
glib-compile-schemas "$APPDIR/usr/share/glib-2.0/schemas"
echo "registered $(grep -hc '<schema ' "$APPDIR"/usr/share/glib-2.0/schemas/*.xml | \
    awk '{total += $1} END {print total}') settings schemas"

GTK4_MODULES="$(pkg-config --variable=libdir gtk4)/gtk-4.0"
mkdir -p "$GTK4_MODULES"
mkdir -p "$APPDIR/usr/lib/x86_64-linux-gnu"
cp -r "$GTK4_MODULES" "$APPDIR/usr/lib/x86_64-linux-gnu/"

export APPIMAGE_EXTRACT_AND_RUN=1
export DEPLOY_GTK_VERSION=4
export LD_LIBRARY_PATH="/usr/local/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"

cd /build
/opt/tools/linuxdeploy-x86_64.AppImage \
    --appdir "$APPDIR" \
    --plugin gtk \
    "${EXTRA[@]}" \
    --desktop-file "$APPDIR/usr/share/applications/com.tijmen.Numa.desktop" \
    --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png"

HOOK="$APPDIR/apprun-hooks/linuxdeploy-plugin-gtk.sh"
if [[ ! -f "$HOOK" ]]; then
    echo "no GTK plugin hook found — the theme fix did not apply" >&2
    exit 1
fi
sed -i -e 's|^export GTK_THEME=|: # removed, see bundle.sh — was: export GTK_THEME=|' \
       -e 's|^export GDK_BACKEND=x11|: # removed, see bundle.sh — was: export GDK_BACKEND=x11|' \
       "$HOOK"
for gone in GTK_THEME GDK_BACKEND; do
    if grep -qE "^export $gone=" "$HOOK"; then
        echo "hook still exports $gone" >&2
        exit 1
    fi
done
echo "startup hook corrected: GTK_THEME and GDK_BACKEND left alone"

DOCS="$APPDIR/usr/share/doc"

fetch_licence() {
    local name="$1"; shift
    mkdir -p "$DOCS/$name"
    for url in "$@"; do
        if ! curl -sSfL "$url" -o "$DOCS/$name/$(basename "$url")"; then
            echo "could not fetch $url" >&2
            exit 1
        fi
    done
    local bytes
    bytes=$(cat "$DOCS/$name"/* | wc -c)
    if (( bytes < 1000 )); then
        echo "the licence for $name came to $bytes bytes, which is not a licence" >&2
        exit 1
    fi
}

fetch_licence glib-2.84.4 \
    https://gitlab.gnome.org/GNOME/glib/-/raw/2.84.4/LICENSES/LGPL-2.1-or-later.txt
fetch_licence gtk-4.22.4 \
    https://gitlab.gnome.org/GNOME/gtk/-/raw/4.22.4/COPYING
fetch_licence pango-1.56.4 \
    https://gitlab.gnome.org/GNOME/pango/-/raw/1.56.4/COPYING
fetch_licence libadwaita-1.9.3 \
    https://gitlab.gnome.org/GNOME/libadwaita/-/raw/1.9.3/COPYING
fetch_licence cairo-1.18.4 \
    https://gitlab.freedesktop.org/cairo/cairo/-/raw/1.18.4/COPYING \
    https://gitlab.freedesktop.org/cairo/cairo/-/raw/1.18.4/COPYING-LGPL-2.1 \
    https://gitlab.freedesktop.org/cairo/cairo/-/raw/1.18.4/COPYING-MPL-1.1
fetch_licence harfbuzz-10.4.0 \
    https://raw.githubusercontent.com/harfbuzz/harfbuzz/10.4.0/COPYING
fetch_licence wayland-1.24.0 \
    https://gitlab.freedesktop.org/wayland/wayland/-/raw/1.24.0/COPYING

echo "licences for $(ls "$DOCS" | grep -cE '^(glib|gtk|pango|libadwaita|cairo|harfbuzz|wayland)-') source-built libraries"

ARCH=x86_64 /opt/tools/appimagetool-x86_64.AppImage \
    "$APPDIR" "/build/Numa-$VERSION-x86_64.AppImage"

rm -rf "$APPDIR"
