//! The host protocol of the `hook` subcommand: one process, one event.
//!
//! `src/limae/hook.py` is the reference implementation and
//! `docs/adr/0009-polish-hook-contract.md` the normative description. Every
//! decision about *what* to show is already made by the time this module runs
//! — [`crate::hook::block`] makes it — so what is left here is the shape the
//! two hosts speak in, and the entry point that always ends at exit code 0.
//!
//! **Two hosts, three events, three shapes on the wire.**
//!
//! * Claude Code's `MessageDisplay` fires once per batch of newly completed
//!   lines while an assistant message streams, so the batches are cached by
//!   message id and the model is called once, on the batch marked `final`, over
//!   the whole message (ADR-0009 section 二). The answer replaces that batch, as
//!   `hookSpecificOutput.displayContent`; a middle batch produces no output at
//!   all, which is how the host displays the original.
//! * Claude Code's `Stop` is the other half of the A/B trial
//!   ([`crate::hook::ab`]): a `MessageDisplay` rewrite is invisible to the
//!   model, so the code name is handed over here as
//!   `hookSpecificOutput.additionalContext`.
//! * Codex has no display-replacement event. Its `Stop` supplies the whole
//!   `last_assistant_message` instead, so that host needs no batch cache: the
//!   rewrite is returned as a `systemMessage` warning below the original reply.
//!   It does not replace the input message or return any continuation field
//!   (ADR-0014).
//!
//! **Failure is silence on screen, and only there.** A missing engine, a
//! timeout, an empty answer, a malformed payload, a bug in this file: every one
//! of them ends the same way, with no output and the user's own text on screen
//! (ADR-0009 section 六). The exit code is 0 for every one of them, which is the
//! opposite of the CLI's contract (ADR-0008 section 六) and deliberately so —
//! this code sits in front of every reply the user reads, so the worst thing it
//! can do is get in the way. The one exit code that is not 0 belongs to a person
//! who ran the subcommand by hand, which is not a hook event at all.
//!
//! Silent on screen is not the same as silent everywhere: each of those paths
//! also writes one line to the session's diagnostics, because failing open and
//! leaving no trace are two different things and only the first one was ever the
//! intention.

use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use serde_json::{Map, Value, json};

use super::state::{self, Kind, Step};
use super::{ab, block, parts, render};
use crate::polish::engines::HOOK_DISABLE_VARIABLE;
use crate::polish::value;
use crate::text::is_python_whitespace;

/// What this subcommand is called on the command line.
pub const SUBCOMMAND: &str = "hook";
/// Claude Code's per-batch display event.
pub const MESSAGE_DISPLAY: &str = "MessageDisplay";
/// The end-of-turn event both hosts send, with different payloads.
pub const STOP: &str = "Stop";

/// What a hook event exits with, whatever happened (ADR-0009 section 六).
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
    match output(&payload, env, cwd, now, stderr) {
        Ok(Some(answer)) => {
            // Non-ASCII goes out as itself, which is `ensure_ascii=False`:
            // what this prints is Chinese prose bound for a screen.
            let _ = writeln!(stdout, "{answer}");
            OK
        }
        Ok(None) => OK,
        Err(_) => {
            // Fail open, and mean it: nothing this file can go wrong at is
            // worth showing the user instead of their own reply (ADR-0009
            // section 六). Silent on screen is not the same as silent
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
                    &message(&payload),
                    Step::Display,
                    Kind::Crashed,
                    now,
                );
            }
            OK
        }
    }
}

