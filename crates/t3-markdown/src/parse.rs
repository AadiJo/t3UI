//! pulldown-cmark events -> [`Document`], reproducing what the fork's react-markdown pipeline
//! (remark-gfm, optional remark-breaks, rehype-raw + rehype-sanitize) shows:
//!
//! - GFM tables, task lists, strikethrough, footnotes and literal autolinks (bare URLs, `www.`,
//!   emails). pulldown-cmark has no literal autolinks, so [`autolink`] adds them.
//! - Whitespace collapses like HTML text; soft breaks are spaces, or line breaks with
//!   `line_breaks` (user messages).
//! - Links pass through the fork's `urlTransform`; file-path destinations become chips, external
//!   links get a favicon and break anywhere after their protocol.
//! - Raw HTML is sanitized: comments and `script`/`style` content disappear, a few inline tags
//!   style text, `<details>`/`<summary>` become a collapsible, other tags are unwrapped.

use std::{collections::HashMap, iter::Peekable};

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, OffsetIter, Options, Parser, Tag, TagEnd,
};

use crate::{
    document::{
        ATOM_CHAR, Align, Atom, AtomKind, Block, CodeBlock, Details, Document, Footnote, Inline,
        InlineStyle, Link, List, ListItem, Span, Table,
    },
    links,
};

/// Inputs that change how a chunk parses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParseOptions {
    /// remark-breaks: single newlines become line breaks (user messages).
    pub line_breaks: bool,
    /// The thread's workspace root, used to resolve relative file links into chips.
    pub cwd: Option<String>,
}

/// Parses one markdown chunk.
pub fn parse(source: &str, options: &ParseOptions) -> Document {
    let mut flags = Options::empty();
    flags.insert(Options::ENABLE_TABLES);
    flags.insert(Options::ENABLE_FOOTNOTES);
    flags.insert(Options::ENABLE_STRIKETHROUGH);
    flags.insert(Options::ENABLE_TASKLISTS);
    let mut parser = BlockParser {
        events: Parser::new_ext(source, flags).into_offset_iter().peekable(),
        options,
        footnote_numbers: HashMap::new(),
        footnote_definitions: HashMap::new(),
        task_markers: Vec::new(),
    };
    let mut blocks = parser.blocks_until(None);
    let mut footnotes: Vec<Footnote> = parser
        .footnote_numbers
        .iter()
        .filter_map(|(label, number)| {
            Some(Footnote {
                number: *number,
                blocks: parser.footnote_definitions.remove(label)?,
            })
        })
        .collect();
    if !footnotes.is_empty() {
        footnotes.sort_by_key(|footnote| footnote.number);
        blocks.push(Block::Footnotes(footnotes));
    }
    Document { blocks }
}

struct BlockParser<'a, 'o> {
    events: Peekable<OffsetIter<'a>>,
    options: &'o ParseOptions,
    /// GFM numbers footnotes in order of first reference.
    footnote_numbers: HashMap<String, usize>,
    footnote_definitions: HashMap<String, Vec<Block>>,
    /// One slot per list item being parsed. The task marker arrives first in a tight item but
    /// inside the first paragraph of a loose one, so it is recorded wherever it shows up.
    task_markers: Vec<Option<bool>>,
}

/// An open `<details>` collecting blocks until `</details>`.
struct OpenDetails {
    id: usize,
    summary: Option<Inline>,
    open: bool,
    blocks: Vec<Block>,
}

