# Pull requests page

Written against t3code-fork `fe7d3092c` (2026-10-02). The source wins over this file; fix the spec
when they disagree. Paths are relative to the fork's `apps/web/src` unless noted.

| Piece | Fork source | Native |
| --- | --- | --- |
| Route, page state, list column | `routes/_chat.pull-requests.tsx` | `crates/t3-app/src/pull_requests/` |
| Row, group header, ghosts, empty and unavailable states | `components/pullRequest/PullRequestRow.tsx`, `PullRequestListRow.tsx`, `PullRequestGhosts.tsx`, `PullRequestListEmptyState.tsx`, `PullRequestsUnavailableState.tsx` | `pull_requests/list.rs`, `pull_requests/states.rs` |
| Search, sort, filters, provider menus | `PullRequestListFilters.tsx`, route `CompactFilterMenu` (`:2211`) | `pull_requests/controls.rs` |
| Pure list logic | `pullRequestList.logic.ts`, `pullRequestProjectFilter.logic.ts`, `pullRequestProjectAssignment.logic.ts`, `pullRequestListPreferences.ts` | `crates/t3-logic/src/pull_requests/` |
| Presentation (state/checks/review tones, labels) | `pullRequestPresentation.tsx`, `pullRequestIcons.tsx` | `t3-logic` (`presentation.rs`) + `pull_requests/style.rs` |
| Detail panel | `PullRequestDetailPanel.tsx`, `PullRequestSummaryTab.tsx`, `PullRequestTimelineTab.tsx`, `PullRequestCodeTab.tsx`, `PullRequestComposer.tsx` | `pull_requests/detail/` (in progress) |
| Wire types | `packages/contracts/src/pullRequest.ts`, `rpc.ts:737-920`, `environmentHttp.ts:541` | `t3-protocol::pull_requests` (client agent) |

Units are CSS px at the desktop breakpoint. The window is always wider than Tailwind's `sm`
(640px), so every `sm:` value applies and the non-`sm` value never does.

---

## 1. Entry, route and page state

- Opened by the sidebar footer's "Pull Requests" button (`components/sidebar/SidebarChrome.tsx:123,163`),
  route `/pull-requests`. While on it the sidebar swaps its utility row the way Settings does
  (`components/sidebar/mainAppLocation.ts:4-13`). Escape navigates back (`useEscapeToGoBack`,
  route `:344`) unless a panel shortcut consumed it first (route `:2058-2071`).
- The web keeps every list control in URL search params (`PullRequestsSearch`, route `:175-199`,
  validated at `:295-341`). Native keeps them in the view (`PullRequestsView`) as a
  `ListScope { involvement, state, sort, environment, project, host, q, draft, review, checks,
  author, labels }` plus the selection (`repository, number, selected_project, selected_host,
  selected_environment`).
- Defaults: `involvement=all`, `state=open`, `sort=ready`. Labels: at most 10, deduped
  case-insensitively, each trimmed to 200 chars (`pullRequestSearchLabels`, `:276-293`).
- **Preferences** (`pullRequestListPreferences.ts`): every list-control change writes the whole
  scope (selection excluded) to storage key `t3.pullRequests.preferences`; opening the page reads it
  back. `sort` is omitted when `ready`; empty fields are omitted. Native: a JSON file in the app's
  store, same shape.

## 2. Data

### 2.1 Which servers are read

- Capable environments: connected ones whose `serverConfig.environment.capabilities.pullRequests`
  is `true`, sorted by environment id (route `:355-363`).
- `capabilityKnown` = any environment has a server config yet. Until then the list shows the ghost.
- No capable environment (but some config known): the page shows the unavailable state titled
  **"Pull requests unavailable"** / **"Update your T3 Code servers to browse pull requests."**
  (route `:1749-1753`), and the right-panel controls are not mounted.
