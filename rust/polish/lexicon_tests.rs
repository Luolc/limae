use super::{Detail, LexiconError, render};

/// A synthetic lexicon in the shape of `spec/lexicon/zh.toml`.
const SOURCE: &str = r#"
title = "词典标题"
subtitle = "副标题"
preface = ["引子"]
standard = "判据一句。"
threshold = "门槛一段。"

[[entry]]
term = "甲词"
pinyin = ["jiǎ", "cí"]
plain = "乙说法"
gloss = "本意甲。"
fault = "不这么说。"
examples = [
  { before = "原句甲。", after = "改句乙。" },
]
"#;

#[test]
fn full_carries_the_standard_meaning_and_examples() -> Result<(), LexiconError> {
    let markdown = render(SOURCE, Detail::Full)?;
    assert_eq!(
        markdown,
        "# 词典标题\n\n判据一句。\n\n## 甲词\n\n- 白：乙说法\n- 解：本意甲。\n- 病：不这么说。\n- 原：原句甲。\n  改：改句乙。\n"
    );
    Ok(())
}

#[test]
fn brief_carries_only_the_term_its_replacement_and_its_fault() -> Result<(), LexiconError> {
    let markdown = render(SOURCE, Detail::Brief)?;
    assert_eq!(
        markdown,
        "# 词典标题\n\n## 甲词\n\n- 白：乙说法\n- 病：不这么说。\n"
    );
    Ok(())
}

#[test]
fn a_missing_field_names_its_entry() {
    let source = SOURCE.replace("fault = \"不这么说。\"\n", "");
    match render(&source, Detail::Full) {
        Err(LexiconError::Shape { key, .. }) => assert_eq!(key, "entry[0].fault"),
        other => panic!("expected a shape error, got {other:?}"),
    }
}
