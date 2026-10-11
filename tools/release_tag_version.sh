#!/usr/bin/env bash
# Release guard: the tag must be `v` + the `limae` version in the manifest.
# `cargo publish` uploads whatever `[package].version` says, and the tag is
# not an input to it: v0.13.1 pushed at a commit whose manifest reads 0.14.0
# would publish 0.14.0, a version crates.io cannot delete, under a Release
# called v0.13.1.
# Usage: release_tag_version.sh TAG [MANIFEST]   (MANIFEST defaults to Cargo.toml)
# Exit 0 when they match, 1 when they differ, 4 when no version can be read.
set -uo pipefail
case $# in 1 | 2) ;; *) echo "usage: release_tag_version.sh TAG [MANIFEST]" >&2; exit 2 ;; esac
tag=$1
manifest=${2:-Cargo.toml}
if ! version=$(cargo metadata --no-deps --format-version 1 --locked --manifest-path "$manifest" |
  jq -er '.packages[] | select(.name == "limae") | .version'); then
  echo "cannot read the limae version from $manifest" >&2
  exit 4
fi
echo "tag=$tag manifest=v$version"
if [ "$tag" != "v$version" ]; then
  echo "tag $tag is not v$version" >&2
  exit 1
fi
