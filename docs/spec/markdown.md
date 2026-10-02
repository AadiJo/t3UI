# Chat markdown build spec (`ChatMarkdown`)

> Refreshed against fe7d3092c (2026-10-02). Conventions (paths, units, radius, tokens, tooltip
> defaults) are in [`chat.md`](chat.md) section 0. Paths are relative to
> `~/L-Projects/t3UI-refs/t3code-fork/apps/web/src/` unless prefixed `crt:` / `shared:`.

`components/ChatMarkdown.tsx` (3,409 lines) renders every markdown surface in the chat: assistant
messages, reasoning traces, user messages, proposed plans, review-comment cards. It is one
`react-markdown` document per message (no per-chunk documents anymore; the July
`streamingMarkdown.ts` chunker was removed).

## 1. Hosts and props

`ChatMarkdownProps` (`ChatMarkdown.tsx:205-239`). The variants that matter:

| Host | Props | Effect |
|---|---|---|
| Assistant message (`chat-timeline.md`) | `isStreaming`, `cwd` = git cwd, `threadRef`, `skills`, `onRunShellCommand`, `onUseArtifactTemplate`, `onImageExpand`, `headingLevelOffset` | Body text `foreground` at 80% |
| User message (`MessagesTimeline.tsx:4033-4056`) | `lineBreaks`, `parseRawHtml={false}`, `className="text-message-foreground"`, `renderContextReference`, `headingLevelOffset=3` | Single newlines are hard breaks; raw HTML shows as literal text; body color `message-foreground` (replaces the 80% foreground) |
| Review comment fence (`MessagesTimeline.tsx:4086-4094`) | `className="text-message-foreground"` | |
| Proposed plan card | (timeline spec) | |

`headingLevelOffset` only changes `aria-level`; the rendered tag and style stay.

## 2. Pipeline

`ChatMarkdown.tsx:473-534,3340-3407`.

1. Pre-transform: `renderClaudeInsightsAsMarkdown(text)` (`crt:claudeInsights.ts`) rewrites
   Claude "★ Insight" boxes into markdown.
2. remark plugins in order: `remark-gfm` (^4.0.1), `remarkGithubAlerts` (`markdown-github-alerts.ts`),
   `remarkNormalizeListItemIndentation` (`markdown-list-indentation.ts`), `remarkCodexDirectives`
   (Codex `::artifact-template` and file-citation directives, `crt:codexMarkdownDirectives.ts`),
   `remarkChatMath` (`markdown-math.ts`: `$…$` with Pandoc delimiter rules, `$$…$$` and `\[…\]`
   display), [`remark-breaks` ^4.0.0 when `lineBreaks`], `remarkPreserveCodeMeta` (fence meta →
   `data-code-meta`), `remarkNormalizeLinksAndTagInlineCode` (Windows drive links → `file:///`;
   tags inline code outside links with `data-inline-code`).
3. While streaming and the text contains a code fence: `createIncrementalMarkdownPlugin`
   (`markdown-incremental.ts:37-107`) caches the parse of everything up to the last **closed
   top-level fence followed by a blank line** and parses only the rest. Bails to a full parse on
   `\r`, BOM, or any link/footnote definition. Pure performance; output identical.
4. rehype: `rehype-raw` → `rehypePreserveImageSourceMeta` (marks an image that is the only content
   of `p/div/li/td/th/figure/center/blockquote` or the root, optionally inside `a/strong/em`, as
   `data-standalone`) → `rehype-sanitize` with `CHAT_MARKDOWN_SANITIZE_SCHEMA` (`:473-501`: no
   `title` attribute anywhere; allows `language-*` / `math-display` classes, the data attributes
   above, `data-alert`, `data-pull-request-autolink`; extra protocols `file`, `t3-citation`,
   `t3-context` for href and `file`, `t3-context` for src) → `rehypeMathCopySource` →
   `rehype-katex` (^7.0.1, KaTeX ^0.16.47). With `parseRawHtml=false` only the two math plugins
   run, and raw HTML renders as literal text (`skipHtml={false}`).
