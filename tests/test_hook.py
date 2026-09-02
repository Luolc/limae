import io
import json
import os
import pathlib
import re
import sys
import threading
import time

import pytest

from limae import ab, engines, hook, zh_format

# Every engine here is a shell stub in a temporary directory: no test
# reaches the network or calls a real model (AGENTS.md quality bar), and
# every piece of prose is synthetic.
LONG = "ACME 的报告写得不好，请把它改得像人话一些。" * 20
SHORT = "好的。"
POLISHED = "已润色的正文"
FIRST = "甲稿"
SECOND = "乙稿"
SESSION = "11111111-2222-3333-4444-555555555555"
MESSAGE = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
PAIR = (ab.Candidate("claude", "haiku"), ab.Candidate("grok", "grok-4.6"))
HANDBOOK = (
    pathlib.Path(__file__).resolve().parents[1]
    / "docs"
    / "knowledge"
    / "polish-hook-self-trial.md"
)
# A rewrite carrying the slip a model reliably makes in Chinese prose:
# the dash of `zh-typography-8`, fixable and error-level, with the
# spaces eaten off both sides. TIDY is the same sentence already right.
SLOPPY = "台账——A/B 的对照写在这里。"
TIDIED = "台账 —— A/B 的对照写在这里。"


def stub(directory: pathlib.Path, name: str, body: str) -> pathlib.Path:
  directory.mkdir(parents=True, exist_ok=True)
  path = directory / name
  path.write_text(f"#!/bin/sh\n{body}\n", encoding="utf-8")
  path.chmod(0o755)
  return path


def calls(tmp_path: pathlib.Path) -> int:
  """Return how many times any stub engine has run."""
  log = tmp_path / "calls"
  if not log.is_file():
    return 0
  return len(log.read_text(encoding="utf-8").splitlines())


def gateway(tmp_path: pathlib.Path, body: str) -> None:
  """Install a `custom` engine, which is what an unsampled turn runs."""
  stub(
      tmp_path / "bin",
      "mygateway",
      f"echo run >> {tmp_path / 'calls'}\n{body}",
  )
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "custom"\ncommand = ["mygateway"]\n', encoding="utf-8"
  )


def answering(tmp_path: pathlib.Path, answer: str) -> None:
  """Install a `custom` engine that returns exactly `answer`."""
  path = tmp_path / "answer.txt"
  path.write_text(answer, encoding="utf-8")
  gateway(tmp_path, f"cat > /dev/null\ncat {path}")


def runs(tmp_path: pathlib.Path, session: str = SESSION) -> list[pathlib.Path]:
  """Return the single-run records this session has written."""
  return sorted((state(tmp_path, session) / ab.RUN_DIRECTORY).glob("*.json"))


def leaked(text: str, names: set[str]) -> set[str]:
  """Return which of `names` appear in `text`."""
  return {name for name in names if name in text}


def diagnostics(
    tmp_path: pathlib.Path, session: str = SESSION
) -> list[dict[str, object]]:
  """Return the fail-open lines this session has written."""
  path = state(tmp_path, session) / hook.DIAGNOSTICS_FILENAME
  if not path.is_file():
    return []
  return [
      json.loads(line)
      for line in path.read_text(encoding="utf-8").splitlines()
      if line
  ]


def painted(
    batches: list[tuple[str, bool]],
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    message: str,
) -> str:
  """Run one message's batches and return what the screen ends up with.

  The host paints a batch's own delta unless the hook answers, and the
  answer replaces that batch and nothing before it — so the aggregate,
  not any one answer, is what a reader sees.
  """
  screen = ""
  for index, (delta, final) in enumerate(batches):
    answer = run_hook(
        display(delta, index=index, final=final, message=message, cwd=tmp_path),
        tmp_path,
        monkeypatch,
        capsys,
    )
    screen += delta if answer is None else str(answer["displayContent"])
  return screen


def candidates(tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch) -> None:
  """Install the two preset engines an A/B trial draws from here."""
  monkeypatch.setattr(ab, "CANDIDATES", PAIR)
  count = f"echo run >> {tmp_path / 'calls'}"
  stub(tmp_path / "bin", "claude", f"{count}\ncat > /dev/null\necho {FIRST}")
  stub(tmp_path / "bin", "grok", f"{count}\necho {SECOND}")


def run_hook(
    payload: dict[str, object],
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
    text: str | None = None,
    **extra: str,
) -> dict[str, object] | None:
  """Run one hook event and return its `hookSpecificOutput`, if any."""
  environment = {
      "PATH": f"{tmp_path / 'bin'}:/usr/bin:/bin",
      "HOME": str(tmp_path / "home"),
      "XDG_CACHE_HOME": str(tmp_path / "cache"),
      # Where scratch goes is where the state goes: the hook has no
      # setting of its own for it, on purpose.
      "TMPDIR": str(tmp_path),
      # Off unless a test asks for it: sampling is what these tests
      # control, never chance.
      hook.RATE_VARIABLE: "0",
      **extra,
  }
  monkeypatch.delenv(hook.DISABLE_VARIABLE, raising=False)
  for name, value in environment.items():
    monkeypatch.setenv(name, value)
  monkeypatch.chdir(tmp_path)
  monkeypatch.setattr(
      sys, "stdin", io.StringIO(json.dumps(payload) if text is None else text)
  )
  assert hook.main([]) == hook.OK
  out = capsys.readouterr().out
  if not out:
    return None
  answer = json.loads(out)
  return answer["hookSpecificOutput"]


