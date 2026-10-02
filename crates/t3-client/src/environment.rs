//! [`Environment`]: one server the app talks to. Owns the connection supervisor and keeps the
//! server config and shell in sync across reconnects.
//!
//! Everything runs on the networking runtime. The app reads state through `tokio::sync::watch`
//! receivers, which any executor can await:
//!
//! ```ignore
//! let env = Environment::start(EnvironmentOptions::new(saved.environment_id, saved.label, endpoint));
//! let mut shell = env.shell();
//! cx.spawn(async move |cx| {
//!     while shell.changed().await.is_ok() {
//!         let state = shell.borrow_and_update().clone(); // Arc<ShellState>
//!         // entity.update(cx, |e, cx| { e.shell = state; cx.notify() })
//!     }
//! });
//! let thread = env.open_thread(thread_id);          // dropping it unsubscribes
//! env.dispatch(commands::archive_thread(id)).await?;
//! ```
//!
//! Supervisor (connections.md 1.6, upstream `connection/supervisor.ts`): attempts are prepared
//! by the [`Endpoint`], open the socket, and count as connected once the first
//! `subscribeServerConfig` snapshot arrives. Transient failures retry after 3, 4, 8, then 16 s;
//! blocked failures wait for [`retry_now`](Environment::retry_now) or
//! [`app_active`](Environment::app_active). A connection that lasted 30 s resets the ladder.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use t3_protocol::{
    EnvironmentId, Stream, ThreadId, Unary,
    commands::{ClientCommand, TurnAttachment},
    environment::ExecutionEnvironmentDescriptor,
    errors::ServerError,
    methods::{
        AttachmentsCreateUploadUrl, DispatchCommand, Empty, ServerGetConfig, ServerProbe,
        SubscribeServerConfig, SubscribeShell, SubscribeThread,
    },
    orchestration::{AttachmentKind, DispatchResult, SubscribeShellInput, SubscribeThreadInput},
    projects::AttachmentCreateUploadUrlInput,
    server::{ServerConfig, ServerConfigStreamEvent, SubscribeServerConfigInput},
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use crate::{
    auth::{ClientInfo, Endpoint},
    connection::{
        BlockedReason, ConnectStage, ConnectionFailure, ConnectionStatus, TransientReason,
    },
    http::{EnvironmentHttp, HttpError, ThreadSnapshotQuery},
    rpc::{CloseReason, ConnectError, RpcConnection, RpcError, StreamNext, Subscription},
    runtime::runtime,
    shell::ShellState,
    thread::ThreadState,
};

const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_secs(3),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(16),
];
const ESTABLISH_TIMEOUT: Duration = Duration::from_secs(15);
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);
const STABLE_AFTER: Duration = Duration::from_secs(30);
/// Turns in the first page of a thread (upstream `INITIAL_THREAD_USER_TURN_LIMIT`).
const INITIAL_TURN_LIMIT: u32 = 10;
/// Turns per "load earlier" page.
const OLDER_TURN_LIMIT: u32 = 20;

/// What [`Environment::start`] needs.
pub struct EnvironmentOptions {
    /// The id the server must report; a mismatch blocks the connection.
    pub id: EnvironmentId,
    pub label: String,
    pub endpoint: Arc<dyn Endpoint>,
    pub client: ClientInfo,
    /// `false` keeps the environment [`Available`](ConnectionStatus::Available) until
    /// [`connect`](Environment::connect).
    pub enabled: bool,
}

impl EnvironmentOptions {
    /// Options for a catalog entry (id, label, enabled flag) with its endpoint, e.g. from
    /// [`saved_bearer_endpoint`](crate::auth::saved_bearer_endpoint).
    pub fn from_saved(saved: &crate::store::SavedEnvironment, endpoint: Arc<dyn Endpoint>) -> Self {
        EnvironmentOptions {
            enabled: saved.enabled && saved.unsupported_reason.is_none(),
            ..Self::new(saved.environment_id.clone(), saved.label.clone(), endpoint)
        }
    }

