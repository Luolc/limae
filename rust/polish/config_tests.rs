use super::{AUTO_ENGINE, ENGINE_VARIABLE, PolishSettings, engine, resolve};
use crate::config::{ConfigError, ConfigOrigin};
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn Error>>;

/// A synthetic command word; no arm may let it reach an error message.
const SYNTHETIC_COMMAND: &str = "mygateway";
const SYNTHETIC_MODEL: &str = "synthetic-model-name";

struct TempDir(PathBuf);

impl TempDir {
    /// Create an empty directory that ends the upward walk at itself.
    ///
    /// The `.git` marker is what makes these arms hermetic: without it the walk
    /// would leave the temporary directory and could find whatever
    /// configuration the machine running the tests happens to keep above it.
    fn new(name: &str) -> Result<Self, std::io::Error> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "limae-polish-config-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        fs::create_dir(path.join(".git"))?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, body: &str) -> Result<PathBuf, std::io::Error> {
        let path = self.0.join(name);
        fs::write(&path, body)?;
        Ok(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn environment(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).into(), (*value).into()))
        .collect()
}

fn settings_for(engine: &str) -> Result<PolishSettings, Box<dyn Error>> {
    let root = TempDir::new("chain")?;
    let _ = root.write("limae.toml", &format!("[polish]\nengine = \"{engine}\"\n"))?;
    Ok(resolve(root.path())?)
}

#[test]
fn no_configuration_at_all_means_auto_and_each_preset_s_own_model() -> TestResult {
    let root = TempDir::new("empty")?;
    let settings = resolve(root.path())?;
    assert_eq!(settings.engine(), AUTO_ENGINE);
    assert_eq!(settings.model(), "");
    assert!(settings.command().is_empty());
    Ok(())
}

#[test]
fn a_file_without_a_polish_table_is_the_same_as_no_file() -> TestResult {
    let root = TempDir::new("norole")?;
    let _ = root.write("limae.toml", "disable = [\"zh-typography-1\"]\n")?;
    let settings = resolve(root.path())?;
    assert_eq!(settings.engine(), AUTO_ENGINE);
    assert_eq!(settings.model(), "");
    assert!(settings.command().is_empty());
    Ok(())
}

#[test]
fn the_standalone_file_carries_all_three_keys() -> TestResult {
    let root = TempDir::new("standalone")?;
    let _ = root.write(
        "limae.toml",
        &format!(
            "[polish]\nengine = \"custom\"\nmodel = \"{SYNTHETIC_MODEL}\"\n\
             command = [\"{SYNTHETIC_COMMAND}\", \"{{spec_file}}\"]\n"
        ),
    )?;
    let settings = resolve(root.path())?;
    assert_eq!(settings.engine(), "custom");
    assert_eq!(settings.model(), SYNTHETIC_MODEL);
    assert_eq!(settings.command(), [SYNTHETIC_COMMAND, "{spec_file}"]);
    Ok(())
}

#[test]
fn the_pyproject_carrier_holds_the_same_keys_one_table_deeper() -> TestResult {
    let root = TempDir::new("pyproject")?;
    let _ = root.write(
        "pyproject.toml",
        &format!(
            "[project]\nname = \"synthetic\"\n\n[tool.limae.polish]\n\
             engine = \"codex\"\nmodel = \"{SYNTHETIC_MODEL}\"\n"
        ),
    )?;
    let settings = resolve(root.path())?;
    assert_eq!(settings.engine(), "codex");
    assert_eq!(settings.model(), SYNTHETIC_MODEL);
    assert!(settings.command().is_empty());
    Ok(())
}

#[test]
fn the_search_walks_up_from_a_directory_that_has_no_file_of_its_own() -> TestResult {
    let root = TempDir::new("walk")?;
    let _ = root.write("limae.toml", "[polish]\nengine = \"grok\"\n")?;
    let nested = root.path().join("deep").join("deeper");
    fs::create_dir_all(&nested)?;

    assert_eq!(resolve(&nested)?.engine(), "grok");
    // The same walk, the same file: a second search rooted at the file's own
    // directory must not disagree with the one that climbed to it.
    assert_eq!(resolve(root.path())?.engine(), "grok");
    Ok(())
}

#[test]
fn broken_toml_is_reported_against_the_file_it_came_from() -> TestResult {
    let root = TempDir::new("badtoml")?;
    let file = root.write("limae.toml", "[polish\nengine = \"codex\"\n")?;
    let Err(ConfigError::Parse { path, .. }) = resolve(root.path()) else {
        return Err("broken toml must be a parse error".into());
    };
    assert_eq!(path, file);
    Ok(())
}

