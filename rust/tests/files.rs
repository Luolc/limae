use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use limae::files::{GitError, IgnoreError, find_ignore, not_ignored, tracked_markdown};

type TestResult = Result<(), Box<dyn Error>>;

struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-files-{}-{}",
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

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn isolated_git_test(name: &str) -> Result<bool, Box<dyn Error>> {
    if std::env::var_os("LIMAE_TEST_GIT_ISOLATED").is_some() {
        return Ok(false);
    }
    let output = Command::new(std::env::current_exe()?)
        .args(["--exact", &format!("files::{name}"), "--nocapture"])
        .env_clear()
        .env("PATH", std::env::var_os("PATH").ok_or("missing PATH")?)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("LIMAE_TEST_GIT_ISOLATED", "1")
        .output()?;
    assert!(
        output.status.success(),
        "isolated re-exec failed: {}\n--- child stdout ---\n{}\n--- child stderr ---\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(true)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn tracked_selection_preserves_index_order_and_cwd_scope() -> TestResult {
    if isolated_git_test("tracked_selection_preserves_index_order_and_cwd_scope")? {
        return Ok(());
    }
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    let before = std::env::current_dir()?;
    for root in [a.path(), b.path()] {
        git(root, &["init", "-q"])?;
        fs::create_dir_all(root.join("docs/sub"))?;
        for name in [
            "z.md",
            "a space.md",
            "docs/b.md",
            "docs/sub/a.md",
            "gone.md",
            "removed.md",
            "x.txt",
        ] {
            fs::write(root.join(name), "ACME")?;
        }
        git(root, &["add", "."])?;
        git(root, &["rm", "--cached", "-q", "removed.md"])?;
        fs::remove_file(root.join("gone.md"))?;
        fs::write(root.join("untracked.md"), "ACME")?;
        // Repository ignore rules do not remove tracked files from ls-files.
        fs::write(root.join(".gitignore"), "*.md\n")?;
        assert_eq!(
            tracked_markdown(root)?,
            paths(&[
                "a space.md",
                "docs/b.md",
                "docs/sub/a.md",
                "gone.md",
                "z.md"
            ])
        );
        assert_eq!(
            tracked_markdown(&root.join("docs"))?,
            paths(&["b.md", "sub/a.md"])
        );
        fs::write(root.join(".limae-ignore"), "*.md\n!sub/a.md\n")?;
        assert_eq!(
            not_ignored(&tracked_markdown(&root.join("docs"))?, &root.join("docs"))?,
            Vec::<PathBuf>::new()
        );
        fs::write(root.join(".limae-ignore"), "*.md\n!docs/sub/a.md\n")?;
        assert_eq!(
            not_ignored(&tracked_markdown(&root.join("docs"))?, &root.join("docs"))?,
            paths(&["sub/a.md"])
        );
    }
    fs::write(b.path().join(".limae-ignore"), "")?;
    assert_eq!(
        not_ignored(&paths(&["z.md", "a space.md"]), b.path())?,
        paths(&["z.md", "a space.md"])
    );
    assert_eq!(
        not_ignored(&paths(&["z.md", "a space.md"]), a.path())?,
        Vec::<PathBuf>::new()
    );
    assert_eq!(std::env::current_dir()?, before);
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn tracked_names_use_nul_delimiters_and_native_bytes() -> TestResult {
    if isolated_git_test("tracked_names_use_nul_delimiters_and_native_bytes")? {
        return Ok(());
    }
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let root = TempDir::new()?;
    git(root.path(), &["init", "-q"])?;
    let names = paths(&[
        "a space.md",
        "line\nbreak.md",
        "quote\".md",
        "tab\t.md",
        "中文.md",
    ]);
    for name in &names {
        fs::write(root.path().join(name), "ACME")?;
    }
    let native = PathBuf::from(OsString::from_vec(b"\xff.md".to_vec()));
    fs::write(root.path().join(&native), "ACME")?;
    git(root.path(), &["add", "."])?;
    let mut expected = names;
    expected.push(native);
    assert_eq!(tracked_markdown(root.path())?, expected);
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn git_failure_is_distinct_from_an_empty_index() -> TestResult {
    if isolated_git_test("git_failure_is_distinct_from_an_empty_index")? {
        return Ok(());
    }
    let root = TempDir::new()?;
    assert!(matches!(
        tracked_markdown(root.path()),
        Err(GitError::Failed { .. })
    ));
    git(root.path(), &["init", "-q"])?;
    assert_eq!(tracked_markdown(root.path())?, Vec::<PathBuf>::new());
    let missing = root.path().join("missing");
    assert!(matches!(tracked_markdown(&missing), Err(GitError::Io { cwd, .. }) if cwd == missing));
    Ok(())
}

#[test]
fn ignore_discovery_is_independent_nearest_and_checks_before_git() -> TestResult {
    let root = TempDir::new()?;
    let child = root.path().join("child");
    let nested = child.join("nested");
    fs::create_dir_all(&nested)?;
    fs::write(root.path().join(".limae-ignore"), "*.md\n")?;
    fs::write(child.join("limae.toml"), "")?;
    assert_eq!(
        find_ignore(&nested)?,
        Some(root.path().join(".limae-ignore"))
    );
    for git_is_file in [true, false] {
        if git_is_file {
            fs::write(child.join(".git"), "gitdir: synthetic")?;
        } else {
            fs::create_dir(child.join(".git"))?;
        }
        assert_eq!(find_ignore(&nested)?, None);
        fs::write(child.join(".limae-ignore"), "skip.md\n")?;
        assert_eq!(find_ignore(&nested)?, Some(child.join(".limae-ignore")));
        assert_eq!(
            not_ignored(&paths(&["keep.md", "skip.md"]), &nested)?,
            paths(&["keep.md"])
        );
        fs::remove_file(child.join(".limae-ignore"))?;
        if git_is_file {
            fs::remove_file(child.join(".git"))?;
        } else {
            fs::remove_dir(child.join(".git"))?;
        }
    }
    fs::remove_file(root.path().join(".limae-ignore"))?;
    fs::create_dir(root.path().join(".git"))?;
    let input = paths(&["missing.md", "./missing.md", "missing.md"]);
    assert_eq!(not_ignored(&input, root.path())?, input);
    Ok(())
}

#[test]
fn ignore_patterns_match_python_file_semantics_in_order() -> TestResult {
    let root = TempDir::new()?;
    fs::create_dir(root.path().join(".git"))?;
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "# comment\n\n*.md\n!keep.md\n",
            &["keep.md", "skip.md", "sub/keep.md", "keep.md"],
            &["keep.md", "sub/keep.md", "keep.md"],
        ),
        ("/a.md\n", &["a.md", "sub/a.md"], &["sub/a.md"]),
        (
            "a/**/b.md\n",
            &["a/b.md", "a/x/b.md", "x/a/b.md"],
            &["x/a/b.md"],
        ),
        (
            "vendor/\n!vendor/keep.md\n",
            &["vendor/x.md", "vendor/keep.md"],
            &["vendor/keep.md"],
        ),
        (
            "!vendor/keep.md\nvendor/\n",
            &["vendor/x.md", "vendor/keep.md"],
            &["vendor/keep.md"],
        ),
        (
            "vendor/inner/\n!vendor/\n",
            &["vendor/inner/x.md"],
            &["vendor/inner/x.md"],
        ),
        ("!vendor/inner/\nvendor/\n", &["vendor/inner/x.md"], &[]),
        (
            "vendor/*\n!vendor/\n",
            &["vendor/inner/x.md"],
            &["vendor/inner/x.md"],
        ),
        ("*.md\n!keep.md\nkeep.md\n", &["keep.md", "skip.md"], &[]),
        (
            "\\#x.md\r\\!x.md\r\n",
            &["#x.md", "!x.md", "x.md"],
            &["x.md"],
        ),
        (
            "a?.md\na[0-9].txt\n",
            &["ab.md", "a1.txt", "a10.txt"],
            &["a10.txt"],
        ),
        (
            "a\\ b.md\nspace\\ \n",
            &["a b.md", "space ", "space"],
            &["space"],
        ),
        (
            "{a,b}.md\n",
            &["a.md", "b.md", "{a,b}.md"],
            &["a.md", "b.md"],
        ),
        ("[ab.md\n", &["[ab.md", "a.md"], &["[ab.md", "a.md"]),
        (
            "[a/]b.md\n",
            &["ab.md", "[a/]b.md", "a/b.md"],
            &["ab.md", "[a/]b.md", "a/b.md"],
        ),
        ("/\n", &["a.md", "dir/a.md"], &["a.md", "dir/a.md"]),
        ("[{}].md\n", &["{.md", "}.md", "\\.md"], &["\\.md"]),
    ];
    for (patterns, input, expected) in cases {
        fs::write(root.path().join(".limae-ignore"), patterns)?;
        assert_eq!(
            not_ignored(&paths(input), root.path())?,
            paths(expected),
            "{patterns:?}"
        );
    }
    Ok(())
}

#[test]
fn missing_suffixes_and_outer_paths_are_not_lost() -> TestResult {
    let root = TempDir::new()?;
    let repo = root.path().join("repo");
    fs::create_dir(&repo)?;
    fs::write(repo.join(".limae-ignore"), "/skip.md\n")?;
    let input = vec![
        PathBuf::from("./keep.md"),
        PathBuf::from("missing/../skip.md"),
        root.path().join("skip.md"),
        PathBuf::from("./keep.md"),
    ];
    let actual = not_ignored(&input, &repo)?;
    assert_eq!(
        actual
            .iter()
            .map(|path| path.as_os_str())
            .collect::<Vec<_>>(),
        [
            input[0].as_os_str(),
            input[2].as_os_str(),
            input[3].as_os_str()
        ]
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn ignore_resolves_symlinks_before_root_and_pattern_matching() -> TestResult {
    use std::os::unix::fs::symlink;
    let root = TempDir::new()?;
    let repo = root.path().join("repo");
    fs::create_dir(&repo)?;
    fs::create_dir(repo.join("nested"))?;
    fs::write(repo.join(".limae-ignore"), "*.md\n")?;
    symlink(root.path(), repo.join("outside"))?;
    symlink(repo.join("nested"), repo.join("inside"))?;
    symlink(repo.join("missing.md"), root.path().join("link.md"))?;
    let input = paths(&[
        "outside/missing.md",
        "inside/missing.md",
        "../link.md",
        "inside/../missing.md",
    ]);
    assert_eq!(not_ignored(&input, &repo)?, paths(&["outside/missing.md"]));
    Ok(())
}

#[test]
fn ignore_errors_retain_the_source_and_path() -> TestResult {
    let root = TempDir::new()?;
    let ignore = root.path().join(".limae-ignore");
    fs::write(&ignore, b"\xff")?;
    let error = not_ignored(&paths(&["x.md"]), root.path())
        .err()
        .ok_or("missing error")?;
    assert!(
        matches!(&error, IgnoreError::Io { path, source } if path == &ignore && source.kind() == std::io::ErrorKind::InvalidData)
    );
    assert!(error.source().is_some());
    fs::write(&ignore, "*.md\r\n[z-a].md\r\n")?;
    assert!(
        matches!(not_ignored(&paths(&["x.md"]), root.path()), Err(IgnoreError::Pattern { path, line: 2, .. }) if path == ignore)
    );
    fs::write(&ignore, "*.md\n!\n")?;
    assert!(matches!(
        not_ignored(&paths(&["x.md"]), root.path()),
        Err(IgnoreError::Pattern { line: 2, .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn ignore_read_permission_failure_is_reported() -> TestResult {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new()?;
    let ignore = root.path().join(".limae-ignore");
    fs::write(&ignore, "*.md\n")?;
    fs::set_permissions(&ignore, fs::Permissions::from_mode(0o000))?;
    let result = not_ignored(&paths(&["x.md"]), root.path());
    fs::set_permissions(&ignore, fs::Permissions::from_mode(0o600))?;
    assert!(
        matches!(result, Err(IgnoreError::Io { path, source }) if path == ignore && source.kind() == std::io::ErrorKind::PermissionDenied)
    );
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn git_inherits_environment_routing_without_changing_the_caller() -> TestResult {
    if let Some(cwd) = std::env::var_os("LIMAE_TEST_ROUTING_CWD") {
        let cwd = PathBuf::from(cwd);
        let before = std::env::current_dir()?;
        let expected = std::env::var_os("LIMAE_TEST_ROUTING_EXPECTED").ok_or("expected name")?;
        assert_eq!(tracked_markdown(&cwd)?, vec![PathBuf::from(expected)]);
        assert_eq!(std::env::current_dir()?, before);
        return Ok(());
    }
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    for (root, name) in [(a.path(), "a.md"), (b.path(), "b.md")] {
        git(root, &["init", "-q"])?;
        fs::write(root.join(name), "ACME")?;
        git(root, &["add", name])?;
    }
    for (route, expected) in [(None, "a.md"), (Some(b.path().join(".git")), "b.md")] {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--exact",
                "files::git_inherits_environment_routing_without_changing_the_caller",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", std::env::var_os("PATH").ok_or("missing PATH")?)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("LIMAE_TEST_ROUTING_CWD", a.path())
            .env("LIMAE_TEST_ROUTING_EXPECTED", expected);
        if let Some(route) = route {
            command.env("GIT_DIR", route);
        }
        let output = command.output()?;
        assert!(
            output.status.success(),
            "isolated re-exec failed: {}\n--- child stdout ---\n{}\n--- child stderr ---\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
