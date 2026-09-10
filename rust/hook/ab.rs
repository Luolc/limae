//! What the display hook did, written down: A/B trials and single runs.
//!
//! The A/B trial is the elaborate shape — two candidates under one code name —
//! and most of this module is about it. A turn that was not sampled is the
//! plain shape: one engine, one rewrite, recorded the same way, in the same
//! place, under the same permissions. Both are here rather than split across
//! modules because the guarantee is a property of the pair: whatever polish
//! does gets written to the session's own directory and nowhere else, and there
//! is one piece of code that decides that.
//!
//! `docs/adr/0009-polish-hook-contract.md` sections 三 to 五 are the normative
//! description; the Python reference implementation this was ported from has
//! since been deleted. On a
//! sampled turn the hook runs two models over the same assistant message and
//! shows both, so that ADR-0008 section 五 — the default model is not frozen,
//! measurement decides it — has evidence to decide on.
//!
//! Two properties this module exists to keep:
//!
//! * **The screen stays blind.** ADR-0008 section 五 asks for 10 to 20 blind
//!   comparisons of real prose, so the two candidates are labelled A and B and
//!   nothing else. Which model wrote which reaches the model through the `Stop`
//!   hook (ADR-0009 section 五) and the ledger, never the screen: a reader who
//!   can see the names is no longer judging the prose.
//! * **The ledger never leaves the session.** It holds the assistant's own
//!   reply, so it is written under the session-state directory the hook owns
//!   and nowhere else — not this repository, not another agent (ADR-0009
//!   sections 五 and 八). The modes and the create-then-rename come from
//!   [`super::state`], because one piece of code deciding that is the whole
//!   point.
//!
//! The code names live here rather than in `spec/`. `spec/` is the contract
//! every implementation of this tool must reproduce — the rules, the golden
//! fixtures, the prompt layers — and two implementations drawing different
//! words are both right, because no output anyone compares depends on which
//! word came up. What the list has to satisfy is a property of the person, not
//! of the tool: ADR-0009 section 四 picks two-character Chinese nouns so that
//! feedback given by voice ("the 灯塔 round, B was better") survives
//! speech-to-text.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::thread::ScopedJoinHandle;
use std::time::SystemTime;

use getrandom::fill;
use serde_json::Value;

use super::state;
use crate::polish::diagnosis::FailureReason;
use crate::polish::engines::{ENGINES, Engine, EngineLimits, EngineRequest};
use crate::polish::process::CancellationToken;
use crate::polish::{prompt, select};
use crate::text::is_python_whitespace;

/// One trial per sampled turn; ADR-0009 section 三 starts at about one turn in
/// ten and leaves the final value open.
pub const SAMPLE_RATE: f64 = 0.1;

/// Where the trials of one session are kept, one file per code name.
pub const LEDGER_DIRECTORY: &str = "ab";
/// Where a turn that was not sampled is written down.
///
/// Separate from the A/B ledger because it is keyed by message rather than by
/// code name, and because a reader asking "what did polish do to this message"
/// is not asking "which trial was this".
pub const RUN_DIRECTORY: &str = "polish";
/// What the `Stop` hook reads to find out what this turn ran.
pub const PENDING_FILENAME: &str = "pending.json";

/// The two columns, in the order they are shown.
pub const LABELS: [&str; 2] = ["A", "B"];

/// Two-character Chinese nouns, common enough that speech-to-text gets them
/// right and distinct enough from each other that a mistyped transcription
/// still names one round only. Extend the list when a session runs out of them.
#[rustfmt::skip]
pub const CODE_NAMES: [&str; 48] = [
    "灯塔", "山谷", "河流", "森林", "海鸥", "松树", "石桥", "麦田",
    "竹林", "晚霞", "清风", "溪水", "雪山", "月光", "春雨", "沙洲",
    "港湾", "草原", "湖泊", "峡谷", "岛屿", "稻田", "果园", "篝火",
    "铜镜", "陶罐", "木船", "风筝", "灯笼", "钟楼", "石阶", "屋檐",
    "屏风", "门廊", "砚台", "竹简", "罗盘", "铁锚", "帆船", "号角",
    "琥珀", "玛瑙", "青苔", "榆树", "白杨", "芦苇", "荷塘", "银杏",
];

