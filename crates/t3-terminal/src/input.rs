//! Keyboard and mouse input to PTY bytes.
//!
//! Matches xterm.js 6.0 (MIT): `src/common/input/Keyboard.ts` for keys and
//! `src/common/services/CoreMouseService.ts` for mouse reports, which is what the reference
//! drawer runs. The fork's key intercepts (`apps/web/src/keybindings.ts:446-529`) run first.
//! Printable characters are not encoded here: the view leaves them to the platform text input
//! path (IME, dead keys, Option-composed characters), which is what xterm does on macOS with
//! `macOptionIsMeta: false`.

use std::borrow::Cow;

/// Modifier keys held during a key or mouse event. `platform` is Cmd on macOS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Modifiers {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub platform: bool,
}

/// Which platform's conventions apply. macOS treats Option as a third-level shift and maps
/// Cmd shortcuts; everything else treats Alt as Meta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    Mac,
    Other,
}

impl Platform {
    /// The platform this binary was built for.
    pub(crate) const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

/// What a key press does in the terminal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum KeyOutcome {
    /// Bytes to send to the PTY.
    Send(Cow<'static, str>),
    /// Scroll the viewport up by `rows - 1` (Shift+PageUp).
    ScrollPageUp,
    /// Scroll the viewport down by `rows - 1` (Shift+PageDown).
    ScrollPageDown,
    /// Select the whole buffer (Cmd+A on macOS).
    SelectAll,
}

const ESC: &str = "\x1b";

/// Encodes a key press. `key` is GPUI's key name (`"a"`, `"enter"`, `"left"`, `"f5"`, `"["`).
/// Returns `None` when the key produces no terminal action, so printable text can reach the
/// text input handler and app shortcuts can bubble.
pub(crate) fn encode_key(
    key: &str,
    mods: Modifiers,
    app_cursor: bool,
    platform: Platform,
) -> Option<KeyOutcome> {
    if let Some(intercept) = fork_intercept(key, mods, platform) {
        return Some(KeyOutcome::Send(Cow::Borrowed(intercept)));
    }
    // macOS Option is a third-level shift for keys that produce characters (xterm
    // `_isThirdLevelShift`, keyCode > 47): the composed character arrives as text input.
    if platform == Platform::Mac
        && mods.alt
        && !mods.control
        && !mods.platform
        && produces_character(key)
    {
        return None;
    }
    xterm_key(key, mods, app_cursor, platform)
}

/// The fork's `attachCustomKeyEventHandler` intercepts, evaluated before xterm sees the key.
fn fork_intercept(key: &str, mods: Modifiers, platform: Platform) -> Option<&'static str> {
    let Modifiers {
        control,
        alt,
        shift,
        platform: cmd,
    } = mods;
    let mac = platform == Platform::Mac;
    // isTerminalClearShortcut: Ctrl+L everywhere, Cmd+K on macOS.
    if key == "l" && control && !cmd && !alt && !shift {
        return Some("\x0c");
    }
    if mac && key == "k" && cmd && !control && !alt && !shift {
        return Some("\x0c");
    }
    // terminalDeleteShortcutData: Cmd+Backspace deletes to line start on macOS.
    if mac && key == "backspace" && cmd && !control && !alt && !shift {
        return Some("\x15");
    }
    // terminalNavigationShortcutData: word and line movement on Left/Right.
    if shift || !matches!(key, "left" | "right") {
        return None;
    }
    let (word, line) = if key == "left" {
        ("\x1bb", "\x01")
    } else {
        ("\x1bf", "\x05")
    };
    if mac {
        if alt && !cmd && !control {
            return Some(word);
        }
        if cmd && !alt && !control {
            return Some(line);
        }
        return None;
    }
    if (control && !cmd && !alt) || (alt && !cmd && !control) {
        return Some(word);
    }
    None
}

/// Whether xterm would classify the key's keyCode as > 47 (characters and F-keys), as opposed
/// to editing and navigation keys.
fn produces_character(key: &str) -> bool {
    key.chars().count() == 1 || function_key_number(key).is_some()
}

