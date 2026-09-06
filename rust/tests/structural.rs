use std::error::Error;
use std::path::Path;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, resolve};
use limae::markdown::Markdown;
use limae::pipeline::Pipeline;
use limae::rules::{
    LineMatch,
    structural::{FragmentContext, StructuralRules},
};
use limae::text::snippet;

type TestResult = Result<(), Box<dyn Error>>;

fn configured(disable: &[String], enable: &[String]) -> Result<ResolvedConfig, Box<dyn Error>> {
    Ok(resolve(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        CliOverrides {
            disable: Some(disable),
            enable: Some(enable),
        },
    )?)
}

#[test]
fn punctuation_directions_consume_independently_and_merge_stably() -> TestResult {
    let rules = StructuralRules::new()?;
    for (line, expected_ranges, fixed) in [
        (
            "， ， 文",
            vec![(0..7, "before"), (0..7, "after")],
            "，，文",
        ),
        (
            "， ， ， ，",
            vec![
                (0..7, "before"),
                (0..7, "after"),
                (8..15, "before"),
                (8..15, "after"),
            ],
            "，，，，",
        ),
        (
            "前后 ： 都删",
            vec![(3..10, "before"), (7..14, "after")],
            "前后：都删",
        ),
        (
            "— ， ，",
            vec![(4..11, "before"), (4..11, "after")],
            "— ，，",
        ),
        (
            "a | ， ，",
            vec![(4..11, "before"), (4..11, "after")],
            "a | ，，",
        ),
    ] {
        let expected: Vec<_> = expected_ranges
            .into_iter()
            .map(|(range, direction)| LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_11,
                name: if direction == "before" {
                    "zh-typography-11 space before fullwidth punct"
                } else {
                    "zh-typography-11 space after fullwidth punct"
                }
                .into(),
                range,
            })
            .collect();
        let config = ResolvedConfig::default();
        let p = Markdown::new()?.protect(&[line]);
        assert_eq!(rules.check_line(line, &p[0], &config), expected, "{line}");
        assert_eq!(
            rules.fix_fragment(line, &config, FragmentContext::default()),
            fixed
        );
    }
    Ok(())
}

#[test]
fn code_delimiters_use_equal_runs_and_zero_length_multiline_interiors() -> TestResult {
    let rules = StructuralRules::new()?;
    let config = ResolvedConfig::default();
    let lines = [
        "🙂中``",
        "``文A",
        "",
        "中``a`b``文",
        "",
        "裸`文",
        "",
        "中`甲`文`乙`字",
    ];
    let p = Markdown::new()?.protect(&lines);
    let expected = [
        std::iter::once(7..9).collect(),
        std::iter::once(0..2).collect(),
        vec![],
        vec![3..5, 8..10],
        vec![],
        vec![],
        vec![],
        vec![3..4, 7..8, 11..12, 15..16],
    ];
    for ((line, p), expected) in lines.iter().zip(&p).zip(expected) {
        let found = rules.check_line(line, p, &config);
        assert_eq!(
            found.iter().map(|m| m.range.clone()).collect::<Vec<_>>(),
            expected,
            "{line}"
        );
        assert!(found.iter().all(|m| m.rule == RuleId::ZH_TYPOGRAPHY_7));
    }
    assert_eq!(
        Pipeline::new()?.fix(&lines.join("\n"), &config)?,
        [
            "🙂中 ``",
            "`` 文 A",
            "",
            "中 ``a`b`` 文",
            "",
            "裸`文",
            "",
            "中 `甲` 文 `乙` 字"
        ]
        .join("\n")
    );
    Ok(())
}

#[test]
fn dashes_consume_neighbors_but_fix_each_side() -> TestResult {
    let rules = StructuralRules::new()?;
    let config = ResolvedConfig::default();
    for (line, expected, slices) in [
        (
            "🙂e\u{301}⸺中——文",
            "🙂e\u{301} ⸺ 中 —— 文",
            vec!["\u{301}⸺", "⸺中", "中——", "——文"],
        ),
        ("中——", "中 ——", vec!["中——"]),
        ("⸺文", "⸺ 文", vec!["⸺文"]),
        (
            "中—文 中———文 中————文 a-b 1–2",
            "中—文 中———文 中————文 a-b 1–2",
            vec![],
        ),
        ("中—⸺文", "中—⸺ 文", vec!["⸺文"]),
        ("中⸺⸺文", "中 ⸺⸺ 文", vec!["中⸺", "⸺文"]),
        ("中，——文", "中， —— 文", vec!["，——", "——文"]),
    ] {
        let p = Markdown::new()?.protect(&[line]);
        assert_eq!(
            rules
                .check_line(line, &p[0], &config)
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            slices,
            "{line}"
        );
        assert_eq!(
            rules.fix_fragment(line, &config, FragmentContext::default()),
            expected,
            "{line}"
        );
    }
    for space in [
        '\t', '\r', '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}', '\u{85}', '\u{a0}', '\u{3000}',
    ] {
        let line = format!("{space}——{space}， {space} 文");
        let p = Markdown::new()?.protect(&[&line]);
        assert_eq!(rules.check_line(&line, &p[0], &config), []);
        assert_eq!(
            rules.fix_fragment(&line, &config, FragmentContext::default()),
            line
        );
    }
    Ok(())
}

