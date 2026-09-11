//! One batch of a message: caching it, waiting for the batches before it, and
//! the prefix replay that decides what to show in its place.
//!
//! Claude Code's `MessageDisplay` fires once per batch of newly completed lines
//! while an assistant message streams, and every batch is a process of its own,
//! so the only thing a batch knows about the ones before it is what they left
//! on disk (ADR-0016 section 二). The host starts one process per batch and
//! does not wait for it before starting the next (2026-09-01, Claude Code
//! 2.1.257: the dispatcher only serialises what the answers do to the screen,
//! not the runs), so a batch can be running while an earlier sibling is still
//! writing — hence [`assemble`] waits, and polls.
//!
//! [`replay`] is the fix itself. A batch is not fixed on its own: the rules
//! carry state from earlier lines — a fence that is open, a directive that is
//! in force, a paragraph that continues — and a batch fixed without that state
//! puts `foo ()` inside a code block (ADR-0016 读数 B). So the whole message
//! so far is fixed, prefix and batch together, and the lines belonging to this
//! batch are cut out of the result. Nothing here parses Markdown: the fence,
//! the directive and the span are whatever `limae --fix` says they are.
//!
//! Caching a batch is [`crate::hook::state::keep`]: it creates state, so it
//! lives with the rest of the state rules rather than here.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::config::{self, CliOverrides, ResolvedConfig};
use crate::hook::state::{self, Kind, Step};
use crate::pipeline::Pipeline;

/// How long a batch waits for a slower sibling before it to land.
///
/// A batch knows its own index, and so how many came before it. Waiting this
/// long before giving up and showing the batch as it came is the second line of
/// defence against the host's unserialised runs; the first is that a batch is
/// renamed into place, so a half-written one is never read.
pub const SIBLING_WAIT: Duration = Duration::from_secs(2);
/// How often the wait looks again.
pub const SIBLING_POLL: Duration = Duration::from_millis(20);

/// What one message instance may cost before the hook stops fixing it.
///
/// Every batch fixes the whole message so far, so the work a message costs is
/// the sum over its batches of the text before them — bounded per batch by the
/// text's size, and in total by that times the number of batches. The host
/// allows a batch ten seconds; what these bound is the total, on a message far
/// longer than any reply a person reads on screen. Past a limit the instance
/// is abandoned as a whole: the prefix is never cut short to keep going,
/// because a cut prefix is exactly the missing state the replay exists to
/// carry (ADR-0016 section 二「成本」).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// The most bytes of original text one message may reach, this batch
    /// included.
    pub bytes: usize,
    /// The most batches one message may have.
    pub batches: usize,
    /// How long one batch waits for the batches before it.
    pub wait: Duration,
}

impl Limits {
    /// The limits a hook event runs under.
    ///
    /// A 256 KiB message fixes in well under a second in a release build
    /// (200 KB measured at 151 ms, 2026-09-11), and a thousand batches of it is
    /// minutes of work spread over the minutes such a message takes to stream.
    pub const DEFAULT: Self = Self {
        bytes: 256 * 1024,
        batches: 1000,
        wait: SIBLING_WAIT,
    };
}

/// What caching one batch found already under its index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stored {
    /// Nothing; the batch is now on disk, and it is the one under this index.
    Kept,
    /// The same batch, already there: the host sent it twice, which changes
    /// nothing.
    Repeated,
    /// A different batch under the same index: this is not the same message
    /// instance any more, and nothing was overwritten.
    Conflict,
}

/// What the prefix replay decided for one batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// Show this in place of the batch.
    Fixed(String),
    /// The batch is already what the fixes would make it; the host shows it.
    Unchanged,
    /// The batch is shown as it came, and the diagnostics say why.
    Declined(Step, Kind),
}

/// Cache one batch, unless the same index is already taken.
///
/// Publishing is what decides who was first: [`state::keep`] refuses, across
/// processes and in one step, when a batch is already under this index, and
/// only then is that batch read and compared. Reading first and writing after
/// would let two processes both find nothing and both write.
///
/// # Errors
/// The new batch would not write, or the existing one would not read back.
pub fn store(parts: &Path, index: usize, delta: &str) -> io::Result<Stored> {
    match state::keep(parts, index, delta) {
        Ok(()) => Ok(Stored::Kept),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let existing = fs::read(state::part(parts, index))?;
            Ok(if existing == delta.as_bytes() {
                Stored::Repeated
            } else {
                Stored::Conflict
            })
        }
        Err(error) => Err(error),
    }
}