5. `urlTransform`: keeps `t3-citation:`, `t3-context:`, Windows drive paths; rewrites `file:` URLs
   to paths; otherwise react-markdown's default (`:2466-2471`).

## 3. Root container

`ChatMarkdown.tsx:3374-3385`, `index.css:1757-1768`.

```
div.chat-markdown  w-full min-w-0 text-sm (14px) leading-relaxed (1.625 → 22.75px)
    color: foreground at calc(80% + contrast-boost/5) = 80% by default
    overflow-wrap: anywhere; word-break: break-word
    data-streaming present while the message streams
```

- First child `margin-top: 0`, last child `margin-bottom: 0`.
- `--appearance-contrast-boost` defaults to 0% (`index.css:83`).

## 4. Block elements

All values from `index.css:1757-2110` unless noted. 1rem = 16px; `contrast-*` tokens are the
contrast-adjusted versions of the named token (equal to the plain token at default contrast).

| Element | Style |
|---|---|
| `p`, `ul`, `ol`, `blockquote`, `pre`, table container | margin 0.65rem (10.4px) top and bottom (CSS margins collapse) |
| `h1`–`h6` | margin 1.25rem 0 0.5rem (20 / 8px), weight 600, line-height 1.3, color `contrast-foreground`. Sizes: h1 1.25rem (20px), h2 1.125rem (18px), h3 1rem (16px), h4–h6 0.875rem (14px). h6 color `contrast-muted-foreground` |
| `ul` | padding-left 1.25rem (20px), `disc`; nested `circle`, then `square` |
| `ol` | padding-left `--list-gutter` (1.25rem; widened to `(digits + 1)ch` when the widest marker has 2+ characters, `ChatMarkdown.tsx:384-394`), `decimal`, markers tabular-nums; nested `lower-alpha`, then `lower-roman` |
| `li + li` | margin-top 0.25rem (4px) |
| Task list item | no marker; the checkbox sits in the gutter: margin `0 0.35em 0.15em -gutter`, `vertical-align: middle`. Read-only in chat (`ChatMarkdown.tsx:2866-2893`; toggling only exists for markdown files) |
| `blockquote` | border-left 2px `contrast-border`, padding-left 0.8rem (12.8px), color `contrast-muted-foreground` |
| GitHub alert (`> [!NOTE]` …) | not a blockquote: `div role="note"` margin-y 4px, border-left 2px, padding-left 12px; title row `p` flex gap 6px font-medium with a 14px icon; body = normal text. Kinds (`ChatMarkdown.tsx:537-571`): Note `InfoIcon` border `blue-500/70` title `blue-600` (dark `blue-400`); Tip `LightbulbIcon` `emerald-500/70`, `emerald-600`/`emerald-400`; Important `MessageSquareWarningIcon` `purple-500/70`, `purple-600`/`purple-400`; Warning `TriangleAlertIcon` `amber-500/70`, `amber-600`/`amber-500`; Caution `OctagonAlertIcon` `red-500/70`, `red-600`/`red-400` |
| `hr` | preflight only: 1px top border in `contrast-border`, no own margin |
| Footnotes section | margin-top 1.25rem, border-top 1px `contrast-border`, padding-top 0.75rem, color `contrast-muted-foreground`, 12px; `li + li` 0.35rem; refs/backrefs inline-flex min-width 1rem, radius 4px, 11px, weight 600 |
| `img` | `display: inline-block` (images flow inline like GitHub badge rows; see 6) |
| `details` | section 4.3 |

### 4.1 Paragraph and list text children

`p` and `li` children run through `renderSkillInlineMarkdownChildren`
(`chat/SkillInlineText.tsx:41-66`): text tokens `$name` (currency sign, name with a letter, not a
number like `$5k`) whose name matches a provider skill become a skill chip (ContextChip
`kind="skill"`, `SkillChipIcon` + display name, copy text `$name`). Not inside `code` or `a`.

### 4.2 Codex artifact template card

`ChatMarkdown.tsx:298-342`, rendered for a `div` carrying artifact-template properties.