#[test]
fn links_check_consuming_openers_and_fix_fragment_local_lookahead() -> TestResult {
    let rules = StructuralRules::new()?;
    let config = configured(&[], &["zh-typography-9".into()])?;
    for (line, expected, slices) in [
        ("中[文[字](x)", "中 [文 [字](x)", vec!["[文[字]("]),
        ("[中[字](x)", "[中 [字](x)", vec!["[字]("]),
        (
            "中[文`x`字](url)后",
            "中[文 `x` 字](url) 后",
            vec!["`", "`"],
        ),
        ("中[普通] 中![图](url)", "中[普通] 中！[图](url)", vec![]),
        ("中[字](url)后", "中 [字](url) 后", vec!["[字]("]),
    ] {
        let p = Markdown::new()?.protect(&[line]);
        assert_eq!(
            rules
                .check_line(line, &p[0], &config)
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            slices,
            "{line}"
        );
        assert_eq!(Pipeline::new()?.fix(line, &config)?, expected, "{line}");
    }
    Ok(())
}

#[test]
fn protection_preserves_code_urls_destinations_and_kana_quotations() -> TestResult {
    let rules = StructuralRules::new()?;
    let config = configured(&[], &["zh-typography-9".into()])?;
    let lines = [
        "中`——， 文`后",
        "https://example.com——文",
        "中——https://example.com",
        "文 ，https://example.com",
        "https://example.com ， 文",
        "[x](， 文——字)",
        "「かな中`x`字——文， 字」",
        "```",
        "中`x`字——文， 字",
        "~~~",
    ];
    let p = Markdown::new()?.protect(&lines);
    let counts: Vec<_> = lines
        .iter()
        .zip(&p)
        .map(|(line, p)| rules.check_line(line, p, &config).len())
        .collect();
    assert_eq!(counts, [2, 1, 1, 1, 1, 0, 0, 0, 0, 0]);
    assert_eq!(
        Pipeline::new()?.fix(&lines.join("\n"), &config)?,
        [
            "中 `——， 文` 后",
            "https://example.com—— 文",
            "中 ——https://example.com",
            "文，https://example.com",
            "https://example.com ，文",
            "[x](， 文——字)",
            "「かな中`x`字——文， 字」",
            "```",
            "中`x`字——文， 字",
            "~~~",
        ]
        .join("\n")
    );
    Ok(())
}

#[test]
fn config_switches_and_width_spacing_interactions_are_real() -> TestResult {
    let line = "中（１６GB）用`x`字——文, 字[链](url)后";
    let p = Markdown::new()?.protect(&[line]);
    let rules = StructuralRules::new()?;
    for (disabled, enabled, count, expected) in [
        ("", "", 4, "中 (16 GB) 用 `x` 字 —— 文，字[链](url) 后"),
        (
            "zh-typography-7",
            "",
            2,
            "中 (16 GB) 用`x`字 —— 文，字[链](url) 后",
        ),
        (
            "zh-typography-8",
            "",
            2,
            "中 (16 GB) 用 `x` 字——文，字[链](url) 后",
        ),
        (
            "zh-typography-11",
            "",
            4,
            "中 (16 GB) 用 `x` 字 —— 文， 字[链](url) 后",
        ),
        (
            "",
            "zh-typography-9",
            5,
            "中 (16 GB) 用 `x` 字 —— 文，字 [链](url) 后",
        ),
    ] {
        let config = configured(
            &disabled
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            &enabled
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
        )?;
        assert_eq!(rules.check_line(line, &p[0], &config).len(), count);
        assert_eq!(Pipeline::new()?.fix(line, &config)?, expected);
    }
    let line = "🙂中 ， 文";
    let config = configured(&["zh-typography-11".into()], &[])?;
    let p = Markdown::new()?.protect(&[line]);
    assert_eq!(rules.check_line(line, &p[0], &config), []);
    assert_eq!(Pipeline::new()?.fix(line, &config)?, line);
    Ok(())
}

#[test]
fn original_ranges_keep_twelve_scalar_snippets() -> TestResult {
    let rules = StructuralRules::new()?;
    let left = "🙂".repeat(11) + "中";
    let right = "e\u{301}".repeat(6);
    let line = format!("前{left} ，{right}后");
    let p = Markdown::new()?.protect(&[&line]);
    let found = rules.check_line(&line, &p[0], &ResolvedConfig::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].range, 47..54);
    assert_eq!(
        snippet(&line, found[0].range.clone()),
        Some(format!("前{left} ，{right}").as_str())
    );
    Ok(())
}
