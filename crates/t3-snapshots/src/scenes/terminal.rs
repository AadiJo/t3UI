//! Terminal view scenes: recorded ANSI output fed into the real `TerminalView`, inside the
//! drawer's `p-4` viewport wrapper. After the first frame, scenes can drive real input through
//! the window (pointer hover, drag selection, wheel scroll, key presses). A local echo stands in
//! for the PTY, so typed keys show up as output exactly as the view encoded them.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, Focusable as _, IntoElement, Modifiers,
    MouseMoveEvent, ParentElement as _, Pixels, PlatformInput, Point, Render, ScrollDelta,
    ScrollWheelEvent, Styled as _, TouchPhase, Window, div, point, px, test::TestWindowExt as _,
};
use t3_terminal::{TerminalEvent, TerminalView};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

/// What a scene feeds the terminal and does after the first frame.
struct Fixture {
    output: fn() -> String,
    /// Focus the terminal in an active window (block cursor instead of the outline).
    focus: bool,
    /// Input run after the first frame, each step in its own update so echoes interleave.
    steps: fn() -> Vec<Step>,
    echo: Echo,
}

const BASE: Fixture = Fixture {
    output: String::new,
    focus: false,
    steps: Vec::new,
    echo: Echo::None,
};

/// How the scene answers `TerminalEvent::Input`, standing in for the PTY.
#[derive(Clone, Copy)]
enum Echo {
    None,
    /// Feed input back as output, like a shell's line editor echoing keys.
    Raw,
    /// Print the bytes in caret notation (`^[[D`).
    Caret,
}

/// One interaction, in viewport cells where a position is needed.
enum Step {
    Hover {
        col: usize,
        row: usize,
    },
    Drag {
        from: (usize, usize),
        to: (usize, usize),
    },
    /// Wheel toward older output by `lines` rows, with the pointer over `(col, row)`.
    ScrollUp {
        col: usize,
        row: usize,
        lines: f32,
    },
    /// A GPUI keystroke such as `"left"` or `"cmd-k"`.
    Press(&'static str),
    /// Text typed through the platform text input path.
    Type(&'static str),
    /// Output written directly, e.g. a label.
    Feed(String),
}

const SIZE: (f32, f32) = (960., 420.);
const CURSOR_SIZE: (f32, f32) = (360., 72.);

macro_rules! scene {
    ($name:literal, $theme:expr, $fixture:expr) => {
        Scene::new($name, $theme, |window, cx| build(&$fixture, window, cx))
    };
}

pub fn scenes() -> Vec<Scene> {
    use ThemeMode::{Dark, Light};
    let (w, h) = SIZE;
    let (cw, ch) = CURSOR_SIZE;
    vec![
        scene!("terminal-shell-dark", Dark, SHELL).size(w, h),
        scene!("terminal-shell-light", Light, SHELL).size(w, h),
        scene!("terminal-tests-dark", Dark, TESTS).size(w, h),
        scene!("terminal-tests-light", Light, TESTS).size(w, h),
        scene!("terminal-colors-dark", Dark, COLORS).size(w, h),
        scene!("terminal-colors-light", Light, COLORS).size(w, h),
        scene!("terminal-unicode-dark", Dark, UNICODE).size(w, h),
        scene!("terminal-unicode-light", Light, UNICODE).size(w, h),
        scene!("terminal-selection-dark", Dark, SELECTION).size(w, h),
        scene!("terminal-selection-light", Light, SELECTION).size(w, h),
        scene!("terminal-scrollback-dark", Dark, SCROLLBACK).size(w, h),
        scene!("terminal-scrollback-light", Light, SCROLLBACK).size(w, h),
        scene!("terminal-keys-dark", Dark, KEYS).size(w, 560.),
        scene!("terminal-cursor-block-dark", Dark, CURSOR_BLOCK).size(cw, ch),
        scene!("terminal-cursor-block-light", Light, CURSOR_BLOCK).size(cw, ch),
        scene!("terminal-cursor-bar-dark", Dark, CURSOR_BAR).size(cw, ch),
        scene!("terminal-cursor-bar-light", Light, CURSOR_BAR).size(cw, ch),
        scene!("terminal-cursor-underline-dark", Dark, CURSOR_UNDERLINE).size(cw, ch),
        scene!("terminal-cursor-underline-light", Light, CURSOR_UNDERLINE).size(cw, ch),
        scene!("terminal-cursor-unfocused-dark", Dark, CURSOR_UNFOCUSED).size(cw, ch),
        scene!("terminal-cursor-unfocused-light", Light, CURSOR_UNFOCUSED).size(cw, ch),
    ]
}

/// The drawer's viewport wrapper: `h-full p-4` around the xterm host.
struct TerminalScene {
    terminal: Entity<TerminalView>,
}

impl Render for TerminalScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(4.))
            .bg(cx.colors().background)
            .child(self.terminal.clone())
    }
}

