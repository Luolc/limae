//! Inline `limae-disable` directives from `spec/rules.md`.
//!
//! A directive is an HTML comment alone on its line. Persistent disable and
//! enable directives update a running off-set; disable-next-line contributes a
//! one-line pending set. Every directive line masks all rules itself.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::config::RuleId;
use crate::text::is_python_whitespace;

pub(crate) type RuleMask = BTreeSet<RuleId>;

/// An inline directive named at least one unknown rule id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectiveError {
    line: usize,
    unknown: Vec<String>,
}

impl DirectiveError {
    /// Return the one-based line number in the pipeline's current line view.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Return unknown ids in their source order, including duplicates.
    #[must_use]
    pub fn unknown_ids(&self) -> &[String] {
        &self.unknown
    }
}

impl fmt::Display for DirectiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: unknown rule id(s) {}; known ids are {}",
            self.line,
            self.unknown.join(", "),
            RuleId::all()
                .map(RuleId::as_str)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl Error for DirectiveError {}

#[derive(Clone, Copy)]
enum DirectiveKind {
    DisableNextLine,
    Disable,
    Enable,
}

pub(crate) fn rule_masks(
    lines: &[&str],
    verbatim: &[bool],
) -> Result<Vec<RuleMask>, DirectiveError> {
    assert_eq!(lines.len(), verbatim.len(), "line protection must align");
    let everything: RuleMask = RuleId::all().collect();
    let mut masks = Vec::with_capacity(lines.len());
    let mut off = RuleMask::new();
    let mut pending = RuleMask::new();

    for (index, (&line, &is_verbatim)) in lines.iter().zip(verbatim).enumerate() {
        if !is_verbatim && let Some(directive) = parse(line, index + 1) {
            let (kind, ids) = directive?;
            pending.clear();
            match kind {
                DirectiveKind::DisableNextLine => pending = ids,
                DirectiveKind::Disable => off.extend(ids),
                DirectiveKind::Enable => off.retain(|rule| !ids.contains(rule)),
            }
            masks.push(everything.clone());
            continue;
        }

        let mut mask = off.clone();
        mask.append(&mut pending);
        masks.push(mask);
    }
    Ok(masks)
}

fn parse(
    line: &str,
    line_number: usize,
) -> Option<Result<(DirectiveKind, RuleMask), DirectiveError>> {
    let source = line.trim_matches(is_python_whitespace);
    let body = source
        .strip_prefix("<!--")?
        .trim_start_matches(is_python_whitespace)
        .strip_prefix("limae-")?;
    let (kind, rest) = [
        (DirectiveKind::DisableNextLine, "disable-next-line"),
        (DirectiveKind::Disable, "disable"),
        (DirectiveKind::Enable, "enable"),
    ]
    .into_iter()
    .find_map(|(kind, name)| body.strip_prefix(name).map(|rest| (kind, rest)))?;

    let listed = if rest == "-->" {
        None
    } else {
        let first = rest.chars().next()?;
        if !is_python_whitespace(first) {
            return None;
        }
        let listed = rest.strip_suffix("-->")?;
        if listed.contains('>') {
            return None;
        }
        Some(listed.trim_matches(is_python_whitespace))
    };
    Some(ids(listed, line_number).map(|ids| (kind, ids)))
}

fn ids(listed: Option<&str>, line: usize) -> Result<RuleMask, DirectiveError> {
    let Some(listed) = listed.filter(|listed| !listed.is_empty()) else {
        return Ok(RuleId::all().collect());
    };
    let mut ids = RuleMask::new();
    let mut unknown = Vec::new();
    for name in listed
        .split(|ch| ch == ',' || is_python_whitespace(ch))
        .filter(|name| !name.is_empty())
    {
        if let Some(rule) = RuleId::from_name(name) {
            ids.insert(rule);
        } else {
            unknown.push(name.to_owned());
        }
    }
    if unknown.is_empty() {
        Ok(ids)
    } else {
        Err(DirectiveError { line, unknown })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(mask: &RuleMask) -> Vec<&'static str> {
        mask.iter().copied().map(RuleId::as_str).collect()
    }

    #[test]
    fn empty_and_separator_only_lists_remain_distinct() -> Result<(), DirectiveError> {
        for source in [
            "<!-- limae-disable -->",
            "<!-- limae-disable    -->",
            "\u{a0}<!--\u{2003}limae-disable\u{3000}-->\u{202f}",
        ] {
            let masks = rule_masks(&[source, "中A"], &[false, false])?;
            assert_eq!(masks[0].len(), 21);
            assert_eq!(masks[1].len(), 21);
        }
        for source in ["<!-- limae-disable , -->", "<!-- limae-disable , , -->"] {
            let masks = rule_masks(&[source, "中A"], &[false, false])?;
            assert_eq!(masks[0].len(), 21);
            assert!(masks[1].is_empty());
        }
        Ok(())
    }

    #[test]
    fn persistent_and_pending_state_follow_line_boundaries() -> Result<(), DirectiveError> {
        let lines = [
            "<!-- limae-disable zh-typography-4 -->",
            "中A",
            "<!-- limae-disable-next-line zh-typography-1 -->",
            "<!-- limae-enable zh-typography-4 -->",
            "中,A",
            "<!-- limae-disable-next-line -->",
            "",
            "中A",
            "<!-- limae-disable-next-line zh-typography-4 -->",
        ];
        let masks = rule_masks(&lines, &[false; 9])?;
        assert_eq!(names(&masks[1]), ["zh-typography-4"]);
        assert!(masks[4].is_empty());
        assert_eq!(masks[6].len(), 21);
        assert!(masks[7].is_empty());
        assert_eq!(masks[8].len(), 21);
        Ok(())
    }

    #[test]
    fn verbatim_and_similar_comments_are_ordinary_lines() -> Result<(), DirectiveError> {
        let lines = [
            "<!-- limae-disable-next-line zh-typography-4 -->",
            "<!-- limae-disable unknown -->",
            "中A",
            "<!-- limae-disabled -->",
            "前文 <!-- limae-disable -->",
        ];
        let masks = rule_masks(&lines, &[false, true, false, false, false])?;
        assert_eq!(names(&masks[1]), ["zh-typography-4"]);
        assert!(masks[2].is_empty());
        assert!(masks[3].is_empty());
        assert!(masks[4].is_empty());
        Ok(())
    }

    #[test]
    fn unknown_ids_report_source_order_and_one_based_line() -> Result<(), &'static str> {
        let Err(error) = rule_masks(
            &["plain", "<!-- limae-disable fake, zh-typography-4 fake -->"],
            &[false, false],
        ) else {
            return Err("unknown ids must fail");
        };
        assert_eq!(error.line(), 2);
        assert_eq!(error.unknown_ids(), ["fake", "fake"]);
        assert!(
            error
                .to_string()
                .starts_with("2: unknown rule id(s) fake, fake;")
        );
        Ok(())
    }
}
