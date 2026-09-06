//! Deterministic rule primitives for line checks and prose fragment fixes.

pub mod spacing;
pub mod typography;

use std::ops::Range;

use crate::config::RuleId;

/// One rule match on an original line, before any fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    pub rule: RuleId,
    pub name: &'static str,
    /// UTF-8 byte range relative to the checked line. Use [`crate::text::snippet`]
    /// with that same line to obtain the reference's 12-scalar context window.
    pub range: Range<usize>,
}
