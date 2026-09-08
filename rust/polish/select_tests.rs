use super::{
    Engine, EngineLimits, EngineState, PROBE_MARKER, PROBE_SPEC, cache, host, installed, order,
    polish, probe, select,
};
use crate::polish::engines::{EngineError, EngineRequest};
use crate::polish::process::CancellationToken;
use crate::testing::NEVER_ELAPSES;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        // Not this test's subject; see `NEVER_ELAPSES`.
        timeout: NEVER_ELAPSES,
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

/// One arm of the mapping [`super::attributed`] makes from an invocation's
/// failure to the state `probe` reports.
#[cfg(unix)]
struct Arm {
    /// What the arm is called, so a failure names itself.
    label: &'static str,
    /// Which preset to run, because the answer channel differs by engine.
    engine: Engine,
    /// The stub's shell body, which produces the failure.
    body: String,
    /// The bounds this arm needs to reach its failure.
    limits: EngineLimits,
    /// The state `probe` must report.
    expected: EngineState,
}

/// Every failure the reference implementation attributes to the engine has to
/// survive the trip through `probe` as its own state.
///
/// Without this the mapping has no assertion of its own: the C1 tests prove
/// these errors happen, and `probe`'s other tests would stay green if a
/// timeout started reporting `Failed` or an unreadable answer started
/// propagating as an error instead of a diagnosis.
///
/// Every arm carries a credential trace, so an arm that expects `Failed` is
/// asserting the mapping and not the no-credentials refinement that would
/// otherwise rewrite it.
#[cfg(unix)]
#[test]
fn every_engine_attributable_failure_becomes_its_reference_state() -> TestResult {
    let root = TempDir::new("attributed")?;
    let home = root.path().join("home");
    fs::create_dir(&home)?;
    let credentials = [
        ("ANTHROPIC_API_KEY", SYNTHETIC_VALUE),
        ("OPENAI_API_KEY", SYNTHETIC_VALUE),
    ];
    // Codex is the only preset that reads its answer from a file, so the two
    // file-answer failures are its arms and the rest are stdout's.
    let codex_answer = concat!(
        "out=; while [ $# -gt 0 ]; do ",
        "if [ \"$1\" = '--output-last-message' ]; then out=$2; fi; shift; done; "
    );

    let arms = [
        Arm {
            label: "timeout",
            engine: Engine::Claude,
            // A shell loop rather than `sleep`: `PATH` here is the stub
            // directory alone, so that no arm can reach a real CLI.
            body: "while :; do :; done".to_owned(),
            limits: EngineLimits {
                timeout: Duration::from_millis(200),
                ..limits()
            },
            expected: EngineState::Unreachable,
        },
        Arm {
            label: "output limit",
            engine: Engine::Claude,
            body: format!("printf '%s' '{}'", "a".repeat(200)),
            limits: EngineLimits {
                stdout: 64,
                ..limits()
            },
            expected: EngineState::Failed,
        },
        Arm {
            label: "empty answer",
            engine: Engine::Claude,
            body: "printf '  \\n'".to_owned(),
            limits: limits(),
            expected: EngineState::Failed,
        },
        Arm {
            label: "answer encoding",
            engine: Engine::Claude,
            body: "printf '\\377\\376'".to_owned(),
            limits: limits(),
            expected: EngineState::Failed,
        },
        Arm {
            label: "answer read",
            engine: Engine::Codex,
            // Exits successfully without ever writing the answer file.
            body: "exit 0".to_owned(),
            limits: limits(),
            expected: EngineState::Failed,
        },
        Arm {
            label: "answer limit",
            engine: Engine::Codex,
            body: format!("{codex_answer}printf '%s' '{}' > \"$out\"", "a".repeat(200)),
            limits: EngineLimits {
                answer: 64,
                ..limits()
            },
            expected: EngineState::Failed,
        },
    ];

    for (index, arm) in arms.iter().enumerate() {
        let bin = root.path().join(format!("bin{index}"));
        let binary = arm.engine.preset().ok_or("arm is not a preset")?.binary;
        stub(&bin, binary, &arm.body)?;
        let env = environment(&bin, &home, &credentials);
        let state = probe(&arm.engine, &env, arm.limits, &CancellationToken::new())?;
        assert_eq!(state, arm.expected, "arm: {}", arm.label);
    }
    Ok(())
}

/// A fixed clock reading, so that no cache assertion depends on the wall clock.
fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

/// An environment whose cache lives inside this test's own directory.
///
/// Nothing here may read the machine's real cache: the machine running these
/// tests has all three CLIs installed and logged in, and a remembered answer
/// about them would decide these arms instead of the fixtures.
fn cached_environment(
    bin: &Path,
    home: &Path,
    cache: &Path,
    extra: &[(&str, &str)],
) -> Vec<(OsString, OsString)> {
    let mut env = environment(bin, home, extra);
    env.push(("XDG_CACHE_HOME".into(), cache.as_os_str().to_owned()));
    env
}

fn cache_file(env: &[(OsString, OsString)]) -> Result<PathBuf, Box<dyn Error>> {
    Ok(cache::file(env).ok_or("no cache path")?)
}

