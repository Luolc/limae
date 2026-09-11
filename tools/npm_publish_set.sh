#!/usr/bin/env bash
# Publish the npm tarballs in <dir>, platform packages first and the
# `@limae/cli` launcher last, skipping the ones npm already holds byte for
# byte.
#
#   tools/npm_publish_set.sh <dir> [--dry-run]
#
# npm refuses a second publish of the same name@version, so a publish job
# re-run after failing part-way would trip over the packages that did land.
# Before each publish this asks the registry for `dist.integrity` of that
# name@version and compares it with the sha512 of the local tarball (which
# is what the registry computes over the bytes it received):
#
#   not on npm (the registry said 404)  -> published
#   on npm with the same integrity      -> skipped
#   on npm with a different integrity   -> exit 1, before publishing anything
#   any other answer (unreachable, 5xx) -> exit 1, before publishing anything
#
# Every tarball is compared before the first publish, so a mismatch stops
# the whole set rather than half of it. Only a 404 counts as "not on npm":
# a lookup that failed for any other reason says nothing about what is
# there, and treating it as absence would publish on top of a package this
# script never managed to look at. `--dry-run` goes to `npm publish`
# verbatim; the registry lookups still run.
#
# Credentials: `npm publish` reads them from the npm configuration
# (`NODE_AUTH_TOKEN` via actions/setup-node); nothing here touches them.

set -euo pipefail
shopt -s nullglob

work=$(mktemp -d "${TMPDIR:-/tmp}/limae-npm-publish.XXXXXX")
trap 'rm -rf "$work"' EXIT

fail() {
  printf 'npm_publish_set: %s\n' "$1" >&2
  exit 1
}

[[ $# -ge 1 ]] || fail 'usage: tools/npm_publish_set.sh <dir> [--dry-run]'
dir=$1
shift

# `@limae/<platform>` first, `@limae/cli` last: the launcher's
# optionalDependencies must resolve the moment it lands.
#
# The sort is by the `name` inside each tarball, not by its file name. `npm
# pack` derives the file name from the package name, so `@limae/cli` packs as
# `limae-cli-<version>.tgz` — a file name a `limae-*-*.tgz` glob reads as a
# platform package, leaving no launcher at all. The name is what the registry
# publishes under; the file name was only ever a proxy for it.
launcher_name='@limae/cli' # launchers/npm/cli/package.json

platform_tarballs=()
launcher_tarballs=()
for tarball in "$dir"/*.tgz; do
  name=$(tar -xzOf "$tarball" package/package.json | jq -er '.name') ||
    fail "cannot read the package name out of $tarball"
  if [[ $name == "$launcher_name" ]]; then
    launcher_tarballs+=("$tarball")
  else
    platform_tarballs+=("$tarball")
  fi
done
[[ ${#platform_tarballs[@]} -gt 0 ]] || fail "no platform tarballs in $dir"
[[ ${#launcher_tarballs[@]} -eq 1 ]] || fail "expected one $launcher_name tarball in $dir, found ${#launcher_tarballs[@]}"
tarballs=("${platform_tarballs[@]}" "${launcher_tarballs[@]}")

to_publish=()
for tarball in "${tarballs[@]}"; do
  spec=$(tar -xzOf "$tarball" package/package.json | jq -er '"\(.name)@\(.version)"')
  local_integrity="sha512-$(openssl dgst -sha512 -binary "$tarball" | base64 -w0)"
  # With `--json`, a lookup that fails writes `{"error": {"code": …}}` to
  # stdout; E404 is the one answer that means "not there", everything else
  # (ECONNREFUSED, E5xx, an auth error) is a lookup that did not happen.
  if npm view "$spec" dist.integrity --json >"$work/view" 2>"$work/view.err"; then
    remote_integrity=$([[ -s "$work/view" ]] && jq -r '.' "$work/view" || true)
  else
    code=$(jq -r '.error.code // empty' "$work/view" 2>/dev/null || true)
    if [[ $code != E404 ]]; then
      cat "$work/view.err" >&2
      fail "npm view $spec failed with ${code:-no error code}; nothing published"
    fi
    remote_integrity=""
  fi
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
