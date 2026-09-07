use super::{
    BLOCK_GAP, Fixes, MIN_CHARS, Opcode, TYPOGRAPHY_ONLY, Tag, UNCHANGED, changes, count, flat,
    folded, opcodes, prose_length, shown, trailing_newlines,
};

// The reference implementation's answers over a fixed synthetic corpus, as
// `Case` values in `CASES`. Included rather than declared as a module because
// it is generated: `tools/render_diff_cases.py` writes it and `--check` says
// whether it is current.
include!("render_cases.rs");

fn characters(text: &str) -> Vec<char> {
    text.chars().collect()
}

/// One opcode as the corpus spells it: `tag:i1:i2:j1:j2`.
fn spelled(op: &Opcode) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        op.tag.as_str(),
        op.i1,
        op.i2,
        op.j1,
        op.j2
    )
}

/// The whole point of the port: `difflib.SequenceMatcher`'s own division of a
/// rewrite, not a reasonable diff of it. A different-but-sensible alignment
/// would give the reader a different number of changes for the same rewrite, so
/// the reference implementation's answers are replayed here character for
/// character. `--count` on the generator raises the corpus for a wider sweep.
#[test]
fn the_alignment_and_the_pairs_match_the_reference_implementation() {
    assert!(
        CASES.len() >= 90,
        "the corpus shrank to {} cases",
        CASES.len()
    );
    let mut pruned = 0;
    for case in CASES {
        let (before, after) = (case.before, case.after);
        let found: Vec<String> = opcodes(&characters(before), &characters(after))
            .iter()
            .map(spelled)
            .collect();
        assert_eq!(
            found.join(" "),
            case.opcodes,
            "alignment differs for {before:?} -> {after:?}"
        );
        assert_eq!(
            changes(before, after),
            case.changes,
            "pairs differ for {before:?} -> {after:?}"
        );
        assert_eq!(prose_length(before), case.prose_length, "for {before:?}");
        assert_eq!(trailing_newlines(before), case.trailing, "for {before:?}");
        assert_eq!(flat(before), case.flat, "for {before:?}");
        assert_eq!(folded(before), case.folded, "for {before:?}");
        pruned += usize::from(after.chars().count() >= 200);
    }
    // Above 200 elements `difflib` stops seeding matches on the characters that
    // are everywhere, and that pruning changes which blocks it finds. A corpus
    // that never crossed the threshold would leave the port's copy of the rule
    // untested while looking like a parity check.
    //
    // The length that decides it is `after`'s: the popularity table is built
    // over the second sequence, so a corpus of long inputs rewritten into short
    // outputs never prunes anything. Counting `before` here would be a guard
    // that says the same thing whether or not the branch it names was reached.
    assert!(
        pruned >= 10,
        "only {pruned} cases have an `after` long enough for difflib to prune"
    );
}

#[test]
fn a_message_is_measured_by_its_prose_and_not_by_its_code() {
    let prose = "正文一句话。";
    let fenced = "```sh\necho 这一行不算数，它在围栏里，长得很\n```\n";
    assert_eq!(prose_length(prose), 6);
    // The fence markers are not counted either, so a message that is nothing
    // but a code block counts zero rather than counting its markers.
    assert_eq!(prose_length(&format!("{fenced}{prose}")), 6);
    assert_eq!(prose_length(fenced), 0);
    assert_eq!(prose_length("~~~\nx\n~~~"), 0);
}

#[test]
fn whitespace_is_not_prose_however_it_is_spelled() {
    // Python counts the file/group/record/unit separators as whitespace and
    // `char::is_whitespace` does not. A message padded with them would be
    // polished on one side of the port and skipped on the other.
    assert_eq!(prose_length("a b\tc\u{a0}d\u{1f}e\u{3000}f"), 6);
    assert_eq!(prose_length("   \t\u{1c}\u{2028}\u{85} "), 0);
}

#[test]
fn the_threshold_is_a_threshold_and_not_a_direction() {
    // Both sides of it, because a comparison with the wrong strictness — or
    // one that counts the fenced characters — is wrong on exactly one of these.
    let short = "字".repeat(MIN_CHARS - 1);
    let long = "字".repeat(MIN_CHARS);
    assert!(prose_length(&short) < MIN_CHARS);
    assert!(prose_length(&long) >= MIN_CHARS);
    let padded = format!("```\n{}\n```\n{long}", "x".repeat(500));
    assert_eq!(prose_length(&padded), MIN_CHARS);
}

