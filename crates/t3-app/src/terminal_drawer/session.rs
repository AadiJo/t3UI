//! One terminal session: a `TerminalView` wired to `terminal.attach/write/resize`
//! (panels.md 3.6-3.7, web `TerminalViewport` and `terminalSession.ts`).
//!
//! The attach stream replays the server's history into the view and then streams output. When
//! the shell exits (or the server closes the session) the session prints the fork's
//! `[terminal] Process exited` line and asks the store to close the tab on the next tick.

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Subscription, Task, Window};
use t3_logic::ThreadRef;
use t3_protocol::{
    methods::{TerminalAttach, TerminalResize, TerminalWrite},
    terminal::{
        TerminalAttachInput, TerminalEvent as WireEvent, TerminalResizeInput,
        TerminalSessionSnapshot, TerminalStatus, TerminalWriteInput,
    },
};
use t3_terminal::{TerminalEvent, TerminalView};

use super::{RETRY_DELAY, ThreadWorkspace, environment_client, rpc_error_text};

/// The fork keeps at most the last 512 KB of a session's buffer on the client.
const MAX_BUFFER_BYTES: usize = 512 * 1024;

/// Lifecycle as the drawer sees it (`terminalSession.ts` status).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    /// Waiting for the attach snapshot.
    Attaching,
    Running,
    Exited,
    Closed,
    Error,
}

/// What a session asks its owner to do.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// The shell exited or the server closed the session: close the tab.
    Ended,
    /// A Cmd+clicked link (URL or path), already resolved against the session's cwd for paths.
    OpenLink { url: Option<String>, path: Option<String> },
    /// A finished selection asks for the "Add to chat" menu at `position` (window coordinates).
    SelectionMenu(super::TerminalContextSelection, gpui_kit::Point<gpui_kit::Pixels>),
}

/// One `(thread, terminalId)` session and its emulator view.
pub struct TerminalSession {
    thread: ThreadRef,
    terminal_id: String,
    cwd: String,
    view: Entity<TerminalView>,
    status: SessionStatus,
    /// The size the view last fit to; re-sent after each attach snapshot so a resize that
    /// raced the session's creation is not lost.
    fitted: Option<(u16, u16)>,
    /// Latest size not yet sent; the in-flight resize task picks it up (latest wins).
    pending_resize: Option<(u16, u16)>,
    resize_task: Option<Task<()>>,
    _attach: Task<()>,
    _view_events: Subscription,
}

impl EventEmitter<SessionEvent> for TerminalSession {}

impl TerminalSession {
    /// Creates the view and starts attaching. `terminal.attach` opens the session on the
    /// server if it does not exist yet, at the workspace's cwd with the project env.
    pub fn new(
        workspace: &ThreadWorkspace,
        terminal_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let view = cx.new(|cx| TerminalView::new(window, cx));
        let view_events = cx.subscribe(&view, Self::on_view_event);
        let input = TerminalAttachInput {
            thread_id: workspace.thread.thread_id.clone(),
            terminal_id: terminal_id.as_str().into(),
            cwd: Some(workspace.cwd().to_owned()),
            worktree_path: workspace.worktree_path.clone(),
            cols: None,
            rows: None,
            env: Some(workspace.runtime_env()),
            provider_instance_id: None,
            restart_if_not_running: None,
        };
        let attach = Self::attach(workspace.thread.clone(), input, cx);
        Self {
            thread: workspace.thread.clone(),
            terminal_id,
            cwd: workspace.cwd().to_owned(),
            view,
            status: SessionStatus::Attaching,
            fitted: None,
            pending_resize: None,
            resize_task: None,
            _attach: attach,
            _view_events: view_events,
        }
    }