    pub fn new(id: EnvironmentId, label: impl Into<String>, endpoint: Arc<dyn Endpoint>) -> Self {
        EnvironmentOptions {
            id,
            label: label.into(),
            endpoint,
            client: ClientInfo::default(),
            enabled: true,
        }
    }
}

/// One live connection. Replaced on every reconnect (`generation` increments).
#[derive(Clone, Debug)]
pub struct Session {
    pub generation: u64,
    pub rpc: RpcConnection,
    /// Authenticated HTTP client for snapshots, uploads, and resolving asset URLs.
    pub http: EnvironmentHttp,
    pub descriptor: ExecutionEnvironmentDescriptor,
}

impl Session {
    fn is_open(&self) -> bool {
        self.rpc.close_reason().is_none()
    }
}

#[derive(Debug, Clone, Copy)]
struct Intent {
    desired: bool,
    online: bool,
    reset_retry: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signal {
    /// The intent changed (connect, disconnect, network).
    Wake,
    /// The user asked to reconnect now: reset the ladder, interrupt waits.
    RetryNow,
    /// The app became active: probe a live connection, reset the ladder otherwise.
    AppActive,
}

struct Shared {
    id: EnvironmentId,
    label: String,
    endpoint: Arc<dyn Endpoint>,
    client: ClientInfo,
    intent: Mutex<Intent>,
    status: watch::Sender<ConnectionStatus>,
    session: watch::Sender<Option<Session>>,
    config: watch::Sender<Option<Arc<ServerConfig>>>,
    shell: watch::Sender<Arc<ShellState>>,
}

impl Shared {
    fn set_status(&self, status: ConnectionStatus) {
        self.status.send_replace(status);
    }

    fn intent(&self) -> Intent {
        *self.intent.lock()
    }
}

struct Inner {
    shared: Arc<Shared>,
    signals: mpsc::UnboundedSender<Signal>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
        if let Some(session) = self.shared.session.send_replace(None) {
            session.rpc.close();
        }
    }
}

/// Handle to one environment. Cheap to clone; the connection closes when the last clone drops.
#[derive(Clone)]
pub struct Environment {
    inner: Arc<Inner>,
}

