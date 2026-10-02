# Chat view build spec (shell around the timeline)

> Refreshed against fe7d3092c (2026-10-02). Replaces the July-era chat spec in full.
> Companion files: [`chat-timeline.md`](chat-timeline.md) (rows, row logic, list mechanics),
> [`markdown.md`](markdown.md) (ChatMarkdown, code blocks, Shiki). The composer, its banner stack,
> pickers, pending approval / user-input panels and the BranchToolbar are in
> [`composer.md`](composer.md); this file only specifies the slot they sit in. Right panel, terminal
> drawer and header tool controls (scripts, git) are in [`panels.md`](panels.md).

## 0. Conventions

- **Paths.** Relative to `~/L-Projects/t3UI-refs/t3code-fork/apps/web/src/` unless prefixed:
  `contracts:` = `packages/contracts/src/`, `crt:` = `packages/client-runtime/src/`,
  `shared:` = `packages/shared/src/`, `desktop:` = `apps/desktop/src/`.
- **Units.** 1 Tailwind unit = 4px. Root font size is 16px (`appearanceFonts.ts:112`, setting
  `fontSizeInterface` default 16, `contracts:settings.ts:131`), so 1rem = 16px. `text-xs` 12/16,
  `text-sm` 14/20, `text-base` 16/24, `text-2xl` 24/32, `text-3xl` 30/36, `text-2xs` 11/16,
  `text-3xs` 10/14 (`index.css:168-171`).
- **Desktop.** Values assume a macOS Electron window >= 1100px, so `sm:` (640), `md:` (768) and
  `lg:` (1024) variants apply. The right panel switches inline to sheet at `max-width: 980px`
  (`rightPanelLayout.ts:1`), which a desktop window can reach; noted where it matters.
- **Fonts.** Sans = system stack `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui,
  sans-serif` (SF Pro on macOS); mono = `ui-monospace, "SF Mono", "SFMono-Regular", Menlo, Consolas,
  "Liberation Mono", monospace` (`index.css:156-160`), overridable in Settings → Appearance.
  Grayscale antialiasing on macOS by default (`fontSmoothing`, `appearanceFonts.ts:119-125`).
- **Radius.** `--radius` 10px: `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14,
  `rounded-2xl` 18, `rounded-3xl` 22, `rounded` 4. Buttons use `--control-radius` 8px
  (`index.css:92`, `ui/button.tsx:11`).
- **Colors.** Token names (`foreground`, `muted-foreground`, `border`, `accent`, `info`,
  `error`, `error-surface`, `warning-surface` ...); `X/NN` = token X at NN% alpha. Utility-backed
  roles: `text-placeholder` = `--contrast-placeholder`, `text-secondary-label` =
  `--contrast-secondary-label`, `bg-message` = `--message-surface`, `text-message-foreground` =
  `--contrast-message-foreground`, `text-icon-muted` = `--contrast-icon-muted`. In the default
  theme these resolve to `muted-foreground` / `foreground` (compiled CSS). Values live in
  `docs/spec/tokens.json` (design agent).
- **Default theme.** Theme preference defaults to `"system"` with no palette, so
  `html[data-theme-id]` is absent (`hooks/useTheme.ts:41`, `themePalette.ts:1516-1536`). Every
  `html[data-theme-id] ...` override in `index.css` (toolbar colors in the header, code-block
  tokens) is for named palettes only. Implement the default first.
- **Glass.** `surface-glass`, `alert-glass`, `dropdown-glass`, `dialog-glass` blur the backdrop
  (`index.css:322-406`). GPUI has no backdrop blur: use each utility's documented fallback
  (`@supports not (backdrop-filter)` branch): solid `background` (or `popover`), plus the 4% tint
  for `alert-glass`.
- **Tooltips.** Base UI `Tooltip`, open delay 600ms, close delay 0 (Base UI
  `tooltip/utils/constants.js` `OPEN_DELAY = 600`), side `top` unless stated, side offset 4px.
  Popup: `rounded-md` 8, 1px `border`, `bg-popover`, `text-xs`, `leading-snug`, max-w 320px,
  px 8 py 4, `shadow-md/5`; enter/exit fade + scale 0.98 (`ui/tooltip.tsx:12-62`).
- **Animations.** Setting `panelAnimationDurationMs` defaults to **0** (`contracts:settings.ts:119`),
  so every "panel animation" below is off by default (`panelAnimations.ts:71-79`: active only when
  duration > 0 and not reduced motion). Implement the 0ms behavior first.

---

## 1. Routes and render states

### 1.1 `_chat` layout

`routes/_chat.tsx:209-234`.

- `beforeLoad` redirects to `/pair` (replace) unless the auth gate is `authenticated` or
  `hosted-static` (`:225-232`).
- Renders `ChatRouteGlobalShortcuts` (1.4) plus either `ThreadRouteView` (when the URL has
  `environmentId`+`threadId`, or `draftId`) or the `<Outlet/>` (index and other children). Both
  thread routes render through this one parent so a draft's promotion to a server thread keeps the
  same `ChatView` mounted (`:209-221`, leaf routes render `null`:
  `routes/_chat.$environmentId.$threadId.tsx:5-7`, `routes/_chat.draft.$draftId.tsx:5-7`).

### 1.2 Index route `/`

`routes/_chat.index.tsx:25-111`.

- Hosted static build with zero environments: `HostedStaticOnboardingState` (browser only; never
  in the desktop app).
- Otherwise `IndexDraftLanding`: renders **nothing** until every environment shell is bootstrapped
  (`useAllEnvironmentShellsBootstrapped`). Then it immediately opens a new draft for the most
  recently updated project (`sortScopedProjectsForSidebar(projects, threads, "updated_at")[0]`) with
  `replace: true` (`:50-69`), so the desktop app never shows an empty index once a project exists.
- No project: `NoProjectsHero` (onboarding spec).
- Draft creation failed: `DraftStartError` (`:91-111`): `SidebarInset` + `WorkspacePageHeader`
  (Electron, empty) + `Empty` centered: title "Couldn’t start a new thread" (note the curly
  apostrophe in source), description "The project is still available. Try opening the draft
  again.", `mt-5` (20px) row with `Button size="sm"` `RefreshIcon` + "Try again" (retries).
  `EmptyHeader` max-w-md (448px). Title `font-semibold text-xl` (20/28); description
  `text-muted-foreground text-sm`, `mt-1` after the title (`ui/empty.tsx:92-110`).

### 1.3 Thread routes (`ThreadRouteView`)

`components/ThreadRouteView.tsx:51-214`, `threadRoutes.ts:21-40`, `threadSync.ts:1-27`.

Render state (`resolveThreadRouteRenderState`):

| Input | State |
|---|---|
| env shell snapshot not yet received | `loading` |
| thread detail exists, or a local draft exists for the ref | `ready` |
| thread detail marked deleted | `missing` |
| shell row exists, detail not yet loaded | `loading` |
| otherwise | `missing` |

- Server route renders `ChatView routeKind="server"` when `ready`, or `loading` **with** a shell
  row (the shell row is enough to paint header + held timeline). Otherwise renders an empty
  `SidebarInset` (`:196-214`).
- `missing` after bootstrap: drops pending sidebar file drops for that thread; if the environment
  has any threads (server or draft) navigates to `/` (replace). With zero threads it stays blank
  (`:162-177`).
- `threadSyncPhase` (`threadSync.ts:5-23`): shell row absent → none; status `empty|cached|
  synchronizing` → `"loading"` without detail, `"syncing"` with detail; `live|deleted` → none.
  Labels "Loading messages..." / "Syncing messages..." are shown by the composer
  (`chat/ComposerActivityStatus.tsx:18`), not the timeline.
- Draft route renders `ChatView routeKind="draft" forceExpandedMobileComposer` keyed by the
  draft id while the draft session exists; with no draft and no canonical thread it navigates to
  `/` (replace) (`:155-160`).
- Draft promotion: the draft's server ref is `draftSession.promotedTo` or a server thread whose
  ref equals the draft's reserved `(environmentId, threadId)`. It navigates to
  `/$environmentId/$threadId` (replace) once `resolveDraftPromotionNavigationTarget` returns a ref
  (`ChatView.logic.ts:414-433`: not while a background submission is pending; only when the server
  turn has `startedAt`, or session status is `error|stopped|interrupted`, or a user message is
  persisted). It first awaits the draft hero transition animation (`draftHeroTransition.ts`
  `waitForDraftHeroTransition`). The server route keeps the draft id as the `ChatView` key, so the
  view is not remounted (`:96-110`).
- Wrapper: `SidebarInset` `h-svh min-h-0 overflow-hidden overscroll-y-none md:h-dvh`
  (`:211`). `SidebarInset` = `<main>` `relative flex min-w-0 w-full flex-1 flex-col bg-background
  surface-grain` (`ui/sidebar.tsx:545-557`). `surface-grain` is a 256px fractal-noise tile at 3.5%
  opacity (`index.css:1013-1017,1673-1676`); design spec decides whether GPUI paints it.

### 1.4 Global chat shortcuts (`ChatRouteGlobalShortcuts`)

`routes/_chat.tsx:31-207`, window `keydown` (bubble). Skipped when the event is already
`defaultPrevented` or the command palette is open. Commands (defaults from
`shared:keybindings.ts:24-70`):

| Command | Default | Behavior |
|---|---|---|
| `thread.undo` | mod+Z when `!terminalFocus && !editableFocus` | Undo latest thread action (archive etc.) unless model picker open or repeat |
| Escape | | Clears multi-selection of sidebar threads when any are selected |
| `chat.newLocal` | mod+shift+N | `startNewThreadFromContext` |
| `chat.newWithoutProject` | mod+alt+N | Scratch ("no project") thread in the active/primary environment |
| `chat.new` | mod+N, mod+shift+O | Opens the command palette "new-thread-in" when the default sidebar is on and there are >1 project groups; otherwise like `chat.newLocal` |
| `preview.toggle` | mod+shift+J | Toggles the browser preview panel; outside desktop shows toast "Preview is desktop-only" / "Open T3 Code in the desktop app to use the in-app preview." |
| `preview.refresh/focusUrl/zoomIn/zoomOut/resetZoom` | (when `previewFocus`) | Preview panel actions |

### 1.5 No active thread

`ChatView.tsx:9577-9579` returns `NoActiveThreadState` (`components/NoActiveThreadState.tsx:6-35`)
when the view has no thread (no server thread and no draft).

```
SidebarInset h-dvh min-h-0 overflow-hidden overscroll-y-none
└─ div flex min-h-0 min-w-0 flex-1 flex-col overflow-x-hidden bg-background
   ├─ WorkspacePageHeader (section 3.1) + border-b border-border
   │    Electron: span text-xs text-muted-foreground/50 "No active thread"
   └─ Empty flex-1 (centered column, gap 24, p 48 at md)
        └─ div w-full max-w-lg (512px) px-8 py-12 (32/48)
             title  "Pick a thread to continue"       font-semibold text-xl (20/28)
             desc   "Select an existing thread or create a new one to get started."
                    text-sm text-muted-foreground, mt-4px
