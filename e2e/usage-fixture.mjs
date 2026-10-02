#!/usr/bin/env node
// Records real Usage page data from an e2e server.
//
// The fake Codex writes no transcripts, so a fresh server reports empty usage. This writes
// deterministic Codex rollouts and Claude Code transcripts into the run's isolated HOME
// (the server scans `~/.codex/sessions` and `~/.claude/projects` there), then asks the server
// for `server.getUsageSummary` over the Usage page's windows and saves the answers next to its
// server config.
//
//   e2e/run-local.sh up --server nightly --detach            (T3UI_RUN_DIR / T3UI_PORT to isolate)
//   node e2e/usage-fixture.mjs --run /tmp/t3ui-e2e/run-nightly --out crates/t3-snapshots/fixtures/usage
//
// Writes `<out>/summary-30d.json`, `summary-24h.json`, `server-config.json` and `meta.json`
// (`now` and the time zone the windows were computed in). Never touches the developer's HOME.
import fs from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";

import { RpcSocket } from "./lib/t3-protocol.mjs";

const { values: args } = parseArgs({
  options: {
    run: { type: "string" },
    out: { type: "string" },
    "time-zone": { type: "string", default: "UTC" },
  },
});
if (!args.run || !args.out) throw new Error("--run and --out are required");

const state = JSON.parse(fs.readFileSync(path.join(args.run, "state.json"), "utf8"));
const home = path.join(args.run, "userhome");
const HOUR = 3_600_000;
const DAY = 24 * HOUR;
const now = Date.now();

/** mulberry32: a small deterministic PRNG so every run writes the same transcripts. */
function prng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const random = prng(20261002);
const between = (low, high) => Math.round(low + random() * (high - low));
const uuid = (n) => `00000000-0000-4000-8000-${n.toString(16).padStart(12, "0")}`;

// A working-hours rhythm: weekdays busier, a quiet stretch mid-window, Claude sessions sparser.
let session = 0;
const codexModels = ["gpt-5", "gpt-5", "gpt-5-mini"];
const claudeModels = ["claude-sonnet-4-5", "claude-sonnet-4-5", "claude-opus-4-1"];

function writeCodexSession(startMs, turns) {
  session += 1;
  const id = uuid(session);
  const day = new Date(startMs);
  const dir = path.join(
    home,
    ".codex/sessions",
    String(day.getUTCFullYear()),
    String(day.getUTCMonth() + 1).padStart(2, "0"),
    String(day.getUTCDate()).padStart(2, "0"),
  );
  fs.mkdirSync(dir, { recursive: true });
  const model = codexModels[session % codexModels.length];
  const lines = [
    { timestamp: new Date(startMs).toISOString(), type: "session_meta", payload: { id, cwd: "/tmp/aurora" } },
    { timestamp: new Date(startMs).toISOString(), type: "turn_context", payload: { model } },
  ];
  for (let turn = 0; turn < turns; turn += 1) {
    const at = startMs + (turn + 1) * between(40_000, 240_000);
    const input = between(8_000, 60_000);
    const cached = Math.round(input * (0.5 + random() * 0.4));
    const output = between(400, 6_000);
    lines.push({
      timestamp: new Date(at).toISOString(),
      type: "event_msg",
      payload: {
        type: "token_count",
        info: {
          last_token_usage: {
            input_tokens: input,
            cached_input_tokens: cached,
            output_tokens: output,
            reasoning_output_tokens: Math.round(output * 0.4),
            total_tokens: input + output,
          },
        },
      },
    });
  }
  const name = `rollout-${new Date(startMs).toISOString().replace(/[:.]/g, "-")}-${id}.jsonl`;
  fs.writeFileSync(path.join(dir, name), lines.map((line) => JSON.stringify(line)).join("\n") + "\n");
}

