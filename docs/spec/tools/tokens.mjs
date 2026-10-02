// Build docs/spec/tokens.json from $T3SPEC_TMP/resolved.json (resolve-css.mjs output) plus the
// values that only exist in TypeScript (window options, status classes) or Tailwind defaults.
//
// Run order: compile.mjs -> resolve-css.mjs -> tokens.mjs. Sections that this pass did not
// re-derive from the fork are listed in `$meta.carriedOver` and keep their earlier values.
import fs from "node:fs";
import path from "node:path";

import { FORK, TMP, forkCommit } from "./fork.mjs";

const resolved = JSON.parse(fs.readFileSync(`${TMP}/resolved.json`, "utf8"));
const previous = JSON.parse(fs.readFileSync(new URL("../tokens.json", import.meta.url), "utf8"));
const hexes = (map) => Object.fromEntries(Object.entries(map).map(([k, v]) => [k, v.hex]));
const perMode = (pick) => ({ light: pick("light"), dark: pick("dark") });
const root = (mode, name) => {
  const value = resolved.root[mode][name];
  if (!value) throw new Error(`token --${name} missing (${mode})`);
  return value.hex;
};
const raw = (mode, name) => resolved.raw[mode][name];
const shade = (hue, s) => resolved.palette[hue][String(s)].hex;
/** `#RRGGBBAA` with its alpha multiplied (Tailwind `/NN` on an opaque color). */
const withAlpha = (hex, percent) => {
  const a = parseInt(hex.slice(7, 9), 16) / 255;
  return `${hex.slice(0, 7)}${Math.round(a * (percent / 100) * 255).toString(16).padStart(2, "0").toUpperCase()}`;
};
const T = "#00000000";

// ---------------------------------------------------------------------------- status colors
// apps/web/src/components/Sidebar.logic.ts + ThreadStatusIndicators.tsx: light text -600 /
// dot -500, dark both -300 at 80% (sky) or 90%.
const statusTone = (hue, darkAlpha = 90, dot = true) => ({
  "light.text": shade(hue, 600),
  ...(dot ? { "light.dot": shade(hue, 500) } : {}),
  "dark.text": withAlpha(shade(hue, 300), darkAlpha),
  ...(dot ? { "dark.dot": withAlpha(shade(hue, 300), darkAlpha) } : {}),
});
const status = {
  "working (sky)": statusTone("sky", 80),
  "pending-approval (amber)": statusTone("amber"),
  "awaiting-input (indigo)": statusTone("indigo"),
  "error (rose)": statusTone("rose"),
  "plan-ready (violet)": statusTone("violet"),
  "completed / pr-open (emerald)": statusTone("emerald"),
  "pr-closed (zinc)": { "light.text": shade("zinc", 500), "dark.text": withAlpha(shade("zinc", 400), 80) },
  "pr-merged (violet)": { "light.text": shade("violet", 600), "dark.text": withAlpha(shade("violet", 300), 90) },
  "terminal-running (teal)": { "light.text": shade("teal", 600), "dark.text": withAlpha(shade("teal", 300), 90) },
};

// --------------------------------------------------------------------------------- terminal
// The fork's terminal is a Ghostty renderer now; background, foreground, cursor and selection
// come from CSS tokens. The ANSI table is the pre-refresh xterm palette (terminal agent owns it).
const terminal = {
  ...previous.terminal,
  fontFamily: '"SF Mono", "SFMono-Regular", Menlo, Consolas, "Liberation Mono", monospace',
  fontSizePx: 12,
  ansi: Object.fromEntries(
    ["light", "dark"].map((mode) => [
      mode,
      {
        ...previous.terminal.ansi[mode],
        cursor: root(mode, "terminal-cursor"),
        selectionBackground: root(mode, "terminal-selection-background"),
      },
    ]),
  ),
};

