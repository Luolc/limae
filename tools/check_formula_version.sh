#!/usr/bin/env bash
# Assert that the version Homebrew reads out of a formula is the release's.
#
#   tools/check_formula_version.sh v0.13.2 luolc/local/limae
#
# The rendered formula declares no `version` (tools/render_homebrew_formula.sh
# says why), so the version Homebrew serves a user is whatever it scans out of
# the download URL. That scan is not ours: change the tag naming or the asset
# path and it can come back with something else — and nothing downstream would
# notice, because a formula carrying the wrong version installs and audits
# exactly like one carrying the right one. `brew audit` passing and the
# version being right are two different claims.
#
# So this is a positive assertion on the value itself: ask Homebrew what it
# read, and require it to equal <tag> without the leading `v`. Anything that
# is not that value — a different version, an empty one, a formula brew
# cannot parse — stops the run.
set -euo pipefail

fail() {
  printf 'check_formula_version: %s\n' "$1" >&2
  exit 1
}

[[ $# -eq 2 ]] || fail 'usage: tools/check_formula_version.sh <tag> <formula>'
tag=$1
formula=$2

[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "\`$tag\` is not a v<major>.<minor>.<patch> tag"
expected=${tag#v}

info=$(brew info --json=v2 "$formula") || fail "brew info could not read $formula"

# The shape is `brew info --json=v2`: an object with `formulae` and `casks`
# arrays, each formula carrying `versions.stable` as a string (the per-formula
# half is what formulae.brew.sh serves, checked 2026-09-11; no Homebrew exists
# on the development machine, so the wrapper is not verified here). The shape
# moving fails this step rather than passing it, which is the direction that
# gets looked at.
#
# `jq -e` carries the shape checks: exactly one formula, and a stable version
# that is a non-empty string. A `null` reaching the comparison below would
# otherwise be just another value that is not $expected — true, but it would
# report the wrong thing.
actual=$(printf '%s' "$info" | jq -er '
  if (.formulae | length) != 1 then
    error("brew info named \(.formulae | length) formulae")
  elif (.formulae[0].versions.stable | type) != "string" or (.formulae[0].versions.stable | length) == 0 then
    error("brew info reported no stable version")
  else .formulae[0].versions.stable end
') || fail "brew info did not report one stable version for $formula"

[[ $actual == "$expected" ]] ||
  fail "$formula is version $actual, expected $expected (from $tag)"
printf 'check_formula_version: %s is version %s, as %s says it should be\n' "$formula" "$actual" "$tag"