#[test]
fn the_newlines_a_message_ends_on_are_counted_and_nothing_else_is() {
    // The caller subtracts what the host has already painted from the gap it
    // wants, so a trailing space is not a line that reached the screen.
    assert_eq!(trailing_newlines("话\n\n"), 2);
    assert_eq!(trailing_newlines("话"), 0);
    assert_eq!(trailing_newlines("话\n \n"), 1);
    assert_eq!(trailing_newlines("话\n  "), 0);
    assert_eq!(trailing_newlines("\n\n\n"), 3);
    assert_eq!(BLOCK_GAP, 2);
}

#[test]
fn a_rewrite_that_moved_only_a_punctuation_width_is_told_from_one_that_moved_a_word() {
    // The two answers the block gives when it prints no pairs are different
    // statements, and an implementation that cannot tell them apart passes
    // every test that only asks "was there a change".
    let body = "这里有（注）一处，正文继续写下去，后面还有别的话。";
    let widened = body.replace('（', "(").replace('）', ")");
    assert_ne!(widened, body);
    // Nothing to show: the deterministic rules own the width.
    assert!(changes(body, &widened).is_empty());
    // ... and that is not the same as the two texts being equal, which is what
    // the caller checks first to say `UNCHANGED` instead of `TYPOGRAPHY_ONLY`.
    assert!(changes(body, body).is_empty());
    assert_ne!(UNCHANGED, TYPOGRAPHY_ONLY);
}

#[test]
fn a_bracket_the_model_replaced_or_dropped_is_not_called_typography() {
    // The rules pick a width. They do not delete a bracket and do not turn one
    // kind into another, so neither of these may be folded away.
    let body = "这里有 (注) 一处，正文继续写下去，后面还有别的话。";
    assert!(!changes(body, &body.replace("(注)", "「注」")).is_empty());
    assert!(!changes(body, &body.replace("(注)", "注")).is_empty());
    // Whitespace alone is the rules' too.
    assert!(changes(body, &body.replace(" (注) ", "(注)")).is_empty());
}

#[test]
fn folding_settles_the_comparison_and_never_reaches_the_screen() {
    // The normal form exists to decide whether a change is worth showing. If it
    // reached the pairs, the reader would be shown a sentence nobody wrote:
    // half-width punctuation where the message has full-width, and no spaces.
    let before = "他说（这一段）要改一个词，后面继续写下去，还有更多的话。";
    let after = before.replace("要改一个词", "要换一个词");
    let rendered = changes(before, &after);
    assert!(
        rendered.contains('（') && rendered.contains('）'),
        "{rendered}"
    );
    assert!(rendered.contains('，'), "{rendered}");
    assert!(!rendered.contains('('), "{rendered}");
    // The excerpts themselves, not their normal forms.
    assert!(
        rendered.contains("要改一个词") && rendered.contains("要换一个词"),
        "{rendered}"
    );
}

#[test]
fn an_excerpt_that_straddles_a_line_break_is_shown_on_one_line() {
    let before = "前面这一句写完了，\n然后另起一行接着写下去。";
    let after = before.replace("接着", "继续");
    let rendered = changes(before, &after);
    assert_eq!(rendered.lines().count(), 2, "{rendered}");
    assert!(
        rendered.contains("写完了， 然后另起") || rendered.contains("， 然后"),
        "{rendered}"
    );
}

#[test]
fn the_count_is_how_many_changes_there_were_and_not_whether_there_were_any() {
    // An implementation that reported one change for any non-empty set passes
    // every single-change test there is.
    let before = "第一处要改的词在这里，中间隔着足够长的一段话，第二处要改的词在那里。";
    let one = before.replace("第一处要改", "第一处会改");
    let two = one.replace("第二处要改", "第二处会改");
    assert_eq!(count(&changes(before, &one)), 1);
    assert_eq!(count(&changes(before, &two)), 2);
    assert_eq!(count(&changes(before, before)), 0);
}

#[test]
fn two_edits_within_a_few_characters_read_as_one() {
    // Two changes closer than `NEAR` are one edit to a reader, and printing
    // them as two would inflate the count on the screen. Both sides of the
    // distance, so that a merge rule of any other width is wrong on one of
    // them: six characters apart is one change, seven is two.
    let pair = |gap: &str| {
        let before = format!("开头这一段话之后，甲{gap}乙，后面还有别的话。");
        let after = before.replace('甲', "丙").replace('乙', "丁");
        count(&changes(&before, &after))
    };
    assert_eq!(pair("六个字的间隔"), 1);
    assert_eq!(pair("七个字的间隔啊"), 2);
}

