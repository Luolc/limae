use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use limae::cli::run_from;

type TestResult = Result<(), Box<dyn Error>>;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-cli-{}-{}",
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

fn run(cwd: &Path, args: &[impl AsRef<OsStr>]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_limae"))
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("missing PATH")?)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()?)
}

fn git(cwd: &Path, args: &[&str]) -> TestResult {
    let output = Command::new("git")
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("missing PATH")?)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()?;
    assert!(output.status.success(), "git {args:?}: {:?}", output.status);
    Ok(())
}

fn output_text(output: Output) -> Result<(i32, String, String), Box<dyn Error>> {
    Ok((
        output.status.code().ok_or("process terminated by signal")?,
        String::from_utf8(output.stdout)?,
        String::from_utf8(output.stderr)?,
    ))
}

#[test]
fn reports_configured_error_and_warning_in_source_order() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("limae.toml"),
        concat!(
            "enable_experimental = true\n",
            "severity = { zh-typography-1 = 'warning', zh-tell-1 = 'error' }\n",
        ),
    )?;
    fs::write(root.path().join("t.md"), "综上所述,这条路走不通。\n")?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["t.md"])?)?;
    assert_eq!(code, 1);
    assert_eq!(stderr, "");
    assert!(stdout.contains("t.md:1: warning: [zh-typography-1"));
    assert!(stdout.contains("t.md:1: error: [zh-tell-1 formulaic phrase]"));
    assert!(stdout.ends_with("\n1 error(s), 1 warning(s). --fix auto-fixes most.\n"));
    Ok(())
}

#[test]
fn warning_only_exits_zero_and_fix_rechecks_the_file() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("limae.toml"),
        "severity = { zh-typography-1 = 'warning' }\n",
    )?;
    fs::write(root.path().join("t.md"), "你好,世界")?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["t.md"])?)?;
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(stdout.ends_with("\n0 error(s), 1 warning(s). --fix auto-fixes most.\n"));

    let (code, stdout, stderr) = output_text(run(root.path(), &["--fix", "t.md"])?)?;
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(stdout, "fixed: t.md\nOK: 1 file(s) clean\n");
    assert_eq!(fs::read_to_string(root.path().join("t.md"))?, "你好，世界");
    Ok(())
}

#[test]
fn raw_empty_flag_replaces_an_unreadable_file_config() -> TestResult {
    let root = TempDir::new()?;
    fs::write(root.path().join("limae.toml"), "not valid toml = [")?;
    fs::write(root.path().join("t.md"), "你好,世界")?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["--disable", "", "t.md"])?)?;
    assert_eq!(code, 1);
    assert_eq!(stderr, "");
    assert!(stdout.contains("t.md:1: error: [zh-typography-1"));
    Ok(())
}

#[test]
fn enable_repeatable_comma_flags_and_double_dash_are_parsed() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("--all"),
        "中文[链接](https://example.com/) 后文",
    )?;
    let (code, stdout, stderr) = output_text(run(
        root.path(),
        &["--enable", "zh-typography-9", "--", "--all"],
    )?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.contains("--all:1: error: [zh-typography-9"));

    fs::write(root.path().join("--all"), "你好,世界")?;

    let (code, stdout, stderr) = output_text(run(
        root.path(),
        &[
            "--disable",
            "zh-typography-1,zh-typography-2",
            "--disable",
            "zh-typography-3",
            "--",
            "--all",
        ],
    )?)?;
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(stdout, "OK: 1 file(s) clean\n");
    Ok(())
}

