//! Rule configuration discovery and resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;
use toml::Value;

use crate::text::is_cjk;

const CONFIG_FILENAME: &str = "limae.toml";
const PYPROJECT_FILENAME: &str = "pyproject.toml";

/// A rule identity from `spec/rules.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(u8);

impl RuleId {
    pub const ZH_TYPOGRAPHY_1: Self = Self(0);
    pub const ZH_TYPOGRAPHY_2: Self = Self(1);
    pub const ZH_TYPOGRAPHY_3: Self = Self(2);
    pub const ZH_TYPOGRAPHY_4: Self = Self(3);
    pub const ZH_TYPOGRAPHY_5: Self = Self(4);
    pub const ZH_TYPOGRAPHY_6: Self = Self(5);
    pub const ZH_TYPOGRAPHY_7: Self = Self(6);
    pub const ZH_TYPOGRAPHY_8: Self = Self(7);
    pub const ZH_TYPOGRAPHY_9: Self = Self(8);
    pub const ZH_TYPOGRAPHY_10: Self = Self(9);
    pub const ZH_TYPOGRAPHY_11: Self = Self(10);
    pub const ZH_TELL_1: Self = Self(11);
    pub const ZH_TELL_2: Self = Self(12);
    pub const ZH_TELL_3: Self = Self(13);
    pub const ZH_TELL_4: Self = Self(14);
    pub const EN_TELL_1: Self = Self(15);
    pub const EN_TELL_2: Self = Self(16);
    pub const EN_TELL_3: Self = Self(17);
    pub const ZH_TELL_5: Self = Self(18);
    pub const ZH_WORD_1: Self = Self(19);
    pub const ZH_WORD_2: Self = Self(20);

    /// Return the stable external rule id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        RULES[self.0 as usize].name
    }

    /// Return this rule's configuration metadata.
    #[must_use]
    pub const fn metadata(self) -> &'static RuleMetadata {
        &RULES[self.0 as usize]
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A finding's configured severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Whether a rule is available by default or only through the maturity switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maturity {
    Stable,
    Experimental,
}

/// Configuration-relevant properties of one known rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMetadata {
    pub id: RuleId,
    pub name: &'static str,
    pub default_enabled: bool,
    pub default_severity: Severity,
    pub maturity: Maturity,
}

const fn stable(id: RuleId, name: &'static str, default_enabled: bool) -> RuleMetadata {
    RuleMetadata {
        id,
        name,
        default_enabled,
        default_severity: Severity::Error,
        maturity: Maturity::Stable,
    }
}

const fn experimental(id: RuleId, name: &'static str) -> RuleMetadata {
    RuleMetadata {
        id,
        name,
        default_enabled: false,
        default_severity: Severity::Warning,
        maturity: Maturity::Experimental,
    }
}

/// All known rules in specification order; this is the sole Rust metadata table.
pub const RULES: [RuleMetadata; 21] = [
    stable(RuleId::ZH_TYPOGRAPHY_1, "zh-typography-1", true),
    stable(RuleId::ZH_TYPOGRAPHY_2, "zh-typography-2", true),
    stable(RuleId::ZH_TYPOGRAPHY_3, "zh-typography-3", true),
    stable(RuleId::ZH_TYPOGRAPHY_4, "zh-typography-4", true),
    stable(RuleId::ZH_TYPOGRAPHY_5, "zh-typography-5", true),
    stable(RuleId::ZH_TYPOGRAPHY_6, "zh-typography-6", true),
    stable(RuleId::ZH_TYPOGRAPHY_7, "zh-typography-7", true),
    stable(RuleId::ZH_TYPOGRAPHY_8, "zh-typography-8", true),
    stable(RuleId::ZH_TYPOGRAPHY_9, "zh-typography-9", false),
    stable(RuleId::ZH_TYPOGRAPHY_10, "zh-typography-10", true),
    stable(RuleId::ZH_TYPOGRAPHY_11, "zh-typography-11", true),
    experimental(RuleId::ZH_TELL_1, "zh-tell-1"),
    experimental(RuleId::ZH_TELL_2, "zh-tell-2"),
    experimental(RuleId::ZH_TELL_3, "zh-tell-3"),
    experimental(RuleId::ZH_TELL_4, "zh-tell-4"),
    experimental(RuleId::EN_TELL_1, "en-tell-1"),
    experimental(RuleId::EN_TELL_2, "en-tell-2"),
    experimental(RuleId::EN_TELL_3, "en-tell-3"),
    experimental(RuleId::ZH_TELL_5, "zh-tell-5"),
    experimental(RuleId::ZH_WORD_1, "zh-word-1"),
    experimental(RuleId::ZH_WORD_2, "zh-word-2"),
];

