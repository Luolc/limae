"""Chinese Markdown formatting checker.

``spec/rules.md`` is the normative, language-agnostic rule spec and
``spec/fixtures/`` the golden set every implementation runs against; this
module is the Python reference implementation of both.

Rules:
  zh-typography-1: No half-width , ; : ? ! adjacent to a CJK character.
  zh-typography-2: No full-width parentheses; use half-width ( ) instead.
  zh-typography-3: Half-width parens need an outside space when adjacent
    to a word character, a closing paren, a bold marker, or an
    inline-code backtick. Parens inside an English token (``word(s)``,
    ``401(k)``) and Markdown link syntax ``](...)`` are exempt.
  zh-typography-4: A space at every boundary between a CJK character and
    an ASCII letter, both directions.
  zh-typography-5: A space at every boundary between a CJK character and
    an ASCII digit, both directions; 年月日 are exempt only where the
    ``skip_zh_units`` config key lists them.
  zh-typography-6: A space between a number and a listed ASCII unit
    (``16GB``); ``%`` and ``°`` stay tight, letter-prefixed tokens (hex)
    are exempt.
  zh-typography-7: A space between a CJK character and the delimiter run
    of an inline code span; unpaired backticks are plain text and exempt.
  zh-typography-8: A space on each side of a dash — exactly two U+2014 or
    one U+2E3A — next to a non-space, non-dash character.
  zh-typography-9 (default off): A space between a CJK character and the
    opening ``[`` of an inline link.
  zh-typography-10: No full-width digits; use half-width 0-9 instead.
  zh-typography-11: No spaces next to full-width punctuation
    (，。、；：？！); spaces next to a dash stay (zh-typography-8 owns
    those).
  zh-tell-1 (experimental): A formulaic Chinese phrase listed in the wordlist.
  zh-tell-2 (experimental): The negative-parallel 不是 … 而是 … within 20
    characters.
  zh-tell-3 (experimental): A corporate buzzword listed in the wordlist.
  zh-tell-4 (experimental): Chat residue listed in the wordlist.
  en-tell-1 (experimental): An English AI-vocabulary word listed in the
    wordlist, matched as a whole word, case-insensitively.
  en-tell-2 (experimental): English negative parallelism — ``not just X, but
    Y`` or a negated copula answered by an affirmative one — within 40
    characters.
  en-tell-3 (experimental): A Claudish register word listed in the wordlist,
    matched the same way as en-tell-1.
  zh-tell-5 (experimental): A 零 + noun coinage — a 2-5 character Chinese run
    opening with 零 that no allowlist entry covers.
  zh-word-1 (experimental): A wrong term listed in the wordlist, on a line
    carrying one of that entry's context anchors.
  zh-word-2 (experimental): 秘密 used as a category term, which no allowlist
    entry covers.

The tell and word families read their wordlists from
``spec/wordlists/`` through :mod:`limae.wordlists`; the wordlist rules
(zh-tell-1 / zh-tell-3 / zh-tell-4 / en-tell-1 / en-tell-3) report at
most one violation per line, the sentence-shape rules (zh-tell-2 /
en-tell-2), zh-tell-5, zh-word-1 and zh-word-2 one per occurrence.
zh-tell-5 and zh-word-2 read allowlists instead — wordlists of the
opposite polarity, where a hit means no violation.

Fenced code blocks and inline code spans (CommonMark backtick runs, which
may cross line breaks inside a paragraph but not block boundaries) are
exempt from every rule and never rewritten by ``--fix``, and so are link
destinations, raw URLs and quote spans whose content holds kana (a
verbatim Japanese quotation).

Every rule except zh-typography-9 is enabled by default; ``--disable`` /
``--enable`` and the toml config found by :mod:`limae.config` turn rules
off and on. A disabled rule is neither reported nor fixed. The config's
``skip_zh_units`` key additionally tunes zh-typography-5.

``GRADES`` carries the three orthogonal axes of ``spec/rules.md`` section
「规则属性」 — fixability, default severity, maturity — one entry per rule
and nowhere else. The ``severity`` config key overrides the default
severity per rule, ``enable_experimental`` joins the experimental rules
into the enabled set: the zh-typography family is fixable · error ·
stable, the zh-tell / en-tell / zh-word families warning · experimental.

Two escape hatches sit below the configuration: the inline directives of
:mod:`limae.directives` narrow the enabled set line by line, and the
``.limae-ignore`` file drops whole input files (both in
:mod:`limae.config`).

Usage (from the repo root)::

  uv run limae [--fix] [--disable zh-typography-1,zh-typography-3] FILE...
  uv run limae --all [--fix]

Exit code 0 = clean or warnings only, 1 = at least one error-level
violation, 2 = bad configuration or bad inline directive
(``spec/rules.md`` section 「退出码」).
"""

import argparse
from collections.abc import Collection, Mapping
import pathlib
import re
import subprocess
import sys
import typing

from limae import config, directives, hook, polish, wordlists

CJK = "一-鿿"
WORD = f"A-Za-z0-9{CJK}"
PUNCT_MAP = {",": "，", ";": "；", ":": "：", "?": "？", "!": "！"}

# zh-typography-1's full-stop extension: a `.` next to CJK becomes `。` unless
# it is glued to an ASCII letter/digit (extensions, domains, versions), part of
# a `...` run, or one of these abbreviations' dots (spec's normative list). An
# occurrence must not be preceded by an ASCII letter.
ABBREVIATIONS = (
    "e.g.", "i.e.", "etc.", "cf.", "vs.",
    "Mr.", "Mrs.", "Ms.", "Dr.", "Prof.", "St.",
)  # fmt: skip
ABBREV = re.compile(
    "(?<![A-Za-z])(?:"
    + "|".join(
        re.escape(a) for a in sorted(ABBREVIATIONS, key=len, reverse=True)
    )
    + ")"
)
ZH_TYPOGRAPHY_1_DOT = re.compile(
    f"(?<=[{CJK}])\\.(?![A-Za-z0-9.])|(?<!\\.)\\.(?=[{CJK}])"
)
CJK_CHAR = re.compile(f"[{CJK}]")

