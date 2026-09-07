//! The `auto` search's ordering and liveness probe (ADR-0008 section 三).
//!
//! Steps 2 to 4 order the candidates without running anything: the host whose
//! session we are inside comes first, a credential trace only sorts, and a
//! missing binary is the only hard negative. Step 5 asks the survivors, in that
//! order, the smallest question there is.
//!
//! Credentials are only ever tested for existence. A variable is looked up by
//! name and a login file is opened to look for one key; nothing read here
//! reaches a return value, an error, a log or a diagnostic, because this
//! repository is public (`AGENTS.md` 「隐私边界」).
//!
//! The environment passed in is the whole environment of the run. This module
//! never falls back to the ambient process environment: an ordering that
//! depended on which CLIs happen to be installed for whoever is running the
//! tests would be untestable, and a caller that wants the process environment
//! passes it.

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::diagnosis::{EngineState, Signs};
use super::engines::{ENGINES, Engine, EngineError, EngineLimits, EngineRequest, polish};
use super::process::{CancellationToken, ProcessError};

/// The token the probe asks for and accepts as proof of life.
pub const PROBE_MARKER: &str = "LIMAE-PROBE-OK";

/// The probe's whole prompt spec.
///
/// This is deliberately the cheapest possible check, and it is not a persona
/// test: the PONG acceptance of ADR-0008 section 四 is a once-per-model manual
/// step, kept off the hot path because it is slow and, under a long spec, not
/// stable enough to gate every run on
/// (`docs/research/polish-engine-cli-behavior.md` section 六).
pub const PROBE_SPEC: &str = "Reply with this marker and nothing else: LIMAE-PROBE-OK";

/// The prose a probe hands the engine.
pub const PROBE_INPUT: &str = "probe";

/// How long one probe is allowed to take, from the reference implementation.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(90);

/// Return the home directory of a run.
///
/// `None` when the environment carries no usable `HOME`; the caller's
/// environment is the whole environment, so there is no ambient fallback.
#[must_use]
pub fn home(env: &[(OsString, OsString)]) -> Option<PathBuf> {
    value(env, "HOME").map(PathBuf::from)
}

/// Return whether an engine's CLI is on `PATH` (step 3).
///
/// This is the `auto` search's only hard negative. A custom command is never
/// searched for: it is the user's own, and this module does not know what its
/// first word means.
#[must_use]
pub fn installed(engine: &Engine, env: &[(OsString, OsString)]) -> bool {
    let Some(preset) = engine.preset() else {
        return false;
    };
    let Some(path) = value(env, "PATH") else {
        return false;
    };
    std::env::split_paths(path).any(|directory| executable(&directory.join(preset.binary)))
}

/// Return the engine whose own session we are running inside (step 2).
///
/// Only variables a CLI always sets in its own sessions count; one that varies
/// with the installation method is not a marker (ADR-0008 section 三 step 2).
#[must_use]
pub fn host(env: &[(OsString, OsString)]) -> Option<&'static Engine> {
    ENGINES.iter().find(|engine| {
        engine.preset().is_some_and(|preset| {
            preset
                .host_env
                .iter()
                .any(|variable| value(env, variable).is_some())
        })
    })
}

/// Return the installed engines, best candidate first (steps 2 to 4).
///
/// The host we are running inside comes first — the user is already paying for
/// that session — then the engines showing a credential trace, then the rest,
/// each group keeping the order of [`ENGINES`]. A credential trace only sorts,
/// never excludes: [`has_credentials`] returning false does not mean the engine
/// is logged out.
///
/// Empty when no preset CLI is installed at all.
#[must_use]
pub fn order(env: &[(OsString, OsString)]) -> Vec<&'static Engine> {
    let inside = host(env).map(Engine::name);
    let mut candidates: Vec<&'static Engine> = ENGINES
        .iter()
        .filter(|engine| installed(engine, env))
        .collect();
    // A stable sort is what keeps `ENGINES` order inside each group, which is
    // the reference implementation's secondary key.
    candidates.sort_by_key(|engine| {
        if Some(engine.name()) == inside {
            0
        } else if has_credentials(engine, env) {
            1
        } else {
            2
        }
    });
    candidates
}

