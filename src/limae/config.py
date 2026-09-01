"""Configuration: the config keys and the ``.limae-ignore`` file.

``spec/rules.md`` section 「配置」 is the normative description; this module
is its Python implementation. The enabled set is
``((default | experimental) | enable) - disable``, with the experimental
rules joining only under ``enable_experimental``; ``skip_zh_units``
defaults to the empty string and ``severity`` to no override, so no
configuration at all is exactly the default behaviour.

Sources, highest priority first:

1. The CLI ``--disable`` / ``--enable`` flags (repeatable,
   comma-separated); either one present replaces the config file wholesale.
2. The nearest config file, walking up from the current working directory
   to the repository root: ``limae.toml`` (keys at top level), else a
   ``pyproject.toml`` carrying a ``[tool.limae]`` table.

The two file carriers are isomorphic: same keys, different nesting.

The pre-rename spellings are still recognised, with identical semantics.
Which way the two spellings clash differs by file, and the criterion is
whether the state is a config shape that makes long-term sense: two
config sources of the same carrier in one directory can only be a
half-finished migration, so that is a config error, while a leftover
ignore file changes nothing about how the run reads and is simply
ignored (``spec/rules.md`` sections 「发现顺序」 and 「忽略文件」).

The ignore file ``.limae-ignore`` (``spec/rules.md`` section
「忽略文件」) is found by the same upward walk, independently of the config
file, and drops input files by gitignore patterns.

The ``polish`` subcommand has its own sub-table, ``[polish]``, whose
normative description is ``docs/adr/0008-limae-polish-cli.md`` section
三; it is read by :func:`resolve_polish` out of the same carrier, and no
rule key affects it.
"""

from collections.abc import Collection, Mapping, Sequence
import pathlib
import re
import tomllib
import typing

import pathspec

CONFIG_FILENAME = "limae.toml"
PYPROJECT_FILENAME = "pyproject.toml"
IGNORE_FILENAME = ".limae-ignore"
TOOL_TABLE = "limae"
# The pre-rename spellings, still recognised through the transition.
LEGACY_CONFIG_FILENAME = "lo-md-lint.toml"
LEGACY_IGNORE_FILENAME = ".lo-md-lint-ignore"
LEGACY_TOOL_TABLE = "lo-md-lint"
DISABLE_KEY = "disable"
ENABLE_KEY = "enable"
SKIP_ZH_UNITS_KEY = "skip_zh_units"
SEVERITY_KEY = "severity"
ENABLE_EXPERIMENTAL_KEY = "enable_experimental"
# The `polish` sub-table (ADR-0008 section 三): which engine rewrites the
# prose, and with what.
POLISH_TABLE = "polish"
ENGINE_KEY = "engine"
MODEL_KEY = "model"
COMMAND_KEY = "command"
AUTO_ENGINE = "auto"
CUSTOM_ENGINE = "custom"
# The two severities of spec/rules.md 「规则属性」: an error violation
# makes the run fail, a warning one is reported but does not.
ERROR = "error"
WARNING = "warning"
SEVERITIES = frozenset({ERROR, WARNING})
# The spec's CJK class (spec/rules.md 「CJK 与 word 字符」), spelled out here
# rather than imported so that validation does not depend on the checker,
# which imports this module.
ALL_CJK = re.compile("^[一-鿿]*$")


class ConfigError(Exception):
  """Configuration the spec does not allow (bad toml, unknown rule id)."""


class Settings(typing.NamedTuple):
  """The resolved configuration of one run.

  Attributes:
    rules: The enabled rule ids.
    skip_zh_units: Chinese measure-word characters exempted from R5, one
      character per unit; empty means no exemption.
    severity: The user's per-rule severity overrides; a rule absent here
      keeps the severity the spec gives it.
  """

  rules: frozenset[str]
  skip_zh_units: str
  severity: Mapping[str, str]


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


def _standalone(directory: pathlib.Path) -> pathlib.Path | None:
  """Find the standalone config file of one directory.

  Args:
    directory: Directory to look in.

  Returns:
    ``limae.toml``, else the transitional ``lo-md-lint.toml``, else None.

  Raises:
    ConfigError: Both spellings are present.
  """
  current = directory / CONFIG_FILENAME
  legacy = directory / LEGACY_CONFIG_FILENAME
  if current.is_file() and legacy.is_file():
    raise ConfigError(
        f"{directory}: both {CONFIG_FILENAME} and {LEGACY_CONFIG_FILENAME}"
        f" are present; keep only {CONFIG_FILENAME}"
    )
  if current.is_file():
    return current
  return legacy if legacy.is_file() else None


