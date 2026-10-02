#!/usr/bin/env node
// Capture reference screenshots of the fork's web UI, the visual target for
// the native GPUI port. Expects a server seeded by e2e/seed.mjs and the fork's
// Vite web UI in front of it (e2e/run-local.sh up --server fork --web).
//
//   node e2e/capture-web-reference.mjs --state /tmp/t3ui-e2e/run-fork/state.json \
//     --web-url http://127.0.0.1:4721 --out docs/reference [--only diff-panel,settings-general] [--themes dark]
//
// Output: <out>/<surface>-<theme>.png at 1440x900 CSS px, device scale 2
// (2880x1800 PNGs). The surface list below is the source of truth for
// docs/reference/README.md.
//
// playwright-core is resolved from e2e/node_modules if present, otherwise from
// the fork checkout (T3UI_FORK_DIR, default ~/L-Projects/t3code-again). The
// browser is Playwright's chromium-headless-shell for that version
// (`npx playwright-core@1.60.0 install chromium-headless-shell`).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { parseArgs } from "node:util";

import { mintPairingToken } from "./lib/t3-protocol.mjs";

const { values: args } = parseArgs({
  options: {
    state: { type: "string" },
    "web-url": { type: "string" },
    out: { type: "string", default: "docs/reference" },
    only: { type: "string" },
    themes: { type: "string", default: "dark,light" },
    headed: { type: "boolean", default: false },
  },
});

const VIEWPORT = { width: 1440, height: 900 };
const DEVICE_SCALE = 2;
const FIXED_CLOCK_OFFSET_MS = 2 * 60_000; // page clock = seededAt + 2 minutes

function loadPlaywright() {
  const require = createRequire(import.meta.url);
  try {
    return require("playwright-core");
  } catch {
    const forkDir = process.env.T3UI_FORK_DIR ?? path.join(os.homedir(), "L-Projects/t3code-again");
    return createRequire(path.join(forkDir, "apps/desktop/package.json"))("playwright-core");
  }
}

const state = JSON.parse(fs.readFileSync(args.state, "utf8"));
const webUrl = args["web-url"].replace(/\/$/, "");
const threadUrl = (threadId) => `${webUrl}/${state.environmentId}/${threadId}`;
const thread = (scenario) => state.threads.find((entry) => entry.scenario === scenario);
const mod = process.platform === "darwin" ? "Meta" : "Control";

// ---------------------------------------------------------------------------
// Page helpers
// ---------------------------------------------------------------------------

/** Wait until the app shell is connected and idle enough to screenshot. */
async function settle(page, { extraMs = 400 } = {}) {
  await page.waitForLoadState("networkidle").catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(extraMs);
}

async function openThread(page, scenario) {
  const target = thread(scenario);
  await page.goto(threadUrl(target.id));
  await page.getByTestId("composer-editor").waitFor();
  await page.getByText(target.prompt, { exact: false }).first().waitFor();
  await settle(page);
}

/** Park the pointer on empty background so no hover styles or tooltips leak into shots. */
async function parkMouse(page) {
  await page.mouse.move(VIEWPORT.width - 2, VIEWPORT.height - 2);
}

/** Open a composer control (model/traits picker) in the showcase thread. */
async function openComposerControl(page, locator, popupRole) {
  await openThread(page, "showcase");
  await scrollTimeline(page, "bottom");
  await locator(page).click();
  await page.getByRole(popupRole).first().waitFor();
  await parkMouse(page);
  await settle(page, { extraMs: 300 });
}

/**
 * Scroll the conversation timeline to "top" | "bottom". The timeline is a
 * virtualized list that re-measures rows and re-anchors after layout changes,
 * so repeat until the position holds.
 */
async function scrollTimeline(page, where) {
  let stable = 0;
  for (let attempt = 0; attempt < 10 && stable < 2; attempt += 1) {
    const atTarget = await page.evaluate((where) => {
      const candidates = [...document.querySelectorAll("*")].filter((el) => {
        const style = getComputedStyle(el);
        return /(auto|scroll)/.test(style.overflowY) && el.scrollHeight > el.clientHeight + 20;
      });
      // The timeline is the largest scrollable element in the main pane.
      const timeline = candidates.sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
      if (!timeline) return true;
      const target = where === "top" ? 0 : timeline.scrollHeight - timeline.clientHeight;
      const held = Math.abs(timeline.scrollTop - target) < 2;
      timeline.scrollTop = where === "top" ? 0 : timeline.scrollHeight;
      return held;
    }, where);
    stable = atTarget ? stable + 1 : 0;
    await page.waitForTimeout(400);
  }
}

// ---------------------------------------------------------------------------
// Surfaces. Each entry: what state it shows (for the README) and how to reach it.
// ---------------------------------------------------------------------------

