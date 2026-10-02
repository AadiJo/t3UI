# Diff surface

> Refreshed against fe7d3092c (2026-10-02). Part of the panels spec; index in docs/spec/panels.md.

The right panel's `diff` surface (`components/DiffPanel.tsx`, mounted with `mode="embedded"` and
`key={threadKey}` from `ChatView.tsx:9671-9679`). The body is `@pierre/diffs` 1.3.0-beta.10
(`CodeView`, shadow DOM, virtualized, Shiki in a worker pool) with the app's stylesheet overrides.
Paths and conventions as in `panels.md` §0.

## 0. Primitives used

| Use | Primitive | Resolved (desktop) |
| --- | --- | --- |
| Scope menu trigger | `Button size="xs" variant="secondary"` | h-6 (24px), px 7px, gap 4px, `text-xs`, `rounded-[8px]`, `bg-secondary`, hover `bg-secondary/90`, pressed `bg-secondary/80` |
| Base-ref trigger | `Button size="xs" variant="ghost-muted"` | 24px, `text-muted-foreground`, hover `bg-accent text-foreground` |
| Refresh, collapse-all | `Button size="icon-sm" variant="ghost"` | 28x28, icon 14px |
| Layout toggle | `ToggleGroup variant="segmented"` + `Toggle` (segmented size) | group `rounded-lg bg-input/40 p-0.5 gap-0.5`; items `h-6 rounded-md px-2.5 text-xs text-muted-foreground`, hover `bg-background/55` (dark `bg-input/32`) + `text-foreground`, pressed `bg-background text-foreground shadow-xs/10` (dark `bg-input/72`) (`components/ui/toggle-group.tsx:15-45`, `toggle.tsx:24-25,42-43`) |
| Wrap / whitespace / file tree toggles | `Toggle variant="ghost" size="sm"` | 28x28, `rounded-lg`, pressed `bg-accent text-accent-foreground` |
| File header chevron, copy path, file status | `Button size="icon-micro"` (`ghost` / `ghost-muted`) | 20x20, `rounded-sm` (6px), icon 12px (chevron overrides to 16px) |
| Comment buttons | `Button size="xs"` (`ghost-muted` Cancel, default Comment), `Button size="icon-xs" variant="ghost-muted"` (delete) | 24px |
| Comment input | `Textarea size="sm"` | design-system spec |
| Skeletons | `Skeleton` | `bg-muted-foreground/15`, `animate-skeleton` (2.4s, opacity 1 -> 0.55 in `steps(4)` holds; repaints while loading) (`components/ui/skeleton.tsx:7-16`, `index.css:271-289`) |
| Refresh glyph | `RefreshIcon size="sm"` (14px) | spins (`visible-animate-spin`, paused when offscreen) while refreshing (`components/ui/refresh-icon.tsx`) |

---

## 1. Selection model (`diffPanelStore.ts`)

```ts
DiffPanelSelection =
  | { kind: "unstaged" }                                   // "Working tree" (default)
  | { kind: "branch"; baseRef: string | null }            // "Branch changes"; null = automatic base
  | { kind: "turn"; turnId; filePath: string | null; revealRequestId: number }
```

- Per thread, persisted (localStorage `t3code:diff-panel-state:v1`, version 1) with a per-thread memory
  of the last branch base ref (`branchBaseRefByThreadKey`) (`:8-139`).
- `selectGitScope("branch")` restores the remembered base ref; `selectGitScope("unstaged")` saves
  the current one first. `selectBranchBaseRef(ref)` trims (empty -> null) and selects branch.
- `selectTurn(turnId, filePath?)` stores the trimmed path (or null) and bumps `revealRequestId` when
  the previous selection was a turn (else 1).
- `reconcileTurnSelection(turnIds)`: if the selected turn no longer exists, select the newest turn
  (`:88-106`); run whenever the turn list changes (`DiffPanel.tsx:209-215`).

Entry points:

| Trigger | Effect |
| --- | --- |
| Add-menu / launcher `Diff` (`ChatView.tsx:4639-4644`) | `selectGitScope("unstaged")`, `open("diff")` |
| `⌘D` (`diff.toggle`, outside a terminal) | `toggle("diff")`: closes the panel if Diff is the visible surface, else opens/activates it; selection unchanged |
| Timeline "view diff" on a turn / file (`onOpenTurnDiff(turnId, filePath?)`, `ChatView.tsx:9511-9520`) | `selectTurn(turnId, filePath)`, `open("diff")` |
| Proactive diff after a turn (off by default) | `openProactive(diff)` + `selectGitScope("unstaged")` (panels.md §2.5) |

