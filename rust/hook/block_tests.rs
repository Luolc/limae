use super::{
    HEADING, MIN_CHARS_VARIABLE, RATE_VARIABLE, Refused, TIMEOUT_VARIABLE, block, one, shown,
    single, trial,
};
use crate::hook::ab::{Candidate, LEDGER_DIRECTORY, RUN_DIRECTORY, Trial};
use crate::hook::render::{TYPOGRAPHY_ONLY, UNCHANGED};
use crate::hook::state::DIAGNOSTICS_FILENAME;
use crate::polish::diagnosis::FailureReason;

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// The sanitised message id every record and diagnostics line is written under.
const MESSAGE: &str = "m1";

/// One sentence of synthetic Chinese prose that this repository's own rules
/// leave alone, so that a rewrite made of it changes only where a test changes
/// it.
const LINE: &str = "ACME 的报告写得不好，请把它改得像人话一些。";

/// A message long enough to be worth polishing.
fn long() -> String {
    LINE.repeat(20)
}

/// A rewrite that this repository's fixer refuses to run over at all: the
/// directive names a rule that does not exist. This is what makes "the fixes
/// did not run" a real arm rather than a branch nothing reaches.
const UNFIXABLE: &str = "<!-- limae-disable zh-typography-99 -->";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-block-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// One run's four directories: a `PATH` of fake CLIs, a home nothing real lives
/// in, the session state, and the directory the configuration is read from.
///
/// No assertion here depends on what this machine has installed or on what the
/// user running the suite has configured.
struct Fixture {
    bin: TempDir,
    home: TempDir,
    state: TempDir,
    cwd: TempDir,
}

impl Fixture {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        Ok(Self {
            bin: TempDir::new(&format!("{name}-bin"))?,
            home: TempDir::new(&format!("{name}-home"))?,
            state: TempDir::new(&format!("{name}-state"))?,
            cwd: TempDir::new(&format!("{name}-cwd"))?,
        })
    }

    fn env(&self) -> Vec<(OsString, OsString)> {
        self.with(&[])
    }

    /// The environment of a run, plus whatever knobs a test sets.
    fn with(&self, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        let mut env: Vec<(OsString, OsString)> = [
            ("PATH", self.bin.path().as_os_str().to_owned()),
            ("HOME", self.home.path().as_os_str().to_owned()),
            (
                "XDG_CACHE_HOME",
                self.home.path().join("cache").into_os_string(),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (OsString::from(name), value))
        .collect();
        env.extend(
            extra
                .iter()
                .map(|(name, value)| (OsString::from(*name), OsString::from(*value))),
        );
        env
    }

    /// Put one executable of the given name on the fake `PATH`.
    ///
    /// The engine's own `PATH` is the fake one, which holds these scripts and
    /// nothing else, so a script that needs `cat`, `grep` or `sleep` gets this
    /// run's real `PATH` back at the top of itself. Without that the tools are
    /// simply missing and a script meant to wait returns at once — which is a
    /// test that no longer measures what it says it does.
    ///
    /// Every stub also records that it ran, so that a test can say how many
    /// engine calls a turn made.
    fn install(&self, name: &str, script: &str) -> TestResult {
        let tools = std::env::var("PATH").unwrap_or_default();
        let path = self.bin.path().join(name);
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nPATH='{tools}'\nexport PATH\necho run >> '{}'\n{script}\n",
                self.log().display()
            ),
        )?;
        fs::set_permissions(&path, Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn log(&self) -> PathBuf {
        self.home.path().join("calls")
    }

    /// How many times any stub engine has run.
    fn calls(&self) -> usize {
        fs::read_to_string(self.log()).map_or(0, |log| log.lines().count())
    }

    /// Write the `[polish]` table this run's configuration holds.
    fn configure(&self, table: &str) -> TestResult {
        fs::write(self.cwd.path().join("limae.toml"), table)?;
        Ok(())
    }

    /// The fail-open lines this session has written.
    fn diagnostics(&self) -> Result<Vec<Value>, Box<dyn Error>> {
        let path = self.state.path().join(DIAGNOSTICS_FILENAME);
        if !path.is_file() {
            return Ok(Vec::new());
        }
        fs::read_to_string(path)?
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }

    /// The `(step, kind)` pairs of this session's diagnostics, in order.
    fn steps(&self) -> Result<Vec<(String, String)>, Box<dyn Error>> {
        Ok(self
            .diagnostics()?
            .iter()
            .map(|line| {
                (
                    field(line, "step").to_owned(),
                    field(line, "kind").to_owned(),
                )
            })
            .collect())
    }

    /// The single-run records this session has written.
    fn records(&self) -> Vec<PathBuf> {
        listing(&self.state.path().join(RUN_DIRECTORY))
    }

    /// The A/B ledger entries this session has written.
    fn ledger(&self) -> Vec<PathBuf> {
        listing(&self.state.path().join(LEDGER_DIRECTORY))
    }
}

