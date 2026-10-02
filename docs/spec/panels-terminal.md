# Terminal drawer and terminal surface

> Refreshed against fe7d3092c (2026-10-02). Part of the panels spec; index in docs/spec/panels.md.

The fork renders terminals with libghostty-vt compiled to WASM and a Canvas 2D renderer
(`terminal/ghostty/*`, README `terminal/ghostty/README.md:1-19`). xterm.js is gone. The same view
(`ThreadTerminalDrawer`) renders both the drawer under the chat column (`mode="drawer"`) and the
content of a right-panel terminal tab (`mode="panel"`). Paths and conventions as in `panels.md` §0.

## 0. Primitives used

| Use | Primitive | Resolved (desktop) |
| --- | --- | --- |
| Empty-state button | `Button size="xs" variant="outline"` | h-6 (24px), px 7px, `text-xs`, gap 4px, `rounded-[8px]`, `bg-popover` + `border-input` |
| Toolbar buttons | plain `button` inside `TerminalActionButton` (`ThreadTerminalDrawer.tsx:1028-1048`) | see 2.3; hover label is a `Popover` with `openOnHover` (Base UI open delay 300ms), `tooltipStyle` (`w-fit rounded-md text-xs shadow-md/5`), side bottom, offset 6px, centered, non-interactive |
| Sidebar row close/icon | `PanelTabCloseButton` with a child icon (`components/ui/panel-tab-close-button.tsx:19-38`) | 16x16 `rounded-sm`, `hover:bg-muted`; shows the child icon, swaps to `X` 12px on row hover or keyboard focus; `Tooltip` with the close label (600ms, top) |
| Close confirmation | `ConfirmDialogHost` alert dialog via `localApi.dialogs.confirm` (in-app, not NSAlert) | panels.md §0 |
| Context menu / selection popup | `localApi.contextMenu.show` (in-app DOM menu `contextMenuFallback.ts`) | panels.md §0 |

---

## 1. Placement and mounting

### 1.1 Drawer

- The drawer lives at the bottom of the chat column wrapper, under the timeline and composer, beside
  (not under) the right panel (`ChatView.tsx:10364-10380`; box tree in panels.md §3.1). It spans the
  chat column width.
- One `PersistentThreadTerminalDrawer` is mounted per thread in `mountedTerminalThreadRefs`: the active
  thread when its drawer is open, plus up to 10 hidden threads whose drawers were open
  (`MAX_HIDDEN_MOUNTED_TERMINAL_THREADS = 10`, oldest dropped first;
  `ChatView.logic.ts:62,674-703`, `ChatView.tsx:2153-2166`). Hidden ones render with `display: none`
  so their sessions and scrollback survive thread switches.
- Wrapper per thread (`ChatView.tsx:1232-1272`): `div.grid.shrink-0.overflow-clip`, rows `1fr` when
  visible (active + open), `0fr` when active but closed, `hidden` for non-active threads; inner
  `div.min-h-0.overflow-clip`. Not rendered at all without a project, without a cwd, or when the
  thread is neither active nor open.
- Open/close animation: only when panel animations are active (default off, panels.md §0):
  `grid-template-rows` 0fr <-> 1fr, `ease-out`, `--panel-animation-duration`, and `starting:grid-rows-[0fr]`
  on open. With the default the drawer appears and disappears instantly. The terminal inside keeps its
  full height while clipped (it is not squeezed).

### 1.2 Right-panel terminal surface

- `PersistentThreadTerminalPanel` (`ChatView.tsx:1275-1430`) renders `ThreadTerminalDrawer mode="panel"`
  as the active right-panel surface: one group `{id: surface.id, terminalIds: surface.terminalIds,
  splitDirection}`; `height` ignored (fills the panel content area).
- Drawer and panel terminals are disjoint: the drawer lists the thread's known server sessions minus
  every id held by a panel terminal surface (`ChatView.tsx:943-962`). A terminal never moves between
  the drawer and the panel.
- In the panel, "New terminal" creates a new terminal tab (`addTerminalSurface`, panels.md §2.2); split
  adds to the same tab (max 4); close removes the id from the tab and the tab when empty.

### 1.3 Data per terminal