---

## 2. Data

| Need | RPC (upstream `packages/contracts/src`) | T3UI |
| --- | --- | --- |
| Turn list | thread `checkpoints` (`TurnDiffSummary {turnId, checkpointTurnCount?, completedAt, status, files[{path, additions, deletions}]}`) via `useTurnDiffSummaries`; ordered by turn count desc, then `completedAt` desc (`DiffPanel.tsx:192-207`) | thread detail in `t3-client` |
| Turn diff | `checkpointTurnCount = n`: from 0 -> `orchestration.getFullThreadDiff {threadId, toTurnCount: n, ignoreWhitespace?}`; else `orchestration.getTurnDiff {threadId, fromTurnCount: n-1, toTurnCount: n, ignoreWhitespace?}` -> `ThreadTurnDiff {diff, ...}` (`state/queries.ts:319-358`, `orchestration.ts:2272-2289`) | typed (`GetTurnDiff`, `GetFullThreadDiff`) |
| Working tree / branch diff | `review.getDiffPreview {cwd, baseRef?, ignoreWhitespace?}` -> `{cwd, generatedAt, sources[{id, kind: "working-tree"\|"branch-range", title, baseRef, headRef, diff, diffHash, truncated, files?[{path, previousPath, additions, deletions}]}]}`; stale after 5s (`review.ts`, `crt:state/review.ts:22-29`) | typed (`ReviewGetDiffPreview`); `files` must be `Option<Vec>`: absent (old server) and empty differ (lazy mode keys on presence) |
| Retry at env root | when the error contains `configured workspace root` and the server `cwd` differs, retry the same request with `cwd = serverConfig.cwd` (`DiffPanel.tsx:283-302`) | data loading exists on branch `diff` |
| Per-file patches (truncated sources) | `review.getDiffPreview {cwd, baseRef?, ignoreWhitespace, file: {path, previousPath, sourceKind}}`, at most 4 in flight, stale 5 min; error `Diff no longer available. Refresh the comparison.` when the source is missing (`crt:state/review.ts:30-53`) | typed (input has `file`) |
| Expand collapsed context | `review.getDiffFileContents {cwd, sourceKind, changeType, baseRef, headRef, oldPath, newPath}` -> `{oldContents, newContents}`, single-flight per key (`crt:state/review.ts:54-71`, `lib/diffFileContents.ts`) | **missing** |
| Base refs | two `vcs.listRefs {cwd: preview.cwd, includeMatchingRemoteRefs: true, refKind: "local"\|"remote", query?, limit: 100}` (local list excludes the head ref) (`DiffPanel.tsx:343-380`) | typed (`VcsListRefs`, input has `ref_kind`, `include_matching_remote_refs`, `query`, `limit`) |
| Git repo? | `vcs.status`/`subscribeVcsStatus {cwd}` -> `isRepo` (defaults to true while unknown) | typed |
| Client settings | `diffLayout` ("stacked" default \| "split"), `wordWrap` (default true), `diffIgnoreWhitespace` (default true), `diffFilesCollapsed` (default **true**), `diffColorScheme` ("red-green" default \| "blue-orange"), `timestampFormat` (`contracts:settings.ts:303,374-376,513`) | `ClientSettings` lacks most of these |

Refresh: working-tree/branch previews refresh on window focus and when the thread reports a workspace
mutation (latest `tool.completed`/terminal `tool.updated` with `itemType` `command_execution` or
`file_change`, `hooks/useWorkspaceMutationRefresh.ts:1-40`), plus the refresh button. Turn diffs are
immutable.

`cwd` = the thread's worktree path, else the project workspace root; the repository root (for path
mapping) only when there is no worktree (`DiffPanel.tsx:167-170`).

---

## 3. Layout