/// Raw values of the two repeatable CLI rule flags.
///
/// `Some` means the flag appeared and therefore replaces the configuration
/// file wholesale. A real empty-string argument remains one element here.
#[derive(Debug, Clone, Copy, Default)]
pub struct CliOverrides<'a> {
    pub disable: Option<&'a [String]>,
    pub enable: Option<&'a [String]>,
}

impl CliOverrides<'_> {
    fn is_present(self) -> bool {
        self.disable.is_some() || self.enable.is_some()
    }
}

/// The origin of a configuration validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigOrigin {
    File(PathBuf),
    CommandLine,
}

impl fmt::Display for ConfigOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => path.display().fmt(formatter),
            Self::CommandLine => formatter.write_str("command line"),
        }
    }
}

/// A configuration discovery, parsing, or validation failure.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot inspect configuration candidate {path}: {source}")]
    Inspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot read configuration {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot parse configuration {path} at {line}:{column}")]
    Parse {
        path: PathBuf,
        line: usize,
        column: usize,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("{origin}: `{key}` has the wrong type; expected {expected}")]
    InvalidType {
        origin: ConfigOrigin,
        key: &'static str,
        expected: &'static str,
    },
    #[error("{origin}: `{key}` contains an unknown rule id")]
    UnknownRule {
        origin: ConfigOrigin,
        key: &'static str,
    },
    #[error("{origin}: a rule id appears in both `disable` and `enable`")]
    ConflictingRule { origin: ConfigOrigin },
    #[error("{origin}: `enable` cannot name an experimental rule")]
    ExperimentalRuleEnabled { origin: ConfigOrigin },
    #[error("{origin}: `severity` contains a value other than `error` or `warning`")]
    InvalidSeverity { origin: ConfigOrigin },
    #[error("{origin}: `skip_zh_units` must contain only CJK characters")]
    InvalidSkipZhUnits { origin: ConfigOrigin },
}

/// The effective rule configuration for one run.
#[derive(Clone)]
pub struct ResolvedConfig {
    enabled: BTreeSet<RuleId>,
    severity: BTreeMap<RuleId, Severity>,
    skip_zh_units: String,
}

impl ResolvedConfig {
    /// Return whether a rule participates in this run.
    #[must_use]
    pub fn is_enabled(&self, rule: RuleId) -> bool {
        self.enabled.contains(&rule)
    }

    /// Iterate over enabled rules in specification order.
    pub fn enabled_rules(&self) -> impl Iterator<Item = RuleId> + '_ {
        self.enabled.iter().copied()
    }

    /// Return a rule's override or its specification default.
    #[must_use]
    pub fn severity(&self, rule: RuleId) -> Severity {
        self.severity
            .get(&rule)
            .copied()
            .unwrap_or(rule.metadata().default_severity)
    }

    /// Return the configured CJK unit exemptions for `zh-typography-5`.
    #[must_use]
    pub fn skip_zh_units(&self) -> &str {
        &self.skip_zh_units
    }
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            enabled: RULES
                .iter()
                .filter(|metadata| metadata.default_enabled)
                .map(|metadata| metadata.id)
                .collect(),
            severity: BTreeMap::new(),
            skip_zh_units: String::new(),
        }
    }
}