impl<'a> BlockParser<'a, '_> {
    /// Parses blocks until `end` (or the end of input), with any inline events directly inside the
    /// container (tight list items) collected as a tight paragraph.
    fn blocks_until(&mut self, end: Option<TagEnd>) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut details: Vec<OpenDetails> = Vec::new();
        let mut tight: Option<InlineBuilder> = None;

        macro_rules! push {
            ($block:expr) => {{
                let block = $block;
                match details.last_mut() {
                    Some(open) => open.blocks.push(block),
                    None => blocks.push(block),
                }
            }};
        }
        macro_rules! flush_tight {
            () => {
                if let Some(builder) = tight.take() {
                    let content = builder.finish();
                    if !content.is_empty() {
                        push!(Block::Paragraph {
                            content,
                            tight: true
                        });
                    }
                }
            };
        }

        while let Some((event, range)) = self.events.next() {
            match event {
                Event::End(tag_end) if Some(tag_end) == end => break,
                Event::End(_) => {}
                Event::Start(tag) => match tag {
                    Tag::Paragraph => {
                        flush_tight!();
                        let content = self.inline_until(TagEnd::Paragraph);
                        if !content.is_empty() {
                            push!(Block::Paragraph {
                                content,
                                tight: false
                            });
                        }
                    }
                    Tag::Heading { level, .. } => {
                        flush_tight!();
                        let content = self.inline_until(TagEnd::Heading(level));
                        push!(Block::Heading {
                            level: heading_level(level),
                            content,
                        });
                    }
                    Tag::BlockQuote(kind) => {
                        flush_tight!();
                        let children = self.blocks_until(Some(TagEnd::BlockQuote(kind)));
                        push!(Block::Quote(children));
                    }
                    Tag::CodeBlock(kind) => {
                        flush_tight!();
                        push!(Block::Code(self.code_block(kind, range.start)));
                    }
                    Tag::List(start) => {
                        flush_tight!();
                        push!(Block::List(self.list(start)));
                    }
                    Tag::Table(alignments) => {
                        flush_tight!();
                        push!(Block::Table(self.table(&alignments, range.start)));
                    }
                    Tag::FootnoteDefinition(label) => {
                        flush_tight!();
                        let children = self.blocks_until(Some(TagEnd::FootnoteDefinition));
                        self.footnote_definitions
                            .entry(label.to_string())
                            .or_insert(children);
                    }
                    Tag::HtmlBlock => {
                        flush_tight!();
                        let html = self.html_block();
                        self.apply_html_block(&html, range.start, &mut details, &mut |block| {
                            blocks.push(block)
                        });
                    }
                    Tag::Item
                    | Tag::TableHead
                    | Tag::TableRow
                    | Tag::TableCell
                    | Tag::DefinitionList
                    | Tag::DefinitionListTitle
                    | Tag::DefinitionListDefinition
                    | Tag::MetadataBlock(_) => {
                        // Not produced with our options outside their parents; skip contents.
                        let _ = self.blocks_until(Some(tag.to_end()));
                    }
                    inline_tag => {
                        let builder = tight.get_or_insert_with(|| InlineBuilder::new(self.options));
                        builder.event(
                            Event::Start(inline_tag),
                            &mut self.footnote_numbers,
                            &mut self.events,
                        );
                    }
                },
                Event::Rule => {
                    flush_tight!();
                    push!(Block::Rule);
                }
                Event::Html(html) => {
                    flush_tight!();
                    let html = html.to_string();
                    self.apply_html_block(&html, range.start, &mut details, &mut |block| {
                        blocks.push(block)
                    });
                }
                Event::DisplayMath(_) | Event::InlineMath(_) => {}
                Event::TaskListMarker(checked) => self.record_task_marker(checked),
                inline_event => {
                    let builder = tight.get_or_insert_with(|| InlineBuilder::new(self.options));
                    builder.event(inline_event, &mut self.footnote_numbers, &mut self.events);
                }
            }
        }
        flush_tight!();
        // Unclosed `<details>` still render, as the browser would auto-close them.
        while let Some(open) = details.pop() {
            let block = Block::Details(Details {
                id: open.id,
                summary: open.summary,
                open: open.open,
                blocks: open.blocks,
            });
            match details.last_mut() {
                Some(parent) => parent.blocks.push(block),
                None => blocks.push(block),
            }
        }
        blocks
    }

    /// Parses inline events until `end` into one [`Inline`].
    fn inline_until(&mut self, end: TagEnd) -> Inline {
        let mut builder = InlineBuilder::new(self.options);
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::End(tag_end) if tag_end == end => break,
                Event::TaskListMarker(checked) => self.record_task_marker(checked),
                event => builder.event(event, &mut self.footnote_numbers, &mut self.events),
            }
        }
        builder.finish()
    }

    fn record_task_marker(&mut self, checked: bool) {
        if let Some(slot) = self.task_markers.last_mut() {
            slot.get_or_insert(checked);
        }
    }

    fn code_block(&mut self, kind: CodeBlockKind<'a>, id: usize) -> CodeBlock {
        let mut code = String::new();
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Text(text) => code.push_str(&text),
                Event::End(TagEnd::CodeBlock) => break,
                _ => {}
            }
        }
        if code.ends_with('\n') {
            code.pop();
        }
        let (language, title) = match &kind {
            CodeBlockKind::Fenced(info) => {
                let info = info.trim();
                let (word, meta) = info
                    .split_once(char::is_whitespace)
                    .map_or((info, ""), |(word, meta)| (word, meta.trim()));
                let language = match word {
                    "" => "text",
                    // Shiki doesn't bundle a gitignore grammar; the fork shows ini (#685).
                    "gitignore" => "ini",
                    word => word,
                };
                (language.to_string(), fence_title(meta))
            }
            CodeBlockKind::Indented => ("text".to_string(), None),
        };
        CodeBlock {
            id,
            language,
            title,
            code,
        }
    }

    fn list(&mut self, start: Option<u64>) -> List {
        let end = TagEnd::List(start.is_some());
        let mut items = Vec::new();
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::End(tag_end) if tag_end == end => break,
                Event::Start(Tag::Item) => {
                    self.task_markers.push(None);
                    let blocks = self.blocks_until(Some(TagEnd::Item));
                    let task = self.task_markers.pop().flatten();
                    items.push(ListItem { task, blocks });
                }
                _ => {}
            }
        }
        List { start, items }
    }

    fn table(&mut self, alignments: &[Alignment], id: usize) -> Table {
        let mut header = Vec::new();
        let mut rows: Vec<Vec<Inline>> = Vec::new();
        let mut in_head = false;
        while let Some((event, _)) = self.events.next() {
            match event {
                Event::End(TagEnd::Table) => break,
                Event::Start(Tag::TableHead) => in_head = true,
                Event::End(TagEnd::TableHead) => in_head = false,
                Event::Start(Tag::TableRow) => rows.push(Vec::new()),
                Event::Start(Tag::TableCell) => {
                    let cell = self.inline_until(TagEnd::TableCell);
                    if in_head {
                        header.push(cell);
                    } else if let Some(row) = rows.last_mut() {
                        row.push(cell);
                    }
                }
                _ => {}
            }
        }
        Table {
            id,
            alignments: alignments
                .iter()
                .map(|alignment| match alignment {
                    Alignment::None => Align::None,
                    Alignment::Left => Align::Left,
                    Alignment::Center => Align::Center,
                    Alignment::Right => Align::Right,
                })
                .collect(),
            header,
            rows,
        }
    }

    /// Concatenates the raw HTML of an HTML block.
    fn html_block(&mut self) -> String {
        let mut html = String::new();
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Html(text) | Event::Text(text) => html.push_str(&text),
                Event::End(TagEnd::HtmlBlock) => break,
                _ => {}
            }
        }
        html
    }

    /// Applies one raw HTML block: opens/closes `<details>`, or renders sanitized text.
    fn apply_html_block(
        &mut self,
        html: &str,
        offset: usize,
        details: &mut Vec<OpenDetails>,
        push_top: &mut dyn FnMut(Block),
    ) {
        let mut text = String::new();
        let mut summary: Option<String> = None;
        let mut in_summary = false;
        let mut push = |block: Block, details: &mut Vec<OpenDetails>| match details.last_mut() {
            Some(open) => open.blocks.push(block),
            None => push_top(block),
        };
        for token in html::tokens(html) {
            match token {
                html::Token::Text(value) => {
                    if in_summary {
                        summary.get_or_insert_with(String::new).push_str(&value);
                    } else {
                        text.push_str(&value);
                    }
                }
                html::Token::Open { name, attrs, .. } => match name.as_str() {
                    "details" => {
                        flush_html_text(&mut text, self.options, |block| push(block, details));
                        details.push(OpenDetails {
                            id: offset,
                            summary: None,
                            open: html::has_attr(&attrs, "open"),
                            blocks: Vec::new(),
                        });
                    }
                    "summary" => in_summary = true,
                    "br" => {
                        if in_summary {
                            summary.get_or_insert_with(String::new).push(' ');
                        } else {
                            text.push('\n');
                        }
                    }
                    "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        flush_html_text(&mut text, self.options, |block| push(block, details));
                    }
                    _ => {}
                },
                html::Token::Close { name } => match name.as_str() {
                    "summary" => {
                        in_summary = false;
                        if let (Some(open), Some(value)) = (details.last_mut(), summary.take()) {
                            open.summary = Some(plain_inline(value.trim(), self.options));
                        }
                    }
                    "details" => {
                        flush_html_text(&mut text, self.options, |block| push(block, details));
                        if let Some(open) = details.pop() {
                            push(
                                Block::Details(Details {
                                    id: open.id,
                                    summary: open.summary,
                                    open: open.open,
                                    blocks: open.blocks,
                                }),
                                details,
                            );
                        }
                    }
                    "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        flush_html_text(&mut text, self.options, |block| push(block, details));
                    }
                    _ => {}
                },
            }
        }
        flush_html_text(&mut text, self.options, |block| push(block, details));
    }
}

