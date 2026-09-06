use std::error::Error;
use std::fs;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, resolve};
use limae::markdown::{LineProtection, Markdown};
use limae::rules::{spacing::SpacingRules, typography::WidthRules};
use limae::text::snippet;

type TestResult = Result<(), Box<dyn Error>>;

fn config(contents: &str) -> Result<ResolvedConfig, Box<dyn Error>> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "limae-spacing-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&root)?;
    fs::write(root.join("limae.toml"), contents)?;
    let result = resolve(&root, CliOverrides::default());
    fs::remove_dir_all(root)?;
    Ok(result?)
}

#[test]
fn parens_keep_consuming_ranges_and_independent_directions() -> TestResult {
    let rules = SpacingRules::new()?;
    let config = ResolvedConfig::default();
    let markdown = Markdown::new()?;
    for (line, expected, slices) in [
        ("中(文)字", "中 (文) 字", vec!["中(", ")字"]),
        ("f(g(x))中", "f(g(x)) 中", vec![")中"]),
        ("中()()字", "中 () () 字", vec!["中(", ")(", ")字"]),
        ("(x)**中**", "(x) **中**", vec![")**中"]),
        ("**中(x)**", "**中 (x)**", vec!["中("]),
        ("**(x)", "** (x)", vec!["**("]),
        ("(x)**", "(x)**", vec![]),
        ("`x`(y)`z`", "`x` (y) `z`", vec!["`(", ")`"]),
        ("[中](url)(注)", "[中](url) (注)", vec![")("]),
    ] {
        let protection = markdown.protect(&[line]);
        let found = rules.check_line(line, &protection[0], &config);
        assert_eq!(
            found
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            slices,
            "{line}"
        );
        assert!(found.iter().all(|m| m.rule == RuleId::ZH_TYPOGRAPHY_3));
        assert_eq!(rules.fix_fragment(line, &config), expected, "{line}");
    }
    Ok(())
}

#[test]
fn adjacent_zero_width_matches_use_byte_offsets_and_scalar_context() -> TestResult {
    let rules = SpacingRules::new()?;
    let config = ResolvedConfig::default();
    let line = "🙂e\u{301}用A表示第3个 一B鿿 㐀C\u{a000} é中Ａ中３";
    let protection = Markdown::new()?.protect(&[line]);
    let found = rules.check_line(line, &protection[0], &config);
    let positions: Vec<_> = found.iter().map(|m| m.range.clone()).collect();
    assert_eq!(positions, [10..10, 11..11, 28..28, 29..29, 20..20, 21..21]);
    assert_eq!(
        rules.fix_fragment(line, &config),
        "🙂e\u{301}用 A 表示第 3 个 一 B 鿿 㐀C\u{a000} é中Ａ中３"
    );

    let left = "🙂".repeat(11) + "中";
    let right = "Ae\u{301}".to_owned() + &"🙂".repeat(9);
    let line = format!("前{left}{right}后");
    let p = Markdown::new()?.protect(&[&line]);
    let found = rules.check_line(&line, &p[0], &config);
    assert_eq!(found.len(), 1);
    assert_eq!(
        snippet(&line, found[0].range.clone()),
        Some(format!("{left}{right}").as_str())
    );
    Ok(())
}

#[test]
fn measure_words_exempt_both_number_run_ends_only() -> TestResult {
    let rules = SpacingRules::new()?;
    let markdown = Markdown::new()?;
    let line = "第3天1次 共1,000天 共1.5年 2011年5月";
    for (units, expected, count) in [
        ("", "第 3 天 1 次 共 1,000 天 共 1.5 年 2011 年 5 月", 11),
        ("天", "第3天 1 次 共1,000天 共 1.5 年 2011 年 5 月", 7),
        ("天年月", "第3天 1 次 共1,000天 共1.5年 2011年5月", 2),
    ] {
        let config = config(&format!("skip_zh_units = \"{units}\""))?;
        let p = markdown.protect(&[line]);
        assert_eq!(
            rules.check_line(line, &p[0], &config).len(),
            count,
            "{units}"
        );
        assert_eq!(rules.fix_fragment(line, &config), expected, "{units}");
    }
    Ok(())
}

