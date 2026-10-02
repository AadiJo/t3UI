# Chat view handoff

Written for the agent rebuilding the chat UI against the fork at fe7d3092c. Visuals in `crates/t3-app/src/chat/` follow the July fork; keep the mechanics and replace the visuals.

## Modules

On main:
- `crates/t3-logic/src/timeline/` (`mod.rs`, `rows.rs`, `work_log.rs`, `text.rs`, `format.rs`, `plan.rs`, `tests.rs`): reusable mechanics (`TimelineModel`, `share_rows`, `diff_rows`/`RowsDiff`, shell-wrapper unwrapping, exit-code stripping, changed-file collection, `format_*`). The row rules follow the July logic. The fork rewrote them: new row kinds, reasoning rows, keyed lifecycle collapse, `turn.plan.updated` is no longer a work row, agent spawn batches, user-input folding. Port `MessagesTimeline.logic.ts` and `session-logic.ts` again from fe7d3092c.
- `crates/t3-app/src/chat/mod.rs`: reusable mechanics. Thread subscription, local-dispatch acknowledgement (port of `hasServerAcknowledgedLocalDispatch`, `mod.rs:119`), completion acknowledgement (`mod.rs:399`), the slot registry (`register_slot`, `mod.rs:92`), overlay inset geometry (`OverlayGeometry::measure`, `mod.rs:178`).
- `crates/t3-app/src/chat/timeline.rs`: reusable mechanics. GPUI list bookkeeping, tail follow, manual-navigation flag, anchored disclosure, Markdown/ChangedFilesTree caches.
- `crates/t3-app/src/chat/body.rs`: mechanics reusable (overlay measurement, toggles, revert flow); the layout code is July visuals.
- `crates/t3-app/src/chat/{rows,header,banners,controls}.rs`: visual, July-era. `markdown.rs` (view cache) and `fixtures.rs` (detached-environment thread table) are reusable.
- `crates/t3-snapshots/src/scenes/chat.rs`: scene harness reusable (loads `fixtures/threads/*.json`, pins the clock); the scenes target the July references.
- `crates/t3-client/tests/timeline_streaming.rs`: reusable (replays recorded token streams through `TimelineModel`).

On branch `chat` (not on main): `701303d` adds `crates/t3-logic/src/timeline/presentation.rs`, a port of `client-runtime/src/work-log/presentation.ts` + `toolPresentation.ts` from fe7d3092c (labels, group summaries, status predicates, T3 MCP tool names). It is not declared in `timeline/mod.rs` and needs new `WorkLogEntry` fields (`viewed_image_path`, `tool_source`, `task_id`, `agent_spawn`, a `for_test` constructor) before it compiles. `command_program_name` there is a simplified stand-in for the 1.3k-line shell parser in `commandLabel.ts`.

## GPUI / gpui-kit gotchas

- gpui-component's `MessageScroller` forces its own row padding (`px_3`, `pb_8`), so the timeline uses `gpui::list` + `ListState` directly (`timeline.rs:94`).
- `ListState::splice` resets the scroll anchor to the start of the spliced range. Rows that kept their key get `remeasure_items`, which keeps the anchor (`timeline.rs:180`).
- The list's scroll handler fires only for wheel scrolls. Scrollbar drags show up only through `is_scrollbar_dragging()`, which is polled when the pill renders (`timeline.rs:99`, `:304`).
- `window.on_next_frame` callbacks run before that frame draws, so the first callback after a toggle still sees the old layout. The anchored-disclosure loop needs extra stable frames when nothing moved (`timeline.rs:257`, `body.rs:244`). One frame of movement is still visible.
- GPUI has no letter spacing. `controls.rs:99` fakes `tracking-*` with one box per character.
- GPUI has no backdrop blur. Glass surfaces fall back to solid `card` (spec 2.2).
- The overlay is measured with `on_children_prepainted` (`body.rs:176`). Updating the view from inside it costs one extra frame before the insets apply.
- `t3_markdown::Markdown` needs `gpui_kit::base::TextSelectionLayer` in the window root (`scenes/chat.rs:184`).
- Headless snapshot windows are never active, so completion acknowledgement never runs there. Scenes mark threads visited themselves (`scenes/chat.rs:101`).
- `script/check-macos.sh` restores `Cargo.lock` on exit. After adding a path dependency, run `cargo metadata --format-version 1 >/dev/null` to write the lock entries you need.

## Known bugs and limitations

- Timeline rows follow July rules (see Modules). Message `phase` (`final_answer`/`commentary`) exists only in the fork's contracts, not upstream b33eda13 or `t3-protocol`.
- No keyboard scrolling of the timeline (PageUp/Down/Home/End/arrows); GPUI `list` has none.
- Not built: tool-call morph, working shimmer, minimap, attachments, context chips, image dialog, plan card menu actions, "View diff" opening the diff panel. Header actions, composer, and branch toolbar are static stand-ins until their slots are registered.
- Copy feedback is a check icon only; there is no anchored "Copied!" toast.
- Timestamps use local time with en-US formatting for `Locale`.
- Behavior at fe7d3092c that the July-era code lacks (read-only survey, not ported):
  - Revert is now an in-app dialog offering "revert files too" or "keep changes", and it restores the prompt into the composer. Ours uses `window.prompt` (`body.rs:268`).
  - The timeline inset now comes from `composerFooterLayout.ts:82` `resolveComposerTimelineInset`. `composerTimelineGeometry.ts` is gone.
  - The completion acknowledgement is local only, which matches ours.

## Tests

```
export CARGO_TARGET_DIR=$HOME/L-Projects/t3UI-targets/<agent> CARGO_BUILD_JOBS=3
cargo test -p t3-logic timeline                              # derivation, recorded fixtures
cargo test -p t3-client --test timeline_streaming            # streamed turns through TimelineModel
CARGO_SUBCOMMAND=clippy script/check-macos.sh -p t3-app -p t3-snapshots --all-targets -- -D warnings
script/preland.sh                                            # before landing
```
Pixels: push a branch, run `gh workflow run ci.yml --ref <branch>`, then download the `snapshots` artifact (`chat-*.png`).

## Verified against a live server

- Ran a seeded nightly (`e2e/run-local.sh up --server nightly`, port 4750) and dumped thread detail over HTTP. That dump matched the shared `fixtures/threads/*.json` closely, and the derivation tests run on those fixtures.
- The streaming test replays a real token-level recording (`t3-client/tests/fixtures/stream-scenarios.jsonl`).
- Not verified live: the GPUI view mounted in the running app against a server (sending, interrupt, revert, scrolling). Only headless snapshot scenes on CI ran it.
