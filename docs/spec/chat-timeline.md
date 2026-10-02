# Chat timeline build spec (`MessagesTimeline`)

> Refreshed against fe7d3092c (2026-10-02). Conventions are in [`chat.md`](chat.md) section 0;
> markdown inside rows is [`markdown.md`](markdown.md). Paths are relative to
> `~/L-Projects/t3UI-refs/t3code-fork/apps/web/src/` unless prefixed `crt:` (`packages/client-runtime/src/`),
> `shared:`, `contracts:`. Short names: `MT` = `components/chat/MessagesTimeline.tsx` (5,041 lines),
> `MTL` = `components/chat/MessagesTimeline.logic.ts`, `SL` = `session-logic.ts`,
> `PRES` = `crt:work-log/presentation.ts`.

## 1. Pipeline

```
thread detail (contracts:orchestration.ts)
  messages: OrchestrationMessage[]        :576  {id, role user|assistant|system|reasoning, phase?, text,
                                                  attachments?, context?, turnId, streaming, createdAt, updatedAt}
  activities: OrchestrationThreadActivity[] :664 {id, tone info|tool|approval|error, kind, summary, payload, turnId, sequence?, createdAt}
  proposedPlans: OrchestrationProposedPlan[] :593 {id, turnId, planMarkdown, implementedAt, implementationThreadId, createdAt, updatedAt}
  checkpoints: OrchestrationCheckpointSummary[] :645 {turnId, checkpointTurnCount, status, files[{path,kind,additions,deletions}], assistantMessageId, completedAt}
  latestTurn: OrchestrationLatestTurn       :684 {turnId, state running|interrupted|completed|error, requestedAt, startedAt, completedAt, assistantMessageId}
  session: OrchestrationSession             :622 {status, activeTurnId, lastError, ...}
        │
        │ ChatView.tsx:2958  deriveWorkLogEntries(activities)                → WorkLogEntry[]   (section 2)
        │ ChatView.tsx:3517  timelineMessages = server messages (+ preview URL handoff)
        │                                     + optimistic user messages not yet echoed
        │ ChatView.tsx:3566  deriveTimelineEntriesWithState(messages, plans, work, previous)
        ▼                                                                    → TimelineEntry[]  (section 3)
  held-timeline selection on thread switch (chat.md 5)
        │ MT:780-818  deriveMessagesTimelineRowsWithState(input, previousProjection) → MessagesTimelineRow[] (section 4)
        │ MT:4128-4148 useStableRows → computeStableMessagesTimelineRows (MTL:1641-1760)
        ▼
  LegendList rows (section 6) → TimelineRowContent per row (section 7-8)
```

- `system` role messages are not rendered (no row component handles them; `MT:1749-1764`).
- `reasoning` messages arrive only when the thread subscription asks for `reasoningMessages`
  (otherwise the server rewrites them to `system`). `t3_client` already asks
  (`t3-client/src/environment.rs:1012-1065`).
- Message `phase` (`final_answer` / `commentary`) is in the fork's contracts only; upstream
  b33eda13 never sends it. Every rule below that mentions `phase` sees `undefined`.

### 1.1 Incremental reuse (performance invariants)

- `deriveTimelineEntriesWithState` (`SL:1654-1715`): if plans and work entries are unchanged and
  only streaming messages changed text/updatedAt (`isStreamingMessageTextUpdate`, `SL:1620-1632`:
  same role assistant|reasoning, both streaming, all other fields shallow-equal), reuse the previous
  ordered entries and swap message objects. If arrays only grew by appending, merge the sorted
  suffix. Otherwise full stable sort by `createdAt` (ties: message < plan < work, then source order).
- `deriveMessagesTimelineRowsWithState` (`MTL:1562-1639`): same idea for rows; only streaming text
  updates reuse the previous rows.
- `computeStableMessagesTimelineRows` (`MTL:1641-1760`): reuses the previous row object per id when
  the per-kind shallow comparison says unchanged, so unchanged rows never re-render.
- Work-log entries are memoized per activity object (`SL:108-111`).

## 2. Work log entries (`deriveWorkLogEntries`)

`SL:451-515,542-923`, `crt:work-log/userInput.ts:77-157`.

### 2.1 Order and filters

1. Sort activities by `sequence` (activities with a sequence come after those without), then
   `createdAt`, then lifecycle rank (`*.started` 0, `*.progress`/`*.updated` 1,
   `*.completed`/`*.resolved` 2, other 1), then id (`SL:1391-1430`).
