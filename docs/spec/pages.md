# Standalone pages: Usage, Welcome, no-projects hero, project links

Distilled from the fork at `fe7d3092c` (`~/L-Projects/t3UI-refs/t3code-fork`). Paths are relative to
`apps/web/src` unless they start with `packages/`. When this spec and the source disagree, the
source wins. Tailwind: 1 unit = 4px; `text-3xs` 10/14, `text-2xs` 11/16, `text-xs` 12/16,
`text-sm` 14/20, `text-base` 16/24, `text-2xl` 24/32, `text-3xl` 30/36, `text-4xl` 36/40.
The window's minimum width (840) is past `sm` (640) and `md` (768); `lg` (1024), `xl` (1280) and
`2xl` (1536) depend on the main column width, and the web evaluates them against the viewport.

## 1. Shared page chrome (`t3_app::pages::chrome`)

**WorkspacePageHeader** (`components/WorkspacePageHeader.tsx:6-27`): `<header>` with
`h/min-h --workspace-topbar-height` (52px on macOS), `flex items-center gap-3 shrink-0`,
`pl/pr --workspace-gutter-*` (20px at `sm`+), drag region in Electron. While the sidebar is
collapsed the left padding becomes `--workspace-titlebar-content-left` (`workspaceTitlebar.ts:1`,
`ui/sidebar.tsx:169`): `controls-left + control size + gap` = 90 + 28 + 12 = 130px on macOS. The
padding transitions with the panel animation (180ms); the port snaps. `reserveNativeControls`
only applies under `.wco` (Windows), never on macOS.

**WorkspacePageContainer** (`components/WorkspacePageContainer.tsx:5-27`):
`mx-auto flex w-full flex-col gap-6 px-6 pt-6 pb-12` plus a max width: `readable` 896 (settings),
`wide` 1024 (Usage), `expanded` 1152 (Pull Requests).

**WorkspaceBreadcrumb** (`components/WorkspaceBreadcrumb.tsx`): `<ol>` `flex min-w-0 items-center
gap-3 text-sm`; items `font-medium`, current `text-foreground`, others `shrink-0
text-muted-foreground`; separator "/" in `text-icon-muted` (= muted-foreground at default contrast).

**topbar-scroll-fade** (`index.css:408-440`): a mask on the page's scroll container, transparent
at the top fading to opaque over 24px (+1). GPUI has no masks: the port paints the main-column
glass color fading to transparent over the same band. Content starts 24px down (`pt-6`), so the
fade only shows while scrolled.

Main-column direct children with `bg-background` are forced transparent (design-system 2.2), so
pages draw no background of their own and show the main glass.

## 2. Routes and entry points

| Route | Fork | Port |
| --- | --- | --- |
| `/usage` | `routes/usage.tsx` -> `UsagePage` | `Route::Usage` -> `pages::usage::UsageView` |
| `/welcome` | `routes/welcome.tsx:13-62`: `NoProjectsHero` + `WelcomeWizard` overlay | `Route::Welcome` -> `pages::welcome::WelcomeView` |
| `/projects/$projectKey` | `routes/projects.$projectKey.tsx`: `beforeLoad` redirect (replace) to `/settings/projects?project=<key>` | `AppState::navigate` redirects `Route::Project(key)` to `Settings(Projects)` and records `settings_project()`. The settings implementer builds `ProjectSettingsPanel`. |
| `/` with no projects | `routes/_chat.index.tsx:86-88` | `pages::no_projects::NoProjectsHero` |

Sidebar footer (`components/sidebar/SidebarChrome.tsx:102-180`): on non-utility routes, icon
buttons Settings, Pull Requests (when any connected config has `capabilities.pullRequests`), Usage
(lucide `chart-no-axes-column`, tooltip "Usage", navigates to `/usage`). On utility pages
(settings, project, usage, pull requests) a single "Back" button returns to the last main route
(`useNavigateToMainApp`). Escape on Usage also goes back (`useEscapeToGoBack`, `UsagePage.tsx:106`).

## 3. Usage page (`components/usage/UsagePage.tsx`)

### 3.1 State

- Preferences, persisted (`usagePagePreferences.ts`, key `t3code:usage-page-preferences:v1`):
  `metric` in `cost | tokens | limits`, `windowDays` in `1 | 7 | 30 | 90`. First visit:
  `{ metric: "limits", windowDays: 30 }`. The port stores them in `ui-state.json`.
- Window (`packages/shared/src/usageFormat.ts:208-263` `makeWindow`): IANA zone of the viewer
  (UTC on failure); `untilDay` = today in that zone; daily: `sinceDay` = untilDay - (days - 1)
  by calendar arithmetic; hourly (`days == 1`): `untilTime` = now floored to the minute,
  `sinceTime` = untilTime - 24h, days from those instants, `resolution: "hour"`.
