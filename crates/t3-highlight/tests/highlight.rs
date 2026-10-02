//! Isolated tests for the highlighter. Ways it can fail, written before the code:
//!
//! 1. Language detection
//!    a. A fence info string with meta (`ts title="a.ts"`) picks the wrong word.
//!    b. Shiki aliases (`ts`, `py`, `sh`, `zsh`, `yml`, `rs`) or mixed case (`TypeScript`) miss.
//!    c. An unknown language panics or highlights as something random instead of plain text.
//!    d. The fork's `gitignore` -> `ini` substitution is lost.
//!    e. File paths: extension, compound extension (`x.d.ts` is still TypeScript), bare names
//!    (`Dockerfile`, `Makefile`), and case (`R`) resolve differently from @pierre/diffs.
//! 2. Tokens
//!    a. Ranges overlap, leave gaps, or split a multi-byte character.
//!    b. The last line without a trailing newline, CRLF endings, or empty input break lines.
//!    c. Plain text does not use the theme foreground.
//! 3. Theme conversion
//!    a. Scope rules lose their colors (keywords, strings, comments) or dark/light are swapped.
//!    b. Diff fences lose inserted/deleted colors.
//! 4. Caching and streaming
//!    a. The cache returns a result for different code, language or theme.
//!    b. Incremental highlighting of a growing block differs from highlighting it at once,
//!    including when the previous update ended mid-line or the text was rewritten.
//! 5. Pathological input
//!    a. A huge single line hangs the tokenizer.

use std::{ops::Range, sync::Arc};

use t3_highlight::{Highlighted, Language, StreamingHighlighter, Theme, highlight};

/// `keyword.control` (`return`, `if`). Note Shiki gives `const`/`let` the type color: Pierre
/// lists `storage.type` twice and VS Code lets the later rule win.
const DARK_KEYWORD: u32 = 0xff678dff;
const DARK_STORAGE_TYPE: u32 = 0xd568eaff;
const DARK_STRING: u32 = 0x5ecc71ff;
const DARK_COMMENT: u32 = 0x737373ff;
const DARK_FOREGROUND: u32 = 0xfafafaff;
const LIGHT_FOREGROUND: u32 = 0x0a0a0aff;

fn color_at(highlighted: &Highlighted, source: &str, needle: &str) -> u32 {
    let start = source.find(needle).expect("needle in source");
    highlighted
        .spans()
        .find(|(range, _)| range.contains(&start))
        .map(|(_, style)| style.color)
        .expect("a span covers every byte")
}

fn assert_contiguous(highlighted: &Highlighted, source: &str) {
    let mut expected_start = 0;
    for (range, _) in highlighted.spans() {
        assert_eq!(range.start, expected_start, "gap or overlap at {range:?}");
        assert!(range.start < range.end, "empty span {range:?}");
        assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        expected_start = range.end;
    }
    assert_eq!(expected_start, source.len(), "spans must cover the source");
    for line in highlighted.lines() {
        let range: Range<usize> = line.range();
        assert!(
            !source[range.clone()].contains('\n'),
            "line includes its terminator"
        );
        for token in line.tokens() {
            assert!(token.range.start >= range.start && token.range.end <= range.end);
        }
    }
}

#[test]
fn fence_info_uses_the_first_word_and_shiki_aliases() {
    let ts = Language::from_fence("ts");
    assert!(!ts.is_plain());
    assert_eq!(Language::from_fence("ts title=\"src/a.ts\""), ts);
    assert_eq!(Language::from_fence("typescript"), ts);
    assert_eq!(Language::from_fence("TypeScript"), ts);
    assert_eq!(Language::from_fence("  ts  "), ts);
    assert_eq!(Language::from_fence("py"), Language::from_fence("python"));
    assert_eq!(Language::from_fence("rs"), Language::from_fence("rust"));
    assert_eq!(Language::from_fence("yml"), Language::from_fence("yaml"));
    let bash = Language::from_fence("bash");
    for alias in ["sh", "shell", "zsh", "shellscript"] {
        assert_eq!(Language::from_fence(alias), bash, "{alias}");
    }
    assert_eq!(
        Language::from_fence("gitignore"),
        Language::from_fence("ini")
    );
    assert!(!Language::from_fence("diff").is_plain());
    assert!(!Language::from_fence("tsx").is_plain());
}

#[test]
fn unknown_or_plain_languages_fall_back_to_plain_text() {
    for info in [
        "",
        "text",
        "txt",
        "plaintext",
        "definitely-not-a-language",
        "{.rust}",
    ] {
        assert!(Language::from_fence(info).is_plain(), "{info:?}");
    }
    let source = "let x = 1;\n";
    let highlighted = highlight(source, Language::from_fence("nope"), Theme::Dark);
    assert_contiguous(&highlighted, source);
    assert!(
        highlighted
            .spans()
            .all(|(_, style)| style.color == DARK_FOREGROUND)
    );
    let light = highlight(source, Language::PLAIN, Theme::Light);
    assert!(
        light
            .spans()
            .all(|(_, style)| style.color == LIGHT_FOREGROUND)
    );
}

#[test]
fn paths_resolve_like_pierre_diffs() {
    assert_eq!(
        Language::from_path("src/main.rs"),
        Language::from_fence("rust")
    );
    assert_eq!(
        Language::from_path("/a/b/index.d.ts"),
        Language::from_fence("ts")
    );
    assert_eq!(Language::from_path("App.tsx"), Language::from_fence("tsx"));
    assert_eq!(
        Language::from_path("Dockerfile"),
        Language::from_fence("dockerfile")
    );
    assert_eq!(
        Language::from_path("build/Makefile"),
        Language::from_fence("make")
    );
    assert_eq!(Language::from_path("script.R"), Language::from_fence("r"));
    assert_eq!(
        Language::from_path("C:\\work\\x.py"),
        Language::from_fence("python")
    );
    assert!(Language::from_path("README").is_plain());
    assert!(Language::from_path("archive.unknownext").is_plain());
}

