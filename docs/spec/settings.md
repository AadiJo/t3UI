# Settings: framework (routes, layout, nav, search, scope, primitives)

Refreshed against fe7d3092c.

Source: `~/L-Projects/t3UI-refs/t3code-fork` @ `fe7d3092c`. Prefixes: `web/` = `apps/web/src/`,
`desk/` = `apps/desktop/src/`, `pkg/` = `packages/`, `cs/` = `web/components/settings/`.

This file covers everything shared by all settings pages. Page specs:

| Page (nav label) | Route | Spec |
| --- | --- | --- |
| General (+ Diagnostics, Open source licenses entry rows, Restore device defaults) | `/settings/general` | `settings-pages-general-appearance.md` |
| Appearance | `/settings/appearance` | `settings-pages-general-appearance.md` |
| Connections, `/pair`, `/connect`, T3 Connect sidebar sign-in | `/settings/connections` | `settings-pages-connections.md` (+ `settings-pages-pairing.md`) |
| Providers | `/settings/providers` | `settings-pages-providers.md` |
| Keybindings, SnapShots | `/settings/keybindings`, `/settings/snap-shot` | `settings-pages-keybindings-snapshot.md` |
| Project, Source Control, Integrations | `/settings/projects`, `/settings/source-control`, `/settings/integrations` | `settings-pages-projects-integrations.md` |
| Storage, Diagnostics, Archive, Open source licenses | `/settings/storage`, `/settings/diagnostics`, `/settings/archived`, `/settings/open-source-licenses` | `settings-pages-storage-diagnostics.md` |

Base primitives (Button sizes/variants, Switch, Select, Input, Menu, Popover, Tooltip, Alert,
Kbd, Collapsible, toast) are in `design-system.md`. Shell pieces reused here (sidebar container,
`SidebarChromeHeader` stage backdrop, notifications bell, update pill) are in the shell spec.

