use super::{CACHE_TTL, EngineState, FAILURE_CACHE_TTL, Observed, ago, file, remember, remembered};
use crate::polish::engines::Engine;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn Error>>;

/// A fixed clock reading, so that no assertion here depends on the wall clock.
const NOW: Duration = Duration::from_secs(1_700_000_000);

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-cache-{name}-{}-{}",
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

fn now() -> SystemTime {
    UNIX_EPOCH + NOW
}

/// The epoch timestamp an answer found `age` ago carries.
fn at(age: Duration) -> f64 {
    (NOW - age).as_secs_f64()
}

fn document(entries: &str) -> String {
    format!(r#"{{"engines": {entries}}}"#)
}

fn states(remembered: &[(&'static Engine, Observed)]) -> Vec<(&'static str, EngineState)> {
    remembered
        .iter()
        .map(|(engine, observed)| (engine.name(), observed.state))
        .collect()
}

/// The engine names the cache file itself holds, in the file's own order.
fn engine_keys(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let engines = written
        .get("engines")
        .and_then(serde_json::Value::as_object)
        .ok_or("no engines object")?;
    Ok(engines.keys().cloned().collect())
}

/// A good entry has to be accepted, or every malformed arm below is also green
/// under an implementation that simply drops everything.
#[test]
fn a_well_formed_entry_is_read_back_with_its_age() -> TestResult {
    let root = TempDir::new("wellformed")?;
    let path = root.path().join("engine.json");
    fs::write(
        &path,
        document(&format!(
            r#"{{"claude": {{"state": "ok", "at": {}}}}}"#,
            at(Duration::from_secs(120))
        )),
    )?;

    let fresh = remembered(&path, now());
    assert_eq!(states(&fresh), [("claude", EngineState::Ok)]);
    assert_eq!(
        fresh.first().ok_or("no entry")?.1.age,
        Duration::from_secs(120)
    );
    Ok(())
}

/// Every malformed shape the reference implementation drops, one arm each, and
/// a good entry alongside each one so that the arm proves a per-entry drop and
/// not a wholesale one where the file itself is still readable.
#[test]
fn each_malformed_entry_is_dropped() -> TestResult {
    let root = TempDir::new("malformed")?;
    let path = root.path().join("engine.json");
    let good = format!(
        r#""claude": {{"state": "ok", "at": {}}}"#,
        at(Duration::ZERO)
    );
    // Every malformed entry carries a fresh timestamp, so that only its shape
    // can be what drops it: an entry stale enough to fall out of its TTL would
    // leave the shape check with nothing to prove.
    let fresh = at(Duration::ZERO);
    let arms = [
        (
            "unknown engine name",
            document(&format!(
                r#"{{{good}, "banana": {{"state": "ok", "at": {fresh}}}}}"#
            )),
        ),
        (
            "entry is not an object",
            document(&format!(r#"{{{good}, "codex": 7}}"#)),
        ),
        (
            "state is not a string",
            document(&format!(
                r#"{{{good}, "codex": {{"state": 1, "at": {fresh}}}}}"#
            )),
        ),
        (
            "state is unknown",
            document(&format!(
                r#"{{{good}, "codex": {{"state": "banana", "at": {fresh}}}}}"#
            )),
        ),
        (
            "state is missing",
            document(&format!(r#"{{{good}, "codex": {{"at": {fresh}}}}}"#)),
        ),
        (
            "at is not a number",
            document(&format!(
                r#"{{{good}, "codex": {{"state": "ok", "at": "now"}}}}"#
            )),
        ),
        (
            "at is missing",
            document(&format!(r#"{{{good}, "codex": {{"state": "ok"}}}}"#)),
        ),
    ];
    for (label, body) in &arms {
        fs::write(&path, body)?;
        assert_eq!(
            states(&remembered(&path, now())),
            [("claude", EngineState::Ok)],
            "arm: {label}"
        );
    }

    // Nothing readable at all, one arm each: these drop the whole file, which is
    // why they cannot carry the good entry with them.
    for (label, body) in [
        ("file is not JSON", "{not json".to_owned()),
        ("document is not an object", "[1, 2]".to_owned()),
        ("engines is not an object", r#"{"engines": 4}"#.to_owned()),
        ("engines is absent", r#"{"other": {}}"#.to_owned()),
    ] {
        fs::write(&path, &body)?;
        assert!(remembered(&path, now()).is_empty(), "arm: {label}");
    }

    // An absent file is not an error either.
    assert!(remembered(&root.path().join("absent.json"), now()).is_empty());
    Ok(())
}

/// The two TTLs are one design, not one value used twice: the middle two arms
/// are the ones that go red if success and failure ever share a TTL.
#[test]
fn a_success_and_a_failure_each_keep_their_own_ttl() -> TestResult {
    let root = TempDir::new("ttl")?;
    let path = root.path().join("engine.json");
    let second = Duration::from_secs(1);
    let between = FAILURE_CACHE_TTL + Duration::from_secs(60);
    assert!(between < CACHE_TTL);

    let arms = [
        ("ok at its TTL", "ok", CACHE_TTL, true),
        ("ok past its TTL", "ok", CACHE_TTL + second, false),
        ("ok past the failure TTL", "ok", between, true),
        (
            "failure at its TTL",
            "ran but failed",
            FAILURE_CACHE_TTL,
            true,
        ),
        (
            "failure past its TTL",
            "ran but failed",
            FAILURE_CACHE_TTL + second,
            false,
        ),
        (
            "failure past the failure TTL",
            "ran but failed",
            between,
            false,
        ),
    ];
    for (label, state, age, kept) in arms {
        fs::write(
            &path,
            document(&format!(
                r#"{{"claude": {{"state": "{state}", "at": {}}}}}"#,
                at(age)
            )),
        )?;
        assert_eq!(!remembered(&path, now()).is_empty(), kept, "arm: {label}");
    }

    // An answer from the future is not from a clock this run can reason about.
    fs::write(
        &path,
        document(&format!(
            r#"{{"claude": {{"state": "ok", "at": {}}}}}"#,
            (NOW + second).as_secs_f64()
        )),
    )?;
    assert!(remembered(&path, now()).is_empty());
    Ok(())
}

#[test]
fn remembering_one_engine_replaces_its_entry_and_keeps_the_others() -> TestResult {
    let root = TempDir::new("remember")?;
    let path = root.path().join("nested").join("engine.json");
    remember(&path, &Engine::Codex, EngineState::Ok, now());
    remember(&path, &Engine::Claude, EngineState::Unauthorized, now());
    remember(&path, &Engine::Claude, EngineState::Unreachable, now());

    // `remembered` reports in `ENGINES` order, whatever order they were written.
    assert_eq!(
        states(&remembered(&path, now())),
        [
            ("claude", EngineState::Unreachable),
            ("codex", EngineState::Ok),
        ]
    );

    // Nothing but a name, a state and a timestamp is ever written.
    assert_eq!(engine_keys(&path)?, ["claude", "codex"]);
    let written: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
    let entry = written
        .get("engines")
        .and_then(|engines| engines.get("codex"))
        .and_then(serde_json::Value::as_object)
        .ok_or("no codex entry")?;
    assert_eq!(entry.keys().collect::<Vec<_>>(), ["at", "state"]);

    // A custom command is nobody's cached engine. This has to be read off the
    // file itself: `remembered` walks the presets, so it would report the same
    // two entries whether or not a `custom` key had been written beside them.
    remember(
        &path,
        &Engine::Custom(vec!["true".to_owned()]),
        EngineState::Failed,
        now(),
    );
    assert_eq!(engine_keys(&path)?, ["claude", "codex"]);
    Ok(())
}

/// A cache that cannot be written changes nothing but the cost of the next run.
#[test]
fn a_cache_that_cannot_be_written_is_silent() -> TestResult {
    let root = TempDir::new("unwritable")?;
    let blocking = root.path().join("blocking");
    fs::write(&blocking, "not a directory")?;
    let path = blocking.join("engine.json");

    remember(&path, &Engine::Claude, EngineState::Ok, now());
    assert!(remembered(&path, now()).is_empty());
    Ok(())
}

#[test]
fn an_age_is_reported_in_whole_minutes() {
    assert_eq!(ago(Duration::ZERO), "last checked under a minute ago");
    assert_eq!(
        ago(Duration::from_secs(59)),
        "last checked under a minute ago"
    );
    assert_eq!(ago(Duration::from_secs(60)), "last checked 1 minute(s) ago");
    assert_eq!(
        ago(Duration::from_secs(7260)),
        "last checked 121 minute(s) ago"
    );
}

#[test]
fn the_cache_file_follows_xdg_then_home() {
    let env = |pairs: &[(&str, &str)]| -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect()
    };

    assert_eq!(
        file(&env(&[("HOME", "/synthetic/home")])),
        Some(PathBuf::from("/synthetic/home/.cache/limae/engine.json"))
    );
    assert_eq!(
        file(&env(&[
            ("HOME", "/synthetic/home"),
            ("XDG_CACHE_HOME", "/synthetic/cache"),
        ])),
        Some(PathBuf::from("/synthetic/cache/limae/engine.json"))
    );
    // Empty is unset, the way it is everywhere else in this module.
    assert_eq!(
        file(&env(&[("HOME", "/synthetic/home"), ("XDG_CACHE_HOME", "")])),
        Some(PathBuf::from("/synthetic/home/.cache/limae/engine.json"))
    );
    // No home and no override: this run remembers nothing rather than reading
    // whoever happens to be logged in on this machine.
    assert_eq!(file(&env(&[("PATH", "/synthetic/bin")])), None);
}
