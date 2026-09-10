import os
import pathlib
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


def _cargo_fmt_hook_ran_for(path: str) -> bool:
  """Report whether pre-commit selected the cargo-fmt hook for one path.

  Args:
    path: Repository-relative path to offer to pre-commit.

  Returns:
    True when pre-commit ran the hook, False when it skipped it for having
    no files to check.
  """
  repository = pathlib.Path(__file__).resolve().parents[1]
  result = subprocess.run(  # noqa: S603
      ["uv", "run", "pre-commit", "run", "cargo-fmt", "--files", path],
      cwd=repository,
      check=False,
      capture_output=True,
      text=True,
      timeout=300,
  )
  output = result.stdout + result.stderr
  if "(no files to check)" in output:
    return False
  assert "rust format (cargo fmt)" in output, output
  return True


def test_cargo_fmt_hook_wakes_for_files_that_change_what_it_prints() -> None:
  # Cargo.toml fixes the targets and the edition, so it changes rustfmt's
  # output without any .rs file being staged. A hook keyed on `\.rs$` alone
  # is skipped here, which is the hole this asserts is closed.
  assert _cargo_fmt_hook_ran_for("Cargo.toml")
  assert _cargo_fmt_hook_ran_for("rust-toolchain.toml")


def test_cargo_fmt_hook_stays_out_of_commits_it_cannot_speak_to() -> None:
  # The other half: without this arm, a `files` pattern matching everything
  # would pass the arm above and cost every commit a cargo invocation.
  assert not _cargo_fmt_hook_ran_for("README.md")