impl Environment {
    /// Starts supervising an environment and syncing its shell. Callable from any thread.
    pub fn start(options: EnvironmentOptions) -> Environment {
        let (signals, signal_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            id: options.id,
            label: options.label,
            endpoint: options.endpoint,
            client: options.client,
            intent: Mutex::new(Intent {
                desired: options.enabled,
                online: true,
                reset_retry: false,
            }),
            status: watch::channel(ConnectionStatus::Available).0,
            session: watch::channel(None).0,
            config: watch::channel(None).0,
            shell: watch::channel(Arc::new(ShellState::default())).0,
        });
        let tasks = vec![
            runtime().spawn(supervise(shared.clone(), signal_rx)),
            runtime().spawn(follow_shell(shared.clone())),
        ];
        Environment {
            inner: Arc::new(Inner {
                shared,
                signals,
                tasks,
            }),
        }
    }

    pub fn id(&self) -> &EnvironmentId {
        &self.inner.shared.id
    }

    pub fn label(&self) -> &str {
        &self.inner.shared.label
    }

    /// The base URL to show in the UI (none for relay environments).
    pub fn display_url(&self) -> Option<url::Url> {
        self.inner.shared.endpoint.display_url()
    }

    /// Connection status. `borrow()` for the current value, `changed().await` for updates.
    pub fn status(&self) -> watch::Receiver<ConnectionStatus> {
        self.inner.shared.status.subscribe()
    }

    /// Server config; `None` until the first connection. Kept current by `subscribeServerConfig`.
    pub fn config(&self) -> watch::Receiver<Option<Arc<ServerConfig>>> {
        self.inner.shared.config.subscribe()
    }

    /// Projects and active threads. Survives reconnects (resumes from its cursor).
    pub fn shell(&self) -> watch::Receiver<Arc<ShellState>> {
        self.inner.shared.shell.subscribe()
    }

    /// Session changes: `Some` on every new connection, `None` when it drops. App-owned
    /// streams (terminal attach, VCS status) resubscribe on each new `Some`.
    pub fn sessions(&self) -> watch::Receiver<Option<Session>> {
        self.inner.shared.session.subscribe()
    }

    /// The live session, if connected.
    pub fn session(&self) -> Option<Session> {
        self.inner
            .shared
            .session
            .borrow()
            .clone()
            .filter(Session::is_open)
    }

    /// Resolves once connected, or with the failure if the connection is blocked.
    pub async fn wait_connected(&self) -> Result<Session, ConnectionFailure> {
        let mut status = self.status();
        let mut sessions = self.inner.shared.session.subscribe();
        loop {
            // Mark both seen together so a change to either after this point wakes the select.
            let session = sessions.borrow_and_update().clone().filter(Session::is_open);
            let current = status.borrow_and_update().clone();
            match (session, current) {
                (Some(session), ConnectionStatus::Connected { .. }) => return Ok(session),
                (_, ConnectionStatus::Blocked { failure }) => return Err(failure),
                _ => {}
            }
            tokio::select! {
                changed = status.changed() => changed.map_err(|_| closed_failure())?,
                changed = sessions.changed() => changed.map_err(|_| closed_failure())?,
            }
        }
    }

    /// Subscribes to one thread's detail. Dropping the handle unsubscribes.
    pub fn open_thread(&self, thread_id: ThreadId) -> ThreadHandle {
        let (state_tx, state) = watch::channel(Arc::new(ThreadState::new(thread_id.clone())));
        let (commands, command_rx) = mpsc::unbounded_channel();
        let task = runtime().spawn(follow_thread(
            self.inner.shared.clone(),
            thread_id,
            state_tx,
            command_rx,
        ));
        ThreadHandle {
            state,
            commands,
            task,
        }
    }

    /// Sends a unary RPC on the current session.
    pub async fn request<M: Unary>(
        &self,
        payload: &M::Payload,
    ) -> Result<M::Success, RpcError<M::Error>> {
        let session = self.session().ok_or(RpcError::Disconnected(CloseReason::NotConnected))?;
        session.rpc.request::<M>(payload).await
    }

    /// Starts a stream RPC on the current session. It ends when the session does; resubscribe
    /// after the next [`status`](Self::status) change to `Connected`.
    pub fn subscribe<M: Stream>(
        &self,
        payload: &M::Payload,
    ) -> Result<Subscription<M>, RpcError<M::Error>> {
        let session = self.session().ok_or(RpcError::Disconnected(CloseReason::NotConnected))?;
        Ok(session.rpc.subscribe::<M>(payload))
    }

    /// Dispatches an orchestration command (build one with [`crate::commands`]). The resulting
    /// events arrive on the shell and thread streams, before or after this resolves.
    pub async fn dispatch(
        &self,
        command: ClientCommand,
    ) -> Result<DispatchResult, RpcError<ServerError>> {
        self.request::<DispatchCommand>(&command).await
    }

    /// Uploads a file for a new message: `attachments.createUploadUrl`, then the raw bytes over
    /// HTTP. Put the result in `TurnStartMessage.attachments`. Needs
    /// `capabilities.attachment_uploads`; images can be sent inline (`data_url`) instead.
    pub async fn upload_attachment(
        &self,
        kind: AttachmentKind,
        name: impl Into<String>,
        mime_type: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<TurnAttachment, UploadError> {
        let (name, mime_type) = (name.into(), mime_type.into());
        let size_bytes = bytes.len() as u64;
        let session = self
            .session()
            .ok_or(UploadError::Rpc(RpcError::Disconnected(CloseReason::NotConnected)))?;
        let upload = session
            .rpc
            .request::<AttachmentsCreateUploadUrl>(&AttachmentCreateUploadUrlInput {
                kind: kind.clone(),
                name: name.clone(),
                mime_type: mime_type.clone(),
                size_bytes,
            })
            .await?;
        session
            .http
            .upload_attachment(&upload.relative_url, &mime_type, bytes)
            .await?;
        Ok(TurnAttachment {
            kind,
            id: Some(upload.attachment_id),
            name,
            mime_type,
            size_bytes,
            data_url: None,
        })
    }

    /// Wants a connection (the "Connect" button). No-op if already connecting or connected.
    pub fn connect(&self) {
        self.update_intent(|intent| intent.desired = true);
    }

    /// Stops connecting and closes the session. Data stays cached.
    pub fn disconnect(&self) {
        self.update_intent(|intent| intent.desired = false);
    }

    /// Reconnects now, skipping any backoff wait ("Reconnect" button).
    pub fn retry_now(&self) {
        self.inner.shared.intent.lock().reset_retry = true;
        let _ = self.inner.signals.send(Signal::RetryNow);
    }

    /// The app became active or woke from sleep: probes a live connection (reconnecting at
    /// once if it is dead) or resets the retry ladder.
    pub fn app_active(&self) {
        let _ = self.inner.signals.send(Signal::AppActive);
    }

    /// Network reachability from the platform. Offline pauses connection attempts.
    pub fn set_online(&self, online: bool) {
        self.update_intent(|intent| intent.online = online);
    }

    fn update_intent(&self, change: impl FnOnce(&mut Intent)) {
        let changed = {
            let mut intent = self.inner.shared.intent.lock();
            let before = *intent;
            change(&mut intent);
            before.desired != intent.desired || before.online != intent.online
        };
        if changed {
            let _ = self.inner.signals.send(Signal::Wake);
        }
    }
}

