# Chat surface build spec

Build spec for porting the fork's chat surface (ChatView, timeline, markdown, composer, pickers, branch toolbar) to GPUI + gpui-component 0.7. Read it alongside the TSX. Token values live in the tokens doc; this file only names tokens.

## 0. Conventions

- **Paths.** Relative to `~/L-Projects/t3code-again/apps/web/src/` unless prefixed: `shared:` = `packages/shared/src/`, `contracts:` = `packages/contracts/src/`, `crt:` = `packages/client-runtime/src/`, `upstream:` = `~/L-Projects/t3UI-refs/t3code-upstream/`.
- **Breakpoints.** All px values assume a window at least 640px wide (Tailwind `sm`), which covers every realistic desktop window. Values for narrower windows appear in parentheses as `(<640: …)`. Other breakpoints: `md` 768, `lg` 1024. A window ≤980px switches the right panel from inline to sheet (`rightPanelLayout.ts:1`). Mobile-only collapsed composer (`max-sm`) is out of scope for desktop; section 4.12 covers it briefly.
- **Spacing.** 1 Tailwind unit = 4px. `px-[calc(--spacing(2)-1px)]` = 7px.
- **Shorthand in this doc.** Class-like shorthands are already converted to px: `px20` = 20px horizontal padding, `pt-8` = 8px top padding, `max-w-768` = 768px, `size-20` = 20×20px, `gap6` = 6px. Original Tailwind classes appear only inside backticked file quotes.
- **Radius scale.** `--radius` = 10px (`index.css:455`), so: `rounded-sm` 6, `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18, `rounded-3xl` 22, bare `rounded` 4 (verified in compiled CSS), `rounded-full` pill. Arbitrary values (`rounded-[20px]`) are literal.
- **Type.** `text-xs` 12/16, `text-sm` 14/20, `text-base` 16/24, `text-lg` 18/28, `text-xl` 20/28. `leading-relaxed` 1.625, `leading-snug` 1.375, `leading-tight` 1.25, `leading-5` 20px. `tracking-widest` 0.1em. `tabular-nums` = tabular figures.
- **Fonts.** Sans: DM Sans Variable (`main.tsx:8`). Mono: `"SF Mono", "SFMono-Regular", "JetBrains Mono", …` (`index.css:56-57`); JetBrains Mono 400/500 bundled (`main.tsx:9-10`), so on Linux/Windows mono resolves to JetBrains Mono.
- **Colors.** Token names (`foreground`, `muted-foreground`, `border`, `card`, `popover`, `accent`, `primary`, `primary-foreground`, `destructive`, `destructive-foreground`, `warning`, `info`, `info-foreground`, `success`, `input`, `ring`, `secondary`, `muted`, `background`). `X/NN` means token X at NN% alpha. Raw Tailwind palette colors appear as e.g. `fuchsia-500`.
- **Motion.** Durations are given in ms. `prefers-reduced-motion` disables the disclosure, morph and shimmer animations (`index.css:386-405`).

### 0.1 Primitive geometry used everywhere in chat (≥640px)

Source: `components/ui/*.tsx`. Repeated here because every row and control below uses them.

| Primitive | Geometry |
|---|---|
| Button base (`ui/button.tsx`) | radius 10, 1px border, `font-medium`, text 14, gap 8, svg default 16px at 80% opacity, svg `-mx-0.5` (−2px each side), disabled opacity 64%, focus ring 2px `ring` offset 1px |
| Button `default` size | h32 px11 |
| Button `sm` | h28 px9 gap6 |
| Button `xs` | h24 px7 gap4 radius 8 text 12, svg 14 |
| Button `icon` / `icon-sm` / `icon-xs` | 32² / 28² / 24² (icon-xs radius 8, svg 14) |
| Button variants | `default`: bg `primary`, fg `primary-foreground`, inset top highlight white/16%, hover `primary/90`. `ghost`: transparent, hover/pressed bg `accent`, svgs `muted-foreground`. `outline`: border `input`, bg `popover` (dark: `input/32`), shadow-xs/5, hover `accent/50` (dark `input/64`). `destructive-outline`: like outline, fg `destructive-foreground`, hover border `destructive/32` bg `destructive/4`. `secondary`: bg `secondary` |
| Toggle `sm` ghost (`ui/toggle.tsx:16-21`) | h28 min-w28 px5, pressed bg `accent` |
| Badge default / sm (`ui/badge.tsx:17-22`) | h18 min-w18 px3 text12 radius 6 / h16 min-w16 px3 text10 radius 4. `info`: bg `info/8` (dark `/16`) fg `info-foreground`. `secondary`: bg `secondary` |
| Alert (`ui/alert.tsx`) | radius 14, 1px border, px14 py12, text 14, row `flex items-center gap-8`; icon box 16²; title `font-medium`; description `muted-foreground`, gap10; action `flex gap-4`. Variants: `error` border `destructive/32` bg `destructive/4` fg `destructive-foreground`, icon `destructive`; `warning` border `warning/32` bg `warning/4` icon `warning`; `info`/`success` analogous |
| Tooltip popup (`ui/tooltip.tsx:39-46`) | radius 8, border, bg `popover`, text 12, shadow-md/5, inner px8 py4, sideOffset 4, scale 0.98 + fade enter/exit. Open delay: Base UI default (600ms); model picker uses 0 |
| Menu popup (`ui/menu.tsx:48-58`) | radius 10, border, bg `popover`, shadow-lg/5, min-w 128, inner p4, max-h = available height, sideOffset 4 |
| Menu item (`ui/menu.tsx:76-90`) | min-h28 px8 py4 radius 6 text14 gap8, highlighted bg `accent`, svg 16 `muted-foreground` |
| Combobox item (`ui/combobox.tsx:196-210`) | grid `[16px 1fr]` gap8 min-h28 py4 ps8 pe16 radius 6 text14; highlighted bg `accent`; selected bg `accent/50` |
| Kbd (`ui/kbd.tsx:5-14`) | h20 min-w20 px4 radius 4 bg `muted` text12 `muted-foreground` |
| Separator vertical | 1px `border` |

---

## 1. Routes and top-level states

| Route | File | Behavior |
|---|---|---|
| `/_chat` layout | `routes/_chat.tsx:151-170` | Redirects to `/pair` unless auth gate is `authenticated` or `hosted-static`. Mounts `ChatRouteGlobalShortcuts` (section 7). |
| `/` index | `routes/_chat.index.tsx:14-23` | `NoActiveThreadState`, or `HostedStaticOnboardingState` (browser-only, no environments). |
| `/$environmentId/$threadId` | `routes/_chat.$environmentId.$threadId.tsx:13-73` | Renders **nothing** until the env shell snapshot exists (`bootstrapComplete`) and the thread exists (server shell/detail or a draft with that ref). If bootstrap completed, the thread is missing, and the env has any threads → `navigate("/", replace)`. When a server thread has started and a draft for the same ref exists → `finalizePromotedDraftThreadByRef`. Wraps `ChatView routeKind="server"` in `SidebarInset`. |
| `/draft/$draftId` | `routes/_chat.draft.$draftId.tsx:14-83` | Looks up the draft session. Infers the server thread (`promotedTo` or a matching server thread ref); once that server thread has started (`threadHasStarted`: latestTurn ≠ null or messages > 0 or session ≠ null, `ChatView.logic.ts:263-267`) → `navigate("/$env/$thread", replace)`. No draft and no canonical thread → `navigate("/")`. Otherwise renders `ChatView routeKind="draft"`. |

`SidebarInset` (`ui/sidebar.tsx:625-637`): `<main class="app-main-glass flex flex-col flex-1 min-w-0">`; route adds `h-dvh overflow-hidden bg-background`. Glass fallback when blur is unsupported: solid `background` (`index.css:556-566`). Use the fallback in GPUI.

### 1.1 Empty / loading / error / disconnected states

| State | Trigger | Rendering |
|---|---|---|
| Loading | route before bootstrap | blank (route returns `null`) |
| No active thread | `/` or `ChatView` with no `activeThread` (`ChatView.tsx:5012-5014`) | `NoActiveThreadState.tsx:7-44`: header (topbar 52px, `border-b border`, px20 (<640: px12)); Electron header text "No active thread" 12px `muted-foreground/50`. Body: `Empty flex-1` centered; box `max-w-lg` (512) px32 py48; title "Pick a thread to continue" 20px `foreground`; description "Select an existing thread or create a new one to get started." 14px `muted-foreground/78`, mt8. |
| Empty thread | `rows.length === 0 && !isWorking` (`MessagesTimeline.tsx:724-732`) | Centered `<p>` "Send a message to start the conversation." 14px `muted-foreground/30`. No list is mounted. |
| Thread error | `localServerError ?? session.lastError` (server) or local draft error (`ChatView.tsx:1369-1371`) | `ThreadErrorBanner.tsx:7-37` below header: wrapper `pt-12 mx-auto max-w-768`; Alert `error`; `CircleAlertIcon`; description clamped to 3 lines with full-text tooltip (max-w 384, pre-wrap); dismiss = icon-xs ghost `XIcon` `text-destructive` → clears local error. Errors are sanitized (`rpc/transportError` `sanitizeThreadErrorMessage`). |
| Provider not ready | provider snapshot status `warning`/`error` (not `ready`/`disabled`) (`ProviderStatusBanner.tsx:8-55`) | Centered pill under header: wrapper `mx-auto w-fit max-w-[calc(100%-32px)] pt-12`; inner `inline-flex gap-12 radius 14 border px14 py12 text14`. Warning: border `warning/32` bg `warning/4` icon `warning`. Error: border `destructive/32` bg `destructive/4` fg `destructive-foreground` icon `destructive`. `InfoIcon` 16. Title `font-medium`: "`{name}` is unauthenticated" (error + auth unauthenticated) else "`{name}` provider status". Message 3-line clamp `muted-foreground` with tooltip; default copy "Sign in via the CLI to authenticate again." / "`{name}` provider is unavailable." / "`{name}` provider has limited availability." |
| Environment disconnected | active env `connection.phase !== "connected"` (`ChatView.tsx:1567-1583`) | Composer banner (section 4.10) + composer surface `opacity-75`, editor disabled, placeholder `"{envLabel}: {connectionStatusText}"`, send aria "Environment disconnected". Status strings (`crt:connection/presentation.ts:58-77`): Available, Offline, Connecting..., Reconnecting... / "Failed to connect. Reconnecting... Reason: X", Connected, "Connection failed" / "Connection failed. Reason: X". Revert also blocked: "Reconnect {label} before reverting checkpoints." |
| Version skew | server config version ≠ client (`ChatView.tsx:1796-1813`) | Dismissible warning banner (section 4.10). |

### 1.2 Draft threads

- A draft is a client-only session (`composerDraftStore.ts:286-299`): `threadId, environmentId, projectId, logicalProjectKey, createdAt, runtimeMode, interactionMode, branch, worktreePath, envMode ("local"|"worktree"), startFromOrigin, promotedTo`.
- `ChatView` builds a fake `Thread` from it (`buildLocalDraftThread`, `ChatView.logic.ts:30`) using the project default model, falling back to `{instanceId:"codex", model: DEFAULT_MODEL}` (`ChatView.tsx:1353-1366`).
- One draft per logical project is reused (`openOrReuseProjectDraftThread`, `ChatView.tsx:1652-1727`).
- Before the first send, environment, env mode, branch and start-from-origin are editable (BranchToolbar). `envLocked` = messages > 0 or session not stopped (`ChatView.tsx:2366-2370`).
- First send dispatches `thread.turn.start` with `bootstrap.createThread` (section 4.9). The route then swaps to the server URL once the thread shows up as started.
- A server thread with zero messages and no worktree can still override env mode/branch locally (`canOverrideServerThreadEnvMode`, `ChatView.tsx:3657-3676`).

---

## 2. ChatView layout

`ChatView.tsx:5120-5481`. Box tree (≥640px). `W` = chat column width.

```
div.relative.flex.flex-1.min-h-0.min-w-0.overflow-hidden.bg-background          (root, row)
├─ [panelLayoutControls when right panel mounted inline]                        (absolute, see 2.1)
├─ div.flex.flex-col.flex-1.min-w-0 (w-0 when right panel maximized)            (chat column)
│  ├─ header[data-chat-header]  52px, border-b border                           (2.1)
│  ├─ ProviderStatusBanner                                                      (1.1)
│  ├─ ThreadErrorBanner                                                         (1.1)
│  ├─ div.flex.flex-1.min-h-0                                                   (main row)
│  │  └─ div.relative.flex.flex-col.flex-1.min-h-0                              (chat body)
│  │     ├─ div.relative.flex.flex-col.flex-1.min-h-0                           (messages wrapper)
│  │     │  ├─ MessagesTimeline  height = 100% − viewportBottomInset            (section 3)
│  │     │  └─ scroll-to-end pill (absolute, bottom = overlayHeight + 4)        (2.3)
│  │     ├─ composer overlay (absolute inset-x-0 bottom-0 z-20)                 (2.2)
│  │     └─ PullRequestThreadDialog (modal, drafts only)
│  └─ PersistentThreadTerminalDrawer × mounted threads                          (out of scope)
├─ RightPanelTabs inline (window >980px)                                        (out of scope)
├─ RightPanelSheet (window ≤980px)                                              (out of scope)
└─ ExpandedImageDialog (fixed overlay)                                          (3.10)
```

### 2.1 Header / topbar

`ChatView.tsx:5131-5167`, `chat/ChatHeader.tsx:54-138`, `chat/PanelLayoutControls.tsx`.

- **Header box.** `workspace-topbar` = flex, items-center, height `--workspace-topbar-height` 52px (`index.css:11,124-130`). Electron: `drag-region` (window drag; buttons/inputs are no-drag), px20 (<640: px12). With Windows Controls Overlay, right padding = native controls inset (`index.css:39-49`). Browser: padding includes safe-area. `border-b border`. Animates `padding-left` over 180ms `cubic-bezier(0.4,0,0.2,1)`.
- **Collapsed sidebar inset.** When the sidebar is collapsed, padding-left = controls-left 12 + titlebar control 28 + gap 12 = 52px (`workspaceTitlebar.ts:1-2`, `index.css:33-37`). This leaves room for the sidebar toggle in the titlebar.
- **ChatHeader** (`ChatHeader.tsx:79-136`): a container-query root `@container/header-actions`, `flex h-full flex-1 items-center gap-12` (<640 gap8).
  - Left: `flex min-w-0 flex-1 items-baseline gap-8 leading-none overflow-hidden`; `h2` thread title 14px `font-medium foreground` truncate; project name 12px `muted-foreground`, `max-w-[40%]` truncate. Whole block has a tooltip (side top) with the full title.
  - Right (`data-chat-header-actions`): `flex shrink-0 justify-end gap-8` (gap12 when container ≥768px), `padding-right: var(--workspace-chat-header-control-reserve)` = 28·2 + 4 = 60px, or 0 when the controls moved into the right panel. The padding animates 180ms.
    1. `ProjectScriptsControl` (`ProjectScriptsControl.tsx:250-345`): joined `Group` of outline xs "Run {primary script}" (script icon, label visible only when container ≥768px) + separator (hidden <768 container) + outline icon-xs chevron (16) opening a menu of scripts (icon, name, shortcut, hover-reveal gear to edit) + "Add action". With no scripts: outline xs "+ Add action".
    2. `OpenInPicker` (section 5.6). Only when a project exists and the thread's environment is the primary environment (`ChatHeader.tsx:42-52`).
    3. `GitActionsControl` (`GitActionsControl.tsx`, 2k lines, separate spec): outline xs group "Git actions" + icon-xs options chevron. Shown when a project exists.
- **Panel layout controls** (`ChatView.tsx:5016-5040`, `PanelLayoutControls.tsx:25-94`): wrapper `.workspace-titlebar-controls` = absolute, top 0, right 12px, height 52px, `isolation: isolate`, no-drag; a `::before` scrim fades from transparent to `background` over 14px on its left (`index.css:132-151`). The scrim is removed under desktop glass (`index.css:552-554`). Children `gap-4`:
  - Maximize toggle (only when right panel open, entered, and inline): ghost Toggle sm, `Maximize2Icon`/`Minimize2Icon` 14, tooltip "Maximize panel"/"Restore panel size". Mount animates scale 0.94→1 + fade over 70ms (`index.css:153-155,416-426`).
  - Terminal toggle: ghost Toggle sm (28²), `PanelBottomOpenIcon`/`PanelBottomCloseIcon` 14, pressed when open, tooltip (bottom) "Toggle terminal drawer ({shortcut label})". Disabled with no project → "Terminal drawer is unavailable".
  - Right panel toggle: `PanelRightOpenIcon`/`PanelRightCloseIcon` 14, tooltip "Toggle right panel ({shortcut label})" / "Right panel is unavailable". Shortcut labels come from `shortcutLabelForCommand` (`keybindings.ts`), formatted per platform.
  - Placement: rendered inside the header when the right panel is not mounted; otherwise as a root-level absolute sibling so they sit over the panel's title bar (`ChatView.tsx:5122,5146`).
- There is **no environment/branch info in the header** in this fork. Environment, workspace and branch live in the BranchToolbar under the composer (section 6.3).

### 2.2 Composer overlay

`ChatView.tsx:5247-5369`, `index.css:217-269`.

```
div[data-chat-composer-overlay] absolute inset-x-0 bottom-0 z-20 pt-8 (<640 pt-6), pointer-events-none
├─ div aria-hidden .chat-composer-horizontal-inset absolute inset-x-0 top-8 bottom-0 z-0
│  └─ div relative mx-auto h-full w-full max-w-768 overflow-clip rounded-t-[20px]
│     └─ div.chat-composer-shared-blur absolute -inset-32          (backdrop blur 16px)
├─ div.chat-composer-horizontal-inset                               (px20, <640 px12, + safe area)
│  └─ div pointer-events-auto relative z-10 isolate
│     ├─ ComposerBannerStack (mx-auto mb-8 max-w-768)               (4.10)
│     └─ div relative z-10 → ChatComposer                           (section 4)
└─ div.chat-composer-horizontal-inset.chat-composer-lower-chrome relative z-10
      pb-4 when git repo, else pb-12 (<640) / pb-16 (≥640) (+ safe-area-bottom)
   └─ [isGitRepo] div pointer-events-auto → BranchToolbar           (6.3)
```

- `.chat-composer-lower-chrome`: background = `card` at 20% (dark 45%), margin-top −1px, margin-inline-end 6px (the scrollbar width), padding-top 1px (`index.css:222-255`). No blur.
- Shared blur: a backdrop blur over the 768px column, from 8px below the overlay top to the bottom, top corners radius 20. GPUI has no element backdrop blur, so drop it and use the CSS fallback (`@supports not (backdrop-filter)` → solid `card`, `index.css:264-269`).
- **Overlay measurement drives timeline insets** (`ChatView.tsx:1257-1304`, `chat/composerTimelineGeometry.ts:1-27`). A ResizeObserver on the overlay and on the composer surface (`[data-chat-composer-mobile-collapsed]`) computes:
  - `overlayHeight` = ceil(overlay height).
  - `viewportEnd` = clamp(surfaceOffsetTop + 0.75 × surfaceHeight, 0, overlayHeight).
  - `contentInsetEndAdjustment` = ceil(viewportEnd). Passed to the list as extra end padding so the last row can scroll above the composer.
  - `viewportBottomInset` = ceil(overlayHeight − viewportEnd). The timeline viewport height is `100% − viewportBottomInset`.
  - Net effect: the list's visible area extends under the banner stack and the top 75% of the composer surface. The bottom 25% of the composer plus the lower chrome sit outside the scroll viewport.

### 2.3 Scroll-to-end pill

`ChatView.tsx:5218-5243`.

- Wrapper: absolute, `left-1/2 -translate-x-1/2`, `bottom = composerOverlayHeight + 4px`, z-30, `py-6`, pointer-events none. Transition opacity+transform 200ms ease-out. Hidden: `translate-y 8px, scale .95, opacity 0`. Visible: identity.
- Button: `rounded-full border border/60 bg-card px12 py4 text-12 muted-foreground shadow-sm gap-6`, `ChevronDownIcon` 14, label "Scroll to end". Hover: `border` full + `foreground`. Click → `scrollToEnd(animated=true)`.
- Visibility comes from the scroll state machine (3.12).

### 2.4 Other ChatView behavior

- **Focus.** On thread open (and terminal closed) the composer is focused at the end on the next frame (`ChatView.tsx:3599-3607`). Closing the terminal refocuses the composer (`3738-3758`).
- **Type-to-focus.** A printable key (`key.length === 1`, no ⌘/Ctrl/Alt, not composing) pressed outside any input/editable/button/role-control, with no dialog, menu, select, popover or combobox open, terminal not focused and model picker closed, is appended to the composer prompt and swallowed (`ChatView.tsx:272-335,3775-3785`). It is refused while connecting, while an approval is pending, while user input is pending, or while the environment is unavailable (`ChatComposer.tsx:1746-1758`).
- **Completion acknowledgement.** While the window is visible and focused, ChatView marks the thread visited and dispatches `thread.completion.acknowledge {threadId, completedAt}` (`ChatView.tsx:1740-1779`). This clears the sidebar "Completed" pill (section 6.2).
- **Plan sidebar auto-open.** When the setting `autoOpenPlanSidebar` is on and a `turn.plan.updated` arrives for the latest turn, the right panel opens to "plan", unless the user dismissed it for this turn (`ChatView.tsx:3573-3593`).
- **Revert.** `onRevertToTurnCount` (`ChatView.tsx:3905-3962`): blocked while running/sending ("Interrupt the current turn before reverting checkpoints.") or while disconnected. Shows a native confirm: "Revert this thread to checkpoint N?\nThis will discard newer messages and turn diffs in this thread.\nThis action cannot be undone." Then dispatches `thread.checkpoint.revert {threadId, turnCount}`.
- **Interrupt.** `thread.turn.interrupt {threadId, turnId?}` (`ChatView.tsx:4312-4325`, `buildThreadTurnInterruptInput`).
- **Model change on a started thread.** Blocked when the current or next provider has `requiresNewThreadForModelChange` (toast "Start a new chat to change models"). Switching driver kind is blocked once locked (`ChatView.tsx:4830-4937`, `ChatView.logic.ts:281-340`). Locked provider = session `providerName` → thread instance → selected instance, once the thread has started.

---

## 3. MessagesTimeline

`chat/MessagesTimeline.tsx` (2566 lines), `chat/MessagesTimeline.logic.ts`, `session-logic.ts`.

### 3.1 Data pipeline (read model → rows)

Read model (`contracts:orchestration.ts:224-364`): `thread.messages[]` (`id, role user|assistant|system, text, attachments?, turnId|null, streaming, createdAt, updatedAt`), `thread.activities[]` (`id, tone info|tool|approval|error, kind, summary, payload, turnId, sequence?, createdAt`), `thread.proposedPlans[]`, `thread.latestTurn` (`turnId, state running|interrupted|completed|error, requestedAt, startedAt, completedAt, assistantMessageId, sourceProposedPlan?`), `thread.session` (`status idle|starting|running|ready|interrupted|stopped|error, providerName, providerInstanceId, activeTurnId, lastError`), `thread.checkpoints[]` (`turnId, checkpointTurnCount, status, files[{path, kind, additions, deletions}], assistantMessageId, completedAt`).

1. **Messages for display** (`ChatView.tsx:2056-2235`): server messages get attachment preview URLs resolved via asset URLs. Optimistic user messages (added on send) are appended at the end until the server echoes the same id. Blob previews are handed off to server URLs once those preload.
2. **Work log entries** (`session-logic.ts:714-737,787-878`):
   - Sort activities by `sequence` (entries with a sequence sort after those without), then `createdAt`, then lifecycle rank (`*.started` 0, `*.progress`/`*.updated` 1, `*.completed`/`*.resolved` 2), then id (`session-logic.ts:1419-1458`).
   - Skip `tool.started`, `task.started`, `context-window.updated`, summary "Checkpoint captured", and tool events whose `payload.detail` starts with `ExitPlanMode:`.
   - Map each activity to `WorkLogEntry {id, createdAt, turnId, label, detail?, command?, rawCommand?, changedFiles?, tone, toolTitle?, toolData?, itemType?, requestKind?, toolLifecycleStatus?, toolCallId?, sourceActivityKind}`.
     - `tone`: `task.progress` → `thinking`; activity tone `approval` → `info`; else the activity tone.
     - `label`: task summary/detail for `task.*`, else `activity.summary`.
     - `command`: first of `payload.data.item.command`, `item.input.command`, `item.result.command`, `data.command`, or detail for `command_execution`. Shell wrappers are unwrapped (`bash -lc`, `pwsh -Command`, `cmd /c`). `rawCommand` keeps the original when it differs.
     - `detail` has the trailing `<exited with exit code N>` stripped. It is dropped when equal to the heading. For non-command tools, falls back to a raw-output summary ("N files", first line of stdout/content, "N lines").
     - `changedFiles`: up to 12 paths collected recursively from `payload.data` (`path`, `filePath`, `relativePath`, `filename`, `newPath`, `oldPath`, nested `item/result/input/data/changes/files/edits/patch/patches/operations`, depth ≤4).
     - `toolLifecycleStatus` from `payload.status` (`inProgress|completed|failed|declined|stopped`); `tool.completed` without status → `completed`.
     - `toolCallId` from `payload.data.toolCallId`.
   - Collapse consecutive `tool.updated` → `tool.updated|completed` for the same call (same `collapseKey` = `tool:{toolCallId}` or `itemType␟label␟detail`), keeping the first entry's id and createdAt (`session-logic.ts:880-956`).
   - `stabilizeWorkLogEntryPresentation` (`session-logic.ts:315-350`) pins each row to the `createdAt`/presentation id the client first showed, so backfilled lifecycle events never reorder visible rows.
3. **Timeline entries** (`session-logic.ts:1487-1519`): `messages ∪ proposedPlans ∪ workEntries`, sorted by `createdAt` string compare. Work entry id = presentation id (`tool:{toolCallId}` when present).
4. **Rows** (`MessagesTimeline.logic.ts:456-682`, details in 3.2): fold settled turns, group work entries, append "working" and "changed files" rows. Structural sharing (`computeStableMessagesTimelineRows`, `:684-760`) reuses row objects whose content is unchanged, so the virtual list skips re-render.

Pending approvals, user inputs, plans and context window are separate derivations from the same activities (`session-logic.ts:442-652`, `lib/contextWindow.ts:50-96`), consumed by the composer (section 4).

### 3.2 Row derivation rules

`MessagesTimeline.logic.ts`.

- **Terminal assistant message** = the last assistant message per turn. Messages with `turnId = null` are keyed by user-message segment (`:275-299`).
- **Unsettled turn** = `session.activeTurnId` while running, else `latestTurn.turnId` if `completedAt == null || state === "running"` (`:317-329`).
- **Turn folding** (`:336-454`): for every turn that is neither unsettled nor contains a streaming message, all entries except the terminal assistant message are hidden behind a `turn-fold` row placed at the turn's first entry. Label:
  - Latest turn and `state === "interrupted"` → "You stopped after {d}" or "You stopped this response".
  - Else "Worked for {d}" or "Worked".
  - `d` = `formatDuration` (`session-logic.ts:352-366`): <1s "Nms"; <10s "N.Ns" (9.95+ → "10s"); <60s "Ns"; else "Mm" or "Mm Ss".
  - Elapsed = `latestTurn.startedAt→completedAt` for the latest turn; otherwise from the preceding user message `createdAt` (else first entry) to the max of the terminal message `updatedAt` and the last entry end.
  - Expanding a fold (`expandedTurnIds`, local state) shows the hidden entries as normal rows tagged `turnFoldDetail`.
  - When the running latest turn becomes `interrupted` in-session, it is auto-expanded. When a new turn starts, the previous latest turn is auto-collapsed (`MessagesTimeline.tsx:413-438`).
- **Work grouping** (`:547-620`): consecutive visible work entries form a group (id `work-group:{firstEntryId}`).
  - Every entry is a tool call (`workLogEntryIsToolCall`, `session-logic.ts:160-183`) → one `tool-stack` row.
  - Otherwise drop "neutral" entries. If ≤1 remains → one `work` row. Else show only the last entry as a `work` row, preceded by the hidden ones when expanded, followed by a `work-toggle` row (`MAX_VISIBLE_WORK_LOG_ENTRIES = 1`).
- **Message rows**: `showAssistantMeta` (copy + timestamp row) is true only for the terminal assistant message of a settled turn. `revertTurnCount` is attached to user messages (3.4).
- **Changed files row**: after a terminal assistant message whose checkpoint summary has files, a `changed-files` row is emitted once the next entry belongs to a different turn, or at the very end. Edge: the end flush happens after the `working` row, so it can render below "Working…".
- **Working row**: appended when `isWorking` (`ChatView.tsx:1984-1990`: phase running/connecting, optimistic dispatch active, send busy, connecting, or reverting). `createdAt` = `activeWorkStartedAt` (`session-logic.ts:392-408`). The user's local send timestamp wins so the timer never resets.
- **Row keys**: `turn-fold:{turnId}`, `tool-stack:{entryId}`, `work-toggle:{entryId}`, `changed-files:{turnId}`, `working-indicator-row`, message id, plan id, work entry id. Item type for recycling: `message:{role}` or `kind` (`MessagesTimeline.tsx:788-794`).

### 3.3 Row frame (all rows)

`MessagesTimeline.tsx:715-722,1088-1123`.

- List padding: px20 (<640 px12). Header and footer spacers: 16px (<640 12px) (`:157-158`).
- Each item: `mx-auto w-full max-w-768 min-w-0 overflow-x-clip`.
- Row bottom padding: 8px for commentary assistant rows (no meta), `work`, `tool-stack`, `work-toggle`; 16px for everything else.
- Assistant rows carry `group/assistant` (hover reveals meta).
- `turnFoldDetail` rows: `border-s-2 border/70 bg-muted/20 ps12 pe8`, entering with `timeline-disclosure-enter` (opacity 0→1, translateY −4→0, 180ms ease-out, `index.css:332-347`).
- `system` messages render an empty row (no branch for them). Upstream adds `role: "reasoning"` messages, which would also render empty (section 10).

### 3.4 User message row

`MessagesTimeline.tsx:1141-1243` (+ helpers 1784-2171).

```
div.group.flex.flex-col.items-end.gap-4
├─ bubble: relative max-w-[80%] rounded-2xl(18) border border bg-secondary p12
│  ├─ image grid (regular attachments): mb8 grid grid-cols-2 gap8 max-w-420
│  │   tile: overflow-hidden rounded-lg(10) border/80 bg-background/70
│  │     img block w-full h-auto max-h-220 object-cover, cursor zoom-in → ExpandedImageDialog
│  │     no previewUrl: min-h-72 px8 py12 centered name, 11px muted-foreground/70
│  ├─ preview annotation cards (images named "preview-annotation-*")          (6.4)
│  ├─ element context chips: mb8 flex-wrap gap6
│  │   chip: inline-flex gap4 rounded-md(8) border/70 bg-background/70 px6 py2 12px foreground/85,
│  │         MousePointerClickIcon 12, label truncate; tooltip header+body (max-w 384, pre-wrap)
│  └─ CollapsibleUserMessageBody
└─ meta row: flex w-full max-w-[80%] justify-end pe4 12px tabular-nums,
             opacity 0 → 1 on row hover or focus-within (200ms)
   └─ flex gap8: timestamp (12px muted-foreground, tooltip long form)
                 + flex gap2: [Revert] [Copy]
```

- **Parsing the stored text.** Contexts are embedded in the message text as trailing XML-ish blocks. They are stripped for display and rendered as chips/cards:
  - `<element_context>` (stripped first), then `<terminal_context>` (`lib/terminalContext.ts:248-260`).
  - Repeated trailing `<preview_annotation>` blocks (`MessagesTimeline.tsx:1147-1153`, `lib/previewAnnotation.ts:4-5`).
  - A further `<element_context>` (`lib/elementContext.ts:8-9`).
  - Inline terminal labels look like `@terminal-1:3-8` (`lib/terminalContext.ts:119-130`).
- **Body** (`:1886-2118`):
  - `ChatMarkdown` with `lineBreaks` (single newlines become `<br>`), class `text-foreground`.
  - Collapsed when text >600 chars or >8 lines: `max-h-176 overflow-hidden` with a mask fading the bottom 28px to transparent. Footer `mt6`: ghost xs button "Show full message"/"Show less", `h24 px6 radius 8 text12 muted-foreground/72 -ml-4`, hover `bg-muted/55 text foreground/85`.
  - Terminal contexts: if the text contains each `@label:range`, the label is replaced in place by a terminal chip and the surrounding text is rendered as inline markdown segments, whitespace preserved. Otherwise chips are prefixed and followed by the markdown body. Container `whitespace-pre-wrap 14px leading-relaxed foreground`.
  - Review comment blocks (`reviewCommentContext.ts`) render as cards: `rounded-lg border/70 bg-background/70 p12 space-y-8`; file path 12px `font-medium`; "section · range" 11px muted; comment text with skill chips; a fenced snippet via ChatMarkdown, or a unified diff via `@pierre/diffs` `FileDiff` (theme `pierre-light|dark`).
- **Revert button** (`:1245-1268`): shown when `revertTurnCount` is a number. Ghost xs, `Undo2Icon` 12, tooltip "Revert to this message", disabled while reverting or working. The count comes from `ChatView.tsx:2250-2281`: for each user message, the first following assistant message (before the next user message) with a checkpoint summary → `max(0, checkpointTurnCount − 1)`, falling back to the inferred count by completion order.
- **Copy button** (`chat/MessageCopyButton.tsx`): ghost xs, `CopyIcon` 12 → `CheckIcon` 12 `text-primary` for 1000ms, plus an anchored tooltip-style toast "Copied!" (or "Failed to copy" + message) for 1000ms; disabled while in the copied state; tooltip "Copy to clipboard". Copies the **full original text including context blocks** (`copyText = prompt`).
- **Timestamp**: `formatShortTimestamp(createdAt)` = hour + 2-digit minute, 12/24h/locale per setting (`timestampFormat.ts:81-83`). Tooltip: "{time}, {d}{st|nd|rd|th} {Month} {yyyy}" (`timestampFormat.ts:68-79`).

### 3.5 Assistant message row

`MessagesTimeline.tsx:1290-1344`.

- Container `relative min-w-0 px4 py2`.
- `ChatMarkdown text={smoothed} isStreaming={message.streaming}` (3.13). Root color `foreground/80`.
- Empty text that is not streaming renders "(empty response)".
- **Streaming smoothing** (`chat/useSmoothStreamingText.ts`, `chat/smoothStreamingText.ts`): presentation-only pacing of append-only text.
  - Cadence 40ms per paint (`setTimeout` then `rAF`).
  - Each step reveals whole words: budget = clamp(ceil(backlogWords × 40 / 240), 1, 12).
  - An incomplete trailing word is held up to 80ms, then forced out by grapheme-safe character budget clamp(ceil(backlogChars × 40/240), 12, 512).
  - Backlog ≥4096 chars → jump straight to the target.
  - Non-prefix change, stream end, or message id change → show the target immediately.
  - Word segmentation: `Intl.Segmenter("word")`, with a regex fallback.
- **Meta row** (only `showAssistantMeta`): `mt6 flex gap8 12px tabular-nums`, opacity 0 → 1 on `group/assistant` hover or focus-within (200ms). Contents: copy button (only when text non-empty and not streaming, `MessagesTimeline.logic.ts:259-273`) and the timestamp of `updatedAt` (hidden while streaming).

### 3.6 Turn fold row ("Worked for …")

`MessagesTimeline.tsx:1270-1288`.

- Wrapper `border-b border/60 pb8 pt4`.
- Button `inline-flex gap4 radius 8 px4 12px muted-foreground tabular-nums`, hover `foreground`, inset focus ring.
- Content: label + `ChevronRightIcon` (collapsed) / `ChevronDownIcon` (expanded) at 14.
- `aria-expanded`. Click → anchored disclosure toggle with the button as the anchor (3.12).

### 3.7 Tool call stack row (all-tool groups)

`MessagesTimeline.tsx:1417-1598`, `index.css:318-384`.

- `section.group/tool-stack relative -mx4 px4 py2`, aria-label "N tool calls".
- **Viewport** `relative min-h-24 overflow-clip` shows only the latest entry (`SimpleWorkEntryRow`).
- **Morph** when the latest entry id changes within the same group (`shouldMorphToolCallTransition`, `MessagesTimeline.logic.ts:108-120`). Duration 650ms, easing `cubic-bezier(0.22,1,0.36,1)`.
  - Outgoing entry absolutely overlaid, inert, animating opacity 1→0, blur 0→3px, translateY 0→−60%, scale 1→0.985.
  - Incoming entry animating opacity 0→1, blur 3px→0, translateY 70%→0, scale 0.985→1.
  - Suppressed while expanded.
- **History** (expanded and >1 entry): below the viewport in DOM, `space-y-1 pt4`, disclosure-enter animation, older entries oldest first.
- **Toggle**: `absolute top4 left4 z-10 size-20 radius 8 border/40 bg-background/90 shadow-sm backdrop-blur-sm muted-foreground`. Opacity 0 until stack hover/focus; always visible when expanded or on coarse pointers. Hover `bg-accent foreground`. `ChevronDownIcon` 14, rotated 180° when expanded (200ms). aria "Show all N tool calls" / "Collapse tool call history". Click → anchored toggle with the row element as anchor.

### 3.8 Work log rows (mixed/non-tool groups)

- **`work` row** (`:1601-1638`): `section -mx4 space-y-2 px4 py2`. A "Work Log" label (`px2 pb2 11px font-medium muted-foreground/65`) shows only when the group is not all tool-like. Entries are `SimpleWorkEntryRow`s with `space-y-1`. Neutral entries are filtered out.
- **`work-toggle` row** (`:1640-1679`): full-width button `gap6 radius 8 px2 py2 12px/20px`, hover `bg-accent/20`. 20px box holding `ChevronDownIcon` 14 at 70% opacity in `muted-foreground/65`, rotated 180° when expanded. Text `font-medium foreground/82`: "+N previous log entr(y|ies)" or "+N previous tool call(s)", and "Show fewer log entries" / "Show fewer tool calls". Anchored toggle.

### 3.9 SimpleWorkEntryRow (one tool / command / file change / web search / MCP / approval / user-input / reasoning entry)

`MessagesTimeline.tsx:2224-2566`.

- **Container** `flex flex-col radius 8 px2 py2`. When expandable: `role=button tabindex=0`, cursor pointer, hover `bg-accent/20`, Enter/Space toggles, inset focus ring.
- **Line** `flex items-center gap6 select-none`:
  - Icon box 20², icon 14 stroke 1.8 opacity .8.
  - Text `<p>` `flex items-baseline gap6 12px leading-20`: heading (`font-medium foreground/82`, truncate, shrinks) + preview (`flex-1 truncate muted-foreground/55`).
  - Right `flex gap1 muted-foreground/55`: chevron box 16 (`ChevronDownIcon` 12, opacity .7, rotate 180° when expanded) + status box 16.
- **Icon choice** (first match):
  1. `sourceActivityKind` `runtime.warning` → `XIcon`.
  2. `user-input.requested|resolved` → `MessageCircleIcon`.
  3. `requestKind` `command` → `TerminalIcon`; `file-read` → `EyeIcon`; `file-change` → `SquarePenIcon`.
  4. `itemType` `command_execution` or has command → Terminal; `file_change` or changedFiles → SquarePen; `web_search` → `GlobeIcon`; `image_view` → Eye; `mcp_tool_call` → `WrenchIcon`; `dynamic_tool_call`/`collab_agent_tool_call` → `HammerIcon`.
  5. Tone fallback: error → `CircleAlertIcon`, thinking → `BotIcon` (this is the "reasoning" presentation), info → `CheckIcon`, tool → `ZapIcon`.
- **Icon color**: warning → `destructive`; destructive row (failure AND (`runtime.error` or not tool-like)) → `destructive`; tone tool or failed → `muted-foreground/65`; error/thinking/tool-zap → `foreground/92`; info → `muted-foreground`.
- **Heading** = (`toolTitle ?? label`) with trailing " complete"/" completed" removed and the first letter capitalized. Warning heading `text-warning`; destructive heading `text-destructive`.
- **Preview** = command > detail > first changed file (workspace-relative, "+N more"). Hidden when equal to the heading (case-insensitive).
- **Status indicator** (status box, priority order):
  - Failed → `XIcon` 12 `destructive`, tooltip "Failed". Failure = tone error, status failed/declined, or detail/command text matching failure heuristics such as "command not found", ENOENT or a non-zero exit code (`session-logic.ts:186-255`).
  - Success → `CheckIcon` 12, tooltip "Completed". Success = tool-like, not failed, not thinking, status not inProgress/stopped. A neutral entry also shows success once the turn settles.
  - In progress (turn running and status inProgress) → 6px dot `muted-foreground/65`, `animate-pulse`, tooltip "Running".
  - Neutral while the turn is running → `MinusIcon` 12 at 70% opacity, tooltip "Empty".
- **Expanded body** (expandable when any body text exists): `mt4 ms28 border-s border/45 ps12 pt2`, clicks don't toggle. `<pre> max-h-256 overflow-auto whitespace-pre-wrap break-words mono 11px leading-relaxed muted-foreground select-text`. Content = blocks joined by blank lines: "MCP call\n" + JSON(toolData, 2-space) for MCP; raw command (else command); detail; changed files one per line.
- **Approvals and user-input requests appear in the timeline only as these entries.** The interactive UI is in the composer (4.6, 4.7).

### 3.10 Proposed plan row, working row, changed files row, image dialog

- **Proposed plan row** (`:1346-1364`): `min-w-0 px4 py2` → `ProposedPlanCard` (6.1).
- **Working row** (`:1366-1410`): `py2 pl6`; inner `pt4 11px tabular-nums`; `TextShimmer` (6s) containing "Working for {t}" or "Working".
  - Timer text updates every 1s by mutating the text node, with no re-render (`formatWorkingTimer`: "Ns", "Mm Ss", "Hh Mm").
  - Shimmer (`index.css:284-316`): text clipped to a 200%-wide gradient `muted-foreground 30% → foreground 50% → muted-foreground 70%`, background-position animating 200%→−200% linear infinite.
- **Changed files row** (`:1683-1778`, `chat/ChangedFilesTree.tsx:100-224`, `chat/DiffStatLabel.tsx`):
  - Card `mt8 rounded-lg border/80 bg-card/45 p10`.
  - Header `mb6 flex justify-between gap8`: label 10px uppercase tracking 0.12em `muted-foreground/65` "Changed files (N)" + " • " + DiffStat; right `gap6`: outline xs "Collapse all"/"Expand all" (persisted per thread+turn in `uiStateStore.threadChangedFilesExpandedById`) and outline xs "View diff" (opens the diff panel at the first file).
  - Tree rows: `w-full gap6 rounded-xl(14) py4 pr12`, `padding-left = 8 + depth×14`, hover `bg-accent/60`.
    - Directory: `ChevronRightIcon` 14 `muted-foreground/70` (rotated 90° when open), `FolderIcon`/`FolderClosedIcon` 14 `muted-foreground/75`, name mono 11px `muted-foreground/90` (hover `foreground/90`), stat right-aligned mono 10px.
    - File: 14px spacer when any directories exist, Pierre file icon 14, name mono 11px `muted-foreground/80`, stat. Click → diff panel for (turn, file).
  - A directory is open by default when it has ≤3 children.
  - DiffStat: grid `[4ch 4ch]` gap8, right-aligned mono tabular. "+N" in `success`, "-N" in `destructive`. Counts compact as 1.2k / 12k / 1.2m.
- **ExpandedImageDialog** (`chat/ExpandedImageDialog.tsx`): fixed full-window `bg-black/75 px16 py24`; click backdrop closes; image `max-h-86vh max-w-92vw rounded-lg border/70 bg-background object-contain shadow-2xl`; caption 12px `muted-foreground/80` "name (i/n)"; close icon-xs ghost at top-right 8px; prev/next ghost icon buttons (20px chevrons, white/90) at left/right 24px, vertically centered. Keys: Esc closes, ←/→ cycle with wrap-around.

### 3.11 Timeline minimap

`MessagesTimeline.tsx:796-1075`, `MessagesTimeline.logic.ts:12-106`.

- Shown when there are ≥2 user messages and the pointer is fine (mouse).
- Box: absolute top0 left0 z-40 w72, `bottom = contentInsetEndAdjustment`.
- Opacity 0 → 1 on hover or focus (150ms). Always visible when the side gutter `(viewportWidth − min(viewportWidth, 768))/2 ≥ 48`, i.e. viewport ≥864px.
- Rail button: absolute `top 50%` left12 w40, `-translateY(50%)`, height `min((n−1)×8px, 100vh − 288px)`.
  - Vertical line at left12 inside the button, 1px `border/15`.
  - One strip per user message at `top = i/(n−1)×100%`: h2 rounded-full `muted-foreground/35`. In-view strips (row intersects `[scroll, scroll + viewport − contentInsetEnd]`) use `foreground/90`.
  - Strip width by distance from the hovered index: 24 (active, `muted-foreground/75`), 16, 10, else 8. Transition 150ms.
- Hover tooltip card: left32 w320 `rounded-xl border/70 bg-popover/95 p12 shadow-xl backdrop-blur`. User text 14px `font-medium` leading-20, single-line ellipsis. Final assistant text of that turn: mt4, 14px muted, 3-line clamp. Vertical anchor: top/middle/bottom alignment for first/middle/last.
- Click (pointer y → nearest index) or Enter/Space → manual navigation + `scrollToIndex(row, animated, viewOffset 24)`. Keys: ↑/↓ move highlight, Home/End jump.

### 3.12 Virtualization and scroll behavior

**List.** `@legendapp/list` 3.2 `LegendList` (`MessagesTimeline.tsx:742-774`):

- `estimatedItemSize 90`, `initialScrollAtEnd`, `keyExtractor`, `getItemType`.
- `contentInsetEndAdjustment` = composer inset (2.2).
- `maintainScrollAtEnd` = `{animated:false, on:{dataChange, itemLayout, layout}}`, disabled when an anchored end space is active, a disclosure is settling, or live-edge maintenance is off.
- `maintainScrollAtEndThreshold` and `onEndReachedThreshold` = 0.1 (`TIMELINE_END_THRESHOLD`).
- `maintainVisibleContentPosition {data: !suppressed, size: false}`.
- `overflow-anchor:none`, `overscroll-y-contain`. The `scrollbar-gutter-both` class is a no-op (Tailwind 4.3 has no such utility). Scrollbar: 6px, thumb `rgba(255,255,255,0.10)` dark / `rgba(0,0,0,0.15)` light, hover .18/.25, radius 3 (`index.css:633-657`).
- Wrapper height `calc(100% − viewportBottomInset)`. The list is keyed by `activeThreadKey` (env+thread), so each thread gets a fresh list.

**State machine** (ChatView refs, `ChatView.tsx:3286-3571`). Modes: `following-end` | `anchoring-new-turn` | `free-scrolling`.

- `anchorUserScrollGenerationRef` increments on every manual navigation. `liveFollowUserScrollGenerationRef` equals the current generation while following, `null` while free.
- **Manual navigation** (`cancelTimelineLiveFollowForUserNavigation`, `:3299-3312`): mode free, follow generation null, clears all anchor refs. Detected inside MessagesTimeline (`:554-631`) by:
  - `wheel` and `touchmove` on the scroll node;
  - `pointerdown` with button 0 inside the native scrollbar strip;
  - document `keydown` (capture) of ArrowUp/ArrowDown/PageUp/PageDown/Home/End/Space when the target is not editable and no modifier is held;
  - minimap selection.
  - It also turns off `liveEdgeMaintenanceEnabled` (LegendList `maintainScrollAtEnd`). Maintenance turns back on only when a scroll event reports `state.isAtEnd === true` (`MessagesTimeline.logic.ts:45-55`).
- **onScroll** (`:528-542`): `isAtEnd = state.isAtEnd ?? state.isNearEnd` → `onIsAtEndChange` (`ChatView.tsx:3466-3485`):
  - Not at end while still following (same generation) → keep the pill hidden. Content growth is not a user scroll.
  - At end → mode following, pill hidden.
  - Not at end after manual navigation → mode free, pill shown.
- **Follow effect** (`ChatView.tsx:3487-3551`): runs on every `timelineEntries` change and thread switch while following.
  - After 2 animation frames: in `anchoring-new-turn`, scroll by `scrollDeltaToRevealEnd` (`chat/timelineScrollAnchoring.ts:37-78`).
  - In `following-end`, `scrollToEnd(animated:false)`, but only if the real last row's bottom exceeds `scrollLength − contentInsetEnd − 16`. This avoids jumping on short threads.
- **Send**: forces mode following, the current generation and the pill hidden. `ENABLE_SENT_MESSAGE_TOP_ANCHORING = false` (`ChatView.tsx:258-260,4100-4113`). The alternative "pin the sent message to the top" path (LegendList `anchoredEndSpace`, `CHAT_LIST_ANCHOR_OFFSET = 16` from `shared:chatList.ts:1`, `onTimelineAnchorReady/SizeChanged`, `:3374-3464`) is kept but disabled. You can skip it.
- **Thread switch** (`:3553-3571`): reset to following, clear anchors, hide pill.
- **Anchored disclosure toggles** (`MessagesTimeline.tsx:276-409`). Turn-fold, tool-stack and work-toggle clicks keep the anchor element's **bottom edge** at a fixed viewport y:
  1. Record the anchor bottom and the scroll offset.
  2. Toggle synchronously (`flushSync`) with `disclosureToggleSettling = true` (disables `maintainScrollAtEnd`) and `disclosureDataMaintenanceSuppressed = true` (disables `maintainVisibleContentPosition.data`).
  3. Every frame, for up to 60 frames: `scrollTop += currentAnchorBottom − recordedBottom` when |Δ| ≥ 0.5. If the anchor element unmounted, restore the recorded scroll offset instead.
  4. Finish after 2 consecutive stable frames once any adjustment happened.
  5. Data maintenance resumes 250ms after the next `timelineEntries` change that follows the settle.
- **The "stabilize chat scroll" fix** (commit `d28ad86b`; audit at `.plans/2026-07-09_chat_scroll_behavior_audit.html`):
  1. Removed ChatView's one-shot listener attach. On an empty thread no list exists, so the attach found no scroll node, never retried, and the user got "pulled back" after the first message. Detection moved into MessagesTimeline with up to 8 rAF retries, re-armed on `rows.length` / `isWorking` / thread key changes.
  2. Added scrollbar-drag and keyboard navigation detection.
  3. Added `liveEdgeMaintenanceEnabled` so LegendList stops auto-pinning after a manual scroll.
  4. Added the anchored disclosure transaction (replacing a single-frame delta correction).
  5. Keyed the timeline and effects by env-scoped thread key.
  6. Minimap in-view math subtracts the composer inset.
  7. Unified end threshold at 0.1. A later commit (`f56bef11`) simplified `resolveTimelineIsAtEnd` to `isAtEnd ?? isNearEnd`.

**GPUI mapping.** gpui-component 0.7 ships `MessageScroller` (`~/.cargo/registry/src/*/gpui-component-0.7.0/src/message_scroller.rs`) on `gpui::list` + `ListState` with `FollowMode::Tail` and `is_scrolled_up()`. Use it as the base: variable-height rows, tail following, jump button. Then add the generation/manual-navigation rules above and the anchored disclosure.

### 3.13 ChatMarkdown

`components/ChatMarkdown.tsx` (1634 lines), styles `index.css:696-1038`.

**Pipeline** (`:1484-1501`): `react-markdown` with `remark-gfm` (+ `remark-breaks` for user messages) + a custom `remarkPreserveCodeMeta` (fence meta → `data-code-meta`) → `rehype-raw` (raw HTML allowed) → `rehype-sanitize` with GitHub's default schema minus the `title` attribute, plus `file:` hrefs and `dataCodeMeta` on `code` (`:156-167`). `urlTransform` rewrites `file://` URLs to paths.

