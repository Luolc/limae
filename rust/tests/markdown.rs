use limae::markdown::{LineProtection, Markdown};

fn assert_spans(line: &str, protection: &LineProtection, code_text: &[&str], prose_text: &[&str]) {
    let LineProtection::Inline { code, prose } = protection else {
        panic!("expected an inline container for {line:?}");
    };
    assert_eq!(
        code.iter()
            .map(|span| &line[span.clone()])
            .collect::<Vec<_>>(),
        code_text,
        "code in {line:?}"
    );
    assert_eq!(
        prose
            .iter()
            .map(|span| &line[span.clone()])
            .collect::<Vec<_>>(),
        prose_text,
        "prose exemptions in {line:?}"
    );
}

#[test]
fn fences_toggle_on_either_prefix_with_python_whitespace() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (open, close) in [("```rust", "~~~ trailing"), ("~~~~", "````")] {
        let indented = format!("\u{001c}\u{001f}\u{3000}\t    {open}");
        let lines = ["正文A", &indented, "代码B「かな」", "", close, "正文C"];
        let protection = scanner.protect(&lines);
        assert_spans(lines[0], &protection[0], &[], &[]);
        assert!(
            protection[1..5]
                .iter()
                .all(|entry| *entry == LineProtection::Verbatim)
        );
        assert_spans(lines[5], &protection[5], &[], &[]);
        assert!(!protection[5].is_exempt(0..lines[5].len()));
        assert!(protection[2].is_exempt(0..0));
    }
    let lines = ["~~~", "直到末尾A"];
    assert!(
        scanner
            .protect(&lines)
            .iter()
            .all(|entry| *entry == LineProtection::Verbatim)
    );
    Ok(())
}

#[test]
fn equal_width_code_runs_keep_delimiters_and_unpaired_runs_as_prose() -> Result<(), regex::Error> {
    let lines = ["首🙂```字A``尾B```中C `e\u{301}（D）`末E ``未配F"];
    let protection = Markdown::new()?.protect(&lines);
    assert_spans(
        lines[0],
        &protection[0],
        &["字A``尾B", "e\u{301}（D）"],
        &[],
    );
    let LineProtection::Inline { code, .. } = &protection[0] else {
        unreachable!()
    };
    assert_eq!(&lines[0][code[0].start - 3..code[0].start], "```");
    assert_eq!(&lines[0][code[0].end..code[0].end + 3], "```");
    assert!(!protection[0].is_exempt(0.."首🙂".len()));
    assert!(!protection[0].is_exempt(code[0].start - 3..code[0].start));
    Ok(())
}

#[test]
fn utf8_multiline_interiors_retain_both_zero_length_edges() -> Result<(), regex::Error> {
    let lines = ["正文A🙂``", "中B e\u{301}", "``末C"];
    let protection = Markdown::new()?.protect(&lines);
    assert_spans(lines[0], &protection[0], &[""], &[]);
    assert_spans(lines[1], &protection[1], &[lines[1]], &[]);
    assert_spans(lines[2], &protection[2], &[""], &[]);
    for (entry, position) in [(&protection[0], lines[0].len()), (&protection[2], 0)] {
        let LineProtection::Inline { code, .. } = entry else {
            unreachable!()
        };
        assert_eq!(code[0], position..position);
    }
    assert!(protection[0].is_exempt(lines[0].len()..lines[0].len()));
    assert!(protection[2].is_exempt(0..0));
    assert!(!protection[0].is_exempt(0..lines[0].len()));
    assert!(!protection[2].is_exempt(0..lines[2].len()));
    Ok(())
}

#[test]
fn block_openers_and_blank_lines_end_code_containers() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for boundary in [
        "",
        "\u{001f}",
        "# 标题",
        "   ######",
        "#\u{001c}标题",
        "| 单元 |",
        "- * _",
        "---\u{001f}",
        "- 项",
        "* 项",
        "+\u{001f}项",
        "123456789) 项",
        "１. 项",
        "> 引用",
    ] {
        let lines = ["首`中A", boundary, "尾B`末C"];
        for (line, entry) in lines.iter().zip(scanner.protect(&lines)) {
            assert_spans(line, &entry, &[], &[]);
            assert!(!entry.is_exempt(0..line.len()));
        }
    }
    for continuation in [
        "普通续行",
        "    # 四空格",
        "\t- 制表符",
        "####### 七井号",
        "1234567890. 十位",
        "-- 两横线",
    ] {
        let lines = ["首`中A", continuation, "尾B`末C"];
        let protection = scanner.protect(&lines);
        assert_spans(lines[0], &protection[0], &["中A"], &[]);
        assert_spans(lines[1], &protection[1], &[continuation], &[]);
        assert_spans(lines[2], &protection[2], &["尾B"], &[]);
    }
    Ok(())
}

