// Build docs/spec/tokens.json from colors.json (colors.mjs output) + hand-transcribed values.
import fs from "node:fs";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
// Scratch dir for generated intermediates (compiled CSS, colors.json). Override with T3SPEC_TMP.
const TMP = process.env.T3SPEC_TMP ?? "/tmp/t3spec";
fs.mkdirSync(TMP, { recursive: true });
const culori = require("/home/aadi/L-Projects/t3code-again/node_modules/.pnpm/culori@4.0.2/node_modules/culori/bundled/culori.cjs");
const colors = JSON.parse(fs.readFileSync(`${TMP}/colors.json`, "utf8"));
const theme = fs.readFileSync("/home/aadi/L-Projects/t3code-again/node_modules/.pnpm/tailwindcss@4.3.0/node_modules/tailwindcss/theme.css", "utf8");
const tw = (n) => {
  const m = theme.match(new RegExp(`--color-${n}:\\s*([^;]+);`));
  const c = culori.converter("rgb")(culori.parse(m[1]));
  const cl = (v) => Math.min(1, Math.max(0, v));
  return culori.formatHex8({ mode: "rgb", r: cl(c.r), g: cl(c.g), b: cl(c.b), alpha: 1 }).toUpperCase();
};
const rgbToHex = (s) => {
  const m = s.match(/rgba?\(([^)]+)\)/);
  const [r, g, b, a = "1"] = m[1].split(",").map((x) => x.trim());
  const h = (n) => Math.round(Number(n)).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}${h(Number(a) * 255)}`.toUpperCase();
};
const pick = (mode) => Object.fromEntries(Object.entries(colors[mode]).map(([k, v]) => [k, v.hex]));
const pickDerived = (mode) => Object.fromEntries(Object.entries(colors.derived[mode]).map(([k, v]) => [k, v.hex]));

const L = pick("light"), D = pick("dark");
const dl = pickDerived("light"), dd = pickDerived("dark");
const T = "#00000000";

const terminal = {
  light: { cursor: "rgb(38, 56, 78)", selectionBackground: "rgba(37, 63, 99, 0.2)", scrollbarSliderBackground: "rgba(0, 0, 0, 0.15)", scrollbarSliderHoverBackground: "rgba(0, 0, 0, 0.25)", scrollbarSliderActiveBackground: "rgba(0, 0, 0, 0.3)", black: "rgb(44, 53, 66)", red: "rgb(191, 70, 87)", green: "rgb(60, 126, 86)", yellow: "rgb(146, 112, 35)", blue: "rgb(72, 102, 163)", magenta: "rgb(132, 86, 149)", cyan: "rgb(53, 127, 141)", white: "rgb(210, 215, 223)", brightBlack: "rgb(112, 123, 140)", brightRed: "rgb(212, 95, 112)", brightGreen: "rgb(85, 148, 111)", brightYellow: "rgb(173, 133, 45)", brightBlue: "rgb(91, 124, 194)", brightMagenta: "rgb(153, 107, 172)", brightCyan: "rgb(70, 149, 164)", brightWhite: "rgb(236, 240, 246)" },
  dark: { cursor: "rgb(180, 203, 255)", selectionBackground: "rgba(180, 203, 255, 0.25)", scrollbarSliderBackground: "rgba(255, 255, 255, 0.1)", scrollbarSliderHoverBackground: "rgba(255, 255, 255, 0.18)", scrollbarSliderActiveBackground: "rgba(255, 255, 255, 0.22)", black: "rgb(24, 30, 38)", red: "rgb(255, 122, 142)", green: "rgb(134, 231, 149)", yellow: "rgb(244, 205, 114)", blue: "rgb(137, 190, 255)", magenta: "rgb(208, 176, 255)", cyan: "rgb(124, 232, 237)", white: "rgb(210, 218, 230)", brightBlack: "rgb(110, 120, 136)", brightRed: "rgb(255, 168, 180)", brightGreen: "rgb(176, 245, 186)", brightYellow: "rgb(255, 224, 149)", brightBlue: "rgb(174, 210, 255)", brightMagenta: "rgb(229, 203, 255)", brightCyan: "rgb(167, 244, 247)", brightWhite: "rgb(244, 247, 252)" },
};
const terminalHex = Object.fromEntries(Object.entries(terminal).map(([m, v]) => [m, Object.fromEntries(Object.entries(v).map(([k, c]) => [k, rgbToHex(c)]))]));

function gpuiTheme(mode, c, d) {
  const dark = mode === "dark";
  return {
    is_default: false,
    name: dark ? "T3 Code Dark" : "T3 Code Light",
    mode,
    "font.family": "DM Sans",
    "font.size": 16,
    "mono_font.family": "JetBrains Mono",
    "mono_font.size": 12,
    radius: 10,
    "radius.lg": 18,
    shadow: true,
    colors: {
      background: c.background,
      foreground: c.foreground,
      border: c.border,
      "input.border": c.input,
      ring: c.ring,
      caret: c.foreground,
      "accent.background": c.accent,
      "accent.foreground": c["accent-foreground"],
      "muted.background": c.muted,
      "muted.foreground": c["muted-foreground"],
      "popover.background": c.popover,
      "popover.foreground": c["popover-foreground"],
      "primary.background": c.primary,
      "primary.foreground": c["primary-foreground"],
      "primary.hover.background": d["primary/90 (button hover)"],
      "primary.active.background": d["primary/90 (button hover)"],
      "secondary.background": c.secondary,
      "secondary.foreground": c["secondary-foreground"],
      "secondary.hover.background": d["secondary/90"],
      "secondary.active.background": d["secondary/80"],
      "danger.background": c.destructive,
      "danger.foreground": "#FFFFFFFF",
      "danger.hover.background": d["destructive/90"],
      "danger.active.background": d["destructive/90"],
      "info.background": c.info,
      "info.foreground": "#FFFFFFFF",
      "success.background": c.success,
      "success.foreground": "#FFFFFFFF",
      "warning.background": c.warning,
      "warning.foreground": "#FFFFFFFF",
      "button.background": dark ? d["input/32 (dark control fill)"] : c.popover,
      "button.foreground": c.foreground,
      "button.hover.background": dark ? d["input/64 (dark hover / toggle pressed)"] : d["accent/50"],
      "button.active.background": dark ? d["input/64 (dark hover / toggle pressed)"] : d["accent/50"],
      "button.primary.background": c.primary,
      "button.primary.foreground": c["primary-foreground"],
      "button.primary.hover.background": d["primary/90 (button hover)"],
      "button.primary.active.background": d["primary/90 (button hover)"],
      "button.secondary.background": c.secondary,
      "button.secondary.foreground": c["secondary-foreground"],
      "button.secondary.hover.background": d["secondary/90"],
      "button.secondary.active.background": d["secondary/80"],
      "button.danger.background": c.destructive,
      "button.danger.foreground": "#FFFFFFFF",
      "button.danger.hover.background": d["destructive/90"],
      "button.danger.active.background": d["destructive/90"],
      "list.background": T,
      "list.hover.background": c.accent,
      "list.active.background": c.accent,
      "table.background": T,
      "table.hover.background": d["muted/50"],
      "table.active.background": c.muted,
      "table.head.foreground": c.foreground,
      "table.row.border": c.border,
      "sidebar.background": c["app-sidebar-glass"],
      "sidebar.foreground": c.foreground,
      "sidebar.border": c.border,
      "sidebar.accent.background": c.accent,
      "sidebar.accent.foreground": c["accent-foreground"],
      "title_bar.background": T,
      "title_bar.border": c.border,
      "tab.background": T,
      "tab.active.background": c.accent,
      "tab.foreground": c["muted-foreground"],
      "tab.active.foreground": c.foreground,
      "tab_bar.background": T,
      "scrollbar.background": T,
      "scrollbar.thumb.background": dark ? "#FFFFFF1A" : "#00000026",
      "scrollbar.thumb.hover.background": dark ? "#FFFFFF2E" : "#00000040",
      "selection.background": dark ? "#3F638BFF" : "#007AFF4D",
      "switch.background": c.input,
      "switch.thumb.background": c.background,
      "skeleton.background": c.muted,
      "progress.bar.background": c.primary,
      link: c["info-foreground"],
      "link.hover": c["info-foreground"],
      "link.active": c["info-foreground"],
      "drag.border": d["primary/70 (composer drag border)"],
      "drop_target.background": d["accent/45"],
      overlay: d["background/60 (dialog backdrop)"],
      "window.border": c.border,
      "base.red": tw("red-500"),
      "base.green": tw("emerald-500"),
      "base.blue": tw("blue-500"),
      "base.yellow": tw("amber-500"),
      "base.magenta": tw("violet-500"),
      "base.cyan": tw("cyan-500"),
    },
  };
}

const tokens = {
  $meta: {
    source: "~/L-Projects/t3code-again (apps/web/src/index.css, components/ui/*, apps/desktop/src/window/DesktopWindow.ts)",
    generator: `docs/spec/tools/colors.mjs + docs/spec/tools/tokens.mjs (culori 4.0.2, tailwindcss 4.3.0)`,
    colorFormat: "sRGB #RRGGBBAA, channel-clipped. Alpha colors are not pre-composited.",
    desktopBreakpoint: "Window min width is 840px, so Tailwind `sm:` (>=640px) and `md:` (>=768px) values always apply on desktop. All sizes below are the sm/md values.",
    remPx: 16,
    spacingUnitPx: 4,
  },
  color: { light: L, dark: D },
  colorDerived: { light: dl, dark: dd },
  colorOverBackground: {
    light: Object.fromEntries(Object.entries(colors.light).filter(([, v]) => v.overBackground).map(([k, v]) => [k, v.overBackground])),
    dark: Object.fromEntries(Object.entries(colors.dark).filter(([, v]) => v.overBackground).map(([k, v]) => [k, v.overBackground])),
  },
  outOfSrgbGamut: Object.fromEntries(Object.entries(colors.palette).filter(([, v]) => v.outOfSrgbGamut).map(([k, v]) => [k, { css: v.css, srgbClipped: v.hex, displayP3: v.p3 }])),
  status: Object.fromEntries(Object.entries(colors.status).map(([k, v]) => [k, Object.fromEntries(Object.entries(v).map(([p, x]) => [p, x.hex]))])),
  providerAccentSwatches: colors.providerAccentSwatches.map((h) => `${h.toUpperCase()}FF`),
  providerLogoColors: {
    claude: "#D97757FF",
    openai: { light: "#000000FF", dark: "#FFFFFFFF" },
    cursor: { light: "#26251EFF", dark: "#EDECECFF" },
    grok: { light: "#0F0F0FFF", dark: "#F5F5F5FF" },
    opencode: { light: { inner: "#CFCECDFF", outer: "#211E1EFF" }, dark: { inner: "#4B4646FF", outer: "#F1ECECFF" } },
  },
  scrollbar: {
    native: { widthPx: 6, thumbRadiusPx: 3, track: T, thumb: { light: "#00000026", dark: "#FFFFFF1A" }, thumbHover: { light: "#00000040", dark: "#FFFFFF2E" } },
    modelPicker: { widthPx: 4, thumbRadiusPx: 2, thumb: { light: "#0000001A", dark: "#FFFFFF14" }, thumbHover: { light: "#00000033", dark: "#FFFFFF26" } },
    codeBlockHorizontal: { heightPx: 7, thumbRadiusPx: 999, thumb: { light: dl["border@78% (code scrollbar thumb, color-mix srgb)"], dark: dd["border@78% (code scrollbar thumb, color-mix srgb)"] } },
    scrollArea: { thicknessPx: 6, marginPx: 4, thumbRadius: "full", thumb: { light: dl["foreground/20 (ScrollArea thumb)"], dark: dd["foreground/20 (ScrollArea thumb)"] }, fadeInMs: 100, fadeOutMs: 150, fadeOutDelayMs: 300 },
  },
  terminal: { fontFamily: "SF Mono, SFMono-Regular, JetBrains Mono, Consolas, Liberation Mono, Menlo, monospace", fontSizePx: 12, lineHeight: 1, cursorBlink: true, scrollback: 5000, ansi: terminalHex },
  radius: { base: 10, sm: 6, md: 8, lg: 10, xl: 14, "2xl": 18, "3xl": 22, "4xl": 26, rounded: 4, full: 9999, composerOuter: 22, composerSurface: 20 },
  borderWidthPx: { default: 1, ultrathinkFrame: 2, disclosureStart: 2 },
  shadow: {
    "xs/5": [{ x: 0, y: 1, blur: 2, spread: 0, color: "#0000000D" }],
    "sm/5": [{ x: 0, y: 1, blur: 3, spread: 0, color: "#0000000D" }, { x: 0, y: 1, blur: 2, spread: -1, color: "#0000000D" }],
    "md/5": [{ x: 0, y: 4, blur: 6, spread: -1, color: "#0000000D" }, { x: 0, y: 2, blur: 4, spread: -2, color: "#0000000D" }],
    "lg/5": [{ x: 0, y: 10, blur: 15, spread: -3, color: "#0000000D" }, { x: 0, y: 4, blur: 6, spread: -4, color: "#0000000D" }],
    sm: [{ x: 0, y: 1, blur: 3, spread: 0, color: "#0000001A" }, { x: 0, y: 1, blur: 2, spread: -1, color: "#0000001A" }],
    md: [{ x: 0, y: 4, blur: 6, spread: -1, color: "#0000001A" }, { x: 0, y: 2, blur: 4, spread: -2, color: "#0000001A" }],
    lg: [{ x: 0, y: 10, blur: 15, spread: -3, color: "#0000001A" }, { x: 0, y: 4, blur: 6, spread: -4, color: "#0000001A" }],
    buttonPrimary: { light: [{ x: 0, y: 1, blur: 2, spread: 0, color: dl["primary/24 (default button shadow color)"] }], dark: [{ x: 0, y: 1, blur: 2, spread: 0, color: dd["primary/24 (default button shadow color)"] }], insetTop: { y: 1, color: "#FFFFFF29" }, insetTopPressed: { y: 1, color: "#00000014" } },
    composerGlass: {
      light: [{ x: 0, y: 18, blur: 48, spread: -20, color: "#00000047" }, { x: 0, y: 4, blur: 14, spread: -7, color: "#00000038" }],
      dark: [{ x: 0, y: 18, blur: 48, spread: -20, color: "#00000099" }, { x: 0, y: 4, blur: 14, spread: -7, color: "#00000066" }],
    },
    edgeBevel: { note: "::before inset 0, radius r-1, box-shadow. Light: 1px line on the bottom border row. Dark: 1px line on the top border row.", light: { y: 1, color: "#0000000A" }, dark: { y: -1, color: "#FFFFFF0F" }, darkOutlinePressed: { y: -1, color: "#FFFFFF05" } },
  },
  typography: {
    sans: { family: "DM Sans Variable", opticalSize: 14, files: "apps/web/node_modules/@fontsource-variable/dm-sans/files/dm-sans-{latin,latin-ext}-wght-normal.woff2", weightsUsed: [400, 500, 600, 700] },
    mono: { stack: ["SF Mono", "SFMono-Regular", "JetBrains Mono", "Consolas", "Liberation Mono", "Menlo", "monospace"], bundled: "JetBrains Mono 400/500 (@fontsource/jetbrains-mono 5.2.8)" },
    sizes: { xs: [12, 16], sm: [14, 20], base: [16, 24], lg: [18, 28], xl: [20, 28], "2xl": [24, 32], "3xl": [30, 36] },
    arbitrarySizesPx: [7, 8, 10, 11, 12, 13, 14, 16],
    weights: { normal: 400, medium: 500, semibold: 600, bold: 700 },
    tracking: { tight: "-0.025em", wide: "0.025em", wider: "0.05em", widest: "0.1em" },
    leading: { none: 1, tight: 1.25, snug: 1.375, normal: 1.5, relaxed: 1.625 },
  },
  motion: {
    defaultTransition: { durationMs: 150, easing: [0.4, 0, 0.2, 1] },
    workspacePanel: { durationMs: 180, easing: [0.4, 0, 0.2, 1] },
    panelViewportFade: { closeMs: 100, closeEasing: "ease-in", openMs: 140, openDelayMs: 30, openEasing: "ease-out" },
    delayedControl: { durationMs: 70, easing: "ease-out", fromScale: 0.94 },
    tooltip: { openDelayMs: 600, closeDelayMs: 0, durationMs: 150, fromScale: 0.98, sideOffsetPx: 4 },
    popover: { enterMs: 150, exit: "instant", fromScale: 0.98, sideOffsetPx: 4 },
    menu: { enter: "instant", exit: "instant", sideOffsetPx: 4, submenuAlignOffsetPx: -5 },
    dialog: { durationMs: 200, easing: [0.4, 0, 0.2, 1], fromScale: 0.98, backdropMs: 200, backdropBlurPx: 4 },
    sheet: { durationMs: 180, easing: [0.4, 0, 0.2, 1], translatePx: 32 },
    toast: { transformMs: 500, transformEasing: [0.22, 1, 0.36, 1], opacityMs: 500, heightMs: 150, timeoutMs: 5000, limit: 3, gapPx: 12, peekPx: 12, scaleStepPerIndex: 0.1 },
    switchThumb: { translateMs: 150, radiusMs: 150, scaleMs: 100, scaleDelayMs: 100, trackColorMs: 200 },
    sidebarListAutoAnimate: { durationMs: 180, easing: "ease-out" },
    timelineDisclosure: { durationMs: 180, easing: "ease-out", fromTranslateYPx: -4 },
    toolCallMorph: { durationMs: 650, easing: [0.22, 1, 0.36, 1], blurPx: 3, scale: 0.985, enterFromTranslateY: "70%", exitToTranslateY: "-60%" },
    scrollToEndPill: { durationMs: 200, easing: "ease-out", hiddenTranslateYPx: 8, hiddenScale: 0.95 },
    continuous: { spin: "1s linear infinite", pulse: "2s cubic-bezier(0.4,0,0.6,1) infinite (opacity 1 -> 0.5 -> 1)", ping: "1s cubic-bezier(0,0,0.2,1) infinite (scale 2, opacity 0 at 75%)", skeleton: "2s linear infinite, -1s delay", textShimmer: "6s linear infinite", ultrathink: "10s linear infinite (background-position + hue-rotate)", bounce: "2.4s ease-in-out infinite" },
  },
  window: {
    defaultSize: [1100, 780],
    minSize: [840, 620],
    mac: { titleBarStyle: "hiddenInset", trafficLightPosition: { x: 16, y: 18 }, transparent: true, backgroundColor: "#00000000", vibrancy: "under-window", visualEffectState: "active", disableAutoHideCursor: true },
    windowsLinux: { titleBarStyle: "hidden", titleBarOverlay: { color: "#01000000", heightPx: 40, symbolColor: { light: "#1F2937", dark: "#F8FAFC" } }, backgroundColor: { light: "#FFFFFF", dark: "#0A0A0A" } },
    title: "HOME-PC",
  },
  layout: {
    topbarHeightPx: { mac: 52, windowsLinux: 40 },
    macTrafficLightReservePx: 90,
    titlebarControlSizePx: 28,
    titlebarControlGapPx: 12,
    titlebarContentLeftWhenSidebarCollapsedPx: { mac: 130, other: 52 },
    controlsRightInsetPx: 12,
    panelToggleControlSizePx: 28,
    chatHeaderControlReservePx: 60,
    sidebar: { defaultWidthPx: 256, minWidthPx: 208, mainContentMinWidthPx: 640, railHitWidthPx: 16, collapse: "offcanvas", storageKey: "chat_thread_sidebar_width" },
    rightPanel: { defaultWidthPx: 540, minWidthPx: 360, maxWidth: "min(1400px, 70vw)", resizeHandleHitWidthPx: 8, sheetBelowWindowWidthPx: 980, storageKey: "t3code:preview-panel-width" },
    terminalDrawer: { defaultHeightPx: 280, minHeightPx: 180, maxHeight: "75vh" },
    composer: { maxWidthPx: 768, outerRadiusPx: 22, surfaceRadiusPx: 20, insetXPx: 20 },
    subheaderHeightPx: 40,
  },
  noiseOverlay: { opacity: 0.035, tilePx: 256, svgFilter: "feTurbulence type=fractalNoise baseFrequency=0.9 numOctaves=4 stitchTiles=stitch" },
  gpuiComponentTheme: {
    name: "T3 Code",
    author: "transcribed from t3code-again",
    themes: [gpuiTheme("light", L, dl), gpuiTheme("dark", D, dd)],
  },
};

fs.writeFileSync(new URL("../tokens.json", import.meta.url), `${JSON.stringify(tokens, null, 2)}\n`);
console.log("violet-500", tw("violet-500"), "cyan-500", tw("cyan-500"));
console.log("terminal", JSON.stringify(terminalHex.dark).slice(0, 200));
