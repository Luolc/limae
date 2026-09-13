#!/usr/bin/env bash
# The committed skill is what its sources render.
#
# `skills/write-naturally/` is a generated product that is committed — unlike
# `site/`, it is used as files out of a clone, so it has to be in the clone.
# Three sources feed it: `spec/skill/SKILL.md`, `spec/skill/zh.md` and
# `spec/lexicon/zh.toml`, through the `render-skill` Cargo example. A change
# to any of them that is not followed by a re-render leaves the product
# behind its source, and nothing at run time would notice: the skill is read
# by a model, not by a test.
#
# Four arms:
#   - the real sources, rendered here and now, must equal the committed
#     `skills/write-naturally/` byte for byte;
#   - three split arms, one per source: a copy of the sources with that one
#     source perturbed must change exactly the product that source feeds and
#     leave the other two equal to the committed files. Perturbing all three
#     at once and asking for "something changed" would pass a generator that
#     wired the body to the guide's file, or fed one source to every product;
#     one source at a time is what pins the file map down.
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

committed="$repo_root/skills/write-naturally"
[[ -d $committed ]] || fail "$committed is missing"

# Copy the sources into `$work_dir/$label/`, perturb the one named by
# `$2` (`body`, `guide`, `lexicon`, or nothing), and render there.
render() {
  local label=$1 perturb=${2:-}

  mkdir -p "$work_dir/$label/spec/skill" "$work_dir/$label/spec/lexicon"
  cp "$repo_root/spec/skill/SKILL.md" "$repo_root/spec/skill/zh.md" \
    "$work_dir/$label/spec/skill/"
  cp "$repo_root/spec/lexicon/zh.toml" "$work_dir/$label/spec/lexicon/"
  case "$perturb" in
    body)
      printf '\n%s\n' 'Control arm: only in tools/check_skill_render.sh.' \
        >>"$work_dir/$label/spec/skill/SKILL.md"
      ;;
    guide)
      printf '\n%s\n' '对照臂：只在 tools/check_skill_render.sh 里出现。' \
        >>"$work_dir/$label/spec/skill/zh.md"
      ;;
    lexicon)
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
      ;;
    '') ;;
    *) fail "unknown perturbation $perturb" ;;
  esac
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
  [[ -f "$work_dir/current/skills/write-naturally/$file" ]] ||
    fail "the generator did not write $file"
done
if ! diff -r "$work_dir/current/skills/write-naturally" "$committed" >"$work_dir/diff" 2>&1; then
  cat "$work_dir/diff" >&2
  fail 'skills/write-naturally/ is behind its sources; run `cargo run --example render-skill` and commit the result'
fi
printf '%s\n' 'current: skills/write-naturally/ equals what its sources render'

# The one specification rule a reader cannot check from the file alone.
head -n 1 "$committed/SKILL.md" | grep -qx -- '---' ||
  fail 'SKILL.md does not open with YAML front matter'
grep -qx -- 'name: write-naturally' "$committed/SKILL.md" ||
  fail 'SKILL.md front matter `name` is not the directory name `write-naturally`'
printf '%s\n' 'front matter: opens the file, and `name` equals the directory'

# The generator refuses a front matter that reads as valid YAML but is
# not a plain scalar: `description: ""` is two characters to a byte count
# and an empty value to a validator. A generator that renders it would
# leave every other arm here green.
mkdir -p "$work_dir/empty-description/spec/skill" "$work_dir/empty-description/spec/lexicon"
sed 's/^description: .*$/description: ""/' "$repo_root/spec/skill/SKILL.md" \
  >"$work_dir/empty-description/spec/skill/SKILL.md"
grep -qx 'description: ""' "$work_dir/empty-description/spec/skill/SKILL.md" ||
  fail 'the empty-description arm did not rewrite the source'
cp "$repo_root/spec/skill/zh.md" "$work_dir/empty-description/spec/skill/"
cp "$repo_root/spec/lexicon/zh.toml" "$work_dir/empty-description/spec/lexicon/"
if (cd "$work_dir/empty-description" && "$binary") >"$work_dir/empty-description.log" 2>&1; then
  fail 'the generator accepted `description: ""`'
fi
printf '%s\n' 'front matter: `description: ""` is refused by the generator'

# Split arms: each source moves its own product and nothing else.
split_arm() {
  local source=$1 moved=$2 file

  render "$source" "$source"
  for file in "${files[@]}"; do
    if [[ $file == "$moved" ]]; then
      if cmp -s "$work_dir/$source/skills/write-naturally/$file" "$committed/$file"; then
        fail "perturbing the $source source left $file unchanged; the comparison is vacuous"
      fi
    elif ! cmp -s "$work_dir/$source/skills/write-naturally/$file" "$committed/$file"; then
      fail "perturbing the $source source changed $file, which it does not feed"
    fi
  done
  printf 'split arm: the %s source moved %s and nothing else\n' "$source" "$moved"
}
split_arm body SKILL.md
split_arm guide references/zh/guide.md
split_arm lexicon references/zh/lexicon.md
