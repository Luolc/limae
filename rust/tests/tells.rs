use std::error::Error;
use std::fs;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, Severity, resolve};
use limae::markdown::{LineProtection, Markdown};
use limae::rules::tells::WordlistTells;
use limae::text::snippet;

type TestResult = Result<(), Box<dyn Error>>;

fn configured(name: &str, content: &str) -> Result<ResolvedConfig, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("limae-tells-{name}-{}", std::process::id()));
    fs::create_dir(&root)?;
    fs::write(root.join("limae.toml"), content)?;
    let config = resolve(&root, CliOverrides::default());
    fs::remove_dir_all(root)?;
    Ok(config?)
}

#[test]
fn wordlist_rules_report_once_in_rule_order_after_protected_occurrences() -> TestResult {
    let rules = WordlistTells::new()?;
    let config = configured("rules", "enable_experimental = true")?;
    let lines = [
        "`综上所述` 综上所述总的来说 赋能抓手 随时告诉我需要我帮你 TAPESTRY pivotal load-bearing load-bearing",
        "```",
        "综上所述 pivotal",
        "~~~",
        "[x](pivotal) pivotal",
        "「かな综上所述」总的来说",
        "https://example.com/pivotal pivotal",
    ];
    let protected = Markdown::new()?.protect(&lines);
    let actual: Vec<_> = rules
        .check_line(lines[0], &protected[0], &config)
        .into_iter()
        .map(|m| (m.rule, m.name, &lines[0][m.range]))
        .collect();
    assert_eq!(
        actual,
        [
            (RuleId::ZH_TELL_1, "zh-tell-1 formulaic phrase", "综上所述"),
            (RuleId::ZH_TELL_3, "zh-tell-3 corporate buzzword", "赋能"),
            (RuleId::ZH_TELL_4, "zh-tell-4 chat residue", "随时告诉我"),
            (
                RuleId::EN_TELL_1,
                "en-tell-1 English AI vocabulary",
                "TAPESTRY"
            ),
            (
                RuleId::EN_TELL_3,
                "en-tell-3 Claudish register",
                "load-bearing"
            ),
        ]
    );
    for (i, line) in lines.iter().enumerate().skip(1) {
        let found = rules.check_line(line, &protected[i], &config);
        if i < 4 {
            assert!(found.is_empty());
        } else {
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].range.end, line.len());
        }
    }
    assert!(
        rules
            .check_line(lines[0], &protected[0], &ResolvedConfig::default())
            .is_empty()
    );
    for (id, _, _) in actual {
        assert_eq!(config.severity(id), Severity::Warning);
    }
    let disabled = configured(
        "disabled",
        "enable_experimental = true\ndisable = ['zh-tell-3', 'en-tell-1']",
    )?;
    assert_eq!(
        rules
            .check_line(lines[0], &protected[0], &disabled)
            .iter()
            .map(|m| m.rule)
            .collect::<Vec<_>>(),
        [RuleId::ZH_TELL_1, RuleId::ZH_TELL_4, RuleId::EN_TELL_3]
    );
    Ok(())
}

#[test]
fn english_case_and_boundaries_follow_python_ignorecase() -> TestResult {
    let rules = WordlistTells::new()?;
    let config = configured("english", "enable_experimental = true")?;
    let markdown = Markdown::new()?;
    for word in [
        "pivotal",
        "PIVOTAL",
        "pİvotal",
        "pıvotal",
        "tapeſtry",
        "load-bearİng",
    ] {
        for adjacent in [
            "", "-", "中", "é", "７", "\u{301}", "_", "a", "9", "ſ", "K", "İ", "ı",
        ] {
            for line in [format!("{adjacent}{word}"), format!("{word}{adjacent}")] {
                let protected = markdown.protect(&[&line]);
                let found = rules.check_line(&line, &protected[0], &config);
                let allowed = ["", "-", "中", "é", "７", "\u{301}"].contains(&adjacent);
                assert_eq!(found.len(), usize::from(allowed), "{line}");
                if allowed {
                    assert_eq!(&line[found[0].range.clone()], word);
                }
            }
        }
    }
    for (line, expected) in [
        ("xpivotal pivotal", "pivotal"),
        ("pivotalx tapestry", "tapestry"),
        ("`pivotal`-tapestry", "tapestry"),
        ("non-load-bearing", "load-bearing"),
        ("diverse\tarray", ""),
        ("diverse array", "diverse array"),
    ] {
        let protected = markdown.protect(&[line]);
        let found = rules.check_line(line, &protected[0], &config);
        assert_eq!(
            found.first().map_or("", |m| &line[m.range.clone()]),
            expected
        );
    }
    let line = "pivotal-tapestry";
    for (span, expected) in [(0..7, "tapestry"), (7..8, "pivotal")] {
        let protection = LineProtection::Inline {
            code: vec![span],
            prose: vec![],
        };
        let found = rules.check_line(line, &protection, &config);
        assert_eq!(&line[found[0].range.clone()], expected);
    }
    Ok(())
}