#[test]
fn ascii_units_keep_case_token_edges_and_complete_match_ranges() -> TestResult {
    let rules = SpacingRules::new()?;
    let config = ResolvedConfig::default();
    let markdown = Markdown::new()?;
    for unit in [
        "KB", "MB", "GB", "TB", "PB", "KiB", "MiB", "GiB", "TiB", "PiB", "bps", "kbps", "Mbps",
        "Gbps", "Tbps", "ms", "ns", "us", "min", "Hz", "kHz", "MHz", "GHz", "px", "pt", "dpi",
        "fps", "kg", "mg", "km", "cm", "mm", "nm",
    ] {
        let line = format!("16{unit}/1.5{unit}");
        let p = markdown.protect(&[&line]);
        let found = rules.check_line(&line, &p[0], &config);
        assert_eq!(
            found
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            [format!("16{unit}"), format!("5{unit}")]
        );
        assert_eq!(
            rules.fix_fragment(&line, &config),
            format!("16 {unit}/1.5 {unit}")
        );
    }
    for (line, expected, slices) in [
        (
            "0x16GB A16GB 16GBx 16GB2 16gb 16Gb 16gB 16g 15% 90°",
            "0x16GB A16GB 16GBx 16GB2 16gb 16Gb 16gB 16g 15% 90°",
            vec![],
        ),
        (
            "é16GB_16GB🙂16GB",
            "é16 GB_16 GB🙂16 GB",
            vec!["16GB", "16GB", "16GB"],
        ),
    ] {
        let p = markdown.protect(&[line]);
        let found = rules.check_line(line, &p[0], &config);
        assert_eq!(
            found
                .iter()
                .map(|m| &line[m.range.clone()])
                .collect::<Vec<_>>(),
            slices
        );
        assert_eq!(rules.fix_fragment(line, &config), expected);
    }
    Ok(())
}

#[test]
fn protection_checks_overlap_and_both_zero_width_endpoints() -> TestResult {
    let rules = SpacingRules::new()?;
    let config = ResolvedConfig::default();
    let lines = [
        "用A表示 第3个 16GB 中(x)文",
        "```",
        "用A表示 第3个 16GB 中(x)文",
        "~~~",
        "`用A表示 第3个 16GB 中(x)文`",
        "[x](用A表示/第3个/16GB)",
        "「かな用A表示 第3个 16GB 中(x)文」",
        "用https://example.com/16GB表示",
        "`用A表示",
        "第3个 16GB`(x)文",
    ];
    let protected = Markdown::new()?.protect(&lines);
    let counts: Vec<_> = lines
        .iter()
        .zip(protected)
        .map(|(line, p)| rules.check_line(line, &p, &config).len())
        .collect();
    assert_eq!(counts, [7, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
    let line = "用A表示";
    let p = LineProtection::Inline {
        code: Vec::new(),
        prose: std::iter::once(3..4).collect(),
    };
    assert_eq!(rules.check_line(line, &p, &config), []);
    Ok(())
}

#[test]
fn width_then_spacing_and_disabled_stages_have_distinct_results() -> TestResult {
    let width = WidthRules::new()?;
    let spacing = SpacingRules::new()?;
    let line = "中（１６GB）用A第３天";
    for (disabled, units, expected, checked) in [
        ("", "", "中 (16 GB) 用 A 第 3 天", 2),
        ("zh-typography-2", "", "中（16 GB）用 A 第 3 天", 2),
        ("zh-typography-10", "", "中 (１６GB) 用 A 第３天", 2),
        ("zh-typography-3", "", "中(16 GB)用 A 第 3 天", 2),
        ("zh-typography-4", "", "中 (16 GB) 用A第 3 天", 0),
        ("zh-typography-5", "", "中 (16 GB) 用 A 第3天", 2),
        ("zh-typography-6", "", "中 (16GB) 用 A 第 3 天", 2),
        ("", "天", "中 (16 GB) 用 A 第3天", 2),
        ("zh-typography-5", "天", "中 (16 GB) 用 A 第3天", 2),
    ] {
        let disable = if disabled.is_empty() {
            "[]".to_owned()
        } else {
            format!("[\"{disabled}\"]")
        };
        let config = config(&format!("disable = {disable}\nskip_zh_units = \"{units}\""))?;
        let p = Markdown::new()?.protect(&[line]);
        assert_eq!(
            spacing.check_line(line, &p[0], &config).len(),
            checked,
            "{disabled}"
        );
        let fixed = spacing.fix_fragment(&width.fix_fragment(line, &config), &config);
        assert_eq!(fixed, expected, "{disabled}");
        assert_eq!(
            spacing.fix_fragment(&width.fix_fragment(&fixed, &config), &config),
            fixed
        );
    }
    Ok(())
}

#[test]
fn disabled_spacing_rules_suppress_original_matches_and_fixes() -> TestResult {
    let spacing = SpacingRules::new()?;
    let line = "中(x)文 用A表 第3个 16GB";
    let p = Markdown::new()?.protect(&[line]);
    for (disabled, expected, count) in [
        ("zh-typography-3", "中(x)文 用 A 表 第 3 个 16 GB", 5),
        ("zh-typography-4", "中 (x) 文 用A表 第 3 个 16 GB", 5),
        ("zh-typography-5", "中 (x) 文 用 A 表 第3个 16 GB", 5),
        ("zh-typography-6", "中 (x) 文 用 A 表 第 3 个 16GB", 6),
    ] {
        let config = config(&format!("disable = [\"{disabled}\"]"))?;
        let found = spacing.check_line(line, &p[0], &config);
        assert_eq!(found.len(), count, "{disabled}");
        assert!(found.iter().all(|m| m.rule.as_str() != disabled));
        assert_eq!(spacing.fix_fragment(line, &config), expected, "{disabled}");
    }
    Ok(())
}
