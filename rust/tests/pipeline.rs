use std::error::Error;
use std::fs;
use std::path::Path;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, Severity, resolve};
use limae::pipeline::{Finding, Pipeline};

type TestResult = Result<(), Box<dyn Error>>;

fn configured(name: &str, contents: &str) -> Result<ResolvedConfig, Box<dyn Error>> {
    configured_with(name, contents, CliOverrides::default())
}

/// The same, for a run whose command line overrides the file it found.
fn configured_with(
    name: &str,
    contents: &str,
    overrides: CliOverrides<'_>,
) -> Result<ResolvedConfig, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("limae-pipeline-{name}-{}", std::process::id()));
    fs::create_dir(&root)?;
    fs::write(root.join("limae.toml"), contents)?;
    let config = resolve(&root, overrides);
    fs::remove_dir_all(root)?;
    Ok(config?)
}

#[test]
fn all_golden_cases_use_the_document_api() -> TestResult {
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
    for input in inputs {
        let case = input
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("case name")?;
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
            .check(&original, &config)?
            .iter()
            .map(|f| format!("{} {}\n", f.line, f.rule))
            .collect();
        assert_eq!(
            findings,
            fs::read_to_string(input.with_extension("findings"))?,
            "{case}: original findings"
        );
        assert_eq!(pipeline.fix(&original, &config)?, expected, "{case}: fix");
        assert_eq!(
            pipeline.fix(&expected, &config)?,
            expected,
            "{case}: idempotence"
        );
    }
    fs::remove_dir_all(temporary)?;
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
            pipeline.check(&original, &config)?,
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
            pipeline.fix(&original, &config)?,
            format!("中 A{separator}文 B{separator}")
        );
        let fenced = format!("```{separator}中A{separator}~~~{separator}文B");
        let found = pipeline.check(&fenced, &config)?;
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
            pipeline.fix(&fenced, &config)?,
            expected,
            "fence: {separator:?}"
        );
    }
    for whitespace in ["\u{1f}", "\t", "\u{a0}"] {
        let original = format!("中A{whitespace}文B");
        let second = "中A".len() + whitespace.len() + "文".len();
        assert_eq!(
            pipeline.check(&original, &config)?,
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
            pipeline.fix(&original, &config)?,
            format!("中 A{whitespace}文 B")
        );
    }
    for original in ["", "\n", "\r\n", "\n\n", "\r", "\u{2028}", "中", "中\n\n"] {
        assert_eq!(pipeline.check(original, &config)?, []);
        assert_eq!(pipeline.fix(original, &config)?, original);
    }
    Ok(())
}

#[test]
fn directives_use_each_pipeline_line_view_and_consume_pending_once() -> TestResult {
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();

    let unit_separator = "<!-- limae-disable\u{1f}zh-typography-4 -->\n你好,世界与中文English混排";
    let found = pipeline.check(unit_separator, &config)?;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].line, found[0].rule), (2, RuleId::ZH_TYPOGRAPHY_1));
    assert_eq!(
        pipeline.fix(unit_separator, &config)?,
        "<!-- limae-disable\u{1f}zh-typography-4 -->\n你好，世界与中文English混排"
    );

    let file_separator = "<!-- limae-disable\u{1c}zh-typography-4 -->\n你好,世界与中文English混排";
    let found = pipeline.check(file_separator, &config)?;
    assert_eq!(found.len(), 3);
    assert!(found.iter().all(|finding| finding.line == 3));
    assert_eq!(
        found.iter().map(|finding| finding.rule).collect::<Vec<_>>(),
        [
            RuleId::ZH_TYPOGRAPHY_1,
            RuleId::ZH_TYPOGRAPHY_4,
            RuleId::ZH_TYPOGRAPHY_4,
        ]
    );
    assert_eq!(
        pipeline.fix(file_separator, &config)?,
        "<!-- limae-disable\u{1c}zh-typography-4 -->\n你好，世界与中文English混排"
    );

    let blank_consumes = "<!-- limae-disable-next-line zh-typography-4 -->\n\n中A";
    let found = pipeline.check(blank_consumes, &config)?;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].line, found[0].rule), (3, RuleId::ZH_TYPOGRAPHY_4));
    assert_eq!(
        pipeline.fix(blank_consumes, &config)?,
        "<!-- limae-disable-next-line zh-typography-4 -->\n\n中 A"
    );

    let fenced = concat!(
        "<!-- limae-disable-next-line zh-typography-4 -->\n",
        "```\n",
        "<!-- limae-disable unknown -->\n",
        "```\n",
        "中A",
    );
    let found = pipeline.check(fenced, &config)?;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].line, found[0].rule), (5, RuleId::ZH_TYPOGRAPHY_4));
    assert_eq!(
        pipeline.fix(fenced, &config)?,
        concat!(
            "<!-- limae-disable-next-line zh-typography-4 -->\n",
            "```\n",
            "<!-- limae-disable unknown -->\n",
            "```\n",
            "中 A",
        )
    );
    Ok(())
}

