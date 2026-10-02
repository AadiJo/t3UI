//! The row under the composer (`chat.md` 6.3, `BranchToolbar.tsx`): the workspace mode
//! ("Current checkout" / "New worktree", or a static label once locked) on the left and the
//! branch picker on the right. Shown only for git repos (assumed while status loads).
//!
//! The branch picker lists `vcs.listRefs` (first 50 matches; type to filter), offers
//! "Create new ref", and in New worktree mode the "Start from origin" switch. Picking a ref
//! sets a draft's base branch, or switches the checkout and records it on a server thread.

use std::{collections::HashSet, time::Duration};

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, TaskExt as _, Window,
    base::input::{Input, InputEvent, InputState},
    deferred, div,
    prelude::FluentBuilder as _,
    px,
};
use t3_client::commands;
use t3_logic::composer::draft::DraftEnvMode;
use t3_protocol::{
    commands::ThreadMetaPatch,
    methods::{VcsCreateRef, VcsListRefs, VcsSwitchRef},
    vcs::{VcsCreateRefInput, VcsListRefsInput, VcsRef, VcsSwitchRefInput},
};
use t3_ui::{
    ActiveColors as _, Icon, IconName, MenuCheckboxItem, MenuGroupLabel, MenuPopup, Switch,
    TooltipExt as _, tokens::shadow,
};

use super::{
    drafts::{ComposerTarget, DraftStore},
    style::{self, COLUMN_MAX_WIDTH},
};
use crate::state::{
    Environment,
    vcs::{VcsKey, VcsStatusStore},
};

const VCS_OWNER: &str = "composer-branch-toolbar";
const REFS_LIMIT: u32 = 50;

/// The branch toolbar for one composer target.
pub struct BranchToolbar {
    environment: Entity<Environment>,
    vcs: Entity<VcsStatusStore>,
    target: ComposerTarget,
    open: Option<Popup>,
    branch_search: Option<Entity<InputState>>,
    refs: Vec<VcsRef>,
    total_refs: u64,
    loading_refs: bool,
    refs_task: Option<Task<()>>,
    pending_action: bool,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Popup {
    Workspace,
    Branch,
}

/// What the toolbar shows for the target.
struct ToolbarInfo {
    project_cwd: String,
    worktree_path: Option<String>,
    branch: Option<String>,
    env_mode: DraftEnvMode,
    locked: bool,
    start_from_origin: bool,
}

impl BranchToolbar {
    pub fn new(
        environment: Entity<Environment>,
        target: ComposerTarget,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe(&environment, |this, _, cx| {
            this.sync_vcs_interest(cx);
            cx.notify();
        })];
        if let Some(drafts) = DraftStore::global_ref(cx) {
            subscriptions.push(cx.observe(&drafts, |_, _, cx| cx.notify()));
        }
        let vcs = VcsStatusStore::global(cx);
        subscriptions.push(cx.observe(&vcs, |_, _, cx| cx.notify()));
        let vcs_store = vcs.clone();
        let mut toolbar = Self {
            environment,
            vcs: vcs_store,
            target,
            open: None,
            branch_search: None,
            refs: Vec::new(),
            total_refs: 0,
            loading_refs: false,
            refs_task: None,
            pending_action: false,
            _subscriptions: subscriptions,
        };
        toolbar.sync_vcs_interest(cx);
        toolbar
    }

    fn info(&self, cx: &App) -> Option<ToolbarInfo> {
        let environment = self.environment.read(cx);
        match &self.target {
            ComposerTarget::Thread(thread) => {
                let thread = environment.thread(&thread.thread_id)?;
                let project = environment.project(&thread.project_id)?;
                let started = thread.latest_turn.is_some()
                    || thread.session.as_ref().is_some_and(|session| {
                        session.status != t3_protocol::orchestration::SessionStatus::Stopped
                    });
                Some(ToolbarInfo {
                    project_cwd: project.workspace_root.clone(),
                    worktree_path: thread.worktree_path.clone(),
                    branch: thread.branch.clone(),
                    env_mode: if thread.worktree_path.is_some() {
                        DraftEnvMode::Worktree
                    } else {
                        DraftEnvMode::Local
                    },
                    locked: started || thread.worktree_path.is_some(),
                    start_from_origin: false,
                })
            }
            ComposerTarget::Draft(id) => {
                let draft = DraftStore::global_ref(cx)?
                    .read(cx)
                    .draft_thread(id)?
                    .clone();
                let project = environment.project(&draft.project_id)?;
                Some(ToolbarInfo {
                    project_cwd: project.workspace_root.clone(),
                    env_mode: if draft.worktree_path.is_some() {
                        DraftEnvMode::Local
                    } else {
                        draft.env_mode
                    },
                    worktree_path: draft.worktree_path,
                    branch: draft.branch,
                    locked: false,
                    start_from_origin: draft.start_from_origin,
                })
            }
        }
    }