**Supported syntax:** paragraphs, ATX/setext headings, emphasis/strong, strikethrough, inline code, fenced/indented code, blockquote, ordered/unordered/nested lists, GFM task lists, GFM tables, autolinks, links, images, footnotes, `<details>/<summary>`, sanitized raw HTML. **Not supported:** math, mermaid, emoji shortcodes.

**Root** (`:1604-1631`): `chat-markdown w-full min-w-0 14px leading-relaxed(22.75px) foreground/80` (user messages override to `foreground`); `overflow-wrap:anywhere; word-break:break-word`.

| Element | Style (`index.css` lines) |
|---|---|
| Block spacing | `p, ul, ol, blockquote, pre, table-container`: margin 10.4px 0; first child mt0, last child mb0 (703-718) |
| h1–h6 | margin 20px 0 8px, weight 600, line-height 1.3, `foreground`. h1 20px, h2 18px, h3 16px, h4–h6 14px; h6 `muted-foreground` (720-752) |
| ul / ol | padding-left 20px; ul disc → circle → square; ol decimal → lower-alpha → lower-roman; `li + li` mt 4px (754-782) |
| Task list item | no marker; checkbox margin `0 0.35em 0.15em -1.25rem`, vertical-align middle; read-only in chat (785-792, `:1275-1308`) |
| Link (external) | color `info-foreground`, no underline; hover/focus: dotted underline (radial-gradient dots 4×2px at the bottom) (794-805). Favicon 14×14 from `https://www.google.com/s2/favicons?domain={host}&sz=32`, radius 6, margin-inline 0.25em 0.2em, vertical-align −0.125em; on error `GlobeIcon`, and the host is remembered as failed for the session (807-817, `:828-849`). Text after the protocol breaks at every character (`<wbr>`); favicon + protocol stay nowrap. Tooltip shows href (max-w min(576px, 100vw−32)). Opens in a new window. Desktop context menu: "Open in integrated browser" / "Open in system browser". `#fragment` links scroll the target into view (`block: nearest`) inside the same markdown root (`:905-944`) |
| File link | Any link whose href resolves to a filesystem path (`markdown-links.ts:136-213`: absolute POSIX under /Users, /home, /tmp, …, Windows drive/UNC, `./`, `~/`, `a/b.ext`, `file.ext`, optional `:line:col` or `#L12C3`) renders as an inline **file chip** instead (see below) |
| Blockquote | border-left 2px `border`, padding-left 12.8px, `muted-foreground` (819-823) |
| Inline code | 1px `border`, radius 6, bg `muted`, padding 1.6px 5.6px, `foreground`, 12px mono (854-861) |
| Code block | see below |
| Table | see below |
| Footnotes | section mt 20px, border-top 1px, pt 12px, `muted-foreground`, 12px; refs/backrefs inline-flex min-w 16px radius 4, 11px, 600 (825-852) |
| `<details>` | Collapsible: `my8 border-y border/60`; trigger `w-full gap8 py8 14px font-medium foreground`, `ChevronRightIcon` 16 muted rotating 90° when open; panel `pb12 ps24 foreground/80`; default summary "Details" (`:437-478`) |
| Images, hr | No chat-specific CSS. Tailwind preflight applies: img `display:block; max-width:100%; height:auto`; hr = 1px top border, no margin |
| `$skill` tokens | In paragraph/list text (not inside code or links), `$name` matching a known provider skill becomes a fuchsia skill chip (4.4). Copying keeps the raw `$name` (`chat/SkillInlineText.tsx`) |

