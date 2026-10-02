//! Parser tests. Ways it can fail, written before the code:
//!
//! 1. Blocks: heading levels, nested lists (ordered start numbers), tight vs loose items, task
//!    markers in tight and loose items (and not leaking into nested items), quotes, rules,
//!    tables (alignment, header vs body), code fences (language, title meta, final newline,
//!    `gitignore`), indented code.
//! 2. Inline: style spans not covering the text, or wrong ranges for nested emphasis; whitespace
//!    not collapsed like HTML; soft breaks not becoming spaces (or line breaks with
//!    `line_breaks`); hard breaks lost; trailing spaces kept.
//! 3. Links: unsafe `javascript:` kept; file paths not becoming chips (relative needs a cwd);
//!    external links missing the favicon atom or the no-wrap lead; `file://` not rewritten.
//! 4. Literal autolinks: missed bare URLs, trailing punctuation or unbalanced `)` included,
//!    autolinking inside code or existing links, `www.` and emails.
//! 5. Raw HTML: script content shown, comments shown, `<br>` lost, `<details>` not collapsing
//!    the blocks between its tags, unknown tags shown as text.
//! 6. Footnotes numbered by first reference and collected at the end.
//! 7. Never panicking on hostile input.

use t3_markdown::{
    ParseOptions,
    document::{ATOM_CHAR, Align, AtomKind, Block, Document, Inline, InlineStyle},
    parse,
};

fn doc(source: &str) -> Document {
    parse(source, &ParseOptions::default())
}

fn doc_in(source: &str, cwd: &str) -> Document {
    parse(
        source,
        &ParseOptions {
            line_breaks: false,
            cwd: Some(cwd.to_string()),
        },
    )
}

fn paragraph(block: &Block) -> &Inline {
    match block {
        Block::Paragraph { content, .. } => content,
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

fn only_paragraph(source: &str) -> Inline {
    let document = doc(source);
    assert_eq!(document.blocks.len(), 1, "{document:?}");
    paragraph(&document.blocks[0]).clone()
}

fn styled(inline: &Inline, needle: &str) -> InlineStyle {
    let start = inline.text.find(needle).expect("needle");
    inline
        .spans
        .iter()
        .find(|span| span.range.contains(&start))
        .expect("spans cover the text")
        .style
}

fn assert_spans_cover(inline: &Inline) {
    let mut cursor = 0;
    for span in &inline.spans {
        assert_eq!(span.range.start, cursor, "{inline:?}");
        cursor = span.range.end;
    }
    assert_eq!(cursor, inline.text.len(), "{inline:?}");
}

#[test]
fn headings_paragraphs_rules_and_quotes() {
    let document = doc("# One\n\n###### Six\n\ntext\n\n---\n\n> quoted\n> > nested");
    let levels: Vec<u8> = document
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Heading { level, .. } => Some(*level),
            _ => None,
        })
        .collect();
    assert_eq!(levels, vec![1, 6]);
    assert!(matches!(document.blocks[3], Block::Rule));
    let Block::Quote(children) = &document.blocks[4] else {
        panic!("{:?}", document.blocks[4]);
    };
    assert_eq!(paragraph(&children[0]).text, "quoted");
    assert!(matches!(children[1], Block::Quote(_)));
}

#[test]
fn lists_tight_loose_ordered_and_tasks() {
    let document = doc("3. three\n4. four\n   - nested\n\n- [x] done\n- [ ] todo\n  - [x] inner");
    let Block::List(ordered) = &document.blocks[0] else {
        panic!()
    };
    assert_eq!(ordered.start, Some(3));
    assert!(matches!(
        ordered.items[0].blocks[0],
        Block::Paragraph { tight: true, .. }
    ));
    assert!(matches!(ordered.items[1].blocks[1], Block::List(_)));

    let Block::List(tasks) = &document.blocks[1] else {
        panic!()
    };
    assert_eq!(tasks.items[0].task, Some(true));
    assert_eq!(tasks.items[1].task, Some(false), "nested marker leaked out");
    let Block::List(inner) = &tasks.items[1].blocks[1] else {
        panic!()
    };
    assert_eq!(inner.items[0].task, Some(true));
    assert_eq!(paragraph(&tasks.items[0].blocks[0]).text, "done");

    let loose = doc("- [x] a\n\n- [ ] b");
    let Block::List(loose) = &loose.blocks[0] else {
        panic!()
    };
    assert_eq!(loose.items[0].task, Some(true));
    assert_eq!(loose.items[1].task, Some(false));
    assert!(matches!(
        loose.items[0].blocks[0],
        Block::Paragraph { tight: false, .. }
    ));
}

