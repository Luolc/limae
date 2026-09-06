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
