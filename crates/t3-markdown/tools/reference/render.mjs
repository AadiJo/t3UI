// Static pixel reference for the T3 Code fork's ChatMarkdown.
//
// Renders crates/t3-markdown/fixtures/sample.md through the same pipeline as
// apps/web/src/components/ChatMarkdown.tsx (react-markdown + remark-gfm +
// remarkPreserveCodeMeta + rehype-raw + rehype-sanitize with the fork schema,
// the fork's custom components, @pierre/diffs' shared Shiki highlighter) and
// writes one HTML page per theme/wordWrap combination into this folder.
//
// Usage: T3_FORK=<fork checkout> node crates/t3-markdown/tools/reference/render.mjs
// Output: dark.html, light.html (fork default wordWrap=true),
//         dark-nowrap.html, light-nowrap.html (wordWrap=false), index.html.
//
// Everything is read from the fork's node_modules / sources; nothing is written
// outside /tmp/markdown-reference.
import fs from "node:fs";
import path from "node:path";
import { createRequire, register } from "node:module";
import { execFileSync } from "node:child_process";

const OUT = process.env.T3_MD_REF_OUT ?? "/tmp/markdown-reference";
// The fork checkout to render with (its apps/web sources and node_modules). This was written
// against the July checkout; ChatMarkdown.tsx has changed a lot since, so expect to adapt it.
const FORK = process.env.T3_FORK ?? "/home/aadi/L-Projects/t3UI-refs/t3code-fork";
const WEB_SRC = `${FORK}/apps/web/src/`;
const P = `${FORK}/node_modules/.pnpm`;
const SAMPLE = "/home/aadi/L-Projects/t3UI-worktrees/markdown/crates/t3-markdown/fixtures/sample.md";
const COMPILE = "/home/aadi/L-Projects/t3UI-worktrees/markdown/docs/spec/tools/compile.mjs";
const CWD = "/Users/aadi/t3code";

// Let Node import the fork's plain .ts helpers (markdown-links, terminal-links,
// filePathDisplay, pierre-icons, composerInlineChip) straight from source.
// Extensionless relative imports resolve to .ts; terminal-links' only import
// (./lib/utils, which drags in the whole app) is stubbed with the one function
// it uses.
const hooks = `
import fs from "node:fs";
import { fileURLToPath } from "node:url";
const WEB_SRC = ${JSON.stringify(WEB_SRC)};
export async function resolve(specifier, context, next) {
  const parent = context.parentURL?.startsWith("file:") ? fileURLToPath(context.parentURL) : "";
  if (parent.startsWith(WEB_SRC) && specifier.startsWith(".")) {
    if (specifier === "./lib/utils" && parent.endsWith("terminal-links.ts")) {
      return { url: "data:text/javascript,export function isMacPlatform(p){return /mac|iphone|ipad|ipod/i.test(p)}", shortCircuit: true };
    }
    const target = new URL(specifier, context.parentURL);
    if (!/\\.[a-z]+$/i.test(specifier) && fs.existsSync(fileURLToPath(target) + ".ts")) {
      return { url: target.href + ".ts", shortCircuit: true };
    }
  }
  return next(specifier, context);
}`;
register(`data:text/javascript,${encodeURIComponent(hooks)}`);

const require = createRequire(import.meta.url);
const React = require(`${P}/react@19.2.6/node_modules/react/index.js`);
const { renderToStaticMarkup } = require(
  `${P}/react-dom@19.2.6_react@19.2.6/node_modules/react-dom/server.node.js`,
);
const lucide = require(`${P}/lucide-react@0.564.0_react@19.2.6/node_modules/lucide-react/dist/cjs/lucide-react.js`);
const { default: ReactMarkdown, defaultUrlTransform } = await import(
  `${P}/react-markdown@10.1.0_@types+react@19.2.16_react@19.2.6/node_modules/react-markdown/index.js`
);
const { default: remarkGfm } = await import(`${P}/remark-gfm@4.0.1/node_modules/remark-gfm/index.js`);
const { default: rehypeRaw } = await import(`${P}/rehype-raw@7.0.0/node_modules/rehype-raw/index.js`);
const { default: rehypeSanitize, defaultSchema } = await import(
  `${P}/rehype-sanitize@6.0.0/node_modules/rehype-sanitize/index.js`
);
const BASE_UI = `${P}/@base-ui+react@1.5.0_@types+react@19.2.16_react-dom@19.2.6_react@19.2.6__react@19.2.6/node_modules/@base-ui/react/esm`;
const { Collapsible: CollapsiblePrimitive } = await import(`${BASE_UI}/collapsible/index.js`);
const { ScrollArea: ScrollAreaPrimitive } = await import(`${BASE_UI}/scroll-area/index.js`);
const { useRender } = await import(`${BASE_UI}/use-render/index.js`);
const { mergeProps } = await import(`${BASE_UI}/merge-props/index.js`);
const { cva, cx } = await import(`${P}/class-variance-authority@0.7.1/node_modules/class-variance-authority/dist/index.mjs`);
const { twMerge } = await import(`${P}/tailwind-merge@3.6.0/node_modules/tailwind-merge/dist/bundle-mjs.mjs`);
const DIFFS = fs.realpathSync(`${FORK}/apps/web/node_modules/@pierre/diffs`);
const { getSharedHighlighter } = await import(`${DIFFS}/dist/highlighter/shared_highlighter.js`);
const TREES = fs.realpathSync(`${FORK}/apps/web/node_modules/@pierre/trees`);
const { getBuiltInSpriteSheet } = await import(`${TREES}/dist/index.js`);

