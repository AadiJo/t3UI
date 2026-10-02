//! [`DiffView`]: the diff panel body. A virtualized list of file cards that reproduces
//! `@pierre/diffs`' `CodeView` as the web client configures it (`DiffPanel.tsx`): sticky file
//! headers, unified or split rows, wrap or per-file horizontal scroll, collapsible files.
//!
//! Cards are flat: `index.css` gives `diffs-container` a border and radius, but the selector
//! (`.diff-render-surface > diffs-container`) does not match Pierre 1.3's nesting, so the app
//! renders headers and code edge to edge, 8px apart (verified in Chromium).

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ListAlignment, ListOffset, ListState, ParentElement as _, Pixels, Render, ScrollWheelEvent,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window, div,
    prelude::FluentBuilder as _, px, svg,
};

use super::{
    highlight::{self, FileHighlights},
    icons,
    rows::{RowPainter, display_width},
    style::{
        BODY_BOTTOM, CARD_GAP, COUNT_SIZE, HEADER_HEIGHT, HEADER_TEXT_SIZE, LINE_HEIGHT, Metrics,
        SANS, gpui, mono_family, theme_tokens,
    },
};
use crate::{
    palette::{Appearance, DiffPalette},
    patch::{ChangeKind, FileDiff, RenderablePatch, renderable_patch},
    rows::{DiffStyle, Row, Side, build_rows, is_two_column},
};

/// Events for the panel that embeds the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffViewEvent {
    /// The user clicked a file header's path; the panel opens it as a file surface
    /// (`openDiffFilePrimaryAction`).
    OpenFile(SharedString),
}

/// One list item. Each file is a card: a header item, one item per body row, and a footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Item {
    Header(usize),
    Row { file: usize, row: usize },
    Footer(usize),
}

impl Item {
    fn file(self) -> usize {
        match self {
            Self::Header(file) | Self::Footer(file) | Self::Row { file, .. } => file,
        }
    }
}

/// Parsed state of one file for the current style.
struct FileEntry {
    rows: Vec<Row>,
    /// Digits of the largest line number (`--diffs-min-number-column-width-default`).
    digits: usize,
    /// Widest line in display columns per side, for horizontal scroll limits.
    columns: [usize; 2],
}

/// The diff body. Create with [`DiffView::new`], feed patches with [`DiffView::set_patch`] and
/// subscribe to [`DiffViewEvent`].
pub struct DiffView {
    files: Arc<[FileDiff]>,
    entries: Vec<FileEntry>,
    raw: Option<(SharedString, &'static str)>,
    style: DiffStyle,
    wrap: bool,
    palette: DiffPalette,
    mono: SharedString,
    collapsed: HashSet<String>,
    items: Vec<Item>,
    list: ListState,
    scroll_x: HashMap<(usize, Side), Pixels>,
    highlights: Vec<Option<Arc<FileHighlights>>>,
    highlight_task: Option<Task<()>>,
    metrics: Option<Metrics>,
    measured_width: Pixels,
}

impl EventEmitter<DiffViewEvent> for DiffView {}

impl DiffView {
    /// An empty view in the active `t3-ui` theme.
    pub fn new(cx: &App) -> Self {
        let (tokens, appearance) = theme_tokens(cx);
        Self {
            files: Arc::from([]),
            entries: Vec::new(),
            raw: None,
            style: DiffStyle::default(),
            wrap: false,
            palette: DiffPalette::new(&tokens, appearance),
            mono: mono_family(cx),
            collapsed: HashSet::new(),
            items: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            scroll_x: HashMap::new(),
            highlights: Vec::new(),
            highlight_task: None,
            metrics: None,
            measured_width: px(0.),
        }
    }

    /// Shows `patch` (the server's diff text). Resets collapse and scroll state, like a new
    /// selection in the web panel.
    pub fn set_patch(&mut self, patch: &str, cx: &mut Context<Self>) {
        let files = match renderable_patch(patch) {
            RenderablePatch::Files(files) => {
                self.raw = None;
                files
            }
            RenderablePatch::Raw { text, reason } => {
                self.raw = Some((text.into(), reason));
                Vec::new()
            }
            RenderablePatch::Empty => {
                self.raw = None;
                Vec::new()
            }
        };
        self.files = files.into();
        self.collapsed.clear();
        self.scroll_x.clear();
        self.rebuild_entries();
        self.rebuild_items();
        self.list
            .reset_with_uniform_height(self.items.len(), LINE_HEIGHT);
        self.start_highlighting(cx);
        cx.notify();
    }

