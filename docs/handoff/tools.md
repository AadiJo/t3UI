# Handoff: terminal drawer and header tools (scripts, Git actions, PR dialog)

Source of truth is fork `fe7d3092c`. Nothing here describes visuals; re-derive those from the fork.

## Modules
On main (`54ad46e`, `7f955f7`; pure, no GPUI, tested). These are reusable logic and mechanics.
- `crates/t3-logic/src/terminal_layout.rs`: per-thread drawer state (`terminalUiStateStore.ts`, unchanged in fe7d3092c). It covers open/height, `term-N` ids, groups with max 4 per split, close/activate rules, server reconcile with closed-id suppression, height clamp, and the `{terminalUiStateByThreadKey}` JSON. Every transition returns whether it changed anything.
- `crates/t3-logic/src/source_control.rs`: `GitActionsControl.logic.ts` (unchanged in fe7d3092c), plus menu disabled reasons, warnings, progress stages, default-ref dialog copy and provider words (incl. Forgejo). Also `parse_pull_request_reference` (gh/glab/tea/az, GitHub/GitLab/Forgejo/Azure URLs, `#N`).
- `crates/t3-logic/src/project_scripts.rs`: script ids, `script_command` (legacy ids get no shortcut), `ScriptInput`/`build_script` (`async:false` only for waiting setup scripts). Also `resolve_project_scripts` and `scripts_patch`. Scripts now live in server settings (`projectSettingsOverrides[id].defaultProjectScripts`, folded/legacy fallbacks), not `project.meta.update`. Also covers shortcut capture and runtime env.

On branch `tools` only (never compiled; `lib.rs` does not declare these modules):
- `crates/t3-app/src/terminal_drawer/session.rs`: reusable mechanics. It has `terminal.attach` across reconnects, 512 KB history trim, coalesced `terminal.resize` that is re-sent after each snapshot, write errors printed as `[terminal] msg`, and exit handling deferred one tick (session.rs:115, :191, :207, :312).
- `crates/t3-app/src/terminal_drawer/store.rs`: reusable mechanics. It holds the global layouts, the metadata stream, toggle/new/split/close (close falls back to typing `exit\n`), `run_script` (reuses the active terminal unless busy), and LRU session retention (store.rs:186, :302, :414).
- `crates/t3-app/src/terminal_drawer/view.rs`, `crates/t3-app/src/header_tools/*`: visual, July-era. Rewrite from fe7d3092c; mine only for wiring patterns.
- `crates/t3-app/src/chat/mod.rs` on `tools` adds `Slot::TerminalDrawer`, rendered as the chat column's last child (small, reusable). `toast::update` and `AppState::store()` are tiny shell additions there.

## GPUI gotchas
- A focused child's mouse-down `prevent_default`s ancestors, so a container that tracks the `terminalFocus` handle does not steal focus from `TerminalView` (gpui-pre `elements/div.rs:2765`).
- `on_drag_move` listens in capture phase without a hover check (div.rs:360). The drag cursor comes from the dragged element's own `cursor()` (div.rs:2563).
- Window-wide mouse tracking for a resize drag works by registering `window.on_mouse_event` inside a `canvas` paint closure (window.rs:5246; pattern in gpui-base `resizable/panel.rs:473`).
- `t3_ui::Button` refines caller styles last (`components/button.rs:523`), so joined groups can square one side. Its bevel and focus ring children still use the full radius.
- `t3_ui::Dialog` hardcodes `max_w(512)` (`components/dialog.rs:356`), so `max-w-xl` dialogs need a primitive change.
- `TooltipExt` only places tooltips on top (`components/tooltip.rs:65`). Other sides need `base::Positioner` (see `header_tools/widgets.rs` on `tools`).
- `MenuItem` takes only an `IconName` (`components/menu.rs:126`); logos and custom rows need hand-built rows. Menus close by dispatching `base::actions::Cancel` (menu.rs:108).
- Root shortcuts run from `Workspace::on_key_down` (`workspace/mod.rs:186`), not the GPUI keymap. `TerminalView::on_key_down` (t3-terminal `view.rs:546`) consumes Ctrl+letter first, so `mod+d` / `mod+n` inside a terminal only bubble on macOS.
- `ShortcutScope::register_terminal_focus` never unregisters (`keybindings/mod.rs:48`). Register one handle once; my store did this at store.rs:111.
- Stream Acks are handled in the transport (`t3-client/src/rpc.rs:451`). Never ack per feature.
- Call `t3_terminal::init(cx)` (view.rs:54) once at startup. `lib.rs::run` does not yet.

## Known bugs and limitations
- The `tools` app code has never compiled. Expect API mismatches.
- The fork moved the terminal to a Ghostty surface (4px content padding, right-click menu with Add to chat/Copy/Paste, selection popup with Add to chat + Copy). Close now goes through `confirmTerminalClose`. `t3-terminal` exposes no selection getter for a right-click menu.
- Panel animation defaults to 0 ms (`panelAnimationDurationMs`), so the drawer opens instantly. The open/close animation in `view.rs` is obsolete.
- Not ported: t3.json script import (`useT3ProjectFileScripts`), the publish repository wizard, live thread-branch sync (`resolveLiveThreadBranchUpdate`), and the header's collapsed "menu" presentation (<512px).
- Script run failures should set the thread error banner (fork `setThreadError`). My WIP used a toast.
- The quick action wants a `CloudDownload` icon (pull) and the sidebar wants `Square`. Neither is exported in `assets/icons/lucide` yet.

## Tests
- `cargo test -p t3-logic` covers terminal_layout (12 tests), source_control (8) and project_scripts (5). Each test module lists its failure modes first.
- `script/preland.sh` runs them along with the rest of the GPUI-free crates.

## Live server verification
- None of the app-level RPC paths were exercised. My seeded nightly (port 4770) ran, but the drawer and header code never compiled, so attach/write/resize round-trips, `git.runStackedAction`, `vcs.pull` and `server.updateSettings` script saves are all unverified.
- Server semantics I read in upstream `apps/server/src/terminal/Manager.ts:2675-2724`: `terminal.attach` with `cwd` opens a missing session, `restartIfNotRunning` restarts a dead one, and `cols`/`rows` resize a running PTY.
- Seeded repos are on `main` with uncommitted changes and no remote, so the quick action resolves to Commit. Add a bare remote to test push and PR rows.
