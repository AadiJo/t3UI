//! Chunker and link-resolution tests, ported from the fork's `streamingMarkdown.test.ts` and
//! `markdown-links.test.ts`. Extra failure modes for the port: chunks not covering the text,
//! boundaries landing mid-UTF-8 or mid-block, and parent suffixes for colliding chip names.

use t3_markdown::{
    links::{
        format_workspace_relative_path, parent_suffixes, resolve_file_link,
        resolve_file_link_target, rewrite_file_uri,
    },
    streaming::{Chunk, MIN_FROZEN_CHARS, chunks},
};

fn long_paragraph(label: &str) -> String {
    format!(
        "{label} {}",
        "content ".repeat(MIN_FROZEN_CHARS.div_ceil(8))
    )
}

fn texts<'a>(text: &'a str, chunks: &[Chunk]) -> Vec<&'a str> {
    chunks
        .iter()
        .map(|chunk| &text[chunk.range.clone()])
        .collect()
}

#[test]
fn completed_and_short_documents_are_one_chunk() {
    assert_eq!(
        chunks("done", false),
        vec![Chunk {
            range: 0..4,
            is_streaming: false
        }]
    );
    assert_eq!(
        chunks("still streaming", true),
        vec![Chunk {
            range: 0..15,
            is_streaming: true
        }]
    );
}

#[test]
fn freezes_a_large_prefix_once_the_next_block_starts() {
    let first = long_paragraph("first");
    let text = format!("{first}\n\nsecond block is live");
    let result = chunks(&text, true);
    assert_eq!(
        texts(&text, &result),
        vec![format!("{first}\n\n").as_str(), "second block is live"]
    );
    assert!(!result[0].is_streaming && result[1].is_streaming);
}

#[test]
fn never_splits_inside_a_fence() {
    let text = format!(
        "```ts\n{}\n\nconst next = 2;\n```\n\nafter",
        long_paragraph("const value =")
    );
    let result = chunks(&text, true);
    let parts = texts(&text, &result);
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("const next = 2;") && parts[0].contains("```\n\n"));
    assert_eq!(parts[1], "after");
}

#[test]
fn keeps_loose_list_items_together() {
    let text = format!("- {}\n\n- two\n\nafter", long_paragraph("one"));
    let parts = texts(&text, &chunks(&text, true))
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].contains("- two\n\n"));
    assert_eq!(parts[1], "after");
}

#[test]
fn reference_links_and_raw_html_stay_live() {
    let safe = format!("{}\n\n", long_paragraph("safe"));
    let reference = format!("{safe}[documentation][docs]\n\nnext");
    let html = format!("{safe}<details>\n{}\n\nmore", long_paragraph("inside"));
    let reference_chunks = chunks(&reference, true);
    let html_chunks = chunks(&html, true);
    assert_eq!(&reference[reference_chunks[0].range.clone()], safe);
    assert!(
        reference[reference_chunks.last().unwrap().range.clone()].contains("[documentation][docs]")
    );
    assert_eq!(&html[html_chunks[0].range.clone()], safe);
    assert!(html[html_chunks.last().unwrap().range.clone()].contains("<details>"));
}

#[test]
fn chunks_cover_the_text_on_char_boundaries() {
    let text = format!(
        "{}\n\n{}\n\n- é\n- ü\n\n{}\n\ntail ✓",
        long_paragraph("ünïcödé"),
        long_paragraph("second"),
        long_paragraph("third")
    );
    let result = chunks(&text, true);
    assert!(result.len() > 2);
    let mut cursor = 0;
    for chunk in &result {
        assert_eq!(chunk.range.start, cursor);
        assert!(text.is_char_boundary(chunk.range.end));
        cursor = chunk.range.end;
    }
    assert_eq!(cursor, text.len());
    assert!(result.iter().rev().skip(1).all(|chunk| !chunk.is_streaming));
}

