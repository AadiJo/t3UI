// Scripted turns replayed by the fake Codex, keyed by prompt text.
//
// Each scenario has a `prompt` (the exact text e2e/seed.mjs sends; matching is
// a case-insensitive substring test, so a human can trigger one from the UI
// too), a `title` returned for thread-title generation, and `run(t)` which
// drives the turn through the context API in ./codex (message, reasoning,
// command, applyPatch, webSearch, mcpTool, proposedPlan, askUser, todo...).
//
// File edits must stay consistent with the fixture repos in
// e2e/lib/fixture-repos.mjs, because T3 computes diffs from real git state.

"use strict";

/** Thrown by a scenario to end the turn with status "failed" and this message. */
class ScenarioFailure extends Error {}

const FORMAT_TS_AFTER = `const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** Format a byte count with 1024-based steps, e.g. 1536 -> "1.5 KB". */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    throw new RangeError(\`formatBytes expects a non-negative number, got \${bytes}\`);
  }
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? \`\${value} B\` : \`\${value.toFixed(1)} \${UNITS[unit]}\`;
}
`;

const FORMAT_TEST_AFTER = `import { test } from "node:test";
import assert from "node:assert/strict";
import { formatBytes } from "../src/format";

test("formats kilobytes", () => {
  assert.equal(formatBytes(1536), "1.5 KB");
});

test("formats megabytes and gigabytes", () => {
  assert.equal(formatBytes(5 * 1024 ** 2), "5.0 MB");
  assert.equal(formatBytes(3 * 1024 ** 3), "3.0 GB");
});

test("rejects negative input", () => {
  assert.throws(() => formatBytes(-1), RangeError);
});
`;

const SHOWCASE_ANSWER = `## Repository tour

**aurora-web** is a small TypeScript dashboard. Here is how it is laid out:

- \`src/index.ts\`: entry point, logs a formatted size
- \`src/format.ts\`: the \`formatBytes\` helper
- \`test/format.test.ts\`: \`node:test\` coverage for the helper
- \`README.md\`: project notes, with an uncommitted *Status* section

### What changed

\`formatBytes\` now walks up through units instead of stopping at kilobytes, and rejects negative input.

| Input | Before | After |
| ---: | --- | --- |
| \`512\` | \`512 B\` | \`512 B\` |
| \`1536\` | \`1.5 KB\` | \`1.5 KB\` |
| \`5242880\` | \`5120.0 KB\` | \`5.0 MB\` |
| \`3221225472\` | \`3145728.0 KB\` | \`3.0 GB\` |

\`\`\`ts
const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

export function formatBytes(bytes: number): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? \`\${value} B\` : \`\${value.toFixed(1)} \${UNITS[unit]}\`;
}
\`\`\`

### Verification

1. Added megabyte, gigabyte and negative-input cases to \`test/format.test.ts\`
2. Ran the suite: **3 passing**, 0 failing

\`\`\`bash
node --test
# tests 3, pass 3, fail 0
\`\`\`

### Porting it to borealis

The same loop translates directly if the Rust service needs it:

\`\`\`rust
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}
\`\`\`

> Units are 1024-based but labelled \`KB\`/\`MB\` to match the existing test. Switch to \`KiB\`/\`MiB\` if you want IEC labels.

**Next steps**

- [ ] Decide between IEC and SI labels
- [ ] Use \`formatBytes\` in the dashboard header
`;

const MONOREPO_PLAN = `# Move aurora-web to a pnpm monorepo

## Summary

Split the app into \`apps/web\` and a shared \`packages/ui\` so dashboard widgets and helpers like \`formatBytes\` can be reused by other front ends.

## Steps

1. Add \`pnpm-workspace.yaml\` listing \`apps/*\` and \`packages/*\`
2. Move \`src/\` and \`test/\` into \`apps/web/\` with \`git mv\` to keep history
3. Extract \`formatBytes\` into \`packages/ui/src/format.ts\` and export it from \`@aurora/ui\`
4. Depend on it from \`apps/web\` with \`"@aurora/ui": "workspace:*"\`
5. Replace the root \`test\` script with \`pnpm -r test\`

## Risks

- Every import path changes, so land the move in a single commit
- The \`node --test\` globs must be updated for the new layout

## Test plan

- \`pnpm install\` from a clean checkout
- \`pnpm -r test\` passes in CI
`;

