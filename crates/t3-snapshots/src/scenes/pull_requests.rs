//! The Pull Requests page (`docs/spec/pull-requests.md` section 8) over the recorded e2e
//! environment, with the clock at seed time + 2 minutes.
//!
//! - `empty`: the recorded nightly's real answer (its fixture repos have no remotes, so the
//!   listing is empty with no providers).
//! - `error`: the real `PullRequestUnavailableError` text for an unauthenticated `gh`.
//! - `list`: `fixtures/pull-requests/list.json`, a contract-shaped listing covering every group,
//!   state, check and review glyph, labels and the conflict badge, with line counts arriving
//!   through `listStats` (`stats.json`).
//! - `unavailable`: a server without the `pullRequests` capability.
//! - `loading`: a listing that never answers.

use std::sync::Arc;

use gpui_kit::{
    AnyView, App, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    Window, div,
};
use t3_app::{
    pull_requests::data::{Fixture, install_fixture},
    state::{AppState, Route, fixtures},
};
use t3_protocol::{
    EnvironmentId,
    pull_requests::{PullRequestDiffStat, PullRequestListResult},
};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const RECORDED_SHELL: &str = include_str!("../../fixtures/shell.json");
const RECORDED_CONFIG: &str = include_str!("../../fixtures/server-config.json");
const RECORDED_MANIFEST: &str = include_str!("../../fixtures/manifest.json");
const LIST: &str = include_str!("../../fixtures/pull-requests/list.json");
const STATS: &str = include_str!("../../fixtures/pull-requests/stats.json");

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("recorded fixture is JSON")
}

/// The recorded environment, optionally without the pull requests capability.
fn workspace_fixture(capable: bool) -> String {
    let manifest = parse(RECORDED_MANIFEST);
    let seeded_at = manifest["seededAt"]
        .as_str()
        .expect("manifest has seededAt");
    let now = chrono::DateTime::parse_from_rfc3339(seeded_at).expect("seededAt is RFC 3339")
        + chrono::Duration::minutes(2);
    let mut config = parse(RECORDED_CONFIG);
    config["environment"]["capabilities"]["pullRequests"] = capable.into();
    serde_json::json!({
        "now": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "environments": [{
            "id": manifest["environmentId"],
            "label": "fixture-host",
            "kind": "local",
            "shell": parse(RECORDED_SHELL),
            "serverConfig": config,
        }],
    })
    .to_string()
}

fn environment_id() -> EnvironmentId {
    EnvironmentId(
        parse(RECORDED_MANIFEST)["environmentId"]
            .as_str()
            .expect("manifest has environmentId")
            .to_owned(),
    )
}

/// What each scene's server answers.
enum Answer {
    List(&'static str),
    Error(&'static str),
    Pending,
    Incapable,
}

fn page(answer: Answer, window: &mut Window, cx: &mut App) -> AnyView {
    let capable = !matches!(answer, Answer::Incapable);
    let environment = environment_id();
    let mut fixture = Fixture::default();
    match answer {
        Answer::List(json) => {
            let list: PullRequestListResult =
                serde_json::from_str(json).expect("pull request list fixture decodes");
            let stats: Vec<PullRequestDiffStat> =
                serde_json::from_str(STATS).expect("stats fixture decodes");
            fixture.lists.insert(environment.clone(), Ok(list));
            fixture.stats.insert(environment, stats);
        }
        Answer::Error(message) => {
            fixture.lists.insert(environment, Err(message.to_owned()));
        }
        Answer::Pending => fixture.pending = true,
        Answer::Incapable => {}
    }
    install_fixture(fixture, cx);
    let state = fixtures::load(&workspace_fixture(capable), cx).expect("fixture should decode");
    state.update(cx, |state: &mut AppState, cx| {
        state.replace_route(Route::PullRequests, cx)
    });
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx));
    cx.new(|_| Backdrop(Arc::new(workspace.into()))).into()
}

/// The opaque `background` the web reference renders on (headless captures have no glass).
struct Backdrop(Arc<AnyView>);

impl Render for Backdrop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.colors().background)
            .child((*self.0).clone())
    }
}

/// The empty listing the recorded nightly answered with.
const EMPTY: &str =
    r#"{"entries":[],"errors":[],"nextCursors":{},"providers":[],"truncated":false,"viewers":{}}"#;
/// `PullRequestUnavailableError { reason: "cli-unauthenticated", provider: "github" }`.
const UNAUTHENTICATED: &str = "GitHub CLI is not authenticated. Run `gh auth login` and retry.";

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("pull-requests-list-dark", ThemeMode::Dark, |window, cx| {
            page(Answer::List(LIST), window, cx)
        }),
        Scene::new(
            "pull-requests-list-light",
            ThemeMode::Light,
            |window, cx| page(Answer::List(LIST), window, cx),
        ),
        Scene::new("pull-requests-empty-dark", ThemeMode::Dark, |window, cx| {
            page(Answer::List(EMPTY), window, cx)
        }),
        Scene::new("pull-requests-error-dark", ThemeMode::Dark, |window, cx| {
            page(Answer::Error(UNAUTHENTICATED), window, cx)
        }),
        Scene::new(
            "pull-requests-unavailable-dark",
            ThemeMode::Dark,
            |window, cx| page(Answer::Incapable, window, cx),
        ),
        Scene::new(
            "pull-requests-loading-dark",
            ThemeMode::Dark,
            |window, cx| page(Answer::Pending, window, cx),
        ),
    ]
}
