//! Markdown exemptions from `spec/rules.md`, with line-local UTF-8 byte ranges.
//!
//! Callers supply their line view: checking uses Python `splitlines()` semantics,
//! while fixing splits only at LF and retains the final empty line. This module
//! joins lines with LF only while pairing code delimiters within a block.
//!
//! ```
//! use limae::markdown::{LineProtection, Markdown};
//!
//! let lines = ["前🙂`", "`后文A"];
//! let protection = Markdown::new()?.protect(&lines);
//! let LineProtection::Inline { code, prose } = &protection[0] else {
//!     return Err("expected an inline container".into());
//! };
//! assert_eq!(code, &[lines[0].len()..lines[0].len()]);
//! assert!(prose.is_empty());
//! assert!(!protection[1].is_exempt(1..lines[1].len()));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::ops::Range;

use regex::Regex;

use crate::text::{char_before, is_python_whitespace};

/// Exemptions for one input line, keeping code interiors separate for fixes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineProtection {
    /// A fence marker or fenced content; preserve the whole line.
    Verbatim,
    /// Sorted, disjoint byte ranges in each collection, excluding delimiters.
    Inline {
        /// Inline code interiors, including zero-length ranges at line edges.
        code: Vec<Range<usize>>,
        /// Non-code exempt interiors: kana quotations, destinations and URLs.
        /// Quote fragments can include the delimiters of code nested in them.
        prose: Vec<Range<usize>>,
    },
}

impl LineProtection {
    /// Test a line-local match: zero-width positions include both endpoints,
    /// while consuming matches are exempt on any overlap.
    #[must_use]
    pub fn is_exempt(&self, matched: Range<usize>) -> bool {
        match self {
            Self::Verbatim => true,
            Self::Inline { code, prose } => code.iter().chain(prose).any(|span| {
                if matched.start == matched.end {
                    span.start <= matched.start && matched.start <= span.end
                } else {
                    matched.start < span.end && matched.end > span.start
                }
            }),
        }
    }
}

/// Reusable scanner for limae's simplified Markdown protection contract.
pub struct Markdown {
    single_line_block: Regex,
    block_start: Regex,
    quote_line: Regex,
    backticks: Regex,
    destination: Regex,
    raw_url: Regex,
}

impl Markdown {
    /// Compile the scanner's patterns once for reuse across documents.
    ///
    /// # Errors
    /// Returns the regex compilation error if a built-in pattern is invalid.
    pub fn new() -> Result<Self, regex::Error> {
        let single = r"#{1,6}(?:[\s\x1c-\x1f]|$)|\||(?:[-*_][\s\x1c-\x1f]*){3,}$";
        Ok(Self {
            single_line_block: Regex::new(&format!(r"^ {{0,3}}(?:{single})"))?,
            block_start: Regex::new(&format!(
                r"^ {{0,3}}(?:{single}|[-*+][\s\x1c-\x1f]|\d{{1,9}}[.)][\s\x1c-\x1f]|>)"
            ))?,
            quote_line: Regex::new(r"^ {0,3}>")?,
            backticks: Regex::new("`+")?,
            destination: Regex::new(r"\]\([^)]*\)")?,
            raw_url: Regex::new(r"https?://[A-Za-z0-9\-._~:/?#\[\]@!$&'()*+,;=%]*")?,
        })
    }

    /// Return one protection entry per supplied line, with byte offsets relative
    /// to that exact line. No newline normalization or splitting is performed.
    #[must_use]
    pub fn protect(&self, lines: &[&str]) -> Vec<LineProtection> {
        let mut result = vec![LineProtection::Verbatim; lines.len()];
        let tail = self.paragraphs(lines, |segment| match segment {
            Segment::Blank(i) => {
                result[i] = LineProtection::Inline {
                    code: Vec::new(),
                    prose: Vec::new(),
                };
            }
            Segment::Paragraph(range) => self.flush(lines, &mut result, range),
        });
        self.flush(lines, &mut result, tail);
        result
    }

    /// Return whether the text's last paragraph may still be inside an inline
    /// code span that a line arriving later could close.
    ///
    /// The last paragraph is the one only the end of the input ended: nothing
    /// in the text — no blank line, no block start, no fence — closed it, so a
    /// later line would continue it, and a span can cross line breaks within a
    /// paragraph (`spec/rules.md`「全局豁免」第 2 条). The answer is whether a
    /// backtick run in that paragraph is still without a partner. Pairing is
    /// greedy from the left, so a later partner for any unpaired run — not only
    /// the last one — can re-pair every run after it, which is why one unpaired
    /// run anywhere in the paragraph is enough.
    ///
    /// `false` inside a fenced code block, and after anything that closed the
    /// paragraph. A trailing empty line counts as a blank line here, as it does
    /// in [`Self::protect`]; a caller that has not seen the next line yet should
    /// not pass one.
    #[must_use]
    pub fn unclosed_span(&self, lines: &[&str]) -> bool {
        let tail = self.paragraphs(lines, |_| {});
        if tail.is_empty() {
            return false;
        }
        let joined = lines[tail].join("\n");
        let runs: Vec<_> = self.backticks.find_iter(&joined).collect();
        pair(&runs).1
    }