/// The seven of ADR-0008 section 五, which is where this pool is decided; this
/// module only draws from it.
pub const CANDIDATES: [(&str, &str); 7] = [
    ("codex", "gpt-5.6-luna"),
    ("codex", "gpt-5.6-terra"),
    ("codex", "gpt-5.4"),
    ("grok", "grok-4.5"),
    ("grok", "grok-4.6"),
    ("claude", "haiku"),
    ("claude", "sonnet"),
];

/// One engine and model to try.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// A preset name, as [`Engine::name`] spells it.
    pub engine: String,
    /// The model to run under that preset.
    pub model: String,
}

/// One sampled turn's A/B trial.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trial {
    /// The round's code name, unique within the session.
    pub code: String,
    /// The candidate shown as A.
    pub a: Candidate,
    /// The candidate shown as B.
    pub b: Candidate,
}

/// Return the candidates whose CLI is installed, in the order of
/// [`CANDIDATES`].
#[must_use]
pub fn pool(env: &[(OsString, OsString)]) -> Vec<Candidate> {
    CANDIDATES
        .iter()
        .filter(|(engine, _)| preset(engine).is_some_and(|engine| select::installed(engine, env)))
        .map(|(engine, model)| Candidate {
            engine: (*engine).to_owned(),
            model: (*model).to_owned(),
        })
        .collect()
}

/// Decide whether this turn gets an A/B trial, and with what.
///
/// `directory` is the session-state directory, which is also the register of
/// code names already used (ADR-0009 section 四); `rate` is the share of turns
/// to sample, from 0 to 1.
///
/// `None` when this turn is an ordinary one — not sampled, fewer than two
/// candidates installed, or the session has used every code name there is — and
/// also when the system's random source will not answer, which is the same
/// nothing-to-do as not being sampled.
#[must_use]
pub fn draw(directory: &Path, env: &[(OsString, OsString)], rate: f64) -> Option<Trial> {
    // Sampling, not cryptography: a predictable draw would cost nothing here
    // beyond a less even spread of trials.
    if fraction()? >= rate {
        return None;
    }
    let candidates = pool(env);
    if candidates.len() < 2 {
        return None;
    }
    let used = used(directory);
    let free: Vec<&str> = CODE_NAMES
        .iter()
        .copied()
        .filter(|name| !used.contains(*name))
        .collect();
    if free.is_empty() {
        return None;
    }
    let (a, b) = pair(&candidates)?;
    Some(Trial {
        code: free[index(free.len())?].to_owned(),
        a,
        b,
    })
}

/// Run both candidates over the same text.
///
/// The two run at once: one after the other would put two model calls between
/// the user and their own message.
///
/// The answers come back normalised, because two places need the same string:
/// what goes on screen and what goes in the ledger. Stripping in the renderer
/// alone made the ledger's copy a different text from the displayed one by a
/// trailing newline, which is one fact with two versions of itself — enough to
/// make a later comparison of the two disagree for no reason.
///
/// Returns the two rewrites, A first, or the reason the first failing candidate
/// gave, which leaves the turn showing the original text (ADR-0009 section 六).
/// The candidates are waited on in the order they were started, so the reason
/// reported is the one belonging to the earlier column and not to whichever
/// call happened to fail first — the reference implementation reads its two
/// futures in that same order, and the two answers differ when both fail.
pub fn run(
    trial: &Trial,
    text: &str,
    env: &[(OsString, OsString)],
    limits: EngineLimits,
) -> Result<(String, String), FailureReason> {
    let spec = prompt::assemble(text);
    let cancellation = CancellationToken::new();
    let (first, second) = std::thread::scope(|scope| {
        let a = scope.spawn(|| one(&trial.a, &spec, text, env, limits, &cancellation));
        let b = scope.spawn(|| one(&trial.b, &spec, text, env, limits, &cancellation));
        (joined(a), joined(b))
    });
    Ok((first?, second?))
}

/// Lay out one trial for the screen.
///
/// The original is not repeated here: it is the text that has just streamed,
/// immediately above (ADR-0009 sections 二 and 三). Neither model is named — the
/// comparison is blind. `shown` is the two rewrites as they will appear, A
/// first, already normalised by [`run`] and fixed by the hook.
#[must_use]
pub fn render(trial: &Trial, shown: (&str, &str)) -> String {
    let columns = [(LABELS[0], shown.0), (LABELS[1], shown.1)]
        .map(|(label, answer)| format!("── {label} ──\n{answer}"))
        .join("\n\n");
    format!(
        "[A/B {}] 原文如上，以下是两个候选：\n\n{columns}\n",
        trial.code
    )
}