fn function_key_number(key: &str) -> Option<u8> {
    let number: u8 = key.strip_prefix('f')?.parse().ok()?;
    (1..=24).contains(&number).then_some(number)
}

/// Port of xterm.js `evaluateKeyboardEvent` (MIT) over GPUI key names.
fn xterm_key(
    key: &str,
    mods: Modifiers,
    app_cursor: bool,
    platform: Platform,
) -> Option<KeyOutcome> {
    let Modifiers {
        control,
        alt,
        shift,
        platform: meta,
    } = mods;
    let mac = platform == Platform::Mac;
    // xterm's modifier parameter: 1 + (shift 1 | alt 2 | ctrl 4 | meta 8).
    let modifier_bits =
        u8::from(shift) | (u8::from(alt) << 1) | (u8::from(control) << 2) | (u8::from(meta) << 3);
    let param = modifier_bits + 1;
    let send = |s: String| Some(KeyOutcome::Send(Cow::Owned(s)));
    let fixed = |s: &'static str| Some(KeyOutcome::Send(Cow::Borrowed(s)));
    // Cursor-style keys: CSI 1;m X with modifiers, SS3 X in application mode, CSI X otherwise.
    let cursor = |final_byte: char| {
        if modifier_bits != 0 {
            send(format!("{ESC}[1;{param}{final_byte}"))
        } else if app_cursor {
            send(format!("{ESC}O{final_byte}"))
        } else {
            send(format!("{ESC}[{final_byte}"))
        }
    };
    let tilde = |code: u8| {
        if modifier_bits != 0 {
            send(format!("{ESC}[{code};{param}~"))
        } else {
            send(format!("{ESC}[{code}~"))
        }
    };

    match key {
        "backspace" => {
            let base = if control { "\x08" } else { "\x7f" };
            if alt {
                send(format!("{ESC}{base}"))
            } else {
                fixed(base)
            }
        }
        "tab" => {
            if shift {
                fixed("\x1b[Z")
            } else {
                fixed("\t")
            }
        }
        "enter" => fixed(if alt { "\x1b\r" } else { "\r" }),
        "escape" => fixed(if alt { "\x1b\x1b" } else { "\x1b" }),
        "left" | "right" | "up" | "down" => {
            if meta {
                return None;
            }
            cursor(match key {
                "left" => 'D',
                "right" => 'C',
                "up" => 'A',
                _ => 'B',
            })
        }
        "home" => cursor('H'),
        "end" => cursor('F'),
        "insert" => (!shift && !control).then_some(KeyOutcome::Send(Cow::Borrowed("\x1b[2~"))),
        "delete" => tilde(3),
        "pageup" | "pagedown" => {
            let up = key == "pageup";
            if shift {
                Some(if up {
                    KeyOutcome::ScrollPageUp
                } else {
                    KeyOutcome::ScrollPageDown
                })
            } else if control {
                send(format!("{ESC}[{};{param}~", if up { 5 } else { 6 }))
            } else {
                fixed(if up { "\x1b[5~" } else { "\x1b[6~" })
            }
        }
        _ => {
            if let Some(number) = function_key_number(key) {
                return function_key(number, modifier_bits, param);
            }
            default_key(key, mods, mac)
        }
    }
}

fn function_key(number: u8, modifier_bits: u8, param: u8) -> Option<KeyOutcome> {
    let s = match number {
        1..=4 => {
            let final_byte = (b'P' + number - 1) as char;
            if modifier_bits != 0 {
                format!("{ESC}[1;{param}{final_byte}")
            } else {
                format!("{ESC}O{final_byte}")
            }
        }
        5..=12 => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)];
            if modifier_bits != 0 {
                format!("{ESC}[{code};{param}~")
            } else {
                format!("{ESC}[{code}~")
            }
        }
        _ => return None,
    };
    Some(KeyOutcome::Send(Cow::Owned(s)))
}

