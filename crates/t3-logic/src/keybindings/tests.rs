//! Failure modes this covers, written before the tests:
//!
//! 1. Order: the last active matching rule must win (`mod+n` is `terminal.new` in a terminal and
//!    `chat.new` elsewhere; ⌘1 is `modelPicker.jump.1` only while the picker is open).
//! 2. `mod` must resolve to Cmd on macOS and Ctrl elsewhere, and modifiers must match exactly:
//!    ⌘⇧N must not trigger `mod+n`.
//! 3. GPUI reports ⌘⇧[ as `{` with shift cleared; with the real modifier state the press must
//!    still match `mod+shift+[` (and ⌘⇧1 must match `mod+shift+1`), but plain ⌘[ must not.
//! 4. GPUI key names (`space`, `up`, `esc`) must map to DOM names (`" "`, `arrowup`, `escape`).
//! 5. `when`: unknown identifiers read false, `isDesktop` reads true, `!`/`&&`/`||`/parens keep
//!    their precedence, and an unknown AST node never holds.
//! 6. Labels: macOS glyph order is ⌃⌥⇧⌘ then the key; elsewhere `Ctrl+Alt+Shift+Meta+Key`;
//!    special keys get names (Esc, Space, Up).
//! 7. A command whose chord is claimed by a later active rule has no label.
//! 8. Parser: `mod++` is the plus key, two keys or empty tokens are rejected, `space`/`esc` are
//!    normalized; `when` rejects unbalanced parens, trailing tokens, and runaway depth.
//! 9. Jump hints show only when the held modifiers exactly equal a jump shortcut's modifiers.
//! 10. Command parsing: `thread.jump.N` only for 1-9, `script.<id>.run`, unknown strings kept.

use super::*;

const MAC: Platform = Platform::Mac;
const OTHER: Platform = Platform::Other;

fn cmd() -> Modifiers {
    Modifiers {
        meta: true,
        ..Modifiers::default()
    }
}

fn cmd_shift() -> Modifiers {
    Modifiers {
        meta: true,
        shift: true,
        ..Modifiers::default()
    }
}

fn press(key: &str, modifiers: Modifiers) -> ShortcutEvent {
    ShortcutEvent::from_gpui(key, modifiers)
}

fn resolve(event: &ShortcutEvent, context: ShortcutContext) -> Option<Command> {
    resolve_command(event, &default_keybindings(), &context, MAC)
}

#[test]
fn later_rules_win_and_when_clauses_gate() {
    let terminal = ShortcutContext {
        terminal_focus: true,
        ..ShortcutContext::default()
    };
    assert_eq!(
        resolve(&press("n", cmd()), ShortcutContext::default()),
        Some(Command::ChatNew)
    );
    assert_eq!(
        resolve(&press("n", cmd()), terminal),
        Some(Command::TerminalNew)
    );

    let picker = ShortcutContext {
        model_picker_open: true,
        ..ShortcutContext::default()
    };
    assert_eq!(
        resolve(&press("1", cmd()), ShortcutContext::default()),
        Some(Command::ThreadJump(1))
    );
    assert_eq!(
        resolve(&press("1", cmd()), picker),
        Some(Command::ModelPickerJump(1))
    );
}

#[test]
fn modifiers_match_exactly_and_mod_follows_platform() {
    let rules = default_keybindings();
    let context = ShortcutContext::default();
    assert_eq!(
        resolve(&press("n", cmd_shift()), context),
        Some(Command::ChatNewLocal)
    );
    assert_eq!(resolve(&press("b", Modifiers::default()), context), None);

    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    assert_eq!(
        resolve_command(&press("b", ctrl), &rules, &context, MAC),
        None
    );
    assert_eq!(
        resolve_command(&press("b", ctrl), &rules, &context, OTHER),
        Some(Command::SidebarToggle)
    );
    assert_eq!(
        resolve_command(&press("b", cmd()), &rules, &context, OTHER),
        None
    );
}

