#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

IDLE_DAYS=${IDLE_DAYS:-3}
dry=; [ "${1:-}" = --dry-run ] && dry=echo
cutoff=$(date -d "$IDLE_DAYS days ago" +%s)

in_main() { git merge-base --is-ancestor "$1" origin/main || ! git cherry origin/main "$1" | grep -q '^+'; }
idle() { [ "$(git log -1 --format=%ct "$1")" -lt "$cutoff" ]; }

git fetch --quiet --prune origin

for ref in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin); do
    branch=${ref#origin/}
    case $branch in main | HEAD | origin) continue ;; esac
    in_main "$ref" && idle "$ref" && $dry git push --quiet origin --delete "$branch" && echo "remote $branch"
done

git worktree list --porcelain | awk '/^worktree /{w=$2} /^branch /{sub("refs/heads/","",$2); print w, $2}' |
while read -r tree branch; do
    [ "$branch" = main ] && continue
    in_main "$branch" && idle "$branch" || continue
    [ -z "$(find "$tree" -path "$tree/target" -prune -o -path "$tree/.git" -prune -o -type f -newermt "@$cutoff" -print -quit)" ] || continue
    $dry git worktree remove "$tree" && echo "worktree $tree"
done

for branch in $(git for-each-ref --format='%(refname:short)' refs/heads); do
    [ "$branch" = main ] && continue
    git worktree list --porcelain | grep -qx "branch refs/heads/$branch" && continue
    in_main "$branch" && idle "$branch" && $dry git branch --quiet -D "$branch" && echo "branch $branch"
done
exit 0
