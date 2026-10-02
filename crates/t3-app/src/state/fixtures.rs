//! Builds app state from recorded JSON so snapshot scenes and tests run without a server.
//!
//! A fixture is one JSON document:
//!
//! ```json
//! {
//!   "now": "2026-10-01T12:00:00.000Z",
//!   "environments": [
//!     {"id": "env-local", "label": "HOME-PC", "kind": "local", "shell": { ...OrchestrationShellSnapshot }}
//!   ],
//!   "route": {"thread": {"environmentId": "env-local", "threadId": "t1"}},
//!   "settings": { ...ClientSettings },
//!   "ui": { ...UiState }
//! }
//! ```
//!
//! `shell` is exactly what the server sends (`GET /api/orchestration/shell` or the socket's
//! snapshot item), so a capture from a real server drops in unchanged. `now` pins the clock that
//! relative times are computed against.

use gpui_kit::{App, AppContext as _, Entity};
use serde::Deserialize;
use t3_client::ShellState;
use t3_logic::{ThreadRef, settings::ClientSettings, time::parse_timestamp, ui_state::UiState};
use t3_protocol::{
    EnvironmentId,
    orchestration::OrchestrationShellSnapshot,
    vcs::{VcsStatusLocal, VcsStatusRemote},
};

use super::{
    AppState, ConnectionStatus, Environment, EnvironmentKind, Route, SettingsPage, Store,
    vcs::{VcsKey, VcsStatus, VcsStatusStore},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    now: Option<String>,
    #[serde(default)]
    environments: Vec<FixtureEnvironment>,
    #[serde(default)]
    route: FixtureRoute,
    #[serde(default)]
    settings: ClientSettings,
    #[serde(default)]
    ui: UiState,
    #[serde(default)]
    vcs: Vec<FixtureVcs>,
}

/// A recorded `subscribeVcsStatus` state for one working copy.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureVcs {
    environment_id: EnvironmentId,
    cwd: String,
    local: Option<VcsStatusLocal>,
    remote: Option<VcsStatusRemote>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureEnvironment {
    id: EnvironmentId,
    label: String,
    #[serde(default)]
    kind: FixtureKind,
    shell: Option<OrchestrationShellSnapshot>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum FixtureKind {
    #[default]
    Local,
    DesktopLocal,
    Remote,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum FixtureRoute {
    #[default]
    Index,
    #[serde(rename_all = "camelCase")]
    Thread {
        environment_id: EnvironmentId,
        thread_id: t3_protocol::ThreadId,
    },
    Settings,
}

/// Creates the global [`AppState`] from fixture JSON. Persistence is in memory only.
pub fn load(json: &str, cx: &mut App) -> anyhow::Result<Entity<AppState>> {
    let fixture: Fixture = serde_json::from_str(json)?;
    let state = AppState::init(Store::Memory, cx);
    let environments: Vec<Entity<Environment>> = fixture
        .environments
        .into_iter()
        .map(|environment| {
            let kind = match environment.kind {
                FixtureKind::Local => EnvironmentKind::Local,
                FixtureKind::DesktopLocal => EnvironmentKind::DesktopLocal,
                FixtureKind::Remote => EnvironmentKind::Remote,
            };
            cx.new(|cx| {
                let mut entity = Environment::new(environment.id, environment.label, kind);
                entity.set_status(ConnectionStatus::Connected { generation: 1 }, cx);
                if let Some(snapshot) = environment.shell {
                    let mut shell = ShellState::default();
                    shell.apply_snapshot(snapshot);
                    shell.end_sync();
                    entity.set_shell(shell, cx);
                }
                entity
            })
        })
        .collect();
    let route = match fixture.route {
        FixtureRoute::Index => Route::Index,
        FixtureRoute::Thread {
            environment_id,
            thread_id,
        } => Route::Thread(ThreadRef::new(environment_id, thread_id)),
        FixtureRoute::Settings => Route::Settings(SettingsPage::General),
    };
    let now = fixture.now.as_deref().and_then(parse_timestamp);
    let vcs = VcsStatusStore::global(cx);
    vcs.update(cx, |store, cx| {
        for entry in fixture.vcs {
            let key = VcsKey {
                environment_id: entry.environment_id,
                cwd: entry.cwd,
            };
            let status = VcsStatus {
                local: entry.local,
                remote: entry.remote,
            };
            store.set_status(key, status, cx);
        }
    });
    state.update(cx, |state, cx| {
        for environment in environments {
            state.add_environment(environment, cx);
        }
        state.update_settings(|settings| *settings = fixture.settings, cx);
        state.update_ui(
            |ui| {
                *ui = fixture.ui;
                true
            },
            cx,
        );
        state.replace_route(route, cx);
        state.set_fixed_now(now);
    });
    Ok(state)
}
