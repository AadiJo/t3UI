//! One server ("environment") as the UI sees it: a GPUI entity over a `t3_client::Environment`
//! handle. It mirrors the client's `watch` channels (connection status, server config, shell
//! read model) into entity state so views can read them in `render` and `cx.observe` the entity.
//!
//! Live environments come from [`Environment::connected`]; snapshot scenes build detached ones
//! with [`Environment::new`] and feed [`Environment::set_shell`] from recorded JSON.

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, SharedString, Task};
use t3_client::{ConnectionStatus, ShellState, ThreadHandle};
use t3_protocol::{
    EnvironmentId, ProjectId, ThreadId,
    commands::ClientCommand,
    method::Unary,
    orchestration::{OrchestrationProjectShell, OrchestrationThreadShell},
    server::ServerConfig,
    usage::UsageSummary,
};
use tokio::sync::watch;

/// How the client reaches an environment. Decides the sidebar's environment badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentKind {
    /// The server on this machine; the primary environment.
    Local,
    /// A desktop-local secondary backend such as WSL (sandbox badge).
    DesktopLocal,
    /// Any other saved or cloud-linked server (cloud badge).
    Remote,
}

/// A server and its read model.
pub struct Environment {
    id: EnvironmentId,
    label: SharedString,
    kind: EnvironmentKind,
    client: Option<t3_client::Environment>,
    status: ConnectionStatus,
    config: Option<Arc<ServerConfig>>,
    shell: Arc<ShellState>,
    /// What a detached (fixture) environment answers to `server.getUsageSummary`.
    usage_fixture: Option<Arc<UsageSummary>>,
    _watchers: Vec<Task<()>>,
}

impl Environment {
    /// A detached environment with no connection (fixtures and tests).
    pub fn new(id: EnvironmentId, label: impl Into<SharedString>, kind: EnvironmentKind) -> Self {
        Self {
            id,
            label: label.into(),
            kind,
            client: None,
            status: ConnectionStatus::Available,
            config: None,
            shell: Arc::new(ShellState::default()),
            usage_fixture: None,
            _watchers: Vec::new(),
        }
    }

    /// Wraps a started client environment and follows its status, config, and shell.
    pub fn connected(
        client: t3_client::Environment,
        kind: EnvironmentKind,
        cx: &mut Context<Self>,
    ) -> Self {
        let watchers = vec![
            follow(cx, client.status(), |this, status| this.status = status),
            follow(cx, client.config(), |this, config| this.config = config),
            follow(cx, client.shell(), |this, shell| this.shell = shell),
        ];
        Self {
            id: client.id().clone(),
            label: client.label().to_owned().into(),
            kind,
            status: client.status().borrow().clone(),
            config: client.config().borrow().clone(),
            shell: client.shell().borrow().clone(),
            client: Some(client),
            usage_fixture: None,
            _watchers: watchers,
        }
    }

    pub fn id(&self) -> &EnvironmentId {
        &self.id
    }

    /// Display label ("HOME-PC").
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    pub fn kind(&self) -> EnvironmentKind {
        self.kind
    }

    /// The client handle, for RPCs other than commands (`request`, `subscribe`, `open_thread`).
    /// `None` for detached environments.
    pub fn client(&self) -> Option<&t3_client::Environment> {
        self.client.as_ref()
    }

    pub fn status(&self) -> &ConnectionStatus {
        &self.status
    }

    /// Sets the status of a detached environment.
    pub fn set_status(&mut self, status: ConnectionStatus, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// The server config, once received.
    pub fn config(&self) -> Option<&Arc<ServerConfig>> {
        self.config.as_ref()
    }

    /// Sets the config of a detached environment.
    pub fn set_config(&mut self, config: Arc<ServerConfig>, cx: &mut Context<Self>) {
        self.config = Some(config);
        cx.notify();
    }

    /// Shell read model: every project and active thread.
    pub fn shell(&self) -> &Arc<ShellState> {
        &self.shell
    }

    /// Replaces the shell of a detached environment.
    pub fn set_shell(&mut self, shell: ShellState, cx: &mut Context<Self>) {
        self.shell = Arc::new(shell);
        cx.notify();
    }

    /// Every project, in server order.
    pub fn projects(&self) -> &[Arc<OrchestrationProjectShell>] {
        &self.shell.projects
    }

    /// Every active (non-archived) thread.
    pub fn threads(&self) -> &[Arc<OrchestrationThreadShell>] {
        &self.shell.threads
    }

    pub fn project(&self, id: &ProjectId) -> Option<&Arc<OrchestrationProjectShell>> {
        self.shell.project(id)
    }

    pub fn thread(&self, id: &ThreadId) -> Option<&Arc<OrchestrationThreadShell>> {
        self.shell.thread(id)
    }

    /// Subscribes to a thread's detail; dropping the handle unsubscribes. `None` when detached.
    pub fn open_thread(&self, thread_id: ThreadId) -> Option<ThreadHandle> {
        self.client
            .as_ref()
            .map(|client| client.open_thread(thread_id))
    }

    /// Sends a unary RPC (a `t3_protocol::methods` type) off the main thread. Fails with the
    /// server's error message, or when the environment is detached or disconnected.
    pub fn request<M>(&self, payload: M::Payload, cx: &App) -> Task<anyhow::Result<M::Success>>
    where
        M: Unary + 'static,
        M::Payload: Send + Sync + 'static,
        M::Error: std::fmt::Display,
    {
        let Some(client) = self.client.clone() else {
            return Task::ready(Err(anyhow::anyhow!("{} is not connected.", self.label)));
        };
        cx.background_spawn(async move {
            client
                .request::<M>(&payload)
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))
        })
    }

    /// The recorded usage summary a detached environment reports (snapshot fixtures).
    pub fn usage_fixture(&self) -> Option<&Arc<UsageSummary>> {
        self.usage_fixture.as_ref()
    }

    pub fn set_usage_fixture(&mut self, summary: UsageSummary) {
        self.usage_fixture = Some(Arc::new(summary));
    }

    /// Sends an orchestration command (`orchestration.dispatchCommand`; build it with
    /// `t3_client::commands`). Resolves once the server accepted it; the resulting change
    /// arrives through the shell and thread streams.
    pub fn dispatch(&self, command: ClientCommand, cx: &App) -> Task<anyhow::Result<()>> {
        let Some(client) = self.client.clone() else {
            return Task::ready(Err(anyhow::anyhow!("{} is not connected.", self.label)));
        };
        cx.background_spawn(async move {
            client
                .dispatch(command)
                .await
                .map(|_| ())
                .map_err(|error| anyhow::anyhow!("{error}"))
        })
    }
}

/// Mirrors a `watch` channel into the entity: applies each new value and notifies.
fn follow<T: Clone + Send + Sync + 'static>(
    cx: &mut Context<Environment>,
    mut receiver: watch::Receiver<T>,
    apply: impl Fn(&mut Environment, T) + 'static,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        while receiver.changed().await.is_ok() {
            let value = receiver.borrow_and_update().clone();
            let updated = this.update(cx, |environment, cx| {
                apply(environment, value);
                cx.notify();
            });
            if updated.is_err() {
                break;
            }
        }
    })
}