```

---

## 2. ChatView box tree

`ChatView.tsx:9818-10541` (render), props `ChatViewProps` (`:748-769`).

```
div.relative.flex.min-h-0.min-w-0.flex-1.overflow-hidden.bg-background          root (row)
├─ Dialog (device setup wizard; right-panel device surface, out of scope)
├─ [panelLayoutControls]  when right panel present AND inline (window > 980px)    3.4
├─ div.flex.min-h-0.min-w-0.flex-col.overflow-x-hidden                           chat column
│    flex-1; when right panel maximized: w-0 flex-none (data-chat-column-maximized-away)
│  ├─ WorkspacePageHeader[data-chat-header] relative bg-background               3.1
│  │    ├─ [placeholder span] Electron + controls at root: fixed 112x52 no-drag
│  │    ├─ [panelLayoutControls] when right panel not present (closed)           3.4
│  │    └─ ChatHeader                                                            3.2
│  ├─ div.flex.min-h-0.min-w-0.flex-1                                            main row
│  │  └─ div.relative.flex.min-h-0.min-w-0.flex-1.flex-col  (workspace drop target)
│  │     ├─ [drop overlay] while a file drag is over the column                  8
│  │     ├─ div.pointer-events-none.absolute.inset-x-0.top-0.z-20.flex.flex-col  banners, 4
│  │     │    ProviderStatusBanner, ThreadErrorBanner
│  │     ├─ div.relative.flex.min-h-0.flex-1.flex-col.bg-background              messages wrapper
│  │     │  ├─ MessagesTimeline (fills the wrapper; chat-timeline.md)
│  │     │  └─ [scroll-to-end pill] absolute, centered                            6.4
│  │     ├─ div[data-chat-composer-overlay]  (inert while reverting)              6
│  │     ├─ [ThreadPreviewMiniPlayer] floating browser preview (preview spec)
│  │     ├─ AlertDialog "Switch to <branch>?"                                     9.2
│  │     └─ [PullRequestThreadDialog]                                             9.4
│  └─ PersistentThreadTerminalDrawer × mounted threads ([`panels-terminal.md`](panels-terminal.md))
├─ [RightPanelTabs mode="inline"]   right panel present, window > 980px ([`panels.md`](panels.md))
├─ [RightPanelSheet > RightPanelTabs mode="sheet"]  right panel present, window <= 980px
├─ AlertDialog "Edit from here?" (revert)                                         9.1
├─ LinkPullRequestDialogHost (pull request spec)
└─ [ExpandedImageDialog]                                                          9.3
```

Notes:

- The timeline fills the whole messages wrapper. The composer overlay floats over its bottom and
  the banners float over its top. Neither changes the timeline's height; the timeline reserves
  space with bottom padding instead (section 6.3).
- There is **no right-side plan panel** in this version. Proposed plans render inline in the
  timeline (`chat-timeline.md`). The right panel hosts preview / terminal / diff / files / pull
  requests / agents / device surfaces (`ChatView.tsx:9648-9810`, [`panels.md`](panels.md)).
- There is **no border under the chat header**. The top edge of the timeline fades under the
  header with a mask instead (`topbar-scroll-fade`, `chat-timeline.md`). `NoActiveThreadState` is
  the only chat surface with `border-b`.

---

## 3. Header

### 3.1 Header box (`WorkspacePageHeader`)

`components/WorkspacePageHeader.tsx:7-28`, `index.css:111-151`, `workspaceTitlebar.ts:1-2`,
`components/ui/sidebar.tsx:157-172`, `components/AppSidebarLayout.tsx:58,249-256`.

| Property | Value |
|---|---|
| Height | `--workspace-topbar-height` 52px (WCO only on Windows/Linux) |
| Layout | `flex shrink-0 items-center gap-3` (12px) |
| Padding left | `--workspace-gutter-start` = 20px (12px below 640) |
| Padding left, sidebar collapsed | `--workspace-titlebar-content-left` = controls-left + 28 + 12. macOS window not fullscreen: controls-left = 82px (`--desktop-window-controls-inset`, traffic lights) → **122px**. macOS fullscreen / other OS: controls-left 12px → **52px** |
| Padding right | `--workspace-gutter-end` = 20px |
| Padding bottom | `--workspace-titlebar-items-lift-padding` 0 |
| Background | `bg-background` (ChatView passes `relative bg-background`, `ChatView.tsx:9856`) |
| Window drag | Electron: `drag-region`; buttons/inputs/anchors are `no-drag` (`index.css:1717-1737`) |
| Transition | `padding-left, padding-right` over `--panel-animation-duration`, ease-out, only when `data-panel-animations=true` (off by default) |

macOS traffic lights sit at x 16px, vertically centered in the 52px bar (`desktop:window/DesktopWindow.ts:258-265`).
They live over the sidebar; with the sidebar collapsed the header content starts at 122px so the
title clears them and the sidebar toggle.

### 3.2 `ChatHeader` (breadcrumb + actions)

`components/chat/ChatHeader.tsx:128-535`, `components/WorkspaceBreadcrumb.tsx:1-77`.

```
div @container/header-actions  flex min-w-0 flex-1 items-center gap-3 (8 below 640)
├─ nav[aria-label="Thread breadcrumb"] flex-1 overflow-clip (clip margin 2px)
│  └─ ol flex min-w-0 items-center gap-3 (8 below 640) text-sm (14/20)
│     ├─ [project item]  li flex items-center font-medium text-muted-foreground shrink
│     │    button inline-flex gap-1.5 rounded-sm text-muted-foreground hover:text-foreground
│     │      ProjectFavicon 14x14 + text max-w-40 (160px) truncate
│     ├─ [separator]  li text-icon-muted  "/"
│     └─ title item  li flex min-w-10 (40px) flex-1 items-center font-medium text-foreground
│          server thread: button.group/thread-title inline-flex gap-1 rounded-sm
│              h2 > text (truncate)  +  ChevronDownIcon 14px text-muted-foreground
│          draft:  h2 > text (truncate)
│          renaming: input (see below)
└─ div[data-chat-header-actions] flex shrink-0 items-center justify-end gap-2 (12 at container >= 768)
     padding-right: 0 when the right panel is open, else 57px (73px below 640)
     ├─ MenuTrigger (collapsed mode only)  ghost icon-sm 28x28  EllipsisIcon 16
     └─ actions host (ProjectScriptsControl, GitActionsControl)
