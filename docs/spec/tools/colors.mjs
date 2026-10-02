// Resolve the fork's CSS color tokens (apps/web/src/index.css) to sRGB hex (#RRGGBBAA)
// for light and dark. Mirrors the browser algorithm:
//   - color-mix(): convert both inputs to the interpolation space, premultiply by alpha,
//     interpolate, un-premultiply (CSS Color 5).
//   - Tailwind v4 `--alpha(X / N%)` and `bg-x/N` compile to color-mix(in oklab, X N%, transparent).
//   - Final hex: sRGB, channel-clipped to [0,1] (what Chromium paints on an sRGB surface).
// Out-of-gamut sources are flagged with their Display-P3 value.
import { createRequire } from "node:module";
import fs from "node:fs";
const require = createRequire(import.meta.url);
// Scratch dir for generated intermediates (compiled CSS, colors.json). Override with T3SPEC_TMP.
const TMP = process.env.T3SPEC_TMP ?? "/tmp/t3spec";
fs.mkdirSync(TMP, { recursive: true });
const culori = require("/home/aadi/L-Projects/t3code-again/node_modules/.pnpm/culori@4.0.2/node_modules/culori/bundled/culori.cjs");
const { parse, converter, formatHex8, inGamut, toGamut } = culori;
const toRgb = converter("rgb");
const toOklab = converter("oklab");
const toP3 = converter("p3");

// Tailwind v4 palette values (tailwindcss@4.3.0 theme.css)
const theme = fs.readFileSync(
  "/home/aadi/L-Projects/t3code-again/node_modules/.pnpm/tailwindcss@4.3.0/node_modules/tailwindcss/theme.css",
  "utf8",
);
const palette = {};
for (const m of theme.matchAll(/--color-([a-z]+-\d+|black|white):\s*([^;]+);/g)) palette[m[1]] = m[2].trim();

const C = (name) => ({ ...toRgb(parse(palette[name])), src: palette[name], srcName: `--color-${name}` });
const transparent = { mode: "rgb", r: 0, g: 0, b: 0, alpha: 0 };
const alphaOf = (c) => (c.alpha === undefined ? 1 : c.alpha);

function mix(space, a, pa, b) {
  const pb = 1 - pa;
  const conv = space === "srgb" ? toRgb : toOklab;
  const keys = space === "srgb" ? ["r", "g", "b"] : ["l", "a", "b"];
  const A = conv(a), B = conv(b);
  const aa = alphaOf(a), ab = alphaOf(b);
  const alpha = aa * pa + ab * pb;
  const out = { mode: space === "srgb" ? "rgb" : "oklab", alpha };
  for (const k of keys) {
    const v = (A[k] ?? 0) * aa * pa + (B[k] ?? 0) * ab * pb;
    out[k] = alpha === 0 ? 0 : v / alpha;
  }
  return toRgb(out);
}
const alpha = (c, p) => mix("oklab", c, p, transparent); // Tailwind `/N` and --alpha()

function hex(c) {
  const clipped = { mode: "rgb", r: clamp(c.r), g: clamp(c.g), b: clamp(c.b), alpha: alphaOf(c) };
  return formatHex8(clipped).toUpperCase();
}
function over(fg, bg) {
  const fa = alphaOf(fg), ba = alphaOf(bg);
  const oa = fa + ba * (1 - fa);
  const ch = (k) => (clamp(fg[k]) * fa + clamp(bg[k]) * ba * (1 - fa)) / oa;
  return { mode: "rgb", r: ch("r"), g: ch("g"), b: ch("b"), alpha: oa };
}
function clamp(v) { return Math.min(1, Math.max(0, v)); }
function info(c) {
  const opaque = { mode: "rgb", r: c.r, g: c.g, b: c.b };
  const eps = 1e-4;
  const inSrgb = [c.r, c.g, c.b].every((v) => v >= -eps && v <= 1 + eps);
  const res = { hex: hex(c) };
  if (!inSrgb) {
    const p3 = toP3(opaque);
    res.outOfSrgbGamut = true;
    res.p3 = `color(display-p3 ${p3.r.toFixed(4)} ${p3.g.toFixed(4)} ${p3.b.toFixed(4)}${alphaOf(c) < 1 ? ` / ${alphaOf(c).toFixed(3)}` : ""})`;
    res.cssGamutMappedHex = formatHex8({ ...toGamut("rgb", "oklch")(opaque), alpha: alphaOf(c) }).toUpperCase();
  }
  return res;
}

