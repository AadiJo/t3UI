# Composer build spec

> Refreshed against fe7d3092c. Overlaps the refreshed `chat.md` §6.1-6.3 (overlay, hero, inset),
> which point here for everything inside the composer stack; `chat.md` §4 (error banners), §5
> (timeline slot) and §8 (file drop overlay) stay chat-owned. Two research distillations (pickers /
> strip / shortcuts, and the card / editor / send pipeline) were folded in after spot
> re-verification.

The composer is the card at the bottom of the chat column, everything attached to it (drawers above,
the context strip below, the command menu, pickers), and the send/steer/queue/stop pipeline. The
timeline, header and right panel are in `chat.md` / `panels.md`.

## 0. Conventions

- **Paths** are relative to `~/L-Projects/t3UI-refs/t3code-fork/apps/web/src/` unless prefixed:
  `contracts:` = `packages/contracts/src/`, `shared:` = `packages/shared/src/`,
  `crt:` = `packages/client-runtime/src/`. `CC` = `components/chat/ChatComposer.tsx`,
  `CV` = `components/ChatView.tsx`, `ED` = `components/ComposerPromptEditorTiptap.tsx`.
- **Desktop only.** Window >= 1100px, so every `sm:` variant applies. Phone-only paths (`max-sm`,
  the "collapsed mobile" composer, the hero view-transition) are out of scope; they are named only
  where code shares a branch with desktop.
- **Units.** 1 Tailwind unit = 4px. Shorthand like `px16`, `h28`, `gap6` is already in px.
  `rounded-md` 8, `rounded-lg` 10, `rounded-xl` 14, `rounded-2xl` 18, `rounded-3xl` 22
  (`--radius` 10, `index.css:1030,265-270`). `--control-radius` 8 (`index.css:92`).
- **Type.** Sans = system stack `-apple-system, BlinkMacSystemFont, "Segoe UI", system-ui,
  sans-serif` (`index.css:157`); mono = `ui-monospace, "SF Mono", "SFMono-Regular", Menlo,
  Consolas, "Liberation Mono", monospace` (`index.css:158-159`). No fonts are bundled any more
  (the July DM Sans / JetBrains Mono assets are gone).
  `text-xs` 12/16, `text-sm` 14/20, `text-2xs` 11/16, `text-3xs` 10/14, `text-4xs` 8/8,
  `text-5xs` 7/7 (`index.css:168-175`). `leading-relaxed` 1.625, `leading-snug` 1.375.
- **Colors** are token names (`--muted-foreground`, `secondary-label`, `sidebar-row-hover`,
  `placeholder` ...). `X/NN` = token at NN% alpha. Values live in `docs/spec/tokens.json`.
- **Tooltips** open after 600ms (Base UI default, `@base-ui/react/tooltip/utils/constants.js`),
  close at 0, side top, offset 4, unless stated. Popup: radius 8, 1px border, bg `popover`,
  12px, px8 py4, max-w 320, `shadow-md/5`, scale .98 + fade enter/exit (`ui/tooltip.tsx:40-52`).
- **Glass.** GPUI has no backdrop blur (see `docs/handoff/chat.md`). Every "glass" surface below
  falls back to its solid surface color, which is also what the fork paints when
  `backdrop-filter` is unsupported. The glass recipe is still given so a native-blur window can
  match it later.
- **Animations.** `--ease-drawer` = `cubic-bezier(0.32, 0.72, 0, 1)` (`index.css:178`). Durations
  in ms. "Continuous" marks a repainting animation (AGENTS.md rule).

### 0.1 Primitive deltas the composer uses (vs `chat.md` 0.1, which is stale)

- Button (`ui/button.tsx:10-66`): radius 8 (`--control-radius`), 1px border, `font-medium`, text 14,
  gap 8, active scale .97, disabled opacity 64%, svg 16 default. Sizes on desktop: `xs` h24 px7
  gap4 12px svg 14; `sm` h28 px9 gap6; `icon-xs` 24² svg 14; `icon-sm` 28²; `icon` 32².
  Variants used here: `ghost` (hover/pressed bg `accent`, svg `muted-foreground`), `ghost-muted`
  (fg `muted-foreground`, hover bg `accent` + fg `foreground`), `outline` (border `input`, bg
  `popover`, dark `input/32`, `shadow-xs/5`, hover `accent/50`, dark `input/64`), `default` (bg
  `primary`, inset top highlight white/16%), `overlay` (bg black/70, fg white/65, hover black/90),
  `media-close` (bg black/65, fg white, `shadow-sm`, 1px ring white/20, hover black/80).
- `dropdown-glass` (`index.css:393-406`) is the chrome of every menu, select, combobox and
  popover: bg = `popover` mixed to ~84% alpha, blur 12 / saturate 1.14 (dark 16 / 1.08), 1px border
  `contrast-foreground`/10%, radius 10, shadow `0 16px 40px -18px rgb(0 0 0/55%)` (dark
  `0 18px 44px -18px rgb(0 0 0/80%)`) (`ui/popover.tsx:76-82`). Solid fallback: `popover`.
- Menu (`ui/menu.tsx`): glass popup, min-w min(160, 100vw-32), inner p4. Item / radio item:
  min-h28 px8 py4 radius 6 14px gap8; highlighted bg `accent`; radio items have no check
  indicator, checked = bg `foreground`/8%. Group label px8 py6 12px `font-medium muted-foreground`.
  Separator mx8 my4 1px `border`.
- Combobox item (`ui/combobox.tsx:186-212`): `flex min-h28 px8 py4 radius 6 14px`, inner gap8,
  hover/highlight bg `sidebar-row-hover`, selected bg `sidebar-row-selected`. Search input
  (`:109-127`): wrapper px12 pt10; underline box `border-b border/70 pb6`, shifted -1px,
  focus-within `border-ring`; `SearchIcon` 16 `muted-foreground/55` at top6 left0; input h26 ps20
  14px sans, transparent.
- Command item (slash menu rows, `ui/command.tsx:157-175` over `ui/autocomplete.tsx:124-131`):
  `flex min-h28 gap8 radius 6 px8 py6 14px`, svgs `muted-foreground`; when the composer drives the
  highlight (`active` prop) the primitive hover is disabled and the active row gets bg
  `sidebar-row-hover`. List `p8 scroll-py-8`.
- Kbd (`ui/kbd.tsx:5-15`): h20 min-w20 radius 4 px4 12px `font-medium muted-foreground`, bg
  `sidebar-control-surface`, 1px ring `sidebar-border`.
- Badge (`ui/badge.tsx:10-34`): `sm` h16 min-w16 radius 4 px3 10px leading-none; `secondary` bg
  `secondary`; `outline` border `input` bg `background` (dark `input/32`).
- Spinner (`ui/spinner.tsx`): lucide `LoaderCircleIcon`, `visible-animate-spin` (rotates only while
  on screen). Continuous; keep tiny and stop offscreen.

## 1. Placement in the chat view

`CV:10055-10300`.

```
overlay[data-chat-composer-overlay]  absolute inset-x-0 bottom-0 z-20 pt8, pointer-events none
│   (draft hero state: absolute inset-0 flex items-center, vertically centered, see 1.1)
└─ gutter  w-full ps/pe = --workspace-gutter (20px at >=640) + safe area   (CV:10067)
   └─ stack[data-chat-composer-stack]  relative z-10 mx-auto w-full max-w = --chat-max-width,
      │    pointer-events auto                                              (CV:10069-10072)
      ├─ [hero only] headline  absolute inset-x-0 bottom-full, pb32 (pb16 when a shoulder tab
      │    such as the stash badge is shown)                                (CV:10073-10091)
      ├─ Shell (composer-shell)                                             (2.1)
      │   ├─ Host → ChatComposer <form>                                     (2.2-2.4)
      │   └─ context strip (BranchToolbar), when mounted                    (11)
      └─ bottom spacer  h20 (+ safe-area-inset-bottom)                      (CV:10294-10297)
```

- `--chat-max-width` = 768px (setting `chatWidth` "comfortable", default), 1152px ("wide"),
  100% ("full") (`index.css:2116-2128`, `contracts:settings.ts:295,306`).
- The form itself is `mx-auto w-full min-w-0 max-w-(--chat-max-width)` (`CC:6135-6198`).
- **Timeline inset.** The overlay height is published to the timeline as its end inset through
  `resolveComposerTimelineInset` (`components/composerFooterLayout.ts:82-90`, called at
  `CV:5996-6013`): expanded = overlay height; resting = max(current inset, overlay height + 94).
  The 94 constant (`COMPOSER_RESTING_EXPANSION_MIN_PX`, `:68`) is what an empty composer grows by
  when it leaves the resting layout, so re-expanding never covers the last rows. The inset resets
  per thread (`CV:6024-6030`).
- The composer publishes the destination height once per resting transition (`CC:520-528`) rather
  than per frame.

### 1.1 Draft hero state

A local draft with no timeline entries and no work in progress centers the composer vertically with
a headline above it (`ChatView.logic.ts:255-276`, `CV:3719-3727`). Leaving it (first send, a setup
card, background submission) docks the composer at the bottom. Headline
(`components/chat/DraftHeroHeadline.tsx:330-385`):

- `h1` centered, 30px (`sm:text-3xl`, line-height 36) `font-normal tracking-tight foreground`,
  container `max-w-5xl` (1024).
- Text: "What should we build in {project picker}?" with a project; "{project picker} to start" when
  projects exist but none resolved; "Add a project to start" with none; "What should we work on?"
  for a no-project ("scratch") draft.
- Project picker: `InlineButton tone="picker"` (dotted underline `foreground/30`, solid on hover or
  open, `ui/button.tsx:118-140`), max-w 256, label = project name, "No project" or "Choose a
  project". Menu align center, max-h 320, radio items (favicon 16 + name + environment badge when
  projects span environments), separator, "Add project" (`FolderPlusIcon`).
- Under the h1 (only when a scratch workspace exists): `mt8 h24 flex items-center 14px` row with
  `InlineButton tone="muted"` "or start without a project" (tooltip bottom = `chat.newWithoutProject`
  shortcut, mod+alt+N), or the project picker when the draft is already scratch.
- With default `panelAnimationDurationMs` = 0 (`contracts:settings.ts:110-119`) the dock is
  instant; non-zero values run a FLIP over that duration.

## 2. Card geometry

### 2.1 Shell (`components/chat/ComposerSurface.tsx:6-37`)

- `@container/composer-surface relative isolate mx-auto w-full max-w-(--chat-max-width)`.
- CSS vars set here: `--chat-composer-drawer-inset` 22px; `--chat-composer-glass-surface` =
  `card` (dark: `surface-raised`; custom themes: `--app-theme-surface-raised`);
  `--chat-composer-outline` = rgb(0 0 0/8%) (dark: white 5%); dark only
  `--chat-composer-highlight` = white 3%.
- `::before` = the one glass backdrop: inset 0, radius 22, z0, bg glass-surface at
  `--glass-opacity` (80%), blur `--glass-blur` (12, dark 16), saturate `--glass-saturation`
  (1.14, dark 1.08) (`index.css:106-109,135-136`; the appearance setting `glassOpacity` rewrites
  `--glass-opacity`, `routes/__root.tsx:312`). Hidden when any attached drawer exists (then the
  Main surface paints its own glass, 2.2).
- With the context strip, the backdrop extends `--chat-composer-context-extension` (32px) below the
  card and follows a clip-path that keeps the 22px card corners, steps in by the 22px drawer inset,
  and rounds the strip's bottom corners at 16px (`:25-31`). GPUI: draw the card and the strip as
  two solid shapes (2.2 and 11.1); the union is the same silhouette.

### 2.2 Host and Main

- **Host** (`:46-60`): `relative z-10 w-full rounded-3xl`, shadow `--shadow-composer`
  = `0 12px 28px -18px rgb(0 0 0/40%)` (`index.css:181`), none in dark mode. `::after` (z1) is the
  outline: inset 0, radius inherit (22), 1px border `--chat-composer-outline`; dark adds
  `inset 0 1px --chat-composer-highlight`. With a context strip the outline's bottom edge is open
  between x=22 and x=width-22 (2px tall cut, `:43-44`) so it merges into the strip's outline.
  When an attached drawer exists the Host drops its shadow and outline.
- **Main** (`:62-80`, mounted at `CC:6390`): `relative z-10 rounded-3xl p1` (1px), transition
  colors 200ms. With an attached drawer it gets the glass bg, `shadow-composer` (light) and its own
  outline `::after` (z20).
- **Surface** (`CC:6393-6404`): `rounded-3xl`, transition background-color 200ms.
  - Drag-over, only for in-app file-tree mention drags (MIME `application/x-t3code-composer-mention`,
    `chat/composerMentionDrag.ts`): bg `accent/45` + 1px ring `primary/70`. OS file drags show the
    chat-column overlay instead (7.2).
  - Project selection required (draft whose project vanished): opacity 75%.
  - Ultrathink active: `composerSurfaceClassName` (inset 1px white/7% shadow) and the Main gets
    `ultrathink-frame` (10.3).
- **Solid fallback for GPUI:** card bg = `card` (dark `surface-raised`) at 100%, radius 22,
  1px outline as above, shadow as above in light mode only.

### 2.3 Body, editor slot, footer (expanded layout)

```
Main (p1)
└─ Surface
   ├─ Body[data-chat-composer-body]  relative px16 pt16 pb8            (CC:6457-6466)
   │   (approval pending: pb16; resting: py8)
   │   ├─ command menu / stash menu layers (portaled, 6.6 / 13.3)
   │   ├─ media strip   mb12 flex gap8 (wrap; horizontal scroll when snap-shots)   (7.6)
   │   ├─ file rows     mb12 flex-col gap4                                         (7.6)
   │   └─ editor box    relative (resting: flex items-center gap4 + right padding) (4)
   ├─ prompt-length error  px16 pb8 12px destructive, role alert       (CC:6988, 8.7)
   └─ Footer[data-chat-composer-footer]  flex nowrap items-center justify-between
       px16 pb16; gap 0 (compact: 6); pt8 while questions are pending   (CC:6993-7101)
       hidden while an approval is pending
       ├─ left[data-chat-composer-controls=left]  relative -m4 -ms14 p4 ps14 flex min-w0 flex-1
       │    items-center gap4 overflow-x auto, scrollbar hidden          (10)
       └─ right[data-chat-composer-actions=right]  flex shrink-0 items-center justify-end gap8
            [attach button] [context meter, legacy] [primary action]     (8.1)
```