#[test]
fn list_items_and_consecutive_quotes_allow_continuations() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (lines, last_interior) in [
        (["- 首`中A", "  续B", "尾C`正文D"], "尾C"),
        (["> 首`中A", " > 续B", "> 尾C`正文D"], "> 尾C"),
    ] {
        let protection = scanner.protect(&lines);
        assert_spans(lines[0], &protection[0], &["中A"], &[]);
        assert_spans(lines[1], &protection[1], &[lines[1]], &[]);
        assert_spans(lines[2], &protection[2], &[last_interior], &[]);
        let end = lines[2].len();
        assert!(!protection[2].is_exempt(end - "正文D".len()..end));
    }
    for first in ["# 标题`中A", "| 表格`中A |", "- 项`中A"] {
        let next = if first.starts_with('-') {
            "- 项B`正文C"
        } else {
            "续B`正文C"
        };
        let lines = [first, next];
        for (line, entry) in lines.iter().zip(scanner.protect(&lines)) {
            assert_spans(line, &entry, &[], &[]);
        }
    }
    Ok(())
}

#[test]
fn caller_line_views_preserve_check_and_fix_boundaries() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    // The source has U+2028 followed by LF; Python splitlines drops the final
    // empty line, whereas split("\n") retains both U+2028 and the empty line.
    let source = "前🙂`\u{2028}`后A\n";
    let check_lines = ["前🙂`", "`后A"];
    let fix_lines: Vec<_> = source.split('\n').collect();
    let checked = scanner.protect(&check_lines);
    let fixed = scanner.protect(&fix_lines);
    assert_eq!(checked.len(), 2);
    assert_spans(check_lines[0], &checked[0], &[""], &[]);
    assert_spans(check_lines[1], &checked[1], &[""], &[]);
    assert_spans(fix_lines[0], &fixed[0], &["\u{2028}"], &[]);
    assert_spans(fix_lines[1], &fixed[1], &[], &[]);
    assert!(scanner.protect(&[]).is_empty());
    assert_spans("", &scanner.protect(&[""])[0], &[], &[]);
    Ok(())
}

#[test]
fn quotes_take_outer_pairs_and_resume_after_unclosed_openers() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (line, expected) in [
        (
            "正文A「外层「かな」尾部FOO」末B",
            vec!["外层「かな」尾部FOO"],
        ),
        ("正文A『かな《内层B》尾C』末D", vec!["かな《内层B》尾C"]),
        ("正文A「外层「かな」尾部FOO", vec!["かな"]),
        ("正文A《纯中文《内B》尾C》末D", vec![]),
        ("正文A「未闭合かなB", vec![]),
        ("正文A《かなB》与「カナC」末D", vec!["かなB", "カナC"]),
    ] {
        let protection = scanner.protect(&[line]);
        assert_spans(line, &protection[0], &[], &expected);
        assert!(!protection[0].is_exempt(0.."正文A".len()));
    }
    Ok(())
}

#[test]
fn code_interiors_take_priority_within_and_across_quotations() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (line, code, prose) in [
        (
            "正文A「かな`code`尾部FOO」",
            "code",
            vec!["かな`", "`尾部FOO"],
        ),
        ("正文A「`かな`尾部FOO」", "かな", vec!["`", "`尾部FOO"]),
        ("正文A`「かなB」`末C", "「かなB」", vec![]),
        ("正文A「かな`中」尾B`末C", "中」尾B", vec![]),
        ("正文A`中「かな`尾B」末C", "中「かな", vec![]),
    ] {
        let protection = scanner.protect(&[line]);
        assert_spans(line, &protection[0], &[code], &prose);
        assert!(!protection[0].is_exempt(0.."正文A".len()));
    }
    Ok(())
}

#[test]
fn destinations_urls_and_quotes_claim_in_priority_order() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (line, code, prose) in [
        ("正文A![图](中B(a)尾C)末D", vec![], vec!["中B(a"]),
        ("正文A[x]()末B", vec![], vec![]),
        (
            "正文A[x](http://example.com/中B)末C",
            vec![],
            vec!["http://example.com/中B"],
        ),
        (
            "正文A「かなhttp://example.com/x,尾B」",
            vec![],
            vec!["かなhttp://example.com/x,尾B"],
        ),
        ("正文A[x](中`code`尾B)末C", vec!["code"], vec![]),
        ("正文A[x](「かな」尾B)末C", vec![], vec!["かな"]),
        (
            "正文A`https://example.com/x`末B",
            vec!["https://example.com/x"],
            vec![],
        ),
        (
            "正文A[x](http://example.com/`code`)末B",
            vec!["code"],
            vec!["http://example.com/"],
        ),
    ] {
        let protection = scanner.protect(&[line]);
        assert_spans(line, &protection[0], &code, &prose);
        assert!(!protection[0].is_exempt(0.."正文A".len()));
    }
    Ok(())
}

