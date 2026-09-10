//! What a message is worth polishing, and what a rewrite changed.
//!
//! Two jobs, one screen. The first is counting: a message that is short, or
//! long only because it carries code, is left alone (ADR-0009 section 二), and
//! [`prose_length`] is the measure that decides it. The second is showing: the
//! reader has just read the message, so reprinting it whole says nothing about
//! what moved, and a line-level diff of Chinese prose prints a paragraph to
//! show that one word changed. [`changes`] therefore renders the change itself
//! with a little of the sentence on either side.
//!
//! The reference implementation was Python's `hook.py`, since deleted, whose
//! alignment was `difflib.SequenceMatcher(None, before, after)` over the message's
//! characters. That matcher's particular block division — not "a diff", but
//! that one — is what the pairs and the count are built on, so [`opcodes`] is a
//! port of it rather than a substitute for it: the same longest-match rule, the
//! same tie-breaks, and the same `autojunk` pruning above 200 elements. A
//! different-but-reasonable diff would give a different count for the same
//! rewrite.
//!
//! Nothing here writes anything anywhere, and nothing here holds a credential
//! or a child process's output: the text it is given is the assistant message
//! and the rewrite of it, and all it returns is an excerpt of those.

use std::collections::HashMap;

/// Fence markers of `spec/rules.md` 「全局豁免」 first item.
///
/// A message that is long only because it carries code is a short message as
/// far as polishing is concerned.
pub const FENCES: [&str; 2] = ["```", "~~~"];

/// How many non-whitespace characters a message needs before it is polished.
///
/// ADR-0009 section 二 leaves the value open and names Gvozdev's
/// `CLAUDISH_MIN_CHARS` as the starting point.
pub const MIN_CHARS: usize = 200;

/// Newlines between the message and the block: one blank line.
pub const BLOCK_GAP: usize = 2;

/// What the block says when the rewrite came back the same as the input.
pub const UNCHANGED: &str = "无改动";

/// What it says when the rewrite moved only whitespace or the punctuation width
/// the deterministic rules own, which the pairs cannot show.
pub const TYPOGRAPHY_ONLY: &str = "仅排版改动";

/// What the pairs put on either side of a change, so the reader can see where
/// in the sentence it landed.
const CONTEXT: usize = 10;

/// Two changes closer than this read as one edit, not two.
const NEAR: usize = 6;

/// The separator between one pair and the next in [`changes`].
const PAIR_GAP: &str = "\n\n";

/// The one thing the deterministic rules do to punctuation: they pick a width.
///
/// Folding the widths together, and ignoring whitespace, leaves a string that
/// differs only when a character was added or removed — that is, when the model
/// changed something the rules do not own. Stripping the punctuation instead
/// would hide a real edit: `(注)` becoming `「注」` or `注` survives stripping
/// unchanged, and no rule in this repository deletes a bracket or turns one
/// into another.
const WIDTHS: [(char, char); 8] = [
    ('（', '('),
    ('）', ')'),
    ('，', ','),
    ('。', '.'),
    ('；', ';'),
    ('：', ':'),
    ('！', '!'),
    ('？', '?'),
];

/// Whether a character is whitespace to Python's `str`.
///
/// `char::is_whitespace` is the Unicode `White_Space` property; Python's
/// `str.isspace`, and so the `\s` of the reference implementation's patterns,
/// is that set plus the four file/group/record/unit separators. Counting a
/// character as prose that the reference implementation counts as a space would
/// move [`prose_length`] across [`MIN_CHARS`] on a message made of them.
fn space(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
}

