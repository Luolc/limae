#!/usr/bin/env bash
# The lexicon linter must report the TOML line the prose actually sits on.
#
# `lint-lexicon` hands each field of `spec/lexicon/*.toml` to the library
# pipeline, which reports lines *within that field*. Turning those back into
# lines of the file is the whole contract, and it is not one the committed
# lexicon can check: `spec/lexicon/zh.toml` is clean, so the gate that runs
# the tool over it prints `OK` whether the mapping is right, wrong, or gone.
# Delete the newline `"""` swallows, or the fallback below, and every other
# gate stays green while the line numbers quietly drift.
#
# This gate is not a Rust `#[cfg(test)]` block in the example, which is the
# obvious place to reach for. `cargo test --locked` builds example targets
# but does not run their tests: an always-failing `#[test]` added to
# `rust/tools/lint_lexicon.rs` leaves `cargo test --locked` at exit 0 and
# never names the test, and only `--all-targets` turns it red (2026-09-10,
# measured both ways on this checkout). A regression test there would be a
# test that cannot fail, which is worse than none — it looks like a gate, so
# nobody looks again.
#
# The arms are fixtures with fixed line numbers rather than a perturbed copy
# of the real lexicon. A number taken from `spec/lexicon/zh.toml` moves every
# time an entry is added, and a gate that goes red for an unrelated edit gets
# its expectations rewritten until it means nothing.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-lexlint.XXXXXX")

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'lexicon lint check failed: cannot clean temporary directory %s\n' \
      "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'lexicon lint check failed: %s\n' "$1" >&2
  exit 1
}

(cd "$repo_root" && cargo build --locked --bin limae --example lint-lexicon)
linter="$repo_root/target/debug/examples/lint-lexicon"
[[ -x $linter ]] || fail 'the lint-lexicon example was not built'

# Every fixture below is checked from this directory, and this file is the
# arm for the other half of "only typography": the tool passes its rule
# selection to `resolve` explicitly, which is what stops `limae.toml`
# discovery. Should that ever become a plain `CliOverrides::default()`, the
# walk up from here finds this file, the experimental families come back on,
# and the lexical arm below goes red instead of drifting quietly.
printf '%s\n' 'enable_experimental = true' >"$work_dir/limae.toml"

# Run the linter over one fixture, capturing its output and exit code with
# the file name reduced to its basename so the expectations can be literal.
run() {
  local fixture=$1

  if (cd "$work_dir" && "$linter" "$fixture") >"$work_dir/$fixture.log" 2>&1; then
    status=0
  else
    status=$?
  fi
}

# The control arm, and it has to come first: every arm below asserts that
# something is reported, and a linter that reported everything would pass all
# of them. The committed lexicon is the input that must come back clean.
if (cd "$repo_root" && "$linter" spec/lexicon/zh.toml) >"$work_dir/committed.log" 2>&1; then
  printf '%s\n' 'committed lexicon: accepted, exit 0'
else
  cat "$work_dir/committed.log" >&2
  fail 'the committed spec/lexicon/zh.toml was rejected'
fi

# 1. One violation in each shape a field can take, and the line each sits on
# is written into the fixture rather than counted by the tool. A top-level
# single-line string (1), an array element (5), the interior of a `"""`
# string (10), and — a hundred characters and a second entry later — a
# nested field (28) and an element of a nested array (31). The last two are
# what a "which field is this" counter gets wrong: it would still be right
# about line 1.
cat >"$work_dir/lines.toml" <<'TOML'
title = "标题甲,乙"
subtitle = "副标题"
preface = [
  "引子第一段。",
  "引子第二段甲,乙。",
]
standard = "判据。"
threshold = """
门槛第一行。
门槛第二行甲,乙。
门槛第三行。
"""

[[entry]]
term = "词"
pinyin = ["cí"]
plain = "白话"
gloss = "解释。"
fault = "病因。"
examples = [
  { before = "病句。", after = "改句。" },
]

[[entry]]
term = "词二"
pinyin = ["cí", "èr"]
plain = "白话二"
gloss = "解释二甲,乙。"
fault = "病因二。"
examples = [
  { before = "病句二甲,乙。", after = "改句二。" },
]
TOML
cat >"$work_dir/lines.expected" <<'EXPECTED'
lines.toml:1: error: [zh-typography-1 halfwidth punct next to CJK] …标题甲,乙…
lines.toml:5: error: [zh-typography-1 halfwidth punct next to CJK] …引子第二段甲,乙。…
lines.toml:10: error: [zh-typography-1 halfwidth punct next to CJK] …门槛第二行甲,乙。…
lines.toml:28: error: [zh-typography-1 halfwidth punct next to CJK] …解释二甲,乙。…
lines.toml:31: error: [zh-typography-1 halfwidth punct next to CJK] …病句二甲,乙。…

