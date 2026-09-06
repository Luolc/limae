use std::error::Error;
use std::path::Path;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, resolve};
use limae::markdown::Markdown;
use limae::rules::typography::WidthRules;
use limae::text::snippet;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn width_only_fixes_leave_spacing_to_later_stages() -> TestResult {
    let rules = WidthRules::new()?;
    let config = ResolvedConfig::default();
    for (input, expected) in [
        (
            include_str!("../../spec/fixtures/zh-typography-2-fullwidth-parens.in"),
            "(测试)\n\n术语(covered call)策略\n\n[链接](https://example.com)(注)\n",
        ),
        (
            include_str!("../../spec/fixtures/zh-typography-10-fullwidth-digits.in"),
            "2011年\n\n全角0也转\n\n半角 0 不动\n",
        ),
    ] {
        let fixed = input
            .split('\n')
            .map(|line| rules.fix_fragment(line, &config))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(fixed, expected);
    }
    Ok(())
}

#[test]
fn punctuation_check_consumes_pairs_but_fix_converts_each_character() -> TestResult {
    let rules = WidthRules::new()?;
    let config = ResolvedConfig::default();
    let line = "（１）,中,.文;字!";
    let protected = Markdown::new()?.protect(&[line]);
    let found = rules.check_line(line, &protected[0], &config);
    let actual: Vec<_> = found
        .iter()
        .map(|matched| {
            (
                matched.rule,
                matched.name.as_ref(),
                &line[matched.range.clone()],
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (
                RuleId::ZH_TYPOGRAPHY_1,
                "zh-typography-1 halfwidth punct next to CJK",
                ",中"
            ),
            (
                RuleId::ZH_TYPOGRAPHY_1,
                "zh-typography-1 halfwidth period next to CJK",
                "."
            ),
            (
                RuleId::ZH_TYPOGRAPHY_1,
                "zh-typography-1 halfwidth punct next to CJK",
                "文;"
            ),
            (
                RuleId::ZH_TYPOGRAPHY_1,
                "zh-typography-1 halfwidth punct next to CJK",
                "字!"
            ),
            (
                RuleId::ZH_TYPOGRAPHY_2,
                "zh-typography-2 fullwidth paren",
                "（"
            ),
            (
                RuleId::ZH_TYPOGRAPHY_2,
                "zh-typography-2 fullwidth paren",
                "）"
            ),
            (
                RuleId::ZH_TYPOGRAPHY_10,
                "zh-typography-10 fullwidth digit",
                "１"
            ),
        ]
    );
    assert_eq!(rules.fix_fragment(line, &config), "(1)，中，。文；字！");
    Ok(())
}

#[test]
fn periods_use_abbreviation_and_ascii_adjacency_exemptions() -> TestResult {
    let rules = WidthRules::new()?;
    let config = ResolvedConfig::default();
    let markdown = Markdown::new()?;
    for abbreviation in [
        "e.g.", "i.e.", "etc.", "cf.", "vs.", "Mr.", "Mrs.", "Ms.", "Dr.", "Prof.", "St.",
    ] {
        for prefix in ["", "中", "3", "é", "a", "（", "１"] {
            let line = format!("{prefix}{abbreviation}中");
            let protected = markdown.protect(&[&line]);
            let periods = rules
                .check_line(&line, &protected[0], &config)
                .into_iter()
                .filter(|m| m.rule == RuleId::ZH_TYPOGRAPHY_1)
                .count();
            assert_eq!(periods, usize::from(prefix == "a"), "{line}");
            let converted = line.replace('（', "(").replace('１', "1");
            let expected = if prefix == "a" {
                format!("{}。中", &converted[..converted.len() - ".中".len()])
            } else {
                converted
            };
            assert_eq!(rules.fix_fragment(&line, &config), expected);
        }
    }
    for (line, expected, count) in [
        (
            "中.md 中.3 中... ...文 .文 x.文",
            "中.md 中.3 中... ...文 。文 x。文",
            2,
        ),
        ("中.🙂e\u{301}.文", "中。🙂e\u{301}。文", 2),
        ("中.１", "中.1", 1),
        ("", "", 0),
    ] {
        let protected = markdown.protect(&[line]);
        let periods = rules
            .check_line(line, &protected[0], &config)
            .into_iter()
            .filter(|m| m.rule == RuleId::ZH_TYPOGRAPHY_1)
            .count();
        assert_eq!(periods, count, "{line}");
        assert_eq!(rules.fix_fragment(line, &config), expected);
    }
    Ok(())
}

#[test]
fn width_checks_use_markdown_protection_and_scalar_snippets() -> TestResult {
    let rules = WidthRules::new()?;
    let config = ResolvedConfig::default();
    let lines = [
        "中,（１）",
        "```",
        "中,（１）",
        "~~~",
        "`中,（１）`",
        "[x](中,（１）)",
        "「かな中,（１）」",
        "`中,",
        "（１）`",
        "https://example.com,中",
    ];
    let protected = Markdown::new()?.protect(&lines);
    let counts: Vec<_> = lines
        .iter()
        .zip(&protected)
        .map(|(line, p)| rules.check_line(line, p, &config).len())
        .collect();
    assert_eq!(counts, [4, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let line = format!("前{left}１{right}后");
    let protected = Markdown::new()?.protect(&[&line]);
    let found = rules.check_line(&line, &protected[0], &config);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].range,
        "前".len() + left.len().."前".len() + left.len() + '１'.len_utf8()
    );
    assert_eq!(
        snippet(&line, found[0].range.clone()),
        Some(format!("{left}１{right}").as_str())
    );
    Ok(())
}

#[test]
fn each_width_rule_can_be_disabled_independently() -> TestResult {
    let rules = WidthRules::new()?;
    let line = "中.１（文,）";
    let protected = Markdown::new()?.protect(&[line]);
    for mask in 0..8 {
        let ids = [
            RuleId::ZH_TYPOGRAPHY_1,
            RuleId::ZH_TYPOGRAPHY_2,
            RuleId::ZH_TYPOGRAPHY_10,
        ];
        let disabled: Vec<_> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, id)| id.to_string())
            .collect();
        let config = resolve(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            CliOverrides {
                disable: Some(&disabled),
                enable: None,
            },
        )?;
        let expected = format!(
            "中{}{}{}文{}{}",
            if mask & 1 == 0 && mask & 4 != 0 {
                "。"
            } else {
                "."
            },
            if mask & 4 == 0 { "1" } else { "１" },
            if mask & 2 == 0 { "(" } else { "（" },
            if mask & 1 == 0 { "，" } else { "," },
            if mask & 2 == 0 { ")" } else { "）" }
        );
        assert_eq!(rules.fix_fragment(line, &config), expected, "mask {mask}");
        assert_eq!(
            rules.fix_fragment(&expected, &config),
            expected,
            "mask {mask}"
        );
        let actual: Vec<_> = rules
            .check_line(line, &protected[0], &config)
            .into_iter()
            .map(|m| m.rule)
            .collect();
        let expected: Vec<_> = [ids[0], ids[0], ids[1], ids[1], ids[2]]
            .into_iter()
            .filter(|id| config.is_enabled(*id))
            .collect();
        assert_eq!(actual, expected, "mask {mask}");
    }
    Ok(())
}
