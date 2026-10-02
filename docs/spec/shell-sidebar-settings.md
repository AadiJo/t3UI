# Shell, sidebar, settings, command palette, toasts, keybindings: build spec

> **Stale UI warning (2026-10-02):** this spec was written against a July checkout of the fork
> (`ddeeb09`), 4,601 commits behind the real target (`fe7d3092c`, see AGENTS.md). UI details may be
> wrong until a "Refreshed against fe7d3092c" note appears here. Protocol facts are unaffected.


Sources: fork `~/L-Projects/t3code-again` @ `ddeeb09d`, upstream `~/L-Projects/t3UI-refs/t3code-upstream` @ `b33eda13`.

Path prefixes: `web/` = fork `apps/web/src/`, `desk/` = fork `apps/desktop/src/`, `pkg/` = fork `packages/`, `up:` = upstream repo root.

Conventions used below:

- Tailwind 1 unit = 4px. All sizes are the desktop values: the window min width is 840, so `sm:` (>=640) and `md:` (>=768) always apply and `max-sm:` never does. The mobile `Sheet` sidebar (below 768) never renders on desktop.
- Colors are token names from `docs/spec/tokens.json` (`muted-foreground/60` = token at 60% alpha; `dark:` values given when they differ).
- Radii: `rounded` 4, `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18, `rounded-full`.
- Type: `text-xs` 12/16, `text-sm` 14/20, `text-base` 16/24, `text-xl` 20/28, `text-2xl` 24/32. `text-[10px]`, `[11px]`, `[13px]` set size only; line height is inherited (under a `text-xs` parent that is the unitless ratio 1.333). medium = 500, semibold = 600. `tracking-wider` .05em, `tracking-wide` .025em, `tracking-tight` -.025em.
- Icons are lucide names. `size-3` = 12px, `size-3.5` = 14px, `size-4` = 16px.
- "Stacked toast" = `stackedThreadToast()` layout (body, then action row aligned right). See §5.1.
- "Native confirm" = desktop message box (§5.4). "Native menu" = OS context menu (§5.5).
- UI copy is quoted exactly, including typographic characters (`…`, `—`, `·`, `⌘`).

---

## 0. Global finding: the `sidebar-*` color utilities render nothing

`web/index.css` defines no `--color-sidebar*` theme variables. Compiling `index.css` with tailwindcss 4.3.0 (from the fork's `node_modules`) confirms `bg-sidebar`, `bg-sidebar-accent`, `bg-sidebar-border`, `border-sidebar-border`, `text-sidebar-foreground`, and `text-sidebar-accent-foreground` generate no CSS. tailwind-merge still removes conflicting classes in their favor. Port these as "no style":

| Element | Pointer | Effect in the fork |
| --- | --- | --- |
| `SidebarSeparator` | `web/components/ui/sidebar.tsx:672` | twMerge drops `bg-border`, so the 1px line (with 8px side margins) is transparent. Treat it as an invisible 1px spacer. |
| `SidebarMenuButton` base hover and active fills | `ui/sidebar.tsx:782-799` | No fill. Only call sites that add `hover:bg-accent` get a hover fill. `data-active` only adds `font-medium`. |
| Settings nav items | `web/components/settings/SettingsSidebarNav.tsx:75-93` | No hover or active background. Only text color and weight change. |
| Rail hover line | `ui/sidebar.tsx:598` | Invisible. Only the cursor changes. |
| Thread list left rule | `ui/sidebar.tsx:944` | `border-l` (1px) survives and falls back to the global `* { border-color: var(--border) }`, so every thread list has a visible 1px `border` rule on its left. |

`tokens.json` `gpuiComponentTheme` maps `sidebar.*` colors. Do not use them for these elements. Confirm against reference screenshots.

---

## 1. App root and routing

### 1.1 Provider tree

`web/main.tsx:22-50` → (Clerk + `ManagedRelayAuthProvider` only when a Clerk key and cloud config exist) → `AppRoot` (`web/AppRoot.tsx:13-21`: atom registry > `RouterProvider` + `PreviewAutomationHosts` + `ElectronBrowserHost`) → root route (`web/routes/__root.tsx:55-83`, `beforeLoad` resolves `authGateState`) → `RootRouteView` (`__root.tsx:85-141`).

When the primary environment is authenticated (`__root.tsx:117-140`):

```
ToastProvider                        top-right toast stack (§5.1)
└ AnchoredToastProvider              element-anchored toasts (chat copy buttons only)
  ├ DocumentTitleSync                document.title = "HOME-PC"
  ├ AuthenticatedTracingBootstrap
  ├ RelayClientInstallDialog, ConnectOnboardingDialog   cloud (connections spec)
  ├ SshPasswordPromptDialog          §5.3
  ├ SlowRpcRequestToastCoordinator   §5.2
  ├ EventRouter                      §1.5
  ├ ProviderUpdateLaunchNotification §5.2
  └ CommandPalette                   §4 (wraps everything below; provides "open add project")
    └ AppSidebarLayout               §1.3 and §2
      └ <Outlet/>                    route view
```

When not authenticated, only `DocumentTitleSync` + `<Outlet/>` render (the pair route). Fonts: DM Sans Variable (sans), JetBrains Mono 400/500 as the mono fallback after SF Mono (`main.tsx:8-10`).

### 1.2 Route table

| Path | File | Renders | Guards and redirects |
| --- | --- | --- | --- |
| `/` | `web/routes/_chat.index.tsx` | `NoActiveThreadState` | `_chat` `beforeLoad`: not authenticated → replace `/pair` (`web/routes/_chat.tsx:160-168`) |
| `/$environmentId/$threadId` | `web/routes/_chat.$environmentId.$threadId.tsx` | `ChatView` `routeKind="server"` | Renders nothing until the env shell snapshot loads. If the thread is neither a server thread nor a draft and the env has any threads → replace `/` (`:43-51`). Once the server thread has started and a draft exists for it → `finalizePromotedDraftThreadByRef` (`:53-58`). |
| `/draft/$draftId` | `web/routes/_chat.draft.$draftId.tsx` | `ChatView` `routeKind="draft"` | Draft promoted to a started server thread → replace `/$env/$thread`. No draft session → replace `/`. |
| `/settings` | `web/routes/settings.tsx` | redirect | replace `/settings/general` (`:125-127`) |
| `/settings/{general,keybindings,providers,source-control,connections,archived,diagnostics}` | `web/routes/settings.*.tsx` | panels (§3) | same auth guard |
| `/pair` | `web/routes/pair.tsx` | pairing | connections spec |

Native `Route` enum mirrors this (see `docs/architecture.md`). Navigation pushes unless marked replace. Settings Back and Escape use `canGoBack ? history.back() : navigate("/")`. Electron uses hash history, and a relaunch always starts at `/`. Nothing restores the last route.

### 1.3 Window and shell layout

Electron window (`desk/window/DesktopWindow.ts:166-180, 276-290`):

| | macOS | Windows / Linux |
| --- | --- | --- |
| Size | 1100×780, min 840×620 | same |
| Title | "HOME-PC" | same |
| Chrome | `titleBarStyle: "hiddenInset"`, traffic lights at `{x:16, y:18}` | `titleBarStyle: "hidden"` + `titleBarOverlay` height 40 (native min/max/close at the right, symbol color follows theme) |
| Material | transparent, `vibrancy: "under-window"`, `visualEffectState: "active"` | opaque bg `#0a0a0a` dark, `#ffffff` light |

Layout variables (`web/index.css:7-49`, override in `web/components/AppSidebarLayout.tsx:16`):

| Variable | macOS | Win/Linux (window-controls-overlay) |
| --- | --- | --- |
| `--workspace-topbar-height` | 52 | 40 |
| `--workspace-controls-left` | 90 | 12 |
| `--workspace-controls-right` | 12 | width of native controls + 12 |
| `--workspace-titlebar-control-size` | 28 | 28 |
| `--workspace-titlebar-control-gap` | 12 | 12 |
| `--workspace-titlebar-content-left` (left + 28 + 12) | 130 | 52 |
| `--workspace-panel-transition` | 180ms `cubic-bezier(0.4,0,0.2,1)` | same |

Box tree (`AppSidebarLayout.tsx:56-104`, `ui/sidebar.tsx:94-317, 625-637`):

```
SidebarProvider    flex row, w-full, h=100vh; data-sidebar-state=expanded|collapsed; --sidebar-width default 256
├ Sidebar root     data-state, data-collapsible="offcanvas" only while collapsed
│ ├ gap            relative, w=--sidebar-width (0 when collapsed); transition width 180ms   (reserves flex space)
│ └ container      fixed, top 0, bottom 0, left 0 (collapsed: left = -width), h=100vh, w=--sidebar-width, z 10
│   │              transition left/width 180ms; border-right 1px `border`
│   │              bg `app-sidebar-glass`; backdrop blur 36px saturate 145%
│   └ inner        flex col, h-full, bg transparent → ThreadSidebar (§2) + SidebarRail (§2.1)
├ main             route view's SidebarInset <main>: flex-1, min-w-0, flex col, h=100vh
│                  bg `app-main-glass`; backdrop blur 24px saturate 125%; direct `.bg-background` children are transparent
└ SidebarControl   fixed, left=controls-left, top=controls-top, h=topbar, z 50, flex items-center
```

- `html` and `body` are transparent under `.desktop-glass-window` (Electron) so the native material shows through (`index.css:545-548`). `body::after` paints a fractal-noise tile (256px) at opacity .035 over everything (`tokens.json` `noiseOverlay`).
- Native: on macOS use window blur (`WindowBackgroundAppearance::Blurred`, gpui-pre `src/platform.rs:2449`) and paint the sidebar and main surfaces with the glass alpha colors. Win/Linux: opaque `background`.
- Right panel and terminal drawer live inside `ChatView` (chat spec).
- Collapsed-sidebar contract: every main-column header carries `COLLAPSED_SIDEBAR_TITLEBAR_INSET_CLASS` (`web/workspaceTitlebar.ts:1-2`). While collapsed, padding-left = `--workspace-titlebar-content-left` (130 mac, 52 win), animated `padding-left 200ms linear`.

SidebarControl (`AppSidebarLayout.tsx:18-54`, `ui/sidebar.tsx:319-348`):

- Ghost `Toggle`, forced 28×28, `rounded-lg`, hover bg `accent`, pressed (sidebar open) bg `accent`.
- Icon `PanelLeftClose` when open, `PanelLeftOpen` when collapsed, 14px at 80% opacity. aria "Toggle main sidebar".
- Tooltip below: "Toggle main sidebar (⌘B)". The parenthetical comes from the keybinding label and is omitted when unbound.
- On macOS it sits at x=90 in the 52px header strip, right of the traffic lights (x≈16-68).
- The open state is not persisted: `defaultOpen` is true, and `setOpen` writes a `sidebar_state` cookie that nothing reads. Width is persisted (§2.1).

Index view `NoActiveThreadState` (`web/components/NoActiveThreadState.tsx`):

- Header: topbar height, border-bottom, px 20, window drag region. Text "No active thread", `text-xs`, `muted-foreground/50`.
- Body: centered block max-w 512, px 32, py 48. Title "Pick a thread to continue" (`text-xl`, `foreground`). Description "Select an existing thread or create a new one to get started." (mt 8, `text-sm`, `muted-foreground/78`).

### 1.4 Navigation model

Drafts live in the composer draft store (`t3code:composer-drafts:v1`, composer spec), keyed by `draftId`. Each holds `logicalProjectKey`, a pre-generated `threadId`, `branch`, `worktreePath`, `envMode`, `startFromOrigin`, and `promotedTo`.

`handleNewThread(projectRef, opts?)` (`web/hooks/useHandleNewThread.ts:32-188`):

1. `logicalProjectKey = deriveLogicalProjectKeyFromSettings(project)` (§2.7).
2. If a stored draft exists for that key and its `threadId` is not yet a server thread: apply `opts` to it, set it as the project's draft, navigate `/draft/<id>` (skip if already there).
3. Else if the current route is an unpromoted draft of the same logical project: update its context in place. No navigation.
4. Else create a new `draftId` + `threadId`. `envMode = opts.envMode ?? env.defaultThreadEnvMode`. `startFromOrigin = opts ?? (envMode == worktree && env.newWorktreesStartFromOrigin)`. Default runtime mode, apply the sticky model selection, navigate `/draft/<id>`.

Other navigation rules:

- When the first turn starts, the draft route replaces itself with `/$env/$thread`.
- The default project for global `chat.new*` with no active thread is the first project in `uiStateStore.projectOrder` order (`useHandleNewThread.ts:190-229`).
- `chat.new` (`web/lib/chatThreadActions.ts:140-150`): uses the active thread's or draft's project (else the default) and carries its branch, worktreePath, and envMode. `chat.newLocal` uses the same project with no options.
- Archiving the active thread navigates to a new draft in its project (`web/hooks/useThreadActions.ts:91-134`).
- Deleting the active thread replaces the route with the newest remaining thread in the same project (sidebar thread sort), else `/` (`useThreadActions.ts:150-300`). Delete also stops a non-stopped session, closes the terminal with `deleteHistory`, and clears drafts and terminal UI state. If the thread was the only one on its worktree, a native confirm asks `This thread is the only one linked to this worktree:\n<basename>\n\nDelete the worktree too?`. Yes → `vcs.removeWorktree` with force.
- Server welcome (`__root.tsx:276-315`): set the active environment, expand the bootstrap project, and if the path is `/`, replace-navigate to `bootstrapThreadId` once. Upstream still sends `bootstrapThreadId` (`up:packages/contracts/src/server.ts:804`).

### 1.5 EventRouter

`__root.tsx:258-397`. On a `keybindingsUpdated` server config event (`web/components/KeybindingsUpdateToast.logic.ts`):

- Any issue whose kind starts with `keybindings.` → stacked warning toast "Invalid keybindings configuration" / issue message / outline action "Open keybindings.json". The action opens `serverConfig.keybindingsConfigPath` in the preferred editor (`shell.openInEditor`). On failure: error toast "Unable to open keybindings file" / message or "Unknown error opening file.".
- Otherwise a success toast "Keybindings updated" / "Keybindings configuration reloaded successfully.", at most once per 2000ms.

The desktop menu action `open-settings` navigates to `/settings` (`AppSidebarLayout.tsx:63-78`).

### 1.6 Root error view

`__root.tsx:179-220`. Full-window centered card: max-w 576, `rounded-2xl`, border `border/80`, bg `card/90`, p 32, `shadow-2xl`, backdrop blur, with a red radial glow at the top. Contents:

