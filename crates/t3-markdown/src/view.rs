//! [`Markdown`]: the entity a chat row owns for one message's markdown.

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui_kit::{
    AppContext as _, ClipboardItem, Context, Entity, EventEmitter, IntoElement, Pixels, Render,
    SharedString, Window,
};
use t3_highlight::{Highlighted, Language, StreamingHighlighter, Theme};

use crate::{
    Document, ParseOptions,
    document::{AtomKind, Block, CodeBlock, Inline},
    links::{self, FileLink},
    parse::parse,
    streaming,
    style::MarkdownStyle,
    units::{BlockView, Unit, UnitKey, UnitMetrics},
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
    /// (code length, theme) being highlighted on the background executor.
    pub pending: Option<(usize, Theme)>,
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
    /// Cached views for the blocks of stable chunks (see [`crate::units`]).
    pub(crate) units: HashMap<UnitKey, Unit>,
    /// The message width laid out last frame; cached blocks are valid only at that width.
    pub(crate) last_width: Rc<Cell<Option<Pixels>>>,
    pub(crate) block_caching: bool,
}

impl EventEmitter<MarkdownEvent> for Markdown {}

impl Markdown {
    pub fn new(
        text: impl Into<SharedString>,
        options: MarkdownOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        // Load the grammar sets off the main thread before the first code block needs them.
        static PRELOADED: std::sync::Once = std::sync::Once::new();
        PRELOADED.call_once(|| {
            cx.background_spawn(async { t3_highlight::preload() })
                .detach()
        });
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
            units: HashMap::new(),
            last_width: Rc::default(),
            block_caching: true,
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
        self.units.clear();
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
        // Cached blocks live as long as their chunk's parse.
        self.units.retain(|(start, _), unit| {
            chunks
                .iter()
                .any(|chunk| chunk.start == *start && Rc::ptr_eq(&chunk.document, &unit.document))
        });
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

    /// The highlighted form of a code block: the per-block cache, the streaming highlighter
    /// (tail blocks while streaming), `t3_highlight`'s global cache, or `None` while it is being
    /// highlighted on the background executor (render it plain meanwhile).
    pub(crate) fn highlight(
        &mut self,
        id: usize,
        block: &CodeBlock,
        in_tail: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<Arc<Highlighted>> {
        let word_wrap = self.options.word_wrap;
        let streaming = self.streaming && in_tail;
        let state = self.code.entry(id).or_insert_with(|| CodeState {
            wrap: word_wrap,
            highlighted: None,
            pending: None,
            streaming: None,
        });
        let len = block.code.len();
        if let Some((cached_len, cached_theme, highlighted)) = &state.highlighted
            && *cached_len == len
            && *cached_theme == theme
        {
            return Some(highlighted.clone());
        }
        let language = Language::from_fence(&block.language);
        let highlighted = if streaming {
            // A growing block: incremental, proportional to the new lines.
            let highlighter = state
                .streaming
                .get_or_insert_with(|| StreamingHighlighter::new(language, theme));
            if highlighter.language() != language {
                *highlighter = StreamingHighlighter::new(language, theme);
            }
            highlighter.update(&block.code)
        } else if let Some(hit) = t3_highlight::cached(&block.code, language, theme) {
            hit
        } else {
            // Like the fork's Suspense fallback: plain until the background highlight lands.
            if state.pending != Some((len, theme)) {
                state.pending = Some((len, theme));
                let code = block.code.clone();
                let task =
                    cx.background_spawn(
                        async move { t3_highlight::highlight(&code, language, theme) },
                    );
                cx.spawn(async move |this, cx| {
                    let highlighted = task.await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some(state) = this.code.get_mut(&id)
                            && state.pending == Some((len, theme))
                        {
                            state.pending = None;
                            state.highlighted = Some((len, theme, highlighted));
                            this.invalidate_block(id);
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            return None;
        };
        state.highlighted = Some((len, theme, highlighted.clone()));
        Some(highlighted)
    }

    pub(crate) fn toggle_wrap(&mut self, id: usize, cx: &mut Context<Self>) {
        let word_wrap = self.options.word_wrap;
        let state = self.code.entry(id).or_insert_with(|| CodeState {
            wrap: word_wrap,
            highlighted: None,
            pending: None,
            streaming: None,
        });
        state.wrap = !state.wrap;
        self.invalidate_block(id);
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
        self.invalidate_block(id);
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
        self.invalidate_block(id);
        cx.notify();
    }

    /// Copies `text` and shows the check icon on block `id` for 1.2s, like the fork.
    pub(crate) fn copy(&mut self, id: usize, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied.insert(id);
        self.invalidate_block(id);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(1200))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied.remove(&id);
                this.invalidate_block(id);
                cx.notify();
            });
        })
        .detach();
    }

    /// Makes the cached block containing stateful block `id` render again (its look changed).
    pub(crate) fn invalidate_block(&mut self, id: usize) {
        for unit in self.units.values() {
            if unit.metrics.block_ids.borrow().contains(&id) {
                unit.metrics.size.set(None);
            }
        }
    }

    /// The cached view for block `key` of `document`, created on first use.
    pub(crate) fn unit(
        &mut self,
        key: UnitKey,
        document: &Rc<Document>,
        cx: &mut Context<Self>,
    ) -> (Entity<BlockView>, Rc<UnitMetrics>) {
        if let Some(unit) = self.units.get(&key)
            && Rc::ptr_eq(&unit.document, document)
        {
            return (unit.view.clone(), unit.metrics.clone());
        }
        let markdown = cx.entity().downgrade();
        let view = cx.new(|_| BlockView { markdown, key });
        let metrics = Rc::new(UnitMetrics::default());
        self.units.insert(
            key,
            Unit {
                document: document.clone(),
                view: view.clone(),
                metrics: metrics.clone(),
            },
        );
        (view, metrics)
    }

    /// Turns render caching of stable blocks on or off (on by default). For benchmarks.
    #[doc(hidden)]
    pub fn set_block_caching(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.block_caching = enabled;
        cx.notify();
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
