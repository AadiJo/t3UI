//! App-wide state, owned by one global [`AppState`] entity.
//!
//! ```text
//! AppState (global Entity)
//! ├── environments: Vec<Entity<Environment>>   primary first
//! ├── route: Route + back/forward history      (not persisted)
//! ├── settings: ClientSettings                 client-settings.json
//! ├── ui: UiState                              ui-state.json (debounced)
//! └── sidebar_open, optimistic work markers    (not persisted)
//! ```
//!
//! Reading: `AppState::global(cx).read(cx).route()`. Subscribing: `cx.observe(&app_state, ..)`
//! re-renders on any change; `cx.subscribe(&app_state, ..)` receives [`AppEvent`]s (keyboard
//! commands routed to the view that owns them). Writing: `app_state.update(cx, |state, cx|
//! state.navigate(Route::Index, cx))`.

mod environment;
pub mod fixtures;
mod route;
mod store;

use std::{collections::HashSet, ops::Deref, sync::Arc, time::Duration};

use gpui_kit::{App, AppContext as _, Context, Entity, EventEmitter, Global, Task};
use t3_logic::{
    ProjectRef, ThreadRef,
    keybindings::{Command, ResolvedKeybindingRule, default_keybindings},
    settings::ClientSettings,
    ui_state::UiState,
};
use t3_protocol::{EnvironmentId, orchestration::ThreadEnvMode, server::ServerConfig};

pub use environment::{Environment, EnvironmentKind};
pub use route::{DraftId, Route, SettingsPage};
pub use store::Store;
pub use t3_client::ConnectionStatus;

const SETTINGS_FILE: &str = "client-settings.json";
const UI_STATE_FILE: &str = "ui-state.json";
/// The web debounces ui-state writes by 500ms.
const UI_STATE_WRITE_DELAY: Duration = Duration::from_millis(500);

/// Events other views subscribe to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppEvent {
    /// A keyboard shortcut (or menu item) resolved to `command`. The view that owns the command
    /// handles it; everyone else ignores it.
    Command(Command),
    /// Start a new thread (spec 1.4 `handleNewThread`). The draft store owner reuses or creates a
    /// draft for the project and navigates to it.
    NewThread(NewThreadRequest),
}

/// A request to open a new-thread draft, seeded from the sidebar context (spec 2.10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewThreadRequest {
    pub project: ProjectRef,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    /// `None` uses the environment default (`defaultThreadEnvMode`).
    pub env_mode: Option<ThreadEnvMode>,
    pub start_from_origin: Option<bool>,
}

/// The active keybinding rules: the primary environment's, or the defaults before its config
/// arrives. Derefs to the rule slice.
#[derive(Clone)]
pub enum Keybindings {
    Server(Arc<ServerConfig>),
    Default(Arc<[ResolvedKeybindingRule]>),
}

impl Deref for Keybindings {
    type Target = [ResolvedKeybindingRule];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Server(config) => &config.keybindings,
            Self::Default(rules) => rules,
        }
    }
}

/// Global app state. See the module docs for how views use it.
pub struct AppState {
    store: Store,
    environments: Vec<Entity<Environment>>,
    route: Route,
    back: Vec<Route>,
    forward: Vec<Route>,
    settings: ClientSettings,
    ui: UiState,
    sidebar_open: bool,
    optimistic_work: HashSet<ThreadRef>,
    default_keybindings: Arc<[ResolvedKeybindingRule]>,
    fixed_now: Option<i64>,
    pending_ui_write: Option<Task<()>>,
    _quit: gpui_kit::Subscription,
}

struct GlobalAppState(Entity<AppState>);

impl Global for GlobalAppState {}

impl EventEmitter<AppEvent> for AppState {}

impl AppState {
    /// Loads persisted state from `store`, creates the global entity, and returns it.
    pub fn init(store: Store, cx: &mut App) -> Entity<Self> {
        let settings = store
            .read(SETTINGS_FILE)
            .map(|json| ClientSettings::from_json(&json))
            .unwrap_or_default();
        let ui = store
            .read(UI_STATE_FILE)
            .map(|json| UiState::from_json(&json))
            .unwrap_or_default();
        let state = cx.new(|cx| Self {
            store,
            environments: Vec::new(),
            route: Route::Index,
            back: Vec::new(),
            forward: Vec::new(),
            settings,
            ui,
            sidebar_open: true,
            optimistic_work: HashSet::new(),
            default_keybindings: default_keybindings().into(),
            fixed_now: None,
            pending_ui_write: None,
            _quit: cx.on_app_quit(|this: &mut Self, _| {
                this.flush_ui_state();
                async {}
            }),
        });
        cx.set_global(GlobalAppState(state.clone()));
        state
    }