2. `foldUserInputActivities`: all `user-input.requested|resolved|answer-submitted` activities with
   the same `payload.requestId` collapse into **one** activity at the first one's position, kind
   `user-input.answer-submitted`, tone `tool`, summary "User input submitted" (answers or
   attachments present) / "User input dismissed" (a resolved event, no answers) / "User input
   requested", payload `{requestId, questionTextById, answers (option values mapped to labels),
   attachmentsByQuestionId}`. Tool activities that asked the same questions in the same turn are
   dropped unless failed/declined/stopped/cancelled.
3. Dropped kinds: worktree setup activities (`setup-script.requested`, `setup-script.started`,
   `worktree-setup`) except error-tone setup-script ones; `tool.started`; non-agent `task.started`;
   `task.updated`; `tool.progress`; `context-window.updated`; **`turn.plan.updated`** (the plan
   checklist is a composer badge, not a row); summary "Checkpoint captured"; `runtime.warning`
   whose summary ends with "(no displayable text content)"; tool rows whose `payload.detail`
   starts with "ExitPlanMode:"; agent-internal activity (`payload.agentId` or
   `payload.timelineBypass`, except an agent's own task rows); in-progress Claude `Agent`/`Task`
   launch tool rows; tool rows whose `toolCallId` matches an agent task's `toolUseId` (unless
   failed).

### 2.2 Entry fields (`toDerivedWorkLogEntry`, `SL:542-680`)

| Field | Source |
|---|---|
| `id`, `createdAt`, `turnId` | activity |
| `label` | task rows: `payload.summary` or `payload.detail`; else `activity.summary` |
| `tone` | `task.progress` → `thinking`; tone `approval` → `info`; else activity tone |
| `sourceActivityKind` | activity kind |
| `detail` | task rows: `payload.detail` minus a trailing `<exited with exit code N>`; others: command output for command tools, else `payload.detail` unless it equals the heading or echoes the command, else a raw-output summary ("{n} file(s)[+]", first line ≤84 chars, or "{n} lines"); `runtime.error/warning`: `payload.message` when it differs from the summary |
| `command`, `rawCommand` | first of `data.item.command`, `item.input.command`, `item.result.command`, `data.command`, (command_execution) `detail`; arrays joined with quoting; shell wrappers `bash/sh/zsh -c|-lc`, `cmd /c`, `pwsh -command` unwrapped; `rawCommand` only when different |
| `changedFiles` | up to 12 unique `path/filePath/relativePath/filename/newPath/oldPath` found recursively (depth ≤4) under `data` (`item/result/input/data/changes/files/edits/patch/patches/operations`) |
| `toolTitle` | `payload.title` |
| `toolSurface` (`browser`/`computer`), `toolIcon` (`website`/`native-app`/`themed-logo`), `toolSource` `{key,name,kind browser|computer|integration,icon?}` | `crt:work-log/toolPresentation.ts:271-286` |
| `toolData` | MCP calls: `data.item ?? data` |
| `itemType` | `payload.itemType` when a known tool lifecycle item type (`command_execution`, `file_change`, `mcp_tool_call`, `dynamic_tool_call`, `collab_agent_tool_call`, `web_search`, `image_view`, ...) |
| `requestKind` | `payload.requestKind` (`command`/`file-read`/`file-change`/`permission`) or derived from `requestType` |
| `toolCallId` | `payload.toolCallId ?? data.toolCallId` (not for task rows) |
| `toolLifecycleStatus` | `payload.status`: `pending|running|waiting` → `inProgress`; `cancelled|interrupted` → `stopped`; `idle` → `stopped` only for `subagent_batch`; `inProgress|completed|failed|declined|stopped` as-is; `tool.completed` without status → `completed` (`PRES:371-395`) |
| `viewedImagePath` | `data.imagePath` |
| `questionAnswer` | decoded payload of `user-input.answer-submitted` |
| `taskId`, `agentRole` | task rows' `payload.taskId`, `payload.role` |

### 2.3 Collapse (`collapseDerivedWorkLogEntries`, `SL:718-845`)

- **Agent spawn batches**: non-background task rows group by spawn key, decided at the first row
  seen per taskId: workflow member (`taskId` contains `:wf:`) or coordinator → `wf:{id}`; else
  `direct:{turnId}` (or `direct:task:{taskId}` without a turn). One row per group keeps the first
  row's id/createdAt/turnId/label and gains `agentSpawn {workflowId, agentTaskIds[]}`.
- **Tool lifecycle**: consecutive (or same `tool:{turnId}:{toolCallId}` key) `tool.updated` /
  `tool.completed` entries for the same call merge into one entry (later non-empty fields win,
  changed files union). A `tool.completed` ends the merge chain.

## 3. Timeline entries

`TimelineEntry = message | proposed-plan | work` (`SL:134-152`), ids = message id / plan id /
work entry id. Async answers: a **user** message with id `async-answer:{requestId}` is hidden when a
folded question-answer work entry exists for that request (`SL:1670-1676`).

## 4. Row derivation (`deriveMessagesTimelineRows`, `MTL:1014-1551`)

### 4.1 Row kinds (`MTL:350-463`)

| Kind | id | Fields |
|---|---|---|
| `message` | message id | `message`, `durationStart`, `showAssistantMeta`, `showAssistantCopyButton`, `assistantCopyStreaming`, `assistantTurnDiffSummary?`, `revertTurnCount?` |
| `assistant-meta` | `assistant-meta:{messageId}` | `message`, `showAssistantCopyButton`, `assistantCopyStreaming` |
| `turn-fold` | `turn-fold:{turnId}` | `turnId`, `label`, `expanded` |
| `activity-group` | `activity-group:{firstEntryId}` (or work group id), `live-activity-row` when active | `turnId`, `groupId`, `entries` (reasoning messages + work), `expanded`, `active` |
| `work` | entry id, or `{groupId}:details` for an expanded group | `groupedEntries`, `isExpandedToolGroup`, `displayLabel?` |
| `work-live` | `live-activity-row` (tail) or `work-live:{identity}` | `entry`, `groupedEntries`, `groupId`, `expanded`, `active` |
| `work-toggle` | `work-toggle:{firstEntryId}` | `turnId`, `groupId`, `hiddenCount`, `expanded`, `summary`, `summaryKind`, `toolSurface?`, `toolIcon?`, `summaryToolIcon?`, `hasFailure` |
| `context-compaction` | entry id | `label` |
| `proposed-plan` | plan id | `proposedPlan` |
| `working` | `working-indicator-row` | `createdAt` (timer start, nullable) |
| `thinking` | `live-activity-row` | `createdAt` |
| `worktree-setup` | `worktree-setup-row` | `snapshot`, `embedded` |
| `queued-message` | `queued-message:{id}` | `queuedMessage`, `isNext` |

Work group ids: `work-group:tool:{turnId|no-turn}:{toolCallId}` when the first entry has a
`toolCallId`, else `work-group:{entryId}`. `live-activity-row` is shared by the thinking row, an
active activity group and an active tail `work-live` row, so the live row keeps one list slot
across handoffs.

### 4.2 Turn state

- `unsettledTurnId` (`MTL:572-584`): the session's running turn if any; else `latestTurn.turnId`
  unless it has `completedAt` and state ≠ `running`.
- Active visual response turns (`MTL:610-633`): `unsettledTurnId`, plus (while working) every turn
  id seen after the last user message (a provider restart without a prompt stays one response).
- `showWorkProgress` = working and no `final_answer` message in the unsettled turn.
- Terminal assistant message per response: the last assistant message per `turnId` (or per
  user-message boundary for turnless messages) (`MTL:530-554`).

### 4.3 "Worked for" folds (`deriveTurnFolds`, `MTL:648-877`)

Group timeline entries per turn (a user message between parts of the same turn splits it: the part
before the steer stays visible). A group folds when:
not steered-away; not (followed by a user message, terminal message phase `commentary`, and not the
completed latest turn); not an active visual response turn (unless its final answer started); no
streaming non-reasoning message (unless final answer started).

Hidden entries: everything up to the terminal assistant message, plus reasoning and compaction rows
anywhere in the group, plus a **single** trailing non-failed work entry after the terminal message
(reasoning does not count toward "single"). Never hidden: the terminal message itself, question
answers, agent spawn rows. A group whose hidden entries are only reasoning/compaction does not fold.

Label: interrupted latest turn → `You stopped after {d}` / "You stopped this response"; else
`Worked for {d}` / "Worked". `{d}` = `formatDuration` of: (final answer started) turn start (or
the user message that started it, or first entry) → terminal message createdAt; else latest turn
`startedAt → completedAt` when known; else start boundary → later of terminal message `updatedAt`
and last entry end. The fold row is inserted before the first hidden entry; `expanded` from
`expandedTurnIds` (local state). On a new turn, the previous turn's expansion is cleared; a running
turn that becomes `interrupted` is auto-expanded (`MT:711-735`).

`formatDuration` (`shared:orchestrationTiming.ts:14-31`): `<1s` → `{n}ms` (min 1); `<10s` → one
decimal `4.2s` ("10s" at the boundary); `<60s` → `{n}s`; else `1h 2m 3s` with zero parts dropped.

### 4.4 Main pass, per entry in order

1. If working and this is the first entry after the last user message → **working row**.
2. If this entry is where the live tool tail belongs → the active `work-live` row (+ its expanded
   `work` details row unless it is a spawn).
3. A fold anchored here → **turn-fold** row. Collapsed hidden entries are skipped.
4. A run of activity entries (reasoning messages, and work entries that are not agent spawn /
   question / compaction / error) of the same turn that contains **at least one reasoning message**
   → one **activity-group** row; `active` when working, it is the unsettled turn, the run reaches
   the end of the timeline, and the latest tool did not fail.
5. Entries already shown in the live tail are skipped.
6. Compaction work entry → **context-compaction** row (label = entry label).
7. Agent spawn / question answer / error-tone work entry → standalone **work** row.
8. Other work entries: take the run of consecutive plain work entries, drop invisible ones
   (neutral-status tool rows unless in progress in the active run, `MTL:100-110`) and superseded
   lifecycle markers (`PRES:637-673`). Then:
   - any entry in progress in the active run → **work-live** row (+ details row when expanded);
   - exactly one tool-like entry → standalone **work** row with `displayLabel` = (edit)
     `summarizeToolGroup` else `singleToolCallLabel` (presentation name, command, or capitalized
     title) (`MTL:47-54`);
   - else → **work-toggle** row (+ `{groupId}:details` work row when expanded).
9. Proposed plan → **proposed-plan** row.
10. Message → **message** row: `showAssistantMeta` (= copy button) only for the terminal assistant
    message of a response that is no longer active; `assistantCopyStreaming` = streaming or still
    active; `assistantTurnDiffSummary` = the checkpoint whose `assistantMessageId` is this message;
    `revertTurnCount` for user messages (4.6).

Live tool tail (`MTL:1082-1158`): walking back from the end while working, the trailing run of
plain work entries of the unsettled turn (stopping at a failure) becomes one `work-live` row placed
at its latest visible entry; `active` while its latest entry is running or succeeded; dropped when
the latest visible tool failed (the failure then shows as a normal row and the thinking row
returns).

### 4.5 Tail rows

- Worktree setup (`MTL:1474-1522`): shown while a setup snapshot exists, unless the agent's turn has
  started and the setup finished cleanly. Inserted right after the first user message (after the
  working row when one is there). While the setup runs, a working row is inserted above it.
  `embedded` once the agent's turn is live.
- Working row at the end when working and no user message precedes the active turn.
- **Thinking** row when work progress shows, no setup is running, and there is no active activity
  row (or the latest tool failed).
- `attachTrailingToolGroupsToAssistant` (`MTL:884-973`): when a terminal assistant message with meta
  is followed (same turn, only reasoning in between) by tool groups, its meta moves to an
  `assistant-meta` row after the last of those groups.
- Queued messages (`MTL:1541-1549`): one `queued-message` row each, after everything; the first is
  `isNext`.

### 4.6 Revert eligibility (`MTL:976-1012`)

Only when the provider supports rollback. For each user message, the first following assistant
message (before the next user message) that has a checkpoint summary gives
`revertTurnCount = checkpointTurnCount (or inferred index) − 1`. No checkpoint → no button.

## 5. Presentation helpers

- **Tool-like** (`PRES:363-368`): tone `tool|thinking|error`, or a command, or a request kind, or a
  tool lifecycle item type.
- **Failure** (`PRES:398-445`): tone `error`, status `failed`/`declined`, or output text
  containing "file not found", "no files found", "enoent", "no such file", "command not found",
  "commandnotfoundexception", "cannot find path … does not exist", "is not recognized …", "a
  parameter cannot be found…", or a non-zero exit code pattern. Display failure checks `detail`
  only; failure also checks the command.
- **Success marker** allowed: tool-like, not failed, not thinking, not in progress, not stopped.
- **Severe failure** (`SL:165-170`): `runtime.error` or any `*.failed` kind.
- **Action** (`toolGroupAction`, `PRES:464-498`): approvals → `update`; T3 MCP PR tools →
  `link-pr|unlink-pr|list-prs`; T3 preview tools → `browser`; device tools → `device`; file-read /
  image view / viewed image / dynamic "read file" → `read`; file change / changed files → `edit`;
  command → `command`; `web_search` with "grep" in the title → `code-search`; `web_search` →
  `search`; tool-like → `other`; else `update`.
- **Group summary** (`summarizeToolGroup`, `PRES:567-635`):

| Action | Text |
|---|---|
| link-pr / unlink-pr | `Linked {n} pull request(s)` / `Unlinked {n} pull request(s)` |
| list-prs | "Checked linked pull requests" / `Checked linked pull requests {n} times` |
| read | `Read {n} file(s)` |
| edit | `Changed {n} file(s)` (unique files, plus edits without file details) |
| command | `Ran {n} command(s)` |
| device | `Used device controls {n} time(s)` |
| browser | `Used browser {n} time(s)` |
| search | `Searched the web {n} time(s)` |
| code-search | `Searched code {n} time(s)` |
| other | `Used {n} tool(s)` |
| update | `Received {n} update(s)` |

  Entries with a `toolSource` (non-PR) are listed first as `Used {A}`, `Used {A and B}`, `Used {A,
  B, and C}`, with ` integration` / ` integrations` appended when all sources are integrations.
  Phrases join with " and " (two) or ", " + ", and " (three+); every phrase after the first is
  lowercased.
- **Summary kind** (icon choice): all PR presentations → `pull-request`; one action → that action
  (`other` refines to `dynamic-tool` / `agent-tool` / `tone-tool` / `other` by item type / task /
  tone); else `mixed`.
- **T3 MCP tools** (`PRES:74-187`): tool names (prefix `mcp__t3-code__`, `t3-code.`, `t3code:` …
  stripped) map to `{verb} {detail}`; verb = running form (in progress / unknown), completed form,
  `Failed to {action}`, `Declined to {action}`, `Stopped {running}`. PR link/unlink with a number
  → detail `PR #{n}`. Icons: PR tools `pull-request`, `preview_*` `browser`, `device_*` `device`,
  else `t3-code`. Table (name: action / running / completed / detail):
  link_pull_request Link/Linking/Linked/a pull request; unlink_pull_request
  Unlink/Unlinking/Unlinked/a pull request; list_thread_pull_requests Check/Checking/Checked/linked
  pull requests; orchestrator_capabilities Get/Getting/Got/orchestration capabilities;
  delegate_task Delegate/Delegating/Delegated/a child task; task_status Get/Getting/Got/delegated
  task status; task_cancel Cancel/Canceling/Canceled/delegated task; schedule_task
  Schedule/Scheduling/Scheduled/a recurring task; list_scheduled_tasks List/Listing/Listed/scheduled
  tasks; update_scheduled_task Update/Updating/Updated/a scheduled task; delete_scheduled_task
  Delete/Deleting/Deleted/a scheduled task; create_threads Create/Creating/Created/T3 threads;
  t3_thread_start Start/Starting/Started/a T3 thread; t3_thread_list List/Listing/Listed/T3 threads;
  t3_thread_read Read/Reading/Read/a T3 thread; t3_thread_send Send/Sending/Sent/to a T3 thread;
  t3_thread_wait Wait/Waiting/Waited/for a T3 thread; t3_thread_interrupt
  Interrupt/Interrupting/Interrupted/a T3 thread; t3_worktree_handoff Hand off/Handing off/Handed
  off/thread to a git worktree; t3_worktree_status Get/Getting/Got/thread worktree status;
  preview_status Get/Getting/Got/preview browser status; preview_open Open/Opening/Opened/a page in
  the preview browser; preview_navigate Navigate/Navigating/Navigated/the preview browser;
  preview_snapshot Take a snapshot of/Taking a snapshot of/Took a snapshot of/the preview page;
  preview_click Click/Clicking/Clicked/in the preview browser; preview_press
  Press/Pressing/Pressed/a key in the preview browser; preview_type Type/Typing/Typed/in the preview
  browser; preview_scroll Scroll/Scrolling/Scrolled/the preview browser; preview_resize
  Resize/Resizing/Resized/the preview browser; preview_evaluate Evaluate/Evaluating/Evaluated/script
  in the preview browser; preview_wait_for Wait/Waiting/Waited/for the preview page;
  preview_set_appearance Set/Setting/Set/preview browser appearance; preview_recording_start
  Start/Starting/Started/recording the preview browser; preview_recording_stop
  Stop/Stopping/Stopped/recording the preview browser; device_list List/Listing/Listed/simulators
  and emulators; device_open Open/Opening/Opened/a device in the Device panel; device_screenshot
  Take a screenshot of/Taking a screenshot of/Took a screenshot of/the device; device_close
  Close/Closing/Closed/a device.