fn build(fixture: &'static Fixture, window: &mut Window, cx: &mut App) -> AnyView {
    t3_terminal::init(cx);
    let terminal = cx.new(|cx| TerminalView::new(window, cx));
    terminal.update(cx, |terminal, cx| {
        terminal.feed_output(&(fixture.output)(), cx)
    });
    let echo = fixture.echo;
    if !matches!(echo, Echo::None) {
        cx.subscribe(&terminal, move |terminal, event: &TerminalEvent, cx| {
            if let TerminalEvent::Input(data) = event {
                let output = match echo {
                    Echo::Caret => caret_notation(data),
                    _ => data.clone(),
                };
                terminal.update(cx, |terminal, cx| terminal.feed_output(&output, cx));
            }
        })
        .detach();
    }
    if fixture.focus {
        window.activate_window();
        terminal.focus_handle(cx).focus(window, cx);
    }
    let steps = (fixture.steps)();
    if !steps.is_empty() {
        let terminal = terminal.clone();
        window
            .spawn(cx, async move |cx| {
                for step in steps {
                    cx.update(|window, cx| run_step(&step, &terminal, window, cx))
                        .ok();
                }
            })
            .detach();
    }
    cx.new(|_| TerminalScene { terminal }).into()
}

fn run_step(step: &Step, terminal: &Entity<TerminalView>, window: &mut Window, cx: &mut App) {
    match step {
        Step::Hover { col, row } => {
            window.render_frame(cx);
            move_pointer(cell_center(terminal, *col, *row, cx), window, cx);
        }
        Step::Drag { from, to } => {
            let from = cell_center(terminal, from.0, from.1, cx);
            let to = cell_center(terminal, to.0, to.1, cx);
            window.drag(from, to, cx);
        }
        Step::ScrollUp { col, row, lines } => {
            window.render_frame(cx);
            let position = cell_center(terminal, *col, *row, cx);
            move_pointer(position, window, cx);
            window.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position,
                    delta: ScrollDelta::Lines(point(0., *lines)),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            );
        }
        Step::Press(key) => window.press(key, cx),
        Step::Type(text) => window.input(text, cx),
        Step::Feed(text) => terminal.update(cx, |terminal, cx| terminal.feed_output(text, cx)),
    }
}

/// `cat -v` style: ESC as `^[`, other C0 bytes as `^X`, DEL as `^?`.
fn caret_notation(data: &str) -> String {
    data.chars()
        .map(|c| match c {
            '\x7f' => "^?".to_owned(),
            c if (c as u32) < 0x20 => format!("^{}", char::from(c as u8 + 0x40)),
            c => c.to_string(),
        })
        .collect()
}

/// Window position of the middle of viewport cell `(col, row)`.
fn cell_center(terminal: &Entity<TerminalView>, col: usize, row: usize, cx: &App) -> Point<Pixels> {
    let origin = terminal
        .read(cx)
        .cell_origin(col, row)
        .expect("terminal laid out");
    point(origin.x + px(3.), origin.y + px(8.))
}

fn move_pointer(position: Point<Pixels>, window: &mut Window, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }),
        cx,
    );
}

