"""Repository-root pytest command-line option registration."""

import pathlib

import pytest


def pytest_addoption(parser: pytest.Parser) -> None:
  """Add the opt-in Rust parity gate before pytest parses its arguments."""
  parser.addoption(
      "--rust-bin",
      type=pathlib.Path,
      help=(
          "run CLI and text parity against this Rust limae binary; use "
          "--rust-bin=/absolute/path when the binary is outside the repository"
      ),
  )