const white = C("white"), black = C("black");

function buildTheme(dark) {
  const t = {};
  t.background = dark ? mix("srgb", C("neutral-950"), 0.95, white) : white;
  t["app-chrome-background"] = t.background;
  t.foreground = dark ? C("neutral-100") : C("neutral-800");
  t.card = dark ? mix("srgb", t.background, 0.98, white) : white;
  t["card-foreground"] = t.foreground;
  t.popover = dark ? mix("srgb", t.background, 0.98, white) : white;
  t["popover-foreground"] = t.foreground;
  t.primary = dark ? white : C("neutral-950");
  t["primary-foreground"] = dark ? C("neutral-950") : white;
  const wash = dark ? alpha(white, 0.04) : alpha(black, 0.04);
  t.secondary = wash;
  t["secondary-foreground"] = t.foreground;
  t.muted = wash;
  t["muted-foreground"] = mix("srgb", C("neutral-500"), 0.9, dark ? white : black);
  t.accent = wash;
  t["accent-foreground"] = t.foreground;
  t.destructive = dark ? mix("srgb", C("red-500"), 0.9, white) : C("red-500");
  t["destructive-foreground"] = dark ? C("red-400") : C("red-700");
  t.border = dark ? alpha(white, 0.06) : alpha(black, 0.08);
  t.input = dark ? alpha(white, 0.08) : alpha(black, 0.1);
  t.ring = dark ? white : C("neutral-950");
  t.info = C("blue-500");
  t["info-foreground"] = dark ? C("blue-400") : C("blue-700");
  t.success = C("emerald-500");
  t["success-foreground"] = dark ? C("emerald-400") : C("emerald-700");
  t.warning = C("amber-500");
  t["warning-foreground"] = dark ? C("amber-400") : C("amber-700");
  t["app-sidebar-glass"] = mix("srgb", t.card, dark ? 0.52 : 0.58, transparent);
  t["app-main-glass"] = mix("srgb", t.background, dark ? 0.84 : 0.88, transparent);
  return t;
}

