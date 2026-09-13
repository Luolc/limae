use super::{View, ViewError, repository_root};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn Error>>;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-view-test-{name}-{}-{}",
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

fn git(cwd: &Path, args: &[&str]) -> TestResult {
    let status = Command::new("git")
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

fn write(root: &Path, relative: &str, content: &str) -> std::io::Result<()> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

/// A synthetic repository with one of everything the view has to decide
/// about. Every file is fake (`AGENTS.md` 「隐私边界」).
fn repository(root: &Path) -> TestResult {
    git(root, &["init", "-q"])?;
    write(root, "docs/target.md", "ACME 的报告。\n")?;
    write(root, "docs/notes.md", "notes\n")?;
    write(root, ".github/workflows/ci.yml", "on: push\n")?;
    write(root, ".gitignore", ".env\n")?;
    write(root, ".env", "SYNTHETIC=1\n")?;
    for directory in [".claude", ".codex", ".grok"] {
        write(root, &format!("{directory}/settings.json"), "{}\n")?;
        write(root, &format!("nested/{directory}/hook.json"), "{}\n")?;
    }
    write(root, ".mcp.json", "{}\n")?;
    write(root, "nested/.mcp.json", "{}\n")?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("docs/target.md", root.join("link.md"))?;
    git(root, &["add", "-A"])?;
    write(root, "untracked.md", "never added\n")?;
    Ok(())
}

fn listing(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    fn collect(root: &Path, directory: &Path, paths: &mut Vec<String>) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                collect(root, &path, paths)?;
            } else {
                paths.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
        Ok(())
    }
    let mut paths = Vec::new();
    collect(root, root, &mut paths)?;
    paths.sort();
    Ok(paths)
}

#[test]
fn the_view_holds_tracked_regular_files_and_nothing_the_engines_configure_from() -> TestResult {
    let root = TempDir::new("export")?;
    repository(root.path())?;

    let view = View::export(root.path())?;
    let paths = listing(view.root())?;
    view.remove()?;

    // Tracked prose is in, dotted directories included: `.github` is not an
    // engine's directory. Ignored, untracked, `.git`, the three engine
    // directories and `.mcp.json` at any depth, and the link are all out.
    assert_eq!(
        paths,
        [
            ".github/workflows/ci.yml",
            ".gitignore",
            "docs/notes.md",
            "docs/target.md"
        ]
    );
    Ok(())
}

#[test]
fn the_view_is_removed_and_a_removed_view_is_gone() -> TestResult {
    let root = TempDir::new("remove")?;
    repository(root.path())?;

    let view = View::export(root.path())?;
    let path = view.root().to_owned();
    assert!(path.is_dir());
    view.remove()?;
    assert!(!path.exists());
    Ok(())
}

#[test]
fn the_snapshot_sees_a_new_a_changed_and_a_removed_path_and_nothing_else() -> TestResult {
    let root = TempDir::new("snapshot")?;
    repository(root.path())?;
    let view = View::export(root.path())?;

    let before = view.snapshot()?;
    // The control arm: nothing happened, nothing is reported.
    assert_eq!(before.differences(&view.snapshot()?), Vec::<PathBuf>::new());

    write(view.root(), "PWNED.txt", "new\n")?;
    write(view.root(), "docs/notes.md", "changed\n")?;
    fs::remove_file(view.root().join(".gitignore"))?;
    let after = view.snapshot()?;
    let changed = before.differences(&after);
    view.remove()?;

    assert_eq!(
        changed,
        [
            PathBuf::from(".gitignore"),
            PathBuf::from("PWNED.txt"),
            PathBuf::from("docs/notes.md")
        ]
    );
    Ok(())
}

#[test]
fn the_snapshot_sees_a_same_length_edit() -> TestResult {
    let root = TempDir::new("same-length")?;
    repository(root.path())?;
    let view = View::export(root.path())?;

    let before = view.snapshot()?;
    write(view.root(), "docs/notes.md", "notex\n")?;
    let changed = before.differences(&view.snapshot()?);
    view.remove()?;

    assert_eq!(changed, [PathBuf::from("docs/notes.md")]);
    Ok(())
}

#[test]
fn the_repository_root_is_found_from_a_subdirectory_and_refused_outside_one() -> TestResult {
    let root = TempDir::new("root")?;
    repository(root.path())?;

    let found = repository_root(&root.path().join("docs"))?;
    assert_eq!(found.canonicalize()?, root.path().canonicalize()?);

    let outside = TempDir::new("outside")?;
    let error = repository_root(outside.path())
        .err()
        .ok_or("found a repository outside one")?;
    assert!(matches!(error, ViewError::NotARepository), "{error}");
    Ok(())
}

/// A real repository whose index cannot be read is a git failure, not
/// "outside a repository": the two need different next steps. The control
/// arm is the test above, where a directory outside any repository is still
/// reported as such.
#[test]
fn a_listing_failure_inside_a_repository_is_reported_as_git_failing() -> TestResult {
    let root = TempDir::new("bad-index")?;
    repository(root.path())?;
    fs::remove_file(root.path().join(".git/index"))?;
    fs::create_dir(root.path().join(".git/index"))?;

    // `rev-parse` still finds the repository; only the listing fails.
    repository_root(root.path())?;
    let error = View::export(root.path())
        .err()
        .ok_or("exported a view from an unreadable index")?;
    assert!(
        matches!(
            error,
            ViewError::GitFailed {
                command: "ls-files",
                ..
            }
        ),
        "{error}"
    );
    Ok(())
}
