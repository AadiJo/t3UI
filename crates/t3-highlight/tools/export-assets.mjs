#!/usr/bin/env node
// Regenerates crates/t3-highlight/assets from the reference fork (read-only). Needs node + cargo.
//   - pierre-dark.tmTheme / pierre-light.tmTheme: the Shiki (VS Code JSON) themes the fork's
//     code blocks use, converted to TextMate plist so syntect can load them.
//   - languages.tsv: Shiki language ids + aliases (fence info strings) and the
//     extension -> language map @pierre/diffs uses for file paths.
//   - grammars/*.sublime-syntax: Shiki's TextMate grammars for a few languages, translated by
//     tools/grammars (syntect-tmlanguage) so syntect tokenizes them like Shiki.
//
// Usage: node crates/t3-highlight/tools/export-assets.mjs [~/L-Projects/t3code-again]
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, readdirSync, mkdtempSync, rmSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const fork = process.argv[2] ?? join(homedir(), "L-Projects/t3code-again");
const out = join(dirname(fileURLToPath(import.meta.url)), "..", "assets");
const pnpm = join(fork, "node_modules/.pnpm");
const pkgDir = (prefix, ...rest) => {
  const dir = readdirSync(pnpm).find((name) => name.startsWith(prefix));
  if (!dir) throw new Error(`missing ${prefix} in ${pnpm}`);
  return join(pnpm, dir, "node_modules", ...rest);
};

// --- Themes -----------------------------------------------------------------

const escapeXml = (value) =>
  String(value).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

function dict(entries, indent) {
  const pad = "\t".repeat(indent);
  return [
    `${pad}<dict>`,
    ...entries.flatMap(([key, value]) => [
      `${pad}\t<key>${escapeXml(key)}</key>`,
      typeof value === "string" ? `${pad}\t<string>${escapeXml(value)}</string>` : value(indent + 1),
    ]),
    `${pad}</dict>`,
  ].join("\n");
}

// Grammars that stay on bat's Sublime definitions name some scopes differently from the VS Code
// grammars Shiki runs. Each alias gives a bat selector the style Pierre assigns to the VS Code
// scope Shiki emits for the same token (measured with tests/shiki_parity.rs).
const SCOPE_ALIASES = [
  // Bash (bat) -> shellscript (Shiki)
  ["meta.function-call.arguments.shell", "string.unquoted.argument.shell"],
  ["variable.parameter.option.shell", "constant.other.option.shell"],
  ["variable.parameter.option.shell punctuation.definition.parameter.shell", "constant.other.option.shell"],
  ["support.function.double-brace.begin.shell, support.function.double-brace.end.shell", "punctuation.definition.logical-expression.shell"],
  ["meta.group.expansion.parameter.shell keyword.operator.assignment.shell", "punctuation.section.parameter.shell"],
  ["meta.group.expansion.parameter.shell", "variable.other.normal.shell"],
  // Control operators and redirections (`&&`, `|`, `;`, `2>&1`) are plain operator gray.
  [
    "keyword.operator.logical.and.shell, keyword.operator.logical.or.shell, keyword.operator.logical.continue.shell, keyword.operator.logical.pipe.shell, keyword.operator.logical.job.shell",
    "keyword.operator.pipe.shell",
  ],
  ["keyword.operator.assignment.redirection.shell", "keyword.operator.redirect.shell"],
  ["constant.numeric.integer.decimal.file-descriptor.shell", "keyword.operator.redirect.shell"],
];

// The settings VS Code would resolve for a single scope: the longest matching single-scope
// selector wins, and the later rule wins a tie.
function resolveScope(theme, scope) {
  let best = null;
  let bestLength = -1;
  for (const rule of theme.tokenColors) {
    const selectors = Array.isArray(rule.scope) ? rule.scope : String(rule.scope ?? "").split(",");
    for (const selector of selectors.map((value) => value.trim())) {
      if (!selector || selector.includes(" ")) continue;
      if (scope !== selector && !scope.startsWith(`${selector}.`)) continue;
      if (selector.length >= bestLength) {
        bestLength = selector.length;
        best = rule.settings;
      }
    }
  }
  if (!best) throw new Error(`no theme rule matches ${scope}`);
  return best;
}

