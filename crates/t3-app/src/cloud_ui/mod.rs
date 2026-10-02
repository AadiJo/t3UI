//! T3 Connect in the app: the signed-in account, the sign-in dialog, and the environments
//! linked to the account (connections.md 6.6). Backend: [`t3_client::cloud`].
//!
//! [`CloudAccount`] is a global entity. [`CloudAccount::init`] runs at launch: it loads the
//! stored session, starts the saved T3 Connect environments of that account (startup boot skips
//! relay targets), and checks the session with Clerk. Connected T3 Connect environments are
//! ordinary [`AppState`] environments (kind `Remote`), so the sidebar and chat treat them like
//! paired ones; their catalog entries carry a `relay` target owned by the account.
//!
//! What the Connections page embeds:
//! - [`T3ConnectPanel`]: a whole "T3 Connect" section (account row, connected and linked
//!   environments). Mount with `cx.new(|cx| T3ConnectPanel::new(cx))`.
//! - Or the pieces: [`AccountRow`], [`CloudEnvironmentRows`] (discovered environments with
//!   Connect, for the "Remote environments" card), [`RelayEnvironmentRow`] (a saved one with
//!   Disconnect). Hosts render [`CloudAccount::sign_in_dialog`] somewhere in their tree.
//! - Actions: [`CloudAccount::open_sign_in`], [`sign_out`](CloudAccount::sign_out),
//!   [`refresh`](CloudAccount::refresh), [`connect_environment`](CloudAccount::connect_environment),
//!   [`disconnect_environment`](CloudAccount::disconnect_environment).

mod rows;
mod sign_in;
mod web_auth;

use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task, Window};
use t3_client::{
    EnvironmentOptions,
    cloud::{
        Account, CloudConfig, CloudState, RelayEnvironment, T3Connect, WebAuthenticator,
        remove_relay_environments,
    },
    store::{
        CatalogStore, EnvironmentCatalog, KnownTarget, SavedTarget, SecretBackend, StoreError,
        open_secret_store,
    },
};
use t3_protocol::EnvironmentId;
use tokio::sync::watch;

pub use rows::{AccountRow, CloudEnvironmentRows, RelayEnvironmentRow, T3ConnectPanel};
pub use sign_in::{SignInDialog, SignInStep};

use crate::{
    state::{AppState, Environment, EnvironmentKind},
    toast::{self, Toast},
};

/// The T3 Connect session and the environments it reaches. See the module docs.
pub struct CloudAccount {
    /// `None` in snapshot scenes and when the secret store is unusable: actions do nothing.
    connect: Option<T3Connect>,
    state: CloudState,
    app_state: Entity<AppState>,
    /// T3 Connect environments in `AppState`, in the order they were added.
    relay_environments: Vec<EnvironmentId>,
    connecting: Option<EnvironmentId>,
    disconnecting: Option<EnvironmentId>,
    signing_out: bool,
    authenticator: Option<Arc<dyn WebAuthenticator>>,
    sign_in: Option<Entity<SignInDialog>>,
    _state_watch: Option<Task<()>>,
}

struct GlobalCloudAccount(Entity<CloudAccount>);

impl Global for GlobalCloudAccount {}

impl CloudAccount {
    /// Creates the global account at launch (after saved environments started) and starts the
    /// stored account's T3 Connect environments.
    pub fn init(app_state: &Entity<AppState>, cx: &mut App) -> Entity<Self> {
        let connect = T3Connect::new(
            CloudConfig::production(),
            open_secret_store(SecretBackend::from_env()),
        )
        .inspect_err(|error| tracing::warn!("T3 Connect is unavailable: {error}"))
        .ok();
        let account = cx.new(|cx| {
            let mut this = Self::detached(app_state.clone(), CloudState::default());
            if let Some(connect) = connect {
                let receiver = connect.state();
                this.state = receiver.borrow().clone();
                this._state_watch = Some(watch_state(receiver, cx));
                this.authenticator = web_auth::platform_authenticator();
                this.connect = Some(connect);
                this.start_saved(cx);
                this.restore(cx);
            }
            this
        });
        cx.set_global(GlobalCloudAccount(account.clone()));
        account
    }

