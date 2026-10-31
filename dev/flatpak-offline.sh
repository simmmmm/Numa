#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

reference=packaging/flathub-reference
work=target/flathub-ref
mkdir -p "$work"

flatpak run --filesystem="$PWD" --command=flatpak-cargo-generator org.flatpak.Builder \
    Cargo.lock -o "$reference/cargo-sources.json"
if ! git diff --quiet -- "$reference/cargo-sources.json"; then
    echo "cargo-sources.json changed: Cargo.lock and it were out of step; commit the new one" >&2
fi

if grep -q -- '--share=network' <(sed -n '/build-args/,/^[^ ]/p' "$reference/com.tijmen.Numa.yml"); then
    echo "the manifest's build asks for the network" >&2
    exit 1
fi

builder() {
    flatpak run --filesystem="$PWD" org.flatpak.Builder --disable-rofiles-fuse \
        --state-dir="$work/state" --repo="$work/repo" "$@" "$work/build" "$reference/com.tijmen.Numa.yml"
}
builder --force-clean --download-only
builder --force-clean --disable-download
echo "built with no network"
