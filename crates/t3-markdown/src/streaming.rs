//! Splits a growing markdown document into frozen chunks and one live tail, ported from the
//! fork's `chat/streamingMarkdown.ts`.
//!
//! Frozen chunks never change once emitted, so their parse (and highlighting) is cached and only
//! the tail is re-parsed per update. Boundaries fall only after the next top-level block has
//! started, never inside fences, lists, quotes, indented code or raw HTML, and chunks are at least
//! [`MIN_FROZEN_CHARS`] long. Splitting stops at the first chunk that contains raw HTML or a
//! possible reference-style link, because later text could change how it parses.

use std::ops::Range;

/// `STREAMING_MARKDOWN_MIN_FROZEN_CHARS`. The fork counts UTF-16 code units; this counts bytes,
/// which only moves boundaries in non-ASCII text and never inside a block.
pub const MIN_FROZEN_CHARS: usize = 512;

/// One chunk of the document: a byte range of the full text and whether it may still change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub range: Range<usize>,
    pub is_streaming: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BlockMode {
    Simple,
    List,
    Quote,
    Indented,
    Html,
    Fence,
}

#[derive(Clone, Copy)]
struct Fence {
    marker: u8,
    len: usize,
}

struct Line<'a> {
    start: usize,
    content: &'a str,
}

fn split_lines(text: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let end = text[start..]
            .find('\n')
            .map_or(text.len(), |offset| start + offset + 1);
        let content_end = if text.as_bytes().get(end - 1) == Some(&b'\n') {
            end - 1
        } else {
            end
        };
        let content_end = if content_end > start && text.as_bytes()[content_end - 1] == b'\r' {
            content_end - 1
        } else {
            content_end
        };
        lines.push(Line {
            start,
            content: &text[start..content_end],
        });
        start = end;
    }
    lines
}

/// Up to three leading spaces, as CommonMark allows before a block marker.
fn strip_indent(line: &str) -> &str {
    let spaces = line
        .bytes()
        .take(3)
        .take_while(|byte| *byte == b' ')
        .count();
    &line[spaces..]
}

/// `FENCE_LINE`: ` {0,3}(`{3,}|~{3,})`.
fn fence_from_line(line: &str) -> Option<Fence> {
    let rest = strip_indent(line);
    let marker = *rest.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let len = rest.bytes().take_while(|byte| *byte == marker).count();
    (len >= 3).then_some(Fence { marker, len })
}

fn closes_fence(line: &str, fence: Fence) -> bool {
    let trimmed = line
        .strip_prefix("   ")
        .unwrap_or_else(|| line.trim_start());
    let markers = trimmed
        .bytes()
        .take_while(|byte| *byte == fence.marker)
        .count();
    markers >= fence.len && trimmed[markers..].trim().is_empty()
}

/// `LIST_LINE`: ` {0,3}(?:[-+*]|\d+[.)])\s+`.
fn is_list_line(line: &str) -> bool {
    let rest = strip_indent(line);
    let after_marker = match rest.as_bytes().first() {
        Some(b'-' | b'+' | b'*') => &rest[1..],
        Some(byte) if byte.is_ascii_digit() => {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            match rest.as_bytes().get(digits) {
                Some(b'.' | b')') => &rest[digits + 1..],
                _ => return false,
            }
        }
        _ => return false,
    };
    after_marker.starts_with(char::is_whitespace)
}

/// `QUOTE_LINE`: ` {0,3}>`.
fn is_quote_line(line: &str) -> bool {
    strip_indent(line).starts_with('>')
}

/// `INDENTED_LINE`: four spaces or a tab.
fn is_indented_line(line: &str) -> bool {
    line.starts_with("    ") || line.starts_with('\t')
}

/// `HTML_LINE` for one line: ` {0,3}<(?:!--|\/?[A-Za-z][A-Za-z0-9-]*(?:\s|>|\/))`.
/// `newline_follows` is whether the regex could see the line's own `\n` (it can match `\s`).
fn is_html_line(line: &str, newline_follows: bool) -> bool {
    let Some(rest) = strip_indent(line).strip_prefix('<') else {
        return false;
    };
    if rest.starts_with("!--") {
        return true;
    }
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    if !rest.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
        return false;
    }
    let name_len = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        .count();
    match rest[name_len..].chars().next() {
        None => newline_follows,
        Some(ch) => ch.is_whitespace() || ch == '>' || ch == '/',
    }
}

