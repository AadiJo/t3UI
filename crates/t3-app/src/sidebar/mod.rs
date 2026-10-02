//! The main sidebar (spec section 2): search row, projects grouped across environments, thread
//! rows with status, and the footer.
//!
//! The view keeps a derived [`SidebarModel`] (built by `t3_logic::sidebar::build_sidebar`) and
//! rebuilds it whenever [`AppState`] changes, so `render` only lays out what the model says.
//! Transient state that the web keeps in memory lives here too: multi-selection, "Show more"
//! lists, and jump-hint visibility.

mod menus;
mod pulse;
mod render;

use std::{collections::HashSet, time::Duration};

use gpui_kit::{
    App, Context, Entity, FocusHandle, Focusable, ScrollHandle, Subscription, Task, Window,
};
use t3_logic::{
    ThreadRef,
    keybindings::{Modifiers, Platform, ShortcutContext, should_show_thread_jump_hints},
    sidebar::{EnvironmentShell, SidebarInputs, SidebarModel, ThreadSelection, build_sidebar},
};
use t3_protocol::orchestration::ThreadEnvMode;

pub use pulse::{PulseClock, pulse_opacity};

use crate::state::{
    AppState, Environment, EnvironmentKind, NewThreadRequest, Route,
    vcs::{VcsKey, VcsStatusStore},
};

/// Jump-hint pills appear after the modifier is held this long (`THREAD_JUMP_HINT_SHOW_DELAY_MS`).
const JUMP_HINT_DELAY: Duration = Duration::from_millis(100);

/// The sidebar view. Mount one per window inside the workspace's sidebar container.
pub struct Sidebar {
    app_state: Entity<AppState>,
    model: SidebarModel,
    /// Logical project keys with "Show more" active.
    expanded_lists: HashSet<String>,
    selection: ThreadSelection,
    jump_hints_visible: bool,
    jump_hint_timer: Option<Task<()>>,
    /// The thread row being renamed inline.
    rename: Option<menus::Rename>,
    /// The row showing the archive "Confirm" pill (`confirmThreadArchive`).
    confirming_archive: Option<ThreadRef>,
    pulse: PulseClock,
    scroll: ScrollHandle,
    focus: FocusHandle,
    vcs: Entity<VcsStatusStore>,
    _subscriptions: [Subscription; 2],
}

impl Sidebar {
    pub fn new(app_state: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let vcs = VcsStatusStore::global(cx);
        let subscriptions = [
            cx.observe(&app_state, |this, _, cx| this.rebuild(cx)),
            cx.observe(&vcs, |_, _, cx| cx.notify()),
        ];
        let mut sidebar = Self {
            app_state,
            model: SidebarModel::default(),
            expanded_lists: HashSet::new(),
            selection: ThreadSelection::default(),
            jump_hints_visible: false,
            jump_hint_timer: None,
            rename: None,
            confirming_archive: None,
            pulse: PulseClock::default(),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
            vcs,
            _subscriptions: subscriptions,
        };
        sidebar.rebuild(cx);
        sidebar
    }

    /// The derived model, for keyboard navigation and tests.
    pub fn model(&self) -> &SidebarModel {
        &self.model
    }

