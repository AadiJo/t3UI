//! The global terminal store: every thread's drawer layout (persisted to `terminal-state.json`,
//! the web's `t3code:terminal-state:v1`), the server's terminal metadata per environment
//! (labels, running subprocesses, which terminals exist), and the live sessions.
//!
//! Views read it in `render` and `cx.observe` it. Every action that creates a terminal also
//! sends `terminal.open`, like the fork; the drawer then attaches a session per visible
//! terminal.

use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use gpui_kit::{
    App, AppContext as _, Context, Entity, FocusHandle, Global, Subscription, Task, Window,
};
use t3_logic::{
    ThreadRef,
    terminal_layout::{
        DEFAULT_TERMINAL_ID, SplitDirection, TerminalLayouts, ThreadTerminalLayout,
        next_terminal_id, session_label,
    },
};
use t3_protocol::{
    EnvironmentId,
    methods::{Empty, SubscribeTerminalMetadata, TerminalClose, TerminalOpen, TerminalWrite},
    orchestration::ProjectScript,
    terminal::{
        TerminalCloseInput, TerminalMetadataEvent, TerminalOpenInput, TerminalSummary,
        TerminalWriteInput,
    },
};

use super::{
    RETRY_DELAY, ThreadWorkspace, environment_client, rpc_error_text,
    session::{self, SessionEvent, TerminalSession},
};
use crate::{keybindings::ShortcutScope, state::AppState};

const STATE_FILE: &str = "terminal-state.json";
/// Layout writes are debounced like the web's persisted stores.
const WRITE_DELAY: Duration = Duration::from_millis(500);
/// Sessions of this many recently shown threads stay attached while hidden
/// (`MAX_HIDDEN_MOUNTED_TERMINAL_THREADS`), so their scrollback survives thread switches.
const MAX_HIDDEN_THREADS: usize = 10;
/// Scripts run in a new terminal at the server's default spawn size.
const SCRIPT_TERMINAL_SIZE: (u16, u16) = (120, 30);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SessionKey {
    thread: ThreadRef,
    terminal_id: String,
}

#[derive(Default)]
struct EnvironmentTerminals {
    terminals: Vec<TerminalSummary>,
    _stream: Option<Task<()>>,
}

struct LiveSession {
    session: Entity<TerminalSession>,
    _events: Subscription,
}

/// What a session asked the drawer to show or do; drained by the visible drawer.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawerRequest {
    OpenLink {
        thread: ThreadRef,
        url: Option<String>,
        path: Option<String>,
    },
    SelectionMenu(
        super::TerminalContextSelection,
        gpui_kit::Point<gpui_kit::Pixels>,
    ),
}

/// The app's terminals. See the module docs.
pub struct TerminalStore {
    layouts: TerminalLayouts,
    sessions: HashMap<SessionKey, LiveSession>,
    environments: HashMap<EnvironmentId, EnvironmentTerminals>,
    /// Threads whose drawer was shown, most recent first.
    shown: Vec<ThreadRef>,
    focus: FocusHandle,
    focus_request: u64,
    requests: Vec<DrawerRequest>,
    pending_write: Option<Task<()>>,
}

struct GlobalTerminalStore(Entity<TerminalStore>);

impl Global for GlobalTerminalStore {}

