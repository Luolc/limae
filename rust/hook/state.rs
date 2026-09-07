//! Where one session's hook state lives, and what is allowed to be in it.
//!
//! The state directory holds assistant replies — the cached batches of a
//! message still streaming, and the A/B ledger beside them — so every rule here
//! is about keeping them the user's own and keeping them short-lived
//! (ADR-0009 sections 五, 六 and 八).
//!
//! Four of those rules are load-bearing and each has one function to itself:
//!
//! * **Nowhere else.** [`root`] derives the location from the system's scratch
//!   directory and a fixed name under it. There is no setting that moves it,
//!   not even one meant for the tests: a hook's environment is set by whatever
//!   configured the session, so a name is not a boundary, and it would take one
//!   such setting pointing into a checkout for the next `git add` to carry a
//!   reply into a public repository.
//! * **Nobody else's to read.** Every directory is created `0o700` and every
//!   file `0o600`, by the call that creates them rather than by a `chmod`
//!   afterwards — between a default-mode create and a `chmod` the reply is
//!   readable by everyone on the machine.
//! * **Not for long.** [`prune`] has two horizons because state is left behind
//!   two ways: [`RETENTION`] for a session nobody has been in, and
//!   [`ORPHAN_RETENTION`] for one message's batches inside a session that is
//!   still live.
//! * **No prose.** [`note`] writes a step and a kind of failure and nothing
//!   else. The file outlives the run, this repository is public, and an engine
//!   is free to quote its environment back at us.
//!
//! Ids arrive from the host and become path segments here, so [`identifier`]
//! folds away anything that is not a plain name before any of that starts.
//!
//! The modes are Unix modes. On a platform without them every call that would
//! create state fails with [`std::io::ErrorKind::Unsupported`] rather than
//! quietly creating a world-readable copy of the user's replies; the hook then
//! has nowhere to put a reply and does nothing, which is its behaviour for
//! every other failure (ADR-0009 section 六).

use std::ffi::OsString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

use serde_json::json;
use soft_canonicalize::soft_canonicalize;

use crate::polish::diagnosis::FailureReason;
use crate::polish::value;

/// The one directory under the system's scratch directory that all of this
/// lives in.
pub const STATE_DIRECTORY: &str = "limae-hook";
/// Where a session keeps the batches of messages that have not finished
/// streaming, one directory per message.
pub const PARTS_DIRECTORY: &str = "parts";
/// Where a fail-open path says what it did.
///
/// Failing open means the user is never interrupted, and on its own it also
/// means nobody can find out why a reply went unpolished — "no block appeared"
/// would be the whole of the evidence. This file is the other half of ADR-0009
/// section 六: the user still sees nothing, and whoever is debugging sees the
/// step and the kind of failure, which is what a person needs to know where to
/// look next.
pub const DIAGNOSTICS_FILENAME: &str = "diagnostics.jsonl";
/// What a batch is called once it is all there.
pub const PART_SUFFIX: &str = ".part";
/// What a batch is called while it is being written.
pub const TEMPORARY_SUFFIX: &str = ".writing";
/// The mode every directory here is created with.
pub const DIRECTORY_MODE: u32 = 0o700;
/// The mode every file here is created with.
pub const FILE_MODE: u32 = 0o600;
/// How long a session's state is kept.
///
/// It is scratch: the batches of a finished message are deleted as soon as they
/// are assembled, and what is left is the A/B ledger of sessions that have
/// ended.
pub const RETENTION: Duration = Duration::from_secs(24 * 3600);
/// How long one message's cached batches are kept when its final batch never
/// comes.
///
/// That happens for real: a message the host abandons mid-stream — the session
/// is interrupted, or restarted with `--resume` — gets no final flush, so
/// nothing ever assembles it and nothing deletes it (2026-09-01, one such
/// message left five batches behind). [`RETENTION`] alone does not reach these,
/// because the session they are in is the live one. An hour is orders of
/// magnitude past the seconds a message spends streaming, so a sweep can never
/// take the batches of a message still arriving.
pub const ORPHAN_RETENTION: Duration = Duration::from_secs(3600);

/// The longest a sanitised id may be.
const NAME_LIMIT: usize = 64;

/// Which step of a turn failed open.
///
/// A diagnostics line says where to look without saying what was being
/// polished.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Step {
    /// Putting a message back together from its cached batches.
    Assemble,
    /// The one engine call of an ordinary turn.
    Single,
    /// The two engine calls of a sampled turn.
    Ab,
    /// Writing the session's record of what polish did.
    Record,
    /// This repository's own deterministic fixes over a rewrite.
    Fix,
    /// Anything that got as far as the top of the hook and crashed.
    Display,
}

impl Step {
    /// Return the name this step is written down under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assemble => "assemble",
            Self::Single => "single",
            Self::Ab => "ab",
            Self::Record => "record",
            Self::Fix => "fix",
            Self::Display => "display",
        }
    }
}

