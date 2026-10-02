//! The workspace shell: sidebar populated from shell fixtures, collapsed, and empty.
//!
//! `reference.json` is a shell snapshot recorded from the e2e nightly server (`e2e/seed.mjs`),
//! the same seed as `docs/reference/sidebar-*.png`, so `sidebar-reference-*` compares 1:1.
//! `sidebar.json` is handcrafted to cover every status, grouping, badges, and overflow.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div,
};
use t3_app::state::{AppState, Route, fixtures};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const REFERENCE: &str = include_str!("../../fixtures/reference.json");
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
            workspace(REFERENCE, window, cx, |_, _| {})
        }),
        Scene::new("sidebar-reference-light", ThemeMode::Light, |window, cx| {
            workspace(REFERENCE, window, cx, |_, _| {})
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
