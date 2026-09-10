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

write_consumer_config() {
  local engine=$1
  local hook_id=$2
  local display_name=$3

  printf '%s\n' \
    'repos:' \
    "  - repo: $source_repo" \
    "    rev: $revision" \
    '    hooks:' \
    "      - id: $hook_id" \
    "        alias: $engine-check" \
    "      - id: $hook_id" \
    "        alias: $engine-fix" \
    "        name: Fix Markdown with limae ($display_name)" \
    '        args: [--fix]' \
    >"$consumer/.pre-commit-config.yaml"
}

[[ -x $pre_commit ]] || fail 'run uv sync first'
revision=$(git -C "$repo_root" rev-parse --verify 'HEAD^{commit}')
source_repo="$work_dir/source.git"
consumer="$work_dir/consumer"
pre_commit_home="$work_dir/pre-commit-home"
poison_dir="$work_dir/poison"

git clone --bare --no-hardlinks "$repo_root" "$source_repo"
git -C "$source_repo" cat-file -e "$revision^{commit}"
git -C "$source_repo" update-ref refs/heads/precommit-test "$revision"

mkdir -p "$consumer" "$poison_dir"
# Both entry-point names exist on PATH and do nothing but fail. Whatever
# each arm ends up running, it is not one of these, so it came out of
# pre-commit's own isolated install.
printf '%s\n' '#!/usr/bin/env bash' 'exit 97' >"$poison_dir/limae"
printf '%s\n' '#!/usr/bin/env bash' 'exit 98' >"$poison_dir/limae-python"
chmod +x "$poison_dir/limae" "$poison_dir/limae-python"

git -C "$consumer" init -q
[[ ! -e $pre_commit_home ]] || fail 'isolated PRE_COMMIT_HOME was not initially absent'

# `rust` is the default id a consumer gets today; `python` is the rollback
# path, the deprecated reference implementation under its own id.
for engine in rust python; do
  if [[ $engine == rust ]]; then
    hook_id=limae
    display_name=Rust
  else
    hook_id=limae-python
    display_name=Python
  fi
  write_consumer_config "$engine" "$hook_id" "$display_name"
  printf '%s\n' '中A' >"$consumer/sample.md"
  git -C "$consumer" add .

  run_pre_commit_expect 0 "$engine install" "$work_dir/$engine-install.log" install-hooks
  if [[ $engine == rust ]]; then
    rust_installs=("$pre_commit_home"/repo*/rustenv-*/bin/limae)
    [[ ${#rust_installs[@]} -eq 1 && -x ${rust_installs[0]} ]] || \
      fail 'pre-commit did not install exactly one Rust binary in its cache'
  else
    python_installs=("$pre_commit_home"/repo*/py_env-*/bin/limae-python)
    [[ ${#python_installs[@]} -eq 1 && -x ${python_installs[0]} ]] || \
      fail 'pre-commit did not install exactly one Python binary in its cache'
  fi
  printf '%s\n' "$engine install: isolated executable is present"

  run_pre_commit_expect 1 "$engine violation" "$work_dir/$engine-violation.log" \
    run "$engine-check" --all-files
  grep -Fq 'zh-typography-4' "$work_dir/$engine-violation.log" || \
    fail_with_log "$work_dir/$engine-violation.log" \
      "$engine violation did not report zh-typography-4"
  printf '%s\n' '中A' >"$work_dir/$engine-unfixed.expected"
  cmp -s "$work_dir/$engine-unfixed.expected" "$consumer/sample.md" || \
    fail "$engine check changed the violating file"

  run_pre_commit_expect 1 "$engine fix" "$work_dir/$engine-fix.log" \
    run "$engine-fix" --all-files
  printf '%s\n' '中 A' >"$work_dir/$engine-fixed.expected"
  cmp -s "$work_dir/$engine-fixed.expected" "$consumer/sample.md" || \
    fail_with_log "$work_dir/$engine-fix.log" \
      "$engine fix did not write the expected Markdown"
  printf '%s\n' "$engine fix: wrote expected Markdown"

  run_pre_commit_expect 0 "$engine clean" "$work_dir/$engine-clean.log" \
    run "$engine-check" --all-files
  cmp -s "$work_dir/$engine-fixed.expected" "$consumer/sample.md" || \
    fail "$engine clean check changed the fixed file"
done

printf '%s\n' 'install: isolated Rust and Python executables are present'

printf 'pre-commit consumer check passed at revision %s\n' "$revision"
