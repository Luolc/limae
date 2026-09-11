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
/// Every Markdown file means every one: a dotted name and a file under a dotted
/// directory are both checked, because a project keeps real prose in `.github/`
/// and `.agents/`, and a rule the checker silently skips is the blind spot this
/// selection exists to close. `.git` is the one name excluded outright, and it
/// is excluded by name rather than by being hidden, so that the exclusion says
/// what it means. A symbolic link to a file is checked through the link; a
/// symbolic link to a directory is not descended, which is what keeps the walk
/// finite without a loop detector. A link that cannot be resolved at all is
/// selected rather than dropped, and fails when it is read, the way the same
/// path does when it is named on the command line.
///
/// What remains is ignored on purpose: `.gitignore` (inside a repository),
/// `.git/info/exclude`, the global excludes file, `.ignore`, and every parent
/// directory's copies of those, all supplied by [`WalkBuilder`]. `.limae-ignore`
/// is applied afterwards by [`not_ignored`](super::not_ignored), the same way
/// as for explicit inputs.
///
/// Sorting makes the order deterministic; the walk itself reports directory
/// entries in whatever order the filesystem gives them.
pub fn walk_markdown(cwd: &Path) -> Result<Vec<PathBuf>, WalkError> {
    let mut paths = Vec::new();
    let walk = WalkBuilder::new(cwd)
        .hidden(false)
        // Depth 0 is `cwd` itself. Exempting it keeps a run whose working
        // directory happens to be named `.git` from filtering away its own root
        // and reporting an empty tree.
        .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != ".git")
        .build();
    for entry in walk {
        let entry = entry.map_err(WalkError)?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("md")) {
            continue;
        }
        // A directory is the only thing dropped here. `file_type` describes the
        // link rather than its target, so a link is resolved to find out which
        // it is -- and a link that will not resolve is still selected, so that
        // reading names it. Deciding with a boolean that reports "no" for both
        // "not a file" and "cannot tell" would put that file back exactly where
        // this selection change took it out of: unchecked and unmentioned.
        let directory = match entry.file_type() {
            Some(kind) if kind.is_symlink() => path.metadata().is_ok_and(|meta| meta.is_dir()),
            Some(kind) => kind.is_dir(),
            None => false,
        };
        if !directory {
            paths.push(path.strip_prefix(cwd).unwrap_or(path).to_owned());
        }
    }
    paths.sort();
    Ok(paths)
}