```

- **Breadcrumb text** (`WorkspaceBreadcrumbText`): `block min-w-0 truncate`, CSS
  `text-box: trim-both cap alphabetic` with 6px vertical padding where supported, which centers
  capitals on adjacent icons (`WorkspaceBreadcrumb.tsx:29-44`). GPUI: center the cap height of the
  14px text on the 14px favicon/chevron.
- **Project crumb** only when the thread has a project. Click = new thread in that project
  (`onNewThreadInProject`). `aria-label` and tooltip (top): `New thread in {projectName}`. Focus
  ring 2px `ring`.
- **Title crumb, server thread**: tooltip (top) = full title. `aria-label` `Thread actions for
  {title}`, `aria-haspopup="menu"`. Chevron opacity 0 → 1 on hover/focus-visible of the button
  (`transition-opacity`, default 150ms `cubic-bezier(0.4,0,0.2,1)`).
  - Click opens the thread action menu (native context menu through the desktop bridge,
    `hooks/useThreadActionMenu`, same menu as the sidebar row; shell spec) at
    `(button.left, button.bottom + 4)`. Mouse clicks on the text wait **500ms**
    (`TITLE_MENU_OPEN_DELAY_MS`, `:103`) so a double-click can cancel it; keyboard activation
    (`detail === 0`), clicks on the chevron, and the browser build open immediately (`:284-305`).
  - Double-click on the text (no modifiers) starts inline rename (`:306-316`).
- **Title crumb, draft**: plain `h2` with tooltip; no menu, no rename.
- **Inline rename** (`:443-455`): `input` autofocused, all text selected on focus,
  `aria-label="Thread title"`, `min-w-0 flex-1 rounded-sm bg-transparent text-sm font-medium
  text-foreground`, ring 1px `ring/50` (focused `ring`). Enter commits, Escape cancels, blur
  commits. Commit trims; empty → warning toast "Thread title cannot be empty"; unchanged → no-op;
  else `thread.meta.update` `{threadId, title}`; failure → error toast "Failed to rename thread",
  description = error message or "An error occurred." (`:233-257`). Switching threads drops an
  in-progress rename.
- **Context menu on the header** (not on the actions area, not while renaming): server thread →
  thread action menu at the cursor; draft with a logical project → native menu with one item
  `{id:"project-settings", label:"Project settings", icon:"settings"}` (`:317-342`).
- **Header actions** (`:356-402`): in order `ProjectScriptsControl` (when the project's scripts
  are loaded), `OpenInPicker` (**hidden**: `SHOW_OPEN_IN_PICKER = false`, `:107`),
  `GitActionsControl` (when the thread has a project and a git cwd). Their visuals belong to the
  scripts / git spec; this view only hosts them.
- **Collapsed actions** (`:165-198,506-531`): when the window is mobile-sized or the header
  container is narrower than 512px (side panels can cause this on desktop), the actions move into
  a menu: trigger `Button size="icon-sm" variant="ghost"` (28x28 at sm), `EllipsisIcon` 16px,
  `aria-label="More header actions"`; popup `aria-label="Header actions"`, `align="end"`. Each
  control renders its `presentation="menu"` variant, separated by `MenuSeparator`. The trigger is
  hidden when there is nothing to show.
- **Breakpoint fade** (`:153-164`): crossing the 48rem (768px) container width fades the actions
  (`observeResponsiveBreakpointFade`), only when panel animations are on.
- Themed palettes recolor the header and its controls (`index.css:1365-1433`); default theme does
  not.

### 3.3 Header data

| Element | Source | t3UI today |
|---|---|---|
| Title | `thread.title` (server) or draft title (`activeThread.title`) | `ShellState` thread summary / `ThreadState.thread.title` |
| Project name, favicon | `EnvironmentProject.title`, `ProjectFavicon` | shell projects; favicon spec in shell-sidebar-settings |
| Rename | `thread.meta.update` (`threadEnvironment.updateMetadata`) | `t3_client::commands::rename_thread` |
| Thread menu actions | `useThreadActionMenu` | shell spec |

### 3.4 Panel layout controls

`ChatView.tsx:9581-9647`, `components/chat/PanelLayoutControls.tsx:29-146`,
`components/ui/toggle.tsx:8-47`.

Placement: always `position: fixed` at the window's top-right, so it never moves with the header.

```
div[data-workspace-titlebar-controls]  fixed top 0, right 12px (+1px mr), z-50, h 52,
    flex items-center gap-1 (4px), no-drag, pointer-events-none (children re-enable)
    + when the inline right panel is present: ::before solid bg-background from the group's
      left edge to the window's right edge, behind the controls
├─ [maximize span]  only when the right panel is open, not animating, and inline (not sheet)
│    Toggle variant=ghost size=sm (28x28 at sm), Maximize2Icon / Minimize2Icon 16px
│    aria + tooltip (bottom): "Maximize panel" / "Restore panel size"; pressed = maximized
└─ div.pointer-events-auto.flex.h-full.items-center  → PanelLayoutControls
     div[data-panel-layout-controls] flex h-full items-center gap-1 (4px) no-drag
     ├─ terminal toggle   Toggle variant=panel size=panel
     └─ right panel toggle Toggle variant=panel size=panel (+ live agent badge)
