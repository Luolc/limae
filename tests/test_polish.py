import io
import json
import pathlib
import sys

import pytest

from limae import config, engines, polish, zh_format

# Every engine here is a shell stub in a temporary directory: the tests
# never reach the network and never call a real model (AGENTS.md quality
# bar). What is asserted is the template expansion, the `auto` ordering,
# the diagnosis branches and the config parsing.
SPEC = "Polish spec: keep the code fences.\n"
TEXT = "ACME 的报告写得不好。\n"
# A synthetic stand-in for a credential value, so a test can assert it
# never appears in a diagnostic. Not a key of any kind.
SYNTHETIC_VALUE = "synthetic-placeholder-value"
SPEC_DIRECTORY = pathlib.Path(__file__).resolve().parents[1] / "spec" / "polish"


def stub(directory: pathlib.Path, name: str, body: str) -> pathlib.Path:
  directory.mkdir(parents=True, exist_ok=True)
  path = directory / name
  path.write_text(f"#!/bin/sh\n{body}\n", encoding="utf-8")
  path.chmod(0o755)
  return path


def environment(tmp_path: pathlib.Path, **extra: str) -> dict[str, str]:
  bin_dir = tmp_path / "bin"
  bin_dir.mkdir(exist_ok=True)
  home = tmp_path / "home"
  home.mkdir(exist_ok=True)
  return {
      "PATH": f"{bin_dir}:/usr/bin:/bin",
      "HOME": str(home),
      "XDG_CACHE_HOME": str(tmp_path / "cache"),
      **extra,
  }


def test_claude_template_puts_the_spec_in_a_file_and_prose_on_stdin(
    tmp_path: pathlib.Path,
):
  invocation = engines.expand("claude", "", SPEC, TEXT, tmp_path)
  assert invocation.argv == [
      "claude",
      "-p",
      "--system-prompt-file",
      str(tmp_path / engines.SPEC_FILENAME),
      "--model",
      "sonnet",
  ]
  assert invocation.stdin == TEXT
  assert invocation.output is None
  # Privacy boundary: a preset never runs in the caller's directory, so
  # the repository around the user is not context the engine can read.
  assert invocation.cwd == tmp_path
  assert (tmp_path / engines.SPEC_FILENAME).read_text(encoding="utf-8") == SPEC


def test_codex_template_prepends_the_spec_and_reads_an_answer_file(
    tmp_path: pathlib.Path,
):
  invocation = engines.expand("codex", "gpt-5.6-luna", SPEC, TEXT, tmp_path)
  output = tmp_path / engines.OUTPUT_FILENAME
  assert invocation.argv == [
      "codex",
      "exec",
      "--skip-git-repo-check",
      "--ephemeral",
      "-c",
      "model=gpt-5.6-luna",
      "-c",
      f"model_reasoning_effort={engines.CODEX_EFFORT}",
      "--output-last-message",
      str(output),
      "-",
  ]
  # No system channel: the spec leads, the prose follows the separator.
  assert invocation.stdin.startswith(SPEC)
  assert invocation.stdin.endswith(f"{engines.CODEX_SEPARATOR}\n{TEXT}")
  assert invocation.output == output
  assert invocation.cwd == tmp_path


def test_grok_template_passes_the_spec_and_the_prose_as_arguments(
    tmp_path: pathlib.Path,
):
  invocation = engines.expand("grok", "", SPEC, TEXT, tmp_path)
  assert invocation.argv == [
      "grok",
      "--system-prompt-override",
      SPEC,
      "-m",
      "grok-4.6",
      "--verbatim",
      "-p",
      TEXT,
  ]
  assert invocation.stdin == ""
  assert invocation.cwd == tmp_path


