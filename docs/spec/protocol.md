# T3 Code wire protocol (for the Rust client)

Reference for the Rust networking and state layer (serde + tokio-tungstenite + reqwest). It describes what upstream T3 Code speaks on the wire, verified against source and against real encodes produced by the same `effect` build the server uses.

## 0. Sources, versions, conventions

| Name | Path | Role |
|---|---|---|
| `U` | `~/L-Projects/t3UI-refs/t3code-upstream` (pingdotgg/t3code `b33eda13`, v0.0.44) | Wire truth. Matches `npx t3@nightly` (0.0.45-nightly). |
| `F` | `~/L-Projects/t3code-again` (fork, v0.0.28) | UI reference only. Its client speaks an older protocol. |
| `E` | `effect@4.0.0-rc.115` (`U/pnpm-lock.yaml`, patched by `U/patches/effect@4.0.0-rc.115.patch`) | RPC framing and Schema codec used by upstream. Unpacked copy for reading: `npm pack effect@4.0.0-rc.115`. Pointers like `E:rpc/RpcServer.ts:396` are relative to `package/src/unstable/`. |
| `Ef` | `F/node_modules/.pnpm/effect@4.0.0-beta.78_*/node_modules/effect` | Fork's effect. Differences noted where they matter. |

All `path:line` pointers are into `U` unless prefixed with `F:` or `E:`.

Type notation in this doc is the JSON wire shape written as TypeScript:

- `k: T` means the key is always present.
- `k?: T` means the key may be absent. If the type also says `| null`, the server accepts `null` as "absent" and may itself send `null`. If it does not, sending `null` is a decode error.
- `string` covers every branded id (`ThreadId`, `ProjectId`, `CommandId`, ...) and every ISO-8601 timestamp (`IsoDateTime` is a plain `Schema.String`, `packages/contracts/src/baseSchemas.ts:40`).
- Named types are defined in Appendix A, which is generated from the upstream schemas (generator in Appendix B).

## 1. Effect RPC over WebSocket

### 1.1 Transport

- One WebSocket per environment, `GET /ws` (`apps/server/src/ws.ts:4112`). URL and auth are in section 3.
- Server serialization is `RpcSerialization.layerJson` (`apps/server/src/ws.ts:4151`); the client uses the same (`packages/client-runtime/src/rpc/session.ts:205`). No subprotocol and no custom upgrade headers.
- Text frames. Each frame is exactly one JSON value (`E:rpc/RpcSerialization.ts:122-133`):
  - an object is one message,
  - an array is several messages, processed in order.
- The server only ever writes one object per frame. It never batches (verified, see 1.9).
- The client may batch (send an array). Upstream's client never does; the Rust client does not need to.
- Every request is multiplexed over this socket by `id` / `requestId`. There is no HTTP fallback for RPC (the HTTP endpoints in section 3.4 are separate).

### 1.2 Client to server messages

Source: `E:rpc/RpcMessage.ts:34-160`, client encoder `E:rpc/RpcClient.ts:669-717`.

```jsonc
// Request
{"_tag":"Request","id":"7","tag":"orchestration.subscribeThread","payload":{...},"headers":[]}
// optional extra keys the TS client adds because tracing is on (server runs with disableTracing: true and ignores them):
//   "traceId":"<hex>","spanId":"<hex>","sampled":true

// Ack: one per received Chunk frame of a stream request (see 1.5)
{"_tag":"Ack","requestId":"7"}

// Interrupt: cancel a running request (unary or stream)
{"_tag":"Interrupt","requestId":"7"}

// Ping: keepalive (see 1.6)
{"_tag":"Ping"}

// Eof: exists in the protocol. DO NOT SEND (see 1.8).
{"_tag":"Eof"}
```

- `payload` is the method's payload schema encoded with the JSON codec (section 2). Methods with an empty payload struct take `{}`.
- `headers` is an array of `[name, value]` string pairs. rc.115 tolerates the key being absent; always send `[]`. The server prepends the HTTP upgrade request's headers to every request's headers (`E:rpc/RpcServer.ts:1553-1555`), so you never need to repeat auth here.
- The encoded `Interrupt` has no `interruptors` field (`E:rpc/RpcMessage.ts:136-139`).

### 1.3 Server to client messages

Source: `E:rpc/RpcMessage.ts:197-420`, `E:rpc/RpcServer.ts:565-720`.

```jsonc
// Chunk: one or more stream items for a stream request. values is never empty.
{"_tag":"Chunk","requestId":"7","values":[{...},{...}]}

// Exit: final message of every request (unary and stream)
{"_tag":"Exit","requestId":"7","exit":{"_tag":"Success","value":<encoded success>}}
{"_tag":"Exit","requestId":"7","exit":{"_tag":"Failure","cause":[<CauseReason>, ...]}}

// Defect: connection-level failure not tied to a request id (see 1.7)
{"_tag":"Defect","defect":<encoded defect>}

// Pong: reply to Ping
{"_tag":"Pong"}
```

`CauseReason` (`E:rpc/RpcMessage.ts:263-280`):

```jsonc
{"_tag":"Fail","error":<encoded typed error, e.g. {"_tag":"OrchestrationDispatchCommandError","message":"..."}>}
{"_tag":"Die","defect":<encoded defect>}
{"_tag":"Interrupt","fiberId":7}        // fiberId may be null
```

- Unary success: `value` is the encoded success. `Schema.Void` successes encode as `null`.
- Stream success (the stream ended normally): `{"_tag":"Success","value":null}`.
- `ClientProtocolError` appears in the TS types but is client-internal. It never crosses the wire.
- The server never sends `Request` frames to this client (that is only for bidirectional protocols).

### 1.4 Request ids

- Upstream (rc.115) ids are `string | number` and are echoed back unchanged, with the same JSON type (`E:rpc/RpcServer.ts:765-777`). The TS client uses a per-process counter from `0` as a JSON number (`E:rpc/RpcClient.ts:211,266`).
- The fork's effect (beta.78) parses ids with `BigInt(id)` and always replies with a string id (`F` effect `RpcServer.ts:573,586`, client `RpcClient.ts:710`).
- Rules for the Rust client:
  1. Send ids as decimal strings from a monotonic `u64` counter: `"0"`, `"1"`, ... This works against both effect versions.
  2. When reading `requestId`, accept number or string and normalize to the string form before lookup.
  3. Send `Ack` and `Interrupt` with the exact same JSON value you used in the `Request`. rc.115 keys its ack latches by the raw value, so `"11"` and `11` are different keys. A mismatched Ack is silently ignored and the stream stalls (verified, 1.9).
  4. Never reuse an id while that request is in flight. A duplicate id is dropped and it wedges the connection's read loop (verified, 1.9). Never reuse ids at all; a counter is enough.

### 1.5 Streams, Ack, and backpressure (read this twice)

The server runs with client acks enabled (`supportsAck: true` for the socket protocol, `E:rpc/RpcServer.ts:1596`).

Per stream request, the server (`E:rpc/RpcServer.ts:396-448`):

1. pulls the next batch of items from the handler stream,
2. sends one `Chunk` frame with all of them in `values`,
3. closes a per-request latch and waits for an `Ack` with that `requestId`,
4. only then pulls the next batch.

So the server has at most one un-acked Chunk outstanding per stream. If the client never acks, the stream delivers exactly one Chunk and then stops (verified, 1.9). There is no timeout on the server side; it just waits.

What the TS client does (`E:rpc/RpcClient.ts:560-585`): on each Chunk it offers all values into a bounded per-request queue (capacity 16 by default, `RpcClient.ts:341`) and sends the Ack after the offer succeeds. A slow consumer therefore delays the Ack, which is the backpressure.

Rules for the Rust client:

- Send exactly one `Ack` per `Chunk` frame, not per value.
- Send it after the values are handed to the subscription's consumer (e.g. pushed onto a bounded `mpsc`). Do not wait for UI processing. The simplest correct choice is to ack as soon as the frame is decoded and enqueued.
- Do not ack after the stream's `Exit` arrives, and do not ack unknown ids. Both are ignored by the server but are noise.
- Holding acks too long has a cost. The orchestration subscriptions buffer live events server-side while waiting. If more than 1,000 items or 8 MiB pile up un-delivered, the server fails the stream with `OrchestrationGetSnapshotError` `"The live event buffer is full. Resume from the last received sequence."` (`apps/server/src/orchestration/LiveStreamBudget.ts:9-10,64-67`). Recover by resubscribing with `afterSequence` (section 5.3).
- Terminal output streams (`terminal.attach`, `subscribeTerminalEvents`) are wrapped by `withTerminalOutputWindow` (`apps/server/src/terminal/OutputProtocol.ts:5-61`). The server self-acks up to 8 Chunks or 64 KiB in flight, and your Acks drain that window. The rule does not change: ack every Chunk.

Cancelling a stream: send `Interrupt`. The server interrupts the handler and replies `Exit` with `Failure` / `[{"_tag":"Interrupt","fiberId":N}]` (verified). The TS client sends Interrupt from the stream scope finalizer (`E:rpc/RpcClient.ts:452-462`) and gives the write 1 s (`RpcClient.ts:545-557`).

Interrupt for an id the server does not know (finished, never sent) produces no reply.

### 1.6 Keepalive

- Only the client pings. The server answers `Ping` with `Pong` immediately (`E:rpc/RpcServer.ts:799-801`) and never pings on its own. There is no server-side idle timeout in the RPC layer.
- Upstream client timing (rc.115 plus `U/patches/effect@4.0.0-rc.115.patch`, the `makePinger` hunk):
  - every 5 s: if the previous Ping got a Pong, send a new Ping;
  - otherwise count a miss and ping again; on the 3rd consecutive miss, fail the socket with a "ping timeout" `SocketOpenError`.
  - Net effect: a dead connection is detected after roughly 15 to 20 s.
- The fork's client (beta.78 patch) times out after a single missed Pong (about 5 to 10 s).
- Pongs are not correlated with Pings. Any Pong clears the "waiting" flag.
- Socket open timeout in the upstream client is 15 s (`packages/client-runtime/src/rpc/session.ts:45`).
- Reconnect is not done by the RPC layer (`retryPolicy: Schedule.recurs(0)`, `session.ts:197-201`). The connection supervisor reconnects with delays of 3 s, 4 s, 8 s, then 16 s repeating (`packages/client-runtime/src/connection/supervisor.ts:32,104-106`). The first attempt after a "retry now" skips the ladder (`supervisor.ts:715-741`).

### 1.7 Errors and failures

| Situation | What the server sends | Verified |
|---|---|---|
| Handler fails with a typed error | `Exit` / `Failure` / `[{"_tag":"Fail","error":{"_tag":"...", ...}}]` | yes |
| Caller lacks the scope | `Fail` with `{"_tag":"EnvironmentAuthorizationError","message":"...","requiredScope":"orchestration:operate"}`. Every method has this in its error union. Scope per method: section 4. | source |
| Payload fails schema decoding | `Exit` / `Failure` / `[{"_tag":"Die","defect":"<formatted schema issue string>"}]` | yes |
| Unknown method tag | `Exit` / `Failure` / `[{"_tag":"Die","defect":"Unknown request tag: demo.nope"}]` | yes |
| Handler dies (defect, bug) | A top-level `{"_tag":"Defect","defect":...}` frame and NO `Exit` for that request. The t3 server does not set `disableFatalDefects` (`apps/server/src/ws.ts:4137`, `E:rpc/RpcServer.ts:300-306`). | yes |
| Unknown message `_tag`, or a frame that is not JSON | `Defect` frame (`"Unknown request tag: Bogus"`, or `{"name":"SyntaxError",...}`). Connection keeps working. | yes |
| Request interrupted | `Exit` / `Failure` / `[{"_tag":"Interrupt","fiberId":N}]` | yes |

What the TS client does on `Defect`: it fails every in-flight request and stream on that connection with that defect (`E:rpc/RpcClient.ts:592-593`). Higher layers then retry or resubscribe. The Rust client should do the same. Since the request that died never gets an `Exit`, a narrower policy would leave it hanging forever.

Defect encoding (`Schema.Defect()`, verified):

| Value | Wire |
|---|---|
| `Error` | `{"name":"Error","message":"boom"}` plus `"cause":{...}` if the error has a cause. No stack. |
| A `TaggedError` used as a defect | `{"name":"E1","message":"m"}`. The `_tag` is lost. |
| string, number, plain JSON | passed through as is |
| `undefined` | `null` |

`cause` fields on typed errors (`cause: Schema.optional(Schema.Defect())`) use the same encoding. Treat them as opaque `serde_json::Value` and show `message` if present.

### 1.8 Things that wedge a connection

Verified against rc.115 in-process (section 1.9). Avoid all of them:

- Sending `Eof`. The server marks the client ended; later requests are interrupted and never answered. The TS socket client never sends Eof (`E:rpc/RpcClient.ts:714-716`).
- Reusing an in-flight id.
- Acking with the wrong JSON type for the id (stalls only that stream).
- Never acking (stalls only that stream, then eventually trips the 1,000 item / 8 MiB live budget on orchestration streams).

### 1.9 Verified frame traces

Produced by running the real rc.115 `RpcServer` (socket protocol, `RpcSerialization.json`) in-process against a raw JSON client. Harness: Appendix B.5. The garbage-frame, duplicate-id, and Eof cases ran as separate variants of the same harness so one wedge could not hide the next. Timestamps in ms.

```text
--- unary success / typed failure / defect / void
    2 C->S {"_tag":"Request","id":"1","tag":"demo.unary","payload":{"n":2},"headers":[]}
    2 C->S {"_tag":"Request","id":"2","tag":"demo.unary","payload":{"n":-1},"headers":[]}
    2 C->S {"_tag":"Request","id":"3","tag":"demo.unary","payload":{"n":13},"headers":[]}
    2 C->S {"_tag":"Request","id":4,"tag":"demo.void","payload":{},"headers":[]}
   11 S->C {"_tag":"Exit","requestId":"1","exit":{"_tag":"Success","value":"ok:2"}}
   11 S->C {"_tag":"Exit","requestId":"2","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":{"_tag":"DemoError","message":"negative"}}]}}
   11 S->C {"_tag":"Defect","defect":{"name":"Error","message":"unlucky"}}      <- request "3" never gets an Exit
   11 S->C {"_tag":"Exit","requestId":4,"exit":{"_tag":"Success","value":null}}  <- numeric id echoed as number
--- batch array in one frame + ping
  207 C->S [{"_tag":"Request","id":"5","tag":"demo.unary","payload":{"n":5},"headers":[]},{"_tag":"Ping"}]
  212 S->C {"_tag":"Exit","requestId":"5","exit":{"_tag":"Success","value":"ok:5"}}
  212 S->C {"_tag":"Pong"}
--- bad payload / unknown tag / missing headers
  407 C->S {"_tag":"Request","id":"6","tag":"demo.unary","payload":{"n":"x"},"headers":[]}
  407 C->S {"_tag":"Request","id":"7","tag":"demo.nope","payload":{},"headers":[]}
  407 C->S {"_tag":"Request","id":"8","tag":"demo.unary","payload":{"n":8}}
  412 S->C {"_tag":"Exit","requestId":"6","exit":{"_tag":"Failure","cause":[{"_tag":"Die","defect":"Expected \"Infinity\" | \"-Infinity\" | \"NaN\"\n  at [\"n\"]"}]}}
  412 S->C {"_tag":"Exit","requestId":"7","exit":{"_tag":"Failure","cause":[{"_tag":"Die","defect":"Unknown request tag: demo.nope"}]}}
  412 S->C {"_tag":"Exit","requestId":"8","exit":{"_tag":"Success","value":"ok:8"}}
--- stream WITHOUT acks: one chunk, then nothing
  611 C->S {"_tag":"Request","id":"10","tag":"demo.stream","payload":{"count":4},"headers":[]}
  632 S->C {"_tag":"Chunk","requestId":"10","values":[1]}
--- each Ack releases exactly one more Chunk
 1011 C->S {"_tag":"Ack","requestId":"10"}
 1037 S->C {"_tag":"Chunk","requestId":"10","values":[2]}
 1135 C->S {"_tag":"Ack","requestId":"10"}
 1156 S->C {"_tag":"Chunk","requestId":"10","values":[3]}
 1258 C->S {"_tag":"Ack","requestId":"10"}
 1279 S->C {"_tag":"Chunk","requestId":"10","values":[4]}
 1382 C->S {"_tag":"Ack","requestId":"10"}
 1388 S->C {"_tag":"Exit","requestId":"10","exit":{"_tag":"Success","value":null}}
--- Ack with wrong JSON type (string id, numeric ack): stalls; Interrupt still works
 1506 C->S {"_tag":"Request","id":"11","tag":"demo.stream","payload":{"count":3},"headers":[]}
 1532 S->C {"_tag":"Chunk","requestId":"11","values":[1]}
 1660 C->S {"_tag":"Ack","requestId":11}
 1860 C->S {"_tag":"Interrupt","requestId":"11"}
 1865 S->C {"_tag":"Exit","requestId":"11","exit":{"_tag":"Failure","cause":[{"_tag":"Interrupt","fiberId":5}]}}
--- garbage frame and unknown _tag: Defect, connection survives
    1 C->S not json
    6 S->C {"_tag":"Defect","defect":{"name":"SyntaxError","message":"Unexpected token 'o', \"not json\" is not valid JSON"}}
  152 C->S {"_tag":"Request","id":"13","tag":"demo.unary","payload":{"n":1},"headers":[]}
  157 S->C {"_tag":"Exit","requestId":"13","exit":{"_tag":"Success","value":"ok:1"}}
  302 C->S {"_tag":"Bogus"}
  307 S->C {"_tag":"Defect","defect":"Unknown request tag: Bogus"}
--- duplicate in-flight id: connection stops answering
    1 C->S {"_tag":"Request","id":"12","tag":"demo.stream","payload":{"count":2},"headers":[]}
   28 S->C {"_tag":"Chunk","requestId":"12","values":[1]}
  105 C->S {"_tag":"Request","id":"12","tag":"demo.unary","payload":{"n":1},"headers":[]}
  210 C->S {"_tag":"Ack","requestId":"12"}
  463 C->S {"_tag":"Request","id":"13","tag":"demo.unary","payload":{"n":1},"headers":[]}     <- no reply
--- Eof: same, nothing is answered afterwards
    2 C->S {"_tag":"Request","id":"30","tag":"demo.stream","payload":{"count":2},"headers":[]}
   32 S->C {"_tag":"Chunk","requestId":"30","values":[1]}
  105 C->S {"_tag":"Eof"}
  208 C->S {"_tag":"Ack","requestId":"30"}
  362 C->S {"_tag":"Request","id":"31","tag":"demo.unary","payload":{"n":2},"headers":[]}     <- no reply
```

## 2. Effect Schema JSON encoding

The wire form of every payload, success, stream item, and error is `Schema.toCodecJson(schema)` encoded (`E:rpc/RpcSerialization.ts:39`). Verified by encoding sample values with rc.115:

| Schema construct | Wire JSON | Rust/serde |
|---|---|---|
| `Schema.String`, branded ids (`ThreadId`, `CommandId`, ...) | `"..."` | `String` newtypes. Ids are trimmed, non-empty strings (`baseSchemas.ts:129-135`). Client-generated ids are UUID v4 (`apps/web/src/lib/utils.ts:41-55`, `packages/client-runtime/src/operations/commands.ts:68-77`). |
| `IsoDateTime` (`= Schema.String`) | `"2026-01-02T03:04:05.678Z"` | Keep as `String` or parse leniently. It is not validated as a date. |
| `Schema.DateTimeUtc`, `Schema.Date` | `"2026-01-02T03:04:05.678Z"` | Same. Used in a few inputs (`ClientActivityReportInput.observedAt`). |
| `Schema.Number`, `Int`, `NonNegativeInt` | `1.5`. Non-finite values encode as the strings `"NaN"`, `"Infinity"`, `"-Infinity"`. | `f64`/`i64`/`u64`. A custom deserializer that accepts those three strings is cheap insurance. |
| `Schema.Finite` | number only | |
| `Schema.BigInt` | `"123"` (string) | Not used in the core contracts. |
| `Schema.Uint8Array` | base64 string `"AQID"` | Not used in the core contracts. |
| `Schema.Boolean` | `true` / `false` | |
| `Schema.NullOr(X)` | `X` or `null`, key always present | `Option<X>` without `skip_serializing_if`. |
| `Schema.optionalKey(X)` | key absent, or `X`. `null` is rejected on decode. | `#[serde(default, skip_serializing_if = "Option::is_none")] Option<X>`. Never send `null`. |
| `Schema.optional(X)` (= optionalKey of `X \| undefined`) | key absent or `X`. Decode accepts `null` as absent. Encoding an explicit `undefined` writes `null`. | Same as optionalKey. Accept `null` on read. |
| `Schema.UndefinedOr(X)` as a required key | `null` when undefined (decode requires the key) | Rare. |
| `Schema.Option(X)` | `{"_tag":"Some","value":X}` / `{"_tag":"None"}` | Not used in the core contracts. `OptionFromNullOr` is `X \| null`. |
| `Schema.Literal("a")`, `Schema.Literals([...])` | `"a"` | String enums. Add an `Unknown(String)` catch-all where the set grows (statuses, kinds). |
| `Schema.Struct({...})` | object. Unknown keys are ignored on decode. | Do not use `deny_unknown_fields`. |
| `Schema.Class` | plain object, no tag | |
| `Schema.TaggedStruct("X", ...)`, `Schema.TaggedError` | object with `"_tag":"X"` | `#[serde(tag = "_tag")]` |
| Discriminated unions in t3 contracts | discriminated by a literal field: `_tag` (errors, `VcsStatusStreamEvent`), `type` (commands, events, config/lifecycle/auth stream events, terminal events), `kind` (orchestration stream items, shell events, project icons, context records) | `#[serde(tag = "...")]` per union, with an `#[serde(other)]` / untagged fallback variant. |
| `Schema.Record(K, V)` | JSON object | `HashMap<String, V>` / `BTreeMap` |
| `Schema.Array(X)` | JSON array | `Vec<X>` |
| `Schema.Defect()` | see 1.7 | `serde_json::Value` |
| `Schema.Unknown`, `Schema.Any` | any JSON | `serde_json::Value` |
| `Schema.Duration` | `{"_tag":"Millis","value":n}`, `{"_tag":"Nanos","value":"<bigint>"}`, `{"_tag":"Infinity"}` | Not used in the core contracts. |
| `withDecodingDefault(...)` | the encoder always writes the key; the decoder fills a default when it is missing | Read as `#[serde(default = ...)]`. When sending, you may omit it. |
| `TrimmedString` / `TrimmedNonEmptyString` | string, trimmed on decode and encode (`baseSchemas.ts:6-15`) | Trim before sending; empty strings fail where non-empty is required. |

Wire-shape-changing transformations in the t3 contracts:

- `ForwardCompatibleArray(X)` (`baseSchemas.ts:110-125`): encoded as a plain array of `X`, but the decoder drops elements it cannot decode instead of failing. Used for `ServerConfig.providers`, `ServerConfig.keybindings`, `UsageLimitSourceSnapshots`, and more (`grep ForwardCompatible packages/contracts/src`). The Rust decoder should do the same: decode each element independently and skip failures.
- `ForwardCompatibleOptional(X)` / `ForwardCompatibleNullable(X)` (`baseSchemas.ts:51-89`): an unknown value decodes as absent / `null`. `OmittedWhenNull` (`:91-108`) never writes `null`.
- `ModelSelection` (`orchestration.ts:72-125`): wire is `{"instanceId":"codex","model":"gpt-5.5","options":[{"id":"effort","value":"high"}]}`. The decoder also accepts the legacy `{"provider":"codex","model":...}` (promotes `provider` to `instanceId`). `options` is always encoded as the array form; the decoder also accepts the legacy object form `{"effort":"high","fastMode":true}` (`model.ts:55-115`).
- `ChatUnknownAttachment` (`orchestration.ts:332-352`): attachments of a type other than `image` / `file` decode into a catch-all with the base fields, so new attachment types do not break old readers.
- `OrchestrationThreadActivity.kind` is an open string and `payload` is `unknown` (`orchestration.ts:661-679`).

What is strict (and will break a TS client if the server adds members, so expect the server to gate additions behind input flags or capabilities):

- `OrchestrationEvent` union (`orchestration.ts:2044`). A new event `type` fails decoding of that chunk in the TS client and kills the subscription.
- `ServerConfigStreamEvent` (`server.ts:770`). The server only emits `environmentThemesUpdated` / `usageLimitSourcesUpdated` when the subscriber asked for them in the payload, because "an already-shipped client ... dies on an unknown member" (`apps/server/src/ws.ts:3948-3972`).
- Command union `ClientOrchestrationCommand` (server side): an unknown command `type` is a payload decode failure, i.e. `Exit` with `Die` (1.7).

## 3. Connection, auth, and HTTP endpoints

### 3.1 Getting a credential (local machine, `npx t3`)

- `t3 start` / `npx t3` (web mode, loopback, port 3773 or the next free one; `apps/server/src/config.ts:23`, `cli/config.ts:294-309`) mints a startup pairing credential: 12 chars from `23456789ABCDEFGHJKLMNPQRSTUVWXYZ`, single use, 5-minute TTL, admin scopes (`apps/server/src/auth/EnvironmentAuth.ts:989-1016`, `PairingGrantStore.ts:239-261,383-389`). It logs `http://localhost:<port>/pair#token=<cred>` and auto-opens the browser, which usually consumes the token first (`serverRuntimeStartup.ts:306-318,1027-1034`).
- `t3 serve` (headless) prints `Token: <cred>` and `Pairing URL: ...` to stdout (`apps/server/src/startupAccess.ts:122-148`).
- Recommended for the native client: `t3 auth pairing create --json [--base-dir ~/.t3] [--label "..."]` (one-time token, standard scopes, 5 min) then exchange it at `POST /oauth/token` for a 30-day bearer token (`apps/server/src/cli/auth.ts:84-114`). `t3 auth session issue --token-only` prints a bearer directly (admin scopes, `deviceType:"bot"`, 30 days; `cli/auth.ts:162-194`). The CLI writes to the same `~/.t3/userdata/state.sqlite` the server reads, so tokens work against a running server (`config.ts:136-163`).
- An already-authenticated admin client can mint pairing tokens with `POST /api/auth/pairing-token` (3.4).
- Desktop (Electron) passes a reusable bootstrap token to its own server over an fd. Other processes cannot use it (`cli/config.ts:55,278-281,348`, `apps/desktop/src/backend/DesktopLocalEnvironmentAuth.ts:52-91`).

Token exchange, `POST /oauth/token` with a form-urlencoded body (`packages/contracts/src/auth.ts:186-196`, server `apps/server/src/auth/http.ts:312-385`):

```text
grant_type=urn:ietf:params:oauth:grant-type:token-exchange
&subject_token=<pairing credential>
&subject_token_type=urn:t3:params:oauth:token-type:environment-bootstrap
&requested_token_type=urn:ietf:params:oauth:token-type:access_token
&scope=<space separated, optional; omitted = all granted scopes>
&client_label=<optional>&client_device_type=desktop&client_os=<optional>
```

```json
{"access_token":"<b64url-claims>.<sig>","issued_token_type":"urn:ietf:params:oauth:token-type:access_token",
 "token_type":"Bearer","expires_in":2592000,
 "scope":"orchestration:read orchestration:operate terminal:operate review:write relay:read"}
```

