#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/dist}"

mkdir -p "$OUT"
docker build -t numa-appimage -f "$ROOT/packaging/appimage/Dockerfile" \
    "$ROOT/packaging/appimage"

docker run --rm \
    -v "$ROOT":/src:ro \
    -v "$OUT":/build \
    numa-appimage /src/packaging/appimage/bundle.sh

echo
echo "AppImage in $OUT"