/// Why [`Environment::upload_attachment`] failed.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error(transparent)]
    Rpc(#[from] RpcError<ServerError>),
    #[error(transparent)]
    Http(#[from] HttpError),
}

fn closed_failure() -> ConnectionFailure {
    ConnectionFailure::transient(TransientReason::Transport, "The environment was closed.")
}

/// A subscription to one thread's detail. Dropping it unsubscribes (sends `Interrupt`).
pub struct ThreadHandle {
    state: watch::Receiver<Arc<ThreadState>>,
    commands: mpsc::UnboundedSender<ThreadCommand>,
    task: JoinHandle<()>,
}

enum ThreadCommand {
    LoadOlder,
}

impl ThreadHandle {
    /// The thread's state. Survives reconnects (resumes from its cursor).
    pub fn state(&self) -> watch::Receiver<Arc<ThreadState>> {
        self.state.clone()
    }

    /// Fetches the next older page of turns when `state.page.has_more`. Progress shows as
    /// `page.loading_older`.
    pub fn load_older(&self) {
        let _ = self.commands.send(ThreadCommand::LoadOlder);
    }
}

impl Drop for ThreadHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

// ---------------------------------------------------------------------------------------------
// Supervisor

enum Attempt {
    /// Ended by intent or a retry request, not by a failure.
    Interrupted {
        established: bool,
        stable: bool,
        reset: bool,
    },
    Failed {
        failure: ConnectionFailure,
        established: bool,
        stable: bool,
        /// An app-active probe found the connection dead: reconnect without waiting.
        probe_failed: bool,
    },
}

async fn supervise(shared: Arc<Shared>, mut signals: mpsc::UnboundedReceiver<Signal>) {
    let mut failures: u32 = 0;
    let mut generation: u64 = 0;
    let mut last_failure: Option<ConnectionFailure> = None;
    loop {
        let intent = {
            let mut intent = shared.intent.lock();
            let current = *intent;
            intent.reset_retry = false;
            current
        };
        if intent.reset_retry {
            failures = 0;
            last_failure = None;
        }
        if !intent.desired {
            failures = 0;
            last_failure = None;
            shared.set_status(ConnectionStatus::Available);
            next_signal(&mut signals).await;
            continue;
        }
        if !intent.online {
            shared.set_status(ConnectionStatus::Offline);
            if next_signal(&mut signals).await == Signal::AppActive {
                failures = 0;
            }
            continue;
        }

        let attempt = failures + 1;
        let outcome = run_attempt(
            &shared,
            &mut signals,
            attempt,
            generation + 1,
            last_failure.clone(),
        )
        .await;
        match outcome {
            Attempt::Interrupted {
                established,
                stable,
                reset,
            } => {
                if established {
                    generation += 1;
                }
                if stable || reset {
                    failures = 0;
                    last_failure = None;
                }
            }
            Attempt::Failed {
                failure,
                established,
                stable,
                probe_failed,
            } => {
                if established {
                    generation += 1;
                }
                if stable {
                    failures = 0;
                }
                tracing::info!(environment = %shared.id, %failure, "connection attempt failed");
                last_failure = Some(failure.clone());
                if failure.is_blocked() {
                    shared.set_status(ConnectionStatus::Blocked { failure });
                    if next_signal(&mut signals).await == Signal::AppActive {
                        failures = 0;
                    }
                    continue;
                }
                if probe_failed {
                    failures = 0;
                    continue;
                }
                failures += 1;
                let delay = RETRY_DELAYS[(failures as usize - 1).min(RETRY_DELAYS.len() - 1)];
                shared.set_status(ConnectionStatus::Reconnecting {
                    attempt,
                    retry_at: Instant::now() + delay,
                    failure,
                });
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    signal = next_signal(&mut signals) => {
                        if signal == Signal::AppActive {
                            failures = 0;
                        }
                    }
                }
            }
        }
    }
}

