use super::{
    AnswerSource, Engine, EngineError, EngineLimits, EngineRequest, Invocation, expand, polish,
};
use crate::polish::process::{CancellationToken, ProcessError, Stream};
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

type TestResult = Result<(), Box<dyn Error>>;

const SPEC: &str = "Polish spec: keep the code fences.\n";
const TEXT: &str = "ACME 的报告写得不好。\n";
const SYNTHETIC_VALUE: &str = "synthetic-placeholder-value";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-engines-{name}-{}-{}",
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

fn limits() -> EngineLimits {
    EngineLimits {
        timeout: Duration::from_secs(2),
        terminate_grace: Duration::from_millis(10),
        stdout: 128 * 1024,
        stderr: 128 * 1024,
        answer: 128 * 1024,
    }
}

fn environment(bin: &Path) -> Vec<(OsString, OsString)> {
    vec![
        (
            "PATH".into(),
            format!("{}:/usr/bin:/bin", bin.display()).into(),
        ),
        ("HOME".into(), "/home/synthetic".into()),
        ("LANG".into(), "C.UTF-8".into()),
    ]
}

#[cfg(unix)]
fn stub(directory: &Path, name: &str, body: &str) -> Result<PathBuf, std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(directory)?;
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

fn request<'a>(
    engine: &'a Engine,
    model: &'a str,
    cwd: &'a Path,
    env: &'a [(OsString, OsString)],
) -> EngineRequest<'a> {
    EngineRequest {
        engine,
        model,
        spec: SPEC,
        text: TEXT,
        cwd,
        env,
    }
}

fn argv(invocation: &Invocation) -> Vec<&str> {
    invocation
        .argv
        .iter()
        .map(|word| word.to_str().unwrap_or("<non-utf8>"))
        .collect()
}

fn marker(text: &str) -> Result<&str, &'static str> {
    let prefix = "----- The text to rewrite follows this line, marker ";
    let start = text.find(prefix).ok_or("missing marker")? + prefix.len();
    let rest = &text[start..];
    let end = rest.find('.').ok_or("unterminated marker")?;
    Ok(&rest[..end])
}

#[test]
fn templates_expand_with_the_reference_channels_and_random_boundary() -> TestResult {
    let root = TempDir::new("expand")?;
    let env = environment(root.path());

    let claude_dir = root.path().join("claude");
    fs::create_dir(&claude_dir)?;
    let claude_engine = Engine::Claude;
    let claude = expand(&request(&claude_engine, "", root.path(), &env), &claude_dir)?;
    let claude_spec = fs::read_to_string(claude_dir.join("spec.md"))?;
    let claude_stdin = String::from_utf8(claude.stdin.clone())?;
    let nonce = marker(&claude_spec)?;
    assert_eq!(nonce.len(), 16);
    assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(marker(&claude_stdin)?, nonce);
    assert_eq!(
        argv(&claude),
        [
            "claude",
            "-p",
            "--system-prompt-file",
            claude_dir.join("spec.md").to_str().ok_or("non-utf8 path")?,
            "--model",
            "sonnet",
        ]
    );
    assert!(matches!(claude.answer, AnswerSource::Stdout));
    assert_eq!(claude.cwd, claude_dir);

    let codex_dir = root.path().join("codex");
    fs::create_dir(&codex_dir)?;
    let codex_engine = Engine::Codex;
    let codex = expand(
        &request(&codex_engine, "gpt-synthetic", root.path(), &env),
        &codex_dir,
    )?;
    let codex_stdin = String::from_utf8(codex.stdin.clone())?;
    let nonce = marker(&codex_stdin)?;
    assert_eq!(codex_stdin.matches(nonce).count(), 2);
    assert_eq!(
        argv(&codex),
        [
            "codex",
            "exec",
            "--skip-git-repo-check",
            "--ephemeral",
            "-c",
            "model=gpt-synthetic",
            "-c",
            "model_reasoning_effort=low",
            "--output-last-message",
            codex_dir
                .join("output.txt")
                .to_str()
                .ok_or("non-utf8 path")?,
            "-",
        ]
    );
    assert!(matches!(codex.answer, AnswerSource::File(_)));

    let grok_dir = root.path().join("grok");
    fs::create_dir(&grok_dir)?;
    let grok_engine = Engine::Grok;
    let grok = expand(&request(&grok_engine, "", root.path(), &env), &grok_dir)?;
    assert_eq!(
        argv(&grok),
        [
            "grok",
            "--system-prompt-override",
            SPEC,
            "-m",
            "grok-4.6",
            "--verbatim",
            "-p",
            TEXT,
        ]
    );
    assert!(grok.stdin.is_empty());
    assert!(matches!(grok.answer, AnswerSource::Stdout));

    let second_dir = root.path().join("second");
    fs::create_dir(&second_dir)?;
    let second = expand(&request(&claude_engine, "", root.path(), &env), &second_dir)?;
    let second_stdin = String::from_utf8(second.stdin)?;
    assert_ne!(marker(&claude_stdin)?, marker(&second_stdin)?);
    Ok(())
}