fn selected(env: &[(OsString, OsString)]) -> Result<&'static Engine, EngineError> {
    select(env, limits(), &CancellationToken::new(), now())
}

/// A stub that answers the probe and records that it was run.
#[cfg(unix)]
fn recording_stub(directory: &Path, name: &str, ran: &Path) -> Result<(), std::io::Error> {
    stub(
        directory,
        name,
        &format!(": > '{}'\nprintf '%s\\n' '{PROBE_MARKER}'", ran.display()),
    )
}

#[cfg(unix)]
#[test]
fn the_first_engine_that_answers_is_chosen_and_remembered() -> TestResult {
    let root = TempDir::new("chosen")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    stub(&bin, "claude", "exit 7")?;
    stub(&bin, "grok", &format!("printf '%s\\n' '{PROBE_MARKER}'"))?;
    let env = cached_environment(&bin, &home, &cache, &[]);

    assert_eq!(selected(&env)?.name(), "grok");

    // Both answers cost a real call, so both are written down; the choice
    // itself is not, because the next run's ordering is its own question.
    let remembered = cache::remembered(&cache_file(&env)?, now());
    let states: Vec<_> = remembered
        .iter()
        .map(|(engine, observed)| (engine.name(), observed.state))
        .collect();
    assert_eq!(
        states,
        [
            ("claude", EngineState::NoCredentials),
            ("grok", EngineState::Ok),
        ]
    );
    Ok(())
}

/// A caller's deadline says how long the user waits for a rewrite, and a probe
/// is not one: the reference implementation's `engines.select` takes no timeout
/// at all and probes under `PROBE_TIMEOUT`. The probe here outlasts the
/// caller's own deadline many times over and the search still finds the engine,
/// which is the arm that says the two deadlines are apart. Handing the caller's
/// deadline down instead makes this arm report that no engine is usable.
#[cfg(unix)]
#[test]
fn the_search_probes_under_the_probes_own_deadline() -> TestResult {
    let root = TempDir::new("probe-deadline")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    // The engine's own `PATH` is the stub directory alone, where `sleep` does
    // not live, so the stub puts this run's real `PATH` back at the top of
    // itself. Without that the tool is simply missing, the stub answers at once
    // and the arm passes under either deadline — which is no arm at all.
    let tools = std::env::var("PATH").unwrap_or_default();
    stub(
        &bin,
        "claude",
        &format!("PATH='{tools}'\nexport PATH\nsleep 1\nprintf '%s\\n' '{PROBE_MARKER}'"),
    )?;
    let env = cached_environment(&bin, &home, &cache, &[]);

    let brief = EngineLimits {
        timeout: Duration::from_millis(100),
        ..limits()
    };
    let engine = select(&env, brief, &CancellationToken::new(), now())?;

    assert_eq!(engine.name(), "claude");
    Ok(())
}

/// The cache stands in for the probe, which is the whole point of it: an engine
/// with a remembered answer is not run again inside its TTL.
#[cfg(unix)]
#[test]
fn a_remembered_answer_stands_in_for_the_probe() -> TestResult {
    let root = TempDir::new("stand-in")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    let ran = root.path().join("claude-ran");
    recording_stub(&bin, "claude", &ran)?;
    let env = cached_environment(&bin, &home, &cache, &[]);
    let path = cache_file(&env)?;

    // The stub would answer, so anything but the remembered failure selects it.
    cache::remember(&path, &Engine::Claude, EngineState::Unauthorized, now());
    let error = selected(&env).err().ok_or("an engine was selected")?;
    assert!(matches!(error, EngineError::NoUsableEngine { .. }));
    assert!(!ran.exists(), "the CLI was run despite a remembered answer");

    // Once the answer is past its TTL the engine is asked again.
    cache::remember(
        &path,
        &Engine::Claude,
        EngineState::Unauthorized,
        now() - cache::FAILURE_CACHE_TTL - Duration::from_secs(1),
    );
    assert_eq!(selected(&env)?.name(), "claude");
    assert!(ran.exists());
    Ok(())
}

/// An answer cached outside a session cannot outrank the session the user is in
/// now: only the state is remembered, never which engine was chosen.
#[cfg(unix)]
#[test]
fn the_choice_is_recomputed_even_when_every_answer_is_remembered() -> TestResult {
    let root = TempDir::new("recomputed")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    let ran = root.path().join("ran");
    for name in ["claude", "codex", "grok"] {
        recording_stub(&bin, name, &ran)?;
    }
    let plain = cached_environment(&bin, &home, &cache, &[]);
    let path = cache_file(&plain)?;
    for engine in [&Engine::Claude, &Engine::Codex, &Engine::Grok] {
        cache::remember(&path, engine, EngineState::Ok, now());
    }

    assert_eq!(selected(&plain)?.name(), "claude");
    let inside_grok =
        cached_environment(&bin, &home, &cache, &[("GROK_SESSION_ID", SYNTHETIC_VALUE)]);
    assert_eq!(selected(&inside_grok)?.name(), "grok");
    assert!(
        !ran.exists(),
        "an engine was probed despite a remembered answer"
    );
    Ok(())
}

