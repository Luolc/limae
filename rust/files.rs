//! File selection shared by explicit inputs and the `--all` filesystem walk.

use std::fs;
use std::io as std_io;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use soft_canonicalize::soft_canonicalize;
use thiserror::Error;

mod io;
pub use io::{FileError, FileText, FixStatus, fix_file};
mod walk;
pub use walk::{WalkError, walk_markdown};

/// A discovery, reading, resolution, or pattern error in file filtering.
#[derive(Debug, Error)]
pub enum IgnoreError {
    /// The built-in literal adaptation expression could not be compiled.
    #[error("could not build the ignore literal adapter: {0}")]
    Matcher(#[from] regex::Error),
    /// A filesystem operation failed at the given path.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std_io::Error,
    },
    /// The ignore file could not be compiled in full.
    #[error("{path}:{line}: invalid ignore pattern: {source}")]
    Pattern {
        path: PathBuf,
        line: usize,
        #[source]
        source: ignore::Error,
    },
}

fn at(path: &Path, source: std_io::Error) -> IgnoreError {
    IgnoreError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Find the nearest `.limae-ignore`, checking each directory before its `.git`.
///
/// `start` is a directory. A `.git` file or directory stops the search; without
/// either, search continues to the filesystem root. Configuration files have
/// no effect on this search.
pub fn find_ignore(start: &Path) -> Result<Option<PathBuf>, IgnoreError> {
    let start = std::path::absolute(start).map_err(|err| at(start, err))?;
    for directory in start.ancestors() {
        let candidate = directory.join(".limae-ignore");
        match fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_file() => return Ok(Some(candidate)),
            Ok(_) => (),
            Err(err) if err.kind() == std_io::ErrorKind::NotFound => (),
            Err(err) => return Err(at(&candidate, err)),
        }
        let git = directory.join(".git");
        match git.try_exists() {
            Ok(true) => break,
            Ok(false) => (),
            Err(err) => return Err(at(&git, err)),
        }
    }
    Ok(None)
}

/// Filter inputs relative to an explicit working directory, retaining spelling,
/// duplicates, and order. Use this for both explicit inputs and [`walk_markdown`].
///
/// The nearest ignore file replaces all parents. Matching resolves symlinks and
/// permits missing suffixes; paths outside the resolved ignore root stay in the
/// input. This does not require input files to exist.
pub fn not_ignored(paths: &[PathBuf], cwd: &Path) -> Result<Vec<PathBuf>, IgnoreError> {
    let Some(found) = find_ignore(cwd)? else {
        return Ok(paths.to_vec());
    };
    let mut root = found.clone();
    root.pop();
    let root = soft_canonicalize(&root).map_err(|err| at(&root, err))?;
    let text = fs::read_to_string(&found).map_err(|err| at(&found, err))?;
    let patterns = IgnorePatterns::parse(&root, &found, &text)?;
    paths
        .iter()
        .filter_map(|path| {
            let absolute = cwd.join(path);
            match soft_canonicalize(&absolute) {
                Err(err) => Some(Err(at(path, err))),
                Ok(resolved) => {
                    let ignored = resolved.starts_with(&root) && patterns.ignores(&resolved);
                    (!ignored).then(|| Ok(path.clone()))
                }
            }
        })
        .collect()
}

struct IgnorePatterns {
    matcher: Gitignore,
    lines: Vec<String>,
}

impl IgnorePatterns {
    fn parse(root: &Path, path: &Path, text: &str) -> Result<Self, IgnoreError> {
        let mut builder = GitignoreBuilder::new(root);
        builder.allow_unclosed_class(false);
        let literal_tokens = regex::Regex::new(r"\\.|\[[!^]?\]?[^\]]*\]|[{}]")?;
        let mut lines = Vec::new();
        let text = text.replace("\r\n", "\n");
        for (index, line) in text
            .split([
                '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}',
                '\u{2028}', '\u{2029}',
            ])
            .enumerate()
        {
            if line.trim_end() == "!" {
                return Err(IgnoreError::Pattern {
                    path: path.to_owned(),
                    line: index + 1,
                    source: ignore::Error::Glob {
                        glob: None,
                        err: "negation requires a pattern".to_owned(),
                    },
                });
            }
            // Gitignore treats braces literally; globset also supports alternation.
            let mut escaped = String::new();
            let mut start = 0;
            for token in literal_tokens.find_iter(line) {
                escaped.push_str(&line[start..token.start()]);
                if matches!(token.as_str(), "{" | "}") {
                    escaped.push('\\');
                }
                escaped.push_str(token.as_str());
                start = token.end();
            }
            escaped.push_str(&line[start..]);
            // Character classes must close within one path segment (spec/rules.md).
            if escaped.split('/').any(|segment| {
                globset::GlobBuilder::new(segment)
                    .backslash_escape(true)
                    .allow_unclosed_class(false)
                    .build()
                    .is_err_and(|err| matches!(err.kind(), globset::ErrorKind::UnclosedClass))
            }) {
                continue;
            }
            builder
                .add_line(Some(path.to_owned()), &escaped)
                .map_err(|source| IgnoreError::Pattern {
                    path: path.to_owned(),
                    line: index + 1,
                    source,
                })?;
            lines.push(if escaped.ends_with("\\ ") {
                escaped
            } else {
                escaped.trim_end().to_owned()
            });
        }
        let matcher = builder.build().map_err(|source| IgnoreError::Pattern {
            path: path.to_owned(),
            line: lines.len(),
            source,
        })?;
        Ok(Self { matcher, lines })
    }

    fn ignores(&self, path: &Path) -> bool {
        let direct = self.matcher.matched(path, false);
        if !direct.is_none() {
            return direct.is_ignore();
        }
        // File matches outrank directory matches. Among directories, the last
        // matching pattern wins regardless of the ancestor's depth.
        path.ancestors()
            .skip(1)
            .take_while(|parent| *parent != self.matcher.path())
            .filter_map(|parent| {
                let matched = self.matcher.matched(parent, true);
                let glob = matched.inner()?;
                let index = self
                    .lines
                    .iter()
                    .rposition(|line| line == glob.original())?;
                Some((index, matched.is_ignore()))
            })
            .max_by_key(|(index, _)| *index)
            .is_some_and(|(_, ignored)| ignored)
    }
}
