//! [`FileBrowser`]: `FileBrowserPanel.tsx`, the workspace file tree with its 36px header
//! (project name, file count, search and refresh buttons), the always-visible search box and
//! the rows of `@pierre/trees` in compact density.
//!
//! The web mounts one browser per thread and workspace: it survives switching between the
//! `files` tab and the `file:*` tabs' explorer, keeping expansion, selection and data. Surfaces
//! get the same instance through [`FileBrowser::shared`].

use std::{collections::HashMap, f32::consts::TAU, sync::Arc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, AppContext as _, BoxShadow, Context, ElementId,
    Entity, FocusHandle, FontWeight, Global, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, Render, ScrollStrategy, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Transformation, UniformListScrollHandle, WeakEntity, Window,
    component::input::{Escape, Input, InputEvent, InputState, MoveDown, MoveUp},
    div, point,
    prelude::FluentBuilder as _,
    px, radians, svg, uniform_list,
};
use t3_logic::ThreadRef;
use t3_protocol::{
    methods::ProjectsListEntries,
    projects::{EntryKind, ProjectEntries, ProjectListEntriesInput},
};
use t3_ui::{ActiveColors as _, IconName, TooltipExt as _};

use super::{
    style::{
        CHEVRON_NUDGE, FilesPalette, GUIDE_OFFSET, ICON, ITEM_GAP, ITEM_PADDING, LEVEL_INDENT,
        ROW_HEIGHT, ROW_RADIUS, SANS, SEARCH_HEIGHT, SEARCH_RADIUS, TREE_INSET, TREE_TEXT,
    },
    tree::{FileTree, RowKind, TreeRow},
};
use crate::panels::context::PanelContext;

/// `file-tree-icon-chevron` from `@pierre/trees`' sprite, pointing down.
const CHEVRON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path d="M12.4697 5.46973C12.7626 5.17684 13.2374 5.17684 13.5303 5.46973C13.8232 5.76262 13.8232 6.23738 13.5303 6.53028L8.53028 11.5303C8.23738 11.8232 7.76262 11.8232 7.46973 11.5303L2.46973 6.53028C2.17684 6.23738 2.17684 5.76262 2.46973 5.46973C2.76262 5.17684 3.23738 5.17684 3.53028 5.46973L8 9.93946L12.4697 5.46973Z" fill="currentColor"/></svg>"#;

/// Hover group of the whole tree: hovering it shows the indent guides.
const TREE_GROUP: &str = "file-tree";
/// Hover group of a header icon button.
const HEADER_BUTTON_GROUP: &str = "file-browser-header-button";
/// Frame cap of the refresh spinner, like `t3_ui::Spinner`.
const SPIN_FPS: f32 = 30.;

/// The browser of one thread's workspace. Opening a file asks the right panel for a file tab.
pub struct FileBrowser {
    context: PanelContext,
    cwd: String,
    tree: FileTree,
    entries: Option<Arc<ProjectEntries>>,
    error: Option<String>,
    pending: bool,
    /// The clicked row (`data-item-selected`).
    selected: Option<String>,
    /// The keyboard row: ringed while the tree or its search box has focus.
    focused: Option<String>,
    search: Entity<InputState>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    _load: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// Live browsers by thread and workspace, so the `files` tab and the explorer of `file:*` tabs
/// share one tree.
#[derive(Default)]
struct SharedBrowsers(HashMap<(ThreadRef, String), WeakEntity<FileBrowser>>);

impl Global for SharedBrowsers {}

impl FileBrowser {
    /// The thread's browser of `cwd`, created (and its entries requested) on first use. It
    /// lives while a surface holds it.
    pub fn shared(
        context: &PanelContext,
        cwd: String,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let key = (context.thread.clone(), cwd.clone());
        let browsers = cx.default_global::<SharedBrowsers>();
        browsers.0.retain(|_, browser| browser.upgrade().is_some());
        if let Some(browser) = browsers.0.get(&key).and_then(WeakEntity::upgrade) {
            return browser;
        }
        let context = context.clone();
        let browser = cx.new(|cx| Self::new(context, cwd, window, cx));
        cx.default_global::<SharedBrowsers>()
            .0
            .insert(key, browser.downgrade());
        browser
    }