/// Split text the way Python's `str.splitlines` does, dropping the separators.
///
/// Python breaks on more than `\n`: the vertical tab and form feed, the
/// file/group/record separators, `NEL`, and the two Unicode line separators;
/// `\r\n` is one break. `str::lines` knows only `\n`, which would join two
/// lines of the message into one and let a fence marker hide behind whatever
/// came before it.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut characters = text.char_indices().peekable();
    while let Some((at, character)) = characters.next() {
        let width = match character {
            '\n' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => character.len_utf8(),
            '\r' => {
                if characters.peek().is_some_and(|(_, next)| *next == '\n') {
                    let _ = characters.next();
                    2
                } else {
                    1
                }
            }
            _ => continue,
        };
        lines.push(&text[start..at]);
        start = at + width;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// Count the newlines a piece of text ends on.
///
/// Zero when it ends on anything else. The caller is subtracting what the host
/// has already painted from the gap it wants, so this counts newlines and not
/// whitespace: a trailing space is not a line that reached the screen.
#[must_use]
pub fn trailing_newlines(text: &str) -> usize {
    text.chars()
        .rev()
        .take_while(|character| *character == '\n')
        .count()
}

/// Count what there is to polish in one message.
///
/// The number of non-whitespace characters outside fenced code blocks. A line
/// whose first non-space run opens or closes a fence is a marker and is not
/// counted either way, so a message that is nothing but a code block counts
/// zero.
#[must_use]
pub fn prose_length(text: &str) -> usize {
    let mut total = 0;
    let mut in_fence = false;
    for line in split_lines(text) {
        let stripped = line.trim_start_matches(space);
        if FENCES.iter().any(|fence| stripped.starts_with(fence)) {
            in_fence = !in_fence;
        } else if !in_fence {
            total += line.chars().filter(|character| !space(*character)).count();
        }
    }
    total
}

/// Reduce one excerpt to what the deterministic rules cannot change.
///
/// The excerpt without whitespace and with full-width punctuation folded onto
/// its half-width twin.
fn folded(text: &str) -> String {
    text.chars()
        .filter(|character| !space(*character))
        .map(|character| {
            WIDTHS
                .iter()
                .find_map(|(wide, narrow)| (*wide == character).then_some(*narrow))
                .unwrap_or(character)
        })
        .collect()
}

/// Put one excerpt on one line.
///
/// Every run of whitespace becomes a single space, and the ends are trimmed:
/// an excerpt is allowed to straddle a line break, and a pair that printed the
/// break would put the change and its context on two lines of the block.
fn flat(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for character in text.chars() {
        if space(character) {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(character);
        }
    }
    out
}

/// What one opcode of the alignment says happened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    /// The two sides agree over this span.
    Equal,
    /// `before[i1..i2]` was rewritten as `after[j1..j2]`.
    Replace,
    /// `before[i1..i2]` is gone.
    Delete,
    /// `after[j1..j2]` is new.
    Insert,
}

impl Tag {
    /// The reference implementation's name for this tag.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::Replace => "replace",
            Self::Delete => "delete",
            Self::Insert => "insert",
        }
    }
}

/// One step of turning `before` into `after`, in character indices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Opcode {
    /// What happened over this span.
    pub tag: Tag,
    /// Where the span starts in `before`.
    pub i1: usize,
    /// Where it ends in `before`.
    pub i2: usize,
    /// Where the span starts in `after`.
    pub j1: usize,
    /// Where it ends in `after`.
    pub j2: usize,
}

/// One matching run: `a[i..i + size]` equals `b[j..j + size]`.
#[derive(Clone, Copy)]
struct Match {
    i: usize,
    j: usize,
    size: usize,
}

/// Above this many elements, `difflib` stops seeding matches on the elements
/// that are everywhere.
const AUTOJUNK: usize = 200;

/// Where each character of `b` occurs, minus the ones `autojunk` drops.
///
/// `difflib` builds this once per matcher and prunes it: at 200 elements or
/// more, any character occurring more than `len(b) / 100 + 1` times stops
/// seeding matches. The pruning changes which blocks are found — that is the
/// point of it, and skipping it here would give a different division of the
/// same rewrite, and so a different count on screen.
fn occurrences(b: &[char]) -> HashMap<char, Vec<usize>> {
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (index, character) in b.iter().enumerate() {
        b2j.entry(*character).or_default().push(index);
    }
    if b.len() >= AUTOJUNK {
        let limit = b.len() / 100 + 1;
        b2j.retain(|_, indices| indices.len() <= limit);
    }
    b2j
}

