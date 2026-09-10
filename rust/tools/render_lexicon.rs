//! Render the lexicon into one static page — the Rust side of a two-generator
//! contract.
//!
//! `tools/render_lexicon.py` is the reference implementation; this is its port.
//! Both read `spec/lexicon/zh.toml` and write `site/index.html`, and the page
//! is committed, so the two must agree byte for byte: the page is the product,
//! and two products from one source are two products. What holds them together
//! is `tools/check_lexicon_render.sh`, which runs both over the same source and
//! compares the bytes.
//!
//! The HTML and CSS live in `rust/templates/lexicon.html`, an [Askama]
//! template compiled into this example (`askama.toml` at the root names the
//! directory). This file reads the source, sorts the entries, escapes and
//! marks up the fields, and hands the template the shape it prints.
//!
//! [Askama]: https://docs.rs/askama/latest/askama/
//!
//! Two separate things decide where this file lives and how it is built. The
//! Cargo target is `[[example]]` because that is the mechanism that keeps it
//! out of `cargo install`: the shipped binary set is `limae` alone, and
//! `diff-probe` sits under the same declaration for the same reason. The
//! directory is `rust/tools/` because that is what the file is — a
//! development-time tool, the same meaning the repository root's `tools/`
//! carries. The two do not have to agree, because `Cargo.toml` spells the
//! `path` out and never consults Cargo's `examples/` auto-discovery. Either
//! way the file is packaged (`Cargo.toml` carries `rust/**/*.rs`) and compiled
//! by `cargo test`, while nothing about a release build has to change.
//! Compiling is not enough on its own: the source above is read at run time,
//! so `Cargo.toml` also has to `include` it, and the packaging gate runs this
//! example out of the unpacked package to prove it shipped.
//!
//! Run it from the repository root:
//!
//! ```sh
//! cargo run --example render-lexicon
//! ```

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use askama::Template;
use thiserror::Error;
use toml::Value;

const SOURCE: &str = "spec/lexicon/zh.toml";
const TARGET: &str = "site/index.html";
const TARGET_DIR: &str = "site";

/// One before/after pair of an entry.
struct Example {
    before: String,
    after: String,
}

/// One lexicon entry, in the shape the page needs.
struct Entry {
    term: String,
    pinyin: Vec<String>,
    plain: String,
    gloss: String,
    fault: String,
    examples: Vec<Example>,
}

/// The whole lexicon.
struct Lexicon {
    preface: Vec<String>,
    standard: String,
    threshold: String,
    entries: Vec<Entry>,
}

