#!/usr/bin/env bash
# The two lexicon page generators must agree byte for byte, and the page they
# both produce must be the page that is committed.
#
# `site/index.html` is a committed product with two producers:
# `tools/render_lexicon.py` (the reference implementation) and the
# `render-lexicon` Cargo example (its Rust port). Nothing else in the quality
# bar notices when the two part ways, or when either moves and the committed
# page is left behind — every other gate keeps passing, because none of them
# reads the page.
#
# The comparison is run twice: once over the committed source, and once over a
# perturbed copy of it. The second run is the control arm. Agreement over one
# fixed input is also what two generators that both ignored their input would
# show; agreement over a changed input, plus a page that changed with it, is
# not.

set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/limae-lexicon.XXXXXX")

cleanup() {
  local original_status=$?

  trap - EXIT
  if ! find "$work_dir" -depth -delete; then
    printf 'lexicon render check failed: cannot clean temporary directory %s\n' \
      "$work_dir" >&2
    exit 1
  fi
  exit "$original_status"
}
trap cleanup EXIT

fail() {
  printf 'lexicon render check failed: %s\n' "$1" >&2
  exit 1
}

(cd "$repo_root" && cargo build --locked --example render-lexicon)
binary="$repo_root/target/debug/examples/render-lexicon"
[[ -x $binary ]] || fail 'the render-lexicon example was not built'

# Render one source with both generators into `$work_dir/$label/{python,rust}`.
render_both() {
  local label=$1 source=$2 arm

  for arm in python rust; do
    mkdir -p "$work_dir/$label/$arm/spec/lexicon"
    cp "$source" "$work_dir/$label/$arm/spec/lexicon/zh.toml"
  done
  (
    cd "$work_dir/$label/python"
    uv run --project "$repo_root" python "$repo_root/tools/render_lexicon.py"
  ) >"$work_dir/$label-python.log" 2>&1 ||
    { cat "$work_dir/$label-python.log" >&2; fail "$label: the Python generator failed"; }
  (
    cd "$work_dir/$label/rust"
    "$binary"
  ) >"$work_dir/$label-rust.log" 2>&1 ||
    { cat "$work_dir/$label-rust.log" >&2; fail "$label: the Rust generator failed"; }
}

page() {
  printf '%s\n' "$work_dir/$1/$2/site/index.html"
}

# The committed source: the two generators must agree, and the page they agree
# on must be the committed one.
render_both committed "$repo_root/spec/lexicon/zh.toml"
if ! cmp -s "$(page committed python)" "$(page committed rust)"; then
  cmp "$(page committed python)" "$(page committed rust)" >&2 || true
  fail 'the Python and Rust generators disagree about spec/lexicon/zh.toml'
fi
if ! cmp -s "$(page committed python)" "$repo_root/site/index.html"; then
  fail 'site/index.html is out of date; rerun tools/render_lexicon.py'
fi
printf '%s\n' 'committed source: both generators agree, and site/index.html matches'

# The control arm. One entry is appended, chosen to move the parts of the page
# that are easiest to port wrongly: a tone-marked pinyin that has to sort ahead
# of every existing entry, characters HTML has to escape, and a back-quoted
# span that becomes a `<code>` element.
perturbed="$work_dir/perturbed.toml"
cp "$repo_root/spec/lexicon/zh.toml" "$perturbed"
cat >>"$perturbed" <<'TOML'

[[entry]]
term = "对照臂"
pinyin = ["ǎn", "zhào", "bì"]
plain = "control arm & <b> 'quoted' \"both ways\""
gloss = "只在 `tools/check_lexicon_render.sh` 的对照臂里出现，不进 spec。"
fault = "对照臂的 `fault`：<b> & \"quotes\" 也要走同一条转义。"
examples = [
  { before = "a < b && c > d", after = "`code` 与 'quotes' 与 \"quotes\"" },
]
TOML
render_both perturbed "$perturbed"
if ! cmp -s "$(page perturbed python)" "$(page perturbed rust)"; then
  cmp "$(page perturbed python)" "$(page perturbed rust)" >&2 || true
  fail 'the Python and Rust generators disagree about the perturbed source'
fi
if cmp -s "$(page perturbed python)" "$repo_root/site/index.html"; then
  fail 'the perturbed source rendered to the committed page; the comparison is vacuous'
fi
printf '%s\n' 'control arm: both generators followed the changed source, and the page changed with it'