```
DiffPanelShell (embedded): div.flex.h-full.min-w-0.flex-col.bg-background.w-full     (DiffPanelShell.tsx:22-40)
├─ header row [data-surface-subheader]: flex items-center justify-between gap-2 px-2
│   inline panel: h-7 (28px) + mb-3 (12px), no border; sheet: h-10 + border-b border-border/60
│   ├─ left: flex min-w-0 flex-1 items-center gap-3
│   │   ├─ scope menu trigger                                                  4.1
│   │   └─ (branch scope with a resolved base) compare strip                   4.2
│   └─ right: flex shrink-0 items-center gap-1
│       ├─ DiffStatLabel `+A -D` (inline layout, text-2xs, mr-1) when files exist
│       ├─ refresh (git scopes only)
│       ├─ collapse/expand all (when files exist)
│       ├─ layout segmented toggle (stacked | split)
│       ├─ wrap toggle
│       ├─ whitespace toggle
│       └─ file tree toggle (when files exist)
└─ body                                                                         5
```

### 3.1 Scope menu (`DiffPanel.tsx:673-722`)

- Trigger label (truncate) + `ChevronDown` 14px at 70% opacity; `aria-label="Diff scope: <label>"`.
  Label: `Working tree`, `Branch changes`, `Latest turn` (selected turn is the newest), or
  `Turn <n>` (`n` = checkpoint turn count, `?` when unknown) (`:232-239`).
- Dropdown (align start), radio group: `Working tree`, `Branch changes`, `Latest turn`; then a `Turn`
  submenu with one radio item per turn, newest first: `Turn <n>` and, right-aligned, the completion
  time (`formatShortTimestamp`, `text-xs tabular-nums text-muted-foreground`). Items close the menu on
  click. The submenu marks the selected turn even when it is the latest; the top group marks
  `Latest turn` instead.

### 3.2 Branch compare strip (`:723-837`)

Only for `Branch changes` when the preview resolved a base ref:
`flex min-w-0 max-w-full items-center gap-2 overflow-hidden text-xs text-muted-foreground`,
`aria-label="Comparing <head or HEAD> against <base>"`:
- Head name (`max-w-48` 192px, truncate) + `ArrowRight` 14px at 70%; tooltip `<head> → <base>`.
- Base-ref combobox trigger (`ghost-muted xs`, `max-w-48`): base name + `ChevronDown`;
  `aria-label="Change comparison target. Currently <base>"`.
- Popup (align start) `w-72` (288px), `max-w-[calc(100vw-1rem)]`:
  - search input placeholder `Search refs...` (filters both lists; server-side `query` too).
  - Column header row: `grid grid-cols-[1rem_minmax(0,1fr)] gap-2 border-b border-border/70 ps-3 pe-6.5
    pt-2 pb-1.5 text-3xs font-medium uppercase tracking-wide text-muted-foreground`, labels `Branch`
    and right-aligned `Remote` (2rem column).
  - Empty: `No matching refs.`
  - List `max-h-64` (256px): first `Automatic` (only while the query is empty), then one row per
    branch name (`buildBaseRefChoices` merges a local branch with its matching remote): label
    truncate; right cell: a small `Switch` (thumb 12px) `Use remote version of <name>` when both local
    and remote exist (on = compare against the remote), a `Check` 12px with tooltip `Remote only`
    when only the remote exists, else nothing.
  - Selecting sets the base ref (Automatic = null). Closing clears the query.

### 3.3 Right-side controls (`:839-973`)

| Control | Shown | aria-label / tooltip (top) | Action |
| --- | --- | --- | --- |
| Refresh (`RefreshIcon` spins while the preview or file patches load) | git scopes in a repo | `Refresh diff` / `Refreshing diff`; tooltip `Refresh diff` / `Refreshing diff…` | refetch the preview |
| Collapse all | at least one file | `Collapse all files` (`ChevronsDownUp`) / `Expand all files` (`ChevronsUpDown`) when all are collapsed | toggles every file; remounts the viewer |
| Layout | always | group `Diff layout`; items `Stacked diff view` (`Rows3`), `Split diff view` (`Columns2`); no tooltip | writes the `diffLayout` setting |
| Wrap (`TextWrap`) | always | `Disable diff line wrapping` / `Enable diff line wrapping`; tooltip `Disable line wrapping` / `Enable line wrapping` | local state, initialized from `wordWrap` |
| Whitespace (`Pilcrow`) | always | `Show whitespace changes` (pressed = hiding) / `Hide whitespace changes` | local state from `diffIgnoreWhitespace`; refetches with `ignoreWhitespace` |
| File tree (`FolderTree`) | at least one file | `Hide file tree` / `Show file tree` | localStorage `t3code.diffFileTreeOpen` (default false) |

Icons 14px.

---

## 4. Body states (`DiffPanel.tsx:977-1204`)

