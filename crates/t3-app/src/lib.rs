//! The T3 Code desktop app: window shell and views. `main.rs` only boots it, so the
//! snapshot renderer can mount the same views headlessly.
//!
//! - [`state`]: the global [`state::AppState`] (route, environments, settings, persisted UI
//!   state). Every view reads it; see its module docs for the read/navigate/subscribe API.
//! - [`workspace`]: the root view. The main column mounts views per route through
//!   [`workspace::build_main_view`].
//! - [`sidebar`], [`keybindings`], [`toast`], [`dialogs`]: shell pieces other views reuse.

pub mod chrome;
pub mod dialogs;
pub mod keybindings;
pub mod notifications;
pub mod sidebar;
pub mod state;
pub mod toast;
pub mod workspace;

use gpui_kit::AppContext as _;
use t3_logic::ui_state::ThemePreference;

pub use workspace::Workspace;

/// The persisted theme preference in t3-ui's terms.
pub fn theme_mode(preference: ThemePreference) -> t3_ui::ThemeMode {
    match preference {
        ThemePreference::System => t3_ui::ThemeMode::System,
        ThemePreference::Light => t3_ui::ThemeMode::Light,
        ThemePreference::Dark => t3_ui::ThemeMode::Dark,
    }
}

/// Boots the application: bundled assets, gpui-kit and t3-ui init, and the main window
/// with its native glass. Owned by the design system; views are built by `Workspace::new`.
pub fn run() {
    gpui_kit::application()
        .with_assets(t3_ui::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let app_state = state::AppState::init(state::Store::user_data(), cx);
            state::boot::start_saved_environments(&app_state, cx);
            let theme = theme_mode(app_state.read(cx).ui().theme);
            t3_ui::init(theme, cx);
            t3_ui::theme::enable_native_appearance(cx);
            keybindings::menu::install(cx);
            let options = t3_ui::window::main_window_options(cx);
            gpui_kit::open_window(options, cx, |window, cx| {
                t3_ui::window::install_glass(window);
                t3_ui::theme::observe_system_appearance(window, cx).detach();
                cx.new(|cx| Workspace::new(window, cx))
            })
            .expect("failed to open main window");
            cx.activate(true);
        });
}
