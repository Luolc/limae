"""The ``hook`` subcommand: one rewrite of what is about to be shown.

``docs/adr/0009-polish-hook-contract.md`` is the normative description.
This is P0 of ADR-0008 section 十: the polish spec is evolved against
real prose here, where the text is local, nothing is written to the
repository and no cross-file reference can break.

One process handles one hook event: the payload arrives as JSON on
stdin, at most one JSON object goes to stdout, and the exit code is
always 0.

``MessageDisplay`` fires once per batch of newly completed lines while
an assistant message streams, so this subcommand caches the batches by
``message_id`` and calls a model once, on the batch marked ``final``,
over the whole message (ADR-0009 section 二). Middle batches produce no
output at all, which is how the host displays the original.

**Failure is silence.** A missing engine, a timeout, an empty answer, a
malformed payload, a bug in this file: every one of them ends the same
way, with no ``displayContent`` and the user's own text on screen
(ADR-0009 section 六). This is the opposite of the CLI's contract
(ADR-0008 section 六) and deliberately so — this code sits in front of
every reply the user reads, so the worst thing it can do is get in the
way.

``Stop`` is the other half of the A/B trial (:mod:`limae.ab`): the model
cannot see what was displayed, so the code name and the two model names
are handed to it there.
"""

from collections.abc import Mapping, Sequence
import json
import os
import pathlib
import re
import shutil
import sys
import time

from limae import ab, config, engines, polish

SUBCOMMAND = "hook"
MESSAGE_DISPLAY = "MessageDisplay"
STOP = "Stop"

OK = 0
BAD_USAGE = 2

# The knobs ADR-0009 leaves open, all read from the environment because
# that is what a hook has: Claude Code starts it with the environment of
# the session, and `settings.json` can set these per project.
DISABLE_VARIABLE = "LIMAE_HOOK_DISABLE"
MIN_CHARS_VARIABLE = "LIMAE_HOOK_MIN_CHARS"
RATE_VARIABLE = "LIMAE_HOOK_AB_RATE"
TIMEOUT_VARIABLE = "LIMAE_HOOK_TIMEOUT"
STATE_VARIABLE = "LIMAE_HOOK_STATE"

# Short messages are left alone (ADR-0009 section 二). The starting
# value is Gvozdev's `CLAUDISH_MIN_CHARS`
# (`docs/research/llm-polish-survey.md` section 1.4): non-whitespace
# characters, fenced code blocks not counted.
MIN_CHARS = 200
# Well under the CLI's, because a person is waiting behind this one. The
# host's own default for a MessageDisplay hook is 10 seconds, so the
# hook entry in `settings.json` has to raise its `timeout` past this for
# the model to ever get an answer in
# (`docs/knowledge/polish-hook-self-trial.md`).
TIMEOUT = 60.0

STATE_DIRECTORY = "limae-hook"
PARTS_DIRECTORY = "parts"
PART_SUFFIX = ".part"
TEMPORARY_SUFFIX = ".writing"
# The host starts one of these processes per batch and does not wait for
# it before starting the next (2026-09-01, Claude Code 2.1.257: the
# dispatcher only serialises what the answers do to the screen, not the
# runs). Two consequences, and one line of defence each: a batch is
# renamed into place so a half-written one is never read, and the final
# batch — which knows its own index, and so how many came before it —
# waits this long for a slower sibling to land before giving up and
# polishing what it has.
SIBLING_WAIT = 2.0
SIBLING_POLL = 0.02
# The state directory holds assistant replies (the cached batches, and
# the A/B ledger next to them), so it is this user's alone.
DIRECTORY_MODE = 0o700
# How long a session's state is kept. It is scratch: the batches of a
# finished message are deleted as soon as they are assembled, and what
# is left is the A/B ledger of sessions that have ended.
RETENTION = 24 * 3600.0

# Session and message ids are UUIDs, but they arrive from outside and
# become path segments here, so everything that is not a plain name is
# folded away rather than trusted. A dot is folded too: `..` is a
# perfectly ordinary-looking name that would leave the directory.
UNSAFE = re.compile(r"[^A-Za-z0-9_-]")
NAME_LIMIT = 64

# The fence markers of `spec/rules.md` 「全局豁免」 first item. A message
# that is long only because it carries code is a short message as far as
# polishing is concerned.
FENCES = ("```", "~~~")