// Fork sources, imported as-is.
const markdownLinks = await import(`${WEB_SRC}markdown-links.ts`);
const pierreIcons = await import(`${WEB_SRC}pierre-icons.ts`);
const chip = await import(`${WEB_SRC}components/composerInlineChip.ts`);

const h = React.createElement;
const { Children, Fragment, isValidElement } = React;

// lib/utils.ts
function cn(...inputs) {
  return twMerge(cx(inputs));
}

// ---------------------------------------------------------------------------
// components/ui/button.tsx (verbatim class strings)
const buttonVariants = cva(
  "[&_svg]:-mx-0.5 relative inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-lg border font-medium text-base outline-none transition-shadow before:pointer-events-none before:absolute before:inset-0 before:rounded-[calc(var(--radius-lg)-1px)] pointer-coarse:after:absolute pointer-coarse:after:size-full pointer-coarse:after:min-h-11 pointer-coarse:after:min-w-11 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-64 sm:text-sm [&_svg:not([class*='opacity-'])]:opacity-80 [&_svg:not([class*='size-'])]:size-4.5 sm:[&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0",
  {
    defaultVariants: { size: "default", variant: "default" },
    variants: {
      size: {
        default: "h-9 px-[calc(--spacing(3)-1px)] sm:h-8",
        icon: "size-9 sm:size-8",
        "icon-lg": "size-10 sm:size-9",
        "icon-sm": "size-8 sm:size-7",
        "icon-xl":
          "size-11 sm:size-10 [&_svg:not([class*='size-'])]:size-5 sm:[&_svg:not([class*='size-'])]:size-4.5",
        "icon-xs":
          "size-7 rounded-md before:rounded-[calc(var(--radius-md)-1px)] sm:size-6 not-in-data-[slot=input-group]:[&_svg:not([class*='size-'])]:size-4 sm:not-in-data-[slot=input-group]:[&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-10 px-[calc(--spacing(3.5)-1px)] sm:h-9",
        sm: "h-8 gap-1.5 px-[calc(--spacing(2.5)-1px)] sm:h-7",
        xl: "h-11 px-[calc(--spacing(4)-1px)] text-lg sm:h-10 sm:text-base [&_svg:not([class*='size-'])]:size-5 sm:[&_svg:not([class*='size-'])]:size-4.5",
        xs: "h-7 gap-1 rounded-md px-[calc(--spacing(2)-1px)] text-sm before:rounded-[calc(var(--radius-md)-1px)] sm:h-6 sm:text-xs [&_svg:not([class*='size-'])]:size-4 sm:[&_svg:not([class*='size-'])]:size-3.5",
      },
      variant: {
        default:
          "not-disabled:inset-shadow-[0_1px_--theme(--color-white/16%)] border-primary bg-primary text-primary-foreground shadow-primary/24 shadow-xs [:active,[data-pressed]]:inset-shadow-[0_1px_--theme(--color-black/8%)] [:disabled,:active,[data-pressed]]:shadow-none [:hover,[data-pressed]]:bg-primary/90",
        destructive:
          "not-disabled:inset-shadow-[0_1px_--theme(--color-white/16%)] border-destructive bg-destructive text-white shadow-destructive/24 shadow-xs [:active,[data-pressed]]:inset-shadow-[0_1px_--theme(--color-black/8%)] [:disabled,:active,[data-pressed]]:shadow-none [:hover,[data-pressed]]:bg-destructive/90",
        "destructive-outline":
          "border-input bg-popover not-dark:bg-clip-padding text-destructive-foreground shadow-xs/5 not-disabled:not-active:not-data-pressed:before:shadow-[0_1px_--theme(--color-black/4%)] dark:bg-input/32 dark:not-disabled:before:shadow-[0_-1px_--theme(--color-white/2%)] dark:not-disabled:not-active:not-data-pressed:before:shadow-[0_-1px_--theme(--color-white/6%)] [:disabled,:active,[data-pressed]]:shadow-none [:hover,[data-pressed]]:border-destructive/32 [:hover,[data-pressed]]:bg-destructive/4",
        ghost:
          "border-transparent text-foreground data-pressed:bg-accent [:hover,[data-pressed]]:bg-accent [&_svg:not([class*='text-'])]:text-muted-foreground",
        link: "border-transparent underline-offset-4 [:hover,[data-pressed]]:underline",
        outline:
          "border-input bg-popover not-dark:bg-clip-padding text-foreground shadow-xs/5 not-disabled:not-active:not-data-pressed:before:shadow-[0_1px_--theme(--color-black/4%)] dark:bg-input/32 dark:not-disabled:before:shadow-[0_-1px_--theme(--color-white/2%)] dark:not-disabled:not-active:not-data-pressed:before:shadow-[0_-1px_--theme(--color-white/6%)] [:disabled,:active,[data-pressed]]:shadow-none [:hover,[data-pressed]]:bg-accent/50 dark:[:hover,[data-pressed]]:bg-input/64 [&_svg:not([class*='text-'])]:text-muted-foreground",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground [:active,[data-pressed]]:bg-secondary/80 [:hover,[data-pressed]]:bg-secondary/90",
      },
    },
  },
);

function Button({ className, variant, size, render, ...props }) {
  const typeValue = render ? undefined : "button";
  const defaultProps = {
    className: cn(buttonVariants({ className, size, variant })),
    "data-slot": "button",
    type: typeValue,
  };
  return useRender({ defaultTagName: "button", props: mergeProps(defaultProps, props), render });
}