- **Display label** (`workEntryDisplayLabel`, `MTL:56-70`): presentation name > command > detail >
  first changed file (workspace-relative, `+{n} more`) > capitalized `toolTitle || label` with a
  trailing "complete"/"completed" removed.
- **Live label** (`liveWorkEntryLabel`, `MTL:72-98`): presentation name (present tense unless
  failed/declined/stopped) > commands `{Running|Ran|Failed|Declined|Stopped} {program}` where
  `{program}` = `commandProgramName(command) ?? "command"` > display label.
- **`commandProgramName`** (`crt:work-log/commandLabel.ts:1367`, tests in `commandLabel.test.ts`):
  unwraps shell launchers (`/bin/zsh -lc '…'`, `bash --noprofile -l -c`, `sh -ec`, `fish
  --command`, `env`/`sudo` prefixes, nested shells, quoted Windows paths), skips leading `cd …`,
  assignments, `export`, `source`/`.`, comments and redirections, returns the first real program's
  basename (`node.exe` kept), and `null` for shell syntax/builtins (`if`, `for`, `[`, `cd` alone,
  `eval`, `true`…) or malformed scripts. Port the test table as the spec.
- **Timestamps** (`timestampFormat.ts:134-189`): row text `formatDayAwareTimestamp`: today
  `12:34 PM`, yesterday `yesterday at 12:34 PM`, older `8/13 12:34 PM` (year added when it
  differs; locale order). Tooltip `formatChatTimestampTooltip`: `{time}, {D}{st|nd|rd|th} {Month}
  {YYYY}`. Time format from setting `timestampFormat` (`locale` default / `12-hour` / `24-hour`);
  desktop passes the system locale (`desktopBridge.getSystemLocale`).

## 6. List mechanics

`MT:477-1386`. Library: `@legendapp/list` 3.3.5 (patched, `pnpm-workspace.yaml:39,244`).

### 6.1 Container

```
div.relative.h-full.min-h-0 [data-assistant-citation-viewport]
├─ [AssistantSelectionToolbar]                                         8.21
├─ LegendList  h-full, overflow-x hidden, overscroll-y contain, overflow-anchor none,
│     padding-inline 20px (12 below 640), scrollbar-gutter: stable both-edges
│     (6px custom scrollbar, so content is inset 26px each side), topbar-scroll-fade when no banner
│     contentContainer padding-bottom = composer inset + 16 (chat.md 6.3)
│  ├─ ListHeader:  load-earlier header | 24px fade spacer | 16px spacer (12 below 640)
│  └─ each row:  div.mx-auto.w-full.min-w-0.max-w-(--chat-max-width).overflow-x-clip[data-timeline-root]
│                  └─ TimelineRowContent (section 7)
└─ TimelineMinimap                                                     6.5
```

- **Top fade** (`index.css:408-445`): when no banner is shown the list masks its top 25px with a
  stepped gradient (0 → 10% → 30% → 58% → 82% → 96% → 100% opacity at 0/10/24/42/62/82/100%),
  leaving the 6px scrollbar lane unmasked; the header spacer is then exactly the fade height 24px
  (`--workspace-titlebar-scroll-fade-height`). Rows dissolve as they scroll under the header.
- **Load earlier** (`MT:342-364`): when the thread snapshot reports older turns
  (`ChatView.tsx:1588-1600`), the header is `pt-6` (fade) or `pt-4`, then a
  `max-w-(--chat-max-width) pb-2` box with a full-width button `py-1.5 text-xs
  text-muted-foreground/60 hover:text-foreground`: "Load earlier turns" / "Loading earlier turns…"
  (disabled). No spinner. Manual only; reaching the top does not auto-load.
