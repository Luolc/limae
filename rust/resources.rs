//! Shared specification data read at build time, with no runtime paths.

use crate::config::RuleId;
use crate::text::is_python_whitespace;

pub(crate) const ZERO_ALLOWLIST: &str = include_str!("../spec/wordlists/zh-tell-5-allow.txt");

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
    #[test]
    fn phrases_preserve_file_order_and_literal_content() {
        assert_eq!(
            super::phrases(" \u{1f}# comment\r\n \u{a0}\n二字\u{2028} abc \u{2029} x # y\n二字"),
            ["二字", "abc", "x # y", "二字"]
        );
        assert!(super::phrases("# comment\n \t").is_empty());
    }
}
