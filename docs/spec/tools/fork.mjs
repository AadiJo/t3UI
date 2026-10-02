// Shared paths for the spec tools: the fork checkout and packages from its pnpm store.
//
// The fork root defaults to ~/L-Projects/t3UI-refs/t3code-fork (AadiJo/t3code-again main).
// Override with T3_FORK=/path/to/checkout. Packages resolve to whatever version the fork's
// lockfile installed, so a dependency bump never leaves a tool reading an old copy.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

export const FORK = path.resolve(
  process.env.T3_FORK ?? path.join(os.homedir(), "L-Projects/t3UI-refs/t3code-fork"),
);
export const WEB = path.join(FORK, "apps/web");
export const PNPM = path.join(FORK, "node_modules/.pnpm");
// Scratch dir for generated intermediates (compiled CSS, resolved tokens).
export const TMP = process.env.T3SPEC_TMP ?? "/tmp/t3spec";
fs.mkdirSync(TMP, { recursive: true });

/** Directory of `name` as installed for apps/web (e.g. "tailwindcss", "@tailwindcss/node"). */
export function webPackageDir(name) {
  const link = path.join(WEB, "node_modules", name);
  if (fs.existsSync(link)) return fs.realpathSync(link);
  // Transitive dependencies are only in the pnpm store: pick the newest copy.
  const prefix = `${name.replace("/", "+")}@`;
  const dir = fs.readdirSync(PNPM).filter((d) => d.startsWith(prefix)).sort().at(-1);
  if (!dir) throw new Error(`package ${name} not found under ${PNPM}`);
  return path.join(PNPM, dir, "node_modules", name);
}

export function packageVersion(name) {
  return JSON.parse(fs.readFileSync(path.join(webPackageDir(name), "package.json"), "utf8")).version;
}

/** `require` rooted at a package directory, for CommonJS entry points. */
export function requireFrom(name) {
  return createRequire(path.join(webPackageDir(name), "package.json"));
}

export function forkCommit() {
  const head = fs.readFileSync(path.join(FORK, ".git/HEAD"), "utf8").trim();
  if (!head.startsWith("ref:")) return head.slice(0, 9);
  const ref = head.slice(5);
  const loose = path.join(FORK, ".git", ref);
  if (fs.existsSync(loose)) return fs.readFileSync(loose, "utf8").trim().slice(0, 9);
  const packed = fs.readFileSync(path.join(FORK, ".git/packed-refs"), "utf8");
  return packed.split("\n").find((line) => line.endsWith(ref))?.slice(0, 9) ?? "unknown";
}