```

Toggle `panel` geometry: 28x28 (`--workspace-titlebar-control-size` 1.75rem), radius 10
(`rounded-lg`), transparent border, icon 16px at 80% opacity, `text-foreground`. Hover
`bg-accent`. **Pressed (panel open) stays unfilled**: `data-pressed:bg-transparent`, hover while
pressed `bg-accent`. Disabled: `text-muted-foreground`, icon opacity 100%, no pointer events.
Focus-visible ring 2px `ring` offset 1px.

| Control | Icon (closed / open) | aria-label | Tooltip (side bottom) | Disabled |
|---|---|---|---|---|
| Terminal | `PanelBottomIcon` / `PanelBottomCloseIcon` | "Toggle terminal drawer" | `Toggle terminal drawer ({shortcut})`, or "Terminal drawer is unavailable" | thread has no project |
| Right panel | `PanelRightIcon` / `PanelRightCloseIcon` | "Toggle right panel", or `Toggle right panel, {n} agent(s) working` | `Toggle right panel ({shortcut})` + ` · {n} agent(s) working` when n > 0; or "Right panel is unavailable" | thread has no project |

- Shortcut labels come from `shortcutLabelForCommand(keybindings, "terminal.toggle" |
  "rightPanel.toggle")`; defaults mod+J and mod+alt+B (`shared:keybindings.ts:25-26`), rendered
  per platform (macOS glyphs).
- Live agent badge (`PanelLayoutControls.tsx:91-98`): `absolute -top-1 -right-1` (−4px), height
  14px, min-width 14px, `rounded-full bg-info px-1` (4px), `text-3xs` (10/14) `font-semibold
  tabular-nums text-white`. Count = running + waiting subagents (`agentPanelModel.liveCount`);
  forced to 0 while the right panel is open on the Agents surface (`ChatView.tsx:9591-9593`).
- Where it renders: inside the header when the right panel is closed; at the ChatView root when
  the right panel is present inline; inside the sheet's tab bar when the right panel is a sheet
  and open (`ChatView.tsx:9843,9864,10442-10446`). Because it is `fixed`, all three paint at the
  same screen position. The header actions reserve its width with `padding-right` (3.2).

---

## 4. Banners over the timeline

`ChatView.tsx:9920-9936`. Container: `pointer-events-none absolute inset-x-0 top-0 z-20 flex
flex-col` inside the chat column (below the header). Banners float over the timeline and do not
change its height. While any banner shows, the timeline's top fade is off
(`hasTimelineTopBanner`, `ChatView.tsx:3799`, timeline `topFadeEnabled={!hasTimelineTopBanner}`).

### 4.1 Alert geometry used by both banners

`components/ui/alert.tsx:7-150`, `surface="glass"`, `controlAlignment="first-line"`.

```
div[data-slot=alert] rounded-xl (14) border px-3.5 py-3 (14/12) text-sm text-card-foreground
   alert-glass: bg = background @ --glass-opacity 80% + 4% tint of the variant color
                (fallback without blur: solid background)
└─ div flex gap-2 (8) items-start; with an action: min-h-6 (24) pt-0.5 (2)
   ├─ icon box  h = line-height (20) w-4; svg 16x16
   ├─ content   flex min-w-0 flex-1 flex-col gap-0.5 (2)
   │    AlertTitle font-medium
   │    AlertDescription flex flex-col gap-2.5 (10) text-muted-foreground
   └─ action    flex gap-1 (4) shrink-0 items-center, h = line-height, self-start
```

Variants: `error` border `error/32`, bg `error-surface`, text `error-foreground`, description
`error-foreground/80`, icon `error`. `warning` border `warning/32`, bg `warning-surface`, text
`warning-foreground`, description `warning-foreground/80`, icon `warning`. With `alert-glass`
the bg is replaced by the glass mix (`index.css:332-362`, `!important`).

### 4.2 `ProviderStatusBanner`

`components/chat/ProviderStatusBanner.tsx:91-152`. Wrapper `pointer-events-auto mx-auto w-fit
max-w-[calc(100%-2rem)] pt-3` (12px top, width fits content, 16px side margin minimum).

- Shown when `getProviderStatusBannerKey(status)` is non-null and differs from the dismissed key
  (`:19-52`). Null for: no status, `disabled`, `ready` with no compatibility issue, and Antigravity
  installed + `warning` + auth `unknown`. Incompatible versions (`compatibilityAdvisory.status`
  `broken`, or `unsupported` while `ready`) show even when ready.
- Status source: the active provider instance's `ServerProvider` from server config
  (`contracts:server.ts`, `ServerConfig.providers[]`). t3UI: `ServerConfig` in
  `t3_client::Environment::config()`.
- Variant: `warning` when the provider status is `warning` or the version is merely unsupported;
  `error` otherwise (`:113-114`). `role="alert"` except unsupported-version (`role="status"`).
- Icon `InfoIcon`. Title (`:107-111`):
  - unauthenticated error: `{providerName} is unauthenticated`
  - incompatible: `{providerName} {version} is known to be broken` / `... is unsupported`
  - else `{providerName} provider status`
  - `providerName` = `status.displayName` trimmed, else the driver label
    (`formatProviderDriverKindLabel`).
- Message (`getProviderStatusMessage`, `:63-89`), clamped to 3 lines with a tooltip (top,
  `whitespace-pre-wrap`) carrying the full text:
  broken-version advisory message (when not unauthenticated) > `status.message` >
  `Open provider setup to install {driver label} on this environment.` (not installed, setup
  available) > unauthenticated: "Open provider setup to sign in with Google." (Antigravity) /
  "Open provider setup to sign in." (setup available) / "Sign in via the CLI to authenticate
  again." > ready: "No models are available for this provider." > error: `{providerName}
  provider is unavailable.` > `{providerName} provider has limited availability.`
- "Open provider setup" `InlineButton` (text-foreground, font-medium, underline on hover) under
  the message when setup is possible (Antigravity, or `setup.canAuthenticate` /
  `setup.canInstall`); opens the provider setup (settings spec).
- Dismiss: `Button size="icon-xs" variant="ghost-muted"` (24x24 at sm, icon 14px), `XIcon`,
  `aria-label` `Dismiss {providerName} provider {status}`. Dismissal stores the key in ChatView
  state; a new key (status/message change) shows again; the key resets when the banner clears
  (`ChatView.tsx:3780-3798`).

### 4.3 `ThreadErrorBanner`

`components/chat/ThreadErrorBanner.tsx:38-84`. Wrapper
`pointer-events-auto mx-auto w-fit max-w-[min(48rem,calc(100%-2rem))] pt-3`.

- Error source (`ChatView.tsx:1932-1951`): server thread → local error shadow, else
  `session.lastError`; draft → local draft error. Local errors come from failed sends, reverts,
  interrupts (strings in the code paths that call `setThreadError`, e.g. "Interrupt the current
  turn before reverting checkpoints.", `ChatView.tsx:7140`).
- Alert `variant="error"`, icon `CircleAlertIcon`. Description: the error text, 3-line clamp with
  full-text tooltip (top, `whitespace-pre-wrap`).
- ChatGPT usage-limit variant (`isChatGptUsageLimitError(activities, error)`,
  `shared:usageLimits`): icon = OpenAI logo 16px in `foreground`; description two lines
  "ChatGPT usage limit reached" (font-medium) / "Review your usage settings in ChatGPT to
  continue." (`space-y-1`); action `ChatGptUsageButton` (`variant="default" size="sm"`):
  "Manage usage" + `ExternalLinkIcon` 14px, opens the ChatGPT usage URL externally
  (`components/settings/ChatGptUsageButton.tsx`).
- Dismiss: `Button variant="ghost" size="icon-xs"`, `XIcon` in `text-destructive`,
  `aria-label="Dismiss error"`. Dismissal is session-scoped per `threadKey + message`
  (module-level set): navigating away and back keeps it hidden, a different message shows again
  (`:21-36`). Dismissing also clears the local error shadow.

### 4.4 Other banners

Version skew, disconnected environment, branch mismatch, snoozed / settled / woke-from-snooze,
resume compaction, usage limits, project clone, feedback, background liveness: all go through the
**composer banner stack** (`ChatView.tsx:6383-6660` builds `composerBannerItems`, rendered by
`ChatComposer` → `ComposerBannerStack`). See [`composer.md`](composer.md).

---

## 5. The timeline slot

`ChatView.tsx:9938-10028`, details in `chat-timeline.md`.

- `MessagesTimeline` fills the messages wrapper (`relative flex min-h-0 flex-1 flex-col
  bg-background`). The list itself is full height; rows scroll under the composer overlay and under
  the top banners.
- Inputs ChatView computes for it (see `chat-timeline.md` section 1 for the derivation):
  `timelineEntries`, `latestTurn`, `runningTurnId`, `turnDiffSummaries` (= `thread.checkpoints`),
  `isWorking`, `isPreparingWorktree`, `isCompacting`, `activeTurnStartedAt`, `worktreeSetup`,
  `queuedMessages`, `supportsConversationRollback`, `contentInsetEndAdjustment`
  (= composer timeline inset, 6.3), `liveFollowEnabled`, `anchorMessageId`, `topFadeEnabled`,
  `hideEmptyPlaceholder` (= hero state or timeline loading), `loadEarlier`, `timestampFormat`,
  `markdownCwd` (git cwd), `workspaceRoot`, provider `skills`.
- `isWorking` (`ChatView.tsx:3295-3301`) = session phase `running` OR a local send in flight OR
  connecting OR reverting OR compacting OR a worktree bootstrap still running before the first turn.
  `derivePhase` (`session-logic.ts:1738-1750`): no session / `stopped|interrupted|error` →
  disconnected; `starting` → connecting; `running` → running; else ready.
- `activeTurnStartedAt` = `deriveActiveWorkStartedAt` (`session-logic.ts:206-223`): when the
  session runs turn T and `latestTurn` is T → `latestTurn.startedAt ?? localSendStartedAt ??
  latestUserMessageAt`; running another turn → local send time ?? latest user message time; latest
  turn unsettled → `latestTurn.startedAt ?? localSendStartedAt`; else local send time.
- **Held timeline across thread switches** (`ChatView.logic.ts:289-412`, `ChatView.tsx:3583-3601`):
  while the target thread's timeline is loading (`threadSyncPhase` set, or the composer layout has
  not been measured yet), keep painting (a) the target's own last ready entries (cache of 16
  threads, module scope) or (b) the previously shown thread's entries if it is in the same
  environment, or (c) nothing. While painting another thread's snapshot ("paint-only"), every
  callback is a no-op and live state (working rows, citations) is suppressed. Never hand the list an
  empty first frame for a cached thread.
- **Freshly synced rows** (`ChatView.tsx:3602-3640`): rows that appear when a loading/syncing
  thread finishes syncing (ids not in the baseline captured when loading began) get the
  `timeline-sync-entry` class: opacity 0 → 1 over 600ms ease-out, once (`index.css:2050-2061`,
  only without reduced motion).

---

## 6. Composer slot, hero state, scroll-to-end pill

### 6.1 Docked overlay (existing threads)

`ChatView.tsx:10055-10301`, `components/chat/ComposerSurface.tsx:6-37`.

```
div[data-chat-composer-overlay]  pointer-events-none absolute inset-x-0 bottom-0 z-20 pt-2 (8; 6 below 640)
   inert while a checkpoint revert runs
