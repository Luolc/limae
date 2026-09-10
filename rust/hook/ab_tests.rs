use super::{
    CANDIDATES, CODE_NAMES, Candidate, LEDGER_DIRECTORY, PENDING_FILENAME, RUN_DIRECTORY, Trial,
    context, draw, pair, pool, preset, record, record_run, render, run, used,
};
use crate::polish::diagnosis::FailureReason;
use crate::polish::engines::EngineLimits;
use crate::polish::select::installed;

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
const NOW: Duration = Duration::from_micros(1_700_000_000_500_000);

fn now() -> SystemTime {
    UNIX_EPOCH + NOW
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-ab-{name}-{}-{}",
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

/// One environment of a run: a `PATH` of fake CLIs and a home nothing real
/// lives in, so that no assertion depends on what this machine has installed.
fn environment(bin: &Path, home: &Path) -> Vec<(OsString, OsString)> {
    [
        ("PATH", bin.as_os_str().to_owned()),
        ("HOME", home.as_os_str().to_owned()),
        ("XDG_CACHE_HOME", home.join("cache").into_os_string()),
    ]
    .into_iter()
    .map(|(name, value)| (OsString::from(name), value))
    .collect()
}

/// Put one executable of the given name on the fake `PATH`.
///
/// The engine's own `PATH` is the fake one, which holds these scripts and
/// nothing else, so a script that needs `sleep` or `touch` gets this run's real
/// `PATH` back at the top of itself. Without that the tools are simply missing
/// and a script meant to wait returns at once — which is a test that no longer
/// measures what it says it does.
fn install(bin: &Path, name: &str, script: &str) -> TestResult {
    let tools = std::env::var("PATH").unwrap_or_default();
    let path = bin.join(name);
    fs::write(
        &path,
        format!("#!/bin/sh\nPATH='{tools}'\nexport PATH\n{script}\n"),
    )?;
    fs::set_permissions(&path, Permissions::from_mode(0o755))?;
    Ok(())
}

fn candidate(engine: &str, model: &str) -> Candidate {
    Candidate {
        engine: engine.to_owned(),
        model: model.to_owned(),
    }
}

fn trial() -> Trial {
    Trial {
        code: "灯塔".to_owned(),
        a: candidate("codex", "gpt-5.6-luna"),
        b: candidate("claude", "haiku"),
    }
}

/// Carry one engine failure out of a test as something `?` can report.
fn why(reason: FailureReason) -> String {
    reason.to_string()
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

/// Every name in the pool is a preset this repository can actually run. A
/// renamed engine would otherwise leave a candidate that draws fine and fails
/// with "no engine" the moment it is picked.
#[test]
fn every_candidate_names_a_preset() {
    for (engine, model) in CANDIDATES {
        assert!(preset(engine).is_some(), "unknown engine {engine}");
        assert!(!model.is_empty());
    }
}

#[test]
fn the_pool_is_the_candidates_whose_cli_is_installed() -> TestResult {
    let (bin, home) = (TempDir::new("pool-bin")?, TempDir::new("pool-home")?);
    let env = environment(bin.path(), home.path());
    assert!(pool(&env).is_empty());

    install(bin.path(), "claude", "true")?;
    assert_eq!(
        pool(&env),
        vec![candidate("claude", "haiku"), candidate("claude", "sonnet")]
    );

    install(bin.path(), "codex", "true")?;
    let both = pool(&env);
    // The order is `CANDIDATES`', not the order the CLIs turned up in.
    assert_eq!(both[0], candidate("codex", "gpt-5.6-luna"));
    assert_eq!(both[3], candidate("claude", "haiku"));
    assert_eq!(both.len(), 5);
    Ok(())
}

/// The two ends of the rate are the deterministic arms: `random()` is always
/// below 1 and never below 0, so these two say whether the sampling gate is
/// wired the right way round at all.
#[test]
fn a_turn_is_sampled_at_the_rate_and_not_otherwise() -> TestResult {
    let (bin, home, state) = (
        TempDir::new("rate-bin")?,
        TempDir::new("rate-home")?,
        TempDir::new("rate")?,
    );
    install(bin.path(), "claude", "true")?;
    let env = environment(bin.path(), home.path());

    for _ in 0..20 {
        assert!(draw(state.path(), &env, 1.0).is_some());
        assert!(draw(state.path(), &env, 0.0).is_none());
    }
    Ok(())
}

/// One installed engine is not a comparison. This arm and the one above differ
/// only in what is on `PATH`, so between them they say which of the two
/// `None`s a turn got.
#[test]
fn a_trial_needs_two_installed_candidates() -> TestResult {
    let (bin, home, state) = (
        TempDir::new("two-bin")?,
        TempDir::new("two-home")?,
        TempDir::new("two")?,
    );
    let env = environment(bin.path(), home.path());
    assert!(draw(state.path(), &env, 1.0).is_none());

    // `claude` alone already carries two candidates, so the arm that has to be
    // one candidate short is the one with no CLI at all — and one with a
    // single CLI is the arm that shows the count is of candidates.
    install(bin.path(), "grok", "true")?;
    assert_eq!(pool(&env).len(), 2);
    assert!(draw(state.path(), &env, 1.0).is_some());
    Ok(())
}

#[test]
fn a_session_that_has_used_every_code_name_draws_no_more() -> TestResult {
    let (bin, home, state) = (
        TempDir::new("names-bin")?,
        TempDir::new("names-home")?,
        TempDir::new("names")?,
    );
    install(bin.path(), "claude", "true")?;
    let env = environment(bin.path(), home.path());

    let ledger = state.path().join(LEDGER_DIRECTORY);
    fs::create_dir_all(&ledger)?;
    for name in CODE_NAMES {
        fs::write(ledger.join(format!("{name}.json")), "{}")?;
    }
    assert_eq!(used(state.path()).len(), CODE_NAMES.len());
    assert!(draw(state.path(), &env, 1.0).is_none());

    // Free one name and the very next draw is that name: the register is the
    // ledger's own file names, not a counter.
    fs::remove_file(ledger.join(format!("{}.json", CODE_NAMES[7])))?;
    let drawn = draw(state.path(), &env, 1.0).ok_or("no trial")?;
    assert_eq!(drawn.code, CODE_NAMES[7]);
    Ok(())
}

/// A trial of one candidate against itself measures nothing.
#[test]
fn the_two_columns_are_never_the_same_candidate() -> TestResult {
    let (bin, home, state) = (
        TempDir::new("pair-bin")?,
        TempDir::new("pair-home")?,
        TempDir::new("pair")?,
    );
    install(bin.path(), "claude", "true")?;
    install(bin.path(), "grok", "true")?;
    let env = environment(bin.path(), home.path());
    let installed = pool(&env);

    let mut seen = std::collections::HashSet::new();
    for _ in 0..200 {
        let drawn = draw(state.path(), &env, 1.0).ok_or("no trial")?;
        assert_ne!(drawn.a, drawn.b);
        assert!(installed.contains(&drawn.a) && installed.contains(&drawn.b));
        assert!(CODE_NAMES.contains(&drawn.code.as_str()));
        let _ = seen.insert(format!("{:?}{:?}", drawn.a, drawn.b));
    }
    // Both orders of at least one pair, or the draw is not a draw.
    assert!(seen.len() > 2, "only {} pairings in 200 draws", seen.len());
    Ok(())
}

/// A pool too small to draw two from draws nothing at all — including the pool
/// of one, which no preset can produce today because every one of them carries
/// at least two models, and which would otherwise be a division by zero on the
/// day one does.
#[test]
fn a_pool_too_small_for_two_draws_nothing() -> TestResult {
    assert_eq!(pair(&[]), None);
    assert_eq!(pair(&[candidate("claude", "haiku")]), None);
    let two = [candidate("claude", "haiku"), candidate("grok", "grok-4.6")];
    let (a, b) = pair(&two).ok_or("a pool of two drew nothing")?;
    assert_ne!(a, b);
    assert!(two.contains(&a) && two.contains(&b));
    Ok(())
}

#[test]
fn both_candidates_answer_and_the_answers_come_back_stripped() -> TestResult {
    let (bin, home) = (TempDir::new("run-bin")?, TempDir::new("run-home")?);
    install(bin.path(), "claude", "printf '  甲的改写 \\n\\n'")?;
    install(bin.path(), "grok", "printf '乙的改写\\n'")?;
    let env = environment(bin.path(), home.path());
    let trial = Trial {
        code: "灯塔".to_owned(),
        a: candidate("claude", "haiku"),
        b: candidate("grok", "grok-4.6"),
    };

    let answers = run(&trial, "一句中文正文。", &env, limits()).map_err(why)?;
    assert_eq!(answers, ("甲的改写".to_owned(), "乙的改写".to_owned()));
    Ok(())
}

/// The reason belongs to the earlier column, not to whichever call fell over
/// first. The two candidates here fail differently and the earlier one is the
/// slower one, so a run that reported the first failure in time would say
/// `empty` instead.
#[test]
fn the_reason_is_the_earlier_columns_and_not_the_quicker_ones() -> TestResult {
    let (bin, home) = (TempDir::new("order-bin")?, TempDir::new("order-home")?);
    install(bin.path(), "claude", "sleep 0.4; exit 3")?;
    install(bin.path(), "grok", "exit 0")?;
    let env = environment(bin.path(), home.path());
    let slow_first = Trial {
        code: "灯塔".to_owned(),
        a: candidate("claude", "haiku"),
        b: candidate("grok", "grok-4.6"),
    };
    let quick_first = Trial {
        code: "山谷".to_owned(),
        a: candidate("grok", "grok-4.6"),
        b: candidate("claude", "haiku"),
    };

    assert_eq!(
        run(&slow_first, "一句中文正文。", &env, limits()),
        Err(FailureReason::NonzeroExit)
    );
    assert_eq!(
        run(&quick_first, "一句中文正文。", &env, limits()),
        Err(FailureReason::EmptyAnswer)
    );
    Ok(())
}

/// The two candidates run at once, and this says so without timing anything:
/// each waits for the other's marker and answers only if it arrives. Run one
/// after the other, the first would wait out its loop, answer nothing, and the
/// trial would come back `empty`.
#[test]
fn the_two_candidates_run_at_the_same_time() -> TestResult {
    let (bin, home, meet) = (
        TempDir::new("both-bin")?,
        TempDir::new("both-home")?,
        TempDir::new("both-meet")?,
    );
    let rendezvous = |mine: &str, theirs: &str, answer: &str| {
        let (mine, theirs) = (meet.path().join(mine), meet.path().join(theirs));
        format!(
            "touch '{}'\n\
             n=0\n\
             while [ ! -f '{}' ] && [ $n -lt 400 ]; do sleep 0.01; n=$((n+1)); done\n\
             [ -f '{}' ] && printf '{answer}\\n'\n",
            mine.display(),
            theirs.display(),
            theirs.display(),
        )
    };
    install(bin.path(), "claude", &rendezvous("a", "b", "甲"))?;
    install(bin.path(), "grok", &rendezvous("b", "a", "乙"))?;
    let env = environment(bin.path(), home.path());
    let trial = Trial {
        code: "灯塔".to_owned(),
        a: candidate("claude", "haiku"),
        b: candidate("grok", "grok-4.6"),
    };

    assert_eq!(
        run(&trial, "一句中文正文。", &env, limits()).map_err(why)?,
        ("甲".to_owned(), "乙".to_owned())
    );
    Ok(())
}

#[test]
fn the_block_is_two_labelled_columns_under_one_code_name() {
    assert_eq!(
        render(&trial(), ("甲的显示", "乙的显示")),
        "[A/B 灯塔] 原文如上，以下是两个候选：\n\n\
         ── A ──\n甲的显示\n\n\
         ── B ──\n乙的显示\n"
    );
}

/// Blind is a property of a pair of places, so it takes a pair of arms: the
/// names must be absent from everything the reader sees and present in the
/// ledger. The absent half alone would pass for a hook that showed nothing at
/// all.
#[test]
fn the_screen_never_names_a_model_and_the_ledger_always_does() -> TestResult {
    let state = TempDir::new("blind")?;
    let trial = trial();
    record(
        state.path(),
        &trial,
        "原文。",
        ("甲的改写", "乙的改写"),
        ("甲的显示", "乙的显示"),
        now(),
    )?;

    let seen = format!(
        "{}{}",
        render(&trial, ("甲的显示", "乙的显示")),
        context(state.path())
    );
    let ledger = fs::read_to_string(state.path().join(LEDGER_DIRECTORY).join("灯塔.json"))?;
    for name in [
        trial.a.engine.as_str(),
        trial.a.model.as_str(),
        trial.b.engine.as_str(),
        trial.b.model.as_str(),
    ] {
        assert!(!seen.contains(name), "{name} reached the screen");
        assert!(ledger.contains(name), "{name} is missing from the ledger");
    }
    assert!(seen.contains("灯塔"));
    Ok(())
}

/// The ledger is evidence, and evidence the two implementations disagree about
/// is two records of one turn. These are the reference implementation's own
/// bytes, down to the key order it writes and the newline it does not.
#[test]
fn one_trial_is_written_the_way_the_reference_implementation_writes_it() -> TestResult {
    let state = TempDir::new("record")?;
    record(
        state.path(),
        &trial(),
        "原文\"引号\"。",
        ("甲的改写", "乙的改写"),
        ("甲的显示", "乙的显示"),
        now(),
    )?;

    let candidates = "\"candidates\": [\n    {\n      \"label\": \"A\",\n      \
        \"engine\": \"codex\",\n      \"model\": \"gpt-5.6-luna\",\n      \
        \"text\": \"甲的改写\",\n      \"displayed\": \"甲的显示\"\n    },\n    \
        {\n      \"label\": \"B\",\n      \"engine\": \"claude\",\n      \
        \"model\": \"haiku\",\n      \"text\": \"乙的改写\",\n      \
        \"displayed\": \"乙的显示\"\n    }\n  ]";
    let at = "\"at\": \"2023-11-14T22:13:20.500000+00:00\"";
    let ledger = state.path().join(LEDGER_DIRECTORY).join("灯塔.json");
    assert_eq!(
        fs::read_to_string(&ledger)?,
        format!(
            "{{\n  \"code\": \"灯塔\",\n  {at},\n  \
             \"original\": \"原文\\\"引号\\\"。\",\n  {candidates}\n}}"
        )
    );
    let pending = state.path().join(PENDING_FILENAME);
    assert_eq!(
        fs::read_to_string(&pending)?,
        format!("{{\n  \"code\": \"灯塔\",\n  {at},\n  {candidates}\n}}")
    );

    assert_eq!(mode(&ledger)?, 0o600);
    assert_eq!(mode(&pending)?, 0o600);
    assert_eq!(mode(&state.path().join(LEDGER_DIRECTORY))?, 0o700);
    assert_eq!(left_behind(state.path())?, Vec::<String>::new());
    Ok(())
}

#[test]
fn one_un_sampled_turn_is_written_the_way_the_reference_implementation_writes_it() -> TestResult {
    let state = TempDir::new("record-run")?;
    record_run(
        state.path(),
        "m1",
        "原文",
        "写的",
        "显示的",
        &candidate("claude", "sonnet"),
        UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    )?;

    let path = state.path().join(RUN_DIRECTORY).join("m1.json");
    assert_eq!(
        fs::read_to_string(&path)?,
        "{\n  \"at\": \"2023-11-14T22:13:20+00:00\",\n  \"message_id\": \"m1\",\n  \
         \"engine\": \"claude\",\n  \"model\": \"sonnet\",\n  \"original\": \"原文\",\n  \
         \"text\": \"写的\",\n  \"displayed\": \"显示的\"\n}"
    );
    assert_eq!(mode(&path)?, 0o600);
    assert_eq!(mode(&state.path().join(RUN_DIRECTORY))?, 0o700);
    assert_eq!(left_behind(state.path())?, Vec::<String>::new());
    Ok(())
}

/// Announced once: a `Stop` hook that answers every time re-triggers itself and
/// the session never ends.
#[test]
fn the_trial_is_announced_once_and_names_the_ledger() -> TestResult {
    let state = TempDir::new("context")?;
    assert_eq!(context(state.path()), "");

    record(
        state.path(),
        &trial(),
        "原文。",
        ("甲的改写", "乙的改写"),
        ("甲的显示", "乙的显示"),
        now(),
    )?;
    let announced = context(state.path());
    assert!(announced.contains("「灯塔」"), "{announced}");
    assert!(
        announced.contains(
            &state
                .path()
                .join(LEDGER_DIRECTORY)
                .join("灯塔.json")
                .display()
                .to_string()
        ),
        "{announced}"
    );
    assert!(!state.path().join(PENDING_FILENAME).exists());
    assert_eq!(context(state.path()), "");
    Ok(())
}

/// The whole announcement, character for character.
///
/// The two arms above read this string with `contains`, which can only find
/// what it was told to look for. What has to stay out of it is the thing nobody
/// thought to name: a third string riding along — a model, an engine, a
/// rewrite — is a leak the ledger cannot describe, because the ledger only
/// knows what it wrote down. Pinning the whole string is the only assertion
/// that fails on a name no one predicted.
#[test]
fn the_announcement_is_the_code_the_path_and_nothing_else() -> TestResult {
    let state = TempDir::new("context-exact")?;
    let trial = trial();
    record(
        state.path(),
        &trial,
        "原文。",
        ("甲的改写", "乙的改写"),
        ("甲的显示", "乙的显示"),
        now(),
    )?;
    let ledger = state.path().join(LEDGER_DIRECTORY).join("灯塔.json");

    assert_eq!(
        context(state.path()),
        format!(
            "limae A/B：本轮有一次 A/B 对照，编号「灯塔」，两栏是盲评。\
             型号对应在 {}；用户按编号给出偏好之前不要说出哪一栏是哪个模型。",
            ledger.display()
        )
    );
    Ok(())
}

/// The rewrites stay out of the model's context (ADR-0009 section 五).
///
/// This is its own arm because the pair of places differ: the two rewrites
/// belong on the screen, where the user compares them, and must not reach the
/// context, where the model would read its own candidate back. An assertion
/// that concatenates screen and context cannot tell which half a string is in,
/// so it cannot make this claim at all.
#[test]
fn the_two_rewrites_reach_the_screen_and_not_the_context() -> TestResult {
    let state = TempDir::new("context-rewrites")?;
    let trial = trial();
    let written = ("甲的改写", "乙的改写");
    let displayed = ("甲的显示", "乙的显示");
    record(state.path(), &trial, "原文。", written, displayed, now())?;

    let announced = context(state.path());
    let screen = render(&trial, displayed);
    for text in [written.0, written.1, displayed.0, displayed.1] {
        assert!(!announced.contains(text), "{text} reached the context");
    }
    for text in [displayed.0, displayed.1] {
        assert!(screen.contains(text), "{text} never reached the screen");
    }
    Ok(())
}

/// The pool's shape: distinct names, two Chinese characters each.
///
/// The count is in the type. Distinctness is not, and neither is the writing
/// system: a code name is what a person reads back to say which column they
/// preferred, and a duplicate in this array is two trials of one session
/// writing over each other's ledger entry — the second trial's evidence is
/// gone, and nothing anywhere says so.
#[test]
fn the_code_names_are_forty_eight_distinct_two_character_names() {
    let distinct: std::collections::HashSet<&str> = CODE_NAMES.into_iter().collect();
    assert_eq!(distinct.len(), CODE_NAMES.len());
    for name in CODE_NAMES {
        assert_eq!(name.chars().count(), 2, "{name}");
        assert!(
            name.chars()
                .all(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)),
            "{name}"
        );
    }
}

#[test]
fn a_pending_file_that_names_no_trial_announces_nothing() -> TestResult {
    let state = TempDir::new("context-empty")?;
    let pending = state.path().join(PENDING_FILENAME);
    for written in [
        "{\"code\": \"\"}",
        "{\"at\": \"2023-11-14T22:13:20+00:00\"}",
    ] {
        fs::write(&pending, written)?;
        assert_eq!(context(state.path()), "", "for {written}");
    }
    Ok(())
}

/// Engine calls run against fake CLIs on a fake `PATH`, so the only wait that
/// can happen is a bug; the deadline is short enough that one does not hold the
/// suite.
fn limits() -> EngineLimits {
    EngineLimits {
        timeout: Duration::from_secs(20),
        ..EngineLimits::default()
    }
}

/// Return the names of any half-written files left in the session directory.
fn left_behind(directory: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_owned()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.to_string_lossy().contains(".writing") {
                found.push(path.to_string_lossy().into_owned());
            }
        }
    }
    Ok(found)
}

/// `installed` is the pool's only filter, and the fake `PATH` these tests are
/// built on has to be a `PATH` it actually reads.
#[test]
fn the_fake_path_is_the_one_the_pool_looks_at() -> TestResult {
    let (bin, home) = (TempDir::new("path-bin")?, TempDir::new("path-home")?);
    let env = environment(bin.path(), home.path());
    let claude = preset("claude").ok_or("no claude preset")?;
    assert!(!installed(claude, &env));
    install(bin.path(), "claude", "true")?;
    assert!(installed(claude, &env));
    Ok(())
}
