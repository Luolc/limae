"""The ``polish`` subcommand: LLM semantic rewriting, stdin to stdout.

``docs/adr/0008-limae-polish-cli.md`` is the normative description.
``polish`` is the third of the three subcommands (section 二) and the
whole semantic half of the two-stage boundary: ``check`` reports,
``format`` fixes typography by rule, ``polish`` lets a model rewrite the
prose. It never runs in a required check (section 六).

The prompt spec has the three layers of section 九: one general layer in
English, plus one distilled layer per language, written in that language.
Both live in ``spec/polish/`` because they are spec, not implementation —
``src/limae/prompts`` is a directory symlink to it, the same trick
``wordlists`` uses. The wordlists themselves stay out of the prompt
(ADR-0007, ADR-0008 section 九): they are executable data for the T and A
families, and a model handed a word list starts substituting mechanically.

Failure is loud (section 六): a CLI run that cannot polish says which
step failed and exits non-zero. The hook form, which fails open and keeps
the user's text, is a later task.
"""

import argparse
from collections.abc import Mapping, Sequence
import importlib.resources
import os
import pathlib
import re
import sys

from limae import config, engines

SUBCOMMAND = "polish"
STDIN_ARGUMENT = "-"
PACKAGE = "limae"
DIRECTORY = "prompts"
GENERAL_SPEC = "general.md"
# One distilled spec per language (ADR-0008 section 九). A text is Chinese
# when it has a CJK character in it; that is the only language question
# this repository's rules ask too (`spec/rules.md` 「CJK 与 word 字符」).
CHINESE_SPEC = "zh.md"
CJK = re.compile("[一-鿿]")
# The environment variable of step 1 of the `auto` search.
ENGINE_VARIABLE = "LIMAE_ENGINE"

OK = 0
FAILED = 1
BAD_USAGE = 2


def _read(name: str) -> str:
  """Read one prompt-spec file.

  Args:
    name: File name inside ``spec/polish/``.

  Returns:
    The file's text.
  """
  resource = importlib.resources.files(PACKAGE).joinpath(DIRECTORY, name)
  return resource.read_text(encoding="utf-8")


def assemble(text: str) -> str:
  """Assemble the prompt spec for one piece of prose.

  Args:
    text: The prose to rewrite; only its language is looked at.

  Returns:
    The general layer, followed by the layer of the text's own language
    when there is one.
  """
  layers = [_read(GENERAL_SPEC)]
  if CJK.search(text):
    layers.append(_read(CHINESE_SPEC))
  return "\n".join(layers)


def _engine(
    flag: str | None, env: Mapping[str, str], settings: config.PolishSettings
) -> str:
  """Decide which engine to use, before any probing.

  The precedence is the command line, then ``LIMAE_ENGINE`` (step 1 of
  ADR-0008 section 三), then the config file, then ``auto``.

  Args:
    flag: The ``--engine`` value, or None.
    env: The environment of the run.
    settings: The resolved ``[polish]`` configuration.

  Returns:
    An engine name, ``custom``, or ``auto`` when nothing was asked for.
  """
  return flag or env.get(ENGINE_VARIABLE, "") or settings.engine


def _parser() -> argparse.ArgumentParser:
  """Build the subcommand's argument parser.

  Returns:
    The parser.
  """
  ap = argparse.ArgumentParser(
      prog=f"limae {SUBCOMMAND}",
      description="rewrite prose with an LLM; reads stdin, writes stdout",
  )
  _ = ap.add_argument(
      "input",
      metavar="-",
      help="the text to polish, read from stdin; files come later",
  )
  _ = ap.add_argument(
      "--engine",
      metavar="ENGINE",
      help=(
          "which engine to run: auto, one of the presets, or custom;"
          " overrides LIMAE_ENGINE and the config file"
      ),
  )
  _ = ap.add_argument(
      "--model",
      metavar="MODEL",
      help="model to run, overriding the preset's default",
  )
  return ap


def main(argv: Sequence[str]) -> int:
  """Run the ``polish`` subcommand.

  Args:
    argv: The arguments after ``polish``.

  Returns:
    Process exit code: 0 when the rewrite reached stdout, 1 when the
    engine did not answer, 2 for bad usage or bad configuration.
  """
  ap = _parser()
  args = ap.parse_args(argv)
  if args.input != STDIN_ARGUMENT:
    ap.error(
        f"only {STDIN_ARGUMENT!r} (stdin) is supported so far;"
        " file arguments come with the next step"
    )

  env = os.environ
  try:
    settings = config.resolve_polish(pathlib.Path.cwd(), engines.PRESETS)
  except config.ConfigError as e:
    print(f"config error: {e}", file=sys.stderr)
    return BAD_USAGE

  text = sys.stdin.read()
  if not text.strip():
    print("input error: nothing on stdin to polish", file=sys.stderr)
    return BAD_USAGE

  engine = _engine(args.engine, env, settings)
  known = [config.AUTO_ENGINE, *engines.ENGINES, config.CUSTOM_ENGINE]
  if engine not in known:
    print(
        f"config error: unknown engine {engine!r};"
        f" pick one of {', '.join(known)}",
        file=sys.stderr,
    )
    return BAD_USAGE
  if engine == config.CUSTOM_ENGINE and not settings.command:
    print(
        "config error: engine 'custom' needs [polish] command,"
        " the whole command to run",
        file=sys.stderr,
    )
    return BAD_USAGE
  if engine == config.AUTO_ENGINE:
    try:
      engine = engines.select(env)
    except engines.EngineError as e:
      print(f"engine error: {e}", file=sys.stderr)
      return FAILED

  try:
    polished = engines.polish(
        engine,
        args.model or settings.model,
        assemble(text),
        text,
        env,
        settings.command,
    )
  except engines.EngineError as e:
    print(f"engine error: {e}", file=sys.stderr)
    return FAILED
  print(polished, end="")
  return OK
