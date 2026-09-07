//! What one finished message turns into on screen.
//!
//! This is the layer between the host protocol and the pieces that do the
//! work: [`crate::hook::render`] measures and lays out, [`crate::hook::parts`]
//! runs the deterministic fixes and reads the knobs, [`crate::hook::ab`] draws
//! and records the trials, and this module decides which of them a given
//! message gets. `src/limae/hook.py` is the reference implementation and
//! `docs/adr/0009-polish-hook-contract.md` is the normative description.
//!
//! Three decisions live here and nowhere else:
//!
//! * **Whether to polish at all.** A short message is left alone (ADR-0009
//!   section 二), and short is measured in prose rather than in characters: a
//!   message that is long only because it carries a code block has nothing in
//!   it for a model to rewrite.
//! * **One engine or two.** An ordinary turn runs one engine; a sampled turn
//!   runs two and shows the rewrites blind, so that ADR-0008 section 五 has
//!   evidence to decide the default model on.
//! * **What a failure looks like.** Every way this can go wrong ends the same
//!   way: no block, the user's own text on screen, and one line in the
//!   session's diagnostics saying which step failed and how (ADR-0009 section
//!   六). Nothing here ever puts a failure on the screen, and nothing here
//!   writes prose or engine output to the diagnostics.
//!
//! Every rewrite that leaves this module has been through this repository's own
//! deterministic fixes ([`crate::hook::parts::tidy`]). ADR-0005 section 四
//! draws that line: the model settles the words, the rules settle the
//! typography, and a rewrite is not exempt from the rules just because a model
//! wrote it.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::state::{self, Kind, Step};
use super::{ab, parts, render};
use crate::polish::config::{self as polish_config, AUTO_ENGINE};
use crate::polish::diagnosis::FailureReason;
use crate::polish::engines::{ENGINES, Engine, EngineLimits, EngineRequest, HOOK_DISABLE_VARIABLE};
use crate::polish::process::CancellationToken;
use crate::polish::{prompt, select};
use crate::text::is_python_whitespace;

/// How few prose characters leave a message unpolished, overriding
/// [`render::MIN_CHARS`].
pub const MIN_CHARS_VARIABLE: &str = "LIMAE_HOOK_MIN_CHARS";
/// What share of turns get an A/B trial, overriding [`ab::SAMPLE_RATE`].
pub const RATE_VARIABLE: &str = "LIMAE_HOOK_AB_RATE";
/// How long an engine gets to answer, overriding [`TIMEOUT`].
pub const TIMEOUT_VARIABLE: &str = "LIMAE_HOOK_TIMEOUT";

/// How long an engine gets to answer, in seconds.
///
/// Well under the CLI's, because a person is waiting behind this one. The
/// host's own default for a `MessageDisplay` hook is 10 seconds, so the hook
/// entry in `settings.json` has to raise its `timeout` past this for the model
/// to ever get an answer in
/// (`docs/knowledge/polish-hook-self-trial.md`).
pub const TIMEOUT: f64 = 60.0;

/// The heading every one of the three ordinary-turn answers carries.
const HEADING: &str = "── 润色 ──";

/// Build what to show after one finished message.
///
/// `text` is the whole assistant message, `directory` the session-state
/// directory, `message` the sanitised message id the diagnostics line is
/// written under, `env` the environment of the run and `cwd` the directory the
/// configuration is looked up from.
///
/// Empty when this message is not polished at all — too short, or an engine did
/// not answer.
#[must_use]
pub fn block(
    text: &str,
    directory: &Path,
    message: &str,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
) -> String {
    // Prose, not characters: `render::prose_length` leaves out the fenced code
    // blocks, which is what makes a long code answer a short message here.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the knob is a float and the reference implementation compares \
                  the count against it as one; no message is long enough for a \
                  f64 to lose a character of it"
    )]
    let prose = render::prose_length(text) as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "the documented default, converted once for the same comparison"
    )]
    let floor = render::MIN_CHARS as f64;
    if prose < parts::number(env, MIN_CHARS_VARIABLE, floor) {
        return String::new();
    }
    match ab::draw(
        directory,
        env,
        parts::number(env, RATE_VARIABLE, ab::SAMPLE_RATE),
    ) {
        Some(drawn) => trial(&drawn, text, directory, message, env, cwd, now),
        None => one(text, directory, message, env, cwd, now),
    }
}

/// Why one engine call produced no rewrite.
///
/// The two are kept apart because they are two different people's problems: a
/// typo in `[polish]` is the user's to fix and says so by name, rather than
/// arriving as a crash of this file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Refused {
    /// The `[polish]` table says something this code cannot act on.
    Misconfigured,
    /// The engine did not answer, and which way.
    Engine(FailureReason),
}

impl Refused {
    /// Return what the diagnostics line calls this.
    fn kind(self) -> Kind {
        match self {
            Self::Misconfigured => Kind::Misconfigured,
            Self::Engine(reason) => Kind::Engine(reason),
        }
    }
}

