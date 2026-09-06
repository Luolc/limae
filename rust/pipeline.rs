//! Document checks and fixed-point fixes for all implemented rules.
//!
//! ```
//! use limae::{config::ResolvedConfig, pipeline::Pipeline};
//!
//! let pipeline = Pipeline::new()?;
//! let config = ResolvedConfig::default();
//! let original = "中（１６GB）";
//! assert_eq!(pipeline.check(original, &config)?.len(), 4);
//! let fixed = pipeline.fix(original, &config)?;
//! assert_eq!(fixed, "中 (16 GB)");
//! assert!(pipeline.check(&fixed, &config)?.is_empty());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::borrow::Cow;
use std::ops::Range;
use thiserror::Error;

use crate::config::{ResolvedConfig, RuleId};
use crate::directives::{DirectiveError, rule_masks};
use crate::markdown::{LineProtection, Markdown};
use crate::rules::{
    spacing::SpacingRules,
    structural::{FragmentContext, StructuralRules},
    tells::{SentenceTells, WordlistError, WordlistTells},
    typography::WidthRules,
    words::{ActiveTerms, TermResourceError, WordRules},
};
use crate::text::{char_at, char_before, snippet};

/// One violation in the checked document, before any fixes.
/// Severity is supplied by [`ResolvedConfig::severity`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding<'text> {
    /// One-based line number in the checker's Python `splitlines()` view.
    pub line: usize,
    pub rule: RuleId,
    pub name: Cow<'static, str>,
    /// UTF-8 byte range within that check line, excluding its line separator.
    /// Zero-width ranges denote insertion boundaries, not scalar columns.
    pub range: Range<usize>,
    /// Original source with up to 12 Unicode scalars on either side of the match.
    pub snippet: &'text str,
}

