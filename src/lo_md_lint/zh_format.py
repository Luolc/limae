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
  R4: A space at every boundary between a CJK character and an ASCII
    letter, both directions.
  R5: A space at every boundary between a CJK character and an ASCII
    digit, both directions (no exemption for 年月日).
  R6: A space between a number and a listed ASCII unit (``16GB``); ``%``
    and ``°`` stay tight, letter-prefixed tokens (hex) are exempt.
  R7: A space between a CJK character and the delimiter run of an inline
    code span; unpaired backticks are plain text and exempt.
  R8: A space on each side of a dash — exactly two U+2014 or one U+2E3A —
    next to a non-space, non-dash character.
  R9 (default off): A space between a CJK character and the opening ``[``
    of an inline link.

Fenced code blocks and inline code spans (CommonMark backtick runs, which
may cross line breaks inside a paragraph but not block boundaries) are
exempt from every rule and never rewritten by ``--fix``.

Every rule except R9 is enabled by default; ``--disable`` / ``--enable``
and the toml config found by :mod:`lo_md_lint.config` turn rules off and
on. A disabled rule is neither reported nor fixed.

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

# ASCII unit tokens that follow a number (R6). Case-sensitive; the spec's
# normative list. Single-letter units are too ambiguous to include. Sorted
# longest-first so no unit shadows a longer one in the alternation.
UNITS = (
    "KB", "MB", "GB", "TB", "PB", "KiB", "MiB", "GiB", "TiB", "PiB",
    "bps", "kbps", "Mbps", "Gbps", "Tbps",
    "ms", "ns", "us", "min",
    "Hz", "kHz", "MHz", "GHz",
    "px", "pt", "dpi", "fps",
    "kg", "mg", "km", "cm", "mm", "nm",
)  # fmt: skip
_UNIT_ALT = "|".join(sorted(UNITS, key=len, reverse=True))

# R4 / R5: zero-width boundaries between CJK and ASCII letters / digits.
CJK_LATIN_BOUNDARY = re.compile(
    f"(?<=[{CJK}])(?=[A-Za-z])|(?<=[A-Za-z])(?=[{CJK}])"
)
CJK_DIGIT_BOUNDARY = re.compile(f"(?<=[{CJK}])(?=[0-9])|(?<=[0-9])(?=[{CJK}])")
# R6: the digit run must not continue an English token (0x1F, hex strings)
# and the unit must end the token (2FA, 16GBx are not number-plus-unit).
NUMBER_UNIT = re.compile(
    f"(?<![A-Za-z0-9])([0-9]+)({_UNIT_ALT})(?![A-Za-z0-9])"
)
# R7: a backtick run next to CJK is only a violation when it is the
# delimiter of an inline code span; check_text verifies the run's position
# against the spans, _fix_line knows the delimiters structurally.
R7_OPEN = re.compile(f"(?<=[{CJK}])`+")
R7_CLOSE = re.compile(f"`+(?=[{CJK}])")
R7_OPEN_EDGE = re.compile(f"(?<=[{CJK}])(`+)$")
R7_CLOSE_EDGE = re.compile(f"^(`+)(?=[{CJK}])")
# R8: exactly two U+2014 (`——`) or one U+2E3A; a neighbouring dash is a
# malformed dash, not a spacing problem, so it does not trigger. The check
# patterns consume the dash so that a dash inside an inline code span is
# excluded by the span-overlap filter; the fix patterns are the zero-width
# boundaries themselves, which per prose fragment amounts to the same set.
DASH_LEFT = re.compile("(?<=[^\\s—⸺])(?:——(?!—)|⸺)")
DASH_RIGHT = re.compile("(?:(?<!—)——|⸺)(?=[^\\s—⸺])")
DASH_LEFT_FIX = re.compile("(?<=[^\\s—⸺])(?=——(?!—)|⸺)")
DASH_RIGHT_FIX = re.compile("(?:(?<=——)(?<!———)|(?<=⸺))(?=[^\\s—⸺])")
# R9: CJK directly before an inline link, matched with its `[text](` opener
# so a link whose text holds a code span falls under the span exemption.
LINK_AFTER_CJK = re.compile(f"(?<=[{CJK}])\\[[^\\]]*\\]\\(")
LINK_AFTER_CJK_FIX = re.compile(f"(?<=[{CJK}])(?=\\[[^\\]]*\\]\\()")

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
    ("R4", "R4 no space between CJK and Latin", CJK_LATIN_BOUNDARY),
    ("R5", "R5 no space between CJK and digit", CJK_DIGIT_BOUNDARY),
    ("R6", "R6 no space between number and unit", NUMBER_UNIT),
    ("R7", "R7 no space before inline code", R7_OPEN),
    ("R7", "R7 no space after inline code", R7_CLOSE),
    ("R8", "R8 no space before dash", DASH_LEFT),
    ("R8", "R8 no space after dash", DASH_RIGHT),
    ("R9", "R9 no space between CJK and link", LINK_AFTER_CJK),
]

# The rule ids of spec/rules.md. Configuration starts from DEFAULT_RULES
# (every rule except the default-off ones) and can subtract via `disable`
# or add back via `enable`, so no configuration means today's behaviour.
ALL_RULES: frozenset[str] = frozenset(rule for rule, _, _ in CHECKS)
DEFAULT_OFF: frozenset[str] = frozenset({"R9"})
DEFAULT_RULES: frozenset[str] = ALL_RULES - DEFAULT_OFF


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
  closes = False
  for start, end in spans:
    opens = start > pos and line[start - 1] == "`"
    parts.append(_fix_frag(line[pos:start], rules, closes, opens))
    parts.append(line[start:end])
    pos = end
    closes = end < len(line) and line[end] == "`"
  parts.append(_fix_frag(line[pos:], rules, closes, False))
  return "".join(parts)


