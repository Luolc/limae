use std::error::Error;
use std::fs;
use std::path::Path;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, resolve};
use limae::pipeline::{Typography, TypographyFinding};

type TestResult = Result<(), Box<dyn Error>>;

// Whole cases awaiting their actual pipeline integration, never filtered findings.
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "experimental-english-unicode",
        "A4: experimental English rules and terminology await document integration",
    ),
    ("experimental-en-ai-tells", "A4: experimental English rules"),
    ("experimental-zh-ai-tells", "A4: experimental Chinese rules"),
    (
        "experimental-zh-secret",
        "A4: experimental Chinese word rules",
    ),
    (
        "experimental-zh-zero-noun",
        "A4: experimental Chinese coinages",
    ),
    (
        "inline-disable-config-boundary",
        "A6: inline directive masks",
    ),
    ("inline-disable-next-line", "A6: inline directive masks"),
    ("inline-disable-range", "A6: inline directive masks"),
];

#[test]
fn all_applicable_golden_cases_use_the_document_api() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/fixtures");
    let mut inputs = fs::read_dir(&root)?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;
    inputs.retain(|path| path.extension().is_some_and(|ext| ext == "in"));
    inputs.sort();
    assert!(!inputs.is_empty(), "no golden fixtures discovered");
    let typography = Typography::new()?;
    let temporary = std::env::temp_dir().join(format!("limae-pipeline-{}", std::process::id()));
    fs::create_dir(&temporary)?;
    let mut pending = UNSUPPORTED.to_vec();
    for input in inputs {
        let case = input
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("case name")?;
        if let Some(index) = pending.iter().position(|(name, _)| *name == case) {
            pending.remove(index);
            continue;
        }
        let config_path = input.with_extension("conf");
        let config = if config_path.try_exists()? {
            fs::copy(config_path, temporary.join("limae.toml"))?;
            resolve(&temporary, CliOverrides::default())?
        } else {
            ResolvedConfig::default()
        };
        let original = fs::read_to_string(&input)?;
        let expected = fs::read_to_string(input.with_extension("fixed"))?;
        let findings: String = typography
            .check(&original, &config)
            .iter()
            .map(|f| format!("{} {}\n", f.line, f.rule))
            .collect();
        assert_eq!(
            findings,
            fs::read_to_string(input.with_extension("findings"))?,
            "{case}: original findings"
        );
        assert_eq!(typography.fix(&original, &config), expected, "{case}: fix");
        assert_eq!(
            typography.fix(&expected, &config),
            expected,
            "{case}: idempotence"
        );
    }
    fs::remove_dir_all(temporary)?;
    assert!(
        pending.is_empty(),
        "stale unsupported case names: {pending:?}"
    );
    Ok(())
}

#[test]
fn check_and_fix_have_distinct_python_line_views() -> TestResult {
    let typography = Typography::new()?;
    let config = ResolvedConfig::default();
    for separator in [
        "\n", "\r", "\r\n", "\u{b}", "\u{c}", "\u{1c}", "\u{1d}", "\u{1e}", "\u{85}", "\u{2028}",
        "\u{2029}",
    ] {
        let original = format!("中A{separator}文B{separator}");
        assert_eq!(
            typography.check(&original, &config),
            [
                TypographyFinding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: 3..3,
                    snippet: "中A"
                },
                TypographyFinding {
                    line: 2,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: 3..3,
                    snippet: "文B"
                },
            ],
            "{separator:?}"
        );
        assert_eq!(
            typography.fix(&original, &config),
            format!("中 A{separator}文 B{separator}")
        );
        let fenced = format!("```{separator}中A{separator}~~~{separator}文B");
        let found = typography.check(&fenced, &config);
        assert_eq!(
            found,
            [TypographyFinding {
                line: 4,
                rule: RuleId::ZH_TYPOGRAPHY_4,
                name: "zh-typography-4 no space between CJK and Latin".into(),
                range: 3..3,
                snippet: "文B"
            }]
        );
        let expected = if separator.ends_with('\n') {
            format!("```{separator}中A{separator}~~~{separator}文 B")
        } else {
            fenced.clone()
        };
        assert_eq!(
            typography.fix(&fenced, &config),
            expected,
            "fence: {separator:?}"
        );
    }
    for whitespace in ["\u{1f}", "\t", "\u{a0}"] {
        let original = format!("中A{whitespace}文B");
        let second = "中A".len() + whitespace.len() + "文".len();
        assert_eq!(
            typography.check(&original, &config),
            [
                TypographyFinding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: 3..3,
                    snippet: &original
                },
                TypographyFinding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: second..second,
                    snippet: &original
                },
            ]
        );
        assert_eq!(
            typography.fix(&original, &config),
            format!("中 A{whitespace}文 B")
        );
    }
    for original in ["", "\n", "\r\n", "\n\n", "\r", "\u{2028}", "中", "中\n\n"] {
        assert_eq!(typography.check(original, &config), []);
        assert_eq!(typography.fix(original, &config), original);
    }
    Ok(())
}