**File chip** (`:996-1210`, `chat/FileTagChip.tsx`, `components/composerInlineChip.ts:1-12`):

- Chip: `inline-flex max-w-full gap4 rounded-md(8) border border/70 bg-accent/40 px6 py1 font-medium 12px leading-1.1 foreground align-middle`, hover `bg-accent/70`.
- Content: Pierre file icon 14 at 85% opacity, colored per file type (`chat/PierreEntryIcon.tsx:7-60`, light/dark pairs). Label = basename, plus " · {parent suffix}" when basenames collide (shortest unique parent path, at least 2 segments), plus " · L{line}[:C{col}]".
- Tooltip: workspace-relative path, mono 11px, single line, horizontally scrollable.
- Click: browser-previewable file (html etc.) with a thread → open in the integrated browser. Else, if workspace-relative → right panel file preview at that line. Else open in the preferred editor (error toast "Unable to open file").
- Context menu: "Open in editor", "Open in integrated browser" (when previewable), "Copy relative path", "Copy full path". Copy shows a success toast "{X} copied".

**Code block** (`:521-715`, CSS 878-985):

- Wrapper `.chat-markdown-codeblock`: margin 10.4px 0, overflow hidden, 1px `border`, radius 12, bg = mix(`muted` 78%, `background`), `leading-snug` (1.375).
- Header: flex `justify-between` gap8, border-bottom, bg `muted`, padding `4px 6px 4px 11.2px`, `muted-foreground`, no text selection.
  - Title (mono 11px, gap 6.4px): fence title (from meta `title="x.ts"` / `file=` / `filename=` / a bare `path/name.ext` token) → Pierre icon 14 + name. Otherwise, if the language maps to a specific Pierre icon → icon only, with tooltip = language. Otherwise the language text (default "text"; `gitignore` → `ini`).
  - Actions (`gap2`), both ghost icon-xs (24²) with 12px icons, tooltips on top. Color `muted-foreground`, hover `foreground`; pressed state `foreground` + bg `foreground/8%`.
    - Wrap toggle `WrapTextIcon`: "Wrap lines" / "Disable line wrap". Initial state = client setting `wordWrap`.
    - Copy `CopyIcon` → `CheckIcon` for 1200ms: "Copy code" / "Copied". Copies the raw code.
