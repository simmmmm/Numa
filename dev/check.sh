#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo test --workspace --release
python3 dev/shape.py
