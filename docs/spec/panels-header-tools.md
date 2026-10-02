# Chat header tools: project scripts, Open in, Git actions, PR checkout

> Refreshed against fe7d3092c (2026-10-02). Part of the panels spec; index in docs/spec/panels.md.

The action cluster at the right end of the chat header (`components/chat/ChatHeader.tsx:496-533`) and
everything it opens. The breadcrumb/title part of the header belongs to the chat spec; the panel
layout toggles at the window's top-right corner are in panels.md §5. Paths and conventions as in
`panels.md` §0.

## 0. Primitives used

| Use | Primitive | Resolved (desktop) |
| --- | --- | --- |
| Primary buttons (run script, git quick action, Open) | `Button size="xs" variant="outline"` | h-6 (24px), px 7px, gap 4px, `text-xs font-medium`, `rounded-[8px]`, `bg-popover` + `border-input`, `shadow-xs/5`, dark `bg-input/32`, hover `bg-accent/50` (dark `bg-input/64`), icon 14px in `--contrast-muted-foreground`; edge bevel (design-system) |
| Split-button chevrons | `Button size="icon-xs" variant="outline"` | 24x24, chevron 16px (`size-4`) |
| Joined buttons | `Group` + `GroupSeparator` (`components/ui/group.tsx`) | children joined: inner corners squared, inner borders removed, a 1px separator between them; the separator is hidden below the `@3xl/header-actions` breakpoint |
| Menus | `Menu`, `MenuItem`, `MenuItemLabel`, `MenuShortcut`, `MenuSub`, `MenuGroupLabel`, `MenuSeparator` | design-system spec; `density="touch"` rows in the collapsed menu presentation |
| Disabled-reason popovers | `Popover` + `PopoverTrigger openOnHover` + `PopoverPopup tooltipStyle` | open after 300ms hover; tooltip look (`rounded-md text-xs shadow-md/5`) |
| Dialogs | `Dialog`/`DialogPopup` (default max width; `max-w-xl` where noted), `AlertDialog` | design-system spec |
| Toasts | `toastManager` (`loading`, `success`, `error`, `info`), thread-scoped (`stackedThreadToast`) | overlays/design-system spec |

With a named theme (`html[data-theme-id]`, not the default "system" theme) header buttons use the
`--toolbar-control*` tokens and 150ms color transitions (`index.css:1366-1431`).

---

## 1. Cluster layout and responsive rules

```
ChatHeader root: @container/header-actions  flex min-w-0 flex-1 items-center gap-3 (12px)
├─ breadcrumb (chat spec)                                   flex-1
└─ div [data-chat-header-actions]  flex shrink-0 items-center justify-end
       gap-2 (8px), gap-3 (12px) when the header is >= 48rem (768px) wide
       padding-right: 57px (`pr-14.25`) while the right panel is closed (room for the two fixed
       panel toggles, panels.md §5.1); 0 while it is open
    ├─ [collapsed only] "More header actions" menu trigger (1.2)
    ├─ ProjectScriptsControl     (when the project has scripts data)                  §2
    ├─ OpenInPicker              (never in the header today: SHOW_OPEN_IN_PICKER = false) §4
    └─ GitActionsControl         (when the thread has a project and a git cwd)        §5
```

- Container query `@3xl/header-actions` (>= 768px of header width): buttons show their text labels;
  below it they are icon-only and the split-button separators hide. At 1440px with the sidebar open
  and the right panel closed the labels show; opening a 540px right panel usually drops below 768px
  (icon-only).
- Crossing that breakpoint fades the whole action group in (`opacity 0 -> 1`, `min(100ms, panel
  animation duration)`, ease-out) only when panel animations are active (default off)
  (`panelAnimations.ts:34-69`, `ChatHeader.tsx:154-166`).
