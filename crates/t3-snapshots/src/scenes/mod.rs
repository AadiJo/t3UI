//! Scene registry. A scene builds one view in a fresh headless app, themed light or dark,
//! and is captured to `<name>.png`.
//!
//! To add scenes: create `src/scenes/<name>.rs` with `pub fn scenes() -> Vec<Scene>`, declare
//! the module below and append it in [`all`]. Keep names unique and kebab-case, ending in
//! `-dark` / `-light` when a scene exists in both appearances.

mod gallery;
mod terminal;
mod workspace;

use gpui_kit::{AnyView, App, Window};
use t3_ui::ThemeMode;

/// One named capture.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Scene {
    pub name: &'static str,
    /// Theme the app is initialized with before `build` runs.
    pub theme: ThemeMode,
    /// Window size in logical pixels. Defaults to the 1440x900 reference captures.
    pub size: (f32, f32),
    pub build: fn(&mut Window, &mut App) -> AnyView,
}

impl Scene {
    pub fn new(
        name: &'static str,
        theme: ThemeMode,
        build: fn(&mut Window, &mut App) -> AnyView,
    ) -> Self {
        Self {
            name,
            theme,
            size: (1440., 900.),
            build,
        }
    }

    /// Overrides the window size, e.g. for tall component sheets.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = (width, height);
        self
    }
}

/// Every scene, in capture order.
pub fn all() -> Vec<Scene> {
    let mut scenes = Vec::new();
    scenes.extend(workspace::scenes());
    scenes.extend(gallery::scenes());
    scenes.extend(terminal::scenes());
    scenes
}
