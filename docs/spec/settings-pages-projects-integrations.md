# Settings pages: Project, Source Control, Integrations

Refreshed against fe7d3092c.

Source: `~/L-Projects/t3UI-refs/t3code-fork` @ `fe7d3092c`. Prefixes: `web/` = `apps/web/src/`,
`desk/` = `apps/desktop/src/`, `pkg/` = `packages/`, `cs/` = `web/components/settings/`.

Framework (layout, nav, search, scope, `SettingsRow`, confirm dialogs): see `settings.md`.
Shared pieces specified elsewhere:

- `ProjectDefaultsSettings category=…` (the new-thread defaults block at the top of all three
  pages): `settings-pages-general-appearance.md`.
- `DeviceHostsSettings` (bottom of Integrations > Devices) and `GitHubRoutingSettings` (rendered on
  Connections): `settings-pages-connections.md`.
- `ProviderModelPicker` (composer model picker, reused for the writer model): chat/composer spec.
- `ProjectFavicon`, `ProjectMonogram` glyphs: shell/sidebar spec.
- Command palette chrome (reused by the icon file picker): shell spec, command palette.

Desktop assumptions as in `settings.md`. Toasts use the shared toast (design-system); "stacked
toast" = `stackedThreadToast` layout.

---

## 1. Project (`/settings/projects`)

### 1.1 Reaching the page

- Nav item "Project" (`PanelsTopLeft`) is first in the nav and only appears while the scope search
  has `project` (settings.md §3.4).
- Deep links: `/settings/projects?project=<projectKey>[&machine=<envId>][&checkout=<physicalKey>]`.
  Legacy `/projects/<projectKey>` replaces to `/settings/projects?project=<projectKey>`
  (`web/routes/projects.$projectKey.tsx:3-17`). The shell's sidebar project menu and project links
  use this (shell spec).
- The page bypasses the generic scope boundary and does its own gating
  (`cs/ProjectsSettings.tsx:9-36`, settings.md §5.5):

| Scope | Body |
| --- | --- |
| `project` or `checkout`, or unavailable `project-missing`/`checkout-missing` while `project` is set | `ProjectSettingsPanel(projectKey=project, environmentId=machine ?? null, checkoutKey=checkout ?? null)` |
| other unavailable | `SettingsPageContainer` + `<p text-sm muted-foreground>{scope.message}</p>` |
| all / environment | `SettingsScopeNotice target="project"`: "Choose a project to manage its name, icon, checkouts and actions." with one outline button per project group (settings.md §5.4) |

The outer wrapper is `div.flex.min-h-0.flex-1.flex-col`.

### 1.2 `ProjectSettingsPanel` states (`cs/ProjectSettingsPanel.tsx:60-158`)

- `selected` = the group whose `projectKey` equals `project`. `members` = its member checkouts
  filtered by `machine` (environment id) and `checkout` (physical key).
- **Grouping change follow-up** (`:84-128`): the panel remembers the last rendered (key, machine,
  checkout, member physical keys). If the group key disappears (the user changed project grouping
  rules) while the same machine/checkout are selected, it finds the group that now contains any
  remembered member and **replace**-navigates to `?project=<newKey>` keeping `machine`/`checkout`.
- No group: centered (`flex-1`, items/justify center, p 32, `text-sm muted-foreground`):
  "Add a project from the sidebar to configure it here." when there are no groups at all, else
  "This project is no longer available.".
- Group but no members after filtering: `<p p-32 text-sm muted-foreground>` "This checkout is no
  longer available in the selected project and environment.".
- Else `ProjectDetail` keyed by `<projectKey>:<machine or all>:<checkout or all>` with
  `group.memberProjects = members` and `hasOtherMembers = members.length < all members`.

### 1.3 Layout (`ProjectDetail`, `:409-552`)

`SettingsPageContainer className="gap-6"`: same as settings.md §2.5 but the gap between children
is **24 px** (not 32). Children in order:

1. Scope sentence (settings.md §5.2).
2. Info alert (`Alert variant="info"`, `InfoIcon` aria-hidden): "Can't find a setting? Keep this
   project picked above and hop to any other settings page." Alert `info` variant: rounded-xl 14,
   1 px `info/32` border, bg `info/4`, px 14, py 12, `text-sm`, icon `info` color (design-system).
3. Section `id="project-overview"` title "Project" (`hideTitle`): Name row, Project icon row (§1.4).
4. `ProjectDefaultsSettings category="project"`: section "New threads" (`id="project-defaults"`) with only the Default model and workspace (New threads) rows (General spec).
5. Actions section (§1.5).
6. "Checkouts" section, only when the scoped group has more than one member (§1.6).
7. "Danger" section (§1.7).

Representative member (`:174-177`): the first member whose environment has a server config, else
the first member. It supplies the icon/favicon, the file picker's cwd, and the monogram name.

### 1.4 Project overview rows

**Name** (`:419-442`)

| Part | Value |
| --- | --- |
| title | "Name" |
| description | "The shared name for this project group in the sidebar and thread lists." |
| control | `Input size="sm"`, width 256 (`sm:w-64`; full width when narrow), `aria-label="Project name"`, uncontrolled `defaultValue = group.displayName` (re-keyed when the display name changes) |
| commit | on blur; Enter blurs. Only if the user typed since the last commit and the trimmed value differs from any member's title (`ProjectSettingsPanel.logic.ts:1-7`). |
| validation | empty after trim → warning toast "Project title cannot be empty" (no description); the input keeps the empty text until re-keyed |
| write | for every member: `project.meta.update { projectId, title }` on that member's environment, sequentially |

**Project icon** (`:443-490`)

| Part | Value |
| --- | --- |
| title | "Project icon" |
| description | lucide override: `"<icon name> · <color>"` (e.g. `folder-code · blue`); monogram: `"<text> · <color>"`; emoji: the emoji; else the favicon path; else "Automatic" |
| reset | `SettingResetButton` label "project icon" (aria "Reset project icon to default", tooltip "Reset to default"), shown when any member has `faviconPath` or `projectIcon`; disabled while saving; writes `{ faviconPath: null, projectIcon: null }` |
| control | flex gap 8: `ProjectFavicon` of the representative at 24 px; `Button size="sm" variant="outline"` "Choose icon" (aria "Choose a project icon") → icon picker (§1.8); `Button size="sm" variant="outline"` "Choose file" (aria "Choose a project icon file") → file picker (§1.9). Both disabled while saving. |
| write | every member: `project.meta.update { projectId, faviconPath, projectIcon }`; picking an icon sets `faviconPath: null`; picking a file sets `projectIcon: null` |

Group-wide writes (`updateAllMembers`, `:209-253`):

- If any member's environment is not connected (or has no server config): error toast (stacked)
  with the failure title and description `"Connect <member env label or 'the selected environment'> and try again."`; nothing is written.
- Writes run member by member. The first failure stops the loop and shows an error toast (stacked):
  title = the failure title, plus `" on <env label or 'the current environment'>"` when the group
  has more than one member; description = the error message or "An error occurred.". Earlier
  members keep the change.
- Failure titles: "Failed to rename project", "Failed to update project icon".

### 1.5 Actions (`cs/ProjectActionsSettings.tsx`, `cs/ProjectActionsList.tsx`)

Project actions (scripts) for the selected project on every selected environment. Section
`id="project-actions"` title "Actions" (also the search anchor `project-actions`).

Data: `scripts` = representative target's effective `defaultProjectScripts` (project override,
else the environment list). `keybindings` = representative environment's resolved keybindings
(`ServerConfig.keybindings`) or the built-in defaults. `mixed` = any target's effective list
differs (JSON compare).

Write targets (`:66-93`): one per connected target with a server config. For project targets, an
environment without capability `projectSettingsOverrides` is skipped (older servers ignore the
override record).

**Header row** (`SettingsRow`, `serverScoped`, `settingKeys=["defaultProjectScripts"]`, `mixed`):

