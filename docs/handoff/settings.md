# Settings, pairing, and command palette handoff

Written for the agents rebuilding settings, pairing, and the command palette against the fork at fe7d3092c. Only the pairing flow is on main; everything else is on branch `settings` (head `3064e09`, rebased on main at 2b2d99d).

## Modules

On main:
- `crates/t3-app/src/pairing/flow.rs`: reusable logic. GPUI-free add-environment flow: `split_pairing_input` (a pasted link or `t3 serve` output fills Host and code), `resolve_fields` (validation order and copy), `pair_and_save` (descriptor + `/oauth/token`, then the token to the secret store before the catalog entry), `forget`. Unit tests at the bottom.
- `crates/t3-client/examples/pair_flow.rs`: reusable test. Compiles `flow.rs` via `#[path]` and runs it against a live server (see Tests).
- `AppState::remove_environment` (`crates/t3-app/src/state/mod.rs:227`): reusable mechanics. Disconnects the client and drops the environment; a thread route into it falls back to the index.

On branch `settings` only:
- `crates/t3-logic/src/command_palette.rs`: reusable logic. Generic `filter_groups` (`>` actions filter, rank by term index and exact/prefix/contains), placeholders, Unix browse-path helpers. Re-check ranking and copy against fe7d3092c (`CommandPalette.logic.ts` grew).
- `crates/t3-logic/src/keybindings/editor.rs`: reusable logic. Wire strings (`mod+shift+o`, parenthesized `when`), command labels, Default/Custom/Project source, conflicts, search. Re-check against the new `KeybindingsSettings.logic.ts`.
- `crates/t3-app/src/settings/pages.rs`: reusable mechanics. Page registry: nav order (the fe7d3092c section list), `build(page, ..)` per page, a stand-in for unbuilt pages. Nav icons for Appearance, Project, SnapShots, Integrations, Storage are stand-ins; add the lucide icons to t3-ui.
- `crates/t3-app/src/state/route.rs` on the branch: `SettingsPage` gained Projects, Appearance, SnapShot, Integrations, Storage, OpenSourceLicenses, plus `path()`. Reusable.
- `crates/t3-app/src/settings/mod.rs`: mechanics reusable (lazy page cache, typed page access via `AnyView::downcast`, Escape-to-back, restore-defaults confirm); header rendering is visual, July-era.
- `crates/t3-app/src/settings/server.rs`: reusable mechanics (primary config, `server.updateSettings` with an error toast).
- `crates/t3-app/src/settings/general.rs`, `providers.rs`, `connections.rs`, `archived.rs`, `layout.rs`, `nav.rs`: visual, July-era. Reusable parts: upstream setting defaults and mappings (`general.rs` `Effective`, `default_text_model`), provider row building and `providerInstances` writes (`providers.rs` `build_rows`, `write_instance`, `reset_default`), email scramble `redacted`, `ArchivedCache` (`archived.rs`), saved-environment Connect/Disconnect (`connections.rs`).
- `crates/t3-app/src/pairing/mod.rs` on the branch: `PairingForm` mechanics are reusable (paste splitting into both fields, submit, start the environment, emit `Added`); `AddEnvironmentDialog` and `PairView` are visual, July-era.
- `crates/t3-app/src/command_palette/mod.rs`: mechanics reusable (global toggle, view stack, highlight + scroll, `capture_key_down`, browse via `filesystem.browse`, `project.create` then a new draft); rendering is visual, July-era.
- `crates/t3-snapshots/src/scenes/{settings,command_palette}.rs`: scene harness reusable (recorded fixtures, `ArchivedCache` seeding, palette layered over the workspace); scenes target July references.
- `crates/t3-snapshots/src/main.rs:88-97` and `scenes/mod.rs:52` on the branch: `Scene::settle()`, the fix for the capture issue below. Worth landing.

Mounting: `SettingsView::new(app_state, window, cx)`, `SettingsNav::new(app_state)`, `PairView::new(..)`, `CommandPalette::new(..)` are the constructors; the shell owns `workspace/main_column.rs` and `sidebar/render.rs` and has not mounted them.

## GPUI gotchas

- Dialog fades run on wall-clock time from their first frame (`crates/t3-ui/src/components/dialog.rs:351,357`). The snapshot harness draws twice back to back (`crates/t3-snapshots/src/main.rs:82-87` on main), so dialogs and toasts are captured near opacity 0. Fix on the branch: opt-in `Scene::settle()` sleeps 500ms and redraws. It is opt-in because the terminal cursor blink also runs on wall-clock time.
- gpui-base binds Escape to `Cancel` in its Dialog/Popover contexts and keymap actions run before `on_key_down`, so an open dialog consumes Escape before the settings Escape-to-back handler (`settings/mod.rs:109` on the branch).
- `InputState::set_value` emits no `InputEvent::Change` (gpui-base `input/base/state.rs:925`). Update dependent state yourself after programmatic sets (palette `set_query`, branch `command_palette/mod.rs:280`).
- On `&mut InputState`, `.placeholder()` resolves to the builder (`state.rs:799`), not the getter; use `set_placeholder` and track the current value yourself.
- The coss `Dialog` caps the popup at 512px (`dialog.rs:77,356`); wider dialogs (Add Environment is 768) compose `base::Dialog` directly.
- `cx.set_global(X(cx.weak_entity()))` is a borrow error; bind the weak handle first.
- `relative()` padding is relative to the parent width; `10vh` needs `window.viewport_size().height * 0.1`.
- A `form_urlencoded::Serializer` held across `.await` makes the future `!Send`, so `runtime::spawn(pair(..))` fails to compile. Main fixed it (`crates/t3-client/src/http.rs:435`).
- GPUI has no letter-spacing and no blur filter (tracked labels and the blurred redacted email can't match exactly).

## Known bugs and limitations

- Pairing has no SSH mode, no `/connect` route, and no deep link; `split_pairing_input` handles Unix-style URLs only.
- Browse paths in the palette are Unix-only (no `\` or drive letters); clone flows (Git URL, providers) are not built.
- Keybindings, Source Control pages are empty stubs on the branch; recording, the when-builder, upsert/remove calls are not wired.
- Providers: no add-instance dialog, environment-variable editor, custom models, or HSV accent picker; the `+` header button does nothing.
- Settings scope (project/machine/checkout), settings search, and the new pages from fe7d3092c are not started.

## Tests

- `cargo test -p t3-logic command_palette editor` (branch) and `cargo test -p t3-client` cover the pure logic. `flow.rs` tests live in t3-app (GPUI crate, macOS CI only).
- End to end: `T3UI_PORT=4760 T3UI_RUN_DIR=/tmp/t3ui-e2e/run-settings e2e/run-local.sh up --server nightly --detach`, then `LINK=$(node e2e/seed.mjs --pair --state /tmp/t3ui-e2e/run-settings/state.json | tail -1 | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).pairingUrl))')` and `cargo run -p t3-client --example pair_flow -- "$LINK" --data-dir /tmp/pair-flow`. It writes `pair-flow-report.json` there. `seed.mjs --pair` prints JSON, not a CLI line, hence the extraction. Stop the server with `e2e/run-local.sh down` and the same env.

## Verified live

Against `t3@0.0.45-nightly.20261002.2561` with the fake Codex: the pasted link split, empty-field copy, pair and save (catalog + token), a restart from disk the way `state::boot` does it reaching Connected with 3 projects in the shell, a reused code failing with "The environment credential is invalid.", an unreachable host timing out, and forget removing entry and token. All steps passed.
