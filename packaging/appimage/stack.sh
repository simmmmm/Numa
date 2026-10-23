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
    if declare -F patch_source >/dev/null; then (cd "$dir" && patch_source); fi
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
wayland-protocols)
    build https://gitlab.freedesktop.org/wayland/wayland-protocols/-/releases/1.45/downloads/wayland-protocols-1.45.tar.xz \
        -Dtests=false
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
    patch_source() {
        sed -i "s/fontconfig_req = '>= 2.15.0'/fontconfig_req = '>= 2.13.0'/" meson.build
        grep -q "fontconfig_req = '>= 2.13.0'" meson.build
    }
    build https://download.gnome.org/sources/pango/1.56/pango-1.56.4.tar.xz \
        -Dintrospection=disabled -Ddocumentation=false -Dbuild-testsuite=false \
        -Dbuild-examples=false \
        "-Dc_args=['-DFC_ORDER=\"order\"','-DFC_FONT_WRAPPER=\"fontwrapper\"']"
    ;;
glslc)
    src=/tmp/shaderc
    rm -rf "$src"
    git clone -q --depth 1 --branch v2026.4 https://github.com/google/shaderc "$src"
    for dep in glslang:KhronosGroup/glslang spirv-headers:KhronosGroup/SPIRV-Headers \
               spirv-tools:KhronosGroup/SPIRV-Tools; do
        name="${dep%%:*}"
        rev="$(sed -n "s/.*'${name//-/_}_revision': '\([0-9a-f]*\)'.*/\1/p" "$src/DEPS")"
        git init -q "$src/third_party/$name"
        git -C "$src/third_party/$name" fetch -q --depth 1 "https://github.com/${dep#*:}" "$rev"
        git -C "$src/third_party/$name" checkout -q FETCH_HEAD
    done
    cmake -S "$src" -B "$src/build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
        -DSHADERC_SKIP_TESTS=ON -DSHADERC_SKIP_EXAMPLES=ON -DSHADERC_SKIP_COPYRIGHT_CHECK=ON
    ninja -C "$src/build" -j8 glslc_exe
    install -m 0755 "$src/build/glslc/glslc" /usr/local/bin/glslc
    rm -rf "$src"
    glslc --version
    ;;
*)
    echo "unknown package: $1" >&2
    exit 1
    ;;
esac
