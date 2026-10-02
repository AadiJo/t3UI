#!/usr/bin/env node
// Seed a fresh T3 Code server with deterministic projects and threads, using
// only the public client protocol (HTTP auth + Effect RPC over /ws). The
// server must run the fake Codex (e2e/fake-codex/codex) so turns are scripted.
//
//   node e2e/seed.mjs --base-url http://127.0.0.1:4710 --token <pairing token> \
//     --repos-dir /tmp/t3ui-e2e/run/repos --out /tmp/t3ui-e2e/run/state.json
//   node e2e/seed.mjs --pair   --state <state.json>   # print a fresh /pair URL
//   node e2e/seed.mjs --verify --state <state.json>   # re-check seeded states
//
// Node 22+ built-ins only. See e2e/lib/t3-protocol.mjs for the wire format.

import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { parseArgs } from "node:util";

import {
  RpcSocket,
  exchangePairingToken,
  fetchEnvironmentDescriptor,
  mintPairingToken,
} from "./lib/t3-protocol.mjs";
import { createFixtureRepos } from "./lib/fixture-repos.mjs";

const require = createRequire(import.meta.url);
const { SCENARIOS } = require("./fake-codex/scenarios.cjs");
const scenario = (id) => SCENARIOS.find((entry) => entry.id === id);

const { values: args } = parseArgs({
  options: {
    "base-url": { type: "string" },
    token: { type: "string" },
    "repos-dir": { type: "string" },
    out: { type: "string" },
    state: { type: "string" },
    model: { type: "string", default: "gpt-5.4" },
    pair: { type: "boolean", default: false },
    verify: { type: "boolean", default: false },
  },
});

// ---------------------------------------------------------------------------
// What gets seeded. Threads run strictly in this order, each waiting for its
// expected end state, so sidebar ordering is stable (most recent last).
// `expect` names a predicate in EXPECTATIONS below.
// ---------------------------------------------------------------------------

const PROJECTS = [
  { key: "aurora", id: "project-aurora" },
  { key: "borealis", id: "project-borealis" },
  { key: "cirrus", id: "project-cirrus" },
];

const THREADS = [
  { id: "thread-cirrus-summary", project: "cirrus", scenario: "summary", expect: "completed", archive: true },
  { id: "thread-cirrus-deploy", project: "cirrus", scenario: "failure", expect: "failed" },
  { id: "thread-cirrus-guide", project: "cirrus", scenario: "docs", expect: "completedWithDiff" },
  { id: "thread-borealis-ready", project: "borealis", scenario: "readiness", expect: "completedWithDiff" },
  { id: "thread-borealis-persist", project: "borealis", scenario: "question", expect: "awaitingInput" },
  { id: "thread-aurora-plan", project: "aurora", scenario: "plan", interactionMode: "plan", expect: "proposedPlan" },
  {
    id: "thread-aurora-migrate",
    project: "aurora",
    scenario: "approval",
    runtimeMode: "approval-required",
    expect: "awaitingApproval",
  },
  { id: "thread-aurora-watch", project: "aurora", scenario: "running", expect: "running" },
  { id: "thread-aurora-tour", project: "aurora", scenario: "showcase", expect: "completedWithDiff" },
];

// Predicates over (shell, detail). `shell` is the OrchestrationThreadShell from
// the shell subscription; `detail` is the full thread from the HTTP snapshot
// endpoint (fetched only when `needsDetail` is set).
const EXPECTATIONS = {
  completed: { check: ({ shell }) => shell.latestTurn?.state === "completed" },
  failed: { check: ({ shell }) => shell.latestTurn?.state === "error" },
  awaitingInput: { check: ({ shell }) => shell.hasPendingUserInput },
  awaitingApproval: { check: ({ shell }) => shell.hasPendingApprovals },
  proposedPlan: {
    check: ({ shell }) => shell.latestTurn?.state === "completed" && shell.hasActionableProposedPlan,
  },
  // Running and visibly mid-command (the fake parks inside `pnpm test --watch`).
  running: {
    needsDetail: true,
    check: ({ shell, detail }) =>
      shell.latestTurn?.state === "running" && detail.activities.some((activity) => activity.kind.startsWith("tool.")),
  },
  // Completed and the checkpoint diff is computed with a file summary, so the
  // diff panel has data (files stay empty when the pre-turn baseline was missed).
  completedWithDiff: {
    needsDetail: true,
    check: ({ shell, detail }) =>
      shell.latestTurn?.state === "completed" &&
      detail.checkpoints.some(
        (checkpoint) =>
          checkpoint.turnId === shell.latestTurn.turnId && checkpoint.status === "ready" && checkpoint.files.length > 0,
      ),
  },
};

// ---------------------------------------------------------------------------

const log = (...parts) => console.log("[seed]", ...parts);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
let commandCounter = 0;
const nextCommandId = () => `seed-cmd-${String(++commandCounter).padStart(3, "0")}`;
const now = () => new Date().toISOString();