# A "(" inside an English token, e.g. credential(s), word(s), 401(k), f(x): the
# paren belongs to the token, not to prose — exempt from zh-typography-3
# spacing on the left. Lookaround so neighbouring tokens can overlap, as in
# f(g(x)).
ENGLISH_TOKEN_PAREN = re.compile(r"(?<=[A-Za-z0-9])\((?=[A-Za-z0-9])")
BACKTICK_RUN = re.compile("`+")

# ASCII unit tokens that follow a number (zh-typography-6). Case-sensitive; the
# spec's normative list. Single-letter units are too ambiguous to include.
# Sorted longest-first so no unit shadows a longer one in the alternation.
UNITS = (
    "KB", "MB", "GB", "TB", "PB", "KiB", "MiB", "GiB", "TiB", "PiB",
    "bps", "kbps", "Mbps", "Gbps", "Tbps",
    "ms", "ns", "us", "min",
    "Hz", "kHz", "MHz", "GHz",
    "px", "pt", "dpi", "fps",
    "kg", "mg", "km", "cm", "mm", "nm",
)  # fmt: skip
_UNIT_ALT = "|".join(sorted(UNITS, key=len, reverse=True))

# zh-typography-4 / zh-typography-5: zero-width boundaries between CJK and
# ASCII letters / digits.
CJK_LATIN_BOUNDARY = re.compile(
    f"(?<=[{CJK}])(?=[A-Za-z])|(?<=[A-Za-z])(?=[{CJK}])"
)
CJK_DIGIT_BOUNDARY = re.compile(f"(?<=[{CJK}])(?=[0-9])|(?<=[0-9])(?=[{CJK}])")
# zh-typography-5's `skip_zh_units` exemption: a digit run, `.` / `,` separated
# segments included (1.5, 1,000), directly followed by a listed measure word
# exempts both of its own boundaries.
NUMBER_RUN = re.compile("[0-9]+(?:[.,][0-9]+)*")
# zh-typography-6: the digit run must not continue an English token (0x1F, hex
# strings) and the unit must end the token (2FA, 16GBx are not
# number-plus-unit).
NUMBER_UNIT = re.compile(
    f"(?<![A-Za-z0-9])([0-9]+)({_UNIT_ALT})(?![A-Za-z0-9])"
)
# zh-typography-7: a backtick run next to CJK is only a violation when it is
# the delimiter of an inline code span; check_text verifies the run's position
# against the spans, _fix_line knows the delimiters structurally.
ZH_TYPOGRAPHY_7_OPEN = re.compile(f"(?<=[{CJK}])`+")
ZH_TYPOGRAPHY_7_CLOSE = re.compile(f"`+(?=[{CJK}])")
ZH_TYPOGRAPHY_7_OPEN_EDGE = re.compile(f"(?<=[{CJK}])(`+)$")
ZH_TYPOGRAPHY_7_CLOSE_EDGE = re.compile(f"^(`+)(?=[{CJK}])")
# zh-typography-8: exactly two U+2014 (`——`) or one U+2E3A; a neighbouring dash
# is a malformed dash, not a spacing problem, so it does not trigger. The check
# patterns consume the dash and its triggering neighbour so that a dash inside
# — or right at the edge of — an exempt range is excluded by ``_exempt``; the
# fix patterns are the zero-width boundaries themselves, which per prose
# fragment amounts to the same set.
DASH_LEFT = re.compile("[^\\s—⸺](?:——(?!—)|⸺)")
DASH_RIGHT = re.compile("(?:(?<!—)——|⸺)[^\\s—⸺]")
DASH_LEFT_FIX = re.compile("(?<=[^\\s—⸺])(?=——(?!—)|⸺)")
DASH_RIGHT_FIX = re.compile("(?:(?<=——)(?<!———)|(?<=⸺))(?=[^\\s—⸺])")
# zh-typography-9: CJK directly before an inline link, matched with its
# `[text](` opener so a link whose text holds a code span falls under the span
# exemption.
LINK_AFTER_CJK = re.compile(f"(?<=[{CJK}])\\[[^\\]]*\\]\\(")
LINK_AFTER_CJK_FIX = re.compile(f"(?<=[{CJK}])(?=\\[[^\\]]*\\]\\()")

# Link destinations and raw URLs are addresses, not prose (global
# exemption 3 in spec/rules.md). The destination is the minimal-form
# `](...)` interior; a raw URL runs over RFC 3986 characters and then
# drops trailing punctuation, which stays prose.
URL_DESTINATION = re.compile(r"\]\(([^)]*)\)")
RAW_URL = re.compile(
    r"(?<![A-Za-z0-9])https?://[A-Za-z0-9\-._~:/?#\[\]@!$&'()*+,;=%]*"
)
TRAILING_PUNCT = ")]}>,.;:!?"

# A quote-bracket pair whose content holds kana is a verbatim Japanese
# quotation (global exemption 4): rewriting it would break the quote.
QUOTE_PAIRS = {"「": "」", "『": "』", "《": "》"}
KANA = re.compile("[\u3040-\u30ff]")

# zh-typography-10: full-width digits.
FULLWIDTH_DIGIT = re.compile("[０-９]")
HALFWIDTH_DIGITS = str.maketrans("０１２３４５６７８９", "0123456789")
# zh-typography-11: a space run with a full-width punctuation mark on one end
# and a visible character on the other (a dash is zh-typography-8's territory,
# a `|` is table-cell padding). Both end characters are consumed so that
# contact with an exempt range suppresses the finding, consistent with the
# fragment-local fix.
FW_PUNCT = "，。、；：？！"
SPACE_BEFORE_FW = re.compile(f"[^\\s—⸺|] +[{FW_PUNCT}]")
SPACE_AFTER_FW = re.compile(f"[{FW_PUNCT}] +[^\\s—⸺|]")
SPACE_BEFORE_FW_FIX = re.compile(f"(?<=[^\\s—⸺|]) +(?=[{FW_PUNCT}])")
SPACE_AFTER_FW_FIX = re.compile(f"(?<=[{FW_PUNCT}]) +(?=[^\\s—⸺|])")

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


# The word boundary of the English wordlists (en-tell-1 / en-tell-3): a listed
# entry is only a hit as a whole word. `-` is deliberately not a boundary
# character — hyphen compression is itself Claudish, so `non-load-bearing`
# counts — while `_` is, so a snake_case identifier outside a code span does
# not.
EN_LEFT = "(?<![A-Za-z0-9_])"
EN_RIGHT = "(?![A-Za-z0-9_])"
NEVER = "(?!)"