- An `environmentId` scope keeps only that server if it is capable; a stale one falls back to all.
- Projects: only those on capable environments. `projectsKnown` = every environment's shell has
  bootstrapped (until then an empty project list means "not loaded").
- `resolveProjectScope`, `findScopedProject`, `resolveQueryEnvironmentIds`,
  `resolveSelectedEnvironmentId` (logic `:886-960`) decide the project scope and which servers to
  ask. `assignProjectsToEnvironments` (assignment logic `:30-68`) gives each shared repository
  (`repositoryIdentity.canonicalKey`, lowercased) to one server: the first by rank. A server that
  holds everything it was assigned is asked without `projectIds`; one with nothing left is not read
  (route `:637-660`).

### 2.2 Reads per page state

All reads go to every queried environment and are merged with `mergePullRequestLists`
(logic `:703-749`): viewers keyed `"<envId> <host>"`, providers merged per host
(`configured` = any, `searchesOnHost` = all, counts summed), entries tagged with `environmentId`
and sorted by `updatedAt` desc, `nextCursors` per environment, `truncatedEnvironments`.

| Read | Input (`pullRequests.list`) | When |
| --- | --- | --- |
| list | `{state, involvement, limit: pageSize, projectId?, projectIds?, host?, filters? (menu + parsed qualifiers), query? (parsed text), cursors?}` | always |
| baseline | same without `query`/cursors, `limit: 99`, filters = menu filters only | always; it is the answer while nothing is typed and the source of viewers/providers/errors |
| facets | `{state: "all", involvement, limit: 99, projectId?, projectIds?, host?}` | only while the Filters menu is open |
| partitions | baseline input with `involvement: "authored"` and `"reviewing"` | only when `involvement=all`, nothing typed, and the baseline has rows |

- `PAGE_SIZE = 99`, `MAX_PAGE_SIZE = 500`. "Load more" continues with `nextCursors`
  (environments in `truncatedEnvironments` without a cursor are re-read at `pageSize + 99`), or,
  with no cursors, re-reads at `pageSize + 99` (route `:1147-1170`).
- Search text is debounced **250ms** before it is sent (`SEARCH_DEBOUNCE_MS`). While it travels,
  rows already on screen are narrowed locally with `matchesPullRequestQuery`.
- `pullRequests.listStats` fills `+a -d` for rows whose listing carried `0/0`: eager for
  `ready`/`largest`/`smallest` sorts, otherwise only rows within 480px of the viewport. Batches of
  at most 500 refs per environment (logic `:460-549`). Counts merge into a held map keyed
  `"<envId> <projectId> <number>"`.
- **Refresh** (header button): `pullRequests.invalidate {}` on every queried environment, then
  re-read list, baseline, facets, partitions and stats, and bump the detail panel's refresh token
  (route `:887-901`). The button is disabled while invalidating or while the list is pending.
- **Live refresh** (`hooks/useLiveRefresh.ts`): re-read (not invalidate) when the window regains
  focus and every 5 minutes while visible, at most once per 10s, stopping after 6 minutes without
  input.
- `pullRequests.subscribeRefreshes` (stream of a revision number per environment): a new revision
  after a turn re-reads the list when it is showing a continued (cursored) page (route `:1213-1223`).
- **Snapshot**: the last unsearched answer per environment set is stored
  (`t3.pullRequests.list:<sorted env ids>`), at most 99 entries per list, errors/cursors dropped,
  and hydrated on the next visit so the page never cold-starts into ghosts (logic `:751-874`).

### 2.3 Rows on screen

1. Held order (`ordered`): a whole-page answer replaces it (reusing unchanged entries); a
   continuation appends only rows not already held, sorted by `updatedAt` desc among themselves.
2. `filterPullRequestsByInvolvement` (logic `:327-339`).
3. Local filters from the menu plus typed qualifiers (`matchesPullRequestFilters`, `:380-399`).
4. No text: sort by `updatedAt` desc. Text: keep rows a searching host returned plus local matches.
5. Groups (only for `involvement=all`): `partitionPullRequestsWithPriority` when both partitions
   answered (or a snapshot holds them), else `groupPullRequestsByInvolvement`. Order: **Authored**,
   **Review requested**, **Others**; empty groups are dropped. Other involvements render one
   unlabeled group.
