#!/usr/bin/env bash
# Release guard: only a commit already merged into main is released.
# Usage: release_on_main.sh COMMIT [BASE]   (BASE defaults to origin/main)
# Exit 0 when COMMIT is in the history of BASE, 1 when it is not, 4 when
# either cannot be resolved (a shallow clone without BASE, for example).
set -uo pipefail
case $# in 1 | 2) ;; *) echo "usage: release_on_main.sh COMMIT [BASE]" >&2; exit 2 ;; esac
commit=$1
base=${2:-origin/main}
for r in "$commit" "$base"; do
  git rev-parse --verify --quiet "$r^{commit}" >/dev/null || { echo "cannot resolve $r" >&2; exit 4; }
done
git merge-base --is-ancestor "$commit" "$base"
case $? in
  0) echo "$commit is on $base" ;;
  1) echo "$commit is not on $base" >&2; exit 1 ;;
  *) echo "cannot compare $commit with $base" >&2; exit 4 ;;
esac
