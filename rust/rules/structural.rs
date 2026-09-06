//! Structural spacing rules: zh-typography-7, -8, -9 and -11.
//!
//! Checks inspect the original line and A2 protection. Fixes take an unprotected
//! fragment after width and prose spacing, with its code delimiters retained.
//! Document line views, directives and protection rescans belong to the caller.

use std::ops::Range;

use regex::Regex;

use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::text::{char_at, char_before, is_cjk, is_python_whitespace};

use super::{LineMatch, insert_spaces};

/// Code delimiter edges of a prose fragment, derived from the original line's
/// [`LineProtection::Inline`] code interiors, before any fragment edits.
/// A closing run starts at an interior's end; an opening run ends at its start.
/// Only set an edge when the corresponding backtick run belongs to this fragment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FragmentContext {
    pub starts_with_code_closer: bool,
    pub ends_with_code_opener: bool,
}

/// Reusable line checker and fragment fixer for structural spacing only.
pub struct StructuralRules {
    dash: Regex,
    link: Regex,
    space_before: Regex,
    space_after: Regex,
    spaces: Regex,
}

impl StructuralRules {
    /// Compile the built-in patterns for reuse across lines.
    ///
    /// # Errors
    /// Returns the regex compilation error if a built-in pattern is invalid.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            dash: Regex::new("——|⸺")?,
            link: Regex::new(r"\[[^\]]*\]\(")?,
            space_before: Regex::new(r"[^\s\x1c-\x1f—⸺|] +[，。、；：？！]")?,
            space_after: Regex::new(r"[，。、；：？！] +[^\s\x1c-\x1f—⸺|]")?,
            spaces: Regex::new(" +")?,
        })
    }

    /// Check original text, ordering matches by rule then byte position.
    /// Equal starts retain pattern order (before, then after). Each rule 11
    /// direction independently consumes non-overlapping matches before exemption
    /// filtering. Rule 9 is enabled only through the supplied configuration.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let LineProtection::Inline { code, .. } = protection else {
            return Vec::new();
        };
        let mut found = code_matches(line, code);
        for dash in self.dashes(line) {
            if let Some(left) = char_before(line, dash.start()).filter(|&c| dash_neighbor(c)) {
                found.push(LineMatch {
                    rule: RuleId::ZH_TYPOGRAPHY_8,
                    name: "zh-typography-8 no space before dash".into(),
                    range: dash.start() - left.len_utf8()..dash.end(),
                });
            }
            if let Some(right) = char_at(line, dash.end()).filter(|&c| dash_neighbor(c)) {
                found.push(LineMatch {
                    rule: RuleId::ZH_TYPOGRAPHY_8,
                    name: "zh-typography-8 no space after dash".into(),
                    range: dash.start()..dash.end() + right.len_utf8(),
                });
            }
        }
        let mut offset = 0;
        while let Some(link) = self.link.find_at(line, offset) {
            if !char_before(line, link.start()).is_some_and(is_cjk) {
                offset = link.start() + 1;
                continue;
            }
            found.push(LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_9,
                name: "zh-typography-9 no space between CJK and link".into(),
                range: link.range(),
            });
            offset = link.end();
        }
        for (pattern, name) in [
            (
                &self.space_before,
                "zh-typography-11 space before fullwidth punct",
            ),
            (
                &self.space_after,
                "zh-typography-11 space after fullwidth punct",
            ),
        ] {
            found.extend(pattern.find_iter(line).map(|m| LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_11,
                name: name.into(),
                range: m.range(),
            }));
        }
        found.retain(|m| config.is_enabled(m.rule) && !protection.is_exempt(m.range.clone()));
        found.sort_by_key(|m| (m.rule, m.range.start));
        found
    }

    /// Apply stages 7, 8, 9 and 11 to one unprotected prose fragment.
    ///
    /// Run width conversion and prose spacing first. Preserve exempt interiors
    /// separately, deriving context from their boundaries; bare backticks must
    /// not be marked as code delimiters. Rule 11 deletes every locally valid
    /// space run independently of the checker's consuming match count.
    ///
    /// ```
    /// use limae::config::ResolvedConfig;
    /// use limae::rules::structural::{FragmentContext, StructuralRules};
    ///
    /// let rules = StructuralRules::new()?;
    /// let context = FragmentContext {
    ///     starts_with_code_closer: true,
    ///     ends_with_code_opener: true,
    /// };
    /// assert_eq!(rules.fix_fragment("`中——文， 字`", &ResolvedConfig::default(), context),
    ///            "` 中 —— 文，字 `");
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[must_use]
    pub fn fix_fragment(
        &self,
        fragment: &str,
        config: &ResolvedConfig,
        context: FragmentContext,
    ) -> String {
        let mut fixed = fragment.to_owned();
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_7) {
            fixed = fix_code_edges(&fixed, context);
        }
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_8) {
            fixed = insert_spaces(
                &fixed,
                self.dashes(&fixed)
                    .filter(|m| char_before(&fixed, m.start()).is_some_and(dash_neighbor))
                    .map(|m| m.start()),
            );
            fixed = insert_spaces(
                &fixed,
                self.dashes(&fixed)
                    .filter(|m| char_at(&fixed, m.end()).is_some_and(dash_neighbor))
                    .map(|m| m.end()),
            );
        }
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_9) {
            // Fix lookahead can insert at nested openers that a consuming check
            // cannot reuse, for example both brackets in `中[文[字](`.
            fixed = insert_spaces(
                &fixed,
                fixed
                    .char_indices()
                    .filter(|&(i, c)| {
                        c == '['
                            && char_before(&fixed, i).is_some_and(is_cjk)
                            && self.link.find_at(&fixed, i).is_some_and(|m| m.start() == i)
                    })
                    .map(|(i, _)| i),
            );
        }
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_11) {
            fixed = self.fix_punct_spaces(&fixed);
        }
        fixed
    }

    fn fix_punct_spaces(&self, text: &str) -> String {
        let mut fixed = String::new();
        let mut cursor = 0;
        for spaces in self.spaces.find_iter(text) {
            let left = char_before(text, spaces.start());
            let right = char_at(text, spaces.end());
            if (left.is_some_and(fullwidth_punct) && right.is_some_and(punct_neighbor))
                || (left.is_some_and(punct_neighbor) && right.is_some_and(fullwidth_punct))
            {
                fixed.push_str(&text[cursor..spaces.start()]);
                cursor = spaces.end();
            }
        }
        fixed.push_str(&text[cursor..]);
        fixed
    }

    fn dashes<'a>(&'a self, text: &'a str) -> impl Iterator<Item = regex::Match<'a>> {
        self.dash.find_iter(text).filter(|m| {
            m.as_str() == "⸺"
                || (char_before(text, m.start()) != Some('—')
                    && char_at(text, m.end()) != Some('—'))
        })
    }
}