Centered single-line messages use `flex flex-1 items-center justify-center px-5 text-center text-xs
text-muted-foreground/70`:

| Condition | Content |
| --- | --- |
| no thread | `Select a thread to inspect turn diffs.` |
| not a git repo | `Turn diffs are unavailable because this project is not a git repository.` |
| turn selected but no turns | `No completed turns yet.` |
| loading (no patch yet) | skeleton (`DiffPanelLoadingState`, `DiffPanelShell.tsx:76-100`): a 32px file-header skeleton (16px chevron box with a 10px square, 20px square, pill bar 12px tall 50% width max 256px, two 20x12 pills right), a 24px separator row (two 1px `border/40` lines around a 96x10 pill), three code-line rows (20x10 pill + 60/66/80% pill, gap 12px, `px-3 py-2 space-y-2`), then two more header skeletons; `role=status` labels `Loading checkpoint diff...` / `Loading working tree diff...` / `Loading branch diff...` |
| empty patch | `No net changes in this selection.` (`px-3 py-2 text-xs text-muted-foreground/70`, centered in full height) |
| no patch | `No patch available for this selection.` |
| error without a patch | above the body: `text-2xs text-error/80 mb-2` in `px-3`, the error message |
| truncated, not lazy | banner `shrink-0 border-b border-border/70 bg-muted/40 px-3 py-1.5 text-2xs text-muted-foreground`: `This preview exceeds the size limit. Changes shown are incomplete.` + ` Totals include all changes.` when stats exist |
| unparseable | `p-2` scroll: reason line (`text-2xs text-muted-foreground/75`) `Unsupported diff format. Showing raw patch.` or `Failed to parse patch. Showing raw patch.`, then a `pre` `max-h-[72vh] rounded-md border border-border/70 bg-background/70 p-3 font-mono text-2xs leading-relaxed text-muted-foreground/90` with the raw patch (wrapping per the wrap toggle) |
| worker pool initializing | `Loading code...` centered `p-4 text-xs text-muted-foreground` (`DiffWorkerPoolProvider.tsx:97-129`) |
| files | section 5 |

---

## 5. File list (`AnnotatableCodeView` -> `StyledDiffCodeView` -> Pierre `CodeView`)

```
div.flex.min-h-0.flex-1.overflow-hidden
├─ div.min-h-0.min-w-0.flex-1   (click / context-menu capture, 5.4)
│   └─ CodeView  class "diff-render-surface" + [--code-background: var(--background)], h-full overflow-auto
└─ (file tree open) aside  w-[min(16rem,40%)] min-w-40 shrink-0 border-l border-border/60   5.6
```

### 5.1 Files and ordering

- Non-lazy: the patch is parsed with Pierre `parsePatchFiles` (cache key = FNV-1a hashes of the
  patch, `lib/diffRendering.ts:22-44,166-205`); git scopes compact partial hunk offsets.
  File paths are unquoted git paths.
- Lazy (source `truncated` and `files` present): file list = `files` sorted by path (numeric,
  case-insensitive); patches load 4 at a time starting with the first 4; a footer of up to 4 header
  skeletons (`DiffFileLoadingBoundary`, `role=status`, `aria-label="Loading diff…"`, each in a
  `border-b border-border/40` row) loads the next 4 when it comes within 600px of view
  (`components/diffs/useReviewFilePatches.ts`, `DiffFileLoadingBoundary.tsx`). In lazy mode Pierre's own
  `+N/-N` counts are hidden and replaced by the stat from `files` (`DiffStatLabel`), and the
  filename suffix shows `DiffFileStatus` when a file failed or was itself truncated.
- Collapse state: per thread + scope (`<env>:<thread>:<sectionId>`), initialized to all collapsed
  when `diffFilesCollapsed` is on (the default) (`DiffPanel.tsx:461-471`). A file whose patch is still
  pending renders collapsed and its chevron is disabled.
- Selecting a turn with a file path scrolls that file to the top (`scrollTo item, align start`) and,
  in lazy mode, expands and requests it (`:518-577`).

### 5.2 Pierre options passed (`DiffPanel.tsx:1157-1166`, `StyledDiffCodeView.tsx:300-322`)

