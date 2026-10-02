//! Copy-as-markdown for selections, after the fork's `markdown-clipboard.ts`: the selected part
//! of a block is re-serialized with its inline markup and block syntax, so copying out of the
//! rendered view keeps emphasis, links, code and fences. Plain string logic, no GPUI.

use std::ops::Range;

/// Inline markup over a byte range of a block's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Markup {
    Bold,
    Italic,
    Strike,
    Code,
    /// A link; only `http(s)` links keep their markdown, others copy their label.
    Link(String),
}

impl Markup {
    /// Outer-to-inner order for markers opening at the same position.
    fn rank(&self) -> u8 {
        match self {
            Self::Link(_) => 0,
            Self::Bold => 1,
            Self::Italic => 2,
            Self::Strike => 3,
            Self::Code => 4,
        }
    }
}

/// How a block's copied text is framed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyFormat {
    /// Before the first line: `## `, `- `, `1. `, `> `.
    pub first_prefix: String,
    /// Before every following line: quote `> ` or list continuation indent.
    pub line_prefix: String,
    /// `Some(language)` for a code block: fenced, no inline markup.
    pub fence: Option<String>,
    /// End with a newline, so blocks joined by the window's `\n` keep a blank line between them.
    pub block_gap: bool,
}

/// Serializes `text[range]` with the markup that intersects it. `atoms` are (offset, markdown)
/// for inline objects (chips copy `[name](href)`).
pub fn serialize(
    text: &str,
    range: Range<usize>,
    markup: &[(Range<usize>, Markup)],
    atoms: &[(usize, &str)],
    format: &CopyFormat,
) -> String {
    if let Some(language) = &format.fence {
        let code = text[range].trim_end_matches('\n');
        let longest = longest_run(code, '`');
        let fence = "`".repeat(if longest >= 3 { longest + 1 } else { 3 });
        let mut out = format!("{fence}{language}\n{code}\n{fence}");
        if format.block_gap {
            out.push('\n');
        }
        return out;
    }

    // Markers clipped to the selection; emphasis hoists surrounding whitespace outside.
    struct Open<'a> {
        start: usize,
        end: usize,
        markup: &'a Markup,
        close: String,
    }
    let mut markers: Vec<Open<'_>> = Vec::new();
    for (span, kind) in markup {
        let mut start = span.start.max(range.start);
        let mut end = span.end.min(range.end);
        if start >= end {
            continue;
        }
        if matches!(kind, Markup::Bold | Markup::Italic | Markup::Strike) {
            let inner = &text[start..end];
            start += inner.len() - inner.trim_start().len();
            end -= inner.len() - inner.trim_end().len();
            if start >= end {
                continue;
            }
        }
        if let Markup::Link(href) = kind {
            let label = &text[start..end];
            let is_http = href.starts_with("http://") || href.starts_with("https://");
            if !is_http || label.trim() == href {
                continue;
            }
        }
        markers.push(Open {
            start,
            end,
            markup: kind,
            close: String::new(),
        });
    }
    markers.sort_by_key(|marker| {
        (
            marker.start,
            std::cmp::Reverse(marker.end),
            marker.markup.rank(),
        )
    });

    let mut out = String::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut position = range.start;
    let mut chars = text[range.clone()].char_indices().peekable();
    loop {
        // Close markers ending here, innermost first.
        while let Some(&top) = stack.last() {
            if markers[top].end <= position {
                out.push_str(&markers[top].close);
                stack.pop();
            } else {
                break;
            }
        }
        if position >= range.end {
            break;
        }
        while next < markers.len() && markers[next].start == position {
            let marker = &mut markers[next];
            let (open, close) = match marker.markup {
                Markup::Bold => ("**".to_string(), "**".to_string()),
                Markup::Italic => ("*".to_string(), "*".to_string()),
                Markup::Strike => ("~~".to_string(), "~~".to_string()),
                Markup::Link(href) => ("[".to_string(), format!("]({href})")),
                Markup::Code => {
                    let code = &text[marker.start..marker.end];
                    let longest = longest_run(code, '`');
                    let fence = "`".repeat(if longest > 0 { longest + 1 } else { 1 });
                    let pad = if code.starts_with('`') || code.ends_with('`') {
                        " "
                    } else {
                        ""
                    };
                    (format!("{fence}{pad}"), format!("{pad}{fence}"))
                }
            };
            out.push_str(&open);
            marker.close = close;
            stack.push(next);
            next += 1;
        }
        let Some((offset, ch)) = chars.next() else {
            break;
        };
        let at = range.start + offset;
        match atoms.iter().find(|(atom, _)| *atom == at) {
            Some((_, markdown)) => out.push_str(markdown),
            None => out.push(ch),
        }
        position = chars
            .peek()
            .map_or(range.end, |(offset, _)| range.start + offset);
    }

    let mut lines = out.split('\n');
    let mut framed = String::with_capacity(out.len() + 8);
    if let Some(first) = lines.next() {
        framed.push_str(&format.first_prefix);
        framed.push_str(first);
    }
    for line in lines {
        framed.push('\n');
        if !line.is_empty() {
            framed.push_str(&format.line_prefix);
        }
        framed.push_str(line);
    }
    if format.block_gap {
        framed.push('\n');
    }
    framed
}

fn longest_run(text: &str, ch: char) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in text.chars() {
        if c == ch {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}
