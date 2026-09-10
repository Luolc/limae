#!/usr/bin/env bash
# The generator still turns spec/lexicon/zh.toml into a page, and the page
# still follows its source.
#
# `site/index.html` is not committed. Since 2026-09 the workflow in
# .github/workflows/ci.yml renders it from `spec/lexicon/zh.toml` with the
# `render-lexicon` Cargo example on every run and publishes it to GitHub
# Pages from main. There is therefore no committed page to fall behind its
# source, and the arm that compared the two went with it: the failure it
# named ("the page was not regenerated") no longer exists.
#
# What this gate does guard, all of it on freshly rendered output:
#   - the generator runs to completion over the real lexicon;
#   - the page follows its input rather than carrying a fixed body, asserted
#     by rendering a perturbed copy of the source and requiring a different
#     page;
#   - `fault`, `title` and `subtitle` reach the page in the escaped form the
#     template owes them, asserted on the literal HTML.
#
# What it does not guard, and nothing else does either:
#   - whether the page is *right*. There is one generator, so a change to
#     `rust/templates/lexicon.html` that renders a worse page renders it
#     consistently, and this gate has nothing to say about it. Until 2026-09
#     there was a second, independent Python generator and the two were
#     compared byte for byte; that comparison caught a port going wrong, and
#     it went with the Python generator. A template change still has to be
#     read by a person.
#   - whether the published site matches this repository. That is the deploy
#     job's business, and a green deploy is not evidence about the domain.
#   - a field the template names that the data does not carry: askama
#     compiles the template in, so that fails the build, not this gate.
#
# The perturbation is the control arm. Rendering without crashing is also
# what a generator that ignored its input would do; a page that changed with
# its input is not.

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

# The baseline the control arm is measured against: the real lexicon,
# rendered here and now. Nothing is compared to it yet; `render` fails if the
# generator does not finish. The label is `baseline`, not `committed` — the
# lexicon is committed but the page is not, and a diagnostic that says
# "committed page" would name something this repository no longer has.
render baseline "$repo_root/spec/lexicon/zh.toml"
[[ -s $(page baseline) ]] || fail 'the real lexicon rendered an empty page'
printf '%s\n' 'baseline: the generator rendered a page from the real lexicon'

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
if cmp -s "$(page perturbed)" "$(page baseline)"; then
  fail 'the perturbed source rendered the same page as the baseline; the comparison is vacuous'
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
