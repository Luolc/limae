#!/usr/bin/env bash
# Repository maintenance contracts that no crate test can carry.
#
# Three promises in this repository are made to its own maintainers rather
# than to users of the crate, and each lives in a file that is not part of
# the package:
#
#   1. The `kind` table in `docs/knowledge/polish-hook-self-trial.md` lists
#      every kind the hook can write. The list comes from the `hook-kinds`
#      Cargo example, which is exhaustive by construction (see its header);
#      this gate only compares.
#   2. The `cargo-fmt` hook in `.pre-commit-config.yaml` wakes for every file
#      that changes what `cargo fmt --check` prints, and for nothing else.
#   3. The `limae-lexicon` hook in the same file wakes for every lexicon and
#      for no other TOML. It keys on the path rather than the extension so
#      that the build configuration is never fed to the Chinese typography
#      rules, and so that an `en.toml` arrives checked with no edit here.
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

# 1. Every kind the hook can write has a row in the handbook.
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

# 2 and 3. Two hooks' `files` patterns, applied the way pre-commit applies
# them: a search of the pattern against the repository-relative path.
precommit_config="$repo_root/.pre-commit-config.yaml"
[[ -f $precommit_config ]] || fail "$precommit_config is missing"

# Read one hook's `files` pattern out of the configuration.
#
# The block runs from the hook's `id` to whatever declaration comes next, and
# that end is spelled as "the next hook or repo" rather than as "the next
# `- repo:`. Every local hook here currently sits under a `- repo: local` of
# its own, which is the arrangement that makes the narrower end work; put two
# hooks under one `- repo:`, as anyone tidying this file might, and a range
# ending at `- repo:` runs past the first hook and comes back with both
# patterns. Measured 2026-09-10 with `cargo-fmt` and `limae` sharing a block:
# the range yields two lines, `grep -E` reads them as alternatives, and
# `README.md` is then reported as waking the format hook. So it fails rather
# than passing quietly — but it fails naming the wrong thing, which sends the
# next person to edit a `files` pattern that was never wrong. This is a
# sturdier reading of the same file, not a repair of a hole.
#
# An empty result is a failure and not an empty pattern: this is also the arm
# that speaks up if a hook is renamed or deleted, rather than going green
# because there is nothing left to assert about.
hook_files_pattern() {
  local hook=$1

  awk -v id="      - id: $hook" '
    $0 == id { found = 1; next }
    found && /^ *- (id|repo):/ { exit }
    found
  ' "$precommit_config" | sed -n 's/^ *files: //p'
}

pattern=$(hook_files_pattern cargo-fmt)
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

# 3. The lexicon hook takes the lexicon directory and nothing else.
pattern=$(hook_files_pattern limae-lexicon)
[[ -n $pattern ]] || fail 'the limae-lexicon hook has no files pattern'
# `en.toml` is the point of keying on the directory rather than on `zh.toml`:
# an English lexicon is coming, and it has to arrive checked.
for path in spec/lexicon/zh.toml spec/lexicon/en.toml; do
  printf '%s\n' "$path" | grep -qE -- "$pattern" ||
    fail "$path would not wake the limae-lexicon hook"
done
# The other half, and the reason the pattern is a path and not `\.toml$`:
# none of these is prose, and running this repository's Chinese typography
# rules over its own build configuration is not a thing anyone asked for.
for path in Cargo.toml rust-toolchain.toml askama.toml .pre-commit-config.yaml \
  spec/wordlists/zh-word-1.toml docs/tracker.md; do
  if printf '%s\n' "$path" | grep -qE -- "$pattern"; then
    fail "$path would wake the limae-lexicon hook"
  fi
done
printf '%s\n' 'limae-lexicon hook: wakes for every lexicon and for no other TOML'
