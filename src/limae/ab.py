"""What the display hook did, written down: A/B trials and single runs.

The A/B trial is the elaborate shape — two candidates under one code
name — and most of this module is about it. A turn that was not sampled
is the plain shape: one engine, one rewrite, recorded the same way, in
the same place, under the same permissions. Both are here rather than
split across modules because the guarantee is a property of the pair:
whatever polish does gets written to the session's own directory and
nowhere else, and there is one piece of code that decides that.

``docs/adr/0009-polish-hook-contract.md`` sections 三 to 五 are the
normative description. On a sampled turn the hook runs two models over
the same assistant message and shows both, so that ADR-0008 section 五
— the default model is not frozen, measurement decides it — has
evidence to decide on.

Two properties this module exists to keep:

* **The screen stays blind.** ADR-0008 section 五 asks for 10 to 20
  blind comparisons of real prose, so the two candidates are labelled A
  and B and nothing else. Which model wrote which reaches the model
  through the ``Stop`` hook (ADR-0009 section 五) and the ledger, never
  the screen: a reader who can see the names is no longer judging the
  prose.
* **The ledger never leaves the session.** It holds the assistant's own
  reply, so it is written under the session-state directory the hook
  owns and nowhere else — not this repository, not another agent
  (ADR-0009 sections 五 and 八).

The code names live here rather than in ``spec/``. ``spec/`` is the
contract every implementation of this tool must reproduce — the rules,
the golden fixtures, the prompt layers — and two implementations drawing
different words are both right, because no output anyone compares
depends on which word came up. What the list has to satisfy is a
property of the person, not of the tool: ADR-0009 section 四 picks
two-character Chinese nouns so that feedback given by voice ("the 灯塔
round, B was better") survives speech-to-text.
"""

from collections.abc import Mapping
import concurrent.futures
import datetime
import json
import os
import pathlib
import random
import typing

from limae import engines, polish

# One trial per sampled turn; ADR-0009 section 三 starts at about one
# turn in ten and leaves the final value open.
SAMPLE_RATE = 0.1

LEDGER_DIRECTORY = "ab"
# Where a turn that was not sampled is written down. Separate from the
# A/B ledger because it is keyed by message rather than by code name,
# and because a reader asking "what did polish do to this message" is
# not asking "which trial was this".
RUN_DIRECTORY = "polish"
PENDING_FILENAME = "pending.json"
TEMPORARY_SUFFIX = ".writing"
# The ledger holds an assistant reply verbatim. Nobody but this user
# reads it (ADR-0009 section 八).
FILE_MODE = 0o600
DIRECTORY_MODE = 0o700

LABELS = ("A", "B")

# Two-character Chinese nouns, common enough that speech-to-text gets
# them right and distinct enough from each other that a mistyped
# transcription still names one round only. Extend the list when a
# session runs out of them.
# fmt: off
CODE_NAMES = (
    "灯塔", "山谷", "河流", "森林", "海鸥", "松树", "石桥", "麦田",
    "竹林", "晚霞", "清风", "溪水", "雪山", "月光", "春雨", "沙洲",
    "港湾", "草原", "湖泊", "峡谷", "岛屿", "稻田", "果园", "篝火",
    "铜镜", "陶罐", "木船", "风筝", "灯笼", "钟楼", "石阶", "屋檐",
    "屏风", "门廊", "砚台", "竹简", "罗盘", "铁锚", "帆船", "号角",
    "琥珀", "玛瑙", "青苔", "榆树", "白杨", "芦苇", "荷塘", "银杏",
)
# fmt: on


class Candidate(typing.NamedTuple):
  """One engine and model to try.

  Attributes:
    engine: A preset name from :data:`limae.engines.PRESETS`.
    model: The model to run under that preset.
  """

  engine: str
  model: str