#[test]
fn raw_urls_obey_ascii_adjacency_and_leave_trailing_punctuation() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (line, prose) in [
        (
            "中https://example.com/x)]}>,.;:!?尾A",
            vec!["https://example.com/x"],
        ),
        ("Ahttps://example.com/x", vec![]),
        ("7http://example.com/x", vec![]),
        ("_http://example.com/x", vec!["http://example.com/x"]),
        ("éhttp://example.com/x", vec!["http://example.com/x"]),
        ("中HTTP://example.com/x", vec![]),
        ("Ahttp://http://example.com/x", vec!["http://example.com/x"]),
        ("中https://尾A", vec!["https://"]),
        (
            "中http://example.com/a(b)?q=x&k=1#v,尾A",
            vec!["http://example.com/a(b)?q=x&k=1#v"],
        ),
    ] {
        let protection = scanner.protect(&[line]);
        assert_spans(line, &protection[0], &[], &prose);
        assert!(!protection[0].is_exempt(0..line.chars().next().map_or(0, char::len_utf8)));
    }
    Ok(())
}

#[test]
fn match_exemptions_distinguish_contact_from_overlap() -> Result<(), regex::Error> {
    let line = "前🙂`中A`末B";
    let protection = Markdown::new()?.protect(&[line]);
    let start = "前🙂`".len();
    let end = start + "中A".len();
    for position in [start, end] {
        assert!(protection[0].is_exempt(position..position));
    }
    for matched in [start..end, start - 1..end, start..end + 1, 0..line.len()] {
        assert!(protection[0].is_exempt(matched));
    }
    for matched in [
        0..start,
        end..line.len(),
        start - 1..start - 1,
        end + 1..end + 1,
    ] {
        assert!(!protection[0].is_exempt(matched));
    }
    Ok(())
}

#[test]
fn shared_fixture_code_and_prose_slices_match_source() -> Result<(), regex::Error> {
    let scanner = Markdown::new()?;
    for (source, expected_code, expected_prose) in [
        (
            include_str!("../../spec/fixtures/span-opens-at-line-end.in"),
            vec!["", "code"],
            vec![],
        ),
        (
            include_str!("../../spec/fixtures/inline-code-spans.in"),
            vec![
                "ln(K/F)",
                "文件:行号",
                "你好,世界",
                "含 ` 的 code(x)",
                "a(1)",
                "b:中",
                "code",
                "y",
            ],
            vec![],
        ),
        (
            include_str!("../../spec/fixtures/url-protection.in"),
            vec![],
            vec![
                "https://example.com/16GB",
                "https://example.com/Foo中文",
                "https://example.com/16GB",
                "https://example.com/2011",
                "https://example.com/foo",
                "https://example.com/x",
                "https://example.com/a——b",
                "https://example.com/x",
            ],
        ),
    ] {
        let lines: Vec<_> = source.split('\n').collect();
        let protection = scanner.protect(&lines);
        let mut code_text = Vec::new();
        let mut prose_text = Vec::new();
        for (line, entry) in lines.iter().zip(&protection) {
            let LineProtection::Inline { code, prose } = entry else {
                unreachable!()
            };
            code_text.extend(code.iter().map(|span| &line[span.clone()]));
            prose_text.extend(prose.iter().map(|span| &line[span.clone()]));
        }
        assert_eq!(code_text, expected_code);
        assert_eq!(prose_text, expected_prose);
    }
    Ok(())
}

#[test]
fn shared_kana_code_fixture_ignores_brackets_inside_code() -> Result<(), regex::Error> {
    let lines: Vec<_> = include_str!("../../spec/fixtures/kana-code-spans.in")
        .split('\n')
        .collect();
    let protection = Markdown::new()?.protect(&lines);
    let expected: [(&str, &[&str]); 12] = [
        ("「", &[]),
        ("「", &["かな`", "`中A"]),
        ("」", &["かな`", "`中A"]),
        ("code", &["かな`", "`中A"]),
        ("」尾", &[]),
        ("中「", &[]),
        ("中」尾", &[]),
        ("首「かな", &[]),
        ("「」", &["かな`", "`中A"]),
        ("「", &["かな`", "`中A"]),
        ("』", &["かな`", "`中A"]),
        ("《", &["かな`", "`中A"]),
    ];
    for (i, (code, prose)) in expected.iter().enumerate() {
        assert_spans(lines[2 * i], &protection[2 * i], &[*code], prose);
    }
    // The first line's kana is outside any valid quotation; its CJK/ASCII
    // boundary stays editable despite the opening bracket in code.
    let boundary = "`「`かな中".len();
    assert!(!protection[0].is_exempt(boundary..boundary));
    Ok(())
}

#[test]
fn inert_code_opener_can_shorten_a_quotation() -> Result<(), regex::Error> {
    let line = "「かな`「`」中A」后";
    let protection = Markdown::new()?.protect(&[line]);
    assert_spans(line, &protection[0], &["「"], &["かな`", "`"]);
    let tail = line.len() - "中A」后".len();
    assert!(!protection[0].is_exempt(tail..line.len()));
    assert!(!protection[0].is_exempt(tail + '中'.len_utf8()..tail + '中'.len_utf8()));
    Ok(())
}
