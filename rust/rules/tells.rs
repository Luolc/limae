//! Five wordlist tell rules, each reporting at most once per original line.
//!
//! Sentence shapes, coinages and document orchestration are separate consumers.

use regex::Regex;
use std::cmp::Reverse;
use thiserror::Error;

use super::LineMatch;
use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::resources::{TELL_WORDLISTS, phrases};
use crate::text::{char_at, char_before};

/// A built-in wordlist could not be compiled; initialization never drops it.
#[derive(Debug, Error)]
#[error("cannot compile spec/wordlists/{rule}.txt: {source}")]
pub struct WordlistError {
    pub rule: RuleId,
    #[source]
    pub source: regex::Error,
}

/// Reusable zh-tell-1/3/4 and en-tell-1/3 line checker, without fixes.
pub struct WordlistTells {
    patterns: Vec<(RuleId, &'static str, Regex)>,
}

impl WordlistTells {
    /// Parse and compile the five embedded wordlists in specification order.
    ///
    /// # Errors
    /// Returns a [`WordlistError`] naming the resource and retaining its cause.
    pub fn new() -> Result<Self, WordlistError> {
        let patterns = TELL_WORDLISTS
            .into_iter()
            .map(|(rule, name, text)| {
                compile(text, english(rule))
                    .map(|pattern| (rule, name, pattern))
                    .map_err(|source| WordlistError { rule, source })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { patterns })
    }

    /// Check only these five rules, in rule order, on the unmodified line.
    ///
    /// Protection must belong to this line. Exempt matches still consume their
    /// text before searching for the next occurrence. The caller handles line
    /// splitting and directive masks; configuration defaults keep these off.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let mut found = Vec::new();
        for (rule, name, pattern) in &self.patterns {
            if !config.is_enabled(*rule) {
                continue;
            }
            let mut offset = 0;
            while let Some(captures) = pattern.captures_at(line, offset) {
                let Some(matched) = captures.get(1) else {
                    unreachable!("wordlist patterns always capture the literal match");
                };
                if english(*rule)
                    && char_before(line, matched.start()).is_some_and(english_word_char)
                {
                    offset =
                        matched.start() + char_at(line, matched.start()).map_or(0, char::len_utf8);
                    continue;
                }
                // The right boundary participates in alternative selection, but
                // belongs to the next search and never to protection or findings.
                offset = matched.end();
                if !protection.is_exempt(matched.range()) {
                    found.push(LineMatch {
                        rule: *rule,
                        name,
                        range: matched.range(),
                    });
                    break;
                }
            }
        }
        found
    }
}

fn english(rule: RuleId) -> bool {
    matches!(rule, RuleId::EN_TELL_1 | RuleId::EN_TELL_3)
}

fn english_word_char(ch: char) -> bool {
    // spec/rules.md's wordlist contract adds U+0130/U+0131, U+017F and U+212A
    // to the ASCII-letter equivalence classes.
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | 'İ' | 'ı' | 'ſ' | 'K')
}

fn compile(text: &str, english: bool) -> Result<Regex, regex::Error> {
    let mut listed = phrases(text);
    listed.sort_by_key(|phrase| Reverse(phrase.chars().count()));
    let mut alternatives = listed
        .iter()
        .map(|phrase| {
            let escaped = regex::escape(phrase);
            // The wordlist contract in spec/rules.md unifies all four I forms;
            // regex already handles ASCII case, long S and the Kelvin sign.
            if english {
                escaped.replace(['i', 'I'], "[iIİı]")
            } else {
                escaped
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    if alternatives.is_empty() {
        alternatives = "[a&&b]".to_owned();
    }
    let pattern = if english {
        format!("(?i:({alternatives}))(?:[^A-Za-z0-9_İıſK]|$)")
    } else {
        format!("({alternatives})")
    };
    Regex::new(&pattern)
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn rejected_left_boundary_and_protection_consume_different_ranges()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::config::{CliOverrides, RuleId, resolve};
        use crate::markdown::LineProtection;
        let root = std::env::temp_dir().join(format!("limae-tells-overlap-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("limae.toml"), "enable_experimental = true")?;
        let config = resolve(&root, CliOverrides::default());
        std::fs::remove_dir_all(root)?;
        let config = config?;
        let checker = super::WordlistTells {
            patterns: vec![(
                RuleId::EN_TELL_1,
                "synthetic overlap",
                compile("a-a", true)?,
            )],
        };
        for (line, span, expected) in [("za-a-a", 0..0, Some(3..6)), ("a-a-a", 0..1, None)] {
            let protection = LineProtection::Inline {
                code: vec![span],
                prose: vec![],
            };
            let found = checker.check_line(line, &protection, &config);
            assert_eq!(found.first().map(|m| m.range.clone()), expected, "{line}");
        }
        Ok(())
    }

    #[test]
    fn alternatives_are_literal_longest_first_and_boundary_aware() -> Result<(), regex::Error> {
        let chinese = compile("甲\n甲乙\na.b", false)?;
        assert_eq!(
            chinese.find("甲乙 a.b axb").map(|m| m.as_str()),
            Some("甲乙")
        );
        assert!(!chinese.is_match("axb"));
        let english = compile("foo\nfoo-bar\nbar\nink", true)?;
        for (text, expected) in [("foo-bar", "foo-bar"), ("foo-barx", "foo"), ("İnK", "İnK")]
        {
            assert_eq!(
                english
                    .captures(text)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str()),
                Some(expected)
            );
        }
        assert!(!compile("# empty\n", true)?.is_match("anything"));
        Ok(())
    }
}