function themeSettings(settings) {
  const entries = [];
  if (settings.foreground) entries.push(["foreground", settings.foreground]);
  if (settings.background) entries.push(["background", settings.background]);
  // "normal" must stay explicit: it resets an inherited italic in TextMate resolution.
  if (settings.fontStyle !== undefined) {
    entries.push(["fontStyle", settings.fontStyle === "normal" ? "" : settings.fontStyle]);
  }
  return entries;
}

function convertTheme(name) {
  const themePath = pkgDir("@pierre+theme@", "@pierre/theme/themes", `${name}.json`);
  const theme = JSON.parse(readFileSync(themePath, "utf8"));
  const rules = [
    dict(
      [
        [
          "settings",
          (indent) =>
            dict(
              [
                ["background", theme.colors["editor.background"]],
                ["foreground", theme.colors["editor.foreground"]],
              ],
              indent,
            ),
        ],
      ],
      2,
    ),
  ];
  const pushRule = (scope, settings) =>
    rules.push(
      dict(
        [
          ["scope", scope],
          ["settings", (indent) => dict(settings, indent)],
        ],
        2,
      ),
    );
  for (const [selector, shikiScope] of SCOPE_ALIASES) {
    pushRule(selector, themeSettings(resolveScope(theme, shikiScope)));
  }
  // VS Code lets the later of two equally specific rules win; syntect keeps the first. Emitting
  // the rules in reverse order makes syntect resolve ties the way Shiki does.
  for (const rule of [...theme.tokenColors].reverse()) {
    if (!rule.scope) continue;
    const scope = Array.isArray(rule.scope) ? rule.scope.join(", ") : rule.scope;
    const settings = themeSettings(rule.settings);
    rules.push(
      dict(
        [
          ["scope", scope],
          ["settings", (indent) => dict(settings, indent)],
        ],
        2,
      ),
    );
  }
  const xml = [
    `<?xml version="1.0" encoding="UTF-8"?>`,
    `<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">`,
    `<!-- Generated by crates/t3-highlight/tools/export-assets.mjs from @pierre/theme ${name}.json. Do not edit. -->`,
    `<plist version="1.0">`,
    `<dict>`,
    `\t<key>name</key>`,
    `\t<string>${name}</string>`,
    `\t<key>settings</key>`,
    `\t<array>`,
    rules.join("\n"),
    `\t</array>`,
    `</dict>`,
    `</plist>`,
    ``,
  ].join("\n");
  writeFileSync(join(out, `${name}.tmTheme`), xml);
  console.log(`${name}.tmTheme: ${rules.length - 1} rules`);
}

convertTheme("pierre-dark");
convertTheme("pierre-light");

// --- Languages --------------------------------------------------------------

const shikiLangs = await import(pathToFileURL(pkgDir("shiki@", "shiki/dist/langs.mjs")).href);
const diffsDir = readdirSync(pnpm).find((name) => name.startsWith("@pierre+diffs@"));
const filetypes = await import(
  pathToFileURL(
    join(pnpm, diffsDir, "node_modules/@pierre/diffs/dist/utils/getFiletypeFromFileName.js"),
  ).href
);

const rows = ["# kind\tkey\tshiki-language (generated by tools/export-assets.mjs; do not edit)"];
for (const info of shikiLangs.bundledLanguagesInfo) {
  rows.push(`alias\t${info.id}\t${info.id}`);
  for (const alias of info.aliases ?? []) rows.push(`alias\t${alias}\t${info.id}`);
}
for (const [extension, language] of Object.entries(filetypes.EXTENSION_TO_FILE_FORMAT)) {
  rows.push(`ext\t${extension}\t${language}`);
}
writeFileSync(join(out, "languages.tsv"), `${rows.join("\n")}\n`);
console.log(`languages.tsv: ${rows.length - 1} rows`);

