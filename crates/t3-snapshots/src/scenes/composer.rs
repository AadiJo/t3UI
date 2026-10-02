//! The composer in each state the reference captures show: empty draft, chips, images, model
//! picker, traits menu, slash and `@` menus, pending approval, pending question, and a running
//! turn with Stop.
//!
//! Data is the recorded e2e nightly seed (`fixtures/shell.json`, `server-config.json`,
//! `threads/*.json`), the same seed behind `docs/reference/*.png`. The composer sits where the
//! chat view puts it in a 1440×900 window with the 256px sidebar open (`ChatView`'s overlay:
//! 8px top padding, 20px inset, 768px column, lower chrome with the branch toolbar), so a crop
//! of the bottom of each PNG lines up with the reference.

use std::sync::Arc;

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use t3_app::{
    composer::{BranchToolbar, Composer, ComposerTarget, DraftStore},
    state::{AppState, NewThreadRequest, Route, fixtures},
};
use t3_client::ThreadState;
use t3_logic::{ProjectRef, ThreadRef};
use t3_protocol::{
    EnvironmentId, ProjectId, ThreadId,
    projects::{EntryKind, ProjectEntry},
};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const SHELL: &str = include_str!("../../fixtures/shell.json");
const CONFIG: &str = include_str!("../../fixtures/server-config.json");
const MANIFEST: &str = include_str!("../../fixtures/manifest.json");
const TOUR: &str = include_str!("../../fixtures/threads/aurora-tour.json");
const MIGRATE: &str = include_str!("../../fixtures/threads/aurora-migrate.json");
const WATCH: &str = include_str!("../../fixtures/threads/aurora-watch.json");
const PERSIST: &str = include_str!("../../fixtures/threads/borealis-persist.json");

/// The recorded environment, with every seeded repo on `main`.
fn fixture() -> (String, EnvironmentId) {
    let parse =
        |json: &str| -> serde_json::Value { serde_json::from_str(json).expect("fixture JSON") };
    let manifest = parse(MANIFEST);
    let environment_id = manifest["environmentId"].clone();
    let seeded_at = manifest["seededAt"].as_str().expect("seededAt");
    let now = chrono::DateTime::parse_from_rfc3339(seeded_at).expect("RFC 3339")
        + chrono::Duration::minutes(2);
    let vcs: Vec<serde_json::Value> = manifest["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|project| {
            serde_json::json!({
                "environmentId": environment_id,
                "cwd": project["workspaceRoot"],
                "local": {"isRepo": true, "refName": "main", "hasPrimaryRemote": false},
            })
        })
        .collect();
    let json = serde_json::json!({
        "now": now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "environments": [{
            "id": environment_id,
            "label": "fixture-host",
            "kind": "local",
            "shell": parse(SHELL),
            "serverConfig": parse(CONFIG),
        }],
        "vcs": vcs,
    });
    let id = EnvironmentId::from(environment_id.as_str().expect("environment id"));
    (json.to_string(), id)
}

/// Where the chat view would put the composer, on the page background.
struct Host {
    composer: Entity<Composer>,
    toolbar: Entity<BranchToolbar>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let chrome = colors.card.opacity(if colors.is_dark { 0.45 } else { 0.2 });
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(colors.background)
            .text_color(colors.foreground)
            .font_family(t3_ui::tokens::font::SANS)
            .child(
                div()
                    .w(px(256.))
                    .h_full()
                    .flex_none()
                    .border_r_1()
                    .border_color(colors.border)
                    .bg(colors.app_chrome_background),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .child(
                        div().pt(px(8.)).px(px(20.)).child(
                            div()
                                .relative()
                                .mx_auto()
                                .w_full()
                                .max_w(px(768.))
                                .child(self.composer.clone()),
                        ),
                    )
                    .child(
                        div()
                            .mt(px(-1.))
                            .mr(px(6.))
                            .pt(px(1.))
                            .pb(px(4.))
                            .px(px(20.))
                            .bg(chrome)
                            .child(self.toolbar.clone()),
                    ),
            )
    }
}

/// Loads the fixture, builds a composer for `target`, lets `setup` drive it, and mounts it.
fn scene(
    window: &mut Window,
    cx: &mut App,
    target: impl FnOnce(&EnvironmentId, &Entity<AppState>, &mut App) -> ComposerTarget,
    thread: Option<&str>,
    setup: impl FnOnce(&mut Composer, &mut Window, &mut Context<Composer>),
) -> AnyView {
    let (json, environment_id) = fixture();
    let app_state = fixtures::load(&json, cx).expect("fixture should decode");
    DraftStore::global(cx);
    let target = target(&environment_id, &app_state, cx);
    let environment = app_state
        .read(cx)
        .environment(&environment_id, cx)
        .expect("fixture environment");
    let composer = cx.new(|cx| {
        let mut composer = Composer::new(environment.clone(), target.clone(), window, cx);
        if let Some(json) = thread {
            let state: ThreadState = serde_json::from_str(json).expect("thread fixture");
            composer.set_thread_state(Arc::new(state), cx);
        }
        setup(&mut composer, window, cx);
        composer
    });
    let toolbar = cx.new(|cx| BranchToolbar::new(environment, target, cx));
    cx.new(|_| Host { composer, toolbar }).into()
}

