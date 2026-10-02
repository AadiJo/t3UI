# Panels spec: everything outside the sidebar and the chat column

Build spec for the Rust/GPUI port. Covers the right panel system, diff panel, terminal drawer, plan sidebar, files surface, project scripts, Open-in picker, Git actions, PR checkout dialog, and the browser preview.

## 0. Conventions

**Sources.**
- UI truth: fork `~/L-Projects/t3code-again`. Unless noted, paths are relative to `apps/web/src/`.
- Protocol truth: upstream `~/L-Projects/t3UI-refs/t3code-upstream`, tag `v0.0.45-nightly.20261002.2561` (commit `b33eda13`). RPC tags come from `packages/contracts/src/rpc.ts:289-463` (`WS_METHODS`) and `packages/contracts/src/orchestration.ts:35-44` (`ORCHESTRATION_WS_METHODS`).
- Wherever a schema differs between fork and upstream, this doc gives the upstream shape and flags fork-only or upstream-only fields.

**Units.** Tailwind v4: 1 spacing unit is 4px. **In this doc, class-like spacing and size tokens (`p-12`, `px-6`, `gap-8`, `h-28`, `w-144`, `mt-4`, `min-h-112`, …) are already converted to px.** For example, source `px-3` is written here as `px-12`. Only `text-[Npx]`, color/opacity suffixes, and `rounded-*` keep their Tailwind meaning. Text sizes: `text-xs` 12/16, `text-sm` 14/20, `text-[11px]`, `text-[10px]`. Radii: `--radius` 10px, so `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18, plain `rounded` 4.

**Shortcut labels (macOS).** Modifiers render in the order ⌃⌥⇧⌘, followed by the uppercase key, for example `⌥⌘B` and `⇧⌘D` (`keybindings.ts:230-252`).

**Desktop assumptions.**
- The window is always at least 640px wide, so every `sm:` variant applies. For example, `Button size="xs"` renders 24px tall with `text-xs` and 14px icons, and `size="icon-xs"` renders 24x24.
- macOS uses `titleBarStyle: "hiddenInset"` with traffic lights at (16,18) (`apps/desktop/src/window/DesktopWindow.ts:169-170`). Every `wco:` variant is Windows/Linux only, so ignore them.

**Colors.** Colors are given as token names (`background`, `foreground`, `card`, `muted`, `muted-foreground`, `accent`, `border`, `input`, `primary`, `ring`, `success`, `destructive`, `warning`, `info`, plus the `*-foreground` variants). Definitions live in `index.css:453-519`. `x/NN` means that token at NN% alpha, and `mix(a NN%, b)` means `color-mix(in srgb, a NN%, b)` unless "lab" is stated. Literal colors only appear where the source hardcodes them (terminal palette, Pierre theme).

**Shared button and toggle metrics.**

| Control | Size |
|---|---|
| `Button xs` | h-24px, px 7px, gap 4px, rounded-md |
| `Button icon-xs` | 24x24, rounded-md, svg 14px |
| `Button sm` | h-28px, px 9px |
| `Toggle sm` | h-28 min-w-28 |
| `Toggle xs` | h-24 min-w-24, rounded-md |

The Toggle `ghost` variant is transparent; when pressed it fills with `accent` and uses `accent-foreground` text. Sources: `components/ui/button.tsx:18-48`, `components/ui/toggle.tsx:16-27`.

**RPC transport notes that affect panels.**
- Panels use Effect RPC over WebSocket. Client to server: `{_tag:"Request", id:"<n>", tag, payload, headers:[]}`, `{_tag:"Ack", requestId}`, and `{_tag:"Interrupt", requestId}`. Server to client: `{_tag:"Chunk", requestId, values:[...]}` and `{_tag:"Exit", requestId, exit}`.
- **Every stream needs an `Ack` per `Chunk`.** The server waits on a latch before sending the next chunk (`effect/src/unstable/rpc/RpcServer.ts:222-225, 428-450`).
- `terminal.attach` and `subscribeTerminalEvents` get a pre-ack window of 8 chunks or 64 KB (`apps/server/src/terminal/OutputProtocol.ts:5-9`, wired at `apps/server/src/ws.ts:4138`).
- A client that forgets to Ack sees terminal output stall after about 8 chunks.

---

## 1. Right panel system

### 1.1 Files

| File | Role |
|---|---|
| `rightPanelStore.ts` | per-thread surface list and active surface (zustand, persisted) |
| `rightPanelLayout.ts:1-3` | breakpoint and sheet width classes |
| `components/RightPanelTabs.tsx` | tab bar, empty state, context menu, add menu |
| `components/RightPanelSheet.tsx` | overlay mode wrapper (base-ui Sheet) |
| `components/preview/PreviewPanelShell.tsx` | inline width, resize, open/close clip wrapper |
| `components/preview/RightPanelResizeHandle.tsx:17-33` | left-edge drag handle |
| `hooks/useResizableWidth.ts` | drag math and persistence |
| `hooks/usePanelTransitionPresence.ts` | mount/open/entered state machine |
| `components/chat/PanelLayoutControls.tsx` | terminal and right-panel toggles, maximize button |
| `components/ChatView.tsx:5016-5470` | composition |
| `diffPanelStore.ts` | diff scope selection (section 2) |
| `index.css:124-215, 272-282, 416-426` | topbar, titlebar controls, panel and drawer transitions |

### 1.2 State model (`rightPanelStore.ts`)

```ts
type RightPanelSurface =
  | { id: `browser:${tabId}`; kind: "preview"; resourceId: tabId }
  | { id: "browser:new";     kind: "preview"; resourceId: null }      // placeholder before preview.open returns
  | { id: `terminal:${terminalId}`; kind: "terminal"; resourceId; terminalIds: string[]; activeTerminalId; splitDirection?: "horizontal"|"vertical" }
  | { id: "diff";  kind: "diff" }
  | { id: "files"; kind: "files" }
  | { id: `file:${relativePath}`; kind: "file"; relativePath; revealLine: number|null; revealRequestId: number }
  | { id: "plan";  kind: "plan" };
ThreadRightPanelState = { isOpen: boolean; activeSurfaceId: string|null; surfaces: RightPanelSurface[] }
```

- The state is keyed by `scopedThreadKey(ref)` (environmentId plus threadId). It persists to localStorage under `t3code:right-panel-state:v2` at version 7 (`:42-43`). Port this as a per-thread map persisted to disk. The migration at `:156-236` does not apply to a fresh native app.
- Thread entries that become empty, with `!isOpen`, no active surface, and no surfaces, are deleted (`:135-149`).

**Actions and their exact semantics.**

| Action | Lines | Behavior |
|---|---|---|
| `upsertSurface` | `:123-133` | Appends the surface if its id is new, sets `isOpen = true`, and activates it by default. |
| `open(kind)` | `:242-251` | Singleton kinds upsert. For `preview`, reuses the first preview surface or adds `browser:new`. |
| `openBrowser(tabId)` | `:252-261` | When `tabId` is non-null it drops the `browser:new` placeholder, then upserts `browser:${tabId}`. |
| `openFile(path, line?)` | `:262-288` | Removes the standalone `files` explorer surface. Reuses `file:${path}` in place, or appends it. Bumps `revealRequestId`. Line is clamped to an int of at least 1, otherwise null. Activates and opens the panel. |
| `openTerminal(id)` | `:289-294` | Upserts `terminal:${id}` with `terminalIds:[id]`. |
| `splitTerminal(surfaceId, id, dir="horizontal")` | `:295-314` | Adds `id` to that surface's `terminalIds` and makes it active. Sets `splitDirection:"vertical"` only for vertical splits; any horizontal split clears it. |
| `closeTerminal(surfaceId, id)` | `:329-367` | Removing the last id removes the whole surface, using the same fallback as `closeSurface`. Otherwise, if the closed id was active, the new active id is the last remaining one. |
| `closeSurface(id)` | `:376-393` | The new active surface is `surfaces[min(index, len-1)]` after removal. `isOpen` stays true only if surfaces remain. |
| `closeOtherSurfaces(id)` | `:394-406` | Keeps only that surface, active and open. |
| `closeSurfacesToRight(id)` | `:407-422` | Truncates after `id`. If the active surface was removed, `id` becomes active. |
| `closeAllSurfaces` | `:423-430` | Empties the list and closes the panel. |
| `reconcileBrowserSurfaces(tabIds)` | `:431-459` | Called whenever the thread's preview sessions change (`ChatView.tsx:1460-1465`). Non-browser surfaces come first, then known browser tabs in their existing order, then new tabs. Drops `browser:new` and dead tabs. **The order changes: browsers move after non-browser surfaces.** |
| `reconcileFileSurfaces(available)` | `:460-480` | When no workspace exists, removes `files` and `file:*`. The fallback active surface is the last remaining one. |
| `show` / `close` / `toggleVisibility` | `:481-499` | Flip `isOpen` only. Surfaces are kept, so close plus reopen restores the same tabs. |
| `toggle(kind)` | `:500-515` | If the panel is open and the active surface has that kind, closes the panel. Otherwise behaves like `open(kind)`. |

Selectors `selectActiveRightPanel` and `selectActiveRightPanelSurface` (`:544-560`) return null when `!isOpen`.

**Side effects owned by `ChatView` when surfaces close** (`ChatView.tsx:3080-3112`, `cleanupRightPanelSurfaces`):
- **Preview:** calls `closePreviewSession` (RPC `preview.close`).
- **Terminal:** for each id, closes it in the terminal UI store and calls RPC `terminal.close {threadId, terminalId, deleteHistory:true}`.
- **Plan:** marks the plan "dismissed for this turn" so auto-open does not reopen it.

### 1.3 Layout modes

| Mode | Condition | Render |
|---|---|---|
| inline | window width > 980px (`RIGHT_PANEL_INLINE_LAYOUT_MEDIA_QUERY = "(max-width: 980px)"` is false) | `RightPanelTabs mode="inline"`, a sibling to the right of the chat column (`ChatView.tsx:5414-5439`) |
| inline maximized | inline mode plus `maximizedRightPanelThreadKey === routeThreadKey` | The chat column becomes `w-0 flex-none` (`:5124-5126`). The panel switches to `flex-1 border-l border-border` and the clip/resize wrapper is skipped. |
| sheet (overlay) | window width ≤ 980px | `RightPanelSheet` (base-ui Sheet, side right) wraps `RightPanelTabs mode="sheet"` (`:5441-5470`). The terminal and right-panel toggles move inside the tab bar via `layoutControls`. |

- Maximize is per-thread UI state and is not persisted (React state in `ChatView`).
- It is only possible when the panel is open and the layout is inline (`:1455-1457`).
- Closing the panel through `closePreviewPanel` or `closePlanSidebar` clears maximize (`ChatView.tsx:2896-2940`).

**Sheet geometry** (`rightPanelLayout.ts:2-3`, `components/ui/sheet.tsx:21-91`):
- Width is `min(42vw, 448px)`, with min 320px and max 448px. Below a 760px window width, it is `min(88vw, 384px)` with no min.
- Padding 0, `bg-popover`, `border-s`, `shadow-lg/5`.
- Backdrop: `bg-background/60` with `backdrop-blur-xs` (4px), fixed inset 0, z-50.
- Enter and exit animate opacity 0 to 1 plus `translate-x-8` (32px), over 180ms with `cubic-bezier(0.4,0,0.2,1)`.
- Clicking the backdrop calls `onClose`, which runs `closePlanSidebar` if the plan is the active surface and `closePreviewPanel` otherwise.

### 1.4 Box tree (inline, not maximized)

```
ChatViewContent root  div.relative.flex.min-h-0.min-w-0.flex-1.overflow-hidden bg-background   (ChatView.tsx:5121)
├─ [if panel mounted] TitlebarControls (absolute; see 1.8)
├─ ChatColumn  flex-1 flex-col (or w-0 when maximized)
│   ├─ header (52px, chat spec)            ← toggles live here when the panel is NOT mounted
│   ├─ … messages/composer (chat spec)
│   └─ TerminalDrawer(s) (section 3)
└─ div.workspace-inline-panel  relative h-full shrink-0 self-stretch     (PreviewPanelShell.tsx:59-81)
     style: --workspace-inline-panel-width: {W}px
     width: 0 → W when data-open="true" (transition 180ms)
     ├─ div.workspace-inline-panel-viewport absolute inset-0 overflow-clip bg-background   (opacity animated)
     │   └─ Panel  relative flex h-full min-h-0 min-w-0 flex-col self-stretch overflow-clip bg-background isolate
     │        ml-auto  width=W  min-width=W  shrink-0  border-l border-border          (:42-53)
     │        ├─ TabBar (workspace-topbar: flex, h 52px, items-center) gap 4px, pl 8px, pr 112px  (RightPanelTabs.tsx:360-368)
     │        │    ├─ ScrollArea (flex-1, horizontal, hidden scrollbars, edge fade ≤24px; it is the drag region)
     │        │    │    └─ row: h-full w-max min-w-full items-center gap 4px
     │        │    │         ├─ Tab × N
     │        │    │         └─ AddButton (only when N > 0)
     │        │    └─ layoutControls (sheet mode only)
     │        └─ Content  flex min-h-0 flex-1 flex-col → EmptyState | active surface view
     └─ ResizeHandle  absolute inset-y-0 left:-4px width 8px z-20 cursor col-resize     (RightPanelResizeHandle.tsx:20-31)
          └─ 1px line centered; transparent → `border` on hover → `primary/60` while active; color transition 150ms