    /// Walk the lines the way `spec/rules.md` segments them, handing each
    /// blank line and each closed paragraph to `segment` as it is met.
    ///
    /// Returns the paragraph the end of the input left open — empty when the
    /// last line closed its own paragraph, or the text ends inside a fence.
    fn paragraphs(&self, lines: &[&str], mut segment: impl FnMut(Segment)) -> Range<usize> {
        let mut paragraph = 0..0;
        let mut in_fence = false;
        let close = |paragraph: &mut Range<usize>, segment: &mut dyn FnMut(Segment)| {
            if paragraph.start < paragraph.end {
                segment(Segment::Paragraph(paragraph.clone()));
            }
            paragraph.start = paragraph.end;
        };
        for (i, line) in lines.iter().enumerate() {
            let stripped = line.trim_start_matches(is_python_whitespace);
            if stripped.starts_with("```") || stripped.starts_with("~~~") {
                close(&mut paragraph, &mut segment);
                in_fence = !in_fence;
            } else if in_fence {
                continue;
            } else if line.chars().all(is_python_whitespace) {
                close(&mut paragraph, &mut segment);
                segment(Segment::Blank(i));
            } else {
                let continues_quote = !paragraph.is_empty()
                    && self.quote_line.is_match(line)
                    && self.quote_line.is_match(lines[paragraph.end - 1]);
                if self.block_start.is_match(line) && !continues_quote {
                    close(&mut paragraph, &mut segment);
                }
                if paragraph.is_empty() {
                    paragraph.start = i;
                }
                paragraph.end = i + 1;
                if self.single_line_block.is_match(line) {
                    close(&mut paragraph, &mut segment);
                }
            }
        }
        paragraph
    }

    fn flush(&self, lines: &[&str], result: &mut [LineProtection], paragraph: Range<usize>) {
        let joined = lines[paragraph.clone()].join("\n");
        let runs: Vec<_> = self.backticks.find_iter(&joined).collect();
        let (spans, _) = pair(&runs);
        let mut offset = 0;
        for i in paragraph {
            let end = offset + lines[i].len();
            let code: Vec<_> = spans
                .iter()
                .filter(|span| span.start <= end && span.end >= offset)
                .map(|span| span.start.max(offset) - offset..span.end.min(end) - offset)
                .collect();
            let prose = self.prose_spans(lines[i], &code);
            result[i] = LineProtection::Inline { code, prose };
            offset = end + 1;
        }
    }

    fn prose_spans(&self, line: &str, code: &[Range<usize>]) -> Vec<Range<usize>> {
        let mut prose = Vec::new();
        for quote in quote_spans(line, code) {
            let mut start = quote.start;
            for span in code {
                if start <= span.start && span.end <= quote.end {
                    claim(start..span.start, code, &mut prose);
                    start = span.end;
                }
            }
            claim(start..quote.end, code, &mut prose);
        }
        for destination in self.destination.find_iter(line) {
            claim(
                destination.start() + 2..destination.end() - 1,
                code,
                &mut prose,
            );
        }
        let mut offset = 0;
        while let Some(url) = self.raw_url.find_at(line, offset) {
            if char_before(line, url.start()).is_some_and(|ch| ch.is_ascii_alphanumeric()) {
                // A rejected start must not consume a later URL within this run.
                offset = url.start() + 1;
                continue;
            }
            let trimmed = url
                .as_str()
                .trim_end_matches([')', ']', '}', '>', ',', '.', ';', ':', '!', '?']);
            claim(url.start()..url.start() + trimmed.len(), code, &mut prose);
            offset = url.end();
        }
        prose.sort_by_key(|span| (span.start, span.end));
        prose
    }
}

/// One thing [`Markdown::paragraphs`] meets on its way through the lines.
enum Segment {
    /// A line of nothing but whitespace, outside any fence.
    Blank(usize),
    /// A run of lines the rules read as one paragraph, now closed.
    Paragraph(Range<usize>),
}

/// Pair backtick runs into code spans: each run closes with the next run of
/// the same length, and a run with no such partner is ordinary text.
///
/// Returns the interiors between paired runs, and whether any run was left
/// without a partner.
fn pair(runs: &[regex::Match<'_>]) -> (Vec<Range<usize>>, bool) {
    let mut spans = Vec::new();
    let mut unpaired = false;
    let mut i = 0;
    while i < runs.len() {
        if let Some(j) = (i + 1..runs.len()).find(|&j| runs[j].len() == runs[i].len()) {
            spans.push(runs[i].end()..runs[j].start());
            i = j + 1;
        } else {
            unpaired = true;
            i += 1;
        }
    }
    (spans, unpaired)
}

fn claim(candidate: Range<usize>, code: &[Range<usize>], prose: &mut Vec<Range<usize>>) {
    if !candidate.is_empty()
        && !code
            .iter()
            .chain(prose.iter())
            .any(|span| candidate.start < span.end && candidate.end > span.start)
    {
        prose.push(candidate);
    }
}

fn quote_spans(line: &str, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut chars = line
        .char_indices()
        .filter(|(index, _)| !code.iter().any(|span| span.contains(index)));
    while let Some((start, opener)) = chars.next() {
        let closer = match opener {
            '「' => '」',
            '『' => '』',
            '《' => '》',
            _ => continue,
        };
        let mut depth = 1;
        let mut tail = chars.clone();
        for (end, ch) in tail.by_ref() {
            if ch == opener {
                depth += 1;
            } else if ch == closer {
                depth -= 1;
                if depth == 0 {
                    let interior = start + opener.len_utf8()..end;
                    if line[interior.clone()]
                        .chars()
                        .any(|c| matches!(c, '\u{3040}'..='\u{30ff}'))
                    {
                        spans.push(interior);
                    }
                    chars = tail;
                    break;
                }
            }
        }
    }
    spans
}