    fn new(
        context: PanelContext,
        cwd: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![cx.subscribe_in(
            &search,
            window,
            |this, search, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let query = search.read(cx).value();
                    this.set_search(Some(&query), cx);
                }
                InputEvent::PressEnter { .. } => {
                    if let Some(path) = this.focused.clone() {
                        this.activate(path, window, cx);
                    }
                }
                InputEvent::Blur => this.close_search(window, cx),
                InputEvent::Focus => {}
            },
        )];
        let mut this = Self {
            context,
            cwd,
            tree: FileTree::default(),
            entries: None,
            error: None,
            pending: false,
            selected: None,
            focused: None,
            search,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            _load: None,
            _subscriptions: subscriptions,
        };
        this.reload(cx);
        this
    }

    /// Requests the indexed listing (`projects.listEntries {cwd}`); also the refresh button.
    /// Rows already shown stay until the answer arrives.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.pending = true;
        let request = self.context.request::<ProjectsListEntries>(
            ProjectListEntriesInput {
                cwd: self.cwd.clone(),
                directory_path: None,
            },
            cx,
        );
        self._load = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| {
                this.pending = false;
                match result {
                    Ok(entries) => {
                        this.tree.set_entries(&entries.entries);
                        this.entries = Some(Arc::new(entries));
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// `{n} files`, `Indexing…` before the first answer, ` · partial` when truncated.
    fn count_label(&self) -> String {
        let Some(entries) = &self.entries else {
            return if self.pending {
                "Indexing…".into()
            } else {
                "0 files".into()
            };
        };
        let files = entries
            .entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::File)
            .count();
        let mut label = format!("{} files", group_thousands(files));
        if entries.truncated {
            label.push_str(" · partial");
        }
        label
    }

    fn set_search(&mut self, query: Option<&str>, cx: &mut Context<Self>) {
        self.tree.set_search(query, self.selected.as_deref());
        // The first match takes the keyboard row (`focusCandidate`).
        if let Some(first) = self.tree.rows().iter().find(|row| row.search_match) {
            self.focused = Some(first.path.clone());
        }
        cx.notify();
    }

    /// `openSearch()`: the header's search button focuses the search box.
    fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.search_query().is_none() {
            self.tree.set_search(Some(""), None);
        }
        self.search
            .update(cx, |search, cx| search.focus(window, cx));
        cx.notify();
    }

    /// `closeSearch()`: clears the box and restores the tree, keeping the selection visible.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.search_query().is_none() {
            return;
        }
        self.tree.set_search(None, self.selected.as_deref());
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        cx.notify();
    }

    /// A row click or Enter: selects the row, toggles a directory, opens a file.
    fn activate(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(path.clone());
        self.focused = Some(path.clone());
        self.close_search(window, cx);
        window.focus(&self.focus, cx);
        if self.tree.is_directory(&path) {
            self.tree.toggle(&path);
        } else {
            let thread = self.context.thread.clone();
            self.context
                .panels(cx)
                .update(cx, |panels, cx| panels.open_file(&thread, &path, None, cx));
        }
        cx.notify();
    }

    /// Moves the keyboard row by `delta` visible rows.
    fn move_focus(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.tree.rows();
        if rows.is_empty() {
            return;
        }
        let current = self
            .focused
            .as_ref()
            .and_then(|path| rows.iter().position(|row| &row.path == path));
        let next = match current {
            Some(index) => index.saturating_add_signed(delta).min(rows.len() - 1),
            None => 0,
        };
        self.focused = Some(rows[next].path.clone());
        self.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Tree keyboard: arrows move and fold, Enter/Space activate, a letter starts a search.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let focused_row = self
            .focused
            .as_ref()
            .and_then(|path| self.tree.rows().iter().find(|row| &row.path == path))
            .cloned();
        match keystroke.key.as_str() {
            "up" => self.move_focus(-1, cx),
            "down" => self.move_focus(1, cx),
            "enter" | "space" => {
                if let Some(row) = focused_row {
                    self.activate(row.path, window, cx);
                }
            }
            "left" | "right" => {
                if let Some(row) = focused_row.filter(|row| row.kind == RowKind::Directory)
                    && row.expanded == (keystroke.key == "left")
                {
                    self.tree.toggle(&row.path);
                    cx.notify();
                }
            }
            _ => {
                let seed = keystroke
                    .key_char
                    .clone()
                    .filter(|_| !keystroke.modifiers.modified())
                    .filter(|text| {
                        text.chars().count() == 1 && text.chars().all(char::is_alphanumeric)
                    });
                let Some(seed) = seed else {
                    return;
                };
                self.search.update(cx, |search, cx| {
                    search.set_value(seed.clone(), window, cx);
                    search.focus(window, cx);
                });
                self.set_search(Some(&seed), cx);
            }
        }
        cx.stop_propagation();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let project_name: SharedString = self
            .context
            .project(cx)
            .map(|project| project.title.clone())
            .unwrap_or_default()
            .into();
        let button = |id: &'static str, label: &'static str, icon: AnyElement| {
            div()
                .id(id)
                .group(HEADER_BUTTON_GROUP)
                .flex_none()
                .p(px(6.))
                .rounded(px(8.))
                .cursor_pointer()
                .hover(|style| style.bg(colors.accent))
                .tooltip_text(label)
                .child(icon)
        };
        let icon = |name: IconName| {
            svg()
                .path(name.path())
                .flex_none()
                .size(px(14.))
                .text_color(colors.muted_foreground)
                .group_hover(HEADER_BUTTON_GROUP, |style| {
                    style.text_color(colors.foreground)
                })
        };
        let refresh_icon = if self.pending {
            // `animate-spin` while the listing is pending.
            icon(IconName::RefreshCw)
                .with_animation(
                    "files-refresh-spin",
                    Animation::new(Duration::from_secs(1))
                        .repeat_synced()
                        .with_max_fps(SPIN_FPS),
                    |icon, turn| {
                        icon.with_transformation(Transformation::rotate(radians(turn * TAU)))
                    },
                )
                .into_any_element()
        } else {
            icon(IconName::RefreshCw).into_any_element()
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .px(px(12.))
            .border_b_1()
            .border_color(colors.border_60)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.foreground)
                            .child(project_name),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.))
                            .line_height(px(10.))
                            .text_color(colors.muted_foreground)
                            .child(self.count_label()),
                    ),
            )
            .child(
                button(
                    "files-search",
                    "Search workspace files",
                    icon(IconName::Search).into_any_element(),
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_search(window, cx))),
            )
            .child(
                button("files-refresh", "Refresh workspace files", refresh_icon)
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
    }

    /// The search box (`[data-file-tree-search-input]`), with its own placeholder so it can
    /// take the tree's color.
    fn render_search(
        &self,
        palette: &FilesPalette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let focused = self.search.read(cx).focus_handle().is_focused(window);
        let empty = self.search.read(cx).value().is_empty();
        div().flex_none().px(TREE_INSET).pb(ITEM_GAP).child(
            div()
                .id("file-tree-search")
                .relative()
                .my(px(1.))
                .h(SEARCH_HEIGHT)
                .px(ITEM_PADDING)
                .flex()
                .items_center()
                .rounded(SEARCH_RADIUS)
                .border_1()
                .border_color(if focused {
                    palette.focus_ring
                } else {
                    palette.search_border
                })
                .when(focused, |input| {
                    // A 2px outline at -1px offset: the border plus 1px outside it.
                    input.shadow(vec![BoxShadow {
                        color: palette.focus_ring,
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(1.),
                    }])
                })
                .bg(palette.search_bg)
                .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                    this.move_focus(1, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                    this.move_focus(-1, cx);
                    cx.stop_propagation();
                }))
                .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                    window.focus(&this.focus, cx);
                    this.close_search(window, cx);
                    cx.stop_propagation();
                }))
                .when(empty, |input| {
                    input.child(
                        div()
                            .absolute()
                            .left(ITEM_PADDING)
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .text_color(palette.search_placeholder)
                            .child("Search…"),
                    )
                })
                .child(
                    gpui_kit::Styled::h(
                        Input::new(&self.search)
                            .appearance(false)
                            .px_0()
                            .py_0()
                            .text_size(TREE_TEXT)
                            .text_color(colors.foreground),
                        ROW_HEIGHT,
                    )
                    .w_full(),
                ),
        )
    }

    /// One tree row (`button[data-type='item']`).
    fn render_row(
        &self,
        row: &TreeRow,
        palette: &FilesPalette,
        focus_parent: Option<&str>,
        ring: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let selected = self.selected.as_deref() == Some(row.path.as_str());
        let ringed = ring && self.focused.as_deref() == Some(row.path.as_str());
        let path = row.path.clone();
        let icon = match row.kind {
            RowKind::Directory => {
                let chevron = svg()
                    .data(CHEVRON)
                    .flex_none()
                    .size(ICON)
                    .text_color(if selected {
                        colors.foreground
                    } else {
                        palette.tree_muted
                    });
                if row.expanded {
                    chevron.relative().top(CHEVRON_NUDGE).into_any_element()
                } else {
                    chevron
                        .with_transformation(Transformation::rotate(radians(-TAU / 4.)))
                        .into_any_element()
                }
            }
            RowKind::File => t3_ui::file_icon(&row.path, palette.dark).render(ICON),
        };
        div()
            .id(ElementId::Name(format!("file-row:{}", row.path).into()))
            .relative()
            .flex()
            .items_center()
            .gap(ITEM_GAP)
            .h(ROW_HEIGHT)
            .px(ITEM_PADDING)
            .rounded(ROW_RADIUS)
            .cursor_pointer()
            .map(|this| {
                if selected {
                    this.bg(palette.row_selected)
                } else {
                    this.hover(|style| style.bg(palette.row_hover))
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.activate(path.clone(), window, cx);
            }))
            // Indent guides, one per enclosing row.
            .children(row.ancestors.iter().enumerate().map(|(level, ancestor)| {
                let emphasized = focus_parent == Some(ancestor.as_str());
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(ITEM_PADDING + GUIDE_OFFSET + LEVEL_INDENT * level as f32)
                    .w(px(1.))
                    .map(|guide| {
                        if emphasized {
                            guide.bg(palette.guide_focus)
                        } else {
                            guide.group_hover(TREE_GROUP, |style| style.bg(palette.guide_hover))
                        }
                    })
            }))
            .when(row.depth > 0, |this| {
                this.child(
                    div()
                        .flex_none()
                        .w(LEVEL_INDENT * row.depth as f32 - ITEM_GAP),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(ICON)
                    .child(icon),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .when(row.search_match, |name| {
                        name.font_weight(FontWeight::SEMIBOLD)
                    })
                    .child(row.name.clone()),
            )
            .when(ringed, |this| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded(ROW_RADIUS)
                        .border_1()
                        .border_color(palette.focus_ring),
                )
            })
            .into_any_element()
    }

    fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let palette = FilesPalette::current(cx);
        let rows = self.tree.rows().to_vec();
        // Pierre emphasizes the guides of the keyboard row's parent.
        let focus_parent = self
            .focused
            .as_ref()
            .and_then(|path| rows.iter().find(|row| &row.path == path))
            .and_then(|row| row.ancestors.last().cloned());
        let searching = self
            .tree
            .search_query()
            .is_some_and(|query| !query.is_empty());
        let ring = self.focus.is_focused(window) || searching;
        rows.get(range.clone())
            .unwrap_or_default()
            .iter()
            .map(|row| self.render_row(row, &palette, focus_parent.as_deref(), ring, cx))
            .collect()
    }
}

/// `toLocaleString("en-US")` of a count.
fn group_thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

impl Render for FileBrowser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let palette = FilesPalette::current(cx);
        let root = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .size_full()
            .bg(colors.background)
            .font_family(SANS)
            .child(self.render_header(cx));
        if let (Some(error), None) = (&self.error, &self.entries) {
            return root.child(
                div()
                    .p(px(16.))
                    .text_size(px(12.))
                    .line_height(px(19.5))
                    .text_color(colors.destructive)
                    .child(error.clone()),
            );
        }
        let count = self.tree.rows().len();
        root.child(
            div()
                .id("file-tree")
                .key_context("FileTree")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(Self::on_key_down))
                .group(TREE_GROUP)
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .text_size(TREE_TEXT)
                .line_height(ROW_HEIGHT)
                .text_color(colors.foreground)
                .child(self.render_search(&palette, window, cx))
                .child(
                    uniform_list(
                        "file-tree-rows",
                        count,
                        cx.processor(|this, range, window, cx| this.render_rows(range, window, cx)),
                    )
                    .track_scroll(&self.scroll)
                    .flex_1()
                    .px(TREE_INSET),
                ),
        )
    }
}
