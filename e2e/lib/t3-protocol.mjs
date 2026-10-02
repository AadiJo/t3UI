// Minimal T3 Code client protocol: HTTP auth exchange plus the Effect RPC
// JSON protocol spoken over the server's `/ws` WebSocket.
//
// Node built-ins only (Node 22+ has a global WebSocket). This file doubles as
// an executable protocol reference for non-JS clients, so every wire detail is
// spelled out in comments next to the code that depends on it.
//
// Usage:
//   const auth = await exchangePairingToken(baseUrl, "ABCD1234EFGH");
//   const rpc = await RpcSocket.connect(baseUrl, auth.accessToken);
//   const config = await rpc.call("server.getConfig", {});
//   const sub = rpc.subscribe("orchestration.subscribeShell", {}, (item) => ...);
//   sub.close();
//   rpc.close();

// OAuth 2.0 token exchange (RFC 8693) constants, from packages/contracts/src/auth.ts.
const GRANT_TYPE_TOKEN_EXCHANGE = "urn:ietf:params:oauth:grant-type:token-exchange";
const TOKEN_TYPE_BOOTSTRAP = "urn:t3:params:oauth:token-type:environment-bootstrap";
const TOKEN_TYPE_ACCESS = "urn:ietf:params:oauth:token-type:access_token";

/** Thrown for non-2xx HTTP responses; `body` carries the server's tagged error JSON. */
export class HttpError extends Error {
  constructor(method, url, status, body) {
    super(`${method} ${url} -> ${status}: ${typeof body === "string" ? body : JSON.stringify(body)}`);
    this.status = status;
    this.body = body;
  }
}

async function http(method, url, { headers = {}, body } = {}) {
  const response = await fetch(url, { method, headers, body });
  const text = await response.text();
  let parsed = text;
  try {
    parsed = text.length > 0 ? JSON.parse(text) : null;
  } catch {
    // Non-JSON bodies are surfaced as strings.
  }
  if (!response.ok) throw new HttpError(method, url, response.status, parsed);
  return parsed;
}

/**
 * GET /.well-known/t3/environment. Unauthenticated. Returns the environment
 * descriptor: { environmentId, label, platform, serverVersion, capabilities, ... }.
 */
export function fetchEnvironmentDescriptor(baseUrl) {
  return http("GET", new URL("/.well-known/t3/environment", baseUrl));
}

/**
 * Exchange a one-time pairing credential for a reusable bearer access token.
 *
 * Wire: POST /oauth/token, body is application/x-www-form-urlencoded (NOT JSON).
 * The credential is the short code printed by `t3 serve` ("Token: XXXX") or by
 * `t3 auth pairing create`. Credentials are single-use: a second exchange of
 * the same code fails with 401 {"_tag":"EnvironmentAuthInvalidError"}.
 * The startup credential printed by `serve` carries administrative scopes
 * (access:write etc.); CLI-issued ones carry the standard client scopes.
 *
 * Returns { accessToken, scopes, expiresIn } where the token is sent later as
 * `Authorization: Bearer <token>` (valid ~30 days).
 */
export async function exchangePairingToken(baseUrl, credential, { label = "t3ui-e2e-seed" } = {}) {
  const body = new URLSearchParams({
    grant_type: GRANT_TYPE_TOKEN_EXCHANGE,
    subject_token: credential,
    subject_token_type: TOKEN_TYPE_BOOTSTRAP,
    requested_token_type: TOKEN_TYPE_ACCESS,
    client_label: label,
  });
  const result = await http("POST", new URL("/oauth/token", baseUrl), {
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body,
  });
  return {
    accessToken: result.access_token,
    scopes: String(result.scope ?? "").split(" ").filter(Boolean),
    expiresIn: result.expires_in,
  };
}

