//! The diff surface (spec 2, `DiffPanel.tsx`): the 40px header with the scope menu, the
//! compare/base-ref picker and the view toggles, then the body states around the
//! `t3_diff::DiffView`.

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, Window, component::input::InputState, div, prelude::FluentBuilder as _, px,
};
use t3_diff::{DiffView, DiffViewEvent, rows::DiffStyle};
use t3_protocol::{
    TurnId,
    methods::{GetFullThreadDiff, GetTurnDiff, ReviewGetDiffPreview, VcsListRefs},
    orchestration::{GetFullThreadDiffInput, GetTurnDiffInput, OrchestrationCheckpointSummary},
    vcs::{RefKind, ReviewDiffPreviewInput, VcsListRefsInput, VcsRef},
};
use t3_ui::{
    ActiveColors as _, Align, Button, ButtonSize, ButtonVariant, DropdownMenu, Icon, IconName,
    Input, InputSize, MenuCheckboxItem, MenuGroupLabel, Popover, PopoverPopup, Skeleton,
    tokens::font,
};

use super::{context::PanelContext, thread_detail::ThreadDetail};
use crate::state::AppState;

/// Which changes the panel shows (`DiffPanelSelection`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffScope {
    /// Committed and uncommitted changes against a base ref (`None` = automatic).
    Branch,
    /// Uncommitted changes only.
    WorkingTree,
    /// One turn's checkpoint diff.
    Turn(TurnId),
}

/// What the body shows while and after loading.
enum Load {
    Loading,
    Loaded {
        /// Whitespace-only patch: "No net changes in this selection."
        empty: bool,
        truncated: bool,
        head_ref: Option<String>,
        base_ref: Option<String>,
        /// The cwd the preview was computed in (feeds the ref picker).
        cwd: Option<String>,
    },
    Failed(String),
}

