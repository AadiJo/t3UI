//! Plan surface scenes over the recorded e2e threads (`fixtures/threads/*.json`).
//!
//! - `plan-sidebar-*`: 540px panels (the right panel's default width, below its 52px tab bar)
//!   for aurora-plan's proposed plan collapsed and expanded, and cirrus-deploy's empty state.
//! - `plan-steps-*`: step rows: aurora-watch (completed + in progress), aurora-tour at its first
//!   plan update (in progress + pending) and at its last (all completed).
//! - `plan-workspace-*`: the workspace on aurora-plan with the plan tab open, laid out like the
//!   fork capture of the same state (composer "Plan" toggle clicked).

use std::sync::Arc;

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use t3_app::{
    panels::{PanelContext, RightPanels, SurfaceKind, ThreadDetail, plan::PlanSurface},
    state::{AppState, Route, fixtures},
};
use t3_client::ThreadState;
use t3_logic::ThreadRef;
use t3_protocol::EnvironmentId;
use t3_ui::{ActiveColors as _, ThemeMode};

use super::{
    Scene,
    workspace::{Backdrop, recorded_fixture},
};

const AURORA_PLAN: &str = include_str!("../../fixtures/threads/aurora-plan.json");
const AURORA_WATCH: &str = include_str!("../../fixtures/threads/aurora-watch.json");
const AURORA_TOUR: &str = include_str!("../../fixtures/threads/aurora-tour.json");
const CIRRUS_DEPLOY: &str = include_str!("../../fixtures/threads/cirrus-deploy.json");
const MANIFEST: &str = include_str!("../../fixtures/manifest.json");

/// The right panel's default width and its height below the tab bar in a 900px window.
const PANEL: (f32, f32) = (540., 848.);

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("plan-sidebar-dark", ThemeMode::Dark, |window, cx| {
            sidebar(window, cx)
        })
        .size(PANEL.0 * 3., PANEL.1),
        Scene::new("plan-sidebar-light", ThemeMode::Light, |window, cx| {
            sidebar(window, cx)
        })
        .size(PANEL.0 * 3., PANEL.1),
        Scene::new("plan-steps-dark", ThemeMode::Dark, |window, cx| {
            steps(window, cx)
        })
        .size(PANEL.0 * 3., PANEL.1),
        Scene::new("plan-steps-light", ThemeMode::Light, |window, cx| {
            steps(window, cx)
        })
        .size(PANEL.0 * 3., PANEL.1),
        Scene::new("plan-workspace-dark", ThemeMode::Dark, workspace),
        Scene::new("plan-workspace-light", ThemeMode::Light, workspace),
    ]
}

fn thread(json: &str) -> ThreadState {
    serde_json::from_str(json).expect("thread fixture decodes")
}

/// aurora-tour as it was at its first `turn.plan.updated`.
fn tour_first_update() -> ThreadState {
    let mut state = thread(AURORA_TOUR);
    if let Some(thread) = state.thread.as_mut() {
        let mut seen = false;
        thread.activities.retain(|activity| {
            if activity.kind != "turn.plan.updated" {
                return true;
            }
            !std::mem::replace(&mut seen, true)
        });
    }
    state
}

fn environment_id() -> EnvironmentId {
    let manifest: serde_json::Value = serde_json::from_str(MANIFEST).expect("manifest is JSON");
    serde_json::from_value(manifest["environmentId"].clone())
        .expect("manifest has an environment id")
}

/// A plan surface for `state`, optionally with the proposed plan expanded.
fn surface(
    app_state: &Entity<AppState>,
    state: ThreadState,
    expanded: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<PlanSurface> {
    let context = PanelContext::new(
        app_state.clone(),
        ThreadRef::new(environment_id(), state.thread_id.clone()),
    );
    let detail = cx.new(|_| ThreadDetail::fixed(Arc::new(state)));
    cx.new(|cx| {
        let mut surface = PlanSurface::new(context, detail, window, cx);
        surface.set_expanded(expanded, cx);
        surface
    })
}

/// Panels side by side, each framed like the right panel: `background` with a left border.
struct Panels(Vec<Entity<PlanSurface>>);

impl Render for Panels {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(colors.background)
            .font_family(t3_ui::tokens::font::SANS)
            .text_color(colors.foreground)
            .children(self.0.iter().map(|surface| {
                div()
                    .flex_none()
                    .w(px(PANEL.0))
                    .h_full()
                    .border_l_1()
                    .border_color(colors.border)
                    .child(surface.clone())
            }))
    }
}

/// Loads the recorded workspace state and builds one panel per `(thread, expanded)`.
fn panels(window: &mut Window, cx: &mut App, states: Vec<(ThreadState, bool)>) -> AnyView {
    let app_state = fixtures::load(&recorded_fixture(), cx).expect("fixture should decode");
    let surfaces = states
        .into_iter()
        .map(|(state, expanded)| surface(&app_state, state, expanded, window, cx))
        .collect();
    cx.new(|_| Panels(surfaces)).into()
}

fn sidebar(window: &mut Window, cx: &mut App) -> AnyView {
    let states = vec![
        (thread(AURORA_PLAN), false),
        (thread(AURORA_PLAN), true),
        (thread(CIRRUS_DEPLOY), false),
    ];
    panels(window, cx, states)
}

fn steps(window: &mut Window, cx: &mut App) -> AnyView {
    let states = vec![
        (thread(AURORA_WATCH), false),
        (tour_first_update(), false),
        (thread(AURORA_TOUR), false),
    ];
    panels(window, cx, states)
}

/// The recorded workspace routed to aurora-plan with its plan tab open.
fn workspace(window: &mut Window, cx: &mut App) -> AnyView {
    let state = thread(AURORA_PLAN);
    let thread = ThreadRef::new(environment_id(), state.thread_id.clone());
    let app_state = fixtures::load(&recorded_fixture(), cx).expect("fixture should decode");
    app_state.update(cx, |app, cx| {
        app.navigate(Route::Thread(thread.clone()), cx)
    });
    let detail = cx.new(|_| ThreadDetail::fixed(Arc::new(state)));
    RightPanels::global(cx).update(cx, |panels, cx| {
        panels.set_detail(thread.clone(), detail);
        panels.open(&thread, SurfaceKind::Plan, cx);
    });
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx)).into();
    cx.new(|_| Backdrop(workspace)).into()
}
