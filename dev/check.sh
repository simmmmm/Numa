#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

CAP=10G dev/capped.sh cargo test --workspace --release
python3 dev/shape.py
python3 dev/style.py
python3 dev/tokens.py --check

if ! python3 dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p numa | cmp -s - data/THIRD_PARTY_LICENSES.txt; then
    echo "data/THIRD_PARTY_LICENSES.txt is out of date: dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p numa > data/THIRD_PARTY_LICENSES.txt" >&2
    exit 1
fi
