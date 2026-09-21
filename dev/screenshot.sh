#!/usr/bin/env bash
set -u
out=$1; wait_for=${2:-14}
export GDK_BACKEND=x11 GSK_RENDERER=cairo
export NUMA_OPEN=${3:-12:50}
export NUMA_TAB=${4:-light}
[ -n "${5:-}" ] && export NUMA_MASK=$5
Xvfb :77 -screen 0 1920x1080x24 >/dev/null 2>&1 &
xvfb=$!
sleep 2
DISPLAY=:77 "$(dirname "$0")/../target/release/numa" >"${out%.png}.log" 2>&1 &
app=$!
sleep "$wait_for"
DISPLAY=:77 import -window root "$out" 2>/dev/null
kill $app 2>/dev/null; sleep 1; kill $xvfb 2>/dev/null
