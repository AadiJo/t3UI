//! Keyboard shortcuts: a direct port of the web client's matcher (`apps/web/src/keybindings.ts`)
//! and default bindings (`packages/shared/src/keybindings.ts`).
//!
//! The server owns the binding list (`ServerConfig.keybindings`); until it arrives the client uses
//! [`default_keybindings`]. Matching walks the rules from last to first and returns the first rule
//! whose `when` clause holds and whose shortcut matches, so later rules shadow earlier ones.
//! The app runs one resolver at the window root instead of translating rules into GPUI's keymap,
//! which keeps labels, conflicts, and the settings editor identical to the web client.

mod command;
mod parse;

pub use command::Command;
pub use parse::{default_keybindings, parse_shortcut, parse_when};
pub use t3_protocol::server::{KeybindingShortcut, KeybindingWhenNode, ResolvedKeybindingRule};

/// Which platform conventions apply: `mod` means Cmd on macOS and Ctrl elsewhere, and labels use
/// glyphs on macOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    /// The platform this binary was built for.
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

/// Physical modifier state at the time of a key press.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub meta: bool,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// One key press as the resolver sees it (web `ShortcutEventLike`): every key name the press may
/// match, plus the modifiers that were physically held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShortcutEvent {
    keys: Vec<String>,
    modifiers: Modifiers,
}

impl ShortcutEvent {
    /// Builds an event from a GPUI key name (`Keystroke::key`) and the physically held
    /// modifiers (the keystroke's, with shift also taken from the window's modifier state).
    ///
    /// GPUI names keys differently from DOM `event.key` (`space`, `up`, ...) and, on macOS,
    /// reports a shifted symbol with shift cleared (Cmd+Shift+[ arrives as `{`). The web matcher
    /// recovers the physical key from `event.code` for digits and brackets; this does the same
    /// from the US layout so `mod+shift+[` and `mod+shift+1` keep matching.
    pub fn from_gpui(key: &str, modifiers: Modifiers) -> Self {
        let primary = match key {
            "space" => " ".to_owned(),
            "up" => "arrowup".to_owned(),
            "down" => "arrowdown".to_owned(),
            "left" => "arrowleft".to_owned(),
            "right" => "arrowright".to_owned(),
            "esc" => "escape".to_owned(),
            other => other.to_lowercase(),
        };
        let mut keys = vec![primary];
        if modifiers.shift
            && let Some(base) = shifted_symbol_base(&keys[0])
        {
            keys.push(base.to_owned());
        }
        Self { keys, modifiers }
    }

    /// The modifiers held during the press.
    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// The key as GPUI reported it, normalized to DOM naming (`escape`, `arrowup`, `" "`).
    pub fn key(&self) -> &str {
        &self.keys[0]
    }

    fn matches_key(&self, key: &str) -> bool {
        self.keys.iter().any(|candidate| candidate == key)
    }
}

/// The unshifted US-layout key under a shifted digit or bracket, mirroring the web matcher's
/// `Digit0-9` / `BracketLeft` / `BracketRight` code aliases.
fn shifted_symbol_base(key: &str) -> Option<&'static str> {
    Some(match key {
        ")" => "0",
        "!" => "1",
        "@" => "2",
        "#" => "3",
        "$" => "4",
        "%" => "5",
        "^" => "6",
        "&" => "7",
        "*" => "8",
        "(" => "9",
        "{" => "[",
        "}" => "]",
        _ => return None,
    })
}

/// Boolean flags that `when` clauses read. Identifiers the client does not provide read false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShortcutContext {
    /// Focus is inside a terminal.
    pub terminal_focus: bool,
    /// The route thread's terminal drawer is open.
    pub terminal_open: bool,
    /// Focus is inside the preview panel.
    pub preview_focus: bool,
    /// The route thread's active right-panel surface is the preview.
    pub preview_open: bool,
    /// The composer model picker is open.
    pub model_picker_open: bool,
    /// Focus is in an editable text field (upstream `editableFocus`).
    pub editable_focus: bool,
    /// The usage page is showing (upstream `usagePageOpen`).
    pub usage_page_open: bool,
}

impl ShortcutContext {
    /// Value of a `when` identifier. `isDesktop` is always true: this is the desktop client, and
    /// upstream gates `thread.jump.N` on it.
    pub fn flag(&self, name: &str) -> bool {
        match name {
            "terminalFocus" => self.terminal_focus,
            "terminalOpen" => self.terminal_open,
            "previewFocus" => self.preview_focus,
            "previewOpen" => self.preview_open,
            "modelPickerOpen" => self.model_picker_open,
            "editableFocus" => self.editable_focus,
            "usagePageOpen" => self.usage_page_open,
            "isDesktop" => true,
            _ => false,
        }
    }
}

/// Evaluates a `when` AST. Unknown node types never hold, so a binding from a newer server with a
/// condition this client cannot read stays inactive.
pub fn evaluate_when(node: &KeybindingWhenNode, context: &ShortcutContext) -> bool {
    match node {
        KeybindingWhenNode::Identifier { name } => match name.as_str() {
            "true" => true,
            "false" => false,
            other => context.flag(other),
        },
        KeybindingWhenNode::Not { node } => !evaluate_when(node, context),
        KeybindingWhenNode::And { left, right } => {
            evaluate_when(left, context) && evaluate_when(right, context)
        }
        KeybindingWhenNode::Or { left, right } => {
            evaluate_when(left, context) || evaluate_when(right, context)
        }
        KeybindingWhenNode::Unknown => false,
    }
}