    /// A global account showing `state` with no backend (snapshot scenes). `providers` shows
    /// the provider buttons as on macOS; `relay` marks environments already in `app_state` as
    /// connected through T3 Connect.
    pub fn init_fixture(
        app_state: &Entity<AppState>,
        state: CloudState,
        providers: bool,
        relay: Vec<EnvironmentId>,
        cx: &mut App,
    ) -> Entity<Self> {
        let account = cx.new(|_| {
            let mut this = Self::detached(app_state.clone(), state);
            if providers {
                this.authenticator = Some(Arc::new(web_auth::Unavailable));
            }
            this.relay_environments = relay;
            this
        });
        cx.set_global(GlobalCloudAccount(account.clone()));
        account
    }

    fn detached(app_state: Entity<AppState>, state: CloudState) -> Self {
        CloudAccount {
            connect: None,
            state,
            app_state,
            relay_environments: Vec::new(),
            connecting: None,
            disconnecting: None,
            signing_out: false,
            authenticator: None,
            sign_in: None,
            _state_watch: None,
        }
    }

    /// The global account, once [`init`](Self::init) ran.
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalCloudAccount>().map(|g| g.0.clone())
    }

    pub fn state(&self) -> &CloudState {
        &self.state
    }

    pub fn account(&self) -> Option<&Account> {
        self.state.account.as_ref()
    }

    /// The backend handle (`None` in scenes).
    pub fn connect(&self) -> Option<&T3Connect> {
        self.connect.as_ref()
    }

    pub fn app_state(&self) -> &Entity<AppState> {
        &self.app_state
    }

    /// The linked environment whose Connect is running.
    pub fn connecting(&self) -> Option<&EnvironmentId> {
        self.connecting.as_ref()
    }

    pub fn disconnecting(&self) -> Option<&EnvironmentId> {
        self.disconnecting.as_ref()
    }

    pub fn signing_out(&self) -> bool {
        self.signing_out
    }

    /// T3 Connect environments currently in `AppState`.
    pub fn relay_environments(&self) -> &[EnvironmentId] {
        &self.relay_environments
    }

    /// Whether "Continue with GitHub" and friends can run here (macOS only).
    pub fn providers_available(&self) -> bool {
        self.authenticator.is_some()
    }

    pub(crate) fn authenticator(&self) -> Option<Arc<dyn WebAuthenticator>> {
        self.authenticator.clone()
    }

    /// The open sign-in dialog. Hosts render it anywhere; it draws in a full-window layer.
    pub fn sign_in_dialog(&self) -> Option<Entity<SignInDialog>> {
        self.sign_in.clone()
    }

    /// Opens the sign-in dialog ("Sign in to T3 Connect").
    pub fn open_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sign_in.is_none() {
            let account = cx.entity();
            self.sign_in = Some(cx.new(|cx| SignInDialog::new(account, window, cx)));
            cx.notify();
        }
    }

    /// Opens the sign-in dialog on `step` without a backend (snapshot scenes).
    pub fn open_sign_in_preview(
        &mut self,
        step: SignInStep,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let account = cx.entity();
        self.sign_in = Some(cx.new(|cx| {
            let mut dialog = SignInDialog::new(account, window, cx);
            dialog.preview(step, window, cx);
            dialog
        }));
        cx.notify();
    }

    pub fn close_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.sign_in.take().is_some() {
            cx.notify();
        }
    }

    /// Reloads the linked-environment list.
    pub fn refresh(&mut self, _cx: &mut Context<Self>) {
        if let Some(connect) = &self.connect {
            connect.refresh();
        }
    }

    /// "Connect" on a linked environment: saves it to the catalog and starts it.
    pub fn connect_environment(&mut self, environment: &RelayEnvironment, cx: &mut Context<Self>) {
        let Some(connect) = self.connect.clone() else {
            return;
        };
        if self.connecting.is_some() {
            return;
        }
        let saved = match connect.saved_environment(environment) {
            Ok(saved) => saved,
            Err(error) => {
                toast::show(
                    Toast::error("Could not connect environment").description(error.message),
                    cx,
                );
                return;
            }
        };
        self.connecting = Some(saved.environment_id.clone());
        cx.notify();
        let entry = saved.clone();
        let write = edit_catalog(cx, move |catalog| catalog.upsert(entry));
        cx.spawn(async move |this, cx| {
            let result = write.await;
            this.update(cx, |this, cx| {
                this.connecting = None;
                match result {
                    Ok(()) => {
                        this.start(&saved, cx);
                        toast::show(
                            Toast::success("Environment connected").description(format!(
                                "{} is available through T3 Connect.",
                                saved.label
                            )),
                            cx,
                        );
                    }
                    Err(error) => {
                        tracing::warn!("saving {} failed: {error}", saved.label);
                        toast::show(
                            Toast::error("Could not connect environment")
                                .description(error.to_string()),
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

    /// "Disconnect" on a connected T3 Connect environment: forgets it (fork behavior: no
    /// confirm). It stays linked to the account and shows up again under the linked list.
    pub fn disconnect_environment(&mut self, id: &EnvironmentId, cx: &mut Context<Self>) {
        if self.connect.is_none() || self.disconnecting.is_some() {
            return;
        }
        self.disconnecting = Some(id.clone());
        cx.notify();
        let removed = id.clone();
        let write = edit_catalog(cx, move |catalog| {
            catalog.remove(&removed);
        });
        let id = id.clone();
        cx.spawn(async move |this, cx| {
            let result = write.await;
            this.update(cx, |this, cx| {
                this.disconnecting = None;
                match result {
                    Ok(()) => this.stop(&[id], cx),
                    Err(error) => {
                        toast::show(
                            Toast::error("Could not remove backend").description(error.to_string()),
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

    /// Signs out of T3 Connect and forgets every T3 Connect environment (connections.md 1.5).
    /// Directly paired environments stay.
    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        let Some(connect) = self.connect.clone() else {
            return;
        };
        if self.signing_out {
            return;
        }
        self.signing_out = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let signed_out = connect.sign_out().await;
            let write = this
                .update(cx, |_, cx| {
                    edit_catalog(cx, |catalog| {
                        remove_relay_environments(catalog);
                    })
                })
                .ok();
            let written = match write {
                Some(write) => write.await,
                None => return,
            };
            this.update(cx, |this, cx| {
                this.signing_out = false;
                let ids = std::mem::take(&mut this.relay_environments);
                this.stop(&ids, cx);
                if let Err(error) = signed_out {
                    toast::show(
                        Toast::error("Could not sign out").description(error.message),
                        cx,
                    );
                } else if let Err(error) = written {
                    tracing::warn!("removing T3 Connect environments failed: {error}");
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Checks the stored session with Clerk and refreshes the list.
    fn restore(&self, cx: &mut Context<Self>) {
        let Some(connect) = self.connect.clone() else {
            return;
        };
        cx.spawn(async move |_, _| {
            if let Err(error) = connect.restore().await {
                tracing::warn!("could not check the T3 Connect session: {error}");
            }
        })
        .detach();
    }

    /// Mirrors a new backend state. A newly signed-in account gets its saved environments
    /// started (and blocked ones retried); other accounts' entries are dropped.
    fn apply_state(&mut self, state: CloudState, cx: &mut Context<Self>) {
        let previous = self.state.account.as_ref().map(|a| a.user_id.clone());
        let current = state.account.as_ref().map(|a| a.user_id.clone());
        self.state = state;
        if current.is_some() && current != previous {
            self.start_saved(cx);
            for id in &self.relay_environments {
                if let Some(environment) = self.app_state.read(cx).environment(id, cx)
                    && let Some(client) = environment.read(cx).client()
                {
                    client.retry_now();
                }
            }
        }
        cx.notify();
    }

    /// Starts the signed-in account's saved T3 Connect environments that are not running yet,
    /// and drops saved entries that belong to another account. Startup-only disk read.
    fn start_saved(&mut self, cx: &mut Context<Self>) {
        let (Some(connect), Some(account)) = (self.connect.clone(), self.state.account.as_ref())
        else {
            return;
        };
        let catalog = match CatalogStore::new().load() {
            Ok(catalog) => catalog,
            Err(error) => {
                tracing::warn!("failed to load saved environments: {error}");
                return;
            }
        };
        let user_id = account.user_id.clone();
        let mut foreign = Vec::new();
        let mut mine = Vec::new();
        for saved in catalog.environments {
            match &saved.target {
                SavedTarget::Known(KnownTarget::Relay { account_id }) if *account_id == user_id => {
                    if connect.saved_endpoint(&saved).is_some() {
                        mine.push(saved);
                    }
                }
                SavedTarget::Known(KnownTarget::Relay { .. }) => foreign.push(saved.environment_id),
                _ => {}
            }
        }
        for saved in mine {
            if !self.relay_environments.contains(&saved.environment_id) {
                self.start(&saved, cx);
            }
        }
        if !foreign.is_empty() {
            self.stop(&foreign, cx);
            edit_catalog(cx, move |catalog| {
                catalog
                    .environments
                    .retain(|e| !foreign.contains(&e.environment_id));
            })
            .detach();
        }
    }

    /// Starts a saved T3 Connect environment and adds it to `AppState`.
    fn start(&mut self, saved: &t3_client::store::SavedEnvironment, cx: &mut Context<Self>) {
        let Some(connect) = &self.connect else {
            return;
        };
        let id = saved.environment_id.clone();
        if self.relay_environments.contains(&id)
            || self.app_state.read(cx).environment(&id, cx).is_some()
        {
            return;
        }
        let client = t3_client::Environment::start(EnvironmentOptions::from_saved(
            saved,
            connect.endpoint(id.clone()),
        ));
        let environment = cx.new(|cx| Environment::connected(client, EnvironmentKind::Remote, cx));
        self.app_state
            .update(cx, |state, cx| state.add_environment(environment, cx));
        self.relay_environments.push(id);
    }

    /// Removes environments from `AppState` and closes their connections.
    fn stop(&mut self, ids: &[EnvironmentId], cx: &mut Context<Self>) {
        self.relay_environments.retain(|id| !ids.contains(id));
        self.app_state.update(cx, |state, cx| {
            for id in ids {
                if let Some(environment) = state.remove_environment(id, cx)
                    && let Some(client) = environment.read(cx).client()
                {
                    client.disconnect();
                }
            }
        });
    }
}

/// Mirrors the backend's state channel into the entity.
fn watch_state(
    mut receiver: watch::Receiver<CloudState>,
    cx: &mut Context<CloudAccount>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        while receiver.changed().await.is_ok() {
            let state = receiver.borrow_and_update().clone();
            if this
                .update(cx, |this, cx| this.apply_state(state, cx))
                .is_err()
            {
                break;
            }
        }
    })
}

/// Serializes catalog read-modify-writes from this module.
static CATALOG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Edits `environments.json` on the background executor.
fn edit_catalog(
    cx: &App,
    edit: impl FnOnce(&mut EnvironmentCatalog) + Send + 'static,
) -> Task<Result<(), StoreError>> {
    cx.background_spawn(async move {
        let _guard = CATALOG_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let store = CatalogStore::new();
        let mut catalog = store.load()?;
        edit(&mut catalog);
        store.save(&catalog)
    })
}
