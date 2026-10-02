#!/usr/bin/env node
// Tokenizes tests/fixtures/samples/* with the fork's Shiki + Pierre themes and writes the
// per-token colors to tests/fixtures/shiki/<sample>.<theme>.json. The `shiki_parity` test compares
// t3-highlight against these, so highlighting differences are measured instead of eyeballed.
//
// Usage: node crates/t3-highlight/tools/shiki-fixtures.mjs [~/L-Projects/t3code-again]
import { readFileSync, writeFileSync, readdirSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { join, dirname, extname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const fork = process.argv[2] ?? join(homedir(), "L-Projects/t3code-again");
const pnpm = join(fork, "node_modules/.pnpm");
const pkg = (prefix, ...rest) => {
  const dir = readdirSync(pnpm).find((name) => name.startsWith(prefix));
  if (!dir) throw new Error(`missing ${prefix} in ${pnpm}`);
  return join(pnpm, dir, "node_modules", ...rest);
};
const fixtures = join(dirname(fileURLToPath(import.meta.url)), "..", "tests/fixtures");

const { createHighlighter } = await import(
  pathToFileURL(pkg("shiki@", "shiki/dist/index.mjs")).href
);
const themeDir = pkg("@pierre+theme@", "@pierre/theme/themes");
const themes = ["pierre-dark", "pierre-light"].map((name) => ({
  ...JSON.parse(readFileSync(join(themeDir, `${name}.json`), "utf8")),
  name,
}));

// Sample extension -> the fence language a model would write.
const LANGUAGE_BY_EXTENSION = {
  ".ts": "typescript",
  ".tsx": "tsx",
  ".rs": "rust",
  ".py": "python",
  ".sh": "bash",
  ".diff": "diff",
  ".json": "json",
};

const samples = readdirSync(join(fixtures, "samples")).sort();
const highlighter = await createHighlighter({
  themes,
  langs: [...new Set(Object.values(LANGUAGE_BY_EXTENSION))],
});
mkdirSync(join(fixtures, "shiki"), { recursive: true });

for (const sample of samples) {
  const lang = LANGUAGE_BY_EXTENSION[extname(sample)];
  if (!lang) continue;
  const code = readFileSync(join(fixtures, "samples", sample), "utf8");
  for (const theme of themes) {
    const lines = highlighter
      .codeToTokensBase(code, { lang, theme: theme.name })
      .map((line) => line.map((token) => [token.content, token.color, token.fontStyle ?? 0]));
    const out = join(fixtures, "shiki", `${sample}.${theme.name}.json`);
    writeFileSync(out, `${JSON.stringify({ lang, lines })}\n`);
  }
  console.log(`${sample} (${lang})`);
}
