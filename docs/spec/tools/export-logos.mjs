#!/usr/bin/env node
// Export the custom brand/provider/editor logos (apps/web/src/components/Icons.tsx and
// JetBrainsIcons.tsx) as standalone full-color SVGs, one file per theme.
//
// Usage:
//   node docs/spec/tools/export-logos.mjs [referenceRepo] [outDir]
//   defaults: $T3_FORK (default ~/L-Projects/t3UI-refs/t3code-fork)  ->  ./assets/icons/logos
//
// How it works: esbuild (from the reference repo's node_modules) bundles a tiny entry that
// imports every exported component and renders it with react-dom/server. Tailwind classes
// that carry color (`fill-[#d97757]`, `fill-black dark:fill-white`, `dark:hidden`, ...)
// are resolved to plain SVG attributes for each theme, so the output needs no CSS.
//
// GPUI's `svg()` element paints SVGs as a single-color mask. These logos are multi-color,
// so load them with `img()` (full-color raster through resvg) instead.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const refRepo = path.resolve(process.argv[2] ?? process.env.T3_FORK ?? path.join(os.homedir(), "L-Projects/t3UI-refs/t3code-fork"));
const outDir = path.resolve(process.argv[3] ?? "assets/icons/logos");
const webDir = path.join(refRepo, "apps/web");
const pnpmDir = path.join(refRepo, "node_modules/.pnpm");
const esbuildDir = fs.readdirSync(pnpmDir).filter((d) => d.startsWith("esbuild@")).sort().at(-1);
const esbuild = createRequire(path.join(pnpmDir, esbuildDir, "node_modules/esbuild/package.json"))("esbuild");

const entry = `
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import * as Icons from "${path.join(webDir, "src/components/Icons.tsx")}";
import * as JetBrains from "${path.join(webDir, "src/components/JetBrainsIcons.tsx")}";
export const rendered = {};
for (const [name, Component] of Object.entries({ ...Icons, ...JetBrains })) {
  if (typeof Component !== "function") continue;
  rendered[name] = renderToStaticMarkup(React.createElement(Component, { width: 24, height: 24 }));
}
`;
const outfile = path.join(os.tmpdir(), `t3-logos-${process.pid}.cjs`);
await esbuild.build({
  stdin: { contents: entry, resolveDir: webDir, loader: "tsx" },
  bundle: true,
  format: "cjs",
  platform: "node",
  outfile,
  jsx: "automatic",
  alias: { "~": path.join(webDir, "src") },
  nodePaths: [path.join(webDir, "node_modules")],
  logLevel: "error",
});
const { rendered } = createRequire(import.meta.url)(outfile);
fs.rmSync(outfile);

// Resolve the color-bearing Tailwind classes this file uses into attributes.
function resolveTheme(svg, dark) {
  return svg.replace(/\sclass="([^"]*)"/g, (_, classList) => {
    const classes = classList.split(/\s+/).filter(Boolean);
    let fill = null;
    let hidden = false;
    for (const cls of classes) {
      const isDark = cls.startsWith("dark:");
      const base = isDark ? cls.slice(5) : cls;
      if (isDark && !dark) continue; // dark: variants only apply to the dark output
      const arb = base.match(/^fill-\[(#[0-9a-fA-F]{3,8})\]$/);
      if (arb) fill = arb[1];
      else if (base === "fill-black") fill = "#000000";
      else if (base === "fill-white") fill = "#ffffff";
      else if (base === "fill-none") fill = "none";
      else if (base === "hidden") hidden = true;
      else if (base === "block") hidden = false;
    }
    return `${fill ? ` fill="${fill}"` : ""}${hidden ? ' display="none"' : ""}`;
  });
}

fs.mkdirSync(outDir, { recursive: true });
let count = 0;
for (const [name, svg] of Object.entries(rendered)) {
  const withNs = svg.includes("xmlns=") ? svg : svg.replace("<svg", '<svg xmlns="http://www.w3.org/2000/svg"');
  const light = resolveTheme(withNs, false);
  const dark = resolveTheme(withNs, true);
  if (light === dark) {
    fs.writeFileSync(path.join(outDir, `${name}.svg`), `${light}\n`);
  } else {
    fs.writeFileSync(path.join(outDir, `${name}.light.svg`), `${light}\n`);
    fs.writeFileSync(path.join(outDir, `${name}.dark.svg`), `${dark}\n`);
  }
  count += 1;
}
console.log(`exported ${count} logos to ${outDir}`);
