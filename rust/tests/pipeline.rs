use std::error::Error;
use std::fs;
use std::path::Path;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, Severity, resolve};
use limae::pipeline::{Finding, Pipeline};

type TestResult = Result<(), Box<dyn Error>>;

fn configured(name: &str, contents: &str) -> Result<ResolvedConfig, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("limae-pipeline-{name}-{}", std::process::id()));
    fs::create_dir(&root)?;
    fs::write(root.join("limae.toml"), contents)?;
    let config = resolve(&root, CliOverrides::default());
    fs::remove_dir_all(root)?;
    Ok(config?)
}

// Whole cases awaiting their actual pipeline integration, never filtered findings.
const UNSUPPORTED: &[(&str, &str)] = &[
    (
        "experimental-en-ai-tells",
        "A6: active disable-next-line en-tell-1",
    ),
    (
        "experimental-zh-ai-tells",
        "A6: active disable-next-line zh-tell-1",
    ),
    (
        "experimental-zh-secret",
        "A6: active disable-next-line zh-word-2",
    ),
    (
        "experimental-zh-zero-noun",
        "A6: active disable-next-line zh-tell-5",
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
    let pipeline = Pipeline::new()?;
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
        let findings: String = pipeline
            .check(&original, &config)
            .iter()
            .map(|f| format!("{} {}\n", f.line, f.rule))
            .collect();
        assert_eq!(
            findings,
            fs::read_to_string(input.with_extension("findings"))?,
            "{case}: original findings"
        );
        assert_eq!(pipeline.fix(&original, &config), expected, "{case}: fix");
        assert_eq!(
            pipeline.fix(&expected, &config),
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
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    for separator in [
        "\n", "\r", "\r\n", "\u{b}", "\u{c}", "\u{1c}", "\u{1d}", "\u{1e}", "\u{85}", "\u{2028}",
        "\u{2029}",
    ] {
        let original = format!("中A{separator}文B{separator}");
        assert_eq!(
            pipeline.check(&original, &config),
            [
                Finding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: 3..3,
                    snippet: "中A"
                },
                Finding {
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
            pipeline.fix(&original, &config),
            format!("中 A{separator}文 B{separator}")
        );
        let fenced = format!("```{separator}中A{separator}~~~{separator}文B");
        let found = pipeline.check(&fenced, &config);
        assert_eq!(
            found,
            [Finding {
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
            pipeline.fix(&fenced, &config),
            expected,
            "fence: {separator:?}"
        );
    }
    for whitespace in ["\u{1f}", "\t", "\u{a0}"] {
        let original = format!("中A{whitespace}文B");
        let second = "中A".len() + whitespace.len() + "文".len();
        assert_eq!(
            pipeline.check(&original, &config),
            [
                Finding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: 3..3,
                    snippet: &original
                },
                Finding {
                    line: 1,
                    rule: RuleId::ZH_TYPOGRAPHY_4,
                    name: "zh-typography-4 no space between CJK and Latin".into(),
                    range: second..second,
                    snippet: &original
                },
            ]
        );
        assert_eq!(
            pipeline.fix(&original, &config),
            format!("中 A{whitespace}文 B")
        );
    }
    for original in ["", "\n", "\r\n", "\n\n", "\r", "\u{2028}", "中", "中\n\n"] {
        assert_eq!(pipeline.check(original, &config), []);
        assert_eq!(pipeline.fix(original, &config), original);
    }
    Ok(())
}

#[test]
fn original_findings_keep_order_ranges_and_scalar_windows() -> TestResult {
    let pipeline = Pipeline::new()?;
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
    .map(|(line, rule, name, range, snippet)| Finding {
        line,
        rule,
        name: name.into(),
        range,
        snippet,
    });
    assert_eq!(pipeline.check(original, &config), expected);
    let fixed = pipeline.fix(original, &config);
    assert_eq!(fixed, "中 (1)\n，，，，");
    assert_eq!(pipeline.check(&fixed, &config), []);
    assert_eq!(pipeline.check(original, &config), expected);

    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let original = format!("前{left}１{right}后");
    assert_eq!(
        pipeline.check(&original, &config),
        [Finding {
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
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    let original = ")https://example.com/x[x]（A）";
    let first_pass = ")https://example.com/x[x](A)";
    let expected = ") https://example.com/x[x](A)";
    assert_eq!(
        pipeline
            .check(original, &config)
            .iter()
            .map(|f| f.rule)
            .collect::<Vec<_>>(),
        [RuleId::ZH_TYPOGRAPHY_2; 2]
    );
    assert_eq!(
        pipeline.check(first_pass, &config),
        [Finding {
            line: 1,
            rule: RuleId::ZH_TYPOGRAPHY_3,
            name: "zh-typography-3 no space after )".into(),
            range: 0..2,
            snippet: ")https://examp",
        }]
    );
    assert_eq!(pipeline.fix(original, &config), expected);
    assert_eq!(pipeline.fix(first_pass, &config), expected);
    assert_eq!(pipeline.fix(expected, &config), expected);
    assert_eq!(pipeline.check(expected, &config), []);
    Ok(())
}

#[test]
fn protected_interiors_and_unedited_code_edges_survive_all_stages() -> TestResult {
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    let original = "中（１６GB）用``\n``字——文, 字[链](中A， 文)后\n\n中`中A， 文`后\n「かな中A， 文」\nhttps://example.com/x,中文\n```\n中（１６GB）\n~~~";
    let fixed = "中 (16 GB) 用 ``\n`` 字 —— 文，字[链](中A， 文) 后\n\n中 `中A， 文` 后\n「かな中A， 文」\nhttps://example.com/x，中文\n```\n中（１６GB）\n~~~";
    assert_eq!(pipeline.fix(original, &config), fixed);
    assert_eq!(pipeline.fix(fixed, &config), fixed);
    assert_eq!(pipeline.check(fixed, &config), []);
    Ok(())
}

#[test]
fn supplied_configuration_bounds_all_rules_and_keeps_configured_severity() -> TestResult {
    let config = configured(
        "config",
        concat!(
            "enable_experimental = true\n",
            "skip_zh_units = '年'\n",
            "disable = ['zh-typography-4']\n",
            "[severity]\nzh-typography-2 = 'warning'\n",
        ),
    )?;
    assert!(config.is_enabled(RuleId::ZH_TELL_1));
    assert_eq!(config.severity(RuleId::ZH_TYPOGRAPHY_2), Severity::Warning);
    assert_eq!(config.severity(RuleId::ZH_TELL_1), Severity::Warning);
    let pipeline = Pipeline::new()?;
    let original = "综上所述中（２０１１年）用A， 文";
    assert_eq!(
        pipeline
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
            RuleId::ZH_TELL_1,
        ]
    );
    assert_eq!(
        pipeline.fix(original, &config),
        "综上所述中 (2011年) 用A，文"
    );
    assert_eq!(
        pipeline.fix(original, &ResolvedConfig::default()),
        "综上所述中 (2011 年) 用 A，文"
    );
    Ok(())
}

#[test]
fn experimental_families_keep_complete_original_and_fixed_findings() -> TestResult {
    let config = configured("experimental", "enable_experimental = true")?;
    let original = "综上所述\n不是甲而是乙\n赋能 赋能\n希望对你有帮助\npİvotal\nIt's not just X, but it's Y\nload-bearing\n零重复\n秘钥 秘密 `token`";
    let expected = [
        (
            1,
            RuleId::ZH_TELL_1,
            "zh-tell-1 formulaic phrase",
            0..12,
            "综上所述",
        ),
        (
            2,
            RuleId::ZH_TELL_2,
            "zh-tell-2 negative parallelism",
            0..15,
            "不是甲而是乙",
        ),
        (
            3,
            RuleId::ZH_TELL_3,
            "zh-tell-3 corporate buzzword",
            0..6,
            "赋能 赋能",
        ),
        (
            4,
            RuleId::ZH_TELL_4,
            "zh-tell-4 chat residue",
            0..21,
            "希望对你有帮助",
        ),
        (
            5,
            RuleId::EN_TELL_1,
            "en-tell-1 English AI vocabulary",
            0..8,
            "pİvotal",
        ),
        (
            6,
            RuleId::EN_TELL_2,
            "en-tell-2 English negative parallelism",
            0..25,
            "It's not just X, but it's Y",
        ),
        (
            6,
            RuleId::EN_TELL_2,
            "en-tell-2 English negative parallelism",
            5..20,
            "It's not just X, but it's Y",
        ),
        (
            7,
            RuleId::EN_TELL_3,
            "en-tell-3 Claudish register",
            0..12,
            "load-bearing",
        ),
        (
            8,
            RuleId::ZH_TELL_5,
            "zh-tell-5 zero-noun coinage",
            0..3,
            "零重复",
        ),
        (
            9,
            RuleId::ZH_WORD_1,
            "zh-word-1 term 秘钥 -> 密钥",
            0..6,
            "秘钥 秘密 `token`",
        ),
        (
            9,
            RuleId::ZH_WORD_2,
            "zh-word-2 misused 秘密",
            7..13,
            "秘钥 秘密 `token`",
        ),
    ]
    .map(|(line, rule, name, range, snippet)| Finding {
        line,
        rule,
        name: name.into(),
        range,
        snippet,
    });
    let pipeline = Pipeline::new()?;
    // The finding owns dynamic names even when its checker has been dropped.
    let found = Pipeline::new()?.check(original, &config);
    assert_eq!(found, expected);
    assert_eq!(pipeline.check(original, &ResolvedConfig::default()), []);
    assert_eq!(pipeline.fix(original, &ResolvedConfig::default()), original);
    let fixed = pipeline.fix(original, &config);
    assert_eq!(
        fixed,
        "综上所述\n不是甲而是乙\n赋能 赋能\n希望对你有帮助\npİvotal\nIt's not just X, but it's Y\nload-bearing\n零重复\n密钥 秘密 `token`"
    );
    let mut remaining = expected[..9].to_vec();
    remaining.push(Finding {
        snippet: "密钥 秘密 `token`",
        ..expected[10].clone()
    });
    assert_eq!(pipeline.check(&fixed, &config), remaining);
    assert_eq!(pipeline.fix(&fixed, &config), fixed);
    assert_eq!(pipeline.check(original, &config), expected);
    let mixed = "不是甲而是乙 pİvotal";
    assert_eq!(
        pipeline.check(mixed, &config),
        [
            Finding {
                line: 1,
                rule: RuleId::ZH_TELL_2,
                name: "zh-tell-2 negative parallelism".into(),
                range: 0..15,
                snippet: mixed,
            },
            Finding {
                line: 1,
                rule: RuleId::EN_TELL_1,
                name: "en-tell-1 English AI vocabulary".into(),
                range: 19..27,
                snippet: mixed,
            },
        ]
    );
    Ok(())
}

#[test]
fn terms_use_whole_fix_lines_and_protected_anchors_through_typography() -> TestResult {
    let config = configured("terms", "enable_experimental = true")?;
    let pipeline = Pipeline::new()?;
    for (original, expected) in [
        (
            "秘钥（１６GB）代币`token`快取 [链](CACHE)",
            "密钥 (16 GB) 令牌 `token` 缓存 [链](CACHE)",
        ),
        (
            "秘钥 `KEY`\n秘钥 `ſecret`\n秘钥 `credentİal`\n秘钥 `credentıal`",
            "密钥 `KEY`\n秘钥 `ſecret`\n秘钥 `credentİal`\n秘钥 `credentıal`",
        ),
        (
            "快取 https://example.com/CACHE",
            "缓存 https://example.com/CACHE",
        ),
        ("`秘钥 token` 秘钥", "`秘钥 token` 密钥"),
        ("秘钥\r`token`", "密钥\r`token`"),
    ] {
        assert_eq!(pipeline.fix(original, &config), expected);
        assert_eq!(pipeline.fix(expected, &config), expected);
    }
    // Check uses two lines here, while fix sees one line with anchor evidence.
    assert_eq!(pipeline.check("秘钥\r`token`", &config), []);
    let disabled = configured(
        "terms-disabled",
        "enable_experimental = true\ndisable = ['zh-word-1', 'en-tell-1']",
    )?;
    let original = "秘钥（１６GB）`token` pİvotal";
    assert_eq!(
        pipeline.fix(original, &disabled),
        "秘钥 (16 GB) `token` pİvotal"
    );
    assert_eq!(
        pipeline
            .check(original, &disabled)
            .iter()
            .map(|f| f.rule)
            .collect::<Vec<_>>(),
        [
            RuleId::ZH_TYPOGRAPHY_2,
            RuleId::ZH_TYPOGRAPHY_2,
            RuleId::ZH_TYPOGRAPHY_10,
            RuleId::ZH_TYPOGRAPHY_10
        ],
    );
    Ok(())
}