/**
 * Mint another one-time pairing credential (e.g. for a browser to open
 * `/pair#token=<credential>`). Requires the `access:write` scope, which only
 * the startup (administrative) credential grants.
 *
 * Wire: POST /api/auth/pairing-token with Bearer auth and JSON body
 * { label?, scopes? }. Without `scopes` the credential gets the standard
 * client scopes; a caller can delegate any subset of its own scopes (e.g.
 * access:read/access:write so the Settings > Connections page is fully usable).
 * Returns { id, credential, label?, expiresAt }.
 */
export function mintPairingToken(baseUrl, accessToken, { label, scopes } = {}) {
  return http("POST", new URL("/api/auth/pairing-token", baseUrl), {
    headers: { authorization: `Bearer ${accessToken}`, "content-type": "application/json" },
    body: JSON.stringify({ ...(label ? { label } : {}), ...(scopes ? { scopes } : {}) }),
  });
}

/**
 * Issue a short-lived WebSocket ticket. Browsers (and Node's global
 * WebSocket) cannot set an Authorization header on the upgrade request, so the
 * server accepts `?wsTicket=<ticket>` on `/ws` instead. Tickets expire in
 * about 5 minutes; mint a fresh one per connection attempt.
 *
 * Wire: POST /api/auth/websocket-ticket with Bearer auth, empty body.
 * Returns { ticket, expiresAt }.
 */
export async function issueWebSocketTicket(baseUrl, accessToken) {
  const result = await http("POST", new URL("/api/auth/websocket-ticket", baseUrl), {
    headers: { authorization: `Bearer ${accessToken}` },
  });
  return result.ticket;
}

/** Rejection value for an RPC whose Exit is a Failure. `cause` is the encoded Cause array. */
export class RpcFailure extends Error {
  constructor(tag, cause) {
    const first = cause?.[0];
    const detail =
      first?._tag === "Fail"
        ? JSON.stringify(first.error)
        : first?._tag === "Die"
          ? `defect: ${JSON.stringify(first.defect)}`
          : JSON.stringify(cause);
    super(`${tag} failed: ${detail}`);
    this.tag = tag;
    this.cause = cause;
  }
}

/**
 * Effect RPC over WebSocket with `RpcSerialization.layerJson`.
 *
 * Framing: every WebSocket text frame is one JSON value, either a single
 * message object or an array of them (decoders must accept both).
 *
 * Client -> server messages:
 *   {"_tag":"Request","id":"<decimal bigint as string>","tag":"<method>","payload":{...},"headers":[]}
 *   {"_tag":"Ack","requestId":"<id>"}        after EVERY stream Chunk (see below)
 *   {"_tag":"Interrupt","requestId":"<id>"}  cancel a running request/stream
 *   {"_tag":"Ping"}                           keepalive, server answers {"_tag":"Pong"}
 *
 * Server -> client messages:
 *   {"_tag":"Exit","requestId":"<id>","exit":{"_tag":"Success","value":<A>}}
 *   {"_tag":"Exit","requestId":"<id>","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":<E>} | {"_tag":"Die","defect":<any>} | {"_tag":"Interrupt"}]}}
 *   {"_tag":"Chunk","requestId":"<id>","values":[<A>, ...]}   stream items, batched
 *   {"_tag":"Defect","defect":<any>}          connection-wide failure, fails every pending request
 *   {"_tag":"Pong"}
 *
 * Stream backpressure (the main surprise): after the server writes a Chunk for
 * a streaming RPC it blocks that stream until the client sends an Ack for the
 * same requestId. A client that never acks receives exactly one Chunk per
 * subscription and then silence. Unary RPCs never produce Chunks.
 *
 * Errors: an unknown `tag` or an undecodable payload comes back as an Exit
 * Failure with a `Die` defect string (e.g. "Unknown request tag: x"), not as
 * a `Fail`. Typed domain errors arrive as `Fail` with a tagged error object.
 */
export class RpcSocket {
  #ws;
  #nextId = 1n;
  #pending = new Map(); // id -> { tag, resolve, reject, onChunk? }
  #pingTimer;
  #closed = false;

