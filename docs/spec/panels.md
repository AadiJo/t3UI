# Panels spec: right panel system, terminal drawer, chat header tools

> Refreshed against fe7d3092c (2026-10-02). This file replaces the July-era panels spec. Facts below
> were re-read from the fork; nothing was copied from the old file.

Build spec for everything right of (and under) the chat column: the per-thread right panel with its
tab bar, the panel layout toggles in the titlebar, the Agents surface, and the chrome of the preview,
device and pull-request surfaces. The other surfaces and tools live in sibling files:

| File | Covers |
| --- | --- |
| `docs/spec/panels.md` (this) | Surface model, layout modes, width/resize/animation, titlebar layout controls, tab bar, empty-state launcher, surface subheader convention, Agents surface, preview/device/pull-request chrome + appendices |
| `docs/spec/panels-diff.md` | Diff surface (`DiffPanel`, `components/diffs/*`, `@pierre/diffs` 1.3.0-beta.10) |
| `docs/spec/panels-files.md` | Files explorer and file tabs (`components/files/**`, `@pierre/trees`) |
| `docs/spec/panels-terminal.md` | Terminal drawer under the chat column and the terminal surface content |
| `docs/spec/panels-header-tools.md` | Chat header action cluster: project scripts, Open-in, Git actions, PR thread dialog |

## 0. Conventions

- Paths are `path:line` relative to the fork's `apps/web/src/` (fork = `~/L-Projects/t3UI-refs/t3code-fork`
  @ fe7d3092c). Prefixes: `contracts:` = `packages/contracts/src/`, `shared:` = `packages/shared/src/`,
  `crt:` = `packages/client-runtime/src/`, `server:` = `apps/server/src/`, `desktop:` = `apps/desktop/src/`,
  `upstream:` = `~/L-Projects/t3UI-refs/t3code-upstream/` (b33eda13).
- Sizes give the Tailwind class and px: `h-6` (24px). 1 unit = 4px. `text-xs` 12/16, `text-sm` 14/20,
  `text-2xs` 11px / line-height 16px (`calc(1/0.6875)`), `text-3xs` 10px / 14px (`index.css:165-168`).
- Radii (`index.css:265-268`, `--radius` = 10px at `index.css:1030`): `rounded-sm` 6px, `rounded-md` 8px,
  `rounded-lg` 10px, `rounded-xl` 14px, plain `rounded` 4px, `--control-radius` 8px (`index.css:92`).
- Desktop window: min 840x620 (`desktop:window/DesktopWindow.ts:399-400`), macOS `titleBarStyle:
  "hiddenInset"` (`:260`). `sm:` (640) and `md:` (768) always apply. The right panel switches to the
  overlay sheet at window width <= 980px, which the desktop window can reach (840-980px).
- Titlebar geometry on macOS (`index.css:111-126`, no `.wco` overrides on macOS):
  `--workspace-topbar-height` 52px, `--workspace-controls-top` 0, `--workspace-controls-right` 12px,
  `--workspace-titlebar-control-size` 28px, `--workspace-titlebar-control-gap` 12px,
  `--workspace-titlebar-items-lift-padding` 0. `--workspace-controls-left` is 82px on a non-fullscreen
  macOS window (traffic lights, `components/AppSidebarLayout.tsx:58,252-256`), else 12px.
- Tooltips: Base UI 1.5.0, open delay 600ms, close immediately, default side top, `sideOffset` 4px,
  `text-xs`, `max-w-80` (320px), `px-2 py-1`, `rounded-md`, `bg-popover` + border
  (`components/ui/tooltip.tsx:13-60`, `nm:@base-ui/react@1.5.0 esm/tooltip/utils/constants.js:1`).
- Context menus marked "native" go through `readLocalApi().contextMenu.show(items, {x, y})`, which is
  an Electron native menu. In T3UI use the macOS `NativeMenu` path the sidebar already uses.
- Panel animations: `panelAnimationDurationMs` client setting, 0-400ms, default 0
  (`contracts:settings.ts:110-119`). `usePanelAnimationSettings` reports `active` only when the duration
  is > 0, `prefers-reduced-motion` is off and the route is not in its first painted frame
  (`panelAnimations.ts:70-78`, `:14-31`). With the default, every panel open/close/maximize is instant.
  Implement the 0ms path first; the animated path is described where it exists.

### Primitives used

| Use | Primitive | Resolved metrics (desktop) |
| --- | --- | --- |
| Terminal and right-panel toggles | `Toggle variant="panel" size="panel"` | 28x28, `rounded-lg` (10px), transparent border, icon 16px at opacity 0.8, `hover:bg-accent`, pressed stays transparent (`data-pressed:bg-transparent`, hover while pressed `bg-accent`), disabled `text-muted-foreground` at full opacity (`components/ui/toggle.tsx:9,22,40-41`) |
| Maximize/restore | `Toggle variant="ghost" size="sm"` | 28x28 (`sm:h-7 sm:min-w-7`, px 5px), `rounded-lg`, pressed `bg-accent text-accent-foreground`, icon 16px (`toggle.tsx:26,32-33`) |
| Tab "+" and launcher profile chevron | `Button size="icon-xs" variant="ghost-muted"` | 24x24, `rounded-[--control-radius]` (8px), icon 14px, `text-muted-foreground`, hover `bg-accent text-foreground` (`components/ui/button.tsx`) |
| Tab scroll arrows | `Button size="icon-xs" variant="ghost"` | 24x24, icon color `--contrast-muted-foreground`, hover `bg-accent` |
| Agents micro buttons | `Button size="icon-micro" variant="ghost-muted"` | 20x20, `rounded-sm` (6px), icon 12px |
| Launcher key hints | `Kbd` | `h-5 min-w-5` (20px), `px-1`, `rounded` (4px), `bg-sidebar-control-surface`, `text-xs font-medium text-muted-foreground`, `ring-1 ring-sidebar-border` (`components/ui/kbd.tsx:5-15`) |
| Add-menu shortcut hints | `MenuShortcut` | `h-4` (16px), `px-1.5`, `rounded-sm`, `bg-muted`, `text-[10px] font-medium text-secondary-label`, dark `ring-white/5` (`components/ui/menu.tsx:251-260`) |
| Tab close | `PanelTabCloseButton` | see section 6.4 (`components/ui/panel-tab-close-button.tsx`) |
| Tab strip | `ScrollArea radius="none" hideScrollbars scrollFade` | horizontal mask fade 24px (`--fade-size: 1.5rem`) on whichever edge has overflow, no scrollbars (`components/ui/scroll-area.tsx:57-70`) |
| Overlay mode | `Sheet` / `SheetPopup side="right"` | section 3.3 |

---

## 1. Surface model (`rightPanelStore.ts`)

### 1.1 Kinds and ids

`RIGHT_PANEL_KINDS = diff, files, file, preview, device, terminal, pull-request, pull-requests, agents`
(`rightPanelStore.ts:22-32`). The "plan" kind is gone (v9: plans render inline in the transcript,
`:91`). A surface is a tab.

| Kind | Id | Extra fields | Singleton | Source |
| --- | --- | --- | --- | --- |
| `preview` | `browser:<tabId>` or the placeholder `browser:new` | `resourceId: tabId \| null` | no | `:43-44,199-202` |
| `device` | `device` (picker, no target) or `device:<enc(hostId)>:<enc(deviceId)>` | `target?: {hostId, deviceId, platform: "ios"\|"android", name}`, `title?` | `device` is | `:35-45,519-541` |
| `terminal` | `terminal:<terminalId>` | `resourceId`, `terminalIds[]`, `activeTerminalId`, `splitDirection?: "horizontal"\|"vertical"` | no | `:46-53,225-231` |
| `diff` | `diff` | none | yes | `:54` |
| `files` | `files` | none | yes | `:55` |
| `file` | `file:<relativePath>` or `attachment:<attachmentId>` | `relativePath` (workspace-relative, or absolute host path), `revealLine: number\|null`, `revealRequestId`, `attachment?: ChatFileAttachment` | no | `:56-66,204-223` |
| `pull-request` | `pull-request:[<enc(envId)>:]<enc(projectId)>:[<enc(lowercase host)>:]<enc(repository)>:<number>` | `environmentId?`, `projectId`, `host?` (lowercased), `repository`, `number`, `url?` | no | `:67-85,235-268` |
| `pull-requests` | `pull-requests` | none | yes | `:87` |
| `agents` | `agents` | none | yes | `:88` |

`enc` is `encodeURIComponent`.

### 1.2 Thread state

```ts
ThreadRightPanelState = { isOpen: boolean; activeSurfaceId: string | null;
  surfaces: RightPanelSurface[]; dismissedDeviceSurfaceIds?: string[] }   // :109-114
```

- Keyed by `scopedThreadKey(ref)` (`<environmentId>:<threadId>`). Missing entries read as
  `{isOpen:false, activeSurfaceId:null, surfaces:[]}` (`:176-180`).
- An entry whose next state is closed, with no active surface, no surfaces and no dismissed devices is
  deleted (`updateThread`, `:282-301`).
- Session-only `userActionRevisionByThreadKey`: every action is a user choice and bumps the thread's
  revision by 1, except `openProactive`, `openDevice(..., automatic=true)`, `reconcileBrowserSurfaces`
  and `reconcileFileSurfaces` (`automaticUpdate`, `:303-342`).