#[test]
fn run_from_uses_explicit_cwd_and_keeps_relative_error_paths() -> TestResult {
    let root = TempDir::new()?;
    fs::write(root.path().join("t.md"), "你好,世界")?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_from(
        ["limae", "--fix", "t.md"].map(OsString::from),
        root.path(),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!((code, stderr.as_slice()), (0, &[][..]));
    assert_eq!(stdout, b"fixed: t.md\nOK: 1 file(s) clean\n");

    stdout.clear();
    let code = run_from(
        ["limae", "missing.md"].map(OsString::from),
        root.path(),
        &mut stdout,
        &mut stderr,
    );
    assert_eq!((code, stdout.as_slice()), (1, &[][..]));
    let stderr = String::from_utf8(stderr)?;
    assert!(stderr.starts_with("error: cannot read missing.md:"));
    assert!(!stderr.contains(&root.path().display().to_string()));
    Ok(())
}

#[test]
fn invalid_ignore_is_execution_error_and_unclosed_class_is_a_noop() -> TestResult {
    let invalid = TempDir::new()?;
    fs::write(invalid.path().join(".limae-ignore"), "!\n")?;
    fs::write(invalid.path().join("t.md"), "ACME\n")?;
    let (code, stdout, stderr) = output_text(run(invalid.path(), &["t.md"])?)?;
    assert_eq!((code, stdout.as_str()), (1, ""));
    assert!(stderr.contains(".limae-ignore:1: invalid ignore pattern"));
    assert!(!stderr.contains("clean"));

    let control = TempDir::new()?;
    fs::write(control.path().join(".limae-ignore"), "[abc\n")?;
    fs::write(control.path().join("t.md"), "你好,世界\n")?;
    let (code, stdout, stderr) = output_text(run(control.path(), &["t.md"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.contains("t.md:1: error: [zh-typography-1"));
    Ok(())
}

#[test]
fn all_overrides_explicit_files_and_all_ignored_is_clean() -> TestResult {
    let selected = TempDir::new()?;
    fs::write(selected.path().join("t.md"), "你好,世界")?;
    // Walking picks Markdown only, so a non-Markdown explicit input is what
    // still tells "`--all` replaces the list" from "`--all` adds to it".
    fs::write(selected.path().join("notes.txt"), "你好,世界")?;
    let (code, stdout, stderr) = output_text(run(selected.path(), &["notes.txt", "--all"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.starts_with("t.md:1: error:"), "{stdout}");
    assert!(!stdout.contains("notes.txt"), "{stdout}");

    let (code, stdout, stderr) = output_text(run(selected.path(), &["notes.txt"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.starts_with("notes.txt:1: error:"), "{stdout}");

    let ignored = TempDir::new()?;
    git(ignored.path(), &["init", "-q"])?;
    fs::create_dir(ignored.path().join("vendor"))?;
    fs::write(ignored.path().join("vendor/t.md"), "你好,世界")?;
    fs::write(ignored.path().join(".limae-ignore"), "vendor/\n")?;
    git(ignored.path(), &["add", "vendor/t.md"])?;
    let (code, stdout, stderr) = output_text(run(ignored.path(), &["--all"])?)?;
    assert_eq!(
        (code, stdout.as_str(), stderr.as_str()),
        (0, "OK: 0 file(s) clean\n", "")
    );
    Ok(())
}

/// The one arm this change exists for, plus the ones that bound it.
///
/// The positive arm was green before the change for the wrong reason: the file
/// was never opened, so it had no findings. That is why each control arm below
/// has to name a file that must *not* be checked; a selection that simply took
/// everything would pass the positive arm just as well. The dotted and linked
/// arms are the other half of that: a selection that took only what is plainly
/// visible would pass every control arm instead.
#[cfg(unix)]
#[test]
fn all_checks_unindexed_files_but_still_honours_every_ignore_source() -> TestResult {
    let root = TempDir::new()?;
    git(root.path(), &["init", "-q"])?;
    fs::write(root.path().join("tracked.md"), "clean\n")?;
    git(root.path(), &["add", "tracked.md"])?;

    // Positive arm: never `git add`ed, checked all the same.
    fs::write(root.path().join("new.md"), "你好,世界\n")?;
    // Control A: excluded by `.gitignore`.
    fs::create_dir(root.path().join("build"))?;
    fs::write(root.path().join("build/out.md"), "你好,世界\n")?;
    fs::write(root.path().join(".gitignore"), "build/\n")?;
    // Control B: excluded by `.limae-ignore`.
    fs::create_dir(root.path().join("vendor"))?;
    fs::write(root.path().join("vendor/v.md"), "你好,世界\n")?;
    fs::write(root.path().join(".limae-ignore"), "vendor/\n")?;
    // Control D: inside the repository's own metadata directory. `.git` is
    // excluded by name, not for being hidden, so this arm still means something
    // now that hidden entries are selected.
    fs::write(root.path().join(".git/inside.md"), "你好,世界\n")?;
    // Dotted names and dotted directories are prose too: `.github/` and
    // `.agents/` are where a project actually keeps some of it.
    fs::write(root.path().join(".dotted.md"), "你好,世界\n")?;
    fs::create_dir(root.path().join(".agents"))?;
    fs::write(root.path().join(".agents/brief.md"), "你好,世界\n")?;
    // A link to a Markdown file is checked through the link, the way the index
    // listed it before; a link to a directory is not descended.
    std::os::unix::fs::symlink("new.md", root.path().join("link.md"))?;
    fs::create_dir(root.path().join("real"))?;
    fs::write(root.path().join("real/deep.md"), "你好,世界\n")?;
    std::os::unix::fs::symlink("real", root.path().join("loop"))?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["--all"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    for checked in [
        ".agents/brief.md:1: error:",
        ".dotted.md:1: error:",
        "link.md:1: error:",
        "new.md:1: error:",
        "real/deep.md:1: error:",
    ] {
        assert!(stdout.contains(checked), "missing {checked} in {stdout}");
    }
    assert!(stdout.contains("5 error(s), 0 warning(s)"), "{stdout}");
    for unchecked in ["out.md", "v.md", "inside.md", "loop/"] {
        assert!(!stdout.contains(unchecked), "{unchecked} in {stdout}");
    }

    // The removed note has no successor: a selection with no blind spot has
    // nothing to warn about, and a note that never fires is worse than silence.
    assert!(!stderr.contains("note:"));
    Ok(())
}

/// Control C: `--all` outside a repository.
///
/// Before this change the same tree ended in `git ls-files ... failed`, so the
/// ability is new rather than pre-existing. Running the previous binary is what
/// establishes that, and lives in the PR description; what a test can pin is
/// that today's binary needs no repository and that the sibling ignore sources
/// still apply where Git is not the one supplying them.
#[test]
fn all_works_outside_a_repository() -> TestResult {
    let root = TempDir::new()?;
    fs::write(root.path().join("a.md"), "你好,世界\n")?;
    fs::create_dir(root.path().join("vendor"))?;
    fs::write(root.path().join("vendor/v.md"), "你好,世界\n")?;
    fs::write(root.path().join(".limae-ignore"), "vendor/\n")?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["--all"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.starts_with("a.md:1: error:"), "{stdout}");
    assert!(stdout.contains("1 error(s), 0 warning(s)"), "{stdout}");
    Ok(())
}

#[test]
fn an_empty_tree_is_a_usage_error_with_or_without_a_repository() -> TestResult {
    let root = TempDir::new()?;
    let (code, stdout, stderr) = output_text(run(root.path(), &[] as &[&str])?)?;
    assert_eq!(code, 2);
    assert_eq!(stdout, "");
    assert!(stderr.contains("no files given (use --all or list files)"));
    assert!(stderr.contains("Usage:"));

    let empty_git = TempDir::new()?;
    git(empty_git.path(), &["init", "-q"])?;
    let (code, stdout, stderr) = output_text(run(empty_git.path(), &["--all"])?)?;
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("no files given (use --all or list files)"));

    // Outside a repository too: an empty tree is the same usage error, not the
    // execution error that a Git-backed selection used to give here.
    let (code, stdout, stderr) = output_text(run(root.path(), &["--all"])?)?;
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("no files given (use --all or list files)"));
    assert!(!stderr.contains("git ls-files in"));
    Ok(())
}

#[test]
fn later_read_failure_keeps_prior_write_but_withholds_findings() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("limae.toml"),
        "enable_experimental = true\n",
    )?;
    fs::write(root.path().join("first.md"), "综上所述,这条路走不通。\n")?;

    let (code, stdout, stderr) =
        output_text(run(root.path(), &["--fix", "first.md", "missing.md"])?)?;
    assert_eq!(code, 1);
    assert_eq!(stdout, "fixed: first.md\n");
    assert!(stderr.contains("cannot read missing.md"));
    assert_eq!(
        fs::read_to_string(root.path().join("first.md"))?,
        "综上所述，这条路走不通。\n"
    );
    Ok(())
}

#[test]
fn directive_and_unavailable_subcommands_are_usage_errors() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("t.md"),
        "<!-- limae-disable unknown -->\n你好,世界\n",
    )?;
    let (code, stdout, stderr) = output_text(run(root.path(), &["t.md"])?)?;
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("directive error: t.md:1: unknown rule id(s) unknown"));

    // Both subcommands are wired now, and each answers with its own usage,
    // which is what tells the two apart.
    let (code, stdout, stderr) = output_text(run(root.path(), &["hook", "MessageDisplay"])?)?;
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(
        stderr.contains("reads one hook event as JSON on stdin"),
        "{stderr}"
    );

    let (code, stdout, stderr) = output_text(run(root.path(), &["polish", "doc.md"])?)?;
    assert_eq!((code, stdout.as_str()), (2, ""));
    assert!(stderr.contains("only \'-\' (stdin) is supported so far"));
    Ok(())
}

/// One real process for the `polish` subcommand.
///
/// The offline arms exercise `polish::cli::run` directly; this one is here for
/// what only a process has: the subcommand dispatch, the ambient environment
/// and standard input, and the exit code `main` hands back to the shell.
#[cfg(unix)]
#[test]
fn the_polish_subcommand_rewrites_standard_input_through_a_custom_command() -> TestResult {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    let root = TempDir::new()?;
    let bin = root.path().join("bin");
    fs::create_dir(&bin)?;
    // Shell built-ins only: `PATH` is this directory alone, so no engine
    // installed on the machine can answer and nothing reaches a real model.
    let stub = bin.join("mygateway");
    fs::write(
        &stub,
        "#!/bin/sh\nIFS= read -r line\nprintf 'polished: %s\\n' \"$line\"\n",
    )?;
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))?;
    fs::write(
        root.path().join("limae.toml"),
        "[polish]\nengine = \"custom\"\ncommand = [\"mygateway\"]\n",
    )?;

    let mut child = Command::new(env!("CARGO_BIN_EXE_limae"))
        .current_dir(root.path())
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", root.path().join("home"))
        .env("XDG_CACHE_HOME", root.path().join("cache"))
        .args(["polish", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("missing child stdin")?
        .write_all(b"the acme report\n")?;

    let (code, stdout, stderr) = output_text(child.wait_with_output()?)?;
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert_eq!(stdout, "polished: the acme report\n");
    Ok(())
}

/// One real process for the `hook` subcommand.
///
/// The offline arms exercise `hook::cli::run` directly; this one is here for
/// what only a process has: the subcommand dispatch, the ambient environment
/// and standard input, and the exit code the host reads. The host's contract is
/// that this exit code is always 0 (ADR-0009 section 六), which is why the
/// screen output beside it is what says the run did anything at all.
#[cfg(unix)]
#[test]
fn the_hook_subcommand_answers_one_message_display_event_on_standard_input() -> TestResult {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    let root = TempDir::new()?;
    let bin = root.path().join("bin");
    fs::create_dir(&bin)?;
    // Shell built-ins only: `PATH` is this directory alone, so no engine
    // installed on the machine can answer and nothing reaches a real model.
    let stub = bin.join("claude");
    fs::write(
        &stub,
        "#!/bin/sh\ncat > /dev/null\nprintf '%s\\n' 'ACME 的报告写得不好 —— 请改得像人话一些。'\n",
    )?;
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))?;
    fs::write(
        root.path().join("limae.toml"),
        "[polish]\nengine = \"claude\"\n",
    )?;
    let message = "ACME 的报告写得不好，请把它改得像人话一些。".repeat(20);
    let payload = format!(
        concat!(
            r#"{{"session_id":"11111111-2222-3333-4444-555555555555","#,
            r#""message_id":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","#,
            r#""hook_event_name":"MessageDisplay","cwd":{cwd},"#,
            r#""index":0,"final":true,"delta":{delta}}}"#
        ),
        cwd = serde_json::to_string(&root.path().to_string_lossy())?,
        delta = serde_json::to_string(&message)?,
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_limae"))
        .current_dir(root.path())
        .env_clear()
        .env("PATH", &bin)
        .env("HOME", root.path().join("home"))
        .env("XDG_CACHE_HOME", root.path().join("cache"))
        // Where scratch goes is where the state goes: the hook has no setting
        // of its own for it, on purpose.
        .env("TMPDIR", root.path().join("scratch"))
        .env("LIMAE_HOOK_AB_RATE", "0")
        .arg("hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("missing child stdin")?
        .write_all(payload.as_bytes())?;

    let (code, stdout, stderr) = output_text(child.wait_with_output()?)?;
    let answer: serde_json::Value = serde_json::from_str(stdout.trim_end())?;
    let shown = answer
        .pointer("/hookSpecificOutput/displayContent")
        .and_then(serde_json::Value::as_str)
        .ok_or("no displayContent in the answer")?;
    assert_eq!((code, stderr.as_str()), (0, ""));
    assert!(shown.starts_with(&message), "{shown:?}");
    assert!(shown.contains("── 润色 ──"), "{shown:?}");
    Ok(())
}

/// One finding's whole report, byte for byte.
///
/// Every other arm here reads the report with `contains`, which says nothing
/// about the frame around the finding: the `…` on either side of the excerpt is
/// what tells the reader they are looking at a window and not at the line, and
/// a run that lost it, or that cut the window somewhere else, would satisfy
/// every one of those assertions.
#[test]
fn a_single_finding_is_reported_and_summarised_byte_for_byte() -> TestResult {
    let root = TempDir::new()?;
    fs::write(root.path().join("t.md"), "你好,世界")?;

    let (code, stdout, stderr) = output_text(run(root.path(), &["t.md"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert_eq!(
        stdout,
        "t.md:1: error: [zh-typography-1 halfwidth punct next to CJK] …你好,世界…\n\
         \n1 error(s), 0 warning(s). --fix auto-fixes most.\n"
    );
    Ok(())
}

/// A configuration this run cannot act on is the user's own mistake, and the
/// `check` path says so in the two ways that are machine-readable: exit 2 —
/// which is what `polish` already answers, and what tells a caller "you asked
/// for something impossible" apart from "your document has problems" — and a
/// `config error: ` prefix on the diagnostic. Nothing goes to standard output,
/// because a report was never produced.
///
/// The control arm is the same file with the same rule under a configuration
/// that parses: exit 1 and the finding on standard output. Without it, a run
/// that answered 2 to everything would pass the arms above.
#[test]
fn a_configuration_error_is_a_usage_error_on_the_check_path() -> TestResult {
    let root = TempDir::new()?;
    fs::write(root.path().join("t.md"), "你好,世界")?;

    // Unparseable file, invalid value, and an override the command line
    // contradicts: three origins, one contract.
    for (name, contents, flags) in [
        ("pyproject.toml", "[project\n", &[] as &[&str]),
        ("limae.toml", "disable = \"zh-typography-1\"\n", &[]),
        ("limae.toml", "", &["--disable", "R99"]),
    ] {
        fs::write(root.path().join(name), contents)?;
        let mut args: Vec<&str> = flags.to_vec();
        args.push("t.md");
        let (code, stdout, stderr) = output_text(run(root.path(), &args)?)?;
        assert_eq!((code, stdout.as_str()), (2, ""), "for {name} {flags:?}");
        assert!(
            stderr.starts_with("config error: "),
            "for {name} {flags:?}: {stderr}"
        );
        fs::remove_file(root.path().join(name))?;
    }

    // Control arm: the same rule on the same file, configured legally.
    let (code, stdout, stderr) = output_text(run(root.path(), &["t.md"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(
        stdout.contains("t.md:1: error: [zh-typography-1"),
        "{stdout}"
    );
    Ok(())
}
