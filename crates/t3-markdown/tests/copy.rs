//! Copy-as-markdown tests (the fork's `markdown-clipboard.ts`). Ways it can fail, written first:
//!
//! 1. Markers wrap surrounding whitespace (`** bold**`) instead of hoisting it out.
//! 2. A selection inside styled text loses the style, or cuts a marker in half.
//! 3. Nested styles (bold italic, code in a link) come out crossed or unbalanced.
//! 4. Inline code containing backticks gets a fence that closes early.
//! 5. Non-http links keep markdown syntax (the fork copies their label), and a link whose label
//!    is its URL is not copied bare.
//! 6. Chips copy their label instead of their markdown.
//! 7. Block syntax: heading `#`s, list markers only on the first line, quote `> ` on every line,
//!    code fences with the language (and a longer fence when the code holds one).

use t3_markdown::copy::{CopyFormat, Markup, serialize};

fn plain() -> CopyFormat {
    CopyFormat::default()
}

#[test]
fn hoists_whitespace_out_of_markers() {
    let text = "a  bold  b";
    let markup = [(1..8, Markup::Bold)];
    assert_eq!(
        serialize(text, 0..text.len(), &markup, &[], &plain()),
        "a  **bold**  b"
    );
}

#[test]
fn partial_selection_keeps_style() {
    let text = "say hello world";
    let markup = [(4..15, Markup::Italic)];
    assert_eq!(serialize(text, 6..11, &markup, &[], &plain()), "*llo w*");
    assert_eq!(serialize(text, 0..3, &markup, &[], &plain()), "say");
}

#[test]
fn nested_styles_balance() {
    let text = "both and link code";
    let markup = [
        (0..4, Markup::Bold),
        (0..4, Markup::Italic),
        (9..18, Markup::Link("https://a.b".into())),
        (14..18, Markup::Code),
    ];
    assert_eq!(
        serialize(text, 0..text.len(), &markup, &[], &plain()),
        "***both*** and [link `code`](https://a.b)"
    );
}

#[test]
fn inline_code_fences_around_backticks() {
    let text = "a `b` c";
    let markup = [(0..text.len(), Markup::Code)];
    assert_eq!(
        serialize(text, 0..text.len(), &markup, &[], &plain()),
        "``a `b` c``"
    );
    let edge = "`x";
    assert_eq!(
        serialize(edge, 0..edge.len(), &[(0..2, Markup::Code)], &[], &plain()),
        "`` `x ``"
    );
}

#[test]
fn links_follow_the_fork() {
    let text = "docs https://x.y top";
    let markup = [
        (0..4, Markup::Link("https://example.com".into())),
        (5..16, Markup::Link("https://x.y".into())),
        (17..20, Markup::Link("#top".into())),
    ];
    assert_eq!(
        serialize(text, 0..text.len(), &markup, &[], &plain()),
        "[docs](https://example.com) https://x.y top"
    );
}

#[test]
fn atoms_copy_their_markdown() {
    let text = "see \u{FFFC} now";
    let atoms = [(4, "[main.rs](src/main.rs)")];
    assert_eq!(
        serialize(text, 0..text.len(), &[], &atoms, &plain()),
        "see [main.rs](src/main.rs) now"
    );
}

#[test]
fn block_syntax() {
    let heading = CopyFormat {
        first_prefix: "## ".into(),
        ..CopyFormat::default()
    };
    assert_eq!(serialize("Title", 0..5, &[], &[], &heading), "## Title");

    let quote = CopyFormat {
        first_prefix: "> ".into(),
        line_prefix: "> ".into(),
        ..CopyFormat::default()
    };
    assert_eq!(serialize("a\nb", 0..3, &[], &[], &quote), "> a\n> b");

    let item = CopyFormat {
        first_prefix: "- ".into(),
        line_prefix: "  ".into(),
        ..CopyFormat::default()
    };
    assert_eq!(serialize("a\nb", 0..3, &[], &[], &item), "- a\n  b");

    let code = CopyFormat {
        fence: Some("ts".into()),
        ..CopyFormat::default()
    };
    assert_eq!(
        serialize("let a;", 0..6, &[], &[], &code),
        "```ts\nlet a;\n```"
    );
    let nested = "```\nx\n```";
    assert_eq!(
        serialize(
            nested,
            0..nested.len(),
            &[],
            &[],
            &CopyFormat {
                fence: Some(String::new()),
                ..CopyFormat::default()
            }
        ),
        "````\n```\nx\n```\n````"
    );

    let gap = CopyFormat {
        block_gap: true,
        ..CopyFormat::default()
    };
    assert_eq!(serialize("p", 0..1, &[], &[], &gap), "p\n");
}