- A user action that removes a `device` surface with a `target` records its id in
  `dismissedDeviceSurfaceIds`, so an automatic device open never resurrects it (`:319-337`, `:524`).

### 1.3 Actions (exact semantics)

`upsertSurface(current, surface, activate=true)` (`:270-280`): sets `isOpen=true`; appends the surface
only if its id is new (an existing tab keeps its position and its stored fields); activates it.

| Action | Lines | Behavior |
| --- | --- | --- |
| `open(kind)` | `:509-518` | `preview`: upsert the first existing preview surface, else `browser:new`. Other singletons: upsert the singleton. |
| `toggle(kind)` | `:837-852` | If open and the active surface has that kind: `isOpen=false`. Else as `open(kind)`. |
| `openProactive(surface, expectedRevision)` | `:486-508` | Only `diff`, `pull-request`, `pull-requests`. Refused (returns false) if the thread's revision changed since it was read, or if the surface is `diff` while the active kind is `pull-request`/`pull-requests`. Otherwise automatic upsert. |
| `openDevice(target, automatic=false)` | `:519-541` | id `device:<host>:<device>`. Automatic opens are skipped if the id is dismissed. If the id already exists the `device` picker tab is removed; otherwise the picker tab is replaced in place by the new surface. Removes the id from the dismissed list, then upserts. |
| `renameDevice(id, title)` | `:542-552` | `title = trim(title) \|\| target.name \|\| "Device"`. |
| `openBrowser(tabId)` | `:553-562` | Non-null `tabId` first drops `browser:new`, then upserts `browser:<tabId>`. |
| `openPullRequest(target)` | `:563-577` | Upsert; if `target.url` is given the stored surface is replaced so it carries the url. |
| `openFile(path, line?)` | `:578-611` | `"."` opens the `files` singleton. Otherwise strips trailing `/` (except a bare drive root `C:/`), removes the `files` explorer tab, reuses `file:<path>` in place or appends it, sets `revealLine` (finite -> `max(1, trunc)`, else null) and `revealRequestId = previous + 1`, activates, opens. |
| `openAttachment(attachment)` | `:612-623` | Removes the `files` explorer tab, upserts `attachment:<id>` (`relativePath = attachment.name`, `revealRequestId 0`). |
| `openTerminal(id)` | `:624-629` | Upsert `terminal:<id>` with `terminalIds:[id]`. |
| `splitTerminal(surfaceId, id, dir="horizontal")` | `:630-649` | Opens, activates the surface, appends `id` if new, makes it active; `splitDirection` becomes `"vertical"` only for vertical splits (a horizontal split deletes the field). |
| `activateTerminal(surfaceId, id)` | `:650-663` | Activates the surface; sets `activeTerminalId` if the id belongs to it. |
| `closeTerminal(surfaceId, id)` | `:664-702` | Removing the last id removes the surface (fallback below). Else if the closed id was active, the new active id is the last remaining one. |
| `activateSurface(id)` | `:703-710` | If it exists: `isOpen=true`, active = id. |
| `closeSurface(id)` | `:711-728` | Removes it. If it was active, the new active surface is `surfaces[min(index, len-1)]` of the remaining list (the right neighbour, else the new last). `isOpen` stays true only if surfaces remain. |
| `closeOtherSurfaces(id)` | `:729-741` | No-op for a lone tab. Keeps only `id`, active, open. |
| `closeSurfacesToRight(id)` | `:742-757` | No-op on the last tab. Truncates after `id`; if the active one was cut, `id` becomes active. |
| `closeAllSurfaces` | `:758-765` | Empties and closes. |
| `reconcileBrowserSurfaces(tabIds)` | `:766-794` | Automatic. Non-preview surfaces first, then known preview tabs still alive (old order), then new tab ids. Drops `browser:new` and dead tabs. Active falls back to the first preview, else the first surface. Note the reorder: browsers move after every other kind. |
| `reconcileFileSurfaces(workspaceAvailable)` | `:795-817` | Automatic. Without a workspace, removes `files` and non-attachment `file` tabs; closes the panel if nothing remains; active falls back to the last surface. |
| `show` / `close` / `toggleVisibility` | `:818-836` | Flip `isOpen` only; tabs survive a close, so reopening restores them. |
| `removeThread` | `:853-866` | Drops the thread's entry and revision. |

Selectors (`:886-919`): `selectActiveRightPanel` and `selectActiveRightPanelSurface` return null when
`!isOpen`; `selectSelectedRightPanelSurface` returns the selected surface even while hidden.

### 1.4 Persistence