#[test]
fn a_key_of_the_wrong_type_names_that_key() -> TestResult {
    let root = TempDir::new("badtype")?;
    let file = root.write("limae.toml", "[polish]\nengine = 7\n")?;
    let Err(error) = resolve(root.path()) else {
        return Err("a non-string engine must be rejected".into());
    };
    let ConfigError::InvalidType { origin, key, .. } = &error else {
        return Err("a non-string engine must be a type error".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    assert_eq!(*key, "engine");
    assert!(error.to_string().contains("must be a string"));
    Ok(())
}

#[test]
fn a_command_that_is_not_a_list_of_strings_names_the_command_key() -> TestResult {
    let root = TempDir::new("badcommand")?;
    let file = root.write(
        "limae.toml",
        &format!("[polish]\nengine = \"custom\"\ncommand = \"{SYNTHETIC_COMMAND}\"\n"),
    )?;
    let Err(error) = resolve(root.path()) else {
        return Err("a string command must be rejected".into());
    };
    let ConfigError::InvalidType { origin, key, .. } = &error else {
        return Err("a string command must be a type error".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    assert_eq!(*key, "command");
    let message = error.to_string();
    assert!(message.contains("must be a list of strings"));
    assert!(!message.contains(SYNTHETIC_COMMAND));
    Ok(())
}

#[test]
fn a_polish_table_that_is_not_a_table_names_the_table() -> TestResult {
    let root = TempDir::new("badtable")?;
    let file = root.write("limae.toml", "polish = \"codex\"\n")?;
    let Err(error) = resolve(root.path()) else {
        return Err("a scalar [polish] must be rejected".into());
    };
    let ConfigError::InvalidType { origin, key, .. } = &error else {
        return Err("a scalar [polish] must be a type error".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    assert_eq!(*key, "polish");
    assert!(error.to_string().contains("must be a table"));
    Ok(())
}

#[test]
fn an_unknown_engine_name_lists_the_known_ones_without_echoing_the_value() -> TestResult {
    let root = TempDir::new("unknown")?;
    let file = root.write("limae.toml", "[polish]\nengine = \"gemini\"\n")?;
    let Err(error) = resolve(root.path()) else {
        return Err("an unknown engine must be rejected".into());
    };
    let ConfigError::UnknownEngine { origin, key, .. } = &error else {
        return Err("an unknown engine needs its own category".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    assert_eq!(*key, "engine");
    let message = error.to_string();
    assert!(message.contains("must be one of auto, claude, codex, grok, custom"));
    assert!(!message.contains("gemini"));
    Ok(())
}

#[test]
fn custom_without_a_command_has_its_own_category() -> TestResult {
    let root = TempDir::new("nocommand")?;
    let file = root.write("limae.toml", "[polish]\nengine = \"custom\"\n")?;
    let Err(error) = resolve(root.path()) else {
        return Err("custom without a command must be rejected".into());
    };
    let ConfigError::CustomWithoutCommand { origin } = &error else {
        return Err("custom without a command needs its own category".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    // Both keys the diagnosis turns on, spelled as the reference
    // implementation spells them: which value of `engine` asks for a
    // `command`, and which key would supply it.
    assert!(
        error
            .to_string()
            .contains("`engine = \"custom\"` needs `command`, the whole command to run")
    );
    Ok(())
}

#[test]
fn a_command_outside_custom_has_its_own_category_and_stays_out_of_the_message() -> TestResult {
    let root = TempDir::new("straycommand")?;
    let file = root.write(
        "limae.toml",
        &format!("[polish]\nengine = \"claude\"\ncommand = [\"{SYNTHETIC_COMMAND}\"]\n"),
    )?;
    let Err(error) = resolve(root.path()) else {
        return Err("a command outside custom must be rejected".into());
    };
    let ConfigError::CommandWithoutCustom { origin } = &error else {
        return Err("a command outside custom needs its own category".into());
    };
    assert_eq!(*origin, ConfigOrigin::File(file));
    let message = error.to_string();
    assert!(message.contains("`command` only runs under `engine = \"custom\"`"));
    assert!(!message.contains(SYNTHETIC_COMMAND));
    Ok(())
}

#[test]
fn the_flag_beats_the_variable_and_the_file() -> TestResult {
    let settings = settings_for("claude")?;
    let env = environment(&[(ENGINE_VARIABLE, "grok")]);
    assert_eq!(engine(Some("codex"), &env, &settings), "codex");
    Ok(())
}

#[test]
fn the_variable_beats_the_file() -> TestResult {
    let settings = settings_for("claude")?;
    let env = environment(&[(ENGINE_VARIABLE, "grok")]);
    assert_eq!(engine(None, &env, &settings), "grok");
    Ok(())
}

#[test]
fn the_file_beats_the_auto_fallback() -> TestResult {
    let settings = settings_for("claude")?;
    assert_eq!(engine(None, &environment(&[]), &settings), "claude");
    Ok(())
}

#[test]
fn nothing_asked_for_at_any_tier_is_auto() -> TestResult {
    let root = TempDir::new("fallback")?;
    let settings = resolve(root.path())?;
    assert_eq!(engine(None, &environment(&[]), &settings), AUTO_ENGINE);
    Ok(())
}

#[test]
fn an_empty_flag_or_variable_is_not_an_answer() -> TestResult {
    let settings = settings_for("claude")?;
    let env = environment(&[(ENGINE_VARIABLE, "")]);
    assert_eq!(engine(Some(""), &env, &settings), "claude");
    assert_eq!(engine(Some(""), &environment(&[]), &settings), "claude");
    Ok(())
}

#[test]
fn an_engine_asked_for_outside_the_file_reaches_the_caller_as_written() -> TestResult {
    // The chain does not validate: an unknown name must be reported as unknown
    // rather than fall through to the tier below it.
    let settings = settings_for("claude")?;
    let env = environment(&[(ENGINE_VARIABLE, "gemini")]);
    assert_eq!(engine(None, &env, &settings), "gemini");
    assert_eq!(
        engine(Some("gemini"), &environment(&[]), &settings),
        "gemini"
    );
    Ok(())
}