- LegendList config (`MT:1305-1355`): `estimatedItemSize 90`, `keyExtractor = row.id`,
  `getItemType = message:{role}` or kind, `initialScrollAtEnd` unless restoring a not-at-end
  position, `maintainScrollAtEndThreshold 1`, `maintainScrollAtEnd` (off while restoring, citing,
  anchoring, not following, or settling a disclosure) with `on: {dataChange, itemLayout, layout}`
  and **not** `footerLayout`; `animated` while working on streamed assistant output
  (`glideToEnd`, `MT:983-989`), instant otherwise. `maintainVisibleContentPosition {data, size}`
  (restore only the disclosure's own row while a disclosure settles), off while the right panel
  animates its width.

### 6.2 Empty and loading

- No rows and not working: if `hideEmptyPlaceholder` (hero draft or loading) → a plain
  `bg-background` block; else centered `<p class="text-placeholder text-sm">` "Send a message to
  start the conversation." (`MT:1281-1292`).

### 6.3 Disclosure anchoring

Toggling a turn fold, work group, work entry, reasoning block or spawn row suspends end pinning for
two frames and keeps the toggled row's top fixed (`MT:617-655`). Collapsing tool output that leaves
the list at its end restores a resting composer.

### 6.4 Per-thread position restore

`MT:533-940`, `timelineScrollAnchoring.ts:110-141`. The list stays mounted across thread switches.
On every scroll it records `{rowId, offsetWithinRow, scrollOffset, atEnd, disclosures}` per thread
(100 threads). Opening a thread restores the expanded folds/groups/reasoning/spawn state and, if not
at end, scrolls to the saved row with its offset, reconciling for 2 stable frames, cancelled by any
wheel, touch, pointerdown or scroll key. The first end pins after a switch are instant.

### 6.5 Minimap

`MT:1413-1693`, `MTL:42-304`, `components/chat/timelineMinimapItems.ts`. Not setting-gated.

- Items: one per user message row (`TIMELINE_MINIMAP_MIN_ITEMS = 2`: hidden with fewer than 2),
  with the user text and the last assistant text of that turn.
- Hidden on coarse pointers (`[@media(pointer:fine)]:block`).
- Box: `absolute inset-y-0 left-0 z-40 w-18` (72px). Opacity 100% when the side gutter (between the
  viewport edge and the centered content column) is ≥48px; else 0, fading to 100% on hover or
  focus-within over 150ms.
- Rail: `absolute top-1/2 left-3 -translate-y-1/2` (12px from the left, vertically centered),
  height `min({(n−1)×8}px, 100vh − 18rem)`, interactive width = gutter − 12px capped at 40px (0
  disables pointer interaction); 22rem (352px) while a preview is open. A 1px line `bg-border/15`
  at x 12px.
- Strips: per item `absolute left-0 h-0.5 (2px) rounded-full`, top at `i/(n−1)×100%`; width by
  distance from the hovered item: 0 → 24px `muted-foreground/75`, 1 → 16px, 2 → 10px, else 8px;
  base `muted-foreground/35`; items whose row is in view `foreground/90` (set imperatively on
  scroll). `transition: width, background-color 150ms`.
- Hover preview (`left 32px, w-80` 320px): `dropdown-glass rounded-xl p-3 shadow-xl
  shadow-black/25`; first line user text (`text-sm font-medium leading-5`, one line ellipsis,
  "User message" fallback), then assistant text (`mt-1 text-sm leading-5 text-muted-foreground`,
  3-line clamp). Vertically aligned to the hovered strip (top item aligned top, last aligned
  bottom, others centered). Text is selectable.
- Interaction: pointer Y maps to the nearest item; click jumps (`scrollToIndex`, animated,
  `viewOffset 24`) and counts as manual navigation. Keyboard on the rail button: ArrowUp/Down move,
  Home/End, Enter/Space jump. `aria-label` `Jump to message: {userText}`. Previous/next buttons
  (`Button icon-micro ghost-muted`, 20px, `ChevronUpIcon`/`ChevronDownIcon` 16px `foreground/90`)
  sit 2px above / below the rail at x 4px, opacity 0 until hovered/focused, tooltips "Previous
  turn" (top) / "Next turn" (bottom), disabled at the ends, pointer-inert when the gutter is under
  14px.

## 7. Row frame and spacing

`MT:1698-1767`. Every row is a `div` with `data-timeline-row-id`, `data-timeline-row-kind`,
`data-message-id`, `data-message-role`, and bottom padding:

| Row | padding-bottom |
|---|---|
| expanded tool group `work` details row | 4px |
| expanded `work-toggle` / `work-live` header | 0 |
| `turn-fold`, `working` | 6px |
| assistant message without meta, reasoning message, `work`, `work-live`, `work-toggle`, `activity-group`, `thinking`, `worktree-setup` | 8px |
| everything else (user message, assistant with meta, `assistant-meta`, `proposed-plan`, `context-compaction`, `queued-message`) | 16px |

Assistant message and `assistant-meta` rows carry the `group/assistant` hover group. Freshly synced
rows get `timeline-sync-entry` (600ms fade, chat.md 5).

## 8. Rows

### 8.1 User message

`MT:1972-2303`.

```
div.group.flex.flex-col.items-end.gap-1 (4)
├─ bubble  relative max-w-[80%] rounded-2xl (18) bg-message p-3 (12) text-message-foreground
│  ├─ h3.sr-only "You"
│  ├─ [media grid]  mb-2 (8) grid max-w-[210px] grid-cols-2 gap-2 (8)
│  │    image tile: aspect 4/3, overflow-hidden, rounded-lg (10), border border/80, bg-background/70
│  │      button cursor-zoom-in aria-label "Preview {name}" → img object-cover
│  │      no preview URL: min-h 72px centered px-2 py-3 text-2xs text-secondary-label "{name}"
│  │    snap-shot image: spans 2 columns, SNAP_SHOT frame + details (SnapShotAttachmentDetails)
│  │    video tile: MediaVideoPlayer aspect 4/3 rounded-lg border border/80
│  │      unavailable: black box text-2xs white/70 "{name}"
│  ├─ [file list]  mb-2 flex flex-col gap-1 (4)   (files not already shown as chips in the text)
│  │    downloadable: row flex gap-1 →
│  │      button flex-1 gap-2 rounded-md py-1 text-sm hover:underline aria "Preview {name}"
│  │        PierreEntryIcon + name (truncate) + EyeIcon 16
│  │      Button icon-xs ghost-muted DownloadIcon, aria + tooltip (top) "Download {name}"
│  │    not downloadable / unknown type: row gap-2 py-1 text-sm icon + name
│  └─ body (CollapsibleUserMessageBody, 8.2)
└─ footer  flex w-full max-w-[80%] justify-end pe-1 (4) text-xs tabular-nums
     opacity 0 → 100 on row hover / focus-within (transition-opacity 200ms; always 100 on touch)
   └─ flex items-center gap-2 (8)
      ├─ timestamp p text-xs text-muted-foreground, tooltip = long form
      └─ flex gap-0.5 (2)
         ├─ [Edit from here]  Button size=xs ghost, Undo2Icon 12px, aria + tooltip (top) "Edit from here"
         │     disabled while reverting or working; opens the revert dialog (chat.md 9.1)
         └─ [copy]  MessageCopyButton size=xs variant=ghost (→ ghost-muted), aria + tooltip "Copy message"
```

- Images come from `message.attachments` with `type: image` (blob/data preview URLs kept; others
  resolved through `assets.createUrl` for `{_tag: "attachment", attachmentId}`), excluding
  `preview-annotation-*.png` names (`MT:1975-2023`). Click → media viewer gallery of the message's
  images.
- Copy writes the message text with context references rewritten to their labels, or (when the
  message has context records) the canonical text plus a structured clipboard flavor
  (`MT:2276-2290`). Copy feedback: icon swaps to `CheckIcon` 12px `text-primary`, the button is
  disabled for 1000ms, and an anchored tooltip-styled toast "Copied!" (or "Failed to copy" + error)
  appears at the button for 1000ms (`components/ui/anchoredCopyToast.ts:4-33`).

### 8.2 User message body and chips

`MT:3491-4056`.

- **Collapse** when text > 600 chars or > 8 lines: body box `max-h-44` (176px) `overflow-hidden`
  with a mask `linear-gradient(to bottom, black calc(100% − 1.75rem), transparent)` (28px fade).
  Footer `mt-1.5` (6px) row with `Button size=xs variant=ghost-muted -ml-1` "Show full message" /
  "Show less" (`aria-expanded`). No animation.
- Body: `ChatMarkdown` user variant (`markdown.md` 1).
- **Context references** (`t3-context:` links in the text, resolved against `message.context`
  records) render as chips (`ContextChip`, `components/ContextChip.tsx:24-118`): inline-flex,
  height 1.41em, gap 0.33em, radius 0.5em, border 1px, padding-x 0.5em, font-medium, font-size
  0.86em, line-height 1, svg 1.17em, `align-middle`. Each kind has an accent hue
  (`oklch(0.62 C H)`: image 0.16/16, video 0.16/48, file 0.136/237, mention 0.11/215, terminal
  0.134/163, element & preview-annotation 0.134/70, review-comment 0.16/292, pull-request
  0.16/277, pr-open 0.134/163, pr-draft 0.02/259, pr-merged 0.16/292, pr-closed 0.16/16, skill
  0.16/322, citation 0.16/259); border = accent 34% over `contrast-border` (48% on hover for
  buttons/links), background = accent 11% (17% hover), text = accent 22% over
  `contrast-foreground`. `neutral`: `border/70`, `bg-code`, `foreground`. Unresolved: dashed
  border, `foreground` text. Invalid: `destructive/35` border, `destructive/8` bg, `destructive`
  text. Focus-visible outline 2px `foreground` offset 2px.

| Record kind | Chip | Interaction |
|---|---|---|
| mention (file path) | `mention`, Pierre icon + label | click → right panel file viewer; tooltip = path |
| skill | `skill`, skill icon + label | tooltip `${name}`; aria `Skill, {label}` |
| image | `image` button: thumbnail 1.17em (accent = average image color) + middle-truncated name (max 288px) + size `text-3xs` | click → media viewer; aria `Image attachment, {name}, {size}` |
| file / video | `file` / `video`: Pierre icon or `FilmIcon` + name + size | click → file preview / video viewer; tooltip `{name}\n{size}`; disabled when not downloadable |
| terminal | `TerminalContextInlineChip` (composer spec) | |
| element | popover chip `MousePointerClickIcon` → details card (page title, url, selector, source `file:line`, HTML + styles previews 160/128px tall) | aria `Browser element, {label}. Show details` |
| review-comment | popover chip `MessageCircleIcon` → card: file path (workspace-relative, `text-xs font-medium`), `{section} · {range}` (`text-2xs secondary-label`), comment text, then the fenced code (non-diff) or a unified `@pierre/diffs` FileDiff | PR summaries use the PR chip (state icon, tooltip/preview with PR details) |
| preview-annotation | popover chip → card: screenshot (max 256px tall, click to expand) or "Screenshot unavailable", page title/url ("Preview annotation" fallback), comment, target summary, style-change count, selected elements | |
| missing record | unresolved chip `CircleDashedIcon` + label, tooltip "This context is no longer available." | |

  Popover chips open a `PopoverPopup side=top width=lg padding=compact` (384px wide). Copying a
  selection that includes chips copies their link text (`data-markdown-copy`) plus a structured
  context fragment.

### 8.3 Queued message

`MT:1796-1896`. Appears after all rows when messages were sent during a running turn
(`followUpBehavior` `"queue"`, default).

```
div.flex.flex-col.items-end
└─ max-w-[80%] rounded-2xl (18) border border-dashed border-border p-3 (12) text-message-foreground/80
   ├─ [text] UserMessageBody
   ├─ [counts] text-xs text-secondary-label (mt-1.5 after text): "{n} attachment(s)", "{n} context item(s)"
   └─ footer mt-2 (8) flex items-center gap-4 (16) text-xs text-secondary-label
      ├─ status  inline-flex h-6 gap-1 ClockIcon 14px + "Queued" | "Sending"
      │    tooltip (bottom): "Sending to the agent" | "Waits for Send now" | "Sends when the turn ends" (next)
      │                      | "Sends after the messages above it"
      └─ ml-auto flex gap-0.5 (invisible while sending)
         ├─ Button icon-xs ghost-muted ArrowUpIcon 14px aria "Send now",
         │    tooltip (bottom) "Send now" + " ({shortcut})" on the next message (mod+shift+Enter)
         └─ Button icon-xs ghost-muted XIcon 14px, aria + tooltip "Cancel and return to the composer"
```

### 8.4 Context compaction

`MT:1898-1917`: `role=separator` `aria-label={label}`, `mx-auto flex w-full max-w-(--chat-max-width)
items-center gap-3 py-1 text-xs text-muted-foreground`; 1px lines `bg-border/70` on both sides;
center `Minimize2Icon` 12px + label = activity summary: `Compacted context {before} → {after} tokens` or "Context compacted" (upstream `apps/server/src/orchestration/Layers/ProviderRuntimeIngestion.ts:883-892`).

### 8.5 Worktree setup card

`components/chat/WorktreeSetupCard.tsx:335-435`; data = `worktree-setup` activity payload
`WorktreeSetupSnapshot` (`contracts:worktreeSetup.ts`, also upstream) or the live store
(`crt:worktreeSetup.ts:17-60`).

- Header row only when settled and not embedded: same metrics as the working row (8.10): text
  "Worktree ready" (`muted-foreground`), "Worktree ready, setup script failed"
  (`warning-foreground`), "Worktree setup failed" (`destructive-foreground`), "Worktree setup
  cancelled"; total duration right-aligned `text-xs muted-foreground`. While running, the working
  row above reads "Setting up worktree…" with the shimmer.
- Stage rows (`pt-1.5` under a header): `relative flex min-h-6 items-center gap-1.5 rounded-md
  px-0.5 py-0.5 text-sm leading-relaxed`; 24px icon box `text-icon-muted` with `CheckIcon` (done),
  spinner (running), `XIcon` (failed), `CircleAlertIcon` (warning), `MinusIcon` (skipped),
  `CircleIcon` (pending), 16px; label (truncate): "Fetch base branch", "Check out files", "Init
  submodules", "Run setup script" (or the script name), "Start agent"; trailing detail / `{n}%`
  for checkout / "skipped", then duration, `text-xs muted-foreground tabular-nums`. Colors: failed
  `destructive-foreground`, warning `warning-foreground`, pending `secondary-label` at 40%, else
  `secondary-label`. Running stage label carries the shimmer overlay; elapsed ticks every 1s.
- Script output tail (running or failed script): `mb-1 ml-8 rounded-md border px-2.5 py-1.5
  font-mono text-2xs leading-relaxed`, exactly 4 lines (blank lines as NBSP, no wrap); normal
  `border-border bg-code muted-foreground`, failed `destructive/20` border `error-surface` bg.
- Failure text `mt-1 ml-8 text-xs text-muted-foreground`.
- Details (`dl`, `ml-8 grid auto/1fr gap-x-3 gap-y-0.5 text-xs`): Branch / Base / Path
  (middle-truncated mono) / Setup (command).
- Action row (`mt-0.5`, indented to the labels): `Button xs ghost-muted`: "Details" (chevron
  right/down), "Open terminal" (`TerminalIcon`), "Work locally" (`LaptopIcon`, drafts only while
  running), "Cancel" (`XIcon`, while running).
- Embedded + settled: one summary row (icon + header text + duration).
- **Background chip** in the working row when the script still runs after the agent started
  (`MT:2590-2625`): `Button micro ghost-muted ml-auto` spinner 12px + script name ("Setup script"
  fallback), aria `{name} is still running. Show setup progress.`; popover (bottom/end, 384px,
  `dropdown-glass`) with the embedded card.

### 8.6 Turn fold

`MT:2373-2396`.

```
div.group/timeline-row.relative.flex.items-center.gap-1 border-b border-border/60 pt-1 pb-2 pe-0.5
├─ button aria-expanded  flex items-center gap-1 rounded-md px-1 text-sm leading-relaxed
│    text-muted-foreground tabular-nums select-none, hover text-foreground (150ms)
│    "{label}" + ChevronRightIcon / ChevronDownIcon 14px
└─ timestamp (8.20) ms-auto  = fold createdAt
```

### 8.7 Assistant message

`MT:2398-2528`.

```
div.relative.min-w-0.px-1.py-0.5 (4 / 2)
├─ h3.sr-only "T3 Code"
├─ AssistantCitationSource wrapper (data-assistant-citation-source)
│    └─ ChatMarkdown (assistant variant; text or "(empty response)" when empty and not streaming;
│                     hard line breaks when the text starts with "★ Insight")
├─ [changed files card] (8.8)
└─ [meta] mt-1.5 (6)  when showAssistantMeta
```

Meta (`AssistantMessageMeta`): `flex items-center gap-2 text-xs tabular-nums`, opacity 0 → 100 on
`group/assistant` hover / focus-within (200ms; always visible on touch and in the separate
`assistant-meta` row, which uses `px-1` and `mt-0.5`). Contents: copy button (only when the
response is settled and the text is non-blank; copies `renderCodexDirectivesForCopy(text)`; same
`MessageCopyButton` as 8.1), then the `updatedAt` timestamp (`text-muted-foreground`, tooltip
long form) when not streaming. No "Worked for" or model label here.

### 8.8 Changed files card

`MT:3393-3485`, `components/chat/ChangedFilesTree.tsx:29-283`, `components/chat/DiffStatLabel.tsx`.
Shown inside the assistant row when its checkpoint summary has files.

```
section @container/changed-files mt-4 (16) rounded-lg (10) bg-secondary (dark bg-input/32)
├─ header sticky top-2 z-10 flex items-center justify-between gap-2 rounded-t-lg bg-secondary
│    px-3 pt-2 pb-1 (12 / 8 / 4)  (dark: bg-background under an input/32 gradient)
│  ├─ left flex flex-wrap gap-x-3 gap-y-1 text-xs font-medium text-foreground
│  │    "{n} changed file(s)" + DiffStatLabel (when any additions/deletions)
│  └─ right flex gap-1
│     ├─ [Button icon-xs ghost-muted] ChevronsDownUpIcon / ChevronsUpDownIcon 12px,
│     │     aria + tooltip "Collapse all folders" / "Expand all folders"  (only when a path has a directory)
│     └─ Button xs ghost-muted aria "Open diff": FileDiffIcon 12px + "Open diff" (text only when
│           the card is ≥384px wide), tooltip "Open the full diff" → diff panel at the first file
└─ tree px-2 pb-2 (8)
     row: flex w-full items-center gap-2 rounded-md py-1.5 pr-2, padding-left 8 + depth×14,
          hover bg-accent/60, focus ring 2px offset 1
     directory: ChevronRightIcon 14px muted-foreground/70 (rotates 90° 150ms) + FolderIcon/
          FolderClosedIcon 14px muted-foreground/75 + name font-mono text-2xs muted-foreground/90
          + stats (ml-auto, aligned grid) ; toggles only
     file: [14px spacer] + PierreEntryIcon 14px + name font-mono text-xs foreground/85
          (middle-truncated) + stats ; click → diff panel at that file; right-click → native
          file menu (Open, Reveal in Finder, Open with…)
```

- Tree: directories first, then files, locale-numeric order; single-child directory chains
  compacted (`a/b/c`). Expand-all state persists per thread+turn (`uiStateStore`,
  default collapsed); individual directory toggles are local and reset when expand-all changes.
- `DiffStatLabel`: `+{n}` `text-diff-addition` and `-{n}` `text-diff-deletion`, mono; aligned
  layout `inline-grid grid-cols-[4ch_4ch] gap-2 text-right`; compact numbers `1.2k`, `15k`,
  `1.5m`; aria `{a} additions, {d} deletions`.
- "Open diff" / file click: `useDiffPanelStore.selectTurn(threadRef, turnId, path)` then open the
  right panel's diff surface (`ChatView.tsx:9511-9520`, panels spec).

### 8.9 Proposed plan card

Row `min-w-0 px-1 py-0.5`; `components/chat/ProposedPlanCard.tsx`, `proposedPlan.ts`.

```
div rounded-3xl (22) border border/80 bg-card/70 p-5 (20)
├─ header flex flex-wrap items-center justify-between gap-3
│  ├─ flex gap-2: Badge secondary "Plan" (h 18, px 3, 12px, radius 6) + h3 text-sm font-medium truncate
│  │     title = first markdown heading, else "Proposed plan"
│  └─ Menu trigger Button icon-xs outline aria "Plan actions" EllipsisIcon 16px
│        menu (end, dropdown-glass, min-w 160): "Copy to clipboard" (→ "Copied!" 2s),
│        "Download as markdown", "Save to workspace" (disabled without a workspace)
├─ body mt-4 relative: ChatMarkdown of the plan minus its leading title / "Summary" heading
│     collapsible when > 900 chars or > 20 lines: collapsed = first 10 non-empty lines (+ "\n\n...")
│     in max-h-104 (416px) overflow-hidden with a 96px bottom fade from card/95 via card/80
└─ [toggle] mt-4 flex justify-center: Button sm outline "Expand plan" / "Collapse plan"
```

Save dialog: "Save plan to workspace", description `Enter a path relative to <code>{root}</code>.`,
field "Workspace path" (prefilled `{sanitized-title}.md` or `plan.md`), Cancel / Save ("Saving..."),
RPC `projects.writeFile {cwd, relativePath, contents}`; toasts "Plan saved to workspace", "Could not
save plan", "Enter a workspace path", "Workspace path is unavailable", "Could not copy plan".
Implement / Refine actions are composer controls when plan mode is on (composer spec).

### 8.10 Working row

`MT:2550-2588,2920-2955`.

```
div border-b border-border/60 pt-1 pb-2
└─ flex h-6 (24) min-w-0 items-baseline gap-2 px-1 text-sm leading-relaxed text-muted-foreground tabular-nums
   ├─ span relative shrink-0 overflow-hidden whitespace-nowrap
   │    "Setting up worktree…" (shimmer) | Minimize2Icon 12px + "Compacting…" (shimmer)
   │    | "Working for {timer}" | "Working..."
   └─ [background setup chip] (8.5)
```

Timer: `{s}s` under a minute, then `formatDuration` (`1m 5s`); updates text once per second
without re-rendering; starts at the row's `createdAt` (active turn start, or the latest user
message when the response spans several provider turns).