- An optional `dpop` header makes the token DPoP-bound (1 h TTL). Skip DPoP.
- Errors: 400 `invalid_scope` / `scope_not_granted`, 401 `invalid_credential` (unknown, consumed, expired, revoked), 500 `access_token_issuance_failed`.
- There is no refresh endpoint. Bearer sessions last 30 days (`apps/server/src/auth/SessionStore.ts:423`).
- Scopes (`packages/contracts/src/auth.ts:81-115`): standard is `orchestration:read orchestration:operate terminal:operate review:write relay:read`; admin adds `access:read access:write relay:write`.

### 3.2 Connect sequence (bearer)

What upstream's client does for a bearer connection (`packages/client-runtime/src/authorization/remote.ts`, `connection/resolver.ts:250-290`, `connection/driver.ts:54-58`):

1. `GET /.well-known/t3/environment` (no auth) for the descriptor.
2. `POST /api/auth/websocket-ticket` with `Authorization: Bearer <token>`, empty body. Reply `{"ticket":"...","expiresAt":"<ISO>"}`.
3. `GET /.well-known/t3/environment` again. Check `environmentId` is the one you expect, and that `(orchestrationProtocolVersion ?? 1) == 1`; otherwise refuse to connect (`connection/compatibility.ts:9-24`).
4. Open `ws(s)://<host>/ws?wsTicket=<ticket>&clientSurface=desktop&clientAppVersion=<v>&clientDeviceType=desktop&clientOs=Linux&connectionMethod=direct&orchestrationProtocol=1`.

Details:

- Scheme swap `http:` to `ws:`, `https:` to `wss:`, path `/ws` (`packages/shared/src/remote.ts:106-130`). HTTP API calls ignore any path prefix on the base URL (`rpc/http.ts:89-95`).
- Tickets: `base64url(JSON{v:1,kind:"websocket",sid,iat,exp}).base64url(HMAC)`, 5-minute TTL, reusable until expiry (`SessionStore.ts:424,438-445,847-945`). Fetch a new one per connect attempt.
- If `wsTicket` is present it is the only credential checked; an invalid ticket is a 401 with no fallback (`EnvironmentAuth.ts:517,1075-1095`).
- Without a ticket the upgrade request is authenticated like any HTTP request (`EnvironmentAuth.ts:572-689`). Precedence: session cookie, `Authorization: Bearer <t>` (case-sensitive prefix), `Authorization: DPoP <t>` + `dpop` proof, legacy cookie, dev cookie. So sending `Authorization: Bearer <token>` on the upgrade and skipping the ticket also works and is simpler. Upstream's clients use the ticket because browsers cannot set upgrade headers.
- Client metadata query params are optional; the server reads only these, silently dropping bad values (`apps/server/src/ws.ts:439-500`): `clientSurface` (`web|desktop|mobile|cli`), `clientAppVersion` (max 64 chars), `clientDeviceType` (`desktop|phone|tablet|unknown`), `clientOs` (`macOS|Windows|Linux|iOS|Android|ChromeOS|other|unknown`), `clientWebDeployment`, `clientBrowser` (web only), `clientOsMajorVersion`, `clientDeviceModel` (mobile only), `connectionMethod` (`direct|ssh|relay|unknown`). `orchestrationProtocol=1` is appended last and is not read by the server.
- Upgrade failure bodies (`apps/server/src/ws.ts:4120-4130,4189-4193`, `packages/contracts/src/environmentHttp.ts:120-138`):
  - 401 `{"_tag":"EnvironmentAuthInvalidError","code":"auth_invalid","reason":"missing_credential"|"invalid_credential","dpopFailureReason"?:...,"traceId":"..."}`
  - 500 `{"_tag":"EnvironmentInternalError","code":"internal_error","reason":"internal_error","traceId":"..."}`
- No Origin check on `/ws`.
- All HTTP (including `/ws`) waits until the server is command-ready (`apps/server/src/server.ts:587-594`). Responses may be compressed (`http.ts:230-232`); enable gzip in reqwest.

After the socket opens, upstream's RPC session immediately subscribes to `subscribeServerConfig` and treats the first `snapshot` item as the connection's initial config. It checks `config.environment.environmentId` again, then calls `server.probe` if `capabilities.connectionProbe` (else `server.getConfig`) as a liveness probe (`packages/client-runtime/src/rpc/session.ts:223-390`).

### 3.3 Session check

`GET /api/auth/session` (no middleware, always 200; `apps/server/src/auth/EnvironmentAuth.ts:691-710`, schema `packages/contracts/src/auth.ts:348-354`):

```json
{"authenticated":true,
 "auth":{"policy":"loopback-browser","bootstrapMethods":["one-time-token"],
         "sessionMethods":["browser-session-cookie","bearer-access-token","dpop-access-token"],
         "sessionCookieName":"t3_session_3773_ab12cd34ef56"},
 "scopes":["orchestration:read","..."],"sessionMethod":"bearer-access-token","expiresAt":"2026-10-31T00:00:00.000Z"}
```

`scopes`, `sessionMethod`, `expiresAt` are optional keys. `authenticated:false` means the credential is bad.

### 3.4 HTTP endpoints used alongside the socket

Contract: `packages/contracts/src/environmentHttp.ts:411-623`; routes wired at `apps/server/src/server.ts:597-612`. Auth is `Authorization: Bearer <token>` (or the session cookie). Error bodies are tagged errors: 400 `invalid_request`, 401 `auth_invalid`, 403 `insufficient_scope` (with `requiredScope`) / `operation_forbidden`, 404 `not_found`, 500 `internal_error`, all with `traceId` (`environmentHttp.ts:102-213`).

| Method and path | Auth | Request | Response | Used for |
|---|---|---|---|---|
| `GET /.well-known/t3/environment` | none | | `ExecutionEnvironmentDescriptor` (Appendix A) | descriptor, id and protocol check |
| `GET /api/auth/session` | optional | | see 3.3 | is my token valid |
| `POST /oauth/token` | none | form body, 3.1 | 3.1 | pairing credential to bearer |
| `POST /api/auth/websocket-ticket` | bearer | none | `{"ticket","expiresAt"}` | `/ws?wsTicket=` |
| `POST /api/auth/browser-session` | none | `{"credential":"..."}` | `{"authenticated":true,"scopes",...}` + `Set-Cookie` | web app only |
| `POST /api/auth/pairing-token` | `access:write` | `{"label"?,"scopes"?}` | `{"id","credential","label"?,"expiresAt"}` | mint pairing links |
| `GET /api/auth/pairing-links`, `POST .../revoke {"id"}` | `access:read`/`write` | | `[AuthPairingLink]`, `{"revoked":bool}` | settings > access |
| `GET /api/auth/clients`, `POST /api/auth/clients/revoke {"sessionId"}`, `POST /api/auth/clients/revoke-others` | `access:read`/`write` | | `[AuthClientSession]`, `{"revoked"}`, `{"revokedCount"}` | settings > access |
| `GET /api/orchestration/shell` | `orchestration:read` | | `OrchestrationShellSnapshot` | fast initial shell load (5.2) |
| `GET /api/orchestration/threads/:threadId?reasoningMessages=true&turnLimit=N&beforeCursor=X` | `orchestration:read` | all query params optional strings (`environmentHttp.ts:500-506`) | `OrchestrationThreadDetailSnapshot` | fast thread open and older pages (5.2) |
| `POST /api/attachments/upload/<token>` | the token in the path | raw bytes, `Content-Type: <mime>` | 204 | attachment upload (3.5) |
| `GET /api/assets/<token>/<file>` (and `HEAD`) | the token in the path | | file bytes | favicons, images, files (3.5) |
| `POST /api/pull-requests/diff` | `orchestration:read` | `{"projectId","repository","number","host"?,...}` | `{"patch","truncated","nextCursor","omittedFileStats"?}` | PR diff view |

Not used by the UI but available: `GET /api/orchestration/snapshot`, `POST /api/orchestration/dispatch` (body `ClientOrchestrationCommand`, reply `{"sequence"}`).

Thread snapshot 404 is `{"_tag":"EnvironmentResourceNotFoundError","code":"not_found","reason":"thread_not_found","traceId":"..."}`; the upstream client treats it as "defer to the socket". It uses 20 s timeouts for both snapshot endpoints and falls back to the socket on any error (`packages/client-runtime/src/state/shellSnapshotHttp.ts:21-86`, `threadSnapshotHttp.ts:22-133`). Only send `turnLimit` / `reasoningMessages` when `ServerConfig.threadSnapshotPagination` / `reasoningMessages` are `true`.

### 3.5 Assets and attachment uploads

Assets (capability URLs, no auth header):

1. RPC `assets.createUrl` with `{"resource":<AssetResource>}`. Resource variants (`packages/contracts/src/assets.ts:14-61`): `{"_tag":"project-favicon","cwd":"/abs","path"?}`, `{"_tag":"workspace-file","threadId","path"}`, `{"_tag":"media-file","threadId","path"}`, `{"_tag":"draft-workspace-file","cwd","path"}`, `{"_tag":"attachment","attachmentId","fileName"?,"mimeType"?,"disposition"?:"inline"|"attachment"}`, `{"_tag":"native-app-icon","app"}`, `{"_tag":"github-media","cwd","url"}`.
2. Result `{"relativeUrl":"/api/assets/<token>/<name>","expiresAt":<epoch ms>,"sourcePath"?,"imageDimensions"?:{"width","height"}}`. Resolve against the HTTP base URL.
3. `GET` it with no credentials. Tokens live 1 h; favicon tokens are bucketed to 30 min. The upstream client refreshes every 30 min (`state/assets.ts:28,58-64`). Expired or bad token gives 404 `text/plain`. Audio/video support `Range`. A project with no favicon gets a `project-favicon-missing` URL that 404s.

Attachment upload (needs `ExecutionEnvironmentCapabilities.attachmentUploads`; size cap in `fileAttachments.maxUploadBytes`):

1. RPC `attachments.createUploadUrl` with `{"type"?:"image","name","mimeType":"image/png"|"image/jpeg"|"image/gif"|"image/webp","sizeBytes"}` or `{"type":"file","name","mimeType","sizeBytes"}`.
2. Reply `{"attachmentId","relativeUrl":"/api/attachments/upload/<token>","expiresAt":<ms>}` (10-minute URL).
3. `POST <base><relativeUrl>`, body = raw bytes, `Content-Type: <mimeType>`, exact `Content-Length`. 204 on success; 404 bad token; 400 length mismatch; 500 persist failure (`apps/server/src/http.ts:436-473`, `AttachmentUpload.ts:151-222`).
4. Reference it in `thread.turn.start` as `{"type":"image"|"file","id":"<attachmentId>","name","mimeType","sizeBytes"}` (section 6). The server claims it and checks size and type (`apps/server/src/orchestration/Normalizer.ts:162-225`).
5. `attachments.delete {"attachmentId"}` releases an upload you decided not to send.

The alternative for images is inline: `{"type":"image","name","mimeType","sizeBytes","dataUrl":"data:image/png;base64,..."}` directly in `thread.turn.start` (max 10 MiB per image, 14,000,000 chars per data URL, 80 MiB of images per message; `orchestration.ts:165-184`). The server decodes and persists it and rewrites it to a persisted attachment with a server id (`Normalizer.ts:226-296`).

### 3.6 Fork differences (connection layer)

The fork uses the same ticket/bearer scheme with these differences: bare `/ws?wsTicket=...` with no metadata params; no `orchestrationProtocolVersion` in the descriptor; cookie `t3_session` / `t3_session_<port>`; no `/api/pull-requests/diff`; no snapshot query params; no `attachments.createUploadUrl` or upload route; a smaller `AssetResource` union; 6 s snapshot timeouts. None of this changes what the Rust client must do against upstream.

## 4. Method catalog

Generated from upstream `WsRpcGroup` (`packages/contracts/src/rpc.ts:1452`) with Appendix B, scopes from `apps/server/src/auth/RpcAuthorization.ts:24-179`, and "fork UI calls" from `grep -rhoE "(WS_METHODS|ORCHESTRATION_WS_METHODS)\.[a-zA-Z]+" F/apps/web/src F/packages/client-runtime/src`.

- Every method's error union also contains `EnvironmentAuthorizationError` (`{"_tag","message","requiredScope"}`, `packages/contracts/src/auth.ts:297`). The error column lists the rest.
- `stream` methods: the item type is the `Chunk` value type; the final `Exit` carries `null` on success or a `Failure` with the error column's types. Ack every Chunk (1.5).
- Type names in cells are defined in Appendix A. When two exports share one schema, the generator prints the first name. For example, `server.upsertKeybinding` returns a value typed `ServerRemoveKeybindingResult` (same shape as `ServerUpsertKeybindingResult`), and `orchestration.getTurnDiff` returns `OrchestrationGetFullThreadDiffResult` (both are `ThreadTurnDiff`).
- Scope shorthand: `orch:read` = `orchestration:read`, `orch:operate` = `orchestration:operate`.

### 4.1 Core methods (shapes in Appendix A)

These are the methods the fork's UI calls, plus the upstream methods a client needs for the same features (`server.probe`, `server.getSettings`, `attachments.*`, `projects.searchContents`, `orchestration.searchThreads`).

| tag | kind | payload | success / stream item | error (besides EnvironmentAuthorizationError) | scope | fork UI calls |
|---|---|---|---|---|---|---|
| `server.probe` | unary | {} | {} | - | orch:read |  |
| `server.getConfig` | unary | {} | ServerConfig | KeybindingsConfigError \| ServerSettingsError | orch:read | yes |
| `server.refreshProviders` | unary | { instanceId?: string \| null; cwd?: string \| null; fresh?: boolean \| null; refreshModels?: boolean \| null } | ServerProviderUpdatedPayload | ProviderSetupError | orch:operate | yes |
| `server.updateProvider` | unary | ServerProviderUpdateInput | ServerProviderUpdatedPayload | ServerProviderUpdateError | orch:operate | yes |
| `server.upsertKeybinding` | unary | ServerUpsertKeybindingInput | ServerRemoveKeybindingResult | KeybindingsConfigError | orch:operate | yes |
| `server.removeKeybinding` | unary | ServerRemoveKeybindingInput | ServerRemoveKeybindingResult | KeybindingsConfigError | orch:operate | yes |
| `server.getSettings` | unary | {} | ServerSettings | ServerSettingsError | orch:read |  |
| `server.updateSettings` | unary | { patch: ServerSettingsPatch } | ServerSettings | ServerSettingsError | orch:operate | yes |
| `projects.listEntries` | unary | ProjectListEntriesInput | ProjectListEntriesResult | ProjectListEntriesError | orch:read | yes |
| `projects.readFile` | unary | ProjectReadFileInput | ProjectReadFileResult | ProjectReadFileError | orch:read | yes |
| `projects.searchContents` | unary | ProjectSearchContentsInput | ProjectSearchContentsResult | ProjectSearchContentsError | orch:read |  |
| `projects.searchEntries` | unary | ProjectSearchEntriesInput | ProjectSearchEntriesResult | ProjectSearchEntriesError | orch:read | yes |
| `projects.writeFile` | unary | ProjectWriteFileInput | ProjectWriteFileResult | ProjectWriteFileError | orch:operate | yes |
| `shell.openInEditor` | unary | LaunchEditorInput | null | ExternalLauncherError | orch:operate | yes |
| `filesystem.browse` | unary | FilesystemBrowseInput | FilesystemBrowseResult | FilesystemBrowseError | orch:read | yes |
| `assets.createUrl` | unary | AssetCreateUrlInput | AssetCreateUrlResult | AssetAccessError | orch:read | yes |
| `attachments.createUploadUrl` | unary | AttachmentCreateUploadUrlInput | AttachmentCreateUploadUrlResult | AttachmentUploadSigningKeyError | orch:operate |  |
| `attachments.delete` | unary | AttachmentDeleteInput | null | - | orch:operate |  |
| `subscribeVcsStatus` | stream | VcsStatusInput | VcsStatusStreamEvent | GitManagerServiceError | orch:read | yes |
| `vcs.pull` | unary | VcsPullInput | VcsPullResult | GitCommandError | orch:operate | yes |
| `vcs.refreshStatus` | unary | VcsStatusInput | VcsStatusResult | GitManagerServiceError | orch:read | yes |
| `git.runStackedAction` | stream | GitRunStackedActionInput | GitActionProgressEvent | GitManagerServiceError | orch:operate | yes |
| `git.resolvePullRequest` | unary | GitPullRequestRefInput | GitResolvePullRequestResult | GitManagerServiceError | orch:operate | yes |
| `git.preparePullRequestThread` | unary | GitPreparePullRequestThreadInput | GitPreparePullRequestThreadResult | GitManagerServiceError | orch:operate | yes |
| `vcs.listRefs` | unary | VcsListRefsInput | VcsListRefsResult | GitCommandError | orch:read | yes |
| `vcs.createWorktree` | unary | VcsCreateWorktreeInput | VcsCreateWorktreeResult | GitCommandError | orch:operate | yes |
| `vcs.removeWorktree` | unary | VcsRemoveWorktreeInput | null | GitCommandError | orch:operate | yes |
| `vcs.createRef` | unary | VcsCreateRefInput | VcsCreateRefResult | GitCommandError | orch:operate | yes |
| `vcs.switchRef` | unary | VcsSwitchRefInput | VcsSwitchRefResult | GitCommandError | orch:operate | yes |
| `vcs.init` | unary | VcsInitInput | null | VcsError | orch:operate | yes |
| `review.getDiffPreview` | unary | ReviewDiffPreviewInput | ReviewDiffPreviewResult | ReviewDiffPreviewError | review:write | yes |
| `terminal.open` | unary | TerminalOpenInput | TerminalSessionSnapshot | TerminalError | terminal:operate | yes |
| `terminal.attach` | stream | TerminalAttachInput | TerminalAttachStreamEvent | TerminalError | terminal:operate | yes |
| `terminal.write` | unary | TerminalWriteInput | null | TerminalError | terminal:operate | yes |
| `terminal.resize` | unary | TerminalResizeInput | null | TerminalError | terminal:operate | yes |
| `terminal.clear` | unary | TerminalClearInput | null | TerminalError | terminal:operate | yes |
| `terminal.restart` | unary | TerminalRestartInput | TerminalSessionSnapshot | TerminalError | terminal:operate | yes |
| `terminal.close` | unary | TerminalCloseInput | null | TerminalError | terminal:operate | yes |
| `subscribeTerminalEvents` | stream | {} | TerminalEvent | - | terminal:operate | yes |
| `subscribeTerminalMetadata` | stream | {} | TerminalMetadataStreamEvent | - | terminal:operate | yes |
| `subscribeServerConfig` | stream | { environmentThemes?: boolean \| null; usageLimitSources?: boolean \| null; usageLimitsCommand?: boolean \| null } | ServerConfigStreamEvent | KeybindingsConfigError \| ServerSettingsError | orch:read | yes |
| `subscribeServerLifecycle` | stream | {} | ServerLifecycleStreamEvent | - | orch:read | yes |
| `subscribeAuthAccess` | stream | {} | AuthAccessStreamEvent | AuthAccessStreamError | access:read | yes |
| `orchestration.dispatchCommand` | unary | ClientOrchestrationCommand | DispatchResult | OrchestrationDispatchCommandError | orch:operate | yes |
| `orchestration.getTurnDiff` | unary | OrchestrationGetTurnDiffInput | OrchestrationGetFullThreadDiffResult | OrchestrationGetTurnDiffError | orch:read | yes |
| `orchestration.getFullThreadDiff` | unary | OrchestrationGetFullThreadDiffInput | OrchestrationGetFullThreadDiffResult | OrchestrationGetFullThreadDiffError | orch:read | yes |
| `orchestration.searchThreads` | unary | OrchestrationSearchThreadsInput | OrchestrationSearchThreadsResult | OrchestrationSearchThreadsError | orch:read |  |
| `orchestration.getArchivedShellSnapshot` | unary | {} | OrchestrationShellSnapshot | OrchestrationGetSnapshotError | orch:read | yes |
| `orchestration.subscribeShell` | stream | OrchestrationSubscribeShellInput | OrchestrationShellStreamItem | OrchestrationGetSnapshotError | orch:read | yes |
| `orchestration.subscribeThread` | stream | OrchestrationSubscribeThreadInput | OrchestrationThreadStreamItem | OrchestrationGetSnapshotError | orch:read | yes |

### 4.2 Secondary methods the fork UI calls

Run Appendix B with these tags to get their definitions. In-app browser preview, cloud relay, process diagnostics, and source control. Likely out of scope for a first native client.

| tag | kind | payload | success / stream item | error (besides EnvironmentAuthorizationError) | scope | fork UI calls |
|---|---|---|---|---|---|---|
| `server.discoverSourceControl` | unary | {} | SourceControlDiscoveryResult | - | orch:read | yes |
| `server.getTraceDiagnostics` | unary | {} | ServerTraceDiagnosticsResult | - | orch:read | yes |
| `server.getProcessDiagnostics` | unary | {} | ServerProcessDiagnosticsResult | - | orch:read | yes |
| `server.getProcessResourceHistory` | unary | ServerProcessResourceHistoryInput | ServerProcessResourceHistoryResult | - | orch:read | yes |
| `server.signalProcess` | unary | ServerSignalProcessInput | ServerSignalProcessResult | - | orch:operate | yes |
| `server.reportClientActivity` | unary | ClientActivityReportInput | null | - | orch:read |  |
| `cloud.getRelayClientStatus` | unary | {} | RelayClientStatusSchema | - | relay:read | yes |
| `cloud.installRelayClient` | stream | {} | RelayClientInstallProgressEventSchema | RelayClientInstallFailedError | relay:write | yes |
| `sourceControl.lookupRepository` | unary | SourceControlRepositoryLookupInput | SourceControlRepositoryInfo | SourceControlRepositoryError | orch:read | yes |
| `sourceControl.cloneRepository` | unary | SourceControlCloneRepositoryInput | SourceControlCloneRepositoryResult | SourceControlRepositoryError | orch:operate | yes |
| `sourceControl.publishRepository` | unary | SourceControlPublishRepositoryInput | SourceControlPublishRepositoryResult | SourceControlRepositoryError | orch:operate | yes |
| `preview.open` | unary | PreviewOpenInput | PreviewSessionSnapshot | PreviewError | orch:operate | yes |
| `preview.navigate` | unary | PreviewNavigateInput | PreviewSessionSnapshot | PreviewError | orch:operate | yes |
| `preview.resize` | unary | PreviewResizeInput | PreviewSessionSnapshot | PreviewError | orch:operate | yes |
| `preview.refresh` | unary | PreviewRefreshInput | null | PreviewError | orch:operate | yes |
| `preview.close` | unary | PreviewCloseInput | null | PreviewError | orch:operate | yes |
| `preview.list` | unary | PreviewListInput | PreviewListResult | - | orch:read | yes |
| `preview.reportStatus` | unary | PreviewReportStatusInput | null | PreviewError | orch:operate | yes |
| `previewAutomation.connect` | stream | PreviewAutomationHost | PreviewAutomationStreamEvent | PreviewAutomationError | orch:operate | yes |
| `previewAutomation.respond` | unary | PreviewAutomationResponse | null | PreviewAutomationError | orch:operate | yes |
| `previewAutomation.focusHost` | unary | PreviewAutomationHostFocus | null | - | orch:operate | yes |
| `subscribePreviewEvents` | stream | {} | PreviewEvent | - | orch:read | yes |
| `subscribeDiscoveredLocalServers` | stream | { configuredUrls?: Array<string> \| null } | DiscoveredLocalServerList | - | orch:read | yes |
| `orchestration.getWorkflowScript` | unary | OrchestrationGetWorkflowScriptInput | OrchestrationGetWorkflowScriptResult | OrchestrationGetWorkflowScriptError | orch:read |  |

### 4.3 Fork-only methods and how upstream covers them

| Fork tag | Fork shape | Called by the fork UI? | Upstream replacement |
|---|---|---|---|
| `server.ping` | `{}` to `{}` | No (contracts and server only) | `server.probe {}` to `{}` when `capabilities.connectionProbe`, else `server.getConfig` (`packages/client-runtime/src/rpc/session.ts:367-376`). Transport keepalive is the RPC `Ping`/`Pong` (1.6). |
| `orchestration.replayEvents` | `{fromSequenceExclusive}` to `OrchestrationEvent[]` | No (contracts, server, tests only) | Resubscribe with `afterSequence` (5.2, 5.3). |
| command `thread.completion.acknowledge` | `{threadId, completedAt, ...}` | Yes, `F:packages/client-runtime/src/operations/commands.ts:197` | No equivalent. Unread state is client-local; "done" is `thread.settle` (5.7, 5.8). |

Upstream methods not in the fork and relevant to the UI port: `server.probe`, `attachments.createUploadUrl`, `attachments.delete`, `projects.searchContents`, `projects.ensureScratch`, `projects.createNew`, `orchestration.searchThreads`, `orchestration.getWorkflowScript`, `review.getDiffFileContents`, `subscribeWorktreeSetup`, `worktreeSetup.cancel`, `server.reportClientActivity`, `subscribeBackgroundPolicy`, the `provider.auth.*` / `provider.install.*` setup flows, and the `pullRequests.*` family.

`server.reportClientActivity` (`ClientActivityReportInput`, `packages/contracts/src/background.ts:67-81`) is how upstream's web app tells the server it is visible and focused (`apps/web/src/lib/backgroundActivityReporter.ts:205`). The server's background policy (git fetch, provider health refresh) depends on these leases. A native client should send it on focus changes, or accept that background refreshes may slow down when no client reports.


## 5. Orchestration read model

### 5.1 Model and sequences

- The server is event-sourced. Every command appends events in one SQL transaction; events get a global `sequence` from `orchestration_events.sequence INTEGER PRIMARY KEY AUTOINCREMENT` (`apps/server/src/persistence/Migrations/001_OrchestrationEvents.ts:9`, append `apps/server/src/orchestration/Layers/OrchestrationEngine.ts:273-313`). They are published to subscribers only after commit (`:326-328`).
- The global counter has no holes, but every subscription sees gaps: other aggregates, filtered event types, and server-side coalescing (5.2, 5.3). Never treat a gap as missing data. Dedupe by "sequence <= what I have" and nothing else.
- Two projections for the client:
  - Shell: every project plus every active (not deleted, not archived) thread as `OrchestrationThreadShell` rows. Sidebar, inbox, status pills. No pagination (`apps/server/src/orchestration/Services/ProjectionSnapshotQuery.ts:118-131`).
  - Thread detail: one `OrchestrationThread` with `messages`, `activities`, `proposedPlans`, `checkpoints`, `session`. Chat view.
- `snapshotSequence` in both snapshots is the projector cursor read in the same transaction as the data (`ProjectionSnapshotQuery.ts:3700-3721`). Resume from it.
- Shapes: `OrchestrationProjectShell`, `OrchestrationThreadShell` (`orchestration.ts:884`), `OrchestrationThread` (`:793`), `OrchestrationMessage` (`:574`), `OrchestrationThreadActivity` (`:661`), `OrchestrationSession` (`:619`), `OrchestrationLatestTurn` (`:681`), `OrchestrationCheckpointSummary`, `OrchestrationProposedPlan`. All in Appendix A.

### 5.2 `orchestration.subscribeShell`

Input `{afterSequence?: number, requestCompletionMarker?: boolean}` (`orchestration.ts:990`). Items (`orchestration.ts:954-988`):