- `breakdown`: `model | time`, not persisted, starts `model`.
- `selectedEnvironmentIds`: `null` = all; not persisted.
- `limitsNow`: the clock limits are computed against; advanced only on metric switch to Limits
  and after a limits refresh, never ticking (`UsageLimits.tsx:317-321`).

### 3.2 Data (`apps/web/src/state/usage.ts`, `packages/client-runtime/src/state/usage.ts`)

Every environment answers `server.getUsageSummary(UsageSummaryInput)` for the current window
(stale after 60s, `client-runtime/src/state/server.ts:1079`). Per environment status:
`isPending`, `error` ("This environment could not report usage."), `summary`. Merge
(`packages/shared/src/usageMerge.ts:324-562`, ported to `t3_logic::usage::merge`):
- contract version in `[4, 6]` merges; otherwise excluded as `serverBehind` (lower) or
  `clientBehind` (higher).
- Sources are claimed per fingerprint `(hostId, provider, resolvedHomePath, volumeId)`: `ok` scans
  first, then `partial`, then `failed`; within a status the newest `readAt` wins, environment id
  breaks ties. Losing sources are listed as duplicates (`"<label>: <path>"`). A newer partial scan
  of a directory whose owner is an older `ok` scan contributes cells (`day, hourStart, provider,
  model`) the owner lacks.
- Bucket tokens = uncached + cached + cacheCreation + output (reasoning is a subset of output).
- Sessions come from `source.distinctSessions` of owned sources, never from bucket sums.
- Providers sorted by cost desc; models by cost desc then tokens desc; daily by day; hourly by
  `hourStart`. `costQuality.unpricedShare = unpricedRecords / records`.
- `isPending` = no selected env answered and one is still reporting; `isPartial` = some answered
  and some still reporting (failed ones do not count).

Refresh (`UsagePage.tsx:259-286`): recompute the window; for every selected env call
`server.refreshUsageRates({})` then refetch the summary. Limits refresh (`:214-232`): for every
selected connected env `server.refreshProviders({})`, single-flighted per env, automatic refreshes
skipped within 5 minutes of the last (`client-runtime/src/state/usage.ts:53-84`); auto-runs when
Limits is shown or the set of connected selected envs changes (`:287-302`).

### 3.3 Header (`UsagePage.tsx:304-442`)

`WorkspacePageHeader` with `h-auto` (min 52). Content `grid grid-cols-[1fr_auto] gap-x-3 gap-y-2
py-2`, `xl:flex` (one row). At widths below `xl` the toggles become two compact ghost Selects plus
the refresh button on a second row; the port evaluates `xl` against the main column width.
- Breadcrumb: "Usage" / environment filter (`InlineButton`, `min-w-10`). Label "All environments",
  the one env's label, or "N environments". Trailing 14px slot: `circle-dashed` while selected envs
  scan (usage metrics only), `circle-alert` in `warning-foreground` when an env failed or a
  contract mismatches, else a `chevron-down` shown on hover/open.
- `2xl+`, not Limits: window label `text-xs muted` truncate: "Aug 7 to Aug 11"
  (`formatDayShort`) or "Aug 11, 2 PM to Aug 12, 2 PM" (`formatDateTimeShort`, hourly).
- Right cluster `ms-auto gap-2`: metric ToggleGroup (Cost, Tokens, Limits), period ToggleGroup
  (Past 24h, 7 days, 30 days, 90 days; disabled while Limits, kept mounted so nothing shifts),
  refresh `Button size=icon-sm variant=ghost` with `refresh-cw` 14px (spins while refreshing;
  the port shows it static and disabled). Tooltips: `"<label> (<shortcut>)"`.
- Segmented ToggleGroup (`ui/toggle-group.tsx:32-33`, `ui/toggle.tsx:24,42`): group `gap-0.5
  rounded-lg bg-input/40 p-0.5`; item `h-6 rounded-md px-2.5 text-xs font-medium`
  `text-muted-foreground`, hover `bg-background/55` (dark `input/32`) + foreground, pressed
  `bg-background` (dark `input/72`) + foreground + `shadow-xs/10`.
- Environment menu (`:1011-1127`): checkbox "All environments" (keeps the menu open), separator,
  one checkbox per env with a right status `text-xs muted` (Unavailable in destructive, Update
  required, Scanning..., Refreshing..., Ready; usage metrics only), "No environments connected.",
  "Totals are partial while selected environments scan.", the coverage notice, separator,
  "Model prices" (`sliders-horizontal`) which opens the price overrides dialog.

