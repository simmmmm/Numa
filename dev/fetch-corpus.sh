#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../tests/corpus"

grep -v '^#' manifest.tsv | while IFS=$'\t' read -r file make model url sha256 _; do
    if [ -f "$file" ] && echo "$sha256  $file" | sha256sum --check --status; then
        continue
    fi
    echo "fetching $make $model"
    mkdir -p "$(dirname "$file")"
    curl -fsSL --retry 3 -o "$file.part" "$url"
    if ! echo "$sha256  $file.part" | sha256sum --check --status; then
        rm -f "$file.part"
        echo "$file does not match its SHA-256" >&2
        exit 1
    fi
    mv "$file.part" "$file"
done
