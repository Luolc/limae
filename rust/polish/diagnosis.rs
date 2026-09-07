//! What a failed polish engine invocation is told to be, in two layers.
//!
//! [`EngineState`] is the human-readable layer of ADR-0008 section 三 step 6:
//! one diagnosis per engine, with [`EngineState::next_step`] naming what to do
//! about it. [`FailureReason`] is the finer layer a caller needs to tell apart
//! failures a state merges: a timeout and an unreachable network are one state
//! but two different things to look at, and so are a nonzero exit and an empty
//! answer. The hook's diagnostics record the reason so that "no polish
//! appeared" can be answered without guessing
//! (`docs/adr/0009-polish-hook-contract.md` section 六).
//!
//! Nothing here carries anything the engine printed. [`Signs::classify`] reads
//! a child's output and returns a state; the output itself never reaches a
//! return value, an error, a `Debug` rendering or a log, because an engine is
//! free to quote a variable back at us and this repository is public
//! (`AGENTS.md` 「隐私边界」).

use std::fmt;

use regex::Regex;

/// Signs that the engine ran but refused the caller's credentials.
const UNAUTHORIZED_SIGN: &str = r"(?i)401|403|unauthori[sz]ed|forbidden|invalid[ _-]?(api[ _-]?)?key|not logged[ _-]?in|please log[ _-]?in|re-?authenticate";
/// Signs that the engine never reached its service.
const UNREACHABLE_SIGN: &str = r"(?i)getaddrinfo|enotfound|econnrefused|econnreset|etimedout|ehostunreach|network|dns|unreachable|timed out|timeout|proxy|offline";

/// One engine's diagnosis, as it is told to a person.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineState {
    /// The engine answered.
    Ok,
    /// The CLI is not on `PATH`.
    Missing,
    /// The engine ran and rejected the credentials it found.
    Unauthorized,
    /// The engine failed and no credential trace was found for it.
    NoCredentials,
    /// The engine did not reach its service, including by waiting too long.
    Unreachable,
    /// The engine ran and failed for some other reason.
    Failed,
}

impl EngineState {
    /// Return the wording used in diagnostics and error messages.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "not installed",
            Self::Unauthorized => "credentials rejected (401 / 403)",
            Self::NoCredentials => "no credentials found",
            Self::Unreachable => "network unreachable",
            Self::Failed => "ran but failed",
        }
    }

    /// Return the state a stored [`EngineState::as_str`] came from.
    ///
    /// `None` for anything else, which is what lets a cache file written by a
    /// different build, or by hand, be dropped instead of believed.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        [
            Self::Ok,
            Self::Missing,
            Self::Unauthorized,
            Self::NoCredentials,
            Self::Unreachable,
            Self::Failed,
        ]
        .into_iter()
        .find(|state| state.as_str() == text)
    }

    /// Return what the person should do about this state.
    ///
    /// [`EngineState::Ok`] has no next step and is the only `None`.
    #[must_use]
    pub fn next_step(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::Missing => Some("install the CLI, or name another engine with --engine"),
            Self::Unauthorized => Some("log in to that CLI again"),
            Self::NoCredentials => Some("log in to that CLI once, or configure engine = 'custom'"),
            Self::Unreachable => Some("check the network, then retry"),
            Self::Failed => Some("run the CLI by hand once to see what it says"),
        }
    }

    /// Return the state and its next step as one phrase.
    #[must_use]
    pub fn describe(self) -> String {
        match self.next_step() {
            Some(step) => format!("{self} — {step}"),
            None => self.to_string(),
        }
    }
}

impl fmt::Display for EngineState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How one invocation actually ended, for a caller that has to tell apart the
/// failures an [`EngineState`] merges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureReason {
    /// There was no engine to run.
    NoEngine,
    /// The engine's executable could not be started.
    NotInstalled,
    /// The invocation exceeded its deadline.
    TimedOut,
    /// The engine reported that it did not reach its service.
    Unreachable,
    /// The engine rejected the credentials it found.
    Rejected,
    /// The engine exited nonzero for some other reason.
    NonzeroExit,
    /// The engine succeeded but produced only whitespace.
    EmptyAnswer,
    /// The engine's answer could not be read back.
    UnreadableAnswer,
    /// Anything else, including a caller's cancellation.
    Other,
}

impl FailureReason {
    /// Return the stable token recorded by the hook's diagnostics.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoEngine => "no-engine",
            Self::NotInstalled => "not-installed",
            Self::TimedOut => "timeout",
            Self::Unreachable => "unreachable",
            Self::Rejected => "unauthorized",
            Self::NonzeroExit => "exit",
            Self::EmptyAnswer => "empty",
            Self::UnreadableAnswer => "unreadable",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The compiled signs one failed run's output is read for.
///
/// This type holds patterns only. It never retains the output it is given.
pub struct Signs {
    unauthorized: Regex,
    unreachable: Regex,
}

impl Signs {
    /// Compile the signs.
    ///
    /// # Errors
    ///
    /// Returns the compilation failure of this module's own constant patterns.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            unauthorized: Regex::new(UNAUTHORIZED_SIGN)?,
            unreachable: Regex::new(UNREACHABLE_SIGN)?,
        })
    }

    /// Read one failed run's output for what went wrong.
    ///
    /// The order is the reference implementation's: an output matching both
    /// signs is [`EngineState::Unauthorized`], because the unreachable signs
    /// include words an authentication failure can carry too. `output` is only
    /// read; it is never returned or reported.
    #[must_use]
    pub fn classify(&self, output: &str) -> EngineState {
        if self.unauthorized.is_match(output) {
            return EngineState::Unauthorized;
        }
        if self.unreachable.is_match(output) {
            return EngineState::Unreachable;
        }
        EngineState::Failed
    }
}

#[cfg(test)]
#[path = "diagnosis_tests.rs"]
mod tests;