/// Waits for the next signal. Never returns once the environment is gone (the task is aborted).
async fn next_signal(signals: &mut mpsc::UnboundedReceiver<Signal>) -> Signal {
    match signals.recv().await {
        Some(signal) => signal,
        None => std::future::pending().await,
    }
}

/// Waits for a signal that should abort the current attempt: intent no longer wants a
/// connection, or a retry request. Returns whether the ladder should reset.
async fn interrupting_signal(
    shared: &Shared,
    signals: &mut mpsc::UnboundedReceiver<Signal>,
) -> bool {
    loop {
        match next_signal(signals).await {
            Signal::RetryNow => return true,
            Signal::Wake => {
                let intent = shared.intent();
                if !intent.desired || !intent.online {
                    return false;
                }
            }
            Signal::AppActive => {}
        }
    }
}

async fn run_attempt(
    shared: &Arc<Shared>,
    signals: &mut mpsc::UnboundedReceiver<Signal>,
    attempt: u32,
    generation: u64,
    last_failure: Option<ConnectionFailure>,
) -> Attempt {
    let establish = async {
        match tokio::time::timeout(
            ESTABLISH_TIMEOUT,
            establish(shared, generation, attempt, last_failure),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(ConnectionFailure::transient(
                TransientReason::Timeout,
                format!("{} did not respond during connection setup.", shared.label),
            )),
        }
    };
    let result = tokio::select! {
        result = establish => result,
        reset = interrupting_signal(shared, signals) => {
            return Attempt::Interrupted { established: false, stable: false, reset };
        }
    };
    let session = match result {
        Ok(session) => session,
        Err(failure) => {
            return Attempt::Failed {
                failure,
                established: false,
                stable: false,
                probe_failed: false,
            };
        }
    };

    let connected_at = Instant::now();
    shared.session.send_replace(Some(session.clone()));
    shared.set_status(ConnectionStatus::Connected { generation });
    tracing::info!(environment = %shared.id, generation, "connected");

    enum End {
        Closed(CloseReason),
        Interrupted { reset: bool },
        ProbeFailed(String),
    }
    let end = loop {
        tokio::select! {
            reason = session.rpc.closed() => break End::Closed(reason),
            signal = next_signal(signals) => match signal {
                Signal::RetryNow => break End::Interrupted { reset: true },
                Signal::Wake => {
                    let intent = shared.intent();
                    if !intent.desired || !intent.online {
                        break End::Interrupted { reset: false };
                    }
                }
                Signal::AppActive => {
                    if let Err(error) = probe(&session).await {
                        break End::ProbeFailed(error);
                    }
                }
            },
        }
    };

    session.rpc.close();
    shared.session.send_replace(None);
    let stable = connected_at.elapsed() >= STABLE_AFTER;
    match end {
        End::Closed(reason) => Attempt::Failed {
            failure: ConnectionFailure::transient(TransientReason::Transport, reason.to_string()),
            established: true,
            stable,
            probe_failed: false,
        },
        End::Interrupted { reset } => Attempt::Interrupted {
            established: true,
            stable,
            reset,
        },
        End::ProbeFailed(error) => Attempt::Failed {
            failure: ConnectionFailure::transient(TransientReason::Timeout, error),
            established: true,
            stable,
            probe_failed: true,
        },
    }
}