/** Poll `check` until it returns a truthy value or the timeout elapses. */
async function waitFor(description, check, { timeoutMs = 30_000, intervalMs = 150 } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const result = await check();
      if (result) return result;
    } catch (error) {
      lastError = error;
    }
    await sleep(intervalMs);
  }
  throw new Error(`timed out waiting for ${description}${lastError ? ` (last error: ${lastError.message})` : ""}`);
}

/**
 * Keep a live map of thread shells from `orchestration.subscribeShell`.
 * Stream items: first {kind:"snapshot", snapshot:{projects, threads, snapshotSequence}},
 * then {kind:"project-upserted"|"thread-upserted"|"project-removed"|"thread-removed", sequence, ...}.
 * Archived threads are removed from this (active) shell stream.
 */
function watchShell(rpc) {
  const projects = new Map();
  const threads = new Map();
  const subscription = rpc.subscribe("orchestration.subscribeShell", {}, (item) => {
    switch (item.kind) {
      case "snapshot":
        for (const project of item.snapshot.projects) projects.set(project.id, project);
        for (const thread of item.snapshot.threads) threads.set(thread.id, thread);
        break;
      case "project-upserted":
        projects.set(item.project.id, item.project);
        break;
      case "project-removed":
        projects.delete(item.projectId);
        break;
      case "thread-upserted":
        threads.set(item.thread.id, item.thread);
        break;
      case "thread-removed":
        threads.delete(item.threadId);
        break;
    }
  });
  return { projects, threads, close: subscription.close };
}

/** GET /api/orchestration/threads/:id (Bearer auth) -> { snapshotSequence, thread }. */
async function fetchThreadDetail(baseUrl, accessToken, threadId) {
  const response = await fetch(new URL(`/api/orchestration/threads/${encodeURIComponent(threadId)}`, baseUrl), {
    headers: { authorization: `Bearer ${accessToken}` },
  });
  if (!response.ok) throw new Error(`thread snapshot ${threadId}: HTTP ${response.status}`);
  return (await response.json()).thread;
}

async function waitForExpectation(ctx, threadId, expectName, timeoutMs = 60_000) {
  const expectation = EXPECTATIONS[expectName];
  return waitFor(
    `${threadId} to be ${expectName}`,
    async () => {
      const shell = ctx.shell.threads.get(threadId);
      if (!shell) return false;
      const detail = expectation.needsDetail ? await fetchThreadDetail(ctx.baseUrl, ctx.accessToken, threadId) : null;
      return expectation.check({ shell, detail }) ? shell : false;
    },
    { timeoutMs },
  );
}

function dispatch(rpc, command) {
  // orchestration.dispatchCommand payload is the command itself; Success is { sequence }.
  return rpc.call("orchestration.dispatchCommand", { commandId: nextCommandId(), ...command });
}

// Truncation the web composer applies before using the first message as a title.
const draftTitle = (text) => (text.length > 50 ? `${text.slice(0, 47)}...` : text);

