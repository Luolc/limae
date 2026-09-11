use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use limae::files::{IgnoreError, find_ignore, not_ignored, walk_markdown};

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

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

/// `.limae-ignore` patterns are rooted at the ignore file, not at the cwd.
///
/// Both arms walk the same subdirectory and differ only in how the negation is
/// written; if patterns were read relative to the cwd the first arm would keep
/// `sub/a.md` too, and the pair would stop telling the two readings apart.
#[test]
fn ignore_patterns_stay_rooted_at_the_ignore_file_under_a_nested_cwd() -> TestResult {
    let a = TempDir::new()?;
    let b = TempDir::new()?;
    for root in [a.path(), b.path()] {
        fs::create_dir_all(root.join("docs/sub"))?;
        for name in ["z.md", "a space.md", "docs/b.md", "docs/sub/a.md", "x.txt"] {
            fs::write(root.join(name), "ACME")?;
        }
        let docs = root.join("docs");
        assert_eq!(walk_markdown(&docs)?, paths(&["b.md", "sub/a.md"]));
        fs::write(root.join(".limae-ignore"), "*.md\n!sub/a.md\n")?;
        assert_eq!(
            not_ignored(&walk_markdown(&docs)?, &docs)?,
            Vec::<PathBuf>::new()
        );
        fs::write(root.join(".limae-ignore"), "*.md\n!docs/sub/a.md\n")?;
        assert_eq!(
            not_ignored(&walk_markdown(&docs)?, &docs)?,
            paths(&["sub/a.md"])
        );
    }
    // An ignore file with no patterns ignores nothing, which is what keeps the
    // arm above from passing for want of any matching at all.
    fs::write(b.path().join(".limae-ignore"), "")?;
    assert_eq!(
        not_ignored(&paths(&["z.md", "a space.md"]), b.path())?,
        paths(&["z.md", "a space.md"])
    );
    assert_eq!(
        not_ignored(&paths(&["z.md", "a space.md"]), a.path())?,
        Vec::<PathBuf>::new()
    );
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
    assert_matches!(&error, IgnoreError::Io { path, source } if path == &ignore && source.kind() == std::io::ErrorKind::InvalidData);
    assert!(error.source().is_some());
    fs::write(&ignore, "*.md\r\n[z-a].md\r\n")?;
    assert_matches!(not_ignored(&paths(&["x.md"]), root.path()), Err(IgnoreError::Pattern { path, line: 2, .. }) if path == &ignore);
    fs::write(&ignore, "*.md\n!\n")?;
    assert_matches!(
        not_ignored(&paths(&["x.md"]), root.path()),
        Err(IgnoreError::Pattern { line: 2, .. })
    );
    // A bare `!` is one shape of unusable pattern, not the shape. A line that
    // ends in an escape has nothing to escape, and naming only the one we
    // happened to think of leaves every other one silently matching nothing.
    fs::write(&ignore, "*.md\ntrailing\\\n")?;
    assert_matches!(
        not_ignored(&paths(&["x.md"]), root.path()),
        Err(IgnoreError::Pattern { line: 2, .. })
    );
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
    assert_matches!(result, Err(IgnoreError::Io { path, source }) if path == &ignore && source.kind() == std::io::ErrorKind::PermissionDenied);
    Ok(())
}

/// What the walk selects, in what order, and what it refuses to guess about.
///
/// The last arm is the one with a control: an unreadable directory has to be an
/// error, because skipping it would make "nothing to report here" and "never
/// looked here" the same output. The readable sibling is what shows the error
/// is about that directory and not about walking at all.
#[test]
fn the_walk_sorts_relative_markdown_and_refuses_a_directory_it_cannot_read() -> TestResult {
    let root = TempDir::new()?;
    fs::create_dir_all(root.path().join("docs/sub"))?;
    for name in ["z.md", "a space.md", "docs/b.md", "docs/sub/a.md", "x.txt"] {
        fs::write(root.path().join(name), "ACME")?;
    }
    // A directory whose own name ends in `.md` is not a file to check, and
    // neither is a link to one: "cannot tell what this is" is the only reason
    // to keep a link, and here we can tell.
    fs::create_dir(root.path().join("dir.md"))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink("docs", root.path().join("linked.md"))?;
    assert_eq!(
        walk_markdown(root.path())?,
        paths(&["a space.md", "docs/b.md", "docs/sub/a.md", "z.md"])
    );
    assert_eq!(
        walk_markdown(&root.path().join("docs"))?,
        paths(&["b.md", "sub/a.md"])
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = root.path().join("locked");
        fs::create_dir(&locked)?;
        fs::write(locked.join("hidden.md"), "ACME")?;
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000))?;
        let result = walk_markdown(root.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755))?;
        assert_matches!(result, Err(limae::files::WalkError(_)));
        assert_eq!(
            walk_markdown(root.path())?,
            paths(&[
                "a space.md",
                "docs/b.md",
                "docs/sub/a.md",
                "locked/hidden.md",
                "z.md"
            ])
        );
    }
    Ok(())
}
