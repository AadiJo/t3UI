//! Standalone pages: Usage (cost, tokens, past 24h, limits, loading).
//!
//! Usage data is real: `e2e/usage-fixture.mjs` wrote deterministic Codex and Claude transcripts
//! into an e2e nightly server's isolated HOME and recorded its `server.getUsageSummary` answers
//! and server config (`fixtures/usage/`). The clock is pinned to the recording time, in UTC.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div,
};
use t3_app::state::{AppState, Route, fixtures};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const RECORDED_SHELL: &str = include_str!("../../fixtures/shell.json");
const USAGE_CONFIG: &str = include_str!("../../fixtures/usage/server-config.json");
const USAGE_30D: &str = include_str!("../../fixtures/usage/summary-30d.json");
const USAGE_24H: &str = include_str!("../../fixtures/usage/summary-24h.json");
const USAGE_META: &str = include_str!("../../fixtures/usage/meta.json");
const RECORDED_MANIFEST: &str = include_str!("../../fixtures/manifest.json");

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("recorded fixture is JSON")
}

/// The recorded environment with `summary` as its usage answer (`None`: still scanning) and the
/// page set to `metric` over `days`.
fn usage_fixture(summary: Option<&str>, metric: &str, days: u32) -> String {
    let meta = parse(USAGE_META);
    // The shell was recorded under the manifest's environment id; usage does not care.
    let manifest = parse(RECORDED_MANIFEST);
    serde_json::json!({
        "now": meta["now"],
        "environments": [{
            "id": manifest["environmentId"],
            "label": "fixture-host",
            "kind": "local",
            "shell": parse(RECORDED_SHELL),
            "serverConfig": parse(USAGE_CONFIG),
            "usageSummary": summary.map(parse),
        }],
        "ui": { "usage": { "metric": metric, "windowDays": days } },
    })
    .to_string()
}

/// Loads `fixture`, opens `/usage`, and mounts the workspace.
fn usage(fixture: String, window: &mut Window, cx: &mut App) -> AnyView {
    let state = fixtures::load(&fixture, cx).expect("fixture should decode");
    state.update(cx, |state: &mut AppState, cx| {
        state.replace_route(Route::Usage, cx)
    });
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx));
    cx.new(|_| Backdrop(workspace.into())).into()
}

/// Headless captures have no native material behind the glass; paint the opaque background the
/// web reference renders on.
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
        Scene::new("usage-cost-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "cost", 30), window, cx)
        }),
        Scene::new("usage-cost-light", ThemeMode::Light, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "cost", 30), window, cx)
        }),
        Scene::new("usage-tokens-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "tokens", 30), window, cx)
        }),
        Scene::new("usage-24h-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(Some(USAGE_24H), "cost", 1), window, cx)
        }),
        Scene::new("usage-limits-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "limits", 30), window, cx)
        }),
        Scene::new("usage-limits-light", ThemeMode::Light, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "limits", 30), window, cx)
        }),
        Scene::new("usage-loading-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(None, "cost", 30), window, cx)
        }),
        Scene::new("usage-narrow-dark", ThemeMode::Dark, |window, cx| {
            usage(usage_fixture(Some(USAGE_30D), "cost", 30), window, cx)
        })
        .size(1100., 780.),
    ]
}
