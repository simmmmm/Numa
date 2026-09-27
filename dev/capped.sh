#!/usr/bin/env bash
set -euo pipefail
echo 900 > /proc/self/oom_score_adj 2>/dev/null || true
if systemctl --user cat claude-tests.slice >/dev/null 2>&1; then
    exec systemd-run --user --scope --quiet --slice=claude-tests.slice \
        -p MemoryMax="${CAP:-6G}" -p MemorySwapMax=0 -- "$@"
fi
exec "$@"
