#!/usr/bin/env bash
# Build the PyPI wheels and npm packages that wrap a GitHub Release.
#
#   tools/build_launchers.sh <tag> <assets-dir> <out-dir>
#
# <assets-dir> holds the eight Release assets of <tag>: for each of the four
# targets, `limae-<target>.tar.gz` and its sidecar `limae-<target>.sha256` (what
# `gh release download <tag> --pattern 'limae-*'` fetches). The packages
# land in <out-dir>/pypi (wheels) and <out-dir>/npm (tarballs), two directories
# because the PyPI upload action takes a directory and uploads all of it.
#
# The binaries are not built here. Each one is taken out of the Release
# tarball, after the tarball has been checked against its sidecar, and put
# into a wheel and an npm platform package as it is. "The same bytes as the
# Release" is therefore how the packages are constructed, and
# tools/check_launchers.sh is the gate that reads the finished packages back
# and asserts it — this script constructing them that way is not evidence
# that the files it wrote say so.
#
# The package version is the tag's, not the manifest's: the binaries are the
# tag's, and on a `workflow_dispatch` dry run the checkout is main while the
# assets belong to an older tag. The launcher sources (launchers/) and the
# metadata strings (description, license, repository, from `cargo metadata`)
# are the checkout's.
#
# Tooling: `wheel pack` writes the wheel and its RECORD, `npm pack` writes
# the npm tarballs; both are fetched on demand, nothing Python lives in this
# repository.
#
# Platform mapping, one row per Release target. Linux ships one static musl
# binary per architecture, which also runs on glibc systems, so it is
# published twice on PyPI (a manylinux wheel and a musllinux wheel with the
# same file in them — pip picks by the libc it detects, and a single wheel
# carrying both tags is a shape PyPI is not known to accept) and once on npm
# (`os`/`cpu` only, no `libc` field, so it is installed on both). The macOS
# floors are the targets' minimum supported versions (rustc platform
# support: 10.12 on x86_64, 11.0 on aarch64). No Windows binary is built, so
# no Windows package exists; a missing row is the honest reading there.

set -euo pipefail

fail() {
  printf 'build_launchers: %s\n' "$1" >&2
  exit 1
}

