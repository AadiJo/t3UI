//! The app's root view as the shell renders it today.

use gpui_kit::AppContext as _;
use t3_ui::ThemeMode;

use super::Scene;

pub fn scenes() -> Vec<Scene> {
    vec![Scene::new(
        "workspace-empty-dark",
        ThemeMode::Dark,
        |window, cx| cx.new(|cx| t3_app::Workspace::new(window, cx)).into(),
    )]
}
