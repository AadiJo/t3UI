//! Right panel scenes over the recorded e2e environment: the diff panel open beside the
//! aurora-tour thread (compare with `docs/reference/diff-panel-*.png`), the overlay sheet at a
//! narrow window, and the empty state. The turn's patch is the server's exact `git diff` output
//! for the thread's checkpoints, recorded in `fixtures/diffs/`.

use std::sync::Arc;

use gpui_kit::{AnyView, App, AppContext as _, Window};
use t3_app::{
    panels::{FixtureResponder, RightPanels, SurfaceKind, ThreadDetail},
    state::{Route, fixtures},
};
use t3_client::ThreadState;
use t3_logic::ThreadRef;
use t3_protocol::{EnvironmentId, ThreadId, TurnId};
use t3_ui::ThemeMode;

use super::{
    Scene,
    workspace::{Backdrop, recorded_fixture},
};

const AURORA_TOUR: &str = include_str!("../../fixtures/threads/aurora-tour.json");
const AURORA_TOUR_TURN_1: &str = include_str!("../../fixtures/diffs/aurora-tour-turn-1.patch");
const MANIFEST: &str = include_str!("../../fixtures/manifest.json");

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("diff-panel-dark", ThemeMode::Dark, |window, cx| {
            thread_with_panel(window, cx, Panel::LatestTurnDiff)
        }),
        Scene::new("diff-panel-light", ThemeMode::Light, |window, cx| {
            thread_with_panel(window, cx, Panel::LatestTurnDiff)
        }),
        // At or below 980px the panel is an overlay sheet.
        Scene::new("diff-panel-sheet-dark", ThemeMode::Dark, |window, cx| {
            thread_with_panel(window, cx, Panel::LatestTurnDiff)
        })
        .size(900., 700.),
        Scene::new("right-panel-empty-dark", ThemeMode::Dark, |window, cx| {
            thread_with_panel(window, cx, Panel::Empty)
        }),
    ]
}

/// What the panel shows.
enum Panel {
    LatestTurnDiff,
    /// Open with no tabs.
    Empty,
}

/// The recorded workspace routed to aurora-tour, with its right panel open.
fn thread_with_panel(window: &mut Window, cx: &mut App, panel: Panel) -> AnyView {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("manifest is JSON");
    let environment_id: EnvironmentId = serde_json::from_value(manifest["environmentId"].clone())
        .expect("manifest has an environment id");
    let state: ThreadState = serde_json::from_str(AURORA_TOUR).expect("thread fixture decodes");
    let thread_id: ThreadId = state.thread_id.clone();
    let thread = ThreadRef::new(environment_id, thread_id);

    cx.set_global(FixtureResponder(Box::new(|method, payload| {
        matches!(
            method,
            "orchestration.getTurnDiff" | "orchestration.getFullThreadDiff"
        )
        .then(|| {
            serde_json::json!({
                "threadId": payload["threadId"],
                "fromTurnCount": payload["fromTurnCount"].as_u64().unwrap_or(0),
                "toTurnCount": payload["toTurnCount"],
                "diff": AURORA_TOUR_TURN_1,
            })
        })
    })));

    let app_state = fixtures::load(&recorded_fixture(), cx).expect("fixture should decode");
    app_state.update(cx, |state, cx| {
        state.navigate(Route::Thread(thread.clone()), cx)
    });
    let latest_turn: Option<TurnId> = state.thread.as_ref().and_then(|thread| {
        thread
            .checkpoints
            .first()
            .map(|checkpoint| checkpoint.turn_id.clone())
    });
    let detail = cx.new(|_| ThreadDetail::fixed(Arc::new(state)));
    let panels = RightPanels::global(cx);
    panels.update(cx, |panels, cx| {
        panels.set_detail(thread.clone(), detail);
        match panel {
            Panel::LatestTurnDiff => match latest_turn {
                Some(turn) => panels.open_turn_diff(&thread, turn, None, cx),
                None => panels.open(&thread, SurfaceKind::Diff, cx),
            },
            Panel::Empty => panels.toggle_visibility(&thread, cx),
        }
    });
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx)).into();
    cx.new(|_| Backdrop(workspace)).into()
}
