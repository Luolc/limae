use super::{
    Engine, EngineLimits, EngineState, PROBE_MARKER, PROBE_SPEC, host, installed, order, probe,
};
use crate::polish::process::CancellationToken;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const SYNTHETIC_VALUE: &str = "synthetic-placeholder-value";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-select-{name}-{}-{}",
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

/// Build a complete environment for one arm.
///
/// Every arm states its own `PATH` and `HOME`: the machine running these tests
/// may well have all three CLIs installed and logged in, and an ordering read
/// off that machine would be green here and red on the next one.
fn environment(bin: &Path, home: &Path, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    let mut env = vec![
        ("PATH".into(), bin.as_os_str().to_owned()),
        ("HOME".into(), home.as_os_str().to_owned()),
        ("LANG".into(), "C.UTF-8".into()),
    ];
    env.extend(
        extra
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into())),
    );
    env
}

#[cfg(unix)]
fn stub(directory: &Path, name: &str, body: &str) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(directory)?;
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
}

fn names(engines: &[&'static Engine]) -> Vec<&'static str> {
    engines.iter().map(|engine| engine.name()).collect()
}

fn limits() -> EngineLimits {
    EngineLimits {
        timeout: Duration::from_secs(2),
        terminate_grace: Duration::from_millis(10),
        stdout: 64 * 1024,
        stderr: 64 * 1024,
        answer: 64 * 1024,
    }
}

fn probed(engine: &Engine, env: &[(OsString, OsString)]) -> Result<EngineState, Box<dyn Error>> {
    Ok(probe(engine, env, limits(), &CancellationToken::new())?)
}

#[test]
fn the_probe_spec_asks_for_the_marker_it_accepts() {
    assert!(PROBE_SPEC.ends_with(PROBE_MARKER));
}

#[cfg(unix)]
#[test]
fn order_puts_the_host_first_the_credentialed_next_and_keeps_the_preset_order() -> TestResult {
    let root = TempDir::new("order")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    fs::create_dir(&home)?;
    for name in ["claude", "codex", "grok"] {
        stub(&bin, name, "exit 0")?;
    }

    let plain = environment(&bin, &home, &[]);
    assert_eq!(names(&order(&plain)), ["claude", "codex", "grok"]);
    assert!(host(&plain).is_none());

    let inside_codex = environment(&bin, &home, &[("CODEX_SESSION_ID", SYNTHETIC_VALUE)]);
    assert_eq!(names(&order(&inside_codex)), ["codex", "claude", "grok"]);
    assert_eq!(host(&inside_codex).map(Engine::name), Some("codex"));

    let grok_key = environment(&bin, &home, &[("GROK_CODE_XAI_API_KEY", SYNTHETIC_VALUE)]);
    assert_eq!(names(&order(&grok_key)), ["grok", "claude", "codex"]);

    // The host outranks a credential trace, which is only visible when the
    // credentialed engine comes first in `ENGINES` and the host does not.
    let inside_grok = environment(
        &bin,
        &home,
        &[
            ("GROK_SESSION_ID", SYNTHETIC_VALUE),
            ("ANTHROPIC_API_KEY", SYNTHETIC_VALUE),
        ],
    );
    assert_eq!(names(&order(&inside_grok)), ["grok", "claude", "codex"]);

    let both = environment(
        &bin,
        &home,
        &[
            ("CODEX_SESSION_ID", SYNTHETIC_VALUE),
            ("GROK_CODE_XAI_API_KEY", SYNTHETIC_VALUE),
        ],
    );
    assert_eq!(names(&order(&both)), ["codex", "grok", "claude"]);

    // An empty variable is not a marker, the way an unset one is not.
    let empty = environment(&bin, &home, &[("CODEX_SESSION_ID", "")]);
    assert_eq!(names(&order(&empty)), ["claude", "codex", "grok"]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_missing_binary_is_the_only_hard_negative() -> TestResult {
    let root = TempDir::new("negative")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    fs::create_dir(&home)?;
    stub(&bin, "claude", "exit 0")?;
    stub(&bin, "grok", "exit 0")?;

    let plain = environment(&bin, &home, &[]);
    assert_eq!(names(&order(&plain)), ["claude", "grok"]);
    assert!(!installed(&Engine::Codex, &plain));

    // Codex is the session we are inside and the only engine with a key, which
    // is what would put it first if the binary were a sorting key too.
    let favoured = environment(
        &bin,
        &home,
        &[
            ("CODEX_SESSION_ID", SYNTHETIC_VALUE),
            ("OPENAI_API_KEY", SYNTHETIC_VALUE),
        ],
    );
    assert_eq!(host(&favoured).map(Engine::name), Some("codex"));
    assert_eq!(names(&order(&favoured)), ["claude", "grok"]);

    // A custom command is nobody's binary to look for.
    assert!(!installed(
        &Engine::Custom(vec!["claude".to_owned()]),
        &plain
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_login_file_counts_by_its_key_when_the_preset_names_one() -> TestResult {
    let root = TempDir::new("login")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    fs::create_dir_all(home.join(".codex"))?;
    for name in ["claude", "codex", "grok"] {
        stub(&bin, name, "exit 0")?;
    }
    // Codex names no key, so the file's existence is the whole hint — content
    // that is not even JSON still counts.
    fs::write(home.join(".codex/auth.json"), "not json at all")?;
    let env = environment(&bin, &home, &[]);

    let claude_file = home.join(".claude.json");
    fs::write(&claude_file, r#"{"projects": {}}"#)?;
    assert_eq!(names(&order(&env)), ["codex", "claude", "grok"]);

    fs::write(
        &claude_file,
        r#"{"oauthAccount": {"emailAddress": "a@b.test"}}"#,
    )?;
    assert_eq!(names(&order(&env)), ["claude", "codex", "grok"]);

    fs::write(&claude_file, "{not json")?;
    assert_eq!(names(&order(&env)), ["codex", "claude", "grok"]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn only_an_answer_carrying_the_marker_passes_the_probe() -> TestResult {
    let root = TempDir::new("probe")?;
    let home = root.path().join("home");
    fs::create_dir(&home)?;

    let alive = root.path().join("alive");
    stub(
        &alive,
        "claude",
        &format!("printf '%s\\n' '{PROBE_MARKER}'"),
    )?;
    assert_eq!(
        probed(&Engine::Claude, &environment(&alive, &home, &[]))?,
        EngineState::Ok
    );

    // Same exit code, different answer: an engine may exit 0 and still print an
    // authentication error, so the exit code alone decides nothing.
    let refusing = root.path().join("refusing");
    stub(
        &refusing,
        "claude",
        &format!("printf '%s\\n' 'HTTP 401 Unauthorized for key {SYNTHETIC_VALUE}'"),
    )?;
    assert_eq!(
        probed(&Engine::Claude, &environment(&refusing, &home, &[]))?,
        EngineState::Unauthorized
    );

    let empty = root.path().join("empty");
    fs::create_dir(&empty)?;
    assert_eq!(
        probed(&Engine::Claude, &environment(&empty, &home, &[]))?,
        EngineState::Missing
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_failure_without_a_credential_trace_is_reported_as_no_credentials() -> TestResult {
    let root = TempDir::new("nocreds")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    fs::create_dir(&home)?;
    stub(
        &bin,
        "claude",
        &format!("printf '%s\\n' 'the reactor rejected widget {SYNTHETIC_VALUE}'; exit 7"),
    )?;

    assert_eq!(
        probed(&Engine::Claude, &environment(&bin, &home, &[]))?,
        EngineState::NoCredentials
    );

    // The same failure with a credential trace stays `Failed`: the refinement
    // is about the trace, not about the failure.
    let with_key = environment(&bin, &home, &[("ANTHROPIC_API_KEY", SYNTHETIC_VALUE)]);
    assert_eq!(probed(&Engine::Claude, &with_key)?, EngineState::Failed);
    Ok(())
}