- `pre`: margin 0, no border, transparent bg, `overflow-x:auto`, padding 12.8px 14.4px, 12px mono. Horizontal scrollbar height 7px, thumb `border` 78% pill. Wrapped: `white-space:pre-wrap; overflow-wrap:anywhere`.
- **Highlighter:** Shiki via `@pierre/diffs` `getSharedHighlighter({themes:[pierre-dark, pierre-light], langs:[lang], preferredHighlighter:"shiki-js"})` (`:277-296`). Theme = `pierre-dark` / `pierre-light` by resolved theme (`lib/diffRendering.ts:5-12`). Theme JSON (VS Code TextMate format, 248 token rules each; editor bg `#0a0a0a` / `#ffffff` is forced transparent) lives at `node_modules/.pnpm/@pierre+theme@1.0.3/node_modules/@pierre/theme/themes/pierre-{dark,light}.json`.
  - Grammars load lazily per language; unknown → `text`.
  - While streaming, highlighting still runs (Suspense fallback = plain `<pre>`), but results enter the LRU cache only when not streaming (500 entries / 50MB).
  - Highlight errors fall back to plain `text`.

**Table** (`:302-435`, CSS 987-1038):

- Container margin 10.4px 0.
- Horizontal ScrollArea with 24px edge fade masks and hidden scrollbars.
- `table`: width 100%, min-width max-content, collapsed borders, 12px, no forced wrapping.
- `th`/`td`: padding 7.2px 12px, left-aligned.
- `thead th`: bottom border `border` 60%, padding-block 8.8px, weight 600, nowrap.
- `tbody td`: bottom border `border` 60%.
- Collapsed (default unless `wordWrap`): cells nowrap + ellipsis, max-width 384. Expanded: td max-width 384, wraps anywhere. Expanding first freezes header cell min-widths to the measured column widths.
- Footer `flex justify-between mt2`, two ghost icon-xs buttons:
  - Expand: `Maximize2Icon`/`Minimize2Icon` 12, "Expand table cells" / "Collapse table cells".
  - Copy: menu "Copy as Markdown" / "Copy as CSV", then the icon shows `CheckIcon` for 1200ms.