// components/ui/scroll-area.tsx
function ScrollArea({
  className,
  children,
  scrollFade = false,
  scrollbarGutter = false,
  hideScrollbars = false,
  chainVerticalScroll = false,
  ...props
}) {
  return h(
    ScrollAreaPrimitive.Root,
    { className: cn("relative size-full min-h-0 overflow-hidden rounded-[inherit]", className), ...props },
    h(
      ScrollAreaPrimitive.Viewport,
      {
        className: cn(
          "h-full max-h-[inherit] overflow-auto overscroll-contain rounded-[inherit] outline-none transition-shadows focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background data-has-overflow-x:overscroll-x-contain",
          chainVerticalScroll && "overscroll-y-auto",
          scrollFade &&
            "mask-t-from-[calc(100%-min(var(--fade-size),var(--scroll-area-overflow-y-start)))] mask-b-from-[calc(100%-min(var(--fade-size),var(--scroll-area-overflow-y-end)))] mask-l-from-[calc(100%-min(var(--fade-size),var(--scroll-area-overflow-x-start)))] mask-r-from-[calc(100%-min(var(--fade-size),var(--scroll-area-overflow-x-end)))] [--fade-size:1.5rem]",
          scrollbarGutter && "scrollbar-gutter-stable",
          hideScrollbars && "[-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden",
        ),
        "data-slot": "scroll-area-viewport",
      },
      children,
    ),
    // hideScrollbars is always true for markdown tables, so no ScrollBar/Corner.
  );
}

// components/ui/collapsible.tsx
function Collapsible(props) {
  return h(CollapsiblePrimitive.Root, { "data-slot": "collapsible", ...props });
}
function CollapsibleTrigger({ className, ...props }) {
  return h(CollapsiblePrimitive.Trigger, {
    className: cn("cursor-pointer", className),
    "data-slot": "collapsible-trigger",
    ...props,
  });
}
function CollapsiblePanel({ className, ...props }) {
  return h(CollapsiblePrimitive.Panel, {
    className: cn(
      "h-(--collapsible-panel-height) overflow-hidden transition-[height] [transition-duration:var(--workspace-panel-transition-duration)] [transition-timing-function:var(--workspace-panel-transition-easing)] motion-reduce:transition-none data-ending-style:h-0 data-starting-style:h-0 data-open:data-ending-style:[height:var(--collapsible-panel-height)]",
      className,
    ),
    "data-slot": "collapsible-panel",
    ...props,
  });
}

// ---------------------------------------------------------------------------
// components/chat/PierreEntryIcon.tsx
const ICON_COLORS = {
  astro: ["#a631be", "#d568ea"], babel: ["#d5a910", "#ffd452"], bash: ["#199f43", "#5ecc71"],
  biome: ["#1a85d4", "#69b1ff"], bootstrap: ["#693acf", "#9d6afb"], browserslist: ["#d5a910", "#ffd452"],
  bun: ["#594c5b", "#79697b"], c: ["#1a85d4", "#69b1ff"], claude: ["#d47628", "#ffa359"],
  cpp: ["#1a85d4", "#69b1ff"], css: ["#693acf", "#9d6afb"], database: ["#a631be", "#d568ea"],
  default: ["#84848a", "#adadb1"], docker: ["#1a85d4", "#69b1ff"], eslint: ["#693acf", "#9d6afb"],
  git: ["#ff8c5b", "#d5512f"], go: ["#1ca1c7", "#68cdf2"], graphql: ["#d32a61", "#ff678d"],
  html: ["#d47628", "#ffa359"], image: ["#d32a61", "#ff678d"], javascript: ["#d5a910", "#ffd452"],
  json: ["#d47628", "#ffa359"], markdown: ["#199f43", "#5ecc71"], mcp: ["#17a5af", "#64d1db"],
  nextjs: ["#84848a", "#adadb1"], npm: ["#d52c36", "#ff6762"], oxc: ["#1ca1c7", "#68cdf2"],
  postcss: ["#d52c36", "#ff6762"], prettier: ["#17a5af", "#64d1db"], python: ["#1a85d4", "#69b1ff"],
  react: ["#1ca1c7", "#68cdf2"], ruby: ["#d52c36", "#ff6762"], rust: ["#d47628", "#ffa359"],
  sass: ["#d32a61", "#ff678d"], stylelint: ["#84848a", "#adadb1"], svelte: ["#d52c36", "#ff6762"],
  svg: ["#d47628", "#ffa359"], svgo: ["#199f43", "#5ecc71"], swift: ["#d47628", "#ffa359"],
  table: ["#17a5af", "#64d1db"], tailwind: ["#1ca1c7", "#68cdf2"], terraform: ["#693acf", "#9d6afb"],
  text: ["#84848a", "#adadb1"], typescript: ["#1a85d4", "#69b1ff"], vite: ["#a631be", "#d568ea"],
  vscode: ["#1a85d4", "#69b1ff"], vue: ["#199f43", "#5ecc71"], wasm: ["#693acf", "#9d6afb"],
  webpack: ["#1a85d4", "#69b1ff"], yml: ["#d52c36", "#ff6762"], zig: ["#d47628", "#ffa359"],
  zip: ["#d47628", "#ffa359"],
};

function PierreEntryIcon({ pathValue, kind, theme, className }) {
  const icon = pierreIcons.resolvePierreIconForEntry(pathValue, kind);
  if (!icon) {
    const Icon = kind === "directory" ? lucide.FolderIcon : lucide.FileIcon;
    return h(Icon, { className: cn("size-4 text-muted-foreground/80", className) });
  }
  const colors = ICON_COLORS[icon.token ?? "default"] ?? ICON_COLORS.default;
  return h(
    "svg",
    {
      "aria-hidden": "true",
      "data-pierre-icon": icon.name,
      "data-icon-token": icon.token,
      className: cn("size-4 shrink-0", className),
      style: { color: colors?.[theme === "light" ? 0 : 1] },
      viewBox: "0 0 16 16",
    },
    h("use", { href: `#${icon.name}` }),
  );
}

