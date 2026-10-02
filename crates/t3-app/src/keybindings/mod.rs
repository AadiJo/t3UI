//! Keyboard shortcuts at the window root.
//!
//! The workspace's root element forwards every unhandled key-down here. It runs the web
//! client's resolver (`t3_logic::keybindings`) against the active rules and the current
//! [`ShortcutScope`] instead of GPUI's keymap, so server-defined bindings, `when` clauses, and
//! labels behave exactly like the web. GPUI keymap bindings (text inputs, menus) run first, which
//! matches the web's "skip `defaultPrevented` events".
//!
//! Views that own `when` flags publish them through [`ShortcutScope::update`]: the terminal
//! registers its focus handle, the chat view reports whether its terminal drawer or preview is
//! open, and so on.

pub mod menu;

use gpui_kit::{App, FocusHandle, Global, KeyDownEvent, Window};
use t3_logic::keybindings::{
    Command, Modifiers, Platform, ShortcutContext, ShortcutEvent, resolve_command,
};

use crate::state::AppState;

/// `when`-clause inputs owned by other views.
#[derive(Default)]
pub struct ShortcutScope {
    /// The route thread's terminal drawer is open.
    pub terminal_open: bool,
    /// The route thread's active right-panel surface is the preview.
    pub preview_open: bool,
    /// Any right-panel surface is open (lets `mod+w` close it instead of the window).
    pub right_panel_open: bool,
    /// The composer's model picker is open.
    pub model_picker_open: bool,
    /// The command palette is open: global chat shortcuts stand down.
    pub command_palette_open: bool,
    terminal_focus: Vec<FocusHandle>,
    preview_focus: Vec<FocusHandle>,
}

impl Global for ShortcutScope {}

impl ShortcutScope {
    /// Edits the scope, creating it on first use.
    pub fn update(cx: &mut App, edit: impl FnOnce(&mut Self)) {
        edit(cx.default_global::<Self>());
    }

    /// Marks `handle` (and its descendants) as a terminal for `terminalFocus`.
    pub fn register_terminal_focus(&mut self, handle: FocusHandle) {
        self.terminal_focus.push(handle);
    }

    /// Marks `handle` (and its descendants) as the preview for `previewFocus`.
    pub fn register_preview_focus(&mut self, handle: FocusHandle) {
        self.preview_focus.push(handle);
    }

    /// The context `when` clauses evaluate against right now.
    pub fn context(window: &Window, cx: &App) -> ShortcutContext {
        let Some(scope) = cx.try_global::<Self>() else {
            return ShortcutContext::default();
        };
        let focused_in = |handles: &[FocusHandle]| {
            handles
                .iter()
                .any(|handle| handle.contains_focused(window, cx))
        };
        ShortcutContext {
            terminal_focus: focused_in(&scope.terminal_focus),
            terminal_open: scope.terminal_open,
            preview_focus: focused_in(&scope.preview_focus),
            preview_open: scope.preview_open,
            model_picker_open: scope.model_picker_open,
            editable_focus: false,
            usage_page_open: false,
        }
    }

    fn command_palette_open(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|scope| scope.command_palette_open)
    }

    fn right_panel_open(cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|scope| scope.right_panel_open)
    }
}

/// The modifiers physically held for a key press. The keystroke's own flags are exact except
/// shift: macOS reports Cmd+Shift+[ as `{` with shift cleared, so shift also counts when the
/// window's modifier state (from the preceding flags-changed event) has it down.
pub fn key_modifiers(event: &KeyDownEvent, window: &Window) -> Modifiers {
    let modifiers = &event.keystroke.modifiers;
    Modifiers {
        meta: modifiers.platform,
        ctrl: modifiers.control,
        shift: modifiers.shift || window.modifiers().shift,
        alt: modifiers.alt,
    }
}

/// What a key press resolved to.
pub struct ResolvedShortcut {
    pub command: Command,
    /// The key is auto-repeating (thread navigation ignores repeats).
    pub repeat: bool,
}

/// Resolves a key press against the active rules. Returns `None` when no rule matches, or when
/// the command should fall through to the application menu (`rightPanel.close` with no panel
/// open, so `mod+w` still closes the window) or to the open command palette.
pub fn resolve_key_down(
    event: &KeyDownEvent,
    window: &Window,
    cx: &App,
) -> Option<ResolvedShortcut> {
    let shortcut = ShortcutEvent::from_gpui(&event.keystroke.key, key_modifiers(event, window));
    let context = ShortcutScope::context(window, cx);
    let rules = AppState::global(cx).read(cx).keybindings(cx);
    let command = resolve_command(&shortcut, &rules, &context, Platform::current())?;
    match command {
        Command::RightPanelClose if !ShortcutScope::right_panel_open(cx) => return None,
        Command::ChatNew | Command::ChatNewLocal if ShortcutScope::command_palette_open(cx) => {
            return None;
        }
        _ => {}
    }
    Some(ResolvedShortcut {
        command,
        repeat: event.is_held,
    })
}

/// Display label for `command` in tooltips ("Toggle main sidebar (⌘B)"), with the default
/// context (nothing focused or open).
pub fn shortcut_label(command: &Command, cx: &App) -> Option<String> {
    let rules = AppState::global(cx).read(cx).keybindings(cx);
    t3_logic::keybindings::shortcut_label(
        &rules,
        command,
        &ShortcutContext::default(),
        Platform::current(),
    )
}
