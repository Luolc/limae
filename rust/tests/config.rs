use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use limae::config::{CliOverrides, ConfigError, Maturity, RULES, RuleId, Severity, resolve};

type TestResult = Result<(), Box<dyn Error>>;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-config-{}-{label}-{}",
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

fn write_config(directory: &Path, contents: &str) -> Result<(), std::io::Error> {
    fs::write(directory.join("limae.toml"), contents)
}

#[test]
fn rule_metadata_matches_the_specification() {
    let expected = [
        (
            RuleId::ZH_TYPOGRAPHY_1,
            "zh-typography-1",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_2,
            "zh-typography-2",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_3,
            "zh-typography-3",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_4,
            "zh-typography-4",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_5,
            "zh-typography-5",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_6,
            "zh-typography-6",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_7,
            "zh-typography-7",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_8,
            "zh-typography-8",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_9,
            "zh-typography-9",
            false,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_10,
            "zh-typography-10",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TYPOGRAPHY_11,
            "zh-typography-11",
            true,
            Severity::Error,
            Maturity::Stable,
        ),
        (
            RuleId::ZH_TELL_1,
            "zh-tell-1",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_TELL_2,
            "zh-tell-2",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_TELL_3,
            "zh-tell-3",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_TELL_4,
            "zh-tell-4",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::EN_TELL_1,
            "en-tell-1",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::EN_TELL_2,
            "en-tell-2",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::EN_TELL_3,
            "en-tell-3",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_TELL_5,
            "zh-tell-5",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_WORD_1,
            "zh-word-1",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
        (
            RuleId::ZH_WORD_2,
            "zh-word-2",
            false,
            Severity::Warning,
            Maturity::Experimental,
        ),
    ];
    assert_eq!(RULES.len(), expected.len());
    for (metadata, (id, name, default_enabled, default_severity, maturity)) in
        RULES.iter().zip(expected)
    {
        assert_eq!(id.as_str(), name);
        assert_eq!(id.metadata(), metadata);
        assert_eq!(metadata.name, name);
        assert_eq!(metadata.default_enabled, default_enabled);
        assert_eq!(metadata.default_severity, default_severity);
        assert_eq!(metadata.maturity, maturity);
    }
}

#[test]
fn shared_fixture_configs_resolve_business_settings() -> TestResult {
    let temp = TempDir::new("fixtures")?;
    let selections = temp.path().join("selections");
    let experimental = temp.path().join("experimental");
    let severity = temp.path().join("severity");
    let units = temp.path().join("units");
    for directory in [&selections, &experimental, &severity, &units] {
        fs::create_dir(directory)?;
    }
    write_config(
        &selections,
        include_str!("../../spec/fixtures/config-enable-with-disable.conf"),
    )?;
    write_config(
        &experimental,
        include_str!("../../spec/fixtures/config-enable-experimental.conf"),
    )?;
    write_config(
        &severity,
        include_str!("../../spec/fixtures/config-severity-warning.conf"),
    )?;
    write_config(
        &units,
        include_str!("../../spec/fixtures/config-skip-zh-units.conf"),
    )?;

    let selected = resolve(&selections, CliOverrides::default())?;
    assert!(!selected.is_enabled(RuleId::ZH_TYPOGRAPHY_3));
    assert!(selected.is_enabled(RuleId::ZH_TYPOGRAPHY_9));
    let all_experimental = resolve(&experimental, CliOverrides::default())?;
    assert!(all_experimental.is_enabled(RuleId::ZH_TELL_1));
    assert!(all_experimental.is_enabled(RuleId::EN_TELL_3));
    assert!(all_experimental.is_enabled(RuleId::ZH_WORD_2));
    let downgraded = resolve(&severity, CliOverrides::default())?;
    assert_eq!(
        downgraded.severity(RuleId::ZH_TYPOGRAPHY_1),
        Severity::Warning
    );
    assert_eq!(
        downgraded.severity(RuleId::ZH_TYPOGRAPHY_2),
        Severity::Error
    );
    let skipped = resolve(&units, CliOverrides::default())?;
    assert_eq!(skipped.skip_zh_units(), "年月日天号时分秒");
    Ok(())
}

#[test]
fn standalone_wins_and_nearest_source_replaces_the_parent_wholesale() -> TestResult {
    let temp = TempDir::new("precedence")?;
    write_config(temp.path(), "skip_zh_units = \"年\"\n")?;
    let child = temp.path().join("child");
    fs::create_dir(&child)?;
    fs::write(
        child.join("pyproject.toml"),
        "[tool.limae]\nenable_experimental = true\n",
    )?;
    let nested = child.join("nested");
    fs::create_dir(&nested)?;

    let nearest = resolve(&nested, CliOverrides::default())?;
    assert!(nearest.is_enabled(RuleId::ZH_TELL_1));
    assert_eq!(nearest.skip_zh_units(), "");

    write_config(&child, "disable = [\"zh-typography-1\"]\n")?;
    let standalone = resolve(&child, CliOverrides::default())?;
    assert!(!standalone.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    assert!(!standalone.is_enabled(RuleId::ZH_TELL_1));
    Ok(())
}

#[test]
fn pyproject_without_tool_table_continues_upward() -> TestResult {
    let temp = TempDir::new("pyproject-skip")?;
    write_config(temp.path(), "disable = [\"zh-typography-1\"]\n")?;
    let child = temp.path().join("child");
    fs::create_dir(&child)?;
    fs::write(child.join("pyproject.toml"), "[project]\nname = \"ACME\"\n")?;

    let config = resolve(&child, CliOverrides::default())?;
    assert!(!config.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    Ok(())
}

#[test]
fn git_entry_stops_discovery_for_a_file_or_directory() -> TestResult {
    let temp = TempDir::new("git-stop")?;
    write_config(temp.path(), "disable = [\"zh-typography-1\"]\n")?;
    let stopped_by_directory = temp.path().join("directory-repo");
    let stopped_by_file = temp.path().join("file-repo");
    let walking = temp.path().join("walking");
    for directory in [&stopped_by_directory, &stopped_by_file, &walking] {
        fs::create_dir(directory)?;
    }
    fs::create_dir(stopped_by_directory.join(".git"))?;
    fs::write(stopped_by_file.join(".git"), "gitdir: synthetic\n")?;

    assert!(
        resolve(&stopped_by_directory, CliOverrides::default())?
            .is_enabled(RuleId::ZH_TYPOGRAPHY_1)
    );
    assert!(
        resolve(&stopped_by_file, CliOverrides::default())?.is_enabled(RuleId::ZH_TYPOGRAPHY_1)
    );
    assert!(!resolve(&walking, CliOverrides::default())?.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    Ok(())
}

#[test]
fn a_repository_root_configuration_is_still_used() -> TestResult {
    let temp = TempDir::new("git-root-config")?;
    write_config(temp.path(), "disable = [\"zh-typography-1\"]\n")?;
    fs::create_dir(temp.path().join(".git"))?;

    let config = resolve(temp.path(), CliOverrides::default())?;
    assert!(!config.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    Ok(())
}

#[test]
fn any_cli_flag_wholly_replaces_the_file_even_when_its_value_is_empty() -> TestResult {
    let temp = TempDir::new("cli-override")?;
    write_config(
        temp.path(),
        "disable = [\"zh-typography-1\"]\n\
         enable_experimental = true\n\
         skip_zh_units = \"年\"\n\
         severity = { zh-typography-2 = \"warning\" }\n",
    )?;
    let from_file = resolve(temp.path(), CliOverrides::default())?;
    assert!(!from_file.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    assert!(from_file.is_enabled(RuleId::ZH_TELL_1));

    let empty = vec![String::new()];
    let from_cli = resolve(
        temp.path(),
        CliOverrides {
            disable: Some(&empty),
            enable: None,
        },
    )?;
    assert!(from_cli.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    assert!(!from_cli.is_enabled(RuleId::ZH_TELL_1));
    assert_eq!(from_cli.skip_zh_units(), "");
    assert_eq!(from_cli.severity(RuleId::ZH_TYPOGRAPHY_2), Severity::Error);

    let padded = vec![
        " zh-typography-1 ".to_owned(),
        "\u{1c}zh-typography-2\u{1f}".to_owned(),
    ];
    let selected = resolve(
        temp.path(),
        CliOverrides {
            disable: Some(&padded),
            enable: None,
        },
    )?;
    assert!(!selected.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    assert!(!selected.is_enabled(RuleId::ZH_TYPOGRAPHY_2));
    assert!(selected.is_enabled(RuleId::ZH_TYPOGRAPHY_3));
    Ok(())
}

#[test]
fn empty_cli_flag_skips_a_malformed_file_but_absent_flags_do_not() -> TestResult {
    let temp = TempDir::new("cli-skips-file")?;
    write_config(temp.path(), "[broken\n")?;
    let empty = vec![String::new()];
    let config = resolve(
        temp.path(),
        CliOverrides {
            disable: None,
            enable: Some(&empty),
        },
    )?;
    assert!(config.is_enabled(RuleId::ZH_TYPOGRAPHY_1));
    assert_matches!(
        resolve(temp.path(), CliOverrides::default()).as_ref().err(),
        Some(ConfigError::Parse { .. })
    );
    Ok(())
}

#[test]
fn selection_validation_rejects_unknown_conflicting_and_experimental_ids() -> TestResult {
    let temp = TempDir::new("selection-errors")?;
    let cases = [
        (
            "unknown",
            "disable = [\"ACME_SYNTHETIC_MARKER\"]\n",
            "unknown",
            "unknown rule id",
        ),
        (
            "conflict",
            "disable = [\"zh-typography-9\"]\nenable = [\"zh-typography-9\"]\n",
            "conflict",
            "in both",
        ),
        (
            "experimental",
            "enable = [\"zh-tell-1\"]\n",
            "experimental",
            "cannot be enabled one by one",
        ),
    ];
    for (directory_name, contents, category, message) in cases {
        let directory = temp.path().join(directory_name);
        fs::create_dir(&directory)?;
        write_config(&directory, contents)?;
        let error = resolve(&directory, CliOverrides::default());
        let actual = error.as_ref().err();
        assert!(
            matches!(
                (actual, category),
                (Some(ConfigError::UnknownRule { .. }), "unknown")
                    | (Some(ConfigError::ConflictingRule { .. }), "conflict")
                    | (
                        Some(ConfigError::ExperimentalRuleEnabled { .. }),
                        "experimental"
                    )
            ),
            "wrong error category for {directory_name}: {actual:?}"
        );
        let Err(error) = error else {
            return Err("expected a selection error".into());
        };
        assert!(error.to_string().contains(message));
        if category == "experimental" {
            assert!(error.to_string().contains("`enable`"));
        }
        if category == "unknown" {
            assert!(!error.to_string().contains("ACME_SYNTHETIC_MARKER"));
            assert!(!format!("{error:?}").contains("ACME_SYNTHETIC_MARKER"));
        }
    }
    Ok(())
}

#[test]
fn cli_experimental_selection_names_the_cli_key() -> TestResult {
    let enabled = vec!["zh-tell-1".to_owned()];
    let error = resolve(
        Path::new("."),
        CliOverrides {
            disable: None,
            enable: Some(&enabled),
        },
    );
    let Err(ConfigError::ExperimentalRuleEnabled { key, .. }) = error else {
        return Err("expected an experimental selection error".into());
    };
    assert_eq!(key, "--enable");
    Ok(())
}

#[test]
fn known_key_types_severity_and_unit_range_are_validated() -> TestResult {
    let temp = TempDir::new("value-errors")?;
    let cases = [
        (
            "list",
            "disable = \"zh-typography-1\"\n",
            "type",
            "must be a list of rule ids",
        ),
        (
            "bool",
            "enable_experimental = \"true\"\n",
            "type",
            "must be a boolean",
        ),
        (
            "severity-table",
            "severity = \"warning\"\n",
            "type",
            "must be a table of rule id = severity",
        ),
        (
            "severity-value",
            "severity = { zh-typography-1 = \"fatal\" }\n",
            "severity",
            "must be 'error' or 'warning'",
        ),
        (
            "severity-rule",
            "severity = { R99 = \"error\" }\n",
            "unknown",
            "unknown rule id",
        ),
        (
            "units-type",
            "skip_zh_units = [\"年\"]\n",
            "type",
            "must be a string of CJK characters",
        ),
        (
            "units-range",
            "skip_zh_units = \"年 月\"\n",
            "units",
            "must be a string of CJK characters",
        ),
    ];
    for (directory_name, contents, category, message) in cases {
        let directory = temp.path().join(directory_name);
        fs::create_dir(&directory)?;
        write_config(&directory, contents)?;
        let error = resolve(&directory, CliOverrides::default());
        let actual = error.as_ref().err();
        assert!(
            matches!(
                (actual, category),
                (Some(ConfigError::InvalidType { .. }), "type")
                    | (Some(ConfigError::InvalidSeverity { .. }), "severity")
                    | (Some(ConfigError::InvalidSkipZhUnits { .. }), "units")
                    | (Some(ConfigError::UnknownRule { .. }), "unknown")
            ),
            "wrong error category for {directory_name}: {actual:?}"
        );
        let Err(error) = error else {
            return Err("expected a configuration value error".into());
        };
        assert!(error.to_string().contains(message));
    }
    Ok(())
}

#[test]
fn unconsumed_existing_keys_are_accepted_without_exposing_their_values() -> TestResult {
    let temp = TempDir::new("unconsumed")?;
    write_config(
        temp.path(),
        "quote_style = \"curly\"\n\
         [polish]\n\
         engine = \"custom\"\n\
         command = [\"synthetic-engine\", \"{prompt}\"]\n",
    )?;

    let config = resolve(temp.path(), CliOverrides::default())?;
    assert_eq!(config.enabled_rules().count(), 10);
    Ok(())
}

#[test]
fn pyproject_tool_table_must_be_a_table() -> TestResult {
    let temp = TempDir::new("tool-table-type")?;
    fs::write(
        temp.path().join("pyproject.toml"),
        "[tool]\nlimae = \"not-a-table\"\n",
    )?;

    assert_matches!(
        resolve(temp.path(), CliOverrides::default()).as_ref().err(),
        Some(ConfigError::InvalidType {
            key: "tool.limae",
            ..
        })
    );
    Ok(())
}

#[test]
fn malformed_pyproject_stops_before_a_valid_parent_and_keeps_source_details() -> TestResult {
    let temp = TempDir::new("parse-details")?;
    write_config(temp.path(), "disable = [\"zh-typography-1\"]\n")?;
    let child = temp.path().join("child");
    fs::create_dir(&child)?;
    fs::write(
        child.join("pyproject.toml"),
        "[project]\nname = \"ACME\"\n[polish]\ncommand = [\"synthetic-command-value\"\n",
    )?;

    let Err(error) = resolve(&child, CliOverrides::default()) else {
        return Err("expected a TOML parse error".into());
    };
    assert!(Error::source(&error).is_some());
    let diagnostic = error.to_string();
    let debug_diagnostic = format!("{error:?}");
    let source_diagnostic = Error::source(&error)
        .ok_or("expected a TOML parser source")?
        .to_string();
    assert!(!diagnostic.contains("synthetic-command-value"));
    assert!(!debug_diagnostic.contains("synthetic-command-value"));
    assert!(!source_diagnostic.contains("synthetic-command-value"));
    assert!(!source_diagnostic.is_empty());
    let ConfigError::Parse {
        path,
        line,
        column,
        source: _,
    } = error
    else {
        return Err("expected a TOML parse error".into());
    };
    assert_eq!(path, child.join("pyproject.toml"));
    assert_eq!(line, 4);
    assert!(column > 0);
    Ok(())
}

#[test]
fn unreadable_text_is_a_read_error_instead_of_default_configuration() -> TestResult {
    let temp = TempDir::new("read-error")?;
    fs::write(temp.path().join("limae.toml"), [0xff])?;

    let Err(error) = resolve(temp.path(), CliOverrides::default()) else {
        return Err("expected a configuration read error".into());
    };
    assert_matches!(&error, ConfigError::Read { .. });
    assert!(Error::source(&error).is_some());
    Ok(())
}