### 8.11 Thinking row

`MT:2724-2734`: `div.min-h-7` (28px) containing a live activity row (8.13) with label "Thinking",
no icon, shimmer on. Empty (height kept) while preparing a worktree or compacting.

### 8.12 Activity group and reasoning

`MT:2627-2918`.

- Header: button `flex min-h-6 w-full items-center rounded-md text-left` (no hover fill), focus ring,
  `aria-expanded`, `aria-label` `{label}, tool call failed` only when failed. Contains a live
  activity row: label = (active) live work label or "Thinking"; (settled) `summarizeToolGroup` of
  its work, or "Thought" / `Thought (×{n})`; icon from the live/last work entry (settled, no work:
  `brain`). Shimmer only while thinking with no live work.
- Expanded (`mt-2`): runs of work entries render as an expanded work group (8.16); runs of
  reasoning messages render as a reasoning block.
- Reasoning block (`ReasoningTraceBlock`): header (when the group also has work) button
  `flex min-h-6 items-center gap-1.5 rounded-md ps-0.5 pe-2 text-sm`, hover `bg-accent/20`;
  24px icon box `BrainIcon` 16px `icon-muted` at 70% (empty while streaming); label
  `secondary-label`: "Thinking" (streaming) / "Thought", or collapsed = first message flattened to
  one line; chevron 12px rotating 90° over 200ms. Body: `ms-7` (28px) `max-h-96` (384px) scroll,
  `flex-col gap-3`, `px-0.5 py-1`, one `ChatMarkdown` per message in `text-foreground italic` with
  hard line breaks.