```
div role=group aria-label "{displayName} template"  my 10.4px, flex w-full items-center gap-3 (12)
    rounded-xl (14) border border/70 bg-card/60 px-3 py-2.5 (12/10) shadow-xs
├─ icon box 36x36 rounded-lg border border/70 bg-background text-muted-foreground shadow-xs
│    icon 20px by kind (FileText / Presentation / FileSpreadsheet / Globe / Image / Mail / MessageSquare)
│    + badge 16x16 at bottom-right (−4px) rounded-full border-background bg-primary
│      text-primary-foreground shadow-xs, SparklesIcon 10px
├─ texts: name text-sm font-medium truncate; kind label text-xs text-muted-foreground
└─ [Button size=sm variant=outline "Use template"] when the host passes onUseArtifactTemplate
```

### 4.3 `details` / `summary`

`ChatMarkdown.tsx:869-914`, `ui/collapsible.tsx:21-32`.

```
div my-2 (8) border-y border/60
└─ Collapsible (defaultOpen = <details open>)
   ├─ trigger  flex w-full items-center gap-2 py-2 (8) text-left text-sm font-medium text-foreground
   │    ChevronRightIcon 16px text-muted-foreground, rotates 90° when open (transition-transform)
   │    + summary text (default "Details")
   └─ panel  height animates 200ms (none under reduced motion)
        div pb-3 (12) ps-6 (24) color foreground at 80%
```

## 5. Code

### 5.1 Inline code

`index.css:1958-1971`: border 1px `contrast-border`, radius 0.375rem (6px), background `muted`
(dark: `code-background`), padding 0.1rem 0.35rem (1.6 / 5.6px), color `contrast-foreground`,
font `--font-mono` at 0.75rem (12px). Inline code whose text looks like a file path becomes a file
chip instead (rules in 6.2).

### 5.2 Code block chrome

`ChatMarkdown.tsx:957-1100`, `index.css:1972-2034,1696-1704`.

```
div.chat-markdown-codeblock  my 10.4px, overflow-hidden, radius 10px (var(--radius)),
    light: 1px border border/70, bg secondary
    dark:  border transparent, bg code-background (CSS rule beats dark:bg-input/32)
    line-height leading-snug (1.375)
    data-language, data-wrap
├─ header  flex items-center justify-between gap-2, padding 6px 6px 0 12px, select-none
│    color contrast-foreground at 72%  (dark: bg code-background)
│  ├─ title  inline-flex min-w-0 items-center gap-1.5 (6) font-mono text-2xs (11px)
│  │    fence title (```ts title="x.ts", ```ts src/main.ts, filename=…):
│  │        PierreEntryIcon 14px + filename (truncate)
│  │    else, language has a specific Pierre icon: icon 14px only,
│  │        aria-label "Language: {lang}", tooltip (top) = language
│  │    else: language text (truncate); missing language = "text"; "gitignore" → "ini"
│  └─ toolbar role=toolbar aria-label "Code block actions"  flex gap-0.5 (2)
│       ├─ wrap toggle  Button icon-xs (24x24)  variant secondary when wrapped, else ghost-muted
│       │    WrapTextIcon 12px; aria-pressed; aria + tooltip (top) "Disable line wrap" / "Wrap lines"
│       ├─ [run]  Button icon-xs ghost-muted, PlayIcon 12px, aria + tooltip "Run in terminal"
│       └─ copy   Button icon-xs ghost-muted, CopyIcon → CheckIcon 12px for 1200ms,
│                 aria + tooltip "Copy code" → "Copied"
└─ pre (Shiki output)  margin 0, no border, radius 0, transparent background,
     padding 0.8rem 0.9rem (12.8 / 14.4px), overflow-x auto,
     horizontal scrollbar 7px tall, thumb contrast-border/78 radius 999, track transparent
     wrap on: white-space pre-wrap, overflow-wrap anywhere
     code: --font-mono at --font-size-code (13px default) → line height 17.875px
```

- Wrap starts from the `wordWrap` client setting (default **true**), per block afterwards
  (`ChatMarkdown.tsx:737-739,975`).
