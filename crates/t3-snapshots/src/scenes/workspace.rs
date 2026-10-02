//! The workspace shell: sidebar populated from shell fixtures, collapsed, and empty.
//!
//! `sidebar-reference-*` load the shell and server config recorded from the e2e nightly server
//! (`fixtures/shell.json`, `server-config.json`; the seed behind `docs/reference/sidebar-*.png`)
//! with the clock at seed time + 2 minutes, like the reference capture, so they compare 1:1.
//! `sidebar.json` is handcrafted to cover every status, grouping, badges, and overflow.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div,
};
use t3_app::state::{AppState, Route, fixtures};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const RECORDED_SHELL: &str = include_str!("../../fixtures/shell.json");
const RECORDED_CONFIG: &str = include_str!("../../fixtures/server-config.json");
const RECORDED_MANIFEST: &str = include_str!("../../fixtures/manifest.json");

/// The recorded e2e environment as a workspace fixture.
fn recorded_fixture() -> String {
    let parse = |json: &str| -> serde_json::Value {
        serde_json::from_str(json).expect("recorded fixture is JSON")
    };
    let manifest = parse(RECORDED_MANIFEST);
    let seeded_at = manifest["seededAt"]
        .as_str()
        .expect("manifest has seededAt");
    let now = chrono::DateTime::parse_from_rfc3339(seeded_at).expect("seededAt is RFC 3339")
        + chrono::Duration::minutes(2);
    serde_json::json!({
        "now": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "environments": [{
            "id": manifest["environmentId"],
            "label": "fixture-host",
            "kind": "local",
            "shell": parse(RECORDED_SHELL),
            "serverConfig": parse(RECORDED_CONFIG),
        }],
    })
    .to_string()
}
const SIDEBAR: &str = include_str!("../../fixtures/sidebar.json");
const EMPTY: &str = include_str!("../../fixtures/empty.json");

/// Loads `fixture` into a fresh app state, lets `adjust` edit it, and mounts the workspace.
fn workspace(
    fixture: &str,
    window: &mut Window,
    cx: &mut App,
    adjust: impl FnOnce(&mut AppState, &mut gpui_kit::Context<AppState>),
) -> AnyView {
    let state = fixtures::load(fixture, cx).expect("fixture should decode");
    state.update(cx, adjust);
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx)).into();
    cx.new(|_| Backdrop(workspace)).into()
}

/// Headless captures have no native window material behind the translucent glass, so paint the
/// opaque `background` the web reference renders on (`--app-chrome-background`).
struct Backdrop(AnyView);

impl Render for Backdrop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.colors().background)
            .child(self.0.clone())
    }
}

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("sidebar-reference-dark", ThemeMode::Dark, |window, cx| {
            workspace(&recorded_fixture(), window, cx, |_, _| {})
        }),
        Scene::new("sidebar-reference-light", ThemeMode::Light, |window, cx| {
            workspace(&recorded_fixture(), window, cx, |_, _| {})
        }),
        Scene::new("workspace-sidebar-dark", ThemeMode::Dark, |window, cx| {
            workspace(SIDEBAR, window, cx, |_, _| {})
        }),
        Scene::new("workspace-sidebar-light", ThemeMode::Light, |window, cx| {
            workspace(SIDEBAR, window, cx, |_, _| {})
        }),
        Scene::new("workspace-index-dark", ThemeMode::Dark, |window, cx| {
            workspace(SIDEBAR, window, cx, |state, cx| {
                state.replace_route(Route::Index, cx)
            })
        }),
        Scene::new("workspace-collapsed-dark", ThemeMode::Dark, |window, cx| {
            workspace(SIDEBAR, window, cx, |state, cx| {
                state.replace_route(Route::Index, cx);
                state.set_sidebar_open(false, cx);
            })
        }),
        Scene::new("workspace-empty-dark", ThemeMode::Dark, |window, cx| {
            workspace(EMPTY, window, cx, |_, _| {})
        }),
        Scene::new("workspace-empty-light", ThemeMode::Light, |window, cx| {
            workspace(EMPTY, window, cx, |_, _| {})
        }),
    ]
}
