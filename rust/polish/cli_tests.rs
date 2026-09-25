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

/// The file-mode arms' environment: the stub directory first, then the
/// system directories the stubs' `cat` and `grep` live in. A preset CLI on
/// the machine cannot answer these arms either — the engine is named
/// `custom` in every one, and a custom command is never searched for.
fn file_environment(root: &Path, extra: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    let mut env = environment(root, extra);
    env[0].1 = format!("{}:/usr/bin:/bin", root.join("bin").display()).into();
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

fn git(cwd: &Path, args: &[&str]) -> TestResult {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()?;
    if !status.success() {
        return Err(format!("git {} failed: {status}", args.join(" ")).into());
    }
    Ok(())
}

/// A synthetic repository for the file-mode arms: one target, one
/// reference file, one ignored file, one untracked file, one link, and the
/// `claude` preset selected in the configuration. Everything in it is fake
/// (`AGENTS.md` 「隐私边界」).
#[cfg(unix)]
fn repository(root: &Path) -> TestResult {
    git(root, &["init", "-q"])?;
    fs::create_dir_all(root.join("docs"))?;
    fs::write(root.join("docs/target.md"), TEXT)?;
    fs::write(root.join("docs/notes.md"), "notes\n")?;
    fs::write(root.join(".gitignore"), ".env\n")?;
    fs::write(root.join(".env"), "SYNTHETIC=1\n")?;
    std::os::unix::fs::symlink("docs/target.md", root.join("link.md"))?;
    git(root, &["add", "-A"])?;
    fs::write(root.join("untracked.md"), "never added\n")?;
    fs::write(root.join("limae.toml"), "[polish]\nengine = \"claude\"\n")?;
    Ok(())
}

/// A stub standing in for the `claude` binary. File mode only runs the
/// presets, so the arms below impersonate one: the payload arrives on stdin
/// behind the marker line, which `$skip` consumes, and the spec file is the
/// third argument (`-p --system-prompt-file <path> …`). A preset gets the
/// filtered environment, so anything an arm needs to know (a path to write)
/// is baked into the body rather than passed through a variable.
#[cfg(unix)]
fn claude_stub(root: &Path, body: &str) -> Result<(), std::io::Error> {
    stub(root, "claude", &format!("IFS= read -r skip\n{body}"))
}

/// The stub body that answers "polished: <first line>", as the stdin arms do.
const REWRITE: &str = "IFS= read -r line\nprintf 'polished: %s\\n' \"$line\"";

#[cfg(unix)]
#[test]
fn a_file_without_the_flag_is_refused_before_any_engine_starts() -> TestResult {
    let root = TempDir::new("file-refused")?;
    // An engine that leaves a mark when it starts: the refusal has to happen
    // before that, and the mark is what tells "refused before" from
    // "refused after".
    let mark = root.path().join("started");
    claude_stub(root.path(), &format!(": > '{}'\ncat", mark.display()))?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);

    for args in [
        &["docs/target.md"][..],
        &["--all"][..],
        &["docs/target.md", "--engine", "claude"][..],
    ] {
        let ended = invoke(args, "", root.path(), &env)?;
        assert_eq!((ended.code, ended.stdout.as_str()), (2, ""), "{args:?}");
        assert!(
            ended.stderr.contains("Nothing has been started"),
            "{args:?}: {}",
            ended.stderr
        );
        assert!(
            ended.stderr.contains("--share-repo-with-engine"),
            "{}",
            ended.stderr
        );
        assert!(!mark.exists(), "{args:?} started the engine");
    }
    assert_eq!(
        fs::read_to_string(root.path().join("docs/target.md"))?,
        TEXT
    );

    // The control arm: the same call with the flag starts the engine.
    let ended = invoke(
        &["--share-repo-with-engine", "docs/target.md"],
        "",
        root.path(),
        &env,
    )?;
    assert_eq!(ended.code, 0, "{}", ended.stderr);
    assert!(mark.exists());
    Ok(())
}

