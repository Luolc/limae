//! The host protocol of the `hook` subcommand: one process, one event.
//!
//! `docs/adr/0016-hook-mechanical-only.md` is the normative description. What
//! to show is decided by [`crate::hook::parts`]; what is left here is the shape
//! the host speaks in, the bookkeeping of one message instance across the
//! processes that serve its batches, and the entry point that always ends at
//! exit code 0.
//!
//! **One host, one event.** Claude Code's `MessageDisplay` fires once per
//! batch of newly completed lines while an assistant message streams. Each
//! batch is fixed in the light of every batch before it and the answer replaces
//! that batch, as `hookSpecificOutput.displayContent`; a batch the fixes leave
//! alone produces no output at all, which is how the host displays the
//! original. `MessageDisplay` is display-only: the transcript and what the
//! model sees are untouched. Every other event, `Stop` included, is received
//! and left alone.
//!
//! **Failure is silence on screen, and only there.** A missing sibling, a
//! malformed payload, a configuration that will not read, a bug in this file:
//! every one of them ends the same way, with no output and the user's own text
//! on screen (ADR-0016 section 一). The exit code is 0 for every one of them,
//! which is the opposite of the CLI's contract (ADR-0008 section 六) and
//! deliberately so — this code sits in front of every reply the user reads, so
//! the worst thing it can do is get in the way. The one exit code that is not 0
//! belongs to a person who ran the subcommand by hand, which is not a hook
//! event at all.
//!
//! Silent on screen is not the same as silent everywhere: each of those paths
//! also writes one line to the session's diagnostics, because failing open and
//! leaving no trace are two different things and only the first one was ever
//! the intention.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use serde_json::{Map, Value, json};

use super::parts::{self, Limits, Outcome, Stored};
use super::state::{self, Kind, Step};
use crate::polish::engines::HOOK_DISABLE_VARIABLE;
use crate::polish::value;

/// What this subcommand is called on the command line.
pub const SUBCOMMAND: &str = "hook";
/// Claude Code's per-batch display event, the one event this acts on.
pub const MESSAGE_DISPLAY: &str = "MessageDisplay";

/// What a hook event exits with, whatever happened (ADR-0016 section 一).
pub const OK: u8 = 0;
/// What a person who ran this by hand with arguments gets.
pub const BAD_USAGE: u8 = 2;

/// Run the `hook` subcommand.
///
/// `args` is what followed `hook`; there are none, because the event names
/// itself in the payload. `cwd` is the directory the rule configuration is
/// looked up from when the payload does not say, `stdin` carries the one JSON
/// event, and `stdout` takes at most one JSON object back.
///
/// Always [`OK`] for a hook event; [`BAD_USAGE`] only when a person ran this by
/// hand with arguments.
pub fn run(
    args: &[OsString],
    cwd: &Path,
    env: &[(OsString, OsString)],
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    now: SystemTime,
) -> u8 {
    serve(args, cwd, env, stdin, stdout, stderr, now, &Limits::DEFAULT)
}

/// The body of [`run`], with the limits a message is held to passed in so that
/// a test can reach them.
#[expect(
    clippy::too_many_arguments,
    reason = "every process boundary is a parameter, so that no test mutates \
              process-global state; the limits are the one addition"
)]
pub fn serve(
    args: &[OsString],
    cwd: &Path,
    env: &[(OsString, OsString)],
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    now: SystemTime,
    limits: &Limits,
) -> u8 {
    if !args.is_empty() {
        let _ = writeln!(
            stderr,
            "usage: limae {SUBCOMMAND} — reads one hook event as JSON on stdin"
        );
        return BAD_USAGE;
    }
    // Before stdin is even read: a session that has switched the hook off is
    // owed a process that does nothing, not one that does nothing slowly.
    if value(env, HOOK_DISABLE_VARIABLE).is_some() {
        return OK;
    }
    // A payload this cannot read is one more thing not worth interrupting a
    // reply over, so stdin that is not JSON, or JSON that is not an object,
    // ends the same way every other failure does.
    let Ok(Value::Object(payload)) = serde_json::from_reader::<_, Value>(stdin) else {
        return OK;
    };
    match output(&payload, env, cwd, now, stderr, limits) {
        Ok(Some(answer)) => {
            // Non-ASCII goes out as itself, which is `ensure_ascii=False`:
            // what this prints is Chinese prose bound for a screen.
            let _ = writeln!(stdout, "{answer}");
            OK
        }
        Ok(None) => OK,
        Err(_) => {
            // Fail open, and mean it: nothing this file can go wrong at is
            // worth showing the user instead of their own reply (ADR-0016
            // section 一). Silent on screen is not the same as silent
            // everywhere, so a crash says so where the other failures do —
            // best effort, since the state directory is exactly the sort of
            // thing that may be why we are here.
            //
            // This is also where [`parts::assemble`]'s read failure lands. It
            // is returned rather than raised, so the mapping is written here
            // instead of inherited: a batch that is on disk and unreadable is a
            // crash of this code, not the missing batch `Kind::Incomplete`
            // would claim it was.
            let session = state::identifier(payload.get("session_id").and_then(Value::as_str));
            if let Some(root) = state::root(env)
                && !session.is_empty()
            {
                state::note(
                    &root.join(session),
                    &named(&payload, "message_id"),
                    Step::Display,
                    Kind::Crashed,
                    now,
                );
            }
            OK
        }
    }
}