# The seven of ADR-0008 section 五, which is where this pool is decided;
# this module only draws from it.
CANDIDATES = (
    Candidate(engines.CODEX, "gpt-5.6-luna"),
    Candidate(engines.CODEX, "gpt-5.6-terra"),
    Candidate(engines.CODEX, "gpt-5.4"),
    Candidate(engines.GROK, "grok-4.5"),
    Candidate(engines.GROK, "grok-4.6"),
    Candidate(engines.CLAUDE, "haiku"),
    Candidate(engines.CLAUDE, "sonnet"),
)


class Trial(typing.NamedTuple):
  """One sampled turn's A/B trial.

  Attributes:
    code: The round's code name, unique within the session.
    a: The candidate shown as A.
    b: The candidate shown as B.
  """

  code: str
  a: Candidate
  b: Candidate


def pool(env: Mapping[str, str]) -> list[Candidate]:
  """Return the candidates whose CLI is installed.

  Args:
    env: The environment of the run.

  Returns:
    The candidates that could actually run, in the order of
    :data:`CANDIDATES`.
  """
  return [c for c in CANDIDATES if engines.installed(c.engine, env)]


def _used(directory: pathlib.Path) -> set[str]:
  """Return the code names this session has already handed out.

  The ledger is the register: one file per trial, named after the code.

  Args:
    directory: The session-state directory.

  Returns:
    The code names already used.
  """
  ledger = directory / LEDGER_DIRECTORY
  try:
    return {path.stem for path in ledger.glob("*.json")}
  except OSError:
    return set()


def draw(
    directory: pathlib.Path, env: Mapping[str, str], rate: float
) -> Trial | None:
  """Decide whether this turn gets an A/B trial, and with what.

  Args:
    directory: The session-state directory, which is also the register
      of code names already used (ADR-0009 section 四).
    env: The environment of the run.
    rate: The share of turns to sample, from 0 to 1.

  Returns:
    The trial, or None when this turn is an ordinary one — not sampled,
    fewer than two candidates installed, or the session has used every
    code name there is.
  """
  # Sampling, not cryptography: a predictable draw would cost nothing
  # here beyond a less even spread of trials.
  if random.random() >= rate:
    return None
  candidates = pool(env)
  if len(candidates) < 2:
    return None
  free = [name for name in CODE_NAMES if name not in _used(directory)]
  if not free:
    return None
  a, b = random.sample(candidates, 2)
  return Trial(random.choice(free), a, b)


def run(
    trial: Trial, text: str, env: Mapping[str, str], timeout: float
) -> tuple[tuple[str, str] | None, str]:
  """Run both candidates over the same text.

  The two run at once: one after the other would put two model calls
  between the user and their own message.

  The answers come back normalised, because two places need the same
  string: what goes on screen and what goes in the ledger. Stripping in
  the renderer alone made the ledger's copy a different text from the
  displayed one by a trailing newline, which is one fact with two
  versions of itself — enough to make a later comparison of the two
  disagree for no reason.

  Args:
    trial: The trial to run.
    text: The assembled assistant message.
    env: The environment to run the engines in.
    timeout: Seconds to wait for each candidate.

  Returns:
    The two rewrites, A first, and an empty string; or None and the
    reason the first failing candidate gave, which leaves the turn
    showing the original text (ADR-0009 section 六).
  """
  spec = polish.assemble(text)
  with concurrent.futures.ThreadPoolExecutor(max_workers=len(LABELS)) as run_:
    running = [
        run_.submit(
            engines.polish, c.engine, c.model, spec, text, env, (), timeout
        )
        for c in (trial.a, trial.b)
    ]
    answers: list[str] = []
    for future in running:
      try:
        answers.append(future.result().strip())
      except engines.EngineError as e:
        return None, e.reason
  return (answers[0], answers[1]), ""


