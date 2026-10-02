# Command palette and search overlays: build spec

Refreshed against fe7d3092c. Every fact below was read from `~/L-Projects/t3UI-refs/t3code-fork` at that commit.

Companion file: [`overlays-notifications.md`](overlays-notifications.md) (toast system, app-wide toasts, dialogs, sidebar notices, update pill, quit overlay, context menus).

## 0. Conventions

- Path prefixes: `web/` = `apps/web/src/`, `cr/` = `packages/client-runtime/src/`, `contracts/` = `packages/contracts/src/`, `shared/` = `packages/shared/src/`, `desk/` = `apps/desktop/src/`, `bui/` = `apps/web/node_modules/@base-ui/react/` (Base UI 1.5.0, CJS build).
- Desktop window >= 1100px: Tailwind `sm:` applies, `max-sm:` does not. Values below are the `sm:` ones. 1 unit = 4px. `text-xs` 12/16, `text-sm` 14/20, `text-[10px]`, `text-3xs` = 10/14 (`web/index.css:170-171`).
- Radii (`--radius` = 10px, `web/index.css:1030`, `:265-270`): `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18, `rounded` 4, `rounded-full`. `--control-radius` 8 (`web/index.css:91`).
- Colors are CSS variable names (resolve through `docs/spec/tokens.json`). `x/NN` = token at NN% alpha. `--icon-muted`, `--placeholder`, `--secondary-label`, `--sidebar-row-hover`, `--sidebar-control-surface`, `--sidebar-border` are theme tokens (`web/index.css:236-264`, `:1052-1086`, dark `:1117-1146`).
- Tooltips: Base UI default open delay 600ms, close 0 (`bui/esm/tooltip/utils/constants.js:1`); popup `text-xs`, px 8, py 4, `rounded-md`, border, bg `--popover`, `shadow-md/5`, side offset 4 (`web/components/ui/tooltip.tsx:30-64`).
- Kbd (`web/components/ui/kbd.tsx:5-16`): h 20, min-w 20, `rounded` (4), px 4, gap 4, bg `--sidebar-control-surface`, `text-xs` medium sans `--muted-foreground`, 1px ring `--sidebar-border`, svg 12px.

## 1. One overlay, three modes

`CommandPalette` (`web/components/CommandPalette.tsx:485-657`) wraps the app shell (`web/routes/__root.tsx:221-227`; also the `/welcome` tree, `:202-206`). It owns a single Base UI `Dialog` that shows one of three mutually exclusive surfaces:

| Mode | Surface | Popup `aria-label` | Spec |
| --- | --- | --- | --- |
| `command` | Command palette | "Command palette" | §3-§9 |
| `files` | Project file picker | "File picker" | §10 |
| `content` | Project content search | "Search project contents" | §11 |

(`CommandPalette.tsx:669-701`.) While open, the app subtree is `inert` (no focus, no pointer) (`:643-646`).

### 1.1 State machine

`reduceCommandPaletteUiState` (`web/components/CommandPalette.logic.ts:90-122`), state `{open, mode, openIntent}`:

| Action | Result |
| --- | --- |
| `SetOpen(true)` | open, mode `command`, keep intent |
| `SetOpen(false)` | closed, intent cleared |
| `ToggleMode(m)` | if open and mode == m: close; else open in m, intent cleared |
| `OpenSearch(query, linkedThreads?)` | open, `command`, intent `search` |
| `OpenAddProject` / `OpenNewThreadIn` / `OpenChangeTheme` | open, `command`, intent of that kind |
| `ClearOpenIntent` | intent null |

The popup content unmounts on close (Base UI default), so every open starts fresh: root view, empty query, nothing highlighted (`:705-720`).

### 1.2 Triggers

Window `keydown` listener, bubble phase (`CommandPalette.tsx:531-608`). Skips events already `defaultPrevented`. Resolves the shortcut with context `{terminalFocus, terminalOpen, previewFocus, previewOpen, modelPickerOpen}` plus `isDesktop: true` (`web/keybindings.ts:154`). Each handled command calls preventDefault + stopPropagation.

| Command | Default key (`shared/keybindings.ts`) | Effect |
| --- | --- | --- |
| `commandPalette.toggle` | mod+k, `!terminalFocus` (`:40`) | `ToggleMode(command)` |
| `filePicker.toggle` | mod+p, `!terminalFocus` (`:41`) | `ToggleMode(files)` |
| `projectSearch.toggle` | mod+shift+f, `!terminalFocus` (`:42`) | `ToggleMode(content)` |
| `theme.select` | mod+alt+a, `!terminalFocus` (`:44`) | `OpenChangeTheme` (ignores key repeat) |
| `appearance.cycle` | mod+alt+shift+a, `!terminalFocus` (`:45`) | cycles system → light → dark → system; toast (below); ignores repeat |
| `themeEditor.toggle` | mod+alt+shift+t (`:46`) | toggles the theme editor (settings spec) |
| `usage.open` | mod+u, `!terminalFocus` (`:43`) | closes palette, navigates `/usage` |

`appearance.cycle` toast (`CommandPalette.tsx:545-560`): `{id: "appearance-cycle", title: "Appearance: <System|Light|Dark>", timeout: 1500}`, no type (no icon). Reusing the id replaces the toast in place and restarts its timer (`bui/toast/store.js:91-113`). Failure to save: stacked error "Couldn't save theme selection" / "Try again." (`:221-229`).

Bus (`web/commandPaletteBus.ts:8-32`): `openCommandPalette({open?: "add-project" | "new-thread-in", query?, linkedThreads?})`. Handler (`CommandPalette.tsx:610-628`): `new-thread-in` → `OpenNewThreadIn`; `add-project` → `OpenAddProject`; `query` → `OpenSearch`; nothing → `SetOpen(true)`. Callers:

| Caller | Call |
| --- | --- |
| Sidebar "Add project" button | `{open: "add-project"}` (`web/components/Sidebar.tsx:2330`) |
| Sidebar new-thread button, more than one project group, no Shift | `{open: "new-thread-in"}` (`Sidebar.tsx:4580-4596`) |
| `chat.new` (mod+n / mod+shift+o) with the default sidebar and more than one project group | `{open: "new-thread-in"}` (`web/routes/_chat.tsx:121-130`) |
| No-projects hero, draft hero headline | `{open: "add-project"}` (`web/components/NoProjectsHero.tsx:14`, `chat/DraftHeroHeadline.tsx:70`) |
| PR list empty state | `{open: "add-project"}` (`pullRequest/PullRequestListEmptyState.tsx:112`) |
| PR "Linked from N threads" count button | `{query: <pr url>, linkedThreads?}` (`pullRequest/PullRequestThreadLinks.tsx:120-127`) |
| Sidebar "Search" row | toggles (shell spec) |

`isCommandPaletteOpen()` (`commandPaletteBus.ts:35-39`) = the popup is in the tree. These global handlers bail while it is open: `_chat.tsx:79` (all chat route shortcuts incl. `thread.undo`), sidebar thread jump/traversal (`Sidebar.tsx:4507`), model picker (`chat/ModelPickerContent.tsx:744`), composer (`chat/ChatComposer.tsx:5253`), ChatView (`ChatView.tsx:6794`, `:7079`), PR page/panel, usage page.

### 1.3 Closing and focus

- Backdrop pointerdown → close (`CommandPalette.tsx:685-687`).
- Escape: in `command` mode the dialog closes. In `files`/`content` mode a capture-phase window listener returns to `command` mode instead (`:519-529`), and `onOpenChange` with reason `escape-key` is cancelled the same way (`:634-640`). So Esc steps back from the file picker or content search to the palette root.
- Running an item without `keepOpen` closes (`:3113-3115`).
- `finalFocus`: focus the composer at the end of its text; never restore the pre-open focus (`:681-684`).
- Re-focus on view change: every content remount focuses the input in a layout effect (`web/components/CommandPaletteContent.tsx:45-47`).

## 2. Chrome (shared by all three modes)

```
backdrop    fixed inset 0, z 50, `dialog-backdrop`: bg background/60 (dark /64) + backdrop blur 4px
            opacity 0 → 1, 200ms, ease cubic-bezier(.4,0,.2,1)      web/components/ui/dialog-styles.ts:1-4, web/index.css:383-391