def test_custom_command_substitutes_both_placeholders(tmp_path: pathlib.Path):
  invocation = engines.expand(
      "custom",
      "",
      SPEC,
      TEXT,
      tmp_path,
      command=["mygateway", "--spec", "{spec_file}", "--", "{text}"],
  )
  spec_file = tmp_path / engines.SPEC_FILENAME
  assert invocation.argv == [
      "mygateway",
      "--spec",
      str(spec_file),
      "--",
      TEXT,
  ]
  # The prose is an argument here, so stdin stays empty.
  assert invocation.stdin == ""
  # A custom command keeps the caller's directory: it is the user's own
  # command, so that boundary is theirs to draw.
  assert invocation.cwd is None
  assert spec_file.read_text(encoding="utf-8") == SPEC


def test_custom_command_without_a_text_placeholder_uses_stdin(
    tmp_path: pathlib.Path,
):
  invocation = engines.expand(
      "custom", "", SPEC, TEXT, tmp_path, command=["mygateway", "{spec_file}"]
  )
  assert invocation.stdin == TEXT


def test_order_puts_the_host_engine_first(tmp_path: pathlib.Path):
  env = environment(tmp_path, CODEX_SESSION_ID="session")
  for name in engines.ENGINES:
    stub(pathlib.Path(env["PATH"].split(":")[0]), name, "exit 0")
  assert engines.order(env)[0] == "codex"


def test_order_sorts_installed_engines_by_credential_traces(
    tmp_path: pathlib.Path,
):
  env = environment(tmp_path)
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  for name in engines.ENGINES:
    stub(bin_dir, name, "exit 0")
  auth = pathlib.Path(env["HOME"]) / ".grok"
  auth.mkdir()
  (auth / "auth.json").write_text("{}", encoding="utf-8")
  assert engines.order(env) == ["grok", "claude", "codex"]


def test_a_missing_binary_is_the_only_hard_negative(tmp_path: pathlib.Path):
  # Credentials for claude, but no claude binary: it is out anyway.
  env = environment(tmp_path, ANTHROPIC_API_KEY=SYNTHETIC_VALUE)
  stub(pathlib.Path(env["PATH"].split(":")[0]), "codex", "exit 0")
  assert engines.order(env) == ["codex"]


def test_select_takes_the_first_engine_that_answers_and_caches_it(
    tmp_path: pathlib.Path,
):
  # The stubs count their own runs through a path written into the
  # script, not an environment variable: a preset engine no longer sees
  # the caller's environment.
  probes = tmp_path / "probes"
  env = environment(tmp_path)
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  count = f"echo run >> {probes}"
  stub(bin_dir, "claude", f'{count}\necho "HTTP 401" >&2\nexit 1')
  stub(
      bin_dir,
      "codex",
      f'{count}\ncat > /dev/null\nshift 8\necho {engines.PROBE_MARKER} > "$1"',
  )
  assert engines.select(env) == "codex"
  assert probes.read_text(encoding="utf-8").count("\n") == 2

  # Both results are cached for the TTL, the failure included: nothing
  # is probed again, even though claude would now answer.
  stub(
      bin_dir,
      "claude",
      f"{count}\ncat > /dev/null\necho {engines.PROBE_MARKER}",
  )
  assert engines.select(env) == "codex"
  assert probes.read_text(encoding="utf-8").count("\n") == 2


def test_select_skips_an_engine_that_exits_zero_without_answering(
    tmp_path: pathlib.Path,
):
  # An engine can exit 0 and still not have answered: it ignored the
  # spec, or printed an error of its own on stdout. The exit code alone
  # would cache it for the whole TTL and then write that text to stdout.
  env = environment(tmp_path)
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  stub(bin_dir, "claude", 'cat > /dev/null\necho "Let me first read the repo"')
  stub(
      bin_dir,
      "codex",
      f'cat > /dev/null\nshift 8\necho {engines.PROBE_MARKER} > "$1"',
  )
  assert engines.probe("claude", env) != engines.OK
  assert engines.select(env) == "codex"


