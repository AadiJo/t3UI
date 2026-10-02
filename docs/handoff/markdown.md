# Handoff: t3-highlight + t3-markdown

Matched against the July fork (`t3code-again` ddeeb09), not fe7d3092c. Unlanded WIP: branch `markdown` (block caching, below).

## Modules
- `t3-highlight` (all **reusable logic/mechanics**). `src/engine.rs`: syntect, bat grammars + Shiki grammars converted for syntect, `StreamingHighlighter`. `src/language.rs`: Shiki + `@pierre/diffs` language resolution. `src/cache.rs`: LRU cache (500 entries / 50 MB). `tools/export-assets.mjs` regenerates `assets/` (themes, `languages.tsv`, grammars via `tools/grammars`) from a fork's node_modules. **The assets are July-era: regenerate them against fe7d3092c.**
- `t3-markdown/src/parse.rs`, `document.rs`, `streaming.rs` (port of `streamingMarkdown.ts`), `links.rs` (port of `markdown-links.ts`), `copy.rs` (port of `markdown-clipboard.ts`): **reusable logic**. Re-diff them against the new .ts sources.
- `t3-markdown/src/inline_text.rs`: **reusable mechanics**. A text element with mixed font sizes, CSS-style wrapping, atoms (inline elements), window text selection and copy.
- `t3-markdown/src/view.rs` (the `Markdown` entity, events, per-block state, background highlighting): **reusable mechanics**.
- `t3-markdown/src/render.rs` margin collapsing + device-pixel residual (`stack`, `rounding_excess`, render.rs:69-140): **reusable mechanics**. Everything else in `render.rs` and all of `style.rs`: **visual, July-era**.
- `t3-snapshots/src/scenes/markdown.rs`, `t3-markdown/fixtures/sample.md`: scene harness, reusable.

## GPUI gotchas
- font-kit reports descent as a **negative** number. Take its absolute value and round both metrics like Blink, or every line sits about 0.3em low (inline_text.rs:483 `font_metrics`).
- GPUI rounds measured sizes **up** to whole device pixels and snaps fixed lengths before layout. Chromium keeps fractions, so blocks drift. Text heights round to the nearest device pixel, alternating ties between blocks (inline_text.rs:1190 `snap_height`). Spacers carry the leftover rounding (render.rs:86 `stack`), and code/table padding rounding is accounted for (render.rs:69).
- The baseline is the Blink strut: `(line_height - ascent - descent)/2 + ascent` using the block's own font (inline_text.rs `TextLayout::compute`).
- Blink never breaks a line after `/`, but UAX#14 does (inline_text.rs:592).
- Selection order is by window position, because the base crate's paint-order counter is private (inline_text.rs:1155). That's wrong for multi-column layouts.
- Cached views (`Entity::cached`) need a definite size, and their cache key includes **absolute bounds**. Any block that moves re-renders (gpui-pre-0.3.7 `src/view.rs:484`). A notify only dirties a view's ancestors, never its children (`window.rs:2148`).
- gpui-kit's `render_frame` calls `refresh()`, which re-renders every cached view (gpui-kit-0.7.0 `src/test.rs:219`). Time frames with `window.draw(cx).clear(cx)`.
- `Window::with_element_state` can't run inside mouse listeners. Share state through `Rc<RefCell>` (inline_text.rs `Pointer`).
- syntect keeps the **first** of two equally specific theme rules, while VS Code keeps the last, so the theme converter emits rules in reverse (export-assets.mjs:130). fancy-regex can't express `\G` or variable-length lookbehinds, so the converter rewrites patterns (export-assets.mjs `onigToFancy`, `splitLookbehinds`). That's also why Bash stays on bat's grammar.

## Known bugs / gaps
- Branch `markdown` (294807f): caching stable blocks as cached views (`src/units.rs`) passes preland and is measured (numbers below). It hasn't been checked in a rendered snapshot. After a width change, a cached block can overlap its neighbour for one frame.
- Bash highlights at 91.8% parity. Lines over 20k bytes aren't tokenized (engine.rs:20).
- Table column widths approximate Chromium's auto layout (render.rs:1576). The parser's autolinks and HTML sanitizer are approximations of remark-gfm and rehype-sanitize. Chunk sizes count bytes, not UTF-16.
- The selection color is hard-coded macOS blue and was never checked against the fork. Italics depend on t3-ui's baked italic fonts and haven't been checked in a scene.
- Not built: tooltips, context menus, the table's "Copy as CSV" menu, `$skill` chips, fade edges on scrolling tables, styled code scrollbars.

## Tests and tools
- `cargo test -p t3-highlight` (includes `tests/shiki_parity.rs`, which compares with Shiki per language and enforces minimum scores) and `cargo test -p t3-markdown --no-default-features` (parser, chunker, links, copy). Both run in `script/preland.sh`.
- Regenerate highlight assets with `node crates/t3-highlight/tools/export-assets.mjs <fork>` and `node crates/t3-highlight/tools/shiki-fixtures.mjs <fork>` (needs node + cargo), then run the parity test.
- Chromium reference (`crates/t3-markdown/tools/reference/`, written against the July `ChatMarkdown.tsx`):
  1. `T3_FORK=<fork> node render.mjs` renders `fixtures/sample.md` through the fork's real react-markdown, sanitize and Shiki into `dark.html` / `light.html` (output dir `$T3_MD_REF_OUT`, default `/tmp/markdown-reference`).
  2. Serve the output dir with `python3 server.py <port>`, open the pages at 1440x900, take 640x400 @2x tiles, and run `stitch.py` to make `reference-*-full@2x.png`.
  3. `metrics.js` (run in the page) dumps every element's box and computed style.
  4. `python3 compare.py <snapshot.png> <reference@2x.png> <out>` writes side-by-side bands and a diff.
- Snapshots: `cargo run -p t3-snapshots -- snapshots markdown-full-dark`, macOS only (CI uploads them).
- Bench: `gh workflow run markdown-bench.yml -R AadiJo/t3UI --ref markdown` runs `t3-snapshots --bin markdown-bench` (on the `markdown` branch).

## Perf (measured)
- Linux release, GPUI-free paths:
  - Parse: 24 KB of markdown in 0.33 ms. A streaming update (chunking plus re-parsing the tail) averages 119 µs.
  - Highlighting: TypeScript 0.39 ms/line, Rust 0.07 ms/line. Cache hits about 10 µs. Streaming highlighter 125-433 µs per update. Grammar load 130 ms, done in the background at startup.
- Frame timing: see the `markdown-bench` run on branch `markdown` (caching off vs on, top-pinned vs bottom-pinned).
