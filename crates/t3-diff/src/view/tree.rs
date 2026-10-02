//! [`ChangedFilesTree`]: the web client's `ChangedFilesTree` (`components/chat/ChangedFilesTree.tsx`)
//! as a reusable GPUI view. Directories collapse into compact paths, files show their type icon
//! and `+N -N` stats, and clicking a file emits [`ChangedFilesTreeEvent::OpenFile`].

use std::collections::HashMap;

use gpui_kit::{
    AnyElement, App, Context, EventEmitter, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px, svg,
};

use super::{
    icons,
    style::{gpui, mono_family, theme_tokens},
};
use crate::{
    palette::{Appearance, TreePalette},
    tree::{ChangedFile, DiffStat, TreeNode, build_tree, expanded_by_default, format_count},
};

/// Row indent: `8px + depth * 14px`.
const INDENT_BASE: Pixels = px(8.);
const INDENT_STEP: Pixels = px(14.);
const ICON_SIZE: Pixels = px(14.);
/// `text-[11px]` names on the inherited 1.5 line height.
const NAME_SIZE: Pixels = px(11.);
const NAME_LINE_HEIGHT: Pixels = px(16.5);
/// `text-[10px]` stats.
const STAT_SIZE: Pixels = px(10.);

/// Events for the timeline card or panel embedding the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangedFilesTreeEvent {
    /// A file row was clicked (`onOpenTurnDiff(turnId, path)`).
    OpenFile(SharedString),
}

/// The changed-files tree. Owns its directory expansion state like the React component.
pub struct ChangedFilesTree {
    nodes: Vec<TreeNode>,
    has_directories: bool,
    /// Expand/collapse-all override from the card; `None` uses per-directory defaults.
    all_expanded: Option<bool>,
    /// Directories the user toggled, by path. Reset when the files or the override change.
    toggled: HashMap<String, bool>,
    selected: Option<SharedString>,
    palette: TreePalette,
    mono: SharedString,
}

impl EventEmitter<ChangedFilesTreeEvent> for ChangedFilesTree {}

impl ChangedFilesTree {
    /// A tree of `files` in the active `t3-ui` theme.
    pub fn new(files: &[ChangedFile], cx: &App) -> Self {
        let (tokens, appearance) = theme_tokens(cx);
        let nodes = build_tree(files);
        Self {
            has_directories: nodes
                .iter()
                .any(|node| matches!(node, TreeNode::Directory { .. })),
            nodes,
            all_expanded: None,
            toggled: HashMap::new(),
            selected: None,
            palette: TreePalette::new(&tokens, appearance),
            mono: mono_family(cx),
        }
    }

    /// Replaces the files and forgets toggled directories.
    pub fn set_files(&mut self, files: &[ChangedFile], cx: &mut Context<Self>) {
        let nodes = build_tree(files);
        self.has_directories = nodes
            .iter()
            .any(|node| matches!(node, TreeNode::Directory { .. }));
        self.nodes = nodes;
        self.toggled.clear();
        cx.notify();
    }

    /// The card's "Expand all" / "Collapse all" override. Changing it forgets toggles, as the
    /// web component keys its state on the override.
    pub fn set_all_expanded(&mut self, all_expanded: Option<bool>, cx: &mut Context<Self>) {
        if self.all_expanded != all_expanded {
            self.all_expanded = all_expanded;
            self.toggled.clear();
            cx.notify();
        }
    }

    /// Whether every directory starts expanded; the card shows "Collapse all" when true.
    pub fn all_expanded_by_default(&self) -> bool {
        crate::tree::all_expanded_by_default(&self.nodes)
    }

    /// Highlights a file row (for example the file shown in the diff panel).
    pub fn set_selected(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.selected = path;
        cx.notify();
    }

    /// Re-reads the `t3-ui` theme after the appearance changes.
    pub fn sync_theme(&mut self, cx: &mut Context<Self>) {
        let (tokens, appearance) = theme_tokens(cx);
        self.palette = TreePalette::new(&tokens, appearance);
        self.mono = mono_family(cx);
        cx.notify();
    }

    fn is_expanded(&self, path: &str, children: &[TreeNode]) -> bool {
        self.toggled.get(path).copied().unwrap_or_else(|| {
            self.all_expanded
                .unwrap_or_else(|| expanded_by_default(children))
        })
    }

    fn toggle(&mut self, path: String, children_expanded: bool, cx: &mut Context<Self>) {
        self.toggled.insert(path, !children_expanded);
        cx.notify();
    }

    fn render_level(&self, nodes: &[TreeNode], depth: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .children(nodes.iter().map(|node| self.render_node(node, depth, cx)))
            .into_any_element()
    }