fn field<'a>(line: &'a Value, key: &str) -> &'a str {
    line.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn listing(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    found.sort();
    found
}

/// A stub that answers every call with the same text.
fn answering(answer: &str) -> String {
    format!("cat > /dev/null\ncat <<'ANSWER'\n{answer}\nANSWER")
}

/// A stub that answers the `auto` search's probe with the marker it asks for
/// and every real call with `answer`.
///
/// The two are told apart by the spec the call carries, which is the only thing
/// the probe and a rewrite differ in from a CLI's side.
fn probing(answer: &str) -> String {
    format!(
        "if grep -q LIMAE-PROBE-OK \"$3\"; then cat > /dev/null; echo LIMAE-PROBE-OK; else\n{}\nfi",
        answering(answer)
    )
}

fn candidate(engine: &str, model: &str) -> Candidate {
    Candidate {
        engine: engine.to_owned(),
        model: model.to_owned(),
    }
}

// -- one engine call ------------------------------------------------------

/// The record of a run that does not say what ran is not evidence of anything,
/// so the model the candidate carries is the resolved one and not the empty
/// override that produced it.
#[test]
fn single_runs_the_configured_engine_and_reports_the_model_it_resolved() -> TestResult {
    let fixture = Fixture::new("single-configured")?;
    fixture.install("claude", &answering("甲的改写"))?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    let (answer, ran) = single(&long(), &fixture.env(), fixture.cwd.path(), now())
        .map_err(|refused| format!("{refused:?}"))?;

    assert_eq!(answer, "甲的改写\n");
    assert_eq!(ran, candidate("claude", "sonnet"));
    assert_eq!(fixture.calls(), 1);
    Ok(())
}

/// The environment outranks the file, which is the tier the reference
/// implementation reads before the configured engine.
#[test]
fn single_lets_the_environment_outrank_the_configured_engine() -> TestResult {
    let fixture = Fixture::new("single-environment")?;
    fixture.install("claude", &answering("甲的改写"))?;
    fixture.install("grok", &answering("乙的改写"))?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    let (answer, ran) = single(
        &long(),
        &fixture.with(&[("LIMAE_ENGINE", "grok")]),
        fixture.cwd.path(),
        now(),
    )
    .map_err(|refused| format!("{refused:?}"))?;

    assert_eq!(answer, "乙的改写\n");
    assert_eq!(ran, candidate("grok", "grok-4.6"));
    Ok(())
}

/// `auto` is a name to resolve, not a name to run: an implementation that
/// passed it straight through would look for a preset called `auto`, find none,
/// and fall through to an empty custom command.
#[test]
fn single_asks_the_auto_search_when_nothing_names_an_engine() -> TestResult {
    let fixture = Fixture::new("single-auto")?;
    fixture.install("claude", &probing("甲的改写"))?;

    let (answer, ran) = single(&long(), &fixture.env(), fixture.cwd.path(), now())
        .map_err(|refused| format!("{refused:?}"))?;

    assert_eq!(answer, "甲的改写\n");
    assert_eq!(ran, candidate("claude", "sonnet"));
    Ok(())
}