def test_the_cache_never_outranks_the_host_marker(tmp_path: pathlib.Path):
  # What is cached is each engine's probe result, not which engine was
  # chosen: the ordering of ADR-0008 三 runs again every time, so the
  # session the user is in now still comes first (step 2).
  env = environment(tmp_path)
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  answer = f"cat > /dev/null\necho {engines.PROBE_MARKER}"
  stub(bin_dir, "claude", answer)
  stub(bin_dir, "grok", answer)
  assert engines.select(env) == "claude"

  # Same cache, different session: the choice follows the marker.
  assert engines.select(env | {"GROK_SESSION_ID": "session"}) == "grok"
  assert engines.select(env | {"CLAUDECODE": "1"}) == "claude"
  assert engines.select(env) == "claude"


def test_select_ignores_a_stale_cache(tmp_path: pathlib.Path):
  env = environment(tmp_path)
  cache = pathlib.Path(env["XDG_CACHE_HOME"]) / engines.CACHE_DIRECTORY
  cache.mkdir(parents=True)
  (cache / engines.CACHE_FILENAME).write_text(
      json.dumps({"engines": {"codex": {"state": engines.OK, "at": 0}}}),
      encoding="utf-8",
  )
  with pytest.raises(engines.EngineError):
    engines.select(env)


def test_select_diagnoses_every_engine_without_echoing_anything(
    tmp_path: pathlib.Path,
):
  env = environment(tmp_path, ANTHROPIC_API_KEY=SYNTHETIC_VALUE)
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  # claude has a credential and is rejected; grok cannot reach anything;
  # codex is not installed at all.
  stub(
      bin_dir,
      "claude",
      f'echo "401 unauthorized for {SYNTHETIC_VALUE}" >&2\nexit 1',
  )
  stub(bin_dir, "grok", 'echo "getaddrinfo ENOTFOUND api.example" >&2\nexit 1')

  with pytest.raises(engines.EngineError) as caught:
    engines.select(env)
  message = str(caught.value)
  assert f"claude: {engines.UNAUTHORIZED}" in message
  assert f"codex: {engines.MISSING}" in message
  assert f"grok: {engines.UNREACHABLE}" in message
  # Every line names a next step, and no engine output is quoted back.
  assert engines.NEXT_STEP[engines.MISSING] in message
  assert SYNTHETIC_VALUE not in message
  assert "unauthorized for" not in message


def test_an_engine_with_no_credential_trace_is_diagnosed_as_such(
    tmp_path: pathlib.Path,
):
  env = environment(tmp_path)
  stub(pathlib.Path(env["PATH"].split(":")[0]), "grok", "exit 1")
  assert engines.probe("grok", env) == engines.NO_CREDENTIALS


# A stub that answers with its own environment and working directory, so
# a test can assert what the boundary lets through. `env` prints one
# NAME=value line each; the assertions below only ever name variables.
DUMP = 'cat > /dev/null\nenv\necho "cwd=$(pwd)"'
# Stand-ins for the caller's context, none of them real: a repository
# path, a git variable, and the host-session markers of ADR-0008 三 2.
CALLER_DIRECTORY = "/home/nobody/private-repo"
CALLER_CONTEXT = {
    "PWD": CALLER_DIRECTORY,
    "OLDPWD": CALLER_DIRECTORY,
    "GIT_DIR": f"{CALLER_DIRECTORY}/.git",
    "GIT_AUTHOR_NAME": "Nobody",
    "CLAUDECODE": "1",
    "CODEX_SESSION_ID": "session",
    "GROK_SESSION_ID": "session",
}