Desktop assumptions: macOS Electron, window >= 1100 px wide, so `sm:` (>= 640) and `md:` (>= 768)
variants always apply and `max-sm:`/`max-md:` never do. Tailwind 1 unit = 4 px. Radii come from
`--radius: 0.625rem` (`web/index.css:1030`): `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10,
`rounded-xl` 14, `rounded-2xl` 18, `rounded` 4. `text-2xs` = 11 px / 16 px
(`web/index.css:168-169`).

> The old `shell-sidebar-settings.md` §0 finding ("`sidebar-*` colors render nothing") is stale.
> fe7d3092c defines `--color-sidebar-row-hover/-active/-selected`, `--color-sidebar-foreground`,
> `--color-sidebar-muted-foreground`, `--color-sidebar-control-surface`, `--color-sidebar-border`
> (`web/index.css:258-264`). The settings nav has real hover and selected fills.

---

## 1. Routes and navigation

### 1.1 Route table

| Path | File | Renders | Guard / redirect |
| --- | --- | --- | --- |
| `/settings` | `web/routes/settings.tsx:177-193` | layout only | not `authenticated`/`hosted-static` → replace `/pair`; exactly `/settings` → replace `/settings/general` |
| `/settings/general` | `web/routes/settings.general.tsx` | `GeneralSettingsPanel` | parent guard |
| `/settings/appearance` | `settings.appearance.tsx` | `AppearanceSettingsPanel` | device-only page (§5.3) |
| `/settings/projects` | `settings.projects.tsx` | `ProjectsSettings` (`cs/ProjectsSettings.tsx:9-36`) | needs a project scope, else a scope notice (§5.5) |
| `/settings/keybindings` | `settings.keybindings.tsx` | `KeybindingsSettingsPanel` | |
| `/settings/snap-shot` | `settings.snap-shot.tsx` | `SnapShotSettings` | device-only |
| `/settings/providers` | `settings.providers.tsx:12-43` | `ProviderSettingsPanel` for one environment | single-environment scope (§5.3); extra search `environmentId`, `instanceId` |
| `/settings/integrations` | `settings.integrations.tsx` | `IntegrationsSettingsPanel` | |
| `/settings/source-control` | `settings.source-control.tsx` | `SourceControlSettingsPanel` | |
| `/settings/storage` | `settings.storage.tsx` | `StorageSettingsPanel` | |
| `/settings/connections` | `settings.connections.tsx` | `ConnectionsSettings` | device-only |
| `/settings/archived` | `settings.archived.tsx` | `ArchivedThreadsPanel` | |
| `/settings/diagnostics` | `settings.diagnostics.tsx` | `DiagnosticsSettingsPanel` | no nav item; reached from General |
| `/settings/open-source-licenses` | `settings.open-source-licenses.tsx` | `OpenSourceLicensesPanel` | no nav item; General stays highlighted |
| `/projects/$projectKey` | `web/routes/projects.$projectKey.tsx:3-17` | redirect | auth guard as above, then replace → `/settings/projects?project=<projectKey>` (machine cleared). This is the legacy project page link. |
| `/pair`, `/connect` | `web/routes/pair.tsx`, `connect.tsx` | pairing / CLI authorize | see `settings-pages-connections.md` |

Root redirect: when authenticated, the desktop SnapShot bridge exists, SnapShot setup must resume
on startup, and the path is not `/settings/snap-shot`, the root route replaces to
`/settings/snap-shot` (`web/routes/__root.tsx:123-130`).

### 1.2 Search params and hash

Every settings route shares the scope search (`cs/settingsScope.ts:152-156`):

| Param | Meaning |
| --- | --- |
| `project` | logical project key (`SidebarProjectSnapshot.projectKey`). Absent = environment defaults. |
| `machine` | environment id. Absent = all environments. |
| `checkout` | physical project key (`<environmentId>:<normalized workspace root>`). Requires `project`. |

- Validation: each must be a non-empty (after trim) string, else dropped. Stale ids are kept so a
  removed target resolves as "unavailable", never as "all" (`settingsScope.ts:186-198`).
- Legacy: `?environmentId=X` with no `machine` and no `project` is read as `machine=X`
  (`cs/settingsScopeNavigation.ts:5-12`).
- Retain middleware (`settingsScopeNavigation.ts:24-31`): navigating between settings pages keeps
  the previous `project`/`machine`/`checkout` unless the new navigation names any of
  `project`, `machine`, `checkout`, `environmentId`. An explicit target replaces the whole
  selection. So nav clicks keep the scope; scope changes send all three keys.
- `/settings/providers` also accepts `environmentId` and `instanceId` (trimmed non-empty strings,
  `settings.providers.tsx:34-41`); `instanceId` opens/focuses that provider instance card.
- Hash = settings-search target id (§4.3). History state `settingsTargetHighlight` (default true)
  controls the pulse (`cs/settingsLayout.tsx:42-46, 519-521`).

Native: `Route::Settings` must carry `{ page, scope: SettingsScopeSearch, target: Option<String>,
highlight: bool }` plus providers' `instance_id`. Retain rule: a nav click copies the current
scope; a scope change replaces it.

### 1.3 Entry points

| Trigger | Navigation | Pointer |
| --- | --- | --- |
| Sidebar footer gear "Settings" (tooltip "Settings", top) | push `/settings` → `/settings/general` (retains nothing; no prior scope) | `web/components/sidebar/SidebarChrome.tsx:127-130, 155-159` |
| macOS app menu "<AppName> > Settings..." `Cmd+,` (Win/Linux: "File > Settings...") | desktop sends menu action `open-settings`; web pushes `/settings` unless already on a settings route | `desk/window/DesktopApplicationMenu.ts:168-172, 190-196`; `web/components/AppSidebarLayout.tsx:276-292` |
| Command palette action "Open settings" (terms settings, preferences, configuration, keybindings) | push `/settings` | `web/components/CommandPalette.tsx:2244-2253` |
| Command palette: source-control setup link; "Add project" with no connected environment | push `/settings/source-control`; push `/settings/connections` | `CommandPalette.tsx:1585-1588, 1782-1790` |
| Provider update pill, launch notification, primary notification | push `/settings/providers` | `SidebarProviderUpdatePill.tsx:66`, `ProviderUpdateLaunchNotification.tsx:116`, `ProviderUpdatePrimaryNotification.tsx:153` |
| Chat "set up provider" actions | push `/settings/providers?environmentId=<env>&instanceId=<id>` (legacy `environmentId` becomes `machine`) | `web/components/ChatView.tsx:4607-4614` |
| Git actions "configure" | push `/settings/source-control` | `web/components/GitActionsControl.tsx:594` |
| Empty home "connect" button | `Link` to `/settings/connections` | `web/routes/_chat.index.tsx:150` |
| Legacy `/projects/<key>` | replace `/settings/projects?project=<key>` | §1.1 |

There is no default keybinding for settings beyond the menu accelerator `Cmd+,`.

### 1.4 Leaving settings (Back and Escape)

- `useNavigateToMainApp` (`web/components/sidebar/mainAppLocation.ts:31-36`): navigate (push) to
  the last "main app" href, or `/` if none. `MainAppLocationTracker` (mounted once in
  `AppSidebarLayout.tsx:330`) records `location.href` on every navigation whose path is not a
  utility page. Utility pages: `/settings`, `/settings/*`, `/projects/*`, `/usage`,
  `/pull-requests` (`mainAppLocation.ts:7-15`). So Back returns to the exact thread/draft (with
  its search) that was open before entering settings, no matter how many settings pages were
  visited.
- Escape (`web/hooks/useNavigateBack.ts:19-40`, installed by `settings.tsx:116`): a window
  keydown listener. Ignored when `defaultPrevented`, `repeat`, `isComposing`, or key is not
  Escape. Otherwise `preventDefault`, blur the active element, then navigate to the main app.
  Controls that consume Escape first (menus, dialogs, popovers, the search box while it has a
  query, key recorders) call `preventDefault`/`stopPropagation`, so Escape closes them instead.
- Sidebar footer "Back" button (§3.6) does the same navigation. Shell side: `shell.md` (routes,
  `isSidebarUtilityPage`, `last_main_route`) and `sidebar.md` §9.1.

Native: keep a `last_main_route: Route` in `AppState`, updated whenever the route is not
`Settings`/`Usage`/`PullRequests`. Escape handler must run after (and yield to) focused controls.

---

## 2. Layout

### 2.1 Box tree

```
SidebarProvider (shell)
├ Sidebar (aria-label "Settings" on settings routes)        §3
│   SidebarChromeHeader  +  SettingsSidebarNav
└ SidebarInset <main>  h=100dvh, min-h 0, overflow hidden, overscroll-y none, isolate
  │                    bg `background` + `surface-grain` (shell glass rules apply)
  └ div  flex col, flex-1, min-h 0, min-w 0, bg `background`, text `foreground`
    ├ WorkspacePageHeader (drag region)                     §2.2
    │   div flex w-full items-center gap 12
    │   ├ SettingsBreadcrumb                                §2.3
    │   └ (General only) div ms-auto: "Restore device defaults" button   §2.4
    └ div key=`${JSON(search)}:${restoreSignal}`  flex-1 min-h 0 flex col
        └ SettingsScopeBoundary                             §5.5
            └ page component (usually SettingsPageContainer)  §2.5
```

Pointers: `web/routes/settings.tsx:113-147`, `web/components/ui/sidebar.tsx:546-557`
(`SidebarInset`).

The keyed body div remounts the whole page whenever the scope search changes or after a restore
(`settings.tsx:136-139`): local page state (open cards, drafts, scroll) resets. Page navigation
also remounts (new route component). Native: rebuild page views on (page, scope) change and after
restore; do not keep page entities alive across pages (the `origin/settings` `SettingsView`
caches them; change that).

### 2.2 Header (`WorkspacePageHeader`, `web/components/WorkspacePageHeader.tsx:7-28`)

- `<header>`: h = `--workspace-topbar-height` (52 px on macOS; Win/Linux WCO uses
  `env(titlebar-area-height, 52px)`), min-h same, shrink 0, flex, items-center, gap 12.
- Padding: left `--workspace-gutter-start`, right `--workspace-gutter-end` = 20 px each at
  `sm` (`--workspace-gutter: 1.25rem`, `web/index.css:123-130`); bottom
  `--workspace-titlebar-items-lift-padding` (0).
- Electron: `drag-region` (interactive descendants are no-drag, `web/index.css:1717-1740`).
  WCO (Win/Linux): extra right padding `--workspace-native-controls-inset`.
- Collapsed sidebar: padding-left becomes `--workspace-titlebar-content-left` =
  `controls-left + 28 + 12`. macOS windowed: controls-left = `70px/zoom + 12` = 82 → 122 px;
  macOS fullscreen and Win/Linux: controls-left 12 → 52 px (`web/workspaceTitlebar.ts:1-2`,
  `ui/sidebar.tsx:169-170`, `desk/preload.ts:36-46`, `AppSidebarLayout.tsx:58, 252-256`).
- Padding transition only when `data-panel-animations=true`: `padding-left, padding-right`,
  duration `--panel-animation-duration` (client setting `panelAnimationDurationMs`), ease-out,
  motion-safe only.

### 2.3 Breadcrumb (`cs/SettingsBreadcrumb.tsx`, `web/components/WorkspaceBreadcrumb.tsx`)

- `<nav aria-label="Settings breadcrumb">` min-w 0 → `<ol>` flex, items-center, gap 12 (`sm:gap-3`),
  `text-sm` (14/20).
- Items, when the path has a label: `Settings` (li: medium, `muted-foreground`, shrink 0), then a
  separator li `/` (`text-icon-muted`, shrink 0, aria-hidden), then the current label (li: medium,
  `foreground`, truncate, `aria-current="page"`).
- Labels: the nav labels (§3.4) plus `/settings/diagnostics` → "Diagnostics",
  `/settings/open-source-licenses` → "Open source licenses". A trailing slash is ignored. An
  unknown path shows only "Settings" as the current item.
- Example: `Settings / General`, `Settings / Source Control`, `Settings / SnapShots`,
  `Settings / Project`, `Settings / Archive`.

### 2.4 "Restore device defaults" (General only)

`settings.tsx:31-44, 126-132`. Button `size="xs" variant="ghost"`, icon `RotateCcw` 14 px with
`mx-1` (4 px each side), label "Restore device defaults". Disabled when `useSettingsRestore`
reports no changed labels. Click → `restoreDefaults()`; on success `restoreSignal += 1`, which
remounts the body. The changed-label list, confirm copy, and what gets reset are in
`settings-pages-general-appearance.md` (`cs/SettingsPanels.tsx:511-…`).

### 2.5 `SettingsPageContainer` (`cs/settingsLayout.tsx:508-550`)

```
SettingsSearchTargetProvider (targetId = location.hash without "#", or null)
└ div  flex-1, overflow-y auto, `topbar-scroll-fade`, `scrollbar-gutter-both`,
  │    data-settings-page-scroll
  └ WorkspacePageContainer width (default "readable")
      mx auto, w-full, flex col, gap 32 (`gap-8` overrides `gap-6`),
      px 24 (`sm:px-6`), pt 24, pb 48
      max-w: readable 896 (`max-w-4xl`), wide 1024 (`max-w-5xl`), expanded 1152 (`max-w-6xl`)
      ├ SettingsScopeSentence      §5.2 (null on device-only pages)
      └ page sections...
```

- `topbar-scroll-fade` (`web/index.css:408-450`): a mask over the scroll viewport. Top band of
  `--workspace-titlebar-scroll-fade-height` (24 px) + 1 px fades content from transparent (0%) to
  opaque (100%) with stops 10%→.10, 24%→.30, 42%→.58, 62%→.82, 82%→.96. The scrollbar column
  (`--app-scrollbar-width` 6 px, right edge) is excluded from the mask. Native: paint a 25 px
  gradient overlay of the page background at the top of the scroll area, or clip content with an
  alpha mask; it is static (no animation).
- Scrollbar: 6 px thumb, `--app-scrollbar-thumb` (dark: white 8%, hover 12%).
- `width` overrides used by pages: see each page spec (most use `readable`).

---

## 3. Settings sidebar (nav)

### 3.1 Mount

`AppSidebarLayout.tsx:321-327`: while the path is `/settings` or starts with `/settings/`, the
sidebar shows `SidebarChromeHeader` (the 52 px titlebar row; stage artwork backdrop or env pill
per Appearance > Environment identification; shell spec) followed by `SettingsSidebarNav`
instead of the thread sidebar. `aria-label` becomes "Settings". Width, resize, collapse, and the
rail are unchanged (shell spec). `/projects/*` redirects before render, so it never shows the
thread sidebar in between.

### 3.2 Structure (`cs/SettingsSidebarNav.tsx:234-364`)

```
SidebarContent  (ScrollArea, hidden scrollbars, 12 px scroll fade at the edges, overflow-x hidden)
└ SidebarGroup  p 8 (--sidebar-content-inset)
  └ div flex col gap 8
    ├ search row                     §3.3
    ├ "No settings found"            (searching, no results)
    └ result list (searching)  |  nav list (not searching)     §3.4 / §3.5
SidebarFooter  flex col gap 8, px 8, py 4                      §3.6
```

### 3.3 Search row

- Container: h 32, flex, items-center, gap 8, `rounded-md` (8), px 8, py 6, `text-sm` medium,
  `sidebar-muted-foreground`; hover: bg `sidebar-row-hover`, text `sidebar-foreground`.
- `Search` icon 16 px, `sidebar-muted-foreground/80`, shrink 0.
- Input (`SidebarInput` unstyled, native `<input type="search">`): flex-1, min-w 0, no padding,
  auto height, `text-sm` medium, leading-normal, text `sidebar-foreground`, placeholder "Search"
  in `sidebar-muted-foreground`. `aria-label="Search settings"`, `role="combobox"`,
  `aria-autocomplete="list"`, `aria-expanded` = searching with results, `aria-controls` =
  `settings-search-results`, `aria-activedescendant` = `settings-search-result-<id>`.
- Trailing slot:
  - empty query: `Kbd` "/" (h 20, min-w 20, `rounded` 4, bg `sidebar-control-surface`,
    px 4, `text-xs` medium sans, `muted-foreground`, 1 px ring `sidebar-border`; `web/components/ui/kbd.tsx`).
  - non-empty query (after trim): ghost-muted `icon-micro` button `X` 12 px,
    `aria-label="Clear settings search"`. Click clears the query, resets the active index to 0,
    and refocuses the input.
- Typing sets the query and resets the active index to 0. `isSearching` = trimmed query non-empty.

### 3.4 Nav list (not searching)

`SidebarMenu` (flex col, gap 4). One `SidebarMenuItem` > `SidebarMenuButton` (default size) per
item, in this order (`cs/settingsSearch.ts:85-98`, icons `SettingsSidebarNav.tsx:53-90`):

| Label | Path | Icon (lucide unless noted) | Visible |
| --- | --- | --- | --- |
| Project | `/settings/projects` | `PanelsTopLeft` | only when the scope search names a project (`project` set, and not `checkout` without `project`) (`settingsSearch.ts:968-971`) |
| General | `/settings/general` | `Settings2` | always |
| Appearance | `/settings/appearance` | `Palette` | always |
| Keybindings | `/settings/keybindings` | `Keyboard` | always |
| SnapShots | `/settings/snap-shot` | custom "snap-shot" icon (below) | always |
| Providers | `/settings/providers` | `Bot` | always |
| Integrations | `/settings/integrations` | `Blocks` | always |
| Source Control | `/settings/source-control` | `GitBranch` | always |
| Storage | `/settings/storage` | `HardDrive` | always |
| Connections | `/settings/connections` | `Link2` | always |
| Archive | `/settings/archived` | `Archive` | always |

Custom SnapShot icon (24 grid, lucide stroke 2, round caps/joins; `SettingsSidebarNav.tsx:53-63`):
path `M8 3H6a3 3 0 0 0-3 3v2M16 3h2a3 3 0 0 1 3 3v2M21 16v2a3 3 0 0 1-3 3h-2M8 21H6a3 3 0 0 1-3-3v-2`,
rect `x7 y8 w10 h8 rx2`, circle `cx12 cy12 r1.5`.

Button (`web/components/ui/sidebar.tsx:657-678`): h 32, `rounded` = `--control-radius` 8, px 10
(`--sidebar-row-content-inset`), py 6, gap 8, `text-sm` medium, text
`sidebar-muted-foreground/80`, label truncates. Icon 16 px in `--sidebar-icon-color`
(= `sidebar-muted-foreground` 60% mixed into `--sidebar`).

| State | Fill | Text / icon |
| --- | --- | --- |
| rest | none | `sidebar-muted-foreground/80` / `--sidebar-icon-color` |
| hover | `sidebar-row-hover` | `sidebar-foreground` / `sidebar-foreground` |
| pressed | `sidebar-row-active` | `sidebar-foreground` |
| active (current page) | `sidebar-row-selected` | `sidebar-foreground`, medium / `sidebar-foreground` |
| focus-visible | 2 px `ring` ring | |

Transition: only `width, height, padding` (fills change instantly).

Active rule: path equals the item path or starts with `<path>/`; also General is active on
`/settings/open-source-licenses` (`SettingsSidebarNav.tsx:326-332`). Nothing is active on
`/settings/diagnostics`.

Click (`:168-181`): navigate to the path, **replace** (not push), hash cleared, no hash scroll.
The scope search is kept by the retain middleware.

### 3.5 Search results (searching)

- No results: `<p role="status">` "No settings found", px 8, py 24, centered, `text-xs`,
  `sidebar-muted-foreground`.
- Results: `SidebarMenu` `id="settings-search-results"`, `role="listbox"`,
  `aria-label="Settings search results"`. Each item: `SidebarMenuButton size="sm"` with
  `role="option"`, `aria-selected`, `tabIndex=-1`, `id="settings-search-result-<item.id>"`,
  class `h-auto min-h-10 items-start` → min-h 40, `rounded-lg` (10), p 8, gap 8.
  - Left: the section icon (§3.4) at 14 px, `mt-0.5` (2 px), `sidebar-muted-foreground/60`.
  - Text column (min-w 0, flex-1): title (`text-sm` medium `sidebar-foreground`, truncate),
    then the section label (e.g. "General") in `text-2xs` (11/16) `sidebar-muted-foreground/75`,
    truncate.
  - The active result (keyboard index) renders `data-active` → bg `sidebar-row-selected`.
    `mousemove` over a result makes it active. Click activates it (§4.3).
- Active index stays clamped to `[0, results.length-1]` when results shrink; the active result is
  scrolled into view with `block: "nearest"` (`:124-134`).

### 3.6 Footer (`SettingsSidebarNav.tsx:350-363`)

```
SidebarFooter (flex col gap 8, px 8, py 4)
├ T3ConnectSidebarSignIn     (lazy; renders null until loaded or when not applicable)
└ div flex items-center gap 4
  ├ div min-w 0 flex-1 → SidebarUtilityMenu
  │     SidebarMenu flex-row items-center:
  │       "Back" button (SidebarMenuButton default, `ArrowLeft` 16 + "Back"; min-w 0 flex-1)
  │       SidebarNotifications (bell; shell spec)
  │       SidebarUpdatePill (shell spec)
  └ T3ConnectSidebarAvatar   (lazy)
```

"Back" → `useNavigateToMainApp()` (§1.4) (`SidebarChrome.tsx:139-152`). The T3 Connect sign-in
card and avatar are specified in `settings-pages-connections.md`.

### 3.7 Keyboard

| Key | Where | Effect | Pointer |
| --- | --- | --- | --- |
| `/` (no Cmd/Ctrl/Alt) | anywhere in settings, unless the target is an input, textarea, contenteditable, or inside `[role=dialog]`, `[aria-modal=true]`, `[data-slot$=popup]` | preventDefault; expand the sidebar if collapsed; next frame focus + select-all the search input | `SettingsSidebarNav.tsx:136-166` |
| ArrowDown / ArrowUp | search input, results non-empty | move active index with wraparound | `:216-225` |
| Enter | search input, results non-empty | open the active result | `:226-230` |
| Escape | search input with a non-empty query | preventDefault + stopPropagation, clear query (does not leave settings) | `:209-214` |
| Escape | elsewhere (or empty query) | leave settings (§1.4) | |

---

## 4. Settings search

### 4.1 Matching and ranking (`cs/settingsSearch.ts:1003-1048`)

1. `normalize(s)` = NFKD, strip combining marks (`\p{M}`), lowercase, collapse whitespace runs to
   one space, trim (`web/lib/utils.ts:23-25`). Empty normalized query → no results (the nav list
   shows).
2. Platform filters: drop `desktopOnly` items outside Electron (native: never drop), `macOnly`
   unless the platform matches `/mac|iphone|ipad|ipod/i`, `windowsOnly` unless `/^win(dows)?/i`.
3. Tokens = normalized query split on " ". An item matches when every token is a substring of at
   least one field: normalized title, normalized section label (nav label of `item.to`), and each
   normalized search term string.
4. Rank: 5 title == query; 4 title starts with query; 3 title contains query; 2 every token in
   title; 1 some field contains the whole query; else 0.
5. Sort: non-`secondary` before `secondary`; then rank desc; then registry order.

Availability filter applied before search (`settingsSearch.ts:986-1001`,
`cs/useAvailableSettingsSearchItems.ts:19-72`):

| Flag | Kept when |
| --- | --- |
| `cloudOnly` | T3 Connect public config present (Clerk publishable key, JWT template, relay URL; `web/cloud/publicConfig.ts`). Native: T3 Connect configured. |
| `environmentOnly` | some environment has a server config |
| `providerSettingsOnly` | some environment is connected and has a server config |
| `macProviderSettingsOnly` | some connected environment (the selected `machine`, if set) reports `platform.os == "darwin"` |
| `localBackendManagementOnly` | the local environment is not disabled and (Electron, or the primary session is authenticated with the `AuthAccessWriteScope` scope) |
| `localEnvironmentOnly` | the local environment is not disabled (`desktopBridge.getLocalEnvironmentEnabled() !== false`) |
| `wslAvailableOnly` | desktop WSL state is available, enabled, or WSL-only (or failed to load) |
| `requiresThreadAutoSettlement` | at least one connected environment has capability `threadAutoSettlement` |

### 4.2 Registry (`cs/settingsSearch.ts:131-861`)

Result order = this order (before ranking). "Target" is the anchor id the page must render
(`targetId ?? id`). Scope is the required settings scope (§4.4); blank = none. Pages render row
ids/titles through `searchableSetting(id)` (`:978-984`), so these titles are also the row titles.

| id | Title | Page | Target | Scope | Flags | Extra search terms |
| --- | --- | --- | --- | --- | --- | --- |
| `storage-worktrees` | Worktree cleanup | Storage | | project-defaults | | disk storage delete deleted archived threads old inactive merged unchanged worktrees retention days project inherit off custom |
| `storage-artifacts` | Artifacts and logs | Storage | | environment-defaults | | disk storage browser screenshots captures rotated logs cleanup retention |
| `project-defaults` | Project defaults and overrides | General | | project-defaults | | model workspace environments projects inheritance checkout |
| `project-overview` | Project overview | Project | | (category: project) | | name icon emoji image checkout remove delete |
| `default-model` | Default model | General | | project-defaults | | new thread project provider reasoning effort |
| `default-permissions` | Permissions | General | | project-defaults | | new thread default runtime mode supervised approvals auto accept edits full access |
| `color-scheme` | Color scheme | Appearance | `appearance` | | | appearance light dark system mode |
| `theme` | Themes | Appearance | `appearance` | | | appearance colors palette custom import |
| `setting-appearance-contrast` | Contrast | Appearance | | | | colors borders interface |
| `setting-glass-opacity` | Glass opacity | Appearance | | | | transparent transparency solid menus dialogs composer |
| `diff-color-scheme` | Diff colors | Appearance | | | | red green blue orange additions deletions changes counts palette colorblind |
| `chat-width` | Chat width | Appearance | | | | wide full width column layout messages composer monitor |
| `panel-animations` | Panel animations | Appearance | | | | |
| `environment-identification` | Environment identification | Appearance | `appearance-interface` | | | dev nightly artwork pill label hide none |
| `interface-font` | Interface font | Appearance | | | | typography family size system sans |
| `prompt-font` | Prompt font | Appearance | | | | typography family size composer input |
| `code-font` | Code font | Appearance | | | | typography family size monospace code blocks diffs file previews |
| `terminal-font` | Terminal font | Appearance | | | | typography family size monospace output |
| `font-smoothing` | Font smoothing | Appearance | | | macOnly | typography text grayscale anti aliasing macos thin |
| `word-wrap` | Word wrap | Appearance | | | | long lines code blocks tables diffs file previews |
| `project-grouping` | Project grouping | General | | | | combine matching repositories environments sidebar |
| `working-shelf` | Working section (beta) | General | | | | hide fold running monitoring threads inbox sidebar shelf |
| `auto-settle-inactive-threads` | Auto-settle inactive threads | General | | project-defaults | requiresThreadAutoSettlement | sidebar inactivity days no activity automatically |
| `auto-settle-merged-threads` | Auto-settle merged threads | General | | project-defaults | requiresThreadAutoSettlement | pull request merge closed automatically sidebar |
| `days-before-auto-settle` | Days of inactivity before auto-settle | General | `auto-settle-inactive-threads` | project-defaults | requiresThreadAutoSettlement | thread timeout activity sidebar |
| `thread-notifications` | Thread notifications | General | | | | notification sound alert completion input approval desktop |
| `in-app-notifications` | In-app notifications | General | | | | notification toast popup completion input approval failure |
| `time-format` | Time format | General | | | | timestamp clock locale system browser os 12 hour 24 hour |
| `response-streaming` | Response streaming | General | | project-defaults | | output token paragraph buffered wait turn legacy |
| `hide-whitespace-changes` | Hide whitespace changes | General | | | | diff ignore spaces edits default |
| `default-diff-file-state` | Default diff file state | General | | | | collapsed expanded collapse expand files pull request pr code tab |
| `diff-layout` | Diff layout | General | | | | stacked split side by side unified inline view |
| `proactive-panels` | Proactive panels | General | | | | automatically open diff pull request pr right panel agent completion |
| `resume-compaction-banner` | Suggest compacting idle Claude threads | General | | | | resume with less context compact cache expired tokens banner claude usage |
| `skills-in-slash-menu` | Show skills in slash menu | General | | | | command menu dollar $ slash / |
| `composer-rich-text` | Rich text composer | General | | | | composer rich text tiptap bold italic markdown styled wysiwyg |
| `composer-collapse` | Collapse composer on scroll | General | | | | composer rest resting scroll wheel conversation timeline shrink minimize |
| `send-shortcut` | Send shortcut | General | | | | enter return command ctrl multiline prompt new line composer |
| `follow-up-behavior` | Follow-up behavior | General | | | | queue steer running turn send default behavior composer |
| `provider-update-checks` | Provider update checks | General | | environment-defaults | | installed cli versions newer available codex claude cursor grok opencode |
| `continue-threads-after-server-update` | Continue threads after restarts | General | | project-defaults | | resume running active interrupted work restart reboot machine crash desktop update automatically |
| `background-activity` | Background activity | General | | environment-defaults | | balanced performance battery saver advanced git fetch provider health refresh host power monitor idle policy |
| `new-threads` | New threads | General | | project-defaults | | default workspace mode draft local worktree |
| `worktree-submodules` | Submodules | General | | project-defaults | | git submodule init recursive top-level none worktree t3.json |
| `start-from-origin` | Start from origin | General | | project-defaults | | new worktrees latest matching remote branch local |
| `add-project-starts-in` | Add project starts in | General | | environment-defaults | | base directory folder browser path home |
| `unpin-confirmation` | Unpin confirmation | General | | | | ask before thread pinned section |
| `archive-confirmation` | Archive confirmation | General | | | | ask before thread second click inline action |
| `delete-confirmation` | Delete confirmation | General | | | | ask before thread chat history |
| `quit-confirmation` | Quit shortcut | General | | | desktopOnly | confirmation desktop app exit direct hold double click press twice |
| `text-generation-model` | Text generation model | General | | project-defaults | | generated thread titles source control content default provider |
| `diagnostics` | Diagnostics | General | | | | logs traces processes resource history failures spans cpu memory |
| `open-source-licenses` | Open source licenses | General | | | | |
| `legacy-plan-mode` | Plan mode (legacy) | General | | | | build plan composer old |
| `legacy-context-window-indicator` | Context window indicator (legacy) | General | | | | composer meter usage tokens circle old |
| `legacy-access-level-indicator` | Access level indicator (legacy) | General | | | | composer runtime mode permissions supervised full access old |
| `legacy-sidebar` | Sidebar (legacy) | General | | | | project thread tree old flat list |
| `legacy-notification-inbox` | Notification inbox (legacy) | General | | | desktopOnly | bell sidebar notifications unread mark read macos old |
| `keybindings` | Keybindings | Keybindings | | | | keyboard shortcuts hotkeys commands bindings json |
| `keybinding-<command>` (one per static command) | command label | Keybindings | `keybindings` when the command has no default binding | | secondary | the command id + its default key strings |
| `snap-shot-enabled` | SnapShots | SnapShots | | | | window capture screenshot |
| `snap-shot-accessibility` | Include app text | SnapShots | `snap-shot-enabled` | | | capture accessibility data text UI structure elements privacy omit agent context |
| `snap-shot-shortcut` | Capture shortcut | SnapShots | `snap-shot-enabled` | | | |
| `snap-shot-sound` | Capture sound | SnapShots | `snap-shot-enabled` | | | |
| `snap-shot-flash` | Capture flash | SnapShots | `snap-shot-enabled` | | | |
| `snap-shot-animations` | Capture animations | SnapShots | `snap-shot-enabled` | | | |
| `providers` | Providers | Providers | | | | agents cli codex claude cursor grok opencode antigravity google sign in sign out install subscription instances authentication api key models configuration binary path config directory endpoint arguments environment variables display name accent color custom favorite hidden auto compact |
| `usage-providers` | Usage providers | Providers | | | providerSettingsOnly | usage sources CLIProxyAPI CLI proxy hub quota subscription limits management key add remove |
| `cursor-keychain-usage` | Cursor account usage | Providers | | | providerSettingsOnly, macProviderSettingsOnly | cursor macOS keychain usage tokens cost limits permission |
| `provider-health-check-interval` | Health check interval | Providers | | | providerSettingsOnly | refresh availability versions auth state models background probes seconds off |
| `agent-browser-access` | Agent browser access | Integrations | | project-defaults | | allow disable enable open drive preview tools sessions project override |
| `device-hosts` | Device hosts | Integrations | | | | ssh remote simulator emulator ios android mac mini identity key connection |
| `agent-device-access` | Agent device access | Integrations | `devices` | | | allow simulator emulator ios android drive tools sessions |
| `device-hub` | Device hub | Integrations | `devices` | | | simulator emulator ios android install start |
| `device-platform-support` | Simulator support | Integrations | `devices` | | | xcode android studio sdk avd runtime |
| `browser-profiles` | Browser profiles | Integrations | `browser` | | | |
| `browser-default-profile` | Default browser profile | Integrations | `browser-profiles` | | | |
| `browser-default-viewport` | Default browser viewport | Integrations | | | | preview size width height device desktop mobile rotate |
| `browser-default-zoom` | Default browser zoom | Integrations | | | | preview page scale tabs percent |
| `browser-default-appearance` | Default browser appearance | Integrations | | | | preview color scheme light dark system os |
| `browser-recording-frame-rate` | Browser recording frame rate | Integrations | | | | |
| `browser-recording-key-presses` | Show key presses in recordings | Integrations | | | | browser preview keyboard shortcuts keystrokes overlay capture |
| `browser-recording-mouse-presses` | Show mouse presses in recordings | Integrations | | | | browser preview clicks buttons drag overlay capture |
| `browser-link-target` | Open links in | Integrations | | | | links default browser in-app browser external open |
| `browser-auto-show-floating-preview` | Auto-show floating preview | Integrations | | | | agent opens browser device simulator pop into view hide |
| `automatic-pull` | Automatically pull | Source Control | | project-defaults | | auto pull default branch current checkout fast forward upstream |
| `pull-request-merge-method` | Default merge method | Source Control | | project-defaults | | pull request merge squash rebase last selected |
| `source-control` | Source control | Source Control | | environment-defaults | | version control git github gitlab forgejo gitea tea codeberg bitbucket azure devops hosting integrations credentials scan server environment |
| `git-fetch-interval` | Git fetch interval | Source Control | | environment-defaults | environmentOnly | automatic remote branch refresh background credentials security keys seconds off |
| `bitbucket-credentials` | Bitbucket credentials | Source Control | | environment-defaults | environmentOnly | bitbucket atlassian access token api token email credentials sign in |
| `source-control-writing-style` | Source control writing style | Source Control | | (category: environment-defaults) | environmentOnly | repository conventions conventional commits custom instructions change descriptions request titles |
| `follow-change-request-templates` | Follow change request templates | Source Control | | (category) | environmentOnly | repository pr pull request description structure |
| `source-control-writer-model` | Source control writer model | Source Control | | project-defaults | environmentOnly | override generated commit change request pr titles descriptions branch bookmark |
| `project-actions` | Actions | Project | | (category: project) | | commands scripts setup run dev server checkout worktree t3.json import |
| `environment-icon` | Environment icon | Connections | `connections-environment` | | localBackendManagementOnly | machine glyph sidebar mac mini studio laptop desktop server cloud vm |
| `local-environment` | Local environment | Connections | `connections-environment` | | desktopOnly | turn off on disable enable local server agents remote only restart |
| `network-access` | Network access | Connections | `connections-environment` | | localBackendManagementOnly | expose backend remote pairing local machine interfaces host restart |
| `tailscale-https` | Tailscale HTTPS | Connections | `connections-environment` | | desktopOnly, localBackendManagementOnly | serve magicdns endpoint remote secure network |
| `wsl-backend` | WSL backend | Connections | | | desktopOnly, windowsOnly, localBackendManagementOnly, wslAvailableOnly | windows subsystem linux distro second server projects stop windows backend restart |
| `t3-connect` | T3 Connect | Connections | `connections-environment` | | localEnvironmentOnly, desktopOnly, cloudOnly | managed tunnel cloud other devices remote |
| `publish-agent-activity` | Publish agent activity | Connections | `connections-environment` | | localEnvironmentOnly, cloudOnly | mobile push notifications live activities cloud tunnel |
| `connections-environment` | This machine | Connections | | | | connections server backend local remote access administrative permissions scope pairing links qr code authorized clients sessions revoke endpoint |
| `remote-environments` | Environments | Connections | | | | add pair backend host code ssh config agent tunnel saved t3 connect |
| `load-balancing` | Load balancing | Connections | | | | automatic machine environment resources cpu memory capacity preference weight shared projects |
| `github-routing` | GitHub sharing | Connections | | | | pull request trusted environments shared credentials permissions read actions |
| `archive` | Archived threads | Archive | | (category: project-defaults) | | restore reopen deleted history projects |

Keybinding items (`settingsSearch.ts:110-124`): one per `STATIC_KEYBINDING_COMMANDS`, sorted by
`commandLabel(command)` (`cs/KeybindingsSettings.logic.ts`), id `keybinding-<command>`,
`secondary: true`, search terms `[command, ...default key strings]`. Full table in
`settings-pages-keybindings-snapshot.md`.

### 4.3 Opening a result and landing on the target

Click or Enter on a result (`SettingsSidebarNav.tsx:186-206`):

1. Clear the query and active index.
2. `targetId = item.targetId ?? item.id`.
3. Already on `item.to` with the same hash → scroll to the target again (`scrollToSettingsTarget`).
4. Else navigate to `item.to` with `hash = targetId`, **replace**, no browser hash scroll, history
   state `{ settingsTargetHighlight: true }`. The scope search is retained.

Landing (`cs/settingsLayout.tsx:79-138, 508-550`):

- `SettingsPageContainer` reads the hash as `targetId`. Any `SettingsSection`, `SettingsRow`,
  `SettingsSearchTarget`, or `FoldedSettingsSection` whose `id` equals it handles it on mount:
  - scroll element: a child `[data-settings-scroll-target]` if present, else (for `<section>`) its
    first child, else the element. `scrollIntoView({ behavior: smooth, block: "center" })`;
    `behavior: auto` with reduced motion.
  - focus the element (`tabIndex=-1`, `preventScroll`).
  - pulse (unless reduced motion or `highlight=false`): class `settings-search-target-pulse`
    on the element. CSS (`web/index.css:856-876`): for `div` targets the element itself, for
    `section` targets its first child div (the header row). Keyframes: box-shadow
    `0 0 0 0 transparent` at 0%/100%, `0 0 0 2px <primary 70% in oklab>` at 50%;
    `650ms ease-in-out`, 2 iterations (1.3 s total, then stops); border-radius 12 px during the
    animation; suppresses the focus outline. The class is removed on blur.
  - then replace-navigate with `hash: ""` (scroll kept, state reset to highlight true).
- `FoldedSettingsSection` opens itself before scrolling when it is the target (§6.9).
- Native: implement as "scroll to anchor id + 2 x 650 ms ring pulse" (finite, not continuous).
  Anchors that a page may not mount use `targetId` to point at a stable section.

### 4.4 Scope gating of search targets

`getSettingsSearchTargetScope(hash)` (`settingsSearch.ts:884-899`) finds the item by `id` (or
first item whose `targetId` equals the hash) and returns `{ title, scope, requiresThreadAutoSettlement }`.
`scope` = the item's `scope` or its page's category scope:

| Page | Category scope (`settingsSearch.ts:866-879`) |
| --- | --- |
| Project | `project` |
| Source Control | `environment-defaults` |
| Storage | `project-defaults` |
| Connections | `connections` |
| Archive | `project-defaults` |
| all others | none |

Availability (`isSettingsSearchScopeAvailable`, `:936-966`) by resolved scope kind:

| Required | all | environment | project | checkout |
| --- | --- | --- | --- | --- |
| none, `connections` | yes | yes | yes | yes |
| `environment` | no | yes | no | no |
| `checkout` | no | no | no | yes |
| `project` | no | no | yes | yes |
| `environment-defaults` | yes | yes | no | no |
| `project-defaults` | yes | yes | yes | yes |

`SettingsScopeBoundary` (`settings.tsx:46-111`) checks, in order, while a hash is present and the
scope is not `unavailable`:

1. Target requires thread auto-settlement and not every connected selected environment has
   capability `threadAutoSettlement` → `SettingsScopeNotice target="environment"` with
   `eligibleEnvironmentIds` and text `"<title> requires a supporting environment. Choose one to continue."`
   (eligible list non-empty) or `"<title> requires a supporting environment. Connect or update an environment to continue."`.
2. Target scope not available → `SettingsScopeNotice` with `target` = the required scope when it
   is `environment`/`project`/`checkout`, else `all`, and text
   `"<title> is not available for the selected target. Choose its owning scope to continue."`.

Choosing a notice button navigates to the same page with that scope and the same hash, so the
target then lands (§5.4).

---

## 5. Settings scope (where a change is written)

### 5.1 Model (`cs/settingsScope.ts`, `cs/SettingsScopeContext.tsx`)

Two axes: the environment axis (`machine`: all, or one environment) and the project axis
(`project`: none = environment defaults, or one logical project; optionally `checkout`).
Device-local (client) preferences ignore scope.

Project groups (`cs/useSettingsProjectGroups.ts:9-24`): the same logical projects as the sidebar
(`buildSidebarProjectSnapshots` with the client grouping settings `sidebarProjectGroupingMode` +
`sidebarProjectGroupingOverrides`), sorted by `displayName` (`localeCompare`). Each group has
`projectKey`, `displayName`, favicon, and `memberProjects` (one per physical checkout:
`environmentId`, `id` (ProjectId), `physicalProjectKey`, `workspaceRoot`, `environmentLabel`).

`resolveSettingsScope(search, groups, environments)` (`settingsScope.ts:201-293`), in order:

| Condition | Result kind | Label / message |
| --- | --- | --- |
| `checkout` without `project` | unavailable `project-required` | "Select a project to choose one of its checkouts." |
| `machine` names no known environment | unavailable `environment-missing` | "This environment is no longer available." |
| `project` names no group | unavailable `project-missing` | "This project is no longer available." |
| project, but no member matches `machine`/`checkout` | unavailable `checkout-missing` | checkout set: "This checkout is no longer available in the selected project and environment." else "This project has no checkout on this environment." |
| project + checkout, checkout's environment unknown | unavailable `environment-missing` | "This checkout's environment is no longer available." |
| project + checkout | `checkout` | `"<project> / <env label>"` + `" · <workspaceRoot>"` when another checkout of the group shares that environment; environmentIds = [checkout env] |
| project | `project` | `"<project> / <env label or 'All checkouts'>"`; environmentIds = distinct member envs |
| machine | `environment` | env label; environmentIds = [it] |
| neither | `all` | "All environments"; environmentIds = every known environment |

Unavailable scopes have label "Unavailable selection", no members, no environments; they never
broaden a write.

Selection (`cs/scopedSettings.ts:52-70`): `environments` = known environments in the scope;
`connectedEnvironments` = those with `connection.phase == "connected"` and a server config;
representative `environment` = the primary environment if connected in scope, else the first
connected one, else null.

Targets (`scopedSettings.ts:87-130`): at project/checkout scope, one target per member whose
environment is connected (effective settings = `resolveProjectSettings(envSettings, member.id,
null, t3json)`); at all/environment scope, one per connected environment (its settings). The
representative `target` (display values) = the target on the representative environment, else the
first. `useScopedSettings()` = representative target settings (or `DEFAULT_SERVER_SETTINGS`)
merged with client settings, server keys winning (`cs/useScopedSettings.ts:30-41`,
`web/hooks/useSettings.ts:303-310`).

t3.json: for project/checkout scopes each member's `t3.json` (`T3_PROJECT_FILE_NAME`) is read via
the project files query (environment, workspaceRoot) and parsed; truncated or invalid → null. A
member whose read has not settled has no file tier yet (`SettingsScopeContext.tsx:20-55`).

Effective value resolution for a project-scoped key (`pkg/shared/src/projectSettings.ts:86-190`,
`cs/SettingInheritance.tsx:82-136`):

1. Project override in `ServerSettings.projectSettingsOverrides[projectId]` if the key is present
   and not `undefined`. A model override on a disabled provider is ignored.
2. Environment value (`ServerSettings[key]`).
3. For file-backed keys whose value is still null: `t3.json` field (`defaultThreadEnvMode`,
   `worktreeSubmodules`; `pkg/contracts/src/t3ProjectFile.ts:119-121`).
4. Built-in: file-backed `defaultThreadEnvMode` → `"local"`, `worktreeSubmodules` →
   `"recursive"`; otherwise `DEFAULT_SERVER_SETTINGS[key]`.

Project-scoped keys (`pkg/contracts/src/settings.ts:1040-1059`): `worktreeCleanup`,
`defaultModelSelection`, `defaultRuntimeMode`, `defaultThreadEnvMode`,
`newWorktreesStartFromOrigin`, `worktreeSubmodules`, `defaultAutoPull`, `defaultProjectScripts`,
`enableAgentBrowserAccess`, `enableAgentDeviceAccess`, `textGenerationModelSelection`,
`sourceControlWriterModelSelection`, `sourceControlWritingStyle`, `pullRequestMergeMethod`,
`sidebarAutoSettleOnMerge`, `sidebarAutoSettleAfterDays`, `continueThreadsAfterServerUpdate`,
`responseStreamingMode`. Nullable overrides (null is a real stored value):
`defaultModelSelection`, `sourceControlWriterModelSelection`, `pullRequestMergeMethod`,
`sidebarAutoSettleAfterDays` (`settings.ts:1095-1107`).

Mixed: the selected targets disagree (deep equality) on any of the row's keys
(`scopedSettings.ts:132-143`).

Source of a row's keys across targets (`scopedSettings.ts:148-162`): `"environment"` when no
scoped keys or no targets; `"mixed"` when targets disagree on source; else `project` > `t3.json` >
`environment`.

### 5.2 Scope sentence (`cs/SettingsScopeSentence.tsx:53-221`)

Rendered as the first child of every `SettingsPageContainer`, except on device-only pages (§5.3)
or outside the scope provider.

```
<p> flex, flex-wrap, items-center, column gap 6, row gap 4, px 16 (`sm:px-4`),
    `text-base` (16/24), `muted-foreground`
├ span (flex, gap 6, min-w 0): "Applying settings for" (shrink 0) + Project picker
└ span (flex, gap 6, min-w 0): "on" | "across" (shrink 0) + Environment picker
```

- Connective: "on" when `machine` is set or the scope is a checkout; otherwise "across".
- Each connective stays with its picker when wrapping.
- Picker trigger: `InlineButton tone="picker"` (inline text button: medium, `foreground`,
  dotted underline at `foreground/30`, offset 4 px, gap 6; hover and open → solid
  `foreground` underline), `min-w 0`, max-w 288 (`max-w-72`). Content: optional 14 px icon,
  truncating label, `ChevronDown` 14 px `muted-foreground`. `aria-label` = `"<Axis> scope: <label>"`
  with Axis "Project" / "Environment".
- Menu: `Menu` popup aligned start, radio group.

Project picker:

| Element | Content |
| --- | --- |
| trigger icon | the selected group's `ProjectFavicon` (14 px), none for all |
| trigger label | group `displayName`; "Unavailable project" if `project` is set but missing; else "All projects" |
| items | "All projects" (value `all`), separator, then every group: favicon 14 + name (truncate) + radio indicator |
| change | keep `machine`; set or drop `project`; always drop `checkout` (`cs/settingsScopeAxis.ts:342-348`) |

Environment picker:

| Element | Content |
| --- | --- |
| value | `machine`, else the checkout's environment for checkout scopes, else `all` |
| trigger icon | `EnvironmentMachineIcon` (14 px) of the selected environment (kind = `settings.environmentIcon` ?? `platform.machine` ?? `server`; `pkg/contracts/src/server.ts:650-657`) |
| trigger label | `settingsScopeEnvironmentLabel` (label, or `"<label> · <displayUrl or id>"` when another environment has the same label); "Unavailable environment" when the value names a missing env; "No environments" (single-environment pages) or "All environments" |
| items | (not on single-environment pages) `Layers` 14 + "All environments" + indicator, separator; then every known environment: machine icon 14 + label (truncate) + "Offline" (`text-xs muted-foreground`, when phase is not connected) + indicator |
| change | keep `project`; set or drop `machine`; drop `checkout` (`settingsScopeAxis.ts:332-340`) |

Machine icons (`web/components/EnvironmentMachineIcon.tsx`): server `Server`, cloud `Cloud`,
linux custom `LinuxIcon`, desktop `Monitor`, laptop `Laptop`, mac-mini custom (rect x2 y8 w20 h8
rx2 + LED `M6 12h.01`), mac-studio custom (rect x3 y5 w18 h14 rx2 + `M7 15h.01M11 15h.01M15 15h.01`).

A selection change navigates to the same path with `{project, machine, checkout}` (all three keys
sent), `hash: ""`, scroll not reset (`settings.tsx:154-170`). Push navigation. The body remounts
(§2.1).

### 5.3 Device-only and single-environment pages

- Device-only (`SETTINGS_DEVICE_ONLY_PATHS`, `SettingsScopeSentence.tsx:33-37`): Appearance,
  SnapShots, Connections. No scope sentence; `SettingsScopeBoundary` passes them through without
  "unavailable"/"reconnect" gating. `/settings/projects` also bypasses the boundary's gating
  (`settings.tsx:91-93`) and does its own (§5.5).
- Single environment: Providers (`singleEnvironment = pathname == "/settings/providers"`,
  `settings.tsx:156`). When `machine` is absent and the scope is not unavailable, the provider
  inserts `machine` = primary env if in scope, else the first connected env in scope, else the
  first env in scope (`settingsScopeAxis.ts:350-370`). The environment picker omits "All
  environments" and its empty label is "No environments". No environment →
  `"Reconnect <label> to set up its providers."` (environment scope) or
  `"Connect an environment to set up its providers."` as `<p>` with p 32, `text-sm`
  `muted-foreground` (`settings.providers.tsx:15-23`).

### 5.4 Scope notice (`cs/SettingsScopeNotice.tsx:12-89`)

Rendered inside `SettingsPageContainer` (so the scope sentence shows above it):

```
Alert role=status (rounded-xl 14, 1 px border, px 14, py 12, text-sm; default variant:
transparent bg, dark `input/32`)
└ AlertDescription
   ├ <p>{message}</p>
   └ AlertAction (flex-wrap): one Button size="sm-multiline" variant="outline"
       (max-w full, break-all, text-left) per choice
```

Choices by `target`:

| target | Buttons |
| --- | --- |
| `checkout` | every member of every group (only the selected group when `project` is set): `"<group> · <env label or 'Environment'> · <workspaceRoot>"` → `{project, machine, checkout}` |
| `project` | every group: `displayName` → `{project}` |
| `environment` | every environment (filtered to `eligibleEnvironmentIds` when given): label or `"<label> · <displayUrl or id>"` for duplicates → `{machine}` |
| `all` | one button "Open all environments" → `{}` |

Click: with a `targetId` (search landing) navigate to the same path with that search and
`hash = targetId`; otherwise `selectScope(search)` (§5.2).

### 5.5 Boundary states (`settings.tsx:94-110`, `cs/ProjectsSettings.tsx`)

For non-device-only pages other than Project, after the search-target checks (§4.4):

| Condition | Body |
| --- | --- |
| scope unavailable | `SettingsPageContainer` (scope sentence) + `<p class="text-sm text-muted-foreground">{scope.message}</p>` |
| environment scope and its environment is not connected | same container + "Reconnect <label> to change its settings." |
| otherwise | the page |

Project page: project/checkout scope (or unavailable `project-missing`/`checkout-missing` while
`project` is set) → `ProjectSettingsPanel` (it follows remembered members when a grouping change
replaces the key); other unavailable → the message; otherwise a project scope notice
"Choose a project to manage its name, icon, checkouts and actions." (`ProjectsSettings.tsx:17-34`).
Panel details: `settings-pages-projects-integrations.md`.

### 5.6 Writing scoped settings (`cs/scopedSettings.ts:212-305`, `cs/useScopedSettings.ts:68-121`)

`useUpdateScopedSettings()(patch)` is how scoped pages write. The patch is split by schema field
membership: `ClientSettingsSchema` keys → client patch; `ServerSettings` keys → server patch.

| Scope | Server writes |
| --- | --- |
| all / environment | the server patch to every connected environment in scope (`server.updateSettings {patch}`), each separately. Special case: when the environment has a non-null `worktreeCleanup` and the patch touches `storageCleanup.worktree*`, also write `worktreeCleanup = {mode:"custom", rules: resolved rules + patched worktree fields}`. |
| project / checkout | only if every server key is project-scoped; one write per member environment (connected, capability `projectSettingsOverrides == true`): `{ projectSettingsOverrides: { [projectId]: nextEntry } }` where `nextEntry` = current override entry with each patched key set. Object values merge onto the member's effective value (partial patches). `worktreeCleanup` with `mode:"custom"` merges rules onto resolved rules. A `null` for a non-nullable override key deletes the key ("Inherit"). Members on one environment merge into one write. |
| unavailable | none |

Client patch → `persistClientSettingsPatch` (§5.8) regardless of scope.

When nothing can be written, a warning toast, title "Setting not saved", description:

| Case | Description |
| --- | --- |
| scope unavailable | the scope message |
| project scope with a non-project-scoped server key | "This setting is environment-wide and cannot be overridden by a project." |
| project scope, no eligible member env | "Connect the selected checkouts, or update their environments, to save a project override." |
| environment scope, none connected | "Connect <label> to save this setting." |
| all scope, none connected | "Connect an environment to save this setting." |

Writes run in parallel; afterwards, if any failed (`Failure` result or rejected): error toast,
title "Setting saved on some environments" (some saved) or "Setting not saved", description
`"Could not update <labels joined ", ">."` + `" The other selected environments saved the change."`
when some saved. The server responds with the full `ServerSettings`; the UI updates from the
config stream / optimistic atom.

Reset override (`useClearScopedSettings`, `planScopedSettingsClear`): at project/checkout scope,
write each member's entry with the keys removed (`null` entry when empty). No eligible member →
warning "Setting not saved" / "Connect the selected checkouts, or update their environments, to
reset this override.".

Clear specific project overrides from an environment-scope chain popover
(`useClearProjectOverrides`, `planProjectOverridesClear`): one write per environment; none
connected → "Connect the environments to reset these overrides.".

### 5.7 Unscoped writes (`web/hooks/useSettings.ts:421-506`)

Rows outside the scope provider (dialogs, provider cards) and some global rows use
`useUpdatePrimarySettings` / `useUpdateEnvironmentSettings(id)`:

- Split by `ServerSettings` field membership; the rest goes to the client store.
- Shared server keys (`pkg/client-runtime/src/state/sharedSettings.ts:24-31`):
  `continueThreadsAfterServerUpdate`, `sidebarAutoSettleAfterDays`, `sidebarAutoSettleOnMerge`,
  `newWorktreesStartFromOrigin`, `sourceControlWritingStyle`, `textGenerationModelSelection`. They
  are written to the target environment and to every environment eligible for shared sync
  (connected with capability `threadAutoSettlement`), filtered per target: drop
  `textGenerationModelSelection` on a non-source target whose provider instance driver differs or
  whose provider is disabled; drop `continueThreadsAfterServerUpdate` where capability
  `threadRestartContinuation` is not true. Nothing written → warning toast "Setting not saved" /
  "Update older servers to save this setting." (targets existed) or the primary-unavailable
  message.
- Other server keys → the target environment only. No target → warning "Setting not saved" with
  `PRIMARY_SETTINGS_UNAVAILABLE_MESSAGE` = "This setting is saved on a server, and the hosted app
  is not anchored to one. Change it from the desktop app or from the server's own address."
  (never on desktop).
- RPC failures on this path report through `useAtomCommand(..., "server settings update")`
  (generic failure toast; design-system/toast spec).

### 5.8 Client settings persistence

- Desktop: `desktopBridge.getClientSettings()` / `setClientSettings(settings)` IPC
  (`web/localApi.ts:58-70`, `desk/ipc/methods/clientSettings.ts`), stored as
  `<stateDir>/client-settings.json` (`desk/app/DesktopEnvironment.ts:214`). The file decodes
  leniently: either the bare object or `{ settings: {...} }`; invalid documents fail rather than
  silently resetting (`desk/settings/DesktopClientSettings.ts:15-26`). Writes are atomic (temp
  file + replace). `setClientSettings` also reconfigures SnapShot.
- The in-memory snapshot starts at `DEFAULT_CLIENT_SETTINGS`; hydration merges the file over
  defaults. Patches made before hydration are deferred and applied in order after it. Every write
  persists the whole snapshot (queued, serialized) (`web/hooks/useSettings.ts:116-230`).
- `mergeEnvironmentSettings`: `{...client, ...server}` (server keys win).
- Native: `ClientSettings` must round-trip unknown keys (the current `t3-logic` struct drops them
  on write; add `#[serde(flatten)] other: Map<String, Value>`) and add every fe7d3092c field
  (key list: `pkg/contracts/src/settings.ts:298-515`).

### 5.9 Server settings source

- Read: `ServerConfig.settings` from `subscribeServerConfig` (snapshot + `settingsUpdated`
  events) per environment (`pkg/contracts/src/rpc.ts:1400-1420`); `server.getSettings` exists but
  the UI uses the config stream.
- Write: `server.updateSettings` payload `{ patch: ServerSettingsPatch }` → `ServerSettings`
  (`rpc.ts:635-639`; patch schema `pkg/contracts/src/settings.ts:1482-1611`). Errors:
  `ServerSettingsError`, `EnvironmentAuthorizationError`.
- Capabilities used by the framework (`ServerConfig.environment.capabilities`):
  `projectSettingsOverrides`, `threadAutoSettlement`, `threadRestartContinuation`,
  `pullRequests`.

---

## 6. Layout primitives (`cs/settingsLayout.tsx` and helpers)

### 6.1 `SettingsSection` (`settingsLayout.tsx:170-218`)

Props: `title`, `hideTitle`, `icon`, `headerAction`, `variant` ("grouped" | "plain"), `id`.

```
<section id tabIndex=-1 when id>  (space-y 10 unless hideTitle)
├ header div [data-settings-scroll-target]: flex, min-h 28, items-start, justify-between,
│   gap 16, px 16
│   ├ div min-w 0 → h2: flex, min-h 28, items-center, gap 8, `text-sm` font-normal,
│   │                `foreground/70`: {icon}{title}
│   └ div: flex, min-h 28, min-w 28, items-center, justify-end: {headerAction}
│   (hideTitle: an sr-only h2 instead; the group becomes the scroll target)
└ SettingsGroup variant
```

Section titles have no descriptions (`settingsLayout.tsx:170`).

### 6.2 `SettingsGroup` (`cs/SettingsGroup.tsx`)

- grouped: relative, overflow visible, `foreground` text, `rounded-xl` (14), 1 px
  `border/60`, bg `card/40`, `shadow-xs` at 5% (`0 1px 2px` black 5%). When `divided` (default):
  every child after the first has a 1 px top border `border/50`, and direct `SettingsRow`
  children lose their own radius.
- plain: `space-y-1` (4 px between children), no card.

### 6.3 `SettingsRow` (`settingsLayout.tsx:240-472`)

Props: `id`, `title`, `description`, `status`, `control`, `children`, `resetAction`,
`onResetOverride`, `serverScoped` (value lives in an environment's settings.json), `settingKeys`
(server keys it edits), `mixed` (override).

```
div[data-slot=settings-row] id tabIndex=-1 when id; container query name "settings-row"
  rounded-xl, px 16, py 12 (with children: pt 12 pb 4)
  aria-disabled → opacity .64 and all descendants `muted-foreground`
├ grid (row width >= 512 px: columns [minmax(0,1fr) minmax(160px,auto)], items-center, gap 32;
│       narrower: flex col gap 12)
│ ├ text column (min-w 0, flex-1, space-y 4)
│ │  ├ title line: flex, min-h 20, items-center, gap 6
│ │  │   h3 `text-sm` medium `foreground` {title}
│ │  │   [20x20 slot] inheritance indicator (§6.6), only when in a scope context,
│ │  │                serverScoped, and settingKeys non-empty
│ │  │   [20x20 slot] reset button (always reserved, may be empty)
│ │  ├ description: <p> max-w 576 (`max-w-xl`), `text-xs` leading 1.5 (18 px),
│ │  │              `muted-foreground/80`
│ │  └ status: div pt 2, `text-xs` `muted-foreground`
│ └ control: flex, gap 8, items-center, w-full (wide: w-auto, justify-end), min-w 0, shrink 0
└ children (full width under the grid)
```

On desktop the row is always >= 512 px wide (container max 848 px minus card padding), so the
two-column grid applies; the narrow layout only appears in very narrow windows.

Control sizing tiers (`settingsLayout.tsx:249-253`): `control` slot uses `size="sm"` (Button,
Select, Input, NumberField) or `icon-sm`; section `headerAction`s and buttons inside list
items/cards use `xs`/`icon-xs`; inline affordances (reset, info) use `icon-micro`. Dialog footers
use the default size.

Derived states:

| State | Condition | Effect |
| --- | --- | --- |
| project scope | scope kind project/checkout | |
| environment-wide (inert) | project scope, `serverScoped`, no project-scoped key in `settingKeys` | control rendered inert at opacity .5 inside a focusable span (rounded-md, focus ring 2 px `ring`) with tooltip (top) "Environment-wide setting. Select an environment to change it." |
| unavailable (inert) | `serverScoped` and no connected environment in scope (outside a scope: no primary settings) | control inert + tooltip "Reconnect the selected environment to change this setting." (outside scope: `PRIMARY_SETTINGS_UNAVAILABLE_MESSAGE`); `children` inert at .5; reset hidden |
| mixed | targets disagree on `settingKeys` (or `mixed` prop) | control keeps working; switches show the mixed state (§6.8); selects show "Mixed" placeholder (page specs) |
| reset slot | project scope with scoped keys: `SettingResetButton` tooltip "Reset to inherited value" when source is `project` or `mixed`, click → `onResetOverride` or clear overrides for the scoped keys; otherwise the page's `resetAction` | |

Inheritance summary (tooltip + aria; `settingsLayout.tsx:398-408`), first match:

| State | Summary |
| --- | --- |
| mixed | "Mixed across selected environments" |
| source project | "Overridden for this project" |
| source t3.json | "Inherited from the repository's t3.json" |
| source environment, has scoped keys | `"Inherited from <env label>"` when the scope has exactly one environment, else "Inherited from environment" |
| any target's environment value differs from the built-in default | "Set on the environment" (state `environment`) |
| else | "Built-in default" (state `default`) |

At environment/all scope, projects that override the row's scoped keys are listed in the chain
popover (§6.6) with " · N project override(s)" appended to the summary.

### 6.4 `SettingResetButton` (`settingsLayout.tsx:474-506`)

`Button size="icon-micro" variant="ghost-muted"`, icon `Undo2` 12 px,
`aria-label="Reset <label> to default"`, tooltip (top) default "Reset to default". Click stops
propagation then calls `onClick`. Pages show it only when the value differs from its default.

### 6.5 `SettingsUnavailableGroup` (`settingsLayout.tsx:220-238`)

When `message` is given: wrapper `border-border/60 bg-muted/20 py-6px`; a notice row (flex,
items-start, gap 8, px 16, py 8, `text-xs` leading-relaxed `muted-foreground`) with `Info` 14 px
`warning` icon (mt 2) and the message; then the children with every `h3` and `p` at opacity .64.
Without a message it renders the children unchanged.

### 6.6 `SettingInheritance` (chain popover, `cs/SettingInheritance.tsx:162-316`)

- Trigger: `Button size="icon-micro" variant="ghost-muted"` with `Layers` 12 px; icon color
  `primary` when overridden, `warning` when mixed, else the button's muted color.
  `aria-label="<summary>. Show where this value comes from"`. Tooltip (top) = summary.
- Popover: align start, width 320 (`md` = `w-80`), no padding, sections divided by
  `border/60`. One section per target (px 12, py 10):
  - Header h4: flex, gap 6, pb 6, `text-xs` medium `muted-foreground`: machine icon 14 + target
    label (environment label).
  - `<ol>` `text-sm`, one row per layer: grid `[minmax(0,1fr) auto]`, gap-x 12, `rounded-md`,
    px 8, py 4; effective layer bg `foreground/6%`.
    - Left: layer name ("Project", "Environment", "t3.json", "Default"); effective → medium
      `foreground`, else `muted-foreground`.
    - Right (flex, gap 6, tabular-nums): value (max-w 128, truncate) + `Check` 14 `primary` when
      effective (else a 14 px spacer). Color: effective `foreground`; set `muted-foreground`;
      unset `muted-foreground/60`.
  - Layers in order: Project (only for project targets on scoped keys; value or "Inherits"),
    Environment (value if it differs from the default, else "Inherits"), t3.json (file-backed keys
    on project targets), Default (built-in).
  - "Overridden by" block (environment scopes, when projects on this environment override): mt 8,
    border-top `border/60`, pt 8. Header row: px 8, `text-xs` `muted-foreground`, "Overridden by"
    + `InlineButton` "Reset it" / "Reset all" (clears those projects' overrides for the scoped
    keys). Then one row per project: `InlineButton` with the group name (opens that project's
    scope keeping `machine`) and its override value (max-w 128, truncate, `muted-foreground`).
- Value labels (`SettingInheritance.tsx:38-80`): null/undefined → `pullRequestMergeMethod`
  "Last selected", `sidebarAutoSettleAfterDays` "Never", `defaultModelSelection` "Automatic",
  `sourceControlWriterModelSelection` "Text generation model", `defaultThreadEnvMode` /
  `worktreeSubmodules` "Inherit", else "Not set". Booleans "On"/"Off". Days → `"N day"`/`"N days"`.
  Env mode → its label (Local / Worktree; `BranchToolbar.logic`). Submodules →
  `WORKTREE_SUBMODULES_LABELS`. Merge method → `PULL_REQUEST_MERGE_METHOD_LABELS`. `""` →
  "Empty". Arrays → `"N item(s)"`. Objects with `model` → the model id; with `mode` → writing
  style ("Repository conventions", "Conventional Commits", "Custom instructions") or the raw mode.
  Else "Custom".

### 6.7 `PolicyTooltip` (`settingsLayout.tsx:143-158`)

`Button size="icon-micro" variant="ghost-muted"` with `Info` 14 px,
`aria-label="Background policy details"`, tooltip (top, open delay 200 ms) with the given text.

### 6.8 `ScopedSwitch` (`cs/ScopedSwitch.tsx`)

`Switch` with `mixed` = targets disagree on `settingKeys`. Mixed renders the switch's mixed
visual with `checked=false`; clicking turns it on everywhere (macOS mixed-checkbox convention).
Switch visuals: design-system.

### 6.9 `FoldedSettingsSection` (`cs/FoldedSettingsSection.tsx`)

A section that starts closed. A settings-search landing on its `id` opens it before scrolling.

- `headerPlacement="inside"` (default): `<section id tabIndex=-1>` → a `SettingsGroup`
  (not divided) as the collapsible root.
  - Header: flex, items-center, gap 16, px 16. Trigger (flex-1, min-h 44, py 8, gap 8,
    text-left, `rounded-md`, focus ring 2 px): `ChevronRight` 16 `muted-foreground` (rotates 90°
    when open, `transform 150ms`, none with reduced motion), title `text-sm` medium, summary
    (`text-xs` `muted-foreground`, truncate) when given. Optional `control` at the right.
  - Panel: border-top `border/50`, children divided by `border/50`.
- `headerPlacement="outside"`: the section header looks like `SettingsSection`'s (min-h 28, px 16,
  `text-sm` `foreground/70`) with the chevron (16 px, after the title) inside the trigger;
  `control` at the right; the panel is a normal divided `SettingsGroup`, space-y 10.
- Panel open/close animation: `CollapsiblePanel` (design-system).

### 6.10 Other helpers

- `SettingsSearchTarget` (`settingsLayout.tsx:128-138`): a `div` that accepts a search landing
  (for anchors that are not rows or sections).
- `ITEM_ROW_CLASSNAME` (`cs/itemRows.ts`): direct list rows inside a grouped section:
  first/last rows `rounded-t-xl`/`rounded-b-xl`, px 16, py 12. `ITEM_ROW_INNER_CLASSNAME`: flex
  row, items-center, justify-between, gap 12.
- `ExpandableText` (`cs/ExpandableText.tsx`): pre-wrap, break-words; when text > 180 chars or
  contains a newline it clamps to 3 lines with an `InlineButton tone="muted"` (mt 4) "Show full
  error" / "Show less" (label overridable).
- `RedactedSensitiveText` (`cs/RedactedSensitiveText.tsx`): renders trimmed value as a button
  `font-mono text-2xs`, `muted-foreground`, hover `foreground`; hidden state shows a deterministic
  scramble of the same length (FNV-1a seed, alphabet `abcdefghjkmnpqrstuvwxyz23456789`, keeping
  `@ . - _`) with `blur-xs` (4 px) and no selection; click toggles. Tooltip (top) = reveal/hide
  text from props. Empty value renders nothing.
- `useRelativeTimeTick(intervalMs = 1000)`: re-renders every interval for relative-time labels.
  Native: a 1 s timer only while such a label is visible.
- `SETTINGS_PICKER_TRIGGER_CLASSNAME` = `min-w-0 max-w-none shrink-0`: composer model/traits
  pickers placed in a settings control slot drop their composer max-width.

### 6.11 Confirm dialogs (`localApi.dialogs.confirm`)

Pages call `readLocalApi().dialogs.confirm(message, { variant })` for destructive confirms. In
fe7d3092c this is an in-app `AlertDialog` (`ConfirmDialogHost`), not a native OS dialog: the first
line ending in `?` is the title, the remaining lines (pre-line) the description, buttons "Cancel"
and "Confirm" (requested variant). Full spec: `shell.md` §6.3. Page specs quote the message lines.

---

## 7. Data

| Element | Source (fork) | t3UI today |
| --- | --- | --- |
| environments list, label, displayUrl, connection phase, serverConfig | `useEnvironments()` (`web/state/environments`) | `AppState::environments()`, `Environment::{label, status, config}` (main) |
| primary environment | `usePrimaryEnvironmentId()` | `AppState::primary_environment()` |
| project groups for scope | `useSettingsProjectGroups` (sidebar snapshots) | `t3_logic::sidebar::{build_sidebar, logical_project_key, physical_project_key}`; needs a "groups sorted by display name with members" view |
| per-member `t3.json` | project files query `projects.readFile`-style RPC for (env, workspaceRoot, "t3.json") | check `t3_protocol::projects`; parse `T3ProjectFile` (`pkg/contracts/src/t3ProjectFile.ts`) — missing |
| server settings read | `subscribeServerConfig` snapshot + settings events | `t3_client::Environment::config()` watch → `ServerConfig.settings` (typed subset + `other`) |
| server settings write | `server.updateSettings {patch}` → `ServerSettings` | `t3_protocol::methods::ServerUpdateSettings`; `ServerSettingsPatch` typed subset + `other` map (fits; add `projectSettingsOverrides` typing as needed) |
| capabilities `projectSettingsOverrides`, `threadAutoSettlement`, `threadRestartContinuation`, `pullRequests` | `ServerConfig.environment.capabilities` | verify fields exist in `t3_protocol::server` — add if missing |
| `platform.os`, `platform.machine`, `settings.environmentIcon` | `ServerConfig.environment.platform`, settings | `ServerSettings.environment_icon` exists; platform fields: verify |
| client settings | `client-settings.json` via desktop IPC | `t3_logic::settings::ClientSettings` + `AppState::{settings, update_settings}` (missing fields, drops unknown keys) |
| T3 Connect public config (search `cloudOnly`) | env vars | `t3_client::cloud` config (connections spec) |
| local environment enabled, WSL state | desktop bridge | native app owns these (connections spec) |
| session scopes (`AuthAccessWriteScope`) | primary session state | `t3_client::auth` session (verify scope list is exposed) |

---

## 8. Reuse map

| t3UI module | Verdict |
| --- | --- |
| `crates/t3-app/src/state/route.rs` + `AppState::settings_project` (main, 2976a48: `Route::Project(key)` → `Settings(Projects)` and remembers the key) | needs changes: replace the side field with a scope search on the route (`project`, `machine`, `checkout`), plus target anchor + highlight flag and providers `instance_id`; retain-scope rule on page switches; `/projects/<key>` sets `project` and clears `machine`. |
| `crates/t3-app/src/state/mod.rs` (`navigate`, `replace_route`, `go_back`) | fits; add `last_main_route` for Back/Escape (§1.4). Settings nav clicks and search landings use `replace_route`; scope changes use `navigate`. |
| `crates/t3-logic/src/settings.rs` (`ClientSettings`) | needs changes: add every fe7d3092c client key; keep unknown keys on write; defaults from `DEFAULT_CLIENT_SETTINGS`. |
| `crates/t3-logic/src/sidebar/` grouping + keys | fits for scope groups (same logical projects as the sidebar); add a settings-facing projection sorted by display name with members. |
| `crates/t3-protocol/src/server.rs` (`ServerSettings`, `ServerSettingsPatch`, `UpdateSettingsInput`) | fits (typed subset + `other`); add typed `project_settings_overrides` for scope resolution; keep tolerant decoding. |
| `origin/settings`: `settings/mod.rs` (`SettingsView`) | needs changes: header copy is "Restore device defaults" (ghost xs), breadcrumb `Settings / <Section>` instead of a "Settings" label; rebuild pages on page/scope change instead of caching. |
| `origin/settings`: `settings/nav.rs` (`SettingsNav`) | July-era visuals: replace. Real nav: 32 px rows, `text-sm` medium, hover/selected fills (§3.4), search row + results (§3.3, §3.5), Project item conditional, footer with T3 Connect + Back + bell + update pill. |
| `origin/settings`: `settings/pages.rs` (registry) | fits; its order matches `SETTINGS_SECTION_LABELS`. Replace stand-in icons (Palette, PanelsTopLeft, custom SnapShot, Blocks, HardDrive) and gate Project on scope. |
| `origin/settings`: `settings/layout.rs` (`page`, `SettingsSection`, `SettingsRow`, `reset_button`) | needs changes: `PAGE_MAX_WIDTH` 768 → 896 (readable), 1024 wide, 1152 expanded; padding 24/24/48, gap 32; section header `text-sm` normal `foreground/70` min-h 28; group `rounded-xl` 14 + `border/60` + `card/40`; row grid 2-col with 32 gap, 20 px inheritance + reset slots; add scope sentence, inheritance popover, inert states, folded section, search-target pulse. |
| `origin/settings`: `settings/server.rs` (`update_server_settings` to primary only) | needs changes: route through the scope planner (§5.6) and the shared-key fan-out (§5.7); toast copy per §5.6. Keep the "config stream re-renders" approach. |
| `crates/t3-app/src/keybindings` | add Escape-leaves-settings and `/`-focuses-search at the settings view level; `Cmd+,` menu item → `Route::Settings(General)`. |
| `crates/t3-ui` components (Button, Switch, Select, Menu, Popover, Tooltip, Kbd) | per design-system refresh. |

---

## 9. Reference screenshots needed

All at 1440x900 @2x, dark and light unless noted. Seed: `e2e/seed.mjs` default (one local env
with 2+ projects and threads) unless noted.

| Name | How to reach | Seed / notes |
| --- | --- | --- |
| `settings-general-top` | Sidebar gear → General, scroll top | default; shows scope sentence "Applying settings for All projects across All environments", breadcrumb, "Restore device defaults" disabled |
| `settings-nav-hover` | General, hover "Appearance" nav item | default |
| `settings-nav-project-item` | Scope sentence → pick a project; nav shows "Project" first | 2+ projects |
| `settings-search-results` | Press `/`, type "font" | default; results with section subtitles, first active |
| `settings-search-empty` | Type "zzzz" | "No settings found" |
| `settings-search-landing-pulse` | Search "word wrap", Enter; capture ~300 ms after landing | Appearance; ring pulse on the row |
| `settings-scope-project-menu` | Click the "All projects" picker | 2+ projects |
| `settings-scope-environment-menu` | Click the "All environments" picker | 2 environments, one offline (shows "Offline") |
| `settings-scope-project-selected` | Pick a project; General | rows show inheritance (Layers) icons |
| `settings-inheritance-popover` | Project scope, click a row's Layers icon | a project override set on Default model |
| `settings-row-environment-wide-inert` | Project scope, hover an environment-wide row's control | tooltip "Environment-wide setting. Select an environment to change it." |
| `settings-scope-notice-search` | All scope, search "Actions" (Project page target) | notice with project buttons |
| `settings-environment-reconnect` | Pick an offline environment in the picker on General | "Reconnect <label> to change its settings." |
| `settings-providers-single-env` | Providers page; environment picker shows no "All environments" | default |
| `settings-collapsed-sidebar-header` | Collapse sidebar on General | header padding 122 px (macOS windowed) |
| `settings-footer` | Any page, bottom of sidebar | "Back" + bell (+ T3 Connect card when configured) |

---

## 10. Open questions / risks

1. **Scope model is the largest new piece.** Every server-backed row needs targets, mixed
   detection, sources, and the inheritance popover. Build it once in `t3-logic` (pure, testable
   with recorded `ServerConfig`s) before pages. Failure modes to test first: unavailable scopes
   never write; project scopes refuse non-scoped keys; null "Inherit" deletes non-nullable
   overrides; partial object patches merge onto the member's effective value; shared keys fan out
   with the two filters.
2. **t3.json tier** needs a project file read RPC per member and `T3ProjectFile` parsing; until it
   exists, file-backed keys show only Project/Environment/Default layers (the fork does the same
   while the read is pending).
3. **Body remount on scope change** loses page-local state (drafts, open cards). Matches the fork;
   do not "improve" it.
4. **Escape conflicts:** GPUI dispatch order must let menus, popovers, dialogs, the search box,
   and key recorders consume Escape before the settings-level handler.
5. **`/` shortcut** must be ignored while a text field, dialog, or popup has focus.
6. **Search registry drift:** titles/ids are shared with page rows through `searchableSetting`.
   Mirror that in Rust (one const table used by both search and row builders).
7. `topbar-scroll-fade` is a CSS mask; GPUI has no mask. A gradient overlay in the page background
   color is the closest static match; glass themes may show a seam. Compare against screenshots.
8. `desktopOnly` / Electron-only availability flags: the native app is the desktop app, so treat
   `isElectron` as true. `localBackendManagementOnly`/`localEnvironmentOnly`/`wslAvailableOnly`
   depend on the native app's own local-backend management, which may not exist at first; hide
   those results until it does.
