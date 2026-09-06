use std::error::Error;
use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use limae::config::{ResolvedConfig, RuleId, Severity, resolve};
use limae::files::{FileError, FileText, FixStatus, fix_file};
use limae::pipeline::Pipeline;

type TestResult = Result<(), Box<dyn Error>>;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-fileio-{}-{}",
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

#[test]
fn clean_universal_newlines_preserve_bytes_and_mtime() -> TestResult {
    let root = TempDir::new()?;
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    let old = UNIX_EPOCH + Duration::from_secs(946_684_800);

    for (index, (bytes, logical)) in [
        (&b"ACME\r\nFoo\r\n"[..], "ACME\nFoo\n"),
        (&b"ACME\rFoo\r"[..], "ACME\nFoo\n"),
        (
            &b"ACME without final newline"[..],
            "ACME without final newline",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let path = root.path().join(format!("clean-{index}.md"));
        fs::write(&path, bytes)?;
        File::open(&path)?.set_times(FileTimes::new().set_modified(old))?;
        let before = fs::metadata(&path)?.modified()?;
        let source = FileText::read(&path)?;
        assert_eq!(source.as_str(), logical);
        assert!(source.check(&pipeline, &config)?.is_empty());
        assert_eq!(fix_file(&path, &pipeline, &config)?, FixStatus::Unchanged);
        assert_eq!(fs::read(&path)?, bytes);
        assert_eq!(fs::metadata(&path)?.modified()?, before);
    }
    Ok(())
}

#[test]
fn changed_files_write_lf_without_adding_a_final_newline() -> TestResult {
    let root = TempDir::new()?;
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();
    for (name, source, logical, fixed) in [
        ("crlf.md", "中A\r\n文B", "中A\n文B", "中 A\n文 B"),
        ("cr.md", "中A\r文B\r", "中A\n文B\n", "中 A\n文 B\n"),
        (
            "unicode.md",
            "中A\u{2028}文B",
            "中A\u{2028}文B",
            "中 A\u{2028}文 B",
        ),
    ] {
        let path = root.path().join(name);
        fs::write(&path, source)?;
        let before = FileText::read(&path)?;
        assert_eq!(before.as_str(), logical);
        assert!(!before.check(&pipeline, &config)?.is_empty());
        assert_eq!(fix_file(&path, &pipeline, &config)?, FixStatus::Written);
        assert_eq!(fs::read_to_string(&path)?, fixed);
        let after = FileText::read(&path)?;
        assert!(after.check(&pipeline, &config)?.is_empty());
    }
    Ok(())
}

#[test]
fn configured_warnings_are_fixed_and_unfixable_errors_remain() -> TestResult {
    let root = TempDir::new()?;
    fs::write(
        root.path().join("limae.toml"),
        concat!(
            "enable_experimental = true\n",
            "[severity]\n",
            "zh-typography-4 = 'warning'\n",
            "zh-tell-1 = 'error'\n",
        ),
    )?;
    let config = resolve(root.path(), Default::default())?;
    let pipeline = Pipeline::new()?;
    let path = root.path().join("graded.md");
    fs::write(&path, "综上所述中A")?;

    let before = FileText::read(&path)?;
    let findings = before.check(&pipeline, &config)?;
    assert_eq!(
        findings
            .iter()
            .map(|finding| (finding.rule, config.severity(finding.rule)))
            .collect::<Vec<_>>(),
        [
            (RuleId::ZH_TYPOGRAPHY_4, Severity::Warning),
            (RuleId::ZH_TELL_1, Severity::Error),
        ]
    );
    assert_eq!(fix_file(&path, &pipeline, &config)?, FixStatus::Written);
    assert_eq!(fs::read_to_string(&path)?, "综上所述中 A");
    let after = FileText::read(&path)?;
    let findings = after.check(&pipeline, &config)?;
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule, RuleId::ZH_TELL_1);
    assert_eq!(config.severity(findings[0].rule), Severity::Error);
    Ok(())
}

