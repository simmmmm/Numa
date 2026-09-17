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
wayland)
    build https://gitlab.freedesktop.org/wayland/wayland/-/releases/1.24.0/downloads/wayland-1.24.0.tar.xz \
        -Ddocumentation=false -Dtests=false -Ddtd_validation=false
    ;;
glib)
    build https://download.gnome.org/sources/glib/2.84/glib-2.84.4.tar.xz \
        -Dintrospection=disabled -Ddocumentation=false -Dtests=false -Dman-pages=disabled
    ;;
cairo)
    build https://cairographics.org/releases/cairo-1.18.4.tar.xz \
        -Dtests=disabled -Dspectre=disabled -Dsymbol-lookup=disabled
    ;;
harfbuzz)
    build https://github.com/harfbuzz/harfbuzz/releases/download/10.4.0/harfbuzz-10.4.0.tar.xz \
        -Dtests=disabled -Ddocs=disabled -Dintrospection=disabled \
        -Dfreetype=enabled -Dglib=enabled -Dcairo=enabled
    ;;
pango)
    build https://download.gnome.org/sources/pango/1.56/pango-1.56.4.tar.xz \
        -Dintrospection=disabled -Ddocumentation=false -Dbuild-testsuite=false \
        -Dbuild-examples=false
    ;;
*)
    echo "unknown package: $1" >&2
    exit 1
    ;;
esac