def render(trial: Trial, shown: tuple[str, str]) -> str:
  """Lay out one trial for the screen.

  The original is not repeated here: it is the text that has just
  streamed, immediately above (ADR-0009 sections 二 and 三). Neither
  model is named — the comparison is blind.

  Args:
    trial: The trial being shown.
    shown: The two rewrites as they will appear, A first, already
      normalised by :func:`run` and fixed by the hook.

  Returns:
    The block to display after the message.
  """
  columns = "\n\n".join(
      f"── {label} ──\n{answer}"
      for label, answer in zip(LABELS, shown, strict=True)
  )
  return f"[A/B {trial.code}] 原文如上，以下是两个候选：\n\n{columns}\n"


def record(
    directory: pathlib.Path,
    trial: Trial,
    original: str,
    answers: tuple[str, str],
    shown: tuple[str, str],
    now: float,
) -> None:
  """Write one trial to the session's ledger.

  Two files: the ledger entry, which is the evidence ADR-0008 section 五
  will be decided on, and the pending file the ``Stop`` hook reads to
  tell the model what just happened (:func:`context`).

  Both versions of each candidate are kept, because they answer
  different questions. ``text`` is what the model wrote, which is what
  section 五 compares — fold the deterministic fixes into it and a model
  that keeps dropping a space beside an inline code span becomes
  indistinguishable from one that never does, which is a selection
  signal erased. ``displayed`` is what the reader saw, without which no
  later reading of the ledger can reproduce the screen.

  Args:
    directory: The session-state directory.
    trial: The trial that ran.
    original: The assistant message as it was written.
    answers: The two rewrites as the models wrote them, A first.
    shown: The same two after the deterministic fixes, A first.
    now: The current time, in seconds since the epoch.
  """
  at = datetime.datetime.fromtimestamp(now, datetime.UTC).isoformat()
  candidates = [
      {
          "label": label,
          "engine": c.engine,
          "model": c.model,
          "text": answer,
          "displayed": display,
      }
      for label, c, answer, display in zip(
          LABELS, (trial.a, trial.b), answers, shown, strict=True
      )
  ]
  ledger = directory / LEDGER_DIRECTORY
  ledger.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
  _write(
      ledger / f"{trial.code}.json",
      {
          "code": trial.code,
          "at": at,
          "original": original,
          "candidates": candidates,
      },
  )
  _write(
      directory / PENDING_FILENAME,
      {"code": trial.code, "at": at, "candidates": candidates},
  )


def record_run(
    directory: pathlib.Path,
    message: str,
    original: str,
    written: str,
    displayed: str,
    engine: Candidate,
    now: float,
) -> None:
  """Write down one un-sampled turn: one engine, one rewrite.

  ``docs/adr/0012-single-run-polish-records.md`` is the normative
  description, including why this is not the "no writing to disk" that
  ADR-0008 section 十 rules out: that phrase is about the user's files
  and about side-channel artefacts needing review, not about a
  session's own scratch.

  Until this existed, a single polish left nothing on disk: what went in
  and what came out could only be recovered from a screenshot. That is
  not merely inconvenient — it makes the polish spec unmeasurable. A
  model's rewriting is not a fixed function; the same input has come
  back anywhere from untouched to stripped of every emphasis marker, so
  a single observation says nothing about a change to the spec. Judging
  one needs a sample, and a sample needs every run on disk.

  The three versions are kept apart for the reason the A/B ledger keeps
  two: ``original`` is what the assistant wrote, ``text`` is what the
  model made of it, ``displayed`` is what the reader saw after the
  deterministic fixes. Folding the fixes into ``text`` would hide how
  much of the tidiness was the model's doing and how much was the rules
  cleaning up after it — which is the question.

  Args:
    directory: The session-state directory.
    message: The sanitised message id, which names the file.
    original: The assistant message as it was written.
    written: The rewrite as the engine returned it.
    displayed: The rewrite as it went on screen.
    engine: The engine and model that ran.
    now: The current time, in seconds since the epoch.
  """
  runs = directory / RUN_DIRECTORY
  runs.mkdir(parents=True, exist_ok=True, mode=DIRECTORY_MODE)
  _write(
      runs / f"{message}.json",
      {
          "at": datetime.datetime.fromtimestamp(now, datetime.UTC).isoformat(),
          "message_id": message,
          "engine": engine.engine,
          "model": engine.model,
          "original": original,
          "text": written,
          "displayed": displayed,
      },
  )