// ESC sequences used below.
const RESET: &str = "\x1b[0m";

fn prompt(cwd: &str, branch: &str) -> String {
    format!(
        "\x1b[1;32m➜{RESET}  \x1b[1;36m{cwd}{RESET} \x1b[1;34mgit:(\x1b[31m{branch}\x1b[1;34m){RESET} "
    )
}

const SHELL: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        out += &prompt("t3UI", "main");
        out += "ls -la --color\r\n";
        out += "total 72\r\n";
        let entries = [
            ("drwxr-xr-x", "12", "384", "\x1b[1;34m.\x1b[0m"),
            ("drwxr-xr-x", " 9", "288", "\x1b[1;34m..\x1b[0m"),
            ("-rw-r--r--", " 1", "1846", ".gitignore"),
            ("drwxr-xr-x", " 3", " 96", "\x1b[1;34m.github\x1b[0m"),
            ("-rw-r--r--", " 1", "2210", "AGENTS.md"),
            (
                "lrwxr-xr-x",
                " 1",
                "  9",
                "\x1b[1;36mCLAUDE.md\x1b[0m -> AGENTS.md",
            ),
            ("-rw-r--r--", " 1", "48211", "Cargo.lock"),
            ("drwxr-xr-x", " 9", "288", "\x1b[1;34mcrates\x1b[0m"),
            (
                "-rwxr-xr-x",
                " 1",
                "1290",
                "\x1b[1;32mcheck-macos.sh\x1b[0m",
            ),
            (
                "-rw-r--r--",
                " 1",
                "88213",
                "\x1b[1;35mreference.png\x1b[0m",
            ),
            (
                "-rw-r--r--",
                " 1",
                "40960",
                "\x1b[1;31mrelease.tar.gz\x1b[0m",
            ),
        ];
        for (mode, links, size, name) in entries {
            out += &format!("{mode}  {links} aadi  staff  {size:>5} Oct  1 09:12 {name}\r\n");
        }
        out += &prompt("t3UI", "main");
        out += "git log --oneline -3\r\n";
        out += "\x1b[33m68901e2\x1b[m (\x1b[1;36mHEAD -> \x1b[1;32mmain\x1b[m, \x1b[1;31morigin/main\x1b[m) docs(spec): protocol, design system, and generated tokens\r\n";
        out += "\x1b[33m85ba4ef\x1b[m docs(spec): chat view, timeline, and composer spec\r\n";
        out +=
            "\x1b[33m36f3761\x1b[m docs(spec): connections, t3 connect, and right panel specs\r\n";
        out += &prompt("t3UI", "main");
        out += "echo \"review https://github.com/AadiJo/t3UI/pull/12 then open crates/t3-terminal/src/view.rs:42:7\"\r\n";
        out += "review https://github.com/AadiJo/t3UI/pull/12 then open crates/t3-terminal/src/view.rs:42:7\r\n";
        out += &prompt("t3UI", "main");
        out
    },
    focus: true,
    // Hover the URL on the echo output row: it underlines with a pointing-hand cursor.
    steps: || vec![Step::Hover { col: 14, row: 18 }],
    ..BASE
};