def test_a_preset_sees_no_repository_or_session_context(
    tmp_path: pathlib.Path,
):
  env = environment(
      tmp_path, **CALLER_CONTEXT, ANTHROPIC_API_KEY=SYNTHETIC_VALUE
  )
  stub(pathlib.Path(env["PATH"].split(":")[0]), "claude", DUMP)
  seen = engines.polish("claude", "", SPEC, TEXT, env)

  # Nothing about where the user was standing.
  assert CALLER_DIRECTORY not in seen
  assert "GIT_DIR" not in seen
  assert "GIT_AUTHOR_NAME" not in seen
  # No host-session marker: it is both a session identity and a way for
  # the child to think it is a nested session.
  for marker in ("CLAUDECODE", "CODEX_SESSION_ID", "GROK_SESSION_ID"):
    assert marker not in seen
  # What is left is what the CLI needs to run and to find its login,
  # its own vendor's variables included and no other vendor's.
  assert "PATH=" in seen
  assert "HOME=" in seen
  assert "ANTHROPIC_API_KEY=" in seen
  assert "OPENAI_API_KEY" not in seen
  # The directory variables point at the throwaway directory the engine
  # actually runs in.
  workdir = next(
      line.removeprefix("cwd=")
      for line in seen.splitlines()
      if line.startswith("cwd=")
  )
  assert f"PWD={workdir}" in seen


def test_a_custom_command_keeps_the_callers_environment(
    tmp_path: pathlib.Path,
):
  env = environment(tmp_path, **CALLER_CONTEXT)
  stub(pathlib.Path(env["PATH"].split(":")[0]), "mygateway", DUMP)
  seen = engines.polish("custom", "", SPEC, TEXT, env, command=["mygateway"])
  # The user wrote this command, so its boundary is theirs to draw.
  assert CALLER_DIRECTORY in seen


def test_polish_returns_what_the_engine_answered(tmp_path: pathlib.Path):
  env = environment(tmp_path)
  stub(pathlib.Path(env["PATH"].split(":")[0]), "mygateway", "tr a-z A-Z")
  answer = engines.polish(
      "custom", "", SPEC, "polished please", env, command=["mygateway"]
  )
  assert answer == "POLISHED PLEASE\n"


def test_polish_reports_a_failure_without_the_engine_output(
    tmp_path: pathlib.Path,
):
  env = environment(tmp_path)
  stub(
      pathlib.Path(env["PATH"].split(":")[0]),
      "mygateway",
      f'echo "403 forbidden {SYNTHETIC_VALUE}" >&2\nexit 1',
  )
  with pytest.raises(engines.EngineError) as caught:
    engines.polish("custom", "", SPEC, TEXT, env, command=["mygateway"])
  assert SYNTHETIC_VALUE not in str(caught.value)
  assert engines.UNAUTHORIZED in str(caught.value)


def test_an_empty_answer_is_a_failure(tmp_path: pathlib.Path):
  env = environment(tmp_path)
  stub(pathlib.Path(env["PATH"].split(":")[0]), "mygateway", "exit 0")
  with pytest.raises(engines.EngineError):
    engines.polish("custom", "", SPEC, TEXT, env, command=["mygateway"])


def test_assemble_adds_the_chinese_layer_only_for_chinese():
  # Also checks that the package symlink resolves to spec/polish/.
  english = polish.assemble("An English paragraph about ACME.")
  chinese = polish.assemble(TEXT)
  assert english == (SPEC_DIRECTORY / polish.GENERAL_SPEC).read_text(
      encoding="utf-8"
  )
  assert chinese.startswith(english)
  assert (SPEC_DIRECTORY / polish.CHINESE_SPEC).read_text(
      encoding="utf-8"
  ) in chinese


def test_polish_config_defaults_to_auto(tmp_path: pathlib.Path):
  settings = config.resolve_polish(tmp_path, engines.PRESETS)
  assert settings == config.PolishSettings("auto", "", ())


def test_polish_config_reads_the_standalone_file(tmp_path: pathlib.Path):
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "codex"\nmodel = "gpt-5.6-terra"\n', encoding="utf-8"
  )
  settings = config.resolve_polish(tmp_path, engines.PRESETS)
  assert settings == config.PolishSettings("codex", "gpt-5.6-terra", ())


