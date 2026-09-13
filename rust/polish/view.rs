//! The exported view a file-mode engine is started in (ADR-0017 §二).
//!
//! `limae polish <files>` does not start the engine in the caller's
//! repository. It starts it in a private directory holding a copy of the
//! repository's tracked files — what `git ls-files` lists — and nothing else:
//! no `.git`, no ignored or untracked file, and none of the paths the
//! engines read project configuration from: the `.claude`, `.codex` and
//! `.grok` directories and the `.mcp.json` file. The exclusion is structural
//! rather than a flag: a hook, a setting or an MCP server the repository
//! carries is not in the directory the engine starts in, so no engine loads
//! it, whatever its flags mean this version.
//! The per-engine flags that also switch project configuration off
//! (`engines.rs`) are depth behind this layer, not the load-bearing one.
//!
//! What the view is not: it is not isolation. The engine keeps its file
//! tools, and a path outside the view that it is given, or guesses, it can
//! still read (`docs/research/polish-engine-cli-behavior.md` §一). It is a
//! copy, so the engine reads the tracked bytes as they were when the run
//! started; a target that an earlier engine call of the same run has already
//! rewritten is stale in it.
//!
//! The view is also what the tripwire watches. [`View::snapshot`] hashes
//! every path in it before the engines run and again after each call, and a
//! difference means an engine had write capability it was not supposed to
//! have — a read-only flag that stopped meaning that after a CLI upgrade, a
//! tool name that was silently renamed out of the allow list. The tripwire
//! detects; it does not prevent, and it sees only the view: nothing about
//! the repository, the home directory or the network.

use std::collections::BTreeMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use getrandom::fill;
use thiserror::Error;

/// Names kept out of the view wherever they occur: the engines' own
/// project-configuration directories, and the project MCP file that starts
/// a process on its own (ADR-0017 §二; measured in
/// `docs/research/polish-engine-cli-behavior.md` §七).
pub const EXCLUDED_NAMES: &[&str] = &[".claude", ".codex", ".grok", ".mcp.json"];

/// Failure to find the repository, build the view, or read it back.
///
/// No variant carries a path from inside the repository or a byte the engine
/// wrote: the tripwire reports a count and one relative path of its own view,
/// and git's output is not quoted (`AGENTS.md` 「隐私边界」).
#[derive(Debug, Error)]
pub enum ViewError {
    /// `git` could not be started at all.
    #[error("file mode needs git on PATH: {source}")]
    GitMissing {
        #[source]
        source: io::Error,
    },
    /// `git rev-parse` found no repository above the working directory.
    #[error("file mode needs a git repository; the working directory is not inside one")]
    NotARepository,
    /// `git` found the repository but the listing failed — an index or
    /// I/O failure inside a real repository, which is not the same
    /// mistake as being outside one. Git's own message stays on its
    /// stderr, which is not captured here.
    #[error("git {command} failed in the repository ({status}); run it by hand to see why")]
    GitFailed {
        command: &'static str,
        status: std::process::ExitStatus,
    },
    /// A tracked path is not UTF-8, which this implementation does not carry.
    #[error("a tracked path is not valid UTF-8")]
    PathEncoding,
    /// The OS random source for the private directory's name was unavailable.
    #[error("view directory randomness failed: {source}")]
    Random {
        #[source]
        source: getrandom::Error,
    },
    /// Creating, copying into, reading or removing the view failed.
    #[error("cannot {action} the view: {source}")]
    Io {
        action: &'static str,
        #[source]
        source: io::Error,
    },
}

/// Return the root of the repository `cwd` is inside.
///
/// # Errors
/// [`ViewError::GitMissing`] when `git` cannot be started,
/// [`ViewError::NotARepository`] when it reports no repository.
pub fn repository_root(cwd: &Path) -> Result<PathBuf, ViewError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|source| ViewError::GitMissing { source })?;
    if !output.status.success() {
        return Err(ViewError::NotARepository);
    }
    let root = std::str::from_utf8(&output.stdout)
        .map_err(|_| ViewError::PathEncoding)?
        .trim_end_matches('\n');
    Ok(PathBuf::from(root))
}

/// A private directory holding the repository's tracked files.
pub struct View {
    root: PathBuf,
}

/// The contents of a view at one moment: every path under it, with the
/// length and a hash of what it holds.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot(BTreeMap<PathBuf, (u64, u64)>);