| Field | Source |
| --- | --- |
| Label | `resolveTerminalSessionLabel(id, summary)`: the server summary `label` if non-blank, else `Terminal N` for `term-N` (`shared:terminalLabels.ts:4-23`). The server sets `label` to the running child command while a subprocess runs, else `Terminal N` (`upstream:apps/server/src/terminal/Manager.ts:359-367`). |
| cwd / worktree / env | the latest launch context (a script run) else the session summary's `cwd`/`worktreePath`, else `projectScriptCwd` = `worktreePath ?? project.workspaceRoot`; env `projectScriptRuntimeEnv` = `{T3CODE_PROJECT_ROOT, T3CODE_WORKTREE_PATH?}` (`shared:projectScripts.ts:49-71`, `ChatView.tsx:963-1050`) |
| Known sessions | stream `subscribeTerminalMetadata` (`TerminalMetadataStreamEvent`: `snapshot {terminals}`, `upsert {terminal}`, `remove {threadId, terminalId}`), folded by `applyTerminalMetadataStreamEvent` (`crt:state/terminalSession.ts:191-213`) |
| Output | stream `terminal.attach` per visible viewport (section 5) |

---

## 2. Drawer layout (`components/ThreadTerminalDrawer.tsx`)

### 2.1 Outer box (`:1424-1442`)

```
aside [data-thread-terminal-drawer] [data-terminal-owner="drawer"|"right-panel"]
      relative flex min-w-0 flex-col overflow-hidden bg-background
      drawer: shrink-0 border-t border-border/80, style height = drawerHeight px
      panel:  h-full flex-1 (no border, no resize strip)
├─ resize strip (drawer only): absolute inset-x-0 top-0 z-20 h-1.5 (6px) cursor-row-resize, invisible
├─ floating action group (only when the thread has exactly one terminal: no sidebar)   2.3
└─ div.min-h-0.w-full.flex-1
    └─ div.flex.h-full.min-h-0  bg-(--terminal-background)  [gap-1.5 (6px) when the sidebar shows]
        ├─ div.min-w-0.flex-1   > one viewport, or the split grid (2.4)
        └─ sidebar (when the drawer/surface holds more than one terminal)                2.5
```

### 2.2 Height and resize (drawer only)

- Default 280px (`types.ts:28`), stored per thread in the terminal UI store.
- Clamp: `min(max(round(h), 180), max(180, floor(window.innerHeight * 0.75)))`
  (`:93-105`). Non-finite heights read as 280.
- Drag the 6px strip at the drawer's top edge (left button only, pointer capture): height =
  `startHeight + (startY - y)`, clamped every move, applied live. On release, if it moved, the height
  is written to the store and every viewport re-fits (`resizeEpoch`) (`:1309-1354`).
- Window resize: re-clamp; if it changed, apply it, persist it when not dragging, and re-fit
  (`:1356-1377`). Unmount persists the current height.
- No visual indicator on the strip; only the row-resize cursor.

### 2.3 Single-terminal floating toolbar (`:1444-1488`)

Shown when the drawer (or panel tab) has exactly one terminal:

- Position `pointer-events-none absolute right-2 top-2 z-20` (8px from the top-right of the drawer),
  inner `pointer-events-auto inline-flex items-center overflow-hidden rounded-md border
  border-border/80 bg-background shadow-xs`.
- Four buttons, each `p-1` (4px) around a 13px icon (`size-3.25`): 21x21; `text-foreground/90`,
  `transition-colors`, hover `bg-accent`. Separated by `h-4 w-px bg-border/80` (16px x 1px) dividers.
  1. `SquareSplitHorizontal`: `Split Terminal Horizontally (⌘D)`
  2. `SquareSplitVertical`: `Split Terminal Vertically (⇧⌘D)`
  3. `Plus`: `New Terminal (⌘N)`
  4. `Trash2`: `Close Terminal (⌘W)` (asks for confirmation, section 4)
- Labels are the `aria-label` and the hover popover text. Without a binding the parenthesized part is
  omitted. When the active group already has 4 terminals the split labels become
  `Split Terminal Horizontally (max 4 per group)` / `Split Terminal Vertically (max 4 per group)`, the
  buttons get `cursor-not-allowed opacity-64` and no hover fill, and clicks do nothing (`:1253-1276`).
- Shortcut labels are only passed while the drawer is visible; they use the terminal-focus context
  (`ChatView.tsx:3847-3863`).

### 2.4 Splits (`:1497-1586`)

- A group holds 1-4 terminals (`MAX_TERMINALS_PER_GROUP = 4`, `types.ts:30`). Only the active group
  is shown.