- App display name: `text-[11px]` semibold, tracking .18em, uppercase, muted.
- "Something went wrong." (`text-3xl` semibold).
- Message: `error.message` or "An unexpected router error occurred." (`text-sm`, muted).
- Buttons: sm "Try again" (reset) and sm outline "Reload app".
- Disclosure "Show error details" / "Hide error details" with the stack in a `pre` (max-h 224, `text-xs`).

---

## 2. Sidebar

Main file: `web/components/Sidebar.tsx` (default export at `:3064-3710`). Logic: `web/components/Sidebar.logic.ts`. Primitives: `web/components/ui/sidebar.tsx`.

Not present in the fork sidebar: environment sections (environments only appear as a project badge, §2.8), an archived section (archive lives in Settings > Archive), and a search/filter field (the "Search" row opens the command palette).

### 2.1 Width, resize, collapse

| Item | Value | Pointer |
| --- | --- | --- |
| Default width | 256 (`16rem`) | `ui/sidebar.tsx:27` |
| Min width | 208 (`13*16`) | `AppSidebarLayout.tsx:14` |
| Max width | unbounded, but a drag step is rejected unless `wrapperWidth - nextWidth >= 640` | `AppSidebarLayout.tsx:15, 92-94` |
| Persisted width | localStorage `chat_thread_sidebar_width` (JSON number). Read on mount and clamped to min only (the 640 rule is not applied at load). Written on drag end. | `ui/sidebar.tsx:562-574, 398-400` |
| Collapse | offcanvas: container slides to `left: -width`, gap shrinks to 0, 180ms `cubic-bezier(0.4,0,0.2,1)`. Transitions are disabled during a drag. | `ui/sidebar.tsx:280-303` |

Rail (`ui/sidebar.tsx:354-623`):

- Absolute, full height, 16px wide, z 20, straddling the sidebar's right edge (8px each side).
- Cursor `w-resize` while expanded, `e-resize` while collapsed. While collapsed (offcanvas) it has `pointer-events: none`.
- Pointer down (left button, expanded): capture the pointer, set body cursor `col-resize` and `user-select: none`, start width = current width.
- Pointer move: `width = clamp(start + dx, 208, ∞)`, applied once per animation frame only if the 640 rule accepts it. More than 2px of movement marks it a drag.
- Pointer up: persist the width. Clicks are suppressed after a drag, and a click while expanded does nothing (no toggle).
- Tooltip right: "Drag to resize sidebar" (resizable + open), else "Toggle Sidebar".

### 2.2 Box tree

```
ThreadSidebar (inside sidebar inner, flex col)
├ [prewarm] first 10 visible threads subscribe to thread detail (no UI)        Sidebar.tsx:246-249, 3453-3464
├ SidebarChromeHeader  h=topbar (52/40), shrink-0, flex row items-center, px 0; Electron: window drag region   :2714-2730
│                      (brand link is display:none via `.sidebar-brand`, index.css:119-122 → visually an empty strip)
├ if path starts with /settings → SettingsSidebarNav (§3.2), else:
├ SidebarContent       ScrollArea flex-1 min-h-0, hidden scrollbars, 24px edge fade mask where content overflows;
│                      inner flex col, gap 0                                                               :2897-3061
│ ├ Search group       px 8, pt 8, pb 4                                                                   §2.3
│ ├ [Arm64 alert]      px 8, pt 8, pb 0                                                                   §2.4
│ ├ [LocalSecondaryStatus] px 8, pt 8, pb 0                                                               §2.4
│ └ Projects group     p 8                                                                                §2.5-2.12
├ SidebarSeparator     invisible 1px (§0), mx 8
└ SidebarChromeFooter  p 8, flex col gap 8                                                                §2.14
```

### 2.3 Search row

`Sidebar.tsx:2899-2921`.

- `SidebarMenuButton` size sm: h 28, `rounded-lg`, px 8, gap 8, `text-xs`, color `muted-foreground/70`, hover bg `accent` + text `foreground`, no focus ring.
- Contents: `Search` 14px `muted-foreground/70`, "Search" (flex-1, truncate), and when bound a `Kbd` with the `commandPalette.toggle` label ("⌘K" mac, "Ctrl+K" otherwise). The Kbd is h 16, px 6, `rounded-sm`, bg `muted`, `text-[10px]` medium `muted-foreground`.
- Click toggles the command palette.

### 2.4 Alerts

Arm64 Intel-build warning (Electron, host arm64 + app x64; `Sidebar.tsx:2922-2944`): warning `Alert` (`rounded-2xl`, border `warning/40`, bg `warning/8`, `TriangleAlert` icon). Title "Intel build on Apple Silicon", description from `desktopUpdate.logic.ts:40-53`. Optional action, xs outline: "Download ARM build" or "Install ARM build".

`LocalSecondaryStatus` (`Sidebar.tsx:2448-2521`, desktop-local secondary backends such as WSL):

- Connecting: default `Alert` (`rounded-2xl`, border `border/40`, bg `accent/40`) with a spinning `Loader` and "Connecting <label, label>".
- Failed: warning `Alert` "Couldn't connect <labels>" / errors joined with "; ", or "The backend didn't respond.".

### 2.5 Projects header row and sort menu

Header row (`Sidebar.tsx:2947-2979`): mb 4, flex justify-between, pl 8, pr 6.

