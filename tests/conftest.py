import os

import pytest


@pytest.fixture(autouse=True)
def isolate_limae_environment(monkeypatch: pytest.MonkeyPatch) -> None:
  """Remove inherited limae settings before each test."""
  for name in tuple(os.environ):
    if name.startswith("LIMAE_"):
      monkeypatch.delenv(name)
