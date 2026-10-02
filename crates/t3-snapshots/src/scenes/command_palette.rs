//! The command palette over the recorded e2e workspace with the showcase thread as the route,
//! like `docs/reference/command-palette-*.png`, plus a search and the "New thread in..."
//! submenu.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div,
};
use t3_app::{command_palette::CommandPalette, state::fixtures};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const RECORDED_SHELL: &str = include_str!("../../fixtures/shell.json");
const RECORDED_CONFIG: &str = include_str!("../../fixtures/server-config.json");
const RECORDED_MANIFEST: &str = include_str!("../../fixtures/manifest.json");

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("recorded fixture is JSON")
}

/// The recorded environment with the route on the showcase thread.
fn fixture() -> String {
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
        "route": {"thread": {"environmentId": manifest["environmentId"], "threadId": "thread-aurora-tour"}},
    })
    .to_string()
}

/// The workspace with the palette layered on top, as the workspace mounts it.
struct Host {
    workspace: AnyView,
    palette: Entity<CommandPalette>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .bg(cx.colors().background)
            .child(self.workspace.clone())
            .child(self.palette.clone())
    }
}

fn palette_scene(
    window: &mut Window,
    cx: &mut App,
    act: impl FnOnce(&mut CommandPalette, &mut Window, &mut Context<CommandPalette>),
) -> AnyView {
    let app_state = fixtures::load(&fixture(), cx).expect("fixture should decode");
    let workspace = cx.new(|cx| t3_app::Workspace::new(window, cx)).into();
    let palette = cx.new(|cx| CommandPalette::new(app_state, window, cx));
    palette.update(cx, |palette, cx| {
        palette.set_open(true, window, cx);
        act(palette, window, cx);
    });
    cx.new(|_| Host { workspace, palette }).into()
}

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("command-palette-dark", ThemeMode::Dark, |window, cx| {
            palette_scene(window, cx, |_, _, _| {})
        }),
        Scene::new("command-palette-light", ThemeMode::Light, |window, cx| {
            palette_scene(window, cx, |_, _, _| {})
        }),
        Scene::new(
            "command-palette-search-dark",
            ThemeMode::Dark,
            |window, cx| {
                palette_scene(window, cx, |palette, window, cx| {
                    palette.search("aurora", window, cx)
                })
            },
        ),
        Scene::new(
            "command-palette-submenu-dark",
            ThemeMode::Dark,
            |window, cx| {
                palette_scene(window, cx, |palette, window, cx| {
                    palette.run_item("action:new-thread-in", window, cx)
                })
            },
        ),
    ]
}