fn thread(
    id: &'static str,
) -> impl FnOnce(&EnvironmentId, &Entity<AppState>, &mut App) -> ComposerTarget {
    move |environment, _, _| {
        ComposerTarget::Thread(ThreadRef::new(environment.clone(), ThreadId::from(id)))
    }
}

/// A fresh draft in aurora-web, opened the way Ctrl+N does.
fn new_draft(
    environment: &EnvironmentId,
    app_state: &Entity<AppState>,
    cx: &mut App,
) -> ComposerTarget {
    app_state.update(cx, |state, cx| {
        state.request_new_thread(
            NewThreadRequest {
                project: ProjectRef::new(environment.clone(), ProjectId::from("project-aurora")),
                branch: None,
                worktree_path: None,
                env_mode: None,
                start_from_origin: None,
            },
            cx,
        )
    });
    match app_state.read(cx).route() {
        Route::Draft(id) => ComposerTarget::Draft(id.clone()),
        other => panic!("new thread should open a draft, got {other:?}"),
    }
}

/// A 2×2 checkerboard-ish PNG to stand in for a pasted screenshot.
fn sample_png(hue: u8) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(96, 96, |x, y| {
        let band = ((x / 24 + y / 24) % 2) as u8;
        image::Rgba([hue, 120 + band * 60, 200 - band * 80, 255])
    });
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode png");
    bytes
}

fn both(name: &'static str, build: fn(&mut Window, &mut App) -> AnyView) -> [Scene; 2] {
    let dark: &'static str = Box::leak(format!("{name}-dark").into_boxed_str());
    let light: &'static str = Box::leak(format!("{name}-light").into_boxed_str());
    [
        Scene::new(dark, ThemeMode::Dark, build),
        Scene::new(light, ThemeMode::Light, build),
    ]
}

pub fn scenes() -> Vec<Scene> {
    let mut scenes = Vec::new();
    scenes.extend(both("composer-new-thread", |window, cx| {
        scene(window, cx, new_draft, None, |_, _, _| {})
    }));
    scenes.extend(both("composer-chips", |window, cx| {
        scene(window, cx, thread("thread-aurora-tour"), Some(TOUR), |composer, window, cx| {
            composer.set_prompt(
                "Compare [format.ts](src/format.ts) with [format.test.ts](test/format.test.ts) and draft notes with $changelog then summarize",
                window,
                cx,
            );
        })
    }));
    scenes.push(Scene::new(
        "composer-images-dark",
        ThemeMode::Dark,
        |window, cx| {
            scene(
                window,
                cx,
                thread("thread-aurora-tour"),
                Some(TOUR),
                |composer, window, cx| {
                    composer.add_image("screenshot.png", "image/png", sample_png(40), cx);
                    composer.add_image("diagram.png", "image/png", sample_png(200), cx);
                    composer.set_prompt("What changed between these two?", window, cx);
                },
            )
        },
    ));
    scenes.extend(both("composer-model-picker", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-aurora-tour"),
            Some(TOUR),
            |composer, window, cx| {
                composer.show_model_picker(window, cx);
            },
        )
    }));
    scenes.extend(both("composer-traits", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-aurora-tour"),
            Some(TOUR),
            |composer, _, cx| {
                composer.show_traits(cx);
            },
        )
    }));
    scenes.extend(both("composer-slash-menu", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-aurora-tour"),
            Some(TOUR),
            |composer, window, cx| {
                composer.set_prompt("/", window, cx);
                composer.focus(window, cx);
            },
        )
    }));
    scenes.push(Scene::new(
        "composer-path-menu-dark",
        ThemeMode::Dark,
        |window, cx| {
            scene(
                window,
                cx,
                thread("thread-aurora-tour"),
                Some(TOUR),
                |composer, window, cx| {
                    composer.set_prompt("Look at @for", window, cx);
                    let entry = |path: &str, kind: EntryKind| ProjectEntry {
                        path: path.into(),
                        kind,
                        ignored: None,
                    };
                    composer.preview_path_results(
                        vec![
                            entry("src/format.ts", EntryKind::File),
                            entry("test/format.test.ts", EntryKind::File),
                            entry("src/formatters", EntryKind::Directory),
                        ],
                        cx,
                    );
                    composer.focus(window, cx);
                },
            )
        },
    ));
    scenes.push(Scene::new(
        "composer-skill-menu-dark",
        ThemeMode::Dark,
        |window, cx| {
            scene(
                window,
                cx,
                thread("thread-aurora-tour"),
                Some(TOUR),
                |composer, window, cx| {
                    composer.set_prompt("Use $", window, cx);
                    composer.focus(window, cx);
                },
            )
        },
    ));
    scenes.extend(both("composer-approval", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-aurora-migrate"),
            Some(MIGRATE),
            |_, _, _| {},
        )
    }));
    scenes.extend(both("composer-user-input", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-borealis-persist"),
            Some(PERSIST),
            |_, _, _| {},
        )
    }));
    scenes.extend(both("composer-running", |window, cx| {
        scene(
            window,
            cx,
            thread("thread-aurora-watch"),
            Some(WATCH),
            |_, _, _| {},
        )
    }));
    scenes
}