/// `server.probe` when the server supports it, else `server.getConfig` (15 s).
async fn probe(session: &Session) -> Result<(), String> {
    let request = async {
        if session.descriptor.capabilities.connection_probe {
            session.rpc.request::<ServerProbe>(&Empty {}).await.map(drop)
        } else {
            session.rpc.request::<ServerGetConfig>(&Empty {}).await.map(drop)
        }
    };
    match tokio::time::timeout(PROBE_TIMEOUT, request).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("The environment stopped responding.".into()),
    }
}

/// One attempt: prepare (descriptor, ticket), open the socket, wait for the config snapshot.
async fn establish(
    shared: &Arc<Shared>,
    generation: u64,
    attempt: u32,
    last_failure: Option<ConnectionFailure>,
) -> Result<Session, ConnectionFailure> {
    let stage = |stage| {
        shared.set_status(ConnectionStatus::Connecting {
            attempt,
            stage,
            last_failure: last_failure.clone(),
        })
    };
    stage(ConnectStage::Preparing);
    let prepared = shared
        .endpoint
        .prepare(shared.id.clone(), shared.client.clone())
        .await?;

    stage(ConnectStage::Opening);
    let rpc = RpcConnection::connect(prepared.ws_url.to_string())
        .await
        .map_err(connect_failure)?;

    stage(ConnectStage::Synchronizing);
    let mut config_stream =
        rpc.subscribe::<SubscribeServerConfig>(&SubscribeServerConfigInput::default());
    let config = match config_stream.next().await {
        Some(Ok(ServerConfigStreamEvent::Snapshot { config })) => *config,
        Some(Ok(_)) => {
            return Err(ConnectionFailure::transient(
                TransientReason::Transport,
                "The server did not send its configuration.",
            ));
        }
        Some(Err(error)) => {
            return Err(ConnectionFailure::transient(
                TransientReason::Transport,
                error.to_string(),
            ));
        }
        None => {
            return Err(ConnectionFailure::transient(
                TransientReason::Transport,
                "The server closed the configuration stream.",
            ));
        }
    };
    if config.environment.environment_id != shared.id {
        return Err(ConnectionFailure::blocked(
            BlockedReason::Configuration,
            format!(
                "Connected environment {} does not match {}.",
                config.environment.environment_id, shared.id
            ),
        ));
    }
    shared.config.send_replace(Some(Arc::new(config.clone())));
    runtime().spawn(follow_config(
        shared.clone(),
        rpc.clone(),
        config,
        config_stream,
    ));
    Ok(Session {
        generation,
        rpc,
        http: prepared.http,
        descriptor: prepared.descriptor,
    })
}

fn connect_failure(error: ConnectError) -> ConnectionFailure {
    match error {
        ConnectError::Timeout => ConnectionFailure::transient(
            TransientReason::Timeout,
            "Timed out opening the connection.",
        ),
        ConnectError::Transport(message) => {
            ConnectionFailure::transient(TransientReason::Network, message)
        }
        ConnectError::InvalidRequest(message) => {
            ConnectionFailure::blocked(BlockedReason::Configuration, message)
        }
        ConnectError::Rejected { status, body } => {
            let body = body.unwrap_or_default();
            HttpError::Status {
                status,
                error: serde_json::from_str(&body).ok().map(Box::new),
                body,
            }
            .connection_failure()
        }
    }
}

