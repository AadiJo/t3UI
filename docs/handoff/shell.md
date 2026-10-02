# Shell handoff (app state, workspace, sidebar, keybindings)

Main is at e2ea6bb or later. Branch `shell` (06a050c) adds one unlanded WIP commit: utility routes
(`PullRequests`, `Usage`, `Welcome`, `Project(key)`), `AppState::navigate_to_main_app`, and a shared
sidebar footer. It passes clippy for macOS but has no preland run and no Escape-to-go-back yet.

## Modules

| Module | Kind |
| --- | --- |
| `t3-logic/src/keybindings/` (matcher, parser, defaults, labels, `Command`) | reusable logic. Defaults are the July fork's; upstream's list is a superset, and the server sends the real list anyway |
| `t3-logic/src/{settings,ui_state,time,paths,refs}.rs` | reusable logic. `ClientSettings` lacks the many new fe7d3092c fields (`legacySidebarEnabled`, `sidebarWorkingShelfEnabled`, ...), and unknown keys are dropped on write |
| `t3-logic/src/sidebar/` (grouping, sort, status, selection, PR badge) | July-era rules. They match `LegacySidebar.tsx`, not the new default sidebar. Status rules predate upstream's lifecycle (no settle/snooze/pin, has the fork's Error pill) |
| `t3-app/src/state/` (`AppState`, `Route`, `Environment`, `boot`, `fixtures`, `vcs`, `favicons`) | reusable mechanics |
| `t3-app/src/keybindings/` (root resolver, `ShortcutScope`, `menu.rs` app menu) | reusable mechanics |
| `t3-app/src/workspace/` (offcanvas collapse, rail resize, main column `build_main_view`, root key dispatch) | reusable mechanics. `index_view.rs` is July-era visuals |
| `t3-app/src/toast.rs` (stack, timers, thread scoping, actions), `dialogs.rs`, `notifications.rs` | reusable mechanics; the card styling is July-era |
| `t3-app/src/sidebar/` | July-era visuals. These are reusable: `pulse.rs` (one shared ~30fps clock), drag/drop and selection plumbing, `menus.rs` delete/archive/rename/remove flows (fe7d3092c behavior not checked) |
| `t3-snapshots/src/scenes/workspace.rs`, `fixtures/sidebar.json`, `fixtures/empty.json` | July-era scenes |

## GPUI / gpui-kit gotchas

- Root shortcuts listen on the workspace element. They only fire while focus is inside it. The
  workspace focuses itself and refocuses on focus loss: `t3-app/src/workspace/mod.rs:100-102`.
  Without that, a fresh window or a closed dialog leaves no shortcuts working.
- `Window::modifiers()` is only updated by flags-changed and mouse events, not by key-down. On
  macOS, Cmd+Shift+[ arrives as `{` with shift cleared. The fix is in
  `t3-app/src/keybindings/mod.rs:92` (`key_modifiers`) plus `shifted_symbol_base` in
  `t3-logic/src/keybindings/mod.rs:97`.
- Menu accelerators are bound under a key context nothing sets
  (`t3-app/src/keybindings/menu.rs:16`). That makes the page see keys before the menu does, as in
  Electron. `cx.set_menus` reads the keymap only when called.
- `NativeMenu` dispatches its item actions to the focused element. The sidebar focuses itself
  before showing a menu (`t3-app/src/sidebar/menus.rs:146,196`), and the handlers live on the
  sidebar root.
- `Window::prompt` on macOS puts Return on the first button ("No"), but focus/Space goes to the
  last non-cancel button ("Yes") (`t3-app/src/dialogs.rs:18`). This mismatch is unresolved.
- Headless captures have no native glass behind the translucent fills. The scenes paint an opaque
  backdrop: `t3-snapshots/src/scenes/workspace.rs:77`.
- Fixtures pin the clock (`AppState::set_fixed_now`, `state/mod.rs:342`). `clock_is_live` (`:337`)
  then turns the pulse clock off so captures are reproducible.
- `gpui_kit::test` `find()` only sees elements opted in with `.test_support()`. The shell tests
  avoid it and drive everything by keyboard.
- GPUI has no letter spacing, so the `tracking-*` classes are not reproduced.
- Use a per-worktree `CARGO_TARGET_DIR` (AGENTS.md). `script/check-macos.sh` rewrites
  `Cargo.lock`; the script now restores it.

## Known bugs and limitations

- No draft store exists. `Route::Draft` mounts `ChatView` with `project: None`, and nothing handles
  `AppEvent::NewThread` yet.
- No view sets the `ShortcutScope` flags (`terminal_open`, `preview_open`, `model_picker_open`,
  terminal/preview focus) yet.
- Favicons are fetched once per session and never refreshed after the URL expires. Detached
  (fixture) environments never fetch.
- Hover fades are instant. Toasts have no enter/exit animation and don't expand on hover.
- The settings nav slot in `sidebar/render.rs` is still a placeholder.

## Tests

- `cargo test -p t3-logic`: runs on Linux. Unit tests for keybindings, sidebar model, selection,
  settings, ui state and time; each file lists its failure modes.
- `cargo test -p t3-app --test shell` (`crates/t3-app/tests/shell.rs`): three keyboard-driven UI
  tests. They need the GPUI system libs, so they run in CI Linux. Pushing a revert of the focus fix
  made all three fail (CI run 36967525662).
- `script/preland.sh` before landing. macOS snapshot PNGs come from CI.

## Verified against a live server

- Only data has been verified. A real nightly e2e server's `GET /api/orchestration/shell` decoded
  into the fixtures, and the recorded `shell.json` / `server-config.json` decode.
- The GPUI app has never run against a live server: there is no Linux GPUI build here and no
  macOS host. Archive, delete, rename, project remove, VCS streams, favicon fetch and saved
  environment boot are compile-verified only.