Keyboard (`usageShortcuts.ts`, `packages/shared/src/keybindings.ts:80-86`, when `usagePageOpen`):
`c` Cost, `t` Tokens, `l` Limits, `mod+shift+1..4` periods (ignored on Limits). Skipped while a
text field, dialog or popup has focus, on repeat, or while the command palette / model picker is
open.

### 3.4 Body (`UsagePage.tsx:451-772`)

`ScrollArea` > `WorkspacePageContainer width="wide"`.
- No selected envs: `text-sm muted` "Connect an environment to see usage." / "Select an
  environment to see usage." (`limits` instead of `usage` on Limits).
- Limits: section 3.6.
- Pending: skeleton (`:1135-1191`): mirrors the layout with `Skeleton` bars (6px radius, `muted`;
  the shimmer is continuous so the port keeps them static).
- Loaded:
  1. Source messages (`text-sm muted mb-4`): unique `source.message` of partial/failed/cursor
     sources without an action.
  2. Summary grid `lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)] gap-6`:
     - Left column `gap-5`: headline `text-4xl font-semibold tabular-nums` (`formatUsd` cost or
       `formatTokens` tokens), under it `text-xs muted` "N sessions" plus " · API estimate" on
       Cost, plus an `info` 12px popover "API estimate excludes X% unpriced records." when
       `unpricedShare > 0`. Then one row per active provider (`providersWithUsage`: tokens or cost
       > 0, in PROVIDER_ORDER), with Cursor "Enable" rows inserted after codex/claude: a `flex
       items-baseline justify-between gap-4` line of 8px dot in the provider color, 16px provider
       mark, label `text-sm`, `text-2xs muted` "N session(s)", and right `text-sm font-medium`
       value; below `text-xs muted` "X% of cost · 1.2M tokens" (Cost) or "X% of tokens · $1.23".
     - Right column `gap-3`: `h2 text-sm font-medium` "Daily cost" / "Daily processed tokens" /
       "Hourly ..." then the chart (3.5).
  3. Totals: `h2` "Totals", `grid grid-cols-2 md:grid-cols-5 gap-x-6 gap-y-4 py-1` of Metric
     (`text-xs muted` label over `text-base font-medium tabular-nums` value): Processed tokens,
     Cached input, Uncached input, Output, Cache savings.
  4. Breakdown: header row with `h2` "Breakdown" and a segmented toggle (Model, Day|Hour). Table
     `text-sm table-fixed`; head row `border-b text-xs muted`, cells `py-2 font-normal`; body rows
     `border-b border-border/50 hover:bg-muted/50`, cells `py-2`.
     - Model (cols 40/20/20/20%): Model (14px mark + name, `gap-2`), Cost (or muted "Unpriced"),
       Share (or "—"), Tokens. Sorted by tokens on the Tokens metric.
     - Time (40% then `60/(providers+2)%` each): Day|Hour, one column per active provider (cost),
       Total, Tokens. Newest period first.
     - Empty: one row `py-6 text-center muted` "No activity in this window."

Provider presentation (`usageProviders.ts`): codex "Codex" `--contrast-foreground` OpenAI mark;
claude "Claude Code" `#d97757`; grok "Grok Build" foreground 72% over background; cursor "Cursor"
`#8b8b8b`; opencode "OpenCode" `#5b9bbd`; antigravity "Antigravity" `#8c7bd1`.

Formatting (`packages/shared/src/usageFormat.ts`): `formatUsd` en-US currency, 2 decimals;
`formatTokens` 3 significant figures with K/M/B/T (`19.9B`, `76.7M`, `804K`, `950`);
`formatPercent` 1 decimal, `<0.1%` for tiny shares; `formatCount` grouped integer.

### 3.5 Chart (`UsageProviderChart.tsx`)

- Periods: every day in the window, or every hour start in the rolling 24h.
- Columns: per period, per provider value (tokens or cost). Scale max = `niceScale(peak, 4)`:
  peak is the largest single provider-period value; step `{1,2,5,10} x 10^n` at or above
  `peak/4`; max rounds the peak up to a step; ticks every step from 0.
- Geometry: y-axis label column `w-14` (56px) + `gap-2` + plot `h-56` (224px) flex-1. SVG
  viewBox 960x260 stretched (`preserveAspectRatio=none`), `PLOT_TOP` 8 viewBox units:
  `y = 260 - v/max * 252` (all 260 when max is 0). Strokes are non-scaling.
- Gridlines at each tick: 1px `border` color. Tick labels `text-3xs muted tabular-nums`,
  right-aligned, vertically centered on the line; "0" for zero, else formatted.
