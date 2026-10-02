//! The workspace shell: sidebar populated from a recorded shell fixture, collapsed, and empty.

use gpui_kit::{AnyView, App, AppContext as _, Window};
use t3_app::state::{AppState, Route, fixtures};
use t3_ui::ThemeMode;

use super::Scene;

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
    cx.new(|cx| t3_app::Workspace::new(window, cx)).into()
}

pub fn scenes() -> Vec<Scene> {
    vec![
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
