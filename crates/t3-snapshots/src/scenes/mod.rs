//! Scene registry. A scene builds one view in a fresh headless app, themed light or dark,
//! and is captured to `<name>.png`.
//!
//! To add scenes: create `src/scenes/<name>.rs` with `pub fn scenes() -> Vec<Scene>`, declare
//! the module below and append it in [`all`]. Keep names unique and kebab-case, ending in
//! `-dark` / `-light` when a scene exists in both appearances.

mod chat;
mod diff;
mod gallery;
mod markdown;
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
    /// Real time to let pass before the final frame, so enter animations (dialog and popover
    /// fades, toasts) finish. Zero by default: scenes with time-based state, like the
    /// terminal's blinking cursor, must not wait.
    pub settle: std::time::Duration,
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
            settle: std::time::Duration::ZERO,
            build,
        }
    }

    /// Waits for enter animations (500ms covers dialogs, popovers, and toasts) before the
    /// capture.
    pub fn settle(mut self) -> Self {
        self.settle = std::time::Duration::from_millis(500);
        self
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
    scenes.extend(chat::scenes());
    scenes.extend(gallery::scenes());
    scenes.extend(terminal::scenes());
    scenes.extend(diff::scenes());
    scenes.extend(markdown::scenes());
    scenes
}
