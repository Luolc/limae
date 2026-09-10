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


def _hook_files_pattern(hook: str) -> str:
  """Read the `files` pattern one local hook is configured with.

  Args:
    hook: The hook's `id` in .pre-commit-config.yaml.

  Returns:
    The pattern exactly as written in .pre-commit-config.yaml.
  """
  repository = pathlib.Path(__file__).resolve().parents[1]
  config = repository / ".pre-commit-config.yaml"
  block = re.search(
      rf"^\s*- id: {re.escape(hook)}$(.*?)(?=^\s*- id: |\Z)",
      config.read_text(encoding="utf-8"),
      re.MULTILINE | re.DOTALL,
  )
  assert block is not None, f"no {hook} hook in .pre-commit-config.yaml"
  pattern = re.search(r"^\s*files: (.+)$", block.group(1), re.MULTILINE)
  assert pattern is not None, f"{hook} hook has no files pattern"
  return pattern.group(1).strip()


# pre-commit selects a hook by `re.search` of `files` against the
# repository-relative path, so this asserts the same expression the same way
# rather than paying for a real hook run. Gate 11 runs the hook for real.
def test_cargo_fmt_hook_wakes_for_files_that_change_what_it_prints() -> None:
  # `cargo fmt --check` reads more than the files it prints about: Cargo.toml
  # fixes the targets and the edition, a rustfmt.toml would carry the style,
  # and rust-toolchain.toml decides which rustfmt runs. A commit touching one
  # of those reformats the tree with no `.rs` staged.
  pattern = _hook_files_pattern("cargo-fmt")

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
  pattern = _hook_files_pattern("cargo-fmt")

  for path in ("README.md", "docs/tracker.md", "pyproject.toml", "uv.lock"):
    assert not re.search(pattern, path), f"{path} would wake the hook"


# The lexicon hook selects on the path, not on the extension, so both arms
# have to be asserted: `.toml` alone would drag in the build configuration,
# and `zh\.toml$` would leave the English lexicon unchecked when it lands.
# Read from the configuration rather than restated here, for the same reason
# as above; gate 11 runs the hook for real. This assertion lives here rather
# than in `rust/tests/` because the Cargo package does not ship
# .pre-commit-config.yaml, and gate 13 runs the Rust tests out of it.
def test_lexicon_hook_wakes_for_every_lexicon() -> None:
  pattern = _hook_files_pattern("limae-lexicon")

  for path in ("spec/lexicon/zh.toml", "spec/lexicon/en.toml"):
    assert re.search(pattern, path), f"{path} would not wake the hook"


def test_lexicon_hook_stays_off_every_other_toml() -> None:
  # None of these is prose; a `\.toml$` hook would run this repository's
  # Chinese typography rules over its own build configuration.
  pattern = _hook_files_pattern("limae-lexicon")

  for path in (
      "Cargo.toml",
      "rust-toolchain.toml",
      "askama.toml",
      "pyproject.toml",
      "spec/wordlists/zh-word-1.toml",
  ):
    assert not re.search(pattern, path), f"{path} would wake the hook"