| Part | Value |
| --- | --- |
| title | "Actions" |
| description | "Commands that run in this project's checkout or its worktree, with optional shortcuts." |
| reset override | at project scope the row's reset (settings.md §6.3) calls `persist(() => null)`: removes `defaultProjectScripts` from each project override entry |
| control | flex-wrap, gap 6: optional "Import scripts" menu, then `Button size="xs" variant="outline"` with `Plus` 14 + "Add action" (disabled while saving or with no targets) → editor dialog in add mode |

"Import scripts" (only when the representative checkout's `t3.json` declares scripts not already
in the list, matched by equal command or case-insensitive equal name):

- Trigger `Button size="xs" variant="ghost" id="import-scripts"`: "Import scripts" + `ChevronDown`
  14; disabled while saving.
- Menu (align end): group label "Import from t3.json", then `<p>` px 8, pb 8, `text-sm`
  `muted-foreground`: "Add actions declared by this checkout without editing them first.",
  separator, then one item per importable script: script icon 16 + two lines (name medium truncate;
  command `font-mono muted-foreground` truncate).
- Click imports directly (no dialog): name, command, icon (default `play`), run on worktree
  create, wait-for-setup = `runOnWorktreeCreate && async === false`, no keybinding, preview URL and
  auto-open (only with a URL). On failure the editor dialog opens prefilled with the error
  message (or "Failed to import action.").

Below the header row:

- `mixed` → `SettingsRow` title "Different actions across environments", description "Choose one
  environment to edit its list. Adding an action here adds it on every selected environment.".
- Else the list (`ProjectActionsList`):
  - Empty: `<p>` px 16, py 8, `text-sm` (`text-base` when narrow) `muted-foreground`
    "No actions configured.".
  - One `SettingsRow` per script, class `group py-2` (py 8):
    - title (flex, gap 8): script icon 16 `muted-foreground`; name (truncate); badge "setup" when
      `runOnWorktreeCreate`; badge "preview · desktop only" when `previewUrl`. Badges:
      `rounded-sm` 6, 1 px `border/60`, px 6, py 1, `text-2xs`, normal weight, `muted-foreground`.
    - description: `<code>` mono, truncate: the command.
    - control: the bound shortcut label (`text-xs muted-foreground`, from keybindings for command
      `script.<id>.run`), then an edit button (`icon-xs` ghost-muted, `Settings` 14,
      aria `"Edit <name>"`) that is opacity 0 until the row is hovered or focus-within.
- When the representative `t3.json` exists but fails to parse: `SettingsRow` (warning text color)
  title "t3.json is invalid", description "A t3.json exists in this checkout but fails to parse, so
  every action and icon it declares is ignored. Check the JSON syntax and icon values.".

Script icons (`web/components/projectScriptEditor.tsx:59-81`): `play` → `Play`, `test` →
`FlaskConical`, `lint` → `ListChecks`, `configure` → `Wrench`, `build` → `Hammer`, `debug` →
`Bug`.

**Action editor dialog** (`ProjectScriptEditorDialog`, `web/components/projectScriptEditor.tsx:142-481`;
also used by the chat header scripts menu):

- `Dialog`, title "Add Action" / "Edit Action", description "Actions are project-scoped commands
  you can run from the top bar or keybindings.".
- Form (`fieldset` space-y 16, disabled while saving):
  1. Label "Name" (space-y 6). Row (gap 8): icon button (`outline`, 36x36, aria "Choose icon",
     current icon 18 px) opening a popover (align start) with a 3-column grid (gap 8) of icon tiles
     (flex col, gap 8, `rounded-md`, 1 px border, px 8, py 8, `text-xs`: icon 16 + label Play /
     Test / Lint / Configure / Build / Debug; selected: `primary/70` border, `primary/10` bg (dark
     adds 1 px `primary/30` ring); else `border/70`, hover `accent/60`, dark bg white 3.5%,
     dark border transparent); picking closes the popover. Then `Input id="script-name"`
     autofocus, placeholder "Test".
  2. Label "Keybinding". `Input` read-only, placeholder "Press shortcut". Keydown: Tab passes
     through; everything else is prevented; Backspace/Delete clears; otherwise the key event is
     converted with the keybindings editor's `keybindingFromKeyboardEvent` (Keybindings spec).
     Help `text-xs muted-foreground`: "Press a shortcut. Use `Backspace` to clear. Shortcuts are
     environment-wide. Projects using the same action share its shortcut."
  3. Label "Command". `Textarea`, placeholder "bun test".
  4. Label "Preview URL (optional)". `Input`, placeholder "http://localhost:5173". Help: "Open
     this URL in the in-app preview when this action runs."
  5. Three switch rows (label element: flex, justify-between, gap 12, `rounded-md`, 1 px
     `border/70` (dark: transparent border, bg white 3.5%), px 12, py 8, `text-sm`):
     - "Run automatically on worktree creation"
     - "Wait for it to finish before the agent starts" (disabled and opacity .6 unless the first
       is on)
     - "Open preview automatically when this action runs" (disabled and opacity .6 while the
       preview URL is blank)
  6. Validation error `<p text-sm destructive>`.
- Footer (bare): edit mode only, `Delete` (`destructive-outline`, `mr-auto`) → delete confirm;
  `Cancel` (outline); submit (default): "Saving…" while saving, else "Save changes" (edit) /
  "Save action" (add).
- Validation, in order: trimmed name empty → "Name is required."; trimmed command empty →
  "Command is required."; keybinding that does not decode as a `KeybindingRule` for
  `script.<id>.run` → "Invalid keybinding."; save failure → error message or "Failed to save
  action.".
- New id (`web/projectScripts.ts:37-90`): lowercase, non-`[a-z0-9]` runs → `-`, trim dashes,
  max 24 chars (`MAX_SCRIPT_ID_LENGTH`), empty → `script`; on collision with any existing script
  id (all projects, all targets' defaults and overrides) append `-2`, `-3`, ... (truncating the
  base to fit 24).
- Saved script: `{ id, name, command, icon, runOnWorktreeCreate, async: false (only when
  run-on-create and wait), previewUrl + autoOpenPreview (only with a URL) }`. Saving a script with
  run-on-create turns `runOnWorktreeCreate` off on every other script in the list (one setup
  action per project).
- Delete confirm (`AlertDialog`): title `Delete action "<name>"?`, description "This action
  cannot be undone.", buttons `Cancel` (outline) and `Delete action` (destructive). Confirm
  closes both dialogs and removes the script.

**Persistence** (`cs/useProjectScriptSettings.ts:52-192`):

- Re-entry guard: while saving, or with no targets, an error toast title "Actions not saved",
  description "No available machine, or another action change is saving.".
- Per target, sequentially: `server.updateSettings { patch }` where patch is
  `{ projectSettingsOverrides: { [projectId]: { ...currentEntry, defaultProjectScripts: next } } }`
  for project targets (or the entry with the key cleared when resetting), else
  `{ defaultProjectScripts: next }`.
- Then (desktop only) keybindings for the changed script id(s): `server.upsertKeybinding
  { key, command: "script.<id>.run", replace?: previous }` when a key is set (with `replace` when
  it changed), or `server.removeKeybinding` of the previous rule when cleared, unless another
  project/default list on that environment still uses the id.
- Failure → error toast "Failed to save project actions" / error message or "An error occurred.",
  stop. Failures from the RPC layer also report through the generic atom-command toast labels
  "project actions update", "action shortcut update", "action shortcut removal".

### 1.6 Checkouts (`:387-407`)

Only when the scoped group has more than one member. `SettingsSection title="Checkouts"`, one
`SettingsRow` per member: title = environment label (or "Environment"), description = workspace
root, control `Button size="sm" variant="outline"` "Remove" (aria `"Remove checkout <root>"`) →
remove flow for that member (§1.7).

### 1.7 Danger (`:495-526`) and removal (`:298-385`)

`SettingsSection title="Danger"`, one row:

| Case | Title | Description | Button (`size="sm" variant="destructive-outline"`, `Trash2` icon) |
| --- | --- | --- | --- |
| scope narrowed to some members (`hasOtherMembers`) | "Remove checkout" | "Deletes the selected machine's checkout entries and their threads. Other machines and files on disk are not touched." | "Remove checkout" |
| whole group, more than one member | "Remove this project everywhere" | `"Deletes all <n> checkout entries and their threads on every machine. Files on disk are not touched."` | "Remove all entries" |
| whole group, one member | "Remove project" | "Deletes the project entry and its threads. Files on disk are not touched." | "Remove project" |

Removal confirm (settings.md §6.11 confirm dialog, `variant: "destructive"`). Message lines:

1. `Remove <kind> "<label>" and delete its <n> thread(s)?` when the members have threads, else
   `Remove <kind> "<label>"?`. `kind` = "checkout" when narrowed or removing a subset, else
   "project". `label` = the single member's title, else the group display name.
2. Single member: `Path: <workspaceRoot>` and, if known, `Environment: <env label>`. Several:
   `This removes <n> grouped project entries.`
3. With threads: "This permanently clears conversation history for those threads and any archived
   threads." Without: "This permanently clears any archived conversation history."
4. Whole group and not narrowed: "This removes only the project entries, not the files on disk."
   else "Other entries in this grouped project are unaffected."
5. "This action cannot be undone."

So the dialog title is line 1; lines 2-5 form the description.

On confirm, per member sequentially: `project.delete { projectId, force: true }` on its
environment; failure → stacked error toast `Failed to remove "<member title>"` and stop. After each
success: release composer draft uploads for the project's threads, clear the project's draft
thread. After removing the whole group (not narrowed): replace-navigate to `/`.

### 1.8 Icon picker dialog (`cs/ProjectIconPickerDialog.tsx`)

`Dialog`, popup width 512 (`sm:w-[32rem]`). Title "Choose project icon", description "Choose an
icon, emoji, or monogram.". Panel (flex col):

- Segmented `ToggleGroup` aria "Icon type": "Icons" | "Emoji" | "Monogram". Initial = current
  override kind, else Icons.
- Color (Icons and Monogram only): label "Color" (`text-xs` medium `muted-foreground`, mb 8), then
  a wrap row (gap 6, `role=group` aria "Icon color") of 18 swatch buttons: 24 px circle, 1 px
  transparent border (selected: `foreground/64`), inner 16 px dot in `<color>-500`. Order and
  labels: Gray, Red, Orange, Amber, Yellow, Lime, Green, Emerald, Teal, Cyan, Sky, Blue, Indigo,
  Violet, Purple, Fuchsia, Pink, Rose (`web/projectIconColors.ts`). Initial color = current
  override color, else the automatic identity color (hash of the project name,
  `web/projectIdentity.ts:29-36`).