- Footer compact flags (`composerFooterLayout.ts:1-24,100-108`, measured on the form width with a
  ResizeObserver, `CC:3209-3258`): footer compact below 620px form width (780 when "wide actions"
  exist: a plan follow-up or pending questions); primary actions compact only with wide actions and
  width < 780. Compact footer = gap 6. On desktop the form is <= 768 wide, so a pending question or
  plan follow-up always compacts the primary actions.
- Left-cluster fitting (which controls go icon-only or into the overflow menu) is a measured
  algorithm, not a breakpoint (10.6).
- **Attach button** (`CC:7027-7062`): shown when file staging is possible
  (`fileAttachmentStagingLimit` non-null) and, during a question, only when the server supports
  question attachments and the question allows custom answers. Button `ghost icon-sm` (28², radius
  8), `PaperclipIcon` 16, aria and tooltip "Attach files" (600ms). Pointer-down does not steal
  editor focus. Opens a hidden `<input type=file multiple>`; GPUI: native open panel, multiple
  selection, any file type.

### 2.4 Heights (desktop, default 14px prompt font)

- Editor box min 78px / max 208px border box with `-m4 p4` (`ED:758`), so **70-200px in flow**,
  then it scrolls. Line height 22.75 (14 x 1.625). The comment at `composerFooterLayout.ts:62-67`
  assumes a 48px footer (144px card); the classes give 44px, so match the classes.
- Empty expanded card: 2 (Main padding) + 16 + 70 + 8 (body) + 28 + 16 (footer) = **140px**.
- With a pending approval: 2 + 16 + 32 (editor min 40 minus 8) + 16 = **66px** (no footer).
- Resting row: 2 + 8 + 32 + 8 = **50px** (3).
- The context strip adds 32px below the card (11.1). Idle git thread overlay:
  8 (overlay pt) + 140 + 32 + 20 (spacer) = 200px.

## 3. Resting composer (collapse on scroll)

`composerFooterLayout.ts:26-60`, `CC:4739-4959,6836-6846,6993-7019`, `useComposerRestingTransition`
`CC:409-847`.

- **When.** `shouldUseRestingComposerLayout`: existing server thread, desktop, the timeline
  overflows, the user scroll-collapsed the composer, the prompt is single-line (no `\n` and not
  soft-wrapped, `useComposerMultilinePrompt`), and no expanded chrome. Expanded chrome
  (`CC:4739-4751`): an approval/question/plan drawer, tasks drawer open, command menu open, stash menu
  open, drag-over, preparing worktree, no provider, project selection required, environment
  unavailable, a validation error, or an image that failed to persist locally. Losing focus never
  rests the composer.
- **Scroll collapse** (setting `composerCollapseOnScroll`, default true, `contracts:settings.ts:452`;
  `CC:4861-4959`, `chat/composerScrollGesture.ts`). Capture-phase wheel listener on the document:
  - Only wheel events over the timeline count, and only when the timeline can scroll that way and
    the gesture is not scrolling toward the logical end.
  - Deltas accumulate (line mode x16, page mode x client height). 24px accumulated collapses. The
    gesture resets after 120ms without events. An editor change suppresses the rest of the current
    gesture.
  - Keys: PageUp/Home collapse when scrollTop > 1; PageDown/End when not at the logical end
    (`shouldCollapseComposerForScrollKey`, routed from the timeline via
    `collapseForTimelineScrollKey`).
  - Not eligible with a multiline prompt, expanded chrome, or the inline tasks badge.
- **Expands again** on any pointer-down inside the form that is not on a control (a click on the
  resting padding focuses the editor without moving the caret, `CC:6137-6156`), focus entering the
  form (except the re-focus that follows window activation, `CC:4877-4899`), any editor change or
  visible selection change, reaching the timeline end (`restoreAfterTimelineReachedEnd`), the
  "Scroll to end" pill, `/model` and the shortcuts that open a control (`openControl`).
- **Resting layout** (`CC:6836-6846,6912-6925,6993-7019`):
  - Body `py8`. Editor box becomes `flex min-w0 items-center gap4` with right padding reserved for
    the overlaid actions: 112px when the legacy context meter shows, 80px when the attach button
    shows, else 48px.
  - Editor: `my0 h32 (min = max = 32) overflow hidden py0 whitespace-pre leading-32`; the
    placeholder is one line, vertically centered, `whitespace-nowrap`.
  - Up to 3 image thumbnails sit right of the editor (`CC:4786-4847`): button 28² radius 8 1px
    `border/70` bg `muted/60`, `object-cover`, cursor zoom-in, aria "Preview {name}"; overflow
    button 28² "+N" 12px `font-medium secondary-label tabular-nums` (hover bg `muted`), aria "Show N
    more image attachments", which expands the composer. Container `flex gap4 ps4`.
  - Footer becomes `absolute bottom1 right1 z10 h48 w-auto gap0 py0`; its left cluster is hidden.
    The model picker, traits and mode controls move to the context strip (11.3) at size `xs`.
- **Transition.** Duration = `panelAnimationDurationMs` (default **0**, so instant), easing
  `cubic-bezier(0.32, 0.72, 0, 1)` (`CC:403-407,4848-4858`). When animated: the card height tweens
  old→new; the prompt and the action cluster slide from their old offsets (translateY); on
  expansion the prompt reveals extra lines with a bottom clip-path; arriving controls fade in with a
  4px drift (collapse: immediately over the full duration; expand: second half only); arriving image
  previews fade the same way. The overlay is pinned to the destination height during the tween.
  Skip on first layout.

## 4. Editor

`components/ComposerPromptEditor.tsx`, `ED`, `composer-rich-text.ts`, `composer-rich-text-doc.ts`,
`composer-list-continuation.ts`, `composer-logic.ts`, `composer-editor-mentions.ts`,
`shared:composerInlineTokens.ts`, `shared:composerContextReferences.ts`, `index.css:2256-2356`.

### 4.1 Model

- **Source of truth is a Markdown string** (the draft `prompt`). The Tiptap document is rebuilt
  from it on external changes and serialized back on every edit. Rich mode and plain mode use the
  same engine; plain mode just has no marks, so `**x**` stays literal (`ED:580-587,787-826`).
- Setting `composerRichTextEnabled` (default **true**, `contracts:settings.ts:454`). Flipping it
  remounts the editor; the prompt survives.
- Three cursor spaces (`composer-rich-text-doc.ts:9-24`): flat document offsets (style markers
  excluded, chips = 1), **collapsed** offsets (markers literal, chips = 1; the draft store and
  trigger code use this) and **expanded/markdown** offsets (chips expand to their source). Trigger
  detection uses the expanded cursor.
- Inline atoms in the string (each is one chip in the editor, 5):
  - File mention: `[basename](percent-encoded path)`; legacy `@path` / `@"quoted path"`.
  - Skill: `$name` (any Unicode currency sign works as the sigil, `\p{Sc}`).
  - Context reference: `[label](t3-context://v1/<kind>/<contextId>)`, images as `![label](...)`
    (`shared:composerContextReferences.ts:10-60`). Label sanitized: brackets, backslashes and line
    breaks become spaces, collapsed, trimmed, max `COMPOSER_CONTEXT_LABEL_MAX_CHARS`, empty → the
    kind.
  - Assistant citation: `[Assistant quote](t3-citation://v1/...)` (`shared:assistantCitations.ts`).
  - Mentions and skills become chips only once whitespace follows; context links and citations are
    chips wherever they are.
- `origin/composer:crates/t3-logic/src/composer/prompt.rs` already implements this grammar against
  fe7d3092c (see 16).

### 4.2 Box and typography

- Container (`ED:1322-1327`): `relative flow-root`, font `--font-composer` (default `--font-sans`),
  size `--font-size-prompt` (default 14px; Settings → Appearance can change it,
  `appearanceFonts.ts:101,113`).
- Editable (`ED:755-782`): `composer-tiptap -m4 block min-h78 max-h208 overflow-y-auto p4
  whitespace-pre-wrap break-words bg-transparent leading-relaxed foreground`, no focus outline.
  Paragraph margins 0. role textbox, aria-multiline, aria-label "Message", aria-placeholder,
  `aria-readonly` when disabled.
- **Placeholder** (`ED:1371-1380`): absolute inset0, same font, `leading-relaxed`, color
  `placeholder/75`, shown only when the value is empty and there are no context records. Text
  (`CC:6930-6946`), first match wins:
  1. Approval pending: "Resolve this approval request to continue"
  2. Question pending, choice-only (`allowCustomAnswer === false`): "Choose an option above"
  3. Question pending: "Type your own answer, or leave this blank to use the selected option"
  4. Plan follow-up with a proposed plan: "Add feedback to refine the plan, or leave this blank to
     implement it"
  5. Draft without a project: "Choose a project above to start a thread"
  6. No provider: "Enable a provider in Settings to send a message"
  7. Phase disconnected: "Ask for changes, send follow-ups, or attach images"
     (`composerPlaceholder.ts:1-2`)
  8. Default: "Ask anything, @tag files/folders, $use skills, or / for commands"
- **Value shown** (`CC:6899-6905`): "" while an approval is pending; the active question's custom
  answer while a question is pending; else the draft prompt.
- **Disabled** (read-only, caret hidden): connecting, approval pending, project selection required,
  choice-only question, or answering in flight (`CC:6947-6953`).

### 4.3 Rich text (when `composerRichTextEnabled`)