def _identifier(value: object) -> str:
  """Turn one id from the payload into a safe path segment.

  Args:
    value: The id as it arrived.

  Returns:
    The id with anything but letters, digits, ``_`` and ``-`` replaced,
    so that no id can name a directory outside the state directory;
    empty when there was no usable id.
  """
  if not isinstance(value, str) or not value:
    return ""
  return UNSAFE.sub("_", value)[:NAME_LIMIT]


def _root(env: Mapping[str, str]) -> pathlib.Path:
  """Return the directory every session's state lives under.

  Args:
    env: The environment of the run.

  Returns:
    The state root, whether or not it exists.
  """
  override = env.get(STATE_VARIABLE)
  if override:
    return pathlib.Path(override)
  return pathlib.Path(env.get("TMPDIR") or "/tmp") / STATE_DIRECTORY


def _session(env: Mapping[str, str], session: str) -> pathlib.Path:
  """Return one session's state directory, creating it.

  Args:
    env: The environment of the run.
    session: The sanitised session id.

  Returns:
    The directory.
  """
  directory = _root(env) / session
  directory.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
  return directory


def _prune(root: pathlib.Path, now: float) -> None:
  """Delete the state of sessions nobody is in any more.

  Args:
    root: The state root.
    now: The current time, in seconds since the epoch.
  """
  try:
    for entry in root.iterdir():
      if entry.is_dir() and now - entry.stat().st_mtime > RETENTION:
        shutil.rmtree(entry, ignore_errors=True)
  except OSError:
    return


def prose_length(text: str) -> int:
  """Count what there is to polish in one message.

  Args:
    text: The whole assistant message.

  Returns:
    The number of non-whitespace characters outside fenced code blocks.
  """
  total = 0
  in_fence = False
  for line in text.splitlines():
    if line.lstrip().startswith(FENCES):
      in_fence = not in_fence
    elif not in_fence:
      total += len("".join(line.split()))
  return total


def _keep(parts: pathlib.Path, index: int, delta: str) -> None:
  """Cache one batch of an unfinished message.

  Args:
    parts: The message's directory.
    index: The batch's index within the message.
    delta: The newly completed lines.
  """
  parts.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
  name = f"{index:06d}"
  writing = parts / f"{name}{TEMPORARY_SUFFIX}"
  _ = writing.write_text(delta, encoding="utf-8")
  # Into place in one step: another batch of the same message may be
  # reading this directory right now (see SIBLING_WAIT).
  _ = writing.replace(parts / f"{name}{PART_SUFFIX}")


def _assemble(parts: pathlib.Path, batches: int, deadline: float) -> str:
  """Put a message back together from its cached batches.

  Args:
    parts: The message's directory.
    batches: How many batches this message had, the final one included.
    deadline: When to stop waiting for the missing ones, on the
      monotonic clock.

  Returns:
    The whole message, batches in index order; whatever arrived in time
    when one of them never did.
  """
  paths = sorted(parts.glob(f"*{PART_SUFFIX}"))
  while len(paths) < batches and time.monotonic() < deadline:
    time.sleep(SIBLING_POLL)
    paths = sorted(parts.glob(f"*{PART_SUFFIX}"))
  return "".join(path.read_text(encoding="utf-8") for path in paths)


def _number(env: Mapping[str, str], variable: str, fallback: float) -> float:
  """Read one numeric knob from the environment.

  Args:
    env: The environment of the run.
    variable: The variable to read.
    fallback: What to use when it is unset or unreadable.

  Returns:
    The value; the fallback when the variable says something that is not
    a number, because a typo in a setting is not a reason to interrupt
    the user.
  """
  try:
    return float(env[variable])
  except (KeyError, ValueError):
    return fallback


def _single(text: str, env: Mapping[str, str], cwd: pathlib.Path) -> str:
  """Polish one message with one engine.

  The engine and model are the ones the ``polish`` configuration names,
  found the same way :mod:`limae.polish` finds them, so a repository
  that has chosen an engine gets it here too.

  Args:
    text: The whole assistant message.
    env: The environment to run the engine in.
    cwd: Directory the configuration is looked up from.

  Returns:
    The rewrite.
  """
  settings = config.resolve_polish(cwd, engines.PRESETS)
  engine = env.get(polish.ENGINE_VARIABLE, "") or settings.engine
  if engine == config.AUTO_ENGINE:
    engine = engines.select(env)
  return engines.polish(
      engine,
      settings.model,
      polish.assemble(text),
      text,
      env,
      settings.command,
      _number(env, TIMEOUT_VARIABLE, TIMEOUT),
  )


