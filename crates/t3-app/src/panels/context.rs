//! [`PanelContext`]: what a right panel surface knows about where it is shown, and how it
//! reaches the server.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Entity, Global, Task};
use t3_logic::ThreadRef;
use t3_protocol::{
    method::Unary,
    orchestration::{OrchestrationProjectShell, OrchestrationThreadShell},
};

use super::store::RightPanels;
use crate::state::{AppState, Environment};

/// The thread a surface belongs to, plus helpers to reach its environment.
#[derive(Clone)]
pub struct PanelContext {
    pub app_state: Entity<AppState>,
    pub thread: ThreadRef,
}

impl PanelContext {
    pub fn new(app_state: Entity<AppState>, thread: ThreadRef) -> Self {
        Self { app_state, thread }
    }

    pub fn environment(&self, cx: &App) -> Option<Entity<Environment>> {
        self.app_state
            .read(cx)
            .environment(&self.thread.environment_id, cx)
    }

    /// The thread's shell summary (title, branch, worktree, project).
    pub fn thread_shell(&self, cx: &App) -> Option<Arc<OrchestrationThreadShell>> {
        self.environment(cx)?
            .read(cx)
            .thread(&self.thread.thread_id)
            .cloned()
    }

    pub fn project(&self, cx: &App) -> Option<Arc<OrchestrationProjectShell>> {
        let thread = self.thread_shell(cx)?;
        self.environment(cx)?
            .read(cx)
            .project(&thread.project_id)
            .cloned()
    }

    /// The working directory surfaces read: the thread's worktree, else the project root
    /// (`thread.worktreePath ?? project.workspaceRoot`).
    pub fn cwd(&self, cx: &App) -> Option<String> {
        let thread = self.thread_shell(cx)?;
        thread.worktree_path.clone().or_else(|| {
            self.project(cx)
                .map(|project| project.workspace_root.clone())
        })
    }

    /// The right panel store, to open files or switch tabs from a surface.
    pub fn panels(&self, cx: &mut App) -> Entity<RightPanels> {
        RightPanels::global(cx)
    }

    /// Sends a unary RPC to the thread's environment on a background thread. Without a live
    /// connection (snapshot scenes) it answers from the installed [`FixtureResponder`].
    pub fn request<M: Unary + 'static>(
        &self,
        payload: M::Payload,
        cx: &App,
    ) -> Task<anyhow::Result<M::Success>>
    where
        M::Payload: Send + Sync + 'static,
        M::Error: std::fmt::Display,
    {
        let client = self
            .environment(cx)
            .and_then(|environment| environment.read(cx).client().cloned());
        let Some(client) = client else {
            let answer = cx
                .try_global::<FixtureResponder>()
                .and_then(|responder| {
                    let payload = serde_json::to_value(&payload).ok()?;
                    (responder.0)(M::TAG, &payload)
                })
                .ok_or_else(|| anyhow::anyhow!("{} is not connected.", self.thread.environment_id))
                .and_then(|value| serde_json::from_value(value).map_err(Into::into));
            return Task::ready(answer);
        };
        cx.background_spawn(async move {
            client
                .request::<M>(&payload)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))
        })
    }
}

/// Canned RPC answers for snapshot scenes: `(method tag, payload) -> success JSON`.
pub struct FixtureResponder(
    #[allow(clippy::type_complexity)]
    pub  Box<dyn Fn(&'static str, &serde_json::Value) -> Option<serde_json::Value>>,
);

impl Global for FixtureResponder {}