/// Resolve CLI rule flags or discover the nearest file configuration.
///
/// # Errors
/// Returns a [`ConfigError`] for candidate I/O failures, invalid TOML, or a
/// known key whose type or rule value violates `spec/rules.md`.
pub fn resolve(start: &Path, cli: CliOverrides<'_>) -> Result<ResolvedConfig, ConfigError> {
    if cli.is_present() {
        return resolve_cli(cli);
    }

    let Some((path, value)) = find_config(start)? else {
        return Ok(ResolvedConfig::default());
    };
    let origin = ConfigOrigin::File(path);
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key: "tool.limae",
        expected: "a table",
    })?;
    resolve_table(table, origin)
}

fn resolve_cli(cli: CliOverrides<'_>) -> Result<ResolvedConfig, ConfigError> {
    let origin = ConfigOrigin::CommandLine;
    let disabled = cli_rule_ids(cli.disable, "--disable", &origin)?;
    let enabled = cli_rule_ids(cli.enable, "--enable", &origin)?;
    validate_selection(&disabled, &enabled, &origin)?;
    let mut config = ResolvedConfig::default();
    apply_selection(&mut config.enabled, disabled, enabled);
    Ok(config)
}

fn resolve_table(table: &toml::Table, origin: ConfigOrigin) -> Result<ResolvedConfig, ConfigError> {
    let disabled = table_rule_ids(table, "disable", &origin)?;
    let enabled = table_rule_ids(table, "enable", &origin)?;
    validate_selection(&disabled, &enabled, &origin)?;

    let experimental = optional_bool(table, "enable_experimental", &origin)?;
    let mut config = ResolvedConfig::default();
    if experimental {
        config.enabled.extend(
            RULES
                .iter()
                .filter(|metadata| metadata.maturity == Maturity::Experimental)
                .map(|metadata| metadata.id),
        );
    }
    apply_selection(&mut config.enabled, disabled, enabled);
    config.skip_zh_units = skip_zh_units(table, &origin)?.to_owned();
    config.severity = severity_overrides(table, &origin)?;
    Ok(config)
}

fn find_config(start: &Path) -> Result<Option<(PathBuf, Value)>, ConfigError> {
    for directory in start.ancestors() {
        let standalone = directory.join(CONFIG_FILENAME);
        if is_file(&standalone)? {
            return load_toml(&standalone).map(|value| Some((standalone, value)));
        }

        let pyproject = directory.join(PYPROJECT_FILENAME);
        if is_file(&pyproject)? {
            let value = load_toml(&pyproject)?;
            if let Some(limae) = value
                .get("tool")
                .and_then(Value::as_table)
                .and_then(|tool| tool.get("limae"))
            {
                return Ok(Some((pyproject, limae.clone())));
            }
        }

        if path_exists(&directory.join(".git"))? {
            break;
        }
    }
    Ok(None)
}

fn is_file(path: &Path) -> Result<bool, ConfigError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(ConfigError::Inspect {
            path: path.to_owned(),
            source,
        }),
    }
}

fn path_exists(path: &Path) -> Result<bool, ConfigError> {
    match fs::metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(ConfigError::Inspect {
            path: path.to_owned(),
            source,
        }),
    }
}

fn load_toml(path: &Path) -> Result<Value, ConfigError> {
    let input = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    toml::from_str(&input).map_err(|source| {
        let (line, column) = source
            .span()
            .map_or((1, 1), |span| line_column(&input, span.start));
        ConfigError::Parse {
            path: path.to_owned(),
            line,
            column,
            source: Box::new(source),
        }
    })
}

fn line_column(input: &str, byte: usize) -> (usize, usize) {
    let prefix = input.get(..byte).unwrap_or(input);
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = prefix[column_start..].chars().count() + 1;
    (line, column)
}

fn rule_id(name: &str) -> Option<RuleId> {
    RULES
        .iter()
        .find(|metadata| metadata.name == name)
        .map(|metadata| metadata.id)
}

