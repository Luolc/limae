use super::{SIBLING_POLL, SIBLING_WAIT, assemble, number, tidy};
use crate::hook::state::{self, DIAGNOSTICS_FILENAME, Kind};

use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-hook-parts-{name}-{}-{}",
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

fn environment(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect()
}

/// Write one batch the way [`state::keep`] does, so the reader under test is
/// reading what the writer under test writes.
fn cache(parts: &Path, index: usize, delta: &str) -> TestResult {
    state::keep(parts, index, delta)?;
    Ok(())
}

fn diagnostics(directory: &Path) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    fs::read_to_string(directory.join(DIAGNOSTICS_FILENAME))?
        .lines()
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

// -- the deterministic fixes over a rewrite -------------------------------

/// The whole reason `tidy` exists: a model rewriting Chinese prose drops the
/// space beside an inline code span, and this repository owns that.
#[test]
fn tidy_fixes_the_typography_a_model_left_in_a_rewrite() -> TestResult {
    let cwd = TempDir::new("tidy")?;
    let (fixed, failure) = tidy("这是 `code`后面的字。", cwd.path());

    assert_eq!(fixed, "这是 `code` 后面的字。");
    assert_eq!(failure, None);
    Ok(())
}

/// The `cwd` is the configuration's, not a decoration: a repository that has
/// switched a rule off keeps it off for what the hook puts on screen.
#[test]
fn tidy_obeys_the_rule_configuration_of_the_directory_it_is_given() -> TestResult {
    let cwd = TempDir::new("tidy-config")?;
    fs::write(
        cwd.path().join("limae.toml"),
        "disable = [\"zh-typography-7\"]\n",
    )?;
    let (fixed, failure) = tidy("这是 `code`后面的字。", cwd.path());

    assert_eq!(fixed, "这是 `code`后面的字。");
    assert_eq!(failure, None);
    Ok(())
}

/// A rewrite with a typography slip in it still beats no rewrite at all, so the
/// fixer declining is a marked pass-through and never an error upwards. The
/// text names a rule that does not exist, which is what the fixer refuses on.
#[test]
fn tidy_returns_the_rewrite_unchanged_when_the_fixer_will_not_run() -> TestResult {
    let cwd = TempDir::new("tidy-crash")?;
    let text = "<!-- limae-disable zh-typography-99 -->\n这是 `code`后面的字。";
    let (fixed, failure) = tidy(text, cwd.path());

    assert_eq!(fixed, text);
    assert_eq!(failure, Some(Kind::Crashed));
    Ok(())
}

// -- the numeric knobs ----------------------------------------------------

#[test]
fn number_reads_the_variable_it_is_named() {
    let env = environment(&[("LIMAE_HOOK_TIMEOUT", "12.5"), ("OTHER", "1")]);
    assert!((number(&env, "LIMAE_HOOK_TIMEOUT", 60.0) - 12.5).abs() < f64::EPSILON);
}

/// A typo in a setting is not a reason to interrupt the user, so every way of
/// not being a number ends at the fallback rather than at an error.
#[test]
fn number_falls_back_for_anything_that_is_not_a_number() {
    for text in ["", "  ", "sixty", "1,000", "30s", "0x10", "1_000", "１２"] {
        let env = environment(&[("LIMAE_HOOK_TIMEOUT", text)]);
        assert!(
            (number(&env, "LIMAE_HOOK_TIMEOUT", 60.0) - 60.0).abs() < f64::EPSILON,
            "{text:?} should not have been read as a number"
        );
    }
    assert!((number(&[], "LIMAE_HOOK_TIMEOUT", 60.0) - 60.0).abs() < f64::EPSILON);
}

/// The forms the reference implementation's `float()` takes and this has to
/// take too, surrounding whitespace included.
#[test]
fn number_takes_the_forms_a_person_writes_a_number_in() {
    for (text, expected) in [(" 30 ", 30.0), ("1e3", 1000.0), (".5", 0.5), ("-2.", -2.0)] {
        let env = environment(&[("LIMAE_HOOK_TIMEOUT", text)]);
        let read = number(&env, "LIMAE_HOOK_TIMEOUT", 60.0);
        assert!(
            (read - expected).abs() < f64::EPSILON,
            "{text:?} read as {read}, not {expected}"
        );
    }
}

// -- putting a message back together --------------------------------------

