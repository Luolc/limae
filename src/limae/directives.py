"""Inline directives: the ``limae-disable`` family of HTML comments.

``spec/rules.md`` section 「行内指令」 is the normative description; this
module is its Python implementation. A directive is an HTML comment alone
on its line and switches rules off for the lines that follow
(``disable`` / ``enable``) or for the next line only
(``disable-next-line``).

The result is one mask per line — the rule ids the directives switch off
there — which the checker subtracts from the run's enabled set. So a
directive can only narrow: ``enable`` restores at most what the
configuration already had on.
"""

from collections.abc import Collection, Sequence
import re

# A directive is an HTML comment alone on its line (whitespace aside), the
# rule ids after its name space- or comma-separated as on the command
# line. The name must be followed by whitespace or the comment's end, so a
# misspelled `limae-disabled` is a plain comment, not a directive.
DIRECTIVE = re.compile(
    r"^\s*<!--\s*limae-(disable-next-line|disable|enable)"
    r"(?:\s+([^>]*?))?\s*-->\s*$"
)
ID_SEPARATOR = re.compile(r"[\s,]+")


class DirectiveError(Exception):
  """A directive the spec does not allow (an unknown rule id)."""


def _ids(
    listed: str | None, known: Collection[str], line: int
) -> frozenset[str]:
  """Return the rule ids a directive names, or all of them when it names none.

  Args:
    listed: The text between the directive name and ``-->``, if any.
    known: Every rule id this implementation knows.
    line: 1-based line number of the directive, for the error message.

  Returns:
    The named rule ids.

  Raises:
    DirectiveError: At least one id is not a known rule id.
  """
  if not listed:
    return frozenset(known)
  ids = [i for i in ID_SEPARATOR.split(listed) if i]
  unknown = [i for i in ids if i not in known]
  if unknown:
    raise DirectiveError(
        f"{line}: unknown rule id(s) {', '.join(unknown)};"
        f" known ids are {', '.join(sorted(known))}"
    )
  return frozenset(ids)


def rule_masks(
    lines: Sequence[str], verbatim: Sequence[bool], known: Collection[str]
) -> list[frozenset[str]]:
  """Return, per line, the rule ids the inline directives switch off there.

  A line-by-line state machine: ``disable`` adds the named ids to the
  switched-off set, ``enable`` removes them, ``disable-next-line`` applies
  to the next line only without touching that set. A directive line is
  itself exempt from every rule, and a pending ``disable-next-line`` that
  the next line does not consume (another directive, end of file) lapses.
  A directive naming an unknown rule id raises ``DirectiveError``.

  Args:
    lines: The Markdown source split into lines.
    verbatim: Per line, whether it is inside a fenced code block (a fence
      line included), where a directive-shaped comment is only code.
    known: Every rule id this implementation knows.

  Returns:
    One mask per line, to subtract from the run's enabled set.
  """
  everything = frozenset(known)
  masks: list[frozenset[str]] = []
  off: frozenset[str] = frozenset()
  pending: frozenset[str] = frozenset()
  for line, (source, is_verbatim) in enumerate(
      zip(lines, verbatim, strict=True), 1
  ):
    match = None if is_verbatim else DIRECTIVE.match(source)
    if match is None:
      masks.append(off | pending)
      pending = frozenset()
      continue
    ids = _ids(match.group(2), known, line)
    pending = frozenset()  # a directive line does not consume the previous one
    if match.group(1) == "disable":
      off |= ids
    elif match.group(1) == "enable":
      off -= ids
    else:
      pending = ids
    masks.append(everything)
  return masks
