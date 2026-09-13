//! The `polish` subcommand's own command line (ADR-0008 sections 二 and 六,
//! ADR-0017 for file mode).
//!
//! This is the wiring, not the work: the prompt layers, the engine templates,
//! the `auto` search, the `[polish]` table and the exported view are already
//! their own modules, and this module only decides the order they run in,
//! what each failure is called, and which exit code it leaves.
//!
//! Two modes. `limae polish -` reads stdin and writes stdout, and nothing in
//! this file changes what it does. `limae polish <files>` and `limae polish
//! --all` rewrite files in place: the engine is started in an exported view
//! of the repository (`super::view`), answers through the same channel as
//! before, and limae is the only thing that writes a file. File mode is
//! behind [`SHARE_FLAG`] on every call — a configuration key or an
//! environment variable cannot pass it, and the refusal happens before any
//! engine is probed or started, so that "I did not know" is answered by the
//! error text and not by an engine that has already read the repository.
//!
//! Three exit codes, and they have to stay distinguishable (ADR-0008 section
//! 六): `0` when the work reached its output, `1` when the engine did not
//! answer, a target changed underneath the run, or the tripwire fired, `2`
//! for a usage or configuration mistake. In stdin mode only the rewrite goes
//! to stdout; in file mode stdout carries one line per target and every
//! diagnosis goes to stderr, so that `limae polish - > out.md` writes prose
//! and nothing else.
//!
//! An engine named on the command line or in `LIMAE_ENGINE` is checked here
//! even though [`super::config::resolve`] checks the one in the file. They are
//! two different doors into the same decision: the file's value is validated
//! when the file is read, and the other two tiers never pass through that read
//! at all (`super::config::engine` deliberately returns them as written).

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clap::{Arg, ArgAction, Command, error::ErrorKind};

use super::config::{AUTO_ENGINE, CUSTOM_ENGINE, PolishSettings};
use super::engines::{ENGINES, Engine, EngineLimits, EngineRequest};
use super::process::CancellationToken;
use super::view::{self, View};
use super::{config, prompt, select};
use crate::files::{not_ignored, walk_markdown};

/// The work reached its output.
const SUCCESS: u8 = 0;
/// The engine did not answer, a target moved, or the tripwire fired.
const FAILED: u8 = 1;
/// The command line or the configuration was wrong.
const USAGE: u8 = 2;

/// The positional value that selects stdin mode.
const STDIN_ARGUMENT: &str = "-";

/// The flag that opens file mode, on every call (ADR-0017 §一).
///
/// Its name says what it grants: the engine gets a copy of the repository's
/// tracked files and sends what it reads to its service. It is deliberately
/// not "read" alone, and deliberately not stored anywhere — a configuration
/// key would let the repository's author consent for every future runner,
/// and a trust record in the home directory would be a hidden state with an
/// expiry question nobody can answer. What the flag cannot do is written in
/// its help text below, because it belongs where the person deciding sees
/// it.
pub const SHARE_FLAG: &str = "share-repo-with-engine";

const SHARE_HELP: &str = "\
let the engine read a copy of this repository's tracked files and send what \
it reads to its service; required for <FILE>... and --all. The copy has no \
.git, no ignored or untracked file and none of the engines' own \
configuration (.claude, .codex, .grok, .mcp.json), so the repository cannot \
make the engine run anything; the engine can still read any absolute path \
it is given. limae's own write-back writes only the files named, and leaves \
a target that changed during the run alone; the engine itself and your \
user-level configuration (hooks under HOME) can still write outside the \
view, and the tripwire does not see that. This \
flag cannot stop a Makefile, a pre-commit `args` list or a CI step in the \
repository from passing it for you — that is the cost of a per-call flag, \
and it is chosen knowingly";

const SHARE_REFUSAL: &str = "\
polishing files starts a coding agent that reads your repository.
`limae polish <FILE>...` and `limae polish --all` copy the repository's \
tracked files (without .git, ignored files and the engines' own \
configuration: .claude, .codex, .grok, .mcp.json) into a private directory \
and start the engine there; what it reads there goes to that engine's \
service. Nothing has been started.
To accept that, pass --share-repo-with-engine on this call. A configuration \
key or an environment variable cannot pass it for you.";