fn rule_is_active(rule: &ResolvedKeybindingRule, context: &ShortcutContext) -> bool {
    rule.when_ast
        .as_ref()
        .is_none_or(|node| evaluate_when(node, context))
}

/// The meta and ctrl state a shortcut expects once `mod` is resolved for `platform`.
fn expected_meta_ctrl(shortcut: &KeybindingShortcut, platform: Platform) -> (bool, bool) {
    let mod_is_meta = platform == Platform::Mac;
    (
        shortcut.meta_key || (shortcut.mod_key && mod_is_meta),
        shortcut.ctrl_key || (shortcut.mod_key && !mod_is_meta),
    )
}

/// True when the held modifiers are exactly the shortcut's (extra modifiers do not match).
pub fn matches_modifiers(
    modifiers: Modifiers,
    shortcut: &KeybindingShortcut,
    platform: Platform,
) -> bool {
    let (meta, ctrl) = expected_meta_ctrl(shortcut, platform);
    modifiers.meta == meta
        && modifiers.ctrl == ctrl
        && modifiers.shift == shortcut.shift_key
        && modifiers.alt == shortcut.alt_key
}

fn matches_shortcut(
    event: &ShortcutEvent,
    shortcut: &KeybindingShortcut,
    platform: Platform,
) -> bool {
    matches_modifiers(event.modifiers, shortcut, platform) && event.matches_key(&shortcut.key)
}

/// The command a key press triggers (web `resolveShortcutCommand`): the last active rule whose
/// shortcut matches.
pub fn resolve_command(
    event: &ShortcutEvent,
    rules: &[ResolvedKeybindingRule],
    context: &ShortcutContext,
    platform: Platform,
) -> Option<Command> {
    rules
        .iter()
        .rev()
        .filter(|rule| rule_is_active(rule, context))
        .find(|rule| matches_shortcut(event, &rule.shortcut, platform))
        .map(|rule| Command::parse(&rule.command))
}

/// Identity of a chord after `mod` resolution, used to find shadowed bindings.
fn conflict_key(
    shortcut: &KeybindingShortcut,
    platform: Platform,
) -> (String, bool, bool, bool, bool) {
    let (meta, ctrl) = expected_meta_ctrl(shortcut, platform);
    (
        shortcut.key.clone(),
        meta,
        ctrl,
        shortcut.shift_key,
        shortcut.alt_key,
    )
}

/// The shortcut that actually triggers `command` in `context`, skipping chords a later active rule
/// already claims (web `findEffectiveShortcutForCommand`).
pub fn effective_shortcut<'a>(
    rules: &'a [ResolvedKeybindingRule],
    command: &Command,
    context: &ShortcutContext,
    platform: Platform,
) -> Option<&'a KeybindingShortcut> {
    let mut claimed = Vec::new();
    for rule in rules.iter().rev() {
        if !rule_is_active(rule, context) {
            continue;
        }
        let key = conflict_key(&rule.shortcut, platform);
        if claimed.contains(&key) {
            continue;
        }
        claimed.push(key);
        if Command::parse(&rule.command) == *command {
            return Some(&rule.shortcut);
        }
    }
    None
}

/// Display label for `command` (for tooltips such as "Toggle main sidebar (⌘B)"), or `None` when
/// it is unbound or shadowed.
pub fn shortcut_label(
    rules: &[ResolvedKeybindingRule],
    command: &Command,
    context: &ShortcutContext,
    platform: Platform,
) -> Option<String> {
    effective_shortcut(rules, command, context, platform)
        .map(|shortcut| format_shortcut(shortcut, platform))
}

fn format_key(key: &str) -> String {
    match key {
        " " => "Space".to_owned(),
        "escape" => "Esc".to_owned(),
        "arrowup" => "Up".to_owned(),
        "arrowdown" => "Down".to_owned(),
        "arrowleft" => "Left".to_owned(),
        "arrowright" => "Right".to_owned(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

/// Formats a shortcut the way the web client does: `⌃⌥⇧⌘K` on macOS, `Ctrl+Alt+Shift+Meta+K`
/// elsewhere.
pub fn format_shortcut(shortcut: &KeybindingShortcut, platform: Platform) -> String {
    let key = format_key(&shortcut.key);
    let (meta, ctrl) = expected_meta_ctrl(shortcut, platform);
    let (alt, shift) = (shortcut.alt_key, shortcut.shift_key);
    if platform == Platform::Mac {
        let mut label = String::new();
        for (held, glyph) in [
            (ctrl, '\u{2303}'),
            (alt, '\u{2325}'),
            (shift, '\u{21e7}'),
            (meta, '\u{2318}'),
        ] {
            if held {
                label.push(glyph);
            }
        }
        label.push_str(&key);
        return label;
    }
    let mut parts: Vec<&str> = Vec::new();
    for (held, name) in [
        (ctrl, "Ctrl"),
        (alt, "Alt"),
        (shift, "Shift"),
        (meta, "Meta"),
    ] {
        if held {
            parts.push(name);
        }
    }
    parts.push(&key);
    parts.join("+")
}

/// True when the held modifiers alone match some effective `thread.jump.N` shortcut, which is
/// when the sidebar shows its jump-hint pills (web `shouldShowThreadJumpHintsForModifiers`).
pub fn should_show_thread_jump_hints(
    modifiers: Modifiers,
    rules: &[ResolvedKeybindingRule],
    context: &ShortcutContext,
    platform: Platform,
) -> bool {
    (1..=9).any(|index| {
        effective_shortcut(rules, &Command::ThreadJump(index), context, platform)
            .is_some_and(|shortcut| matches_modifiers(modifiers, shortcut, platform))
    })
}

#[cfg(test)]
mod tests;