- One terminal: a single viewport filling the area (keyed by terminal id, so switching terminals
  remounts the viewport).
- Several: CSS grid, `gap 0`, `overflow-hidden`:
  - horizontal split ("Side by side", default): `grid-template-columns: repeat(n, minmax(0, 1fr))`;
    each pane after the first has `border-l`.
  - vertical split ("Stacked", `splitDirection: "vertical"`): `grid-template-rows: repeat(n,
    minmax(0,1fr))`; each pane after the first has `border-t`.
  - Pane border color: `border-border` for the active pane, `border-border/70` otherwise.
  - Mouse down on a non-active pane makes it active (and it receives focus).
- New splits insert right after the active terminal in its group; a group's direction is whatever the
  last split requested (a horizontal split clears `vertical`) (store, section 3).

### 2.5 Sidebar (more than one terminal, `:1588-1711`)

```
aside  flex w-36 min-w-36 flex-col border border-border/70 bg-muted/10      (144px wide)
├─ header  flex h-[22px] items-stretch justify-end border-b border-border/70
│   └─ inline-flex h-full items-stretch: 4 buttons (same icons/labels/disabled rules as 2.3),
│      each inline-flex h-full items-center px-1, text-foreground/90, hover bg-accent/70,
│      buttons 2-4 have border-l border-border/70
└─ list  min-h-0 flex-1 overflow-y-auto px-1 py-1
    └─ per group (pb-0.5):
        ├─ group header (only when there is more than one group, or any group has >1 terminal)
        │   button flex h-[22px] w-full items-center gap-1 rounded px-1.5 text-2xs cursor-pointer
        │   active group: bg-accent/50 text-foreground; else text-muted-foreground,
        │   hover bg-accent/40 + text-foreground
        │   ├─ icon 12px: Square (1 terminal) / SquareSplitVertical (stacked) / SquareSplitHorizontal
        │   ├─ label flex-1 truncate: `Single` / `Stacked` / `Side by side`
        │   └─ count text-3xs tabular-nums text-muted-foreground/70
        │   click: activates the group's active terminal (or its first)
        └─ rows flex flex-col gap-0.5:
            row group/tab flex h-6 w-full items-center gap-0.5 rounded-md pr-2 pl-1.5 text-xs
            active: bg-accent text-foreground; else text-muted-foreground, hover bg-accent/60 +
            text-foreground
            ├─ PanelTabCloseButton (TerminalSquare 12px, X on hover): label and tooltip
            │   `Close <label>` + ` (⌘W)` on the active row only; click asks for confirmation
            └─ button flex-1 truncate text-left: the label; click activates
```

### 2.6 Empty state (`:1392-1420`)

When the drawer/panel has no terminal ids: same outer box (drawer keeps its resize strip), content
`flex min-h-0 flex-1 flex-col items-center justify-center gap-3 px-4 py-6 text-center text-sm
text-muted-foreground`: `No terminal sessions for this thread yet.` and an outline xs button
`New Terminal (⌘N)`. Rare in practice: opening the drawer with no terminals creates `term-1`.

### 2.7 Viewport box (`TerminalViewport`, `:980-986`)

`div` `tabIndex=-1`, `relative h-full w-full overflow-hidden bg-(--terminal-background)`, containing
the canvas (`block size-full cursor-text`), a hidden 1x1 textarea at (4,4) for keyboard/IME
(`aria-label="Terminal input"`), and the scrollbar (section 6.6) (`terminal/ghostty/surface.ts:678-702`).
If the WASM fails to load, the box shows the plain text `<error message> — close and reopen the
terminal to retry.` (`:905-915`).

---

## 3. State (`terminalUiStateStore.ts`, unchanged logic in fe7d3092c)

Per-thread `{terminalOpen, terminalHeight, terminalIds[], activeTerminalId, terminalGroups[{id,
terminalIds[], splitDirection?}], activeTerminalGroupId}` persisted under localStorage
`t3code:terminal-state:v1`, version 4 (`:20-30`, `:772-773`), plus session-only
`suppressedTerminalIdsByThreadKey` (closed ids hidden from stale server metadata until reopened).
Already ported as `crates/t3-logic/src/terminal_layout.rs` on branch `tools-logic-land` (see the
tools handoff). Key rules to keep:

- Opening the drawer with no terminals creates `term-1` in a new group (`setThreadTerminalOpen`).
- `new` adds a new group `group-<id>`; `split` inserts after the active terminal in the active group;
  refused (no change) when the group already has 4.