#[test]
fn shifted_symbols_recover_the_physical_key() {
    let context = ShortcutContext::default();
    // GPUI on macOS: Cmd+Shift+[ arrives as "{" (shift cleared in the keystroke; the window's
    // modifier state still has shift down).
    assert_eq!(
        resolve(&press("{", cmd_shift()), context),
        Some(Command::ThreadPrevious)
    );
    assert_eq!(
        resolve(&press("}", cmd_shift()), context),
        Some(Command::ThreadNext)
    );
    // Plain Cmd+[ is not thread.previous.
    assert_eq!(resolve(&press("[", cmd()), context), None);

    let rules = vec![ResolvedKeybindingRule {
        command: "usage.period.day".into(),
        shortcut: parse_shortcut("mod+shift+1").unwrap(),
        when_ast: None,
    }];
    assert_eq!(
        resolve_command(&press("!", cmd_shift()), &rules, &context, MAC),
        Some(Command::Other("usage.period.day".into()))
    );
}

#[test]
fn gpui_key_names_map_to_dom_names() {
    assert_eq!(press("space", Modifiers::default()).key(), " ");
    assert_eq!(press("up", Modifiers::default()).key(), "arrowup");
    assert_eq!(press("esc", Modifiers::default()).key(), "escape");
    assert_eq!(press("Escape", Modifiers::default()).key(), "escape");

    let rules = vec![ResolvedKeybindingRule {
        command: "modelPicker.previousProvider".into(),
        shortcut: parse_shortcut("mod+shift+arrowup").unwrap(),
        when_ast: None,
    }];
    assert_eq!(
        resolve_command(
            &press("up", cmd_shift()),
            &rules,
            &ShortcutContext::default(),
            MAC
        ),
        Some(Command::Other("modelPicker.previousProvider".into()))
    );
}

#[test]
fn when_clauses_evaluate_like_the_web() {
    let context = ShortcutContext {
        terminal_focus: true,
        ..ShortcutContext::default()
    };
    let eval = |expression: &str| evaluate_when(&parse_when(expression).unwrap(), &context);
    assert!(eval("terminalFocus"));
    assert!(!eval("someFutureFlag"));
    assert!(eval("isDesktop"));
    assert!(eval("true"));
    assert!(!eval("false"));
    assert!(!eval("!terminalFocus"));
    // && binds tighter than ||.
    assert!(eval("previewOpen && previewFocus || terminalFocus"));
    assert!(!eval("previewOpen && (previewFocus || terminalFocus)"));
    assert!(eval("!!terminalFocus"));
    assert!(!evaluate_when(&KeybindingWhenNode::Unknown, &context));
}

#[test]
fn labels_follow_platform_conventions() {
    let label = |key: &str, platform| format_shortcut(&parse_shortcut(key).unwrap(), platform);
    assert_eq!(label("mod+b", MAC), "\u{2318}B");
    assert_eq!(label("mod+shift+n", MAC), "\u{21e7}\u{2318}N");
    assert_eq!(
        label("ctrl+alt+shift+cmd+k", MAC),
        "\u{2303}\u{2325}\u{21e7}\u{2318}K"
    );
    assert_eq!(label("mod+shift+n", OTHER), "Ctrl+Shift+N");
    assert_eq!(
        label("ctrl+alt+shift+meta+k", OTHER),
        "Ctrl+Alt+Shift+Meta+K"
    );
    assert_eq!(label("esc", MAC), "Esc");
    assert_eq!(label("mod+space", OTHER), "Ctrl+Space");
    assert_eq!(label("mod+arrowup", MAC), "\u{2318}Up");
    assert_eq!(label("mod+enter", MAC), "\u{2318}Enter");
}