/// Applies config events for the rest of the session; resubscribes if a defect kills it.
async fn follow_config(
    shared: Arc<Shared>,
    rpc: RpcConnection,
    mut config: ServerConfig,
    mut stream: Subscription<SubscribeServerConfig>,
) {
    loop {
        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    if config.apply(event) {
                        shared.config.send_replace(Some(Arc::new(config.clone())));
                    }
                }
                Err(RpcError::Decode(error)) => {
                    tracing::warn!(%error, "skipping undecodable server config event");
                }
                Err(error) => {
                    tracing::debug!(%error, "server config stream ended");
                    break;
                }
            }
        }
        if rpc.close_reason().is_some() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
        stream = rpc.subscribe::<SubscribeServerConfig>(&SubscribeServerConfigInput::default());
    }
}

// ---------------------------------------------------------------------------------------------
// Shell and thread sync

/// Waits for an open session. `None` when the environment is gone.
async fn next_session(sessions: &mut watch::Receiver<Option<Session>>) -> Option<Session> {
    loop {
        if let Some(session) = sessions.borrow_and_update().clone().filter(Session::is_open) {
            return Some(session);
        }
        sessions.changed().await.ok()?;
    }
}

/// Resubscribe delay after a stream error on a live session: 250 ms doubling to 8 s, reset
/// whenever data arrives.
struct RetryDelay(Duration);

impl RetryDelay {
    const FIRST: Duration = Duration::from_millis(250);
    const MAX: Duration = Duration::from_secs(8);

    fn new() -> Self {
        RetryDelay(Self::FIRST)
    }

    fn reset(&mut self) {
        self.0 = Self::FIRST;
    }

    /// Sleeps for the current delay, or until the session changes.
    async fn wait(&mut self, sessions: &mut watch::Receiver<Option<Session>>) {
        let delay = self.0;
        self.0 = (self.0 * 2).min(Self::MAX);
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = sessions.changed() => {}
        }
    }
}

/// Drains everything a subscription has ready into `apply`. Returns `Err` with the terminal
/// error, or `Ok(changed)`. Undecodable items are logged and skipped.
fn drain_ready<M: Stream>(
    stream: &mut Subscription<M>,
    first: StreamNext<M>,
    mut apply: impl FnMut(M::Item) -> bool,
) -> Result<bool, Option<RpcError<M::Error>>> {
    let mut changed = false;
    let mut next = Some(first);
    while let Some(item) = next {
        match item {
            None => return Err(None),
            Some(Ok(item)) => changed |= apply(item),
            Some(Err(RpcError::Decode(error))) => {
                tracing::warn!(method = M::TAG, %error, "skipping undecodable stream item");
            }
            Some(Err(error)) => return Err(Some(error)),
        }
        next = stream.try_next();
    }
    Ok(changed)
}

async fn follow_shell(shared: Arc<Shared>) {
    let mut sessions = shared.session.subscribe();
    let mut state = ShellState::default();
    let mut retry = RetryDelay::new();
    let publish = |state: &ShellState| {
        shared.shell.send_replace(Arc::new(state.clone()));
    };
    loop {
        let Some(session) = next_session(&mut sessions).await else {
            return;
        };
        let marker = shared
            .config
            .borrow()
            .as_ref()
            .is_some_and(|c| c.shell_resume_completion_marker);
        if !state.has_data() {
            match session.http.shell_snapshot().await {
                Ok(snapshot) => {
                    state.apply_snapshot(snapshot);
                    publish(&state);
                }
                Err(error) => tracing::debug!(%error, "shell snapshot over http failed"),
            }
        }
        let mut stream = session.rpc.subscribe::<SubscribeShell>(&SubscribeShellInput {
            after_sequence: state.resume_cursor(),
            request_completion_marker: marker.then_some(true),
        });
        state.begin_sync(marker);
        publish(&state);
        let error = loop {
            let first = stream.next().await;
            match drain_ready(&mut stream, first, |item| state.apply(item)) {
                Ok(changed) => {
                    retry.reset();
                    if changed {
                        publish(&state);
                    }
                }
                Err(error) => break error,
            }
        };
        state.end_sync();
        publish(&state);
        if let Some(error) = error {
            tracing::debug!(%error, "shell stream ended");
        }
        if session.is_open() {
            retry.wait(&mut sessions).await;
        }
    }
}