- The padding-right change animates with the panel duration only when animations are active.
- Right-click on the action group does nothing special (the header's thread menu ignores it).

### 1.1 Order in the toolbar

`[Run <script>│▾]  [<git quick action>│▾]` (scripts first, then git). With no scripts: an `Add
action` button (or an import menu, §2.1). Non-repos show `Initialize Git` instead of the git group.

### 1.2 Collapsed presentation (header narrower than 512px)

Reachable on the desktop when the sidebar and right panel squeeze the header
(`ChatHeader.tsx:168-180`): the actions move into one menu.

- Trigger: `Button size="icon-sm" variant="ghost"` (28x28), `Ellipsis` 16px,
  `aria-label="More header actions"`; menu `align="end"`, `aria-label="Header actions"`.
- Items, separated by `MenuSeparator` between the scripts block and the git block:
  - Scripts (`presentation="menu"`): `Run <primary>` (script icon, its shortcut), then a `Project
    actions` submenu (play icon) holding the full scripts list (§2.2); with no scripts and nothing to
    import: `Add project action…`.
  - Git (`presentation="menu"`): the quick action as an item (its icon + label; disabled with the
    reason printed below it in `max-w-64 px-2 py-1.5 text-xs text-warning`), then a `Git actions`
    submenu (provider icon) with the git menu items (§5.3); disabled reasons print under each
    disabled item (`max-w-64 px-2 pb-2 text-xs text-muted-foreground`) instead of popovers. Non-repo:
    `Initialize Git` / `Initializing...`.
- The controls are re-parented, not remounted, so an open dialog survives the switch.

---

## 2. Project scripts (`components/ProjectScriptsControl.tsx`)

Data: the active project's scripts (`ProjectScript {id, name, command, icon: play|test|lint|
configure|build|debug, runOnWorktreeCreate, async?, previewUrl?, autoOpenPreview?}`), resolved from
server settings (`projectSettingsOverrides[projectId].defaultProjectScripts`, legacy
`projectScriptOverrides`, then the project's own) (ported in `crates/t3-logic/src/project_scripts.rs`
on `tools-logic-land`); scripts declared in the workspace's `t3.json` (read with `projects.readFile`,
`hooks/useT3ProjectFileScripts.ts`); keybindings (`script.<id>.run`).

Primary script: the project's last-invoked script (localStorage `LAST_INVOKED_SCRIPT_BY_PROJECT_KEY`,
`ChatView.tsx:1782-1786`) if it still exists, else the first non-setup script, else the first
(`projectScripts.ts:89-92`).

### 2.1 Toolbar states

| State | Rendering |
| --- | --- |
| has a primary script | `Group aria-label="Project scripts"`: run button (`outline xs`; collapsed width `w-6` 24px icon-only, `w-auto` at >= 768px) with the script icon (14px) + name (`ml-0.5`, only at >= 768px); `aria-label="Run <name>"`; tooltip (top) `Run <name>`. Separator (>= 768px only). Chevron menu button (`icon-xs outline`, `ChevronDown` 16px, `aria-label="Script actions"`) opening the scripts menu (`align="end"`). |
| no scripts, `t3.json` has importable scripts | one `outline xs` menu trigger `aria-label="Project actions"`: `Plus` 14px, `Add action` (>= 768px), `ChevronDown` 14px. Menu: the import group (below) then `Add action` (`Plus`). |
| no scripts | `outline xs` button (24px wide below 768px) `Plus` + `Add action` (>= 768px); tooltip `Add action`; opens the editor dialog. |

Script icons (`projectScriptEditor.tsx:59-81`): play `Play`, test `FlaskConical`, lint `ListChecks`,
configure `Wrench`, build `Hammer`, debug `Bug`.

### 2.2 Scripts menu

- One item per script: script icon (16px), label (`<name>` or `<name> (setup)` for
  `runOnWorktreeCreate`), and at the right a 24px slot holding the script's shortcut (`MenuShortcut`,
  only when one is bound) that, on row hover/focus, fades out and is replaced by an edit button (`Button ghost icon-xs`
  forced to 24x24, `Settings` 14px, `aria-label="Edit <name>"`). Clicking the row runs the script;
  the edit button opens the editor (§3). In the collapsed menu the edit button is always visible and
  the shortcut sits left of it.
- Import group (scripts in `t3.json` whose command or name isn't already a script): separator (when a
  primary exists), group label `From t3.json`, one item per script (icon, name, a `Download` 14px
  in the shortcut slot, `aria-label="Import"`). Clicking imports it as a new script (icon defaults to
  play, setup flags carried over, no keybinding); a failed import opens the add dialog prefilled with
  the error (fallback `Failed to import action.`).
- Last: `Add action` (`Plus`).

### 2.3 Running (`ChatView.tsx:4263-4373`)

Opens the terminal drawer and runs `<command>\r` in the active terminal, or in a new 120x30 terminal
when the active one is busy; details in panels-terminal.md §4. A failure sets the thread error banner
(`Failed to run script "<name>".` or the error). Keybindings `script.<id>.run` trigger the same.

---

## 3. Script editor dialog (`components/projectScriptEditor.tsx:142-481`)

Shared with project settings. `Dialog` (default width):

- Title `Add Action` / `Edit Action`; description `Actions are project-scoped commands you can run
  from the top bar or keybindings.`
- Form (`space-y-4`, disabled while saving), each field `space-y-1.5` with a `Label`:
  1. `Name`: a 36x36 outline icon button (`Choose icon`, current icon 18px) opening a popover with a
     3-column grid of the six icons (`rounded-md border px-2 py-2 text-xs`, icon 16px + label `Play`,
     `Test`, `Lint`, `Configure`, `Build`, `Debug`; selected `border-primary/70 bg-primary/10`, dark
     `ring-1 ring-primary/30`; others `border-border/70 hover:bg-accent/60`, dark
     `bg-white/[0.035]`); then the name `Input` (autofocus, placeholder `Test`).
  2. `Keybinding`: read-only `Input`, placeholder `Press shortcut`; key presses are captured as a
     binding (Tab passes through, Backspace/Delete clear); help `Press a shortcut. Use Backspace to
     clear. Shortcuts are environment-wide. Projects using the same action share its shortcut.`
     (`text-xs text-muted-foreground`, `Backspace` in `code`).
  3. `Command`: `Textarea`, placeholder `bun test`.
  4. `Preview URL (optional)`: `Input`, placeholder `http://localhost:5173`; help `Open this URL in the
     in-app preview when this action runs.`
  5. Switch rows (`flex items-center justify-between gap-3 rounded-md border border-border/70 px-3
     py-2 text-sm`, dark borderless on `bg-white/[0.035]`): `Run automatically on worktree creation`;
     `Wait for it to finish before the agent starts` (disabled and 60% opacity unless the previous is
     on); `Open preview automatically when this action runs` (disabled and 60% unless a preview URL is
     set).
  6. Error line `text-sm text-destructive`: `Name is required.`, `Command is required.`, keybinding
     validation errors, server errors, `Failed to save action.`
- Footer: `Delete` (`destructive-outline`, `mr-auto`, editing only) -> confirm alert `Delete action
  "<name>"?` / `This action cannot be undone.` with `Cancel` and `Delete action` (destructive);
  `Cancel` (outline); submit `Save action` / `Save changes` / `Saving…`.
- Saving: the script id is derived from the name (`nextProjectScriptId`, de-duplicated with `-2`,
  `-3`...). RPC `server.updateSettings {patch: {projectSettingsOverrides: {[projectId]: {...,
  defaultProjectScripts: nextScripts}}}}` (or the legacy `projectScriptOverrides` patch when the
  environment lacks capability `projectSettingsOverrides`), then on desktop
  `server.upsertKeybinding {key, command: "script.<id>.run"}` when a binding was captured
  (`ChatView.tsx:4400-4462`). Delete toasts `Deleted action "<name>"` or `Could not delete action`.

---

## 4. Open in (`components/chat/OpenInPicker.tsx`)

**Not shown in the chat header by default**: `SHOW_OPEN_IN_PICKER = false` (`ChatHeader.tsx:107`). Its
only default-desktop appearance is the compact variant in every file tab's subheader
(panels-files.md §1.2). Spec both variants so the header can turn it on.

- Options: editors the server reports in `availableEditors` (local mode) or the remote-capable set
  (remote modes), in this order: Cursor, Trae, Kiro, VS Code, VS Code Insiders, VSCodium, Zed,
  Antigravity, IntelliJ IDEA, Aqua, CLion, DataGrip, DataSpell, GoLand, PhpStorm, PyCharm, Rider,
  RubyMine, RustRover, WebStorm, then the file manager (`Finder` on macOS, with the Finder logo).
  Brand logos from `components/Icons.tsx` / `JetBrainsIcons.tsx` render at full opacity in
  `text-foreground`; generic icons in `text-muted-foreground` (`:70-193`).
- Preferred editor: localStorage `t3code:last-editor` if still available, else the first available
  in `EDITORS` order (`editorPreferences.ts:41-49`).
- Toolbar variant: `Group aria-label="Open in editor"`: `outline xs` button with the preferred editor's
  logo (14px) and `Open` (shown at >= 768px; always hidden in `compact`), disabled without a
  preferred editor, path, or remote route; `aria-label="Open file in preferred editor"` when compact;
  separator; chevron (`icon-xs outline`, `aria-label="Choose editor"`) menu (`align="end"`): one item
  per editor (logo + label; the preferred one shows the `editor.openFavorite` shortcut `⌘O`). Empty:
  `No installed editors found` (disabled). Remote without SSH: `No SSH route to <environment>`
  (disabled). Remote links mode, first use: a disabled hint `Opens over SSH. Needs your key on
  <environment>`.
- Menu variant (collapsed header): `Open in <Editor>` (+ shortcut) and an `Open in…` submenu
  (`SquareArrowOutUpRight`).
- Action: RPC `shell.openInEditor {cwd: <path>, editor}` (local) or a remote deep link
  (`buildRemoteOpenUrl`), then remembers the editor. `⌘O` (`editor.openFavorite`) opens the preferred
  editor when `enableShortcut` (header only; file tabs disable it).

---

## 5. Git actions (`components/GitActionsControl.tsx`, `GitActionsControl.logic.ts`)

### 5.1 Data

- `subscribeVcsStatus {cwd}` / `vcs.refreshStatus {cwd}` -> `VcsStatusResult` (local: `isRepo`,
  `sourceControlProvider`, `hasPrimaryRemote`, `isDefaultRef`, `refName`, `hasWorkingTreeChanges`,
  `workingTree {files[{path, insertions, deletions}], insertions, deletions}`; remote: `hasUpstream`,
  `aheadCount`, `behindCount`, `aheadOfDefaultCount?`, `pr? {number, title, url, baseRef, headRef,
  state, isDraft?}`). Typed in `crates/t3-protocol/src/vcs.rs:30-100`.
- Refreshed (250ms debounce) on window focus and when the document becomes visible, and whenever the
  git menu opens.
- While loading, `isRepo` is assumed true (no flash of `Initialize Git`).
- Provider terminology (`shared:sourceControl.ts`): `shortLabel` / `singular`, e.g. GitHub `PR` /
  `pull request`, GitLab `MR` / `merge request`; provider icon (GitHub, GitLab, Forgejo, Bitbucket,
  Azure DevOps logos, else the generic PR glyph) (`sourceControlPresentation.ts`).
- Logic (quick action, menu items, disabled reasons, progress stages, default-ref dialog copy) is
  ported in `crates/t3-logic/src/source_control.rs` (branch `tools-logic-land`); the tables below are
  the visible contract.

### 5.2 Quick action button (`resolveQuickAction`, `GitActionsControl.logic.ts:166-300`)

`Group aria-label="Git actions"` (shrink-0): quick action button (`outline xs`, icon 14px + label at
>= 768px; icon-only below), separator (>= 768px), chevron (`icon-xs outline`, `aria-label="Git action
options"`, disabled while an action runs) opening the git menu (`align="end"`).

