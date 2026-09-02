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

**Failure is silence on screen, and only there.** A missing engine, a
timeout, an empty answer, a malformed payload, a bug in this file: every
one of them ends the same way, with no ``displayContent`` and the user's
own text on screen (ADR-0009 section 六). This is the opposite of the
CLI's contract (ADR-0008 section 六) and deliberately so — this code
sits in front of every reply the user reads, so the worst thing it can
do is get in the way.

Each of them also writes one line to ``diagnostics.jsonl`` in the
session's state directory, because failing open and leaving no trace are
two different things and only the first one was ever the intention: a
reply that comes back unpolished should be answerable with "the engine
timed out", not with a guess. The line names the step and the kind of
failure and nothing else — never the prose, never what the engine
printed.

What does reach the screen has been through this repository's own
deterministic fixes first (:func:`_tidy`). ADR-0005 section 四 splits
the work that way: the model settles the words, the rules settle the
typography, and a rewrite is not exempt from the rules just because a
model wrote it.

``Stop`` is the other half of the A/B trial (:mod:`limae.ab`): the model
cannot see what was displayed, so the code name is handed to it there.
Only the code name — that text is rendered on the user's screen too, so
naming the models in it would print the answer key beside a comparison
that is supposed to be blind (ADR-0011). The mapping stays in the
session's ledger.
"""

from collections.abc import Mapping, Sequence
import datetime
import json
import os
import pathlib
import re
import shutil
import sys
import time

from limae import ab, config, engines, polish, zh_format

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
# Where a fail-open path says what it did. Failing open means the user
# is never interrupted, and until now it also meant nobody could find
# out why a reply went unpolished — "no block appeared" was the whole of
# the evidence. This file is the other half of ADR-0009 section 六: the
# user still sees nothing, and whoever is debugging sees everything that
# matters. It holds no prose, no engine output and no credential — the
# step and the kind of failure, which is what a person needs to know
# where to look next.
DIAGNOSTICS_FILENAME = "diagnostics.jsonl"
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
FILE_MODE = 0o600
# How long a session's state is kept. It is scratch: the batches of a
# finished message are deleted as soon as they are assembled, and what
# is left is the A/B ledger of sessions that have ended.
RETENTION = 24 * 3600.0
# How long one message's cached batches are kept when its final batch
# never comes. That happens for real: a message the host abandons
# mid-stream — the session is interrupted, or restarted with `--resume`
# — gets no final flush, so nothing ever assembles it and nothing
# deletes it (2026-09-01, one such message left five batches behind).
# Session retention alone does not reach these, because the session they
# are in is the live one. An hour is orders of magnitude past the
# seconds a message spends streaming, so a sweep can never take the
# batches of a message still arriving.
ORPHAN_RETENTION = 3600.0

# The steps that can fail open, named so a diagnostics line says where
# to look without saying what was being polished.
ASSEMBLE = "assemble"
SINGLE = "single"
AB = "ab"
FIX = "fix"
DISPLAY = "display"
# What went wrong when it was not the engine's doing (`engines.REASONS`
# covers those).
INCOMPLETE = "incomplete"
REPAIRED = "repaired"
MISCONFIGURED = "config"
CRASHED = "crashed"

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

# Newlines between the message and the block: one blank line.
BLOCK_GAP = 2


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


def _in_work_tree(path: pathlib.Path) -> bool:
  """Return whether a path is inside a git checkout.

  Args:
    path: The path to place, existing or not.

  Returns:
    True when the path or one of its ancestors holds a ``.git`` — a
    directory in a normal checkout, a file in a worktree.
  """
  resolved = path.resolve()
  return any(
      (directory / ".git").exists()
      for directory in (resolved, *resolved.parents)
  )


def _root(env: Mapping[str, str]) -> pathlib.Path | None:
  """Return the directory every session's state lives under.

  What is kept there is the assistant's own replies — the cached
  batches, and the A/B ledger beside them — which ADR-0009 sections 五
  and 八 keep to the session and out of any repository. So there is no
  setting for the location: a knob that moves this directory is a knob
  that can point it into a working tree, where the next ``git add`` can
  carry a reply into a public repository. It is the system's scratch
  directory and a fixed name under it, and a project's own configuration
  cannot say otherwise.

  There is no variable of this module's own to move it either, not even
  one meant for the tests: a name is not a boundary — anything that can
  set a hook's environment could set it — and it would take exactly one
  such setting, pointing somewhere persistent, to turn the ledger into
  the long-lived store of replies ADR-0009 section 八 rules out. The
  tests say where the state goes the same way anything else does, by
  saying where scratch files go.

  Args:
    env: The environment of the run.

  Returns:
    The state root, whether or not it exists; None when scratch itself
    is inside a checkout, which leaves the hook with nowhere to put a
    reply and therefore nothing to do.
  """
  root = pathlib.Path(env.get("TMPDIR") or "/tmp") / STATE_DIRECTORY
  return None if _in_work_tree(root) else root


def _session(root: pathlib.Path, session: str) -> pathlib.Path:
  """Return one session's state directory, creating it.

  Args:
    root: The state root.
    session: The sanitised session id.

  Returns:
    The directory. Both it and the root are created with their mode, not
    given it afterwards.
  """
  root.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
  directory = root / session
  directory.mkdir(exist_ok=True, mode=DIRECTORY_MODE)
  return directory


def _note(directory: pathlib.Path, message: str, step: str, kind: str) -> None:
  """Write down that one fail-open path fired.

  Failing open is the right behaviour and a bad witness: the user is not
  interrupted, and nothing anywhere says why their reply came back
  unpolished. One line per failure fixes that without moving the
  boundary — it goes to the session-state directory, never to the
  screen.

  What a line may hold is bounded by the same rule as the ledger
  (ADR-0009 section 八): the message's id, the step, the kind of
  failure. Never the prose, never what an engine printed, never a
  credential — an engine is free to quote its environment back at us,
  and this is a file that outlives the run.

  Args:
    directory: The session-state directory.
    message: The sanitised message id, empty when there is none.
    step: Which step failed; one of :data:`ASSEMBLE`, :data:`SINGLE`,
      :data:`FIX` or :data:`DISPLAY`.
    kind: How it failed; one of :data:`engines.REASONS`,
      :data:`INCOMPLETE`, :data:`REPAIRED`, :data:`MISCONFIGURED` or
      :data:`CRASHED`.
  """
  line = json.dumps(
      {
          "at": datetime.datetime.now(datetime.UTC).isoformat(),
          "message_id": message,
          "step": step,
          "kind": kind,
      },
      ensure_ascii=False,
  )
  try:
    directory.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
    path = directory / DIAGNOSTICS_FILENAME
    # Appended under the mode it is created with, for the reason every
    # other file here has one: a diagnostics line names a session of
    # this user's and nobody else's business.
    descriptor = os.open(
        path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, FILE_MODE
    )
    with os.fdopen(descriptor, "a", encoding="utf-8") as f:
      _ = f.write(f"{line}\n")
  except OSError:
    # A hook that cannot write its own diagnostics still has a reply to
    # get out of the way of.
    return


def _tidy(text: str, cwd: pathlib.Path) -> tuple[str, str]:
  """Run this repository's own deterministic fixes over one rewrite.

  ADR-0005 section 四 draws the line this walks: meaning is the model's
  half, typography is the rules' half. A model rewriting Chinese prose
  reliably drops a space beside an inline code span or a dash — the
  ``zh-typography`` family, fixable and error-level — and until now the
  hook shipped those to the screen, which is the one thing this
  repository exists to stop.

  Only the rewrite goes through here. The original is the user's own
  text and this hook has no business changing it; it has also already
  been displayed, batch by batch, by the time there is anything to fix.

  Args:
    text: The rewrite, as the engine returned it.
    cwd: Directory the rule configuration is looked up from, so a
      repository that has disabled a rule keeps it disabled here.

  Returns:
    The fixed text and an empty string; or the text unchanged and
    :data:`CRASHED` when the fixer would not run — a rewrite with a
    typography slip in it still beats no rewrite at all.
  """
  try:
    settings = config.resolve(
        None,
        None,
        cwd,
        zh_format.ALL_RULES,
        zh_format.DEFAULT_RULES,
        zh_format.EXPERIMENTAL_RULES,
    )
    return (
        zh_format.fix_text(text, settings.rules, settings.skip_zh_units),
        "",
    )
  except Exception:
    return text, CRASHED


def _stale(directory: pathlib.Path, now: float, keep: float) -> bool:
  """Return whether a directory has gone untouched for long enough.

  Args:
    directory: The directory to age.
    now: The current time, in seconds since the epoch.
    keep: How long it is kept, in seconds.

  Returns:
    True when it is older than that, False when it is not or cannot be
    aged.
  """
  try:
    return now - directory.stat().st_mtime > keep
  except OSError:
    return False


def _prune(root: pathlib.Path, now: float) -> None:
  """Delete the state nobody is coming back for.

  Two horizons, because there are two ways state is left behind. A whole
  session goes when nobody has been in it for a day. Inside a session
  that is still live, one message's batches go when its final batch
  never came — the host abandons a message it is killed in the middle
  of, and those batches would otherwise sit there until the session
  itself expired.

  Args:
    root: The state root.
    now: The current time, in seconds since the epoch.
  """
  try:
    sessions = list(root.iterdir())
  except OSError:
    return
  for session in sessions:
    if not session.is_dir():
      continue
    if _stale(session, now, RETENTION):
      shutil.rmtree(session, ignore_errors=True)
      continue
    try:
      orphans = list((session / PARTS_DIRECTORY).iterdir())
    except OSError:
      continue
    for parts in orphans:
      if parts.is_dir() and _stale(parts, now, ORPHAN_RETENTION):
        shutil.rmtree(parts, ignore_errors=True)


def _trailing(text: str) -> int:
  """Count the newlines a piece of text ends on.

  Args:
    text: The text to measure.

  Returns:
    How many newlines run to the end of it, zero when it ends on
    anything else.
  """
  return len(text) - len(text.rstrip("\n"))


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
  # The mode goes in the create call rather than a chmod after it: a
  # batch is a piece of the user's reply, and between a default-mode
  # create and a chmod it is readable by everyone on the machine. The
  # process id keeps the exclusive create exclusive.
  writing = parts / f"{name}.{os.getpid()}{TEMPORARY_SUFFIX}"
  descriptor = os.open(writing, os.O_WRONLY | os.O_CREAT | os.O_EXCL, FILE_MODE)
  with os.fdopen(descriptor, "w", encoding="utf-8") as f:
    _ = f.write(delta)
  # Into place in one step: another batch of the same message may be
  # reading this directory right now (see SIBLING_WAIT).
  _ = writing.replace(parts / f"{name}{PART_SUFFIX}")


def _assemble(
    parts: pathlib.Path,
    batches: int,
    deadline: float,
    directory: pathlib.Path,
    message: str,
) -> str | None:
  """Put a message back together from its cached batches.

  Args:
    parts: The message's directory.
    batches: How many batches this message had, the final one included.
    deadline: When to stop waiting for the missing ones, on the
      monotonic clock.
    directory: The session-state directory, for the diagnostics line.
    message: The sanitised message id.

  Returns:
    The whole message, batches in index order; None when one of them
    never arrived. There is no honest rewrite of a message with a hole
    in it — polishing what did arrive would put a paragraph the user
    never wrote under a message that says it is theirs — so a missing
    batch ends the turn the way every other failure does, with the
    original on screen.
  """
  wanted = [parts / f"{index:06d}{PART_SUFFIX}" for index in range(batches)]
  while not all(path.is_file() for path in wanted):
    if time.monotonic() >= deadline:
      arrived = sum(path.is_file() for path in wanted)
      # The host debug-logs a hook's stderr. How many batches were
      # missing is the whole of what is worth saying; what they held is
      # the user's own text and stays out of every log.
      print(
          f"limae hook: {arrived}/{batches} batches arrived before the"
          " deadline; showing the original",
          file=sys.stderr,
      )
      _note(directory, message, ASSEMBLE, INCOMPLETE)
      return None
    time.sleep(SIBLING_POLL)
  return "".join(path.read_text(encoding="utf-8") for path in wanted)


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


def _shown(
    answers: tuple[str, ...],
    directory: pathlib.Path,
    message: str,
    cwd: pathlib.Path,
) -> tuple[str, ...]:
  """Put every rewrite of one turn through the deterministic fixes.

  Args:
    answers: The rewrites as the models wrote them.
    directory: The session-state directory.
    message: The sanitised message id.
    cwd: Directory the rule configuration is looked up from.

  Returns:
    What to display, in the same order; a rewrite whose fix would not
    run is passed through as it came.
  """
  fixed = [_tidy(answer, cwd) for answer in answers]
  failed = next((why for _, why in fixed if why), "")
  shown = tuple(answer for answer, _ in fixed)
  if failed:
    _note(directory, message, FIX, failed)
  elif shown != answers:
    # Which models need the rules to clean up after them is a selection
    # signal, not only a display fix.
    _note(directory, message, FIX, REPAIRED)
  return shown


def _one(
    text: str,
    directory: pathlib.Path,
    message: str,
    env: Mapping[str, str],
    cwd: pathlib.Path,
) -> str:
  """Build the block of an ordinary turn: one engine, one rewrite.

  Args:
    text: The whole assistant message.
    directory: The session-state directory.
    message: The sanitised message id.
    env: The environment of the run.
    cwd: Directory the configuration is looked up from.

  Returns:
    The block, empty when no engine answered.
  """
  try:
    answer = _single(text, env, cwd)
  except engines.EngineError as e:
    _note(directory, message, SINGLE, e.reason)
    return ""
  except config.ConfigError:
    # A typo in `[polish]` is the user's to fix and says so by name,
    # rather than arriving as a crash of this file.
    _note(directory, message, SINGLE, MISCONFIGURED)
    return ""
  (fixed,) = _shown((answer.strip(),), directory, message, cwd)
  return f"── 润色 ──\n{fixed}\n"


def _trial(
    trial: ab.Trial,
    text: str,
    directory: pathlib.Path,
    message: str,
    env: Mapping[str, str],
    cwd: pathlib.Path,
) -> str:
  """Build the block of a sampled turn: two engines, shown blind.

  Args:
    trial: The trial drawn for this turn.
    text: The whole assistant message.
    directory: The session-state directory.
    message: The sanitised message id.
    env: The environment of the run.
    cwd: Directory the configuration is looked up from.

  Returns:
    The two columns, empty when either candidate did not answer.
  """
  answers, reason = ab.run(
      trial, text, env, _number(env, TIMEOUT_VARIABLE, TIMEOUT)
  )
  if answers is None:
    # One candidate short is not a comparison, and a second round of
    # calls would make the user wait twice; this turn shows its original.
    _note(directory, message, AB, reason)
    return ""
  first, second = _shown(answers, directory, message, cwd)
  try:
    ab.record(directory, trial, text, answers, (first, second), time.time())
  except OSError:
    # The trial is lost as evidence, which is bad; throwing away a
    # comparison the user waited for two model calls to see is worse.
    _note(directory, message, AB, CRASHED)
  return ab.render(trial, (first, second))


def _block(
    text: str,
    directory: pathlib.Path,
    message: str,
    env: Mapping[str, str],
    cwd: pathlib.Path,
) -> str:
  """Build what to show after one finished message.

  Every rewrite that leaves here has been through this repository's own
  deterministic fixes (:func:`_tidy`): the model settles the words, the
  rules settle the typography (ADR-0005 section 四).

  Args:
    text: The whole assistant message.
    directory: The session-state directory.
    message: The sanitised message id, for the diagnostics line.
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
    return _one(text, directory, message, env, cwd)
  return _trial(trial, text, directory, message, env, cwd)


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
  root = _root(env)
  if not session or not message or not isinstance(delta, str) or root is None:
    return ""
  index = payload.get("index")
  if not isinstance(index, int) or isinstance(index, bool) or index < 0:
    return ""
  directory = _session(root, session)
  parts = directory / PARTS_DIRECTORY / message
  _keep(parts, index, delta)
  # `final` is the end-of-message signal whatever the delta holds: the
  # last batch is empty when the message ends on a newline.
  if payload.get("final") is not True:
    return ""
  # Indices are zero-based and increment by one per batch, so the final
  # one says how many there are.
  text = _assemble(
      parts, index + 1, time.monotonic() + SIBLING_WAIT, directory, message
  )
  shutil.rmtree(parts, ignore_errors=True)
  _prune(root, time.time())
  if text is None:
    return ""
  where = payload.get("cwd")
  block = _block(
      text,
      directory,
      message,
      env,
      pathlib.Path(where) if isinstance(where, str) else pathlib.Path.cwd(),
  )
  if not block:
    return ""
  # One blank line, whatever the message happens to end on. The gap is a
  # property of the screen, not of this batch: `displayContent` replaces
  # the final delta and nothing before it, so the trailing newlines an
  # earlier batch already painted cannot be taken back — only counted.
  # A message ending on a newline is exactly that case, since its final
  # delta is empty and every newline is already up there.
  painted = _trailing(text) - _trailing(delta)
  gap = max(0, BLOCK_GAP - painted)
  return f"{delta.rstrip(chr(10))}{chr(10) * gap}{block}"


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
  root = _root(env)
  if not session or root is None:
    return ""
  return ab.context(_session(root, session))


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
    # Silent on screen is not the same as silent everywhere, so a crash
    # says so where the other failures do — best effort, since the state
    # directory is exactly the sort of thing that may be why we are here.
    session = _identifier(payload.get("session_id"))
    root = _root(env)
    if session and root is not None:
      _note(
          root / session,
          _identifier(payload.get("message_id")),
          DISPLAY,
          CRASHED,
      )
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