async function seed() {
  for (const flag of ["base-url", "token", "repos-dir", "out"]) {
    if (!args[flag]) throw new Error(`--${flag} is required`);
  }
  const baseUrl = args["base-url"];
  const descriptor = await fetchEnvironmentDescriptor(baseUrl);
  log(`server ${descriptor.serverVersion} (${descriptor.environmentId}) at ${baseUrl}`);

  // 1. Auth: one-time pairing credential -> bearer token -> WS ticket -> /ws.
  const auth = await exchangePairingToken(baseUrl, args.token);
  log(`authenticated with scopes: ${auth.scopes.join(" ")}`);
  const rpc = await RpcSocket.connect(baseUrl, auth.accessToken);

  // 2. server.getConfig is what clients call first; check the fake Codex is live.
  const config = await rpc.call("server.getConfig", {});
  const codex = config.providers.find((provider) => provider.instanceId === "codex");
  await waitFor("codex provider to report ready with models", async () => {
    const latest = (await rpc.call("server.getConfig", {})).providers.find((p) => p.instanceId === "codex");
    return latest?.status === "ready" && latest.models.length > 0;
  }).catch((error) => {
    throw new Error(`${error.message}; codex status=${codex?.status} message=${codex?.message}`);
  });
  const modelSelection = { instanceId: "codex", model: args.model };
  log(`codex provider ready, using ${args.model}`);

  // 3. Fixture repos + projects.
  const repos = createFixtureRepos(path.resolve(args["repos-dir"]));
  const shell = watchShell(rpc);
  const ctx = { baseUrl, accessToken: auth.accessToken, shell };
  for (const project of PROJECTS) {
    const repo = repos.find((entry) => entry.key === project.key);
    await dispatch(rpc, {
      type: "project.create",
      projectId: project.id,
      title: repo.title,
      workspaceRoot: repo.path,
      createdAt: now(),
    });
    project.title = repo.title;
    project.path = repo.path;
  }
  await waitFor("projects in shell stream", () => PROJECTS.every((project) => shell.projects.has(project.id)));
  log(`created ${PROJECTS.length} projects: ${PROJECTS.map((project) => project.title).join(", ")}`);

  // 4. Threads, created the way the web composer does it: a single
  //    thread.turn.start carrying bootstrap.createThread. The server then asks
  //    Codex (`codex exec`) for a title and renames the thread.
  for (const thread of THREADS) {
    const spec = scenario(thread.scenario);
    const project = PROJECTS.find((entry) => entry.key === thread.project);
    const runtimeMode = thread.runtimeMode ?? "full-access";
    const interactionMode = thread.interactionMode ?? "default";
    const createdAt = now();
    await dispatch(rpc, {
      type: "thread.turn.start",
      threadId: thread.id,
      message: { messageId: `${thread.id}-msg-1`, role: "user", text: spec.prompt, attachments: [] },
      modelSelection,
      titleSeed: draftTitle(spec.prompt),
      runtimeMode,
      interactionMode,
      bootstrap: {
        createThread: {
          projectId: project.id,
          title: draftTitle(spec.prompt),
          modelSelection,
          runtimeMode,
          interactionMode,
          branch: "main",
          worktreePath: null,
          createdAt,
        },
      },
      createdAt,
    });
    await waitForExpectation(ctx, thread.id, thread.expect);
    // Title generation runs asynchronously after the first turn starts.
    await waitFor(`${thread.id} title`, () => shell.threads.get(thread.id)?.title === spec.title, {
      timeoutMs: 10_000,
    }).catch(() => log(`warning: ${thread.id} kept title "${shell.threads.get(thread.id)?.title}"`));
    if (thread.archive) {
      await dispatch(rpc, { type: "thread.archive", threadId: thread.id });
      await waitFor(`${thread.id} archived`, () => !shell.threads.has(thread.id));
    }
    log(`${thread.id}: ${thread.expect}${thread.archive ? " + archived" : ""}`);
  }

  // 5. Hand-off state for capture scripts / the native app.
  const state = {
    baseUrl,
    serverVersion: descriptor.serverVersion,
    environmentId: descriptor.environmentId,
    accessToken: auth.accessToken,
    accessTokenScopes: auth.scopes,
    model: args.model,
    projects: PROJECTS.map(({ key, id, title, path: workspaceRoot }) => ({ key, id, title, workspaceRoot })),
    threads: THREADS.map((thread) => ({
      id: thread.id,
      projectId: PROJECTS.find((entry) => entry.key === thread.project).id,
      scenario: thread.scenario,
      prompt: scenario(thread.scenario).prompt,
      title: scenario(thread.scenario).title,
      expect: thread.expect,
      archived: Boolean(thread.archive),
      runtimeMode: thread.runtimeMode ?? "full-access",
      interactionMode: thread.interactionMode ?? "default",
    })),
    seededAt: now(),
  };
  fs.writeFileSync(args.out, JSON.stringify(state, null, 2) + "\n");
  log(`wrote ${args.out}`);
  shell.close();
  rpc.close();
}

function readState() {
  if (!args.state) throw new Error("--state is required");
  return JSON.parse(fs.readFileSync(args.state, "utf8"));
}

/** Re-check every seeded thread against its expectation (archived ones via the archived snapshot). */
async function verify() {
  const state = readState();
  const rpc = await RpcSocket.connect(state.baseUrl, state.accessToken);
  const shell = watchShell(rpc);
  const ctx = { baseUrl: state.baseUrl, accessToken: state.accessToken, shell };
  await waitFor("shell snapshot", () => shell.threads.size > 0);
  const archived = await rpc.call("orchestration.getArchivedShellSnapshot", {});
  let failures = 0;
  for (const thread of state.threads) {
    try {
      if (thread.archived) {
        if (!archived.threads.some((entry) => entry.id === thread.id)) throw new Error("not in archived snapshot");
      } else {
        await waitForExpectation(ctx, thread.id, thread.expect, 5_000);
      }
      log(`ok   ${thread.id} (${thread.archived ? "archived" : thread.expect})`);
    } catch (error) {
      failures += 1;
      log(`FAIL ${thread.id}: ${error.message}`);
    }
  }
  shell.close();
  rpc.close();
  if (failures > 0) throw new Error(`${failures} thread(s) failed verification`);
}

/** Mint a one-time pairing credential (5 min TTL) for a browser or the native app. */
async function pair() {
  const state = readState();
  const issued = await mintPairingToken(state.baseUrl, state.accessToken, { label: "t3ui-e2e" });
  const url = new URL("/pair", state.baseUrl);
  url.hash = new URLSearchParams({ token: issued.credential }).toString();
  console.log(JSON.stringify({ credential: issued.credential, expiresAt: issued.expiresAt, pairingUrl: url.toString() }));
}

const main = args.pair ? pair : args.verify ? verify : seed;
main().then(
  () => process.exit(0),
  (error) => {
    console.error("[seed] failed:", error.message);
    process.exit(1);
  },
);