viewport    fixed inset 0, z 50, flex col, items-center, px 16, pt/pb 10vh, pointer-events none   ui/command.tsx:37-48
popup       w 100% (max 576 = max-w-xl), max-h 420 (content mode: h 420 fixed), flex col, overflow hidden,
            rounded 18, 1px border, `dialog-glass`, pointer-events auto, text --foreground          ui/command.tsx:50-76, CommandPalette.tsx:677
  dialog-glass: bg = --background at --glass-opacity (80%), backdrop blur(--glass-blur 12px; dark 16px) saturate(1.14; dark 1.08)
                border = --contrast-foreground 10% (dark: white 8%)
                shadow light: 0 24px 64px -24px rgb(0 0 0/.65); dark: inset 0 1px rgb(255 255 255/.04), 0 24px 72px -20px rgb(0 0 0/.9)
                no backdrop-filter support → bg --popover                                          web/index.css:364-381, :101-125
  enter/exit: scale .98 → 1 and opacity 0 → 1, 200ms ease-in-out (cubic-bezier(.4,0,.2,1))        dialog-styles.ts:7-10
├ header      relative block (input + absolutely positioned accessory)                           CommandPaletteContent.tsx:51-54
│  wrapper    px 8, py 6 → total 48 tall                                                          ui/command.tsx:100
│  input box  36 tall (34 + 1px transparent border top/bottom), border/bg/shadow/focus ring all suppressed   ui/command.tsx:104, ui/input.tsx:25-28
│   text      14px, line-height 34, --foreground; placeholder --placeholder
│             starts 42px from the popup's outer left edge (1 border + 8 + 1 + 32 padding)
│             right padding 11 by default; overridden per mode (§6.4)
│   start icon 16px, 80% opacity, vertically centered, left edge 19px from popup outer edge
│             root: `Search` in --icon-muted; submenu: `ArrowLeft` button (aria "Back", --foreground) at 17px;
│             root browse: `FolderPlus` (--foreground) at 17px                                   ui/autocomplete.tsx:30-38, CommandPalette.tsx:3385-3400
│  [accessory] absolute, right 10, vertically centered (§6.4, §11)
├ panel       min-h 0, overflow hidden, top corners 14, transparent; max-h min(448, 70vh)
│             (file picker: min(544, 76vh); content search: flex-1). The 420 popup cap wins at desktop sizes.   CommandPaletteContent.tsx:56-63, ui/command.tsx:126-136
│  [context block]  §6.3 / §7
│  list       ScrollArea (scroll fade 24px top/bottom, scroll-padding 24, stable gutter) > list p 8 (scroll-py 8)   ui/autocomplete.tsx:172-182, ui/command.tsx:116-124
│   scrollbar 6 wide, 1px from the right, mt 8 / mb 4; hidden until hover/scroll (opacity, 300ms delay out, 100ms in); thumb rounded-full --app-scrollbar-thumb   ui/scroll-area.tsx:84-104
│   group     consecutive groups: mt 6                                                          ui/autocomplete.tsx:142
│    label    px 8, py 6, text-xs medium --muted-foreground (28 tall); omitted when label is ""   ui/autocomplete.tsx:152
│    row      §2.1
│  [empty]    py 40, centered, text-sm --muted-foreground                                       CommandPaletteResults.tsx:31
└ footer      flex, items-center, justify-between, gap 8, px 16, py 10 (40 tall), bg foreground/2.5%,
              bottom corners 17, text-sm medium --muted-foreground. No border.                  ui/command.tsx:192-203
   left       flex gap 12 of KbdGroups (inline-flex, gap 4): Kbd then label                      CommandPaletteContent.tsx:67-96
              footer Kbd override: bg --sidebar-control-surface, text --foreground, ring --sidebar-border
   right      optional trailing action (§6.5)
```

Footer hints, in order: `[↑][↓]` "Navigate" (lucide `ArrowUp`/`ArrowDown` 12px); `[Enter]` "<footerActionLabel>" when defined; `[Backspace]` "Back" in submenus; `[Esc]` "<escapeLabel>" ("Close" in the palette, "Back" in file picker and content search) (`CommandPaletteContent.tsx:67-96`).

Height check at 900px window: input 48 + list (<= 420 − 48 − 40 − 2 = 330) + footer 40.

### 2.1 Result row

`CommandPaletteResults.tsx:98-153`, item classes `ui/autocomplete.tsx:128` + `ui/command.tsx:158-177`.

```
row (enabled)  flex, items-center, gap 8, min-h 28, px 8, py 6, rounded 6, text-sm, cursor pointer
               icons without a text class get --muted-foreground
  icon         16px (`ITEM_ICON_CLASS` = size-4 --icon-muted, CommandPalette.logic.ts:20)
  body         no description/match: flex-1, items-center, gap 6, text-sm --foreground: [leading] title (truncate)
               with description or match: flex-1 column:
                 title line   gap 6, text-sm --foreground: [leading content] title (truncate)
                 [match]      text-xs --muted-foreground/85, truncate: "You:" (--info-foreground) | "Agent:" (--success-foreground), space, snippet
                              with every case-insensitive (ASCII fold) occurrence of the query in font-semibold --foreground   ThreadSearchMatch.tsx:5-66
                 [description] text-xs --muted-foreground/70
  [trailing content]
  [timestamp]  min-w 48, right-aligned, text-xs tabular-nums --muted-foreground/70
  [shortcut]   margin-left auto, h 16, px 6, rounded 6, bg --muted, 10px medium sans, --secondary-label; dark: 1px ring white/5%   ui/command.tsx:179-190
               text = shortcutLabelForCommand(keybindings, item.shortcutCommand) (shell spec §6.1); hidden when unbound
  [submenu]    `ChevronRight` 16px, margin-left auto, -2px right margin, --muted-foreground/70
