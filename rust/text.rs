//! UTF-8-safe text primitives shared by limae's rules.
//!
//! Byte offsets remain the internal coordinate system because Rust regular
//! expressions return them. Character windows count Unicode scalar values.
//!
//! ```
//! use limae::text::{halfwidth_digit, snippet};
//!
//! let text = "前文🙂e\u{301}２０１１后文";
//! let start = text.find('２').ok_or("missing digit")?;
//! let end = start + '２'.len_utf8();
//!
//! assert_eq!(snippet(text, start..end), Some(text));
//! assert_eq!(halfwidth_digit('２'), Some('2'));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::ops::Range;

const SNIPPET_RADIUS: usize = 12;

/// Return the character beginning at a UTF-8 byte boundary.
#[must_use]
pub fn char_at(text: &str, byte_index: usize) -> Option<char> {
    text.get(byte_index..)?.chars().next()
}

/// Return the character ending at a UTF-8 byte boundary.
#[must_use]
pub fn char_before(text: &str, byte_index: usize) -> Option<char> {
    text.get(..byte_index)?.chars().next_back()
}

/// Return whether a character is in the CJK range defined by the rule spec.
#[must_use]
pub const fn is_cjk(character: char) -> bool {
    matches!(character, '\u{4e00}'..='\u{9fff}')
}

/// Return whether a character is an ASCII letter.
#[must_use]
pub const fn is_ascii_letter(character: char) -> bool {
    character.is_ascii_alphabetic()
}

/// Return whether a character is an ASCII digit.
#[must_use]
pub const fn is_ascii_digit(character: char) -> bool {
    character.is_ascii_digit()
}

/// Return whether a character is a limae word character.
#[must_use]
pub const fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || is_cjk(character)
}

/// Match Python's `str.isspace()` character set used by the reference code.
///
/// Python adds U+001C–U+001F to Unicode's `White_Space` property.
#[must_use]
pub const fn is_python_whitespace(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\u{001c}'..='\u{001f}')
}

/// Return the source around a byte range, with up to 12 scalars on each side.
///
/// Invalid, reversed, or non-character-boundary ranges return `None`.
#[must_use]
pub fn snippet(text: &str, matched: Range<usize>) -> Option<&str> {
    let _ = text.get(matched.clone())?;
    let before = text.get(..matched.start)?;
    let after = text.get(matched.end..)?;

    let start = before
        .char_indices()
        .rev()
        .nth(SNIPPET_RADIUS - 1)
        .map_or(0, |(index, _)| index);
    let end = after
        .char_indices()
        .nth(SNIPPET_RADIUS)
        .map_or(text.len(), |(index, _)| matched.end + index);

    text.get(start..end)
}

/// Convert one fullwidth decimal digit to its ASCII form.
#[must_use]
pub fn halfwidth_digit(character: char) -> Option<char> {
    if !matches!(character, '０'..='９') {
        return None;
    }

    char::from_u32(u32::from(character) - u32::from('０') + u32::from('0'))
}