// --- Grammars ---------------------------------------------------------------
// Shiki's own (VS Code) grammars for the languages where bat's Sublime grammars scope tokens
// differently enough to change colors (measured by tests/shiki_parity.rs). TypeScript/TSX stay on
// bat's grammars, which are generated from the same source and already match.

// shellscript stays on bat: its grammar leans on `\G` begin rules ("only where the parent began"),
// which syntect cannot express, so its command-name context would reopen on every argument.
const SHIKI_GRAMMARS = ["rust", "python", "json", "diff"];

// Rewrites an Oniguruma pattern into one fancy-regex (syntect's pure-Rust engine) accepts:
//   - `\G` ("where the last match ended") has no syntect equivalent. Dropping it lets those
//     patterns match at the same spot in the cases these grammars use it (right after a begin).
//   - `\N` (not a newline), `\h`/`\H` (hex digit), `\Z`, `\uXXXX`, `\p{^X}` and POSIX
//     properties (`\p{alnum}`) are spelled the
//     way the Rust regex syntax expects.
function onigToFancy(pattern) {
  let out = "";
  let classDepth = 0;
  for (let i = 0; i < pattern.length; i += 1) {
    const ch = pattern[i];
    if (ch === "\\") {
      const next = pattern[i + 1] ?? "";
      i += 1;
      if (next === "G") continue;
      if (next === "N") out += classDepth > 0 ? "\\n" : "[^\\n]";
      else if (next === "h") out += classDepth > 0 ? "0-9a-fA-F" : "[0-9a-fA-F]";
      else if (next === "H") out += classDepth > 0 ? "\\x00-/:-@G-`g-\\x{10FFFF}" : "[^0-9a-fA-F]";
      else if (next === "Z") out += "(?=\\n?\\z)";
      else if (next === "u" && /^[0-9a-fA-F]{4}$/.test(pattern.slice(i + 1, i + 5))) {
        out += `\\x{${pattern.slice(i + 1, i + 5)}}`;
        i += 4;
      } else if (next === "p" && /^\{(alnum|alpha|upper|lower|digit|space|word|punct|xdigit)\}/.test(pattern.slice(i + 1))) {
        // Oniguruma's POSIX-bracket properties, e.g. `\p{alnum}` -> `[[:alnum:]]`.
        const name = /^\{(\w+)\}/.exec(pattern.slice(i + 1))[1];
        out += classDepth > 0 ? `[:${name}:]` : `[[:${name}:]]`;
        i += name.length + 2;
      } else if ((next === "p" || next === "P") && pattern.slice(i + 1, i + 3) === "{^") {
        out += next === "p" ? "\\P{" : "\\p{";
        i += 2;
      } else out += `\\${next}`;
      continue;
    }
    if (ch === "[") classDepth += 1;
    else if (ch === "]" && classDepth > 0) classDepth -= 1;
    out += ch;
  }
  return out;
}
// Index of the `)` closing the group that opens at `open`, skipping escapes and classes.
function groupEnd(pattern, open) {
  let depth = 0;
  let inClass = 0;
  for (let i = open; i < pattern.length; i += 1) {
    const ch = pattern[i];
    if (ch === "\\") i += 1;
    else if (ch === "[") inClass += 1;
    else if (ch === "]" && inClass > 0) inClass -= 1;
    else if (inClass > 0) continue;
    else if (ch === "(") depth += 1;
    else if (ch === ")" && (depth -= 1) === 0) return i;
  }
  throw new Error(`unbalanced group in ${pattern}`);
}