/// A remembered answer about a CLI that is no longer installed is not an answer
/// about anything, and it must not put an age on that engine's diagnosis.
#[cfg(unix)]
#[test]
fn a_remembered_answer_about_an_uninstalled_cli_is_ignored() -> TestResult {
    let root = TempDir::new("uninstalled")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir_all(&bin)?;
    fs::create_dir(&home)?;
    let env = cached_environment(&bin, &home, &cache, &[]);
    cache::remember(&cache_file(&env)?, &Engine::Claude, EngineState::Ok, now());

    let error = selected(&env).err().ok_or("an engine was selected")?;
    let report = error.to_string();
    assert!(report.contains("claude: not installed"), "{report}");
    assert!(!report.contains("last checked"), "{report}");
    Ok(())
}

/// The two diagnoses have to differ: a remembered one says how old it is and
/// how to retry now, because the user who needs that most is the one who has
/// just logged in.
#[cfg(unix)]
#[test]
fn a_remembered_diagnosis_says_so_and_a_fresh_one_does_not() -> TestResult {
    let root = TempDir::new("diagnosis")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    stub(
        &bin,
        "claude",
        &format!("printf '%s\\n' 'the reactor rejected widget {SYNTHETIC_VALUE}'; exit 7"),
    )?;
    let env = cached_environment(&bin, &home, &cache, &[]);
    let path = cache_file(&env)?;

    let fresh = selected(&env)
        .err()
        .ok_or("an engine was selected")?
        .to_string();
    assert!(
        fresh.starts_with("no polish engine is usable:\n"),
        "{fresh}"
    );
    assert!(
        fresh.contains(
            "  claude: no credentials found — log in to that CLI once, or configure engine = 'custom'\n"
        ),
        "{fresh}"
    );
    assert!(
        fresh.contains(
            "  codex: not installed — install the CLI, or name another engine with --engine\n"
        ),
        "{fresh}"
    );
    assert!(fresh.contains("  grok: not installed"), "{fresh}");
    assert!(
        fresh.ends_with("or configure [polish] engine = 'custom' with your own command"),
        "{fresh}"
    );
    assert!(!fresh.contains("last checked"), "{fresh}");

    // That first run wrote the answer down, so the second one is remembered.
    cache::remember(
        &path,
        &Engine::Claude,
        EngineState::NoCredentials,
        now() - Duration::from_secs(120),
    );
    let stale = selected(&env)
        .err()
        .ok_or("an engine was selected")?
        .to_string();
    assert!(
        stale.contains("  claude: no credentials found (last checked 2 minute(s) ago) —"),
        "{stale}"
    );
    assert!(
        stale.ends_with(&format!(
            "some of these are remembered answers, not fresh ones; to retry right now, name the engine with --engine, or delete {}",
            path.display()
        )),
        "{stale}"
    );
    // The uninstalled engines are diagnosed the same way in both.
    assert!(stale.contains("  grok: not installed"), "{stale}");
    Ok(())
}

/// The write-back of a failed real call is what lets a remembered `ok` last an
/// hour: without it the permit would stand until its TTL ran out.
#[cfg(unix)]
#[test]
fn a_real_failure_is_written_over_a_remembered_answer() -> TestResult {
    let root = TempDir::new("writeback")?;
    let bin = root.path().join("bin");
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    fs::create_dir(&home)?;
    stub(&bin, "claude", "printf '%s\\n' 'HTTP 401'; exit 3")?;
    stub(&bin, "failing", "exit 3")?;
    let env = cached_environment(&bin, &home, &cache, &[]);
    let path = cache_file(&env)?;
    cache::remember(&path, &Engine::Claude, EngineState::Ok, now());

    let request = EngineRequest {
        engine: &Engine::Claude,
        model: "",
        spec: "spec",
        text: "text",
        cwd: root.path(),
        env: &env,
    };
    let error = polish(&request, limits(), &CancellationToken::new(), now())
        .err()
        .ok_or("the stub answered")?;
    assert!(matches!(error, EngineError::Exit { .. }));

    let remembered = cache::remembered(&path, now());
    let (engine, observed) = remembered.first().ok_or("nothing was remembered")?;
    assert_eq!(
        (engine.name(), observed.state),
        ("claude", EngineState::Unauthorized)
    );
    assert_eq!(remembered.len(), 1);

    // A custom command is nobody's cached engine, and a missing binary is free
    // to re-check, so neither is written down.
    for engine in [
        Engine::Custom(vec![bin.join("failing").display().to_string()]),
        Engine::Grok,
    ] {
        let request = EngineRequest {
            engine: &engine,
            model: "",
            spec: "spec",
            text: "text",
            cwd: root.path(),
            env: &env,
        };
        let _ = polish(&request, limits(), &CancellationToken::new(), now())
            .err()
            .ok_or("the stub answered")?;
        assert_eq!(
            cache::remembered(&path, now()).len(),
            1,
            "{}",
            engine.name()
        );
    }
    Ok(())
}
