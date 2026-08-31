import pathlib
import sys

import pytest

from lo_md_lint import zh_format


# The rule behaviour itself lives in the language-agnostic golden set; see
# spec/README.md and tests/test_fixtures.py. Only the Python-specific surface
# is tested here.
def test_cli_reports_then_fixes(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  monkeypatch.setattr(sys, "argv", ["lo-md-lint", str(p)])
  assert zh_format.main() == 1
  assert f"{p}:1: [R1 halfwidth punct next to CJK]" in capsys.readouterr().out
  monkeypatch.setattr(sys, "argv", ["lo-md-lint", "--fix", str(p)])
  assert zh_format.main() == 0
  assert p.read_text(encoding="utf-8") == "你好，世界"


def test_tracked_markdown_lists_md_files():
  # Runs inside this repository.
  paths = zh_format.tracked_markdown()
  assert paths, "expected tracked markdown files in the repo"
  assert all(p.suffix == ".md" for p in paths)


def run(argv: list[str], monkeypatch: pytest.MonkeyPatch) -> int:
  monkeypatch.setattr(sys, "argv", ["lo-md-lint", *argv])
  return zh_format.main()


def test_cli_disable_flag_turns_a_rule_off(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  assert run(["--fix", "--disable", "R1", str(p)], monkeypatch) == 0
  assert p.read_text(encoding="utf-8") == "你好,世界"


def test_standalone_config_file_turns_a_rule_off(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'disable = ["R1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 0


def test_pyproject_table_turns_a_rule_off(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "pyproject.toml").write_text(
      '[tool.lo-md-lint]\ndisable = ["R1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 0


def test_unknown_rule_id_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  assert run(["--disable", "R9", str(p)], monkeypatch) == 2
  assert "unknown rule id" in capsys.readouterr().err


def test_cli_disable_replaces_the_config_file(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'disable = ["R1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # Wholesale override, not a merge: R3 goes off and R1 comes back on.
  assert run(["--disable", "R3", "t.md"], monkeypatch) == 1


def test_standalone_file_wins_over_pyproject_table(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'disable = ["R1"]\n', encoding="utf-8"
  )
  (tmp_path / "pyproject.toml").write_text(
      "[tool.lo-md-lint]\ndisable = []\n", encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 0


def test_invalid_toml_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  # Not a config source of ours, but an unparseable candidate still stops
  # the search rather than silently walking past a possible config.
  (tmp_path / "pyproject.toml").write_text("[project\n", encoding="utf-8")
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "pyproject.toml" in capsys.readouterr().err


def test_non_list_disable_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'disable = "R1"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "must be a list of rule ids" in capsys.readouterr().err
