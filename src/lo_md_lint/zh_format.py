"""Chinese Markdown formatting checker.

``spec/rules.md`` is the normative, language-agnostic rule spec and
``spec/fixtures/`` the golden set every implementation runs against; this
module is the Python reference implementation of both.

Rules:
  R1: No half-width , ; : ? ! adjacent to a CJK character.
  R2: No full-width parentheses; use half-width ( ) instead.
  R3: Half-width parens need an outside space when adjacent to a word
    character, a closing paren, a bold marker, or an inline-code backtick.
    Parens inside an English token (``word(s)``, ``401(k)``) and Markdown
    link syntax ``](...)`` are exempt.

Fenced code blocks and inline code spans (CommonMark backtick runs, which
may cross line breaks inside a paragraph but not block boundaries) are
exempt from every rule and never rewritten by ``--fix``.

Every rule is enabled by default; ``--disable`` and the toml config found
by :mod:`lo_md_lint.config` turn rules off, and a disabled rule is neither
reported nor fixed.

Usage (from the repo root)::

  uv run lo-md-lint [--fix] [--disable R1,R3] FILE...
  uv run lo-md-lint --all [--fix]

Exit code 0 = clean, 1 = violations found (in check mode), 2 = bad config.
"""

import argparse
from collections.abc import Collection
import pathlib
import re
import subprocess
import sys
import typing

from lo_md_lint import config

CJK = "一-鿿"
WORD = f"A-Za-z0-9{CJK}"
PUNCT_MAP = {",": "，", ";": "；", ":": "：", "?": "？", "!": "！"}

# A "(" inside an English token, e.g. credential(s), word(s), 401(k), f(x):
# the paren belongs to the token, not to prose — exempt from R3 spacing on the
# left. Lookaround so neighbouring tokens can overlap, as in f(g(x)).
ENGLISH_TOKEN_PAREN = re.compile(r"(?<=[A-Za-z0-9])\((?=[A-Za-z0-9])")
BACKTICK_RUN = re.compile("`+")

# Conservative block boundaries for inline code spans: a span may continue
# onto the next line only inside one paragraph / list item / blockquote.
# ATX heading, table row and thematic break are single-line blocks; any
# block opener also ends the previous line's inline container, except that
# consecutive ``>`` lines form one blockquote paragraph.
SINGLE_LINE_BLOCK = re.compile(
    r"^ {0,3}(?:#{1,6}(?:\s|$)|\||(?:[-*_]\s*){3,}$)"
)
BLOCK_START = re.compile(
    SINGLE_LINE_BLOCK.pattern[:-1] + r"|[-*+]\s|\d{1,9}[.)]\s|>)"
)
QUOTE_LINE = re.compile(r"^ {0,3}>")

# (rule id from spec/rules.md, human-readable name, detection pattern).
CHECKS = [
    (
        "R1",
        "R1 halfwidth punct next to CJK",
        re.compile(f"[{CJK}][,;:?!]|[,;:?!][{CJK}]"),
    ),
    ("R2", "R2 fullwidth paren", re.compile("[（）]")),
    (
        "R3",
        "R3 no space before (",
        re.compile(f"(?:[{WORD}]|\\*\\*|`|\\))\\("),
    ),
    (
        "R3",
        "R3 no space after )",
        re.compile(f"\\)(?:[{WORD}]|\\*\\*[{WORD}]|`)"),
    ),
]

# The rule ids of spec/rules.md, and the default enabled set: config can
# only subtract from this, so no configuration means today's behaviour.
ALL_RULES: frozenset[str] = frozenset(rule for rule, _, _ in CHECKS)


class Finding(typing.NamedTuple):
  """One rule violation found in a text.

  Attributes:
    line: 1-based line number of the violation.
    rule: Stable rule id from ``spec/rules.md`` (``R1`` / ``R2`` / ``R3``).
    name: Human-readable check name, shown in the CLI output.
    snippet: The source text around the match, for the CLI output.
  """

  line: int
  rule: str
  name: str
  snippet: str


def _is_fence(line: str) -> bool:
  """Return whether the line opens or closes a fenced code block.

  Args:
    line: One Markdown source line.

  Returns:
    True when the stripped line starts with a code fence marker.
  """
  return line.lstrip().startswith(("```", "~~~"))


