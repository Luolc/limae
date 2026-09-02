"""Render the lexicon into one static page.

The page is the reading end of ``spec/lexicon/zh.toml``: same content,
laid out as a character primer rather than as data. Run it from the
repository root; it writes ``site/index.html``.
"""

import html
import pathlib
import sys
import tomllib
from typing import Any, cast
import unicodedata

SOURCE = pathlib.Path("spec/lexicon/zh.toml")
TARGET = pathlib.Path("site/index.html")

STYLE = """
:root {
  --paper: #f6f2e9;
  --ink: #24211c;
  --faded: #7a736a;
  --rule: #cfc5b4;
  --cinnabar: #a8433a;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 4rem 1.5rem 6rem;
  background: var(--paper);
  color: var(--ink);
  font-family: "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC",
      "SimSun", "STSong", serif;
  line-height: 1.9;
}
.page {
  max-width: 62rem; margin: 0 auto; display: grid; gap: 3rem;
  grid-template-columns: 11rem minmax(0, 1fr);
  align-items: start;
}
.sheet { max-width: 44rem; }
nav {
  position: sticky; top: 3rem; border-right: 1px solid var(--rule);
  padding-right: 1.2rem; font-size: .9rem;
}
nav .toc-label {
  color: var(--cinnabar); letter-spacing: .3rem; font-size: .75rem;
  margin-bottom: .9rem;
}
nav a {
  display: block; color: var(--ink); text-decoration: none;
  padding: .3rem 0; border-bottom: 1px solid transparent;
}
nav a:hover, nav a:focus-visible {
  color: var(--cinnabar); border-bottom-color: var(--rule);
}
nav .sound {
  font-family: Georgia, "Times New Roman", serif; font-size: .72rem;
  color: var(--faded); margin-left: .4rem;
}
@media (max-width: 52rem) {
  .page { grid-template-columns: 1fr; gap: 2rem; }
  nav {
    position: static; border-right: none;
    border-bottom: 1px solid var(--rule); padding: 0 0 1.2rem;
    columns: 2; column-gap: 1.5rem;
  }
}
:focus-visible { outline: 2px solid var(--cinnabar); outline-offset: 3px; }
h1 {
  font-size: 2.4rem; font-weight: normal; letter-spacing: .5rem;
  margin: 0 0 .4rem; text-align: center;
}
.subtitle {
  text-align: center; color: var(--faded); letter-spacing: .2rem;
  margin: 0 0 3rem; font-size: .95rem;
}
.preface {
  border-top: 1px solid var(--rule); border-bottom: 1px solid var(--rule);
  padding: 1.6rem 0; margin: 0 0 4rem; color: var(--faded); font-size: .95rem;
}
.preface p { margin: .4rem 0; }
.preface b { color: var(--ink); font-weight: normal; }
.entry { margin: 0 0 4.5rem; }
.cells { display: flex; flex-wrap: wrap; gap: .5rem; margin-bottom: 1.4rem; }
.cell { width: 4.6rem; text-align: center; }
.pinyin {
  font-family: Georgia, "Times New Roman", serif;
  font-size: .78rem; color: var(--faded); letter-spacing: .02rem;
  height: 1.5rem; display: flex; align-items: center;
  justify-content: center; position: relative; margin-bottom: .25rem;
}
/* 四线三格：拼音在字帖里写在这里，不是浮在字上面 */
.pinyin::before {
  content: ""; position: absolute; inset: 0;
  border-top: 1px solid var(--cinnabar);
  border-bottom: 1px solid var(--cinnabar);
  opacity: .3;
  background:
      linear-gradient(var(--cinnabar), var(--cinnabar)) 0 33.3% / 100% 1px
          no-repeat,
      linear-gradient(var(--cinnabar), var(--cinnabar)) 0 66.6% / 100% 1px
          no-repeat;
}
.pinyin span { position: relative; }
.grid {
  width: 4.6rem; height: 4.6rem; border: 1px solid var(--cinnabar);
  position: relative; display: flex; align-items: center;
  justify-content: center; background: rgba(255,255,255,.45);
}
.grid::before, .grid::after {
  content: ""; position: absolute; border-color: var(--cinnabar);
  opacity: .38;
}
.grid::before { left: 50%; top: 0; bottom: 0; border-left: 1px dashed; }
.grid::after { top: 50%; left: 0; right: 0; border-top: 1px dashed; }
/* 楷体是字帖里的那一种，宋体不是 */
.glyph {
  font-family: "Kaiti SC", "KaiTi", "STKaiti", "AR PL UKai CN",
      "Noto Serif CJK SC", serif;
  font-size: 2.9rem; line-height: 1; position: relative;
}
.plain { font-size: 1.35rem; margin: 0 0 .6rem; }
.plain .label { color: var(--cinnabar); margin-right: .8rem; font-size: 1rem; }
.gloss { margin: 0 0 1.4rem; color: var(--faded); }
.gloss .label { color: var(--cinnabar); margin-right: .8rem; }
.eg { border-left: 2px solid var(--rule); padding: .1rem 0 .1rem 1.1rem;
     margin: 0 0 1rem; }
.eg .before { color: var(--faded); }
.eg .after { color: var(--ink); }
.eg .mark { color: var(--cinnabar); margin-right: .6rem; }
code {
  font-family: ui-monospace, "SF Mono", Menlo, monospace;
  font-size: .88em; background: rgba(0,0,0,.045); padding: .05em .3em;
}
footer {
  margin-top: 5rem; padding-top: 1.4rem; border-top: 1px solid var(--rule);
  color: var(--faded); font-size: .85rem; text-align: center;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --paper: #191714; --ink: #e6e0d4; --faded: #9a9184;
    --rule: #3b362e; --cinnabar: #c2695d;
  }
  :root:not([data-theme="light"]) .grid { background: rgba(255,255,255,.03); }
  :root:not([data-theme="light"]) code { background: rgba(255,255,255,.07); }
}
:root[data-theme="dark"] {
  --paper: #191714; --ink: #e6e0d4; --faded: #9a9184;
  --rule: #3b362e; --cinnabar: #c2695d;
}
"""