row (disabled) plain div (not navigable, not clickable): same box, opacity .64; icon, body, trailing content only (no timestamp/shortcut/chevron)   CommandPaletteResults.tsx:66-96
```

Highlight: exactly one "active" row, tracked by the palette (`highlightedItemValue`). Active = bg `--sidebar-row-hover`, text `--accent-foreground`. Hover and keyboard highlight both just move the active row; there is no separate hover color (`ui/command.tsx:155-176`). Mousedown is prevented so the input keeps focus; click runs the item (`CommandPaletteResults.tsx:112-117`).

Description variants used by the palette:

- Thread subtitle (`web/components/ThreadCommandSubtitle.tsx:38-113`): inline-flex, gap 4, text-xs --muted-foreground/70. Parts separated by a "·" span (--muted-foreground/50): [project favicon 12px + project title (truncate) + "·" + environment label] · [workspace icon 12px (--muted-foreground/70): `FolderGit2` when the thread has a worktree path, else `Folder`; branch with middle truncation] · [provider instance icon 12px at 70% opacity] · ["Current thread"]. Renders nothing when there is no project title, branch, provider, or current flag.
- Project location (`CommandPalette.tsx:235-265`): flex gap 4: [machine icon 12px, remote only] location label (truncate) "·" workspace root (truncate). For a grouped project (members > 1): the member environment labels joined " · ".
- Location labels (`:899-920`): primary env "Local"; desktop-local secondary (WSL) "<label> (Local)"; other "<env label>"; unknown env "Remote" with machine `server`.

## 3. Root view

`OpenCommandPaletteDialog` (`CommandPalette.tsx:705-3470`). Groups: "Actions" (value `actions`) then "Recent Threads" (value `recent-threads`), each only if non-empty (`CommandPalette.logic.ts:535-551`).

### 3.1 Actions, in order

(`CommandPalette.tsx:1879-2291`.) "keepOpen" items leave the palette open.

| # | Condition | Title | Icon | Shortcut | Run |
| --- | --- | --- | --- | --- | --- |
| 1 | any project and an active project title (preferred picker group name, else current thread/draft project title) | "New thread in **<title>**" (`<title>` semibold) | `SquarePen` | `chat.new` | new draft in the contextual project (`startNewThreadFromContext`, chat spec) |
| 2 | any project | submenu "New thread in..." | `SquarePen` | (chevron) | pushes §4.1 |
| 3 | a scratch environment exists (below) | "New thread without a project" | `MessageSquareDashed` | `chat.newWithoutProject` | §8.3 |
| 4 | copy target exists (§8.4) | "Copy PR link" or "Copy thread ID"; description = the value | `Link` | `thread.copyReference` | §8.4 |
| 5 | active server thread and its env's PR link mode is not `unsupported` | "Link pull request to thread" | PR link glyph | | opens the Link PR dialog (PR spec) |
| 6 | #5 and env `capabilities.threadPullRequests` | "Show linked pull requests"; disabled when the thread has no visible PRs | PR link glyph | | opens the right panel "pull-requests" tab |
| 7 | active server thread | "Restart agent session" | `RotateCcw` | | §8.2 |
| 8 | always | "Go to file" (keepOpen) | `FileSearch` | `filePicker.toggle` | switch to `files` mode |
| 9 | always | "Search project contents" (keepOpen) | `TextSearch` | `projectSearch.toggle` | switch to `content` mode |
| 10 | a connected env has `newProjectsRoot` | "New project" (keepOpen) | `FolderGit2` | | §7 |
| 11 | always | "Add project" (keepOpen) | `FolderPlus` | | §6 |
| 12 | a connected desktop-local `wsl:*` env (Windows only) | "Open WSL folder"; description = env label | `FolderPlus` | | browse that env (§6.4) |
| 13 | always | submenu "Change theme" | `Palette` | `theme.select` | §4.2 |
| 14 | always | submenu "Change appearance" | `Monitor` | `appearance.cycle` | §4.3 |
| 15 | always | "Toggle theme editor" | `Palette` | `themeEditor.toggle` | settings spec |
| 16 | any env `capabilities.pullRequests` | "Open pull requests" | PR glyph | | navigate `/pull-requests` with saved list prefs |
| 17 | always | "Open usage" | `ChartNoAxesColumn` | `usage.open` | navigate `/usage` |
| 18 | always | "Open settings" | `Settings` | | navigate `/settings` |
| 19 | a contextual project group (active thread/draft's group, else the first sidebar group) | "Project settings"; description = group display name | `Folder` | | navigate `/projects/<projectKey>` |

Search terms (used by §5) are listed at each item's source line; copy them verbatim from `CommandPalette.tsx:1889-2280`.

Scratch environment (`web/hooks/useScratchProject.ts:51-63`): the current thread/draft env if it is connected and has `serverConfig.scratchWorkspaceRoot`, else (no current env) the only connected env offering one, else none.

### 3.2 Recent Threads

First 12 (`RECENT_THREAD_LIMIT`, `CommandPalette.logic.ts:19`) of the thread items (`CommandPalette.tsx:1398-1467`):

- Source: every thread shell in every environment, minus archived, sorted by `sidebarThreadSortOrder` (default `updated_at`) (`CommandPalette.logic.ts:273-278`). Sort key: `updated_at` = `latestUserMessageAt`, else newest user message, else `updatedAt`/`createdAt`; `created_at` = `createdAt`. Ties: id descending (`cr/state/threadSort.ts:79-149`).
- Row: icon `MessageSquare`; leading content = `ThreadRowLeadingStatus` (PR status icon + status pill, sidebar spec); title; trailing content = `ThreadRowTrailingStatus` (running-terminal icon, remote machine icon; sidebar spec); description = thread subtitle (§2.1) with provider = session provider instance, else model selection instance; timestamp = `formatRelativeTimeLabel(latestUserMessageAt ?? updatedAt ?? createdAt)`: "just now" (< 60s or future), "Nm ago", "Nh ago", "Nd ago" (`web/timestampFormat.ts:228-247`).
- Value `thread:<threadId>`; run navigates to `/<env>/<thread>`.

## 4. Submenus

A submenu item pushes a view `{addonIcon, groups, initialQuery?}` onto a stack (`CommandPalette.tsx:1469-1492`): query cleared (or set to `initialQuery`), highlight cleared. Back (`popView`, `:1494-1507`): `ArrowLeft` button, Backspace on an empty query, or clearing the query of a view opened with `initialQuery`. Input placeholder in submenus: "Search..." (`CommandPalette.logic.ts:559-560`).

### 4.1 New thread in...

Group "Projects" (value `projects`) (`CommandPalette.tsx:1314-1396`):

- One row per sidebar project group, excluding the scratch project, ordered like the sidebar picker (`buildSidebarProjectPickerEntries`, sidebar spec). Title = group display name, icon = project favicon 16px, description = project location (§2.1).
- Then "No project" (`MessageSquareDashed`) when a scratch environment exists; runs §8.3.
- The first 9 rows get shortcuts `thread.jump.1`..`thread.jump.9` (mod+1..9, `when: isDesktop`); later rows get none, including "No project" (`CommandPalette.logic.ts:175-185`). While the palette input has focus, mod+N runs the displayed row bound to that command (`CommandPalette.tsx:3041-3051`).
- Run: new draft in the group, keeping the contextual project when it is a member of that group (`:1356-1370`).

Opened directly (`new-thread-in` intent, `:1839-1877`): view stack reset, the current project's row moved first, shortcuts re-enumerated in that order, addon `SquarePen`. The intent waits until there is at least one row.

### 4.2 Change theme

Group label "Change theme" (value `themes`) (`:2104-2151`). Rows: the standard card "T3 Code" first (`web/components/settings/ThemePreviewCircles.tsx:58-67`), then built-in, custom, and environment themes, deduped by id (`CommandPalette.tsx:812-824`).

- Icon `Palette`; title = card label; description "For light mode" / "For dark mode" when the card has one preview.
- Trailing: flex gap 8: ["Current" text-xs --muted-foreground/70 when it is the active theme for the resolved appearance] + preview circles 12px, gap 4 (settings spec).
- Run: single-preview card → set that appearance half; else set the theme. Failure → "Couldn't save theme selection" toast. Closes.
- `theme.select` intent pushes this view directly (`:2184-2197`).

### 4.3 Change appearance

Group "Change appearance" (value `appearance`) (`:2153-2182`): "System" (`Monitor`), "Light" (`Sun`), "Dark" (`Moon`); trailing "Current" on the active mode. Run sets the mode; failure toast as above.

## 5. Search

`filterCommandPaletteGroups` (`CommandPalette.logic.ts:382-462`). The query is React-deferred (`CommandPalette.tsx:718`); natively, filter synchronously.

1. `>` prefix: actions-only mode. Strip it; only the `actions` group is searched (empty remainder lists all actions). The empty text becomes "No matching actions." (`CommandPaletteResults.tsx:31-36`).
2. Normalize (`web/lib/utils.ts:23-25`): Unicode NFKD, drop combining marks, lowercase, collapse whitespace runs to one space, trim. Tokens = split on " ".
3. Empty normalized query: return the active groups unchanged.
4. At the root (no submenu) and not actions-only: drop "Recent Threads", then append, when non-empty, "Projects" (`projects-search`), "Settings" (`settings-search`), "Threads" (`threads-search`).
5. Per group, keep items whose `searchTerms.join(" ")`, normalized, contains every token. Rank each kept item:
   - Walk non-empty terms in order. Field rank: −∞ if the normalized field lacks any token; 3 exact match; 2 prefix; 1 contains the whole query; 0 tokens only (`:334-356`).
   - First term with a finite field rank at index i: if i == 0 and the item has `searchRecency` (threads), rank = 1000 + (exact ? 1 : 0); otherwise rank = 1000 − 100·i + fieldRank. No term matched alone: 0 (`:358-380`).
   - Sort: non-`secondary` before `secondary`, rank desc, `searchRecency` desc, original index asc (`:447-453`).
6. Drop empty groups.

Search groups:

- Projects (`CommandPalette.tsx:1269-1312`): one item per sidebar project group (target = preferred member). Value `project:<env>:<id>`, title = display name, icon favicon 16px, description = project location (§2.1). Terms: display name, title, workspace root, then each member's title, workspace root, location label. Run: newest non-archived thread across the group's members (by sort order) → navigate; none → new draft in the project (`:1224-1267`).
- Settings (`:2294-2313`): `searchSettings(query, availableItems)` (`web/components/settings/settingsSearch.ts:1003-1048`, settings spec owns the index). Value `setting:<id>`, title, description "Settings · <section label>", icon `Settings`, `secondary` passed through. Run navigates to the section with hash = `targetId ?? id`, replacing history when already on that path.
- Threads: all non-archived thread items (§3.2) with terms `[title, ...PR terms, project title, branch, content-match snippet, thread id]` (`CommandPalette.logic.ts:305-313`). PR terms per visible PR link: `#<n>`, `<repo>#<n>`, url, PR title; legacy single link: `#<n>`, `<repo>#<n>`, url (`shared/threadPullRequests.ts:312-326`). `searchRecency` = latest-user-message timestamp.
- Linked threads replace the Threads group when the palette was opened with `linkedThreads` and the query still equals the opening query (`CommandPalette.tsx:2336-2349`, `CommandPalette.logic.ts:24-40`): value `thread:<env>:<id>`, title or "Untitled thread", description "Linked thread" / "Archived thread" (archived threads included), terms `[query, title]`, icon `MessageSquare`.