def _code_spans(text: str) -> list[tuple[int, int]]:
  """Return the interiors of the inline code spans in one paragraph.

  A backtick run opens a span closed by the next run of the same length;
  a run with no such closer is literal text (CommonMark code spans).

  Args:
    text: Paragraph text; line breaks inside it may fall within a span.

  Returns:
    ``(start, end)`` index pairs of the text between the delimiters.
  """
  runs = list(BACKTICK_RUN.finditer(text))
  spans: list[tuple[int, int]] = []
  i = 0
  while i < len(runs):
    width = len(runs[i].group(0))
    closer = next(
        (j for j in range(i + 1, len(runs)) if len(runs[j].group(0)) == width),
        None,
    )
    if closer is None:
      i += 1
      continue
    spans.append((runs[i].end(), runs[closer].start()))
    i = closer + 1
  return spans


def _protected(lines: list[str]) -> list[list[tuple[int, int]] | None]:
  """Return, per line, the character ranges the rules must not touch.

  Fence lines and fenced content are protected whole (``None``). Other
  lines carry the inline code interiors on that line, computed per block
  (consecutive lines up to a blank line or a block opener; see
  ``BLOCK_START``) so a span may cross a line break inside a paragraph but
  never a block boundary.

  Args:
    lines: The Markdown source split into lines.

  Returns:
    One entry per line: ``None`` for verbatim lines, else ``(start, end)``
    pairs relative to that line.
  """
  result: list[list[tuple[int, int]] | None] = [None] * len(lines)
  paragraph: list[int] = []

  def flush() -> None:
    spans = _code_spans("\n".join(lines[i] for i in paragraph))
    offset = 0
    for i in paragraph:
      end = offset + len(lines[i])
      result[i] = [
          (max(a, offset) - offset, min(b, end) - offset)
          for a, b in spans
          if a < end and b > offset
      ]
      offset = end + 1
    paragraph.clear()

  in_fence = False
  for i, line in enumerate(lines):
    if _is_fence(line):
      flush()
      in_fence = not in_fence
    elif in_fence:
      continue
    elif line.strip():
      continues_quote = bool(
          paragraph
          and QUOTE_LINE.match(line)
          and QUOTE_LINE.match(lines[paragraph[-1]])
      )
      if BLOCK_START.match(line) and not continues_quote:
        flush()
      paragraph.append(i)
      if SINGLE_LINE_BLOCK.match(line):
        flush()
    else:
      flush()
      result[i] = []
  flush()
  return result


def _fix_line(
    line: str, spans: list[tuple[int, int]], rules: Collection[str]
) -> str:
  """Return one line with violations auto-fixed outside inline code.

  Args:
    line: One Markdown line outside fenced code blocks.
    spans: Inline code interiors on this line, from ``_protected``.
    rules: The enabled rule ids; disabled rules leave the text alone.

  Returns:
    The fixed line; inline code is copied through verbatim.
  """
  parts: list[str] = []
  pos = 0
  for start, end in spans:
    parts.append(_fix_prose(line[pos:start], rules))
    parts.append(line[start:end])
    pos = end
  parts.append(_fix_prose(line[pos:], rules))
  return "".join(parts)


def _fix_prose(line: str, rules: Collection[str]) -> str:
  """Return a prose fragment with formatting violations auto-fixed.

  Args:
    line: Markdown text containing no code.
    rules: The enabled rule ids; disabled rules leave the text alone.

  Returns:
    The fixed fragment.
  """
  if "R2" in rules:
    line = line.replace("（", "(").replace("）", ")")
  if "R1" in rules:
    out: list[str] = []
    chars = list(line)
    for i, ch in enumerate(chars):
      if ch in PUNCT_MAP:
        prev = chars[i - 1] if i > 0 else ""
        nxt = chars[i + 1] if i + 1 < len(chars) else ""
        if re.match(f"[{CJK}]", prev) or re.match(f"[{CJK}]", nxt):
          out.append(PUNCT_MAP[ch])
          continue
      out.append(ch)
    line = "".join(out)
  if "R3" in rules:
    line = ENGLISH_TOKEN_PAREN.sub("\x00", line)
    line = re.sub(f"([{WORD}]|\\*\\*|`|\\))\\(", r"\1 (", line)
    line = re.sub(f"\\)((?:\\*\\*)?[{WORD}]|`)", r") \1", line)
    line = line.replace("\x00", "(")
  return line


