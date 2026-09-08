import os
import pathlib
import subprocess

from cli_runner import CliRunner
import pytest

from limae import config, wordlists, zh_format


# The rule behaviour itself lives in the language-agnostic golden set; see
# spec/README.md and tests/test_fixtures.py. CLI contracts run against every
# selected binary; the remaining direct calls cover Python-only library APIs.
def test_cli_reports_then_fixes(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 1
  assert completed.stderr == ""
  assert completed.stdout == (
      "t.md:1: error: [zh-typography-1 halfwidth punct next to CJK]"
      " …你好,世界…\n\n1 error(s), 0 warning(s). --fix auto-fixes most.\n"
  )
  completed = limae_cli.run(["--fix", "t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "fixed: t.md\nOK: 1 file(s) clean\n"
  assert completed.stderr == ""
  assert p.read_text(encoding="utf-8") == "你好，世界"


def test_tracked_markdown_lists_md_files():
  # Runs inside this repository.
  paths = zh_format.tracked_markdown()
  assert paths, "expected tracked markdown files in the repo"
  assert all(p.suffix == ".md" for p in paths)


@pytest.mark.parametrize("pattern", ["!\n", "trailing\\\n"])
def test_invalid_ignore_keeps_a_named_failure_at_exit_one(
    tmp_path: pathlib.Path, limae_cli: CliRunner, pattern: str
):
  invalid = tmp_path / "invalid"
  invalid.mkdir()
  (invalid / ".limae-ignore").write_text(pattern, encoding="utf-8")
  (invalid / "t.md").write_text("ACME\n", encoding="utf-8")
  completed = limae_cli.run(["t.md"], invalid)
  assert completed.returncode == 1
  assert completed.stdout == ""
  if limae_cli.name == "python":
    assert "GitIgnorePatternError" in completed.stderr
    assert "Invalid git pattern:" in completed.stderr
  else:
    assert "invalid ignore pattern" in completed.stderr
    assert ".limae-ignore" in completed.stderr
  assert "clean" not in completed.stderr

  control = tmp_path / "control"
  control.mkdir()
  (control / ".limae-ignore").write_text("[abc\n", encoding="utf-8")
  (control / "t.md").write_text("你好,世界\n", encoding="utf-8")
  completed = limae_cli.run(["t.md"], control)
  assert completed.returncode == 1
  assert "t.md:1: error: [zh-typography-1" in completed.stdout
  assert completed.stderr == ""


def test_cli_disable_flag_turns_a_rule_off(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(
      ["--fix", "--disable", "zh-typography-1", "t.md"],
      tmp_path,
  )
  assert completed.returncode == 0
  assert completed.stdout == "OK: 1 file(s) clean\n"
  assert completed.stderr == ""
  assert p.read_text(encoding="utf-8") == "你好,世界"


def test_standalone_config_file_turns_a_rule_off(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'disable = ["zh-typography-1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert (completed.returncode, completed.stdout, completed.stderr) == (
      0,
      "OK: 1 file(s) clean\n",
      "",
  )


def test_pyproject_table_turns_a_rule_off(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "pyproject.toml").write_text(
      '[tool.limae]\ndisable = ["zh-typography-1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "OK: 1 file(s) clean\n"
  assert completed.stderr == ""


def test_cli_enable_turns_a_default_off_rule_on(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  p = tmp_path / "t.md"
  p.write_text("中文[链接](https://example.com/) 后文", encoding="utf-8")
  assert limae_cli.run(["t.md"], tmp_path).returncode == 0
  completed = limae_cli.run(["--enable", "zh-typography-9", "t.md"], tmp_path)
  assert completed.returncode == 1
  assert "t.md:1: error: [zh-typography-9" in completed.stdout
  completed = limae_cli.run(
      ["--enable", "zh-typography-9", "--fix", "t.md"],
      tmp_path,
  )
  assert completed.returncode == 0
  assert completed.stdout == "fixed: t.md\nOK: 1 file(s) clean\n"
  expected = "中文 [链接](https://example.com/) 后文"
  assert p.read_text(encoding="utf-8") == expected


def test_config_enable_key_turns_a_default_off_rule_on(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'enable = ["zh-typography-9"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text(
      "中文[链接](https://example.com/) 后文", encoding="utf-8"
  )
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 1
  assert "t.md:1: error: [zh-typography-9" in completed.stdout
  assert completed.stderr == ""


def test_same_id_disabled_and_enabled_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(
      [
          "--disable",
          "zh-typography-9",
          "--enable",
          "zh-typography-9",
          "t.md",
      ],
      tmp_path,
  )
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "in both" in completed.stderr


def test_unknown_rule_id_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  p = tmp_path / "t.md"
  p.write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["--disable", "R99", "t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "unknown rule id" in completed.stderr


def test_cli_disable_replaces_the_config_file(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'disable = ["zh-typography-1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  # Wholesale override, not a merge: zh-typography-3 goes off and
  # zh-typography-1 comes back on.
  completed = limae_cli.run(["--disable", "zh-typography-3", "t.md"], tmp_path)
  assert completed.returncode == 1
  assert "t.md:1: error: [zh-typography-1" in completed.stdout


def test_standalone_file_wins_over_pyproject_table(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'disable = ["zh-typography-1"]\n', encoding="utf-8"
  )
  (tmp_path / "pyproject.toml").write_text(
      "[tool.limae]\ndisable = []\n", encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "OK: 1 file(s) clean\n"


def test_invalid_toml_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  # Not a config source of ours, but an unparseable candidate still stops
  # the search rather than silently walking past a possible config.
  (tmp_path / "pyproject.toml").write_text("[project\n", encoding="utf-8")
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "config error" in completed.stderr
  assert "pyproject.toml" in completed.stderr


def test_non_list_disable_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'disable = "zh-typography-1"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "limae.toml" in completed.stderr
  assert "`disable`" in completed.stderr
  assert "must be a list of rule ids" in completed.stderr


def test_config_skip_zh_units_exempts_a_date(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'skip_zh_units = "年月日"\n', encoding="utf-8"
  )
  p = tmp_path / "t.md"
  p.write_text("他2011年5月15日入职\n", encoding="utf-8")
  completed = limae_cli.run(["--fix", "t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "OK: 1 file(s) clean\n"
  assert p.read_text(encoding="utf-8") == "他2011年5月15日入职\n"


def test_cli_flag_drops_the_config_files_skip_zh_units(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'skip_zh_units = "年"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("共2011年\n", encoding="utf-8")
  # A CLI flag replaces the config file wholesale, this key included.
  completed = limae_cli.run(["--disable", "zh-typography-1", "t.md"], tmp_path)
  assert completed.returncode == 1
  assert "t.md:1: error: [zh-typography-5" in completed.stdout


def test_non_string_skip_zh_units_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'skip_zh_units = ["年"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "`skip_zh_units`" in completed.stderr
  assert "must be a string of CJK characters" in completed.stderr


def test_non_cjk_skip_zh_units_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'skip_zh_units = "年 月"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "`skip_zh_units`" in completed.stderr
  assert "must be a string of CJK characters" in completed.stderr


def test_severity_key_downgrades_a_rule_to_warning(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'severity = { zh-typography-1 = "warning" }\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界\n", encoding="utf-8")
  # Reported and told apart from an error, but the run still passes.
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stderr == ""
  assert "t.md:1: warning: [zh-typography-1" in completed.stdout
  assert "0 error(s), 1 warning(s)" in completed.stdout


def test_a_warning_rule_is_still_fixed(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / "limae.toml").write_text(
      'severity = { zh-typography-1 = "warning" }\n', encoding="utf-8"
  )
  p = tmp_path / "t.md"
  p.write_text("你好,世界\n", encoding="utf-8")
  # Severity drives the exit code, never the fix.
  completed = limae_cli.run(["--fix", "t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "fixed: t.md\nOK: 1 file(s) clean\n"
  assert p.read_text(encoding="utf-8") == "你好，世界\n"


def test_bad_severity_value_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'severity = { zh-typography-1 = "fatal" }\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "`severity`" in completed.stderr
  assert "must be 'error' or 'warning'" in completed.stderr


def test_enable_experimental_joins_the_experimental_rules(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      "enable_experimental = true\n", encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("综上所述，这条路走不通。\n", encoding="utf-8")
  # The experimental rules are warnings, so the run still passes.
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stderr == ""
  assert "t.md:1: warning: [zh-tell-1 formulaic phrase]" in completed.stdout


def test_experimental_id_in_enable_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'enable = ["zh-tell-1"]\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "`enable`" in completed.stderr
  assert "cannot be enabled one by one" in completed.stderr


def test_non_boolean_enable_experimental_is_a_config_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / "limae.toml").write_text(
      'enable_experimental = "true"\n', encoding="utf-8"
  )
  (tmp_path / "t.md").write_text("你好,世界", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "`enable_experimental`" in completed.stderr
  assert "must be a boolean" in completed.stderr


def test_unknown_rule_id_in_a_directive_is_an_error(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  p = tmp_path / "t.md"
  p.write_text("<!-- limae-disable R99 -->\n你好,世界\n", encoding="utf-8")
  completed = limae_cli.run(["t.md"], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "directive error" in completed.stderr
  assert "t.md:1" in completed.stderr
  assert "unknown rule id" in completed.stderr


def test_ignore_file_skips_an_explicitly_listed_file(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".limae-ignore").write_text("vendor/\n", encoding="utf-8")
  (tmp_path / "vendor").mkdir()
  p = tmp_path / "vendor" / "t.md"
  p.write_text("你好,世界\n", encoding="utf-8")
  # Explicit, not --all: pre-commit passes the files it staged.
  completed = limae_cli.run(["--fix", "vendor/t.md"], tmp_path)
  assert completed.returncode == 0
  assert completed.stdout == "OK: 0 file(s) clean\n"
  assert completed.stderr == ""
  assert p.read_text(encoding="utf-8") == "你好,世界\n"


def test_ignore_file_is_found_above_the_cwd(
    tmp_path: pathlib.Path, limae_cli: CliRunner
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".limae-ignore").write_text("*.md\n", encoding="utf-8")
  sub = tmp_path / "sub"
  sub.mkdir()
  (sub / "t.md").write_text("你好,世界\n", encoding="utf-8")
  completed = limae_cli.run(["t.md"], sub)
  assert completed.returncode == 0
  assert completed.stdout == "OK: 0 file(s) clean\n"
  assert completed.stderr == ""


def test_ignore_file_negation_keeps_a_file(
    tmp_path: pathlib.Path,
    limae_cli: CliRunner,
):
  (tmp_path / ".git").mkdir()
  (tmp_path / ".limae-ignore").write_text("*.md\n!keep.md\n", encoding="utf-8")
  (tmp_path / "skip.md").write_text("你好,世界\n", encoding="utf-8")
  (tmp_path / "keep.md").write_text("你好,世界\n", encoding="utf-8")
  completed = limae_cli.run(["skip.md", "keep.md"], tmp_path)
  assert completed.returncode == 1
  assert completed.stderr == ""
  assert "keep.md:1" in completed.stdout
  assert "skip.md" not in completed.stdout


def _git(cwd: pathlib.Path, *args: str) -> None:
  completed = subprocess.run(  # noqa: S603 - fixed VCS command
      ["git", *args],
      cwd=cwd,
      env={
          "GIT_CONFIG_GLOBAL": "/dev/null",
          "GIT_CONFIG_NOSYSTEM": "1",
          "PATH": os.environ.get("PATH", os.defpath),
      },
      check=False,
      capture_output=True,
      text=True,
      timeout=10,
  )
  assert completed.returncode == 0, completed.stderr


def test_all_uses_only_tracked_markdown_and_preserves_git_order(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  _git(tmp_path, "init", "-q")
  (tmp_path / "b.md").write_text("文B\n", encoding="utf-8")
  (tmp_path / "a.md").write_text("中A\n", encoding="utf-8")
  (tmp_path / "untracked.md").write_text("你好,世界\n", encoding="utf-8")
  _git(tmp_path, "add", "b.md", "a.md")

  completed = limae_cli.run(["--all"], tmp_path)

  assert completed.returncode == 1
  # Diagnostic output is the one place the arms may differ: the Python
  # reference is frozen (user ruling 2026-09-08) while Rust adds a note so a
  # new file that selection cannot see stops looking like a clean tree.
  # Checking behaviour — findings, selection, stdout, exit code — stays
  # identical, so each arm states its own exact stderr instead of skipping it.
  assert completed.stderr == (
      "note: 1 untracked *.md not checked (git add them to include)\n"
      if limae_cli.name == "rust"
      else ""
  )
  assert completed.stdout == (
      "a.md:1: error: [zh-typography-4 no space between CJK and Latin] …中A…\n"
      "b.md:1: error: [zh-typography-4 no space between CJK and Latin] …文B…\n"
      "\n2 error(s), 0 warning(s). --fix auto-fixes most.\n"
  )
  assert "untracked.md" not in completed.stdout


def test_no_input_is_usage_but_all_ignored_is_clean(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  completed = limae_cli.run([], tmp_path)
  assert completed.returncode == 2
  assert completed.stdout == ""
  assert "no files given (use --all or list files)" in completed.stderr
  assert "usage:" in completed.stderr.lower()

  _git(tmp_path, "init", "-q")
  (tmp_path / ".limae-ignore").write_text("*.md\n", encoding="utf-8")
  (tmp_path / "t.md").write_text("你好,世界\n", encoding="utf-8")
  _git(tmp_path, "add", "t.md")
  completed = limae_cli.run(["--all"], tmp_path)
  assert (completed.returncode, completed.stdout, completed.stderr) == (
      0,
      "OK: 0 file(s) clean\n",
      "",
  )


def test_warning_and_nonfixable_error_keep_complete_output(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  (tmp_path / "limae.toml").write_text(
      'enable_experimental = true\nseverity = { zh-tell-1 = "error" }\n',
      encoding="utf-8",
  )
  (tmp_path / "t.md").write_text("综上所述，这条路走不通。\n", encoding="utf-8")
  completed = limae_cli.run(["--fix", "t.md"], tmp_path)
  assert completed.returncode == 1
  assert completed.stderr == ""
  assert completed.stdout == (
      "t.md:1: error: [zh-tell-1 formulaic phrase]"
      " …综上所述，这条路走不通。…\n"
      "\n1 error(s), 0 warning(s). --fix auto-fixes most.\n"
  )


def test_later_file_failure_keeps_the_prior_write(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  (tmp_path / "first.md").write_text("你好,世界\n", encoding="utf-8")
  completed = limae_cli.run(["--fix", "first.md", "missing.md"], tmp_path)
  assert completed.returncode == 1
  assert completed.stdout == "fixed: first.md\n"
  assert "missing.md" in completed.stderr
  assert "No such file or directory" in completed.stderr
  assert (tmp_path / "first.md").read_bytes() == "你好，世界\n".encode()


def test_fix_preserves_unchanged_bytes_mtime_and_link_identity(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  clean = tmp_path / "clean.md"
  clean.write_bytes(b"ACME\r\n")
  clean.chmod(0o640)
  before = clean.stat()
  completed = limae_cli.run(["--fix", "clean.md"], tmp_path)
  after = clean.stat()
  assert (completed.returncode, completed.stdout, completed.stderr) == (
      0,
      "OK: 1 file(s) clean\n",
      "",
  )
  assert clean.read_bytes() == b"ACME\r\n"
  assert after.st_mtime_ns == before.st_mtime_ns
  assert after.st_mode == before.st_mode

  target = tmp_path / "target.md"
  target.write_text("你好,世界", encoding="utf-8")
  target.chmod(0o640)
  link = tmp_path / "link.md"
  link.symlink_to(target.name)
  before = target.stat()
  completed = limae_cli.run(["--fix", "link.md"], tmp_path)
  after = target.stat()
  assert (completed.returncode, completed.stdout, completed.stderr) == (
      0,
      "fixed: link.md\nOK: 1 file(s) clean\n",
      "",
  )
  assert link.is_symlink()
  assert target.read_bytes() == "你好，世界".encode()
  assert after.st_ino == before.st_ino
  assert after.st_mode == before.st_mode


def test_changed_newlines_follow_universal_newline_writeback(
    tmp_path: pathlib.Path, limae_cli: CliRunner
) -> None:
  path = tmp_path / "t.md"
  path.write_bytes("你好,世界\r\n下一行".encode())
  completed = limae_cli.run(["--fix", "t.md"], tmp_path)
  assert (completed.returncode, completed.stdout, completed.stderr) == (
      0,
      "fixed: t.md\nOK: 1 file(s) clean\n",
      "",
  )
  assert path.read_bytes() == "你好，世界\n下一行".encode()


@pytest.mark.parametrize(
    ("pattern", "names", "kept"),
    [
        ("[ab].md", ["a.md", "b.md", "c.md"], ["c.md"]),
        ("a/b.md", ["a/b.md", "a/c.md"], ["a/c.md"]),
        ("[ab.md", ["[ab.md", "a.md"], ["[ab.md", "a.md"]),
        (
            "[a/]b.md",
            ["ab.md", "[a/]b.md", "a/b.md"],
            ["ab.md", "[a/]b.md", "a/b.md"],
        ),
    ],
)
def test_ignore_character_classes_stay_within_path_segments(
    tmp_path: pathlib.Path, pattern: str, names: list[str], kept: list[str]
):
  (tmp_path / ".limae-ignore").write_text(pattern + "\n", encoding="utf-8")
  paths = [tmp_path / name for name in names]
  assert config.not_ignored(paths, tmp_path) == [
      tmp_path / name for name in kept
  ]


def test_wordlists_load_from_the_packaged_spec_directory():
  # src/limae/wordlists is a symlink to spec/wordlists; the phrases
  # and terms must be readable through the installed package either way.
  assert "综上所述" in wordlists.phrases("zh-tell-1")
  assert "testament" in wordlists.phrases("en-tell-1")
  assert "load-bearing" in wordlists.phrases("en-tell-3")
  assert "零售" in wordlists.phrases("zh-tell-5-allow")
  assert "保守秘密" in wordlists.phrases("zh-word-2-allow")
  assert not [p for p in wordlists.phrases("zh-tell-1") if p.startswith("#")]
  assert [t for t in wordlists.terms() if t.wrong == "代币"] == [
      wordlists.Term("代币", "令牌", ("token", "OAuth", "JWT", "鉴权", "认证"))
  ]