const CUSTOM_REFUSAL: &str = "\
file mode runs the three presets only, each with its read-only tool set. \
A custom engine is a command taken from the repository's configuration, \
and limae cannot constrain what it runs or writes, so file mode refuses it. \
Use `limae polish -` with a custom engine, or name a preset with --engine.";

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
    // Mode, and the consent it needs, are settled before anything else is
    // touched: no configuration read, no engine probed, no view built.
    let mode = match mode(&matches) {
        Ok(mode) => mode,
        Err(message) => return usage(stderr, ErrorKind::InvalidValue, &message),
    };
    let shared = matches.get_flag(SHARE_FLAG);
    match (&mode, shared) {
        (Mode::Stdin, true) => {
            return usage(
                stderr,
                ErrorKind::ArgumentConflict,
                &format!(
                    "--{SHARE_FLAG} has no effect with '{STDIN_ARGUMENT}': stdin mode never \
                     starts the engine in a repository; drop the flag"
                ),
            );
        }
        (Mode::Files(_) | Mode::All, false) => {
            return usage(stderr, ErrorKind::MissingRequiredArgument, SHARE_REFUSAL);
        }
        _ => {}
    }

    let settings = match config::resolve(cwd) {
        Ok(settings) => settings,
        Err(error) => return report(stderr, &format!("config error: {error}"), USAGE),
    };
    // Stdin is read before an engine is chosen, so that an empty input is
    // the usage error it is and not the `auto` search's report.
    let mut text = String::new();
    if matches!(mode, Mode::Stdin) {
        // A stream that cannot be read, or that is not UTF-8, is neither a
        // usage mistake nor an engine's fault; the reference implementation
        // lets the read raise and exits 1, and so does this.
        if let Err(error) = stdin.read_to_string(&mut text) {
            return report(
                stderr,
                &format!("input error: cannot read standard input: {error}"),
                FAILED,
            );
        }
        if blank(&text) {
            return report(stderr, "input error: nothing on stdin to polish", USAGE);
        }
    }
    let flag = matches.get_one::<String>("engine").map(String::as_str);
    let name = config::engine(flag, env, &settings);
    let limits = EngineLimits::default();
    let cancellation = CancellationToken::new();
    let now = SystemTime::now();
    let picked = match chosen(&name, &settings) {
        Chosen::Preset(engine) => Picked::Static(engine),
        // A custom command is refused in file mode before anything runs: it
        // comes from the repository's own configuration, and nothing here
        // can constrain what it does — the read-only tool sets are the
        // presets' flags, and a custom command inherits the whole
        // environment. Accepting it would let the repository choose the
        // program, which is the one thing the view exists to prevent.
        Chosen::Custom(_) if !matches!(mode, Mode::Stdin) => {
            return usage(stderr, ErrorKind::InvalidValue, CUSTOM_REFUSAL);
        }
        Chosen::Custom(command) => Picked::Owned(Engine::Custom(command)),
        Chosen::Auto => match select::select(env, limits, &cancellation, now) {
            Ok(engine) => Picked::Static(engine),
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
    let run = Run {
        engine: picked.get(),
        model,
        limits,
        cancellation: &cancellation,
        now,
        cwd,
        env,
    };
    match mode {
        Mode::Stdin => run.stdin(&text, stdout, stderr),
        Mode::Files(files) => run.files(&files, stdout, stderr),
        Mode::All => run.all(stdout, stderr),
    }
}

/// What the positional arguments and `--all` ask for.
enum Mode {
    Stdin,
    Files(Vec<PathBuf>),
    All,
}

fn mode(matches: &clap::ArgMatches) -> Result<Mode, String> {
    let inputs: Vec<&String> = matches
        .get_many::<String>("inputs")
        .map(Iterator::collect)
        .unwrap_or_default();
    let all = matches.get_flag("all");
    let stdin = inputs.iter().any(|input| *input == STDIN_ARGUMENT);
    if stdin && (all || inputs.len() > 1) {
        return Err(format!(
            "'{STDIN_ARGUMENT}' reads stdin and writes stdout; it cannot be combined with files or --all"
        ));
    }
    if stdin {
        return Ok(Mode::Stdin);
    }
    if all && !inputs.is_empty() {
        return Err(
            "--all selects every Markdown file below the working directory; do not also name files"
                .to_owned(),
        );
    }
    if all {
        return Ok(Mode::All);
    }
    if inputs.is_empty() {
        return Err(format!(
            "nothing to polish: give '{STDIN_ARGUMENT}' for stdin, or files, or --all"
        ));
    }
    Ok(Mode::Files(inputs.into_iter().map(PathBuf::from).collect()))
}

/// One resolved engine and the bounds every call shares.
struct Run<'a> {
    engine: &'a Engine,
    model: &'a str,
    limits: EngineLimits,
    cancellation: &'a CancellationToken,
    now: SystemTime,
    cwd: &'a Path,
    env: &'a [(OsString, OsString)],
}

