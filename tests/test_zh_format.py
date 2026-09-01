import pathlib
import sys

import pytest

from lo_md_lint import wordlists, zh_format


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
  assert (
      f"{p}:1: error: [R1 halfwidth punct next to CJK]"
      in capsys.readouterr().out
  )
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


def test_cli_enable_turns_a_default_off_rule_on(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  p = tmp_path / "t.md"
  p.write_text("中文[链接](https://example.com/) 后文", encoding="utf-8")
  assert run([str(p)], monkeypatch) == 0  # R9 is off by default
  assert run(["--enable", "R9", str(p)], monkeypatch) == 1
  assert run(["--enable", "R9", "--fix", str(p)], monkeypatch) == 0
  expected = "中文 [链接](https://example.com/) 后文"
  assert p.read_text(encoding="utf-8") == expected


def test_config_enable_key_turns_a_default_off_rule_on(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'enable = ["R9"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text(
      "中文[链接](https://example.com/) 后文", encoding="utf-8"
  )
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 1


def test_same_id_disabled_and_enabled_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  assert run(["--disable", "R9", "--enable", "R9", str(p)], monkeypatch) == 2
  assert "in both" in capsys.readouterr().err


def test_unknown_rule_id_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  assert run(["--disable", "R99", str(p)], monkeypatch) == 2
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


def test_config_skip_zh_units_exempts_a_date(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'skip_zh_units = "年月日"\n', encoding="utf-8"
  )
  p = tmp_path / "t.md"
  p.write_text("他2011年5月15日入职\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["--fix", "t.md"], monkeypatch) == 0
  assert p.read_text(encoding="utf-8") == "他2011年5月15日入职\n"


def test_cli_flag_drops_the_config_files_skip_zh_units(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'skip_zh_units = "年"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("共2011年\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # A CLI flag replaces the config file wholesale, this key included.
  assert run(["--disable", "R1", "t.md"], monkeypatch) == 1


def test_non_string_skip_zh_units_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'skip_zh_units = ["年"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "must be a string of CJK characters" in capsys.readouterr().err


def test_non_cjk_skip_zh_units_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'skip_zh_units = "年 月"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "must be a string of CJK characters" in capsys.readouterr().err


def test_severity_key_downgrades_a_rule_to_warning(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'severity = { R1 = "warning" }\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # Reported and told apart from an error, but the run still passes.
  assert run(["t.md"], monkeypatch) == 0
  out = capsys.readouterr().out
  assert "t.md:1: warning: [R1" in out
  assert "0 error(s), 1 warning(s)" in out


def test_a_warning_rule_is_still_fixed(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'severity = { R1 = "warning" }\n', encoding="utf-8"
  )
  p = tmp_path / "t.md"
  p.write_text("你好,世界\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # Severity drives the exit code, never the fix.
  assert run(["--fix", "t.md"], monkeypatch) == 0
  assert p.read_text(encoding="utf-8") == "你好，世界\n"


def test_bad_severity_value_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'severity = { R1 = "fatal" }\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "must be 'error' or 'warning'" in capsys.readouterr().err


def test_enable_experimental_joins_the_experimental_rules(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      "enable_experimental = true\n", encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("综上所述，这条路走不通。\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # The experimental rules are warnings, so the run still passes.
  assert run(["t.md"], monkeypatch) == 0
  assert "t.md:1: warning: [A1 formulaic phrase]" in capsys.readouterr().out


def test_experimental_id_in_enable_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'enable = ["A1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "cannot be enabled one by one" in capsys.readouterr().err


def test_non_boolean_enable_experimental_is_a_config_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / "lo-md-lint.toml").write_text(
      'enable_experimental = "true"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["t.md"], monkeypatch) == 2
  assert "must be a boolean" in capsys.readouterr().err


def test_unknown_rule_id_in_a_directive_is_an_error(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  p = tmp_path / "t.md"
  p.write_text("<!-- lo-md-lint-disable R99 -->\n你好,世界\n", encoding="utf-8")
  assert run([str(p)], monkeypatch) == 2
  assert "unknown rule id" in capsys.readouterr().err


def test_ignore_file_skips_an_explicitly_listed_file(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".lo-md-lint-ignore").write_text("vendor/\n", encoding="utf-8")
  (tmp_path / "vendor").mkdir()
  p = tmp_path / "vendor" / "t.md"
  p.write_text("你好,世界\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  # Explicit, not --all: pre-commit passes the files it staged.
  assert run(["--fix", "vendor/t.md"], monkeypatch) == 0
  assert p.read_text(encoding="utf-8") == "你好,世界\n"


def test_ignore_file_is_found_above_the_cwd(
    tmp_path: pathlib.Path, monkeypatch: pytest.MonkeyPatch
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".lo-md-lint-ignore").write_text("*.md\n", encoding="utf-8")
  sub = tmp_path / "sub"
  sub.mkdir()
  (sub / "t.md").write_text("你好,世界\n", encoding="utf-8")
  monkeypatch.chdir(sub)
  assert run(["t.md"], monkeypatch) == 0


def test_ignore_file_negation_keeps_a_file(
    tmp_path: pathlib.Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".lo-md-lint-ignore").write_text(
      "*.md\n!keep.md\n", encoding="utf-8"
  )
  (tmp_path / "skip.md").write_text("你好,世界\n", encoding="utf-8")
  (tmp_path / "keep.md").write_text("你好,世界\n", encoding="utf-8")
  monkeypatch.chdir(tmp_path)
  assert run(["skip.md", "keep.md"], monkeypatch) == 1
  out = capsys.readouterr().out
  assert "keep.md:1" in out
  assert "skip.md" not in out


def test_wordlists_load_from_the_packaged_spec_directory():
  # src/lo_md_lint/wordlists is a symlink to spec/wordlists; the phrases
  # and terms must be readable through the installed package either way.
  assert "综上所述" in wordlists.phrases("A1")
  assert "testament" in wordlists.phrases("A5")
  assert "load-bearing" in wordlists.phrases("A7")
  assert not [p for p in wordlists.phrases("A1") if p.startswith("#")]
  assert [t for t in wordlists.terms() if t.wrong == "代币"] == [
      wordlists.Term("代币", "令牌", ("token", "OAuth", "JWT", "鉴权", "认证"))
  ]