#[test]
fn wordlist_findings_keep_byte_ranges_and_scalar_snippets() -> TestResult {
    let rules = WordlistTells::new()?;
    let config = configured("snippet", "enable_experimental = true")?;
    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let line = format!("前{left}综上所述{right}后");
    let protected = Markdown::new()?.protect(&[&line]);
    let found = rules.check_line(&line, &protected[0], &config);
    assert_eq!(found.len(), 1);
    let start = "前".len() + left.len();
    assert_eq!(found[0].range, start..start + "综上所述".len());
    assert_eq!(
        snippet(&line, found[0].range.clone()),
        Some(format!("{left}综上所述{right}").as_str())
    );
    Ok(())
}

#[test]
fn sentence_windows_are_lazy_scalar_windows_and_consume_protected_matches() -> TestResult {
    use limae::rules::tells::SentenceTells;
    let rules = SentenceTells::new()?;
    let config = configured("windows", "enable_experimental = true")?;
    let markdown = Markdown::new()?;
    for (opening, ending, limit) in [("不是", "而是", 20), ("not just", "but", 40)] {
        for size in [limit - 1, limit, limit + 1] {
            let line = format!("{opening}{}{ending}", "🙂".repeat(size));
            let protected = markdown.protect(&[&line]);
            let found = rules.check_line(&line, &protected[0], &config);
            assert_eq!(found.len(), usize::from(size <= limit), "{line}");
            if size <= limit {
                assert_eq!(found[0].range, 0..line.len());
                assert_eq!(snippet(&line, found[0].range.clone()), Some(line.as_str()));
            }
        }
    }
    for (line, expected) in [
        (
            "不是甲而是乙而是丙 不是丁而是戊",
            vec!["不是甲而是", "不是丁而是"],
        ),
        (
            "not just x but y but z not just a but b",
            vec!["not just x but", "not just a but"],
        ),
        ("不是\n而是 not just\nbut", vec![]),
        (
            "不是\r而是 not just\rbut",
            vec!["不是\r而是", "not just\rbut"],
        ),
        ("`不是甲而是乙` 不是丁而是戊", vec!["不是丁而是"]),
        ("not just `not just` but", vec![]),
        ("`not just` not just but", vec![]),
        ("xnot just not just but", vec!["not just but"]),
        ("not just xbut but", vec!["not just xbut but"]),
        ("not just butx but", vec!["not just butx but"]),
        ("不是不是甲而是乙而是", vec!["不是不是甲而是"]),
        ("不仅甲更乙 not only x but also y", vec![]),
    ] {
        let protected = markdown.protect(&[line]);
        let found = rules.check_line(line, &protected[0], &config);
        assert_eq!(
            found
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            expected,
            "{line}"
        );
    }
    Ok(())
}

#[test]
fn english_sentence_keywords_boundaries_and_python_whitespace() -> TestResult {
    use limae::rules::tells::SentenceTells;
    let rules = SentenceTells::new()?;
    let config = configured("sentences", "enable_experimental = true")?;
    let markdown = Markdown::new()?;
    for opening in [
        "it's not",
        "that’s not",
        "they're not",
        "they’re not",
        "is not",
        "are not",
        "was not",
        "were not",
        "isn't",
        "aren’t",
        "wasn't",
        "weren’t",
        "ıſ not",
        "İſn’t",
    ] {
        for ending in [
            "it's",
            "that’s",
            "they're",
            "they’re",
            "it is",
            "that is",
            "they are",
            "İT İS",
            "ıT’ſ",
        ] {
            for space in [" ", "\t", "\u{1c}", "\u{1d}", "\u{1e}", "\u{1f}", "\u{a0}"] {
                let line = format!(
                    "{} x, {} y",
                    opening.replace(' ', space),
                    ending.replace(' ', space)
                );
                let protected = markdown.protect(&[&line]);
                let found = rules.check_line(&line, &protected[0], &config);
                assert_eq!(found.len(), 1, "{line}");
                assert_eq!(found[0].range, 0..line.len() - 2);
                assert_eq!(found[0].name, "en-tell-2 English negative parallelism");
            }
        }
    }
    for adjacent in [
        "a", "Z", "0", "_", "İ", "ı", "ſ", "K", "é", "٧", "-", "中", "\u{301}",
    ] {
        let allowed = ["é", "٧", "-", "中", "\u{301}"].contains(&adjacent);
        for line in [
            format!("{adjacent}not just x but"),
            format!("not just{adjacent} but"),
            format!("not just x {adjacent}but"),
            format!("not just x but{adjacent}"),
        ] {
            let protected = markdown.protect(&[&line]);
            let found = rules.check_line(&line, &protected[0], &config);
            assert_eq!(found.len(), usize::from(allowed), "{line}");
        }
    }
    for (line, expected) in [
        ("not\u{1f}juſt x but", 1),
        ("nót just x but", 0),
        ("not juſt x bú t", 0),
    ] {
        let protected = markdown.protect(&[line]);
        assert_eq!(
            rules.check_line(line, &protected[0], &config).len(),
            expected,
            "{line}"
        );
    }
    Ok(())
}

