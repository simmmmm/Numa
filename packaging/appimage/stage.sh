#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
APPDIR="$ROOT_DIR/packaging/appimage/AppDir"
BIN="$ROOT_DIR/target/release/numa"
ICON_SRC="$ROOT_DIR/data/icons/hicolor/256x256/apps/com.tijmen.Numa.png"
DESKTOP_SRC="$ROOT_DIR/data/com.tijmen.Numa.desktop"

if [[ ! -f "$BIN" ]]; then
  echo "Release binary not found: $BIN" >&2
  echo "Run: cargo build --release" >&2
  exit 1
fi

mkdir -p \
  "$APPDIR/usr/bin" \
  "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/scalable/apps"

install -m 0755 "$BIN" "$APPDIR/usr/bin/numa"
install -m 0644 "$DESKTOP_SRC" "$APPDIR/usr/share/applications/com.tijmen.Numa.desktop"
install -m 0644 "$ICON_SRC" "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png"
install -m 0644 "$ROOT_DIR/data/icons/hicolor/256x256/apps/com.tijmen.Numa-dark.png" \
        "$APPDIR/usr/share/icons/hicolor/256x256/apps/com.tijmen.Numa-dark.png"

install -m 0644 "$DESKTOP_SRC" "$APPDIR/com.tijmen.Numa.desktop"
install -m 0644 "$ICON_SRC" "$APPDIR/com.tijmen.Numa.png"

if [[ ! -x "$APPDIR/AppRun" ]]; then
  echo "AppRun missing or not executable at $APPDIR/AppRun" >&2
  exit 1
fi

echo "AppDir staged at: $APPDIR"