// components/chat/FileTagChip.tsx
function FileTagChipContent({ path: iconPath, label, theme, selectable }) {
  return h(
    Fragment,
    null,
    h(PierreEntryIcon, {
      pathValue: iconPath,
      kind: pierreIcons.inferEntryKindFromPath(iconPath),
      theme,
      className: chip.COMPOSER_INLINE_CHIP_ICON_CLASS_NAME,
    }),
    h(
      "span",
      { className: selectable ? chip.CHAT_INLINE_CHIP_LABEL_CLASS_NAME : chip.COMPOSER_INLINE_CHIP_LABEL_CLASS_NAME },
      label,
    ),
  );
}

// ---------------------------------------------------------------------------
// ChatMarkdown.tsx ports (line numbers refer to the fork file)

// :156-167
const CHAT_MARKDOWN_SANITIZE_SCHEMA = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    "*": (defaultSchema.attributes?.["*"] ?? []).filter((attribute) => attribute !== "title"),
    code: [...(defaultSchema.attributes?.code ?? []), "dataCodeMeta"],
  },
  protocols: {
    ...defaultSchema.protocols,
    href: [...(defaultSchema.protocols?.href ?? []), "file"],
  },
};

const CODE_FENCE_LANGUAGE_REGEX = /(?:^|\s)language-([^\s]+)/;
function extractFenceLanguage(className) {
  const match = className?.match(CODE_FENCE_LANGUAGE_REGEX);
  const raw = match?.[1] ?? "text";
  return raw === "gitignore" ? "ini" : raw;
}

const FENCE_TITLE_ATTR_REGEX = /(?:^|\s)(?:title|file(?:name)?)=(?:"([^"]+)"|'([^']+)'|(\S+))/i;
const FENCE_FILENAME_TOKEN_REGEX = /^[\w@][\w@./-]*\.[A-Za-z0-9]+$/;
function extractFenceTitle(meta) {
  if (!meta) return null;
  const attrMatch = FENCE_TITLE_ATTR_REGEX.exec(meta);
  const attrTitle = attrMatch?.[1] ?? attrMatch?.[2] ?? attrMatch?.[3];
  if (attrTitle) return attrTitle;
  return meta.split(/\s+/).find((candidate) => FENCE_FILENAME_TOKEN_REGEX.test(candidate)) ?? null;
}

function extractPreCodeMeta(node) {
  const codeNode = node?.children?.find((child) => child?.type === "element" && child.tagName === "code");
  const meta = codeNode?.properties?.dataCodeMeta ?? codeNode?.data?.meta;
  return typeof meta === "string" && meta.trim().length > 0 ? meta.trim() : undefined;
}

function remarkPreserveCodeMeta() {
  return (tree) => {
    const visit = (node) => {
      if (node.type === "code" && typeof node.meta === "string" && node.meta.trim().length > 0) {
        node.data = { ...node.data, hProperties: { ...node.data?.hProperties, dataCodeMeta: node.meta.trim() } };
      }
      node.children?.forEach(visit);
    };
    visit(tree);
  };
}

function nodeToPlainText(node) {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(nodeToPlainText).join("");
  if (isValidElement(node)) return nodeToPlainText(node.props.children);
  return "";
}

function extractCodeBlock(children) {
  const childNodes = Children.toArray(children);
  if (childNodes.length !== 1) return null;
  const onlyChild = childNodes[0];
  if (!isValidElement(onlyChild) || onlyChild.type !== "code") return null;
  return { className: onlyChild.props.className, code: nodeToPlainText(onlyChild.props.children) };
}

function findTaskListMarkerOffset(markdown, listItemStart) {
  const firstLineEnd = markdown.indexOf("\n", listItemStart);
  const firstLine = markdown.slice(listItemStart, firstLineEnd === -1 ? markdown.length : firstLineEnd);
  const match = firstLine.match(/^(?:\s*(?:[-+*]|\d+[.)])\s+)(\[[ xX]\])/);
  if (!match?.[1]) return null;
  return listItemStart + firstLine.indexOf(match[1]);
}

// :737-795
function pathParentSegments(p) {
  return p.replaceAll("\\", "/").split("/").filter((s) => s.length > 0).slice(0, -1);
}
function buildFileLinkParentSuffixByPath(filePaths) {
  const groups = new Map();
  for (const filePath of filePaths) {
    const segs = filePath.replaceAll("\\", "/").split("/").filter((s) => s.length > 0);
    const basename = segs[segs.length - 1];
    if (!basename) continue;
    const group = groups.get(basename) ?? new Set();
    group.add(filePath);
    groups.set(basename, group);
  }
  const suffixByPath = new Map();
  for (const group of groups.values()) {
    const uniquePaths = [...group];
    if (uniquePaths.length < 2) continue;
    const parentSegmentsByPath = new Map(uniquePaths.map((fp) => [fp, pathParentSegments(fp)]));
    const minUniqueDepthByPath = new Map();
    for (const filePath of uniquePaths) {
      const segments = parentSegmentsByPath.get(filePath) ?? [];
      let resolvedDepth = segments.length;
      for (let depth = 1; depth <= segments.length; depth += 1) {
        const candidate = segments.slice(-depth).join("/");
        const collision = uniquePaths.some((other) => {
          if (other === filePath) return false;
          return (parentSegmentsByPath.get(other) ?? []).slice(-depth).join("/") === candidate;
        });
        if (!collision) {
          resolvedDepth = depth;
          break;
        }
      }
      minUniqueDepthByPath.set(filePath, resolvedDepth);
    }
    for (const filePath of uniquePaths) {
      const segments = parentSegmentsByPath.get(filePath) ?? [];
      if (segments.length === 0) continue;
      const minUniqueDepth = minUniqueDepthByPath.get(filePath) ?? 1;
      const suffixDepth = Math.min(segments.length, Math.max(minUniqueDepth, 2));
      suffixByPath.set(filePath, segments.slice(-suffixDepth).join("/"));
    }
  }
  return suffixByPath;
}

