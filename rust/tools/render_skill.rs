//! Assemble the agent skill from its sources.
//!
//! Writes `skills/write-naturally/` — the skill a person copies into their
//! agent so that the model keeps limae's rules while it writes, rather than
//! having its text polished afterwards (ADR-0016 §七). Three files, each
//! from one source:
//!
//! - `SKILL.md`: `spec/skill/SKILL.md`, front matter and body, with the
//!   generated-file note inserted between them. The front matter (`name`,
//!   `description`) lives in the source because it is content — what the
//!   skill is called and when a model should reach for it — not packaging;
//!   this tool only checks it against the agentskills.io rules and against
//!   the directory it writes to.
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

/// The skill's directory; the source front matter's `name` must be its last
/// component (the one agentskills.io rule that ties the file to where it is
/// installed), so a rename is a `git mv` plus an edit of the source.
const TARGET_DIR: &str = "skills/write-naturally";

const BODY_SOURCE: &str = "spec/skill/SKILL.md";
const ZH_SOURCE: &str = "spec/skill/zh.md";
const LEXICON_SOURCE: &str = "spec/lexicon/zh.toml";

const SKILL_TARGET: &str = "skills/write-naturally/SKILL.md";
const ZH_TARGET: &str = "skills/write-naturally/references/zh/guide.md";
const LEXICON_TARGET: &str = "skills/write-naturally/references/zh/lexicon.md";

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

/// The source `SKILL.md` split at its front matter.
struct Source<'a> {
    /// The front matter, `---` fences included, ending in a newline.
    front_matter: &'a str,
    /// Everything after the closing fence.
    body: &'a str,
    name: &'a str,
    description: &'a str,
}

/// Split the source at its YAML front matter and read the two keys the
/// specification requires.
///
/// The front matter is two plain scalars on their own lines, so this reads
/// it as lines rather than pulling in a YAML parser; a value that would need
/// one (`: ` or ` #` inside it, a block scalar) is refused instead.
fn split_source(source: &str) -> Result<Source<'_>, RenderError> {
    let inner = source
        .strip_prefix("---\n")
        .ok_or(RenderError::FrontMatter("source must open with `---`"))?;
    let end = inner.find("\n---\n").ok_or(RenderError::FrontMatter(
        "source front matter is not closed",
    ))?;
    let fields = &inner[..end];
    let front_len = "---\n".len() + end + "\n---\n".len();
    let (mut name, mut description) = (None, None);
    for line in fields.lines() {
        let (key, value) = line.split_once(':').ok_or(RenderError::FrontMatter(
            "front matter line is not `key: value`",
        ))?;
        let value = value.trim();
        match key {
            "name" => name = Some(value),
            "description" => description = Some(value),
            _ => return Err(RenderError::FrontMatter("unknown front matter key")),
        }
    }
    Ok(Source {
        front_matter: &source[..front_len],
        body: &source[front_len..],
        name: name.ok_or(RenderError::FrontMatter("front matter has no `name`"))?,
        description: description.ok_or(RenderError::FrontMatter(
            "front matter has no `description`",
        ))?,
    })
}

/// Refuse a front matter the specification would reject.
///
/// The values come from `spec/skill/SKILL.md`, so this runs against the
/// person who edits that file, not against user input: it is the shortest
/// way to keep the rule next to the place the value is checked.
fn check_front_matter(source: &Source<'_>) -> Result<(), RenderError> {
    let directory = Path::new(TARGET_DIR)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RenderError::FrontMatter("target directory has no name"))?;
    if source.name != directory {
        return Err(RenderError::FrontMatter(
            "`name` must equal the skill's directory name",
        ));
    }
    if source.name.is_empty() || source.name.len() > 64 {
        return Err(RenderError::FrontMatter("`name` must be 1–64 characters"));
    }
    if !source
        .name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(RenderError::FrontMatter(
            "`name` must be lowercase letters, digits and hyphens",
        ));
    }
    if source.name.starts_with('-') || source.name.ends_with('-') || source.name.contains("--") {
        return Err(RenderError::FrontMatter(
            "`name` must not start or end with a hyphen, or contain `--`",
        ));
    }
    // The value is taken as the text after `description:`, not as parsed
    // YAML, so what this tool accepts has to be a closed set: a plain scalar
    // that any YAML reader gives back verbatim. Anything that would start a
    // quoted, flow, block or otherwise special scalar is refused rather than
    // read literally — `""` is the case that would otherwise pass as two
    // characters and reach a validator as an empty description.
    if source.description.is_empty() || source.description.chars().count() > 1024 {
        return Err(RenderError::FrontMatter(
            "`description` must be 1–1024 characters",
        ));
    }
    if source.description.starts_with([
        '"', '\'', '|', '>', '&', '*', '!', '%', '@', '`', '[', '{', '-', '?', ':', ',',
    ]) {
        return Err(RenderError::FrontMatter(
            "`description` must be a plain YAML scalar: no quotes, block or flow indicator",
        ));
    }
    if source.description.contains(": ") || source.description.contains(" #") {
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
    let skill_source = read(cwd, BODY_SOURCE)?;
    let source = split_source(&skill_source)?;
    check_front_matter(&source)?;
    let guide = read(cwd, ZH_SOURCE)?;
    let lexicon = lexicon::render(&read(cwd, LEXICON_SOURCE)?, Detail::Full)?;

    let skill = format!(
        "{}\n{}{}",
        source.front_matter,
        generated_note(BODY_SOURCE),
        source.body.trim_start_matches('\n')
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
