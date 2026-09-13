//! The AI-Chinese lexicon, rendered as Markdown.
//!
//! `spec/lexicon/zh.toml` is the one source of the lexicon: the site, the
//! polish prompt and the skill all read it, none of them carries a copy. This
//! module is the renderer the two prose consumers share. `limae polish`
//! embeds the TOML at build time (the same way `rust/resources.rs` embeds the
//! wordlists) and renders it into the Chinese layer of its prompt at run
//! time; the `render-skill` Cargo example calls the same function to write
//! `skills/limae/references/zh/lexicon.md`. One function, two callers, so the
//! two products cannot disagree about what an entry says.
//!
//! The two callers want different amounts of it, which is what [`Detail`]
//! selects. The skill is read on demand by a model about to write Chinese,
//! and gets everything a writer can use: the standard, each entry's meaning,
//! what is wrong with it, and the before/after examples. The polish prompt is
//! loaded whole on every call, and the experiment that put the lexicon there
//! (`docs/tracker.md`, 2026-09-12) showed that the *entries* move the
//! result while prompt length buys nothing; so it gets each term with what to
//! write instead and why, and nothing that the surrounding sentence already
//! tells the model.
//!
//! The parser reads only the fields the Markdown prints. `render-lexicon` and
//! `lint-lexicon` each parse the same file for their own purposes (the page
//! needs pinyin, the linter needs byte spans); they are not this parser and
//! this parser is not theirs.

use thiserror::Error;
use toml::Value;

/// Where the embedded lexicon lives, for diagnostics.
pub const SOURCE: &str = "spec/lexicon/zh.toml";

/// How much of each entry the Markdown carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    /// The standard, and every entry with its meaning, fault and examples.
    Full,
    /// Every entry with what to write instead and why; nothing else.
    Brief,
}

/// The lexicon could not be parsed into the shape the Markdown needs.
#[derive(Debug, Error)]
pub enum LexiconError {
    #[error("cannot parse {SOURCE}: {source}")]
    Parse {
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("{SOURCE}: `{key}` must be {expected}")]
    Shape { key: String, expected: &'static str },
}

struct Example {
    before: String,
    after: String,
}

struct Entry {
    term: String,
    plain: String,
    gloss: String,
    fault: String,
    examples: Vec<Example>,
}

struct Lexicon {
    title: String,
    standard: String,
    entries: Vec<Entry>,
}

fn string(table: &Value, key: &str, at: &dyn Fn(&str) -> String) -> Result<String, LexiconError> {
    table
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| LexiconError::Shape {
            key: at(key),
            expected: "a string",
        })
}

fn parse(text: &str) -> Result<Lexicon, LexiconError> {
    let document: Value = toml::from_str(text).map_err(|mut source: toml::de::Error| {
        source.set_input(None);
        LexiconError::Parse {
            source: Box::new(source),
        }
    })?;
    let top = |key: &str| key.to_owned();
    let raw_entries = document
        .get("entry")
        .and_then(Value::as_array)
        .ok_or_else(|| LexiconError::Shape {
            key: "entry".to_owned(),
            expected: "an array of tables",
        })?;
    let mut entries = Vec::with_capacity(raw_entries.len());
    for (index, raw) in raw_entries.iter().enumerate() {
        let at = |field: &str| format!("entry[{index}].{field}");
        let raw_examples = raw
            .get("examples")
            .and_then(Value::as_array)
            .ok_or_else(|| LexiconError::Shape {
                key: at("examples"),
                expected: "an array of tables",
            })?;
        let mut examples = Vec::with_capacity(raw_examples.len());
        for (position, example) in raw_examples.iter().enumerate() {
            let at = |field: &str| format!("entry[{index}].examples[{position}].{field}");
            examples.push(Example {
                before: string(example, "before", &at)?,
                after: string(example, "after", &at)?,
            });
        }
        entries.push(Entry {
            term: string(raw, "term", &at)?,
            plain: string(raw, "plain", &at)?,
            gloss: string(raw, "gloss", &at)?,
            fault: string(raw, "fault", &at)?,
            examples,
        });
    }
    Ok(Lexicon {
        title: string(&document, "title", &top)?,
        standard: string(&document, "standard", &top)?,
        entries,
    })
}

/// Render the lexicon TOML as Markdown.
///
/// Entries come out in source order. The labels (白 / 解 / 病 / 原 / 改) are
/// the ones the published page uses, so a reader who has seen one has seen
/// the other.
pub fn render(text: &str, detail: Detail) -> Result<String, LexiconError> {
    let lexicon = parse(text)?;
    let mut out = format!("# {}\n", lexicon.title);
    if detail == Detail::Full {
        out.push('\n');
        out.push_str(&lexicon.standard);
        out.push('\n');
    }
    for entry in &lexicon.entries {
        out.push_str("\n## ");
        out.push_str(&entry.term);
        out.push_str("\n\n- 白：");
        out.push_str(&entry.plain);
        out.push('\n');
        if detail == Detail::Full {
            out.push_str("- 解：");
            out.push_str(&entry.gloss);
            out.push('\n');
        }
        out.push_str("- 病：");
        out.push_str(&entry.fault);
        out.push('\n');
        if detail == Detail::Full {
            for example in &entry.examples {
                out.push_str("- 原：");
                out.push_str(&example.before);
                out.push_str("\n  改：");
                out.push_str(&example.after);
                out.push('\n');
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "lexicon_tests.rs"]
mod tests;