| Option | Value |
| --- | --- |
| `diffStyle` | `"unified"` (stacked, default) or `"split"` |
| `lineDiffType` | `"none"` on the main thread; the worker pool's render options don't set it, so the worker uses Pierre's default `"word-alt"` and word emphasis is visible (handoff, `DiffWorkerPoolProvider.tsx:37-60`) |
| `overflow` | `"wrap"` (default) or `"scroll"` |
| `theme` / `themeType` | `pierre-dark` / `pierre-light` (`@pierre/theme` 1.1.0) by resolved app theme |
| `preferredHighlighter` | `"shiki-wasm"` |
| `stickyHeaders` | true |
| `loadDiffFiles` | contents loader for git scopes (enables expanding collapsed context) |
| `enableGutterUtility`, `enableLineSelection` | true unless a comment draft is open |
| `itemMetrics` | `diffHeaderHeight 32`, `hunkSeparatorHeight 24`, `spacing 0`, `paddingTop 0`, `paddingBottom 8` |
| `layout` | `{paddingTop: 0, paddingBottom: 0, gap: 0}` |
| defaults kept | `diffIndicators: "bars"`, `hunkSeparators: "line-info"`, `expansionLineCount: 100`, `collapsedContextThreshold: 1`, `maxLineDiffLength: 1000`, `tokenizeMaxLineLength: 1000` (`nm:@pierre/diffs@1.3.0-beta.10 dist/renderers/DiffHunksRenderer.js:277`) |
| worker pool | shared `WorkerPoolManager`, `totalASTLRUCacheSize 240`, `useTokenTransformer: true`, kept 30s after the last consumer unmounts |

The pnpm patch on 1.3.0-beta.10 (`patches/@pierre%2Fdiffs@1.3.0-beta.10.patch`, 8 files) only fixes
virtualizer height bookkeeping (re-measure wrapped rows when the code width changes, partial layout
cache resets in the editor). No visual change.

### 5.3 Colors and metrics

Font: `--diffs-font-family` = `--font-mono` (code font), header font `--font-sans`;
`--diffs-font-size` = the code font size setting (13px default, `appearanceFonts.ts:115-117`);
line height 20px; tab size 2 (Pierre defaults, `dist/style.js`).

Surface tokens (`lib/diffRendering.ts:322-385`, `StyledDiffCodeView.tsx:15-261`, `index.css:2109-2160`).
Inside the diff panel `--code-background` is overridden to `--background` (`StyledDiffCodeView.tsx:298`),
`--code-foreground` = `--foreground`. Let `bg = --background`, `fg = --foreground`,
`add = --diff-addition` (= `--success`; blue-600/400 in the blue-orange scheme), `del = --diff-deletion`
(= `--destructive`; orange-600/400), all mixes `color-mix(in srgb, ...)`:

| Pierre variable | Value |
| --- | --- |
| `--diffs-bg` (code, header, file info) | `bg` |
| context row bg | `bg 97% + fg` |
| hover row bg | `bg 94% + fg` |
| separator bg | `bg 95% + fg` |
| buffer (split empty side) | `bg 90% + fg` |
| addition row | light `bg 50% + add`, dark `bg 70% + add` |
| addition line number | light `bg 35% + add`, dark `bg 60% + add` |
| addition hover | `bg 85% + add` |
| addition word emphasis | `bg 80% + add` |
| deletion row / number / hover / emphasis | same formulas with `del` |
| addition/deletion/modified base (bars, header icons, counts) | `--diffs-addition-base` = Pierre `#0dbe4e` light / `#5ecc71` dark unless the blue-orange scheme overrides it to `add`; deletion `#ff2e3f` / `#ff6762`; modified `#009fff` / `#69b1ff` |
| token colors | Shiki `pierre-light` / `pierre-dark` theme tokens; token backgrounds transparent |
| selected line (`[data-selected-line]`) | light `lab mix: bg 88% + (bg 50% + modified)`, dark `bg 80% + (bg 70% + modified)`; gutter/number cells `bg 91% + (bg 35% + modified)` / `bg 85% + (bg 60% + modified)`; in `bars` mode a 4px `--diffs-modified-base` bar at the start of the number cell |

Dark/light hex for `bg`/`fg`/`add`/`del` come from `docs/spec/tokens.json`. Rows, gutters, the
"bars" change indicator, split buffers and the expand affordances follow Pierre's default stylesheet
(dump it with `node --input-type=module -e 'import s from "<fork>/apps/web/node_modules/@pierre/diffs/dist/style.js"; process.stdout.write(s)'`).