const MARKDOWN_LINK_HREF_PATTERN = /\[[^\]]*]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)/g;
const MARKDOWN_FILE_LINK_CLASS_NAME = "chat-markdown-file-link cursor-pointer transition-colors hover:bg-accent/70";
function extractMarkdownLinkHrefs(text) {
  const hrefs = [];
  for (const match of text.matchAll(MARKDOWN_LINK_HREF_PATTERN)) {
    const href = match[1]?.trim();
    if (href) hrefs.push(href);
  }
  return hrefs;
}
function normalizeMarkdownLinkHrefKey(href) {
  const normalizedHref = markdownLinks.normalizeMarkdownLinkDestination(href);
  return markdownLinks.rewriteMarkdownFileUriHref(normalizedHref) ?? normalizedHref;
}

const MARKDOWN_LINK_FAVICON_CLASS_NAME = "block size-full shrink-0 select-none";
function resolveExternalLinkHost(href) {
  if (!href) return null;
  try {
    const url = new URL(href);
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.hostname || null;
  } catch {
    return null;
  }
}

// :828-849, rendered in its failed-favicon state (GlobeIcon) so the reference
// needs no network.
function MarkdownLinkFavicon() {
  return h(
    "span",
    { className: "chat-markdown-link-favicon", "aria-hidden": true },
    h(lucide.GlobeIcon, { className: MARKDOWN_LINK_FAVICON_CLASS_NAME }),
  );
}

function leadingExternalLinkTextLength(text) {
  const protocol = /^(?:https?:\/\/)/i.exec(text)?.[0];
  if (protocol) return protocol.length;
  return Math.min(text.length, 1);
}
function breakableExternalLinkText(text) {
  return Array.from(text, (character, index) =>
    h(Fragment, { key: `${index}:${character}` }, character, h("wbr")),
  );
}
function plainHastText(node) {
  if (!node || typeof node !== "object" || !Array.isArray(node.children)) return null;
  const parts = node.children.map((child) =>
    child?.type === "text" && typeof child.value === "string" ? child.value : null,
  );
  return parts.every((part) => part !== null) ? parts.join("") : null;
}

function MarkdownExternalLinkContent({ host, plainText, children }) {
  if (plainText) {
    const leadingLength = leadingExternalLinkTextLength(plainText);
    return h(
      Fragment,
      null,
      h("span", { className: "chat-markdown-link-leading" }, h(MarkdownLinkFavicon, { host }), plainText.slice(0, leadingLength)),
      breakableExternalLinkText(plainText.slice(leadingLength)),
    );
  }
  const childNodes = Children.toArray(children);
  const firstChild = childNodes[0];
  if (typeof firstChild === "string" && firstChild.length > 0) {
    const leadingLength = leadingExternalLinkTextLength(firstChild);
    return h(
      Fragment,
      null,
      h("span", { className: "chat-markdown-link-leading" }, h(MarkdownLinkFavicon, { host }), firstChild.slice(0, leadingLength)),
      breakableExternalLinkText(firstChild.slice(leadingLength)),
      childNodes.slice(1),
    );
  }
  return h(
    Fragment,
    null,
    h("span", { className: "chat-markdown-link-leading" }, h(MarkdownLinkFavicon, { host }), firstChild),
    childNodes.slice(1),
  );
}

// ---------------------------------------------------------------------------
// Code highlighting (:277-296, :642-715). The fork suspends on the shared
// @pierre/diffs highlighter; here every block is highlighted up front with the
// same highlighter and the same fallbacks, then rendered synchronously.
const highlighterPromiseCache = new Map();
function getHighlighterPromise(language) {
  const cached = highlighterPromiseCache.get(language);
  if (cached) return cached;
  const promise = getSharedHighlighter({
    themes: ["pierre-dark", "pierre-light"],
    langs: [language],
    preferredHighlighter: "shiki-js",
  }).catch((err) => {
    highlighterPromiseCache.delete(language);
    if (language === "text") throw err;
    return getHighlighterPromise("text");
  });
  highlighterPromiseCache.set(language, promise);
  return promise;
}
async function highlight(code, language, themeName) {
  const highlighter = await getHighlighterPromise(language);
  try {
    return highlighter.codeToHtml(code, { lang: language, theme: themeName });
  } catch (error) {
    console.warn(`highlight failed for ${language}, falling back to text`, error?.message ?? error);
    return highlighter.codeToHtml(code, { lang: "text", theme: themeName });
  }
}
const highlightKey = (code, language, themeName) => `${language}\u0000${themeName}\u0000${code}`;