const SCENARIOS = [
  {
    id: "showcase",
    prompt: "Give me a tour of this repo, then make formatBytes handle megabytes and gigabytes.",
    title: "Repo tour and formatBytes units",
    async run(t) {
      await t.message("I'll start by looking around the repository to see how it is laid out.", { phase: "commentary" });
      await t.reasoning(
        "**Surveying the project layout**\n\nThe repo is small: a package.json, a src folder and a test folder. I'll list files, read the helper and check the existing test before changing anything.",
      );
      t.todo([
        ["Survey the repository layout", "inProgress"],
        ["Research byte formatting conventions", "pending"],
        ["Extend formatBytes to MB and GB", "pending"],
        ["Run the test suite", "pending"],
      ]);
      await t.command("ls -la", {
        action: { type: "listFiles", command: "ls -la", path: null },
        output:
          "total 24\ndrwxr-xr-x 5 dev dev 4096 Sep  3 09:15 .\ndrwxr-xr-x 3 dev dev 4096 Sep  3 09:15 ..\ndrwxr-xr-x 8 dev dev 4096 Sep  3 09:15 .git\n-rw-r--r-- 1 dev dev  103 Sep  3 09:15 README.md\n-rw-r--r-- 1 dev dev  118 Sep  1 10:00 package.json\ndrwxr-xr-x 2 dev dev 4096 Sep  2 14:30 src\ndrwxr-xr-x 2 dev dev 4096 Sep  3 09:15 test\n",
      });
      await t.command("cat src/format.ts", {
        action: { type: "read", command: "cat src/format.ts", name: "format.ts", path: "src/format.ts" },
        output:
          "export function formatBytes(bytes: number): string {\n  if (bytes < 1024) return `${bytes} B`;\n  return `${(bytes / 1024).toFixed(1)} KB`;\n}\n",
      });
      await t.command("rg -n formatBytes", {
        action: { type: "search", command: "rg -n formatBytes", path: null, query: "formatBytes" },
        output:
          'src/format.ts:1:export function formatBytes(bytes: number): string {\nsrc/index.ts:1:import { formatBytes } from "./format";\nsrc/index.ts:3:console.log(formatBytes(1536));\ntest/format.test.ts:3:import { formatBytes } from "../src/format";\ntest/format.test.ts:6:  assert.equal(formatBytes(1536), "1.5 KB");\n',
      });
      t.todo([
        ["Survey the repository layout", "completed"],
        ["Research byte formatting conventions", "inProgress"],
        ["Extend formatBytes to MB and GB", "pending"],
        ["Run the test suite", "pending"],
      ]);
      await t.webSearch("binary vs decimal byte units KiB MiB convention");
      await t.mcpTool(
        "docs",
        "lookup",
        { topic: "Intl.NumberFormat byte units" },
        "Intl.NumberFormat supports unit: 'kilobyte' | 'megabyte' | 'gigabyte' with unitDisplay: 'short', but uses 1000-based values.",
      );
      await t.reasoning(
        "**Choosing units**\n\nThe existing test expects `1.5 KB` for 1536 bytes, so the helper is 1024-based with KB labels. I'll keep that convention, add MB and GB, and guard against negative input.",
      );
      t.todo([
        ["Survey the repository layout", "completed"],
        ["Research byte formatting conventions", "completed"],
        ["Extend formatBytes to MB and GB", "inProgress"],
        ["Run the test suite", "pending"],
      ]);
      await t.applyPatch([
        { path: "src/format.ts", content: FORMAT_TS_AFTER },
        { path: "test/format.test.ts", content: FORMAT_TEST_AFTER },
      ]);
      await t.command("node --test", {
        output:
          "✔ formats kilobytes (0.61ms)\n✔ formats megabytes and gigabytes (0.12ms)\n✔ rejects negative input (0.2ms)\nℹ tests 3\nℹ suites 0\nℹ pass 3\nℹ fail 0\nℹ cancelled 0\nℹ skipped 0\nℹ todo 0\nℹ duration_ms 41.3\n",
        durationMs: 812,
      });
      t.todo([
        ["Survey the repository layout", "completed"],
        ["Research byte formatting conventions", "completed"],
        ["Extend formatBytes to MB and GB", "completed"],
        ["Run the test suite", "completed"],
      ]);
      await t.message(SHOWCASE_ANSWER);
      t.tokenUsage(48210);
    },
  },
  {
    id: "approval",
    prompt: "Run the database migration against the local dev database.",
    title: "Run local database migration",
    async run(t) {
      await t.message("I'll check which migrations are pending, then apply them.", { phase: "commentary" });
      await t.command("ls migrations", {
        action: { type: "listFiles", command: "ls migrations", path: "migrations" },
        output: "0001_init.sql\n0002_add_widgets.sql\n0003_widget_owner.sql\n",
      });
      const decision = await t.command("pnpm db:migrate", {
        approval: true,
        reason: "Applies 0003_widget_owner.sql to the local Postgres database (writes outside the workspace).",
        output: "> aurora-web@0.1.0 db:migrate\n> node scripts/migrate.mjs\n\napplying 0003_widget_owner.sql ... ok\n1 migration applied\n",
      });
      await t.message(
        decision === "decline"
          ? "Understood, I did not run the migration. `0003_widget_owner.sql` is still pending."
          : "Applied `0003_widget_owner.sql`. The local database is now at migration **3**.",
      );
      t.tokenUsage(9120);
    },
  },
  {
    id: "plan",
    prompt: "Plan the move to a pnpm monorepo with a shared ui package.",
    title: "Plan pnpm monorepo migration",
    async run(t) {
      await t.reasoning(
        "**Mapping the current layout**\n\nSingle package, no workspace file yet. I need to know the scripts and entry points before proposing a split.",
      );
      await t.command("cat package.json", {
        action: { type: "read", command: "cat package.json", name: "package.json", path: "package.json" },
        output: '{\n  "name": "aurora-web",\n  "version": "0.1.0",\n  "type": "module",\n  "scripts": {\n    "test": "node --test"\n  }\n}\n',
      });
      await t.message("I have enough context. Here is the plan I'd follow:", { phase: "commentary" });
      await t.proposedPlan(MONOREPO_PLAN);
      t.tokenUsage(15870);
    },
  },
  {
    id: "running",
    prompt: "Watch the test suite and fix failures as they come in.",
    title: "Watch tests and fix failures",
    async run(t) {
      await t.message("Starting the test watcher. I'll fix failures as they show up.", { phase: "commentary" });
      t.todo([
        ["Start the test watcher", "completed"],
        ["Fix failing tests", "inProgress"],
      ]);
      await t.command("pnpm test --watch", {
        hang: true,
        output: "> aurora-web@0.1.0 test\n> node --test --watch\n\n✔ formats kilobytes (0.58ms)\nℹ tests 1\nℹ pass 1\nℹ fail 0\n\nWatching for file changes...\n",
      });
    },
  },
  {
    id: "question",
    prompt: "Set up persistence for the health checks.",
    title: "Persist health check results",
    async run(t) {
      await t.reasoning(
        "**Picking a store**\n\nThe service has no database yet. The choice changes the dependencies, so I should ask before adding one.",
      );
      const answers = await t.askUser([
        {
          id: "database",
          header: "Database",
          question: "Which database should the health checks write to?",
          options: [
            { label: "Postgres", description: "Matches production, needs a running server" },
            { label: "SQLite", description: "Embedded file, zero setup" },
          ],
        },
      ]);
      const choice = answers?.database?.answers?.[0] ?? "SQLite";
      await t.message(`Going with **${choice}**. I'll add the connection pool next.`);
    },
  },
  {
    id: "readiness",
    prompt: "Add a readiness endpoint next to /health.",
    title: "Add readiness endpoint",
    async run(t) {
      await t.command("cat src/health.rs", {
        action: { type: "read", command: "cat src/health.rs", name: "health.rs", path: "src/health.rs" },
        output: "pub fn health() -> &'static str {\n    \"ok\"\n}\n",
      });
      await t.applyPatch([
        {
          path: "src/health.rs",
          content:
            "pub fn health() -> &'static str {\n    \"ok\"\n}\n\n/// Ready once configuration is loaded; used by the load balancer.\npub fn ready(config_loaded: bool) -> (u16, &'static str) {\n    if config_loaded {\n        (200, \"ready\")\n    } else {\n        (503, \"starting\")\n    }\n}\n",
        },
      ]);
      await t.command("cargo check", {
        output: "    Checking borealis v0.1.0 (/workspace/borealis-api)\n    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.84s\n",
      });
      await t.message(
        "Added `ready()` in `src/health.rs`. It returns `503 starting` until config is loaded, then `200 ready`.\n\n```rust\nlet (status, body) = health::ready(config.is_loaded());\n```",
      );
      t.tokenUsage(11240);
    },
  },
  {
    id: "docs",
    prompt: "Tighten up the deployment guide.",
    title: "Tighten deployment guide",
    async run(t) {
      await t.command("cat docs/deploy.md", {
        action: { type: "read", command: "cat docs/deploy.md", name: "deploy.md", path: "docs/deploy.md" },
        output: "# Deploying\n\n1. Build\n2. Ship\n",
      });
      await t.applyPatch([
        {
          path: "docs/deploy.md",
          content:
            "# Deploying\n\n1. Build the site: `mkdocs build --strict`\n2. Upload `site/` to the bucket: `aws s3 sync site/ s3://cirrus-docs --delete`\n3. Invalidate the CDN cache for `/*`\n\nRollback: re-run step 2 from the previous release tag.\n",
        },
      ]);
      await t.message("Rewrote `docs/deploy.md` with the exact commands for each step and a rollback note.");
      t.tokenUsage(6400);
    },
  },
  {
    id: "summary",
    prompt: "Summarize the README.",
    title: "Summarize the README",
    async run(t) {
      await t.message(
        "The README describes **Cirrus**, a set of cloud notes used by the fixture suite. It currently has an intro page and a deployment guide, plus a TODO to add diagrams.",
      );
      t.tokenUsage(3100);
    },
  },
  {
    id: "failure",
    prompt: "Deploy the docs to production.",
    title: "Deploy docs to production",
    async run(t) {
      await t.command("aws s3 sync site/ s3://cirrus-docs --delete", {
        exitCode: 1,
        output: "fatal error: Unable to locate credentials. You can configure credentials by running \"aws configure\".\n",
      });
      throw new ScenarioFailure("Deployment failed: AWS credentials are not configured on this machine.");
    },
  },
];