### 5.1 Thread content search

`useThreadSearch` (`web/state/queries.ts:79-102`), only at the root and not in actions-only mode (`CommandPalette.tsx:847-859`):

- Query trimmed; needs >= 2 chars and at least one connected environment. Debounced 200ms.
- RPC `orchestration.searchThreads {query}` to every connected environment (`cr/state/threadSearch.ts:50-85`), results merged; failed or disconnected envs contribute nothing. Contract: `contracts/orchestration.ts:2299-2322` (`query` 2..200 chars, optional `limit` 1..50; matches `{threadId, projectId, source: "user"|"assistant", snippet <= 240, messageCreatedAt}`).
- A match attaches `threadContentMatch {source, snippet, query}` to that thread's row (keyed by env + thread id) and adds the snippet to its search terms.
- While debouncing or loading: matches empty, `isPending` → the empty state reads "Searching thread messages…" (only visible when no group survives).

### 5.2 Empty state copy

First match wins (`CommandPalette.tsx:3443-3467`):

| Condition | Text |
| --- | --- |
| clone, repository step, Git URL source | "Enter a Git clone URL and press Enter to continue." |
| clone, repository step, provider source | "Enter a repository path and press Enter to look it up." |
| clone, destination step | "Choose a destination path and press Enter to clone." |
| relative path without an active project in that env | "Relative paths require an active project." |
| path does not exist yet | "Press Enter to create this folder and add it as a project." |
| thread search pending | "Searching thread messages…" |
| actions-only | "No matching actions." |
| otherwise | "No matching commands, projects, or threads." |

## 6. Add project

### 6.1 Entry

`openAddProjectFlow` (`CommandPalette.tsx:1783-1809`). Environment options = connected environments only, primary first then by label (`:997-1024`). Option label: primary → its runtime label unless generic ("local", "local environment"), else "This device"; others → label or id (`web/components/BranchToolbar.logic.ts:28-53`).

- No connected environment: close palette, navigate `/settings/connections`.
- More than one: push view (addon `FolderPlus`) with group "Environments" (`environments`): rows = machine icon 16px, title = option label, description "This device" (primary) or the environment id (`:1740-1781`). Run (keepOpen) → §6.2 for that env.
- Exactly one: go straight to §6.2.

A target env that disconnected meanwhile → stacked error "Environment unavailable" / "<label> is not connected." (fallback "The selected environment is not connected.") (`:1704-1718`).

### 6.2 Sources

Pushes a view (addon `FolderPlus`) with group "Sources" (value `sources:<env>`) (`:1590-1702`). The group is rebuilt live from the latest discovery result while shown (`:2314-2323`).