def _tool_table(tools: Mapping[str, object], path: pathlib.Path) -> object:
  """Find this tool's table in a ``pyproject.toml``'s ``[tool]`` table.

  Args:
    tools: The parsed ``[tool]`` table.
    path: The ``pyproject.toml`` it came from, for the error message.

  Returns:
    The table under the current or the transitional name, or None when
    neither is there.

  Raises:
    ConfigError: Both names are present.
  """
  if TOOL_TABLE in tools and LEGACY_TOOL_TABLE in tools:
    raise ConfigError(
        f"{path}: both [tool.{TOOL_TABLE}] and [tool.{LEGACY_TOOL_TABLE}]"
        f" are present; keep only [tool.{TOOL_TABLE}]"
    )
  if TOOL_TABLE in tools:
    return tools[TOOL_TABLE]
  return tools.get(LEGACY_TOOL_TABLE)


def find_config(start: pathlib.Path) -> tuple[pathlib.Path, object] | None:
  """Find the nearest config file at or above a directory.

  Walks up from ``start``, stopping after the directory holding ``.git``
  (the repository root) or at the filesystem root. In one directory the
  standalone file wins over the ``pyproject.toml`` table; a
  ``pyproject.toml`` without the table is not a config source.

  Args:
    start: Directory to start the upward walk from, normally the cwd.

  Returns:
    The config file and its config table, or None when there is none.
  """
  for directory in [start, *start.parents]:
    standalone = _standalone(directory)
    if standalone is not None:
      return standalone, _load_toml(standalone)
    pyproject = directory / PYPROJECT_FILENAME
    if pyproject.is_file():
      tools = _load_toml(pyproject).get("tool")
      if isinstance(tools, dict):
        table = _tool_table(tools, pyproject)
        if table is not None:
          return pyproject, table
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
    table: The config table.
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


def _key_units(table: dict[str, object], path: pathlib.Path) -> str:
  """Read the ``skip_zh_units`` key from a config table.

  Args:
    table: The config table.
    path: The config file, for the error message.

  Returns:
    The measure-word characters, empty when the key is absent.

  Raises:
    ConfigError: The value is not a string of CJK characters.
  """
  raw = table.get(SKIP_ZH_UNITS_KEY, "")
  if not isinstance(raw, str) or not ALL_CJK.match(raw):
    raise ConfigError(
        f"{path}: `{SKIP_ZH_UNITS_KEY}` must be a string of CJK characters,"
        " one per unit"
    )
  return raw


def _key_severity(
    table: dict[str, object], path: pathlib.Path, known: Collection[str]
) -> dict[str, str]:
  """Read the ``severity`` key from a config table.

  Args:
    table: The config table.
    path: The config file, for the error message.
    known: Every rule id this implementation knows.

  Returns:
    The per-rule overrides, empty when the key is absent.

  Raises:
    ConfigError: The value is not a table, names an unknown rule id, or
      gives a severity outside ``error`` / ``warning``.
  """
  raw = table.get(SEVERITY_KEY, {})
  if not isinstance(raw, dict):
    raise ConfigError(
        f"{path}: `{SEVERITY_KEY}` must be a table of rule id = severity"
    )
  overrides: dict[str, str] = dict(raw)
  _ = _checked(list(overrides), known, f"{path}: `{SEVERITY_KEY}`")
  bad = sorted(r for r, v in overrides.items() if v not in SEVERITIES)
  if bad:
    raise ConfigError(
        f"{path}: `{SEVERITY_KEY}` value for {', '.join(bad)} must be"
        f" {ERROR!r} or {WARNING!r}"
    )
  return overrides


def _key_bool(table: dict[str, object], key: str, path: pathlib.Path) -> bool:
  """Read one boolean key from a config table.

  Args:
    table: The config table.
    key: The key to read.
    path: The config file, for the error message.

  Returns:
    The value, False when the key is absent.

  Raises:
    ConfigError: The value is not a boolean.
  """
  raw = table.get(key, False)
  if not isinstance(raw, bool):
    raise ConfigError(f"{path}: `{key}` must be a boolean")
  return raw