function writeClaudeSession(startMs, turns) {
  session += 1;
  const id = uuid(session);
  const dir = path.join(home, ".claude/projects/-tmp-aurora");
  fs.mkdirSync(dir, { recursive: true });
  const model = claudeModels[session % claudeModels.length];
  const lines = [];
  for (let turn = 0; turn < turns; turn += 1) {
    const at = startMs + (turn + 1) * between(30_000, 200_000);
    lines.push({
      type: "assistant",
      timestamp: new Date(at).toISOString(),
      sessionId: id,
      requestId: `req_${session}_${turn}`,
      message: {
        id: `msg_${session}_${turn}`,
        model,
        usage: {
          input_tokens: between(20, 400),
          cache_read_input_tokens: between(10_000, 80_000),
          cache_creation_input_tokens: between(500, 8_000),
          output_tokens: between(200, 3_000),
        },
      },
    });
  }
  fs.writeFileSync(path.join(dir, `${id}.jsonl`), lines.map((line) => JSON.stringify(line)).join("\n") + "\n");
}

// 30 days back from now: activity on most days, heavier on weekdays.
for (let daysAgo = 29; daysAgo >= 0; daysAgo -= 1) {
  const dayStart = Math.floor((now - daysAgo * DAY) / DAY) * DAY;
  const weekday = new Date(dayStart).getUTCDay();
  const busy = weekday !== 0 && weekday !== 6;
  if (daysAgo >= 12 && daysAgo <= 14) continue; // a quiet stretch
  const codexSessions = busy ? between(2, 5) : between(0, 1);
  for (let index = 0; index < codexSessions; index += 1) {
    const start = dayStart + between(9, 19) * HOUR + between(0, 50) * 60_000;
    if (start < now - HOUR) writeCodexSession(start, between(3, 12));
  }
  const claudeSessions = busy ? between(0, 3) : 0;
  for (let index = 0; index < claudeSessions; index += 1) {
    const start = dayStart + between(10, 18) * HOUR + between(0, 50) * 60_000;
    if (start < now - HOUR) writeClaudeSession(start, between(2, 8));
  }
}
// The past 24 hours get a few more, so the hourly view has a shape.
for (let hoursAgo = 22; hoursAgo >= 2; hoursAgo -= 3) {
  writeCodexSession(now - hoursAgo * HOUR, between(2, 6));
  if (hoursAgo % 2 === 0) writeClaudeSession(now - hoursAgo * HOUR + 20 * 60_000, between(2, 5));
}

/** The Usage page's request for `days` ending now (`makeWindow`), in UTC. */
function window(days) {
  const day = (ms) => new Date(ms).toISOString().slice(0, 10);
  if (days === 1) {
    const until = Math.floor(now / 60_000) * 60_000;
    const since = until - DAY;
    return {
      sinceDay: day(since),
      untilDay: day(until),
      timeZone: args["time-zone"],
      resolution: "hour",
      sinceTime: new Date(since).toISOString(),
      untilTime: new Date(until).toISOString(),
    };
  }
  return { sinceDay: day(now - (days - 1) * DAY), untilDay: day(now), timeZone: args["time-zone"], resolution: "day" };
}

const rpc = await RpcSocket.connect(state.baseUrl, state.accessToken);
try {
  await rpc.call("server.refreshUsageRates", {}).catch((error) => console.warn(`rates: ${error.message}`));
  const monthly = await rpc.call("server.getUsageSummary", window(30));
  const daily = await rpc.call("server.getUsageSummary", window(1));
  const config = await new Promise((resolve, reject) => {
    const stream = rpc.subscribe("subscribeServerConfig", { usageLimitSources: true }, (item) => {
      if (item.type === "snapshot") {
        stream.close();
        resolve(item.config);
      }
    });
    stream.done.catch(reject);
  });
  fs.mkdirSync(args.out, { recursive: true });
  const write = (name, value) => fs.writeFileSync(path.join(args.out, name), JSON.stringify(value, null, 2) + "\n");
  write("summary-30d.json", monthly);
  write("summary-24h.json", daily);
  write("server-config.json", config);
  write("meta.json", {
    now: new Date(now).toISOString(),
    timeZone: args["time-zone"],
    environmentId: state.environmentId,
    sessionsWritten: session,
  });
  console.log(
    `[usage-fixture] ${monthly.buckets.length} daily buckets, ${daily.buckets.length} hourly buckets, ` +
      `pricing ${monthly.pricing.status}, sources ${monthly.sources.map((s) => `${s.fingerprint.provider}:${s.status}`).join(" ")}`,
  );
} finally {
  rpc.close();
}