/// Write one trial to the session's ledger.
///
/// Two files: the ledger entry, which is the evidence ADR-0008 section 五 will
/// be decided on, and the pending file the `Stop` hook reads to tell the model
/// what just happened ([`context`]).
///
/// Both versions of each candidate are kept, because they answer different
/// questions. `answers` is what the models wrote, which is what section 五
/// compares — fold the deterministic fixes into it and a model that keeps
/// dropping a space beside an inline code span becomes indistinguishable from
/// one that never does, which is a selection signal erased. `shown` is what the
/// reader saw, without which no later reading of the ledger can reproduce the
/// screen. Both are A first, as is `original`'s pair of columns.
pub fn record(
    directory: &Path,
    trial: &Trial,
    original: &str,
    answers: (&str, &str),
    shown: (&str, &str),
    now: SystemTime,
) -> io::Result<()> {
    let at = state::timestamp(now);
    let candidates = candidates(trial, answers, shown);
    let ledger = directory.join(LEDGER_DIRECTORY);
    state::create_directory(&ledger)?;
    write(
        &ledger.join(format!("{}.json", trial.code)),
        &object(
            0,
            &[
                ("code", quote(&trial.code)),
                ("at", quote(&at)),
                ("original", quote(original)),
                ("candidates", candidates.clone()),
            ],
        ),
    )?;
    write(
        &directory.join(PENDING_FILENAME),
        &object(
            0,
            &[
                ("code", quote(&trial.code)),
                ("at", quote(&at)),
                ("candidates", candidates),
            ],
        ),
    )
}

/// Write down one un-sampled turn: one engine, one rewrite.
///
/// `docs/adr/0012-single-run-polish-records.md` is the normative description,
/// including why this is not the "no writing to disk" that ADR-0008 section 十
/// rules out: that phrase is about the user's files and about side-channel
/// artefacts needing review, not about a session's own scratch.
///
/// Until this existed, a single polish left nothing on disk: what went in and
/// what came out could only be recovered from a screenshot. That is not merely
/// inconvenient — it makes the polish spec unmeasurable. A model's rewriting is
/// not a fixed function; the same input has come back anywhere from untouched
/// to stripped of every emphasis marker, so a single observation says nothing
/// about a change to the spec. Judging one needs a sample, and a sample needs
/// every run on disk.
///
/// The three versions are kept apart for the reason the A/B ledger keeps two:
/// `original` is what the assistant wrote, `written` is what the model made of
/// it, `displayed` is what the reader saw after the deterministic fixes.
/// Folding the fixes into `written` would hide how much of the tidiness was the
/// model's doing and how much was the rules cleaning up after it — which is the
/// question. `message` is the sanitised message id, which names the file.
pub fn record_run(
    directory: &Path,
    message: &str,
    original: &str,
    written: &str,
    displayed: &str,
    engine: &Candidate,
    now: SystemTime,
) -> io::Result<()> {
    let runs = directory.join(RUN_DIRECTORY);
    state::create_directory(&runs)?;
    write(
        &runs.join(format!("{message}.json")),
        &object(
            0,
            &[
                ("at", quote(&state::timestamp(now))),
                ("message_id", quote(message)),
                ("engine", quote(&engine.engine)),
                ("model", quote(&engine.model)),
                ("original", quote(original)),
                ("text", quote(written)),
                ("displayed", quote(displayed)),
            ],
        ),
    )
}