/// The diff tab of a thread's right panel.
pub struct DiffPanel {
    context: PanelContext,
    detail: Entity<ThreadDetail>,
    view: Entity<DiffView>,
    scope: DiffScope,
    /// Remembered across scope switches (`branchBaseRefByThreadKey`).
    base_ref: Option<String>,
    ignore_whitespace: bool,
    load: Load,
    refs: Vec<VcsRef>,
    ref_query: Entity<InputState>,
    _load_task: Option<Task<()>>,
    _refs_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl DiffPanel {
    pub fn new(
        context: PanelContext,
        detail: Entity<ThreadDetail>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = context.app_state.read(cx).settings().clone();
        let view = cx.new(|cx| {
            let mut view = DiffView::new(cx);
            view.set_wrap(settings.word_wrap, cx);
            view
        });
        let ref_query = cx.new(|cx| InputState::new(window, cx).placeholder("Search refs..."));
        let subscriptions = vec![
            cx.subscribe(&view, |this, _, event, cx| {
                // Review comment events join this once DiffView emits them.
                #[allow(irrefutable_let_patterns)]
                if let DiffViewEvent::OpenFile(path) = event {
                    let thread = this.context.thread.clone();
                    this.context
                        .panels(cx)
                        .update(cx, |panels, cx| panels.open_file(&thread, path, None, cx));
                }
            }),
            cx.observe(&detail, |this, _, cx| {
                // A turn scope needs the checkpoints; reload once they arrive or change.
                if matches!(this.scope, DiffScope::Turn(_)) {
                    this.reload(cx);
                }
                cx.notify();
            }),
            cx.observe_global::<t3_ui::Theme>(|this, cx| {
                this.view.update(cx, |view, cx| view.sync_theme(cx));
            }),
        ];
        let mut this = Self {
            context,
            detail,
            view,
            scope: DiffScope::Branch,
            base_ref: None,
            ignore_whitespace: settings.diff_ignore_whitespace,
            load: Load::Loading,
            refs: Vec::new(),
            ref_query,
            _load_task: None,
            _refs_task: None,
            _subscriptions: subscriptions,
        };
        this.reload(cx);
        this
    }

    /// Shows a turn and scrolls `file_path` to the top (`selectTurn`, timeline "View diff").
    pub fn select_turn(
        &mut self,
        turn_id: TurnId,
        file_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.scope = DiffScope::Turn(turn_id);
        self.reload(cx);
        if let Some(path) = file_path {
            self.view.update(cx, |view, cx| view.reveal_file(&path, cx));
        }
    }

    /// Shows the newest turn (the scope menu's "Latest turn").
    pub fn select_latest_turn(&mut self, cx: &mut Context<Self>) {
        if let Some(latest) = self.turns(cx).first() {
            let turn_id = latest.turn_id.clone();
            self.select_turn(turn_id, None, cx);
        }
    }

    fn select_scope(&mut self, scope: DiffScope, cx: &mut Context<Self>) {
        if self.scope != scope {
            self.scope = scope;
            self.reload(cx);
        }
    }

    /// Checkpoints newest first (`orderedTurnDiffSummaries`).
    fn turns(&self, cx: &App) -> Vec<std::sync::Arc<OrchestrationCheckpointSummary>> {
        let mut turns = self
            .detail
            .read(cx)
            .thread()
            .map(|thread| thread.checkpoints.clone())
            .unwrap_or_default();
        turns.sort_by(|a, b| {
            b.checkpoint_turn_count
                .cmp(&a.checkpoint_turn_count)
                .then_with(|| b.completed_at.cmp(&a.completed_at))
        });
        turns
    }

    fn scope_label(&self, cx: &App) -> SharedString {
        match &self.scope {
            DiffScope::WorkingTree => "Working tree".into(),
            DiffScope::Branch => "Branch changes".into(),
            DiffScope::Turn(turn_id) => {
                let turns = self.turns(cx);
                match turns.iter().position(|turn| &turn.turn_id == turn_id) {
                    Some(0) => "Latest turn".into(),
                    Some(ix) => format!("Turn {}", turns[ix].checkpoint_turn_count).into(),
                    None => "Turn ?".into(),
                }
            }
        }
    }

    /// Fetches the patch for the current scope.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.load = Load::Loading;
        let ignore_whitespace = Some(self.ignore_whitespace);
        let thread_id = self.context.thread.thread_id.clone();
        let task: Task<anyhow::Result<(String, Load)>> = match &self.scope {
            DiffScope::Turn(turn_id) => {
                let count = self
                    .turns(cx)
                    .iter()
                    .find(|turn| &turn.turn_id == turn_id)
                    .map(|turn| turn.checkpoint_turn_count);
                let Some(count) = count else {
                    // Checkpoints have not arrived yet; the detail observer reloads.
                    cx.notify();
                    return;
                };
                let request = if count > 1 {
                    self.context.request::<GetTurnDiff>(
                        GetTurnDiffInput {
                            thread_id,
                            from_turn_count: count - 1,
                            to_turn_count: count,
                            ignore_whitespace,
                        },
                        cx,
                    )
                } else {
                    self.context.request::<GetFullThreadDiff>(
                        GetFullThreadDiffInput {
                            thread_id,
                            to_turn_count: count,
                            ignore_whitespace,
                        },
                        cx,
                    )
                };
                cx.background_spawn(async move {
                    let diff = request.await?.diff;
                    let loaded = Load::Loaded {
                        empty: diff.trim().is_empty(),
                        truncated: false,
                        head_ref: None,
                        base_ref: None,
                        cwd: None,
                    };
                    Ok((diff, loaded))
                })
            }
            DiffScope::Branch | DiffScope::WorkingTree => {
                let Some(cwd) = self.context.cwd(cx) else {
                    self.load = Load::Failed("This thread has no workspace.".into());
                    cx.notify();
                    return;
                };
                let working_tree = self.scope == DiffScope::WorkingTree;
                let input = ReviewDiffPreviewInput {
                    cwd: cwd.clone(),
                    base_ref: self.base_ref.clone(),
                    ignore_whitespace,
                    file: None,
                };
                let primary = self
                    .context
                    .request::<ReviewGetDiffPreview>(input.clone(), cx);
                // Retry once at the server's cwd when the thread's is outside the workspace root.
                let server_cwd = self
                    .context
                    .environment(cx)
                    .and_then(|environment| {
                        environment
                            .read(cx)
                            .config()
                            .map(|config| config.cwd.clone())
                    })
                    .filter(|server_cwd| *server_cwd != cwd);
                let context = self.context.clone();
                let fallback = server_cwd.map(|server_cwd| {
                    context.request::<ReviewGetDiffPreview>(
                        ReviewDiffPreviewInput {
                            cwd: server_cwd,
                            ..input
                        },
                        cx,
                    )
                });
                cx.background_spawn(async move {
                    let preview = match primary.await {
                        Err(error)
                            if error.to_string().contains("configured workspace root")
                                && fallback.is_some() =>
                        {
                            fallback.expect("checked above").await?
                        }
                        other => other?,
                    };
                    let kind = if working_tree {
                        "working-tree"
                    } else {
                        "branch-range"
                    };
                    let source = preview
                        .sources
                        .into_iter()
                        .find(|source| source.kind == kind);
                    let Some(source) = source else {
                        return Ok((
                            String::new(),
                            Load::Loaded {
                                empty: false,
                                truncated: false,
                                head_ref: None,
                                base_ref: None,
                                cwd: Some(preview.cwd),
                            },
                        ));
                    };
                    let loaded = Load::Loaded {
                        empty: source.diff.trim().is_empty(),
                        truncated: source.truncated,
                        head_ref: source.head_ref,
                        base_ref: source.base_ref,
                        cwd: Some(preview.cwd),
                    };
                    Ok((source.diff, loaded))
                })
            }
        };
        self._load_task = Some(cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok((patch, load)) => {
                        this.view.update(cx, |view, cx| view.set_patch(&patch, cx));
                        let wants_refs = matches!(
                            &load,
                            Load::Loaded {
                                base_ref: Some(_),
                                cwd: Some(_),
                                ..
                            }
                        );
                        this.load = load;
                        if wants_refs && this.scope == DiffScope::Branch {
                            this.load_refs(cx);
                        }
                    }
                    Err(error) => this.load = Load::Failed(error.to_string()),
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Lists local and remote refs for the base-ref picker (`vcs.listRefs`, two calls).
    fn load_refs(&mut self, cx: &mut Context<Self>) {
        let Load::Loaded {
            cwd: Some(cwd),
            head_ref,
            ..
        } = &self.load
        else {
            return;
        };
        let query = self.ref_query.read(cx).value().trim().to_owned();
        let request = |ref_kind| VcsListRefsInput {
            cwd: cwd.clone(),
            query: (!query.is_empty()).then(|| query.clone()),
            include_matching_remote_refs: Some(true),
            ref_kind: Some(ref_kind),
            limit: Some(100),
            ..VcsListRefsInput::default()
        };
        let local = self
            .context
            .request::<VcsListRefs>(request(RefKind::Local), cx);
        let remote = self
            .context
            .request::<VcsListRefs>(request(RefKind::Remote), cx);
        let head_ref = head_ref.clone();
        self._refs_task = Some(cx.spawn(async move |this, cx| {
            let mut refs: Vec<VcsRef> = local
                .await
                .map(|result| result.refs)
                .unwrap_or_default()
                .into_iter()
                .filter(|vcs_ref| Some(&vcs_ref.name) != head_ref.as_ref())
                .collect();
            refs.extend(remote.await.map(|result| result.refs).unwrap_or_default());
            this.update(cx, |this, cx| {
                this.refs = refs;
                cx.notify();
            })
            .ok();
        }));
    }

    fn set_ignore_whitespace(&mut self, ignore: bool, cx: &mut Context<Self>) {
        if self.ignore_whitespace != ignore {
            self.ignore_whitespace = ignore;
            self.reload(cx);
        }
    }

    fn render_scope_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let label = self.scope_label(cx);
        let this = cx.entity();
        let turns = self.turns(cx);
        let scope = self.scope.clone();
        DropdownMenu::new("diff-scope")
            .align(Align::Start)
            .min_width(px(240.))
            .trigger(move |open| {
                div()
                    .id("diff-scope-trigger")
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .h(px(24.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .bg(colors.muted.opacity(0.7))
                    .when(open, |this| this.bg(colors.muted))
                    .hover(|this| this.bg(colors.muted))
                    .text_size(px(12.))
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(div().truncate().child(label))
                    .child(
                        Icon::new(IconName::ChevronDown)
                            .size(px(14.))
                            .color(colors.muted_foreground),
                    )
                    .into_any_element()
            })
            .items(move |_, _| {
                // The fork marks the current choice with a trailing check.
                let item = |id: &'static str,
                            label: SharedString,
                            selected: bool,
                            scope: Option<DiffScope>| {
                    let this = this.clone();
                    MenuCheckboxItem::new(id, label)
                        .checked(selected)
                        .on_change(move |_, _, cx| {
                            this.update(cx, |panel, cx| match scope.clone() {
                                Some(scope) => panel.select_scope(scope, cx),
                                None => panel.select_latest_turn(cx),
                            })
                        })
                        .into_any_element()
                };
                let latest = turns.first().map(|turn| turn.turn_id.clone());
                let mut items = vec![
                    item(
                        "working-tree",
                        "Working tree".into(),
                        scope == DiffScope::WorkingTree,
                        Some(DiffScope::WorkingTree),
                    ),
                    item(
                        "branch",
                        "Branch changes".into(),
                        scope == DiffScope::Branch,
                        Some(DiffScope::Branch),
                    ),
                    item(
                        "latest-turn",
                        "Latest turn".into(),
                        latest
                            .as_ref()
                            .is_some_and(|latest| scope == DiffScope::Turn(latest.clone())),
                        None,
                    ),
                ];
                if !turns.is_empty() {
                    items.push(MenuGroupLabel::new("Turn").into_any_element());
                    for turn in &turns {
                        let this = this.clone();
                        let turn_id = turn.turn_id.clone();
                        let selected = scope == DiffScope::Turn(turn_id.clone());
                        let row = MenuCheckboxItem::new(
                            SharedString::from(format!("turn-{}", turn.turn_id)),
                            format!("Turn {}", turn.checkpoint_turn_count),
                        )
                        .checked(selected)
                        .on_change(move |_, _, cx| {
                            let turn_id = turn_id.clone();
                            this.update(cx, |panel, cx| panel.select_turn(turn_id, None, cx));
                        });
                        items.push(row.into_any_element());
                    }
                }
                items
            })
    }

    fn render_compare(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.scope != DiffScope::Branch {
            return None;
        }
        let Load::Loaded {
            head_ref,
            base_ref: Some(base_ref),
            ..
        } = &self.load
        else {
            return None;
        };
        let colors = cx.colors();
        let head = head_ref.clone().unwrap_or_else(|| "HEAD".into());
        let base: SharedString = base_ref.clone().into();
        let this = cx.entity();
        let refs = self.refs.clone();
        let ref_query = self.ref_query.clone();
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_w_0()
                .overflow_hidden()
                .text_size(px(12.))
                .text_color(colors.muted_foreground)
                .child(div().max_w(px(192.)).truncate().child(head))
                .child(
                    Icon::new(IconName::ArrowRight)
                        .size(px(14.))
                        .color(colors.muted_foreground.opacity(0.7)),
                )
                .child(
                    Popover::new("diff-base-ref")
                        .align(Align::Start)
                        .trigger(move |_| {
                            div()
                                .id("diff-base-ref-trigger")
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .max_w(px(192.))
                                .px(px(6.))
                                .py(px(4.))
                                .rounded(px(8.))
                                .hover(|this| this.bg(colors.muted).text_color(colors.foreground))
                                .child(div().truncate().child(base))
                                .child(
                                    Icon::new(IconName::ChevronDown)
                                        .size(px(14.))
                                        .color(colors.muted_foreground.opacity(0.7)),
                                )
                                .into_any_element()
                        })
                        .content(move |_, cx| {
                            let colors = cx.colors();
                            let pick =
                                |id: SharedString, label: SharedString, value: Option<String>| {
                                    let this = this.clone();
                                    div()
                                        .id(id)
                                        .flex()
                                        .items_center()
                                        .h(px(32.))
                                        .px(px(12.))
                                        .rounded(px(6.))
                                        .text_size(px(14.))
                                        .hover(|row| row.bg(colors.accent))
                                        .on_click(move |_, _, cx| {
                                            let value = value.clone();
                                            this.update(cx, |panel, cx| {
                                                panel.base_ref = value;
                                                panel.reload(cx);
                                            })
                                        })
                                        .child(div().truncate().child(label))
                                };
                            PopoverPopup::new()
                                .w(px(288.))
                                .child(
                                    div()
                                        .px(px(12.))
                                        .pt(px(10.))
                                        .pb(px(6.))
                                        .border_b_1()
                                        .border_color(colors.border.opacity(0.7))
                                        .child(Input::new(&ref_query).size(InputSize::Sm)),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .justify_between()
                                        .px(px(12.))
                                        .pt(px(8.))
                                        .pb(px(6.))
                                        .border_b_1()
                                        .border_color(colors.border.opacity(0.7))
                                        .text_size(px(10.))
                                        .text_color(colors.muted_foreground)
                                        .child("BRANCH")
                                        .child("REMOTE"),
                                )
                                .child(
                                    div()
                                        .id("diff-ref-list")
                                        .max_h(px(256.))
                                        .overflow_y_scroll()
                                        .p(px(4.))
                                        .child(pick(
                                            "ref-automatic".into(),
                                            "Automatic".into(),
                                            None,
                                        ))
                                        .children(refs.iter().map(|vcs_ref| {
                                            pick(
                                                format!("ref-{}", vcs_ref.name).into(),
                                                vcs_ref.name.clone().into(),
                                                Some(vcs_ref.name.clone()),
                                            )
                                        })),
                                )
                        }),
                )
                .into_any_element(),
        )
    }

    fn render_toggles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = self.view.read(cx);
        let (split, wrap) = (view.style() == DiffStyle::Split, view.wrap());
        let ignore = self.ignore_whitespace;
        let this = cx.entity();
        let toggle = |id: &'static str, icon: IconName, pressed: bool, tooltip: &'static str| {
            Button::new(id)
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::IconXs)
                .icon(icon)
                .pressed(pressed)
                .tooltip(tooltip)
        };
        let set_style = |style: DiffStyle| {
            let this = this.clone();
            move |_: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |panel, cx| {
                    panel.view.update(cx, |view, cx| view.set_style(style, cx));
                    cx.notify();
                })
            }
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .child(
                        toggle("diff-stacked", IconName::Rows3, !split, "Stacked diff view")
                            .on_click(set_style(DiffStyle::Unified)),
                    )
                    .child(
                        toggle("diff-split", IconName::Columns2, split, "Split diff view")
                            .on_click(set_style(DiffStyle::Split)),
                    ),
            )
            .child(
                toggle(
                    "diff-wrap",
                    IconName::TextWrap,
                    wrap,
                    if wrap {
                        "Disable line wrapping"
                    } else {
                        "Enable line wrapping"
                    },
                )
                .on_click({
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |panel, cx| {
                            panel.view.update(cx, |view, cx| view.set_wrap(!wrap, cx));
                            cx.notify();
                        })
                    }
                }),
            )
            .child(
                toggle(
                    "diff-whitespace",
                    IconName::Pilcrow,
                    ignore,
                    if ignore {
                        "Show whitespace changes"
                    } else {
                        "Hide whitespace changes"
                    },
                )
                .on_click(move |_, _, cx| {
                    this.update(cx, |panel, cx| panel.set_ignore_whitespace(!ignore, cx))
                }),
            )
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let placeholder = |text: &'static str| {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px(px(20.))
                .text_size(px(12.))
                .text_color(colors.muted_foreground.opacity(0.7))
                .child(text)
                .into_any_element()
        };
        if matches!(self.scope, DiffScope::Turn(_)) && self.turns(cx).is_empty() {
            return placeholder("No completed turns yet.");
        }
        match &self.load {
            Load::Loading => loading_skeleton(
                match self.scope {
                    DiffScope::Turn(_) => "Loading checkpoint diff...",
                    DiffScope::WorkingTree => "Loading working tree diff...",
                    DiffScope::Branch => "Loading branch diff...",
                },
                cx,
            ),
            Load::Failed(error) => div()
                .flex_1()
                .px(px(12.))
                .pt(px(8.))
                .text_size(px(11.))
                .text_color(gpui_kit::rgba(0xEF4444CC))
                .child(error.clone())
                .into_any_element(),
            Load::Loaded {
                empty, truncated, ..
            } => {
                if self.view.read(cx).files().is_empty() && !*empty {
                    return placeholder("No patch available for this selection.");
                }
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(*truncated, |body| {
                        body.child(
                            div()
                                .flex_none()
                                .px(px(12.))
                                .py(px(6.))
                                .border_b_1()
                                .border_color(colors.border.opacity(0.7))
                                .bg(colors.muted.opacity(0.4))
                                .text_size(px(11.))
                                .text_color(colors.muted_foreground)
                                .child("This diff was truncated because it exceeded the preview limit. The changes shown are incomplete."),
                        )
                    })
                    .child(div().flex_1().min_h_0().child(self.view.clone()))
                    .into_any_element()
            }
        }
    }
}

