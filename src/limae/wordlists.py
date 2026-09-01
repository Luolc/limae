"""Wordlists: the shared data behind the A and T rule families.

``spec/rules.md`` section 「词表」 is the normative description. The files
live in ``spec/wordlists/`` because they are part of the spec, not of this
implementation — every implementation reads the same ones, so changing a
wordlist changes no code. ``src/limae/wordlists`` is a directory
symlink to that directory, which both the editable install and the built
wheel resolve, so :mod:`importlib.resources` finds the files either way.

Every ``.txt`` wordlist is one phrase per line, ``#`` comments and blank
lines ignored; ``zh-word-1.toml`` is an ``entries`` array of ``wrong`` /
``right`` / ``anchors`` tables. The ``-allow`` files (``zh-tell-5-allow.txt``,
``zh-word-2-allow.txt``) have the same format and the opposite polarity: a hit
there exempts rather than reports.
"""

import importlib.resources
import tomllib
import typing

PACKAGE = "limae"
DIRECTORY = "wordlists"
TERMS_FILE = "zh-word-1.toml"
COMMENT = "#"


class Term(typing.NamedTuple):
  """One terminology entry of ``spec/wordlists/zh-word-1.toml``.

  Attributes:
    wrong: The wrong wording, matched as a literal substring.
    right: What ``--fix`` puts in its place.
    anchors: Context words disambiguating the term; a line carrying none
      of them is left alone, so an entry without anchors never matches.
  """

  wrong: str
  right: str
  anchors: tuple[str, ...]


def _read(name: str) -> str:
  """Read one wordlist file.

  Args:
    name: File name inside the wordlist directory.

  Returns:
    The file's text.
  """
  resource = importlib.resources.files(PACKAGE).joinpath(DIRECTORY, name)
  return resource.read_text(encoding="utf-8")


def phrases(rule: str) -> tuple[str, ...]:
  """Return the phrases of one line-oriented wordlist.

  Args:
    rule: The wordlist's file stem — a rule id (``zh-tell-1``) for a wordlist,
      the rule id plus ``-allow`` (``zh-tell-5-allow``) for an allowlist.

  Returns:
    The listed phrases in file order, comments and blank lines dropped.
  """
  lines = (line.strip() for line in _read(f"{rule}.txt").splitlines())
  return tuple(line for line in lines if line and not line.startswith(COMMENT))


def terms() -> tuple[Term, ...]:
  """Return the terminology entries of ``zh-word-1.toml``.

  Returns:
    The entries in file order.
  """
  entries = tomllib.loads(_read(TERMS_FILE))["entries"]
  return tuple(
      Term(e["wrong"], e["right"], tuple(e["anchors"])) for e in entries
  )
