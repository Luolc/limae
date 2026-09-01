"""Wordlists: the shared data behind the A and T rule families.

``spec/rules.md`` section 「词表」 is the normative description. The files
live in ``spec/wordlists/`` because they are part of the spec, not of this
implementation — every implementation reads the same ones, so changing a
wordlist changes no code. ``src/lo_md_lint/wordlists`` is a directory
symlink to that directory, which both the editable install and the built
wheel resolve, so :mod:`importlib.resources` finds the files either way.

``A1.txt`` / ``A3.txt`` / ``A4.txt`` are one phrase per line, ``#``
comments and blank lines ignored; ``T1.toml`` is an ``entries`` array of
``wrong`` / ``right`` / ``anchors`` tables.
"""

import importlib.resources
import tomllib
import typing

PACKAGE = "lo_md_lint"
DIRECTORY = "wordlists"
TERMS_FILE = "T1.toml"
COMMENT = "#"


class Term(typing.NamedTuple):
  """One terminology entry of ``spec/wordlists/T1.toml``.

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
    rule: The rule id, which is also the file's stem (``A1``).

  Returns:
    The listed phrases in file order, comments and blank lines dropped.
  """
  lines = (line.strip() for line in _read(f"{rule}.txt").splitlines())
  return tuple(line for line in lines if line and not line.startswith(COMMENT))


def terms() -> tuple[Term, ...]:
  """Return the terminology entries of ``T1.toml``.

  Returns:
    The entries in file order.
  """
  entries = tomllib.loads(_read(TERMS_FILE))["entries"]
  return tuple(
      Term(e["wrong"], e["right"], tuple(e["anchors"])) for e in entries
  )
