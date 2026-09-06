import os
import pathlib
import sys

from cli_runner import CliRunner, validated_rust_artifacts
import pytest


def _python_binary() -> pathlib.Path:
  return pathlib.Path(sys.executable).with_name("limae")


def _rust_artifacts(
    config: pytest.Config,
) -> tuple[pathlib.Path, pathlib.Path] | None:
  configured = config.getoption("--rust-bin")
  if configured is None:
    return None
  return validated_rust_artifacts(pathlib.Path(configured))


def pytest_sessionstart(session: pytest.Session) -> None:
  """Fail before collection when a requested process arm cannot run."""
  python = _python_binary()
  if not python.is_file() or not os.access(python, os.X_OK):
    raise pytest.UsageError(f"Python console binary is unavailable: {python}")
  _rust_artifacts(session.config)


def pytest_generate_tests(metafunc: pytest.Metafunc) -> None:
  """Run every CLI contract once per explicitly enabled implementation."""
  if "limae_cli" not in metafunc.fixturenames:
    return
  names = ["python"]
  if _rust_artifacts(metafunc.config) is not None:
    names.append("rust")
  metafunc.parametrize("limae_cli", names, indirect=True, ids=names)


@pytest.hookimpl(tryfirst=True)
def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
  """Reject full collections that silently drop a requested CLI arm."""
  if any(
      "::" in argument and argument.endswith("]")
      for argument in config.invocation_params.args
  ):
    return
  expected = {"python"}
  if _rust_artifacts(config) is not None:
    expected.add("rust")
  collected: set[str] = set()
  for item in items:
    callspec = getattr(item, "callspec", None)
    params = getattr(callspec, "params", {})
    arm = params.get("limae_cli")
    if arm is not None:
      collected.add(str(arm))
  if not collected:
    return
  if collected != expected:
    raise pytest.UsageError(
        "CLI subprocess arms collected "
        f"{sorted(collected)}, expected {sorted(expected)}"
    )


@pytest.fixture
def limae_cli(request: pytest.FixtureRequest) -> CliRunner:
  """Return the real console binary selected by dynamic parametrization."""
  name = str(request.param)
  if name == "python":
    return CliRunner(name, _python_binary())
  artifacts = _rust_artifacts(request.config)
  if artifacts is None:
    raise pytest.UsageError("the Rust CLI arm was selected without --rust-bin")
  return CliRunner(name, artifacts[0])


@pytest.fixture(scope="session")
def rust_probe(pytestconfig: pytest.Config) -> pathlib.Path | None:
  """Return the requested probe without finding or building it implicitly."""
  artifacts = _rust_artifacts(pytestconfig)
  return None if artifacts is None else artifacts[1]


@pytest.fixture(autouse=True)
def isolate_limae_environment(monkeypatch: pytest.MonkeyPatch) -> None:
  """Remove inherited limae settings before each test."""
  for name in tuple(os.environ):
    if name.startswith("LIMAE_"):
      monkeypatch.delenv(name)
