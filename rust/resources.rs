//! Shared specification data read at build time, with no runtime paths.

use crate::config::RuleId;
use crate::text::is_python_whitespace;
use thiserror::Error;
use toml::Value;

pub(crate) const ZERO_ALLOWLIST: &str = include_str!("../spec/wordlists/zh-tell-5-allow.txt");
pub(crate) const SECRET_ALLOWLIST: &str = include_str!("../spec/wordlists/zh-word-2-allow.txt");
pub(crate) const TERMS: &str = include_str!("../spec/wordlists/zh-word-1.toml");

/// The embedded terminology resource could not be parsed or validated.
#[derive(Debug, Error)]
pub enum TermResourceError {
    #[error("cannot parse spec/wordlists/zh-word-1.toml: {source}")]
    Parse {
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("spec/wordlists/zh-word-1.toml: `{key}` must be {expected}")]
    Invalid { key: String, expected: &'static str },
    #[error("spec/wordlists/zh-word-1.toml: entries[{entry}].wrong duplicates an earlier entry")]
    DuplicateWrong { entry: usize },
}

pub(crate) struct Term {
    pub wrong: String,
    pub right: String,
    pub anchors: Vec<String>,
}

pub(crate) fn terms(text: &str) -> Result<Vec<Term>, TermResourceError> {
    let value: Value = toml::from_str(text).map_err(|mut source: toml::de::Error| {
        source.set_input(None);
        TermResourceError::Parse {
            source: Box::new(source),
        }
    })?;
    let entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| TermResourceError::Invalid {
            key: "entries".into(),
            expected: "an array of tables",
        })?;
    let mut terms: Vec<Term> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let invalid = |field: &str, expected| TermResourceError::Invalid {
            key: format!("entries[{index}]{field}"),
            expected,
        };
        let table = entry.as_table().ok_or_else(|| invalid("", "a table"))?;
        let string = |field| {
            table
                .get(field)
                .and_then(Value::as_str)
                .ok_or_else(|| invalid(&format!(".{field}"), "a string"))
        };
        let wrong = string("wrong")?;
        if wrong.is_empty() {
            return Err(invalid(".wrong", "a non-empty string"));
        }
        if terms.iter().any(|term| term.wrong == wrong) {
            return Err(TermResourceError::DuplicateWrong { entry: index });
        }
        let right = string("right")?;
        let anchors = table
            .get("anchors")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(".anchors", "an array of strings"))?
            .iter()
            .map(|anchor| {
                anchor
                    .as_str()
                    .map(str::to_lowercase)
                    .ok_or_else(|| invalid(".anchors", "an array of strings"))
            })
            .collect::<Result<_, _>>()?;
        terms.push(Term {
            wrong: wrong.to_owned(),
            right: right.to_owned(),
            anchors,
        });
    }
    Ok(terms)
}

pub(crate) const TELL_WORDLISTS: [(RuleId, &str, &str); 5] = [
    (
        RuleId::ZH_TELL_1,
        "zh-tell-1 formulaic phrase",
        include_str!("../spec/wordlists/zh-tell-1.txt"),
    ),
    (
        RuleId::ZH_TELL_3,
        "zh-tell-3 corporate buzzword",
        include_str!("../spec/wordlists/zh-tell-3.txt"),
    ),
    (
        RuleId::ZH_TELL_4,
        "zh-tell-4 chat residue",
        include_str!("../spec/wordlists/zh-tell-4.txt"),
    ),
    (
        RuleId::EN_TELL_1,
        "en-tell-1 English AI vocabulary",
        include_str!("../spec/wordlists/en-tell-1.txt"),
    ),
    (
        RuleId::EN_TELL_3,
        "en-tell-3 Claudish register",
        include_str!("../spec/wordlists/en-tell-3.txt"),
    ),
];

pub(crate) fn phrases(text: &str) -> Vec<&str> {
    text.split([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ])
    .map(|line| line.trim_matches(is_python_whitespace))
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .collect()
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    #[test]
    fn term_resource_errors_keep_the_cause_without_source_text() -> Result<(), Box<dyn Error>> {
        let source = "# synthetic private document marker\nentries = [";
        let error = super::terms(source).err().ok_or("expected parse error")?;
        assert!(matches!(error, super::TermResourceError::Parse { .. }));
        assert!(error.to_string().contains("spec/wordlists/zh-word-1.toml"));
        let cause = error.source().ok_or("missing TOML cause")?;
        assert!(!cause.to_string().is_empty());
        for rendered in [
            error.to_string(),
            format!("{error:?}"),
            cause.to_string(),
            format!("{cause:?}"),
        ] {
            assert!(!rendered.contains("synthetic private document marker"));
        }
        Ok(())
    }

    #[test]
    fn term_resource_validates_structure_and_unique_nonempty_wrong() -> Result<(), Box<dyn Error>> {
        assert!(super::terms("entries = []")?.is_empty());
        let entry = "[[entries]]\nwrong = '甲'\nright = '乙'\nanchors = []\n";
        assert_eq!(super::terms(entry)?.len(), 1);
        for (source, key) in [
            ("".to_owned(), "entries"),
            ("entries = [1]".to_owned(), "entries[0]"),
            (
                entry.replace("wrong = '甲'", "wrong = ''"),
                "entries[0].wrong",
            ),
            (entry.replace("right = '乙'\n", ""), "entries[0].right"),
            (
                entry.replace("anchors = []", "anchors = [1]"),
                "entries[0].anchors",
            ),
        ] {
            let error = super::terms(&source)
                .err()
                .ok_or("expected validation error")?;
            let super::TermResourceError::Invalid { key: actual, .. } = error else {
                return Err("wrong error kind".into());
            };
            assert_eq!(actual, key);
        }
        assert!(matches!(
            super::terms(&entry.repeat(2)),
            Err(super::TermResourceError::DuplicateWrong { entry: 1 })
        ));
        Ok(())
    }

    #[test]
    fn phrases_preserve_file_order_and_literal_content() {
        assert_eq!(
            super::phrases(" \u{1f}# comment\r\n \u{a0}\n二字\u{2028} abc \u{2029} x # y\n二字"),
            ["二字", "abc", "x # y", "二字"]
        );
        assert!(super::phrases("# comment\n \t").is_empty());
    }
}