6. Pending action overrides (close/reopen/merge/draft/ready) are applied per group, dropping rows the
   state filter no longer holds (`applyPullRequestOverrides`, `:1186-1204`), and cleared by an
   answer that agrees or after 60s (`settlePullRequestOverrides`).
7. `sortPullRequestGroups(groups, sort, text, hasMeasuredSize, involvement)` (`:1077-1128`).

Sorts (route `:241-249`), label and icon:

| Value | Label | Icon | Rule |
| --- | --- | --- | --- |
| `ready` | Merge readiness | list-checks | tiers: passing+approved, passing, other open/draft, closed/merged, conflicting; then measured size asc, then recency. Not applied while searching. |
| `blocked` | Blocked on me | user-lock | authored group: conflicting, changes requested, failing, draft, other, ready, closed; review group: open first; recency inside |
| `updated` | Recently updated | clock | host order (already recency) |
| `newest` / `oldest` | Newest shown / Oldest shown | calendar-arrow-down / -up | by `createdAt` |
| `largest` / `smallest` | Largest shown / Smallest shown | maximize-2 / minimize-2 | measured first, by additions+deletions |

---

## 3. Page layout

```
SidebarInset (h 100%, overflow hidden)
└─ relative flex row, flex-1
   ├─ [panel open] titlebar controls strip (absolute, see 3.2)
   ├─ PullRequestsColumn: flex col, flex-1, min-w-0, bg background, @container/pr-list
   │  ├─ WorkspacePageHeader (3.1)
   │  └─ scroll area: flex-1, overflow-y auto, topbar-scroll-fade, scrollbar gutter both sides
   │     └─ WorkspacePageContainer width=expanded (3.3)
   │        ├─ controls block (3.4)
   │        └─ list body (section 4)
   └─ [panel open] RightPanelTabs inline, default width floor(window width / 2) (section 6)
```

### 3.1 Header (`WorkspacePageHeader`, route `:2472-2534`)

- Height 52 (`--workspace-topbar-height`), flex row, `items-center`, gap **12**, `bg-background`,
  `position: relative`, window drag region. Padding left/right = `--workspace-gutter` = **20**
  (sm). While the sidebar is collapsed the left padding becomes the titlebar content inset
  (native: `collapsed_titlebar_inset`). No bottom border.
- Children, in order:
  1. Breadcrumb `<ol>`: flex, gap **12** (sm:gap-3), text-sm 14/20.
     - At rest: one item, font-medium, `foreground`: **"Pull Requests"** (truncate).
     - Condensed (the controls row scrolled out of view, 3.5): "Pull Requests" `/` (separator
       in `--icon-muted` = muted-foreground) then an item holding compact menus for State,
       Involvement and (only with more than one host) Host: ghost-muted `sm` buttons (h 28,
       px 9, gap 6) showing the current label and a 12px chevron-down at `muted-foreground/70`.
       While the condensed search is expanded the title is visually hidden and the separator dropped.
  2. Spacer `flex-1`.
  3. Condensed only: flex gap **6**: expandable search (a ghost `icon-sm` 28×28 search button that
     becomes a 224px-wide (`w-56`, min 96) search input while focused or non-empty) and the compact
     refresh button (ghost `icon-sm` 28×28, 16px refresh-cw).
  4. Right-panel spacer: width **20** (sm:w-5) while the panel is closed, 0 with -12 margin while
     open. It keeps refresh clear of the floating toggle.

### 3.2 Titlebar controls (`openPanelControls`, route `:1726-1737`)

- Absolute, top 0, right **12 + 1** (`--workspace-controls-right` + `mr-px`), height 52, flex
  gap 4, `z-50`, no-drag. Mounted inside the header while the panel is closed, at the route level
  (over the panel) while it is open.
