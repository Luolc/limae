import pathlib
import re
import sys

import pytest

from lo_md_lint import zh_format
from lo_md_lint.zh_format import check_file, fix_text

# Excerpt of a real review-guidelines doc that produced 19 false positives,
# all inside backticks.
INLINE_CODE_DOC = "\n".join(
    [
        "| 2.8 | `for x in seq` | 不要 `range(len)`、`adict.keys()` |",
        "- 库用 `logging.getLogger(__name__)`，**不要** `print`。",
        '时区：用 `zoneinfo` (`ZoneInfo("America/New_York")`)。',
        "`datetime.utcnow()` 已弃用，改 `datetime.now(timezone.utc)`。",
        "ruff：`DTZ001`–`DTZ006` (`datetime()` 无 tz、`.now()` 等)。",
        "",
    ]
)

FENCE_SRC = "你好,世界\n```\nfoo(bar)\nprint('你好,世界')\n```\n结尾(测试)\n"
FENCE_FIXED = (
    "你好，世界\n```\nfoo(bar)\nprint('你好,世界')\n```\n结尾 (测试)\n"
)


def kept(text: str, name: str):
  return pytest.param(text, text, id=name)


@pytest.mark.parametrize(
    ("src", "expected"),
    [
        # R1: half-width punctuation adjacent to CJK.
        pytest.param("你好,世界", "你好，世界", id="cjk-comma"),
        pytest.param("规则:如下", "规则：如下", id="cjk-colon"),
        pytest.param("完了;继续", "完了；继续", id="cjk-semicolon"),
        pytest.param("什么?", "什么？", id="cjk-question"),
        pytest.param("真的!", "真的！", id="cjk-exclamation"),
        pytest.param(
            "ACME,主动层", "ACME，主动层", id="latin-before-cjk-after"
        ),
        kept("a, b: c; d? e!", "english-punct-kept"),
        kept("$1,000", "thousands-separator-kept"),
        kept("https://example.com:8080", "url-port-kept"),
        # R2: full-width parentheses.
        pytest.param("（测试）", "(测试)", id="fullwidth-parens"),
        pytest.param(
            "术语（covered call）策略",
            "术语 (covered call) 策略",
            id="fullwidth-parens-get-spacing",
        ),
        # R3: spacing around half-width parentheses.
        pytest.param(
            "期权(covered call)策略",
            "期权 (covered call) 策略",
            id="paren-after-cjk",
        ),
        pytest.param("Foo(核心)", "Foo (核心)", id="paren-after-latin"),
        pytest.param("**粗体**(x)", "**粗体** (x)", id="paren-after-bold"),
        pytest.param("`code`(x)", "`code` (x)", id="paren-after-backtick"),
        kept("期权 (covered call) 策略", "already-spaced-kept"),
        # Fenced code blocks are exempt; prose around them is still fixed.
        pytest.param(FENCE_SRC, FENCE_FIXED, id="fenced-code-kept"),
        # Term-intrinsic parens like 401(k) are not split; spacing after
        # still applies.
        pytest.param("查 401(k)计划", "查 401(k) 计划", id="term-paren-kept"),
        pytest.param(
            "普通情况1,234(股)", "普通情况1,234 (股)", id="number-then-paren"
        ),
        # Inline code spans are exempt.
        kept(INLINE_CODE_DOC, "inline-code-doc-kept"),
        kept("用 `ln(K/F)` 计算", "code-span-paren-kept"),
        kept("见 `文件:行号` 定位", "code-span-colon-kept"),
        kept("报错 `你好,世界` 原样", "code-span-cjk-comma-kept"),
        kept("``含 ` 的 code(x)`` 后文", "double-backtick-span-kept"),
        pytest.param(
            "`a(1)` 与 `b:中` 之间,还有正文(x)",
            "`a(1)` 与 `b:中` 之间，还有正文 (x)",
            id="prose-between-spans",
        ),
        pytest.param(
            "`code`(x) 与 (`y`)后文",
            "`code` (x) 与 (`y`) 后文",
            id="spacing-outside-span",
        ),
        pytest.param(
            "未闭合`的反引号(x)",
            "未闭合`的反引号 (x)",
            id="unclosed-backtick-is-text",
        ),
        pytest.param(
            "`你好,世界\n函数(x)` 后文(y)\n",
            "`你好,世界\n函数(x)` 后文 (y)\n",
            id="span-across-line-break",
        ),
        pytest.param(
            "未闭合`的反引号(x)\n\n另一段`结束(y)\n",
            "未闭合`的反引号 (x)\n\n另一段`结束 (y)\n",
            id="span-not-across-blank-line",
        ),
        pytest.param(
            "# 标题`未闭合(x)\n正文`结束(y)\n",
            "# 标题`未闭合 (x)\n正文`结束 (y)\n",
            id="span-not-across-heading",
        ),
        pytest.param(
            "- a`(x)\n- b`(y)",
            "- a` (x)\n- b` (y)",
            id="span-not-across-list-items",
        ),
        pytest.param(
            "| a`(x) |\n| b`(y) |",
            "| a` (x) |\n| b` (y) |",
            id="span-not-across-table-rows",
        ),
        pytest.param(
            "- 见 `ln(K/F)\n  的公式` 后文(x)\n",
            "- 见 `ln(K/F)\n  的公式` 后文 (x)\n",
            id="span-continues-in-list-item",
        ),
        kept("> `你好,世界\n> 函数(x)`\n", "span-across-blockquote-lines-kept"),
    ],
)
def test_fix_text(src: str, expected: str):
  assert fix_text(src) == expected


