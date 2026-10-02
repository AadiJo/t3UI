# End-to-end harness

Brings up a real T3 Code server with a scripted fake Codex, seeds it with
deterministic projects and threads over the public protocol, and captures
reference screenshots of the fork's web UI. Nothing here uses real provider
credentials, touches `~/.t3`, `~/.codex`, or the daily-driver server on :3333.

| File | What it is |
| --- | --- |
| `run-local.sh` | One command: isolated dirs, settings, server, (web), seed, (capture), teardown |
| `fake-codex/codex` | Fake `codex` CLI speaking the Codex app-server JSON-RPC protocol (Node, zero deps) |
| `fake-codex/scenarios.cjs` | Scripted turns keyed by prompt text |
| `seed.mjs` | Seeds projects/threads via HTTP auth + Effect RPC over `/ws`; also `--pair` and `--verify` |
| `lib/t3-protocol.mjs` | The client protocol (auth exchange, RPC socket), commented as a wire reference |
| `lib/fixture-repos.mjs` | Three deterministic git repos used as project workspaces |
| `capture-web-reference.mjs` | Playwright capture of the fork web UI into `docs/reference/` |

Verified against the fork `t3@0.0.28` (source, `~/L-Projects/t3code-again`) and
`t3@0.0.45-nightly.20261002.2561` (npm), on 2026-10-01.

## Quick start

```bash
e2e/run-local.sh smoke --server nightly          # what CI runs: up, seed, verify, down
e2e/run-local.sh smoke --server fork             # same against the fork from source
e2e/run-local.sh up --server fork --capture      # fresh seed + docs/reference/*.png, then down
e2e/run-local.sh up --server nightly --detach    # leave it running for the native app
node e2e/seed.mjs --pair --state /tmp/t3ui-e2e/run-nightly/state.json   # fresh pairing URL (5 min TTL)
e2e/run-local.sh down --server nightly
```

`state.json` in the run dir holds `baseUrl`, a reusable bearer `accessToken`
(30 days, administrative scopes), the environment id and every seeded
project/thread id. A native client can skip pairing and use the bearer directly.

Defaults: nightly on `127.0.0.1:4710`, fork on `:4720` with Vite on `:4721`,
run dir `/tmp/t3ui-e2e/run-<server>`. Override with `T3UI_PORT`,
`T3UI_WEB_PORT`, `T3UI_RUN_DIR`, `T3UI_FORK_DIR`, `T3_NIGHTLY_VERSION`,
`T3UI_CAPTURE_OUT`. Requirements: Node 22+, git, bash, curl; `vp` on PATH and
the fork's `node_modules` for `--server fork`; network for the first `npx`.

## Run recipes (what `run-local.sh` does)

Both servers take the same flags. `serve` is the headless mode: it never opens
a browser, skips the auto-bootstrap project, and prints the pairing details.

| Flag | Env | Meaning |
| --- | --- | --- |
| `--base-dir <dir>` | `T3CODE_HOME` | All state. Runtime data goes in `<dir>/userdata`, or `<dir>/dev` when `--dev-url` is set |
| `--port <n>` | `T3CODE_PORT` | HTTP + WebSocket port |
| `--host <ip>` | `T3CODE_HOST` | Bind address. Loopback gives auth policy `loopback-browser`; anything else is `remote-reachable` |
| `--dev-url <url>` | `VITE_DEV_SERVER_URL` | Redirect web routes to a Vite dev server (fork web UI) |
| `--no-browser` | `T3CODE_NO_BROWSER` | Implied by `serve` |
| | `T3CODE_TELEMETRY_ENABLED=false` | No PostHog |

`serve` prints:

```
T3 Code server is ready.
Connection string: http://127.0.0.1:4710
Token: QFVPMVRAX3ZF
Pairing URL: http://127.0.0.1:4710/pair#token=QFVPMVRAX3ZF
```

The token is a one-time credential with administrative scopes and a 5 minute
TTL. More can be minted offline with `t3 auth pairing create --base-dir <dir> --json`
(standard client scopes), or online with `node e2e/seed.mjs --pair`.

