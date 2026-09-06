//! Command-line orchestration for the temporary `limae-rs` binary.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, Command, error::ErrorKind, value_parser};
use thiserror::Error;

use crate::config::{CliOverrides, ConfigError, Severity, resolve};
use crate::files::{
    FileError, FileText, FixStatus, GitError, IgnoreError, fix_file, not_ignored, tracked_markdown,
};
use crate::pipeline::{InitError, Pipeline};

const SUCCESS: u8 = 0;
const FINDINGS: u8 = 1;
const USAGE: u8 = 2;

/// Run `limae-rs` against the process arguments and standard streams.
#[must_use]
pub fn run_process() -> u8 {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(source) => {
            return write_process_error(&mut stderr, &source);
        }
    };
    run_from(std::env::args_os(), &cwd, &mut stdout, &mut stderr)
}

/// Run one CLI invocation with explicit process boundaries.
///
/// `args` includes the executable name. Callers supply the working directory
/// and output streams so tests and embedders need not mutate process-global
/// state.
#[must_use]
pub fn run_from(
    args: impl IntoIterator<Item = OsString>,
    cwd: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> u8 {
    let args: Vec<_> = args.into_iter().collect();
    if let Some(subcommand @ ("polish" | "hook")) =
        args.get(1).and_then(|argument| argument.to_str())
    {
        return write_clap_error(
            command().error(
                ErrorKind::InvalidSubcommand,
                format!("the '{subcommand}' subcommand is not provided yet"),
            ),
            stderr,
        );
    }

    let mut command = command();
    let matches = match command.try_get_matches_from_mut(args) {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            return if write!(stdout, "{}", error.render()).is_ok() {
                SUCCESS
            } else {
                FINDINGS
            };
        }
        Err(error) => return write_clap_error(error, stderr),
    };

    let disable = matches
        .get_many::<String>("disable")
        .map(|values| values.cloned().collect::<Vec<_>>());
    let enable = matches
        .get_many::<String>("enable")
        .map(|values| values.cloned().collect::<Vec<_>>());
    let options = Options {
        all: matches.get_flag("all"),
        fix: matches.get_flag("fix"),
        disable,
        enable,
        files: matches
            .get_many::<PathBuf>("files")
            .map(|values| values.cloned().collect())
            .unwrap_or_default(),
    };

    match execute(&options, cwd, stdout) {
        Ok(code) => code,
        Err(RunError::NoFiles) => write_clap_error(
            command.error(
                ErrorKind::MissingRequiredArgument,
                "no files given (use --all or list files)",
            ),
            stderr,
        ),
        Err(error) => write_run_error(stderr, &error),
    }
}

fn command() -> Command {
    Command::new("limae-rs")
        .disable_version_flag(true)
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("check all Git-tracked Markdown files"),
        )
        .arg(
            Arg::new("fix")
                .long("fix")
                .action(ArgAction::SetTrue)
                .help("fix supported findings before checking"),
        )
        .arg(
            Arg::new("disable")
                .long("disable")
                .action(ArgAction::Append)
                .value_name("RULE")
                .help(
                    "rule ids to disable, repeatable or comma-separated; overrides the config file",
                ),
        )
        .arg(
            Arg::new("enable")
                .long("enable")
                .action(ArgAction::Append)
                .value_name("RULE")
                .help(
                    "rule ids to enable (for default-off rules), same syntax and precedence as --disable",
                ),
        )
        .arg(
            Arg::new("files")
                .action(ArgAction::Append)
                .num_args(0..)
                .value_name("FILE")
                .value_parser(value_parser!(PathBuf)),
        )
}

struct Options {
    all: bool,
    fix: bool,
    disable: Option<Vec<String>>,
    enable: Option<Vec<String>>,
    files: Vec<PathBuf>,
}