| Situation (first match) | Label | Icon | Effect |
| --- | --- | --- | --- |
| action running | `Commit` (disabled) | `GitCommit` | hint `Git action in progress.` |
| no status | `Commit` (disabled) | `GitCommit` | hint `Git status is unavailable.` |
| detached HEAD | `Commit` (disabled) | `GitCommit` | hint `Create and checkout a ref before pushing or opening a <singular>.` |
| changes, no upstream and no remote | `Commit` | `GitCommit` | action `commit` |
| changes, open PR or default ref | `Commit & push` | `CloudUpload` | `commit_push` |
| changes | `Commit, push & <PR>` | provider icon | `commit_push_pr` |
| no upstream, no remote, open PR not ahead | `View <PR>` | provider icon | open PR |
| no upstream, no remote | `Publish repository` | `CloudUpload` | opens the publish wizard (5.6) |
| no upstream, not ahead, open PR | `View <PR>` | provider | open PR |
| no upstream, not ahead | `Push` (disabled) | `CloudUpload` | hint `No local commits to push.` |
| no upstream, ahead, open PR or default ref | `Push` | `CloudUpload` | `push` (`commit_push` on the default ref) |
| no upstream, ahead | `Push & create <PR>` | provider | `create_pr` |
| ahead and behind | `Sync ref` (disabled) | `Info` | hint `Branch has diverged from upstream. Rebase/merge first.` |
| behind | `Pull` | `CloudDownload` | `vcs.pull` |
| ahead, open PR or default ref | `Push` | `CloudUpload` | `push` / `commit_push` |
| ahead | `Push & create <PR>` | provider | `create_pr` |
| open PR with upstream | `View <PR>` | provider | open PR |
| ahead of default, not default ref | `Create <PR>` | provider | `create_pr` |
| otherwise | `Commit` (disabled) | `GitCommit` | hint `Branch is up to date. No action needed.` |

