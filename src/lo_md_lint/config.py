"""Rule-flag configuration: the ``disable`` / ``enable`` keys and sources.

``spec/rules.md`` section 「配置」 is the normative description; this module
is its Python implementation. The enabled set is
``(default | enable) - disable``, so no configuration at all is exactly
the default behaviour.

Sources, highest priority first:

1. The CLI ``--disable`` / ``--enable`` flags (repeatable,
   comma-separated); either one present replaces the config file wholesale.
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
ENABLE_KEY = "enable"


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


def _split_cli(values: Sequence[str] | None) -> list[str]:
  """Split repeatable, comma-separated CLI flag values into rule ids.

  Args:
    values: Raw flag values, or None when the flag was not given.

  Returns:
    The individual rule ids, whitespace stripped.
  """
  return [
      rule.strip()
      for value in values or []
      for rule in value.split(",")
      if rule.strip()
  ]


def _key_list(
    table: dict[str, object], key: str, path: pathlib.Path
) -> list[str]:
  """Read one rule-id list key from a config table.

  Args:
    table: The lo-md-lint config table.
    key: ``disable`` or ``enable``.
    path: The config file, for the error message.

  Returns:
    The listed rule ids, empty when the key is absent.

  Raises:
    ConfigError: The value is not a list of strings.
  """
  raw = table.get(key, [])
  if not isinstance(raw, list) or not all(isinstance(r, str) for r in raw):
    raise ConfigError(f"{path}: `{key}` must be a list of rule ids")
  return [r for r in raw if isinstance(r, str)]


def _exclusive(
    disabled: frozenset[str], enabled: frozenset[str], source: str
) -> None:
  """Reject rule ids listed as both disabled and enabled.

  Args:
    disabled: Ids from ``disable``.
    enabled: Ids from ``enable``.
    source: Where the ids came from, for the error message.

  Raises:
    ConfigError: At least one id appears in both lists.
  """
  both = disabled & enabled
  if both:
    raise ConfigError(
        f"{source}: rule id(s) {', '.join(sorted(both))} in both"
        f" `{DISABLE_KEY}` and `{ENABLE_KEY}`"
    )


def resolve_rules(
    cli_disable: Sequence[str] | None,
    cli_enable: Sequence[str] | None,
    start: pathlib.Path,
    known: Collection[str],
    default: frozenset[str],
) -> frozenset[str]:
  """Return the enabled rule ids for this run.

  The enabled set is ``(default | enable) - disable``. Either CLI flag on
  the command line replaces the config file wholesale; there is no
  per-key merging.

  Args:
    cli_disable: Raw ``--disable`` values, each one or more comma-separated
      rule ids; empty or None means the flag was not given.
    cli_enable: Raw ``--enable`` values, same syntax.
    start: Directory the config-file search starts from, normally the cwd.
    known: Every rule id this implementation knows.
    default: The rule ids enabled when nothing is configured.

  Returns:
    The enabled rule ids.

  Raises:
    ConfigError: Bad toml, a malformed key, an unknown id, or an id in
      both ``disable`` and ``enable``.
  """
  if cli_disable or cli_enable:
    disabled = _checked(_split_cli(cli_disable), known, "--disable")
    enabled = _checked(_split_cli(cli_enable), known, "--enable")
    _exclusive(disabled, enabled, "command line")
    return (default | enabled) - disabled

  found = find_config(start)
  if found is None:
    return default
  path, table = found
  if not isinstance(table, dict):
    raise ConfigError(f"{path}: [tool.{TOOL_TABLE}] must be a table")
  disabled = _checked(_key_list(table, DISABLE_KEY, path), known, str(path))
  enabled = _checked(_key_list(table, ENABLE_KEY, path), known, str(path))
  _exclusive(disabled, enabled, str(path))
  return (default | enabled) - disabled
