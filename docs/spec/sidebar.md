# Default sidebar: build spec

Refreshed against fe7d3092c (`~/L-Projects/t3UI-refs/t3code-fork`). Companion to `shell.md` (window, layout,
keybindings, menus, toasts, dialogs, persistence). Settings, Pull Requests, Usage pages, the command
palette and the chat view are other specs.

Path prefixes: `web/` = `apps/web/src/`, `cr/` = `packages/client-runtime/src/`, `contracts/` =
`packages/contracts/src/`, `shared/` = `packages/shared/src/`, `desk/` = `apps/desktop/src/`.

Conventions:

- Desktop only: window min width is 840 (`desk/window/DesktopWindow.ts:399`), so `sm:` (>=640) and
  `md:` (>=768) always apply, `max-sm:`/`max-md:` never do, and the mobile `Sheet` sidebar
  (`web/components/ui/sidebar.tsx:233-270`, `useIsMobile` < 768) never renders.
- Radii: `--radius` = 10px, so `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14,
  `rounded` 4, `rounded-full` (`web/index.css:265-270`). `--control-radius` = 8 (`web/index.css:92`).
- Type: `text-xs` 12/16, `text-sm` 14/20, `text-2xs` 11px (line-height 1/0.6875 = 16px),
  `text-3xs` 10px/14px, `text-5xs` 7px/7px (`web/index.css:168-175`). medium 500, semibold 600.
- Sidebar geometry vars (`web/index.css:92-100`): `--sidebar-content-inset` 8px,
  `--sidebar-row-content-inset` 10px, `--sidebar-control-gap` 8px, `--sidebar-icon-color` =
  `color-mix(srgb, --contrast-sidebar-muted-foreground 60%, --sidebar)`.
- Colors are token names. Inside the sidebar (`[data-app-sidebar]`) the palette is re-scoped
  (`web/index.css:1160-1212`), see 1.3. `X/NN` = token at NN% alpha. Light/dark Tailwind tones
  given as `light / dark`.
- Icons are lucide names. `size-3` 12px, `size-3.5` 14px, `size-4` 16px.
- Strings are verbatim, including `…` and `−` (U+2212).

---

## 0. Which sidebar renders

`AppSidebarLayout` (`web/components/AppSidebarLayout.tsx:321-330`) mounts exactly one child in the
`Sidebar` container:

| Condition | Child |
| --- | --- |
| pathname `/settings` or `/settings/*` | `SidebarChromeHeader` + `SettingsSidebarNav` (settings spec) |
| else `useLegacySidebarEnabled()` | `LegacySidebar.tsx` default export (`LegacyThreadSidebar`) |
| else | `Sidebar.tsx` default export (`ThreadSidebar`), this spec |

- `useLegacySidebarEnabled()` = `clientSettingsHydrated && settings.legacySidebarEnabled`
  (`web/hooks/useSettings.ts:376-380`). Before client settings hydrate it is always false, so the
  default sidebar renders first and only legacy opt-ins swap after hydration.
- Setting: `legacySidebarEnabled: boolean`, default `false` (`contracts/settings.ts:467-471`). The
  key is new: old `sidebarV2Enabled`/`sidebarV2ConfiguredByUser` are dropped on decode, so every
  user lands on the default sidebar. UI: Settings > General > Legacy features, row description
  "Restore per-project thread trees instead of the default flat sidebar.", switch aria-label
  "Sidebar (legacy)" (`web/components/settings/SettingsPanels.tsx:2156-2168`).
- The toggle also changes `chat.new` (shell.md 4.3): default sidebar + more than one project group
  opens the palette's "New thread in..." picker; legacy creates immediately
  (`web/routes/_chat.tsx:121-137`).
- Native port: the current t3UI sidebar (`crates/t3-app/src/sidebar/`) is the legacy tree and
  becomes the `legacySidebarEnabled` branch. This spec is the new default.

Both sidebars share `SidebarChromeHeader` (2) and `SidebarChromeFooter` (9).

---

## 1. Container and box tree

### 1.1 Sidebar column

`Sidebar side="left" collapsible="offcanvas" data-app-sidebar role="navigation"
aria-label="Threads"` ("Settings" on settings routes) with `resizable` options
(`web/components/AppSidebarLayout.tsx:305-320`, `web/components/ui/sidebar.tsx:272-321`):

```
sidebar root     data-state=expanded|collapsed, data-collapsible="offcanvas" only when collapsed
├ gap            relative, w=--sidebar-width (0 when collapsed)      reserves flex space
└ container      fixed top 0 bottom 0 left 0, h=100vh, w=--sidebar-width, z 10, border-right 1px
  │              collapsed: left = -width. border color = sidebar `--border` (zinc-200 / #3e3e3e)
  └ inner        flex col, h-full, bg `--sidebar` + surface-grain (data-sidebar="sidebar")
     ├ SidebarChromeHeader          2
     ├ fixed header group           3   (SidebarContent fixedHeader)
     ├ scroll area (flex-1)         4-8 (SidebarContent)
     ├ SidebarChromeFooter          9
     └ SidebarRail                  1.4
```

- Width: `--sidebar-width` from `chat_thread_sidebar_width` (shell.md 2.3), default 256, min 208,
  max `viewport - 640`.
- Collapse/expand transition only when `panelAnimationDurationMs > 0` (default 0, so instant):
  `transition-[width]` on gap and `transition-[left,right,width]` on container, duration
  `--panel-animation-duration`, `ease-out` (`web/components/ui/sidebar.tsx:286,298`; shell.md 2.4).
- surface-grain: 256px tile of fractal noise at 0.035 opacity painted over the background
  (`web/index.css:1664-1675,1013-1017`). Optional in native (see Open questions).

### 1.2 Inner layout (`web/components/Sidebar.tsx:4611-5197`)

```
SidebarChromeHeader                         h 52, shrink-0 (2)
fixedHeader wrapper  w-full shrink-0
└ SidebarGroup z-[1]  p 8                   (web/components/ui/sidebar.tsx:621-633)
  └ SidebarThreadHeader  row h 32          (3)
ScrollArea  flex-1 min-h-0, scrollbars hidden, edge fade 12px   (web/components/ui/sidebar.tsx:593-616)
└ content div  flex col min-w-0 min-h-full [overflow-anchor:none]
  └ SidebarGroup flex-1 role=presentation   padding 8, top 0 (first group after a fixed header)
    ├ search mode: results <ul> or status <p>   (4.6)
    └ list mode: <ul role=presentation> relative flex col gap 1px, flex-1 when it has items
         draft block, then rows and section markers (4.1)
       + "Show N more" row (6.5)
    └ empty state (8)
SidebarChromeFooter  px 8 py 4 col gap 8    (9)
```

- Scroll edge fade: `scrollFade` with `--fade-size: 0.75rem`: the viewport is masked so the top
  edge fades over `min(12px, distance scrolled from top)` and the bottom over `min(12px, distance
  to bottom)`; no fade at rest at the top (`web/components/ui/sidebar.tsx:593-601`,
  `web/components/ui/scroll-area.tsx:62-63`). Scrollbars hidden (`hideScrollbars`).
- Shelf headers carry `mt-auto` (6), so with a short list the Working/Snoozed/Settled block sits at
  the bottom of the scroll area, right above the footer.

### 1.3 Sidebar-scoped palette (`web/index.css:1160-1212`)

| Token inside `[data-app-sidebar]` | Light | Dark |
| --- | --- | --- |
| `--background` | zinc-25 (oklch 99.2% 0 0) | #1c1c1c |
| `--foreground` | zinc-800 | #fafaf9 |
| `--card` | white | #2d2d2c |
| `--popover` | (root) | #343434 |
| `--accent` | zinc-100 | #3e3e3e |
| `--muted` | zinc-50 | #343434 |
| `--muted-foreground` | zinc-500 | #9e9e9e |
| `--border` | zinc-200 | #3e3e3e |
| `--input` | zinc-300 | #3e3e3e |
| `--sidebar` | zinc-50 | #222222 |
| `--sidebar-foreground` | zinc-800 | #e1e1e0 |
| `--sidebar-muted-foreground` | zinc-500 | `--muted-foreground` (#9e9e9e) |
| `--sidebar-control-surface` | zinc-100 | `--muted` (#343434) |
| `--sidebar-row-hover` | zinc-25 | `--contrast-foreground` 8% |
| `--sidebar-row-active` | white | `--contrast-foreground` 11% |
| `--sidebar-row-selected` | white | `--contrast-foreground` 7% |
| `--sidebar-border` | zinc-200 | `--border` (#3e3e3e) |
| `--sidebar-stage-fade` | `--sidebar` | `--card` (#2d2d2c) |

`--secondary-label` = `--muted-foreground` (`web/index.css:1052,1117`). Built-in palette themes
(`html[data-theme-id]`) override these (`web/index.css:1480-1499`); design-system spec owns themes.
The `--contrast-*` variants are recomputed inside the sidebar from these values
(`web/index.css:1517+`); at the default appearance contrast (100) they equal the plain tokens.

### 1.4 Rail (resize handle) (`web/components/ui/sidebar.tsx:350-529`)

- `button` 16px wide, full height, absolute, centered on the container's right edge
  (`-right-4` then `-translate-x-1/2`, i.e. x from `width-8` to `width+8`), z 20, `tabIndex=-1`.
  Collapsed: moves to `-right-2` with no translate and `pointer-events: none`.
- Its `::after` 2px line at the center gets `bg-sidebar-border` on hover. Because the rail
  straddles the 1px container border, the hover line overlaps the border. On hover while
  collapsed the rail also gets `bg-sidebar` (only reachable when pointer events are on; they are
  off when collapsed).
- Cursor `col-resize` when expanded (resizable). Tooltip (side right, default 600ms delay):
  "Drag to resize sidebar"; aria-label "Resize Sidebar". (When not resizable: "Toggle Sidebar".)
- Drag: pointer down, move; width = clamp(pointerX-based width, min 208, max `viewport - 640`);
  a width is accepted only if it shrinks or leaves the main column >= 640px
  (`web/components/AppSidebarLayout.tsx:311-319`). Transitions are disabled during the drag. On
  release the width persists to localStorage `chat_thread_sidebar_width` and a click that ended a
  drag is swallowed.
- Click without drag while expanded: nothing. Double-click: reset width (remove the key, go back to
  256 clamped to max) (`web/components/AppSidebarLayout.tsx:235-242,331`).
- Window resize re-clamps the max live (`useSyncExternalStore` on `resize`,
  `web/components/AppSidebarLayout.tsx:229-234`).

---

## 2. Header row: `SidebarChromeHeader` (`web/components/sidebar/SidebarChrome.tsx:33-75`)

- `div` h 52 (`--workspace-topbar-height`), flex row, items-center, gap 8, padding-x 0 (desktop),
  `relative`, `@container/sidebar-header`, Electron `drag-region` (the whole row drags the window;
  buttons inside are no-drag). Nothing else is visible at rest: the sidebar toggle button is
  drawn by the shell at a fixed position over this row (shell.md 2.5), and the in-row
  `SidebarTrigger` is `md:hidden`.
- Stage backdrop (2.1) when the environment stage is Nightly or Dev and identification mode is
  `artwork`.
- Pill badge when identification mode is `pill` and stage is Dev/Nightly: `Badge size="sm"
  variant="secondary"` with text "Dev" or "Nightly", `ml-1` (4px), z 10, shown only when the
  header container is >= 15rem (240px) wide (`hidden @[15rem]/sidebar-header:inline-flex`).
  Badge styles are design-system.

Identification mode (`useEnvironmentIdentificationMode`, `web/hooks/useSettings.ts:333-366`):
setting `environmentIdentificationMode` `"artwork" | "pill" | "none"`, default `"artwork"`
(`contracts/settings.ts:157-159,377-379`). Resolves to `"none"` until client settings hydrate,
and `"artwork"` falls back to `"pill"` while a palette theme that does not allow artwork is active.

Stage label (`web/components/SidebarStageBackdrop.tsx:35-43`, `web/branding.logic.ts:1-22`):
`"Nightly"` when the primary server's `serverVersion` matches
`/^[^-+]+-(?:nightly|preview)\.\d{8}\.\d+$/`, else the app's own stage label (desktop branding;
for the native app use its own channel). The user runs `npx t3@nightly`, so the daily desktop
shows the Nightly sky art.

### 2.1 Stage backdrop (`web/components/SidebarStageBackdrop.tsx:49-391`, `web/index.css:467-490`)

- `div` absolute, top 0, inset-x 0, h 80 (`h-20`), z 0, `pointer-events-none`, overflow hidden.
  It extends 28px below the 52px header row, behind the search row (the fixed header group is
  `z-[1]` so it paints over the art).
- Art: an SVG with `viewBox="0 0 8192 96"`, `preserveAspectRatio="xMinYMin slice"` scaled to the
  80px height (scale 80/96); patterns repeat every 288 units (stars), 640 (glows), so widening the
  sidebar reveals more canvas instead of zooming. Nightly: sky gradient, glow, star pattern with 17
  stars and 3 sparkles, two blurred clouds (`:97-195`). Dev: blueprint paper, grids, ruler,
  annotations (`:197-391`). Port the SVG verbatim as an asset; colors are the `--stage-night-*` and
  `--stage-art-*` oklch vars at `web/index.css:617-650`.
- Mask: `linear-gradient(to bottom, black 0%, black 55%, transparent 92%)`.
- `::after` fade overlay to `--stage-fade` (= `--sidebar-stage-fade`): stops transparent 0-28%,
  10% at 40%, 30% at 52%, 58% at 64%, 82% at 75%, 96% at 85%, 100% at 93%.
- Static: no animation.
- With the art showing, the shell's sidebar toggle uses the `media-navigation` look (white icon,
  hover `white/10`) while the sidebar is visible (shell.md 2.5).

---

## 3. Thread header row (`web/components/sidebar/SidebarThreadHeader.tsx`, wired at `web/components/Sidebar.tsx:4618-4773`)

Row: flex, items-center, gap 4. Inside the fixed-header `SidebarGroup` (padding 8), so the row
spans `width - 16`.

### 3.1 Search field (`SidebarThreadHeader.tsx:144-184`)

- Container `div` (also the popup anchor for the project scope combobox): h 32, flex-1, min-w 0,
  flex items-center gap 8, rounded-md (8), padding 8 x 6, text-sm medium,
  color `--sidebar-muted-foreground`; hover bg `--sidebar-row-hover`, hover text
  `--sidebar-foreground`.
- `search` icon 16px, color `--sidebar-icon-color`.
- Input: unstyled native `type=search` (WebKit cancel/decoration hidden), height auto, padding 0,
  font medium 14px, `leading-normal` (1.5), text `--sidebar-foreground`, placeholder "Search" in
  `--sidebar-muted-foreground`. aria-label "Search threads", `role=combobox`,
  `aria-controls="sidebar-thread-search-results"` while results show.
- While the query is non-empty (trimmed): clear button at the end, `Button size="icon-micro"
  variant="ghost-muted"` (20x20, rounded-sm 6, icon `x` 12px, text `--muted-foreground`,
  hover bg `--accent` + text `--foreground`), aria-label "Clear thread search". Click clears and
  refocuses the input.
- Keys in the input (`web/components/Sidebar.tsx:3101-3138`), ignored during IME composition:
  Escape (only while searching) clears; ArrowDown/ArrowUp move the highlighted result with wrap;
  Enter opens the highlighted result (clears search, navigates).

### 3.2 Trailing icon group (`SidebarThreadHeader.tsx:188-217`)

`div` flex items-center shrink-0, no gap, no fill. Children, in order:

1. Project scope combobox trigger (only when projects exist) (3.3).
2. "Add project" (only when projects exist): `folder-plus`. Opens the command palette in
   `add-project` mode (`openCommandPalette({ open: "add-project" })`, command palette spec).
3. "New thread": `square-pen`, disabled when there are zero projects (`projects.length === 0`).

Each is `SidebarHeaderIconButton` (`SidebarThreadHeader.tsx:227-266`) = `SidebarMenuButton
size="icon"` resized to 28x28: rounded `--control-radius` 8, padding 0, centered; svg 16px colored
`--sidebar-icon-color`; hover bg `--sidebar-row-hover`, hover svg `--sidebar-foreground`; active
(pressed) bg `--sidebar-row-active`; disabled opacity .64 no pointer; focus-visible ring 2px
`--ring` (`web/components/ui/sidebar.tsx:657-678`). Tooltip side top, default delay 600ms.
Tooltip text = the label, except New thread:

- Single project group: `New thread (⌘N)` (label falls back to `chat.newLocal`'s shortcut if
  `chat.new` has none), or `New thread` with no shortcut.
- More than one project group: two lines, `New thread (⌘N)` then, in `--muted-foreground`,
  `New thread in current project: Shift+click (⇧⌘N)` (shortcut part omitted when unbound)
  (`web/components/Sidebar.tsx:4599-4610`, `SidebarThreadHeader.tsx:138-211`).

New thread click (`web/components/Sidebar.tsx:4578-4597`): Shift+click, or <= 1 project group,
starts a draft in the context project (route thread's project, else route draft's project, else the
first ordered project; `web/lib/chatThreadActions.ts:74-102`, `useHandleNewThread` chat spec).
Otherwise opens the command palette in `new-thread-in` mode.

### 3.3 Project scope (combobox) (`web/components/Sidebar.tsx:4623-4753`)

Filters the whole list to one logical project group. Scope persists in UI state
`sidebarProjectScopeKey` (shell.md 7). Trigger icon: `folder` 16px when unscoped; the scoped
project's favicon (5.6) when scoped. aria-label / tooltip: "Filter threads by project" or
`Filter threads by project: <displayName>`.

Popup (`web/components/ui/combobox.tsx:137-241`):

- Anchored to the search field `div` (not the 28px trigger), side bottom, align start, offset 4,
  z 130. Width >= anchor width, max `min(18rem, available width)`; rows truncate past that.
- Surface: `dropdown-glass` (shell.md 6.1) rounded-lg 10, shadow
  `0 16px 40px -18px rgb(0 0 0/55%)` (dark `0 18px 44px -18px rgb(0 0 0/80%)`), max height
  `min(available height, 23rem=368px)`.
- Search input block: padding 12 x 10 top (`px-3 pt-2.5`), underline row: border-bottom 1px
  `--border/70` (focus-within `--ring`), pb 6, `search` icon 16px at left 0 top 6 in
  `--muted-foreground/55`, input h 26, padding-left 20, font sans; placeholder "Search projects...",
  aria-label "Search projects".
- List: padding 4, scroll fade.
- Items (`ComboboxItem hideIndicator`): min-h 28, rounded-sm 6, padding 8 x 4, text-sm, flex gap 8.
  Hover/highlight bg `--sidebar-row-hover`; selected bg `--sidebar-row-selected` (highlighted +
  selected stays hover). Content: favicon 16 (or `folder` icon for "All projects"), label
  (flex-1 truncate, text-sm), environment badge when projects span > 1 environment
  (`web/components/ProjectEnvironmentBadge.tsx`), and for project rows a settings button
  (`Button size="icon-xs" variant="ghost-muted"`, 24x24, `settings` icon 14px, `ml-auto`,
  title `Project settings for <name>`, not focusable).
- Items: first `{value:"all", label:"All projects"}`, then each project group by sidebar sort
  (`web/components/Sidebar.tsx:2471-2480`). While the query is non-empty "All projects" is
  removed and the rest are filtered by Base UI's contains filter on the label
  (`web/components/Sidebar.logic.ts:959-967`). `autoHighlight`: first match highlighted.
- Empty: "No matching projects." (p 8, centered, text-sm, `--muted-foreground`).
- Choosing an item sets scope (`all` -> null). Right-click on a project item, ContextMenu key, or
  Shift+F10 in the search input opens that project's settings (`/projects/$projectKey`, which
  redirects into settings) and closes the popup. Opening resets the query.
- A persisted scope whose project no longer exists resets to all, but only once every catalog
  environment has a live project snapshot (`web/components/Sidebar.tsx:2537-2545`).
- Scope change clears multi-selection and resets settled paging to 10.

---

## 4. List model (pure logic, port to `t3-logic`)

Inputs: all thread shells across environments (`OrchestrationThreadShell`), projects, server
configs (capabilities per environment), client settings, UI state, route, clock.

### 4.1 Item order

`orderedItems` (`web/components/Sidebar.tsx:3529-3577,5010-5147`):

```
draft block (5.4) + its divider          only drafts with content, newest first
marker pinned-header                      zero height at rest
pinned rows                               (card)
marker pinned-divider                     zero height at rest
marker active-placeholder                 zero height at rest
active (inbox) rows                       (card)
[working-header + visible working rows]   only if sidebarWorkingShelfEnabled and >=1 working thread (card)
[snoozed-header + visible snoozed rows]   only if >=1 snoozed thread (slim)
settled-header                            always (when any thread is listed)
marker settled-placeholder                zero height at rest
rendered settled rows                     (slim)
"Show N more" row                         (6.5)
```

When there are no listed threads at all (pinned+active+working+snoozed+settled = 0) no markers
render, only the draft block and the empty state.

### 4.2 Classification (`web/components/Sidebar.tsx:2636-2756`)

Visible threads: `archivedAt === null` and (no scope, or `${environmentId}:${projectId}` is in the
scoped group's member refs). For each, with `caps = serverConfig(environment).environment.capabilities`:

1. `snoozed` if `caps.threadSnooze` and `effectiveSnoozed(thread, now)` (4.4). Snooze outranks
   settlement and pinning.
2. else `settled` if `caps.threadSettlement` and `settledOverride === "settled"`.
3. else `pinned` if `pinnedAt != null`.
4. else `working` if `sidebarWorkingShelfEnabled` and `isSidebarThreadWorking(thread)` (4.5);
5. else `active`.

`now` is a precise clock (ISO). The list re-classifies every minute (`useNowMinute`) and exactly
at the next snooze wake: a timer is armed for `min(max(0, firstSnoozedUntil - now) + 50ms,
2^31-1 ms)` (`web/components/Sidebar.tsx:2809-2825`).

Capabilities also gate drag: a row is draggable only when its environment has `threadPinning`
and `threadPinReorder` (and it is not renaming, not working, and no drop is pending); active
placement additionally needs `threadActiveReorder` (`:2669-2675,4996-5004`).

### 4.3 Sort rules (`cr/state/threadSort.ts`, `web/components/Sidebar.logic.ts`)

All "id" tiebreaks are `String.localeCompare`. Malformed timestamps parse as 0 unless noted.

- **Pinned** (`sortPinnedThreadsByOrderKey`, `cr/state/threadSort.ts:303-336`): threads with
  `pinOrderKey` first, ascending by plain string comparison (`<`), tiebreak id then environmentId;
  then keyless threads by `createdAt` descending, same tiebreak.
- **Active**, Working beta off (`sortActiveThreadsByOrderKey`, `:340-373`): keyless
  (`activeOrderKey == null`) threads first, newest anchor first where anchor =
  `max(createdAt, unsettledAt)`; then keyed threads ascending by key; tiebreak id then
  environmentId. Activity does not reorder rows.
- **Active**, Working beta on (`sortInboxThreadsByReturn`, `web/components/Sidebar.logic.ts:997-1021`):
  descending by `max(createdAt, unsettledAt, latestTurn.requestedAt, latestTurn.completedAt,
  observedReturnAt)`, tiebreak id then environmentId. `observedReturnAt` is a client-side,
  in-memory map: when a thread that was in the working set on the previous classification is no
  longer working, stamp `now` for it (first call only takes a baseline; deleted threads are
  dropped; reset when the beta is off) (`web/components/Sidebar.tsx:270-300`).
- **Working shelf**: `sortInboxThreadsByReturn` without observed returns (`:2736-2737`).
- **Snoozed**: ascending by `snoozedUntil` (first valid timestamp, invalid = 0) (`:2738-2743`).
- **Settled** (`sortSettledThreads`, `cr/state/threadSort.ts:53-66`): descending by
  `resolveSettledThreadTimestamp` = `settledAt` if parseable, else the latest of
  `latestUserMessageAt`, `latestTurn.requestedAt|startedAt|completedAt`, else `updatedAt`
  (`:27-48`); null sorts as 0; tiebreak id.
- While a drop is pending, the pinned or active list is ordered by the drop's planned order
  (`orderItemsByPreferredIds`, `web/components/Sidebar.logic.ts:729-766`).
- Fractional order keys: base-26 `a-z`, `pinOrderKeyBetween`, `generateSpreadPinOrderKeys`,
  `planPinnedReorder` (single write when neighbors are keyed, else rewrite the whole visible
  section with spread keys, skipping keys held by hidden rows) (`cr/state/threadSort.ts:175-295`).
  New pins without an explicit key get `topOfPinnedRunOrderKey()` (key before the smallest key
  across all pinned shells, hidden snoozed pins included) when the server supports
  `threadPinReorder` (`web/hooks/useThreadActions.ts:626-643`). Port these verbatim; tests exist
  at `cr/state/threadSort.test.ts`.

### 4.4 Snooze rules (`cr/state/threadSettled.ts`)

- `hasQueuedTurnStart` (`:23-45`): `latestUserMessageAt` set, session status not `error`, the
  message is within +-2 min of now (`QUEUED_TURN_START_GRACE_MS`), and either `latestTurn` is null
  or every turn timestamp is strictly before the message.
- `canSnooze` (`:102-112`): false when `hasPendingApprovals`, `hasPendingUserInput`, or a queued
  turn start. A running session can be snoozed.
- `threadRaisedHandWhileSnoozed` (`:71-92`): pending approval or user input; or session `error`
  newer than `snoozedAt` (or no `snoozedAt`); or `latestTurn` completed after `snoozedAt`.
- `effectiveSnoozed` (`:121-131`): `snoozedUntil` parses, is in the future, and no raised hand.
  Malformed data never hides a thread.
- `threadWokeAt` (`:144-168`): null if never snoozed or still snoozed; raised hand: the
  completion time (if the trigger was a completion) else `session.updatedAt ?? snoozedAt`; timer
  wake: `snoozedUntil` once past.
- Presets (`:212-260`, client relabel `web/components/Sidebar.snooze.ts:18-34`):
  "In 1 hour" (+1h), "In 3 hours" (+3h), "This evening" (today 18:00, only if > 1h away),
  "Tomorrow" (tomorrow 09:00), "Next week" (next Monday 09:00, omitted when equal to Tomorrow).
  `whenLabel` = time formatted with the `timestampFormat` setting (`hour: numeric, minute:
  2-digit`, hour12 per setting, locale default) (`web/timestampFormat.ts:3-21,147-151`); Next week
  prefixes the short weekday (`Mon 9:00 AM`). Calendar math uses local days (DST-safe).
- `snoozeWakeLabel` (`:267-276`): `"now"` if past/invalid; < 1h `${ceil(min)}m` (min 1); < 1 day
  `${ceil(h)}h`; else `${ceil(d)}d`.
- Custom snooze (`resolveCustomSnooze`, `:287-303`): duration (amount > 0, unit minutes/hours/days)
  or local date `YYYY-MM-DD` + `HH:MM` (rejects rolled-over dates / DST-nonexistent times); must
  be in the future.

### 4.5 Status model (`web/components/Sidebar.logic.ts:811-883,1045-1136`)

`resolveSidebarThreadStatus` (row status), first match wins:

1. `hasPendingApprovals` -> `approval`
2. `hasPendingUserInput` -> `input`
3. `session.status` `running` or `starting` -> `working`
4. `session.status` `error` -> `failed`
5. `backgroundLiveness === "working"` -> `working`
6. `backgroundLiveness === "monitoring"` -> `monitoring`
7. else `ready`

`hasUnseenCompletion` (`:674-684`): true if an unread desktop completion notification exists for
the thread (macOS notification inbox, shell.md 9); else needs `latestTurn.completedAt` (valid) and
a `lastVisitedAt` (from UI state `threadLastVisitedAtById`); a missing `lastVisitedAt` returns
**false** (never-visited threads are not unread); an unparseable one returns true; else
`completedAt > lastVisitedAt`.

`isWoke` (row, `web/components/Sidebar.tsx:1183-1188`): `wokeAt` (4.4) parses, and
(`lastVisitedAt` missing/unparseable or `< wokeAt`), and `settledOverride !== "settled"`.

`isSidebarThreadWorking` (Working beta): status is `working` or `monitoring` and
`resolveThreadStatusPill(thread).label !== "Plan Ready"`.

`resolveThreadStatusPill` (used for the Working beta and the legacy sidebar; also exported for
other surfaces), first match wins:

| Condition | Label | Text color | Dot | Pulse |
| --- | --- | --- | --- | --- |
| pending approval | Pending Approval | amber-600 / amber-300/90 | amber-500 / amber-300/90 | no |
| pending user input | Awaiting Input | indigo-600 / indigo-300/90 | indigo-500 / indigo-300/90 | no |
| session running | Working | sky-600 / sky-300/80 | sky-500 / sky-300/80 | yes |
| session starting | Connecting | sky-600 / sky-300/80 | same | yes |
| no pending input, `interactionMode === "plan"`, latest turn settled, `hasActionableProposedPlan` | Plan Ready | violet-600 / violet-300/90 | violet-500 / violet-300/90 | no |
| `backgroundLiveness === "working"` | Working | sky | sky | yes |
| `backgroundLiveness === "monitoring"` and no unseen completion | Monitoring | sky | sky | no |
| unseen completion | Completed | emerald-600 / emerald-300/90 | emerald-500 / emerald-300/90 | no |

Rollup priority (`:565-573`): Pending Approval 6, Awaiting Input 5, Working 4, Connecting 4, Plan
Ready 3, Monitoring 2, Completed 1. There is no Error pill anymore.

`shouldRecedeSidebarThread` (`:826-842`): false when active (route), selected, or status `input`;
true for `working`; `monitoring` recedes unless unread; `ready`/`approval` recede unless unread or
woke; `failed` never recedes.

### 4.6 Search (`web/components/Sidebar.tsx:2757-2807`, `web/components/Sidebar.logic.ts:926-957`)

- Searching = trimmed query non-empty. The list is replaced by results; the draft block, shelves,
  drag and paging are not shown.
- Corpus: pinned + active + working + snoozed + settled (all, ignoring shelf collapse and paging),
  in that order, already scope-filtered.
- Title/PR match: case-insensitive substring of the title or any PR search term
  (`threadPullRequestSearchTerms`, `shared/threadPullRequests.ts`). Content match: threads whose
  messages the server matched. Result order: all title matches (corpus order), then content-only
  matches (corpus order).
- Server content search: RPC `orchestration.searchThreads` per connected environment
  (`t3_protocol::methods::SearchThreads`), query >= 2 chars after trim, debounced 200ms
  (`web/state/queries.ts:36,79-102`). `isPending` while debouncing or loading.
- Highlighted index resets to 0 when the result set changes; the highlighted row scrolls into view
  (`block: nearest`).

---

## 5. Rows

All thread rows share one surface (`web/components/Sidebar.tsx:1471-1497`):

| State | Surface |
| --- | --- |
| route thread (`isActive`) | bg `--sidebar-row-active`, text `--sidebar-foreground` |
| multi-selected | bg `--sidebar-row-selected`, text `--sidebar-foreground` |
| unsent draft on a non-route thread | bg `--warning/4`, hover `--warning/8` |
| receding | no bg, text `--sidebar-muted-foreground/75`; hover bg `--sidebar-row-hover` + text `--sidebar-foreground` |
| default | transparent, text `--sidebar-foreground`; hover bg `--sidebar-row-hover` |
| receding and status working/monitoring | whole row opacity .70, hover/focus-within 1.0, `transition-opacity` (150ms default) |
| external file drag over | 1px inset ring `--primary/70`, plus hover bg unless active/selected |
| being dragged | opaque: bg `--sidebar` with a `--sidebar-row-active` overlay, text `--sidebar-foreground`, opacity 1, `shadow-lg`, z 20 |
| settled rows (`variantAction=unsettle`) | when neither hovered nor focus-within, all descendants text `--secondary-label/70` |

Common: rounded-md 8, overflow hidden, `cursor-pointer`, `select-none`, no outline; keyboard
focus-visible: 2px inset ring `--ring`. Row root is `role=button tabIndex=0`. aria-label =
`[title, statusLabel, projectDisplayName].filter(Boolean).join(", ")`, `aria-current="page"` when
active (`web/components/Sidebar.logic.ts:46-58`). Regenerating title: `aria-busy`, title opacity .55,
sr-only "Regenerating title".

### 5.1 Card row (pinned, active, working) (`web/components/Sidebar.tsx:1819-2076`)

```
li  py 2 (total 82px)                                 content-visibility auto (intrinsic 78)
└ row surface (role=button)
  └ content  h 78, px 10, py 8, relative z 10
    ├ line 1  h 20, flex items-center gap 6
    │   [draft pen] favicon 16 | project name (flex-1 truncate, text-xs, --secondary-label,
    │   medium; normal when receding) | [pin] | status/action slot (ml-auto, h 20, min-w 32, text-xs)
    ├ line 2  mt 4: title (text-sm, truncate)
    └ line 3  mt 2, flex items-center gap 6, text-xs --secondary-label
        [worktree icon] branch (flex-1, --muted-foreground/40, middle-truncated) | [terminal] |
        [PR badge] | [diff +a −d] | trailing (ml-auto, gap 4): [remote machine icon] [provider icon]
  [jump hint badge]
```

- Title color (card): receding `--secondary-label` normal weight; else medium weight and unread /
  woke / `input` -> `--foreground`; `failed` -> `--foreground/95`; else `--foreground/90`.
- Project name absent -> an empty flex-1 spacer.
- Branch row absent -> empty flex-1 spacer. Worktree icon (`folder-git-2` 12px,
  `--muted-foreground/40`) only when `worktreePath` is set; tooltip `Worktree: <path>` or
  `Worktree: <path> (<branch>)` with `~`-shortened display path
  (`web/components/ThreadStatusIndicators.tsx:411-443`). Branch text middle-truncates
  (`web/components/ui/middle-truncate.tsx`).
- Diff stats: `latestTurnDiff` always returns null today (`:2079-2086`); the slot never renders.
  Keep it out of the port.
- Remote machine icon: only when the thread's environment is not the primary environment;
  `EnvironmentMachineIcon` 14px in `--sidebar-muted-foreground/70`
  (`web/components/EnvironmentMachineIcon.tsx`, kind from `resolveEnvironmentMachineKind`).
- Provider icon: `ProviderInstanceIcon` 14px glyph at opacity .60, with an instance badge
  (h 12, min-w 12, px 2, 7px text, offset -3,-3) when several instances of the driver exist
  (`web/components/chat/ProviderInstanceIcon.tsx`, chat spec). Model instance =
  `session.providerInstanceId ?? modelSelection.instanceId`.

#### Status/action slot (line 1 right)

At rest, when not hovered/keyboard-focused: the status label (below) or, with no status, the
compact time label `threadTimeLabel` (5.5) in `--secondary-label`, tabular nums.

Status label (`web/components/Sidebar.tsx:1202-1251,1913-1941`): `inline-flex gap 4 medium`, icon
16px + label (+ for Working a ticking duration):

| Status | Label | Icon | Color (light / dark) |
| --- | --- | --- | --- |
| working | Working + ` 12s`/`4m`/`1h 5m` | `circle-dashed` | sky-600 / sky-400 |
| monitoring and not unread | Monitoring | `eye` | `--foreground` / white |
| approval | Approval | `shield-question` | `--warning-foreground` |
| input | Input | `message-circle-question` | indigo-600 / indigo-300 |
| failed | Failed | `circle-alert` | red-700 / red-300 |
| woke (and none above) | Woke (button, 5.7) | `alarm-clock` | `--warning-foreground` |
| unread (and none above) | Done | `circle-check` | emerald-700 / emerald-300 |
| ready, read | none: time label | | |

- A monitoring thread with an unread completion shows Done.
- Working duration: `formatWorkingDurationLabel(now - startedAt)`: `<60s` `${s}s`, `<60m` `${m}m`,
  else `${h}h ${m}m`; startedAt = running turn `startedAt ?? requestedAt ?? session.updatedAt`, or
  `session.updatedAt` when no running turn (`web/components/Sidebar.logic.ts:1027-1043`). Ticks
  every 1s, only that span re-renders. No shimmer.
- Hover / keyboard focus inside the slot: the status label cross-fades out (opacity 0, taken out of
  flow) and the action group fades in (`transition-opacity`). Woke stays visible and clickable.
  While the snooze menu is open the actions stay shown and the label hidden.

Action group (`:1947-2003`), shown only if any action applies: absolute right, z 10, flex
items-stretch gap 2, pl 4; on hover/focus-visible becomes static and opaque:

1. Discard draft (only if the thread has unsent composer text, 5.3): `x` 14px, px 6, text-xs
   `--muted-foreground` -> hover `--foreground`; tooltip (top) "Discard draft"; aria-label "Discard
   draft". Clears that thread's composer content and releases uploads.
2. Snooze (only if `caps.threadSnooze` and `canSnooze`): `clock` 12px, px 6, rounded-md, aria-label
   "Snooze thread", tooltip "Snooze thread", opens the snooze menu (6.2).
3. Settle (only if `caps.threadSettlement`): `check` 14px + "Settle", gap 4, px 6, `-mr-1`,
   text-xs `--muted-foreground` -> hover `--foreground`; aria-label/tooltip "Settle thread".

#### Pin marker

When `pinnedAt != null` (in any section): a `pin` 12px in `--muted-foreground/65`. When the
environment supports pinning (and not dragging) it is a button: hover `--foreground`, focus ring,
aria-label and tooltip "Unpin thread", click runs unpin with confirmation (7). Otherwise a static
icon aria-label "Pinned". During a drag only the pinned thread still over the pinned section
keeps it.

### 5.2 Slim row (snoozed, settled) (`web/components/Sidebar.tsx:1664-1817`)

```
li  (36px, content-visibility auto, intrinsic 36)
└ row surface  h 36, flex items-center gap 10, px 10
   favicon 16 (dimmed) | [draft pen] | title (flex-1 truncate, text-sm) | [pin] | [terminal] |
   [PR badge] | trailing slot (ml-auto, h 24, min-w 32, justify-end)
  [jump hint badge]
```

- Favicon wrapper: opacity .40 + grayscale unless (active and not settled); restores on row
  hover/focus-within (`transition-opacity`).
- Title: receding `--secondary-label/70` normal; else medium and active/woke/input ->
  `--foreground`, unread -> `--muted-foreground`, else `--secondary-label/70`; any hover/focus
  inside the row -> `--foreground`.
- Trailing slot at rest (tabular, `--secondary-label`, fades out on hover unless woke):
  - snoozed shelf: wake countdown (`snoozeWakeLabel`), text-xs `--info-foreground`;
  - woke: button `alarm-clock` 12px + "Woke", text-xs medium `--warning-foreground`, hover
    underline, aria-label/tooltip "Dismiss Woke notification";
  - settled shelf: `settledTimeLabel` (5.5); otherwise `threadTimeLabel`.
- Trailing action (absolute right, fades in on hover/focus-visible; static next to Woke):
  - snoozed shelf (needs `threadSnooze`): `alarm-clock-off` 12px, px 6, `-mr-1`, aria-label
    "Wake thread now" (no tooltip). Unsnoozes.
  - settled shelf (needs `threadSettlement`): `undo-2` 14px, px 6, `-mr-1`, aria-label/tooltip
    "Un-settle thread".
  - otherwise (needs `threadSettlement`): `check` 12px, px 8, aria-label "Settle thread" (no
    tooltip). Unreachable in the default sidebar: slim rows are only snoozed or settled.

### 5.3 Unsent-draft decoration

`useThreadHasUnsentDraft(threadRef) && !isActive` (`web/composerDraftStore.ts`, chat spec owns the
store): row tint 5 (warning/4), and a `square-pen` 12px `--warning-foreground` marker (role img,
aria-label/tooltip "Unsent draft") before the favicon (card) or after it (slim).

### 5.4 Draft block (`web/components/Sidebar.tsx:747-991`)

Rows for unsent new-thread drafts with content, above everything:

- Source: every draft session not promoted (`promotedTo == null`) whose composer has user content
  (`composerDraftHasUserContent`), scope-filtered by `${environmentId}:${projectId}`; sorted by
  session `createdAt` descending.
- The route's own draft: shows a frozen snapshot taken when the route became that draft (so it
  never repaints while typing); a draft that was never navigated away from has no row.
- Row: `li` py 2, surface like a card (h 78 content, px 10 py 8, rounded-md); bg
  `--sidebar-row-active` when route, else warning tint. Line 1 (h 20, gap 6): `square-pen` 12
  `--warning-foreground`, favicon 16, project name (flex-1 truncate, text-xs medium
  `--secondary-label`), discard button (fades in on hover: `x` 12px, px 4, rounded-md, aria-label
  and tooltip "Discard draft"). Line 2 (mt 2): preview text-sm medium `--foreground/90` truncate =
  first line of the prompt (context references replaced by their labels) or `N attachment(s)`.
  aria-label `<preview>, Unsent draft, <project>`.
- Click/Enter/Space navigates to `/draft/$draftId` (clears selection). Discard clears the draft
  session (route redirects home if it was open).
- Divider after the last draft: `li` h 1, mx 10, my 6, bg `--sidebar-border/60`.

### 5.5 Time labels (`web/components/Sidebar.tsx:302-318`, `web/timestampFormat.ts:228-247`)

`compact(formatRelativeTimeLabel(t))`: diff < 60s or negative -> `now`; < 60m `${m}m`; < 24h
`${h}h`; else `${d}d` (the ` ago` suffix is dropped). Active/pinned/working rows use
`latestUserMessageAt ?? updatedAt`; settled rows use `resolveSettledThreadTimestamp`. Updates
with the minute clock.

### 5.6 Project favicon (`web/components/ProjectFavicon.tsx`, `web/projectIdentity.ts`)

16px everywhere in the sidebar (`size-4`). Resolution: `projectIcon` override (monogram, emoji,
lucide name with color) else the server favicon URL (`projectFaviconUrlAtom`, HTTP asset) else a
generated monogram tile. Monogram: first two glyphs rule (`web/projectIdentity.ts:14-26`), color
from a 31-hash over 18 tones (`web/projectIconColors.ts`), drawn as a 16x16 rounded-[25%] tile
with the tone at 14% behind, mono bold 8.25 text (`web/components/ProjectMonogram.tsx`). Images:
rounded 25%, object-contain. The existing `t3-app/src/state/favicons.rs` covers fetching.

### 5.7 Tooltip card (hover details) (`web/components/Sidebar.tsx:362-481`)

On every thread row and search result: `TooltipPopup side="right" align="start" sideOffset=4
variant="glass"` (dropdown-glass, `shadow-xl shadow-black/25`, rounded-md 8). In the thread list
the provider sets delay 150ms, close delay 0, group timeout 400ms (moving between rows within
400ms opens instantly) (`:4779-4784,4845-4850`). Disabled while dragging or while the snooze menu
is open. Content (`max-w 320`, padding 4 x 8 inside the popup's 8 x 4 viewport, gap 8):

- title: text-xs medium `--foreground`, leading-tight, truncate.
- grid gap 6, pl 2, text-xs `--muted-foreground`; each line flex gap 8 with a 12px icon and
  `--foreground/75` text: project (favicon 12 + display name), environment (machine icon + label),
  branch (`git-branch` + middle-truncated), branch mismatch warning in `--warning`
  (`circle-alert` + "You're currently checked out on another branch.", only when the thread is
  in local mode and the live git branch differs), provider (provider icon 12 grayscale .60 +
  `<model label>` or `<model label> · <instance name>`), terminals (`terminal` 12 in teal +
  `N terminal process(es) running`), error ("Error occurred" with `circle-alert` in
  `--destructive-foreground` when `session.lastError`).
- PR mini list below a `border-t --border/60` (pt 8) when the server supports multiple PRs and the
  thread has links (`ThreadPullRequestsMiniList`, pull-requests spec).

### 5.8 Indicators shared with other surfaces

- Terminal status: when the thread has running terminals (`useThreadRunningTerminalIds`,
  terminal spec): `terminal` icon 14px in teal-600 / teal-300/90, `animate-status-pulse`
  (2s infinite, opacity 1 -> .5 -> 1 in `steps(6)`, `web/index.css:207,290-302`; continuously
  repainting, see Open questions), aria-label `N terminal process(es) running`.
- PR badge (`web/components/ThreadStatusIndicators.tsx:150-289`): `InlineButton` (font medium,
  hover underline) wrapping a state glyph 12px + number (`text-xs tabular-nums`, normal weight) in
  the state tone: open emerald-600/300, draft zinc-500/400, closed red-600/300, merged
  violet-600/300 (`web/components/pullRequest/pullRequestIcons.tsx:13-54`). Stack: `layers` glyph +
  layer count; several unrelated links: `+N`. Single PR is a link (opens in the right panel when a
  thread route is open, and activates that thread; else external); stack/multi opens the thread's
  pull requests tab. Tooltip = presentation label (e.g. `PR #12 - Open: <title>`). Data:
  `thread.pullRequests` / `linkedPullRequest` / `branchPullRequest` plus a live
  `vcs.status` lookup while the row is near the viewport (5.9).

### 5.9 Per-row live data lease (`web/components/Sidebar.logic.ts:60-117`)

A row subscribes to live git status (`vcsEnvironment.status`, RPC `vcs.status {cwd}`) and live
linked-PR status only while it is the route row or within 160px of the scroll viewport
(IntersectionObserver). It keeps the last value per `(environmentId, cwd)` after the lease ends so
badges never blank. Used for the PR badge state and the branch-mismatch line. Existing
`t3-app/src/state/vcs.rs` covers the stream; add the viewport lease.

### 5.10 Search result row (`web/components/Sidebar.tsx:2088-2241`)

`ul#sidebar-thread-search-results role=listbox aria-label="Thread search results"`, flex col gap 1.
Each `li role=presentation` > `button role=option id=sidebar-thread-search-result-<i>`
`tabIndex=-1`: min-h 36, flex items-center gap 10, rounded-md, px 10 py 4, text-sm. Highlighted or
route row: bg `--sidebar-row-active` text `--sidebar-foreground`; else `--sidebar-muted-foreground/75`
with hover bg `--sidebar-row-hover`. Content: favicon 16; column: [title (flex-1 truncate) + time
label (text-xs `--muted-foreground/55`)] and, for content matches, an excerpt line: text-xs
`--muted-foreground/85` truncate, prefix "You:" (`--info-foreground`) or "Agent:"
(`--success-foreground`), then the snippet with every case-insensitive (ASCII-folded) occurrence of
the query in semibold `--foreground` (`web/components/ThreadSearchMatch.tsx`). Mouse move highlights,
click opens. Same tooltip card as 5.7. File drop works on results too.

No results: `p role=status` px 8, py 24, centered, text-xs `--sidebar-muted-foreground`:
"Searching thread messages…" while the server search is pending, else "No threads found".

---

## 6. Shelves, markers, drag

### 6.1 Shelf headers (`web/components/Sidebar.tsx:686-745,5068-5125`)

`li` h 32, mx 2. Button fills it: flex items-center gap 8, px 8, text-xs medium, cursor pointer:

```
label | 1px line (flex-1, min-w 8) | chevron-right 12px (rotate 90 when expanded, transition-transform)
```

| Shelf | Label collapsed | Label expanded | Text | Line |
| --- | --- | --- | --- | --- |
| Working (beta) | `Working (N)` | `Working` | `--sidebar-muted-foreground/60` | `--sidebar-border/60` |
| Snoozed | `Snoozed (N)` | `Snoozed` | `--info-foreground` | `--info/20` |
| Settled | `Settled (N)` | `Settled` | `--sidebar-muted-foreground/60` | `--sidebar-border/60` |

- N counts the whole shelf (not paged, scope-filtered).
- `mt-auto` on the first shelf present (Working, else Snoozed, else Settled), pinning the block to
  the bottom.
- During a drag the Settled header reads `--sidebar-foreground/80` with line
  `--sidebar-foreground/25`, and `--primary` / `--primary/50` while it is the drop target.
- Expanded state persists in localStorage: `t3code:sidebar:working-expanded`,
  `t3code:sidebar:snoozed-expanded`, `t3code:sidebar:settled-expanded` (JSON booleans, default
  false) (`:264-268`).
- Collapsed shelves render no rows, except the route thread's own row (so the open thread never
  vanishes). Collapsed rows do not take part in jump shortcuts, traversal, or multi-select.
- Settled exception: while a settle-then-navigate is in flight for the route thread, its row is not
  pulled out (`threadNavigationStore.settlingThreadKeys`, `web/threadNavigationStore.ts`).

### 6.2 Snooze menu (row hover button) (`web/components/Sidebar.tsx:488-549`)

Base UI `Menu` popup (`web/components/ui/menu.tsx:20-72`): side bottom, align end, offset 4, z 130,
`dropdown-glass` rounded-lg, shadow `0 16px 40px -18px rgb(0 0 0/55%)` (dark 80%), min-w 160,
padding 4. Items: min-h 28, rounded-sm, px 8 py 4, text-sm, gap 8; highlight bg `--accent`.
Presets resolve when the menu opens: label + `MenuShortcut` (right-aligned `kbd`: h 16, rounded-sm,
px 6, bg `--muted`, 10px medium sans, `--secondary-label`, dark ring white/5) showing `whenLabel`.
Separator (mx 8, my 4, 1px `--border`), then "Custom…" which opens the custom snooze dialog (6.3).
Clicks never reach the row.

### 6.3 Custom snooze dialog (`web/components/CustomSnoozeDialog.tsx`)

Singleton host (`CustomSnoozeDialogHost`, mounted at root). `requestCustomSnooze()` resolves a
previous request with null, then opens; resolves `{snoozedUntil}` or null.

- Dialog `max-w sm` (384): title "Custom snooze", description "Choose when snoozed threads return
  to your inbox."
- Segmented toggle (aria-label "Schedule type"), full width: "Date and time" | "Duration".
- Date mode (2 columns, gap 12): "Date" outline button (full width, justify-between) showing
  `Mon D, YYYY` + `calendar` icon 16 `--muted-foreground`, opening a calendar popover (aria-label
  "Choose snooze date", days before today disabled, week start from locale); "Time" native time
  input (h 32). Initial: now + 1h.
- Duration mode: "Snooze for" number field (min 0, step any, decrement/increment aria-labels
  "Decrease duration"/"Increase duration"), "Unit" select: Minutes / Hours / Days. Initial 2 hours.
- Errors (`p role=alert` `--destructive`): "Choose a valid date and time in the future." /
  "Enter a positive duration.".
- Footer: "Cancel" (outline) and "Snooze" (submit). Dialog visuals: design-system dialog.

### 6.4 Drag and drop (`web/components/Sidebar.tsx:3315-3911`, `web/components/Sidebar.logic.ts:119-410`, `web/components/Sidebar.drag.ts`, `web/components/Sidebar.pointer.ts`)

Native port must keep the outcomes; the dnd-kit internals are reference only.

- Pick-up: primary button, the whole row is the handle, activation after > 6px movement
  (`Math.hypot`). Selection text is cleared; the click after a drag is swallowed. Escape, window
  blur, pagehide, resize, visibility hidden, or losing the button cancel. Vertical axis only,
  clamped to the scroll container and never above the "Pinned" label line. Auto-scroll at edges.
- While dragging: the zero-height markers open 24px label slots: `Pinned` above the pinned run and
  `Active` above the active run (`text-xs medium`, `--sidebar-foreground/80`, with a 1px line
  `--sidebar-foreground/25` filling the rest; `--primary` and `--primary/50` on the target)
  (`:647-684`). Empty sections show a 36px dashed drop hint: rounded-md, 1px dashed
  `--sidebar-foreground/25`, text-xs `--sidebar-foreground/80` "Active" or "Settled"; target:
  border `--primary/40`, bg `--primary/5`, text `--primary` (`:619-645`).
- The lifted row shows a destination badge instead of its status slot when the target section
  differs: h 20, rounded-sm, 1px `--primary/40`, bg `--primary/10`, px 6, 11px medium `--primary`,
  icon 12 + verb: Pin (`pin`), Unpin (`pin-off`), Settle (`circle-check`), Un-settle (`undo-2`),
  Wake (`alarm-clock-off`) (`:996-1027,1517-1525`).
- Section of a slot is read from markers above it. Working and Snoozed are never drop targets.
  Verb rules (`resolveSidebarDropVerb`): same section -> none; to pinned "pin"; to settled
  "settle"; to active from pinned "unpin", from settled "unsettle", else "wake".
- Settled shows the lifted row at its time-ordered slot, not under the pointer; the time-ordered
  inbox (Working beta) likewise.
- Drop plan (`planSidebarThreadDrop`, `web/components/Sidebar.logic.ts:261-371`):
  - environment without `threadSettlement` and the target or source is settled -> no-op;
  - to settled from elsewhere -> settle (then forward navigation as 7.2);
  - to pinned -> reorder-pinned (key writes) or pin (`thread.pin` with orderKey, then extra key
    writes); a drop at the original place is a no-op; any key write to a non-reorderable thread
    makes it a no-op;
  - to active -> move-active: unpin and/or unsettle and/or unsnooze (no confirmation for unpin on
    drag) plus active key writes; with the Working beta the active list has no placement (order
    null, no writes).
  - Commands run in sequence; the first failure stops and toasts ("Failed to unpin thread",
    "Failed to un-settle thread", "Failed to wake thread", "Failed to pin thread", "Failed to
    reorder pinned threads", "Failed to reorder active threads", "Failed to settle thread").
- Optimistic hold: after a drop the row stays at its destination (projected via
  `applySidebarThreadDrop`) until the server state shows every planned key, or a concurrent change
  / membership change / failure releases it; no new pick-up while held
  (`web/components/Sidebar.tsx:2610-2626,3389-3457`).

### 6.5 Settled paging (`web/components/Sidebar.tsx:2827-2859,5148-5159`)

Expanded settled shelf shows the first 10, "Show more" adds 25. The route thread is appended if
it is beyond the page. Resets to 10 on scope change. Row: `li` > button h 36, flex gap 10, px 10,
rounded-md, text-sm `--sidebar-muted-foreground/55`, hover bg `--sidebar-row-hover` + text
`--sidebar-foreground`; `plus` 16px + `Show {min(hidden, 25)} more`.

### 6.6 List motion (`web/components/Sidebar.motion.ts`)

When rows change order/section (not during a drag): each moved row animates `translateY(oldTop -
newTop) -> 0`; entering rows fade `opacity 0 -> 1` while translating from their neighbor's
displacement (or -min(height, 40)px); removed rows leave as a non-interactive copy fading to 0 while
translating by the displacement (or +min(height, 40)px). Duration 150ms `ease-out`. Skipped under
reduced motion or when more than 40 rows would fade. After a drop, every row glides from its
released visual position. Card <-> slim swaps re-key the row (fade).

---

## 7. Interactions and commands

### 7.1 Click, keyboard, selection (`web/components/Sidebar.tsx:3016-3031,3176-3198`, `web/threadSelectionStore.ts`)

- Click (not on a nested link): navigate to `/$environmentId/$threadId` (clears any selection,
  sets the selection anchor to this row). The second click of a double-click (`detail > 1`) does
  not navigate.
- Cmd-click (Ctrl elsewhere): toggle the row in the selection (anchor moves to it when added).
- Shift-click: select the range from the anchor to this row in the rendered order (no anchor or
  anchor/target not rendered: add just this row, anchor = it). The anchor stays put.
- Enter/Space on a focused row: navigate.
- Double-click (no modifiers, not on a button/link/input): inline rename (7.4).
- Escape with a selection (window keydown, `web/routes/_chat.tsx:92-96`): clear selection.
- Context menu: 7.3. Selected rows use the selected surface.
- Thread jump / traversal / undo shortcuts: shell.md 4.

### 7.2 Lifecycle actions (RPC via `orchestration.dispatchCommand`, protocol.md section 7)

| Action | Command | Capability | Notes |
| --- | --- | --- | --- |
| Settle | `thread.settle {threadId}` | `threadSettlement` | one in flight per thread; undo notice "Settled"; marks woke visited; acknowledges completion notification |
| Un-settle | `thread.unsettle {threadId, reason:"user"}` | `threadSettlement` | toast "Failed to un-settle thread" |
| Snooze | `thread.snooze {threadId, snoozedUntil}` | `threadSnooze` | rejected client-side unless `canSnooze`; undo notice "Snoozed" |
| Wake | `thread.unsnooze {threadId, reason:"user"}` | `threadSnooze` | toast "Failed to wake thread" |
| Pin | `thread.pin {threadId, orderKey?}` | `threadPinning` (+`threadPinReorder` for orderKey) | toast "Failed to pin thread" |
| Unpin | `thread.unpin {threadId}` | `threadPinning` | confirm when `confirmThreadUnpin` (default false); undo notice "Unpinned" (re-pins with the old key) |
| Pin reorder | `thread.pin.reorder {threadId, orderKey}` | `threadPinReorder` | |
| Active reorder | `thread.active.reorder {threadId, orderKey}` | `threadActiveReorder` | |
| Auto-settle | `thread.auto-settle.set {threadId, enabled}` | `threadAutoSettleOptOut` | toast "Failed to update auto-settle" |
| Rename | `thread.meta.update {threadId, title}` | | toast "Failed to rename thread" |
| Regenerate title | `thread.meta.update {threadId, regenerateTitle:true}` | `threadTitleRegeneration` | disabled while `titleRegeneration != null` |
| Mark unread | local: `threadLastVisitedAtById[key] = latestTurn.completedAt - 1ms` | | no-op without a completion (`web/uiStateStore.ts:273-298`) |
| Archive | `thread.archive {threadId}` | | blocked while a turn runs; confirm when `confirmThreadArchive` (default false); undo notice "Archived" (unarchive) |
| Delete | session stop, terminal close (deleteHistory), `thread.delete` | | confirm when `confirmThreadDelete` (default true); orphan worktree prompt |

Source: `web/hooks/useThreadActions.ts:300-953`, `web/components/Sidebar.tsx:3200-3497,3912-3978`.
Every action re-checks the capability and fails with a typed error instead of sending an unknown
command (version skew). Failures that are interruptions never toast.

Undo for settle restores: un-settle, then re-pin with the previous key if it was pinned, then
re-snooze to the previous `snoozedUntil` (`web/hooks/useThreadActions.ts:676-747`).

**Forward navigation after parking the route thread** (`planForwardNavigation`,
`web/components/Sidebar.tsx:3200-3230`): planned before the command, executed only if the command
succeeded, the route is still that thread, and the thread really is settled / effectively snoozed
afterwards (`shouldNavigateAfterThreadPark`, `web/components/Sidebar.logic.ts:27-41`). Target: the
next rendered row after it (wrapping) that is not settled, not snoozed, and not parking in the same
batch; else a new draft in the thread's project; else `/`. Background parks never navigate.

**Delete details** (`web/hooks/useThreadActions.ts:366-560`): if this thread is the only one
linked to its worktree (and the project is not the scratch project and the server's
`worktreeOnDelete` cleanup is off), confirm (destructive):
`This thread is the only one linked to this worktree:\n<path>\n\nDelete the worktree too?`. Stop
the session if not stopped; close terminals with history deletion; dispatch delete; clear composer
draft, project draft mapping and terminal UI state; if it was the route, navigate (replace) to the
newest remaining thread of the same project (`getFallbackThreadIdAfterDelete`, by
`sidebarThreadSortOrder`) or `/`. Then remove the worktree (`git.removeWorktree {cwd, path,
force:true}`) and refresh VCS status; failures toast "Failed to delete worktree" (`Could not
remove <path>. <msg>`) or "Worktree deleted, but Git status refresh failed". Navigation failure
toasts "Thread deleted, but navigation failed".

**Archive details** (`:304-364`): if the thread is the route, navigate to a new draft in its
project after archiving; undo navigates back.

### 7.3 Context menus (DOM menu, shell.md 6.2)

Single thread (`web/components/threadActionMenu.logic.ts:69-199`), built per open; right-click on a
selected row with a non-empty selection opens the multi menu instead.

| Item (id) | Shown when | Icon | Notes |
| --- | --- | --- | --- |
| `New thread on <branch>` | branch set | message-square-plus | draft on that branch: worktree mode reusing `worktreePath` if any, else local; `startFromOrigin:false`; failure toast "Could not create thread" |
| `Pin thread` / `Unpin thread` | `threadPinning` | pin / pin-off | |
| `Settle thread` / `Un-settle thread` | `threadSettlement` | circle-check | |
| `Snooze` (submenu) / `Wake thread` | `threadSnooze` | clock | Snooze disabled unless `canSnooze`; children `<label> (<whenLabel>)` per preset, separator, `Custom…` |
| separator, `Rename thread` | always | pencil | |
| `Regenerate title` / `Regenerating…` (disabled) | `threadTitleRegeneration` | refresh-cw | |
| `Mark unread` | always | mail-open | |
| `Filter by <project>` / `Show all projects` | thread's group found | folder-tree | toggles scope |
| `Auto-settle behavior` (submenu: `Enabled`, `Disabled`, checked = current) | `threadAutoSettleOptOut` | timer | |
| separator, `Copy` (submenu: `Path` folder, `Branch` git-branch if branch, `Thread ID` hash) | always | copy | Path = `worktreePath ?? project workspaceRoot`; missing -> toast "Path unavailable" / "This thread does not have a workspace path to copy." |
| `Project settings` | always | settings | navigates `/projects/$projectKey` |
| separator, `Archive thread` | always (disabled while running) | archive | |
| `Delete` (destructive) | always | trash | |

Copy toasts: success "Path copied" / "Branch copied" / "Thread ID copied" with the value as
description; failures "Failed to copy path" / "Failed to copy branch" / "Failed to copy thread ID".

Multi-select (`web/components/Sidebar.tsx:3981-4206`), N = selected keys whose rows are rendered:

1. `Unpin (k)`: only pinned rows in pin-capable environments; omitted if k = 0.
2. `Settle (N)`: skips already settled rows; pinned rows included.
3. `Snooze (N)` submenu: only if every selected thread can snooze; presets `label (when)`,
   separator, `Custom…`.
4. `Regenerate titles (n)` or disabled `Regenerating… (n)`; omitted if no selected environment
   supports it.
5. `Mark unread (N)`.
6. `Delete (N)` destructive. Confirm (when `confirmThreadDelete`):
   `Delete N thread(s)?\nThis permanently clears conversation history for these threads.`
   Deletes sequentially, toasts "Failed to delete threads" with the first error, and removes
   deleted/missing keys from the selection.

All bulk actions clear the selection. Bulk snooze failures: "Failed to snooze threads" (none
succeeded) or `Failed to snooze N thread(s)`.

Confirm messages (single): archive `Archive thread "<title>"?`; delete
`Delete thread "<title>"?\nThis permanently clears conversation history for this thread.`
(destructive); unpin `Unpin thread "<title>"?\nThis will move the thread out of your pinned
section.` (`web/hooks/useThreadActions.ts:158-176`).

### 7.4 Inline rename (`web/components/Sidebar.tsx:1534-1546,3140-3174`)

The title becomes an input: flex-1, rounded-sm, 1px `--input`, bg `--card`, px 4, text-sm medium
`--card-foreground`, focus border `--foreground`; autofocus and select all; aria-label "Thread
title". Enter commits, Escape cancels, blur commits. Empty after trim -> warning toast "Thread
title cannot be empty"; unchanged -> no-op. Keys do not propagate; IME composition ignored. The row
is not draggable while renaming.

### 7.5 External file drop on rows (`web/components/Sidebar.tsx:3033-3073`)

Dragging OS files over a thread row or search result highlights it (5). Dropping queues the files
for that thread and navigates there; the chat view's composer picks them up on arrival
(`web/sidebarPendingFileDropStore.ts`, chat spec). Folders are ignored.

### 7.6 Undo notice (`web/hooks/showThreadUndoNotice.ts`, `web/components/sidebar/SidebarThreadUndoNotice.tsx`)

- Actions Settled, Snoozed, Unpinned, Archived push an undo entry. The notice shows the latest
  action and counts the consecutive entries of that same action: `<Action> N thread(s), ` +
  inline button `⌘Z to undo` (shortcut label of `thread.undo`; `Undo` when unbound).
- All live entries expire 5000ms after the most recent push (a new push restarts the timer).
- Undo (button or `thread.undo`): consumes that group and runs every undo in parallel; failures
  toast with the per-action failure title ("Failed to undo settle", "Failed to wake thread",
  "Failed to undo unpin", "Failed to undo archive").
- Newer conflicting actions invalidate older claims (`web/hooks/threadUndo.ts`), e.g. pinning a
  thread invalidates its pending unpin undo.
- Visual: `Alert variant="sidebar"` (`web/components/ui/alert.tsx:7-21`): rounded-lg 10, 1px
  `--sidebar-border`, bg `--sidebar-control-surface`, px 8 py 6, 11px / 16px, description
  `--sidebar-muted-foreground`; `role=status`. The inline button: font medium, hover underline,
  inherits color.

---

## 8. Empty states (`web/components/Sidebar.tsx:5165-5192`)

When not searching, no draft rows, and no listed threads: centered column gap 8, px 8 py 24,
text-xs `--muted-foreground/60`:

- no projects: "No projects yet" + button (rounded-md, 1px `--sidebar-border`, px 10 py 4, 11px
  medium `--sidebar-muted-foreground`, hover bg `--sidebar-row-hover` + `--sidebar-foreground`,
  `plus` 12px + "Add project") opening the command palette `add-project`;
- scoped: `No threads in <project> yet`;
- else "No threads yet".

---

## 9. Footer: `SidebarChromeFooter` (`web/components/sidebar/SidebarChrome.tsx:77-189`)

`SidebarFooter`: flex col, gap 8, px 8, py 4. Children top to bottom:

1. Undo notice (7.6), when present.
2. Provider update pill (9.2), when present.
3. Intel-on-Apple-Silicon warning (Electron only; port as N/A for a universal native build):
   `Alert variant="warning"` with `triangle-alert`, title "Intel build on Apple Silicon" and the
   description from `web/components/desktopUpdate.logic.ts:58-71`.
4. Utility row (9.1).

### 9.1 Utility row (`SidebarUtilityMenu`, `:102-178`)

`ul` flex row items-center gap 4. Also rendered at the bottom of the settings nav
(`web/components/settings/SettingsSidebarNav.tsx:350-362`).

Not on a utility page:

| Button | Icon | Shown when | Action |
| --- | --- | --- | --- |
| Settings | `settings` | always | navigate `/settings` (redirects to `/settings/general`) |
| Pull Requests | `git-pull-request-arrow` | any connected environment has `capabilities.pullRequests` | navigate `/pull-requests` with the saved list preferences as search params (`readPullRequestListPreferences`) |
| Usage | `chart-no-axes-column` | always | navigate `/usage` |

Each is `SidebarMenuButton size="icon"`: 32x32, rounded 8, svg 16px `--sidebar-icon-color`; hover
bg `--sidebar-row-hover` + svg `--sidebar-foreground`; pressed bg `--sidebar-row-active`; focus ring;
tooltip side top (600ms) with the label; aria-label = label. No active state for the current page
(they are replaced by Back there).

On a utility page (`/settings*`, `/projects/*`, `/usage`, `/pull-requests`;
`web/components/sidebar/mainAppLocation.ts:7-15`): one `Back` button instead, flex-1:
`SidebarMenuButton` default size: h 32, rounded 8, px 10, py 6, gap 8, text-sm medium
`--sidebar-muted-foreground/80`, `arrow-left` 16px in `--sidebar-icon-color`; hover bg
`--sidebar-row-hover` + text `--sidebar-foreground`. Click navigates to the last main-app URL
(`MainAppLocationTracker` records every non-utility location href) or `/`.

Then, in both modes:

- Notifications bell (`web/components/sidebar/SidebarNotifications.tsx`): only when
  `notificationInboxEnabled` (default false) and the macOS notification bridge exists. Icon button
  `bell` with a 6px `--destructive` dot at top 4 right 4 when unread exists; aria-label
  "Notifications" / "Notifications, unread items"; popover (side top, align start, offset 10,
  width lg) titled "Notifications", "Mark all read", tabs "Unread (N)" / "All", items with kind
  icon (approval `shield-alert` warning, input `circle-help` info, failed `circle-alert`
  destructive, completed `check` success), thread title, `<kind title>[ · Resolved]`,
  `<project> · <environment>`, per-item "Mark as read" check button; empty "No unread
  notifications" / "No notifications"; error toast "Could not mark notification as read". Off by
  default: document, low priority.
- Update button (`web/components/sidebar/SidebarUpdatePill.tsx`, Electron only, `ml-auto`): 9.3.

### 9.2 Provider update pill (`web/components/sidebar/SidebarProviderUpdatePill.tsx`)

Data: primary server `providers[]` (`server.getConfig` / `subscribeServerConfig` provider
statuses) through `getProviderUpdateSidebarPillView` (`web/components/ProviderUpdateLaunchNotification.logic.ts`).
Shows only for provider update states newer than the first `checkedAt` seen this session.

- Box: min-h 28, full width, rounded-lg 10, 11px/16px medium, overflow hidden. Tone backgrounds:
  loading/success `--sidebar-control-surface` (hover on the main button `--sidebar-row-hover`),
  warning `--warning/12` text `--warning` (hover `/18`), error `--destructive/12` text
  `--destructive` (hover `/18`).
- Main button (flex-1, gap 8, px 8 py 6): icon 14px (loading spinner, success `circle-check`,
  error `triangle-alert`, warning `download`) + title (wraps). aria-label and tooltip = description.
  Click: navigate `/settings/providers`.
- Dismiss (when dismissible): 20x20, mr 4, rounded-md, `x` 14px, opacity .7 -> 1; tooltip "Dismiss
  until provider status changes"; aria-label "Dismiss provider update notice". Dismissal is
  in-memory per view key.
- Auto-dismiss views draw a progress fill (`origin-left` scale-x 1 -> 0 over `dismissAfterVisibleMs`,
  linear; tone `--foreground/8`, `--warning/14`, `--destructive/14`, with a right border
  `currentColor/15`) and exit when done.
- Enter/exit: 180ms `--ease-drawer` (`cubic-bezier(0.32,0.72,0,1)`); exit = translateY 6px +
  opacity 0, then the next view (if any) mounts.

### 9.3 Desktop update button (`web/components/sidebar/SidebarUpdatePill.tsx:115-419`)

32x32 round button at the row's right end. States (`DesktopUpdateState` from the Electron
updater; the native app needs its own updater, see Open questions):

| State | Icon | Look | Click |
| --- | --- | --- | --- |
| idle | `refresh-cw` 16 | `--sidebar-icon-color`, hover bg `--sidebar-row-hover`; opacity .60 when checks are impossible | check for updates; error toast "Could not check for updates" |
| checking | `refresh-cw` spinning (latched to whole turns) | disabled | |
| available | `download` 16 + 6px dot (currentColor, 2px ring `--sidebar-control-surface`) top-right | bg `--sidebar-control-surface`, text `--sidebar-foreground` | download; toasts "Could not download update" / "Could not start update download" |
| downloading | 32px progress ring (r 14, stroke 1.5, track `currentColor/22`, progress dashoffset 300ms ease-out) around `download` 16 | disabled | |
| downloaded | `rotate-cw` 16 + 10px badge (bg `--foreground`, `check` 8px stroke 3, ring 2px `--background`) | filled | confirm `Install update <v> and restart T3 Code?\n\nAny running tasks will be interrupted. Make sure you're ready before continuing.` then install |

Tooltip (side top; glass variant when an update is pending) from
`web/components/desktopUpdate.logic.ts:73-98`: "Check for updates", "Checking for updates…",
`Update <v> ready to download`, `Downloading update (<p>%)`, `Update <v> downloaded. Click to
restart and install.`, `Download failed for <v>. Click to retry.`, `Install failed for <v>. Click to
retry.`, "Up to date". On the nightly channel with release notes, hovering opens a release-notes
popover instead ("Update ready to download", "What's changed", `Changes in <v>`, "View release on
GitHub", `N more change(s) on GitHub`, `N older release(s) on GitHub`)
(`web/components/sidebar/SidebarUpdateReleaseNotes.tsx`).

---

## 10. Data sources

| Element | Source (upstream) | t3UI today |
| --- | --- | --- |
| Thread rows | `orchestration.subscribeShell` thread shells (`OrchestrationThreadShell`: settle/snooze/pin fields, `backgroundLiveness`, `titleRegeneration`, `pullRequests`, `latestTurn`, `session`) | `t3_client::ShellState`; fields present in `t3_protocol::orchestration` (`orchestration.rs:505-530`) |
| Projects + favicons | shell projects; favicon via HTTP asset | `ShellState`, `t3-app/src/state/favicons.rs` |
| Capabilities | `server.getConfig` `environment.capabilities` | `t3_protocol::environment::ExecutionEnvironmentCapabilities` (`environment.rs:62-99`) |
| Provider entries (icon, instance badge, model label) | server config `providers` | in `ServerConfig`; display helpers missing (chat spec) |
| Environment labels / machine kind | environment catalog + `resolveEnvironmentMachineKind(serverConfig)` | `t3-app/src/state/environment.rs`; machine kind resolver missing |
| Lifecycle commands | `orchestration.dispatchCommand` | protocol variants exist (`t3-protocol/src/commands.rs:84-138`); builders exist only for settle/unsettle/archive/unarchive/delete/rename/update (`t3-client/src/commands.rs`): **missing** builders for snooze, unsnooze, pin, unpin, pin reorder, active reorder, auto-settle set |
| Optimistic lifecycle overlay | pending patch until `snapshotSequence >= DispatchResult.sequence` (`cr/state/threadLifecycle.ts:18-101`) | **missing** in `t3_client` |
| Content search | `orchestration.searchThreads` | `t3_protocol::methods::SearchThreads` exists; no client helper/debounce |
| Git status per row | `vcs.status {cwd}` stream | `t3-app/src/state/vcs.rs` |
| Running terminals | terminal session state | terminal spec |
| Unsent drafts | composer draft store (local) | chat spec (`DraftStore`) |
| Last visited, scope, project order | UI state (shell.md 7) | `t3_logic::ui_state::UiState` (needs `sidebarProjectScopeKey`) |
| Shelf expansion | localStorage keys (6.1) | **missing** |
| Unread completion notifications | macOS notification inbox (desktop main) | **missing** (shell.md 9) |
| Thread search corpus terms for PRs | `shared/threadPullRequests.ts` `threadPullRequestSearchTerms` | missing |

---

## 11. Reuse map (sidebar)

| t3UI module | Verdict |
| --- | --- |
| `t3-app/src/sidebar/` (`render.rs`, `mod.rs`, `sort_menu.rs`, `project_dialogs.rs`) | July-era per-project tree = the legacy sidebar. Keep it as the `legacySidebarEnabled` branch; do not grow it into the default. Build the default sidebar as a new view (e.g. `sidebar_v2/` or rename the old one `legacy_sidebar/`). |
| `t3-app/src/sidebar/menus.rs` | Action flows (archive/delete/rename/worktree prompt) are reusable; the native `NativeMenu` presentation must be replaced by the DOM-style menu (shell.md 6.2) and items rebuilt from 7.3. |
| `t3-app/src/sidebar/drag.rs` | Project-drag plumbing for the legacy tree; the default sidebar needs a new thread drag (6.4). |
| `t3-app/src/sidebar/pulse.rs` | Reusable for the terminal pulse (stepped opacity); stop offscreen. |
| `t3-logic/src/sidebar/selection.rs` | Matches `threadSelectionStore` semantics (toggle, range from anchor, anchor rules). Reuse. |
| `t3-logic/src/sidebar/mod.rs` grouping (`physical_project_key`, `logical_project_key`, `legacy_cwd_key`, group labels) | Reuse for scope items and display names; verify against `cr/state/projectGrouping.ts` (label rule `deriveProjectGroupLabel`, representative = preferred environment member). |
| `t3-logic/src/sidebar/status.rs` | Replace: implement `resolve_sidebar_thread_status`, `resolve_thread_status_pill` (adds Connecting, Plan Ready, Monitoring; no Error), `should_recede`, `has_unseen_completion` with the missing-visit rule (4.5). |
| `t3-logic/src/sidebar/sort.rs` | Thread sort by `updated_at`/`created_at` stays for the legacy tree and delete fallback. Add `sort_pinned_by_order_key`, `sort_active_by_order_key`, `sort_inbox_by_return`, `sort_settled`, `resolve_settled_timestamp`, `pin_order_key_between`, `generate_spread_keys`, `plan_pinned_reorder` (4.3). |
| `t3-logic/src/sidebar/pull_request.rs` | Partially reusable; extend to stacks/`+N` (5.8). |
| New `t3-logic` modules | `thread_settled` (4.4: queued start, canSnooze, raised hand, effectiveSnoozed, wokeAt, presets, wake label, custom snooze), sidebar list model (4.1-4.2, 4.6), drop planner (6.4), forward-navigation planner (7.2), relative time compact label (5.5), working duration label. All pure; list failure modes before tests (AGENTS.md). |
| `shell` branch `391b330` `sidebar/footer.rs` | Good start for 9.1: route split and `navigate_to_main_app` match. Fix: PR icon is `git-pull-request-arrow` (not `git-pull-request`), Usage icon `chart-no-axes-column` (stand-in `rows-3`), colors to sidebar tokens (`--sidebar-icon-color`, `--sidebar-row-hover`), add the bell slot and update button (`ml-auto`), and the rows above (undo notice, provider pill). The PR link should carry the list preferences. |

---

## 12. Reference screenshots needed (sidebar)

All at 1440x900 @2x, macOS, light and dark, against a seeded nightly e2e server (so the Nightly art
shows) unless noted. "Seed" refers to `e2e/seed.mjs` data the harness agent should extend.

1. `sidebar-default-populated`: 2 projects; 2 pinned, 4 active (one Working with a running turn,
   one Approval, one Input, one Failed), 1 snoozed, 12 settled; shelves collapsed. Route on an
   active thread.
2. `sidebar-row-hover-card`: pointer over an active ready row (Snooze + Settle actions shown).
3. `sidebar-row-hover-slim`: settled shelf expanded, pointer over a settled row (un-settle icon).
4. `sidebar-shelves-expanded`: Snoozed and Settled expanded, settled > 10 so "Show 2 more" shows.
5. `sidebar-done-woke`: one thread completed after the user last visited it (Done), one woken
   snooze (Woke), one Monitoring thread (needs a server that reports `backgroundLiveness`).
6. `sidebar-multiselect-menu`: three rows Cmd-clicked, right-click menu open.
7. `sidebar-thread-menu`: right-click on a pinned thread with a branch (all submenus closed), then
   with the Snooze submenu open.
8. `sidebar-snooze-popover`: hover Snooze button menu open.
9. `sidebar-custom-snooze-dialog`: both modes.
10. `sidebar-search-results`: query matching two titles and one message content (excerpt line).
11. `sidebar-search-empty`: "No threads found".
12. `sidebar-scope-popup`: project scope combobox open (3 projects), then scoped state (favicon in
    trigger, "No threads in X yet" for an empty project).
13. `sidebar-empty-no-projects`: fresh server, "No projects yet" + Add project.
14. `sidebar-draft-row`: an unsent new-thread draft (type text in a draft, navigate away) and an
    existing thread with unsent composer text.
15. `sidebar-drag-to-settled`: mid-drag of an active row over the Settled header (labels, badge
    "Settle"). Capture via scripted pointer moves.
16. `sidebar-undo-notice`: right after settling two threads ("Settled 2 threads, ⌘Z to undo").
17. `sidebar-rename`: inline rename input.
18. `sidebar-jump-hints`: holding ⌘ for > 200ms (kbd badges on the first 9 rows).
19. `sidebar-tooltip-card`: hover details card on a row with branch, provider, terminal.
20. `sidebar-collapsed`: sidebar toggled off (main column header inset).
21. `sidebar-resize-max` and `sidebar-resize-min`: rail dragged to the limits.
22. `sidebar-footer-utility-page`: on `/usage` (Back button).
23. `sidebar-provider-update-pill`: a provider with an update available.
24. `sidebar-working-shelf-beta`: `sidebarWorkingShelfEnabled = true` with 2 working threads,
    shelf collapsed and expanded.
25. `sidebar-identification-pill` and `-none`: environment identification modes.

---

## 13. Open questions / risks

- Unread semantics depend on `lastVisitedAt`, which only exists for threads visited in this client;
  the macOS notification inbox (Electron main process) also feeds Done. Without porting the inbox,
  a fresh native install shows no Done badges until threads are visited once. Decide whether to port
  the inbox (`desk/notifications/*`, `web/desktopNotifications.ts`).
- The terminal-status pulse and the Working duration ticker are the only continuous animations in
  the list. The pulse is stepped (`steps(6)` per half, 2s cycle); a native port can tick ~6 frames
  per second or render it static. Working duration needs a 1s timer per working row (cheap text
  repaint).
- Desktop update button: the fork drives it from Electron's updater (`DesktopUpdateState`). The
  native app ships DMGs via GitHub Releases (`.github/workflows/release.yml`); it needs its own
  update check or the button should be hidden. Same for the Intel-on-Apple-Silicon alert.
- `surface-grain` noise on the sidebar/main backgrounds is very faint (3.5% alpha); a native port
  can skip it, but reference screenshots include it.
- Drag projection math (`web/components/Sidebar.drag.ts`) is DOM-measurement heavy; the native
  version should compute the projected layout from known row heights (card 82, slim 36, labels 24,
  header 32) instead.
- `content-visibility: auto` on rows is a web perf trick; GPUI should use a virtualized `list` with
  heights 82/36/32/0 and keep markers zero height at rest.
- Stage art SVG is large; render once to an image per width bucket or draw with GPUI paths.
- Theme palettes (`html[data-theme-id]`) override the sidebar palette; the design-system spec must
  confirm which theme the daily app uses before matching screenshots.