def _alternation(rule: str) -> str:
  """Return one wordlist as an escaped alternation, longest first.

  Args:
    rule: The rule id, which is also the wordlist's file stem.

  Returns:
    The listed entries joined by ``|``, longest first so no entry
    shadows a longer one; the empty string for an empty wordlist.
  """
  listed = sorted(wordlists.phrases(rule), key=len, reverse=True)
  return "|".join(re.escape(p) for p in listed)


def _phrase_pattern(rule: str) -> re.Pattern[str]:
  """Return the literal-substring pattern of one Chinese wordlist.

  Args:
    rule: The rule id, which is also the wordlist's file stem.

  Returns:
    An alternation of the listed phrases; a never-matching pattern for an
    empty wordlist.
  """
  return re.compile(_alternation(rule) or NEVER)


def _word_pattern(rule: str) -> re.Pattern[str]:
  """Return the whole-word pattern of one English wordlist.

  Args:
    rule: The rule id, which is also the wordlist's file stem.

  Returns:
    An alternation of the listed entries, matched case-insensitively
    between word boundaries; a never-matching pattern for an empty
    wordlist.
  """
  alternation = _alternation(rule)
  if not alternation:
    return re.compile(NEVER)
  return re.compile(f"{EN_LEFT}(?:{alternation}){EN_RIGHT}", re.IGNORECASE)


# zh-tell-2: 不是 … 而是 …, at most 20 characters apart, punctuation included.
# The progressive 不仅 … 更 … is normal Chinese prose and not collected.
NEGATIVE_PARALLEL = re.compile("不是.{0,20}?而是")
# en-tell-2: the two English negative-parallel shapes, at most 40 characters
# apart. `not only … but also …` is ordinary formal English and not
# collected, the same call zh-tell-2 makes about the progressive 不仅 … 更 ….
APOSTROPHE = "['\u2019]"
NEGATED_COPULA = (
    f"(?:(?:it|that){APOSTROPHE}s|they{APOSTROPHE}re|is|are|was|were)"
    f"\\s+not|(?:is|are|was|were)n{APOSTROPHE}t"
)
AFFIRMED_COPULA = (
    f"(?:it|that){APOSTROPHE}s|they{APOSTROPHE}re"
    "|(?:it|that)\\s+is|they\\s+are"
)
NOT_JUST_BUT = re.compile(
    f"{EN_LEFT}not\\s+just{EN_RIGHT}.{{0,40}}?{EN_LEFT}but{EN_RIGHT}",
    re.IGNORECASE,
)
NEGATIVE_PARALLEL_EN = re.compile(
    f"{EN_LEFT}(?:{NEGATED_COPULA}){EN_RIGHT}"
    f".{{0,40}}?{EN_LEFT}(?:{AFFIRMED_COPULA}){EN_RIGHT}",
    re.IGNORECASE,
)
# zh-tell-5: one candidate per 零. The candidate string runs from that 零 to the
# end of its CJK run and is judged in `_is_coinage`, not by the pattern —
# a fixed-width window would truncate 零额外请求 into 零额外请 and judge a
# word nobody wrote (spec 「匹配单位」).
ZERO = re.compile("零")
CJK_RUN = re.compile(f"[{CJK}]*")
ZH_TELL_5_MIN_LENGTH = 2  # a lone 零 is not a coinage
ZH_TELL_5_MAX_LENGTH = (
    5  # longer runs are sentences written without punctuation
)
# zh-word-2: 秘密 as a category term; no anchor, unlike zh-word-1.
SECRET = re.compile("秘密")
# The allowlists of zh-tell-5 and zh-word-2, of the opposite polarity to every
# other wordlist: an entry covering the hit means no violation.
ALLOWED: dict[str, tuple[str, ...]] = {
    "zh-tell-5": wordlists.phrases("zh-tell-5-allow"),
    "zh-word-2": wordlists.phrases("zh-word-2-allow"),
}

# The wordlist rules fire at most once per line: listed words come in clusters
# and one finding per occurrence would just flood the report. The
# sentence-shape rules (zh-tell-2 / en-tell-2) report every match — each shape
# is its own violation.
ONCE_PER_LINE = frozenset(
    {"zh-tell-1", "zh-tell-3", "zh-tell-4", "en-tell-1", "en-tell-3"}
)
# zh-word-1: one pattern per wordlist entry, so the findings of one line stay
# ordered by position and each maps to its own fix.
TERMS: tuple[wordlists.Term, ...] = wordlists.terms()
TERM_PATTERNS: dict[re.Pattern[str], wordlists.Term] = {
    re.compile(re.escape(t.wrong)): t for t in TERMS
}