- Holds `PanelLayoutControls` with only the right-panel toggle: `Toggle variant=panel size=panel`,
  icon panel-right (closed) / panel-right-close (open), 16px. Disabled with tooltip
  **"Select a pull request first"** until a pull request surface exists; otherwise tooltip
  "Toggle right panel (<shortcut>)", side bottom.

### 3.3 Content frame (`WorkspacePageContainer width="expanded"`)

- `mx-auto`, full width, max-width **1152** (`max-w-6xl`), padding x **24** (sm:px-6), top
  **24**, bottom **48**, flex column, gap **16** (`gap-4` overrides the default 24), min-height 100%.

### 3.4 Controls block (route `:2544-2567`)

Flex column gap 12 holding one wrapping row (flex, wrap, `items-center`, gap **8**) and a 1px
marker with -12 top margin.

| # | Control | Box | Content |
| --- | --- | --- | --- |
| 1 | Search wrapper | full row width below 512px of column (`basis-full`); at `@lg/pr-list` (column ≥ 512) `basis-0 flex-1` | `PullRequestSearchInput` |
| 2 | Sort | `Button variant=outline size=default` h 32, px 11, gap 8, 14px medium | arrow-down-up 16px + **"Sort"** |
| 3 | Filters | outline default | list-filter 16px + **"Filters"** + count pill when > 0 (rounded-full, `bg-muted`, px 6, 12px, `muted-foreground`, tabular) |
| 4 | Provider | outline default; `size=icon` 32×32 icon-only while a host is selected | plug-2 16px + **"All"**; icon-only shows the selected host's provider icon |
| 5 | Refresh | `Button variant=outline size=icon` 32×32 | refresh-cw 16px; spins while refreshing (static glyph natively, see AGENTS.md) |

Search input (`PullRequestSearchInput`, filters `:98-122`): InputGroup (radius 8 =
`--control-radius`, 1px `input` border, `bg-background` light / `input/32` dark, shadow-xs/5 +
bevel), inner input height **32**, text 14. Leading addon padding-left 11 (`calc(3*4px-1px)`):
search 16px at 80% opacity, replaced by a spinner while a search is on its way. Input padding-left
8. Placeholder **"Search pull requests, or label:bug"** at `muted-foreground/72`. Focus: border
`ring`, 3px `ring/24` halo.

Filter count (filters `:456-468`) counts: state ≠ open, involvement ≠ all, host, server, project,
draft, review, checks, author, and each selected label.

### 3.5 Condensing

An intersection observer on the marker (root = the scroll area) sets `condensed` once the marker
leaves the viewport. Ctrl/Cmd+F focuses and selects the in-flow search at rest, or expands and
focuses the condensed search (route `:2430-2447`). Typing in the condensed search that shortens the
list enough to un-condense moves focus back to the in-flow input with the caret at the end.

---

## 4. List body

Exactly one of, in order (route `:1745-1853`):

| Condition | Shows |
| --- | --- |
| `!capabilityKnown` | ghost, 7 rows |
| no capable environment | unavailable: "Pull requests unavailable" / "Update your T3 Code servers to browse pull requests." (no retry) |
| first load (pending, nothing held) | ghost, 7 rows |
| list error and no rows | unavailable: **"Could not load pull requests"** / the error message, **Retry** |
| carried rows narrowed to nothing while pending, no text | ghost, 7 rows |
| no rows | empty state (4.4) |
| otherwise | groups (4.1) |

Below it, when there is a list error and rows are showing: a warning banner (rounded-lg, 1px
`warning/30` border, `warning-surface` bg = warning 8% light / 16% dark, px 12 py 8, text-xs,
flex justify-between gap 12): "<error> Showing the last pull requests loaded." + outline `xs`
**Retry**. When `truncated` and rows exist: a centered 12px `muted-foreground` footer, py 12:
"Updating pull requests" / "Loading more" with a spinner while loading, an outline `sm`
**"Load more pull requests"** button, or "Narrow your search to find more pull requests." at the
500 cap.