#[test]
fn sentence_rules_sort_both_english_shapes_and_respect_configuration() -> TestResult {
    use limae::rules::tells::SentenceTells;
    let rules = SentenceTells::new()?;
    let config = configured("sentence-order", "enable_experimental = true")?;
    let line = "isn't x it's y not just z but w 不是甲而是乙 零秘密";
    let protected = Markdown::new()?.protect(&[line]);
    let found = rules.check_line(line, &protected[0], &config);
    assert_eq!(
        found
            .iter()
            .map(|m| (m.rule, &line[m.range.clone()]))
            .collect::<Vec<_>>(),
        [
            (RuleId::ZH_TELL_2, "不是甲而是"),
            (RuleId::EN_TELL_2, "isn't x it's"),
            (RuleId::EN_TELL_2, "not just z but"),
            (RuleId::ZH_TELL_5, "零"),
        ]
    );
    assert!(
        rules
            .check_line(line, &protected[0], &ResolvedConfig::default())
            .is_empty()
    );
    let disabled = configured(
        "sentence-disabled",
        "enable_experimental = true\ndisable = ['zh-tell-2', 'en-tell-2', 'zh-tell-5']",
    )?;
    assert!(rules.check_line(line, &protected[0], &disabled).is_empty());
    assert!(
        rules
            .check_line(line, &LineProtection::Verbatim, &config)
            .is_empty()
    );
    assert!(
        found
            .iter()
            .all(|m| config.severity(m.rule) == Severity::Warning)
    );
    let line = "It's not just X, but it's Y";
    let protected = Markdown::new()?.protect(&[line]);
    assert_eq!(
        rules
            .check_line(line, &protected[0], &config)
            .iter()
            .map(|m| &line[m.range.clone()])
            .collect::<Vec<_>>(),
        ["It's not just X, but it's", "not just X, but"]
    );
    Ok(())
}

#[test]
fn coinages_use_each_zero_full_cjk_run_and_whole_line_allowlist() -> TestResult {
    use limae::rules::tells::SentenceTells;
    let rules = SentenceTells::new()?;
    let config = configured("coinages", "enable_experimental = true")?;
    let markdown = Markdown::new()?;
    for (line, expected) in [
        (
            "零，零甲，零甲乙丙，零甲乙丙丁，零甲乙丙丁戊",
            vec![
                "零，".len(),
                "零，零甲，".len(),
                "零，零甲，零甲乙丙，".len(),
            ],
        ),
        ("零甲乙丙丁零秘密", vec!["零甲乙丙丁".len()]),
        ("零零秘密", vec![0, "零".len()]),
        (
            "零\u{4e00}，零\u{9fff}，零\u{3400}，零\u{a000}",
            vec![0, "零\u{4e00}，".len()],
        ),
        (
            "从零建一台，非零退出，零售价格，零秘密",
            vec!["从零建一台，非零退出，零售价格，".len()],
        ),
        (
            "零成本，零风险，零拷贝，零秘密",
            vec!["零成本，零风险，零拷贝，".len()],
        ),
        ("零API，零额外API，零", vec!["零API，".len()]),
        (
            "`零秘密` 零秘密 [x](零秘密) https://example.com/零秘密",
            vec![
                "`零秘密` ".len(),
                "`零秘密` 零秘密 [x](零秘密) https://example.com/".len(),
            ],
        ),
    ] {
        let protected = markdown.protect(&[line]);
        let found = rules.check_line(line, &protected[0], &config);
        assert_eq!(
            found.iter().map(|m| m.range.start).collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        for matched in found {
            assert_eq!(matched.rule, RuleId::ZH_TELL_5);
            assert_eq!(matched.name, "zh-tell-5 zero-noun coinage");
            assert_eq!(&line[matched.range], "零");
        }
    }
    for (line, span, expected) in [
        ("从零秘密", 0.."从".len(), 0),
        ("零售价格", "零".len().."零售".len(), 0),
        ("零秘密", "零".len().."零秘".len(), 1),
    ] {
        let protected = LineProtection::Inline {
            code: vec![span],
            prose: vec![],
        };
        assert_eq!(
            rules.check_line(line, &protected, &config).len(),
            expected,
            "{line}"
        );
    }
    Ok(())
}