/// A `model` the user configured is the one that runs, and it is also the one
/// written down. This is the arm that says the two values are read from the
/// settings rather than from the preset.
#[test]
fn single_runs_the_configured_model_and_writes_that_one_down() -> TestResult {
    let fixture = Fixture::new("single-model")?;
    // The Claude template puts the model in its fifth argument.
    fixture.install("claude", "cat > /dev/null\necho \"model=$5\"")?;
    fixture.configure("[polish]\nengine = \"claude\"\nmodel = \"opus\"\n")?;

    let (answer, ran) = single(&long(), &fixture.env(), fixture.cwd.path(), now())
        .map_err(|refused| format!("{refused:?}"))?;

    assert_eq!(answer, "model=opus\n");
    assert_eq!(ran, candidate("claude", "opus"));
    Ok(())
}

/// A typo in `[polish]` is the user's to fix and says so by name, rather than
/// arriving as this file's crash or as an engine's failure.
#[test]
fn single_tells_a_broken_polish_table_from_an_engine_that_did_not_answer() -> TestResult {
    let fixture = Fixture::new("single-config")?;
    fixture.install("claude", "cat > /dev/null\nexit 1")?;

    fixture.configure("[polish]\nengine = \"no-such-engine\"\n")?;
    assert_eq!(
        single(&long(), &fixture.env(), fixture.cwd.path(), now()).err(),
        Some(Refused::Misconfigured)
    );

    fixture.configure("[polish]\nengine = \"claude\"\n")?;
    assert_eq!(
        single(&long(), &fixture.env(), fixture.cwd.path(), now()).err(),
        Some(Refused::Engine(FailureReason::NonzeroExit))
    );
    Ok(())
}

/// A polish engine is itself a coding agent: one whose own display hook fired
/// on the rewrite would be a recursion bounded by nothing.
#[test]
fn single_hands_the_engine_a_hook_that_is_switched_off() -> TestResult {
    let fixture = Fixture::new("single-disable")?;
    let seen = fixture.home.path().join("env");
    fixture.install(
        "claude",
        &format!("cat > /dev/null\nenv > '{}'\necho 甲的改写", seen.display()),
    )?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    // The variable is already set to something else, which is the arm that
    // says it is replaced rather than appended beside the old value.
    let _ = single(
        &long(),
        &fixture.with(&[("LIMAE_HOOK_DISABLE", "0")]),
        fixture.cwd.path(),
        now(),
    )
    .map_err(|refused| format!("{refused:?}"))?;

    let child = fs::read_to_string(&seen)?;
    let values: Vec<&str> = child
        .lines()
        .filter(|line| line.starts_with("LIMAE_HOOK_DISABLE="))
        .collect();
    assert_eq!(values, vec!["LIMAE_HOOK_DISABLE=1"]);
    Ok(())
}

/// `TIMEOUT_VARIABLE` says how long the user waits for a rewrite, and a probe
/// is not one: the reference implementation's `engines.select` takes no timeout
/// at all. The probe here outlasts the hook's deadline and the search still
/// finds the engine, which is the arm that says the two deadlines are apart.
#[test]
fn the_auto_search_does_not_probe_under_the_hooks_own_deadline() -> TestResult {
    let fixture = Fixture::new("single-probe-deadline")?;
    fixture.install(
        "claude",
        &format!(
            "if grep -q LIMAE-PROBE-OK \"$3\"; then cat > /dev/null; sleep 2; \
             echo LIMAE-PROBE-OK; else\n{}\nfi",
            answering("甲的改写")
        ),
    )?;

    let (answer, ran) = single(
        &long(),
        &fixture.with(&[(TIMEOUT_VARIABLE, "1")]),
        fixture.cwd.path(),
        now(),
    )
    .map_err(|refused| format!("{refused:?}"))?;

    assert_eq!(answer, "甲的改写\n");
    assert_eq!(ran, candidate("claude", "sonnet"));
    Ok(())
}