└─ div  w-full ps/pe = --workspace-gutter (20px; 12 below 640)          (draft hero transition group)
   └─ div[data-chat-composer-stack] group/composer-stack pointer-events-auto relative z-10
         mx-auto w-full max-w-(--chat-max-width)
      ├─ [hero headline]  only in hero state (6.2)
      └─ div.relative
         ├─ ComposerSurface.Shell  (@container/composer-surface, max-w chat width; [`composer.md`](composer.md))
         │  ├─ ComposerSurface.Host → ChatComposer                      ([`composer.md`](composer.md))
         │  └─ div.min-h-0 > div.relative.z-0[data-terminal-open]
         │       └─ [context strip host] min-h 32px (36 below 640) when visible → BranchToolbar
         └─ div aria-hidden  height 20px (16 below 640) + safe-area bottom   (bottom spacer)
```

- `--chat-max-width` from setting `chatWidth` (default `"comfortable"`): comfortable 48rem
  (768px), wide 72rem (1152px), full 100% (`index.css:2116-2127`, `routes/__root.tsx:295-298`,
  `contracts:settings.ts:295-306`). The same variable bounds every timeline row.
- The composer's bottom edge (including its context strip) sits 20px above the window bottom.
- Context strip mounting (`ChatView.tsx:3826-3838`, `BranchToolbar.logic.ts:66-77`): mounted when
  the thread has a project and (git repo, or environment indicator shown, or a server thread whose
  resting composer may host controls there); visible (`min-h`) only when it actually has content.
- Pending approvals, pending user input, the plan follow-up prompt, queued-message steering, the
  "Loading messages..." status and the composer banner stack all render inside `ChatComposer`
  (props at `ChatView.tsx:10157-10170`). See [`composer.md`](composer.md).

### 6.2 Hero state (empty draft)

`ChatView.logic.ts:255-276` `resolveDraftHeroState`, `ChatView.tsx:3717-3738,10059-10092`,
`components/chat/DraftHeroHeadline.tsx:53-385`.

Hero when: no worktree setup card AND (a background submission is pending, OR (local draft AND
no timeline entries AND not working AND the hero has not been asked to dock)). Sending from the
hero sets "dock requested" for that thread (`shouldDockDraftHeroForSubmission`,
`ChatView.logic.ts:213-223`).

Layout in hero state:

- The overlay becomes `pointer-events-none absolute inset-0 z-20 flex items-center`: the composer
  stack is **vertically centered** in the chat column (header excluded).
- The headline sits above the composer: `absolute inset-x-0 bottom-full z-0`, inner padding-bottom
  32px (16px when the stack contains a `[data-composer-shoulder-tab]` element).
- Timeline: `hideEmptyPlaceholder` → the list area paints plain `bg-background`
  (`data-timeline-loading`), no "Send a message..." text.

Headline (`DraftHeroHeadline`):

```
div mx-auto flex w-full max-w-5xl (1024) flex-col items-center
├─ h1 w-full text-center font-normal text-3xl (30/36; text-2xl below 640) tracking-tight text-foreground
│    aria-label = full sentence (below)
└─ [p] mt-2 (8) flex h-6 (24) items-center text-sm      only when a scratch ("no project") root exists
```

| State | h1 content | second line |
|---|---|---|
| scratch draft ("no project") | "What should we work on?" | project picker (label "No project") |
| resolved project | "What should we build in {picker}?" | "or start without a project" |
| no project yet, picker has entries | "{picker} to start" (picker label "Choose a project") | "or start without a project" |
| no projects at all | "Add a project to start" | none |

- **Picker trigger**: `InlineButton tone="picker"` (`ui/button.tsx:105-140`): inline-flex gap 6px,
  `text-foreground`, underline dotted `foreground/30`, thickness from font, offset 4px; hover and
  open → solid `foreground` underline. `max-w-64` (256px), `align-baseline`, label truncates.
  Tooltip (top) = project display name (not for scratch). Label: scratch → "No project", else the
  logical project display name, else "Choose a project".
- **Picker menu** (`:252-309`): `MenuPopup align="center"` max-h 320px scrolling. Radio group:
  "No project" (`MessageSquareDashedIcon` 16px in the gray project-icon color) when a scratch root
  exists; one row per logical project (favicon 16px, name truncated with tooltip, environment
  badge when projects span environments); `MenuSeparator`; "Add project" (`FolderPlusIcon`) →
  command palette `add-project`. Choosing a project retargets the open draft in place
  (`setLogicalProjectDraftThreadId`) and, unless the user already picked a model, applies the
  project's default model selection (`:178-208`).
- No picker entries: a plain button with dotted bottom border `muted-foreground/35`, text
  `muted-foreground/60` (hover `/60` border, `/80` text) reading the project title or "Add a
  project"; opens the add-project palette (`:312-318`).
- "or start without a project": `InlineButton tone="muted"` (muted-foreground, hover foreground +
  underline offset 2px); tooltip (bottom) = shortcut label of `chat.newWithoutProject` (default
  mod+alt+N). Starts a scratch draft, then focuses the picker trigger (`:336-358`).

Transition hero → docked (`ChatView.tsx:546-618`, `draftHeroTransition.ts:1-2`): FLIP of the
transition group from the old composer rect to the new one, `translate3d` over
`panelAnimationDurationMs` with `cubic-bezier(0.4, 0, 0.2, 1)`. Off by default (duration 0):
the composer jumps from center to bottom.

### 6.3 Composer inset (how the timeline makes room)

`components/composerFooterLayout.ts:62-90,209-224`, `ChatView.tsx:1791-1801,5966-6058`.

- The composer reports its overlay height (`onComposerOverlayHeightChange`, plus a
  `ResizeObserver` on the overlay element). ChatView turns it into `composerTimelineInset`:
  - expanded composer: inset = overlay height (rounded up);
  - resting (collapsed) composer: inset = `max(currentInset, overlayHeight + 94)` where 94px
    (`COMPOSER_RESTING_EXPANSION_MIN_PX`) is how much taller the empty expanded composer is.
    The reservation never shrinks while resting, so re-expanding never covers rows.
  - On thread switch the inset is rebuilt from this thread's overlay (`:6026-6032`).
- The timeline uses it as bottom padding: `paddingBottom = inset + 16` (12 below 640), or `16`
  only while an anchored first-turn end space is active (`MessagesTimeline.tsx:978-979`).
  LegendList's `maintainScrollAtEnd` ignores footer/inset changes (`footerLayout: false`), so the
  inset changing never moves visible rows.
- The timeline waits to paint a thread with entries until the composer has measured its final
  layout for it (`timelineWaitingForComposerInset`, `ChatView.tsx:3583-3586`).

### 6.4 Scroll-to-end pill

`ChatView.tsx:10030-10051,1694,5378-5382,5478-5493,5724-5759,5966-5996`.

```
div  pointer-events-none absolute left-1/2 -translate-x-1/2 z-30 flex justify-center py-1.5 (6)
     bottom = scrollToEndClearance + 4px