    /// Rebuilds the model from app state.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let app_state = self.app_state.read(cx);
        let environments: Vec<&Environment> = app_state
            .environments()
            .iter()
            .map(|environment| environment.read(cx))
            .collect();
        let shells: Vec<EnvironmentShell<'_>> = environments
            .iter()
            .map(|environment| EnvironmentShell {
                id: environment.id(),
                label: Some(environment.label().as_ref()),
                desktop_local: environment.kind() == EnvironmentKind::DesktopLocal,
                projects: environment.projects(),
                threads: environment.threads(),
            })
            .collect();
        self.model = build_sidebar(&SidebarInputs {
            environments: &shells,
            primary_environment: environments.first().map(|environment| environment.id()),
            settings: app_state.settings(),
            ui: app_state.ui(),
            route_thread: app_state.route().thread(),
            expanded_thread_lists: &self.expanded_lists,
            optimistic_working: app_state.optimistic_work(),
        });
        let animate = app_state.clock_is_live() && self.has_visible_pulse(cx);
        self.pulse.set_active(animate, cx);
        let interest = self.vcs_interest();
        self.vcs
            .update(cx, |store, cx| store.set_interest("sidebar", interest, cx));
        cx.notify();
    }

    /// The working copy whose VCS status decides a row's PR badge: the thread's worktree, else
    /// its project root. Only threads on a branch have a badge.
    fn vcs_key(row: &t3_logic::sidebar::SidebarThread) -> Option<VcsKey> {
        row.thread.branch.as_ref()?;
        let cwd = row
            .thread
            .worktree_path
            .clone()
            .or_else(|| row.project_root.clone())?;
        Some(VcsKey {
            environment_id: row.thread_ref.environment_id.clone(),
            cwd,
        })
    }

    fn vcs_interest(&self) -> HashSet<VcsKey> {
        self.model
            .projects
            .iter()
            .flat_map(|project| &project.rendered_threads)
            .filter_map(Self::vcs_key)
            .collect()
    }

    /// Whether a pulsing dot is on screen: a Working row, a collapsed project's Working dot, or a
    /// Working thread behind "Show more".
    fn has_visible_pulse(&self, cx: &App) -> bool {
        if !self.app_state.read(cx).sidebar_open() {
            return false;
        }
        self.model.projects.iter().any(|project| {
            let header = !project.expanded && project.status.is_some_and(|status| status.pulses());
            let hidden = project.expanded
                && !project.thread_list_expanded
                && project.hidden_status.is_some_and(|status| status.pulses());
            let rows = project
                .rendered_threads
                .iter()
                .any(|row| row.status.is_some_and(|status| status.pulses()));
            header || hidden || rows
        })
    }

    // -------------------------------------------------------------------------------------------
    // Navigation

    /// Opens a thread: clears any selection, anchors it, and navigates.
    pub fn navigate_to_thread(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) {
        self.selection.clear();
        self.selection.set_anchor(thread);
        let route = Route::Thread(thread.clone());
        self.app_state
            .update(cx, |state, cx| state.navigate(route, cx));
    }

    /// `thread.previous` / `thread.next`. Returns whether it navigated.
    pub fn navigate_adjacent(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let current = self.app_state.read(cx).route().thread().cloned();
        let Some(target) = self
            .model
            .adjacent_thread(current.as_ref(), forward)
            .cloned()
        else {
            return false;
        };
        self.navigate_to_thread(&target, cx);
        true
    }

    /// `thread.jump.N`. Returns whether it navigated.
    pub fn jump_to(&mut self, index: u8, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.model.jump_target(index).cloned() else {
            return false;
        };
        self.navigate_to_thread(&target, cx);
        true
    }

    // -------------------------------------------------------------------------------------------
    // Selection

    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Clears the multi-selection (Escape, outside clicks). Returns whether anything changed.
    pub fn clear_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let changed = self.selection.clear();
        if changed {
            cx.notify();
        }
        changed
    }

    /// Row click (spec 2.12.4): mod-click toggles, shift-click selects a range in the project,
    /// a plain click navigates (except the second click of a double click).
    fn click_thread(
        &mut self,
        thread: &ThreadRef,
        project_threads: &[ThreadRef],
        modifiers: gpui_kit::Modifiers,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        let mod_click = match Platform::current() {
            Platform::Mac => modifiers.platform,
            Platform::Other => modifiers.control,
        };
        if mod_click {
            self.selection.toggle(thread);
            cx.notify();
        } else if modifiers.shift {
            self.selection.select_range(thread, project_threads);
            cx.notify();
        } else if click_count <= 1 {
            self.navigate_to_thread(thread, cx);
        }
    }

    // -------------------------------------------------------------------------------------------
    // Projects

    fn toggle_project(&mut self, key: &str, cx: &mut Context<Self>) {
        self.clear_selection(cx);
        let Some(project) = self
            .model
            .projects
            .iter()
            .find(|project| project.key == key)
        else {
            return;
        };
        let keys = project.expansion_keys.clone();
        let expanded = !project.expanded;
        self.app_state.update(cx, |state, cx| {
            state.update_ui(|ui| ui.set_project_expanded(&keys, expanded), cx)
        });
    }

    fn set_thread_list_expanded(&mut self, key: &str, expanded: bool, cx: &mut Context<Self>) {
        let changed = if expanded {
            self.expanded_lists.insert(key.to_owned())
        } else {
            self.expanded_lists.remove(key)
        };
        if changed {
            self.rebuild(cx);
        }
    }

    /// New thread in a project member (spec 2.10), seeded from the route thread when it belongs
    /// to the same project. Draft seeding is left to the draft owner.
    fn new_thread_in(&mut self, project: t3_logic::ProjectRef, cx: &mut Context<Self>) {
        let state = self.app_state.read(cx);
        let environment = state.environment(&project.environment_id, cx);
        let default_mode = environment.as_ref().and_then(|environment| {
            environment
                .read(cx)
                .project(&project.project_id)
                .and_then(|shell| shell.default_thread_env_mode.clone())
        });
        let active = state.route().thread().and_then(|thread| {
            let shell = state
                .environment(&thread.environment_id, cx)?
                .read(cx)
                .thread(&thread.thread_id)?
                .clone();
            (thread.environment_id == project.environment_id
                && shell.project_id == project.project_id)
                .then_some(shell)
        });
        let request = match (default_mode, active) {
            (Some(ThreadEnvMode::Worktree), _) => NewThreadRequest {
                project,
                branch: None,
                worktree_path: None,
                env_mode: Some(ThreadEnvMode::Worktree),
                start_from_origin: None,
            },
            (_, Some(active)) => NewThreadRequest {
                env_mode: Some(if active.worktree_path.is_some() {
                    ThreadEnvMode::Worktree
                } else {
                    ThreadEnvMode::Local
                }),
                branch: active.branch.clone(),
                worktree_path: active.worktree_path.clone(),
                project,
                start_from_origin: None,
            },
            _ => NewThreadRequest {
                project,
                branch: None,
                worktree_path: None,
                env_mode: None,
                start_from_origin: None,
            },
        };
        self.app_state
            .update(cx, |state, cx| state.request_new_thread(request, cx));
    }

    /// `chat.new` / `chat.newLocal` outside a draft (web `startNewThreadFromContext`): a new
    /// thread in the route thread's project, else the first project in sidebar order. `chat.new`
    /// carries the route thread's branch and worktree. Returns false with no project at all.
    pub fn new_thread_from_shortcut(
        &mut self,
        carry_context: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let state = self.app_state.read(cx);
        let active = state.route().thread().and_then(|thread| {
            let shell = state
                .environment(&thread.environment_id, cx)?
                .read(cx)
                .thread(&thread.thread_id)?
                .clone();
            Some((thread.environment_id.clone(), shell))
        });
        let request = match active {
            Some((environment_id, shell)) => {
                let project = t3_logic::ProjectRef::new(environment_id, shell.project_id.clone());
                if carry_context {
                    NewThreadRequest {
                        project,
                        branch: shell.branch.clone(),
                        worktree_path: shell.worktree_path.clone(),
                        env_mode: Some(if shell.worktree_path.is_some() {
                            ThreadEnvMode::Worktree
                        } else {
                            ThreadEnvMode::Local
                        }),
                        start_from_origin: None,
                    }
                } else {
                    NewThreadRequest {
                        project,
                        branch: None,
                        worktree_path: None,
                        env_mode: None,
                        start_from_origin: None,
                    }
                }
            }
            None => {
                let Some(first) = self.model.project_order_keys.first() else {
                    return false;
                };
                let Some(member) = self
                    .model
                    .projects
                    .iter()
                    .flat_map(|project| &project.members)
                    .find(|member| &member.physical_key == first)
                else {
                    return false;
                };
                NewThreadRequest {
                    project: member.project_ref.clone(),
                    branch: None,
                    worktree_path: None,
                    env_mode: None,
                    start_from_origin: None,
                }
            }
        };
        self.app_state
            .update(cx, |state, cx| state.request_new_thread(request, cx));
        true
    }

    // -------------------------------------------------------------------------------------------
    // Jump hints

    /// Shows jump-hint pills 100ms after the held modifiers exactly match a `thread.jump.N`
    /// shortcut, and hides them as soon as they stop matching.
    pub fn sync_jump_hints(
        &mut self,
        modifiers: Modifiers,
        context: ShortcutContext,
        cx: &mut Context<Self>,
    ) {
        let rules = self.app_state.read(cx).keybindings(cx);
        let show = should_show_thread_jump_hints(modifiers, &rules, &context, Platform::current());
        if !show {
            self.jump_hint_timer = None;
            if self.jump_hints_visible {
                self.jump_hints_visible = false;
                cx.notify();
            }
            return;
        }
        if self.jump_hints_visible || self.jump_hint_timer.is_some() {
            return;
        }
        self.jump_hint_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(JUMP_HINT_DELAY).await;
            this.update(cx, |this, cx| {
                this.jump_hint_timer = None;
                this.jump_hints_visible = true;
                cx.notify();
            })
            .ok();
        }));
    }
}

impl Focusable for Sidebar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