export const SURFACES = [
  {
    name: "sidebar",
    describe: "Home route with the sidebar listing the three seeded projects and their threads; no thread selected.",
    async setup(page) {
      await page.goto(`${webUrl}/`);
      await page.getByTestId(`thread-row-${thread("showcase").id}`).waitFor();
      await settle(page);
    },
  },
  {
    name: "new-thread",
    describe: "Empty draft thread in aurora-web (Ctrl+N from the showcase thread): empty timeline and idle composer.",
    async setup(page) {
      await openThread(page, "showcase");
      await page.keyboard.press(`${mod}+n`);
      await page.waitForURL(/\/draft\//);
      await page.getByTestId("composer-editor").waitFor();
      await settle(page);
    },
  },
  {
    name: "conversation-top",
    describe: "Showcase thread scrolled to the top: user prompt, collapsed 'Worked for' row, start of the markdown answer (headings, bullet list, inline code, table).",
    async setup(page) {
      await openThread(page, "showcase");
      await scrollTimeline(page, "top");
    },
  },
  {
    name: "conversation-bottom",
    describe: "Showcase thread scrolled to the bottom: rust code block, blockquote, task list, changed-files card.",
    async setup(page) {
      await openThread(page, "showcase");
      await scrollTimeline(page, "bottom");
    },
  },
  {
    name: "work-log",
    describe: "Showcase thread with the 'Worked for' section expanded: commentary, commands, web search, MCP call, file change.",
    async setup(page) {
      await openThread(page, "showcase");
      await scrollTimeline(page, "top");
      await page.getByText(/^Worked for/).first().click();
      await page.getByText(/previous log entr/).first().click();
      await page.getByText(/Show fewer log entries/).first().waitFor();
      await page.waitForTimeout(800); // expansion animates; the list re-anchors while it does
      await scrollTimeline(page, "top");
      await parkMouse(page);
      await settle(page);
    },
  },
  {
    name: "model-picker",
    describe: "Showcase thread with the composer model picker open (fake Codex models).",
    setup: (page) =>
      openComposerControl(page, (p) => p.locator('[data-chat-provider-model-picker="true"]').first(), "dialog"),
  },
  {
    name: "traits-picker",
    describe: "Showcase thread with the reasoning/speed traits menu open.",
    setup: (page) => openComposerControl(page, (p) => p.getByRole("button", { name: /Medium/ }).first(), "menu"),
  },
  {
    name: "slash-menu",
    describe: "Showcase thread with '/' typed in the composer, showing the slash-command menu.",
    async setup(page) {
      await openThread(page, "showcase");
      await scrollTimeline(page, "bottom");
      await page.getByTestId("composer-editor").click();
      await page.keyboard.type("/");
      await page.getByRole("listbox").or(page.getByRole("menu")).first().waitFor({ timeout: 5_000 }).catch(() => {});
      await parkMouse(page);
      await settle(page, { extraMs: 300 });
    },
    async teardown(page) {
      await page.keyboard.press("Escape");
      await page.keyboard.press("Backspace");
    },
  },
  {
    name: "command-palette",
    describe: "Command palette (Ctrl+K) opened over the showcase thread.",
    async setup(page) {
      await openThread(page, "showcase");
      await page.keyboard.press(`${mod}+k`);
      await page.getByTestId("command-palette").waitFor();
      await parkMouse(page);
      await settle(page, { extraMs: 300 });
    },
  },
  {
    name: "diff-panel",
    describe:
      "Showcase thread with the diff panel open via the changed-files card's 'View diff': the turn's edits to src/format.ts and test/format.test.ts.",
    async setup(page) {
      await openThread(page, "showcase");
      await scrollTimeline(page, "bottom");
      await page.getByRole("button", { name: "View diff" }).first().click();
      // Wait for rendered diff hunks (added lines), not just the panel chrome.
      await page.getByText("formats megabytes and gigabytes").last().waitFor();
      await parkMouse(page);
      await settle(page, { extraMs: 800 });
    },
    async teardown(page) {
      await page.keyboard.press(`${mod}+d`); // diff.toggle closes the panel
      await page.waitForTimeout(300);
    },
  },
  {
    name: "approval-pending",
    describe: "Approval-required thread waiting on a command approval for `pnpm db:migrate`.",
    async setup(page) {
      await openThread(page, "approval");
      await scrollTimeline(page, "bottom");
      await parkMouse(page);
    },
  },
  {
    name: "user-input-pending",
    describe: "borealis-api thread blocked on a multiple-choice question (Postgres vs SQLite).",
    async setup(page) {
      await openThread(page, "question");
      await scrollTimeline(page, "bottom");
      await parkMouse(page);
    },
  },
  {
    name: "plan-proposed",
    describe: "Plan-mode thread showing the proposed plan card and its follow-up actions.",
    async setup(page) {
      await openThread(page, "plan");
      await scrollTimeline(page, "bottom");
      await parkMouse(page);
    },
  },
  {
    name: "thread-running",
    describe: "Thread with a turn still running (parked inside `pnpm test --watch`).",
    async setup(page) {
      await openThread(page, "running");
      await scrollTimeline(page, "bottom");
      await parkMouse(page);
    },
  },
  {
    name: "thread-failed",
    describe: "cirrus-docs thread whose turn failed (missing AWS credentials).",
    async setup(page) {
      await openThread(page, "failure");
      await scrollTimeline(page, "bottom");
      await parkMouse(page);
    },
  },
  {
    name: "settings-general",
    describe: "Settings, General section.",
    async setup(page) {
      await page.goto(`${webUrl}/settings/general`);
      await page.waitForURL(/\/settings\/general/);
      await settle(page, { extraMs: 800 });
    },
  },
  {
    name: "settings-connections",
    describe: "Settings, Connections section (pairing links and paired clients for this environment).",
    async setup(page) {
      await page.goto(`${webUrl}/settings/connections`);
      await page.waitForURL(/\/settings\/connections/);
      await settle(page, { extraMs: 800 });
    },
  },
  {
    name: "settings-providers",
    describe: "Settings, Providers section with the fake Codex instance ready and other providers disabled.",
    async setup(page) {
      await page.goto(`${webUrl}/settings/providers`);
      await page.waitForURL(/\/settings\/providers/);
      await settle(page, { extraMs: 800 });
    },
  },
  {
    name: "settings-archived",
    describe: "Settings, Archived section listing the archived cirrus-docs thread.",
    async setup(page) {
      await page.goto(`${webUrl}/settings/archived`);
      await page.waitForURL(/\/settings\/archived/);
      await settle(page, { extraMs: 800 });
    },
  },
  // Last: opening a terminal starts a real shell on the server, which then
  // shows up as a terminal indicator on the thread row in later captures.
  {
    name: "terminal-drawer",
    describe: "Showcase thread with the terminal drawer open (Ctrl+J), a real shell in the aurora-web checkout.",
    async setup(page) {
      await openThread(page, "showcase");
      await page.keyboard.press(`${mod}+j`);
      await page.locator(".xterm").first().waitFor();
      await page.waitForTimeout(1500); // let the shell print its prompt
      await parkMouse(page);
      await settle(page);
    },
    async teardown(page) {
      // Close the server-side terminal (terminal.close, Ctrl+W with terminal
      // focus); merely hiding the drawer would leave it for the next theme.
      await page.locator(".xterm").first().click();
      await page.keyboard.press(`${mod}+w`);
      await page.locator(".xterm").first().waitFor({ state: "detached", timeout: 5_000 }).catch(async () => {
        await page.keyboard.press(`${mod}+j`);
      });
      await page.waitForTimeout(300);
    },
  },
];

/**
 * Keep server state identical across captures: viewing a completed thread
 * makes the client dispatch `thread.completion.acknowledge`, which would clear
 * the "Completed" badge for every later capture (and the next theme). Answer
 * that command locally with a synthetic Success; pass everything else through
 * and report other mutating commands so new side effects get noticed.
 */
async function guardServerState(context) {
  await context.routeWebSocket(/\/ws(\?|$)/, (ws) => {
    const server = ws.connectToServer();
    ws.onMessage((frame) => {
      const messages = [].concat(JSON.parse(String(frame)));
      for (const message of messages) {
        if (message._tag === "Request" && message.tag === "orchestration.dispatchCommand") {
          if (message.payload?.type === "thread.completion.acknowledge") {
            ws.send(
              JSON.stringify({ _tag: "Exit", requestId: message.id, exit: { _tag: "Success", value: { sequence: 0 } } }),
            );
            continue;
          }
          console.warn(`[capture] browser dispatched ${message.payload?.type}`);
        }
        server.send(JSON.stringify(message));
      }
    });
    server.onMessage((frame) => ws.send(frame));
  });
}

// ---------------------------------------------------------------------------

async function main() {
  const { chromium } = loadPlaywright();
  const outDir = path.resolve(args.out);
  fs.mkdirSync(outDir, { recursive: true });
  const only = args.only ? new Set(args.only.split(",")) : null;
  const surfaces = SURFACES.filter((surface) => !only || only.has(surface.name));
  const themes = args.themes.split(",");

  const browser = await chromium.launch({ headless: !args.headed });
  const failures = [];
  try {
    for (const theme of themes) {
      // One browser context per theme: pair once (one-time token), then reuse the session cookie.
      const context = await browser.newContext({
        viewport: VIEWPORT,
        deviceScaleFactor: DEVICE_SCALE,
        colorScheme: theme,
        reducedMotion: "reduce",
        locale: "en-US",
        timezoneId: "UTC",
      });
      await context.addInitScript((theme) => {
        window.localStorage.setItem("t3code:theme", theme);
      }, theme);
      // Pin Date.now() (timers keep running) so relative labels like "2m ago"
      // do not depend on how long after seeding the capture runs.
      await context.clock.setFixedTime(new Date(Date.parse(state.seededAt) + FIXED_CLOCK_OFFSET_MS));
      await guardServerState(context);
      const page = await context.newPage();
      page.on("pageerror", (error) => console.warn(`[capture] page error: ${error.message}`));

      // Delegate the seed's administrative scopes so the browser session looks
      // like the desktop app's (Settings > Connections needs access:read/write).
      const { credential } = await mintPairingToken(state.baseUrl, state.accessToken, {
        label: `capture-${theme}`,
        scopes: state.accessTokenScopes,
      });
      await page.goto(`${webUrl}/pair#token=${credential}`);
      await page.waitForURL((url) => !url.pathname.startsWith("/pair"), { timeout: 30_000 });

      for (const surface of surfaces) {
        const file = path.join(outDir, `${surface.name}-${theme}.png`);
        try {
          await surface.setup(page);
          await parkMouse(page);
          await page.screenshot({ path: file, animations: "disabled", caret: "hide" });
          console.log(`[capture] ${file}`);
          await surface.teardown?.(page);
        } catch (error) {
          failures.push(`${surface.name}-${theme}: ${error.message.split("\n")[0]}`);
          console.error(`[capture] FAILED ${surface.name}-${theme}: ${error.message.split("\n")[0]}`);
        }
        await page.keyboard.press("Escape").catch(() => {});
      }
      await context.close();
    }
  } finally {
    await browser.close();
  }
  if (failures.length > 0) throw new Error(`${failures.length} capture(s) failed:\n  ${failures.join("\n  ")}`);
  if (!only) writeReadme(outDir, themes);
}

/** Regenerate <out>/README.md from SURFACES so the index never drifts from the script. */
function writeReadme(outDir, themes) {
  const rows = SURFACES.map(
    (surface) =>
      `| ${surface.name} | ${themes.map((theme) => `[${theme}](${surface.name}-${theme}.png)`).join(" · ")} | ${surface.describe} |`,
  );
  const readme = `# Reference screenshots: fork web UI

The visual target for the native GPUI port: the fork's web UI (\`t3@${state.serverVersion}\`
from \`~/L-Projects/t3code-again\`, Vite dev mode) against a server seeded by
\`e2e/seed.mjs\` with the fake Codex. Generated by \`e2e/capture-web-reference.mjs\`;
do not edit by hand.

- Viewport 1440x900 CSS px, device scale factor 2 (PNG files are 2880x1800).
- Headless Chromium (playwright-core 1.60.0, chromium-headless-shell 1223), locale en-US, timezone UTC.
- Theme via \`localStorage["t3code:theme"]\` plus \`prefers-color-scheme\`; CSS animations disabled, caret hidden.
- Page clock pinned to seed time + 2 minutes, so relative times read "2m ago".
- Captured ${new Date().toISOString().slice(0, 10)}.

Reproduce from scratch (fresh isolated server, seed, capture, teardown):

\`\`\`bash
e2e/run-local.sh up --server fork --capture
\`\`\`

Or against an already running fork server + web UI (\`e2e/run-local.sh up --server fork --web --detach\`):

\`\`\`bash
node e2e/capture-web-reference.mjs --state /tmp/t3ui-e2e/run-fork/state.json \\
  --web-url http://127.0.0.1:4721 --out docs/reference [--only ${SURFACES[0].name}] [--themes dark]
\`\`\`

| Surface | Files | State shown |
| --- | --- | --- |
${rows.join("\n")}

Notes on what is fork behavior rather than a capture artifact:

- The app name reads "T3 Code (Dev)" because the web UI runs under Vite dev mode.
- The error banner in \`thread-failed\` really is ~78 px wide: its \`mx-auto max-w-3xl\`
  wrapper shrink-wraps a line-clamped flex row. The fork renders it that way.
- In \`thread-running\` the in-progress command is not listed in the work log; the fork only lists finished tool rows.
- The fork drops reasoning text, so no reasoning appears even though the fake sends it.
- The environment label and the "Worked for" durations vary slightly between machines and runs.
`;
  fs.writeFileSync(path.join(outDir, "README.md"), readme);
  console.log(`[capture] ${path.join(outDir, "README.md")}`);
}

main().catch((error) => {
  console.error("[capture] failed:", error);
  process.exit(1);
});