```jsonc
{"kind":"snapshot","snapshot":{"snapshotSequence":48200,"projects":[...],"threads":[...],"updatedAt":"..."}}
{"kind":"synchronized"}                                      // only if requestCompletionMarker: true
{"kind":"project-upserted","sequence":48201,"project":{...OrchestrationProjectShell}}
{"kind":"project-removed","sequence":48202,"projectId":"..."}
{"kind":"thread-upserted","sequence":48203,"thread":{...OrchestrationThreadShell}}
{"kind":"thread-removed","sequence":48204,"threadId":"..."}
```

Server behavior (`apps/server/src/ws.ts:2275-2430`):

- The live tail is attached before the snapshot or replay is read, so nothing published meanwhile is lost; overlap is possible and the client dedupes by sequence.
- No `afterSequence`: `snapshot`, then (if requested) `synchronized`, then live.
- With `afterSequence`: if `0 <= head - afterSequence <= 1000` (`SHELL_RESUME_MAX_GAP`, `ws.ts:377`) and the replay fits 8 MiB, the server replays the events after the cursor (as shell items, coalesced), then `synchronized`, then live. Otherwise (too far behind, or cursor ahead of head) it sends a fresh `snapshot` instead.
- `synchronized` is enqueued behind everything buffered during the snapshot/replay work, so it means "you are caught up" (`ws.ts:2354-2371`).
- Events become shell items by re-reading the current row (`ws.ts:841-973`): project created/updated gives `project-upserted` (or `project-removed` if gone); `project.deleted` gives `project-removed`; `thread.deleted` and `thread.archived` give `thread-removed`; any other thread event gives `thread-upserted` with the full current row (or `thread-removed` if no active row). The item's `sequence` is the triggering event's sequence.
- Coalescing: within 50 ms / 512 items only the newest event per aggregate survives (`ws.ts:975-1022`). Bursts of message deltas therefore produce one `thread-upserted`.
- `thread-removed` for a thread you do not have is a harmless no-op.
- Live budget: 1,000 items or 8 MiB un-acked, then the stream fails with `OrchestrationGetSnapshotError` (1.5).

Archived threads are not in the shell. `orchestration.getArchivedShellSnapshot {}` returns all projects plus archived threads, one-shot, no live updates (`ws.ts:2431-2447`). Archived threads have no detail view on this server version: thread snapshots and subscriptions report "not found" for them (`ProjectionSnapshotQuery.ts:1289-1331`).

### 5.3 `orchestration.subscribeThread`

Input (`orchestration.ts:1008`):

```jsonc
{"threadId":"...",
 "afterSequence":48203,          // resume cursor; omit for a fresh snapshot
 "requestCompletionMarker":true, // only if ServerConfig.threadResumeCompletionMarker
 "reasoningMessages":true,       // only if ServerConfig.reasoningMessages; else role "reasoning" is rewritten to "system"
 "turnLimit":10}                 // only if ServerConfig.threadSnapshotPagination; windows the snapshot to the last N user turns
```

Items (`orchestration.ts:2213`):

```jsonc
{"kind":"snapshot","snapshot":{"snapshotSequence":48203,"thread":{...OrchestrationThread},
  "page":{"beforeCursor":"<opaque>"|null,"hasMore":true,"snapshotSequence":48203,"threadSequence":48203}}}
{"kind":"synchronized"}
{"kind":"event","event":{...OrchestrationEvent}}
```

Server behavior (`apps/server/src/ws.ts:2448-2610`):

- Only six event types reach thread subscribers (`isThreadDetailEvent`, `ws.ts:348-368`): `thread.message-sent`, `thread.proposed-plan-upserted`, `thread.activity-appended`, `thread.turn-diff-completed`, `thread.reverted`, `thread.session-set`. Title, branch, model, archive, delete, settle/snooze/pin, `turn-start-requested`, interrupts, and every other thread event arrive only through the shell's `thread-upserted` / `thread-removed`.
- Resume with `afterSequence`: the server measures this thread's events in `(afterSequence, head]`. If there are at most 1,000 of them (`THREAD_RESUME_MAX_EVENTS`, `ws.ts:381`) and they fit in 8 MiB (`ws.ts:385`), it replays them as `event` items, then `synchronized`, then live. Otherwise it sends a fresh `snapshot`. If the range contains a `thread.created` / `project.created` (the thread was recreated) it prefers a snapshot (`ws.ts:2495-2557`).
- Snapshot path: `snapshot`, then `synchronized` (if requested), then live (`ws.ts:2559-2608`).
- Thread not found: the stream fails with `OrchestrationGetSnapshotError` `"Thread <id> was not found"` (`ws.ts:2585-2588`). There is no dedicated not-found error upstream.
- Live coalescing: a run of `tool.updated` activities is held for 50 ms and only the latest per `(turnId, toolCallId)` is kept (`apps/server/src/orchestration/ThreadLiveEventCoalescer.ts:56-94`). More sequence gaps. Replayed events are not coalesced.
- Outgoing projection (`apps/server/src/orchestration/ActivityPayloadProjection.ts:646-689`): activity payloads are slimmed (5.5); snapshots drop superseded `tool.updated` rows and all but the last `context-window.updated` per turn.
- Snapshot contents: the last 500 activities, plus the latest unresolved `approval.requested` / `user-input.requested` rows even if older (`ProjectionSnapshotQuery.ts:97,1470-1505,1865-1958`). With `turnLimit`, messages and turns are windowed by user turns (max 150 raw turns per page; cursor format `apps/server/src/orchestration/threadDetailCursor.ts:20-62`).
- Older pages: HTTP only, `GET /api/orchestration/threads/:id?turnLimit=20&beforeCursor=<page.beforeCursor>&reasoningMessages=true` (3.4).

### 5.4 Client reducer semantics (upstream)

Shell (`packages/client-runtime/src/state/shell.ts:141-258`, `state/shellReducer.ts:12-46`):

- State: `{snapshot: {snapshotSequence, projects, threads}, status: empty|cached|synchronizing|live}`.
- `snapshot`: replace everything.
- Event items: drop if `item.sequence <= snapshotSequence`; else upsert (replace the whole row by id, or append) or remove by id; then `snapshotSequence = item.sequence`.
- `synchronized`: status `live`. Without completion-marker support, set `live` right after subscribing.
- On each new connection: `GET /api/orchestration/shell`, apply it as a `snapshot`, then subscribe with `afterSequence = snapshotSequence`. If HTTP fails, subscribe without a cursor and take the socket snapshot.
- On stream error: retry after 250 ms with the current cursor. On app foreground: resubscribe.

Thread detail (`packages/client-runtime/src/state/threads.ts:440-861`, `state/threadReducer.ts:101-747`):

- State per thread: `{data: OrchestrationThread | null, lastSequence, status: empty|cached|synchronizing|live|deleted, page: {beforeCursor, hasMore, loadingOlder}, historyEpoch}`.
- Subscribe: if there is no data, `GET /api/orchestration/threads/:id?turnLimit=10&reasoningMessages=true` first; then subscribe with `afterSequence = lastSequence` (only if data exists), `requestCompletionMarker`, `reasoningMessages`, `turnLimit: 10` (`threads.ts:49-50,802-847`).
- `snapshot`: replace the thread (including older pages already merged), `lastSequence = snapshotSequence`, page from `snapshot.page`, bump `historyEpoch`.
- `event`: drop if `event.sequence <= lastSequence`; else `lastSequence = event.sequence` and apply:

| Event | Transition |
|---|---|
| `thread.message-sent` | Find message by `payload.messageId`. Found: if `streaming: true`, append `payload.text` to the existing text; if `streaming: false`, replace text only when `payload.text` is non-empty (completion events carry `""`). Update `streaming`, `turnId`, attachments/context if present, `updatedAt` only when not streaming. Not found: push a new message. For assistant messages with a `turnId`, set `latestTurn` to `running`, or to `completed` when not streaming and the session is no longer running that turn (`threadReducer.ts:389-487`). |
| `thread.session-set` | Replace `session`. If `running` with `activeTurnId`, `latestTurn` = that turn, `running`. Else settle a running `latestTurn`: `ready`/`idle` to `completed`, `error` to `error`, `interrupted`/`stopped` to `interrupted` (`:490-537,756-772`). |
| `thread.turn-diff-completed` | Upsert checkpoint by `turnId`, sort by `checkpointTurnCount`. Never overwrite a non-`missing` checkpoint with `missing` (mid-turn placeholder). Settle `latestTurn` if the session is not running that turn (`:574-623`). |
| `thread.reverted` | Keep checkpoints with `checkpointTurnCount <= turnCount`, messages/plans/activities of retained turns (null `turnId` kept, plus system and imported messages), rebuild `latestTurn`. Bump `historyEpoch` (`:626-677,832-875`). |
| `thread.activity-appended` | Upsert by `activity.id` (stable ids replace, e.g. `task-progress:*`). Keep sorted by `(activity.sequence ?? +inf, createdAt, id)`. A new resolvable `context-window.updated` removes earlier ones in the same turn (`:680-736`). |
| `thread.proposed-plan-upserted` | Replace by plan id, sort by `(createdAt, id)` (`:557-571`). |
| anything else | No change (the reducer handles the shell-only events too, but they never arrive on this stream). |

Server-side truth matches: deltas are emitted as `{text: <delta>, streaming: true}` and completion as `{text: "", streaming: false}` (`apps/server/src/orchestration/decider.ts:1935-2001`, `Layers/ProjectionPipeline.ts:1146-1198`). Assistant message ids look like `assistant:<key>[:segment:N]`, reasoning ids `reasoning:...` (`Layers/ProviderRuntimeIngestion.ts:317-333`). The user message from `thread.turn.start` has `turnId: null` and the client's `messageId`.

Merging shell and detail (`packages/client-runtime/src/state/threadDetail.ts:246-283`): for display, the shell row overrides the detail's `projectId`, `title`, `modelSelection`, `runtimeMode`, `interactionMode`, `branch`, `worktreePath`, `latestTurn`, `session`, timestamps, and all settle/snooze/pin fields. The detail owns `messages`, `activities`, `proposedPlans`, `checkpoints`.

Older pages (`threads.ts:589-716`): drop the response if `historyEpoch` changed or `snapshot.snapshotSequence < lastSequence`; if `page.threadSequence > lastSequence`, hold it until live events catch up; then prepend rows deduped by id (checkpoints by `turnId`).

Recovery: there is no gap detection. Recovery happens on reconnect (new session), app foreground, or the 250 ms retry after a stream error. Each resubscribe recomputes `afterSequence` from current state and lets the server pick replay or snapshot.

Optimistic UI:

- User messages: the client picks `messageId`, shows a local copy until a server message with that id appears, removes it and restores the draft if dispatch fails (`apps/web/src/components/ChatView.tsx:1698,3489-3498,5780-5807`).
- Lifecycle commands (settle, snooze, pin, reorders): overlay a pending patch on the shell row until `shell.snapshotSequence >= DispatchResult.sequence`; drop it on failure (`packages/client-runtime/src/state/threadLifecycle.ts:18-101`, `threadCommands.ts:267-340`).
- Approvals: no optimistic removal; disable the buttons until the call returns.
- Retrying a dispatch with the same `commandId` is idempotent: the engine returns the stored receipt's sequence (or the stored rejection) instead of applying it again (`OrchestrationEngine.ts:144-172`).

### 5.5 Activities and the work log

`OrchestrationThreadActivity` (`orchestration.ts:661`): `{id, tone: "info"|"tool"|"approval"|"error", kind: string, summary, payload: unknown, turnId, sequence?, createdAt}`. `activity.sequence` is the provider runtime's session sequence, not the orchestration event sequence.

Kinds the server produces (from `apps/server/src/orchestration/Layers/ProviderRuntimeIngestion.ts`, `ProviderCommandReactor.ts`, `CheckpointReactor.ts`, `decider.ts`, `ws.ts`):

| kind | tone | payload fields the UI reads |
|---|---|---|
| `tool.started` / `tool.updated` / `tool.completed` | tool | `itemType` (`command_execution`, `file_change`, `mcp_tool_call`, `dynamic_tool_call`, `collab_agent_tool_call`, `web_search`, `image_view`; `packages/contracts/src/providerRuntime.ts:106-121`), `toolCallId?`, `status?`, `title?`, `detail?`, `toolSurface?`, `toolIcon?`, `toolSource?`, `agentId?`, `parentToolUseId?`, `data?` (slimmed, below) |
| `approval.requested` / `approval.resolved` | approval | `requestId`, `requestKind?` (`command`/`file-read`/`file-change`/`mcp-elicitation`/`permission`), `requestType`, `detail?`, `appName?`, `options?` / `decision?` |
| `user-input.requested` / `user-input.resolved` / `user-input.answer-submitted` | info | `requestId`, `questions[]`, `responseMode?: "message"` / `answers?` / `questionTextById`, `attachmentsByQuestionId` |
| `turn.plan.updated` | info | `plan: [{step, status}]`, `explanation?` |
| `task.started` / `task.progress` / `task.updated` / `task.completed` | info (error on failure) | `taskId`, `title?`, `detail?`, `summary?`, `status?`, `usage?`, subagent linkage (`agentKind`, `agentId`, `role`, `model`, `toolUseId`, `parentAgentId`, `workflowName`, ...; `ProviderRuntimeIngestion.ts:451-490`) |
| `tool.progress` | info | `taskId`, `toolName?`, `toolUseId?`, `elapsedSeconds?` |
| `context-window.updated` | info | token usage snapshot (`usedTokens`, ...) |
| `context-compaction` | info | `state`, `beforeTokens?`, `afterTokens?` |
| `runtime.error` / `runtime.warning` / `tool.denied` | error / info / error | `message`, `code?`, `detail?`, `toolName` |
| `provider.turn.start.failed`, `provider.turn.interrupt.failed`, `provider.approval.respond.failed`, `provider.user-input.respond.failed`, `provider.session.stop.failed` | error | `detail`, `requestId?` (for `turn.start.failed`, the user `messageId`) |
| `provider.auth.signed-out` | info | `providerInstanceId` |
| `checkpoint.captured` / `checkpoint.capture.failed` / `checkpoint.revert.failed` | info / error | `turnCount`, `status` / `detail` |
| `setup-script.requested` / `.started` / `.failed`, `worktree-setup` | info or error | `scriptId`, `scriptName`, `terminalId`, `worktreePath` / `detail`; `WorktreeSetupSnapshot` |

Stable ids replace earlier rows: `task-progress:<threadId>:<taskId>`, `task-usage:...`, `tool-progress:...`, `async-answer:<requestId>`, `async-dismiss:<requestId>`, `settle:<cmd>:<requestId>`.

Payload slimming on the wire (`ActivityPayloadProjection.ts:425-501`): only payloads with a `data` object are rewritten. `data` is reduced to `item {command, aggregatedOutput (first-line summary), input.command, result {command, content summary}}`, `command`, `imagePath`, `files[{path}]` (max 12), `toolCallId`, `kind`, `toolName`, `rawOutput {content summary | totalFiles, truncated}`, and question-tool input. MCP items keep `type, id, tool, server, status, arguments, appContext, error, durationMs` plus a result summary. `status` may be rewritten to `failed` / `declined` from `data.item.status`.

Work-log derivation is client-side (`apps/web/src/session-logic.ts`, `packages/client-runtime/src/work-log/*`):

- Order: `activity.sequence` (missing first), then `createdAt`, then lifecycle rank (`.started` < `.progress`/`.updated` < `.completed`/`.resolved`), then id (`session-logic.ts:1391-1430`).
- Hidden: `tool.started`, `task.updated`, `tool.progress`, `context-window.updated`, `turn.plan.updated`, non-agent `task.started`, "Checkpoint captured", successful worktree-setup rows, agent-internal rows (`payload.agentId` or `timelineBypass`), plan-mode exit rows (`session-logic.ts:451-515`, `work-log/presentation.ts:19-25`).
- Tool rows merge into one entry per `tool:<turnId>:<toolCallId>`; `tool.completed` closes it. Agent task rows collapse into one "spawn" row per workflow or turn (`session-logic.ts:718-923`).
- Entry fields: label (task summary or `activity.summary`), tone (`task.progress` renders as "thinking"), detail, command (`data.item.command` / `item.input.command` / `item.result.command` / `data.command` / `detail` for `command_execution`), changed files (walks `data`), tool title, item type, request kind, lifecycle status (`session-logic.ts:542-680,1110-1143,1336-1389`).
- Question rows: `requested` / `resolved` / `answer-submitted` for one `requestId` fold into a single row (`packages/client-runtime/src/work-log/userInput.ts:77-157`).
- Timeline: messages, proposed plans, and work entries merged and sorted by `createdAt` (`session-logic.ts:1654-1715`).
- Active plan card: the latest `turn.plan.updated` for the latest turn (`session-logic.ts:324-351`).

The fork's work log (`F:apps/web/src/session-logic.ts`) hides fewer kinds; match whichever the visual reference needs.

### 5.6 Pending approvals and user input

`derivePendingRequests(activities)` (`packages/client-runtime/src/pendingRequests.ts:124-198`):

- Approval opens on `approval.requested` (unless `requestType` is `tool_user_input` or `auth_tokens_refresh`), keyed by `payload.requestId`. It closes on `approval.resolved`, or on `provider.approval.respond.failed` whose detail says the request is stale/unknown (`:100-113`). Other failures leave it open for retry.
- User input opens on `user-input.requested` with at least one parseable question; `dismissible = responseMode === "message"`. It closes on `user-input.resolved` or a stale `provider.user-input.respond.failed`.
- A terminal row wins regardless of order. Sorted by `createdAt`.
- Answer with `thread.approval.respond` / `thread.user-input.respond` / `thread.user-input.dismiss` (6.2). Async answers (`responseMode: "message"`) become a new turn whose user message id is `async-answer:<requestId>` (`decider.ts:1650-1730`).
- The sidebar uses the shell booleans `hasPendingApprovals` / `hasPendingUserInput` instead; the server clears `hasPendingApprovals` as soon as the response is requested (`ProjectionPipeline.ts:1784-1951`).

### 5.7 Turns, sessions, status

- `session.status`: `idle|starting|running|ready|interrupted|stopped|error`. `starting` when a turn start is requested and no session runs; `running` with `activeTurnId` on turn start; `ready` (or `error`) on turn end; `interrupted` on abort; `stopped` on exit. The server never produces `idle` (`ProviderRuntimeIngestion.ts:399-417,1880-1960`, `ProviderCommandReactor.ts:655-669`).
- `latestTurn.state`: `running|interrupted|completed|error`. Authoritative on the shell row.
- Checkpoints: `thread.turn-diff-completed` with `status: "missing"` is a mid-turn placeholder; the real capture follows with `ready`. Diff text comes from `orchestration.getTurnDiff` / `getFullThreadDiff`.
- Proposed plans are upserted whole when complete. Implementing one is `thread.turn.start` with `sourceProposedPlan`; the plan then gets `implementedAt` / `implementationThreadId`. Shell `hasActionableProposedPlan` is true when the newest plan of the latest turn is unimplemented.
- Sidebar status priority (`apps/web/src/components/Sidebar.logic.ts:1040-1121`): Pending Approval, Awaiting Input, Working (`session.status == running`), Connecting (`starting`), Plan Ready, Working (`backgroundLiveness == "working"`), Monitoring, Completed (unseen). Upstream has no Error pill.
- "Completed / unread" is client-local in upstream: `lastVisitedAt` per thread in local storage; unread when `latestTurn.completedAt > lastVisitedAt`; opening a thread sets `lastVisitedAt = latestTurn.completedAt`; "mark unread" sets it to `completedAt - 1ms` (`apps/web/src/uiStateStore.ts:250-296`, `Sidebar.logic.ts:673-682`, `ChatView.tsx:2120-2137`). Nothing syncs across clients.

### 5.8 Fork differences (read model)

| Fork (`F`) | Upstream replacement |
|---|---|
| `orchestration.replayEvents {fromSequenceExclusive}` returns up to 1,000 events (`F:packages/contracts/src/orchestration.ts:29,1282`, `F:apps/server/src/ws.ts:1043-1065`). Not called by the fork's web UI or client-runtime. | None. Resubscribe with `afterSequence`; the server replays (bounded) or sends a snapshot. |
| `server.ping` (`F:packages/contracts/src/rpc.ts:205,257`). Not called by the fork's UI. | `server.probe {}`, used as a liveness probe when `capabilities.connectionProbe`. RPC-level `Ping`/`Pong` (1.6) is separate and still exists. |
| `thread.completion.acknowledge {completedAt}` command, `thread.completion-acknowledged` event, `completionAcknowledgedAt` on thread and shell, "Error" pill (`F:packages/shared/src/threadStatus.ts:45-100`). | Removed. Unread is client-local `lastVisitedAt` (5.7). "Done" is the server-side settle model (`thread.settle`/`unsettle`, `settledOverride`, `settledAt`, auto-settle settings). |
| `apps/web/src/orchestrationRecovery.ts` gap detection (`sequence !== latest + 1` triggers replay). Dead code in the fork (only its test imports it). | No gap detection (5.4). |
| No `synchronized` item, no `requestCompletionMarker`, `turnLimit`, `reasoningMessages`, no `page` in the thread snapshot. | As described in 5.2 and 5.3. |
| Thread not found is `{"_tag":"OrchestrationThreadNotFoundError","threadId"}`. | `OrchestrationGetSnapshotError` with message `Thread <id> was not found`. |
| Server replays unbounded with no snapshot fallback, no coalescing, no live budget, unslimmed activity payloads. | Bounded replay, coalescing, budget, slimmed payloads. |
| Missing: `thread.settled/unsettled/snoozed/unsnoozed/pinned/unpinned/pin-reordered/auto-settle-set/pull-request-linked/unlinked/synced` events and the matching commands, `thread.user-input.dismiss`, `thread.conversation.revert`, message role `reasoning`, `message.context`, PR fields, settle/snooze/pin fields, `backgroundLiveness`, `planProgress`, project `defaultThreadEnvMode`/`autoPull`/`faviconPath`/`projectIcon`. | Present upstream. |

## 6. Commands (`orchestration.dispatchCommand`)

### 6.1 Envelope and result

The payload is the command object itself, discriminated by `type` (`ClientOrchestrationCommand`, `packages/contracts/src/orchestration.ts:1461`). Full union in Appendix A.

```jsonc
// C->S
{"_tag":"Request","id":"12","tag":"orchestration.dispatchCommand","headers":[],
 "payload":{"type":"thread.archive","commandId":"6f1c...","threadId":"9a7e..."}}
// S->C success
{"_tag":"Exit","requestId":"12","exit":{"_tag":"Success","value":{"sequence":48211}}}
// S->C failure
{"_tag":"Exit","requestId":"12","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":
  {"_tag":"OrchestrationDispatchCommandError","message":"...","bootstrapThreadDisposition":"not-created"}}]}}
```

- `commandId`: a fresh UUID v4 per command (`packages/client-runtime/src/operations/commands.ts:68-77`). It reappears as `commandId` on the resulting events. Re-dispatching the same `commandId` is idempotent: the engine returns the stored receipt's `sequence` (or the stored rejection) and does not apply it twice (`apps/server/src/orchestration/Layers/OrchestrationEngine.ts:144-172`). Reusing it for a different aggregate fails with a conflict error. So it is safe to retry a dispatch whose reply was lost.
- `createdAt`: required on many commands, but the server overwrites it with its own receipt time (`apps/server/src/orchestration/Normalizer.ts:77-80`, test `Normalizer.test.ts:16-31`). Send the local time; do not rely on it.
- `sequence` in the result is the sequence of the last event the command produced. The events themselves arrive on the subscriptions, not in this reply. Upstream uses it for optimistic patches: keep the patch until `shell.snapshotSequence >= sequence` (5.4).
- `bootstrapThreadDisposition` (`orchestration.ts:2410-2417`): on a failed `thread.turn.start` with `bootstrap.createThread`, `"not-created"` / `"deleted"` means the thread does not exist, so a retry cannot create a duplicate.
- Server-side extras: `project.create` / `project.meta.update` normalize `workspaceRoot` (`Normalizer.ts:86-135`). `thread.archive` also stops a live session and closes the thread's terminals (`apps/server/src/ws.ts:2147-2210`). Commands for a project that is still cloning are rejected (`ws.ts:2144`).

### 6.2 Command catalog

All have `type` and `commandId`. `?` keys are optional (omit them; see section 2).

| type | other fields | notes |
|---|---|---|
| `project.create` | `projectId`, `title`, `workspaceRoot`, `createWorkspaceRootIfMissing?`, `defaultModelSelection?`, `createdAt` | `projectId` is a client UUID |
| `project.meta.update` | `projectId`, `title?`, `workspaceRoot?`, `defaultModelSelection?`, `defaultThreadEnvMode?` (`local`/`worktree`), `autoPull?`, `faviconPath?`, `projectIcon?`, `scripts?` (full array, replaces) | project scripts are edited here |
| `project.delete` | `projectId`, `force?` | |
| `thread.create` | `threadId`, `projectId`, `title`, `modelSelection`, `runtimeMode`, `interactionMode` (`default`/`plan`, nullable), `branch` (nullable), `worktreePath` (nullable), `createdAt`, `historyImport?: true` | upstream UI prefers `thread.turn.start` + `bootstrap.createThread` |
| `thread.delete` / `thread.archive` / `thread.unarchive` | `threadId` | |
| `thread.settle` / `thread.unsettle` | `threadId`; unsettle has `reason: "user"` | upstream "done" state; capability `threadSettlement` |
| `thread.snooze` / `thread.unsnooze` | `threadId`, `snoozedUntil` (ISO) / `reason: "user"` | capability `threadSnooze` |
| `thread.pin` / `thread.unpin` / `thread.pin.reorder` | `threadId`, `orderKey?` / - / `orderKey` | capability `threadPinning`, `threadPinReorder` |
| `thread.active.reorder` | `threadId`, `orderKey` | capability `threadActiveReorder` |
| `thread.auto-settle.set` | `threadId`, `enabled` | capability `threadAutoSettleOptOut` |
| `thread.meta.update` | `threadId`, `title?`, `regenerateTitle?: true`, `modelSelection?`, `branch?`, `expectedBranch?`, `worktreePath?`, `linkedPullRequest?` | rename = `{title}` |
| `thread.pull-request.link` / `.unlink` | `threadId`, `host`, `repository`, `number`, `url`, `source` / `threadId`, `host`, `repository`, `number` | |
| `thread.runtime-mode.set` | `threadId`, `runtimeMode` (`approval-required`/`auto-accept-edits`/`auto`/`full-access`), `createdAt` | |
| `thread.interaction-mode.set` | `threadId`, `interactionMode` (`default`/`plan`), `createdAt` | |
| `thread.turn.start` | see 6.3 | |
| `thread.turn.interrupt` | `threadId`, `turnId?`, `createdAt` | stop button |
| `thread.approval.respond` | `threadId`, `requestId`, `decision` (`accept`/`acceptForSession`/`acceptAlways`/`decline`/`cancel`), `createdAt` | |
| `thread.user-input.respond` | `threadId`, `requestId`, `answers` (record keyed by question id, values are JSON), `attachmentsByQuestionId?`, `createdAt` | |
| `thread.user-input.dismiss` | `threadId`, `requestId`, `createdAt` | only async questions can be dismissed |
| `thread.checkpoint.revert` | `threadId`, `turnCount`, `createdAt` | restores files and history |
| `thread.conversation.revert` | `threadId`, `turnCount`, `createdAt` | history only, files untouched |
| `thread.session.stop` | `threadId`, `createdAt`, `onlyIfSettled?` | |

Fork-only: `thread.completion.acknowledge` (`F:packages/contracts/src/orchestration.ts:578`, dispatched from `F:packages/client-runtime/src/operations/commands.ts:197`). Upstream has no such command or event. See 5.7 and 5.8 for what replaces it.