/// The knob is read, and a deadline that passes is a turn that shows its
/// original. Without the knob this call would wait out the default minute and
/// then answer, which is the failure this arm catches.
#[test]
fn an_engine_that_runs_past_the_deadline_answers_nothing() -> TestResult {
    let fixture = Fixture::new("single-timeout")?;
    fixture.install("claude", "cat > /dev/null\nsleep 3\necho 甲的改写")?;
    fixture.configure("[polish]\nengine = \"claude\"\n")?;

    let refused = single(
        &long(),
        &fixture.with(&[(TIMEOUT_VARIABLE, "0.1")]),
        fixture.cwd.path(),
        now(),
    )
    .err();

    assert_eq!(refused, Some(Refused::Engine(FailureReason::TimedOut)));
    Ok(())
}

// -- the deterministic fixes over a turn's rewrites -----------------------

/// The two `fix` lines are two different facts and a diagnostics line that only
/// said "a fix line was written" would not tell them apart: one says a rewrite
/// reached the screen without this repository's rules over it, the other says
/// which model needs the rules to clean up after it (ADR-0008 section 五).
#[test]
fn shown_tells_a_fix_that_would_not_run_from_one_that_repaired_something() -> TestResult {
    let fixture = Fixture::new("shown-kinds")?;
    let cwd = fixture.cwd.path();
    let state = fixture.state.path();

    let clean = vec![LINE.to_owned()];
    let (display, failed) = shown(&clean, state, MESSAGE, cwd, now());
    assert_eq!(display, clean);
    assert_eq!(failed, None);
    assert_eq!(fixture.steps()?, Vec::new());

    // A slip this repository owns: the space beside an inline code span.
    let sloppy = vec!["这是 `code`后面的字。".to_owned()];
    let (display, failed) = shown(&sloppy, state, MESSAGE, cwd, now());
    assert_eq!(display, vec!["这是 `code` 后面的字。".to_owned()]);
    assert_eq!(failed, None);
    assert_eq!(
        fixture.steps()?,
        vec![("fix".to_owned(), "repaired".to_owned())]
    );

    let refused = vec![format!("{UNFIXABLE}\n这是 `code`后面的字。")];
    let (display, failed) = shown(&refused, state, MESSAGE, cwd, now());
    // Passed through as the model wrote it: a rewrite with a typography slip in
    // it still beats no rewrite at all.
    assert_eq!(display, refused);
    assert!(failed.is_some());
    assert_eq!(
        fixture.steps()?,
        vec![
            ("fix".to_owned(), "repaired".to_owned()),
            ("fix".to_owned(), "crashed".to_owned()),
        ]
    );
    Ok(())
}

// -- an ordinary turn -----------------------------------------------------

/// Three answers, three different facts. Reprinting the message the reader just
/// read says nothing, so a rewrite that came back identical is answered in
/// words; a rewrite that moved only what the deterministic rules own is a
/// different sentence again, because calling it no change would be false.
#[test]
fn one_answers_no_change_a_count_and_typography_apart() -> TestResult {
    let unchanged = Fixture::new("one-unchanged")?;
    unchanged.install("mygateway", &answering(&long()))?;
    unchanged.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_eq!(
        one(
            &long(),
            unchanged.state.path(),
            MESSAGE,
            &unchanged.env(),
            unchanged.cwd.path(),
            now(),
        ),
        format!("{HEADING} {UNCHANGED}\n")
    );

    let counted = Fixture::new("one-counted")?;
    let before = format!("{}\n再看要不要引外部工具。", long());
    let after = format!("{}\n再看要不要引入外部工具。", long());
    counted.install("mygateway", &answering(&after))?;
    counted.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_eq!(
        one(
            &before,
            counted.state.path(),
            MESSAGE,
            &counted.env(),
            counted.cwd.path(),
            now(),
        ),
        format!("{HEADING} 1 处改动\n{after}\n")
    );

    let typography = Fixture::new("one-typography")?;
    let before = format!("{}\n\n{}", long(), long());
    let after = format!("{}\n{}", long(), long());
    typography.install("mygateway", &answering(&after))?;
    typography.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    let shown = one(
        &before,
        typography.state.path(),
        MESSAGE,
        &typography.env(),
        typography.cwd.path(),
        now(),
    );
    assert_eq!(shown, format!("{HEADING} {TYPOGRAPHY_ONLY}\n"));
    assert!(!shown.contains(UNCHANGED));
    Ok(())
}

