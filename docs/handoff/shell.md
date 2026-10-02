# Shell handoff (app state, workspace, sidebar, keybindings)

On main through 89adf8d. Branch `shell` has one unlanded WIP commit: utility routes (`PullRequests`,
`Usage`, `Welcome`, `Project(key)`), `AppState::navigate_to_main_app`, and a shared sidebar footer.
It passes macOS clippy. It has not been through preland and has no Escape-to-go-back yet.

## Modules

| Module | Kind |
| --- | --- |
| `t3-logic/src/keybindings/` (matcher, parser, defaults, labels, `Command`) | reusable logic (defaults are July's; the server sends the real list) |
| `t3-logic/src/{settings,ui_state,time,paths,refs}.rs` | reusable logic; `ClientSettings` lacks fe7d3092c fields and drops unknown keys on write |
| `t3-logic/src/sidebar/` (grouping, sort, status, selection, PR badge) | July-era rules = `LegacySidebar.tsx`; no settle/snooze/pin, has the old Error pill |
| `t3-app/src/state/` (`AppState`, `Route`, `Environment`, `boot`, `fixtures`, `vcs`, `favicons`) | reusable mechanics |
| `t3-app/src/keybindings/` (root resolver, `ShortcutScope`, app menu) | reusable mechanics |
| `t3-app/src/workspace/` (collapse, rail resize, `build_main_view`, key dispatch) | reusable mechanics; `index_view.rs` is July-era visuals |
| `t3-app/src/{toast,dialogs,notifications}.rs` | reusable mechanics; toast card styling is July-era |
| `t3-app/src/sidebar/` | July-era visuals; `pulse.rs`, drag/selection plumbing, and the flows in `menus.rs` are reusable |
| `t3-snapshots/src/scenes/workspace.rs`, `fixtures/{sidebar,empty}.json` | July-era scenes |

## GPUI / gpui-kit gotchas

- Root shortcuts live on the workspace element, so they only fire while focus is inside it. The
  workspace focuses itself and refocuses on focus loss (`t3-app/src/workspace/mod.rs:100-102`).
- `Window::modifiers()` updates only on flags-changed and mouse events, not on key-down. macOS
  sends Cmd+Shift+[ as `{` with shift cleared. See `key_modifiers`
  (`t3-app/src/keybindings/mod.rs:92`) and `shifted_symbol_base`
  (`t3-logic/src/keybindings/mod.rs:97`).
- Menu accelerators sit under a key context nothing sets (`t3-app/src/keybindings/menu.rs:16`),
  so the page sees keys first, as in Electron. `cx.set_menus` reads the keymap only when called.
- `NativeMenu` dispatches actions to the focused element. The sidebar focuses itself before
  showing a menu (`t3-app/src/sidebar/menus.rs:146,196`).
- `Window::prompt` (macOS) puts Return on the first button ("No") but initial focus on "Yes"
  (`t3-app/src/dialogs.rs:18`). Unresolved.
- Headless captures have no native glass. Scenes paint an opaque backdrop
  (`t3-snapshots/src/scenes/workspace.rs:77`).
- Fixtures pin the clock (`state/mod.rs:342`). `clock_is_live` (`:337`) then stops the pulse
  clock.
- Test `find()` only sees `.test_support()` elements, so the shell tests drive by keyboard. GPUI
  has no letter spacing. Use a per-worktree `CARGO_TARGET_DIR`.

## Known bugs and limitations

- No draft store: `Route::Draft` mounts `ChatView` with `project: None`, and nothing handles
  `AppEvent::NewThread`.
- No view sets the `ShortcutScope` flags (terminal/preview open or focus, model picker).
- Favicons are never refreshed after their URL expires. Fixture environments never fetch.
- Hover fades are instant. Toasts don't animate. The settings-nav slot in `sidebar/render.rs` is a
  placeholder.

## Tests

- `cargo test -p t3-logic` runs on Linux. Each test file lists its failure modes.
- `crates/t3-app/tests/shell.rs` has three keyboard UI tests, run in CI Linux (they need the GPUI
  libs). With the focus fix reverted, all three failed (CI run 36967525662).
- Run `script/preland.sh` before landing. Snapshot PNGs come from macOS CI.

## Verified against a live server

- Data only: a nightly e2e server's `/api/orchestration/shell` decoded into the fixtures.
- The GPUI app never ran against a server: there is no Linux GPUI build and no macOS host. Archive,
  delete, rename, project remove, VCS streams, favicons, and saved-environment boot are
  compile-verified only.