### 6.3 `thread.turn.start`

```jsonc
{
  "type": "thread.turn.start",
  "commandId": "c0b2...",
  "threadId": "9a7e...",            // new UUID when bootstrapping a new thread
  "message": {
    "messageId": "5d1f...",          // client UUID; the user message event will carry this id
    "role": "user",
    "text": "Fix the failing test",
    "attachments": [
      {"type":"image","id":"att_...","name":"shot.png","mimeType":"image/png","sizeBytes":12345},          // pre-uploaded (3.5)
      {"type":"image","name":"paste.png","mimeType":"image/png","sizeBytes":2048,"dataUrl":"data:image/png;base64,..."}, // inline
      {"type":"file","id":"att_...","name":"log.txt","mimeType":"text/plain","sizeBytes":999}
    ],
    "context": { "version": 1, "records": [ ... ] }   // optional; only if capabilities.inlineMessageContext
  },
  "modelSelection": {"instanceId":"codex","model":"gpt-5.5","options":[{"id":"effort","value":"high"}]}, // optional
  "titleSeed": "Fix the failing test",   // optional
  "runtimeMode": "full-access",          // required on the client command
  "interactionMode": "default",          // required on the client command
  "bootstrap": {                         // optional
    "createThread": {"projectId":"...","title":"...","modelSelection":{...},"runtimeMode":"full-access",
                     "interactionMode":"default","branch":"main","worktreePath":null,"createdAt":"..."},
    "prepareWorktree": {"projectCwd":"/repo","baseBranch":"main","branch":"t3/abc123","startFromOrigin":true,"requireWorktree":true},
    "runSetupScript": true
  },
  "sourceProposedPlan": {"threadId":"...","planId":"..."},  // optional: "implement this plan"
  "createdAt": "2026-10-01T12:00:00.000Z"
}
```

How the upstream UI uses it (`apps/web/src/components/ChatView.tsx:7985-8030,8335-8420`):

- New thread from a draft: one `thread.turn.start` with `bootstrap.createThread`. Add `prepareWorktree` + `runSetupScript: true` for worktree mode.
- Existing thread: first send `thread.meta.update` (model or branch changed), `thread.runtime-mode.set`, and `thread.interaction-mode.set` only for the values that changed, then `thread.turn.start` (`ChatView.tsx:5217-5285`).
- `context` is only sent when `ExecutionEnvironmentCapabilities.inlineMessageContext` is true; otherwise the UI folds context into the text.
- Limits: 120,000 chars of input text, 100 attachments, 10 MiB per image, 80 MiB of images, 50 MiB per file (`orchestration.ts:165-184`). Duplicate attachment ids in one message are rejected (`Normalizer.ts:148-160`).

## 7. Server config, providers, settings, keybindings, lifecycle, auth access

### 7.1 `subscribeServerConfig`

Payload: `{}` (or opt-in flags `environmentThemes`, `usageLimitSources`, `usageLimitsCommand`; each enables one extra event type, see below). Items are `ServerConfigStreamEvent` (`server.ts:770`), each with `"version":1`:

| `type` | payload | when |
|---|---|---|
| `snapshot` | `config: ServerConfig` | always first |
| `keybindingsUpdated` | `{keybindings, issues}` | keybindings file changed |
| `providerStatuses` | `{providers: ServerProvider[]}` | provider list or status changed. Debounced; identical lists are dropped (`apps/server/src/ws.ts:3912-3946`) |
| `settingsUpdated` | `{settings: ServerSettings}` | settings changed (redacted, full object) |
| `environmentThemesUpdated` | `{themes}` | only if payload had `environmentThemes: true` |
| `usageLimitSourcesUpdated` | `{sources}` | only if payload had `usageLimitSources: true` |

Handler: `apps/server/src/ws.ts:3898-3998`. Each event replaces that slice of the config; there are no partial patches. `server.getConfig` returns the same `ServerConfig` as a one-shot. Upstream's session uses the first `snapshot` as the connection's initial config and replays a merged snapshot to late subscribers (`packages/client-runtime/src/rpc/session.ts:223-330`, `state/serverConfigProjection.ts`).

`ServerConfig` (`server.ts:578`, Appendix A) fields that drive client behavior:

- `environment`: `ExecutionEnvironmentDescriptor` (`environmentId`, `label`, `platform`, `serverVersion`, `orchestrationProtocolVersion?`, `capabilities`). Gate features on `capabilities.*` (all optional booleans: `attachmentUploads`, `inlineMessageContext`, `threadSettlement`, `threadSnooze`, `threadPinning`, `pullRequests`, `connectionProbe`, ...; `environment.ts:89-148`).
- `auth`: `ServerAuthDescriptor` (policy, bootstrap and session methods, cookie name).
- `cwd`, `keybindingsConfigPath`, `keybindings`, `issues`, `providers`, `availableEditors`, `remoteOpenTargets?`, `observability`, `settings`.
- Protocol feature flags (optional booleans): `shellResumeCompletionMarker`, `threadResumeCompletionMarker`, `threadSnapshotPagination`, `reasoningMessages` (section 5.3), plus `shellRevealInFileManager[Kind]`, `scratchWorkspaceRoot`, `newProjectsRoot`.

### 7.2 Providers, models, options

`ServerConfig.providers` is `ForwardCompatibleArray(ServerProvider)` (`server.ts:207-271`): skip entries you cannot decode.

- One entry per configured provider instance. `instanceId` is the routing key used in `ModelSelection.instanceId`. `driver` is the implementation (`codex`, `claudeAgent`, `cursor`, `opencode`, ...; open string). `displayName?`, `accentColor?`, `badgeLabel?` drive the picker.
- Health: `enabled`, `installed`, `version`, `status` (`ready`/`warning`/`error`/`disabled`), `auth.status` (`authenticated`/`unauthenticated`/`unknown`), `message?`, `availability?`, `checkedAt`.
- Behavior flags: `showInteractionModeToggle?` (plan mode), `requiresNewThreadForModelChange?`, `supportsConversationRollback?`, `reportsContextWindow?`, `supportsTextGeneration?`.
- `models: ServerProviderModel[]` (`server.ts:71`): `slug` (goes in `ModelSelection.model`), `name`, `shortName?`, `subProvider?`, `aliases?`, `isCustom`, `isDefault?`, `isLegacy?`, `badge?: "new"`, `capabilities.optionDescriptors?`.
- Option descriptors ("traits", `model.ts:7-45`): `{"type":"select","id":"effort","label":"Reasoning","options":[{"id":"high","label":"High","isDefault":true}],"currentValue"?,"promptInjectedValues"?}` or `{"type":"boolean","id":"fastMode","label":"Fast","currentValue"?}`. The user's picks become `ModelSelection.options = [{"id":"effort","value":"high"},{"id":"fastMode","value":true}]`.
- `slashCommands`, `skills`, `usageLimits?`, `updateState?`, advisories: optional extras for the composer and settings.
- `server.refreshProviders {instanceId?, cwd?, fresh?, refreshModels?}` and `server.updateProvider` return `{providers}`; the same list also arrives as `providerStatuses`.

### 7.3 Settings

- `server.getSettings {}` returns `ServerSettings` (`settings.ts:1109`). Secrets are redacted (`ServerSettings.redactServerSettingsForClient`).
- `server.updateSettings {"patch": ServerSettingsPatch}` (`settings.ts:1470`). The patch is a deep partial; the server deep-merges it (`apps/server/src/serverSettings.ts:241-259,1020-1026`) and returns the full redacted settings. Every subscriber also gets `settingsUpdated`.
- Client-only preferences (`ClientSettingsSchema`, `settings.ts:298`: chat width, font sizes, diff layout, confirmations, ...) are never sent to the server. Upstream stores them locally (`apps/web/src/clientPersistenceStorage.ts`).

### 7.4 Keybindings

- Resolved rules arrive in `ServerConfig.keybindings` and `keybindingsUpdated`: `{command, shortcut: {key, metaKey, ctrlKey, shiftKey, altKey, modKey}, whenAst?}` (`keybindings.ts:131-190`). `modKey` means Cmd on macOS, Ctrl elsewhere (`apps/web/src/keybindings.ts:118-119`). `whenAst` is `{type:"identifier",name}` / `not` / `and` / `or`.
- `command` is a fixed literal set (`keybindings.ts:116`, ~80 values like `chat.new`, `thread.stop`, `modelPicker.jump.3`) or `script.<id>.run`. Unknown commands are dropped by the forward-compatible decoder; do the same.
- Edit: `server.upsertKeybinding {"key":"mod+shift+k","command":"chat.new","when"?:"...","replace"?:{"key","command","when"?}}` and `server.removeKeybinding {"key","command","when"?}`. Both return `{keybindings, issues}`. Key string grammar (`packages/shared/src/keybindings.ts:95-160`, `parseKeybindingShortcut`): lowercase tokens joined by `+`; modifiers `cmd`/`meta`, `ctrl`/`control`, `shift`, `alt`/`option`, `mod`; the remaining token is the key (a trailing `+` means the plus key).

### 7.5 Lifecycle

`subscribeServerLifecycle {}` (`apps/server/src/ws.ts:4003-4024`): replays buffered events sorted by `sequence`, then live ones.

- `{"version":1,"sequence":n,"type":"welcome","payload":{"environment","cwd","projectName","bootstrapStatus"?,"bootstrapProjectId"?,"bootstrapThreadId"?,...}}`
- `{"version":1,"sequence":n,"type":"ready","payload":{"at","environment","updateOutcome"?:{"id","fromVersion","targetVersion","status":"committed"|"rolled-back"|"failed","reason"?}}}`

The fork uses it to show "server ready" and post-update notices.

### 7.6 Auth access (settings > devices)

`subscribeAuthAccess {}` needs `access:read` (`apps/server/src/ws.ts:4025-4056`). First item `{"version":1,"revision":1,"type":"snapshot","payload":{"pairingLinks":[AuthPairingLink],"clientSessions":[AuthClientSession]}}`, then `pairingLinkUpserted` / `pairingLinkRemoved {id}` / `clientUpserted` / `clientRemoved {sessionId}`, each with an incrementing `revision`. A bearer token issued with standard scopes cannot call this; it gets `EnvironmentAuthorizationError` with `requiredScope: "access:read"`.

## 8. Terminal, VCS, filesystem, project scripts

Shapes only (Appendix A has full definitions).

### 8.1 Terminal

Terminals are keyed by `(threadId, terminalId)`. The fork uses `terminalId` values like `"default"`, `"terminal-2"`.