/// `POTENTIAL_REFERENCE_LINK`: `(^|[^!])\[[^\]\n]+\](?!\s*\()`, multiline.
fn has_potential_reference_link(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(offset) = text[i..].find('[') {
        let open = i + offset;
        i = open + 1;
        if open > 0 && bytes[open - 1] == b'!' {
            continue;
        }
        let Some(close_offset) = text[open + 1..].find([']', '\n']) else {
            continue;
        };
        let close = open + 1 + close_offset;
        if bytes[close] != b']' || close == open + 1 {
            continue;
        }
        let after = text[close + 1..].trim_start();
        if !after.starts_with('(') {
            return true;
        }
    }
    false
}

fn block_mode(line: &str) -> BlockMode {
    if fence_from_line(line).is_some() {
        BlockMode::Fence
    } else if is_list_line(line) {
        BlockMode::List
    } else if is_quote_line(line) {
        BlockMode::Quote
    } else if is_indented_line(line) {
        BlockMode::Indented
    } else if is_html_line(line, false) {
        BlockMode::Html
    } else {
        BlockMode::Simple
    }
}

fn continues_block(mode: BlockMode, line: &str) -> bool {
    match mode {
        BlockMode::List => is_list_line(line) || is_indented_line(line),
        BlockMode::Quote => is_quote_line(line) || is_indented_line(line),
        BlockMode::Indented => is_indented_line(line),
        BlockMode::Html => true,
        BlockMode::Fence | BlockMode::Simple => false,
    }
}

/// `canFreezeChunk`. The fork tests the whole chunk with the multiline `HTML_LINE` regex.
fn can_freeze(text: &str) -> bool {
    let mut lines = text.split('\n').peekable();
    while let Some(line) = lines.next() {
        if is_html_line(line, lines.peek().is_some()) {
            return false;
        }
    }
    !has_potential_reference_link(text)
}

/// `deriveStreamingMarkdownChunks`: the chunks of `text`, covering it exactly, in order.
pub fn chunks(text: &str, is_streaming: bool) -> Vec<Chunk> {
    let whole = || {
        vec![Chunk {
            range: 0..text.len(),
            is_streaming,
        }]
    };
    if !is_streaming || text.len() < MIN_FROZEN_CHARS {
        return whole();
    }

    let lines = split_lines(text);
    let mut boundaries = Vec::new();
    let mut frozen_start = 0;
    let mut mode: Option<BlockMode> = None;
    let mut fence: Option<Fence> = None;
    let mut index = 0;
    while index < lines.len() {
        let line = &lines[index];
        if let Some(open) = fence {
            if closes_fence(line.content, open) {
                fence = None;
            }
            index += 1;
            continue;
        }
        if let Some(opening) = fence_from_line(line.content) {
            fence = Some(opening);
            mode.get_or_insert(BlockMode::Fence);
            index += 1;
            continue;
        }
        if !line.content.trim().is_empty() {
            mode.get_or_insert_with(|| block_mode(line.content));
            index += 1;
            continue;
        }

        let mut next_index = index + 1;
        while lines
            .get(next_index)
            .is_some_and(|next| next.content.trim().is_empty())
        {
            next_index += 1;
        }
        let (Some(next), Some(current_mode)) = (lines.get(next_index), mode) else {
            index += 1;
            continue;
        };
        if continues_block(current_mode, next.content) {
            index = next_index;
            continue;
        }

        let boundary = next.start;
        let candidate = &text[frozen_start..boundary];
        if !can_freeze(candidate) {
            break;
        }
        if candidate.len() >= MIN_FROZEN_CHARS {
            boundaries.push(boundary);
            frozen_start = boundary;
        }
        mode = Some(block_mode(next.content));
        index = next_index;
    }

    if boundaries.is_empty() {
        return vec![Chunk {
            range: 0..text.len(),
            is_streaming: true,
        }];
    }
    let mut chunks = Vec::with_capacity(boundaries.len() + 1);
    let mut start = 0;
    for end in boundaries {
        chunks.push(Chunk {
            range: start..end,
            is_streaming: false,
        });
        start = end;
    }
    chunks.push(Chunk {
        range: start..text.len(),
        is_streaming: true,
    });
    chunks
}