/// Emits accumulated sanitized HTML text as a paragraph.
fn flush_html_text(text: &mut String, options: &ParseOptions, mut push: impl FnMut(Block)) {
    let value = std::mem::take(text);
    let value = value.trim();
    if !value.is_empty() {
        push(Block::Paragraph {
            content: plain_inline(value, options),
            tight: false,
        });
    }
}

/// Plain text as an [`Inline`], whitespace collapsed (newlines from `<br>` kept).
fn plain_inline(text: &str, options: &ParseOptions) -> Inline {
    let mut builder = InlineBuilder::new(options);
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            builder.hard_break();
        }
        builder.text(line);
    }
    builder.finish()
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// `extractFenceTitle`: `title="x.ts"` / `file=x` / `filename='x'`, else a bare `path/name.ext`.
fn fence_title(meta: &str) -> Option<String> {
    if meta.is_empty() {
        return None;
    }
    let lower = meta.to_ascii_lowercase();
    for key in ["title=", "file=", "filename="] {
        let mut search = 0;
        while let Some(found) = lower[search..].find(key) {
            let at = search + found;
            search = at + key.len();
            let boundary = at == 0 || meta[..at].ends_with(char::is_whitespace);
            if !boundary {
                continue;
            }
            let value = &meta[at + key.len()..];
            let title = match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value[1..]
                    .find(quote)
                    .map(|end| &value[1..1 + end])
                    .filter(|title| !title.is_empty()),
                Some(_) => value.split_whitespace().next(),
                None => None,
            };
            if let Some(title) = title {
                return Some(title.to_string());
            }
        }
    }
    // `FENCE_FILENAME_TOKEN_REGEX`: /^[\w@][\w@./-]*\.[A-Za-z0-9]+$/
    meta.split_whitespace()
        .find(|token| {
            let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_' || ch == '@';
            let Some(dot) = token.rfind('.') else {
                return false;
            };
            let extension = &token[dot + 1..];
            token.starts_with(word)
                && token[..dot]
                    .chars()
                    .all(|ch| word(ch) || matches!(ch, '.' | '/' | '-'))
                && !extension.is_empty()
                && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .map(str::to_string)
}

/// An open link while building inline content.
struct OpenLink {
    start: usize,
    href: String,
    nowrap_until: Option<usize>,
    /// Set until the first text of a favicon link arrives, to place `nowrap_until`.
    awaiting_leading: bool,
}

/// Accumulates inline events into an [`Inline`].
struct InlineBuilder<'o> {
    options: &'o ParseOptions,
    inline: Inline,
    style: InlineStyle,
    /// Styles to restore when an inline tag (markdown or raw HTML) closes.
    style_stack: Vec<(StyleKey, InlineStyle)>,
    links: Vec<OpenLink>,
    /// Consecutive plain text, held back so literal autolinks can span text events.
    pending: String,
    /// Whether the text so far ends in collapsible whitespace (or is empty).
    ends_with_space: bool,
}

