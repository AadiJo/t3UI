#!/usr/bin/env node
// Export every lucide-react icon imported by the reference web UI as standalone SVG files.
//
// Usage:
//   node docs/spec/tools/export-icons.mjs [referenceRepo] [outDir]
//   defaults: ~/L-Projects/t3code-again  ->  ./assets/icons/lucide
//
// How it works:
//   1. Scans <ref>/apps/web/src for `import { ... } from "lucide-react"` (tests excluded).
//   2. Resolves each imported name (aliases like Loader2, AlertTriangle, Globe2) to its
//      canonical icon file through lucide-react's own ESM barrel.
//   3. Serialises the icon's `__iconNode` with lucide's default attributes
//      (24x24 viewBox, fill none, stroke currentColor, stroke-width 2, round caps/joins).
//   Writes `<canonical-name>.svg` plus `manifest.json` (import name -> file, usage counts).
//
// The SVGs use `currentColor`, so GPUI can tint them with `svg().text_color(...)`.
// Stroke width is baked at 2 (lucide default). A few call sites override it
// (e.g. toast chevrons use strokeWidth 2.25); see design-system.md "Icons".
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const refRepo = path.resolve(process.argv[2] ?? path.join(os.homedir(), "L-Projects/t3code-again"));
const outDir = path.resolve(process.argv[3] ?? "assets/icons/lucide");
const webSrc = path.join(refRepo, "apps/web/src");
const lucideRoot = path.join(refRepo, "apps/web/node_modules/lucide-react/dist/esm");

const files = [];
(function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (/\.(tsx?|jsx?)$/.test(entry.name) && !/\.(test|browser|spec)\./.test(entry.name)) files.push(full);
  }
})(webSrc);

// import name -> { files: Set }
const used = new Map();
const importRe = /import\s+\{([^}]*)\}\s+from\s+["']lucide-react["']/g;
for (const file of files) {
  const src = fs.readFileSync(file, "utf8");
  for (const match of src.matchAll(importRe)) {
    for (const raw of match[1].split(",")) {
      const name = raw.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0]?.trim();
      if (!name || name === "LucideIcon" || name === "LucideProps") continue;
      const entry = used.get(name) ?? { files: new Set() };
      entry.files.add(path.relative(webSrc, file));
      used.set(name, entry);
    }
  }
}

// Map export name -> icon file using the barrel's `export { default as X, ... } from './icons/x.js'` lines.
const barrel = fs.readFileSync(path.join(lucideRoot, "lucide-react.js"), "utf8");
const exportToFile = new Map();
for (const line of barrel.matchAll(/export \{([^}]*)\} from '\.\/icons\/([^']+)\.js'/g)) {
  for (const part of line[1].split(",")) {
    const alias = part.trim().split(/\s+as\s+/)[1];
    if (alias) exportToFile.set(alias, line[2]);
  }
}

const attrs = (obj) =>
  Object.entries(obj)
    .filter(([key]) => key !== "key")
    .map(([key, value]) => `${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}="${value}"`)
    .join(" ");

fs.mkdirSync(outDir, { recursive: true });
const manifest = {};
for (const [name, { files: usedIn }] of [...used].sort()) {
  const canonical = exportToFile.get(name);
  if (!canonical) {
    console.warn(`unresolved lucide import: ${name}`);
    continue;
  }
  const mod = await import(pathToFileURL(path.join(lucideRoot, "icons", `${canonical}.js`)).href);
  const body = mod.__iconNode.map(([tag, a]) => `  <${tag} ${attrs(a)}/>`).join("\n");
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" ` +
    `stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">\n${body}\n</svg>\n`;
  fs.writeFileSync(path.join(outDir, `${canonical}.svg`), svg);
  manifest[name] = { file: `${canonical}.svg`, files: usedIn.size };
}
fs.writeFileSync(path.join(outDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`exported ${Object.keys(manifest).length} imports (${new Set(Object.values(manifest).map((m) => m.file)).size} unique SVGs) to ${outDir}`);
