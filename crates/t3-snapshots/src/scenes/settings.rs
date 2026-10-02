//! Settings pages and the Add Environment dialog, fed by the recorded e2e environment
//! (`fixtures/shell.json`, `server-config.json`, `archived-shell.json`) with the clock at seed
//! time + 2 minutes, like `docs/reference/settings-*.png`.
//!
//! The scenes frame the views like the workspace does (256px sidebar with the settings nav,
//! main column) so they compare 1:1 with the references.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div,
};
use t3_app::{
    settings::{
        SettingsNav, SettingsView,
        archived::{ArchivedCache, ArchivedSnapshot},
    },
    state::{AppState, Route, SettingsPage, fixtures},
};
use t3_logic::ui_state::ThemePreference;
use t3_ui::{ActiveColors as _, ThemeMode, tokens::layout};

use super::Scene;

const RECORDED_SHELL: &str = include_str!("../../fixtures/shell.json");
const RECORDED_CONFIG: &str = include_str!("../../fixtures/server-config.json");
const RECORDED_ARCHIVED: &str = include_str!("../../fixtures/archived-shell.json");
const RECORDED_MANIFEST: &str = include_str!("../../fixtures/manifest.json");

fn parse(json: &str) -> serde_json::Value {
    serde_json::from_str(json).expect("recorded fixture is JSON")
}

/// The recorded environment as a workspace fixture on `page`.
fn load(page: SettingsPage, theme: ThemePreference, cx: &mut App) -> Entity<AppState> {
    let manifest = parse(RECORDED_MANIFEST);
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
            "shell": parse(RECORDED_SHELL),
            "serverConfig": parse(RECORDED_CONFIG),
        }],
    });
    let state = fixtures::load(&fixture.to_string(), cx).expect("fixture should decode");
    let environment_id = manifest["environmentId"]
        .as_str()
        .expect("manifest has environmentId")
        .into();
    let archived = serde_json::from_str(RECORDED_ARCHIVED).expect("archived shell decodes");
    ArchivedCache::set(
        environment_id,
        ArchivedSnapshot::Loaded(std::sync::Arc::new(archived)),
        cx,
    );
    state.update(cx, |state, cx| {
        state.set_theme(theme, cx);
        state.replace_route(Route::Settings(page), cx);
    });
    state
}

/// The workspace frame around the settings view.
struct Frame {
    app_state: Entity<AppState>,
    main: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .flex()
            .bg(colors.background)
            .font_family(t3_ui::tokens::font::SANS)
            .text_color(colors.foreground)
            .child(
                div()
                    .w(layout::SIDEBAR_WIDTH)
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .bg(colors.app_sidebar_glass)
                    .border_r_1()
                    .border_color(colors.border)
                    .child(div().h(layout::TOPBAR_HEIGHT).flex_none())
                    .child(SettingsNav::new(self.app_state.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(colors.app_main_glass)
                    .child(self.main.clone()),
            )
    }
}

fn settings_scene(
    page: SettingsPage,
    theme: ThemePreference,
    window: &mut Window,
    cx: &mut App,
    after: impl FnOnce(&Entity<SettingsView>, &mut Window, &mut App),
) -> AnyView {
    let app_state = load(page, theme, cx);
    let view = cx.new(|cx| SettingsView::new(app_state.clone(), window, cx));
    after(&view, window, cx);
    cx.new(|_| Frame {
        app_state,
        main: view.into(),
    })
    .into()
}

macro_rules! page_scenes {
    ($($name:literal => $page:expr),* $(,)?) => {
        vec![$(
            Scene::new(concat!($name, "-dark"), ThemeMode::Dark, |window, cx| {
                settings_scene($page, ThemePreference::Dark, window, cx, |_, _, _| {})
            }),
            Scene::new(concat!($name, "-light"), ThemeMode::Light, |window, cx| {
                settings_scene($page, ThemePreference::Light, window, cx, |_, _, _| {})
            }),
        )*]
    };
}

pub fn scenes() -> Vec<Scene> {
    let mut scenes = page_scenes![
        "settings-general" => SettingsPage::General,
        "settings-connections" => SettingsPage::Connections,
        "settings-providers" => SettingsPage::Providers,
        "settings-archived" => SettingsPage::Archived,
    ];
    scenes.push(Scene::new(
        "settings-providers-expanded-dark",
        ThemeMode::Dark,
        |window, cx| {
            settings_scene(
                SettingsPage::Providers,
                ThemePreference::Dark,
                window,
                cx,
                |view, window, cx| {
                    view.update(cx, |view, cx| view.expand_provider("codex", window, cx))
                },
            )
        },
    ));
    scenes.push(Scene::new(
        "settings-add-environment-dark",
        ThemeMode::Dark,
        |window, cx| {
            settings_scene(
                SettingsPage::Connections,
                ThemePreference::Dark,
                window,
                cx,
                |view, window, cx| {
                    view.update(cx, |view, cx| view.open_add_environment(window, cx))
                },
            )
        },
    ));
    scenes
}
