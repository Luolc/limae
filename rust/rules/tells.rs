//! Wordlist tells, sentence shapes and zero-noun coinages on original lines.
//! Document orchestration remains a separate consumer.

use regex::Regex;
use std::cmp::Reverse;
use thiserror::Error;

use super::LineMatch;
use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::resources::{TELL_WORDLISTS, ZERO_ALLOWLIST, phrases};
use crate::text::{char_at, char_before, is_cjk};

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

/// Reusable zh-tell-2, en-tell-2 and zh-tell-5 checker, without fixes.
pub struct SentenceTells {
    chinese: Regex,
    english: [(Regex, Regex); 2],
    allowed_zero: Vec<&'static str>,
}

impl SentenceTells {
    /// Compile sentence patterns and parse the embedded zero-noun allowlist.
    ///
    /// # Errors
    /// Returns the compilation error of an invalid built-in sentence pattern.
    pub fn new() -> Result<Self, regex::Error> {
        // Only literal I tokens are expanded. The other ASCII equivalents use
        // regex's case folding; Python whitespace additionally includes 1C–1F.
        let english = |body: &str| Regex::new(&format!("(?i:({body}))(?:[^A-Za-z0-9_İıſK]|$)"));
        Ok(Self {
            chinese: Regex::new("不是.{0,20}?而是")?,
            english: [
                (english(r"not[\s\x1c-\x1f]+just")?, english(r"\Abut")?),
                (
                    english(concat!(
                        r"(?:(?:[iİı]t|that)['’]s|they['’]re|[iİı]s|are|was|were)",
                        r"[\s\x1c-\x1f]+not|(?:[iİı]s|are|was|were)n['’]t"
                    ))?,
                    english(concat!(
                        r"\A(?:(?:[iİı]t|that)['’]s|they['’]re",
                        r"|(?:[iİı]t|that)[\s\x1c-\x1f]+[iİı]s|they[\s\x1c-\x1f]+are)"
                    ))?,
                ),
            ],
            allowed_zero: phrases(ZERO_ALLOWLIST),
        })
    }

    /// Check these three rules in RuleId / start order on an unmodified line.
    ///
    /// Each sentence pattern consumes non-overlapping matches independently,
    /// including protected matches. Every zero is judged against the remainder
    /// of its CJK run; allowlist evidence uses the entire original line.
    /// Protection must belong to this line. Splitting and directives belong to
    /// the caller; this primitive is not integrated into `Typography`.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let mut found = Vec::new();
        if config.is_enabled(RuleId::ZH_TELL_2) {
            for matched in self.chinese.find_iter(line) {
                if !protection.is_exempt(matched.range()) {
                    found.push(LineMatch {
                        rule: RuleId::ZH_TELL_2,
                        name: "zh-tell-2 negative parallelism",
                        range: matched.range(),
                    });
                }
            }
        }
        if config.is_enabled(RuleId::EN_TELL_2) {
            for (opening, ending) in &self.english {
                found.extend(english_sentences(line, protection, opening, ending));
            }
        }
        if config.is_enabled(RuleId::ZH_TELL_5) {
            for (start, zero) in line.match_indices('零') {
                let length = line[start..].chars().take_while(|&ch| is_cjk(ch)).count();
                let range = start..start + zero.len();
                if (2..=5).contains(&length)
                    && !covered(line, start, &self.allowed_zero)
                    && !protection.is_exempt(range.clone())
                {
                    found.push(LineMatch {
                        rule: RuleId::ZH_TELL_5,
                        name: "zh-tell-5 zero-noun coinage",
                        range,
                    });
                }
            }
        }
        found.sort_by_key(|m| (m.rule, m.range.start));
        found
    }
}

fn english_sentences(
    line: &str,
    protection: &LineProtection,
    opening: &Regex,
    ending: &Regex,
) -> Vec<LineMatch> {
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(captures) = opening.captures_at(line, offset) {
        let Some(start) = captures.get(1) else {
            unreachable!("sentence patterns always capture the keyword match");
        };
        // Invalid boundaries or a missing ending allow overlapping openers.
        offset = start.start() + char_at(line, start.start()).map_or(0, char::len_utf8);
        if char_before(line, start.start()).is_some_and(english_word_char) {
            continue;
        }
        // Try at most 41 ending positions, from a zero- to a 40-scalar gap.
        // The slice extends to the real line end so right boundaries stay real.
        for (gap, _) in line[start.end()..]
            .char_indices()
            .take(41)
            .take_while(|&(_, ch)| ch != '\n')
        {
            let position = start.end() + gap;
            if char_before(line, position).is_some_and(english_word_char) {
                continue;
            }
            let Some(end) = ending.captures(&line[position..]).and_then(|c| c.get(1)) else {
                continue;
            };
            offset = position + end.end();
            let range = start.start()..offset;
            if !protection.is_exempt(range.clone()) {
                found.push(LineMatch {
                    rule: RuleId::EN_TELL_2,
                    name: "en-tell-2 English negative parallelism",
                    range,
                });
            }
            // Protection consumes the complete legal sentence, like finditer.
            break;
        }
    }
    found
}

fn covered(line: &str, start: usize, allowed: &[&str]) -> bool {
    allowed.iter().any(|word| {
        // Every character boundary may begin an occurrence, even when two
        // allowlist occurrences overlap. Only the hit's first character counts.
        std::iter::once(start)
            .chain(
                line[..start]
                    .char_indices()
                    .rev()
                    .map(|(position, _)| position),
            )
            .take_while(|&position| start - position < word.len())
            .any(|position| line[position..].starts_with(word))
    })
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn allowlist_coverage_includes_overlapping_occurrences_and_only_the_hit_start() {
        assert!(super::covered("零零零", "零零".len(), &["零零"]));
        assert!(super::covered("从零秘密", "从".len(), &["从零"]));
        assert!(!super::covered("零零", "零".len(), &["零甲"]));
        assert!(!super::covered("零售 零秘密", "零售 ".len(), &["零售"]));
    }

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
