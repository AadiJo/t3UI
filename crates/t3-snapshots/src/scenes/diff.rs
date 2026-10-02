//! Diff viewer and changed-files tree scenes. Panel widths match the web reference captures:
//! 540px for the stacked view (the right panel's default width) and 900px for the split view.
//! Fixtures live in `crates/t3-diff/fixtures`; the long patch is generated with the same formula
//! as the reference harness.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use t3_diff::{
    ChangedFilesTree, DiffView,
    rows::DiffStyle,
    tree::{ChangedFile, DiffStat, format_count, summarize},
};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

/// Every diff and changed-files tree scene.
pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("diff-multi-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(cx, MULTI, &[STACKED, SPLIT])
        }),
        Scene::new("diff-multi-light", ThemeMode::Light, |_, cx| {
            diff_scene(cx, MULTI, &[STACKED, SPLIT])
        }),
        // Wrapped lines, and the stacked view scrolled 160px to pin a sticky header.
        Scene::new("diff-wrap-sticky-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(
                cx,
                MULTI,
                &[
                    Panel {
                        wrap: true,
                        ..STACKED
                    },
                    Panel {
                        scroll: 160.,
                        ..STACKED
                    },
                ],
            )
        }),
        // Collapsed files keep only their header, with a right chevron.
        Scene::new("diff-collapsed-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(
                cx,
                MULTI,
                &[Panel {
                    collapsed: &[
                        "apps/server/src/legacy/checkpointPaths.ts",
                        "apps/web/src/components/ChatView.tsx",
                    ],
                    ..STACKED
                }],
            )
        }),
        Scene::new("diff-rename-binary-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(cx, RENAME, &[STACKED, SPLIT])
        }),
        Scene::new("diff-rename-binary-light", ThemeMode::Light, |_, cx| {
            diff_scene(cx, RENAME, &[STACKED, SPLIT])
        }),
        Scene::new("diff-long-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(cx, &long_patch(), &[STACKED, SPLIT])
        }),
        // The long file wrapped, and scrolled to its end to show virtualization.
        Scene::new("diff-long-wrap-dark", ThemeMode::Dark, |_, cx| {
            diff_scene(
                cx,
                &long_patch(),
                &[
                    Panel {
                        wrap: true,
                        ..STACKED
                    },
                    Panel {
                        scroll: 100_000.,
                        ..STACKED
                    },
                ],
            )
        }),
        Scene::new("changed-files-tree-dark", ThemeMode::Dark, |_, cx| {
            tree_scene(cx)
        }),
        Scene::new("changed-files-tree-light", ThemeMode::Light, |_, cx| {
            tree_scene(cx)
        }),
    ]
}

const MULTI: &str = include_str!("../../../t3-diff/fixtures/multi-file.patch");
const RENAME: &str = include_str!("../../../t3-diff/fixtures/rename-binary.patch");

/// A new 10,000-line file; every 25th line is long enough to overflow the panel.
fn long_patch() -> String {
    let lines = 10_000;
    let mut patch = format!(
        "diff --git a/src/generated/fixtures.ts b/src/generated/fixtures.ts\nnew file mode 100644\nindex 0000000..1234567\n--- /dev/null\n+++ b/src/generated/fixtures.ts\n@@ -0,0 +1,{lines} @@\n"
    );
    let filler =
        "long line to exercise horizontal overflow and wrapping in the diff viewer ".repeat(4);
    for i in 1..=lines {
        let id = format!("{i:05}");
        if i % 25 == 0 {
            patch.push_str(&format!("+// {id}: {}\n", filler.trim()));
        } else {
            patch.push_str(&format!(
                "+export const fixture{id} = createFixture({{ id: {i}, label: \"fixture-{i}\", weight: {} }});\n",
                (i * 37) % 101
            ));
        }
    }
    patch
}

/// One diff panel in a scene.
struct Panel {
    width: f32,
    style: DiffStyle,
    wrap: bool,
    /// Pixels to scroll after the first frame has measured the rows.
    scroll: f32,
    /// Files collapsed to their header.
    collapsed: &'static [&'static str],
}

const STACKED: Panel = Panel {
    width: 540.,
    style: DiffStyle::Unified,
    wrap: false,
    scroll: 0.,
    collapsed: &[],
};
const SPLIT: Panel = Panel {
    width: 900.,
    style: DiffStyle::Split,
    wrap: false,
    scroll: 0.,
    collapsed: &[],
};