    fn render_node(&self, node: &TreeNode, depth: usize, cx: &mut Context<Self>) -> AnyElement {
        let palette = self.palette;
        let mono = self.mono.clone();
        let row = |id: SharedString| {
            div()
                .id(id)
                .group("changed-file-row")
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .w_full()
                .rounded(px(14.))
                .py(px(4.))
                .pr(px(12.))
                .pl(INDENT_BASE + INDENT_STEP * depth as f32)
                .cursor_pointer()
                .hover(|row| row.bg(gpui(palette.row_hover)))
        };
        let name = |text: &str, color| {
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(mono.clone())
                .text_size(NAME_SIZE)
                .line_height(NAME_LINE_HEIGHT)
                .text_color(gpui(color))
                .group_hover("changed-file-row", |name| {
                    name.text_color(gpui(palette.name_hover))
                })
                .child(text.to_owned())
        };
        match node {
            TreeNode::Directory {
                name: label,
                path,
                stat,
                children,
            } => {
                let expanded = self.is_expanded(path, children);
                let toggle_path = path.clone();
                div()
                    .flex()
                    .flex_col()
                    .child(
                        row(format!("dir:{path}").into())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle(toggle_path.clone(), expanded, cx);
                            }))
                            .child(
                                svg()
                                    .path(if expanded {
                                        icons::CHEVRON_DOWN
                                    } else {
                                        icons::CHEVRON_RIGHT
                                    })
                                    .flex_none()
                                    .size(ICON_SIZE)
                                    .text_color(gpui(palette.chevron))
                                    .group_hover("changed-file-row", |icon| {
                                        icon.text_color(gpui(palette.chevron_hover))
                                    }),
                            )
                            .child(
                                svg()
                                    .path(if expanded {
                                        icons::FOLDER
                                    } else {
                                        icons::FOLDER_CLOSED
                                    })
                                    .flex_none()
                                    .size(ICON_SIZE)
                                    .text_color(gpui(palette.folder)),
                            )
                            .child(name(label, palette.directory_name))
                            .when(!stat.is_zero(), |row| {
                                row.child(stat_label(*stat, &palette, mono.clone()))
                            }),
                    )
                    .when(expanded, |wrapper| {
                        wrapper.child(self.render_level(children, depth + 1, cx))
                    })
                    .into_any_element()
            }
            TreeNode::File {
                name: label,
                path,
                stat,
            } => {
                let open_path: SharedString = path.clone().into();
                let selected = self.selected.as_deref() == Some(path.as_str());
                row(format!("file:{path}").into())
                    .when(selected, |row| row.bg(gpui(palette.row_hover)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ChangedFilesTreeEvent::OpenFile(open_path.clone()));
                    }))
                    .when(self.has_directories || depth > 0, |row| {
                        row.child(div().flex_none().size(ICON_SIZE))
                    })
                    .child(file_icon(path, &palette))
                    .child(name(label, palette.file_name))
                    .when_some(*stat, |row, stat| {
                        row.child(stat_label(stat, &palette, mono.clone()))
                    })
                    .into_any_element()
            }
        }
    }
}

impl Render for ChangedFilesTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let nodes = std::mem::take(&mut self.nodes);
        let level = self.render_level(&nodes, 0, cx);
        self.nodes = nodes;
        level
    }
}

/// `DiffStatLabel` in its aligned layout: two right-aligned `4ch` columns, 8px apart. Like
/// CSS `text-align: right`, a label longer than its column starts at the column's left edge
/// and overflows to the right.
fn stat_label(stat: DiffStat, palette: &TreePalette, mono: SharedString) -> impl IntoElement {
    let column = |text: String, color| {
        let fits = text.chars().count() <= 4;
        div()
            .w(STAT_SIZE * 0.6 * 4.)
            .flex()
            .when(fits, |column| column.justify_end())
            .whitespace_nowrap()
            .text_color(gpui(color))
            .child(text)
    };
    div()
        .ml_auto()
        .flex()
        .flex_none()
        .flex_row()
        .gap(px(8.))
        .font_family(mono)
        .text_size(STAT_SIZE)
        .line_height(NAME_LINE_HEIGHT)
        .child(column(
            format!("+{}", format_count(stat.additions)),
            palette.additions,
        ))
        .child(column(
            format!("-{}", format_count(stat.deletions)),
            palette.deletions,
        ))
}

/// The file-type icon of `path` (`PierreEntryIcon`).
fn file_icon(path: &str, palette: &TreePalette) -> AnyElement {
    t3_ui::file_icon(path, palette.appearance == Appearance::Dark).render(ICON_SIZE)
}
