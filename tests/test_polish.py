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
  probes = tmp_path / "probes"
  env = environment(tmp_path, PROBES=str(probes))
  bin_dir = pathlib.Path(env["PATH"].split(":")[0])
  stub(bin_dir, "claude", 'echo "$0" >> "$PROBES"\necho "HTTP 401" >&2\nexit 1')
  stub(
      bin_dir,
      "codex",
      'echo "$0" >> "$PROBES"\ncat > /dev/null\nshift 8\necho PONG > "$1"',
  )
  assert engines.select(env) == "codex"
  assert probes.read_text(encoding="utf-8").count("\n") == 2

  # The cached answer is trusted for its TTL: no engine is probed again,
  # even though claude would now answer first.
  stub(bin_dir, "claude", 'echo "$0" >> "$PROBES"\ncat > /dev/null\necho PONG')
  assert engines.select(env) == "codex"
  assert probes.read_text(encoding="utf-8").count("\n") == 2


def test_select_ignores_a_stale_cache(tmp_path: pathlib.Path):
  env = environment(tmp_path)
  cache = pathlib.Path(env["XDG_CACHE_HOME"]) / engines.CACHE_DIRECTORY
  cache.mkdir(parents=True)
  (cache / engines.CACHE_FILENAME).write_text(
      json.dumps({"engine": "codex", "at": 0}), encoding="utf-8"
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