/// How a step failed.
///
/// The engine's own reasons are carried as they are; the rest are the ways this
/// code fails on its own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// The engine did not answer, and why.
    Engine(FailureReason),
    /// A message went to the deadline with a batch missing.
    Incomplete,
    /// The deterministic fixes changed the rewrite, which is a selection signal
    /// and not only a display fix.
    Repaired,
    /// The `[polish]` table says something this code cannot act on.
    Misconfigured,
    /// A bug here, or a state directory that would not cooperate.
    Crashed,
}

impl Kind {
    /// Return the name this kind is written down under.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Engine(reason) => reason.as_str(),
            Self::Incomplete => "incomplete",
            Self::Repaired => "repaired",
            Self::Misconfigured => "config",
            Self::Crashed => "crashed",
        }
    }
}

/// Turn one id from the payload into a safe path segment.
///
/// Session and message ids are UUIDs, but they arrive from outside and become
/// path segments here, so everything that is not a plain name is folded away
/// rather than trusted. A dot is folded too: `..` is a perfectly
/// ordinary-looking name that would leave the directory.
///
/// The result is empty when there was no usable id, which is how a caller knows
/// there is nothing to do.
#[must_use]
pub fn identifier(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .take(NAME_LIMIT)
        .map(|character| if plain(character) { character } else { '_' })
        .collect()
}

/// Return whether a path is inside a git checkout.
///
/// True when the path or one of its ancestors holds a `.git` — a directory in a
/// normal checkout, a file in a worktree. A path that cannot be resolved at all
/// is reported as inside one: the answer decides whether replies may be written
/// there, and the safe end of an unanswerable question is the one where they
/// are not.
#[must_use]
pub fn in_work_tree(path: &Path) -> bool {
    let Ok(resolved) = soft_canonicalize(path) else {
        return true;
    };
    resolved
        .ancestors()
        .any(|directory| directory.join(".git").exists())
}

/// Return the directory every session's state lives under.
///
/// See the module documentation for why this has no setting. `None` when
/// scratch itself is inside a checkout, which leaves the hook with nowhere to
/// put a reply and therefore nothing to do.
#[must_use]
pub fn root(env: &[(OsString, OsString)]) -> Option<PathBuf> {
    let scratch = value(env, "TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let root = scratch.join(STATE_DIRECTORY);
    (!in_work_tree(&root)).then_some(root)
}

/// Return one session's state directory, creating it.
///
/// `session` is expected to have been through [`identifier`] already; it is
/// joined as one path segment.
pub fn session(root: &Path, session: &str) -> io::Result<PathBuf> {
    let directory = root.join(session);
    create_directory(&directory)?;
    Ok(directory)
}

/// Return the name one batch of a message is cached under.
#[must_use]
pub fn part(parts: &Path, index: usize) -> PathBuf {
    parts.join(format!("{index:06}{PART_SUFFIX}"))
}

/// Cache one batch of an unfinished message.
///
/// The batch is written under a temporary name and renamed into place in one
/// step, because another batch of the same message may be reading this
/// directory right now: the host starts one process per batch and does not wait
/// for it before starting the next (2026-09-01, Claude Code 2.1.257: the
/// dispatcher only serialises what the answers do to the screen, not the runs).
/// The temporary name carries this process's id so that the exclusive create
/// stays exclusive without a leftover from a dead sibling blocking it.
pub fn keep(parts: &Path, index: usize, delta: &str) -> io::Result<()> {
    create_directory(parts)?;
    let writing = parts.join(format!(
        "{index:06}.{}{TEMPORARY_SUFFIX}",
        std::process::id()
    ));
    let mut file = create(&writing)?;
    file.write_all(delta.as_bytes())?;
    drop(file);
    fs::rename(&writing, part(parts, index))
}

/// Write down that one fail-open path fired.
///
/// Failing open is the right behaviour and a bad witness. One line per failure
/// fixes that without moving the boundary: it goes to the session-state
/// directory, never to the screen, and what it may hold is bounded by the same
/// rule as the ledger (ADR-0009 section 八) — the message's id, the step, the
/// kind. Never the prose, never what an engine printed, never a credential.
///
/// A hook that cannot write its own diagnostics still has a reply to get out of
/// the way of, so nothing is reported when this fails.
pub fn note(directory: &Path, message: &str, step: Step, kind: Kind, now: SystemTime) {
    let line = json!({
        "at": timestamp(now),
        "message_id": message,
        "step": step.as_str(),
        "kind": kind.as_str(),
    });
    let _ = append(directory, &line.to_string());
}

/// Return whether a directory has gone untouched for longer than `keep`.
///
/// False when it is not, and when it cannot be aged at all: a directory this
/// run cannot stat is not one it should be deleting.
#[must_use]
pub fn stale(path: &Path, now: SystemTime, keep: Duration) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return false;
    };
    now.duration_since(modified).is_ok_and(|age| age > keep)
}