- Icons: search `Input type="search"` aria "Search Lucide icons", placeholder "Search all Lucide
  icons". Grid (max-h 256 scroll with fade; 10 columns at `sm`, gap 4, p 2) of square buttons
  (`rounded-md`, transparent border, hover `accent`; selected: `border` + `accent` bg; glyph 20 px
  in the selected color's text class (`<color>-600`, dark `<color>-400`)). Empty query shows 24
  popular icons: folder-code, code-2, terminal, globe-2, server, database, bot, sparkles,
  smartphone, monitor, cloud-cog, package, book-open, flask-conical, shield-check, rocket,
  gamepad-2, music, image, shopping-bag, git-branch, workflow, wrench, layers-3. A query
  (lowercased, whitespace → `-`) matches lucide names containing it, first 60. No match:
  "No icons found." (py 32, centered, `text-sm muted-foreground`). Default selection
  `folder-code`. Aria labels title-case the name ("Folder Code").
- Monogram: row (gap 16, py 8): 48 px `ProjectMonogram` preview (shows the automatic monogram while
  invalid); label "Letters" (`text-sm` medium), `Input` (initial: current text or the automatic
  monogram), hint "One or two letters or numbers." (`text-xs muted-foreground`). Value is NFKC,
  trimmed, uppercased; valid when it matches `ProjectMonogramText` (1-32 chars, starts with a
  letter/number). Invalid → `aria-invalid`, Save disabled.
- Emoji: grid (same as icons, glyph `text-xl`) of 30 emojis (💻 Computer, 🛠️ Tools, 🚀 Rocket,
  🤖 Robot, ✨ Sparkles, ⚡ Lightning, 🌐 Web, 📱 Mobile, 🖥️ Desktop, ⌨️ Keyboard, ⚙️ Gear,
  🗄️ Database, ☁️ Cloud, 📦 Package, 📚 Books, 🧪 Test tube, 🔒 Lock, 🎮 Game, 🎵 Music,
  🎬 Movie, 🖼️ Picture, 🛍️ Shopping, 🔥 Fire, 💡 Idea, 🧩 Puzzle, 📊 Chart, 🧠 Brain,
  🦄 Unicorn, 🐙 Octopus, 🌱 Seedling); then label "Or paste any emoji" and `Input` aria "Custom
  emoji", placeholder "Paste an emoji": the first grapheme that is an extended pictographic,
  flag, or keycap becomes the selection. Default 💻.
- Footer: `Cancel` (outline), `Save icon` (default; disabled for an invalid monogram). Save emits
  `{kind:"lucide", name, color}` / `{kind:"emoji", emoji}` / `{kind:"monogram", text, color}` and
  closes.

### 1.9 Icon file picker (`cs/ProjectFaviconPickerDialog.tsx`)

A command-palette-styled dialog (`CommandDialog`, palette content `panelSize="tall-list"`,
auto-highlight first item; shell spec for chrome):

- aria "Choose project icon"; input placeholder "Search image files…"; footer action label
  "Select icon", escape label "Close".
- Results: one group labeled with the project name; each item = file icon (Pierre file icon) +
  file name + path as description. Query: `projects.searchEntries { cwd, query, kind: "file",
  imageOnly: true }` on the representative's environment, limit 200, empty query allowed.
- Empty states: the query error, else "Searching project files…" (query) / "Indexing project
  files…" (no query) while pending, else "No matching image files." / "No image files found.".
- Selecting closes the dialog and sets `faviconPath = <path>` (relative path) on every member.
- Trailing footer action `Open in Finder` (macOS; "File Explorer" on Windows, "Files" elsewhere),
  only when every member is on the primary environment and (non-Windows, or the root is a Windows
  absolute path). Opens the native file chooser (`desktopBridge.pickProjectFavicon(root)`); a
  chosen path closes the dialog and is saved; failure → error toast "Could not open image picker"
  / message or "An error occurred.". Disabled while the chooser is open.

### 1.10 Scope behavior summary

| Element | all/environment | project | checkout |
| --- | --- | --- | --- |
| page | scope notice | panel for every member | panel for one member |
| Name / icon | n/a | writes all members of the scoped group | writes that member only |
| Actions | n/a (page not shown) | project override on each member environment | same, one member |
| Checkouts | n/a | listed when >1 member | hidden (1 member) |
| Danger | n/a | "Remove this project everywhere" / "Remove project", or "Remove checkout" when `machine` narrows | "Remove checkout" |

---

## 2. Source Control (`/settings/source-control`)

`SourceControlSettingsPanel` (`cs/SourceControlSettings.tsx:509-623`). `SettingsPageContainer`
(readable, gap 32). Category scope `environment-defaults` (settings.md §4.4) only matters for
search landings; the page renders at every scope. Order:

1. Scope sentence.
2. `ProjectDefaultsSettings category="source-control"`: section "Repositories" (`id="source-control-defaults"`) with "Automatically pull" (`id="automatic-pull"`) and "Default merge method" (General spec).
3. Server environment discovery (§2.1).
4. Text generation (§2.3).

### 2.1 Discovery

Discovery scans one machine: the representative environment (settings.md §5.1) if connected.
`server.discoverSourceControl {}` → `SourceControlDiscoveryResult { versionControlSystems[],
sourceControlProviders[] }` (`pkg/contracts/src/rpc.ts:641-645`, `sourceControl.ts:121-156`).
Fields: `kind`, `label`, `executable?`, `status` ("available" or not), `version` (Option),
`installHint`, `detail` (Option); VCS items add `implemented`; provider items add
`auth { status: authenticated|unauthenticated|unknown, account, host, detail }` (Options encode
as `{"_tag":"Some","value":…}` / `{"_tag":"None"}`, protocol.md). Cached per environment; "Scan"
re-runs it.

`environmentSuffix` = `" · <representative label>"` when the scope is not a single environment and
more than one environment is connected.

| State | Render |
| --- | --- |
| no connected representative | Section `id="source-control"` title "Server environment": `<p>` px 16, py 12, `text-sm muted-foreground` "Connect an environment to inspect its version control tools and hosting integrations." |
| first scan pending (no data yet) | two skeleton sections: "Version Control<suffix>" (with the scan button) and "Source Control Providers"; each has 2 rows (§2.2 skeleton) |
| result with items | "Version Control<suffix>" section (when VCS items exist; `id="source-control"`, header action = scan button) with one row per VCS; then "Source Control Providers" (suffix and scan button only when there were no VCS items, and then it takes `id="source-control"`) with one row per provider |
| no items | Section `id="source-control"` title "Server environment" containing an `Empty`: icon = pull-request glyph; title "Could not scan the server environment" (error) or "Nothing detected yet"; description = the error, or "Install Git on the server, add optional hosting integrations or credentials your workspace needs, then rescan."; content `Button size="sm" variant="outline"` with `RefreshIcon` (14, spinning while pending) + "Scan" (disabled while pending) |

Scan button: `Button size="icon-xs" variant="ghost-muted"` aria "Rescan server environment",
`RefreshIcon` spinning while pending, disabled while pending, tooltip (top) "Rescan Git and
hosting integrations".

`RefreshIcon` (`web/components/ui/refresh-icon.tsx`): lucide `RefreshCw`; while refreshing it
spins with `animate-spin` (360° per 1 s, linear, infinite), paused when offscreen
(`visible-animate-spin`). **Continuously repainting while a scan runs**; native: spin only while
the request is in flight and the icon is visible, or use a static icon + disabled state.

### 2.2 Discovery rows (`DiscoveryItemRow`, `:264-351`)

```
div (first/last rounded-xl corners; hover bg muted/20 with color transition; opacity .8 when "not ready")
├ div px 16 py 12 → flex row (items-center, justify-between, gap 12)
│ ├ text (min-w 0, flex-1, space-y 4)
│ │  ├ line (flex-wrap, gap 8): mark · label (text-sm medium foreground, truncate)
│ │  │   · version (<code> text-xs muted-foreground) · badges
│ │  └ summary <p> (flex-wrap, gap-x 4, text-xs leading-normal muted-foreground/80)
│ └ controls (gap 8): chevron button (when the row has details) · availability Switch (disabled)
└ Collapsible details: px 16, pb 16, pt 4
```

- Mark (`:178-204`): 20 px box with the brand icon at 18 px (`foreground/80`): GitHub, GitLab,
  Forgejo, Azure DevOps, Bitbucket, Git, Jujutsu (`web/components/Icons.tsx`); a status dot 8 px
  at top-left (-2, -2) with a 2 px `background` ring. Unknown kinds: just an 8 px dot. Dot color:
  not ready (VCS `implemented=false`) `muted-foreground/35`; status not available `warning`;
  provider not authenticated `warning`; else `success`.
- Badges (`Badge size="sm"`): not-ready VCS → `warning` "Coming Soon"; provider unauthenticated →
  `warning` "Not authenticated".
- Summary copy (`:206-262`), first match:

| Condition | Text |
| --- | --- |
| VCS not implemented | `Support for <label> is coming soon.` |
| status not available | `Not available on this server: <installHint>` |
| provider authenticated | "Authenticated" + (account? " as " + redacted account) |
| provider without executable, unauthenticated | `Available. <installHint>` |
| provider unauthenticated | `<label> is not authenticated on this server. Sign in or configure credentials using the <executable> tool on the server host to enable change request features.` (`<executable>` in `<code>` `rounded` bg `muted` px 4 py 1 `text-2xs`) |
| provider auth unknown | `Could not verify <label>. <auth.detail or installHint>` |
| otherwise | "Available" |

  Redacted account: `RedactedSensitiveText` (settings.md §6.10), aria "Toggle source control
  account visibility", tooltips "Click to reveal account" / "Click to hide account".
- Availability switch: checked when (provider) available and authenticated, or (VCS) available and
  implemented; always disabled; aria `"<label> availability"`; hidden for not-ready VCS.
- Chevron: `Button size="icon-xs" variant="ghost-muted"`, `ChevronDown` 14 rotating 180° when
  open (default transition), aria `"Toggle <label> details"`, `aria-expanded`. Details exist for
  `git` (fetch interval) and `bitbucket` (credentials).
- A search landing on `git-fetch-interval` / `bitbucket-credentials` expands the matching row
  (`:282-290`).
- Skeleton row (`:433-469`): same paddings; skeleton 18 px square + 8 px pill dot, 112x16 pill,
  56x20 pill; 12 px pill line (max-w 320); right: 28 px square and 36x20 pill.

**Git > Automatic fetch interval** (`GitFetchIntervalSettings`, `:353-431`), wrapped in
`SettingsSearchTarget id="git-fetch-interval"` (grid gap 12):

- Left: title line (gap 4): "Git fetch interval" (`text-xs` medium `foreground`), `PolicyTooltip`
  "This interval is configured for Git only. The shared Background activity policy still decides
  whether Git refreshes may run when the timer fires. Custom intervals appear as Advanced in
  General settings.", and a 20 px slot with `SettingResetButton` (label "fetch interval") visible
  only when the value differs from the base profile preset (fades via opacity). Description
  (`max-w-2xl`, `text-xs` leading-relaxed `muted-foreground`): "Refresh remote branches in the
  background. Set to 0 to avoid automatic Git prompts."
- Right (gap 8): `NumberField size="sm"` width 128, min 0, step 5, with decrement/increment
  (aria "Decrease fetch interval", "Automatic Git fetch interval in seconds", "Increase fetch
  interval") + "seconds" (`text-xs muted-foreground`).
- Value = resolved `backgroundActivity.automaticGitFetchInterval` in whole seconds. Presets:
  performance 15 s, balanced 30 s (default), battery-saver 0 s
  (`pkg/shared/src/backgroundActivitySettings.ts:24-60`).
- Write (scoped, settings.md §5.6): `backgroundActivity = { schemaVersion: 1, profile: "custom",
  baseProfile: <current base>, overrides: { ...overrides, automaticGitFetchInterval: ms } }`
  (reset removes the override key). Not project-scoped: at project scope the write is refused with
  the "environment-wide" warning toast.

**Bitbucket credentials** (`cs/BitbucketCredentialsSettings.tsx`), wrapped in
`SettingsSearchTarget id="bitbucket-credentials"`, re-keyed per environment. Form (grid gap 16,
fieldset disabled while saving):

- Segmented `ToggleGroup` aria "Bitbucket sign-in method": "Access token" | "API token". Initial:
  the saved method (access token if `accessToken` set; API token if email and apiToken set), else
  "Access token".
- Help `<p>` (`max-w-2xl`, `text-xs` leading-relaxed `muted-foreground`) + `InlineButton` link with
  `ExternalLink` 12:
  - Access token: "Scoped to one repository, project, or workspace. Create it in that item's
    Bitbucket settings." + "Learn more" → `https://support.atlassian.com/bitbucket-cloud/docs/access-tokens/`
  - API token: "Uses your Atlassian account, so it reaches every repository you can. Give it read
    and write access to repositories and pull requests, and read:user:bitbucket." + "Create an API
    token" → `https://id.atlassian.com/manage-profile/security/api-tokens`
- Fields (label + input, grid gap 6, `Input size="sm"`, autocomplete off):
  - Access token: "Access token" password input.
  - API token: "Atlassian account email" (`type=email`, placeholder "you@example.com", prefilled
    with the saved email) and "API token" password input.
  - Token inputs never show the saved secret; placeholder "Stored secret, enter a new value to
    replace" when that method is saved, else "Not set".
- Footer row (justify-between, gap 12): note (`text-xs muted-foreground`): no saved credential →
  "Without a saved token, the server falls back to its T3CODE_BITBUCKET_* environment variables.";
  switching away from the saved method → `"Saving replaces your <access token|api token>."`.
  Buttons (`size="xs"`): "Remove" (outline; only when a credential is saved; writes all three
  empty) and "Save" (submit).
- Save enabled when: access token typed (access method); or (API method) email non-empty and a
  new or saved API token, and something changed (new token, changed email, or switching method).
  Saving one method blanks the other's fields. Write: `server.updateSettings { patch: { bitbucket:
  { accessToken, email, apiToken } } }` directly on the representative environment (not scoped);
  an unchanged saved API token is resent as its redacted value, which the server keeps. Success
  clears drafts and rescans discovery. Failure → generic atom-command toast ("save Bitbucket
  credentials").

### 2.3 Text generation (`cs/SourceControlWritingSettings.tsx`)

`SettingsSection id="source-control-text-generation" title="Text generation"`. Values come from
the scoped settings (representative target); mixed is computed per field across targets.

**Source control writing style** (`id="source-control-writing-style"`, `serverScoped`,
`settingKeys=["sourceControlWritingStyle"]`, mixed when `mode` or `customInstructions` differ):

| Mode | Label | Row description |
| --- | --- | --- |
| `repo_conventions` (default) | Repository conventions | "In each project, matches recent change descriptions and change request titles." |
| `conventional_commits` | Conventional Commits | "Use Conventional Commit prefixes and keep change request text concise." |
| `custom` | Custom instructions | "Use your instructions for change descriptions and change requests in every project." |

- Control: `Select` trigger `size="sm"` width 224 (`sm:w-56`), aria "Source control writing
  style", shows the label or "Mixed"; popup align end, items without indicators.
- Change writes `{ sourceControlWritingStyle: { mode, customInstructions: <current textarea text
  trimmed, when the textarea is mounted> } }`.
- Reset (when mixed or mode/instructions differ from defaults): writes `{ mode:
  "repo_conventions", customInstructions: "" }`.
- Children (below the grid):
  - Mixed: (mt 12, max-w 672, space-y 8, pb 14) button "Write custom instructions for all"
    (`sm` outline) → reveals a 4-row `Textarea` (aria "Custom source control instructions for all
    selected environments", placeholder "Write the instructions each selected environment should
    use.") and "Apply instructions to all" (`sm` outline; disabled until typed) which writes
    `{ mode: "custom", customInstructions: <trimmed> }`.
  - Custom mode: (mt 12, max-w 672, pb 14) 4-row `Textarea` (aria "Custom source control writing
    instructions", placeholder "Keep titles concise. Use short bullet points in descriptions.",
    uncontrolled, re-keyed on the saved text); on blur writes `{ customInstructions }` when the
    trimmed text changed.

**Follow change request templates** (`id="follow-change-request-templates"`, same keys):
description "Use the repository's template for change request descriptions when available.";
control `Switch` (mixed-aware) aria "Follow change request templates"; default on; reset when
mixed or off → writes `{ followChangeRequestTemplates: true }`.

**Source control writer model** (`id="source-control-writer-model"`, `serverScoped`,
`settingKeys=["sourceControlWriterModelSelection"]`): description "Model for source control text
and branch or bookmark names. Off uses the environment's text generation model."

- No connected targets: control is the text "Connect an environment to choose its source control
  writer model." (`text-sm muted-foreground`).
- Else (flex-wrap, justify-end, gap 8):
  - When a dedicated model is set but the default text-generation instance is not enabled and
    available: "No text generation providers available." (`text-sm muted-foreground`).
  - When a dedicated model is set and usable: `ProviderModelPicker` (composer picker, trigger
    class `min-w-0 max-w-none shrink-0`, aria "Source control writer model", label "Mixed" when
    targets differ; providers limited to those with `supportsTextGeneration !== false`;
    "Set up" links go to `/settings/providers?environmentId=…&instanceId=…`). Choosing a model
    that is unusable on any selected target (instance disabled/unavailable, different driver, or
    model missing) shows an error toast "Source control writer model not saved" / `This model is
    unavailable on <env label>. Select that environment to choose its model separately.`
    (`cs/useScopedModelAvailability.ts`); else writes `{ sourceControlWriterModelSelection:
    { instanceId, model } }`.
  - `Switch` aria "Use a separate source control writer model": on = a dedicated model is set.
    Turning on copies the resolved text-generation selection (instance, model, options); off
    writes `null`. Disabled when off and no usable default instance exists.

---

## 3. Integrations (`/settings/integrations`)

`IntegrationsSettingsPanel` (`cs/IntegrationsSettings.tsx:1428-1461`). `SettingsPageContainer`
(readable). Order:

1. Scope sentence.
2. `ProjectDefaultsSettings category="integrations"`: a section titled "Browser"
   (`id="browser-access"`) holding "Agent browser access" (`id="agent-browser-access"`; General
   spec). Note the page then has **two sections titled "Browser"** (this server-scoped one and the
   device-local one below).
3. "Browser" section (`id="browser"`), device-local client settings (§3.1). In a non-desktop
   build every row is wrapped in `SettingsUnavailableGroup` "Only available in the desktop app."
   and disabled; the native app is always "desktop", so render them enabled.
4. "Devices" section (`id="devices"`) (§3.3).

Browser rows write client settings through `useUpdatePrimarySettings` (client keys go to
`client-settings.json`; settings.md §5.7-5.8). They ignore the scope.

### 3.1 Browser rows (in order)

**Browser profiles** (`id="browser-profiles"`, `:907-1426`)

- Description: "Profiles separate cookies and logins. Incognito data is cleared when the app
  closes."
- Control: `Menu` trigger `Button size="sm" variant="outline"` `Plus` + "Add profile" (disabled
  until client settings hydrate, or while an import runs). Opening the menu refreshes the import
  sources (`desktopBridge.preview.listBrowserImportSources()`), keeping the cached list visible.
  Menu (align end):
  - "Blank profile" (disabled at the 24-profile cap or before hydration) → creates
    `{ id: "profile-<uuid>", name: "New profile" (or "New profile 2", 3, … if taken), kind:
    "persistent" }`.
  - At the cap: disabled item "You’ve reached the profile limit".
  - Separator, group label "Import from":
    - sources not yet loaded: disabled "Looking for browsers…"
    - none importable: disabled "No supported browsers found"
    - else one item per source (name), excluding `notInstalled` and `unsupportedPlatform`; disabled
      without a primary environment; click opens the import wizard (§3.2) targeting the primary
      environment. Without a primary environment also a disabled "Connect to an environment to
      import cookies".
- Children: a bordered list (mt 8, mb 8, `rounded-lg` 10, 1 px `border/60`, overflow hidden).
  Rows: Default (built-in), then user profiles (Incognito is never listed). Each row: flex,
  items-center, gap 12, px 12, py 8, top border `border/60` after the first.
  - Name: built-ins as text (`text-sm foreground`, opacity .64 while writes are disabled); user
    profiles as `DraftInput` (native input, `size="sm"`, full width up to 224, aria `"Rename
    <name>"`, max 48 chars) committing on blur/Enter: trimmed, sliced to 48, ignored if empty.
  - `Badge` "Default" on the default profile (stored `browserDefaultProfileId`, falling back to
    `default` when it names an unlisted profile).
  - Row menu: `Button size="icon-xs" variant="ghost-muted"` `MoreVertical`, aria
    `"<name> options"`. Items: "Set as default" (disabled when already default);
    "Clear cookies and cache" (clears that profile's partition on every environment; success toast
    `Cleared <name>'s cookies and cache`; failure `Could not clear <name>'s data`, or with no
    environment yet the description "You're not connected to a server yet."); user profiles only:
    destructive "Remove profile and data" → confirm. When removal is unavailable: separator +
    disabled "Connect to an environment to clear profile data" (or "Checking environments…").