/// The repository's own configuration cannot make file mode run a program:
/// a `custom` engine is refused before it starts, whichever tier names it.
/// The control arm is stdin mode, where the same configuration runs the
/// same command — so the refusal is file mode's, not a broken stub.
#[cfg(unix)]
#[test]
fn a_custom_engine_from_the_repository_is_refused_in_file_mode_before_it_runs() -> TestResult {
    let root = TempDir::new("file-custom")?;
    let mark = root.path().join("outside-the-view");
    stub(
        root.path(),
        "mygateway",
        &format!(": > '{}'\ncat", mark.display()),
    )?;
    repository(root.path())?;
    custom(root.path(), "mygateway")?;
    let env = file_environment(root.path(), &[]);

    for args in [
        &["--share-repo-with-engine", "docs/target.md"][..],
        &["--share-repo-with-engine", "--all"][..],
        &[
            "--share-repo-with-engine",
            "--engine",
            "custom",
            "docs/target.md",
        ][..],
    ] {
        let ended = invoke(args, "", root.path(), &env)?;
        assert_eq!((ended.code, ended.stdout.as_str()), (2, ""), "{args:?}");
        assert!(
            ended
                .stderr
                .contains("file mode runs the three presets only"),
            "{args:?}: {}",
            ended.stderr
        );
        assert!(!mark.exists(), "{args:?} ran the custom command");
    }
    let variable = file_environment(root.path(), &[("LIMAE_ENGINE", "custom")]);
    let ended = invoke(
        &["--share-repo-with-engine", "docs/target.md"],
        "",
        root.path(),
        &variable,
    )?;
    assert_eq!(ended.code, 2, "{}", ended.stderr);
    assert!(!mark.exists());
    assert_eq!(
        fs::read_to_string(root.path().join("docs/target.md"))?,
        TEXT
    );

    let ended = invoke(&["-"], TEXT, root.path(), &env)?;
    assert_eq!(ended.code, 0, "{}", ended.stderr);
    assert_eq!(ended.stdout, TEXT);
    assert!(mark.exists(), "stdin mode did not run the custom command");
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_file_is_rewritten_in_place_from_an_engine_started_in_the_view() -> TestResult {
    let root = TempDir::new("file-ok")?;
    // The engine reports where it was started: the reference file is there,
    // the ignored file and `.git` are not, the spec carries the file-mode
    // layer with the target's repository-relative path, and the directory
    // itself is named so that it can be checked afterwards.
    claude_stub(
        root.path(),
        "printf 'notes=%s env=%s git=%s layer=%s\\ncwd=%s\\n' \
         \"$([ -e docs/notes.md ] && echo yes || echo no)\" \
         \"$([ -e .env ] && echo yes || echo no)\" \
         \"$([ -e .git ] && echo yes || echo no)\" \
         \"$(grep -c 'the file `docs/target.md` of a repository' \"$3\")\" \
         \"$(pwd)\"",
    )?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &["--share-repo-with-engine", "docs/target.md"],
        "",
        root.path(),
        &env,
    )?;

    assert_eq!(ended.code, 0, "{}", ended.stderr);
    assert_eq!(ended.stdout, "polished: docs/target.md\n");
    assert_eq!(ended.stderr, "");
    let written = fs::read_to_string(root.path().join("docs/target.md"))?;
    let (report, cwd) = written
        .split_once("cwd=")
        .ok_or("the stub did not report its directory")?;
    assert_eq!(report, "notes=yes env=no git=no layer=1\n");
    // The engine ran in a private directory that is neither the repository
    // nor inside it, and that directory is gone once the run is over.
    let cwd = Path::new(cwd.trim_end());
    assert!(
        !cwd.starts_with(root.path().canonicalize()?),
        "{}",
        cwd.display()
    );
    assert!(!cwd.exists(), "{} was left behind", cwd.display());
    Ok(())
}

