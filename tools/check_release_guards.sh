#!/usr/bin/env bash
# The arms of the two release guards in this repository that `verify` in
# release.yml runs before anything is published; the third, release-ci-green,
# is tested in Luolc/ci-actions. Each guard has a matching arm (exit 0), a
# differing arm (exit 1) and an unreadable arm (exit 4), each run on its own;
# a guard that exited 0 on everything, or 1 on everything, fails here.
#
#   release_tag_version.sh  over a throwaway manifest
#   release_on_main.sh      in a throwaway repository: a commit on main, one
#                           on a branch never merged, one that does not exist

set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d "${TMPDIR:-/tmp}/limae-release-guards.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
status=0

fail() {
  printf 'release guards: FAIL: %s\n' "$1" >&2
  status=1
}

# expect <want exit> <name> <command>...
expect() {
  local want=$1 name=$2 got
  shift 2
  "$@" >"$tmp/out" 2>&1
  got=$?
  [ "$got" -eq "$want" ] || { cat "$tmp/out" >&2; fail "$name: exit $got, want $want"; }
}

# release_tag_version.sh
mkdir "$tmp/crate"
printf '%s\n' '[package]' 'name = "limae"' 'version = "1.2.3"' 'edition = "2024"' '' '[lib]' 'path = "lib.rs"' \
  >"$tmp/crate/Cargo.toml"
: >"$tmp/crate/lib.rs"
tag_guard=$repo_root/tools/release_tag_version.sh
expect 0 "tag: matching" "$tag_guard" v1.2.3 "$tmp/crate/Cargo.toml"
expect 1 "tag: other version" "$tag_guard" v1.2.4 "$tmp/crate/Cargo.toml"
expect 1 "tag: no v" "$tag_guard" 1.2.3 "$tmp/crate/Cargo.toml"
expect 4 "tag: no manifest" "$tag_guard" v1.2.3 "$tmp/crate/missing.toml"
# The manifest of this repository is the one release.yml reads.
version=$(cargo metadata --no-deps --format-version 1 --locked --manifest-path "$repo_root/Cargo.toml" |
  jq -er '.packages[] | select(.name == "limae") | .version') || fail "cannot read this repository's version"
expect 0 "tag: this repository" "$tag_guard" "v$version" "$repo_root/Cargo.toml"

# release_on_main.sh
g() { git -C "$tmp/git" -c user.name=test -c user.email=test@example.invalid "$@"; }
mkdir "$tmp/git"
g init -q -b main
g commit -q --allow-empty -m one
on=$(g rev-parse HEAD)
g switch -q -c side
g commit -q --allow-empty -m two
off=$(g rev-parse HEAD)
g switch -q main
g commit -q --allow-empty -m three
main_guard=$repo_root/tools/release_on_main.sh
in_git() { (cd "$tmp/git" && "$@"); }
expect 0 "main: on main" in_git "$main_guard" "$on" main
expect 1 "main: never merged" in_git "$main_guard" "$off" main
expect 4 "main: no such commit" in_git "$main_guard" 0000000000000000000000000000000000000000 main
expect 4 "main: no base" in_git "$main_guard" "$on" origin/main

[ "$status" -eq 0 ] && echo "release guards: OK"
exit "$status"