// ---------------------------------------------------------------------------
// Markdown components, closed over one render context.
function createComponents(ctx) {
  const chromeAction = (props, icon) =>
    h(Button, { type: "button", variant: "ghost", size: "icon-xs", className: "chat-markdown-chrome-action", ...props }, icon);

  // :302-435
  function MarkdownTable({ node: _node, children, ...props }) {
    const expanded = ctx.wordWrap;
    const expandLabel = expanded ? "Collapse table cells" : "Expand table cells";
    return h(
      "div",
      { className: "chat-markdown-table-container", "data-expanded": expanded ? "true" : "false" },
      h(
        ScrollArea,
        { chainVerticalScroll: true, scrollFade: true, hideScrollbars: true, className: "w-full max-w-full rounded-none" },
        h("table", props, children),
      ),
      h(
        "div",
        { className: "chat-markdown-table-footer select-none" },
        chromeAction(
          { "aria-pressed": expanded, "aria-label": expandLabel },
          h(expanded ? lucide.Minimize2Icon : lucide.Maximize2Icon, { className: "size-3" }),
        ),
        chromeAction({ "aria-label": "Copy table" }, h(lucide.CopyIcon, { className: "size-3" })),
      ),
    );
  }

  // :437-478 (closed: base-ui unmounts the panel)
  function MarkdownDetails({ node: _node, children, open = false }) {
    const childNodes = Children.toArray(children);
    const summaryIndex = childNodes.findIndex((child) => isValidElement(child) && child.type === "summary");
    const summaryNode = summaryIndex >= 0 ? childNodes[summaryIndex] : null;
    const summary = isValidElement(summaryNode) && summaryNode.props.children ? summaryNode.props.children : "Details";
    const content = childNodes.filter((_, index) => index !== summaryIndex);
    return h(
      Collapsible,
      {
        defaultOpen: open,
        className: "chat-markdown-details my-2 border-y border-border/60",
        "data-markdown-details": "",
        "data-markdown-details-open": open ? "true" : "false",
      },
      h(
        CollapsibleTrigger,
        {
          className:
            "flex w-full items-center gap-2 py-2 text-left text-sm font-medium text-foreground data-panel-open:[&_svg]:rotate-90",
          "data-markdown-details-summary": "",
        },
        h(lucide.ChevronRightIcon, { className: "size-4 shrink-0 text-muted-foreground transition-transform", "aria-hidden": true }),
        h("span", null, summary),
      ),
      h(
        CollapsiblePanel,
        null,
        h("div", { className: "pb-3 ps-6 text-foreground/80", "data-markdown-details-content": "" }, content),
      ),
    );
  }

  // :485-519
  function MarkdownCodeBlockTitleContent({ fenceTitle, language, theme }) {
    if (fenceTitle) {
      return h(
        Fragment,
        null,
        h(PierreEntryIcon, { pathValue: fenceTitle, kind: "file", theme, className: "size-3.5" }),
        h("span", { className: "truncate" }, fenceTitle),
      );
    }
    const fileName = pierreIcons.syntheticFileNameForLanguageId(language);
    if (!pierreIcons.hasSpecificPierreIconForFileName(fileName)) {
      return h("span", { className: "truncate" }, language);
    }
    return h(
      "span",
      { className: "inline-flex shrink-0 rounded-sm", "aria-label": `Language: ${language}` },
      h(PierreEntryIcon, { pathValue: fileName, kind: "file", theme, className: "size-3.5" }),
    );
  }

  // :521-633 + :1444-1472
  function MarkdownPre({ node, children, ...props }) {
    const codeBlock = extractCodeBlock(children);
    if (!codeBlock) return h("pre", props, children);
    const language = extractFenceLanguage(codeBlock.className);
    const fenceTitle = extractFenceTitle(extractPreCodeMeta(node));
    const wrapped = ctx.wordWrap;
    const key = highlightKey(codeBlock.code, language, ctx.diffThemeName);
    const html = ctx.highlighted.get(key);
    if (html == null) ctx.pendingHighlights.set(key, { code: codeBlock.code, language, themeName: ctx.diffThemeName });
    return h(
      "div",
      { className: "chat-markdown-codeblock leading-snug", "data-language": language, "data-wrap": wrapped ? "true" : "false" },
      h(
        "div",
        { className: "chat-markdown-codeblock-header select-none" },
        h("span", { className: "chat-markdown-codeblock-title" }, h(MarkdownCodeBlockTitleContent, { fenceTitle, language, theme: ctx.theme })),
        h(
          "span",
          { className: "flex items-center gap-0.5" },
          chromeAction(
            { "aria-pressed": wrapped, "aria-label": wrapped ? "Disable line wrap" : "Wrap lines" },
            h(lucide.WrapTextIcon, { className: "size-3" }),
          ),
          chromeAction({ "aria-label": "Copy code" }, h(lucide.CopyIcon, { className: "size-3" })),
        ),
      ),
      html == null
        ? h("pre", props, children) // Suspense fallback; only seen on the first (collecting) pass
        : h("div", { className: "chat-markdown-shiki", dangerouslySetInnerHTML: { __html: html } }),
    );
  }

  // :1258-1273
  function MarkdownParagraph({ node: _node, children, ...props }) {
    return h("p", props, children);
  }
  function MarkdownListItem({ node, children, ...props }) {
    const listItemStart = node?.position?.start.offset;
    const markerOffset = typeof listItemStart === "number" ? findTaskListMarkerOffset(ctx.text, listItemStart) : null;
    return h("li", { ...props, "data-task-marker-offset": markerOffset ?? undefined }, children);
  }
  // :1275-1308 (no onTaskListChange for assistant rows)
  function MarkdownInput({ node: _node, type, checked, disabled, ...props }) {
    return h("input", { ...props, type, checked, disabled, readOnly: type === "checkbox" });
  }

  // :1310-1430
  function MarkdownAnchor({ node, href, children, ...props }) {
    const normalizedHref = href ? normalizeMarkdownLinkHrefKey(href) : "";
    const fileLinkMeta = normalizedHref ? ctx.metaByHref.get(normalizedHref) : null;
    if (!fileLinkMeta) {
      const faviconHost = resolveExternalLinkHost(href);
      const isSameDocumentLink = href?.startsWith("#") ?? false;
      return h(
        "a",
        {
          ...props,
          href,
          target: isSameDocumentLink ? undefined : "_blank",
          rel: isSameDocumentLink ? undefined : "noopener noreferrer",
        },
        faviconHost ? h(MarkdownExternalLinkContent, { host: faviconHost, plainText: plainHastText(node) }, children) : children,
      );
    }
    const parentSuffix = ctx.suffixByPath.get(fileLinkMeta.filePath);
    const labelParts = [fileLinkMeta.basename];
    if (typeof parentSuffix === "string" && parentSuffix.length > 0) labelParts.push(parentSuffix);
    if (fileLinkMeta.line) labelParts.push(`L${fileLinkMeta.line}${fileLinkMeta.column ? `:C${fileLinkMeta.column}` : ""}`);
    return h(
      "a",
      {
        href: fileLinkMeta.targetPath,
        className: cn(chip.CHAT_INLINE_CHIP_CLASS_NAME, MARKDOWN_FILE_LINK_CLASS_NAME, props.className),
        "data-markdown-copy": `[${fileLinkMeta.basename}](${normalizedHref})`,
      },
      h(FileTagChipContent, { path: fileLinkMeta.filePath, label: labelParts.join(" · "), theme: ctx.theme, selectable: true }),
    );
  }

  return {
    p: MarkdownParagraph,
    li: MarkdownListItem,
    input: MarkdownInput,
    a: MarkdownAnchor,
    table: MarkdownTable,
    details: MarkdownDetails,
    pre: MarkdownPre,
  };
}

