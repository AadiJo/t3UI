//! The chat view on threads recorded from the e2e nightly server (`fixtures/threads/*.json`,
//! the seed behind `docs/reference/*.png`), next to the real sidebar at the reference's 256px
//! width, with the clock at seed time + 2 minutes like the reference captures. Each scene is
//! named after the reference it reproduces (`chat-<reference>-<theme>`).

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, base::TextSelectionLayer, div, px,
};
use t3_app::{
    chat::{ChatTarget, ChatView, fixtures as chat_fixtures},
    sidebar::Sidebar,
    state::{AppState, DraftId, Route, fixtures},
};
use t3_logic::{ProjectRef, ThreadRef};
use t3_ui::{ActiveColors as _, ThemeMode, tokens::font};

use super::Scene;

const SHELL: &str = include_str!("../../fixtures/shell.json");
const CONFIG: &str = include_str!("../../fixtures/server-config.json");
const MANIFEST: &str = include_str!("../../fixtures/manifest.json");
const THREADS: [(&str, &str); 8] = [
    (
        "aurora-tour",
        include_str!("../../fixtures/threads/aurora-tour.json"),
    ),
    (
        "aurora-watch",
        include_str!("../../fixtures/threads/aurora-watch.json"),
    ),
    (
        "aurora-migrate",
        include_str!("../../fixtures/threads/aurora-migrate.json"),
    ),
    (
        "aurora-plan",
        include_str!("../../fixtures/threads/aurora-plan.json"),
    ),
    (
        "borealis-persist",
        include_str!("../../fixtures/threads/borealis-persist.json"),
    ),
    (
        "borealis-ready",
        include_str!("../../fixtures/threads/borealis-ready.json"),
    ),
    (
        "cirrus-deploy",
        include_str!("../../fixtures/threads/cirrus-deploy.json"),
    ),
    (
        "cirrus-guide",
        include_str!("../../fixtures/threads/cirrus-guide.json"),
    ),
];

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("fixture is JSON")
}

/// The recorded environment as app state, every thread installed as chat fixture detail.
fn load(cx: &mut App) -> (Entity<AppState>, t3_protocol::EnvironmentId) {
    let manifest = json(MANIFEST);
    let seeded_at = manifest["seededAt"]
        .as_str()
        .expect("manifest has seededAt");
    let now = chrono::DateTime::parse_from_rfc3339(seeded_at).expect("seededAt is RFC 3339")
        + chrono::Duration::minutes(2);
    let fixture = serde_json::json!({
        "now": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "environments": [{
            "id": manifest["environmentId"],
            "label": "fixture-host",
            "kind": "local",
            "shell": json(SHELL),
            "serverConfig": json(CONFIG),
        }],
    });
    let state = fixtures::load(&fixture.to_string(), cx).expect("fixture should decode");
    let environment: t3_protocol::EnvironmentId = manifest["environmentId"]
        .as_str()
        .expect("manifest has environmentId")
        .into();
    for (_, text) in THREADS {
        let thread: t3_client::ThreadState =
            serde_json::from_str(text).expect("thread fixture decodes");
        let thread_ref = ThreadRef::new(environment.clone(), thread.thread_id.clone());
        chat_fixtures::install(thread_ref, thread, cx);
    }
    (state, environment)
}

/// What a scene shows.
#[derive(Clone, Copy)]
enum Show {
    /// A thread, scrolled to the end (the default).
    End(&'static str),
    /// A thread scrolled to the top.
    Top(&'static str),
    /// A thread with every fold and work group open, scrolled to the top.
    Expanded(&'static str),
    /// A new-thread draft in the aurora-web project.
    Draft,
}

fn build(show: Show, window: &mut Window, cx: &mut App) -> AnyView {
    let (app_state, environment) = load(cx);
    let target = match show {
        Show::End(name) | Show::Top(name) | Show::Expanded(name) => {
            let thread = ThreadRef::new(environment, format!("thread-{name}").into());
            // The reference visited the thread, which clears its sidebar "Completed" pill. The
            // view does this itself only while its window is active, which a headless one is not.
            let completed_at = THREADS
                .iter()
                .find(|(fixture, _)| *fixture == name)
                .and_then(|(_, text)| {
                    let state = json(text);
                    state["thread"]["latestTurn"]["completedAt"]
                        .as_str()
                        .map(str::to_owned)
                });
            app_state.update(cx, |state, cx| {
                state.replace_route(Route::Thread(thread.clone()), cx);
                if let Some(completed_at) = completed_at {
                    state.mark_thread_visited(&thread, &completed_at, cx);
                }
            });
            ChatTarget::Thread(thread)
        }
        Show::Draft => {
            let id = DraftId("draft-aurora".into());
            app_state.update(cx, |state, cx| {
                state.replace_route(Route::Draft(id.clone()), cx)
            });
            ChatTarget::Draft {
                id,
                project: Some(ProjectRef::new(environment, "project-aurora".into())),
            }
        }
    };
    let chat = cx.new(|cx| ChatView::new(target, app_state.clone(), window, cx));
    match show {
        Show::Top(_) => chat.update(cx, |chat, cx| chat.scroll_to_top(cx)),
        Show::Expanded(_) => chat.update(cx, |chat, cx| {
            chat.expand_all(cx);
            chat.scroll_to_top(cx);
        }),
        Show::End(_) | Show::Draft => {}
    }
    let sidebar = cx.new(|cx| Sidebar::new(app_state, window, cx));
    cx.new(|_| Frame { sidebar, chat }).into()
}

/// The workspace layout at the reference size: the 256px sidebar and the main column.
struct Frame {
    sidebar: Entity<Sidebar>,
    chat: Entity<ChatView>,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .flex()
            .font_family(font::SANS)
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(TextSelectionLayer)
            .child(
                div()
                    .w(px(256.))
                    .h_full()
                    .flex_none()
                    .child(self.sidebar.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(colors.app_main_glass)
                    .child(self.chat.clone()),
            )
    }
}

macro_rules! scene_pair {
    ($dark:literal, $light:literal, $show:expr) => {
        [
            Scene::new($dark, ThemeMode::Dark, |window, cx| {
                build($show, window, cx)
            }),
            Scene::new($light, ThemeMode::Light, |window, cx| {
                build($show, window, cx)
            }),
        ]
    };
}

pub fn scenes() -> Vec<Scene> {
    [
        scene_pair!(
            "chat-conversation-top-dark",
            "chat-conversation-top-light",
            Show::Top("aurora-tour")
        ),
        scene_pair!(
            "chat-conversation-bottom-dark",
            "chat-conversation-bottom-light",
            Show::End("aurora-tour")
        ),
        scene_pair!(
            "chat-work-log-dark",
            "chat-work-log-light",
            Show::Expanded("aurora-tour")
        ),
        scene_pair!(
            "chat-thread-running-dark",
            "chat-thread-running-light",
            Show::End("aurora-watch")
        ),
        scene_pair!(
            "chat-thread-failed-dark",
            "chat-thread-failed-light",
            Show::End("cirrus-deploy")
        ),
        scene_pair!(
            "chat-approval-pending-dark",
            "chat-approval-pending-light",
            Show::End("aurora-migrate")
        ),
        scene_pair!(
            "chat-user-input-pending-dark",
            "chat-user-input-pending-light",
            Show::End("borealis-persist")
        ),
        scene_pair!(
            "chat-plan-proposed-dark",
            "chat-plan-proposed-light",
            Show::End("aurora-plan")
        ),
        scene_pair!("chat-new-thread-dark", "chat-new-thread-light", Show::Draft),
    ]
    .into_iter()
    .flatten()
    .collect()
}