- A disabled quick action renders as an `aria-disabled` button whose hover popover (bottom, start)
  shows the hint (fallback `This action is currently unavailable.`); clicking it shows an `info`
  toast titled with the label and the hint.
- `View <PR>`: opens the thread's PR as a right-panel `pull-request` tab when the environment supports
  pull requests and the project has a repository identity (`onOpenPullRequest`,
  `ChatView.tsx:4779-4795`); otherwise opens the URL in the browser; no URL: error toast `No open
  pull request found.`; failure `Unable to open pull request link`.
- Pull: loading toast `Pulling...`, then `Pulled` / `Updated <ref> from <upstream>` or `Already up
  to date` / `<ref> is already synchronized.`; failure `Pull failed` + message. RPC `vcs.pull {cwd}`
  -> `{status: "pulled"|"skipped_up_to_date", refName, upstreamRef?}`.
- Non-repo: a single `outline xs` button `GitBranchPlus` 14px + `Initialize Git` (`Initializing...`
  while running; label always visible, `ml-0.5`); RPC `vcs.init {cwd}`; failure toast `Git
  initialization failed`.

### 5.3 Git menu (`buildMenuItems`, `:94-164`; `getMenuActionDisabledReason`, `GitActionsControl.tsx:299-366`)

Items (provider icons from 5.1):

