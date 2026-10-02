// Compile the fork's apps/web/src/index.css with the Tailwind v4 the app installs, scanning
// apps/web/src for candidates. Writes $T3SPEC_TMP/out.css (unminified),
// out.optimized.css (lightningcss: flattens nesting and @variant blocks) and candidates.json.
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { TMP, WEB, packageVersion, requireFrom, webPackageDir } from "./fork.mjs";

const node = await import(
  pathToFileURL(path.join(webPackageDir("@tailwindcss/node"), "dist/index.mjs")).href
);
const oxide = requireFrom("@tailwindcss/oxide")(webPackageDir("@tailwindcss/oxide"));

const cssPath = path.join(WEB, "src/index.css");
const css = fs.readFileSync(cssPath, "utf8");
const compiler = await node.compile(css, { base: path.dirname(cssPath), onDependency: () => {} });

const scanner = new oxide.Scanner({
  sources: [{ base: path.join(WEB, "src"), pattern: "**/*", negated: false }],
});
const candidates = scanner.scan();
const out = compiler.build(candidates);
fs.writeFileSync(`${TMP}/out.css`, out);
const optimized = node.optimize(out, { minify: false });
fs.writeFileSync(`${TMP}/out.optimized.css`, typeof optimized === "string" ? optimized : optimized.code);
fs.writeFileSync(`${TMP}/candidates.json`, JSON.stringify(candidates.sort(), null, 0));
console.log(`tailwindcss ${packageVersion("tailwindcss")}: ${candidates.length} candidates -> ${TMP}/out.css`);
