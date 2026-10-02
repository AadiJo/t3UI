# Files explorer and file tabs

> Refreshed against fe7d3092c (2026-10-02). Part of the panels spec; index in docs/spec/panels.md.

Two right-panel kinds share one component, `components/files/FilePreviewPanel.tsx` (mounted from
`ChatView.tsx:9763-9808`, keyed by environment + workspace root, or by attachment id):

- `files` (id `files`): the workspace explorer alone.
- `file` (id `file:<path>` or `attachment:<id>`): one file with breadcrumbs, a preview or an
  **editor that autosaves**, and (by default) the explorer docked to its right.

Only shown when the thread has a project and workspace root, or for attachments
(`ChatView.tsx:9763-9767`). Paths and conventions as in `panels.md` §0.

## 0. Primitives used

| Use | Primitive | Resolved (desktop) |
| --- | --- | --- |
| Header actions | `FileSurfaceAction`: `Button variant="ghost" size="icon-sm"` for commands, `Toggle variant="ghost" size="sm"` for on/off (`fileSurfaceChrome.tsx:72-114`) | 28x28, icon 14px, tooltip = label (top, 600ms) |
| Explorer refresh / expand-all | `Button variant="ghost" size="icon-xs"` | 24x24, icon 14px |
| Explorer search | `InputGroup variant="ghost"` `h-7 flex-1` + `InputGroupInput size="sm" type="search"` | design-system spec |
| Spinners | `Spinner size="lg"` (files), `Spinner` (menus) | continuous rotation; stop offscreen |
| Breadcrumb menus | `Menu` / `MenuPopup` / `MenuRadioGroup` | design-system spec |
| Open in editor | `OpenInPicker compact enableShortcut={false}` | panels-header-tools.md §4 |
| Comments | `DiffCommentAnnotation` | panels-diff.md §6 |

---

## 1. Layout

### 1.1 `files` tab (explorer only)

`div.flex.min-h-0.flex-1.flex-col.overflow-hidden.bg-background` with no file subheader; the explorer
aside is `min-w-0 flex-1` (fills the panel) (`FilePreviewPanel.tsx:1117-1339`, `filePreviewMode.ts:12-21`).

### 1.2 `file` tab

```
div.flex.min-h-0.flex-1.flex-col.overflow-hidden.bg-background
├─ subheader [data-surface-subheader]  FILE_SURFACE_SUBHEADER_CLASS: flex items-center gap-2 px-3
│   inline: h-7 (28px) + mb-3, no border; sheet: h-10 + border-b border-border/60
│   ├─ breadcrumbs: ScrollArea (no scrollbars, 24px edge fade) min-w-0 flex-1, row text-xs   1.3
│   ├─ OpenInPicker (compact) when the file has an absolute path and the environment is local or
│   │   has a remote-open route
│   ├─ rendered/source toggle (markdown, csv/tsv, html)                                     1.4
│   ├─ word wrap toggle (raw text bodies only)
│   ├─ "Open file in preview browser" (html, desktop preview support only)
│   └─ explorer toggle (workspace files only)
├─ notice: `Preview limited to the first 1 MB of a <n> byte file.` (truncated reads, not media/pdf)
│   shrink-0 border-b border-warning/20 bg-warning-surface px-3 py-1.5 text-2xs text-warning-foreground
└─ div.flex.min-h-0.flex-1.overflow-hidden
    ├─ preview column  min-w-0 flex-1 flex-col overflow-hidden (hidden when the path is a folder)   2
    └─ explorer aside (when open)  w-[min(22rem,46%)] min-w-64 border-l border-border/60            3
```

- The explorer starts open (`localStorage t3code.fileExplorerOpen`, default true) and never shows for
  attachments or absolute host paths. At the default 540px panel width it is 256px (`min-w-64`
  wins over 46%), leaving 283px for the file.
- A path that turns out to be a folder (`projects.readFile` fails with `path_not_file`) keeps the
  breadcrumbs, drops the preview column and lets the explorer fill the surface with that folder
  revealed (`:978-985`).
- When the path changes, the current breadcrumb scrolls into view (`inline: end`).