#[test]
fn tokens_cover_the_source_and_respect_char_boundaries() {
    let sources = [
        "const greeting = \"héllo 🌍\"; // ünïcode\nconst n = 1",
        "a\r\nb\r\n",
        "",
        "\n\n",
        "trailing newline\n",
    ];
    for source in sources {
        let highlighted = highlight(source, Language::from_fence("ts"), Theme::Dark);
        assert_contiguous(&highlighted, source);
    }
    let crlf = highlight("a\r\nb\r\n", Language::from_fence("ts"), Theme::Dark);
    let lines: Vec<_> = crlf.lines().iter().map(|line| line.range()).collect();
    assert_eq!(lines, vec![0..1, 3..4]);
    assert_eq!(highlight("", Language::PLAIN, Theme::Dark).lines().len(), 0);
    assert_eq!(
        highlight("x", Language::PLAIN, Theme::Dark).lines().len(),
        1
    );
}

#[test]
fn pierre_colors_apply_to_common_scopes() {
    let source = "// note\nconst value = \"text\";\nexport function run() { return 1; }\n";
    let dark = highlight(source, Language::from_fence("ts"), Theme::Dark);
    assert_eq!(color_at(&dark, source, "// note"), DARK_COMMENT);
    assert_eq!(color_at(&dark, source, "return"), DARK_KEYWORD);
    assert_eq!(color_at(&dark, source, "const"), DARK_STORAGE_TYPE);
    assert_eq!(color_at(&dark, source, "\"text\""), DARK_STRING);
    assert_eq!(color_at(&dark, source, "run"), 0x9d6afbff);

    let light = highlight(source, Language::from_fence("ts"), Theme::Light);
    assert_ne!(
        color_at(&light, source, "return"),
        DARK_KEYWORD,
        "themes swapped"
    );
    assert_eq!(color_at(&light, source, "\"text\""), 0x199f43ff);

    let rust = "fn main() { let s = \"x\"; return; }\n";
    let rust_dark = highlight(rust, Language::from_fence("rust"), Theme::Dark);
    // Shiki's Rust grammar scopes the quotes as plain punctuation; the contents are the string.
    assert_eq!(color_at(&rust_dark, rust, "x\""), DARK_STRING);
    assert_eq!(color_at(&rust_dark, rust, "return"), DARK_KEYWORD);
}

#[test]
fn diff_fences_color_inserted_and_deleted_lines() {
    let source = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
    let dark = highlight(source, Language::from_fence("diff"), Theme::Dark);
    // Shiki colors the +/- marker as punctuation and the rest of the line as the change.
    assert_eq!(color_at(&dark, source, "new"), 0x5ecc71ff);
    assert_eq!(color_at(&dark, source, "old"), 0xff855eff);
    assert_eq!(color_at(&dark, source, "+new"), 0x636363ff);
}

#[test]
fn cache_is_keyed_by_code_language_and_theme() {
    let ts = Language::from_fence("ts");
    let a = highlight("let a = 1;", ts, Theme::Dark);
    let again = highlight("let a = 1;", ts, Theme::Dark);
    assert!(
        Arc::ptr_eq(&a, &again),
        "identical input should hit the cache"
    );
    assert!(!Arc::ptr_eq(&a, &highlight("let a = 2;", ts, Theme::Dark)));
    assert!(!Arc::ptr_eq(&a, &highlight("let a = 1;", ts, Theme::Light)));
    assert!(!Arc::ptr_eq(
        &a,
        &highlight("let a = 1;", Language::PLAIN, Theme::Dark)
    ));
}

#[test]
fn streaming_matches_one_shot_highlighting() {
    let full = "/* block\ncomment */\nconst a = `tpl\n${1}`;\nfunction f() {\n  return \"s\";\n}\n";
    let language = Language::from_fence("ts");
    let expected = highlight(full, language, Theme::Dark);

    // Grow one byte at a time (ending mid-line and mid-token), checking every prefix.
    let mut streaming = StreamingHighlighter::new(language, Theme::Dark);
    for end in (0..=full.len()).filter(|end| full.is_char_boundary(*end)) {
        let prefix = &full[..end];
        let got = streaming.update(prefix);
        let one_shot = highlight(prefix, language, Theme::Dark);
        assert_eq!(*got, *one_shot, "prefix {end}");
    }
    assert_eq!(*streaming.update(full), *expected);

    // A rewrite that is not an extension starts over instead of reusing stale state.
    let rewritten = "const b = 2;\n";
    assert_eq!(
        *streaming.update(rewritten),
        *highlight(rewritten, language, Theme::Dark)
    );
}

#[test]
fn huge_lines_are_not_tokenized() {
    let line = format!("const x = \"{}\";\nreturn y;\n", "a".repeat(200_000));
    let started = std::time::Instant::now();
    let highlighted = highlight(&line, Language::from_fence("ts"), Theme::Dark);
    assert!(
        started.elapsed().as_secs() < 2,
        "took {:?}",
        started.elapsed()
    );
    assert_contiguous(&highlighted, &line);
    // The following line still highlights normally.
    let second = line.rfind("return").unwrap();
    let color = highlighted
        .spans()
        .find(|(range, _)| range.contains(&second))
        .unwrap()
        .1
        .color;
    assert_eq!(color, DARK_KEYWORD);
}