- Run button only when the host passes `onRunShellCommand` (assistant rows with a project), the
  message is not streaming, the fence is closed in the source, the language is `sh|bash|zsh|
  fish|shell|powershell|pwsh`, the code ends with a newline, is non-empty, does not end with `\`,
  and contains no control / format characters (`:980-989,3305-3309`). Runs the trimmed command in
  the thread's terminal.
- Copy writes the raw code (not trimmed) as `text/plain`.
- Themed palettes swap the surface to `code-background` / `code-foreground`
  (`index.css:1439-1456`).

### 5.3 Highlighting

`lib/syntaxHighlighting.ts:1-39`, `lib/diffRendering.ts:7-13`, `lib/incrementalHighlighting.ts`,
`components/chat/HighlightedCodeLines.tsx`, `ChatMarkdown.tsx:1102-1205,3288-3337`.

- Shiki 4.2.0 through `@pierre/diffs` 1.3.0-beta.10 `getSharedHighlighter`, Oniguruma WASM engine
  (`PREFERRED_HIGHLIGHTER = "shiki-wasm"`). Themes **`pierre-dark`** / **`pierre-light`** from
  `@pierre/theme` 1.1.0 (colors identical to 1.0.3, only the theme `name` changed). The Shiki
  `pre` background is forced transparent (`index.css:2032-2034`); the block surface shows through.
- Language resolution: the fence language as-is; unsupported languages fall back to `text`.
- Until the grammar loads, the block renders the plain code **invisible** (same size) so height
  is reserved and unstyled text never flashes (`:3316-3326`). On highlight error: plain `text`.
- Finished blocks: highlighted HTML cached in an LRU (500 entries / 50MB) keyed by
  `fnv1a(code):length:language:theme`.
- Streaming blocks: incremental highlighting resumes from the grammar state after the last
  complete line and re-highlights only the current line; lines render as separate nodes so
  finished lines never re-render. A block that streamed keeps the per-line renderer after the
  stream ends (no swap to cached HTML, which would clear a selection).

### 5.4 Fence title rules

`ChatMarkdown.tsx:580-608`: meta `title=…`, `file=…` or `filename=…` (quoted or bare), else the
first meta token shaped like `name.ext` (`/^[\w@][\w@./-]*\.[A-Za-z0-9]+$/`).

## 6. Links, chips and media

### 6.1 Ordinary links

`ChatMarkdown.tsx:2894-3129`, `index.css:1864-1885`.

- Color `--info-foreground`, no underline. Hover / focus-visible: a dotted underline drawn as a
  background (radial dots: 0.75px radius, repeating every 4px, 2px tall, at the bottom).
- External `http(s)` links whose text has words get a favicon before the text: 14x14,
  margin-inline 0.25em / 0.2em, `vertical-align: -0.125em`; GitHub hosts draw the GitHub mark in
  `currentColor`; other hosts load `https://www.google.com/s2/favicons?domain={host}&sz=32`
  (`shared:favicon.ts:86-97`, public hosts only); failed hosts (remembered for the session) and
  non-public hosts show `GlobeIcon`. The favicon plus the first character (or the whole `https://`
  prefix) are kept on one line; every following character is a break opportunity.
- Tooltip (top) = the href, unless the link has a pull-request preview (pull request spec).
- `#fragment` links scroll the matching id (sanitizer prefixes `user-content-` are ignored) into
  view (`block: nearest`) and push the hash (`:1876-1896`).
- Click (`:2965-3028`): media URL (by extension) → media viewer; change-request URL of a workspace
  project → pull request panel; otherwise setting `browserLinkTarget` (default `"system"`):
  system → open in the default browser; `"app"` → the in-app browser (desktop) unless a modifier
  key is held.
- Right-click on an external link → native menu (`chat/externalLinkContextMenu.ts:26-55`):
  ["Link to thread" or "Unlink from thread" when the URL is a linkable/linked PR], "Open in
  integrated browser" (only with a thread and the in-app browser), "Open in system browser",
  "Copy Link". Failures to link/unlink toast "Unable to link pull request" / "Unable to unlink pull
  request".
