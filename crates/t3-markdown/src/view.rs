//! [`Markdown`]: the entity a chat row owns for one message's markdown.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui_kit::{ClipboardItem, Context, EventEmitter, IntoElement, Render, SharedString, Window};
use t3_highlight::{Highlighted, Language, StreamingHighlighter, Theme};

use crate::{
    Document, ParseOptions,
    document::{AtomKind, Block, CodeBlock, Inline},
    links::{self, FileLink},
    parse::parse,
    streaming,
    style::MarkdownStyle,
};

/// How a message's markdown renders. Mirrors `ChatMarkdown`'s props.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownOptions {
    /// The thread's workspace root; relative file links resolve against it into chips.
    pub cwd: Option<String>,
    /// remark-breaks: single newlines are line breaks (user messages).
    pub line_breaks: bool,
    /// The client `wordWrap` setting: initial state of code wrapping and table expansion.
    /// The fork defaults it to on.
    pub word_wrap: bool,
    /// Text at full `foreground` (user messages) instead of `foreground/80` (assistant).
    pub full_foreground: bool,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            line_breaks: false,
            word_wrap: true,
            full_foreground: false,
        }
    }
}

/// What the user asked the markdown to do; the owner decides how.
#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownEvent {
    /// A link to a URL (or `#fragment`, `mailto:`) was clicked.
    OpenUrl(SharedString),
    /// A file chip was clicked: open it in the file preview or editor.
    OpenFile(FileLink),
}

/// A parsed chunk of the document. Frozen chunks are shared between updates.
#[derive(Clone)]
pub(crate) struct ParsedChunk {
    pub start: usize,
    pub document: Rc<Document>,
}

/// Per-code-block UI state, keyed by the block's byte offset in the message.
pub(crate) struct CodeState {
    pub wrap: bool,
    /// The highlight for (code length, theme), reused while the code is unchanged.
    pub highlighted: Option<(usize, Theme, Arc<Highlighted>)>,
    /// Incremental highlighter while the block is in the streaming tail.
    pub streaming: Option<StreamingHighlighter>,
}

/// One message's markdown: parsed (incrementally while streaming) and rendered like the fork's
/// `ChatMarkdown`.
///
/// ```ignore
/// let markdown = cx.new(|cx| Markdown::new(message.text.clone(), options, cx));
/// cx.subscribe(&markdown, |this, _, event: &MarkdownEvent, cx| { /* open url/file */ });
/// // Each streamed delta: pass the full text so far.
/// markdown.update(cx, |markdown, cx| markdown.set_text(text, message.streaming, cx));
/// ```
///
/// While streaming, the text splits into frozen chunks plus a live tail
/// ([`crate::streaming`]); frozen chunks are parsed and highlighted once, so an update costs a
/// re-parse of the tail only. Selection and copy go through the window's text selection: the
/// window root must render `gpui_kit::base::TextSelectionLayer`.
pub struct Markdown {
    text: SharedString,
    streaming: bool,
    options: MarkdownOptions,
    parse_options: ParseOptions,
    chunks: Vec<ParsedChunk>,
    /// Parsed frozen chunks by byte range; valid while new text extends the old.
    frozen: HashMap<(usize, usize), Rc<Document>>,
    /// Shortest unique parent paths for file chips whose names collide, by file path.
    suffixes: Rc<HashMap<String, String>>,
    style: Option<MarkdownStyle>,
    pub(crate) code: HashMap<usize, CodeState>,
    pub(crate) tables_expanded: HashMap<usize, bool>,
    pub(crate) details_open: HashMap<usize, bool>,
    /// Code blocks and tables showing the "copied" check.
    pub(crate) copied: HashSet<usize>,
}

impl EventEmitter<MarkdownEvent> for Markdown {}

impl Markdown {
    pub fn new(
        text: impl Into<SharedString>,
        options: MarkdownOptions,
        _cx: &mut Context<Self>,
    ) -> Self {
        let parse_options = ParseOptions {
            line_breaks: options.line_breaks,
            cwd: options.cwd.clone(),
        };
        let mut this = Self {
            text: SharedString::default(),
            streaming: false,
            options,
            parse_options,
            chunks: Vec::new(),
            frozen: HashMap::new(),
            suffixes: Rc::default(),
            style: None,
            code: HashMap::new(),
            tables_expanded: HashMap::new(),
            details_open: HashMap::new(),
            copied: HashSet::new(),
        };
        this.reparse(text.into(), false);
        this
    }