/// xterm's `default:` branch: Ctrl combinations, Alt-as-Meta, Cmd+A. Plain printable keys
/// return `None` (they arrive as text input).
fn default_key(key: &str, mods: Modifiers, mac: bool) -> Option<KeyOutcome> {
    let Modifiers {
        control,
        alt,
        shift,
        platform: meta,
    } = mods;
    let send = |s: String| Some(KeyOutcome::Send(Cow::Owned(s)));
    let letter = single_ascii(key).filter(char::is_ascii_lowercase);

    if control && !shift && !alt && !meta {
        let byte = match key {
            "space" => 0,
            "[" => 0x1b,
            "\\" => 0x1c,
            "]" => 0x1d,
            "8" => 0x7f,
            // GPUI reports Ctrl+Shift+2 as "@" and Ctrl+Shift+- as "_" without shift.
            "@" => 0,
            "_" => 0x1f,
            _ => match single_ascii(key) {
                Some(c @ 'a'..='z') => c as u8 - b'a' + 1,
                Some(c @ '3'..='7') => c as u8 - b'3' + 0x1b,
                _ => return None,
            },
        };
        return send(char::from(byte).to_string());
    }
    if !mac && alt && !meta {
        if let Some(c) = letter {
            let c = if control {
                char::from(c as u8 - b'a' + 1)
            } else if shift {
                c.to_ascii_uppercase()
            } else {
                c
            };
            return send(format!("{ESC}{c}"));
        }
        if key == "space" {
            return send(format!("{ESC}{}", if control { "\0" } else { " " }));
        }
        if let Some(c) = single_ascii(key).filter(|c| c.is_ascii_graphic()) {
            return send(format!("{ESC}{c}"));
        }
        return None;
    }
    if mac && meta && !alt && !control && !shift {
        return (key == "a").then_some(KeyOutcome::SelectAll);
    }
    if control {
        return match key {
            "_" => send("\x1f".into()),
            "@" => send("\0".into()),
            _ => None,
        };
    }
    None
}

fn single_ascii(key: &str) -> Option<char> {
    let mut chars = key.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.is_ascii()).then_some(c)
}

/// Mouse buttons xterm reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseButton {
    Left,
    Middle,
    Right,
    /// No button held (motion-only reports).
    None,
    WheelUp,
    WheelDown,
}

/// What happened to the button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseAction {
    Press,
    Release,
    Move,
}

/// Which mouse events the application asked for (DECSET 1000/1002/1003).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MouseProtocol {
    /// 1000: press, release, wheel.
    Click,
    /// 1002: plus motion while a button is held.
    Drag,
    /// 1003: plus all motion.
    Any,
}

/// Encodes a mouse report (xterm `CoreMouseService`, MIT). `col` and `row` are 0-based cells
/// in the viewport. Returns `None` when the protocol filters the event or the default
/// encoding cannot address the cell.
pub(crate) fn encode_mouse(
    button: MouseButton,
    action: MouseAction,
    col: usize,
    row: usize,
    mods: Modifiers,
    protocol: MouseProtocol,
    sgr: bool,
) -> Option<String> {
    let wheel = matches!(button, MouseButton::WheelUp | MouseButton::WheelDown);
    match (protocol, action) {
        (_, MouseAction::Move) if wheel => return None,
        (MouseProtocol::Click, MouseAction::Move) => return None,
        (MouseProtocol::Drag, MouseAction::Move) if button == MouseButton::None => return None,
        (_, MouseAction::Press | MouseAction::Release) if button == MouseButton::None => {
            return None;
        }
        _ => {}
    }

    let mut code: u32 = 0;
    if mods.control {
        code |= 16;
    }
    if mods.shift {
        code |= 4;
    }
    if mods.alt {
        code |= 8;
    }
    match button {
        MouseButton::WheelUp => code |= 64,
        MouseButton::WheelDown => code |= 65,
        _ => {
            let base = match button {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
                _ => 3,
            };
            if action == MouseAction::Release && !sgr {
                // Only SGR can say which button was released.
                code |= 3;
            } else {
                code |= base;
            }
            if action == MouseAction::Move {
                code |= 32;
            }
        }
    }

    let (col, row) = (col as u32 + 1, row as u32 + 1);
    if sgr {
        let final_byte = if action == MouseAction::Release && !wheel {
            'm'
        } else {
            'M'
        };
        return Some(format!("{ESC}[<{code};{col};{row}{final_byte}"));
    }
    let params = [code + 32, col + 32, row + 32];
    if params.iter().any(|&p| p > 255) {
        return None;
    }
    let mut report = String::from("\x1b[M");
    report.extend(params.iter().filter_map(|&p| char::from_u32(p)));
    Some(report)
}