/// The longest matching run of `a[alo..ahi]` and `b[blo..bhi]`.
///
/// Of all longest runs, the one starting earliest in `a`, and of those the one
/// starting earliest in `b` — `difflib`'s tie-break, kept because it decides
/// where a repeated word aligns and therefore what the reader is shown.
fn longest_match(
    a: &[char],
    b: &[char],
    b2j: &HashMap<char, Vec<usize>>,
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> Match {
    let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
    let mut j2len: HashMap<usize, usize> = HashMap::new();
    for (i, character) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut newj2len: HashMap<usize, usize> = HashMap::new();
        for j in b2j.get(character).map_or(&[][..], Vec::as_slice) {
            let j = *j;
            if j < blo {
                continue;
            }
            if j >= bhi {
                break;
            }
            let k = j
                .checked_sub(1)
                .and_then(|previous| j2len.get(&previous))
                .unwrap_or(&0)
                + 1;
            let _ = newj2len.insert(j, k);
            if k > bestsize {
                (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
            }
        }
        j2len = newj2len;
    }
    // Extend over the characters that never seeded a match: `autojunk` keeps
    // the common ones from starting a block, not from belonging to one.
    while besti > alo && bestj > blo && a[besti - 1] == b[bestj - 1] {
        besti -= 1;
        bestj -= 1;
        bestsize += 1;
    }
    while besti + bestsize < ahi
        && bestj + bestsize < bhi
        && a[besti + bestsize] == b[bestj + bestsize]
    {
        bestsize += 1;
    }
    Match {
        i: besti,
        j: bestj,
        size: bestsize,
    }
}

/// The runs `a` and `b` have in common, in order, ending on the empty sentinel.
fn matching_blocks(a: &[char], b: &[char]) -> Vec<Match> {
    let b2j = occurrences(b);
    let mut queue = vec![(0, a.len(), 0, b.len())];
    let mut blocks: Vec<Match> = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let found = longest_match(a, b, &b2j, alo, ahi, blo, bhi);
        if found.size == 0 {
            continue;
        }
        if alo < found.i && blo < found.j {
            queue.push((alo, found.i, blo, found.j));
        }
        if found.i + found.size < ahi && found.j + found.size < bhi {
            queue.push((found.i + found.size, ahi, found.j + found.size, bhi));
        }
        blocks.push(found);
    }
    blocks.sort_by_key(|block| (block.i, block.j, block.size));
    let mut merged: Vec<Match> = Vec::new();
    let mut run = Match {
        i: 0,
        j: 0,
        size: 0,
    };
    for block in blocks {
        if run.i + run.size == block.i && run.j + run.size == block.j {
            run.size += block.size;
        } else {
            if run.size > 0 {
                merged.push(run);
            }
            run = block;
        }
    }
    if run.size > 0 {
        merged.push(run);
    }
    merged.push(Match {
        i: a.len(),
        j: b.len(),
        size: 0,
    });
    merged
}

/// How to turn `a` into `b`, as `difflib.SequenceMatcher.get_opcodes` puts it.
#[must_use]
pub fn opcodes(a: &[char], b: &[char]) -> Vec<Opcode> {
    let (mut i, mut j) = (0, 0);
    let mut answer = Vec::new();
    for block in matching_blocks(a, b) {
        let tag = match (i < block.i, j < block.j) {
            (true, true) => Some(Tag::Replace),
            (true, false) => Some(Tag::Delete),
            (false, true) => Some(Tag::Insert),
            (false, false) => None,
        };
        if let Some(tag) = tag {
            answer.push(Opcode {
                tag,
                i1: i,
                i2: block.i,
                j1: j,
                j2: block.j,
            });
        }
        i = block.i + block.size;
        j = block.j + block.size;
        if block.size > 0 {
            answer.push(Opcode {
                tag: Tag::Equal,
                i1: block.i,
                i2: i,
                j1: block.j,
                j2: j,
            });
        }
    }
    answer
}