- Remove confirm (`AlertDialog`): title `Remove “<name>”?` (curly quotes), description "Its
  cookies and logins are deleted. Tabs already open in this profile stay open until you close
  them."; error line (`text-sm destructive`) "Profile data could not be deleted. Try again." or
  "Connect to an environment before removing this profile."; when unavailable also "Connect to an
  environment to remove this profile and its data." (`text-sm muted-foreground`). Footer: `Cancel`
  (outline) and destructive "Remove profile" ("Removing…" in flight). Removal clears cookies and
  cache on every environment first, then drops the profile and resets the default to `default` if
  it was the default.
- Keys: `ClientSettings.browserProfiles` (default `[]`, max 24), `browserDefaultProfileId`
  (default `"default"`).

**Default browser viewport** (`id="browser-default-viewport"`, `:232-384`)

- Description: "Tab size for you and agents. Fill fits the panel; other sizes show the device
  toolbar."
- Reset when the viewport kind is not `fill` → `{ _tag: "fill" }` (default).
- Control (flex-wrap, justify-end, gap 8): `Select` trigger `size="sm"` width 176 (`sm:w-44`),
  aria "Default browser viewport". Items: "Fill panel", "Responsive", group "Standard": one item
  per preset with label and a right-aligned `text-xs tabular-nums muted-foreground` detail (gap
  20): iPhone SE 375 × 667, iPhone XR 414 × 896, iPhone 12 Pro 390 × 844, iPhone 14 Pro Max
  430 × 932, Pixel 7 412 × 915, Samsung Galaxy S8+ 360 × 740, Samsung Galaxy S20 Ultra 412 × 915,
  iPad Mini 768 × 1024, iPad Air 820 × 1180, iPad Pro 1024 × 1366, Surface Pro 7 912 × 1368,
  Surface Duo 540 × 720, Galaxy Z Fold 5 344 × 882, Asus Zenbook Fold 853 × 1280, Samsung Galaxy
  A51/71 412 × 914, Nest Hub 1024 × 600, Nest Hub Max 1280 × 800 (`pkg/shared/src/previewViewport.ts`).
  Trigger label: "Fill panel", the preset label, or "Responsive".