def _sound(entry: dict[str, Any]) -> str:
  """Build the sort key of one entry.

  Args:
    entry: One lexicon entry.

  Returns:
    Its pinyin with the tone marks stripped, so 「按」 sorts under a.
  """
  joined = "".join(cast(list[str], entry["pinyin"]))
  return "".join(
      c
      for c in unicodedata.normalize("NFD", joined)
      if not unicodedata.combining(c)
  )


def _cells(term: str, pinyin: list[str]) -> str:
  """Lay one term out as one grid square per character.

  Args:
    term: The term itself.
    pinyin: One syllable per character of the term.

  Returns:
    The HTML of the row of squares.
  """
  out = []
  for i, ch in enumerate(term):
    sound = pinyin[i] if i < len(pinyin) else ""
    out.append(
        f'<div class="cell"><div class="pinyin">'
        f"<span>{html.escape(sound)}</span></div>"
        f'<div class="grid"><span class="glyph">{html.escape(ch)}</span>'
        "</div></div>"
    )
  return f'<div class="cells">{"".join(out)}</div>'


def _inline(text: str) -> str:
  """Escape one line and give back-quoted spans a code face.

  Args:
    text: The line as written in the source.

  Returns:
    The line as HTML.
  """
  parts = html.escape(text).split("`")
  return "".join(
      p if i % 2 == 0 else f"<code>{p}</code>" for i, p in enumerate(parts)
  )


def render(data: dict[str, Any]) -> str:
  """Build the whole page.

  Args:
    data: The parsed lexicon.

  Returns:
    The page as HTML.
  """
  entries = []
  ordered = sorted(data["entry"], key=_sound)
  for i, e in enumerate(ordered):
    egs = "".join(
        f'<div class="eg"><div class="before"><span class="mark">原</span>'
        f'{_inline(x["before"])}</div>'
        f'<div class="after"><span class="mark">改</span>'
        f'{_inline(x["after"])}</div></div>'
        for x in e["examples"]
    )
    entries.append(
        f'<section class="entry" id="w{i}">'
        f'{_cells(e["term"], e["pinyin"])}'
        f'<p class="plain"><span class="label">白</span>'
        f'{_inline(e["plain"])}</p>'
        f'<p class="gloss"><span class="label">解</span>'
        f'{_inline(e["gloss"])}</p>{egs}</section>'
    )
  return (
      f"<title>AI 文言</title><style>{STYLE}</style>"
      '<div class="page"><nav><div class="toc-label">目次</div>'
      + "".join(
          f'<a href="#w{i}">{html.escape(str(e["term"]))}'
          f'<span class="sound">{html.escape("".join(e["pinyin"]))}</span></a>'
          for i, e in enumerate(ordered)
      )
      + '</nav><div class="sheet"><h1>AI 文言</h1>'
      '<p class="subtitle">机器写的中文里，读得懂却没人这么说的词</p>'
      f'<div class="preface"><p><b>判据</b>　{_inline(str(data["standard"]))}'
      f'</p><p><b>门槛</b>　{_inline(str(data["threshold"]).strip())}</p></div>'
      f'{"".join(entries)}'
      "<footer>正文在 <code>spec/lexicon/zh.toml</code>，"
      "本页由 <code>tools/render_lexicon.py</code> 生成</footer></div></div>"
  )


def main() -> int:
  """Read the lexicon and write the page.

  Returns:
    Process exit code.
  """
  if not SOURCE.exists():
    _ = sys.stderr.write(f"not found: {SOURCE}; run from the root\n")
    return 1
  with SOURCE.open("rb") as f:
    data = tomllib.load(f)
  entries = cast(list[dict[str, Any]], data["entry"])
  TARGET.parent.mkdir(exist_ok=True)
  _ = TARGET.write_text(render(data), encoding="utf-8")
  _ = sys.stdout.write(f"{TARGET}: {len(entries)} entries\n")
  return 0


if __name__ == "__main__":
  raise SystemExit(main())
