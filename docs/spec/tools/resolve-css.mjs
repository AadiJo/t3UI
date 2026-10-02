// Resolve the fork's design tokens from the compiled CSS (run compile.mjs first).
//
// Evaluates every custom property the way Chromium would for the default appearance (no theme
// preset, contrast 100) in two scopes and both modes:
//   root     <html> (`:root`, `:root:is(.dark, .dark *)`)
//   sidebar  an element with `[data-app-sidebar]` (re-declares some tokens)
// Cascade: specificity then source order; `@supports` for color-mix/oklch/lab counts as
// supported; desktop `@media (width >= ...)` up to 840px counts as matching.
// `var()` chains are substituted per scope (a property declared on the sidebar is computed
// there; anything else is inherited from the root), and `color-mix()` follows CSS Color 5
// (premultiplied alpha, percentage normalization).
//
// Also resolves the full Tailwind palette, the pre-refresh t3-ui names in legacy-colors.json
// (plus `name_NN` fields listed in T3_LEGACY_FIELDS), and collects every opacity utility the app uses (`bg-foreground/8`, `text-muted-foreground/72`,
// ...) from the compiled rules, keyed `foreground/8`.
//
// Writes $T3SPEC_TMP/resolved.json:
//   { meta, root, sidebar, alpha, legacy, palette, raw } (per mode; sidebar holds only overrides)
// Colors are sRGB `#RRGGBBAA`, channel-clipped; out-of-sRGB sources also carry `p3`.
import fs from "node:fs";
import path from "node:path";

import { TMP, WEB, forkCommit, packageVersion, requireFrom, webPackageDir } from "./fork.mjs";

const culori = requireFrom("culori")("culori");
const css = fs.readFileSync(`${TMP}/out.optimized.css`, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

// ---------------------------------------------------------------------------------- parsing
/** Index of the bracket matching the one at `open`, skipping strings. */
function matching(text, open) {
  const pairs = { "{": "}", "(": ")", "[": "]" };
  const close = pairs[text[open]];
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'") {
      i = text.indexOf(c, i + 1);
      continue;
    }
    if (c === text[open]) depth++;
    else if (c === close && --depth === 0) return i;
  }
  return text.length;
}

/** Splits at `sep` outside brackets and strings. */
function splitTop(text, sep) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'") i = text.indexOf(c, i + 1);
    else if ("({[".includes(c)) depth++;
    else if (")}]".includes(c)) depth--;
    else if (c === sep && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((p) => p.trim()).filter(Boolean);
}

const rules = [];
let order = 0;
function parse(text, conditions) {
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf("{", i);
    if (open === -1) break;
    const prelude = text.slice(i, open).trim().replace(/^[;}\s]+/, "");
    const close = matching(text, open);
    const body = text.slice(open + 1, close);
    if (prelude.startsWith("@")) {
      const [, name, cond] = prelude.match(/^@([\w-]+)\s*(.*)$/s) ?? [];
      if (["layer", "supports", "media"].includes(name)) parse(body, [...conditions, { name, cond }]);
    } else {
      const decls = [];
      for (const part of splitTop(body, ";")) {
        if (part.includes("{")) continue; // nested rules (none after lightningcss)
        const colon = part.indexOf(":");
        if (colon > 0) decls.push([part.slice(0, colon).trim(), part.slice(colon + 1).trim()]);
      }
      rules.push({ selectors: splitTop(prelude, ","), decls, conditions, order: order++ });
    }
    i = close + 1;
  }
}
parse(css, []);