/// Return what the model should be told about the trial this turn ran.
///
/// This is ADR-0009 section 五: a `MessageDisplay` rewrite is invisible to the
/// model, so the code name is handed over here instead — and only that, never
/// the rewrites themselves.
///
/// **The model names are deliberately not here, and that is a correction.**
/// ADR-0009 section 五 assumed this channel reached the model alone. It does
/// not. Measured 2026-09-01 against Claude Code `2.1.258`: a `Stop` hook's
/// `additionalContext` is wrapped in a `stop_hook_summary` system message and
/// rendered on screen as `Stop hook feedback: …` — and the summary is *hidden*
/// when there is no additionalContext, so supplying it is precisely what makes
/// it visible. The schema's "delivered to the model" says where it goes in the
/// context, not that the reader cannot see it. Naming the models here therefore
/// printed the answer key next to the blind comparison, which is the one thing
/// section 三 asks this not to do. The mapping stays in the ledger, which the
/// reader is not looking at.
///
/// **Announced once, and that is load-bearing.** The pending file is consumed
/// here. The obvious-looking simplification — always return something, since
/// there is always a trial to describe — hangs the session:
/// `additionalContext` is non-error feedback that continues the conversation,
/// so a `Stop` hook that always answers re-triggers itself. Measured the same
/// day: a hook returning a constant string turned "say only: hello" into five
/// turns and counting.
///
/// **It says "this turn", not "the last reply", because a turn holds many
/// replies.** `Stop` fires once, at the end; the trial ran on one assistant
/// message somewhere inside it. In a long agent turn that can be an hour and
/// dozens of messages earlier — observed at 47 minutes. Claiming it was the
/// previous reply would simply be false, and the code name is what the user
/// gives feedback by in any case.
///
/// Returns the text for `additionalContext`, empty when this turn ran no trial.
#[must_use]
pub fn context(directory: &Path) -> String {
    let path = directory.join(PENDING_FILENAME);
    let Ok(bytes) = fs::read(&path) else {
        return String::new();
    };
    let Ok(pending) = serde_json::from_slice::<Value>(&bytes) else {
        return String::new();
    };
    let _ = fs::remove_file(&path);
    let code = pending.get("code").and_then(Value::as_str).unwrap_or("");
    if code.is_empty() {
        return String::new();
    }
    let ledger = directory
        .join(LEDGER_DIRECTORY)
        .join(format!("{code}.json"));
    // Short on purpose: this lands on the user's screen, where the host
    // truncates it.
    format!(
        "limae A/B：本轮有一次 A/B 对照，编号「{code}」，两栏是盲评。\
         型号对应在 {}；用户按编号给出偏好之前不要说出哪一栏是哪个模型。",
        ledger.display()
    )
}

/// Return the preset one candidate names.
fn preset(name: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|engine| engine.name() == name)
}

/// Return the code names this session has already handed out.
///
/// The ledger is the register: one file per trial, named after the code. A
/// ledger that cannot be read hands out nothing, which is the reference
/// implementation's empty set.
fn used(directory: &Path) -> HashSet<String> {
    let Ok(entries) = fs::read_dir(directory.join(LEDGER_DIRECTORY)) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|suffix| suffix == "json"))
        .filter_map(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .collect()
}

/// Run one candidate and return its rewrite, stripped, or why it did not
/// answer.
fn one(
    candidate: &Candidate,
    spec: &str,
    text: &str,
    env: &[(OsString, OsString)],
    limits: EngineLimits,
    cancellation: &CancellationToken,
) -> Result<String, FailureReason> {
    let Some(engine) = preset(&candidate.engine) else {
        return Err(FailureReason::NoEngine);
    };
    let request = EngineRequest {
        engine,
        model: &candidate.model,
        spec,
        text,
        // A preset runs in the private directory the call makes for it; `cwd`
        // is the working directory of a `custom` command only, and the pool
        // holds no custom commands.
        cwd: Path::new("."),
        env,
    };
    select::polish(&request, limits, cancellation, SystemTime::now())
        .map(|answer| answer.trim_matches(is_python_whitespace).to_owned())
        .map_err(|error| error.reason())
}

