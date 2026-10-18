#!/usr/bin/env bash
set -uo pipefail

name=$1
image=$2
how=$3
heading=$4
release=$5
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
out=${OUT:-$PWD/smoke-out}
raws=${RAWS:-$HOME/.cache/numa-smoke}
github=${REPOSITORY:-simmmmm/Numa}
wait_minutes=${WAIT_MINUTES:-180}

rm -rf "$out"
mkdir -p "$out" "$raws"

fail() {
    echo "$1" > "$out/stage"
    echo "$2" > "$out/why"
}

served() {
    for arch in x86_64 amd64; do
        curl -fsSL "https://download.opensuse.org/repositories/home:/Numa/$1/$arch/" 2>/dev/null
    done | grep -oE 'numa(-photo)?[-_][0-9]+\.[0-9]+\.[0-9]+' | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | sort -V | tail -n 1
}

run() {
    awk -v h="$heading" 'index($0, h) == 1 { f = 1; next } f && /^```sh$/ { p = 1; next } p && /^```$/ { exit } p' "$repo/README.md" > "$out/readme.sh"
    if [[ ! -s "$out/readme.sh" ]]; then
        fail README "no sh block under $heading"
        return 1
    fi
    if [[ "$how" == readme ]]; then
        dir=$(grep -m 1 -oE 'home:/Numa/[^/ ]+' "$out/readme.sh" | cut -d/ -f3)
        deadline=$((SECONDS + wait_minutes * 60))
        while version=$(served "$dir"); [[ "$version" != "$release" ]]; do
            if ((SECONDS >= deadline)); then
                fail OBS "$dir serves ${version:-nothing}, the release is $release (waited $wait_minutes min)"
                return 1
            fi
            echo "OBS $dir serves ${version:-nothing}, waiting for $release"
            sleep 600
        done
    fi

    while IFS=$'\t' read -r file url sha256 _; do
        if ! echo "$sha256  $raws/$file" | sha256sum --check --status 2>/dev/null; then
            curl -fsSL --retry 3 -o "$raws/$file" "$url"
            echo "$sha256  $raws/$file" | sha256sum --check --status || { fail raws "$file does not match its SHA-256"; return 1; }
        fi
    done < "$here/raws.tsv"

    mkdir -p "$out/bundle"
    local flags=()
    case "$how" in
    appimage) asset="Numa-$release-x86_64.AppImage" ;;
    flatpak) asset="Numa-$release-x86_64.flatpak" flags=(--privileged) ;;
    esac
    if [[ "$how" != readme ]]; then
        curl -fsSL --retry 3 -o "$out/bundle/$asset" "https://github.com/$github/releases/download/v$release/$asset" \
            || { fail download "$asset is not on the v$release release"; return 1; }
    fi

    docker run --rm "${flags[@]}" -e LOCALE="${LOCALE:-}" \
        -v "$repo:/repo:ro" -v "$raws:/raws:ro" -v "$out/bundle:/bundle:ro" -v "$out:/out" \
        "$image" bash /repo/packaging/smoke/inside.sh "$how" "$heading"
    echo $? > "$out/exit"
}

run 2>&1 | tee "$out/log.txt"
python3 "$here/check.py" "$name" "$out" "$release" "$here/raws.tsv"