/// Polish one message with one engine.
///
/// The engine and model are the ones the `polish` configuration names, found
/// the same way [`crate::polish::cli`] finds them, so a repository that has
/// chosen an engine gets it here too.
///
/// Returns the rewrite, and which engine and model produced it. The model is
/// resolved here rather than left blank, because the record of a run that does
/// not say what ran is not evidence of anything — which is why the resolved
/// name goes into the [`ab::Candidate`] while the engine itself is still handed
/// the setting as written, empty string and all, so that the preset's own
/// default is what actually picks the model.
fn single(
    text: &str,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
) -> Result<(String, ab::Candidate), Refused> {
    let settings = polish_config::resolve(cwd).map_err(|_| Refused::Misconfigured)?;
    let limits = limits(env);
    let cancellation = CancellationToken::new();
    // `LIMAE_ENGINE` outranks the file, and `auto` is answered by the search;
    // there is no flag tier here, because a hook has no command line.
    let named = polish_config::engine(None, env, &settings);
    let name = if named == AUTO_ENGINE {
        // The search's own deadline, not this hook's: `TIMEOUT_VARIABLE` says
        // how long the user waits for a rewrite, and a probe is not one. The
        // reference implementation's `engines.select` takes no timeout at all
        // and probes under `PROBE_TIMEOUT`, so a short hook deadline shortens
        // the wait for a rewrite and not the search for an engine.
        let probing = EngineLimits {
            timeout: select::PROBE_TIMEOUT,
            ..EngineLimits::default()
        };
        select::select(env, probing, &cancellation, now)
            .map_err(|error| Refused::Engine(error.reason()))?
            .name()
            .to_owned()
    } else {
        named.into_owned()
    };
    let preset = ENGINES.iter().find(|engine| engine.name() == name);
    let model = if settings.model().is_empty() {
        preset.map_or("", |engine| {
            engine.preset().map_or("", |preset| preset.model)
        })
    } else {
        settings.model()
    };
    // A name that is not a preset is the user's own command, `custom` and an
    // unknown name alike: the reference implementation's `expand` falls through
    // to the custom template for both, and an empty command there is what says
    // there was no engine.
    let custom;
    let engine: &Engine = match preset {
        Some(engine) => engine,
        None => {
            custom = Engine::Custom(settings.command().to_vec());
            &custom
        }
    };
    let spec = prompt::assemble(text);
    let child = disabled(env);
    let request = EngineRequest {
        engine,
        model: settings.model(),
        spec: &spec,
        text,
        cwd,
        env: &child,
    };
    // The cache-writing front door, so that a real call that fails now outranks
    // a remembered success (`crate::polish::select::polish`); it is the call
    // the reference implementation makes too.
    let answer = select::polish(&request, limits, &cancellation, now)
        .map_err(|error| Refused::Engine(error.reason()))?;
    Ok((
        answer,
        ab::Candidate {
            engine: name,
            model: model.to_owned(),
        },
    ))
}

/// Put every rewrite of one turn through the deterministic fixes, and write
/// down what they did.
///
/// The mechanics are [`render::shown`]; what this adds is the pair of
/// diagnostics lines, which say two different things and are not
/// interchangeable. A fix that would not run is a rewrite reaching the screen
/// without this repository's own rules over it. A fix that ran and *changed*
/// something is a note about the model: which of them need the rules to clean
/// up after them is a selection signal (ADR-0008 section 五), not only a
/// display fix.
///
/// Returns what to display, in the same order, and why the fixes did not run —
/// `None` when they did. A rewrite whose fix would not run is passed through as
/// it came, so the caller gets something to show either way and decides for
/// itself what an unfixed rewrite is worth.
fn shown(
    answers: &[String],
    directory: &Path,
    message: &str,
    cwd: &Path,
    now: SystemTime,
) -> (Vec<String>, Option<Kind>) {
    // `render::shown` carries the reason as a string, because naming it is the
    // caller's job; this caller's name for it is a `Kind`, so the `Kind` is
    // kept beside the string rather than parsed back out of it.
    let mut refused: Option<Kind> = None;
    let (display, fixes) = render::shown(answers, |answer| match parts::tidy(answer, cwd) {
        (fixed, None) => Ok(fixed),
        (_, Some(kind)) => Err(refused.get_or_insert(kind).as_str().to_owned()),
    });
    match fixes {
        render::Fixes::Failed(_) => {
            if let Some(kind) = refused {
                state::note(directory, message, Step::Fix, kind, now);
            }
        }
        render::Fixes::Repaired => {
            state::note(directory, message, Step::Fix, Kind::Repaired, now);
        }
        render::Fixes::Clean => {}
    }
    (display, refused)
}