// ------------------------------------------------------------------------- gpui-component
function gpuiTheme(mode) {
  const c = (name) => root(mode, name);
  const a = (key, fallback) => resolved.alpha[mode][key]?.hex ?? fallback;
  const dark = mode === "dark";
  return {
    is_default: false,
    name: dark ? "T3 Code Dark" : "T3 Code Light",
    mode,
    "font.family": ".SystemUIFont",
    "font.size": 16,
    "mono_font.family": "SF Mono",
    "mono_font.size": 13,
    radius: 8,
    "radius.lg": 10,
    shadow: true,
    colors: {
      background: c("background"),
      foreground: c("contrast-foreground"),
      border: c("contrast-border"),
      "input.border": c("contrast-input"),
      ring: c("ring"),
      caret: c("contrast-foreground"),
      "accent.background": c("accent"),
      "accent.foreground": c("contrast-accent-foreground"),
      "muted.background": c("muted"),
      "muted.foreground": c("contrast-muted-foreground"),
      "popover.background": c("popover"),
      "popover.foreground": c("contrast-popover-foreground"),
      "primary.background": c("primary"),
      "primary.foreground": c("primary-foreground"),
      "primary.hover.background": a("primary/90", withAlpha(c("primary"), 90)),
      "primary.active.background": a("primary/90", withAlpha(c("primary"), 90)),
      "secondary.background": c("secondary"),
      "secondary.foreground": c("contrast-secondary-foreground"),
      "secondary.hover.background": a("secondary/90", withAlpha(c("secondary"), 90)),
      "secondary.active.background": a("secondary/80", withAlpha(c("secondary"), 80)),
      "danger.background": c("destructive"),
      "danger.foreground": "#FFFFFFFF",
      "danger.hover.background": a("destructive/90", withAlpha(c("destructive"), 90)),
      "danger.active.background": a("destructive/90", withAlpha(c("destructive"), 90)),
      "info.background": c("info"),
      "info.foreground": "#FFFFFFFF",
      "success.background": c("success"),
      "success.foreground": "#FFFFFFFF",
      "warning.background": c("warning"),
      "warning.foreground": "#FFFFFFFF",
      "list.background": T,
      "list.hover.background": c("accent"),
      "list.active.background": c("accent"),
      "table.background": T,
      "table.hover.background": a("muted/50", withAlpha(c("muted"), 50)),
      "table.active.background": c("accent"),
      "table.head.foreground": c("contrast-foreground"),
      "table.row.border": c("contrast-border"),
      "sidebar.background": c("sidebar"),
      "sidebar.foreground": c("contrast-sidebar-foreground"),
      "sidebar.border": c("contrast-sidebar-border"),
      "sidebar.accent.background": c("sidebar-row-active"),
      "sidebar.accent.foreground": c("contrast-sidebar-foreground"),
      "title_bar.background": T,
      "title_bar.border": c("contrast-border"),
      "tab.background": T,
      "tab.active.background": c("accent"),
      "tab.foreground": c("contrast-muted-foreground"),
      "tab.active.foreground": c("contrast-foreground"),
      "tab_bar.background": T,
      "scrollbar.background": T,
      "scrollbar.thumb.background": c("app-scrollbar-thumb"),
      "scrollbar.thumb.hover.background": c("app-scrollbar-thumb-hover"),
      "selection.background": "#007AFF4D",
      "switch.background": c("contrast-input"),
      "switch.thumb.background": c("background"),
      "skeleton.background": c("muted"),
      "progress.bar.background": c("primary"),
      link: c("info-foreground"),
      "link.hover": c("info-foreground"),
      "link.active": c("info-foreground"),
      "drag.border": a("primary/70", withAlpha(c("primary"), 70)),
      "drop_target.background": a("accent/45", withAlpha(c("accent"), 45)),
      // dialog-backdrop: color-mix(in srgb, background 60% / 64% dark, transparent).
      overlay: withAlpha(c("background"), dark ? 64 : 60),
      "window.border": c("contrast-border"),
      "base.red": shade("red", 500),
      "base.green": shade("emerald", 500),
      "base.blue": shade("blue", 500),
      "base.yellow": shade("amber", 500),
      "base.magenta": shade("violet", 500),
      "base.cyan": shade("cyan", 500),
    },
  };
}

// ------------------------------------------------------------------------------- assemble
const outOfSrgbGamut = {};
for (const mode of ["light", "dark"]) {
  for (const [name, value] of Object.entries(resolved.root[mode])) {
    if (value.p3) outOfSrgbGamut[`${mode}.${name}`] = { srgbClipped: value.hex, displayP3: value.p3 };
  }
}

const rem = (value) => {
  const m = String(value).match(/^([\d.]+)rem$/);
  return m ? Number(m[1]) * 16 : Number.parseFloat(value);
};

