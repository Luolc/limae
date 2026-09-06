from dataclasses import dataclass
import os
import pathlib
import subprocess

import pytest


@dataclass(frozen=True, slots=True)
class CliRunner:
  """One installed CLI entry point exercised in an isolated subprocess."""

  name: str
  binary: pathlib.Path

  def run(
      self,
      args: list[str],
      cwd: pathlib.Path,
  ) -> subprocess.CompletedProcess[str]:
    env = {
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_NOSYSTEM": "1",
        "PATH": os.environ.get("PATH", os.defpath),
    }
    return subprocess.run(  # noqa: S603 - paths are validated test artifacts
        [self.binary, *args],
        cwd=cwd,
        env=env,
        check=False,
        capture_output=True,
        text=True,
        timeout=10,
    )


def validated_rust_artifacts(
    configured: pathlib.Path,
) -> tuple[pathlib.Path, pathlib.Path]:
  """Require the selected CLI and its same-profile differential probe."""
  binary = configured.resolve()
  probe = binary.parent / "examples" / "diff-probe"
  missing = [path for path in (binary, probe) if not path.is_file()]
  if missing:
    paths = ", ".join(str(path) for path in missing)
    raise pytest.UsageError(f"--rust-bin requires built executable(s): {paths}")
  non_executable = [
      path for path in (binary, probe) if not os.access(path, os.X_OK)
  ]
  if non_executable:
    paths = ", ".join(str(path) for path in non_executable)
    raise pytest.UsageError(f"--rust-bin artifact is not executable: {paths}")
  return binary, probe