const TESTS: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        out += &prompt("web", "terminal");
        out += "bun run test\r\n\r\n";
        out += " \x1b[46;30m RUN \x1b[49;39m \x1b[36mv3.2.4 \x1b[39m\x1b[90m/Users/aadi/t3code/apps/web\x1b[39m\r\n\r\n";
        out += " \x1b[32m✓\x1b[39m src/terminal-links.test.ts \x1b[2m(\x1b[22m\x1b[2m12 tests\x1b[22m\x1b[2m)\x1b[22m\x1b[32m 4\x1b[2mms\x1b[22m\x1b[39m\r\n";
        out += " \x1b[32m✓\x1b[39m src/keybindings.test.ts \x1b[2m(\x1b[22m\x1b[2m48 tests\x1b[22m\x1b[2m)\x1b[22m\x1b[32m 9\x1b[2mms\x1b[22m\x1b[39m\r\n";
        out += " \x1b[33m❯\x1b[39m src/composer.test.ts \x1b[2m(\x1b[22m\x1b[2m6 tests\x1b[22m\x1b[2m | \x1b[22m\x1b[31m1 failed\x1b[39m\x1b[2m)\x1b[22m\x1b[33m 21\x1b[2mms\x1b[22m\x1b[39m\r\n";
        out += "   \x1b[32m✓\x1b[39m restores the draft after reload\x1b[2m 3ms\x1b[22m\r\n";
        out += "   \x1b[31m×\x1b[39m keeps the draft when switching threads\x1b[2m 8ms\x1b[22m\r\n";
        out += "     \x1b[31m→ expected \x1b[32m'hello world'\x1b[31m to be \x1b[31m'hello'\x1b[31m // Object.is equality\x1b[39m\r\n\r\n";
        out += "\x1b[31m⎯⎯⎯⎯⎯⎯⎯ \x1b[1m\x1b[7m Failed Tests 1 \x1b[27m\x1b[22m ⎯⎯⎯⎯⎯⎯⎯\x1b[39m\r\n\r\n";
        out += "\x1b[41m\x1b[1m FAIL \x1b[22m\x1b[49m src/composer.test.ts\x1b[2m > \x1b[22mkeeps the draft when switching threads\r\n";
        out += "\x1b[31m\x1b[1mAssertionError\x1b[22m: expected 'hello world' to be 'hello'\x1b[39m\r\n";
        out +=
            "\x1b[36m \x1b[2m❯\x1b[22m src/composer.test.ts:\x1b[2m88:31\x1b[22m\x1b[39m\r\n\r\n";
        out += "\x1b[2m Test Files \x1b[22m \x1b[1m\x1b[31m1 failed\x1b[39m\x1b[22m\x1b[2m | \x1b[22m\x1b[1m\x1b[32m2 passed\x1b[39m\x1b[22m\x1b[90m (3)\x1b[39m\r\n";
        out += "\x1b[2m      Tests \x1b[22m \x1b[1m\x1b[31m1 failed\x1b[39m\x1b[22m\x1b[2m | \x1b[22m\x1b[1m\x1b[32m65 passed\x1b[39m\x1b[22m\x1b[90m (66)\x1b[39m\r\n";
        out += "\x1b[2m   Start at \x1b[22m 09:41:12\r\n";
        out += "\x1b[2m   Duration \x1b[22m 1.24s\x1b[2m (transform 210ms, setup 0ms, collect 1.02s, tests 34ms)\x1b[22m\r\n\r\n";
        out += &prompt("web", "terminal");
        out
    },
    ..BASE
};

const COLORS: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        let names = [
            "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
        ];
        out += "16 colors   ";
        for (i, name) in names.iter().enumerate() {
            out += &format!("\x1b[3{i}m{name:<8}");
        }
        out += "\x1b[0m\r\n bright     ";
        for (i, name) in names.iter().enumerate() {
            out += &format!("\x1b[9{i}m{name:<8}");
        }
        out += "\x1b[0m\r\n bold       ";
        for (i, name) in names.iter().enumerate() {
            out += &format!("\x1b[1;3{i}m{name:<8}");
        }
        out += "\x1b[0m\r\n background ";
        for i in 0..8 {
            out += &format!("\x1b[4{i}m  {i:<2}  \x1b[0m ");
        }
        out += "\r\n            ";
        for i in 0..8 {
            out += &format!("\x1b[10{i}m  {:<2}  \x1b[0m ", i + 8);
        }
        out += "\r\n\r\n256 colors\r\n";
        for row in 0..6 {
            out += " ";
            for col in 0..36 {
                out += &format!("\x1b[48;5;{}m  ", 16 + row * 36 + col);
            }
            out += "\x1b[0m\r\n";
        }
        out += " ";
        for gray in 232..256 {
            out += &format!("\x1b[48;5;{gray}m   ");
        }
        out += "\x1b[0m\r\n\r\ntruecolor\r\n ";
        for i in 0..96u32 {
            let t = i as f32 / 95.;
            let (r, g, b) = hue((t * 360.) as u32);
            out += &format!("\x1b[48;2;{r};{g};{b}m ");
        }
        out += "\x1b[0m\r\n ";
        for i in 0..96u32 {
            let v = (i * 255 / 95) as u8;
            out += &format!("\x1b[38;2;{v};{};{}m█", 255 - v, 128);
        }
        out += "\x1b[0m\r\n\r\nattributes  \x1b[1mbold\x1b[22m  \x1b[2mdim\x1b[22m  \x1b[3mitalic\x1b[23m  \x1b[1;3mbold italic\x1b[0m  \x1b[4munderline\x1b[24m  \x1b[4:3mcurly\x1b[4:0m  \x1b[9mstrikethrough\x1b[29m  \x1b[7minverse\x1b[27m  [\x1b[8mhidden\x1b[28m]  \x1b[2;31mdim red\x1b[0m  \x1b[7;32m inverse green \x1b[0m\r\n";
        out
    },
    ..BASE
};