#[test]
fn custom_expansion_substitutes_both_placeholders_or_uses_stdin() -> TestResult {
    let root = TempDir::new("custom-expand")?;
    let workdir = root.path().join("work");
    fs::create_dir(&workdir)?;
    let env = environment(root.path());
    let with_text = Engine::Custom(vec![
        "gateway".into(),
        "--spec={spec_file}".into(),
        "--text={text}".into(),
    ]);
    let invocation = expand(&request(&with_text, "", root.path(), &env), &workdir)?;
    assert_eq!(
        argv(&invocation),
        [
            "gateway",
            &format!("--spec={}", workdir.join("spec.md").display()),
            &format!("--text={TEXT}"),
        ]
    );
    assert!(invocation.stdin.is_empty());
    assert_eq!(invocation.cwd, root.path());
    assert_eq!(fs::read_to_string(workdir.join("spec.md"))?, SPEC);

    let on_stdin = Engine::Custom(vec!["gateway".into(), "{spec_file}".into()]);
    let invocation = expand(&request(&on_stdin, "", root.path(), &env), &workdir)?;
    assert_eq!(invocation.stdin, TEXT.as_bytes());
    Ok(())
}

#[cfg(unix)]
#[test]
fn claude_stub_consumes_argv_stdin_cwd_environment_and_spec() -> TestResult {
    let root = TempDir::new("claude-run")?;
    let bin = root.path().join("bin");
    let observed = root.path().join("observed");
    stub(
        &bin,
        "claude",
        &format!(
            "{{ printf '%s\\n' \"$@\"; printf '%s\\n' --stdin--; cat; printf '%s\\n' --cwd--; pwd; printf '%s\\n' --env--; env; printf '%s\\n' --spec--; cat \"$3\"; }} > '{}'; printf '  polished by claude  '",
            observed.display()
        ),
    )?;
    let mut env = environment(&bin);
    env.extend([
        ("LC_TIME".into(), "C".into()),
        ("ANTHROPIC_API_KEY".into(), SYNTHETIC_VALUE.into()),
        ("OPENAI_API_KEY".into(), SYNTHETIC_VALUE.into()),
        ("LIMAE_HOOK_DISABLE".into(), "1".into()),
        ("LIMAE_HOOK_MIN_CHARS".into(), "17".into()),
        ("CLAUDECODE".into(), "1".into()),
        ("GIT_DIR".into(), "/synthetic/repo/.git".into()),
    ]);
    let engine = Engine::Claude;
    let answer = polish(
        &request(&engine, "model-synthetic", root.path(), &env),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(answer, "polished by claude\n");

    let seen = fs::read_to_string(observed)?;
    assert!(seen.contains("-p\n--system-prompt-file\n"));
    assert!(seen.contains("--model\nmodel-synthetic\n"));
    assert!(seen.contains("--stdin--\n"));
    assert!(seen.contains(TEXT));
    assert!(seen.contains(SPEC));
    assert!(seen.contains("PATH="));
    assert!(seen.contains("HOME=/home/synthetic"));
    assert!(seen.contains("LC_TIME=C"));
    assert!(seen.contains("ANTHROPIC_API_KEY="));
    assert!(seen.contains("LIMAE_HOOK_DISABLE=1"));
    assert!(!seen.contains("OPENAI_API_KEY"));
    assert!(!seen.contains("LIMAE_HOOK_MIN_CHARS"));
    assert!(!seen.contains("CLAUDECODE"));
    assert!(!seen.contains("GIT_DIR"));
    let cwd = seen
        .split_once("--cwd--\n")
        .and_then(|(_, rest)| rest.lines().next())
        .ok_or("missing cwd")?;
    assert_ne!(Path::new(cwd), root.path());
    assert!(!Path::new(cwd).exists());
    assert!(seen.contains(&format!("PWD={cwd}")));
    Ok(())
}

#[cfg(unix)]
#[test]
fn codex_stub_supplies_the_file_answer_instead_of_process_stdout() -> TestResult {
    let root = TempDir::new("codex-run")?;
    let bin = root.path().join("bin");
    let observed = root.path().join("observed");
    stub(
        &bin,
        "codex",
        &format!(
            "cat > '{}'; printf '%s\\n' \"$@\" >> '{}'; env >> '{}'; shift 8; printf ' answer from file ' > \"$1\"; printf 'process progress'",
            observed.display(),
            observed.display(),
            observed.display()
        ),
    )?;
    let mut env = environment(&bin);
    env.extend([
        ("OPENAI_API_KEY".into(), SYNTHETIC_VALUE.into()),
        ("ANTHROPIC_API_KEY".into(), SYNTHETIC_VALUE.into()),
    ]);
    let engine = Engine::Codex;
    let answer = polish(
        &request(&engine, "gpt-synthetic", root.path(), &env),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(answer, "answer from file\n");
    let seen = fs::read_to_string(observed)?;
    assert!(seen.starts_with(SPEC));
    assert!(seen.contains("exec\n--skip-git-repo-check\n--ephemeral\n"));
    assert!(seen.contains("model=gpt-synthetic"));
    assert!(seen.contains("model_reasoning_effort=low"));
    assert!(seen.contains("-\n"));
    assert!(seen.contains("OPENAI_API_KEY="));
    assert!(!seen.contains("ANTHROPIC_API_KEY"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn grok_stub_consumes_the_spec_and_text_as_distinct_arguments() -> TestResult {
    let root = TempDir::new("grok-run")?;
    let bin = root.path().join("bin");
    let observed = root.path().join("observed");
    stub(
        &bin,
        "grok",
        &format!(
            "printf '%s\\n' \"$@\" > '{}'; env >> '{}'; printf 'grok answer'",
            observed.display(),
            observed.display()
        ),
    )?;
    let mut env = environment(&bin);
    env.extend([
        ("GROK_CODE_XAI_API_KEY".into(), SYNTHETIC_VALUE.into()),
        ("OPENAI_API_KEY".into(), SYNTHETIC_VALUE.into()),
    ]);
    let engine = Engine::Grok;
    assert_eq!(
        polish(
            &request(&engine, "", root.path(), &env),
            limits(),
            &CancellationToken::new(),
        )?,
        "grok answer\n"
    );
    let seen = fs::read_to_string(observed)?;
    assert!(seen.starts_with(&format!(
        "--system-prompt-override\n{SPEC}\n-m\ngrok-4.6\n--verbatim\n-p\n{TEXT}\n"
    )));
    assert!(seen.contains("GROK_CODE_XAI_API_KEY="));
    assert!(!seen.contains("OPENAI_API_KEY"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn custom_stub_retains_the_callers_cwd_and_complete_environment() -> TestResult {
    let root = TempDir::new("custom-run")?;
    let script = stub(
        &root.path().join("bin"),
        "gateway",
        "printf 'cwd=%s\\nvalue=%s\\nsetting=%s\\n' \"$(pwd)\" \"$ACME_VALUE\" \"$LIMAE_CUSTOM_SETTING\"; cat",
    )?;
    let env = vec![
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("ACME_VALUE".into(), "synthetic".into()),
        ("LIMAE_CUSTOM_SETTING".into(), "present".into()),
    ];
    let engine = Engine::Custom(vec![script.to_string_lossy().into_owned()]);
    let answer = polish(
        &request(&engine, "", root.path(), &env),
        limits(),
        &CancellationToken::new(),
    )?;
    assert_eq!(
        answer,
        format!(
            "cwd={}\nvalue=synthetic\nsetting=present\n{TEXT}",
            root.path().display()
        )
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn stdout_and_file_answers_enforce_independent_inclusive_caps() -> TestResult {
    let root = TempDir::new("answer-limits")?;
    let bin = root.path().join("bin");
    let env = environment(&bin);
    let custom_script = stub(&bin, "custom", "printf 123456")?;
    let custom = Engine::Custom(vec![custom_script.to_string_lossy().into_owned()]);
    let stdout = polish(
        &request(&custom, "", root.path(), &env),
        EngineLimits {
            stdout: 5,
            ..limits()
        },
        &CancellationToken::new(),
    );
    assert!(matches!(
        stdout,
        Err(EngineError::Process {
            source: ProcessError::OutputLimit {
                stream: Stream::Stdout,
                limit: 5
            }
        })
    ));

    stub(
        &bin,
        "codex",
        "cat > /dev/null; shift 8; printf 12345 > \"$1\"",
    )?;
    let codex = Engine::Codex;
    let exact = polish(
        &request(&codex, "", root.path(), &env),
        EngineLimits {
            answer: 5,
            ..limits()
        },
        &CancellationToken::new(),
    )?;
    assert_eq!(exact, "12345\n");

    stub(
        &bin,
        "codex",
        "cat > /dev/null; shift 8; printf 123456 > \"$1\"",
    )?;
    let excessive = polish(
        &request(&codex, "", root.path(), &env),
        EngineLimits {
            answer: 5,
            ..limits()
        },
        &CancellationToken::new(),
    );
    assert!(matches!(
        excessive,
        Err(EngineError::AnswerLimit { limit: 5 })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn failures_and_debug_output_do_not_echo_request_or_child_content() -> TestResult {
    let root = TempDir::new("redaction")?;
    let script = stub(
        &root.path().join("bin"),
        "gateway",
        &format!("printf '{SYNTHETIC_VALUE}' >&2; exit 7"),
    )?;
    let env = vec![(SYNTHETIC_VALUE.into(), SYNTHETIC_VALUE.into())];
    let engine = Engine::Custom(vec![
        script.to_string_lossy().into_owned(),
        SYNTHETIC_VALUE.into(),
    ]);
    let request = EngineRequest {
        engine: &engine,
        model: SYNTHETIC_VALUE,
        spec: SYNTHETIC_VALUE,
        text: SYNTHETIC_VALUE,
        cwd: root.path(),
        env: &env,
    };
    let error = polish(&request, limits(), &CancellationToken::new())
        .err()
        .ok_or("failing command unexpectedly succeeded")?;
    assert!(!error.to_string().contains(SYNTHETIC_VALUE));
    assert!(!format!("{error:?}").contains(SYNTHETIC_VALUE));
    assert!(error.source().is_none());

    let missing = Engine::Custom(vec![
        root.path()
            .join(SYNTHETIC_VALUE)
            .to_string_lossy()
            .into_owned(),
    ]);
    let request = EngineRequest {
        engine: &missing,
        model: SYNTHETIC_VALUE,
        spec: SYNTHETIC_VALUE,
        text: SYNTHETIC_VALUE,
        cwd: root.path(),
        env: &env,
    };
    let error = polish(&request, limits(), &CancellationToken::new())
        .err()
        .ok_or("missing command unexpectedly succeeded")?;
    assert!(!error.to_string().contains(SYNTHETIC_VALUE));
    assert!(!format!("{error:?}").contains(SYNTHETIC_VALUE));
    let mut source = error.source();
    assert!(source.is_some());
    while let Some(current) = source {
        assert!(!current.to_string().contains(SYNTHETIC_VALUE));
        assert!(!format!("{current:?}").contains(SYNTHETIC_VALUE));
        source = current.source();
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn stdout_and_file_answers_use_python_universal_newlines() -> TestResult {
    let root = TempDir::new("newlines")?;
    let bin = root.path().join("bin");
    let env = environment(&bin);
    let custom_script = stub(
        &bin,
        "custom",
        "printf 'line one\\r\\nline two\\rline three\\n'",
    )?;
    let custom = Engine::Custom(vec![custom_script.to_string_lossy().into_owned()]);
    assert_eq!(
        polish(
            &request(&custom, "", root.path(), &env),
            limits(),
            &CancellationToken::new(),
        )?,
        "line one\nline two\nline three\n"
    );

    stub(
        &bin,
        "codex",
        "cat > /dev/null; shift 8; printf 'line one\\r\\nline two\\rline three\\n' > \"$1\"",
    )?;
    let codex = Engine::Codex;
    assert_eq!(
        polish(
            &request(&codex, "", root.path(), &env),
            limits(),
            &CancellationToken::new(),
        )?,
        "line one\nline two\nline three\n"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn empty_answer_is_rejected_and_temporary_resources_are_cleaned() -> TestResult {
    let root = TempDir::new("empty-answer")?;
    let observed = root.path().join("spec-path");
    let script = stub(
        &root.path().join("bin"),
        "gateway",
        &format!("printf '%s' \"$1\" > '{}'", observed.display()),
    )?;
    let env = environment(root.path());
    let engine = Engine::Custom(vec![
        script.to_string_lossy().into_owned(),
        "{spec_file}".into(),
        "{text}".into(),
    ]);
    let result = polish(
        &request(&engine, "", root.path(), &env),
        limits(),
        &CancellationToken::new(),
    );
    assert!(matches!(result, Err(EngineError::EmptyAnswer)));
    let spec = PathBuf::from(fs::read_to_string(observed)?);
    assert!(!spec.exists());
    assert!(!spec.parent().ok_or("missing spec parent")?.exists());
    Ok(())
}

#[test]
fn empty_custom_command_is_rejected_without_spawning() -> TestResult {
    let root = TempDir::new("empty-custom")?;
    let env = environment(root.path());
    let engine = Engine::Custom(Vec::new());
    let result = polish(
        &request(&engine, "", root.path(), &env),
        limits(),
        &CancellationToken::new(),
    );
    assert!(matches!(result, Err(EngineError::EmptyCommand)));
    Ok(())
}

#[test]
fn answer_normalization_uses_the_python_whitespace_contract() -> TestResult {
    let root = TempDir::new("normalization")?;
    let workdir = root.path().join("work");
    fs::create_dir(&workdir)?;
    let engine = Engine::Grok;
    let env = environment(root.path());
    let invocation = expand(&request(&engine, "", root.path(), &env), &workdir)?;
    assert!(invocation.cwd.starts_with(root.path()));
    assert_eq!(
        super::normalize("\u{1c} polished \u{1f}".as_bytes().to_vec())?,
        "polished\n"
    );
    assert!(matches!(
        super::normalize("\u{1c}\u{1f}".as_bytes().to_vec()),
        Err(EngineError::EmptyAnswer)
    ));
    Ok(())
}