### 5.4 File header (Pierre `createFileHeaderElement` + app slots)

Sticky at the top of the scroll area (`z-index 4`), height 32px min, `padding: 6px 12px 6px 8px`,
`font: 12px/1 var(--font-sans)`, `bg = --code-background`, no bottom border (`StyledDiffCodeView.tsx:79-92`).
Hover: no fill, an inset 3px left edge `fg at 24%` (`:94-99`). Contents left to right, gap 8px:

1. Prefix slot: collapse chevron (`icon-micro ghost`, `-ms-0.5`, `ChevronRight` collapsed /
   `ChevronDown` expanded, 16px, colored by change type: new `--diffs-addition-base`, deleted
   `--diffs-deletion-base`, change/rename `--diffs-modified-base`, else `muted-foreground/80`);
   `aria-label="Expand <path>"` / `Collapse <path>`; tooltip `Expand diff` / `Collapse diff`;
   disabled while the file's patch is pending (`DiffPanel.tsx:1117-1156`, `lib/diffRendering.ts:301-316`).
2. Change icon (Pierre sprite: `symbol-modified`, `symbol-added`, `symbol-deleted`, `symbol-moved`),
   colored modified/addition/deletion base.
3. For renames: previous name at 70% opacity, then a right arrow icon.
4. Title: the path, start-ellipsis (`direction: rtl`), `line-height 16px`, `cursor: pointer`, hover
   color `fg 84% + --primary` over 120ms.
5. Filename suffix slot: copy-path button (`icon-micro ghost-muted`, `Copy` 12px, `Check` 12px
   `text-success` for a moment after copying; `aria-label="Copy file path"`; tooltip `Copy path` /
   `Copied`; an anchored "copied" toast near the button) and, lazy mode only, the status button
   (`RotateCw` `Retry loading diff` / `Info` `Partial diff preview`; tooltip `Retry loading diff` or
   `This file is too large to show in full. Counts include all changes.`).
6. Metadata (right, gap 1ch, tabular): `-D` (only when deletions > 0 or nothing was added) in
   `--diffs-deletion-base`, `+A` in `--diffs-addition-base`, mono 11px. Lazy mode: replaced by
   `DiffStatLabel` (`+A -D`, mono, `text-diff-addition` / `text-diff-deletion`, counts abbreviated
   `1.2k`, `12k`, `1.2m`).

Interactions (capture handlers on the wrapper, `DiffPanel.tsx:1029-1078`): a click on the title opens
the file in a right-panel file tab (`openFile(workspace-relative path)`, `diffFileActions.ts:79-98`;
paths outside the workspace do nothing); a click elsewhere on the header toggles collapse; buttons
keep their own actions. Right-click on the title opens the shared file context menu (panels-files.md):
`Open`, `Reveal in Finder`, `Open with` submenu, depending on the server's editors.

Expanding a file fades its body in (`opacity 0 -> 1, 200ms ease-out`, `@starting-style`; none with
reduced motion). Heights never animate.

### 5.5 Hunk separators (`line-info`)

24px rows, `bg = --code-background`, wrapper padding `0 12px 0 8px`, content `gap 8px`,
`11px var(--font-sans)`, color `fg 52% + bg`. The "N unmodified lines" text sits between two 1px rules
(`bg 92% + fg`) that fill the remaining width. When the hunk can expand (contents loader present) the
whole row is the button (pointer cursor; Pierre's expand buttons are visually hidden but focusable);
hover/focus raises the text to `fg 76%` and the rules to `bg 84% + fg`. Expanding reveals up to 100
lines (`StyledDiffCodeView.tsx:101-196`).

### 5.6 Changed-files tree (`components/diffs/DiffFileTree.tsx`)

- Aside `w-[min(16rem,40%)]` (max 256px) `min-w-40` (160px) `border-l border-border/60`.
- Header row (subheader convention, `px-2 text-xs text-muted-foreground gap-1`): `Files`
  (`px-1 font-medium text-foreground`), the file count right-aligned (`tabular-nums`), and when there
  are folders a `Button icon-xs ghost` `Collapse all folders` (`ChevronsDownUp`) / `Expand all folders`
  (`ChevronsUpDown`) with the same tooltip.
- Tree: `@pierre/trees` 1.0.0-beta.4, `density: "compact"`, empty directories flattened, every folder
  starts open, no search, the diff's own order (folders take their first file's position), git status
  per file (`added`, `deleted`, `renamed`, `modified`; a type change shows as modified), Pierre file
  icons (`pierre-icons.ts`) and the app's tree theme (`pierre-tree-theme.ts`; row metrics and colors in
  panels-files.md).