5 error(s), 0 warning(s) in the lexicon prose.
EXPECTED
# The fixture is the source of truth for its own numbering, so it is read
# back rather than trusted: an edit that shifts a line has to move the
# expectation with it, and this says so instead of failing further down.
for line in 1 5 10 28 31; do
  sed -n "${line}p" "$work_dir/lines.toml" | grep -qF -- '甲,乙' ||
    fail "the fixture no longer carries the violation on line $line"
done
run lines.toml
[[ $status -eq 1 ]] || fail "the line fixture exited $status, expected 1"
diff -u "$work_dir/lines.expected" "$work_dir/lines.toml.log" >&2 ||
  fail 'the reported TOML lines are not the lines the prose sits on'
printf '%s\n' 'line mapping: single-line, array, `"""` interior and nested fields all reported at their own line'

# 2. A value whose lines are not the file's lines. `\n` inside a basic string
# makes the pipeline report a second line for a field that occupies one line
# of the file, and the offset arithmetic alone would answer 12 — a real line,
# holding a different field, which is why a wrong answer here would read as a
# plausible one. The mapping confirms its candidate against the source before
# using it and falls back to the field's own line.
cat >"$work_dir/escape.toml" <<'TOML'
title = "标题"
subtitle = "副标题"
preface = ["引子。"]
standard = "判据。"
threshold = "门槛。"

[[entry]]
term = "词"
pinyin = ["cí"]
plain = "白话"
gloss = "第一行。\n第二行甲,乙。"
fault = "病因。"
examples = [
  { before = "病句。", after = "改句。" },
]
TOML
run escape.toml
[[ $status -eq 1 ]] || fail "the escape fixture exited $status, expected 1"
grep -qF -- 'escape.toml:11: error:' "$work_dir/escape.toml.log" ||
  fail 'an escaped newline was not reported at the field it belongs to'
if grep -qF -- 'escape.toml:12:' "$work_dir/escape.toml.log"; then
  fail 'an escaped newline was reported at the following line, which holds another field'
fi
printf '%s\n' 'line mapping: a value the source cannot match falls back to the field line'

# 3. The lexical families stay off. `examples[].before` holds the specimens
# each entry exists to document, so they must not be checked for the very
# words they collect.
cat >"$work_dir/lexical.toml" <<'TOML'
title = "标题"
subtitle = "副标题"
preface = ["引子。"]
standard = "判据。"
threshold = "门槛。"

[[entry]]
term = "词"
pinyin = ["cí"]
plain = "白话"
gloss = "一套命名不是一开始设计好的，而是随着时间逐步积累成今天这样。"
fault = "病因。"
examples = [
  { before = "正本在仓内 `.agents/skills/`。", after = "源文件在仓内 `.agents/skills/`。" },
]
TOML
run lexical.toml
# The assertion is on the output, not on the exit status. The lexical
# families are experimental, and an experimental rule's default severity is
# `warning`, which this tool prints but does not fail on — so an exit code of
# 0 here would be the same answer whether they were off or merely quiet, and
# a switch that turned them back on would slip through unremarked (measured
# 2026-09-10: with rule selection changed to discover `limae.toml`, an
# exit-status check stayed green while the finding was printed).
printf '%s\n' 'OK: 1 lexicon file(s) clean' >"$work_dir/lexical.expected"
[[ $status -eq 0 ]] || {
  cat "$work_dir/lexical.toml.log" >&2
  fail 'the lexical fixture was rejected'
}
diff -u "$work_dir/lexical.expected" "$work_dir/lexical.toml.log" >&2 ||
  fail 'a lexical-rule specimen was reported; the experimental families are on'
# Silence proves nothing on its own: an unread field is just as quiet. The
# control arm runs the same sentence through `limae` with the experimental
# families switched on, and it has to speak.
control="$work_dir/control"
mkdir -p "$control"
sed -n 's/^gloss = "\(.*\)"$/\1/p' "$work_dir/lexical.toml" >"$control/prose.md"
[[ -s "$control/prose.md" ]] || fail 'the control arm could not take the sentence from the fixture'
printf '%s\n' 'enable_experimental = true' >"$control/limae.toml"
(cd "$control" && "$repo_root/target/debug/limae" prose.md) >"$control/log" 2>&1 || true
grep -q 'zh-tell-2' "$control/log" || {
  cat "$control/log" >&2
  fail 'the control arm did not report the sentence with the experimental families on'
}
printf '%s\n' 'lexical families: silent here, and the control arm shows the same text does trip them'