impl Run<'_> {
    /// `limae polish -`: the text on stdin, rewritten on stdout.
    fn stdin(&self, text: &str, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
        let spec = match prompt::assemble(text) {
            Ok(spec) => spec,
            Err(error) => return report(stderr, &format!("prompt error: {error}"), FAILED),
        };
        let polished = match self.polish(&spec, text, None) {
            Ok(polished) => polished,
            Err(error) => return report(stderr, &format!("engine error: {error}"), FAILED),
        };
        match write!(stdout, "{polished}") {
            Ok(()) => SUCCESS,
            Err(_) => FAILED,
        }
    }

    /// `limae polish <files>`: the named files, rewritten in place.
    fn files(&self, files: &[PathBuf], stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
        let mut targets = Vec::with_capacity(files.len());
        for file in files {
            match Target::new(file, self.cwd) {
                Ok(Some(target)) => targets.push(target),
                Ok(None) => {
                    return usage(
                        stderr,
                        ErrorKind::InvalidValue,
                        &format!(
                            "{} is a symbolic link; name its target instead, so that the write goes where you can see it",
                            file.display()
                        ),
                    );
                }
                Err(error) => {
                    return usage(
                        stderr,
                        ErrorKind::InvalidValue,
                        &format!("cannot polish {}: {error}", file.display()),
                    );
                }
            }
        }
        self.rewrite(targets, stdout, stderr)
    }

    /// `limae polish --all`: every Markdown file below the working directory
    /// that `.gitignore`, `.ignore` and `.limae-ignore` leave in — the same
    /// selection as `limae --all`, so that the two commands agree on what
    /// "every file" means.
    fn all(&self, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
        let selected = match walk_markdown(self.cwd) {
            Ok(selected) => selected,
            Err(error) => return report(stderr, &format!("error: {error}"), FAILED),
        };
        let selected = match not_ignored(&selected, self.cwd) {
            Ok(selected) => selected,
            Err(error) => return report(stderr, &format!("error: {error}"), FAILED),
        };
        let mut targets = Vec::with_capacity(selected.len());
        for file in &selected {
            match Target::new(file, self.cwd) {
                Ok(Some(target)) => targets.push(target),
                // Named under --all rather than by the user, so a link is
                // skipped and said so, not refused: its target is in the
                // same walk and gets polished under its own name.
                Ok(None) => {
                    if writeln!(
                        stderr,
                        "skipped: {} is a symbolic link; its target is polished under its own name",
                        file.display()
                    )
                    .is_err()
                    {
                        return FAILED;
                    }
                }
                Err(error) => {
                    return report(
                        stderr,
                        &format!("error: cannot polish {}: {error}", file.display()),
                        FAILED,
                    );
                }
            }
        }
        self.rewrite(targets, stdout, stderr)
    }

    /// Rewrite `targets` in place through one exported view (ADR-0017 §二).
    ///
    /// The view is built once; the engine is started in it once per target.
    /// After each call the view is compared with its snapshot (the tripwire)
    /// and the target with the bytes read before the call (the conflict
    /// check); only then is anything written, and only by this function.
    fn rewrite(&self, targets: Vec<Target>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> u8 {
        let repository = match view::repository_root(self.cwd) {
            Ok(repository) => repository,
            Err(error) => return report(stderr, &format!("error: {error}"), USAGE),
        };
        let repository = match repository.canonicalize() {
            Ok(repository) => repository,
            Err(error) => {
                return report(
                    stderr,
                    &format!("error: cannot resolve the repository root: {error}"),
                    FAILED,
                );
            }
        };
        let mut inside = Vec::with_capacity(targets.len());
        for target in targets {
            match target.canonical.strip_prefix(&repository) {
                Ok(relative) => {
                    inside.push((target.clone(), relative.to_string_lossy().into_owned()))
                }
                Err(_) => {
                    return usage(
                        stderr,
                        ErrorKind::InvalidValue,
                        &format!(
                            "{} is outside the repository at {}; file mode polishes tracked-repository files only",
                            target.shown.display(),
                            repository.display()
                        ),
                    );
                }
            }
        }
        if inside.is_empty() {
            return match writeln!(stdout, "nothing to polish") {
                Ok(()) => SUCCESS,
                Err(_) => FAILED,
            };
        }

        let view = match View::export(&repository) {
            Ok(view) => view,
            Err(error) => return report(stderr, &format!("error: {error}"), FAILED),
        };
        let code = self.rewrite_in(&view, &inside, stdout, stderr);
        match view.remove() {
            Ok(()) => code,
            Err(error) => report(stderr, &format!("error: {error}"), FAILED),
        }
    }

    fn rewrite_in(
        &self,
        view: &View,
        targets: &[(Target, String)],
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> u8 {
        let before = match view.snapshot() {
            Ok(before) => before,
            Err(error) => return report(stderr, &format!("error: {error}"), FAILED),
        };
        let mut code = SUCCESS;
        for (target, relative) in targets {
            let shown = target.shown.display();
            let original = match fs::read(&target.canonical) {
                Ok(original) => original,
                Err(error) => {
                    return report(
                        stderr,
                        &format!("error: cannot read {shown}: {error}"),
                        FAILED,
                    );
                }
            };
            let Ok(text) = String::from_utf8(original.clone()) else {
                return report(
                    stderr,
                    &format!("error: cannot decode {shown} as UTF-8"),
                    FAILED,
                );
            };
            if blank(&text) {
                if writeln!(stderr, "skipped: {shown}: nothing to polish").is_err() {
                    return FAILED;
                }
                continue;
            }
            let spec = match prompt::assemble(&text) {
                Ok(mut spec) => {
                    spec.push_str(&prompt::file_mode(relative));
                    spec
                }
                Err(error) => return report(stderr, &format!("prompt error: {error}"), FAILED),
            };
            let polished = match self.polish(&spec, &text, Some(view.root())) {
                Ok(polished) => polished,
                Err(error) => {
                    return report(stderr, &format!("engine error: {shown}: {error}"), FAILED);
                }
            };
            // The tripwire: an engine that wrote in its view had a capability
            // the read-only setup was supposed to remove. Nothing from such a
            // run is written, and the run stops here.
            let after = match view.snapshot() {
                Ok(after) => after,
                Err(error) => return report(stderr, &format!("error: {error}"), FAILED),
            };
            let changed = before.differences(&after);
            if let Some(first) = changed.first() {
                return report(
                    stderr,
                    &format!(
                        "tripwire: the engine changed {} path(s) in its view (first: {}); the read-only setup did not hold, so {shown} was not written and the run stops here",
                        changed.len(),
                        first.display()
                    ),
                    FAILED,
                );
            }
            // The conflict check: the target is written only if it is still
            // the bytes the engine was given. Anything else — an editor, a
            // formatter, a concurrent run — keeps its version.
            match fs::read(&target.canonical) {
                Ok(current) if current == original => {}
                Ok(_) => {
                    if writeln!(
                        stderr,
                        "not written: {shown} changed while the engine was running; its current content is kept, polish it again"
                    )
                    .is_err()
                    {
                        return FAILED;
                    }
                    code = FAILED;
                    continue;
                }
                Err(error) => {
                    return report(
                        stderr,
                        &format!("error: cannot re-read {shown}: {error}"),
                        FAILED,
                    );
                }
            }
            if polished == text {
                if writeln!(stdout, "unchanged: {shown}").is_err() {
                    return FAILED;
                }
                continue;
            }
            if let Err(error) = write_through(&target.canonical, &polished) {
                return report(
                    stderr,
                    &format!("error: cannot write {shown}: {error}"),
                    FAILED,
                );
            }
            if writeln!(stdout, "polished: {shown}").is_err() {
                return FAILED;
            }
        }
        code
    }

    /// One engine call through the cache-writing front door, so that a real
    /// call that fails now outranks a remembered success
    /// (`super::select::polish`).
    fn polish(
        &self,
        spec: &str,
        text: &str,
        view: Option<&Path>,
    ) -> Result<String, super::engines::EngineError> {
        let request = EngineRequest {
            engine: self.engine,
            model: self.model,
            spec,
            text,
            cwd: self.cwd,
            env: self.env,
            view,
        };
        select::polish(&request, self.limits, self.cancellation, self.now)
    }
}

/// One file to rewrite: as the user named it, and where it really is.
#[derive(Clone)]
struct Target {
    shown: PathBuf,
    canonical: PathBuf,
}

impl Target {
    /// Resolve `file` against `cwd`; `None` when it is a symbolic link.
    ///
    /// A link is not written through, unlike `limae --fix`: in file mode the
    /// write is the product of an engine call, and it should land on the
    /// path the person named, where the diff will be looked at. Hard links
    /// cannot be told apart here and are written through (ADR-0017 §四).
    fn new(file: &Path, cwd: &Path) -> Result<Option<Self>, io::Error> {
        let path = cwd.join(file);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Ok(None);
        }
        if !metadata.is_file() {
            return Err(io::Error::other("not a regular file"));
        }
        Ok(Some(Self {
            shown: file.to_owned(),
            canonical: path.canonicalize()?,
        }))
    }
}