/// Handle one supported host event and build its wire output.
///
/// `None` when there is nothing to say, which is every unsupported event and
/// every fail-open path: no JSON at all is printed, and the host displays what
/// it already had.
///
/// # Errors
/// Whatever the event's handler could not do — a state directory that will not
/// hold the batches, a cached batch that will not read back. The caller owes all
/// of it the same fail-open path.
fn output(
    payload: &Map<String, Value>,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
    stderr: &mut dyn Write,
) -> io::Result<Option<Value>> {
    let event = payload.get("hook_event_name").and_then(Value::as_str);
    if event == Some(MESSAGE_DISPLAY) {
        let answer = display(payload, env, cwd, now, stderr)?;
        if !answer.is_empty() {
            return Ok(Some(json!({
                "hookSpecificOutput": {
                    "hookEventName": MESSAGE_DISPLAY,
                    "displayContent": answer,
                }
            })));
        }
    } else if event == Some(STOP) && is_codex_stop(payload) {
        let answer = codex_stop(payload, env, cwd, now)?;
        if !answer.is_empty() {
            return Ok(Some(json!({ "systemMessage": answer })));
        }
    } else if event == Some(STOP) {
        let answer = stop(payload, env)?;
        if !answer.is_empty() {
            return Ok(Some(json!({
                "hookSpecificOutput": {
                    "hookEventName": STOP,
                    "additionalContext": answer,
                }
            })));
        }
    }
    Ok(None)
}

/// Handle one `MessageDisplay` batch.
///
/// Returns what to display in place of this batch, empty to leave it alone.
///
/// # Errors
/// The state directory would not take the batch, or a cached batch would not
/// read back ([`parts::assemble`]).
fn display(
    payload: &Map<String, Value>,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
    stderr: &mut dyn Write,
) -> io::Result<String> {
    let session = state::identifier(payload.get("session_id").and_then(Value::as_str));
    let message = state::identifier(payload.get("message_id").and_then(Value::as_str));
    let Some(delta) = payload.get("delta").and_then(Value::as_str) else {
        return Ok(String::new());
    };
    let Some(root) = state::root(env) else {
        return Ok(String::new());
    };
    if session.is_empty() || message.is_empty() {
        return Ok(String::new());
    }
    // A batch index is a whole non-negative number and nothing else. The
    // reference implementation has to say so twice — `isinstance(index, int)`
    // and then `not isinstance(index, bool)`, because in Python `True` is an
    // `int` — where here JSON's booleans and its numbers are separate variants
    // of `Value` and `as_u64` answers `None` to a boolean already.
    let Some(index) = payload
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
    else {
        return Ok(String::new());
    };
    let directory = state::session(&root, &session)?;
    let parts = directory.join(state::PARTS_DIRECTORY).join(&message);
    state::keep(&parts, index, delta)?;
    // `final` is the end-of-message signal whatever the delta holds: the last
    // batch is empty when the message ends on a newline.
    if payload.get("final") != Some(&Value::Bool(true)) {
        return Ok(String::new());
    }
    // Indices are zero-based and increment by one per batch, so the final one
    // says how many there are.
    let text = parts::assemble(
        &parts,
        index + 1,
        Instant::now() + parts::SIBLING_WAIT,
        &directory,
        &message,
        now,
        stderr,
    )?;
    // The batches of a finished message are scratch and go now, whether or not
    // they made a whole message; a sweep that will not run is not a reason to
    // hold up the one that will.
    let _ = std::fs::remove_dir_all(&parts);
    state::prune(&root, now);
    let Some(text) = text else {
        return Ok(String::new());
    };
    let built = block::block(
        &text,
        &directory,
        &message,
        env,
        &configured(payload, cwd),
        now,
    );
    if built.is_empty() {
        return Ok(String::new());
    }
    Ok(joined(delta, &text, &built))
}

/// Put one blank line between the message and the block, whatever the message
/// happens to end on.
///
/// `delta` is the final batch, which this answer replaces, and `text` is the
/// whole message it closes.
///
/// The gap is a property of the screen, not of this batch: `displayContent`
/// replaces the final delta and nothing before it, so the trailing newlines an
/// earlier batch already painted cannot be taken back — only counted. A message
/// ending on a newline is exactly that case, since its final delta is empty and
/// every newline is already up there.
fn joined(delta: &str, text: &str, block: &str) -> String {
    let painted = ending(text) - ending(delta);
    let gap = usize::try_from(i64::try_from(render::BLOCK_GAP).unwrap_or(0) - painted).unwrap_or(0);
    format!(
        "{}{}{block}",
        delta.trim_end_matches('\n'),
        "\n".repeat(gap)
    )
}