- PR / commit autolinks (`data-pull-request-autolink`): commit links in `font-mono`; reference
  autolinks (`#123`) confirm before opening (pull request spec).

### 6.2 File links (chips)

`ChatMarkdown.tsx:1207-1290,1948-2307,2633-2707`, `markdown-links.ts`, `crt:markdownLinks.ts`.

A markdown link destination or an inline code span that resolves to a file path renders as a
`ContextChip kind="mention"` (`chat-timeline.md` gives chip geometry) containing
`FileTagChipContent`: Pierre icon for the path + label.

- **Detection** (`crt:markdownLinks.ts:161-321`): links: `file:` URLs, Windows drive paths,
  `./ ../ ~/` prefixes, POSIX absolute paths with a known root, extension, `:line` suffix or a
  known extensionless file name, relative paths with an extension. Not: `#…`, `//…`, external
  schemes. Inline code needs stronger evidence: a path separator or `:line`, an explicit path
  prefix or an extension, not a hostname-looking first segment, not a version-like basename.
- **Label**: basename; plus ` · {parent suffix}` when two linked files in the same message share a
  basename (shortest unique parent path, at least 2 segments, `:1239-1289`); plus ` · L{line}` or
  ` · L{line}:C{col}`.
- **Tooltip** (top, `variant="code"`: mono 11px, max-w 480px, leading-relaxed) = full target path
  on one line, horizontally scrollable (6px thin scrollbar).
- **Click**: mod-click → open in the preferred editor (when the environment can run local shell
  actions); `.pdf` → in-app browser when available; else open in the right panel's file viewer at
  the line (bare file names first search the workspace index); media files → media viewer.
- **Right-click / chip without primary action** → native menu: "Preview media", "Open in
  {editor}", "Open in integrated browser", "{Reveal in Finder}" (platform label), "Copy relative
  path", "Copy full path". Copy success toast `{Relative path|Full path} copied` with the value as
  description; error toasts "Unable to open file", "Unable to open file in browser", "Unable to
  reveal file", `Failed to copy {relative path|full path}`.
- Copy-as-markdown text: `[basename](href)` for links, `` `code` `` for inline code.

### 6.3 Context and citation chips

- `t3-context:` links/images (user messages): rendered by the host's `renderContextReference`
  (user-message chips, `chat-timeline.md`); without it, the label as plain text.
- `t3-citation:` links: `AssistantCitationChip` (a citation of assistant text the user quoted;
  timeline spec).

### 6.4 Images and video

`ChatMarkdown.tsx:1356-1768,3153-3281`.

- Standalone image (sole content of its block): reserves a slot until decoded:
  `inline-block aspect-video w-full max-w-[min(100%,30rem)] overflow-hidden bg-muted/60
  rounded-lg border border/40` (16:9, max 480px wide), or the authored `width`/`height` box.
  Decoded: the bare image, `h-auto w-auto object-contain max-h-[30rem] max-w-[min(100%,30rem)]`
  (480px). Inline images (in a sentence) get no slot and appear at natural size.
- Workspace / attachment images load through signed asset URLs (`assets.createUrl` RPC) and keep
  the `rounded-lg border border/40` frame; when the server reports pixel dimensions the slot uses
  the exact aspect ratio.
- Click (not inside a link): opens the media viewer (`chat.md` 9.3) with all images of the message
  as a gallery; `role=button`, `aria-label` `Preview {alt}`, Enter/Space open, `cursor-zoom-in`.
- Failure: slot shows `TriangleAlertIcon` 14px + "Image unavailable" / "Video unavailable"
  (`· {alt}` appended when alt is set), centered, `text-xs text-muted-foreground`. Inline failure:
  chip `rounded-md border border/40 bg-muted/40 px-2 py-1 text-xs text-muted-foreground`.
- Loading placeholder for inline images: empty span `role=status aria-label="Loading image"`.
- Video: `MediaVideoPlayer` (media spec), same 480px bounds and frame.

### 6.5 Math

KaTeX HTML output. Display math (`$$`, `\[`) in `.chat-markdown-math-display`: block,
`max-width: 100%`, horizontal scroll, thin scrollbar like code blocks. Inline math inline.
Copying math copies its TeX source (`rehypeMathCopySource`).