/// The count is the number of changes and not whether there were any: an
/// implementation printing "1" for every non-empty set passes every
/// single-change test there is.
#[test]
fn one_counts_the_changes_rather_than_reporting_that_there_were_some() -> TestResult {
    let fixture = Fixture::new("one-two-changes")?;
    let before = format!(
        "{}\n再看要不要引外部工具，然后把结论写进那一份文件。",
        long()
    );
    let after = format!(
        "{}\n再看要不要引入外部工具，然后把结论写进那一份档案。",
        long()
    );
    fixture.install("mygateway", &answering(&after))?;
    fixture.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;

    assert_eq!(
        one(
            &before,
            fixture.state.path(),
            MESSAGE,
            &fixture.env(),
            fixture.cwd.path(),
            now(),
        ),
        format!("{HEADING} 2 处改动\n{after}\n")
    );
    Ok(())
}

/// ADR-0012: a round whose fixes did not run is not an observation of what
/// polish does, so it does not enter the sample — and it still reaches the
/// screen and still leaves its `fix` line, which is what keeps this from being
/// "a failed round is dropped".
///
/// The two arms differ only in whether the model's rewrite is one the fixer will
/// run over at all, so between them they say which of the two the ledger is
/// keyed on.
#[test]
fn one_leaves_a_round_whose_fixes_did_not_run_out_of_the_sample() -> TestResult {
    let refused = Fixture::new("one-unsampled")?;
    let answer = format!("{UNFIXABLE}\n{}", long());
    refused.install("mygateway", &answering(&answer))?;
    refused.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;

    let shown = one(
        &long(),
        refused.state.path(),
        MESSAGE,
        &refused.env(),
        refused.cwd.path(),
        now(),
    );
    assert!(shown.starts_with(HEADING), "{shown}");
    assert_eq!(
        refused.steps()?,
        vec![("fix".to_owned(), "crashed".to_owned())]
    );
    assert_eq!(refused.records(), Vec::<PathBuf>::new());

    // The control arm: the same turn, with a rewrite the fixer will run over.
    let recorded = Fixture::new("one-sampled")?;
    recorded.install("mygateway", &answering(&long()))?;
    recorded.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    let shown = one(
        &long(),
        recorded.state.path(),
        MESSAGE,
        &recorded.env(),
        recorded.cwd.path(),
        now(),
    );
    assert!(shown.starts_with(HEADING), "{shown}");
    assert_eq!(recorded.steps()?, Vec::new());
    assert_eq!(
        recorded.records(),
        vec![recorded.state.path().join(RUN_DIRECTORY).join("m1.json")]
    );
    Ok(())
}

/// What the model wrote and what the reader saw are two versions of one turn,
/// and folding them together would hide how much of the tidiness was the rules
/// cleaning up after the model.
#[test]
fn one_writes_down_what_the_model_wrote_and_what_was_shown() -> TestResult {
    let fixture = Fixture::new("one-record")?;
    let text = format!("{}\n这是 `code`后面的字。", long());
    let answer = format!("{}\n这是 `code`后面的字，改过了。", long());
    fixture.install("mygateway", &answering(&answer))?;
    fixture.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;

    let _ = one(
        &text,
        fixture.state.path(),
        MESSAGE,
        &fixture.env(),
        fixture.cwd.path(),
        now(),
    );

    let record: Value = serde_json::from_str(&fs::read_to_string(
        fixture.state.path().join(RUN_DIRECTORY).join("m1.json"),
    )?)?;
    assert_eq!(field(&record, "original"), text);
    assert_eq!(field(&record, "text"), answer);
    assert_eq!(
        field(&record, "displayed"),
        format!("{}\n这是 `code` 后面的字，改过了。", long())
    );
    assert_eq!(field(&record, "engine"), "custom");
    Ok(())
}

