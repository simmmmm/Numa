#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

CAP=10G dev/capped.sh cargo test --workspace --release
python3 dev/shape.py
