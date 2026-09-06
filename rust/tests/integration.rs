mod markdown;

use limae::text::{
    char_at, char_before, halfwidth_digit, is_cjk, is_python_whitespace, is_word, snippet,
};

#[test]
fn byte_positions_follow_utf8_boundaries() -> Result<(), &'static str> {
    let text = "中🙂e\u{301}文";
    let emoji = text.find('🙂').ok_or("missing emoji")?;
    let combining = text.find('\u{301}').ok_or("missing combining mark")?;

    assert_eq!(char_at(text, emoji), Some('🙂'));
    assert_eq!(char_before(text, emoji), Some('中'));
    assert_eq!(char_at(text, combining), Some('\u{301}'));
    assert_eq!(char_before(text, combining), Some('e'));
    assert_eq!(char_at(text, emoji + 1), None);
    assert_eq!(char_before(text, emoji + 1), None);
    assert_eq!(char_at(text, text.len()), None);
    Ok(())
}

#[test]
fn snippet_counts_scalars_around_a_byte_range() -> Result<(), &'static str> {
    let left = "甲".repeat(11) + "🙂";
    let right = "e\u{301}".repeat(6);
    let text = format!("前{left}Ａ{right}后");
    let start = text.find('Ａ').ok_or("missing fullwidth letter")?;
    let end = start + 'Ａ'.len_utf8();
    let expected = format!("{left}Ａ{right}");
    let zero_text = format!("前{left}{right}后");
    let boundary = "前".len() + left.len();
    let zero_expected = format!("{left}{right}");

    assert_eq!(snippet(&text, start..end), Some(expected.as_str()));
    assert_eq!(
        snippet(&zero_text, boundary..boundary),
        Some(zero_expected.as_str())
    );
    assert_eq!(snippet(&text, start + 1..end), None);
    assert_eq!(snippet(&text, end..start), None);
    Ok(())
}

#[test]
fn character_classes_match_the_rule_and_python_boundaries() -> Result<(), &'static str> {
    const PYTHON_WHITESPACE: &str = "\u{0009}\u{000a}\u{000b}\u{000c}\u{000d}\
        \u{001c}\u{001d}\u{001e}\u{001f}\u{0020}\u{0085}\u{00a0}\u{1680}\
        \u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\
        \u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";

    assert!(is_cjk('一'));
    assert!(is_cjk('鿿'));
    assert!(!is_cjk('㐀'));
    assert!(!is_cjk('\u{a000}'));
    assert!(is_word('文'));
    assert!(is_word('A'));
    assert!(is_word('7'));
    assert!(!is_word('é'));
    assert!(!is_word('７'));
    assert!(!is_word('_'));
    assert!(PYTHON_WHITESPACE.chars().all(is_python_whitespace));
    assert!(!is_python_whitespace('\u{001b}'));
    assert!(!is_python_whitespace('!'));
    assert!(!is_python_whitespace('\u{200b}'));
    Ok(())
}

#[test]
fn fullwidth_digit_conversion_is_narrow() -> Result<(), &'static str> {
    assert_eq!(halfwidth_digit('０'), Some('0'));
    assert_eq!(halfwidth_digit('９'), Some('9'));
    assert_eq!(halfwidth_digit('0'), None);
    assert_eq!(halfwidth_digit('Ａ'), None);
    Ok(())
}