/// Build the block of an ordinary turn: one engine, one rewrite.
///
/// Empty when no engine answered.
fn one(
    text: &str,
    directory: &Path,
    message: &str,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
) -> String {
    let (answer, ran) = match single(text, env, cwd, now) {
        Ok(polished) => polished,
        Err(refused) => {
            state::note(directory, message, Step::Single, refused.kind(), now);
            return String::new();
        }
    };
    let written = answer.trim_matches(is_python_whitespace).to_owned();
    let (display, failed) = shown(std::slice::from_ref(&written), directory, message, cwd, now);
    // One rewrite went in and `render::shown` answers in the order it was
    // asked, so this is that rewrite.
    let fixed = display.into_iter().next().unwrap_or_default();
    if failed.is_none() {
        // ADR-0012: a round whose fixes did not run is not an observation of
        // what polish does. Its `displayed` would equal `text` because the
        // rules never ran, not because the model wrote nothing to clean up,
        // and a distribution counting those understates the rules' half of
        // ADR-0005 section 四. The round still reaches the screen and still
        // leaves its `fix` line; it just does not enter the sample.
        if ab::record_run(directory, message, text, &written, &fixed, &ran, now).is_err() {
            // The same trade the A/B ledger makes: losing the evidence is bad,
            // throwing away a rewrite the user waited for is worse.
            state::note(directory, message, Step::Record, Kind::Crashed, now);
        }
    }
    if fixed == text {
        return format!("{HEADING} {}\n", render::UNCHANGED);
    }
    let changes = render::changes(text, &fixed);
    if changes.is_empty() {
        // Something moved, but only whitespace or punctuation did, and the
        // deterministic rules own that layer. Saying "no change" would be
        // false.
        return format!("{HEADING} {}\n", render::TYPOGRAPHY_ONLY);
    }
    // The count is the part a reader can act on at a glance; the whole rewrite
    // is what they need to judge whether it reads better, and only they can
    // judge that. Both, in that order.
    format!("{HEADING} {} 处改动\n{fixed}\n", render::count(&changes))
}

/// Build the block of a sampled turn: two engines, shown blind.
///
/// Empty when either candidate did not answer.
fn trial(
    drawn: &ab::Trial,
    text: &str,
    directory: &Path,
    message: &str,
    env: &[(OsString, OsString)],
    cwd: &Path,
    now: SystemTime,
) -> String {
    let answers = match ab::run(drawn, text, &disabled(env), limits(env)) {
        Ok(answers) => answers,
        Err(reason) => {
            // One candidate short is not a comparison, and a second round of
            // calls would make the user wait twice; this turn shows its
            // original.
            state::note(directory, message, Step::Ab, Kind::Engine(reason), now);
            return String::new();
        }
    };
    let written = [answers.0.clone(), answers.1.clone()];
    // Whether the fixes ran is not consulted here: two columns are shown side
    // by side either way, and `ab::record` keeps both what the models wrote and
    // what the reader saw, so an unfixed column is still a readable trial.
    let (display, _) = shown(&written, directory, message, cwd, now);
    let first = display.first().map_or("", String::as_str);
    let second = display.get(1).map_or("", String::as_str);
    if ab::record(
        directory,
        drawn,
        text,
        (&answers.0, &answers.1),
        (first, second),
        now,
    )
    .is_err()
    {
        // The trial is lost as evidence, which is bad; throwing away a
        // comparison the user waited for two model calls to see is worse.
        state::note(directory, message, Step::Record, Kind::Crashed, now);
    }
    ab::render(drawn, (first, second))
}

/// Return the environment an engine of this hook runs in.
///
/// `LIMAE_HOOK_DISABLE` is set in it, because a polish engine is itself a
/// coding agent whose own hooks would otherwise fire on the rewrite: one hook
/// starting another is a recursion whose depth is bounded by nothing.
///
/// The variable is replaced where it is already there rather than appended
/// after it, because what this returns is an environment and an environment is
/// a map: a caller that reads it back should not find one name twice.
fn disabled(env: &[(OsString, OsString)]) -> Vec<(OsString, OsString)> {
    let name = OsStr::new(HOOK_DISABLE_VARIABLE);
    let mut child = env.to_vec();
    if let Some((_, value)) = child.iter_mut().find(|(variable, _)| variable == name) {
        *value = "1".into();
    } else {
        child.push((name.to_owned(), "1".into()));
    }
    child
}

/// Return the bounds one engine call of this hook runs under.
///
/// Only the deadline is the hook's own; the rest are the same caps every polish
/// call has.
///
/// A knob that is not a duration — negative, or past what a [`Duration`] holds
/// — becomes a deadline that has already passed, so the call ends as a timeout
/// and the turn shows its original. That is what the reference implementation's
/// `subprocess` does with a negative timeout; the two part company on an
/// infinite one, where Python waits forever and this reports a timeout at once.
/// Neither is a value anybody types into a setting, and of the two this is the
/// one that gives the user their reply back.
fn limits(env: &[(OsString, OsString)]) -> EngineLimits {
    EngineLimits {
        timeout: Duration::try_from_secs_f64(parts::number(env, TIMEOUT_VARIABLE, TIMEOUT))
            .unwrap_or(Duration::ZERO),
        ..EngineLimits::default()
    }
}

// Unix-only: every path here writes session state, and on another platform
// every one of those calls reports that it cannot.
#[cfg(all(test, unix))]
#[path = "block_tests.rs"]
mod tests;