#[test]
fn a_change_is_shown_with_its_context_and_the_ellipsis_says_there_is_more() {
    let before = "这是很长的一段开头文字，中间这个词要改掉，后面还有很长的一段文字收尾。";
    let after = before.replace("要改掉", "要换掉");
    let rendered = changes(before, &after);
    assert!(rendered.starts_with("原 …"), "{rendered}");
    assert!(rendered.contains('…'), "{rendered}");
    // How much context, exactly: ten characters of the sentence are kept on
    // the near side, and the ellipsis appears only once something was elided.
    let ten = "零一二三四五六七八九";
    let before = format!("{ten}改这个词，后面还有很长的一段文字收尾在这里。");
    let rendered = changes(&before, &before.replace("改这个词", "换这个词"));
    assert!(
        rendered.starts_with(&format!("原 {ten}改这个词")),
        "{rendered}"
    );
    let before = format!("补{before}");
    let rendered = changes(&before, &before.replace("改这个词", "换这个词"));
    assert!(
        rendered.starts_with(&format!("原 …{ten}改这个词")),
        "{rendered}"
    );
}

#[test]
fn every_rewrite_is_offered_to_the_fixer_and_the_failure_is_named() {
    let answers = vec!["甲".to_owned(), "乙".to_owned()];
    let mut seen: Vec<String> = Vec::new();
    let (display, fixes) = shown(&answers, |answer| {
        seen.push(answer.to_owned());
        if answer == "甲" {
            Err("crashed".to_owned())
        } else {
            Ok(format!("{answer}!"))
        }
    });
    // The second column is still fixed: two rewrites shown side by side, one of
    // them quietly skipping the rules, is a comparison of two pipelines.
    assert_eq!(seen, answers);
    // The one whose fix would not run is passed through as the model wrote it.
    assert_eq!(display, vec!["甲".to_owned(), "乙!".to_owned()]);
    assert_eq!(fixes, Fixes::Failed("crashed".to_owned()));
}

#[test]
fn a_fixer_that_changed_something_is_told_from_one_that_had_nothing_to_do() {
    // Which models need the rules to clean up after them is a selection signal,
    // so "ran and fixed" and "ran and found nothing" are different answers.
    let answers = vec!["原样".to_owned()];
    let (display, fixes) = shown(&answers, |answer| Ok(answer.to_owned()));
    assert_eq!(display, answers);
    assert_eq!(fixes, Fixes::Clean);
    let (display, fixes) = shown(&answers, |answer| Ok(format!("{answer}。")));
    assert_eq!(display, vec!["原样。".to_owned()]);
    assert_eq!(fixes, Fixes::Repaired);
}

#[test]
fn the_first_reason_is_the_one_reported_and_the_order_is_kept() {
    let answers = vec!["一".to_owned(), "二".to_owned(), "三".to_owned()];
    let (display, fixes) = shown(&answers, |answer| match answer {
        "二" => Err("first".to_owned()),
        "三" => Err("second".to_owned()),
        _ => Ok(answer.to_owned()),
    });
    assert_eq!(display, answers);
    assert_eq!(fixes, Fixes::Failed("first".to_owned()));
}

#[test]
fn an_empty_turn_has_nothing_to_fix() {
    let (display, fixes) = shown(&[], |answer| Ok(answer.to_owned()));
    assert!(display.is_empty());
    assert_eq!(fixes, Fixes::Clean);
}

#[test]
fn the_opcodes_cover_both_sides_end_to_end() {
    // A hole in the coverage would drop a change without anything saying so.
    let (before, after) = ("甲乙丙丁戊", "甲庚丙丁己戊");
    let (a, b) = (characters(before), characters(after));
    let ops = opcodes(&a, &b);
    assert!(!ops.is_empty());
    let (mut i, mut j) = (0, 0);
    for op in &ops {
        assert_eq!((op.i1, op.j1), (i, j), "{ops:?}");
        assert!(op.i2 >= op.i1 && op.j2 >= op.j1);
        if op.tag == Tag::Equal {
            assert_eq!(a[op.i1..op.i2], b[op.j1..op.j2]);
        }
        i = op.i2;
        j = op.j2;
    }
    assert_eq!((i, j), (a.len(), b.len()));
}