- Standalone reasoning row (reasoning without a turn id): button "Thought" + brain + chevron,
  same body.

### 8.13 Live activity row (shared)

`MT:3161-3275`, `index.css:490-599`, `lib/visibleAnimation.ts`.

- Container `relative min-h-6 w-fit max-w-full min-w-0 overflow-hidden rounded-md text-sm
  leading-relaxed`.
- Content `flex min-h-6 items-center gap-1.5 py-0.5`, px 2px (with icon) / 4px; text
  `secondary-label`; 24px icon box (`icon-muted`, failed `tool-error-icon/40` with
  `aria-label="Tool call failed"`), icon 16px stroke 2; label truncates.
- While active and not failed: either the label text shines (`live-tool-shine`: 72px bright band
  of `contrast-foreground` sweeping across `secondary-label` text, 2.2s, `steps(30)` ≈ 13.6fps), or
  (shimmer mode) an overlay copy in `foreground` is revealed through a 72px soft mask that slides
  across in 2.2s linear while the text itself stays still.
- **Both are continuously repainting**. The fork pauses them when the row is off screen, the
  window is hidden, or reduced motion is on (none at all under reduced motion / forced colors).
  T3UI must do the same: animate only visible live rows; prefer a stepped (~14fps) repaint.

### 8.14 Live work row (`work-live`)

`MT:3277-3322`: agent spawn → spawn row (8.19). Otherwise a header button like 8.12 containing a
live activity row (no shimmer, `active` from the row): label = question preview, or
`liveWorkEntryLabel`; an answered question shows `{question}` then `{answer}` in `foreground`.
Expanded adds the details row (8.16 expanded).

### 8.15 Work toggle row

`MT:3324-3389`: button `group/timeline-row relative flex min-h-6 w-full items-center gap-1.5
rounded-md px-0.5 py-0.5 text-left text-sm leading-relaxed`, hover `bg-accent/20`, focus ring,
`aria-expanded`, `aria-label` `{summary}, tool call failed` when failed. 24px icon box (`icon-muted`,
icon 16px muted): `toolIcon` image, else `summaryToolIcon` / `toolSurface` / by summary kind:
pull-request → `pull-request`; read → `eye`; edit → `square-pen`; command → `terminal`; browser →
`browser`; device → `device`; search → `globe`; code-search → `search`; other → `wrench`;
dynamic-tool → `hammer`; agent-tool → `bot`; tone-tool → `zap`; update / mixed → `hammer`. Label
`flex-1 truncate secondary-label` = summary; hover-revealed timestamp; no chevron.