def _block(
    text: str,
    directory: pathlib.Path,
    env: Mapping[str, str],
    cwd: pathlib.Path,
) -> str:
  """Build what to show after one finished message.

  Args:
    text: The whole assistant message.
    directory: The session-state directory.
    env: The environment of the run.
    cwd: Directory the configuration is looked up from.

  Returns:
    The block to append, empty when this message is not polished at all
    — too short, or an engine did not answer.
  """
  if prose_length(text) < _number(env, MIN_CHARS_VARIABLE, MIN_CHARS):
    return ""
  trial = ab.draw(directory, env, _number(env, RATE_VARIABLE, ab.SAMPLE_RATE))
  if trial is None:
    return f"── 润色 ──\n{_single(text, env, cwd).strip()}\n"
  answers = ab.run(trial, text, env, _number(env, TIMEOUT_VARIABLE, TIMEOUT))
  if answers is None:
    # One candidate short is not a comparison, and a second round of
    # calls would make the user wait twice; this turn shows its original.
    return ""
  ab.record(directory, trial, text, answers, time.time())
  return ab.render(trial, answers)


def _display(payload: Mapping[str, object], env: Mapping[str, str]) -> str:
  """Handle one ``MessageDisplay`` batch.

  Args:
    payload: The hook input.
    env: The environment of the run.

  Returns:
    What to display in place of this batch, empty to leave it alone.
  """
  session = _identifier(payload.get("session_id"))
  message = _identifier(payload.get("message_id"))
  delta = payload.get("delta")
  if not session or not message or not isinstance(delta, str):
    return ""
  index = payload.get("index")
  if not isinstance(index, int) or isinstance(index, bool) or index < 0:
    return ""
  directory = _session(env, session)
  parts = directory / PARTS_DIRECTORY / message
  _keep(parts, index, delta)
  # `final` is the end-of-message signal whatever the delta holds: the
  # last batch is empty when the message ends on a newline.
  if payload.get("final") is not True:
    return ""
  # Indices are zero-based and increment by one per batch, so the final
  # one says how many there are.
  text = _assemble(parts, index + 1, time.monotonic() + SIBLING_WAIT)
  shutil.rmtree(parts, ignore_errors=True)
  _prune(_root(env), time.time())
  where = payload.get("cwd")
  block = _block(
      text,
      directory,
      env,
      pathlib.Path(where) if isinstance(where, str) else pathlib.Path.cwd(),
  )
  if not block:
    return ""
  separator = "\n\n" if delta.endswith("\n") else "\n\n\n"
  return f"{delta}{separator}{block}"


def _stop(payload: Mapping[str, object], env: Mapping[str, str]) -> str:
  """Handle one ``Stop`` event.

  Args:
    payload: The hook input.
    env: The environment of the run.

  Returns:
    The context to hand the model, empty when the turn that just ended
    had no A/B trial.
  """
  session = _identifier(payload.get("session_id"))
  if not session:
    return ""
  return ab.context(_session(env, session))


def main(argv: Sequence[str]) -> int:
  """Run the ``hook`` subcommand.

  Args:
    argv: The arguments after ``hook``; there are none, because the
      event names itself in the payload.

  Returns:
    Process exit code: always 0 for a hook event, whatever happened
    (ADR-0009 section 六); 2 when a person ran this by hand with
    arguments.
  """
  if argv:
    print(
        f"usage: limae {SUBCOMMAND} — reads one hook event as JSON on stdin",
        file=sys.stderr,
    )
    return BAD_USAGE

  env = os.environ
  if env.get(DISABLE_VARIABLE):
    return OK
  try:
    payload = json.load(sys.stdin)
  except (OSError, ValueError):
    return OK
  if not isinstance(payload, dict):
    return OK

  event = payload.get("hook_event_name")
  try:
    if event == MESSAGE_DISPLAY:
      key, answer = "displayContent", _display(payload, env)
    elif event == STOP:
      key, answer = "additionalContext", _stop(payload, env)
    else:
      return OK
  except Exception:
    # Fail open, and mean it: nothing this file can go wrong at is worth
    # showing the user instead of their own reply (ADR-0009 section 六).
    return OK
  if not answer:
    return OK
  print(
      json.dumps(
          {"hookSpecificOutput": {"hookEventName": event, key: answer}},
          ensure_ascii=False,
      )
  )
  return OK