// ChatMarkdown root (:1503-1631) with one non-streaming chunk.
function renderChatMarkdown(ctx) {
  return renderToStaticMarkup(
    h(
      "div",
      { className: cn("chat-markdown w-full min-w-0 text-sm leading-relaxed text-foreground/80") },
      h(ReactMarkdown, {
        remarkPlugins: [remarkGfm, remarkPreserveCodeMeta],
        rehypePlugins: [rehypeRaw, [rehypeSanitize, CHAT_MARKDOWN_SANITIZE_SCHEMA]],
        components: createComponents(ctx),
        urlTransform: (href) => markdownLinks.rewriteMarkdownFileUriHref(href) ?? defaultUrlTransform(href),
        children: ctx.text,
      }),
    ),
  );
}

const highlighted = new Map();
async function renderMarkdownHtml({ text, theme, wordWrap }) {
  const metaByHref = new Map();
  for (const href of extractMarkdownLinkHrefs(text)) {
    const normalizedHref = normalizeMarkdownLinkHrefKey(href);
    if (metaByHref.has(normalizedHref)) continue;
    const meta = markdownLinks.resolveMarkdownFileLinkMeta(normalizedHref, CWD);
    if (meta) metaByHref.set(normalizedHref, meta);
  }
  const ctx = {
    text,
    theme,
    wordWrap,
    diffThemeName: theme === "light" ? "pierre-light" : "pierre-dark",
    metaByHref,
    suffixByPath: buildFileLinkParentSuffixByPath([...metaByHref.values()].map((m) => m.filePath)),
    highlighted,
    pendingHighlights: new Map(),
  };
  renderChatMarkdown(ctx); // collect code blocks
  for (const [key, { code, language, themeName }] of ctx.pendingHighlights) {
    highlighted.set(key, await highlight(code, language, themeName));
  }
  ctx.pendingHighlights.clear();
  const html = renderChatMarkdown(ctx);
  if (ctx.pendingHighlights.size > 0) throw new Error("unhighlighted code blocks remain");
  return { html, metaByHref };
}

// ---------------------------------------------------------------------------
// Assets: compiled CSS + fonts.
function ensureCss() {
  if (fs.existsSync(`${OUT}/css/out.css`)) return;
  execFileSync(process.execPath, [COMPILE], { env: { ...process.env, T3SPEC_TMP: `${OUT}/css` }, stdio: "inherit" });
}

// main.tsx imports @fontsource-variable/dm-sans/index.css and
// @fontsource/jetbrains-mono/{400,500}.css; copy their faces and woff2 files.
function buildFonts() {
  fs.mkdirSync(`${OUT}/fonts`, { recursive: true });
  const sources = [
    `${P}/@fontsource-variable+dm-sans@5.2.8/node_modules/@fontsource-variable/dm-sans/index.css`,
    `${P}/@fontsource+jetbrains-mono@5.2.8/node_modules/@fontsource/jetbrains-mono/400.css`,
    `${P}/@fontsource+jetbrains-mono@5.2.8/node_modules/@fontsource/jetbrains-mono/500.css`,
  ];
  let css = "/* Copied from the fork's @fontsource packages (see apps/web/src/main.tsx:8-10). */\n";
  for (const source of sources) {
    const text = fs
      .readFileSync(source, "utf8")
      // keep only the woff2 source
      .replace(/,\s*url\(\.\/files\/[^)]+\.woff\)\s*format\('woff'\)/g, "")
      .replace(/url\(\.\/files\/([^)]+)\)/g, (_, file) => {
        fs.copyFileSync(path.join(path.dirname(source), "files", file), `${OUT}/fonts/${file}`);
        return `url(./${file})`;
      });
    css += `\n/* ${path.relative(P, source)} */\n${text}\n`;
  }
  fs.writeFileSync(`${OUT}/fonts/fonts.css`, css);
}

