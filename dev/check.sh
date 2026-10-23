#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

CAP=10G dev/capped.sh cargo test --workspace --release
python3 dev/shape.py
python3 dev/style.py
python3 dev/style_web.py
python3 dev/tokens.py --check
python3 dev/smoke-readme.py

for pair in numa:THIRD_PARTY_LICENSES numa-server:THIRD_PARTY_LICENSES-server; do
    crate=${pair%%:*} file=data/${pair#*:}.txt
    if ! python3 dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p "$crate" | cmp -s - "$file"; then
        echo "$file is out of date: dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p $crate > $file" >&2
        exit 1
    fi
done

(cd data/map && sha256sum --quiet --strict -c SHA256SUMS) || { echo "data/map changed: update its NOTICE.txt and SHA256SUMS" >&2; exit 1; }
if [ "$(find data/map -type f ! -name NOTICE.txt ! -name SHA256SUMS | wc -l)" != "$(wc -l < data/map/SHA256SUMS)" ]; then
    echo "data/map has a file SHA256SUMS does not list" >&2
    exit 1
fi
maplibre=$(grep -o 'maplibre-gl-js/blob/v[0-9.]*/' data/map/maplibre-gl.mjs | head -1 | grep -o '[0-9][0-9.]*[0-9]')
grep -qF "MapLibre GL JS $maplibre " data/map/NOTICE.txt && grep -qF "/map/$maplibre/" crates/numa-server/src/pages.rs && grep -qF "/map/$maplibre/" crates/numa-server/src/web/map.js \
    || { echo "MapLibre GL JS $maplibre: data/map/NOTICE.txt, pages.rs and map.js must each name it" >&2; exit 1; }