/// Render what the rewrite changed, one excerpt per change.
///
/// Not the whole rewrite: that is the message the reader just read, with a
/// small share of its characters different. Not whole lines either — a
/// paragraph is one line here, so a line-level pair prints two hundred
/// characters to show that one of them moved.
///
/// So the unit is the change itself, with a little of the sentence on either
/// side of it. Changes that survive only as whitespace or as the punctuation
/// width the deterministic rules own are dropped: that layer is already handled
/// (ADR-0005 section 四), and it is not what the reader is being asked to look
/// at. An empty string therefore means "nothing a reader would call a wording
/// change", which is not the same as "the two texts are equal" — the caller
/// tells those apart before asking, and says [`TYPOGRAPHY_ONLY`] rather than
/// [`UNCHANGED`] for this one.
#[must_use]
pub fn changes(before: &str, after: &str) -> String {
    let (a, b): (Vec<char>, Vec<char>) = (before.chars().collect(), after.chars().collect());
    let mut merged: Vec<Opcode> = Vec::new();
    for op in opcodes(&a, &b)
        .into_iter()
        .filter(|op| op.tag != Tag::Equal)
    {
        match merged.last_mut() {
            Some(last) if op.i1 - last.i2 <= NEAR => {
                last.tag = Tag::Replace;
                last.i2 = op.i2;
                last.j2 = op.j2;
            }
            _ => merged.push(op),
        }
    }
    let mut pairs: Vec<String> = Vec::new();
    for op in merged {
        let (old, new) = (excerpt(&a, op.i1, op.i2), excerpt(&b, op.j1, op.j2));
        if folded(&old) == folded(&new) {
            continue;
        }
        let lead = excerpt(&a, op.i1.saturating_sub(CONTEXT), op.i1);
        let tail = excerpt(&a, op.i2, (op.i2 + CONTEXT).min(a.len()));
        let head = if op.i1 > CONTEXT { "…" } else { "" };
        let end = if op.i2 + CONTEXT < a.len() { "…" } else { "" };
        pairs.push(format!(
            "原 {}",
            flat(&format!("{head}{lead}{old}{tail}{end}"))
        ));
        pairs.push(format!(
            "改 {}",
            flat(&format!("{head}{lead}{new}{tail}{end}"))
        ));
    }
    let rendered = pairs
        .chunks(2)
        .map(|pair| pair.join("\n"))
        .collect::<Vec<_>>()
        .join(PAIR_GAP);
    rendered.trim_matches(space).to_owned()
}

/// How many changes [`changes`] rendered.
///
/// The count is the part a reader can act on at a glance; the rewrite itself is
/// what they need to judge whether it reads better, and only they can judge
/// that. Zero for the empty rendering, which the caller answers in words
/// instead.
#[must_use]
pub fn count(changes: &str) -> usize {
    if changes.is_empty() {
        0
    } else {
        changes.split(PAIR_GAP).count()
    }
}

/// One span of a character sequence, as a string.
fn excerpt(text: &[char], from: usize, to: usize) -> String {
    text[from..to].iter().collect()
}

/// What became of the deterministic fixes over one turn's rewrites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fixes {
    /// They ran and had nothing to fix.
    Clean,
    /// They ran and changed at least one rewrite. Which models need the rules
    /// to clean up after them is a selection signal, not only a display fix.
    Repaired,
    /// They would not run, and the rewrites are passed through as they came: a
    /// rewrite with a typography slip in it still beats no rewrite at all. The
    /// string is the caller's own name for why, for its diagnostics line.
    Failed(String),
}

/// Put every rewrite of one turn through the deterministic fixes.
///
/// The fixer is the caller's, because what it needs — this repository's rules,
/// resolved from the directory the host was run in — is not this module's
/// business; what is this module's business is that a failed fix does not cost
/// the user their rewrite. `tidy` returns the fixed text, or the reason it
/// would not run, in which case the rewrite is shown as the model wrote it.
///
/// Returns what to display, in the same order as the rewrites came in, and what
/// the fixes did. Every rewrite is offered to the fixer even after one has
/// failed: the two columns of an A/B turn are shown side by side, and one of
/// them silently skipping the rules would make the comparison a comparison of
/// two different pipelines.
pub fn shown<F>(answers: &[String], mut tidy: F) -> (Vec<String>, Fixes)
where
    F: FnMut(&str) -> Result<String, String>,
{
    let mut display: Vec<String> = Vec::with_capacity(answers.len());
    let mut failed: Option<String> = None;
    for answer in answers {
        match tidy(answer) {
            Ok(fixed) => display.push(fixed),
            Err(why) => {
                display.push(answer.clone());
                failed.get_or_insert(why);
            }
        }
    }
    let fixes = match failed {
        Some(why) => Fixes::Failed(why),
        None if display == answers => Fixes::Clean,
        None => Fixes::Repaired,
    };
    (display, fixes)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