/// Wait for one candidate, letting a panic in it stay a panic.
///
/// The reference implementation's `future.result()` re-raises anything that is
/// not an [`crate::polish::engines::EngineError`]; swallowing one here would
/// turn a bug in this process into an engine's fault in the diagnostics.
fn joined(
    handle: ScopedJoinHandle<'_, Result<String, FailureReason>>,
) -> Result<String, FailureReason> {
    match handle.join() {
        Ok(answer) => answer,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

/// Render both columns of one trial as the ledger's `candidates` array.
fn candidates(trial: &Trial, answers: (&str, &str), shown: (&str, &str)) -> String {
    let columns = [
        (LABELS[0], &trial.a, answers.0, shown.0),
        (LABELS[1], &trial.b, answers.1, shown.1),
    ]
    .map(|(label, candidate, answer, display)| {
        object(
            4,
            &[
                ("label", quote(label)),
                ("engine", quote(&candidate.engine)),
                ("model", quote(&candidate.model)),
                ("text", quote(answer)),
                ("displayed", quote(display)),
            ],
        )
    });
    array(2, &columns)
}

/// Write one JSON file only this user can read.
///
/// The mode is part of the create call, not a `chmod` after it: between a
/// default-mode create and a `chmod`, a file holding an assistant reply is
/// readable by everyone on the machine. It is then renamed into place, so the
/// `Stop` hook never reads a half-written pending file. The temporary name
/// carries this process's id, for the reason [`super::state::keep`] gives.
fn write(path: &Path, entry: &str) -> io::Result<()> {
    let Some(name) = path.file_name() else {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    };
    let mut writing = name.to_owned();
    writing.push(format!(
        ".{}{}",
        std::process::id(),
        state::TEMPORARY_SUFFIX
    ));
    let writing = path.with_file_name(writing);
    let mut file = state::create(&writing)?;
    io::Write::write_all(&mut file, entry.as_bytes())?;
    drop(file);
    fs::rename(&writing, path)
}

/// Render one JSON object with its keys in the order they are given.
///
/// The reference implementation writes `json.dumps(..., ensure_ascii=False,
/// indent=2)`, whose keys come out in insertion order, and the two files are
/// meant to be the same bytes. `serde_json`'s own map is sorted, which would
/// reorder every entry, so the layout is spelled out here; the values still go
/// through [`quote`], which escapes exactly what Python escapes with
/// `ensure_ascii=False`.
///
/// `indent` is the column the closing brace sits at, and every value is already
/// rendered for the column it lands in.
fn object(indent: usize, fields: &[(&str, String)]) -> String {
    let pad = " ".repeat(indent + 2);
    let body = fields
        .iter()
        .map(|(key, value)| format!("{pad}{}: {value}", quote(key)))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("{{\n{body}\n{}}}", " ".repeat(indent))
}

/// Render one JSON array whose items are already rendered for their column.
fn array(indent: usize, items: &[String]) -> String {
    let pad = " ".repeat(indent + 2);
    let body = items
        .iter()
        .map(|item| format!("{pad}{item}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("[\n{body}\n{}]", " ".repeat(indent))
}

/// Render one string as JSON, leaving anything printable as itself.
fn quote(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// Return a uniform value in `[0, 1)`, the reference implementation's
/// `random.random()`.
///
/// `None` when the system's random source will not answer; every caller reads
/// that as "no trial this turn", which is what an unsampled turn is anyway.
fn fraction() -> Option<f64> {
    let mut bytes = [0u8; 8];
    fill(&mut bytes).ok()?;
    // The 53 bits of a double's mantissa, which is the spread `random.random()`
    // has.
    Some((u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64)
}

/// Return a uniform index into a list of `count` things, or `None` when there
/// is nothing to index.
///
/// The empty case is answered rather than assumed: it is what makes [`pair`]
/// total over a pool of one, which would otherwise be a division by zero
/// waiting for the day [`CANDIDATES`] holds one engine.
fn index(count: usize) -> Option<usize> {
    let count = u64::try_from(count).ok().filter(|count| *count > 0)?;
    let mut bytes = [0u8; 8];
    fill(&mut bytes).ok()?;
    // Sampling, not cryptography: the modulo's bias over a list of at most a
    // few dozen is far below anything a spread of trials could show.
    usize::try_from(u64::from_le_bytes(bytes) % count).ok()
}

/// Return two different candidates, the reference implementation's
/// `random.sample(candidates, 2)`.
///
/// `None` for a pool too small to draw two from, which [`draw`] has already
/// turned away.
fn pair(candidates: &[Candidate]) -> Option<(Candidate, Candidate)> {
    let first = index(candidates.len())?;
    // Draw from the rest and shift back over the one already taken, so the two
    // are never the same candidate.
    let second = index(candidates.len().saturating_sub(1))?;
    let second = second + usize::from(second >= first);
    Some((candidates[first].clone(), candidates[second].clone()))
}

// Unix-only: everything here writes state, and on another platform every one of
// those calls reports that it cannot.
#[cfg(all(test, unix))]
#[path = "ab_tests.rs"]
mod tests;