/// Handle one host event and build its wire output.
///
/// `None` when there is nothing to say, which is every event but
/// `MessageDisplay` and every fail-open path: no JSON at all is printed, and
/// the host displays what it already had.
///
/// # Errors
/// Whatever the batch's handler could not do — a state directory that will not
/// hold the batch, a cached batch that will not read back. The caller owes all
/// of it the same fail-open path.
fn output(
    payload: &Map<String, Value>,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
    stderr: &mut dyn Write,
    limits: &Limits,
) -> io::Result<Option<Value>> {
    if payload.get("hook_event_name").and_then(Value::as_str) != Some(MESSAGE_DISPLAY) {
        return Ok(None);
    }
    let answer = display(payload, env, cwd, now, stderr, limits)?;
    Ok((!answer.is_empty()).then(|| {
        json!({
            "hookSpecificOutput": {
                "hookEventName": MESSAGE_DISPLAY,
                "displayContent": answer,
            }
        })
    }))
}

/// Handle one `MessageDisplay` batch.
///
/// Returns what to display in place of this batch, empty to leave it alone.
///
/// The order of the checks is the order the costs come in. The instance's
/// standing — abandoned, or over the batch limit — is settled before the batch
/// is even cached; the batch is cached before the wait, so that the siblings
/// after it can find it; the size limit is checked once the prefix is there to
/// measure; and the fixer runs last. Abandoning an instance is permanent and
/// marks the directory rather than deleting it, so that a sibling still waiting
/// on it finds what it was waiting for.
///
/// # Errors
/// The state directory would not take the batch or the mark, or a cached batch
/// would not read back ([`parts::assemble`]).
fn display(
    payload: &Map<String, Value>,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
    stderr: &mut dyn Write,
    limits: &Limits,
) -> io::Result<String> {
    let session = named(payload, "session_id");
    let message = named(payload, "message_id");
    let turn = named(payload, "turn_id");
    let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
        return Ok(String::new());
    };
    let Some(root) = state::root(env) else {
        return Ok(String::new());
    };
    // The turn is half of the instance key (ADR-0016 section 二): without it
    // there is no directory this batch belongs in.
    if session.is_empty() || message.is_empty() || turn.is_empty() {
        return Ok(String::new());
    }
    // A batch index is a whole non-negative number; JSON's booleans are a
    // separate variant of `Value`, so `as_u64` already answers `None` to one.
    let Some(index) = payload
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
    else {
        return Ok(String::new());
    };
    let is_final = payload.get("final") == Some(&Value::Bool(true));
    let directory = state::session(&root, &session)?;
    let parts = directory
        .join(state::PARTS_DIRECTORY)
        .join(state::instance(&message, &turn));
    // Every batch sweeps: no batch is the last one to run for its message
    // (ADR-0016 section 二「清理」), so there is no better moment, and a sweep
    // is a listing of directories an hour or a day old.
    state::prune(&root, now);
    let declined = |step: Step, kind: Kind| {
        state::note(&directory, &message, step, kind, now);
        Ok(String::new())
    };
    if state::voided(&parts) {
        return declined(Step::Siblings, Kind::Incomplete);
    }
    if index >= limits.batches {
        state::void(&parts)?;
        return declined(Step::Siblings, Kind::Incomplete);
    }
    if parts::store(&parts, index, delta)? == Stored::Conflict {
        state::void(&parts)?;
        return declined(Step::Siblings, Kind::Incomplete);
    }
    // An empty batch — the final one, when the message ends on a line feed —
    // has nothing to show, and nothing to wait for either.
    if delta.is_empty() {
        return Ok(String::new());
    }
    let Some(prefix) = parts::assemble(
        &parts,
        index,
        Instant::now() + limits.wait,
        &directory,
        &message,
        now,
        stderr,
    )?
    else {
        return Ok(String::new());
    };
    if prefix.len().saturating_add(delta.len()) > limits.bytes {
        state::void(&parts)?;
        return declined(Step::Siblings, Kind::Incomplete);
    }
    match parts::replay(&prefix, delta, is_final, &configured(payload, cwd)) {
        Outcome::Fixed(shown) => Ok(shown),
        Outcome::Unchanged => Ok(String::new()),
        Outcome::Declined(step, kind) => declined(step, kind),
    }
}

/// Return one of the host's ids as a safe path segment, empty when absent.
fn named(payload: &Map<String, Value>, key: &str) -> String {
    state::identifier(payload.get(key).and_then(Value::as_str))
}

/// Return the directory this event's rule configuration is looked up from.
///
/// The host says where the session is; this process's own working directory is
/// the fallback.
fn configured(payload: &Map<String, Value>, cwd: &Path) -> PathBuf {
    payload
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| cwd.to_owned(), PathBuf::from)
}

// Unix-only: every path here writes session state, and on another platform every
// one of those calls reports that it cannot.
#[cfg(all(test, unix))]
#[path = "cli_tests.rs"]
mod tests;
