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
//! This is an example rather than a `[[bin]]` because it is a development tool
//! and the shipped binary set is `limae` alone; `diff-probe` sits here for
//! the same reason. Examples are packaged (`Cargo.toml` carries
//! `rust/**/*.rs`) and compiled by `cargo test`, while nothing about a release
//! build has to change. Compiling is not enough on its own: the source above
//! is read at run time, so `Cargo.toml` also has to `include` it, and the
//! packaging gate runs this example out of the unpacked package to prove it
//! shipped.
//!
//! Run it from the repository root:
//!
//! ```sh
//! cargo run --example render-lexicon
//! ```

use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use thiserror::Error;
use toml::Value;

const SOURCE: &str = "spec/lexicon/zh.toml";
const TARGET: &str = "site/index.html";
const TARGET_DIR: &str = "site";

const STYLE: &str = r##"
:root {
  --paper: #f1e8d5;
  --ink: #24211c;
  --faded: #7a736a;
  --rule: #cfc5b4;
  --cinnabar: #a8433a;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 4rem 1.5rem 6rem;
  background: var(--paper);
  color: var(--ink);
  font-family: "Songti SC", "Noto Serif CJK SC", "Source Han Serif SC",
      "SimSun", "STSong", serif;
  line-height: 1.9;
}
.page {
  max-width: 62rem; margin: 0 auto; display: grid; gap: 3rem;
  grid-template-columns: 11rem minmax(0, 1fr);
  align-items: start;
}
.sheet { max-width: 44rem; }
nav {
  position: sticky; top: 3rem; border-right: 1px solid var(--rule);
  padding-right: 1.2rem; font-size: .9rem;
}
nav .toc-label {
  color: var(--cinnabar); letter-spacing: .3rem; font-size: .75rem;
  margin-bottom: .9rem;
}
nav a {
  display: block; color: var(--ink); text-decoration: none;
  padding: .3rem 0; border-bottom: 1px solid transparent;
}
nav a:hover, nav a:focus-visible {
  color: var(--cinnabar); border-bottom-color: var(--rule);
}
@media (max-width: 52rem) {
  .page { grid-template-columns: 1fr; gap: 2rem; }
  nav {
    position: static; border-right: none;
    border-bottom: 1px solid var(--rule); padding: 0 0 1.2rem;
    columns: 2; column-gap: 1.5rem;
  }
}
:focus-visible { outline: 2px solid var(--cinnabar); outline-offset: 3px; }
h1 {
  font-size: 2.4rem; font-weight: normal; letter-spacing: .5rem;
  margin: 0 0 .4rem; text-align: center;
}
.subtitle {
  text-align: center; color: var(--faded); letter-spacing: .2rem;
  margin: 0 0 3rem; font-size: .95rem;
}
.intro { margin: 0 0 2.6rem; }
.intro p { margin: 0 0 1rem; }
.preface {
  border-top: 1px solid var(--rule); border-bottom: 1px solid var(--rule);
  padding: 1.6rem 0; margin: 0 0 4rem; color: var(--faded); font-size: .95rem;
}
.preface p { margin: .4rem 0; }
.preface b { color: var(--ink); font-weight: normal; }
.entry { margin: 0 0 4.5rem; }
.cells { display: flex; flex-wrap: wrap; gap: .5rem; margin-bottom: 1.4rem; }
.cell { width: 4.6rem; text-align: center; }
.pinyin {
  font-family: Georgia, "Times New Roman", serif;
  font-size: .78rem; color: var(--faded); letter-spacing: .02rem;
  height: 1.5rem; display: flex; align-items: center;
  justify-content: center; position: relative; margin-bottom: .25rem;
}
/* 四线三格：拼音在字帖里写在这里，不是浮在字上面 */
.pinyin::before {
  content: ""; position: absolute; inset: 0;
  border-top: 1px solid var(--cinnabar);
  border-bottom: 1px solid var(--cinnabar);
  opacity: .3;
  background:
      linear-gradient(var(--cinnabar), var(--cinnabar)) 0 33.3% / 100% 1px
          no-repeat,
      linear-gradient(var(--cinnabar), var(--cinnabar)) 0 66.6% / 100% 1px
          no-repeat;
}
.pinyin span { position: relative; }
.grid {
  width: 4.6rem; height: 4.6rem; border: 1px solid var(--cinnabar);
  position: relative; display: flex; align-items: center;
  justify-content: center; background: rgba(255,255,255,.45);
}
.grid::before, .grid::after {
  content: ""; position: absolute; border-color: var(--cinnabar);
  opacity: .38;
}
.grid::before { left: 50%; top: 0; bottom: 0; border-left: 1px dashed; }
.grid::after { top: 50%; left: 0; right: 0; border-top: 1px dashed; }
/* 楷体是字帖里的那一种，宋体不是 */
.glyph {
  font-family: "Kaiti SC", "KaiTi", "STKaiti", "AR PL UKai CN",
      "Noto Serif CJK SC", serif;
  font-size: 2.9rem; line-height: 1; position: relative;
}
.plain { font-size: 1.35rem; margin: 0 0 .6rem; }
.plain .label, .gloss .label, .fault .label, .eg .mark {
  color: var(--cinnabar); font-family: "PingFang SC", "Noto Sans CJK SC",
      "Source Han Sans SC", "Heiti SC", "Microsoft YaHei", sans-serif;
  font-weight: 800; background: rgba(168,67,58,.10); border-radius: .2rem;
  padding: .05rem .3rem; font-size: .9rem;
}
.plain .label, .gloss .label, .fault .label { margin-right: .8rem; }
.gloss, .fault { color: var(--faded); }
.gloss { margin: 0 0 .6rem; }
.fault { margin: 0 0 1.4rem; }
.eg { border-left: 2px solid var(--rule); padding: .1rem 0 .1rem 1.1rem;
     margin: 0 0 1rem; }
