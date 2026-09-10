import os
import pathlib
import re
import subprocess
import sys


def test_external_limae_engine_does_not_affect_tests() -> None:
  repository = pathlib.Path(__file__).resolve().parents[1]
  target = (
      repository
      / "tests"
      / "test_polish.py::test_cli_writes_the_polished_text_to_stdout"
  )
  environment = os.environ.copy()
  environment["LIMAE_ENGINE"] = "codex"

  result = subprocess.run(  # noqa: S603
      [sys.executable, "-m", "pytest", "-q", str(target)],
      cwd=repository,
      env=environment,
      check=False,
      capture_output=True,
      text=True,
      timeout=30,
  )

  assert result.returncode == 0, result.stdout + result.stderr


def _cargo_fmt_files_pattern() -> str:
  """Read the `files` pattern the cargo-fmt hook is configured with.

  Returns:
    The pattern exactly as written in .pre-commit-config.yaml.
  """
  repository = pathlib.Path(__file__).resolve().parents[1]
  config = repository / ".pre-commit-config.yaml"
  block = re.search(
      r"^\s*- id: cargo-fmt$(.*?)(?=^\s*- id: |\Z)",
      config.read_text(encoding="utf-8"),
      re.MULTILINE | re.DOTALL,
  )
  assert block is not None, "no cargo-fmt hook in .pre-commit-config.yaml"
  pattern = re.search(r"^\s*files: (.+)$", block.group(1), re.MULTILINE)
  assert pattern is not None, "cargo-fmt hook has no files pattern"
  return pattern.group(1).strip()


# pre-commit selects a hook by `re.search` of `files` against the
# repository-relative path, so this asserts the same expression the same way
# rather than paying for a real hook run. Gate 10 runs the hook for real.
def test_cargo_fmt_hook_wakes_for_files_that_change_what_it_prints() -> None:
  # `cargo fmt --check` reads more than the files it prints about: Cargo.toml
  # fixes the targets and the edition, a rustfmt.toml would carry the style,
  # and rust-toolchain.toml decides which rustfmt runs. A commit touching one
  # of those reformats the tree with no `.rs` staged.
  pattern = _cargo_fmt_files_pattern()

  for path in (
      "rust/main.rs",
      "Cargo.toml",
      "rustfmt.toml",
      ".rustfmt.toml",
      "rust-toolchain.toml",
  ):
    assert re.search(pattern, path), f"{path} would not wake the hook"


def test_cargo_fmt_hook_stays_out_of_commits_it_cannot_speak_to() -> None:
  # The other half: without this arm a pattern matching everything passes the
  # arm above, and every commit in the repository pays for a cargo run.
  pattern = _cargo_fmt_files_pattern()

  for path in ("README.md", "docs/tracker.md", "pyproject.toml", "uv.lock"):
    assert not re.search(pattern, path), f"{path} would wake the hook"