    /// Overrides the colors and fonts (by default they follow the `t3-ui` theme).
    pub fn with_style(mut self, style: MarkdownStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn set_style(&mut self, style: Option<MarkdownStyle>, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    /// The full markdown source.
    pub fn text(&self) -> &SharedString {
        &self.text
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// Replaces the source. While `streaming`, text that extends the previous text re-parses
    /// only the live tail.
    pub fn set_text(
        &mut self,
        text: impl Into<SharedString>,
        streaming: bool,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        if text == self.text && streaming == self.streaming {
            return;
        }
        self.reparse(text, streaming);
        cx.notify();
    }

    /// Marks the stream finished (or restarted) without changing the text.
    pub fn set_streaming(&mut self, streaming: bool, cx: &mut Context<Self>) {
        let text = self.text.clone();
        self.set_text(text, streaming, cx);
    }

    fn reparse(&mut self, text: SharedString, streaming: bool) {
        if !text.starts_with(self.text.as_ref()) {
            self.frozen.clear();
        }
        let mut chunks = Vec::new();
        for chunk in streaming::chunks(&text, streaming) {
            let Range { start, end } = chunk.range;
            let document = if chunk.is_streaming {
                Rc::new(parse(&text[start..end], &self.parse_options))
            } else if let Some(document) = self.frozen.get(&(start, end)) {
                document.clone()
            } else {
                let document = Rc::new(parse(&text[start..end], &self.parse_options));
                if streaming {
                    self.frozen.insert((start, end), document.clone());
                }
                document
            };
            chunks.push(ParsedChunk { start, document });
        }
        if !streaming {
            self.frozen.clear();
        }
        let mut paths = Vec::new();
        for chunk in &chunks {
            collect_file_paths(&chunk.document.blocks, &mut paths);
        }
        self.suffixes = Rc::new(links::parent_suffixes(paths.iter().map(String::as_str)));
        // Streaming highlighters only live while their block is still streaming.
        if !streaming {
            for state in self.code.values_mut() {
                state.streaming = None;
            }
        }
        self.text = text;
        self.streaming = streaming;
        self.chunks = chunks;
    }

    pub(crate) fn chunks(&self) -> &[ParsedChunk] {
        &self.chunks
    }

    pub(crate) fn options(&self) -> &MarkdownOptions {
        &self.options
    }

    pub(crate) fn suffixes(&self) -> Rc<HashMap<String, String>> {
        self.suffixes.clone()
    }

    pub(crate) fn resolved_style(&self, cx: &gpui_kit::App) -> MarkdownStyle {
        self.style
            .clone()
            .unwrap_or_else(|| MarkdownStyle::from_theme(cx))
    }

    /// The highlighted form of a code block, from the per-block cache, the streaming
    /// highlighter (tail blocks while streaming) or `t3_highlight`'s global cache.
    pub(crate) fn highlight(
        &mut self,
        id: usize,
        block: &CodeBlock,
        in_tail: bool,
        theme: Theme,
    ) -> Arc<Highlighted> {
        let word_wrap = self.options.word_wrap;
        let streaming = self.streaming && in_tail;
        let state = self.code.entry(id).or_insert_with(|| CodeState {
            wrap: word_wrap,
            highlighted: None,
            streaming: None,
        });
        if let Some((len, cached_theme, highlighted)) = &state.highlighted
            && *len == block.code.len()
            && *cached_theme == theme
        {
            return highlighted.clone();
        }
        let language = Language::from_fence(&block.language);
        let highlighted = if streaming {
            let highlighter = state
                .streaming
                .get_or_insert_with(|| StreamingHighlighter::new(language, theme));
            if highlighter.language() != language {
                *highlighter = StreamingHighlighter::new(language, theme);
            }
            highlighter.update(&block.code)
        } else {
            t3_highlight::highlight(&block.code, language, theme)
        };
        state.highlighted = Some((block.code.len(), theme, highlighted.clone()));
        highlighted
    }

    pub(crate) fn toggle_wrap(&mut self, id: usize, cx: &mut Context<Self>) {
        let word_wrap = self.options.word_wrap;
        let state = self.code.entry(id).or_insert_with(|| CodeState {
            wrap: word_wrap,
            highlighted: None,
            streaming: None,
        });
        state.wrap = !state.wrap;
        cx.notify();
    }

    pub(crate) fn code_wraps(&self, id: usize) -> bool {
        self.code
            .get(&id)
            .map_or(self.options.word_wrap, |state| state.wrap)
    }

    pub(crate) fn table_expanded(&self, id: usize) -> bool {
        self.tables_expanded
            .get(&id)
            .copied()
            .unwrap_or(self.options.word_wrap)
    }

    pub(crate) fn toggle_table(&mut self, id: usize, cx: &mut Context<Self>) {
        let expanded = !self.table_expanded(id);
        self.tables_expanded.insert(id, expanded);
        cx.notify();
    }

    pub(crate) fn toggle_details(
        &mut self,
        id: usize,
        initially_open: bool,
        cx: &mut Context<Self>,
    ) {
        let open = !self
            .details_open
            .get(&id)
            .copied()
            .unwrap_or(initially_open);
        self.details_open.insert(id, open);
        cx.notify();
    }

    /// Copies `text` and shows the check icon on block `id` for 1.2s, like the fork.
    pub(crate) fn copy(&mut self, id: usize, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied.insert(id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1200))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied.remove(&id);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn emit_url(&mut self, href: SharedString, cx: &mut Context<Self>) {
        cx.emit(MarkdownEvent::OpenUrl(href));
    }

    pub(crate) fn emit_file(&mut self, link: FileLink, cx: &mut Context<Self>) {
        cx.emit(MarkdownEvent::OpenFile(link));
    }
}

impl Render for Markdown {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::render::render_markdown(self, window, cx)
    }
}

fn collect_file_paths(blocks: &[Block], out: &mut Vec<String>) {
    let inline = |content: &Inline, out: &mut Vec<String>| {
        for atom in &content.atoms {
            if let AtomKind::FileChip { link, .. } = &atom.kind {
                out.push(link.file_path.clone());
            }
        }
    };
    for block in blocks {
        match block {
            Block::Paragraph { content, .. } | Block::Heading { content, .. } => {
                inline(content, out)
            }
            Block::Quote(children) => collect_file_paths(children, out),
            Block::List(list) => {
                for item in &list.items {
                    collect_file_paths(&item.blocks, out);
                }
            }
            Block::Table(table) => {
                for cell in table.header.iter().chain(table.rows.iter().flatten()) {
                    inline(cell, out);
                }
            }
            Block::Details(details) => collect_file_paths(&details.blocks, out),
            Block::Footnotes(notes) => {
                for note in notes {
                    collect_file_paths(&note.blocks, out);
                }
            }
            Block::Code(_) | Block::Rule => {}
        }
    }
}