// Splits `body` at top-level `|`.
function topLevelAlternatives(body) {
  const parts = [];
  let depth = 0;
  let inClass = 0;
  let start = 0;
  for (let i = 0; i < body.length; i += 1) {
    const ch = body[i];
    if (ch === "\\") i += 1;
    else if (ch === "[") inClass += 1;
    else if (ch === "]" && inClass > 0) inClass -= 1;
    else if (inClass > 0) continue;
    else if (ch === "(") depth += 1;
    else if (ch === ")") depth -= 1;
    else if (ch === "|" && depth === 0) {
      parts.push(body.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(body.slice(start));
  return parts;
}

// Expands `prefix(?:a|b)suffix` into its alternatives, repeatedly, so every alternative is a
// plain sequence. Groups that are not `(?:` alternations are left alone.
function expandAlternatives(alternative) {
  let i = alternative.indexOf("(?:");
  while (i !== -1) {
    if (i > 0 && alternative[i - 1] === "\\") {
      i = alternative.indexOf("(?:", i + 1);
      continue;
    }
    const end = groupEnd(alternative, i);
    const quantified = /^[*+?{]/.test(alternative.slice(end + 1));
    const inner = topLevelAlternatives(alternative.slice(i + 3, end));
    if (inner.length > 1 && !quantified) {
      const prefix = alternative.slice(0, i);
      const suffix = alternative.slice(end + 1);
      return inner.flatMap((choice) => expandAlternatives(prefix + choice + suffix));
    }
    i = alternative.indexOf("(?:", end);
  }
  return [alternative];
}

// Oniguruma accepts lookbehinds whose alternatives differ in length; fancy-regex needs each
// lookbehind to be fixed-size. `(?<=a|bc)` becomes `(?:(?<=a)|(?<=bc))` and `(?<!a|bc)` becomes
// `(?<!a)(?<!bc)`, which match exactly the same positions.
function splitLookbehinds(pattern) {
  let out = "";
  let i = 0;
  while (i < pattern.length) {
    const ch = pattern[i];
    if (ch === "\\") {
      out += pattern.slice(i, i + 2);
      i += 2;
      continue;
    }
    if (ch === "[") {
      // Copy a character class verbatim (it cannot contain a lookbehind).
      let j = i + 1;
      let depth = 1;
      while (j < pattern.length && depth > 0) {
        if (pattern[j] === "\\") j += 1;
        else if (pattern[j] === "[") depth += 1;
        else if (pattern[j] === "]") depth -= 1;
        j += 1;
      }
      out += pattern.slice(i, j);
      i = j;
      continue;
    }
    const kind = pattern.slice(i, i + 4);
    if (kind === "(?<=" || kind === "(?<!") {
      const end = groupEnd(pattern, i);
      const body = splitLookbehinds(pattern.slice(i + 4, end));
      const alternatives = topLevelAlternatives(body).flatMap(expandAlternatives);
      if (alternatives.length === 1) out += `${kind}${body})`;
      else if (kind === "(?<!") out += alternatives.map((alt) => `(?<!${alt})`).join("");
      else out += `(?:${alternatives.map((alt) => (alt === "^" ? "^" : `(?<=${alt})`)).join("|")})`;
      i = end + 1;
      continue;
    }
    out += ch;
    i += 1;
  }
  return out;
}

function rewritePatterns(node) {
  if (Array.isArray(node)) return node.map(rewritePatterns);
  if (!node || typeof node !== "object") return node;
  return Object.fromEntries(
    Object.entries(node).map(([key, value]) => [
      key,
      ["match", "begin", "end", "while"].includes(key) && typeof value === "string"
        ? splitLookbehinds(onigToFancy(value))
        : rewritePatterns(value),
    ]),
  );
}

const scratch = mkdtempSync(join(tmpdir(), "t3-highlight-grammars-"));
for (const name of SHIKI_GRAMMARS) {
  const module = await import(pathToFileURL(pkgDir("@shikijs+langs@", "@shikijs/langs/dist", `${name}.mjs`)).href);
  const [grammar, ...embedded] = module.default;
  if (embedded.length > 0) throw new Error(`${name} embeds other grammars; not supported`);
  writeFileSync(join(scratch, `${name}.tmLanguage.json`), JSON.stringify(rewritePatterns(grammar)));
}
execFileSync(
  "cargo",
  [
    "run",
    "--quiet",
    "--release",
    "--manifest-path",
    join(out, "..", "tools/grammars/Cargo.toml"),
    "--",
    scratch,
    join(out, "grammars"),
  ],
  { stdio: "inherit" },
);
rmSync(scratch, { recursive: true });
