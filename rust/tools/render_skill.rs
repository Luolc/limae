//! Assemble the agent skill from its sources.
//!
//! Writes `skills/limae/` — the skill a person copies into their agent so
//! that the model keeps limae's rules while it writes, rather than having its
//! text polished afterwards (ADR-0016 §七). Three files, each from one
//! source:
//!
//! - `SKILL.md`: the YAML front matter this tool owns, then the body of
//!   `spec/skill/SKILL.md`. The front matter is not in the source because it
//!   is packaging: `name` has to equal the directory the skill is installed
//!   under, which the language-neutral body has no business knowing.
//! - `references/zh/guide.md`: `spec/skill/zh.md`, verbatim behind a note.
//! - `references/zh/lexicon.md`: `spec/lexicon/zh.toml`, rendered by the
//!   same [`limae::polish::lexicon::render`] the polish prompt uses, in its
//!   full shape. One renderer for both products, so the skill and the prompt
//!   cannot disagree about an entry.
//!
//! Unlike `site/`, the output is committed: the skill is cloned and used as
//! files, so a product that only existed after a build would not be
//! published. `tools/check_skill_render.sh` is the gate that keeps the
//! committed copy equal to what these sources render, with a control arm
//! that changes the sources and requires a different result.
//!
//! The polish prompt does not include `SKILL.md`, and this tool does not
//! include the polish spec. The two assemble different lists from shared
//! sources: the skill's file map ("read `references/zh/…` first") would be
//! noise inside a prompt that is loaded whole, and the polish output
//! contract is noise to a model that is writing, not rewriting.
//!
//! A development-time tool, so `rust/tools/` and `[[example]]`, for the
//! reasons written at the top of `render_lexicon.rs`. Run it from the
//! repository root:
//!
//! ```sh
//! cargo run --example render-skill
//! ```

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use limae::polish::lexicon::{self, Detail, LexiconError};
use thiserror::Error;

/// The skill's directory; `NAME` must be its last component.
const TARGET_DIR: &str = "skills/limae";
/// Front matter `name`, by the agentskills.io specification: equal to the
/// parent directory, at most 64 characters, lowercase letters, digits and
/// hyphens, no leading, trailing or doubled hyphen.
const NAME: &str = "limae";
/// Front matter `description`: what the skill does and when to use it, at
/// most 1024 characters. Kept free of `: ` and leading punctuation so that it
/// stays a plain YAML scalar.
const DESCRIPTION: &str = "Write Chinese technical prose the way a native \
speaker writes it, not the way a machine translation or an LLM draft reads. \
Use when drafting or revising anything a person will read (replies, Markdown, \
code comments, commit messages) in a repository that uses limae, or whenever \
the user asks for writing without AI tells. Other languages will follow.";

const BODY_SOURCE: &str = "spec/skill/SKILL.md";
const ZH_SOURCE: &str = "spec/skill/zh.md";
const LEXICON_SOURCE: &str = "spec/lexicon/zh.toml";

const SKILL_TARGET: &str = "skills/limae/SKILL.md";
const ZH_TARGET: &str = "skills/limae/references/zh/guide.md";
const LEXICON_TARGET: &str = "skills/limae/references/zh/lexicon.md";

#[derive(Debug, Error)]
enum RenderError {
    #[error("not found: {0}; run from the root")]
    Missing(&'static str),
    #[error("cannot read {path}: {source}")]
    Read {
        path: &'static str,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Lexicon(#[from] LexiconError),
    #[error("front matter: {0}")]
    FrontMatter(&'static str),
    #[error("cannot write {path}: {source}")]
    Write {
        path: &'static str,
        #[source]
        source: io::Error,
    },
}

/// Refuse a front matter the specification would reject.
///
/// The values are constants in this file, so this runs against the person
/// who edits them, not against user input: it is the shortest way to keep
/// the rule and the value in the same place.
fn check_front_matter() -> Result<(), RenderError> {
    let directory = Path::new(TARGET_DIR)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RenderError::FrontMatter("target directory has no name"))?;
    if NAME != directory {
        return Err(RenderError::FrontMatter(
            "`name` must equal the skill's directory name",
        ));
    }
    if NAME.is_empty() || NAME.len() > 64 {
        return Err(RenderError::FrontMatter("`name` must be 1–64 characters"));
    }
    if !NAME
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(RenderError::FrontMatter(
            "`name` must be lowercase letters, digits and hyphens",
        ));
    }
    if NAME.starts_with('-') || NAME.ends_with('-') || NAME.contains("--") {
        return Err(RenderError::FrontMatter(
            "`name` must not start or end with a hyphen, or contain `--`",
        ));
    }
    if DESCRIPTION.is_empty() || DESCRIPTION.chars().count() > 1024 {
        return Err(RenderError::FrontMatter(
            "`description` must be 1–1024 characters",
        ));
    }
    if DESCRIPTION.contains(": ") || DESCRIPTION.contains(" #") {
        return Err(RenderError::FrontMatter(
            "`description` must stay a plain YAML scalar",
        ));
    }
    Ok(())
}

fn read(cwd: &Path, path: &'static str) -> Result<String, RenderError> {
    let file = cwd.join(path);
    if !file.exists() {
        return Err(RenderError::Missing(path));
    }
    std::fs::read_to_string(&file).map_err(|source| RenderError::Read { path, source })
}

fn write(cwd: &Path, path: &'static str, content: &str) -> Result<(), RenderError> {
    let file = cwd.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|source| RenderError::Write { path, source })?;
    }
    std::fs::write(&file, content).map_err(|source| RenderError::Write { path, source })
}

fn generated_note(source: &str) -> String {
    format!(
        "<!-- Generated from {source} by `cargo run --example render-skill`; \
         edit the source, not this file. -->\n\n"
    )
}

/// Read the sources and write the skill, relative to `cwd`.
fn run(cwd: &Path, stdout: &mut dyn Write) -> Result<(), RenderError> {
    check_front_matter()?;
    let body = read(cwd, BODY_SOURCE)?;
    let guide = read(cwd, ZH_SOURCE)?;
    let lexicon = lexicon::render(&read(cwd, LEXICON_SOURCE)?, Detail::Full)?;

    let skill = format!(
        "---\nname: {NAME}\ndescription: {DESCRIPTION}\n---\n\n{}{body}",
        generated_note(BODY_SOURCE)
    );
    write(cwd, SKILL_TARGET, &skill)?;
    write(
        cwd,
        ZH_TARGET,
        &format!("{}{guide}", generated_note(ZH_SOURCE)),
    )?;
    write(
        cwd,
        LEXICON_TARGET,
        &format!("{}{lexicon}", generated_note(LEXICON_SOURCE)),
    )?;
    writeln!(stdout, "{TARGET_DIR}: 3 files").map_err(|source| RenderError::Write {
        path: TARGET_DIR,
        source,
    })
}

fn main() -> ExitCode {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(source) => {
            let _ = writeln!(stderr, "cannot determine the working directory: {source}");
            return ExitCode::FAILURE;
        }
    };
    match run(&cwd, &mut stdout) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            ExitCode::FAILURE
        }
    }
}