└─ Button size=xs variant=glass  pointer-events-auto  aria-label "Scroll to end"
     h 24, px 7, gap 4, text-xs, rounded-full, border border/60 (hover border), shadow-sm,
     surface-glass bg (fallback solid background), text-foreground
     ChevronDownIcon 14px (icon color contrast-muted-foreground) + "Scroll to end"
```

- Shown when the user scrolled away from the live edge: show is debounced **150ms**, hide is
  immediate. Never shown while live follow owns the scroll (anchored first turn, streaming).
  After a thread switch it shows immediately if that thread was remembered as not-at-end.
- Clearance: `resolveScrollToEndClearance` = overlay height minus the distance between the
  composer main surface top and the top of any attached composer banner that horizontally overlaps
  the pill. Net effect: the pill floats 4px + 6px above the composer card (or above an attached
  banner in its column).
- `pointerdown` is prevented so the composer keeps focus. Click: tells the composer to restore
  from its resting layout, then `scrollToEnd(animated: true)` on the next frame.
- Press feedback: `scale(0.97)` while active, `transition: box-shadow, scale` (button base).

---

## 7. Scroll behavior owned by ChatView

`ChatView.tsx:5378-5900`, `components/chat/timelineScrollAnchoring.ts:1-141`,
`components/chat/MessagesTimeline.logic.ts:141-172`, `components/chat/pageScrollController.ts`.
List-internal mechanics (restore, disclosure anchoring, minimap) are in `chat-timeline.md`.

### 7.1 Scroll modes

`TimelineScrollMode = "following-end" | "anchoring-new-turn" | "free-scrolling"`.

| Mode | Entered by | Effect |
|---|---|---|
| following-end | thread open with no saved "not at end" position; `scrollToEnd`; user scrolls back within 40px of the end | LegendList `maintainScrollAtEnd` keeps the end pinned as rows are added or grow. While working and streaming assistant text (not right after the user's own send), pinning glides (`animated: true`), else instant |
| anchoring-new-turn | sending the **first** user message of a thread (no latest turn, no user message yet) (`ChatView.tsx:8304-8321`) | The new user row is positioned 24px (`CHAT_TIMELINE_ANCHOR_OFFSET`) below the top of the viewport (`scrollToIndex viewPosition 0, viewOffset 24, animated`) with LegendList `anchoredEndSpace` filling the space under it; normal end pinning is off; ChatView scrolls by the overflow amount two frames after each content change so the end of the turn stays visible (`:5762-5806`). Released to following-end as soon as the running turn produces a tool/command/approval work entry (`shouldReleaseTimelineAnchorForToolActivity`, `ChatView.logic.ts:225-248`) |
| free-scrolling | any manual navigation away from the end | No pinning; the scroll-to-end pill appears |

Every later send (not the first) calls `scrollToEnd()` (instant) and stays in following-end.

### 7.2 Manual navigation detection

Attached to the list's scroll node (retries for 12 frames after a thread switch) and to
`document` for keys (`ChatView.tsx:5513-5672`):

- Wheel up (deltaY < 0, not ctrl-zoom, target scrolls in that direction) breaks follow, but only if
  the real content overflows the viewport above the composer. Wheel down at the end restores a
  resting composer.
- Touch move breaks follow once the viewport is out of the 40px end band.
- Pointer down on the scroll node itself (scrollbar drag) breaks follow when content overflows;
  pointer down on content breaks follow only when already away from the end (selecting text near
  the end keeps following).
- Keys PageUp / Home / ArrowUp (target inside the list, or body/html, no modifiers, no editable
  target, no open floating layer) break follow when content overflows and ask the composer to
  collapse; PageDown / End / ArrowDown break follow only when away from the end, collapse the
  composer, and restore it when the end is reached.
- "At end" = `contentLength - scroll - scrollLength <= 40` (`TIMELINE_FOLLOW_REARM_THRESHOLD_PX`).
  Reaching it re-arms following-end, releases any anchor, hides the pill.

### 7.3 Composer page-scroll keys

`createPageScrollController` (`ChatView.tsx:5443-5476`, `components/chat/pageScrollController.ts`)
lets PageUp/PageDown typed in the composer scroll the timeline: scroll padding bottom = overlay
height; starting such a scroll sets intent (`away-from-end` for PageUp), collapses the composer,
and breaks follow under the same rules as 7.2. Key repeat is driven by the controller's own
key-down/key-up tracking.

### 7.4 Per-thread memory

- Last scroll position per thread (row id + offset within row + atEnd flag + disclosure state),
  bounded to 100 threads, module scope (`timelineScrollAnchoring.ts:110-141`). On thread open:
  atEnd or unknown → start at end; otherwise restore the row (`chat-timeline.md`).
- Focus: opening a thread focuses the composer on the next frame unless the terminal drawer is
  open (`ChatView.tsx:5826-5834`). Window focus refocuses the composer after two frames when focus
  isn't already in something that takes typing, except while the terminal drawer is open
  (`:5836-5862`).

---

## 8. Workspace file drop overlay

`ChatView.tsx:9898-9919`. While files are dragged over the chat column:

```
div pointer-events-none absolute inset-2 (8px) z-40 flex items-center justify-center
    rounded-2xl (18) border-2 border-dashed border-primary/60 bg-primary/[0.035]