### 1.3 Breadcrumbs (`FileBreadcrumbs.tsx`)

Crumbs: project name, each folder, the file (`filePath.ts` `fileBreadcrumbs`). Between crumbs a
`ChevronRight` 14px `mx-1 text-muted-foreground/60`.

- File crumb: `max-w-40` (160px) truncate `rounded-sm px-0.5 font-medium text-foreground`,
  `aria-current="page"`; tooltip (top) = the path.
- Folder crumbs (and the project crumb): buttons `max-w-40 truncate rounded-sm px-0.5
  text-muted-foreground`, hover / menu open `bg-accent text-foreground`, focus ring 2px;
  `aria-label="Browse <name>"`; tooltip = folder path (or project name). For host paths outside the
  workspace they are plain labels.
- Clicking a folder crumb opens a menu (align start, below) listing that folder's entries
  (`projects.listEntries {cwd, directoryPath}`), refreshed on workspace mutations:
  - `Back to <parent>` (`ArrowLeft`) + separator, when inside a subfolder of the crumb (also Left
    arrow key).
  - Folders: Pierre icon + name (tooltip right = path) + `ChevronRight`; clicking navigates the menu
    into that folder without closing.
  - Files: radio items (the open file is checked), Pierre icon + name; ignored entries in
    `text-muted-foreground`; selecting opens the file and closes the menu.
  - States: `Loading folder…` (spinner, disabled), `Retry loading folder` (refresh icon) on error,
    `This folder is no longer available.`, `This folder is empty.`,
    `No entries from this folder are available in the partial workspace index.`, a trailing
    `Refresh failed — retry` after a failed refresh, and `Some workspace entries are not shown.` when
    truncated.

### 1.4 Header actions (`FilePreviewPanel.tsx:1151-1196`)

| Action | Shown | Label (aria + tooltip) | Icon (14px) |
| --- | --- | --- | --- |
| Rendered toggle (pressed = rendered) | `.md`/`.mdx`, `.csv`/`.tsv`, `.html` workspace files | markdown: `Show markdown source` / `Show rendered markdown`; table: `Show source` / `Show table`; html: `Show HTML source` / `Show rendered page` | `Code2` when rendered; else `Table2` (tables) or `Eye` |
| Word wrap (pressed = on) | raw text bodies (not rendered markdown/table/browser) | `Disable word wrap` / `Enable word wrap` | `WrapText` |
| Open in preview browser | html, desktop preview available | `Open file in preview browser` | `Globe2` |
| Explorer (pressed = open) | workspace files (not host paths, not folders) | `Hide file explorer` / `Show file explorer` | `FolderTree` |

Preferences: rendered markdown off by default (`t3code.renderMarkdown`), rendered HTML on
(`t3code.renderBrowserFile`), table on (`t3code.renderTable`); word wrap is the shared `wordWrap`
client setting (default true). A pending line reveal forces source until handled (the line only
exists in source).

---

## 2. Preview column (`FilePreviewPanel.tsx:1209-1313`)

Chosen in this order:

| Case | Rendering |
| --- | --- |
| attachment | `AttachmentFilePreview` (2.5) |
| video (`isWorkspaceVideoPreviewPath`) | centered media player `max-w-5xl` in `p-4`, from a signed asset URL (`assets.createUrl`, resource `media-file`) |
| audio | native audio controls `w-full max-w-xl` centered in `p-6`; failure `Unable to load audio.` + `Try again` |
| image | `img` `max-h-full max-w-full object-contain` centered in `p-4`, scrollable; failure `Unable to load workspace image.` (`text-xs text-destructive`) |
| pdf, or html rendered | sandboxed frame from the asset URL, `bg-white`, no border; PDFs with `#toolbar=0&view=FitH`; failure `Unable to load file preview.` |
| read error | the error message centered, `px-6 text-xs leading-relaxed text-destructive` (server messages such as `Failed to read workspace file '<p>' in '<cwd>'.`) |
| loading | `Spinner size="lg"` centered, `text-muted-foreground` |
| markdown rendered | `ChatMarkdown` in a ScrollArea, `mx-auto max-w-4xl px-6 py-5`; task-list checkboxes toggle `[ ]`/`[x]` in the file and autosave (not for host files) |
| csv/tsv rendered | `DelimitedTablePreview` (2.4) |
| truncated read or host file | read-only highlighted source (2.2) |
| otherwise | the editor (2.1) |

