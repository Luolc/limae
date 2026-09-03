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