    /// Parsed files, sorted by path.
    pub fn files(&self) -> &[FileDiff] {
        &self.files
    }

    pub fn style(&self) -> DiffStyle {
        self.style
    }

    /// Stacked (unified) or split rows. Keeps the file at the top of the viewport in view.
    pub fn set_style(&mut self, style: DiffStyle, cx: &mut Context<Self>) {
        if self.style == style {
            return;
        }
        let top_file = self.top_file();
        self.style = style;
        self.scroll_x.clear();
        self.rebuild_entries();
        self.rebuild_items();
        self.list
            .reset_with_uniform_height(self.items.len(), LINE_HEIGHT);
        if let Some(file) = top_file {
            self.scroll_to_file(file);
        }
        cx.notify();
    }

    pub fn wrap(&self) -> bool {
        self.wrap
    }

    /// Wraps long lines (`overflow: wrap`) or scrolls each file horizontally (`scroll`).
    pub fn set_wrap(&mut self, wrap: bool, cx: &mut Context<Self>) {
        if self.wrap == wrap {
            return;
        }
        self.wrap = wrap;
        self.scroll_x.clear();
        self.list.remeasure();
        cx.notify();
    }

    /// Re-reads the `t3-ui` theme (call after the appearance changes) and re-highlights with
    /// the matching Pierre theme.
    pub fn sync_theme(&mut self, cx: &mut Context<Self>) {
        let (tokens, appearance) = theme_tokens(cx);
        let rehighlight = self.palette.appearance != appearance;
        self.palette = DiffPalette::new(&tokens, appearance);
        self.mono = mono_family(cx);
        if rehighlight {
            self.start_highlighting(cx);
        }
        cx.notify();
    }

    /// Scrolls `path`'s card to the top, like selecting a file in a turn's changed-files list.
    pub fn reveal_file(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(file) = self.files.iter().position(|file| file.path == path) {
            self.scroll_to_file(file);
            cx.notify();
        }
    }

    /// Whether `path` is collapsed to its header.
    pub fn is_collapsed(&self, path: &str) -> bool {
        self.collapsed.contains(path)
    }

    /// Collapses or expands `path`'s body, like its header chevron.
    pub fn set_collapsed(&mut self, path: &str, collapsed: bool, cx: &mut Context<Self>) {
        if let Some(file) = self.files.iter().position(|file| file.path == path)
            && self.is_collapsed(path) != collapsed
        {
            self.toggle_collapsed(file, cx);
        }
    }