# (rule id from spec/rules.md, human-readable name, detection pattern).
CHECKS = [
    (
        "zh-typography-1",
        "zh-typography-1 halfwidth punct next to CJK",
        re.compile(f"[{CJK}][,;:?!]|[,;:?!][{CJK}]"),
    ),
    (
        "zh-typography-1",
        "zh-typography-1 halfwidth period next to CJK",
        ZH_TYPOGRAPHY_1_DOT,
    ),
    (
        "zh-typography-2",
        "zh-typography-2 fullwidth paren",
        re.compile("[（）]"),
    ),
    (
        "zh-typography-3",
        "zh-typography-3 no space before (",
        re.compile(f"(?:[{WORD}]|\\*\\*|`|\\))\\("),
    ),
    (
        "zh-typography-3",
        "zh-typography-3 no space after )",
        re.compile(f"\\)(?:[{WORD}]|\\*\\*[{WORD}]|`)"),
    ),
    (
        "zh-typography-4",
        "zh-typography-4 no space between CJK and Latin",
        CJK_LATIN_BOUNDARY,
    ),
    (
        "zh-typography-5",
        "zh-typography-5 no space between CJK and digit",
        CJK_DIGIT_BOUNDARY,
    ),
    (
        "zh-typography-6",
        "zh-typography-6 no space between number and unit",
        NUMBER_UNIT,
    ),
    (
        "zh-typography-7",
        "zh-typography-7 no space before inline code",
        ZH_TYPOGRAPHY_7_OPEN,
    ),
    (
        "zh-typography-7",
        "zh-typography-7 no space after inline code",
        ZH_TYPOGRAPHY_7_CLOSE,
    ),
    ("zh-typography-8", "zh-typography-8 no space before dash", DASH_LEFT),
    ("zh-typography-8", "zh-typography-8 no space after dash", DASH_RIGHT),
    (
        "zh-typography-9",
        "zh-typography-9 no space between CJK and link",
        LINK_AFTER_CJK,
    ),
    ("zh-typography-10", "zh-typography-10 fullwidth digit", FULLWIDTH_DIGIT),
    (
        "zh-typography-11",
        "zh-typography-11 space before fullwidth punct",
        SPACE_BEFORE_FW,
    ),
    (
        "zh-typography-11",
        "zh-typography-11 space after fullwidth punct",
        SPACE_AFTER_FW,
    ),
    ("zh-tell-1", "zh-tell-1 formulaic phrase", _phrase_pattern("zh-tell-1")),
    ("zh-tell-2", "zh-tell-2 negative parallelism", NEGATIVE_PARALLEL),
    ("zh-tell-3", "zh-tell-3 corporate buzzword", _phrase_pattern("zh-tell-3")),
    ("zh-tell-4", "zh-tell-4 chat residue", _phrase_pattern("zh-tell-4")),
    (
        "en-tell-1",
        "en-tell-1 English AI vocabulary",
        _word_pattern("en-tell-1"),
    ),
    ("en-tell-2", "en-tell-2 English negative parallelism", NOT_JUST_BUT),
    (
        "en-tell-2",
        "en-tell-2 English negative parallelism",
        NEGATIVE_PARALLEL_EN,
    ),
    ("en-tell-3", "en-tell-3 Claudish register", _word_pattern("en-tell-3")),
    ("zh-tell-5", "zh-tell-5 zero-noun coinage", ZERO),
    *(
        ("zh-word-1", f"zh-word-1 term {term.wrong} -> {term.right}", pattern)
        for pattern, term in TERM_PATTERNS.items()
    ),
    ("zh-word-2", "zh-word-2 misused 秘密", SECRET),
]


class RuleGrade(typing.NamedTuple):
  """The three orthogonal attributes of one rule.

  ``spec/rules.md`` section 「规则属性」 is normative; ``GRADES`` mirrors the
  属性 line of every rule entry there, and is the only place the axes
  live in this implementation.

  Attributes:
    fixable: Whether the rule has one deterministic fix, i.e. whether
      ``--fix`` rewrites the text; a non-fixable rule is only reported.
    severity: The severity the spec gives the rule, ``error`` or
      ``warning``; the ``severity`` config key overrides it per rule.
    experimental: Whether the rule is still experimental, i.e. out of the
      default set until ``enable_experimental`` joins it in.
  """

  fixable: bool
  severity: str
  experimental: bool


# Every rule id of spec/rules.md with its three axes: the zh-typography
# family (中文排版) is fixable · error · stable, the zh-tell / en-tell
# families (AI 腔) and the zh-word family (中文用词) are warning ·
# experimental, so they stay out of the enabled set until
# `enable_experimental` joins them in.
GRADES: dict[str, RuleGrade] = {
    "zh-typography-1": RuleGrade(True, config.ERROR, False),
    "zh-typography-2": RuleGrade(True, config.ERROR, False),
    "zh-typography-3": RuleGrade(True, config.ERROR, False),
    "zh-typography-4": RuleGrade(True, config.ERROR, False),
    "zh-typography-5": RuleGrade(True, config.ERROR, False),
    "zh-typography-6": RuleGrade(True, config.ERROR, False),
    "zh-typography-7": RuleGrade(True, config.ERROR, False),
    "zh-typography-8": RuleGrade(True, config.ERROR, False),
    "zh-typography-9": RuleGrade(True, config.ERROR, False),
    "zh-typography-10": RuleGrade(True, config.ERROR, False),
    "zh-typography-11": RuleGrade(True, config.ERROR, False),
    "zh-tell-1": RuleGrade(False, config.WARNING, True),
    "zh-tell-2": RuleGrade(False, config.WARNING, True),
    "zh-tell-3": RuleGrade(False, config.WARNING, True),
    "zh-tell-4": RuleGrade(False, config.WARNING, True),
    "en-tell-1": RuleGrade(False, config.WARNING, True),
    "en-tell-2": RuleGrade(False, config.WARNING, True),
    "en-tell-3": RuleGrade(False, config.WARNING, True),
    "zh-tell-5": RuleGrade(False, config.WARNING, True),
    "zh-word-1": RuleGrade(True, config.WARNING, True),
    "zh-word-2": RuleGrade(False, config.WARNING, True),
}

# Configuration starts from DEFAULT_RULES (every rule except the
# default-off and the experimental ones) and can subtract via `disable` or
# add back via `enable`, so no configuration means today's behaviour.
ALL_RULES: frozenset[str] = frozenset(GRADES)
DEFAULT_OFF: frozenset[str] = frozenset({"zh-typography-9"})
EXPERIMENTAL_RULES: frozenset[str] = frozenset(
    rule for rule, grade in GRADES.items() if grade.experimental
)
DEFAULT_RULES: frozenset[str] = ALL_RULES - DEFAULT_OFF - EXPERIMENTAL_RULES

# Report order within one line: the rule-id order of spec/rules.md, then
# position (spec/README.md's findings order).
RULE_ORDER: dict[str, int] = {}
for _rule, _, _ in CHECKS:
  if _rule not in RULE_ORDER:
    RULE_ORDER[_rule] = len(RULE_ORDER)