/// The document pipeline could not initialize its built-in rules.
#[derive(Debug, Error)]
pub enum InitError {
    #[error("cannot compile built-in patterns")]
    Pattern(#[from] regex::Error),
    #[error("cannot initialize wordlist rules")]
    Wordlist(#[from] WordlistError),
    #[error("cannot initialize terminology rules")]
    Terms(#[from] TermResourceError),
}

/// Reusable document checker and fixed-point formatter, with no I/O.
pub struct Pipeline {
    markdown: Markdown,
    width: WidthRules,
    spacing: SpacingRules,
    structural: StructuralRules,
    wordlists: WordlistTells,
    sentences: SentenceTells,
    words: WordRules,
}

impl Pipeline {
    /// Compile built-in patterns and parse embedded resources once for reuse.
    ///
    /// # Errors
    /// Returns the failing rule family's error, preserving its concrete source.
    pub fn new() -> Result<Self, InitError> {
        Ok(Self {
            markdown: Markdown::new()?,
            width: WidthRules::new()?,
            spacing: SpacingRules::new()?,
            structural: StructuralRules::new()?,
            wordlists: WordlistTells::new()?,
            sentences: SentenceTells::new()?,
            words: WordRules::new()?,
        })
    }

    /// Check original text only, in line / RuleId / start order. Equal starts
    /// retain pattern order, including rule 11's before / after directions.
    ///
    /// Lines follow Python `splitlines()`: LF, CR, CRLF, VT, FF, U+001C–U+001E,
    /// U+0085 and U+2028–U+2029. CRLF is one boundary; a terminal separator adds
    /// no empty check line. TAB, U+001F and NBSP are not line separators.
    ///
    /// # Errors
    /// Returns [`DirectiveError`] when an inline directive names an unknown rule.
    pub fn check<'text>(
        &self,
        text: &'text str,
        config: &ResolvedConfig,
    ) -> Result<Vec<Finding<'text>>, DirectiveError> {
        let lines = check_lines(text);
        let protected = self.markdown.protect(&lines);
        let verbatim = protected
            .iter()
            .map(|protection| matches!(protection, LineProtection::Verbatim))
            .collect::<Vec<_>>();
        let masks = rule_masks(&lines, &verbatim)?;
        let mut findings = Vec::new();
        for (i, ((line, protection), mask)) in lines.iter().zip(&protected).zip(&masks).enumerate()
        {
            let line_config = config.without_rules(mask);
            let mut matches = self.width.check_line(line, protection, &line_config);
            matches.extend(self.spacing.check_line(line, protection, &line_config));
            matches.extend(self.structural.check_line(line, protection, &line_config));
            matches.extend(self.wordlists.check_line(line, protection, &line_config));
            matches.extend(self.sentences.check_line(line, protection, &line_config));
            matches.extend(self.words.check_line(line, protection, &line_config));
            matches.sort_by_key(|m| (m.rule, m.range.start));
            findings.extend(matches.into_iter().map(|m| {
                let Some(snippet) = snippet(line, m.range.clone()) else {
                    unreachable!("rule matches must be UTF-8 ranges in the checked line");
                };
                Finding {
                    line: i + 1,
                    rule: m.rule,
                    name: m.name,
                    range: m.range,
                    snippet,
                }
            }));
        }
        Ok(findings)
    }

    /// Fix until the string is unchanged, rescanning Markdown on every pass.
    ///
    /// Each pass splits only LF, retaining CR and final empty fragments, then
    /// rejoins with LF. Each original line selects active terms once, including
    /// evidence in protected text. Protected interiors are copied verbatim;
    /// prose fragments run through terminology, width, prose spacing, then
    /// structural spacing / cleanup with their original Markdown boundaries.
    /// Fixes are independent of the original finding list.
    ///
    /// # Errors
    /// Returns [`DirectiveError`] when an inline directive names an unknown rule.
    pub fn fix(&self, text: &str, config: &ResolvedConfig) -> Result<String, DirectiveError> {
        let mut current = text.to_owned();
        loop {
            let lines: Vec<_> = current.split('\n').collect();
            let protected = self.markdown.protect(&lines);
            let verbatim = protected
                .iter()
                .map(|protection| matches!(protection, LineProtection::Verbatim))
                .collect::<Vec<_>>();
            let masks = rule_masks(&lines, &verbatim)?;
            let fixed = lines
                .iter()
                .zip(&protected)
                .zip(&masks)
                .map(|((line, protection), mask)| {
                    self.fix_line(line, protection, &config.without_rules(mask))
                })
                .collect::<Vec<_>>()
                .join("\n");
            if fixed == current {
                return Ok(fixed);
            }
            current = fixed;
        }
    }

    fn fix_line(&self, line: &str, protection: &LineProtection, config: &ResolvedConfig) -> String {
        let LineProtection::Inline { code, prose } = protection else {
            return line.to_owned();
        };
        let active = self.words.active_terms(line, config);
        let mut ranges: Vec<_> = code
            .iter()
            .map(|range| (range, true))
            .chain(prose.iter().map(|range| (range, false)))
            .collect();
        ranges.sort_by_key(|(range, _)| (range.start, range.end));
        let mut fixed = String::new();
        let mut cursor = 0;
        let mut context = FragmentContext::default();
        for (range, is_code) in ranges {
            context.ends_with_code_opener =
                is_code && range.start > cursor && char_before(line, range.start) == Some('`');
            fixed.push_str(&self.fix_fragment(
                &line[cursor..range.start],
                &active,
                config,
                context,
            ));
            fixed.push_str(&line[range.clone()]);
            cursor = range.end;
            context = FragmentContext {
                starts_with_code_closer: is_code && char_at(line, range.end) == Some('`'),
                ends_with_code_opener: false,
            };
        }
        fixed.push_str(&self.fix_fragment(&line[cursor..], &active, config, context));
        fixed
    }

    fn fix_fragment(
        &self,
        fragment: &str,
        active: &ActiveTerms<'_>,
        config: &ResolvedConfig,
        context: FragmentContext,
    ) -> String {
        let terms = active.fix_fragment(fragment);
        let width = self.width.fix_fragment(&terms, config);
        let spacing = self.spacing.fix_fragment(&width, config);
        self.structural.fix_fragment(&spacing, config, context)
    }
}

fn check_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if matches!(
            ch,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            lines.push(&text[start..index]);
            start = index + ch.len_utf8();
            if ch == '\r' && chars.next_if(|&(_, next)| next == '\n').is_some() {
                start += 1;
            }
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::{Pipeline, check_lines};
    use crate::config::{CliOverrides, resolve};
    use crate::rules::words::WordRules;

    #[test]
    fn check_line_edges_match_splitlines() {
        for (text, expected) in [
            ("", vec![]),
            ("\n", vec![""]),
            ("\r\n", vec![""]),
            ("\r\n\r", vec!["", ""]),
            ("中\n\n", vec!["中", ""]),
            ("中\u{2028}", vec!["中"]),
            ("中\r\n文", vec!["中", "文"]),
        ] {
            assert_eq!(check_lines(text), expected, "{text:?}");
        }
    }

    #[test]
    fn terminology_precedes_width_and_spacing_in_one_pass() -> Result<(), Box<dyn Error>> {
        let root =
            std::env::temp_dir().join(format!("limae-pipeline-order-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("limae.toml"), "enable_experimental = true")?;
        let config = resolve(&root, CliOverrides::default());
        std::fs::remove_dir_all(root)?;
        let config = config?;

        let mut pipeline = Pipeline::new()?;
        pipeline.words = WordRules::from_test_resource(concat!(
            "[[entries]]\n",
            "wrong = '术'\n",
            "right = '（１６GB）'\n",
            "anchors = ['术']\n",
        ))?;
        let line = "术";
        let protection = pipeline.markdown.protect(&[line]);
        assert_eq!(pipeline.fix_line(line, &protection[0], &config), "(16 GB)");
        Ok(())
    }
}
