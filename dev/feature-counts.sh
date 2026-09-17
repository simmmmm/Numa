#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
count() { grep -cE "^- $1 \*\*[A-Z]+-[0-9]{3}\*\*" docs/FEATURES.md || true; }
echo "built $(count ✅) · partly built $(count 🟡) · planned $(count ◻️) · withdrawn $(count ❌)"