/// Handle one Claude Code `Stop` event.
///
/// Returns the context to hand the model, empty when the turn that just ended
/// had no A/B trial.
///
/// # Errors
/// The session-state directory would not open.
fn stop(payload: &Map<String, Value>, env: &[(OsString, OsString)]) -> io::Result<String> {
    let session = state::identifier(payload.get("session_id").and_then(Value::as_str));
    let Some(root) = state::root(env) else {
        return Ok(String::new());
    };
    if session.is_empty() {
        return Ok(String::new());
    }
    Ok(ab::context(&state::session(&root, &session)?))
}

/// Polish the complete reply carried by a Codex `Stop` event.
///
/// Returns the warning to append below the original reply, empty when the
/// payload is incomplete or polishing fails open.
///
/// # Errors
/// The session-state directory would not open.
fn codex_stop(
    payload: &Map<String, Value>,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
) -> io::Result<String> {
    let session = state::identifier(payload.get("session_id").and_then(Value::as_str));
    let message = message(payload);
    let text = payload
        .get("last_assistant_message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(root) = state::root(env) else {
        return Ok(String::new());
    };
    if session.is_empty() || message.is_empty() || text.is_empty() {
        return Ok(String::new());
    }
    let directory = state::session(&root, &session)?;
    state::prune(&root, now);
    let built = block::block(
        text,
        &directory,
        &message,
        env,
        &configured(payload, cwd),
        now,
    );
    if built.is_empty() {
        return Ok(String::new());
    }
    let built = built.trim_end_matches(is_python_whitespace);
    // `ab::record` leaves a pending note for Claude Code's second `Stop` hook.
    // Codex has only this one event, and returning `decision:block` to
    // manufacture another would run the model again. Consume the note here; the
    // comparison and its code remain in the ledger and on screen. A
    // `systemMessage` is not ADR-0009's model-context channel, so this preserves
    // the reader's code without claiming the model received it.
    let context = ab::context(&directory);
    Ok(if context.is_empty() {
        built.to_owned()
    } else {
        format!("{built}\n\n{context}")
    })
}

/// Return whether a `Stop` payload carries Codex's extensions.
fn is_codex_stop(payload: &Map<String, Value>) -> bool {
    payload.get("model").is_some_and(Value::is_string)
        && payload.contains_key("last_assistant_message")
}

/// Return the host's safe per-message identifier.
fn message(payload: &Map<String, Value>) -> String {
    let named = |key: &str| state::identifier(payload.get(key).and_then(Value::as_str));
    let id = named("message_id");
    if id.is_empty() { named("turn_id") } else { id }
}

/// Return the directory this event's rule configuration is looked up from.
///
/// The host says where the session is; this process's own working directory is
/// the fallback, which is where the reference implementation's `Path.cwd()`
/// lands.
fn configured(payload: &Map<String, Value>, cwd: &Path) -> PathBuf {
    payload
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| cwd.to_owned(), PathBuf::from)
}

/// The trailing newlines of one piece of text, as [`joined`] counts them.
///
/// Signed, because the reference implementation's subtraction is: the `max(0, …)`
/// there is what puts the floor back, and doing it in unsigned arithmetic would
/// move the floor one step earlier.
fn ending(text: &str) -> i64 {
    i64::try_from(render::trailing_newlines(text)).unwrap_or(i64::MAX)
}

// Unix-only: every path here writes session state, and on another platform every
// one of those calls reports that it cannot.
#[cfg(all(test, unix))]
#[path = "cli_tests.rs"]
mod tests;