/// A fully saturated color at `degrees` on the hue wheel.
fn hue(degrees: u32) -> (u8, u8, u8) {
    let h = degrees % 360;
    let x = ((60 - (h % 120).abs_diff(60)) * 255 / 60) as u8;
    match h / 60 {
        0 => (255, x, 0),
        1 => (x, 255, 0),
        2 => (0, 255, x),
        3 => (0, x, 255),
        4 => (x, 0, 255),
        _ => (255, 0, x),
    }
}

const UNICODE: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        out += "wide      日本語のテキスト｜中文字符｜한국어 텍스트\r\n";
        out += "emoji     🚀 launch  ✨ sparkle  🎉 party  👍🏽 thumbs  ❤️ heart\r\n";
        out +=
            "combining e\u{301} a\u{308} n\u{303} o\u{302} u\u{30a}  (é ä ñ ô ů precomposed)\r\n";
        out += "symbols   ✓ ✗ ➜ ❯ → ← ↑ ↓ • … ± × ÷ ≠ ≤ ≥ λ π Σ\r\n";
        out += "blocks    ▁▂▃▄▅▆▇█ ░▒▓ ⣾⣽⣻⢿⡿⣟⣯⣷\r\n\r\n";
        out += "┌───────────────┬──────────┬──────────┐\r\n";
        out +=
            "│ \x1b[1mPackage\x1b[0m       │ \x1b[1mVersion\x1b[0m  │ \x1b[1mLicense\x1b[0m  │\r\n";
        out += "├───────────────┼──────────┼──────────┤\r\n";
        out += "│ gpui-kit      │ 0.7.0    │ \x1b[32mApache\x1b[0m   │\r\n";
        out += "│ alacritty     │ 0.26.0   │ \x1b[32mApache\x1b[0m   │\r\n";
        out += "│ 終端機        │ 1.0      │ \x1b[33mMIT\x1b[0m      │\r\n";
        out += "└───────────────┴──────────┴──────────┘\r\n\r\n";
        out += &prompt("t3UI", "main");
        out += "echo 你好";
        out
    },
    focus: true,
    ..BASE
};

const SELECTION: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        out += &prompt("t3UI", "main");
        out += "cat crates/t3-terminal/src/lib.rs\r\n";
        let source = [
            "//! Terminal emulator view for T3 Code's terminal drawer and terminal panel.",
            "",
            "mod element;",
            "mod input;",
            "mod links;",
            "mod session;",
            "",
            "pub use links::{TerminalLinkKind, resolve_path_link_target};",
            "pub use view::{TerminalEvent, TerminalView};",
        ];
        for line in source {
            out += line;
            out += "\r\n";
        }
        out += &prompt("t3UI", "main");
        out
    },
    focus: true,
    // Drag from "mod element" to the middle of the `pub use links` line.
    steps: || {
        vec![Step::Drag {
            from: (4, 3),
            to: (33, 8),
        }]
    },
    ..BASE
};