#[test]
fn file_uri_rewrites() {
    assert_eq!(
        rewrite_file_uri("file:///Users/julius/project/src/main.ts#L42").as_deref(),
        Some("/Users/julius/project/src/main.ts#L42")
    );
    assert_eq!(
        rewrite_file_uri("file:///Users/julius/project/file%2520name.md").as_deref(),
        Some("/Users/julius/project/file%2520name.md")
    );
    assert_eq!(
        rewrite_file_uri(
            "file:///D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69"
        )
        .as_deref(),
        Some("D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69")
    );
    assert_eq!(
        rewrite_file_uri(" <file:///D:/Programme/t3code/apps/web/src/markdown-links.ts> ")
            .as_deref(),
        Some("D:/Programme/t3code/apps/web/src/markdown-links.ts")
    );
}

#[test]
fn file_link_targets() {
    let project = Some("/Users/julius/project");
    let cases: &[(&str, Option<&str>, Option<&str>)] = &[
        (
            "/Users/julius/project/AGENTS.md",
            None,
            Some("/Users/julius/project/AGENTS.md"),
        ),
        (
            "src/processRunner.ts:71",
            project,
            Some("/Users/julius/project/src/processRunner.ts:71"),
        ),
        (
            "script.ts:10",
            project,
            Some("/Users/julius/project/script.ts:10"),
        ),
        (
            "AGENTS.md",
            project,
            Some("/Users/julius/project/AGENTS.md"),
        ),
        (
            "/Users/julius/project/src/main.ts#L42C7",
            None,
            Some("/Users/julius/project/src/main.ts:42:7"),
        ),
        ("https://example.com/docs", None, None),
        (
            "file:///Users/julius/project/file%2520name.md",
            None,
            Some("/Users/julius/project/file%20name.md"),
        ),
        (
            "/D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx#L69",
            None,
            Some("D:/Programme/t3code/apps/web/src/components/chat/OpenInPicker.tsx:69"),
        ),
        (
            "</D:/Programme/t3code/apps/web/src/components/ChatMarkdown.tsx:1>",
            None,
            Some("D:/Programme/t3code/apps/web/src/components/ChatMarkdown.tsx:1"),
        ),
        ("/chat/settings", None, None),
        ("#heading", project, None),
        (
            "~/notes/todo.md",
            project,
            Some("/Users/julius/notes/todo.md"),
        ),
    ];
    for (href, cwd, expected) in cases {
        assert_eq!(
            resolve_file_link_target(href, *cwd).as_deref(),
            *expected,
            "{href}"
        );
    }
}

#[test]
fn file_link_display_paths() {
    let meta = resolve_file_link(
        "file:///C:/Users/mike/dev-stuff/t3code/apps/web/src/session-logic.ts#L501",
        Some("C:/Users/mike/dev-stuff/t3code"),
    )
    .unwrap();
    assert_eq!(
        meta.display_path,
        "t3code/apps/web/src/session-logic.ts:501"
    );
    assert_eq!(
        meta.workspace_relative_path.as_deref(),
        Some("apps/web/src/session-logic.ts")
    );
    assert_eq!(meta.line, Some(501));

    let outside = resolve_file_link("/tmp/report.ts", Some("/repo/project")).unwrap();
    assert_eq!(outside.workspace_relative_path, None);
    assert_eq!(
        format_workspace_relative_path("/repo/project/a/b.rs:3:4", Some("/repo/project/")),
        "project/a/b.rs:3:4"
    );
}

#[test]
fn colliding_basenames_get_parent_suffixes() {
    let suffixes = parent_suffixes([
        "/repo/crates/app/src/lib.rs",
        "/repo/crates/ui/src/lib.rs",
        "/repo/crates/ui/src/main.rs",
    ]);
    assert_eq!(suffixes["/repo/crates/app/src/lib.rs"], "app/src");
    assert_eq!(suffixes["/repo/crates/ui/src/lib.rs"], "ui/src");
    assert!(!suffixes.contains_key("/repo/crates/ui/src/main.rs"));
}