def _fix_frag(
    frag: str, rules: Collection[str], closes: bool, opens: bool
) -> str:
  """Return one prose fragment fixed, including its span delimiters (R7).

  A fragment between two inline code spans starts with the left span's
  closing delimiter run and ends with the right span's opening run; only
  ``_fix_line`` knows which backticks are delimiters, so R7 spacing
  happens here and not in ``_fix_prose``.

  Args:
    frag: Prose between two code interiors (delimiter runs included).
    rules: The enabled rule ids; disabled rules leave the text alone.
    closes: Whether the fragment starts with a closing delimiter run.
    opens: Whether the fragment ends with an opening delimiter run.

  Returns:
    The fixed fragment.
  """
  frag = _fix_prose(frag, rules)
  if "R7" in rules:
    if closes:
      frag = R7_CLOSE_EDGE.sub(r"\1 ", frag)
    if opens:
      frag = R7_OPEN_EDGE.sub(r" \1", frag)
  return frag


def _fix_r1(line: str) -> str:
  """Return the fragment with CJK-adjacent half-width punct full-width.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
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
  return "".join(out)


def _fix_r3(line: str) -> str:
  """Return the fragment with half-width paren outside spacing applied.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
  line = ENGLISH_TOKEN_PAREN.sub("\x00", line)
  line = re.sub(f"([{WORD}]|\\*\\*|`|\\))\\(", r"\1 (", line)
  line = re.sub(f"\\)((?:\\*\\*)?[{WORD}]|`)", r") \1", line)
  return line.replace("\x00", "(")


def _fix_r8(line: str) -> str:
  """Return the fragment with dash-side spaces inserted.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
  line = DASH_LEFT_FIX.sub(" ", line)
  return DASH_RIGHT_FIX.sub(" ", line)


# The per-fragment fix pipeline in the fix order of spec/rules.md 「修复顺序」:
# width conversions first, then the spacing rules in id order. R7 is
# missing because only _fix_line knows which backticks delimit a span.
_PROSE_FIXES: list[tuple[str, typing.Callable[[str], str]]] = [
    ("R2", lambda line: line.replace("（", "(").replace("）", ")")),
    ("R1", _fix_r1),
    ("R3", _fix_r3),
    ("R4", lambda line: CJK_LATIN_BOUNDARY.sub(" ", line)),
    ("R5", lambda line: CJK_DIGIT_BOUNDARY.sub(" ", line)),
    ("R6", lambda line: NUMBER_UNIT.sub(r"\1 \2", line)),
    ("R8", _fix_r8),
    ("R9", lambda line: LINK_AFTER_CJK_FIX.sub(" ", line)),
]


def _fix_prose(line: str, rules: Collection[str]) -> str:
  """Return a prose fragment with formatting violations auto-fixed.

  Args:
    line: Markdown text containing no code.
    rules: The enabled rule ids; disabled rules leave the text alone.

  Returns:
    The fixed fragment.
  """
  for rule, fix in _PROSE_FIXES:
    if rule in rules:
      line = fix(line)
  return line


def fix_text(text: str, rules: Collection[str] = DEFAULT_RULES) -> str:
  """Return the text with formatting violations auto-fixed.

  Fenced code blocks and inline code spans are left untouched (code keeps
  half-width punctuation).

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to the default-enabled set.

  Returns:
    The content with full-width parens replaced, CJK-adjacent punctuation
    converted to full-width, and spacing inserted.
  """
  lines = text.split("\n")
  return "\n".join(
      line if spans is None else _fix_line(line, spans, rules)
      for line, spans in zip(lines, _protected(lines), strict=True)
  )


def check_text(
    text: str, rules: Collection[str] = DEFAULT_RULES
) -> list[Finding]:
  """Check Markdown text and return its violations in reading order.

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to the default-enabled set.

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
        if rule == "R7" and not _is_delimiter_run(m, pattern, code_spans):
          continue  # unpaired backticks are plain text, not a span
        snippet = line[max(0, m.start() - 12) : m.end() + 12]
        findings.append(Finding(lineno, rule, name, snippet))
  return findings


def _is_delimiter_run(
    m: re.Match[str],
    pattern: re.Pattern[str],
    code_spans: list[tuple[int, int]],
) -> bool:
  """Return whether an R7 backtick-run match delimits a code span.

  Args:
    m: A match of ``R7_OPEN`` or ``R7_CLOSE`` (a full backtick run).
    pattern: The pattern the match came from, to tell open from close.
    code_spans: Inline code interiors on this line, from ``_protected``.

  Returns:
    True when the run ends exactly where a span interior starts (open) or
    starts exactly where one ends (close).
  """
  if pattern is R7_OPEN:
    return any(m.end() == a for a, _ in code_spans)
  return any(m.start() == b for _, b in code_spans)


def check_file(
    path: pathlib.Path, rules: Collection[str] = DEFAULT_RULES
) -> list[str]:
  """Check one file and return its violation descriptions.

  Args:
    path: Markdown file to scan.
    rules: The enabled rule ids; defaults to the default-enabled set.

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
  _ = ap.add_argument(
      "--enable",
      action="append",
      metavar="RULE",
      help=(
          "rule ids to enable (for default-off rules), same syntax and"
          " precedence as --disable"
      ),
  )
  args = ap.parse_args()

  try:
    rules = config.resolve_rules(
        args.disable, args.enable, pathlib.Path.cwd(), ALL_RULES, DEFAULT_RULES
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