/// Wraps pasted text for the PTY (xterm `prepareTextForTerminal` + `bracketTextForPaste`):
/// newlines become CR, and bracketed paste mode adds the `ESC [200~` / `ESC [201~` guards.
pub(crate) fn paste_text(text: &str, bracketed: bool) -> String {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{normalized}\x1b[201~")
    } else {
        normalized
    }
}

/// The arrow key xterm sends for one wheel event on a buffer without scrollback (alt screen).
pub(crate) fn wheel_arrow(up: bool, app_cursor: bool) -> &'static str {
    match (up, app_cursor) {
        (true, true) => "\x1bOA",
        (true, false) => "\x1b[A",
        (false, true) => "\x1bOB",
        (false, false) => "\x1b[B",
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes covered, listed before the tests were written:
    //! 1. Printable keys (with or without Shift) get encoded here and then also arrive as text
    //!    input, doubling every character.
    //! 2. Enter/Tab/Backspace/Escape send the wrong control byte, or ignore Alt (ESC prefix) and
    //!    Shift+Tab (CSI Z) / Ctrl+Backspace (BS).
    //! 3. Arrows ignore application cursor mode (SS3) or apply it when modifiers are held
    //!    (xterm uses CSI 1;m X then); Cmd+Up/Down send something on macOS.
    //! 4. The modifier parameter is miscomputed (shift 1, alt 2, ctrl 4, meta 8, plus one).
    //! 5. Fork intercepts are missed or leak to the wrong platform: Option/Cmd+Left/Right,
    //!    Ctrl/Alt+Left/Right off macOS, Cmd+Backspace, Ctrl+L, Cmd+K; Shift disables them.
    //! 6. Home/End/Insert/Delete/PageUp/PageDown/F1-F12 sequences are wrong, Shift+PageUp does
    //!    not scroll, or Ctrl/Shift+Insert sends CSI 2~.
    //! 7. Ctrl+letter/space/[\]/3-8/@/_ map to the wrong C0 byte, or Ctrl+Shift+letter sends one.
    //! 8. Alt is treated as Meta on macOS (must compose text), or not treated as Meta elsewhere.
    //! 9. Cmd+A does not select all; other Cmd shortcuts get swallowed instead of bubbling.
    //! 10. Mouse reports: wrong button codes, modifiers, 1-based coordinates, SGR release final
    //!     byte, X10 release code, protocol filtering, or out-of-range X10 coordinates.
    //! 11. Paste keeps LF line endings or forgets the bracketed paste guards.
    use super::*;

    const NONE: Modifiers = Modifiers {
        control: false,
        alt: false,
        shift: false,
        platform: false,
    };
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..NONE
    };
    const ALT: Modifiers = Modifiers { alt: true, ..NONE };
    const CTRL: Modifiers = Modifiers {
        control: true,
        ..NONE
    };
    const CMD: Modifiers = Modifiers {
        platform: true,
        ..NONE
    };

    fn mac(key: &str, mods: Modifiers) -> Option<String> {
        sent(encode_key(key, mods, false, Platform::Mac))
    }

    fn linux(key: &str, mods: Modifiers) -> Option<String> {
        sent(encode_key(key, mods, false, Platform::Other))
    }

    fn sent(outcome: Option<KeyOutcome>) -> Option<String> {
        match outcome? {
            KeyOutcome::Send(s) => Some(s.into_owned()),
            other => panic!("expected bytes, got {other:?}"),
        }
    }

    #[test]
    fn printable_keys_are_left_to_text_input() {
        for key in ["a", "z", "1", "/", "space", "@"] {
            assert_eq!(mac(key, NONE), None, "{key}");
            assert_eq!(mac(key, SHIFT), None, "shift {key}");
            assert_eq!(linux(key, NONE), None, "{key}");
        }
    }

    #[test]
    fn editing_keys() {
        assert_eq!(mac("enter", NONE).as_deref(), Some("\r"));
        assert_eq!(mac("enter", SHIFT).as_deref(), Some("\r"));
        assert_eq!(mac("enter", ALT).as_deref(), Some("\x1b\r"));
        assert_eq!(mac("tab", NONE).as_deref(), Some("\t"));
        assert_eq!(mac("tab", SHIFT).as_deref(), Some("\x1b[Z"));
        assert_eq!(mac("backspace", NONE).as_deref(), Some("\x7f"));
        assert_eq!(mac("backspace", CTRL).as_deref(), Some("\x08"));
        assert_eq!(mac("backspace", ALT).as_deref(), Some("\x1b\x7f"));
        assert_eq!(mac("escape", NONE).as_deref(), Some("\x1b"));
        assert_eq!(mac("escape", ALT).as_deref(), Some("\x1b\x1b"));
    }

    #[test]
    fn arrows_respect_cursor_mode_and_modifiers() {
        assert_eq!(mac("up", NONE).as_deref(), Some("\x1b[A"));
        assert_eq!(mac("down", NONE).as_deref(), Some("\x1b[B"));
        assert_eq!(mac("right", NONE).as_deref(), Some("\x1b[C"));
        assert_eq!(mac("left", NONE).as_deref(), Some("\x1b[D"));
        let app = |key, mods| sent(encode_key(key, mods, true, Platform::Mac));
        assert_eq!(app("up", NONE).as_deref(), Some("\x1bOA"));
        assert_eq!(app("left", NONE).as_deref(), Some("\x1bOD"));
        assert_eq!(app("up", SHIFT).as_deref(), Some("\x1b[1;2A"));
        assert_eq!(mac("up", CTRL).as_deref(), Some("\x1b[1;5A"));
        assert_eq!(mac("up", ALT).as_deref(), Some("\x1b[1;3A"));
        assert_eq!(
            mac(
                "down",
                Modifiers {
                    shift: true,
                    control: true,
                    ..NONE
                }
            )
            .as_deref(),
            Some("\x1b[1;6B")
        );
        assert_eq!(mac("left", SHIFT).as_deref(), Some("\x1b[1;2D"));
        assert_eq!(mac("up", CMD), None);
        assert_eq!(mac("down", CMD), None);
    }

    #[test]
    fn fork_intercepts() {
        assert_eq!(mac("left", ALT).as_deref(), Some("\x1bb"));
        assert_eq!(mac("right", ALT).as_deref(), Some("\x1bf"));
        assert_eq!(mac("left", CMD).as_deref(), Some("\x01"));
        assert_eq!(mac("right", CMD).as_deref(), Some("\x05"));
        assert_eq!(mac("left", CTRL).as_deref(), Some("\x1b[1;5D"));
        assert_eq!(linux("left", CTRL).as_deref(), Some("\x1bb"));
        assert_eq!(linux("right", ALT).as_deref(), Some("\x1bf"));
        assert_eq!(
            mac(
                "left",
                Modifiers {
                    alt: true,
                    shift: true,
                    ..NONE
                }
            )
            .as_deref(),
            Some("\x1b[1;4D")
        );
        assert_eq!(mac("backspace", CMD).as_deref(), Some("\x15"));
        assert_eq!(linux("backspace", CMD).as_deref(), Some("\x7f"));
        assert_eq!(mac("l", CTRL).as_deref(), Some("\x0c"));
        assert_eq!(linux("l", CTRL).as_deref(), Some("\x0c"));
        assert_eq!(mac("k", CMD).as_deref(), Some("\x0c"));
        assert_eq!(linux("k", CMD), None);
    }

    #[test]
    fn navigation_and_function_keys() {
        assert_eq!(mac("home", NONE).as_deref(), Some("\x1b[H"));
        assert_eq!(mac("end", NONE).as_deref(), Some("\x1b[F"));
        assert_eq!(
            sent(encode_key("home", NONE, true, Platform::Mac)).as_deref(),
            Some("\x1bOH")
        );
        assert_eq!(mac("end", SHIFT).as_deref(), Some("\x1b[1;2F"));
        assert_eq!(mac("insert", NONE).as_deref(), Some("\x1b[2~"));
        assert_eq!(mac("insert", SHIFT), None);
        assert_eq!(mac("insert", CTRL), None);
        assert_eq!(mac("delete", NONE).as_deref(), Some("\x1b[3~"));
        assert_eq!(mac("delete", CMD).as_deref(), Some("\x1b[3;9~"));
        assert_eq!(mac("pageup", NONE).as_deref(), Some("\x1b[5~"));
        assert_eq!(mac("pagedown", CTRL).as_deref(), Some("\x1b[6;5~"));
        assert_eq!(
            encode_key("pageup", SHIFT, false, Platform::Mac),
            Some(KeyOutcome::ScrollPageUp)
        );
        assert_eq!(
            encode_key("pagedown", SHIFT, false, Platform::Mac),
            Some(KeyOutcome::ScrollPageDown)
        );
        assert_eq!(mac("f1", NONE).as_deref(), Some("\x1bOP"));
        assert_eq!(mac("f4", NONE).as_deref(), Some("\x1bOS"));
        assert_eq!(mac("f1", SHIFT).as_deref(), Some("\x1b[1;2P"));
        assert_eq!(mac("f5", NONE).as_deref(), Some("\x1b[15~"));
        assert_eq!(mac("f6", NONE).as_deref(), Some("\x1b[17~"));
        assert_eq!(mac("f11", NONE).as_deref(), Some("\x1b[23~"));
        assert_eq!(mac("f12", CTRL).as_deref(), Some("\x1b[24;5~"));
        assert_eq!(mac("f13", NONE), None);
    }

    #[test]
    fn control_combinations() {
        assert_eq!(mac("a", CTRL).as_deref(), Some("\x01"));
        assert_eq!(mac("c", CTRL).as_deref(), Some("\x03"));
        assert_eq!(mac("z", CTRL).as_deref(), Some("\x1a"));
        assert_eq!(mac("space", CTRL).as_deref(), Some("\0"));
        assert_eq!(mac("[", CTRL).as_deref(), Some("\x1b"));
        assert_eq!(mac("\\", CTRL).as_deref(), Some("\x1c"));
        assert_eq!(mac("]", CTRL).as_deref(), Some("\x1d"));
        assert_eq!(mac("3", CTRL).as_deref(), Some("\x1b"));
        assert_eq!(mac("7", CTRL).as_deref(), Some("\x1f"));
        assert_eq!(mac("8", CTRL).as_deref(), Some("\x7f"));
        assert_eq!(mac("2", CTRL), None);
        assert_eq!(mac("@", CTRL).as_deref(), Some("\0"));
        assert_eq!(mac("_", CTRL).as_deref(), Some("\x1f"));
        assert_eq!(
            mac(
                "_",
                Modifiers {
                    control: true,
                    shift: true,
                    ..NONE
                }
            )
            .as_deref(),
            Some("\x1f")
        );
        assert_eq!(
            mac(
                "a",
                Modifiers {
                    control: true,
                    shift: true,
                    ..NONE
                }
            ),
            None
        );
    }

    #[test]
    fn alt_is_meta_only_off_macos() {
        assert_eq!(mac("b", ALT), None);
        assert_eq!(mac("1", ALT), None);
        assert_eq!(mac("f1", ALT), None);
        assert_eq!(linux("b", ALT).as_deref(), Some("\x1bb"));
        assert_eq!(
            linux(
                "b",
                Modifiers {
                    alt: true,
                    shift: true,
                    ..NONE
                }
            )
            .as_deref(),
            Some("\x1bB")
        );
        assert_eq!(
            linux(
                "b",
                Modifiers {
                    alt: true,
                    control: true,
                    ..NONE
                }
            )
            .as_deref(),
            Some("\x1b\x02")
        );
        assert_eq!(linux(".", ALT).as_deref(), Some("\x1b."));
        assert_eq!(linux("space", ALT).as_deref(), Some("\x1b "));
    }

    #[test]
    fn command_shortcuts() {
        assert_eq!(
            encode_key("a", CMD, false, Platform::Mac),
            Some(KeyOutcome::SelectAll)
        );
        assert_eq!(encode_key("a", CMD, false, Platform::Other), None);
        for key in ["c", "v", "d", "n", "w", "j", "1"] {
            assert_eq!(mac(key, CMD), None, "cmd-{key}");
        }
    }

    #[test]
    fn mouse_reports() {
        let sgr = |button, action, mods, protocol| {
            encode_mouse(button, action, 4, 9, mods, protocol, true)
        };
        let x10 = |button, action, mods, protocol| {
            encode_mouse(button, action, 4, 9, mods, protocol, false)
        };
        use MouseAction::*;
        use MouseButton as B;
        use MouseProtocol::*;
        assert_eq!(
            sgr(B::Left, Press, NONE, Click).as_deref(),
            Some("\x1b[<0;5;10M")
        );
        assert_eq!(
            sgr(B::Left, Release, NONE, Click).as_deref(),
            Some("\x1b[<0;5;10m")
        );
        assert_eq!(
            sgr(B::Right, Press, NONE, Click).as_deref(),
            Some("\x1b[<2;5;10M")
        );
        assert_eq!(
            sgr(B::Middle, Press, CTRL, Click).as_deref(),
            Some("\x1b[<17;5;10M")
        );
        assert_eq!(
            sgr(
                B::Left,
                Press,
                Modifiers {
                    shift: true,
                    alt: true,
                    ..NONE
                },
                Click
            )
            .as_deref(),
            Some("\x1b[<12;5;10M")
        );
        assert_eq!(
            sgr(B::WheelUp, Press, NONE, Click).as_deref(),
            Some("\x1b[<64;5;10M")
        );
        assert_eq!(
            sgr(B::WheelDown, Press, NONE, Click).as_deref(),
            Some("\x1b[<65;5;10M")
        );
        assert_eq!(sgr(B::Left, Move, NONE, Click), None);
        assert_eq!(
            sgr(B::Left, Move, NONE, Drag).as_deref(),
            Some("\x1b[<32;5;10M")
        );
        assert_eq!(sgr(B::None, Move, NONE, Drag), None);
        assert_eq!(
            sgr(B::None, Move, NONE, Any).as_deref(),
            Some("\x1b[<35;5;10M")
        );
        assert_eq!(sgr(B::None, Press, NONE, Any), None);
        assert_eq!(
            x10(B::Left, Press, NONE, Click).as_deref(),
            Some("\x1b[M %*")
        );
        assert_eq!(
            x10(B::Right, Release, NONE, Click).as_deref(),
            Some("\x1b[M#%*")
        );
        assert_eq!(
            encode_mouse(B::Left, Press, 230, 0, NONE, Click, false),
            None
        );
        assert_eq!(
            encode_mouse(B::Left, Press, 230, 0, NONE, Click, true).as_deref(),
            Some("\x1b[<0;231;1M")
        );
    }

    #[test]
    fn paste_and_wheel() {
        assert_eq!(paste_text("a\nb\r\nc", false), "a\rb\rc");
        assert_eq!(paste_text("ls\n", true), "\x1b[200~ls\r\x1b[201~");
        assert_eq!(wheel_arrow(true, false), "\x1b[A");
        assert_eq!(wheel_arrow(false, true), "\x1bOB");
    }
}
