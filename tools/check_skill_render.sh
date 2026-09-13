#!/usr/bin/env bash
# The committed skill is what its sources render.
#
# `skills/limae/` is a generated product that is committed — unlike
# `site/`, it is used as files out of a clone, so it has to be in the clone.
# Three sources feed it: `spec/skill/SKILL.md`, `spec/skill/zh.md` and
# `spec/lexicon/zh.toml`, through the `render-skill` Cargo example. A change
# to any of them that is not followed by a re-render leaves the product
# behind its source, and nothing at run time would notice: the skill is read
# by a model, not by a test.
#
# Two arms:
#   - the real sources, rendered here and now, must equal the committed
#     `skills/limae/` byte for byte;
#   - a perturbed copy of the sources must render something different, and
#     the differences must land in every one of the three files. Without
#     this arm, a generator that ignored its inputs and printed the committed
#     files would pass the first.
#
# Also asserted on the rendered `SKILL.md`: the front matter opens the file
# and its `name` equals the directory the skill lives in, which is the one
# rule of the agentskills.io specification a reader of the file cannot check
# by looking at the file alone.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-skill.XXXXXX")

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'skill render check failed: cannot clean temporary directory %s\n' \
      "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'skill render check failed: %s\n' "$1" >&2
  exit 1
}

(cd "$repo_root" && cargo build --locked --example render-skill)
binary="$repo_root/target/debug/examples/render-skill"
[[ -x $binary ]] || fail 'the render-skill example was not built'

committed="$repo_root/skills/limae"
[[ -d $committed ]] || fail "$committed is missing"

# Copy the sources into `$work_dir/$label/` and render there.
render() {
  local label=$1

  mkdir -p "$work_dir/$label/spec/skill" "$work_dir/$label/spec/lexicon"
  cp "$repo_root/spec/skill/SKILL.md" "$repo_root/spec/skill/zh.md" \
    "$work_dir/$label/spec/skill/"
  cp "$repo_root/spec/lexicon/zh.toml" "$work_dir/$label/spec/lexicon/"
  if [[ $label == perturbed ]]; then
    printf '\n%s\n' '对照臂：只在 tools/check_skill_render.sh 里出现。' \
      >>"$work_dir/$label/spec/skill/SKILL.md"
    printf '\n%s\n' '对照臂：只在 tools/check_skill_render.sh 里出现。' \
      >>"$work_dir/$label/spec/skill/zh.md"
    cat >>"$work_dir/$label/spec/lexicon/zh.toml" <<'TOML'

[[entry]]
term = "对照臂"
pinyin = ["duì", "zhào", "bì"]
plain = "control arm"
gloss = "只在 `tools/check_skill_render.sh` 的对照臂里出现，不进 spec。"
fault = "对照臂的 `fault`。"
examples = [
  { before = "原句。", after = "改句。" },
]
TOML
  fi
  (
    cd "$work_dir/$label"
    "$binary"
  ) >"$work_dir/$label.log" 2>&1 ||
    { cat "$work_dir/$label.log" >&2; fail "$label: the generator failed"; }
}

files=(SKILL.md references/zh/guide.md references/zh/lexicon.md)

# Positive arm: the real sources render exactly the committed product.
render current
for file in "${files[@]}"; do
  [[ -f "$work_dir/current/skills/limae/$file" ]] ||
    fail "the generator did not write $file"
done
if ! diff -r "$work_dir/current/skills/limae" "$committed" >"$work_dir/diff" 2>&1; then
  cat "$work_dir/diff" >&2
  fail 'skills/limae/ is behind its sources; run `cargo run --example render-skill` and commit the result'
fi
printf '%s\n' 'current: skills/limae/ equals what its sources render'

# The one specification rule a reader cannot check from the file alone.
head -n 1 "$committed/SKILL.md" | grep -qx -- '---' ||
  fail 'SKILL.md does not open with YAML front matter'
grep -qx -- 'name: limae' "$committed/SKILL.md" ||
  fail 'SKILL.md front matter `name` is not the directory name `limae`'
printf '%s\n' 'front matter: opens the file, and `name` equals the directory'

# Control arm: changed sources must change every file.
render perturbed
for file in "${files[@]}"; do
  if cmp -s "$work_dir/perturbed/skills/limae/$file" "$committed/$file"; then
    fail "the perturbed sources rendered the same $file as the committed one; the comparison is vacuous"
  fi
done
printf '%s\n' 'control arm: every file changed with its source'