#[test]
fn shadowed_commands_have_no_label() {
    let rules = default_keybindings();
    let context = ShortcutContext::default();
    assert_eq!(
        shortcut_label(&rules, &Command::SidebarToggle, &context, MAC).as_deref(),
        Some("\u{2318}B")
    );
    assert_eq!(
        shortcut_label(&rules, &Command::ChatNewLocal, &context, MAC).as_deref(),
        Some("\u{21e7}\u{2318}N")
    );
    // chat.new has two rules; the later one (⇧⌘O) is effective.
    assert_eq!(
        shortcut_label(&rules, &Command::ChatNew, &context, MAC).as_deref(),
        Some("\u{21e7}\u{2318}O")
    );

    let mut shadowed = rules.clone();
    shadowed.push(ResolvedKeybindingRule {
        command: "custom.thing".into(),
        shortcut: parse_shortcut("mod+b").unwrap(),
        when_ast: None,
    });
    assert_eq!(
        shortcut_label(&shadowed, &Command::SidebarToggle, &context, MAC),
        None
    );
    // Inactive rules do not claim chords.
    let terminal = ShortcutContext {
        terminal_focus: true,
        ..ShortcutContext::default()
    };
    assert_eq!(
        shortcut_label(&rules, &Command::TerminalNew, &context, MAC),
        None
    );
    assert_eq!(
        shortcut_label(&rules, &Command::TerminalNew, &terminal, MAC).as_deref(),
        Some("\u{2318}N")
    );
}

#[test]
fn shortcut_parser_matches_the_web() {
    let plus = parse_shortcut("mod++").unwrap();
    assert_eq!(plus.key, "+");
    assert!(plus.mod_key);
    assert_eq!(parse_shortcut("Mod+Space").unwrap().key, " ");
    assert_eq!(parse_shortcut("esc").unwrap().key, "escape");
    let full = parse_shortcut("cmd+control+option+shift+x").unwrap();
    assert!(full.meta_key && full.ctrl_key && full.alt_key && full.shift_key && !full.mod_key);
    assert_eq!(parse_shortcut("mod+a+b"), None);
    assert_eq!(parse_shortcut("mod++a"), None);
    assert_eq!(parse_shortcut("mod+shift"), None);
    // The web parser reads an empty string as a lone trailing "+"; keep parity.
    assert_eq!(parse_shortcut("").unwrap().key, "+");
}

#[test]
fn when_parser_rejects_malformed_input() {
    assert!(parse_when("a && (b || c)").is_some());
    assert!(parse_when("foo.bar-baz_1").is_some());
    assert_eq!(parse_when(""), None);
    assert_eq!(parse_when("(a"), None);
    assert_eq!(parse_when("a b"), None);
    assert_eq!(parse_when("a &&"), None);
    assert_eq!(parse_when("a & b"), None);
    assert_eq!(parse_when("1abc"), None);
    let deep = format!("{}a{}", "(".repeat(70), ")".repeat(70));
    assert_eq!(parse_when(&deep), None);
    let shallow = format!("{}a{}", "(".repeat(10), ")".repeat(10));
    assert!(parse_when(&shallow).is_some());
}

#[test]
fn jump_hints_need_exact_modifiers() {
    let rules = default_keybindings();
    let context = ShortcutContext::default();
    assert!(should_show_thread_jump_hints(cmd(), &rules, &context, MAC));
    assert!(!should_show_thread_jump_hints(
        cmd_shift(),
        &rules,
        &context,
        MAC
    ));
    assert!(!should_show_thread_jump_hints(
        Modifiers::default(),
        &rules,
        &context,
        MAC
    ));
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    assert!(should_show_thread_jump_hints(ctrl, &rules, &context, OTHER));
}

#[test]
fn commands_round_trip() {
    for wire in [
        "sidebar.toggle",
        "thread.jump.9",
        "modelPicker.jump.1",
        "script.lint.run",
        "chat.newLocal",
        "usage.cost",
    ] {
        assert_eq!(Command::parse(wire).as_str(), wire);
    }
    assert_eq!(
        Command::parse("thread.jump.0"),
        Command::Other("thread.jump.0".into())
    );
    assert_eq!(
        Command::parse("thread.jump.10"),
        Command::Other("thread.jump.10".into())
    );
    assert_eq!(
        Command::parse("script..run"),
        Command::Other("script..run".into())
    );
}

#[test]
fn defaults_compile_completely() {
    // 23 fixed rules + 9 thread jumps + 9 model picker jumps; none may fail to parse.
    assert_eq!(default_keybindings().len(), 41);
}
