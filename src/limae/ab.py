"""A/B trials for the display hook: two candidates under one code name.

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
import pathlib
import random
import typing

from limae import engines, polish

# One trial per sampled turn; ADR-0009 section 三 starts at about one
# turn in ten and leaves the final value open.
SAMPLE_RATE = 0.1

LEDGER_DIRECTORY = "ab"
PENDING_FILENAME = "pending.json"
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
) -> tuple[str, str] | None:
  """Run both candidates over the same text.

  The two run at once: one after the other would put two model calls
  between the user and their own message.

  Args:
    trial: The trial to run.
    text: The assembled assistant message.
    env: The environment to run the engines in.
    timeout: Seconds to wait for each candidate.

  Returns:
    The two rewrites, A first; None when either candidate failed, which
    leaves the turn showing the original text (ADR-0009 section 六).
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
        answers.append(future.result())
      except engines.EngineError:
        return None
  return answers[0], answers[1]


def render(trial: Trial, answers: tuple[str, str]) -> str:
  """Lay out one trial for the screen.

  The original is not repeated here: it is the text that has just
  streamed, immediately above (ADR-0009 sections 二 and 三). Neither
  model is named — the comparison is blind.

  Args:
    trial: The trial being shown.
    answers: The two rewrites, A first.

  Returns:
    The block to display after the message.
  """
  columns = "\n\n".join(
      f"── {label} ──\n{answer.strip()}"
      for label, answer in zip(LABELS, answers, strict=True)
  )
  return f"[A/B {trial.code}] 原文如上，以下是两个候选：\n\n{columns}\n"


def record(
    directory: pathlib.Path,
    trial: Trial,
    original: str,
    answers: tuple[str, str],
    now: float,
) -> None:
  """Write one trial to the session's ledger.

  Two files: the ledger entry, which is the evidence ADR-0008 section 五
  will be decided on, and the pending file the ``Stop`` hook reads to
  tell the model what just happened (:func:`context`).

  Args:
    directory: The session-state directory.
    trial: The trial that ran.
    original: The assistant message as it was written.
    answers: The two rewrites, A first.
    now: The current time, in seconds since the epoch.
  """
  at = datetime.datetime.fromtimestamp(now, datetime.UTC).isoformat()
  candidates = [
      {"label": label, "engine": c.engine, "model": c.model, "text": answer}
      for label, c, answer in zip(
          LABELS, (trial.a, trial.b), answers, strict=True
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


def _write(path: pathlib.Path, entry: dict[str, object]) -> None:
  """Write one JSON file only this user can read.

  Args:
    path: The file to write.
    entry: What to write.
  """
  _ = path.write_text(
      json.dumps(entry, ensure_ascii=False, indent=2), encoding="utf-8"
  )
  path.chmod(FILE_MODE)


def context(directory: pathlib.Path) -> str:
  """Return what the model should be told about the turn that just ended.

  This is the whole of ADR-0009 section 五: a ``MessageDisplay`` rewrite
  is invisible to the model, so the code name and the two model names
  are handed over here instead — and only these, never the rewrites
  themselves. The pending file is consumed, so one trial is announced
  once.

  Args:
    directory: The session-state directory.

  Returns:
    The text for ``additionalContext``, empty when the turn that just
    ended had no trial.
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
  candidates = pending.get("candidates")
  if not isinstance(code, str) or not isinstance(candidates, list):
    return ""
  named = "，".join(
      f"{c.get('label')} = {c.get('engine')} {c.get('model')}"
      for c in candidates
      if isinstance(c, dict)
  )
  return (
      f"limae A/B：上一条回复做了 A/B 对照，编号「{code}」，{named}。"
      "屏幕上只有 A 与 B 两个标签，没有型号 —— 这是盲评，"
      "用户按编号给出偏好之前不要说出哪一边是哪个模型。"
      f"台账在 {directory / LEDGER_DIRECTORY / f'{code}.json'}。"
  )