```

- Tab-bar right padding is 112px in inline mode and 12px in sheet mode.
- When inline **and** maximized **and** the sidebar is collapsed, the tab bar also gets `padding-left: var(--workspace-titlebar-content-left)` (12 + 28 + 12 = 52px) so tabs clear the traffic lights and sidebar toggle (`workspaceTitlebar.ts:1-2`, `index.css:33-37`).
- The empty part of the tab strip is a window drag region (`drag-region`). Tabs and buttons are not.
- The panel itself keeps its full width W while the clip wrapper animates from 0 to W. It is right-anchored by `ml-auto`, so the content slides in without reflowing.
- In GPUI, render the panel at fixed width W inside a clipping container whose width animates. Do not animate the panel's own width.

### 1.5 Width and resize (`PreviewPanelShell.tsx:11-39, 91-111`, `useResizableWidth.ts`)

**Width bounds.** Default 540px, min 360px. The max is `min(1400, floor(0.7 × windowWidth))` and recomputes on window resize (rAF-coalesced). A stored width is clamped on every render.

**Persistence.** The width is global, shared by all threads, and stored under the localStorage key `t3code:preview-panel-width` as a number. It is written once, on pointer-up. A cancelled drag reverts to the start width.

**Drag behavior.**
- Primary button only. It captures the pointer and sets the body cursor to `col-resize` with `user-select:none`.
- New width is `clamp(startWidth + (startX - pointerX))`, applied at most once per frame.
- While dragging, `data-resizing="true"` disables the width transition.

### 1.6 Open/close animation (`usePanelTransitionPresence.ts`, `index.css:158-187`)

Presence state is `{mounted, open, entered}`, with constants `WORKSPACE_PANEL_TRANSITION_MS = 180` and `WORKSPACE_PANEL_DELAYED_CONTROL_MS = 120`.

1. When a request to open arrives and the panel is not mounted, set `mounted=true` with `open=false` and paint one closed frame. For the right panel, `openingFrameCount` is 1; the terminal drawer uses 2.
2. After N animation frames, set `open=true`, which starts the CSS transitions.
3. 120ms after `open`, set `entered=true`. The maximize button appears at this point.
4. When a request to close arrives, set `open=false` and `entered=false`, then unmount 180ms later.

CSS (honor reduced motion by disabling the transitions):

| Property | Opening | Closing |
|---|---|---|
| wrapper width | 0 to W, 180ms `cubic-bezier(0.4,0,0.2,1)` | W to 0, same timing |
| viewport opacity | 0 to 1, 140ms ease-out, 30ms delay | 1 to 0, 100ms ease-in |

While closed-but-mounted, the wrapper has `pointer-events:none`.

The chat header's action cluster animates `padding-right` with the same 180ms curve (`ChatHeader.tsx:104-107`, see 1.8).

### 1.7 Tab bar (`RightPanelTabs.tsx`)

**Tab element** (`:382-436`):
- Box: `group flex h-28 min-w-100 max-w-176 shrink-0 items-center gap-6 rounded-md px-8 text-sm`.
- Active: `bg-accent text-foreground`.
- Inactive: `text-muted-foreground`; on hover `bg-accent/60 text-foreground`.

Tab children:
1. The main button (`flex min-w-0 flex-1 items-center gap-6`) holds a 14px icon and a truncated title. Hovering shows a tooltip with the title. Clicking calls `onActivate`.
2. The close button is 16x16 with `rounded` (4px) and `hover:bg-muted`. It contains an X icon at 12px.
   - Normally it is `opacity-0`, and the group's hover reveals it.
   - When the surface is **pending** (an unsaved file write), it shows an 8px `rounded-full bg-current` dot at full opacity instead. Group hover swaps the dot for the X.
   - aria-label: `Close {title}`.

**Titles** (`:190-220`):

| Surface | Title |
|---|---|
| diff | `Diff` |
| files | `Files` |
| file | basename of `relativePath` |
| terminal | server label of `activeTerminalId` (`TerminalSummary.label`), falling back to `Terminal N` for `term-N` |
| plan | `Plan` |
| preview | `Browser` when there is no snapshot or nav is Idle; otherwise `navStatus.title` if non-blank, else `new URL(url).host`, else `Browser` |

**Icons** (`:222-271`), all 14px:

| Surface | Icon |
|---|---|
| preview | favicon `img` from `faviconUrlForOrigin(url, 32)`, 14px, `rounded-sm`. Falls back to lucide `Globe2` when there is no URL or the image errors. |
| diff | `FileDiff` |
| files | `Files` |
| file | `PierreEntryIcon` (file-type icon, theme-aware, see 4.2) |
| terminal | `TerminalSquare` |
| plan | `ClipboardList` |

**Interactions.**
- **Middle-click** (`:335-347`): `mousedown` with button 1 is prevented, and `auxclick` with button 1 closes the surface.
- **Context menu** (`:278-334`) uses the native macOS menu via `api.contextMenu.show` at the cursor. Items in order:
  - `Copy path` (file surfaces only; copies the relative path and shows the toast `Path copied` / description = path)
  - `Close`
  - `Close others` (disabled when ≤1 surface)
  - `Close to the right` (disabled when last)
  - `Close all` (disabled when 0)
- **Auto-scroll:** when `activeSurfaceId` changes, scroll the active tab into view with "nearest" alignment (`:349-352`).

**Add button** (`:438-477`):
- Rendered only when there is at least 1 surface. It is 28x28 `rounded-md text-muted-foreground`; on hover it gets `bg-accent text-foreground`. Plus icon 16px; aria `Add panel surface`.
- The menu opens below, start-aligned, with a 6px offset and min-width 176px. Items in order: Browser (`Globe2`), Terminal (`TerminalSquare`), Files (`Files`), Diff (`FileDiff`).
- Disabled items stay hoverable and show a tooltip (top) with the reason:
  - `Browser previews are only available in the T3 Code desktop app.`
  - `Files are only available when a project is open.`
  - `Diff is only available for server threads in Git repositories.`
  - In the native app the Browser item is always available on macOS.
- Availability flags (`ChatView.tsx:5431-5434`): Browser is always available on desktop; Files needs an active project; Diff needs `isServerThread && isGitRepo`. Terminal is always available.

**Empty state** (`:90-188`), shown when `activeSurfaceId === null`:
- Centered in a `p-24` container; max width 576px.
- Heading `Open a surface` (`text-sm font-medium`), subtext `Choose what to show in the right panel.` (`text-xs muted-foreground`, mt 4px). The heading block has mb 20px.
- A 2-column grid (gap 8px) of four cards:

| Label | Description |
|---|---|
| Browser | `Open a local app or URL.` |
| Terminal | `Start a shell in this workspace.` |
| Files | `Browse and read workspace files.` |
| Diff | `Review changes in this thread.` |

- Card box: `flex min-h-112 w-full flex-col items-start rounded-lg border border-border/80 bg-card/40 p-16 text-left`. On hover: `border-border bg-accent/60`.
- Card contents: a 20px icon (mb 12px), the label (`text-sm font-medium`), and the description (`mt-4 text-xs leading-relaxed muted-foreground`).
- Disabled cards add `opacity-40 cursor-not-allowed` and show the disabled-reason tooltip.

**Add or activate handlers** (`ChatView.tsx:2902-2969, 3040-3060`):
- **Browser:** `addBrowserSurface`, which calls `preview.open {threadId}` and then `openBrowser(tabId)`.
- **Terminal:** id = `nextTerminalId(known ∪ panel ids)` (`term-N`, lowest unused). Calls `openTerminal(surface)` and then RPC `terminal.open {threadId, terminalId, cwd: gitCwd ?? workspaceRoot, worktreePath?, env}`.
- **Diff:** `open("diff")`.
- **Files:** `open("files")`.
- Activating a preview tab sets the active preview tab. Activating a terminal tab bumps the terminal focus request.

### 1.8 Titlebar controls and chat-header coupling

`PanelLayoutControls` (`components/chat/PanelLayoutControls.tsx:25-94`) is `flex h-full items-center gap-4`, no-drag. It holds two toggles, each `Toggle variant="ghost" size="sm"` (28x28, rounded-lg) with a 14px icon:

| Order | Toggle | Pressed | Icon open / closed | aria-label | Tooltip (bottom) | Disabled when |
|---|---|---|---|---|---|---|
| 1 | Terminal | `terminalOpen` | `PanelBottomCloseIcon` / `PanelBottomOpenIcon` | `Toggle terminal drawer` | `Toggle terminal drawer (⌘J)`; unavailable: `Terminal drawer is unavailable` | no active project |
| 2 | Right panel | `rightPanelOpen` | `PanelRightCloseIcon` / `PanelRightOpenIcon` | `Toggle right panel` | `Toggle right panel (⌥⌘B)`; unavailable: `Right panel is unavailable` | no active project |

The maximize control, `RightPanelMaximizeControl` (`:96-127`), is also a ghost `Toggle sm`:
- Icon: `Maximize2Icon`, or `Minimize2Icon` when maximized.
- Label and tooltip: `Maximize panel` / `Restore panel size`.
- It renders only when the panel is open, `presence.entered` is true, and the layout is inline.
- On appearing it animates `workspace-panel-control-enter`: 70ms ease-out, opacity 0 to 1, scale 0.94 to 1 (`index.css:416-426`).

**Placement** (`ChatView.tsx:5028-5039, 5122, 5146`):
- The wrapper `div.workspace-titlebar-controls` is positioned absolute at top 0, right 12px, 52px tall, flex `items-center`, gap 4px, z-50.
- It has a `::before` scrim: a gradient from transparent to `background` over 14px, extending 14px left and 4px right of the controls. On the glass window (`.desktop-glass-window`) the scrim is removed.
- Order inside the wrapper: [Maximize], Terminal toggle, Right-panel toggle.
- **Panel not mounted:** the wrapper is rendered inside the chat `<header>`, which is `relative`. The chat header's action cluster reserves `padding-right: 60px` (`--workspace-chat-header-control-reserve` = 2 × 28 + 4) for it.
- **Panel mounted (inline):** the wrapper moves to the ChatView root, so it floats over the right panel's tab bar. The tab bar reserves 112px of right padding for it. The chat header reserve animates to 0 (`ChatHeader.tsx:102-107`, `controlsInRightPanel = presence.open && inline`).

### 1.9 Entry points, keybindings, auto-open

Keybindings (`packages/shared/src/keybindings.ts:21-54`, handled in `ChatView.tsx:3760-3880` and `routes/_chat.tsx:87-127`):

| Command | Key | When | Action |
|---|---|---|---|
| `terminal.toggle` | mod+j | always | toggle drawer (3.2) |
| `rightPanel.toggle` | mod+alt+b | always | If open: `closePlanSidebar` when plan is active, else `closePreviewPanel`. If closed: `toggleVisibility`. With no surfaces this shows the empty state. |
| `diff.toggle` | mod+d | `!terminalFocus` | server threads only: `toggle("diff")` |
| `preview.toggle` | mod+shift+j | always | If the browser surface is active, close the panel. Otherwise open the active preview tab, or create a browser surface (`ChatView.tsx:2920-2934`). |
| `preview.refresh` / `focusUrl` / `zoomIn` / `zoomOut` / `resetZoom` | mod+r / mod+l / mod+= and mod++ / mod+- / mod+0 | `previewFocus` | forwarded to the visible PreviewView (5.x) |

Other entry points:
- **Turn diff from the timeline:** `selectTurn(turnId, filePath)`, then `open("diff")` (`ChatView.tsx:4988-4997`).
- **File links in markdown:** `openFile(path, line)` (`ChatMarkdown.tsx:1051`).
- **File title in a diff header:** `openFile` (`diffFileActions.ts:14-25`).
- **Terminal URL links:** Cmd-click shows a native menu with `Open in preview` and `Open in browser` (3.6).

**Plan auto-open** (`ChatView.tsx:3574-3594`). The setting `autoOpenPlanSidebar` controls it. The plan opens when all of these hold:
- an active plan exists;
- the plan surface is not already showing;
- the plan's turn is the latest turn;
- the user has not dismissed it for that turn.

A switch to a new thread can also force the plan open (`:3563-3568`).

### 1.10 GPUI notes

- Keep `RightPanelState` as a model entity keyed by thread. Persist it with serde to app data; store the width separately as a global value.
- **Inline panel:** use a custom element, `div().w(animated_w).overflow_hidden()`, containing a child with fixed width W aligned right.
  - Drive `animated_w` and opacity with a GPUI animation, or with a timer-driven tween using the curve `(0.4,0,0.2,1)`.
  - Keep the content entity alive during the 180ms close.
- **Sheet mode:** `gpui-component` `Sheet` can be restyled to the metrics above.
- The tab strip is a horizontally scrollable `div` with a manual edge-fade mask. Use a gradient overlay that matches `background`, sized to `min(24px, overflow)`.

---

## 2. Diff panel

### 2.1 Files

- `components/DiffPanel.tsx`: the whole panel.
- `components/DiffPanelShell.tsx`: header row and loading skeleton. Only `mode="embedded"` is used (`ChatView.tsx:5078`); the `inline` width classes there are dead code.
- `components/diffs/AnnotatableCodeView.tsx`: Pierre `CodeView` plus review comments.
- `lib/diffRendering.ts`: parse, keys, theme names.
- `diffFileActions.ts`: clicking a header title opens a file surface.
- `components/DiffWorkerPoolProvider.tsx`: Shiki worker pool.
- `reviewCommentContext.ts`: comment model and prompt serialization.
- `components/files/LocalCommentAnnotation.tsx`: comment card UI.
- `diffPanelStore.ts`: scope selection.
- `components/chat/ChangedFilesTree.tsx`, `lib/turnDiffTree.ts`, `components/chat/DiffStatLabel.tsx`: the per-turn changed-files card in the timeline (2.8).

### 2.2 Data sources

| Scope | RPC (upstream tag) | Payload | Response |
|---|---|---|---|
| Turn N, with N-1 > 0 | `orchestration.getTurnDiff` | `{threadId, fromTurnCount: N-1, toTurnCount: N, ignoreWhitespace?}` | `ThreadTurnDiff {threadId, fromTurnCount, toTurnCount, diff: string}` (`orchestration.ts:2245-2292`) |
| Turn 1 (from = 0) | `orchestration.getFullThreadDiff` | `{threadId, toTurnCount: N, ignoreWhitespace?}` | same shape |
| Working tree / branch | `review.getDiffPreview` | `{cwd, baseRef?, ignoreWhitespace?}`; upstream-only: `file?: {path, previousPath, sourceKind}` | `{cwd, generatedAt, sources: [{id, kind: "working-tree"\|"branch-range", title, baseRef\|null, headRef\|null, diff, diffHash, truncated, files?: [{path, previousPath, additions, deletions}]}]}` (`review.ts:6-67`). The UI picks the `working-tree` source for "Working tree" and `branch-range` for "Branch changes". |
| Base-ref picker | `vcs.listRefs` | `{cwd: preview.cwd, includeMatchingRemoteRefs:true, refKind:"local"\|"remote", query?, limit:100}` (two calls) | `{refs: VcsRef[], isRepo, hasPrimaryRemote, nextCursor, totalCount}`; `VcsRef = {name, isRemote?, remoteName?, current, isDefault, worktreePath\|null}` |
| Is-repo gate | `subscribeVcsStatus` (stream) | `{cwd}` | `VcsStatusStreamEvent` (6.3) |
| Turn list | thread projection `checkpoints[]` from `orchestration.subscribeThread` | n/a | `OrchestrationCheckpointSummary {turnId, checkpointTurnCount, checkpointRef, status: "ready"\|"missing"\|"error", files: [{path, kind, additions, deletions}], assistantMessageId\|null, completedAt}` (`orchestration.ts:631-651`) |

Details:
- `cwd` is `thread.worktreePath ?? project.workspaceRoot` (`DiffPanel.tsx:216`).
- If the branch preview fails with an error containing `configured workspace root` (server message `Review diff preview cwd must stay within the configured workspace root.`, upstream `apps/server/src/review/ReviewService.ts:84-86`), retry once with `serverConfig.cwd` (`DiffPanel.tsx:327-346`).
- The turn list is sorted by `checkpointTurnCount` (or the inferred count) descending, then `completedAt` descending (`:235-248`). If the selected turn disappears, the selection snaps to the latest (`:250-256`, `diffPanelStore.ts:88-106`).
- Upstream also has `review.getDiffFileContents {cwd, sourceKind, changeType, baseRef, headRef, oldPath, newPath}`, which returns `{oldContents, newContents}`. The fork does not use it. It is only needed if "expand unchanged lines" is added later.

### 2.3 Selection state (`diffPanelStore.ts`)

- `DiffPanelSelection` is one of `{kind:"branch", baseRef|null}`, `{kind:"unstaged"}`, or `{kind:"turn", turnId, filePath|null, revealRequestId}`. The default is `branch/null` (`:8-13`).
- It is per-thread and persisted under `t3code:diff-panel-state:v1`. The branch base ref is remembered separately in `branchBaseRefByThreadKey`, so switching to "Working tree" and back keeps it (`:35-71`).
- `selectTurn` bumps `revealRequestId`. The view then scrolls file `filePath` to the top (`DiffPanel.tsx:440-445`).
- Panel-local and not persisted:
  - `diffRenderMode`: `"stacked"`, which maps to Pierre `unified`, or `"split"`. Default stacked.
  - `wordWrap`: initialized from setting `wordWrap`.
  - `diffIgnoreWhitespace`: initialized from setting `diffIgnoreWhitespace`.
  - collapsed-file set, scoped to `env:thread:sectionId`.

### 2.4 Layout

```
DiffPanelShell  flex h-full min-w-0 flex-col bg-background w-full            (DiffPanelShell.tsx:20-46)
├─ HeaderRow  .surface-subheader: h 40px, border-b border/60, bg-background; px 16px; justify-between; gap 8px
│   ├─ Left  flex min-w-0 flex-1 items-center gap 12px
│   │   ├─ ScopeTrigger  h 24px, px 8px, rounded-md, bg-muted/70 → hover bg-muted, text-xs font-medium; label + ChevronDown 14px muted
│   │   └─ [branch scope only] Compare: text-xs muted, gap 8px:  {headRef ?? "HEAD"} (max-w 192, truncate)  ArrowRight 14px opacity-70  BaseRefCombobox
│   └─ Right  flex shrink-0 items-center gap 4px
│       ├─ ToggleGroup outline xs: [Rows3 12px "Stacked diff view"] [Columns2 12px "Split diff view"]
│       ├─ Toggle outline xs: TextWrap 12px   (tooltip "Enable line wrapping" / "Disable line wrapping")
│       └─ Toggle outline xs: Pilcrow 12px    (tooltip "Hide whitespace changes" / "Show whitespace changes"; pressed = ignoring)
└─ Body (see 2.5)
```

**Scope label.** One of `Working tree`, `Branch changes`, `Latest turn`, or `Turn {n}` (`DiffPanel.tsx:273-280`).

**Scope menu** (`:505-560`), width 240px. Items:
- `Working tree`
- `Branch changes`
- `Latest turn`
- submenu `Turn`, width 256px: one row per turn, showing `Turn {n}` with `ml-auto text-xs tabular-nums muted` `formatShortTimestamp(completedAt)`.

The current choice shows a trailing `CheckIcon`.

**Base-ref combobox** (`:569-671`):
- Trigger: `rounded-md px-6 py-4`, max width 192px. On hover: `bg-muted text-foreground`. Contents: `{baseRef}` plus ChevronDown 14px at opacity 70.
- Popup: width 288px.
  - A search row: magnifier 16px at `muted/55`; input with placeholder `Search refs...`, height 26px, transparent; bottom border `border/70` that becomes `ring` on focus.
  - A column header row: `text-[10px] uppercase tracking-wide muted`, columns `Branch` and `Remote` (right-aligned, 32px column).
  - A list with max height 256px. Rows are 32px tall:
    - The first row is `Automatic`; choosing it sets baseRef null.
    - Ref rows show the label (truncated).
    - If both local and remote exist: a small switch (`--thumb-size: 12px`) toggles the remote version, with aria `Use remote version of {label}`.
    - Remote-only refs show a 12px check (title `Remote only`).
  - Empty text: `No matching refs.`
- Local refs exclude the head ref.

### 2.5 Body states (`DiffPanel.tsx:741-875`)

All placeholder text is `flex-1 centered px-20 text-center text-xs muted-foreground/70`. States in priority order:

1. No thread: `Select a thread to inspect turn diffs.`
2. Not a repo: `Turn diffs are unavailable because this project is not a git repository.`
3. Turn scope with zero turns: `No completed turns yet.`
4. Otherwise a viewport (`diff-panel-viewport`, bg `mix(background 94%, card)`, flex-1 overflow-hidden) containing:
   - **If truncated:** a banner `px-12 py-6 text-[11px] muted bg-muted/40 border-b border/70` reading `This diff was truncated because it exceeded the preview limit. The changes shown are incomplete.`
   - **Error with no patch:** `text-[11px] text-red-500/80`, the error string, `px-12`, mb 8px.
   - **Loading:** a skeleton card (`DiffPanelShell.tsx:62-88`): p 8px; card `rounded-md border border/60 bg-card/25`; header row with a 128px and an 80px pill; five line pills at 12px height and widths 100/100/83/92/75%. Labels: `Loading checkpoint diff...`, `Loading working tree diff...`, `Loading branch diff...`.
   - **Empty:** `No net changes in this selection.` when the patch is whitespace-only, else `No patch available for this selection.`
   - **Unparseable:** a `p-8` block with the reason `Unsupported diff format. Showing raw patch.` or `Failed to parse patch. Showing raw patch.` (`text-[11px] muted/75`). Below it, a `<pre>` (`max-h 72vh rounded-md border/70 bg-background/70 p-12 font-mono text-[11px] leading-relaxed muted/90`) that wraps if `wordWrap` is on.
   - **Files:** the code view (2.6).

### 2.6 Rendering the diff (Pierre `@pierre/diffs` 1.3.0-beta.5, Apache-2.0, patched)

**Parsing.**
- `parsePatchFiles(patch.trim(), cacheKey)` produces `FileDiffMetadata[]`. The fork patches `@pierre/diffs` (`patches/@pierre%2Fdiffs@1.3.0-beta.5.patch`, editor-only changes).
- For git scopes, partial-hunk offsets are compacted (`lib/diffRendering.ts:65-88`). Patches are always partial (no full file contents), so **hunk separators are never expandable**.
- Files are sorted by path with numeric, base-sensitivity collation (`DiffPanel.tsx:415-425`).
- Path: `name ?? prevName`, with a leading `a/` or `b/` stripped (`diffRendering.ts:127-133`).
- File key: `cacheKey ?? "${prevName ?? "none"}:${name}"`.
- **Rust:** parse the unified diff yourself, or use `diffy`/`patch` and handle git headers (`diff --git`, `rename from/to`, `new/deleted file mode`, binary). Change types: `change`, `new`, `deleted`, `rename-pure`, `rename-changed`.

**Options passed** (`DiffPanel.tsx:843-852`):
- `diffStyle` is `unified` or `split`.
- `lineDiffType: "none"` is passed, **but changed words are still emphasized**: with the worker pool active, Pierre takes render options from the pool, whose default is `word-alt` (verified by rendering the real components in Chromium). Pairs are the i-th deletion and i-th addition of a change run; spans come from jsdiff 8 `diffWordsWithSpace` with Pierre's `word-alt` joining (`utils/parseDiffDecorations.js`), skipped for lines over 1000 chars. Emphasis bg is the host-level `--diffs-bg-{addition,deletion}-emphasis` (base color at 20% dark / 15% light), radius 3px.
- `overflow` is `wrap` or `scroll`.
- `theme` is `pierre-dark` or `pierre-light`; `themeType` follows the app.
- `stickyHeaders: true`.
- `layout: {paddingTop: 8, paddingBottom: 8, gap: 8}`.
- `diffIndicators` defaults to `"bars"`, `hunkSeparators` to `"line-info"`, `collapsedContextThreshold` to 1, and `tokenizeMaxLineLength` is 1000 (`DiffWorkerPoolProvider.tsx:51-55`).

**Geometry.**
- Per-file cards are full-width, with 8px between cards and 8px above the first and below the last.
- Cards are **flat**: no border, no radius. `index.css:1058-1064` targets `.diff-render-surface > diffs-container`, but Pierre 1.3's CodeView nests the containers two divs deep, so the rule never matches. The 8px gaps show the viewport color.
- The scroll container shows the app's 6px styled scrollbar (`index.css` `::-webkit-scrollbar`), so cards are `W - 6` wide while the content overflows.
- No gap between a header and the file's first row.
- Base font: mono (the `--font-mono` stack: `"SF Mono", "SFMono-Regular", "JetBrains Mono", Consolas, "Liberation Mono", Menlo, monospace`) at **13px with a 20px line height**. Tab size 2.
- Rows are 20px in scroll mode. In wrap mode they use `pre-wrap` and `word-break: break-word`.
- **Scroll mode:** each file's code area scrolls horizontally on its own (each side separately in split mode). The gutter is `position: sticky; left: 0` at z 3, so line numbers stay put. A `scrollbar-gutter: stable` horizontal scrollbar sits under each file. Annotation cards are sticky-left too, sized to the visible content width.

**File header** (Pierre `utils/createFileHeaderElement.js` plus T3 overrides at `DiffPanel.tsx:116-175`):
- Sticky to the top of the scroll view at z 4. Min height 32px, padding-block 6px, padding-inline 16px, gap 8px.
- bg `mix(card 94%, foreground)`, border-bottom 1px `border`, sans 12px, line-height 1.
- Children, left to right:
  1. Collapse button (T3 prefix slot): 20x20, radius 6, hover `foreground/10`. ChevronDown 16px when expanded, ChevronRight when collapsed. Colored by change type: new uses addition-base, deleted uses deletion-base, change or rename uses modified-base. Tooltip `Collapse diff` / `Expand diff`.
  2. Change icon, filled with the type color (sprite ids `diffs-icon-symbol-modified`, `-added`, `-deleted`, `-moved`; extract the SVGs from `@pierre/diffs/dist/sprite.js`).
  3. For renames: the previous name at opacity 0.7, then `diffs-icon-arrow-right-short`.
  4. The title (full path). It truncates **from the left** (`direction: rtl` with ellipsis). Cursor pointer; text-decoration underline with a transparent color. On hover the color becomes `lab-mix(foreground 84%, primary)` and the underline becomes visible, with 120ms transitions. **Click opens `openFile(path)` as a file surface** (`DiffPanel.tsx:792-800`, `diffFileActions.ts`).
  5. Metadata, right-aligned, gap 1ch, mono 11px tabular. `-{deletions}` in deletion-base comes first, shown if deletions > 0 or additions == 0. Then `+{additions}` in addition-base, shown if additions > 0 or deletions == 0.

**Gutter.**
- **Unified mode has ONE line-number column.** It shows the addition line number when present, else the deletion line number (Pierre `renderers/DiffHunksRenderer.js:521`).
- Split mode has a number column on each side. Each half has a 1px `--diffs-bg` border on the inner edge, so the divider is 2px total.
- Number cell: right-aligned, padding left 2ch and right 1ch, min content width = digits of the file's largest line number (`--diffs-min-number-column-width-default`), color `fg-number` = `lab-mix(fg 65%, #0a0a0a / #fff)` (`#9d9d9d` dark, see the note on host-level variables below).
- The column's right border is 2px of `--diffs-bg`.
- Bars indicator: a 4px-wide strip at the left edge of the number cell.
  - Addition: solid addition-base.
  - Deletion: a dashed 2px pattern of deletion-base over bg-deletion.
- Code cell: `padding-inline: 1ch`.

**Hunk separator** (`line-info`):
- A 32px-tall row with bg `--diffs-bg-separator`; the separator content has radius 6px. Vertical margin 8px; none before the first hunk or after the last.
- Text `{N} unmodified line` / `{N} unmodified lines` in `fg-number`, sans, with 1ch padding.
- No expand buttons, because diffs are partial.

**Colors.** `--diffs-bg` is overridden to `mix(card 90%, background)`. Theme-derived values (`utils/getHighlighterThemeStyles.js`):

| | dark (`pierre-dark`) | light (`pierre-light`) |
|---|---|---|
| `fg` (plain text) | `#fafafa` | `#0a0a0a` |
| addition-base | `#07c480` | `#18a46c` |
| deletion-base | `#ff2e3f` | `#d52c36` |
| modified-base | `#009fff` | `#009fff` |

Line backgrounds are a lab-mix of the line's base bg (`--diffs-bg`, or the decoration bg) with a target. The percentage is the share of the base bg:

| Row | Target (T3 override, sRGB) | Code cell dark / light | Number cell dark / light | Hover code dark / light |
|---|---|---|---|---|
| context | `--diffs-bg` (no change) | n/a | n/a | lab-mix with `mix(background 94%, foreground)` at 91% / 97% |
| addition | `mix(background 92%, success)`; number cell target `mix(background 88%, success)` | 80% / 88% | 85% / 91% | 70% / 80% |
| deletion | `mix(background 92%, destructive)`; number cell target `mix(background 88%, destructive)` | 80% / 88% | 85% / 91% | 75% / 80% |
| separator | host-level: `lab-mix(#0a0a0a 85%, #fff)` dark (`#2b2b2b`), `lab-mix(#fff 96%, #000)` light; T3's override does not apply | n/a | n/a | n/a |
| annotation row | `--diffs-bg-context` = `mix(background 97%, foreground)` | n/a | n/a | n/a |

- Number text on addition and deletion rows uses addition-base or deletion-base respectively.
- Selected lines mix the row bg with modified-base: code cells 82% light / 75% dark; number cells 75% / 60%. Selected number fg = `lab-mix(modified-base 65%/75%, black/white)`.
- Source: `@pierre/diffs/dist/style.js`, one minified CSS string (base layer). Split it on `}` to read the rules quoted here.
- **Host-level variables ignore T3's overrides.** Pierre declares `--diffs-bg-separator`, `--diffs-fg-number`, `--diffs-bg-buffer`, `--diffs-bg-context(-gutter)`, `--diffs-bg-deletion` and the emphasis colors on the shadow host, where `--diffs-bg` is still the Pierre theme background (`#0a0a0a` / `#ffffff`) and T3's `*-override` variables (declared on `[data-diff]`, below the host) are not visible. Descendants inherit those computed values. Rules evaluated on line elements (line and number backgrounds, hover) do see T3's overrides. Measured: separator `#2b2b2b`, numbers `#9d9d9d`, addition number cell `#191d1c`, addition code `#191c1b`.

**Syntax highlighting.**
- Shiki, using the TextMate grammars, with themes `pierre-dark` and `pierre-light` from `@pierre/theme` 1.0.3 (MIT; `node_modules/.pnpm/@pierre+theme@1.0.3/.../themes/pierre-dark.json`, 248 tokenColor rules). It runs in a worker pool: size = clamp(floor(cores/2), 2, 6), AST LRU of 240, token transformer enabled.
- Token background is transparent. Lines longer than 1000 chars are left plain.

**Collapse.** Clicking the chevron toggles the file. A collapsed file shows only its header. State resets when the scope key changes.

**Virtualization.** Pierre `CodeView` virtualizes by file, using estimated metrics (line 20px, header 44px, spacing 8px). Use a GPUI `list` with variable item heights; one item per file, or per chunk for very large files.

### 2.7 Review comments (annotations) on diff lines

Sources: `AnnotatableCodeView.tsx`, `reviewCommentContext.ts`, `LocalCommentAnnotation.tsx`.

**Gesture.**
- Hovering a line shows the gutter utility button at the right end of the number cell: a 20x20 rounded-4 square, bg modified-base, fg `--diffs-bg`, with a plus icon.
- Clicking it, or click-dragging over line numbers, selects a range. On selection end, a **draft** annotation opens under the last selected line on that side. Side is `deletions` if the end side is deletions, else `additions`.
- While a draft is open, line selection and the gutter button are disabled (`:242-244`).

**Annotation row.** A full-width row with bg `mix(background 97%, foreground)` and 4px vertical padding. It holds one card per entry.

**Draft card** (`LocalCommentAnnotation.tsx:49-89`): `mx-12 my-8 rounded-xl border border/70 bg-background p-12 shadow-lg`.
- Header: MessageCircle 16px muted, then `Local comment` (`text-sm font-medium`).
- Subtext `Comment on lines {rangeLabel}` (`text-xs muted`, mt 4px).
- Textarea `sm`, autofocus, placeholder `Request change`, mt 12px.
- Footer, right-aligned with gap 8px: `Cancel` (ghost sm) and `Comment` (primary sm, disabled when the text is empty).
- Keys: Esc cancels; ⌘↩ submits when the text is non-empty.

**Saved card** (`:26-47`): same box with `shadow-sm`.
- Header: icon, `Local comment` (`text-xs font-medium`), then `ml-auto text-[11px] muted` `{rangeLabel}`, then a ghost `icon-xs` Trash2 button (aria `Delete comment`).
- Body: the text, `text-sm leading-relaxed whitespace-pre-wrap` at mt 8px.

**Storage.** Comments are stored in the **composer draft** of the current thread (`composerDraftStore.addReviewComment` / `removeReviewComment`), not on the server. The record is `ReviewCommentContext`:

```
{id, sectionId, sectionTitle, filePath, startIndex, endIndex, rangeLabel, text, diff, fenceLanguage?}
```

- `sectionId` is `turn:{turnId}`, `unstaged`, or `branch`. `sectionTitle` is `Turn {n}`, `Working tree`, or `Branch changes`.
- Indices are positions in the file's flattened review-line list.
- `diff` is a mini-hunk `@@ -a,b +c,d @@` followed by the marked lines (`buildDiffReviewComment`, `reviewCommentContext.ts:399-438`).
- `rangeLabel` is `+12`, `-12`, `12 to 15`, `+12 to +15`, `line`, or `N lines` (`formatDiffReviewRangeLabel`, `:379-397`).
- Saved comments re-render as annotations whenever the same section and file are shown.

**On send.** The composer appends each comment to the prompt as:

````
<review_comment sectionId="…" sectionTitle="…" filePath="…" startIndex="…" endIndex="…" rangeLabel="…">
{text}
```diff
{diff}
```
</review_comment>
````

The fence grows to (longest backtick run + 1), with a minimum of 3. Attributes are escaped with `&amp; &quot; &lt; &gt;`. See `reviewCommentContext.ts:178-215`. This serialization is shared with the composer spec.

### 2.8 Changed-files card in the timeline (`components/chat/ChangedFilesTree.tsx`)

**The live card is `AssistantChangedFilesSectionInner` in `MessagesTimeline.tsx:1710-1780`, not `ChangedFilesCard`** (which nothing renders). Its card is `mt-8 rounded-lg border border/80 bg-card/45 p-10`; the header row (`mb-6`, 24px) has a `text-[10px] uppercase tracking-[0.12em] muted-foreground/65` label `Changed files ({n})`, a `•`, and an aligned `DiffStatLabel` (uppercased, so `+1.5K`), then the two outline xs buttons (gap 6px). The tree gets `allDirectoriesExpanded` = the per-turn override from `uiStateStore` (undefined by default, so per-directory defaults apply), and tree rows measure 24.5px (11px names on the inherited 1.5 line height). The `ChangedFilesCard` description below is kept for reference.

Rendered under each assistant turn that changed files. It lives in the chat column, but it is the main way into the diff panel.

**Card** (`:53`): `relative mt-16 rounded-2xl bg-card/40 shadow-xs/5`, plus an inset 1px `input` border overlay (rounded-2xl = 18px).

**Header** (`:54-85`): sticky top, z10, mb 12px, `rounded-t-2xl bg-card/72 p-12 backdrop-blur-md`.
- Left: `{n} changed files` (`text-xs font-medium leading-4`), then an inline `DiffStatLabel`.
- Right: gap 6px, two outline `Button xs`:
  - `Collapse all` / `Expand all` toggles every directory.
  - `View diff` calls `onOpenTurnDiff(turnId, files[0].path)`.

**Tree** (`lib/turnDiffTree.ts`):
- Built from checkpoint `files[]`. Directories sort before files; both sort by name with numeric collation. Single-child directory chains compact to `a/b/c` (`:56-75`). Directory stats sum their children.
- A directory is expanded by default when it has 3 or fewer children (`CHANGED_FILES_AUTO_EXPAND_MAX_ITEMS`). "Expand all" overrides this per-directory default.

**Rows** (`:150-221`): `rounded-xl py-4 pr-12`, padding-left `8 + depth × 14` px. Hover `bg-accent/60`.
- Directory row: ChevronRight 14px (rotated 90° when open), Folder or FolderClosed 14px at `muted/75`, name `font-mono text-[11px] muted/90`, then right-aligned stats `font-mono text-[10px]`.
- File row: a 14px spacer when the tree has directories, then a `PierreEntryIcon` 14px, then the name `font-mono text-[11px] muted/80`. Click calls `onOpenTurnDiff(turnId, path)`.

**`DiffStatLabel`** (`DiffStatLabel.tsx`):
- `+{n}` in `success` and `-{n}` in `destructive`, mono, tabular.
- Aligned layout is a grid of 4ch and 4ch with gap 8px, right-aligned. Inline layout uses gap 4px.
- Counts are compact: `1.2k`, `12k`, `1.2m`, and so on.

### 2.9 Rust approach for the diff view

- Write a custom virtualized `DiffView` element:
  - A `list` with one item per file.
  - Each file: a header element plus rows painted as runs of `ShapedLine` (`window.text_system().shape_line`).
  - Precompute row heights (wrap mode needs a width-dependent layout; cache per width).
  - Sticky header: paint the current file's header at the top of the viewport in a later paint pass.
- **Highlighting, two options:**
  1. `syntect` with TextMate grammars and the **actual `pierre-dark.json` / `pierre-light.json` tokenColors** converted into a syntect `Theme`. Scopes map 1:1, so this is the closest to Shiki. Grammars can come from `two-face` or bat's set. **Recommended, for parity.**
  2. `gpui-component`'s tree-sitter `SyntaxHighlighter` (`gpui-component-0.7.0/src/highlighter`, with `tree-sitter-*` features). It is faster to integrate, but capture names do not map one-to-one onto TextMate scopes, so colors will drift.
- Highlight each file's old and new sides on a background executor and cache by `(path, diffHash, theme)`.
- Colors: precompute the mixed colors in 2.6 once per theme (implement `mix_srgb` and `mix_lab`).

---

## 3. Terminal drawer and terminal panel

### 3.1 Files

| File | Role |
|---|---|
| `components/ThreadTerminalDrawer.tsx` | drawer and panel UI, `TerminalViewport` (xterm wrapper) |
| `components/ChatView.tsx:551-906` | `PersistentThreadTerminalDrawer` (create, split, close wiring per thread) |
| `components/ChatView.tsx:908-1057` | `PersistentThreadTerminalPanel` (right-panel terminal surface) |
| `terminalUiStateStore.ts` | per-thread drawer state (persisted) |
| `terminal-links.ts` | URL and path link detection, path resolution |
| `components/preview/openTerminalLinkInPreview.ts` | URL link menu |
| `keybindings.ts:446-529` | terminal key intercepts |
| `lib/terminalContext.ts`, `lib/terminalFocus.ts` | "Add to chat" payload, focus owner |
| `packages/client-runtime/src/state/terminal.ts`, `terminalSession.ts` | RPC atoms and buffer reducer |
| `apps/web/src/state/terminalSessions.ts` | attach plus metadata join |
| `packages/shared/src/terminalLabels.ts` | `Terminal N` labels, `nextTerminalId` |

### 3.2 Drawer placement, size, animation

**Placement.** The drawer is the last child of the **chat column** (`ChatView.tsx:5393-5412`), below the messages/composer area, so it spans only the chat column width. It sits above nothing in the right panel.

**Box.** `aside.thread-terminal-drawer.workspace-terminal-drawer`: relative flex column, overflow hidden, bg `background`, `shrink-0`, `border-t border-border/80` (`ThreadTerminalDrawer.tsx:1257-1268`).

**Height.**
- Default 280px (`types.ts:23`), min 180px, max `max(180, floor(0.75 × windowHeight))` (`:71-84`).
- Stored per thread in `terminalUiStateStore.terminalHeight` and written when a drag ends.
- On window resize the height re-clamps and the terminal refits.

**Resize handle** (`:1269-1277`):
- Absolute, top 0, full width, 6px tall, z-20, `cursor: row-resize`, with no visual.
- Pointer capture. New height is `clamp(startHeight + (startY - y))`.
- On release, if the height changed: persist it and bump `resizeEpoch` to refit.

**Animation** (`index.css:189-211`):

| State | Height | Opacity | Pointer events |
|---|---|---|---|
| closed | 0, `overflow: clip` | 0 | none |
| open | H, 180ms `cubic-bezier(0.4,0,0.2,1)` | 1, 140ms ease-out with 30ms delay | auto |
| closing | H to 0 | 1 to 0, 100ms ease-in | n/a |

- No transition while resizing.
- Presence uses `openingFrameCount: 2` (`ChatView.tsx:1451-1454`). The `transitionend` event for `height` bumps `resizeEpoch` so xterm refits after the open animation (`:1099-1111`).
- **Persistence across thread switches:** the drawers of up to 10 inactive threads stay mounted and hidden (`MAX_HIDDEN_MOUNTED_TERMINAL_THREADS = 10`, `ChatView.logic.ts:25`), so scrollback survives. Natively, keep each terminal's `Term` in a model, independent of the view.

**Toggle** (`ChatView.tsx:2455-2497`):
- If opening with zero terminals: allocate `term-N`, call `ensureTerminal(open)`, and send `terminal.open {threadId, terminalId, cwd: gitCwd ?? workspaceRoot, worktreePath?, env}`.
- `env` is `projectScriptRuntimeEnv`: `{T3CODE_PROJECT_ROOT: workspaceRoot, T3CODE_WORKTREE_PATH?: worktreePath}` (`packages/shared/src/projectScripts.ts:20-33`).

### 3.3 Drawer state (`terminalUiStateStore.ts`)

**Shape.** Per thread: `{terminalOpen, terminalHeight, terminalIds[], activeTerminalId, terminalGroups: {id, terminalIds[], splitDirection?}[], activeTerminalGroupId}`. Persisted under `t3code:terminal-state:v1` at version 4 (`:30, :773`).

**Ids.** Terminal ids are always chosen by the client: `term-N`, the lowest unused N (`terminalLabels.ts:32-40`). Group ids are `group-{terminalId}`, deduplicated with a `-2`, `-3`… suffix.

**`upsertTerminalIntoGroups`** (`:254-348`):
- `new`: pushes a new group with the terminal and makes it active.
- `split`: inserts the new id right after the active terminal in the active group, and sets the group's `splitDirection` (`vertical`, or none for horizontal).
- **Max 4 terminals per group** (`MAX_TERMINALS_PER_GROUP`). A split beyond that is a no-op.
- `setThreadTerminalOpen(true)` with no terminals creates `term-1` (`:350-357`).

**`closeThreadTerminal`** (`:409-452`):
- If it was the last terminal, reset to the default state, which closes the drawer.
- Otherwise the next active terminal is the one at the same index, clamped. Empty groups are removed.

### 3.4 Drawer layouts (`ThreadTerminalDrawer.tsx:1217-1549`)

Terminals in the active group are rendered (`visibleTerminalIds`). The "sidebar" mode activates when more than one terminal exists in total, across all groups.

**Empty state** (0 terminals, `:1217-1253`): centered `gap-12 px-16 py-24 text-sm muted`.
- Text: `No terminal sessions for this thread yet.`
- A button: `rounded-md border border/80 bg-background px-12 py-6 text-xs font-medium`, hover `bg-accent`, label `New Terminal (⌘N)` (`New Terminal` when no shortcut is bound).

**One terminal, no sidebar:**
- A floating toolbar at absolute top 8px, right 8px, z-20: `inline-flex rounded-md border border/80 bg-background shadow-sm overflow-hidden`.
- Four buttons, each `p-4 text-foreground/90 hover:bg-accent` with a 13px icon, separated by 1×16px `bg-border/80` dividers:
  1. `SquareSplitHorizontal`, label `Split Terminal Horizontally (⌘D)`
  2. `SquareSplitVertical`, label `Split Terminal Vertically (⇧⌘D)`
  3. `Plus`, label `New Terminal (⌘N)`
  4. `Trash2`, label `Close Terminal (⌘W)`
- At the split limit, both split buttons get `cursor-not-allowed opacity-45` and labels `Split Terminal Horizontally (max 4 per group)` / `… Vertically (max 4 per group)`.
- Tooltips are hover popovers shown below, 6px offset, tooltip-styled.

**Viewport wrapper.** `h-full p-4`, containing the xterm host `relative h-full w-full overflow-hidden rounded-[4px] bg-background` (`:799-804`).

**Split view** (`:1328-1386`):
- A CSS grid with equal tracks. **`horizontal` means side-by-side columns; `vertical` means stacked rows.**
- Each pane is `min-h-0 min-w-0`. Columns get `border-l` and rows get `border-t`, except the first pane. The active pane uses `border`; others use `border/70`.
- Mousedown on an inactive pane activates it.
- Every pane wraps its viewport in `p-4`.

**Sidebar mode** (>1 terminal total):
- The main area and a right sidebar are separated by a 6px gap.
- Sidebar (`:1414-1546`): `w-144 min-w-144 flex-col border border/70 bg-muted/10`.
  - Header: 22px tall, `border-b border/70`, buttons right-aligned. Each is `h-full px-4 inline-flex items-center` with `border-l border/70` between them and `hover:bg-accent/70`. The same 4 actions with 13px icons.
  - List: `overflow-y-auto px-4 py-4`.
- Group headers show only when there is more than 1 group or any group holds more than 1 terminal. Header button: `w-full rounded px-4 py-2 text-[10px] uppercase tracking-[0.08em]`, text `Group {i}`. The active group gets `bg-accent/70 text-foreground`; others are `muted` with hover `bg-accent/50`. Clicking an inactive group's header activates its first terminal; clicking the active group's header keeps the current terminal.
- Group body when headers show: `ml-4 border-l border/60 pl-6`.
- Terminal row: `group flex items-center gap-4 rounded px-4 py-2 text-[11px]`. Active: `bg-accent text-foreground`. Otherwise `muted` with hover `bg-accent/50`.
  - With headers: a `└` prefix (`text-[10px] muted/80`).
  - Then a button with `TerminalSquare` 12px and the label, truncated.
  - If more than 1 terminal: a close button, 14x14, `opacity-0` until group hover, with an X icon at 10px. Label `Close {label}`, plus ` (⌘W)` on the active row.
- Labels come from the server `TerminalSummary.label` (for example the running command), else `Terminal N`.

**Panel mode** (a terminal surface in the right panel): same component with `mode="panel"`. It is `h-full flex-1` with no border, no resize handle, and no transition. `terminalGroups` contains only the surface itself (`ChatView.tsx:1032-1039`).

### 3.5 xterm configuration (`ThreadTerminalDrawer.tsx:389-398`) and theme (`:128-200`)

- `@xterm/xterm` 6.0.0 (MIT), DOM renderer (no WebGL addon), `@xterm/addon-fit` 0.11.
- Options: `cursorBlink: true`, `lineHeight: 1`, `fontSize: 12`, `scrollback: 5000`. Cursor style is block (default); when unfocused it draws as an outline. `macOptionIsMeta` is false (default).
- `fontFamily`: `"SF Mono", "SFMono-Regular", "JetBrains Mono", Consolas, "Liberation Mono", Menlo, monospace`.
- Theme `background` and `foreground` are read from the drawer's computed styles, i.e. the `background` and `foreground` tokens. Fallbacks: dark `rgb(14,18,24)` / `rgb(237,241,247)`; light white / `rgb(28,33,41)`.
- The theme is re-read whenever the `class` or `style` of `<html>` changes (`:668-677`).

| key | dark | light |
|---|---|---|
| cursor | rgb(180,203,255) | rgb(38,56,78) |
| selectionBackground | rgba(180,203,255,0.25) | rgba(37,63,99,0.2) |
| scrollbar slider / hover / active | rgba(255,255,255,.10/.18/.22) | rgba(0,0,0,.15/.25/.30) |
| black | rgb(24,30,38) | rgb(44,53,66) |
| red | rgb(255,122,142) | rgb(191,70,87) |
| green | rgb(134,231,149) | rgb(60,126,86) |
| yellow | rgb(244,205,114) | rgb(146,112,35) |
| blue | rgb(137,190,255) | rgb(72,102,163) |
| magenta | rgb(208,176,255) | rgb(132,86,149) |
| cyan | rgb(124,232,237) | rgb(53,127,141) |
| white | rgb(210,218,230) | rgb(210,215,223) |
| brightBlack | rgb(110,120,136) | rgb(112,123,140) |
| brightRed | rgb(255,168,180) | rgb(212,95,112) |
| brightGreen | rgb(176,245,186) | rgb(85,148,111) |
| brightYellow | rgb(255,224,149) | rgb(173,133,45) |
| brightBlue | rgb(174,210,255) | rgb(91,124,194) |
| brightMagenta | rgb(229,203,255) | rgb(153,107,172) |
| brightCyan | rgb(167,244,247) | rgb(70,149,164) |
| brightWhite | rgb(244,247,252) | rgb(236,240,246) |

The vertical scrollbar is 6px wide with a 3px radius (`index.css:670-681`).

### 3.6 Behaviors (`TerminalViewport`, `ThreadTerminalDrawer.tsx:296-805`)

**Input.**
- Every `onData` string is sent with `terminal.write {threadId, terminalId, data}`. Data must be non-empty and at most 65,536 chars.
- On failure, the terminal prints `\r\n[terminal] {message}\r\n`, with fallback `Terminal write failed`.

**Key intercepts** (`:501-536`), evaluated before xterm sees the key:
1. These app shortcuts are **not** consumed by the terminal, so they bubble to the app: `terminal.toggle`, `terminal.split`, `terminal.splitVertical`, `terminal.new`, `terminal.close`, `diff.toggle`. Context is `terminalFocus = true`.
2. Option+←/→ (macOS) sends `ESC b` / `ESC f` (word back/forward). Cmd+←/→ sends `^A` / `^E` (`keybindings.ts:492-529`, constants at `:47-51`).
3. Cmd+Backspace (macOS) sends `^U` (`\u0015`).
4. Ctrl+L, or Cmd+K on macOS, sends `\u000c` (form feed). This is a clear request to the shell, not RPC `terminal.clear`.

**Links** (`terminal-links.ts`, provider at `ThreadTerminalDrawer.tsx:538-620`).

Detection runs on the logical line, which joins soft-wrapped rows (`collectWrappedTerminalLinkLine`, `terminal-links.ts:175-220`). Patterns:
- URL: `/https?:\/\/[^\s"'`<>]+/g`
- Path: `/(?:~\/|\.{1,2}\/|\/|[A-Za-z]:[\\/]|\\\\)[^\s"'`<>]+|[A-Za-z0-9._-]+(?:\/[A-Za-z0-9._-]+)+(?::\d+){0,2}/g`

Matching rules:
- Trailing `.,;!?` is trimmed, and unbalanced closing `)]}` are trimmed.
- URLs take priority; a path that overlaps a URL is dropped.

Activation:
- **Only Cmd+click on macOS** (Ctrl+click elsewhere) activates a link (`isTerminalLinkActivation`). Plain hover shows the xterm default underline.
- **URL click:** if previewable, show the native menu `Open in preview` / `Open in browser` at the cursor.
  - Open in preview: `preview.open {threadId, url}`, apply the snapshot, then `openBrowser(tabId)`.
  - Open in browser: `shell.openExternal`.
  - When preview is unsupported, open the external browser directly (`openTerminalLinkInPreview.ts:44-109`).
- **Path click:** resolve against `cwd`, preserving `:line:col` (`resolvePathLinkTarget`, `terminal-links.ts:269-286`).
  - `~/` resolves to the home directory inferred from cwd (`/Users/x`, `/home/x`).
  - Relative paths are joined with cwd.
  - Then call `shell.openInEditor {cwd: target, editor: preferred}`. On failure print `[terminal] Unable to open path`.

**Selection and "Add to chat"** (`:412-489, 636-666`):
- After a left-button mouseup with a non-empty selection, after a delay (0ms, or 260ms for double/triple click), a native context menu appears with one item, `Add to chat`.
- Position: at the selection's last client rect (right, bottom + 4), else at the pointer, clamped to the viewport with an 8px margin.
- Choosing it calls `onAddTerminalContext({terminalId, terminalLabel, lineStart: selection.start.y+1, lineEnd, text})`. The text has CRLF normalized and leading/trailing newlines trimmed. The selection is then cleared and the terminal refocused.
- Pointerdown cancels a pending menu.

**Output application** (`:712-769`):
- The session state holds `{buffer, status, error, version}`.
- On a version bump: if the new buffer extends the old one, write only the suffix; otherwise write `ESC c` (RIS) followed by the full buffer.
- New errors print `[terminal] {error}`.
- When status becomes `exited` or `closed`, print `[terminal] Process exited` or `[terminal] Terminal closed`, then **close the terminal on the next tick** with `onCloseTerminal`, which sends RPC `terminal.close {deleteHistory:true}` and, on failure, falls back to writing `exit\n` (`ChatView.tsx:805-838`). **A tab disappears when its shell exits.**

**Fit and resize:**
- 30ms after mount, and on every change to drawer height, `resizeEpoch`, the thread, or the terminal, run `fit()`.
- If the view was scrolled to the bottom, keep it there.
- Then send `terminal.resize {threadId, terminalId, cols, rows}`. The client coalesces to the latest per session (`terminal.ts:68-73`).

**Focus.** The active terminal autofocuses on mount and on every `focusRequestId` bump. Focus owner is `"drawer"` or `"right-panel"` (`data-terminal-owner`). It routes `terminal.*` shortcuts to the drawer or the panel surface (`ChatView.tsx:3806-3858`).

### 3.7 Terminal RPCs (upstream `packages/contracts/src/terminal.ts`, `rpc.ts:1169-1206, 1386-1398`)

| Tag | Kind | Payload | Success |
|---|---|---|---|
| `terminal.open` | unary | `{threadId, terminalId (≤128), cwd, worktreePath?: string\|null, cols?: 1..1000, rows?: 1..500, env?: Record<[A-Za-z_][A-Za-z0-9_]*, ≤8192 chars> (≤128 keys), providerInstanceId?}` | `TerminalSessionSnapshot` |
| `terminal.attach` | **stream** | `{threadId, terminalId, cwd?, worktreePath?, cols?, rows?, env?, providerInstanceId?, restartIfNotRunning?}`. Opens the session if it is missing and `cwd` is given (server `Manager.ts:2680-2724`). | `TerminalAttachStreamEvent` |
| `terminal.write` | unary | `{threadId, terminalId, data: 1..65536 chars}` | void |
| `terminal.resize` | unary | `{threadId, terminalId, cols, rows}` | void |
| `terminal.clear` | unary | `{threadId, terminalId}` | void (unused by the drawer) |
| `terminal.restart` | unary | `{threadId, terminalId, cwd, worktreePath?, cols, rows, env?, providerInstanceId?}` | `TerminalSessionSnapshot` (unused by the drawer) |
| `terminal.close` | unary | `{threadId, terminalId?, deleteHistory?}` | void |
| `subscribeTerminalMetadata` | stream | `{}` | `{type:"snapshot", terminals: TerminalSummary[]}` \| `{type:"upsert", terminal}` \| `{type:"remove", threadId, terminalId}` |
| `subscribeTerminalEvents` | stream | `{}` | `TerminalEvent` (all sessions; unused by the drawer UI) |

`providerInstanceId` is upstream-only; the fork omits it.

Shapes:

```
TerminalSessionSnapshot = {threadId, terminalId, cwd, worktreePath|null, status: "starting"|"running"|"exited"|"error",
                           pid|null, history: string, exitCode|null, exitSignal|null, label (≤128), updatedAt, sequence?}
TerminalSummary        = {threadId, terminalId, cwd, worktreePath|null, status, pid|null, exitCode|null, exitSignal|null,
                           hasRunningSubprocess: boolean, label, updatedAt}
TerminalAttachStreamEvent = {type:"snapshot", snapshot}
  | {type:"output", data: string} | {type:"exited", exitCode|null, exitSignal|null} | {type:"closed"}
  | {type:"error", message} | {type:"cleared"} | {type:"restarted", snapshot}
  | {type:"activity", hasRunningSubprocess, label}
```

Every event also carries `threadId`, `terminalId`, and `sequence?`.

**Encoding.** `data` and `history` are JSON strings, UTF-8 decoded on the server from node-pty output. Feed `data.as_bytes()` straight into the VT parser. The server sets `COLORTERM=truecolor` (`Manager.ts:1323-1324`).

**Server limits.** History is capped at 5,000 lines and 8 MB, with chunks of at most 16 KB. Default spawn size is 120×30 (`Manager.ts:93-102`). Scripts open at 120×30 (`ChatView.tsx:348-349`).

**Client reducer** (`terminalSession.ts:125-173`):

| Event | Effect |
|---|---|
| `snapshot`, `restarted` | buffer = `history`, trimmed to its last **512 KB** on a UTF-8 boundary; status from the snapshot; version 1 |
| `output` | append and trim; status `closed` becomes `running` |
| `cleared` | buffer = "" |
| `exited` | status `exited` |
| `closed` | status `closed` |
| `error` | status `error`, error = message |
| `activity` | no buffer change |

Labels and `hasRunningSubprocess` come from the metadata stream.

### 3.8 Rust approach

- **Emulator:** `alacritty_terminal` (Apache-2.0). Run one `Term<EventProxy>` per `(thread, terminalId)` in a model entity, and feed bytes through `vte::ansi::Processor`. **Do not copy Zed's `terminal` or `terminal_view` crates (GPL-3.0)**; studying their public design is fine, copying code is not.
- **Reducer:**
  - On `snapshot` or `restarted`: replace the `Term` (or `reset_state`) and feed the trimmed history.
  - On `output`: feed the data.
  - On `cleared`: reset.
  - Keep scrollback at 5000 (`Config { scrolling_history: 5000 }`).
- **Renderer:** a custom GPUI `Element`.
  - Measure the cell from the chosen mono font at 12px with line-height 1.0. xterm measures a run of `W` characters: cell width is the advance of `W`, and cell height is the font's line box (ascent + descent) × 1.0, floored.
  - Group cells into runs by style and shape them with `shape_line`. Paint backgrounds as quads, then text, then the cursor (block; hollow when unfocused; 600ms blink).
  - Paint the selection with the theme colors above. Draw the scrollbar overlay (6px).
- **Input encoding:** `alacritty_terminal` does not encode keys. Implement key-to-bytes mapping, respecting `TermMode::APP_CURSOR`, `APP_KEYPAD`, `BRACKETED_PASTE`, and the kitty flags if you want them.
  - Reference with attribution: `alacritty` (Apache-2.0, `alacritty/src/input/keyboard.rs`) or `termwiz` (MIT).
  - Apply the intercepts in 3.6 first.
  - Mouse reporting: support SGR 1006 and the 1000/1002/1003 modes when `TermMode::MOUSE_*` is set. Shift bypasses reporting so the user can select.
- **Fit** (matches `@xterm/addon-fit` 0.11): cols = max(2, floor((paneW − 8 − 14) / cell_w)) and rows = max(1, floor((paneH − 8) / cell_h)). The 8 comes from the `p-4` wrapper (4px each side). The 14 is FitAddon's fixed scrollbar reserve, applied whenever scrollback > 0, even though the visible scrollbar is 6px. Call `term.resize` and send `terminal.resize` debounced to the latest value. Refit after the drawer animation ends.
- **Links:** scan the rows visible under the mouse with the regexes above. Join rows that have the `WRAPLINE` flag. Underline on hover with Cmd held.
- **Selection:** use `alacritty_terminal::selection` (semantic or line on double/triple click). Copy with Cmd+C. Show the "Add to chat" native menu (`gpui` context menu or NSMenu) on mouseup.
- **Effort:** 3-5 engineer-weeks for parity, mostly in key/mouse encoding, IME, and rendering performance.

---

## 4. Plan, files, scripts, Open-in, Git actions, PR dialog

### 4.1 Plan surface (`components/PlanSidebar.tsx`, `proposedPlan.ts`, `session-logic.ts:597-704`)

It renders as the `plan` right-panel surface with `mode="embedded"` (`ChatView.tsx:5081-5092`): `flex min-h-0 flex-col bg-card/50 h-full w-full`. The legacy `sidebar` mode is 340px wide with `border-l border/70`.

**Header** (`:148-193`): 48px tall, `border-b border/60 px-12`, space-between.
- Left (gap 8px):
  - A badge: `Badge info sm`, `rounded-md px-6 py-0 font-semibold uppercase tracking-wide`, 16px tall, 10px text, bg `info/16` in dark or `info/8` in light, text `info-foreground`.
  - The badge text is the label: `Plan` if there is a proposed plan or interaction mode is plan, else `Tasks` (`ChatView.tsx:1962`).
  - Then `formatTimestamp(activePlan.createdAt)` (`text-[11px] muted/60 tabular-nums`).
- Right: when a plan markdown exists, a ghost `icon-xs` button `EllipsisIcon` 14px (`muted/50`, hover `foreground/70`, aria `Plan actions`) opens an end-aligned menu:
  - `Copy to clipboard` (shows `Copied!` once done)
  - `Download as markdown` (save dialog natively; filename below)
  - `Save to workspace` (disabled with no workspace or while saving). RPC `projects.writeFile {cwd: workspaceRoot, relativePath: filename, contents}`, which returns `{relativePath}`. Toasts: `Plan saved` with the path, or `Could not save plan`.

**Content** (`:196-278`): a ScrollArea with `p-12`, sections 16px apart.
- **Explanation:** `text-[13px] leading-relaxed muted/80`.
- **Steps:** a heading `Steps` (`text-[10px] font-semibold tracking-widest muted/40 uppercase`, mb 8px), then rows `flex items-center gap-10 rounded-lg px-10 py-8` with 200ms color transitions.
  - inProgress row bg `blue-500/5`; completed row bg `emerald-500/5`.
  - Icon (20px circle):
    - completed: `bg-success/10 text-success-foreground` with Check 12px
    - inProgress: `bg-primary/10 text-primary` with a spinning Loader 12px
    - pending: `border border/60 bg-muted/30` with a 6px `muted-foreground/30` dot
  - Text `13px leading-snug`: completed is `muted/50` with `line-through` (decoration `muted/20`); inProgress is `foreground/90`; pending is `muted/70`.
- **Proposed plan:** a disclosure button with a Chevron 12px at `muted/40` and the title (first markdown heading, else `Full Plan`) in the same 10px uppercase style, hover `muted/60`. Collapsed by default. When expanded, a card `rounded-lg border border/50 bg-background/50 p-12` renders the markdown via ChatMarkdown with the leading heading and a `Summary` heading stripped (`proposedPlan.ts:6-20`).
- **Empty state:** centered `py-48`, `No active plan yet.` (`13px muted/40`) and `Plans will appear here when generated.` (`11px muted/30`, mt 4px).

**Data:**
- `activePlan` is derived from the latest `turn.plan.updated` activity. The current turn is preferred, falling back to the last activity. Payload: `{plan: [{step, status: "pending"|"inProgress"|"completed"}], explanation?}`.
- The proposed plan comes from `thread.proposedPlans` (latest by `updatedAt`, preferring the latest turn), or from the source thread's plan while a turn implementing it is unsettled.

**Download filename:** the plan title lowercased, with quotes and punctuation stripped, non-alphanumerics replaced by `-`, and leading/trailing dashes trimmed, plus `.md`. Fallback `plan.md`. The contents gain a trailing newline (`proposedPlan.ts:64-110`).

### 4.2 Files surface (`components/files/FilePreviewPanel.tsx`, `FileBrowserPanel.tsx`)

The `files` surface (no file selected) shows the browser full-width. A `file:{path}` surface shows the editor plus an optional explorer.

**Subheader** (`:711-819`), only when a file is open: `.surface-subheader` (40px) with `gap-8 px-12`.
- **Breadcrumbs:** a horizontally scrolling area with hidden scrollbars, an edge fade, and `text-xs`. Crumbs are `projectName / dir / … / file`, separated by ChevronRight 14px (`muted/60`, mx 4px). Each crumb is truncated at 160px. Directories are `muted-foreground`; the file is `font-medium foreground`. The current crumb is scrolled into view (inline end).
- **OpenInPicker, compact** (4.4), only for the primary environment. It opens the absolute file path.
- **Markdown toggle** (`.md`/`.mdx` only): ghost Toggle sm. Eye icon with label `Show rendered markdown`, or Code2 with `Show markdown source`.
- **Open in preview** (html-like files, desktop): Globe2, `Open file in preview browser` (5.x).
- **Explorer toggle:** FolderTree, `Hide file explorer` / `Show file explorer`. Persisted under the localStorage key `t3code.fileExplorerOpen` (default true).

**Truncation banner:** `border-b border-amber-500/20 bg-amber-500/8 px-12 py-6 text-[11px] text-amber-700 dark:text-amber-300`, text `Preview limited to the first 1 MB of a {bytes} byte file.`

**Body** (`:826-912`): a row containing the editor (flex-1) and, when the explorer is open, an `aside` with width `min(352px, 46%)`, min 256px, `border-l border/60`.

Editor states:
- Error: `text-xs leading-relaxed text-destructive`, centered.
- Loading: a spinning LoaderCircle at 20px, muted.
- Truncated file: read-only Pierre `File` with no header and the same theme.
- Otherwise: an **editable** Pierre `File` with `contentEditable`, line selection, and the gutter "+" for local comments. Comment cards are the same as 2.7, with `rangeLabel` `L{n}` or `L{a} to L{b}`.
  - Saving is debounced by **500ms** (`FileSaveCoordinator`). A save in flight blocks; the coordinator always persists the latest revision. RPC `projects.writeFile {cwd, relativePath, contents}`.
  - The tab shows the pending dot (1.7) while a save is pending.
- Markdown rendered view: ChatMarkdown with `max-w-4xl px-24 py-20`, centered. Checking a task-list box rewrites `[ ]` / `[x]` at the marker offset and saves.

**Reveal line:** when a surface opens with `revealLine` (for example from a markdown link `path:42`), the line is centered in the viewport. That line and its number cell are tinted with the selection colors (82%/75% and 75%/60% mixes with modified-base) (`:82-113, 145-242`).

**File RPCs** (upstream `project.ts`):
- `projects.readFile {cwd, relativePath (≤ max)}` returns `{relativePath, contents, byteLength, truncated}`. Query stale time 30s, idle TTL 5min.
- Errors are `ProjectReadFileError {message, failure?: "workspace_path_outside_root"|"resolved_path_outside_root"|"path_not_file"|"binary_file"|"operation_failed", …}`.

**Browser panel** (`FileBrowserPanel.tsx`):
- Header: 36px, `border-b border/60 px-12`, gap 8px.
  - Left: project name (`text-xs font-medium`), and below it `{n} files` (`text-[10px] leading-none muted`). The count reads `Indexing…` while pending, with a ` · partial` suffix when truncated.
  - Right: ghost icon buttons (`p-6 rounded-md`, hover `bg-accent`), Search 14px (`Search workspace files`) and RefreshCw 14px (`Refresh workspace files`, spins while pending).
- Tree: `@pierre/trees` 1.0.0-beta.4 (Apache-2.0).
  - Compact density, 12px sans, items rounded 5px, transparent bg.
  - Selected bg is `currentColor` at 12%; hover at 7%.
  - Empty single-child directories are flattened. Initial expansion is 1 level.
  - Search mode hides non-matches.
- Selecting a file calls `openFile(path)`.
- RPC `projects.listEntries {cwd}` returns `{entries: [{path, kind: "file"|"directory", ignored?}], truncated}`. This is the recursive indexed listing. Upstream also accepts `directoryPath` for lazy children; the fork never sends it.

**File-type icons:** `pierre-icons.ts` plus `components/chat/PierreEntryIcon.tsx`. They are sprite symbols with a light/dark color table (`PierreEntryIcon.tsx:7-60`); for example, the `default` color is `#84848a` / `#adadb1`. Export the SVG symbols at build time as assets.

### 4.3 Project scripts (`components/ProjectScriptsControl.tsx`, `projectScripts.ts`)

**Placement.** First item in the chat header's action cluster (`ChatHeader.tsx:109-119`). Cluster gap is 8px, or 12px when the header container is ≥ 768px (`@3xl/header-actions`).

**Primary button** (`:252-271`):
- Primary script: the last-invoked script for the project (localStorage key `LAST_INVOKED_SCRIPT_BY_PROJECT_KEY`, `ChatView.tsx:1239`), else the first non-setup script, else the first script.
- It is a joined `Group` of two parts:
  1. An outline `Button xs` with the script icon (14px). The name is screen-reader-only below a 768px header width; at or above it, the name shows visibly with ml 2px. aria and tooltip: `Run {name}`.
  2. A `GroupSeparator`, visible only at ≥768px, then an outline `icon-xs` ChevronDown 16px (`Script actions`). It opens an end-aligned menu:
     - One item per script: icon 16px, then the name (`{name} (setup)` for `runOnWorktreeCreate`), then a right slot 24px tall with min width 24px. The slot shows the shortcut label, which fades out on hover, and a ghost `icon-xs` Settings button (`Edit {name}`) that fades in on hover and opens the edit dialog.
     - Last item: `Add action` with Plus 16px.
- **No scripts:** a single outline `Button xs` with Plus and the label `Add action`.

**Icons** (`ProjectScriptIcon`): `play`→Play, `test`→FlaskConical, `lint`→ListChecks, `configure`→Wrench, `build`→Hammer, `debug`→Bug.

**Dialog** (`:346-508`): title `Add Action` / `Edit Action`. Description: `Actions are project-scoped commands you can run from the top bar or keybindings.` Form fields, 16px apart:
- **Name:** an icon-picker button (outline, 36x36, icon 18px) and an input (placeholder `Test`, autofocus). The picker is a popover with a 3-column grid of tiles: `rounded-md border px-8 py-8 text-xs`, 16px icon plus label. The selected tile is `border-primary/70 bg-primary/10`.
- **Keybinding:** a read-only input (placeholder `Press shortcut`) that captures the next key combination. Backspace or Delete clears it. Help text: `Press a shortcut. Use Backspace to clear.`
- **Command:** a textarea (placeholder `bun test`).
- **Preview URL (optional):** an input (placeholder `http://localhost:5173`). Help text: `Open this URL in the in-app preview when this action runs.`
- Switch row `Run automatically on worktree creation`: `rounded-md border border/70 px-12 py-8 text-sm`.
- Switch row `Open preview automatically when this action runs`: disabled, at 60% opacity, without a preview URL.
- Validation messages (`text-sm text-destructive`): `Name is required.`, `Command is required.`, or a keybinding error.
- Footer: when editing, `Delete` (destructive-outline, `mr-auto`), then `Cancel` (outline), then `Save changes` / `Save action` (primary).
- Delete confirmation (AlertDialog): `Delete action "{name}"?`, `This action cannot be undone.`, `Cancel` and `Delete action` (destructive).

**Save** (`ChatView.tsx:2730-2870`, `persistProjectScripts`, `saveProjectScript`, `updateProjectScript`, `deleteProjectScript`):
- New id: the name lowercased, non-alphanumerics replaced by `-`, max length `MAX_SCRIPT_ID_LENGTH`, with `-2`, `-3` suffixes on collision (`projectScripts.ts:10-57`).
- Only one script may have `runOnWorktreeCreate`. Setting it clears the flag on the others.
- RPC `orchestration.dispatchCommand {type:"project.meta.update", commandId, projectId, scripts}` (`orchestration.ts:1097-1110`). Then, if a keybinding was given, `server.upsertKeybinding {key, command: "script.{id}.run"}`.
- Fork quirk: `saveProjectScript` / `updateProjectScript` do **not** copy `previewUrl` / `autoOpenPreview` into the persisted script (`ChatView.tsx:2780-2786, 2820-2826`). Match that, or fix it deliberately.

**Run** (`ChatView.tsx:2611-2725`):
1. Remember the script as last-invoked.
2. Open the drawer and focus it.
3. Pick the target terminal: the active drawer terminal, or a new `term-N` if that terminal has a running subprocess.
4. `terminal.open {threadId, terminalId, cwd: gitCwd ?? workspaceRoot, worktreePath?, env: {T3CODE_PROJECT_ROOT, T3CODE_WORKTREE_PATH?}, cols:120, rows:30 (new terminal only)}`.
5. `terminal.write {data: command + "\r"}`.
6. Failures set the thread error: `Failed to run script "{name}".`

Script keybindings dispatch through the same handler (`ChatView.tsx:3874-3880`).

### 4.4 Open-in picker (`components/chat/OpenInPicker.tsx`, `editorPreferences.ts`)

**Shown** in the chat header only when the project has a name and the thread's environment is the primary (local) one (`ChatHeader.tsx:42-52`). It is also used in compact form in the file subheader.

**Appearance:** a joined `Group`.
1. An outline `Button xs` with the preferred editor's icon (14px). The label `Open` is visible only at ≥768px; compact mode is screen-reader-only and uses aria `Open file in preferred editor`. Disabled when there is no preferred editor or no path.
2. A separator (≥768px only), then an outline `icon-xs` ChevronDown 16px. aria is `Copy options`, or `Choose editor` in compact mode.

**Menu** (end-aligned):
- One item per available editor: a muted icon, the label, and the shortcut `MenuShortcut` (`⌘O`, from `editor.openFavorite`) on the preferred one.
- If no editors are installed: one disabled item, `No installed editors found`.

Options, in order (`:38-152`): Cursor, Trae, Kiro, VS Code, VS Code Insiders, VSCodium, Zed, Antigravity, IntelliJ IDEA, Aqua, CLion, DataGrip, DataSpell, GoLand, PhpStorm, PyCharm, Rider, RubyMine, RustRover, WebStorm, and the file manager (labelled `Finder` on macOS, with a FolderClosed icon). The list is filtered to `serverConfig.availableEditors`. Brand icons come from `components/Icons.tsx` and `JetBrainsIcons.tsx`; export them as SVG.

**Preferred editor:** localStorage key `t3code:last-editor` if it is available, else the first available editor in `EDITORS` order (`editorPreferences.ts:41-61`). Choosing an editor sets it as preferred.

**Behavior.** RPC `shell.openInEditor {cwd: path, editor: EditorId, reveal?}`; `reveal` is upstream-only, and only meaningful for `file-manager` when the server reports `shellRevealInFileManager`. The shortcut ⌘O (`editor.openFavorite`) opens the preferred editor from anywhere when the picker is mounted with `enableShortcut`.

`EditorId` (upstream `editor.ts`): `cursor`, `trae`, `kiro`, `vscode`, `vscode-insiders`, `vscodium`, `zed`, `antigravity`, `idea`, `aqua`, `clion`, `datagrip`, `dataspell`, `goland`, `phpstorm`, `pycharm`, `rider`, `rubymine`, `rustrover`, `webstorm`, `file-manager`.

### 4.5 Git actions (`components/GitActionsControl.tsx`, `GitActionsControl.logic.ts`)

The web `.logic.ts` file is authoritative. `packages/client-runtime/src/state/gitActions.ts` is an older copy with GitHub-only strings, used by mobile.

**Placement.** Last item in the chat header actions; it needs a project name and a git cwd.

**Status:**
- Comes from `subscribeVcsStatus {cwd}`, a stream of `VcsStatusStreamEvent` values merged into `VcsStatusResult` (6.3).
- `vcs.refreshStatus {cwd}` runs on window focus or visibility, debounced 250ms (`:1180-1211`), and whenever the menu opens.

**Not a repo:** an outline `Button xs` with GitBranchPlus 14px and the label `Initialize Git` (`Initializing...` while pending). RPC `vcs.init {cwd}`. Failure toast: `Git initialization failed`.

**Repo:** a joined Group.
1. **Quick action** (`resolveQuickAction`, `.logic.ts:167-310`): an outline `Button xs` with an icon and the label, which is screen-reader-only below the 768px header width. When disabled it is a hover popover (bottom-start) with the hint, `opacity-64 cursor-not-allowed`.

   | Condition (first match) | Label | Kind / action | Hint |
   |---|---|---|---|
   | busy | Commit | hint | `Git action in progress.` |
   | no status | Commit | hint | `Git status is unavailable.` |
   | detached (refName null) | Commit | hint | `Create and checkout a ref before pushing or opening a {singular}.` |
   | changes, no upstream, no primary remote | Commit | run `commit` | n/a |
   | changes and (open PR or default ref) | Commit & push | run `commit_push` | n/a |
   | changes | `Commit, push & {PR}` | run `commit_push_pr` | n/a |
   | no upstream, no primary remote | `View {PR}` if open PR and not ahead, else `Publish repository` | open_pr / open_publish | n/a |
   | no upstream, not ahead | `View {PR}` if open PR, else Push (disabled) | hint | `No local commits to push.` |
   | no upstream, ahead, open PR or default ref | Push | run `commit_push` if default ref, else `push` | n/a |
   | no upstream, ahead | `Push & create {PR}` | run `create_pr` | n/a |
   | diverged | Sync ref | hint | `Branch has diverged from upstream. Rebase/merge first.` |
   | behind | Pull | run_pull (`vcs.pull`) | n/a |
   | ahead, open PR or default ref | Push | as above | n/a |
   | ahead | `Push & create {PR}` | `create_pr` | n/a |
   | open PR and upstream | `View {PR}` | open_pr | n/a |
   | default-branch delta, not default ref | `Create {PR}` | `create_pr` | n/a |
   | else | Commit | hint | `Branch is up to date. No action needed.` |

   Quick-action icons: open_pr uses the provider icon; publish uses CloudUpload; pull uses Info; commit uses GitCommit; push and commit_push use CloudUpload; other runs use the provider icon; hints use GitCommit when the label is "Commit", else Info.
2. **Menu trigger:** an outline `icon-xs` ChevronDown 16px (`Git action options`), disabled while busy. The menu is end-aligned:
   - Items from `buildMenuItems` (`.logic.ts:94-165`): `Commit` (GitCommit), `Push` (CloudUpload), then either `View {PR}` or `Create {PR}` (provider icon). If there is no primary remote, only Commit appears.
   - Disabled items wrap in a hover popover (left side) with the reason (`GitActionsControl.tsx:260-327`), for example `Worktree is clean. Make changes before committing.`, `No local commits to push.`, `Commit local changes before creating a {singular}.`
   - `Publish repository...` appears when the repo has no primary remote.
   - Warnings (`px-8 py-6 text-xs text-warning`): `Detached HEAD: create and checkout a refName to enable push and pull request actions.` and `Behind upstream. Pull/rebase first.`
   - The status error, if any, in `text-destructive`.

**Terminology** (`packages/shared/src/sourceControl.ts`): `{PR}` / `{singular}` is `PR`/`pull request` for GitHub, Azure DevOps, Bitbucket, Forgejo (upstream), and the default (no provider). It is `MR`/`merge request` for GitLab, and `change request` for `unknown`. Provider icons: GitHub, GitLab, AzureDevOps, Bitbucket, or the lucide `GitPullRequest` generic.

**Running an action** (`:1245-1478`):
- **Default-ref confirmation:** when on the default ref and the action is push, create_pr, commit_push, or commit_push_pr, show a dialog first (`max-w-xl`):
  - Title and description from `resolveDefaultBranchActionDialogCopy` (`.logic.ts:325-362`), for example `Commit & push to default ref?` / `This action will commit and push changes on "{branch}". You can continue on this ref or create a feature ref and run the same action there.`
  - Buttons: `Abort` (outline, mr-auto), `{continueLabel}` (outline), `Checkout feature branch & continue` (primary), which sets `featureBranch: true`.
- **Progress toast:** a loading toast with no timeout. Its title is the first stage from `buildGitActionProgressStages`: `Preparing feature ref...`, `Generating commit message...`, `Committing...`, `Pushing...`, `Preparing {PR}...`, `Generating {PR} content...`, `Creating {singular}...`. Its description is `Waiting for Git...`.
- Stream events update the toast:
  - `phase_started`: title = label
  - `hook_started`: `Running {hook}...`
  - `hook_output`: description = last line
  - Otherwise the description is `Running for {s}s` / `Running for {m}m {s}s`, refreshed every 1s.
- **RPC `git.runStackedAction`** (stream): `{actionId, cwd, action: "commit"|"push"|"create_pr"|"commit_push"|"commit_push_pr", commitMessage? (≤10000), featureBranch?, filePaths? (≥1), threadId? (upstream)}`.
  - `actionId` on the wire is `"{len}:{JSON[envId,cwd]}{localUuid}"` (`client-runtime/src/state/vcsAction.ts:231-237`). Events with a different actionId or cwd are ignored.
  - Events (`git.ts` upstream): `action_started {phases}`, `phase_started {phase: "branch"|"commit"|"push"|"pr", label}`, `hook_started {hookName}`, `hook_output {hookName|null, stream: "stdout"|"stderr", text}`, `hook_finished {hookName, exitCode|null, durationMs|null}`, `action_finished {result}`, `action_failed {phase|null, message}`. Each also carries `{actionId, cwd, action}`.
  - `GitRunStackedActionResult`: `{action, branch: {status: "created"|"skipped_not_requested", name?}, commit: {status: "created"|"skipped_no_changes"|"skipped_not_requested", commitSha?, subject?}, push: {status: "pushed"|"skipped_not_requested"|"skipped_up_to_date", branch?, upstreamBranch?, setUpstream?}, pr: {status: "created"|"opened_existing"|"skipped_not_requested", url?, number?, baseBranch?, headBranch?, title?}, toast: {title, description?, cta: {kind:"none"} | {kind:"open_pr", label, url} | {kind:"run_action", label, action:{kind}}}}`.
- **Completion:**
  - Success: the toast becomes a success toast with `result.toast.title` / `description`, an optional CTA button, and auto-dismiss after 10s visible.
  - Failure: `Action failed` plus the message.
  - If `branch.status === "created"`, update the thread branch with `thread.meta.update` (orchestration dispatch) or with the draft context.
- **Pull:** a loading toast `Pulling...`, then RPC `vcs.pull {cwd}`, which returns `{status: "pulled"|"skipped_up_to_date", refName, upstreamRef|null}`. Success: `Pulled` / `Updated {ref} from {upstream}`, or `Already up to date` / `{ref} is already synchronized.` Failure: `Pull failed`.

**Commit dialog** (`:1817-1984`):
- Title `Commit changes`; description `Review and confirm your commit. Leave the message blank to auto-generate one.`
- Summary box: `rounded-lg border border-input bg-muted/40 p-12 text-xs`.
  - Row `Branch`, then `{refName}` (`(detached HEAD)` when detached). On the default ref, a right-aligned warning `Warning: default refName`.
  - Row `Files`: a count `({selected} of {all})` when filtered, and an `Edit`/`Done` ghost xs button.
  - File list: a scroll area 176px tall, `rounded-md border-input bg-background`. Rows are `rounded-md px-8 py-4 font-mono text-xs`, hover `accent/50`.
    - In edit mode, a checkbox appears per row, plus an all/indeterminate checkbox in the header.
    - Each row shows the path, then right-aligned `+ins` (`success`), ` / ` (muted), and `-del` (`destructive`). Excluded rows show `Excluded`.
    - Clicking a path opens it in the editor.
  - Totals are right-aligned, mono.
  - Files come from `status.workingTree.files [{path, insertions, deletions}]`. With no files, it shows `none`.
- `Commit message (optional)`: textarea `sm`, placeholder `Leave empty to auto-generate`.
- Footer: `Cancel` (outline), `Commit on new refName` (outline, sets `featureBranch`), `Commit` (primary). Both commit buttons are disabled when no files are selected. `filePaths` is sent only when the selection is partial.

**Publish repository dialog** (`:374-968`), `max-w-xl`:
- Title `Publish repository`; description `Pick where to host it, then point us at a repo to push to.`
- **Step chips** in a 3-column grid: `Provider`, `Repository`, `Summary`. Each chip is `rounded-lg border px-12 py-8`, with `Step {n}` (`text-[10px] uppercase muted`) above the label (`text-xs font-semibold`). States: current is `border-primary bg-primary/10 ring-1 ring-primary/25`; done shows a filled check circle.
- **Step 0 (Provider):** a 2-column grid of radio cards: GitHub (`github.com`), GitLab (`gitlab.com`), Bitbucket (`bitbucket.org`), Azure DevOps (`dev.azure.com`). Ready providers sort first.
  - Unready cards sit at 55% opacity with a `Setup Required` button that navigates to Settings › Source Control. Its tooltip is the install or auth hint.
  - Readiness comes from `server.discoverSourceControl {}`, which returns `sourceControlProviders [{kind, status: "available"|"missing", label, installHint, auth: {status, account: Option, detail: Option}}]`.
- **Step 1 (Repository):**
  - An input with the prefix `{host}/` and a placeholder of `owner/repo`, `group/project`, `workspace/repository`, or `project/repository` depending on the provider. It is prefilled with `{account}/`.
  - Visibility radio cards: `Private` / `Only invited people` (Lock) and `Public` / `Anyone on the web` (Globe).
  - An `Advanced` disclosure with `Remote` (default `origin`) and `Protocol` (SSH | HTTPS).
  - A pending row `Publishing repository to {provider}...`; an error box `Publish failed` with the message.
- **Step 2 (Summary):** a success check, then `Repository published` (`{branch} is now live on {provider}.`) or `Repository created` (`Remote "{remote}" is set up. Make a commit and push it to share your code.`). Below that, a `nameWithOwner` box and an `Open on {provider}` button.
- Footer: `Cancel`/`Back`, then `Next` or `Publish` (`Publishing...` while pending), and finally `Done`.
- **RPC `sourceControl.publishRepository`:** `{cwd, provider, repository, visibility: "private"|"public", remoteName?, protocol?: "auto"|"ssh"|"https"}` returns `{repository: {provider, nameWithOwner, url, sshUrl}, remoteName, remoteUrl, branch, upstreamBranch?, status: "pushed"|"remote_added"}`.

### 4.6 PR checkout dialog (`components/PullRequestThreadDialog.tsx`, `pullRequestReference.ts`)

**Opened from** the branch toolbar (chat spec) for local draft threads only (`canCheckoutPullRequestIntoThread`).

**Layout:** a Dialog, `max-w-xl`.
- Title: the provider icon (16px) and `Checkout {singular}`.
- Description: `Resolve a {providerName} {singular}, then create the draft thread in the main repo or in a dedicated worktree.`
- A labelled input (`{singular}`, capitalized) with placeholder `{PR} URL, checkout command, or #42`. It is autofocused with its text selected. Enter confirms in local mode.

**Parsing** (`pullRequestReference.ts:25-59`) accepts:
- `gh pr checkout …`, `glab mr checkout …`, `az repos pr checkout --id N`
- GitHub `/pull/N`, GitLab `/-/merge_requests/N`, and Azure `/_git/x/pullrequest/N` URLs
- `N` or `#N`

**Resolution:**
- Debounced **450ms**, then RPC `git.resolvePullRequest {cwd, reference}`, which returns `{pullRequest: {number, title, url, baseBranch, headBranch, state: "open"|"closed"|"merged"}}`.
- Card: `rounded-xl border border/70 bg-muted/24 p-12`. It shows the title (`text-sm font-medium`), `#{n} · {head} to {base}` (`text-xs muted`), and the state in color: open `emerald-600`/`emerald-300/90`, merged `violet-600`/`violet-300/90`, closed `zinc-500`/`zinc-400/80`.
- While resolving: a spinner and `Resolving {singular}...`.
- Errors (`text-xs text-destructive`): `Paste a {singular} URL, checkout command, or enter 123 / #123.`, `Use a {singular} URL, checkout command, 123, or #123.`, a resolution error, or `Failed to prepare {singular} thread.`

**Footer:** `Cancel`, `Local` (outline; `Preparing local...`), `Worktree` (primary; `Preparing worktree...`).
- RPC `git.preparePullRequestThread {cwd, reference, mode: "local"|"worktree", threadId? (worktree only)}` returns `{pullRequest, branch, worktreePath|null, isOnPullRequestHead}`.
- `isOnPullRequestHead` is upstream-only and defaults to true.
- Then `onPrepared({branch, worktreePath})` updates the draft thread.

---

## 5. Browser preview

Path prefixes: `D/` = `apps/desktop/src/`, `W/` = `apps/web/src/`, `C/` = `packages/contracts/src/`, `S/` = `packages/shared/src/` (all in the fork). `U:` = upstream repo root.

### 5.1 Files

**Visible UI** (`W/components/preview/`):
- `PreviewPanel.tsx`
- `PreviewView.tsx` (the whole surface)
- `PreviewChromeRow.tsx`, `PreviewMoreMenu.tsx`
- `PreviewEmptyState.tsx`, `PreviewLocalServerCard.tsx`, `BrowserMockup.tsx`
- `PreviewUnreachable.tsx`, `errorCodeMessages.ts`, `previewConstants.ts`
- `ZoomIndicator.tsx`, `useLoadingProgress.ts`
- `AgentBrowserCursor.tsx`
- `previewUrlPresentation.ts`
- `W/browser/BrowserDeviceToolbar.tsx`, `BrowserViewportResizeHandles.tsx`

**Client state and sync:**
- `W/previewStateStore.ts`
- `usePreviewSession.ts`, `usePreviewBridge.ts`
- `openPreviewSession.ts`, `closePreviewSession.ts`, `addBrowserSurface.ts`, `openDiscoveredPort.ts`
- `useDiscoveredLocalServers.ts`
- `W/browser/browserTargetResolver.ts`, `openFileInPreview.ts`

**Electron host:**
- `W/browser/{ElectronBrowserHost,HostedBrowserWebview,BrowserSurfaceSlot}.tsx`, `browserSurfaceStore.ts`, `desktopTabLifetime.ts`
- `D/preview/Manager.ts` (2973 lines), `BrowserSession.ts`, `PickPreload.ts`, `PlaywrightInjectedRuntime.ts`, `PreviewKeyboard.ts`
- `D/ipc/methods/preview.ts`

**Automation host:**
- `W/components/preview/PreviewAutomationHosts.tsx`, `previewAutomationRequestConsumer.ts`, `previewAutomationErrors.ts`

**Upstream server:**
- `U:apps/server/src/preview/{Manager,PortScanner}.ts`
- `U:apps/server/src/mcp/PreviewAutomationBroker.ts`
- `U:apps/server/src/mcp/toolkits/preview/{tools,handlers}.ts`

### 5.2 How it works in Electron

**Guests.** Guests are `<webview>` tags (`webviewTag: true`, `D/window/DesktopWindow.ts:293`). They are not WebContentsView or BrowserView.

**Host.** A single app-root host renders one `HostedBrowserWebview` per server preview session across **all** threads (`W/AppRoot.tsx:17-18`, `W/browser/ElectronBrowserHost.tsx`). Guests are never reparented, so switching threads or panels never reloads a page.

**Slot/lease placement:**
- `BrowserSurfaceSlot` lives inside `PreviewView`. It measures its rect (ResizeObserver plus window resize and scroll) and calls `lease.present(rect, visible)`.
- The host positions the webview `position:fixed` at that rect with z 30.
- Inactive guests park at `(-100000, -100000)` with `zIndex:-1`, `pointer-events:none`, and stay CSS-visible. Hiding a guest would stall CDP (`W/browser/hostedBrowserWebviewStyle.ts:18-48`).
- Tab webContents are ref-counted (`desktopTabLifetime.ts`).

**Partitions.**
- Fork: `persist:t3code-preview-{sha256(environmentId)[0..20]}` (`D/preview/BrowserSession.ts:12,95-107`).
- Upstream adds named browser profiles and incognito (`t3code-preview-ephemeral-…`), plus cookie import.

**Sessions.**
- The user agent is stripped of `Electron/` and `t3code/`.
- Allowed permissions: `clipboard-read`, `clipboard-write`, `notifications`, `geolocation`.
- Webview prefs: `contextIsolation=false,sandbox=true,nodeIntegration=false`. contextIsolation is off so react-grab sees the React DevTools hook.

**Navigation events.**
- Main listens to `did-navigate(-in-page)`, `page-title-updated`, `did-start/stop-loading`, and `did-fail-load`.
- `did-fail-load` ignores code -3 and subframes.
- `window.open` loads in the same guest (`D/preview/Manager.ts:1245-1262`).
- URL normalization: loopback hosts get `http`, others get `https`, and only http(s) is allowed (`S/preview.ts:81-109`).

**Zoom and color scheme.**
- Zoom ladder: `0.25 0.33 0.5 0.67 0.75 0.8 0.9 1.0 1.1 1.25 1.5 1.75 2.0 2.5 3.0 4.0 5.0`, via `setZoomFactor` (`Manager.ts:92-94`).
- Color-scheme emulation is upstream only, through CDP `Emulation.setEmulatedMedia`.

**Device viewports.** CSS-only: the webview element is sized to `w×h×zoom` and scaled down to fit. The user agent and touch behavior do not change.

**Automation.**
- CDP via `webContents.debugger.attach("1.3")`.
- Playwright: only the **injected selector runtime** is used, sliced out of `playwright-core/lib/coreBundle.js` and run with `browserName:"chromium"` (`D/preview/PlaywrightInjectedRuntime.ts`). Real Playwright is not used.

**Shortcut forwarding.** ⌘⇧J, ⌘K, ⌘, and ⌘W pressed in the guest are cancelled and re-dispatched to the app (`Manager.ts:325-339`).

### 5.3 Visible UI

**`PreviewView`** box tree (`PreviewView.tsx:558-646`):

```
div flex min-h-0 flex-1 flex-col bg-background
├─ ChromeRow (.surface-subheader h 40px, gap 4px, px 8px) + 2px load bar
└─ Stage  relative min-h-0 flex-1 overflow-hidden
    ├─ BrowserSurfaceSlot absolute inset-0         (only when tab && snapshot && !emptyState; hidden while LoadFailed)
    ├─ EmptyState                                   (no snapshot or navStatus Idle)
    ├─ ZoomIndicator                                (when desktop overlay exists)
    ├─ AgentBrowserCursor                           (overlay present, not empty, not failed)
    ├─ ControllerBadge  absolute left 12 top 12 z-40 rounded-full border/70 bg-background/90 px-10 py-4 text-[11px] font-medium shadow-sm backdrop-blur
    │     text: "Agent controlling browser" | "Human control"   (when controller ≠ "none")
    └─ Unreachable overlay absolute inset-0 z-10 bg-background   (navStatus LoadFailed)
```

**Chrome row** (`PreviewChromeRow.tsx:146-331`), left to right:
1. Navigation group (gap 2px). Each button is ghost `icon-xs` (24px) with a 14px icon:
   - Back (`ArrowLeft`), tooltip `Back`, disabled when it can't go back.
   - Forward (`ArrowRight`), tooltip `Forward`.
   - Refresh (`RotateCw`, spinning while loading), aria `Stop`/`Refresh`, tooltip `Loading…`/`Refresh`. Disabled when nav is Idle. It always refreshes; there is no stop action.
2. Address field, `flex-1`, height 28px, rounded-md, transparent border and bg, no shadow.
   - Background: `muted/40` on hover, `background` when focused.
   - Placeholder `Search or enter URL`, `sm` input, no spellcheck.
   - **Unfocused** it shows `displayUrl`: the host only for http(s), or `{environmentLabel} · {fileName}` for `/api/assets/…` URLs (`previewUrlPresentation.ts`). A tooltip shows the full URL.
   - **On focus** the draft becomes the full URL and is selected.
   - Enter submits the trimmed value. Escape reverts and blurs.
   - On hover (unfocused), an inline-end ghost `icon-xs` `ExternalLink` fades in, aria and tooltip `Open in system browser`. The input gains 28px of end padding.
3. Annotate toggle, ghost `icon-xs`. It switches to the `secondary` variant when active.
   - Icon `MousePointerClick`, tinted `primary` when active.
   - aria: `Annotate preview` / `Cancel annotation`.
   - Tooltip: `Annotate elements, regions, and drawings` / `Cancel annotation (Esc)`, or when disabled: `Page didn't load — pick unavailable until the page renders`.
   - Disabled when there is no tab or the page failed to load.
4. Capture, ghost `icon-xs`. It switches to `secondary` while recording.
   - Icon `Camera`, tinted `destructive` while recording, with a 6px pulsing `destructive` dot at the top-right (2px inset).
   - Tooltip: `Screenshot · Shift-click to record` / `Stop recording`.
   - Disabled until a webContents exists, or when the page failed to load.
5. More menu, ghost `icon-xs` `MoreVertical`, tooltip `More`. It opens an end-aligned menu with a 6px offset and min width 224px (`PreviewMoreMenu.tsx:65-131`):
   - `Hard reload`, `Open DevTools`, `Show device toolbar`/`Hide device toolbar`
   - separator
   - Zoom row (stays open on click): `Zoom`, then outline `icon-xs` `Minus`, a 48px-min-width `{n}%` in `text-xs tabular-nums muted`, outline `icon-xs` `Plus`, and ghost `icon-xs` `RotateCcw`
   - separator
   - `Clear cookies`, `Clear cache`
   - Tab-scoped items are disabled until the webContents registers.

**Load bar** (`PreviewChromeRow.tsx:320-329`, `useLoadingProgress.ts`):
- Absolute at the bottom-left, 2px tall, `rounded-r-full bg-primary`, with `box-shadow 0 0 6px 1px ring`. Width transitions over 150ms ease-out.
- Simulated progress: seeds at 4%, then every 120ms adds `max(0.5, (90 − p) × 0.08)`, approaching 90%. On finish it jumps to 100% and resets to 0 after 220ms.

**Empty state** (`PreviewEmptyState.tsx`). The list of discovered servers is configured script `previewUrl`s, then the `subscribeDiscoveredLocalServers` stream, then up to 10 recently seen URLs, deduplicated by `host:port`.
- **No servers:** the `Empty` primitive with a `Globe` 18px icon, `No preview yet`, and `Type a URL above, or run a dev script. Listening localhost ports will show up here automatically.`
- **With servers:** a scroll area (`px-20 py-32`) centered at max width 576px with 12px gaps:
  - Header: `RadioTower` 16px and `Local servers` (`text-sm font-medium muted`).
  - A card list `divide-y divide-border/60 rounded-xl border border/70 bg-background`.
  - Footer `Select a listening port to open it in this browser tab.` (`text-xs muted`, px 4px).
- **Server row** (`PreviewLocalServerCard.tsx`): `px-12 py-12 gap-12`, hover `accent/40`.
  - A `BrowserMockup` glyph, 28px: `rounded-[5px] border/60 bg-card p-4` with three 3px dots (`destructive/80`, `warning/80`, `success/80`) and two 2px bars.
  - Title (`text-sm font-medium`): `processName`, else `Listening`, else `Configured`, else `Recently seen`.
  - Subtitle `host:port` (`text-xs muted`).
  - Status dot at the end: 8px `success` with a ping animation when listening, else `muted-foreground/40`. **Avoid a continuous animation natively; use a static dot or a single fade.**
  - Clicking resolves the URL through the environment host (`browserTargetResolver.ts`).

**Unreachable view** (`PreviewUnreachable.tsx`):
- Container: max width 576px, `px-32 py-48` (64 vertical at `sm`), scrollable.
- A 48px stroked "page with slash" SVG at `muted/70`.
- Title `This site can’t be reached` (`text-2xl font-semibold`).
- `{host}: {friendly}.` with the host in bold.
- An optional details box (`rounded-lg border bg-muted/40 p-16`): `Try:` followed by the bullets `Checking your connection`, `Confirming the dev server is running`, `Checking the proxy and the firewall`.
- An uppercase error code label (`text-xs tracking-wide muted/70`), for example `ERR_CONNECTION_REFUSED`.
- Footer: `Details`/`Hide details` (outline sm) and `Reload` (primary sm).
- The friendly table (`previewConstants.ts:8-21`) maps Chromium `ERR_*` names to text such as `Connection refused` or `DNS address could not be found`. **Natively, map `NSURLError` codes onto the same names.**

**Zoom pill** (`ZoomIndicator.tsx`):
- Absolute at top 12 / right 12, z-20, `rounded-full border/70 bg-popover/95 px-10 py-4 text-xs font-medium shadow-md/10 backdrop-blur`, showing `{n}%`.
- It appears on any zoom change (not on mount): fades in and translates from −4px to 0 over 200ms, then hides after 1500ms.

**Agent cursor** (`AgentBrowserCursor.tsx`):
- Lucide `MousePointer2`, 20px, filled with `background`, stroke `primary`, with a drop shadow.
- Position: `(x × zoom × scale + content.x − scrollLeft, …)`. It moves with a 150ms ease-out transition.
- Clicks add a 16px `primary/25` ping.
- Opacity is 1 for 700ms after each event, then 0.35 (agent controlling) or 0.18 (human controlling).

**Device toolbar** (fixed viewport, nice-to-have):
- A 32px sticky bar with `border-b border/70 bg-background/95 backdrop-blur-md`.
- Controls: a preset select (`Responsive` plus the presets below), W×H inputs, an aspect-lock toggle (`Link2`), rotate, and close.
- 10px resize rails around the scaled viewport. Fit scale is `min(1, cw/(w·z), ch/(h·z))`, centered (`W/browser/browserViewportLayout.ts:22-23,54-117`).
- Presets (`S/previewViewport.ts`): iPhone SE 375×667, iPhone XR 414×896, iPhone 12 Pro 390×844, iPhone 14 Pro Max 430×932, Pixel 7 412×915, Samsung Galaxy S8+ 360×740, Samsung Galaxy S20 Ultra 412×915, iPad Mini 768×1024, iPad Air 820×1180, iPad Pro 1024×1366, Surface Pro 7 912×1368, Surface Duo 540×720, and others.
- Toggling from fill sets freeform at the responsive size of the panel rect, or 1024×768 (`PreviewView.tsx:179-192`).

**Screenshot and record toasts** (`PreviewView.tsx:212-458`):
- **Screenshot:** a PNG saved to the artifacts directory. The toast `Screenshot saved` has the primary action `Copy image` (then `Copied!` for 2s), an additional action `Copy path`, and a secondary action `Reveal in Finder` (outline).
- **Shift-click record:** starts recording. Clicking again stops it, producing `Recording saved` with `Reveal in Finder` and `Copy path`.
- Errors:
  - `Unable to capture screenshot`
  - `Another preview is recording` / `Stop the active recording before starting a new one.`
  - `Unable to start recording`
  - `Unable to stop recording`
  - `Unable to resize browser viewport`

**Unsupported runtime text:** `Preview is only available in the T3 Code desktop app.` This never shows natively.

### 5.4 Client state and sync

**Per-thread state** (`W/previewStateStore.ts`):

```
{ sessions: Record<tabId, PreviewSessionSnapshot>, activeTabId, suppressedTabIds,
  desktopByTabId: Record<tabId, {canGoBack, canGoForward, loading, zoomFactor, controller: "human"|"agent"|"none"}>,
  recentlySeenUrls (≤10) }
```

The right panel reconciles its browser tabs from `sessions` (1.2).

**Server sync** (`usePreviewSession.ts`):
- Uses `preview.list {threadId}` plus the `subscribePreviewEvents {}` stream.
- If the server forgets a session that has a URL, the client re-opens it.

**Desktop to server** (`usePreviewBridge.ts`):
- The guest's state drives `preview.reportStatus {threadId, tabId, navStatus, canGoBack, canGoForward}`, deduplicated on (kind, url).
- `LoadFailed` always reports. `Idle` never reports.
- The URL bar navigates the guest directly; it does **not** call `preview.navigate`.

**Opening a browser tab:**
- From the `+` menu: `preview.open {threadId}`, then `openBrowser(tabId)`.
- From a URL: `preview.open {threadId, url}`.
- An HTML or PDF file: `assets.createUrl`, then open the asset URL (`openFileInPreview.ts`).

**Closing.** `preview.close {threadId, tabId}`, optimistic with rollback.

### 5.5 Preview RPCs (upstream `rpc.ts:1208-1285`, `preview.ts`, `previewAutomation.ts`)

| Tag | Kind | Payload | Success |
|---|---|---|---|
| `preview.open` | unary | `{threadId, url?, viewport?, profileId?}` (fork lacks `viewport` and `profileId`) | `PreviewSessionSnapshot` |
| `preview.navigate` | unary | `{threadId, tabId, url, resolvedTitle?}` | snapshot |
| `preview.resize` | unary | `{threadId, tabId, viewport: PreviewViewportSetting}` | snapshot |
| `preview.refresh` | unary | `{threadId, tabId}` | void |
| `preview.close` | unary | `{threadId, tabId?}` (no tabId closes all tabs of the thread) | void |
| `preview.list` | unary | `{threadId}` | `{sessions[], serverEpoch, revision}` (fork: `{sessions}`) |
| `preview.reportStatus` | unary | `{threadId, tabId, navStatus, canGoBack, canGoForward}` | void |
| `subscribePreviewEvents` | stream | `{}` | `PreviewEvent` |
| `subscribeDiscoveredLocalServers` | stream | upstream `{configuredUrls?: string[≤32]}`, fork `{}` | `{servers[], scannedAt, configuredUrlProbing?}` |
| `previewAutomation.connect` | stream | `{clientId, environmentId, supportedOperations?}` | `{type:"connected", connectionId}` \| `{type:"request", connectionId, request}` |
| `previewAutomation.respond` | unary | `{clientId, connectionId, requestId, ok, result?, error?: {_tag, message, detail?}}` | void |
| `previewAutomation.focusHost` | unary | `{clientId, environmentId, connectionId, focused, liveTabs?: [{threadId, tabId, visible?}]}` (`liveTabs` is upstream only) | void |

Shapes:
- `PreviewNavStatus` (`_tag`) is one of `Idle`, `Loading {url, title}`, `Success {url, title}`, `LoadFailed {url, title, code: Int, description}`. URLs are ≤2048 chars, titles ≤512.
- `PreviewSessionSnapshot = {threadId, tabId (≤128), navStatus, canGoBack, canGoForward, viewport?, profileId?, updatedAt}`.
- `PreviewViewportSetting` is `{_tag:"fill"}`, `{_tag:"freeform", width, height}`, or `{_tag:"preset", width, height, presetId}`. Each dimension is an integer from 240 to 3840, and the area is at most 3840×2160.
- `PreviewEvent` (`type`): `opened`, `navigated`, and `resized` carry `{snapshot}`; `failed` carries `{url, title, code, description}`; `closed`. All share `{threadId, tabId, createdAt}`, plus `serverEpoch` and `revision` upstream.
- Errors: `PreviewSessionLookupError{threadId, tabId}` and `PreviewInvalidUrlError{inputLength, reason: "empty"|"parse"|"unsupported-protocol"|"unexpected", protocol?}`.

**Server port scanner** (`U:apps/server/src/preview/PortScanner.ts`): `lsof -iTCP -sTCP:LISTEN` every 3s. It publishes only ports that pass an HTML probe (1s timeout, 15s cache).

### 5.6 Element picking and annotations

**Flow:**
1. The annotate toggle snapshots the focused element and asks the host to start picking.
2. The guest preload (`D/preview/PickPreload.ts`, 1263 lines) mounts a closed shadow-root overlay at z `2147483646`, styled by Tailwind CSS compiled from `D/preview/Annotation.css` and themed by 17 app tokens sent from the renderer.
3. Tools (keys V, R, D, E):
   - `Select elements (V)`: hover box and label `tag#id.class`; click toggles, Shift adds.
   - `Draw a region or marquee-select elements (R)`: up to 20 elements whose centers fall inside the marquee, else a region.
   - `Draw freehand (D)`: SVG strokes, width 4, in `primary`.
   - `Remove an annotation target (E)`.
4. Editor: `Describe the change…` with an `Attach` button (`Capturing…` while working), an expand/collapse control, and a live style panel (`Font`, `Font size`, … `Gap`). The panel applies `!important` edits and records them as `styleChanges`.
5. Esc cancels. ⌘↩ submits.
6. On submit, each element goes through react-grab `getElementContext`, producing `{pageUrl, pageTitle, tagName, selector, htmlPreview, componentName, source, stack[], styles}`.
7. The crop rect is the union of target rects plus 20px, clamped to the viewport. Main crops a screenshot with `capturePage(cropRect)`.
8. The resulting `PreviewAnnotationPayload` (`C/ipc.ts:837-870`):

```
{id:"annotation_<n>", pageUrl, pageTitle, comment, elements[{id, element, rect}], regions[{id, rect}],
 strokes[{id, color, width, points, bounds}], styleChanges[{targetId, selector, property, previousValue, value}],
 screenshot: {dataUrl, width, height, cropRect}|null, createdAt}
```

**Composer handoff:**
- `addPreviewAnnotation` puts the payload in the draft, and the screenshot is added as image `preview-annotation-{id}.png`. Chips render in `ComposerPreviewAnnotationCards`.
- On send, a `<preview_annotation>` block is appended (`W/lib/previewAnnotation.ts:21-69`, `W/lib/elementContext.ts`). It contains `Id:`, `Page:`, `Comment:`, `Targets: N selected element(s), N marked region(s), N drawing(s).`, `Requested visual changes:` lines of the form `- prop: prev → value`, and an `<element_context>` block with html and styles capped at 4000 chars. The composer spec owns this format.
- After the pick, focus returns to the previously focused element (`PreviewView.tsx:466-513`).

### 5.7 Automation host protocol (agent drives the browser)

**Flow:**
1. The agent calls MCP tools on the server's `t3-code` MCP endpoint (`U:apps/server/src/mcp/toolkits/preview/tools.ts`): `preview_status`, `preview_open`, `preview_navigate`, `preview_resize`, `preview_set_appearance`, `preview_snapshot`, `preview_click`, `preview_type`, `preview_press`, `preview_scroll`, `preview_evaluate`, `preview_wait_for`, `preview_recording_start`, `preview_recording_stop`.
2. The broker forwards each call to a connected desktop host as `{requestId:"preview-<n>", threadId, tabId?, tabIdExplicit?, operation, input, timeoutMs}`. The default timeout is 15000ms (`recordingStop` uses 120000).
3. The host answers with `previewAutomation.respond`.

**Routing.** Each provider session is pinned to one live host. Ties break on: owns the visible target tab, then owns the tab, then is focused, then most recently focused.

**Operations.**
- A host advertises its operations through `supportedOperations`. V1 set (`previewAutomation.ts:25-38`): `status, open, navigate, snapshot, click, type, press, scroll, evaluate, waitFor, recordingStart, recordingStop`. Current hosts add `resize` and `setColorScheme`.
- **Advertise only what is implemented.**
- **With no host connected, preview MCP tools fail.** That is acceptable until 5.8 ships.

| Operation | Host must… | `result` |
|---|---|---|
| status | resolve the tab (explicit, else the thread's active one); report visibility and measured inner size | `{available, visible, tabId, url, title, loading, viewportSetting?, viewport?}` |
| open | reuse a tab (default) or `preview.open`; open the panel unless `show/open:false`; wait for the guest | status |
| navigate | `url`, or `target{kind:"environment-port", port, protocol?, path?}`; wait for readiness `load`, `domContentLoaded`, or `none` (poll every 50ms) | status |
| resize | `preview.resize`; wait until the measured size matches ±1px | `{tabId, setting, viewport}` |
| snapshot | visible text (≤20000 chars), interactive elements (≤200, with selectors), AX tree, PNG ≤1280px wide (base64), console and network diagnostics (≤200 each), action timeline | snapshot object |
| click | resolve a Playwright locator, a CSS selector, or x/y; scroll into view; trusted mouse down/up; emit pointer move, then click, for the cursor overlay | void |
| type | focus the target; `execCommand("insertText")` with a native-setter fallback; dispatch `change`; reject non-editable targets | void |
| press | key chord to trusted key events | void |
| scroll | `scrollBy` on the window or the target | void |
| evaluate | run JS (await promises); JSON result ≤64000 bytes | value |
| waitFor | poll every 100ms (default 15000) for locator, text, or url | void |
| recordingStart / recordingStop | screencast to MP4/WebM | `{tabId, recording:true, startedAt}` / `{id, tabId, path, mimeType, sizeBytes, createdAt}` (+`uploadedAttachmentId` upstream) |

**Error `_tag`s the broker classifies:**
- `PreviewAutomationTimeoutError`
- `PreviewAutomationTabNotFoundError`
- `PreviewAutomationExecutionError`
- `PreviewAutomationInvalidSelectorError` {selectorKind, selectorLength}
- `PreviewAutomationTargetNotEditableError` {selectorKind}
- `PreviewAutomationResultTooLargeError` {maximumBytes}
- `PreviewAutomationControlInterruptedError`
- the recording errors

Source: `W/components/preview/previewAutomationErrors.ts`.

**Human vs agent control:**
- Each action takes a per-tab mutex and sets `controller:"agent"`.
- Trusted human input bumps a control epoch, which interrupts the running action, and shows `Human control` for 750ms.
- Agent-synthesized input is pre-registered for 1000ms (±1px, same button or key) so its echo isn't mistaken for a human.
- Automation fails while DevTools is open.

### 5.8 Native macOS approach

**Recommendation:** WKWebView driven through **`objc2` + `objc2-web-kit` directly**, attached as child NSViews of the GPUI window's content view. The host keeps one WKWebView per preview tab for the app's lifetime, mirroring the Electron host. A GPUI `BrowserSurfaceSlot` element reports its bounds during prepaint, and the host syncs the matching NSView frame (convert for flipped coordinates and the backing scale).

Inactive tabs stay in the view hierarchy, moved offscreen. Do not use `isHidden`: WebKit throttles hidden views, which breaks background automation and snapshots.

**Why not `wry` or `gpui-wry`?** `gpui-wry` 0.7 (`~/L-Projects/t3UI-refs/gpui-kit/crates/webview`, built on `lb-wry` 0.53) works for a spike. `wry` owns the navigation and UI delegates, though, and does not expose:
- load-error NSError codes
- `window.open` interception
- KVO on `canGoBack`, `canGoForward`, `isLoading`, `title`, `URL`
- `takeSnapshot`
- per-view `appearance`
- `WKContentWorld`
- reply-capable message handlers
- `performKeyEquivalent:` overrides

**Feature map:**

| Feature | WKWebView mapping | v1 | Size |
|---|---|---|---|
| Embed and position; keep tabs alive offscreen | child NSView, frame synced from GPUI bounds | Essential | M (throttling risk) |
| Navigation, back/forward, reload, hard reload, title/URL/loading | `load`, `goBack`/`goForward`, `reload`, `reloadFromOrigin`, KVO | Essential | S |
| LoadFailed | navigation delegate `didFail*` NSError; ignore `NSURLErrorCancelled` (-999, the analog of Chromium -3); map to `ERR_*` names for the unreachable view | Essential | S |
| Popups in the same tab | `WKUIDelegate createWebViewWith` returns nil and loads the request in place | Essential | S |
| Zoom ladder | `pageZoom` (macOS 11+) | Essential | S |
| Partitions per environment or profile | `WKWebsiteDataStore(forIdentifier: UUID)` (macOS 14+, UUID from the sha256 of the scope); `.nonPersistent()` for incognito | Essential (min macOS 14) | S |
| Clear cookies and cache | `WKWebsiteDataStore.removeData` | Nice | S |
| User agent | `applicationNameForUserAgent = "Version/x Safari/605.1.15"` | Essential | S |
| App shortcuts while the page has focus | WKWebView subclass `performKeyEquivalent:` forwards ⌘⇧J, ⌘K, ⌘, and ⌘W to GPUI | Essential | S-M |
| Annotate and pick | ship `PickPreload` as a `WKUserScript` at document start in the page world (react-grab needs the DevTools hook); replace `ipcRenderer` with `webkit.messageHandlers` plus host-called `window.__t3pick.*`; validate payloads strictly in Rust | Essential | M |
| Crops and screenshots | `takeSnapshot(with: WKSnapshotConfiguration{rect, snapshotWidth})` | Essential | S |
| Locators | embed Playwright's InjectedScript JS with `include_str!`, pinned version, `browserName:"webkit"`, run in a named `WKContentWorld` | Essential for automation | M |
| click | locator to point; synthesized `NSEvent` mouse down/up on the WKWebView (trusted); JS `click()` fallback | Essential for automation | M |
| type, scroll, waitFor, snapshot text and elements | port the fork's JS verbatim | Essential for automation | S |
| evaluate | `callAsyncJavaScript` plus a `JSON.stringify` wrapper; keep the 64000-byte cap | Essential for automation | S |
| press | no CDP `Input.dispatchKeyEvent`: make the webview first responder, post `NSEvent` keyDown/keyUp, restore focus | Nice (defer) | L |
| AX tree, console, network diagnostics | Playwright `ariaSnapshot` or a JS role walk; console and fetch shims at document start | Nice | M |
| Color scheme override | `webView.appearance = darkAqua / aqua / nil` | Nice | S |
| DevTools | `isInspectable = true` (13.3+), opens in Safari Web Inspector | Nice | S |
| Agent cursor and controller badge, zoom pill, unreachable overlay | GPUI cannot paint above a native NSView: use a transparent, non-hit-testing NSView/CALayer above the webview, or hide the webview when the overlay must cover it (LoadFailed) | Nice (cursor) / Essential (unreachable) | M |
| Device toolbar and fixed viewports | frame = w×h×zoom×scale with `pageZoom` = zoom×scale (this changes `devicePixelRatio`, unlike Chromium) | Nice | M |
| Recording | `takeSnapshot` loop at ~12fps into AVAssetWriter, or ScreenCaptureKit (needs a TCC permission) | Nice (defer) | L |

**Airspace (the largest UX risk).** GPUI popovers, menus, tooltips, and toasts that overlap the preview draw **under** the WKWebView. Options:
- While any app overlay intersects the webview rect, swap the webview for a `takeSnapshot` image.
- Or position overlays so they never overlap the webview.

This also affects the right-panel tab context menu, which is safe if native (NSMenu) and unsafe if drawn by GPUI.

**v1 cut.**
- **In:** embedding, the chrome row, navigation and status sync (`preview.open/list/reportStatus/close`, `subscribePreviewEvents`), the unreachable view, the empty state with discovered servers, zoom, the per-environment data store, screenshots, and annotate/pick with the composer chip.
- **Automation host:** advertise `status, open, navigate, snapshot (text, elements, image; no AX or diagnostics), click, type, scroll, evaluate, waitFor`.
- **Defer:** `press`, recording, the device toolbar, the agent cursor overlay, color scheme, profiles, and DevTools polish.

**Size:** core preview M, annotate M, automation M-L, full parity XL.

---

## 6. Implementation order and the hardest pieces

### 6.1 Order

1. **Right panel shell** (1.x): state model, inline clip and animation, resize, tab bar, empty state, titlebar toggles and maximize, sheet mode. This unblocks every other surface. Effort S-M.
2. **Plan surface** (4.1): pure rendering plus one RPC. It reuses ChatMarkdown from the chat spec. Effort S.
3. **Git actions, Open-in, project scripts, PR dialog** (4.3-4.6): header controls, dialogs, toasts, streaming RPC. Effort M. Scripts depend on the terminal for running, so stub the run step until step 4 lands.
4. **Terminal** (3.x): an emulator model per session, the custom element, input encoding, drawer, splits and sidebar, links, Add to chat. Effort L.
5. **Diff panel** (2.x): parser, virtualized view, syntax highlighting, sticky headers, split mode, wrap, collapse, review comments. Effort L.
6. **Files surface** (4.2): the tree (`gpui-component` `Tree` restyled), a read-only viewer reusing the diff renderer's file mode, then an editable buffer. Comments come last. Effort M for read-only; L for editing.
7. **Browser preview** (5.x): WKWebView child view, chrome row, empty state, then picking, annotations, and automation. Effort L for core; XL for everything.
8. **Polish:** ChangedFilesTree card, device toolbar, recording, reduced-motion.

### 6.2 Hardest pieces and suggested approaches

| Piece | Why hard | Approach |
|---|---|---|
| Terminal fidelity | No GPL code to lean on. Key, IME, and mouse encoding plus fast rendering all need building. | `alacritty_terminal` for VT and grid. Write the key encoder from the xterm spec and Apache-licensed alacritty references. Batch quads and shaped runs per row and cache shaped rows by content hash. Implement the Ack window correctly. Test with `vim`, `htop`, `less`, and bracketed paste. |
| Diff syntax colors equal to Shiki | Shiki's TextMate engine and the Pierre theme JSON have no Rust equivalent. | `syntect` plus the converted Pierre themes. Check a sample set of files against screenshots. Fall back to the tree-sitter highlighter if grammar coverage is poor. |
| Virtualized diff with sticky headers, wrap, split, inline annotations | Variable heights, width-dependent wrapping, annotation rows inside the grid | Per-file item heights computed from a row model, cached by width. Annotation rows are ordinary rows of measured height. Sticky header is an overlay paint. |
| Inline panel animation without reflow | GPUI has no CSS transitions | A custom wrapper with an animated width and content at fixed width, aligned right. One animation driver shared with the terminal drawer height. |
| Editable file view | Pierre's contentEditable editor, with comments | v1 can ship read-only with comments, and edit via the gpui-component `Input` code editor (rope-based, with highlighter) behind a feature flag. Keep the 500ms debounced `projects.writeFile` semantics. |
| WKWebView inside GPUI | Native child NSView z-order over GPUI's Metal layer; GPUI popovers cannot draw above it | See section 5. Hide the webview while app popovers overlap it, or snapshot it to an image. Keep chrome UI outside the webview rect. |
| Native context menus | Several flows use `api.contextMenu.show` (tabs, terminal selection, link menu) | Use NSMenu via objc2 `popUpMenuPositioningItem`, or gpui-component `ContextMenu`. Native is closer to Electron. |

### 6.3 VCS status shape (used by 2.x and 4.5)

**`subscribeVcsStatus {cwd}`** streams events:
- `{_tag:"snapshot", local, remote|null}`
- `{_tag:"localUpdated", local}`
- `{_tag:"remoteUpdated", remote|null}`

`local` = `{isRepo, sourceControlProvider?: {kind, name, baseUrl}, hasPrimaryRemote, isDefaultRef, refName|null, hasWorkingTreeChanges, workingTree: {files: [{path, insertions, deletions}], insertions, deletions}}`

`remote` = `{hasUpstream, aheadCount, behindCount, aheadOfDefaultCount?, pr: {number, title, url, baseRef, headRef, state, isDraft?, updatedAt?}|null}` (`isDraft` and `updatedAt` are upstream-only)

The client merges these into `VcsStatusResult = local ∪ remote`. `vcs.refreshStatus {cwd}` returns the full `VcsStatusResult` (`git.ts` upstream).

---

## 7. Open questions and risks

1. **Terminal font.** The xterm stack starts with `"SF Mono"`. In Electron on macOS that family only resolves if SF Mono is installed as a user font; otherwise it falls through to Menlo. Check the rendered font on the user's Mac (DevTools › Computed › Rendered Fonts on `.xterm-rows`) before you pick a font for GPUI. The same question applies to the diff and code `--font-mono`.
2. **Shiki parity.** Exact token colors need TextMate grammars. Tree-sitter highlighting will look different, and the "visually identical" bar may need syntect.
3. **Pierre internals.** The values in 2.6 come from `@pierre/diffs` 1.3.0-beta.5 CSS (`dist/style.js`) and T3's `unsafeCSS`. Any Pierre update changes the look. Pin to these numbers, not to "whatever Pierre does".
4. **Fork vs upstream drift.**
   - Upstream adds `review.getDiffPreview.file`, `files` stats, `review.getDiffFileContents`, `terminal.*.providerInstanceId`, `git.runStackedAction.threadId`, `LaunchEditorInput.reveal`, Forgejo, and the `isOnPullRequestHead`, `isDraft`, and `updatedAt` fields.
   - All of them are optional, so the fork's payloads stay valid against the upstream server.
   - Decode responses leniently: unknown fields are allowed, and treat `isOnPullRequestHead` as defaulting to true.
5. **Effect RPC Acks.** Missing Acks deadlock every stream: terminal output, git action progress, VCS status, preview events. Implement Ack in the transport layer, not per feature.
6. **Native menus vs custom popovers.** Electron uses native context menus in some places (tabs, terminal) and base-ui popovers elsewhere. Keep that split for parity.
7. **Persisted state migration.** localStorage state (right panel, diff scope, terminal layout, panel width, last editor, explorer toggle) does not carry over from the Electron app. The native app starts with defaults. Confirm that is acceptable.
8. **Review comments live in composer drafts.** Diff and file comments are part of composer state and are serialized into the prompt. The composer spec must own `reviewComments`, its chips (`ComposerPendingReviewComments`), and the `<review_comment>` format, and agree with 2.7.
9. **Scripts preview URL quirk.** The fork drops `previewUrl` and `autoOpenPreview` on save (4.3). Decide whether to keep or fix that.
10. **Reduced motion.** The Electron app disables all panel and drawer transitions under `prefers-reduced-motion`. Read `NSWorkspace.accessibilityDisplayShouldReduceMotion` to match.
