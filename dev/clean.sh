#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

du -sh target 2>/dev/null || exit 0
rm -rf target/*/incremental
cargo clean -q -p numa
cargo clean -q --release
du -sh target
