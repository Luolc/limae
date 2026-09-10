#!/usr/bin/env bash
# See docs/knowledge/rust-binary-distribution.md for the acceptance contract.

set -euo pipefail
shopt -s nullglob

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-precommit.XXXXXX")
# pre-commit itself is fetched by `uvx`, so this repository carries no
# Python project, lockfile or virtualenv for it.
pre_commit=(uvx pre-commit@4.2.0)

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'pre-commit consumer check failed: cannot clean temporary directory %s\n' "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'pre-commit consumer check failed: %s\n' "$1" >&2
  exit 1
}

fail_with_log() {
  cat "$1" >&2
  fail "$2"
}

run_pre_commit_expect() {
  local expected=$1
  local label=$2
  local log=$3
  shift 3
  local actual

  if (
    cd "$consumer"
    env PRE_COMMIT_HOME="$pre_commit_home" PATH="$poison_dir:$PATH" \
      "${pre_commit[@]}" "$@"
  ) >"$log" 2>&1; then
    actual=0
  else
    actual=$?
  fi
  printf '%s: exit %d\n' "$label" "$actual"
  if [[ $actual -ne $expected ]]; then
    fail_with_log "$log" "$label returned $actual, expected $expected"
  fi
}

write_consumer_config() {
  printf '%s\n' \
    'repos:' \
    "  - repo: $source_repo" \
    "    rev: $revision" \
    '    hooks:' \
    '      - id: limae' \
    '        alias: check' \
    '      - id: limae' \
    '        alias: fix' \
    '        name: Fix Markdown with limae' \
    '        args: [--fix]' \
    >"$consumer/.pre-commit-config.yaml"
}

revision=$(git -C "$repo_root" rev-parse --verify 'HEAD^{commit}')
source_repo="$work_dir/source.git"
consumer="$work_dir/consumer"
pre_commit_home="$work_dir/pre-commit-home"
poison_dir="$work_dir/poison"

git clone --bare --no-hardlinks "$repo_root" "$source_repo"
git -C "$source_repo" cat-file -e "$revision^{commit}"
git -C "$source_repo" update-ref refs/heads/precommit-test "$revision"

mkdir -p "$consumer" "$poison_dir"
# The entry-point name exists on PATH and does nothing but fail. Whatever
# the consumer ends up running, it is not this one, so it came out of
# pre-commit's own isolated install.
printf '%s\n' '#!/usr/bin/env bash' 'exit 97' >"$poison_dir/limae"
chmod +x "$poison_dir/limae"

git -C "$consumer" init -q
[[ ! -e $pre_commit_home ]] || fail 'isolated PRE_COMMIT_HOME was not initially absent'

write_consumer_config
printf '%s\n' '中A' >"$consumer/sample.md"
git -C "$consumer" add .

run_pre_commit_expect 0 'install' "$work_dir/install.log" install-hooks
rust_installs=("$pre_commit_home"/repo*/rustenv-*/bin/limae)
[[ ${#rust_installs[@]} -eq 1 && -x ${rust_installs[0]} ]] || \
  fail 'pre-commit did not install exactly one Rust binary in its cache'
printf '%s\n' 'install: isolated executable is present'

run_pre_commit_expect 1 'violation' "$work_dir/violation.log" run check --all-files
grep -Fq 'zh-typography-4' "$work_dir/violation.log" || \
  fail_with_log "$work_dir/violation.log" 'violation did not report zh-typography-4'
printf '%s\n' '中A' >"$work_dir/unfixed.expected"
cmp -s "$work_dir/unfixed.expected" "$consumer/sample.md" || \
  fail 'check changed the violating file'

run_pre_commit_expect 1 'fix' "$work_dir/fix.log" run fix --all-files
printf '%s\n' '中 A' >"$work_dir/fixed.expected"
cmp -s "$work_dir/fixed.expected" "$consumer/sample.md" || \
  fail_with_log "$work_dir/fix.log" 'fix did not write the expected Markdown'
printf '%s\n' 'fix: wrote expected Markdown'

run_pre_commit_expect 0 'clean' "$work_dir/clean.log" run check --all-files
cmp -s "$work_dir/fixed.expected" "$consumer/sample.md" || \
  fail 'clean check changed the fixed file'

printf 'pre-commit consumer check passed at revision %s\n' "$revision"
