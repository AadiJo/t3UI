//! The thread terminal drawer (panels.md 3, web `ThreadTerminalDrawer.tsx`,
//! `terminalUiStateStore.ts`, ChatView's `PersistentThreadTerminalDrawer`).
//!
//! - [`TerminalStore`]: global. Per-thread layouts (open, height, terminals, split groups),
//!   persisted to `terminal-state.json`; the server's terminal metadata; live sessions.
//! - [`TerminalDrawer`]: the view at the bottom of the chat column. It animates open and
//!   closed, resizes from its top edge, and lays out the active group's terminals plus the
//!   terminal sidebar once a thread has more than one.
//! - [`TerminalSession`]: one terminal's `TerminalView` wired to `terminal.attach/write/
//!   resize`; the tab closes itself when the shell exits.
//!
//! Mounting (chat view): build one drawer per chat view and make it the last child of the
//! chat column, below the composer:
//!
//! ```ignore
//! let workspace = ThreadWorkspace::for_server_thread(&thread, cx); // or build one for a draft
//! let drawer = cx.new(|cx| TerminalDrawer::new(workspace, window, cx));
//! cx.subscribe(&drawer, |this, _, event: &TerminalDrawerEvent, cx| match event {
//!     TerminalDrawerEvent::AddToChat(selection) => { /* composer: add terminal context */ }
//! });
//! // render: column.child(self.drawer.clone())
//! ```
//!
//! The drawer handles `terminal.toggle/new/split/splitVertical/close` from
//! [`crate::state::AppEvent::Command`] for its thread while it is the route's drawer.

mod session;
mod store;
mod view;

use std::{collections::BTreeMap, time::Duration};

use gpui_kit::App;
use t3_client::RpcError;
use t3_logic::{ProjectRef, ThreadRef, project_scripts};
use t3_protocol::{EnvironmentId, ProjectId, errors::ServerError};

pub use session::{SessionStatus, TerminalSession};
pub use store::{DrawerRequest, TerminalStore};
pub use view::{TerminalDrawer, TerminalDrawerEvent};

use crate::state::AppState;

/// Pause before re-subscribing a stream that ended or failed.
pub(crate) const RETRY_DELAY: Duration = Duration::from_millis(250);

/// Where a thread's terminals, scripts and git actions run: the project's workspace root, or
/// the thread's worktree when it has one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadWorkspace {
    pub thread: ThreadRef,
    pub project_id: ProjectId,
    pub project_root: String,
    pub worktree_path: Option<String>,
}

impl ThreadWorkspace {
    /// The workspace of a server thread, from its environment's shell. `None` until the shell
    /// lists the thread and its project.
    pub fn for_server_thread(thread: &ThreadRef, cx: &App) -> Option<Self> {
        let environment = AppState::global(cx)
            .read(cx)
            .environment(&thread.environment_id, cx)?;
        let environment = environment.read(cx);
        let shell = environment.thread(&thread.thread_id)?;
        let project = environment.project(&shell.project_id)?;
        Some(Self {
            thread: thread.clone(),
            project_id: project.id.clone(),
            project_root: project.workspace_root.clone(),
            worktree_path: shell.worktree_path.clone(),
        })
    }

    /// `gitCwd ?? workspaceRoot`: terminals start here and git actions run here.
    pub fn cwd(&self) -> &str {
        self.worktree_path.as_deref().unwrap_or(&self.project_root)
    }

    /// `T3CODE_PROJECT_ROOT` and, in a worktree, `T3CODE_WORKTREE_PATH`.
    pub fn runtime_env(&self) -> BTreeMap<String, String> {
        project_scripts::runtime_env(&self.project_root, self.worktree_path.as_deref())
    }

    pub fn project(&self) -> ProjectRef {
        ProjectRef::new(self.thread.environment_id.clone(), self.project_id.clone())
    }
}

/// A terminal selection the user chose "Add to chat" for (web `TerminalContextSelection`).
/// Lines are 1-based buffer lines; text has LF line endings and no leading/trailing newlines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalContextSelection {
    pub thread: ThreadRef,
    pub terminal_id: String,
    pub terminal_label: String,
    pub line_start: usize,
    pub line_end: usize,
    pub text: String,
}

/// The connected client of an environment, if any.
pub(crate) fn environment_client(
    environment_id: &EnvironmentId,
    cx: &App,
) -> Option<t3_client::Environment> {
    AppState::global(cx)
        .read(cx)
        .environment(environment_id, cx)?
        .read(cx)
        .client()
        .cloned()
}

/// What to show for a failed RPC: the server's message, else the transport error.
pub(crate) fn rpc_error_text(error: &RpcError<ServerError>) -> String {
    match error.failure() {
        Some(failure) => failure.display_message().to_owned(),
        None => error.to_string(),
    }
}