1. `Commit` (`GitCommit`): enabled with working-tree changes; opens the commit dialog (5.4).
2. `Push` (`CloudUpload`), only when the repo has a primary remote: enabled with a branch, not behind,
   ahead > 0, and an upstream or a primary remote. Runs `push`.
3. `View <PR>` (open PR) or `Create <PR>` (enabled with a branch, clean tree, no open PR, commits ahead
   of default, not behind, upstream or remote). Only with a primary remote.
4. `Publish repository...` (`CloudUpload`) when the repo has no primary remote (disabled while an
   action runs).
5. Notes: `Detached HEAD: create and check out a branch to enable push and pull request actions.`
   (`px-2 py-1.5 text-xs text-warning`) when detached; `Behind upstream. Pull/rebase first.` when
   clean, behind and not ahead; the status error (`text-destructive`).

Disabled items are wrapped in a hover popover (side left) with the reason: `Git action in progress.`,
`Git status is unavailable.`, `Worktree is clean. Make changes before committing.`, `Detached HEAD:
check out a branch before pushing.`, `Commit or stash local changes before pushing.`, `Branch is
behind upstream. Pull/rebase before pushing.`, `Add an "origin" remote before pushing.`, `No local
commits to push.`, and for PRs `Detached HEAD: check out a branch before creating a <singular>.`,
`Commit local changes before creating a <singular>.`, `Add an "origin" remote before creating a
<singular>.`, `No local commits to include in a <singular>.`, `Branch is behind upstream. Pull/rebase
before creating a <singular>.`, `View <singular> is currently unavailable.` (fallbacks `… is currently
unavailable.`).

### 5.4 Commit dialog (`:1856-2007`)

`Dialog` (default width), title `Commit changes`, description `Review and confirm your commit. Leave
the message blank to auto-generate one.`