/// What pushed a style, so the matching close restores it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum StyleKey {
    Markdown(TagEnd),
    Html(String),
}

impl<'o> InlineBuilder<'o> {
    fn new(options: &'o ParseOptions) -> Self {
        Self {
            options,
            inline: Inline::default(),
            style: InlineStyle::default(),
            style_stack: Vec::new(),
            links: Vec::new(),
            pending: String::new(),
            ends_with_space: true,
        }
    }

    fn event(
        &mut self,
        event: Event<'_>,
        footnotes: &mut HashMap<String, usize>,
        events: &mut Peekable<OffsetIter<'_>>,
    ) {
        match event {
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                self.flush_pending();
                let mut style = self.style;
                style.code = true;
                // Code spans keep their spaces in markdown, but HTML still collapses them.
                let collapsed = collapse_whitespace(&code, false);
                self.append(&collapsed, style);
                self.ends_with_space = false;
            }
            Event::SoftBreak => {
                if self.options.line_breaks {
                    self.hard_break();
                } else {
                    self.text(" ");
                }
            }
            Event::HardBreak => self.hard_break(),
            Event::InlineHtml(html) | Event::Html(html) => self.inline_html(&html),
            Event::FootnoteReference(label) => {
                self.flush_pending();
                let next = footnotes.len() + 1;
                let number = *footnotes.entry(label.to_string()).or_insert(next);
                self.atom(AtomKind::FootnoteRef { number });
            }
            Event::TaskListMarker(_) | Event::Rule => {}
            Event::Start(tag) => self.start(tag, events),
            Event::End(tag_end) => self.end(tag_end),
            Event::InlineMath(_) | Event::DisplayMath(_) => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>, events: &mut Peekable<OffsetIter<'_>>) {
        self.flush_pending();
        let end = tag.to_end();
        match tag {
            Tag::Emphasis => self.push_style(StyleKey::Markdown(end), |style| style.italic = true),
            Tag::Strong => self.push_style(StyleKey::Markdown(end), |style| style.bold = true),
            Tag::Strikethrough => {
                self.push_style(StyleKey::Markdown(end), |style| style.strike = true)
            }
            Tag::Superscript => {
                self.push_style(StyleKey::Markdown(end), |style| style.superscript = true)
            }
            Tag::Subscript => {
                self.push_style(StyleKey::Markdown(end), |style| style.subscript = true)
            }
            Tag::Link { dest_url, .. } => {
                let href = transform_url(&dest_url);
                let normalized = links::normalize_destination(&href).to_string();
                if let Some(link) =
                    links::resolve_file_link(&normalized, self.options.cwd.as_deref())
                {
                    // The chip replaces the link's children entirely.
                    skip_until_end(events, &TagEnd::Link);
                    self.atom(AtomKind::FileChip {
                        link,
                        href: normalized,
                    });
                    return;
                }
                self.open_link(href);
            }
            Tag::Image { dest_url, .. } => {
                let mut alt = String::new();
                for (event, _) in events.by_ref() {
                    match event {
                        Event::End(TagEnd::Image) => break,
                        Event::Text(text) | Event::Code(text) => alt.push_str(&text),
                        _ => {}
                    }
                }
                self.atom(AtomKind::Image {
                    url: transform_url(&dest_url),
                    alt,
                });
            }
            _ => {}
        }
    }

