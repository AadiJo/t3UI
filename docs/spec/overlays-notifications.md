# Toasts, dialogs, and app-wide notices: build spec

Refreshed against fe7d3092c. Conventions and path prefixes are in [`overlays.md` §0](overlays.md#0-conventions). The command palette, file picker, and content search are in `overlays.md`.

Scope: everything mounted at the app root or in the sidebar chrome that is not owned by one page (`web/routes/__root.tsx:233-275`, `web/AppRoot.tsx:14-23`, `web/components/sidebar/SidebarChrome.tsx:102-189`). Owned elsewhere: Link PR dialog (PR spec), T3 Connect onboarding and relay client install dialogs (connections spec), theme editor (settings spec), composer banners for server updates and clone progress (chat spec), `/welcome` (pages spec).

Overlap: `shell.md` §5-§6 (toast primitive, glass, context menus, confirm) and `sidebar.md` §7.6, §9 (undo notice, footer, provider pill, update button) describe some of the same surfaces from the shell/sidebar side; the facts agree. This file adds the full copy catalog, data/RPC behind each notice, and the palette-adjacent flows. Where `shell.md` is more detailed (context menu keyboard and edge cases, confirm button metrics), it is canonical.

## 1. Toast system

`web/components/ui/toast.tsx`, `toast.logic.ts`, `toastHelpers.ts`; Base UI toast 1.5.0. Two managers: the main stack (`toastManager`) and element-anchored toasts (`anchoredToastManager`) (`toast.tsx:79-80`). Both providers wrap the whole app (`__root.tsx:234-235`).

### 1.1 Lifecycle and timing

| Rule | Value | Source |
| --- | --- | --- |
| Default timeout | 5000ms | `bui/toast/provider/ToastProvider.js:23` |
| `timeout: 0` | never auto-closes | `bui/toast/store.js:148-151` |
| `type: "loading"` | never auto-closes, whatever the timeout | same |
| Visible limit | 3 active (non-closing) toasts; the oldest extras are marked `limited`: opacity 0, inert. When a newer one closes, a limited one returns. | `ToastProvider.js:24`, `store.js:120-147`, `:236-252` |
| Order | newest first (index 0 = front) | `store.js:118` |
| Same `id` | replaces the live toast in place and restarts its timer | `store.js:91-113` |
| `update(id, ...)` | merges in place; reschedules the timer when the timeout changed or it was loading | `store.js:155-211` |
| Pause | all timers pause while the pointer is over the stack, while a toast has keyboard focus (focus-visible), and while the window is blurred; they resume with the remaining time | `bui/toast/viewport/ToastViewport.js:76-161` |
| `data.dismissAfterVisibleMs` | extra timer that runs only while the document is visible and focused; remaining time survives re-renders | `toast.tsx:454-530` |
| Close button | runs `data.onClose` then closes. Swipe, action buttons, and programmatic closes do not run `data.onClose`. | `toast.tsx:112-119`, `:664-676` |
| Swipe | right or up past 40px dismisses (top-right stack) | `toast.tsx:651-657`, `bui/toast/root/ToastRoot.js:34` |
| F6 | focuses the stack (Base UI) | `ToastViewport.js:51-71` |

### 1.2 Thread scoping

`shouldRenderThreadScopedToast` (`toast.logic.ts:108-131`): a toast with `data.threadRef` shows only while that thread (environment + id) is the route thread; with `data.threadId` only while the route thread id matches; otherwise always. The route thread of a draft route is the draft session's `{environmentId, threadId}` (`toast.tsx:431-452`). Hidden toasts still count toward the limit and keep their timers.

### 1.3 Stack layout

```
viewport   fixed, z 100, top-right: top = 32 + 52 (--workspace-topbar-height) = 84, right 32,
           width min(100% − 64, 360)                                                   toast.tsx:560-579
toast      absolute, top 0, w 100%, rounded 10, `dropdown-glass`, shadow-xl in black/25,
           text --popover-foreground, overflow visible, no text selection, z = 9999 − index, transform-origin top   :591-597
  dropdown-glass: bg --popover at ~84% alpha (popover 18% over popover@--glass-opacity), backdrop blur 12px
                  (dark 16) + saturate, 1px border --contrast-foreground/10; no backdrop-filter → opaque --popover   web/index.css:393-405
  shadow-xl/25:   0 20px 25px −5px rgb(0 0 0/.25), 0 8px 10px −6px rgb(0 0 0/.25)
  ::after         invisible hover bridge: full width, 13px tall, directly below the toast   :599-601
├ close orb  absolute, z 20, top −6, right −6: 24px circle, 1px border --border/60, bg --popover/92,
│            backdrop blur 8, shadow-sm, --muted-foreground; hover bg --popover + --foreground
│            (color/bg transition 150ms); X 12px, stroke 2.25; aria "Dismiss notification"   :105-110, :664-676
└ content    pl 14, text-sm (14/20); opacity transition 250ms                          :677-688
             inline layout:  flex, items-center, justify-between, gap 6, py 12, pr 24 (with trailing controls) or 40
             stacked layout: flex col, gap 8, py 10, pr 14        (data.actionLayout "stacked-end" AND a visible primary action)
   body      flex col, gap 2; inline: flex 1; stacked: pr 20                           :348-377
     row     flex, gap 8:
       icon  box w 16 × h 20 (one line); svg 16 wide. Type icons: error `CircleAlert` --destructive, info `Info` --info,
             success `CircleCheck` --success, warning `TriangleAlert` --warning, loading spinner `LoaderCircle` at 80% opacity
             (spins, 1s linear, continuously repainting). `data.leadingIcon` replaces it (centered in the same box). No type: no icon.   :84-90, :354-369
       title medium, wraps anywhere
     description  --muted-foreground, selectable, wraps; an error description string of 180+ chars clamps to 4 lines   :93-102, :192-199
     [expandable] §1.5
   trailing  flex, items-center, gap 6; inline: no shrink; stacked: full width, right-aligned        :378-414
             order: [copy error] [additional actions] [secondary action] [primary action]
             copy error: shown for `type: "error"` with a string description unless `data.hideCopyButton`. 20px icon button,
               radius 8, --muted-foreground/80 (hover --muted-foreground, no bg); `Copy` 12px → `Check` 12px --success for 2000ms
               after copying; tooltip "Copy error" / "Copied error"                    :121-143, web/hooks/useCopyToClipboard.ts:201
             additional/secondary: Button xs, variant `data.secondaryActionVariant` (default outline)
             primary (`actionProps`): Button xs, variant `data.actionVariant` (default `default`); hidden when its children are empty
             Button xs = h 24, px 7, gap 4, text-xs medium, radius 8 (design-system spec)
```

`stackedThreadToast(options)` (`toastHelpers.ts:31-66`) = the same options with `data.actionLayout: "stacked-end"`. Without an action it still renders the inline layout (`toast.tsx:292-293`). Most error toasts in the app use it.

### 1.4 Stack motion

Variables per toast (`toast.tsx:604-641`): index i, gap 12, peek 12, scale = max(0, 1 − 0.1·i), shrink = 1 − scale, H0 = front toast height, offsetY = sum of heights of the live toasts in front of it (`toast.logic.ts:45-102`).

| State | Transform (top-right) | Height | Content |
| --- | --- | --- | --- |
| collapsed, i = 0 | none | natural | visible |
| collapsed, i > 0 | translateY(i·12 + shrink·H0) scale(scale) | H0 | opacity 0, no pointer events |
| expanded (pointer over the stack, or keyboard focus inside) | translateY(offsetY + i·12), scale 1 | natural | visible |
| entering | from translateX(100% + 32) translateY(offsetY + i·12) | | |
| exiting (close/timeout) | to translateX(100% + 32) translateY(offsetY + i·12), opacity 0 | | |
| exiting by swipe | continues off in the swipe direction by 100% + 32 | | |
| limited | opacity 0 | | |

Collapsed result: each toast behind the front one shows as a slab whose bottom edge sits 12px (i = 1) or 24px (i = 2) below the front toast, 10% narrower per step. Transition: `transform 500ms cubic-bezier(.22,1,.36,1), opacity 500ms, height 150ms` (`:591`). A closing toast keeps its slot while it animates out; the live toasts reflow past it at the same time (`toast.logic.ts:51-92`).

Swipe drag offsets follow the pointer (`--toast-swipe-movement-x/y`) until release.

### 1.5 Expandable details

`data.expandableContent` (`toast.tsx:145-271`):

- Default: below the description, a toggle button: inline-flex, gap 4, py 2, `rounded-md`, text-xs medium --muted-foreground (hover --foreground): `ChevronDown`/`ChevronUp` 14px (stroke 2.25, 80%) + "Show details" / "Hide details" (or `data.expandableLabels`).
- `data.expandableDescriptionTrigger`: the description itself is the toggle: full-width row, gap 6, `rounded-sm`, hover bg muted/40 and underlined description (offset 2, decoration muted-foreground/60), chevron 14px at the right (mt 2). Tooltip = the expand/collapse label. Enter/Space toggle.
- Open panel: mt 8, max-h 160, scrolls (overscroll contained), pr 2, selectable.

### 1.6 Anchored toasts

`AnchoredToasts` (`toast.tsx:705-801`), used only by the message copy button and the diff file path copy button (`web/components/ui/anchoredCopyToast.ts`, `chat/MessageCopyButton.tsx:28-29`, `DiffFilePathCopyButton.tsx:15-16`):

- Positioned above the anchor (Base UI default side top, align center), offset 4, z 100, max-w min(256, available).
- Tooltip style (`data.tooltipStyle`): px 8, py 4, `rounded-md`, text-xs, `dropdown-glass`, shadow-xl/25; title only. Enter/exit: scale .98 + opacity (150ms default transition).
- "Copied!" or "Failed to copy", timeout 1000ms.

## 2. Toast copy catalog (app-wide sources)

Palette-originated toasts are listed in `overlays.md` (§3.1, §6-§8). Below: toasts raised by root coordinators.

### 2.1 Keybindings reload

`EventRouter` (`__root.tsx:613-690`) on each `subscribeServerConfig` event of type `keybindingsUpdated` (`web/components/KeybindingsUpdateToast.logic.ts:20-44`):

- Payload has an issue whose kind starts with `keybindings.` → stacked warning "Invalid keybindings configuration" / issue message, outline action "Open keybindings.json": resolves the preferred editor and calls `shell.openInEditor {cwd: keybindingsConfigPath, editor}`; failure → stacked error "Unable to open keybindings file" / message or "Unknown error opening file.".
- Else success "Keybindings updated" / "Keybindings configuration reloaded successfully.", at most once per 2000ms.

Every `keybindingsUpdated` event triggers this, even when the keybindings did not change. The first config of a session is not an event.

### 2.2 Slow requests

`SlowRpcRequestToastCoordinator` (`web/components/SlowRpcRequestToastCoordinator.tsx`), data `web/rpc/requestLatencyState.ts`:

- Every unary RPC is tracked from send to ack, except methods containing "subscribe", `pullRequests.*`, `previewAutomation.connect`, `server.getUsageSummary` (`requestLatencyState.ts:30-62`). Threshold 15000ms; 120000ms for `server.updateProvider`, `server.refreshProviders`, `server.updateServer` (`:7-39`). At most 256 tracked.
- One toast while any request is past its threshold: warning, `timeout: 0`, title "Some requests are slow", description "<n> request[s] waiting longer than <s>s." (s = smallest threshold in the batch, rounded), description acts as the expand toggle with labels "Show requests" / "Hide requests" (`:6-64`). Updated in place as the set changes; closed when empty.
- Details list (`:16-32`): stacked items 10px apart, text-xs --muted-foreground; each: tag (`<method> · <environmentId>`, medium --foreground, wraps), then "Started <locale time>" (10px, 75% opacity, mt 2); items separated by a bottom border --border/50 with pb 8 (none on the last).

### 2.3 Thread alerts

`ThreadNotificationCoordinator` (`web/components/ThreadNotificationCoordinator.tsx`), one instance per environment, from the env's shell stream. Off by default: needs `notificationMode != "off"` or `inAppNotificationsEnabled` (both default off, `contracts/settings.ts:299-302`).

- Alert kinds per thread (`web/threadNotifications.ts:122-161`): "input" when the thread newly needs input/approval or newly failed (keyed by `turnId:status`); "completion"/"monitoring" when a newer turn completion appears and the agent has stopped (status ready or monitoring). The first snapshot only seeds memory. Archived, settled, and auto-send-queued threads are skipped. Claude threads without server-reported background work wait for a background-wake check before announcing (`:257-270`, chat spec).
- In-app toast when `inAppNotificationsEnabled`, the window is visible and focused, and the alert is not for the route thread (`:150-189`): type success (completion/monitoring), error (failed), else warning; title "Thread completed" / "Thread monitoring" / "Approval needed" / "Thread failed" / "Input needed"; description = thread title; leading icon 16px: `CircleCheck` --success-foreground, `ShieldQuestion` --warning-foreground, `CircleAlert` --destructive-foreground, `MessageCircleQuestion` --info-foreground; action "Open thread" (closes, navigates); no copy button; default 5s. A newer alert for the same thread closes the previous toast.
- Sounds (`notificationMode` "sound" / "notifications-and-sound"): completion or input chime (`web/assets/notification-*.mp3`).
- OS notifications (window not focused, mode includes notifications, and not the macOS inbox path): title as above, body = thread title, click focuses and navigates. On macOS the desktop shell owns OS notifications through the inbox (§5.6); the dock badge counts pending notifications.

### 2.4 Project clone progress

`ProjectCloneToastCoordinator` (`web/components/ProjectCloneToastCoordinator.tsx`), one per environment, data from `subscribeProjectClones {}` (list of `ProjectCloneSnapshot`, `contracts/projectClone.ts:34-58`), only on envs with `capabilities.projectCloneTracking` (`web/state/projectClones.ts:19-28`). One toast per clone, updated in place; nothing shown while the user is viewing that project's draft (the composer banner shows it instead) (`:85-121`).

| Phase | Toast |
| --- | --- |
| running | stacked loading "Cloning <name>", description `projectCloneProgressSummary` = "<Stage>[ · <n>%][ · <detail>]" with stages "Connecting", "Counting objects", "Receiving objects", "Resolving deltas", "Checking out files"; `timeout: 0`; action "Cancel" → `projectClone.cancel {projectId}` (failure: stacked "Failed to cancel clone") |
| done | stacked success "Cloned <name>" / destination path, timeout 8000ms, action "Open project" (closes, opens a draft) |
| failed | stacked error "Failed to clone <name>" / error or "The clone failed.", `timeout: 0`, action "Retry" → `projectClone.retry` (failure "Failed to retry clone"), secondary "Remove project" |
| cancelled | stacked info "Cancelled cloning <name>" / destination path, `timeout: 0`, same actions, no copy button |

Name = repository `nameWithOwner`, else the destination folder (`contracts/projectClone.ts:105-122`). "Remove project" (`web/hooks/useRemoveClonedProject.ts`): `project.delete {projectId}` (not forced); failure → stacked "Failed to remove project"; success clears the project's draft and leaves its draft route. A clone the server drops closes its toast unless it is a settled success.

### 2.5 Provider update prompts

`ProviderUpdateLaunchNotification` (`web/components/ProviderUpdateLaunchNotification.tsx:26-48`), mounted when the primary env is authenticated. Uses the multi-environment flow (§2.5.2) when any catalog env is desktop-local (`local:` connection id, i.e. WSL), else the primary flow (§2.5.1). On macOS: always the primary flow.

Shared data (`contracts/server.ts:160-263`): `ServerProvider.versionAdvisory {status: "unknown"|"current"|"behind_latest", currentVersion, latestVersion, updateCommand, canUpdate, canInstallVersion?, checkedAt, message}`, `compatibilityAdvisory`, `updateState {status: "idle"|"queued"|"running"|"succeeded"|"failed"|"unchanged", startedAt, finishedAt, message, output}`. Fed by `server.getConfig` and `subscribeServerConfig` `providerStatuses` events. Update RPC `server.updateProvider {provider, instanceId}` → `{providers}`; returns when the update finishes (server timeout 5 min); the client serializes calls per environment (`cr/state/server.ts:631-637`, `:1111-1116`).

Candidates (`ProviderUpdateLaunchNotification.logic.ts:74-226`): enabled providers with `versionAdvisory.status == "behind_latest"`, a `latestVersion`, and `compatibilityAdvisory.latestVersionStatus` not `broken`/`unsupported`; deduped by driver (representative: the default instance `instanceId == driver`, else latest `checkedAt`). One-click = `canUpdate`, has `updateCommand`, all same-driver candidates share it, and not queued/running. Display names: "Antigravity", "Codex", "Claude", "Cursor", "Grok", "OpenCode" (`contracts/model.ts:217-224`). Versions get a "v" prefix unless present. Lists read "A", "A and B", "A, B, and C".

#### 2.5.1 Primary flow

`web/components/ProviderUpdatePrimaryNotification.tsx`.

- Key = sorted `driver:latestVersion` joined "|". Shown once per app session per key (module-level set) and never again after the user closes it with the X (persisted in localStorage `t3code:provider-update-dismissals:v1`, `web/providerUpdateDismissal.ts`). A key change closes an open prompt.
- Prompt (`:268-303`): stacked warning, `timeout: 0`, no copy button. Title "Update Available: <Name> v<x>" (one candidate) or "Updates Available: <n> providers". Description "Install the update now or review provider settings." (any one-click) or "<list> can be updated from provider settings.". Actions (all outline): with one-click → [Settings] [Update]; else [Settings]. Leading icon (one candidate only): 16px provider logo with a 12px `--popover` circle badge at −4/−4 holding `Download` 10px --success (stroke 2.5); without a logo, `Download` 16px --success.
- "Settings": close, navigate `/settings/providers`. "Update": close, run `server.updateProvider` for each one-click provider in turn. No toast while running (the sidebar pill shows progress, §5.3).
- Result toast (`:59-97`, logic `:266-332`, `:590-598`), all `timeout: 0`, no copy button:
  - any RPC failure → stacked error "Provider update failed" / "Provider updates failed", description = error message or "Provider update failed.", action "Settings" (outline).
  - a provider `failed` → same titles (by failed count), description = its `updateState.message` (single) or "<list> failed to update. Check provider settings for details.", action "Settings".
  - `unchanged` → stacked warning "Provider still needs an update" / "Providers still need updates", "<list> still appears outdated. Check provider settings for details." ("still appear" for several), action "Settings".
  - all succeeded → plain success "Provider updated" / "Provider updates finished", "New sessions will use the updated provider." / "...providers.", `dismissAfterVisibleMs: 3000`, no action.

#### 2.5.2 Multi-environment flow (Windows + WSL)

`ProviderUpdateLaunchNotification.tsx:58-198`, rows `ProviderUpdateEnvironmentRows.tsx`. Not reachable on macOS; summary only. Waits up to 30s while a local backend is connecting; one stacked warning (same title formula over all envs), leading `Download` 16px --success, action "Settings", description = live rows: per env, label ("Windows"/"macOS"/"Linux" for the primary, env label otherwise) + status line (12px, --destructive / --warning / --success / --muted-foreground) + trailing spinner 16, `Check` 16 --success, or xs outline "Retry"/"Update". Strings: "Updated", "Updating…", "Update timed out — try again." (6 min), "Provider update failed.". Persisted key `envId=driver:ver,...|...`.

### 2.6 "Update all" providers

`web/components/ProviderUpdatesAction.tsx`, mounted in the Providers settings header (settings spec). Result toast (`ProviderUpdateLaunchNotification.logic.ts:350-387`): stacked, default 5s, no action. Success "Provider updated" / "<n> providers updated", "New sessions will use the updated provider(s).". Else error "<f> of <n> provider updates failed" (partial) / "Provider update failed" / "Provider updates failed"; description = one line per failure "<machine> · <Name>: <message>" (fallbacks "Provider update failed.", "Provider update did not finish."), preserving newlines; no copy button (description is not a plain string).

### 2.7 Server self-update

`web/components/ServerUpdateAction.tsx`, mounted by the composer banner, auto-balance banner, and Connections settings (chat/settings specs). RPCs: `server.updateServer` or the streamed `server.updateServerWithProgress` when `capabilities.serverSelfUpdateProgress`, then `server.commitDesktopUpdate` for desktop-managed servers, then wait up to 4 min for a `subscribeServerLifecycle` ready event (`cr/state/server.ts:683-910`).

- Success (default 5s): "<server label> updated" / "Desktop app relaunched on <v>." (desktop-managed) or "Reconnected on t3@<v>." (`ServerUpdateAction.tsx:74-81`).
- Failure: error "<failure title, default 'Server update failed'>" / message or "Server update failed." (copy button shown) (`:83-87`). Resume timeout message: "The server did not resume on t3@<v>." (`cr/state/server.ts:99-101`).
- Manual path (no `serverSelfUpdate` capability): button "Copy update command" copies `npx t3@<v>`; success "Update command copied" / "Run `npx t3@<v>` on <server label> to update it." (literal backticks); failure "Could not copy update command".
- Confirms (§3.2): "Update the T3 Code desktop app that runs the <server label>? It will close and relaunch on that machine." and, bulk, "Update the T3 Code desktop apps on <labels>? They will close and relaunch on those machines.".
- Stage labels: "Downloading…" (downloading, installing), "Restarting…" (resuming) (`:20-27`).

### 2.8 Other root sources

| Source | Toast |
| --- | --- |
| Desktop app update | §5.5 |
| SnapShot capture (`web/components/desktop/SnapShotCoordinator.tsx:285-381`, desktop screenshot-to-composer feature) | stacked error "Snapshot taken, but no project is available" / "Add a project, then capture the window again."; stacked error "Snapshot failed" / "Capture <id>: <message>" or message or "Try the capture again." |
| Notifications inbox (§5.6) | error "Could not mark notification as read" (no description) |
| Release notes link | error "Unable to open release notes" (`web/components/desktopUpdate.toast.tsx:135-145`) |
| Hosted-web provider auth callback (`ProviderAuthCallbackCoordinator`) | web only; not on desktop |

## 3. Dialogs

### 3.1 Dialog chrome

`web/components/ui/dialog.tsx`, `alert-dialog.tsx`, `dialog-styles.ts`:

```
backdrop   fixed inset 0, z 50, `dialog-backdrop` (bg --background/60, dark /64, blur 4), fade 200ms
viewport   fixed inset 0, z 50, grid rows [1fr auto 1fr], items centered, p 16 → popup vertically centered
popup      row 2, w 100%, max-w 512 (`max-w-lg`), max-h 100%, flex col, rounded 18, 1px border, `dialog-glass` (overlays.md §2),
           text --popover-foreground; enter/exit scale .98 + opacity, 200ms ease-in-out
           nested dialogs: each level behind scales −10%, fades −10%, moves up 20px          dialog-styles.ts:7-10
  [close]  Dialog only (default on): ghost icon button 32px at top 8, right 8, `X` 16, aria "Close"   dialog.tsx:91-99
  header   flex col, gap 8, p 24 (pb 12 when a panel follows)                            dialog.tsx:106-117
    title  text-xl (20) semibold, line-height 1, wraps anywhere
    description  text-sm --muted-foreground
  panel    ScrollArea (scroll fade by default) > p 24, pt 4 after a header, gaps 16          dialog.tsx:166-182
  footer   flex row, justify-end, gap 8, px 24, py 16, border-top, bg --muted/72, bottom corners 17   dialog.tsx:119-136
```

AlertDialog: same popup (max-w 512), header p 24 left-aligned, footer identical; no close button; Escape cancels; backdrop clicks do nothing (Base UI forces `disablePointerDismissal` for alert dialogs, `bui/dialog/root/useRenderDialogRoot.js:30-33`); initial focus is the first focusable control (`alert-dialog.tsx:40-99`). A plain Dialog closes on Escape and on backdrop press.

### 3.2 Confirm dialog (replaces the native message box)

`web/components/ConfirmDialogHost.tsx`, store `web/confirmDialog.ts`. `LocalApi.dialogs.confirm(message, {variant?})` resolves through this in-app AlertDialog whenever the host is mounted, on desktop too (`web/localApi.ts:16-18`); with no host it resolves `false` (callers that pass `?? true` proceed).

- Queue: one dialog at a time; later requests wait until the previous close animation finishes (`confirmDialog.ts:80-124`). Unmounting the last host resolves everything `false`.
- Copy split (`ConfirmDialogHost.tsx:26-52`): trim the message; the first line ending with "?" is the title and the remaining lines (joined with "\n", trimmed) the description; else, if a "?" occurs, title = text through the first "?", description = the rest; else title "Confirm action", description = the message or "This action requires your confirmation.". Description keeps line breaks (`whitespace-pre-line`).
- Footer: outline "Cancel" (closes → false) and "Confirm" (variant `default` or `destructive` per option → true). Escape → false; backdrop clicks do nothing. Initial focus is "Cancel", so Enter cancels unless the user tabs to Confirm. Same spec, with button metrics: `shell.md` §6.3.
- Callers include thread archive/delete/multi-delete, project remove, worktree delete, terminal close, restore defaults, desktop app install (§5.5), server desktop-app update (§2.7), provider setup, diagnostics process kill (`grep dialogs.confirm`). Destructive variant: `ChatView.tsx:5161`, `LegacySidebar.tsx:1670`.

### 3.3 SSH password prompt

`web/components/desktop/SshPasswordPromptDialog.tsx`. Trigger: desktop bridge `onSshPasswordPrompt(request)` while connecting to an SSH environment; requests queue FIFO, one shown at a time (`:31-61`). Request: `{requestId, destination, username?, prompt, expiresAt}` (`DesktopSshPasswordPromptRequest`, `contracts/ipc.ts`).

- Dialog, max-w 448, no close button (`:159`).
- Title "SSH Password Required". Description: "T3 needs your SSH password to connect to `<user@host>`. The password is passed to the local SSH process for this connection attempt and is not saved by T3 Code." (`<user@host>` in `code`; host alone without a username).
- Panel (no scroll fade), form, gaps 12: row (justify-between, gap 12): prompt text (text-sm medium --foreground) + countdown "m:ss" (text-xs --muted-foreground; "Expired" text-xs medium --destructive), ticking every 1s; password input (focused and selected on the next frame; disabled while responding or expired); help line text-sm: "Use SSH keys to avoid repeated password prompts on new SSH sessions." (--muted-foreground) or the error (--destructive).
- Footer: outline "Cancel" ("Dismiss" once expired), primary "Continue" (submits; disabled while responding or expired). Enter submits.
- Respond: `resolveSshPasswordPrompt(requestId, password | null)`. Cancel sends null; Dismiss (expired) just drops it. Submit failure shows "This SSH password prompt expired. Try connecting again." for expired/no-longer-pending errors, else the message. Escape = Cancel.
- Native: the SSH transport is T3UI's own (connections spec); this dialog is the UI it calls into.

### 3.4 ChatGPT plan connected

`web/components/settings/ChatGptWelcomeCoordinator.tsx`: shown once per `[environmentId, instanceId, profileId|email|"default"]` for connected envs whose provider has `auth.status == "authenticated"` and `auth.subscriptionSharing` (`shared/usageLimits.ts:31-33`); acknowledgements in localStorage `t3:chatgpt-sharing-welcome:v1`.

- Dialog (max-w 512, close button). Header: OpenAI mark 32px (mb 8), title "Your ChatGPT plan is connected", description "Eligible usage in T3 Code uses your ChatGPT plan. Manage your shared usage and any credit settings in ChatGPT.", then text-xs --muted-foreground "<provider name> on <environment label>".
- Footer: ghost-muted sm "Manage usage" + `ExternalLink` 14 (opens the ChatGPT usage URL) and primary "Continue" (acknowledge). Closing also acknowledges.

### 3.5 Custom snooze

`web/components/CustomSnoozeDialog.tsx`, opened by `requestCustomSnooze()` from thread snooze menus (sidebar spec); resolves `{snoozedUntil}` or null.

- Dialog, max-w 384. Title "Custom snooze", description "Choose when snoozed threads return to your inbox.".
- Panel: toggle group (full width, two equal segments) "Date and time" | "Duration" (aria "Schedule type").
  - Date and time: two columns, gap 12: "Date" outline button (full width, label like "Oct 2, 2026", `Calendar` 16 muted) opening a calendar popover (past days disabled); "Time" native time input (h 32). Default = now + 1h.
  - Duration: "Snooze for" number field with −/+ (aria "Decrease duration"/"Increase duration"), default 2; "Unit" select "Minutes"/"Hours"/"Days", default Hours.
  - Error (role alert, --destructive): "Choose a valid date and time in the future." / "Enter a positive duration.".
- Footer: outline "Cancel", primary "Snooze" (submit).

## 4. Sidebar footer layout

`SidebarChromeFooter` (`web/components/sidebar/SidebarChrome.tsx:180-189`): `SidebarFooter` = flex col, gap 8, px 8, py 4 (`web/components/ui/sidebar.tsx:570-579`). Children top to bottom:

1. Thread undo notice (§5.1)
2. Provider update pill (§5.3)
3. Intel-build warning (§5.4)
4. Utility row (`:102-178`): `ul` flex row, items-center, gap 4. Off utility pages: icon buttons 32×32 (radius 8, `--sidebar-icon-color` icons 16, hover bg `--sidebar-row-hover`) "Settings", "Pull Requests" (when any env supports PRs), "Usage", each with a top tooltip of the same text; on utility pages a full-width "Back" row instead. Then the notifications bell (§5.6), then the desktop update button pushed right (§5.5).

## 5. Notices

### 5.1 Thread undo notice

`web/hooks/showThreadUndoNotice.ts`, `web/components/sidebar/SidebarThreadUndoNotice.tsx`.

- Raised after a successful archive, settle, unpin, or snooze from any surface (`web/hooks/useThreadActions.ts:340-345`, `:666`, `:720-741`, `:879`). Consecutive actions of the same kind group into one notice; a different kind starts a new group (only the latest group is shown). Each action holds a claim (`web/hooks/threadUndo.ts`): a later action of the same kind on the same thread (or a settle, which invalidates pin/snooze claims) expires it.
- Expires 5000ms after the last action (all claims finished) (`showThreadUndoNotice.ts:95-106`).
- Alert `sidebar` variant (`web/components/ui/alert.tsx:20-21`): `rounded-lg` (10), 1px border --sidebar-border, bg --sidebar-control-surface, px 8, py 6, 11px/16, description block in --sidebar-muted-foreground, role status.
- Text: "<Archived|Settled|Unpinned|Snoozed> <n> thread[s], " + inline button (medium, --foreground, underline on hover, offset 2): "<shortcut> to undo" when `thread.undo` is bound (default mod+z, `when: !terminalFocus && !editableFocus`, label "⌘Z"), else "Undo".
- Undo (button, or mod+z through `_chat.tsx:83-90`, skipped while the palette or model picker is open): consumes the group, runs every inverse at once: unarchive (and returns to the thread if archiving had moved the user to a draft), unsettle (+ re-pin and re-snooze to the prior values), pin with the old order key, unsnooze. Failures: stacked error "Failed to undo archive" / "Failed to undo settle" / "Failed to undo unpin" / "Failed to wake thread". Commands belong to the sidebar spec.

### 5.2 Provider update pill

`web/components/sidebar/SidebarProviderUpdatePill.tsx`; view logic `ProviderUpdateLaunchNotification.logic.ts:426-577`. Reads the primary env's providers only.

- Loading (any deduped provider queued/running): title "Updating <Name>" / "Updating <n> providers"; description "<Name> update in progress." / "<list> updates are in progress.".
- Terminal results only when `updateState.finishedAt` >= the newest provider `checkedAt` seen at first load (older results stay hidden). Most recent `finishedAt` wins, skipping dismissed keys:
  - failed (dismissible, error tone): "<Name> v<latest> update failed" (or "<Name> update failed") / "<n> provider updates failed"; description as §2.5.1.
  - unchanged (dismissible, warning tone): "<Name> still needs an update" / "<n> providers still need updates"; "<list> still appears outdated. Review provider settings for details." ("still appear").
  - succeeded (success tone, auto-dismiss 3000ms by plain timer): "<Name> updated: v<installed>" (or "<Name> updated") / "<n> providers updated"; "New sessions will use the updated provider." / "...providers.".
- Box (`:127-134`): min-h 28, full width, `rounded-lg`, overflow hidden, 11px/16 medium. Tones: loading/success bg --sidebar-control-surface + --sidebar-foreground (hovered main button: --sidebar-row-hover); warning bg warning/12 + --warning (hover warning/18); error bg destructive/12 + --destructive (hover destructive/18).
- Main button: flex-1, gap 8, px 8, py 6, left-aligned; icon 14px: spinner (loading, continuously animated), `CircleCheck` (success), `TriangleAlert` (error), `Download` (warning); title wraps. aria-label and top tooltip = description. Click → `/settings/providers`.
- Dismiss (dismissible only): 20px button, mr 4, radius 8, opacity .7 (hover/focus 1), `X` 14; aria "Dismiss provider update notice"; tooltip "Dismiss until provider status changes". Dismissal is per session.
- Countdown bar (success): absolute full-height layer from the left, bg foreground/8, right border current/15, shrinking scaleX 1 → 0 linearly over 3000ms (one-shot).
- View change motion: the old view exits (translateY 6, opacity 0) over 180ms `cubic-bezier(.32,.72,0,1)` (`--ease-drawer`, `web/index.css:178`), then the new view enters the reverse way.

### 5.3 Intel build warning

`SidebarUpdateArchitectureWarning` (`SidebarUpdatePill.tsx:91-109`), when `hostArch == "arm64"` and `appArch == "x64"` (`web/components/desktopUpdate.logic.ts:50-52`): Alert `warning` variant (`rounded-xl`, 1px border warning/32, bg --warning-surface, px 14, py 12, text-sm --warning-foreground; description --warning-foreground/80; icon --warning): `TriangleAlert` 16, title "Intel build on Apple Silicon", description (`desktopUpdate.logic.ts:58-71`): "This Mac has Apple Silicon, but T3 Code is still running the Intel build under Rosetta." + " Download the available update to switch to the native Apple Silicon build." (download action) / " Restart to install the downloaded Apple Silicon build." (install) / " The next app update will replace it with the native Apple Silicon build.".

### 5.4 Desktop app update button and release notes

`web/components/sidebar/SidebarUpdatePill.tsx:111-419`, `DesktopUpdateStatusIcon.tsx`, `SidebarUpdateReleaseNotes.tsx`, `desktopUpdate.logic.ts`, `desktopUpdate.toast.tsx`. Desktop only. Data: `DesktopUpdateState` (`contracts/ipc.ts:84-111`, `:283-353`) `{enabled, status: "disabled"|"idle"|"checking"|"up-to-date"|"available"|"downloading"|"downloaded"|"error", channel: "latest"|"nightly", currentVersion, hostArch, appArch, availableVersion, downloadedVersion, releaseNotes: [{version, items, totalItems}], omittedReleaseCount, downloadPercent, checkedAt, message, errorContext: "check"|"download"|"install"|null, canRetry}`; bridge `getUpdateState`, `onUpdateState`, `checkForUpdate`, `downloadUpdate`, `installUpdate`. Natively this is T3UI's own updater (missing).

Action (`desktopUpdate.logic.ts:28-48`): install when a downloaded version exists and status is `downloaded` or an `error` in install context (or no context); download when `available`, or an `error` in download context with an available version; else none (check).

Button (`SidebarUpdatePill.tsx:300-348`): 32px circle, focus ring 2px.
- With an update (action != none or downloading): bg --sidebar-control-surface, --sidebar-foreground, hover --sidebar-row-hover.
- Otherwise: `--sidebar-icon-color`, hover bg --sidebar-row-hover + --sidebar-foreground; 60% opacity when disabled.
- Disabled: while checking; while downloading; when it cannot check (`!enabled` or status checking/downloading/disabled). Cursor not-allowed.

Icon (`DesktopUpdateStatusIcon.tsx`), 16px:
- idle: `RefreshCw`; checking: the same spinning (continuously, 1s linear, finishes the current turn after the check ends; static with reduced motion).
- available: `Download` with a 6px dot in currentColor at top-right (−2/−2) ringed 2px --sidebar-control-surface.
- downloading: `Download` inside a 32px progress ring (r 14, stroke 1.5: track current/22, progress current with round cap, starting at 12 o'clock, dashoffset transition 300ms ease-out).
- downloaded: `RotateCw` with a 10px circle at bottom-right (−4/−4), bg --foreground, ring 2px --background, holding `Check` 8px (stroke 3) in --background.

Tooltip (top, glass variant when an update exists), `getDesktopUpdateButtonTooltip` (`desktopUpdate.logic.ts:73-98`): "Update <v> ready to download" | "Downloading update (<n>%)" | "Update <v> downloaded. Click to restart and install." | "Download failed for <v>. Click to retry." | "Install failed for <v>. Click to retry." | message or "Update failed" | "Up to date"; without update details: "Checking for updates…" / "Check for updates". aria-label = tooltip.

Click (`SidebarUpdatePill.tsx:176-289`):
- download → `downloadUpdate()`; completed → success toast "Update downloaded" / "Restart the app from the update button to install it." + inline "Read more" link (muted, dotted underline offset 4, `ArrowRight` 12 rotated −45°) opening `https://github.com/pingdotgg/t3code/releases/tag/v<version>`; errors: stacked "Could not download update" / message, "Could not start update download" / message.
- install → confirm (§3.2) "Install update <v> and restart T3 Code?\n\nAny running tasks will be interrupted. Make sure you're ready before continuing." → `installUpdate()`; errors: "Could not confirm update", "Could not install update".
- none → `checkForUpdate()`; not checked → stacked "Could not check for updates" / message or "Automatic updates are not available in this build.".

Release notes popover (nightly channel with notes and an update to show) (`SidebarUpdatePill.tsx:350-416`, `SidebarUpdateReleaseNotes.tsx:172-246`): opens on hover (close delay 150ms) and on keyboard focus / forward Tab; clicking the button never toggles it; replaces the tooltip. Popover: tooltip style (`rounded-md`, text-xs, dropdown-glass, `shadow-md/5`, px 8, py 4), side top, centered, max-w min(384, 100vw − 32), aria "Nightly update release notes".
- Header (px 4): "Update ready to download" (text-sm medium, no wrap) + version (text-xs --muted-foreground, mt 2) when available; else the tooltip text (text-sm medium).
- Body: scrolls, max-h min(448, 100vh − 96), px 4, pt 16, pb 4. Per release: separator (my 12) between releases; heading "What's changed" (first) / "Changes in <version>" (text-xs semibold --foreground); bullet list (mt 8, gaps 6, pl 16, text-xs/20, --popover-foreground/90, disc); link (mt 8, text-xs/20 --muted-foreground, dotted underline offset 4, `ExternalLink` 12): "View release on GitHub" or "<n> more change[s] on GitHub". Footer link when releases were omitted: "<n> older release[s] on GitHub" → releases page.

### 5.5 Notifications inbox (macOS)

`web/components/sidebar/SidebarNotifications.tsx`, `web/desktopNotifications.ts`, `web/components/desktop/DesktopNotificationCoordinator.tsx`; model `desk/notifications/model.ts`. Shown only when `notificationInboxEnabled` (default false, `contracts/settings.ts:474`) on macOS desktop.

- Data: the renderer pushes per-env thread signals (`syncNotifications {notificationMode, environmentIds, threads: [{environmentId, threadId, threadTitle, projectTitle, environmentTitle, turnId, turnState, completedAt, approval, input, archived, settled, snoozed, backgroundWorking}]}`); the desktop side derives items `{id, environmentId, threadId, kind: "approval"|"input"|"failed"|"completed", threadTitle, projectTitle, environmentTitle, createdAt, read, resolved}` (`contracts/desktopNotifications.ts:13-25`), posts OS notifications, and persists the inbox. Commands: `{type: "read", ids}`, `{type: "read-thread", environmentId, threadId}`. Viewing a thread while focused marks its items read. Natively, port `desk/notifications/model.ts` into T3UI.
- Button: 32px sidebar icon button, `Bell`; 6px --destructive dot at top 4, right 4 when unread; aria "Notifications, unread items" / "Notifications"; tooltip "Notifications".
- Popover (side top, align start, offset 10, w 384, padding 16): header (mb 12, justify-between): "Notifications" (text-sm semibold) + "Mark all read" (text-xs --muted-foreground, hover --popover-foreground, 40% when nothing unread). Tabs row (gap 16, border-bottom, pb 8, text-xs): "Unread (<n>)" / "All", active --popover-foreground, inactive --muted-foreground. List max-h 384, scrolls; empty "No unread notifications" / "No notifications" (py 24, centered, text-xs muted).
- Item (border-bottom except last): button (flex-1, gap 8, py 12, hover bg --accent + --accent-foreground): icon 16 (mt 2): approval `ShieldAlert` --warning-foreground, input `CircleHelp` --info-foreground, failed `CircleAlert` --destructive-foreground, completed `Check` --success-foreground; text: thread title (text-xs medium, truncate), "Approval required" / "Input required" / "Turn failed" / "Turn completed" + " · Resolved" (text-xs muted), "<project> · <environment>" (11px muted, truncate). Unread: trailing `Check` 14 button (p 4, muted, hover --popover-foreground), aria "Mark <title> as read", tooltip "Mark as read". Click opens the thread and marks it read.

## 6. Quit hold overlay

`web/components/QuitHoldOverlay.tsx`, desktop logic `desk/window/QuitHold.ts`. The quit accelerator (⌘Q) is intercepted before the menu; menu Quit always quits. Mode = client setting `confirmQuit`: "hold" (default), "double-click", "direct" (`contracts/settings.ts:229-243`).

- Any mode: a second press within 500ms quits.
- hold: on press, show the hint; quit once the key has been held (auto-repeat) for 1200ms, then wait for key-up or 600ms of no repeats. Release earlier: the hint lingers 1200ms.
- double-click: show "Press ⌘Q again to quit" for the rest of the 500ms window.
- Overlay: fixed, full width, top 22%, z 100, centered, no pointer events, role status. Pill: `rounded-full`, bg --foreground/95, px 32, py 16, text-2xl (24/32) bold, --background, shadow-xl. Text: "Hold ⌘Q or press twice to quit" (hold) / "Press ⌘Q again to quit" ("Ctrl+Q" off mac).

## 7. Context menus

Canonical spec: `shell.md` §6.2 (keyboard, edge cases, icon paths). Summary: `web/contextMenuFallback.ts`. `LocalApi.contextMenu.show(items, {x, y})` renders this DOM menu on every platform, desktop included; only text-editing menus stay native (`web/localApi.ts:43-56`). No open/close animation; keyboard is Escape plus Tab between rows (no arrow keys). Item: `{id, label, icon?, destructive?, disabled?, checked?, header?, separatorBefore?, children?}` (`ContextMenuItem`, `contracts/ipc.ts`).

- Menu (`:296-310`): fixed at the pointer, clamped 4px inside the window, z 10000, min-w 128, max-w 384, `rounded-lg`, `dropdown-glass`, overflow hidden; inner p 4, max-h min(384, 70vh), scrolls.
- Separator (before an item with `separatorBefore`, not first): 1px --border, mx 8, my 4. Header item: px 8, py 6, text-xs medium --muted-foreground.
- Row: button, flex, gap 8, min-h 28, px 8, py 4, `rounded-sm`, text-sm/20, --foreground; icon 16 --muted-foreground (icon names: archive, check, timer, chevron-right, circle-check, clock, pencil, copy, folder, git-branch, hash, mail-open, message-square-plus, pin, pin-off, refresh-cw, settings, folder-tree, trash, `:6-137`). Checked rows (`checked` boolean) use the icon slot for a `Check` or an empty 16px spacer. Disabled: --muted-foreground, 64% opacity, no pointer.
- Destructive leaf (`destructive` or id "delete"): --destructive-foreground text and icon; highlight bg destructive/10.
- Highlight (hover moves focus): bg --accent, --accent-foreground. Submenu rows: `ChevronRight` 16 at the right (80% muted); hover or click opens the child menu at the row's right + 4 (flipped left if it overflows); hovering a leaf or the parent menu closes deeper menus.
- Close: Escape, pointerdown or right-click outside (enabled one frame after opening), choosing a leaf (resolves its id). Only one menu at a time; a new one dismisses the previous (resolves null). Focus returns to the previously focused element if focus was inside the menu.

## 8. First-run gate

`web/components/onboarding/FirstRunGate.tsx` wraps the authenticated app tree (`__root.tsx:243-272`): renders nothing until it knows whether to show the app or redirect to `/welcome` (pages spec). Decision: `onboardingCompletedAt` set → app at once; else wait for shells and judge the workspace fresh or not (`web/onboarding/firstRun.logic.ts`).

Recovery screens (`:200-238`): full window, bg --background, centered column max-w 384: title text-lg semibold, text-sm muted line (mt 8), outline sm button (mt 20) with `RefreshCw` (spinning while retrying):
- Settings failed to load: "Could not read settings" / "Your saved settings could not be loaded." / "Retry" (re-hydrate).
- No decision after 4000ms (after settings hydrate): "Still connecting" / "T3 Code could not confirm this workspace." / "Reload".

## 9. Data and API map

| Need | Upstream | t3UI today |
| --- | --- | --- |
| keybindings reload events | `subscribeServerConfig` `keybindingsUpdated` | `ServerConfigStreamEvent` decoded; notifier compares config snapshots instead of events (`t3-app/src/notifications.rs`) |
| open keybindings file | `shell.openInEditor` | `ShellOpenInEditor` (exists) |
| RPC latency | every unary request | missing: `t3_client::rpc` needs per-request start/ack hooks |
| clone progress | `subscribeProjectClones`, `projectClone.cancel`, `projectClone.retry`, `project.delete` | stream + cancel/retry missing; `project.delete` command exists |
| provider advisories | `ServerProvider.versionAdvisory`, `updateState`, `compatibilityAdvisory` | `version_advisory: Option<Value>` untyped; `updateState`, `compatibilityAdvisory` not decoded (`t3-protocol/src/server.rs:263`) |
| provider update | `server.updateProvider` | `ServerUpdateProvider` (exists) |
| server self-update | `server.updateServer`, `server.updateServerWithProgress`, `server.commitDesktopUpdate`, `subscribeServerLifecycle` | lifecycle stream exists; update methods missing |
| thread alerts | shell stream | `ShellState` (exists) |
| desktop update | T3UI updater | missing (native) |
| notifications inbox | desktop notification model | missing (native port of `desk/notifications/model.ts`) |
| SSH prompt | desktop SSH transport | depends on the native SSH transport (connections spec) |
| confirm | in-app AlertDialog | `t3-app/src/dialogs.rs` uses a native `Window::prompt`: wrong surface now |
| client settings | `notificationMode`, `inAppNotificationsEnabled`, `notificationInboxEnabled`, `confirmQuit` | missing from `t3_logic::settings::ClientSettings` (shell handoff: lacks fe7d3092c fields) |

## 10. Reuse map

| Module | Verdict |
| --- | --- |
| `t3-app/src/toast.rs` | Mechanics reusable: global layer, `show`/`dismiss`, thread scoping, copy-error button, actions, persistent. Needs: glass card + `shadow-xl/25` (it uses `--popover` + `shadow-lg/5`, July-era), inline vs stacked layout rules (stacked only with a primary action), secondary/additional actions with their own variant, `leadingIcon`, untyped toasts (no icon), toast ids that replace in place, `update`, `onClose` only from the X, `dismissAfterVisibleMs`, timers that pause on hover/focus/window blur, limited (hidden) toasts beyond 3 that come back, hover expansion with 12px gaps, enter/exit slide from the right (500ms `cubic-bezier(.22,1,.36,1)`), swipe dismiss, expandable details, error 4-line clamp, draft-route scoping, anchored copy toasts. Its collapsed slab math approximates §1.4; recheck against the scale-from-top formula. |
| `t3-ui/src/components/toast.rs` | Card primitive: replace the surface (glass, radius 10, `shadow-xl/25`); the corner orb is right; add a leading-icon slot and the stacked layout. |
| `t3-app/src/dialogs.rs` | Replace: `confirm` must render the in-app AlertDialog (§3.2) with the title/description split, queueing, and Cancel/Confirm (+ destructive). The native prompt's Return/focus issue (shell handoff) goes away. |
| `t3-ui/src/components/dialog.rs` | Reusable chrome (popup, header, panel, footer, 200ms fade); needs `dialog-glass`, nested-dialog scaling, scale .98 enter, exit animation. |
| `t3-app/src/notifications.rs` | Reusable; switch to `keybindingsUpdated` events (fire on each event, not only on a diff) to match §2.1. |
| `t3-app/src/sidebar/` footer | Shell/sidebar owner mounts §5 notices in the order of §4. |
| gpui-kit `NativeMenu` | Replace for app menus: the fork draws in-app menus (§7) on desktop. Keep native only for text-editing menus. |

## 11. Reference screenshots needed

1440x900 @2x, dark and light, e2e fixture server.

1. One success toast (inline layout) and one stacked error toast with "Copy error" and an action; hover the copy button for its tooltip.
2. Three toasts collapsed (slabs peeking 12/24px), then the same stack expanded on hover.
3. A fourth toast added (oldest hidden), and mid-enter (250ms) / mid-exit frames.
4. "Some requests are slow" collapsed and with "Show requests" expanded (seed: stall the server, e.g. SIGSTOP the e2e server for 20s).
5. Clone toasts: running ("Receiving objects · 45% · …" with Cancel), done ("Open project"), failed (Retry + Remove project). Seed: clone a large public repo through the palette.
6. Provider update prompt: single provider (logo badge, Settings + Update) and multi-provider title; seed a fake provider with `versionAdvisory.status = behind_latest`.
7. Provider update pill: loading, failed (dismiss X + tooltip), success with countdown bar.
8. Confirm dialog: thread delete (destructive Confirm) and archive ("Archive thread "X"?").
9. SSH password prompt: counting down and expired.
10. Thread undo notice after archiving two threads ("Archived 2 threads, ⌘Z to undo").
11. Desktop update button states: idle, checking, available, downloading (ring at 40%), downloaded; nightly release notes popover.
12. Notifications inbox popover with unread and "All" tabs (macOS, inbox enabled).
13. Context menu on a sidebar thread with a submenu open and a destructive item highlighted.
14. Quit overlay (hold mode).
15. Keybindings invalid-config toast (write a broken `keybindings.json`).
16. First-run recovery "Still connecting" (block the primary server at launch).
17. Anchored "Copied!" over a message copy button.
18. Custom snooze dialog in both modes; ChatGPT plan connected dialog.

## 12. Open questions / risks

- GPUI has no backdrop blur: glass toasts, dialogs, popovers, and menus need the fork's opaque fallbacks (`--popover`, `--background/60`) or a design decision.
- The fork renders confirms and context menus in-app on desktop now (also recorded in `shell.md` §6.2-§6.3). The current `t3-app/src/dialogs.rs` native `Window::prompt` and the sidebar's `NativeMenu` are stale; the shell owner replaces them.
- Toast hover expansion depends on measuring every toast's height (Base UI measures with `height: auto`); in GPUI, measure after layout and animate with the same 150ms height / 500ms transform timings.
- Several surfaces need native backends that do not exist yet: the app updater (§5.4), notification inbox model (§5.5), SSH transport (§3.3), quit-hold key interception (§6, needs key-repeat and key-up observation in GPUI on macOS, where ⌘ suppresses letter key-up).
- Spinners (loading toasts, provider pill, update check, content search) animate continuously; pause them offscreen and while the window is hidden, as `visible-animate-spin` does.
- Thread-alert toasts and sounds are off by default; low priority unless the user enables them.
- Two upstream strings contain an em dash and a curly apostrophe ("Update timed out — try again.", "This environment isn’t connected — try again once it reconnects."); copy them exactly.