- Summary card `space-y-3 rounded-xl bg-zinc-25 p-3 text-sm ring-1 ring-black/5` (dark
  `bg-white/[0.035] ring-white/5`):
  - Grid row: `Branch` (muted) | branch name (`font-medium`, `(detached HEAD)` when none) and, on the
    default ref, `Default branch` right-aligned in `text-warning`.
  - Files header: (edit mode: a select-all `Checkbox`, indeterminate when partial) `Files` (muted),
    `(<selected> of <total>)` when some are excluded and not editing; right: `Edit` / `Done`
    (`ghost xs`).
  - No files: `none` (`font-medium`). Else a 176px (`h-44`) list `rounded-lg bg-card ring-1 ring-black/5`
    (dark `bg-white/[0.025] ring-white/5`) in a ScrollArea, rows `flex items-center gap-2 rounded-md
    px-2 py-1 font-mono hover:bg-accent/50`: (edit mode: a `Checkbox`), the path start-truncated
    (`StartTruncatedPath`, muted when excluded), right: `Excluded` (muted) or `+<insertions>` (diff
    addition color) ` / ` `-<deletions>` (diff deletion color). Clicking a row opens the file in the
    preferred editor (`Editor opening is unavailable.` / `Unable to open file` toasts).
  - Totals right-aligned in mono: `+<sum>` / `-<sum>` of the selected files.
- `Commit message (optional)` (`text-sm font-medium`) + `Textarea size="sm"` placeholder `Leave empty
  to auto-generate`.
- Footer (bare): `Cancel` (outline sm), `Commit on new branch` (outline sm; runs `commit` with
  `featureBranch: true`, skipping the default-ref prompt), `Commit` (default sm). Both commit buttons
  are disabled when no file is selected. Partial selection sends `filePaths`.

### 5.5 Running an action (`runGitActionWithToast`, `:1225-1488`)

1. On the default ref, `push` / `create_pr` / `commit_push` / `commit_push_pr` first ask
   (`resolveDefaultBranchActionDialogCopy`, logic `:313-350`), `Dialog` `max-w-xl`:
   - push only: `Push to default ref?` / `This action will push local commits on "<ref>". You can
     continue on this ref or create a feature ref and run the same action there.`; continue
     `Push to <ref>`.
   - commit + push: `Commit & push to default ref?` / `This action will commit and push changes on
     "<ref>". …`; `Commit & push to <ref>`.
   - with PR: `Push & create <PR> from default ref?` / `Commit, push & create <PR> from default ref?`
     with matching descriptions and continue labels.
   - Footer: `Abort` (outline sm, `mr-auto`), the continue label (outline `sm-multiline`), `Check out
     feature branch & continue` (default `sm-multiline`, runs with `featureBranch: true`).
2. A loading toast (no timeout) titled with the first progress stage (fallback `Running git
   action...`) and description `Waiting for Git...`. Stages (`buildGitActionProgressStages`):
   `Preparing feature ref...`, `Generating commit message...` (no custom message), `Committing...`,
   `Pushing to <target>...` / `Pushing...`, `Preparing <PR>...`, `Generating <PR> content...`,
   `Creating <singular>...`.
3. Stream `git.runStackedAction {actionId (uuid), cwd, action, commitMessage?, featureBranch?,
   filePaths?, threadId? (server threads; links a created PR)}` -> progress events
   `action_started`, `phase_started {label}` (title = label), `hook_started {hookName}` (title
   `Running <hook>...`), `hook_output {text}` (description = last line), `hook_finished`,
   `action_finished {result}`, `action_failed`. The toast refreshes every second; the description
   falls back to `Running for <n>s` / `Running for <m>m <s>s` since the phase/hook started.
4. Result: the toast turns into `success` with the server-provided `toast.title`/`description`,
   auto-dismissing 10s after it has been visible, and an action button when `toast.cta` is
   `open_pr {label, url}` (opens the PR) or `run_action {label, action}` (runs the follow-up action).
   Failure: `error` toast `Action failed` + message. A newly created branch is written to the
   thread's metadata (`branch`).

### 5.6 Publish repository wizard (`PublishRepositoryDialog`, `:408-944`)

Shown for repos without a primary remote (quick action `Publish repository` or the menu item). A
multi-step dialog: title `Publish repository`, `Pick where to host it, then point us at a repo to push
to.`; provider choice (`Forgejo / Gitea` "Your signed-in server", `GitHub` github.com, `GitLab`
gitlab.com, `Bitbucket` bitbucket.org, `Azure DevOps` dev.azure.com; readiness hints such as `Provider
status unavailable. Open Settings -> Source Control and rescan.` or `<Provider> is not authenticated.
Open Settings -> Source Control for setup guidance.`), repository path (placeholder `owner/repo`,
`group/project`, `workspace/repository`, `project/repository`), visibility `Public` (`Anyone on the
web`) / `Private` (`Only invited people`), remote name (placeholder `origin`), protocol `HTTPS` /
`SSH`, a `Summary` step, and results `Repository created` / `Repository published` / `Remote "<name>"
is set up. Make a commit and push it to share your code.` / `Publish failed`; buttons `Cancel`,
`Publish`, `Done`. RPC `sourceControl.publishRepository` (upstream `contracts:rpc.ts:444,966`) plus
provider discovery from the server config. Not ported anywhere yet (tools handoff); spec it in full
from the source when it is built.