async fn follow_thread(
    shared: Arc<Shared>,
    thread_id: ThreadId,
    state_tx: watch::Sender<Arc<ThreadState>>,
    mut commands: mpsc::UnboundedReceiver<ThreadCommand>,
) {
    let mut sessions = shared.session.subscribe();
    let mut state = ThreadState::new(thread_id.clone());
    let mut retry = RetryDelay::new();
    let (older_tx, mut older_rx) = mpsc::unbounded_channel();
    let publish = |state: &ThreadState| {
        state_tx.send_replace(Arc::new(state.clone()));
    };
    loop {
        let Some(session) = next_session(&mut sessions).await else {
            return;
        };
        let config = shared.config.borrow().clone();
        let flag = |get: fn(&ServerConfig) -> bool| config.as_deref().is_some_and(get);
        let marker = flag(|c| c.thread_resume_completion_marker);
        let reasoning = flag(|c| c.reasoning_messages);
        let pagination = flag(|c| c.thread_snapshot_pagination);

        if state.thread.is_none() {
            let query = ThreadSnapshotQuery {
                turn_limit: pagination.then_some(INITIAL_TURN_LIMIT),
                reasoning_messages: reasoning,
                before_cursor: None,
            };
            match session.http.thread_snapshot(&thread_id, &query).await {
                Ok(Some(snapshot)) => {
                    state.apply_snapshot(snapshot);
                    publish(&state);
                }
                // Not found or failed: the socket subscription decides.
                Ok(None) => {}
                Err(error) => tracing::debug!(%error, "thread snapshot over http failed"),
            }
        }

        let mut stream = session.rpc.subscribe::<SubscribeThread>(&SubscribeThreadInput {
            thread_id: thread_id.clone(),
            reasoning_messages: reasoning.then_some(true),
            after_sequence: state.resume_cursor(),
            request_completion_marker: marker.then_some(true),
            turn_limit: pagination.then_some(INITIAL_TURN_LIMIT),
        });
        state.begin_sync(marker);
        publish(&state);

        let error = loop {
            tokio::select! {
                first = stream.next() => {
                    match drain_ready(&mut stream, first, |item| state.apply(item)) {
                        Ok(changed) => {
                            retry.reset();
                            if changed {
                                state.error = None;
                                publish(&state);
                            }
                        }
                        Err(error) => break error,
                    }
                }
                Some(ThreadCommand::LoadOlder) = commands.recv() => {
                    let Some(page) = state.page.as_mut().filter(|p| p.has_more && !p.loading_older) else {
                        continue;
                    };
                    page.loading_older = true;
                    let query = ThreadSnapshotQuery {
                        turn_limit: Some(OLDER_TURN_LIMIT),
                        reasoning_messages: reasoning,
                        before_cursor: page.before_cursor.clone(),
                    };
                    publish(&state);
                    let (http, id, epoch, tx) =
                        (session.http.clone(), thread_id.clone(), state.history_epoch, older_tx.clone());
                    runtime().spawn(async move {
                        let _ = tx.send((epoch, http.thread_snapshot(&id, &query).await));
                    });
                }
                Some((epoch, result)) = older_rx.recv() => {
                    match result {
                        Ok(Some(snapshot)) => {
                            state.merge_older_page(epoch, snapshot);
                        }
                        Ok(None) | Err(_) => {
                            if let Some(page) = &mut state.page {
                                page.loading_older = false;
                            }
                        }
                    }
                    publish(&state);
                }
            }
        };
        let message = error.as_ref().map(error_message);
        if let Some(message) = &message {
            tracing::debug!(thread = %thread_id, %message, "thread stream ended");
        }
        state.end_sync(message);
        publish(&state);
        if session.is_open() {
            retry.wait(&mut sessions).await;
        }
    }
}

fn error_message(error: &RpcError<ServerError>) -> String {
    match error {
        RpcError::Failed(error) => error.display_message().to_owned(),
        other => other.to_string(),
    }
}