Rows, in order:

1. [env has `newProjectsRoot`] "New project" / "Start a new Git repository from a name" (`FolderGit2`, keepOpen) → §7 with this env as the sources env.
2. "Local folder" / "Browse a folder on disk" (`FolderPlus`, keepOpen) → §6.4.
3. "Git URL" / "Clone from a remote URL" (`Link`) → §6.6.
4. Providers, ready first, then by label: "GitHub repository" / "Clone GitHub owner/repo", "Forgejo / Gitea repository" / "Clone Forgejo / Gitea owner/repo", "GitLab repository" / "Clone GitLab group/project", "Bitbucket repository" / "Clone Bitbucket workspace/repository", "Azure DevOps repository" / "Clone Azure DevOps project/repository". Icons: GitHub, Forgejo, GitLab, Bitbucket, Azure DevOps marks (`web/components/Icons.tsx`), 16px.

Provider readiness (`CommandPalette.tsx:413-463`) from `server.discoverSourceControl {}` (`contracts/rpc.ts:641-645`, result `contracts/sourceControl.ts:111-156`) for the browse environment. Fetched whenever the palette has a browse environment (on open, for the default env) and cached per env (`:1067-1074`):

| Discovery | Ready | Hint |
| --- | --- | --- |
| no data yet, or provider missing from the list | no | "Provider status unavailable. Open Settings -> Source Control and rescan." |
| `status != "available"` | no | provider `installHint` |
| `auth.status == "unauthenticated"` | no | `auth.detail` or "<label> is not authenticated. Open Settings -> Source Control for setup guidance." |
| otherwise | yes | |

A not-ready provider is a disabled row whose trailing content is a "Setup Required" button: `warning-outline` micro (h 20, px 5, gap 4, `rounded-sm`, 11px medium, border warning/32, bg `--warning-surface`, text `--warning-foreground`, `shadow-xs/5`; hover border warning/40 + bg warning/16 (dark warning/24)) (`web/components/ui/button.tsx` micro + warning-outline), pushed right (`ml-auto`). Tooltip side left, align end: the hint, else "Open Settings -> Source Control to configure this provider." Click: close palette, navigate `/settings/source-control` (`:1640-1661`, `:1585-1588`). Note `auth.account/host/detail` are `Schema.Option` on the wire: `{"_tag":"Some","value":..}` / `{"_tag":"None"}` (`contracts/sourceControl.ts:121-126`).

### 6.3 View stack effects

`popView` also cancels any clone or new-project flow, and when leaving the last view resets the chosen environment (`:1494-1507`).

### 6.4 Browse a folder

`startAddProjectBrowse(env)` (`:1518-1548`): initial query = env `settings.addProjectBaseDirectory` with a trailing separator, or "~/" (`:1108-1121`). The first directory listing is prefetched; the view (addon `FolderPlus`, `initialQuery`) is pushed only after it loads, and is dropped if another navigation starts first (`cr/state/filesystem.ts:49-66`).

Browse mode (`cr/state/filesystem.ts:16-30`, `cr/state/projects.ts:80-91`) is on whenever the query starts with `./`, `../`, `.\`, `..\`, `/`, `~/`, or (Windows envs only) a Windows absolute path, and a browse env exists and no clone-repository / new-project step is active (`CommandPalette.tsx:1090-1107`). This also works from the root: typing `~/` in the palette browses the default env. Placeholders: root "Enter project path (e.g. ~/projects/my-app)", submenu "Enter path (e.g. ~/projects/my-app)" (`CommandPalette.logic.ts:553-564`).

- Split: directory = everything through the last separator; filter = the leaf after it (empty when the query ends with a separator); parent of the directory (`cr/state/projects.ts:158-205`).
- Request `filesystem.browse {partialPath: <directory>, cwd?}` (`contracts/rpc.ts:1021-1025`). `cwd` = the active thread's project root when it is in the same env (`:1149-1173`). Skipped while a relative path has no active project in that env.
- Entries shown: names starting with the leaf (case-insensitive); dot-entries only when the leaf starts with "." (`cr/state/filesystem.ts:32-47`). Exact entry = visible entry whose name equals the leaf exactly.
- Group "Directories" (value `directories`): ".." row first when the directory has a parent (`CornerLeftUp`), then one row per entry (`Folder`, title = name), all keepOpen (`CommandPalette.logic.ts:464-504`). Picking a directory sets the query to `<directory><name><sep>` after prefetching it; ".." sets the parent (`CommandPalette.tsx:2735-2779`).
- Nothing is auto-highlighted in browse mode (`:3366-3368`).

Submit (`:3082-3095`): Enter when nothing is highlighted, or mod+Enter when a directory row is highlighted (mod = ⌘ on mac, Ctrl elsewhere, the other modifier must be up, `:3032-3034`). Plain Enter on a highlighted row navigates into it (Base UI clicks the row; it ignores keys with modifiers, `bui/combobox/input/ComboboxInput.js:315-320`, `:379-397`). Path submitted = the browse result's `parentPath` when the query ends with a separator, else the exact entry's `fullPath`, else the raw query (`:2785-2787`).

Input accessory: outline xs button (h 24, px 7, gap 4, text-xs, radius 8, border `--input`, bg `--popover` (dark input/32), `shadow-xs/5`; hover accent/50, dark input/64), `tabIndex -1`, mousedown prevented, absolute right 10, centered (`:3294-3335`):

- Label: "Add", or "Create & Add" when the path will be created (browse settled, query non-empty, no highlighted row, and no listing for a trailing-separator path / no exact entry otherwise, `:2963-2972`). In the clone destination step: "Clone" / "Create & Clone", and "Cloning" while pending.
- Then a Kbd (2px negative right margin): "Enter", or "⌘ Enter" ("Ctrl Enter") while a directory row is highlighted.
- Tooltip (top): "<label> (<Enter | ⌘ Enter>)".
- Disabled when the env is not connected, a relative path lacks an active project, or a clone is pending.
- Input right padding reserves room: 152px when creating, 120 with a highlighted row, else 96 (`CommandPalette.logic.ts:42-53`).

Footer Enter hint: shown as "Select" when a row is highlighted or submit is impossible; otherwise hidden (the accessory carries it) (`:3338-3349`).

### 6.5 Open in Finder

Footer right: ghost-muted xs action, auto height, px 8, text-xs, no hover bg (`ui/command.tsx:205-217`): "Open in Finder" (mac) / "Open in File Explorer" (Windows) / "Open in Files" (`web/lib/utils.ts:27-35`). Shown while browsing, on desktop, when the browse env is the primary env or a desktop-local env whose pool id is known (`:2995-3007`). Disabled while the picker is open.

Click (`:3128-3237`): native folder picker (`LocalApi.dialogs.pickFolder({initialPath?, targetEnvironmentId?})`). Initial path = the browse result's parent path (trailing separator) or the browse directory or the raw query, resolved against the active project root. Cancel/failure: leave the palette open. A pick → add it (§6.7). Windows-only: a `\\wsl$` UNC pick maps to the matching WSL env, else stacked error "Could not add WSL project" / "Start the matching WSL backend, then choose the folder again.".

### 6.6 Clone

`startAddProjectClone(env, source)` pushes a view (addon = source icon, empty `initialQuery`) and sets the repository step (`:1550-1561`). Nothing auto-highlighted; no rows.

Repository step:

- Placeholder: "Enter Git clone URL" (URL) or "Enter <Label> repository (<path hint>)" (`:382-389`).
- Accessory: outline xs "Continue" (URL) / "Lookup" (provider), "Working" while pending, + Kbd "Enter"; tooltip "<Continue|Lookup> (Enter)"; disabled when the query is empty, env disconnected, or pending (`:3267-3293`). Input right padding 128.
- Footer Enter hint: "Continue" / "Lookup".
- Submit (Enter or button, `:2556-2622`): URL source → destination = `<default parent>/<repo folder>`, remote = pasted URL, with GitHub shorthand `owner/repo` expanded to `https://github.com/owner/repo.git` (`cr/operations/projects.ts:115-124`). Provider source → `sourceControl.lookupRepository {provider, repository}` (`contracts/sourceControl.ts:57-70`); failure → stacked "Repository lookup failed" / message; success → destination from `nameWithOwner`, remote = `url` for GitHub and Forgejo, else `sshUrl` (`cr/operations/projects.ts:126-133`). Stale lookups (palette moved on) are ignored.
- Repo folder name = last path segment minus ".git" of the name or URL; empty for host-only URLs (`cr/operations/projects.ts:207-236`). Default parent = §6.4 initial query.

