//! Embedded prompt resources for semantic polishing.

use crate::text::is_cjk;

const GENERAL: &str = include_str!("../../spec/polish/general.md");
const CHINESE: &str = include_str!("../../spec/polish/zh.md");

/// Assemble the general prompt and the input language's distilled layer.
///
/// Chinese is the same narrow CJK range used by the deterministic rules. An
/// input without those characters receives only the general layer.
#[must_use]
pub fn assemble(text: &str) -> String {
    let mut prompt = String::from(GENERAL);
    if text.chars().any(is_cjk) {
        prompt.push('\n');
        prompt.push_str(CHINESE);
    }
    prompt
}

#[cfg(test)]
mod tests {
    use super::{CHINESE, GENERAL, assemble};

    #[test]
    fn selects_the_language_layer_from_the_input() {
        assert_eq!(assemble("An ACME report."), GENERAL);
        assert_eq!(assemble("ACME 的报告。"), format!("{GENERAL}\n{CHINESE}"));
        assert_eq!(assemble("㐀 is outside the contract."), GENERAL);
    }
}