- Left: "Projects", `text-[10px]` medium, uppercase, tracking-wider, `muted-foreground/60`.
- Right (gap 4): two icon buttons, each h 24, min-w 24, `rounded-md`, px 3, `muted-foreground/60`, hover bg `accent` + `foreground`. Icons 14px: `ArrowUpDown` (tooltip right "Sidebar options") and `FolderPlus` (tooltip right "Add project", opens the palette's add-project flow, §4.5).

`ProjectSortMenu` (`Sidebar.tsx:2528-2675`): popup aligned end, below the trigger, min-w 208, `rounded-lg` border bg `popover` `shadow-lg/5`, inner p 4. Section labels are px 8, py 4 (later ones pt 8, pb 4), `text-xs` medium, `muted-foreground`. Radio items: min-h 28, py 4, `text-xs`, grid `[16px 1fr]` gap 8, `rounded-sm`, highlighted bg `accent`.

| Section | Options (label → value) | Writes client setting |
| --- | --- | --- |
| "Sort projects" | "Last user message" `updated_at`, "Created at" `created_at`, "Manual" `manual` | `sidebarProjectSortOrder` (default `updated_at`) |
| "Sort threads" | "Last user message" `updated_at`, "Created at" `created_at` | `sidebarThreadSortOrder` (default `updated_at`) |
| "Visible threads" | NumberField 1-15, step 1. Width 112; group h 26 `rounded-md`; input width 36, `text-xs`; `−`/`+` buttons px 8 with 14px icons. Input keydown stops propagation. | `sidebarThreadPreviewCount` (default 6) |
| separator (mx 8, my 4, h 1, `border`) | | |
| "Group projects" | "Group by repository" `repository`, "Group by repository path" `repository_path`, "Keep separate" `separate` | `sidebarProjectGroupingMode` (default `repository`) |

Empty state (`Sidebar.tsx:3054-3058`): "No projects yet", px 8, pt 16, centered, `text-xs`, `muted-foreground/60`.

### 2.6 Data sources

- Projects: every project in every connected environment (`useProjects`). Threads: every thread shell in every environment (`useThreadShells`). The thread shell type is `OrchestrationThreadShell` + `environmentId`.
- Scoped keys: thread and project keys are `${environmentId}:${id}` (`pkg/client-runtime/src/environment/scoped.ts:25-27`). The physical project key is `${environmentId}:${normalizedPath}`, where the normalized path is trimmed with trailing separators removed, and Windows drive or UNC paths are lowercased with `\` separators (`pkg/client-runtime/src/state/projects.ts:142-148`).

### 2.7 Project grouping and ordering

Grouping (`pkg/client-runtime/src/state/projectGrouping.ts`, `web/sidebarProjectGrouping.ts:48-133`):

- Mode per project = `sidebarProjectGroupingOverrides[physicalKey] ?? sidebarProjectGroupingMode`.
- Logical key (`projectGrouping.ts:118-137`):
  - `separate` → physical key.
  - `repository` → `repositoryIdentity.canonicalKey`, else physical key.
  - `repository_path` → `canonicalKey` + `"::" + repoRelativePath` (an empty relative path gives `canonicalKey`), else physical key.
- One snapshot per logical key, in first-seen order of the ordered projects.
- Representative = the member in the primary env, else the first member.
- `displayName` for groups of more than one member: the shared `repositoryIdentity.displayName`, else the shared `repositoryIdentity.name`, else the representative title (`projectGrouping.ts:164-183`).
- `environmentPresence`: `local-only`, `remote-only`, or `mixed`, relative to the primary env. `allRemoteMembersAreDesktopLocal` = every non-primary member is in a desktop-local (WSL) env.
- Upstream still has `repositoryIdentity` on projects (`up:packages/contracts/src/orchestration.ts:871`).

Ordering:

1. `orderedProjects = orderItemsByPreferredIds(projects, uiState.projectOrder)` (`Sidebar.logic.ts:251-288`). `projectOrder` holds physical keys or legacy `legacy-project-cwd:<path>` keys. Projects matching an entry come first in list order. The rest follow in store order.
2. `sortProjectsForSidebar` (`Sidebar.logic.ts:555-590`):
   - `manual`: keep `orderedProjects` order.
   - `updated_at` / `created_at`: a project's timestamp is the max sort timestamp over its logical group's non-archived threads. With no threads, `created_at` uses `project.createdAt` and `updated_at` uses `project.updatedAt ?? createdAt`. Sort descending, then title `localeCompare`, then id.
3. Thread sort (`pkg/client-runtime/src/state/threadSort.ts:57-86`):
   - `updated_at`: timestamp = `latestUserMessageAt`, else the newest user message, else `updatedAt`, else `createdAt`.
   - `created_at`: `createdAt`, else `updatedAt`.
   - Descending, ties by id descending. Archived threads (`archivedAt != null`) are excluded everywhere in the sidebar.

### 2.8 Project row

Wrapper: `li` (`rounded-md`) in a `ul` (flex col, gap 4). Header group (`Sidebar.tsx:2174-2280`):

```
div.group/project-header  relative
├ button (SidebarMenuButton sm)  h 28, rounded-lg, px 8, pr 32, gap 8, text-left
│   hover (anywhere on the header group): bg accent; cursor pointer (manual sort: grab / grabbing)
│ ├ leading 14px slot (ml -2):
│ │   expanded, or no status → ChevronRight 14px muted-foreground/70, rotated 90° when expanded (150ms)
│ │   collapsed + project status → 9px status dot (dotClass, pulses if Working);
│ │                                 on header hover crossfades (150ms) to the chevron; tooltip top = status label
│ ├ ProjectFavicon 14px
│ └ span flex-1 min-w-0 gap 8:
│     name: text-xs medium foreground/90 truncate
│     [groupedCount > 1] "<n> projects": text-[10px] muted-foreground/60, shrink-0
├ [remote-only] env badge: absolute top 4, right 6, 20×20, rounded-md, muted-foreground/60, icon 12px;
│     fades out (150ms) on header hover/focus-within
│     all remote members desktop-local → Container icon, tooltip "Local sandbox: <labels>", aria "Local sandbox project"
│     otherwise → Cloud icon, tooltip "Remote environment: <labels>", aria "Remote project"
└ new-thread button: absolute right 2, vertically centered (+1px), opacity 0 → 1 on header hover/focus-within (150ms)
      button h 24 min-w 24 rounded-md px 3 muted-foreground/60, hover foreground; SquarePen 14px
      aria "Create new thread in <displayName>"; tooltip top "New thread (<label>)" or "New thread"
      label = shortcut for `chat.newLocal`, else `chat.new` (default "⇧⌘N")
```

Project status (collapsed only): `resolveProjectStatusIndicator` over the statuses of all visible threads (`Sidebar.logic.ts:445-461`). Priority: Pending Approval 5, Error 5, Awaiting Input 4, Working 3, Plan Ready 2, Completed 1. Replacement requires strictly higher priority, so on ties the first thread in sort order wins.

`ProjectFavicon` (`web/components/ProjectFavicon.tsx`):

- `assets.createUrl` RPC with `{resource: {_tag: "project-favicon", cwd}}` returns a relative URL, resolved against the env's `httpBaseUrl`.
- Until the image loads (and on error, or with no URL) show `Folder` 14px `muted-foreground/50`. The loaded image is 14px, `rounded-sm`, object-contain.
- Loaded URLs are remembered process-wide so a remount skips the fallback flash. URL cache: stale after 5min, refresh every 30min, idle TTL 60min (`pkg/client-runtime/src/state/assets.ts:8-10`).

Expand and collapse (`web/uiStateStore.ts:337-368`, `Sidebar.tsx:271-277`):

- Preference keys = `[logicalKey, ...member physical keys, ...member legacy-cwd keys]`. The first defined value wins. Default true (false only if the legacy default key is false).
- Toggling writes the new boolean to every preference key. Persisted.

Click semantics (`Sidebar.tsx:1330-1393`):

- Click: if a thread selection exists, clear it; then toggle expansion.
- Enter or Space also toggles.
- Ignored: the click right after a context-menu pointerdown (right button, or ctrl+left on mac), during a drag, and right after a drag.

### 2.9 Project context menu and dialogs

Native menu (`Sidebar.tsx:1563-1669`). Items: "Rename", "Group into...", "Copy Path", "Remove" (destructive, trash icon). For grouped projects (more than one member) each item becomes a submenu of members. Member label (`Sidebar.tsx:258-269`): with one member, the member title; otherwise `"<envLabel> — <workspaceRoot>"`, or the workspace root when there is no env label.

| Action | Behavior |
| --- | --- |
| Rename | Opens the rename dialog (below). |
| Group into... | Opens the grouping dialog (below). |
| Copy Path | Copies `workspaceRoot`. Success toast "Path copied" / path. Error: stacked "Failed to copy path" / message. |
| Remove, member has threads | Stacked warning toast "Project is not empty" / "Delete all threads in this project before removing it." / destructive action "Delete anyway". The action closes the toast, waits 180ms, then shows a native confirm: `Remove project "<title>" and delete its <n> thread[s]?` / `Path: <root>` / [`Environment: <label>`] / `This permanently clears conversation history for those threads.` / `This removes only this project entry.` / `This action cannot be undone.` (lines joined with `\n`; the short form below if the count is now 0). Yes → `project.delete {projectId, force: true}`. |
| Remove, no threads | Native confirm `Remove project "<title>"?` / `Path: <root>` / [`Environment: <label>`] / `This removes only this project entry.` → `project.delete`. |

After a successful delete, clear the project's draft thread. Failure: stacked error `Failed to remove "<title>"` / message, or "Unknown error removing project.". Thread counts come from the thread shells loaded for that member.

Rename dialog (`Sidebar.tsx:2319-2364`):

- Popup max-w 512 (base dialog: `rounded-2xl` border bg `popover` `shadow-lg/5`; header p 24, gap 8; title `text-xl` semibold; description `text-sm` muted; panel p 24 with pt 4; footer border-top bg `muted/72`, px 24, py 16, buttons right with gap 8).
- Title "Rename project". Description "Update the title for <workspaceRoot>.".
- Field label "Project title" (`text-xs` medium) and an Input. Enter submits. Optional line `Environment: <label>` (`text-xs` muted).
- Footer: outline "Cancel", default "Save".
- Submit: trimmed empty → warning toast "Project title cannot be empty" (dialog stays open). Unchanged → close. Otherwise `project.meta.update {projectId, title}`. Close on success; failure → stacked error "Failed to rename project".

Grouping dialog (`Sidebar.tsx:2366-2435`):

- Title "Project grouping". Description "Choose how <workspaceRoot> should be grouped in the sidebar.".
- Label "Grouping rule". Full-width Select, popup aligned end. The trigger shows "Use global default (<global mode label>)" when inheriting. Options: "Use global default", "Group by repository", "Group by repository path", "Keep separate".
- Help text (`text-xs` muted) for the effective mode:
  - repository: "Projects from the same repository share one sidebar row."
  - repository_path: "Projects group only when both the repository and repo-relative path match."
  - separate: "Every project path gets its own sidebar row."
- Save writes client `sidebarProjectGroupingOverrides[physicalKey]`, deleting the entry for "inherit".

### 2.10 New thread from the sidebar

`Sidebar.tsx:1811-1936`, `Sidebar.logic.ts:202-249`.

- One member: create directly. Several members: native menu of members (label rule from §2.9), then create in the chosen member. Picker failure → stacked "Could not choose environment".
- Seed context:
  - The member env's `defaultThreadEnvMode` is `worktree` → `{envMode: worktree}`.
  - Else the active draft belongs to the same project → copy its branch, worktreePath, envMode, startFromOrigin.
  - Else the active thread belongs to the same project → its branch and worktreePath; envMode `worktree` if it has a worktreePath, else `local`.
  - Else the env default.
- Calls `handleNewThread` (§1.4). Failure → stacked "Could not create thread".

### 2.11 Thread list per project

`Sidebar.tsx:910-1036, 1222-1328`.

```
ul  margin-left 4, width 100% (overhangs 4px right into the group's padding), px 6, py 0, flex col gap 2,
    overflow hidden, border-left 1px `border` (§0)
├ [expanded, 0 threads] "No threads yet": h 24, px 8, text-[10px], muted-foreground/60
├ thread rows (§2.12)
├ [expanded, overflow, not expanded-list] "Show more" row
└ [expanded, overflow, expanded-list] "Show less" row
```

- Expanded project: render the first `sidebarThreadPreviewCount` threads, or all of them after "Show more".
- Collapsed project whose threads include the route's thread: render only that thread (pinned). No more/less row.
- "Show more" / "Show less" rows: `SidebarMenuSubButton` sm, h 24, px 8, `text-[10px]`, `muted-foreground/60`, hover bg `accent` + `muted-foreground/80`. "Show more" is prefixed (gap 8) with a compact status dot (9px dot in a 14px box) for the highest-priority hidden status, if any.
- The "Show more" expansion is per project, in memory only (`expandedThreadListsByProject`, reset on reload).

### 2.12 Thread row

`Sidebar.tsx:358-860`. Geometry: the row starts 19px from the sidebar edge (8 group padding + 4 margin + 1 rule + 6 padding). Its content starts at 27px. The project name starts at 58px.

```
li.group/menu-sub-item  relative, w-full
└ div[role=button tabindex=0]  h 28, px 8, gap 8, rounded-lg, text-xs, select-none, cursor pointer,
  │                            relative isolate; focus-visible: 1px inset ring (`ring`)
  ├ left  flex-1 min-w-0 gap 6
  │  ├ [PR] button: GitPullRequest 12px (PR color), rounded-sm; tooltip top = PR tooltip; click opens URL
  │  ├ [status] dot 6×6 rounded-full (dotClass, pulses if Working) + label text-[10px] (colorClass), gap 4;
  │  │          tooltip top = label
  │  └ title: text-xs, flex-1, truncate; tooltip top (max-w 320, wraps) = full title
  │           OR rename input (§2.12.4)
  └ right  ml-auto shrink-0 gap 6
     ├ [ports] Globe2 12px emerald-600 (dark emerald-400); tooltip "Open localhost:<port>[ (+N)]"
     ├ [worktree] FolderGit2 12px muted-foreground/40; tooltip "Worktree: <basename>[ (<branch>)]"
     ├ [terminal] Terminal 12px teal-600 (dark teal-300/90), pulsing; tooltip "Terminal process running"
     └ meta box  min-w 48, justify-end
        ├ archive control (absolute, see §2.12.3)
        └ meta: relative time OR jump-hint pill
```

#### 2.12.1 States and colors (`Sidebar.logic.ts:344-373`)

| State | Background | Hover background | Text |
| --- | --- | --- | --- |
| default | none | `accent` | `muted-foreground` → hover `foreground` |
| active (route thread) | `accent/85` (dark `accent/55`) | `accent` (dark `accent/70`) | `foreground`, medium |
| selected | `primary/15` (dark `primary/22`) | `primary/19` (dark `primary/28`) | `foreground` |
| selected + active | `primary/22` (dark `primary/30`) | `primary/26` (dark `primary/36`) | `foreground`, medium |

Relative time: `text-[10px]`, tabular-nums. Color `foreground/72` (dark `foreground/82`) when active or selected, else `muted-foreground/40`.

Relative time format (`web/timestampFormat.ts:90-106`): value = `latestUserMessageAt ?? updatedAt ?? createdAt`. Under 60s (or a future time) → "just now", then "<m>m ago", "<h>h ago", "<d>d ago". There are no weeks or months, so "45d ago" is possible. Labels do not tick: they only recompute when the row re-renders.

#### 2.12.2 Status pill (`Sidebar.logic.ts:375-443`, `pkg/shared/src/threadStatus.ts:83-102`)

Status kind, first match wins:

1. `hasPendingApprovals` → "Pending Approval" (amber)
2. `hasPendingUserInput` → "Awaiting Input" (indigo)
3. `session.status == error` or `latestTurn.state == error` → "Error" (rose)
4. `session.status == running` → "Working" (sky, pulse)
5. `session.status == starting` → "Working" (the connecting kind reuses the Working pill)
6. `interactionMode == plan`, latest turn settled (has `startedAt` and `completedAt`, session not running), and `hasActionableProposedPlan` → "Plan Ready" (violet)
7. Unseen completion → "Completed" (emerald)
8. else none

Unseen completion (`threadStatus.ts:45-64`): `latestTurn.completedAt > max(server completionAcknowledgedAt, local lastVisitedAt)`, or forced unread when `completedAt - lastVisitedAt == 1ms`.

Optimistic override: if the user just sent a message (`useOptimisticThreadWorkStartedAt`) and the status is not Pending Approval, Awaiting Input, or Error → Working.

Colors: `tokens.json` `status`. Class pairs:

| Pill | Text class | Dot class |
| --- | --- | --- |
| Working | `sky-600` / dark `sky-300/80` | `sky-500` / dark `sky-300/80` |
| Pending Approval | `amber-600` / dark `amber-300/90` | `amber-500` / dark `amber-300/90` |
| Awaiting Input | `indigo-600` / dark `indigo-300/90` | `indigo-500` / dark `indigo-300/90` |
| Error | `rose-600` / dark `rose-300/90` | `rose-500` / dark `rose-300/90` |
| Plan Ready | `violet-600` / dark `violet-300/90` | `violet-500` / dark `violet-300/90` |
| Completed | `emerald-600` / dark `emerald-300/90` | `emerald-500` / dark `emerald-300/90` |

Visiting a thread whose completion is visible calls `markThreadVisited(threadKey, completedAt)` and (fork only) dispatches `thread.completion.acknowledge` (`web/components/ChatView.tsx:1740-1779`). Upstream has neither `completionAcknowledgedAt` nor that command (verified by grep of `up:packages/contracts/src`). Native: rely on local `threadLastVisitedAtById` only, and never dispatch the acknowledge command.

"Mark unread" sets `lastVisitedAt = completedAt - 1ms` (`web/uiStateStore.ts:252-275`). It is a no-op when the thread has no completed turn.

#### 2.12.3 Badges and actions

- PR (`web/components/ThreadStatusIndicators.tsx:35-82`): only when `thread.branch != null`.
  - Data: `subscribeVcsStatus` for `cwd = worktreePath ?? thread's project workspaceRoot`. Keep one subscription per (env, cwd). Show only when `status.refName == thread.branch` and `status.pr != null`.
  - Short name: "PR" (GitHub/Bitbucket/Azure DevOps), "MR" (GitLab), "change request" (unknown) (`pkg/shared/src/sourceControl.ts`).
  - Tooltip `#<n> <short> open: <title>` (or closed / merged). Colors: open emerald (as Completed), closed `zinc-500` / dark `zinc-400/80`, merged violet.
  - Click: prevent default and stop propagation, then open the URL externally (`web/lib/openPullRequestLink.ts:50-75`). No local API → error "Link opening is unavailable."; failure → stacked "Unable to open pull request link" / message.
  - Upstream `VcsStatusResult` still has `refName`, `pr`, `sourceControlProvider` (`up:packages/contracts/src/git.ts:213-250`).
- Ports: click stops propagation, navigates to the thread, then calls `preview.open` for the first port. Failure → stacked "Unable to open preview" / message or "The preview could not be opened.".
- Archive control (hidden while running; running = `session.status == running && activeTurnId != null`):
  - Hover or focus-within the row: the meta fades to opacity 0 (150ms) and the archive button fades in at absolute right 2, vertically centered. Button: h 24, min-w 24, `rounded-md`, px 3, `muted-foreground/60`, hover `foreground`, `Archive` 14px, aria "Archive <title>".
  - `confirmThreadArchive` off: tooltip top "Archive"; click archives.
  - `confirmThreadArchive` on: the first click swaps in a "Confirm" pill (absolute right 4, h 20, `rounded-md`, px 8, `text-[10px]` medium, bg `destructive/12`, hover `destructive/18`, text `destructive`, aria "Confirm archive <title>") and focuses it. A second click archives. Leaving the row or moving focus out cancels.
  - Archive failure → stacked "Failed to archive thread".
- Jump-hint pill (replaces the relative time): h 20, `rounded-full`, border `border/80`, bg `background/90`, px 6, mono `text-[10px]` medium tracking-tight `foreground`, `shadow-sm`. Text such as "⌘1". Tooltip shows the same label.

#### 2.12.4 Interaction

Click (`Sidebar.tsx:1688-1740`):

- mod-click (⌘ mac, Ctrl elsewhere): toggle selection; the anchor becomes the clicked thread if it was added.
- shift-click: select the range from the anchor within that project's ordered visible thread keys. If the anchor is not in that list, just add the thread.
- plain click: ignore when `event.detail > 1` (the second click of a double click). Otherwise clear any selection, set the anchor, navigate `/$env/$thread`.
- Enter or Space on a focused row navigates.

Selection store (`web/threadSelectionStore.ts`):

- In memory only.
- A global mousedown outside `[data-thread-item], [data-thread-selection-safe]` clears it.
- Escape clears it (`web/routes/_chat.tsx:57-61`, skipped while the palette is open).

Inline rename (`Sidebar.tsx:494-618, 1955-2010`):

- Starts on a double click of the row body (not on a nested button or link, no modifiers) or from the "Rename thread" menu item.
- The input replaces the title span: transparent, 1px border `ring`, radius 4, px 2, `text-xs`. Autofocus with all text selected. Clicks and double clicks inside it do not bubble.
- Enter commits, Escape cancels, blur commits.
- Commit: trimmed empty → warning toast "Thread title cannot be empty". Unchanged → end. Otherwise `thread.meta.update {threadId, title}`; failure → stacked "Failed to rename thread".

Context menu (native; `Sidebar.tsx:520-569, 1742-1809, 2088-2172`):

- Right-click a selected row while a selection exists → multi menu: "Mark unread (N)", "Delete (N)" (destructive).
  - Delete with `confirmThreadDelete` on: confirm `Delete N thread[s]?\nThis permanently clears conversation history for these threads.`. Threads delete one at a time; stop at the first failure with stacked "Failed to delete threads"; deleted keys leave the selection.
- Right-click an unselected row while a selection exists → clear the selection, then the single menu.
- Single menu: "Rename thread", "Mark unread", "Copy Path", "Copy Thread ID", "Delete" (destructive, trash).
  - Copy Path: `worktreePath ?? member root ?? project root`. None → stacked error "Path unavailable" / "This thread does not have a workspace path to copy.". Success → "Path copied" / path.
  - Copy Thread ID → success "Thread ID copied" / id. Error → stacked "Failed to copy thread ID".
  - Delete with `confirmThreadDelete` on: confirm `Delete thread "<title>"?\nThis permanently clears conversation history for this thread.`. Failure → stacked "Failed to delete thread".
- Any menu failure → stacked "Thread action failed" / message or "An error occurred.".

### 2.13 Keyboard navigation

`Sidebar.tsx:3363-3536`.

- Visible thread keys, in order: for each sorted project, an expanded project contributes its rendered threads (preview slice, or all after "Show more"); a collapsed project contributes only the pinned route thread.
- `thread.previous` / `thread.next` (default ⇧⌘[ / ⇧⌘]): move to the adjacent visible thread. With no route thread, previous goes to the last and next to the first. Stops at the ends. A route thread outside the visible list does nothing (`Sidebar.logic.ts:308-333`).
- `thread.jump.1..9` (⌘1..⌘9): the first 9 visible threads. Key repeat is ignored. Navigating clears the selection and sets the anchor.
- Jump hints: when the held modifiers exactly match a jump shortcut's modifiers (for example ⌘ alone), pills appear after 100ms and hide immediately on release (`Sidebar.logic.ts:85-163`, `web/keybindings.ts:293-309`). Modifier tracking uses capture-phase keydown/keyup and resets on window blur (`web/shortcutModifierState.ts`).

### 2.14 Footer

`Sidebar.tsx:2763-2791`. Order: provider update pill, desktop update pill, settings button.

- Settings button: `SidebarMenuButton` sm, h 28, px 8, gap 8, `muted-foreground/70`, hover bg `accent` + `foreground`. `Settings` 14px + "Settings" (`text-xs`). Navigates `/settings`.
- Desktop update pill (`web/components/sidebar/SidebarUpdatePill.tsx`, Electron only):
  - Shown while downloading, or when the action is download or install (`web/components/desktopUpdate.logic.ts:5-30`).
  - h 28, `rounded-lg`, bg `primary/15` (hover `primary/22`), `text-xs` medium `primary`. Disabled while downloading (opacity 60, `not-allowed` cursor).
  - Contents: `RotateCw` "Restart to update", or `Download` "Downloading (NN%)" / "Downloading…", or `Download` "Update available". Tooltip from `getDesktopUpdateButtonTooltip`.
  - For the download action, an X button (20px, `primary/60`, tooltip "Dismiss until next launch") hides it for the session.
  - Install asks `window.confirm("Install update <v> and restart T3 Code?\n\nAny running tasks will be interrupted. Make sure you're ready before continuing.")`.
  - Toasts: "Update downloaded" / "Restart the app from the update button to install it."; "Could not download update"; "Could not start update download"; "Could not install update".
- Provider update pill (`web/components/sidebar/SidebarProviderUpdatePill.tsx`, view from `ProviderUpdateLaunchNotification.logic.ts:408-533`):
  - h 28, `rounded-lg`, `text-xs` medium. Tones: loading `primary/15`, success `success/12`, warning `warning/12`, error `destructive/12` (hover +6%).
  - Title text:

    | Tone | Title |
    | --- | --- |
    | loading | "Updating <Provider>" / "Updating N providers" (spinner) |
    | error | "<Provider> v<x> update failed" / "N provider updates failed" (dismissible) |
    | warning | "<Provider> still needs an update" / "N providers still need updates" (dismissible) |
    | success | "<Provider> updated: v<x>" / "N providers updated" (auto-dismiss after 3000ms with a progress fill) |

  - Only terminal update states whose `finishedAt` is at or after the newest `checkedAt` seen at first render are shown.
  - Click → `/settings/providers`. Dismiss tooltip "Dismiss until provider status changes".
  - Exit: translateY 6px + fade over 180ms `cubic-bezier(0.22,1,0.36,1)`.

### 2.15 Drag and drop (manual sort only)

`Sidebar.tsx:2677-2712, 2981-3025, 3259-3307`.

- Active only when `sidebarProjectSortOrder == manual`. The project header button is the drag handle.
- Pointer sensor with a 6px activation distance. Movement is restricted to the vertical axis and to the scroll container. Collision: pointer-within, falling back to closest corners.
- Dragged item: opacity .8, z 20. Hovered target: 1px ring `primary/40`.
- Drop: `reorderProjects(currentOrderKeys, draggedMemberPhysicalKeys, targetMemberPhysicalKeys)` (`web/uiStateStore.ts:370-412`) moves all dragged member keys to the target's index (adjusted for removed keys before it). Persisted in `projectOrder`.

### 2.16 Animations

- Project list (non-manual) and every thread list: `@formkit/auto-animate` with 180ms ease-out for insert, remove, and move (`Sidebar.tsx:233-236, 3309-3325`).
- Chevron rotation 150ms. Hover crossfades 150ms.
- `animate-pulse` (2s; opacity 1 → .5 → 1) on the Working dot and the terminal icon.

---

## 3. Settings

### 3.1 Route layout

`web/routes/settings.tsx:35-110`. `SidebarInset` (isolate) → flex col:

- Electron header strip: h 52 (h 40 on Win/Linux), border-bottom, px 20, drag region, collapsed-sidebar inset.
  - "Settings": `text-xs` medium tracking-wide `muted-foreground/70`.
  - On `/settings/general` only, at the right: xs outline "Restore defaults" with `RotateCcw` 14px (mx 4). Disabled when nothing differs from defaults.
- Body: flex-1 min-h-0. It remounts after a restore.

Escape anywhere (unless already handled) navigates back (`settings.tsx:50-63`).

Restore defaults (`web/components/settings/SettingsPanels.tsx:381-505`):

- Changed labels, in order: Theme, Time format, Visible threads, Word wrap, Diff whitespace changes, Auto-open task panel, Assistant output, Automatic Git fetch interval, New thread mode, New worktrees start from origin, Add project base directory, Archive confirmation, Delete confirmation, Git writing model, Commit generation instructions, PR generation instructions. Provider update checks is not included.
- Native confirm `Restore default settings?\nThis will reset: <labels joined ", ">.`.
- Yes: theme → system and the listed settings → defaults. The two generation-instruction fields are written to every connected environment.

### 3.2 Settings sidebar nav

`web/components/settings/SettingsSidebarNav.tsx`. It replaces the projects content while the path starts with `/settings`. The header strip stays.

- Items group: px 8, py 12, `ul` gap 4. Each item: h 28, px 10, gap 10, `text-[13px]`, `rounded-lg`, no hover or active fill (§0).
  - Active: `foreground`, medium, 16px icon in `foreground`.
  - Inactive: `muted-foreground/70`, hover `foreground/80`, icon `muted-foreground/60`.
  - Click → `navigate(to, {replace: true})`.
- Items: "General" `Settings2`, "Keybindings" `Keyboard`, "Providers" `Bot`, "Source Control" `GitBranch`, "Connections" `Link2`, "Archive" `Archive`. Diagnostics has no nav item; it is reached from General > About.
- Footer: invisible separator, then p 8: [T3 Connect sign-in, only with cloud config], then a grid `[1fr auto]` gap 4 with the Back button (h 28, px 8, gap 8, `text-xs` muted, hover bg `accent` + `foreground`, `ArrowLeft` 16px, "Back") and [T3 Connect avatar].

### 3.3 Layout primitives

`web/components/settings/settingsLayout.tsx`.

| Primitive | Spec |
| --- | --- |
| `SettingsPageContainer` (`:122-136`) | Scroll container flex-1, p 32. Inner: centered, max-w 768 (Keybindings 1024), flex col, gap 32. |
| `SettingsSection` (`:18-46`) | Vertical gap 10. Header row px 4, flex justify-between. Title h2: `text-[11px]` semibold uppercase tracking .08em `foreground/50`, preceded (gap 8) by a 12×1 `border` line and an optional icon. Right action box h 20, min-w 20. Card: `rounded-2xl`, border, bg `card`, `shadow-sm/4`, inner top hairline (light: 0 1px black/4%; dark: 0 -1px white/6%). |
| `SettingsRow` (`:48-96`) | Border-top `border/60` (none on the first), px 20, py 14 (pt 14, pb 0 when it has children). Flex row, items-center, justify-between, gap 12. Left (flex-1, gap 4): title row min-h 20 gap 6 with h3 `text-[13px]` semibold tracking -.01em and a 20×20 reset slot; description `text-xs` `muted-foreground/80`; optional status `text-[11px]` muted. Right: control, gap 8. |
| `SettingResetButton` (`:98-120`) | 20px ghost, `rounded-sm`, `Undo2` 12px, muted → hover `foreground`. aria "Reset <label> to default". Tooltip top "Reset to default". |
| Select trigger widths | 160 (`sm:w-40`) or 176 (`sm:w-44`). DraftInput 288 (`sm:w-72`). |
| `DraftInput` / `DraftTextarea` | Buffer locally; commit on blur or Enter (`web/components/ui/draft-input.tsx`). |

### 3.4 General page

`SettingsPanels.tsx:507-1069`. Section "General". Store column: client = client settings (§7); server = primary env `server.updateSettings` patch, and "all envs" = patch sent to every catalog env.

| Title | Description | Control | Store / key | Default | Reset aria label |
| --- | --- | --- | --- | --- | --- |
| Theme | "Choose how T3 Code looks across the app." | Select (160): System / Light / Dark | localStorage `t3code:theme` | system | theme |
| Time format | "System default follows your browser or OS clock preference." | Select (160): "System default" / "12-hour" / "24-hour" | client `timestampFormat` (`locale`, `12-hour`, `24-hour`) | locale | time format |
| Word wrap | "Wrap long lines in code blocks, tables, diffs, and file previews by default." | Switch | client `wordWrap` | true | word wrapping |
| Hide whitespace changes | "Set whether the diff panel ignores whitespace-only edits by default." | Switch | client `diffIgnoreWhitespace` | true | diff whitespace changes |
| Assistant output | "Show smoothly paced live output while a response is in progress." | Switch | server `enableAssistantStreaming` (**not in upstream**, §9) | false | assistant output |
| Provider update checks | "Check installed provider CLIs for newer available versions." | Switch | server `enableProviderUpdateChecks` | true | provider update checks |
| Auto-open task panel | "Open the right-side plan and task panel automatically when steps appear." | Switch | client `autoOpenPlanSidebar` | false | auto-open task panel |
| New threads | "Pick the default workspace mode for newly created draft threads." | Select (176): "Local" / "New worktree" | server `defaultThreadEnvMode` | local | new threads (also resets start-from-origin) |
| Start from origin (only when mode = worktree; row bg `muted/20`, pl 36) | "Creates the worktree from the latest matching branch on origin instead of your local branch." | Switch | server `newWorktreesStartFromOrigin` | false | new worktrees start from origin |
| Add project starts in | `Leave empty to use "~/" when the Add Project browser opens.` | DraftInput (288), placeholder "~/" | server `addProjectBaseDirectory` | "" | add project base directory |
| Archive confirmation | "Require a second click on the inline archive action before a thread is archived." | Switch | client `confirmThreadArchive` | false | archive confirmation |
| Delete confirmation | "Ask before deleting a thread and its chat history." | Switch | client `confirmThreadDelete` | true | delete confirmation |
| Text generation model | "Configure the model used for generated commit messages, PR titles, and similar Git text." | `ProviderModelPicker` + `TraitsPicker` (outline triggers, gap 6, wraps) | server `textGenerationModelSelection` | `{instanceId: "codex", model: "gpt-5.4-mini"}` | text generation model |
| Commit generation instructions | "Additional guidance for generated commit subjects and bodies. Applied to every connected environment." | DraftTextarea below (mt 12, pb 16), maxLength 4000, placeholder "For example: use lowercase conventional commit prefixes and keep the body concise." | all envs `commitGenerationInstructions` (**not in upstream**) | "" | commit generation instructions |
| PR generation instructions | "Additional guidance for generated pull request titles and descriptions. Applied to every connected environment, including WSL on Windows." | DraftTextarea, placeholder "For example: include rollout notes and group testing by automated and manual checks." | all envs `changeRequestGenerationInstructions` (**not in upstream**) | "" | PR generation instructions |

The model picker components belong to the chat spec. Settings routing between stores: `web/hooks/useSettings.ts:146-163` (keys found in `ServerSettings` go to the server patch, everything else to client settings).

Section "About":

- Version row (Electron or hosted channel): title "Version" + `code` APP_VERSION (`text-[11px]` medium muted). Description "Update available." when the action is download or install, else "Current version of the application.".
  - xs button (default variant for install, else outline). Label: "Download", "Install", "Checking…", "Downloading…", "Up to Date", or "Check for Updates". Tooltip = update tooltip. Behavior in `SettingsPanels.tsx:210-272`.
  - Toasts: "Could not download update", "Could not install update", "Could not check for updates" / `result.state.message` or "Automatic updates are not available in this build.".
- Update track (desktop bridge present): "Stable follows full releases. Nightly follows the nightly desktop channel and can switch back to stable immediately." Select (160) "Stable" (`latest`) / "Nightly". Calls `desktopBridge.setUpdateChannel`; failure → "Could not change update track".
- Diagnostics: description from `formatDiagnosticsDescription` (`web/components/settings/SettingsPanels.logic.ts:29-56`), for example "Local trace file. Exporting OTEL to <base>/{traces,metrics}." or "Terminal logs only.". xs outline "View diagnostics" → `/settings/diagnostics`.

### 3.5 Keybindings page

`web/components/settings/KeybindingsSettings.tsx:1082-1335`, logic in `KeybindingsSettings.logic.ts`. Container max-w 1024. Section "Keybindings".

- Header action (gap 6):
  - Collapsed: "<n> binding[s]" (`text-[11px]` muted) + search button (20px ghost, `Search` 12px, tooltip "Search keybindings").
  - Open: search input (h 24, w 176, `rounded-md`, border `input`, pl 28 with a 12px icon, `text-[11px]`, placeholder "Search keybindings"). Escape clears and closes. Blur with an empty query closes.
  - "Add keybinding" (+). "Open keybindings.json" (`FileJson`, disabled without a config path). Open failure → "Unable to open keybindings file".
- ⌘F / Ctrl+F opens and focuses search, unless focus is in another input.
- Non-Electron banner (skip natively): "Some shortcuts may be claimed by the browser before T3 Code sees them. Use the desktop app for better keybinding support."

Table (horizontal scroll, min-w 680, columns `[minmax(190,1.1fr) minmax(220,.85fr) minmax(210,1fr) 60px]`):

- Header: border-bottom `border/70`, bg `muted/25`, px 16, py 8, `text-[11px]` semibold uppercase tracking .07em muted: "Command", "Keybinding", "When", "Status".
- Rows: px 16, py 6, `text-sm`, even rows bg `muted/15`, hover `accent/40`, dividers `border/60`.

| Cell | Spec |
| --- | --- |
| Command | `commandLabel(id)`: split on "." and title-case each segment ("Thread: Jump: 1"); `script.<id>.run` → "Run Script: <Id>". `text-[13px]` medium, truncate. Tooltip shows the raw id. |
| Keybinding | Not editing: button h 28, `rounded-md`, px 6, transparent border (hover border `border/70` + bg `background`) containing the pill (one `Kbd` per `+` part, min-w 24, px 6: `mod` → "⌘"/"Ctrl", `shift` → "⇧", `alt` → "⌥"/"Alt", `ctrl` → "⌃", one char → uppercase, else raw) and an "Edit" hint (`text-[10px]` uppercase tracking .08em, visible on hover). Click starts recording. Recording or dirty: Input h 28, w 176, mono `text-[12px]`, placeholder "Press shortcut" (recording) or "Unassigned", recording border `primary/70` bg `primary/5`. Dirty: xs "Save" ("Saving"), disabled while saving, with an empty key, or an invalid when-clause. |
| When | Popover trigger h 28, full width, `rounded-md`, border `input`, bg `background`, px 10, mono `text-[12px]`; text is the expression or "Always" (muted); `ChevronDown` 14px at 60%. Opens the when builder (below), aligned start, offset 6. |
| Status | Conflict warning (20px, `TriangleAlert` 14px `warning`). Tooltip `Conflicts with <A>.` or `Conflicts with A, B, C[, and more].` followed by " The most recent matching binding wins when both conditions can apply." Then an actions menu (28px ghost, `Ellipsis`): "Reset to default" (Custom rows that have a default) and "Remove" (destructive, non-Default rows). |

Rules:

- Recording (`KeybindingsSettings.logic.ts:288-340`): Tab passes through, Escape restores. The key must have at least one modifier. On mac, Meta → `mod` and Ctrl → `ctrl`; elsewhere Ctrl → `mod` and Meta → `meta`; then `alt`, `shift`, then the key token (`space`, `esc`, arrows, letters/digits, f1-f99, enter, tab, backspace, delete, home, end, pageup, pagedown).
- Row source: "Project" if the command starts with `script.`; "Default" if command + key + when exactly match the client-side defaults; else "Custom".
- Sort by command id, then key. Search matches the lowercased command id, key, when, or source.
- Conflict: same key string and (either when empty, or the two whens equal).
- Save → `server.upsertKeybinding {command, key, when?, replace: {command, key, when?}}`. Reset → upsert of the default key/when replacing the current one. Remove → `server.removeKeybinding {command, key, when?}`. Failure toasts: "Unable to save keybinding" / "The keybinding was not saved.", "Unable to remove keybinding" / "The keybinding was not removed.". Upstream has the same inputs (`up:packages/contracts/src/server.ts:665-683`).
- The new-binding row sits at the top: a Command select (max-w 240, sorted by label), the key input (always visible), "Save", the when popover, and a cancel X (tooltip "Cancel").
- Empty table: "No keybindings match your search." (py 48, centered, `text-sm` muted).

When builder (`KeybindingsSettings.tsx:586-729`), width `min(544, 100vw-32)`, vertical gap 12:

- Header: "When" (`text-sm` medium) + xs outline "+ Condition" and "+ Group".
- Expression input: h 28, mono `text-[12px]`, placeholder "Always". Parse error: red border, plus a `CircleX` line "Use variables with !, &&, ||, and parentheses.".
- Visual tree below, covered while invalid by "Fix the expression above to continue editing visually.".
- Condition row: "Not" toggle (h 28, min-w 40), variable Select (mono 12px), remove (−).
- Group card: and/or Select (w 96), "+ Condition", "+ Group", remove. Children are indented 16px with 1px connector lines.
- Known variables: `terminalFocus`, `terminalOpen`, `true`, `false`, plus identifiers found in the defaults (`previewFocus`, `modelPickerOpen`). Unknown identifiers show a warning icon with the tooltip "T3 Code does not recognize this condition yet. It can still be saved, but it may not match unless the runtime provides it."

### 3.6 Providers page

`SettingsPanels.tsx:1071-1514`. Section "Providers".

- Header action (gap 6): "Checked <value> ago" / "Checked just now" (`text-[11px]` `muted-foreground/60`, value mono tabular, ticks every 1s); "Add provider instance" (+, opens the dialog); "Refresh provider status" (`RefreshCw` 12px, spinning `Loader` while pending) → `server.refreshProviders {}`.

Row construction (`:1191-1264`):

- For each driver in `[codex, claudeAgent, cursor (only if the server reports instance "cursor"), grok, opencode]`:
  - The default instance row (`instanceId = defaultInstanceIdForDriver(driver)`). Its effective config is `providerInstances[defaultId] ?? {driver, enabled: legacy.enabled, config: legacy providers[driver]}`. It is dirty if an explicit instance exists or the legacy config differs from defaults; dirty rows get a reset button "Reset <Label> provider settings to default".
  - Then the custom instances of that driver.
- Then instances whose driver is not in the list.

Edit rules (`SettingsPanels.logic.ts:58-91`):

- Writing `providerInstances[id] = next` replaces the whole map. For default rows it also resets legacy `providers[driver]` to its default.
- Disabling the instance currently used for text generation also resets `textGenerationModelSelection` to its default.
- Delete (custom rows only) removes `providerInstances[id]`, `providerModelPreferences[id]`, and the favorites for that id.
- Inline update → `server.updateProvider {provider: driver, instanceId}`; failure → stacked "Could not update <Provider>".

`ProviderInstanceCard` (`web/components/settings/ProviderInstanceCard.tsx:378-806`):

```
row  border-top border/60 (none on first)
├ header  px 20, py 14, flex row items-center justify-between gap 12
│ ├ left  flex-1 min-w-0, gap 4
│ │ ├ title line  flex-wrap items-center gap 8
│ │ │   icon box 20px (provider logo 16px foreground/80 + status dot + accent badge; chat/ProviderInstanceIcon)
│ │ │   name h3: text-[13px] semibold tracking -.01em, truncate (displayName || driver label)
│ │ │   [id ≠ driver] code: rounded, bg muted/60, px 4, py 2, text-[10px] muted (instance id)
│ │ │   [cursor, grok] Badge warning sm "Early Access"
│ │ │   [version] code text-xs muted "v1.2.3"
│ │ │   [advisory] 20px ghost ArrowUpCircle 14px `primary`, bouncing 2.4s → popover (below)
│ │ │   [reset 20px]  [delete 20px: Trash2 12px, hover destructive, tooltip "Delete instance"]
│ │ └ auth line  text-xs muted-foreground/80, flex-wrap, gap-x 4
│ │     authenticated with email: "Authenticated as" <redacted email> ["· <auth label>"]
│ │     otherwise: <headline> ["· Email" <redacted email>]
│ │     then ["- <detail>"]
│ └ right  ghost sm h 28 px 8 ChevronDown 14px (rotates 180° when open), aria "Toggle <name> details"; Switch (enabled)
└ collapsible body (open state is per page view, in memory)
  ├ [px 20 py 12, border-top] "Display name" (text-xs medium) + DraftInput (mt 6, placeholder = driver label)
  │                           + "Optional label shown in the provider list." (mt 4, text-xs muted)
  ├ [px 20 py 12] accent color picker; description "Used to distinguish this instance in picker rails and model lists."; commit delay 120ms
  ├ [px 20 py 12] environment variables (below)
  ├ provider settings form fields, one [px 20 py 12] block each (below)
  └ models section [px 20 py 12] (below)
  (unknown driver instead: "This instance uses a driver (<driver>) that is not shipped with the current build. Configuration values are preserved but cannot be edited from this surface.")
```

- Status dot (`web/components/settings/providerStatus.ts:7-20`): disabled `amber-400`, error `destructive`, ready `success`, warning `warning`. Key = live status, else (enabled ? warning : disabled).
- Summary headline and detail (`providerStatus.ts:31-81`):

  | Condition | Headline | Default detail |
  | --- | --- | --- |
  | no live provider | "Checking provider status" | "Waiting for the server to report installation and authentication details." |
  | disabled | "Disabled" | "This provider is installed but disabled for new sessions in T3 Code." |
  | not installed | "Not found" | "CLI not detected on PATH." |
  | authenticated | "Authenticated · <label>" or "Authenticated" | none |
  | unauthenticated | "Not authenticated" | none |
  | status warning | "Needs attention" | "The provider is installed, but the server could not fully verify it." |
  | status error | "Unavailable" | "The provider failed its startup checks." |
  | otherwise | "Available" | "Installed and ready, but authentication could not be verified." |

  `provider.message` overrides the default detail.
- Redacted text (`web/components/settings/RedactedSensitiveText.tsx`): mono `text-[11px]`, blurred 2px, showing a deterministic scramble that keeps `@ . - _`. Click toggles reveal. Tooltips "Click to reveal email" / "Click to hide email".
- Advisory popover (`ProviderInstanceCard.tsx:610-704`, width `min(336, 100vw-24)`, aligned start):
  - "Update available" (`text-[13px]` semibold).
  - Detail (`text-xs`): `advisory.message`, or "Update available: install v<x>." / "Update available: install the latest provider version.".
  - [xs full-width "Update now" with `Download`, or "Updating" with a spinner].
  - [divider text "or, update manually using", `text-[10px]` uppercase tracking-wider].
  - [command box: border `border/70`, bg `muted/40`, mono `text-[11px]`, horizontal scroll with fade, copy button 24px with tooltip "Copy command"]. Copy toast "<Provider> update command copied" / "Run it in a terminal when you are ready to update."; error "Could not copy <Provider> update command".
- Environment variables (`ProviderInstanceCard.tsx:156-319`):
  - Header "Environment variables" + sm outline "+ Add" (h 28).
  - Empty: "Add variables to pass API keys, base URLs, or other per-instance CLI settings.".
  - Table (border `border/70`, `rounded-md`): header bg `muted/25` `text-[11px]` "Variable", "Value", "Sensitive" (w 80), options (w 48). Rows alternate `muted/20` / `background/20`. Name DraftInput placeholder "VARIABLE_NAME". Value DraftInput (password when sensitive) with placeholder "Value", or "Stored secret - enter a new value to replace" when the value is redacted. Sensitive checkbox. Remove X (32px, hover destructive).
  - Footer note: "Sensitive values are stored separately and are not returned to the app after saving."
  - Names must match `^[a-zA-Z_][a-zA-Z0-9_]*$`. While any row is invalid and non-empty, nothing is published.
- Settings form (`web/components/settings/ProviderSettingsForm.tsx:72-131, 183-275`): fields come from the driver schema annotations (title, description, placeholder, control, order); hidden fields are skipped. Card variant uses DraftInput (commit on blur or Enter). Empty strings are omitted from config.

  | Driver | Fields (fork) |
  | --- | --- |
  | codex | Binary path ("codex"), CODEX_HOME path ("~/.codex"), Shadow home path ("~/.codex-t3/personal") |
  | claudeAgent | Binary path ("claude"), Claude HOME path ("~"), Launch arguments ("e.g. --chrome") |
  | cursor | Binary path ("agent"), API endpoint ("https://...") |
  | grok | Binary path ("grok") |
  | opencode | Binary path ("opencode"), Server URL ("http://127.0.0.1:4096"), Server password (password, "Optional", "Stored in plain text on disk.") |

  Descriptions are in `pkg/contracts/src/settings.ts:158-356`. Upstream's driver fields differ (Claude: "CLAUDE_CONFIG_DIR path", "Auto-compact after"; Codex adds "Launch arguments"; new Antigravity driver; `up:packages/contracts/src/settings.ts:583-896`). See §9.
- Models section (`web/components/settings/ProviderModelsSection.tsx:87-411`):
  - "Models" (`text-xs` medium) and "<n> model[s] available." (mt 4).
  - List (mt 8, max-h 160, scrolls): rows min-h 28, py 4. Name `text-xs` `foreground/90` (hidden: muted + strikethrough). [Info 20px: tooltip with the slug and capability tags "Fast mode", "Thinking", "Reasoning"]. ["hidden"], ["custom"] (`text-[10px]` muted).
  - Row actions (20px each): star (yellow-500 filled when favorite; tooltips "Add to favorites" / "Remove from favorites"); move up / move down (disabled across the favorites boundary); eye toggle (server models; "Hide from picker" / "Show in picker"); remove X (custom; "Remove custom model").
  - Order: favorites first, then `modelOrder` (`web/modelOrdering.ts`).
  - Add row (mt 12, gap 8): Input placeholder by driver (codex "gpt-6.7-codex-ultra-preview", claudeAgent "claude-sonnet-5", cursor "claude-sonnet-4-6", opencode "openai/gpt-5", else "model-slug") + outline "+ Add". Enter adds.
  - Validation errors (`text-xs` destructive): "Enter a model slug.", "That model is already built in.", "Model slugs must be <N> characters or less.", "That custom model is already saved.".
  - Writes: custom models → `config.customModels`; hidden and order → client `providerModelPreferences[id]`; favorites → client `favorites`.
- Accent color picker (`web/components/settings/ProviderAccentColorPicker.tsx:225-335`):
  - "Accent color" label.
  - Custom swatch (24px circle in the current color with `Pipette` 12px at `foreground/25`) → popover: 224 wide, 144px saturation/value plane, 12px hue slider, hex input (h 32, mono `text-xs`, accepts only `#rrggbb`).
  - Six 24px swatches `#2563eb #16a34a #ea580c #dc2626 #7c3aed #0891b2`. Selected swatch ring: inset 2px `card` + outer 2px of the swatch color.
  - Clear X (28px, visible only when set).
  - Stored as normalized `#rrggbb`, or the key is omitted.

Add provider instance dialog (`web/components/settings/AddProviderInstanceDialog.tsx:116-469`), popup max-w 576:

- Header: title "Add provider instance". Description "Configure an additional provider instance — for example, a second Codex install pointed at a different workspace.".
- Stepper: 3 columns, gap 8. Each step tile: `rounded-lg` border, px 12, py 8, a 16px circle (check when done), "Step N" (`text-[10px]` uppercase), and a name ("Driver", "Identity", "Config", plus ": <summary>" once done). Current tile: border `primary`, bg `primary/10`, 1px ring `primary/25`. Tiles are clickable.
- Body (bg `muted/20`, px 24, py 20, animated height):
  1. Driver: 2-column radio grid (gap 10) of tiles (`rounded-lg`, px 12, py 12, 20px icon, `text-sm` medium). Selected: border `primary` + 2px ring `primary/35`. Five drivers plus disabled "Coming Soon" tiles: "Github Copilot", "Gemini", "ACP Registry", "Pi Agent".
  2. Identity:
     - "Label" (placeholder "e.g. Work", help "Shown in the provider list. Optional.").
     - "Instance ID" (placeholder "<driver>_work"; auto-derived as `<driver>_<slug(label)>` with slug = lowercase, non-alphanumerics → `_`, trimmed, 48 chars max; editable; help "Routing key used by threads and sessions. Letters, digits, '-', or '_'.").
     - "Accent color" (native color input + the six swatches + "Clear"; help "Optional marker shown in the picker.").
  3. Config: the dialog variant of the settings form, or "This driver has no required configuration. You can add the instance now."
- Footer: "Cancel" (step 1) or "Back"; "Next", or "Add instance" on step 3.
- ID validation errors (shown after the first submit attempt): "Instance ID is required.", "Instance ID must be 64 characters or fewer.", "Instance ID must start with a letter and use only letters, digits, '-', or '_'.", "An instance named '<id>' already exists.".
- Save writes `providerInstances[id] = {driver, enabled: true, displayName?, accentColor?, config?}`. Toast "Provider instance added" / "<Driver> instance '<id>' was added."; error "Could not add provider instance".

### 3.7 Source Control page

`web/components/settings/SourceControlSettings.tsx:442-518`. Query `server.discoverSourceControl {}` on the primary env.

- Initial pending: skeleton sections "Version Control" and "Source Control Providers", two skeleton rows each.
- Sections: "Version Control" (header action: rescan button, 20px `RefreshCw` 12px, spins while pending, tooltip "Rescan Git and hosting integrations") and "Source Control Providers" (it gets the rescan button when there is no VCS section).
- Row (`:212-291`): px 20, py 14, flex-wrap title line gap 8:
  - Icon 18px in a 20px box with a status dot: `muted-foreground/35` when not implemented, `warning` when unavailable or unauthenticated, `success` otherwise. Icons: GitHub, GitLab, AzureDevOps, Bitbucket, Git, Jujutsu.
  - Label (`text-[13px]` semibold), version (`code` `text-xs` muted), Badge warning "Coming Soon" (not implemented), Badge warning "Not authenticated".
  - Summary (`text-xs` `muted-foreground/80`): "Support for <label> is coming soon." / "Not available on this server: <installHint>" / "Authenticated" ["as" <redacted account>] / "Available. <installHint>" / "<label> is not authenticated on this server. Sign in or configure credentials using the `<executable>` tool on the server host to enable pull request features." / "Could not verify <label>. <installHint>" / "Available".
  - Right: chevron (git row only) + a disabled Switch reflecting availability.
- Git row details (`:293-359`):
  - "Fetch interval" + reset button. Help: "Refresh remote branch status in the background. Set this to 0 seconds if Git credentials or security keys should only be prompted by explicit Git actions."
  - NumberField (w 128, min 0, step 5) + "seconds". Writes server `automaticGitFetchInterval` (ms; default 30000).
- Empty or error (`:398-440`): section "Server environment", Empty (min-h 352) with a `GitPullRequest` media icon. Title "Nothing detected yet" or "Could not scan the server environment". Description "Install Git on the server, add optional hosting integrations or credentials your workspace needs, then rescan." or the error. sm outline "Scan" (h 32).

### 3.8 Connections page (summary)

`web/components/settings/ConnectionsSettings.tsx` (3371 lines; protocol in the connections spec).

- Sections: "This environment" (WSL backend rows, "Tailscale HTTPS", "Network access", "Version drift", "Administrative access"), "Authorized clients" (pairing links with QR, connected clients, dialog "Create pairing link"), "Remote environments" (saved backends, desktop SSH hosts, T3 Connect rows "T3 Connect" / "Publish agent activity", dialog "Add Environment").
- Dialogs: "Set up Tailscale HTTPS?", "Create pairing link", "Add Environment".
- Reuses `SettingsSection` / `SettingsRow` and `ITEM_ROW_CLASSNAME` (`web/components/settings/itemRows.ts`: px 20, py 16).

### 3.9 Archive page

`SettingsPanels.tsx:1516-1737`. Source: `orchestration.getArchivedShellSnapshot {}` per environment that has projects. Refreshed after unarchive or delete, and whenever any thread is archived or deleted.

- No archived threads: section "Archived threads", one row.
  - Title with a 14px icon: spinning `Loader` + "Loading archived threads" / "Could not load archived threads" / `Archive` + "No archived threads".
  - Description: "Checking connected environments." / the error / "Archived threads will appear here.".
- Otherwise one section per project that has archived threads: title = project title, icon = favicon. Threads sorted by `archivedAt ?? createdAt` descending, then id.
- Row: title = thread title. Description "Archived <rel> · Created <rel>". Control: sm outline h 28, px 10, gap 6, `ArchiveX` 14px + "Unarchive".
- Right-click → native menu "Unarchive", "Delete" (destructive). Delete with `confirmThreadDelete` on: confirm `Delete thread "<title>"?\nThis permanently clears conversation history for this thread.`.
- Toasts: "Failed to unarchive thread", "Failed to delete thread", "Archived thread action failed".

### 3.10 Diagnostics page

`web/components/settings/DiagnosticsSettings.tsx:807-1352`. Primary env queries: `server.getTraceDiagnostics {}`, `server.getProcessDiagnostics {}`, `server.getProcessResourceHistory {windowMs, bucketMs}`. All exist upstream.

Shared pieces:

- `StatsGrid`: 4 columns with 1px `border/60` dividers. `StatBlock`: px 20, py 12. Label `text-[11px]` medium uppercase tracking .08em `muted-foreground/70` with an optional info tooltip. Value: mono `text-lg` semibold tabular (warning tone `amber-600` / dark `amber-400`, danger `destructive`). "..." while loading.
- Header actions: "Checked <rel>" / "Checking" (ticks every 1s) and a refresh button.
- Tables: `text-xs`. Header `text-[11px]` uppercase tracking .08em `muted-foreground/70`. Cell px 16 (first cell pl 20), py 12 (process tables py 8). Rows hover `muted/20`. Horizontal scroll with fade.

Sections in order:

1. "Live Processes": stats "Child Processes", "CPU" (tooltip "Total CPU across live child processes of the current server process. The desktop shell and other parent processes are not included."), "Memory" (same wording for resident memory), "Server PID".
   - Table (min-w 1040, max-h `min(64vh, 704)`, sticky header): "Name" (collapsible tree indented 10px per depth up to 6, 6px emerald dot, name = executable basename, tooltip = full command), "CPU" (`x.x%`), "Memory" (B/KB/MB/GB), "Command", "PID", "Type" ("Subprocess" when depth > 0; "Agent" when the command matches codex, claude, opencode, or cursor; else "Process"), "Kill" ("INT" muted link, "KILL" destructive link).
   - SIGKILL asks `window.confirm("Send SIGKILL to process <pid>? This cannot be handled by the process.")`. Calls `server.signalProcess {pid, signal}`.
   - Toasts: error "Could not send <SIG>"; info "Process already exited" / "The process is not a child of the T3 Server. It might already have exited.".
   - Empty: "Loading live processes..." / "No live descendant processes found.".
2. "Resource History": window selector (bordered, segments h 24: "5m" (30s buckets), "15m" (60s, default), "30m" (2m), "1h" (5m)).
   - Stats "CPU Time" (tooltip "Approximate active CPU time for the T3 server root process and its descendants during the selected window. It grows only while sampled processes use CPU and older samples leave as the window moves."), "Samples" (tooltip "In-memory process samples retained by the server. This resets when the server restarts."), "Interval", "Processes".
   - Bar chart (h 112, bg `muted/10`, p 8, gap 4; per bucket a peak bar `foreground/15` and an average bar `foreground/60`; tooltip "Avg x%, peak y%").
   - Table "Process", "CPU Time", "Current", "Average", "Peak", "Max Mem", "Command", "PID". Root process dot `amber-500/90`.
   - Empty: "Collecting process resource samples..." / "No process resource samples found for this window.".
3. "Trace Diagnostics": "Open logs folder" button (opens `observability.logsDirectoryPath` in the preferred editor; errors inline "No available editors found." / "No environment is selected." / "Unable to open logs folder.").
   - Stats "Spans", "Failures" (danger when > 0), "Slow Spans" (warning; tooltip "Spans with a duration of <d> or longer."), "Parse Errors" (warning).
   - Partial failure line: "Some trace files could not be read, so diagnostics may be incomplete. <msg>".
4. "Latest Failures": "Span", "Cause" (expandable, clamps to 3 lines, "Show full error" / "Show less"), "Duration", "Ended". Empty: "Loading failures..." / "No failed spans found.".
5. "Most Common Failures": "Span", "Count", "Cause", "Last Seen". Empty: "No repeated failures found.".
6. "Slowest Spans": "Span", "Duration", "Ended", "Trace" (trace id shortened to first 18 + "..." + last 10, copy button with tooltip "Copy full trace ID" / "Copied"). Empty: "No spans found.".
7. "Span Logs": "Time", "Level" (`muted` chip, mono uppercase), "Span", "Message" (clamps to 2 lines, "Show full message"), "Trace". Empty: "No warnings or errors found.".
8. "Top Span Names": "Span", "Count", "Failures", "Average", "Max".

Formats: duration `< 1000` → "N ms", else "x.xx s" ("x.x s" at or above 10s). CPU time "Ns", "Nm", "Nh".

---

## 4. Command palette

Files: `web/components/CommandPalette.tsx`, `web/components/CommandPalette.logic.ts`, `web/components/CommandPaletteResults.tsx`, `web/commandPaletteContext.tsx`, `web/components/ui/command.tsx`, `web/components/ui/autocomplete.tsx`.

### 4.1 Triggers and state

- `commandPalette.toggle` (default ⌘K / Ctrl+K, when `!terminalFocus`), from a window keydown (`CommandPalette.tsx:393-411`). The sidebar "Search" row toggles it. The sidebar "Add project" button opens it directly in the add-project flow (`OpenAddProject` intent, `:352-369, 955-961`).
- Close: backdrop pointerdown, Esc, or running an item without `keepOpen`. On close, focus returns to the end of the composer.
- Each open starts at the root view with an empty query.
- While open (`[data-command-palette]` present), the `_chat` global shortcuts bail out (`web/commandPaletteContext.tsx:25-29`).

### 4.2 Layout

```
backdrop   fixed inset 0, z 50, bg background/60, backdrop blur 4px; fades 200ms
viewport   fixed inset 0, flex col items-center, px 16, py 10vh (pointer-events none)
popup      w 100%, max-w 576, max-h 420, flex col, rounded-2xl, border, bg popover, shadow-lg/5;
           ::before overlay bg muted/72 plus top hairline; enter/exit: scale .98 + opacity, 200ms ease-in-out
├ input wrapper   px 10, py 6
│   input row     h 34, text-sm, transparent border and bg; text starts at x=31
│   start addon   at x=12, 16px, opacity 80%: Search (root) | ArrowLeft button aria "Back" (submenu) | FolderPlus (browse)
│   [action btn]  absolute right 10, vertically centered, xs outline (h 24, rounded-md, ps 8, pe 4, gap 6): "<label>" + Kbd
├ panel           mx -1, border (no bottom border), rounded-t-xl, bg popover, shadow-xs/5, max-h min(448, 70vh), overflow hidden
│ ├ [clone confirm] p 8, pb 0: "Repository" (px 8, py 6, text-xs medium muted) + row (min-h 32, px 8, py 6, gap 8):
│ │                 icon, title (text-sm), url (text-xs muted-foreground/70)
│ └ list          p 8, scrollable
│     group label px 8, py 6, text-xs medium muted-foreground; groups after the first get mt 6
│     item        flex, min-h 28, items-center, gap 8, rounded-sm, px 8, py 6, text-sm;
│                 highlighted: bg accent, text accent-foreground
│       16px icon (muted-foreground/80) | [leading content] title (truncate, may contain bold spans)
│       [description below: text-xs muted-foreground/70 truncate] | [trailing content]
│       [timestamp: min-w 48, right-aligned, text-[10px] tabular muted-foreground/70]
│       [shortcut: ms-auto, text-xs medium sans tracking-widest muted-foreground/72] | [submenu: ChevronRight 16px muted/50]
│     disabled item: same layout, opacity .64, not interactive
│     empty       py 40, centered, text-sm muted
└ footer          flex justify-between gap 12, border-top, px 20, py 12, text-xs muted
    left (gap 12): Kbd pairs, gap 6, labels muted-foreground/80:
      [↑][↓] "Navigate"; "Enter" "Select" (or "Enter" "<Continue|Lookup>" during clone input);
      "Backspace" "Back" (submenu); "Esc" "Close"
    right: ghost xs "Open in Finder" / "Open in Explorer" / "Open in Files" (browse mode only, desktop, routable env)
```

`Kbd`: h 20, min-w 20, `rounded`, bg `muted`, px 4, `text-xs` medium `muted-foreground`, icons 12px.

### 4.3 Root view

Groups (`CommandPalette.tsx:963-1059`, `CommandPalette.logic.ts:334-350`).

"Actions":

1. [when an active project exists] "New thread in **<project>**" (`SquarePen`, shortcut `chat.new`) → `startNewThreadFromContext`.
2. [when projects exist] submenu "New thread in..." → group "Projects": one item per project (favicon icon, title, workspaceRoot as description, `chat.new` shortcut label on every row) → new thread in that project, carrying the active context.
3. "Add project" (`FolderPlus`, keepOpen) → add-project flow (§4.5).
4. [when a WSL desktop-local env exists] "Open WSL folder" / env label (keepOpen) → browse in that env.
5. "Open settings" (`Settings`) → `/settings`.

"Recent Threads": the 12 most recent non-archived threads across all envs, sorted by `sidebarThreadSortOrder` (`CommandPalette.logic.ts:122-180`).

- `MessageSquare` icon. Leading content: PR icon + status label (`ThreadRowLeadingStatus`). Title. Trailing content: terminal icon.
- Description joined with " · ": project title, `#<branch>`, "Current thread".
- Timestamp = relative time (§2.12.1).

### 4.4 Search

`CommandPalette.logic.ts:182-280`. The query is deferred (React `useDeferredValue`).

- A leading `>` limits results to "Actions" (empty text shows all actions).
- Normalize: trim, lowercase, collapse whitespace.
- Root view with a non-empty query: drop "Recent Threads", append "Projects" (every project: title + workspaceRoot; run opens the project's latest thread, else a new draft) and "Threads" (all non-archived threads, same item shape as Recent Threads).
- Match: the normalized query is a substring of the item's `searchTerms` joined with spaces.
- Rank: for the first search term that contains the query, score `1000 - termIndex*100 + (3 exact | 2 prefix | 1 contains)`. Sort by rank descending, then original order. Empty groups are dropped.
- Empty text: "No matching actions." (actions-only mode) or "No matching commands, projects, or threads.".

Placeholders (`CommandPalette.logic.ts:352-363`):

| Mode | Placeholder |
| --- | --- |
| root | "Search commands, projects, and threads..." |
| root browse | "Enter project path (e.g. ~/projects/my-app)" |
| submenu | "Search..." |
| submenu browse | "Enter path (e.g. ~/projects/my-app)" |
| clone input | "Enter Git clone URL" / "Enter GitHub repository (owner/repo)" (GitLab "group/project", Bitbucket "workspace/repository", Azure DevOps "project/repository") |

Keyboard (`CommandPalette.tsx:1493-1519`):

- Arrows move the highlight. It auto-highlights the first item, except in browse and clone modes.
- Enter runs the highlighted item.
- Backspace on an empty query in a submenu pops the view. Deleting the query to empty in a view opened with an initial query also pops it.
- Items prevent mousedown (the input keeps focus); click runs the item.
- Submenus push a view (back button + Backspace). Errors from `run` → stacked "Unable to run command".

### 4.5 Add project flow

1. More than one environment → view "Environments": label, description "This device" (primary) or the env id. Sorted primary first, then by label. Single env → skip to step 2. No env → stacked "Unable to browse projects" / "No environment is available.".
2. View "Sources":
   - "Local folder" / "Browse a folder on disk".
   - "Git URL" / "Clone from a remote URL".
   - "<Provider> repository" / "Clone <Provider> <path hint>", ready providers first, then by label.
   - Readiness comes from `server.discoverSourceControl` of that env. A not-ready provider is a disabled row with a "Setup Required" xs outline button (h 20, radius 4, px 6, `text-[10px]` `warning-foreground`). Tooltip = install hint, or "Provider status unavailable. Open Settings -> Source Control and rescan.", or "<Label> is not authenticated. Open Settings -> Source Control for setup guidance.". Clicking it closes the palette and opens `/settings/source-control`.
3. Browse mode:
   - Query starts as the env's `addProjectBaseDirectory` (with a trailing separator) or "~/". Browse mode is any query starting with `./`, `../`, `.\`, `..\`, `/`, `~/`, or a drive path on Windows envs (`pkg/client-runtime/src/state/projects.ts:94-105`).
   - Request `filesystem.browse {partialPath: dir, cwd?}`. `cwd` = the active project root when it is in the same env (needed for relative paths).
   - Group "Directories" (or "Select where to clone"). Entries are filtered by a case-insensitive prefix of the leaf; dot entries appear only when the leaf starts with ".". A ".." item (`CornerLeftUp`) navigates up. Picking a directory appends "<name>/" and stays open.
   - Submit: Enter with nothing highlighted, or ⌘/Ctrl+Enter with an entry highlighted. Button label "Add", or "Create & Add" when the path does not exist; Kbd "Enter" or "⌘ Enter".
   - Empty-state copy: "Press Enter to create this folder and add it as a project." / "Relative paths require an active project.".
4. Add (`CommandPalette.tsx:1080-1204`):
   - A Windows path on a non-Windows env → stacked "Failed to add project" / "Windows-style paths are only supported on Windows.".
   - A relative path without an active project → "Relative paths require an active project.".
   - An existing project at that path (same env) → open its latest thread, else a new draft (failure: "Failed to open project").
   - Else `project.create {projectId: new, title: basename, workspaceRoot, createWorkspaceRootIfMissing: true, defaultModelSelection: {instanceId: "codex", model: "gpt-5.4"}}`, then a new draft, then close. Failure → "Failed to add project".
5. Clone:
   - Repository step: Enter or the "Continue" (URL) / "Lookup" (provider) button ("Working" while pending). Provider sources call `sourceControl.lookupRepository {provider, repository}` and use `sshUrl` as the remote; failure → "Repository lookup failed". Empty-state copy: "Enter a Git clone URL and press Enter to continue." / "Enter a repository path and press Enter to look it up.".
   - Confirm step: destination browse with the Repository context block, "Choose a destination path and press Enter to clone.", button "Clone" / "Create & Clone" ("Cloning").
   - `sourceControl.cloneRepository {remoteUrl, destinationPath}` → add project at the result `cwd`. Failure → "Clone failed".
6. "Open in <file manager>": native folder picker with the current path as the initial path. A WSL UNC selection maps to the matching WSL env, else stacked "Could not add WSL project" / "Start the matching WSL backend, then choose the folder again.".

---

## 5. Toasts, dialogs, notifications

### 5.1 Toast system

`web/components/ui/toast.tsx`, `toast.logic.ts`, `toastHelpers.ts`. Base UI toast manager defaults: timeout 5000ms, limit 3 (verified in `@base-ui/react@1.5.0`).

- Viewport (`toast.tsx:560-578`): fixed, z 100, top-right. Width `100% - 64`, max 360. Inset 32 from the right; top = 32 + 52 = 84.
- Card: `rounded-lg`, border, bg `popover`, `shadow-lg/5`, inner top hairline.
- Content: pl 14, `text-sm`.
  - Inline layout: py 12, flex items-center justify-between, gap 6, pr 24 when there are trailing controls, else pr 40.
  - Stacked layout (`stackedThreadToast`): flex col, gap 8, py 10, pr 14; body pr 20; action row full width, right-aligned, gap 6.
- Icon 16px (20px line box), by type: error `CircleAlert` `destructive`, info `Info` `info`, success `CircleCheck` `success`, warning `TriangleAlert` `warning`, loading `LoaderCircle` spinning at 80%. A custom `leadingIcon` replaces it.
- Title medium. Description `muted-foreground`, selectable. Error descriptions of 180+ characters clamp to 4 lines.
- Error toasts with a string description get a copy button (20px, `Copy` 12px; tooltip "Copy error", then "Copied error" with a success check).
- Close orb: absolute top -6, right -6, 24px circle, border `border/60`, bg `popover/92`, backdrop blur, `shadow-sm`, X 12px (stroke 2.25), aria "Dismiss notification". Runs `data.onClose` and closes.
- Actions: primary action is an xs button (variant `actionVariant`, default `default`); secondary and additional actions are xs (default `outline`).
- Expandable section ("Show details" / "Hide details", or custom labels): panel max-h 160, scrolls. With `expandableDescriptionTrigger`, the description itself is the toggle (hover underline + chevron).
- Stack: gap 12, peek 12, scale `1 - 0.1*index`. Toasts behind the front one show only the peek until the stack expands on hover.
  - Enter and exit slide horizontally by width + inset. Transition: transform .5s `cubic-bezier(.22,1,.36,1)`, opacity .5s, height .15s.
  - Swipe right or up dismisses.
- Thread scoping: toasts with `data.threadRef` or `data.threadId` render only while that thread is the route thread (server or draft).
- `dismissAfterVisibleMs`: the timer runs only while the document is visible and focused, and resumes with the remaining time.
- `timeout: 0` = persistent.
- Anchored toasts (`toast.tsx:704-802`): positioned next to an anchor element, max-w 256. Used only by chat message copy buttons.

### 5.2 App-level notifications

| Source | When | Toast |
| --- | --- | --- |
| `SlowRpcRequestToastCoordinator` (`web/components/SlowRpcRequestToastCoordinator.tsx`) | Any RPC waiting > 15000ms for an ack (`web/rpc/requestLatencyState.ts:7`) | Persistent warning "Some requests are slow" / "<n> request[s] waiting longer than 15s." The description toggles a list ("Show requests" / "Hide requests") of RPC tag + "Started <locale time>". Updates in place and closes when the list empties. |
| `ProviderUpdatePrimaryNotification` (`web/components/ProviderUpdatePrimaryNotification.tsx`), single local env | Enabled providers with `versionAdvisory.status == behind_latest`, deduped by driver. Key = sorted `driver:latestVersion`. Shown once per session per key; never after a dismissal (persisted). | Persistent stacked warning. Title "Update Available: <Provider> v<x>" / "Updates Available: <n> providers". Description "Install the update now or review provider settings." (one-click possible) or "<A, B, and C> can be updated from provider settings.". Actions "Update" (default) + secondary "Settings" (outline), or just "Settings". Leading icon: provider logo with a download badge (single provider). Close records the dismissal. "Update" runs `server.updateProvider` per one-click candidate in turn and morphs the toast: loading "Updating provider[s]" / "Running provider update command." → success "Provider updated" / "Provider updates finished" + "New sessions will use the updated provider[s]." (auto-dismiss), or error "Provider update[s] failed", or warning "Provider still needs an update" / "Providers still need updates". |
| `ProviderUpdateEnvironmentsNotification` (`web/components/ProviderUpdateLaunchNotification.tsx:58-198`) | A desktop-local secondary (WSL) env is present | The same prompt split per environment (`ProviderUpdateEnvironmentRows`). Waits up to 30s for local backends to settle. Action "Settings". |
| `EventRouter` | keybindings file reloaded | §1.5 |

### 5.3 Dialogs

| Dialog | Where | Copy |
| --- | --- | --- |
| Rename project | §2.9 | |
| Project grouping | §2.9 | |
| Add provider instance | §3.6 | |
| Checkout PR (`web/components/PullRequestThreadDialog.tsx`, opened from ChatView) | max-w 576 | Title (provider icon 16px) "Checkout <pull request\|merge request>". Description "Resolve a <provider> <term>, then create the draft thread in the main repo or in a dedicated worktree.". Label = term (capitalized). Input placeholder "<PR\|MR> URL, checkout command, or #42", debounced 450ms. Resolved card (`rounded-xl`, border `border/70`, bg `muted/24`, p 12): title, "#<n> · <head> to <base>", state (colored). "Resolving <term>..." with a spinner. Errors: "Paste a <term> URL, checkout command, or enter 123 / #123.", "Use a <term> URL, checkout command, 123, or #123.", "Failed to prepare <term> thread.". Footer: "Cancel", outline "Local" ("Preparing local..."), default "Worktree" ("Preparing worktree..."). Enter = Local. Uses `git.resolvePullRequest` and `git.preparePullRequestThread`. |
| SSH password (`web/components/desktop/SshPasswordPromptDialog.tsx`) | desktop bridge `onSshPasswordPrompt`, FIFO queue | max-w 448, no close button. Title "SSH Password Required". Description "T3 needs your SSH password to connect to `<user@host>`. The password is passed to the local SSH process for this connection attempt and is not saved by T3 Code.". Prompt line (`text-sm` medium) + countdown "m:ss" (or "Expired" in destructive). Password input, autofocused. Help "Use SSH keys to avoid repeated password prompts on new SSH sessions." or the error "This SSH password prompt expired. Try connecting again.". Footer "Cancel" / "Dismiss" (when expired) + "Continue". |
| Cloud and connections dialogs | connections spec | "Set up T3 Connect", relay client install, "Create pairing link", "Set up Tailscale HTTPS?", "Add Environment" |

### 5.4 Native confirms

`LocalApi.dialogs.confirm` → Electron message box (`desk/electron/ElectronDialog.ts:140-170`): type question, buttons `["No", "Yes"]`, default and cancel = No, `noLink`, the whole text as the message, returns false on empty text.

Used by: project remove (§2.9), thread delete and multi delete (§2.12.4), the worktree delete prompt (§1.4), restore defaults (§3.1), archived-thread delete (§3.9).

`window.confirm` (OK/Cancel) is used instead for desktop update install (§2.14) and SIGKILL (§3.10).

GPUI: `Window::prompt` (gpui-pre `src/window.rs:6455`) with answers `["No", "Yes"]`. Verify that "No" maps to the default and cancel button.

### 5.5 Native context menus

`desktopBridge.showContextMenu(items, {x, y})` → Electron `Menu.popup` (`desk/electron/ElectronMenu.ts:131-163`). Item shape: `{id, label, destructive?, disabled?, children?}`.

- A separator is inserted before the first destructive item (unless it is first).
- On macOS, destructive leaf items get the 12px "trash" template icon.
- Returns the clicked id or null.
- Native: gpui-kit `NativeMenu` (`~/L-Projects/t3UI-refs/gpui-kit/crates/component/src/native_menu/mod.rs`): `menu`, `menu_with_icon_disabled`, `submenu`, `separator`, `show(position, window, cx)`. It uses OS menus on macOS and Windows and a drawn fallback on Linux.

---

## 6. Keybindings

### 6.1 Model

- The server owns the config. `server.getConfig` returns `keybindings: ResolvedKeybindingRule[]` (`{command, shortcut: {key, metaKey, ctrlKey, shiftKey, altKey, modKey}, whenAst?}`) and `keybindingsConfigPath`. Changes arrive through `subscribeServerConfig` (`keybindingsUpdated`) and from the upsert/remove results.
- Fork server merge (fork `apps/server/src/keybindings.ts:209-225`): defaults whose command appears in the custom config are dropped; the result is `[...retained defaults, ...custom]`, keeping the last 256. Upstream instead writes its defaults into the file and backfills missing commands on startup (`up:apps/server/src/keybindings.ts:450-530`). The client just consumes the resolved list either way.
- Matching (`web/keybindings.ts:201-217`): iterate the rules from last to first; return the first rule whose when-clause is true and whose shortcut matches.
  - Modifiers must match exactly. `mod` = Meta on mac, Ctrl elsewhere.
  - The key matches if it equals the lowercased `event.key` (`esc` → `escape`), or the letter from `event.code` (`KeyX` → `x`), or a code alias (`Digit0-9` → `0-9`, `BracketLeft` → `[`, `BracketRight` → `]`).
- When-clause: an AST over identifiers with `!`, `&&`, `||`, and parentheses. `true` / `false` are literals; any other identifier reads a context boolean (missing = false).
- Context sources:

  | Identifier | Meaning | Pointer |
  | --- | --- | --- |
  | `terminalFocus` | Focus is inside a terminal | `web/lib/terminalFocus.ts` |
  | `terminalOpen` | The route thread's terminal drawer is open | |
  | `previewFocus` | Focus is inside the preview | `web/lib/previewFocus.ts` |
  | `previewOpen` | The route thread's active right-panel surface is preview | |
  | `modelPickerOpen` | The composer model picker is open | |

- Labels (`web/keybindings.ts:219-266`):
  - mac: `⌃⌥⇧⌘` + key. Others: "Ctrl+Alt+Shift+Meta+Key".
  - Keys: space "Space", single char uppercased, `escape` "Esc", arrows "Up" / "Down" / "Left" / "Right", else capitalized.
  - `shortcutLabelForCommand` walks the rules from last to first and skips shortcuts already claimed by a later rule whose when-clause matches the label context. So a shadowed binding shows no label.
- Upstream adds many commands (`navigation.back/forward`, `rightPanel.close`, `filePicker.toggle`, `usage.*`, `thread.settle`, `thread.pin`, ...). Its `thread.jump.N` rules carry `when: "isDesktop"` and `modelPicker.jump.N` carry `"modelPickerOpen && isDesktop"`; other rules use `editableFocus` and `usagePageOpen` (`up:packages/shared/src/keybindings.ts:21-87`).
  - Native must provide `isDesktop = true` (or ⌘1-9 thread jumps never fire) and `editableFocus`.
  - Unknown commands must decode (upstream wraps configs in `ForwardCompatibleArray`), be ignored by the dispatcher, and still display in the editor through the generic `commandLabel`.

### 6.2 Fork default bindings

`pkg/shared/src/keybindings.ts:21-54`.

| Key | Command | When | Handler |
| --- | --- | --- | --- |
| mod+b | sidebar.toggle | | `AppSidebarLayout.tsx:22-35` |
| mod+j | terminal.toggle | | ChatView (chat spec) |
| mod+alt+b | rightPanel.toggle | | ChatView |
| mod+d | terminal.split | terminalFocus | ChatView |
| mod+shift+d | terminal.splitVertical | terminalFocus | ChatView |
| mod+n | terminal.new | terminalFocus | ChatView |
| mod+w | terminal.close | terminalFocus | ChatView |
| mod+d | diff.toggle | !terminalFocus | ChatView |
| mod+shift+j | preview.toggle | | `_chat.tsx:87-103` (the web-only "Preview is desktop-only" toast does not apply natively) |
| mod+r | preview.refresh | previewFocus | `_chat.tsx:108-128` |
| mod+l | preview.focusUrl | previewFocus | same |
| mod+= and mod++ | preview.zoomIn | previewFocus | same |
| mod+- | preview.zoomOut | previewFocus | same |
| mod+0 | preview.resetZoom | previewFocus | same |
| mod+k | commandPalette.toggle | !terminalFocus | `CommandPalette.tsx:393-411` |
| mod+n | chat.new | !terminalFocus | `_chat.tsx:75-85` |
| mod+shift+o | chat.new | !terminalFocus | same |
| mod+shift+n | chat.newLocal | !terminalFocus | `_chat.tsx:63-73` |
| mod+shift+m | modelPicker.toggle | !terminalFocus | ChatView |
| mod+o | editor.openFavorite | | `chat/OpenInPicker.tsx` |
| mod+shift+[ | thread.previous | | `Sidebar.tsx:3470-3536` |
| mod+shift+] | thread.next | | same |
| mod+1 … mod+9 | thread.jump.1 … 9 | | same |
| mod+1 … mod+9 | modelPicker.jump.1 … 9 | modelPickerOpen | `chat/ModelPickerContent.tsx` |

Non-configurable keys:

- Escape: settings back (§3.1), clear thread selection (`_chat.tsx:57-61`), close palette or dialogs.
- ⌘F / Ctrl+F on the Keybindings page.
- Terminal-local keys (`web/keybindings.ts:446-529`, chat spec).

The fork wires each handler as its own window keydown listener; a listener skips events that are already `defaultPrevented`. Native: one root key dispatcher that ports `resolveShortcutCommand` exactly, builds the context per keystroke, and routes the command to the owning view. Bail out for global chat commands while the palette is open, and ignore key repeat for thread navigation.

### 6.3 Application menu

`desk/window/DesktopApplicationMenu.ts:123-205`.

| Menu | Items (accelerator) |
| --- | --- |
| T3 Code (macOS only) | About (role); "Check for Updates..."; ─; "Settings..." (⌘,) → `open-settings`; ─; Services; ─; Hide (⌘H), Hide Others (⌥⌘H), Show All; ─; Quit (⌘Q) |
| File | non-mac: "Settings..." (Ctrl+,), ─; then Close Window (mac, ⌘W) or Quit (non-mac) |
| Edit | role editMenu: Undo ⌘Z, Redo ⇧⌘Z, ─, Cut, Copy, Paste, Paste and Match Style, Delete, Select All (mac adds Speech) |
| View | Reload (⌘R), Force Reload (⇧⌘R), Toggle Developer Tools (⌥⌘I), ─, Actual Size (⌘0), Zoom In (⌘=, hidden alias ⌘+), Zoom Out (⌘-), ─, Toggle Full Screen (⌃⌘F) |
| Window | role windowMenu: Minimize ⌘M, Zoom, ─, Bring All to Front |
| Help | "Check for Updates..." |

"Check for Updates..." message boxes:

- Up to date: "You're up to date!" / "T3 Code <v> is currently the newest version available."
- Check failed: "Update check failed" / "Could not check for updates." / detail = message or "An unknown error occurred. Please try again later."
- Updates disabled: "Updates unavailable" / "Automatic updates are not available right now." / detail = the reason.

GPUI: `cx.set_menus` with `gpui::Menu` / `MenuItem` (gpui-pre `src/platform/app_menu.rs`), OS actions for edit roles.

---

## 7. Client persistence

Everything below is client-side state the native app must persist (one JSON file per store, or a single state file). The keys are the web's localStorage keys.

| Key | Shape (defaults) | Notes |
| --- | --- | --- |
| client settings: Electron `~/.t3/userdata/client-settings.json` (`desk/app/DesktopEnvironment.ts:157-177`, via `desktopBridge.get/setClientSettings`); browser `t3code:client-settings:v1` | `ClientSettingsSchema` (`pkg/contracts/src/settings.ts:42-95`): `autoOpenPlanSidebar` false, `confirmThreadArchive` false, `confirmThreadDelete` true, `dismissedProviderUpdateNotificationKeys` [], `diffIgnoreWhitespace` true, `favorites` [{provider: instanceId, model}], `providerModelPreferences` {instanceId: {hiddenModels, modelOrder}}, `sidebarProjectGroupingMode` "repository", `sidebarProjectGroupingOverrides` {physicalKey: mode}, `sidebarProjectSortOrder` "updated_at", `sidebarThreadSortOrder` "updated_at", `sidebarThreadPreviewCount` 6 (1-15), `timestampFormat` "locale", `wordWrap` true | Every field has a decode default. Unknown keys are ignored. The native app may import the Electron file once. |
| `t3code:ui-state:v1` (`web/uiStateStore.ts:5, 188-225`) | `projectExpandedById` {key: bool}, `projectOrder` [physical or legacy keys], `threadLastVisitedAtById` {threadKey: ISO}, `threadChangedFilesExpandedById` {threadKey: {turnId: false}} (only false entries stored), `defaultAdvertisedEndpointKey` | Debounced 500ms, flushed on unload. Legacy keys `t3code:renderer-state:v3..v8` and `codething:renderer-state:v1..v4` are migrated, then removed. |
| `chat_thread_sidebar_width` | number (px) | §2.1 |
| `t3code:theme` | "system" \| "light" \| "dark" | `web/hooks/useTheme.ts:15` |
| `t3code:composer-drafts:v1` | drafts by thread key, draft threads, logical project → draft key, sticky model selection and provider | composer spec (`web/composerDraftStore.ts:3341-3370`) |
| `t3code:right-panel-state:v2` (v7) | `byThreadKey: {isOpen, activeSurfaceId, surfaces[]}` | chat spec (`web/rightPanelStore.ts:42-43, 530`) |
| `t3code:terminal-state:v1` (v4) | `terminalUiStateByThreadKey: {terminalOpen, terminalHeight, terminalIds, activeTerminalId, terminalGroups, activeTerminalGroupId}` | chat spec |
| `t3code:diff-panel-state:v1` | `byThreadKey` selection, `branchBaseRefByThreadKey` | chat spec |
| `t3code:preview-panel-width` | number | |
| `t3code.fileExplorerOpen` | bool (default true) | |
| `t3code:last-editor` | EditorId | preferred editor for open-in-editor |
| `t3code:last-invoked-script-by-project` | map | chat spec |
| `t3code:provider-update-dismissals:v1` | `{keys: string[]}` | §5.2 |
| `t3code:version-mismatch-dismissals:v1` | dismissal keys | connections spec |
| `t3code:connect-onboarding-opt-out:v1` | opt-out state | cloud |
| IndexedDB `t3code:connection-runtime` (v4) | catalog (saved connections), shell snapshot cache, thread snapshot cache, server config cache, vcs refs cache | connections spec (`web/connection/storage.ts:36-44`) |
| Electron `~/.t3/userdata/saved-environments.json`, `desktop-settings.json` | saved envs, update channel | desktop-owned |

Not persisted: sidebar open/collapsed, thread selection, per-project "Show more", the route, palette state, expanded provider cards, the dismissed desktop update pill (until next launch), provider pill dismissals, and toasts.

---

## 8. Implementation order and hard parts

### 8.1 Order

1. **Shell frame.** Window chrome per platform (traffic lights at 16,18; 52px header drag strip; Win/Linux 40px custom title bar with native controls). Workspace row: sidebar container, main inset, fixed sidebar toggle. Collapse animation, rail resize with the 640 rule, persisted width. Theme tokens and glass surfaces. Client settings and ui-state persistence. Key dispatcher with `sidebar.toggle`, `commandPalette.toggle`, `chat.new`, `chat.newLocal`, `thread.*`.
2. **Sidebar, read-only.** Grouping and sorting (§2.7), project rows (favicon, chevron, status dot, env badge), thread rows (status pill, relative time, worktree, terminal, PR via deduped `subscribeVcsStatus`), expand/collapse, Show more/less, pinned route thread, navigation, `NoActiveThreadState`.
3. **Toast system** (§5.1) and native confirm + native menu wrappers. Every later step depends on them.
4. **Sidebar interactions.** Context menus, inline rename, rename and grouping dialogs, archive hover/confirm, delete flows, multi-select, new-thread button and member picker, jump hints, sort menu.
5. **Command palette.** Root, search, submenus, then the add-project browse and clone flows.
6. **Settings shell + General + Archive**, then Keybindings (table, recorder, when builder).
7. **Providers** (cards, env vars, models, accent picker, add dialog), **Source Control**, **Diagnostics**. Connections follows the connections spec.
8. **Notifications.** Provider update toasts and pill, slow-RPC toast, SSH prompt, PR checkout dialog (with chat).
9. **Polish.** Manual-sort drag and drop, list insert/remove/move animations, hover crossfades, thread prewarming, noise overlay.

### 8.2 Hardest pieces in GPUI

| Piece | Why it is hard | Suggested approach |
| --- | --- | --- |
| Glass surfaces | No per-element `backdrop-filter`. | macOS window blur plus translucent fills (`app-sidebar-glass`, `app-main-glass`). Skip saturate. Draw the noise tile as the last full-window layer at 3.5% opacity with no hit testing. |
| Offcanvas sidebar + rail | Fixed-position container sliding over a reserved gap, both animating; a live clamp against the main width; persisted width. | A custom element: one `sidebar_width` and one `open` value, the gap and container offset driven by a 180ms eased progress (`with_animation` or a frame task). The rail is a 16px absolute hit area using mouse-down/move/up with capture. Write the width on mouse up. Do not force-fit gpui-component's sidebar/resizable modules. |
| Hover-revealed overlays | Archive button vs meta swap, new-thread button vs env badge, chevron vs status dot. All use group hover with 150ms opacity. | `group("thread-row")` + `group_hover`, with absolute children. Opacity transitions need a small animation state per hovered row; instant swaps are a fallback (open question). |
| Native menus and confirms | Exact Electron behavior: separator before the first destructive item, mac trash icon, submenus; No/Yes confirm with No as default. | gpui-kit `NativeMenu` (§5.5) and `Window::prompt`. Map ids to actions through a per-menu enum. |
| Command palette | Combobox with a persistent highlight keyed by item value, a view stack, deferred filtering, browse mode driven by server filesystem queries, footer that changes with mode. | A custom view on top of the t3-ui Input: the item model from `CommandPalette.logic.ts` ported verbatim (pure functions, so test them), the highlight held as an `Option<String>` value, and the list virtualized only if long. |
| Toast stack | Peek, scale, and offset math; expand on hover; swipe; per-toast timers that pause when the window is unfocused; thread scoping. | A custom overlay in the root view. Port `buildVisibleToastLayout`. Skip swipe. Keep the 5s default timeout and the limit of 3. |
| Keybinding engine | "Last matching rule wins", `mod` resolution, `event.code` aliases, dynamic config from the server, runtime context flags. | Do not translate rules into the GPUI keymap. Capture key-down at the root, build a `ShortcutEventLike` from the GPUI `Keystroke` (lowercased key plus physical-key letter/digit aliases), and run the ported resolver. This keeps labels, conflicts, and the editor identical. |
| Per-row live data | VCS status per cwd, discovered ports, running terminals, relative time. | Subscription registries keyed by (env, cwd) with reference counts from visible rows. Relative times recompute only on re-render (fork behavior). |
| List animations | `auto-animate` insert/remove/move at 180ms has no GPUI equivalent. | Defer. Optionally animate height on insert and remove only. |
| Keybindings when builder | A recursive AST editor inside a popover, with selects and toggles. | A pure AST module (port of `KeybindingsSettings.tsx:170-231` plus the logic file) and a recursive render function. |
| Accent color plane | HSV plane with gradients, hue bar, pointer capture. | GPUI `linear_gradient` fills (layer white→transparent and transparent→black over the hue color) and drag tracking with mouse-move while pressed. |

---

## 9. Open questions and risks

1. **Undefined `sidebar-*` utilities** (§0). Separators, settings nav hover/active, and the rail line render as nothing in the fork. This is verified from the CSS build and tailwind-merge, not from screenshots. Confirm against `docs/reference/*.png` before treating it as intended.
2. **Unread state has no server sync upstream.** Upstream lacks `completionAcknowledgedAt` and `thread.completion.acknowledge`, so "Completed" relies only on the local `threadLastVisitedAtById`. Unread state will not sync across devices. Decide whether upstream's settled/pinned model should drive anything; fork-identical means ignoring it.
3. **"Assistant output" toggle.** `enableAssistantStreaming` no longer exists upstream; it was replaced by `responseStreamingMode: "turn" | "paragraph" | "token"` (default paragraph). A mapping decision is needed (for example on → token, off → paragraph), or the row should be hidden.
4. **Commit and PR generation instructions** do not exist in upstream `ServerSettings`. Upstream drops unknown patch keys, so the rows would silently do nothing. Hide them, or keep them as dead controls for visual parity?
5. **Provider settings fields.** Fork and upstream driver schemas differ (§3.6). The config blob is opaque, so rendering fork fields could write keys upstream ignores. Recommendation: fork visuals, upstream field lists.
6. **Keybindings against an upstream server.** The resolved list contains commands the fork does not know, plus `isDesktop` / `editableFocus` conditions. The editor's "Default" detection compares against client defaults: compare against upstream's list, or many rows will read "Custom". Upstream binds `mod+w → rightPanel.close` (outside terminals), which collides with the menu's Close Window (⌘W). Choose a precedence.
7. **Cmd+W and Cmd+R precedence in the fork.** Electron menu accelerators (Close ⌘W, Reload ⌘R) versus the renderer bindings `terminal.close` and `preview.refresh` were not verified. Check in the running fork before choosing the native precedence.
8. **Electron-only features.** Desktop auto-update (Version row, Update track, update pill, Arm64 warning, Help > Check for Updates), View > Reload, Force Reload, Developer Tools, and page zoom have no native meaning. Should zoom map to UI scale? Should the native app ship its own updater, or hide these?
9. **WSL and desktop-local secondary backends.** `LocalSecondaryStatus`, "Open WSL folder", picker routing, the per-environment provider update toast, and WSL UNC handling depend on Electron's desktop bridge. A native equivalent is undecided.
10. **Prewarming.** The fork keeps live thread-detail subscriptions for the first 10 visible threads. Keep that for parity of perceived speed, or drop it to save server load?
11. **Relative times do not tick** in the sidebar or palette (they refresh only on re-render). Identical behavior means stale labels; a 60s tick would be better but differs from the fork.
12. **Continuous animations.** Pulsing Working dots, the pulsing terminal icon, the bouncing update arrow, and spinners conflict with the repo's no-continuous-repaint rule. The fork has them. Keep them small and stop them offscreen, as AGENTS.md allows.
13. **Two confirm styles.** Electron `confirm` uses No/Yes, while `window.confirm` (update install, SIGKILL) uses OK/Cancel. Keep both for parity?
14. **Multi-environment project order.** How `useProjects` orders projects across environments, which decides the default order before `projectOrder` applies, is in the entity store and was not traced here.
15. **Project "not empty" count.** It uses loaded thread shells. Whether archived threads are included depends on the shell store's contents and was not verified.
16. **Settings Escape** navigates back even when focus is in a text field (unless a popup handled the key). Replicate as-is?
