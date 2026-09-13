//! Embedded prompt resources for semantic polishing.
//!
//! The Chinese layer is two things concatenated: the hand-written
//! `spec/polish/zh.md` and the lexicon, rendered from `spec/lexicon/zh.toml`
//! by [`lexicon::render`] in its brief shape. The lexicon is what moves the
//! result — the prompt experiment recorded in `docs/tracker.md` (2026-09-12)
//! read 9/20 collected words left in place without it and 0–1/20 with it,
//! while the length of the prose around it made no measurable difference —
//! so it is assembled from the source rather than copied into `zh.md`, where
//! it would drift.
//!
//! Every language layer the binary knows is loaded whenever its script is
//! present; there is no language detection beyond that. Reconsider when a
//! third language lands, not before.

use super::lexicon::{self, Detail, LexiconError};
use crate::text::is_cjk;

const GENERAL: &str = include_str!("../../spec/polish/general.md");
const CHINESE: &str = include_str!("../../spec/polish/zh.md");
const CHINESE_LEXICON: &str = include_str!("../../spec/lexicon/zh.toml");

/// Assemble the general prompt and the input language's distilled layer.
///
/// Chinese is the same narrow CJK range used by the deterministic rules. An
/// input without those characters receives only the general layer.
///
/// # Errors
///
/// The embedded lexicon does not parse. The bytes are fixed at build time
/// and covered by a test, so a failure here is a broken build, not an input.
pub fn assemble(text: &str) -> Result<String, LexiconError> {
    let mut prompt = String::from(GENERAL);
    if text.chars().any(is_cjk) {
        prompt.push('\n');
        prompt.push_str(CHINESE);
        prompt.push('\n');
        prompt.push_str(&lexicon::render(CHINESE_LEXICON, Detail::Brief)?);
    }
    Ok(prompt)
}

#[cfg(test)]
mod tests {
    use super::{CHINESE, CHINESE_LEXICON, Detail, GENERAL, LexiconError, assemble, lexicon};

    #[test]
    fn selects_the_language_layer_from_the_input() -> Result<(), LexiconError> {
        assert_eq!(assemble("An ACME report.")?, GENERAL);
        let chinese = lexicon::render(CHINESE_LEXICON, Detail::Brief)?;
        assert_eq!(
            assemble("ACME 的报告。")?,
            format!("{GENERAL}\n{CHINESE}\n{chinese}")
        );
        assert_eq!(assemble("㐀 is outside the contract.")?, GENERAL);
        Ok(())
    }

    /// The lexicon reaches the prompt as entries, not as a heading alone: a
    /// renderer that parsed the file and printed nothing would pass the
    /// test above.
    #[test]
    fn the_chinese_layer_carries_the_lexicon_entries() -> Result<(), LexiconError> {
        let prompt = assemble("ACME 的报告。")?;
        assert!(prompt.contains("\n## 正本\n"));
        assert!(prompt.contains("- 白："));
        assert!(prompt.contains("- 病："));
        assert!(!prompt.contains("- 解："));
        assert!(!prompt.contains("- 原："));
        Ok(())
    }
}