### 4.1 Groups and header (route `:211-225`, `:1784-1820`)

- Groups stack with gap **12** (`space-y-3`); rows within a group with gap **2** (`space-y-0.5`).
- Header: flex `items-center` gap 8, padding x 12, bottom 4, text-xs 12/16 medium,
  `muted-foreground/70`: icon 14px (authored = pen-line, reviewRequested = eye, others = users),
  the label, the count in `muted-foreground/50` tabular, then a 1px `border` rule `flex-1`
  (min-width 8).

### 4.2 Row (`PullRequestRow.tsx`, `PullRequestListRow.tsx`)

```
button: flex row, items-center, gap 8, w 100%, radius 6 (rounded-md), padding 10 / 12, text-left
│  bg: selected `accent`; hover `accent/60`; no transition on bg
├─ glyph column: w 16, flex col, items-center, gap 2, self-start, margin-top 3
│  └─ state glyph 16px (tone per 5.1) with the conflict badge (12px triangle-alert,
│     `destructive`, fill background, stroke 2.5) at right -4, bottom -4 when open, not draft,
│     mergeability = conflicting
└─ lines: flex-1, min-w-0
   ├─ line 1: flex items-center gap 6
   │  ├─ "#<n>": mono 12/16 tabular `muted-foreground`, shrink-0
   │  ├─ title: 14/20 truncate (`foreground`)
   │  ├─ signals: flex gap 4, 11px: checks glyph (14px, 5.2) then review glyph (14px, 5.3)
   │  └─ status: ml-auto flex gap 6, 11px: stack chip (when `stack`), diff stat mono
   │     "+a" in diff-addition-foreground, "-d" in diff-deletion (hidden when both 0)
   └─ line 2: flex items-center gap 6, overflow hidden, 11px (`text-2xs`, line-height 16) `muted-foreground`
      ├─ "matched in the description" pill (search only, when the row's own fields score ≤ 10)
      ├─ provider icon 12px (only with more than one host)
      ├─ author: avatar 14px + login (login hidden below a 320px meta width), min-w 14, max-w 160
      ├─ repository (`owner/name`) truncate
      ├─ environment label, max-w 128 (only with more than one capable environment)
      ├─ labels: up to 3 chips (5.4); chip 2 from meta width 576 (`@xl`), chip 3 from 768 (`@3xl`);
      │  the last visible chip carries "+N"
      └─ updated time ml-auto, tabular, "5m ago" / "3h ago" / "12d ago" / "just now"
```

Row content height: 20 + 16 = 36 (the fork reserves 36.5); total 56.