#[derive(Debug, Error)]
enum RenderError {
    #[error("not found: {SOURCE}; run from the root")]
    Missing,
    #[error("cannot read {SOURCE}: {0}")]
    Read(#[source] io::Error),
    #[error("cannot parse {SOURCE}: {0}")]
    Parse(#[source] Box<toml::de::Error>),
    #[error("{SOURCE}: `{key}` must be {expected}")]
    Shape { key: String, expected: &'static str },
    #[error("{SOURCE}: `{key}` holds {ch:?}, which has no known tone-free form")]
    Pinyin { key: String, ch: char },
    #[error("cannot render {TARGET}: {0}")]
    Render(#[source] askama::Error),
    #[error("cannot write {TARGET}: {0}")]
    Write(#[source] io::Error),
}

/// Escape one string the way Python's `html.escape` does by default.
///
/// The default escapes the quotes too, and the page relies on that: the
/// reference implementation passes attribute text through the same call.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Escape one line and give its back-quoted spans a code face.
fn inline(text: &str) -> String {
    let escaped = escape(text);
    let mut out = String::with_capacity(escaped.len());
    for (index, part) in escaped.split('`').enumerate() {
        if index % 2 == 0 {
            out.push_str(part);
        } else {
            out.push_str("<code>");
            out.push_str(part);
            out.push_str("</code>");
        }
    }
    out
}

/// Fold one pinyin letter to the letter it is a toned form of.
///
/// The reference implementation strips the combining marks off the NFD form,
/// which over pinyin means exactly this table: the tone marks, the diaeresis
/// on `ü`, and anything already written as a combining mark. Unknown letters
/// are refused rather than passed through — a letter this table does not know
/// would sort here and in Python by different keys, and a wrong order is a
/// different page, so it is better to stop than to guess.
fn fold(ch: char) -> Option<char> {
    match ch {
        '\u{0300}'..='\u{036f}' => None,
        'ā' | 'á' | 'ǎ' | 'à' => Some('a'),
        'ē' | 'é' | 'ě' | 'è' | 'ê' => Some('e'),
        'ī' | 'í' | 'ǐ' | 'ì' => Some('i'),
        'ō' | 'ó' | 'ǒ' | 'ò' => Some('o'),
        'ū' | 'ú' | 'ǔ' | 'ù' | 'ü' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => Some('u'),
        'ń' | 'ň' | 'ǹ' => Some('n'),
        'ḿ' => Some('m'),
        _ => Some(ch),
    }
}

/// Build the sort key of one entry: its pinyin with the tone marks stripped.
fn sound(entry: &Entry, index: usize) -> Result<String, RenderError> {
    let mut out = String::new();
    for syllable in &entry.pinyin {
        for ch in syllable.chars() {
            if ch.is_ascii() {
                out.push(ch);
                continue;
            }
            match fold(ch) {
                None => {}
                Some(folded) if folded.is_ascii() => out.push(folded),
                Some(_) => {
                    return Err(RenderError::Pinyin {
                        key: format!("entry[{index}].pinyin"),
                        ch,
                    });
                }
            }
        }
    }
    Ok(out)
}

const NUMERALS: &str = "一二三四五六七八九十";

/// One character of a term with the pinyin written over it.
struct Cell {
    pinyin: String,
    glyph: char,
}

/// One before/after pair, marked up for the page.
struct ExampleView {
    numeral: char,
    before: String,
    after: String,
}

/// One entry, marked up for the page.
struct EntryView {
    term: String,
    cells: Vec<Cell>,
    plain: String,
    gloss: String,
    fault: String,
    examples: Vec<ExampleView>,
}

/// The whole page: `rust/templates/lexicon.html` over the marked-up lexicon.
///
/// The template escapes what it prints. The fields that `inline` has already
/// escaped and given `<code>` spans are printed through `|safe` there; the
/// division is written down at the top of the template.
#[derive(Template)]
#[template(path = "lexicon.html")]
struct Page {
    preface: Vec<String>,
    standard: String,
    threshold: String,
    entries: Vec<EntryView>,
}

/// Mark one entry up for the page.
fn view(entry: &Entry) -> EntryView {
    let cells = entry
        .term
        .chars()
        .enumerate()
        .map(|(index, glyph)| Cell {
            pinyin: entry.pinyin.get(index).cloned().unwrap_or_default(),
            glyph,
        })
        .collect();
    let examples = entry
        .examples
        .iter()
        .enumerate()
        .map(|(order, example)| ExampleView {
            numeral: NUMERALS.chars().nth(order).unwrap_or('?'),
            before: inline(&example.before),
            after: inline(&example.after),
        })
        .collect();
    EntryView {
        term: entry.term.clone(),
        cells,
        plain: inline(&entry.plain),
        gloss: inline(&entry.gloss),
        fault: inline(&entry.fault),
        examples,
    }
}

/// Build the whole page.
fn render(lexicon: &Lexicon, ordered: &[&Entry]) -> Result<String, RenderError> {
    let page = Page {
        preface: lexicon.preface.iter().map(|p| inline(p)).collect(),
        standard: inline(&lexicon.standard),
        threshold: inline(lexicon.threshold.trim()),
        entries: ordered.iter().map(|entry| view(entry)).collect(),
    };
    page.render().map_err(RenderError::Render)
}

/// Read one required string out of a table.
fn string(table: &Value, key: &str) -> Result<String, RenderError> {
    table
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| RenderError::Shape {
            key: key.to_owned(),
            expected: "a string",
        })
}

/// Read one required array of strings out of a table.
fn strings(table: &Value, key: &str) -> Result<Vec<String>, RenderError> {
    let shape = || RenderError::Shape {
        key: key.to_owned(),
        expected: "an array of strings",
    };
    table
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(shape)?
        .iter()
        .map(|item| item.as_str().map(str::to_owned).ok_or_else(shape))
        .collect()
}

/// Parse the source into the shape the page needs.
fn parse(text: &str) -> Result<Lexicon, RenderError> {
    let document: Value =
        toml::from_str(text).map_err(|source| RenderError::Parse(Box::new(source)))?;
    let raw_entries = document
        .get("entry")
        .and_then(Value::as_array)
        .ok_or_else(|| RenderError::Shape {
            key: "entry".to_owned(),
            expected: "an array of tables",
        })?;
    let mut entries = Vec::with_capacity(raw_entries.len());
    for (index, raw) in raw_entries.iter().enumerate() {
        let at = |field: &str| format!("entry[{index}].{field}");
        let mut examples = Vec::new();
        let raw_examples = raw
            .get("examples")
            .and_then(Value::as_array)
            .ok_or_else(|| RenderError::Shape {
                key: at("examples"),
                expected: "an array of tables",
            })?;
        for (position, example) in raw_examples.iter().enumerate() {
            examples.push(Example {
                before: string(example, "before").map_err(|_| RenderError::Shape {
                    key: at(&format!("examples[{position}].before")),
                    expected: "a string",
                })?,
                after: string(example, "after").map_err(|_| RenderError::Shape {
                    key: at(&format!("examples[{position}].after")),
                    expected: "a string",
                })?,
            });
        }
        entries.push(Entry {
            term: string(raw, "term").map_err(|_| RenderError::Shape {
                key: at("term"),
                expected: "a string",
            })?,
            pinyin: strings(raw, "pinyin").map_err(|_| RenderError::Shape {
                key: at("pinyin"),
                expected: "an array of strings",
            })?,
            plain: string(raw, "plain").map_err(|_| RenderError::Shape {
                key: at("plain"),
                expected: "a string",
            })?,
            gloss: string(raw, "gloss").map_err(|_| RenderError::Shape {
                key: at("gloss"),
                expected: "a string",
            })?,
            fault: string(raw, "fault").map_err(|_| RenderError::Shape {
                key: at("fault"),
                expected: "a string",
            })?,
            examples,
        });
    }
    Ok(Lexicon {
        preface: strings(&document, "preface")?,
        standard: string(&document, "standard")?,
        threshold: string(&document, "threshold")?,
        entries,
    })
}

/// Read the lexicon and write the page, relative to `cwd`.
fn run(cwd: &Path, stdout: &mut dyn Write) -> Result<(), RenderError> {
    let source = cwd.join(SOURCE);
    if !source.exists() {
        return Err(RenderError::Missing);
    }
    let text = std::fs::read_to_string(&source).map_err(RenderError::Read)?;
    let lexicon = parse(&text)?;
    let mut ordered: Vec<(String, &Entry)> = Vec::with_capacity(lexicon.entries.len());
    for (index, entry) in lexicon.entries.iter().enumerate() {
        ordered.push((sound(entry, index)?, entry));
    }
    ordered.sort_by(|left, right| left.0.cmp(&right.0));
    let ordered: Vec<&Entry> = ordered.into_iter().map(|(_, entry)| entry).collect();
    let page = render(&lexicon, &ordered)?;
    std::fs::create_dir_all(cwd.join(TARGET_DIR)).map_err(RenderError::Write)?;
    std::fs::write(cwd.join(TARGET), page).map_err(RenderError::Write)?;
    writeln!(stdout, "{TARGET}: {} entries", lexicon.entries.len()).map_err(RenderError::Write)
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
