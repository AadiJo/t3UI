# App shell, window, keybindings, menus, toasts, dialogs, persistence: build spec

Refreshed against fe7d3092c (`~/L-Projects/t3UI-refs/t3code-fork`). The sidebar itself is
`sidebar.md`. Settings, Pull Requests, Usage, Welcome, command palette, chat, connections are other
specs; this file owns how they are mounted and navigated to.

Path prefixes: `web/` = `apps/web/src/`, `desk/` = `apps/desktop/src/`, `cr/` =
`packages/client-runtime/src/`, `contracts/` = `packages/contracts/src/`, `shared/` =
`packages/shared/src/`.

Conventions: as `sidebar.md` (desktop >= 840px wide, so `sm:`/`md:` apply; radii `rounded-sm` 6,
`rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18; `--control-radius` 8; colors are
token names; at the default appearance contrast every `--contrast-X` equals `--X`,
`web/index.css:82-83,1516-1628`). Strings verbatim.

---

## 1. Root, routes, auth gate

### 1.1 Provider tree (`web/AppRoot.tsx:14-23`, `web/routes/__root.tsx:158-276`)

```
AppAtomRegistryProvider
├ RouterProvider (hash history on desktop: web/main.tsx:20-23)
│ └ root route  beforeLoad -> authGateState (__root.tsx:104-134)
│   ├ /pair, /connect                    DocumentTitleSync + Outlet only
│   ├ not authenticated (and not hosted) DocumentTitleSync + Outlet only
│   └ authenticated:
│     ToastProvider > AnchoredToastProvider
│       DocumentTitleSync, ContrastAppearanceSync, EnvironmentThemeSync, GlassAppearanceSync,
│       FontAppearanceSync, ProviderAuthCallbackCoordinator, ChatGptWelcomeCoordinator
│       FirstRunGate  (renders nothing until the first-run decision; fresh installs -> /welcome)
│         AuthenticatedTracingBootstrap, DesktopAppActivationCoordinator, RunningThreadKeepAlive,
│         RelayClientInstallDialog, ConnectOnboardingDialog, SshPasswordPromptDialog,
│         SnapShotCoordinator, ThreadNotificationCoordinator, QueuedMessageSender,
│         ConfirmDialogHost (6.3), CustomSnoozeDialogHost (sidebar.md 6.3),
│         SlowRpcRequestToastCoordinator, ProjectCloneToastCoordinator,
│         HostedStaticEnvironmentBootstrap, ThreadCompletionNotificationCoordinator,
│         DesktopNotificationCoordinator, EventRouter (8.1), PlanAgentSelectionHeal,
│         ProviderUpdateLaunchNotification
│         CommandPalette > AppSidebarLayout (2) > Outlet
│         ThemeEditorHost
├ PreviewAutomationHosts, ElectronBrowserHost   (preview spec)
└ QuitHoldOverlay                               (3.3)
```

`/welcome` renders the same shell (palette + sidebar layout) without FirstRunGate and most
coordinators (`__root.tsx:191-210`). Appearance sync components write CSS vars from client
settings (contrast, chat width, diff colors, glass opacity, fonts); design-system spec.

### 1.2 Routes

| Path | File | Under `/_chat` layout | Renders |
| --- | --- | --- | --- |
| `/` | `web/routes/_chat.index.tsx` | yes | Once every environment shell has bootstrapped: if any project exists, immediately opens a new draft (replace) in the most recently active project (`sortScopedProjectsForSidebar(..., "updated_at")[0]`); draft start failure shows "Couldn’t start a new thread" / "The project is still available. Try opening the draft again." / "Try again". No projects: `NoProjectsHero` (pages spec). Before bootstrap: nothing. (`:113-200`) |
| `/$environmentId/$threadId` | `_chat.$environmentId.$threadId.tsx` | yes | `ThreadRouteView` (chat spec), mounted by the layout so draft promotion keeps one ChatView (`web/routes/_chat.tsx:209-222`) |
| `/draft/$draftId` | `_chat.draft.$draftId.tsx` | yes | same |
| `/pull-requests` | `_chat.pull-requests.tsx` | yes | PR inbox (pull-requests spec); search params = list filters |
| `/usage` | `usage.tsx` | no | Usage page (usage spec) |
| `/settings` | `settings.tsx` | no | redirect (replace) to `/settings/general` |
| `/settings/{general,appearance,keybindings,providers,source-control,connections,integrations,projects,archived,storage,diagnostics,snap-shot,open-source-licenses}` | `settings.*.tsx` | no | settings spec |
| `/projects/$projectKey` | `projects.$projectKey.tsx` | no | redirect (replace) to `/settings/projects?project=<key>` |
| `/welcome` | `welcome.tsx` | no | first-run wizard (pages spec) |
| `/pair` | `pair.tsx` | no | pairing (connections spec) |
| `/connect` | `connect.tsx` | no | CLI authorize surface (connections spec) |

- `_chat` `beforeLoad`: not authenticated -> replace `/pair` (`web/routes/_chat.tsx:224-233`).
- The `/_chat` layout mounts `ChatRouteGlobalShortcuts` (4.4), so those shortcuts do not run on
  `/usage` or settings.
- Not found (`__root.tsx:143-156`): centered `max-w-sm` column, gap 16: "Page not found" (text-lg
  medium), "This link doesn't point to a page in T3 Code. Go home to choose a project or start a
  thread." (text-sm `--muted-foreground`; the app name is the display name), button "Go home"
  (replace `/`).
- Route error (`__root.tsx:400-496`): standalone error page: eyebrow app name, title "Something went
  wrong.", description = error message (fallback "An unexpected router error occurred."), buttons
  "Try again" (re-run loaders), "Reload app", "Copy error" / "Copied" (copies a report: `<app>
  <version>`, `Path: <pathname>`, `Time: <ISO>`, blank, stack, up to 5 `Caused by:` blocks); report
  box: header "Error report" (text-xs medium muted), `pre` max-h 256 text-xs.
- History: in-memory back/forward stack (hash history). A relaunch always starts at `/`; no route
  is persisted (7.3).
- Document title (`__root.tsx:355-370`): display name, `T3 Code (Nightly)` when the primary
  server version is nightly. Electron ignores it for the native title (3.1).

### 1.3 Utility pages and Back

`isSidebarUtilityPage`: `/settings`, `/settings/*`, `/projects/*`, `/usage`, `/pull-requests`
(`web/components/sidebar/mainAppLocation.ts:7-15`). `MainAppLocationTracker` (mounted in
`AppSidebarLayout`) records the href of every non-utility location in a module variable; the
sidebar's Back button navigates (push) to it or `/` (sidebar.md 9.1). Menu "Settings..." and the
footer Settings button push `/settings`.

---

## 2. Window and workspace layout

### 2.1 Electron window (`desk/window/DesktopWindow.ts`)

| | macOS (target) | Windows/Linux |
| --- | --- | --- |
| Size | saved bounds if fully on one display, else 1100x780 (`:187-198,392`) | same |
| Min size | 840x620 (`:399-400`) | same |
| Title | `T3 Code`, `T3 Code (Nightly)`, `T3 Code (Dev)` (display name; page titles are blocked) (`:406,669-672,746`) | same |
| Chrome | `titleBarStyle: "hiddenInset"`, traffic lights at `{x:16, y:19}` = `{16, round(52*zoom/2 - 7)}` (`:37-49,258-264`) | `hidden` + `titleBarOverlay` h 40, color `#01000000`, symbols `#f8fafc` dark / `#1f2937` light |
| Background | `#0a0a0a` dark, `#ffffff` light, follows `nativeTheme` (`:157-159,278-294`) | same |
| Vibrancy | none (no vibrancy or transparency anywhere) | none |
| Show | hidden until `ready-to-show`, then maximize if saved maximized, show, focus (`:813-829`) | same |

- Bounds persist in `<stateDir>/desktop-settings.json` (7.1): debounced 500ms on
  resize/move/maximize/unmaximize, flushed on close; maximized/fullscreen saves normal bounds.
- Close on macOS destroys the window; the app stays running; dock click reopens
  (`desk/app/DesktopLifecycle.ts:235-254`). One main window; no "New Window".
- Zoom: View menu Actual Size / Zoom In / Zoom Out change Chromium zoom by 0.5 levels (not
  persisted) and re-position the traffic lights (`:1007-1023`). Native: optional UI scale; if
  implemented, keep traffic lights vertically centered in the 52px topbar.
- Cmd+W key auto-repeat is swallowed (`:657-664`).
- Fullscreen (macOS): the renderer learns via `desktop:window-fullscreen-state` and drops the
  traffic-light inset (2.2).
- Links: new windows are denied; http(s) and safe `vscode://`/`zed://` remote links open in the
  system browser (`:605-625`, `desk/shell/ElectronShell.ts:17-75`).

### 2.2 Layout variables (`web/index.css:84-127`, `web/components/AppSidebarLayout.tsx:58,243-257`)

| Variable | macOS windowed | macOS fullscreen |
| --- | --- | --- |
| `--workspace-topbar-height` | 52 | 52 |
| `--workspace-controls-top` | 0 | 0 |
| `--workspace-controls-left` | `--desktop-window-controls-inset` = `70/zoom + 12` = **82** | 12 |
| `--workspace-controls-right` | 12 | 12 |
| `--workspace-titlebar-control-size` | 28 | 28 |
| `--workspace-titlebar-control-gap` | 12 | 12 |
| `--workspace-titlebar-content-left` = left + 28 + 12 | **122** | 52 |
| `--workspace-titlebar-items-lift-padding` | 0 | 0 |
| `--workspace-gutter` (page headers) | 20 (`sm`) | 20 |
| `--sidebar-width` | 256 default (sidebar.md 1.1) | |
| `--panel-animation-duration` | `panelAnimationDurationMs` (default 0) | |

`--desktop-window-controls-inset` is written by the preload on DOMContentLoaded and resize
(`desk/preload.ts:37-48`): native buttons end at 70pt regardless of zoom.

### 2.3 Box tree (`web/components/AppSidebarLayout.tsx:296-339`, `web/components/ui/sidebar.tsx:93-180`)

```
SidebarProvider  flex row, w 100%, h 100vh, data-sidebar-state=expanded|collapsed,
│                data-panel-animations=true|false, style vars above
├ ProjectProjectionRetention (keeps the project list subscribed while settings hides the sidebar)
├ Sidebar column  (sidebar.md 1)    fixed, left 0, z 10, border-right 1px
├ route view      the route's SidebarInset <main>: relative flex col flex-1 min-w-0,
│                 bg `--background` + surface-grain (chat / pages specs)
├ SidebarControl  fixed, left = controls-left, top = controls-top, h 52, ml 1px, z 50,
│                 pointer-events none except the toggle (2.5)
├ NavigationHistoryShortcuts (4.4)
└ MainAppLocationTracker (1.3)
```

- `html`/`body` paint `--app-chrome-background` (= `--background`) with the surface grain
  (`web/index.css:663-668,1685-1689`). No glass/vibrancy behind the app.
- Root tokens: light `--background` zinc-25, dark `#1c1c1c`; the sidebar has its own scope
  (sidebar.md 1.3).

### 2.4 Collapse

- `SidebarProvider` keeps `open` in memory, `defaultOpen = true`; collapse state is **not**
  persisted (the `sidebar_state` cookie is written but never read) (`web/components/ui/sidebar.tsx:111-131`).
  Every launch starts expanded.
- Toggle sources: the fixed toggle button (2.5), `sidebar.toggle` (⌘B, 4.4), the rail only when
  not resizable (never here).
- Collapsed: sidebar gap width 0, container `left: -width`, rail pointer-events none.
- Motion: only when `panelAnimationDurationMs > 0` and reduced motion is off and the first two
  frames after a route change have painted (`web/panelAnimations.ts:15-32,71-79`): width/left
  transitions with that duration, `ease-out`. Default 0: instant. The setting ranges 0-400ms
  (`contracts/settings.ts:110-119`).
- Main-column headers carry `COLLAPSED_SIDEBAR_TITLEBAR_INSET_CLASS`: while collapsed their
  padding-left becomes `--workspace-titlebar-content-left` (122) so content clears the traffic
  lights and the toggle (`web/workspaceTitlebar.ts:1-2`). Animated only with panel animations on.

### 2.5 Sidebar toggle (`SidebarControl`, `web/components/AppSidebarLayout.tsx:81-157`)

- `Toggle variant="panel" size="panel"`: 28x28, rounded-lg 10, 1px transparent border, text
  `--foreground`; hover bg `--accent`; pressed (sidebar open) bg transparent, pressed+hover
  `--accent`; focus-visible ring 2px `--ring` offset 1; no drag region
  (`web/components/ui/toggle.tsx:8-45`). Root tokens (it is outside `[data-app-sidebar]`).
- Icon 16px at opacity .80: `panel-left-close` when open, `panel-left` when collapsed.
- Position: x = 82 + 1 = 83, y = 12 (centered in the 52px topbar) on macOS windowed; x = 13 in
  fullscreen.
- With the sidebar visible and the stage art showing (sidebar.md 2.1): `media-navigation` look:
  text white/90, icon opacity 1, hover/pressed bg white/10 and text white, focus ring white.
- Tooltip (side bottom, 600ms): `Toggle main sidebar (⌘B)` (shortcut omitted when unbound).
  aria-label "Toggle main sidebar".
- Over themed palettes the toggle has theme overrides (`web/index.css:1381-1395`); design-system.

---

## 3. macOS application menu and quit

### 3.1 Menu bar (`desk/window/DesktopApplicationMenu.ts`)

Built once at startup; never rebuilt; accelerators are hardcoded (not from server keybindings).

| Menu | Items (in order) |
| --- | --- |
| `T3 Code` (app name = display name) | About (native panel: name, version, commit) / "Check for Updates..." / — / "Settings..." `⌘,` / — / Services / — / Hide `⌘H` / Hide Others `⌥⌘H` / Show All / — / Quit `⌘Q` (3.2) |
| File | Close Window `⌘W` |
| Edit | Undo `⌘Z` / Redo `⇧⌘Z` / — / Cut / Copy / Paste / "Paste as Text" `⇧⌘V` / Delete / — / Select All / — / Speech (Start Speaking, Stop Speaking) |
| View | Reload / Force Reload / Toggle Developer Tools / — / "Actual Size" `⌘0` / "Zoom In" `⌘=` (hidden duplicate `⌘+`) / "Zoom Out" `⌘-` / — / Toggle Full Screen `⌃⌘F` |
| Window | Minimize `⌘M` / Zoom / — / Bring All to Front |
| Help | "Check for Updates..." |

Actions:

- "Settings...": sends `open-settings`; the renderer pushes `/settings` unless already on a
  settings route (`web/components/AppSidebarLayout.tsx:276-294`).
- "Paste as Text": only for a mouse click on the menu item (the `⇧⌘V` accelerator path returns
  early); arms plain-text paste in the composer, then pastes (`web/lib/desktopPasteAsText.ts`,
  chat spec).
- "Check for Updates...": message boxes (`OK` only): disabled -> "Updates unavailable" /
  "Automatic updates are not available right now." + reason; up to date -> "You're up to date!" /
  `T3 Code <version> is currently the newest version available.`; error -> "Update check failed" /
  "Could not check for updates." / message or "An unknown error occurred. Please try again later."
  (`:61-105`).
- Native: Reload/Force Reload/Toggle Developer Tools have no meaning; drop them (Open questions).
  Zoom items depend on a UI scale feature.

### 3.2 Hold to quit (`desk/window/QuitHold.ts`, wired `desk/window/DesktopWindow.ts:633-658`)

Setting `confirmQuit`: `"direct" | "hold" | "double-click"`, default `"hold"`
(`contracts/settings.ts:229-243,365-367`). ⌘Q handling before the menu accelerator:

- A second ⌘Q within 500ms quits (any mode).
- `direct`: quit.
- `double-click`: show overlay "double-click" mode; auto-hide after the rest of 500ms.
- `hold`: show overlay "hold" mode while held; after 1200ms of key auto-repeat the window hides
  (exits fullscreen, opacity 0) and the app quits on Q key-up or after
  `max(600ms, 2 x repeat cadence)` of quiet. Releasing ⌘ earlier cancels. Any other key cancels.
  Watchdog release at 1800ms.
- Menu Quit (mouse) quits immediately.

### 3.3 Quit overlay (`web/components/QuitHoldOverlay.tsx`)

`role=status`, fixed, centered horizontally, top 22% of the window, z 100, pointer-events none.
Pill: rounded-full, px 32, py 16, 24px/32px bold, bg `--foreground/95`, text `--background`,
`shadow-xl`. Text: hold `Hold ⌘Q or press twice to quit`, double-click `Press ⌘Q again to quit`.
After release in hold mode it lingers 1200ms; no animation.

---

## 4. Keybindings

### 4.1 Rules source and merge

- The server sends resolved rules in `ServerConfig.keybindings` (`ResolvedKeybindingRule {command,
  shortcut {key, metaKey, ctrlKey, shiftKey, altKey, modKey}, whenAst?}`,
  `contracts/keybindings.ts:143-199`; protocol.md section 7). Unknown commands/when nodes are
  dropped on decode.
- The client always merges with the built-in defaults (`mergeWithDefaultKeybindings`,
  `shared/keybindings.ts:329-346`; used by `primaryServerKeybindingsAtom`, `web/state/server.ts:93-95`):
  empty server list -> defaults; else defaults whose command the server did not mention, followed
  by the server rules, keeping the last 256.
- Only the primary environment's keybindings apply.
- Server reload events: `keybindingsUpdated` config event -> toast "Keybindings updated" /
  "Keybindings configuration reloaded successfully." (success, at most once per 2000ms), or
  warning "Invalid keybindings configuration" with the issue message and an outline action
  "Open keybindings.json" (opens `keybindingsConfigPath` in the preferred editor via
  `shell.openInEditor`; failure toast "Unable to open keybindings file")
  (`web/routes/__root.tsx:613-670`, `web/components/KeybindingsUpdateToast.logic.ts`).

### 4.2 Matching (`web/keybindings.ts`)

- Walk rules last to first; the first rule whose `when` holds and whose shortcut matches wins
  (`:236-252`).
- Modifiers must match exactly: `mod` = Meta on macOS, Ctrl elsewhere; `metaKey`/`ctrlKey`
  explicit flags add to it (`:112-126`).
- Key: lowercase `event.key` (`esc` -> `escape`); a non-Latin layout key also matches its
  physical `KeyX` letter; punctuation/digits also match by `event.code` (Backquote, Backslash,
  BracketLeft/Right, Comma, Digit0-9, Equal, Minus, Period, Quote, Semicolon, Slash)
  (`:59-110`). Non-mac AltGraph chords only match letters/digits.
- `when` identifiers: `terminalFocus`, `terminalOpen`, `previewFocus`, `previewOpen`, `isWeb`
  (false), `isDesktop` (true), `editableFocus`, `modelPickerOpen`, `usagePageOpen`; unknown
  identifiers read false; `true`/`false` literals; `!`, `&&`, `||`, parentheses
  (`:147-181`, parser `shared/keybindings.ts:159-290`).
- Labels (`formatShortcutLabel`, `:254-287`): macOS glyph order `⌃⌥⇧⌘` then key; key label:
  space `Space`, single char uppercased, `escape` `Esc`, arrows `Up`/`Down`/`Left`/`Right`, other
  names capitalized (`Enter`). Other platforms `Ctrl+Alt+Shift+Meta+Key`.
- `shortcutLabelForCommand` shows the binding that would actually fire: walking last to first,
  each chord is claimed by its first (latest) matching rule, so a shadowed binding has no label
  (`:199-225,289-302`).
- Every handler ignores events with `defaultPrevented`. Elements under `[data-keybinding-capture]`
  (the keybinding recorder) are skipped by the shell handlers.

### 4.3 Default rules (`shared/keybindings.ts:21-87`)

| Key | Command | When | Owner |
| --- | --- | --- | --- |
| mod+b | sidebar.toggle | | shell 4.4 |
| mod+[ | navigation.back | !terminalFocus | shell 4.4 |
| mod+] | navigation.forward | !terminalFocus | shell 4.4 |
| mod+j | terminal.toggle | | chat (`web/components/ChatView.tsx:6871`) |
| mod+alt+b | rightPanel.toggle | | chat / PR page |
| mod+d | terminal.split | terminalFocus | terminal |
| mod+shift+d | terminal.splitVertical | terminalFocus | terminal |
| mod+n | terminal.new | terminalFocus | terminal |
| mod+w | terminal.close | terminalFocus | terminal |
| mod+w | rightPanel.close | !terminalFocus | chat / PR page |
| mod+d | diff.toggle | !terminalFocus | chat |
| mod+shift+j | preview.toggle | | shell 4.4 |
| mod+r | preview.refresh | previewFocus | shell 4.4 -> preview |
| mod+l | preview.focusUrl | previewFocus | shell 4.4 -> preview |
| mod+= and mod++ | preview.zoomIn | previewFocus | shell 4.4 -> preview |
| mod+- | preview.zoomOut | previewFocus | shell 4.4 -> preview |
| mod+0 | preview.resetZoom | previewFocus | shell 4.4 -> preview |
| mod+k | commandPalette.toggle | !terminalFocus | command palette |
| mod+p | filePicker.toggle | !terminalFocus | command palette |
| mod+shift+f | projectSearch.toggle | !terminalFocus | command palette |
| mod+u | usage.open | !terminalFocus | command palette (push `/usage`) |
| mod+alt+a | theme.select | !terminalFocus | command palette |
| mod+alt+shift+a | appearance.cycle | !terminalFocus | command palette (toast `Appearance: <Light/Dark/System>`, id `appearance-cycle`, 1500ms) |
| mod+alt+shift+t | themeEditor.toggle | | command palette |
| mod+s | composer.stash | !terminalFocus | composer |
| mod+shift+enter | thread.steerQueuedMessage | !terminalFocus | chat |
| mod+n | chat.new | !terminalFocus | shell 4.4 |
| mod+shift+o | chat.new | !terminalFocus | shell 4.4 |
| mod+shift+n | chat.newLocal | !terminalFocus | shell 4.4 |
| mod+alt+n | chat.newWithoutProject | !terminalFocus | shell 4.4 |
| mod+shift+m | modelPicker.toggle | !terminalFocus | chat |
| mod+shift+h | composer.host | !terminalFocus | chat |
| mod+shift+e | composer.effort | !terminalFocus | chat |
| mod+shift+a | composer.mode | !terminalFocus | chat |
| mod+shift+x | composer.workspace | !terminalFocus | chat |
| mod+shift+g | composer.branch | !terminalFocus | chat |
| mod+shift+l | composer.previousWorktree | !terminalFocus | chat |
| mod+shift+k | pullRequest.copyNumber | !terminalFocus | PR detail |
| mod+shift+arrowup | modelPicker.previousProvider | modelPickerOpen | model picker |
| mod+shift+arrowdown | modelPicker.nextProvider | modelPickerOpen | model picker |
| mod+o | editor.openFavorite | | chat (Open in) |
| mod+shift+[ | thread.previous | | sidebar 4.4 |
| mod+shift+] | thread.next | | sidebar 4.4 |
| mod+shift+c | thread.copyReference | !terminalFocus | chat / palette / PR page |
| mod+shift+s | thread.settle | !terminalFocus | chat (toggles settle/un-settle on the route thread) |
| mod+shift+p | thread.pin | !terminalFocus | chat (pin / confirm-unpin the route thread) |
| mod+z | thread.undo | !terminalFocus && !editableFocus | shell 4.4 |
| mod+1 .. mod+9 | thread.jump.1 .. 9 | isDesktop | sidebar 4.4 |
| mod+1 .. mod+9 | modelPicker.jump.1 .. 9 | modelPickerOpen && isDesktop | model picker |
| c / t / l | usage.cost / usage.tokens / usage.limits | usagePageOpen | usage |
| mod+shift+1..4 | usage.period.day / week / month / quarter | usagePageOpen | usage |

`thread.stop` and `script.<id>.run` have no default key (chat spec). Full command list:
`contracts/keybindings.ts:10-119`.

### 4.4 Shell-owned handlers

Context flags each handler passes: `terminalFocus` (focus inside a terminal), `terminalOpen`
(route thread's terminal drawer open), `previewFocus`, `previewOpen` (route thread's active right
panel surface is preview), `editableFocus` (target is a text field/textarea/select/rich text),
`modelPickerOpen`, `usagePageOpen` (pathname `/usage`).

- **sidebar.toggle** (`web/components/AppSidebarLayout.tsx:94-126`): window keydown in the
  **capture** phase (before focused editors). Skips `[data-keybinding-capture]` targets and Mod+B
  (no Alt/Shift) inside the rich-text composer (`[data-composer-rich-text="true"]`, which uses it for
  bold). Context: only `usagePageOpen`. preventDefault + stopPropagation, toggle.
- **navigation.back/forward** (`:160-207`): bubble phase; history back/forward.
- **ChatRouteGlobalShortcuts** (`web/routes/_chat.tsx:31-207`, `/_chat` routes only), bubble
  phase, returns early while the command palette is open:
  - `thread.undo`: ignored on key repeat or with the model picker open; runs the sidebar undo group
    (sidebar.md 7.6) and only then swallows the event (so ⌘Z falls through to text undo when
    nothing is undoable).
  - Escape with a sidebar multi-selection: clear it.
  - `chat.newLocal`: new draft in the context project (route thread's, else route draft's, else
    the first ordered project).
  - `chat.newWithoutProject`: scratch thread in the context environment (scratch project, chat
    spec); ignored when no environment.
  - `chat.new`: default sidebar with more than one project group -> command palette
    `new-thread-in`; legacy sidebar or <= 1 group -> same as `chat.newLocal`.
  - `preview.toggle`: needs a route thread; non-desktop runtime toasts info "Preview is
    desktop-only" / "Open T3 Code in the desktop app to use the in-app preview."; else dispatch
    `toggle-panel` on the preview action bus.
  - `preview.refresh|focusUrl|zoomIn|zoomOut|resetZoom` -> `refresh|focus-url|zoom-in|zoom-out|reset-zoom`.
- **Sidebar list shortcuts** (`web/components/Sidebar.tsx:4498-4551`): window keydown, ignored on
  repeat, with the palette or model picker open. `thread.previous/next`: adjacent row in the
  rendered order (sidebar.md 4.1; collapsed shelves excluded); with no route thread, previous =
  last row, next = first; at the ends nothing. `thread.jump.N`: the Nth rendered row.
- **Jump hints** (`web/components/Sidebar.tsx:4553-4572`, `web/keybindings.ts:321-347`,
  `web/shortcutModifierState.ts`): modifier state is tracked from capture-phase keydown/keyup of
  modifier keys (non-modifier keys may only clear bits); `paste` and window blur reset it. Hints
  show when the held modifiers exactly equal some jump command's effective shortcut modifiers
  and the terminal is not focused, after a 200ms delay; hide immediately. Badge: absolute right 6,
  vertically centered, z 20, h 16, rounded-sm 6, px 6, bg `--sidebar-control-surface`, 1px ring
  `--sidebar-border`, 10px medium sans `--sidebar-muted-foreground`, label e.g. `⌘1`; one per row
  for the first 9 rendered rows (`web/components/Sidebar.tsx:266-267,320-338`).

The rest of the table's handlers belong to their surfaces (chat, terminal, palette, usage, PR).

---

## 5. Toasts (`web/components/ui/toast.tsx`, `toast.logic.ts`, `toastHelpers.ts`)

Base UI Toast 1.5.0 with `toastManager` (global) and `anchoredToastManager`.

### 5.1 Viewport and stack

- Viewport: fixed, z 100, top-right. Width `min(100% - 64px, 360px)`; inset 32px (`sm`), plus
  the 52px header offset, so top = 84px, right = 32px (`:561-579`).
- Limit 3 visible (Base UI default); a 4th pushes the oldest to `data-limited` (opacity 0).
  Default timeout 5000ms; `timeout: 0` sticky; `loading` toasts never time out. Timers pause while
  the pointer is over / focus is in the viewport and while the window is blurred. F6 focuses the
  viewport.
- Thread-scoped toasts (`data.threadRef` or `data.threadId`) render only while that thread is the
  route thread (server or draft) (`toast.logic.ts:108-131`).
- Stack (newest in front, `--toast-index` 0..2): behind toasts are clipped to the frontmost
  height, content hidden (opacity 0, no pointer events), translated down by `index * 12px` (peek)
  plus the shrink compensation, and scaled `1 - 0.1 * index` from the top (`:589-642`).
- Hover/focus expands: each toast moves to `offsetY + index * 12px` (offsetY = sum of heights of
  the toasts in front) at full size with content visible.
- Motion: `transform .5s cubic-bezier(.22,1,.36,1), opacity .5s, height .15s`; content opacity
  250ms. Enter (top-right): from `translateX(100% + 32px)`. Exit: slide right
  `translateX(100% + 32px)` + opacity 0; swipe right/up dismisses in that direction. Ending toasts
  keep their slot while live ones reflow (`toast.logic.ts:45-102`).
- `dismissAfterVisibleMs` (data): a second timer that runs only while the window is visible and
  focused (`:454-530`).

### 5.2 Card

- `dropdown-glass` (6.1), rounded-lg 10, `shadow-xl` with black/25 shadow color, text
  `--popover-foreground`, overflow visible, select-none.
- Corner dismiss (always visible): absolute top -6 right -6, z 20: 24x24 round, 1px
  `--border/60`, bg `--popover/92` + 8px backdrop blur, text `--muted-foreground`, `shadow-sm`;
  hover bg `--popover` text `--foreground`; `x` 12px stroke 2.25; aria-label "Dismiss
  notification" (`:104-110,664-676`).
- Content: pl 14, text-sm; inline layout: py 12, flex items-center justify-between gap 6, pr 24
  when there are trailing controls else pr 40. Stacked layout (`stackedThreadToast`): flex col
  gap 8, py 10, pr 14; body pr 20; controls row full width, right-aligned (`:677-688,346-417`).
- Body: column gap 2. Title row gap 8: icon slot (h = 20px line, w 16) + title (medium, wraps).
  Icons: success `circle-check` `--success`, error `circle-alert` `--destructive`, warning
  `triangle-alert` `--warning`, info `info` `--info`, loading spinner (opacity .8). Description:
  `--muted-foreground`, selectable, wraps; error descriptions >= 180 chars clamp to 4 lines.
- Controls row (gap 6): for error toasts with a string description, a copy button (20x20
  `icon-micro` ghost, rounded-md, `copy` 12px `--muted-foreground/80`; tooltip "Copy error" /
  "Copied error"; check `--success` when copied; copies the full description); additional and
  secondary actions (`Button size="xs"`, default variant `outline`); primary action
  (`Button size="xs"` with `actionVariant`, default `default`). `xs`: h 24, px 7, gap 4, text-xs,
  svg 14 (`web/components/ui/button.tsx:27`).
- Expandable details (`data.expandableContent`): "Show details" / "Hide details" toggle (chevron
  14px stroke 2.25, text-xs medium `--muted-foreground`), panel mt 8, max-h 160, scrolls
  (`:145-271`).

### 5.3 Toasts raised by the shell and sidebar

| Trigger | Type | Title / description |
| --- | --- | --- |
| keybindings reload | success | "Keybindings updated" / "Keybindings configuration reloaded successfully." |
| invalid keybindings | warning (stacked) | "Invalid keybindings configuration" / issue message; action "Open keybindings.json" |
| open keybindings file failed | error | "Unable to open keybindings file" / message or "Unknown error opening file." |
| preview.toggle on web | info | "Preview is desktop-only" / "Open T3 Code in the desktop app to use the in-app preview." |
| slow RPCs | warning, sticky | "Some requests are slow" (`web/components/SlowRpcRequestToastCoordinator.tsx:50-56`) |
| project clone | loading/success/error/info | `Cloning <name>` (Cancel) / `Cloned <name>` (8000ms, "Open project") / `Failed to clone <name>` or `Cancelled cloning <name>` (Retry, Remove project) (`web/components/ProjectCloneToastCoordinator.tsx:120-195`) |
| provider update on launch | warning, sticky | view title; action "Settings" (`web/components/ProviderUpdateLaunchNotification.tsx:160-180`) |
| sidebar actions | various | sidebar.md 7 (copy, rename, settle, snooze, pin, delete, archive, regenerate) |
| desktop update | error | "Could not check for updates", "Could not download update", "Could not start update download", "Could not install update", "Could not confirm update" (sidebar.md 9.3) |

---

## 6. Popups, menus, dialogs

### 6.1 Shared glass surfaces (`web/index.css:364-406`)

- `dropdown-glass` (menus, combobox, toasts, glass tooltips, context menu): background
  `color-mix(--popover 18%, color-mix(--popover --glass-opacity, transparent))` = `--popover` at
  0.836 alpha at the default 80% glass opacity; `backdrop-filter: blur(--glass-blur)
  saturate(--glass-saturation)` (light 12px/1.14, dark 16px/1.08); border 1px
  `--contrast-foreground` at 10%. `--popover`: white / #343434. No shadow of its own.
- `dialog-glass`: `--background` at 80%, same blur; border `--contrast-foreground` 10% (dark
  white 8%); shadow `0 24px 64px -24px rgb(0 0 0/65%)` (dark `inset 0 1px rgb(255 255 255/4%),
  0 24px 72px -20px rgb(0 0 0/90%)`).
- `dialog-backdrop`: `--background` at 60% (dark 64%) + blur 4px.
- `glassOpacity` setting (40-100, default 80) sets `--glass-opacity`; 100 sets blur 0
  (`web/routes/__root.tsx:307-321`).
- Native: GPUI has no backdrop blur on arbitrary layers; paint the tinted color at the computed
  alpha (Open questions).

### 6.2 Context menus are DOM menus (`web/contextMenuFallback.ts`)

All `api.contextMenu.show(items, position)` calls render this in-page menu, desktop included
(`web/localApi.ts:44-57`); the native Electron menu is unused. Only text-field editing menus are
native (spelling suggestions, Copy Link, Copy Image, Cut/Copy/Paste/Select All,
`desk/window/DesktopWindow.ts:528-603`).

Item: `{id, label, destructive?, disabled?, header?, icon?, separatorBefore?, checked?, children?}`
(`contracts/ipc.ts:36-50`). Resolves the clicked leaf id or null.

- Container (one per level): fixed, z 10000, appended to the window root (root tokens, not the
  sidebar's), min-w 128, max-w 384, shrink-to-fit, rounded-lg 10, `dropdown-glass`, no shadow, text
  `--popover-foreground`. Inner scroller max-h `min(384px, 70vh)`, padding 4 (`:296-309`).
- Row (button): min-h 28, flex items-center gap 8, padding 8 x 4, rounded-sm 6, 14px/20px regular,
  `--foreground`, cursor default; label flex-1 truncate (`:334-381`).
- Icon slot: 16px stroke-2 glyph in `--muted-foreground` (destructive: inherits). No gutter is
  reserved: rows without an icon start the label at 8px, rows with one at 32px. Unknown icon
  names render nothing. Glyphs are inline SVG paths (`:6-137`): `archive`, `check`, `timer`,
  `chevron-right` (`m9 19 7-7-7-7`, differs from lucide), `circle-check`, `clock`, `pencil`,
  `copy`, `folder`, `git-branch` (older lucide form), `hash`, `mail-open`, `message-square-plus`,
  `pin`, `pin-off`, `refresh-cw`, `settings` (older gear path), `folder-tree`, `trash` (lucide
  `trash-2` geometry). Copy these paths, not current lucide.
- Checked items (`checked` boolean): role menuitemradio; checked draws `check` in the icon slot,
  unchecked a 16px spacer; `icon` ignored.
- Destructive (leaf with `destructive` or id `delete`): text/icon `--destructive-foreground`
  (red-700 / red-400); highlight bg `--destructive` 10%.
- Disabled: `--muted-foreground`, opacity .64, no hover; still shows its chevron.
- Separator: only for `separatorBefore` on a non-first item: 1px `--border`, margin 4 x 8. No
  automatic separators.
- Header items: padding 8 x 6, 12px/16px medium `--muted-foreground`, wraps, inert.
- Highlight: hover or focus (hover also focuses the row): bg `--accent`, text
  `--accent-foreground`; the last hovered row stays highlighted after the pointer leaves. Color
  transition 150ms `cubic-bezier(0.4,0,0.2,1)`. No focus ring.
- Submenus: chevron 16px `--muted-foreground` opacity .8, mr -2. Open immediately on hover (no
  intent delay) or click/Enter/Space (focusing the first enabled child). Placed at
  `trigger.right + 4`, `trigger.top`; clamped 4px inside the window (an overflowing submenu ends
  up pinned 4px from the right edge, overlapping the parent). Hovering another row at that level
  or entering any menu container closes deeper levels.
- Positioning: at the pointer, clamped 4px inside the window.
- Keyboard: only Escape (closes all, null). Tab/Shift+Tab move through buttons; no arrows, no
  typeahead, no initial focus.
- Dismiss with null: pointerdown or contextmenu outside (contextmenu is swallowed), Escape, a new
  menu, `close()`. Clicks during the first frame are ignored. Not dismissed by scroll/resize/blur.
  Focus returns to the previous element if it was inside the menu.
- No open/close animation.

### 6.3 Confirm dialog (`web/components/ConfirmDialogHost.tsx`, `web/confirmDialog.ts`, `web/components/ui/alert-dialog.tsx`)

`api.dialogs.confirm(message, {variant: "default" | "destructive"})` -> Promise<boolean>. Not a
native message box. One host; requests queue FIFO; the next opens after the close animation; with
no host it resolves false.

- Title/description split (`ConfirmDialogHost.tsx:26-52`): trim, split lines. If a line ends in
  `?`, the first such line is the title and the remaining lines (joined, trimmed) the description.
  Else if a `?` appears, title = text through the first `?`, description = rest. Else title
  "Confirm action" and description = message (or "This action requires your confirmation.").
- Backdrop: full window, `dialog-backdrop`, opacity fade 200ms.
- Viewport: grid rows `1fr auto 1fr`, centered, padding 16.
- Popup: width 512 (`max-w-lg`), rounded-2xl 18, 1px border, `dialog-glass`, text
  `--popover-foreground`. Header: padding 24, gap 8; title 20px/20px semibold; description 14px/20px
  `--muted-foreground`, `white-space: pre-line`. Footer: border-top 1px `--border`, bg
  `--muted/72`, padding 24 x 16, flex end, gap 8, bottom radius 17.
- Buttons (h 32, px 11, rounded 8, 14px medium, active scale .97 over 150ms): "Cancel" outline
  (border `--input`, bg `--popover` / dark `--input/32`, `shadow-xs` 5%, hover `--accent/50` /
  dark `--input/64`) then "Confirm": `default` (`--primary` bg/border, `--primary-foreground`
  text, inset top highlight white/16 + `0 1px 2px --primary/24`, hover 90%) or `destructive`
  (`--destructive` bg, white text, hover 90%).
- Focus: modal, trapped; initial focus on Cancel; Enter activates the focused button (so Enter
  cancels by default); Escape cancels; backdrop clicks do nothing; focus returns on close.
- Motion: popup scale .98 + opacity 0 <-> 1 over 200ms `cubic-bezier(0.4,0,0.2,1)`.
- Callers and messages: sidebar.md 7.2-7.3, and update install (sidebar.md 9.3).

### 6.4 Tooltips (`web/components/ui/tooltip.tsx`)

Base UI default open delay 600ms, close 0 (sidebar thread list overrides 150/0/400ms group).
Popup: rounded-md 8, 1px `--border`, bg `--popover`, `shadow-md/5`, text-xs `--popover-foreground`,
leading-snug, max-w 320, padding 8 x 4, offset 4, z 140; enter/exit scale .98 + opacity over the
default 150ms. `glass` variant: `dropdown-glass`, `shadow-xl` black/25, no border shadow. Side
defaults to top.

---

## 7. Local persistence (what the native app must save)

`stateDir` = `~/.t3/userdata` (dev `~/.t3/dev`, or `$T3CODE_HOME/userdata`)
(`desk/app/DesktopStatePaths.ts:13-32`). The native app keeps its own directory
(`~/Library/Application Support/T3UI/`, architecture.md) with equivalent files; importing the
fork's files is optional (Open questions).

### 7.1 Files owned by the Electron main process

| File | Contents | Writes |
| --- | --- | --- |
| `client-settings.json` | full `ClientSettings` (legacy `{settings:{...}}` wrapper accepted); every field defaults independently (`desk/settings/DesktopClientSettings.ts:20-171`) | every patch, serialized queue, atomic temp+rename |
| `desktop-settings.json` | `mainWindowBounds {x,y,width>=840,height>=620} \| null`, `mainWindowMaximized`, plus backend options (`localEnvironmentEnabled`, `serverExposureMode`, `tailscaleServe*`, `updateChannel`, `wsl*`) (`desk/settings/DesktopAppSettings.ts:27-115`) | bounds debounced 500ms, flushed on close |
| `connection-catalog.json` | encrypted connection catalog (connections spec) | on change |
| `notifications.json` | macOS notification inbox (dock badge = unread count) (`desk/notifications/ElectronNotifications.ts:36-116`) | on change |

`ClientSettings` fields that the shell and sidebar read (`contracts/settings.ts:298-517`):
`legacySidebarEnabled` false, `sidebarWorkingShelfEnabled` false, `notificationInboxEnabled`
false, `environmentIdentificationMode` "artwork", `confirmThreadArchive` false,
`confirmThreadDelete` true, `confirmThreadUnpin` false, `confirmQuit` "hold",
`sidebarProjectSortOrder` "updated_at", `sidebarThreadSortOrder` "updated_at",
`sidebarProjectGroupingMode` "repository", `sidebarProjectGroupingOverrides` {},
`timestampFormat` "locale", `panelAnimationDurationMs` 0, `glassOpacity` 80,
`appearanceContrast` 100, font fields, `onboardingCompletedAt` null. The full list (about 70
fields) is in the persistence inventory below.

### 7.2 Renderer localStorage keys (shell + sidebar scope)

Values are JSON (Effect Schema encoding) unless noted; `null` removes the key.

| Key | Shape / default | Owner |
| --- | --- | --- |
| `t3code:ui-state:v1` | `{projectExpandedById, projectOrder, threadLastVisitedAtById, defaultAdvertisedEndpointKey, sidebarProjectScopeKey, threadChangedFilesExpansionVersion:2, threadChangedFilesExpandedById, pullRequestMergeMethod:"merge"}`; debounced 500ms, flushed on unload; migrates `t3code:renderer-state:v8..v3`, `codething:renderer-state:v4..v1` (`web/uiStateStore.ts:6-248`) | sidebar, chat |
| `chat_thread_sidebar_width` | number px; default 256, min 208, max `viewport - 640`; written on drag end, removed on reset (`web/components/threadSidebarWidth.ts`) | sidebar |
| `t3code:sidebar:settled-expanded`, `t3code:sidebar:snoozed-expanded`, `t3code:sidebar:working-expanded` | boolean, default false | sidebar |
| `t3code:theme`, `t3code:theme-appearance-mode`, `t3code:theme-halves:v1`, `t3code:themes:v1`, `t3code:default-theme-applied:v2:<env>` | theme (design-system spec) | theme |
| `t3code:provider-update-dismissals:v1` | `{keys: string[]}` (`web/providerUpdateDismissal.ts:6-31`) | launch notification |
| `t3code:version-mismatch-dismissals:v1` | `{keys: string[]}` | version banner |
| `t3code:last-editor` | `EditorId \| null` (`web/editorPreferences.ts:15-60`) | Open in editor, keybindings.json action |
| `t3.pullRequests.preferences` | PR list filters; default `{involvement:"all", state:"open"}`; the sidebar's Pull Requests button sends them as search params (`web/components/pullRequest/pullRequestListPreferences.ts:22-123`) | PR page |
| `t3code:composer-drafts:v1` | composer drafts + draft sessions (sidebar draft rows read it) | chat spec |
| `t3code:terminal-state:v1`, `t3code:right-panel-state:v2`, `t3code:diff-panel-state:v1`, `t3code:prompt-stash:v2`, `t3code:browser-history:v1`, `t3code:browser-favicons:v1`, `t3code:preview-panel-width:<env>:<thread>`, `t3code:pull-request-panel-width`, `t3code.diffFileTreeOpen`, `t3code.pullRequestFileTreeOpen`, `t3code.fileExplorerOpen`, `t3code.renderMarkdown`, `t3code.renderBrowserFile`, `t3code.renderTable`, `t3code:typography-advanced`, `t3code:last-invoked-script-by-project`, `t3code:resume-compaction-dismissed:<env>:<instance>`, `t3code:remote-open-hint-seen`, `t3code:usage-page-preferences:v1`, `t3code:connect-onboarding-opt-out:v1`, `t3:chatgpt-sharing-welcome:v1`, `t3code:last-enabled-project-grouping-mode`, `t3code:snap-shot-setup-resume:v1`, `t3.backgroundActivity.clientId`, `t3code:github-routing:<env>`, `t3.pullRequests.list:<envs>`, `t3.pullRequests.detail:<...>` | per-feature state | owning specs |

IndexedDB `t3code:connection-runtime` v4 caches shell snapshots, thread details, server configs
and VCS refs (debounced 500ms) for warm starts; `t3code:project-favicons` v2 caches favicon data
URLs (`web/connection/storage.ts:47-337`, `web/assets/projectFaviconCache.ts`). Native: optional
caches (Open questions).

### 7.3 Not persisted

Route and history (relaunch at `/`), sidebar open/collapsed (always expanded at launch),
multi-selection, search query, settled paging, Working-beta return stamps, undo notices, right
panel maximized, queued messages.

---

## 8. App-level coordinators

### 8.1 EventRouter (`web/routes/__root.tsx:544-693`)

- On every primary `serverConfig`: set the active environment to the primary.
- On the server welcome (`subscribeServerLifecycle` `welcome` event): set the active environment; if the
  payload has `bootstrapProjectId` + `bootstrapThreadId`: expand that project in UI state, and if
  the route is still `/` (and the welcome was not handled already, and the app did not just come
  back from `/welcome`) navigate (replace) to `/$env/$bootstrapThread`.
- Keybinding reload toasts (4.1).

### 8.2 Notifications and badge

- macOS: the Electron main process owns a notification inbox (`notifications.json`), posts native
  notifications for approval / input / failed / completed, sets the dock badge to the unread
  count, and syncs with the renderer (`DesktopNotificationCoordinator`, `web/desktopNotifications.ts`,
  `desk/notifications/`). It also feeds the sidebar Done badges (sidebar.md 4.5) and the bell.
- Without the bridge: `ThreadCompletionNotificationCoordinator` posts a browser notification
  (title = app name, body = thread title) for newly completed, non-settled threads while the
  window is unfocused, skipping threads with an auto-send queued message
  (`web/routes/__root.tsx:506-542`, `web/threadCompletionNotifications.ts`).
- `notificationMode` setting (`off` default) controls in-app sounds/notifications
  (`web/components/ThreadNotificationCoordinator.tsx`).

### 8.3 Others

- `RunningThreadKeepAlive` (Electron): keeps every running thread subscribed (thread detail
  streams stay open) so completions arrive in the background (`web/components/desktop/RunningThreadKeepAlive.tsx`).
- `DesktopAppActivationCoordinator`: handles `t3 app` CLI "open workspace" requests over a local
  socket (`desk/app/DesktopAppActivation.ts`); out of scope unless the native app ships a CLI.
- `HostedStaticEnvironmentBootstrap`: hosted web only.

---

## 9. Data sources and missing client APIs

| Need | Upstream | t3UI |
| --- | --- | --- |
| Keybindings | `ServerConfig.keybindings` + `keybindingsConfigPath`; `server.subscribeServerConfig` `keybindingsUpdated` event with `issues` | config decoded (`t3_protocol::server::ServerConfig.keybindings`, `issues`); merge with defaults **missing** (uses server-or-default) |
| Open keybindings.json | `shell.openInEditor {cwd, editor}` + preferred editor | present (`t3_protocol::methods::ShellOpenInEditor`, used by `t3-app/src/notifications.rs:88`) |
| Server welcome bootstrap | `subscribeServerLifecycle` `welcome` event: `bootstrapProjectId`, `bootstrapThreadId` (protocol.md 7.5) | decoded (`t3_protocol::server::LifecycleWelcome`); check the app consumes it for 8.1 |
| Stage label | `ServerConfig.environment.serverVersion` | present |
| Capabilities | `environment.capabilities.*` | present |
| Lifecycle commands | sidebar.md 10 | builders missing for snooze/unsnooze/pin/unpin/pin.reorder/active.reorder/auto-settle.set |
| Thread search | `orchestration.searchThreads` | method exists; no `Environment` helper |
| Optimistic lifecycle overlay | `cr/state/threadLifecycle.ts` | missing |
| Desktop update state | Electron updater IPC | n/a: native updater missing |
| Notification inbox | Electron main | missing |

---

## 10. Reuse map (shell)

| t3UI module | Verdict |
| --- | --- |
| `t3-app/src/state/` (`AppState`, `Route`, `Environment`, boot, fixtures, vcs, favicons) | Reusable mechanics. Add routes `PullRequests`, `Usage`, `Welcome`, `Project(key)` (+ redirect to settings projects) and `last_main_route` from branch `shell` 391b330 (matches `mainAppLocation.ts`). Route `Index` must start a draft for the most recent project (1.2). Settings pages list grew (1.2). |
| `t3-app/src/workspace/` (collapse, rail resize, `build_main_view`, key dispatch) | Reusable mechanics. Fix: no glass/vibrancy (opaque `--background`), toggle at x 83/y 12 (controls-left 82 = 70 + 12, not 90), content inset 122, sidebar min 208 / max viewport-640 / main >= 640, width key semantics, collapse not persisted, panel animation 0 by default. `index_view.rs` is July-era visuals: replace with the draft landing (chat) and `NoProjectsHero` (pages). |
| `t3-app/src/keybindings/` root resolver, `ShortcutScope` | Reusable. Add the capture-phase `sidebar.toggle` exception for the rich-text composer, `editableFocus` and `usagePageOpen` flags from real views, `thread.undo` fall-through, `chat.new` palette routing, Escape clears selection. |
| `t3-app/src/keybindings/menu.rs` | Mostly right. Add Edit: "Paste as Text" `⇧⌘V`, Delete, Speech submenu; View: "Actual Size"/"Zoom In"/"Zoom Out" if UI scale ships; About shows version + commit like Electron's panel; Check for Updates dialogs per 3.1 strings ("You're up to date!" etc.) once a native updater exists. Hold-to-quit (3.2-3.3) is missing. |
| `t3-logic/src/keybindings/` | Matcher, parser, labels reusable. Replace `DEFAULT_RULES` with 4.3 (73 rules; jump rules need `when: isDesktop` and `modelPickerOpen && isDesktop`); add `merge_with_defaults` (4.1) and use it instead of server-or-default (`t3-app/src/state/mod.rs:294-301`); add `Command` variants for the new commands (navigation, filePicker, projectSearch, usage.*, theme.*, appearance.cycle, themeEditor.toggle, composer.*, chat.newWithoutProject, thread.steerQueuedMessage, pullRequest.copyNumber, modelPicker prev/next, thread.copyReference/settle/pin/undo/stop). |
| `t3-logic/src/{settings,ui_state}.rs` | Reusable; add fields: settings `legacySidebarEnabled`, `sidebarWorkingShelfEnabled`, `notificationInboxEnabled`, `environmentIdentificationMode`, `confirmThreadUnpin`, `confirmQuit`, `panelAnimationDurationMs`, `glassOpacity`, `timestampFormat`; ui state `sidebarProjectScopeKey`, `pullRequestMergeMethod`, `defaultAdvertisedEndpointKey`, changed-files expansion. Preserve unknown keys on write (handoff notes they are dropped). |
| `t3-app/src/toast.rs` | Mechanics reusable (stack of 3, 5s, thread scoping). Restyle per 5.2: glass card, corner dismiss orb, stacked layout, copy-error button, expand-on-hover offsets, slide-in from the right, timers paused on hover/blur. |
| `t3-app/src/dialogs.rs` (`Window::prompt`) | Replace for confirmations: the fork uses the in-app glass AlertDialog (6.3), not native message boxes. Keep native boxes only for "Check for Updates..." results and fatal startup errors. This also resolves the handoff's Return-on-"No" issue. |
| `t3-app/src/sidebar/menus.rs` NativeMenu | Replace presentation with the DOM-style menu (6.2); keep the action flows. |
| `t3-app/src/notifications.rs` | Keybindings notifier matches 4.1. |
| `t3-app/src/chrome.rs` | Type scale and drag region reusable; text tooltips need the 6.4 styling and delays. |
| Branch `shell` 391b330 | Land the route/back-navigation parts; footer per sidebar.md 11. Not landed yet. |

---

## 11. Reference screenshots needed (shell)

1440x900 @2x, light and dark, nightly e2e server unless noted.

1. `shell-default`: launch, primary connected, two projects: lands on a new draft (route
   `/draft/...`), sidebar expanded, toggle at 83,12.
2. `shell-sidebar-collapsed`: ⌘B; main header inset 122.
3. `shell-fullscreen` and `shell-fullscreen-collapsed`: macOS fullscreen (controls-left 12).
4. `shell-toggle-tooltip`: hover the toggle 700ms ("Toggle main sidebar (⌘B)").
5. `shell-toast-success`: copy a thread ID from the context menu.
6. `shell-toast-error-stacked`: force a failing action (e.g. rename against a stopped server) to
   show the stacked error toast with the copy button; then three toasts stacked, and the same
   stack hovered (expanded).
7. `shell-context-menu`: sidebar thread context menu (DOM menu) with the Copy submenu open.
8. `shell-confirm-delete`: Delete thread confirm (destructive) and Archive confirm (enable
   `confirmThreadArchive`).
9. `shell-confirm-worktree`: delete the only thread on a worktree.
10. `shell-quit-overlay`: hold ⌘Q briefly (hold mode) and double-click mode.
11. `shell-not-found` and `shell-route-error`: navigate to a bad hash route; force a loader error.
12. `shell-keybindings-invalid-toast`: write an invalid `keybindings.json` on the server.
13. `shell-app-menu`: each macOS menu opened (Electron reference via the harness), for labels and
    accelerators.
14. `shell-index-no-projects`: fresh server with no projects (NoProjectsHero, pages spec).

---

## 12. Open questions / risks

- Backdrop blur: GPUI cannot blur what is behind an arbitrary layer. Menus, toasts and dialogs use
  `dropdown-glass`/`dialog-glass`; paint the tint at the resolved alpha over the app (0.836 popover
  for dropdowns, 0.8 background for dialogs) and accept the missing blur, or use an opaque fill.
  Decide with the design-system agent.
- Native vs fork menus: the fork renders context menus and confirms inside the page. Matching
  that means custom GPUI popups (focus handling, z-order above everything, Escape). The existing
  NativeMenu/`Window::prompt` code paths would go.
- Developer menu items (Reload, Force Reload, Toggle Developer Tools) and zoom have no native
  equivalent. Proposal: drop the developer items; implement zoom only if a UI scale lands.
- Hold-to-quit needs key-down/up timing for ⌘Q before AppKit's menu handles it; GPUI menus fire on
  key-down. Needs a probe on macOS.
- Notification inbox, dock badge and native notifications are an unassigned subsystem.
- Updater: no native updater; the update button, menu "Check for Updates...", and release-notes
  popover depend on one.
- Importing existing fork state (client-settings.json, localStorage last-visited timestamps) would
  make the native app match a user's current Done/unread state; not required, but without it the
  first launch differs.
- `panelAnimationDurationMs` defaults to 0, so collapse is instant in the reference; animations
  only matter when the user raises it.
