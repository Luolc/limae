//! What each engine last did, remembered with a TTL (ADR-0008 section 三 step 5).
//!
//! One answer per engine is stored — a probe's, or the state a real call
//! observed when it failed — never which engine was chosen: the choice runs
//! through the six steps on every run, so an answer cached outside a session
//! cannot outrank the session the user is in now (step 2). The cache only saves
//! the model call an answer would cost.
//!
//! Nothing but an engine name, a state and a timestamp is written. No child
//! output and no credential ever reaches this file, because this repository is
//! public and the file is the user's own (`AGENTS.md` 「隐私边界」).
//!
//! A cache that cannot be written changes nothing but the cost of the next run,
//! so a write failure is silent. A cache that cannot be read, or whose entries
//! are malformed, is treated as absent entry by entry: this file is a plain
//! JSON document in the user's home and anything at all can be in it.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};

use super::diagnosis::EngineState;
use super::engines::{ENGINES, Engine};
use super::{home, value};

/// How long a remembered [`EngineState::Ok`] is trusted.
///
/// A cached answer is a claim about the future, and the two directions claim
/// opposite things. An `Ok` is a permit — "this engine will still work" — and
/// that flip, working to broken, can happen in an instant: the login expires, a
/// quota runs out, a rate limit lands. A failure is a veto — "this engine still
/// will not work" — and undoing it normally takes a person: a login, a config
/// change, a new key. The veto's direction is the slower one.
///
/// The bound on a TTL is the smaller of how fast that direction can flip and
/// how long it takes us to notice a flip. [`super::select::polish`] writes the
/// state it observes back here on every failed call, so an `Ok` is re-checked
/// by every use made of it and the detection delay for a permit is one call:
/// this value is then an upper bound on how long a stale permit can survive,
/// not a promise that the engine is alive for the hour. Without that
/// self-correction the nominal value would also be the effective one, and an
/// hour of permit against a flip that takes an instant is indefensible on any
/// argument.
pub const CACHE_TTL: Duration = Duration::from_secs(3600);

/// How long a remembered failure is trusted.
///
/// A veto gets no correction of the kind [`CACHE_TTL`] describes — nothing
/// re-runs an engine that is being skipped — so its whole TTL is detection
/// delay and it has to be short by itself. That a stale veto also hurts more
/// (it costs the feature entirely, and lands on the user who has just logged
/// in, while a stale permit costs one reported failure) is a consequence of the
/// same asymmetry, not the reason for it.
pub const FAILURE_CACHE_TTL: Duration = Duration::from_secs(300);

const CACHE_DIRECTORY: &str = "limae";
const CACHE_FILENAME: &str = "engine.json";
const ENGINES_KEY: &str = "engines";
const STATE_KEY: &str = "state";
const AT_KEY: &str = "at";

/// What one engine last did, as the cache remembers it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Observed {
    /// What was found — [`EngineState::Ok`] or one of the diagnoses.
    pub state: EngineState,
    /// How long ago it was found; a diagnosis says this out loud so that a user
    /// who has just logged in knows why the answer has not changed yet.
    pub age: Duration,
}

/// Return the file the engines' answers are remembered in, whether or not it
/// exists.
///
/// `None` when the environment names neither `XDG_CACHE_HOME` nor a home
/// directory; the caller's environment is the whole environment, so there is no
/// ambient fallback and a run without either simply remembers nothing.
#[must_use]
pub fn file(env: &[(OsString, OsString)]) -> Option<PathBuf> {
    let root = match value(env, "XDG_CACHE_HOME") {
        Some(configured) => PathBuf::from(configured),
        None => home(env)?.join(".cache"),
    };
    Some(root.join(CACHE_DIRECTORY).join(CACHE_FILENAME))
}

/// Return every engine's cached answer that is still inside its own TTL.
///
/// Failures are cached too, or a broken engine ahead in the order would cost a
/// real model call on every run — but only for [`FAILURE_CACHE_TTL`]. An answer
/// timestamped in the future is not from a clock this run can reason about and
/// is dropped like any other malformed entry.
#[must_use]
pub fn remembered(path: &Path, now: SystemTime) -> Vec<(&'static Engine, Observed)> {
    let now = epoch_seconds(now);
    entries(path)
        .into_iter()
        .filter_map(|entry| {
            let age = now - entry.at;
            let ttl = if entry.state == EngineState::Ok {
                CACHE_TTL
            } else {
                FAILURE_CACHE_TTL
            };
            (age >= 0.0 && age <= ttl.as_secs_f64()).then(|| {
                (
                    entry.engine,
                    Observed {
                        state: entry.state,
                        age: Duration::from_secs_f64(age),
                    },
                )
            })
        })
        .collect()
}

/// Remember what one engine last did, until its TTL runs out.
///
/// Both a probe and a failed real call write here; the second is what keeps a
/// remembered `Ok` from outliving the engine (see [`CACHE_TTL`]). Whatever is
/// written replaces that engine's entry, TTL included, and the other engines'
/// entries are kept as they are. A custom command is nobody's cached engine and
/// is never written down.
///
/// A cache that cannot be written changes nothing but the cost of the next run,
/// so nothing is reported.
pub fn remember(path: &Path, engine: &Engine, state: EngineState, now: SystemTime) {
    if engine.preset().is_none() {
        return;
    }
    let mut stored = Map::new();
    for entry in entries(path) {
        if entry.engine.name() != engine.name() {
            stored.insert(
                entry.engine.name().to_owned(),
                stored_entry(entry.state, entry.at),
            );
        }
    }
    stored.insert(
        engine.name().to_owned(),
        stored_entry(state, epoch_seconds(now)),
    );
    let document = json!({ ENGINES_KEY: stored });
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let _ = fs::write(path, document.to_string());
}

/// Say how old a cached answer is, for a diagnosis.
///
/// The answer may have come from a probe or from a real call that failed, so
/// the phrase says when it was found and not what found it.
#[must_use]
pub fn ago(age: Duration) -> String {
    let minutes = age.as_secs() / 60;
    if minutes < 1 {
        "last checked under a minute ago".to_owned()
    } else {
        format!("last checked {minutes} minute(s) ago")
    }
}

/// One stored answer that survived validation.
struct Entry {
    engine: &'static Engine,
    state: EngineState,
    at: f64,
}

/// Read the cache file's per-engine entries, dropping anything malformed.
///
/// Iterating [`ENGINES`] is what drops an unknown engine name; the rest of the
/// shape is checked field by field. An entry whose state is not one this build
/// knows is dropped too, which is the one place this is stricter than the
/// reference implementation: only a hand-edited file can hold such a state, and
/// the alternative is a diagnosis line with no next step to give.
fn entries(path: &Path) -> Vec<Entry> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_reader::<_, Value>(BufReader::new(file)) else {
        return Vec::new();
    };
    let Some(stored) = document.get(ENGINES_KEY).and_then(Value::as_object) else {
        return Vec::new();
    };
    ENGINES
        .iter()
        .filter_map(|engine| {
            let entry = stored.get(engine.name())?.as_object()?;
            Some(Entry {
                engine,
                state: EngineState::parse(entry.get(STATE_KEY)?.as_str()?)?,
                at: entry.get(AT_KEY)?.as_f64()?,
            })
        })
        .collect()
}

fn stored_entry(state: EngineState, at: f64) -> Value {
    json!({ STATE_KEY: state.as_str(), AT_KEY: at })
}

fn epoch_seconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs_f64(),
        Err(before) => -before.duration().as_secs_f64(),
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