@pytest.mark.parametrize(
    "sample",
    [
        pytest.param("你好,世界", id="cjk-punct"),
        pytest.param(
            "期权(covered call)策略（含**粗体**(x)与`code`(y)）",
            id="mixed-parens",
        ),
        pytest.param("ACME,主动层;Foo(核心)", id="mixed-list"),
    ],
)
def test_fix_is_idempotent(sample: str):
  once = fix_text(sample)
  assert fix_text(once) == once


def findings(tmp_path: pathlib.Path, text: str) -> list[tuple[int, str]]:
  """Return ``(line, rule)`` per problem reported for ``text``."""
  p = tmp_path / "t.md"
  p.write_text(text, encoding="utf-8")
  pattern = re.compile(rf"{re.escape(str(p))}:(\d+): \[(R\d)")
  out: list[tuple[int, str]] = []
  for problem in check_file(p):
    m = pattern.match(problem)
    assert m is not None, problem
    out.append((int(m.group(1)), m.group(2)))
  return out


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        pytest.param("你好,世界", [(1, "R1")], id="cjk-comma"),
        pytest.param("（测试）", [(1, "R2"), (1, "R2")], id="fullwidth-parens"),
        pytest.param(
            "期权(covered call)策略", [(1, "R3"), (1, "R3")], id="paren-spacing"
        ),
        pytest.param("干净的一行\n你好,世界\n", [(2, "R1")], id="line-number"),
        pytest.param(
            "规则：半角括号 (外侧留空格)，全角标点。\n", [], id="clean"
        ),
        pytest.param(
            "[链接](https://example.com) 后文", [], id="markdown-link"
        ),
        pytest.param(
            "正文 (合规)。\n```python\nwalk(p, out)\nprint('你好,世界')\n```\n",
            [],
            id="fenced-code",
        ),
        pytest.param(
            "某 401(k) pre-tax 与 403(b) 计划。\n", [], id="term-paren"
        ),
        pytest.param(INLINE_CODE_DOC, [], id="inline-code-doc"),
        pytest.param(
            "`code`(x) 与 (`y`)后文\n",
            [(1, "R3"), (1, "R3")],
            id="spacing-outside-span",
        ),
        pytest.param("未闭合`的反引号(x)", [(1, "R3")], id="unclosed-backtick"),
        pytest.param(
            "`你好,世界\n函数(x)` 后文(y)\n",
            [(2, "R3")],
            id="span-across-line-break",
        ),
        pytest.param(
            "# 标题`未闭合(x)\n正文`结束(y)\n",
            [(1, "R3"), (2, "R3")],
            id="span-not-across-heading",
        ),
        pytest.param(
            "> `你好,世界\n> 函数(x)`\n", [], id="span-across-blockquote-lines"
        ),
    ],
)
def test_check_file(
    tmp_path: pathlib.Path, text: str, expected: list[tuple[int, str]]
):
  assert findings(tmp_path, text) == expected


def test_cli_reports_then_fixes(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  monkeypatch.setattr(sys, "argv", ["lo-md-lint", str(p)])
  assert zh_format.main() == 1
  monkeypatch.setattr(sys, "argv", ["lo-md-lint", "--fix", str(p)])
  assert zh_format.main() == 0
  assert p.read_text(encoding="utf-8") == "你好，世界"


def test_tracked_markdown_lists_md_files():
  # Runs inside this repository.
  paths = zh_format.tracked_markdown()
  assert paths, "expected tracked markdown files in the repo"
  assert all(p.suffix == ".md" for p in paths)