**Selection copy:** `onCopy` re-serializes the selection to markdown (`text/plain`) and HTML (`markdown-clipboard.ts`), honoring `data-markdown-copy` attributes on chips.

**Streaming render** (`chat/streamingMarkdown.ts:102-187`): while streaming and text ≥512 chars, the document splits into frozen prefix chunks plus one live tail. Boundaries fall only after a following top-level block starts, never inside fences, lists, quotes, indented blocks or HTML. Chunks are ≥512 chars, and splitting stops once a chunk contains HTML or a possible reference-style link. Each chunk is a memoized markdown render; only the tail re-parses. In GPUI: cache parsed blocks per frozen chunk and re-parse only the tail.

**GPUI mapping:** gpui-base 0.7 `TextView::markdown` + `MarkdownPlugin`/`MarkdownBlockRenderFn` hooks (`gpui-base-0.7.0/src/text/markdown_ext.rs:45`) for custom code-block chrome, tables, file chips. Highlighting via gpui-component `highlighter` (tree-sitter). Pierre colors must be mapped from TextMate scopes to tree-sitter capture names (section 10).

---

## 4. Composer

`chat/ChatComposer.tsx` (2381), `ComposerPromptEditor.tsx` (1697), `composer-logic.ts`, `composer-editor-mentions.ts`, `composerDraftStore.ts`, `chat/Composer*.tsx`.

### 4.1 Card geometry

`ChatComposer.tsx:1868-2381`.

```
form[data-chat-composer-form] mx-auto w-full max-w-768 min-w-0
└─ frame: rounded-[22px] p1 (+ "ultrathink-frame" when active); drag/drop target
   └─ surface[data-chat-composer-mobile-collapsed]: chat-composer-glass rounded-[20px] 1px border
      border: border (default) | ring/45 when a descendant has focus-visible | primary/70 + bg accent/45 while dragging files
      opacity .75 when environment unavailable; transition colors 200ms
      ├─ header panel (one of, see 4.6/4.7/4.8):  rounded-t-[19px] border-b border/65 bg-muted/20
      ├─ content box: relative px16 (<640 px12) pb8; pt16 without header, pt12 with header (<640: 14 / 10)
      │  ├─ command menu (absolute, above the content box)        (4.5)
      │  ├─ preview annotation cards  mb12                        (6.4)
      │  ├─ review comment chips      mb12                        (4.4)
      │  ├─ element context chips     mb12                        (4.4)
      │  ├─ image thumbnails          mb12 flex-wrap gap8         (4.3)
      │  └─ editor                                                (4.2)
      └─ footer (or approval actions)                             (4.11)
```

- **Glass**: bg `card` 20% (dark 45%), backdrop blur 16px saturate 125%, shadow `0 18px 48px -20px rgba(0,0,0,.28), 0 4px 14px -7px rgba(0,0,0,.22)` (dark .60/.40) (`index.css:222-249`). GPUI: solid `card` fallback plus the two shadows.
- **Ultrathink** (Claude effort "ultrathink" active, `chat/composerProviderState.tsx:58-84`, `index.css:1084-1151`): the frame gets a 2px animated rainbow border (`#ff6b6b → #f59e0b → #22c55e → #14b8a6 → #3b82f6 → #ec4899`, 120°, 220% size, 10s linear loop, saturate .82, brightness .92). The surface gets an inset 1px `rgba(255,255,255,.07)` ring, and the model-picker icon hue-rotates over 10s.
- **Height** (single line, no extras): 16 + 44 (editor min) + 8 + 28 (footer row) + 12 + 2 (borders) = 110px. Editor max 200px, then scrolls internally.

### 4.2 Editor

`ComposerPromptEditor.tsx`.

- **Tech.** Lexical 0.41 `PlainTextPlugin` (one paragraph, newlines are `LineBreakNode`s) + `HistoryPlugin` + `OnChangePlugin`. Three inline `DecoratorNode`s: `composer-mention`, `composer-skill`, `composer-terminal-context` (`:131-419`).
- **ContentEditable** (`:1611-1620`): `block max-h-200 min-h-44 (<640 52) w-full overflow-y-auto whitespace-pre-wrap break-words bg-transparent 14px (<640 16px) leading-relaxed foreground`, no outline.
- **Placeholder**: absolute inset0, same font, `muted-foreground/35`, pointer-events none. Hidden whenever terminal contexts exist. Text, in priority order (`ChatComposer.tsx:2230-2244`):
  1. Approval pending: `approval.detail` ?? "Resolve this approval request to continue".
  2. Answering user input: "Type your own answer, or leave this blank to use the selected option".
  3. Plan follow-up: "Add feedback to refine the plan, or leave this blank to implement it".
  4. Env unavailable: "{label}: {status}".
  5. Phase disconnected (no live session): "Ask for follow-up changes or attach images".
  6. Default: "Ask anything, @tag files/folders, $use skills, or / for commands".
- **Disabled** when connecting, when an approval is pending (the value is forced to ""), or when the env is unavailable and not answering user input.
- **Source of truth is a plain string** (`prompt`). The editor tree is rebuilt from the string via `splitPromptIntoComposerSegments` (`composer-editor-mentions.ts:198-223`), and `getTextContent()` serializes back:
  - Mention node → `[basename](encoded path)` (`shared:composerTrigger.ts:156-159`).
  - Skill node → `$name`.
  - Terminal node → `U+FFFC` placeholder (`lib/terminalContext.ts:45`).
- **Token recognition** (`shared:composerInlineTokens.ts:21-23`). A token becomes a chip only once followed by whitespace:
  - Markdown file link `[label](path)` where label = basename and path has no URI scheme.
  - `@path` or `@"quoted path"`.
  - `$skill` (`[a-zA-Z][a-zA-Z0-9:_-]*`).
- **Cursor model.** "Collapsed" offsets count each chip as 1 char; "expanded" offsets index the raw string (`composer-logic.ts:45-186`). The composer stores the collapsed cursor; trigger detection uses the expanded one.
- **Controlled sync** (`:1432-1475`): when `value`/`cursor`/terminal contexts/skills change externally, the editor state is rewritten and the selection restored. Change events during that write are ignored.
- **Plugins:**
  - `ComposerCommandKeyPlugin` (`:897-957`): intercepts ArrowDown/ArrowUp/Enter/Tab at high priority → `onCommandKeyDown`. Enter during IME composition (`isComposing` or keyCode 229) is swallowed.
  - `ComposerInlineTokenArrowPlugin` (`:959-1023`): ←/→ next to a chip moves across it as one unit.
  - `ComposerInlineTokenSelectionNormalizePlugin` (`:1025-1051`): a caret landing inside a chip snaps after it.
  - `ComposerInlineTokenBackspacePlugin` (`:1053-1118`): Backspace right after a chip removes the whole chip. Removing a terminal chip also drops that terminal context.
  - `ComposerSurroundSelectionPlugin` (`:1120-1384`): typing one of `( [ { ' " “ \` < « * _` with a non-empty selection that doesn't touch a chip wraps the selection with the matching closer and keeps the inner text selected. Handles the dead-key backtick on intl layouts.
- **Paste**: image files are added as attachments (4.3) and the default paste is prevented. Text pastes as plain text.

### 4.3 Attachments (images)

`ChatComposer.tsx:1588-1682,2130-2206`.

- **Sources**: paste (clipboard files of type `image/*`) and drag-drop onto the frame. A drag depth counter prevents flicker from child enter/leave events. Drop focuses the composer.
- **Limits**: `image/*` only ("Unsupported file type for 'X'. Please attach image files only."), each ≤10MB ("'X' exceeds the 10MB attachment limit."), ≤8 per message ("You can attach up to 8 images per message."). Values from `contracts:orchestration.ts:142-143`; upstream raises the count to 100 and adds an 80MB total. Errors appear in the thread error banner. Attaching while user input is pending → toast "Attach images after answering plan questions."
- **Thumbnail**: 64×64 `rounded-lg border/80 bg-background`, `object-cover`, cursor zoom-in → ExpandedImageDialog. Remove = icon-xs ghost `XIcon` at top4 right4, `bg-background/80` (hover /90). When the draft image could not be saved locally: amber `CircleAlertIcon` 12 at top4 left4 in a `bg-background/85` 2px-padded box, tooltip "Draft attachment could not be saved locally and may be lost on navigation."
- **Persistence**: each image is also stored as a data URL in the draft (`syncPersistedAttachments`). Failures mark the image id non-persisted.
- **Send**: data URLs are uploaded in `message.attachments` (`{type:"image", name, mimeType, sizeBytes, dataUrl}`). An image-only message sends the text "[User attached one or more images without additional text. Respond using the conversation context and the attached image(s).]" (`ChatView.tsx:256-257`).

### 4.4 Inline chips and context chips

Classes in `components/composerInlineChip.ts`.

