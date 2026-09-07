//! The `polish` subcommand's own command line (ADR-0008 sections 二 and 六).
//!
//! This is the wiring, not the work: the prompt layers, the engine templates,
//! the `auto` search and the `[polish]` table are already their own modules,
//! and this module only decides the order they run in, what each failure is
//! called, and which exit code it leaves.
//!
//! Three exit codes, and they have to stay distinguishable (ADR-0008 section
//! 六): `0` when a rewrite reached stdout, `1` when the engine did not answer,
//! `2` for a usage or configuration mistake. Only the rewrite goes to stdout;
//! every diagnosis goes to stderr under one of three prefixes, so that
//! `limae polish - > out.md` writes prose and nothing else.
//!
//! An engine named on the command line or in `LIMAE_ENGINE` is checked here
//! even though [`super::config::resolve`] checks the one in the file. They are
//! two different doors into the same decision: the file's value is validated
//! when the file is read, and the other two tiers never pass through that read
//! at all (`super::config::engine` deliberately returns them as written).

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::time::SystemTime;

use clap::{Arg, Command, error::ErrorKind};

use super::config::{AUTO_ENGINE, CUSTOM_ENGINE, PolishSettings};
use super::engines::{ENGINES, Engine, EngineLimits, EngineRequest};
use super::process::CancellationToken;
use super::{config, prompt, select};

/// The rewrite reached stdout.
const SUCCESS: u8 = 0;
/// The engine did not answer.
const FAILED: u8 = 1;
/// The command line or the configuration was wrong.
const USAGE: u8 = 2;

/// The only positional value accepted so far; files come with the next step.
const STDIN_ARGUMENT: &str = "-";

/// Run the `polish` subcommand.
///
/// `args` is what follows `polish`. The caller supplies the working directory,
/// the whole environment of the run and the three streams, so that nothing
/// here reads process-global state: the ordering of the `auto` search and the
/// path of its cache both come out of `env`.
#[must_use]
pub fn run(
    args: &[OsString],
    cwd: &Path,
    env: &[(OsString, OsString)],
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let matches = match parse(args) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            return match write!(stdout, "{}", error.render()) {
                Ok(()) => SUCCESS,
                Err(_) => FAILED,
            };
        }
        Err(error) => return report(stderr, &error.render().to_string(), USAGE),
    };
    let input = matches.get_one::<String>("input").map(String::as_str);
    if input != Some(STDIN_ARGUMENT) {
        return report(
            stderr,
            &command()
                .error(
                    ErrorKind::InvalidValue,
                    format!(
                        "only '{STDIN_ARGUMENT}' (stdin) is supported so far; \
                         file arguments come with the next step"
                    ),
                )
                .render()
                .to_string(),
            USAGE,
        );
    }

    let settings = match config::resolve(cwd) {
        Ok(settings) => settings,
        Err(error) => return report(stderr, &format!("config error: {error}"), USAGE),
    };

    let mut text = String::new();
    // A stream that cannot be read, or that is not UTF-8, is neither a usage
    // mistake nor an engine's fault; the reference implementation lets the
    // read raise and exits 1, and so does this.
    if let Err(error) = stdin.read_to_string(&mut text) {
        return report(
            stderr,
            &format!("input error: cannot read standard input: {error}"),
            FAILED,
        );
    }
    if text
        .trim_matches(crate::text::is_python_whitespace)
        .is_empty()
    {
        return report(stderr, "input error: nothing on stdin to polish", USAGE);
    }

    let flag = matches.get_one::<String>("engine").map(String::as_str);
    let name = config::engine(flag, env, &settings);
    let limits = EngineLimits::default();
    let cancellation = CancellationToken::new();
    let now = SystemTime::now();
    let custom;
    let engine: &Engine = match chosen(&name, &settings) {
        Chosen::Preset(engine) => engine,
        Chosen::Custom(command) => {
            custom = Engine::Custom(command);
            &custom
        }
        Chosen::Auto => match select::select(env, limits, &cancellation, now) {
            Ok(engine) => engine,
            Err(error) => return report(stderr, &format!("engine error: {error}"), FAILED),
        },
        Chosen::Unknown => {
            return report(
                stderr,
                &format!(
                    "config error: unknown engine '{name}'; pick one of {}",
                    config::known_engines()
                ),
                USAGE,
            );
        }
        Chosen::CustomWithoutCommand => {
            return report(
                stderr,
                "config error: engine 'custom' needs [polish] command, \
                 the whole command to run",
                USAGE,
            );
        }
    };

    let model = matches
        .get_one::<String>("model")
        .map(String::as_str)
        .filter(|model| !model.is_empty())
        .unwrap_or_else(|| settings.model());
    let spec = prompt::assemble(&text);
    let request = EngineRequest {
        engine,
        model,
        spec: &spec,
        text: &text,
        cwd,
        env,
    };
    // The cache-writing front door, so that a real call that fails now
    // outranks a remembered success (`super::select::polish`).
    let polished = match select::polish(&request, limits, &cancellation, now) {
        Ok(polished) => polished,
        Err(error) => return report(stderr, &format!("engine error: {error}"), FAILED),
    };
    match write!(stdout, "{polished}") {
        Ok(()) => SUCCESS,
        Err(_) => FAILED,
    }
}

/// What the resolved engine name turned out to name.
enum Chosen {
    /// A built-in preset, ready to run.
    Preset(&'static Engine),
    /// The user's own command.
    Custom(Vec<String>),
    /// The `auto` search still has to pick one.
    Auto,
    /// A name no tier of the precedence chain accepts.
    Unknown,
    /// `custom` without the command it needs.
    CustomWithoutCommand,
}

fn chosen(name: &str, settings: &PolishSettings) -> Chosen {
    if name == AUTO_ENGINE {
        return Chosen::Auto;
    }
    if name == CUSTOM_ENGINE {
        return if settings.command().is_empty() {
            Chosen::CustomWithoutCommand
        } else {
            Chosen::Custom(settings.command().to_vec())
        };
    }
    ENGINES
        .iter()
        .find(|engine| engine.name() == name)
        .map_or(Chosen::Unknown, Chosen::Preset)
}

fn parse(args: &[OsString]) -> Result<clap::ArgMatches, clap::Error> {
    let argv = std::iter::once(OsString::from("limae polish")).chain(args.iter().cloned());
    command().try_get_matches_from(argv)
}

fn command() -> Command {
    Command::new("limae polish")
        .disable_version_flag(true)
        .about("rewrite prose with an LLM; reads stdin, writes stdout")
        .arg(
            Arg::new("input")
                .required(true)
                .value_name(STDIN_ARGUMENT)
                .help("the text to polish, read from stdin; files come later"),
        )
        .arg(Arg::new("engine").long("engine").value_name("ENGINE").help(
            "which engine to run: auto, one of the presets, or custom; \
                     overrides LIMAE_ENGINE and the config file",
        ))
        .arg(
            Arg::new("model")
                .long("model")
                .value_name("MODEL")
                .help("model to run, overriding the preset's default"),
        )
}

fn report(stderr: &mut dyn Write, message: &str, code: u8) -> u8 {
    let written = if message.ends_with('\n') {
        write!(stderr, "{message}")
    } else {
        writeln!(stderr, "{message}")
    };
    if written.is_ok() { code } else { FAILED }
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