    fn toggle_collapsed(&mut self, file: usize, cx: &mut Context<Self>) {
        let Some(path) = self.files.get(file).map(|file| file.path.clone()) else {
            return;
        };
        let old_range = self.item_range(file);
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path);
        }
        self.rebuild_items();
        let new_range = self.item_range(file);
        self.list.splice(old_range, new_range.len());
        cx.notify();
    }

    /// Scrolls the list by `delta` pixels (positive moves content up). For tests and scenes.
    pub fn scroll_by(&mut self, delta: Pixels, cx: &mut Context<Self>) {
        self.list.scroll_by(delta);
        cx.notify();
    }

    fn rebuild_entries(&mut self) {
        self.entries = self
            .files
            .iter()
            .map(|file| FileEntry {
                rows: build_rows(file, self.style),
                digits: file.total_lines().max(1).to_string().len(),
                columns: [
                    file.old_lines
                        .iter()
                        .map(|line| display_width(line))
                        .max()
                        .unwrap_or(0),
                    file.new_lines
                        .iter()
                        .map(|line| display_width(line))
                        .max()
                        .unwrap_or(0),
                ],
            })
            .collect();
    }

    fn has_body(&self, file: usize) -> bool {
        self.entries
            .get(file)
            .is_some_and(|entry| !entry.rows.is_empty())
            && !self.collapsed.contains(&self.files[file].path)
    }

    fn rebuild_items(&mut self) {
        let mut items = Vec::new();
        for file in 0..self.files.len() {
            items.push(Item::Header(file));
            if self.has_body(file) {
                items.extend((0..self.entries[file].rows.len()).map(|row| Item::Row { file, row }));
                items.push(Item::Footer(file));
            }
        }
        self.items = items;
    }

    /// Item indices of `file`'s card.
    fn item_range(&self, file: usize) -> std::ops::Range<usize> {
        let start = self
            .items
            .iter()
            .position(|item| item.file() == file)
            .unwrap_or(self.items.len());
        let len = self.items[start..]
            .iter()
            .take_while(|item| item.file() == file)
            .count();
        start..start + len
    }

    fn top_file(&self) -> Option<usize> {
        let top = self.list.logical_scroll_top();
        self.items.get(top.item_ix).map(|item| item.file())
    }

    /// Puts the top of `file`'s card at the top of the viewport (CodeView `scrollTo` with
    /// `align: "start"`).
    fn scroll_to_file(&mut self, file: usize) {
        let item_ix = self.item_range(file).start;
        self.list.scroll_to(ListOffset {
            item_ix,
            offset_in_item: CARD_GAP,
        });
    }

    fn start_highlighting(&mut self, cx: &mut Context<Self>) {
        let files = self.files.clone();
        let theme = highlight::theme(self.palette.appearance);
        self.highlights = vec![None; files.len()];
        self.highlight_task = Some(cx.spawn(async move |this, cx| {
            for ix in 0..files.len() {
                let files = files.clone();
                let highlights = cx
                    .background_spawn(async move { highlight::highlight_file(&files[ix], theme) })
                    .await;
                let applied = this.update(cx, |this, cx| {
                    if let Some(slot) = this.highlights.get_mut(ix) {
                        *slot = Some(Arc::new(highlights));
                    }
                    cx.notify();
                });
                if applied.is_err() {
                    return;
                }
            }
        }));
    }

    fn scroll_horizontally(
        &mut self,
        file: usize,
        side: Side,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(LINE_HEIGHT).x;
        if delta == px(0.) || self.wrap {
            return;
        }
        let max = self.max_scroll_x(file, side);
        let offset = self.scroll_x.entry((file, side)).or_default();
        let next = (*offset - delta).clamp(px(0.), max);
        if next != *offset {
            *offset = next;
            cx.notify();
        }
    }

    /// How far a file side's code can scroll: its widest line plus padding, minus the code
    /// cell width.
    fn max_scroll_x(&self, file: usize, side: Side) -> Pixels {
        let (Some(entry), Some(metrics)) = (self.entries.get(file), self.metrics) else {
            return px(0.);
        };
        let two_column = is_two_column(&self.files[file], self.style);
        let columns = if two_column {
            entry.columns[side_index(side)]
        } else {
            entry.columns[0].max(entry.columns[1])
        };
        let width = self.list.viewport_bounds().size.width;
        let half = if two_column {
            width / 2. - px(1.)
        } else {
            width
        };
        let code_width = half - metrics.gutter_width(entry.digits) - super::style::GUTTER_BORDER;
        (metrics.code_ch * (columns + 2) as f32 - code_width).max(px(0.))
    }

    fn render_item(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(&item) = self.items.get(ix) else {
            return div().into_any_element();
        };
        let mono = self.mono.clone();
        let metrics = *self
            .metrics
            .get_or_insert_with(|| Metrics::measure(window, &mono));
        let palette = self.palette;
        let last_file = self.files.len().saturating_sub(1);
        match item {
            Item::Header(file) => div()
                .pt(CARD_GAP)
                .when(!self.has_body(file) && file == last_file, |this| {
                    this.pb(CARD_GAP)
                })
                .child(self.render_header(file, metrics, cx))
                .into_any_element(),
            Item::Footer(file) => div()
                .when(file == last_file, |this| this.pb(CARD_GAP))
                .child(div().h(BODY_BOTTOM).bg(gpui(palette.surface)))
                .into_any_element(),
            Item::Row { file, row } => {
                let painter = RowPainter {
                    file: &self.files[file],
                    file_ix: file,
                    rows: &self.entries[file].rows,
                    highlights: self.highlights.get(file).and_then(Option::as_deref),
                    palette: &palette,
                    metrics,
                    mono: self.mono.clone(),
                    gutter_width: metrics.gutter_width(self.entries[file].digits),
                    wrap: self.wrap,
                    two_column: is_two_column(&self.files[file], self.style),
                    scroll_x: [
                        self.scroll_x
                            .get(&(file, Side::Old))
                            .copied()
                            .unwrap_or_default(),
                        self.scroll_x
                            .get(&(file, Side::New))
                            .copied()
                            .unwrap_or_default(),
                    ],
                };
                painter.render(row, cx)
            }
        }
    }

    fn render_header(
        &self,
        file_ix: usize,
        metrics: Metrics,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let palette = self.palette;
        let file = &self.files[file_ix];
        let collapsed = self.collapsed.contains(&file.path);
        let kind_color = match file.kind {
            ChangeKind::Added => palette.addition,
            ChangeKind::Deleted => palette.deletion,
            _ => palette.modified,
        };
        let path: SharedString = file.path.clone().into();
        let (additions, deletions) = (file.additions, file.deletions);
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(8.))
            .h(HEADER_HEIGHT)
            .px(px(16.))
            .bg(gpui(palette.header))
            .border_color(gpui(palette.header_border))
            .border_b_1()
            .font_family(SANS)
            .text_size(HEADER_TEXT_SIZE)
            .line_height(HEADER_TEXT_SIZE)
            .text_color(gpui(palette.text))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .min_w_0()
                    .child(
                        div()
                            .id(("diff-collapse", file_ix))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(20.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|button| button.bg(gpui(palette.button_hover)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_collapsed(file_ix, cx);
                            }))
                            .child(
                                svg()
                                    .path(if collapsed {
                                        icons::CHEVRON_RIGHT
                                    } else {
                                        icons::CHEVRON_DOWN
                                    })
                                    .size(px(16.))
                                    .text_color(gpui(kind_color)),
                            ),
                    )
                    .child(
                        svg()
                            .data(icons::change_icon(file.kind))
                            .flex_none()
                            .size(px(16.))
                            .text_color(gpui(kind_color)),
                    )
                    .when_some(file.previous_path.clone(), |content, previous| {
                        content
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis_start()
                                    .opacity(0.7)
                                    .child(previous),
                            )
                            .child(
                                svg()
                                    .data(icons::ARROW_RIGHT_SHORT)
                                    .flex_none()
                                    .size(px(16.))
                                    .text_color(gpui(palette.text)),
                            )
                    })
                    .child(
                        div()
                            .id(("diff-title", file_ix))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis_start()
                            .cursor_pointer()
                            .hover(|title| title.text_color(gpui(palette.title_hover)).underline())
                            .on_click(cx.listener({
                                let path = path.clone();
                                move |_, _, _, cx| cx.emit(DiffViewEvent::OpenFile(path.clone()))
                            }))
                            .child(path),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .flex_row()
                    .items_center()
                    .gap(metrics.header_ch)
                    .font_family(self.mono.clone())
                    .text_size(COUNT_SIZE)
                    .line_height(COUNT_SIZE)
                    .when(deletions > 0 || additions == 0, |meta| {
                        meta.child(
                            div()
                                .text_color(gpui(palette.deletion))
                                .child(format!("-{deletions}")),
                        )
                    })
                    .when(additions > 0 || deletions == 0, |meta| {
                        meta.child(
                            div()
                                .text_color(gpui(palette.addition))
                                .child(format!("+{additions}")),
                        )
                    }),
            )
    }

    /// The header pinned to the top of the viewport while its card scrolls under it, pushed up
    /// by the card's bottom like CSS `position: sticky`.
    fn sticky_header(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let metrics = self.metrics?;
        let top = self.list.logical_scroll_top();
        let item = *self.items.get(top.item_ix)?;
        let file = item.file();
        if !self.has_body(file) {
            return None;
        }
        // The header starts after the card gap.
        if let Item::Header(_) = item
            && top.offset_in_item <= CARD_GAP
        {
            return None;
        }
        let viewport = self.list.viewport_bounds();
        let footer_ix = self.item_range(file).end - 1;
        let offset = self
            .list
            .bounds_for_item(footer_ix)
            .map(|footer| {
                let card_inner_bottom = footer.top() - viewport.top() + BODY_BOTTOM;
                (card_inner_bottom - HEADER_HEIGHT).min(px(0.))
            })
            .unwrap_or_default();
        Some(
            div()
                .absolute()
                .top(offset)
                .left_0()
                .right_0()
                .child(self.render_header(file, metrics, cx))
                .into_any_element(),
        )
    }

    fn render_raw(&self, text: SharedString, reason: &'static str) -> AnyElement {
        let palette = self.palette;
        div()
            .id("diff-raw")
            .size_full()
            .overflow_y_scroll()
            .p(px(8.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .font_family(SANS)
                    .text_size(px(11.))
                    .text_color(gpui(palette.raw_reason))
                    .child(reason),
            )
            .child(
                div()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(gpui(palette.raw_border))
                    .bg(gpui(palette.raw_fill))
                    .p(px(12.))
                    .font_family(self.mono.clone())
                    .text_size(px(11.))
                    .line_height(px(11. * 1.625))
                    .text_color(gpui(palette.raw_text))
                    .when(!self.wrap, |pre| pre.whitespace_nowrap())
                    .children(
                        text.lines()
                            .map(|line| div().min_h(px(11. * 1.625)).child(line.to_owned())),
                    ),
            )
            .into_any_element()
    }
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Old => 0,
        Side::New => 1,
    }
}

