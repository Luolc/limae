use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

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
    Ok(Command::new(env!("CARGO_BIN_EXE_limae-rs"))
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
fn fix_reports_write_then_rechecks_the_file() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("limae.toml"),
        "severity = { zh-typography-1 = 'warning' }\n",
    )?;
    fs::write(root.path().join("t.md"), "你好,世界")?;

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
fn repeatable_comma_flags_and_double_dash_keep_a_hyphen_path() -> TestResult {
    let root = TempDir::new()?;
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
fn all_overrides_explicit_files_and_all_ignored_is_clean() -> TestResult {
    let selected = TempDir::new()?;
    git(selected.path(), &["init", "-q"])?;
    fs::write(selected.path().join("tracked.md"), "你好,世界")?;
    fs::write(selected.path().join("explicit.md"), "clean")?;
    git(selected.path(), &["add", "tracked.md"])?;
    let (code, stdout, stderr) = output_text(run(selected.path(), &["explicit.md", "--all"])?)?;
    assert_eq!((code, stderr.as_str()), (1, ""));
    assert!(stdout.contains("tracked.md:1: error:"));
    assert!(!stdout.contains("explicit.md"));

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

#[test]
fn empty_selection_is_usage_but_git_failure_is_execution_error() -> TestResult {
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

    let (code, stdout, stderr) = output_text(run(root.path(), &["--all"])?)?;
    assert_eq!((code, stdout.as_str()), (1, ""));
    assert!(stderr.contains("git ls-files in"));
    assert!(!stderr.contains("fatal:"));
    assert!(!stderr.contains("clean"));
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

    for subcommand in ["polish", "hook"] {
        let (code, stdout, stderr) = output_text(run(root.path(), &[subcommand])?)?;
        assert_eq!((code, stdout.as_str()), (2, ""));
        assert!(stderr.contains("subcommand is not provided yet"));
    }
    Ok(())
}