- Closing the active terminal activates the terminal now at the same index (else the last); closing
  the last terminal resets the thread to the default state (drawer closed).
- `reconcileTerminalIds(serverIds)` adopts the server order unless the server list is a strict subset
  of the client list (a fresh open the server hasn't reported yet) (`ChatView.tsx:1003-1013`).
- Ids are allocated on the client: lowest free `term-N` across the drawer store, known server
  sessions and every panel terminal (`shared:terminalLabels.ts:27-37`, `ChatView.tsx:985-997`).

---

## 4. Actions and flows

| Action | Drawer (`ChatView.tsx:1052-1196`) | Panel (`ChatView.tsx:5000-5087`) |
| --- | --- | --- |
| Toggle (`⌘J`, titlebar toggle) | Opening with no terminals: allocate an id, `ensureTerminal(id, {open})`, RPC `terminal.open {threadId, terminalId, cwd: gitCwd ?? workspaceRoot, worktreePath?, env}`. Else flip `terminalOpen` (`:4108-4140`). | n/a |
| New | allocate id, store `newTerminal`, focus, `terminal.open` | `addTerminalSurface`: new tab + `terminal.open` |
| Split H / V | allocate id, store `splitTerminal(Vertical)`, focus, `terminal.open` (no-op at 4) | `rightPanelStore.splitTerminal(surface, id, dir)` + `terminal.open` (no-op at 4) |
| Activate | store `setActiveTerminal`, focus | `rightPanelStore.activateTerminal`, focus |
| Close (button, row X, `⌘W` in a terminal) | confirm, then RPC `terminal.close {threadId, terminalId, deleteHistory: true}`; if that fails, write `exit\n` to the terminal as a fallback; store `closeTerminal`; refocus | confirm, then `terminal.close {…, deleteHistory: true}`, store `closeTerminal`, `rightPanelStore.closeTerminal` |
| Process exits / session closed | the viewport prints `[terminal] Process exited` or `[terminal] Terminal closed`, then on the next tick closes that terminal without confirmation | same |

Close confirmation (`lib/terminalCloseConfirm.ts:17-41`), in-app alert dialog, destructive confirm:
- one terminal: title `Close terminal "<label>"?`, description `This stops the running process and
  clears its history.`
- several (closing a panel tab with splits): title `Close N terminals?`, description `This stops their
  running processes and clears their histories: "<a>", "<b>".` (active label first).
- Buttons `Cancel` (outline) and `Confirm` (destructive). Escape/backdrop = cancel. While a
  confirmation is pending, a repeated `⌘W` does nothing (`isTerminalCloseConfirmPending`).
- Bulk tab closes (close others / to the right / all) skip it: they go through the agent-browser
  confirm only, and terminal surfaces in a bulk close are cleaned up without the terminal confirm
  (`ChatView.tsx:5223-5262`, panels.md §2.3). Closing a single terminal tab does confirm.

Running a project script (`runProjectScript`, `ChatView.tsx:4263-4373`; UI in panels-header-tools.md):
1. Remember it as the project's last-invoked script (unless told not to).
2. Target cwd = option `cwd` ?? `gitCwd` ?? workspace root; base terminal = the drawer's active id ??
   first known ?? `term-1`. If the base terminal has a running subprocess (summary
   `hasRunningSubprocess`) or a new terminal was requested, allocate a new id.
3. Record a launch context `{threadId, cwd, worktreePath}`, open the drawer, request focus.
4. New terminal: store `newTerminal`, RPC `terminal.open` with `cols: 120, rows: 30`
   (`ChatView.tsx:740-741`) and env incl. extra env; existing: store `setActiveTerminal`, `terminal.open`
   without size (idempotent for a running session).
5. RPC `terminal.write {data: "<command>\r"}`.
6. Failures set the thread error banner: the error message, else `Failed to run script "<name>".`

"Add to chat" (selection): `TerminalContextSelection {terminalId, terminalLabel, lineStart, lineEnd,
text}` (1-based lines from the selection's rows; text with CRLF -> LF and leading/trailing newlines
trimmed) goes to `composerRef.addTerminalContext` (`ChatView.tsx:4095-4100`); the composer spec owns
the resulting chip. The selection is then cleared and the terminal refocused (`:603-607`). Disabled
for hidden drawers.

---

## 5. Session wiring (`TerminalViewport`, `:335-987`; `state/terminalSessions.ts:123-158`)

- Each mounted viewport subscribes `terminal.attach {threadId, terminalId, cwd, worktreePath?, env?,
  providerInstanceId?}` and folds events with `applyTerminalAttachStreamEvent`
  (`crt:state/terminalSession.ts:135-189`):
  - `snapshot` / `restarted`: replace the buffer with `snapshot.history` (trimmed to the last 512 KB,
    `DEFAULT_MAX_TERMINAL_BUFFER_BYTES`), status from the snapshot.
  - `output {data}`: append (same cap; at most 1,024 retained chunks of 16 KB), status `running`.
  - `cleared`: empty the buffer.
  - `exited` / `closed`: status only. `error {message}`: status `error` + message.
  - `activity`: ignored here (metadata carries labels).
- The viewport replays: on mount it writes the whole buffer; afterwards it writes only the delta since
  its cursor, or resets and rewrites when the buffer was replaced (`readTerminalOutputUpdate`,
  `writeTerminalOutputUpdate` `:111-120`). Every write clears the selection.
- New error message -> `\r\n[terminal] <message>\r\n` written into the terminal (`:107-109`).
- Status sync (`:432-448`): entering `exited` (or `closed` after having run) writes `Process exited` /
  `Terminal closed` as a system line, then closes the terminal one tick later; a later `running`
  re-arms it. A session already `exited` at mount still gets the message once.
- Input: every keystroke/paste is RPC `terminal.write {threadId, terminalId, data}`; failure prints
  `[terminal] <message>` (fallback `Terminal write failed`).
- Resize: the surface emits `onResize(cols, rows)` whenever its grid changes; that is the only source
  of RPC `terminal.resize {threadId, terminalId, cols, rows}` (scheduled, latest wins). Refits run on
  mount (+30ms), on drawer height changes, `resizeEpoch` bumps, visibility changes, and the surface's
  own ResizeObserver; if the view was at the bottom it stays at the bottom (`:876-884`, `:963-979`).
- Hidden viewports (`visible=false`) stay attached and keep writing into their grid; they just don't
  paint (`surface.setVisible`).
- Focus: when the drawer becomes visible or a focus request bumps, the active viewport takes focus
  (`:956-961`); after WASM setup completes it focuses only if focus is still inside its box.

RPC shapes (upstream `packages/contracts/src/terminal.ts`; all typed in T3UI's
`crates/t3-protocol/src/terminal.rs` and `methods.rs`):

| Tag | Payload | Result |
| --- | --- | --- |
| `terminal.open` | `{threadId, terminalId, cwd, worktreePath?: string\|null, cols?, rows?, env?, providerInstanceId?}` | `TerminalSessionSnapshot {threadId, terminalId, cwd, worktreePath, status: starting\|running\|exited\|error, pid, history, exitCode, exitSignal, label, updatedAt, sequence?}` |
| `terminal.attach` (stream) | `{threadId, terminalId, cwd?, worktreePath?, cols?, rows?, env?, providerInstanceId?, restartIfNotRunning?}` | `snapshot` then `started`/`output`/`exited`/`closed`/`error`/`cleared`/`restarted`/`activity` events |
| `terminal.write` | `{threadId, terminalId, data}` (1..65,536 chars) | void |
| `terminal.resize` | `{threadId, terminalId, cols, rows}` | void |
| `terminal.clear` | `{threadId, terminalId}` | void (not used by the drawer UI) |
| `terminal.restart` | `{threadId, terminalId, cwd, worktreePath?, cols, rows, env?, providerInstanceId?}` | snapshot (not used by the drawer UI) |
| `terminal.close` | `{threadId, terminalId?, deleteHistory?}` | void |
| `subscribeTerminalMetadata` (stream) | `{}` | `snapshot`/`upsert`/`remove` of `TerminalSummary {…, hasRunningSubprocess, label}` |

Stream acks are handled by the transport (`t3-client/src/rpc.rs`), not per feature.

---

## 6. The terminal surface (Ghostty, `terminal/ghostty/surface.ts`, `renderer.ts`, `core.ts`)

### 6.1 Font and grid

- Family: the code font setting by default (`fontFamilyCode`, empty = default), or the terminal font
  when "advanced typography" is on (`appearanceFonts.ts:35-51`). Default stack
  `"SF Mono", "SFMono-Regular", Menlo, Consolas, "Liberation Mono"` + Nerd Font symbol fallbacks
  (bundled `Symbols Nerd Font Mono` woff2) (`surface.ts:20-35`). Proportional families are rejected.
- Size: `fontSizeCode` by default = **13px** (`contracts:settings.ts:147`; the terminal-only setting
  defaults to 12 but is used only with advanced typography). Clamped 6-32.
- Cell metrics (`renderer.ts:62-80`): width = measured advance of `M`; height =
  `max(1, round(size * 1.35), ceil(ascent + descent of "Mg"))` (18px at 13px); baseline centered:
  `round((height - glyphHeight)/2 + ascent)`.
- Content padding 4px on every side (`CONTENT_PADDING`). Grid = `floor((w - 8) / cellW)` x
  `floor((h - 8) / cellH)`, minimum 1x1. No column is reserved for the scrollbar (it overlays).
- Vertical origin: while there is no scrollback the grid sits at the top (y = 4); once scrollback
  exists it anchors to the bottom, the leftover sub-row slack moves above row 0 (`surface.ts:153-169`).
- Scrollback 10,000 rows (`core.ts:12,255`). Device pixel ratio changes re-rasterize.

### 6.2 Colors

`terminalThemeFromApp` (`ThreadTerminalDrawer.tsx:176-237`), re-read whenever `<html>` class/style
changes:

| Role | Value |
| --- | --- |
| background | `--terminal-background` = `--background` (both themes, `index.css:1093,1153`) |
| foreground | `--terminal-foreground` = `--foreground` |
| cursor | `--terminal-cursor`: light `rgb(38 56 78)`, dark `rgb(180 203 255)` |
| selection overlay | `--terminal-selection-background`: light `rgb(37 63 99 / 20%)`, dark `rgb(180 203 255 / 25%)`, painted as a translucent fill over cell backgrounds, under text |
| named theme (`html[data-theme-id]`) | `--app-theme-terminal-*` (`index.css:1283-1286`) |
| ANSI 0-15 | libghostty's built-in default palette, the same in light and dark (verified in `vendor/ghostty-vt.wasm`): `#1d1f21 #cc6666 #b5bd68 #f0c674 #81a2be #b294bb #8abeb7 #c5c8c6 #666666 #d54e53 #b9ca4a #e7c547 #7aa6da #c397d8 #70c0b1 #eaeaea`; 16-255 the standard xterm cube and grays |

### 6.3 Rendering rules (`renderer.ts:96-272`)

- Rows are repainted when dirty; background runs merge equal backgrounds; text runs merge equal
  foreground/bold/italic and are clipped to their cells; bold = weight 700, italic = italic face.
- Underline: 1px at `top + cellH - 2`; strikethrough 1px at `top + floor(cellH * 0.55)`; overline 1px
  at `top + 1`. A hovered link draws the underline too.
- Cursor (color = cursor role): unfocused = 1px hollow box; focused by shape: bar 2px wide, underline
  2px tall at the bottom, hollow box, or block (default) with the glyph redrawn in the background
  color. Blink: 500ms on / 500ms off, only while focused, the app requested blinking (default on), and
  reduced motion is off (`surface.ts:139-150`). Stop blinking when hidden; this is the terminal's only
  continuous repaint.

### 6.4 Mouse and selection

- Click-drag selects cells; double-click selects a word; triple-click a line (`surface.ts:1335-1345`).
  Shift-click extends. When the running program enables mouse reporting, plain mouse input goes to the
  program; Shift bypasses it.
- Links: URLs (`https?://…`) and file paths (`~/`, `./`, `../`, `/`, `C:\`, or `a/b/c` with optional
  `:line[:col]`, trailing punctuation and unbalanced closers trimmed) (`terminal-links.ts:34-60`).
  Hover shows the pointer cursor and underlines the link; a plain single click (no Shift) activates it
  on release if still over the same link:
  - URL: opens in the in-app browser preview when the "Open links in" preference is `app` and preview
    is supported, else the system browser; Cmd-click always uses the system browser
    (`components/preview/openTerminalLinkInPreview.ts`). Without preview support (T3UI today) every
    URL goes to the system browser. Errors print `[terminal] <message>` or toast `Unable to open link`.
  - Path: resolved against the terminal cwd (`resolvePathLinkTarget`) and opened in the preferred
    editor (`shell.openInEditor`); failure prints `[terminal] <message>` (fallback
    `Unable to open path`).
- Wheel scrolls the scrollback; on the alternate screen it is translated into arrow keys
  (`surface.ts:1578`).
- Middle click pastes the terminal's own selection (X11 convention, only when something is selected).

### 6.5 Keyboard (`ThreadTerminalDrawer.tsx:745-782`, `keybindings.ts:435-511`)

Order in `beforeKey` (return false = the surface does not handle it):
1. `terminal.close` (`⌘W`): default prevented, then left to the app's handler (section 4).
2. App shortcuts pass through to the app: `terminal.toggle` (`⌘J`), `terminal.split` (`⌘D`),
   `terminal.splitVertical` (`⇧⌘D`), `terminal.new` (`⌘N`), `diff.toggle`.
3. macOS line editing sent as escape sequences: `⌥←` `\x1bb` (word back), `⌥→` `\x1bf` (word forward),
   `⌘←` `\x01` (line start), `⌘→` `\x05` (line end), `⌘⌫` `\x15` (delete to line start). Write errors
   print `Failed to move cursor` / `Failed to delete terminal input`.
4. Clear: `⌃L` or `⌘K` writes `\x0c` (form feed) (`Failed to clear terminal` on error).
5. Everything else goes to libghostty's key encoder (`terminal/ghostty/keyCodes.ts`), including IME
   composition through the hidden textarea.
- Copy: `⌘C` with a selection copies it (the selection stays). Paste: `⌘V` reads the clipboard and
  sends it with bracketed-paste encoding when the program enabled it (`surface.ts:330-400,1060-1140`).
- Root shortcuts other than the ones above do not reach the app while the terminal has focus; the
  `terminalFocus` keybinding context is true.

### 6.6 Scrollbar (`surface.ts:693-702`)

Overlay at `top-1 right-px bottom-1` (4px top/bottom inset, 1px from the right), width
`--app-scrollbar-width` (6px), shown only when there is scrollback. Thumb `inset-x-px` (4px wide),
`rounded-[3px]`, min height 18px, color `--app-scrollbar-thumb` (light `rgb(217 217 217)`, dark
`rgb(255 255 255 / 8%)`), hover or keyboard focus `--app-scrollbar-thumb-hover` (light
`rgb(191 191 191)`, dark `white/12%`), `transition background-color 120ms ease-out`. Draggable;
`role="scrollbar"`, `aria-label="Terminal scrollback"`, focusable.

### 6.7 Context menu and selection popup (`ThreadTerminalDrawer.tsx:250-282,551-730`)

- Right click on the canvas (unless the program captures mouse events): in-app context menu at the
  pointer with, in order: `Add to chat` (only when the drawer can add to the composer), `Copy` (both
  disabled without a selection), `Paste`. Paste goes through the same race-safe paste path as `⌘V`;
  errors print `[terminal] Unable to read the clipboard` / `Unable to copy terminal selection` /
  `Unable to open the terminal context menu`.
- Selection popup: after a mouse selection ends (immediately on single-click drags, after 500ms for
  double/triple clicks so multi-clicks can finish; `lib/selectionActions.ts:3,116-123`), the same
  in-app menu opens near the pointer (clamped 8px inside the viewport and the terminal bounds) with
  `Add to chat` and `Copy`. It closes on Escape, on a new pointer down inside the terminal, on a
  right click (which replaces it), on typing, or when the selection is cleared.

---

## Reuse map

| T3UI module | Verdict |
| --- | --- |
| `crates/t3-terminal` (main): `session.rs`, `input.rs`, `links.rs`, `view.rs` event API (`TerminalEvent::Input/Resize/LinkActivated/SelectionMenuRequested`, `feed_snapshot/feed_output/reset/write_system_message`) | Mechanics fit. The RPC wiring contract in `lib.rs` matches fe7d3092c (snapshot/output/cleared/error/exited/closed). |
| `t3-terminal` visuals (`theme.rs`, `element.rs`, `view.rs` constants) | xterm-era, need changes to match Ghostty: font size from `fontSizeCode` (13px default, not 12); cell height `max(round(size*1.35), ceil(glyph))`; 4px content padding on all sides; no `FIT_SCROLLBAR_RESERVE` (scrollbar overlays, 6px, styled as 6.6); ANSI palette = Ghostty default (6.2) in both themes, not theme-derived; selection = translucent overlay, not an opaque blend; unfocused cursor = 1px hollow box; bottom-anchored grid once scrollback exists; scrollback 10,000 rows; underline/strike/overline offsets as 6.3. Alacritty can render all of this; the differences are configuration and paint code. |
| `t3-terminal` `links.rs` | Re-diff against `terminal-links.ts` (path pattern now accepts `a/b/c:line:col` without a prefix and trims a trailing `:`). |
| `crates/t3-logic/src/terminal_layout.rs` (branch `tools-logic-land`) | Fits as-is (store logic unchanged). |
| `crates/t3-app/src/terminal_drawer/{session,store}.rs` (branch `tools`, never compiled) | Mechanics reusable (attach across reconnects, 512 KB trim, coalesced resize, deferred exit, `exit\n` fallback, script run). Add: the 10-thread hidden-drawer retention, panel terminals excluded from the drawer, confirm-before-close. |
| `terminal_drawer/view.rs` (branch `tools`) | July visuals: replace with section 2. |
| Close confirmation, in-app context menu | Shared with the overlays spec; do not use NSAlert/NSMenu. `t3-terminal` needs a selection getter (text + line range + end rect) for "Add to chat"/"Copy" (the tools handoff notes it has none). |

## Reference screenshots needed

Seed: default e2e seed, thread `thread-aurora-tour` (project `aurora-web`). Dark and light.

1. `terminal-drawer-single`: press `⌘J` on the thread; run `ls -la` and `git status` so the grid has
   colored output. Shows the 280px drawer, floating 4-button toolbar, prompt glyphs.
2. `terminal-drawer-toolbar-hover`: as 1, pointer on the split button (popover label below it).
3. `terminal-drawer-split-h`: as 1 then `⌘D` (two panes side by side, sidebar with `Side by side`
   group header and two rows, active row highlighted).
4. `terminal-drawer-split-v-groups`: `⇧⌘D` in one group plus `⌘N` for a second group (group headers
   `Stacked` and `Single`, counts).
5. `terminal-drawer-max-split`: a group with 4 terminals, pointer on the split button (disabled label
   `(max 4 per group)`).
6. `terminal-selection-popup`: select two lines with the mouse (popup `Add to chat` / `Copy`).
7. `terminal-context-menu`: right click with no selection (`Add to chat`, `Copy` disabled, `Paste`).
8. `terminal-close-confirm`: click the trash button (alert `Close terminal "Terminal 1"?`).
9. `terminal-link-hover`: `echo https://example.com src/format.ts:3` then hover each link.
10. `terminal-scrollback`: `seq 1 500` then scroll up a little (scrollbar thumb visible).
11. `terminal-panel-tab`: open a terminal from the right panel "+" menu and split it (panel mode, no
    resize strip, sidebar inside the panel).
12. `terminal-drawer-resized`: drag the drawer to its max (75% of the window).
13. `terminal-process-exited`: type `exit` (the `[terminal] Process exited` line flashes before the
    terminal closes; capture with a short delay or a slowed build).
14. `terminal-script-run`: needs a project script (create `test` = `node --test` via the scripts
    control) and run it: new terminal at 120x30 with the command echoed.

## Missing client APIs

None for the terminal itself: `terminal.open/attach/write/resize/clear/restart/close` and
`subscribeTerminalMetadata` are typed in `crates/t3-protocol/src/methods.rs`. Needed elsewhere:
`shell.openInEditor` exists; the "Open links in" preference (`resolveBrowserLinkTargetPreference`) is a
client setting T3UI lacks (only matters once a preview exists).

## Open questions / risks

- Pixel parity with Chromium canvas text: the fork draws text runs with `fillText(text, x, y,
  maxWidth)` clipped per run, so glyphs may compress to fit cell widths. GPUI's shaper will place
  glyphs on its own advances; snap to the cell grid as `t3-terminal` does today.
- Ghostty vs alacritty VT semantics differ in corner cases (kitty keyboard protocol, OSC 8 links,
  grapheme widths). Ghostty's key encoder (`keyCodes.ts`) is the reference for what bytes each key
  sends; compare `t3-terminal/src/input.rs` against it for Alt/Option and function keys.
- The selection popup appears automatically after every mouse selection; on a native app this can
  feel intrusive, but it is the fork's behavior.
- `terminal.close` with `deleteHistory: true` on close is destructive; the confirm dialog is the only
  guard. Bulk tab closes skip it.