### Nightly (headless, what CI uses)

```bash
RUN=/tmp/t3ui-e2e/run-nightly
mkdir -p $RUN/home/userdata $RUN/userhome $RUN/cwd
# write $RUN/home/userdata/settings.json (below), then:
cd $RUN/cwd && env -i PATH="$PWD/e2e/fake-codex:$PATH" HOME=$RUN/userhome T3CODE_TELEMETRY_ENABLED=false \
  npm_config_cache=/tmp/t3ui-e2e/npm-cache \
  npx -y t3@0.0.45-nightly.20261002.2561 serve --base-dir $RUN/home --host 127.0.0.1 --port 4710
```

Use a private `npm_config_cache`: `npx t3@nightly` with the default cache
installs into the same `~/.npm/_npx/<hash>` directory the daily driver runs from.

### Fork from source (server + web UI)

```bash
RUN=/tmp/t3ui-e2e/run-fork; FORK=~/L-Projects/t3code-again
# server (Node 24 runs the TypeScript directly)
cd $RUN/cwd && env -i PATH="$PATH" HOME=$RUN/userhome T3CODE_TELEMETRY_ENABLED=false \
  node $FORK/apps/server/src/bin.ts serve --base-dir $RUN/home --host 127.0.0.1 --port 4720 \
  --dev-url http://127.0.0.1:4721
# web UI
cd $FORK/apps/web && PORT=4721 HOST=127.0.0.1 VITE_HTTP_URL=http://127.0.0.1:4720 \
  VITE_WS_URL=ws://127.0.0.1:4720 VITE_DEV_SERVER_URL=http://127.0.0.1:4721 vp dev
```

`VITE_DEV_SERVER_URL` is required: with it the web app sends HTTP through the
Vite proxy (same origin), so the browser-session cookie set by
`/api/auth/browser-session` is stored. Without it the app calls the server
cross-origin, the server answers `access-control-allow-origin: *`, the browser
drops the cookie, and `/ws` fails with "no valid credentials". The fork's
`vp run dev --home-dir <dir> [--port N] [--dev-url URL]` wires the same env
but picks ports from 13773/5733 plus `T3CODE_PORT_OFFSET`. Dev mode labels the
app "T3 Code (Dev)".

### Server settings

`run-local.sh` writes this to both `home/userdata/settings.json` and
`home/dev/settings.json` before boot:

```json
{
  "enableProviderUpdateChecks": false,
  "providers": {
    "codex": { "binaryPath": "<repo>/e2e/fake-codex/codex", "homePath": "<run>/userhome/.codex" },
    "claudeAgent": { "enabled": false }, "cursor": { "enabled": false },
    "grok": { "enabled": false }, "opencode": { "enabled": false }, "antigravity": { "enabled": false }
  }
}
```

`providers.codex.binaryPath` is the legacy single-instance setting; both
versions still hydrate the default `codex` instance from it (the newer
`providerInstances` map is optional). `homePath` becomes `CODEX_HOME` for the
fake. Unknown keys (`antigravity` on the fork) are ignored.

## Fake Codex

T3 spawns `binaryPath` directly (no shell), so `fake-codex/codex` is an
executable Node script. It handles:

- `codex app-server`: newline-delimited JSON-RPC on stdio (no `jsonrpc` field).
  Status probe (`initialize`, `account/read`, `skills/list`, `model/list`,
  upstream also `account/rateLimits/read`), then per thread session
  `thread/start|resume`, `config/mcpServer/reload`, `turn/start`, `turn/interrupt`,
  plus `thread/read`, `thread/rollback` (fork), `thread/turns/list`,
  `thread/revert`, `thread/compact/start`, `feedback/upload` (upstream).
- `codex exec ... --output-schema F --output-last-message O -`: thread title
  (and commit/PR/branch text) generation. Writes JSON matching the schema's keys.
- `codex --version` / `codex app-server --help`: upstream probes `codex` on
  PATH at boot, so `run-local.sh` puts `e2e/fake-codex` first on PATH.