    fn end(&mut self, tag_end: TagEnd) {
        self.flush_pending();
        if tag_end == TagEnd::Link {
            self.close_link();
            return;
        }
        let key = StyleKey::Markdown(tag_end);
        if let Some(index) = self.style_stack.iter().rposition(|(open, _)| *open == key) {
            self.style = self.style_stack[index].1;
            self.style_stack.truncate(index);
        }
    }

    fn push_style(&mut self, key: StyleKey, change: impl FnOnce(&mut InlineStyle)) {
        self.style_stack.push((key, self.style));
        change(&mut self.style);
    }

    fn open_link(&mut self, href: String) {
        let start = self.inline.text.len();
        let host = external_host(&href);
        if let Some(host) = &host {
            self.atom(AtomKind::Favicon { host: host.clone() });
        }
        self.links.push(OpenLink {
            start,
            href,
            nowrap_until: None,
            awaiting_leading: host.is_some(),
        });
        self.ends_with_space = false;
    }

    fn close_link(&mut self) {
        let Some(open) = self.links.pop() else {
            return;
        };
        let end = self.inline.text.len();
        if end > open.start {
            self.inline.links.push(Link {
                range: open.start..end,
                href: open.href,
                nowrap_until: open.nowrap_until.or(open.awaiting_leading.then_some(end)),
            });
        }
    }

    /// Plain text from markdown: collapsed, autolinked when outside links and code.
    fn text(&mut self, text: &str) {
        let collapsed = collapse_whitespace(text, self.ends_with_space);
        if collapsed.is_empty() {
            return;
        }
        self.ends_with_space = collapsed.ends_with(' ');
        if self.links.is_empty() && !self.style.code {
            self.pending.push_str(&collapsed);
        } else {
            self.append_link_text(&collapsed);
        }
    }

    /// Appends text inside a link, recording where a favicon link's no-wrap lead ends.
    fn append_link_text(&mut self, text: &str) {
        let start = self.inline.text.len();
        self.append(text, self.style);
        if let Some(open) = self.links.last_mut()
            && open.awaiting_leading
        {
            open.awaiting_leading = false;
            let lead = leading_link_text_len(text);
            open.nowrap_until = Some(start + lead);
        }
    }

    fn hard_break(&mut self) {
        self.flush_pending();
        // Trailing spaces before a break never render.
        if self.inline.text.ends_with(' ') {
            self.trim_trailing_space();
        }
        self.append("\n", self.style);
        self.ends_with_space = true;
    }

    fn trim_trailing_space(&mut self) {
        self.inline.text.pop();
        if let Some(last) = self.inline.spans.last_mut() {
            last.range.end -= 1;
            if last.range.is_empty() {
                self.inline.spans.pop();
            }
        }
    }