#[cfg(unix)]
#[test]
fn an_identical_answer_leaves_the_file_untouched() -> TestResult {
    let root = TempDir::new("file-unchanged")?;
    claude_stub(root.path(), "cat")?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);
    let before = fs::metadata(root.path().join("docs/target.md"))?.modified()?;

    let ended = invoke(
        &["--share-repo-with-engine", "docs/target.md"],
        "",
        root.path(),
        &env,
    )?;

    assert_eq!(ended.code, 0, "{}", ended.stderr);
    assert_eq!(ended.stdout, "unchanged: docs/target.md\n");
    assert_eq!(
        fs::metadata(root.path().join("docs/target.md"))?.modified()?,
        before
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn the_tripwire_stops_the_run_when_the_engine_writes_in_its_view() -> TestResult {
    let root = TempDir::new("tripwire")?;
    claude_stub(root.path(), "printf 'x' > PWNED.txt; cat")?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &[
            "--share-repo-with-engine",
            "docs/target.md",
            "docs/notes.md",
        ],
        "",
        root.path(),
        &env,
    )?;

    assert_eq!((ended.code, ended.stdout.as_str()), (1, ""));
    assert!(
        ended
            .stderr
            .contains("tripwire: the engine changed 1 path(s) in its view (first: PWNED.txt)"),
        "{}",
        ended.stderr
    );
    assert_eq!(
        fs::read_to_string(root.path().join("docs/target.md"))?,
        TEXT
    );
    assert_eq!(
        fs::read_to_string(root.path().join("docs/notes.md"))?,
        "notes\n"
    );
    assert!(!root.path().join("PWNED.txt").exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_target_that_changed_during_the_run_keeps_its_new_content() -> TestResult {
    let root = TempDir::new("conflict")?;
    // The engine plays the concurrent editor: it overwrites the target
    // through an absolute path baked into it, then answers normally.
    let target = root.path().join("docs/target.md");
    claude_stub(
        root.path(),
        &format!("printf 'edited meanwhile\\n' > '{}'; cat", target.display()),
    )?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &[
            "--share-repo-with-engine",
            "docs/target.md",
            "docs/notes.md",
        ],
        "",
        root.path(),
        &env,
    )?;

    // The first target is refused, the second still goes through: a
    // conflict is per file, not a stop.
    assert_eq!(ended.code, 1);
    assert_eq!(ended.stdout, "unchanged: docs/notes.md\n");
    assert!(
        ended
            .stderr
            .contains("not written: docs/target.md changed while the engine was running"),
        "{}",
        ended.stderr
    );
    assert_eq!(fs::read_to_string(&target)?, "edited meanwhile\n");
    Ok(())
}

/// A synthetic target with the three kinds of line the structure check
/// counts.
const STRUCTURED: &str =
    "# ACME\n\nthe acme report\n\n## Foo\n\n- one\n- two\n\n```sh\nacme run\n```\n";

#[cfg(unix)]
#[test]
fn a_rewrite_that_lost_a_heading_is_not_written() -> TestResult {
    let root = TempDir::new("structure")?;
    claude_stub(root.path(), "sed '/^## Foo/d; s/report/summary/'")?;
    repository(root.path())?;
    let target = root.path().join("docs/target.md");
    fs::write(&target, STRUCTURED)?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &["--share-repo-with-engine", "docs/target.md"],
        "",
        root.path(),
        &env,
    )?;

    assert_eq!((ended.code, ended.stdout.as_str()), (1, ""));
    assert_eq!(
        ended.stderr,
        "not written: docs/target.md: the rewrite changed the count of heading lines 2 → 1; the file is kept, polish it again\n"
    );
    assert_eq!(fs::read_to_string(&target)?, STRUCTURED);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_rewrite_that_lost_a_list_is_not_written_and_the_next_file_still_is() -> TestResult {
    let root = TempDir::new("structure-next")?;
    claude_stub(root.path(), "sed '/^- /d; s/notes/polished notes/'")?;
    repository(root.path())?;
    let target = root.path().join("docs/target.md");
    fs::write(&target, STRUCTURED)?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &[
            "--share-repo-with-engine",
            "docs/target.md",
            "docs/notes.md",
        ],
        "",
        root.path(),
        &env,
    )?;

    // Per file, like the conflict check: the broken rewrite is dropped and
    // the run goes on to the next target.
    assert_eq!(ended.code, 1);
    assert_eq!(ended.stdout, "polished: docs/notes.md\n");
    assert_eq!(
        ended.stderr,
        "not written: docs/target.md: the rewrite changed the count of list items 2 → 0; the file is kept, polish it again\n"
    );
    assert_eq!(fs::read_to_string(&target)?, STRUCTURED);
    assert_eq!(
        fs::read_to_string(root.path().join("docs/notes.md"))?,
        "polished notes\n"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn all_polishes_the_walk_skips_links_and_honours_limae_ignore() -> TestResult {
    let root = TempDir::new("file-all")?;
    claude_stub(root.path(), REWRITE)?;
    repository(root.path())?;
    fs::write(root.path().join("ignored.md"), "ignored by limae\n")?;
    fs::write(root.path().join(".limae-ignore"), "ignored.md\n")?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &["--share-repo-with-engine", "--all"],
        "",
        root.path(),
        &env,
    )?;

    assert_eq!(ended.code, 0, "{}", ended.stderr);
    // The walk's order: sorted, `docs/` before the root files.
    assert_eq!(
        ended.stdout,
        "polished: docs/notes.md\npolished: docs/target.md\npolished: untracked.md\n"
    );
    assert_eq!(
        ended.stderr,
        "skipped: link.md is a symbolic link; its target is polished under its own name\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("docs/target.md"))?,
        "polished: the acme report\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("ignored.md"))?,
        "ignored by limae\n"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_named_link_and_a_file_outside_a_repository_are_usage_errors() -> TestResult {
    let root = TempDir::new("file-link")?;
    claude_stub(root.path(), REWRITE)?;
    repository(root.path())?;
    let env = file_environment(root.path(), &[]);

    let ended = invoke(
        &["--share-repo-with-engine", "link.md"],
        "",
        root.path(),
        &env,
    )?;
    assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
    assert!(
        ended.stderr.contains("link.md is a symbolic link"),
        "{}",
        ended.stderr
    );

    let outside = TempDir::new("file-outside")?;
    claude_stub(outside.path(), REWRITE)?;
    fs::write(
        outside.path().join("limae.toml"),
        "[polish]\nengine = \"claude\"\n",
    )?;
    fs::write(outside.path().join("doc.md"), TEXT)?;
    let env = file_environment(outside.path(), &[]);
    let ended = invoke(
        &["--share-repo-with-engine", "doc.md"],
        "",
        outside.path(),
        &env,
    )?;
    assert_eq!((ended.code, ended.stdout.as_str()), (2, ""));
    assert!(
        ended.stderr.contains("file mode needs a git repository"),
        "{}",
        ended.stderr
    );
    assert_eq!(fs::read_to_string(outside.path().join("doc.md"))?, TEXT);
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
        &file_environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let from_variable = invoke(
        &["-"],
        TEXT,
        root.path(),
        &file_environment(root.path(), &[("LIMAE_ENGINE", "gemini")]),
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
        &file_environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let with_variable = invoke(
        &["-"],
        TEXT,
        root.path(),
        &file_environment(root.path(), &[("LIMAE_ENGINE", "grok")]),
    )?;
    let from_file = invoke(
        &["-"],
        TEXT,
        root.path(),
        &file_environment(root.path(), &[]),
    )?;

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
    let env = file_environment(root.path(), &[]);

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
    let env = file_environment(root.path(), &[]);

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
    let env = file_environment(root.path(), &[]);

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
    let env = file_environment(root.path(), &[]);

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
    let env = file_environment(root.path(), &[]);

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