Interactions:
- Click: opens the pull request in the right panel (`openPullRequest` on the page's panel ref) and
  records the selection (`repository, number, selectedProjectId, selectedEnvironmentId,
  selectedHost`).
- Right-click on "#n": the link context menu (open on host, copy link) (`pullRequestLinkContextMenu.ts`).
- Tooltips: state label ("Open", "Draft", "Closed", "Merged"), conflict ("Conflicts with <base>"),
  checks headline, review label, author "Name (@login)", provider name.

### 4.3 Ghost (`PullRequestGhosts.tsx:56-90`)

`space-y-0.5` stack; optional caption (px 12, pb 4, 12px medium `muted-foreground/70`); each row a
grid `auto | 1fr | auto`, items-center, gap 8, radius 6, padding 10/12:
- 16×16 round bar;
- column gap 6: title bar h 16 with widths cycling 60/40/50/66/40/60/50 %; meta bar h 14 with widths
  40/33/40/25/33/40/33 %;
- right column, items-end, gap 6: 48×12 and 64×12 bars.

Bars: radius 4, `muted-foreground/15`. The fork pulses the container (opacity, 2.4s, 4 steps);
native draws it still.

Search in flight with no rows: the ghost with 5 rows and the caption
**"Searching every host for "<query>""** (query cut to 48 chars + "…").

### 4.4 Empty states (`PullRequestListEmptyState.tsx`)

`Empty`: flex col, items-center, justify-center, text-center, flex-1, gap **24**, padding 48.
Contents: BranchMark (128×80 svg, 120×72 viewBox, stroke 2 round, `muted-foreground/60`; base line
`M10 58h100` at `muted-foreground/30` with r5 end dots at 25% fill; the branch `M30 58c0-18 8-26
24-26h4` solid and `M90 58c0-18-8-26-24-26h-4` dashed `2 7` at `muted-foreground/50`; a hollow r4
node at (60,32) in `muted-foreground/45`), header (max-w 384, title 20/28 semibold, description
14/20 `muted-foreground` with 4 top margin), buttons (flex wrap, centered, gap 8, `sm` outline,
h 28, px 9, gap 6, icons 14px).

| Case | Title | Description | Buttons |
| --- | --- | --- | --- |
| no projects | No projects in this workspace | Add a project, and the pull requests from its repository appear here. | **Add project** (default variant, plus icon) opens the command palette's add-project |
| search in flight | (ghost with caption, 4.3) | | |
| search answered | Nothing matches "<query>" | The hosts were searched for it. Try fewer words, or search by number, author or branch. | **Clear search** (search icon), **Check again** / "Checking..." (refresh icon) |
| filtered | Nothing under these filters | Widen the state, involvement or project filter to see more. | [Load more pull requests], Check again |
| unfiltered | No pull requests | Pull requests from every project in this workspace appear here. | [Load more pull requests], Check again |

"Filtered" = any menu filter, state ≠ open, involvement ≠ all, a project scope, or a host.
"Check again" runs the header refresh (invalidate + re-read).

### 4.5 Unavailable state (`PullRequestsUnavailableState.tsx`)

`Empty` (as 4.4) with `EmptyMedia variant=icon`: a 36×36 card (radius 8, 1px border, `card` bg,
shadow-sm/5, 18px git-pull-request-arrow at `foreground`) with two ghost copies behind it rotated
∓10° at 84% scale (bottom-left / bottom-right origins); media bottom margin 24 on top of the gap.
Title (default **"Could not load pull requests"**), description = the error, then an outline `sm`
**Retry** (refresh icon) when retryable, and **"Open on GitHub"** (external-link 14px) when a URL is
known (detail panel only).

Error strings come from the server error's `message` (`packages/contracts/src/pullRequest.ts:1256-1411`):
- `PullRequestUnavailableError` with `reason` and `provider`: the provider's sentence, e.g.
  "GitHub CLI (`gh`) is required to browse change requests on this host. Install it from
  https://cli.github.com/ and reload." / "GitHub CLI is not authenticated. Run `gh auth login` and
  retry."; fallbacks "The tool this host is read through is not installed or set up." / "This host
  has no working credentials." / "Change requests cannot be browsed for this project's host yet."
- `PullRequestOperationError`: "Pull request operation <operation> failed: <detail>".
- anything else: its message, or "The environment request failed.".

---

## 5. Presentation tokens

### 5.1 State (`pullRequestIcons.tsx`)

| Key | Label | Icon | Light | Dark |
| --- | --- | --- | --- | --- |
| open | Open | git-pull-request-arrow | emerald-600 | emerald-300 @90% |
| draft (open + isDraft) | Draft | git-pull-request-draft | zinc-500 | zinc-400 @80% |
| closed | Closed | git-pull-request-closed | red-600 | red-300 @90% |
| merged | Merged | git-merge | violet-600 | violet-300 @90% |

Closed and merged win over a stale draft flag. t3-ui: open = `status.completed.text`, draft =
`status.pr_closed.text`, merged = `status.pr_merged.text`; red has no token yet.

### 5.2 Checks rollup (`pullRequestPresentation.tsx:264-287`)

| State | Tooltip | Icon | Tone |
| --- | --- | --- | --- |
| passing | All checks have passed | circle-check | emerald-600 / emerald-300 @90% |
| failing | Some checks were not successful | circle-x | `destructive` |
| pending | Some checks haven't completed yet | circle-dot | amber-600 / amber-400 @90% |

Clicking the row glyph opens the checks popover (`PullRequestChecksPopover.tsx`, reads
`pullRequests.detail`).

### 5.3 Review decision (`:76-114`)

| Decision | Label | Icon | Tone |
| --- | --- | --- | --- |
| approved | Approved | user-check | emerald-600 / emerald-300 @90% |
| changes-requested | Changes requested | user-round-x | amber-600 @90% / amber-400 @80% |
| review-required | Awaiting review | user-round | `muted-foreground/60` |

### 5.4 Label chip (`:46-69`)

Badge `sm`: h 16, min-w 16, radius 4, padding x 3, 10px medium, `leading-none`, gap 4, max-w 160,
truncating. With a valid 6-digit hex color (`pullRequestLabelColor`): background = color at 8%
(light) / 12% (dark); text = mix(color 30%, foreground) light / mix(color 45%, foreground) dark.
Otherwise the `secondary` badge (`secondary` bg, `secondary-foreground`).

### 5.5 Diff stat and avatars

- `+1,234` in `--diff-addition-foreground` (emerald-600 / emerald-400) and `-56` in
  `--diff-deletion` (= `destructive`), gap 4, tabular, grouped thousands.
- Avatar: the host image, or a circle in `muted` with the login's first letter (10px medium,
  `muted-foreground`). Native draws the initial until image loading lands.

