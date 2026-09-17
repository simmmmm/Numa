#!/usr/bin/env bash
set -euo pipefail

PREFIX=/usr/local
export PKG_CONFIG_PATH="$PREFIX/lib/x86_64-linux-gnu/pkgconfig:$PREFIX/lib/pkgconfig:$PREFIX/share/pkgconfig"
export LD_LIBRARY_PATH="$PREFIX/lib/x86_64-linux-gnu:$PREFIX/lib"

build() {
    local url="$1"; shift
    local dir="/tmp/src"
    rm -rf "$dir" && mkdir -p "$dir"
    curl -sSL "$url" | tar -xJ -C "$dir" --strip-components=1
    meson setup "$dir/_build" "$dir" \
        --prefix="$PREFIX" --libdir="lib/x86_64-linux-gnu" \
        --buildtype=release "$@"
    meson compile -C "$dir/_build"
    meson install -C "$dir/_build"
    ldconfig
    rm -rf "$dir"
}

case "${1:?which package}" in
gtk)
    build https://download.gnome.org/sources/gtk/4.22/gtk-4.22.4.tar.xz \
        -Dintrospection=disabled -Ddocumentation=false -Dman-pages=false \
        -Dbuild-demos=false -Dbuild-examples=false -Dbuild-tests=false \
        -Dbuild-testsuite=false -Dmedia-gstreamer=disabled -Dvulkan=enabled \
        -Dcloudproviders=disabled -Dsysprof=disabled -Dcolord=disabled \
        -Dprint-cups=disabled
    ;;
libadwaita)
    build https://download.gnome.org/sources/libadwaita/1.9/libadwaita-1.9.3.tar.xz \
        -Dintrospection=disabled -Ddocumentation=false -Dtests=false -Dexamples=false
    pkg-config --modversion gtk4 libadwaita-1 glib-2.0 pango cairo
    ;;
*)
    echo "unknown package: $1" >&2
    exit 1
    ;;
esac