---

## 6. PR checkout dialog (`components/PullRequestThreadDialog.tsx`)

Not a header control: opened from the branch picker in a **local draft** thread when the user enters a
PR reference (`canCheckoutPullRequestIntoThread = isLocalDraftThread`, `ChatView.tsx:1961,2540-2555,
10272-10274`; branch toolbar spec).

- `DialogPopup max-w-xl`; title: provider icon (16px, `me-2`) + `Checkout <singular>`; description
  `Resolve a <Provider> <singular>, then create the draft thread in the main repo or in a dedicated
  worktree.`
- Field label (`text-xs font-medium capitalize`) `<singular>`; `Input` placeholder `<PR> URL,
  checkout command, or #42`, focused and selected on open. Enter (not composing) confirms in worktree
  mode.
- References parse with `parsePullRequestReference` (URLs for GitHub/GitLab/Forgejo/Azure, `gh`/`glab`
  /`tea`/`az` checkout commands, `123`, `#123`; ported in `source_control.rs`). Resolution is debounced
  450ms: `git.resolvePullRequest {cwd, reference}` -> `{pullRequest {number, title, url, baseBranch,
  headBranch, state}}`; cached results show immediately.
- Resolved card `rounded-xl border border-border/70 bg-muted/24 p-3`: title (`text-sm font-medium`
  truncate), `#<n> · <head> to <base>` (`text-xs text-muted-foreground` truncate), state on the right
  (`text-xs capitalize`): open `text-emerald-600 dark:text-emerald-300/90`, merged
  `text-violet-600 dark:text-violet-300/90`, closed `text-zinc-500 dark:text-zinc-400/80`.
- Resolving: `Spinner size="sm"` + `Resolving <singular>...` (`text-xs text-muted-foreground`).
- Errors (`text-xs text-destructive`): `Paste a <singular> URL, checkout command, or enter 123 / #123.`
  (empty), `Use a <singular> URL, checkout command, 123, or #123.` (unparseable), the resolve error,
  or `Failed to prepare <singular> thread.`
- Footer: `Cancel` (outline sm), `Local` / `Preparing local...` (outline sm), `Worktree` / `Preparing
  worktree...` (default sm); both disabled until resolved. RPC `git.preparePullRequestThread
  {cwd, reference, mode: "local"|"worktree", threadId? (worktree)}` -> `{branch, worktreePath}`, then
  the draft thread switches to that branch/worktree and the dialog closes. The dialog can't be
  dismissed while preparing.

`PullRequestContextDetails` (`components/PullRequestContextDetails.tsx`) is the tooltip body of a PR
context chip (composer spec): `max-w-80 space-y-1 py-0.5`: state icon 14px + `Pull request #<n>` +
state label in the state tone (`text-xs font-medium`), the title, and `<head>` `ArrowRight` `<base>`
in `code` (`text-3xs text-secondary-label`, truncated).

---

## Reuse map

| T3UI module | Verdict |
| --- | --- |
| `crates/t3-logic/src/source_control.rs` (branch `tools-logic-land`) | Fits: quick action, menu items, disabled reasons, progress stages, default-ref copy, terminology, PR reference parsing. Add the `Running for …` elapsed formatter and the toast CTA mapping if missing. |
| `crates/t3-logic/src/project_scripts.rs` (branch `tools-logic-land`) | Fits: ids, `script.<id>.run`, primary script, settings patch shape, runtime env. Add t3.json import (`From t3.json`) and last-invoked persistence. |
| `crates/t3-app/src/header_tools/*` (branch `tools`, never compiled) | July visuals; wiring patterns only (the tools handoff notes `t3_ui::Dialog` hardcodes `max_w(512)`, so `max-w-xl` (576px) dialogs need a primitive change, and `TooltipExt` only places on top). |
| `t3-ui` Button/Group/Menu/Popover/Dialog/Switch/Checkbox/Textarea | Reuse; check the outline xs bevel and joined-group corners. `MenuItem` takes only an `IconName`: editor logos and provider logos need custom rows. |
| Icons | Need `CloudDownload`, `CloudUpload`, `GitCommit`, `GitBranchPlus`, `Ellipsis`, `FlaskConical`, `ListChecks`, `Wrench`, `Hammer`, `Bug`, `Play`, `Settings`, `Download`, `SquareArrowOutUpRight`, provider logos (GitHub, GitLab, Forgejo, Bitbucket, Azure DevOps) and editor logos (Cursor, Trae, Kiro, VS Code (+Insiders), VSCodium, Zed, Antigravity, JetBrains family, Finder). |