Images, video, audio, pdf and html read the file only to learn whether it is a folder; their bodies
come from asset URLs with `?workspace-revision=<mutationId>` appended so they reload after workspace
mutations.

### 2.1 Editor (`EditableFileSurface`, `:566-859`)

- `@pierre/diffs` `Editor` (contentEditable over Pierre's `File` renderer) inside a `Virtualizer`
  (`file-preview-virtualizer min-h-0 flex-1 overflow-auto`, overscroll 600px, intersection margin
  1200px). Options: no file header, `overflow` wrap/scroll by the wrap setting, theme
  `pierre-light/dark`, Shiki wasm, surface CSS `FILE_LINK_REVEAL_UNSAFE_CSS` (the diff surface tokens
  with `--code-background` as the background, `fileSurfaceChrome.tsx:21-65`).
- Typography/colors: same as the diff body (panels-diff.md §5.3): code font at the code size (13px),
  20px lines, line numbers in the gutter, `--code-background` (light `#FFFFFF`, dark `#222222`;
  `index.css:1091,1151`).
- Autosave: every edit updates the cached file contents optimistically and schedules a save 500ms
  after the last change (`FILE_SAVE_DEBOUNCE_MS`, `useFileSaveCoordinator.ts`); saves are serialized
  (one in flight, the latest contents win), confirmed contents update the cache, a pending save is
  flushed when the tab unmounts. RPC `projects.writeFile {cwd, relativePath, contents}` ->
  `{relativePath}`. While unsaved, the tab shows the pending dot (panels.md §6.2) and workspace-mutation
  refreshes of this file are paused. No explicit save shortcut, no dirty dialog.
- External changes: when the file refreshes with new contents and the editor holds no unsaved edits,
  the editor takes the new contents (`fileEditorSourceState.ts`).
- Line comments: select lines in the gutter (drag) or click the gutter utility; on selection end a
  draft comment opens under the last line (`DiffCommentAnnotation`, label `L<a>` or `L<a> to L<b>`).
  Saving adds a review comment to the composer draft (`buildFileReviewComment`: fenced with the
  file's language, `reviewCommentContext.ts:59-82`) and keeps it as a saved comment row; comments
  move with their lines as the file is edited. Clicking outside dismisses a selection unless a form is
  open (`fileEditorDismissal.ts`).
- Line reveal (`revealLine` from `openFile(path, line)`): clamp to the file's line count, center the
  line in the viewport (retries up to 30 frames while metrics load, then holds the position for 20
  frames unless the user scrolls, clicks or types), and tint the line: row `lab mix
  (line bg 82% light / 75% dark) + selection base (= --diffs-modified-base)`, number cell 75% / 60% with
  `--diffs-selection-number-fg` (`FilePreviewPanel.tsx:331-546`, `fileSurfaceChrome.tsx:32-64`).
  Each new `revealRequestId` reveals again.

### 2.2 Read-only source (`ReadOnlySourcePreview.tsx`)

Same Pierre `File` renderer and virtualizer without the editor (attachments, host files, truncated
reads). Supports the same line reveal.

### 2.3 Markdown

`FileMarkdownPreview.tsx`: `ChatMarkdown` (chat spec) with `imageBaseDir` = the file's folder.

### 2.4 Table (`DelimitedTablePreview.tsx`)

- Parsed with `shared:delimitedPreview` (first 100 rows x 30 columns). Over the limit: notice
  (`FileSurfaceNotice`: `border-b border-warning/20 bg-warning-surface px-3 py-1.5 text-2xs
  text-warning-foreground`) `Table limited to the first 100 rows and 30 columns. Switch to source for
  the rest.`
- `table` `min-w-full border-separate border-spacing-0 text-xs`; sticky header cells `max-w-80
  border-b border-border bg-muted/60 px-3 py-1.5 text-left align-bottom font-medium whitespace-pre-wrap
  break-words backdrop-blur`; body cells `max-w-80 border-b border-border/60 px-3 py-1.5 align-top
  whitespace-pre-wrap break-words tabular-nums`; even rows `bg-muted/30`.

### 2.5 Attachment tabs (`AttachmentFilePreview.tsx`)

- Own subheader (same class): `Attachment` (or origin) `ChevronRight` name (`font-medium`, truncate)
  and the size (`ml-2 text-muted-foreground`); actions: rendered toggle, word wrap, `Copy contents`
  / `Copy preview` (truncated) / `Copied` (`Copy` / `Check`), `Save file` / `Preparing file…`
  (`Download`, disabled while preparing), optional `Remove from draft` (`Trash2`), `Close` (`X`).
- Body by kind: text/markdown/table/html/pdf/audio/video/image like above (video on `bg-black`);
  unknown: `No preview for this file` (`text-sm font-medium`) + `Save it to open in an app that
  supports <ext> files.` (`max-w-sm text-xs text-muted-foreground`). Errors: `Reconnect to the
  environment and try again.`, `The attachment is unavailable.`, `Could not load this file.`; save
  failure toast `Could not save file`.
- Truncated: notice `Preview limited to the first 1 MB of a <n> byte file. Save the file to read it in
  full.`

---

## 3. Explorer (`FileBrowserPanel.tsx`)

### 3.1 Box

```
div.flex.min-h-0.flex-1.flex-col.bg-background
├─ toolbar [data-surface-subheader]  flex items-center gap-1 px-2
│   inline: h-9 (36px) + mb-1 (4px), no border; sheet: h-10 + border-b border-border/60
│   ├─ refresh (icon-xs ghost, RefreshIcon spinning while loading); aria `Refresh workspace files`,
│   │   tooltip `Refresh files` / `Refreshing…`
│   ├─ search field (flex-1, h-7): placeholder `Search files`, aria `Search <project> files`;
│   │   Escape clears and blurs
│   └─ expand/collapse all folders (icon-xs ghost, only when folders exist): `Expand all folders`
│       (`ChevronsUpDown`) / `Collapse all folders` (`ChevronsDownUp`)
├─ error row (button): `<message> Click to retry.` p-4 text-left text-xs leading-relaxed text-destructive
├─ `More matches available. Refine your search.` px-3 py-1 text-xs text-muted-foreground
├─ `Loading files…` px-3 py-1 text-xs text-muted-foreground (role=status) while any load is pending
└─ FileTree (@pierre/trees) min-h-0 flex-1 overflow-hidden, aria-label `<project> files`
```

### 3.2 Tree (`@pierre/trees` 1.0.0-beta.4)

Options (`FileBrowserPanel.tsx:249-288`): `density: "compact"`, `flattenEmptyDirectories: true`
(single-child folder chains render as `a/b/c` on one row, each segment underlines on hover),
`initialExpansion: "closed"`, search mode `hide-non-matches`, colored Pierre icons with the app's
overrides (`pierre-icons.ts`: `package.json` npm, `tsconfig.json` TypeScript, `agents.md` agents glyph,
pnpm files, video extensions -> film icon), right-click context menu, rows draggable (no drop).

Row geometry (compact factor 0.8; `nm:@pierre/trees@1.0.0-beta.4 dist/style.js`, `model/density.js`):

| Metric | Value |
| --- | --- |
| row height | 24px |
| scroll container inline padding | 16px each side |
| row margin-x / padding-x | 1.6px / 6.4px |
| gap between icon and label | 4.8px |
| indent per level | 6.4px plus a 16px icon column; 1px indent guides (fg-muted 25%) visible only while the tree is hovered (opacity 0.75, 150ms) |
| icon column | 16px; folders show a chevron (down when open, rotated -90deg when closed) |
| font | `--font-sans` 12px (override), weight 400; search matches 600 |
| radius | 5px (override) |

Colors (`pierre-tree-theme.ts`): background transparent; text `--contrast-foreground`; hover
`currentColor 7%`; selected `currentColor 12%`; borders `currentColor 14%`; focus ring 1px inset;
icons `fg-muted` unless colored by type (`PierreEntryIcon` table of light/dark hexes, e.g. typescript
`#1a85d4`/`#69b1ff`, markdown `#199f43`/`#5ecc71`, json `#d47628`/`#ffa359`). Ignored entries (from
`ProjectEntry.ignored`) get git status `ignored`: text and icon `#adadb1` light / `#4a4a4e` dark, icon at
50% opacity. Diff color scheme `blue-orange` re-maps added/deleted colors.

### 3.3 Loading and data

- Directories load lazily: the root on mount, then each folder when it is expanded (or on the path to
  a revealed file), with `projects.listEntries {cwd, directoryPath}` (immediate children, includes
  ignored entries; `directoryPath ""` = root), at most 4 requests at a time; results are cached while
  collapsed (`useDirectoryEntries.ts`). Per-folder errors show in the error row (`Unable to load
  folder.` fallback).
- Search: typing sets the tree's search and queries `projects.searchEntries {cwd, query (<=256),
  limit: 200}` (120ms debounce); matching paths and their ancestor folders are merged into the tree;
  non-matches hide. Clearing the query closes the search.
- Refresh button: reloads loaded folders, re-runs the search, and re-reads the open file. Workspace
  mutations (panels-diff.md §2) refresh folders and search automatically.
- Expand all: expands every loaded folder (and keeps expanding newly loaded ones until a folder is
  collapsed by hand).

### 3.4 Interaction

- Clicking a file row opens it (`openFile(path)`: reuses or appends `file:<path>`, removes the
  standalone `files` tab). Clicking a folder toggles it.
- External opens (file picker, chat links, diff titles) reveal the path: close the search, deselect,
  expand ancestors, select the row, scroll it to the center and focus it. Opens from inside the tree
  are not re-revealed (search stays open).
- Drag a row (or the selection) onto the composer: the drag carries the composer mention payload
  (`[name](path)` markdown links joined by spaces, custom drag type `COMPOSER_MENTION_DRAG_TYPE`;
  `fileTreeDragMention.ts`); selection changes caused by starting a drag don't open files.
- Right-click (in-app menu at the pointer, `FileBrowserPanel.tsx:163-235`), items in order:
  1. `Open` (pencil icon) when the server lists the `file-manager` editor.
  2. `Reveal in Finder` / `Reveal in File Explorer` / `Reveal in Files` (folder icon), per the
     server's `shellRevealInFileManagerKind` or OS, only when `shellRevealInFileManager` is enabled.
  3. `Open with` submenu: one item per available editor (labels from `EDITORS`).
  4. `Copy mention`: copies `[name](path)`; toast `Mention copied` (path) or `Failed to copy mention`.
  5. `Add to chat`: appends `[name](path) ` to the composer; toast `Unable to add to chat` with
     `Open a chat for this project and try again.` or `The chat isn't ready to accept input right
     now.`
  Open/reveal/open-with call `shell.openInEditor {cwd: <absolute path>, editor, reveal?}`; failures
  toast `Could not open file` / `Unable to reveal file` / `Could not open in <Editor>` with the path
  (`fileContextMenu.ts`). The same file menu (without the mention items) is used on diff file titles.

---

## 4. Opening files (entry points)

| Entry | Effect |
| --- | --- |
| Tab bar "+" / launcher `Files` | `open("files")` |
| `⌘P` (`filePicker.toggle`, outside terminals) | command palette in "files" mode (`ProjectFilePicker.tsx`): placeholder `Search files…`, fuzzy results grouped under the project name (Pierre icon, name with matched chars bold, path below), footer `Open file`, Escape `Back`; empty states `Searching workspace files…`, `Indexing workspace files…`, `No matching files.`, `No files found.`, `Open a project to search its files.`; data `projects.searchEntries` (empty query = frecency list). Choosing a file calls `openFile(path)`. The palette chrome belongs to the command palette spec. |
| Chat file links / file chips | `openFile(path, line?)` (line reveal) |
| Diff file title | `openFile(workspace-relative path)` (panels-diff.md §5.4) |
| Timeline / composer attachments | `openAttachment(attachment)` |
| `.` link (workspace root) | `open("files")` |

Without a workspace, `reconcileFileSurfaces(false)` drops `files` and non-attachment `file` tabs
(panels.md §1.3).

---

## Reuse map

| T3UI module | Verdict |
| --- | --- |
| `crates/t3-app/src/panels/files/tree.rs` (branch `diff`) | Model reusable (dirs-first natural sort, empty-dir flattening, search with expansion restore). Change: lazy per-folder loading via `listEntries {directoryPath}` (4 concurrent), closed by default, ignored-entry status, server-side search merged with ancestors, reveal semantics of 3.4. |
| `panels/files/browser.rs`, `files/style.rs` (branch `diff`) | July-era visuals, never wired: replace with 3.1-3.2 metrics. |
| `crates/t3-diff` rows + `t3-highlight` | Basis for the read-only source view and the editor's rendering (same Pierre surface). |
| Editor | Nothing in T3UI. Needs a code editor element (gpui-kit/gpui-component editor or a custom one on `t3-diff` rows) with Pierre's look, gutter selection, comment annotations and 500ms debounced autosave. Largest piece of this area. |
| `t3-markdown` | Rendered markdown view (task-list toggles need a callback with the marker offset). |
| `crates/t3-protocol/src/projects.rs` | `listEntries` (with `directory_path`), `readFile`, `writeFile`, `searchEntries`, `assets.createUrl` typed. |

## Reference screenshots needed

Seed: `thread-aurora-tour` (aurora-web: `README.md`, `package.json`, `src/index.ts`, `src/format.ts`,
`test/format.test.ts`). Dark and light.

1. `files-explorer`: "+" -> Files (explorer only, folders closed).
2. `files-explorer-expanded`: expand `src` and `test`, hover a row (indent guides, hover fill).
3. `files-explorer-search`: type `form` in the search (matches only, bold).
4. `files-file-ts`: click `src/format.ts` (breadcrumbs, editor with highlighting, explorer docked right
   with the row selected).
5. `files-file-markdown-source` and `files-file-markdown-rendered`: open `README.md`, toggle rendered.
6. `files-breadcrumb-menu`: click the `src` crumb (menu with files, current file checked).
7. `files-context-menu`: right-click a row (Open / Reveal / Open with / Copy mention / Add to chat).
8. `files-line-reveal`: open `src/format.ts` at line 3 from a chat file link (tinted, centered line).
9. `files-comment-draft`: drag-select lines 2-3 in the gutter (draft comment form).
10. `files-pending-dot`: type in the editor and capture within 500ms (tab pending dot).
11. `files-explorer-hidden`: toggle the explorer off (file fills the panel).
12. `files-csv-table`: needs a `.csv` in the repo (add one to the fixture repos), rendered table.
13. `files-image`: needs an image file in the workspace (fixture addition).
14. `files-attachment`: needs a thread with a file attachment (fixture addition).

## Missing client APIs

- None for core files: `projects.listEntries` (with `directoryPath`), `projects.readFile`,
  `projects.writeFile`, `projects.searchEntries`, `shell.openInEditor`, `assets.createUrl` are typed.
- Server config fields used by the context menu (`availableEditors` incl. `file-manager`,
  `shellRevealInFileManager`, `shellRevealInFileManagerKind`, `environment.platform.os`) are present
  in `crates/t3-protocol/src/server.rs:63-71` and `environment.rs:40-57`.
- Client settings `wordWrap`, code font size, `diffColorScheme`.

## Open questions / risks

- The editor is a real editor (contentEditable, autosave) in the fork. A GPUI port needs either an
  existing editor component that can match Pierre's rendering or a custom one; scope it before
  promising parity. A read-only first version would change behavior (no autosave, no edits).
- HTML/PDF previews need a web view (same constraint as the browser preview); without one, show the
  source by default (set rendered-html off) and hide `Open file in preview browser`.
- Video/audio playback needs a native player; the fork uses `MediaVideoPlayer` and `<audio controls>`.
- `@pierre/trees` virtualization, sticky folder rows and flattened segment interactions are library
  behavior; capture references while scrolling a deep tree.