/** Whether Chromium on a >= 840px desktop window applies the at-rule conditions. */
function conditionsHold(conditions) {
  return conditions.every(({ name, cond }) => {
    if (name === "layer") return true;
    if (name === "supports") return !/^not\b/.test(cond) && /color-mix|oklch|lab\(|color\(/.test(cond);
    if (name === "media") {
      const min = cond.match(/^\(width\s*>=\s*([\d.]+)rem\)$/);
      return min ? Number(min[1]) * 16 <= 840 : false;
    }
    return false;
  });
}

// --------------------------------------------------------------------------------- matching
const DARK = ":is(.dark, .dark *)";
const LIGHT_FORMS = [":not(:is(.dark, .dark *))", ":not(.dark, .dark *)"];
/** Specificity (as a sortable number) of `selector` for `scope`/`mode`, or null if no match. */
function match(selector, scope, mode) {
  let s = selector.replace(/\s+/g, " ").trim();
  let extra = 0;
  if (s.endsWith(DARK)) {
    if (mode !== "dark") return null;
    s = s.slice(0, -DARK.length);
    extra = 10;
  } else {
    for (const form of LIGHT_FORMS) {
      if (s.endsWith(form)) {
        if (mode !== "light") return null;
        s = s.slice(0, -form.length);
        extra = 10;
      }
    }
  }
  if (scope === "root") {
    if (s === ":root" || s === ":host") return 10 + extra;
    if (s === "html") return 1 + extra;
  }
  if (scope === "sidebar" && s === "[data-app-sidebar]") return 10 + extra;
  return null;
}

/** Winning declarations (name -> value) for a scope and mode. */
function declarations(scope, mode) {
  const won = new Map();
  for (const rule of rules) {
    if (!conditionsHold(rule.conditions)) continue;
    for (const selector of rule.selectors) {
      const specificity = match(selector, scope, mode);
      if (specificity === null) continue;
      for (const [name, value] of rule.decls) {
        if (!name.startsWith("--")) continue;
        const prev = won.get(name);
        if (!prev || specificity >= prev.specificity) won.set(name, { value, specificity });
      }
    }
  }
  return new Map([...won].map(([name, { value }]) => [name, value]));
}

// ------------------------------------------------------------------------------ evaluation
/** A scope computes its own declarations and inherits the rest from `parent`. */
function makeScope(decls, parent) {
  const cache = new Map();
  const scope = {
    declared: decls,
    parent,
    get(name, stack = new Set()) {
      if (!decls.has(name)) return parent ? parent.get(name) : undefined;
      if (cache.has(name)) return cache.get(name);
      if (stack.has(name)) return undefined; // cycle: invalid at computed time
      stack.add(name);
      const value = substitute(decls.get(name), (n) => scope.get(n, stack));
      stack.delete(name);
      cache.set(name, value);
      return value;
    },
  };
  return scope;
}

/** Replaces `var(--x[, fallback])` recursively; undefined if a reference cannot resolve. */
function substitute(value, lookup) {
  let out = "";
  let i = 0;
  while (i < value.length) {
    const at = value.indexOf("var(", i);
    if (at === -1) {
      out += value.slice(i);
      break;
    }
    out += value.slice(i, at);
    const close = matching(value, at + 3);
    const [name, ...fallback] = splitTop(value.slice(at + 4, close), ",");
    let resolved = lookup(name.trim());
    if (resolved === undefined && fallback.length) resolved = substitute(fallback.join(","), lookup);
    if (resolved === undefined) return undefined;
    out += resolved;
    i = close + 1;
  }
  return out;
}

const SPACES = {
  srgb: { mode: "rgb", keys: ["r", "g", "b"] },
  "srgb-linear": { mode: "lrgb", keys: ["r", "g", "b"] },
  oklab: { mode: "oklab", keys: ["l", "a", "b"] },
  lab: { mode: "lab", keys: ["l", "a", "b"] },
  oklch: { mode: "oklch", keys: ["l", "c"], hue: "h" },
  lch: { mode: "lch", keys: ["l", "c"], hue: "h" },
};

function percent(text) {
  const t = text.trim();
  const plain = t.match(/^(-?[\d.]+)%$/);
  if (plain) return Number(plain[1]);
  const calc = t.match(/^calc\((.*)\)$/s);
  if (calc) {
    // Only the forms the token sheet uses: `a% op b` / `a op b%`.
    const expr = calc[1].replace(/%/g, "");
    if (/^[\d.\s+*/()-]+$/.test(expr)) return Function(`"use strict"; return (${expr});`)();
  }
  return NaN;
}

/** Evaluates a fully substituted color expression to a culori color, or null. */
function color(text) {
  const t = text.trim();
  const mix = t.match(/^color-mix\((.*)\)$/s);
  if (mix) {
    const [space, a, b] = splitTop(mix[1], ",");
    const spaceName = space.replace(/^in\s+/, "").split(/\s+/)[0];
    const parts = [a, b].map((part) => {
      const tokens = splitTop(part, " ");
      const last = tokens.at(-1);
      if (tokens.length > 1 && /%\)?$/.test(last)) {
        return { color: color(tokens.slice(0, -1).join(" ")), p: percent(last) };
      }
      return { color: color(part), p: null };
    });
    if (parts.some((p) => !p.color)) return null;
    let [p1, p2] = parts.map((p) => p.p);
    if (p1 === null && p2 === null) [p1, p2] = [50, 50];
    else if (p1 === null) p1 = 100 - p2;
    else if (p2 === null) p2 = 100 - p1;
    const sum = p1 + p2;
    if (!(sum > 0)) return null;
    return interpolate(spaceName, parts[0].color, parts[1].color, p1 / sum, Math.min(sum, 100) / 100);
  }
  const parsed = culori.parse(t);
  return parsed ?? null;
}

function interpolate(spaceName, c1, c2, w1, alphaScale) {
  const space = SPACES[spaceName];
  if (!space) throw new Error(`color-mix space ${spaceName}`);
  const conv = culori.converter(space.mode);
  const a = conv(c1);
  const b = conv(c2);
  const alpha1 = c1.alpha ?? 1;
  const alpha2 = c2.alpha ?? 1;
  const w2 = 1 - w1;
  const alpha = alpha1 * w1 + alpha2 * w2;
  const out = { mode: space.mode, alpha: alpha * alphaScale };
  for (const k of space.keys) {
    const v = (a[k] ?? 0) * alpha1 * w1 + (b[k] ?? 0) * alpha2 * w2;
    out[k] = alpha === 0 ? 0 : v / alpha;
  }
  if (space.hue) {
    // Missing (powerless) hues take the other color's hue; then the shorter arc.
    let h1 = a.h ?? b.h ?? 0;
    let h2 = b.h ?? a.h ?? 0;
    if (h2 - h1 > 180) h1 += 360;
    else if (h1 - h2 > 180) h2 += 360;
    out.h = (h1 * w1 + h2 * w2) % 360;
  }
  return out;
}

const toRgb = culori.converter("rgb");
const toP3 = culori.converter("p3");
const clamp = (v) => Math.min(1, Math.max(0, v));
function describe(c) {
  const rgb = toRgb(c);
  const alpha = rgb.alpha ?? 1;
  const hex = culori
    .formatHex8({ mode: "rgb", r: clamp(rgb.r), g: clamp(rgb.g), b: clamp(rgb.b), alpha })
    .toUpperCase();
  const eps = 1e-4;
  const inGamut = [rgb.r, rgb.g, rgb.b].every((v) => v >= -eps && v <= 1 + eps);
  if (inGamut) return { hex };
  const p3 = toP3(rgb);
  return { hex, p3: `color(display-p3 ${[p3.r, p3.g, p3.b].map((v) => v.toFixed(4)).join(" ")})` };
}

// ---------------------------------------------------------------------------------- output
const sourceCss = fs.readFileSync(path.join(WEB, "src/index.css"), "utf8");
// Utility color names from `@theme inline` (`--color-foreground: var(--contrast-foreground)`),
// so `foreground/80` means what `bg-foreground/80` paints.
const utilityColor = new Map();
for (const m of sourceCss.matchAll(/--color-([\w-]+):\s*(var\(--[\w-]+\))\s*;/g)) utilityColor.set(m[1], m[2]);
const utilityVar = (name) => utilityColor.get(name) ?? `var(--${name})`;

// The full Tailwind palette (the theme layer only emits the tones the app uses), plus the
// fork's own additions such as zinc-25.
const themeCss = fs.readFileSync(path.join(webPackageDir("tailwindcss"), "theme.css"), "utf8");
const palette = {};
for (const m of [...themeCss.matchAll(/--color-([a-z]+)-(\d+):\s*([^;]+);/g), ...sourceCss.matchAll(/--color-([a-z]+)-(\d+):\s*(oklch\([^;]+\));/g)]) {
  (palette[m[1]] ??= {})[m[2]] = describe(color(m[3]));
}
for (const hue of Object.values(palette)) {
  const sorted = Object.entries(hue).sort(([a], [b]) => Number(a) - Number(b));
  for (const key of Object.keys(hue)) delete hue[key];
  for (const [shade, value] of sorted) hue[shade] = value;
}

const legacy = JSON.parse(fs.readFileSync(new URL("./legacy-colors.json", import.meta.url), "utf8"));
delete legacy.$comment;

const output = {
  meta: {
    fork: forkCommit(),
    tailwindcss: packageVersion("tailwindcss"),
    culori: packageVersion("culori"),
    note: "Default appearance: no theme preset, contrast 100. sRGB #RRGGBBAA, channel-clipped, not pre-composited.",
  },
  root: {},
  sidebar: {},
  alpha: {},
  legacy: {},
  palette,
  raw: {},
};

// Opacity utilities: `.bg-foreground\/8 { background-color: color-mix(in oklab, var(--x) 8%, transparent) }`.
const alphaUtilities = new Map();
const unescape = (s) => s.replace(/\\(.)/g, "$1");
for (const rule of rules) {
  if (!rule.conditions.some((c) => c.name === "supports")) continue;
  for (const selector of rule.selectors) {
    const cls = selector.match(/^\.((?:\\.|[\w-])+)/)?.[1];
    if (!cls) continue;
    const base = unescape(cls).split(":").at(-1);
    const m = base.match(/^[a-z-]+?-((?:[a-z]+-)*[a-z]+(?:-\d+)?)\/(\d+(?:\.\d+)?)$/);
    if (!m) continue;
    for (const [, value] of rule.decls) {
      const mix = value.match(/^color-mix\(in oklab, (var\(--[\w-]+\)) ([\d.]+)%, transparent\)$/);
      if (mix && mix[2] === m[2]) alphaUtilities.set(`${m[1]}/${m[2]}`, value);
    }
  }
}

/** Evaluates a CSS color expression in `scope`. */
function evaluate(expr, scope) {
  const substituted = substitute(expr, (n) => scope.get(n));
  const c = substituted && color(substituted);
  return c ? describe(c) : null;
}

/** Every color a scope paints: tokens, opacity utilities and legacy aliases. */
function colorsIn(scope, mode) {
  const tokens = {};
  const raw = {};
  const names = new Set();
  for (let s = scope; s; s = s.parent) for (const name of s.declared.keys()) names.add(name);
  for (const name of [...names].sort()) {
    const value = scope.get(name);
    if (value === undefined) continue;
    const c = color(value);
    if (c) tokens[name.slice(2)] = describe(c);
    else raw[name.slice(2)] = value;
  }
  const alpha = {};
  for (const [key, value] of [...alphaUtilities].sort()) {
    const described = evaluate(value, scope);
    if (described) alpha[key] = described;
  }
  const legacyOut = {};
  for (const [name, def] of Object.entries(legacy)) {
    const expr = typeof def === "string" ? def : def[mode];
    const described = evaluate(expr, scope);
    if (!described) throw new Error(`legacy color ${name} did not resolve`);
    legacyOut[name] = described;
  }
  return { tokens, raw, alpha, legacy: legacyOut };
}

/** Legacy `name_NN` field (not in the table) resolved as Tailwind `name/NN`. */
function legacyAlpha(fieldName, scope) {
  const m = fieldName.match(/^([a-z_]+?)_(\d+)$/);
  if (!m) return null;
  const name = m[1].replaceAll("_", "-");
  return evaluate(`color-mix(in oklab, ${utilityVar(name)} ${m[2]}%, transparent)`, scope);
}

const diff = (a, b) =>
  Object.fromEntries(Object.entries(a).filter(([k, v]) => b[k]?.hex !== v.hex));

const legacyFields = (process.env.T3_LEGACY_FIELDS ?? "").split(",").filter(Boolean);
for (const mode of ["light", "dark"]) {
  const rootScope = makeScope(declarations("root", mode), null);
  const sidebarScope = makeScope(declarations("sidebar", mode), rootScope);
  const root = colorsIn(rootScope, mode);
  const sidebar = colorsIn(sidebarScope, mode);
  // Pre-refresh `name_NN` fields still read by other crates.
  for (const field of legacyFields) {
    const key = field.replace(/_(\d+)$/, "/$1").replaceAll("_", "-");
    if (root.tokens[field.replaceAll("_", "-")] || root.alpha[key] || root.legacy[field]) continue;
    const value = legacyAlpha(field, rootScope);
    if (!value) {
      if (/_\d+$/.test(field)) console.warn(`legacy field ${field} did not resolve`);
      continue;
    }
    root.legacy[field] = value;
    sidebar.legacy[field] = legacyAlpha(field, sidebarScope);
  }
  output.root[mode] = root.tokens;
  output.raw[mode] = root.raw;
  output.alpha[mode] = root.alpha;
  output.legacy[mode] = root.legacy;
  // Only what the sidebar computes differently from the root.
  output.sidebar[mode] = {
    tokens: diff(sidebar.tokens, root.tokens),
    alpha: diff(sidebar.alpha, root.alpha),
    legacy: diff(sidebar.legacy, root.legacy),
  };
}

fs.writeFileSync(`${TMP}/resolved.json`, JSON.stringify(output, null, 1));
const count = (o) => Object.keys(o).length;
console.log(
  `fork ${output.meta.fork}: ${count(output.root.light)} tokens, ${count(output.alpha.light)} opacity utilities, ${count(output.legacy.light)} legacy aliases, ${count(palette)} palette hues; sidebar dark overrides ${count(output.sidebar.dark.tokens)}+${count(output.sidebar.dark.alpha)} -> ${TMP}/resolved.json`,
);