def _write(path: pathlib.Path, entry: dict[str, object]) -> None:
  """Write one JSON file only this user can read.

  The mode is part of the create call, not a chmod after it: between a
  default-mode create and a chmod, a file holding an assistant reply is
  readable by everyone on the machine. It is then renamed into place, so
  the ``Stop`` hook never reads a half-written pending file.

  Args:
    path: The file to write.
    entry: What to write.
  """
  writing = path.with_name(f"{path.name}.{os.getpid()}{TEMPORARY_SUFFIX}")
  descriptor = os.open(writing, os.O_WRONLY | os.O_CREAT | os.O_EXCL, FILE_MODE)
  with os.fdopen(descriptor, "w", encoding="utf-8") as f:
    _ = f.write(json.dumps(entry, ensure_ascii=False, indent=2))
  _ = writing.replace(path)


def context(directory: pathlib.Path) -> str:
  """Return what the model should be told about the trial this turn ran.

  This is ADR-0009 section 五: a ``MessageDisplay`` rewrite is invisible
  to the model, so the code name is handed over here instead — and only
  that, never the rewrites themselves.

  **The model names are deliberately not here, and that is a
  correction.** ADR-0009 section 五 assumed this channel reached the
  model alone. It does not. Measured 2026-09-01 against Claude Code
  ``2.1.258``: a ``Stop`` hook's ``additionalContext`` is wrapped in a
  ``stop_hook_summary`` system message and rendered on screen as
  ``Stop hook feedback: …`` — and the summary is *hidden* when there is
  no additionalContext, so supplying it is precisely what makes it
  visible. The schema's "delivered to the model" says where it goes in
  the context, not that the reader cannot see it. Naming the models here
  therefore printed the answer key next to the blind comparison, which
  is the one thing section 三 asks this not to do. The mapping stays in
  the ledger, which the reader is not looking at.

  **Announced once, and that is load-bearing.** The pending file is
  consumed here. The obvious-looking simplification — always return
  something, since there is always a trial to describe — hangs the
  session: ``additionalContext`` is non-error feedback that continues
  the conversation, so a ``Stop`` hook that always answers re-triggers
  itself. Measured the same day: a hook returning a constant string
  turned "say only: hello" into five turns and counting.

  **It says "this turn", not "the last reply", because a turn holds many
  replies.** ``Stop`` fires once, at the end; the trial ran on one
  assistant message somewhere inside it. In a long agent turn that can
  be an hour and dozens of messages earlier — observed at 47 minutes.
  Claiming it was the previous reply would simply be false, and the code
  name is what the user gives feedback by in any case.

  Args:
    directory: The session-state directory.

  Returns:
    The text for ``additionalContext``, empty when this turn ran no
    trial.
  """
  path = directory / PENDING_FILENAME
  try:
    with path.open("rb") as f:
      pending = json.load(f)
  except (OSError, ValueError):
    return ""
  path.unlink(missing_ok=True)
  if not isinstance(pending, dict):
    return ""
  code = pending.get("code")
  if not isinstance(code, str) or not code:
    return ""
  # Short on purpose: this lands on the user's screen, where the host
  # truncates it.
  return (
      f"limae A/B：本轮有一次 A/B 对照，编号「{code}」，两栏是盲评。"
      f"型号对应在 {directory / LEDGER_DIRECTORY / f'{code}.json'}；"
      "用户按编号给出偏好之前不要说出哪一栏是哪个模型。"
  )
