//! yaml-frontmatter-1: a `SKILL.md` front matter block must parse as YAML 1.2.

use saphyr_parser::Parser;

use super::LineMatch;
use crate::config::RuleId;

/// The only file name whose front matter is checked.
pub const SKILL_FILE_NAME: &str = "SKILL.md";

const FENCE: &str = "---";

/// Return the zero-based line index and match of the block's first error.
///
/// `lines` is the whole document in the checker's line view. A document whose
/// first line is not a fence has no front matter and yields nothing; an opening
/// fence with no closing fence is reported on the opening line.
#[must_use]
pub fn check(lines: &[&str]) -> Option<(usize, LineMatch)> {
    let is_fence = |line: &&str| line.trim_end() == FENCE;
    if !lines.first().is_some_and(is_fence) {
        return None;
    }
    let Some(close) = lines.iter().skip(1).position(is_fence).map(|i| i + 1) else {
        return Some((
            0,
            LineMatch {
                rule: RuleId::YAML_FRONTMATTER_1,
                name: "yaml-frontmatter-1 front matter has no closing ---".into(),
                range: 0..lines[0].len(),
            },
        ));
    };
    let block = lines[1..close].join("\n");
    let error = Parser::new_from_str(&block).find_map(Result::err)?;
    // Parser lines are one-based within the block, which starts on line 2; an
    // error at the end of the block points at the closing fence.
    let index = error.marker().line().clamp(1, close);
    let line = lines[index];
    let start = line
        .char_indices()
        .nth(error.marker().col())
        .map_or(line.len(), |(position, _)| position);
    Some((
        index,
        LineMatch {
            rule: RuleId::YAML_FRONTMATTER_1,
            name: format!(
                "yaml-frontmatter-1 invalid YAML front matter: {}",
                error.info()
            )
            .into(),
            range: start..start,
        },
    ))
}
