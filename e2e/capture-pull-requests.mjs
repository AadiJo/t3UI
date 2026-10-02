#!/usr/bin/env node
// Capture the fork's Pull Requests page (the reference for the native page) in the states a
// seeded server can reach without GitHub credentials. The browser's /ws traffic is proxied:
// `pullRequests.list` / `listStats` are answered from the same contract-shaped fixtures the
// native snapshot scenes use (crates/t3-snapshots/fixtures/pull-requests), so the two render
// identical data. Everything else passes through to the real server.
//
//   node e2e/capture-pull-requests.mjs --state /tmp/t3ui-e2e/run-pulls/state.json \
//     --web-url http://127.0.0.1:4761 --out /tmp/pulls-reference [--only list,empty] [--themes dark]
//
// States: list (fixture), empty (the server's real answer), error (cli-unauthenticated),
// unavailable (server config rewritten to drop the capability), loading (never answered).
// Output: <out>/pull-requests-<state>-<theme>.png at 1440x900 CSS px, device scale 2.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import { mintPairingToken } from "./lib/t3-protocol.mjs";

const { values: args } = parseArgs({
  options: {
    state: { type: "string" },
    "web-url": { type: "string" },
    out: { type: "string", default: "/tmp/pulls-reference" },
    only: { type: "string" },
    themes: { type: "string", default: "dark,light" },
  },
});

const VIEWPORT = { width: 1440, height: 900 };
const FIXED_CLOCK_OFFSET_MS = 2 * 60_000;
const FIXTURES = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "../crates/t3-snapshots/fixtures",
);

function loadPlaywright() {
  const require = createRequire(import.meta.url);
  try {
    return require("playwright-core");
  } catch {
    const forkDir = process.env.T3UI_FORK_DIR ?? path.join(os.homedir(), "L-Projects/t3UI-refs/t3code-fork");
    return createRequire(path.join(forkDir, "apps/desktop/package.json"))("playwright-core");
  }
}

const state = JSON.parse(fs.readFileSync(args.state, "utf8"));
const webUrl = args["web-url"].replace(/\/$/, "");

// The fixtures' timestamps are relative to the recorded seed; shift them onto this run's.
const recordedSeed = Date.parse(
  JSON.parse(fs.readFileSync(path.join(FIXTURES, "manifest.json"), "utf8")).seededAt,
);
const shift = Date.parse(state.seededAt) - recordedSeed;
const shiftTime = (iso) => new Date(Date.parse(iso) + shift).toISOString();
const list = JSON.parse(fs.readFileSync(path.join(FIXTURES, "pull-requests/list.json"), "utf8"));
for (const entry of list.entries) {
  entry.createdAt = shiftTime(entry.createdAt);
  entry.updatedAt = shiftTime(entry.updatedAt);
}
const stats = JSON.parse(fs.readFileSync(path.join(FIXTURES, "pull-requests/stats.json"), "utf8"));

/** What the server answers for one listing input, from the `state: all` fixture. */
function narrowList(input) {
  const viewer = (host) => list.viewers[host]?.toLowerCase();
  return {
    ...list,
    entries: list.entries.filter((entry) => {
      const authored = entry.author?.login.toLowerCase() === viewer(entry.host);
      return (
        (input.state === "all" || entry.state === input.state) &&
        (input.involvement !== "reviewing" || entry.viewerReviewRequested) &&
        (input.involvement !== "authored" || authored) &&
        (input.projectId === undefined || entry.projectId === input.projectId) &&
        (input.host === undefined || entry.host === input.host)
      );
    }),
  };
}

const success = (requestId, value) => ({ _tag: "Exit", requestId, exit: { _tag: "Success", value } });
const failure = (requestId, error) => ({
  _tag: "Exit",
  requestId,
  exit: { _tag: "Failure", cause: [{ _tag: "Fail", error }] },
});

/** Per state: how a `pullRequests.*` request is answered (null passes it to the server). */
const STATES = {
  list: (message) =>
    message.tag === "pullRequests.list"
      ? success(message.id, narrowList(message.payload))
      : message.tag === "pullRequests.listStats"
        ? success(message.id, {
            stats: stats.filter((stat) =>
              message.payload.refs.some((ref) => ref.projectId === stat.projectId && ref.number === stat.number),
            ),
          })
        : null,
  empty: () => null,
  error: (message) =>
    message.tag === "pullRequests.list"
      ? failure(message.id, {
          _tag: "PullRequestUnavailableError",
          reason: "cli-unauthenticated",
          provider: "github",
        })
      : null,
  unavailable: () => null,
  loading: (message) => (message.tag === "pullRequests.list" ? "drop" : null),
};

async function proxy(context, scenario) {
  await context.routeWebSocket(/\/ws(\?|$)/, (ws) => {
    const server = ws.connectToServer();
    ws.onMessage((frame) => {
      for (const message of [].concat(JSON.parse(String(frame)))) {
        if (message._tag === "Request") {
          const answer = STATES[scenario](message);
          if (answer === "drop") continue;
          if (answer) {
            ws.send(JSON.stringify(answer));
            continue;
          }
          if (message.tag === "orchestration.dispatchCommand") {
            console.warn(`[capture] browser dispatched ${message.payload?.type}`);
          }
        }
        server.send(JSON.stringify(message));
      }
    });
    server.onMessage((frame) => {
      const text = String(frame);
      ws.send(scenario === "unavailable" ? text.replaceAll('"pullRequests":true', '"pullRequests":false') : text);
    });
  });
}

async function main() {
  const { chromium } = loadPlaywright();
  const outDir = path.resolve(args.out);
  fs.mkdirSync(outDir, { recursive: true });
  const scenarios = (args.only ?? Object.keys(STATES).join(",")).split(",");
  const browser = await chromium.launch({ headless: true });
  try {
    for (const theme of args.themes.split(",")) {
      for (const scenario of scenarios) {
        const context = await browser.newContext({
          viewport: VIEWPORT,
          deviceScaleFactor: 2,
          colorScheme: theme,
          reducedMotion: "reduce",
          locale: "en-US",
          timezoneId: "UTC",
        });
        await context.addInitScript((theme) => {
          window.localStorage.setItem("t3code:theme", theme);
        }, theme);
        await context.clock.setFixedTime(new Date(Date.parse(state.seededAt) + FIXED_CLOCK_OFFSET_MS));
        await proxy(context, scenario);
        const page = await context.newPage();
        page.on("pageerror", (error) => console.warn(`[capture] page error: ${error.message}`));
        const { credential } = await mintPairingToken(state.baseUrl, state.accessToken, {
          label: `pulls-${scenario}-${theme}`,
          scopes: state.accessTokenScopes,
        });
        await page.goto(`${webUrl}/pair#token=${credential}`);
        await page.waitForURL((url) => !url.pathname.startsWith("/pair"), { timeout: 30_000 });
        await page.goto(`${webUrl}/pull-requests`);
        await page.waitForLoadState("networkidle").catch(() => {});
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(1500);
        await page.mouse.move(VIEWPORT.width - 2, VIEWPORT.height - 2);
        const file = path.join(outDir, `pull-requests-${scenario}-${theme}.png`);
        await page.screenshot({ path: file, animations: "disabled", caret: "hide" });
        console.log(`[capture] ${file}`);
        await context.close();
      }
    }
  } finally {
    await browser.close();
  }
}

await main();