def display(
    delta: str,
    *,
    index: int = 0,
    final: bool = True,
    session: str = SESSION,
    message: str = MESSAGE,
    cwd: pathlib.Path | None = None,
) -> dict[str, object]:
  return {
      "session_id": session,
      "transcript_path": "/dev/null",
      "cwd": str(cwd) if cwd is not None else "/nonexistent",
      "hook_event_name": hook.MESSAGE_DISPLAY,
      "turn_id": "turn",
      "message_id": message,
      "index": index,
      "final": final,
      "delta": delta,
  }


def stop(session: str = SESSION) -> dict[str, object]:
  return {
      "session_id": session,
      "transcript_path": "/dev/null",
      "cwd": "/nonexistent",
      "hook_event_name": hook.STOP,
      "stop_hook_active": False,
  }


def state(tmp_path: pathlib.Path, session: str = SESSION) -> pathlib.Path:
  return tmp_path / hook.STATE_DIRECTORY / session


def ledger(
    tmp_path: pathlib.Path, session: str = SESSION
) -> list[pathlib.Path]:
  return sorted((state(tmp_path, session) / ab.LEDGER_DIRECTORY).glob("*.json"))


def test_a_middle_batch_is_passed_through_and_calls_no_model(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  answer = run_hook(
      display(LONG, index=0, final=False, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
  )
  # No output at all is how the host shows the original delta.
  assert answer is None
  assert calls(tmp_path) == 0


def test_the_final_batch_polishes_the_whole_message_exactly_once(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  seen = tmp_path / "seen"
  gateway(tmp_path, f"cat > {seen}\necho {POLISHED}")
  batches = ["第一批。\n", "第二批。\n", LONG]
  shown_per_batch = [
      run_hook(
          display(
              delta,
              index=index,
              final=index == len(batches) - 1,
              cwd=tmp_path,
          ),
          tmp_path,
          monkeypatch,
          capsys,
      )
      for index, delta in enumerate(batches)
  ]
  # Every batch but the last passed through untouched.
  assert shown_per_batch[:-1] == [None] * (len(batches) - 1)
  answer = shown_per_batch[-1]
  assert calls(tmp_path) == 1
  # One call, over the batches joined in the order they arrived.
  assert seen.read_text(encoding="utf-8") == "".join(batches)
  assert answer is not None
  shown = answer["displayContent"]
  assert isinstance(shown, str)
  # The last batch keeps its own text; the rewrite follows it, because
  # the batches before it are already on screen.
  assert shown.startswith(batches[-1])
  assert POLISHED in shown
  # The batches are scratch and do not outlive the message.
  assert not (state(tmp_path) / hook.PARTS_DIRECTORY / MESSAGE).exists()


def test_a_short_message_is_left_alone_and_a_long_one_is_not(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert (
      run_hook(display(SHORT, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  assert calls(tmp_path) == 0

  # Same guard, the other way round: the threshold has to let something
  # through, or it is indistinguishable from the hook being off.
  answer = run_hook(
      display(LONG, message="second", cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
  )
  assert answer is not None
  assert calls(tmp_path) == 1


def test_fenced_code_does_not_count_towards_the_threshold(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  code = "```\n" + "x = 1  # a long line of code\n" * 40 + "```\n"
  assert (
      run_hook(
          display(f"{SHORT}\n{code}", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert calls(tmp_path) == 0

  # The same fence, with enough prose around it, is polished.
  answer = run_hook(
      display(f"{LONG}\n{code}", message="second", cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
  )
  assert answer is not None
  assert calls(tmp_path) == 1


def test_an_engine_that_fails_leaves_the_original_on_screen(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, "cat > /dev/null\nexit 1")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  assert calls(tmp_path) == 1

  # The engine answers now, so the same message is rewritten: the
  # fail-open path is not simply "this hook never shows anything".
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  answer = run_hook(
      display(LONG, message="second", cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
  )
  assert answer is not None


def test_an_empty_answer_leaves_the_original_on_screen(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, "cat > /dev/null\nexit 0")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )


def test_a_payload_that_makes_no_sense_is_ignored(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert run_hook({}, tmp_path, monkeypatch, capsys, text="{not json") is None
  assert run_hook({}, tmp_path, monkeypatch, capsys, text="[1, 2]") is None
  assert (
      run_hook({"hook_event_name": "PreToolUse"}, tmp_path, monkeypatch, capsys)
      is None
  )
  broken = display(LONG, cwd=tmp_path) | {"index": "one"}
  assert run_hook(broken, tmp_path, monkeypatch, capsys) is None
  assert calls(tmp_path) == 0


def test_the_disable_variable_turns_the_hook_off(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  payload = display(LONG, cwd=tmp_path)
  assert (
      run_hook(
          payload, tmp_path, monkeypatch, capsys, **{hook.DISABLE_VARIABLE: "1"}
      )
      is None
  )
  assert calls(tmp_path) == 0

  # Unset, the same message is polished: the switch switches something.
  assert run_hook(payload, tmp_path, monkeypatch, capsys) is not None
  assert calls(tmp_path) == 1


def test_an_unsampled_turn_runs_one_engine_and_writes_no_ledger(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  candidates(tmp_path, monkeypatch)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert calls(tmp_path) == 1
  assert ledger(tmp_path) == []
  assert not (state(tmp_path) / ab.PENDING_FILENAME).exists()


def test_a_sampled_turn_runs_two_engines_and_shows_them_blind(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  candidates(tmp_path, monkeypatch)
  answer = run_hook(
      display(LONG, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  assert answer is not None
  shown = answer["displayContent"]
  assert isinstance(shown, str)
  assert calls(tmp_path) == 2
  # Two candidates, labelled and nothing more: ADR-0008 五 wants a blind
  # comparison, so no model name reaches the screen.
  assert "── A ──" in shown
  assert "── B ──" in shown
  assert FIRST in shown
  assert SECOND in shown
  for candidate in PAIR:
    assert candidate.model not in shown
    assert candidate.engine not in shown
  entries = ledger(tmp_path)
  assert len(entries) == 1
  assert entries[0].stem in shown


def test_every_sampled_turn_of_a_session_gets_its_own_code_name(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  candidates(tmp_path, monkeypatch)
  rounds = 8
  for index in range(rounds):
    assert (
        run_hook(
            display(LONG, message=f"message-{index}", cwd=tmp_path),
            tmp_path,
            monkeypatch,
            capsys,
            **{hook.RATE_VARIABLE: "1"},
        )
        is not None
    )
  entries = ledger(tmp_path)
  assert len(entries) == rounds
  assert len({path.stem for path in entries}) == rounds


def test_the_ledger_keeps_the_whole_trial_inside_the_session_directory(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  candidates(tmp_path, monkeypatch)
  _ = run_hook(
      display(LONG, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  entries = ledger(tmp_path)
  assert len(entries) == 1
  entry = json.loads(entries[0].read_text(encoding="utf-8"))
  assert entry["code"] == entries[0].stem
  assert entry["original"] == LONG
  assert entry["at"].endswith("+00:00")
  assert [c["label"] for c in entry["candidates"]] == list(ab.LABELS)
  assert {(c["engine"], c["model"]) for c in entry["candidates"]} == set(PAIR)
  assert {c["text"].strip() for c in entry["candidates"]} == {FIRST, SECOND}
  # Session state, not repository state, and nobody else's to read.
  assert entries[0].is_relative_to(tmp_path / hook.STATE_DIRECTORY)
  assert entries[0].stat().st_mode & 0o777 == ab.FILE_MODE


def test_a_trial_that_loses_a_candidate_leaves_the_original_on_screen(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  candidates(tmp_path, monkeypatch)
  stub(tmp_path / "bin", "grok", f"echo run >> {tmp_path / 'calls'}\nexit 1")
  assert (
      run_hook(
          display(LONG, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
          **{hook.RATE_VARIABLE: "1"},
      )
      is None
  )
  assert calls(tmp_path) == 2
  assert ledger(tmp_path) == []


def test_stop_hands_over_the_code_and_never_which_model_is_which(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  candidates(tmp_path, monkeypatch)
  shown = run_hook(
      display(LONG, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  assert shown is not None
  answer = run_hook(stop(), tmp_path, monkeypatch, capsys)
  assert answer is not None
  context = answer["additionalContext"]
  assert isinstance(context, str)
  code = ledger(tmp_path)[0].stem
  assert code in context

  # This text is rendered on the user's screen, so a model name in it is
  # the answer key printed beside the blind comparison. The mapping
  # lives in the ledger, which the reader is not looking at.
  #
  # The names come out of the ledger rather than out of `PAIR`, because
  # what has to stay off the screen is whatever actually ran; and the
  # check below is asserted to be capable of failing before it is
  # trusted, because a leak test that cannot detect a leak passes for
  # the wrong reason.
  entry = json.loads(ledger(tmp_path)[0].read_text(encoding="utf-8"))
  names = {
      value
      for candidate in entry["candidates"]
      for value in (candidate["engine"], candidate["model"])
  }
  assert len(names) == 2 * len(PAIR)
  assert leaked(context, names) == set()
  # The predicate is not vacuous: it does catch a name when one is there.
  assert leaked(f"{context} {sorted(names)[0]}", names)

  # Nothing else may ride along either — an unknown third name is a leak
  # the ledger cannot describe, so the whole string is pinned.
  assert context == (
      f"limae A/B：本轮有一次 A/B 对照，编号「{code}」，两栏是盲评。"
      f"型号对应在 {ledger(tmp_path)[0]}；"
      "用户按编号给出偏好之前不要说出哪一栏是哪个模型。"
  )

  # The rewrites themselves stay off the model's context too
  # (ADR-0009 五).
  assert FIRST not in context
  assert SECOND not in context

  # One trial is announced once. Always answering would re-trigger the
  # Stop hook, because additionalContext continues the conversation.
  assert run_hook(stop(), tmp_path, monkeypatch, capsys) is None


def test_the_ledger_still_holds_the_mapping_the_context_withholds(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Withholding the models from the screen only works if they are still
  # written down somewhere: the point is a blind reader, not a lost
  # measurement.
  candidates(tmp_path, monkeypatch)
  assert (
      run_hook(
          display(LONG, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
          **{hook.RATE_VARIABLE: "1"},
      )
      is not None
  )
  entries = ledger(tmp_path)
  assert len(entries) == 1
  written = json.loads(entries[0].read_text(encoding="utf-8"))["candidates"]
  assert len(written) == len(PAIR)
  assert {(c["engine"], c["model"]) for c in written} == set(PAIR)

  # And the context names the file that holds it.
  answer = run_hook(stop(), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert str(entries[0]) in str(answer["additionalContext"])


def test_stop_says_nothing_when_the_turn_had_no_trial(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  assert run_hook(stop(), tmp_path, monkeypatch, capsys) is None


def test_a_session_id_cannot_name_a_directory_outside_the_state_root(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert (
      run_hook(
          display(LONG, index=0, final=False, session="../../escape"),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  root = tmp_path / hook.STATE_DIRECTORY
  assert [path.name for path in root.iterdir()] == ["______escape"]
  assert not (tmp_path.parent / "escape").exists()

  # A well-formed id is left as it is, so the folding is not just
  # renaming everything.
  assert (
      run_hook(
          display(LONG, index=0, final=False),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert (root / SESSION).is_dir()


def test_old_sessions_are_pruned_and_the_current_one_is_not(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  old = tmp_path / hook.STATE_DIRECTORY / "older-session"
  old.mkdir(parents=True)
  stale = time.time() - hook.RETENTION - 60
  os.utime(old, (stale, stale))
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  assert not old.exists()
  assert state(tmp_path).is_dir()


def test_the_candidate_pool_is_the_seven_of_adr_0008():
  assert len(ab.CANDIDATES) == 7
  assert len(set(ab.CANDIDATES)) == 7
  assert {c.engine for c in ab.CANDIDATES} == set(engines.PRESETS)
  assert {c.model for c in ab.CANDIDATES} == {
      "gpt-5.6-luna",
      "gpt-5.6-terra",
      "gpt-5.4",
      "grok-4.5",
      "grok-4.6",
      "haiku",
      "sonnet",
  }


def test_a_trial_only_draws_from_the_engines_that_are_installed(
    tmp_path: pathlib.Path,
):
  env = {"PATH": f"{tmp_path / 'bin'}:/usr/bin:/bin", "HOME": str(tmp_path)}
  assert ab.pool(env) == []
  # Nothing installed is not a comparison, whatever the sampling says.
  assert ab.draw(tmp_path, env, 1.0) is None

  stub(tmp_path / "bin", "grok", "exit 0")
  installed = ab.pool(env)
  assert len(installed) == 2
  assert {c.engine for c in installed} == {"grok"}
  trial = ab.draw(tmp_path, env, 1.0)
  assert trial is not None
  assert {trial.a, trial.b} == set(installed)


def test_the_code_names_are_distinct_two_character_chinese_nouns():
  assert len(ab.CODE_NAMES) == len(set(ab.CODE_NAMES))
  assert len(ab.CODE_NAMES) == 48
  for name in ab.CODE_NAMES:
    assert len(name) == 2
    assert all("一" <= character <= "鿿" for character in name)


def test_a_session_that_runs_out_of_code_names_stops_sampling(
    tmp_path: pathlib.Path,
):
  env = {"PATH": f"{tmp_path / 'bin'}:/usr/bin:/bin", "HOME": str(tmp_path)}
  stub(tmp_path / "bin", "grok", "exit 0")
  used = tmp_path / ab.LEDGER_DIRECTORY
  used.mkdir()
  assert ab.draw(tmp_path, env, 1.0) is not None
  for name in ab.CODE_NAMES:
    (used / f"{name}.json").write_text("{}", encoding="utf-8")
  assert ab.draw(tmp_path, env, 1.0) is None


def test_the_entry_point_dispatches_the_hook_subcommand(
    monkeypatch: pytest.MonkeyPatch,
):
  seen: list[list[str]] = []

  def record(argv: list[str]) -> int:
    seen.append(list(argv))
    return 0

  monkeypatch.setattr(sys, "argv", ["limae", "hook"])
  monkeypatch.setattr(hook, "main", record)
  assert zh_format.main() == 0
  assert seen == [[]]


def test_running_the_subcommand_by_hand_says_what_it_wants(
    capsys: pytest.CaptureFixture[str],
):
  assert hook.main(["MessageDisplay"]) == hook.BAD_USAGE
  assert "JSON on stdin" in capsys.readouterr().err


def test_a_late_batch_is_waited_for_and_a_missing_one_gives_up(
    tmp_path: pathlib.Path,
):
  # The host starts one process per batch without waiting for the last
  # one, so the final batch can reach the assembly before a sibling has
  # written itself down.
  parts = tmp_path / "parts"
  parts.mkdir()
  hook._keep(parts, 0, "第一批。")
  hook._keep(parts, 2, "第三批。")

  def land() -> None:
    time.sleep(0.1)
    hook._keep(parts, 1, "第二批。")

  late = threading.Thread(target=land)
  late.start()
  try:
    assembled = hook._assemble(
        parts, 3, time.monotonic() + hook.SIBLING_WAIT, tmp_path, MESSAGE
    )
  finally:
    late.join()
  assert assembled == "第一批。第二批。第三批。"

  # A batch that never lands ends the turn instead of polishing a
  # message with a hole in it, and it does not wait forever to do so.
  started = time.monotonic()
  assert hook._assemble(parts, 4, started + 0.2, tmp_path, MESSAGE) is None
  assert time.monotonic() - started < hook.SIBLING_WAIT


def test_a_message_missing_a_batch_shows_the_original_and_calls_nothing(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Batch 1 never arrives; batch 2 is the final one. Polishing what did
  # arrive would put a rewrite of a truncated message under the user's
  # name, which is worse than not polishing at all: it reads fine.
  monkeypatch.setattr(hook, "SIBLING_WAIT", 0.1)
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert (
      run_hook(
          display(LONG, index=0, final=False, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert (
      run_hook(
          display(LONG, index=2, final=True, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert calls(tmp_path) == 0

  # The same two batches with the one in between: polished, once.
  whole = [
      run_hook(
          display(
              LONG, index=index, final=index == 2, message="whole", cwd=tmp_path
          ),
          tmp_path,
          monkeypatch,
          capsys,
      )
      for index in range(3)
  ]
  assert whole[:-1] == [None, None]
  assert whole[-1] is not None
  assert calls(tmp_path) == 1


def test_the_state_root_is_the_one_place_and_is_not_configurable(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  # One place, derived from scratch and nothing else. No variable of
  # this module's own moves it: a hook's environment is set by whatever
  # configured the session, so a name is not a boundary.
  assert (tmp_path / hook.STATE_DIRECTORY / SESSION).is_dir()
  elsewhere = tmp_path / "elsewhere"
  for variable in ("LIMAE_HOOK_STATE", "LIMAE_HOOK_STATE_FOR_TESTS"):
    monkeypatch.setenv(variable, str(elsewhere))
  assert (
      run_hook(
          display(LONG, message="second", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is not None
  )
  assert not elsewhere.exists()


def test_a_scratch_directory_inside_a_checkout_is_refused(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # A reply that lands in a working tree is one `git add` away from a
  # public repository (ADR-0009 五, 八).
  gateway(tmp_path, f"cat > /dev/null\necho {POLISHED}")
  checkout = tmp_path / "repo"
  (checkout / ".git").mkdir(parents=True)
  assert (
      run_hook(
          display(LONG, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
          TMPDIR=str(checkout),
      )
      is None
  )
  assert calls(tmp_path) == 0
  assert [path.name for path in checkout.iterdir()] == [".git"]

  # The same message, with scratch somewhere that is nobody's checkout:
  # polished as usual.
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )


def test_nothing_holding_a_reply_is_created_readable_by_others(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # With the umask wide open, the mode can only come from the call that
  # created the file — a chmod afterwards would still leave a window in
  # which the reply was everyone's to read.
  previous = os.umask(0)
  try:
    parts = tmp_path / "parts"
    parts.mkdir()
    hook._keep(parts, 0, "一批。")
    mode = (parts / f"000000{hook.PART_SUFFIX}").stat().st_mode
    assert mode & 0o777 == hook.FILE_MODE

    candidates(tmp_path, monkeypatch)
    _ = run_hook(
        display(LONG, cwd=tmp_path),
        tmp_path,
        monkeypatch,
        capsys,
        **{hook.RATE_VARIABLE: "1"},
    )
    entries = ledger(tmp_path)
    assert len(entries) == 1
    assert entries[0].stat().st_mode & 0o777 == ab.FILE_MODE
    assert state(tmp_path).stat().st_mode & 0o777 == hook.DIRECTORY_MODE
  finally:
    _ = os.umask(previous)


def test_the_rewrite_goes_through_this_repository_s_own_fixes(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The model settles the words, the rules settle the typography
  # (ADR-0005 section 四). A rewrite that drops the spaces around a dash
  # is an error-level `zh-typography` violation, and shipping it to the
  # screen is the one thing this repository exists to stop.
  answering(tmp_path, SLOPPY)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  shown = str(answer["displayContent"])
  assert TIDIED in shown
  assert SLOPPY not in shown
  assert not zh_format.check_text(shown.split("── 润色 ──\n")[1])


def test_a_rewrite_that_needs_no_fixing_reaches_the_screen_unchanged(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The other half of the pair: the fixes are not free to reword a
  # rewrite that was already right.
  answering(tmp_path, TIDIED)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert str(answer["displayContent"]).endswith(
      f"── 润色 ──\n原 {LONG}\n改 {TIDIED}\n"
  )


def test_a_rewrite_identical_to_the_input_says_so_instead_of_repeating_it(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Reprinting the message the reader just read says nothing. Measured
  # over this session's own records the models leave a fifth of the
  # turns byte-identical, so this is the common case, not a corner.
  answering(tmp_path, LONG)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert str(answer["displayContent"]).endswith(
      f"── 润色 ── {hook.UNCHANGED}\n"
  )


def test_only_the_changed_lines_reach_the_screen(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The point of the block is what moved. A line the rewrite left alone
  # is already on the screen above it.
  kept = "第一行原样不动，它不该再出现一次。"
  before = f"{kept}\n{LONG}"
  after = f"{kept}\n{TIDIED}"
  answering(tmp_path, after)
  answer = run_hook(
      display(before, cwd=tmp_path), tmp_path, monkeypatch, capsys
  )
  assert answer is not None
  shown = str(answer["displayContent"])
  block = shown.split("── 润色 ──")[1]
  assert kept not in block
  assert f"原 {LONG}" in block
  assert f"改 {TIDIED}" in block


def test_the_gap_above_the_block_is_one_blank_line_for_every_ending(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The gap used to be built by adding to whatever trailing newlines the
  # delta happened to have, which only holds still while there is
  # exactly one of them.
  answering(tmp_path, TIDIED)
  endings = ("", "\n", "\n\n", "\n\n\n")
  gaps: list[str] = []
  for number, ending in enumerate(endings):
    answer = run_hook(
        display(LONG + ending, message=f"m{number}", cwd=tmp_path),
        tmp_path,
        monkeypatch,
        capsys,
    )
    assert answer is not None
    shown = str(answer["displayContent"])
    head, _, _ = shown.partition("── 润色 ──")
    gaps.append(head[len(head.rstrip("\n")) :])
  assert len(gaps) == len(endings)
  assert set(gaps) == {"\n\n"}


def test_an_engine_that_fails_leaves_a_line_saying_how(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Fail-open is not fail-silent: the user sees their own text, and
  # whoever is debugging sees which step failed and which way.
  gateway(tmp_path, "cat > /dev/null\nexit 1")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.SINGLE
  assert lines[0]["kind"] == engines.NONZERO_EXIT
  assert lines[0]["message_id"] == MESSAGE

  # An engine that answers with nothing is a different failure, and says
  # so rather than being folded into the one above.
  gateway(tmp_path, "cat > /dev/null\nexit 0")
  assert (
      run_hook(
          display(LONG, message="empty", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 2
  assert lines[1]["kind"] == engines.EMPTY_ANSWER


def test_a_message_that_polishes_cleanly_writes_no_diagnostics(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The file is for failures. A turn that worked leaves it empty, so a
  # line in it always means something.
  answering(tmp_path, TIDIED)
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  assert diagnostics(tmp_path) == []

  # A message too short to polish is not a failure either.
  assert (
      run_hook(
          display(SHORT, message="short", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert diagnostics(tmp_path) == []


def test_a_repair_is_written_down_because_it_is_a_selection_signal(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Which models need the rules to clean up after them is evidence for
  # ADR-0008 section 五, not only a display fix.
  answering(tmp_path, SLOPPY)
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.FIX
  assert lines[0]["kind"] == hook.REPAIRED


def test_a_missing_batch_says_so_where_the_other_failures_do(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  monkeypatch.setattr(hook, "SIBLING_WAIT", 0.1)
  answering(tmp_path, TIDIED)
  assert (
      run_hook(
          display(LONG, index=2, final=True, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.ASSEMBLE
  assert lines[0]["kind"] == hook.INCOMPLETE


def test_the_ledger_keeps_both_what_the_model_wrote_and_what_was_shown(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Folding the fixes into the recorded text would erase the difference
  # between a model that needs them and one that does not, which is the
  # very thing the ledger is being collected to decide.
  monkeypatch.setattr(ab, "CANDIDATES", PAIR)
  for name in ("claude", "grok"):
    stub(tmp_path / "bin", name, f"cat > /dev/null\ncat {tmp_path / name}.txt")
  (tmp_path / "claude.txt").write_text(SLOPPY, encoding="utf-8")
  (tmp_path / "grok.txt").write_text(TIDIED, encoding="utf-8")
  answer = run_hook(
      display(LONG, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  assert answer is not None
  entries = ledger(tmp_path)
  assert len(entries) == 1
  written = json.loads(entries[0].read_text(encoding="utf-8"))["candidates"]
  assert len(written) == 2
  # Which candidate is shown as A is drawn, so the check is by engine.
  by_engine = {str(c["engine"]): c for c in written}
  assert set(by_engine) == {"claude", "grok"}
  assert by_engine["claude"]["text"] == SLOPPY
  assert by_engine["grok"]["text"] == TIDIED
  assert [c["displayed"] for c in written] == [TIDIED, TIDIED]
  # What the screen got is the fixed pair, not the written one.
  assert SLOPPY not in str(answer["displayContent"])


def test_a_trial_that_loses_a_candidate_says_which_way_it_lost(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  monkeypatch.setattr(ab, "CANDIDATES", PAIR)
  stub(tmp_path / "bin", "claude", f"cat > /dev/null\necho {FIRST}")
  stub(tmp_path / "bin", "grok", "cat > /dev/null\nexit 1")
  assert (
      run_hook(
          display(LONG, cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
          **{hook.RATE_VARIABLE: "1"},
      )
      is None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.AB
  assert lines[0]["kind"] == engines.NONZERO_EXIT


def test_no_diagnostics_line_carries_the_prose_or_the_engine_s_output(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The same boundary the ledger keeps (ADR-0009 section 八): this file
  # outlives the run, so it holds ids and classifications and nothing an
  # engine was free to print.
  printed = "MARKER-ACME-1000"
  gateway(tmp_path, f"cat > /dev/null\necho {printed} >&2\nexit 1")
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  path = state(tmp_path) / hook.DIAGNOSTICS_FILENAME
  raw = path.read_text(encoding="utf-8")
  assert printed not in raw
  assert LONG[:12] not in raw
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert set(lines[0]) == {"at", "message_id", "step", "kind"}
  # And it is nobody else's to read.
  assert path.stat().st_mode & 0o777 == hook.FILE_MODE


def test_the_batches_of_an_abandoned_message_are_swept_up(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # A message the host is killed in the middle of never gets its final
  # batch, so nothing assembles it and nothing deletes it. Its session
  # is the live one, so session retention does not reach it either.
  answering(tmp_path, TIDIED)
  parts = state(tmp_path) / hook.PARTS_DIRECTORY
  abandoned = parts / "abandoned"
  fresh = parts / "still-streaming"
  for directory in (abandoned, fresh):
    directory.mkdir(parents=True)
    (directory / f"000000{hook.PART_SUFFIX}").write_text("甲", encoding="utf-8")
  stale = time.time() - hook.ORPHAN_RETENTION - 60
  os.utime(abandoned, (stale, stale))

  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  assert not abandoned.exists()
  # A message still arriving keeps its batches, and so does the session.
  assert fresh.is_dir()
  assert state(tmp_path).is_dir()


def test_the_gap_is_one_blank_line_across_a_whole_batch_sequence(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The gap belongs to the screen, not to the last batch: what an
  # earlier batch painted cannot be taken back by `displayContent`, only
  # counted. The case that makes this visible is a message ending on a
  # newline, whose final batch is empty because there is nothing left.
  answering(tmp_path, TIDIED)
  sequences = {
      "empty-final": [(LONG + "\n\n", False), ("", True)],
      "final-on-newline": [(LONG + "\n", False), ("尾批。\n", True)],
      "final-mid-line": [(LONG + "\n\n", False), ("尾批。", True)],
      "single-batch": [(LONG, True)],
  }
  gaps: dict[str, int] = {}
  for name, batches in sequences.items():
    screen = painted(batches, tmp_path, monkeypatch, capsys, name)
    head, marker, _ = screen.partition("── 润色 ──")
    assert marker, name
    gaps[name] = len(head) - len(head.rstrip("\n"))
  assert len(gaps) == len(sequences)
  assert set(gaps.values()) == {hook.BLOCK_GAP}, gaps


def test_a_fix_that_will_not_run_shows_the_rewrite_and_says_so(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Fail open at the finest grain: a rewrite with a typography slip in
  # it still beats no rewrite. Both paths have to leave the line, or the
  # file stops being the place you can trust to answer "what happened".
  def unfixable(text: str, cwd: pathlib.Path) -> tuple[str, str]:
    del cwd
    return text, hook.CRASHED

  monkeypatch.setattr(hook, "_tidy", unfixable)
  answering(tmp_path, SLOPPY)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert SLOPPY in str(answer["displayContent"])
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.FIX
  assert lines[0]["kind"] == hook.CRASHED
  # ADR-0012 section 决定: nothing is recorded when the fixes did not
  # run. Such a record would carry `displayed` equal to `text` for a
  # reason that has nothing to do with the model, and the whole point of
  # keeping the two apart is to measure what the model did.
  assert runs(tmp_path) == []

  # And the same on the A/B path, which used to drop the failure on the
  # floor: the candidates went up unfixed and nothing said why.
  monkeypatch.setattr(ab, "CANDIDATES", PAIR)
  for name in ("claude", "grok"):
    stub(tmp_path / "bin", name, f"cat > /dev/null\ncat {tmp_path / name}.txt")
    (tmp_path / f"{name}.txt").write_text(SLOPPY, encoding="utf-8")
  answer = run_hook(
      display(LONG, message="trial", cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  assert answer is not None
  assert SLOPPY in str(answer["displayContent"])
  lines = diagnostics(tmp_path)
  assert len(lines) == 2
  assert lines[1]["step"] == hook.FIX
  assert lines[1]["kind"] == hook.CRASHED


def test_a_broken_polish_table_says_so_instead_of_reading_as_a_crash(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # A typo in the user's own configuration is theirs to fix, so it is
  # worth telling apart from this file falling over.
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "nosuchengine"\n', encoding="utf-8"
  )
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.SINGLE
  assert lines[0]["kind"] == hook.MISCONFIGURED


def test_a_ledger_that_will_not_write_still_shows_the_comparison(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Two model calls have already been paid for and the user has already
  # waited for them; losing the evidence is not a reason to lose those.
  monkeypatch.setattr(ab, "CANDIDATES", PAIR)
  for name, text in (("claude", FIRST), ("grok", SECOND)):
    stub(tmp_path / "bin", name, f"cat > /dev/null\necho {text}")

  def refuse(*_args: object, **_kwargs: object) -> None:
    raise OSError("no room")

  monkeypatch.setattr(ab, "record", refuse)
  answer = run_hook(
      display(LONG, cwd=tmp_path),
      tmp_path,
      monkeypatch,
      capsys,
      **{hook.RATE_VARIABLE: "1"},
  )
  assert answer is not None
  shown = str(answer["displayContent"])
  assert FIRST in shown
  assert SECOND in shown
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  # Writing the record down is its own step: a ledger that will not
  # write is not an engine that would not answer.
  assert lines[0]["step"] == hook.RECORD
  assert lines[0]["kind"] == hook.CRASHED


def test_the_handbook_lists_every_kind_the_hook_can_write():
  # The diagnostics file is only useful if a reader can look a `kind` up,
  # so a category that exists in the code and not in the table is a
  # category nobody can act on. Kept as a test rather than a habit: the
  # table has already drifted once.
  kinds: set[str] = set(engines.REASONS)
  kinds.discard(engines.OK)
  kinds |= {hook.INCOMPLETE, hook.REPAIRED, hook.MISCONFIGURED, hook.CRASHED}
  listed = set(
      re.findall(
          r"^\| `([a-z-]+)` \|", HANDBOOK.read_text(encoding="utf-8"), re.M
      )
  )
  assert kinds - listed == set()
  assert len(kinds) == 13


def test_an_ordinary_turn_writes_down_what_polish_did(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Until this existed, a single polish left nothing on disk: what went
  # in and what came out could only be recovered from a screenshot,
  # which makes the spec unmeasurable — the same input has come back
  # anywhere from untouched to stripped of every emphasis marker, so one
  # observation says nothing.
  answering(tmp_path, SLOPPY)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None

  written = runs(tmp_path)
  assert len(written) == 1
  assert written[0].stem == MESSAGE
  record = json.loads(written[0].read_text(encoding="utf-8"))

  # Three versions, kept apart, because they answer different questions:
  # what the assistant wrote, what the model made of it, what the reader
  # saw once the rules had cleaned up after the model.
  assert record["original"] == LONG
  assert record["text"] == SLOPPY
  assert record["displayed"] == TIDIED
  assert record["displayed"] in str(answer["displayContent"])
  # A record that does not say what ran is not evidence of anything.
  assert record["engine"] == "custom"
  assert record["message_id"] == MESSAGE
  assert record["at"]


def test_the_run_record_is_this_user_s_alone_and_goes_nowhere_else(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # It holds the assistant's own reply, so it lives under the same
  # boundary as the A/B ledger: the session's directory, this user's
  # permissions, and out of every repository (ADR-0009 五、八).
  answering(tmp_path, TIDIED)
  assert (
      run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is not None
  )
  written = runs(tmp_path)
  assert len(written) == 1
  assert written[0].stat().st_mode & 0o777 == hook.FILE_MODE
  assert written[0].parent.stat().st_mode & 0o777 == hook.DIRECTORY_MODE
  # Same session directory the ledger uses, so the same prune reaches it.
  assert written[0].parent.parent == state(tmp_path)
  assert not list(tmp_path.glob("**/.git"))


def test_a_run_record_that_will_not_write_still_shows_the_rewrite(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Same trade the A/B ledger makes: losing the evidence is bad,
  # throwing away a rewrite the user waited for is worse.
  answering(tmp_path, TIDIED)

  def refuse(*_args: object, **_kwargs: object) -> None:
    raise OSError("no room")

  monkeypatch.setattr(ab, "record_run", refuse)
  answer = run_hook(display(LONG, cwd=tmp_path), tmp_path, monkeypatch, capsys)
  assert answer is not None
  assert TIDIED in str(answer["displayContent"])
  lines = diagnostics(tmp_path)
  assert len(lines) == 1
  assert lines[0]["step"] == hook.RECORD
  assert lines[0]["kind"] == hook.CRASHED


def test_a_turn_that_is_not_polished_writes_no_run_record(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # The record is of what polish did. A message too short to polish and
  # an engine that would not answer both did nothing, so both write
  # nothing — a directory of records has to mean what it says.
  answering(tmp_path, TIDIED)
  assert (
      run_hook(display(SHORT, cwd=tmp_path), tmp_path, monkeypatch, capsys)
      is None
  )
  assert runs(tmp_path) == []

  gateway(tmp_path, "cat > /dev/null\nexit 1")
  assert (
      run_hook(
          display(LONG, message="failed", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is None
  )
  assert runs(tmp_path) == []


def test_each_message_gets_its_own_record_and_a_retry_replaces_it(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Sampling the spec means many runs side by side, so the file is named
  # by the message rather than appended to one log.
  answering(tmp_path, TIDIED)
  for number in range(3):
    assert (
        run_hook(
            display(LONG, message=f"m{number}", cwd=tmp_path),
            tmp_path,
            monkeypatch,
            capsys,
        )
        is not None
    )
  written = runs(tmp_path)
  assert len(written) == 3
  assert {path.stem for path in written} == {"m0", "m1", "m2"}

  # The same message polished again overwrites its own record rather
  # than failing on the exclusive create the batches use.
  answering(tmp_path, SLOPPY)
  assert (
      run_hook(
          display(LONG, message="m0", cwd=tmp_path),
          tmp_path,
          monkeypatch,
          capsys,
      )
      is not None
  )
  assert len(runs(tmp_path)) == 3
  again = json.loads(
      (state(tmp_path) / ab.RUN_DIRECTORY / "m0.json").read_text(
          encoding="utf-8"
      )
  )
  assert again["text"] == SLOPPY