fn execute(options: &Options, cwd: &Path, stdout: &mut dyn Write) -> Result<u8, RunError> {
    let config = resolve(
        cwd,
        CliOverrides {
            disable: options.disable.as_deref(),
            enable: options.enable.as_deref(),
        },
    )?;
    let selected = if options.all {
        tracked_markdown(cwd)?
    } else {
        options.files.clone()
    };
    if selected.is_empty() {
        return Err(RunError::NoFiles);
    }
    let paths = not_ignored(&selected, cwd)?;
    if paths.is_empty() {
        writeln!(stdout, "OK: 0 file(s) clean")?;
        return Ok(SUCCESS);
    }

    let pipeline = Pipeline::new()?;
    let mut messages = Vec::new();
    for path in &paths {
        let actual_path = cwd.join(path);
        if options.fix
            && fix_file(&actual_path, &pipeline, &config)
                .map_err(|error| relabel_file_error(error, path))?
                == FixStatus::Written
        {
            writeln!(stdout, "fixed: {}", path.display())?;
        }
        let source =
            FileText::read(&actual_path).map_err(|error| relabel_file_error(error, path))?;
        for finding in source
            .check(&pipeline, &config)
            .map_err(|error| relabel_file_error(error, path))?
        {
            let severity = config.severity(finding.rule);
            messages.push((
                severity,
                format!(
                    "{}:{}: {}: [{}] …{}…",
                    path.display(),
                    finding.line,
                    severity_name(severity),
                    finding.name,
                    finding.snippet,
                ),
            ));
        }
    }

    if messages.is_empty() {
        writeln!(stdout, "OK: {} file(s) clean", paths.len())?;
        return Ok(SUCCESS);
    }
    for (_, message) in &messages {
        writeln!(stdout, "{message}")?;
    }
    let errors = messages
        .iter()
        .filter(|(severity, _)| *severity == Severity::Error)
        .count();
    writeln!(
        stdout,
        "\n{errors} error(s), {} warning(s). --fix auto-fixes most.",
        messages.len() - errors,
    )?;
    Ok(if errors == 0 { SUCCESS } else { FINDINGS })
}

fn relabel_file_error(error: FileError, path: &Path) -> FileError {
    match error {
        FileError::Read { source, .. } => FileError::Read {
            path: path.to_owned(),
            source,
        },
        FileError::Utf8 { source, .. } => FileError::Utf8 {
            path: path.to_owned(),
            source,
        },
        FileError::Directive { source, .. } => FileError::Directive {
            path: path.to_owned(),
            source,
        },
        FileError::Write { source, .. } => FileError::Write {
            path: path.to_owned(),
            source,
        },
        FileError::Flush { source, .. } => FileError::Flush {
            path: path.to_owned(),
            source,
        },
    }
}

const fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

#[derive(Debug, Error)]
enum RunError {
    #[error("no files selected")]
    NoFiles,
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Ignore(#[from] IgnoreError),
    #[error(transparent)]
    Init(#[from] InitError),
    #[error(transparent)]
    File(#[from] FileError),
    #[error("cannot write command output: {0}")]
    Output(#[from] io::Error),
}

impl RunError {
    const fn exit_code(&self) -> u8 {
        match self {
            Self::NoFiles | Self::Config(_) => USAGE,
            Self::File(FileError::Directive { .. }) => USAGE,
            Self::Git(_) | Self::Ignore(_) | Self::Init(_) | Self::File(_) | Self::Output(_) => {
                FINDINGS
            }
        }
    }

    const fn label(&self) -> &'static str {
        match self {
            Self::Config(_) => "config error",
            Self::File(FileError::Directive { .. }) => "directive error",
            Self::NoFiles
            | Self::Git(_)
            | Self::Ignore(_)
            | Self::Init(_)
            | Self::File(_)
            | Self::Output(_) => "error",
        }
    }
}

fn write_clap_error(error: clap::Error, stderr: &mut dyn Write) -> u8 {
    if write!(stderr, "{}", error.render()).is_ok() {
        USAGE
    } else {
        FINDINGS
    }
}

fn write_run_error(stderr: &mut dyn Write, error: &RunError) -> u8 {
    if writeln!(stderr, "{}: {error}", error.label()).is_ok() {
        error.exit_code()
    } else {
        FINDINGS
    }
}

fn write_process_error(stderr: &mut dyn Write, error: &io::Error) -> u8 {
    let _ = writeln!(stderr, "error: cannot determine current directory: {error}");
    FINDINGS
}