    pub fn view(&self) -> &Entity<TerminalView> {
        &self.view
    }

    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }

    pub fn status(&self) -> SessionStatus {
        self.status
    }

    /// Follows `terminal.attach` across reconnects until the session ends.
    fn attach(thread: ThreadRef, input: TerminalAttachInput, cx: &mut Context<Self>) -> Task<()> {
        let client = environment_client(&thread.environment_id, cx);
        cx.spawn(async move |this, cx| {
            let Some(client) = client else {
                this.update(cx, |this, cx| {
                    this.fail("Environment is not connected.", cx)
                })
                .ok();
                return;
            };
            let mut status = client.status();
            loop {
                while !status.borrow_and_update().is_connected() {
                    if status.changed().await.is_err() {
                        return;
                    }
                }
                match client.subscribe::<TerminalAttach>(&input) {
                    Ok(mut subscription) => {
                        while let Some(item) = subscription.next().await {
                            let ended = this.update(cx, |this, cx| match item {
                                Ok(event) => this.apply(event, cx),
                                Err(error) => {
                                    this.fail(&rpc_error_text(&error), cx);
                                    false
                                }
                            });
                            match ended {
                                Ok(false) => {}
                                // The session ended, or the entity is gone.
                                _ => return,
                            }
                        }
                    }
                    Err(error) => {
                        let message = rpc_error_text(&error);
                        if this.update(cx, |this, cx| this.fail(&message, cx)).is_err() {
                            return;
                        }
                    }
                }
                cx.background_executor().timer(RETRY_DELAY).await;
            }
        })
    }

    /// Applies one attach event (web `terminalSession.ts` reducer plus the viewport's
    /// status effects). Returns true once the session ended.
    fn apply(&mut self, event: WireEvent, cx: &mut Context<Self>) -> bool {
        match event {
            WireEvent::Snapshot { snapshot } | WireEvent::Restarted { snapshot, .. } => {
                self.apply_snapshot(&snapshot, cx)
            }
            WireEvent::Output { data, .. } => {
                if matches!(self.status, SessionStatus::Closed | SessionStatus::Attaching) {
                    self.status = SessionStatus::Running;
                }
                self.view
                    .update(cx, |view, cx| view.feed_output(&data, cx));
                false
            }
            WireEvent::Cleared { .. } => {
                self.view.update(cx, |view, cx| view.reset(cx));
                false
            }
            WireEvent::Exited { .. } => self.end(SessionStatus::Exited, cx),
            WireEvent::Closed { .. } => self.end(SessionStatus::Closed, cx),
            WireEvent::Error { message, .. } => {
                self.fail(&message, cx);
                false
            }
            WireEvent::Started { .. } | WireEvent::Activity { .. } | WireEvent::Unknown => false,
        }
    }

    fn apply_snapshot(&mut self, snapshot: &TerminalSessionSnapshot, cx: &mut Context<Self>) -> bool {
        let history = trim_to_last_bytes(&snapshot.history, MAX_BUFFER_BYTES);
        self.view
            .update(cx, |view, cx| view.feed_snapshot(history, cx));
        match snapshot.status {
            TerminalStatus::Exited => return self.end(SessionStatus::Exited, cx),
            TerminalStatus::Error => self.status = SessionStatus::Error,
            _ => self.status = SessionStatus::Running,
        }
        if let Some(size) = self.fitted {
            self.queue_resize(size, cx);
        }
        cx.notify();
        false
    }

    /// Prints the fork's exit line and asks the owner to close the tab.
    fn end(&mut self, status: SessionStatus, cx: &mut Context<Self>) -> bool {
        if matches!(self.status, SessionStatus::Exited | SessionStatus::Closed) {
            return true;
        }
        self.status = status;
        let message = if status == SessionStatus::Closed {
            "Terminal closed"
        } else {
            "Process exited"
        };
        self.view
            .update(cx, |view, cx| view.write_system_message(message, cx));
        // "Close the terminal on the next tick", after the message rendered.
        cx.spawn(async move |this, cx| {
            this.update(cx, |_, cx| cx.emit(SessionEvent::Ended)).ok();
        })
        .detach();
        cx.notify();
        true
    }

    fn fail(&mut self, message: &str, cx: &mut Context<Self>) {
        self.status = SessionStatus::Error;
        self.view
            .update(cx, |view, cx| view.write_system_message(message, cx));
        cx.notify();
    }

    fn on_view_event(
        &mut self,
        _: Entity<TerminalView>,
        event: &TerminalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalEvent::Input(data) => self.write(data.clone(), cx),
            TerminalEvent::Resize { cols, rows } => {
                self.fitted = Some((*cols, *rows));
                self.queue_resize((*cols, *rows), cx);
            }
            TerminalEvent::LinkActivated { kind, text, .. } => {
                let event = match kind {
                    t3_terminal::TerminalLinkKind::Url => SessionEvent::OpenLink {
                        url: Some(text.clone()),
                        path: None,
                    },
                    _ => SessionEvent::OpenLink {
                        url: None,
                        path: Some(t3_terminal::resolve_path_link_target(text, &self.cwd)),
                    },
                };
                cx.emit(event);
            }
            TerminalEvent::SelectionMenuRequested {
                text,
                line_start,
                line_end,
                position,
            } => {
                let selection = super::TerminalContextSelection {
                    thread: self.thread.clone(),
                    terminal_id: self.terminal_id.clone(),
                    terminal_label: String::new(),
                    line_start: *line_start,
                    line_end: *line_end,
                    text: normalize_selection(text),
                };
                if !selection.text.is_empty() {
                    cx.emit(SessionEvent::SelectionMenu(selection, *position));
                }
            }
        }
    }

    /// `terminal.write`; failures print `[terminal] {message}` like the fork.
    pub fn write(&mut self, data: String, cx: &mut Context<Self>) {
        if data.is_empty() {
            return;
        }
        let client = environment_client(&self.thread.environment_id, cx);
        let input = TerminalWriteInput {
            thread_id: self.thread.thread_id.clone(),
            terminal_id: self.terminal_id.as_str().into(),
            data,
        };
        cx.spawn(async move |this, cx| {
            let result = match client {
                Some(client) => client
                    .request::<TerminalWrite>(&input)
                    .await
                    .map_err(|error| rpc_error_text(&error)),
                None => Err("Terminal write failed".to_owned()),
            };
            if let Err(message) = result {
                this.update(cx, |this, cx| {
                    this.view
                        .update(cx, |view, cx| view.write_system_message(&message, cx))
                })
                .ok();
            }
        })
        .detach();
    }

    /// `terminal.resize`, coalesced to the latest size (`terminal.ts:68-73`).
    fn queue_resize(&mut self, size: (u16, u16), cx: &mut Context<Self>) {
        self.pending_resize = Some(size);
        if self.resize_task.is_some() || self.status == SessionStatus::Attaching {
            return;
        }
        let client = environment_client(&self.thread.environment_id, cx);
        let thread_id = self.thread.thread_id.clone();
        let terminal_id = self.terminal_id.clone();
        self.resize_task = Some(cx.spawn(async move |this, cx| {
            let Some(client) = client else {
                return;
            };
            while let Ok(Some((cols, rows))) =
                this.update(cx, |this, _| this.pending_resize.take())
            {
                let input = TerminalResizeInput {
                    thread_id: thread_id.clone(),
                    terminal_id: terminal_id.as_str().into(),
                    cols,
                    rows,
                };
                // A failed resize is retried by the next fit or attach snapshot.
                client.request::<TerminalResize>(&input).await.ok();
            }
            this.update(cx, |this, _| this.resize_task = None).ok();
        }));
    }
}

/// The selection text the fork attaches: CRLF normalized, leading and trailing newlines
/// trimmed.
fn normalize_selection(text: &str) -> String {
    text.replace("\r\n", "\n").trim_matches('\n').to_owned()
}

/// The last `max` bytes of `text`, starting on a character boundary.
fn trim_to_last_bytes(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Creates a session entity. Kept out of `impl` so the store can call it with its window.
pub(super) fn create(
    workspace: &ThreadWorkspace,
    terminal_id: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TerminalSession> {
    cx.new(|cx| TerminalSession::new(workspace, terminal_id.to_owned(), window, cx))
}