impl Render for DiffView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.metrics = Some(Metrics::measure(window, &self.mono));
        let palette = self.palette;
        let root = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(gpui(palette.viewport));
        if let Some((text, reason)) = self.raw.clone() {
            return root.child(self.render_raw(text, reason));
        }
        if self.files.is_empty() {
            return root
                .flex()
                .items_center()
                .justify_center()
                .px(px(12.))
                .font_family(SANS)
                .text_size(px(12.))
                .text_color(gpui(palette.placeholder))
                .child("No net changes in this selection.");
        }
        // Wrapped rows change height with the width.
        let width = self.list.viewport_bounds().size.width;
        if self.wrap && width != self.measured_width {
            self.measured_width = width;
            self.list.remeasure();
        }
        let sticky = self.sticky_header(cx);
        let scrollbar = self.scrollbar();
        root.flex()
            .flex_row()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        gpui_kit::list(
                            self.list.clone(),
                            cx.processor(|this, ix, window, cx| this.render_item(ix, window, cx)),
                        )
                        .size_full(),
                    )
                    .children(sticky),
            )
            .children(scrollbar)
    }
}

/// Width of the app's styled native scrollbar (`index.css` `::-webkit-scrollbar`).
const SCROLLBAR_WIDTH: Pixels = px(6.);