/// Put the prefix of a message back together from its cached batches.
///
/// `batches` is how many batches come before the one asking, so the prefix is
/// batches `0..batches` in index order and `0` is a prefix with nothing to
/// wait for; `deadline` is when to stop waiting for the missing ones.
/// `directory` and `message` are the session-state directory and the
/// sanitised message id the diagnostics line is written under.
///
/// What this costs is set by the batches that exist on disk, never by
/// `batches`. That number reaches here from the host's payload — it is the
/// index of the batch asking — so anything laid out one per batch is a piece of
/// work the payload gets to size, and a large enough one takes the process
/// down, on a capacity overflow or a failed allocation. Either is an exit code
/// that is not 0 out of a hook event, which ADR-0016 section 一 does not allow.
/// Reading the directory instead also makes the question an honest one:
/// whether a prefix is whole is a question about which batches are in that
/// directory, not about a number somebody sent us.
///
/// Counting them is not enough on its own, which is why the check below is on
/// indices rather than on how many there are: the batch asking, and any after
/// it that have already landed, are in the same directory, so it can hold as
/// many files as `batches` and still be missing one of them.
///
/// `Ok(None)` when a batch never arrived. A prefix with a hole in it carries
/// none of the state the replay exists to carry, so a missing batch ends this
/// batch the way every other failure does, with the original on screen. It
/// says so three ways: a line on `stderr`, which the host debug-logs; a
/// diagnostics line; and the `None` itself.
///
/// # Errors
/// Returns the failure of reading a batch back, or of listing the directory
/// they are in. This is deliberately not folded into `Ok(None)`: a batch that
/// is on disk and unreadable is not a batch that never arrived, and a
/// diagnostics line saying it never arrived would be a record that lies, which
/// is worse than no record. A directory that is not there yet is not a failure
/// to read one — it is a message every batch of which is missing — and reports
/// itself as the hole it is.
///
/// **The caller owes this error the same fail-open path as every other crash**:
/// [`Kind::Crashed`] in the diagnostics line, and the user's own text on screen.
pub fn assemble(
    parts: &Path,
    batches: usize,
    deadline: Instant,
    directory: &Path,
    message: &str,
    now: SystemTime,
    stderr: &mut dyn Write,
) -> io::Result<Option<String>> {
    loop {
        let found = cached(parts)?;
        if whole(&found, batches) {
            let mut text = String::new();
            for (_, path) in found.iter().take(batches) {
                text.push_str(&fs::read_to_string(path)?);
            }
            return Ok(Some(text));
        }
        if Instant::now() >= deadline {
            // The host debug-logs a hook's stderr. How many batches were
            // missing is the whole of what is worth saying; what they held is
            // the user's own text and stays out of every log. A batch past the
            // prefix is not one of its batches and is not counted as one, or
            // the line would say a batch had come that had not.
            let arrived = found.iter().filter(|(index, _)| *index < batches).count();
            let _ = writeln!(
                stderr,
                "limae hook: {arrived}/{batches} earlier batches arrived before the deadline; showing this one as it came"
            );
            state::note(directory, message, Step::Siblings, Kind::Incomplete, now);
            return Ok(None);
        }
        std::thread::sleep(SIBLING_POLL);
    }
}

/// Fix one batch in the light of everything before it.
///
/// `prefix` is the message before this batch, `delta` the batch, `is_final`
/// whether the host marked it the last, and `cwd` where the rule configuration
/// is looked up from — the same discovery as `limae --fix`, so a repository
/// that has disabled a rule keeps it disabled here (ADR-0016 section 三). A
/// configuration that will not read is the user's to fix and says so by name;
/// a fixer that will not build is a bug.
#[must_use]
pub fn replay(prefix: &str, delta: &str, is_final: bool, cwd: &Path) -> Outcome {
    let Ok(pipeline) = Pipeline::new() else {
        return Outcome::Declined(Step::Fix, Kind::Crashed);
    };
    let Ok(config) = config::resolve(cwd, CliOverrides::default()) else {
        return Outcome::Declined(Step::Fix, Kind::Misconfigured);
    };
    replay_with(&pipeline, &config, prefix, delta, is_final)
}