/// Delete the state nobody is coming back for.
///
/// Two horizons, because there are two ways state is left behind. A whole
/// session goes when nobody has been in it for [`RETENTION`]. Inside a session
/// that is still live, one message's batches go after [`ORPHAN_RETENTION`],
/// when its final batch never came — the host abandons a message it is killed
/// in the middle of, and those batches would otherwise sit there until the
/// session itself expired.
pub fn prune(root: &Path, now: SystemTime) {
    let Ok(sessions) = fs::read_dir(root) else {
        return;
    };
    for session in sessions.flatten() {
        let session = session.path();
        if !session.is_dir() {
            continue;
        }
        if stale(&session, now, RETENTION) {
            let _ = fs::remove_dir_all(&session);
            continue;
        }
        let Ok(messages) = fs::read_dir(session.join(PARTS_DIRECTORY)) else {
            continue;
        };
        for message in messages.flatten() {
            let message = message.path();
            if message.is_dir() && stale(&message, now, ORPHAN_RETENTION) {
                let _ = fs::remove_dir_all(&message);
            }
        }
    }
}

/// Return whether a character may stand for itself in a path segment.
fn plain(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '-'
}

/// Append one line to the session's diagnostics file, creating what is missing.
fn append(directory: &Path, line: &str) -> io::Result<()> {
    create_directory(directory)?;
    let path = directory.join(DIAGNOSTICS_FILENAME);
    let mut file = open(OpenOptions::new().append(true).create(true), &path)?;
    writeln!(file, "{line}")
}

/// Create one directory and every missing ancestor, each with
/// [`DIRECTORY_MODE`].
///
/// Every level gets the mode, not only the last one: a directory that holds
/// directories of replies is as much this user's own as the replies are.
fn create_directory(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_directory(parent)?;
    }
    let mut builder = DirBuilder::new();
    apply_directory_mode(&mut builder);
    match builder.create(path) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && path.is_dir() => Ok(()),
        result => result,
    }
}

/// Create one file that did not exist, with [`FILE_MODE`].
fn create(path: &Path) -> io::Result<File> {
    open(OpenOptions::new().write(true).create_new(true), path)
}

/// Open one file, giving it [`FILE_MODE`] if this call is what creates it.
///
/// The mode goes in the create call rather than a `chmod` after it, for the
/// reason every file here has one: between the two the user's reply is
/// everyone's to read.
fn open(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        options.mode(FILE_MODE).open(path)
    }
    #[cfg(not(unix))]
    {
        let _ = (options, path);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "limae's hook state needs Unix file modes",
        ))
    }
}

/// Give a directory being created [`DIRECTORY_MODE`], where that exists.
fn apply_directory_mode(builder: &mut DirBuilder) {
    #[cfg(unix)]
    {
        let _ = builder.mode(DIRECTORY_MODE);
    }
    #[cfg(not(unix))]
    {
        let _ = builder;
    }
}

/// Render one instant the way the diagnostics file writes it: UTC, ISO 8601.
///
/// The reference implementation's `datetime.now(UTC).isoformat()`, digit for
/// digit — the fractional part is left out when there is none, and the offset
/// is spelled out rather than abbreviated to `Z`.
fn timestamp(now: SystemTime) -> String {
    let (seconds, microseconds) = since_epoch(now);
    let (year, month, day) = civil(seconds.div_euclid(86_400));
    let time = seconds.rem_euclid(86_400);
    let (hour, minute, second) = (time / 3600, (time % 3600) / 60, time % 60);
    let fraction = if microseconds == 0 {
        String::new()
    } else {
        format!(".{microseconds:06}")
    };
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}+00:00")
}

/// Split one instant into whole seconds since the epoch and microseconds after
/// them, with the microseconds always counting forwards.
fn since_epoch(now: SystemTime) -> (i64, u32) {
    match now.duration_since(UNIX_EPOCH) {
        Ok(since) => (as_seconds(since.as_secs()), since.subsec_micros()),
        Err(before) => {
            let before = before.duration();
            let seconds = -as_seconds(before.as_secs());
            match before.subsec_micros() {
                0 => (seconds, 0),
                micros => (seconds - 1, 1_000_000 - micros),
            }
        }
    }
}

fn as_seconds(seconds: u64) -> i64 {
    i64::try_from(seconds).unwrap_or(i64::MAX)
}

/// Return the civil year, month and day of a count of days since 1970-01-01.
///
/// Howard Hinnant's `civil_from_days`, whose era arithmetic is exact for every
/// day this can be handed.
fn civil(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        u32::try_from(month).unwrap_or(0),
        u32::try_from(day).unwrap_or(0),
    )
}

// Unix-only: everything here creates state, and on another platform every one
// of those calls reports that it cannot.
#[cfg(all(test, unix))]
#[path = "state_tests.rs"]
mod tests;