## Reference screenshots needed

Seeded repos are on `main` with uncommitted changes and no remote (tools handoff), so the default quick
action is `Commit` (changes, no upstream, no remote). Dark and light, 1440x900, right panel closed
unless noted.

1. `header-actions-wide`: `thread-aurora-tour` (scripts none, git `Commit`): `Add action` + `Commit`
   groups with labels.
2. `header-actions-narrow`: same with the right panel open (icon-only buttons, no separators).
3. `header-actions-collapsed`: shrink the window and open the panel so the header is < 512px;
   `More header actions` menu open.
4. `header-scripts-menu`: add a script `test` = `node --test` (Test icon, shortcut `⌘⇧T`); open the
   chevron menu and hover the row (edit button replaces the shortcut).
5. `header-script-editor`: the `Add Action` dialog, then the icon picker popover.
6. `header-git-menu`: chevron menu (Commit enabled, Publish repository... item since there is no
   remote).
7. `header-git-menu-disabled-reason`: needs a clean repo; hover a disabled item (side-left popover).
8. `header-commit-dialog`: click `Commit` (files list, Edit mode with checkboxes as a second capture).
9. `header-default-ref-dialog`: needs a repo with a remote (add a bare remote to the fixture) on
   `main` with commits ahead; click `Push` (default-ref dialog).
10. `header-git-progress-toast`: run a commit (loading toast `Generating commit message...`), then the
    success toast.
11. `header-publish-wizard`: quick action `Publish repository` on a committed repo without a remote.
12. `header-init-git`: a project that isn't a git repo (`Initialize Git`).
13. `file-tab-open-in`: a file tab subheader with the compact Open-in group and its menu.
14. `pr-checkout-dialog`: a local draft thread, branch picker -> PR reference (needs a repo with a
    GitHub remote and `gh` auth; optional).

## Missing client APIs

- `sourceControl.publishRepository` (publish wizard) and the source-control provider discovery list it
  reads (`SourceControlProviderDiscoveryItem {kind, label, status, installHint, auth}`); upstream
  `contracts:sourceControl.ts`, `rpc.ts:444,966`.
- Client-side: `editor.openFavorite` keybinding handling, `t3code:last-editor` preference,
  last-invoked script per project.
- Everything else is typed: `git.runStackedAction` (stream), `vcs.pull`, `vcs.refreshStatus`,
  `subscribeVcsStatus`, `vcs.init`, `git.resolvePullRequest`, `git.preparePullRequestThread`,
  `shell.openInEditor`, `server.updateSettings`, `server.upsertKeybinding`. `GitActionProgressEvent`
  covers `hook_output {text}`; `action_finished.result` is an untyped `Value` today
  (`crates/t3-protocol/src/vcs.rs:304-307`): type `GitRunStackedActionResult` (`branch`, `commit`,
  `push`, `pr`, `toast {title, description?, cta: none | open_pr {label, url} | run_action {label,
  action {kind}}}`) for the success toast.

## Open questions / risks

- The Open-in picker is hidden in the header by a constant; if the user's build flips it, the header
  gains a third group between scripts and git.
- Remote open modes (`local-exec`, `remote-links`, `remote-unavailable`) depend on desktop SSH probing
  (`remoteOpen.ts`); a native app needs its own probe or should treat every remote environment as
  `remote-unavailable`.
- The publish wizard is large and unported; only its strings are listed here.
- Toast styling, stacking (`stackedThreadToast`) and the 10s visible-time dismissal belong to the
  toast spec; the git flow depends on toasts that can update in place.