#[test]
fn tables_keep_alignment_header_and_rows() {
    let document = doc("| a | b | c |\n| :-- | :-: | --: |\n| 1 | `2` | 3 |\n| 4 | 5 | 6 |");
    let Block::Table(table) = &document.blocks[0] else {
        panic!()
    };
    assert_eq!(
        table.alignments,
        vec![Align::Left, Align::Center, Align::Right]
    );
    assert_eq!(table.header.len(), 3);
    assert_eq!(table.rows.len(), 2);
    assert!(styled(&table.rows[0][1], "2").code);
}

#[test]
fn code_fences_language_title_and_newline() {
    let document = doc(
        "```ts title=\"src/a.ts\"\nlet a = 1;\n```\n\n```rust src/main.rs\nfn main() {}\n```\n\n```\nplain\n```\n\n```gitignore\ntarget\n```\n\n    indented\n",
    );
    let codes: Vec<_> = document
        .blocks
        .iter()
        .map(|block| match block {
            Block::Code(code) => code.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(codes[0].language, "ts");
    assert_eq!(codes[0].title.as_deref(), Some("src/a.ts"));
    assert_eq!(codes[0].code, "let a = 1;");
    assert_eq!(codes[1].title.as_deref(), Some("src/main.rs"));
    assert_eq!(codes[2].language, "text");
    assert_eq!(codes[3].language, "ini");
    assert_eq!(codes[4].language, "text");
    assert_eq!(codes[4].code, "indented");
    assert!(codes[0].id < codes[1].id, "ids are source offsets");
}

#[test]
fn inline_styles_and_whitespace() {
    let inline = only_paragraph("a **bold *both*** ~~gone~~   `x   y` end  ");
    assert_spans_cover(&inline);
    assert_eq!(inline.text, "a bold both gone x y end");
    assert!(styled(&inline, "bold").bold);
    let both = styled(&inline, "both");
    assert!(both.bold && both.italic);
    assert!(styled(&inline, "gone").strike);
    assert!(styled(&inline, "x y").code);
    assert_eq!(styled(&inline, "end"), InlineStyle::default());
}

#[test]
fn soft_and_hard_breaks() {
    assert_eq!(only_paragraph("one\ntwo").text, "one two");
    assert_eq!(only_paragraph("one  \ntwo").text, "one\ntwo");
    assert_eq!(only_paragraph("one\\\ntwo").text, "one\ntwo");
    let user = parse(
        "one\ntwo",
        &ParseOptions {
            line_breaks: true,
            cwd: None,
        },
    );
    assert_eq!(paragraph(&user.blocks[0]).text, "one\ntwo");
}

#[test]
fn links_files_and_favicons() {
    let document = doc_in(
        "[docs](https://example.com/a) [bad](javascript:alert(1)) [chip](src/main.rs:42) [abs](/Users/me/x.ts) [f](file:///Users/me/y.rs#L3) [rel](notes) [frag](#top)",
        "/Users/me/project",
    );
    let inline = paragraph(&document.blocks[0]);
    let hrefs: Vec<&str> = inline.links.iter().map(|link| link.href.as_str()).collect();
    assert_eq!(hrefs, vec!["https://example.com/a", "", "notes", "#top"]);
    let chips: Vec<_> = inline
        .atoms
        .iter()
        .filter_map(|atom| match &atom.kind {
            AtomKind::FileChip { link, .. } => Some(link.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(chips.len(), 3);
    assert_eq!(chips[0].target_path, "/Users/me/project/src/main.rs:42");
    assert_eq!(chips[0].line, Some(42));
    assert_eq!(chips[1].basename, "x.ts");
    assert_eq!(chips[2].target_path, "/Users/me/y.rs:3");
    // The chip replaces the link text.
    assert!(!inline.text.contains("chip"));

    // External links start with a favicon atom and keep the lead from wrapping.
    let favicon = &inline.atoms[0];
    assert!(matches!(&favicon.kind, AtomKind::Favicon { host } if host == "example.com"));
    let docs = &inline.links[0];
    assert_eq!(docs.range.start, favicon.offset);
    assert_eq!(&inline.text[docs.range.clone()], format!("{ATOM_CHAR}docs"));
    assert_eq!(
        docs.nowrap_until,
        Some(favicon.offset + ATOM_CHAR.len_utf8() + 1)
    );

    // Relative paths need a cwd to become chips.
    assert!(
        doc("[chip](src/main.rs)")
            .blocks
            .iter()
            .all(|block| paragraph(block).atoms.is_empty())
    );
}

#[test]
fn literal_autolinks() {
    let inline = only_paragraph(
        "see https://example.com/path_(x)). and www.rust-lang.org, mail me@example.com! `https://code.example` [x](https://a.b)",
    );
    let hrefs: Vec<&str> = inline.links.iter().map(|link| link.href.as_str()).collect();
    assert_eq!(
        hrefs,
        vec![
            "https://example.com/path_(x)",
            "http://www.rust-lang.org",
            "mailto:me@example.com",
            "https://a.b",
        ]
    );
    let url = &inline.links[0];
    assert!(inline.text[url.range.clone()].ends_with("path_(x)"));
    // `https://` stays on the favicon's line; the rest may break anywhere.
    let lead = url.nowrap_until.unwrap();
    assert!(inline.text[url.range.start..lead].ends_with("https://"));
}

#[test]
fn raw_html_is_sanitized() {
    let document = doc(
        "<script>alert(1)</script>\n\nbefore<br>after <!-- hidden --> <b>bold</b> <span>kept</span>\n\n<div align=\"center\">centered &amp; safe</div>\n",
    );
    let texts: Vec<String> = document
        .blocks
        .iter()
        .map(|block| paragraph(block).text.clone())
        .collect();
    assert!(
        texts.iter().all(|text| !text.contains("alert")),
        "{texts:?}"
    );
    let inline = paragraph(&document.blocks[0]);
    assert!(
        inline.text.starts_with("before\nafter"),
        "{:?}",
        inline.text
    );
    assert!(!inline.text.contains("hidden"));
    assert!(styled(inline, "bold").bold);
    assert!(inline.text.contains("kept") && !inline.text.contains("span"));
    assert_eq!(texts.last().unwrap(), "centered & safe");
}

#[test]
fn details_collect_their_blocks() {
    let document = doc(
        "<details>\n<summary>More</summary>\n\nhidden **text**\n\n- item\n\n</details>\n\nafter\n",
    );
    let Block::Details(details) = &document.blocks[0] else {
        panic!("{document:?}")
    };
    assert_eq!(details.summary.as_ref().unwrap().text, "More");
    assert!(!details.open);
    assert_eq!(details.blocks.len(), 2);
    assert_eq!(paragraph(&document.blocks[1]).text, "after");

    let open = doc("<details open>\n\nbody\n\n</details>");
    let Block::Details(open) = &open.blocks[0] else {
        panic!()
    };
    assert!(open.open && open.summary.is_none());
}

#[test]
fn footnotes_numbered_by_reference_order() {
    let document = doc("b[^b] a[^a] b again[^b]\n\n[^a]: Alpha.\n[^b]: Beta.\n");
    let inline = paragraph(&document.blocks[0]);
    let numbers: Vec<usize> = inline
        .atoms
        .iter()
        .filter_map(|atom| match atom.kind {
            AtomKind::FootnoteRef { number } => Some(number),
            _ => None,
        })
        .collect();
    assert_eq!(numbers, vec![1, 2, 1]);
    let Some(Block::Footnotes(footnotes)) = document.blocks.last() else {
        panic!()
    };
    assert_eq!(footnotes[0].number, 1);
    assert_eq!(paragraph(&footnotes[0].blocks[0]).text, "Beta.");
}

#[test]
fn hostile_input_never_panics() {
    let inputs = [
        "",
        "[",
        "<",
        "<a href=",
        "<details><details><summary>",
        "</details></details>",
        "```",
        "> > > > > > > > > > > > > > > > > > > > > > > > x",
        "- - - - - - - - - - - - - - - - - - - - - - - - x",
        "|a|\n|-|\n|b|c|d|",
        "[^x]\n\n[^x]: [^x]",
        "https://",
        "www.",
        "@@@@@",
        "a@b",
        "\u{FFFC}\u{FFFC}",
        "**\u{301}*_`~",
        "<sub><sup><b><i></b></i>",
        "&#xZZ; &#99999999; &amp",
    ];
    for input in inputs {
        let document = doc(input);
        fn walk(blocks: &[Block]) {
            for block in blocks {
                match block {
                    Block::Paragraph { content, .. } | Block::Heading { content, .. } => {
                        assert_spans_cover(content);
                        for atom in &content.atoms {
                            assert!(content.text[atom.offset..].starts_with(ATOM_CHAR));
                        }
                        for link in &content.links {
                            assert!(content.text.get(link.range.clone()).is_some());
                        }
                    }
                    Block::Quote(children) => walk(children),
                    Block::List(list) => list.items.iter().for_each(|item| walk(&item.blocks)),
                    Block::Details(details) => walk(&details.blocks),
                    Block::Footnotes(notes) => notes.iter().for_each(|note| walk(&note.blocks)),
                    _ => {}
                }
            }
        }
        walk(&document.blocks);
    }
}