[[ $# -eq 3 ]] || fail 'usage: tools/build_launchers.sh <tag> <assets-dir> <out-dir>'
tag=$1
assets=$(cd "$2" && pwd)
out=$3
version=${tag#v}
[[ $version != "$tag" ]] || fail "tag '$tag' does not start with v"
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "tag '$tag' is not v<major>.<minor>.<patch>"

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/limae-launchers.XXXXXX")
trap 'rm -rf "$work"' EXIT
mkdir -p "$out/pypi" "$out/npm"
out=$(cd "$out" && pwd)

# Package metadata has one source, the Cargo manifest.
manifest=$(cd "$repo_root" && cargo metadata --no-deps --format-version 1 --locked |
  jq -ec '.packages[] | select(.name == "limae")')
description=$(jq -er '.description' <<<"$manifest")
license=$(jq -er '.license' <<<"$manifest")
repository=$(jq -er '.repository' <<<"$manifest")

# target | wheel platform tags (space separated, one wheel per group, groups
# separated by `,`) | npm platform | npm os | npm cpu
mapping='
x86_64-unknown-linux-musl|manylinux_2_17_x86_64 manylinux2014_x86_64,musllinux_1_2_x86_64|linux-x64|linux|x64
aarch64-unknown-linux-musl|manylinux_2_17_aarch64 manylinux2014_aarch64,musllinux_1_2_aarch64|linux-arm64|linux|arm64
x86_64-apple-darwin|macosx_10_12_x86_64|darwin-x64|darwin|x64
aarch64-apple-darwin|macosx_11_0_arm64|darwin-arm64|darwin|arm64
'

# Take one binary out of its Release tarball, after the tarball has passed
# its sidecar. `sha256sum -c` reads the sidecar's own file name, so it runs
# in the assets directory.
extract_binary() {
  local target=$1 dest=$2
  local archive="limae-$target.tar.gz" sidecar="limae-$target.sha256"
  [[ -f "$assets/$archive" ]] || fail "$assets/$archive is missing"
  [[ -f "$assets/$sidecar" ]] || fail "$assets/$sidecar is missing"
  (cd "$assets" && sha256sum -c --strict "$sidecar")
  mkdir -p "$dest"
  tar -xzf "$assets/$archive" -C "$dest" limae
  [[ -x "$dest/limae" ]] || fail "$archive did not contain an executable limae"
}

build_wheel() {
  local target=$1 tags=$2 binary=$3
  local dir="$work/wheel-$target-${tags%% *}"
  local distinfo="$dir/limae-$version.dist-info"
  mkdir -p "$distinfo/licenses" "$dir/limae-$version.data/scripts"
  cp -p "$binary" "$dir/limae-$version.data/scripts/limae"
  cp "$repo_root/LICENSE" "$distinfo/licenses/LICENSE"
  {
    printf 'Metadata-Version: 2.4\n'
    printf 'Name: limae\n'
    printf 'Version: %s\n' "$version"
    printf 'Summary: %s\n' "$description"
    printf 'License-Expression: %s\n' "$license"
    printf 'License-File: LICENSE\n'
    printf 'Project-URL: Repository, %s\n' "$repository"
    printf 'Classifier: Programming Language :: Rust\n'
    printf 'Description-Content-Type: text/markdown\n'
    printf '\n'
    cat "$repo_root/launchers/pypi/README.md"
  } >"$distinfo/METADATA"
  {
    printf 'Wheel-Version: 1.0\n'
    printf 'Generator: limae tools/build_launchers.sh\n'
    printf 'Root-Is-Purelib: false\n'
    local tag
    for tag in $tags; do
      printf 'Tag: py3-none-%s\n' "$tag"
    done
  } >"$distinfo/WHEEL"
  uvx --from 'wheel==0.45.1' wheel pack --dest-dir "$out/pypi" "$dir"
}

build_npm_platform() {
  local target=$1 platform=$2 os=$3 cpu=$4 binary=$5
  local dir="$work/npm-$platform"
  mkdir -p "$dir"
  cp -p "$binary" "$dir/limae"
  cp "$repo_root/LICENSE" "$repo_root/launchers/npm/platform/README.md" "$dir/"
  jq -n \
    --arg name "@limae/$platform" --arg version "$version" \
    --arg description "$description ($target binary; install the limae package, not this one)" \
    --arg license "$license" --arg repository "$repository" \
    --arg os "$os" --arg cpu "$cpu" '{
      name: $name, version: $version, description: $description,
      license: $license,
      repository: { type: "git", url: ("git+" + $repository + ".git") },
      os: [$os], cpu: [$cpu],
      publishConfig: { access: "public" },
      files: ["limae"]
    }' >"$dir/package.json"
  (cd "$dir" && npm pack --silent --pack-destination "$out/npm")
}

build_npm_main() {
  local dir="$work/npm-limae"
  cp -r "$repo_root/launchers/npm/limae" "$dir"
  cp "$repo_root/LICENSE" "$dir/"
  jq --arg version "$version" \
    '.version = $version | .optionalDependencies |= with_entries(.value = $version)' \
    "$repo_root/launchers/npm/limae/package.json" >"$dir/package.json"
  (cd "$dir" && npm pack --silent --pack-destination "$out/npm")
}

while IFS='|' read -r target tag_groups platform os cpu; do
  [[ -n $target ]] || continue
  extract_binary "$target" "$work/release-$target"
  binary="$work/release-$target/limae"
  IFS=',' read -ra groups <<<"$tag_groups"
  for tags in "${groups[@]}"; do
    build_wheel "$target" "$tags" "$binary"
  done
  build_npm_platform "$target" "$platform" "$os" "$cpu" "$binary"
done <<<"$mapping"
build_npm_main

printf 'built into %s:\n' "$out"
ls -1 "$out/pypi" "$out/npm"