- Series: one per active provider, painted heaviest total first; monotone cubic (Fritsch-Carlson,
  `:72-111`) through `(i * 960/(n-1), y)`. Fills first (provider color at 12% opacity, closed to
  the baseline), then 2px strokes in the provider color.
- X labels: `pl-16 flex justify-between text-3xs muted uppercase`: first, middle
  (`floor(n/2)`), last period ("AUG 7" / "2 PM").
- Hover: nearest period index by x; a 1px `muted-foreground` vertical line from PLOT_TOP to the
  baseline; a tooltip card (`surface-glass rounded-xl border-border/50 px-2.5 py-2 text-xs
  shadow-lg min-w-36`) offset 12px from the pointer, flipped to stay inside the plot: period
  label (`formatRelativeHourShort` "2 PM today" / "yesterday" for hours), one row per provider
  (12px mark + label muted, value foreground), then a `border-t pt-1 mt-1` Total row.

### 3.6 Limits (`UsageLimits.tsx`, `UsageLimitsPooled.tsx`, `packages/shared/src/usageLimits.ts`)

Source: `ServerConfig.providers[].usageLimits` of every selected env (enabled, installed,
available, with `usageLimits`) plus `ServerConfig.usageLimitSources` accounts. Logic is ported to
`t3_logic::usage::limits`:
- Accounts keyed by `driver:email` (lowercased), else `driver:credential:<fingerprint>`, else
  per instance; duplicates merge (environments unioned, freshest `checkedAt` supplies windows,
  freshest credit read supplies credits). Providers/accounts whose `limitsNotice` is non-null are
  skipped: unsupported ("This account has no subscription limits."), probe failed ("Could not read
  limits."), or no windows ("No limits reported.").
- Pools by driver; windows pooled by `kind:id`, ordered session, weekly, monthly, other.
  `remainingPercent = round(100 - mean(used))`. Pace over windows with a clock: `gap = used -
  elapsed*100`, `> 5` ahead, `< -5` under, else on. Accounts ordered by the first window kind's
  reset time, then name, then key. Resets sorted by time, each restoring `round(used / members)`.
- Layout: `flex flex-col gap-8`. Empty: "No provider on the selected environments reports
  subscription limits." Per pool a section `gap-3`: `h2 text-sm font-medium` with a 20px provider
  icon and the driver label (Codex, Claude, Cursor, ...); per window a card `rounded-lg border
  border-border/60 p-4 grid md:grid-cols-[11rem_minmax(0,1fr)] gap-x-6 gap-y-3 items-center`:
  - left `gap-1`: label `text-sm font-medium`; `text-3xl font-semibold tabular-nums` "N%" +
    `text-sm muted` "left" + pace icon (14px `trending-up` / `gauge` / `trending-down`, muted,
    tooltip); with more than one account `text-xs font-medium` "↻ +N%".
  - right: the pool bar, `grid gap-1` with one equal column per account. Each segment
    `h-5 rounded-md bg-muted overflow-hidden` (`h-8` when the bar is at least 672px wide):
    fill `opacity-35` provider color at `remaining%` width; the spent share is hatched
    (135deg, 1px color, 5px period, 20%) when a reset is known; narrow: centered index
    `text-3xs font-semibold foreground/80`; wide: account name (more than one account), `N%`
    `font-semibold`, and a right plate `rounded-sm bg-background/85 px-1.5 py-0.5 text-2xs` with
    "↻ 2h 13m" and a ticket + credit count. Narrow bars also list legend rows below.
  - Cursor windows get fixed labels and descriptions (`usageLimits.ts:57-73`).
- Bar color (`UsageLimits.tsx:48-52`): codex -> Codex color, claudeAgent -> Claude color, else
  foreground.
- Segment popover (hover or click): name, email (blurred, click to reveal), Plan, Signed in/Via,
  Left, Resets "<time> · in 2h 13m", Restores "+N% of pool", and "Use reset" with a confirm
  dialog ("Use a reset credit?") calling `provider.consumeResetCredit`.
- External usage links (`collectExternalUsageLinks`): a `rounded-xl border p-4` row with the link
  label, message, and a ghost-muted xs "Manage usage" button opening the URL.
- Notices: a warning Alert listing `"<env> · <name>: <notice>"` lines.

### 3.7 Not ported yet

Model price overrides dialog (`UsagePriceOverrides.tsx`), ChatGPT shared-usage row (needs
`auth.subscriptionSharing`), Cursor Keychain enable flow, segment popovers and the reset credit
redeem, the refresh spin.

## 4. No-projects hero (`components/NoProjectsHero.tsx`)

`SidebarInset` > column: `WorkspacePageHeader electron` (an empty 52px drag strip, no border) then
`Empty size="hero" flex-1` (`ui/empty.tsx:11`: centered column, `gap-6 p-12`, title `text-3xl`
at `sm`+). Inner `w-full max-w-lg (512) px-8 py-12`, `EmptyHeader max-w-none` (centered text):
- Title "What should we work on?" `font-semibold text-3xl` (30/36).
- Description `text-sm muted`: "Add a project to start your first thread." when no environment can
  host a scratch thread, else "Add a project, or start without one."
- Buttons `mt-6 flex justify-center gap-2`: primary sm "Add project" (`plus` 16px) opens the
  command palette in add-project mode; outline sm "Start without a project"
  (`message-square-dashed`) starts a scratch thread (only when a scratch environment exists).

## 5. Welcome (`routes/welcome.tsx`, `components/onboarding/WelcomeWizard.tsx`)

The route renders `NoProjectsHero` with the wizard dialog over it. `FirstRunGate`
(`FirstRunGate.tsx`) sends a fresh install (no `onboardingCompletedAt` client setting and a
fresh workspace) to `/welcome` before the app tree mounts. Finishing or skipping writes
`onboardingCompletedAt` and navigates (replace) to a new draft in the landing project or `/`.

Dialog: `WizardPopup size="wide"` (max-w-3xl 768, `rounded-2xl border`, dialog glass), no close
button, not dismissable by outside click or Escape.
- Header (`DialogHeader`: `p-6 pb-3 gap-2`): T3 wordmark (h 16) + "Code" `text-2xl font-medium
  tracking-tight muted`; then WizardSteps (`ui/wizard.tsx:70-138`): `grid grid-flow-col
  auto-cols-fr gap-1 rounded-xl p-1 bg-zinc-25 ring-1 ring-black/5` (dark `white/4`, ring
  `white/5`); each step `rounded-lg px-2.5 py-2 gap-2`, current `bg-card shadow-xs ring-1`; badge
  20px circle `text-sm font-medium`: done `bg-primary` + check, current `bg-primary/10
  text-primary ring-primary/30`, upcoming `bg-card muted ring-black/10`; label `text-sm
  font-medium` (current foreground, else muted). Steps: Connect, Agents, Projects. Earlier steps
  are clickable.
- Panel (`ui/wizard.tsx:140-155`): `px-6 py-5 bg-zinc-25/80 ring-1 ring-black/5` (dark
  `white/2`), height animates between steps.
- Step 1, Connect (`:268-430`): `h1 text-2xl font-semibold tracking-tight` "Connect your
  computers", `p mt-2.5 text-sm leading-relaxed muted` "Choose one or more computers. We'll set up
  agents and projects on each." Then one checkbox row per environment (`rounded-lg border px-3
  py-3 gap-3`: checkbox, `monitor` 16px muted, label `text-sm font-medium`, right `text-xs muted`
  "Connected" / "Connecting...", URL below `text-xs muted`); the T3 Connect row (when cloud is
  configured); "Add a computer" collapsible (`link` icon, ghost, `min-h-14`) with the pairing form
  ("Pairing link" input, placeholder `https://your-server:5230/pair#token=…`, "Need a pairing
  link?" with `npx t3 pair`, "Pair"). Footer `mt-6 justify-end`: primary "Continue" + `arrow-right`
  14px, enabled when every selected env is connected.
- Step 2, Agents (`:617-1050`): "Connect your agents" / "Choose an agent to start coding. You can
  add more later." Per env a section (`h2 text-sm font-medium` env label) with Codex (managed
  setup or the existing-CLI card) and Claude Code cards (`rounded-lg border px-4 py-4 gap-3`:
  20px logo, name `text-sm font-medium`, status `text-xs muted`; right: "Ready" with check in
  `success-foreground`, "Checking...", "Disabled", or an xs ghost "Install"/"Sign in" button that
  opens an inline terminal pre-typed with the install/login command). "Continue".
- Step 3, Projects (`:1050-1720`): `agentSessions.scan` per env; "Choose your projects" / "Import
  projects and conversations from your selected computers.", "N of M selected" with Select
  all/none, grouped repositories with tri-state checkboxes, per row source icons, thread count
  and age; "Do not import projects" (ghost-muted) and "Import N projects". Importing runs
  `project.create` then `agentSessions.import` per selection.

### 5.1 Port status

Step 1 with connected environments only; steps 2 and 3, pairing inside the wizard, T3 Connect,
and FirstRunGate are not ported yet.