/// A `[polish]` typo and an engine that fell over are two different people's
/// problems, and a diagnostics line that named them the same way would send
/// whoever is debugging to the wrong file. Both leave the original on screen.
#[test]
fn one_names_a_misconfiguration_and_an_engine_failure_apart() -> TestResult {
    let broken = Fixture::new("one-config")?;
    broken.configure("[polish]\nengine = \"no-such-engine\"\n")?;
    assert_eq!(
        one(
            &long(),
            broken.state.path(),
            MESSAGE,
            &broken.env(),
            broken.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(
        broken.steps()?,
        vec![("single".to_owned(), "config".to_owned())]
    );

    let failing = Fixture::new("one-engine")?;
    failing.install("mygateway", "cat > /dev/null\nexit 1")?;
    failing.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_eq!(
        one(
            &long(),
            failing.state.path(),
            MESSAGE,
            &failing.env(),
            failing.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(
        failing.steps()?,
        vec![("single".to_owned(), "exit".to_owned())]
    );
    Ok(())
}

// -- a sampled turn -------------------------------------------------------

fn drawn() -> Trial {
    Trial {
        code: "灯塔".to_owned(),
        a: candidate("claude", "haiku"),
        b: candidate("grok", "grok-4.6"),
    }
}

/// The two columns reach the screen blind and fixed, and the ledger keeps both
/// versions of each of them.
///
/// Both rewrites carry a slip this repository owns, so the screen and the
/// ledger's `displayed` differ from what the models wrote: showing the raw
/// column would exempt a rewrite from the rules because a model produced it
/// (ADR-0005 section 四), and a ledger holding one version of the two would
/// hide how much of the tidiness was the rules cleaning up after the model.
#[test]
fn a_trial_shows_two_columns_and_writes_the_ledger() -> TestResult {
    let fixture = Fixture::new("trial-columns")?;
    let written = ("甲的改写 `a`后面。", "乙的改写 `b`后面。");
    let displayed = ("甲的改写 `a` 后面。", "乙的改写 `b` 后面。");
    fixture.install("claude", &answering(written.0))?;
    fixture.install("grok", &answering(written.1))?;

    let shown = trial(
        &drawn(),
        &long(),
        fixture.state.path(),
        MESSAGE,
        &fixture.env(),
        fixture.cwd.path(),
        now(),
    );

    assert!(
        shown.contains(&format!("── A ──\n{}", displayed.0)),
        "{shown}"
    );
    assert!(
        shown.contains(&format!("── B ──\n{}", displayed.1)),
        "{shown}"
    );
    assert!(!shown.contains(written.0), "{shown}");
    assert!(!shown.contains(written.1), "{shown}");
    assert!(shown.contains("灯塔"), "{shown}");
    for name in ["claude", "haiku", "grok", "grok-4.6"] {
        assert!(!shown.contains(name), "{name} reached the screen");
    }

    let path = fixture
        .state
        .path()
        .join(LEDGER_DIRECTORY)
        .join("灯塔.json");
    assert_eq!(fixture.ledger(), vec![path.clone()]);
    let entry: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    assert_eq!(field(&entry, "original"), long());
    let columns = entry
        .get("candidates")
        .and_then(Value::as_array)
        .ok_or("the ledger entry has no candidates")?;
    assert_eq!(field(&columns[0], "engine"), "claude");
    assert_eq!(field(&columns[0], "text"), written.0);
    assert_eq!(field(&columns[0], "displayed"), displayed.0);
    assert_eq!(field(&columns[1], "engine"), "grok");
    assert_eq!(field(&columns[1], "text"), written.1);
    assert_eq!(field(&columns[1], "displayed"), displayed.1);
    assert_eq!(fixture.calls(), 2);
    Ok(())
}

/// Losing the evidence is bad; throwing away a rewrite the user waited for is
/// worse. Both arms keep what the reader was waiting for and say in the
/// diagnostics that the writing is what failed, which is the line that stops
/// "the ledger is empty" from being the whole of the evidence.
#[test]
fn a_record_that_cannot_be_written_still_shows_the_rewrite() -> TestResult {
    let ordinary = Fixture::new("record-one")?;
    let answer = format!("{}\n再看要不要引入外部工具。", long());
    ordinary.install("mygateway", &answering(&answer))?;
    ordinary.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    // Where the record would go, as a file rather than a directory.
    fs::write(ordinary.state.path().join(RUN_DIRECTORY), "")?;

    let shown = one(
        &format!("{}\n再看要不要引外部工具。", long()),
        ordinary.state.path(),
        MESSAGE,
        &ordinary.env(),
        ordinary.cwd.path(),
        now(),
    );
    assert_eq!(shown, format!("{HEADING} 1 处改动\n{answer}\n"));
    assert_eq!(
        ordinary.steps()?,
        vec![("record".to_owned(), "crashed".to_owned())]
    );

    let sampled = Fixture::new("record-trial")?;
    sampled.install("claude", &answering("甲的改写"))?;
    sampled.install("grok", &answering("乙的改写"))?;
    fs::write(sampled.state.path().join(LEDGER_DIRECTORY), "")?;

    let shown = trial(
        &drawn(),
        &long(),
        sampled.state.path(),
        MESSAGE,
        &sampled.env(),
        sampled.cwd.path(),
        now(),
    );
    assert!(shown.contains("── A ──\n甲的改写"), "{shown}");
    assert!(shown.contains("── B ──\n乙的改写"), "{shown}");
    assert_eq!(
        sampled.steps()?,
        vec![("record".to_owned(), "crashed".to_owned())]
    );
    Ok(())
}

/// One candidate short is not a comparison, and a second round of calls would
/// make the user wait twice. The call count is the arm that says there was no
/// retry; the diagnostics line is the arm that says which way it lost.
#[test]
fn a_trial_that_loses_a_candidate_shows_the_original_and_does_not_run_again() -> TestResult {
    let fixture = Fixture::new("trial-lost")?;
    fixture.install("claude", &answering("甲的改写"))?;
    fixture.install("grok", "cat > /dev/null\nexit 1")?;

    let shown = trial(
        &drawn(),
        &long(),
        fixture.state.path(),
        MESSAGE,
        &fixture.env(),
        fixture.cwd.path(),
        now(),
    );

    assert_eq!(shown, "");
    assert_eq!(fixture.calls(), 2);
    assert_eq!(fixture.ledger(), Vec::<PathBuf>::new());
    assert_eq!(fixture.steps()?, vec![("ab".to_owned(), "exit".to_owned())]);
    Ok(())
}

// -- which of the two a message gets --------------------------------------

/// Short messages are left alone, and short is measured in prose: the knob and
/// the default are the same measure, so the arm that moves the knob says the
/// knob is read at all.
#[test]
fn a_short_message_is_left_alone_and_the_floor_is_a_knob() -> TestResult {
    let fixture = Fixture::new("block-short")?;
    fixture.install("mygateway", &answering(&long()))?;
    fixture.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    let short = "好的。";

    assert_eq!(
        block(
            short,
            fixture.state.path(),
            MESSAGE,
            &fixture.env(),
            fixture.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(fixture.calls(), 0);

    // The same message under a floor it clears.
    assert_ne!(
        block(
            short,
            fixture.state.path(),
            MESSAGE,
            &fixture.with(&[(MIN_CHARS_VARIABLE, "1")]),
            fixture.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(fixture.calls(), 1);
    Ok(())
}

/// A message that is long only because it carries code is a short message as
/// far as polishing is concerned. Counting characters instead of prose would
/// send this one to a model.
#[test]
fn a_message_that_is_long_only_in_code_is_left_alone() -> TestResult {
    let fixture = Fixture::new("block-code")?;
    fixture.install("mygateway", &answering(&long()))?;
    fixture.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    let fenced = format!("```\n{}\n```", long());

    assert_eq!(
        block(
            &fenced,
            fixture.state.path(),
            MESSAGE,
            &fixture.env(),
            fixture.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(fixture.calls(), 0);

    // The same characters, outside the fence.
    assert_ne!(
        block(
            &long(),
            fixture.state.path(),
            MESSAGE,
            &fixture.env(),
            fixture.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(fixture.calls(), 1);
    Ok(())
}

/// A turn that was not polished writes no run record.
///
/// The record is of what polish did. A message too short to be worth polishing
/// and an engine that would not answer both did nothing, so both have nothing
/// to write down — a directory of records has to mean what it says, or the
/// evidence it holds is diluted by entries for turns no model ever saw. Today
/// only the third way of doing nothing, a fix that would not run, has an arm.
#[test]
fn a_turn_that_was_not_polished_writes_no_run_record() -> TestResult {
    let short = Fixture::new("record-short")?;
    short.install("mygateway", &answering(&long()))?;
    short.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_eq!(
        block(
            "好的。",
            short.state.path(),
            MESSAGE,
            &short.with(&[(RATE_VARIABLE, "0")]),
            short.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(short.records(), Vec::<PathBuf>::new());

    let failing = Fixture::new("record-failing")?;
    failing.install("mygateway", "cat > /dev/null\nexit 1")?;
    failing.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_eq!(
        block(
            &long(),
            failing.state.path(),
            MESSAGE,
            &failing.with(&[(RATE_VARIABLE, "0")]),
            failing.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(failing.records(), Vec::<PathBuf>::new());

    // Control arm: the same message through an engine that answers does write
    // one, so "no record" above is the turn and not a directory nothing ever
    // reaches.
    let polished = Fixture::new("record-polished")?;
    polished.install(
        "mygateway",
        &answering("ACME 的报告写得不错，读起来像人话。"),
    )?;
    polished.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    assert_ne!(
        block(
            &long(),
            polished.state.path(),
            MESSAGE,
            &polished.with(&[(RATE_VARIABLE, "0")]),
            polished.cwd.path(),
            now(),
        ),
        ""
    );
    assert_eq!(polished.records().len(), 1);
    Ok(())
}

/// The rate decides which of the two blocks a message gets, and the two ends of
/// it are the deterministic arms: one engine and an ordinary block, or two
/// engines and a blind comparison.
#[test]
fn the_rate_decides_between_one_engine_and_two() -> TestResult {
    let sampled = Fixture::new("block-sampled")?;
    // `claude` alone carries two candidates, so one stub is a whole pool.
    sampled.install(
        "claude",
        "cat > /dev/null\ncase \"$5\" in haiku) echo 甲的改写;; *) echo 乙的改写;; esac",
    )?;
    let shown = block(
        &long(),
        sampled.state.path(),
        MESSAGE,
        &sampled.with(&[(RATE_VARIABLE, "1")]),
        sampled.cwd.path(),
        now(),
    );
    assert!(shown.contains("── A ──"), "{shown}");
    assert!(shown.contains("── B ──"), "{shown}");
    assert_eq!(sampled.calls(), 2);
    assert_eq!(sampled.ledger().len(), 1);

    let ordinary = Fixture::new("block-ordinary")?;
    ordinary.install("mygateway", &answering(&long()))?;
    ordinary.configure("[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n")?;
    let shown = block(
        &long(),
        ordinary.state.path(),
        MESSAGE,
        &ordinary.with(&[(RATE_VARIABLE, "0")]),
        ordinary.cwd.path(),
        now(),
    );
    assert_eq!(shown, format!("{HEADING} {UNCHANGED}\n"));
    assert_eq!(ordinary.calls(), 1);
    assert_eq!(ordinary.ledger(), Vec::<PathBuf>::new());
    Ok(())
}

/// The sampled turn's engines are told the hook is off too, for the same reason
/// the ordinary one's is.
#[test]
fn a_sampled_turn_hands_its_engines_a_hook_that_is_switched_off() -> TestResult {
    let fixture = Fixture::new("trial-disable")?;
    let seen = fixture.home.path().join("env");
    fixture.install(
        "claude",
        &format!("cat > /dev/null\nenv > '{}'\necho 甲的改写", seen.display()),
    )?;
    fixture.install("grok", &answering("乙的改写"))?;

    let _ = trial(
        &drawn(),
        &long(),
        fixture.state.path(),
        MESSAGE,
        &fixture.env(),
        fixture.cwd.path(),
        now(),
    );

    let child = fs::read_to_string(&seen)?;
    assert!(
        child.lines().any(|line| line == "LIMAE_HOOK_DISABLE=1"),
        "{child}"
    );
    Ok(())
}
