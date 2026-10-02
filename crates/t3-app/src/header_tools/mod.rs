//! The chat header's right-side controls (panels.md 4.3-4.6, fork `ChatHeader.tsx`): project
//! scripts and Git actions, plus the PR checkout dialog and the (hidden in the header) Open-in
//! picker.
//!
//! [`register`] installs this module's chat slots at startup: [`Slot::HeaderActions`] mounts a
//! [`HeaderActions`] per chat view and [`Slot::TerminalDrawer`] the thread's
//! [`TerminalDrawer`](crate::terminal_drawer::TerminalDrawer).
//!
//! The header shows labels ("Add action", "Commit") only when its container is at least 768px
//! wide (`@3xl/header-actions`); narrower headers show icon-only buttons.

pub mod git_actions;
pub mod open_in;
pub mod pr_dialog;
pub mod prefs;
pub mod scripts;
pub mod widgets;

use gpui_kit::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels, Render,
    Styled as _, Subscription, Window, div, px,
};
use t3_logic::ProjectRef;
use t3_protocol::EnvironmentId;

use crate::{
    chat::{ChatTarget, Slot, SlotContext, register_slot},
    state::{AppEvent, AppState},
    terminal_drawer::{TerminalDrawer, ThreadWorkspace},
};
use git_actions::GitActionsControl;
use scripts::ProjectScriptsControl;
use t3_logic::keybindings::Command;

/// `@3xl/header-actions`: labels show from this container width.
const LABELS_MIN_WIDTH: Pixels = px(768.);

/// Registers the header actions and terminal drawer slots. Call once at startup, after
/// `t3_terminal::init`.
pub fn register(cx: &mut App) {
    register_slot(
        Slot::HeaderActions,
        |context: SlotContext, window, cx| {
            cx.new(|cx| HeaderActions::new(context.target, window, cx))
                .into()
        },
        cx,
    );
    register_slot(
        Slot::TerminalDrawer,
        |context: SlotContext, window, cx| {
            let workspace = match &context.target {
                ChatTarget::Thread(thread) => ThreadWorkspace::for_server_thread(thread, cx),
                ChatTarget::Draft { .. } => None,
            };
            cx.new(|cx| TerminalDrawer::new(workspace, window, cx)).into()
        },
        cx,
    );
}

/// What the header controls act on, re-resolved whenever the shell changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderContext {
    pub environment_id: EnvironmentId,
    pub project: ProjectRef,
    /// The project's workspace root.
    pub project_root: String,
    /// Server threads only: drafts cannot run scripts until they start.
    pub workspace: Option<ThreadWorkspace>,
}

impl HeaderContext {
    /// `gitCwd`: the thread's worktree, else the project root.
    pub fn git_cwd(&self) -> &str {
        self.workspace
            .as_ref()
            .map_or(&self.project_root, |workspace| workspace.cwd())
    }

    /// Resolves the context of a chat target from its environment's shell.
    pub fn resolve(target: &ChatTarget, cx: &App) -> Option<Self> {
        match target {
            ChatTarget::Thread(thread) => {
                let workspace = ThreadWorkspace::for_server_thread(thread, cx)?;
                Some(Self {
                    environment_id: thread.environment_id.clone(),
                    project: workspace.project(),
                    project_root: workspace.project_root.clone(),
                    workspace: Some(workspace),
                })
            }
            ChatTarget::Draft {
                project: Some(project),
                ..
            } => {
                let environment = AppState::global(cx)
                    .read(cx)
                    .environment(&project.environment_id, cx)?;
                let root = environment
                    .read(cx)
                    .project(&project.project_id)?
                    .workspace_root
                    .clone();
                Some(Self {
                    environment_id: project.environment_id.clone(),
                    project: project.clone(),
                    project_root: root,
                    workspace: None,
                })
            }
            ChatTarget::Draft { project: None, .. } => None,
        }
    }
}

/// The header's actions: scripts, then Git (the fork hides the Open-in picker here).
pub struct HeaderActions {
    target: ChatTarget,
    context: Option<HeaderContext>,
    scripts: Entity<ProjectScriptsControl>,
    git: Entity<GitActionsControl>,
    _subscriptions: Vec<Subscription>,
}

impl HeaderActions {
    pub fn new(target: ChatTarget, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_state = AppState::global(cx);
        let context = HeaderContext::resolve(&target, cx);
        let scripts = cx.new(|cx| ProjectScriptsControl::new(context.clone(), window, cx));
        let git = cx.new(|cx| GitActionsControl::new(context.clone(), window, cx));
        let subscriptions = vec![
            cx.observe(&app_state, Self::refresh_context),
            cx.subscribe_in(&app_state, window, Self::on_app_event),
        ];
        Self {
            target,
            context,
            scripts,
            git,
            _subscriptions: subscriptions,
        }
    }

    fn refresh_context(&mut self, _: Entity<AppState>, cx: &mut Context<Self>) {
        let context = HeaderContext::resolve(&self.target, cx);
        if context != self.context {
            self.context = context.clone();
            self.scripts
                .update(cx, |scripts, cx| scripts.set_context(context.clone(), cx));
            self.git.update(cx, |git, cx| git.set_context(context, cx));
        }
        cx.notify();
    }

    /// `script.{id}.run` shortcuts run through the same handler as the header button.
    fn on_app_event(
        &mut self,
        _: &Entity<AppState>,
        event: &AppEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let AppEvent::Command(Command::ScriptRun(id)) = event {
            let id = id.clone();
            self.scripts
                .update(cx, |scripts, cx| scripts.run_script_by_id(&id, cx));
        }
    }
}

/// Whether the header container is wide enough for labels: the main column minus the
/// sidebar and the header's 20px side padding.
pub(crate) fn header_is_wide(window: &Window, cx: &App) -> bool {
    let state = AppState::global(cx).read(cx);
    let sidebar = if state.sidebar_open() {
        state
            .ui()
            .sidebar_width
            .map_or(t3_ui::tokens::layout::SIDEBAR_WIDTH, px)
    } else {
        px(0.)
    };
    window.viewport_size().width - sidebar - px(40.) >= LABELS_MIN_WIDTH
}

impl Render for HeaderActions {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(12.))
            .when(self.context.is_some(), |this| {
                this.child(self.scripts.clone()).child(self.git.clone())
            })
    }
}

use gpui_kit::prelude::FluentBuilder as _;