const FALLBACK = {
  id: "fallback",
  prompt: "",
  title: "Fake Codex conversation",
  async run(t) {
    await t.message(
      "This server is running the **fake Codex** used by the t3UI end-to-end harness, so replies are scripted.\n\nTry one of the scripted prompts, for example:\n\n" +
        SCENARIOS.map((scenario) => `- \`${scenario.prompt}\``).join("\n"),
    );
  },
};

const PLAN_FALLBACK = {
  id: "plan-fallback",
  prompt: "",
  title: "Fake Codex plan",
  async run(t) {
    await t.proposedPlan("# Plan\n\n1. Read the relevant files\n2. Make the change\n3. Run the tests\n");
  },
};

function findScenario(promptText, { mode = "default" } = {}) {
  const haystack = promptText.toLowerCase();
  const match = SCENARIOS.find((scenario) => haystack.includes(scenario.prompt.toLowerCase()));
  if (match) return match;
  return mode === "plan" ? PLAN_FALLBACK : FALLBACK;
}

/** Structured output for `codex exec` (titles, commit messages, PR text, branch names). */
function generateText(promptText, keys) {
  const scenario = findScenario(promptText);
  const values = {
    title: scenario.title,
    needsRefinement: false,
    subject: `chore: ${scenario.title.toLowerCase()}`,
    body: "Generated by the fake Codex CLI used in t3UI end-to-end runs.",
    branch: `fake/${scenario.id}`,
  };
  return Object.fromEntries(keys.filter((key) => key in values).map((key) => [key, values[key]]));
}

module.exports = { SCENARIOS, ScenarioFailure, findScenario, generateText };