class Finding(typing.NamedTuple):
  """One rule violation found in a text.

  Attributes:
    line: 1-based line number of the violation.
    rule: Stable rule id from ``spec/rules.md``
      (``zh-typography-1`` / ``zh-typography-2`` / ``zh-typography-3``).
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
    pairs relative to that line. A span whose interior falls entirely on
    other lines still leaves a zero-length pair at the line edge, so a
    delimiter run ending a line (opener) or starting one (closer) stays
    identifiable for zh-typography-7.
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
          if a <= end and b >= offset
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


def _rule_masks(
    lines: list[str], protected: list[list[tuple[int, int]] | None]
) -> list[frozenset[str]]:
  """Return, per line, the rule ids inline directives switch off there.

  Args:
    lines: The Markdown source split into lines.
    protected: The ``_protected`` result for those lines; its ``None``
      entries are the verbatim lines, where a directive-shaped comment is
      only code.

  Returns:
    One mask per line, to subtract from the run's enabled set.
  """
  return directives.rule_masks(
      lines, [spans is None for spans in protected], ALL_RULES
  )


def _quote_spans(line: str) -> list[tuple[int, int]]:
  """Return the interiors of the kana-holding quote spans on one line.

  Scanning left to right, an opener pairs with the closer that brings its
  bracket type back to depth zero — the outermost pair, so a nested pair
  is never identified on its own. An opener without such a closer is not
  a span and the scan resumes right after it.

  Args:
    line: One Markdown line outside fenced code blocks.

  Returns:
    Sorted, disjoint ``(start, end)`` ranges between the brackets of the
    pairs whose content holds kana.
  """
  spans: list[tuple[int, int]] = []
  i = 0
  while i < len(line):
    close = QUOTE_PAIRS.get(line[i])
    if close is None:
      i += 1
      continue
    depth = 0
    for j in range(i, len(line)):
      if line[j] == line[i]:
        depth += 1
      elif line[j] == close:
        depth -= 1
        if depth == 0:
          if KANA.search(line[i + 1 : j]):
            spans.append((i + 1, j))
          i = j
          break
    i += 1
  return spans


def _prose_spans(
    line: str, code_spans: list[tuple[int, int]]
) -> list[tuple[int, int]]:
  """Return the non-code exempt ranges on one line (exemptions 3 and 4).

  Kana quote interiors first, so a verbatim quotation wins over a URL
  range inside it; an interior is claimed around the inline code spans
  nested in it, which stay code, so the quotation as a whole is still
  exempt end to end. Then minimal-form inline link destinations and raw
  ``http(s)://`` URLs with their trailing punctuation stripped.
  Candidates overlapping an inline code interior, or a range already
  claimed, are dropped so the ranges stay disjoint.

  Args:
    line: One Markdown line outside fenced code blocks.
    code_spans: Inline code interiors on this line, from ``_protected``.

  Returns:
    Sorted, disjoint ``(start, end)`` ranges.
  """
  spans: list[tuple[int, int]] = []
  taken = list(code_spans)

  def claim(a: int, b: int) -> None:
    if a < b and not any(a < d and b > c for c, d in taken):
      spans.append((a, b))
      taken.append((a, b))

  for a, b in _quote_spans(line):
    start = a
    for c, d in sorted(code_spans):
      if start <= c and d <= b:  # code nested in the quotation
        claim(start, c)  # up to and including the opening delimiter run
        start = d  # resume at the closing run
    claim(start, b)
  for m in URL_DESTINATION.finditer(line):
    claim(*m.span(1))
  for m in RAW_URL.finditer(line):
    a, b = m.span()
    while b > a and line[b - 1] in TRAILING_PUNCT:
      b -= 1
    claim(a, b)
  return sorted(spans)


def _exempt(m: re.Match[str], spans: list[tuple[int, int]]) -> bool:
  """Return whether a match involves characters of an exempt range.

  A zero-width match sits between two characters, so it is exempt when
  either neighbour falls inside a range (position within the closed
  interval); a consuming match is exempt on any overlap.

  Args:
    m: A match of one of the ``CHECKS`` patterns.
    spans: Exempt ranges on the line (code interiors and URL ranges).

  Returns:
    True when the finding must be suppressed.
  """
  if m.start() == m.end():
    return any(a <= m.start() <= b for a, b in spans)
  return any(m.start() < b and m.end() > a for a, b in spans)


class FixContext(typing.NamedTuple):
  """What the per-fragment fixes need beyond the fragment itself.

  Attributes:
    units: ``skip_zh_units``, the measure words exempting
      zh-typography-5 boundaries.
    terms: The zh-word-1 entries whose anchors the line carries. The anchor test
      is per line but the fixes run per prose fragment, so the answer
      travels down here.
  """

  units: str
  terms: tuple[wordlists.Term, ...]


def _anchored(line: str, term: wordlists.Term) -> bool:
  """Return whether a line carries one of a term's context anchors.

  An anchor is evidence about the line's subject, not part of the
  violation, so it is looked for in the whole line — exempt ranges
  included, since the commonest anchor is a ``secret`` or ``token``
  inside an inline code span. Matching ignores case.

  Args:
    line: One Markdown line outside fenced code blocks.
    term: One wordlist entry.

  Returns:
    True when zh-word-1 may report and fix this term on this line.
  """
  lowered = line.lower()
  return any(anchor.lower() in lowered for anchor in term.anchors)


def _covered(line: str, start: int, allowed: tuple[str, ...]) -> bool:
  """Return whether an allowlist entry covers a hit on this line.

  「覆盖」 of ``spec/rules.md`` 「词表」: an entry counts when one of its
  occurrences holds the hit's first character — 零售 covers the 零 of
  零售价格, 从零 covers the 零 of 从零建一台, so a fixed collocation on
  either side of the hit is expressible in one table. The whole line is
  searched, exempt ranges included, as with zh-word-1's anchors: an allowlist
  entry is evidence about the wording, not a violation.

  Args:
    line: One Markdown line outside fenced code blocks.
    start: Offset of the hit's first character.
    allowed: The rule's allowlist entries.

  Returns:
    True when the hit must not be reported.
  """
  return any(
      line.find(word, max(0, start - len(word) + 1), start + len(word)) != -1
      for word in allowed
  )


def _is_coinage(line: str, start: int) -> bool:
  """Return whether one 零 opens a 零 + noun coinage (zh-tell-5).

  Args:
    line: One Markdown line outside fenced code blocks.
    start: Offset of the 零.

  Returns:
    True when the candidate string — this 零 plus the rest of its CJK
    run — is 2 to 5 characters long and no allowlist entry covers it.
  """
  run = CJK_RUN.match(line, start)
  candidate = run.group(0) if run else ""
  if not ZH_TELL_5_MIN_LENGTH <= len(candidate) <= ZH_TELL_5_MAX_LENGTH:
    return False
  return not _covered(line, start, ALLOWED["zh-tell-5"])


def _fix_line(
    line: str,
    spans: list[tuple[int, int]],
    rules: Collection[str],
    units: str,
) -> str:
  """Return one line with violations auto-fixed outside exempt ranges.

  Args:
    line: One Markdown line outside fenced code blocks.
    spans: Inline code interiors on this line, from ``_protected``.
    rules: The enabled rule ids; disabled rules leave the text alone.
    units: ``skip_zh_units``, the measure words exempting
      zh-typography-5 boundaries.

  Returns:
    The fixed line; inline code, URL and kana quote ranges are copied
    through verbatim.
  """
  active = TERMS if "zh-word-1" in rules else ()
  ctx = FixContext(units, tuple(t for t in active if _anchored(line, t)))
  ranges = [(a, b, True) for a, b in spans]
  ranges += [(a, b, False) for a, b in _prose_spans(line, spans)]
  ranges.sort()
  parts: list[str] = []
  pos = 0
  closes = False
  for start, end, is_code in ranges:
    opens = is_code and start > pos and line[start - 1] == "`"
    parts.append(_fix_frag(line[pos:start], rules, ctx, closes, opens))
    parts.append(line[start:end])
    pos = end
    closes = is_code and end < len(line) and line[end] == "`"
  parts.append(_fix_frag(line[pos:], rules, ctx, closes, False))
  return "".join(parts)


def _fix_frag(
    frag: str,
    rules: Collection[str],
    ctx: FixContext,
    closes: bool,
    opens: bool,
) -> str:
  """Return one prose fragment fixed, its span delimiters included.

  A fragment between two inline code spans starts with the left span's
  closing delimiter run and ends with the right span's opening run; only
  ``_fix_line`` knows which backticks are delimiters, so zh-typography-7 spacing
  happens here and not in ``_fix_prose``.

  Args:
    frag: Prose between two code interiors (delimiter runs included).
    rules: The enabled rule ids; disabled rules leave the text alone.
    ctx: The line's fix context (``skip_zh_units``, active zh-word-1 terms).
    closes: Whether the fragment starts with a closing delimiter run.
    opens: Whether the fragment ends with an opening delimiter run.

  Returns:
    The fixed fragment.
  """
  frag = _fix_prose(frag, rules, ctx)
  if "zh-typography-7" in rules:
    if closes:
      frag = ZH_TYPOGRAPHY_7_CLOSE_EDGE.sub(r"\1 ", frag)
    if opens:
      frag = ZH_TYPOGRAPHY_7_OPEN_EDGE.sub(r" \1", frag)
  return frag


def _abbrev_dots(line: str) -> set[int]:
  """Return the positions of dots inside abbreviation occurrences.

  Args:
    line: Markdown text.

  Returns:
    0-based indexes of every ``.`` within a listed abbreviation.
  """
  return {
      m.start() + i
      for m in ABBREV.finditer(line)
      for i, ch in enumerate(m.group(0))
      if ch == "."
  }


def _cjk(ch: str) -> bool:
  """Return whether a character is CJK.

  Args:
    ch: A single character, or the empty string for a missing neighbour.

  Returns:
    True for a character in the CJK range.
  """
  return bool(CJK_CHAR.match(ch)) if ch else False


def _fix_zh_typography_1(line: str) -> str:
  """Return the fragment with CJK-adjacent half-width punct full-width.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
  abbrev = _abbrev_dots(line)
  out: list[str] = []
  chars = list(line)
  for i, ch in enumerate(chars):
    prev = chars[i - 1] if i > 0 else ""
    nxt = chars[i + 1] if i + 1 < len(chars) else ""
    if ch in PUNCT_MAP and (_cjk(prev) or _cjk(nxt)):
      out.append(PUNCT_MAP[ch])
      continue
    if (
        ch == "."
        and (_cjk(prev) or _cjk(nxt))
        and prev != "."
        and not (nxt and re.match(r"[A-Za-z0-9.]", nxt))
        and i not in abbrev
    ):
      out.append("。")
      continue
    out.append(ch)
  return "".join(out)