/// The body of [`replay`], with the fixer and configuration supplied.
///
/// An empty batch has nothing to show and changes no prefix, so it decides
/// nothing. Past that, two signals are read off the text before anything is
/// fixed; both mean "this cannot be decided yet", and both end with the batch
/// shown as it came:
///
/// * [`Kind::Partial`]: a batch boundary that is not a line boundary. The
///   prefix ending mid-line means this batch starts mid-line, and a slice of
///   the fixed text cut at line boundaries would show the first half of that
///   line twice; a middle batch ending mid-line means the host has stopped
///   batching on lines at all (52 of 52 middle batches ended on a newline and
///   0 of 6 final ones did, ADR-0016 读数 A). Neither carries forward: the
///   next batch has a whole prefix and judges for itself.
/// * [`Kind::Unclosed`]: the text so far may still be inside an inline code
///   span. This is the one place the future decides the past — a later batch
///   closing the span would make this one code, and by then this one is on
///   screen and cannot be taken back. A fence has no such problem: its opener
///   is in the prefix, and the prefix is replayed. The final batch has no
///   later batch, so it is never declined for this.
///
/// Then the whole text is fixed and this batch's lines are cut out of the
/// result by counting line feeds: skip as many as the prefix holds, and what
/// remains — carriage returns, the trailing line feed, all of it — is this
/// batch's. The slice is only this batch's because the fixes never add or
/// remove a line (`spec/rules.md`「处理单位」; every golden fixture asserts
/// it), which is why the count is checked rather than assumed: a fixer that
/// broke it is a bug, and the batch is shown as it came.
#[must_use]
pub fn replay_with(
    pipeline: &Pipeline,
    config: &ResolvedConfig,
    prefix: &str,
    delta: &str,
    is_final: bool,
) -> Outcome {
    if delta.is_empty() {
        return Outcome::Unchanged;
    }
    if (!prefix.is_empty() && !prefix.ends_with('\n')) || (!is_final && !delta.ends_with('\n')) {
        return Outcome::Declined(Step::Siblings, Kind::Partial);
    }
    let whole = format!("{prefix}{delta}");
    // The final batch has no batch after it, so nothing can still close a span
    // in it: the message ends where it ends, and the whole is decided.
    if !is_final && pipeline.unclosed_span(&whole) {
        return Outcome::Declined(Step::Siblings, Kind::Unclosed);
    }
    // The one way the fixer refuses a text is an inline directive naming a
    // rule that does not exist. The user wrote that directive, and the answer
    // is the same one the configuration error gets: fix the name.
    let Ok(fixed) = pipeline.fix(&whole, config) else {
        return Outcome::Declined(Step::Fix, Kind::Misconfigured);
    };
    let Some(shown) = slice(&fixed, prefix, delta) else {
        return Outcome::Declined(Step::Fix, Kind::Crashed);
    };
    if shown == delta {
        Outcome::Unchanged
    } else {
        Outcome::Fixed(shown.to_owned())
    }
}

/// Cut this batch's lines out of the fixed whole, or `None` when the fixed
/// whole no longer has the lines the original had.
fn slice<'a>(fixed: &'a str, prefix: &str, delta: &str) -> Option<&'a str> {
    if line_feeds(fixed) != line_feeds(prefix) + line_feeds(delta) {
        return None;
    }
    let skip = line_feeds(prefix);
    let start = if skip == 0 {
        0
    } else {
        fixed
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .nth(skip - 1)
            .map(|(at, _)| at + 1)?
    };
    let shown = &fixed[start..];
    (shown.ends_with('\n') == delta.ends_with('\n')).then_some(shown)
}

/// How many line feeds a text holds, which is how the fixer counts lines.
fn line_feeds(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count()
}

/// Every batch on disk under this message instance right now, by index, in
/// index order.
///
/// This listing is what bounds the whole of [`assemble`]: it is as long as the
/// directory is, whatever number the payload named.
fn cached(parts: &Path) -> io::Result<Vec<(usize, PathBuf)>> {
    let listing = match fs::read_dir(parts) {
        Ok(listing) => listing,
        // Nothing has been cached under this instance, which is a message
        // with every batch still missing rather than a directory that will not
        // read back.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut found = Vec::new();
    for entry in listing {
        let path = entry?.path();
        // A batch being written is still under its temporary name, so it is not
        // one of these until the rename puts it here; the void marker is not a
        // batch either.
        let Some(index) = indexed(&path) else {
            continue;
        };
        found.push((index, path));
    }
    found.sort_unstable_by_key(|(index, _)| *index);
    Ok(found)
}

/// Return whether `found` opens with every batch of a prefix of `batches`
/// batches.
///
/// The test is that its first `batches` entries are the indices `0..batches`,
/// asserted forwards — `found` is in index order, so the batch in slot `n` has
/// to be batch `n`. How many files are there does not settle it: the batch
/// asking, and any after it, are in the same directory, so it can hold as many
/// as the prefix has and still be missing one of them. Anything past those
/// first `batches` is neither waited for nor read.
fn whole(found: &[(usize, PathBuf)], batches: usize) -> bool {
    found.len() >= batches
        && found
            .iter()
            .take(batches)
            .enumerate()
            .all(|(slot, (index, _))| slot == *index)
}

/// Read the batch index out of a cached batch's path, or `None` when the path
/// is not a cached batch.
fn indexed(path: &Path) -> Option<usize> {
    path.file_name()?
        .to_str()?
        .strip_suffix(state::PART_SUFFIX)?
        .parse()
        .ok()
}

#[cfg(all(test, unix))]
#[path = "parts_tests.rs"]
mod tests;
