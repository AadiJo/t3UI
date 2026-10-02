#!/usr/bin/env node
// Cross-checks T3UI's DPoP proofs (crates/t3-client/src/cloud/dpop.rs) against the TypeScript
// the relay and environments run. Read-only use of the fork checkout (same verifier and web
// DPoP code as upstream main).
//
//   node e2e/dpop-crosscheck.mjs [path/to/connect_probe]
//
// 1. Every proof from `connect_probe proofs` must pass `verifyDpopProof`
//    (packages/shared/src/dpop.ts) with the expected method, URL, thumbprint and access token,
//    and fail it when any of those differ.
// 2. A proof from the web client's `createBrowserDpopProof` (apps/web/src/cloud/dpop.ts, jose)
//    must have the same header and payload members, in the same order, as ours.
// Writes a JSON report to $T3UI_DPOP_REPORT (default /tmp/t3ui-dpop-crosscheck.json).
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const fork = process.env.T3UI_FORK_DIR ?? `${homedir()}/L-Projects/t3code-again`;
const probe =
  process.argv[2] ??
  `${process.env.CARGO_TARGET_DIR ?? `${homedir()}/L-Projects/t3UI-targets/cloud`}/debug/examples/connect_probe`;
const reportPath = process.env.T3UI_DPOP_REPORT ?? "/tmp/t3ui-dpop-crosscheck.json";

const { verifyDpopProof } = await import(pathToFileURL(`${fork}/packages/shared/src/dpop.ts`).href);
const web = await import(pathToFileURL(`${fork}/apps/web/src/cloud/dpop.ts`).href);
const requireFromWeb = createRequire(`${fork}/apps/web/package.json`);
const Effect = await import(pathToFileURL(requireFromWeb.resolve("effect/Effect")).href);

const decode = (part) => JSON.parse(Buffer.from(part, "base64url").toString("utf8"));
const now = () => Math.floor(Date.now() / 1000);
const failures = [];
const check = (condition, message) => {
  if (!condition) failures.push(message);
};

// 1. Our proofs through the real verifier.
const run = spawnSync(probe, ["proofs", "40"], { encoding: "utf8" });
if (run.status !== 0) throw new Error(`connect_probe proofs failed: ${run.stderr}`);
const proofs = run.stdout.trim().split("\n").map((line) => JSON.parse(line));
let verified = 0;
for (const sample of proofs) {
  const input = {
    proof: sample.proof,
    method: sample.method,
    url: sample.url,
    nowEpochSeconds: now(),
    expectedThumbprint: sample.thumbprint,
    ...(sample.accessToken ? { expectedAccessToken: sample.accessToken } : {}),
  };
  const result = verifyDpopProof(input);
  check(result.ok, `rejected ${sample.method} ${sample.url}: ${result.reason}`);
  if (result.ok) verified += 1;
  // The verifier must also notice each kind of mismatch, or the checks above prove nothing.
  check(!verifyDpopProof({ ...input, method: sample.method === "GET" ? "POST" : "GET" }).ok, "method mismatch accepted");
  check(!verifyDpopProof({ ...input, url: `${sample.url.split("?")[0]}x` }).ok, "URL mismatch accepted");
  check(!verifyDpopProof({ ...input, expectedThumbprint: "wrong" }).ok, "thumbprint mismatch accepted");
  check(!verifyDpopProof({ ...input, expectedAccessToken: "other-token" }).ok, "access token mismatch accepted");
  check(!verifyDpopProof({ ...input, nowEpochSeconds: now() + 600 }).ok, "stale proof accepted");
}

// 2. Structure against the web client's proof.
const key = await Effect.runPromise(web.generateBrowserDpopKey);
const reference = await Effect.runPromise(
  Effect.provide(
    web.createBrowserDpopProof({
      method: "POST",
      url: "https://relay.t3.codes/v1/environments/env-1/connect?x=1",
      accessToken: "relay-access-token",
      proofKey: key,
    }),
    web.browserCryptoLayer,
  ),
);
const [refHeader, refPayload] = reference.proof.split(".").slice(0, 2).map(decode);
const ours = proofs.find((p) => p.accessToken === "relay-access-token");
const [ourHeader, ourPayload] = ours.proof.split(".").slice(0, 2).map(decode);
const keys = (value) => JSON.stringify(Object.keys(value));
check(keys(refHeader) === keys(ourHeader), `header members ${keys(ourHeader)} != ${keys(refHeader)}`);
check(keys(refHeader.jwk) === keys(ourHeader.jwk), `jwk members ${keys(ourHeader.jwk)} != ${keys(refHeader.jwk)}`);
check(keys(refPayload) === keys(ourPayload), `payload members ${keys(ourPayload)} != ${keys(refPayload)}`);
check(refHeader.typ === ourHeader.typ && refHeader.alg === ourHeader.alg, "typ/alg differ");
check(refPayload.htu === ourPayload.htu, `htu ${ourPayload.htu} != ${refPayload.htu}`);
check(web.createBrowserDpopProof !== undefined && reference.thumbprint === key.thumbprint, "web thumbprint");

const report = {
  fork,
  proofsVerified: verified,
  proofsTotal: proofs.length,
  reference: { header: Object.keys(refHeader), jwk: Object.keys(refHeader.jwk), payload: Object.keys(refPayload) },
  ours: { header: Object.keys(ourHeader), jwk: Object.keys(ourHeader.jwk), payload: Object.keys(ourPayload) },
  failures,
};
writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(JSON.stringify(report, null, 2));
if (failures.length > 0) {
  console.error(`dpop-crosscheck: ${failures.length} failure(s)`);
  process.exit(1);
}
console.log(`dpop-crosscheck: ok (${verified}/${proofs.length} proofs verified; report ${reportPath})`);