def _no_experimental(
    enabled: frozenset[str], experimental: Collection[str], source: str
) -> None:
  """Reject experimental rule ids listed one by one.

  Maturity has a single switch, ``enable_experimental``; neither
  ``enable`` nor ``--enable`` may reach an experimental rule
  (``spec/rules.md`` section 「成熟度的总开关」).

  Args:
    enabled: Ids from ``enable`` or ``--enable``.
    experimental: Every experimental rule id.
    source: Where the ids came from, for the error message.

  Raises:
    ConfigError: At least one id is an experimental rule.
  """
  listed = sorted(enabled & frozenset(experimental))
  if listed:
    raise ConfigError(
        f"{source}: experimental rule id(s) {', '.join(listed)} cannot be"
        f" enabled one by one; use `{ENABLE_EXPERIMENTAL_KEY} = true`"
    )


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


def resolve(
    cli_disable: Sequence[str] | None,
    cli_enable: Sequence[str] | None,
    start: pathlib.Path,
    known: Collection[str],
    default: frozenset[str],
    experimental: frozenset[str],
) -> Settings:
  """Return the settings of this run.

  The enabled set is ``((default | experimental) | enable) - disable``,
  the experimental rules joining only when ``enable_experimental`` is on.
  Either CLI flag on the command line replaces the config file wholesale;
  there is no per-key merging, so ``skip_zh_units``, ``severity`` and
  ``enable_experimental`` fall back to their defaults too.

  Args:
    cli_disable: Raw ``--disable`` values, each one or more comma-separated
      rule ids; empty or None means the flag was not given.
    cli_enable: Raw ``--enable`` values, same syntax.
    start: Directory the config-file search starts from, normally the cwd.
    known: Every rule id this implementation knows.
    default: The rule ids enabled when nothing is configured.
    experimental: The experimental rule ids, which only
      ``enable_experimental`` can add to the enabled set.

  Returns:
    The enabled rule ids, the R5 measure-word exemptions and the severity
    overrides.

  Raises:
    ConfigError: Bad toml, a malformed key, an unknown id, an id in both
      ``disable`` and ``enable``, or an experimental id in ``enable``.
  """
  if cli_disable or cli_enable:
    disabled = _checked(_split_cli(cli_disable), known, "--disable")
    enabled = _checked(_split_cli(cli_enable), known, "--enable")
    _exclusive(disabled, enabled, "command line")
    _no_experimental(enabled, experimental, "--enable")
    return Settings((default | enabled) - disabled, "", {})

  found = find_config(start)
  if found is None:
    return Settings(default, "", {})
  path, table = found
  if not isinstance(table, dict):
    raise ConfigError(f"{path}: [tool.{TOOL_TABLE}] must be a table")
  disabled = _checked(_key_list(table, DISABLE_KEY, path), known, str(path))
  enabled = _checked(_key_list(table, ENABLE_KEY, path), known, str(path))
  _exclusive(disabled, enabled, str(path))
  _no_experimental(enabled, experimental, f"{path}: `{ENABLE_KEY}`")
  base = default
  if _key_bool(table, ENABLE_EXPERIMENTAL_KEY, path):
    base = default | experimental
  return Settings(
      (base | enabled) - disabled,
      _key_units(table, path),
      _key_severity(table, path, known),
  )


class PolishSettings(typing.NamedTuple):
  """The resolved ``[polish]`` configuration of one run.

  Attributes:
    engine: ``auto``, a preset name, or ``custom``.
    model: The model to run; empty means the preset's own default
      (ADR-0008 section 五 does not freeze the defaults).
    command: The whole command to run under ``custom``, placeholders
      included; empty for every other engine.
  """

  engine: str
  model: str
  command: tuple[str, ...]


def _key_str(table: dict[str, object], key: str, path: pathlib.Path) -> str:
  """Read one string key from a config table.

  Args:
    table: The config table.
    key: The key to read.
    path: The config file, for the error message.

  Returns:
    The value, empty when the key is absent.

  Raises:
    ConfigError: The value is not a string.
  """
  raw = table.get(key, "")
  if not isinstance(raw, str):
    raise ConfigError(f"{path}: `{key}` must be a string")
  return raw


def _key_words(
    table: dict[str, object], key: str, path: pathlib.Path
) -> list[str]:
  """Read one list-of-strings key from a config table.

  Args:
    table: The config table.
    key: The key to read.
    path: The config file, for the error message.

  Returns:
    The listed words, empty when the key is absent.

  Raises:
    ConfigError: The value is not a list of strings.
  """
  raw = table.get(key, [])
  if not isinstance(raw, list) or not all(isinstance(w, str) for w in raw):
    raise ConfigError(f"{path}: `{key}` must be a list of strings")
  return [w for w in raw if isinstance(w, str)]


