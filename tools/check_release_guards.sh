#!/usr/bin/env bash
# The arms of the three release guards that `verify` in release.yml runs
# before anything is published. Each guard has a matching arm (exit 0), a
# differing arm (exit 1) and an unreadable arm (exit 4), each run on its own;
# a guard that exited 0 on everything, or 1 on everything, fails here.
#
#   release_tag_version.sh  over a throwaway manifest
#   release_on_main.sh      in a throwaway repository: a commit on main, one
#                           on a branch never merged, one that does not exist
#   release_ci_green.sh     with gh replaced by a stub that prints one canned
#                           answer per call (the last one repeats)

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

# release_ci_green.sh
mkdir "$tmp/bin"
cat >"$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$STUB_DIR/args"
n=$(($(cat "$STUB_DIR/calls" 2>/dev/null || echo 0) + 1))
echo "$n" >"$STUB_DIR/calls"
[ -e "$STUB_DIR/$n" ] || n=$(ls "$STUB_DIR" | grep -Ex '[0-9]+' | sort -n | tail -n 1)
[ "$(cat "$STUB_DIR/$n")" = FAIL ] && { echo "HTTP 502" >&2; exit 1; }
cat "$STUB_DIR/$n"
STUB
chmod +x "$tmp/bin/gh"
ci_guard=$repo_root/tools/release_ci_green.sh
# run <id> <status> <conclusion or null> <run_attempt>
run() {
  local c=$3
  [ "$c" = null ] || c="\"$c\""
  printf '{"workflow_runs":[{"id":%s,"status":"%s","conclusion":%s,"run_attempt":%s}]}' "$1" "$2" "$c" "$4"
}
none='{"workflow_runs":[]}'
case_n=0
# ci <want exit> <name> <answer>...
ci() {
  local want=$1 name=$2 dir i=0 a
  shift 2
  case_n=$((case_n + 1))
  dir=$tmp/ci-$case_n
  mkdir "$dir"
  for a in "$@"; do
    i=$((i + 1))
    printf '%s\n' "$a" >"$dir/$i"
  done
  expect "$want" "ci: $name" env STUB_DIR="$dir" PATH="$tmp/bin:$PATH" RELEASE_CI_INTERVAL=0 \
    RELEASE_CI_TIMEOUT="${TIMEOUT:-5}" "$ci_guard" abc123 owner/repo
}
ci 0 "green first attempt" "$(run 7 completed success 1)"
ci 1 "red" "$(run 7 completed failure 1)"
ci 1 "cancelled" "$(run 7 completed cancelled 1)"
ci 1 "green rerun" "$(run 7 completed success 2)"
ci 1 "no run" "$none"
ci 0 "completes while waited for" "$(run 7 in_progress null 1)" "$(run 7 queued null 1)" "$(run 7 completed success 1)"
# A green run after the gap: without the disappearance check, the guard
# would wait it out and exit 0.
ci 1 "disappears while waited for" "$(run 7 in_progress null 1)" "$none" "$(run 7 completed success 1)"
TIMEOUT=0 ci 1 "never completes" "$(run 7 in_progress null 1)"
ci 4 "API error" FAIL
ci 4 "API error while waited for" "$(run 7 in_progress null 1)" FAIL
ci 4 "not JSON" "<html>rate limited</html>"
ci 4 "no workflow_runs" '{"message":"Not Found"}'
ci 4 "run without run_attempt" '{"workflow_runs":[{"id":7,"status":"completed","conclusion":"success"}]}'
# The query names the commit, the push event and main.
grep -qx 'api repos/owner/repo/actions/workflows/ci.yml/runs?head_sha=abc123&event=push&branch=main' "$tmp/ci-1/args" ||
  fail "ci: query was $(cat "$tmp/ci-1/args")"

[ "$status" -eq 0 ] && echo "release guards: OK"
exit "$status"
