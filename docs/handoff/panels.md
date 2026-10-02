# Handoff: diff core, right panel, files (from the July-era diff/panels agent)

Target is now `~/L-Projects/t3UI-refs/t3code-fork` @ fe7d3092c. Everything visual below was matched to
the July fork and must be redone. Branch with all WIP: `diff` (efaa305), never landed.

## Modules
On main (`crates/t3-diff`, GPUI-free unless noted):
- `patch.rs`: unified/git patch parser (renames, binary, modes, `\ No newline`, truncated `[truncated]`
  patches, quoted paths, CRLF), `renderable_patch`, natural path order. Reusable logic.
- `color.rs`: CSS `color-mix` in srgb and lab (premultiplied). Reusable logic.
- `rows.rs`: unified/split row model, buffers, no-newline markers. Reusable logic.
- `word_diff.rs`: jsdiff 8 `diffWordsWithSpace` + Pierre `word-alt` joining. Reusable logic (still used
  by the new fork: emphasis shows despite `lineDiffType: "none"`, worker pool default).
- `tree.rs`: changed-files tree model, compaction, compact counts. Reusable logic; the new fork
  collapses all folders by default (no per-directory auto-expand).
- `palette.rs`: color formulas. Visual, July-era (new fork tints rows far stronger, uses
  `--code-background`). `view/*` (`DiffView`, `ChangedFilesTree`): visual, July-era; mechanics below
  are reusable.
On `diff` only:
- `t3-diff/src/review.rs`: review comment record, range build/restore, prompt block. Logic from the
  OLD fork; the new fork adds a `selection` field and moved the block to
  `packages/shared/src/composerContextLegacySend.ts` (attribute order changed). Redo before use.
- `t3-app/src/panels/model.rs`: per-thread tab model with the new kinds (diff, files, file incl.
  attachments, terminal, preview, pull-request with fork-exact ids, pull-requests, agents, device).
  Reusable logic. Missing: user-action revision + `openProactive`, persistence, dismissed devices.
- `panels/store.rs` (`RightPanels` global, width clamp, `controls_in_panel`), `panels/context.rs`
  (`PanelContext::request<M>` + `FixtureResponder` for scenes), `panels/thread_detail.rs`
  (`subscribeThread` follower): reusable mechanics. Width is now per thread in the fork
  (`t3code:preview-panel-width:{threadKey}`) with max `max(360, min(0.7·vw, row − 360))`; mine is global.
- `panels/diff_panel.rs`: data loading is reusable (turn → getTurnDiff/getFullThreadDiff, git scopes →
  review.getDiffPreview with the workspace-root retry, vcs.listRefs local+remote); chrome is July-era.
  The fork now also lazy-loads per-file patches (`components/diffs/useReviewFilePatches.ts`).
- `panels/files/tree.rs`: workspace tree from `ProjectEntry` (dirs-first natural sort, empty-dir
  flattening, search with expansion restore). Reusable logic; new fork starts all-closed and loads
  directories lazily. `files/browser.rs`, `files/style.rs`: visual, July-era, not wired.
- `panels/plan/logic.rs`: plan step model and latest-plan derivation. Reusable for the inline
  transcript plan (the plan surface was removed in the fork, rightPanelStore.ts:91).
- `panels/view.rs`: panel view (inline/sheet, resize, presence). Visual, July-era.
- `t3-logic::timeline::format_timestamp` (seconds), fixtures `t3-snapshots/fixtures/diffs/`.

## GPUI mechanics and gotchas
- Virtualize per row, not per file: `gpui_kit::list` items are header/row/footer
  (`t3-diff/src/view/diff_view.rs:269`); one item per file made long files unscrollable. Use
  `reset_with_uniform_height` (`:137`) so the scrollbar is sized before measuring.
- Sticky header = repaint the top card's header as an absolute overlay, pushed up by the footer's
  bounds (`diff_view.rs:564`). `bounds_for_item` is `None` for unmeasured items: guard it.
- Wrap mode: row heights depend on width; call `ListState::remeasure()` when the viewport width
  changes (`diff_view.rs:671`), else rows overlap.
- Horizontal scroll per file side: offset the text with negative margin inside an
  `overflow_hidden` cell and handle `on_scroll_wheel` x (`rows.rs:129`, `diff_view.rs:333`);
  GPUI has no nested horizontal scroll with a sticky gutter.
- Hatched split buffers and dashed deletion bars are painted with `Path`/quads in `canvas`
  (`rows.rs:453,480`); the hatch phase must continue across rows.
- Tabs in code: expand to columns before shaping and remap highlight ranges (`rows.rs:406`).
- Highlight per hunk side, off the main thread (`view/highlight.rs:46`), as Pierre does for partial
  patches; lines over 1000 chars stay plain.
- Snapshot scenes need several `render_frame` + `run_until_parked` rounds for background work.
- `cargo fmt -p t3-app` reformats other agents' in-progress files; rustfmt your files only.

## Known bugs and limits
- `PanelContext::request` flattens `RpcError` to "Tag: msg"; the fork shows only the message.
- Panel width/tabs/file explorer state are not persisted; no `openProactive`.
- Icons missing from t3-ui: `PanelBottom`, `PanelRight`, `ChevronsDownUp`.
- Pierre's virtualizer jitters the first card offset in Chromium captures (`randomOffset` in
  CodeView sticky positioning); don't chase it.

## Tests
`cargo test -p t3-diff --no-default-features` (core, incl. review); `cargo test -p t3-logic`;
t3-app model/store tests run in CI (needs GPUI). `script/preland.sh` covers the t3-diff core.

## Rendering @pierre/diffs references
Render the fork's real components in Chromium: a Vite app reusing the fork's toolchain (React plugin,
`@tailwindcss/vite`, `~` alias to `apps/web/src`, `server.fs.allow`) that imports the fork's real
`index.css` and mounts its CodeView wrapper with DiffPanel's exact options, captured with
playwright-core (in the fork's node_modules) at DPR 2; read computed styles through the
`diffs-container` shadow roots for exact colors. Harness: `/tmp/diff-ref` (July fork, working,
`node /tmp/diff-ref/capture.mjs`); `/tmp/diff-ref-new` points at the new fork but only its probe
works (`node /tmp/diff-ref-new/.probe.mjs "<query>"`); `capture.mjs` there still has old selectors.
Write-guard anything under `~/L-Projects` (Vite temp dirs).