impl DiffView {
    /// The vertical scrollbar: a 6px column with a rounded thumb, present while the content
    /// overflows, like Chromium's classic scrollbar under the app's `::-webkit-scrollbar` rules.
    fn scrollbar(&self) -> Option<AnyElement> {
        let viewport = self.list.viewport_bounds().size.height;
        let max = self.list.max_offset_for_scrollbar().y;
        if max <= px(0.) || viewport <= px(0.) {
            return None;
        }
        let content = viewport + max;
        let thumb = (viewport * (viewport / content)).max(px(20.));
        let scrolled = -self.list.scroll_px_offset_for_scrollbar().y;
        let top = (viewport - thumb) * (scrolled / max).clamp(0., 1.);
        let color = match self.palette.appearance {
            Appearance::Light => crate::color::Rgba::hex(0x00000026),
            Appearance::Dark => crate::color::Rgba::hex(0xFFFFFF1A),
        };
        Some(
            div()
                .relative()
                .flex_none()
                .w(SCROLLBAR_WIDTH)
                .h_full()
                .child(
                    div()
                        .absolute()
                        .top(top)
                        .left_0()
                        .w(SCROLLBAR_WIDTH)
                        .h(thumb)
                        .rounded(px(3.))
                        .bg(gpui(color)),
                )
                .into_any_element(),
        )
    }
}

impl DiffView {
    /// Scroll a file side horizontally from a row's wheel handler.
    pub(super) fn on_row_scroll(
        &mut self,
        file: usize,
        side: Side,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        self.scroll_horizontally(file, side, event, cx);
    }
}