    /// Flushes held-back text, turning literal URLs and emails into links.
    fn flush_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut cursor = 0;
        for found in autolink::find(&pending) {
            if found.range.start > cursor {
                self.append(&pending[cursor..found.range.start], self.style);
            }
            self.open_link(found.href);
            self.append_link_text(&pending[found.range.clone()]);
            self.close_link();
            cursor = found.range.end;
        }
        if cursor < pending.len() {
            self.append(&pending[cursor..], self.style);
        }
    }

    fn append(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        let start = self.inline.text.len();
        self.inline.text.push_str(text);
        let end = self.inline.text.len();
        match self.inline.spans.last_mut() {
            Some(last) if last.style == style && last.range.end == start => last.range.end = end,
            _ => self.inline.spans.push(Span {
                range: start..end,
                style,
            }),
        }
    }

    fn atom(&mut self, kind: AtomKind) {
        self.flush_pending();
        let offset = self.inline.text.len();
        let mut buffer = [0; 4];
        self.append(ATOM_CHAR.encode_utf8(&mut buffer), self.style);
        self.inline.atoms.push(Atom { offset, kind });
        self.ends_with_space = false;
    }

    /// Inline raw HTML, sanitized: a few tags style text, others are dropped (text kept).
    fn inline_html(&mut self, html: &str) {
        self.flush_pending();
        for token in html::tokens(html) {
            match token {
                html::Token::Text(text) => self.text(&text),
                html::Token::Open {
                    name,
                    attrs,
                    self_closing,
                } => {
                    let key = StyleKey::Html(name.clone());
                    match name.as_str() {
                        "br" => self.hard_break(),
                        "b" | "strong" => self.push_style(key, |style| style.bold = true),
                        "i" | "em" | "var" => self.push_style(key, |style| style.italic = true),
                        "s" | "del" | "strike" => self.push_style(key, |style| style.strike = true),
                        "code" => self.push_style(key, |style| style.code = true),
                        "kbd" | "samp" | "tt" => self.push_style(key, |style| style.mono = true),
                        "sup" => self.push_style(key, |style| style.superscript = true),
                        "sub" => self.push_style(key, |style| style.subscript = true),
                        "a" if !self_closing => {
                            let href = html::attr(&attrs, "href").unwrap_or_default();
                            self.style_stack.push((key, self.style));
                            self.open_link(transform_url(&href));
                        }
                        _ => {}
                    }
                }
                html::Token::Close { name } => {
                    if name == "a" {
                        self.close_link();
                    }
                    let key = StyleKey::Html(name);
                    if let Some(index) = self.style_stack.iter().rposition(|(open, _)| *open == key)
                    {
                        self.style = self.style_stack[index].1;
                        self.style_stack.truncate(index);
                    }
                }
            }
        }
    }

    fn finish(mut self) -> Inline {
        self.flush_pending();
        while !self.links.is_empty() {
            self.close_link();
        }
        // Trailing whitespace never renders and would only confuse wrapping and copy.
        while self.inline.text.ends_with(' ') {
            self.trim_trailing_space();
        }
        self.inline.links.sort_by_key(|link| link.range.start);
        self.inline
    }
}