const SCROLLBACK: Fixture = Fixture {
    output: || {
        let mut out = String::new();
        out += &prompt("t3UI", "main");
        out += "cargo build --timings\r\n";
        for i in 1..=240 {
            let krate = [
                "serde",
                "tokio",
                "gpui-pre",
                "alacritty_terminal",
                "regex",
                "syntect",
            ][i % 6];
            out += &format!(
                "   \x1b[1;32mCompiling\x1b[0m {krate} v0.{}.{} \x1b[2m({i:>3}/240)\x1b[0m\r\n",
                i % 9,
                i % 17
            );
        }
        out += "    \x1b[1;32mFinished\x1b[0m `dev` profile [unoptimized + debuginfo] target(s) in 41.07s\r\n";
        out += &prompt("t3UI", "main");
        out
    },
    // Hover, then wheel up 60 lines: the viewport moves into scrollback and the slider shows.
    steps: || {
        vec![Step::ScrollUp {
            col: 40,
            row: 10,
            lines: 60.,
        }]
    },
    ..BASE
};

/// Types `git status` through the keyboard path, then presses Left six times so the cursor
/// sits on the `s` of `status`. The local echo applies the bytes the view sent.
fn typed_status() -> Vec<Step> {
    let mut steps = vec![
        Step::Type("git"),
        Step::Press("space"),
        Step::Type("status"),
    ];
    steps.extend(std::iter::repeat_with(|| Step::Press("left")).take(6));
    steps
}

const CURSOR_BLOCK: Fixture = Fixture {
    output: || "$ ".into(),
    focus: true,
    steps: typed_status,
    echo: Echo::Raw,
};
const CURSOR_BAR: Fixture = Fixture {
    output: || "\x1b[6 q$ ".into(),
    ..CURSOR_BLOCK
};
const CURSOR_UNDERLINE: Fixture = Fixture {
    output: || "\x1b[4 q$ ".into(),
    ..CURSOR_BLOCK
};
const CURSOR_UNFOCUSED: Fixture = Fixture {
    output: || "$ git status\x1b[6D".into(),
    ..BASE
};

/// Keys and the label printed before each; the echo prints what the view sent.
const KEY_ROWS: &[(&str, &str)] = &[
    ("enter", "enter"),
    ("shift-enter", "shift-enter"),
    ("tab", "tab"),
    ("shift-tab", "shift-tab"),
    ("backspace", "backspace"),
    ("ctrl-backspace", "ctrl-backspace"),
    ("alt-backspace", "alt-backspace"),
    ("cmd-backspace", "cmd-backspace (fork)"),
    ("escape", "escape"),
    ("up", "up"),
    ("shift-up", "shift-up"),
    ("ctrl-down", "ctrl-down"),
    ("alt-left", "alt-left (fork)"),
    ("alt-right", "alt-right (fork)"),
    ("cmd-left", "cmd-left (fork)"),
    ("cmd-right", "cmd-right (fork)"),
    ("home", "home"),
    ("end", "end"),
    ("pageup", "pageup"),
    ("delete", "delete"),
    ("f1", "f1"),
    ("f5", "f5"),
    ("shift-f12", "shift-f12"),
    ("ctrl-c", "ctrl-c"),
    ("ctrl-l", "ctrl-l"),
    ("cmd-k", "cmd-k (fork)"),
    ("ctrl-[", "ctrl-["),
    ("ctrl-space", "ctrl-space"),
    ("ctrl-@", "ctrl-@"),
];

const KEYS: Fixture = Fixture {
    output: || "\x1b[1mkey                   bytes sent\x1b[0m".into(),
    focus: true,
    steps: || {
        let mut steps: Vec<Step> = KEY_ROWS
            .iter()
            .flat_map(|(key, label)| [Step::Feed(format!("\r\n{label:<22}")), Step::Press(key)])
            .collect();
        steps.push(Step::Feed(format!("\r\n{:<22}", "typed \"héllo\"")));
        steps.push(Step::Type("héllo"));
        steps
    },
    echo: Echo::Caret,
};