/// Return whether an engine leaves a trace of being logged in (step 4).
///
/// Only existence is tested: the login file is opened to look for one key, and
/// the variables are looked up by name. Nothing read here is returned, logged
/// or reported.
///
/// False does not mean "not logged in" — an API key behind a custom base URL,
/// or a credential fetched by an external command, leaves no trace this
/// function can see — which is why this only sorts the candidates and why the
/// probe, not this, decides whether an engine is usable.
#[must_use]
pub fn has_credentials(engine: &Engine, env: &[(OsString, OsString)]) -> bool {
    let Some(preset) = engine.preset() else {
        return false;
    };
    if preset
        .credential_env
        .iter()
        .any(|variable| value(env, variable).is_some())
    {
        return true;
    }
    let Some(home) = home(env) else {
        return false;
    };
    let path = home.join(preset.auth_file);
    let Some(key) = preset.auth_key else {
        return path.is_file();
    };
    let Ok(file) = File::open(&path) else {
        return false;
    };
    // The parsed document is examined for one key and dropped here; only the
    // answer to "is that key present" leaves this function.
    let Ok(state) = serde_json::from_reader::<_, serde_json::Value>(BufReader::new(file)) else {
        return false;
    };
    state.get(key).is_some()
}

/// Ask one engine the smallest question there is (step 5).
///
/// The engine is alive when its answer carries [`PROBE_MARKER`]. An answer
/// without it — an authentication error printed on stdout under a zero exit
/// code, an error body from a gateway — is read for what went wrong and
/// otherwise counts as a failure, so `auto` moves on instead of trusting an
/// exit code that says nothing.
///
/// A preset that failed and shows no credential trace is reported as
/// [`EngineState::NoCredentials`], which is the one diagnosis that names a
/// login as the next step.
///
/// # Errors
///
/// Returns the failures that are this process's own rather than the engine's:
/// a temporary file, the OS random source, the caller's own cancellation. Every
/// outcome the reference implementation attributes to the engine — a spawn
/// failure, a deadline, a nonzero exit, an unusable answer — becomes a state.
pub fn probe(
    engine: &Engine,
    env: &[(OsString, OsString)],
    limits: EngineLimits,
    cancellation: &CancellationToken,
) -> Result<EngineState, EngineError> {
    let request = EngineRequest {
        engine,
        model: "",
        spec: PROBE_SPEC,
        text: PROBE_INPUT,
        // Only a custom command reads this, and a custom command is not probed.
        cwd: Path::new("."),
        env,
    };
    let state = match polish(&request, limits, cancellation) {
        Ok(answer) if answer.to_uppercase().contains(PROBE_MARKER) => EngineState::Ok,
        Ok(answer) => {
            let signs = Signs::new().map_err(|source| EngineError::Classifier { source })?;
            signs.classify(&answer)
        }
        Err(error) => attributed(error)?,
    };
    if state == EngineState::Failed && !has_credentials(engine, env) && engine.preset().is_some() {
        return Ok(EngineState::NoCredentials);
    }
    Ok(state)
}

/// Turn the failures the reference implementation blames on the engine into a
/// state, and hand back the ones it has no counterpart for.
fn attributed(error: EngineError) -> Result<EngineState, EngineError> {
    Ok(match &error {
        // One state, two things to look at: waiting too long and finding no
        // route are the same thing to tell a person.
        EngineError::Process {
            source: ProcessError::Timeout,
        } => EngineState::Unreachable,
        EngineError::Process {
            source: ProcessError::Spawn { .. },
        } => EngineState::Missing,
        EngineError::Process {
            source: ProcessError::OutputLimit { .. },
        }
        | EngineError::AnswerRead { .. }
        | EngineError::AnswerLimit { .. }
        | EngineError::AnswerEncoding
        | EngineError::EmptyAnswer => EngineState::Failed,
        EngineError::Exit { state } => *state,
        _ => return Err(error),
    })
}

fn value<'a>(env: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    env.iter()
        .find(|(variable, value)| variable == OsStr::new(name) && !value.is_empty())
        .map(|(_, value)| value.as_os_str())
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
