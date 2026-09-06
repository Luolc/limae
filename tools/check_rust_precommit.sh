#!/usr/bin/env bash
# See docs/knowledge/rust-binary-distribution.md for the acceptance contract.

set -euo pipefail
shopt -s nullglob

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-precommit.XXXXXX")
pre_commit="$repo_root/.venv/bin/pre-commit"

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'pre-commit check failed: cannot clean temporary directory %s\n' "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'pre-commit check failed: %s\n' "$1" >&2
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
      "$pre_commit" "$@"
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

[[ -x $pre_commit ]] || fail 'run uv sync before the pre-commit distribution check'
revision=$(git -C "$repo_root" rev-parse --verify 'HEAD^{commit}')
source_repo="$work_dir/source.git"
consumer="$work_dir/consumer"
pre_commit_home="$work_dir/pre-commit-home"
poison_dir="$work_dir/poison"

git clone --bare --no-hardlinks "$repo_root" "$source_repo"
git -C "$source_repo" cat-file -e "$revision^{commit}"
git -C "$source_repo" update-ref refs/heads/precommit-test "$revision"

mkdir -p "$consumer" "$poison_dir"
printf '%s\n' '#!/usr/bin/env bash' 'exit 97' >"$poison_dir/limae-rs"
printf '%s\n' '#!/usr/bin/env bash' 'exit 98' >"$poison_dir/limae"
chmod +x "$poison_dir/limae-rs" "$poison_dir/limae"

git -C "$consumer" init -q
printf '%s\n' \
  'repos:' \
  "  - repo: $source_repo" \
  "    rev: $revision" \
  '    hooks:' \
  '      - id: limae-rs' \
  '        alias: rust-check' \
  '        files: ^rust\.md$' \
  '      - id: limae-rs' \
  '        alias: rust-fix' \
  '        name: Fix Markdown with limae (Rust)' \
  '        args: [--fix]' \
  '        files: ^rust\.md$' \
  '      - id: limae' \
  '        alias: python-check' \
  '        files: ^python\.md$' \
  '      - id: limae' \
  '        alias: python-fix' \
  '        name: Fix Markdown with limae (Python)' \
  '        args: [--fix]' \
  '        files: ^python\.md$' \
  >"$consumer/.pre-commit-config.yaml"
printf '%s\n' '中A' >"$consumer/rust.md"
printf '%s\n' '中A' >"$consumer/python.md"
git -C "$consumer" add .

[[ ! -e $pre_commit_home ]] || fail 'isolated PRE_COMMIT_HOME was not initially absent'
run_pre_commit_expect 0 install "$work_dir/install.log" install-hooks

rust_installs=("$pre_commit_home"/repo*/rustenv-*/bin/limae-rs)
python_installs=("$pre_commit_home"/repo*/py_env-*/bin/limae)
[[ ${#rust_installs[@]} -eq 1 && -x ${rust_installs[0]} ]] || \
  fail 'pre-commit did not install exactly one Rust binary in its cache'
[[ ${#python_installs[@]} -eq 1 && -x ${python_installs[0]} ]] || \
  fail 'pre-commit did not install exactly one Python binary in its cache'
printf '%s\n' 'install: isolated Rust and Python executables are present'

for engine in rust python; do
  run_pre_commit_expect 1 "$engine violation" "$work_dir/$engine-violation.log" \
    run "$engine-check" --all-files
  grep -Fq 'zh-typography-4' "$work_dir/$engine-violation.log" || \
    fail_with_log "$work_dir/$engine-violation.log" \
      "$engine violation did not report zh-typography-4"
  printf '%s\n' '中A' >"$work_dir/$engine-unfixed.expected"
  cmp -s "$work_dir/$engine-unfixed.expected" "$consumer/$engine.md" || \
    fail "$engine check changed the violating file"

  run_pre_commit_expect 1 "$engine fix" "$work_dir/$engine-fix.log" \
    run "$engine-fix" --all-files
  printf '%s\n' '中 A' >"$work_dir/$engine-fixed.expected"
  cmp -s "$work_dir/$engine-fixed.expected" "$consumer/$engine.md" || \
    fail_with_log "$work_dir/$engine-fix.log" \
      "$engine fix did not write the expected Markdown"
  printf '%s\n' "$engine fix: wrote expected Markdown"

  run_pre_commit_expect 0 "$engine clean" "$work_dir/$engine-clean.log" \
    run "$engine-check" --all-files
  cmp -s "$work_dir/$engine-fixed.expected" "$consumer/$engine.md" || \
    fail "$engine clean check changed the fixed file"
done

printf 'pre-commit consumer check passed at revision %s\n' "$revision"
