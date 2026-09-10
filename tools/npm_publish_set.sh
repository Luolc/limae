#!/usr/bin/env bash
# Publish the npm tarballs in <dir>, platform packages first and the `limae`
# launcher last, skipping the ones npm already holds byte for byte.
#
#   tools/npm_publish_set.sh <dir> [--dry-run]
#
# npm refuses a second publish of the same name@version, so a publish job
# re-run after failing part-way would trip over the packages that did land.
# Before each publish this asks the registry for `dist.integrity` of that
# name@version and compares it with the sha512 of the local tarball (which
# is what the registry computes over the bytes it received):
#
#   not on npm                          -> published
#   on npm with the same integrity      -> skipped
#   on npm with a different integrity   -> exit 1, before publishing anything
#
# Every tarball is compared before the first publish, so a mismatch stops
# the whole set rather than half of it. `--dry-run` goes to `npm publish`
# verbatim; the registry lookups still run.
#
# Credentials: `npm publish` reads them from the npm configuration
# (`NODE_AUTH_TOKEN` via actions/setup-node); nothing here touches them.

set -euo pipefail
shopt -s nullglob

fail() {
  printf 'npm_publish_set: %s\n' "$1" >&2
  exit 1
}

[[ $# -ge 1 ]] || fail 'usage: tools/npm_publish_set.sh <dir> [--dry-run]'
dir=$1
shift

# `@limae/<platform>` first, `limae` last: the launcher's optionalDependencies
# must resolve the moment it lands.
platform_tarballs=("$dir"/limae-*-*.tgz)
launcher_tarballs=("$dir"/limae-[0-9]*.tgz)
[[ ${#platform_tarballs[@]} -gt 0 ]] || fail "no platform tarballs in $dir"
[[ ${#launcher_tarballs[@]} -eq 1 ]] || fail "expected one launcher tarball in $dir, found ${#launcher_tarballs[@]}"
tarballs=("${platform_tarballs[@]}" "${launcher_tarballs[@]}")

to_publish=()
for tarball in "${tarballs[@]}"; do
  spec=$(tar -xzOf "$tarball" package/package.json | jq -er '"\(.name)@\(.version)"')
  local_integrity="sha512-$(openssl dgst -sha512 -binary "$tarball" | base64 -w0)"
  # `npm view` exits non-zero for a package npm has never seen, and prints
  # nothing for a version it does not have; both mean "not on npm".
  remote_integrity=$(npm view "$spec" dist.integrity 2>/dev/null || true)
  if [[ -z $remote_integrity ]]; then
    printf '%s: not on npm, to be published\n' "$spec"
    to_publish+=("$tarball")
  elif [[ $remote_integrity == "$local_integrity" ]]; then
    printf '%s: already on npm with the same integrity, skipped\n' "$spec"
  else
    fail "$spec is on npm with $remote_integrity, this build has $local_integrity"
  fi
done

for tarball in "${to_publish[@]}"; do
  npm publish "$@" "$tarball"
done
printf 'npm_publish_set: %s published, %s skipped\n' "${#to_publish[@]}" "$((${#tarballs[@]} - ${#to_publish[@]}))"