#[test]
fn directives_cannot_enable_rules_outside_configuration() -> TestResult {
    let config = configured(
        "directive-config-boundary",
        concat!(
            "enable_experimental = true\n",
            "disable = ['zh-typography-1']\n",
            "skip_zh_units = '年'\n",
            "[severity]\nzh-tell-1 = 'error'\n",
        ),
    )?;
    let pipeline = Pipeline::new()?;
    let original = concat!(
        "<!-- limae-disable -->\n",
        "综上所述你好,世界2011年 中[链](x)\n",
        "<!-- limae-enable -->\n",
        "综上所述你好,世界2011年 中[链](x)",
    );
    let found = pipeline.check(original, &config)?;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].line, found[0].rule), (4, RuleId::ZH_TELL_1));
    assert_eq!(config.severity(found[0].rule), Severity::Error);
    assert_eq!(pipeline.fix(original, &config)?, original);
    Ok(())
}

#[test]
fn directive_errors_propagate_from_check_and_fix_with_line_numbers() -> TestResult {
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    let original = "plain\n<!-- limae-disable fake, zh-typography-4 other -->\n中A";
    let check_error = match pipeline.check(original, &config) {
        Err(error) => error,
        Ok(_) => return Err("check must reject an unknown directive".into()),
    };
    let fix_error = match pipeline.fix(original, &config) {
        Err(error) => error,
        Ok(_) => return Err("fix must reject an unknown directive".into()),
    };
    for error in [check_error, fix_error] {
        assert_eq!(error.line(), 2);
        assert_eq!(error.unknown_ids(), ["fake", "other"]);
        assert!(error.to_string().contains("unknown rule id(s) fake, other"));
    }

    let similar = concat!(
        "<!-- limae-disabled -->\n",
        "prefix <!-- limae-disable -->\n",
        "中A",
    );
    let found = pipeline.check(similar, &config)?;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].line, found[0].rule), (3, RuleId::ZH_TYPOGRAPHY_4));
    assert_eq!(
        pipeline.fix(similar, &config)?,
        "<!-- limae-disabled -->\nprefix <!-- limae-disable -->\n中 A"
    );
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
    assert_eq!(pipeline.check(original, &config)?, expected);
    let fixed = pipeline.fix(original, &config)?;
    assert_eq!(fixed, "中 (1)\n，，，，");
    assert_eq!(pipeline.check(&fixed, &config)?, []);
    assert_eq!(pipeline.check(original, &config)?, expected);

    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let original = format!("前{left}１{right}后");
    assert_eq!(
        pipeline.check(&original, &config)?,
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
            .check(original, &config)?
            .iter()
            .map(|f| f.rule)
            .collect::<Vec<_>>(),
        [RuleId::ZH_TYPOGRAPHY_2; 2]
    );
    assert_eq!(
        pipeline.check(first_pass, &config)?,
        [Finding {
            line: 1,
            rule: RuleId::ZH_TYPOGRAPHY_3,
            name: "zh-typography-3 no space after )".into(),
            range: 0..2,
            snippet: ")https://examp",
        }]
    );
    assert_eq!(pipeline.fix(original, &config)?, expected);
    assert_eq!(pipeline.fix(first_pass, &config)?, expected);
    assert_eq!(pipeline.fix(expected, &config)?, expected);
    assert_eq!(pipeline.check(expected, &config)?, []);
    Ok(())
}

