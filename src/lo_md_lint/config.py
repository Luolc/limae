"""Rule-flag configuration: the ``disable`` key and where it comes from.

``spec/rules.md`` section 「配置」 is the normative description; this module
is its Python implementation. Rules are all enabled by default and config
can only turn rules off, so no configuration at all is exactly today's
behaviour.

Sources, highest priority first:

1. The CLI ``--disable`` flag (repeatable, comma-separated).
2. The nearest config file, walking up from the current working directory
   to the repository root: ``lo-md-lint.toml`` (keys at top level), else a
   ``pyproject.toml`` carrying a ``[tool.lo-md-lint]`` table.

The two file carriers are isomorphic: same keys, different nesting.
"""

from collections.abc import Collection, Sequence
import pathlib
import tomllib

CONFIG_FILENAME = "lo-md-lint.toml"
PYPROJECT_FILENAME = "pyproject.toml"
TOOL_TABLE = "lo-md-lint"
DISABLE_KEY = "disable"


class ConfigError(Exception):
  """Configuration the spec does not allow (bad toml, unknown rule id)."""


def _load_toml(path: pathlib.Path) -> dict[str, object]:
  """Parse one toml file.

  Args:
    path: File to read.

  Returns:
    The parsed top-level table.

  Raises:
    ConfigError: The file cannot be read or is not valid toml.
  """
  try:
    with path.open("rb") as f:
      return tomllib.load(f)
  except (OSError, tomllib.TOMLDecodeError) as e:
    raise ConfigError(f"{path}: {e}") from e


def find_config(start: pathlib.Path) -> tuple[pathlib.Path, object] | None:
  """Find the nearest config file at or above a directory.

  Walks up from ``start``, stopping after the directory holding ``.git``
  (the repository root) or at the filesystem root. In one directory the
  standalone file wins over the ``pyproject.toml`` table; a
  ``pyproject.toml`` without the table is not a config source.

  Args:
    start: Directory to start the upward walk from, normally the cwd.

  Returns:
    The config file and its lo-md-lint table, or None when there is none.
  """
  for directory in [start, *start.parents]:
    standalone = directory / CONFIG_FILENAME
    if standalone.is_file():
      return standalone, _load_toml(standalone)
    pyproject = directory / PYPROJECT_FILENAME
    if pyproject.is_file():
      tools = _load_toml(pyproject).get("tool")
      if isinstance(tools, dict) and TOOL_TABLE in tools:
        return pyproject, tools[TOOL_TABLE]
    if (directory / ".git").exists():
      break
  return None


def _checked(
    ids: Sequence[str], known: Collection[str], source: str
) -> frozenset[str]:
  """Validate rule ids against the spec's rule set.

  Args:
    ids: Rule ids to disable, as written by the user.
    known: Every rule id this implementation knows.
    source: Where the ids came from, for the error message.

  Returns:
    The ids as a set.

  Raises:
    ConfigError: At least one id is not a known rule id.
  """
  unknown = [rule for rule in ids if rule not in known]
  if unknown:
    raise ConfigError(
        f"{source}: unknown rule id(s) {', '.join(unknown)};"
        f" known ids are {', '.join(sorted(known))}"
    )
  return frozenset(ids)


def resolve_disabled(
    cli_disable: Sequence[str] | None,
    start: pathlib.Path,
    known: Collection[str],
) -> frozenset[str]:
  """Return the rule ids to turn off for this run.

  ``--disable`` on the command line replaces the config file wholesale;
  there is no per-key merging.

  Args:
    cli_disable: Raw ``--disable`` values, each one or more comma-separated
      rule ids; empty or None to fall back to the config file.
    start: Directory the config-file search starts from, normally the cwd.
    known: Every rule id this implementation knows.

  Returns:
    The disabled rule ids, empty when everything stays enabled.

  Raises:
    ConfigError: Bad toml, a malformed ``disable`` value, or an unknown id.
  """
  if cli_disable:
    ids = [
        rule.strip()
        for value in cli_disable
        for rule in value.split(",")
        if rule.strip()
    ]
    return _checked(ids, known, "--disable")

  found = find_config(start)
  if found is None:
    return frozenset()
  path, table = found
  if not isinstance(table, dict):
    raise ConfigError(f"{path}: [tool.{TOOL_TABLE}] must be a table")
  raw = table.get(DISABLE_KEY, [])
  if not isinstance(raw, list) or not all(isinstance(r, str) for r in raw):
    raise ConfigError(f"{path}: `{DISABLE_KEY}` must be a list of rule ids")
  return _checked(raw, known, str(path))