- Choosing: Fill → `{_tag:"fill"}`; Responsive → `{_tag:"freeform", width, height}` (current
  size, else 1280 × 800); preset → `{_tag:"preset", presetId, width, height}`.
- When not Fill: two `NumberField size="sm"` (width 80, no digit grouping, min 240, max 3840; aria
  "Default viewport width"/"…height") separated by "×" (`text-xs muted-foreground`), committing on
  blur only (valid integer in range, area ≤ 3840 × 2160, changed) as `freeform`; and a rotate
  button (`icon-sm` ghost-muted, screen-rotation icon, aria `Rotate to landscape|portrait`,
  tooltip "Rotate") that swaps width and height (keeping a preset id).

**Default browser zoom** (`id="browser-default-zoom"`): description "Page zoom applied to new
browser tabs."; `Select` (`sm`, width 160, aria "Default browser zoom", no indicators) with 25%,
33%, 50%, 67%, 75%, 80%, 90%, 100% (default), 110%, 125%, 150%, 175%, 200%, 250%, 300%, 400%,
500% (`round(factor*100)%`); reset when ≠ 1.0. Key `browserDefaultZoomFactor`.

**Default browser appearance** (`id="browser-default-appearance"`): description "The color scheme
pages are told to prefer. System follows your OS setting."; `Select` (`sm`, 160, aria "Default
browser appearance"): System (default), Light, Dark; reset when ≠ system. Key
`browserDefaultAppearance`.

**Browser recording frame rate** (`id="browser-recording-frame-rate"`): description "Maximum
recording rate. 30 fps saves CPU and storage; 60 fps is smoother."; `Select` (`sm`, 160, aria
"Browser recording frame rate"): "30 fps" (default), "60 fps"; reset when ≠ 30. Key
`browserRecordingFrameRate`.

**Show key presses in recordings** (`id="browser-recording-key-presses"`): description "Show
pressed keys and shortcuts in new recordings. Password fields are excluded."; `Switch` aria same
as title. Key `browserRecordingShowKeyPresses` (default off).

**Show mouse presses in recordings** (`id="browser-recording-mouse-presses"`): description
"Highlight mouse presses and held buttons in new recordings."; `Switch`. Key
`browserRecordingShowMousePresses` (default off).

**Open links in** (`id="browser-link-target"`): description "Where links in the chat and terminal
open. Hold ⌘ or Ctrl while clicking a link to open it in your default browser either way.";
`Select` (`sm`, 160, aria "Open links in"): "Your default browser" (`system`, default), "T3 Code"
(`app`); reset when ≠ system. Key `browserLinkTarget`.

**Auto-show floating preview** (`id="browser-auto-show-floating-preview"`): description "Show the
floating preview when an agent opens a browser or device unless the agent says otherwise.";
`Switch` aria "Auto-show floating preview"; default on; reset when off. Key
`browserAutoShowFloatingPreview`.

### 3.2 Browser import wizard (`cs/BrowserImportWizard.tsx`, `cs/browserImportWizard.logic.ts`)

`Dialog` (max-w 512 `max-w-lg`), close button hidden and dismissal ignored while importing.
Sources: chrome, edge, brave, vivaldi, opera, arc, helium, firefox, safari
(`pkg/contracts/src/browserImport.ts`). Start step from the source's `unavailable`:
`browserRunning` → Quit; `needsFullDiskAccess` → Full Disk Access (resume configure); any other
reason → Blocked; no source profiles → Blocked `unknownSourceProfile`; else Configure.

| Step | Title | Description / body | Footer |
| --- | --- | --- | --- |
| Quit | `Quit <name> to import` | `<name> is open, so its cookies can’t be read yet. Quit it, then continue.` | `Cancel` (outline), `I’ve quit it` → Checking (browser) then re-route by refreshed source |
| Full Disk Access | `Let T3 Code read <name>’s cookies` | "To import cookies from <name>, T3 Code needs Full Disk Access. Turn it on in System Settings, then come back to finish the import — you can revoke it again once the import is done." Panel: permission checklist row (icon `HardDrive` 32 `muted-foreground`, title "Full Disk Access", description `Read <name>'s cookies for this import.`, right side "Allowed" (success, `CircleCheck` 16) or `xs` outline "Allow" which opens System Settings > Full Disk Access); status/error line; while not granted: "Access is still required. Quit and reopen T3 Code if you just allowed it, then retry the import." (after a failed check) or "If access doesn't update after you allow it, quit and reopen T3 Code, then retry the import." | `Cancel`, `Continue` (enabled once granted) → import (if resuming an import) or recheck |
| Configure | `Import from <name>` | `Choose which cookies to import for <environment name>.` Panel: two columns side by side (gap 16, centered `ArrowRight` 16 between): "From" (uppercase `text-xs` medium tracking-wide `muted-foreground`) with one tile per source profile (name; subtitle cookie count `"5,065 cookies"`, `"1 cookie"`, `"no cookies"`, or none); "Into": "New profile" / "Created for these cookies" (when under the cap), then each existing profile / "Existing profile". Error line (`text-sm destructive`, mt 12): "That profile is no longer available. Choose where to import these cookies." or "You've reached the profile limit. Choose an existing profile to import into." | `Cancel`, `Import` (disabled without a source profile or a valid target) |
| Checking | `Checking <name>` | "Checking Full Disk Access." / "Checking whether the browser has closed."; spinner (md, muted) + "Checking access…" / "Checking…" | none |
| Importing | "Importing cookies" | "This may take a moment."; spinner + "Importing…" | none |
| Done | `Imported <n cookies>` / `Skipped <n cookies>` / "No cookies found" | `Added to <profile> for <env>.` (+ ` <n cookies> skipped.`) / `No cookies were imported for <env>.` / `There were no cookies to import for <env>.`; when domains were skipped: "Skipped" label + `a, b, c and N more` | `Done` |
| Blocked | `Couldn’t import from <name>` | the failure copy (below) | `Close` (outline), `Try again` for retryable reasons |

