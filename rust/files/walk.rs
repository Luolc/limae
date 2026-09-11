//! Filesystem discovery of the Markdown files `--all` checks.
//!
//! `.limae-ignore` is deliberately not registered as a [`WalkBuilder`] custom
//! ignore file. Its meaning is `spec/rules.md`'s rather than gitignore's: the
//! nearest file replaces all parents instead of stacking with them, a bare `!`
//! is an error, braces are literal, and a character class left unclosed within
//! a segment is a no-op. Explicit inputs have to keep going through
//! [`not_ignored`](super::not_ignored) for that, so giving the walk a second,
//! gitignore-shaped reading of the same file would give one ignore file two
//! meanings, decided by nothing but how its subject happened to be selected.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use thiserror::Error;

/// Failure to obtain a complete Markdown list from the filesystem.
///
/// Any error aborts the walk rather than skipping the entry it came from: a
/// directory that cannot be read is exactly the case where skipping it would
/// report the tree clean without having looked at it. The wrapped error names
/// the path or ignore file it came from, so this adds no location of its own.
#[derive(Debug, Error)]
#[error("cannot walk the file tree: {0}")]
pub struct WalkError(#[from] pub ignore::Error);

/// List `*.md` files at and below `cwd`, relative to that directory and sorted.
///
/// Discovery is the filesystem, not the Git index, so a file that has never
/// been `git add`ed is checked like any other. `cwd` need not be a repository.
///
/// [`WalkBuilder`]'s defaults do the filtering: `.gitignore` (inside a
/// repository), `.git/info/exclude`, the global excludes file, `.ignore`, and
/// every parent directory's copies of those; hidden entries are skipped, which
/// is what keeps `.git` itself out. Symbolic links are not followed. The
/// `.limae-ignore` file is applied afterwards by
/// [`not_ignored`](super::not_ignored), the same way as for explicit inputs.
///
/// Sorting makes the order deterministic; the walk itself reports directory
/// entries in whatever order the filesystem gives them.
pub fn walk_markdown(cwd: &Path) -> Result<Vec<PathBuf>, WalkError> {
    let mut paths = Vec::new();
    for entry in WalkBuilder::new(cwd).build() {
        let entry = entry.map_err(WalkError)?;
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && path.extension() == Some(OsStr::new("md"))
        {
            paths.push(path.strip_prefix(cwd).unwrap_or(path).to_owned());
        }
    }
    paths.sort();
    Ok(paths)
}