- Selecting a file reveals it in the diff (expands it if collapsed, requests it in lazy mode, scrolls
  it to the top). Clicking the already-selected row reveals it again. The selected path follows a
  timeline-driven file selection (folders on its path expand, the row scrolls into view).

---

## 6. Review comments (`AnnotatableCodeView.tsx`, `DiffCommentAnnotation.tsx`, `reviewCommentContext.ts`)

- Start: Pierre's gutter utility (a "+" in the number gutter on hover) or a line-range selection in
  the gutter (drag), enabled while no draft is open. Clicking the utility opens a draft under the
  last selected line (`annotation side = endSide ?? side`, deletions vs additions).
- Draft (`DiffCommentAnnotation kind="draft"`, `px-3 py-2 font-sans`): `Textarea size="sm"`, focused
  on the next frame with the caret at the end, placeholder `Add a comment…`,
  `aria-label="Comment on lines <rangeLabel>"`. Footer `mt-1.5 flex items-center gap-1`: hint
  `⌘/Ctrl Enter to send` (`mr-auto text-3xs text-muted-foreground/70`), `Cancel` (`ghost-muted xs`),
  `Comment` (default xs, disabled while empty). Escape cancels; Cmd/Ctrl+Enter submits when non-empty
  (`commentSubmitShortcut.ts`). Pointer events inside don't reach the diff.
- Saved comment (`kind="comment"`): `group/comment flex items-start gap-2.5 border-s-2 border-primary/55
  bg-primary/[0.045] px-3 py-2.5`: `MessageCircle` 14px `text-primary/70` (`mt-0.5`), text
  `text-sm leading-5 whitespace-pre-wrap`, and on hover/focus a delete button (`icon-xs ghost-muted`,
  `Trash2` 12px, `aria-label="Delete comment"`). Several comments on one line stack with
  `divide-y divide-border/30 border-y border-border/30`; a draft annotation group gets `py-1`.
- Range label (`formatDiffReviewRangeLabel`, `reviewCommentContext.ts:382-400`): line numbers from the
  new side (old side for deleted lines); prefixed with the change marker (`+`/`-`) when every line
  shares it; `+12`, `-4 to -9`, `30 to 34`, `line`, `N lines`.
- Saving (`buildDiffReviewComment`, `:402-456`) adds a composer context record to the thread's draft
  (`composerDraftStore.addReviewComment`): `{id, sectionId ("turn:<id>" \| "unstaged" \| "branch"),
  sectionTitle ("Turn n" \| "Working tree" \| "Branch changes"), filePath, startIndex, endIndex,
  rangeLabel, text (trimmed), diff ("@@ -a,b +c,d @@" + selected lines with markers), fenceLanguage:
  "diff", selection {start, side, end, endSide}}`. Comments re-render on their lines while the same
  section is shown (range restored from `selection`), and deleting one removes it from the draft. The
  composer spec owns the resulting chip; on send it serializes as
  `<review_comment sectionId="…" sectionTitle="…" filePath="…" rangeLabel="…" startIndex="n"
  endIndex="m">\n<text>\n\n```diff\n<diff>\n```\n</review_comment>` (fence lengthened past any backtick
  run, `shared:composerContextLegacySend.ts:141-161`).

---

## Reuse map