/// Diff panels side by side on the app background.
struct DiffScene {
    panels: Vec<(Entity<DiffView>, f32, f32)>,
    frame: usize,
}

impl Render for DiffScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Scroll once rows have been measured by the first frame.
        if self.frame == 1 {
            for (view, _, scroll) in &self.panels {
                if *scroll > 0. {
                    view.update(cx, |view, cx| view.scroll_by(px(*scroll), cx));
                }
            }
        }
        self.frame += 1;
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(cx.colors().background)
            .children(self.panels.iter().map(|(view, width, _)| {
                div().flex_none().w(px(*width)).h_full().child(view.clone())
            }))
    }
}

fn diff_scene(cx: &mut App, patch: &str, panels: &[Panel]) -> AnyView {
    let panels = panels
        .iter()
        .map(|panel| {
            let view = cx.new(|cx| {
                let mut view = DiffView::new(cx);
                view.set_style(panel.style, cx);
                view.set_wrap(panel.wrap, cx);
                view.set_patch(patch, cx);
                for path in panel.collapsed {
                    view.set_collapsed(path, true, cx);
                }
                view
            });
            (view, panel.width, panel.scroll)
        })
        .collect();
    cx.new(|_| DiffScene { panels, frame: 0 }).into()
}

/// The turn's changed files from the reference harness.
fn tree_files() -> Vec<ChangedFile> {
    [
        ("apps/web/src/components/ChatView.tsx", 9, 4),
        ("apps/web/src/components/DiffPanel.tsx", 120, 38),
        ("apps/web/src/components/chat/ChangedFilesTree.tsx", 42, 7),
        ("apps/web/src/lib/diffStats.ts", 14, 0),
        ("apps/server/src/legacy/checkpointPaths.ts", 0, 7),
        ("apps/server/src/orchestration/turnDiff.ts", 23, 11),
        ("README.md", 1, 1),
        ("package.json", 2, 1),
        ("crates/t3-diff/src/lib.rs", 1250, 0),
    ]
    .into_iter()
    .map(|(path, additions, deletions)| ChangedFile {
        path: path.into(),
        stat: Some(DiffStat {
            additions,
            deletions,
        }),
    })
    .collect()
}

/// The tree inside the timeline's changed-files card (`AssistantChangedFilesSectionInner`).
/// The card and its buttons are approximated here; they belong to the timeline.
struct TreeScene {
    tree: Entity<ChangedFilesTree>,
    files: Vec<ChangedFile>,
}

impl Render for TreeScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let stat = summarize(&self.files);
        let collapse_label = if self.tree.read(cx).all_expanded_by_default() {
            "Collapse all"
        } else {
            "Expand all"
        };
        let button = |label: &'static str| {
            div()
                .flex()
                .items_center()
                .h(px(24.))
                .px(px(7.))
                .rounded(px(8.))
                .border_1()
                .border_color(colors.input)
                .bg(colors.input.opacity(0.32))
                .text_size(px(12.))
                .text_color(colors.foreground)
                .child(label)
        };
        div()
            .size_full()
            .bg(colors.background)
            .p(px(24.))
            .font_family(t3_ui::tokens::font::SANS)
            .child(
                div()
                    .w(px(640.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(colors.border.opacity(0.8))
                    .bg(colors.card.opacity(0.45))
                    .p(px(10.))
                    .child(
                        div()
                            .mb(px(6.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(4.))
                                    .text_size(px(10.))
                                    .text_color(colors.muted_foreground.opacity(0.65))
                                    .child(format!("CHANGED FILES ({})", self.files.len()))
                                    .child("•")
                                    .child(div().text_color(colors.success).child(format!(
                                        "+{}",
                                        format_count(stat.additions).to_uppercase()
                                    )))
                                    .child(
                                        div()
                                            .text_color(colors.destructive)
                                            .child(format!("-{}", format_count(stat.deletions))),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(button(collapse_label))
                                    .child(button("View diff")),
                            ),
                    )
                    .child(self.tree.clone()),
            )
    }
}

fn tree_scene(cx: &mut App) -> AnyView {
    let files = tree_files();
    let tree = cx.new(|cx| ChangedFilesTree::new(&files, cx));
    cx.new(|_| TreeScene { tree, files }).into()
}
