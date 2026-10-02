//! The application menu bar (spec 6.3, `desk/window/DesktopApplicationMenu.ts`).
//!
//! Accelerators are bound under a key context no element sets ([`MENU_CONTEXT`]), so the menu
//! shows them and AppKit fires them only after the window declines the key. That keeps Electron's
//! order: the page's shortcuts first (`terminal.close` on ⌘W in a terminal), the menu second
//! (Close Window).

use gpui_kit::{
    App, KeyBinding, Menu, MenuItem, OsAction, PromptLevel, SystemMenuType, actions,
    component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo},
};

use crate::state::{AppState, Route, SettingsPage};

/// Key context that never matches in the element tree; see the module docs.
const MENU_CONTEXT: &str = "T3AppMenu";

actions!(
    t3_app_menu,
    [
        /// "About T3 Code".
        About,
        /// "Check for Updates...".
        CheckForUpdates,
        /// "Settings..." (⌘,).
        OpenSettings,
        Hide,
        HideOthers,
        ShowAll,
        Quit,
        CloseWindow,
        Minimize,
        Zoom,
        ToggleFullScreen,
        BringAllToFront,
    ]
);

/// Binds the menu accelerators, registers the handlers, and installs the menu bar. Call once
/// after [`AppState::init`].
pub fn install(cx: &mut App) {
    let mac = cfg!(target_os = "macos");
    let mut bindings = vec![
        KeyBinding::new("cmd-,", OpenSettings, Some(MENU_CONTEXT)),
        KeyBinding::new("cmd-h", Hide, Some(MENU_CONTEXT)),
        KeyBinding::new("alt-cmd-h", HideOthers, Some(MENU_CONTEXT)),
        KeyBinding::new("cmd-q", Quit, Some(MENU_CONTEXT)),
        KeyBinding::new("cmd-w", CloseWindow, Some(MENU_CONTEXT)),
        KeyBinding::new("cmd-m", Minimize, Some(MENU_CONTEXT)),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, Some(MENU_CONTEXT)),
    ];
    if !mac {
        bindings.push(KeyBinding::new("ctrl-,", OpenSettings, Some(MENU_CONTEXT)));
    }
    cx.bind_keys(bindings);

    cx.on_action(|_: &OpenSettings, cx| {
        let state = AppState::global(cx);
        state.update(cx, |state, cx| {
            state.navigate(Route::Settings(SettingsPage::General), cx)
        });
    });
    cx.on_action(|_: &About, cx| {
        prompt_in_active_window(
            "T3 Code",
            Some(&format!("Version {}", env!("CARGO_PKG_VERSION"))),
            cx,
        );
    });
    cx.on_action(|_: &CheckForUpdates, cx| {
        prompt_in_active_window(
            "Updates unavailable",
            Some("Automatic updates are not available right now."),
            cx,
        );
    });
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &CloseWindow, cx| with_active_window(cx, |window| window.remove_window()));
    cx.on_action(|_: &Minimize, cx| with_active_window(cx, |window| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_active_window(cx, |window| window.zoom_window()));
    cx.on_action(|_: &ToggleFullScreen, cx| {
        with_active_window(cx, |window| window.toggle_fullscreen())
    });
    cx.on_action(|_: &BringAllToFront, cx| cx.activate(true));

    let edit = Menu::new("Edit").items([
        MenuItem::os_action("Undo", Undo, OsAction::Undo),
        MenuItem::os_action("Redo", Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", Cut, OsAction::Cut),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
        MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
    ]);
    let view = Menu::new("View").items([MenuItem::action("Toggle Full Screen", ToggleFullScreen)]);
    let window = Menu::new("Window").items([
        MenuItem::action("Minimize", Minimize),
        MenuItem::action("Zoom", Zoom),
        MenuItem::separator(),
        MenuItem::action("Bring All to Front", BringAllToFront),
    ]);
    let help = Menu::new("Help").items([MenuItem::action("Check for Updates...", CheckForUpdates)]);

    let menus = if mac {
        vec![
            Menu::new("T3 Code").items([
                MenuItem::action("About T3 Code", About),
                MenuItem::action("Check for Updates...", CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Settings...", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide T3 Code", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit T3 Code", Quit),
            ]),
            Menu::new("File").items([MenuItem::action("Close Window", CloseWindow)]),
            edit,
            view,
            window,
            help,
        ]
    } else {
        vec![
            Menu::new("File").items([
                MenuItem::action("Settings...", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ]),
            edit,
            view,
            window,
            help,
        ]
    };
    cx.set_menus(menus);
}

fn with_active_window(cx: &mut App, f: impl FnOnce(&mut gpui_kit::Window)) {
    if let Some(handle) = cx.active_window() {
        handle.update(cx, |_, window, _| f(window)).ok();
    }
}

/// Shows an informational message box with a single OK button.
fn prompt_in_active_window(message: &str, detail: Option<&str>, cx: &mut App) {
    let Some(handle) = cx.active_window() else {
        return;
    };
    handle
        .update(cx, |_, window, cx| {
            // The answer is not needed; dropping the receiver leaves the box open.
            drop(window.prompt(PromptLevel::Info, message, detail, &["OK"], cx));
        })
        .ok();
}
