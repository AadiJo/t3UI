// Deterministic git repositories used as T3 Code project workspaces.
//
// Every repo gets fixed author/committer identities and dates, so commit
// hashes are identical across runs and machines. Each repo also ends with an
// uncommitted change, so the branch toolbar and git status show a dirty tree.
//
// Usage: createFixtureRepos("/tmp/t3ui-e2e/run/repos") -> [{ key, title, path }]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const GIT_ENV = {
  // Ignore the developer's ~/.gitconfig and /etc/gitconfig entirely.
  GIT_CONFIG_GLOBAL: "/dev/null",
  GIT_CONFIG_NOSYSTEM: "1",
  GIT_AUTHOR_NAME: "T3 Fixture",
  GIT_AUTHOR_EMAIL: "fixture@t3ui.invalid",
  GIT_COMMITTER_NAME: "T3 Fixture",
  GIT_COMMITTER_EMAIL: "fixture@t3ui.invalid",
};

function git(cwd, args, date) {
  const env = { ...process.env, ...GIT_ENV };
  if (date) Object.assign(env, { GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date });
  return execFileSync("git", ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", ...args], {
    cwd,
    env,
    stdio: ["ignore", "pipe", "pipe"],
  }).toString();
}

function writeFiles(root, files) {
  for (const [relative, content] of Object.entries(files)) {
    const target = path.join(root, relative);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content);
  }
}

/**
 * Repo specs. `commits` are applied in order (files are written, then
 * `git add -A && git commit`); `dirty` is written last and left uncommitted.
 * The fake Codex edits files in the first repo during the scripted turns, so
 * keep `src/format.ts` and `README.md` there in sync with fake-codex scripts.
 */
export const FIXTURE_REPOS = [
  {
    key: "aurora",
    title: "aurora-web",
    commits: [
      {
        date: "2026-09-01T10:00:00Z",
        message: "chore: scaffold aurora web app",
        files: {
          "package.json": JSON.stringify(
            { name: "aurora-web", version: "0.1.0", type: "module", scripts: { test: "node --test" } },
            null,
            2,
          ) + "\n",
          "README.md": "# Aurora\n\nA tiny dashboard used as a T3 Code fixture.\n",
          "src/index.ts": 'import { formatBytes } from "./format";\n\nconsole.log(formatBytes(1536));\n',
        },
      },
      {
        date: "2026-09-02T14:30:00Z",
        message: "feat: add byte formatting helper",
        files: {
          "src/format.ts":
            "export function formatBytes(bytes: number): string {\n" +
            "  if (bytes < 1024) return `${bytes} B`;\n" +
            "  return `${(bytes / 1024).toFixed(1)} KB`;\n" +
            "}\n",
        },
      },
      {
        date: "2026-09-03T09:15:00Z",
        message: "test: cover formatBytes",
        files: {
          "test/format.test.ts":
            'import { test } from "node:test";\nimport assert from "node:assert/strict";\n' +
            'import { formatBytes } from "../src/format";\n\n' +
            'test("formats kilobytes", () => {\n  assert.equal(formatBytes(1536), "1.5 KB");\n});\n',
        },
      },
    ],
    dirty: {
      "README.md": "# Aurora\n\nA tiny dashboard used as a T3 Code fixture.\n\n## Status\n\nWork in progress.\n",
    },
  },
  {
    key: "borealis",
    title: "borealis-api",
    commits: [
      {
        date: "2026-08-20T08:00:00Z",
        message: "chore: init borealis service",
        files: {
          "Cargo.toml": '[package]\nname = "borealis"\nversion = "0.1.0"\nedition = "2021"\n',
          "src/main.rs": 'fn main() {\n    println!("borealis listening on :8080");\n}\n',
        },
      },
      {
        date: "2026-08-21T16:45:00Z",
        message: "feat: add health route",
        files: {
          "src/health.rs": 'pub fn health() -> &\'static str {\n    "ok"\n}\n',
        },
      },
    ],
    dirty: {
      "src/config.rs": "pub const PORT: u16 = 8080;\n",
    },
  },
  {
    key: "cirrus",
    title: "cirrus-docs",
    commits: [
      {
        date: "2026-07-10T12:00:00Z",
        message: "docs: initial outline",
        files: {
          "mkdocs.yml": "site_name: Cirrus\nnav:\n  - Intro: intro.md\n",
          "docs/intro.md": "# Cirrus\n\nCloud notes for the fixture suite.\n",
        },
      },
      {
        date: "2026-07-11T12:00:00Z",
        message: "docs: add deployment guide",
        files: {
          "docs/deploy.md": "# Deploying\n\n1. Build\n2. Ship\n",
        },
      },
    ],
    dirty: {
      "docs/intro.md": "# Cirrus\n\nCloud notes for the fixture suite.\n\nTODO: add diagrams.\n",
    },
  },
];

/** Create all fixture repos under `rootDir` (which must not already contain them). */
export function createFixtureRepos(rootDir) {
  fs.mkdirSync(rootDir, { recursive: true });
  return FIXTURE_REPOS.map((spec) => {
    const repoPath = path.join(rootDir, spec.title);
    if (fs.existsSync(repoPath)) throw new Error(`fixture repo already exists: ${repoPath}`);
    fs.mkdirSync(repoPath);
    git(repoPath, ["init", "-q"]);
    for (const commit of spec.commits) {
      writeFiles(repoPath, commit.files);
      git(repoPath, ["add", "-A"], commit.date);
      git(repoPath, ["commit", "-q", "-m", commit.message], commit.date);
    }
    writeFiles(repoPath, spec.dirty);
    return { key: spec.key, title: spec.title, path: repoPath };
  });
}