def _fix_zh_typography_11(line: str) -> str:
  """Return the fragment with spaces next to full-width punct removed.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
  line = SPACE_AFTER_FW_FIX.sub("", line)
  return SPACE_BEFORE_FW_FIX.sub("", line)


def _fix_zh_typography_3(line: str) -> str:
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


def _unit_skips(text: str, units: str) -> set[int]:
  """Return the zh-typography-5 boundary offsets exempted by ``skip_zh_units``.

  Args:
    text: One line or prose fragment; offsets are relative to it.
    units: The listed measure-word characters, empty for no exemption.

  Returns:
    Offsets of the exempt zero-width boundaries: both ends of every digit
    run that a listed measure word directly follows.
  """
  if not units:
    return set()
  return {
      pos
      for m in NUMBER_RUN.finditer(text)
      if m.end() < len(text) and text[m.end()] in units
      for pos in (m.start(), m.end())
  }


def _fix_zh_word_1(line: str, ctx: FixContext) -> str:
  """Return the fragment with the line's anchored wrong terms replaced.

  Args:
    line: Markdown text containing no code.
    ctx: The line's fix context; only its anchored terms are applied, so
      a line without the anchors of an entry keeps its wording.

  Returns:
    The fixed fragment.
  """
  for term in ctx.terms:
    line = line.replace(term.wrong, term.right)
  return line


def _fix_zh_typography_5(line: str, ctx: FixContext) -> str:
  """Return the fragment with CJK-to-digit spaces inserted.

  Args:
    line: Markdown text containing no code.
    ctx: The line's fix context; its ``units`` exempt zh-typography-5
      boundaries.

  Returns:
    The fixed fragment.
  """
  skips = _unit_skips(line, ctx.units)
  return CJK_DIGIT_BOUNDARY.sub(
      lambda m: "" if m.start() in skips else " ", line
  )


def _fix_zh_typography_8(line: str) -> str:
  """Return the fragment with dash-side spaces inserted.

  Args:
    line: Markdown text containing no code.

  Returns:
    The fixed fragment.
  """
  line = DASH_LEFT_FIX.sub(" ", line)
  return DASH_RIGHT_FIX.sub(" ", line)


# The per-fragment fix pipeline in the fix order of spec/rules.md 「修复顺序」:
# wording first (zh-word-1), then the width conversions, the space-inserting
# rules in id order, and the space-removing zh-typography-11 last.
# zh-typography-7 is missing because only _fix_line knows which backticks
# delimit a span. Every step takes the line's FixContext, but only zh-word-1
# and zh-typography-5 have a use for it.
_PROSE_FIXES: list[tuple[str, typing.Callable[[str, FixContext], str]]] = [
    ("zh-word-1", _fix_zh_word_1),
    ("zh-typography-10", lambda line, _ctx: line.translate(HALFWIDTH_DIGITS)),
    (
        "zh-typography-2",
        lambda line, _ctx: line.replace("（", "(").replace("）", ")"),
    ),
    ("zh-typography-1", lambda line, _ctx: _fix_zh_typography_1(line)),
    ("zh-typography-3", lambda line, _ctx: _fix_zh_typography_3(line)),
    ("zh-typography-4", lambda line, _ctx: CJK_LATIN_BOUNDARY.sub(" ", line)),
    ("zh-typography-5", _fix_zh_typography_5),
    ("zh-typography-6", lambda line, _ctx: NUMBER_UNIT.sub(r"\1 \2", line)),
    ("zh-typography-8", lambda line, _ctx: _fix_zh_typography_8(line)),
    ("zh-typography-9", lambda line, _ctx: LINK_AFTER_CJK_FIX.sub(" ", line)),
    ("zh-typography-11", lambda line, _ctx: _fix_zh_typography_11(line)),
]


def _fix_prose(line: str, rules: Collection[str], ctx: FixContext) -> str:
  """Return a prose fragment with formatting violations auto-fixed.

  Args:
    line: Markdown text containing no code.
    rules: The enabled rule ids; disabled rules leave the text alone.
    ctx: The line's fix context (``skip_zh_units``, active zh-word-1 terms).

  Returns:
    The fixed fragment.
  """
  for rule, fix in _PROSE_FIXES:
    if rule in rules:
      line = fix(line, ctx)
  return line


def fix_text(
    text: str,
    rules: Collection[str] = DEFAULT_RULES,
    skip_zh_units: str = "",
) -> str:
  """Return the text with formatting violations auto-fixed.

  Fenced code blocks and inline code spans are left untouched (code keeps
  half-width punctuation), and the inline directives of
  :mod:`limae.directives` narrow the enabled set line by line; an
  unknown rule id in one raises ``directives.DirectiveError``.

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to the default-enabled set.
    skip_zh_units: Measure words whose digit runs are exempt from
      zh-typography-5; defaults to no exemption.

  Returns:
    The content with full-width parens replaced, CJK-adjacent punctuation
    converted to full-width, and spacing inserted; a fixpoint, since one
    pass can make new determinations true (e.g. zh-typography-2 turning
    a full-width paren into the closer of a link destination shifts the
    exempt ranges), so the pass repeats until the text is stable.
  """
  enabled = frozenset(rules)
  while True:
    lines = text.split("\n")
    protected = _protected(lines)
    masks = _rule_masks(lines, protected)
    fixed = "\n".join(
        line
        if spans is None
        else _fix_line(line, spans, enabled - mask, skip_zh_units)
        for line, spans, mask in zip(lines, protected, masks, strict=True)
    )
    if fixed == text:
      return text
    text = fixed


class _LineScan(typing.NamedTuple):
  """What checking one line needs beyond the line and the pattern.

  Attributes:
    line: The line itself, for the checks reading its whole text (zh-word-1's
      anchors).
    code_spans: Inline code interiors on this line, from ``_protected``.
    exempt: Every exempt range on this line, code interiors included.
    unit_skips: zh-typography-5 boundaries ``skip_zh_units`` exempts.
    token_parens: Offsets of the ``(`` inside an English token
      (zh-typography-3).
    abbrev_dots: Offsets of the dots inside abbreviations (zh-typography-1).
  """

  line: str
  code_spans: list[tuple[int, int]]
  exempt: list[tuple[int, int]]
  unit_skips: set[int]
  token_parens: set[int]
  abbrev_dots: set[int]


def _suppressed(
    m: re.Match[str], rule: str, pattern: re.Pattern[str], scan: _LineScan
) -> bool:
  """Return whether a pattern match is not a violation after all.

  Args:
    m: A match of one of the ``CHECKS`` patterns.
    rule: The rule id the pattern belongs to.
    pattern: The pattern itself, telling the same rule's checks apart.
    scan: What this line's checks share.

  Returns:
    True when the match must not be reported.
  """
  if _exempt(m, scan.exempt):
    return True  # inline code, URL or kana quote range
  if rule == "zh-typography-5" and m.start() in scan.unit_skips:
    return True  # a skip_zh_units boundary
  if m.group(0).endswith("(") and m.end() - 1 in scan.token_parens:
    return True  # paren inside an English token, e.g. word(s), 401(k)
  if rule == "zh-typography-7" and not _is_delimiter_run(
      m, pattern, scan.code_spans
  ):
    return True  # unpaired backticks are plain text, not a span
  if pattern is ZH_TYPOGRAPHY_1_DOT and m.start() in scan.abbrev_dots:
    return True  # a dot of e.g. / Dr. / ... stays half-width
  if rule == "zh-tell-5" and not _is_coinage(scan.line, m.start()):
    return True  # a lone 零, a whole sentence, or an allowlisted word
  if rule == "zh-word-2" and _covered(
      scan.line, m.start(), ALLOWED["zh-word-2"]
  ):
    return True  # 秘密 in its own sense, e.g. 保守秘密
  term = TERM_PATTERNS.get(pattern)
  return term is not None and not _anchored(scan.line, term)


def check_text(
    text: str,
    rules: Collection[str] = DEFAULT_RULES,
    skip_zh_units: str = "",
) -> list[Finding]:
  """Check Markdown text and return its violations in reading order.

  The inline directives of :mod:`limae.directives` narrow the enabled
  set line by line; an unknown rule id in one raises
  ``directives.DirectiveError``.

  Args:
    text: Raw Markdown content.
    rules: The enabled rule ids; defaults to the default-enabled set.
    skip_zh_units: Measure words whose digit runs are exempt from
      zh-typography-5; defaults to no exemption.

  Returns:
    One ``Finding`` per violation, ordered by line then by rule id.
  """
  findings: list[Finding] = []
  enabled = frozenset(rules)
  lines = text.splitlines()
  protected = _protected(lines)
  for lineno, (line, code_spans, mask) in enumerate(
      zip(lines, protected, _rule_masks(lines, protected), strict=True), 1
  ):
    if code_spans is None:
      continue
    line_rules = enabled - mask
    scan = _LineScan(
        line,
        code_spans,
        code_spans + _prose_spans(line, code_spans),
        _unit_skips(line, skip_zh_units),
        {t.start() for t in ENGLISH_TOKEN_PAREN.finditer(line)},
        _abbrev_dots(line),
    )
    line_findings: list[tuple[int, int, Finding]] = []
    for rule, name, pattern in CHECKS:
      if rule not in line_rules:
        continue
      for m in pattern.finditer(line):
        if _suppressed(m, rule, pattern, scan):
          continue
        snippet = line[max(0, m.start() - 12) : m.end() + 12]
        line_findings.append(
            (RULE_ORDER[rule], m.start(), Finding(lineno, rule, name, snippet))
        )
        if rule in ONCE_PER_LINE:
          break  # these phrases cluster; one finding per line is enough
    line_findings.sort(key=lambda t: (t[0], t[1]))
    findings.extend(f for _, _, f in line_findings)
  return findings


def _is_delimiter_run(
    m: re.Match[str],
    pattern: re.Pattern[str],
    code_spans: list[tuple[int, int]],
) -> bool:
  """Return whether an zh-typography-7 backtick-run match delimits a code span.

  Args:
    m: A match of ``ZH_TYPOGRAPHY_7_OPEN`` or ``ZH_TYPOGRAPHY_7_CLOSE``
      (a full backtick run).
    pattern: The pattern the match came from, to tell open from close.
    code_spans: Inline code interiors on this line, from ``_protected``.

  Returns:
    True when the run ends exactly where a span interior starts (open) or
    starts exactly where one ends (close).
  """
  if pattern is ZH_TYPOGRAPHY_7_OPEN:
    return any(m.end() == a for a, _ in code_spans)
  return any(m.start() == b for _, b in code_spans)


def _severity(rule: str, overrides: Mapping[str, str]) -> str:
  """Return the severity one violation is reported at.

  Args:
    rule: The rule id of the violation.
    overrides: This run's ``severity`` config key.

  Returns:
    ``error`` or ``warning``: the spec's default for the rule unless the
    configuration overrides it.
  """
  return overrides.get(rule, GRADES[rule].severity)


def check_file(
    path: pathlib.Path, settings: config.Settings
) -> list[tuple[str, str]]:
  """Check one file and return its violations with their severities.

  Args:
    path: Markdown file to scan.
    settings: The resolved configuration of this run.

  Returns:
    One ``(severity, description)`` pair per violation, the description a
    human-readable ``file:line`` line; empty when the file is clean.
  """
  problems: list[tuple[str, str]] = []
  for f in check_text(
      path.read_text(encoding="utf-8"),
      settings.rules,
      settings.skip_zh_units,
  ):
    severity = _severity(f.rule, settings.severity)
    problems.append(
        (severity, f"{path}:{f.line}: {severity}: [{f.name}] …{f.snippet}…")
    )
  return problems


def tracked_markdown() -> list[pathlib.Path]:
  """Return the git-tracked Markdown files.

  Returns:
    Paths relative to the repository root.
  """
  out = subprocess.run(
      ["git", "ls-files", "*.md"], capture_output=True, text=True, check=True
  )
  return [pathlib.Path(p) for p in out.stdout.splitlines()]


def _fix_in_place(path: pathlib.Path, settings: config.Settings) -> None:
  """Rewrite one file with its violations auto-fixed, if any change.

  Args:
    path: Markdown file to fix.
    settings: The resolved configuration of this run.
  """
  src = path.read_text(encoding="utf-8")
  dst = fix_text(src, settings.rules, settings.skip_zh_units)
  if src != dst:
    _ = path.write_text(dst, encoding="utf-8")
    print(f"fixed: {path}")


def main() -> int:
  """Run the checker CLI.

  ``limae polish`` is dispatched to :mod:`limae.polish` and ``limae
  hook`` to :mod:`limae.hook`; every other invocation is the checker,
  whose exit codes are unchanged. The other two subcommands of ADR-0008
  section 二 (``check`` and ``format``) are not split out yet, so the
  bare form is still the checker.

  Returns:
    Process exit code (``spec/rules.md`` section 「退出码」): 0 when clean
    or only warnings remain, 1 when an error-level violation was found,
    2 when the configuration or an inline directive is invalid. The
    ``polish`` and ``hook`` subcommands bring their own
    (:func:`limae.polish.main`, :func:`limae.hook.main`).
  """
  if sys.argv[1:2] == [polish.SUBCOMMAND]:
    return polish.main(sys.argv[2:])
  if sys.argv[1:2] == [hook.SUBCOMMAND]:
    return hook.main(sys.argv[2:])

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
    settings = config.resolve(
        args.disable,
        args.enable,
        pathlib.Path.cwd(),
        ALL_RULES,
        DEFAULT_RULES,
        EXPERIMENTAL_RULES,
    )
  except config.ConfigError as e:
    print(f"config error: {e}", file=sys.stderr)
    return 2

  paths = (
      tracked_markdown() if args.all else [pathlib.Path(f) for f in args.files]
  )
  if not paths:
    ap.error("no files given (use --all or list files)")
  paths = config.not_ignored(paths, pathlib.Path.cwd())

  all_problems: list[tuple[str, str]] = []
  for path in paths:
    try:
      if args.fix:
        _fix_in_place(path, settings)
      all_problems.extend(check_file(path, settings))
    except directives.DirectiveError as e:
      print(f"directive error: {path}:{e}", file=sys.stderr)
      return 2

  if all_problems:
    print("\n".join(message for _, message in all_problems))
    errors = sum(1 for s, _ in all_problems if s == config.ERROR)
    print(
        f"\n{errors} error(s), {len(all_problems) - errors} warning(s)."
        " --fix auto-fixes most."
    )
    return 1 if errors else 0
  print(f"OK: {len(paths)} file(s) clean")
  return 0


if __name__ == "__main__":
  sys.exit(main())
