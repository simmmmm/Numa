#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TREE="$(cd "${1:-$ROOT/../Numa-public}" && pwd)"
VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$TREE/Cargo.toml" | head -1)"
ORT_URL=https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-x64-1.28.0.tgz
ORT_SHA=a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407
OUT="$ROOT/target/obs"
SRC="$OUT/numa-$VERSION"

rm -rf "$OUT"
mkdir -p "$SRC/onnxruntime" "$SRC/.cargo" "$ROOT/target/obs-cache"
tar -C "$TREE" --exclude=./target --exclude=./.git -cf - . | tar -C "$SRC" -xf -

(cd "$SRC" && cargo vendor --locked --versioned-dirs cargo-vendor > .cargo/config.toml)

ORT="$ROOT/target/obs-cache/${ORT_URL##*/}"
[[ -f "$ORT" ]] || curl -fsSL "$ORT_URL" -o "$ORT"
echo "$ORT_SHA  $ORT" | sha256sum -c --quiet
tar -xzf "$ORT" --strip-components=1 -C "$SRC/onnxruntime"

tar -C "$OUT" -cJf "$OUT/numa-$VERSION.tar.xz" "numa-$VERSION"
rm -rf "$SRC"

HERE="$ROOT/packaging/obs"
for f in numa.spec numa.dsc PKGBUILD debian.control debian.rules _constraints; do
    sed "s/@VERSION@/$VERSION/g" "$HERE/$f" > "$OUT/$f"
done
cat > "$OUT/debian.changelog" <<CHANGES
numa ($VERSION-1) unstable; urgency=medium

  * Numa $VERSION.

 -- $(sed -n 's/^Maintainer: //p' "$HERE/debian.control")  $(date -R)
CHANGES
ls -lh "$OUT"