#[test]
fn original_findings_keep_order_ranges_and_scalar_windows() -> TestResult {
    let typography = Typography::new()?;
    let config = ResolvedConfig::default();
    let original = "中（１）\n， ， ， ，";
    let expected = [
        (
            1,
            RuleId::ZH_TYPOGRAPHY_2,
            "zh-typography-2 fullwidth paren",
            3..6,
            "中（１）",
        ),
        (
            1,
            RuleId::ZH_TYPOGRAPHY_2,
            "zh-typography-2 fullwidth paren",
            9..12,
            "中（１）",
        ),
        (
            1,
            RuleId::ZH_TYPOGRAPHY_10,
            "zh-typography-10 fullwidth digit",
            6..9,
            "中（１）",
        ),
        (
            2,
            RuleId::ZH_TYPOGRAPHY_11,
            "zh-typography-11 space before fullwidth punct",
            0..7,
            "， ， ， ，",
        ),
        (
            2,
            RuleId::ZH_TYPOGRAPHY_11,
            "zh-typography-11 space after fullwidth punct",
            0..7,
            "， ， ， ，",
        ),
        (
            2,
            RuleId::ZH_TYPOGRAPHY_11,
            "zh-typography-11 space before fullwidth punct",
            8..15,
            "， ， ， ，",
        ),
        (
            2,
            RuleId::ZH_TYPOGRAPHY_11,
            "zh-typography-11 space after fullwidth punct",
            8..15,
            "， ， ， ，",
        ),
    ]
    .map(|(line, rule, name, range, snippet)| TypographyFinding {
        line,
        rule,
        name: name.into(),
        range,
        snippet,
    });
    assert_eq!(typography.check(original, &config), expected);
    let fixed = typography.fix(original, &config);
    assert_eq!(fixed, "中 (1)\n，，，，");
    assert_eq!(typography.check(&fixed, &config), []);
    assert_eq!(typography.check(original, &config), expected);

    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let original = format!("前{left}１{right}后");
    assert_eq!(
        typography.check(&original, &config),
        [TypographyFinding {
            line: 1,
            rule: RuleId::ZH_TYPOGRAPHY_10,
            name: "zh-typography-10 fullwidth digit".into(),
            range: 51..54,
            snippet: &format!("{left}１{right}"),
        }]
    );
    Ok(())
}

#[test]
fn destination_reclassification_requires_another_pass() -> TestResult {
    let typography = Typography::new()?;
    let config = ResolvedConfig::default();
    let original = ")https://example.com/x[x]（A）";
    let first_pass = ")https://example.com/x[x](A)";
    let expected = ") https://example.com/x[x](A)";
    assert_eq!(
        typography
            .check(original, &config)
            .iter()
            .map(|f| f.rule)
            .collect::<Vec<_>>(),
        [RuleId::ZH_TYPOGRAPHY_2; 2]
    );
    assert_eq!(
        typography.check(first_pass, &config),
        [TypographyFinding {
            line: 1,
            rule: RuleId::ZH_TYPOGRAPHY_3,
            name: "zh-typography-3 no space after )".into(),
            range: 0..2,
            snippet: ")https://examp",
        }]
    );
    assert_eq!(typography.fix(original, &config), expected);
    assert_eq!(typography.fix(first_pass, &config), expected);
    assert_eq!(typography.fix(expected, &config), expected);
    assert_eq!(typography.check(expected, &config), []);
    Ok(())
}

#[test]
fn protected_interiors_and_unedited_code_edges_survive_all_stages() -> TestResult {
    let typography = Typography::new()?;
    let config = ResolvedConfig::default();
    let original = "中（１６GB）用``\n``字——文, 字[链](中A， 文)后\n\n中`中A， 文`后\n「かな中A， 文」\nhttps://example.com/x,中文\n```\n中（１６GB）\n~~~";
    let fixed = "中 (16 GB) 用 ``\n`` 字 —— 文，字[链](中A， 文) 后\n\n中 `中A， 文` 后\n「かな中A， 文」\nhttps://example.com/x，中文\n```\n中（１６GB）\n~~~";
    assert_eq!(typography.fix(original, &config), fixed);
    assert_eq!(typography.fix(fixed, &config), fixed);
    assert_eq!(typography.check(fixed, &config), []);
    Ok(())
}

#[test]
fn supplied_configuration_applies_only_to_typography() -> TestResult {
    let temporary =
        std::env::temp_dir().join(format!("limae-pipeline-config-{}", std::process::id()));
    fs::create_dir(&temporary)?;
    fs::write(
        temporary.join("limae.toml"),
        concat!(
            "enable_experimental = true\n",
            "skip_zh_units = '年'\n",
            "disable = ['zh-typography-4']\n",
            "[severity]\nzh-typography-2 = 'warning'\n",
        ),
    )?;
    let config = resolve(&temporary, CliOverrides::default())?;
    fs::remove_dir_all(temporary)?;
    assert!(config.is_enabled(RuleId::ZH_TELL_1));
    let typography = Typography::new()?;
    let original = "深入探讨中（２０１１年）用A， 文";
    assert_eq!(
        typography
            .check(original, &config)
            .iter()
            .map(|f| f.rule)
            .collect::<Vec<_>>(),
        [
            RuleId::ZH_TYPOGRAPHY_2,
            RuleId::ZH_TYPOGRAPHY_2,
            RuleId::ZH_TYPOGRAPHY_10,
            RuleId::ZH_TYPOGRAPHY_10,
            RuleId::ZH_TYPOGRAPHY_10,
            RuleId::ZH_TYPOGRAPHY_10,
            RuleId::ZH_TYPOGRAPHY_11,
        ]
    );
    assert_eq!(
        typography.fix(original, &config),
        "深入探讨中 (2011年) 用A，文"
    );
    assert_eq!(
        typography.fix(original, &ResolvedConfig::default()),
        "深入探讨中 (2011 年) 用 A，文"
    );
    Ok(())
}
