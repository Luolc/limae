//! Prose spacing rules: zh-typography-3, -4, -5 and -6.
//!
//! Checking uses the original line and its protection. Fixing takes only an
//! unprotected fragment, after width conversion by [`super::typography::WidthRules`].
//! Document splitting, directives and protection rescans belong to the caller.

use regex::Regex;

use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::text::{char_at, char_before, is_cjk};

use super::{LineMatch, insert_spaces};

/// Reusable checker and fixer for the four prose spacing rules only.
pub struct SpacingRules {
    paren_before: Regex,
    paren_after: Regex,
    number_run: Regex,
    number_unit: Regex,
}

impl SpacingRules {
    /// Compile the built-in patterns for reuse across lines.
    ///
    /// # Errors
    /// Returns the regex compilation error if a built-in pattern is invalid.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            paren_before: Regex::new(r"(?:[A-Za-z0-9一-鿿]|\*\*|`|\))\(")?,
            paren_after: Regex::new(r"\)(?:[A-Za-z0-9一-鿿]|\*\*[A-Za-z0-9一-鿿]|`)")?,
            number_run: Regex::new(r"[0-9]+(?:[.,][0-9]+)*")?,
            // The normative ASCII units in spec/rules.md, longest first.
            number_unit: Regex::new(concat!(
                r"[0-9]+(?:kbps|Mbps|Gbps|Tbps|",
                r"KiB|MiB|GiB|TiB|PiB|bps|min|kHz|MHz|GHz|dpi|fps|",
                r"KB|MB|GB|TB|PB|ms|ns|us|Hz|px|pt|kg|mg|km|cm|mm|nm)"
            ))?,
        })
    }

    /// Check the original line with its exact Markdown protection and config.
    ///
    /// Results are ordered by rule then byte position. Rules 3 and 6 consume
    /// matches; rules 4 and 5 have zero-width ranges, retaining both adjacent
    /// boundaries in `用A表示`. The caller applies inline directive exclusions.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let mut found: Vec<_> = self
            .paren_before
            .find_iter(line)
            .filter(|m| !english_token_paren(line, m.end() - 1))
            .map(|m| LineMatch {
                rule: RuleId::ZH_TYPOGRAPHY_3,
                name: "zh-typography-3 no space before (",
                range: m.range(),
            })
            .collect();
        found.extend(self.paren_after.find_iter(line).map(|m| LineMatch {
            rule: RuleId::ZH_TYPOGRAPHY_3,
            name: "zh-typography-3 no space after )",
            range: m.range(),
        }));
        found.extend(self.cjk_boundaries(line, config));
        found.extend(self.number_units(line).map(|m| LineMatch {
            rule: RuleId::ZH_TYPOGRAPHY_6,
            name: "zh-typography-6 no space between number and unit",
            range: m.range(),
        }));
        found.retain(|m| config.is_enabled(m.rule) && !protection.is_exempt(m.range.clone()));
        found.sort_by_key(|m| (m.rule, m.range.start));
        found
    }

    /// Apply spacing stages (3, 4, 5, 6) to an unprotected prose fragment.
    ///
    /// Width conversion must run first. This method does not split Markdown,
    /// parse directives, or rescan protection to reach a document fixed point.
    ///
    /// ```
    /// use limae::config::ResolvedConfig;
    /// use limae::rules::{spacing::SpacingRules, typography::WidthRules};
    ///
    /// let config = ResolvedConfig::default();
    /// let width = WidthRules::new()?.fix_fragment("中（１６GB）用A表示", &config);
    /// let spacing = SpacingRules::new()?;
    /// assert_eq!(spacing.fix_fragment(&width, &config), "中 (16 GB) 用 A 表示");
    /// # Ok::<(), regex::Error>(())
    /// ```
    #[must_use]
    pub fn fix_fragment(&self, fragment: &str, config: &ResolvedConfig) -> String {
        let mut fixed = fragment.to_owned();
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_3) {
            fixed = insert_spaces(
                &fixed,
                self.paren_before
                    .find_iter(&fixed)
                    .filter(|m| !english_token_paren(&fixed, m.end() - 1))
                    .map(|m| m.end() - 1),
            );
            fixed = insert_spaces(
                &fixed,
                self.paren_after.find_iter(&fixed).map(|m| m.start() + 1),
            );
        }
        for rule in [RuleId::ZH_TYPOGRAPHY_4, RuleId::ZH_TYPOGRAPHY_5] {
            if config.is_enabled(rule) {
                fixed = insert_spaces(
                    &fixed,
                    self.cjk_boundaries(&fixed, config)
                        .into_iter()
                        .filter(|m| m.rule == rule)
                        .map(|m| m.range.start),
                );
            }
        }
        if config.is_enabled(RuleId::ZH_TYPOGRAPHY_6) {
            fixed = insert_spaces(
                &fixed,
                self.number_units(&fixed)
                    .map(|m| m.start() + m.as_str().bytes().take_while(u8::is_ascii_digit).count()),
            );
        }
        fixed
    }

    fn cjk_boundaries(&self, line: &str, config: &ResolvedConfig) -> Vec<LineMatch> {
        let skips: Vec<_> = self
            .number_run
            .find_iter(line)
            .filter(|m| char_at(line, m.end()).is_some_and(|c| config.skip_zh_units().contains(c)))
            .flat_map(|m| [m.start(), m.end()])
            .collect();
        let mut found = Vec::new();
        for ((_, left), (start, right)) in line.char_indices().zip(line.char_indices().skip(1)) {
            let ascii = if is_cjk(left) {
                right
            } else if is_cjk(right) {
                left
            } else {
                continue;
            };
            let (rule, name) = if ascii.is_ascii_alphabetic() {
                (
                    RuleId::ZH_TYPOGRAPHY_4,
                    "zh-typography-4 no space between CJK and Latin",
                )
            } else if ascii.is_ascii_digit() && !skips.contains(&start) {
                (
                    RuleId::ZH_TYPOGRAPHY_5,
                    "zh-typography-5 no space between CJK and digit",
                )
            } else {
                continue;
            };
            found.push(LineMatch {
                rule,
                name,
                range: start..start,
            });
        }
        found
    }

    fn number_units<'a>(&'a self, line: &'a str) -> impl Iterator<Item = regex::Match<'a>> {
        self.number_unit.find_iter(line).filter(|m| {
            !char_before(line, m.start()).is_some_and(|c| c.is_ascii_alphanumeric())
                && !char_at(line, m.end()).is_some_and(|c| c.is_ascii_alphanumeric())
        })
    }
}

fn english_token_paren(line: &str, start: usize) -> bool {
    char_before(line, start).is_some_and(|c| c.is_ascii_alphanumeric())
        && char_at(line, start + 1).is_some_and(|c| c.is_ascii_alphanumeric())
}