// Derived values used by primitives / chrome. key -> fn(theme) ; `a(x,p)` = Tailwind x/p.
const a = alpha;
const derived = {
  "outline (ring/50, every element)": (t) => a(t.ring, 0.5),
  "ring/24 (input focus ring 3px)": (t) => a(t.ring, 0.24),
  "ring/70": (t) => a(t.ring, 0.7),
  "border-ring/45 (composer focus border)": (t) => a(t.ring, 0.45),
  "border/60": (t) => a(t.border, 0.6),
  "border/80": (t) => a(t.border, 0.8),
  "border@78% (code scrollbar thumb, color-mix srgb)": (t) => mix("srgb", t.border, 0.78, transparent),
  "border@60% (markdown table rule, color-mix srgb)": (t) => mix("srgb", t.border, 0.6, transparent),
  "input/32 (dark control fill)": (t) => a(t.input, 0.32),
  "input/48": (t) => a(t.input, 0.48),
  "input/64 (dark hover / toggle pressed)": (t) => a(t.input, 0.64),
  "accent/45": (t) => a(t.accent, 0.45),
  "accent/50": (t) => a(t.accent, 0.5),
  "accent/60": (t) => a(t.accent, 0.6),
  "primary/90 (button hover)": (t) => a(t.primary, 0.9),
  "primary/70 (composer drag border)": (t) => a(t.primary, 0.7),
  "primary/60 (resize handle active)": (t) => a(t.primary, 0.6),
  "primary/40": (t) => a(t.primary, 0.4),
  "primary/24 (default button shadow color)": (t) => a(t.primary, 0.24),
  "secondary/90": (t) => a(t.secondary, 0.9),
  "secondary/80": (t) => a(t.secondary, 0.8),
  "destructive/90": (t) => a(t.destructive, 0.9),
  "destructive/4": (t) => a(t.destructive, 0.04),
  "destructive/8": (t) => a(t.destructive, 0.08),
  "destructive/16": (t) => a(t.destructive, 0.16),
  "destructive/24": (t) => a(t.destructive, 0.24),
  "destructive/32": (t) => a(t.destructive, 0.32),
  "destructive/36": (t) => a(t.destructive, 0.36),
  "destructive/48": (t) => a(t.destructive, 0.48),
  "destructive/64": (t) => a(t.destructive, 0.64),
  "info/4": (t) => a(t.info, 0.04), "info/8": (t) => a(t.info, 0.08), "info/16": (t) => a(t.info, 0.16), "info/32": (t) => a(t.info, 0.32),
  "success/4": (t) => a(t.success, 0.04), "success/8": (t) => a(t.success, 0.08), "success/16": (t) => a(t.success, 0.16), "success/32": (t) => a(t.success, 0.32),
  "warning/4": (t) => a(t.warning, 0.04), "warning/8": (t) => a(t.warning, 0.08), "warning/16": (t) => a(t.warning, 0.16), "warning/32": (t) => a(t.warning, 0.32),
  "muted/40": (t) => a(t.muted, 0.4),
  "muted/50": (t) => a(t.muted, 0.5),
  "muted/60": (t) => a(t.muted, 0.6),
  "muted/72 (dialog footer, command bg)": (t) => a(t.muted, 0.72),
  "muted-foreground/50": (t) => a(t["muted-foreground"], 0.5),
  "muted-foreground/60": (t) => a(t["muted-foreground"], 0.6),
  "muted-foreground/70": (t) => a(t["muted-foreground"], 0.7),
  "muted-foreground/72 (placeholder, shortcuts)": (t) => a(t["muted-foreground"], 0.72),
  "muted-foreground/80": (t) => a(t["muted-foreground"], 0.8),
  "foreground/4 (autofill light)": (t) => a(t.foreground, 0.04),
  "foreground/8": (t) => a(t.foreground, 0.08),
  "foreground/20 (ScrollArea thumb)": (t) => a(t.foreground, 0.2),
  "foreground@8% (code chrome pressed, color-mix srgb)": (t) => mix("srgb", t.foreground, 0.08, transparent),
  "background/60 (dialog backdrop)": (t) => a(t.background, 0.6),
  "background/95": (t) => a(t.background, 0.95),
  "popover/92 (toast close orb)": (t) => a(t.popover, 0.92),
  "composer glass bg (card 20% light / 45% dark, srgb)": (t, dark) => mix("srgb", t.card, dark ? 0.45 : 0.2, transparent),
  "codeblock bg (muted 78% + background, srgb)": (t) => mix("srgb", t.muted, 0.78, t.background),
  "diff panel viewport (background 94% + card)": (t) => mix("srgb", t.background, 0.94, t.card),
  "diff file surface (card 92% + background)": (t) => mix("srgb", t.card, 0.92, t.background),
  "inset highlight white/16 (solid buttons)": () => a(white, 0.16),
  "inset press black/8": () => a(black, 0.08),
  "control top-edge shadow black/4 (light ::before)": () => a(black, 0.04),
  "control top-edge highlight white/6 (dark ::before)": () => a(white, 0.06),
  "control top-edge highlight white/2 (dark ::before, outline btn)": () => a(white, 0.02),
  "group-text before black/6": () => a(black, 0.06),
  "skeleton highlight (white/64 light, white/4 dark)": (t, dark) => a(white, dark ? 0.04 : 0.64),
};

const out = { meta: {
  generatedBy: `docs/spec/tools/colors.mjs (culori 4.0.2, tailwindcss 4.3.0 palette)`,
  note: "Hex is sRGB #RRGGBBAA, channel-clipped. Alpha colors are NOT pre-composited; composite over the surface below.",
}, palette: {}, light: {}, dark: {}, derived: { light: {}, dark: {} } };

const usedPalette = ["white","black","neutral-100","neutral-500","neutral-800","neutral-950","red-400","red-500","red-700","blue-400","blue-500","blue-700","emerald-400","emerald-500","emerald-700","amber-400","amber-500","amber-700"];
for (const n of usedPalette) out.palette[n] = { css: palette[n], ...info(C(n)) };