fn cli_rule_ids(
    values: Option<&[String]>,
    key: &'static str,
    origin: &ConfigOrigin,
) -> Result<BTreeSet<RuleId>, ConfigError> {
    let mut rules = BTreeSet::new();
    for value in values.into_iter().flatten() {
        for name in value
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let id = rule_id(name).ok_or_else(|| ConfigError::UnknownRule {
                origin: origin.clone(),
                key,
            })?;
            rules.insert(id);
        }
    }
    Ok(rules)
}

fn table_rule_ids(
    table: &toml::Table,
    key: &'static str,
    origin: &ConfigOrigin,
) -> Result<BTreeSet<RuleId>, ConfigError> {
    let Some(value) = table.get(key) else {
        return Ok(BTreeSet::new());
    };
    let values = value.as_array().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key,
        expected: "a list of rule ids",
    })?;
    let mut rules = BTreeSet::new();
    for value in values {
        let name = value.as_str().ok_or_else(|| ConfigError::InvalidType {
            origin: origin.clone(),
            key,
            expected: "a list of rule ids",
        })?;
        let id = rule_id(name).ok_or_else(|| ConfigError::UnknownRule {
            origin: origin.clone(),
            key,
        })?;
        rules.insert(id);
    }
    Ok(rules)
}

fn validate_selection(
    disabled: &BTreeSet<RuleId>,
    enabled: &BTreeSet<RuleId>,
    origin: &ConfigOrigin,
) -> Result<(), ConfigError> {
    if !disabled.is_disjoint(enabled) {
        return Err(ConfigError::ConflictingRule {
            origin: origin.clone(),
        });
    }
    if enabled
        .iter()
        .any(|rule| rule.metadata().maturity == Maturity::Experimental)
    {
        return Err(ConfigError::ExperimentalRuleEnabled {
            origin: origin.clone(),
        });
    }
    Ok(())
}

fn apply_selection(
    active: &mut BTreeSet<RuleId>,
    disabled: BTreeSet<RuleId>,
    enabled: BTreeSet<RuleId>,
) {
    active.extend(enabled);
    active.retain(|rule| !disabled.contains(rule));
}

fn optional_bool(
    table: &toml::Table,
    key: &'static str,
    origin: &ConfigOrigin,
) -> Result<bool, ConfigError> {
    table.get(key).map_or(Ok(false), |value| {
        value.as_bool().ok_or_else(|| ConfigError::InvalidType {
            origin: origin.clone(),
            key,
            expected: "a boolean",
        })
    })
}

fn skip_zh_units<'a>(
    table: &'a toml::Table,
    origin: &ConfigOrigin,
) -> Result<&'a str, ConfigError> {
    let Some(value) = table.get("skip_zh_units") else {
        return Ok("");
    };
    let units = value.as_str().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key: "skip_zh_units",
        expected: "a string of CJK characters",
    })?;
    if units.chars().all(is_cjk) {
        Ok(units)
    } else {
        Err(ConfigError::InvalidSkipZhUnits {
            origin: origin.clone(),
        })
    }
}

fn severity_overrides(
    table: &toml::Table,
    origin: &ConfigOrigin,
) -> Result<BTreeMap<RuleId, Severity>, ConfigError> {
    let Some(value) = table.get("severity") else {
        return Ok(BTreeMap::new());
    };
    let values = value.as_table().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key: "severity",
        expected: "a table of rule id = severity",
    })?;
    let mut overrides = BTreeMap::new();
    for (name, value) in values {
        let id = rule_id(name).ok_or_else(|| ConfigError::UnknownRule {
            origin: origin.clone(),
            key: "severity",
        })?;
        let severity = match value.as_str() {
            Some("error") => Severity::Error,
            Some("warning") => Severity::Warning,
            _ => {
                return Err(ConfigError::InvalidSeverity {
                    origin: origin.clone(),
                });
            }
        };
        overrides.insert(id, severity);
    }
    Ok(overrides)
}