└─ div role=status flex items-center gap-2 rounded-full border border-primary/25
       bg-background/95 px-4 py-2.5 (16/10) text-sm font-medium text-foreground shadow-lg
     PaperclipIcon 16px text-primary + "Drop files to attach"
```

Drop adds files or folders to the composer (`makeWorkspaceFileDropHandlers`, [`composer.md`](composer.md)).

---

## 9. Dialogs mounted by ChatView

### 9.1 Revert ("Edit from here?")

`ChatView.tsx:7114-7240,10489-10531`, trigger in `chat-timeline.md` (user row "Edit from here").

- Pre-checks (each sets the thread error banner text instead of opening the dialog): provider
  without rollback → "This provider does not support reverting conversation history. Start a new
  thread instead."; environment unavailable → `Reconnect {label} before reverting checkpoints.`;
  running / sending / connecting → "Interrupt the current turn before reverting checkpoints."
- `AlertDialog` (design-system dialog): title "Edit from here?"; description "Rewind chat to before
  this message. Your prompt and attachments return to the composer." plus, when the thread has no
  worktree, " Files stay as they are because this thread shares the project directory."
- Footer: `Cancel` (`Button variant="outline"`, closes); when the thread has a worktree:
  "Revert files too" (`variant="destructive"`) → revert with `restoreFiles: true`; always
  "Revert and keep changes" (default variant) → `restoreFiles: false`.
- Effect: marks the thread rewinding (composer shows "Rewinding conversation" as send-disabled
  reason, overlay `inert`), dispatches `thread.checkpoint.revert` (restoreFiles true) or
  `thread.conversation.revert` (restoreFiles false) with `{threadId, turnCount}`
  (`crt:operations/commands.ts:368-378`), waits for the reverted message to disappear, then puts
  the message text into the composer (appended after any existing draft with a blank line) and
  re-attaches its attachments, and focuses the composer at the end. Errors go to the thread error
  banner: "Wait for attachments to finish preparing before rewinding.", "The environment is not
  connected.", "Make room for this message's attachments in the composer before rewinding.", or
  "Failed to revert thread state.".

### 9.2 Branch switch confirmation

`ChatView.tsx:10312-10340`: title `Switch to <code>{threadBranch}</code>?`, description "You have
uncommitted changes. They'll carry over to the other branch, or block the switch if they
conflict.", buttons Cancel (outline) / "Switch branch" (default). Opened from the composer's branch
mismatch banner ([`composer.md`](composer.md)).

### 9.3 Expanded image / media viewer

`components/chat/ExpandedImageDialog.tsx:67-288`, `components/chat/ZoomableImage.tsx`,
`components/ui/dialog-styles.ts:1-14`. Opened by clicking user-message images, markdown images,
viewed-image work entries, video attachments.

- Backdrop `bg-black/75` (no blur), fade 200ms. Popup transparent, `max-h-92vh w-92vw`, media box
  `--media-width` = 92vw − 96px (≥640), `--media-height` = min(86vh, 100vh − 160px).
- Close button `icon-xs` `media-close` (black/65 bg, white icon, ring white/20) absolutely at
  `right 0, top −40px` of the media; `aria-label` `Close image preview` / `Close video preview`.
  Escape closes (capture phase). Clicking the backdrop area closes. Focus returns to the opener.
- Multiple items: prev/next `icon` buttons (`media-navigation`, white/90, hover white/10 bg) at the
  left/right edges, vertically centered; `aria-label` "Previous media" / "Next media";
  ArrowLeft/ArrowRight navigate (wrapping). Caption under the media: `mt-2 text-xs text-white/80`
  `{name} ({i}/{n})`.
- Image: `ZoomableImage`: fits within the box, wheel zooms around the pointer (factor
  `exp(-delta × 0.002)`, ×5 with ctrl / pinch), 1x–8x, drag to pan when zoomed, arrow keys pan 40px
  when zoomed, resize resets to 1x. Container `rounded-lg bg-background shadow-2xl ring-1
  ring-border/70`.
- Failure: black box 192px tall, `rounded-lg`, text "Image unavailable. The file may have been
  moved or deleted." or "This image could not be loaded." + open-original link.
- Snap-shot attachments add a contents toggle ("Show extracted text" / "Show accessibility JSON" /
  "Show screenshot"). Media actions context menu wraps the media (media spec).

### 9.4 Others

`PullRequestThreadDialog` (checkout a PR into a draft), `LinkPullRequestDialogHost`, device setup
wizard: pull-request / device specs.

---

## 10. Keyboard

`ChatView.tsx:6780-7065` (window `keydown`, **capture** phase), plus 1.4.

- Ignored when there is no active thread or the command palette is open; `defaultPrevented`
  events are ignored unless a terminal owns focus.
- **Type-to-focus** (`:672-713,6801-6812`): a printable key (length 1, no meta/ctrl/alt, not
  composing) whose target is not editable or interactive (`input, textarea, select,
  contenteditable, role=textbox`; `button, a[href], summary, role=button|checkbox|menuitem|option|
  radio|switch|tab`), with no open dialog/menu/popover/select/combobox/sheet, and not a right-panel
  launcher letter → inserted at the end of the composer and focuses it.
- **Paste-to-focus** (`:7067-7100`): plain-text paste with nothing editable focused goes into the
  composer (terminal focus and open model picker excluded).
- Commands (defaults `shared:keybindings.ts`):

| Command | Default | Action |
|---|---|---|
| `thread.copyReference` | mod+shift+C | Copy a reference to the thread |
| `thread.settle` | mod+shift+S | Settle / unsettle (server threads); error toast "Failed to settle thread" |
| `thread.pin` | mod+shift+P | Pin / unpin; "Failed to pin thread" / "Failed to unpin thread" |
| `terminal.toggle` | mod+J | Toggle terminal drawer |
| `rightPanel.toggle` | mod+alt+B | Toggle right panel |
| `rightPanel.toggleMaximized` | (none) | Toggle maximize |
| `rightPanel.close` | mod+W when `!terminalFocus` | Close active right-panel surface; with none open the event passes through (closes the window) |
| `terminal.split` / `splitVertical` / `close` / `new` | mod+D, mod+W, ... when `terminalFocus` | Terminal spec |
| `diff.toggle` | mod+D when `!terminalFocus` | Toggle diff panel |
| `modelPicker.toggle` | mod+shift+M | Composer model picker |
| `composer.host/effort/mode/workspace/branch/previousWorktree` | mod+shift+H, ... | Composer controls ([`composer.md`](composer.md)) |
| `thread.steerQueuedMessage` | mod+shift+Enter | Send the oldest queued message now |
| `thread.stop` | (none by default) | Interrupt the running turn |
| project script commands | user-defined | Run that script |

---

## 11. Settings and feature presence (fe7d3092c)

Client settings that change this view (`contracts:settings.ts:298-513`):

| Setting | Default | Effect here |
|---|---|---|
| `chatWidth` | `"comfortable"` | `--chat-max-width` 768 / 1152 / 100% |
| `timestampFormat` | `"locale"` | Row timestamps (`chat-timeline.md`) |
| `panelAnimationDurationMs` | `0` | Header padding, panel, hero FLIP animations off |
| `planModeEnabled` | `false` | Restores the composer Build/Plan toggle and `/plan` `/default` ([`composer.md`](composer.md)). Plans still render inline when the server produces them |
| `accessLevelIndicatorEnabled` | `false` | Restores the composer access-level selector; runtime mode otherwise follows project defaults (`ChatView.tsx:1952-1956`) |
| `contextWindowMeterEnabled` | `false` | Composer context meter |
| `composerCollapseOnScroll` | `true` | Scrolling the timeline rests the composer (6.3) |
| `wordWrap` | `true` | Initial wrap state of markdown code blocks and expanded tables (`markdown.md`) |
| `fontSizeCode` | `13` | Code text in markdown (`--font-size-code`) |
| `browserLinkTarget` | `"system"` | Where markdown links open (`markdown.md`) |
| `followUpBehavior` | `"queue"` | Sends during a running turn become queued-message rows (`chat-timeline.md`) |

Feature presence: plan / default interaction modes **exist** (thread `interactionMode`, plan
follow-up prompt `shouldShowPlanFollowUpPrompt`, `ChatView.logic.ts:950-964`) but the toggle is
hidden by default. Runtime-mode / access selection **exists** behind `accessLevelIndicatorEnabled`.
The right-panel plan surface is gone; the composer shows a tasks badge for the active
`turn.plan.updated` plan ([`composer.md`](composer.md)). The timeline minimap **exists** and is not
setting-gated (`chat-timeline.md`).

---

## 12. Data and client API map

| Need | Upstream source | t3UI today |
|---|---|---|
| Thread detail (messages, activities, plans, checkpoints, latest turn, session) | `orchestration.subscribeThread` + `GET /api/orchestration/threads/:id` (protocol.md 5.2-5.3) | `t3_client::Environment::open_thread` → `ThreadHandle::state()` (`Arc<ThreadState>`) |
| Sync phase | thread status `empty/cached/synchronizing/live/deleted` | `ThreadState.status: SyncStatus` + `deleted` |
| Load earlier turns | HTTP thread snapshot with `beforeCursor` | `ThreadHandle::load_older`, `ThreadState.page` |
| Provider status | `ServerConfig.providers[]` | `Environment::config()` |
| Rename | `thread.meta.update` | `commands::rename_thread` |
| Revert with files | `thread.checkpoint.revert {threadId, turnCount}` | `commands::revert_checkpoint` |
| Revert history only | `thread.conversation.revert {threadId, turnCount}` | **missing helper** (`ClientCommand::ThreadConversationRevert` exists in `t3-protocol/src/commands.rs:208`, no `commands::` constructor) |
| Interrupt | `thread.turn.interrupt` | `commands::interrupt_turn` |
| Settle / pin | `thread.settle`, pin commands | `commands::settle_thread`, pin: shell spec |
| Keybindings | server config keybindings | keybindings module (`t3-logic/src/keybindings`) |
| `supportsConversationRollback` | `ServerProvider.supportsConversationRollback` (optional; absent means supported, `ChatView.tsx:2936-2938`) | **wrong default**: `t3-protocol/src/server.rs:238-239` decodes absent as `false`; needs `Option<bool>` or default `true` |

---

## 13. Reuse map

| t3UI module | Verdict |
|---|---|
| `t3-app/src/chat/mod.rs` | Keep mechanics: thread subscription, `LocalDispatch` (port of `hasServerAcknowledgedLocalDispatch`, `ChatView.logic.ts:1270`), slot registry, `OverlayGeometry`. Change: overlay inset must follow `resolveComposerTimelineInset` (6.3), not a fixed 75% rule; add hero state, held timeline, sync-entry fade |
| `t3-app/src/chat/header.rs` | July visuals: replace with 3.1-3.4 (breadcrumb with project crumb, no border, fixed panel controls, collapsed actions menu, inline rename) |
| `t3-app/src/chat/banners.rs` | July visuals: replace with section 4 (glass alerts floating over the timeline, provider banner variants, ChatGPT usage variant) |
| `t3-app/src/chat/body.rs` | Keep overlay measurement and toggles; replace the revert `window.prompt` with the 9.1 dialog and the restoreFiles split; layout code is July-era |
| `t3-app/src/chat/controls.rs` | July visuals (letter-spacing hack may still help elsewhere) |
| `t3-app/src/chat/timeline.rs` | Keep list bookkeeping, tail follow, manual-navigation flag, disclosure anchor; rework to the three scroll modes in 7.1 and the 40px re-arm band |
| `t3-snapshots/src/scenes/chat.rs` | Harness reusable; scenes target July references |
| `docs/handoff/chat.md` | Read first: GPUI list gotchas (splice resets anchor, scroll handler only on wheel, scrollbar drag polling) apply directly to 7.2 |

---

## 14. Reference screenshots needed

All at 1440x900 @2x, dark and light, seeded with `e2e/seed.mjs` + fake Codex, unless noted.

| Name | How to reach | Data |
|---|---|---|
| `chat-hero-draft` | Ctrl/Cmd+N in aurora (or open `/` with projects) | empty draft: centered composer, "What should we build in aurora?" + "or start without a project" |
| `chat-hero-picker-open` | click the project name in the hero | project menu with "No project", 3 projects, "Add project" |
| `chat-header-default` | open `thread-aurora-tour`, sidebar expanded | header breadcrumb project / title, scripts + git actions, panel toggles |
| `chat-header-sidebar-collapsed` | same, collapse sidebar | header padding-left 122px past traffic lights |
| `chat-header-title-hover` | hover the title | chevron visible + tooltip |
| `chat-header-renaming` | double-click the title | inline rename input |
| `chat-header-actions-collapsed` | open right panel and shrink the window until the header < 512px | "More header actions" ellipsis menu (open) |
| `chat-panel-controls-right-open` | open right panel (diff) on aurora-tour | controls over the panel tab bar, maximize toggle visible |
| `chat-scroll-to-end-pill` | aurora-tour, scroll up 600px | pill above the composer |
| `chat-thread-error-banner` | `thread-cirrus-deploy` (failure scenario; error in session.lastError) | glass error alert over the timeline top |
| `chat-provider-status-banner` | run a server whose fake provider reports `warning` (needs a seeded provider status; see open questions) | warning alert |
| `chat-revert-dialog` | aurora-tour, hover the user message, click "Edit from here" | "Edit from here?" dialog with both revert buttons (thread with worktree) or the single button (no worktree) |
| `chat-drop-overlay` | drag a file over the chat column | dashed overlay pill "Drop files to attach" |
| `chat-image-viewer` | thread with an image attachment (seed one), click it | media viewer with close + caption |
| `chat-no-active-thread` | route with a deleted thread in an empty environment | "Pick a thread to continue" |
| `chat-loading-switch` | switch quickly between two seeded threads | held timeline, no blank frame (video or two frames) |

---

## 15. Open questions / risks

- **Message `phase`.** Several rules key on `message.phase === "final_answer" | "commentary"`.
  That field exists only in the fork's contracts (`contracts:orchestration.ts:574-579`), not in
  upstream b33eda13, so a `t3@nightly` server never sends it. Port the rules with `phase: None`
  behaving exactly as the fork does when the field is absent (it is optional there too). Do not
  invent a phase.
- **Backdrop blur.** Banners, the pill and the composer are glass. GPUI falls back to solid
  surfaces; the look will differ from the reference where rows scroll under them. Accept, or
  approximate with a translucent fill (no blur).
- **`text-box: trim`** on breadcrumb text has no GPUI equivalent; position by cap height.
- **Native context menus.** The title menu and header context menu use the Electron native menu
  via the desktop bridge. Decide whether T3UI draws them with gpui-kit menus (visual mismatch with
  macOS native menus) or calls the macOS menu API.
- **Provider-status screenshot** needs a server whose provider snapshot is `warning`/`error`; the
  fake Codex reports ready. The harness may need a provider that is installed but unauthenticated.
- **Surface grain** (3.5% noise on `SidebarInset`): design spec decides.
- **Default keybindings** for `thread.stop` and `rightPanel.toggleMaximized` were not found in
  `shared:keybindings.ts`; confirm in the keybindings spec.
