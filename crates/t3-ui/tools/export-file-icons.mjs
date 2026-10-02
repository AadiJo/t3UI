#!/usr/bin/env node
// Export the file-tree file-type icons the reference UI shows (Pierre "complete" set plus the
// T3 overrides in apps/web/src/pierre-icons.ts) as standalone SVGs, with a manifest that
// t3-ui's gen_icons.py turns into a Rust resolver.
//
// Usage (from the repo root):
//   node crates/t3-ui/tools/export-file-icons.mjs [referenceRepo] [outDir]
//   defaults: $T3_FORK (default ~/L-Projects/t3UI-refs/t3code-fork)  ->  ./assets/icons/files
//
// Output:
//   <token>.svg        Pierre built-ins. Single-color (`currentColor` with opacity layers), so
//                      render with `svg()` tinted by the token's color.
//   t3-<name>.svg      T3 overrides. Multi-color, render with `img()`. Icons that use
//                      `currentColor` get `.light.svg` / `.dark.svg` variants with the
//                      foreground color baked in.
//   manifest.json      { fileNames, extensions, completeExtensionOverrides, tokenColors,
//                        palette, t3FileNames }
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const refRepo = path.resolve(process.argv[2] ?? process.env.T3_FORK ?? path.join(os.homedir(), "L-Projects/t3UI-refs/t3code-fork"));
const outDir = path.resolve(process.argv[3] ?? "assets/icons/files");
const treesDist = path.join(refRepo, "apps/web/node_modules/@pierre/trees/dist");

// builtInIcons.js keeps its lookup tables private; re-export them from a scratch copy.
const builtInSrc = fs.readFileSync(path.join(treesDist, "builtInIcons.js"), "utf8");
const scratch = path.join(os.tmpdir(), `pierre-builtins-${process.pid}.mjs`);
fs.writeFileSync(
  scratch,
  `${builtInSrc}\nexport { BUILT_IN_FILE_NAME_TOKENS, BUILT_IN_FILE_EXTENSION_TOKENS, COMPLETE_EXTENSION_OVERRIDES };\n`,
);
const builtIns = await import(pathToFileURL(scratch).href);
fs.rmSync(scratch);

fs.rmSync(outDir, { recursive: true, force: true });
fs.mkdirSync(outDir, { recursive: true });

const symbolRe = /<symbol id="([^"]+)" viewBox="([^"]+)">([\s\S]*?)<\/symbol>/g;
const writeSvg = (file, viewBox, body) =>
  fs.writeFileSync(
    path.join(outDir, file),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}">${body.trim()}</svg>\n`,
  );

// Pierre built-ins: file-tree-builtin-<token>.
const sprite = builtIns.getBuiltInSpriteSheet("complete");
const tokens = [];
for (const [, id, viewBox, body] of sprite.matchAll(symbolRe)) {
  if (!id.startsWith("file-tree-builtin-")) continue;
  const token = id.slice("file-tree-builtin-".length);
  tokens.push(token);
  writeSvg(`${token}.svg`, viewBox, body);
}

// T3 overrides from pierre-icons.ts.
const t3Src = fs.readFileSync(path.join(refRepo, "apps/web/src/pierre-icons.ts"), "utf8");
const t3Sprite = t3Src.match(/const T3_FILE_ICON_SPRITE = `([\s\S]*?)`;/)[1];
// `currentColor` inside the file tree is the tree foreground (`--foreground`).
const specTokens = JSON.parse(fs.readFileSync(new URL("../../../docs/spec/tokens.json", import.meta.url), "utf8"));
const FOREGROUND = { light: specTokens.color.light.foreground.slice(0, 7), dark: specTokens.color.dark.foreground.slice(0, 7) };
const t3Icons = {};
for (const [, id, viewBox, body] of t3Sprite.matchAll(symbolRe)) {
  const name = id.replace(/^t3-file-icon-/, "t3-");
  if (/currentColor/i.test(body)) {
    for (const mode of ["light", "dark"]) {
      writeSvg(`${name}.${mode}.svg`, viewBox, body.replace(/currentColor/gi, FOREGROUND[mode]));
    }
    t3Icons[id] = { name, themed: true };
  } else {
    writeSvg(`${name}.svg`, viewBox, body);
    t3Icons[id] = { name, themed: false };
  }
}
const t3FileNames = {};
// File-name overrides that point at a Pierre built-in instead of a T3 symbol.
const fileNameOverrides = {};
const byFileName = t3Src.match(/byFileName: \{([\s\S]*?)\}/)[1];
for (const [, fileName, id] of byFileName.matchAll(/"([^"]+)": "([^"]+)"/g)) {
  if (id.startsWith("t3-file-icon-")) t3FileNames[fileName] = t3Icons[id];
  else fileNameOverrides[fileName] = id.replace(/^file-tree-builtin-/, "");
}
// Extension overrides: `byFileExtension` maps the shared video extensions to the T3 video icon.
const t3Extensions = {};
if (/VIDEO_FILE_EXTENSIONS\.map\(\(extension\) => \[extension, "t3-file-icon-video"\]\)/.test(t3Src)) {
  const video = fs.readFileSync(path.join(refRepo, "packages/shared/src/video.ts"), "utf8");
  for (const [, extension] of video.matchAll(/\["([a-z0-9]+)", "video\/[^"]+"\]/g)) {
    t3Extensions[extension] = t3Icons["t3-file-icon-video"];
  }
}

// Token colors: --trees-file-icon-color-<token> -> --trees-icon-<palette> -> light-dark(a, b).
const style = fs.readFileSync(path.join(treesDist, "style.js"), "utf8");
const palette = {};
for (const [, name, light, dark] of style.matchAll(
  /--trees-icon-([a-z]+): light-dark\((#[0-9a-fA-F]{6}), (#[0-9a-fA-F]{6})\)/g,
)) {
  palette[name] = [light.toUpperCase(), dark.toUpperCase()];
}
const tokenColors = {};
for (const [, token, color] of style.matchAll(
  /--trees-file-icon-color-([a-z0-9-]+): var\(\\n\s*--trees-file-icon-[a-z]+,\\n\s*var\(--trees-icon-([a-z]+)\)/g,
)) {
  tokenColors[token] = color;
}

const manifest = {
  fileNames: { ...builtIns.BUILT_IN_FILE_NAME_TOKENS, ...fileNameOverrides },
  extensions: builtIns.BUILT_IN_FILE_EXTENSION_TOKENS,
  completeExtensionOverrides: builtIns.COMPLETE_EXTENSION_OVERRIDES,
  tokenColors,
  palette,
  t3FileNames,
  t3Extensions,
};
fs.writeFileSync(path.join(outDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(
  `exported ${tokens.length} Pierre icons and ${Object.keys(t3Icons).length} T3 icons to ${outDir}`,
);
const uncolored = tokens.filter((t) => !tokenColors[t]);
if (uncolored.length) console.log(`tokens without a color (use muted): ${uncolored.join(", ")}`);
