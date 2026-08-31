import pathlib
import tomllib

import pytest

from lo_md_lint.zh_format import check_text, DEFAULT_RULES, fix_text

FIXTURES = pathlib.Path(__file__).resolve().parents[1] / "spec" / "fixtures"
CASES = sorted(p.stem for p in FIXTURES.glob("*.in"))


def read(case: str, suffix: str) -> str:
  return (FIXTURES / (case + suffix)).read_text(encoding="utf-8")


def rules(case: str) -> frozenset[str]:
  conf = FIXTURES / (case + ".conf")
  if not conf.exists():
    return DEFAULT_RULES
  table = tomllib.loads(read(case, ".conf"))
  return (DEFAULT_RULES | frozenset(table.get("enable", []))) - frozenset(
      table.get("disable", [])
  )


def test_fixtures_are_discovered():
  assert CASES, f"no golden fixtures found under {FIXTURES}"


@pytest.mark.parametrize("case", CASES, ids=CASES)
def test_fixture(case: str):
  src = read(case, ".in")
  fixed = read(case, ".fixed")
  enabled = rules(case)
  expected = [
      (int(lineno), rule)
      for lineno, rule in (
          line.split() for line in read(case, ".findings").splitlines()
      )
  ]
  assert fix_text(src, enabled) == fixed
  assert fix_text(fixed, enabled) == fixed, "fix is not idempotent"
  assert [(f.line, f.rule) for f in check_text(src, enabled)] == expected