def fix_text(text: str, rules: Collection[str] = ALL_RULES) -> str:
  """Return the text with formatting violations auto-fixed.

  Fenced code blocks and inline code spans are left untouched (code keeps
  half-width punctuation).

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to every rule.

  Returns:
    The content with full-width parens replaced, CJK-adjacent punctuation
    converted to full-width, and paren spacing inserted.
  """
  lines = text.split("\n")
  return "\n".join(
      line if spans is None else _fix_line(line, spans, rules)
      for line, spans in zip(lines, _protected(lines), strict=True)
  )


def check_text(text: str, rules: Collection[str] = ALL_RULES) -> list[Finding]:
  """Check Markdown text and return its violations in reading order.

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to every rule.

  Returns:
    One ``Finding`` per violation, ordered by line then by rule id.
  """
  findings: list[Finding] = []
  lines = text.splitlines()
  for lineno, (line, code_spans) in enumerate(
      zip(lines, _protected(lines), strict=True), 1
  ):
    if code_spans is None:
      continue
    token_parens = {t.start() for t in ENGLISH_TOKEN_PAREN.finditer(line)}
    for rule, name, pattern in CHECKS:
      if rule not in rules:
        continue
      for m in pattern.finditer(line):
        if any(m.start() < b and m.end() > a for a, b in code_spans):
          continue  # inside inline code
        if m.group(0).endswith("(") and m.end() - 1 in token_parens:
          continue  # paren inside an English token, e.g. word(s), 401(k)
        snippet = line[max(0, m.start() - 12) : m.end() + 12]
        findings.append(Finding(lineno, rule, name, snippet))
  return findings


def check_file(
    path: pathlib.Path, rules: Collection[str] = ALL_RULES
) -> list[str]:
  """Check one file and return its violation descriptions.

  Args:
    path: Markdown file to scan.
    rules: The enabled rule ids; defaults to every rule.

  Returns:
    Human-readable ``file:line`` violation lines, empty when clean.
  """
  return [
      f"{path}:{f.line}: [{f.name}] …{f.snippet}…"
      for f in check_text(path.read_text(encoding="utf-8"), rules)
  ]


def tracked_markdown() -> list[pathlib.Path]:
  """Return the git-tracked Markdown files.

  Returns:
    Paths relative to the repository root.
  """
  out = subprocess.run(
      ["git", "ls-files", "*.md"], capture_output=True, text=True, check=True
  )
  return [pathlib.Path(p) for p in out.stdout.splitlines()]


def main() -> int:
  """Run the checker CLI.

  Returns:
    Process exit code: 0 when clean, 1 when violations were found, 2 when
    the configuration is invalid.
  """
  ap = argparse.ArgumentParser()
  _ = ap.add_argument("files", nargs="*")
  _ = ap.add_argument("--all", action="store_true")
  _ = ap.add_argument("--fix", action="store_true")
  _ = ap.add_argument(
      "--disable",
      action="append",
      metavar="RULE",
      help=(
          "rule ids to disable, repeatable or comma-separated; overrides"
          " the config file"
      ),
  )
  args = ap.parse_args()

  try:
    rules = ALL_RULES - config.resolve_disabled(
        args.disable, pathlib.Path.cwd(), ALL_RULES
    )
  except config.ConfigError as e:
    print(f"config error: {e}", file=sys.stderr)
    return 2

  paths = (
      tracked_markdown() if args.all else [pathlib.Path(f) for f in args.files]
  )
  if not paths:
    ap.error("no files given (use --all or list files)")

  all_problems: list[str] = []
  for path in paths:
    if args.fix:
      src = path.read_text(encoding="utf-8")
      dst = fix_text(src, rules)
      if src != dst:
        _ = path.write_text(dst, encoding="utf-8")
        print(f"fixed: {path}")
    all_problems.extend(check_file(path, rules))

  if all_problems:
    print("\n".join(all_problems))
    print(f"\n{len(all_problems)} violation(s). --fix auto-fixes most.")
    return 1
  print(f"OK: {len(paths)} file(s) clean")
  return 0


if __name__ == "__main__":
  sys.exit(main())