/// Consumes events through the matching end tag.
fn skip_until_end(events: &mut Peekable<OffsetIter<'_>>, end: &TagEnd) {
    let mut depth = 0usize;
    for (event, _) in events.by_ref() {
        match &event {
            Event::Start(tag) if tag.to_end() == *end => depth += 1,
            Event::End(tag_end) if tag_end == end => {
                if depth == 0 {
                    return;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
}

/// HTML whitespace collapsing: runs of spaces, tabs and newlines become one space, and a leading
/// space is dropped when the text so far already ends with one.
fn collapse_whitespace(text: &str, after_space: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = after_space;
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c') {
            if !in_space {
                out.push(' ');
                in_space = true;
            }
        } else {
            out.push(ch);
            in_space = false;
        }
    }
    out
}

/// The fork's `urlTransform`: `file://` URLs become paths, unsafe protocols become empty.
fn transform_url(url: &str) -> String {
    if let Some(path) = links::rewrite_file_uri(url) {
        return path;
    }
    if links::is_safe_url(url) {
        url.to_string()
    } else {
        String::new()
    }
}

/// `resolveExternalLinkHost`: the host of an `http(s)` URL.
fn external_host(href: &str) -> Option<String> {
    let lower = href.get(..8).unwrap_or(href).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &href[8..]
    } else if lower.starts_with("http://") {
        &href[7..]
    } else {
        return None;
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = if host.starts_with('[') {
        host.split(']').next().map(|value| format!("{value}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// `leadingExternalLinkTextLength`: the protocol, else the first character.
fn leading_link_text_len(text: &str) -> usize {
    let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    if lower.starts_with("https://") {
        8
    } else if lower.starts_with("http://") {
        7
    } else {
        text.chars().next().map_or(0, char::len_utf8)
    }
}

/// GFM literal autolinks (remark-gfm's `gfm-autolink-literal`), approximately: `http(s)://`,
/// `www.` and email addresses, with trailing punctuation and unbalanced `)` excluded.
pub(crate) mod autolink {
    use std::ops::Range;

    pub struct Found {
        pub range: Range<usize>,
        pub href: String,
    }

    fn is_boundary_before(text: &str, at: usize) -> bool {
        text[..at]
            .chars()
            .next_back()
            .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '*' | '_' | '~' | '(' | '"'))
    }

    fn domain_end(text: &str, start: usize) -> usize {
        start
            + text[start..]
                .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.')))
                .unwrap_or(text.len() - start)
    }

    /// Extends past the domain through the path, then trims GFM's trailing punctuation.
    fn url_end(text: &str, domain_end: usize) -> usize {
        let mut end = domain_end
            + text[domain_end..]
                .find(|ch: char| ch.is_whitespace() || ch == '<')
                .unwrap_or(text.len() - domain_end);
        loop {
            let candidate = &text[..end];
            let Some(last) = candidate.chars().next_back() else {
                break;
            };
            if matches!(
                last,
                '?' | '!' | '.' | ',' | ':' | '*' | '_' | '~' | '\'' | '"'
            ) {
                end -= last.len_utf8();
            } else if last == ')' {
                let segment = &text[..end];
                let opens = segment.matches('(').count();
                let closes = segment.matches(')').count();
                if closes > opens {
                    end -= 1;
                } else {
                    break;
                }
            } else if last == ';' {
                // `&amp;`-style entity at the end is not part of the URL.
                let entity_start = candidate[..candidate.len() - 1].rfind('&').filter(|amp| {
                    candidate[amp + 1..candidate.len() - 1]
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric())
                });
                match entity_start {
                    Some(amp) => end = amp,
                    None => break,
                }
            } else {
                break;
            }
        }
        end
    }

    fn valid_domain(domain: &str) -> bool {
        let labels: Vec<&str> = domain.split('.').collect();
        !domain.is_empty()
            && labels.len() >= 2
            && labels.iter().all(|label| !label.is_empty())
            // The last two labels may not contain underscores.
            && labels[labels.len() - 2..].iter().all(|label| !label.contains('_'))
    }

    pub fn find(text: &str) -> Vec<Found> {
        let mut found = Vec::new();
        let mut index = 0;
        let lower = text.to_ascii_lowercase();
        while index < text.len() {
            let rest = &lower[index..];
            let protocol = if rest.starts_with("https://") {
                Some(8)
            } else if rest.starts_with("http://") {
                Some(7)
            } else {
                None
            };
            if let Some(protocol) = protocol
                && is_boundary_before(text, index)
            {
                let domain_start = index + protocol;
                let domain_end = domain_end(text, domain_start);
                if domain_end > domain_start {
                    let end = url_end(text, domain_end);
                    if end > domain_start {
                        found.push(Found {
                            range: index..end,
                            href: text[index..end].to_string(),
                        });
                        index = end;
                        continue;
                    }
                }
            }
            if rest.starts_with("www.") && is_boundary_before(text, index) {
                let domain_end = domain_end(text, index);
                if valid_domain(&text[index..domain_end]) {
                    let end = url_end(text, domain_end);
                    found.push(Found {
                        range: index..end,
                        href: format!("http://{}", &text[index..end]),
                    });
                    index = end;
                    continue;
                }
            }
            if text.as_bytes()[index] == b'@' {
                let local_start = text[..index]
                    .rfind(|ch: char| {
                        !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '+' | '-' | '_'))
                    })
                    .map_or(0, |at| at + 1);
                let domain_end = domain_end(text, index + 1);
                let mut end = domain_end;
                while end > index + 1 && text[..end].ends_with(['.', '-', '_']) {
                    end -= 1;
                }
                let already = found
                    .last()
                    .is_some_and(|last| last.range.end > local_start);
                if local_start < index && !already && valid_domain(&text[index + 1..end]) {
                    found.push(Found {
                        range: local_start..end,
                        href: format!("mailto:{}", &text[local_start..end]),
                    });
                    index = end;
                    continue;
                }
            }
            index += text[index..].chars().next().map_or(1, char::len_utf8);
        }
        found
    }
}

/// A tiny HTML tokenizer for sanitizing raw HTML: tags, text (entities decoded), and nothing
/// from comments or `script`/`style` elements.
pub(crate) mod html {
    pub enum Token {
        Text(String),
        Open {
            name: String,
            attrs: Vec<(String, String)>,
            self_closing: bool,
        },
        Close {
            name: String,
        },
    }

    pub fn has_attr(attrs: &[(String, String)], name: &str) -> bool {
        attrs.iter().any(|(key, _)| key == name)
    }

    pub fn attr(attrs: &[(String, String)], name: &str) -> Option<String> {
        attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    fn decode_entities(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(amp) = rest.find('&') {
            out.push_str(&rest[..amp]);
            rest = &rest[amp..];
            let Some(semi) = rest[..rest.len().min(12)].find(';') else {
                out.push('&');
                rest = &rest[1..];
                continue;
            };
            let entity = &rest[1..semi];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                    .and_then(char::from_u32),
            };
            match decoded {
                Some(ch) => {
                    out.push(ch);
                    rest = &rest[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn parse_attrs(source: &str) -> Vec<(String, String)> {
        let mut attrs = Vec::new();
        let mut rest = source.trim();
        while !rest.is_empty() {
            let name_end = rest
                .find(|ch: char| ch.is_whitespace() || ch == '=' || ch == '/')
                .unwrap_or(rest.len());
            let name = rest[..name_end].to_ascii_lowercase();
            rest = rest[name_end..].trim_start();
            let mut value = String::new();
            if let Some(after) = rest.strip_prefix('=') {
                let after = after.trim_start();
                if let Some(quote @ ('"' | '\'')) = after.chars().next() {
                    let close = after[1..].find(quote).map_or(after.len(), |end| end + 1);
                    value = decode_entities(&after[1..close.min(after.len())]);
                    rest = after.get(close + 1..).unwrap_or("");
                } else {
                    let end = after.find(char::is_whitespace).unwrap_or(after.len());
                    value = decode_entities(&after[..end]);
                    rest = &after[end..];
                }
            } else if name.is_empty() {
                rest = rest.get(1..).unwrap_or("");
            }
            if !name.is_empty() {
                attrs.push((name, value));
            }
            rest = rest.trim_start();
        }
        attrs
    }

    pub fn tokens(html: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut rest = html;
        let mut skip_until: Option<String> = None;
        while !rest.is_empty() {
            let Some(lt) = rest.find('<') else {
                if skip_until.is_none() {
                    tokens.push(Token::Text(decode_entities(rest)));
                }
                break;
            };
            if lt > 0 && skip_until.is_none() {
                tokens.push(Token::Text(decode_entities(&rest[..lt])));
            }
            rest = &rest[lt..];
            if let Some(after) = rest.strip_prefix("<!--") {
                rest = after.find("-->").map_or("", |end| &after[end + 3..]);
                continue;
            }
            let Some(gt) = rest.find('>') else {
                if skip_until.is_none() {
                    tokens.push(Token::Text(decode_entities(rest)));
                }
                break;
            };
            let inner = &rest[1..gt];
            rest = &rest[gt + 1..];
            let (closing, inner) = match inner.strip_prefix('/') {
                Some(inner) => (true, inner),
                None => (false, inner),
            };
            let name_end = inner
                .find(|ch: char| ch.is_whitespace() || ch == '/')
                .unwrap_or(inner.len());
            let name = inner[..name_end].to_ascii_lowercase();
            if name.is_empty() || !name.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
                if skip_until.is_none() {
                    tokens.push(Token::Text(decode_entities(&format!("<{inner}>"))));
                }
                continue;
            }
            if let Some(skipped) = &skip_until {
                if closing && *skipped == name {
                    skip_until = None;
                }
                continue;
            }
            if closing {
                tokens.push(Token::Close { name });
            } else if matches!(name.as_str(), "script" | "style" | "textarea" | "title") {
                skip_until = Some(name);
            } else {
                let self_closing = inner.trim_end().ends_with('/');
                let attrs_source = inner[name_end..].trim_end().trim_end_matches('/');
                tokens.push(Token::Open {
                    name,
                    attrs: parse_attrs(attrs_source),
                    self_closing,
                });
            }
        }
        tokens
    }
}