#[test]
fn protected_interiors_and_unedited_code_edges_survive_all_stages() -> TestResult {
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    let original = "中（１６GB）用``\n``字——文, 字[链](中A， 文)后\n\n中`中A， 文`后\n「かな中A， 文」\nhttps://example.com/x,中文\n```\n中（１６GB）\n~~~";
    let fixed = "中 (16 GB) 用 ``\n`` 字 —— 文，字[链](中A， 文) 后\n\n中 `中A， 文` 后\n「かな中A， 文」\nhttps://example.com/x，中文\n```\n中（１６GB）\n~~~";
    assert_eq!(pipeline.fix(original, &config)?, fixed);
    assert_eq!(pipeline.fix(fixed, &config)?, fixed);
    assert_eq!(pipeline.check(fixed, &config)?, []);
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
            .check(original, &config)?
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
        pipeline.fix(original, &config)?,
        "综上所述中 (2011年) 用A，文"
    );
    assert_eq!(
        pipeline.fix(original, &ResolvedConfig::default())?,
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
    let found = Pipeline::new()?.check(original, &config)?;
    assert_eq!(found, expected);
    assert_eq!(pipeline.check(original, &ResolvedConfig::default())?, []);
    assert_eq!(
        pipeline.fix(original, &ResolvedConfig::default())?,
        original
    );
    let fixed = pipeline.fix(original, &config)?;
    assert_eq!(
        fixed,
        "综上所述\n不是甲而是乙\n赋能 赋能\n希望对你有帮助\npİvotal\nIt's not just X, but it's Y\nload-bearing\n零重复\n密钥 秘密 `token`"
    );
    let mut remaining = expected[..9].to_vec();
    remaining.push(Finding {
        snippet: "密钥 秘密 `token`",
        ..expected[10].clone()
    });
    assert_eq!(pipeline.check(&fixed, &config)?, remaining);
    assert_eq!(pipeline.fix(&fixed, &config)?, fixed);
    assert_eq!(pipeline.check(original, &config)?, expected);
    let mixed = "不是甲而是乙 pİvotal";
    assert_eq!(
        pipeline.check(mixed, &config)?,
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
        assert_eq!(pipeline.fix(original, &config)?, expected);
        assert_eq!(pipeline.fix(expected, &config)?, expected);
    }
    // Check uses two lines here, while fix sees one line with anchor evidence.
    assert_eq!(pipeline.check("秘钥\r`token`", &config)?, []);
    let disabled = configured(
        "terms-disabled",
        "enable_experimental = true\ndisable = ['zh-word-1', 'en-tell-1']",
    )?;
    let original = "秘钥（１６GB）`token` pİvotal";
    assert_eq!(
        pipeline.fix(original, &disabled)?,
        "秘钥 (16 GB) `token` pİvotal"
    );
    assert_eq!(
        pipeline
            .check(original, &disabled)?
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

/// The five inputs that were hand-picked because a rule interaction went wrong
/// on them once, pinned to what this implementation answers today.
///
/// Each one is an interaction rather than a rule: a line separator that is not
/// `\n`, a grapheme cluster the byte ranges have to step around, an
/// experimental rule reached through a dotted capital, three protected spans
/// competing on one line, and a directive under a configured severity. The
/// golden fixtures cover the rules; nothing covered these five together.
#[test]
fn hand_picked_rule_interactions_keep_their_findings_and_fixes() -> TestResult {
    let pipeline = Pipeline::new()?;
    let default = ResolvedConfig::default();

    // U+2028 is a line break to the checker and an ordinary character to the
    // fixer, so one input has two line numbers and one unbroken fixed string.
    let original = "中A\u{2028}文B";
    let expected = [(1, "中A"), (2, "文B")].map(|(line, snippet)| Finding {
        line,
        rule: RuleId::ZH_TYPOGRAPHY_4,
        name: "zh-typography-4 no space between CJK and Latin".into(),
        range: 3..3,
        snippet,
    });
    assert_eq!(pipeline.check(original, &default)?, expected);
    assert_eq!(pipeline.fix(original, &default)?, "中 A\u{2028}文 B");

    // An emoji and a combining mark before the match: the two insertion points
    // are byte offsets into a line no scalar index would land on.
    let original = "前🙂e\u{301}中A后";
    let expected = [13, 14].map(|start| Finding {
        line: 1,
        rule: RuleId::ZH_TYPOGRAPHY_4,
        name: "zh-typography-4 no space between CJK and Latin".into(),
        range: start..start,
        snippet: original,
    });
    assert_eq!(pipeline.check(original, &default)?, expected);
    assert_eq!(pipeline.fix(original, &default)?, "前🙂e\u{301}中 A 后");

    // `İ` lowercases to two scalars, so a word rule that folded case naively
    // would either miss this or report a range that is not a boundary.
    let experimental = configured("seed-experimental", "enable_experimental = true\n")?;
    let original = "pİvotal\n";
    assert_eq!(
        pipeline.check(original, &experimental)?,
        [Finding {
            line: 1,
            rule: RuleId::EN_TELL_1,
            name: "en-tell-1 English AI vocabulary".into(),
            range: 0..8,
            snippet: "pİvotal",
        }]
    );
    assert_eq!(pipeline.fix(original, &experimental)?, original);

    // Inline code, a bare URL and a link on one line, with the link rule turned
    // on from the command line: three protected spans and three rules that all
    // want to insert a space near their edges.
    let enable = ["zh-typography-9".to_owned()];
    let linked = configured_with(
        "seed-linked",
        "",
        CliOverrides {
            disable: None,
            enable: Some(&enable),
        },
    )?;
    let original = "中`A,B`文 https://example.com/中A 中文[链接](x)\n";
    let expected = [
        (
            RuleId::ZH_TYPOGRAPHY_4,
            "zh-typography-4 no space between CJK and Latin",
            35..35,
            "xample.com/中A 中文[链接](x)",
        ),
        (
            RuleId::ZH_TYPOGRAPHY_7,
            "zh-typography-7 no space before inline code",
            3..4,
            "中`A,B`文 https:",
        ),
        (
            RuleId::ZH_TYPOGRAPHY_7,
            "zh-typography-7 no space after inline code",
            7..8,
            "中`A,B`文 https://ex",
        ),
        (
            RuleId::ZH_TYPOGRAPHY_9,
            "zh-typography-9 no space between CJK and link",
            43..52,
            "le.com/中A 中文[链接](x)",
        ),
    ]
    .map(|(rule, name, range, snippet)| Finding {
        line: 1,
        rule,
        name: name.into(),
        range,
        snippet,
    });
    assert_eq!(pipeline.check(original, &linked)?, expected);
    let fixed = "中 `A,B` 文 https://example.com/中 A 中文 [链接](x)\n";
    assert_eq!(pipeline.fix(original, &linked)?, fixed);
    assert_eq!(pipeline.fix(fixed, &linked)?, fixed);

    // A directive silences the rule the line would otherwise trip, while an
    // unrelated rule stays configured as a warning: the two settings are read
    // from different places and neither may swallow the other.
    let downgraded = configured(
        "seed-directive",
        "severity = { zh-typography-4 = \"warning\" }\n",
    )?;
    let original = "<!-- limae-disable-next-line zh-typography-1 -->\n你好,世界\n";
    assert_eq!(pipeline.check(original, &downgraded)?, []);
    assert_eq!(pipeline.fix(original, &downgraded)?, original);
    assert_eq!(
        downgraded.severity(RuleId::ZH_TYPOGRAPHY_4),
        Severity::Warning
    );
    // Control arm: without the directive the same line is a finding, so the
    // empty result above is the directive and not an inert configuration.
    assert_eq!(
        pipeline
            .check("你好,世界\n", &downgraded)?
            .iter()
            .map(|finding| finding.rule)
            .collect::<Vec<_>>(),
        [RuleId::ZH_TYPOGRAPHY_1]
    );
    Ok(())
}
