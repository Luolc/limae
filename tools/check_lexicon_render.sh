#!/usr/bin/env bash
# The lexicon page that is committed must be the one the generator produces.
#
# `site/index.html` is a committed product of the `render-lexicon` Cargo
# example over `spec/lexicon/zh.toml`. Nothing else in the quality bar reads
# the page, so editing the source without regenerating it, or changing the
# generator and leaving the page behind, keeps every other gate green.
#
# What this gate does not do: it has one generator, so it cannot notice the
# generator drifting. Until 2026-09 there was a second, independent Python
# generator and the two were compared byte for byte; that comparison caught
# a port going wrong, and it went with the Python generator. The check
# against the committed page pins the current output, which is a different
# and weaker thing: an intended change to the template is committed together
# with the page it renders, and this gate has nothing to say about it. The
# template itself is compiled in by askama, so a field the template names
# that the data does not carry fails the build, not this gate.
#
# The comparison is run twice: once over the committed source, and once over a
# perturbed copy of it. The second run is the control arm. Agreement over one
# fixed input is also what a generator that ignored its input would show;
# a page that changed with its input is not.

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

# Render one source into `$work_dir/$label/site/index.html`.
render() {
  local label=$1 source=$2

  mkdir -p "$work_dir/$label/spec/lexicon"
  cp "$source" "$work_dir/$label/spec/lexicon/zh.toml"
  (
    cd "$work_dir/$label"
    "$binary"
  ) >"$work_dir/$label.log" 2>&1 ||
    { cat "$work_dir/$label.log" >&2; fail "$label: the generator failed"; }
}

page() {
  printf '%s\n' "$work_dir/$1/site/index.html"
}

# The committed source must render to the committed page.
render committed "$repo_root/spec/lexicon/zh.toml"
if ! cmp -s "$(page committed)" "$repo_root/site/index.html"; then
  cmp "$(page committed)" "$repo_root/site/index.html" >&2 || true
  fail 'site/index.html is out of date; rerun `cargo run --example render-lexicon`'
fi
printf '%s\n' 'committed source: site/index.html matches'

# The control arm. The title and subtitle are replaced, and one entry is
# appended, chosen to move the parts of the page that are easiest to get
# wrong: a tone-marked pinyin that has to sort ahead of every existing entry,
# characters HTML has to escape, and a back-quoted span that becomes a
# `<code>` element.
perturbed="$work_dir/perturbed.toml"
sed -e 's/^title = .*/title = "对照臂标题"/' \
  -e 's/^subtitle = .*/subtitle = "对照臂副标题"/' \
  "$repo_root/spec/lexicon/zh.toml" >"$perturbed"
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
render perturbed "$perturbed"
if cmp -s "$(page perturbed)" "$repo_root/site/index.html"; then
  fail 'the perturbed source rendered to the committed page; the comparison is vacuous'
fi
# A changed page is still what a generator that dropped `fault` would show:
# the other fields of the appended entry move the page on their own. So the
# field's rendering is asserted directly, on the escaped form the page must
# carry.
expected_fault='<p class="fault"><span class="label">病</span>对照臂的 <code>fault</code>：&#60;b&#62; &#38; &#34;quotes&#34; 也要走同一条转义。</p>'
grep -qF -- "$expected_fault" "$(page perturbed)" ||
  fail 'the perturbed source did not render its `fault` as the expected escaped HTML'
# The title and subtitle come from the source too; a generator that carried
# them as literals would still pass everything above.
for expected in '<title>对照臂标题</title>' '<h1>对照臂标题</h1>' \
  '<p class="subtitle">对照臂副标题</p>'; do
  grep -qF -- "$expected" "$(page perturbed)" ||
    fail "the perturbed source did not render its title or subtitle: $expected"
done
printf '%s\n' 'control arm: the page changed with the source, and `fault`, `title` and `subtitle` rendered as expected'
