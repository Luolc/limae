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