/// Replace a target's content through its existing path, the way
/// `limae --fix` does, so that mode and hard links survive.
fn write_through(path: &Path, content: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
    file.write_all(content.as_bytes())?;
    file.flush()
}

fn blank(text: &str) -> bool {
    text.trim_matches(crate::text::is_python_whitespace)
        .is_empty()
}

/// A preset borrowed from the static table, or the user's own command.
enum Picked {
    Static(&'static Engine),
    Owned(Engine),
}

impl Picked {
    fn get(&self) -> &Engine {
        match self {
            Self::Static(engine) => engine,
            Self::Owned(engine) => engine,
        }
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
        .about(
            "rewrite prose with an LLM: '-' reads stdin and writes stdout; \
             files are rewritten in place, behind --share-repo-with-engine",
        )
        .arg(
            Arg::new("inputs")
                .action(ArgAction::Append)
                .num_args(0..)
                .value_name("FILE")
                .help("files to polish in place, or '-' to read stdin and write stdout"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .action(ArgAction::SetTrue)
                .help("polish every Markdown file below the working directory, honouring .gitignore and .limae-ignore"),
        )
        .arg(
            Arg::new(SHARE_FLAG)
                .long(SHARE_FLAG)
                .action(ArgAction::SetTrue)
                .help(SHARE_HELP),
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

fn usage(stderr: &mut dyn Write, kind: ErrorKind, message: &str) -> u8 {
    report(
        stderr,
        &command().error(kind, message).render().to_string(),
        USAGE,
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
