//! Terminology and misused-word checks on original lines, with prose fixes.

use super::{LineMatch, covered};
use crate::config::{ResolvedConfig, RuleId};
use crate::markdown::LineProtection;
use crate::resources::{SECRET_ALLOWLIST, TERMS, Term, phrases, terms};

pub use crate::resources::TermResourceError;

/// Reusable zh-word-1/2 line checker backed by embedded specification data.
pub struct WordRules {
    terms: Vec<Term>,
    allowed_secret: Vec<&'static str>,
}

impl WordRules {
    /// Parse the embedded terminology TOML and misused-secret allowlist.
    ///
    /// # Errors
    /// Returns a named [`TermResourceError`] for invalid terminology data,
    /// retaining the TOML cause without embedding the resource's source text.
    pub fn new() -> Result<Self, TermResourceError> {
        Ok(Self {
            terms: terms(TERMS)?,
            allowed_secret: phrases(SECRET_ALLOWLIST),
        })
    }

    /// Check both rules in RuleId / start order on the unmodified line.
    ///
    /// Anchors and allowlist evidence use the entire line, including protected
    /// interiors. Each wrong-word occurrence is still subject to protection,
    /// which must belong to this line. The caller handles splitting/directives;
    /// these primitives are not integrated into `Typography`.
    #[must_use]
    pub fn check_line(
        &self,
        line: &str,
        protection: &LineProtection,
        config: &ResolvedConfig,
    ) -> Vec<LineMatch> {
        let mut found = Vec::new();
        for term in self.active_terms(line, config).terms {
            for (start, wrong) in line.match_indices(term.wrong.as_str()) {
                let range = start..start + wrong.len();
                if !protection.is_exempt(range.clone()) {
                    found.push(LineMatch {
                        rule: RuleId::ZH_WORD_1,
                        name: format!("zh-word-1 term {} -> {}", term.wrong, term.right).into(),
                        range,
                    });
                }
            }
        }
        if config.is_enabled(RuleId::ZH_WORD_2) {
            for (start, secret) in line.match_indices("秘密") {
                let range = start..start + secret.len();
                if !covered(line, start, &self.allowed_secret)
                    && !protection.is_exempt(range.clone())
                {
                    found.push(LineMatch {
                        rule: RuleId::ZH_WORD_2,
                        name: "zh-word-2 misused 秘密".into(),
                        range,
                    });
                }
            }
        }
        found.sort_by_key(|m| (m.rule, m.range.start));
        found
    }

    /// Select zh-word-1 entries using Unicode lowercase substrings in the whole
    /// original line, before splitting prose or applying any replacements.
    /// Reuse this context for every prose fragment of that line in this pass.
    #[must_use]
    pub fn active_terms(&self, line: &str, config: &ResolvedConfig) -> ActiveTerms<'_> {
        let mut active = Vec::new();
        if config.is_enabled(RuleId::ZH_WORD_1) {
            let lowered = line.to_lowercase();
            active.extend(
                self.terms
                    .iter()
                    .filter(|term| term.anchors.iter().any(|anchor| lowered.contains(anchor))),
            );
        }
        ActiveTerms { terms: active }
    }
}

/// The entries enabled by the original line's anchors and configuration.
pub struct ActiveTerms<'rules> {
    terms: Vec<&'rules Term>,
}

impl ActiveTerms<'_> {
    /// Replace literal wrong terms in specification order in one prose fragment.
    ///
    /// The caller splits out protected interiors using [`crate::markdown`] and
    /// copies them verbatim. Run this stage before every typography stage; reuse
    /// the same original-line context so replacements cannot change anchors for
    /// later entries or fragments. zh-word-2 has no unique fix and is check-only.
    #[must_use]
    pub fn fix_fragment(&self, fragment: &str) -> String {
        let mut fixed = fragment.to_owned();
        for term in &self.terms {
            fixed = fixed.replace(&term.wrong, &term.right);
        }
        fixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CliOverrides, resolve};

    #[test]
    fn synthetic_terms_freeze_anchors_and_preserve_literal_overlap_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("limae-word-order-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("limae.toml"), "enable_experimental = true")?;
        let config = resolve(&root, CliOverrides::default());
        std::fs::remove_dir_all(root)?;
        let config = config?;
        let rules = WordRules {
            terms: terms(concat!(
                "[[entries]]\nwrong='引'\nright='现'\nanchors=['引']\n",
                "[[entries]]\nwrong='术'\nright='译'\nanchors=['引']\n",
                "[[entries]]\nwrong='名'\nright='文'\nanchors=['现']\n",
                "[[entries]]\nwrong='空'\nright='无'\nanchors=[]\n",
                "[[entries]]\nwrong='甲甲'\nright='乙'\nanchors=['TOKEN']\n",
                "[[entries]]\nwrong='甲'\nright='丙'\nanchors=['token']\n",
                "[[entries]]\nwrong='乙'\nright='丁'\nanchors=['token']\n",
            ))?,
            allowed_secret: vec!["守秘", "秘密事", "秘秘"],
        };
        let protection = LineProtection::Inline {
            code: vec![],
            prose: vec![],
        };
        let line = "引术名空";
        let active = rules.active_terms(line, &config);
        assert_eq!(active.fix_fragment(line), "现译名空");
        assert_eq!(active.fix_fragment("术名空"), "译名空");
        let found = rules.check_line(line, &protection, &config);
        assert_eq!(found.len(), 2);
        let line = "甲甲甲 token";
        let found = rules.check_line(line, &protection, &config);
        assert_eq!(
            found
                .iter()
                .map(|m| (m.name.as_ref(), m.range.clone()))
                .collect::<Vec<_>>(),
            [
                ("zh-word-1 term 甲甲 -> 乙", 0..6),
                ("zh-word-1 term 甲 -> 丙", 0..3),
                ("zh-word-1 term 甲 -> 丙", 3..6),
                ("zh-word-1 term 甲 -> 丙", 6..9),
            ]
        );
        assert_eq!(
            rules.active_terms(line, &config).fix_fragment(line),
            "丁丙 token"
        );
        // Synthetic allowlist entries exercise both sides, overlapping coverage
        // and coverage of just the first character of the two-character hit.
        let line = "守秘密 秘密事 秘秘秘密 秘密";
        let found = rules.check_line(line, &protection, &config);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].range, line.len() - 6..line.len());
        Ok(())
    }
}
