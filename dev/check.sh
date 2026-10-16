#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

CAP=10G dev/capped.sh cargo test --workspace --release
python3 dev/shape.py
python3 dev/style.py
python3 dev/tokens.py --check

for pair in numa:THIRD_PARTY_LICENSES numa-server:THIRD_PARTY_LICENSES-server; do
    crate=${pair%%:*} file=data/${pair#*:}.txt
    if ! python3 dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p "$crate" | cmp -s - "$file"; then
        echo "$file is out of date: dev/third-party-licences.py --target x86_64-unknown-linux-gnu -p $crate > $file" >&2
        exit 1
    fi
done
