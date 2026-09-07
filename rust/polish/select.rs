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

use std::ffi::OsString;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::cache::{self, Observed};
use super::diagnosis::{EngineState, Signs};
use super::engines::{self, ENGINES, Engine, EngineError, EngineLimits, EngineRequest};
use super::process::CancellationToken;
use super::value;

/// What a diagnosis adds when some of its answers came from the cache.
///
/// The user who needs this most is the one who has just logged in: the engine
/// they fixed is still being skipped, and this says why and how to retry now.
const STALE_HINT: &str = "some of these are remembered answers, not fresh ones; to retry right now, name the engine with --engine, or delete";

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
    let Some(home) = super::home(env) else {
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
    let state = match engines::polish(&request, limits, cancellation) {
        Ok(answer) if answer.to_uppercase().contains(PROBE_MARKER) => EngineState::Ok,
        Ok(answer) => {
            let signs = Signs::new().map_err(|source| EngineError::Classifier { source })?;
            signs.classify(&answer)
        }
        Err(error) => {
            let state = error.state();
            state.ok_or(error)?
        }
    };
    if state == EngineState::Failed && !has_credentials(engine, env) && engine.preset().is_some() {
        return Ok(EngineState::NoCredentials);
    }
    Ok(state)
}

/// Find an engine to polish with (ADR-0008 section 三 steps 2 to 6).
///
/// The ordering is recomputed every time; only each engine's observed state is
/// served from the cache, standing in for the probe of step 5 whether it was a
/// probe or a failed real call that last set it. A diagnosis that came from the
/// cache says so and says how to retry now, because the user who needs it most
/// is the one who has just logged in.
///
/// `now` is the run's own clock reading, shared by the TTL check and by
/// whatever this run writes back, so that one search cannot read one clock and
/// write another.
///
/// Only `limits`' caps apply to a probe; its `timeout` does not. A caller's
/// deadline says how long the user waits for a rewrite, and a probe is not one:
/// the reference implementation's `engines.select` takes no timeout at all and
/// probes under [`PROBE_TIMEOUT`].
///
/// # Errors
///
/// Returns [`EngineError::NoUsableEngine`] when no engine answered; its report
/// diagnoses every preset one by one and gives each one's next step. Nothing an
/// engine printed is quoted. A failure that is this process's own rather than
/// an engine's — a temporary file, the OS random source, a cancellation — is
/// returned as itself.
pub fn select(
    env: &[(OsString, OsString)],
    limits: EngineLimits,
    cancellation: &CancellationToken,
    now: SystemTime,
) -> Result<&'static Engine, EngineError> {
    let path = cache::file(env);
    let mut fresh = path
        .as_deref()
        .map_or_else(Vec::new, |path| cache::remembered(path, now));
    // An answer about a CLI that is no longer installed is not an answer about
    // anything: step 3 excludes it before its remembered state is consulted.
    fresh.retain(|(engine, _)| installed(engine, env));

    // The search's own deadline, not the caller's: `PROBE_TIMEOUT` is how long
    // one engine may take to say it is alive, which is a different question
    // from how long a rewrite may take. A caller that hands its own deadline
    // down would otherwise shorten or lengthen the search along with it.
    let probing = EngineLimits {
        timeout: PROBE_TIMEOUT,
        ..limits
    };
    let candidates = order(env);
    let mut diagnosed: Vec<(&'static Engine, EngineState)> = Vec::new();
    let mut stale = false;
    for engine in candidates {
        let remembered = observed(&fresh, engine);
        let state = match remembered {
            Some(observed) => observed.state,
            None => {
                let state = probe(engine, env, probing, cancellation)?;
                // `command -v` is free to redo, so a missing binary is never
                // written down; only what cost a real call is.
                if state != EngineState::Missing
                    && let Some(path) = path.as_deref()
                {
                    cache::remember(path, engine, state, now);
                }
                state
            }
        };
        if state == EngineState::Ok {
            return Ok(engine);
        }
        diagnosed.push((engine, state));
        stale = stale || remembered.is_some();
    }

    let mut report = vec!["no polish engine is usable:".to_owned()];
    for engine in &ENGINES {
        let state = diagnosed
            .iter()
            .find(|(diagnosed, _)| diagnosed.name() == engine.name())
            .map_or(EngineState::Missing, |(_, state)| *state);
        let when = observed(&fresh, engine).map_or_else(String::new, |observed| {
            format!(" ({})", cache::ago(observed.age))
        });
        let step = state.next_step().unwrap_or_default();
        report.push(format!("  {}: {state}{when} — {step}", engine.name()));
    }
    report.push("or configure [polish] engine = 'custom' with your own command".to_owned());
    if let (true, Some(path)) = (stale, path.as_deref()) {
        report.push(format!("{STALE_HINT} {}", path.display()));
    }
    Err(EngineError::NoUsableEngine {
        report: report.join("\n"),
    })
}

/// Rewrite one piece of prose, keeping the cache honest.
///
/// This is the `auto` search's front door for a real call, and the reason
/// [`cache::CACHE_TTL`] can be an hour: a call that failed for real is the
/// check that a remembered `Ok` has to survive, so its state is written over
/// that `Ok` and the next run picks another engine instead of failing the same
/// way for the rest of the hour. [`super::engines::polish`] is the same call
/// without that write-back, for a caller that has already been told which
/// engine to run.
///
/// A custom command is nobody's cached engine, and a missing binary is free to
/// re-check, so neither is written down.
///
/// # Errors
///
/// Returns whatever the invocation failed with, unchanged.
pub fn polish(
    request: &EngineRequest<'_>,
    limits: EngineLimits,
    cancellation: &CancellationToken,
    now: SystemTime,
) -> Result<String, EngineError> {
    engines::polish(request, limits, cancellation).inspect_err(|error| {
        let Some(state) = error.state().filter(|state| *state != EngineState::Missing) else {
            return;
        };
        if let Some(path) = cache::file(request.env) {
            cache::remember(&path, request.engine, state, now);
        }
    })
}

fn observed<'a>(fresh: &'a [(&'static Engine, Observed)], engine: &Engine) -> Option<&'a Observed> {
    fresh
        .iter()
        .find(|(remembered, _)| remembered.name() == engine.name())
        .map(|(_, observed)| observed)
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