Destination step (`:2624-2733`): the query becomes the destination path and browse mode takes over, with the repo folder pinned:

- Group label "Select where to clone" (`:2800-2806`); the pinned folder name is hidden from filtering, and picking a directory proposes `<dir>/<repo>` unless the directory already is the repo folder (`CommandPalette.logic.ts:506-523`, `cr/operations/projects.ts:284-305`).
- Context block above the list (`:3429-3442`): p 8, pb 0; "Repository" (px 8, py 6, text-xs medium --muted-foreground); row min-h 28, px 8, py 6, gap 8, rounded 6: source icon 16, column [title = `nameWithOwner` or raw input, text-sm --foreground, truncate] [url or remote, text-xs --muted-foreground/85, truncate].
- Validation before cloning: Windows path on non-Windows env → stacked "Clone failed" / "Windows-style paths are only supported on Windows."; relative path without active project → "Clone failed" / "Relative paths require an active project.".
- Server without `capabilities.projectCloneTracking`: `sourceControl.cloneRepository {remoteUrl, destinationPath}` (blocking; "Cloning" shown) → add project at result `cwd` (§6.7). Failure → stacked "Clone failed" / message.
- With clone tracking: `projectClone.start {projectId: new, title: <leaf of destination>, createdAt: now, remoteUrl, destinationPath}` (`contracts/projectClone.ts:60-78`). Failure → "Clone failed". Success → close palette, wait up to 3s for the project to reach the shell, open a new draft in it (failure → "Failed to open project"). Progress then lives in the clone toast (`overlays-notifications.md` §2.4) and the draft's composer banner (chat spec).

### 6.7 Add a path

`handleAddProjectForEnvironment` (`:2352-2489`):

1. Env disconnected → "Environment unavailable" toast.
2. Windows path on a non-Windows env → stacked "Failed to add project" / "Windows-style paths are only supported on Windows.".
3. Relative path without an active project → stacked "Failed to add project" / "Relative paths require an active project.".
4. Resolve relative paths against the active project root; normalize.
5. Existing project with that path in this env: its newest thread unless that thread is settled → navigate; else a new draft (failure → "Failed to open project"). Close.
6. Else dispatch `project.create {projectId: new, title: <last path segment>, workspaceRoot, createWorkspaceRootIfMissing: true, defaultModelSelection: null}`; failure → "Failed to add project" / message (not shown when interrupted). Then a new draft (failure → "Failed to add project"). Close.

## 7. New project

Entry: root "New project" (first env offering `newProjectsRoot`, sources env none) or the Sources row (`:1571-1583`, `:1811-1816`). Pushes a view with addon `FolderGit2`, no groups. The input is the project name: placeholder "Project name", nothing auto-highlighted, right padding 128 (`:2956-2960`, `:3372-3383`).

- Preview block (`:3411-3428`), shown when the env has `newProjectsRoot`: p 8, pb 0; row min-h 32, px 8, py 6, gap 8, rounded 6: `FolderGit2` 16 (--icon-muted), column [name or "New project", text-sm --foreground] [text-xs --muted-foreground/85: "Creates <path preview>" or "Goes in <root>", plus " on <env label>" when the env choice is shown]. Path preview = `<root>/<folder name from name>` (`cr/operations/projects.ts:254-260`; the server may append -2, -3).
- Groups below the name, in order (`:2862-2954`):
  - "Environments" (`new-project-machines`), when more than one env offers new projects, or the chosen env vanished and another exists: env rows (§6.1 shape) with a trailing `Check` 16 (--muted-foreground/70) on the chosen one; all disabled while creating. Choosing one switches the env and keeps the typed name.
  - "Options" (`new-project-options`), when GitHub is ready on the env and a root exists: "Create private repository on GitHub", description = `<account>/<folder>` once a name is typed, else the account or "Your GitHub account"; GitHub icon; trailing checkbox (not focusable) showing the toggle; keepOpen; toggles (`cr/operations/projects.ts:262-282`).
  - Unlabeled group (`new-project-existing`): "Add existing project" / "Open a folder or clone a repository", `FolderPlus`, trailing `ChevronRight`; replaces this step with the Sources view of the chosen env (`:2827-2838`).
- Accessory: outline xs "Create" ("Creating" while pending) + Kbd "Enter"; tooltip "Create (Enter)"; disabled until a non-empty name and a connected env (`:3239-3266`).
- Footer Enter hint: "Create" (nothing highlighted) / "Toggle" (GitHub row) / "Select".
- Enter with nothing highlighted (ignoring IME composition) creates; a second Enter while creating is ignored (`:3061-3080`, `:2518-2535`).
- Create (`web/hooks/useNewProject.ts:76-146`): `projects.createNew {name}` (`contracts/project.ts:291-303`). Failure → stacked "Could not create the project" / message. Success → stacked success "Created <name>" / workspace root, or warning "Created <name> without a first commit" / "<commitError> The project is in <root>."; if GitHub was ticked, `sourceControl.publishRepository {cwd, provider: "github", repository, visibility: "private"}` in the background → stacked success "Published to GitHub" / `nameWithOwner`, or error "Could not create the GitHub repository" / "<message> Use Publish Repository in the Git menu to try again."; wait for the project in the shell (failure: "Failed to open project" / "<message> It will appear in the sidebar once this client catches up."), then open a draft. Palette closes on success.

## 8. Action details

### 8.1 Running an item

`executeItem` (`:3103-3126`): disabled → nothing; submenu → push; else close unless keepOpen, then run. A thrown error → stacked error "Unable to run command" / message or "An unexpected error occurred.".