const tokens = {
  $meta: {
    source: `${path.relative(process.env.HOME, FORK).replace(/^/, "~/")} @ ${forkCommit()} (apps/web/src/index.css, components/ui/*, apps/desktop/src/window/DesktopWindow.ts)`,
    generator: "docs/spec/tools/{compile,resolve-css,tokens}.mjs",
    tailwindcss: resolved.meta.tailwindcss,
    colorFormat: "sRGB #RRGGBBAA, channel-clipped. Alpha colors are not pre-composited.",
    appearance:
      "Default appearance: theme preference `system` (no theme preset applied), contrast 100. `color` is the <html> scope; `colorSidebar` lists what an element with [data-app-sidebar] computes differently.",
    desktopBreakpoint:
      "Window min width is 840px, so Tailwind `sm:` (>=640px) and `md:` (>=768px) values always apply on desktop.",
    remPx: 16,
    spacingUnitPx: 4,
    carriedOver: [
      "motion",
      "layout (beyond topbarHeightPx and titlebar control sizes)",
      "terminal.ansi (16 ANSI colors; terminal now renders via Ghostty)",
      "providerAccentSwatches",
      "providerLogoColors",
      "scrollbar.modelPicker",
      "scrollbar.codeBlockHorizontal",
      "status pr-closed / error (no longer in Sidebar.logic.ts)",
    ],
  },
  color: perMode((mode) => hexes(resolved.root[mode])),
  colorAlpha: perMode((mode) => hexes(resolved.alpha[mode])),
  colorLegacy: perMode((mode) => hexes(resolved.legacy[mode])),
  colorSidebar: perMode((mode) => ({
    tokens: hexes(resolved.sidebar[mode].tokens),
    alpha: hexes(resolved.sidebar[mode].alpha),
    legacy: hexes(resolved.sidebar[mode].legacy),
  })),
  outOfSrgbGamut,
  palette: Object.fromEntries(Object.entries(resolved.palette).map(([hue, shades]) => [hue, hexes(shades)])),
  status,
  providerAccentSwatches: previous.providerAccentSwatches,
  providerLogoColors: previous.providerLogoColors,
  scrollbar: {
    ...previous.scrollbar,
    native: {
      widthPx: rem(raw("light", "app-scrollbar-width")),
      thumbRadiusPx: 3,
      track: T,
      thumb: perMode((mode) => root(mode, "app-scrollbar-thumb")),
      thumbHover: perMode((mode) => root(mode, "app-scrollbar-thumb-hover")),
    },
  },
  terminal,
  radius: {
    base: rem(raw("light", "radius")),
    control: rem(raw("light", "control-radius")),
    xs: 2,
    sm: rem(raw("light", "radius")) - 4,
    md: rem(raw("light", "radius")) - 2,
    lg: rem(raw("light", "radius")),
    xl: rem(raw("light", "radius")) + 4,
    "2xl": rem(raw("light", "radius")) + 8,
    "3xl": rem(raw("light", "radius")) + 12,
    rounded: 4,
    full: 9999,
  },
  borderWidthPx: { default: 1 },
  shadow: {
    ...previous.shadow,
    composer: {
      light: [{ x: 0, y: 12, blur: 28, spread: -18, color: "#00000066" }],
      dark: [{ x: 0, y: 14, blur: 32, spread: -18, color: "#000000BF" }],
    },
    dialogGlass: {
      light: [{ x: 0, y: 24, blur: 64, spread: -24, color: "#000000A6" }],
      dark: [{ x: 0, y: 24, blur: 72, spread: -20, color: "#000000E6" }],
      darkInsetTop: { y: 1, color: "#FFFFFF0A" },
    },
  },
  typography: {
    sans: {
      family: "system UI (-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif)",
      gpuiFamily: ".SystemUIFont",
      note: "Settings -> Appearance can prepend a custom family; the default renders SF Pro on macOS.",
    },
    mono: {
      stack: ["SF Mono", "SFMono-Regular", "Menlo", "Consolas", "Liberation Mono", "monospace"],
      note: "Runtime default (appearanceFonts.ts DEFAULT_CODE_FONT_STACK); index.css's --font-mono also starts with ui-monospace.",
    },
    defaultSizesPx: { interface: 16, prompt: 14, code: 13, terminal: 12 },
    sizes: {
      "5xs": [7, 7],
      "4xs": [8, 8],
      "3xs": [10, 14],
      "2xs": [11, 16],
      xs: [12, 16],
      sm: [14, 20],
      base: [16, 24],
      lg: [18, 28],
      xl: [20, 28],
      "2xl": [24, 32],
      "3xl": [30, 36],
    },
    weights: previous.typography.weights,
    tracking: previous.typography.tracking,
    leading: previous.typography.leading,
  },
  motion: previous.motion,
  window: {
    defaultSize: [1100, 780],
    minSize: [840, 620],
    mac: {
      titleBarStyle: "hiddenInset",
      trafficLightPosition: { x: 16, y: 52 / 2 - 7 },
      transparent: false,
      backgroundColor: { light: "#FFFFFF", dark: "#0A0A0A" },
      disableAutoHideCursor: true,
    },
    windowsLinux: previous.window.windowsLinux,
    title: "T3 Code",
  },
  layout: {
    ...previous.layout,
    topbarHeightPx: { mac: Number.parseFloat(raw("light", "workspace-topbar-height")), windowsLinux: 40 },
    titlebarControlSizePx: rem(raw("light", "workspace-titlebar-control-size")),
    titlebarControlGapPx: rem(raw("light", "workspace-titlebar-control-gap")),
  },
  noiseOverlay: {
    opacity: 0.035,
    tilePx: 256,
    svgFilter: "feTurbulence type=fractalNoise baseFrequency=0.9 numOctaves=4 stitchTiles=stitch",
    placement:
      "Behind content: body background-image and the surface-grain utility on surfaces that float over the body (index.css --surface-grain).",
  },
  gpuiComponentTheme: {
    name: "T3 Code",
    author: "generated from the t3code fork tokens",
    themes: [gpuiTheme("light"), gpuiTheme("dark")],
  },
};

fs.writeFileSync(new URL("../tokens.json", import.meta.url), `${JSON.stringify(tokens, null, 2)}\n`);
console.log(`wrote docs/spec/tokens.json from ${tokens.$meta.source}`);
