use super::run;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn Error>>;

/// Synthetic prose, as the golden fixtures are: nothing here is a real
/// document and nothing reaches a real model (`AGENTS.md` 「隐私边界」).
const TEXT: &str = "the acme report\n";
const CHINESE: &str = "ACME 的报告写得不好。\n";

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-polish-cli-{name}-{}-{}",
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

/// One arm's whole environment.
///
/// `PATH`, `HOME` and `XDG_CACHE_HOME` all point inside the arm's own
/// temporary directory: the machine running these tests may have all three
/// CLIs installed and logged in, and an arm that read them would pass here and
/// fail on the next machine.
fn environment(root: &Path, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    let mut env = vec![
        ("PATH".into(), root.join("bin").into_os_string()),
        ("HOME".into(), root.join("home").into_os_string()),
        ("XDG_CACHE_HOME".into(), root.join("cache").into_os_string()),
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
fn stub(root: &Path, name: &str, body: &str) -> Result<(), std::io::Error> {
    use std::os::unix::fs::PermissionsExt;

    let directory = root.join("bin");
    fs::create_dir_all(&directory)?;
    let path = directory.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
}

struct Ended {
    code: u8,
    stdout: String,
    stderr: String,
}

fn invoke(
    args: &[&str],
    text: &str,
    root: &Path,
    env: &[(OsString, OsString)],
) -> Result<Ended, Box<dyn Error>> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut stdin = Cursor::new(text.as_bytes());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run(&args, root, env, &mut stdin, &mut stdout, &mut stderr);
    Ok(Ended {
        code,
        stdout: String::from_utf8(stdout)?,
        stderr: String::from_utf8(stderr)?,
    })
}

fn custom(root: &Path, command: &str) -> Result<(), std::io::Error> {
    fs::write(
        root.join("limae.toml"),
        format!("[polish]\nengine = \"custom\"\ncommand = [\"{command}\"]\n"),
    )
}

#[cfg(unix)]
#[test]
fn a_rewrite_reaches_stdout_alone_and_exits_zero() -> TestResult {
    let root = TempDir::new("ok")?;
    // Shell built-ins only: `PATH` holds this arm's stub directory and
    // nothing else, so that no engine installed on the machine can answer.
    stub(
        root.path(),
        "mygateway",
        "IFS= read -r line\nprintf 'polished: %s\\n' \"$line\"",
    )?;
    custom(root.path(), "mygateway")?;

    let env = environment(root.path(), &[]);
    let ended = invoke(&["-"], TEXT, root.path(), &env)?;

    assert_eq!(ended.code, 0);
    assert_eq!(ended.stdout, "polished: the acme report\n");
    assert_eq!(ended.stderr, "");
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_engine_that_did_not_answer_exits_one_and_says_so() -> TestResult {
    let root = TempDir::new("failed")?;
    stub(root.path(), "mygateway", "exit 1")?;
    custom(root.path(), "mygateway")?;

    let env = environment(root.path(), &[]);
    let ended = invoke(&["-"], CHINESE, root.path(), &env)?;

    assert_eq!(ended.code, 1);
    assert_eq!(ended.stdout, "");
    assert!(
        ended.stderr.starts_with("engine error:"),
        "stderr was {:?}",
        ended.stderr
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_engine_that_answers_nothing_exits_one() -> TestResult {
    let root = TempDir::new("empty-answer")?;
    stub(root.path(), "mygateway", "printf '   \\n'")?;
    custom(root.path(), "mygateway")?;

    let env = environment(root.path(), &[]);
    let ended = invoke(&["-"], TEXT, root.path(), &env)?;

    assert_eq!((ended.code, ended.stdout.as_str()), (1, ""));
    assert!(ended.stderr.starts_with("engine error:"));
    Ok(())
}

#[test]
fn blank_stdin_is_a_usage_error_not_an_engine_failure() -> TestResult {
    let root = TempDir::new("blank")?;
    let env = environment(root.path(), &[]);

    for text in ["", "  \n", "\u{3000}\t\n"] {
        let ended = invoke(&["-"], text, root.path(), &env)?;
        assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
        assert_eq!(ended.stderr, "input error: nothing on stdin to polish\n");
    }
    Ok(())
}

#[test]
fn a_file_argument_is_refused_until_the_next_step() -> TestResult {
    let root = TempDir::new("file-argument")?;
    let env = environment(root.path(), &[]);

    let ended = invoke(&["doc.md"], TEXT, root.path(), &env)?;

    assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
    assert!(
        ended
            .stderr
            .contains("only '-' (stdin) is supported so far"),
        "stderr was {:?}",
        ended.stderr
    );
    Ok(())
}

#[test]
fn an_unknown_engine_is_caught_at_both_tiers_the_config_file_never_sees() -> TestResult {
    let root = TempDir::new("unknown")?;
    fs::write(
        root.path().join("limae.toml"),
        "[polish]\nengine = \"grok\"\n",
    )?;

    let flagged = invoke(
        &["-", "--engine", "gemini"],
        TEXT,
        root.path(),
        &environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let from_variable = invoke(
        &["-"],
        TEXT,
        root.path(),
        &environment(root.path(), &[("LIMAE_ENGINE", "gemini")]),
    )?;

    for ended in [flagged, from_variable] {
        assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
        assert_eq!(
            ended.stderr,
            "config error: unknown engine 'gemini'; \
             pick one of auto, claude, codex, grok, custom\n"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn the_flag_outranks_the_variable_which_outranks_the_file() -> TestResult {
    let root = TempDir::new("precedence")?;
    stub(root.path(), "claude", "echo claude")?;
    stub(root.path(), "grok", "echo grok")?;
    stub(root.path(), "mygateway", "echo file")?;
    custom(root.path(), "mygateway")?;

    let with_flag = invoke(
        &["-", "--engine", "claude"],
        TEXT,
        root.path(),
        &environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let with_variable = invoke(
        &["-"],
        TEXT,
        root.path(),
        &environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let from_file = invoke(&["-"], TEXT, root.path(), &environment(root.path(), &[]))?;

    assert_eq!(
        (
            with_flag.stdout.as_str(),
            with_variable.stdout.as_str(),
            from_file.stdout.as_str()
        ),
        ("claude\n", "grok\n", "file\n")
    );
    assert_eq!(
        (with_flag.code, with_variable.code, from_file.code),
        (0, 0, 0)
    );
    Ok(())
}

#[test]
fn custom_without_a_command_is_a_config_error() -> TestResult {
    let root = TempDir::new("custom-bare")?;
    let env = environment(root.path(), &[]);

    let ended = invoke(&["-", "--engine", "custom"], TEXT, root.path(), &env)?;

    assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
    assert_eq!(
        ended.stderr,
        "config error: engine 'custom' needs [polish] command, \
         the whole command to run\n"
    );
    Ok(())
}

#[test]
fn an_unreadable_config_file_is_a_config_error() -> TestResult {
    let root = TempDir::new("bad-config")?;
    fs::write(root.path().join("limae.toml"), "not valid toml = [")?;
    let env = environment(root.path(), &[]);

    let ended = invoke(&["-"], TEXT, root.path(), &env)?;

    assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
    assert!(
        ended.stderr.starts_with("config error:"),
        "stderr was {:?}",
        ended.stderr
    );
    Ok(())
}

#[test]
fn auto_with_nothing_installed_diagnoses_every_engine_and_exits_one() -> TestResult {
    let root = TempDir::new("auto-none")?;
    fs::create_dir(root.path().join("bin"))?;
    let env = environment(root.path(), &[]);

    let ended = invoke(&["-"], TEXT, root.path(), &env)?;

    assert_eq!((ended.code, ended.stdout.as_str()), (1, ""));
    assert!(ended.stderr.starts_with("engine error:"));
    for engine in ["claude", "codex", "grok"] {
        assert!(
            ended.stderr.contains(&format!("{engine}: not installed")),
            "stderr was {:?}",
            ended.stderr
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn auto_runs_the_first_engine_that_answers_the_probe() -> TestResult {
    let root = TempDir::new("auto-probe")?;
    // The probe hands the engine the word `probe`; anything else in this arm
    // is the real call, which this stub answers by shouting the prose back.
    stub(
        root.path(),
        "claude",
        "body=''\n\
         while IFS= read -r line || [ -n \"$line\" ]; \
         do body=\"$body$line\"; done\n\
         case \"$body\" in *probe*) echo LIMAE-PROBE-OK ;; \
         *) printf 'polished: %s\\n' \"$body\" ;; esac",
    )?;
    let env = environment(root.path(), &[]);

    let ended = invoke(&["-"], TEXT, root.path(), &env)?;

    assert_eq!(ended.code, 0, "stderr {:?}", ended.stderr);
    assert!(
        ended.stdout.contains("polished: ") && ended.stdout.contains("the acme report"),
        "stdout was {:?}",
        ended.stdout
    );
    assert_eq!(ended.stderr, "");
    Ok(())
}

#[cfg(unix)]
#[test]
fn the_model_flag_replaces_the_presets_own_default() -> TestResult {
    let root = TempDir::new("model")?;
    stub(root.path(), "claude", "printf '%s\\n' \"$*\"")?;
    let env = environment(root.path(), &[]);

    let default = invoke(&["-", "--engine", "claude"], TEXT, root.path(), &env)?;
    let overridden = invoke(
        &["-", "--engine", "claude", "--model", "opus"],
        TEXT,
        root.path(),
        &env,
    )?;

    assert_eq!((default.code, overridden.code), (0, 0));
    assert!(default.stdout.contains("--model sonnet"));
    assert!(overridden.stdout.contains("--model opus"));
    Ok(())
}