    /// The global entity. Panics before [`AppState::init`].
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalAppState>().0.clone()
    }

    // ---------------------------------------------------------------------------------------
    // Route

    /// The route the main column shows.
    pub fn route(&self) -> &Route {
        &self.route
    }

    /// Pushes `route` (web `navigate`). No-op when already there.
    pub fn navigate(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route == route {
            return;
        }
        let previous = std::mem::replace(&mut self.route, route);
        self.back.push(previous);
        self.forward.clear();
        cx.notify();
    }

    /// Replaces the current route without adding history (web `navigate({replace: true})`).
    pub fn replace_route(&mut self, route: Route, cx: &mut Context<Self>) {
        if self.route != route {
            self.route = route;
            cx.notify();
        }
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// History back, or `/` when there is none (settings Back and Escape).
    pub fn go_back(&mut self, cx: &mut Context<Self>) {
        let previous = self.back.pop().unwrap_or_default();
        let current = std::mem::replace(&mut self.route, previous);
        self.forward.push(current);
        cx.notify();
    }

    /// History forward, if any.
    pub fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.forward.pop() {
            let current = std::mem::replace(&mut self.route, next);
            self.back.push(current);
            cx.notify();
        }
    }

    // ---------------------------------------------------------------------------------------
    // Environments

    /// Connected environments, primary first.
    pub fn environments(&self) -> &[Entity<Environment>] {
        &self.environments
    }

    /// The local environment (this machine's server), when connected.
    pub fn primary_environment(&self) -> Option<&Entity<Environment>> {
        self.environments.first()
    }

    pub fn environment(&self, id: &EnvironmentId, cx: &App) -> Option<Entity<Environment>> {
        self.environments
            .iter()
            .find(|environment| environment.read(cx).id() == id)
            .cloned()
    }

    /// Adds an environment. The first one added is the primary. AppState re-notifies whenever
    /// an environment changes, so observing AppState is enough for views that read every
    /// environment (the sidebar).
    pub fn add_environment(&mut self, environment: Entity<Environment>, cx: &mut Context<Self>) {
        cx.observe(&environment, |_, _, cx| cx.notify()).detach();
        self.environments.push(environment);
        cx.notify();
    }

    /// The active keybinding rules.
    pub fn keybindings(&self, cx: &App) -> Keybindings {
        self.primary_environment()
            .and_then(|environment| environment.read(cx).config().cloned())
            .map_or_else(
                || Keybindings::Default(self.default_keybindings.clone()),
                Keybindings::Server,
            )
    }

    /// Routes a keyboard or menu command to the view that owns it.
    pub fn dispatch_command(&mut self, command: Command, cx: &mut Context<Self>) {
        cx.emit(AppEvent::Command(command));
    }

    /// Asks the draft owner to open a new thread.
    pub fn request_new_thread(&mut self, request: NewThreadRequest, cx: &mut Context<Self>) {
        cx.emit(AppEvent::NewThread(request));
    }

    // ---------------------------------------------------------------------------------------
    // Settings and persisted UI state

    pub fn settings(&self) -> &ClientSettings {
        &self.settings
    }

    /// Edits client settings and saves them when something changed.
    pub fn update_settings(
        &mut self,
        edit: impl FnOnce(&mut ClientSettings),
        cx: &mut Context<Self>,
    ) {
        let before = self.settings.clone();
        edit(&mut self.settings);
        if self.settings == before {
            return;
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.settings) {
            self.store.write(SETTINGS_FILE, json, cx).detach();
        }
        cx.notify();
    }

    pub fn ui(&self) -> &UiState {
        &self.ui
    }

    /// Edits persisted UI state. `edit` returns whether it changed anything (the `UiState`
    /// transitions do); changes notify and are written 500ms after the last one.
    pub fn update_ui(&mut self, edit: impl FnOnce(&mut UiState) -> bool, cx: &mut Context<Self>) {
        if !edit(&mut self.ui) {
            return;
        }
        cx.notify();
        let delay = cx.background_executor().timer(UI_STATE_WRITE_DELAY);
        self.pending_ui_write = Some(cx.spawn(async move |this, cx| {
            delay.await;
            this.update(cx, |this, cx| {
                this.pending_ui_write = None;
                if let Ok(json) = serde_json::to_string_pretty(&this.ui) {
                    this.store.write(UI_STATE_FILE, json, cx).detach();
                }
            })
            .ok();
        }));
    }

    fn flush_ui_state(&mut self) {
        if self.pending_ui_write.take().is_some()
            && let Ok(json) = serde_json::to_string_pretty(&self.ui)
        {
            self.store.write_now(UI_STATE_FILE, &json);
        }
    }

    /// Records that the user saw `thread`'s completion at `completed_at` (clears "Completed").
    /// Upstream has no server acknowledgement, so this is local only.
    pub fn mark_thread_visited(
        &mut self,
        thread: &ThreadRef,
        completed_at: &str,
        cx: &mut Context<Self>,
    ) {
        let key = thread.key();
        self.update_ui(|ui| ui.mark_thread_visited(&key, completed_at), cx);
    }

    // ---------------------------------------------------------------------------------------
    // Transient UI state

    /// The time relative labels ("5m ago") are computed against, in epoch milliseconds. Fixed in
    /// snapshot scenes so captures are reproducible.
    pub fn now_millis(&self) -> i64 {
        self.fixed_now.unwrap_or_else(t3_logic::time::now_millis)
    }

    /// False when the clock is pinned (fixtures): continuous animations stay still so captures
    /// are reproducible.
    pub fn clock_is_live(&self) -> bool {
        self.fixed_now.is_none()
    }

    /// Pins [`AppState::now_millis`] (fixtures only).
    pub fn set_fixed_now(&mut self, now: Option<i64>) {
        self.fixed_now = now;
    }

    /// Whether the main sidebar is open. Not persisted; the app starts open.
    pub fn sidebar_open(&self) -> bool {
        self.sidebar_open
    }

    pub fn set_sidebar_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.sidebar_open != open {
            self.sidebar_open = open;
            cx.notify();
        }
    }

    /// Threads where the user just sent a message and the server has not reported work yet.
    /// The sidebar shows them as Working.
    pub fn optimistic_work(&self) -> &HashSet<ThreadRef> {
        &self.optimistic_work
    }

    /// Sets or clears the optimistic "work started" marker for a thread.
    pub fn set_optimistic_work(
        &mut self,
        thread: ThreadRef,
        started: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = if started {
            self.optimistic_work.insert(thread)
        } else {
            self.optimistic_work.remove(&thread)
        };
        if changed {
            cx.notify();
        }
    }
}