    fn vcs_key(&self, info: &ToolbarInfo, cx: &App) -> VcsKey {
        VcsKey {
            environment_id: self.environment.read(cx).id().clone(),
            cwd: info
                .worktree_path
                .clone()
                .unwrap_or_else(|| info.project_cwd.clone()),
        }
    }

    fn sync_vcs_interest(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.info(cx) else {
            return;
        };
        let key = self.vcs_key(&info, cx);
        self.vcs.update(cx, |store, cx| {
            store.set_interest(VCS_OWNER, HashSet::from([key]), cx)
        });
    }

    /// The project is a git repo (true while status is still loading).
    pub fn is_repo(&self, cx: &App) -> bool {
        let Some(info) = self.info(cx) else {
            return false;
        };
        self.local_status(&info, cx)
            .is_none_or(|local| local.is_repo)
    }

    fn local_status<'a>(
        &self,
        info: &ToolbarInfo,
        cx: &'a App,
    ) -> Option<&'a t3_protocol::vcs::VcsStatusLocal> {
        let key = self.vcs_key(info, cx);
        self.vcs.read(cx).status(&key)?.local.as_ref()
    }

    fn current_git_branch(&self, info: &ToolbarInfo, cx: &App) -> Option<String> {
        self.local_status(info, cx)?.ref_name.clone()
    }

    fn set_env_mode(&mut self, mode: DraftEnvMode, cx: &mut Context<Self>) {
        self.open = None;
        if let ComposerTarget::Draft(id) = &self.target {
            let id = id.clone();
            DraftStore::global(cx).update(cx, |drafts, cx| {
                drafts.update_draft_thread(&id, |draft| draft.env_mode = mode, cx)
            });
        }
        cx.notify();
    }

    fn open_branch_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = Some(Popup::Branch);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search refs..."));
        self._subscriptions.push(
            cx.subscribe(&search, |this, search, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = search.read(cx).value().to_string();
                    this.load_refs(query, cx);
                }
            }),
        );
        search.update(cx, |search, cx| search.focus(window, cx));
        self.branch_search = Some(search);
        self.load_refs(String::new(), cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.open = None;
        self.branch_search = None;
        self.refs_task = None;
        cx.notify();
    }

    fn load_refs(&mut self, query: String, cx: &mut Context<Self>) {
        let (Some(client), Some(info)) =
            (self.environment.read(cx).client().cloned(), self.info(cx))
        else {
            return;
        };
        let cwd = info.worktree_path.unwrap_or(info.project_cwd);
        self.loading_refs = true;
        let delay = cx.background_executor().timer(Duration::from_millis(120));
        self.refs_task = Some(cx.spawn(async move |this, cx| {
            delay.await;
            let input = VcsListRefsInput {
                cwd,
                query: (!query.is_empty()).then_some(query),
                limit: Some(REFS_LIMIT),
                include_matching_remote_refs: Some(true),
                ..Default::default()
            };
            let result = cx
                .background_spawn(async move { client.request::<VcsListRefs>(&input).await })
                .await;
            this.update(cx, |this, cx| {
                this.loading_refs = false;
                if let Ok(result) = result {
                    this.refs = result.refs;
                    this.total_refs = result.total_count;
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Picks a ref (`BranchToolbarBranchSelector` actions): a draft in New worktree mode only
    /// records the base branch; otherwise the checkout switches (or a ref with a worktree is
    /// reused) and the thread/draft records the branch.
    fn pick_ref(&mut self, git_ref: VcsRef, create: bool, cx: &mut Context<Self>) {
        let Some(info) = self.info(cx) else {
            return;
        };
        self.close(cx);
        let branch_name = if git_ref.is_remote == Some(true) {
            git_ref
                .remote_name
                .as_ref()
                .and_then(|remote| git_ref.name.strip_prefix(&format!("{remote}/")))
                .unwrap_or(&git_ref.name)
                .to_owned()
        } else {
            git_ref.name.clone()
        };
        let picking_base = matches!(self.target, ComposerTarget::Draft(_))
            && info.env_mode == DraftEnvMode::Worktree
            && info.worktree_path.is_none();
        if picking_base {
            self.record_branch(Some(branch_name), info.worktree_path, cx);
            return;
        }
        if let Some(worktree) = git_ref.worktree_path.clone() {
            let path = (worktree != info.project_cwd).then_some(worktree);
            self.record_branch(Some(branch_name), path, cx);
            return;
        }
        let Some(client) = self.environment.read(cx).client().cloned() else {
            return;
        };
        let cwd = info
            .worktree_path
            .clone()
            .unwrap_or(info.project_cwd.clone());
        self.pending_action = true;
        let worktree = info.worktree_path;
        cx.spawn(async move |this, cx| {
            let name = branch_name.clone();
            let result = cx
                .background_spawn(async move {
                    if create {
                        client
                            .request::<VcsCreateRef>(&VcsCreateRefInput {
                                cwd,
                                ref_name: name,
                                switch_ref: Some(true),
                            })
                            .await
                            .map(|_| ())
                    } else {
                        client
                            .request::<VcsSwitchRef>(&VcsSwitchRefInput {
                                cwd,
                                ref_name: name,
                            })
                            .await
                            .map(|_| ())
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.pending_action = false;
                match result {
                    Ok(()) => this.record_branch(Some(branch_name), worktree, cx),
                    Err(_) => {
                        crate::toast::show(
                            crate::toast::Toast::error(if create {
                                "Failed to create and switch ref."
                            } else {
                                "Failed to switch ref."
                            }),
                            cx,
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn record_branch(
        &mut self,
        branch: Option<String>,
        worktree_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match &self.target {
            ComposerTarget::Draft(id) => {
                let id = id.clone();
                DraftStore::global(cx).update(cx, |drafts, cx| {
                    drafts.update_draft_thread(
                        &id,
                        |draft| {
                            draft.branch = branch;
                            draft.worktree_path = worktree_path;
                        },
                        cx,
                    )
                });
            }
            ComposerTarget::Thread(thread) => {
                let patch = ThreadMetaPatch {
                    branch: Some(branch),
                    worktree_path: Some(worktree_path),
                    ..Default::default()
                };
                self.environment
                    .read(cx)
                    .dispatch(commands::update_thread(thread.thread_id.clone(), patch), cx)
                    .detach_and_log_err(cx);
            }
        }
    }

    fn set_start_from_origin(&mut self, value: bool, cx: &mut Context<Self>) {
        if let ComposerTarget::Draft(id) = &self.target {
            let id = id.clone();
            DraftStore::global(cx).update(cx, |drafts, cx| {
                drafts.update_draft_thread(&id, |draft| draft.start_from_origin = value, cx)
            });
        }
    }

    fn render_workspace(&mut self, info: &ToolbarInfo, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let local_label = if info.worktree_path.is_some() {
            "Current worktree"
        } else {
            "Current checkout"
        };
        if info.locked {
            let (icon, label) = if info.worktree_path.is_some() {
                (IconName::FolderGit, "Worktree")
            } else {
                (IconName::Folder, "Local checkout")
            };
            return div()
                .flex()
                .items_center()
                .gap(px(4.))
                .border_1()
                .border_color(gpui_kit::transparent_black())
                .px(px(11.))
                .text_size(px(12.))
                .line_height(px(16.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(style::alpha(colors.muted_foreground, 0.7))
                .child(Icon::new(icon).size(px(12.)))
                .child(label)
                .into_any_element();
        }
        let worktree = info.env_mode == DraftEnvMode::Worktree;
        let icon = if worktree {
            IconName::FolderGit2
        } else if info.worktree_path.is_some() {
            IconName::FolderGit
        } else {
            IconName::Folder
        };
        let open = self.open == Some(Popup::Workspace);
        div()
            .relative()
            .child(
                style::ghost_trigger("branch-toolbar-workspace", open, colors)
                    .px(px(7.))
                    .gap(px(4.))
                    .child(Icon::new(icon).size(px(12.)))
                    .child(if worktree {
                        "New worktree"
                    } else {
                        local_label
                    })
                    .child(Icon::new(IconName::ChevronDown).size(px(12.)).opacity(0.6))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open = if open { None } else { Some(Popup::Workspace) };
                        cx.notify();
                    })),
            )
            .when(open, |this| {
                let menu = MenuPopup::new()
                    .child(MenuGroupLabel::new("Workspace"))
                    .child(
                        MenuCheckboxItem::new("workspace-local", local_label)
                            .checked(!worktree)
                            .on_change(cx.listener(|this, _, _, cx| {
                                this.set_env_mode(DraftEnvMode::Local, cx)
                            })),
                    )
                    .child(
                        MenuCheckboxItem::new("workspace-worktree", "New worktree")
                            .checked(worktree)
                            .on_change(cx.listener(|this, _, _, cx| {
                                this.set_env_mode(DraftEnvMode::Worktree, cx)
                            })),
                    );
                this.child(
                    deferred(
                        div()
                            .id("workspace-menu")
                            .absolute()
                            .bottom(px(28.))
                            .left_0()
                            .occlude()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
                            .child(menu),
                    )
                    .with_priority(2),
                )
            })
            .into_any_element()
    }

    fn render_branch(&mut self, info: &ToolbarInfo, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let worktree_base = info.env_mode == DraftEnvMode::Worktree && info.worktree_path.is_none();
        let git_branch = self.current_git_branch(info, cx);
        let active = if worktree_base {
            info.branch.clone().or(git_branch)
        } else {
            git_branch.or(info.branch.clone())
        };
        let label = match &active {
            None => "Select ref".to_owned(),
            Some(branch) if worktree_base => format!("From {branch}"),
            Some(branch) => branch.clone(),
        };
        let open = self.open == Some(Popup::Branch);
        let rest = style::alpha(colors.muted_foreground, 0.7);
        let trigger = style::ghost_trigger("branch-toolbar-branch", open, colors)
            .px(px(7.))
            .gap(px(4.))
            .min_w_0()
            .when(self.pending_action, |this| this.opacity(0.64))
            .child(Icon::new(IconName::GitBranch).size(px(12.)).opacity(0.7))
            .child(div().min_w_0().max_w(px(240.)).truncate().child(label))
            .child(Icon::new(IconName::ChevronDown).size(px(12.)).opacity(0.5))
            .when(!self.pending_action, |this| {
                this.on_click(cx.listener(move |this, _, window, cx| {
                    if open {
                        this.close(cx);
                    } else {
                        this.open_branch_picker(window, cx);
                    }
                }))
            });
        div()
            .relative()
            .min_w_0()
            .child(trigger)
            .when(open, |this| {
                let query = self
                    .branch_search
                    .as_ref()
                    .map(|search| search.read(cx).value().to_string())
                    .unwrap_or_default();
                let exact = self.refs.iter().any(|r| r.name == query);
                let show_create = !query.trim().is_empty() && !exact && !worktree_base;
                let status = if self.loading_refs && self.refs.is_empty() {
                    "Loading refs...".to_owned()
                } else {
                    format!("Showing {} of {} refs", self.refs.len(), self.total_refs.max(self.refs.len() as u64))
                };
                let current = active.clone();
                let list = div()
                    .id("branch-list")
                    .max_h(px(224.))
                    .overflow_y_scroll()
                    .pl(px(4.))
                    .pt(px(8.))
                    .pb(px(4.))
                    .when(self.refs.is_empty() && !self.loading_refs, |this| {
                        this.child(
                            div()
                                .py(px(24.))
                                .text_center()
                                .text_size(px(12.))
                                .text_color(colors.muted_foreground)
                                .child("No refs found."),
                        )
                    })
                    .children(self.refs.iter().cloned().map(|git_ref| {
                        let badge = if current.as_deref() == Some(git_ref.name.as_str()) {
                            Some("current")
                        } else if git_ref.worktree_path.is_some() {
                            Some("worktree")
                        } else if git_ref.is_remote == Some(true) {
                            Some("remote")
                        } else if git_ref.is_default {
                            Some("default")
                        } else {
                            None
                        };
                        let name = git_ref.name.clone();
                        div()
                            .id(SharedString::from(format!("ref-{name}")))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .min_h(px(28.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .text_size(px(14.))
                            .line_height(px(20.))
                            .text_color(colors.foreground)
                            .cursor_pointer()
                            .hover(|style| style.bg(colors.accent))
                            .child(div().flex_1().min_w_0().truncate().child(name))
                            .when_some(badge, |this, badge| {
                                this.child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(style::alpha(colors.muted_foreground, 0.45))
                                        .child(badge),
                                )
                            })
                            .on_click(cx.listener(move |this, _, _, cx| this.pick_ref(git_ref.clone(), false, cx)))
                    }))
                    .when(show_create, |this| {
                        let name = query.trim().to_owned();
                        this.child(
                            div()
                                .id("ref-create")
                                .flex()
                                .items_center()
                                .min_h(px(28.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .text_size(px(14.))
                                .cursor_pointer()
                                .hover(|style| style.bg(colors.accent))
                                .child(format!("Create new ref \"{name}\""))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.pick_ref(
                                        VcsRef {
                                            name: name.clone(),
                                            is_remote: None,
                                            remote_name: None,
                                            current: false,
                                            is_default: false,
                                            worktree_path: None,
                                        },
                                        true,
                                        cx,
                                    )
                                })),
                        )
                    });
                let popup = div()
                    .id("branch-popup")
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close(cx)))
                    .w(px(320.))
                    .flex()
                    .flex_col()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(colors.border)
                    .bg(colors.popover)
                    .shadow(shadow::LG_5.to_vec())
                    .child(
                        div().px(px(12.)).pt(px(10.)).child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .pb(px(6.))
                                .border_b_1()
                                .border_color(colors.border_80)
                                .child(
                                    Icon::new(IconName::Search)
                                        .size(px(16.))
                                        .color(style::alpha(colors.muted_foreground, 0.55)),
                                )
                                .children(self.branch_search.as_ref().map(|search| {
                                    div().flex_1().h(px(26.)).text_size(px(14.)).child(Input::new(search))
                                })),
                        ),
                    )
                    .child(list)
                    .child(
                        div()
                            .px(px(12.))
                            .pb(px(6.))
                            .text_size(px(11.))
                            .text_color(rest)
                            .child(status),
                    )
                    .when(worktree_base, |this| {
                        let checked = info.start_from_origin;
                        this.child(
                            div()
                                .id("start-from-origin")
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .border_t_1()
                                .border_color(colors.border_60)
                                .px(px(12.))
                                .py(px(8.))
                                .text_size(px(12.))
                                .child(Icon::new(IconName::RefreshCw).size(px(12.)).color(colors.muted_foreground))
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.muted_foreground)
                                        .child("Start from origin"),
                                )
                                .child(Switch::new("start-from-origin-switch").small().checked(checked))
                                .tooltip_text("Creates the worktree from the latest matching branch on origin instead of your local branch.")
                                .on_click(cx.listener(move |this, _, _, cx| this.set_start_from_origin(!checked, cx))),
                        )
                    });
                this.child(
                    deferred(div().absolute().bottom(px(28.)).right_0().child(popup)).with_priority(2),
                )
            })
            .into_any_element()
    }
}

impl Render for BranchToolbar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(info) = self.info(cx) else {
            return div().into_any_element();
        };
        div()
            .mx_auto()
            .w_full()
            .max_w(COLUMN_MAX_WIDTH)
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .pb(px(12.))
            .pt(px(4.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.))
                    .child(self.render_workspace(&info, cx)),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .min_w_0()
                    .child(self.render_branch(&info, cx)),
            )
            .into_any_element()
    }
}