| T3UI module | Verdict |
| --- | --- |
| `crates/t3-diff` `patch.rs`, `rows.rs`, `word_diff.rs`, `color.rs` (main) | Fit (parser, row model, jsdiff `word-alt` emphasis, srgb/lab `color-mix`). Add: whitespace-insensitive diffs come from the server (`ignoreWhitespace`), nothing to do client-side; partial-hunk offset compaction for git scopes (`lib/diffRendering.ts` `compactPartialHunkOffsets`). |
| `t3-diff` `tree.rs` | Logic for the timeline card. The panel tree differs: diff order (folders at their first file's position), all folders open, flattened empty folders, git-status colors. |
| `t3-diff` `palette.rs`, `view/*` (`DiffView`, row painting) | July-era colors: re-derive from 5.3 (much stronger row tints; `--code-background` = `--background`; per-scheme overrides; selected-line formulas). Mechanics (per-row virtualization, sticky header overlay, wrap re-measure, hatched split buffers, per-side horizontal scroll) stay, per the panels handoff. Add: gutter utility + range selection, annotation rows, collapse per file with all-collapsed default, line-info separators with expand, lazy header-only placeholders. |
| `crates/t3-highlight` | Fits for Shiki `pierre-dark/light` token colors; regenerate assets from fe7d3092c's `@pierre/theme` 1.1.0 (handoff). |
| `crates/t3-app/src/panels/diff_panel.rs` (branch `diff`) | Data loading reusable (turn vs full-thread diff, preview + workspace-root retry, listRefs local+remote). Missing: per-file lazy patches, contents loader, focus/mutation refresh, whitespace refetch, all chrome. Chrome is July-era: replace with section 3. |
| `crates/t3-diff/src/review.rs` (branch `diff`) | Redo against `reviewCommentContext.ts`: `selection` field, range label rules above, and the `<review_comment …>` serialization moved to `shared:composerContextLegacySend.ts` (attribute order: sectionId, sectionTitle, filePath, rangeLabel, startIndex, endIndex). |
| Reference harness `/tmp/diff-ref-new` | Probe works against the new fork; `capture.mjs` has stale selectors (panels handoff). |

## Reference screenshots needed

Seed: `thread-aurora-tour` (completed turn editing `src/format.ts` and `test/format.test.ts`; dirty
`README.md` in the working tree). Dark and light. Default settings (stacked, wrap on, whitespace
hidden, files collapsed).

1. `diff-working-tree-collapsed`: open Diff from the "+" menu (Working tree): one collapsed file header
   (`README.md`), header controls.
2. `diff-working-tree-expanded`: click the README header: added lines, line-info separator.
3. `diff-latest-turn`: scope menu -> Latest turn, expand both files (word emphasis visible on the
   changed `formatBytes` lines).
4. `diff-split`: as 3 with the split layout.
5. `diff-nowrap`: as 3 with wrap off (horizontal scroll).
6. `diff-scope-menu-open`: scope menu open with the Turn submenu.
7. `diff-branch-compare`: Branch changes (needs a branch with commits ahead of `main`; seed a worktree
   thread or commit on a side branch), combobox open with `Branch / Remote` header.
8. `diff-file-tree`: as 3 with the file tree open and `src/format.ts` selected.
9. `diff-comment-draft`: hover a line gutter (utility "+"), click it, type text (draft with
   `⌘/Ctrl Enter to send`).
10. `diff-comment-saved`: submit the draft (saved comment row; composer shows the chip).
11. `diff-header-hover`: pointer over a file header (left edge cue, title hover color).
12. `diff-loading`: skeleton (throttle the server or capture on first open of a large diff).
13. `diff-not-git`: a project that is not a git repository (add a plain folder project).
14. `diff-truncated-lazy`: a working tree change above the server's preview limit (large generated
    file) so `truncated` + lazy per-file loading shows.
15. `diff-blue-orange`: as 3 with Settings -> diff color scheme blue-orange.

## Missing client APIs

- `review.getDiffFileContents {cwd, sourceKind, changeType, baseRef, headRef, oldPath, newPath}` ->
  `{oldContents, newContents}` (expanding collapsed context in git scopes; upstream
  `contracts:review.ts` `ReviewDiffFileContentsInput/Result`, tag `review.getDiffFileContents`).
- `ReviewDiffPreviewSource.files` should be `Option<Vec<ReviewDiffFileStat>>` (absent vs empty).
- Client settings `diffLayout`, `wordWrap`, `diffIgnoreWhitespace`, `diffFilesCollapsed`,
  `diffColorScheme`, code font size in T3UI `ClientSettings`.

## Open questions / risks

- Pierre computes many colors with `color-mix(in lab, …)` and `light-dark()`; the app's overrides use
  srgb. Resolve each formula with `t3-diff::color` in the right space; the Chromium harness is the
  check.
- Word-level emphasis comes from the worker's default `word-alt` despite `lineDiffType: "none"`; if a
  future fork build sets the worker option, emphasis disappears. Match today's behavior.
- The expand-context loader needs `review.getDiffFileContents`; without it, separators must render
  without the expand affordance (Pierre behavior without `loadDiffFiles`).
- Gutter utility/selection visuals live in Pierre's stylesheet (not app CSS); capture references with
  the pointer in the gutter.
