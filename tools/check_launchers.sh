#!/usr/bin/env bash
# The invariant behind the PyPI and npm launchers: the binary a user installs
# from either registry is byte-for-byte the one attached to the GitHub
# Release. This gate reads it off the finished packages.
#
#   tools/check_launchers.sh <assets-dir> <out-dir>
#
# For each of the four targets it compares two values:
#
#   release  sha256 of `limae` extracted here, from the Release tarball in
#            <assets-dir>, after that tarball has passed its own `.sha256`
#            sidecar (so this value is chained to what the Release page
#            publishes, not to whatever happened to be on disk);
#   package  sha256 of `limae` extracted here, from each wheel and npm
#            tarball under <out-dir> that belongs to the target — the package
#            file itself, unpacked with the same tools an installer uses,
#            not the staging directory it was built from.
#
# Both values are printed on every line, and the job is red on the first
# mismatch. "Both sides were built by us" is not evidence of anything: it
# reads the same for "the same bytes" and for "two builds that both happen
# to run". This comparison is the evidence; a byte changed inside a package
# turns it red (the control arm recorded in the pull request that added it).
#
# The count of packages per target is asserted too. A gate that loops over
# whatever it finds passes on an empty directory.

set -euo pipefail
shopt -s nullglob

fail() {
  printf 'check_launchers: %s\n' "$1" >&2
  exit 1
}

[[ $# -eq 2 ]] || fail 'usage: tools/check_launchers.sh <assets-dir> <out-dir>'
assets=$(cd "$1" && pwd)
out=$(cd "$2" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/limae-check-launchers.XXXXXX")
trap 'rm -rf "$work"' EXIT

# target | wheel platform-tag substrings | npm platform. The wheel column
# lists one distinguishing substring per wheel the target must have (the
# same rows as tools/build_launchers.sh).
mapping='
x86_64-unknown-linux-musl|manylinux_2_17_x86_64 musllinux_1_2_x86_64|linux-x64
aarch64-unknown-linux-musl|manylinux_2_17_aarch64 musllinux_1_2_aarch64|linux-arm64
x86_64-apple-darwin|macosx_10_12_x86_64|darwin-x64
aarch64-apple-darwin|macosx_11_0_arm64|darwin-arm64
'

sha256_of() {
  sha256sum "$1" | cut -d' ' -f1
}

compare() {
  local target=$1 package=$2 expected=$3 actual=$4
  printf '%s\n  release %s\n  package %s  %s\n' "$target" "$expected" "$actual" "$(basename "$package")"
  [[ $actual == "$expected" ]] || fail "$(basename "$package") does not carry the Release binary"
}

checked=0
while IFS='|' read -r target wheel_tags platform; do
  [[ -n $target ]] || continue

  archive="limae-$target.tar.gz" sidecar="limae-$target.sha256"
  [[ -f "$assets/$archive" && -f "$assets/$sidecar" ]] || fail "$target: Release assets missing in $assets"
  (cd "$assets" && sha256sum -c --strict --quiet "$sidecar")
  mkdir -p "$work/release-$target"
  tar -xzf "$assets/$archive" -C "$work/release-$target" limae
  release=$(sha256_of "$work/release-$target/limae")

  for tag in $wheel_tags; do
    wheels=("$out"/pypi/limae-*-py3-none-*"$tag"*.whl)
    [[ ${#wheels[@]} -eq 1 ]] ||
      fail "$target: expected exactly one wheel tagged $tag in $out, found ${#wheels[@]}"
    dir="$work/wheel-$tag"
    uvx --from 'wheel==0.45.1' wheel unpack --dest "$dir" "${wheels[0]}" >/dev/null
    binaries=("$dir"/limae-*/limae-*.data/scripts/limae)
    [[ ${#binaries[@]} -eq 1 ]] ||
      fail "$(basename "${wheels[0]}") does not contain exactly one .data/scripts/limae"
    compare "$target" "${wheels[0]}" "$release" "$(sha256_of "${binaries[0]}")"
    checked=$((checked + 1))
  done

  tarballs=("$out"/npm/limae-"$platform"-*.tgz)
  [[ ${#tarballs[@]} -eq 1 ]] ||
    fail "$target: expected exactly one npm tarball for $platform in $out, found ${#tarballs[@]}"
  dir="$work/npm-$platform"
  mkdir -p "$dir"
  tar -xzf "${tarballs[0]}" -C "$dir" package/limae
  # npm keeps the mode; the launcher execs this file directly, so a binary
  # that arrived without its x bit would install fine and fail at first use.
  [[ -x "$dir/package/limae" ]] || fail "$(basename "${tarballs[0]}") carries limae without the executable bit"
  compare "$target" "${tarballs[0]}" "$release" "$(sha256_of "$dir/package/limae")"
  checked=$((checked + 1))
done <<<"$mapping"

[[ $checked -eq 10 ]] || fail "expected 10 package comparisons, made $checked"
printf 'check_launchers: %s packages carry the Release binary of their target\n' "$checked"