## 7. Tables

`ChatMarkdown.tsx:741-867`, `index.css:2063-2110`, `ui/scroll-area.tsx`.

```
div.chat-markdown-table-container  my 10.4px  data-expanded
├─ ScrollArea (no radius, horizontal scroll, 1.5rem edge fade masks where content overflows,
│    overlay scrollbars 6px, appear on hover/scroll, delay 300ms fade-out)
│  └─ table  width 100%, min-width max-content, border-collapse, font-size 0.75rem (12px),
│            overflow-wrap normal, word-break normal
│       th/td  padding 0.45rem 0.75rem (7.2 / 12px), text-align left
│       thead th  padding-block 0.55rem (8.8px), weight 600, nowrap,
│                 border-bottom 1px contrast-border/60
│       tbody td  border-bottom 1px contrast-border/60
└─ footer  mt-0.5 (2) flex items-center justify-between select-none
   ├─ expand toggle  Button icon-xs; variant secondary when expanded, else ghost-muted
   │    Maximize2Icon / Minimize2Icon 12px; aria-pressed
   │    aria + tooltip (top): "Expand table cells" / "Collapse table cells"
   └─ copy menu trigger  Button icon-xs ghost-muted, CopyIcon → CheckIcon 1200ms
        aria + tooltip "Copy table" → "Copied"; menu (align end): "Copy as Markdown", "Copy as CSV"
```

- Expanded starts from the `wordWrap` setting (default **true**). Expanded: cells wrap
  (`overflow-wrap: anywhere`), `td` max-width 24rem (384px). Collapsed: every cell single-line,
  ellipsis, max-width 24rem. Expanding from collapsed first pins each header cell's
  `min-width` to the widest measured cell of its column so columns do not shrink.

## 8. Streaming

- `data-streaming` on the root while `isStreaming`. Without reduced motion, every **newly
  inserted** top-level block (and every new highlighted code container) fades in: opacity 0 → 1,
  600ms ease-out, once (`@starting-style`; `index.css:2035-2048`). Opening a finished thread never
  fades. A paragraph that grows keeps its element and does not re-fade.
- No cursor or caret is drawn at the end of streaming text.
- Code blocks: see 5.3 (incremental highlighting); run button hidden while streaming.

## 9. Copy behavior

- Selecting rendered markdown and copying writes `text/plain` = markdown re-serialized from the
  selection (links, emphasis, lists, fences, tables kept; elements with `data-markdown-copy` copy
  that text; a selection that is exactly one code block copies as plain code), and `text/html` =
  sanitized rendered fragment (`markdown-clipboard.ts:1-411`, `ChatMarkdown.tsx:2474-2488`).
- Table copy: `serializeTableElementToMarkdown` / `serializeTableElementToCsv`.
- Message copy buttons live in the timeline (`chat-timeline.md`).

## 10. Data

| Need | Source | t3UI |
|---|---|---|
| Provider skills (`$skill` chips) | `ServerProvider.skills` for the active instance, filtered by cwd (`resolveProviderSkillsForCwd`) | `t3_protocol::server::ProviderSkill` in `ServerProvider.skills` |
| Workspace image URLs | `assets.createUrl` (`assetEnvironment.createUrl`) → signed relative URL + optional `imageDimensions` | `t3_protocol::methods::AssetsCreateUrl` (`AssetResource::{MediaFile, GithubMedia, ..}`, result has `image_dimensions`); `Environment::fetch_asset` downloads |
| Workspace basename lookup | `projects.searchEntries {cwd, query, limit, kind:"file"}` | `t3_protocol::methods::ProjectsSearchEntries` |
| Open in editor / reveal | `shell.openInEditor {cwd, editor, reveal}` | `t3_protocol::methods::ShellOpenInEditor` (check `LaunchEditorInput` has `reveal`) |
| Reveal label | `ServerConfig.shellRevealInFileManager(Kind)`, `environment.platform.os` | decoded in `t3-protocol` server config |

## 11. Reuse map