Tile (`SelectableTile`, `:443-484`): full-width button, `rounded-lg`, 1 px border, px 12, py 8,
gap 12; title `text-sm` medium; subtitle `text-xs tabular-nums muted-foreground`; right 16 px
round check (selected: `primary` fill with 10 px `Check`; else `input` border). Selected border
`primary`, bg `primary/8`; else `border/60`, hover `border` + `muted/40`. Focus ring 2 px with
1 px offset.

Failure copy (`BROWSER_IMPORT_FAILURE_COPY`): notInstalled "Not installed on this machine.";
needsKeychainApproval "Needs Keychain access to read its cookies."; keychainItemMissing "No
encryption key in your Keychain — sign in to that browser once, then retry."; needsFullDiskAccess
"Give T3 Code Full Disk Access in System Settings → Privacy & Security, then retry.";
browserRunning "Quit the browser first so its cookie database can be read.";
unsupportedPlatform "Importing from this browser isn't possible on this platform.";
keychainUnavailable "The system keyring could not be accessed. Make sure your desktop keyring is
running and unlocked, then retry."; unknownSource "That browser is no longer available to import
from."; unknownSourceProfile "That browser profile no longer exists."; sessionUnavailable "The
target profile could not be opened."; profileNotSaved "The cookies were imported, but the new
profile couldn't be saved. Try again."; profileLimitReached "You've reached the profile limit.
Delete a profile or import into an existing one."; readFailed "The browser's cookie database could
not be read.". Retryable: needsKeychainApproval, keychainItemMissing, keychainUnavailable,
readFailed, sessionUnavailable, profileNotSaved.

Import mechanics (`IntegrationsSettings.tsx:1048-1149`): desktop bridge
`preview.importBrowserCookies({ environmentId, sourceId, sourceProfileDirectory, targetProfileId })`
→ `{ imported, skipped, skippedDomains }`. A new target profile is registered only if
`imported > 0` (name = source name, de-duplicated with " 2", " 3"…); if registration fails, the
partition is cleared and the outcome is `profileLimitReached`/`profileNotSaved`. The new profile
id is stable across retries. Opening System Settings failure → error toast "Could not open System
Settings" / "Open Privacy & Security → Full Disk Access manually.". The permission checklist polls
every 1.5 s and on window focus (`web/components/permissions/usePermissionStatus.ts`); error line
"Could not check permissions. We'll try again automatically."

### 3.3 Devices (`:609-855`)

`SettingsSection id="devices" title="Devices"`. Device state comes from the representative
environment when it is connected: `subscribeDeviceState` (`DeviceServiceState`: `hostStatus`
disabled|installing|starting|ready|failed, `hostStatusDetail`, `hosts[]` with `tools.hub/agent`
{`runningVersion`, `requiredVersion`, `installedVersions`}, `platforms`, `agentDeviceInstalled`,
`hostStatuses`, `devices`, `agentAccessEnabled`, `supportsToolUpdate`, `supportsToolInspection`,
`supportsHostRetry`). The controls remount on environment/scope change.

**Device hub** (`id="device-hub"`, `serverScoped`, `settingKeys=["enableDeviceSupport"]`):
description "Enable this environment to open simulators and emulators, whether they run here or on
a remote device host."; control (in order): tool version popover (§ below, kind hub), compact
status while toggling ("Installing…", "Starting…", "Updating…" with an `xs` spinner), then
`ScopedSwitch` aria "Device hub" (key `enableDeviceSupport`, default off). Disabled at
project/checkout scope, until state loads, without a connected representative, while the host is
installing/starting, or while any device action is pending. Toggle → `device.configure { enabled,
onboardingCompleted: true when enabling, agentAccessEnabled: false when disabling }` on **every
environment in scope**; any failure → error toast "Device settings not saved on all environments"
/ `Could not update <labels>.`.