def test_polish_config_reads_the_pyproject_table(tmp_path: pathlib.Path):
  (tmp_path / "pyproject.toml").write_text(
      '[tool.limae.polish]\nengine = "custom"\ncommand = ["mygateway",'
      ' "{spec_file}"]\n',
      encoding="utf-8",
  )
  settings = config.resolve_polish(tmp_path, engines.PRESETS)
  assert settings.engine == "custom"
  assert settings.command == ("mygateway", "{spec_file}")


def test_polish_config_rejects_an_unknown_engine(tmp_path: pathlib.Path):
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "gemini"\n', encoding="utf-8"
  )
  with pytest.raises(config.ConfigError):
    config.resolve_polish(tmp_path, engines.PRESETS)


def test_polish_config_rejects_custom_without_a_command(
    tmp_path: pathlib.Path,
):
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "custom"\n', encoding="utf-8"
  )
  with pytest.raises(config.ConfigError):
    config.resolve_polish(tmp_path, engines.PRESETS)


def test_polish_config_rejects_a_command_without_custom(
    tmp_path: pathlib.Path,
):
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "claude"\ncommand = ["mygateway"]\n', encoding="utf-8"
  )
  with pytest.raises(config.ConfigError):
    config.resolve_polish(tmp_path, engines.PRESETS)


def run_cli(
    argv: list[str],
    text: str,
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    **extra: str,
) -> int:
  for name, value in environment(tmp_path, **extra).items():
    monkeypatch.setenv(name, value)
  monkeypatch.chdir(tmp_path)
  monkeypatch.setattr(sys, "stdin", io.StringIO(text))
  return polish.main(argv)


def test_cli_writes_the_polished_text_to_stdout(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  stub(tmp_path / "bin", "mygateway", "tr a-z A-Z")
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "custom"\ncommand = ["mygateway"]\n', encoding="utf-8"
  )
  assert run_cli(["-"], "the acme report\n", tmp_path, monkeypatch) == 0
  assert capsys.readouterr().out == "THE ACME REPORT\n"


def test_cli_engine_flag_beats_the_environment_and_the_config(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "claude"\n', encoding="utf-8"
  )
  code = run_cli(
      ["-", "--engine", "gemini"],
      TEXT,
      tmp_path,
      monkeypatch,
      LIMAE_ENGINE="grok",
  )
  assert code == polish.BAD_USAGE
  assert "unknown engine 'gemini'" in capsys.readouterr().err


def test_cli_reports_a_failing_engine_and_exits_non_zero(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  stub(tmp_path / "bin", "mygateway", "exit 1")
  (tmp_path / "limae.toml").write_text(
      '[polish]\nengine = "custom"\ncommand = ["mygateway"]\n', encoding="utf-8"
  )
  assert run_cli(["-"], TEXT, tmp_path, monkeypatch) == polish.FAILED
  captured = capsys.readouterr()
  assert captured.out == ""
  assert "engine error:" in captured.err


def test_cli_rejects_empty_input(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  assert run_cli(["-"], "  \n", tmp_path, monkeypatch) == polish.BAD_USAGE
  assert "nothing on stdin" in capsys.readouterr().err


def test_cli_rejects_a_file_argument_for_now(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  with pytest.raises(SystemExit) as caught:
    run_cli(["doc.md"], TEXT, tmp_path, monkeypatch)
  assert caught.value.code == 2


def test_the_entry_point_dispatches_the_subcommand(
    monkeypatch: pytest.MonkeyPatch,
):
  seen: list[list[str]] = []

  def record(argv: list[str]) -> int:
    seen.append(list(argv))
    return 0

  monkeypatch.setattr(sys, "argv", ["limae", "polish", "-", "--engine", "grok"])
  monkeypatch.setattr(polish, "main", record)
  assert zh_format.main() == 0
  assert seen == [["-", "--engine", "grok"]]