| Module | Verdict |
|---|---|
| `t3-highlight` (engine, language, cache) | Reusable as-is. Theme colors unchanged between `@pierre/theme` 1.0.3 and 1.1.0; Shiki 4.2.0 in both. Re-run `tools/export-assets.mjs` against fe7d3092c only to confirm grammars; expect no diff |
| `t3-markdown/src/parse.rs`, `document.rs` | Needs changes: add math (`$`, `$$`, `\[`), GitHub alerts, skill tokens, `t3-context:` / `t3-citation:` links, Codex artifact-template directives, Claude insight rewrite, `title` attribute stripping, standalone-image marking. Images are now `inline-block` (document.rs comment says block) |
| `t3-markdown/src/streaming.rs` | The fork no longer chunks. Keep only as an internal cache if margins between chunks render exactly like one document; otherwise replace with "reparse after the last closed fence + blank line" (section 2.3) |
| `t3-markdown/src/links.rs` | Needs re-diff against `markdown-links.ts` + `crt:markdownLinks.ts` (rewritten since July: hostname / version heuristics, `#L` anchors no longer make a path, inline-code evidence rules). `parent_suffixes` matches `buildFileLinkParentSuffixByPath` |
| `t3-markdown/src/copy.rs` | Needs re-diff against `markdown-clipboard.ts` (311 → 411 lines: lone code block copies plain, whitespace hoisting, `data-markdown-copy`) |
| `t3-markdown/src/inline_text.rs`, `view.rs`, `render.rs` margin collapsing | Reusable mechanics (see `docs/handoff/markdown.md` gotchas) |
| `t3-markdown/src/render.rs` visuals, `style.rs` | July-era: replace with sections 3-7 (80% body color, 6px inline-code radius, code-block header with icon/title + wrap/run/copy, table footer with expand + copy menu, link dotted underline, file chips) |
| `t3-snapshots/src/scenes/markdown.rs`, `fixtures/sample.md` | Harness reusable; extend `sample.md` with alerts, math, details, file links, skills, tables with long cells |

## 12. Reference screenshots needed

| Name | How to reach | Data |
|---|---|---|
| `markdown-showcase-top` / `-bottom` | `thread-aurora-tour` (showcase scenario), scroll to the assistant answer | headings, lists, inline code, table, rust code block, blockquote, task list |
| `markdown-codeblock-hover` | hover a code block's copy button | tooltip "Copy code", header with language icon |
| `markdown-codeblock-wrap-off` | click "Disable line wrap" on a long-line block | horizontal scrollbar |
| `markdown-table-collapsed` / `-copy-menu` | click "Collapse table cells", then the copy button | ellipsized cells; menu "Copy as Markdown / Copy as CSV" |
| `markdown-file-chip-tooltip` | hover a file link chip | code tooltip with full path |
| `markdown-link-hover` | hover an external link | dotted underline + favicon + href tooltip |
| `markdown-alerts-math-details` | needs a fake-Codex scenario that emits GitHub alerts, `$…$`, `$$…$$`, `<details>` | add to `e2e/fake-codex/scenarios.cjs` |
| `markdown-streaming-fade` | `thread-aurora-watch` (running) mid-stream | capture a few frames |
| `markdown-user-message` | user message with line breaks and a raw `<b>` tag | literal HTML text, hard breaks |

## 13. Open questions / risks

- **KaTeX in GPUI.** No math layout engine exists in the crates. Options: render TeX source in a
  code-styled span (visual mismatch), or rasterize via an embedded KaTeX/MathJax-to-SVG step.
  Needs a decision before math scenarios are compared.
- **Favicons** require network fetches from Google's favicon service; decide on caching and
  privacy (the fork does it unconditionally for public hosts).
- **`text-box: trim`, `@starting-style` fades, dotted underline as background** have no direct
  GPUI equivalents; emulate (custom paint for the dotted underline, per-block opacity animation).
- **Native context menus** for links and file chips (same question as `chat.md` 15).
- **Run in terminal** needs the terminal surface (terminal spec) to accept a command.
- **Themed palettes** override code-block colors; out of scope until themes land.