It reports `codex-cli 0.159.0`, a ChatGPT Pro account
(`fake-codex@t3ui.invalid`), models `gpt-5.4`, `gpt-5.4-mini`, `gpt-6-astra`,
`gpt-6-luna` (both versions' defaults), and one skill (`changelog`).
`FAKE_CODEX_LOG=<file>` records every message as JSONL (run-local puts it in
`logs/fake-codex.jsonl`); `FAKE_CODEX_SPEED` scales pacing (0 = no delays, but
see the checkpoint race below).

| Scenario | Prompt (substring match) | What the turn does | Seeded end state |
| --- | --- | --- | --- |
| `showcase` | "Give me a tour of this repo, then make formatBytes handle megabytes and gigabytes." | commentary, reasoning, todo list, `ls`/`cat`/`rg`, web search, MCP call, real edits to 2 files, `node --test`, long markdown answer (headings, table, lists, task list, quote, ts/bash/rust code), token usage | completed with diff |
| `approval` | "Run the database migration against the local dev database." | `ls migrations`, then `item/commandExecution/requestApproval` for `pnpm db:migrate` | pending approval (`approval-required` mode) |
| `plan` | "Plan the move to a pnpm monorepo with a shared ui package." | reasoning, `cat package.json`, a `plan` item | proposed plan (plan mode) |
| `running` | "Watch the test suite and fix failures as they come in." | starts `pnpm test --watch` and parks until `turn/interrupt` | running |
| `question` | "Set up persistence for the health checks." | `item/tool/requestUserInput` with two options | awaiting input |
| `readiness` | "Add a readiness endpoint next to /health." | edits `src/health.rs`, `cargo check` | completed with diff |
| `docs` | "Tighten up the deployment guide." | edits `docs/deploy.md` | completed with diff |
| `summary` | "Summarize the README." | one message | completed, then archived |
| `failure` | "Deploy the docs to production." | failing command, `turn/completed` status `failed` | error |
| fallback | anything else | lists the scripted prompts (plan mode: a tiny plan) | |

Fork vs nightly differences the fake absorbs:

- Upstream requires `projectId` on Thread objects and `isBlocking` on
  `requestUserInput`; the fork requires `email` to be a string and `planType`
  from its enum (`pro`). The fake satisfies both. Every emitted message was
  decoded against both versions' generated schemas.
- Reasoning deltas render only on nightly; the fork drops them.
- The fork stops reading the fake's stdout while an approval or question is
  pending, so nothing sent after a request is seen until it is answered.
- Upstream title generation asks for `{title, needsRefinement}`; the fork for `{title}`.
- Nightly's model manifest marks `gpt-5.4*` as legacy; the seed still uses `gpt-5.4`.
- Diffs are not taken from the protocol. Both servers snapshot the workspace
  with git (`refs/t3/checkpoints/...`) at turn start and end, so the fake
  writes real files. The pre-turn baseline is captured asynchronously; a fake
  that edits files within ~50 ms of `turn/started` loses the diff (the fork
  logs "checkpoint capture missing pre-turn baseline"). The fake waits 600 ms
  (scaled by `FAKE_CODEX_SPEED`) before every scenario.

## Seed

```bash
node e2e/seed.mjs --base-url http://127.0.0.1:4710 --token <serve token> \
  --repos-dir <run>/repos --out <run>/state.json [--model gpt-5.4]
```

Creates three repos (`aurora-web` TypeScript, `borealis-api` Rust,
`cirrus-docs` MkDocs; fixed authors and dates, each with an uncommitted
change), three projects, and nine threads, one at a time in this order so the
sidebar order is stable: cirrus summary (archived), cirrus deploy (error),
cirrus guide (diff), borealis readiness (diff), borealis persistence
(question), aurora plan (plan mode), aurora migration (approval), aurora watch
(running), aurora tour (showcase, most recent). Threads are created the way the
web composer does it: one `thread.turn.start` with `bootstrap.createThread`.
The server then calls `codex exec` and renames each thread to the scenario
title. The seed expects a fresh server (fixed ids).

## Protocol notes for a native client

Auth (identical in 0.0.28 and nightly):

1. `GET /.well-known/t3/environment` (no auth): `environmentId`, `serverVersion`, capabilities.
2. `POST /oauth/token`, **form-urlencoded**: `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`,
   `subject_token=<pairing credential>`,
   `subject_token_type=urn:t3:params:oauth:token-type:environment-bootstrap`,
   `requested_token_type=urn:ietf:params:oauth:token-type:access_token`
   (optional `client_label`, `scope`). Returns `{access_token, token_type:"Bearer", expires_in, scope}`.
   Credentials are single use and expire after 5 minutes.
3. `POST /api/auth/websocket-ticket` with `Authorization: Bearer <token>`: `{ticket, expiresAt}` (5 min).
4. Connect `ws://host:port/ws?wsTicket=<ticket>`. A rejected upgrade is an HTTP
   401 (`1006` in WebSocket terms). Cookies or a Bearer header on the upgrade
   also work if your WebSocket library can send them.
5. `GET /api/auth/session` tells you whether the current credential is valid
   and lists the server's auth policy and cookie name.

RPC (Effect `RpcSerialization.layerJson`): one JSON value per text frame, an
object or an array of objects. Client sends
`{"_tag":"Request","id":"1","tag":"server.getConfig","payload":{},"headers":[]}`
with ids as decimal strings. Unary results come back as
`{"_tag":"Exit","requestId":"1","exit":{"_tag":"Success","value":...}}` or
`{"_tag":"Failure","cause":[{"_tag":"Fail","error":{...}}]}`. Streams send
`{"_tag":"Chunk","requestId","values":[...]}` and **block until the client
replies `{"_tag":"Ack","requestId"}`**; a client that never acks gets one chunk
and then silence. Cancel with `{"_tag":"Interrupt","requestId"}`. `{"_tag":"Ping"}`
gets `{"_tag":"Pong"}`; the server never pings first. Unknown tags or bad
payloads fail that request with a `Die` defect string, not a `Fail`.

Methods the seed uses: `server.getConfig`, `orchestration.dispatchCommand`
(payload is the command itself, returns `{sequence}`),
`orchestration.subscribeShell` (first item `{kind:"snapshot"}`, then
`project-upserted|thread-upserted|...-removed`; archived threads leave this
stream), `orchestration.getArchivedShellSnapshot`. Full thread detail (messages,
activities, checkpoints) is also available over HTTP at
`GET /api/orchestration/threads/:threadId`. Command shapes for `project.create`,
`thread.turn.start`, `thread.archive`, `thread.approval.respond` and
`thread.user-input.respond` are the same in both versions; nightly only adds
optional fields and new commands.

## GitHub Actions

CI cannot reach the private fork, so it runs the pinned nightly. Bump
`DEFAULT_NIGHTLY_VERSION` in `run-local.sh` (or set `T3_NIGHTLY_VERSION`)
deliberately; `t3@nightly` moves daily.

```yaml
- uses: actions/setup-node@v4
  with: { node-version: 24 }
- run: e2e/run-local.sh smoke --server nightly
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: e2e-nightly
    path: |
      /tmp/t3ui-e2e/run-nightly/state.json
      /tmp/t3ui-e2e/run-nightly/logs/
```

`smoke` exits non-zero if seeding or `seed.mjs --verify` fails. The artifact
has the server log and `fake-codex.jsonl` (every JSON-RPC message in and out),
which is usually enough to see why a turn did not land.

## Known variability

- The environment label comes from `/etc/machine-info` or the hostname (no
  override); it differs per machine. Mask it or run in a container with a fixed hostname.
- "Worked for 2.0s"-style durations depend on real timing (±0.1 s).
- Relative times are pinned in captures (page clock = `seededAt` + 2 min).
- Glyphs outside DM Sans/JetBrains Mono (e.g. `✔` in test output) use system fallback fonts.
- Captures should run against a fresh seed: each capture adds paired-client
  rows to Settings > Connections, and the terminal capture leaves a shell running.