  constructor(ws) {
    this.#ws = ws;
    ws.addEventListener("message", (event) => this.#onFrame(event.data));
    ws.addEventListener("close", (event) => this.#failAll(new Error(`socket closed (${event.code})`)));
    // The official client pings every 5 s and drops the socket if a Pong is
    // missed; the server does not ping on its own. A plain keepalive is enough here.
    this.#pingTimer = setInterval(() => this.#send({ _tag: "Ping" }), 5000);
    this.#pingTimer.unref?.();
  }

  /** Authenticate (ticket) and open `/ws`. Resolves once the socket is open. */
  static async connect(baseUrl, accessToken) {
    const ticket = await issueWebSocketTicket(baseUrl, accessToken);
    const url = new URL("/ws", baseUrl);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("wsTicket", ticket);
    const ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      // A rejected upgrade (bad/expired ticket) surfaces as error + close 1006.
      ws.addEventListener("error", () => reject(new Error(`WebSocket upgrade failed for ${url.origin}/ws`)), {
        once: true,
      });
    });
    return new RpcSocket(ws);
  }

  /** Unary RPC. Resolves with the Success value, rejects with RpcFailure. */
  call(tag, payload) {
    const id = this.#allocateId();
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { tag, resolve, reject });
      this.#send({ _tag: "Request", id, tag, payload, headers: [] });
    });
  }

  /**
   * Streaming RPC (subscriptions). `onItem` runs once per value. Returns
   * { done, close }: `done` settles when the server ends or fails the stream;
   * `close()` sends an Interrupt.
   */
  subscribe(tag, payload, onItem) {
    const id = this.#allocateId();
    const done = new Promise((resolve, reject) => {
      this.#pending.set(id, { tag, resolve, reject, onChunk: onItem });
    });
    done.catch(() => {}); // callers that only use close() should not see unhandled rejections
    this.#send({ _tag: "Request", id, tag, payload, headers: [] });
    return {
      done,
      close: () => {
        if (this.#pending.delete(id)) this.#send({ _tag: "Interrupt", requestId: id });
      },
    };
  }

  close() {
    this.#closed = true;
    clearInterval(this.#pingTimer);
    this.#ws.close();
  }

  #allocateId() {
    // Request ids are bigints on both sides; on the wire they are decimal strings.
    const id = this.#nextId.toString();
    this.#nextId += 1n;
    return id;
  }

  #send(message) {
    if (this.#ws.readyState === WebSocket.OPEN) this.#ws.send(JSON.stringify(message));
  }

  #onFrame(data) {
    const decoded = JSON.parse(typeof data === "string" ? data : Buffer.from(data).toString("utf8"));
    for (const message of Array.isArray(decoded) ? decoded : [decoded]) this.#onMessage(message);
  }

  #onMessage(message) {
    switch (message._tag) {
      case "Pong":
        return;
      case "Chunk": {
        const entry = this.#pending.get(message.requestId);
        // Ack first so the server can prepare the next batch while we process this one.
        this.#send({ _tag: "Ack", requestId: message.requestId });
        if (!entry?.onChunk) return;
        for (const value of message.values) {
          try {
            entry.onChunk(value);
          } catch (error) {
            console.error(`[rpc] ${entry.tag} handler threw`, error);
          }
        }
        return;
      }
      case "Exit": {
        const entry = this.#pending.get(message.requestId);
        if (!entry) return;
        this.#pending.delete(message.requestId);
        if (message.exit._tag === "Success") entry.resolve(message.exit.value);
        else entry.reject(new RpcFailure(entry.tag, message.exit.cause));
        return;
      }
      case "Defect":
        this.#failAll(new Error(`server defect: ${JSON.stringify(message.defect)}`));
        return;
      default:
        console.warn("[rpc] unexpected message", message);
    }
  }

  #failAll(error) {
    clearInterval(this.#pingTimer);
    for (const [, entry] of this.#pending) {
      if (!this.#closed) entry.reject(error);
    }
    this.#pending.clear();
  }
}