### 8.16 Work group section

`MT:2963-3157`.

- Standalone (`isExpandedToolGroup = false`): `section aria-label="Activity" -mx-1 space-y-0.5
  px-1 py-0.5` → `space-y-px` list of work entry rows.
- Expanded details (`{groupId}:details` row): a nested virtual list `role=region aria-label="Tool
  calls" tabIndex=0`, `max-h-[min(18rem,50dvh)]` (288px), scroll padding 24px, rounded-md,
  `scrollbar-gutter: stable`, 24px top/bottom fade masks when scrolled away from that edge, estimated
  row 24px. Appended entries keep it pinned to its end when it was within 1px. Scroll position and
  expanded entries persist per group.

### 8.17 Work entry row

`MT:4724-4975`.

```
div.group/timeline-row.relative.flex.flex-col.rounded-md.px-0.5  py-0.5 (0 inside an expanded group), mb-1 when expanded
    expandable: cursor-pointer, hover bg-accent/20, focus ring, role=button tabIndex=0 aria-expanded,
                Enter/Space toggle
├─ header flex select-none items-center gap-1.5
│  ├─ icon box 24px  (color: runtime.warning warning; destructive style destructive; failed
│  │     tool-error-icon/40 [aria "Tool call failed"]; tone tool icon-muted; error foreground;
│  │     thinking/info icon-muted)  icon 16px stroke 2, muted (70% opacity)
│  ├─ p flex min-w-0 w-full items-baseline gap-1.5 text-sm leading-relaxed
│  │    heading: displayLabel | question preview (" · " joined) | workEntryDisplayLabel
│  │      color: warning → font-medium warning; destructive → font-medium destructive;
│  │             tool-like → secondary-label; else foreground/80
│  │      collapsed: truncate; expanded: pre-wrap, selectable
│  │    [answer preview] truncate, foreground when answered else muted-foreground
│  ├─ [XIcon 12px tool-error-icon/40]  failed, not destructive, icon is an image
│  ├─ timestamp (8.20)
│  └─ chevron slot 16px: ChevronRightIcon 12px icon-muted 70%, rotates 90° over 200ms (invisible
│        when not expandable)
└─ expanded body (click/pointerdown do not toggle)
     viewed image: mt-1 ms-7 → workspace image, max 256px tall
     question answer: QuestionAnswerHistory (8.18)
     else: mt-1 ms-7 (28) rounded-md bg-muted/40 px-3 py-2 →
           pre max-h-64 (256px) overflow-auto pre-wrap font-mono secondary-label
           font-size --font-size-code (13px), leading-relaxed, selectable; no syntax colors
```

- Expandable when: a question answer; failed with text; MCP call with `toolData`; any raw command,
  command, detail, changed files, or a viewable workspace image.
- Body text (`MT:4439-4485`), blocks separated by blank lines, each skipped when equal to the
  heading: `MCP call\n{JSON pretty-printed toolData}`; the raw command (or command); the detail
  (command output); workspace-relative changed file paths, one per line.
- Destructive style: failed and (severe failure or not tool-like); it also drops the custom icon
  for `circle-alert`.
- Icon names (`workEntryIconName`, `MT:4490-4519`): question → `message-circle`; tool surface;
  T3 MCP icon; by action (read `eye`, edit `square-pen`, command `terminal`, code-search `search`,
  search `globe`, browser `browser`, device `device`, PR `pull-request`); `mcp_tool_call` →
  `wrench`; `dynamic_tool_call` → `hammer`; collab agent / task → `bot`; approvals and other
  updates → `hammer`; warnings / destructive → `circle-alert`.
- Server labels (upstream `apps/server/src/orchestration/Layers/ProviderRuntimeIngestion.ts`,
  identical in upstream b33eda13): "Command approval requested", "File-read approval requested",
  "File-change approval requested", "App access approval requested" (mcp-elicitation), "App
  permission approval requested" (permission), "Approval resolved", "Runtime error", `Tool denied:
  {toolName}`.

### 8.18 Question answer history

`MT:4977-5041`: `ms-7 mt-2 space-y-2`; per question `space-y-1`: question
`whitespace-pre-wrap text-sm text-muted-foreground`; answer `ms-3` same style (arrays joined with
", "); attachments `flex flex-wrap gap-2`: images `h-20 max-w-32 rounded object-contain` (80px tall,
128px max), others underlined name links.

### 8.19 Agent spawn rows

`MT:4535-4722`, `components/chat/agentSpawnSummary.ts`.

- Header button (`rounded-md`, hover `bg-accent/20`, focus ring) with a live activity row, icon
  `bot`: `{lead}` or `{lead} · {workflowName}`; lead = (`Launched` with batches | `Kicked off`
  while live | `Ran`) + `{n} subagent(s)` and/or `{n} [subagent ]batch(es)` joined by " and "
  (fallback "subagents"). Status (not shown in the header text) feeds `active` / `failed`.
- Expanded `ms-7 mt-0.5 flex-col`: member rows, then button "Open Agents panel ›" (`mt-1 rounded-sm
  px-1 text-xs text-muted-foreground`, hover foreground) → right panel Agents surface.
- Member row (`px-1 py-0.5 rounded-md`, expandable button-role, aria `{title}, {status}`): title
  (`text-sm`, `foreground/80`, failed `tool-error-icon/40`), optional role chip (max 112px,
  `rounded-sm border border/60 px-1 font-mono text-3xs muted`), status `font-mono text-2xs
  muted` ("Working", "Idle", "Completed", "Failed", "Stopped", with `{dur} · {n} tok` metrics);
  collapsed shows the first activity line `text-xs muted`; expanded shows a muted body like 8.17
  with the activity and `{model} · {effort}`.
- Data: `agentPanelModel` (`crt:state/subagentRuntime.ts`), derived from task activities.

### 8.20 Hover timestamps on work rows

`MT:2345-2372`: span `absolute me-1 shrink-0 whitespace-nowrap rounded-md text-xs
text-muted-foreground tabular-nums opacity-0 pointer-events-none`; while the row
(`group/timeline-row`) is hovered or focus-within it becomes `static`, opacity 100, interactive.
No transition; hidden timestamps take no space. Placed before the chevron so the chevron never
moves.

### 8.21 Assistant text citation

`components/chat/AssistantSelectionToolbar.tsx`, `AssistantCitationSource.tsx`.

- Selecting text inside an assistant message shows a floating `Button xs glass` "Cite" with
  `QuoteIcon` 14px (aria "Cite selection in composer"), or disabled "Shorten selection" (aria
  "Selection is too long to cite") over 8000 chars. Placed at the mouseup point (or the selection's
  end + 4px), clamped 8px inside the window; appears right after mouseup (500ms after a
  multi-click). Tab focuses it, Escape dismisses; scroll, resize, blur, right-click, or clicking
  elsewhere dismiss. Click adds a citation chip to the composer.
- Following a citation scrolls the source into view (`min(120px, viewport/3)` from the top) and
  pulses a `primary` highlight over the quoted range (opacity 0 → .45 → 0 → .45, hold, fade out
  over 3000ms). Toasts: "Could not open the cited response" / "Click the citation to try again.";
  "The quoted text has changed" / "Showing the source response. The saved quote is unchanged.";
  "Could not load the cited response" / "Load earlier turns, then click the citation to try again.
  Your saved quote is unchanged."; "The cited response is unavailable" / "It may have been removed.
  The selected text is still saved in your citation."; "The citation does not refer to an
  assistant response" / "The selected text is still saved in your citation."

### 8.22 Icon set

`MT:4164-4399`. lucide (^0.564): `bot` Bot, `brain` Brain, `device` Smartphone, `check`, `circle-
alert`, `eye`, `globe`, `hammer`, `message-circle`, `search`, `square-pen`, `terminal`, `wrench`,
`x`, `zap`; `pull-request` GitPullRequestArrow; custom: `browser` (window outline with a cursor,
paths at `MT:4185-4203`), `computer` (gradient tile `#00dff0→#3b9cff→#b044f5→#ff78b6` with a white
cursor, `components/Icons.tsx:906-928`), `t3-code` (T3 wordmark). Tool icon images (`website`
favicons, `themed-logo`, `native-app` icons via `assets.createUrl {_tag:"native-app-icon"}`) render
16px with 2px radius on `bg-background`, showing the glyph until loaded; muted icons at 70% opacity
(light mode also `brightness(60%)`). Pierre file icons: `components/chat/PierreEntryIcon.tsx`,
colors per language in `pierre-icons.ts` (`[light, dark]`, default `#84848a`/`#adadb1`).

