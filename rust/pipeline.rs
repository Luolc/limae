//! Pure typography document processing (zh-typography-1 through -11).
//!
//! This API applies only the typography settings in the supplied configuration,
//! including when experimental rules are enabled. Experimental rules (A4) and
//! inline directives (A6) require subsequent integration; this is not the full
//! linter or a replacement for Python's `check_text` / `fix_text`.
//!
//! ```
//! use limae::{config::ResolvedConfig, pipeline::Typography};
//!
//! let typography = Typography::new()?;
//! let config = ResolvedConfig::default();
//! let original = "中（１６GB）";
//! assert_eq!(typography.check(original, &config).len(), 4);
//! let fixed = typography.fix(original, &config);
//! assert_eq!(fixed, "中 (16 GB)");
//! assert!(typography.check(&fixed, &config).is_empty());
//! # Ok::<(), regex::Error>(())
//! ```

use std::ops::Range;

use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::{LineProtection, Markdown};
use crate::rules::{
    spacing::SpacingRules,
    structural::{FragmentContext, StructuralRules},
    typography::WidthRules,
};
use crate::text::{char_at, char_before, snippet};

/// One typography violation in the original document, before any fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypographyFinding<'text> {
    /// One-based line number in the checker's Python `splitlines()` view.
    pub line: usize,
    pub rule: RuleId,
    pub name: &'static str,
    /// UTF-8 byte range within that check line, excluding its line separator.
    /// Zero-width ranges denote insertion boundaries, not scalar columns.
    pub range: Range<usize>,
    /// Original source with up to 12 Unicode scalars on either side of the match.
    pub snippet: &'text str,
}

/// Reusable typography checker and fixed-point formatter, with no I/O.
pub struct Typography {
    markdown: Markdown,
    width: WidthRules,
    spacing: SpacingRules,
    structural: StructuralRules,
}

impl Typography {
    /// Compile built-in patterns once for reuse across documents.
    ///
    /// # Errors
    /// Returns the concrete regex compilation error for an invalid pattern.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            markdown: Markdown::new()?,
            width: WidthRules::new()?,
            spacing: SpacingRules::new()?,
            structural: StructuralRules::new()?,
        })
    }

    /// Check original text only, in line / RuleId / start order. Equal starts
    /// retain pattern order, including rule 11's before / after directions.
    ///
    /// Lines follow Python `splitlines()`: LF, CR, CRLF, VT, FF, U+001C–U+001E,
    /// U+0085 and U+2028–U+2029. CRLF is one boundary; a terminal separator adds
    /// no empty check line. TAB, U+001F and NBSP are not line separators.
    #[must_use]
    pub fn check<'text>(
        &self,
        text: &'text str,
        config: &ResolvedConfig,
    ) -> Vec<TypographyFinding<'text>> {
        let lines = check_lines(text);
        let protected = self.markdown.protect(&lines);
        let mut findings = Vec::new();
        for (i, (line, protection)) in lines.iter().zip(&protected).enumerate() {
            let mut matches = self.width.check_line(line, protection, config);
            matches.extend(self.spacing.check_line(line, protection, config));
            matches.extend(self.structural.check_line(line, protection, config));
            matches.sort_by_key(|m| (m.rule, m.range.start));
            findings.extend(matches.into_iter().map(|m| {
                let Some(snippet) = snippet(line, m.range.clone()) else {
                    unreachable!("rule matches must be UTF-8 ranges in the checked line");
                };
                TypographyFinding {
                    line: i + 1,
                    rule: m.rule,
                    name: m.name,
                    range: m.range,
                    snippet,
                }
            }));
        }
        findings
    }

    /// Fix until the string is unchanged, rescanning Markdown on every pass.
    ///
    /// Each pass splits only LF, retaining CR and final empty fragments, then
    /// rejoins with LF. Protected interiors are copied verbatim; prose fragments
    /// run through width, prose spacing, then structural spacing / cleanup.
    /// Fixes are independent of the original finding list.
    #[must_use]
    pub fn fix(&self, text: &str, config: &ResolvedConfig) -> String {
        let mut current = text.to_owned();
        loop {
            let lines: Vec<_> = current.split('\n').collect();
            let protected = self.markdown.protect(&lines);
            let fixed = lines
                .iter()
                .zip(&protected)
                .map(|(line, protection)| self.fix_line(line, protection, config))
                .collect::<Vec<_>>()
                .join("\n");
            if fixed == current {
                return fixed;
            }
            current = fixed;
        }
    }

    fn fix_line(&self, line: &str, protection: &LineProtection, config: &ResolvedConfig) -> String {
        let LineProtection::Inline { code, prose } = protection else {
            return line.to_owned();
        };
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
            fixed.push_str(&self.fix_fragment(&line[cursor..range.start], config, context));
            fixed.push_str(&line[range.clone()]);
            cursor = range.end;
            context = FragmentContext {
                starts_with_code_closer: is_code && char_at(line, range.end) == Some('`'),
                ends_with_code_opener: false,
            };
        }
        fixed.push_str(&self.fix_fragment(&line[cursor..], config, context));
        fixed
    }

    fn fix_fragment(
        &self,
        fragment: &str,
        config: &ResolvedConfig,
        context: FragmentContext,
    ) -> String {
        let width = self.width.fix_fragment(fragment, config);
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
    use super::check_lines;

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
}
