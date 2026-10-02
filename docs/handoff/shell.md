# Shell handoff (app state, workspace, sidebar, keybindings)

Branch `shell` holds one unlanded WIP commit: utility routes (`PullRequests`, `Usage`, `Welcome`,
`Project(key)`), `navigate_to_main_app`, a shared footer. macOS clippy passes; no preland run yet.

## Modules

| Module | Kind |
| --- | --- |
| `t3-logic/src/keybindings/` (matcher, parser, defaults, labels, `Command`) | reusable logic (defaults are July's; the server sends the real list) |
| `t3-logic/src/{settings,ui_state,time,paths,refs}.rs` | reusable logic; `ClientSettings` lacks fe7d3092c fields and drops unknown keys on write |
| `t3-logic/src/sidebar/` `build_sidebar` (grouping, sort, selection, PR badge) | the legacy project-grouped layout = `LegacySidebar.tsx` (`legacySidebarEnabled`) |
| `t3-logic/src/sidebar/` `build_inbox`, `snooze.rs`, `order.rs`, `status.rs` | the fork's default sectioned sidebar (see "Inbox sidebar" below); not rendered by `t3-app` yet |
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
- Test `find()` only sees `.test_support()` elements, so the shell tests drive by keyboard.

## Known bugs and limitations

- No draft store: `Route::Draft` mounts `ChatView` with `project: None`, and nothing handles
  `AppEvent::NewThread`.
- No view sets the `ShortcutScope` flags (terminal/preview open or focus, model picker).
- Favicons are never refreshed after their URL expires. Fixture environments never fetch.
- Hover fades are instant, toasts don't animate, and the settings-nav slot is a placeholder.

## Tests

- `cargo test -p t3-logic` runs on Linux. Each test file lists its failure modes.
- `crates/t3-app/tests/shell.rs` has three keyboard UI tests, run in CI Linux (they need the GPUI
  libs). With the focus fix reverted, all three failed (CI run 36967525662).
- Run `script/preland.sh` before landing. Snapshot PNGs come from macOS CI.

## Verified against a live server

- Data only: a nightly e2e server's `/api/orchestration/shell` decoded into the fixtures. The GPUI
  app never ran against a server: there is no Linux GPUI build and no macOS host. Archive,
  delete, rename, project remove, VCS streams, favicons, and saved-environment boot are
  compile-verified only.

## Inbox sidebar (logic ready, view not built)

Added by the client agent after the shell handoff; ported from fork `Sidebar.tsx` and
client-runtime `threadSettled.ts` / `threadSort.ts` (identical to upstream b33eda13).

- `build_inbox(&InboxInputs) -> InboxModel { pinned, active, working, snoozed, settled,
  draggable, active_reorderable, next_wake_at }`. Each row is an `InboxThread` with `status`
  (pill), `row_status`, `unread`, `woke`, `recedes`, and `snooze_wake_label`.
- Pass each environment's `capabilities` from its server config: without `threadSnooze` /
  `threadSettlement` nothing classifies as snoozed / settled. Rebuild at `next_wake_at`.
- Beta Working shelf: `ClientSettings::sidebar_working_shelf_enabled`. Keep one `InboxReturns` per
  window and call `observe(all threads, now)` before every build (`reset()` when the beta is off).
- Status pills now follow the fork for both layouts: no Error pill, new `Connecting` and
  `Monitoring` (both use the Working hue), and an unvisited thread has no unseen completion.
- Snooze menu: `resolve_snooze_presets(chrono::Local::now())`, `can_snooze`, custom dialog via
  `resolve_custom_snooze`. Drag/move reorders: `plan_pinned_reorder` / `plan_pinned_move` give
  `(thread id, order key)` writes for `commands::reorder_pinned_thread` / `reorder_active_thread`.
- Not ported (view state): the optimistic drop override (`applySidebarThreadDrop`), search,
  the project scope menu, multi-select menus.

