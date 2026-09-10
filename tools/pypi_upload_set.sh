#!/usr/bin/env bash
# Decide which wheels still have to go to PyPI, so that a publish job can be
# re-run after failing part-way through an upload.
#
#   tools/pypi_upload_set.sh <project> <version> <dir>
#
# PyPI never lets a file name be uploaded twice, so a re-run that uploads
# everything trips over the wheels that did land. The upload action's
# `skip-existing` would step over them, but it cannot tell "this file, already
# uploaded" from "a different file under this name". This script asks PyPI
# what it holds for <project> <version> and, for every wheel in <dir>:
#
#   not on PyPI                        -> kept, it is the one to upload
#   on PyPI with the same sha256       -> deleted from <dir>, already there
#   on PyPI with a different sha256    -> exit 1, nothing is uploaded
#
# A version PyPI does not know yet keeps every wheel. Any other answer from
# PyPI (an outage, a 5xx) is an exit 1: not knowing what is up there is not
# the same as knowing nothing is.
#
# Reads only the public JSON API — no credential is involved here.

set -euo pipefail
shopt -s nullglob

fail() {
  printf 'pypi_upload_set: %s\n' "$1" >&2
  exit 1
}

[[ $# -eq 3 ]] || fail 'usage: tools/pypi_upload_set.sh <project> <version> <dir>'
project=$1 version=$2 dir=$3
wheels=("$dir"/*.whl)
[[ ${#wheels[@]} -gt 0 ]] || fail "no wheels in $dir"

remote=$(mktemp "${TMPDIR:-/tmp}/limae-pypi.XXXXXX")
trap 'rm -f "$remote"' EXIT
status=$(curl -sS -o "$remote" -w '%{http_code}' "https://pypi.org/pypi/$project/$version/json")
case $status in
  404)
    printf '%s %s is not on PyPI; all %s wheels are to be uploaded\n' "$project" "$version" "${#wheels[@]}"
    exit 0
    ;;
  200) ;;
  *) fail "PyPI answered HTTP $status for $project $version" ;;
esac

for wheel in "${wheels[@]}"; do
  name=$(basename "$wheel")
  local_sha=$(sha256sum "$wheel" | cut -d' ' -f1)
  remote_sha=$(jq -r --arg n "$name" '.urls[] | select(.filename == $n) | .digests.sha256' "$remote")
  if [[ -z $remote_sha ]]; then
    printf '%s: not on PyPI, to be uploaded\n' "$name"
  elif [[ $remote_sha == "$local_sha" ]]; then
    printf '%s: already on PyPI with sha256 %s, dropped\n' "$name" "$local_sha"
    rm "$wheel"
  else
    fail "$name is on PyPI with sha256 $remote_sha, this build has $local_sha"
  fi
done