for (const [mode, dark] of [["light", false], ["dark", true]]) {
  const t = buildTheme(dark);
  for (const [k, v] of Object.entries(t)) out[mode][k] = { ...info(v), ...(alphaOf(v) < 1 ? { overBackground: hex(over(v, t.background)) } : {}) };
  for (const [k, fn] of Object.entries(derived)) { const v = fn(t, dark); out.derived[mode][k] = { ...info(v), ...(alphaOf(v) < 1 ? { overBackground: hex(over(v, t.background)) } : {}) }; }
}
fs.writeFileSync(`${TMP}/colors.json`, JSON.stringify(out, null, 1));

// Pretty table
const keys = Object.keys(out.light);
console.log("| token | light | dark |");
for (const k of keys) {
  const l = out.light[k], d = out.dark[k];
  const f = (x) => x.hex + (x.outOfSrgbGamut ? "*" : "") + (x.overBackground ? ` (on bg ${x.overBackground.slice(0,7)})` : "");
  console.log(`| --${k} | ${f(l)} | ${f(d)} |`);
}
console.log("\n| derived | light | dark |");
for (const k of Object.keys(derived)) {
  const l = out.derived.light[k], d = out.derived.dark[k];
  const f = (x) => x.hex + (x.outOfSrgbGamut ? "*" : "") + (x.overBackground ? ` (on bg ${x.overBackground.slice(0,7)})` : "");
  console.log(`| ${k} | ${f(l)} | ${f(d)} |`);
}
console.log("\npalette");
for (const [k, v] of Object.entries(out.palette)) console.log(k, v.css, v.hex, v.outOfSrgbGamut ? `OOG p3=${v.p3} mapped=${v.cssGamutMappedHex}` : "");

// ---- Status palette used by sidebar thread / PR / terminal indicators (Sidebar.logic.ts, ThreadStatusIndicators.tsx)
const status = {
  "working (sky)": { light: { text: C("sky-600"), dot: C("sky-500") }, dark: { text: alpha(C("sky-300"), 0.8), dot: alpha(C("sky-300"), 0.8) } },
  "pending-approval (amber)": { light: { text: C("amber-600"), dot: C("amber-500") }, dark: { text: alpha(C("amber-300"), 0.9), dot: alpha(C("amber-300"), 0.9) } },
  "awaiting-input (indigo)": { light: { text: C("indigo-600"), dot: C("indigo-500") }, dark: { text: alpha(C("indigo-300"), 0.9), dot: alpha(C("indigo-300"), 0.9) } },
  "error (rose)": { light: { text: C("rose-600"), dot: C("rose-500") }, dark: { text: alpha(C("rose-300"), 0.9), dot: alpha(C("rose-300"), 0.9) } },
  "plan-ready (violet)": { light: { text: C("violet-600"), dot: C("violet-500") }, dark: { text: alpha(C("violet-300"), 0.9), dot: alpha(C("violet-300"), 0.9) } },
  "completed / pr-open (emerald)": { light: { text: C("emerald-600"), dot: C("emerald-500") }, dark: { text: alpha(C("emerald-300"), 0.9), dot: alpha(C("emerald-300"), 0.9) } },
  "pr-closed (zinc)": { light: { text: C("zinc-500") }, dark: { text: alpha(C("zinc-400"), 0.8) } },
  "pr-merged (violet)": { light: { text: C("violet-600") }, dark: { text: alpha(C("violet-300"), 0.9) } },
  "terminal-running (teal)": { light: { text: C("teal-600") }, dark: { text: alpha(C("teal-300"), 0.9) } },
};
const statusOut = {};
for (const [k, v] of Object.entries(status)) {
  statusOut[k] = {};
  for (const mode of ["light", "dark"]) for (const [part, col] of Object.entries(v[mode])) statusOut[k][`${mode}.${part}`] = info(col);
}
const all = JSON.parse(fs.readFileSync(`${TMP}/colors.json`, "utf8"));
all.status = statusOut;
all.providerAccentSwatches = ["#2563eb", "#16a34a", "#ea580c", "#dc2626", "#7c3aed", "#0891b2"];
fs.writeFileSync(`${TMP}/colors.json`, JSON.stringify(all, null, 1));
console.log("\nstatus");
for (const [k, v] of Object.entries(statusOut)) console.log(k, Object.entries(v).map(([p, x]) => `${p}=${x.hex}`).join(" "));
