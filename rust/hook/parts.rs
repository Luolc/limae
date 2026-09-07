//! The pieces of a message, and the knobs that decide how long to wait for
//! them.
//!
//! Claude Code's `MessageDisplay` fires once per batch of newly completed lines
//! while an assistant message streams, so a whole message only exists once its
//! final batch has arrived and every batch before it has been read back
//! (ADR-0009 section 二). [`assemble`] is that read-back. The host starts one
//! process per batch and does not wait for it before starting the next
//! (2026-09-01, Claude Code 2.1.257: the dispatcher only serialises what the
//! answers do to the screen, not the runs), so the final batch can be running
//! while a sibling is still writing — hence a wait, and hence a poll.
//!
//! Caching a batch is [`crate::hook::state::keep`]: it creates state, so it
//! lives with the rest of the state rules rather than here.
//!
//! [`tidy`] is the other half of what reaches the screen. ADR-0005 section 四
//! draws the line it walks: meaning is the model's half, typography is the
//! rules' half, and a rewrite is not exempt from the rules just because a model
//! wrote it.
//!
//! [`number`] reads the numeric knobs ADR-0009 leaves open, all from the
//! environment because that is what a hook has.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use crate::config::{self, CliOverrides};
use crate::hook::state::{self, Kind, Step};
use crate::pipeline::Pipeline;
use crate::polish::value;

/// How long the final batch waits for a slower sibling to land.
///
/// The final batch knows its own index, and so how many came before it. Waiting
/// this long before giving up and polishing what it has is the second line of
/// defence against the host's unserialised runs; the first is that a batch is
/// renamed into place, so a half-written one is never read.
pub const SIBLING_WAIT: Duration = Duration::from_secs(2);
/// How often the wait looks again.
pub const SIBLING_POLL: Duration = Duration::from_millis(20);

/// Run this repository's own deterministic fixes over one rewrite.
///
/// Only the rewrite goes through here. The original is the user's own text and
/// this hook has no business changing it; it has also already been displayed,
/// batch by batch, by the time there is anything to fix.
///
/// `cwd` is where the rule configuration is looked up from, so a repository
/// that has disabled a rule keeps it disabled here.
///
/// Returns the fixed text and no failure; or the text unchanged and
/// [`Kind::Crashed`] when the fixer would not run — a rewrite with a typography
/// slip in it still beats no rewrite at all, so nothing here is reported
/// upwards as an error.
#[must_use]
pub fn tidy(text: &str, cwd: &Path) -> (String, Option<Kind>) {
    match fixed(text, cwd) {
        Some(fixed) => (fixed, None),
        None => (text.to_owned(), Some(Kind::Crashed)),
    }
}

/// Put a message back together from its cached batches.
///
/// `batches` is how many this message had, the final one included; `deadline`
/// is when to stop waiting for the missing ones. `directory` and `message` are
/// the session-state directory and the sanitised message id the diagnostics
/// line is written under.
///
/// `Ok(None)` when a batch never arrived. There is no honest rewrite of a
/// message with a hole in it — polishing what did arrive would put a paragraph
/// the user never wrote under a message that says it is theirs — so a missing
/// batch ends the turn the way every other failure does, with the original on
/// screen. It says so three ways: a line on `stderr`, which the host
/// debug-logs; a diagnostics line; and the `None` itself.
///
/// # Errors
/// Returns the failure of reading a batch back. This is deliberately not folded
/// into `Ok(None)`: a batch that is on disk and unreadable is not a batch that
/// never arrived, and a diagnostics line saying it never arrived would be a
/// record that lies, which is worse than no record.
///
/// **The caller owes this error the same fail-open path as every other crash**:
/// [`Kind::Crashed`] in the diagnostics line, and the user's own text on screen.
/// The reference implementation gets there by raising out of `_assemble` into
/// the hook's top-level catch (`display` / `crashed`); here the error is
/// returned instead, so the mapping has to be written rather than inherited. A
/// caller that reports it as [`Kind::Incomplete`], or that returns a partial
/// message, has diverged from `src/limae/hook.py`.
pub fn assemble(
    parts: &Path,
    batches: usize,
    deadline: Instant,
    directory: &Path,
    message: &str,
    now: SystemTime,
    stderr: &mut dyn Write,
) -> io::Result<Option<String>> {
    let wanted: Vec<_> = (0..batches)
        .map(|index| state::part(parts, index))
        .collect();
    while !wanted.iter().all(|path| path.is_file()) {
        if Instant::now() >= deadline {
            // The host debug-logs a hook's stderr. How many batches were
            // missing is the whole of what is worth saying; what they held is
            // the user's own text and stays out of every log.
            let arrived = wanted.iter().filter(|path| path.is_file()).count();
            let _ = writeln!(
                stderr,
                "limae hook: {arrived}/{batches} batches arrived before the deadline; showing the original"
            );
            state::note(directory, message, Step::Assemble, Kind::Incomplete, now);
            return Ok(None);
        }
        std::thread::sleep(SIBLING_POLL);
    }
    let mut whole = String::new();
    for path in &wanted {
        whole.push_str(&std::fs::read_to_string(path)?);
    }
    Ok(Some(whole))
}

/// Read one numeric knob from the environment.
///
/// `fallback` is used when the variable is unset, empty, or says something that
/// is not a number, because a typo in a setting is not a reason to interrupt
/// the user.
///
/// Two forms are a number to the reference implementation's `float()` and a
/// typo here: PEP 515 digit separators (`"1_000"`) and non-ASCII decimal digits
/// (`"１２３"`), measured against Python 3 on 2026-09-07. The difference is left
/// standing rather than coded around, because it falls the harmless way —
/// nobody types either into a setting, and where they differ this returns the
/// documented default, which is what `float()` itself does with a typo.
///
/// Surrounding whitespace is taken by both, but only because of the [`str::trim`]
/// below — `f64::from_str` alone refuses it, and a stray space in a settings
/// file is a real typo rather than a hypothetical one. The two definitions of
/// whitespace are not the same set (`str::trim` follows `char::is_whitespace`,
/// Python's `str.strip` its own table); the ASCII ones agree, and the code
/// points they disagree on — U+001C to U+001F among them — were not measured.
#[must_use]
pub fn number(env: &[(OsString, OsString)], variable: &str, fallback: f64) -> f64 {
    value(env, variable)
        .and_then(|text| text.to_str())
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(fallback)
}

/// The body of [`tidy`], with every way it can decline folded into `None`.
fn fixed(text: &str, cwd: &Path) -> Option<String> {
    let pipeline = Pipeline::new().ok()?;
    let config = config::resolve(cwd, CliOverrides::default()).ok()?;
    pipeline.fix(text, &config).ok()
}

#[cfg(all(test, unix))]
#[path = "parts_tests.rs"]
mod tests;