// After fonts load, mimic what base-ui's ScrollArea does in its layout effect:
// register the overflow vars (initial 0px) and set them plus the overflow data
// attributes from the measured viewport, so the table fade masks match the app.
const SCROLL_AREA_SCRIPT = `
(async () => {
  await document.fonts.ready;
  const names = ["--scroll-area-overflow-x-start", "--scroll-area-overflow-x-end", "--scroll-area-overflow-y-start", "--scroll-area-overflow-y-end"];
  for (const name of names) { try { CSS.registerProperty({ name, syntax: "<length>", inherits: false, initialValue: "0px" }); } catch {} }
  for (const vp of document.querySelectorAll('[data-slot="scroll-area-viewport"]')) {
    const root = vp.parentElement;
    const xHidden = vp.clientWidth >= vp.scrollWidth;
    const yHidden = vp.clientHeight >= vp.scrollHeight;
    const maxX = Math.max(0, vp.scrollWidth - vp.clientWidth);
    const maxY = Math.max(0, vp.scrollHeight - vp.clientHeight);
    const xs = xHidden ? 0 : Math.min(Math.max(vp.scrollLeft, 0), maxX);
    const ys = yHidden ? 0 : Math.min(Math.max(vp.scrollTop, 0), maxY);
    const vals = [xs, xHidden ? 0 : maxX - xs, ys, yHidden ? 0 : maxY - ys];
    names.forEach((n, i) => vp.style.setProperty(n, vals[i] + "px"));
    const attrs = {
      "data-has-overflow-x": !xHidden, "data-has-overflow-y": !yHidden,
      "data-overflow-x-start": vals[0] > 0, "data-overflow-x-end": vals[1] > 0,
      "data-overflow-y-start": vals[2] > 0, "data-overflow-y-end": vals[3] > 0,
    };
    for (const el of [root, vp]) for (const [k, on] of Object.entries(attrs)) on ? el.setAttribute(k, "") : el.removeAttribute(k);
  }
  document.documentElement.dataset.referenceReady = "true";
})();`;

// Page frame: index.html boot styles + MessagesTimeline list/row/assistant frame.
function pageHtml({ theme, wordWrap, markdownHtml, sprite }) {
  // React 19 hoists <style precedence> into <head> on the client.
  const hoisted = [];
  const body = markdownHtml.replace(/<style data-precedence="[^"]*"[^>]*>[\s\S]*?<\/style>/g, (m) => {
    if (!hoisted.includes(m)) hoisted.push(m);
    return "";
  });
  return `<!doctype html>
<html lang="en"${theme === "dark" ? ' class="dark"' : ""}>
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>ChatMarkdown reference: ${theme}${wordWrap ? "" : " (wordWrap off)"}</title>
<style>
  /* apps/web/index.html boot styles (unlayered, so they beat @layer base) */
  html, body, #root { width: 100%; height: 100%; margin: 0; }
  body { background: #ffffff; color: #262626; font-family: "DM Sans Variable", "DM Sans", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif; }
  html.dark body { background: #161616; color: #f5f5f5; }
</style>
<link rel="stylesheet" href="./fonts/fonts.css" />
<link rel="stylesheet" href="./css/out.css" />
${hoisted.join("\n")}
<style>
  /* Reference frame: paint the theme background everywhere and drop the
     app-wide 3.5% film-grain overlay (index.css body::after) so pixels are
     deterministic. */
  html, body { background: var(--background); }
  body::after { display: none; }
  /* ?tile: lay out at 1440px wide regardless of the viewport and let the
     window scroll both ways (hidden scrollbars), so a small viewport can
     capture unscaled tiles of the same layout. */
  html.reference-tile { overflow: auto; scrollbar-width: none; min-height: 0; }
  html.reference-tile body, html.reference-tile #root { width: 1440px; height: auto; min-height: 0; overflow: visible; }
  html.reference-tile [data-reference-scroller] { height: auto; overflow: visible !important; }
</style>
<script>if (new URLSearchParams(location.search).has("tile")) document.documentElement.classList.add("reference-tile");</script>
</head>
<body>
<div id="t3code-pierre-file-icon-sprite" aria-hidden="true" style="position:absolute;width:0;height:0;overflow:hidden;pointer-events:none">${sprite}</div>
<div id="root">
  <div class="scrollbar-gutter-both h-full min-h-0 overflow-x-hidden overscroll-y-contain px-3 [overflow-anchor:none] sm:px-5" style="overflow-y:auto" data-reference-scroller="">
    <div class="h-3 sm:h-4"></div>
    <div class="mx-auto w-full min-w-0 max-w-3xl overflow-x-clip" data-timeline-root="true">
      <div class="pb-4 group/assistant" data-timeline-row-kind="message" data-message-role="assistant">
        <div class="relative min-w-0 px-1 py-0.5">
${body}
        </div>
      </div>
    </div>
    <div class="h-3 sm:h-4"></div>
  </div>
</div>
<script>${SCROLL_AREA_SCRIPT}</script>
</body>
</html>
`;
}

// ---------------------------------------------------------------------------
ensureCss();
buildFonts();
const text = fs.readFileSync(SAMPLE, "utf8");
const sprite = `${getBuiltInSpriteSheet("complete")}${pierreIcons.T3_PIERRE_ICONS.spriteSheet}`;
const variants = [
  { file: "dark.html", theme: "dark", wordWrap: true },
  { file: "light.html", theme: "light", wordWrap: true },
  { file: "dark-nowrap.html", theme: "dark", wordWrap: false },
  { file: "light-nowrap.html", theme: "light", wordWrap: false },
];
for (const variant of variants) {
  const { html, metaByHref } = await renderMarkdownHtml({ text, ...variant });
  fs.writeFileSync(`${OUT}/${variant.file}`, pageHtml({ ...variant, markdownHtml: html, sprite }));
  fs.writeFileSync(`${OUT}/${variant.file.replace(".html", ".fragment.html")}`, html);
  if (variant.file === "dark.html") {
    console.log("file links:", JSON.stringify([...metaByHref.entries()], null, 1));
  }
  console.log("wrote", variant.file);
}
fs.writeFileSync(
  `${OUT}/index.html`,
  `<!doctype html><meta charset="utf-8"><title>ChatMarkdown reference</title><ul>${variants
    .map((v) => `<li><a href="./${v.file}">${v.file}</a></li>`)
    .join("")}</ul>`,
);
