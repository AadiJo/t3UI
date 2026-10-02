// Compile the fork's apps/web/src/index.css with Tailwind v4 (same version the app uses),
// scanning apps/web/src for candidates. Writes /tmp/t3spec/out.css (unminified) and
// /tmp/t3spec/out.optimized.css (lightningcss-optimized, which resolves color-mix where possible).
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";

const P = "/home/aadi/L-Projects/t3code-again/node_modules/.pnpm";
const node = await import(`${P}/@tailwindcss+node@4.3.0/node_modules/@tailwindcss/node/dist/index.mjs`);
const require = createRequire(import.meta.url);
// Scratch dir for generated intermediates (compiled CSS, colors.json). Override with T3SPEC_TMP.
const TMP = process.env.T3SPEC_TMP ?? "/tmp/t3spec";
fs.mkdirSync(TMP, { recursive: true });
const oxide = require(`${P}/@tailwindcss+oxide@4.3.0/node_modules/@tailwindcss/oxide/index.js`);

const webDir = "/home/aadi/L-Projects/t3code-again/apps/web";
const cssPath = path.join(webDir, "src/index.css");
const css = fs.readFileSync(cssPath, "utf8");

const compiler = await node.compile(css, {
  base: path.dirname(cssPath),
  onDependency: () => {},
});

const sources = [{ base: path.join(webDir, "src"), pattern: "**/*", negated: false }];
const scanner = new oxide.Scanner({ sources });
const candidates = scanner.scan();
console.log("candidates", candidates.length);
const out = compiler.build(candidates);
fs.writeFileSync(`${TMP}/out.css`, out);
const optimized = node.optimize(out, { minify: false });
fs.writeFileSync(`${TMP}/out.optimized.css`, typeof optimized === "string" ? optimized : optimized.code);
fs.writeFileSync(`${TMP}/candidates.json`, JSON.stringify(candidates.sort(), null, 0));
console.log("done");
