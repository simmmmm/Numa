#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/dist}"

mkdir -p "$OUT"
docker build -t numa-appimage -f "$ROOT/packaging/appimage/Dockerfile" \
    "$ROOT/packaging/appimage"

docker run --rm \
    --tmpfs /tmp:rw,exec,size=12g \
    --tmpfs /root/.cargo/registry:rw,exec,size=4g \
    --tmpfs /root/.cargo/git:rw,exec,size=1g \
    -v "$ROOT":/src:ro \
    -v "$OUT":/build \
    numa-appimage /src/packaging/appimage/bundle.sh

echo
echo "AppImage in $OUT"