| RPC | Payload | Result |
|---|---|---|
| `terminal.open` | `TerminalOpenInput {threadId, terminalId, cwd, worktreePath?, cols?, rows?, env?, providerInstanceId?}` | `TerminalSessionSnapshot {threadId, terminalId, cwd, worktreePath, status: starting/running/exited/error, pid, history, exitCode, exitSignal, label, updatedAt, sequence?}` |
| `terminal.attach` (stream) | `TerminalAttachInput {threadId, terminalId, cwd?, worktreePath?, cols?, rows?, env?, providerInstanceId?, restartIfNotRunning?}` | items: `{"type":"snapshot","snapshot"}` first, then `output {data}`, `exited {exitCode, exitSignal}`, `closed`, `error {message}`, `cleared`, `restarted {snapshot}`, `activity {hasRunningSubprocess, label}`, each with `threadId`, `terminalId`, `sequence?` |
| `terminal.write` | `{threadId, terminalId, data}` | `null` |
| `terminal.resize` | `{threadId, terminalId, cols, rows}` | `null` |
| `terminal.clear` | `{threadId, terminalId}` | `null` |
| `terminal.restart` | `{threadId, terminalId, cwd, worktreePath?, cols, rows, env?, providerInstanceId?}` | `TerminalSessionSnapshot` |
| `terminal.close` | `{threadId, terminalId?, deleteHistory?}` (no `terminalId` = all of the thread's terminals) | `null` |
| `subscribeTerminalEvents` (stream) | `{}` | `TerminalEvent` for all terminals: the attach variants, with `started {snapshot}` in place of `snapshot` |
| `subscribeTerminalMetadata` (stream) | `{}` | `{"type":"snapshot","terminals":[TerminalSummary]}`, then `upsert {terminal}` / `remove {threadId, terminalId}` |

`data` in `output` is raw terminal bytes as a UTF-8 string (feed it to the VT parser). `history` in the snapshot is the scrollback to replay before live output. All terminal RPCs need `terminal:operate`. Terminal streams use the windowed ack (1.5).

### 8.2 Project scripts

There is no script RPC. Scripts live in `project.scripts` (`ProjectScript {id, name, command, icon: play/test/lint/configure/build/debug, runOnWorktreeCreate, async?, previewUrl?, autoOpenPreview?}`) and are edited with `project.meta.update {scripts}` (replaces the array). Defaults are in `ServerSettings.defaultProjectScripts` and `projectScriptOverrides`.

Running one (`F:apps/web/src/components/ChatView.tsx:2611-2700`):

1. `terminal.open {threadId, terminalId, cwd: <worktree or project root>, worktreePath?, env: {T3CODE_PROJECT_ROOT: <project root>, T3CODE_WORKTREE_PATH?: <worktree>}, cols?, rows?}` (env from `F:packages/shared/src/projectScripts.ts:20-33`). Use a new `terminalId` if the current one is busy.
2. `terminal.write {threadId, terminalId, data: "<command>\r"}`.

Keybinding commands `script.<id>.run` map to scripts by id.

### 8.3 VCS / git

| RPC | Payload | Result |
|---|---|---|
| `subscribeVcsStatus` (stream) | `{cwd}` | `VcsStatusStreamEvent`, tagged by `_tag`: `snapshot {local, remote}`, `localUpdated {local}`, `remoteUpdated {remote}` (`git.ts:253`) |
| `vcs.refreshStatus` | `{cwd}` | `VcsStatusResult` (local and remote fields merged) |
| `vcs.listRefs` | `{cwd, query?, cursor?, includeMatchingRemoteRefs?, refKind?: all/local/remote, refresh?, limit?}` | `{refs: VcsRef[], isRepo, hasPrimaryRemote, nextCursor, totalCount}` |
| `vcs.switchRef` / `vcs.createRef` | see Appendix A | |
| `vcs.createWorktree` / `vcs.removeWorktree` | see Appendix A | |
| `vcs.pull` | `{cwd}` | `VcsPullResult` |
| `vcs.init` | `{cwd, kind?: git/jj}` | `null` |
| `git.runStackedAction` (stream) | `{actionId, cwd, action: commit/push/create_pr/commit_push/commit_push_pr, commitMessage?, featureBranch?, filePaths?, threadId?}` | `GitActionProgressEvent` items, final result in the last item |
| `git.resolvePullRequest`, `git.preparePullRequestThread` | see Appendix A | |
| `review.getDiffPreview` | `ReviewDiffPreviewInput` | `ReviewDiffPreviewResult` (needs `review:write`) |
| `orchestration.getTurnDiff` | `{threadId, fromTurnCount, toTurnCount, ignoreWhitespace?}` | `{threadId, fromTurnCount, toTurnCount, diff}` (unified diff text) |
| `orchestration.getFullThreadDiff` | `{threadId, toTurnCount, ignoreWhitespace?}` | same shape |

`VcsStatusLocalResult`: `isRepo`, `sourceControlProvider?`, `hasPrimaryRemote`, `isDefaultRef`, `refName`, `hasWorkingTreeChanges`, `workingTree {files: [{path, insertions, deletions}], insertions, deletions}`. `VcsStatusRemoteResult`: `hasUpstream`, `aheadCount`, `behindCount`, `aheadOfDefaultCount?`, `pr` (nullable).

### 8.4 Filesystem and search

| RPC | Payload | Result |
|---|---|---|
| `filesystem.browse` | `{partialPath, cwd?}` | `{parentPath, entries: [{name, fullPath}]}` (add-project path picker) |
| `projects.searchEntries` | `{cwd, query, limit, kind?: file/directory, imageOnly?}` | `{entries: [{path, kind, ignored?}], truncated}` (`@` mentions, file picker) |
| `projects.searchContents` | `ProjectSearchContentsInput` | `{matches: ProjectContentMatch[], truncated, regexFallbackError?}` (upstream only) |
| `projects.listEntries` | `{cwd, directoryPath?}` | `{entries, truncated}` |
| `projects.readFile` | `{cwd, relativePath}` | `{relativePath, contents, byteLength, truncated}` |
| `projects.writeFile` | `{cwd, relativePath, contents}` | `{relativePath}` |
| `shell.openInEditor` | `{cwd, editor: EditorId, reveal?}` | `null`. `editor` must be one of `ServerConfig.availableEditors`. |

## 9. Recommendations for the Rust layer

### 9.1 Module layout

```text
t3_proto/            # pure data, no IO. serde types generated/hand-written from Appendix A
  rpc.rs             # FromClient / FromServer envelopes, Exit, CauseReason, RequestId
  schema.rs          # helpers: lenient numbers, forward-compatible Vec, OptionalNullable
  orchestration.rs   # read model, OrchestrationEvent, stream items, commands
  server.rs          # ServerConfig, providers, settings, keybindings, lifecycle, auth access
  terminal.rs vcs.rs projects.rs assets.rs errors.rs
  methods.rs         # one zero-sized type per RPC: const TAG, type Payload, type Success, type Error, const STREAM
t3_client/           # IO
  auth.rs            # pairing exchange, token store, ws ticket, descriptor + protocol check (reqwest)
  socket.rs          # tokio-tungstenite task: frame codec, id allocator, pending map, ack, ping, defect fan-out
  rpc.rs             # call::<M>(payload) -> Result<M::Success, RpcError<M::Error>>; subscribe::<M>(payload) -> Stream
  supervisor.rs      # connect/backoff (3s,4s,8s,16s), resubscribe on reconnect, connection state for the UI
t3_state/            # reducers, no IO
  shell.rs thread.rs server_config.rs terminal.rs   # apply(item) -> state, sequence tracking, resume cursors
```

Keep `t3_state` free of IO so the reducers can be driven by recorded frame logs in end-to-end tests.

### 9.2 Socket task

- One task owns the WebSocket. It holds `HashMap<String, Pending>` where `Pending` is either `Unary(oneshot::Sender<Exit>)` or `Stream(mpsc::Sender<StreamMsg>)`.
- Outgoing: serialize one message per text frame. Allocate ids from an `AtomicU64`, send as strings.
- Incoming `Chunk`: push each value into the stream's channel, then send `Ack` with the identical id string. With a bounded channel, the `send().await` before the Ack is your backpressure; keep the bound modest (16 to 64) and never block the socket task on UI work.
- Incoming `Exit`: remove the entry; complete the oneshot or close the stream channel with the exit.
- Incoming `Defect`: fail every pending entry (1.7), then close the socket so the supervisor reconnects. Streams that survived on the server would otherwise stay allocated and un-acked (their entries are gone, so nobody can Interrupt them) while followers resubscribe on top.
- Incoming `Pong`: clear the ping flag.
- Ping timer: every 5 s, if a Pong arrived since the last Ping send a new one; after 3 consecutive misses, drop the connection and reconnect.
- Dropping a stream handle sends `Interrupt` (best effort).
- On socket close: fail all pending with a transport error; the supervisor reconnects and every live subscription resubscribes with its resume cursor (5.3).

### 9.3 serde strategy

- Envelopes: `#[serde(tag = "_tag")] enum FromServer { Chunk{requestId, values: Vec<Box<RawValue>>}, Exit{requestId, exit}, Defect{defect: Value}, Pong, #[serde(other)] Unknown }`. Decode `values` lazily (`RawValue`) and decode per method, so one bad item does not take down the socket task.
- `requestId: RequestId` with a custom deserializer accepting string or number, stored as `String`.
- Unions: `#[serde(tag = "type")]` (events, commands, config events, terminal events), `tag = "kind"` (stream items, shell events), `tag = "_tag"` (errors, VCS status, causes). Every server-to-client union gets an `Unknown` fallback. Serde's `#[serde(other)]` only works on unit variants, so either add `#[serde(other)] Unknown` and accept losing the payload, or deserialize to `Value` first and dispatch on the tag string yourself. For `OrchestrationEvent` keep the raw `Value` for unknown types so the envelope `sequence` is still usable.
- Structs: never `deny_unknown_fields`. Optional keys: `#[serde(default, skip_serializing_if = "Option::is_none")]`, and accept `null` on read (serde's `Option` already does).
- Nullable keys (`NullOr`): plain `Option<T>` without `skip_serializing_if`, so `null` is written.
- Forward-compatible arrays (`providers`, `keybindings`, ...): deserialize as `Vec<Value>`, then try each element and drop failures.
- String enums that grow (session status, provider status, activity tone, attachment type, keybinding command, editor id): `enum X { Known..., #[serde(untagged)] Other(String) }` (serde >= 1.0.181 supports an untagged catch-all variant) or keep as `String` with consts.
- Numbers: accept `"NaN"`/`"Infinity"`/`"-Infinity"` strings on `f64` fields that come from `Schema.Number` if you want to be fully faithful. Counts and sequences are integers in practice.
- `OrchestrationThreadActivity.payload` and `ProviderUserInputAnswers` values stay `serde_json::Value`; interpret by `kind` in the work-log layer (5.5).
- `ModelSelection.options`: deserialize both the array form and the legacy object form; always serialize the array form.

### 9.4 Golden sequence

A minimal session against a local `npx t3@nightly` on port 3773 with a bearer token. Ids are illustrative; values elided with `...`.

```text
# HTTP
GET  http://127.0.0.1:3773/.well-known/t3/environment
  <- 200 {"environmentId":"env_1","label":"repo","platform":{"os":"linux","arch":"x64"},"serverVersion":"0.0.45-nightly...","orchestrationProtocolVersion":1,"capabilities":{...}}
POST http://127.0.0.1:3773/api/auth/websocket-ticket      Authorization: Bearer <token>
  <- 200 {"ticket":"eyJ2Ijox...","expiresAt":"2026-10-01T12:05:00.000Z"}
GET  http://127.0.0.1:3773/api/orchestration/shell         Authorization: Bearer <token>     (optional fast path)
  <- 200 {"snapshotSequence":48200,"projects":[...],"threads":[...],"updatedAt":"..."}

# WebSocket  ws://127.0.0.1:3773/ws?wsTicket=eyJ2Ijox...&clientSurface=desktop&clientAppVersion=0.1.0&clientDeviceType=desktop&clientOs=Linux&connectionMethod=direct&orchestrationProtocol=1
C->S {"_tag":"Request","id":"0","tag":"subscribeServerConfig","payload":{},"headers":[]}
S->C {"_tag":"Chunk","requestId":"0","values":[{"version":1,"type":"snapshot","config":{"environment":{...},"auth":{...},"cwd":"...","providers":[...],"settings":{...},"shellResumeCompletionMarker":true,"threadResumeCompletionMarker":true,"threadSnapshotPagination":true,"reasoningMessages":true,...}}]}
C->S {"_tag":"Ack","requestId":"0"}
C->S {"_tag":"Request","id":"1","tag":"server.probe","payload":{},"headers":[]}
S->C {"_tag":"Exit","requestId":"1","exit":{"_tag":"Success","value":{}}}

# shell: resume from the HTTP snapshot (or send {"requestCompletionMarker":true} alone to get a socket snapshot)
C->S {"_tag":"Request","id":"2","tag":"orchestration.subscribeShell","payload":{"afterSequence":48200,"requestCompletionMarker":true},"headers":[]}
S->C {"_tag":"Chunk","requestId":"2","values":[{"kind":"thread-upserted","sequence":48203,"thread":{...}}]}
C->S {"_tag":"Ack","requestId":"2"}
S->C {"_tag":"Chunk","requestId":"2","values":[{"kind":"synchronized"}]}
C->S {"_tag":"Ack","requestId":"2"}

# open a thread
C->S {"_tag":"Request","id":"3","tag":"orchestration.subscribeThread","payload":{"threadId":"t_9a7e","requestCompletionMarker":true,"reasoningMessages":true,"turnLimit":10},"headers":[]}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"snapshot","snapshot":{"snapshotSequence":48203,"thread":{"id":"t_9a7e","messages":[...],"activities":[...],"checkpoints":[...],"session":{...},...},"page":{"beforeCursor":"...","hasMore":true,"snapshotSequence":48203}}}]}
C->S {"_tag":"Ack","requestId":"3"}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"synchronized"}]}
C->S {"_tag":"Ack","requestId":"3"}

# start a turn
C->S {"_tag":"Request","id":"4","tag":"orchestration.dispatchCommand","payload":{"type":"thread.turn.start","commandId":"c_1","threadId":"t_9a7e","message":{"messageId":"m_1","role":"user","text":"hi","attachments":[]},"runtimeMode":"full-access","interactionMode":"default","createdAt":"2026-10-01T12:00:00.000Z"},"headers":[]}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"event","event":{"sequence":48204,"eventId":"e_1","aggregateKind":"thread","aggregateId":"t_9a7e","occurredAt":"...","commandId":"c_1","causationEventId":null,"correlationId":"c_1","metadata":{},"type":"thread.message-sent","payload":{"threadId":"t_9a7e","messageId":"m_1","role":"user","text":"hi","turnId":null,"streaming":false,"createdAt":"...","updatedAt":"..."}}}]}
C->S {"_tag":"Ack","requestId":"3"}
S->C {"_tag":"Exit","requestId":"4","exit":{"_tag":"Success","value":{"sequence":48205}}}
      # 48205 is thread.turn-start-requested. It is NOT sent on the thread stream (5.3); the shell sees it:
S->C {"_tag":"Chunk","requestId":"2","values":[{"kind":"thread-upserted","sequence":48205,"thread":{...,"session":{"status":"starting",...}}}]}
C->S {"_tag":"Ack","requestId":"2"}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"event","event":{...,"type":"thread.session-set","payload":{"threadId":"t_9a7e","session":{"status":"running","activeTurnId":"turn_1",...}}}}]}
C->S {"_tag":"Ack","requestId":"3"}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"event","event":{...,"type":"thread.message-sent","payload":{"messageId":"assistant:...","role":"assistant","text":"Hel","turnId":"turn_1","streaming":true,...}}}]}
C->S {"_tag":"Ack","requestId":"3"}
S->C {"_tag":"Chunk","requestId":"3","values":[{"kind":"event","event":{...,"type":"thread.message-sent","payload":{"messageId":"assistant:...","role":"assistant","text":"lo","turnId":"turn_1","streaming":true,...}}}]}
C->S {"_tag":"Ack","requestId":"3"}      # text is now "Hello" (append)
...                                    # activity-appended (tool.*), message-sent {"text":"","streaming":false}, turn-diff-completed, session-set ready
S->C {"_tag":"Chunk","requestId":"2","values":[{"kind":"thread-upserted","sequence":48230,"thread":{...,"latestTurn":{"state":"completed",...}}}]}
C->S {"_tag":"Ack","requestId":"2"}

# every 5 s
C->S {"_tag":"Ping"}
S->C {"_tag":"Pong"}

# leaving the thread view
C->S {"_tag":"Interrupt","requestId":"3"}
S->C {"_tag":"Exit","requestId":"3","exit":{"_tag":"Failure","cause":[{"_tag":"Interrupt","fiberId":123}]}}
```

The orchestration frames are assembled from the schemas and handlers, not captured from a live server (the user's live server was out of bounds for this research). The RPC framing around them is verified (1.9).

## 10. Open questions and risks

- Nightly drift. `U` is pinned at `b33eda13` (v0.0.44); nightly moves daily. Re-run Appendix B against a fresh checkout before relying on a shape, and diff the output. The strict unions (`OrchestrationEvent`, `ServerConfigStreamEvent`, the command union) are where breakage shows up first.
- Not captured live. Section 1.9 runs the real rc.115 RPC server code in-process, but no frames were captured from a running t3 server. Worth doing once in CI against a throwaway `npx t3@nightly` with a temp `--base-dir`: record a session and keep it as a golden file for the reducers.
- Handler defects kill everything in flight on the connection (1.7). A buggy server method can repeatedly reset all subscriptions. Rate-limit resubscribes.
- `Defect` frames carry no request id, so you cannot tell which request died.
- Effect version skew. Fork (beta.78) and upstream (rc.115) differ in id types, schema-issue message formatting, and ping tolerance. String decimal ids work against both. The fork server also lacks most upstream methods, so do not target it.
- `effect` RPC is "unstable" API in effect 4. Upstream patches it (`patches/effect@4.0.0-rc.115.patch`); a future bump could change framing details such as Ack semantics.
- Ticket vs header auth. The `Authorization: Bearer` header on the upgrade works per source but upstream clients never use it, so it is less tested. The ticket path is the safe default.
- Scopes. A standard-scope bearer cannot use `subscribeAuthAccess` or the admin HTTP endpoints. Decide whether the native client should request admin scopes.
- Activity payloads are untyped (`payload: unknown`). The work-log rendering depends on per-`kind` conventions that live in the server's ingestion code and client-runtime presentation helpers, not in schemas (5.5). Expect churn.
- `DispatchResult.sequence` vs stream delivery. Events for a command can arrive before or after the dispatch `Exit` (the golden sequence shows both). Do not assume ordering between the reply and the subscription chunks.

## Appendix A. Wire types (generated)

Generated from upstream `packages/contracts/src` at `b33eda13` with effect `4.0.0-rc.115` (Appendix B). Notation as in section 0:

- `k?: T | null`: optional key that also accepts `null` (`Schema.optional`). `k?: T`: optional key, never `null` (`Schema.optionalKey`, or a forward-compatible field). `k: T | null`: required, nullable.
- `number` fields also accept the strings `"NaN"`, `"Infinity"`, `"-Infinity"` (section 2); omitted below for readability.
- `unknown` is arbitrary JSON (defects, activity payloads, user-input answers).
- Types are listed in first-use order starting from the section 4.1 methods. `OrchestrationEvent` is rewritten as envelope plus per-type payload; on the wire every event is one flat object containing the envelope fields plus `type` and `payload`.

```ts
type EnvironmentAuthorizationError = {
  _tag: "EnvironmentAuthorizationError"
  message: string
  requiredScope: "orchestration:read" | "orchestration:operate" | "terminal:operate" | "review:write" | "access:read" | "access:write" | "relay:read" | "relay:write"
}

type ServerConfig = {
  environment: ExecutionEnvironmentDescriptor
  auth: ServerAuthDescriptor
  cwd: string
  keybindingsConfigPath: string
  keybindings: Array<ResolvedKeybindingRule>
  issues: Array<
    | { kind: "keybindings.malformed-config"; message: string }
    | { kind: "keybindings.invalid-entry"; message: string; index: number }>
  providers: Array<ServerProvider>
  availableEditors: Array<EditorId>
  remoteOpenTargets?: Array<RemoteOpenTarget>
  observability: ServerObservability
  settings: ServerSettings
  shellResumeCompletionMarker?: boolean
  shellRevealInFileManager?: boolean
  shellRevealInFileManagerKind?: "finder" | "file-explorer" | "files"
  threadResumeCompletionMarker?: boolean
  threadSnapshotPagination?: boolean
  reasoningMessages?: boolean
  scratchWorkspaceRoot?: string
  newProjectsRoot?: string
  environmentThemes?: Array<EnvironmentTheme> | null
  usageLimitSources?: Array<UsageLimitSourceSnapshot> | null
}

type KeybindingsConfigError = { _tag: "KeybindingsConfigParseError"; configPath: string; detail: string; cause?: unknown | null }

type ServerSettingsError = {
  _tag: "ServerSettingsError"
  settingsPath: string
  operation: ServerSettingsOperation
  providerInstanceId?: string | null
  environmentVariable?: string | null
  cause: unknown
}

type ServerProviderUpdatedPayload = { providers: Array<ServerProvider> }

type ProviderSetupError = {
  _tag: "ProviderSetupError"
  instanceId: string
  operation: string
  detail: string
  cause?: unknown | null
}

type ServerProviderUpdateInput = { provider: string; targetVersion?: string; instanceId?: string }

type ServerProviderUpdateError = { _tag: "ServerProviderUpdateError"; provider: string; reason: string; cause?: unknown | null }

type ServerUpsertKeybindingInput = {
  key: string
  command: 
    | "sidebar.toggle" | "navigation.back" | "navigation.forward" | "terminal.toggle" | "terminal.split" | "terminal.splitVertical" | "terminal.new" | "terminal.close" | "rightPanel.toggle" | "rightPanel.toggleMaximized" | "rightPanel.close" | "pullRequest.copyNumber" | "diff.toggle" | "preview.toggle" | "preview.refresh" | "preview.focusUrl" | "preview.zoomIn" | "preview.zoomOut" | "preview.resetZoom" | "commandPalette.toggle" | "filePicker.toggle" | "projectSearch.toggle" | "usage.open" | "theme.select" | "appearance.cycle" | "themeEditor.toggle" | "composer.stash" | "composer.host" | "composer.effort" | "composer.mode" | "composer.workspace" | "composer.previousWorktree" | "composer.branch" | "chat.new" | "chat.newLocal" | "chat.newWithoutProject" | "editor.openFavorite" | "usage.cost" | "usage.tokens" | "usage.limits" | "usage.period.day" | "usage.period.week" | "usage.period.month" | "usage.period.quarter" | "modelPicker.toggle" | "modelPicker.previousProvider" | "modelPicker.nextProvider" | "modelPicker.jump.1" | "modelPicker.jump.2" | "modelPicker.jump.3" | "modelPicker.jump.4" | "modelPicker.jump.5" | "modelPicker.jump.6" | "modelPicker.jump.7" | "modelPicker.jump.8" | "modelPicker.jump.9" | "thread.stop" | "thread.steerQueuedMessage" | "thread.previous" | "thread.next" | "thread.copyReference" | "thread.settle" | "thread.pin" | "thread.undo" | "thread.jump.1" | "thread.jump.2" | "thread.jump.3" | "thread.jump.4" | "thread.jump.5" | "thread.jump.6" | "thread.jump.7" | "thread.jump.8" | "thread.jump.9"
    | `script.${string}.run`
  when?: string | null
  replace?: ServerRemoveKeybindingInput | null
}

type ServerRemoveKeybindingResult = {
  keybindings: Array<ResolvedKeybindingRule>
  issues: Array<
    | { kind: "keybindings.malformed-config"; message: string }
    | { kind: "keybindings.invalid-entry"; message: string; index: number }>
}

type ServerRemoveKeybindingInput = {
  key: string
  command: 
    | "sidebar.toggle" | "navigation.back" | "navigation.forward" | "terminal.toggle" | "terminal.split" | "terminal.splitVertical" | "terminal.new" | "terminal.close" | "rightPanel.toggle" | "rightPanel.toggleMaximized" | "rightPanel.close" | "pullRequest.copyNumber" | "diff.toggle" | "preview.toggle" | "preview.refresh" | "preview.focusUrl" | "preview.zoomIn" | "preview.zoomOut" | "preview.resetZoom" | "commandPalette.toggle" | "filePicker.toggle" | "projectSearch.toggle" | "usage.open" | "theme.select" | "appearance.cycle" | "themeEditor.toggle" | "composer.stash" | "composer.host" | "composer.effort" | "composer.mode" | "composer.workspace" | "composer.previousWorktree" | "composer.branch" | "chat.new" | "chat.newLocal" | "chat.newWithoutProject" | "editor.openFavorite" | "usage.cost" | "usage.tokens" | "usage.limits" | "usage.period.day" | "usage.period.week" | "usage.period.month" | "usage.period.quarter" | "modelPicker.toggle" | "modelPicker.previousProvider" | "modelPicker.nextProvider" | "modelPicker.jump.1" | "modelPicker.jump.2" | "modelPicker.jump.3" | "modelPicker.jump.4" | "modelPicker.jump.5" | "modelPicker.jump.6" | "modelPicker.jump.7" | "modelPicker.jump.8" | "modelPicker.jump.9" | "thread.stop" | "thread.steerQueuedMessage" | "thread.previous" | "thread.next" | "thread.copyReference" | "thread.settle" | "thread.pin" | "thread.undo" | "thread.jump.1" | "thread.jump.2" | "thread.jump.3" | "thread.jump.4" | "thread.jump.5" | "thread.jump.6" | "thread.jump.7" | "thread.jump.8" | "thread.jump.9"
    | `script.${string}.run`
  when?: string | null
}

type ServerSettings = {
  worktreeCleanup: { mode: "off" } | { mode: "custom"; rules: WorktreeCleanupRules } | null
  storageCleanup: 
    | {
      worktreeAfterDays?: number | null
      worktreeOnMerge?: boolean | null
      worktreeOnDelete?: boolean | null
      worktreeUnchanged?: boolean | null
      browserArtifactsAfterDays?: number | null
      logsAfterDays?: number | null
    }
    | null
  responseStreamingMode: "turn" | "paragraph" | "token" | null
  enableProviderUpdateChecks: boolean | null
  continueThreadsAfterServerUpdate: boolean | null
  enableAgentBrowserAccess: boolean | null
  projectAgentBrowserAccessOverrides: { [k: string]: boolean } | null
  defaultAutoPull: boolean | null
  defaultProjectScripts: Array<ProjectScript> | null
  projectScriptOverrides: { [k: string]: Array<ProjectScript> | null } | null
  projectAutoPullOverrides: { [k: string]: boolean } | null
  defaultModelSelection: ModelSelection | null
  defaultRuntimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access" | null
  projectSettingsOverrides: 
    | {
      [k: string]: {
        worktreeCleanup?: { mode: "off" } | { mode: "custom"; rules: WorktreeCleanupRules } | null
        defaultModelSelection?: 
          | ModelSelection
          | null
        defaultRuntimeMode?: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
        defaultThreadEnvMode?: "local" | "worktree"
        newWorktreesStartFromOrigin?: boolean
        worktreeSubmodules?: unknown
        defaultAutoPull?: boolean
        defaultProjectScripts?: Array<ProjectScript>
        enableAgentBrowserAccess?: boolean
        enableAgentDeviceAccess?: boolean
        textGenerationModelSelection?: ModelSelection
        sourceControlWriterModelSelection?: 
          | ModelSelection
          | null
        sourceControlWritingStyle?: {
          mode?: "repo_conventions" | "conventional_commits" | "custom" | null
          customInstructions?: string | null
          followChangeRequestTemplates?: boolean | null
        }
        pullRequestMergeMethod?: "merge" | "squash" | "rebase" | null
        sidebarAutoSettleOnMerge?: boolean
        sidebarAutoSettleAfterDays?: number | null
        continueThreadsAfterServerUpdate?: boolean
        responseStreamingMode?: "turn" | "paragraph" | "token"
      }
    }
    | null
  projectSettingsFolded: boolean | null
  enableAgentDeviceAccess: boolean | null
  enableDeviceSupport: boolean | null
  deviceOnboardingCompleted: boolean | null
  deviceHosts: Array<SshDeviceHostConfig> | null
  sidebarAutoSettleAfterDays: number | null
  sidebarAutoSettleOnMerge: boolean | null
  backgroundActivity: BackgroundActivitySettings
  automaticGitFetchInterval: number | null
  providerHealthRefreshInterval: number | null
  backgroundActivityProfile: "balanced" | "performance" | "battery-saver" | null
  defaultTheme: string | null
  defaultThemeSetAt: string | null
  environmentIcon: "server" | "cloud" | "linux" | "desktop" | "laptop" | "mac-mini" | "mac-studio" | null
  defaultThreadEnvMode: "local" | "worktree" | null
  newWorktreesStartFromOrigin: boolean | null
  worktreeSubmodules: "recursive" | "top-level" | "none" | null
  addProjectBaseDirectory: string | null
  textGenerationModelSelection: ModelSelection
  sourceControlWritingStyle: 
    | {
      mode?: "repo_conventions" | "conventional_commits" | "custom" | null
      customInstructions?: string | null
      followChangeRequestTemplates?: boolean | null
    }
    | null
  sourceControlWriterModelSelection: ModelSelection | null
  pullRequestMergeMethod: "merge" | "squash" | "rebase" | null
  providers: 
    | {
      codex?: 
        | {
          setupMode?: "managed" | "existing"
          enabled?: boolean | null
          binaryPath?: string | null
          homePath?: string | null
          shadowHomePath?: string | null
          launchArgs?: string | null
          customModels?: Array<CustomModelSetting> | null
        }
        | null
      claudeAgent?: 
        | {
          enabled?: boolean | null
          binaryPath?: string | null
          homePath?: string | null
          customModels?: Array<CustomModelSetting> | null
          launchArgs?: string | null
          autoCompactWindow?: string | null
        }
        | null
      cursor?: 
        | {
          enabled?: boolean | null
          binaryPath?: string | null
          apiEndpoint?: string | null
          customModels?: Array<CustomModelSetting> | null
        }
        | null
      grok?: 
        | {
          enabled?: boolean | null
          binaryPath?: string | null
          customModels?: Array<CustomModelSetting> | null
        }
        | null
      opencode?: 
        | {
          enabled?: boolean | null
          binaryPath?: string | null
          serverUrl?: string | null
          serverPassword?: string | null
          customModels?: Array<CustomModelSetting> | null
        }
        | null
      antigravity?: 
        | {
          enabled?: boolean | null
          authMethod?: "oauth-personal" | "oauth-business" | "gemini-api-key" | "agent-platform" | null
          apiKey?: string | null
          gcpProject?: string | null
          gcpLocation?: string | null
          binaryPath?: string | null
          customModels?: Array<CustomModelSetting> | null
        }
        | null
    }
    | null
  providerInstances: 
    | {
      [k: string]: {
        driver: string
        displayName?: string | null
        accentColor?: string | null
        environment?: Array<{ name: string; value?: string | null; sensitive?: boolean | null; valueRedacted?: boolean }>
        enabled?: boolean
        config?: unknown
      }
    }
    | null
  observability: { otlpTracesUrl?: string | null; otlpMetricsUrl?: string | null; otlpLogsUrl?: string | null } | null
  bitbucket: { email?: string | null; accessToken?: string | null; apiToken?: string | null } | null
  usageLimitSources: 
    | {
      [k: string]: {
        kind: "cliproxy"
        label?: string | null
        url: string
        managementKey?: string | null
        enabled?: boolean | null
      }
    }
    | null
  cursorKeychainUsageEnabled: boolean | null
  usagePriceOverrides: { [k: string]: UsageModelPriceOverride } | null
}

type ServerSettingsPatch = {
  worktreeCleanup?: 
    | 
      | { mode: "off" }
      | {
        mode: "custom"
        rules: {
          worktreeAfterDays?: number | null
          worktreeOnMerge?: boolean
          worktreeOnDelete?: boolean
          worktreeUnchanged?: boolean
        }
      }
    | null
  storageCleanup?: {
    worktreeAfterDays?: number | null
    worktreeOnMerge?: boolean
    worktreeOnDelete?: boolean
    worktreeUnchanged?: boolean
    browserArtifactsAfterDays?: number | null
    logsAfterDays?: number | null
  }
  responseStreamingMode?: "turn" | "paragraph" | "token"
  enableProviderUpdateChecks?: boolean
  continueThreadsAfterServerUpdate?: boolean
  enableAgentBrowserAccess?: boolean
  projectAgentBrowserAccessOverrides?: { [k: string]: boolean | null }
  defaultAutoPull?: boolean
  defaultProjectScripts?: Array<ProjectScript>
  projectScriptOverrides?: { [k: string]: Array<ProjectScript> | null }
  projectAutoPullOverrides?: { [k: string]: boolean | null }
  defaultModelSelection?: ModelSelection | null
  defaultRuntimeMode?: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
  projectSettingsOverrides?: { [k: string]: ProjectSettingsOverrides | null }
  enableAgentDeviceAccess?: boolean
  enableDeviceSupport?: boolean
  deviceOnboardingCompleted?: boolean
  deviceHosts?: Array<SshDeviceHostConfig>
  sidebarAutoSettleAfterDays?: number | null
  sidebarAutoSettleOnMerge?: boolean
  backgroundActivity?: {
    schemaVersion?: 1
    profile?: "balanced" | "performance" | "battery-saver" | "custom"
    baseProfile?: "balanced" | "performance" | "battery-saver"
    overrides?: BackgroundActivityOverrides
  }
  automaticGitFetchInterval?: number
  providerHealthRefreshInterval?: number
  backgroundActivityProfile?: "balanced" | "performance" | "battery-saver"
  environmentIcon?: "server" | "cloud" | "linux" | "desktop" | "laptop" | "mac-mini" | "mac-studio" | null
  defaultThreadEnvMode?: "local" | "worktree" | null
  newWorktreesStartFromOrigin?: boolean
  worktreeSubmodules?: "recursive" | "top-level" | "none" | null
  addProjectBaseDirectory?: string
  textGenerationModelSelection?: { instanceId?: string; model?: string; options?: ProviderOptionSelections }
  sourceControlWritingStyle?: {
    mode?: "repo_conventions" | "conventional_commits" | "custom"
    customInstructions?: string
    followChangeRequestTemplates?: boolean
  }
  sourceControlWriterModelSelection?: ModelSelection | null
  pullRequestMergeMethod?: "merge" | "squash" | "rebase" | null
  observability?: { otlpTracesUrl?: string; otlpMetricsUrl?: string; otlpLogsUrl?: string }
  bitbucket?: { email?: string; accessToken?: string; apiToken?: string }
  providers?: {
    codex?: {
      enabled?: boolean
      binaryPath?: string
      homePath?: string
      shadowHomePath?: string
      launchArgs?: string
      customModels?: Array<CustomModelSetting>
    }
    claudeAgent?: {
      enabled?: boolean
      binaryPath?: string
      homePath?: string
      customModels?: Array<CustomModelSetting>
      launchArgs?: string
      autoCompactWindow?: string
    }
    cursor?: {
      enabled?: boolean
      binaryPath?: string
      apiEndpoint?: string
      customModels?: Array<CustomModelSetting>
    }
    grok?: { enabled?: boolean; binaryPath?: string; customModels?: Array<CustomModelSetting> }
    opencode?: {
      enabled?: boolean
      binaryPath?: string
      serverUrl?: string
      serverPassword?: string
      customModels?: Array<CustomModelSetting>
    }
    antigravity?: {
      enabled?: boolean
      authMethod?: "oauth-personal" | "oauth-business" | "gemini-api-key" | "agent-platform"
      apiKey?: string
      gcpProject?: string
      gcpLocation?: string
      binaryPath?: string
      customModels?: Array<CustomModelSetting>
    }
  }
  providerInstances?: { [k: string]: ProviderInstanceConfig }
  usageLimitSources?: { [k: string]: UsageLimitSourceConfig | null }
  cursorKeychainUsageEnabled?: boolean
  usagePriceOverrides?: { [k: string]: UsageModelPriceOverride | null }
}

type ProjectListEntriesInput = { cwd: string; directoryPath?: string | null }

type ProjectListEntriesResult = { entries: Array<ProjectEntry>; truncated: boolean }

type ProjectListEntriesError = {
  _tag: "ProjectListEntriesError"
  cwd?: string | null
  failure?: 
    | "workspace_root_not_found" | "workspace_root_create_failed" | "workspace_root_stat_failed" | "workspace_root_not_directory" | "search_index_create_failed" | "search_index_scan_timed_out" | "search_index_search_failed" | "directory_list_failed"
    | null
  normalizedCwd?: string | null
  timeout?: string | null
  detail?: string | null
  message: string
  cause?: unknown | null
}

type ProjectReadFileInput = { cwd: string; relativePath: string }

type ProjectReadFileResult = { relativePath: string; contents: string; byteLength: number; truncated: boolean }

type ProjectReadFileError = {
  _tag: "ProjectReadFileError"
  cwd?: string | null
  relativePath?: string | null
  failure?: 
    | "workspace_path_outside_root" | "resolved_path_outside_root" | "path_not_file" | "binary_file" | "operation_failed"
    | null
  resolvedPath?: string | null
  resolvedWorkspaceRoot?: string | null
  operation?: 
    | "realpath-workspace-root" | "realpath-target" | "open" | "stat" | "read" | "close" | "make-directory" | "write-file"
    | null
  operationPath?: string | null
  message: string
  cause?: unknown | null
}

type ProjectSearchContentsInput = {
  cwd: string
  query: string
  limit: number
  caseSensitive: boolean
  wholeWord: boolean
  useRegex: boolean
}

type ProjectSearchContentsResult = { matches: Array<ProjectContentMatch>; truncated: boolean; regexFallbackError?: string | null }

type ProjectSearchContentsError = {
  _tag: "ProjectSearchContentsError"
  cwd?: string | null
  queryLength?: number | null
  limit?: number | null
  failure?: 
    | "workspace_root_not_found" | "workspace_root_create_failed" | "workspace_root_stat_failed" | "workspace_root_not_directory" | "search_index_create_failed" | "search_index_scan_timed_out" | "search_index_search_failed" | "directory_list_failed"
    | null
  normalizedCwd?: string | null
  timeout?: string | null
  detail?: string | null
  message: string
  cause?: unknown | null
}

type ProjectSearchEntriesInput = {
  cwd: string
  query: string
  limit: number
  kind?: "file" | "directory" | null
  imageOnly?: boolean | null
}

type ProjectSearchEntriesResult = { entries: Array<ProjectEntry>; truncated: boolean }

type ProjectSearchEntriesError = {
  _tag: "ProjectSearchEntriesError"
  cwd?: string | null
  queryLength?: number | null
  limit?: number | null
  failure?: 
    | "workspace_root_not_found" | "workspace_root_create_failed" | "workspace_root_stat_failed" | "workspace_root_not_directory" | "search_index_create_failed" | "search_index_scan_timed_out" | "search_index_search_failed" | "directory_list_failed"
    | null
  normalizedCwd?: string | null
  timeout?: string | null
  detail?: string | null
  message: string
  cause?: unknown | null
}

type ProjectWriteFileInput = { cwd: string; relativePath: string; contents: string }

type ProjectWriteFileResult = { relativePath: string }

type ProjectWriteFileError = {
  _tag: "ProjectWriteFileError"
  cwd?: string | null
  relativePath?: string | null
  failure?: 
    | "workspace_path_outside_root" | "resolved_path_outside_root" | "path_not_file" | "binary_file" | "operation_failed"
    | null
  resolvedPath?: string | null
  resolvedWorkspaceRoot?: string | null
  operation?: 
    | "realpath-workspace-root" | "realpath-target" | "open" | "stat" | "read" | "close" | "make-directory" | "write-file"
    | null
  operationPath?: string | null
  message: string
  cause?: unknown | null
}

type LaunchEditorInput = { cwd: string; editor: EditorId; reveal?: boolean | null }

type ExternalLauncherError = 
  | ExternalLauncherUnknownEditorError
  | ExternalLauncherUnsupportedEditorError
  | ExternalLauncherCommandNotFoundError
  | ExternalLauncherBrowserSpawnError
  | ExternalLauncherEditorSpawnError

type FilesystemBrowseInput = { partialPath: string; cwd?: string | null }

type FilesystemBrowseResult = { parentPath: string; entries: Array<FilesystemBrowseEntry> }

type FilesystemBrowseError = {
  _tag: "FilesystemBrowseError"
  partialPath?: string | null
  cwd?: string | null
  failure?: "windows_path_unsupported" | "current_project_required" | "read_directory_failed" | null
  parentPath?: string | null
  platform?: string | null
  message: string
  cause?: unknown | null
}

type AssetCreateUrlInput = { resource: AssetResource }

type AssetCreateUrlResult = {
  relativeUrl: string
  expiresAt: number
  sourcePath?: string | null
  imageDimensions?: AssetImageDimensions | null
}

type AssetAccessError = 
  | AssetWorkspaceContextNotFoundError
  | AssetWorkspaceContextResolutionError
  | AssetWorkspaceRootNormalizationError
  | AssetWorkspacePathValidationError
  | AssetPreviewTypeValidationError
  | AssetWorkspaceAssetInspectionError
  | AssetWorkspaceAssetNotFoundError
  | AssetWorkspaceResolutionError
  | AssetAttachmentNotFoundError
  | AssetProjectFaviconResolutionError
  | AssetProjectFaviconInspectionError
  | AssetProjectFaviconNotFoundError
  | AssetGitHubMediaUrlValidationError
  | AssetSigningKeyLoadError

type AttachmentCreateUploadUrlInput = 
  | {
    type?: "image"
    name: string
    mimeType: "image/gif" | "image/jpeg" | "image/png" | "image/webp"
    sizeBytes: number
  }
  | { type: "file"; name: string; mimeType: string; sizeBytes: number }

type AttachmentCreateUploadUrlResult = { attachmentId: string; relativeUrl: string; expiresAt: number }

type AttachmentUploadSigningKeyError = { _tag: "AttachmentUploadSigningKeyError"; cause: unknown }

type AttachmentDeleteInput = { attachmentId: string }

type VcsStatusInput = { cwd: string }

type VcsStatusStreamEvent = 
  | { _tag: "snapshot"; local: VcsStatusLocalResult; remote: VcsStatusRemoteResult | null }
  | { _tag: "localUpdated"; local: VcsStatusLocalResult }
  | { _tag: "remoteUpdated"; remote: VcsStatusRemoteResult | null }

type GitManagerServiceError = 
  | GitManagerError
  | GitPullRequestMaterializationError
  | GitCommandError
  | SourceControlProviderError
  | TextGenerationError

type VcsPullInput = { cwd: string }

type VcsPullResult = { status: "pulled" | "skipped_up_to_date"; refName: string; upstreamRef: string | null }

type GitCommandError = {
  _tag: "GitCommandError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  exitCode?: number | null
  stdoutLength?: number | null
  stderrLength?: number | null
  outputLength?: number | null
  detail: string
  cause?: unknown | null
}

type VcsStatusResult = {
  isRepo: boolean
  sourceControlProvider?: SourceControlProviderInfo | null
  hasPrimaryRemote: boolean
  isDefaultRef: boolean
  refName: string | null
  hasWorkingTreeChanges: boolean
  workingTree: {
    files: Array<{ path: string; insertions: number; deletions: number }>
    insertions: number
    deletions: number
  }
  hasUpstream: boolean
  aheadCount: number
  behindCount: number
  aheadOfDefaultCount?: number | null
  pr: 
    | {
      number: number
      title: string
      url: string
      baseRef: string
      headRef: string
      state: "open" | "closed" | "merged"
      isDraft?: boolean | null
      updatedAt?: string | null
    }
    | null
}

type GitRunStackedActionInput = {
  actionId: string
  cwd: string
  action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
  commitMessage?: string | null
  featureBranch?: boolean | null
  filePaths?: Array<string> | null
  threadId?: string | null
}

type GitActionProgressEvent = 
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "action_started"
    phases: Array<"branch" | "commit" | "push" | "pr">
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "phase_started"
    phase: "branch" | "commit" | "push" | "pr"
    label: string
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "hook_started"
    hookName: string
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "hook_output"
    hookName: string | null
    stream: "stdout" | "stderr"
    text: string
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "hook_finished"
    hookName: string
    exitCode: number | null
    durationMs: number | null
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "action_finished"
    result: GitRunStackedActionResult
  }
  | {
    actionId: string
    cwd: string
    action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
    kind: "action_failed"
    phase: "branch" | "commit" | "push" | "pr" | null
    message: string
  }

type GitPullRequestRefInput = { cwd: string; reference: string }

type GitResolvePullRequestResult = {
  pullRequest: {
    number: number
    title: string
    url: string
    baseBranch: string
    headBranch: string
    state: "open" | "closed" | "merged"
  }
}

type GitPreparePullRequestThreadInput = { cwd: string; reference: string; mode: "local" | "worktree"; threadId?: string | null }

type GitPreparePullRequestThreadResult = {
  pullRequest: {
    number: number
    title: string
    url: string
    baseBranch: string
    headBranch: string
    state: "open" | "closed" | "merged"
  }
  branch: string
  worktreePath: string | null
  isOnPullRequestHead: boolean
}

type VcsListRefsInput = {
  cwd: string
  query?: string | null
  cursor?: number | null
  includeMatchingRemoteRefs?: boolean | null
  refKind?: "all" | "local" | "remote" | null
  refresh?: boolean | null
  limit?: number | null
}

type VcsListRefsResult = {
  refs: Array<VcsRef>
  isRepo: boolean
  hasPrimaryRemote: boolean
  nextCursor: number | null
  totalCount: number
}

type VcsCreateWorktreeInput = {
  cwd: string
  refName: string
  newRefName?: string | null
  baseRefName?: string | null
  path: string | null
}

type VcsCreateWorktreeResult = { worktree: { path: string; refName: string } }

type VcsRemoveWorktreeInput = { cwd: string; path: string; force?: boolean | null }

type VcsCreateRefInput = { cwd: string; refName: string; switchRef?: boolean | null }

type VcsCreateRefResult = { refName: string }

type VcsSwitchRefInput = { cwd: string; refName: string }

type VcsSwitchRefResult = { refName: string | null }

type VcsInitInput = { cwd: string; kind?: "git" | "jj" | "unknown" | null }

type VcsError = 
  | VcsProcessSpawnError
  | VcsProcessExitError
  | VcsProcessTimeoutError
  | VcsProcessStdinWriteError
  | VcsProcessOutputReadError
  | VcsProcessOutputLimitError
  | VcsProcessMissingExitCodeError
  | VcsRepositoryDetectionError
  | VcsUnsupportedOperationError

type ReviewDiffPreviewInput = {
  cwd: string
  baseRef?: string | null
  ignoreWhitespace?: boolean
  file?: { path: string; previousPath: string | null; sourceKind: "working-tree" | "branch-range" }
}

type ReviewDiffPreviewResult = { cwd: string; generatedAt: string; sources: Array<ReviewDiffPreviewSource> }

type ReviewDiffPreviewError = VcsError | GitCommandError

type TerminalOpenInput = {
  threadId: string
  terminalId: string
  cwd: string
  worktreePath?: string | null
  cols?: number | null
  rows?: number | null
  env?: { [k: string]: string } | null
  providerInstanceId?: string | null
}

type TerminalSessionSnapshot = {
  threadId: string
  terminalId: string
  cwd: string
  worktreePath: string | null
  status: "starting" | "running" | "exited" | "error"
  pid: number | null
  history: string
  exitCode: number | null
  exitSignal: number | null
  label: string
  updatedAt: string
  sequence?: number | null
}

type TerminalError = 
  | TerminalCwdError
  | TerminalHistoryError
  | TerminalSessionLookupError
  | TerminalProviderInstanceNotFoundError
  | TerminalProviderEnvironmentError
  | TerminalNotRunningError
  | TerminalWriteError
  | TerminalResizeError

type TerminalAttachInput = {
  threadId: string
  terminalId: string
  cwd?: string | null
  worktreePath?: string | null
  cols?: number | null
  rows?: number | null
  env?: { [k: string]: string } | null
  providerInstanceId?: string | null
  restartIfNotRunning?: boolean | null
}

type TerminalAttachStreamEvent = 
  | { type: "snapshot"; snapshot: TerminalSessionSnapshot }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "output"; data: string }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "exited"
    exitCode: number | null
    exitSignal: number | null
  }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "closed" }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "error"; message: string }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "cleared" }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "restarted"
    snapshot: TerminalSessionSnapshot
  }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "activity"
    hasRunningSubprocess: boolean
    label: string
  }

type TerminalWriteInput = { threadId: string; terminalId: string; data: string }

type TerminalResizeInput = { threadId: string; terminalId: string; cols: number; rows: number }

type TerminalClearInput = { threadId: string; terminalId: string }

type TerminalRestartInput = {
  threadId: string
  terminalId: string
  cwd: string
  worktreePath?: string | null
  cols: number
  rows: number
  env?: { [k: string]: string } | null
  providerInstanceId?: string | null
}

type TerminalCloseInput = { threadId: string; terminalId?: string | null; deleteHistory?: boolean | null }

type TerminalEvent = 
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "started"
    snapshot: TerminalSessionSnapshot
  }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "output"; data: string }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "exited"
    exitCode: number | null
    exitSignal: number | null
  }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "closed" }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "error"; message: string }
  | { threadId: string; terminalId: string; sequence?: number | null; type: "cleared" }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "restarted"
    snapshot: TerminalSessionSnapshot
  }
  | {
    threadId: string
    terminalId: string
    sequence?: number | null
    type: "activity"
    hasRunningSubprocess: boolean
    label: string
  }

type TerminalMetadataStreamEvent = 
  | { type: "snapshot"; terminals: Array<TerminalSummary> }
  | { type: "upsert"; terminal: TerminalSummary }
  | { type: "remove"; threadId: string; terminalId: string }

type ServerConfigStreamEvent = 
  | ServerConfigStreamSnapshotEvent
  | ServerConfigStreamKeybindingsUpdatedEvent
  | ServerConfigStreamProviderStatusesEvent
  | ServerConfigStreamSettingsUpdatedEvent
  | ServerConfigStreamEnvironmentThemesUpdatedEvent
  | ServerConfigStreamUsageLimitSourcesUpdatedEvent

type ServerLifecycleStreamEvent = ServerLifecycleStreamWelcomeEvent | ServerLifecycleStreamReadyEvent

type AuthAccessStreamEvent = 
  | AuthAccessStreamSnapshotEvent
  | AuthAccessStreamPairingLinkUpsertedEvent
  | AuthAccessStreamPairingLinkRemovedEvent
  | AuthAccessStreamClientUpsertedEvent
  | AuthAccessStreamClientRemovedEvent

type AuthAccessStreamError = { _tag: "AuthAccessStreamError"; message: string }

type ClientOrchestrationCommand = 
  | ProjectCreateCommand
  | {
    type: "project.meta.update"
    commandId: string
    projectId: string
    title?: string | null
    workspaceRoot?: string | null
    defaultModelSelection?: ModelSelection | null
    defaultThreadEnvMode?: "local" | "worktree" | null
    autoPull?: boolean | null
    faviconPath?: string | null
    projectIcon?: ProjectIconOverride | null
    scripts?: Array<ProjectScript> | null
  }
  | { type: "project.delete"; commandId: string; projectId: string; force?: boolean | null }
  | {
    type: "thread.create"
    commandId: string
    threadId: string
    projectId: string
    title: string
    modelSelection: ModelSelection
    runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
    interactionMode: "default" | "plan" | null
    branch: string | null
    worktreePath: string | null
    createdAt: string
    historyImport?: true | null
  }
  | { type: "thread.delete"; commandId: string; threadId: string }
  | { type: "thread.archive"; commandId: string; threadId: string }
  | { type: "thread.unarchive"; commandId: string; threadId: string }
  | { type: "thread.settle"; commandId: string; threadId: string }
  | { type: "thread.unsettle"; commandId: string; threadId: string; reason: "user" }
  | { type: "thread.snooze"; commandId: string; threadId: string; snoozedUntil: string }
  | { type: "thread.unsnooze"; commandId: string; threadId: string; reason: "user" }
  | { type: "thread.pin"; commandId: string; threadId: string; orderKey?: string | null }
  | { type: "thread.unpin"; commandId: string; threadId: string }
  | { type: "thread.pin.reorder"; commandId: string; threadId: string; orderKey: string }
  | { type: "thread.auto-settle.set"; commandId: string; threadId: string; enabled: boolean }
  | { type: "thread.active.reorder"; commandId: string; threadId: string; orderKey: string }
  | {
    type: "thread.meta.update"
    commandId: string
    threadId: string
    title?: string | null
    regenerateTitle?: true | null
    modelSelection?: ModelSelection | null
    branch?: string | null
    expectedBranch?: string | null
    worktreePath?: string | null
    linkedPullRequest?: ThreadLinkedPullRequest | null
  }
  | {
    type: "thread.pull-request.link"
    commandId: string
    threadId: string
    host: string
    repository: string
    number: number
    url: string
    source: "manual" | "created" | "agent" | "stack" | "stack-dismissed"
  }
  | {
    type: "thread.pull-request.unlink"
    commandId: string
    threadId: string
    host: string
    repository: string
    number: number
  }
  | {
    type: "thread.runtime-mode.set"
    commandId: string
    threadId: string
    runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
    createdAt: string
  }
  | {
    type: "thread.interaction-mode.set"
    commandId: string
    threadId: string
    interactionMode: "default" | "plan"
    createdAt: string
  }
  | {
    type: "thread.turn.start"
    commandId: string
    threadId: string
    message: {
      messageId: string
      role: "user"
      text: string
      attachments: Array<
        | 
          | {
            type: "image"
            id?: string | null
            name: string
            mimeType: string
            sizeBytes: number
            dataUrl: string
            source?: SnapShotSource | null
          }
        | ChatAttachment>
      context?: OrchestrationMessageContext | null
    }
    modelSelection?: ModelSelection | null
    titleSeed?: string | null
    runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
    interactionMode: "default" | "plan"
    bootstrap?: 
      | {
        createThread?: 
          | {
            projectId: string
            title: string
            modelSelection: ModelSelection
            runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
            interactionMode: "default" | "plan"
            branch: string | null
            worktreePath: string | null
            createdAt: string
          }
          | null
        prepareWorktree?: 
          | {
            projectCwd: string
            baseBranch: string
            branch?: string | null
            startFromOrigin?: boolean | null
            requireWorktree?: boolean | null
          }
          | null
        runSetupScript?: boolean | null
      }
      | null
    sourceProposedPlan?: { threadId: string; planId: string } | null
    createdAt: string
  }
  | {
    type: "thread.turn.interrupt"
    commandId: string
    threadId: string
    turnId?: string | null
    createdAt: string
  }
  | {
    type: "thread.approval.respond"
    commandId: string
    threadId: string
    requestId: string
    decision: "accept" | "acceptForSession" | "acceptAlways" | "decline" | "cancel"
    createdAt: string
  }
  | {
    type: "thread.user-input.respond"
    commandId: string
    threadId: string
    requestId: string
    answers: ProviderUserInputAnswers
    attachmentsByQuestionId?: UserInputAttachments | null
    createdAt: string
  }
  | {
    type: "thread.user-input.dismiss"
    commandId: string
    threadId: string
    requestId: string
    createdAt: string
  }
  | {
    type: "thread.checkpoint.revert"
    commandId: string
    threadId: string
    turnCount: number
    createdAt: string
  }
  | {
    type: "thread.conversation.revert"
    commandId: string
    threadId: string
    turnCount: number
    createdAt: string
  }
  | {
    type: "thread.session.stop"
    commandId: string
    threadId: string
    createdAt: string
    onlyIfSettled?: boolean | null
  }

type DispatchResult = { sequence: number }

type OrchestrationDispatchCommandError = {
  _tag: "OrchestrationDispatchCommandError"
  message: string
  cause?: unknown | null
  bootstrapThreadDisposition?: "deleted" | "not-created" | null
}

type OrchestrationGetTurnDiffInput = { fromTurnCount: number; toTurnCount: number; threadId: string; ignoreWhitespace?: boolean }

type OrchestrationGetFullThreadDiffResult = { fromTurnCount: number; toTurnCount: number; threadId: string; diff: string }

type OrchestrationGetTurnDiffError = { _tag: "OrchestrationGetTurnDiffError"; message: string; cause?: unknown | null }

type OrchestrationGetFullThreadDiffInput = { threadId: string; toTurnCount: number; ignoreWhitespace?: boolean }

type OrchestrationGetFullThreadDiffError = { _tag: "OrchestrationGetFullThreadDiffError"; message: string; cause?: unknown | null }

type OrchestrationSearchThreadsInput = { query: string; limit?: number }

type OrchestrationSearchThreadsResult = { matches: Array<OrchestrationThreadSearchMatch> }

type OrchestrationSearchThreadsError = { _tag: "OrchestrationSearchThreadsError"; message: string; cause?: unknown | null }

type OrchestrationShellSnapshot = {
  snapshotSequence: number
  projects: Array<OrchestrationProjectShell>
  threads: Array<OrchestrationThreadShell>
  updatedAt: string
}

type OrchestrationGetSnapshotError = { _tag: "OrchestrationGetSnapshotError"; message: string; cause?: unknown | null }

type OrchestrationSubscribeShellInput = { afterSequence?: number; requestCompletionMarker?: boolean }

type OrchestrationShellStreamItem = 
  | { kind: "synchronized" }
  | { kind: "snapshot"; snapshot: OrchestrationShellSnapshot }
  | OrchestrationShellStreamEvent

type OrchestrationSubscribeThreadInput = {
  threadId: string
  reasoningMessages?: boolean
  afterSequence?: number
  requestCompletionMarker?: boolean
  turnLimit?: number
}

type OrchestrationThreadStreamItem = 
  | { kind: "synchronized" }
  | { kind: "snapshot"; snapshot: OrchestrationThreadDetailSnapshot }
  | { kind: "event"; event: OrchestrationEvent }

type ExecutionEnvironmentDescriptor = {
  environmentId: string
  label: string
  platform: ExecutionEnvironmentPlatform
  serverVersion: string
  orchestrationProtocolVersion?: number
  capabilities: ExecutionEnvironmentCapabilities
}

type ServerAuthDescriptor = {
  policy: "desktop-managed-local" | "loopback-browser" | "remote-reachable" | "unsafe-no-auth"
  bootstrapMethods: Array<"desktop-bootstrap" | "one-time-token">
  sessionMethods: Array<"browser-session-cookie" | "bearer-access-token" | "dpop-access-token">
  sessionCookieName: string
}

type ResolvedKeybindingRule = {
  command: 
    | "sidebar.toggle" | "navigation.back" | "navigation.forward" | "terminal.toggle" | "terminal.split" | "terminal.splitVertical" | "terminal.new" | "terminal.close" | "rightPanel.toggle" | "rightPanel.toggleMaximized" | "rightPanel.close" | "pullRequest.copyNumber" | "diff.toggle" | "preview.toggle" | "preview.refresh" | "preview.focusUrl" | "preview.zoomIn" | "preview.zoomOut" | "preview.resetZoom" | "commandPalette.toggle" | "filePicker.toggle" | "projectSearch.toggle" | "usage.open" | "theme.select" | "appearance.cycle" | "themeEditor.toggle" | "composer.stash" | "composer.host" | "composer.effort" | "composer.mode" | "composer.workspace" | "composer.previousWorktree" | "composer.branch" | "chat.new" | "chat.newLocal" | "chat.newWithoutProject" | "editor.openFavorite" | "usage.cost" | "usage.tokens" | "usage.limits" | "usage.period.day" | "usage.period.week" | "usage.period.month" | "usage.period.quarter" | "modelPicker.toggle" | "modelPicker.previousProvider" | "modelPicker.nextProvider" | "modelPicker.jump.1" | "modelPicker.jump.2" | "modelPicker.jump.3" | "modelPicker.jump.4" | "modelPicker.jump.5" | "modelPicker.jump.6" | "modelPicker.jump.7" | "modelPicker.jump.8" | "modelPicker.jump.9" | "thread.stop" | "thread.steerQueuedMessage" | "thread.previous" | "thread.next" | "thread.copyReference" | "thread.settle" | "thread.pin" | "thread.undo" | "thread.jump.1" | "thread.jump.2" | "thread.jump.3" | "thread.jump.4" | "thread.jump.5" | "thread.jump.6" | "thread.jump.7" | "thread.jump.8" | "thread.jump.9"
    | `script.${string}.run`
  shortcut: KeybindingShortcut
  whenAst?: KeybindingWhenNode
}

type ServerProvider = {
  instanceId: string
  driver: string
  displayName?: string
  accentColor?: string
  badgeLabel?: string
  continuation?: ServerProviderContinuation
  showInteractionModeToggle?: boolean
  reportsContextWindow?: boolean
  requiresNewThreadForModelChange?: boolean
  supportsConversationRollback?: boolean
  supportsTextGeneration?: boolean
  setup?: { canAuthenticate: boolean; canInstall: boolean }
  runtimePaths?: { homePath: string; shadowHomePath: string | null }
  enabled: boolean
  installed: boolean
  version: string | null
  status: "ready" | "warning" | "error" | "disabled"
  auth: ServerProviderAuth
  checkedAt: string
  message?: string
  availability?: "available" | "unavailable"
  unavailableReason?: string
  models: Array<ServerProviderModel>
  slashCommands: Array<ServerProviderSlashCommand>
  skills: Array<ServerProviderSkill>
  workspaceSnapshots?: Array<ServerProviderWorkspaceSnapshot>
  usageLimits?: ServerProviderUsageLimits
  versionAdvisory?: ServerProviderVersionAdvisory
  compatibilityAdvisory?: ServerProviderCompatibilityAdvisory
  updateState?: ServerProviderUpdateState
}

type EditorId = "cursor" | "trae" | "kiro" | "vscode" | "vscode-insiders" | "vscodium" | "zed" | "antigravity" | "idea" | "aqua" | "clion" | "datagrip" | "dataspell" | "goland" | "phpstorm" | "pycharm" | "rider" | "rubymine" | "rustrover" | "webstorm" | "file-manager"

type RemoteOpenTarget = { kind: "tailscale" | "mdns"; host: string }

type ServerObservability = {
  logsDirectoryPath: string
  localTracingEnabled: boolean
  otlpTracesUrl?: string | null
  otlpTracesEnabled: boolean
  otlpMetricsUrl?: string | null
  otlpMetricsEnabled: boolean
  otlpLogsUrl?: string | null
  otlpLogsEnabled: boolean | null
}

type EnvironmentTheme = {
  id: string
  version?: 1 | null
  name: string
  appearance: "light" | "dark"
  canvas?: string | null
  accent?: string | null
  colors?: { [k: string]: string } | null
  variants?: { light?: { [k: string]: string } | null; dark?: { [k: string]: string } | null } | null
}

type UsageLimitSourceSnapshot = {
  id: string
  kind: "cliproxy"
  label: string
  checkedAt: string
  accounts: Array<UsageLimitSourceAccount>
  error?: string
}

type ServerSettingsOperation = "normalize" | "check-exists" | "read-file" | "read-provider-history" | "read-project-settings" | "read-secret" | "remove-secret" | "remove-stale-secret" | "write-secret" | "write-file" | "prepare-directory"

type WorktreeCleanupRules = {
  worktreeAfterDays: number | null
  worktreeOnMerge: boolean
  worktreeOnDelete: boolean
  worktreeUnchanged: boolean
}

type ProjectScript = {
  id: string
  name: string
  command: string
  icon: "play" | "test" | "lint" | "configure" | "build" | "debug"
  runOnWorktreeCreate: boolean
  async?: boolean | null
  previewUrl?: string | null
  autoOpenPreview?: boolean | null
}

type ModelSelection = { instanceId: string; model: string; options?: ProviderOptionSelections }

type SshDeviceHostConfig = { id: string; label: string; target: string; identityFile?: string | null; port?: number | null }

type BackgroundActivitySettings = 
  | {
    schemaVersion?: 1 | null
    profile?: "balanced" | "performance" | "battery-saver" | "custom" | null
    baseProfile?: "balanced" | "performance" | "battery-saver"
    overrides?: BackgroundActivityOverrides | null
  }
  | null

type CustomModelSetting = string | CustomModelEntry

type UsageModelPriceOverride = {
  inputCostPerMillionTokens: number
  outputCostPerMillionTokens: number
  cacheReadCostPerMillionTokens?: number
  cacheWriteCostPerMillionTokens?: number
}

type ProjectSettingsOverrides = {
  worktreeCleanup?: { mode: "off" } | { mode: "custom"; rules: WorktreeCleanupRules } | null
  defaultModelSelection?: ModelSelection | null
  defaultRuntimeMode?: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
  defaultThreadEnvMode?: "local" | "worktree"
  newWorktreesStartFromOrigin?: boolean
  worktreeSubmodules?: "recursive" | "top-level" | "none"
  defaultAutoPull?: boolean
  defaultProjectScripts?: Array<ProjectScript>
  enableAgentBrowserAccess?: boolean
  enableAgentDeviceAccess?: boolean
  textGenerationModelSelection?: ModelSelection
  sourceControlWriterModelSelection?: ModelSelection | null
  sourceControlWritingStyle?: SourceControlWritingStyleSettings
  pullRequestMergeMethod?: "merge" | "squash" | "rebase" | null
  sidebarAutoSettleOnMerge?: boolean
  sidebarAutoSettleAfterDays?: number | null
  continueThreadsAfterServerUpdate?: boolean
  responseStreamingMode?: "turn" | "paragraph" | "token"
}

type BackgroundActivityOverrides = {
  automaticGitFetchInterval?: number
  providerHealthRefreshInterval?: number
  hostPowerMonitorActiveInterval?: number
  hostPowerMonitorIdleInterval?: number
  idleClientTtl?: number
  pauseWhenHostLocked?: boolean
  pauseWhenHostLowPower?: boolean
  pauseWhenClientLowPower?: boolean
  pauseWhenOnBattery?: boolean
}

type ProviderOptionSelections = Array<ProviderOptionSelection> | { [k: string]: unknown }

type ProviderInstanceConfig = {
  driver: string
  displayName?: string | null
  accentColor?: string | null
  environment?: Array<ProviderInstanceEnvironmentVariable>
  enabled?: boolean
  config?: unknown
}

type UsageLimitSourceConfig = {
  kind: "cliproxy"
  label?: string | null
  url: string
  managementKey: string | null
  enabled: boolean | null
}

type ProjectEntry = { path: string; kind: "file" | "directory"; ignored?: boolean | null }

type ProjectContentMatch = {
  path: string
  lineNumber: number
  lineContent: string
  matchRanges: Array<ProjectContentMatchRange>
}

type ExternalLauncherUnknownEditorError = { _tag: "ExternalLauncherUnknownEditorError"; editor: string }

type ExternalLauncherUnsupportedEditorError = { _tag: "ExternalLauncherUnsupportedEditorError"; editor: EditorId }

type ExternalLauncherCommandNotFoundError = { _tag: "ExternalLauncherCommandNotFoundError"; editor: EditorId; command: string }

type ExternalLauncherBrowserSpawnError = {
  _tag: "ExternalLauncherBrowserSpawnError"
  command: string
  args: Array<string>
  cause: unknown
  target: string
}

type ExternalLauncherEditorSpawnError = {
  _tag: "ExternalLauncherEditorSpawnError"
  command: string
  args: Array<string>
  cause: unknown
  editor: EditorId
  target: string
}

type FilesystemBrowseEntry = { name: string; fullPath: string }

type AssetResource = 
  | { _tag: "workspace-file"; threadId: string; path: string }
  | { _tag: "media-file"; threadId: string; path: string }
  | { _tag: "draft-workspace-file"; cwd: string; path: string }
  | {
    _tag: "attachment"
    attachmentId: string
    fileName?: string
    mimeType?: string
    disposition?: "inline" | "attachment"
  }
  | { _tag: "project-favicon"; cwd: string; path?: string | null }
  | { _tag: "native-app-icon"; app: ToolActivityNativeAppReference }
  | { _tag: "github-media"; cwd: string; url: string }

type AssetImageDimensions = { width: number; height: number }

type AssetWorkspaceContextNotFoundError = { _tag: "AssetWorkspaceContextNotFoundError"; resource: AssetResource }

type AssetWorkspaceContextResolutionError = { _tag: "AssetWorkspaceContextResolutionError"; resource: AssetResource; cause: unknown }

type AssetWorkspaceRootNormalizationError = { _tag: "AssetWorkspaceRootNormalizationError"; resource: AssetResource; cause: unknown }

type AssetWorkspacePathValidationError = { _tag: "AssetWorkspacePathValidationError"; resource: AssetResource; cause: unknown }

type AssetPreviewTypeValidationError = { _tag: "AssetPreviewTypeValidationError"; resource: AssetResource }

type AssetWorkspaceAssetInspectionError = { _tag: "AssetWorkspaceAssetInspectionError"; resource: AssetResource; cause: unknown }

type AssetWorkspaceAssetNotFoundError = { _tag: "AssetWorkspaceAssetNotFoundError"; resource: AssetResource }

type AssetWorkspaceResolutionError = { _tag: "AssetWorkspaceResolutionError"; resource: AssetResource; cause: unknown }

type AssetAttachmentNotFoundError = { _tag: "AssetAttachmentNotFoundError"; resource: AssetResource }

type AssetProjectFaviconResolutionError = { _tag: "AssetProjectFaviconResolutionError"; resource: AssetResource; cause: unknown }

type AssetProjectFaviconInspectionError = { _tag: "AssetProjectFaviconInspectionError"; resource: AssetResource; cause: unknown }

type AssetProjectFaviconNotFoundError = { _tag: "AssetProjectFaviconNotFoundError"; resource: AssetResource }

type AssetGitHubMediaUrlValidationError = { _tag: "AssetGitHubMediaUrlValidationError" }

type AssetSigningKeyLoadError = { _tag: "AssetSigningKeyLoadError"; resource: AssetResource; cause: unknown }

type VcsStatusLocalResult = {
  isRepo: boolean
  sourceControlProvider?: SourceControlProviderInfo | null
  hasPrimaryRemote: boolean
  isDefaultRef: boolean
  refName: string | null
  hasWorkingTreeChanges: boolean
  workingTree: {
    files: Array<{ path: string; insertions: number; deletions: number }>
    insertions: number
    deletions: number
  }
}

type VcsStatusRemoteResult = {
  hasUpstream: boolean
  aheadCount: number
  behindCount: number
  aheadOfDefaultCount?: number | null
  pr: 
    | {
      number: number
      title: string
      url: string
      baseRef: string
      headRef: string
      state: "open" | "closed" | "merged"
      isDraft?: boolean | null
      updatedAt?: string | null
    }
    | null
}

type GitManagerError = { _tag: "GitManagerError"; operation: string; cwd: string; detail: string; cause?: unknown | null }

type GitPullRequestMaterializationError = {
  _tag: "GitPullRequestMaterializationError"
  cwd: string
  pullRequestNumber: number
  headRepository: string | null
  headBranch: string
  localBranch: string
  cause: unknown
}

type SourceControlProviderError = {
  _tag: "SourceControlProviderError"
  provider: "github" | "gitlab" | "forgejo" | "azure-devops" | "bitbucket" | "unknown"
  operation: string
  cwd: string
  command?: string | null
  repository?: string | null
  reference?: string | null
  detail: string
  cause?: unknown | null
}

type TextGenerationError = { _tag: "TextGenerationError"; operation: string; detail: string; cause?: unknown | null }

type SourceControlProviderInfo = {
  kind: "github" | "gitlab" | "forgejo" | "azure-devops" | "bitbucket" | "unknown"
  name: string
  baseUrl: string
}

type GitRunStackedActionResult = {
  action: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr"
  branch: { status: "created" | "skipped_not_requested"; name?: string | null }
  commit: {
    status: "created" | "skipped_no_changes" | "skipped_not_requested"
    commitSha?: string | null
    subject?: string | null
  }
  push: {
    status: "pushed" | "skipped_not_requested" | "skipped_up_to_date"
    branch?: string | null
    upstreamBranch?: string | null
    setUpstream?: boolean | null
  }
  pr: {
    status: "created" | "opened_existing" | "skipped_not_requested"
    url?: string | null
    number?: number | null
    baseBranch?: string | null
    headBranch?: string | null
    title?: string | null
  }
  toast: {
    title: string
    description?: string | null
    cta: 
      | { kind: "none" }
      | { kind: "open_pr"; label: string; url: string }
      | { kind: "run_action"; label: string; action: GitRunStackedActionToastRunAction }
  }
}

type VcsRef = {
  name: string
  isRemote?: boolean | null
  remoteName?: string | null
  current: boolean
  isDefault: boolean
  worktreePath: string | null
}

type VcsProcessSpawnError = {
  _tag: "VcsProcessSpawnError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  cause: unknown
}

type VcsProcessExitError = {
  _tag: "VcsProcessExitError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  exitCode: number
  detail: string
  failureKind?: "authentication" | "not-found" | "rate-limited" | "command-failed" | null
  retryable?: boolean | null
  stderrLength?: number | null
  stderrTruncated?: boolean | null
}

type VcsProcessTimeoutError = {
  _tag: "VcsProcessTimeoutError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  timeoutMs: number
}

type VcsProcessStdinWriteError = {
  _tag: "VcsProcessStdinWriteError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  stdinBytes: number
  cause: unknown
}

type VcsProcessOutputReadError = {
  _tag: "VcsProcessOutputReadError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  stream: "stdout" | "stderr" | "exitCode"
  cause: unknown
}

type VcsProcessOutputLimitError = {
  _tag: "VcsProcessOutputLimitError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
  stream: "stdout" | "stderr"
  maxBytes: number
  observedBytes: number
}

type VcsProcessMissingExitCodeError = {
  _tag: "VcsProcessMissingExitCodeError"
  operation: string
  command: string
  cwd: string
  argumentCount?: number | null
}

type VcsRepositoryDetectionError = {
  _tag: "VcsRepositoryDetectionError"
  operation: string
  cwd: string
  detail: string
  cause?: unknown | null
}

type VcsUnsupportedOperationError = {
  _tag: "VcsUnsupportedOperationError"
  operation: string
  kind: "git" | "jj" | "unknown"
  detail: string
}

type ReviewDiffPreviewSource = {
  id: string
  kind: "working-tree" | "branch-range"
  title: string
  baseRef: string | null
  headRef: string | null
  diff: string
  diffHash: string
  truncated: boolean
  files?: Array<ReviewDiffFileStat>
}

type TerminalCwdError = TerminalCwdNotFoundError | TerminalCwdNotDirectoryError | TerminalCwdStatError

type TerminalHistoryError = {
  _tag: "TerminalHistoryError"
  operation: "read" | "truncate" | "migrate"
  threadId: string
  terminalId: string
  cause?: unknown | null
}

type TerminalSessionLookupError = { _tag: "TerminalSessionLookupError"; threadId: string; terminalId: string }

type TerminalProviderInstanceNotFoundError = { _tag: "TerminalProviderInstanceNotFoundError"; providerInstanceId: string }

type TerminalProviderEnvironmentError = { _tag: "TerminalProviderEnvironmentError"; providerInstanceId: string; cause: unknown }

type TerminalNotRunningError = { _tag: "TerminalNotRunningError"; threadId: string; terminalId: string }

type TerminalWriteError = {
  _tag: "TerminalWriteError"
  threadId: string
  terminalId: string
  terminalPid: number
  cause: unknown
}

type TerminalResizeError = {
  _tag: "TerminalResizeError"
  threadId: string
  terminalId: string
  terminalPid: number
  cols: number
  rows: number
  cause: unknown
}

type TerminalSummary = {
  threadId: string
  terminalId: string
  cwd: string
  worktreePath: string | null
  status: "starting" | "running" | "exited" | "error"
  pid: number | null
  exitCode: number | null
  exitSignal: number | null
  hasRunningSubprocess: boolean
  label: string
  updatedAt: string
}

type ServerConfigStreamSnapshotEvent = { version: 1; type: "snapshot"; config: ServerConfig }

type ServerConfigStreamKeybindingsUpdatedEvent = { version: 1; type: "keybindingsUpdated"; payload: ServerConfigKeybindingsUpdatedPayload }

type ServerConfigStreamProviderStatusesEvent = { version: 1; type: "providerStatuses"; payload: ServerConfigProviderStatusesPayload }

type ServerConfigStreamSettingsUpdatedEvent = { version: 1; type: "settingsUpdated"; payload: ServerConfigSettingsUpdatedPayload }

type ServerConfigStreamEnvironmentThemesUpdatedEvent = {
  version: 1
  type: "environmentThemesUpdated"
  payload: ServerConfigEnvironmentThemesUpdatedPayload
}

type ServerConfigStreamUsageLimitSourcesUpdatedEvent = {
  version: 1
  type: "usageLimitSourcesUpdated"
  payload: ServerConfigUsageLimitSourcesUpdatedPayload
}

type ServerLifecycleStreamWelcomeEvent = { version: 1; sequence: number; type: "welcome"; payload: ServerLifecycleWelcomePayload }

type ServerLifecycleStreamReadyEvent = { version: 1; sequence: number; type: "ready"; payload: ServerLifecycleReadyPayload }

type AuthAccessStreamSnapshotEvent = {
  version: 1
  revision: number
  type: "snapshot"
  payload: AuthAccessSnapshot
}

type AuthAccessStreamPairingLinkUpsertedEvent = {
  version: 1
  revision: number
  type: "pairingLinkUpserted"
  payload: AuthPairingLink
}

type AuthAccessStreamPairingLinkRemovedEvent = {
  version: 1
  revision: number
  type: "pairingLinkRemoved"
  payload: { id: string }
}

type AuthAccessStreamClientUpsertedEvent = {
  version: 1
  revision: number
  type: "clientUpserted"
  payload: AuthClientSession
}

type AuthAccessStreamClientRemovedEvent = {
  version: 1
  revision: number
  type: "clientRemoved"
  payload: { sessionId: string }
}

type ProjectCreateCommand = {
  type: "project.create"
  commandId: string
  projectId: string
  title: string
  workspaceRoot: string
  createWorkspaceRootIfMissing?: boolean | null
  defaultModelSelection?: ModelSelection | null
  createdAt: string
}

type ProjectIconOverride = 
  | {
    kind: "lucide"
    name: string
    color: ProjectIconColor
    monogramText?: string | null
    monogram?: string | null
  }
  | { kind: "emoji"; emoji: string }
  | { kind: "monogram"; text: string; color: ProjectIconColor }

type ThreadLinkedPullRequest = ThreadLinkedPullRequest | null

type SnapShotSource = {
  kind: "snap-shot"
  capturedAt: string
  appName: string
  windowTitle: string
  accessibleText?: string | null
  accessibility?: SnapShotAccessibility | null
  appIdentifier?: string | null
  appIconDataUrl?: string | null
}

type ChatAttachment = ChatImageAttachment | ChatFileAttachment | ChatUnknownAttachment

type OrchestrationMessageContext = { version: 1; records: Array<ComposerContextRecord> }

type ProviderUserInputAnswers = { [k: string]: unknown }

type UserInputAttachments = { [k: string]: Array<ChatImageAttachment | ChatFileAttachment> }

type OrchestrationThreadSearchMatch = {
  threadId: string
  projectId: string
  source: "user" | "assistant"
  snippet: string
  messageCreatedAt: string | null
}

type OrchestrationProjectShell = {
  id: string
  title: string
  workspaceRoot: string
  repositoryIdentity?: RepositoryIdentity | null
  defaultModelSelection: ModelSelection | null
  defaultThreadEnvMode?: "local" | "worktree" | null
  autoPull?: boolean | null
  faviconPath?: string | null
  projectIcon?: ProjectIconOverride | null
  scripts: Array<ProjectScript>
  createdAt: string
  updatedAt: string
}

type OrchestrationThreadShell = {
  id: string
  projectId: string
  title: string
  modelSelection: ModelSelection
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
  interactionMode: "default" | "plan" | null
  branch: string | null
  worktreePath: string | null
  linkedPullRequest?: ThreadLinkedPullRequest | null
  pullRequests: Array<ThreadPullRequestLink> | null
  branchPullRequest?: ThreadLinkedPullRequest | null
  latestTurn: OrchestrationLatestTurn | null
  createdAt: string
  updatedAt: string
  archivedAt: string | null
  settledOverride: "settled" | "active" | null
  settledAt: string | null
  unsettledAt?: string | null
  snoozedUntil?: string | null
  snoozedAt?: string | null
  pinnedAt?: string | null
  pinOrderKey?: string | null
  activeOrderKey?: string | null
  autoSettleDisabledAt?: string | null
  titleRegeneration?: ThreadTitleRegeneration | null
  titleState?: ThreadTitleState | null
  session: OrchestrationSession | null
  latestUserMessageAt: string | null
  hasPendingApprovals: boolean
  hasPendingUserInput: boolean
  hasActionableProposedPlan: boolean
  backgroundLiveness?: "working" | "monitoring" | null
  planProgress?: { step: string; completedSteps: number; totalSteps: number } | null
}

type OrchestrationShellStreamEvent = 
  | { kind: "project-upserted"; sequence: number; project: OrchestrationProjectShell }
  | { kind: "project-removed"; sequence: number; projectId: string }
  | { kind: "thread-upserted"; sequence: number; thread: OrchestrationThreadShell }
  | { kind: "thread-removed"; sequence: number; threadId: string }

type OrchestrationThreadDetailSnapshot = {
  snapshotSequence: number
  thread: OrchestrationThread
  page?: OrchestrationThreadDetailPage | null
}

type OrchestrationEvent = OrchestrationEventEnvelope & (
  | { type: "project.created"; payload: ProjectCreatedPayload }
  | { type: "project.meta-updated"; payload: ProjectMetaUpdatedPayload }
  | { type: "project.deleted"; payload: ProjectDeletedPayload }
  | { type: "thread.created"; payload: ThreadCreatedPayload }
  | { type: "thread.deleted"; payload: ThreadDeletedPayload }
  | { type: "thread.archived"; payload: ThreadArchivedPayload }
  | { type: "thread.unarchived"; payload: ThreadUnarchivedPayload }
  | { type: "thread.settled"; payload: ThreadSettledPayload }
  | { type: "thread.unsettled"; payload: ThreadUnsettledPayload }
  | { type: "thread.snoozed"; payload: ThreadSnoozedPayload }
  | { type: "thread.unsnoozed"; payload: ThreadUnsnoozedPayload }
  | { type: "thread.pinned"; payload: ThreadPinnedPayload }
  | { type: "thread.unpinned"; payload: ThreadUnpinnedPayload }
  | { type: "thread.pin-reordered"; payload: ThreadPinReorderedPayload }
  | { type: "thread.auto-settle-set"; payload: ThreadAutoSettleSetPayload }
  | { type: "thread.meta-updated"; payload: ThreadMetaUpdatedPayload }
  | { type: "thread.pull-request-linked"; payload: ThreadPullRequestLinkedPayload }
  | { type: "thread.pull-request-unlinked"; payload: ThreadPullRequestUnlinkedPayload }
  | { type: "thread.pull-request-synced"; payload: ThreadPullRequestSyncedPayload }
  | { type: "thread.runtime-mode-set"; payload: ThreadRuntimeModeSetPayload }
  | { type: "thread.interaction-mode-set"; payload: ThreadInteractionModeSetPayload }
  | { type: "thread.message-sent"; payload: ThreadMessageSentPayload }
  | { type: "thread.turn-start-requested"; payload: ThreadTurnStartRequestedPayload }
  | { type: "thread.turn-interrupt-requested"; payload: ThreadTurnInterruptRequestedPayload }
  | { type: "thread.approval-response-requested"; payload: ThreadApprovalResponseRequestedPayload }
  | { type: "thread.user-input-response-requested"; payload: {
      threadId: string
      requestId: string
      answers: ProviderUserInputAnswers
      attachmentsByQuestionId?: UserInputAttachments | null
      createdAt: string
    } }
  | { type: "thread.checkpoint-revert-requested"; payload: ThreadCheckpointRevertRequestedPayload }
  | { type: "thread.reverted"; payload: ThreadRevertedPayload }
  | { type: "thread.session-stop-requested"; payload: ThreadSessionStopRequestedPayload }
  | { type: "thread.session-set"; payload: ThreadSessionSetPayload }
  | { type: "thread.proposed-plan-upserted"; payload: ThreadProposedPlanUpsertedPayload }
  | { type: "thread.turn-diff-completed"; payload: ThreadTurnDiffCompletedPayload }
  | { type: "thread.activity-appended"; payload: ThreadActivityAppendedPayload }
)

type OrchestrationEventEnvelope = {
  sequence: number
  eventId: string
  aggregateKind: "project" | "thread"
  aggregateId: string
  occurredAt: string
  commandId: string | null
  causationEventId: string | null
  correlationId: string | null
  metadata: OrchestrationEventMetadata
  type: <see below>
  payload: <see below>
}

type ExecutionEnvironmentPlatform = {
  os: "darwin" | "linux" | "windows" | "unknown"
  arch: "arm64" | "x64" | "other"
  machine?: "server" | "cloud" | "linux" | "desktop" | "laptop" | "mac-mini" | "mac-studio"
}

type ExecutionEnvironmentCapabilities = {
  repositoryIdentity: boolean | null
  connectionProbe?: boolean
  attachmentUploads?: boolean
  questionAttachments?: boolean
  fileAttachments?: { maxUploadBytes: number }
  pullRequests?: boolean
  inlineMessageContext?: boolean
  requiredWorktreeBootstrap?: boolean
  threadSettlement?: boolean
  threadAutoSettlement?: boolean
  storageCleanup?: boolean
  projectWorktreeCleanup?: boolean
  threadRestartContinuation?: boolean
  projectSettingsOverrides?: boolean
  threadSnooze?: boolean
  environmentThemes?: boolean
  usageLimitSources?: boolean
  usagePriceOverrides?: boolean
  threadPinning?: boolean
  threadPinReorder?: boolean
  threadActiveReorder?: boolean
  threadAutoSettleOptOut?: boolean
  threadTitleRegeneration?: boolean
  threadPullRequestLinking?: boolean
  threadPullRequests?: boolean
  pullRequestStackActions?: boolean
  serverSelfUpdate?: "boot-service" | "respawn" | "desktop-managed"
  serverSelfUpdateProgress?: boolean
  serverUpdateThreadContinuation?: boolean
  agentActivityPublishing?: boolean
  projectCloneTracking?: boolean
  environmentIcon?: boolean
  desktopAppUpdate?: boolean
}

type KeybindingShortcut = {
  key: string
  metaKey: boolean
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
  modKey: boolean
}

type KeybindingWhenNode = 
  | { type: "identifier"; name: string }
  | { type: "not"; node: KeybindingWhenNode }
  | { type: "and"; left: KeybindingWhenNode; right: KeybindingWhenNode }
  | { type: "or"; left: KeybindingWhenNode; right: KeybindingWhenNode }

type ServerProviderContinuation = { groupKey: string }

type ServerProviderAuth = {
  status: "authenticated" | "unauthenticated" | "unknown"
  type?: string
  label?: string
  email?: string
  subscriptionSharing?: boolean
  profileId?: string
}

type ServerProviderModel = {
  slug: string
  name: string
  shortName?: string
  subProvider?: string
  aliases?: Array<string>
  badge?: "new"
  isCustom: boolean
  isDefault?: boolean
  isLegacy?: boolean
  capabilities: { optionDescriptors?: Array<ProviderOptionDescriptor> } | null
}

type ServerProviderSlashCommand = { name: string; description?: string; input?: { hint: string } }

type ServerProviderSkill = {
  name: string
  description?: string
  path: string
  scope?: string
  enabled: boolean
  displayName?: string
  shortDescription?: string
  userInvocationOnly?: boolean
  userInvocable?: boolean
}

type ServerProviderWorkspaceSnapshot = {
  cwd: string
  checkedAt: string
  slashCommands: Array<ServerProviderSlashCommand>
  skills: Array<ServerProviderSkill>
}

type ServerProviderUsageLimits = {
  checkedAt: string
  windows: Array<{
    id: string
    kind: "session" | "weekly" | "monthly" | "other"
    label: string
    usedPercent: number
    resetsAt?: string
    windowDurationMins?: number
  }>
  credentialFingerprint?: string
  resetCredits?: ServerProviderResetCredits
  externalUsage?: { label: string; url: string }
  unavailable?: { reason: "unsupported" | "probeFailed"; message?: string }
}

type ServerProviderVersionAdvisory = {
  status: "unknown" | "current" | "behind_latest"
  currentVersion: string | null
  latestVersion: string | null
  updateCommand: string | null
  canUpdate: boolean
  canInstallVersion?: boolean
  checkedAt: string | null
  message: string | null
}

type ServerProviderCompatibilityAdvisory = {
  status: "unknown" | "supported" | "graceful" | "unsupported" | "broken"
  latestVersionStatus?: "unknown" | "supported" | "graceful" | "unsupported" | "broken"
  message: string | null
  recommendedVersion: string | null
  recommendedRange: string | null
}

type ServerProviderUpdateState = {
  status: "idle" | "queued" | "running" | "succeeded" | "failed" | "unchanged"
  startedAt: string | null
  finishedAt: string | null
  message: string | null
  output: string | null
}

type UsageLimitSourceAccount = {
  id: string
  driver: string
  email?: string
  plan?: string
  usageLimits: ServerProviderUsageLimits
}

type CustomModelEntry = {
  slug: string
  name?: string | null
  capabilities?: { optionDescriptors?: Array<ProviderOptionDescriptor> | null } | null
}

type SourceControlWritingStyleSettings = {
  mode: "repo_conventions" | "conventional_commits" | "custom" | null
  customInstructions: string | null
  followChangeRequestTemplates: boolean | null
}

type ProviderOptionSelection = { id: string; value: string | boolean }

type ProviderInstanceEnvironmentVariable = { name: string; value: string | null; sensitive: boolean | null; valueRedacted?: boolean }

type ProjectContentMatchRange = { start: number; end: number }

type ToolActivityNativeAppReference = { _tag: "app-id"; appId: string } | { _tag: "display-name"; displayName: string }

type GitRunStackedActionToastRunAction = { kind: "commit" | "push" | "create_pr" | "commit_push" | "commit_push_pr" }

type ReviewDiffFileStat = {
  path: string
  previousPath: string | null
  additions: number
  deletions: number
}

type TerminalCwdNotFoundError = { _tag: "TerminalCwdNotFoundError"; cwd: string }

type TerminalCwdNotDirectoryError = { _tag: "TerminalCwdNotDirectoryError"; cwd: string }

type TerminalCwdStatError = { _tag: "TerminalCwdStatError"; cwd: string; cause: unknown }

type ServerConfigKeybindingsUpdatedPayload = {
  keybindings: Array<ResolvedKeybindingRule>
  issues: Array<
    | { kind: "keybindings.malformed-config"; message: string }
    | { kind: "keybindings.invalid-entry"; message: string; index: number }>
}

type ServerConfigProviderStatusesPayload = { providers: Array<ServerProvider> }

type ServerConfigSettingsUpdatedPayload = { settings: ServerSettings }

type ServerConfigEnvironmentThemesUpdatedPayload = { themes: Array<EnvironmentTheme> }

type ServerConfigUsageLimitSourcesUpdatedPayload = { sources: Array<UsageLimitSourceSnapshot> }

type ServerLifecycleWelcomePayload = {
  environment: ExecutionEnvironmentDescriptor
  cwd: string
  projectName: string
  bootstrapStatus?: "pending" | "complete" | null
  bootstrapProjectId?: string | null
  bootstrapThreadId?: string | null
  bootstrapProjectCreated?: boolean | null
  bootstrapThreadCreated?: boolean | null
}

type ServerLifecycleReadyPayload = {
  at: string
  environment: ExecutionEnvironmentDescriptor
  updateOutcome?: ServerSelfUpdateOutcome
}

type AuthAccessSnapshot = { pairingLinks: Array<AuthPairingLink>; clientSessions: Array<AuthClientSession> }

type AuthPairingLink = {
  id: string
  scopes: Array<"orchestration:read" | "orchestration:operate" | "terminal:operate" | "review:write" | "access:read" | "access:write" | "relay:read" | "relay:write">
  subject: string
  label?: string
  createdAt: string
  expiresAt: string
}

type AuthClientSession = {
  sessionId: string
  subject: string
  scopes: Array<"orchestration:read" | "orchestration:operate" | "terminal:operate" | "review:write" | "access:read" | "access:write" | "relay:read" | "relay:write">
  method: "browser-session-cookie" | "bearer-access-token" | "dpop-access-token"
  client: AuthClientMetadata
  issuedAt: string
  expiresAt: string
  lastConnectedAt: string | null
  connected: boolean
  current: boolean
}

type ProjectIconColor = "gray" | "red" | "orange" | "amber" | "yellow" | "lime" | "green" | "emerald" | "teal" | "cyan" | "sky" | "blue" | "indigo" | "violet" | "purple" | "fuchsia" | "pink" | "rose"

type SnapShotAccessibility = 
  | { format: "flat-text"; text: string; truncated: boolean }
  | {
    format: "element-tree"
    coordinateSpace: "captured-image"
    imageSize: { width: number; height: number }
    truncated: boolean
    root: SnapShotAccessibilityNode
  }

type ChatImageAttachment = {
  type: "image"
  id: string
  name: string
  mimeType: string
  sizeBytes: number
  source?: SnapShotSource | null
}

type ChatFileAttachment = {
  type: "file"
  id: string
  name: string
  mimeType: string
  sizeBytes: number
  source?: PastedTextAttachmentSource | null
}

type ChatUnknownAttachment = { type: string; id: string; name: string; mimeType: string; sizeBytes: number }

type ComposerContextRecord = 
  | ImageContextRecord
  | FileContextRecord
  | TerminalContextRecord
  | ElementContextRecord
  | PreviewAnnotationContextRecord
  | ReviewCommentContextRecord
  | MentionContextRecord
  | SkillContextRecord
  | UnknownContextRecord

type RepositoryIdentity = {
  canonicalKey: string
  locator: RepositoryIdentityLocator
  webUrl?: string
  rootPath?: string
  displayName?: string
  provider?: string
  owner?: string
  name?: string
}

type ThreadPullRequestLink = {
  host: string
  repository: string
  number: number
  url: string
  source: "manual" | "created" | "agent" | "stack" | "stack-dismissed"
  linkedAt: string
  snapshot: ThreadPullRequestSnapshot | null
  stack: ThreadPullRequestStack | null
}

type OrchestrationLatestTurn = {
  turnId: string
  state: "running" | "interrupted" | "completed" | "error"
  requestedAt: string
  startedAt: string | null
  completedAt: string | null
  assistantMessageId: string | null
  sourceProposedPlan?: { threadId: string; planId: string } | null
}

type ThreadTitleRegeneration = { requestId: string; startedAt: string }

type ThreadTitleState = ThreadTitleState | null

type OrchestrationSession = {
  threadId: string
  status: "idle" | "starting" | "running" | "ready" | "interrupted" | "stopped" | "error"
  providerName: string | null
  providerInstanceId?: string | null
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access" | null
  activeTurnId: string | null
  lastError: string | null
  updatedAt: string
}

type OrchestrationThread = {
  id: string
  projectId: string
  title: string
  modelSelection: ModelSelection
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
  interactionMode: "default" | "plan" | null
  branch: string | null
  worktreePath: string | null
  linkedPullRequest?: ThreadLinkedPullRequest | null
  pullRequests: Array<ThreadPullRequestLink> | null
  branchPullRequest?: ThreadLinkedPullRequest | null
  latestTurn: OrchestrationLatestTurn | null
  createdAt: string
  updatedAt: string
  archivedAt: string | null
  settledOverride: "settled" | "active" | null
  settledAt: string | null
  unsettledAt?: string | null
  snoozedUntil?: string | null
  snoozedAt?: string | null
  pinnedAt?: string | null
  pinOrderKey?: string | null
  activeOrderKey?: string | null
  autoSettleDisabledAt?: string | null
  titleRegeneration?: ThreadTitleRegeneration | null
  titleState?: ThreadTitleState | null
  deletedAt: string | null
  messages: Array<OrchestrationMessage>
  proposedPlans: 
    | Array<{
      id: string
      turnId: string | null
      planMarkdown: string
      implementedAt?: string | null
      implementationThreadId?: string | null
      createdAt: string
      updatedAt: string
    }>
    | null
  activities: Array<OrchestrationThreadActivity>
  checkpoints: Array<OrchestrationCheckpointSummary>
  session: OrchestrationSession | null
}

type OrchestrationThreadDetailPage = {
  beforeCursor: string | null
  hasMore: boolean
  snapshotSequence: number
  threadSequence?: number
}

type OrchestrationEventMetadata = {
  providerTurnId?: string | null
  providerItemId?: string | null
  adapterKey?: string | null
  requestId?: string | null
  ingestedAt?: string | null
  historyImport?: boolean | null
  deferredTurn?: boolean | null
  origin?: OrchestrationClientOrigin | null
}

type ProjectCreatedPayload = {
  projectId: string
  title: string
  workspaceRoot: string
  repositoryIdentity?: RepositoryIdentity | null
  defaultModelSelection: ModelSelection | null
  faviconPath?: string | null
  projectIcon?: ProjectIconOverride | null
  scripts: Array<ProjectScript>
  createdAt: string
  updatedAt: string
}

type ProjectMetaUpdatedPayload = {
  projectId: string
  title?: string | null
  workspaceRoot?: string | null
  repositoryIdentity?: RepositoryIdentity | null
  defaultModelSelection?: ModelSelection | null
  defaultThreadEnvMode?: "local" | "worktree" | null
  autoPull?: boolean | null
  faviconPath?: string | null
  projectIcon?: ProjectIconOverride | null
  scripts?: Array<ProjectScript> | null
  updatedAt: string
}

type ProjectDeletedPayload = { projectId: string; deletedAt: string }

type ThreadCreatedPayload = {
  threadId: string
  projectId: string
  title: string
  modelSelection: ModelSelection
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access" | null
  interactionMode: "default" | "plan" | null
  branch: string | null
  worktreePath: string | null
  createdAt: string
  updatedAt: string
}

type ThreadDeletedPayload = { threadId: string; deletedAt: string }

type ThreadArchivedPayload = { threadId: string; archivedAt: string; updatedAt: string }

type ThreadUnarchivedPayload = { threadId: string; updatedAt: string }

type ThreadSettledPayload = { threadId: string; settledAt: string; updatedAt: string }

type ThreadUnsettledPayload = { threadId: string; reason: "user" | "activity"; updatedAt: string }

type ThreadSnoozedPayload = { threadId: string; snoozedUntil: string; snoozedAt: string; updatedAt: string }

type ThreadUnsnoozedPayload = { threadId: string; reason: "user" | "activity"; updatedAt: string }

type ThreadPinnedPayload = { threadId: string; pinnedAt: string; pinOrderKey?: string | null; updatedAt: string }

type ThreadUnpinnedPayload = { threadId: string; updatedAt: string }

type ThreadPinReorderedPayload = { threadId: string; orderKey: string; updatedAt: string }

type ThreadAutoSettleSetPayload = { threadId: string; autoSettleDisabledAt: string | null; updatedAt: string }

type ThreadMetaUpdatedPayload = {
  threadId: string
  activeOrderKey?: string | null
  title?: string | null
  regenerateTitle?: true | null
  previousTitle?: string | null
  titleRegeneration?: ThreadTitleRegeneration | null
  titleState?: ThreadTitleState | null
  modelSelection?: ModelSelection | null
  branch?: string | null
  worktreePath?: string | null
  linkedPullRequest?: ThreadLinkedPullRequest | null
  branchPullRequest?: ThreadLinkedPullRequest | null
  updatedAt: string
}

type ThreadPullRequestLinkedPayload = { threadId: string; link: ThreadPullRequestLink; updatedAt: string }

type ThreadPullRequestUnlinkedPayload = { threadId: string; host: string; repository: string; number: number; updatedAt: string }

type ThreadPullRequestSyncedPayload = {
  threadId: string
  host: string
  repository: string
  number: number
  snapshot: ThreadPullRequestSnapshot
  stack: ThreadPullRequestStack | null
  updatedAt: string
}

type ThreadRuntimeModeSetPayload = {
  threadId: string
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access"
  updatedAt: string
}

type ThreadInteractionModeSetPayload = { threadId: string; interactionMode: "default" | "plan" | null; updatedAt: string }

type ThreadMessageSentPayload = {
  threadId: string
  messageId: string
  role: "user" | "assistant" | "system" | "reasoning"
  text: string
  attachments?: Array<ChatAttachment> | null
  context?: OrchestrationMessageContext | null
  turnId: string | null
  streaming: boolean
  createdAt: string
  updatedAt: string
}

type ThreadTurnStartRequestedPayload = {
  threadId: string
  messageId: string
  modelSelection?: ModelSelection | null
  titleSeed?: string | null
  runtimeMode: "approval-required" | "auto-accept-edits" | "auto" | "full-access" | null
  interactionMode: "default" | "plan" | null
  sourceProposedPlan?: { threadId: string; planId: string } | null
  createdAt: string
}

type ThreadTurnInterruptRequestedPayload = { threadId: string; turnId?: string | null; createdAt: string }

type ThreadApprovalResponseRequestedPayload = {
  threadId: string
  requestId: string
  decision: "accept" | "acceptForSession" | "acceptAlways" | "decline" | "cancel"
  createdAt: string
}

type ThreadCheckpointRevertRequestedPayload = { threadId: string; turnCount: number; restoreFiles?: boolean | null; createdAt: string }

type ThreadRevertedPayload = { threadId: string; turnCount: number }

type ThreadSessionStopRequestedPayload = { threadId: string; createdAt: string }

type ThreadSessionSetPayload = { threadId: string; session: OrchestrationSession }

type ThreadProposedPlanUpsertedPayload = { threadId: string; proposedPlan: OrchestrationProposedPlan }

type ThreadTurnDiffCompletedPayload = {
  threadId: string
  turnId: string
  checkpointTurnCount: number
  checkpointRef: string
  status: "ready" | "missing" | "error"
  files: Array<OrchestrationCheckpointFile>
  assistantMessageId: string | null
  completedAt: string
}

type ThreadActivityAppendedPayload = { threadId: string; activity: OrchestrationThreadActivity }

type ProviderOptionDescriptor = SelectProviderOptionDescriptor | BooleanProviderOptionDescriptor

type ServerProviderResetCredits = ServerProviderResetCredits

type ServerSelfUpdateOutcome = {
  id: string
  fromVersion: string
  targetVersion: string
  status: "committed" | "rolled-back" | "failed"
  reason?: string
}

type AuthClientMetadata = {
  label?: string
  ipAddress?: string
  userAgent?: string
  deviceType: "desktop" | "mobile" | "tablet" | "bot" | "unknown"
  os?: string
  browser?: string
}

type SnapShotAccessibilityNode = {
  role: string
  name?: string
  value?: string
  description?: string
  bounds: { x: number; y: number; width: number; height: number } | null
  state?: {
    active?: boolean | null
    busy?: boolean | null
    checked?: "on" | "off" | "mixed" | null
    editable?: boolean | null
    enabled?: boolean | null
    expanded?: boolean | null
    focused?: boolean | null
    selected?: boolean | null
    visible?: boolean | null
  }
  actions?: Array<string>
  children: Array<SnapShotAccessibilityNode>
}

type PastedTextAttachmentSource = { _tag: "pasted-text" }

type ImageContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "image"
  attachmentId: string
  name: string
  mimeType: string
  sizeBytes: number
}

type FileContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "file"
  attachmentId: string
  name: string
  mimeType: string
  sizeBytes: number
}

type TerminalContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "terminal"
  terminalId: string
  terminalLabel: string
  lineStart: number
  lineEnd: number
  text: string
}

type ElementContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "element"
  pageUrl: string
  pageTitle: string | null
  tagName: string
  selector: string | null
  htmlPreview: string
  componentName: string | null
  source: ElementContextSource | null
  styles: string
}

type PreviewAnnotationContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "preview-annotation"
  annotationId: string
  pageUrl: string
  pageTitle: string | null
  comment: string
  targetSummary: string
  styleChanges: Array<string>
  elements?: Array<ElementContextDetails>
  elementIds?: Array<string>
  regionCount?: number
  strokeCount?: number
  styleChangeDetails?: Array<PreviewAnnotationStyleChangeSchema>
  screenshotContextId?: string
}

type ReviewCommentContextRecord = {
  version: 1
  contextId: string
  label: string
  kind: "review-comment"
  sectionId: string
  sectionTitle: string
  filePath: string
  startIndex: number
  endIndex: number
  rangeLabel: string
  text: string
  diff: string
  fenceLanguage?: string
  pullRequest?: PullRequestContextMetadata
}

type MentionContextRecord = { version: 1; contextId: string; label: string; kind: "mention"; path: string }

type SkillContextRecord = { version: 1; contextId: string; label: string; kind: "skill"; name: string }

type UnknownContextRecord = { version: 1; contextId: string; label: string; kind: string; payload: unknown }

type RepositoryIdentityLocator = { source: "git-remote"; remoteName: string; remoteUrl: string }

type ThreadPullRequestSnapshot = {
  state: "open" | "closed" | "merged"
  title: string
  headBranch: string
  baseBranch: string
  isDraft: boolean
  updatedAt: string | null
  syncedAt: string
  closedAt?: string | null
  mergedAt?: string | null
  author?: PullRequestActor | null
  additions?: number | null
  deletions?: number | null
  changedFiles?: number | null
  reviewDecision?: "approved" | "changes-requested" | "review-required" | null
  checksState?: "passing" | "failing" | "pending" | null
  mergeability?: "mergeable" | "conflicting" | "unknown" | null
}

type ThreadPullRequestStack = {
  kind: "native"
  id: string
  number: number
  url: string
  base: string
  layers: Array<ThreadPullRequestStackLayer>
}

type OrchestrationMessage = {
  id: string
  role: "user" | "assistant" | "system" | "reasoning"
  text: string
  attachments?: Array<ChatAttachment> | null
  context?: OrchestrationMessageContext | null
  turnId: string | null
  streaming: boolean
  createdAt: string
  updatedAt: string
}

type OrchestrationThreadActivity = {
  id: string
  tone: "info" | "tool" | "approval" | "error"
  kind: string
  summary: string
  payload: unknown
  turnId: string | null
  sequence?: number | null
  createdAt: string
}

type OrchestrationCheckpointSummary = {
  turnId: string
  checkpointTurnCount: number
  checkpointRef: string
  status: "ready" | "missing" | "error"
  files: Array<OrchestrationCheckpointFile>
  assistantMessageId: string | null
  completedAt: string
}

type OrchestrationClientOrigin = { surface?: "web" | "desktop" | "mobile" | "cli" | null; appVersion?: string | null }

type OrchestrationProposedPlan = {
  id: string
  turnId: string | null
  planMarkdown: string
  implementedAt: string | null
  implementationThreadId: string | null
  createdAt: string
  updatedAt: string
}

type OrchestrationCheckpointFile = { path: string; kind: string; additions: number; deletions: number }

type SelectProviderOptionDescriptor = {
  id: string
  label: string
  description?: string
  type: "select"
  options: Array<ProviderOptionChoice>
  currentValue?: string
  promptInjectedValues?: Array<string>
}

type BooleanProviderOptionDescriptor = {
  id: string
  label: string
  description?: string
  type: "boolean"
  currentValue?: boolean
}

type ElementContextSource = {
  functionName: string | null
  fileName: string | null
  lineNumber: number | null
  columnNumber: number | null
}

type ElementContextDetails = {
  pageUrl: string
  pageTitle: string | null
  tagName: string
  selector: string | null
  htmlPreview: string
  componentName: string | null
  source: ElementContextSource | null
  styles: string
}

type PreviewAnnotationStyleChangeSchema = {
  targetId: string
  selector: string | null
  property: string
  previousValue: string
  value: string
}

type PullRequestContextMetadata = {
  number: number
  title: string
  url: string
  headBranch: string
  baseBranch: string
  state: "open" | "closed" | "merged"
  isDraft: boolean
}

type PullRequestActor = PullRequestActor | null

type ThreadPullRequestStackLayer = { number: number; headBranch: string; state: "open" | "closed" | "merged" }

type ProviderOptionChoice = { id: string; label: string; description?: string; isDefault?: boolean }


```

## Appendix B. Regenerating shapes and frame traces

Run against a fresh upstream checkout whenever nightly moves. Requires Node 22.6+ (uses `--experimental-strip-types`). The npm `effect` package is unpatched; upstream's patch only touches client hooks, the pinger, MCP, and HTTP cookies, not Schema or RPC framing.

```bash
U=~/L-Projects/t3UI-refs/t3code-upstream
EFFECT_VERSION=$(grep -m1 -oE "^  effect@[0-9][^:]*" $U/pnpm-lock.yaml | sed 's/  effect@//')   # e.g. 4.0.0-rc.115
mkdir -p /tmp/t3proto && cd /tmp/t3proto
echo '{"name":"t3proto","private":true,"type":"module"}' > package.json
npm i "effect@$EFFECT_VERSION"
rm -rf contracts && cp -r $U/packages/contracts/src ./contracts
# save printer3.ts, dump3.ts, events.ts, gen.sh from below, then:
./gen.sh all.txt                                       # every method + definitions
./gen.sh core.txt orchestration.subscribeThread orchestration.dispatchCommand   # selected methods
node --experimental-strip-types --no-warnings events.ts   # compact OrchestrationEvent
node --experimental-strip-types --no-warnings wire.ts     # RPC framing traces (section 1.9)
```

### B.1 `printer3.ts`

```ts
// Renders the JSON wire shape of an Effect Schema (rc.115) as compact TS-like types.
// Looks through ForwardCompatible* (Unknown-encoded) wrappers to the real element shape.
import * as Schema from "effect/Schema";
import * as AST from "effect/SchemaAST";
import * as C from "./contracts/index.ts";
export const codecOf = (s: any): AST.AST => Schema.toCodecJson(s).ast;
export const names = new Map<AST.AST, string>();
const prefer = (k: string) => !/^(Ws|Trimmed|NonEmpty|NonNegative|Positive|IsoDateTime)/.test(k);
for (const [k, v] of Object.entries(C)) {
  if (Schema.isSchema(v)) { const e = codecOf(v); const prev = names.get(e); if (!prev || (!prefer(prev) && prefer(k))) names.set(e, k); }
}
const pending: string[] = [];
const defined = new Set<string>();
const nameToNode = new Map<string, AST.AST>();
const lit = (v: unknown) => typeof v === "string" ? JSON.stringify(v) : String(v);
function isOpaque(a: AST.AST): boolean {
  if (a._tag === "Unknown" || a._tag === "Any") return true;
  if (a._tag === "Declaration" && (a as any).encoding === undefined && (a as any).typeParameters.length === 0) return true;
  if (a._tag === "Arrays" && (a as any).elements.length === 0 && (a as any).rest.length === 1) return isOpaque((a as any).rest[0]);
  if (a._tag === "Union") return (a as any).types.some(isOpaque);
  if (a._tag === "Objects") { const ps = (a as any).propertySignatures; return ps.length > 0 && ps.every((p: any) => isOpaque(p.type)); }
  return false;
}
function step(a: AST.AST): AST.AST {
  // follow encoding chain to the wire representation, unless that is opaque
  let cur = a;
  for (let i = 0; i < 20 && cur.encoding; i++) {
    const last = cur.encoding[cur.encoding.length - 1].to;
    const lastEnc = AST.toEncoded(last);
    if (isOpaque(lastEnc)) return AST.replaceEncoding(cur, undefined);
    cur = last;
  }
  return cur;
}
function isNamedWorthy(a: AST.AST): boolean {
  const s = step(a);
  if (s._tag === "Objects") return (s as any).propertySignatures.length + (s as any).indexSignatures.length > 0;
  if (s._tag === "Union") return (s as any).types.some((t: AST.AST) => { const x = step(t); return x._tag === "Objects" || x._tag === "Suspend"; }) || (s as any).types.length > 8;
  if (s._tag === "Declaration") return true;
  return false;
}
const fpMemo = new WeakMap<AST.AST, string>();
export function fingerprint(a: AST.AST): string { let f = fpMemo.get(a); if (f === undefined) { f = render(a, true, 0, new Set(), true).replace(/ \| (undefined|null)/g, ""); fpMemo.set(a, f); } return f; }
let fpNames: Map<string, string> | undefined;
function fpWorthy(a: AST.AST): boolean { const s = step(a); return s._tag === "Union" || (s._tag === "Objects" && (s as any).propertySignatures.length >= 3); }
function fpName(a: AST.AST): string | undefined {
  if (!fpWorthy(a)) return undefined;
  if (!fpNames) { fpNames = new Map(); for (const [node, n] of names) { if (isNamedWorthy(node) && fpWorthy(node)) { const f = fingerprint(node); if (!fpNames.has(f) || (/^(Ws)/.test(fpNames.get(f)!))) fpNames.set(f, n); } } }
  return fpNames.get(fingerprint(a));
}
export function render(a0: AST.AST, top = true, ind = 0, seen = new Set<AST.AST>(), pure = false): string {
  const pad = "  ".repeat(ind);
  const nm = pure || top || !isNamedWorthy(a0) ? undefined : (names.get(a0) ?? fpName(a0));
  if (nm) {
    const n = nm;
    if (!nameToNode.has(n)) nameToNode.set(n, a0);
    if (!defined.has(n)) { defined.add(n); pending.push(n); }
    return n;
  }
  const a = step(a0);
  if (!pure && a !== a0 && !top && names.has(a) && isNamedWorthy(a)) return render(a, false, ind, seen, pure);
  switch (a._tag) {
    case "String": return "string";
    case "Number": return "number";
    case "Boolean": return "boolean";
    case "Null": return "null";
    case "Undefined": return "undefined";
    case "Void": return "void";
    case "Never": return "never";
    case "Unknown": return "unknown";
    case "Any": return "any";
    case "BigInt": return "bigint";
    case "ObjectKeyword": return "object";
    case "Literal": return lit((a as any).literal);
    case "Enum": return (a as any).enums.map((e: any) => lit(e[1])).join(" | ");
    case "TemplateLiteral": return "`" + (a as any).parts.map((p: any) => p._tag === "Literal" ? p.literal : "${" + render(p, false, 0, seen, pure) + "}").join("") + "`";
    case "Declaration": {
      const tp = (a as any).typeParameters;
      return tp.length ? `Decl<${tp.map((t: AST.AST) => render(t, false, ind, seen, pure)).join(", ")}>` : "unknown";
    }
    case "Suspend": {
      if (seen.has(a)) return names.get(a0) ?? "<recursive>";
      seen.add(a); const r = render((a as any).thunk(), top, ind, seen, pure); seen.delete(a); return r;
    }
    case "Union": {
      const parts: string[] = [...new Set<string>((a as any).types.map((t: AST.AST) => render(t, false, ind + 1, seen, pure)))];
      const s = parts.join(" | ");
      const allLit = (a as any).types.every((t: AST.AST) => { const x = step(t); return x._tag === "Literal" || x._tag === "TemplateLiteral" || x._tag === "Null"; });
      if (!s.includes("\n") && (s.length < 110 || allLit)) return s;
      return parts.map((p: string) => "\n" + pad + "  | " + p).join("");
    }
    case "Arrays": {
      const els = (a as any).elements.map((e: AST.AST) => render(e, false, ind, seen, pure) + (AST.isOptional(e) ? "?" : ""));
      const rest = (a as any).rest.map((e: AST.AST) => render(e, false, ind, seen, pure));
      if (els.length === 0 && rest.length === 1) return `Array<${rest[0]}>`;
      return `[${[...els, ...rest.map((r: string, i: number) => i === 0 ? "..." + r + "[]" : r)].join(", ")}]`;
    }
    case "Objects": {
      const o = a as any; const lines: string[] = [];
      for (const ps of o.propertySignatures) lines.push(`${pad}  ${String(ps.name)}${AST.isOptional(ps.type) ? "?" : ""}: ${render(ps.type, false, ind + 1, seen, pure)}`);
      for (const is of o.indexSignatures) lines.push(`${pad}  [k: ${render(is.parameter, false, 0, seen, pure)}]: ${render(is.type, false, ind + 1, seen, pure)}`);
      if (!lines.length) return "{}";
      const one = "{ " + lines.map(l => l.trim()).join("; ") + " }";
      if (one.length < 100 && !one.includes("\n")) return one;
      return "{\n" + lines.join("\n") + "\n" + pad + "}";
    }
  }
  return "?" + (a as any)._tag;
}
const NUM = 'number | "Infinity" | "-Infinity" | "NaN"';
export const fix = (s: string) => s.split(NUM).join("number").replace(/ \| null \| null/g, " | null");
export function drainDefs(filter?: (n: string) => boolean): string {
  let out = "";
  while (pending.length) {
    const n = pending.shift()!;
    if (filter && !filter(n)) continue;
    let body = fix(render(nameToNode.get(n)!, true));
    if (body.trim() === n) body = fix(render(nameToNode.get(n)!, true, 0, new Set(), true));
    out += `type ${n} = ${body}\n\n`;
  }
  return out;
}
```

### B.2 `dump3.ts`

```ts
import * as RpcSchema from "effect/unstable/rpc/RpcSchema";
import * as Option from "effect/Option";
import * as C from "./contracts/index.ts";
import { render, codecOf, fix, drainDefs, names } from "./printer3.ts";
const only = process.argv.slice(2);
const group: any = (C as any).WsRpcGroup;
for (const [tag, rpc] of group.requests as Map<string, any>) {
  if (only.length && !only.includes(tag)) continue;
  const ss = RpcSchema.getStreamSchemas(rpc.successSchema);
  const isStream = Option.isSome(ss);
  const success = isStream ? ss.value.success : rpc.successSchema;
  const error = isStream ? ss.value.error : rpc.errorSchema;
  const r = (s: any) => fix(render(codecOf(s), false));
  console.log(`### ${tag} [${isStream ? "stream" : "unary"}]`);
  console.log("payload: " + r(rpc.payloadSchema));
  console.log((isStream ? "item: " : "success: ") + r(success));
  console.log("error: " + r(error));
  console.log();
}
console.log("// ===== DEFINITIONS =====\n" + drainDefs());
```

### B.3 `events.ts`

```ts
import * as AST from "effect/SchemaAST";
import * as C from "./contracts/index.ts";
import { codecOf, render, fix, drainDefs } from "./printer3.ts";
const u: any = codecOf((C as any).OrchestrationEvent);
const lines: string[] = [];
let env = "";
for (const m of u.types) {
  const o: any = m;
  const t = o.propertySignatures.find((p: any) => p.name === "type").type.literal;
  const pl = o.propertySignatures.find((p: any) => p.name === "payload").type;
  if (!env) env = "{\n" + o.propertySignatures.filter((p: any) => p.name !== "type" && p.name !== "payload").map((p: any) => `  ${p.name}: ${fix(render(p.type, false, 1))}`).join("\n") + "\n  type: <see below>\n  payload: <see below>\n}";
  lines.push(`  | { type: ${JSON.stringify(t)}; payload: ${fix(render(pl, false, 2))} }`);
}
console.log("type OrchestrationEvent = OrchestrationEventEnvelope & (\n" + lines.join("\n") + "\n)\n\ntype OrchestrationEventEnvelope = " + env + "\n");
console.log("// ---- referenced\n" + drainDefs());
```

### B.4 `gen.sh`

```bash
#!/bin/bash
# usage: gen.sh out.txt methods...
out=$1; shift
cd "$(dirname "$0")"
node --experimental-strip-types --no-warnings dump3.ts "$@" > $out 2>&1
perl -0pi -e 's/\{\n\s*provider\?: unknown \| null\n\s*instanceId\?: unknown \| null\n\s*model: unknown\n\s*options\?: unknown \| null\n\s*\}/ModelSelection/g; s/ \| undefined//g' $out
```

### B.5 `wire.ts` (in-process rc.115 RpcServer driven by raw JSON frames)

```ts
// In-memory capture of real effect rc.115 RpcServer frames, driven by a raw JSON client.
import * as Effect from "effect/Effect";
import * as Layer from "effect/Layer";
import * as Queue from "effect/Queue";
import * as Schema from "effect/Schema";
import * as Stream from "effect/Stream";
import * as Scope from "effect/Scope";
import * as Fiber from "effect/Fiber";
import * as Socket from "effect/unstable/socket/Socket";
import * as SocketServer from "effect/unstable/socket/SocketServer";
import { Rpc, RpcGroup, RpcServer, RpcSerialization } from "effect/unstable/rpc";

class DemoError extends Schema.TaggedError<DemoError>()("DemoError", { message: Schema.String }) {}
const Unary = Rpc.make("demo.unary", { payload: { n: Schema.Number }, success: Schema.String, error: DemoError });
const Void = Rpc.make("demo.void", { payload: Schema.Struct({}), success: Schema.Void });
const Str = Rpc.make("demo.stream", { payload: { count: Schema.Number }, success: Schema.Number, error: DemoError, stream: true });
const Group = RpcGroup.make(Unary, Void, Str);
const Handlers = Group.toLayer({
  "demo.unary": ({ n }) => n < 0 ? Effect.fail(new DemoError({ message: "negative" })) : n === 13 ? Effect.die(new Error("unlucky")) : Effect.succeed(`ok:${n}`),
  "demo.void": () => Effect.void,
  "demo.stream": ({ count }) => Stream.range(1, count).pipe(Stream.tap(() => Effect.sleep("20 millis"))),
});

const t0 = Date.now();
const log = (dir: string, s: string) => console.log(`${String(Date.now() - t0).padStart(5)}ms ${dir} ${s}`);

const program = Effect.gen(function* () {
  const toServer = yield* Queue.unbounded<string>();
  const toClient = yield* Queue.unbounded<string>();
  const socket = Socket.make({
    reader: Effect.succeed({ pull: Effect.map(Queue.take(toServer), (s) => [s] as const), upgrade: () => Effect.void }) as any,
    writer: Effect.succeed({
      write: (c: any) => Effect.sync(() => { if (typeof c === "string") Queue.offerUnsafe(toClient, c); }),
      writeAll: (cs: any) => Effect.sync(() => cs.forEach((c: any) => Queue.offerUnsafe(toClient, c))),
    }) as any,
  });
  const fakeServer = SocketServer.SocketServer.of({
    address: { _tag: "TcpAddress", hostname: "mem", port: 0 } as any,
    run: (handler: any) => Effect.andThen(Effect.forkScoped(handler(socket)), Effect.never) as any,
  } as any);
  yield* Layer.launch(RpcServer.layer(Group).pipe(
    Layer.provide(Handlers),
    Layer.provide(RpcServer.layerProtocolSocketServer),
    Layer.provide(RpcSerialization.layerJson),
    Layer.provide(Layer.succeed(SocketServer.SocketServer, fakeServer)),
  )).pipe(Effect.forkScoped);
  const send = (o: unknown) => { const s = JSON.stringify(o); log("C->S", s); Queue.offerUnsafe(toServer, s); };
  const recv = (ms = 400) => Effect.gen(function* () {
    const out: any[] = [];
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      const m = yield* Queue.poll(toClient);
      if (m._tag === "Some") { log("S->C", m.value); out.push(JSON.parse(m.value)); } else yield* Effect.sleep("5 millis");
    }
    return out;
  });
  console.log("--- unary success / typed failure / defect / void");
  send({ _tag: "Request", id: "1", tag: "demo.unary", payload: { n: 2 }, headers: [] });
  send({ _tag: "Request", id: "2", tag: "demo.unary", payload: { n: -1 }, headers: [] });
  send({ _tag: "Request", id: "3", tag: "demo.unary", payload: { n: 13 }, headers: [] });
  send({ _tag: "Request", id: 4, tag: "demo.void", payload: {}, headers: [] });
  yield* recv(200);
  console.log("--- batch array in one frame + ping");
  const batch = JSON.stringify([{ _tag: "Request", id: "5", tag: "demo.unary", payload: { n: 5 }, headers: [] }, { _tag: "Ping" }]);
  log("C->S", batch); Queue.offerUnsafe(toServer, batch);
  yield* recv(200);
  console.log("--- bad payload / unknown tag / missing headers");
  send({ _tag: "Request", id: "6", tag: "demo.unary", payload: { n: "x" }, headers: [] });
  send({ _tag: "Request", id: "7", tag: "demo.nope", payload: {}, headers: [] });
  send({ _tag: "Request", id: "8", tag: "demo.unary", payload: { n: 8 } });
  yield* recv(200);
  console.log("--- stream WITHOUT acks (expect stall after first chunk)");
  send({ _tag: "Request", id: "10", tag: "demo.stream", payload: { count: 4 }, headers: [] });
  yield* recv(400);
  console.log("--- now ack id \"10\" three times, slowly");
  for (let i = 0; i < 4; i++) { send({ _tag: "Ack", requestId: "10" }); yield* recv(120); }
  console.log("--- stream with ack type mismatch (request id string, ack number)");
  send({ _tag: "Request", id: "11", tag: "demo.stream", payload: { count: 3 }, headers: [] });
  yield* recv(150);
  send({ _tag: "Ack", requestId: 11 });
  yield* recv(200);
  send({ _tag: "Interrupt", requestId: "11" });
  yield* recv(200);
  console.log("--- interrupt unknown id");
  send({ _tag: "Interrupt", requestId: "999" });
  yield* recv(150);
  console.log("--- duplicate in-flight id");
  send({ _tag: "Request", id: "12", tag: "demo.stream", payload: { count: 2 }, headers: [] });
  send({ _tag: "Request", id: "12", tag: "demo.unary", payload: { n: 1 }, headers: [] });
  yield* recv(150);
  send({ _tag: "Ack", requestId: "12" }); yield* recv(100); send({ _tag: "Ack", requestId: "12" }); yield* recv(150);
  console.log("--- garbage frame");
  log("C->S", "not json"); Queue.offerUnsafe(toServer, "not json");
  yield* recv(150);
  send({ _tag: "Request", id: "13", tag: "demo.unary", payload: { n: 1 }, headers: [] });
  yield* recv(150);
});
Effect.runPromise(Effect.scoped(program)).then(() => process.exit(0), (e) => { console.error(e); process.exit(1); });
```