- localStorage key `t3code:right-panel-state:v2`, version 13 (`:90-95`). Only `byThreadKey` persists;
  the revision map is session-only. Threads whose key ends with `:pull-requests-panel` (the PR list
  page's shared panel, `PULL_REQUESTS_PANEL_REF`, `:97-107`) are never persisted (`:874-880`).
- T3UI: persist `byThreadKey` per thread to `ui-state.json` (or a sibling file). The migration
  (`:349-477`) only matters for importing fork state; a fresh native app can skip it, but keep its
  sanitizing rules when reading our own file: drop unknown kinds, clamp `revealLine`, drop malformed
  `pull-request`/`terminal` entries, and when the active id is gone fall back to the first surface
  (open) or null (closed); never restore an open panel with zero surfaces (`:445-456`).

---

## 2. ChatView wiring (`components/ChatView.tsx`)

### 2.1 Availability per kind

Passed to the tab bar's add menu and empty-state launcher (`ChatView.tsx:10410-10425`):

| Kind | Available when | Source |
| --- | --- | --- |
| Browser | `isPreviewSupportedInRuntime()` = `window.desktopBridge.preview` exists (Electron only) | `previewStateStore.ts:462-465` |
| Terminal | thread has a project | `ChatView.tsx:10413` |
| Files | thread has a project | `:10415` |
| Diff | server thread (not a draft) and the cwd is a git repo | `:10414` |
| Pull request | env capability `pullRequests` and the thread has `linkedPullRequest ?? branchPullRequest` | `:6107` |
| Linked pull requests | server thread, capability `threadPullRequests`, at least one visible linked PR (`visibleThreadPullRequests`) | `:4652-4660` |
| Agents | always | `:10422` |
| Device | always when there is a thread ref | `:10423` |

The panel toggle itself is enabled only when the thread has a project (`rightPanelAvailable`,
`:9587`); the tooltip then reads `Right panel is unavailable`.

### 2.2 Add handlers (what each launcher row / menu item does)

| Item | Handler | Effect |
| --- | --- | --- |
| Browser | `createBrowserSurface(profileId?)` `:4616-4637` | RPC `preview.open` (via `openPreviewSession`), then `openBrowser(snapshot.tabId)` (`components/preview/addBrowserSurface.ts:9-27`). On `BrowserSettingsReadError` an error toast `Unable to open browser` with the error message. |
| Terminal | `addTerminalSurface` `:4972-4999` | `nextTerminalId` over the drawer's known ids plus every panel terminal id (lowest free `term-N`, `shared:terminalLabels.ts:27-37`), `openTerminal(id)`, focus request, RPC `terminal.open {threadId, terminalId, cwd: gitCwd ?? workspaceRoot, worktreePath?, env}` (see panels-terminal.md). |
| Files | `addFilesSurface` `:4645-4648` | `open("files")`. |
| Diff | `addDiffSurface` `:4639-4644` | `diffPanelStore.selectGitScope(ref, "unstaged")`, `open("diff")` (see panels-diff.md). |
| Pull request | `addPullRequestSurface` `:6102-6106` | `openPullRequest(linkedPullRequest ?? branchPullRequest)`. |
| Linked pull requests | `addPullRequestsSurface` `:4661-4664` | `open("pull-requests")`. |
| Agents | `addAgentsSurface` `:4649-4652` | `open("agents")`. |
| Device | `addDeviceSurface` `:4669-4676` | If device onboarding is not completed or the host is disabled, opens the device setup wizard dialog instead. Else `open("device")`. |

### 2.3 Toggle, close, maximize

- Panel toggle (button or `rightPanel.toggle`): if open, `closePreviewPanel()`; else
  `toggleVisibility` (`:5104-5111`).
- `closePreviewPanel()` (`:4937-4952`): if the active surface is a live browser tab or a device with a
  target, it is floated into the mini player first (`previewMiniPlayerStore.open`); clears maximize;
  `close(ref)`.
- Activating a tab (`:5088-5103`): `activateSurface`; preview -> also make it the active preview tab;
  terminal -> bump the terminal focus request; diff when not already showing -> `onDiffPanelOpen`.
- Closing a tab (`closeRightPanelSurface`, `:5192-5222`):
  - preview: if the tab is agent-controlled, a native destructive confirm first
    (`agentControlledBrowserCloseConfirmation`, `:5145-5162`).
  - terminal: `confirmTerminalClose([activeLabel, ...otherLabels])` first (panels-terminal.md).
  - then `cleanupRightPanelSurfaces` (`:5118-5144`): preview -> RPC `preview.close`; terminal -> for each
    id close it in the terminal store and RPC `terminal.close {threadId, terminalId, deleteHistory:
    true}`; then `closeSurface` per surface and re-sync the active preview tab.
  - Close others / to the right / all collect the affected surfaces and go through the same confirm and
    cleanup (`:5223-5262`). Close all therefore closes the panel via `closeSurface` emptying it.
- Copy path (tab context menu, `:5263-5290`): clipboard write; success toast `Path copied` with the
  path as description; failure toast `Failed to copy path` / `Clipboard API unavailable.` or the error
  message.
- Maximize (`:1741`, `:2096-2099`, `:5112-5117`): React state `maximizedRightPanelThreadKey` (not
  persisted, one thread at a time). Only possible while the panel is open and inline. Toggling sets or
  clears it for the current route thread. Closing via `closePreviewPanel` clears it.

### 2.4 Keyboard (defaults in `shared:keybindings.ts:25-63`, handled at `ChatView.tsx:6871-6961`)

| Command | Default (macOS) | When | Effect |
| --- | --- | --- | --- |
| `rightPanel.toggle` | `⌥⌘B` (`mod+alt+b`) | always | toggle as 2.3 |
| `rightPanel.toggleMaximized` | none | always | toggle maximize |
| `rightPanel.close` | `⌘W` | `!terminalFocus` | if a surface is active, close it like its tab X (no repeat); if nothing is open the event is left alone (native close window) |
| `diff.toggle` | `⌘D` | `!terminalFocus` | server threads only: `toggle("diff")` (+ `onDiffPanelOpen` when opening) (`:3864-3874`) |
| `terminal.toggle` | `⌘J` | always | drawer (panels-terminal.md) |
| `terminal.split` / `splitVertical` / `new` / `close` | `⌘D` / `⇧⌘D` / `⌘N` / `⌘W` | `terminalFocus` | act on the panel terminal when focus is in a right-panel terminal, else the drawer |
| `preview.toggle` | `⇧⌘J` | always | browser surface toggle (appendix A) |
| `filePicker.toggle` | `⌘P` | `!terminalFocus` | panels-files.md |

Shortcut labels are produced by `shortcutLabelForCommand` (symbols order ⌃⌥⇧⌘ then the key).

### 2.5 Automatic opens

- `proactivePanelsEnabled` client setting, default false (`contracts:settings.ts:461`), and never
  in sheet mode. When on (`ChatView.tsx:4800-4936`):
  - A new or changed set of linked PRs opens `pull-requests` (more than one, or no PR detail, or no
    `pullRequests` capability) else the single `pull-request` surface, via `openProactive`.
  - A turn that just completed opens `diff` (with git scope `unstaged`) when its checkpoint is `ready`,
    the cwd is a git repo, and it touched >= 3 files or >= 50 changed lines
    (`ChatView.logic.ts:169-202`). Not when a PR target exists.
  - A user panel choice made after the turn started (revision changed) refuses both
    (`observeProactivePanelUserChoice`, `ChatView.logic.ts:134-149`).
- Independent of the setting: when the thread's linked PR changes while the panel shows the previous
  linked PR, the panel retargets to the new one (`shouldRetargetThreadPullRequestPanel`,
  `ChatView.logic.ts:152-167`).
- Device sessions that appear for this thread (opened by the agent or another client) open a device
  tab automatically (`openDevice(target, automatic=true)`), or float in the mini player when
  `autoShowFloatingPreview` is on; never in sheet mode; the first snapshot is a baseline
  (`ChatView.tsx:4677-4736`).
- Preview sessions: `reconcileBrowserSurfaces(Object.keys(previewState.sessions))` on every change
  (`:2101-2106`).

---

## 3. Layout modes

`shouldUseRightPanelSheet = matchMedia("(max-width: 980px)")` (`rightPanelLayout.ts:1`,
`ChatView.tsx:1765`).

### 3.1 Inline (window > 980px)

Box tree of `ChatView`'s root (`ChatView.tsx:9817-10428`):

```
div.relative.flex.min-h-0.min-w-0.flex-1.overflow-hidden.bg-background        (row)
├─ [panelLayoutControls when the panel is present and inline]                (fixed, section 5)
├─ div  chat column wrapper: flex min-h-0 min-w-0 flex-col overflow-x-hidden
│       flex-1, or "w-0 flex-none" when maximized (data-chat-column-maximized-away)
│   ├─ WorkspacePageHeader (52px, ChatHeader inside; chat spec)
│   ├─ div.flex.min-h-0.min-w-0.flex-1  > chat column (timeline + composer)
│   └─ PersistentThreadTerminalDrawer per mounted thread (panels-terminal.md)
└─ RightPanelTabs mode="inline"  (PreviewPanelShell, section 4)
```

The terminal drawer sits inside the chat column wrapper, so the right panel spans the full window
height beside both the chat and the drawer.

### 3.2 Inline maximized

- Chat column wrapper becomes `w-0 flex-none` (fully hidden but mounted).
- Panel shell becomes `flex-1 border-l border-border`, `width: 100%`, no resize handle, no clip wrapper
  width (`PreviewPanelShell.tsx:121-149`).
- Tab bar gets `COLLAPSED_SIDEBAR_TITLEBAR_INSET_CLASS`: when the app sidebar is collapsed, left padding
  `--workspace-titlebar-content-left` = controls-left + 28 + 12 (122px on a windowed Mac) so tabs clear
  the traffic lights and the sidebar trigger (`workspaceTitlebar.ts:1-2`, `components/ui/sidebar.tsx:169-170`).

### 3.3 Sheet (window <= 980px)

- `RightPanelSheet` (`components/RightPanelSheet.tsx:6-31`): Base UI Dialog as a right sheet,
  `keepMounted`, no close button.
  - Backdrop: fixed inset-0, `z-(--z-sheet)` (46), `bg-background/60` + `backdrop-blur-xs` (4px; GPUI
    has no blur, use the 60% tint only). Click closes (calls `closePreviewPanel`).
  - Popup: full height, `bg-popover`, `border-s` (left border, `--border`), `shadow-lg/5`, width
    `w-[min(42vw,28rem)] min-w-80 max-w-[28rem]`: 42% of the window, at least 320px, at most 448px
    (353px at 840px wide, 412px at 980px). Edge bevel: light `0 1px black/4%`, dark `0 -1px white/6%`
    (`components/ui/sheet.tsx:57-105`).
  - Transition: opacity 0 -> 1 and translate-x 32px -> 0, `transition-[opacity,translate]` ease-in-out,
    duration = the panel animation duration when active, else 0 (instant). Backdrop fades the same way.
- `RightPanelTabs mode="sheet"`: the panel layout toggles render inside the tab bar's right end
  (`layoutControls = <div class="mr-px flex items-center">{toggles}</div>`, `ChatView.tsx:10436-10446`)
  instead of fixed at the window corner; no maximize control. Tab bar keeps the 52px height.
- Proactive panels and automatic device tabs are off in this mode.

---

## 4. Shell: width, resize, open/close (`components/preview/PreviewPanelShell.tsx`)

### 4.1 Width

- Per-thread storage key `t3code:preview-panel-width:<threadKey>` (`ChatView.tsx:10386`). Default 540px,
  min 360px (`PreviewPanelShell.tsx:17-25`).
- Max = `max(360, min(floor(0.7 * viewportWidth), floor(rowWidth) - 360))`, where `rowWidth` is the
  `clientWidth` of the panel's parent row (the ChatView root row, so the app sidebar is excluded). The
  360px reserve keeps the chat column usable (`:33-44`). Re-measured on window resize (rAF-coalesced)
  and by a ResizeObserver on the row (`:166-206`); measured before first paint so a stored width never
  flashes too wide.
- Effective width = `clamp(stored ?? 540, 360, max)`; a non-finite stored value reads as 540
  (`hooks/useResizableWidth.ts:52-77`). The clamp is applied every render, but storage is written only
  at drag end, so shrinking the window does not overwrite the stored width.
- Shell box (inline, not maximized): `relative flex h-full min-h-0 min-w-0 max-w-full flex-col
  self-stretch bg-background shrink-0 border-l border-border`, style `width: <width>px` (0px while
  closed-but-present). Inner clip `div.h-full.min-h-0.w-full.overflow-clip`, then the content column at
  fixed `width: calc(<width>px - 1px)` so content never reflows during the width animation (`:118-153`).
  Attributes `data-preview-panel-mode="inline"`, `data-preview-panel-maximized`.

### 4.2 Resize handle (`components/preview/RightPanelResizeHandle.tsx:17-33`)

- `role="separator"`, absolutely positioned `inset-y-0 -left-1 w-2` (8px wide, centered on the panel's
  left border, 4px overlap each side), `z-20`, `cursor-col-resize`, `select-none`.
- Indicator: a centered 1px line, transparent at rest, `--border` on hover, `--primary` at 60% alpha
  while pressed, `transition-colors 150ms`.
- Drag (`useResizeDrag`, edge "left"): pointer capture; width follows the pointer live
  (`startWidth + (startX - x)`), clamped every move; persisted once on pointer up / cancel / lost
  capture. Not rendered when maximized or in sheet mode.

### 4.3 Open/close

- Presence: `usePanelPresence(open, value, animated, threadKey, duration)` keeps the last surfaces
  rendered while a close animates (`panelAnimations.ts:83-148`). With the default 0ms the panel unmounts
  on close immediately.
- Animated path only (duration > 0): width transitions `0 -> width` on open (`starting:w-0`) and
  `width -> 0` on close, `ease-out`, duration `--panel-animation-duration`; pointer events off while
  closing; maximize and drag-resize never animate (the transition is suppressed for two frames,
  `PreviewPanelShell.tsx:81-117`). The timeline is told `layoutWidthTransitioning` while it runs.
- Switching threads restores the destination thread's width before paint (`useResizableWidth.ts:71-75`).

---

## 5. Titlebar layout controls (`components/chat/PanelLayoutControls.tsx`)

### 5.1 Placement

`panelLayoutControls` (`ChatView.tsx:9599-9634`):

```
div  fixed top-0 right-[12px] z-50 mr-px  h-52  flex items-center gap-1  pointer-events-none  no-drag
│    (when the panel is present + inline: isolate, plus a ::before from the group's left edge to the
│     window's right edge (right: -13px), bg-background, z -10: tabs scroll under a solid surface)
├─ span (inline only): maximize control, shown only while the panel is open and not transitioning
│     (hidden, inert otherwise; opacity transition with the panel duration when animations are active)
└─ div.pointer-events-auto.flex.h-full.items-center  > PanelLayoutControls
      div.flex.h-full.shrink-0.items-center.gap-1  [data-panel-layout-controls]
      ├─ terminal Toggle (28x28)
      └─ right-panel Toggle (28x28)
```

- Right edge of the right-panel toggle = window right - 13px. Group width = 28+4+28 (+4+28 with the
  maximize control).
- Where it mounts: inside `WorkspacePageHeader` while the panel is absent (`ChatView.tsx:9864`); at the
  ChatView root while the panel is present inline (`:9842`); inside the sheet's tab bar in sheet mode.
  It is `fixed` in all inline cases, so it never moves.
- The chat header reserves room for it while the panel is closed: header actions get `pr-14.25`
  (57px = two toggles + gap + 1px) on top of the header's 20px right gutter; `pr-0` while the panel
  is open (`components/chat/ChatHeader.tsx:500-504`). The open panel's tab bar instead reserves a
  hit-area spacer of three 28px boxes with 4px gaps and `mr-px` (93px) after its own content
  (`ChatView.tsx:9636-9645`), plus 12px `pr-3`.
- Electron also places a fixed 112px-wide (`w-28`) no-drag rectangle at the top right so the controls
  stay clickable in the drag region (`ChatView.tsx:9858-9862`, `RightPanelTabs.tsx:1409-1414`). In GPUI
  this is just "the titlebar drag area excludes these controls".

### 5.2 Terminal toggle

- `Toggle variant="panel" size="panel"`, `pressed = drawer open`, disabled when the thread has no
  project. Icon 16px: `PanelBottomClose` when open, `PanelBottom` when closed (lucide).
- `aria-label`: `Toggle terminal drawer`.
- Tooltip (side bottom): `Toggle terminal drawer (⌘J)` (label omitted when unbound:
  `Toggle terminal drawer`); disabled: `Terminal drawer is unavailable`. The tooltip trigger wraps the
  toggle in a `span.flex.shrink-0` so a disabled toggle still shows it (`PanelLayoutControls.tsx:46-69`).

### 5.3 Right-panel toggle

- Same primitive. `pressed = panel open`. Icon `PanelRightClose` when open, `PanelRight` when closed.
- Live-agent badge when `liveAgentCount > 0` (count of running + pending + waiting subagents, section
  9; forced to 0 while the Agents surface is the active, visible surface, `ChatView.tsx:9589-9593`):
  `absolute -top-1 -right-1` (-4px), `h-3.5 min-w-3.5` (14px), `rounded-full`, `bg-info`, `px-1`,
  `text-3xs` (10px/14px) `font-semibold tabular-nums text-white`, centered digits.
- `aria-label`: `Toggle right panel` or `Toggle right panel, 1 agent working` /
  `Toggle right panel, N agents working`.
- Tooltip (bottom): `Toggle right panel (⌥⌘B)` + ` · N agent(s) working` when badged; disabled:
  `Right panel is unavailable` (`:70-111`).
- With a named theme active (`html[data-theme-id]`), both toggles use `--contrast-toolbar-foreground`,
  and disabled ones mix it 55% with `--toolbar-background` (`index.css:1379-1400`).

### 5.4 Maximize control (`RightPanelMaximizeControl`, `PanelLayoutControls.tsx:115-146`)

- `Toggle variant="ghost" size="sm"` (28x28), pressed when maximized. Icon `Minimize2` when maximized,
  `Maximize2` otherwise (16px).
- `aria-label` and tooltip (bottom): `Restore panel size` when maximized, else `Maximize panel`.
- Shown only inline while the panel is open (left of the terminal toggle).

---

## 6. Tab bar (`components/RightPanelTabs.tsx`)

### 6.1 Box

```
PreviewPanelShell
├─ div [data-right-panel-tabbar]  h-52 min-h-52 shrink-0 flex items-center gap-1 pl-2 (8px)
│      pr-3 (12px) when layoutControls are passed, else pr-28 (112px) for inline-without-controls
│      inline + Electron: drag-region (window drag), children no-drag
│      maximized inline: + collapsed-sidebar left inset (3.2)
│   ├─ ScrollArea [data-right-panel-tab-list] min-w-0 flex-1, no scrollbars, horizontal fade 24px
│   │   └─ div.flex.h-full.w-max.min-w-full.items-center.gap-1  (4px between tabs)
│   │       ├─ tab × N                      (6.2)
│   │       └─ "+" add-surface menu trigger (6.6), only when at least one tab exists
│   ├─ overflow arrows group (6.7), only when the strip overflows
│   ├─ layoutControls (inline: 93px hit-area spacer; sheet: the real toggles)
│   └─ (inline + Electron) fixed 112px no-drag rect at the window's top right
└─ div [data-right-panel-surface-content] flex min-h-0 flex-1 flex-col
    └─ empty-state launcher (section 7) when activeSurfaceId is null, else the active surface
```

There is no bottom border on the tab bar; surfaces start directly under it (see section 8).

### 6.2 Tab

`div.group/tab` (`RightPanelTabs.tsx:1147-1268`):

- `relative flex h-6 max-w-36 shrink-0 items-center gap-0.5 rounded-md pr-2 pl-1.5 text-xs
  cursor-pointer`: 24px tall, max 144px wide, 8px radius, padding left 6px / right 8px, 2px gaps,
  12/16 text. No fixed min width: a tab hugs its title.
- CSS var `--right-panel-tab-background`: active `var(--accent)`; inactive
  `color-mix(in oklab, var(--accent) 60%, var(--background))`.
- Active: `bg-(--right-panel-tab-background)`, `text-foreground`.
- Inactive: transparent, `text-muted-foreground`; hover fills with the inactive tab background and
  `text-foreground`. No transition.
- `data-active-tab`. Electron inline: `no-drag`.
- Children, in order:
  1. Icon box `span.relative.flex.size-4` (16px) holding the 12px kind icon (6.3). When the surface id
     is in `pendingSurfaceIds` (a file tab with unsaved edits, panels-files.md), a dot sits at its
     bottom-right: `absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full bg-current` (6px, the tab's
     text color).
  2. Preview audio button (preview tabs only, when the page is audible and the desktop runtime tab id
     resolves): 16x16 `rounded-sm`, `hover:bg-muted`, `Volume2` / `VolumeOff` 12px; `aria-label`
     `Mute <title>` / `Unmute <title>`; tooltip `Mute tab` / `Unmute tab`; click toggles mute without
     activating the tab. A muted tab that is silent shows nothing (`:236-245`).
  3. Title button `flex min-w-0 flex-1 items-center` with `span.truncate` (ellipsis); click activates.
     Double-click on a device tab starts rename. Tooltip (top, 600ms): the title; device tabs show a
     two-line tooltip: title, then `<host label or "Device host"> · <device version or "iOS"/"Android">`
     in `text-muted-foreground` (`:1447-1469`).
     - Rename mode (device only): `input` `w-24` (96px) `min-w-0 rounded-sm bg-background px-1
       ring-1 ring-ring`, `aria-label="Device tab name"`, focused with text selected. Enter commits
       (blur), Escape restores the old title then blurs, blur commits via `renameDevice`; key events
       don't propagate.
  4. Close button (6.4).
- Middle click (button 1) closes the tab; its mousedown default is prevented (no autoscroll).
- Right click opens the native context menu (6.5).
- When the strip overflows and the active tab changes, the active tab scrolls into view
  (`block/inline: nearest`, `:1057-1061`).

### 6.3 Titles and icons per kind (`surfaceTitle` `:609-647`, `SurfaceIcon` `:668-728`)

All icons 12px (`size-3`), `shrink-0`, lucide unless noted, color inherits the tab text color.

| Kind | Title | Icon |
| --- | --- | --- |
| diff | `Diff` | `FileDiff` |
| files | `Files` | `Files` |
| file | basename of `relativePath` (after the last `/` or `\`) | Pierre file-type icon for the path (`PierreEntryIcon kind="file"`, themed light/dark; panels-files.md) |
| terminal | server label of `activeTerminalId`, else `Terminal N` for `term-N` / `terminal-N`, else the raw id (`shared:terminalLabels.ts:4-23`) | `TerminalSquare` |
| pull-request | `#<number>` | state glyph with state tone (below) |
| pull-requests | `Pull requests` | `Link2` (`PullRequestGlyph.link`) |
| agents | `Agents` | `Bot` |
| device | `title ?? target.name ?? "Device"` | `AppleIcon` (ios) / `AndroidIcon` (android) from `components/Icons.tsx`, else `Smartphone` |
| preview | `Browser` while the session is missing or `Idle`; else the page title if non-blank; else the URL host; else `Browser` | favicon 12px `rounded-sm object-contain`: captured favicon when its page URL has the same origin as the tab URL, else the public favicon service for the origin at 32px, else `Globe2` (`:649-666,684-691`) |

Pull-request tab icon (`PullRequestSurfaceIcon`, `:759-826`): status = linked thread PR snapshot
(when capability `threadPullRequests`) else the newest of PR detail and the shared summary, else the
seed. No status: `GitPullRequestArrow` in `text-muted-foreground`. Else by
`resolvePullRequestState` (`components/pullRequest/pullRequestPresentation.tsx:123-129`, table at
`pullRequestIcons.tsx`): open `GitPullRequestArrow` `text-emerald-600 dark:text-emerald-300/90`;
draft (open + isDraft) `GitPullRequestDraft` `text-zinc-500 dark:text-zinc-400/80`; closed
`GitPullRequestClosed` `text-red-600 dark:text-red-300/90`; merged `GitMerge`
`text-violet-600 dark:text-violet-300/90`. The tab never shows conflict state.

### 6.4 Close button (`components/ui/panel-tab-close-button.tsx:12-43`, no-children variant)

- `absolute right-1 z-10` (4px from the tab's right edge), 16x16, `rounded-sm`, `X` 12px.
- Hidden (`opacity-0`) until the tab is hovered (`group-hover/tab:opacity-100`) or the button has
  keyboard focus; `transition-opacity 150ms ease-out` (none with reduced motion).
- Behind the X it paints a fade so the title is covered, not clipped: `::before` 44x24 (`w-11 h-6`),
  anchored `-right-1` and vertically centered, `rounded-r-md`, linear gradient left->right
  `transparent -> --right-panel-tab-background -> --right-panel-tab-background`; `::after` 20x24
  solid `--right-panel-tab-background`, same anchor. Both use the tab's own background var, so on an
  inactive tab the fade is the inactive-hover color (the tab itself is filled on hover anyway).
- `aria-label`: `Close <title>`. No tooltip. Click closes via `onCloseSurface` (section 2.3 flow).

### 6.5 Tab context menu (native, `:948-1042`)

Items in order (no separators):

1. `Rename` (device tabs only).
2. `Copy path` (file tabs that are not attachments).
3. `Mute tab` / `Unmute tab` (preview tabs; disabled until desktop overlay state exists for the tab).
4. `Close`
5. `Close others` (disabled when there is one tab)
6. `Close to the right` (disabled on the last tab)
7. `Close all` (disabled with no tabs)

### 6.6 Add-surface menu ("+")

- Trigger: `Button size="icon-xs" variant="ghost-muted"`, `Plus` 14px, `aria-label="Add panel surface"`,
  sits right after the last tab inside the scrolling strip. Shown only when there is at least one tab.
- Popup: `MenuPopup align="start" side="bottom" sideOffset={6}`. Items in order, each with its lucide
  icon (menu icon size) and a `MenuShortcut` letter at the far right:

| Label | Icon | Key |
| --- | --- | --- |
| `Browser` | `Globe2` | B |
| `Terminal` | `TerminalSquare` | T |
| `Files` | `Files` | F |
| `Diff` | `FileDiff` | D |
| `Pull request` | `GitPullRequestArrow` | P |
| `Linked pull requests` | `Link2` | L |
| `Agents` | `Bot` | A |
| `Device` | `Smartphone` | M |

- Unavailable items are disabled (`aria-disabled`) but keep pointer events so a tooltip (side top)
  explains why. Exact reasons (`:157-166`):
  - Browser: `Browser previews are only available in the T3 Code desktop app.`
  - Terminal: `Terminal surfaces are only available from a project thread.`
  - Files: `Files are only available when a project is open.`
  - Diff: `Diff is only available for server threads in Git repositories.`
  - Pull request: `This thread's branch has no pull request yet.`
  - Linked pull requests: `No linked pull requests are available for this thread.`
  - Agents: `Agents are only available from a thread.`
  - Device: `Devices are only available from a thread.`
- Browser, when available, is a submenu trigger: a mouse click opens the default browser profile and
  closes the menu; hovering (or arrow right) reveals a submenu of profiles (`max-w-56`, 224px, names
  truncated), each opening a tab in that profile. Touch: the first tap only opens the submenu.
- While the popup is open, pressing a letter (no Cmd/Ctrl/Alt, not composing) runs the matching
  available item and closes the menu (`:939-946`, `surfaceShortcutActionForKey` `:252-262`).
- Keyboard otherwise follows the Menu primitive (arrows, Enter, Escape).

### 6.7 Overflow

- Overflow = `scrollWidth - clientWidth > 1px`; left/right availability with a 1px tolerance
  (`:841-860`), recomputed on scroll and on resize of the viewport or its content.
- When overflowing, a group `flex shrink-0 items-center gap-0.5` (2px) appears after the strip with two
  `Button size="icon-xs" variant="ghost"` arrows: `ChevronLeft` (`aria-label` and tooltip
  `Scroll tabs left`) and `ChevronRight` (`Scroll tabs right`), each disabled at its end. Click scrolls
  by `max(120px, 75% of the visible width)`, smooth unless reduced motion (`:862-870,1360-1403`).
- Vertical wheel over the strip scrolls it horizontally (the dominant axis delta; line mode x16px,
  page mode x width); ignored with Ctrl held; the event is consumed only if the strip actually moved
  (`:1080-1100`).
- Edge fade: the ScrollArea masks 24px at an edge that has more content beyond it.

---

## 7. Empty-state launcher (`RightPanelEmptyState`, `RightPanelTabs.tsx:312-607`)

Shown in the content area whenever `activeSurfaceId` is null (an open panel with no tabs, e.g. first
open via the toggle).

- Container: focusable (`tabIndex 0`, focused on mount, no outline), `aria-label="Open a surface"`,
  `flex min-h-0 flex-1 items-center justify-center overflow-y-auto px-6` (24px) with bottom padding 52px
  so the list centers against the whole panel height including the tab bar.
- Inner `w-full max-w-xs` (320px) `py-6` (24px):
  - Heading `Open a surface`: `mb-3` (12px), centered, `text-sm font-medium text-foreground`.
  - List `flex flex-col gap-0.5` (2px) of rows in this order: Browser (B), Terminal (T), Files (F),
    Diff (D), Pull request (P), Linked pull requests (L), Agents (A), Device (M). Same icons as 6.6.
- Available row: wrapper `div.group.relative`; button `flex h-8 w-full items-center gap-2.5 rounded-
  [8px] px-2.5 text-left text-sm cursor-pointer transition-colors`, hover (on the wrapper) or
  keyboard highlight `bg-accent/60`. Contents: 16px icon (Agents gets the live-agent badge:
  `absolute -top-1.5 -right-2`, otherwise identical to the toggle badge in 5.3), label
  `min-w-0 flex-1 truncate`, then `Kbd` with the letter.
- Browser with more than one profile: label gets `pr-7`, and a `Button size="icon-xs"
  variant="ghost-muted"` with `ChevronDown` 14px sits `absolute right-8 top-1/2 -translate-y-1/2`
  (`aria-label="Open browser in a profile"`) opening a profile menu (`align end, side bottom, offset 6,
  max-w-56`).
- Unavailable row: same box with `opacity-50`, `cursor-default`, `aria-disabled`, focusable, tooltip
  (top) with the one-line hint (`:181-190`):
  - Browser `Only available in the desktop app.`, Terminal `Available when a project is open.`,
    Files `Available when a project is open.`, Diff `Available for Git repositories.`, Pull request
    `No pull request on this branch yet.`, Linked pull requests `No linked pull requests available.`,
    Agents `Available from a thread.`, Device `Available from a thread.`
- Highlight: starts at -1 (none). Hover sets it to that row; leaving clears it. Arrow Down/Right moves
  to the next available row (wrapping), Up/Left to the previous (from none: the last). Enter, only
  while the launcher itself is focused, runs the highlighted row.
- Global letters: while the launcher is mounted, a capture-phase window keydown runs the matching
  available row for B/T/F/D/P/L/A/M (case-insensitive) unless a modifier (Cmd/Ctrl/Alt) is held,
  the event is composing or already handled, a dialog/menu/select/popover/combobox/autocomplete popup
  is open, or the focused element is an input, textarea, select or contenteditable (`:425-446`,
  `:168-178`, `:264-279`). The data attribute `data-surface-launcher-keys` lists the active letters;
  `ChatView` uses it to let those keys through its own handlers (`ChatView.tsx:700`).
- No animation.

---

## 8. Surface subheader convention

Every surface that has a toolbar row under the tab bar uses the same class pattern, marked
`data-surface-subheader` (`DiffPanelShell.tsx:17`, `PreviewChromeRow.tsx:118`,
`files/fileSurfaceChrome.tsx:17`, `diffs/DiffFileTree.tsx:169`, `files/FileBrowserPanel.tsx:491`):

| Mode | Row |
| --- | --- |
| Inline panel (desktop default, `data-preview-panel-mode="inline"` ancestor) | `h-7` (28px), `mb-3` (12px below), no visible border (`border-b-transparent`), `bg-background` |
| Sheet / embedded elsewhere | `h-10` (40px), `border-b border-border/60`, `bg-background` |

The Files explorer row is the exception: inline `h-9` (36px) with `mb-1` (4px). Horizontal padding and
contents are per surface (sibling files).

---

## 9. Agents surface (`components/AgentsPanel.tsx`)

The fleet view of the thread's subagents and workflows. Also reachable from the composer banner
`N agents working` -> `View` (`ChatView.tsx:6364-6411`) and the timeline's agent rows (chat spec).

### 9.1 Data

- `agentPanelModel = deriveAgentPanelModel({ agents: foldSubagentActivities(threadActivities,
  { sessionLive: phase !== "disconnected" }) })` (`ChatView.tsx:2960-2970`). Pure functions in
  `crt:state/subagentRuntime.ts`: port them verbatim (fold `:120-680`, derive `:732-866`,
  formatters `:869-892`).
- Input: the thread's `OrchestrationThreadActivity` rows of kinds `task.started`, `task.progress`,
  `task.updated`, `task.completed` and related `tool.*` rows (payload `taskId`, `agentKind`,
  `status`, `usage`, `detail`, ...). Rows without `payload.agentKind === "agent"` are background work
  and never join the roster (`:119-121`). T3UI has the rows (`t3_protocol::OrchestrationThreadActivity`
  with `payload: Value`, `crates/t3-protocol/src/orchestration.rs:588-598`).
- Model: `workflows[]` (each `{workflow, phases[{index,title,members,state: pending|running|done,
  activeCount, settledCount}], unphasedMembers}`), `directAgents[]` (sorted by `firstSeenAt`, id),
  `runningCount` (running + pending), `waitingCount`, `idleCount`, `settledCount`, `totalTokens`,
  `hasAgents`, `liveCount = running + waiting`. A workflow coordinator with members is not counted.
  Roster capped at 100 agents.
- Token format (`formatSubagentTokenCount`): `< 1000` -> integer; `< 1M` -> `x.yk` (`>= 100k` rounds to
  an integer `k`); else `x.yM`. Model label: strips `claude-`, a trailing `-YYYYMMDD` and `-latest`;
  appends ` · <effort>`.
- Workflow script viewer: RPC `orchestration.getWorkflowScript` `{threadId, scriptPath}` ->
  `{scriptPath, contents, truncated}` (upstream `contracts:orchestration.ts:2324-2337`,
  `rpc.ts:1339-1342`). Missing in T3UI.

### 9.2 Empty state (`!hasAgents`, `:533-544`)

Centered column `flex h-full flex-col items-center justify-center gap-2 p-6 text-center`:
`Bot` 24px `text-muted-foreground/60`; `No agents yet` (`text-sm font-medium`);
`When this thread spawns subagents or runs a workflow, they show up here with live status, activity,
and token usage.` (`max-w-56` 224px, `text-xs text-muted-foreground`).

### 9.3 Layout

```
div.flex.h-full.min-h-0.flex-col
├─ ScrollArea min-h-0 flex-1  > div.flex.flex-col.gap-2.p-2   (8px gaps, 8px padding)
│   ├─ WorkflowSection × workflows (expanded or collapsed)
│   └─ section "Direct spawns" (when any): header + AgentRow × directAgents
└─ footer  flex items-center justify-between border-t border-border/60 px-3 py-1.5
           font-mono text-2xs text-muted-foreground   (12px x 6px padding)
    ├─ left (gap-2): "● N working" (text-info-foreground, when running+waiting > 0),
    │                "N idle" (when > 0), "N settled" (when > 0)
    └─ right: "Σ <tokens> tok" (tabular-nums)
```

Section header `Direct spawns`: `px-1.5 pt-1 text-3xs font-medium uppercase tracking-wider
text-muted-foreground`.

### 9.4 Agent row (`AgentRow`, `:139-192`)

Fixed height so live updates never move rows: grid `h-[3.875rem]` (62px), columns
`[6px | minmax(0,1fr) | auto]`, rows `[20px | 18px | 16px]`, `gap-x-2` (8px), `rounded-md`, `px-1.5`
(6px) `py-1` (4px). Not interactive (no hover).

- Row 1, col 1: status dot 6px round (colors below).
- Row 1, col 2: title (`text-sm font-medium`, truncate) and, when the role differs from the title
  (case-insensitive), a role chip: `max-w-28` (112px) truncate, `rounded-sm border border-border/60
  px-1 font-mono text-3xs text-muted-foreground`; `items-baseline gap-2`.
- Row 1, col 3: `min-w-14` (56px) right-aligned `font-mono text-2xs text-muted-foreground/80`: elapsed
  time plus a `Check` 12px `text-success` when completed.
  - Elapsed: `Ns`, `Mm SSs`, `Hh MMm` (`:60-71`). Live (running/waiting) ticks every 1s from
    `startedAt`; settled freezes at `completedAt`; nothing without `startedAt`. A 1Hz text update is
    the only repaint; stop it offscreen.
- Row 2 (cols 2-3): activity line `text-xs` truncate, `text-destructive-foreground` when failed, else
  `text-muted-foreground`. Text: live -> progress, else `▸ <lastToolName>`, else result, else error;
  settled -> error, else result, else progress, else `▸ <lastToolName>`; fallback the status label.
- Row 3 (cols 2-3): metadata `font-mono text-2xs tabular-nums text-muted-foreground/70` truncate,
  joined with ` · `: model label, `<tokens> tok` (or `— tok`), `<toolUses> tools` when known,
  `run <n>` when activated more than once.

Status visuals (`:38-49`):

| Status | Dot | Label |
| --- | --- | --- |
| pending, running, waiting | `bg-info` | `Working` |
| idle | `bg-muted-foreground/50` | `Idle · resumable` (`Idle` for a `subagent_batch`) |
| completed | `bg-success` | `Completed` |
| failed | `bg-destructive` | `Failed` |
| cancelled, interrupted | `bg-muted-foreground/60` | `Stopped` |

### 9.5 Workflows

Open state is local presentation state, initialized to "live" (workflow status not completed/failed/
cancelled/interrupted) and never forced closed later (`:501-522`).

Collapsed (`:458-499`): one button `flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-left
hover:bg-accent/40`: status dot (failed if any member failed), name (`workflowName ?? title`,
`text-sm` truncate), then right-aligned `font-mono text-2xs text-muted-foreground/80 gap-1.5`:
`N failed` (destructive, when > 0), `N agents`, `· <tokens> tok`, `· <elapsed>` (only when both
timestamps exist), `ChevronRight` 12px. Tokens sum the members (or the coordinator's own usage when it
has none).

Expanded (`:378-452`): `section.rounded-lg.border.border-border/50.bg-code.p-1.5`:
- Header row `flex items-center gap-2 px-1.5 pt-0.5 text-3xs font-medium uppercase tracking-wider
  text-muted-foreground`: status dot, name (truncate), optional `{} script` toggle (`rounded-sm border
  border-border/60 px-1 font-mono normal-case`, hover/open `text-foreground`; only when
  `runHandles.scriptPath` exists), right-aligned `<settled>/<members> settled` (`font-mono normal-case
  text-muted-foreground/80`), collapse button (`icon-micro ghost-muted`, `ChevronDown` 12px,
  `aria-label="Collapse workflow"`).
- Phase rail (when phases exist): `flex flex-wrap items-center gap-1 px-1.5 pb-1 pt-1.5`; per phase a
  chip `rounded-sm border px-1.5 py-0.5` with border `info/40` (running), `success/30` (done),
  `border/50` (pending); text `font-mono text-3xs` `text-info-foreground` / `text-success-foreground`
  (prefixed `✓ `) / `text-muted-foreground/70`; then one 6px dot per member (gap 2px) or `–`. Phases
  are separated by `ChevronRight` 12px `text-muted-foreground/40`.
- Script viewer when toggled: `mx-1.5 mb-1 rounded-md border border-border/60 bg-background`; header
  `flex items-center gap-2 border-b border-border/50 px-2 py-1` with `Braces` 12px, file basename
  (`font-mono text-3xs text-muted-foreground` truncate), close (`icon-micro ghost-muted`, `X`,
  `aria-label="Close script"`); body `max-h-72 overflow-auto p-2` (288px): contents in
  `pre.whitespace-pre-wrap.break-words.font-mono.text-2xs.leading-relaxed.text-foreground/90`, with
  `\n… (truncated)` appended when truncated; failure `Could not load the script.`
  (`text-xs text-destructive-foreground`); loading `Loading…` (`text-xs text-muted-foreground`).
- Phase sections (`:313-375`): toggle button `mt-2 flex w-full items-center gap-1.5 rounded-sm px-1.5
  text-left text-3xs font-medium uppercase tracking-wider hover:bg-accent/40`, color by state (done
  `text-success-foreground`, running `text-info-foreground`, pending `text-muted-foreground/70`):
  chevron (down/right 12px), `Check` 12px when done, title, then a `font-normal normal-case
  text-muted-foreground/70` summary: `pending` (pending, no members), `N done` (done), else
  `N active · M done`; when collapsed and it has members, the member dots right-aligned. Opens
  automatically when its state becomes running; initial open = workflow not live or phase running.
  Open sections list `AgentRow`s.
- Then unphased members as rows; a workflow with no phases and no members shows itself as one row.

---

## 10. Preview (browser) surface chrome

The preview needs an embedded Chromium view (Electron `WebContentsView`). It may be built later; the
runtime gate `isPreviewSupportedInRuntime()` decides everything visible here. Full behavior summary in
appendix A.

### 10.1 When the runtime has no preview support

- Launcher row `Browser` and the add-menu item are disabled with the reasons in 6.6 / section 7.
- A persisted preview tab still renders its tab (title `Browser`, `Globe2`), and its content is
  `PreviewPanel`'s fallback (`components/preview/PreviewPanel.tsx:31-41`): centered column
  `flex min-h-0 flex-1 flex-col items-center justify-center gap-3 p-8 text-center`, one line
  `Preview is only available in the T3 Code desktop app.` (`max-w-sm` 384px, `text-sm
  text-muted-foreground`).
- `preview.toggle` and the tab's mute items do nothing.

T3UI should ship this state until a native webview exists (decision for the implementer; see open
questions).

### 10.2 Chrome row (`components/preview/PreviewChromeRow.tsx`)

A `form` using the subheader convention (section 8: inline 28px row with `mb-3`), `px-2`, `gap-1`:

1. Navigation group (`gap-0.5`, `role="group"`, `aria-label="Navigation"`): three
   `Button variant="ghost" size="icon-xs"`: `ArrowLeft` (`Back`, disabled without history),
   `ArrowRight` (`Forward`), refresh (`RefreshIcon`, spins while loading; `aria-label` `Stop` while
   loading else `Refresh`; tooltip `Loading…` / `Refresh`). Tooltips equal the labels.
2. `leadingActions` slot: the tab's browser profile name (appendix A).
3. Address field: `InputGroup variant="ghost"` `h-7 flex-1`, input `size="sm"`, placeholder
   `Search or enter URL`, no spellcheck. Shows the live URL at rest; focus swaps in a draft, selects
   all; Enter submits the trimmed draft (ignored when empty) and blurs; Escape restores and blurs.
   On hover/focus-within an `ExternalLink` button appears at its inline end (`Open in system
   browser`), hidden while editing.
4. `MousePointerClick` annotate toggle (`Annotate preview` / `Cancel annotation`; tooltip
   `Annotate elements, regions, and drawings` / `Cancel annotation (Esc)`, or the disabled reason);
   pressed = `variant="secondary"` and the icon `text-primary`.
5. `Camera` capture (`Capture screenshot`; tooltip `Screenshot · Shift-click to record`; while recording
   `Stop recording`, icon `text-destructive`, a 6px `bg-destructive` dot with `animate-status-pulse`
   at the top right: a stepped 2s pulse, repaints while recording).
6. `PictureInPicture2` float toggle (`Float preview over chat` / `Close floating preview`).
7. `trailingActions`: the `...` more menu (appendix A).

Loading bar: `absolute bottom-0 left-0 h-0.5 w-full origin-left rounded-r-full bg-primary` with glow
`0 0 6px 1px var(--color-ring)`, animated by the `.preview-loading-progress` CSS while `data-loading`
(continuous while loading).

### 10.3 Empty and error states

- No session or `navStatus Idle` (`previewEmptyStateLogic.ts:3-5`): `PreviewEmptyState`
  (`components/preview/PreviewEmptyState.tsx:21-96`):
  - With neither recent URLs nor discovered servers: `Empty` primitive with an icon tile (`Globe`
    18px `text-muted-foreground`), title `No preview yet`, description `Type a URL above, or run a dev
    script. Browser-ready localhost servers will show up here automatically.`
  - Else a scroll column `px-5 py-8`, inner `mx-auto max-w-xl gap-6`: section `Recently used`
    (`History` 16px icon, `text-sm font-medium text-muted-foreground`, up to 8 parseable URLs as
    `DiscoveryList` rows: favicon, title or `host+path+query+hash`, subtitle `<label> · <relative
    time>`, hover-only `X` remove button `Remove <label> from history`), then `Local servers`
    (`RadioTower` icon; rows titled with the process name or `Listening`, description
    `host:port`; footnote `Select a live local server to open it in this browser tab.`, `text-xs`).
- Navigation failure: `PreviewUnreachable` (`components/preview/PreviewUnreachable.tsx`): column
  `max-w-xl px-8 py-16`; 48px outline document-with-slash icon `text-muted-foreground/70`; `This site
  can’t be reached` (`text-2xl font-semibold`); `<host>: <friendly message>.` (host bold); optional
  `Try:` box (`Checking your connection`, `Confirming the dev server is running`, `Checking the proxy
  and the firewall`); the error code (`text-xs uppercase tracking-wide text-muted-foreground/70`,
  e.g. `ERR_NAME_NOT_RESOLVED`); bottom row `Details` / `Hide details` (`outline sm`) and `Reload`
  (`default sm`).

---

## 11. Device surface chrome

Streams an iOS Simulator / Android Emulator from the environment's device host. May be built later;
full summary in appendix B.

- Tab: section 6.3 (picker tab `Device` with `Smartphone`; targeted tabs titled by device name, Apple
  or Android glyph; renamable).
- Not onboarded or host disabled: the add action opens the `DeviceSetup` wizard dialog instead of a tab
  (`ChatView.tsx:4669-4676`); a device tab showing in that state renders the wizard as a dialog and
  dismissing it closes the tab and re-shows the panel (`DevicePanel.tsx:153-166`,
  `ChatView.tsx:9757-9762`).
- Status strips at the top of the surface, each `border-b px-3 py-2 text-xs text-muted-foreground`:
  host status detail (when ready with no device open, `whitespace-pre-line`); host updates
  (`DeviceHostUpdates`); `Starting <names>… This can take a minute.` while devices boot; operation
  errors in `bg-destructive/5 text-destructive` with a dismiss `X` (`Dismiss device error`)
  (`DevicePanel.tsx:168-199`).
- Loading (`DeviceLoadingView`, `:1-50`): centered `max-w-sm gap-3`: 48px tile (`rounded-xl border
  bg-muted/30`, `Smartphone` 24px), name (`text-sm font-medium`; `Devices` while listing), optional
  description `<host> · <version>`, a `Spinner size="xs"` with the message (`Opening device…`,
  `Starting device…`, the install detail or `Installing device support…`, `Finding devices…`), and a
  two-segment 96px step bar (`h-1 rounded-full`, done `bg-foreground/60`, pending `bg-muted`).
- Picker (no device open, `:234-309`): scroll column `px-5 py-8`, `mx-auto max-w-xl gap-6`:
  - Nothing found: centered `Smartphone` 24px at 60% opacity and `No simulators or emulators were found
    on this environment.` (or the host failure detail / `The device hub failed to start.`).
  - Groups `iOS Simulators`, `Android Emulators` (booted first, then by name): `DiscoveryList` rows with
    a 32px bordered `Smartphone` tile, title = device name, description
    `<host label> · <version> · Running|Stopped`, trailing `Open` / `Start` (`text-xs`) or a spinner
    while pending; `aria-label` `Open <name>` / `Start <name>`.
  - `No Android virtual devices found. Create one in Android Studio's Device Manager, then refresh.`
    (`text-xs`) when the host is ready and has no Android devices or unavailability reason.
  - `Refresh devices` button (`size="sm"`; `ghost` + `self-start` with devices, `outline` centered
    without).

---

## 12. Pull-request surfaces chrome

The PR detail (`components/pullRequest/PullRequestDetailPanel.tsx`, 2.9k lines) and the linked list
(`ThreadPullRequestsPanel.tsx`) are a separate area (they share code with the `/pull-requests` page).
This spec covers only how they appear as tabs; see appendix C and the open questions.

- Tabs: section 6.3.
- Capability unknown (server config not loaded yet): `PullRequestDetailGhost` skeleton
  (`ChatView.tsx:9690`, `components/pullRequest/PullRequestGhosts.tsx`).
- Capability `pullRequests` false: `PullRequestsUnavailableState` with title `Pull requests
  unavailable` and description `Update this environment's T3 Code server to browse pull requests.`
  (`ChatView.tsx:9691-9696`): `Empty` with a `GitPullRequestArrow` icon tile, title, description, no
  buttons.
- The detail panel inside a thread has no close button of its own (the tab X closes it), and shows a
  back affordance to the linked list only when the thread has more than one linked PR
  (`ChatView.tsx:9697-9741`).

---

## Appendix A. Preview surface behavior (summary)

- Sessions are server objects: `preview.open` -> `PreviewSessionSnapshot {tabId, navStatus: Idle |
  Loading | Loaded | Failed {url, title, ...}, ...}`; `preview.navigate`, `preview.refresh`,
  `preview.resize`, `preview.close`, `preview.list`, `preview.reportStatus`, stream
  `subscribePreviewEvents` (upstream `contracts:rpc.ts:360-366,456,1208-1285`, `preview.ts`).
- The desktop shell owns the real browser views (`window.desktopBridge.preview`), keyed by a runtime
  tab id = server epoch + session id (`ChatView.tsx:2040-2043`); it reports favicon, audio, overlay
  state per tab (`desktopByTabId`).
- Browser profiles (`browser/browserDefaults`): the add menu picks one at tab creation; a tab's profile
  is fixed.
- `...` menu (`PreviewMoreMenu.tsx`): hard reload, devtools, zoom, clear data; zoom indicator
  (`ZoomIndicator.tsx`); keyboard `⌘R`, `⌘L`, `⌘=`/`⌘+`, `⌘-`, `⌘0` while the preview has focus.
- Annotation mode picks elements/regions/drawings and sends them to the composer as an annotation +
  image (`onSendAnnotation`, `ChatView.tsx:9651-9655`).
- Floating mini player (`ThreadPreviewMiniPlayer.tsx`, `previewMiniPlayerStore.ts`): closing the panel
  on a live tab floats it over the chat; `PictureInPicture2` toggles it.
- Agent automation (`PreviewAutomationHosts.tsx`, `previewAutomation*`): the agent can drive tabs; an
  agent cursor overlay is drawn; closing an agent-controlled tab asks for confirmation.
- Terminal links can open in the preview (`openTerminalLinkInPreview.ts`).

## Appendix B. Device surface behavior (summary)

- RPCs (upstream `contracts:rpc.ts:372-379,458`, `contracts:device.ts`): `device.configure`,
  `device.list`, `device.testHost`, `device.open {threadId, hostId, deviceId, platform}` ->
  `{hostId, deviceId, ...}`, `device.close {threadId, hostId, deviceId, shutdown}`,
  `device.shutdown`, `device.detail`, `device.action`, stream `subscribeDeviceState` ->
  `DeviceServiceState {onboardingCompleted, hostStatus, hostStatuses, hostStatusDetail, hosts[],
  devices[], sessions[], bootingDevices[]}`.
- `DeviceWorkspace` streams video (`DeviceStreamView.tsx`) inside a phone/fold/duo frame
  (`DevicePhoneViewport`, `DeviceDuoViewport`, `deviceFrameLayout.ts`), with a controls rail (home,
  rotate, screenshot, float, close, power off), a trackpad mode, and a tools panel (versions, host).
- Closing a device tab only hides it; "power off" calls `device.close {shutdown:true}`.
- Floating: the device can float in the same mini player as the browser.

## Appendix C. Pull-request surfaces (summary)

- `pull-request`: `PullRequestDetailPanel` with Summary / Timeline / Code tabs, review form, checks
  popover, stack layers, reactions, merge controls. Data from `pullRequest.*` RPCs
  (upstream `contracts:pullRequest.ts`, `rpc.ts`), gated on capability `pullRequests` (and
  `threadPullRequests`, `pullRequestStackActions`).
- `pull-requests`: `ThreadPullRequestsPanel`, the thread's linked PRs as rows; selecting one opens its
  `pull-request` tab.
- Shortcut `pullRequest.copyNumber` (`⇧⌘K`, `!terminalFocus`).

---

## Reuse map

| T3UI module | Verdict |
| --- | --- |
| `crates/t3-app/src/panels/model.rs` (branch `diff`, efaa305) | Logic fits: all fe7d3092c kinds, fork-exact ids, the action set. Needs: `userActionRevision` + `openProactive`, `dismissedDeviceSurfaceIds`, `openDevice` (picker replacement) / `renameDevice`, `openAttachment`, `activateTerminal`/`splitTerminal`/`closeTerminal`, the two reconcile functions, persistence with the sanitizing rules in 1.4. |
| `panels/store.rs` (branch `diff`) | Mechanics fit (global entity, `controls_in_panel`, maximized thread). Change: width is per thread (`t3code:preview-panel-width:<threadKey>`), default 540, min 360, max `max(360, min(floor(0.7*vw), floor(row) - 360))`; drop the old 1400px cap. Maximize is per-route-thread and not persisted. |
| `panels/context.rs`, `panels/thread_detail.rs` (branch `diff`) | Fit as-is (request helper, `subscribeThread` follower). |
| `panels/view.rs` (branch `diff`) | July-era visuals: replace. Keep the inline/sheet split and presence plumbing as a starting point. |
| `panels/plan/*` (branch `diff`) | Not a panel any more (no plan surface). Move whatever is reusable to the transcript plan card (chat). |
| `t3-app` sidebar `NativeMenu` usage (`sidebar/menus.rs`) | Reuse for the tab context menu (native). |
| `t3-ui` Toggle/Button/Menu/Tooltip/Kbd | Reuse; check sizes against the primitives table. The tools handoff notes `TooltipExt` only places tooltips on top: the layout toggles need side bottom (`base::Positioner`). |
| Agents panel | Nothing exists. Port `crt:state/subagentRuntime.ts` fold + derive as pure Rust in `t3-logic` (GPUI-free, testable on recorded activities), then the view. |
| Icons | `PanelBottom`, `PanelBottomClose`, `PanelRight`, `PanelRightClose`, `Maximize2`, `Minimize2`, `FileDiff`, `Files`, `Globe2`, `TerminalSquare`, `Bot`, `Smartphone`, `Plus`, `ChevronLeft/Right/Down`, `Volume2`, `VolumeOff`, `X`, `Braces`, `Check`, `GitPullRequestArrow`, `GitPullRequestDraft`, `GitPullRequestClosed`, `GitMerge`, `Link2` (lucide) plus Apple/Android logos: check `t3-ui` assets; the panels handoff lists `PanelBottom`, `PanelRight` as missing. |

## Reference screenshots needed

All at 1440x900 @2x unless noted, dark and light. Seed: `e2e/run-local.sh up --server fork --capture`
(or nightly) with the default seed; thread `thread-aurora-tour` (project `aurora-web`, completed turn
with a diff, dirty `README.md`).

1. `panel-closed-header`: `thread-aurora-tour`, panel closed. Titlebar toggles at the top right
   (terminal + right panel), header actions padded.
2. `panel-empty-launcher`: same thread, press `⌥⌘B` on a thread with no tabs. "Open a surface" list with
   Browser available (Electron) and Pull request / Linked pull requests disabled.
3. `panel-empty-launcher-hover`: as 2 with the pointer on `Terminal` (row highlight) and a disabled-row
   tooltip open on `Pull request`.
4. `panel-tabs-mixed`: open Diff, Files, Terminal, Agents from the "+" menu; active Agents. Shows
   active vs inactive tabs, the "+" button, the titlebar maximize + toggles group.
5. `panel-tab-hover-close`: as 4, pointer over an inactive tab (fill + fading close button).
6. `panel-add-menu`: as 4 with the "+" menu open (letters, disabled items).
7. `panel-tab-overflow`: open about 10 file tabs (open files from the Files tree) at the default width
   so the strip overflows; arrows visible, edge fade.
8. `panel-maximized`: as 4 after "Maximize panel" (sidebar expanded), and a second capture with the app
   sidebar collapsed (tab bar left inset).
9. `panel-resize-hover`: pointer on the panel's left edge (1px border highlight).
10. `panel-sheet`: window resized to 960x900, panel open (sheet over the chat with backdrop, toggles in
    the tab bar).
11. `panel-agents-empty`: Agents tab on `thread-aurora-tour` (`No agents yet`).
12. `panel-agents-live`: needs a seeded thread with subagent activities (none today: a fake-Codex
    scenario emitting `task.started`/`task.progress`/`task.completed` with `agentKind: "agent"`, one
    workflow with two phases, plus one direct spawn). Capture with the workflow expanded, the script
    viewer open, and the right-panel toggle badge visible with the panel showing another surface.
13. `panel-device-tab-renaming`: needs a device host; optional (skip if unavailable).
14. `panel-browser-unavailable`: only reproducible in the web build (no desktop bridge): add-menu with
    Browser disabled and its tooltip. Use the fork web UI from `--server fork`.
15. `panel-pr-unavailable`: optional, needs an environment without the `pullRequests` capability.

## Missing client APIs

- `orchestration.getWorkflowScript` `{threadId, scriptPath}` -> `{scriptPath, contents, truncated}`
  (Agents script viewer; upstream `contracts:orchestration.ts:2324-2337`).
- `preview.open` / `navigate` / `refresh` / `resize` / `close` / `list` / `reportStatus` and
  `subscribePreviewEvents` (preview surface; only needed when a native webview exists; until then the
  surface reconciler needs at least `subscribePreviewEvents` or `preview.list` to drop dead tabs).
- `device.*` and `subscribeDeviceState` (device surface, appendix B).
- `pullRequest.*` detail/summary RPCs used by the PR tab icon (`pullRequestEnvironment.detail`) and the
  PR surfaces (owned by whichever spec covers pull requests).
- Client settings fields `panelAnimationDurationMs`, `proactivePanelsEnabled`,
  `autoShowFloatingPreview` in T3UI's `ClientSettings` (the shell handoff says `ClientSettings` lacks
  fe7d3092c fields).

## Open questions / risks

- Browser preview in a native app: GPUI has no embedded Chromium. Options: ship the "desktop app only"
  fallback (Browser disabled everywhere), or host a `WKWebView` child view on macOS. Visual parity with
  Electron's tab favicons/audio would need the latter. Decide before building appendix A.
- The PR detail and linked-list surfaces (about 22k lines incl. logic) have no spec owner in this
  round. They need their own spec; this file covers only their tabs and unavailable states.
- `openProactive` and proactive panels are off by default; low priority, but the revision counter must
  exist from day one because `openDevice(automatic)` and reconcilers depend on the user/automatic split.
- Sheet mode is reachable on the desktop (840-980px). The backdrop blur cannot be reproduced in GPUI;
  the 60% background tint alone will look lighter.
- The tab close button's gradient fade relies on the tab's background var; GPUI needs a horizontal
  linear gradient quad (supported) with the same three stops.
- `liveAgentCount` depends on the subagent fold; until it is ported the badge and the Agents empty
  state are the only visible pieces.
- Electron titlebar specifics (`drag-region`, the 112px no-drag rectangle, `.wco` variables) have no
  GPUI equivalent beyond "this area does not drag the window"; verify window dragging over empty tab
  bar space still works.