- Marks: bold `**`, italic `*` (also `_`), strike `~~`, inline code `` ` `` (single backtick, one
  line); unmatched or escaped markers stay literal (`composer-rich-text.ts:1-80`). Nesting order
  strike > bold > italic > code. `__bold__` normalizes to `**bold**`, `[X]` to `[x]`.
- Styling: bold/italic/strike as usual; inline code = mono 0.92em, bg `muted-foreground`/12%,
  radius .3em, padding .05em .25em (`index.css:2272-2278`).
- **Marker reveal** (`ED:476-572`, `index.css:2280-2289`): while the caret is inside (or the
  selection overlaps) a styled range, its markers render as non-editable widgets at the range edges:
  `muted-foreground`, opacity .75, weight 400, normal style, no decoration, not selectable.
- **Task lists**: typing `- [ ] ` / `- [x] ` at a line start makes a task item. Rendered as a real
  checkbox 15.2px (0.95rem) in a 1lh-tall label, gap 8, items `flex items-start`; nested lists
  indent 22px (`index.css:2311-2356`). Space toggles the focused checkbox; Enter on it does nothing
  (`ED:942-947`). Enter inside a task item splits it (new unchecked item) or lifts an empty one out
  of the list (`ED:959-967`).
- Disabled in rich mode: headings, lists other than tasks, blockquote, code blocks, links,
  horizontal rules (`ED:787-803`).

### 4.4 Editing behavior

- **Enter** (`ED:938-974`, `CC:4011-4096`): IME composition swallows it. Otherwise
  `onComposerCommandKey`: open menu → select item; else a send intent (8.2) → submit; else list
  continuation (`listContinuationForEnter`: continues `- `, `* `, `1. ` style items, an empty item
  exits the list); else split the paragraph (keeping marks) and scroll the caret into view.
- **Tab** with an open menu selects the active item; on a list line indents by two spaces; Shift+Tab
  is not handled (native focus move). The settings copy for plan mode promises "Shift+Tab", but no
  handler exists at fe7d3092c (18).
- **Arrow Left/Right** next to a chip jumps over it as one unit (`ED:887-911`). Arrowing into or
  clicking a chip makes a node selection: 2px `ring` outline offset 1 radius 6
  (`index.css:2291-2295`); a text selection spanning chips paints `Highlight` at 30% over each chip
  (`index.css:2297-2309`). Backspace after a chip deletes the whole chip.
- **Arrow Up/Down** go to the menu when open, else prompt history (13.1).
- **Home/End on macOS** move to the visual line boundary (soft wraps), Shift extends (`ED:849-886`).
- **Surround** (`ED:994-1020`): typing one of `( [ { ' " “ \` < « * _` with a non-empty selection
  wraps it with the closer `) ] } ' " ” \` > » * _` and keeps the inner text selected. Not when the
  selection touches a chip, other atoms, or styled text, or crosses a mention boundary.
- **Escape** with a menu open dismisses it until the caret leaves that token
  (`chat/useComposerTriggerState.ts:5-42`); otherwise Escape does nothing in the editor.
- **PageUp/PageDown** in the editor scroll the timeline when the editor itself cannot scroll that
  way (`ED:1339-1364`, `chat/pageScrollController.ts`).
- **Copy/Cut** (`ED:1287-1316`): plain text = the Markdown of the selection; if it contains context
  references, also writes a structured fragment (MIME `COMPOSER_CONTEXT_CLIPBOARD_MIME` plus an
  HTML carrier) so a paste into another composer brings the records along (7.5).
- **Paste** (`ED:1021-1056`): files go to the attachment path (7.2); text is inserted as Markdown
  paragraphs (chips rebuilt, marks applied); a mention/skill at the end of the pasted text gets a
  trailing space, one at the start gets a leading space if needed. Large text folds into a file
  (7.4).
- **Controlled sync** (`ED:1108-1173`): when the store value or cursor changes from outside
  (history, chip insertion, send clearing the draft), the document is rebuilt and the caret
  restored; change events during that write are ignored.
- **Type-to-focus**: a printable key outside any editable, with the model picker closed and no
  terminal focus, is inserted at the end of the prompt and focuses the editor (`CV:6816-6826`).
  Paste outside editables is routed the same way.

## 5. Inline chips

`components/ContextChip.tsx:25-129`, `components/contextChipParts.tsx`,
`components/composerContextPresentation.tsx`, `ED:192-474`.

### 5.1 Chip box

Metrics are em-based, so they scale with the prompt font. At the default 14px prompt:

| Property | Class | Value at 14px |
|---|---|---|
| font | `text-[0.86em] font-medium leading-none` | 12.04px, weight 500 |
| height | `h-[1.41em]` | 16.98px |
| radius | `rounded-[0.5em]` | 6.02px |
| padding-x | `px-[0.5em]` | 6.02px |
| gap | `gap-[0.33em]` | 3.97px |
| icon | `size-[1.17em]` | 14.09px |
| border | `border` | 1px |

- `inline-flex align-middle max-w-full`, label truncates (`leading-tight`). Wrapped in a node view
  `relative inline-flex select-none items-center align-middle` (`ED:198-199`).
- **Kind colors** (`ContextChip.tsx:30-75`): each kind sets `--context-chip-accent` (oklch,
  L 0.62): image `0.16 16`, video `0.16 48`, file `0.136 237`, mention `0.11 215`, terminal
  `0.134 163`, element / preview-annotation `0.134 70`, review-comment `0.16 292`, pull-request
  `0.16 277`, pr-open `0.134 163`, pr-draft `0.02 259`, pr-merged `0.16 292`, pr-closed `0.16 16`,
  skill `0.16 322`, citation `0.16 259`. Then: border = mix(accent 34%, `contrast-border`); bg =
  accent/11; text = mix(accent 22%, `contrast-foreground`). Interactive chips (button/link) on hover:
  border mix 48%, bg accent/17, transition colors. `neutral` kind: border `border/70`, bg `code`,
  fg `foreground`.
- States: `unresolved` = dashed border, fg `foreground`; `invalid` = border `destructive/35`, bg
  `destructive/8`, fg `destructive`. Focus-visible: 2px `foreground` outline offset 2.

### 5.2 Chip kinds in the composer

| Kind | Icon | Label | Click | Hover / details |
|---|---|---|---|---|
| File mention | Pierre file/folder icon (`chat/FileTagChip.tsx`) | basename | opens the file in the right panel | tooltip = full path (`ED:224-252`) |
| Skill | cube glyph (`composerInlineChip.ts:1`, stroke 1.85) | display name | popover (top, `lg` width, compact padding): name `font-medium`, description or "No description is available for this skill.", outline sm "View instructions" (opens the skill file) | (`ED:278-309`) |
| Citation | `QuoteIcon` (`chat/AssistantCitationChip.tsx`) | quote, <= 64 chars + "…", max-w 16em | pencil action opens the comment editor popover; Shift+Tab from after the chip focuses it | (`ED:335-437`) |
| Image | thumbnail 1.17em radius 6 `object-cover`, or `ImageIcon` | middle-truncated name (max-w 288) + size 10px (+ suffix 10px: upload "NN%" or "upload failed") | expands the image dialog | accent = the thumbnail's average color; tooltip "{name}\n{size}" (+ blank line + failure reason) (`contextChipParts.tsx:150-215`) |
| File | Pierre icon, or `FilmIcon` for video | middle-truncated name (36 chars, max-w 288) + size 10px + suffix | opens the file preview dialog (video: player) | upload failed → `invalid`; needs-reattach → `unresolved`, suffix "attach again", tooltip "{name} was not saved with this draft. Attach it again to send it." |
| Terminal | `TerminalIcon` | "{terminal} line N" / "{terminal} lines A-B" | popover: header px12 py8 `border-b` (icon 16 `success`, label 14 `font-medium`, "Line N" / "Lines A–B" 12px `secondary-label`), then `pre` max-h320 bg `muted` p12 mono 12 leading-relaxed | expired → `invalid`, tooltip "Terminal context expired. Remove and re-add {label} to include it in your message." (`chat/ComposerPendingTerminalContexts.tsx`) |
| Review comment | `MessageCircleIcon` | "{basename} L12 to L14" (" (before)" for deletions) | popover with path, "{section} · {range}", markdown body, diff preview (h256) | `composerContextPresentation.tsx:267-284` |
| Pull request | PR state glyph, kind pr-open / pr-draft / pr-merged / pr-closed | "#123" | opens the PR (link preview when available) | tooltip with PR details |
| Preview annotation | `MousePointerClickIcon` | comment (<= 48 chars) or page title, else "Preview annotation" | popover with screenshot (max-h256) or "Screenshot unavailable", plus title/comment/"N elements, N regions, N drawings, N style changes" | `:286-310` |
| Missing record | `CircleDashedIcon`, dashed | label | none | "This context is no longer available. Remove it or attach it again." |

- Removing a chip that is the only reference to an attachment removes the attachment; undo restores
  both (payloads are retained, `CC:3342-3486`). Removing an image thumbnail whose chip is in the
  text asks first: confirm dialog "Remove {name} from the message?\nIt is referenced in your text;
  removing it also removes every reference." (destructive) (`CC:5551-5574`).

## 6. Triggers and the command menu

`composer-logic.ts:218-266`, `CC:2270-2660,3603-3765`, `chat/ComposerCommandMenu.tsx`,
`chat/composerSlashCommandSearch.ts`, `chat/composerMenuHighlight.ts`.

### 6.1 Detection (at the expanded cursor)

1. Line prefix matches `^/(\S*)$` → `slash-command`, range = line start..cursor.
2. Token (since the last whitespace) matches `^#([\p{L}\p{N}][\p{L}\p{N}_-]*)?$` → `pull-request`.
3. Token starts with a currency sign (`\p{Sc}`, usually `$`) → `skill`.
4. Token starts with `@` → `path`.

The menu is open while a trigger exists and no approval is pending. A caret adjacent to a chip
suppresses detection. Escape dismisses the trigger at that range start until the caret leaves it.

### 6.2 Items per trigger

- **`/` slash** (`CC:2376-2445`):
  - Built-in `/model` "Switch response model for this thread" (clears the token, opens the model
    picker). With `planModeUiEnabled` (10.5): `/plan` "Switch this thread into plan mode",
    `/default` "Switch this thread back to normal build mode".
  - Provider commands (`/name`, description = `description` ?? `input.hint` ?? "Run provider
    command"); offered **only when the slash is at prompt start** (`composerSlashCommandSearch.ts:21-29`).
    `/compact` only when the composer has nothing else (no attachments, contexts, other text) and
    compaction is available. `/usage-limits` opens the usage panel instead of inserting.
  - Skills as `/skill:name` rows when setting `showSkillsInSlashMenu` (default true): label renders
    "/skill:" in `secondary-label` + display name; description = shortDescription ?? description ??
    "{scope} skill".
  - Search (`composerSlashCommandSearch.ts:31-114`): query lowercased, leading slashes trimmed.
    Commands score on name (exact 0, prefix 2, boundary 4 on `- _ /`, includes 6, fuzzy 100) and
    description (20/22/24/26). Skills: query "skill" → 0; "skill:x" searches x with the skill
    scorer; otherwise skill scorer, or `MAX_SAFE_INTEGER` when "skill" starts with the query. Ties:
    built-ins, then provider commands, then skills, by name. Empty query = all items in that order.
- **`$` skill**: `searchProviderSkills(skills, query)` (`providerSkillSearch.ts`); label = display
  name; description = shortDescription ?? description ?? "{scope} skill" ?? "Run provider skill".
  Skills come from the selected provider's snapshot resolved for the project cwd
  (`crt:providerSkills`). Right side (`ms-auto`): source badge = Badge secondary, default size
  (h18 px3 radius 6 12px `font-medium`, icon 12 at 80% opacity): `BlocksIcon` "App", `FolderIcon`
  "Repo" / "Project", `UserRoundIcon` "Personal", `SettingsIcon` "System", `PackageIcon`
  "Provider", plus " Skill" under `$` (`ComposerCommandMenu.tsx:235-262`).
- **`@` path**: RPC `projects.searchEntries {cwd: gitCwd, query, limit: 80}` debounced 120ms, only
  for non-empty queries (`state/queries.ts:32-33,225-273`, `contracts:` `ProjectSearchEntriesInput`).
  Row: Pierre icon (file or directory, theme-aware) + basename + parent path as description.
- **`#` pull request** (`CC:2302-2362,2477-2524`): `pullRequests.list {state:"all", projectId,
  limit: 99, query?}` (text queries debounced 180ms) plus `pullRequests.detail {projectId,
  repository, number}` for a numeric query not already listed (debounced 180ms). Results filtered
  to the project's repository, max 12 rows. Row: PR state icon 16 (tone per state) + "#123" +
  title. Only when the project supports PRs.

### 6.3 Selection

`onSelectComposerItem` (`CC:3603-3739`), guarded to one selection per frame:

| Item | Replacement |
|---|---|
| path | `[basename](encoded path) ` (a following space is consumed) |
| `/model` | token removed, model picker opens |
| `/plan`, `/default` | token removed, interaction mode set |
| provider command | `/name ` |
| skill (either menu) | `$name ` |
| pull request | a review-comment context record with PR metadata + its reference `[#123](t3-context://v1/review-comment/<id>) ` |

### 6.4 Empty and loading text

Inside the drawer (`ComposerCommandMenu.tsx:128-145`) at `px20 pt14 pb28` 12px `secondary-label`:
- Loading: "Searching workspace skills..." / "Finding pull request..." / "Searching workspace
  files...".
- Empty: skill "No skills found. Try / to browse provider commands."; path "No matching files or
  folders."; slash "No matching command."; PR: "Pull requests are not available for this project.",
  "Pull requests could not be read for this project.", "No pull request matches {query}.", "No pull
  requests found in this repository." (`CC:2600-2631`).
- A typed `@` with an empty query shows "No matching files or folders." (no request).

### 6.5 Highlight and keyboard

- Default highlight = first item; it resets when the search key (`kind:query`) changes
  (`composerMenuHighlight.ts`). Mouse move sets the highlight; mouse-down is prevented so the editor
  keeps focus; click selects. The active row scrolls into view (`nearest`).
- ↑/↓ cycle with wrap-around; Enter or Tab selects the active item (or the first); Escape dismisses.

### 6.6 Drawer geometry

The menu (and the stash menu, 13.3) is a drawer portaled to the window, positioned against the Main
surface (`CC:858-988`):

- `position: fixed; z 40; flex-col`. left = Main.left + 22; width = Main.width - 44; bottom =
  viewport height - Main.top - 24 (the drawer's bottom 24px overlap the card); max-height =
  max(96, Main.top). Re-measured on resize, scroll and any ancestor resize.
- Surface = attached drawer chrome (9.1): bottom padding 17px, `::before` radius 18 at the top only,
  1px outline, glass, `shadow-composer`, the bottom 16px of the backdrop masked out. Net effect: the
  drawer's visible edge ends 8px below the card's top edge, inset 22px each side.
- List: `max-h288 p8 scroll-pb24`. Row (command item): Pierre icon or PR icon, then `flex min-w0
  flex-1 gap8`: label `max-w45% shrink-0 truncate 12px font-medium sans`, description `flex-1
  truncate 12px secondary-label`, optional badge `ms-auto`. Active row bg `sidebar-row-hover`.
- ARIA: listbox labels "Files and folders" / "Pull requests" / "Commands" / "Skills"; the editor
  sets `aria-activedescendant`.

## 7. Attachments

### 7.1 Kinds and limits

`contracts:orchestration.ts:165-175`, `chat/composerAttachmentFiles.ts`, `CC:5305-5529`.

- Up to **100** attachments per message (`PROVIDER_SEND_TURN_MAX_ATTACHMENTS`), images and files
  together; error "You can attach up to 100 files per message."
- **Images**: GIF, HEIC/HEIF, JPEG, PNG, WebP (also by extension when the type is empty). Other
  `image/*` → "'{name}' is not a supported image type. Attach GIF, HEIC, HEIF, JPEG, PNG, or WebP
  images." Images over 10MB are downscaled to fit (`prepareImageForAttachment`); failures:
  "'{name}' could not be read as an image." / "'{name}' is too large to attach, even after
  compression." Total image bytes per turn 80MB (server-side).
- **Files** (any other type; videos are files): need server support
  (`capabilities.attachmentUploads` + `fileAttachments.maxUploadBytes`); limit = that max (50MB
  before the config is known). Errors: "This server does not support file attachments.", "'{name}'
  is empty or could not be read.", `fileAttachmentTooLargeMessage(name, limit)`.
- Errors go to the thread error banner (`setThreadError`), not toasts.
- Questions: attachments ride with the answer when `capabilities.questionAttachments`; otherwise
  toast "This question cannot accept attachments." (error).
- Pasting while the composer is reverting a checkpoint is ignored.

### 7.2 Sources

- **Paste** with files (`CC:5652-5680`): if any file is an image (supported or not), or there is no
  text, files are claimed as attachments. Paste-as-text (mod+shift+V, armed for 1s) skips the image
  chip.
- **Drop** of OS files anywhere on the chat column (`CV:9898-9919`, `chat/workspaceFileDrop.ts`):
  while dragging, an overlay `absolute inset8 z40 radius 18 2px dashed primary/60 bg primary/3.5%`
  centers a pill `rounded-full border primary/25 bg background/95 px16 py10 14px font-medium
  foreground shadow-lg` with `PaperclipIcon` 16 `primary` + "Drop files to attach". Files →
  `addDroppedFiles`; folders → their path inserted as a file mention (local environments only;
  "Folders can't be dropped into remote environments"; no path: "Couldn't get the path of
  \"{name}\"" / "Type the folder path with @ instead.") (`CC:5895-5928`).
- **File-tree drag** (in-app): dropping a tree item on the form inserts a mention at the end with a
  leading space if needed (`chat/composerMentionDrag.ts`); refusal toast "Unable to add to chat" /
  "The composer is busy; try again once it is ready."
- **Attach button** (2.3).
- **Copied chips from another composer** (7.5).

### 7.3 Inline chips vs the media shelf

- Files always get an inline chip at the caret (or appended when the editor refuses input).
- Images get a chip only when the prompt already has prose, when replacing a selection, or when the
  editor refuses input (connecting, approval, questions, project selection); otherwise they show only
  as shelf thumbnails (`CC:5332-5343,5531-5549`). Question answers never get chips; their files show
  in the shelf.
- A large paste folded into `pasted-text.txt` gets an info toast "Large paste attached as {name}" /
  "{size} · Use ⌘⇧V to keep a large paste inline." (`CC:5444-5455`).

### 7.4 Large text paste folding

`crt:textPaste.ts`, `CC:5576-5650`. Text >= 32KB (chars or UTF-8 bytes), or text that would push the
prompt past 120,000 chars, becomes a `text/plain` file `pasted-text.txt` (`pasted-text-2.txt`, ...).
Not when pasting as text (mod+shift+V). When folding is impossible and the text would exceed the
limit: toast "Pasted text is too large for this message" / "Remove some text or an attachment, then
paste again."; a folded file over the file limit: "Pasted text is too large to attach" / "Reduce the
clipboard contents or save a smaller excerpt as a file.".

### 7.5 Structured chip paste

Copying from a composer writes a context fragment (records + source environment). Pasting it
re-creates terminal / review-comment / annotation records (new ids on collision) and re-downloads
image/file bytes from the source environment through `assets.createUrl` → HTTP GET (60s timeout),
re-adding them under fresh ids (`CC:2796-3072`). Failures toast "Couldn't bring {name} into this
message" with "The environment it came from is not connected." / "The original attachment is no
longer available." / "Downloading it from the source failed." / "The draft rejected this attachment
(duplicate or attachment limit reached)." + " Remove the chip or attach the file again.". While a
transfer is pending, sending shows info toast "Still bringing a pasted attachment into this message."
/ "Send again once its chip resolves.".

### 7.6 Shelf rendering (expanded composer only)

- **Media strip** (`CC:6510-6757`): `mb12 flex max-w-full gap8`, wraps; switches to a horizontal
  scroll strip (thin 10px scrollbar) when snap-shot captures are present.
  - Image tile 64x64 radius 10 1px `border/80` bg `background`, `object-cover`, cursor zoom-in, aria
    "Preview {name}" → image dialog. No preview: name 10px `secondary-label` centered.
  - Not saved locally: top4 left4 box `bg-background/85 p2 radius 4` with `CircleAlertIcon` 12
    `warning-foreground`; tooltip "Draft attachment could not be saved locally and may be lost on
    navigation."
  - Uploading: bottom bar `bg-background/85 px4 10px foreground` with the progress text.
  - Upload failed: `overlay icon-xs` retry button bottom4 left4 (`RefreshIcon`), tooltip = failure
    reason, aria "Retry upload for {name}".
  - Remove: `media-close icon-xs` top4 right4 `XIcon`, aria "Remove {name}" (snap-shot frames show
    it on hover or focus only).
  - Video tile 64² radius 10 bg black: first frame + gradient `black/70 → black/10`, `PlayIcon` 16
    filled white; click plays.
  - Snap-shot (desktop screen capture) frames use their own frame class and an arrival animation
    (scale .95 → 1 + fade, 300ms `--ease-drawer`, setting `snapShotAnimations`); out of scope unless
    snap-shot is ported.
- **File rows** (non-media files without an inline chip) (`CC:6758-6833`): `mb12 flex-col gap4`,
  each `flex gap8 py4 14px foreground`: Pierre icon, name button (truncate, hover underline, opens
  preview), right text 12px `secondary-label` = size, upload percent, "Attach again" or "Remove to
  send" (needs reattach); retry `ghost icon-xs`; remove `ghost icon-xs` `XIcon` aria "Remove
  {name}".
- **File preview dialog** (`CC:6848-6884`): `h=min(85vh, 832) max-w 896`, `AttachmentFilePreview`
  with origin "Draft" and a remove action.

### 7.7 Upload pipeline

`lib/attachmentUploadQueue.ts:149-320`, `CC:1767-1880`.

- When the server supports uploads, every image and uploadable file starts uploading as soon as it is
  in the draft: RPC `attachments.createUploadUrl {type?: "file", name, mimeType, sizeBytes}` →
  `{attachmentId, relativeUrl}`; then HTTP `POST {httpBaseUrl}{relativeUrl}` with
  `Content-Type: <mime>` and the raw bytes; progress in 5% steps. Ready state stores the
  `attachmentId` on the draft file. A re-hydrated file with an `attachmentId` is verified via
  `assets.createUrl` instead of re-uploaded. Removing an attachment deletes its pending upload
  (`attachments.delete`).
- Failure reasons (shown in tooltips): "Unsupported image type", "Original file is no longer
  available", "Upload rejected ({status})", "Upload failed", "Upload timed out", "Upload
  cancelled", "Uploaded file expired. Remove it and attach it again.", "Uploaded file could not be
  verified. Retry when the server reconnects.".
- Send waits for all uploads; a failed one blocks with "Retry or remove failed uploads before
  sending." (thread error). Without upload support, images go inline as data URLs in `turn.start`
  and files are refused.
- `t3_client::Environment::upload_attachment` exists (16).

## 8. Send, steer, queue, stop

### 8.1 Primary action button

`chat/ComposerPrimaryActions.tsx`, wrapped by `CC:1200-1268`.

- **Send / queue / steer** (one 28x28 square, `rounded-md` 8, transparent):
  - Idle colors: icon `secondary-label`; hover bg `sidebar-row-hover` + `foreground`; active bg
    `accent` + scale .97; transition bg/color/scale 150ms ease-out; disabled opacity 35%, no pointer.
  - Icon 14 stroke 1.8: `CornerDownLeftIcon` (send or steer), `ListPlusIcon` (queue), `Spinner` 14
    while connecting, sending or preparing a worktree.
  - aria-label: "Environment disconnected" | the send-disabled reason | "Connecting" | "Preparing
    worktree" | "Sending" | running: "Queue message" / "Steer message" | "Send message".
  - Disabled: send busy, a disabled reason, connecting, environment unavailable (or no provider, or
    project selection required), or nothing sendable.
  - Holding ⌘ flips queue ↔ steer for the label/icon and for a click (`resolveFollowUpBehavior`).
  - Environment identification "artwork" mode paints a stage-backdrop art behind the icon and turns
    it white (`SidebarStageBackdrop`); skip unless that setting is ported.
- **Stop**: shown instead of send while a turn is running **and** the composer has nothing sendable.
  Same 28² box, icon `muted-foreground/70`, `SquareIcon` 12 stroke 1.8, hover bg
  `sidebar-row-hover`, aria "Stop generation". (The destructive round variant in the source is
  unreachable.)
- **Pending question**: [Stop if running] [Previous: 28² `ArrowLeftIcon` 14, aria "Previous
  question", when index > 0] [Submit: 28² `CornerDownLeftIcon` 14, spinner while responding;
  aria "Submitting" | "Next question" | "Submit answers" (index > 0) | "Submit answer"; disabled
  until the step is complete] (gap8, compact gap6).
- **Plan follow-up** (only with plan mode on): pill buttons `rounded-full bg message-action
  fg message-action-foreground font-medium 14px shadow-xs`, hover `message-action-hover`, h32:
  with prompt text "Refine" (px16, compact px12); empty "Implement" (px16, right corners square) +
  chevron segment (px8, `border-l message-action-foreground/20`, aria "Implementation actions") →
  menu top/end "Implement in a new thread". Labels become "Sending..." while busy.
- Pointer-down on these buttons does not take focus from the editor while resting.

### 8.2 Enter and the send shortcut

`composer-logic.ts:27-45`, setting `sendShortcut` ("enter" default | "mod-enter-multiline" |
"mod-enter"; labels in Settings: "Enter", "⌘ + Enter for multiline prompts", "⌘ + Enter always",
`components/settings/SettingsPanels.tsx:2186-2191`).

| Keys | Idle | Turn running |
|---|---|---|
| Enter (shortcut "enter") | send | follow-up (setting `followUpBehavior`) |
| Shift+Enter | newline | newline |
| ⌘+Enter | send (draft thread: send in **background**, 8.6) | the **other** follow-up behavior |
| ⌘+Shift+Enter (with a mod-enter shortcut) | newline | other follow-up behavior |

"mod-enter-multiline" requires ⌘ only when the prompt contains a newline; "mod-enter" always.
When ⌘ is required, plain Enter inserts a newline.

### 8.3 Follow-ups while a turn runs

`chat/composerSubmission.ts:4-12`, `CV:7752-7795`, `queuedMessageStore.ts`,
`components/QueuedMessageSender.tsx`. Setting `followUpBehavior` default **"queue"**.

- **Queue**: the whole draft (prompt, images, files, terminal contexts, annotations, review
  comments) plus send settings (model selection, runtime mode, interaction mode, prompt effort) is
  stored in memory (not persisted) per thread, and the composer clears. A send while any queued
  message is still pending also queues, so order is kept.
- **Steer**: dispatches `thread.turn.start` immediately on the running thread (the server treats it
  as a steer). Same pipeline as 8.5.
- **Queued rows** render at the end of the timeline (`chat/MessagesTimeline.tsx:1796-1890`):
  right-aligned, max-w 80%, radius 18, 1px dashed `border`, p12, text `message-foreground/80`;
  the prompt as a user message body; "N attachments, N context items" 12px `secondary-label`;
  footer row `mt8 gap16 12px secondary-label`: `ClockIcon` 14 + "Queued"/"Sending" (tooltip bottom
  "Sends when the turn ends" | "Sends after the messages above it" | "Waits for Send now" | "Sending
  to the agent"), then `ghost-muted icon-xs` "Send now" (`ArrowUpIcon`, tooltip "Send now
  (⌘⇧↩)" on the next one) and "Cancel and return to the composer" (`XIcon`).
- **Sending**: the head message goes out when the thread phase becomes ready (turn finished), it is
  not held, no approval/question is pending, and the server acknowledged the previous dispatch.
  "Send now" / mod+shift+Enter (`thread.steerQueuedMessage`) sends the head immediately
  (`sendQueuedMessage.ts:106-260`).
- **Stop** (`CV:4043-4063`): drains the queue back into the composer (prompts joined with blank
  lines; attachments beyond 100 stay queued with `holdUntilUserAction` and an info toast "Some
  attachments stayed queued"), then dispatches `thread.turn.interrupt {threadId, turnId}`.
  Failure: thread error "Failed to interrupt the current turn.". Cancel on a queued row returns that
  message to the composer.

### 8.4 Guards before sending

`CC:3809-3870`, `CV:7417-7560`:

- No provider or a disabled reason: nothing happens. Disabled reasons (`CC:1955-1962`, `CV:10134`):
  "Rewinding conversation", "Sending feedback", "Messages loading", "Preparing worktree", project
  clone reasons, "Select at least one model.", attachment block reasons ("Update this server to send
  files with question answers", "Attach the interrupted file again or remove it" / "... files again
  or remove them", "Waiting for the server before file attachments can send", "This server does not
  accept file attachments right now. Remove the files to send.", the size message, "Retry or
  remove the failed attachment(s)", "Attachment(s) still uploading" (`lib/attachmentUploadState.ts:
  40-53`)), Antigravity install/auth reasons. The reason doubles as the send button's aria-label.
- A pasted image still compressing: info toast "Still compressing a pasted image." / "Send again
  once its thumbnail appears.".
- Environment unavailable: warning toast "Not connected: message not sent" / "Reconnecting to the
  environment. Try again once it is connected.".
- A pending question: Enter advances the question instead (9.3).
- Draft without a project: warning toast "Choose a project first" / "This draft no longer points
  to an available project.".
- Only expired terminal contexts: warning toast (`buildExpiredTerminalContextToastCopy`).
- `/usage-limits` typed alone opens the usage panel; Codex `/feedback ...` uploads feedback
  (`CV:7419-7655`); `/plan` or `/default` typed alone switches mode when plan mode is on.
- New worktree mode without a base branch: thread error "Select a base branch before sending in
  New worktree mode.".

### 8.5 Dispatch

`CV:7796-8676`.

1. Expired terminal chips are removed from the text; context records are built
   (`lib/composerContextRecords.ts` `buildMessageContext`).
2. Text = `formatOutgoingPrompt`: Claude prompt-injected effort prefixes "Ultrathink:\n"; an
   attachment-only message sends "[User attached one or more files without additional text.
   Respond using the conversation context and the attached files.]"
   (`chat/composerPromptHistory.ts:19-20`); the prefix is never added in front of a `/command`
   (`shared:model.ts:412-431`). Validation: > 120,000 chars → inline error (8.7), no send.
3. Uploads (7.7) are awaited.
4. Optimistic user message (with blob previews) appended; scroll to end (first message anchors the
   timeline); thread error cleared; draft content cleared.
5. Title seed: plain text (citations as text, context links removed) | "Image: {name}" | "File:
   {name}" | terminal label | "Review: {label}" | annotation label | "New thread"; truncated
   (`shared:String.ts`).
6. Server thread: first message → `thread.meta.update {title}`; then
   `persistThreadSettingsForNextTurn`: `thread.meta.update {modelSelection, branch?}` when changed,
   `thread.runtime-mode.set`, `thread.interaction-mode.set` when they differ.
7. `thread.turn.start` (`contracts:orchestration.ts`, `TurnStart`):
   ```
   { threadId, message: { messageId, role: "user", text, attachments: [uploaded refs | {type:"image",
     id, name, mimeType, sizeBytes, dataUrl, source?}], context? },
     modelSelection: {instanceId, model, options?}, titleSeed, runtimeMode, interactionMode,
     bootstrap?: { createThread?: {projectId, title, modelSelection, runtimeMode, interactionMode,
       branch, worktreePath, createdAt},
       prepareWorktree?: {projectCwd, baseBranch, branch: <temp name>, startFromOrigin?},
       runSetupScript?: true },
     createdAt }
   ```
   `message.context` is sent only when `capabilities.inlineMessageContext`; older servers get the
   records serialized into the text (`shared:composerContextLegacySend.ts`).
8. Failure: if the composer is still empty, the prompt, cloned images, files, contexts,
   annotations and review comments are restored and the optimistic message removed; thread error =
   server message or "Failed to send message.".

### 8.6 Background and multi-model sends (drafts)

- ⌘+Enter on a draft sends in the background: the turn starts, a fresh draft opens in place, and a
  success toast "Started in background" with "Open" appears (5s) (`CV:8540-8600`).
- Multi-model (draft, server `capabilities.requiredWorktreeBootstrap`, git repo with a branch):
  Shift+click / Shift+Enter in the model picker selects several models (10.2); each gets its own
  thread and worktree. Toasts: "Started N threads in background", "Could not start {model}",
  "Choose models and a base branch" / "Multiple models need a new thread in a Git project. Each
  gets its own worktree.", "Update this server before starting multiple models." (`CV:7520-7545,
  8022-8250`). Port after single sends work.

### 8.7 Prompt length

`chat/composerSubmission.ts:20-31`, `chat/ComposerPromptLengthValidation.tsx`. Over 120,000
characters (citations expanded): `p role=alert px16 pb8 12px destructive` under the body:
"Prompt is {n} character(s) over the 120,000-character limit. Shorten or split it before sending."
(numbers en-US grouped). Clears when the prompt changes.

## 9. Drawers above the card

### 9.1 Attached drawer chrome (`chat/ComposerBanner.tsx`)

- **Dock** (`:122-132`, mounted `CC:6217-6388`): an Attachment that is hidden unless it holds an
  attached surface; `flex items-end gap4`: a Column (flex-1, the stacked drawers) + the stash tab
  (13.3) at the right.
- **Attachment** (`:106-120`): `mx-auto w = 100% - 44px`, margin-bottom -17px (tucks under the
  card, which sits above at z10). Adjacent attachments share an outline (the lower one loses its top
  radius and top border).
- **Surface** (`:37-71`): attached placement sets `--chat-composer-attachment-overlap` 17px.
  `::before` (z -1): inset 0, radius 18 at the top (floating: all corners), 1px border
  `--chat-composer-attached-outline`, bg glass surface at `--glass-opacity` with a vertical tint
  gradient, `shadow-composer` (dark `shadow-composer-dark` = `0 14px 32px -18px rgb(0 0 0/75%)`),
  the bottom 16px masked out. Dark mode with blur adds a 10px dark seam above the card
  (`--background-image-composer-seam-above`, `index.css:195-201`).
  - Outline per variant: default/info/success = neutral (`contrast-foreground` 8%, dark white 5%);
    error = `--error` 32% + tint `--error` 8%; warning = `--warning` 28% + tint 8%.
- **Root** (`:148-177`): `min-w0 px4 py(4 | comfortable 5 | spacious 12, with px12)`, plus an
  `::after` block 17px tall so content clears the card; 12/16 text; `--composer-banner-icon-column`
  24px. Container queries apply at its own width.
- **Row** (`:180-208`): grid `[24px | minmax(0,1fr) | auto]`, `gap-x4`, min-h24, items-center
  (approval layout: items-start, gap-x8 gap-y12). As a button: radius 8, focus outline 2px `ring`.
  `wrap-actions`: below 400px the actions wrap under the content.
- **Icon** `24 wide, muted-foreground, svg 12` (approval: pt2, `warning`, svg 16). **Content**
  `flex gap4`. **Actions** `flex wrap gap4 justify-end` (approval gap6; below 560px they move to a
  second row). **Separator** "·" `muted-foreground/40` mx4. **Count** min-w24 `font-medium
  muted-foreground tabular-nums`. **Body** ps28. **Scroll**: max-h min(384px, 40dvh), fades at
  overflowing edges. **ToggleIcon**: `icon-xs ghost` box with `ChevronDownIcon` 14, rotated 180°
  when collapsed. **Dismiss**: `icon-xs ghost` `XIcon` 14.
- **Peek** (`:82-104`): `absolute inset-x0 bottom0 mx-auto h12 w96%` radius 18 at the top, 1px
  border without bottom (error `destructive/24`, warning `warning/24`), glass, `shadow-md`,
  opacity transition 150ms.
- Order inside the Column (`CC:6218-6376`): banner stack (9.2) → activity strip (sync row or
  inline tasks badge) → top drawer (approval / question / plan follow-up) → tasks drawer → tasks tab.

### 9.2 Banner stack

`chat/ComposerBannerStack.tsx`; items built at `CV:6604-6690` and `CC:5188-5208`.

- Priority: activity (sync/tasks content) 0, then urgent or error/warning 1, then the rest 2. The
  first item is the front (attached) row; the rest stack behind a Peek cap.
- Hover (not touch) or focus on the stack expands the hidden items above it (grid rows 0fr→1fr
  150ms ease-out; items translateY 4→0 + fade 150ms), as floating surfaces with `space-y8 pb8`.
  Escape collapses and refocuses the Peek. Peek aria "Show other notices".
- Item row (`:285-378`): Root density comfortable, Row (`wrap-actions`): icon (top-aligned),
  title `font-medium` truncate (line-height 24), description truncate `muted-foreground` with an
  info button (`ghost-muted icon-xs` `InfoIcon`, hover popover with the full text) only when the
  description is clipped; actions; dismiss (aria from `dismissLabel` or "Dismiss warning").
- Dismiss: 220ms ease-in exit (front: translateY 64 + fade; stacked: translateY 112 + fade), then
  `onDismiss`.

| Item (order of insertion) | Variant / icon | Title | Description / actions | Source |
|---|---|---|---|---|
| Feedback uploads | per state | Codex feedback progress | | `CV:6589` |
| Usage limits | | usage panel header | | `CV:3186` |
| Project clone | info `DownloadIcon` (error/warning on failure) | "Cloning {name}" / "Failed to clone {name}" / "Cancelled cloning {name}" | progress summary or error; Cancel / Retry | `CV:2224-2300` |
| Environment unavailable | error (phase error) else warning, `WifiOffIcon` | "{label} is offline" / "{label} is reconnecting" | ghost xs "Reconnect" (offline only), ghost xs "Disconnect server" (title "Hide this server's threads. Switch it on again in Connections.") | `CV:2738-2788` |
| Server update | default / error, update icon | "Server update available" (tooltip "{server} {v} → {v}") or update progress | guidance text; "Update" / "Retry"; dismiss "Dismiss update notice" | `CV:2789-2860` |
| Background work (activity) | default, 6px foreground dot (status pulse while working) | "{N} agent(s) working" / "Background work" / "Monitoring" | "View agents", "Stop" ("Stopping...") | `CV:6364-6418` |
| Resume compaction (setting `resumeCompactionBannerEnabled`, default off) | info `Minimize2Icon` | "Resume with less context" | "{n} tokens from earlier"; dismiss "Keep full history" | `CV:6513` |
| Woke from snooze | info `AlarmClockIcon` | "Thread woke from snooze" | "Send a message to continue" | `CV:6419` |
| Branch mismatch | info `GitBranchIcon` | "Branch changed — was {branch}" (tooltip "This thread last ran on {a}. Sending will continue on {b}.") | ghost xs "Restore branch" ("Restoring..."); dismiss "Dismiss branch change notice" | `CV:6633-6668` |
| Snoozed / settled | info `AlarmClockIcon` / `CheckCircle2Icon` | "This thread is snoozed" / "... settled" | "Send a message to wake" / "... unsettle"; "Wake now" | `CV:6437` |

The environment-unavailable item is suppressed while reconnecting during a server update or the
reconnect grace period. Each item's exact copy is in the cited block; port the stack mechanics
first and these items as their features land.

### 9.3 Pending approval

`chat/ComposerPendingApprovalPanel.tsx`, `chat/ComposerPendingApprovalActions.tsx`, `CC:6229-6262`;
derivation `crt:pendingRequests.ts` (= `t3_client::pending_requests`).

- Root: variant **warning**, density **spacious** (px12 py12), Row layout approval.
- Icon: `ShieldIcon` 16 `warning`, pt2.
- Content (`flex-col items-start gap4`):
  - Header `flex gap8 11px muted-foreground`: kind label `font-medium warning` ("Command approval" |
    "File read approval" | "File change approval" | "App access approval" (mcp-elicitation) |
    "App permission approval" (permission)), then the app name (truncate), then "1/N" right-aligned
    when N > 1 (tabular).
  - Detail: block `max-h80 overflow-auto 12px foreground`, mono `whitespace-pre` (mcp-elicitation:
    sans, pre-wrap); focusable, focus ring inset 2px `ring/70`; text = `detail` or the kind label.
- Actions (`ComposerPendingApprovalActions.tsx:89-170`): options = the request's `options` or the
  default set [cancel "Cancel", decline "Decline", acceptForSession "Always allow this session",
  accept "Approve"]. Decline (outline xs) and accept (default xs) show as buttons (labels max-w160
  truncate); a warning adds `TriangleAlertIcon` 12 and a tooltip. Everything else goes in an
  `outline icon-xs` `EllipsisIcon` menu (aria "More approval options", side top, align end), items
  wrap text, warnings with a `warning` triangle. All disabled while responding.
- Dispatch: `thread.approval.respond {threadId, requestId, decision}` with decision `accept |
  acceptForSession | acceptAlways | decline | cancel` (`CV:8713-8739`) → `respond_to_approval`.
- The composer meanwhile: value "", read-only, placeholder "Resolve this approval request to
  continue", no footer, body pb16, no stash tab, no command menu.

### 9.4 Pending user input (questions)

`chat/ComposerPendingUserInputPanel.tsx`, `pendingUserInput.ts`, `CC:6263-6347`, `CV:8741-8833`.

- Root variant info (neutral), density default. A Collapsible whose header Row is a button:
  - Title (native `title`): "Hide the question and its options" / "Show the question and its
    options".
  - Content: `question.header` `font-medium muted-foreground`; when collapsed also the question
    text (truncate, `secondary-label`).
  - Actions: "i/N" 10px `font-medium muted-foreground tabular` (N > 1), ToggleIcon, and for
    dismissible (async) questions a Dismiss, aria/title "Dismiss question without answering" →
    `thread.user-input.dismiss`.
- Panel (Scroll + Body `ps28 pe4 pb4`, wrap anywhere):
  - Question 14px `foreground/85`; multi-select hint "Select one or more options." 12px
    `secondary-label` mt4.
  - Options `mt8 space-y2`: each a button `flex w-full gap8 radius 8 px10 py8 text-left`,
    transition colors 150ms; selected bg `muted/55` fg `foreground`; else transparent
    `foreground/85` hover bg `muted/30`; focus ring 1px `primary/25`; responding: opacity 50%,
    not-allowed. Content: label 14px `font-medium`; description (when different) 11px
    `secondary-label`, gap2. Right: selected `CheckIcon` 14 `primary`; else digit kbd 20² 10px
    `font-medium muted-foreground tabular` (options 1-9).
- Behavior: single-select selects optimistically and advances after **200ms**; multi-select
  toggles. Digits 1-9 pick options when focus is not in an input, textarea or contenteditable, and
  the card is expanded. Typing in the editor writes the custom answer and overrides the option.
  Enter in the editor advances. Collapsing is per question (the next question reopens).
- Answer: on the last question `thread.user-input.respond {threadId, requestId, answers,
  attachments?}`; per-question attachments come from per-question drafts and must be uploaded
  first ("Wait for attachments to finish uploading, or remove failed uploads."). Client API:
  `respond_to_user_input`, `dismiss_user_input` (attachments field: see 15).

### 9.5 Plan follow-up (plan mode only)

`chat/ComposerPlanFollowUpBanner.tsx`, `CC:6301-6305`. Reachable only when `planModeEnabled` and the
provider shows the toggle. Row: empty icon column, "Plan ready" `font-medium muted-foreground` +
plan title (truncate, `foreground/85`). Primary actions: Refine / Implement / "Implement in a new
thread" (8.1). Empty-draft send → implement the plan; text → refine in plan mode
(`resolvePlanFollowUpSubmission`, `CV:7656-7710`).

### 9.6 Activity, tasks

- **Sync row** (`chat/ComposerActivityStatus.tsx`): spinner (continuous) + "Loading messages..." /
  "Syncing messages..." 12px `muted-foreground` px8, shown only after the sync has lasted 400ms
  and then kept for at least 400ms (`crt:delayedStatus.ts:4-6`). With banner items it becomes the
  front stack item instead of its own strip.
- **Tasks** (`chat/ComposerTasksBadge.tsx`), from the latest `turn.plan.updated` steps: row =
  `ListTodoIcon` + "Tasks" (`muted-foreground`) + current step (`font-medium foreground/80`
  truncate) + "done/total" Count (`success` when complete) + segment bar (<= 10 steps, shown at
  >= 560px: w80, 3px bars gap2, completed `success`, in progress `primary`, pending
  `muted-foreground/25`) + ToggleIcon. Expanded list: per step check / `CircleDotIcon` /
  `CircleIcon` 12 (colors as the bars), text (completed `muted-foreground/55`, current
  `foreground/90`, pending `muted-foreground/70`), duration 10px `muted-foreground/45` w48 right
  ("now" for the running one). Placement: a tab under the dock when nothing else is attached; inline
  in the banner stack when banners exist; never while an approval or question is pending.

### 9.7 Context window meter (legacy, off by default)

`chat/ContextWindowMeter.tsx`; setting `contextWindowMeterEnabled` (default false). In the footer
before the primary action: `ghost-muted icon-sm` 28², a 14px ring (viewBox 24, r 9.75, stroke 3,
track `muted-foreground/24`, progress `muted-foreground` 72% or `--error` above 90%, dashoffset
transition 500ms ease-out). Hover popover (150ms open delay; tooltip style, top/end, `sm` width):
"Context Window" 12px `font-medium muted-foreground`; "{pct} · {used}/{max}" 11px `secondary-label`
tabular; 6px progress bar `muted/60`; "Total processed {n}"; auto-compaction note; outline xs
"Compact context" (`Minimize2Icon`) when manual compaction exists, which sends "/compact" as a turn,
plus the disabled reason below it. Compaction note: "Compacts automatically at N tokens." /
"Context for {model} compacts automatically when needed." / "Context compacts automatically when
needed.". Percent shows one decimal below 10%. Tokens format 950 / 1.2k / 12k / 1.2m. While thread
detail loads (and the provider reports context windows) a 28² placeholder holds the slot.

## 10. Footer controls and pickers

Verified research distillation; tags **NEW / CHANGED / REMOVED** compare with `chat.md` §5.

### 10.0 ComposerControl (NEW shared trigger look)

`chat/ComposerControl.tsx:113-231`. Every footer and context-strip trigger uses it.

- Base: `inline-flex`, radius 8, 1px transparent border. Hover, pressed and
  `data-composer-control-open=true` → bg `sidebar-row-hover` (open also sets `foreground`). Active
  scale .97. Transition bg/color/scale 150ms ease-out. Disabled opacity 64%. `aria-pressed` → bg
  `accent`, fg `accent-foreground`, hover `accent/80`.
- `sm` (expanded footer): h28 px10 gap6 14px `font-medium`, color `secondary-label`, hover
  `foreground`; untinted svgs `muted-foreground` 16.
- `xs` (resting strip): h24 px7 gap4 12px `font-normal muted-foreground/70`, hover `foreground/80`;
  svgs 14.
- Icon: xs 12; sm 16, or 18 with `opticalSize="large"`. Chevron (stroke 2.25): xs 12 current color
  at 50% with `-me4`; sm 14 `icon-muted`. Separator: vertical 1px `border`, mx2, h14 (xs) / h16
  (sm).
- Menu triggers open on pointer **release** (mousedown prevented, click toggles).
- Size: `xs` whenever the controls live in the context strip (resting composer), else `sm`
  (`CC:4767,4980,5008,5098`).

### 10.1 ProviderModelPicker (trigger)

`chat/ProviderModelPicker.tsx:29-320`, mounted at `CC:5045-5127`.

- ComposerControl `min-w0 shrink justify-between whitespace-nowrap`. In the expanded footer:
  `-ms2 min-w52`; in the strip: `min-w52 shrink`, and below a 640px composer surface the label
  collapses to width 0 (icon only). No max width when composer-owned. Disabled while the provider
  catalog is pending or a send is busy.
- Content `flex min-w0 flex-1 items-center gap4 (xs) / gap6 (sm)`:
  - ProviderInstanceIcon 16 (`chat/ProviderInstanceIcon.tsx`): driver glyph (OpenAI for codex,
    Claude, OpenCode, Cursor, Grok, Antigravity; Meta/Grok/Gemini overrides by model slug) or
    initials 10px semibold. Badge (`shouldShowInstanceBadge`: accent color set, or several
    instances of one driver): 12px, 7px text, offset -2/-2, border = `contrast-input` (sm) or the
    glass surface (xs), no shadow at xs. In the strip the glyph is monochrome
    `muted-foreground/70`. While ultrathink is active the icon gets `ultrathink-chroma`
    (saturate 1.2).
  - Label (truncate, flex-1): `shortName ?? name` minus a leading `{subProvider}` qualifier
    (`providerIconUtils.ts:62-86`). Unknown slug → first option, except opencode/antigravity which
    show the raw slug or "Choose model". Tooltip (top, 600ms): "{label} · ⇧⌘M".
  - Badge outline sm "Unavailable" when the model is flagged unavailable.
  - Chevron.
- No-provider state (catalog known, nothing resolves): ComposerControl `CircleAlertIcon` 16 +
  "Open provider settings" (→ `/settings/providers`) or disabled "No provider available"
  (`CC:5021-5035`).
- Open: Popover padding none, align start, offset 4, glass. While open, page wheel/touch outside the
  picker is blocked and body overflow hidden (`:104-150`). Footer row `ChatGptSharingControl` when
  the instance uses ChatGPT plan sharing: `border-t px12 py8 flex justify-between gap16`, OpenAI
  glyph 14 + "Using ChatGPT plan" 12px `muted-foreground`, usage button xs.
- Toggled by ⇧⌘M (`modelPicker.toggle`, ignores repeat, `CV:6963-6968`), `/model`, and the trigger.
- On select (`CV:9318-9393`): rejected when the driver doesn't match `lockedProvider` or the
  continuation group differs; `requiresNewThreadForModelChange` → warning toast "Start a new chat to
  change models" / "This provider does not allow switching models after a conversation has
  started." (`ChatView.logic.ts:1056-1092`). Otherwise the draft selection is written with
  `{explicit: true}`, sticky updated, composer refocused. Nothing is dispatched until the next send.

### 10.2 Model picker panel

`chat/ModelPickerContent.tsx:150-1060`, `ModelPickerSidebar.tsx`, `ModelListRow.tsx`,
`modelPickerSearch.ts`, `modelPickerKeys.ts`, `modelOrdering.ts`.

- Panel `flex-row`, h = min(100vh, 346), w = min(100vw, 360), overflow hidden, inside
  `TooltipProvider delay=0` (all its tooltips open instantly).
- **Rail** (hidden while searching): w44, bg `muted/30`, vertical toolbar, scrollbar hidden,
  inner `flex-col gap4 p4`.
  - Order: enabled instances, server order with the default instance first per driver, then every
    Claude instance moved ahead of the first OpenAI one (`crt:providerPickerOrder.ts`). Locked:
    matching instances first.
  - Favorites button first: square 36², radius 8, hover/focus bg `sidebar-row-hover`, `StarIcon` 20
    filled, tooltip "Favorites"; then a `border-b border/70` divider.
  - Instance buttons: same square, icon box 24, glyph 20 (selected shows the model-specific glyph).
    Badge 14px / 8px text `shadow-sm` (accented: 12px / 7px); non-accent badge bg `card` text
    `muted-foreground`; badge border `muted` (hovered), `background` (selected), else `muted` 30%.
  - Disabled (opacity 50%, not-allowed): not picker-ready (`enabled && isAvailable &&
    status==="ready"`) unless it holds a reachable unavailable selection, or locked out.
  - Selected indicator: 3x20 `rounded-l-full bg-primary` at the right edge, top = item center -10,
    animates `top` 200ms ease-out.
  - Tooltips side left, offset 8: "{name}", "{name} — Disabled in settings.", "{name} —
    Unavailable|Limited|Not ready.[ {msg}]", locked: "{name} is unavailable in this thread. Start
    a new thread to switch providers.".
  - Initial selection: the active instance when locked, holding an unavailable selection, or needing
    setup; else Favorites if any exist; else the active instance.
- **Main pane** `flex-1 flex-col bg-muted/40` (+ `border-l border/70` with the rail):
  - Search (combobox search input, 0.1), placeholder "Search models...", autofocused (layout
    effect, rAF, timeout 0).
  - List: LegendList (virtualized) `py6`, content `pl8 pr1`, est. row 52, draw distance 480, 2px
    gap between rows, 24px top/bottom fade masks when scrollable (bottom shown up front with > 5
    rows), scrollbar track inset 8.
  - Empty: "No models found" 14px centered p8 `muted-foreground`.
  - Provider setup block (instead of Empty, when not searching): `max-h176 overflow-y-auto border-t
    border/70 p8`; per entry `px4 py6 12px leading-snug`: status message (3 lines max,
    `muted-foreground`), InlineButton mt4 "Open provider setup" (or "Set up {name}" when several).
- **Row** (combobox item): hover `sidebar-row-hover`, current model = bg `sidebar-row-selected`
  (no check in single mode).
  - Left: line 1 `flex gap8`: name 12px `font-medium leading-snug` truncate (shortName unless
    locked) + Badge outline sm "Unavailable"; line 2 mt4 `gap6`: driver glyph 12 + "{instance
    name}[ · {subProvider}]" 12px `muted-foreground/70`.
  - Right `gap6`: [multi-select: `CheckIcon` 14] + jump Kbd ("⌘1"...) + favorite toggle
    (`ghost-muted icon-xs`, `-mr4`, `StarIcon` 12; favorite = filled `warning`; tooltip "Add to
    favorites" / "Remove from favorites").
  - Disabled rows (opacity 64%, not-allowed): tooltip left "{reason} Start a new thread to use this
    model." (`CV:9301-9316`).
- **Legacy section**: on one instance tab with `isLegacy` models: current models, a "Legacy models"
  row (12px `font-medium`, second line "{n} models" 12px `muted-foreground/70`,
  `ChevronRightIcon` 16 rotated 90° when open), then legacy models if expanded (starts expanded
  when the active model is legacy).
- **Filtering** without a query: lock filter (driver + continuation group), then Favorites (all
  instances) or the selected instance; sort favorites first on an instance tab, instance order on
  Favorites, then original order (`modelOrdering.ts:58-86`). Options per instance
  (`modelSelection.ts:240-273`): server models + custom models, minus `hiddenModels`, ordered by
  `modelOrder`.
- **Search** (`modelPickerSearch.ts`): fields name, shortName, subProvider, driver kind, instance
  display name, combined text; per token the best of field index x10 + exact 0 / prefix 2 /
  boundary 4 / includes 6 / fuzzy 100 (tokens >= 3 chars); sum; favorites -24; ties: favorite, then
  combined text. Rail ignored, lock still applies.
- **Keyboard**: ↑/↓ highlight (scrolls into view); Enter selects (or toggles the legacy row);
  Shift+Enter / Shift+click toggles additively (multi-model); Esc closes; ← with an empty query or
  ⇧Tab focuses the selected rail button, → in the rail returns to search; ⌘⇧↑/⌘⇧↓ cycle the rail
  (Favorites, then enabled unlocked instances, wrapping; clears the search); ⌘1-⌘9 pick the Nth
  selectable visible row (desktop only, ignores repeat and an open command palette).
- **Favorites** persist in client settings `favorites[{provider: instanceId, model}]`.

### 10.3 TraitsPicker (effort, speed, thinking, context window, agent)

`chat/TraitsPicker.tsx:283-703`, `chat/composerProviderState.tsx`.

- Descriptors come from model capabilities (`shared:model.ts:141-154`); **labels are the server's**
  (no client relabeling): Claude "Reasoning" Low / Medium / High / Extra High / Max / [Ultracode] /
  Ultrathink (`promptInjectedValues: ["ultrathink"]`), "Context Window" 200k/1M, boolean "Fast
  Mode", boolean "Thinking"; Codex "Reasoning" None ... Ultra and "Service Tier" (Standard = id
  `default`, Fast, Ultrafast). With `planModeEnabled` off the opencode `agent: "plan"` option is
  dropped. Hidden when no descriptor applies.
- A model with a `fastMode` descriptor the user never touched sends `{fastMode: false}` (new chats
  stay normal speed, `composerProviderState.tsx:71-86`).
- **Trigger** (ComposerControl, `data-composer-shortcut="composer.effort"`): Codex `min-w0 max-w192
  shrink justify-start`; others `shrink-0`. Children: speed icon (or a `BrainIcon` that appears only
  in icon-only mode) + label (`data-composer-control-label`; Codex truncates) + chevron.
  - Label = descriptor values joined " · " (select = option label; prompt-controlled primary shows
    "Ultrathink"; boolean = "{Label} On/Off"); speed is not text: `fastMode` true or service tier
    Fast → `ZapIcon` filled 80% opacity; Ultrafast → double bolt (`Icons.tsx:5-10`, paths
    `m17 2-10 12h7l-1 8 10-12h-7l1-8Z` at .4 opacity + `m11 2-10 12h7l-1 8 10-12h-7l1-8Z`). Bolt
    color: Claude `#d97757`, others `foreground`, current color in the strip. When speed is the
    only trait the label falls back to "Fast"/"Normal" or the tier name. aria/tooltip "{label}, Fast
    mode on" / "..., Ultrafast mode on".
- **Menu** (align start, glass): per select descriptor a label (px8 pt6 pb4 12px `font-medium
  muted-foreground`) and radio items (`closeOnClick`) with label + Badge outline sm "Default" for the
  default, optional description 12px `muted-foreground/80` max-w224; booleans: label + "On"/"Off";
  separators between sections.
- **Ultrathink** is active when the primary select descriptor has prompt-injected values and the
  prompt starts with "Ultrathink:" (`composerProviderState.tsx:140-156`). Choosing the
  prompt-injected value prepends "Ultrathink:\n" (not for slash-command
  prompts); another effort strips it. If "ultrathink" is in the body the effort radios are disabled
  with 'Your prompt contains "ultrathink" in the text. Remove it to change this option.'. While
  active the Main gets `ultrathink-frame` (`index.css:2163-2200`): a 2px ring painted from a 120°
  gradient #ff6b6b → #f59e0b → #22c55e → #14b8a6 → #3b82f6 → #ec4899 → #ff6b6b at 220% size,
  `saturate(.82) brightness(.92)`, masked to the border; the surface gets an inset 1px white/7%
  shadow. **Static** at fe7d3092c (no keyframes are attached), so no continuous repaint.
- Writes `setProviderModelOptions(target, driver, options, {instanceId, model, persistSticky:
  true})` → draft `modelSelectionByProvider[instance].options` → `turn.start.modelSelection.options`.

### 10.4 Mode controls (Plan/Build, access level), legacy, off by default

`CC:1080-1198`, `chat/runtimeModeConfig.ts`, `chat/CompactComposerControlsMenu.tsx`.

- Gates (`contracts:settings.ts:436-450`), both default **false**, set in Settings → General →
  "Legacy features" (collapsed section, `components/settings/SettingsPanels.tsx:2084-2178`):
  - `planModeEnabled`, row description "Restore Build/Plan, /plan, /default, and Shift+Tab. Off
    uses build mode." (switch aria "Plan mode (legacy)").
  - `accessLevelIndicatorEnabled`, "Shows the access level selector in the composer." (aria "Access
    level indicator (legacy)").
  - Also there: `contextWindowMeterEnabled` "Shows context window usage as a circular indicator in
    the composer.".
- `planModeUiEnabled = planModeEnabled && provider && provider.showInteractionModeToggle !== false`;
  otherwise interaction mode is forced to "default" (`ChatView.logic.ts:613-626`).
- **Access select** (first): separator + `ComposerSelectControl` (Select trigger styled as
  ComposerControl + chevron), aria "Runtime mode", shortcut `composer.mode`, content icon + label,
  tooltip = description. Popup not aligned to the trigger; items min-w256, no indicator, row `gap12`
  with a `grid gap2`: title `inline-flex gap6 font-medium foreground` with icon 14
  `muted-foreground`, description 12/16 `muted-foreground`.

  | Value | Label | Icon | Description |
  |---|---|---|---|
  | `approval-required` | Supervised | `LockIcon` | "Ask before commands and file changes." |
  | `auto-accept-edits` | Auto-accept edits | `PenLineIcon` | "Auto-approve edits, ask before other actions." |
  | `auto` | Auto | `SparklesIcon` | "Supported providers approve routine actions; others still ask." |
  | `full-access` | Full access | `LockOpenIcon` | "Allow commands and edits without prompts." |

- **Plan toggle**: separator + ComposerControl `aria-pressed` in plan mode, icon `PencilRulerIcon`
  (plan, current color) or `BotIcon` (build, large optical size at sm), label "Plan" / "Build"
  (screen-reader only below 640). Tooltip "Plan mode — click to return to normal build mode" /
  "Default mode — click to enter plan mode".
- What a send uses (`CV:1955-1959`): `runtimeMode = draft ?? server thread ?? project
  defaultRuntimeMode` (default "full-access", `contracts:orchestration.ts:135`); interaction mode =
  "default" unless gated on, then draft ?? thread ?? "default". Mode changes write the draft (and the
  draft thread context for local drafts) and refocus the composer (`CV:4564-4606`); the server is
  updated before the next turn (8.5 step 6).

### 10.5 Built-in slash commands

`/model`, and with `planModeUiEnabled` `/plan`, `/default` (6.2). A standalone `/plan` or `/default`
typed as the whole message also switches (`composer-logic.ts:280-290`, `CV:7710-7724`).

### 10.6 Left-cluster fitting and the overflow menu

`composerFooterLayout.ts:110-207`, `chat/restingComposerControlsMeasurement.ts`, `CC:1003-1078,
4972-5179`. Applies to the expanded footer and to the strip.

- Blocks: [model picker] then `["traits", "mode"]`, each block preceded by a separator, plus a
  hidden overflow menu.
- When the cluster does not fit: (1) trailing blocks go icon-only, last first (labels hide;
  Brain/bolt and mode icons remain); (2) trailing blocks move into the overflow menu, last first;
  (3) the model picker shrinks to its 52px minimum; (4) below that the whole cluster hides. Coming
  back out needs 1px of slack. Re-measured on resize, mutation and font load.
- Overflow (`CompactComposerControlsMenu.tsx`): ComposerControl `EllipsisIcon` (12/16), aria "More
  composer controls", carries the `composer.mode` / `composer.effort` shortcut scopes. Popup:
  traits menu content; separator; "Mode" label + radios "Chat" (`default`) / "Plan"; separator;
  "Access" label + radios Supervised / Auto-accept edits / Auto / Full access.
- REMOVED: the July footer "Plan"/"Tasks" sidebar toggle; tasks live in the drawers (9.6).

## 11. Context strip (BranchToolbar)

Verified research distillation. `components/BranchToolbar.tsx:506-728`,
`chat/ComposerSurface.tsx:82-101`, `BranchToolbar.logic.ts`, mounted at `CV:10240-10290`.

### 11.1 Mount and geometry

- Mounted when a project exists and (git repo, or the environment indicator shows, or the route is
  a server thread: it hosts the resting controls). Visible under the same rule, except a server
  thread without git/env counts only while the resting controls fit; mounted-but-invisible strips
  are `invisible absolute inset-x0 top-full` (`BranchToolbar.logic.ts:66-77`, `CV:3826-3837`).
- `isGitRepo` = live `vcs.status.isRepo` ?? a recalled per-checkout value ?? false
  (`CV:2521-2535`).
- Wrapper `min-h32` (`CV:10246`).
- Strip (`ComposerSurface.tsx:91-95` + `BranchToolbar.tsx:621-630`): `flex items-center`, width =
  100% - 44, centered, **-16px top margin** (tucked under the card), `pt20 pb4 px10 gap4`, 12px
  `font-normal muted-foreground/70`, overflow-x clip. `::before`: inset 0, radius 18 at the
  bottom, 1px border `--chat-composer-outline` (dark white/7%), `shadow-composer` (dark
  `shadow-composer-dark` + seam), top 15px masked out. Net: 32px below the card.

### 11.2 Children (left to right)

1. Composer surface < 768px wide (`@3xl/composer-surface`, reachable on desktop with a right panel
   open): one menu `MobileRunContextSelector` (`BranchToolbar.tsx:112-313`) with trigger
   ComposerControl xs `max-w48% justify-start`: [environment icon + workspace icon, gap2] + label +
   chevron 12 at 50%; popup align start side top: "Run on" radios ([Auto balance] + environments),
   separator, "Workspace" radios. Locked: static span with the same metrics. Shortcuts
   `composer.host` + `composer.workspace`.
2. >= 768px: group `min-h24 min-w40 gap4`: [environment selector + separator mx2 h14] + workspace
   selector.
3. Composer controls host `flex min-w0 flex-1 justify-start overflow-x-clip`: the resting model
   picker / traits / mode controls portal here at size xs (`CC:6199-6215`), container `relative
   flex w-max min-w0 max-w-full items-center gap4 font-normal muted-foreground/70`, buttons 12px;
   a separator precedes them when git/env context is present (hidden under a 400px surface).
4. Branch selector `min-w0 flex-initial justify-end` (>= 768: `ml-auto`).

- Label compaction (`BranchToolbar.tsx:351-504`, `logic.ts:79-91`): when content doesn't fit the
  strip gets `data-compact` and every `[data-composer-label]` collapses to max-w 0 / opacity 0
  (icon-only); expanding back needs 16px hysteresis; width animates 180ms
  `cubic-bezier(.32,.72,0,1)` (reduced motion: none); labels cap at max-w 240.

### 11.3 Workspace (env mode) selector

`BranchToolbarEnvModeSelector.tsx`.

- Select trigger (ghost xs, ComposerControl look) `min-w0 shrink`, aria "Workspace", shortcut
  `composer.workspace`. Icon 12: `FolderGit2` (new worktree) / `FolderGit` (has worktree path) /
  `Folder`. Label "Current checkout" | "Current worktree" | "New worktree" (tooltip repeats it).
- Popup: group "Workspace", items with 12px icons gap6. "Previous worktree" item (drafts, unlocked,
  not forced): `HistoryIcon` 12 mt4 + two lines "Previous worktree" / branch (12px
  `muted-foreground`, middle-truncated); popup width min(336, 100vw-32). Source: the most recently
  updated non-archived project thread with a different worktree path (`logic.ts:127-162`). Selecting
  sets the draft `{branch, worktreePath, envMode: "worktree"}`. mod+shift+L applies it directly.
- Locked (`envLocked || (server thread && worktreePath)`; `envLocked` = thread has messages or a
  live session, `CV:2678-2682`): static span h24 px7 gap4 12px `muted-foreground/70`, label
  "Worktree" | "New worktree" | "Local checkout". Forced (multi-model): "New worktree", tooltip
  "Each model starts in its own worktree.". An unstarted server thread can change env mode and
  branch as a pending override (`CV:5916-5935`).

### 11.4 Environment selector

`BranchToolbarEnvironmentSelector.tsx`. Shown when there is more than one environment or the active
one is not primary (`logic.ts:58-64`). Icon `EnvironmentMachineIcon` 12 per machine kind (server,
cloud, linux, desktop, laptop, mac-mini, mac-studio) or `ScaleIcon` when auto-balanced. Label =
environment label; the primary one uses its saved name unless generic, then "This device"
(`logic.ts:35-53`). Select ghost xs, aria "Run on", shortcut `composer.host`; popup group "Run
on": ["Auto balance" (`ScaleIcon`), drafts with load balancing only] + environments. Static span
when locked or nothing to pick.

### 11.5 Branch selector

`BranchToolbarBranchSelector.tsx`.

- Trigger: wrapper `flex min-w0 gap4`: [PR badge] + ComboboxTrigger as ComposerControl xs
  `min-w0 max-w-full` without press scale: `GitBranchIcon` 12 at 70% + label (middle-truncated,
  max-w 240) + chevron 12 at 50%. Disabled while refs first load or an action is pending.
  Right-click: desktop context menu "Copy branch name" → toast "Branch name copied".
- Label (`logic.ts:212-237`): "Select ref" | "From {branch}" (new worktree, no path yet; "From
  origin/{branch}" when start-from-origin is on and the branch is local) | branch. The optimistic
  branch shows during a switch.
- PR badge (`ThreadStatusIndicators.tsx:151-283`): ComposerControl xs with icon 12 + 12px tabular
  text in the state tone: open `emerald-600` / dark `emerald-300/90`, draft `zinc-500` /
  `zinc-400/80`, closed `red-600` / `red-300/90`, merged `violet-600` / `violet-300/90`; glyphs
  `GitPullRequestArrow` / draft / closed / `GitMerge`. Text "#n", "+N" when several are linked, or a
  `LayersIcon` count for stacks. Click opens the URL (stacks/multi open the PR list). Tooltip "{PR}
  #{n} - {State}: {title}".
- Popup: combobox align end, side top, w320, flex-col, glass, max-h min(available, 368).
  - Search placeholder "Search refs...". "No refs found." (14px p8) sits before the list.
  - List max-h224, LegendList `ps4 pe0 pt8 pb4`, est. row 28, draw distance 336, 24px fades
    (bottom up front with > 8 items), resets to top on query change. Next page when scrolling down
    within 96px of the bottom (`state/paginatedBranches.ts:5-36`; `vcs.listRefs` with cursor, page
    100, `state/queries.ts:37`).
  - Items: PR checkout first (local drafts, query parses as a PR reference): source-control icon 14
    `muted-foreground` + "Checkout {term}" (`font-medium`) / reference 12px muted → PR thread
    dialog. Refs: middle-truncated name + badge 10px `muted-foreground/45` (current | worktree |
    remote | default); the current ref has the selected bg. 'Create new ref "{sanitized}"' (not
    when picking a worktree base or when the name exists). Filter: lowercase substring or
    sanitized-query substring (`logic.ts:291-325`).
  - "Start from origin" row (worktree base only): `border-t border/60 px12 py8 gap12 12px`;
    `RefreshIcon` 12 + "Start from origin" (`font-medium muted-foreground`) + Switch sm (thumb 14,
    track 26x16, checked `primary`, unchecked `input`); tooltip "Creates the worktree from the latest
    matching branch on origin instead of your local branch.". Default from setting
    `newWorktreesStartFromOrigin`.
  - Status line at the bottom (px12 py8 12px `font-medium muted-foreground`): "Loading refs..." /
    "Loading more refs..." / "Showing N of M refs".
- Actions (`:415-538`): picking a worktree base only sets the branch (default base = repo default
  branch, then current). A ref with a worktree reuses its path. Otherwise `vcs.switchRef {cwd,
  refName}` (remote refs switch to the returned local name), create = `vcs.createRef {cwd, refName,
  switchRef: true}`. Server threads: `thread.session.stop` if the worktree path changes, then
  `thread.meta.update {branch, worktreePath}`. Drafts: update the draft thread context. Errors
  (toasts): "Failed to switch ref." / "Failed to create and switch ref.". mod+shift+G opens the
  picker.

## 12. Keyboard (composer-relevant)

Defaults from `shared:keybindings.ts:21-87`; server-provided keybindings override them. `mod` = ⌘ on
macOS. `when` contexts: `terminalFocus`, `terminalOpen`, `modelPickerOpen`, `editableFocus`,
`isDesktop`, `isWeb`. Global handlers ignore events while the command palette is open.
`composer.*` commands go through `openControl`: expand the composer, find the visible, non-inert
`button[data-composer-shortcut~=<cmd>]` in the composer shell, focus and click it
(`CC:5964-5985`, `CV:6963-6994`).

| Keys | Command / where | Effect |
|---|---|---|
| Enter / Shift+Enter / ⌘+Enter | editor | 8.2 |
| ↑ / ↓ | editor | menu navigation, else prompt history at the visual first/last line (13.1) |
| Tab / Enter | editor, menu open | select active item |
| Tab | editor, list line | indent two spaces |
| Esc | editor, menu open | dismiss the trigger |
| ← / → next to a chip | editor | jump over the chip |
| `( [ { ' " “ \` < « * _` with a selection | editor | surround |
| Home / End (macOS) | editor | visual line boundary |
| PageUp / PageDown | editor | scroll the timeline when the editor can't scroll |
| mod+shift+V | editor / paste-to-focus | paste as text (no folding, no image chip) |
| mod+S | `composer.stash` (`!terminalFocus`) | stash / restore / toggle stash menu (13.3) |
| mod+shift+Enter | `thread.steerQueuedMessage` | send the first queued message now |
| mod+shift+M | `modelPicker.toggle` | toggle the model picker |
| mod+1..9 | `modelPicker.jump.N` (`modelPickerOpen && isDesktop`) | pick Nth selectable model |
| mod+shift+↑ / ↓ | `modelPicker.previous/nextProvider` | cycle the rail |
| mod+shift+E | `composer.effort` | open traits (or overflow) menu |
| mod+shift+A | `composer.mode` | open access select (or overflow); no-op without mode controls |
| mod+shift+H | `composer.host` | environment selector |
| mod+shift+X | `composer.workspace` | workspace selector |
| mod+shift+G | `composer.branch` | branch picker |
| mod+shift+L | `composer.previousWorktree` | move the draft to the previous worktree |
| 1-9 (focus outside editables) | question card | pick option |
| ↑ / ↓ / Enter / mod+Backspace / Esc | stash menu (capture) | navigate / restore / delete / close |
| printable key outside editables | chat view | type-to-focus (4.4) |
| (none by default) | `thread.stop` | interrupt (`CV:7007-7015`) |

## 13. Drafts, history, stash

### 13.1 Prompt history (↑ / ↓)

`chat/composerPromptHistory.ts`, `CC:3936-4006`. Entries = the thread's user messages (including
optimistic ones), oldest first, consecutive duplicates collapsed, built on keypress. Recalled text
has send-time additions stripped (the "Ultrathink:" prefix, trailing context blocks, all context
links); attachment-only and plan-implementation sends are skipped. ↑ recalls older, ↓ newer; ↓
past the newest clears the prompt. Only when: no
modifiers, not composing, no approval or question, no images, files, terminal contexts,
annotations or review comments, the prompt is empty or currently showing a recall, and the caret is
on the first (↑) / last (↓) visual line (soft wraps count, `ED:1253-1282`). Any edit ends browsing.
Resets per thread.

### 13.2 Draft store

`composerDraftStore.ts:84-360,2091-2299,4052-4060`. localStorage key `t3code:composer-drafts:v1`,
**version 9**, writes debounced 300ms, flushed on `beforeunload`.

- `draftsByThreadKey[key]` (key = draft id, or `environmentId:threadId`): `prompt`,
  `attachments[]` (images as data URLs: `{id, name, mimeType, sizeBytes, dataUrl, source?}`),
  `files[]` (`{id, name, mimeType, sizeBytes, source?, attachmentId?, environmentId?}`; a file
  without `attachmentId` hydrates as needs-reattach), `terminalContexts[]` (metadata **and** `text`,
  so they no longer expire on reload), `previewAnnotations[]`, `reviewComments[]`,
  `modelSelectionByProvider{instanceId → {instanceId, model, options}}`, `activeProvider`,
  `modelSelectionExplicit`, `runtimeMode`, `interactionMode`. Empty drafts are not persisted.
- `draftThreadsByThreadKey[draftId]`: `{threadId, environmentId, projectId, logicalProjectKey?,
  environmentSelection? ("auto"|"manual"), loadBalancedEnvironmentId?, createdAt, runtimeMode,
  interactionMode, branch, worktreePath, envMode ("local"|"worktree"), startFromOrigin,
  promotedTo?}`. Only mapped, promoting, or content-holding drafts persist.
- `logicalProjectDraftThreadKeyByLogicalProjectKey`, `stickyModelSelectionByProvider`,
  `stickyActiveProvider` (seed new drafts, `applyStickyState`).
- Images are stored as data URLs after each change (`CC:3261-3330`); an image whose read fails is
  marked non-persisted (warning badge, 7.6).
- Model resolution (`ChatView.logic.ts:557-610`): candidates draft `activeProvider` → session
  instance → thread `modelSelection.instanceId` → project default; filtered by the lock; first
  enabled+available candidate; else within the requested driver the first picker-ready entry, then
  the first enabled+available non-error entry; then across compatible entries; else no provider (send
  blocked). The July `codex` fallback is gone. Lock (`deriveLockedProvider`,
  `ChatView.logic.ts:1028-1054`) applies to started threads only. Model
  (`composerDraftStore.ts:1170-1257`): draft per-instance selection → thread model ?? project
  default resolved for the instance → driver default. Options: draft → thread → project.
- The Rust port can use its own file (`drafts.json`) but must keep: per-target drafts, sticky model,
  draft sessions, terminal context text, needs-reattach markers, send-failure restore.

### 13.3 Prompt stash (⌘S)

`promptStashStore.ts`, `CC:4100-4720,5236-5280`, `chat/ComposerStashBadge.tsx`,
`chat/ComposerStashMenu.tsx`.

- localStorage `t3code:prompt-stash:v2`, max **20** entries (oldest evicted with warning toast
  "Oldest stashed prompt discarded" / "The stash holds 20 prompts; the oldest was removed to make
  room."). Entry: `{id, createdAt, prompt, attachments (re-encoded images), files (uploaded only),
  records (contexts), droppedImageNames, unreadableImageNames, pendingImageCount}`. Files tie to
  their environment.
- ⌘S: with content → stash it (text first, images re-encoded after), clear the composer, pulse the
  badge (1200ms). Empty composer with exactly one finished entry → restore it; otherwise toggle the
  menu. During a question: toggle the menu. Disabled during approval, project selection, checkpoint
  revert, or an open command palette. Always prevents the browser save dialog.
- Restore appends the entry's prompt to the current one with a blank line (`\n\n`) and re-adds its
  attachments and records. Uploaded stash files expire server-side after 24h: "{names}: stashed
  files are kept for 24 hours and this upload expired. Attach the file again." (`CC:4386-4389`).
- Errors: "Attach dropped files again or remove them before stashing", "Wait for file uploads before
  stashing this prompt", "Could not stash this prompt" (storage rejected), "Stashed prompt will not
  survive a reload", "Stashed images were not saved", "Stashed images did not attach", restore:
  "Stashed files belong to another environment" / "Restore this prompt in the environment that
  received its files.", "Restored prompt may reappear in the stash", "Some attachments were not
  restored", delete: "Stash entry may come back".
- **Stash tab** (dock, right of the drawers, hidden during approval; only when count > 0): Root
  comfortable, content width, `ml-auto`; Row button: `BookmarkIcon` + "Stash" + Count; fg
  `muted-foreground` (hover `foreground`), `foreground` while open or pulsing; on stash the count
  turns `primary` and slides up 2px + fades in (180ms ease-out, no loop). aria "Stashed prompts: N.
  Open stash."
- **Stash menu** (same fixed drawer as the command menu, 6.6, while the command menu is closed):
  header Row button "Stash" + count + ToggleIcon (aria "Close stash"); list (Scroll): per entry
  `FileTextIcon` + snippet button (prompt plain text, whitespace collapsed, 90 chars + "…", or
  "(N images/files/attachments)" / "(empty)"), then "saving N images…" | "N images dropped"
  (`warning-foreground`) | up to N 16px thumbnails (radius 4 `border/70`) | `FileIcon` 12 + file
  count; relative time (tabular); Dismiss "Delete stashed prompt". Highlighted row bg `accent`.
  Empty: "Nothing stashed yet. Press ⌘S with a prompt in the composer to stash it.". Keys:
  ↑/↓ (wrap), Enter restore, mod+Backspace delete, Esc close; outside pointer-down closes; typing
  closes it.

## 14. Settings that change the composer

| Setting (client) | Default | Effect |
|---|---|---|
| `chatWidth` | "comfortable" | column 768 / 1152 / 100% |
| `composerRichTextEnabled` | true | rich marks + task lists (4.3) |
| `composerCollapseOnScroll` | true | resting composer (3) |
| `sendShortcut` | "enter" | 8.2 |
| `followUpBehavior` | "queue" | 8.3 |
| `showSkillsInSlashMenu` | true | `/skill:` rows |
| `planModeEnabled` | false | Plan/Build toggle, `/plan`, `/default`, plan follow-up |
| `accessLevelIndicatorEnabled` | false | access select |
| `contextWindowMeterEnabled` | false | footer meter |
| `resumeCompactionBannerEnabled` | false | resume compaction banner |
| `snapShotAnimations` | true | snap-shot arrival animation |
| `panelAnimationDurationMs` | 0 | resting / hero transitions |
| `favorites`, `providerModelPreferences` | [] | model picker |
| `newWorktreesStartFromOrigin` (server settings) | per server | start-from-origin default |
| appearance `--font-composer`, `--font-size-prompt` | sans, 14 | editor font |

All in `contracts:settings.ts:295-520`.

## 15. Data and client API map

| Need | Wire (`contracts:`) | t3UI today |
|---|---|---|
| Send / steer | `orchestration.dispatchCommand` `thread.turn.start` (+`message.context`, bootstrap) | `t3_client::commands::turn_start` / `new_thread_turn`; `t3_protocol` `TurnStart` has `context`, `bootstrap`, `source_proposed_plan` |
| Interrupt | `thread.turn.interrupt` | `commands::interrupt_turn` |
| Mode / meta | `thread.runtime-mode.set`, `thread.interaction-mode.set`, `thread.meta.update`, `thread.session.stop` | `set_runtime_mode`, `set_interaction_mode`, `update_thread`; session stop variant exists in `t3_protocol::commands` |
| Approvals / questions | `thread.approval.respond`, `thread.user-input.respond` (+ per-question `attachments`), `thread.user-input.dismiss` | `respond_to_approval`, `respond_to_user_input`, `dismiss_user_input`; pending derivation `t3_client::pending_requests` (same algorithm as `crt:pendingRequests.ts`). Check the `attachments` field on user-input respond (missing if absent) |
| `@` search | `projects.searchEntries {cwd, query, limit}` | `ProjectsSearchEntries` |
| `#` PR search | `pullRequests.list {state, projectId, limit, query?}`, `pullRequests.detail {projectId, repository, number}` | **missing** |
| Uploads | `attachments.createUploadUrl` + HTTP POST; `attachments.delete`; `assets.createUrl` (verify, chip import) | `AttachmentsCreateUploadUrl`, `AttachmentsDelete`, `AssetsCreateUrl`, `Environment::upload_attachment` |
| Provider skills per cwd | `server.refreshProviders {instanceId, cwd}` → `workspaceSnapshots` (retry cooldown 10s, `CC:1975-2025`) | `ServerRefreshProviders` (has `cwd`); `ServerProvider.workspace_snapshots` is an untyped `Value` (needs typing) |
| Capabilities | `environment.capabilities.{attachmentUploads, fileAttachments.maxUploadBytes, questionAttachments, inlineMessageContext, requiredWorktreeBootstrap}` | present in `t3_protocol::environment` |
| Provider fields | `showInteractionModeToggle`, `reportsContextWindow`, `requiresNewThreadForModelChange`, `slashCommands`, `skills`, `continuation`, model `options` descriptors | present in `t3_protocol::server` |
| Branch strip | `vcs.listRefs` (cursor), `vcs.switchRef`, `vcs.createRef`, `subscribeVcsStatus` (stream) | first three present; **`subscribeVcsStatus` missing** (only `vcs.refreshStatus`) |
| Tasks | `turn.plan.updated` activity | in thread activities |
| Context meter | `context-window.updated` activity | in thread activities |
| Client settings | the rows of 14 | **missing** in `t3_logic::settings::ClientSettings`: `chatWidth`, `composerRichTextEnabled`, `composerCollapseOnScroll`, `sendShortcut`, `followUpBehavior`, `showSkillsInSlashMenu`, `planModeEnabled`, `accessLevelIndicatorEnabled`, `contextWindowMeterEnabled`, `resumeCompactionBannerEnabled`, `snapShotAnimations`, `panelAnimationDurationMs` (has `favorites`, `providerModelPreferences`) |
| Keybindings | server `keybindings` incl. `composer.*`, `thread.steerQueuedMessage`, `modelPicker.*` | `crates/t3-logic/src/keybindings` (check the new commands and `isDesktop` / `editableFocus` contexts) |

## 16. Reuse map

| Module | Status | Notes |
|---|---|---|
| `crates/t3-app/src/chat/mod.rs` slot API (`register_slot(Slot::Composer / BranchToolbar)`, `SlotContext`, `begin/end_local_dispatch`) | fits, needs changes | The doc comment still cites `composerTimelineGeometry.ts` (75% rule); the inset is now `resolveComposerTimelineInset` (1). The composer must publish its resting flag and the destination overlay height. The context strip is part of the composer silhouette (tucked under the card), so the BranchToolbar slot should render inside the composer's Shell rather than as a separate strip, or the composer slot should own both. |
| `t3_client::pending_requests` + `commands::respond_to_*` / `dismiss_user_input` | fits as-is | Same algorithm as the fork. |
| `origin/composer:crates/t3-logic/src/composer/prompt.rs`, `search.rs`, `menu.rs` | reusable, re-diffed to fe7d3092c | Chip grammar, triggers incl. `#`, `/skill:` rows, prompt-start rule, ranking constants. Verify the slash tie-break order (built-ins, provider, skills) and the 12-result PR cap. |
| `.../composer/send.rs` | reusable, update | Title seed now has "File: " and "Review: " fallbacks; image limit is 100 attachments (8 was July); add queue/steer and Enter intents (8.2/8.3). |
| `.../composer/providers.rs` | July-era, update | Resolution order changed (no `codex` fallback, 13.2), raw server labels (no "Light"/"Ultra" relabeling), speed bolt, service tier, implicit fastMode false, legacy models section. |
| `.../composer/pending.rs` | reusable | Question answer state; add the 200ms auto-advance and per-question attachments. |
| `.../composer/draft.rs` | July-era shape | Needs v9 fields: files with `attachmentId`, terminal `text`, `modelSelectionExplicit`, draft thread `environmentSelection`, `startFromOrigin`, `promotedTo`. |
| `origin/composer:crates/t3-app/src/composer/editor.rs`, `chips.rs` (gpui-base `InlineToken`) | partially fits | InlineToken gives atomic chips, caret skipping, whole-chip delete and undo with the prompt string as the value, which matches the Markdown-string model and plain mode exactly. It cannot do the default **rich mode**: bold/italic/strike/code styling, marker reveal widgets and task-list checkboxes need text decorations, which gpui-base only has in `EditorMode`, which forbids tokens (`docs/handoff/composer.md`). Chips also need em-scaled metrics, per-kind accents, popovers on skill/review/annotation chips, and node-selection rings. Options: (a) ship plain mode first (identical to the fork with `composerRichTextEnabled = false`), (b) a custom editor element for rich mode. |
| `.../composer/drafts.rs` (`DraftStore`) | reusable mechanics | 300ms debounce, quit flush, draft promotion. Add the stash store (separate file, 20 entries). |
| `.../composer/mod.rs` (send pipeline, approvals, questions, paste/drop, path search) | mechanics reusable, render July-era | Replace render with sections 2-9; add the upload queue, queue/steer, stash, history. |
| `.../composer/footer.rs`, `model_picker.rs`, `traits.rs`, `command_menu.rs`, `panels.rs`, `branch_toolbar.rs`, `style.rs`, `t3-snapshots/src/scenes/composer.rs` | July visuals, replace | Command menu is now a fixed drawer behind the card; approval/question are attached drawers; branch toolbar is the context strip; ComposerControl replaces the old xs buttons. The scene harness is reusable. |
| `crates/t3-app/src/composer/chat_slots.rs` | glue, reusable | Never compiled against main's ChatView. |

## 17. Reference screenshots needed

All at 1440x900 @2x, dark and light, seeded by `e2e/seed.mjs` with the fake Codex unless noted.
"Showcase" = the seeded aurora-web showcase thread.

1. `composer-idle-thread` — showcase thread scrolled to the end, composer focused, empty: card,
   footer (model picker, traits, attach, send), context strip with workspace + branch.
2. `composer-resting` — showcase thread after a wheel scroll up of >= 24px (composer collapses to
   the 50px row with controls in the strip). Needs `composerCollapseOnScroll` default.
3. `composer-draft-hero` — new draft (mod+N): centered composer + "What should we build in
   aurora-web?" headline.
4. `composer-typed-multiline` — showcase, type three lines including `**bold**`, `` `code` `` and
   `- [ ] task` with the caret inside the bold word (marker reveal).
5. `composer-chips` — prompt with a file mention (`@` → pick `src/format.ts`), a skill (`$` → pick
   one), and an attached image (paste a PNG) plus a 40KB text paste (folded `pasted-text.txt`
   chip).
6. `composer-slash-menu` — type `/` in an empty composer (drawer above the card).
7. `composer-at-menu` — type `@for` (path results).
8. `composer-skill-menu` — type `$` (skills with source badges); needs a fake provider with skills.
9. `composer-model-picker` — ⇧⌘M open (rail + list + Kbd jump labels); second shot with a
   search "gpt".
10. `composer-traits-menu` — traits menu open (Reasoning + Service Tier); second shot with Fast
    selected (bolt in the trigger).
11. `composer-running-stop` — thread-running seed, empty composer (Stop square).
12. `composer-running-queue` — thread-running seed with text typed (ListPlus "Queue message"), then
    after Enter: queued row at the timeline end.
13. `composer-approval` — approval-required seed (`pnpm db:migrate`): warning drawer with Decline /
    Approve / more menu; composer with placeholder only.
14. `composer-question` — borealis-api question seed: question drawer, options with digits; second
    shot after picking option 1 (selected row + check).
15. `composer-banner-stack` — environment offline (stop the server) plus a server-update notice:
    front banner + peek; hover shot with the stack expanded.
16. `composer-tasks` — a running turn with `turn.plan.updated` steps: tasks tab; expanded list.
17. `composer-stash` — type a prompt, ⌘S (badge pulse), then open the stash menu.
18. `composer-branch-picker` — branch combobox open from the strip ("Search refs...", refs with
    badges, status line).
19. `composer-workspace-menu` — workspace select open on a draft (Current checkout / New worktree /
    Previous worktree).
20. `composer-legacy-modes` — with `planModeEnabled` and `accessLevelIndicatorEnabled` on: footer
    with access select + Build toggle; access select open.
21. `composer-narrow-strip` — window 1100px wide with the right panel open (composer surface < 768):
    combined run-context menu and compacted labels.
22. `composer-drag-over` — dragging a file over the composer (accent bg + primary ring); may need a
    scripted drag event.

## 18. Open questions / risks

1. **Rich-text editor in GPUI.** The default is rich mode (marks, marker reveal, task checkboxes,
   em-scaled interactive chips). gpui-base tokens and decorations don't combine. Decide: plain mode
   first (parity only with the setting off) or a custom editor element now.
2. **Glass.** Composer, drawers, strip and popups are blurred glass at 80% opacity; GPUI draws solid
   fallbacks. If the native window gets vibrancy later, revisit (also affects how the timeline shows
   through under the card).
3. **Command menu overlap.** The fixed drawer's visible edge ends 8px inside the card's top edge
   (6.6). Confirm against a screenshot before matching it pixel for pixel.
4. **Shift+Tab plan toggle** is promised by the settings copy but not wired at fe7d3092c
   (`CC:4016-4018` returns false). Match the code (no-op) unless the user wants the promise kept.
5. **Queue is in memory only** (lost on quit) and sends only when the turn fully ends; steer
   dispatches a plain `thread.turn.start`. Both match the fork; confirm upstream servers accept
   turn.start while running (the fork relies on it).
6. **Continuous animations**: spinners (send busy, sync row) and the draft/hero transitions are the
   only ones in the default composer; the ultrathink ring is static. Resting transitions default to
   0ms.
7. **Snap-shot captures, video thumbnails, ChatGPT plan sharing, background liveness, Codex
   feedback, project clone, multi-model sends, stage-backdrop artwork** are fork features with their
   own back ends; they are specced only to the level needed to place them. Port later.
8. **Container queries** (`@max-[400px]`, `@max-[560px]`, `@max-[640px]`, `@3xl` = 768) key off
   the composer surface or banner width, not the window; implement with measured widths.
9. **`#` PR search and `subscribeVcsStatus`** need protocol/client work before the menu and the strip
   can match. Quirk to keep: `#` opens the menu even in projects without PR support and shows
   "Pull requests are not available for this project.".
10. **Draft persistence format**: web localStorage drafts can't be read from Rust; users switching
    lose web drafts and stash (same decision as before).
