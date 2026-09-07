//! The `[polish]` table and the engine precedence chain (ADR-0008 section 三).
//!
//! The table is found the way the rule keys are, by the same upward walk and
//! out of the same two carriers: a top-level `[polish]` in `limae.toml`, or
//! `[tool.limae.polish]` in a `pyproject.toml`. The walk itself lives in
//! [`crate::config`] and is reused rather than repeated — two walks would
//! eventually disagree about which file is the nearest one.
//!
//! Which engine actually runs is decided by four tiers, highest first: the
//! `--engine` flag, `LIMAE_ENGINE`, the configuration file, then `auto`. The
//! fourth tier is the settings' own default, so a run with no configuration at
//! all lands on `auto` with each preset's own model.
//!
//! Nothing here reports a value the user wrote: an error names the file and the
//! key, never what the key was set to. The `custom` command especially stays
//! out of diagnostics — it is the user's own command line, and this repository
//! is public (`AGENTS.md` 「隐私边界」).

use std::borrow::Cow;
use std::ffi::OsString;
use std::path::Path;

use crate::config::{ConfigError, ConfigOrigin, find_config};

use super::engines::{ENGINES, Engine};
use super::value;

/// The sub-table both carriers hold the polish keys in.
const POLISH_TABLE: &str = "polish";
const ENGINE_KEY: &str = "engine";
const MODEL_KEY: &str = "model";
const COMMAND_KEY: &str = "command";

/// The engine name that asks for the `auto` search instead of naming a preset.
pub const AUTO_ENGINE: &str = "auto";
/// The engine name that runs the user's own command.
pub const CUSTOM_ENGINE: &str = "custom";
/// The environment variable of step 1 of the `auto` search.
pub const ENGINE_VARIABLE: &str = "LIMAE_ENGINE";

/// The resolved `[polish]` configuration of one run.
///
/// This type deliberately has no `Debug` implementation, for the reason
/// [`Engine`] has none: `command` is the user's own command line.
#[derive(Clone, PartialEq, Eq)]
pub struct PolishSettings {
    engine: String,
    model: String,
    command: Vec<String>,
}

impl PolishSettings {
    /// Return `auto`, a preset name, or `custom`.
    #[must_use]
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// Return the model override; empty means the preset's own default, which
    /// ADR-0008 section 五 does not freeze.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Return the whole `custom` command, placeholders included; empty for
    /// every other engine.
    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }
}

impl Default for PolishSettings {
    fn default() -> Self {
        Self {
            engine: AUTO_ENGINE.to_owned(),
            model: String::new(),
            command: Vec::new(),
        }
    }
}

/// Return the engine names this implementation accepts, in message order.
///
/// The presets are already in the order the reference implementation sorts
/// them into, so this is one list, not two.
pub fn known_engines() -> String {
    let mut names = Vec::with_capacity(ENGINES.len() + 2);
    names.push(AUTO_ENGINE);
    names.extend(ENGINES.iter().map(Engine::name));
    names.push(CUSTOM_ENGINE);
    names.join(", ")
}

fn is_known(engine: &str) -> bool {
    engine == AUTO_ENGINE
        || engine == CUSTOM_ENGINE
        || ENGINES.iter().any(|preset| preset.name() == engine)
}

/// Resolve the `[polish]` settings of one run.
///
/// No configuration file, or one without a `[polish]` table, means
/// `engine = "auto"` with each preset's own default model.
///
/// # Errors
/// Returns a [`ConfigError`] for candidate I/O failures, invalid TOML, a key
/// whose type is wrong, an engine name this implementation does not know,
/// `custom` without a command, or a command outside `custom`.
pub fn resolve(start: &Path) -> Result<PolishSettings, ConfigError> {
    let Some((path, value)) = find_config(start)? else {
        return Ok(PolishSettings::default());
    };
    let origin = ConfigOrigin::File(path);
    let table = value.as_table().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key: "tool.limae",
        expected: "a table",
    })?;
    let Some(raw) = table.get(POLISH_TABLE) else {
        return Ok(PolishSettings::default());
    };
    let polish = raw.as_table().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key: POLISH_TABLE,
        expected: "a table",
    })?;

    let engine = match string(polish, ENGINE_KEY, &origin)? {
        "" => AUTO_ENGINE,
        engine => engine,
    };
    if !is_known(engine) {
        return Err(ConfigError::UnknownEngine {
            origin,
            key: ENGINE_KEY,
            known: known_engines(),
        });
    }
    let command = words(polish, COMMAND_KEY, &origin)?;
    if engine == CUSTOM_ENGINE && command.is_empty() {
        return Err(ConfigError::CustomWithoutCommand { origin });
    }
    if !command.is_empty() && engine != CUSTOM_ENGINE {
        return Err(ConfigError::CommandWithoutCustom { origin });
    }
    let model = string(polish, MODEL_KEY, &origin)?.to_owned();
    Ok(PolishSettings {
        engine: engine.to_owned(),
        model,
        command,
    })
}

/// Decide which engine to use, before any probing.
///
/// The precedence is the command line, then `LIMAE_ENGINE`, then the
/// configuration file, then `auto` (ADR-0008 section 三 step 1). An empty flag
/// or an empty variable is not an answer, exactly as an unset one is not.
///
/// The name returned is not validated here: an engine asked for on the command
/// line or in the environment reaches the caller as written, so that an unknown
/// one is reported as such instead of silently falling through to the next
/// tier.
#[must_use]
pub fn engine<'settings>(
    flag: Option<&'settings str>,
    env: &'settings [(OsString, OsString)],
    settings: &'settings PolishSettings,
) -> Cow<'settings, str> {
    if let Some(flag) = flag.filter(|flag| !flag.is_empty()) {
        return Cow::Borrowed(flag);
    }
    if let Some(value) = value(env, ENGINE_VARIABLE) {
        return value.to_string_lossy();
    }
    Cow::Borrowed(settings.engine())
}

fn string<'table>(
    table: &'table toml::Table,
    key: &'static str,
    origin: &ConfigOrigin,
) -> Result<&'table str, ConfigError> {
    let Some(value) = table.get(key) else {
        return Ok("");
    };
    value.as_str().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key,
        expected: "a string",
    })
}

fn words(
    table: &toml::Table,
    key: &'static str,
    origin: &ConfigOrigin,
) -> Result<Vec<String>, ConfigError> {
    let Some(value) = table.get(key) else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| ConfigError::InvalidType {
        origin: origin.clone(),
        key,
        expected: "a list of strings",
    })?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| ConfigError::InvalidType {
                    origin: origin.clone(),
                    key,
                    expected: "a list of strings",
                })
        })
        .collect()
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