/// The skeleton card shown while a patch loads (`DiffPanelLoadingState`).
fn loading_skeleton(label: &'static str, cx: &App) -> AnyElement {
    let colors = cx.colors();
    let still = !AppState::global(cx).read(cx).clock_is_live();
    let pill = |id: &'static str, width: gpui_kit::Length, height: f32| {
        Skeleton::new(id)
            .when(still, Skeleton::still)
            .h(px(height))
            .w(width)
            .rounded_full()
    };
    div()
        .id(label)
        .flex_1()
        .flex()
        .flex_col()
        .p(px(8.))
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(8.))
                .border_1()
                .border_color(colors.border.opacity(0.6))
                .bg(colors.card.opacity(0.25))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(8.))
                        .border_b_1()
                        .border_color(colors.border.opacity(0.5))
                        .child(pill("skeleton-title", px(128.).into(), 16.))
                        .child(div().flex_1())
                        .child(pill("skeleton-meta", px(80.).into(), 16.)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .px(px(12.))
                        .py(px(16.))
                        .children(
                            [1.0, 1.0, 0.8333, 0.9167, 0.75]
                                .into_iter()
                                .enumerate()
                                .map(|(ix, fraction)| {
                                    pill(
                                        [
                                            "skeleton-1",
                                            "skeleton-2",
                                            "skeleton-3",
                                            "skeleton-4",
                                            "skeleton-5",
                                        ][ix],
                                        gpui_kit::relative(fraction).into(),
                                        12.,
                                    )
                                }),
                        ),
                ),
        )
        .into_any_element()
}

impl Render for DiffPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let compare = self.render_compare(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.background)
            .font_family(font::SANS)
            .child(
                div()
                    .flex_none()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .px(px(16.))
                    .border_b_1()
                    .border_color(colors.border.opacity(0.6))
                    .bg(colors.background)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(12.))
                            .child(self.render_scope_menu(cx))
                            .children(compare),
                    )
                    .child(self.render_toggles(cx)),
            )
            .child(self.render_body(cx))
    }
}