fn code_matches(line: &str, code: &[Range<usize>]) -> Vec<LineMatch> {
    let mut found = Vec::new();
    for interior in code {
        let start = line[..interior.start].trim_end_matches('`').len();
        if start < interior.start && char_before(line, start).is_some_and(is_cjk) {
            found.push(LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_7,
                name: "zh-typography-7 no space before inline code".into(),
                range: start..interior.start,
            });
        }
        let end = line.len() - line[interior.end..].trim_start_matches('`').len();
        if end > interior.end && char_at(line, end).is_some_and(is_cjk) {
            found.push(LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_7,
                name: "zh-typography-7 no space after inline code".into(),
                range: interior.end..end,
            });
        }
    }
    found
}

fn dash_neighbor(ch: char) -> bool {
    !is_python_whitespace(ch) && !matches!(ch, '—' | '⸺')
}

fn punct_neighbor(ch: char) -> bool {
    dash_neighbor(ch) && ch != '|'
}

fn fullwidth_punct(ch: char) -> bool {
    matches!(ch, '，' | '。' | '、' | '；' | '：' | '？' | '！')
}

fn fix_code_edges(text: &str, context: FragmentContext) -> String {
    let mut positions = Vec::new();
    let close_end = text.len() - text.trim_start_matches('`').len();
    if context.starts_with_code_closer
        && close_end > 0
        && char_at(text, close_end).is_some_and(is_cjk)
    {
        positions.push(close_end);
    }
    let open_start = text.trim_end_matches('`').len();
    if context.ends_with_code_opener
        && open_start < text.len()
        && char_before(text, open_start).is_some_and(is_cjk)
    {
        positions.push(open_start);
    }
    insert_spaces(text, positions.into_iter())
}