def resolve_polish(
    start: pathlib.Path, engines: Collection[str]
) -> PolishSettings:
  """Return the ``[polish]`` settings of this run.

  The table is found the same way the rule keys are: in ``limae.toml`` it
  is a top-level ``[polish]`` table, in a ``pyproject.toml`` it is
  ``[tool.limae.polish]``. No configuration at all means ``engine =
  "auto"`` with each preset's own default model.

  Args:
    start: Directory the config-file search starts from, normally the cwd.
    engines: The preset engine names this implementation knows.

  Returns:
    The engine, the model override and the ``custom`` command.

  Raises:
    ConfigError: Bad toml, a malformed key, an unknown engine name,
      ``custom`` without a command, or a command without ``custom``.
  """
  found = find_config(start)
  if found is None:
    return PolishSettings(AUTO_ENGINE, "", ())
  path, table = found
  if not isinstance(table, dict):
    raise ConfigError(f"{path}: [tool.{TOOL_TABLE}] must be a table")
  raw = table.get(POLISH_TABLE, {})
  if not isinstance(raw, dict):
    raise ConfigError(f"{path}: [{POLISH_TABLE}] must be a table")
  polish: dict[str, object] = raw
  engine = _key_str(polish, ENGINE_KEY, path) or AUTO_ENGINE
  known = [AUTO_ENGINE, *sorted(engines), CUSTOM_ENGINE]
  if engine not in known:
    raise ConfigError(
        f"{path}: `{ENGINE_KEY}` must be one of {', '.join(known)}"
    )
  command = _key_words(polish, COMMAND_KEY, path)
  if engine == CUSTOM_ENGINE and not command:
    raise ConfigError(
        f'{path}: `{ENGINE_KEY} = "{CUSTOM_ENGINE}"` needs `{COMMAND_KEY}`,'
        " the whole command to run"
    )
  if command and engine != CUSTOM_ENGINE:
    raise ConfigError(
        f"{path}: `{COMMAND_KEY}` only runs under"
        f' `{ENGINE_KEY} = "{CUSTOM_ENGINE}"`'
    )
  return PolishSettings(
      engine, _key_str(polish, MODEL_KEY, path), tuple(command)
  )


def _find_ignore(start: pathlib.Path) -> pathlib.Path | None:
  """Find the nearest ignore file at or above a directory.

  Walks up like ``find_config`` but independently of it: a directory
  holding a config file and no ignore file does not end the walk. In one
  directory ``.limae-ignore`` wins over the transitional
  ``.lo-md-lint-ignore``, and a leftover old file is not an error.

  Args:
    start: Directory to start the upward walk from, normally the cwd.

  Returns:
    The ignore file, or None when there is none.
  """
  for directory in [start, *start.parents]:
    for name in (IGNORE_FILENAME, LEGACY_IGNORE_FILENAME):
      candidate = directory / name
      if candidate.is_file():
        return candidate
    if (directory / ".git").exists():
      break
  return None


def not_ignored(
    paths: Sequence[pathlib.Path], start: pathlib.Path
) -> list[pathlib.Path]:
  """Return the input paths the nearest ignore file does not exclude.

  The patterns are gitignore syntax, relative to the ignore file's own
  directory; a path outside that directory is never ignored. Filtering
  happens on every input file, whether it came from ``--all`` or from the
  command line, because passing files explicitly is how pre-commit runs.

  Args:
    paths: The input files of this run.
    start: Directory the ignore-file search starts from, normally the cwd.

  Returns:
    The paths to check, in the order given.
  """
  found = _find_ignore(start)
  if found is None:
    return list(paths)
  root = found.parent.resolve()
  spec = pathspec.GitIgnoreSpec.from_lines(
      found.read_text(encoding="utf-8").splitlines()
  )
  return [p for p in paths if not _ignores(spec, root, p)]


def _ignores(
    spec: pathspec.GitIgnoreSpec, root: pathlib.Path, path: pathlib.Path
) -> bool:
  """Return whether the ignore patterns exclude one input file.

  Args:
    spec: The parsed ignore file.
    root: The ignore file's directory, patterns are relative to it.
    path: One input file.

  Returns:
    True when the file must be skipped silently.
  """
  try:
    relative = path.resolve().relative_to(root)
  except ValueError:
    return False
  return spec.match_file(relative.as_posix())
