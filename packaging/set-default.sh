#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APPS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ID="com.tijmen.Numa.desktop"

if [[ $# -ge 1 ]]; then
    APPIMAGE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
    [[ -x "$APPIMAGE" ]] || { echo "$APPIMAGE is not executable." >&2; exit 1; }
    mkdir -p "$APPS"
    sed "s|^Exec=.*|Exec=\"$APPIMAGE\" %F|" "$ROOT/data/$ID" > "$APPS/$ID"
    echo "Registered $APPIMAGE as $APPS/$ID"
fi

DESKTOP="$APPS/$ID"
[[ -f "$DESKTOP" ]] || DESKTOP="/usr/share/applications/$ID"
[[ -f "$DESKTOP" ]] || {
    echo "No $ID found. Install Numa, or pass the path to the AppImage." >&2
    exit 1
}

command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true

TYPES="$(sed -n 's/^MimeType=//p' "$DESKTOP" | tr ';' ' ')"
for type in $TYPES; do
    xdg-mime default "$ID" "$type" || echo "  could not set $type" >&2
done
echo "Numa opens: $TYPES"