- **Base chip**: `inline-flex max-w-full gap4 rounded-md(8) border border/70 bg-accent/40 px6 py1 font-medium 12px leading-1.1 foreground align-middle select-none`; icon 14 at 85% opacity; label truncated, leading-tight.
- **File mention chip** (`ComposerPromptEditor.tsx:131-152`): Pierre icon + basename; tooltip = full path (max-w 480, wraps anywhere).
- **Skill chip** (`:240-269`): `border-fuchsia-500/25 bg-fuchsia-500/12 text-fuchsia-700` (dark `fuchsia-300`). Icon = cube outline SVG (`composerInlineChip.ts:17`, stroke 1.85). Label = `displayName` or title-cased name (`providerSkillPresentation.ts:16-24`). Tooltip = short description, else description.
- **Terminal context chip** (`chat/TerminalContextInlineChip.tsx`, `chat/ComposerPendingTerminalContexts.tsx`): `TerminalIcon` 14 + "{terminalLabel} line N" / "lines A-B". Tooltip = selected text. **Expired** variant (text missing, e.g. after reload, since text isn't persisted): `border-destructive/35 bg-destructive/8 text-destructive`, tooltip "Terminal context expired. Remove and re-add {label} to include it in your message." Expired contexts are dropped on send with a warning toast (`ChatView.logic.ts:245`).
- **Element context chips** (`chat/ComposerPendingElementContexts.tsx`, above the editor): base chip + `pr4`, `MousePointerClick` 14, label, optional source label 10px `muted-foreground/85`, dismiss button (`ml2 size-14 radius 4 muted-foreground/72`, hover `bg-foreground/6 foreground`, `X` 12). Multi-line tooltip: label / source / url / selector / ≤600 chars of HTML.
- **Review comment chips** (`chat/ComposerPendingReviewComments.tsx`): base chip + `MessageCircle` 14 + "{filePath} {rangeLabel}" + dismiss. Tooltip = comment text.
- **Send-time materialization** (`ChatView.tsx:4062-4073`): placeholders → `@terminal-label:range`, then trailing `<terminal_context>` block (`lib/terminalContext.ts:159-221`), `<element_context>` block, `<preview_annotation>` blocks, review comment blocks. The timeline parses these back (3.4).

### 4.5 Triggers and command menu

`composer-logic.ts:218-256`, `ChatComposer.tsx:779-916,1384-1488`, `chat/ComposerCommandMenu.tsx`.

**Trigger detection** at the expanded cursor:

- Line prefix `/\S*` at the start of a line → `slash-command`.
- Token (since the last whitespace or U+FFFC) starting with `$` → `skill`.
- Token starting with `@` → `path`.
- The menu is open whenever a trigger exists and no approval is pending. Typing right next to a chip suppresses the trigger.

**Items:**

- **path**: RPC `projects.searchEntries {cwd: gitCwd, query, limit: 80}`, debounced 120ms (`state/queries.ts:26-27,184-214`). Only for non-empty queries. Item: Pierre entry icon (file/dir), label = basename, description = parent dir. Select → replace the token with `[basename](encoded path) ` (trailing space; an existing following space is consumed).
- **slash-command**: built-in `/model` ("Switch response model for this thread"), which clears the token and opens the model picker; plus the provider's `slashCommands` (label `/name`, description = description ?? `input.hint` ?? "Run provider command"), inserted as `/name `. Search: ranked match on name (exact 0, prefix 2, boundary 4 on `- _ /`, includes 6, fuzzy 100) and description (20+) (`chat/composerSlashCommandSearch.ts`). With an empty query, items group into "Built-in" and "Provider".
- **skill**: provider `skills` via `searchProviderSkills`. Label = display name, description = short description ?? description ?? "{scope} skill" ?? "Run provider skill". Right side shows the install source ("App" / "System" / "Project" …, 12px muted/70). Inserted as `$name `. Group label "Skills".
- **Empty states**: "Searching workspace files..." / "Searching workspace skills..." while loading; else "No matching files or folders." / "No skills found. Try / to browse provider commands." / "No matching command."

**Geometry:**

- Absolute `inset-x-0 bottom-full mb8` relative to the content box, so it spans the surface's inner width and floats 8px above the editor content box, z-20.
- Box: `rounded-[20px] border border/80 bg-popover/96 shadow-lg/8 backdrop-blur-xs`. List max-h 288 p8.
- Group label: `px12 pt8 pb4 10px semibold uppercase tracking .08em muted-foreground/55`. Separator `my2 mx8` 1px `border`.
- Item: `min-h-28 px8 py6 radius 6 14px gap8 select-none`; icon 16 (`BotIcon` for `/model`, cube glyph 14 in a 16 box for provider commands and skills); label (no shrink) + description (`flex-1 truncate 12px muted-foreground/70`). Active item `bg-accent text accent-foreground`. Mouse hover sets active; mousedown is prevented so the editor keeps focus.
- Empty box: `px20 py14`, 12px `muted-foreground/70`.
- Active item is scrolled into view (`nearest`).

**Keyboard:**

- ↑/↓ cycle with wrap-around.
- Enter or Tab selects the active item (default = first; the highlight resets when the query changes, `chat/composerMenuHighlight.ts`).
- There is **no Escape handler**. The menu closes only when the trigger disappears.

### 4.6 Pending approval

- **Header panel** (`chat/ComposerPendingApprovalPanel.tsx`): `px20 py16` (<640 px16 py14). `flex-wrap gap8`: "PENDING APPROVAL" 14px uppercase tracking .2em, then a summary 14px `font-medium` ("Command approval requested" / "File-read approval requested" / "File-change approval requested"), then "1/N" 12px muted when N > 1.
- **Footer replaced** by `flex justify-end gap8 px12 pb12` (`ChatComposer.tsx:2278-2285`, `chat/ComposerPendingApprovalActions.tsx`) with four buttons, all size sm:
  - ghost "Cancel turn" → `cancel`
  - destructive-outline "Decline" → `decline`
  - outline "Always allow this session" → `acceptForSession`
  - default "Approve once" → `accept`
  - All disabled while responding.
- **Dispatch**: `thread.approval.respond {threadId, requestId, decision}`.
- **Derivation** (`session-logic.ts:442-496`): `approval.requested` (needs `requestId` and `requestKind`, or a `requestType` mapping) opens a request; `approval.resolved` closes it, as does `provider.approval.respond.failed` with a stale/unknown detail. The oldest request is shown first.

### 4.7 Pending user input (questions)

`chat/ComposerPendingUserInputPanel.tsx`, `pendingUserInput.ts`, `ChatView.tsx:4355-4487`.

- **Panel** `px20 py12`:
  - Header row `mb8 gap12`: question header 11px semibold `tracking-widest uppercase muted-foreground/55`; when multiple questions, an "i/N" pill (`h20 radius 8 bg-muted/60 px6 10px font-medium tabular muted-foreground/60`).
  - Question 14px `foreground/90`.
  - Multi-select hint "Select one or more options." 12px `muted-foreground/65`, mt4.
  - Options `mt12 space-y-6`. Each is a button `w-full gap12 rounded-lg border px12 py8 text-left`, transition 150ms:
    - Unselected: transparent border, `bg-muted/22`, `foreground/85`, hover `border/45 bg-muted/34`.
    - Selected: `border-primary/30 bg-primary/8 foreground`.
    - Content: label 14px `font-medium`; description 12px `muted-foreground/50` (when it differs from the label).
    - Right side: selected → `CheckIcon` 14 `primary`; else a kbd digit (`size-20 radius 4 border/50 bg-background/35 11px muted-foreground/70`).
    - Responding → opacity 50%, not-allowed cursor.
- **Behavior:**
  - Single-select: click selects optimistically and auto-advances after 200ms. Multi-select toggles.
  - Digit keys 1–9 pick options when focus is not in an input, textarea or contenteditable.
  - The editor becomes the "custom answer" field for the active question. Typing there overrides the option selection.
  - Primary actions (4.11): Previous / "Next question" / "Submit answer(s)" / "Submitting...".
  - Enter in the editor = advance.
- **Dispatch**: on the last question, `thread.user-input.respond {threadId, requestId, answers}`.
- **Derivation** (`session-logic.ts:548-595`): `user-input.requested` with `payload.questions[{id, header, question, options[{label, description}], multiSelect}]`; closed by `user-input.resolved` or a stale failure.

### 4.8 Plan follow-up banner (unreachable in this fork)

`chat/ComposerPlanFollowUpBanner.tsx`, `ChatView.tsx:1963-1967,4489-4825`.

- The banner requires `interactionMode === "plan"`. The fork pins `interactionMode = "default"` (`ChatView.tsx:309-315,1372`, commit `eeca4b9f` removed the plan/build toggle and the runtime-mode picker). So the "Plan Ready" banner, the Refine/Implement buttons and "Implement in a new thread" never render.
- For completeness:
  - Banner: info badge sm "PLAN READY" (`radius 8 px6 semibold tracking-wide uppercase`) + plan title 14px `font-medium` truncate, padding `px20 py16`.
  - Empty draft → "Implement" (sends "PLEASE IMPLEMENT THIS PLAN:\n{plan}" in `default` mode with `sourceProposedPlan`).
  - Text → "Refine" (sends text in `plan` mode).
  - Split chevron → "Implement in a new thread" (`thread.create` + `turn.start` on a new thread, then navigate).
- Recommend skipping this in v1 (section 10).

### 4.9 Send pipeline

`ChatView.tsx:3964-4310`.

1. Guards: no thread, send busy, connecting, env unavailable, send in flight → return. If answering questions → advance instead.
2. Read the send context from the composer handle (prompt, images, contexts, selected instance/model/options/effort).
3. `deriveComposerSendState` (`ChatView.logic.ts:212-243`): sendable if trimmed text (placeholders removed) is non-empty, or there are images, terminal contexts with text, element contexts, preview annotations or review comments. Not sendable with only expired terminal contexts → warning toast and return.
4. New worktree mode on the first message without a base branch → error "Select a base branch before sending in New worktree mode."
5. `beginLocalDispatch` (optimistic working state + timer, keyed by thread so it survives the draft→server route swap, `:380-495`).
6. Compose text: terminal labels + blocks, element blocks, preview annotations, review comments. Then `formatOutgoingPrompt` prefixes "Ultrathink:" when the Claude effort is the prompt-injected value (`:337-347`, `shared:model.ts:352`).
7. Force the scroll state to following-end. Push an optimistic user message (with blob previews). Clear the composer draft content.
8. Title seed: trimmed text | "Image: {name}" | terminal label | element label | "New thread", truncated to 50 chars + "..." (`shared:String.ts:1-8`).
9. Server thread, first message → `thread.meta.update {title}`. Then `persistThreadSettingsForNextTurn`: `thread.meta.update {modelSelection}` if changed; `thread.runtime-mode.set` / `thread.interaction-mode.set` if they differ from `full-access`/`default` (`:3210-3284`).
10. Dispatch `thread.turn.start` (`contracts:orchestration.ts:633-649`):
    ```
    { threadId, message: {messageId, role:"user", text, attachments:[upload…]},
      modelSelection: {instanceId, model, options?}, titleSeed, runtimeMode:"full-access",
      interactionMode:"default", createdAt,
      bootstrap?: { createThread?: {projectId,title,modelSelection,runtimeMode,interactionMode,branch,worktreePath,createdAt},
                    prepareWorktree?: {projectCwd, baseBranch, branch:"<temp name>", startFromOrigin?},
                    runSetupScript?: true } }
    ```
    `createThread` is included for drafts. `prepareWorktree` + `runSetupScript` are included on the first message in New worktree mode.
11. Failure: if the composer is still empty, restore the prompt, images (cloned), contexts, annotations and review comments, remove the optimistic message, and set the thread error ("Failed to send message.").

**No client-side queue.** While a turn runs, the Send button is replaced by Stop, but **Enter in the editor still dispatches `thread.turn.start`**. `onSend` does not check `phase === "running"`; the server treats it as a steer. Keep this behavior.

### 4.10 Banner stack (above the composer)

`chat/ComposerBannerStack.tsx`, items built in `ChatView.tsx:1814-1880`.

- Container `mx-auto mb8 max-w-768`. Front banner = Alert of the item variant: icon, title, description, actions.
- Extra items stack **behind** the front banner:
  - A "cap" strip (absolute `-top-12`, 96% width, h12, `rounded-t-xl` border without bottom, `border-warning/24 bg-background/96`, shadow `0 6px 18px rgba(0,0,0,.06)`) hints at more banners.
  - On hover or focus-within of the stack, the hidden items appear above (absolute `bottom: calc(100% + 8px)`, `space-y-8`, translateY 4→0, opacity 0→1, 150ms) and the cap fades out.
- Dismiss: icon-xs ghost `XIcon` 14. Exit animation 220ms ease-in (front: translateY +64px & fade; stacked: +112px), then `onDismiss`.
- **Items:**
  - Environment unavailable: variant error (phase error) else warning; `WifiOffIcon`; title "{label}: {status}"; description = connection error ?? "Reconnect this environment before sending messages or running actions."; actions: xs "Reconnect" ("Reconnecting..." disabled while connecting) → `environmentCatalog.retryNow`, xs outline "Connections" → settings.
  - Version mismatch: warning, `TriangleAlertIcon`, "Client and server versions differ", "Client {v} is connected to {server label} {v}. Sync them if RPC calls or reconnects fail.", dismissible (persisted).

### 4.11 Footer toolbar and primary actions

`ChatComposer.tsx:2287-2376`.

- Footer `flex flex-nowrap items-center justify-between px12 pb12 (<640 px10 pb10) overflow-visible`. `pt8` when user input is pending. Gap: 0 normally, 6 when compact.
- **Left** (`-m4 p4 flex min-w-0 flex-1 gap4 overflow-x-auto`, scrollbar hidden):
  - `ProviderModelPicker` (5.1).
  - Wide: [vertical separator mx2 h16] `TraitsPicker` (5.3), then the plan toggle (separator + ghost sm button with `ListTodoIcon` + label "Plan"/"Tasks", px12 (<640 px8), active `bg-accent foreground`, inactive `muted-foreground/70` hover `foreground/80`, tooltip "Show/Hide {label} sidebar"). The plan toggle shows when there is an active plan (`turn.plan.updated`), a sidebar proposed plan, or the plan panel is open. Label "Plan" if a proposed plan exists, else "Tasks".
  - Compact: an ellipsis menu (`CompactComposerControlsMenu.tsx`: ghost sm px8 `EllipsisIcon` 16) holding the traits menu content, a separator, and the plan item.
- **Compact rules** (`composerFooterLayout.ts`): footer compact when form width <620px (<780 with wide actions = plan follow-up or pending questions). Primary actions compact only with wide actions and width <780.
- **Right** `flex shrink-0 gap8`:
  - `ContextWindowMeter` (`chat/ContextWindowMeter.tsx`), when a `context-window.updated` activity exists:
    - Trigger 24² round, hover `bg-accent`. 16px ring: track `muted-foreground` 35%, progress stroke 3 (viewBox 24, r 9.75) in `primary`, or `red-500` when >90%, rotated −90°, dashoffset transition 500ms.
    - Hover popover (150ms delay), side top, align end, w256, tooltip style, p12 gap8: "Context Window" 12px `font-medium` muted; "{pct} · {used}/{max}" 11px tabular `muted-foreground/70`; 6px progress bar; "Total processed {n}"; "{Provider} automatically compacts its context when needed." Tokens format as 950 / 1.2k / 12k / 1.2m.
  - "Preparing worktree..." 12px `muted-foreground/70` while preparing.
  - `ComposerPrimaryActions` (`chat/ComposerPrimaryActions.tsx`), in priority order:
    1. **Pending questions**: [Previous: outline sm rounded-full, or icon-sm `ChevronLeftIcon` 14 when compact] + submit sm rounded-full px16 (compact px12). Label: "Submitting..." | "Next"/"Submit" when compact | "Next question" | "Submit answers" (index > 0) | "Submit answer". Disabled when responding or when the step is incomplete.
    2. **Running → Stop**: 28² circle (<640 32²) `bg-primary/90 text-card shadow-xs`, inset top highlight, hover `bg-primary` + scale 1.05, 150ms. 11px rounded square glyph (rx 1.25). aria "Stop generation" → interrupt.
    3. **Plan follow-up** (unreachable, 4.8).
    4. **Send**: 28² circle (<640 32²) `bg-primary/90 text-primary-foreground shadow-xs`, hover `bg-primary` scale 1.05. Disabled → opacity 30%, no shadow. Up-arrow glyph 13px (path `M7 11.5V2.5M7 2.5L3 6.5M7 2.5L11 6.5`, stroke 1.8, round caps). While sending or connecting → `Spinner` 14. aria = "Environment disconnected" | "Connecting" | "Preparing worktree" | "Sending" | "Send message". Disabled when busy, connecting, env unavailable, or nothing sendable.

### 4.12 Mobile collapsed composer (skip for desktop)

Below 640px and unfocused, the composer collapses to one row (prompt preview + 32² send) with its own approval/question variants (`ChatComposer.tsx:1941-2058`). Desktop windows never hit this.

### 4.13 Draft persistence

`composerDraftStore.ts`.

- zustand `persist` → localStorage key `t3code:composer-drafts:v1`, version 8, writes debounced 300ms, flushed on `beforeunload` (`:59-79,3342-3344`).
- **Per target** (draft id or `env:thread` key): `prompt`, `attachments[]` (`{id, name, mimeType, sizeBytes, dataUrl}`), `terminalContexts[]` (metadata only: id, threadId, createdAt, terminalId, terminalLabel, lineStart, lineEnd; the text is not persisted, so contexts restore as expired), `elementContexts[]` (full payload), `previewAnnotations[]`, `reviewComments[]`, `modelSelectionByProvider{instanceId → {instanceId, model, options}}`, `activeProvider`, `runtimeMode`, `interactionMode` (`:128-150`).
- **Store level**: `draftThreadsByThreadKey` (draft sessions, `:207-227`), `logicalProjectDraftThreadKeyByLogicalProjectKey`, sticky model selection per instance + sticky active provider. These seed new drafts and threads; picking a model also sets sticky (`ChatView.tsx:4917-4921`).
- **Model resolution order** for the composer (`ChatComposer.tsx:556-619`): draft `activeProvider` → session instance → thread `modelSelection.instanceId` → project default → first enabled entry of the selected driver kind → any enabled entry → `codex`. Entries that violate the provider lock are skipped.
- The Rust port can choose its own storage. It must preserve the semantics: per-thread draft, sticky model, terminal contexts losing their text on restart, and send-failure restore.

---

## 5. Pickers

### 5.1 ProviderModelPicker (trigger)

`chat/ProviderModelPicker.tsx:22-213`.

- **Trigger**: Button ghost xs (h24, px8), `justify-between whitespace-nowrap 12px muted-foreground/70`, hover `foreground/80`. Width `max-w-224` (<640 192); compact: `max-w-168 shrink-0`.
  - Content `flex gap8`: `ProviderInstanceIcon` (16 icon; 20 box with a badge when the instance has an accent color or the driver has duplicates) + model name truncate (tooltip = same name, side top) + `ChevronDownIcon` 12 at 60% opacity with `-me-4`.
  - Name = `shortName ?? name` with a leading "{subProvider}" qualifier stripped (`chat/providerIconUtils.ts:385-413`). If the model slug is not in the active instance's list, the first option is shown.
- **Open**: Popover with transparent chrome, align start, sideOffset 4. While open it locks page wheel/touch scrolling outside the picker content.
- **Controlled**: `/model`, ⇧⌘M (`modelPicker.toggle`) and the trigger all toggle it.
- **On select**: `onProviderModelSelect(instanceId, model)` (`ChatView.tsx:4852-4937`) → `composerDraft.modelSelectionByProvider[instance] = {instanceId, model}`, `activeProvider = instance`, sticky selection updated, composer refocused. Nothing is dispatched until the next send (then `thread.meta.update {modelSelection}` + `turn.start.modelSelection`).

**ProviderInstanceIcon** (`chat/ProviderInstanceIcon.tsx`): driver glyph (OpenAI for codex, ClaudeAI, OpenCode, Cursor, Grok from `components/Icons.tsx`), or 2-letter initials (10px semibold) for unknown drivers. Optional badge at the bottom-right: `h14 min-w14 rounded-full border px2 8px semibold`; bg = accent color (white text) or `muted`; border color = surrounding background; initials of the display name.

### 5.2 ModelPickerContent + sidebar

`chat/ModelPickerContent.tsx:60-677`, `chat/ModelPickerSidebar.tsx`, `chat/ModelListRow.tsx`, `chat/modelPickerSearch.ts`.

- **Panel**: `flex-row h = min(100vh, 384) w = min(100vw, 400) overflow-hidden rounded-lg border bg-popover shadow-lg/5`, with a 1px inner top highlight (black 4% light / white 6% dark).
- **Sidebar** (hidden while searching): w48, `border-r bg-muted/30`, vertical scroll with no scrollbar, inner `flex-col gap4 px4 pb4 pt2`.
  - Favorites button first (`StarIcon` 20 filled, in a `mb4 border-b pb4` section).
  - One button per visible provider instance: square (aspect 1, ≈40×40), `rounded-md hover:bg-muted`, icon 20 in a 24 box. Unavailable (not ready/disabled) or locked-out → opacity 50% + not-allowed cursor.
  - "New" sparkle badge: `absolute -right-2 top2 size-14`, `SparklesIcon` 8 `amber-600` (dark `amber-300`).
  - Selected indicator: 3×20px `rounded-l-full bg-primary` bar at the right edge, centered on the selected button, animating `top` over 200ms ease-out.
  - Tooltips open to the **left** (offset 8, max-w 256): display name, "{name} — New", "{name} — Disabled in settings.", "{name} — Unavailable/Limited/Not ready. {msg}", or for locked: "{name} is unavailable in this thread. Start a new thread to switch providers."
- **Initial sidebar selection**: favorites if any exist (unlocked), else the active instance.
- **Main**: `flex-1 flex-col bg-muted/40 border-l`.
  - Search: `px16 pt10`; underline box `border-b border/70 pb10`, focus-within `border-ring`. Input h26 14px sans, transparent, placeholder "Search models...". `SearchIcon` 16 `muted-foreground/55` as the start addon. Autofocus on open (3 attempts).
  - List: virtualized LegendList, py6, estimated row 60, draw distance 480, 24px top/bottom fade masks when scrollable, scrollbar 4px (`index.css:1153-1173`). Empty: "No models found" 12px py24.
- **Row** (`ModelListRow.tsx:45-128`): combobox item `w-full rounded-md px8 py10`, highlighted `bg-muted/56`, `flex gap12`.
  - Left: name 12px `font-medium leading-snug` truncate (`shortName` unless locked) + `CheckIcon` 14 `blue-400` when selected + "New" badge (`radius 4 border amber-500/35 bg-amber-500/15 px2 py1 10px bold uppercase tracking-wide amber-800`, dark `amber-200`). Second line mt4: provider glyph 12 + "{instance display name}[ · subProvider]" 12px `muted-foreground/70`.
  - Right `gap6`: jump `Kbd` (h16 px6 10px radius 6, e.g. "⌘1") + favorite toggle (icon-xs ghost, `StarIcon` 12, opacity 64% until row hover; favorite → filled `yellow-500`, full opacity, tooltip "Add to/Remove from favorites").
  - Disabled rows (model change blocked) get a tooltip on the left with the reason.
- **Filtering**:
  - No query: models of the selected instance (or favorites across instances in instance order), favorites grouped first (`modelOrdering.ts`).
  - Query: tokenized match across name, shortName, subProvider, driver kind, display name and the combined text. Per token, take the best field score (field base = index×10; exact +0, prefix +2, boundary +4, includes +6, fuzzy +100 for tokens ≥3 chars). Sum across tokens. Favorites get −24. Ties: favorite, then text. Sidebar selection is ignored; only the lock filters.
  - Only instances with "picker ready" status contribute models.
- **Keyboard**: ↑/↓ highlight (scrolls into view), Enter selects the highlighted row, Esc closes, ⌘1–⌘9 (`modelPicker.jump.N`, active only while the picker is open) select the Nth selectable row.
- **Favorites** persist in client settings (`favorites[{provider: instanceId, model}]`).

### 5.3 TraitsPicker (reasoning effort, fast mode, thinking, context window, agent)

`chat/TraitsPicker.tsx`, `chat/composerProviderState.tsx`.

- **Descriptors** come from model capabilities (`shared:model` `getProviderOptionDescriptors`): `select` (primary = first select, typically `effort`; also `contextWindow`, `agent`) and `boolean` (`fastMode`, `thinking`). Hidden when there are no descriptors.
- **Trigger**: ghost xs, `muted-foreground/70` hover `foreground/80`.
  - Codex: `max-w-192` (<640 160), justify-start, truncate, px8.
  - Others: shrink-0, px8.
  - Label = parts joined " · ": select current option label (effort ids `low`→"Light", `max`→"Max", `ultra`→"Ultra"; "Ultra" renders `purple-400`); booleans: `fastMode` → "Fast"/"Normal", others "{Label} On/Off"; Claude prompt-controlled → "Ultrathink". Plus `ChevronDownIcon` 12 at 60% opacity.
- **Menu**: align start. With the xs trigger the popup is w160 radius 8 with compact items. Per select descriptor: section label (`px8 pt4 pb2 11px font-medium muted-foreground`) + radio items (compact: `min-h-24 grid [12px 1fr] gap6 py2 ps6 pe8 12px`, 12px check). Option text + " (default)" for the default option. Booleans: label + On/Off radios. Separators between sections (`mx6 my2`).
- **Claude ultrathink**: choosing a prompt-injected value doesn't set an option. It prefixes the prompt with "Ultrathink:\n" (or rewrites the prefix). Choosing another effort strips the prefix. If "ultrathink" appears in the body text, the effort radios are disabled with the note 'Your prompt contains "ultrathink" in the text. Remove it to change this option.'
- **Writes**: `setProviderModelOptions(target, driver, options, {instanceId, model, persistSticky:true})` → draft `modelSelectionByProvider[instance].options` → sent as `turn.start.modelSelection.options` (`ProviderOptionSelection[]`).

### 5.4 Interaction mode (plan vs default) and runtime mode / access

**Removed in this fork** (commit `eeca4b9f`). There is no toggle and no access picker. Every send uses `runtimeMode: "full-access"`, `interactionMode: "default"` (`ChatView.tsx:309-315`). If the server thread differs, it is updated via `thread.runtime-mode.set` / `thread.interaction-mode.set` before the turn (`:3245-3274`). Contract enums for reference: runtime `approval-required | auto-accept-edits | full-access` (upstream adds `auto`); interaction `default | plan`. Do not build these controls unless asked.

### 5.5 Built-in `/model`

See 4.5. Selecting it opens the model picker.

### 5.6 OpenInPicker

`chat/OpenInPicker.tsx:154-275`.

- **Shape**: joined `Group` (`ui/group.tsx`):
  - Outline xs button: preferred editor icon 14 + "Open" label, visible only when the header container is ≥768px, else screen-reader only. Disabled with no preferred editor or no cwd.
  - Group separator (hidden below a 768px container).
  - Outline icon-xs `ChevronDownIcon` 16 → Menu align end.
- **Items**: editors filtered by the server's `availableEditors`, in this order: Cursor, Trae, Kiro, VS Code, VS Code Insiders, VSCodium, Zed, Antigravity, IntelliJ IDEA, Aqua, CLion, DataGrip, DataSpell, GoLand, PhpStorm, PyCharm, Rider, RubyMine, RustRover, WebStorm, and Finder/Explorer/Files (`file-manager`). Each item: icon (`muted-foreground`) + label + `MenuShortcut` (⌘O) on the preferred one. None available → disabled "No installed editors found".
- **Action**: `shell.openInEditor {cwd: gitCwd, editor}` and remember the preferred editor. ⌘O (`editor.openFavorite`) opens the preferred editor.
- **Visibility**: thread env = primary env and a project exists.

---

## 6. Other components

### 6.1 ProposedPlanCard

`chat/ProposedPlanCard.tsx:36-256`.

- Card `rounded-[24px] border border/80 bg-card/70 p20 (<640 p16)`.
- Header `flex-wrap justify-between gap12`:
  - Left `gap8`: Badge secondary "Plan" + title 14px `font-medium foreground` truncate. Title = first markdown heading, else "Proposed plan" (`proposedPlan.ts:1-4`).
  - Right: icon-xs outline `EllipsisIcon` 16 → menu: "Copy to clipboard" ("Copied!"), "Download as markdown", "Save to workspace" (disabled with no workspace).
- Body `mt16`: ChatMarkdown of the plan with the leading heading and a following "Summary" heading stripped (`proposedPlan.ts:6-20`).
  - Collapsible when >900 chars or >20 lines. Collapsed: `max-h-416 overflow-hidden`, renders a 10-line preview, and a bottom gradient (h96, `card/95 → card/80 → transparent`) overlays the end.
  - Toggle `mt16` centered: outline sm "Expand plan"/"Collapse plan".
- **Save dialog**: max-w 576, title "Save plan to workspace", description "Enter a path relative to `{root}`.", input "Workspace path" (default = generated filename), Cancel / Save ("Saving..."). Writes via `projects.writeFile {cwd, relativePath, contents}`. Toasts: "Plan saved to workspace" / "Could not save plan" / "Enter a workspace path".

### 6.2 ThreadStatusIndicators

`components/ThreadStatusIndicators.tsx`. Used by the sidebar, the command palette and the branch selector PR pill. ChatView itself shows no status pill.

- **PR status** (`prStatusIndicator`): open → `emerald-600` (dark `emerald-300/90`) "{PR} open"; closed → `zinc-500` (dark `zinc-400/80`); merged → `violet-600` (dark `violet-300/90`); `GitPullRequestIcon`. Tooltip "#{n} {PR} {state}: {title}". A PR counts only when `gitStatus.refName === thread.branch`.
- **Terminal running**: `TerminalIcon` 12 `teal-600` (dark `teal-300/90`) pulsing, "Terminal process running".
- **Worktree**: `FolderGit2Icon` 12 `muted-foreground/40`, tooltip "Worktree: {path} ({branch})".
- **Status label**: dot + text 10px, text hidden below `md`; compact = 9px dot in a 14 box.
- **Pills** (`components/Sidebar.logic.ts:29-57,375-445`), with priority:

  | Pill | Color (light / dark) | Pulse | Priority |
  |---|---|---|---|
  | Working, Connecting | `sky-600` / `sky-300/80`, dot `sky-500` / `sky-300/80` | yes | 3 |
  | Pending Approval | `amber-600` / `amber-300/90` | no | 5 |
  | Awaiting Input | `indigo` | no | 4 |
  | Error | `rose` | no | 5 |
  | Plan Ready | `violet` | no | 2 |
  | Completed | `emerald` | no | 1 |

  "Completed" clears when ChatView acknowledges completion (2.4).

### 6.3 BranchToolbar (under the composer)

`components/BranchToolbar.tsx:193-293`, `BranchToolbarEnvModeSelector.tsx`, `BranchToolbarEnvironmentSelector.tsx`, `BranchToolbar.logic.ts`.

- Rendered only when the project is a git repo (`vcs.status.isRepo`, default true while loading) and a thread + project exist.
- **Row**: `mx-auto flex w-full max-w-768 items-center gap8 px12 (<640 px10) pb12 pt4`.
- **Left** (≥768px: `flex shrink-0 gap4`):
  - Environment selector (only when the logical project spans >1 environment and the thread is a draft). Select trigger ghost xs `font-medium`, icon `MonitorIcon` (primary env) / `CloudIcon` 12 + env label. Popup group "Run on". Locked → static span (12px `font-medium muted-foreground/70`, px11). Separator `mx2 h14`.
  - Workspace (env mode) selector: Select ghost xs. Icon: `FolderGit2Icon` (New worktree) / `FolderGitIcon` (current worktree) / `FolderIcon` 12. Values: "Current checkout" / "Current worktree" (local) and "New worktree". Locked → static "Local checkout" / "Worktree". Locked when `envLocked`, or when a server thread has a worktree.
  - Below 768px both collapse into one menu (`MobileRunContextSelector`, `:69-191`).
- **Right**: `BranchToolbarBranchSelector` (`flex-1 justify-end`, ≥768px `ml-auto flex-none`).

**BranchToolbarBranchSelector** (`BranchToolbarBranchSelector.tsx`):

- **PR pill** (when the active branch has a PR): `inline-flex gap2 rounded(4) px4 py2 11px font-medium tabular` in the PR color, hover `bg-muted/60`, `GitPullRequestIcon` 12 + "#123". Tooltip "Open {PR} #{n} ({state}) in browser". Click opens the URL.
- **Trigger**: ghost xs `muted-foreground/70` hover `foreground/80`: `GitBranchIcon` 12 at 70% opacity + label (max-w 240 truncate) + `ChevronDownIcon` 12 at 50% opacity. Label = "Select ref" | "From {branch}" (New worktree, no worktree yet) | branch. Disabled while refs first load or an action is pending. The optimistic branch shows immediately during a switch.
- **Popup**: Combobox align end, side top, w320, flex-col.
  - Search: `px12 pt10`; underline `border-b border/70 pb6` focus `border-ring`; `SearchIcon` 16 at left 0 top 6; input h26 ps20 14px, placeholder "Search refs...".
  - List: virtualized, max-h 224, est. row 28, `ps4 pt8 pb4`, 24px fade masks. Infinite pagination: loads the next page when within 96px of the bottom (`usePaginatedBranches` → VCS refs RPC with cursor).
  - Status line: "Loading refs..." / "Loading more refs..." / "Showing N of M refs".
  - Empty: "No refs found."
  - Items:
    - PR checkout (query parses as a PR reference and the thread is a draft; shown first): source-control icon 14 + "Checkout {PR}" (`font-medium`) / reference 12px muted. Click → `PullRequestThreadDialog`.
    - Branches: name truncate + badge 10px `muted-foreground/45`: current | worktree | remote | default.
    - Create: 'Create new ref "{query}"' (not when picking a worktree base, nor on an exact match).
  - "Start from origin" switch row (only when picking a New worktree base): `border-t border/60 px12 py8 12px`, `RefreshCwIcon` 12 + "Start from origin" (muted `font-medium`) + Switch (thumb 14). Tooltip "Creates the worktree from the latest matching branch on origin instead of your local branch."
- **Actions** (`:328-423`):
  - Selecting a ref while picking a worktree base → only sets the thread/draft branch.
  - A ref with an existing worktree → reuses that worktree path.
  - Otherwise `vcs.switchRef {cwd, refName}` (remote refs become a local branch name), then set the thread branch.
  - Create → `vcs.createRef {cwd, refName, switchRef:true}`.
  - Server threads: `thread.meta.update {branch, worktreePath}`, plus `thread.session.stop` when the worktree path changes.
  - Drafts: `setDraftThreadContext`.
  - Error toasts "Failed to switch ref." / "Failed to create and switch ref."
  - In New worktree mode with no branch, the current git branch is auto-selected.

### 6.4 ComposerPreviewAnnotationCards

`chat/ComposerPreviewAnnotationCards.tsx`, data `PreviewAnnotationPayload` (`contracts`).

- Row `flex-wrap gap6 mb12`.
- Card `group relative flex max-w-full items-center overflow-hidden rounded-lg border/80 bg-background/72`:
  - Thumbnail (the image whose id = annotation id): `size-56 border-r border/70 bg-muted object-cover`, scale 1.03 on card hover (200ms), click → expand. No image → 40² cell with `MousePointerClick` 14 `blue-500`.
  - Body `px10 py8 pr32`:
    - Comment 12px `font-medium foreground/90` truncate, max-w 320.
    - Meta row (mt4 when there's a comment): up to 2 element labels (mono 10px `foreground/65` max-w 160 truncate) + "+N" + stats (`inline-flex gap4 10px font-medium muted-foreground`, icon 12 + count; tooltips "N element(s)/region(s)/drawing(s)/style change(s)") using `MousePointerClick`, `Frame`, `PenLine`, `Paintbrush`.
  - Remove: absolute top6 right6 20² radius 4 `muted-foreground/60`, hover `bg-muted foreground`, `X` 12.
- Hidden while an approval or user input is pending. The same annotation images are excluded from the plain thumbnail row.
- The **timeline** version (`MessagesTimeline.tsx:1818-1868`) is read-only: 56px thumb, comment, target summary 10px, paintbrush count.

---

## 7. Keyboard shortcuts (chat-relevant)

Server-provided keybindings (`primaryServerKeybindingsAtom`; defaults `shared:keybindings.ts:21-55`) with `when` contexts `terminalFocus`, `terminalOpen`, `previewFocus`, `previewOpen`, `modelPickerOpen`. `mod` = ⌘ on macOS, Ctrl elsewhere. The Rust app must load the server keybindings and evaluate `when` expressions.

| Keys (default) | Command | Handler / effect |
|---|---|---|
| mod+J | `terminal.toggle` | ChatView (`ChatView.tsx:3792-3797`); opens a terminal if none |
| mod+alt+B | `rightPanel.toggle` | ChatView |
| mod+D (`!terminalFocus`) | `diff.toggle` | right panel "diff" (server threads only) |
| mod+D / mod+shift+D / mod+N / mod+W (`terminalFocus`) | `terminal.split` / `splitVertical` / `new` / `close` | ChatView (drawer or right-panel terminal) |
| mod+shift+M (`!terminalFocus`) | `modelPicker.toggle` | composer model picker |
| mod+1…9 (`modelPickerOpen`) | `modelPicker.jump.N` | select Nth model (`ModelPickerContent.tsx:475-505`) |
| mod+N, mod+shift+O (`!terminalFocus`) | `chat.new` | `_chat.tsx:75-85` |
| mod+shift+N | `chat.newLocal` | `_chat.tsx:63-73` |
| mod+shift+J | `preview.toggle` | toast "Preview is desktop-only" outside desktop |
| mod+R / L / = / + / - / 0 (`previewFocus`) | preview refresh / focusUrl / zoom | preview |
| mod+O | `editor.openFavorite` | OpenInPicker |
| mod+K (`!terminalFocus`) | `commandPalette.toggle` | (palette spec) |
| mod+B | `sidebar.toggle` | (sidebar spec) |
| mod+shift+[ / ] | `thread.previous` / `thread.next` | (sidebar spec) |
| project script keybindings | `script.{id}.run` | run script in terminal (`ChatView.tsx:3874-3880`) |
| Esc | (none) | clears multi-thread selection (`_chat.tsx:57-61`); closes image dialog; closes model picker |
| printable key outside inputs | (none) | type-to-focus composer (2.4) |
| Enter / Shift+Enter | (none) | composer: send (or select menu item / advance question) / newline |
| ↑ ↓ Tab Enter | (none) | composer command menu navigation/select |
| ← → at chip edge, Backspace after chip | (none) | chip-atomic caret move / delete |
| `( [ { ' " “ \` < « * _` with selection | (none) | surround selection |
| 1–9 (focus outside editables) | (none) | pick user-input option |
| ↑ ↓ PgUp PgDn Home End Space (outside editables) | (none) | timeline manual navigation (cancels live follow) |
| ↑ ↓ Home End Enter Space on minimap | (none) | minimap navigation |
| ← → in image dialog | (none) | prev/next image |

Global handlers ignore events while the command palette is open (`_chat.tsx:53`, `ChatView.tsx:3762`). ChatView's handler is registered in the capture phase.

---

## 8. Implementation order

Ordered for "usable and identical-looking as early as possible".

1. **Shell + header + empty states.** Chat column, 52px topbar with title/project, panel toggles (stubbed), NoActiveThread, the empty-thread placeholder, the banner components (thread error, provider status). Uses only primitives.
2. **Timeline v1 (read-only).** The data pipeline 3.1–3.2 as pure Rust functions; port `MessagesTimeline.logic.test.ts` and `session-logic` tests as golden tests. User bubble (text only), assistant markdown (plain paragraphs first), the working row (static text before the shimmer), `MessageScroller` with tail follow and the scroll-to-end pill.
3. **Composer v1.** Card geometry (solid `card` fallback + shadows), multi-line autogrow input (gpui-base `InputState::auto_grow`, 44→200px), Send/Stop button, Enter/Shift+Enter, `thread.turn.start` without attachments/contexts, interrupt, optimistic user message + local dispatch timer. This is the first fully usable chat.
4. **Markdown fidelity.** Headings, lists, task lists, blockquote, inline code, links (no favicons yet), code block chrome + tree-sitter highlighting with the mapped Pierre theme, tables (collapsed/expanded), footnotes, details. Streaming chunk caching + word-paced smoothing.
5. **Work log.** `SimpleWorkEntryRow` (icons, status, expand body), tool-stack (no morph first), work-toggle, turn folds + anchored disclosure, changed-files card (tree), revert button, copy buttons, hover meta rows, timestamps.
6. **Pickers.** Model picker trigger + panel (sidebar, search, favorites, jump keys), traits picker, BranchToolbar (workspace select, branch combobox with pagination/create/start-from-origin), OpenInPicker.
7. **Composer v2.** Inline chips (file/skill/terminal) on gpui-base `InlineToken`; `@`/`$`/`/` triggers + command menu with `projects.searchEntries`; images (paste/drop/limits/thumbnails/dialog); context-window meter; banner stack.
8. **Approvals and user input** panels in the composer header.
9. **Drafts.** Draft sessions, the `bootstrap.createThread` / `prepareWorktree` first send, route swap, draft persistence.
10. **Polish.** Tool-call morph, shimmer, minimap, file chips with context menus, external link favicons, selection-to-markdown copy, ultrathink frame, preview annotation cards, element/review chips. Plan follow-up only if the product re-enables plan mode.

### 8.1 Hardest pieces in GPUI (and approaches)

| Piece | Why hard | Approach |
|---|---|---|
| Inline chips inside the editor | Atomic tokens with custom rendering, caret skipping, backspace-delete, serialization back to a markdown/`$`/U+FFFC string, tooltip on hover | gpui-base 0.7 has this: `InlineToken::new(id, text).with_label(label)`, `InputState::replace_range_with_token`, `Input::token(render_fn)` and `on_token_click` (`gpui-base-0.7.0/src/input/base/inline_tokens.rs:12-40`, `gpui-component-0.7.0/src/input/input.rs:174-194`). Store the serialized form in `text` (`[name](path)`, `$skill`, U+FFFC) so `value()` matches the web prompt string. Render with a custom element that matches 4.4. Verify caret, IME and undo behavior against 4.2. Fallback: a custom `EntityInputHandler` element that draws chip quads over U+FFFC glyph runs. |
| Scroll state machine | Tail follow vs manual navigation vs content growth, composer inset, anchored disclosures | `MessageScroller`/`ListState` with `FollowMode::Tail` (`gpui-component-0.7.0/src/message_scroller.rs`). Detect manual navigation from wheel/scrollbar-drag/keys yourself and keep the generation counter (3.12). Implement the anchored disclosure as "measure anchor bottom → mutate → correct `scroll_to(offset)` each frame until stable". Bottom padding = `contentInsetEndAdjustment`. |
| Streaming markdown perf | Re-parse per token; long responses | Port the frozen-chunk split (`streamingMarkdown.ts`) and cache a parsed tree per frozen chunk; re-parse only the tail. Run the 40ms word pacer on a timer that only notifies the assistant row. |
| Syntax theme parity | Pierre themes are TextMate scope JSON; gpui-component's highlighter uses tree-sitter captures (`highlighter/registry.rs:439-475`) | Write a one-time converter from Pierre `tokenColors` scopes to tree-sitter capture names (`keyword`, `string`, `function`, `type`, `comment`, `constant`, `variable.*`, …). Verify visually against Shiki output for ~10 languages. Alternative: `syntect` with the TextMate theme for exact parity (heavier). |
| Glass / backdrop blur | No element backdrop-filter in GPUI | Use the CSS fallbacks the fork already defines: solid `card` for the composer and lower chrome, solid `background` for the main inset. Keep the box shadows. |
| Container queries / responsive | Header labels at a 768px container, footer compact <620/780, minimap gutter at 864 | Measure element widths in prepaint (GPUI layout bounds) and switch variants. No CSS needed. |
| Tool-call morph and shimmer | Blur + translate + scale animations; continuous shimmer | gpui-component has `shimmer.rs` (duration, highlight color). Morph: animate opacity + y offset with `with_animation`; skip blur (GPUI has no element blur), which is a visible but minor difference. |
| Tables with ellipsis + horizontal scroll + fade | Max-content tables with per-cell truncation | Lay out columns manually: measure each column's max content width, clamp at 384 when collapsed, horizontal scroll container, gradient edge masks. |
| Favicons, Pierre icon sprite, editor brand icons | Network images + an SVG sprite | Fetch favicons with a cache (failed-host set). Export Pierre icons as individual SVG assets (`pierre-icons.ts` + sprite) with the light/dark color table from `PierreEntryIcon.tsx`. |
| Selection-to-markdown copy across rows | Browser selection spans rows | GPUI text selection is per element. Start with per-message copy buttons, then the gpui-base `TextView` selection (`window_selection.rs`), and treat cross-row selection as a later task. |

---

## 9. Data/protocol checklist (what the chat reads and writes)

- **Reads**: thread detail (messages, activities, proposedPlans, latestTurn, session, checkpoints, branch, worktreePath, modelSelection, runtimeMode, interactionMode, completionAcknowledgedAt), project (title, workspaceRoot, scripts, defaultModelSelection), server config (providers[status, auth, message, displayName, driver, instanceId, models, slashCommands, skills, requiresNewThreadForModelChange, continuation.groupKey], availableEditors, keybindings, environment label, version), environment connection presentation, `vcs.status` (isRepo, refName, pr, sourceControlProvider), paginated VCS refs, client settings (timestampFormat, wordWrap, favorites, autoOpenPlanSidebar, newWorktreesStartFromOrigin, provider instance settings).
- **Writes**: `orchestration.dispatchCommand` with `thread.turn.start`, `thread.turn.interrupt`, `thread.approval.respond`, `thread.user-input.respond`, `thread.checkpoint.revert`, `thread.meta.update`, `thread.runtime-mode.set`, `thread.interaction-mode.set`, `thread.completion.acknowledge`, `thread.create`/`thread.delete` (plan-implement path only), `thread.session.stop`. Plus `projects.searchEntries`, `projects.writeFile`, `shell.openInEditor`, `vcs.switchRef`, `vcs.createRef`, `environmentCatalog.retryNow`.

---

## 10. Open questions / risks

1. **Upstream protocol drift** (the server you run is upstream, newer than the fork):
   - Messages can have `role: "reasoning"` (`upstream:packages/contracts/src/orchestration.ts:566-571`). The fork timeline renders nothing for non-user/assistant roles. Decide whether to render reasoning (e.g. as a collapsed `BotIcon` work row) or hide it.
   - `message.context` (`OrchestrationMessageContext`, upstream `composerContext.ts:261`) is optional in upstream `thread.turn.start`. The fork embeds contexts as text blocks, which still works, but the timeline may need to read `message.context` for messages sent by upstream clients.
   - `ProviderApprovalDecision` gains `acceptAlways`. Requests can carry `ProviderApprovalOption[]` with labels and warnings. `requestKind` gains `mcp-elicitation` and `permission`. The fork drops approvals whose requestKind it can't map, so approval UI could silently disappear for those kinds.
   - `RuntimeMode` gains `auto`; attachment limits change (100 images, 80MB total). Check with the protocol agent.
2. **Plan mode is dead code in the fork** (fixed `interactionMode: "default"`). Confirm it should stay removed. If yes, skip 4.8 and the Plan Ready path, and note that every send force-sets threads to `full-access`/`default`.
3. **Enter-while-running sends a steer** with no visual affordance (the Stop button shows). Confirm this is intended before porting.
4. **Continuous animations** (working shimmer 6s loop, ultrathink 10s rainbow, pulsing dots) conflict with the "avoid continuously repainting animations" guideline. Parity requires them. Suggest throttling to ~30fps, or freezing when the window is unfocused. Needs a decision.
5. **Highlighting parity**: Shiki/TextMate vs tree-sitter will differ at the token level; exact parity needs `syntect` + the Pierre JSON. Pick one.
6. **Tooltip delay** values come from Base UI defaults (assumed 600ms open, 0 close). Verify in the running app.
7. **Blur fallbacks** change the look slightly. The desktop app uses native window glass (`desktop-glass-window`, translucent `app-main-glass`). Coordinate with the layout/tokens agent on whether the GPUI window uses native blur so composer translucency could be kept.
8. **LegendList specifics** (`isAtEnd` threshold semantics, `maintainScrollAtEnd` on itemLayout) don't map 1:1 to GPUI `ListState`. Build an end-to-end harness that streams a long response, scrolls up mid-stream, toggles folds and resizes the composer, and compare against the web app recording.
9. **Terminal/element/preview contexts and review comments** depend on features specced elsewhere (terminal drawer, preview browser, diff panel). The composer can ship without them; the timeline must still parse their blocks.
10. **Draft persistence format**: reusing the web localStorage format is impossible from Rust. Users lose existing web drafts on switch. Acceptable?
11. `GitActionsControl`, `ProjectScriptsControl`, the right panel, the terminal drawer, `PlanSidebar` and `PullRequestThreadDialog` are only referenced here. They need their own specs.