---

## 6. Right panel and detail

- One panel for the page, keyed by a fixed sentinel ref so it survives a server dropping
  (`PULL_REQUESTS_PANEL_REF`). Surfaces are `pull-request` tabs `{environmentId, projectId, host,
  repository, number}`; tab strip and close/close-others/close-to-right/close-all follow the shared
  RightPanelTabs (owned by the panels agent). Width stored under `t3code:pull-request-panel-width`,
  default `floor(window width / 2)`.
- Shortcuts while on the page: `rightPanel.toggle`, `rightPanel.close` (close active tab),
  `thread.copyReference` (copy the open PR's URL; toast "PR link copied").

### 6.1 Detail header (`PullRequestDetailPanel.tsx:1665-2525`)

```
grid [1fr auto], gap-x 8, border-bottom border/60
├─ left (pl 16, h 28): repository (12px medium muted, link to repo) + "#n" (state tone,
│  medium, external-link 10px) — condensed: "#n" + title
├─ right (mr 16, h 28, flex gap 4): [stack menu] [linked threads count] [checkout menu]
│  [auto-merge badge] [primary action] [⋯ menu, icon-xs ghost-muted] [collapse panel icon-xs]
├─ fold (expanded, px 16, pb 16, mt 4):
│  ├─ title 16/1.375 semibold truncate (+ edit button when editable)
│  ├─ mt 8, 12px muted: author label · "updated 5m ago"      [checkout command, mono, right]
│  └─ mt 16, 12px: base ← head (mono, muted/70, base max 40%)  [file-diff "N files"] [+a -d]
└─ tabs nav (border-top border/60, px 16, py 8, gap 8): segmented Summary | Timeline | Code
   + right side: Summary → checks summary ("All checks passed", "2 of 5 failing"...);
     Timeline → comment/commit/approval counts + "Newest first"/"Oldest first" toggle
```

Primary action (one of): Resolve conflicts (destructive-outline), Ready for review, Enable
auto-merge, auto-merge armed badge, Merge (`Merge pull request` / `Squash and merge` /
`Rebase and merge`), or a Merged/Closed outline badge. The ⋯ menu: Link to thread, Refresh, Ask a
question, Explain this PR, Fix findings, [act on server], —, Ready for review / Convert to draft,
Merge now, Enable/Disable auto-merge, merge method radio, —, Open on GitHub, Copy link, Copy PR
number, —, Close pull request / Reopen pull request / Revert changes.

Confirm dialog titles: "Merge pull request?", "Enable auto-merge?", "Revert these changes?",
"Approve workflows to run?", "Close pull request?"; bodies at `:2829-2840`; confirm labels at
`:2862-2870`.

### 6.2 Summary tab (`PullRequestSummaryTab.tsx:710-1056`)

- Meta (px 16, pt 10, pb 4, rows gap 8): grid `96px | 1fr`, min-h 24, 12px: Reviewers (users
  icon; overlapping 16px avatars ringed by their verdict, or "None"; reviewer picker), Labels (tag
  icon; `default`-size chips or "None"; label picker).
- Sections with sticky headings (px 16, py 12, 12px medium muted, chevron-right 14px rotated when
  open, body px 16 pb 16): **Description** (markdown or "_No description provided._"),
  **Checks** (collapsed by default; rows: status icon, name, status label; "No checks reported."),
  **Comments (N)** (newest/oldest toggle, comment cards).

### 6.3 Reads and writes per interaction

| Interaction | RPC |
| --- | --- |
| open a pull request | `pullRequests.detail` + `pullRequests.activity` (separate; activity may arrive later) |
| Code tab | HTTP `POST /api/pull-requests/diff` (paged by `nextCursor`), `pullRequests.filesViewed`, `pullRequests.setFilesViewed`, `pullRequests.diffFileContents` (expand context) |
| merge / ready / draft / close / reopen / auto-merge / update branch / revert / approve workflows | `pullRequests.runAction` |
| edit title or description | `pullRequests.update` |
| comment / edit comment | `pullRequests.comment` / `pullRequests.updateComment` |
| submit review | `pullRequests.submitReview` |
| reply / resolve thread / more comments | `pullRequests.replyToThread` / `setThreadResolution` / `threadComments` |
| react | `pullRequests.setReaction` |
| reviewers / labels menus | `reviewerCandidates` + `requestReviewers`, `labelCandidates` + `setLabels` |
| stack menu, linked threads, hover card | `pullRequests.stack`, `linkedThreads`, `preview` |
| refresh in ⋯ menu | `pullRequests.invalidate {reference}` then re-read |

After `runAction`, the page writes an override onto the list row (`pullRequestOverrideAfterAction`)
as the action is sent (merge only once done) and re-reads list + stats for that environment.

---

## 7. Native implementation notes

- `t3_logic::pull_requests` ports the logic files 1:1 (function names snake_cased) over
  `t3_protocol::pull_requests` types; `EnvironmentEntry` wraps a `PullRequestListEntry` with its
  `EnvironmentId`. Tests list failure modes first.
- `pull_requests::PullRequestsView` (entity) owns the scope, query state, held rows and stats; the
  list is a GPUI `uniform_list`-free `div` stack for now (rows are fixed 56px, so a virtualized list
  is a later optimization). Rows render from `Arc<EnvironmentEntry>`.
- Icons and Tailwind tones missing from t3-ui live in `pull_requests/style.rs` behind one table,
  using stand-in icons until the design agent's regenerated set lands.

## 8. Verification

Scenes in `crates/t3-snapshots/src/scenes/pull_requests.rs`, 1440×900 @2x, dark and light:

| Scene | State |
| --- | --- |
| `pull-requests-loading` | ghost, 7 rows |
| `pull-requests-unavailable` | no capable server |
| `pull-requests-error` | "Could not load pull requests" with the `gh` unauthenticated message |
| `pull-requests-empty` | no pull requests, unfiltered |
| `pull-requests-list` | fixture list: three groups, labels, checks, review glyphs, conflict badge, draft/closed/merged |
