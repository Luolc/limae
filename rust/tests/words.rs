use std::error::Error;
use std::fs;

use limae::config::{CliOverrides, ResolvedConfig, RuleId, Severity, resolve};
use limae::markdown::{LineProtection, Markdown};
use limae::pipeline::Pipeline;
use limae::rules::words::WordRules;
use limae::text::snippet;

type TestResult = Result<(), Box<dyn Error>>;

fn configured(name: &str, content: &str) -> Result<ResolvedConfig, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("limae-words-{name}-{}", std::process::id()));
    fs::create_dir(&root)?;
    fs::write(root.join("limae.toml"), content)?;
    let config = resolve(&root, CliOverrides::default());
    fs::remove_dir_all(root)?;
    Ok(config?)
}

#[test]
fn term_findings_own_names_and_keep_each_original_range_and_scalar_snippet() -> TestResult {
    let config = configured("findings", "enable_experimental = true")?;
    let left = "🙂".repeat(12);
    let right = "e\u{301}".repeat(6);
    let line = format!("前{left}秘钥{right}后 秘钥 代币 快取 秘密 `token cache`");
    let protection = Markdown::new()?.protect(&[&line]);
    let found = WordRules::new()?.check_line(&line, &protection[0], &config);
    let expected = [
        (RuleId::ZH_WORD_1, "zh-word-1 term 秘钥 -> 密钥", "秘钥"),
        (RuleId::ZH_WORD_1, "zh-word-1 term 秘钥 -> 密钥", "秘钥"),
        (RuleId::ZH_WORD_1, "zh-word-1 term 代币 -> 令牌", "代币"),
        (RuleId::ZH_WORD_1, "zh-word-1 term 快取 -> 缓存", "快取"),
        (RuleId::ZH_WORD_2, "zh-word-2 misused 秘密", "秘密"),
    ];
    assert_eq!(
        found
            .iter()
            .map(|m| (m.rule, m.name.as_ref(), &line[m.range.clone()]))
            .collect::<Vec<_>>(),
        expected,
    );
    let start = "前".len() + left.len();
    assert_eq!(found[0].range, start..start + "秘钥".len());
    assert_eq!(
        snippet(&line, found[0].range.clone()),
        Some(format!("{left}秘钥{right}").as_str())
    );
    assert!(
        found
            .iter()
            .all(|m| config.severity(m.rule) == Severity::Warning)
    );
    Ok(())
}

#[test]
fn anchors_use_whole_line_lowercase_but_wrong_terms_are_literal_and_protected() -> TestResult {
    let config = configured("anchors", "enable_experimental = true")?;
    let rules = WordRules::new()?;
    let markdown = Markdown::new()?;
    for (line, count) in [
        ("秘钥 `KEY`", 1),
        ("秘钥 `KEY`", 1),
        ("秘钥 `ſecret`", 0),
        ("秘钥 `credentİal`", 0),
        ("秘钥 `credentıal`", 0),
        ("秘钥 [x](TOKEN)", 1),
        ("快取 https://example.com/CACHE", 1),
        ("代币「かな OAuth」", 1),
        ("代币 preJWTpost", 1),
        ("代币换成现金", 0),
        ("秘钥 加密", 1),
        ("秘鑰 token", 0),
        ("`秘钥 token` 秘钥", 1),
        ("[x](秘钥 token) 秘钥", 1),
        ("「かな秘钥 token」秘钥", 1),
        ("秘`钥` token", 0),
    ] {
        let protected = markdown.protect(&[line]);
        assert_eq!(
            rules.check_line(line, &protected[0], &config).len(),
            count,
            "{line}"
        );
        assert_eq!(
            rules
                .active_terms(line, &config)
                .fix_fragment("秘钥 代币 快取")
                .contains("密钥"),
            ["KEY", "KEY", "TOKEN", "加密", "token"]
                .iter()
                .any(|a| line.contains(a)),
            "{line}",
        );
    }
    assert!(
        rules
            .check_line("秘钥 token 秘密", &LineProtection::Verbatim, &config)
            .is_empty()
    );
    Ok(())
}

#[test]
fn fragment_fixes_reuse_original_line_evidence_across_markdown_interiors() -> TestResult {
    let config = configured("fragments", "enable_experimental = true")?;
    let rules = WordRules::new()?;
    let line = "秘钥 `token 秘钥` 代币 [x](cache 快取) 快取「かな 秘钥」秘密";
    assert_eq!(
        Pipeline::new()?.fix(line, &config),
        "密钥 `token 秘钥` 令牌 [x](cache 快取) 缓存「かな 秘钥」秘密"
    );
    assert_eq!(
        rules
            .active_terms("代币换成现金", &config)
            .fix_fragment("代币"),
        "代币"
    );
    Ok(())
}

#[test]
fn secrets_use_whole_line_allowlist_coverage_per_occurrence_without_fixes() -> TestResult {
    let config = configured("secrets", "enable_experimental = true")?;
    let rules = WordRules::new()?;
    let line = "秘密 保守秘密 商业秘密 国家秘密 秘密秘密 `秘密`";
    let protected = Markdown::new()?.protect(&[line]);
    let found = rules.check_line(line, &protected[0], &config);
    let second = "秘密 保守秘密 商业秘密 国家秘密 ".len();
    assert_eq!(
        found.iter().map(|m| m.range.clone()).collect::<Vec<_>>(),
        [0..6, second..second + 6, second + 6..second + 12]
    );
    assert_eq!(
        rules.active_terms(line, &config).fix_fragment("秘密秘密"),
        "秘密秘密"
    );
    let line = "保守秘密 秘密";
    let prefix = 0.."保守".len();
    let protection = LineProtection::Inline {
        code: vec![prefix],
        prose: vec![],
    };
    assert_eq!(
        rules.check_line(line, &protection, &config)[0].range.start,
        "保守秘密 ".len()
    );
    for content in [
        "",
        "enable_experimental = true\ndisable = ['zh-word-1', 'zh-word-2']",
    ] {
        let config = configured("disabled", content)?;
        assert!(
            rules
                .check_line(
                    "token 秘钥 秘密",
                    &Markdown::new()?.protect(&["token 秘钥 秘密"])[0],
                    &config
                )
                .is_empty()
        );
        assert_eq!(
            rules
                .active_terms("token 秘钥", &config)
                .fix_fragment("秘钥"),
            "秘钥"
        );
    }
    Ok(())
}

#[test]
fn unicode_fixture_checks_only_the_term_primitive_contract() -> TestResult {
    // These five anchor lines exercise the single terminology stage.
    // The document runner compares the complete mixed fixture.
    let text = include_str!("../../spec/fixtures/experimental-english-unicode.in");
    let config = configured("unicode-fixture", "enable_experimental = true")?;
    let rules = WordRules::new()?;
    let markdown = Markdown::new()?;
    for (index, expected) in [
        (90, "密钥"),
        (92, "密钥"),
        (94, "秘钥"),
        (96, "秘钥"),
        (98, "秘钥"),
    ] {
        let line = text.lines().nth(index).ok_or("missing anchor line")?;
        assert!(line.starts_with("秘钥 "));
        let protected = markdown.protect(&[line]);
        assert_eq!(
            rules.check_line(line, &protected[0], &config).len(),
            usize::from(expected == "密钥")
        );
        assert_eq!(
            rules.active_terms(line, &config).fix_fragment("秘钥 "),
            format!("{expected} ")
        );
    }
    Ok(())
}
