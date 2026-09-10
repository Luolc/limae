//! Check the lexicon's Chinese prose with limae's own typography rules.
//!
//! `spec/lexicon/zh.toml` carries well over a hundred fields written to be
//! read by a person — the title, the preface, the standard and the threshold,
//! and every entry's `plain` / `gloss` / `fault` and before/after examples.
//! The `limae` pre-commit hook selects `\.md$`, so none of it was ever
//! checked: a lexicon about how Chinese should be written had no gate on its
//! own typography. This example is that gate. It takes lexicon paths on the
//! command line (the hook passes whatever matched
//! `^spec/lexicon/.*\.toml$`), hands each prose field to the library
//! pipeline, and prints findings against the *TOML* line they sit on.
//!
//! Like `render-lexicon` and `diff-probe` this is a development-time tool:
//! `rust/tools/` says what it is, `[[example]]` is what keeps it out of
//! `cargo install`.
//!
//! # Only typography, never the lexical families
//!
//! Every experimental rule is switched off by name, derived from
//! [`limae::config::RULES`] rather than a list copied here. That is not a
//! preference: `examples[].before` holds the specimens this lexicon
//! *collects*, so `zh-tell-*` / `en-tell-*` / `zh-word-*` would fire on the
//! very words each entry exists to document. Going through
//! [`limae::config::resolve`] with the switches present also means no
//! `limae.toml` discovery — a repository that turned the experimental
//! families on for its Markdown must not thereby turn them on here.
//!
//! That reason covers `examples[].before` alone. `gloss`, `fault`,
//! `preface`, `threshold`, `title` and `subtitle` are the lexicon speaking
//! in its own voice, and they are exactly the prose the lexical families
//! were written for; switching those families off over them is a
//! conservative choice, not a principle. What it costs is visible in
//! `tools/check_lexicon_lint.sh`: the sentence its lexical arm proves is
//! going unreported is a `gloss`, not a collected specimen.
//!
//! # Why not the `render-lexicon` types
//!
//! Reporting a real TOML line needs each field's byte span, so the fields are
//! [`toml::Spanned<String>`] rather than `String`, and a Cargo example cannot
//! import another example's private types in any case. `deny_unknown_fields`
//! keeps this from silently drifting into a second, shorter field list: a
//! field added to the lexicon and not decided about here fails the parse
//! instead of going unchecked, which is the failure this whole gate exists to
//! stop. `term` and `pinyin` are declared and deliberately not linted — one
//! word and a row of Latin syllables have no typography to check.
//!
//! Run it from the repository root:
//!
//! ```sh
//! cargo run --example lint-lexicon -- spec/lexicon/zh.toml
//! ```

use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use limae::config::{
    CliOverrides, ConfigError, Maturity, RULES, ResolvedConfig, Severity, resolve,
};
use limae::directives::DirectiveError;
use limae::pipeline::{InitError, Pipeline};
use serde::Deserialize;
use thiserror::Error;
use toml::Spanned;

/// The whole lexicon, in the shape this check needs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Lexicon {
    title: Spanned<String>,
    subtitle: Spanned<String>,
    preface: Vec<Spanned<String>>,
    standard: Spanned<String>,
    threshold: Spanned<String>,
    entry: Vec<Entry>,
}

/// One lexicon entry.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    #[expect(
        dead_code,
        reason = "declared so `deny_unknown_fields` stays exhaustive"
    )]
    term: String,
    #[expect(
        dead_code,
        reason = "declared so `deny_unknown_fields` stays exhaustive"
    )]
    pinyin: Vec<String>,
    plain: Spanned<String>,
    gloss: Spanned<String>,
    fault: Spanned<String>,
    examples: Vec<Example>,
}

/// One before/after pair of an entry.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Example {
    before: Spanned<String>,
    after: Spanned<String>,
}

