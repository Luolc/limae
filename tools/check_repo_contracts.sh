#!/usr/bin/env bash
# Repository maintenance contracts that no crate test can carry.
#
# Three promises in this repository are made to its own maintainers rather
# than to users of the crate, and each lives in a file that is not part of
# the package:
#
#   1. The Codex `Stop` hook in `.codex/config.toml` fails open until `limae`
#      is built, and hands the binary's exit code straight back once it is.
#      That command is what a contributor's own editor runs after every
#      reply; a broken `test -x` guard makes an unbuilt checkout complain on
#      every turn.
#   2. The `kind` table in `docs/knowledge/polish-hook-self-trial.md` lists
#      every kind the hook can write. The list comes from the `hook-kinds`
#      Cargo example, which is exhaustive by construction (see its header);
#      this gate only compares.
#   3. The `cargo-fmt` hook in `.pre-commit-config.yaml` wakes for every file
#      that changes what `cargo fmt --check` prints, and for nothing else.
#
# They are here and not in `rust/tests/` because a test that reads these
# files either ships them in the crate (they are not crate resources) or
# skips when they are absent, and a gate that skips in the package is green
# there while guarding nothing. This script runs from the checkout and never
# conditionally: a missing file is a failure.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-contracts.XXXXXX")

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'repository contracts check failed: cannot clean temporary directory %s\n' \
      "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'repository contracts check failed: %s\n' "$1" >&2
  exit 1
}

# 1. The Codex Stop hook, run as Codex runs it.
codex_config="$repo_root/.codex/config.toml"
[[ -f $codex_config ]] || fail "$codex_config is missing"
command_count=$(grep -c "^command = '" "$codex_config") ||
  fail 'no Stop hook command in .codex/config.toml'
[[ $command_count -eq 1 ]] ||
  fail "expected one hook command in .codex/config.toml, found $command_count"
hook_command=$(sed -n "s/^command = '\(.*\)'\$/\1/p" "$codex_config")
[[ -n $hook_command ]] || fail 'the Stop hook command could not be read'

# The command asks Git where the checkout is; this stub answers with a
# directory that has no `target/debug/limae` in it yet. The stub comes
# first, so `git` is this one and `sh` is the machine's.
checkout="$work_dir/checkout"
stub_bin="$work_dir/bin"
mkdir -p "$checkout" "$stub_bin"
printf '%s\n' '#!/bin/sh' "echo '$checkout'" >"$stub_bin/git"
chmod +x "$stub_bin/git"
stub_path="$stub_bin:$PATH"

if env -i PATH="$stub_path" sh -c "$hook_command" >"$work_dir/unbuilt.log" 2>&1; then
  unbuilt_status=0
else
  unbuilt_status=$?
fi
[[ $unbuilt_status -eq 0 ]] || {
  cat "$work_dir/unbuilt.log" >&2
  fail "the Stop hook exited $unbuilt_status with no binary built; it must fail open"
}
[[ ! -s "$work_dir/unbuilt.log" ]] || {
  cat "$work_dir/unbuilt.log" >&2
  fail 'the Stop hook printed something with no binary built'
}
printf '%s\n' 'Stop hook: silent and exit 0 with no binary built'

# Now build one. Anything it says is the host's answer, exit code included,
# so the guard must not be swallowing that either.
mkdir -p "$checkout/target/debug"
marker="$work_dir/ran"
printf '%s\n' '#!/bin/sh' "touch '$marker'" 'exit 7' >"$checkout/target/debug/limae"
chmod +x "$checkout/target/debug/limae"
if env -i PATH="$stub_path" sh -c "$hook_command" >"$work_dir/built.log" 2>&1; then
  built_status=0
else
  built_status=$?
fi
[[ $built_status -eq 7 ]] ||
  fail "the Stop hook exited $built_status with a binary that exits 7; it must pass the code through"
[[ -f $marker ]] || fail 'the Stop hook never reached the binary'
printf '%s\n' 'Stop hook: the binary ran and its exit code came through'

# 2. Every kind the hook can write has a row in the handbook.
handbook="$repo_root/docs/knowledge/polish-hook-self-trial.md"
[[ -f $handbook ]] || fail "$handbook is missing"
(cd "$repo_root" && cargo build --locked --example hook-kinds)
kinds_binary="$repo_root/target/debug/examples/hook-kinds"
[[ -x $kinds_binary ]] || fail 'the hook-kinds example was not built'
"$kinds_binary" >"$work_dir/kinds"
[[ -s "$work_dir/kinds" ]] || fail 'hook-kinds listed no kinds'
# A table row is `| `name` | ...`; this takes the name of every such row in
# the file, so the handbook's other tables are a superset that does no harm.
sed -n 's/^| `\([^`]*\)` |.*/\1/p' "$handbook" >"$work_dir/rows"
missing=$(grep -vxF -f "$work_dir/rows" "$work_dir/kinds" || true)
[[ -z $missing ]] || {
  printf '%s\n' "$missing" >&2
  fail 'the handbook kind table is missing the kinds above'
}
printf 'handbook: all %s kinds have a row\n' "$(wc -l <"$work_dir/kinds")"

# 3. The cargo-fmt hook's `files` pattern, applied the way pre-commit applies
# it: a search of the pattern against the repository-relative path.
precommit_config="$repo_root/.pre-commit-config.yaml"
[[ -f $precommit_config ]] || fail "$precommit_config is missing"
pattern=$(sed -n '/^      - id: cargo-fmt$/,/^  - repo:/p' "$precommit_config" |
  sed -n 's/^ *files: //p')
[[ -n $pattern ]] || fail 'the cargo-fmt hook has no files pattern'
# `cargo fmt --check` reads more than the files it prints about: Cargo.toml
# fixes the targets and the edition, a rustfmt.toml would carry the style,
# and rust-toolchain.toml decides which rustfmt runs.
for path in rust/main.rs Cargo.toml rustfmt.toml .rustfmt.toml rust-toolchain.toml; do
  printf '%s\n' "$path" | grep -qE -- "$pattern" ||
    fail "$path would not wake the cargo-fmt hook"
done
# The other half: without this arm a pattern matching everything passes the
# arm above, and every commit in the repository pays for a cargo run.
for path in README.md docs/tracker.md .pre-commit-config.yaml spec/rules.md; do
  if printf '%s\n' "$path" | grep -qE -- "$pattern"; then
    fail "$path would wake the cargo-fmt hook"
  fi
done
printf '%s\n' 'cargo-fmt hook: wakes for the Rust inputs and for nothing else'