### 8.2 Restart agent session

(`:1976-2017`.) If the thread has a session that is not `stopped`: dispatch `thread.session.stop {threadId}`. Then success toast "Agent session will restart" / "Your next message starts a fresh session.". Then `server.refreshProviders {instanceId: <session or model instance>, cwd: <worktree path or project root>, fresh: true}`. Failures surface as "Unable to run command".

### 8.3 Thread without a project

(`web/hooks/useScratchProject.ts:65-98`.) `projects.ensureScratch {}` → `{projectId}` (`contracts/project.ts:286-289`); wait for the project in the shell; open a draft. Failure → stacked "Could not start without a project" / message.

### 8.4 Copy thread reference

Target (`shared/threadReference.ts:13-40`, `CommandPalette.tsx:752-797`): on `/pull-requests` with a PR-capable env, the open PR panel's URL (none while it loads); else the active thread's current PR link URL, else its legacy linked/branch PR URL → kind PR ("Copy PR link", toast "PR link copied" / url; failure "Failed to copy PR link"); else the thread id ("Copy thread ID", "Thread ID copied", "Failed to copy thread ID"). Success toast is plain success with the value as description; failure is stacked error.

`thread.copyReference` (mod+shift+c) while the palette input is focused: close and copy (`:3052-3059`).

## 9. Palette keyboard

Input `onKeyDown` (`:3036-3101`) runs before Base UI's handler:

| Key | Effect |
| --- | --- |
| mod+1..9 (`thread.jump.N`) | run the displayed row bound to it (§4.1) |
| mod+shift+c | copy thread reference, close |
| Enter in clone repository step | submit repository |
| Enter in new project, nothing highlighted, not composing | create |
| Enter (browse, nothing highlighted) / mod+Enter (row highlighted) | submit path (§6.4) |
| Backspace on empty query in a submenu | back |
| Arrow up/down | move highlight; wraps (`loopFocus` default true, `bui/combobox/root/AriaCombobox.js:75`); scrolls into view |
| Enter with a highlighted row | click it (Base UI) |
| Esc | close (or back to `command` from files/content) |

Highlight rules (`ui/command.tsx:78-92`, `CommandPalette.tsx:3366-3368`): `autoHighlight: always` (first row highlighted on open and after each query change) except in browse, clone, and new-project steps; hover highlights (`highlightItemOnHover` default true); `keepHighlight` keeps it when the pointer leaves. The content remounts (resetting highlight) on every view push/pop and browse navigation (`:3364`).

## 10. File picker (`files` mode)

`web/components/files/ProjectFilePicker.tsx`. Target (`web/hooks/useActiveProjectTarget.ts`): the active thread or draft, its project, and `cwd` = worktree path or project root.

- No target: disabled input, placeholder "Search files…", body "Open a project to search its files." (py 40, centered, text-sm muted) (`:53-69`).
- Otherwise (`:71-153`): placeholder "Search files…", first row auto-highlighted, `tall-list` panel. Footer: Navigate / Enter "Open file" / Esc "Back".
- Data: `projects.searchEntries {cwd, query: trimmed, limit: 200, kind: "file"}`, debounced 120ms, also for an empty query (`web/state/queries.ts:225-269`, `web/components/files/projectFilesQueryState.ts:173-198`). Results hidden while pending. Server order kept; only `kind == file` (`ProjectFilePicker.logic.ts:45-73`).
- Group label = project title (value `project-files`). Row: file-type icon (Pierre entry icon, files spec) 16px; title = file name; description = relative path.
- Highlighting when the matched query has non-space text: query = trimmed, leading `@`, `.`, `/` stripped, lowercased, whitespace removed; first in-order subsequence match per string; matched chars semibold --foreground, the rest of that string --muted-foreground; no match → no highlight (`ProjectFilePicker.logic.ts:22-37`, `ProjectFilePicker.tsx:23-44`).
- Empty text: error message; pending: "Searching workspace files…" (query) / "Indexing workspace files…"; else "No matching files." / "No files found." (`:46-51`).
- Run: close, open the file in the target thread's right panel (panels spec).

## 11. Content search (`content` mode)

`web/components/search/ProjectContentSearchDialog.tsx`. Popup fixed 420 tall (`CommandPalette.tsx:677`); panel fills (`flex-1`). Remounted (query and options reset) when the target workspace changes (`:310-323`).

- No target: disabled input "Search project contents…", body "Open a project to search its files." (centered, px 24, text-sm muted) (`:83-100`).
- Input: placeholder "Search in <project>", aria "Search file contents in <project>", input box right padding 120 (`:164-196`).
- Accessory (`:169-193`): absolute right 10, centered; flex gap 2, `rounded-md` (8), 1px border, bg muted/30, p 2. Three segmented toggles (`web/components/ui/toggle.tsx`: h 24, px 10, `rounded-md`, text-xs, mono glyph; off: --muted-foreground, hover bg background/55 (dark input/32) + --foreground; on: bg --background (dark input/72), --foreground, `shadow-xs/10`): "Aa" (tooltip "Match case"), "ab" underlined (2px, offset 2) ("Match whole word"), ".*" ("Use regular expression"). Tooltips side top.
- Data (`web/state/queries.ts:284-317`): `projects.searchContents {cwd, query (untrimmed), limit: 500, caseSensitive, wholeWord, useRegex}` debounced 120ms when the trimmed query is non-empty. Invalid regex = `useRegex` and the result has `regexFallbackError`.
- Status bar, when there is a query, pending, error, or invalid regex (`:226-240`): h 36, border-bottom, px 12, text-xs --muted-foreground: spinner 14px + "Searching…" | error (--destructive) | "Invalid regular expression" (--destructive) | "<n>[+] results in <m> files" (locale-formatted; "+" when truncated). The spinner is continuously animated.
- Empty body (`:242-247`): centered, px 24, text-sm muted: "No results found." (query, settled, no error) else "Type to search across your project.".
- Results (`:248-305`): ScrollArea with scroll fade; py 8; one section per file (pb 8), in result order:
  - Sticky header: h 32, px 12, gap 8, text-xs, bg popover/95 + backdrop blur 8: file icon 14px, name (medium --foreground), directory (truncate --muted-foreground), count pill (margin-left auto, `rounded-full`, bg --muted, px 6, py 2, 10/14 tabular --muted-foreground).
  - Match row (button): h 28, px 12, gap 12, mono text-xs, left aligned; hover bg accent/60; selected bg --accent + --accent-foreground; disabled (no pointer) while pending. Line number: w 40, right aligned, tabular --muted-foreground/70. Line: truncate, whitespace preserved, syntax highlighted (diff theme for the resolved appearance), match ranges merged and drawn as `mark` with bg primary/25, radius 2, inheriting token color (`HighlightedSearchLine.tsx:30-183`).
  - Rows render in windows of 100; a 32px sentinel at the end grows the window when it scrolls into view or the selection passes it (`:23-29`, `:134-153`).