#[test]
fn assemble_joins_every_batch_in_index_order() -> TestResult {
    let session = TempDir::new("assemble")?;
    let parts = session.path().join("parts/message");
    for (index, delta) in ["第一段。\n", "第二段。\n", "第三段。\n"]
        .iter()
        .enumerate()
    {
        cache(&parts, index, delta)?;
    }
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        3,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    assert_eq!(whole.as_deref(), Some("第一段。\n第二段。\n第三段。\n"));
    assert_eq!(stderr, b"");
    assert!(!session.path().join(DIAGNOSTICS_FILENAME).exists());
    Ok(())
}

/// The wait is the point: the host starts one process per batch without waiting
/// for it, so the final batch can be here before a sibling is. The deadline is
/// [`SIBLING_WAIT`] from now, as the caller sets it, and the sibling lands well
/// inside it — a wait shorter than the sleep below turns this red.
#[test]
fn assemble_waits_for_a_sibling_that_is_still_being_written() -> TestResult {
    let session = TempDir::new("assemble-wait")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    let late = parts.clone();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        state::keep(&late, 1, "第二段。\n")
    });
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        2,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;

    writer.join().map_err(|_| "the writing thread panicked")??;
    assert_eq!(whole.as_deref(), Some("第一段。\n第二段。\n"));
    assert_eq!(stderr, b"");
    Ok(())
}

/// A message with a hole in it: three batches were announced, two of them are
/// on disk, and the third never comes. Polishing what did arrive would put a
/// paragraph the user never wrote under a message that says it is theirs, so
/// the turn ends the way every other failure does — and says so three ways.
#[test]
fn assemble_gives_up_on_a_message_that_is_still_missing_a_batch() -> TestResult {
    let session = TempDir::new("assemble-hole")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    cache(&parts, 2, "第三段。\n")?;
    let mut stderr = Vec::new();

    let started = Instant::now();
    let whole = assemble(
        &parts,
        3,
        started + Duration::from_millis(150),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;
    let waited = started.elapsed();

    assert_eq!(whole, None);
    assert!(
        waited >= Duration::from_millis(150),
        "gave up after {waited:?}"
    );
    assert_eq!(
        String::from_utf8(stderr)?,
        "limae hook: 2/3 batches arrived before the deadline; showing the original\n"
    );
    let written = diagnostics(session.path())?;
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["message_id"], "message");
    assert_eq!(written[0]["step"], "assemble");
    assert_eq!(written[0]["kind"], "incomplete");
    Ok(())
}

/// The count in that line is of what arrived, not of what was asked for: it is
/// the one number worth saying, and the batches themselves are the user's own
/// text and stay out of every log.
#[test]
fn the_line_about_a_hole_counts_the_batches_and_quotes_none_of_them() -> TestResult {
    let session = TempDir::new("assemble-count")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "机密的第一段。\n")?;
    let mut stderr = Vec::new();

    let whole = assemble(
        &parts,
        4,
        Instant::now(),
        session.path(),
        "message",
        now(),
        &mut stderr,
    )?;
    let line = String::from_utf8(stderr)?;

    assert_eq!(whole, None);
    assert!(line.starts_with("limae hook: 1/4 batches"), "{line:?}");
    assert!(!line.contains("机密"), "{line:?}");
    Ok(())
}

/// Reading a batch back can fail, and that is the caller's to fail open on the
/// way it fails open on everything else — not something this quietly reports as
/// a hole, which would say a batch never arrived when one did.
#[test]
fn assemble_reports_a_batch_it_cannot_read_rather_than_calling_it_missing() -> TestResult {
    let session = TempDir::new("assemble-unreadable")?;
    let parts = session.path().join("parts/message");
    cache(&parts, 0, "第一段。\n")?;
    let mut file = fs::File::create(state::part(&parts, 1))?;
    file.write_all(&[0xff, 0xfe])?;
    drop(file);
    let mut stderr = Vec::new();

    let failed = assemble(
        &parts,
        2,
        Instant::now() + SIBLING_WAIT,
        session.path(),
        "message",
        now(),
        &mut stderr,
    );

    assert!(failed.is_err(), "an unreadable batch is not a missing one");
    Ok(())
}

// -- the two waits --------------------------------------------------------

/// Both values are spelled out here because both are load-bearing and neither
/// is derivable: the wait is two orders of magnitude past the time a sibling
/// takes to land, and the poll is short enough that the wait is not spent
/// sleeping through an arrival.
#[test]
fn the_wait_and_the_poll_are_the_values_the_host_behaviour_calls_for() {
    assert_eq!(SIBLING_WAIT, Duration::from_secs(2));
    assert_eq!(SIBLING_POLL, Duration::from_millis(20));
    assert!(SIBLING_POLL * 10 < SIBLING_WAIT);
}
