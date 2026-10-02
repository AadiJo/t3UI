//! UI integration tests for the workspace shell, driven through real key events over the
//! recorded e2e shell (`t3-snapshots/fixtures/shell.json`).
//!
//! Failure modes covered, written before the tests:
//!
//! 1. Root shortcuts listen on the workspace element, so they only fire while focus is inside
//!    it. A fresh window (nothing focused yet) must still toggle the sidebar on mod+b.
//! 2. When the focused element disappears (a dialog closes), focus must return to the
//!    workspace; otherwise every shortcut silently stops working.
//! 3. thread.next with no route thread opens the first visible thread, and thread.previous at
//!    the first thread stays put.

use gpui_kit::{
    AppContext as _, Entity, TestAppContext, WindowHandle, px, size, test::TestWindowExt as _,
};
use t3_app::{
    Workspace,
    state::{AppState, Route, fixtures},
};

/// `mod` in GPUI keystroke syntax for the platform the test runs on.
const MOD: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

fn fixture() -> String {
    serde_json::json!({
        "now": "2026-10-02T04:32:19.555Z",
        "environments": [{
            "id": "env-e2e",
            "label": "fixture-host",
            "kind": "local",
            "shell": serde_json::from_str::<serde_json::Value>(
                include_str!("../../t3-snapshots/fixtures/shell.json")
            ).unwrap(),
        }],
    })
    .to_string()
}

fn open(
    cx: &mut TestAppContext,
) -> (
    WindowHandle<gpui_kit::component::Root>,
    Entity<AppState>,
    Entity<Workspace>,
) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        t3_ui::init(t3_ui::ThemeMode::Dark, cx);
    });
    let state = cx.update(|cx| fixtures::load(&fixture(), cx).unwrap());
    let mut workspace = None;
    let handle = cx.open_window(size(px(1440.), px(900.)), |window, cx| {
        let view = cx.new(|cx| Workspace::new(window, cx));
        workspace = Some(view.clone());
        gpui_kit::component::Root::new(view, window, cx)
    });
    (handle, state, workspace.unwrap())
}

#[gpui_kit::test]
fn sidebar_toggle_works_in_a_fresh_window(cx: &mut TestAppContext) {
    let (handle, state, _) = open(cx);
    cx.update(|cx| assert!(state.read(cx).sidebar_open()));
    cx.update_window(handle.into(), |_, window, cx| {
        window.press(&format!("{MOD}-b"), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(!state.read(cx).sidebar_open()));
}

#[gpui_kit::test]
fn shortcuts_survive_a_closed_dialog(cx: &mut TestAppContext) {
    let (handle, state, workspace) = open(cx);
    cx.update_window(handle.into(), |_, window, cx| {
        let sidebar = workspace.read(cx).sidebar().clone();
        sidebar.update(cx, |sidebar, cx| {
            let key = sidebar.model().projects[0].key.clone();
            sidebar.open_project_rename(&key, window, cx);
        });
        window.render_frame(cx);
        // Escape closes the dialog and removes its focused input.
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);
        window.press(&format!("{MOD}-b"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert!(!state.read(cx).sidebar_open()));
}

#[gpui_kit::test]
fn thread_traversal_starts_at_the_first_visible_thread(cx: &mut TestAppContext) {
    let (handle, state, workspace) = open(cx);
    let first = cx.update(|cx| {
        workspace
            .read(cx)
            .sidebar()
            .read(cx)
            .model()
            .visible_threads[0]
            .clone()
    });
    cx.update_window(handle.into(), |_, window, cx| {
        window.press(&format!("{MOD}-shift-]"), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(state.read(cx).route(), &Route::Thread(first.clone())));

    cx.update_window(handle.into(), |_, window, cx| {
        window.press(&format!("{MOD}-shift-["), cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(state.read(cx).route(), &Route::Thread(first)));
}