#[cfg(unix)]
#[test]
fn writes_follow_symlinks_and_preserve_linked_inode_and_mode() -> TestResult {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    let root = TempDir::new()?;
    let target = root.path().join("target.md");
    let alias = root.path().join("alias.md");
    let link = root.path().join("link.md");
    fs::write(&target, "中A")?;
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640))?;
    fs::hard_link(&target, &alias)?;
    symlink(&target, &link)?;
    let before = fs::metadata(&target)?;

    assert_eq!(
        fix_file(&link, &Pipeline::new()?, &ResolvedConfig::default())?,
        FixStatus::Written
    );

    let after = fs::metadata(&target)?;
    assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
    assert_eq!(after.ino(), before.ino());
    assert_eq!(after.mode(), before.mode());
    assert_eq!(fs::metadata(&alias)?.ino(), before.ino());
    assert_eq!(fs::read_to_string(&target)?, "中 A");
    assert_eq!(fs::read_to_string(&alias)?, "中 A");
    Ok(())
}

#[test]
fn earlier_writes_survive_later_directive_and_read_errors() -> TestResult {
    let pipeline = Pipeline::new()?;
    let config = ResolvedConfig::default();

    let directive_root = TempDir::new()?;
    let first = directive_root.path().join("first.md");
    let second = directive_root.path().join("second.md");
    fs::write(&first, "中A")?;
    fs::write(
        &second,
        "ACME-CONTEXT-MARKER\n<!-- limae-disable unknown -->\n中A",
    )?;
    assert_eq!(fix_file(&first, &pipeline, &config)?, FixStatus::Written);
    let error = fix_file(&second, &pipeline, &config)
        .err()
        .ok_or("missing directive error")?;
    assert!(
        matches!(&error, FileError::Directive { path, source } if path == &second && source.line() == 2)
    );
    assert!(!format!("{error:?} {error}").contains("ACME-CONTEXT-MARKER"));
    assert_eq!(fs::read_to_string(&first)?, "中 A");
    assert_eq!(
        fs::read_to_string(&second)?,
        "ACME-CONTEXT-MARKER\n<!-- limae-disable unknown -->\n中A"
    );

    let read_root = TempDir::new()?;
    let first = read_root.path().join("first.md");
    let missing = read_root.path().join("missing.md");
    fs::write(&first, "文B")?;
    assert_eq!(fix_file(&first, &pipeline, &config)?, FixStatus::Written);
    assert!(matches!(
        fix_file(&missing, &pipeline, &config),
        Err(FileError::Read { path, source })
            if path == missing && source.kind() == std::io::ErrorKind::NotFound
    ));
    assert_eq!(fs::read_to_string(&first)?, "文 B");
    Ok(())
}

#[test]
fn decode_and_filesystem_failures_keep_their_paths_and_sources() -> TestResult {
    let root = TempDir::new()?;
    let invalid = root.path().join("invalid.md");
    fs::write(&invalid, [0xff, 0xfe])?;
    let error = FileText::read(&invalid)
        .err()
        .ok_or("missing UTF-8 error")?;
    assert!(
        matches!(&error, FileError::Utf8 { path, source } if path == &invalid && source.valid_up_to() == 0)
    );
    assert!(error.source().is_some());

    let missing = root.path().join("missing.md");
    assert!(matches!(
        FileText::read(&missing),
        Err(FileError::Read { path, source })
            if path == missing && source.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(matches!(
        FileText::read(root.path()),
        Err(FileError::Read { path, .. }) if path == root.path()
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn permission_errors_are_asserted_only_when_the_identity_cannot_bypass_them() -> TestResult {
    use std::os::unix::fs::PermissionsExt;

    if rustix::process::geteuid().is_root() {
        return Ok(());
    }

    let root = TempDir::new()?;
    let unreadable = root.path().join("unreadable.md");
    fs::write(&unreadable, "中A")?;
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;
    let read_result = FileText::read(&unreadable);
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600))?;
    assert!(matches!(
        read_result,
        Err(FileError::Read { path, source })
            if path == unreadable && source.kind() == std::io::ErrorKind::PermissionDenied
    ));

    let unwritable = root.path().join("unwritable.md");
    fs::write(&unwritable, "中A")?;
    fs::set_permissions(&unwritable, fs::Permissions::from_mode(0o400))?;
    let write_result = fix_file(&unwritable, &Pipeline::new()?, &ResolvedConfig::default());
    fs::set_permissions(&unwritable, fs::Permissions::from_mode(0o600))?;
    assert!(matches!(
        write_result,
        Err(FileError::Write { path, source })
            if path == unwritable && source.kind() == std::io::ErrorKind::PermissionDenied
    ));
    assert_eq!(fs::read_to_string(&unwritable)?, "中A");
    Ok(())
}
