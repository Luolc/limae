//! Semantic polish support.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

pub mod cache;
pub mod cli;
pub mod config;
pub mod diagnosis;
pub mod engines;
pub mod process;
pub mod prompt;
pub mod select;

/// Return the home directory of a run.
///
/// `None` when the environment carries no usable `HOME`; the environment passed
/// in is the whole environment of the run, so there is no ambient fallback. A
/// lookup that fell back to the process environment would make an ordering, and
/// the cache path that goes with it, depend on whoever is running the tests.
#[must_use]
pub fn home(env: &[(OsString, OsString)]) -> Option<PathBuf> {
    value(env, "HOME").map(PathBuf::from)
}

/// Return one environment variable's value, treating empty as unset.
///
/// An empty variable is not a marker, the way an unset one is not: this is the
/// reference implementation's `env.get(name) or ...`.
pub(crate) fn value<'a>(env: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    env.iter()
        .find(|(variable, value)| variable == OsStr::new(name) && !value.is_empty())
        .map(|(_, value)| value.as_os_str())
}
