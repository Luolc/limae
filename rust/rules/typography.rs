//! Width conversion rules: zh-typography-1, zh-typography-2 and zh-typography-10.
//!
//! These are line/fragment primitives, not a document lint or format pipeline.
//! The caller supplies protection for the exact original line when checking,
//! and passes only unprotected prose fragments when fixing. Inline directive
//! parsing, line splitting and repeated protection scans belong to the caller.

use regex::Regex;

use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::text::{char_at, char_before, halfwidth_digit, is_cjk};

use super::LineMatch;

/// Reusable checker and fixer for the three width conversion rules only.
pub struct WidthRules {
    punctuation: Regex,
    abbreviation: Regex,
}

impl WidthRules {
    /// Compile the built-in patterns for reuse across lines.
    ///
    /// # Errors
    /// Returns the regex compilation error if a built-in pattern is invalid.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            punctuation: Regex::new(r"[一-鿿][,;:?!]|[,;:?!][一-鿿]")?,
            abbreviation: Regex::new(
                r"e\.g\.|i\.e\.|etc\.|cf\.|vs\.|Mr\.|Mrs\.|Ms\.|Dr\.|Prof\.|St\.",
            )?,
        })
    }

    /// Check width rules on the original line using its Markdown protection.
    ///
    /// Results use line-local byte ranges and are ordered by rule then position.
    /// Other rule families/settings are outside this primitive's scope. The
    /// caller must apply any inline directive exclusions before using results.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let mut found: Vec<_> = self
            .punctuation
            .find_iter(line)
            .map(|matched| LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_1,
                name: "zh-typography-1 halfwidth punct next to CJK",
                range: matched.range(),
            })
            .collect();
        let abbrev = self.abbreviation_dots(line);
        for (start, ch) in line.char_indices() {
            let (rule, name) = match ch {
                '.' if period_needs_width(line, start, &abbrev) => (
                    RuleId::ZH_TYPOGRAPHY_1,
                    "zh-typography-1 halfwidth period next to CJK",
                ),
                '（' | '）' => (RuleId::ZH_TYPOGRAPHY_2, "zh-typography-2 fullwidth paren"),
                '０'..='９' => (RuleId::ZH_TYPOGRAPHY_10, "zh-typography-10 fullwidth digit"),
                _ => continue,
            };
            found.push(LineMatch {
                rule,
                name,
                range: start..start + ch.len_utf8(),
            });
        }
        found.retain(|matched| {
            config.is_enabled(matched.rule) && !protection.is_exempt(matched.range.clone())
        });
        found.sort_by_key(|matched| (matched.rule, matched.range.start));
        found
    }

    /// Apply width conversion stages (10, 2, 1) to one unprotected prose fragment.
    ///
    /// This does not parse Markdown, process directives or add/remove spacing.
    /// The caller must split protected interiors out before calling this method.
    /// Fixes scan characters independently of the checker's non-overlapping
    /// consuming matches, so `,中,` reports once but fixes both commas.
    ///
    /// ```
    /// use limae::config::ResolvedConfig;
    /// use limae::rules::typography::WidthRules;
    ///
    /// let rules = WidthRules::new()?;
    /// let config = ResolvedConfig::default();
    /// assert_eq!(rules.fix_fragment("中,文,字（２０１１年）", &config), "中，文，字(2011年)");
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[must_use]
    pub fn fix_fragment(&self, fragment: &str, config: &ResolvedConfig) -> String {
        let converted: String = fragment
            .chars()
            .map(|ch| {
                if config.is_enabled(RuleId::ZH_TYPOGRAPHY_10)
                    && let Some(digit) = halfwidth_digit(ch)
                {
                    return digit;
                }
                match ch {
                    '（' if config.is_enabled(RuleId::ZH_TYPOGRAPHY_2) => '(',
                    '）' if config.is_enabled(RuleId::ZH_TYPOGRAPHY_2) => ')',
                    _ => ch,
                }
            })
            .collect();
        if !config.is_enabled(RuleId::ZH_TYPOGRAPHY_1) {
            return converted;
        }
        let abbrev = self.abbreviation_dots(&converted);
        converted
            .char_indices()
            .map(|(start, ch)| {
                if char_before(&converted, start).is_some_and(is_cjk)
                    || char_at(&converted, start + ch.len_utf8()).is_some_and(is_cjk)
                {
                    match ch {
                        ',' => return '，',
                        ';' => return '；',
                        ':' => return '：',
                        '?' => return '？',
                        '!' => return '！',
                        '.' if period_needs_width(&converted, start, &abbrev) => return '。',
                        _ => {}
                    }
                }
                ch
            })
            .collect()
    }

    fn abbreviation_dots(&self, line: &str) -> Vec<usize> {
        self.abbreviation
            .find_iter(line)
            .filter(|matched| {
                !char_before(line, matched.start()).is_some_and(|ch| ch.is_ascii_alphabetic())
            })
            .flat_map(|matched| {
                matched
                    .as_str()
                    .match_indices('.')
                    .map(move |(offset, _)| matched.start() + offset)
            })
            .collect()
    }
}

fn period_needs_width(line: &str, start: usize, abbreviation_dots: &[usize]) -> bool {
    let previous = char_before(line, start);
    let next = char_at(line, start + 1);
    (previous.is_some_and(is_cjk) || next.is_some_and(is_cjk))
        && previous != Some('.')
        && !next.is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '.')
        && !abbreviation_dots.contains(&start)
}