impl View {
    /// Copy the tracked files of `repository` into a fresh private directory.
    ///
    /// A tracked path that is a symbolic link, a submodule, or no longer on
    /// disk is left out rather than recreated: a link would point the engine
    /// outside the view, and the other two are not files.
    ///
    /// # Errors
    /// Any failure to run `git ls-files`, create the directory, or copy a
    /// file. A partly built view is removed before the error is returned.
    pub fn export(repository: &Path) -> Result<Self, ViewError> {
        let listing = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(["ls-files", "-z"])
            .output()
            .map_err(|source| ViewError::GitMissing { source })?;
        if !listing.status.success() {
            return Err(ViewError::GitFailed {
                command: "ls-files",
                status: listing.status,
            });
        }
        let view = Self {
            root: private_directory()?,
        };
        if let Err(error) = view.copy_tracked(repository, &listing.stdout) {
            let _ = fs::remove_dir_all(&view.root);
            return Err(error);
        }
        Ok(view)
    }

    fn copy_tracked(&self, repository: &Path, listing: &[u8]) -> Result<(), ViewError> {
        for entry in listing
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let relative =
                Path::new(std::str::from_utf8(entry).map_err(|_| ViewError::PathEncoding)?);
            if relative.components().any(|component| {
                EXCLUDED_NAMES
                    .iter()
                    .any(|excluded| component.as_os_str() == *excluded)
            }) {
                continue;
            }
            let source = repository.join(relative);
            let kind = match fs::symlink_metadata(&source) {
                Ok(metadata) => metadata.file_type(),
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(ViewError::Io {
                        action: "read",
                        source,
                    });
                }
            };
            if !kind.is_file() {
                continue;
            }
            let target = self.root.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|source| ViewError::Io {
                    action: "create",
                    source,
                })?;
            }
            fs::copy(&source, &target).map_err(|source| ViewError::Io {
                action: "copy into",
                source,
            })?;
        }
        Ok(())
    }

    /// The directory the engine is started in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Record every path under the view with a hash of its contents.
    ///
    /// # Errors
    /// Any failure to read the view.
    pub fn snapshot(&self) -> Result<Snapshot, ViewError> {
        let mut entries = BTreeMap::new();
        collect(&self.root, &self.root, &mut entries)?;
        Ok(Snapshot(entries))
    }

    /// Remove the view.
    ///
    /// # Errors
    /// The directory could not be removed.
    pub fn remove(self) -> Result<(), ViewError> {
        fs::remove_dir_all(&self.root).map_err(|source| ViewError::Io {
            action: "remove",
            source,
        })
    }
}

impl Snapshot {
    /// Return the paths that differ between two snapshots: added, removed, or
    /// changed in length or content, relative to the view.
    #[must_use]
    pub fn differences(&self, later: &Self) -> Vec<PathBuf> {
        let mut paths: Vec<&PathBuf> = self
            .0
            .iter()
            .filter(|(path, entry)| later.0.get(*path) != Some(entry))
            .map(|(path, _)| path)
            .chain(later.0.keys().filter(|path| !self.0.contains_key(*path)))
            .collect();
        paths.sort();
        paths.dedup();
        paths.into_iter().cloned().collect()
    }
}

fn collect(
    root: &Path,
    directory: &Path,
    entries: &mut BTreeMap<PathBuf, (u64, u64)>,
) -> Result<(), ViewError> {
    let read = fs::read_dir(directory).map_err(|source| ViewError::Io {
        action: "read",
        source,
    })?;
    for entry in read {
        let entry = entry.map_err(|source| ViewError::Io {
            action: "read",
            source,
        })?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
        let kind = entry.file_type().map_err(|source| ViewError::Io {
            action: "read",
            source,
        })?;
        // A link the engine created is recorded by where it points, not
        // followed: following it would hash something outside the view.
        if kind.is_symlink() {
            let target = fs::read_link(&path).map_err(|source| ViewError::Io {
                action: "read",
                source,
            })?;
            let mut hasher = DefaultHasher::new();
            target.hash(&mut hasher);
            entries.insert(relative, (0, hasher.finish()));
        } else if kind.is_dir() {
            entries.insert(relative, (0, 0));
            collect(root, &path, entries)?;
        } else {
            let bytes = fs::read(&path).map_err(|source| ViewError::Io {
                action: "read",
                source,
            })?;
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            entries.insert(relative, (bytes.len() as u64, hasher.finish()));
        }
    }
    Ok(())
}

fn private_directory() -> Result<PathBuf, ViewError> {
    let mut nonce = [0_u8; 16];
    fill(&mut nonce).map_err(|source| ViewError::Random { source })?;
    let mut name = String::with_capacity(48);
    name.push_str("limae-view-");
    name.push_str(&std::process::id().to_string());
    name.push('-');
    for byte in nonce {
        use std::fmt::Write;
        let _ = write!(name, "{byte:02x}");
    }
    let path = std::env::temp_dir().join(name);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|source| ViewError::Io {
        action: "create",
        source,
    })?;
    Ok(path)
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