#[derive(Debug, Error)]
enum LintError {
    #[error("usage: lint-lexicon <lexicon.toml>...")]
    NoFiles,
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("cannot resolve the rule selection")]
    Config(#[from] ConfigError),
    #[error("cannot initialize the rule pipeline")]
    Init(#[from] InitError),
    #[error("{path}: {source}")]
    Directive {
        path: PathBuf,
        #[source]
        source: DirectiveError,
    },
    #[error("cannot write command output: {0}")]
    Output(#[from] io::Error),
}

/// Every prose field of one lexicon, in document order.
fn prose(lexicon: &Lexicon) -> Vec<&Spanned<String>> {
    let mut fields = vec![&lexicon.title, &lexicon.subtitle];
    fields.extend(&lexicon.preface);
    fields.push(&lexicon.standard);
    fields.push(&lexicon.threshold);
    for entry in &lexicon.entry {
        fields.push(&entry.plain);
        fields.push(&entry.gloss);
        fields.push(&entry.fault);
        for example in &entry.examples {
            fields.push(&example.before);
            fields.push(&example.after);
        }
    }
    fields
}

/// One-based line holding a byte offset.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

/// One line of the source, without its carriage return.
fn source_line(text: &str, line: usize) -> Option<&str> {
    text.split('\n')
        .nth(line - 1)
        .map(|found| found.strip_suffix('\r').unwrap_or(found))
}

/// Byte offset where a TOML string value's content begins.
///
/// The span a [`Spanned`] carries starts at the opening delimiter, so the
/// delimiter has to come off before an offset inside the value means
/// anything. A `"""` or `'''` swallows a single newline immediately after it,
/// which is what puts a multi-line value's first content line one below the
/// line the key sits on.
fn content_start(text: &str, span: &Range<usize>) -> usize {
    let raw = &text[span.start..span.end];
    for delimiter in ["\"\"\"", "'''"] {
        if let Some(rest) = raw.strip_prefix(delimiter) {
            let swallowed = if rest.starts_with("\r\n") {
                2
            } else {
                usize::from(rest.starts_with('\n'))
            };
            return span.start + delimiter.len() + swallowed;
        }
    }
    span.start + usize::from(raw.starts_with('"') || raw.starts_with('\''))
}

/// Translate a line within one field into a line of the TOML file.
///
/// Each field is checked as its own document, so the pipeline reports lines
/// within that field. Offsetting from where the field's content starts is
/// right whenever the value's lines are the source's lines — every field this
/// lexicon has today, since none of them uses an escape. It is not right in
/// general: a `\n` escape, or a line-ending backslash, makes a value line
/// that no source line matches. So the offset is only used once it has been
/// confirmed against the source; otherwise the field's own first line is
/// reported, which still points at the field that has the problem. Counting
/// fields instead would be wrong the moment an entry is added.
fn toml_line(text: &str, field: &Spanned<String>, line: usize) -> usize {
    let first = line_of(text, content_start(text, &field.span()));
    let candidate = first + line - 1;
    let value_line = field
        .get_ref()
        .split('\n')
        .nth(line - 1)
        .map(|found| found.strip_suffix('\r').unwrap_or(found));
    if value_line.is_some() && value_line == source_line(text, candidate) {
        candidate
    } else {
        first
    }
}

const fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

/// How many findings one lexicon produced, and how many of them are errors.
#[derive(Default)]
struct Report {
    errors: usize,
    total: usize,
}

/// Check one lexicon, printing one message per finding.
fn check_file(
    path: &Path,
    pipeline: &Pipeline,
    config: &ResolvedConfig,
    stdout: &mut dyn Write,
) -> Result<Report, LintError> {
    let text = std::fs::read_to_string(path).map_err(|source| LintError::Read {
        path: path.to_owned(),
        source,
    })?;
    let lexicon: Lexicon = toml::from_str(&text).map_err(|source| LintError::Parse {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    let mut messages = Vec::new();
    for field in prose(&lexicon) {
        let findings =
            pipeline
                .check(field.get_ref(), config)
                .map_err(|source| LintError::Directive {
                    path: path.to_owned(),
                    source,
                })?;
        for finding in findings {
            let severity = config.severity(finding.rule);
            messages.push((
                toml_line(&text, field, finding.line),
                severity,
                finding.name,
                finding.snippet.to_owned(),
            ));
        }
    }
    messages.sort_by_key(|(line, ..)| *line);
    for (line, severity, name, snippet) in &messages {
        writeln!(
            stdout,
            "{}:{line}: {}: [{name}] …{snippet}…",
            path.display(),
            severity_name(*severity),
        )?;
    }
    Ok(Report {
        errors: messages
            .iter()
            .filter(|(_, severity, ..)| *severity == Severity::Error)
            .count(),
        total: messages.len(),
    })
}

/// Check every named lexicon; the flag is whether the run is clean.
fn run(cwd: &Path, paths: &[PathBuf], stdout: &mut dyn Write) -> Result<bool, LintError> {
    if paths.is_empty() {
        return Err(LintError::NoFiles);
    }
    let disable: Vec<String> = RULES
        .iter()
        .filter(|rule| rule.maturity == Maturity::Experimental)
        .map(|rule| rule.name.to_owned())
        .collect();
    let config = resolve(
        cwd,
        CliOverrides {
            disable: Some(&disable),
            enable: None,
        },
    )?;
    let pipeline = Pipeline::new()?;
    let mut report = Report::default();
    for path in paths {
        let one = check_file(path, &pipeline, &config, stdout)?;
        report.errors += one.errors;
        report.total += one.total;
    }
    if report.total == 0 {
        writeln!(stdout, "OK: {} lexicon file(s) clean", paths.len())?;
        return Ok(true);
    }
    // Only an error fails the run, as in `limae` itself. A warning still has
    // to reach the summary: printing one and then calling the file clean is
    // a line that contradicts the line above it.
    writeln!(
        stdout,
        "\n{} error(s), {} warning(s) in the lexicon prose.",
        report.errors,
        report.total - report.errors,
    )?;
    Ok(report.errors == 0)
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
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match run(&cwd, &paths, &mut stdout) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            ExitCode::FAILURE
        }
    }
}