**Simulator support** (`id="device-platform-support"`), revealed (animated height, 200 ms
ease-out) once the hub is enabled and ready, hidden when disabled: description only when more than
one environment is connected: `Status for <label>. Select an environment to inspect its simulator
support.`; status (flex-wrap, gap-x 20, gap-y 8): compact platform statuses for iOS and Android
(check 16 `success` + "iOS" medium + "Ready"; or `CircleAlert` 16 `muted-foreground` + platform +
message: the host's reason, `"<platform> support was not detected."`, "Xcode is installed, but no
iOS Simulator is available. Install a runtime in Xcode Settings → Components.", "The Android SDK
is installed, but no virtual device exists. Create one in Android Studio → Device Manager.");
control `sm` outline "Refresh" ("Checking…" while pending) → `device.list {}`.

**Agent device access** (`id="agent-device-access"`, `serverScoped`,
`settingKeys=["enableAgentDeviceAccess"]`): description "Allow new agent sessions in this
environment to start and control local and remote devices, with required tools set up
automatically."; control: version popover (kind agent), compact agent status, `ScopedSwitch` aria
"Agent device access" (default off). Disabled with no connected environment, or (outside project
scope) until loaded / no environment has the hub enabled / host busy, or while pending. Project
scope → scoped write `{ enableAgentDeviceAccess }` (project override); otherwise
`device.configure { agentAccessEnabled }` on every environment in scope.

Tool version popover (`web/components/device/DeviceToolVersions.tsx`): trigger `InlineButton
tone="muted"`: `v<version>` (running, else required if installed, else highest installed),
"Not installed", or "Version unknown"; aria `"<Device hub|Agent device>: version <v>|not
installed|version unknown. Show details"`. Popover (align end, width 320): title "Device hub" /
"Agent device"; `dl` (2 cols, gap-x 24, gap-y 4, `text-xs`): Running (or "Not running"), Required,
Installed (comma list or "None"), values mono right-aligned; or "Versions have not been checked.";
footer note (border-top, pt 12) "Tools update automatically on this host when needed."; actions:
"Update to v<required>" (default `sm`, when an update is needed and supported; "Updating…") →
`device.list { updateTool }` (failure line "Update failed. Check this host's network connection
and try again."), and outline "Check versions" ("Checking…") → `device.list { inspectOnly: true }`
(when inspection is supported).

Host updates (`DeviceHostUpdates`, local host only): for a host whose status is installing,
starting, or failed: bordered box (`rounded-md`, `border/60`, px 12, py 8, `text-xs`): host label
(medium), detail or "Installing device tools…" / "Starting device tools…" / "Device support could
not start."; failed adds "Check the host connection and network access, then retry. Your device
settings are saved." and (when supported) a compact outline "Retry" ("Retrying…") →
`device.list { retryHostId }`.

Then `DeviceHostsSettings` (remote device hosts; `settings-pages-connections.md`).

---

## 4. Data

| Element | Source / RPC (fe7d3092c) | t3UI today |
| --- | --- | --- |
| project groups, members, titles, favicon/icon | sidebar project snapshots (`web/sidebarProjectGrouping.ts`) over shell projects | `t3_logic::sidebar` grouping; `OrchestrationProjectShell.{favicon_path, project_icon}` (`t3-protocol/src/orchestration.rs:344-345`) |
| rename / icon | dispatch `project.meta.update { projectId, title? , faviconPath?, projectIcon? }` | `t3_protocol::commands::ClientCommand::ProjectMetaUpdate` + `ProjectMetaPatch` (has `title`, `favicon_path`, `project_icon`) |
| remove | dispatch `project.delete { projectId, force: true }` | `ClientCommand::ProjectDelete { force }` |
| thread counts for the remove confirm | shell threads (`useThreadShells`) | `Environment::threads()` |
| image file search | `projects.searchEntries { cwd, query, kind: "file", imageOnly: true, limit: 200 }` | `methods::ProjectsSearchEntries` (input has `image_only`, `t3-protocol/src/projects.rs:55`) |
| native file chooser for icon | desktop IPC `pickProjectFavicon(root)` | missing (use `rfd`/NSOpenPanel) |
| t3.json scripts / validity | `projects.readFile { cwd, relativePath: "t3.json" }` + `parseT3ProjectFile` | `methods::ProjectsReadFile`; `T3ProjectFile` parser missing |
| actions list | `ServerSettings.defaultProjectScripts`, `projectSettingsOverrides[pid].defaultProjectScripts` | `ServerSettings.default_project_scripts` typed; overrides only in `other` (add typed) |
| action write | `server.updateSettings { patch }` | `methods::ServerUpdateSettings` (patch `other` map works) |
| action shortcuts | `server.upsertKeybinding { key, command, replace? }`, `server.removeKeybinding { key, command, when? }`; `ServerConfig.keybindings` | `methods::{ServerUpsertKeybinding, ServerRemoveKeybinding}` exist |
| source control discovery | `server.discoverSourceControl {}` → `SourceControlDiscoveryResult` | missing in `t3_protocol::methods` (listed in protocol.md:476) |
| git fetch interval | `ServerSettings.backgroundActivity` | in `other` (add typed `BackgroundActivitySettings`) |
| Bitbucket credentials | `server.updateSettings { patch: { bitbucket } }`; read `ServerSettings.bitbucket` (redacted) | write fits via `other`; typed read missing |
| writing style / writer model | `ServerSettings.sourceControlWritingStyle`, `sourceControlWriterModelSelection`; providers from `ServerConfig.providers` | `source_control_writer_model_selection` typed; writing style in `other` |
| browser profiles, viewport, zoom, appearance, recording, link target, auto-show | `ClientSettings.browser*` | missing in `t3_logic::settings::ClientSettings` |
| browser import sources / import / clear data | Electron preview bridge (`listBrowserImportSources`, `importBrowserCookies`, `clearCookies`, `clearCache`), `checkSystemPermission("full-disk-access")`, `openSystemSettings` | no equivalent: the native app has no Electron session partitions; needs a design decision (see §7) |
| device state | `subscribeDeviceState {}` → `DeviceServiceState` | missing |
| device configure / list | `device.configure { enabled?, agentAccessEnabled?, onboardingCompleted? }`, `device.list { updateTool?, inspectOnly?, retryHostId? }` | missing |
| device keys | `ServerSettings.enableDeviceSupport`, `enableAgentDeviceAccess`, `deviceOnboardingCompleted`, `deviceHosts` | in `other` |

---

## 5. Reuse map

| t3UI module | Verdict |
| --- | --- |
| `crates/t3-app/src/state/route.rs` on main (2976a48: utility routes, project links redirect to settings) | fits for `/projects/<key>`; make sure it lands on `Settings{page: Projects, scope.project = key}` and that the Project nav item shows. |
| `crates/t3-logic/src/sidebar/` (`logical_project_key`, `physical_project_key`, `SidebarProject` members) | fits for project groups/members; needs a "follow the moved group" lookup by member physical keys (§1.2). |
| `crates/t3-protocol/src/commands.rs` (`ProjectMetaUpdate`, `ProjectDelete`) | fits as-is. |
| `crates/t3-protocol/src/methods.rs` keybinding upsert/remove, `ProjectsReadFile`, `ProjectsSearchEntries` | fits as-is. |
| `crates/t3-protocol/src/server.rs` `ServerSettings` | needs typed fields: `project_settings_overrides`, `background_activity`, `bitbucket`, `source_control_writing_style`, `enable_device_support`, `enable_agent_device_access`. Keep `other`. |
| `crates/t3-logic/src/keybindings/` + `origin/settings:crates/t3-logic/src/keybindings/editor.rs` | reuse the key-event → wire string conversion for the action editor's Keybinding field. |
| `origin/settings:crates/t3-app/src/settings/source_control.rs` | stub; replace. |
| `origin/settings:crates/t3-app/src/settings/general.rs` (`default_text_model`) | reusable for the writer-model "copy the text generation selection" behavior. |
| `crates/t3-app/src/command_palette` (origin/settings) | reusable chrome for the icon file picker (§1.9). |
| `crates/t3-ui` `Dialog`, `ToggleGroup` (segmented), `Select`, `NumberField`, `Switch`, `Badge`, `Skeleton`, `Empty` | per design-system refresh. Need a lucide-by-name lookup for project icons (dynamic icon set) and the brand icons (GitHub, GitLab, Forgejo, Azure DevOps, Bitbucket, Git, Jujutsu). |

---

## 6. Reference screenshots needed

| Name | How to reach | Seed |
| --- | --- | --- |
| `settings-project-notice` | Settings > General, then search "Actions", Enter | default (no project scope): notice "Actions is not available…" with project buttons |
| `settings-project-overview` | Scope sentence → pick a project with one checkout → nav "Project" | project with 2+ threads, a `t3.json` with one script |
| `settings-project-two-checkouts` | Same, project grouped across 2 environments | two environments with the same repo (shows "Checkouts" and "Remove this project everywhere") |
| `settings-project-actions-import-menu` | Project page, click "Import scripts" | `t3.json` declaring scripts not yet added |
| `settings-project-action-editor-add` | Click "Add action" | default |
| `settings-project-action-editor-edit` | Hover an action row, click the gear | project with a script with keybinding + preview URL |
| `settings-project-remove-confirm` | Danger > "Remove project" | project with 3 threads |
| `settings-project-icon-picker-icons` / `-emoji` / `-monogram` | "Choose icon", each tab | default |
| `settings-project-icon-file-picker` | "Choose file" | project containing png/svg files |
| `settings-source-control-discovered` | Settings > Source Control | server with git + gh authenticated (account redacted) |
| `settings-source-control-git-expanded` | Click the Git row chevron | default |
| `settings-source-control-bitbucket` | Expand Bitbucket (API token tab) | server reporting bitbucket provider |
| `settings-source-control-scanning` | First load (skeleton) | throttle discovery |
| `settings-source-control-custom-instructions` | Writing style → Custom instructions | default |
| `settings-integrations-top` | Settings > Integrations | default (browser rows, devices off) |
| `settings-integrations-profiles-menu` | Click "Add profile" | macOS with Chrome installed |
| `settings-integrations-viewport-responsive` | Default browser viewport → Responsive | default |
| `settings-integrations-devices-ready` | Enable Device hub on a mac with Xcode | device hub ready (shows Simulator support) |
| `settings-integrations-import-configure` | Add profile → Import from Chrome (browser closed) | Chrome with 2 profiles |

---

## 7. Open questions / risks

1. **Browser profiles and cookie import are Electron-session features** (partitions per profile,
   cookie DB import into a partition). The native app's preview story decides whether these rows
   exist at all. If there is no in-app browser, hide the Browser section rather than showing inert
   rows; confirm with the user.
2. **Device hub** depends on `subscribeDeviceState` and `device.*` RPCs that `t3_client` does not
   speak yet; the rows are useless without them.
3. `ProjectDefaultsSettings` is the top block on all three pages; its spec lives in the General
   file. Make sure its `category` filter is implemented once and shared.
4. The Project page container uses a 24 px section gap (`gap-6`), unlike other pages (32 px).
5. Name commits on blur only when the user typed; programmatic re-keys (group renamed elsewhere)
   must not trigger a write.
6. `RefreshIcon` spins continuously while a discovery scan runs. Keep it bounded to the request.
7. Action shortcuts are written only on desktop (`isElectron`); the native app is desktop, so it
   always writes them.
8. `Schema.Option` fields in discovery results use the `_tag` encoding; decoders must tolerate both
   shapes and unknown `kind`s (`unknown` exists in both enums).