- Keys (`:197-218`): ArrowDown/ArrowUp move the selection with wraparound and scroll it into view (nearest); mouse enter selects; Enter opens the selected match unless a newer query is pending (then swallowed). Selection resets to 0 when results change.
- Open: close the overlay, open the file at the line in the target thread's right panel.
- Footer: Navigate / Enter "Open file" / Esc "Back".

## 12. Data and API map

| Need | Upstream | t3UI today (main @ 6456079; all in `t3_protocol::methods`) |
| --- | --- | --- |
| keybindings for labels/dispatch | `server.getConfig` + `subscribeServerConfig` | `ServerConfig.keybindings`; resolver in `t3-logic/src/keybindings` |
| thread rows, projects, env status | `orchestration.subscribeShell`, env catalog | `t3_client::ShellState`, `AppState` envs |
| thread content search | `orchestration.searchThreads` | `t3_protocol::methods::SearchThreads` (exists); multi-env fan-out + debounce helper pending (client agent) |
| browse | `filesystem.browse` | `FilesystemBrowse` (exists) |
| add project | `orchestration.dispatchCommand` `project.create` | `ClientCommand::ProjectCreate` (exists) |
| restart session | `thread.session.stop`, `server.refreshProviders` | `ThreadSessionStop`, `ServerRefreshProviders` (exist) |
| file picker | `projects.searchEntries` | `ProjectsSearchEntries` (exists) |
| content search | `projects.searchContents` | `ProjectsSearchContents` (exists) |
| provider readiness | `server.discoverSourceControl` | `ServerDiscoverSourceControl` (Option fields decode to `Option<String>`) |
| repo lookup | `sourceControl.lookupRepository` | `SourceControlLookupRepository` |
| blocking clone | `sourceControl.cloneRepository` | `SourceControlCloneRepository` |
| tracked clone | `projectClone.start` / `.cancel` / `.retry`, `subscribeProjectClones` | `ProjectCloneStart` / `ProjectCloneCancel` / `ProjectCloneRetry`, stream `SubscribeProjectClones` |
| new project | `projects.createNew` | `ProjectsCreateNew` |
| publish | `sourceControl.publishRepository` | `SourceControlPublishRepository` |
| scratch project | `projects.ensureScratch` | `ProjectsEnsureScratch` |
| config fields | `newProjectsRoot`, `scratchWorkspaceRoot`, `settings.addProjectBaseDirectory`, `environment.capabilities.{pullRequests, threadPullRequests, projectCloneTracking}`, `environment.platform.os` | present in `t3_protocol::server::ServerConfig` / `environment.rs` |
| folder picker | desktop `pickFolder` | native `NSOpenPanel` (GPUI `cx.prompt_for_paths`) |

## 13. Reuse map

| Module | Verdict |
| --- | --- |
| `t3-logic/src/command_palette.rs` (branch `settings`) | Needs changes: token matching (every token, not the whole query), field rank 0 for token-only matches, thread title tier `1000 + exact`, `secondary` last, recency tiebreak, Settings group between Projects and Threads, NFKD + mark stripping in `normalize_search_text`, Windows/backslash paths. Placeholders and `RECENT_THREAD_LIMIT` are correct. |
| `t3-app/src/command_palette/mod.rs` (branch `settings`) | Mechanics reusable (global toggle, view stack, highlight + scroll, browse via `filesystem.browse`, `project.create`). Rendering is July-era: no glass popup, footer border instead of the tinted footer, old highlight color (`--accent` instead of `--sidebar-row-hover`), no files/content modes, no clone/new-project flows, no settings/linked-thread search. |
| `t3-snapshots/src/scenes/command_palette.rs` (branch `settings`) | Harness reusable; scenes target July references. |
| `t3-ui` `Kbd`, `Button` (`outline`/`xs`, `warning-outline`/`micro`, `ghost-muted`), `Toggle` (segmented), `Tooltip`, `ScrollArea` | Reuse if they match `design-system` refresh; verify radii (control radius 8) and glass. |
| `t3-ui` `components/dialog.rs` | Backdrop/fade reusable; the palette needs `dialog-glass` (blur not available in GPUI: approximate with opaque `--popover` per the no-backdrop-filter fallback) and 10vh top anchoring, not centered. |
| `t3-app/src/keybindings` | Reusable; add `filePicker.toggle`, `projectSearch.toggle`, `theme.select`, `appearance.cycle`, `themeEditor.toggle`, `usage.open`, `thread.copyReference`, `thread.jump.N` handling while the palette has focus. |

## 14. Reference screenshots needed

All at 1440x900 @2x, dark and light, seeded with the e2e fixture (2+ projects, 13+ threads, one thread with a PR link, one thread with a worktree branch).

1. Palette root, empty query (⌘K from a thread): Actions + Recent Threads, first row active.
2. Palette root, hover on a lower row (active row moved).
3. Query "the" matching actions, projects, settings, and threads (with a content-match row showing "You:"/"Agent:").
4. Query ">" (actions only) and ">zzz" (No matching actions.).
5. Query with no matches ("qqqqqq").
6. "New thread in..." submenu (via ⌘N with 2+ projects): back arrow, ⌘1.. shortcuts, "No project" row.
7. Change theme submenu (⌥⌘A) with "Current" and preview circles; Change appearance submenu.
8. Add project with two connected environments (Environments view).
9. Sources view with GitHub ready and others "Setup Required" (hover the button for its tooltip).
10. Browse `~/` with directories, a highlighted directory (⌘ Enter accessory), and a non-existent leaf ("Create & Add" + "Press Enter to create…").
11. Clone: Git URL repository step (empty and filled); destination step with the Repository block.
12. New project name step with a typed name, Options (GitHub) and "Add existing project".
13. File picker (⌘P) empty query and with a query showing fuzzy highlights; no-project state.
14. Content search (⇧⌘F) with results (sticky headers, count pills, selected row, match marks), "No results found.", and "Invalid regular expression".
15. Enter/exit animation midpoints (100ms) for the popup scale/opacity.

## 15. Open questions / risks

- GPUI has no backdrop blur. The fork's own fallback for no `backdrop-filter` is opaque `--popover` (popup) and plain `--background/60` (backdrop) (`web/index.css:378-380`). Decide with the design agent which to ship.
- `useDeferredValue` lets typing stay responsive while the list re-renders; native filtering must be fast enough to run per keystroke over all threads (hundreds) or move off the main thread.
- Thread content search depends on a 200ms debounce and per-env RPC fan-out; the empty-state text flickers to "Searching thread messages…" only when nothing else matches.
- Discovery (`server.discoverSourceControl`) is fetched for the default env on every palette open in the fork. Natively, fetching lazily on entering Sources/New project is cheaper but makes the first Sources render show every provider as "Setup Required" until it lands, which the fork also does on a cold cache.
- "Open WSL folder", WSL UNC mapping, and `targetEnvironmentId` routing are Windows-only; skip on macOS.
- Theme cards, the theme editor, and settings search items belong to the settings spec; the palette only consumes them.
- Base UI moves the highlight on pointer move over rows; GPUI hover must update the active row the same way and not leave a second hover color.