## 9. Pending approvals and questions

The timeline never shows approve/answer controls. Requests appear as passive work rows (8.17:
"… approval requested" / "Approval resolved"; folded user input rows with the answers). The
interactive panels are in the composer's top drawer (`ChatComposer.tsx:6231-6300`,
`ComposerPendingApprovalPanel`, `ComposerPendingApprovalActions`, `ComposerPendingUserInputPanel`;
pending list from `derivePendingRequests(activities)`, `crt:pendingRequests.ts:124`). Composer spec.
Approval kinds there: "App access approval" (mcp-elicitation), "Command approval", "File read
approval", "App permission approval" (permission), "File change approval"; buttons Decline / Approve
/ ⋯ (Cancel, Always allow this session).

## 10. Animations

| Animation | Spec | Repaint |
|---|---|---|
| Live tool shine | 2.2s `steps(30)` infinite | continuous; pause off screen / hidden / reduced motion |
| Live activity focus mask | 2.2s linear infinite translate | continuous; same pausing; hidden under reduced motion |
| Spinners (setup stages, background chip) | 1s linear rotation | continuous; same pausing; `motion-safe` |
| Working timer | text update 1/s | 1Hz |
| Worktree stage elapsed | 1/s while running | 1Hz |
| Synced-row entry | opacity 600ms ease-out once | bounded |
| Streaming markdown block entry | opacity 600ms ease-out once | bounded |
| Chevron rotations | 150ms (fold, changed-files dir) / 200ms (work rows, reasoning) | bounded |
| Hover reveals (user footer, assistant meta) | opacity 200ms | bounded |
| Minimap strip / opacity | 150ms | bounded |
| Citation pulse | 3000ms | bounded |

## 11. Data and client API map

| Need | Upstream | t3UI today |
|---|---|---|
| Messages incl. `reasoning` role | thread detail | `ThreadState.thread.messages` (role `Reasoning` decoded) |
| Message `phase` | fork only | not decoded (not needed for upstream) |
| Activities with `sequence`, payload | thread detail | `OrchestrationThreadActivity` (`payload: Value`) |
| Checkpoints with `assistantMessageId`, files | thread detail | `OrchestrationCheckpointSummary` (`assistant_message_id`, `files` decoded) |
| Message `context` records (chips) | `OrchestrationMessage.context` | `OrchestrationMessageContext`; records keep kind-specific fields untyped in `fields` (`t3-protocol/src/orchestration.rs:561-576`); typed accessors per kind needed by the chip renderer |
| Attachment image URLs | `assets.createUrl {_tag:"attachment", attachmentId}` | `methods::AssetsCreateUrl` |
| Worktree setup snapshot | `worktree-setup` activity payload + live setup stream | **missing** typed decoder for `WorktreeSetupSnapshot` (`contracts:worktreeSetup.ts`) and the live stream |
| Queued messages | client-side store | local (composer agent) |
| Agent panel model | derived from task activities | **missing** port of `crt:state/subagentRuntime.ts` |
| Plan save | `projects.writeFile` | `t3_protocol` `ProjectWriteFileInput` / `ProjectWriteFileResult` exist |
| Thread visited / changed-files expansion | local UI state | `t3-logic::ui_state` |

## 12. Reuse map

| Module | Verdict |
|---|---|
| `t3-logic/src/timeline/mod.rs` (`TimelineModel`, `share_rows`, `diff_rows`) | Keep the model / structural sharing / diff mechanics. Re-key on the fork's row ids (`live-activity-row`, `working-indicator-row`, `turn-fold:{turnId}`, …) so splices stay minimal |
| `t3-logic/src/timeline/rows.rs` | July rules: rewrite to section 4. Kinds to add: `activity-group`, `work-live`, `assistant-meta`, `context-compaction`, `thinking`, `worktree-setup`, `queued-message`; remove `ToolStack` and the `ChangedFiles` row (now inside the assistant row). Fold labels already match ("Worked for", "You stopped after") |
| `t3-logic/src/timeline/work_log.rs` | Partially reusable (sorting, lifecycle rank, shell-wrapper unwrapping, exit-code stripping, changed files). Add: user-input folding, agent spawn batching, agent-internal filtering, `turn.plan.updated` drop, runtime-warning filter, keyed lifecycle collapse via `toolCallId`, `toolSource/toolIcon/toolSurface/toolData/viewedImagePath/taskId/agentRole` fields |
| branch `chat` `timeline/presentation.rs` (a5720f9, unlanded) | Port of `PRES`; land it (wire into `mod.rs`, add the missing `WorkLogEntry` fields). Replace its `command_program_name` stand-in with a full port of `commandLabel.ts` using `commandLabel.test.ts` as the test table |
| `t3-logic/src/timeline/format.rs` | Keep `format_duration` (check against `orchestrationTiming.ts:14-31`), timestamp formatting: add `yesterday at` and numeric-date forms, ordinal tooltip with year |
| `t3-logic/src/timeline/plan.rs` | Proposed-plan title/preview helpers: re-diff against `proposedPlan.ts` (900 chars / 20 lines / 10 preview lines) |
| `t3-app/src/chat/rows.rs` | July visuals: replace with section 8 |
| `t3-app/src/chat/timeline.rs` | Keep list bookkeeping; add LegendList-equivalent behaviors from 6.1-6.4 and the minimap |
| `t3-diff` (`ChangedFilesTree`) | Tree building / stats reusable if it matches 8.8 (compacted chains, dir-first order); visuals must follow 8.8 (card, sticky header, 14px indent step) |
| `t3-client/tests/timeline_streaming.rs` | Reusable harness; update expectations to the new rows |

## 13. Reference screenshots needed

| Name | How to reach | Data |
|---|---|---|
| `timeline-showcase-folded` | `thread-aurora-tour` at the top | user bubble, "Worked for …" fold, answer, changed files card |
| `timeline-showcase-expanded` | click the fold | activity rows, work toggles, commands, web search, MCP, file change |
| `timeline-work-toggle-expanded` | expand a "Ran 3 commands" style toggle | nested tool list with fades |
| `timeline-work-entry-expanded` | expand a command row | output body box |
| `timeline-running` | `thread-aurora-watch` | working row with timer, live tool row (shine frame), thinking row |
| `timeline-approval-row` | `thread-aurora-migrate` | "Command approval requested" row; composer approval panel |
| `timeline-question-answered` | `thread-borealis-persist` after answering | folded user input row expanded (question/answer history) |
| `timeline-plan-card` / `-collapsed` | `thread-aurora-plan` | plan card, plan menu open, collapsed with fade |
| `timeline-failure` | `thread-cirrus-deploy` | failed tool styling, error banner |
| `timeline-user-hover` | hover a user message | timestamp, Edit from here, copy |
| `timeline-user-long` | seed a >600 char prompt | collapsed body with fade + "Show full message" |
| `timeline-user-attachments` | seed a prompt with 2 images + 1 file | media grid + file row |
| `timeline-assistant-hover` | hover the answer | copy + timestamp meta |
| `timeline-changed-files-expanded` | click "Expand all folders" | tree with nested dirs |
| `timeline-minimap` | thread with ≥2 user messages, window wide enough for a 48px gutter, hover the rail | strips + preview card |
| `timeline-load-earlier` | thread with more turns than the first page | "Load earlier turns" header |
| `timeline-queued` | send while `thread-aurora-watch` runs | dashed queued bubble |
| `timeline-empty` | new server thread with no messages (route directly) | "Send a message to start the conversation." |
| `timeline-worktree-setup` | new thread in worktree mode with a setup script | stage list + output tail |

## 14. Open questions / risks

- **LegendList parity.** The fork relies on LegendList features (anchored end space,
  maintain-visible-content-position with a per-row filter, `alwaysRender`, async `scrollToIndex`).
  GPUI `list` has none of these directly; `docs/handoff/chat.md` documents the workarounds in
  `timeline.rs`. Expect the most iteration here.
- **Shimmer / shine** are continuously repainting; T3UI must gate them to visible rows and should
  step them (~14fps) to stay within the no-repaint rule.
- **Seed coverage.** Several states (activity groups with reasoning, agent spawns, worktree setup,
  compaction rows, citations, context chips, long user messages, attachments) are not produced by
  the current fake Codex scenarios. The harness needs new scenarios or recorded fixtures.
- **Reasoning messages** depend on the provider; fake Codex may never emit `reasoning`. Activity
  groups require at least one reasoning message, so without one every run is a work toggle.
- **`@pierre/diffs` FileDiff** inside review-comment chips and the changed-files card's Pierre
  icons need `t3-diff` parity.
- **Agent panel model** (`subagentRuntime.ts`) is a large client-runtime module with no Rust port.
