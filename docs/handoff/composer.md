# Composer handoff

Branch `composer` (origin) holds all composer work; main has none. Visuals target the July fork
(ddeeb09); `t3-logic::composer` was partly re-diffed against fe7d3092c.

## Modules
| Module | Where | Status |
| --- | --- | --- |
| `crates/t3-logic/src/composer/{prompt,search,menu}.rs` | branch | Reusable logic, updated to fe7d3092c: chip grammar (file links, `@` with scoped-package rule, currency-sigil skills minus amounts, `t3-context://v1` refs, `t3-citation://v1`), triggers incl. `#` pull request, ungrouped `/` menu with `/skill:` rows and prompt-start rule, skill dedupe/invocable/source kinds. |
| `.../composer/send.rs` | branch | Reusable, updated: context refs not sendable text, 120,000-char limit copy, title seed, image limits. Steer/queue follow-up behavior not ported. |
| `.../composer/providers.rs` | branch | Reusable logic, July-era: model resolution order, traits labels, picker filtering/search. Not re-diffed (fast-mode bolt, new resolution order unchecked). |
| `.../composer/pending.rs` | branch | Reusable: question answer state, context-window token format. Requests come from `t3_client::pending_requests`. |
| `.../composer/draft.rs` | branch | July-era shape (`drafts.json`, U+FFFC terminal metadata). Needs the new context-record model. |
| `crates/t3-app/src/composer/editor.rs`, `chips.rs` | branch | Reusable mechanics (InlineToken chips). Chip kinds still July-era (no context/citation kinds). |
| `.../composer/drafts.rs` (`DraftStore`) | branch | Reusable mechanics: persistence, 300ms debounce, quit flush, `AppEvent::NewThread` -> `open_draft`, promotion to a server thread. |
| `.../composer/mod.rs` (send pipeline, approvals, questions, paste/drop, path search) | branch | Mechanics reusable; render code visual, July-era. |
| `footer.rs`, `model_picker.rs`, `traits.rs`, `command_menu.rs`, `panels.rs`, `branch_toolbar.rs`, `style.rs`, `t3-snapshots/src/scenes/composer.rs` | branch | Visual, July-era. |
| `crates/t3-app/src/composer/chat_slots.rs` | branch | Slot glue for ChatView (`register_chat_slots`). Not declared in `mod.rs` and never compiled against main's ChatView. |

**Branch tip:** `t3-logic` passes; `t3-app` does NOT build (`editor.rs`/`mod.rs` use the removed U+FFFC
placeholder, old `menu_items` args, `MenuIcon`, 3-arg `has_sendable_content`). `ae42857` (pre re-diff)
builds and passes macOS clippy for t3-app + t3-snapshots.

## GPUI / gpui-base 0.7 gotchas
- Chips: `InlineToken::new(id, text).with_label(label)` where `text` is the prompt source, so the input's
  `value()` is the prompt string with no serialization. Tokens need non-empty id/text/label without control
  chars (`gpui-base-0.7.0/src/input/base/inline_tokens.rs:41`), and are refused in code-editor/masked modes
  (`:325-329`). gpui-base handles caret skipping, whole-chip delete and undo.
- Typed tokens do not become chips by themselves: `sync_chips` re-scans after every `InputEvent::Change`
  and calls `replace_range_with_token`, then restores the selection (`composer/editor.rs:109-136`).
  Guard with an `applying` flag so your own edits don't recurse.
- Use gpui-base's unstyled `Textarea` (`base::input::Textarea`), not gpui-component's: the styled one
  re-applies paddings from its `Size` every render (`gpui-component-0.7.0/src/input/input.rs:579`) and
  draws its own border/background.
- Unstyled inputs paint with transparent ink unless you call `set_editor_style` (`editor.rs:35`); unset
  colors resolve from the theme (`gpui-base .../editor/highlighting.rs:137`).
- Enter inserts a newline unless `submit_on_enter`; we `capture_action::<Enter>` on a wrapper and stop
  plain Enter (`composer/mod.rs:1711`); same for `MoveUp`/`MoveDown`/`IndentInline`/`Paste` (`:1696-1717`).
- Text decorations exist only in `EditorMode` (`gpui-base .../editor/decorations.rs:496`), which forbids
  tokens, so rich-text composer styling needs a custom element; the branch shows markdown literally.
- `AppEvent` subscribers run at effect flush; scenes call `DraftStore::open_draft` directly (`drafts.rs:212`).
- `base::ElementExt::on_prepaint` measures bounds (`mod.rs:1759`). Import traits `TaskExt`
  (`detach_and_log_err`) and `StyledImage` (`object_fit`).

## Known bugs and limitations
- t3-app composer does not build at branch tip (see above). CI run 36967900180 (at 1343a77's tree) rendered
  every composer scene (artifact `snapshots`); the PNGs were never compared to any reference.
- No PR (`#`) search, no queue/steer follow-ups, no prompt history, no stash, no resting composer, no
  rich-text styling, no context records (images/terminal/element as `t3-context` refs), no banner stack.
- Branch picker loads only the first 50 refs; `footer.rs` keeps the form width in a shared `thread_local`.

## Tests
- `cargo test -p t3-logic composer` (20 tests; failure modes at the top of `composer/tests.rs`; some read
  `crates/t3-snapshots/fixtures/server-config.json`).
- `CARGO_SUBCOMMAND=clippy script/check-macos.sh -p t3-app -p t3-snapshots --all-targets -- -D warnings`.

## Verified against a live server
- Read-only against the chat agent's nightly e2e run: `server.getConfig` (providers, option
  descriptors, slash commands, skills), `projects.searchEntries`, `vcs.listRefs`, thread snapshots with
  a pending approval and question. Send, uploads, and approval/answer dispatch were never run end-to-end.
