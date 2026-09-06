#!/usr/bin/env bash
# See docs/knowledge/rust-binary-distribution.md for the acceptance contract.

set -euo pipefail
shopt -s nullglob

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-package.XXXXXX")

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'package check failed: cannot clean temporary directory %s\n' "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'package check failed: %s\n' "$1" >&2
  exit 1
}

fail_with_log() {
  cat "$1" >&2
  fail "$2"
}

run_business_smoke() {
  local binary=$1
  local label=$2
  local run_dir="$work_dir/run-$label"

  mkdir -p "$run_dir/home"
  printf '%s\n' '[tool.limae]' 'enable_experimental = true' >"$run_dir/pyproject.toml"
  printf '%s\n' 'This release is a testament.' >"$run_dir/experimental.md"
  printf '%s\n' '秘钥 key' >"$run_dir/term.md"

  if ! (
    cd "$run_dir"
    env -i HOME="$run_dir/home" PATH= "$binary" experimental.md \
      >experimental.log 2>&1
  ); then
    fail_with_log "$run_dir/experimental.log" "$label experimental rule run failed"
  fi
  if ! grep -Fq '[en-tell-1 ' "$run_dir/experimental.log"; then
    fail_with_log \
      "$run_dir/experimental.log" "$label experimental finding is missing en-tell-1"
  fi

  if ! (
    cd "$run_dir"
    env -i HOME="$run_dir/home" PATH= "$binary" --fix term.md >fix.log 2>&1
  ); then
    fail_with_log "$run_dir/fix.log" "$label terminology fix run failed"
  fi
  printf '%s\n' '密钥 key' >"$run_dir/term.expected"
  if ! cmp -s "$run_dir/term.expected" "$run_dir/term.md"; then
    fail_with_log "$run_dir/fix.log" "$label did not apply the embedded terminology fix"
  fi
}

check_static_elf() {
  local binary=$1
  local output=$2

  readelf -l "$binary" >"$output" 2>&1 || return 1
  grep -Fq 'Program Headers' "$output" || return 1
  if grep -Fq 'INTERP' "$output"; then
    return 1
  fi
}

check_package() {
  local cargo_target="$work_dir/package-target"
  local consumer_target="$work_dir/package-consumer-target"
  local archives package_roots archive package_root missing_root

  (
    cd "$repo_root"
    CARGO_TARGET_DIR="$cargo_target" cargo package --locked --allow-dirty
  )

  archives=("$cargo_target"/package/*.crate)
  [[ ${#archives[@]} -eq 1 ]] || fail "expected exactly one Cargo archive"
  archive=${archives[0]}
  mkdir "$work_dir/unpacked"
  tar -xzf "$archive" -C "$work_dir/unpacked"
  package_roots=("$work_dir"/unpacked/*)
  [[ ${#package_roots[@]} -eq 1 && -d ${package_roots[0]} ]] || \
    fail "expected exactly one unpacked Cargo package"
  package_root=${package_roots[0]}

  for required in \
    rust/main.rs \
    rust/examples/diff_probe.rs \
    rust/tests/integration.rs \
    spec/fixtures/clean.in \
    spec/wordlists/zh-word-1.toml; do
    [[ -f "$package_root/$required" ]] || fail "Cargo archive omitted $required"
  done

  (
    cd "$package_root"
    CARGO_TARGET_DIR="$consumer_target" cargo test --locked
  )
  printf '%s\n' 'Cargo package: declared targets and packaged fixtures passed their tests'

  cp -a "$package_root" "$work_dir/missing-wordlist"
  missing_root="$work_dir/missing-wordlist"
  mv "$missing_root/spec/wordlists/zh-word-1.toml" "$work_dir/removed-wordlist"
  if (
    cd "$missing_root"
    CARGO_TARGET_DIR="$consumer_target" cargo build --locked --release --bin limae-rs \
      >"$work_dir/missing-build.log" 2>&1
  ); then
    fail "package without the embedded terminology wordlist still built"
  fi
  if ! grep -Fq 'spec/wordlists/zh-word-1.toml' "$work_dir/missing-build.log"; then
    fail_with_log \
      "$work_dir/missing-build.log" 'missing-wordlist build failed for an unrelated reason'
  fi
  printf '%s\n' 'missing-wordlist control: rejected by the embedded resource build'

  (
    cd "$package_root"
    CARGO_TARGET_DIR="$consumer_target" cargo install \
      --path . --locked --root "$work_dir/install"
  )
  mkdir "$work_dir/standalone"
  mv "$work_dir/install/bin/limae-rs" "$work_dir/standalone/limae-rs"
  mv "$package_root" "$work_dir/source-unavailable"
  [[ ! -e "$package_root" ]] || fail 'unpacked source directory is still available'
  run_business_smoke "$work_dir/standalone/limae-rs" package
  printf '%s\n' 'Cargo package: unpacked, installed, and ran without source resources'
}

check_target() {
  local target=$1
  local cargo_target="$work_dir/target-$target"
  local binary

  case "$target" in
    x86_64-unknown-linux-gnu | x86_64-unknown-linux-musl) ;;
    *) fail "unsupported target $target" ;;
  esac

  (
    cd "$repo_root"
    CARGO_TARGET_DIR="$cargo_target" cargo build \
      --locked --release --target "$target" --bin limae-rs
  )
  binary="$cargo_target/$target/release/limae-rs"
  [[ -x "$binary" ]] || fail "$target binary is not executable"
  run_business_smoke "$binary" "$target"
  printf '%s\n' "$target: business smoke passed"

  command -v readelf >/dev/null || fail 'readelf is required for target checks'
  if [[ $target == x86_64-unknown-linux-gnu ]]; then
    if ! readelf -l "$binary" >"$work_dir/gnu-readelf.log" 2>&1; then
      fail_with_log "$work_dir/gnu-readelf.log" 'GNU control is not a valid ELF executable'
    fi
    if ! grep -Fq 'Program Headers' "$work_dir/gnu-readelf.log"; then
      fail_with_log "$work_dir/gnu-readelf.log" 'GNU control has no Program Headers'
    fi
    if ! grep -Fq 'INTERP' "$work_dir/gnu-readelf.log"; then
      fail_with_log "$work_dir/gnu-readelf.log" 'GNU control is not dynamically linked'
    fi
    if check_static_elf "$binary" "$work_dir/gnu-static-check.log"; then
      fail_with_log \
        "$work_dir/gnu-static-check.log" 'static ELF gate accepted the dynamic GNU control'
    fi
    printf '%s\n' 'static ELF gate: dynamic GNU control rejected'
  else
    if ! check_static_elf "$binary" "$work_dir/musl-readelf.log"; then
      fail_with_log \
        "$work_dir/musl-readelf.log" 'musl artifact is not a valid static ELF executable'
    fi
    printf '%s\n' '#!/bin/sh' 'exit 0' >"$work_dir/not-elf"
    chmod +x "$work_dir/not-elf"
    if check_static_elf "$work_dir/not-elf" "$work_dir/not-elf-readelf.log"; then
      fail "static ELF gate accepted an executable shell script"
    fi
    printf '%s\n' 'musl ELF gate: real artifact accepted and shell control rejected'
  fi
}

case "${1:-}" in
  package)
    [[ $# -eq 1 ]] || fail 'usage: check_rust_package.sh package'
    check_package
    ;;
  target)
    [[ $# -eq 2 ]] || fail 'usage: check_rust_package.sh target TARGET'
    check_target "$2"
    ;;
  *)
    fail 'usage: check_rust_package.sh package | target TARGET'
    ;;
esac