.eg .eg-no {
  color: var(--faded); font-family: "PingFang SC", "Noto Sans CJK SC",
     "Source Han Sans SC", "Heiti SC", "Microsoft YaHei", sans-serif;
     font-weight: 800; font-size: .8rem; letter-spacing: .1em;
     margin-bottom: .2rem; }
.eg .before { color: var(--faded); }
.eg .after { color: var(--ink); }
.eg .mark { margin-right: .6rem; }
code {
  font-family: ui-monospace, "SF Mono", Menlo, monospace;
  font-size: .88em; background: rgba(0,0,0,.045); padding: .05em .3em;
}
footer {
  margin-top: 5rem; padding-top: 1.4rem; border-top: 1px solid var(--rule);
  color: var(--faded); font-size: .85rem; text-align: center;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --paper: #191714; --ink: #e6e0d4; --faded: #9a9184;
    --rule: #3b362e; --cinnabar: #c2695d;
  }
  :root:not([data-theme="light"]) .grid { background: rgba(255,255,255,.03); }
  :root:not([data-theme="light"]) code { background: rgba(255,255,255,.07); }
}
:root[data-theme="dark"] {
  --paper: #191714; --ink: #e6e0d4; --faded: #9a9184;
  --rule: #3b362e; --cinnabar: #c2695d;
}
"##;

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

/// Lay one term out as one grid square per character.
fn cells(term: &str, pinyin: &[String]) -> String {
    let mut out = String::new();
    out.push_str("<div class=\"cells\">");
    for (index, ch) in term.chars().enumerate() {
        let empty = String::new();
        let sound = pinyin.get(index).unwrap_or(&empty);
        let _ = write!(
            out,
            "<div class=\"cell\"><div class=\"pinyin\">\
             <span>{}</span></div>\
             <div class=\"grid\"><span class=\"glyph\">{}</span>\
             </div></div>",
            escape(sound),
            escape(&ch.to_string()),
        );
    }
    out.push_str("</div>");
    out
}

const NUMERALS: &str = "一二三四五六七八九十";

/// Build the whole page.
fn render(lexicon: &Lexicon, ordered: &[&Entry]) -> String {
    let mut page = String::new();
    let _ = write!(page, "<title>机器文言</title><style>{STYLE}</style>");
    page.push_str("<div class=\"page\"><nav><div class=\"toc-label\">目录</div>");
    for (index, entry) in ordered.iter().enumerate() {
        let _ = write!(page, "<a href=\"#w{index}\">{}</a>", escape(&entry.term),);
    }
    page.push_str("</nav><div class=\"sheet\"><h1>机器文言</h1>");
    page.push_str("<p class=\"subtitle\">机器写的中文里，读得懂却没人这么说的词</p>");
    page.push_str("<div class=\"intro\">");
    for paragraph in &lexicon.preface {
        let _ = write!(page, "<p>{}</p>", inline(paragraph));
    }
    page.push_str("</div>");
    let _ = write!(
        page,
        "<div class=\"preface\"><p><b>判据</b>　{}</p>\
         <p><b>门槛</b>　{}</p></div>",
        inline(&lexicon.standard),
        inline(lexicon.threshold.trim()),
    );
    for (index, entry) in ordered.iter().enumerate() {
        let _ = write!(
            page,
            "<section class=\"entry\" id=\"w{index}\">{}\
             <p class=\"plain\"><span class=\"label\">白</span>{}</p>\
             <p class=\"gloss\"><span class=\"label\">解</span>{}</p>\
             <p class=\"fault\"><span class=\"label\">病</span>{}</p>",
            cells(&entry.term, &entry.pinyin),
            inline(&entry.plain),
            inline(&entry.gloss),
            inline(&entry.fault),
        );
        for (order, example) in entry.examples.iter().enumerate() {
            let numeral = NUMERALS.chars().nth(order).unwrap_or('?');
            let _ = write!(
                page,
                "<div class=\"eg\"><div class=\"eg-no\">例{numeral}</div>\
                 <div class=\"before\"><span class=\"mark\">原</span>{}</div>\
                 <div class=\"after\"><span class=\"mark\">改</span>{}</div></div>",
                inline(&example.before),
                inline(&example.after),
            );
        }
        page.push_str("</section>");
    }
    page.push_str(
        "<footer>正文在 <code>spec/lexicon/zh.toml</code>，\
         本页由 <code>tools/render_lexicon.py</code> 生成</footer></div></div>",
    );
    page
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
    let page = render(&lexicon, &ordered);
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
