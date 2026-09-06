import json
import os
import pathlib
import subprocess
import typing

from cli_runner import validated_rust_artifacts
import pytest

from limae import config, zh_format

REPOSITORY = pathlib.Path(__file__).resolve().parents[1]
FIXTURES = REPOSITORY / "spec" / "fixtures"


class TextResult(typing.NamedTuple):
  findings: list[tuple[int, str, str, str]]
  fixed: str
  refixed: str


class ProbeFinding(typing.TypedDict):
  line: int
  rule: str
  name: str
  range: list[int]
  snippet: str


class ProbeResponse(typing.TypedDict):
  findings: list[ProbeFinding]
  fixed: str
  refixed: str


def _python_result(
    text: str,
    cwd: pathlib.Path,
    disable: list[str] | None = None,
    enable: list[str] | None = None,
) -> TextResult:
  settings = config.resolve(
      disable,
      enable,
      cwd,
      zh_format.ALL_RULES,
      zh_format.DEFAULT_RULES,
      zh_format.EXPERIMENTAL_RULES,
  )
  findings = [
      (finding.line, finding.rule, finding.name, finding.snippet)
      for finding in zh_format.check_text(
          text, settings.rules, settings.skip_zh_units
      )
  ]
  fixed = zh_format.fix_text(text, settings.rules, settings.skip_zh_units)
  refixed = zh_format.fix_text(fixed, settings.rules, settings.skip_zh_units)
  return TextResult(findings, fixed, refixed)


def _decode_probe(
    completed: subprocess.CompletedProcess[str],
) -> ProbeResponse:
  assert completed.returncode == 0, completed.stderr
  assert completed.stderr == ""
  decoded = json.loads(completed.stdout)
  assert isinstance(decoded, dict)
  assert set(decoded) == {"findings", "fixed", "refixed"}
  findings = decoded["findings"]
  assert isinstance(findings, list)
  for finding in findings:
    assert isinstance(finding, dict)
    assert set(finding) == {"line", "rule", "name", "range", "snippet"}
    assert (
        isinstance(finding["range"], list)
        and len(finding["range"]) == 2
        and all(isinstance(offset, int) for offset in finding["range"])
    )
  assert isinstance(decoded["fixed"], str)
  assert isinstance(decoded["refixed"], str)
  return ProbeResponse(
      findings=typing.cast(list[ProbeFinding], typing.cast(object, findings)),
      fixed=decoded["fixed"],
      refixed=decoded["refixed"],
  )


def _rust_result(
    probe: pathlib.Path,
    text: str,
    cwd: pathlib.Path,
    disable: list[str] | None = None,
    enable: list[str] | None = None,
) -> TextResult:
  completed = subprocess.run(  # noqa: S603 - validated Cargo example path
      [probe],
      cwd=cwd,
      env={"PATH": os.environ.get("PATH", os.defpath)},
      input=json.dumps({"text": text, "disable": disable, "enable": enable}),
      check=False,
      capture_output=True,
      text=True,
      timeout=10,
  )
  decoded = _decode_probe(completed)
  findings = [
      (
          finding["line"],
          finding["rule"],
          finding["name"],
          finding["snippet"],
      )
      for finding in decoded["findings"]
  ]
  return TextResult(findings, decoded["fixed"], decoded["refixed"])


def _assert_parity(
    probe: pathlib.Path,
    text: str,
    cwd: pathlib.Path,
    disable: list[str] | None = None,
    enable: list[str] | None = None,
) -> None:
  assert _rust_result(probe, text, cwd, disable, enable) == _python_result(
      text, cwd, disable, enable
  )


def _require_probe(probe: pathlib.Path | None) -> pathlib.Path:
  if probe is None:
    pytest.skip("Rust differential checks require --rust-bin")
  return probe


def test_all_golden_texts_match_the_reference(
    rust_probe: pathlib.Path | None, tmp_path: pathlib.Path
) -> None:
  probe = _require_probe(rust_probe)
  cases = sorted(path.stem for path in FIXTURES.glob("*.in"))
  assert len(cases) == 52
  for case in cases:
    root = tmp_path / case
    root.mkdir()
    configured = FIXTURES / f"{case}.conf"
    if configured.exists():
      (root / "limae.toml").write_bytes(configured.read_bytes())
    _assert_parity(probe, (FIXTURES / f"{case}.in").read_text(), root)


@pytest.mark.parametrize(
    ("text", "configuration", "disable", "enable"),
    [
        ("中A\u2028文B", "", None, None),
        ("前🙂e\u0301中A后", "", None, None),
        ("pİvotal\n", "enable_experimental = true\n", None, None),
        (
            "中`A,B`文 https://example.com/中A 中文[链接](x)\n",
            "",
            None,
            ["zh-typography-9"],
        ),
        (
            "<!-- limae-disable-next-line zh-typography-1 -->\n你好,世界\n",
            'severity = { zh-typography-4 = "warning" }\n',
            None,
            None,
        ),
    ],
)
def test_fixed_seed_rule_interactions_match_the_reference(
    rust_probe: pathlib.Path | None,
    tmp_path: pathlib.Path,
    text: str,
    configuration: str,
    disable: list[str] | None,
    enable: list[str] | None,
) -> None:
  probe = _require_probe(rust_probe)
  if configuration:
    (tmp_path / "limae.toml").write_text(configuration, encoding="utf-8")
  _assert_parity(probe, text, tmp_path, disable, enable)


def test_all_tracked_markdown_matches_the_reference(
    rust_probe: pathlib.Path | None,
) -> None:
  probe = _require_probe(rust_probe)
  completed = subprocess.run(  # noqa: S603 - fixed VCS query
      ["git", "ls-files", "-z", "--", "*.md"],
      cwd=REPOSITORY,
      check=True,
      capture_output=True,
      timeout=10,
  )
  paths = [path for path in completed.stdout.split(b"\0") if path]
  assert paths
  for raw_path in paths:
    path = REPOSITORY / os.fsdecode(raw_path)
    _assert_parity(probe, path.read_text(encoding="utf-8"), REPOSITORY)


def test_probe_failures_are_not_normalized_into_a_match() -> None:
  with pytest.raises(AssertionError, match="controlled failure"):
    _decode_probe(
        subprocess.CompletedProcess(
            ["diff-probe"], 7, stdout="{}", stderr="controlled failure"
        )
    )
  with pytest.raises(json.JSONDecodeError):
    _decode_probe(
        subprocess.CompletedProcess(
            ["diff-probe"], 0, stdout="not json", stderr=""
        )
    )


def test_rust_artifact_validation_requires_the_probe(
    tmp_path: pathlib.Path,
) -> None:
  binary = tmp_path / "limae-rs"
  binary.write_text("placeholder", encoding="utf-8")
  binary.chmod(0o755)
  with pytest.raises(pytest.UsageError, match="diff-probe"):
    validated_rust_artifacts(binary)
