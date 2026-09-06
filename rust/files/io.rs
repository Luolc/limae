//! Synchronous UTF-8 file boundaries for the document pipeline.

use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::Utf8Error;

use thiserror::Error;

use crate::config::ResolvedConfig;
use crate::directives::DirectiveError;
use crate::pipeline::{Finding, Pipeline};

/// A file read into the logical text seen by Python's universal-newline mode.
///
/// CRLF and bare CR are represented as LF. The path is retained so a later
/// pipeline error can identify the file without copying the finding snippets.
pub struct FileText {
    path: PathBuf,
    text: String,
}

impl FileText {
    /// Read one complete UTF-8 file before exposing any text to the pipeline.
    ///
    /// # Errors
    /// Returns a path-bearing [`FileError`] when reading or UTF-8 decoding fails.
    pub fn read(path: &Path) -> Result<Self, FileError> {
        let bytes = fs::read(path).map_err(|source| FileError::Read {
            path: path.to_owned(),
            source,
        })?;
        let text = std::str::from_utf8(&bytes).map_err(|source| FileError::Utf8 {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self {
            path: path.to_owned(),
            text: universal_newlines(text),
        })
    }

    /// Return the normalized logical text, including whether it ends at a line boundary.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Check this owned text while borrowing finding snippets from it.
    ///
    /// # Errors
    /// Returns a path-bearing [`FileError::Directive`] for an unknown inline rule.
    pub fn check<'text>(
        &'text self,
        pipeline: &Pipeline,
        config: &ResolvedConfig,
    ) -> Result<Vec<Finding<'text>>, FileError> {
        pipeline
            .check(&self.text, config)
            .map_err(|source| FileError::Directive {
                path: self.path.clone(),
                source,
            })
    }
}

/// Whether a fix operation left the bytes alone or wrote changed logical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixStatus {
    Unchanged,
    Written,
}

/// A complete file read, decode, pipeline, write, or flush failure.
#[derive(Debug, Error)]
pub enum FileError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot decode {path} as UTF-8: {source}")]
    Utf8 {
        path: PathBuf,
        #[source]
        source: Utf8Error,
    },
    #[error("{path}:{source}")]
    Directive {
        path: PathBuf,
        #[source]
        source: DirectiveError,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot flush {path}: {source}")]
    Flush {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Read and calculate the complete fix before writing changed UTF-8 LF text.
///
/// An unchanged logical string is not opened for writing, preserving its exact
/// bytes and metadata. A changed file is truncated through its existing path,
/// which follows symlinks and preserves the target inode, mode, and hard links.
/// Call [`FileText::read`] again before post-fix checking so the check observes
/// the actual file state.
///
/// # Errors
/// Returns a path-bearing [`FileError`] for every read, decode, directive, write,
/// or flush failure. Fix calculation completes before the file is truncated.
pub fn fix_file(
    path: &Path,
    pipeline: &Pipeline,
    config: &ResolvedConfig,
) -> Result<FixStatus, FileError> {
    let source = FileText::read(path)?;
    let fixed = pipeline
        .fix(source.as_str(), config)
        .map_err(|source| FileError::Directive {
            path: path.to_owned(),
            source,
        })?;
    if fixed == source.as_str() {
        return Ok(FixStatus::Unchanged);
    }

    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|source| FileError::Write {
            path: path.to_owned(),
            source,
        })?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(fixed.as_bytes())
        .map_err(|source| FileError::Write {
            path: path.to_owned(),
            source,
        })?;
    writer.flush().map_err(|source| FileError::Flush {
        path: path.to_owned(),
        source,
    })?;
    Ok(FixStatus::Written)
}

fn universal_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
