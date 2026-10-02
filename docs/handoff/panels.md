# Handoff: diff core, right panel, files (from the July-era diff/panels agent)

Target is now `~/L-Projects/t3UI-refs/t3code-fork` @ fe7d3092c. Everything visual below was matched to
the July fork and must be redone. Branch with all WIP: `diff` (efaa305), never landed.

## Modules
On main (`crates/t3-diff`, GPUI-free unless noted):
- Reusable logic: `patch.rs` (git/unified parser: renames, binary, modes, `\ No newline`, truncated
  patches, quoted paths, CRLF; natural path order), `color.rs` (CSS `color-mix` srgb/lab),
  `rows.rs` (unified/split rows, buffers, no-newline markers).
- `word_diff.rs` (jsdiff 8 + Pierre `word-alt`; the fork still emphasizes words via the worker pool
  default) and `tree.rs` (changed-files tree; the fork now starts all folders collapsed): reusable.
- `palette.rs` and `view/*` (`DiffView`, `ChangedFilesTree`): visual, July-era; mechanics reusable.
On `diff` only:
- `t3-diff/src/review.rs`: review comment model + prompt block from the OLD fork; redo (fork adds
  `selection`, block moved to `packages/shared/src/composerContextLegacySend.ts`).
- `t3-app/src/panels/model.rs`: per-thread tab model with the new kinds (diff, files, file incl.
  attachments, terminal, preview, pull-request with fork-exact ids, pull-requests, agents, device).
  Reusable logic. Missing: user-action revision + `openProactive`, persistence, dismissed devices.
- Reusable mechanics: `panels/store.rs` (`RightPanels`, width clamp), `panels/context.rs`
  (`PanelContext::request<M>`, `FixtureResponder` for scenes), `panels/thread_detail.rs`. The fork's
  width is per thread (`t3code:preview-panel-width:{threadKey}`), max `max(360, min(0.7vw, row-360))`.
- `panels/diff_panel.rs`: data loading is reusable (turn → getTurnDiff/getFullThreadDiff, git scopes →
  review.getDiffPreview with the workspace-root retry, vcs.listRefs local+remote); chrome is July-era.
  The fork now also lazy-loads per-file patches (`components/diffs/useReviewFilePatches.ts`).
- `panels/files/tree.rs`: workspace tree logic, reusable (fork: all-closed, lazy dirs);
  `files/browser.rs`, `files/style.rs`: visual, July-era, unwired.
- `panels/plan/logic.rs`: plan steps, reusable inline (plan surface removed, rightPanelStore.ts:91).
- `panels/view.rs`: visual, July-era. Also: `t3-logic` `format_timestamp`, `fixtures/diffs/`.

## GPUI mechanics and gotchas
- Virtualize per row: `list` items are header/row/footer (`t3-diff/src/view/diff_view.rs:269`),
  `reset_with_uniform_height` (`:137`) sizes the scrollbar before measuring.
- Sticky header = repaint the top card's header as an absolute overlay, pushed up by the footer's
  bounds (`diff_view.rs:564`). `bounds_for_item` is `None` for unmeasured items: guard it.
- Wrap mode: `ListState::remeasure()` when the width changes (`diff_view.rs:671`), else rows overlap.
- Horizontal scroll per file side: offset the text with negative margin inside an
  `overflow_hidden` cell and handle `on_scroll_wheel` x (`rows.rs:129`, `diff_view.rs:333`);
  GPUI has no nested horizontal scroll with a sticky gutter.
- Hatch/dashed bars are `canvas` paths, phase continued across rows (`rows.rs:453,480`); expand tabs
  before shaping and remap highlights (`rows.rs:406`).
- Highlight per hunk side off the main thread like Pierre (`view/highlight.rs:46`); >1000 chars plain.
- Scenes need several `render_frame` + `run_until_parked` rounds; rustfmt only your own files.

## Known bugs and limits
- `PanelContext::request` flattens `RpcError` to "Tag: msg"; the fork shows only the message.
- Panel width/tabs/file explorer state are not persisted; no `openProactive`.
- Icons missing from t3-ui: `PanelBottom`, `PanelRight`, `ChevronsDownUp`.
- Pierre jitters the first card's offset in captures (`randomOffset` in CodeView); don't chase it.

## Tests
`cargo test -p t3-diff --no-default-features` (core, preland runs it), `cargo test -p t3-logic`;
t3-app model/store tests run only in CI (GPUI).

## Rendering @pierre/diffs references
A Vite app reusing the fork's toolchain (React, `@tailwindcss/vite`, `~` alias, `server.fs.allow`)
imports the fork's real `index.css`, mounts its CodeView wrapper with DiffPanel's exact options, and
playwright-core captures at DPR 2; read colors through the `diffs-container` shadow roots.
`/tmp/diff-ref` (July fork, `node /tmp/diff-ref/capture.mjs`); `/tmp/diff-ref-new` targets the new fork
but only `node /tmp/diff-ref-new/.probe.mjs "<query>"` works. Guard writes under `~/L-Projects`.