impl TerminalStore {
    /// The global store, created (and its layouts loaded) on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(store) = cx.try_global::<GlobalTerminalStore>() {
            return store.0.clone();
        }
        let layouts = AppState::global(cx)
            .read(cx)
            .store()
            .read(STATE_FILE)
            .map(|json| TerminalLayouts::from_json(&json))
            .unwrap_or_default();
        let store = cx.new(|cx| {
            let focus = cx.focus_handle();
            ShortcutScope::update(cx, |scope| scope.register_terminal_focus(focus.clone()));
            cx.on_app_quit(|this: &mut Self, cx| {
                if this.pending_write.take().is_some() {
                    let store = AppState::global(cx).read(cx).store().clone();
                    store.write_now(STATE_FILE, &this.layouts.to_json());
                }
                async {}
            })
            .detach();
            Self {
                layouts,
                sessions: HashMap::new(),
                environments: HashMap::new(),
                shown: Vec::new(),
                focus,
                focus_request: 0,
                requests: Vec::new(),
                pending_write: None,
            }
        });
        cx.set_global(GlobalTerminalStore(store.clone()));
        store
    }

    /// A thread's drawer layout.
    pub fn layout(&self, thread: &ThreadRef) -> ThreadTerminalLayout {
        self.layouts.layout(&thread.key())
    }

    /// The handle the visible drawer tracks; focus inside it is `terminalFocus`.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Bumped whenever the active terminal should take focus (open, new, split, run script).
    pub fn focus_request(&self) -> u64 {
        self.focus_request
    }

    /// The tab label: the server's label (the running command) or `Terminal N`.
    pub fn label(&self, thread: &ThreadRef, terminal_id: &str) -> String {
        let server = self
            .summary(thread, terminal_id)
            .map(|summary| summary.label.as_str());
        session_label(terminal_id, server)
    }

    fn summary(&self, thread: &ThreadRef, terminal_id: &str) -> Option<&TerminalSummary> {
        self.environments
            .get(&thread.environment_id)?
            .terminals
            .iter()
            .find(|summary| {
                summary.thread_id == thread.thread_id && summary.terminal_id.as_str() == terminal_id
            })
    }

    /// Terminal ids the server knows for `thread`, in `term-N` numeric order.
    fn server_terminal_ids(&self, thread: &ThreadRef) -> Vec<String> {
        let mut ids: Vec<String> = self
            .environments
            .get(&thread.environment_id)
            .map(|env| {
                env.terminals
                    .iter()
                    .filter(|summary| summary.thread_id == thread.thread_id)
                    .map(|summary| summary.terminal_id.to_string())
                    .collect()
            })
            .unwrap_or_default();
        ids.sort_by(|a, b| natural_order(a).cmp(&natural_order(b)));
        ids
    }

    /// The lowest unused `term-N` among the layout's and the server's terminals.
    fn allocate_id(&self, thread: &ThreadRef) -> String {
        let layout = self.layout(thread);
        let server = self.server_terminal_ids(thread);
        next_terminal_id(
            layout
                .terminal_ids
                .iter()
                .chain(server.iter())
                .map(String::as_str),
        )
    }

    // -------------------------------------------------------------------------------------
    // Actions

    /// `terminal.toggle`: opening a drawer with no terminals creates one and opens it on the
    /// server.
    pub fn toggle(&mut self, workspace: &ThreadWorkspace, cx: &mut Context<Self>) {
        let key = workspace.thread.key();
        let layout = self.layouts.layout(&key);
        if layout.terminal_open {
            self.edit(cx, |layouts| layouts.set_open(&key, false));
            return;
        }
        if layout.terminal_ids.is_empty() {
            let id = self.allocate_id(&workspace.thread);
            self.edit(cx, |layouts| layouts.ensure(&key, &id, true, true));
            self.open_on_server(workspace, &id, None, cx).detach();
        } else {
            self.edit(cx, |layouts| layouts.set_open(&key, true));
        }
        self.request_focus(cx);
    }

    /// Opens or closes a thread's drawer without touching its terminals.
    pub fn set_open(&mut self, thread: &ThreadRef, open: bool, cx: &mut Context<Self>) {
        let key = thread.key();
        if open && self.layouts.layout(&key).terminal_ids.is_empty() {
            return;
        }
        self.edit(cx, |layouts| layouts.set_open(&key, open));
    }

    /// `terminal.new`: a terminal in a group of its own.
    pub fn new_terminal(&mut self, workspace: &ThreadWorkspace, cx: &mut Context<Self>) {
        let key = workspace.thread.key();
        let id = self.allocate_id(&workspace.thread);
        self.edit(cx, |layouts| layouts.new_terminal(&key, &id));
        self.open_on_server(workspace, &id, None, cx).detach();
        self.request_focus(cx);
    }

    /// `terminal.split` / `terminal.splitVertical`. A no-op once the group holds four.
    pub fn split(
        &mut self,
        workspace: &ThreadWorkspace,
        direction: SplitDirection,
        cx: &mut Context<Self>,
    ) {
        let key = workspace.thread.key();
        if self.layouts.layout(&key).split_limit_reached() {
            return;
        }
        let id = self.allocate_id(&workspace.thread);
        if self.edit(cx, |layouts| layouts.split(&key, &id, direction)) {
            self.open_on_server(workspace, &id, None, cx).detach();
            self.request_focus(cx);
        }
    }

    /// Activates a terminal (and its group).
    pub fn set_active(&mut self, thread: &ThreadRef, terminal_id: &str, cx: &mut Context<Self>) {
        let key = thread.key();
        self.edit(cx, |layouts| layouts.set_active(&key, terminal_id));
        self.request_focus(cx);
    }

    /// Stores the drawer height a drag ended at.
    pub fn set_height(&mut self, thread: &ThreadRef, height: f32, cx: &mut Context<Self>) {
        let key = thread.key();
        self.edit(cx, |layouts| layouts.set_height(&key, height));
    }

    /// `terminal.close` for one terminal: removes the tab, then closes it on the server with
    /// `deleteHistory`, falling back to typing `exit` if that fails.
    pub fn close(&mut self, thread: &ThreadRef, terminal_id: &str, cx: &mut Context<Self>) {
        let key = thread.key();
        self.edit(cx, |layouts| layouts.close(&key, terminal_id));
        self.sessions.remove(&SessionKey {
            thread: thread.clone(),
            terminal_id: terminal_id.to_owned(),
        });
        self.request_focus(cx);
        let Some(client) = environment_client(&thread.environment_id, cx) else {
            return;
        };
        let close = TerminalCloseInput {
            thread_id: thread.thread_id.clone(),
            terminal_id: Some(terminal_id.into()),
            delete_history: Some(true),
        };
        let exit = TerminalWriteInput {
            thread_id: thread.thread_id.clone(),
            terminal_id: terminal_id.into(),
            data: "exit\n".into(),
        };
        cx.background_spawn(async move {
            if client.request::<TerminalClose>(&close).await.is_err() {
                client.request::<TerminalWrite>(&exit).await.ok();
            }
        })
        .detach();
    }

    /// Runs a project script in the thread's drawer (`runProjectScript`): reuses the active
    /// terminal unless it is busy, opens the drawer, and types the command.
    pub fn run_script(
        &mut self,
        workspace: &ThreadWorkspace,
        script: &ProjectScript,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), String>> {
        let key = workspace.thread.key();
        let layout = self.layouts.layout(&key);
        let server_ids = self.server_terminal_ids(&workspace.thread);
        let base = Some(layout.active_terminal_id.clone())
            .filter(|id| !id.is_empty())
            .or_else(|| server_ids.first().cloned())
            .unwrap_or_else(|| DEFAULT_TERMINAL_ID.to_owned());
        let busy = self
            .summary(&workspace.thread, &base)
            .is_some_and(|summary| summary.has_running_subprocess);
        let (target, size) = if busy {
            (self.allocate_id(&workspace.thread), Some(SCRIPT_TERMINAL_SIZE))
        } else {
            (base, None)
        };
        self.edit(cx, |layouts| {
            let opened = layouts.set_open(&key, true);
            let placed = if busy {
                layouts.new_terminal(&key, &target)
            } else {
                layouts.ensure(&key, &target, true, true)
            };
            opened || placed
        });
        self.request_focus(cx);

        let open = self.open_on_server(workspace, &target, size, cx);
        let client = environment_client(&workspace.thread.environment_id, cx);
        let write = TerminalWriteInput {
            thread_id: workspace.thread.thread_id.clone(),
            terminal_id: target.as_str().into(),
            data: format!("{}\r", script.command),
        };
        cx.background_spawn(async move {
            open.await?;
            let client = client.ok_or_else(|| "Environment is not connected.".to_owned())?;
            client
                .request::<TerminalWrite>(&write)
                .await
                .map_err(|error| rpc_error_text(&error))
        })
    }

    /// `terminal.open` at the workspace cwd with the project env.
    fn open_on_server(
        &self,
        workspace: &ThreadWorkspace,
        terminal_id: &str,
        size: Option<(u16, u16)>,
        cx: &App,
    ) -> Task<Result<(), String>> {
        let Some(client) = environment_client(&workspace.thread.environment_id, cx) else {
            return Task::ready(Err("Environment is not connected.".into()));
        };
        let input = TerminalOpenInput {
            thread_id: workspace.thread.thread_id.clone(),
            terminal_id: terminal_id.into(),
            cwd: workspace.cwd().to_owned(),
            worktree_path: workspace.worktree_path.clone(),
            cols: size.map(|(cols, _)| cols),
            rows: size.map(|(_, rows)| rows),
            env: Some(workspace.runtime_env()),
            provider_instance_id: None,
        };
        cx.background_spawn(async move {
            client
                .request::<TerminalOpen>(&input)
                .await
                .map(|_| ())
                .map_err(|error| rpc_error_text(&error))
        })
    }

    fn request_focus(&mut self, cx: &mut Context<Self>) {
        self.focus_request += 1;
        cx.notify();
    }

    /// Applies a layout transition; notifies and schedules a write when it changed something.
    fn edit(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut TerminalLayouts) -> bool,
    ) -> bool {
        if !change(&mut self.layouts) {
            return false;
        }
        cx.notify();
        let delay = cx.background_executor().timer(WRITE_DELAY);
        self.pending_write = Some(cx.spawn(async move |this, cx| {
            delay.await;
            this.update(cx, |this, cx| {
                this.pending_write = None;
                let store = AppState::global(cx).read(cx).store().clone();
                store.write(STATE_FILE, this.layouts.to_json(), cx).detach();
            })
            .ok();
        }));
        true
    }

    // -------------------------------------------------------------------------------------
    // Sessions and metadata

    /// The visible drawer calls this each render: remembers the thread as recently shown,
    /// starts the environment's metadata stream, and drops sessions nobody will show again.
    pub fn note_shown(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) {
        if self.shown.first() != Some(thread) {
            self.shown.retain(|shown| shown != thread);
            self.shown.insert(0, thread.clone());
            self.shown.truncate(MAX_HIDDEN_THREADS + 1);
            let shown: HashSet<&ThreadRef> = self.shown.iter().collect();
            self.sessions.retain(|key, _| shown.contains(&key.thread));
        }
        self.ensure_metadata(&thread.environment_id, cx);
        let layout = self.layout(thread);
        let live: HashSet<&String> = layout.terminal_ids.iter().collect();
        self.sessions
            .retain(|key, _| key.thread != *thread || live.contains(&key.terminal_id));
    }

    /// The session for a visible terminal, created (and attached) on first use.
    pub fn session(
        &mut self,
        workspace: &ThreadWorkspace,
        terminal_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalSession> {
        let key = SessionKey {
            thread: workspace.thread.clone(),
            terminal_id: terminal_id.to_owned(),
        };
        if let Some(live) = self.sessions.get(&key) {
            return live.session.clone();
        }
        let session = session::create(workspace, terminal_id, window, cx);
        let thread = workspace.thread.clone();
        let events = cx.subscribe(&session, move |this, session, event, cx| {
            let terminal_id = session.read(cx).terminal_id().to_owned();
            match event {
                SessionEvent::Ended => this.close(&thread, &terminal_id, cx),
                SessionEvent::OpenLink { url, path } => {
                    this.requests.push(DrawerRequest::OpenLink {
                        thread: thread.clone(),
                        url: url.clone(),
                        path: path.clone(),
                    });
                    cx.notify();
                }
                SessionEvent::SelectionMenu(selection, position) => {
                    let mut selection = selection.clone();
                    selection.terminal_label = this.label(&thread, &terminal_id);
                    this.requests
                        .push(DrawerRequest::SelectionMenu(selection, *position));
                    cx.notify();
                }
            }
        });
        self.sessions.insert(
            key,
            LiveSession {
                session: session.clone(),
                _events: events,
            },
        );
        session
    }

    /// Whether `thread`'s drawer is the one shown last (the visible one).
    pub fn is_front(&self, thread: &ThreadRef) -> bool {
        self.shown.first() == Some(thread)
    }

    /// A live session, if the drawer created one.
    pub fn find_session(
        &self,
        thread: &ThreadRef,
        terminal_id: &str,
    ) -> Option<Entity<TerminalSession>> {
        self.sessions
            .get(&SessionKey {
                thread: thread.clone(),
                terminal_id: terminal_id.to_owned(),
            })
            .map(|live| live.session.clone())
    }

    /// Requests from sessions since the last call, for the visible drawer to act on.
    pub fn take_requests(&mut self) -> Vec<DrawerRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Follows `subscribeTerminalMetadata` for an environment (once).
    fn ensure_metadata(&mut self, environment_id: &EnvironmentId, cx: &mut Context<Self>) {
        if self.environments.contains_key(environment_id) {
            return;
        }
        let client = environment_client(environment_id, cx);
        let id = environment_id.clone();
        let stream = client.map(|client| {
            cx.spawn(async move |this, cx| {
                let mut status = client.status();
                loop {
                    while !status.borrow_and_update().is_connected() {
                        if status.changed().await.is_err() {
                            return;
                        }
                    }
                    if let Ok(mut subscription) =
                        client.subscribe::<SubscribeTerminalMetadata>(&Empty {})
                    {
                        while let Some(Ok(event)) = subscription.next().await {
                            if this
                                .update(cx, |this, cx| this.apply_metadata(&id, event, cx))
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                    cx.background_executor().timer(RETRY_DELAY).await;
                }
            })
        });
        self.environments.insert(
            environment_id.clone(),
            EnvironmentTerminals {
                terminals: Vec::new(),
                _stream: stream,
            },
        );
    }

    fn apply_metadata(
        &mut self,
        environment_id: &EnvironmentId,
        event: TerminalMetadataEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(environment) = self.environments.get_mut(environment_id) else {
            return;
        };
        match event {
            TerminalMetadataEvent::Snapshot { terminals } => environment.terminals = terminals,
            TerminalMetadataEvent::Upsert { terminal } => {
                environment.terminals.retain(|summary| {
                    summary.thread_id != terminal.thread_id
                        || summary.terminal_id != terminal.terminal_id
                });
                environment.terminals.push(terminal);
            }
            TerminalMetadataEvent::Remove {
                thread_id,
                terminal_id,
            } => environment.terminals.retain(|summary| {
                summary.thread_id != thread_id || summary.terminal_id != terminal_id
            }),
            TerminalMetadataEvent::Unknown => return,
        }
        self.reconcile_shown(environment_id, cx);
        cx.notify();
    }

    /// Adopts the server's terminal list for the shown threads of an environment (ChatView's
    /// reconcile effect).
    fn reconcile_shown(&mut self, environment_id: &EnvironmentId, cx: &mut Context<Self>) {
        let threads: Vec<ThreadRef> = self
            .shown
            .iter()
            .filter(|thread| thread.environment_id == *environment_id)
            .cloned()
            .collect();
        for thread in threads {
            let ids = self.server_terminal_ids(&thread);
            let key = thread.key();
            self.edit(cx, |layouts| layouts.reconcile(&key, &ids));
        }
    }

    /// Test and scene hook: replaces an environment's terminal metadata.
    pub fn set_metadata(
        &mut self,
        environment_id: EnvironmentId,
        terminals: Vec<TerminalSummary>,
        cx: &mut Context<Self>,
    ) {
        self.environments.insert(
            environment_id,
            EnvironmentTerminals {
                terminals,
                _stream: None,
            },
        );
        cx.notify();
    }

    /// Scene hook: replaces a thread's layout state through the normal transitions.
    pub fn edit_layouts(
        &mut self,
        change: impl FnOnce(&mut TerminalLayouts) -> bool,
        cx: &mut Context<Self>,
    ) {
        self.edit(cx, change);
    }
}

/// Sort key that orders `term-2` before `term-10` (`localeCompare` with `numeric`).
fn natural_order(id: &str) -> (String, u64) {
    let digits_start = id
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |index| index + 1);
    let number = id[digits_start..].parse().unwrap_or(0);
    (id[..digits_start].to_owned(), number)
}
