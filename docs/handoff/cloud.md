# T3 Connect: backend handoff for the UI

Backend: `t3_client::cloud` (GPUI-free; futures run on the networking runtime, await them from
GPUI). Spec: `docs/spec/connections.md` 3, 4, 9. Visual target: the fork's `components/cloud/*`
and `components/clerk/*` (Clerk's stock sign-in modal in T3 colors). Old UI WIP: branch `cloud`.

## Launch

```rust
let connect = T3Connect::new(CloudConfig::production(), open_secret_store(SecretBackend::from_env()))?;
let mut state = connect.state();          // watch::Receiver<CloudState { account, discovery }>
connect.restore().await?;                 // checks the stored session with Clerk, then refreshes
```

`state::boot` skips relay targets. For each catalog entry with `KnownTarget::Relay`,
`connect.saved_endpoint(&saved)` is `Some` when it belongs to the signed-in account: start it
with `EnvironmentOptions::from_saved(&saved, endpoint)`, kind `Remote`. Drop entries of other
accounts. When an account signs in later, start its entries and `retry_now()` started ones.

## Sign in / out

- Email: `let p = connect.start_email_sign_in(email).await?` (Clerk emails a code), show
  `p.masked_email()`, then `p.verify(code).await?`; `p.resend().await?`. Wrong codes return
  Clerk's message and can be retried on the same `p`.
- Providers: `connect.sign_in_with(OAuthProvider::GitHub, auth).await` with
  `auth = cloud::system_authenticator()` (`None` off macOS: hide the buttons). Needs a window.
- Errors: `CloudError { message, trace_id, cancelled }`; no toast when `cancelled`. Sign-up
  needs a captcha, so link `config().sign_up_url()` instead.
- Sign out: `connect.sign_out().await`, `remove_relay_environments(&mut catalog)`, save, and
  drop those environments from `AppState` (no remove method yet; branch `cloud` has one).

## Linked environments

- `connect.refresh()` (or `refresh_now().await`) fills `state.discovery`: `refreshing`,
  `error` (listing failed), `environments: [DiscoveredEnvironment]` with `availability`
  (`Checking | Online | Offline{reason} | Error(failure)`) and `compatibility_error()` for
  "Client not supported". Refresh when the list mounts, as the fork does.
- Add: `let saved = connect.saved_environment(&entry.environment)?`, upsert into the catalog,
  start it with `connect.endpoint(saved.environment_id.clone())`. Toast copy is in the fork.
- Remove from device: catalog `remove`, drop the environment. The account link stays;
  "Deregister" (relay `DELETE /v1/client/environment-links/:id`) is not implemented.

## Behavior to know

- The endpoint mints a relay credential and a one-hour DPoP token per environment, renews it
  before expiry and after a 401 (`HttpAuth::renew`), so `Session::http` keeps working.
- Signed out, relay environments block ("Sign in to T3 Connect to connect this environment.")
  until sign-in and `retry_now()`.
- Failures are `ConnectionFailure` with upstream's copy and `trace_id` ("Copy trace ID").
- t3-ui lacks server/laptop/Mac icons for the machine glyph; the snapshot harness captures
  dialogs mid fade-in (`sidebar-rename-project` shows it too).

## Verified / not

Verified without an account: live Clerk and relay public shapes (`connect_probe public`), DPoP
proofs against upstream's verifier and the web client's proof (`e2e/dpop-crosscheck.mjs`),
the whole environment half over DPoP on a throwaway nightly server incl. revoke and re-mint
(`connect_probe dpop`). Needs the user's account: real Clerk sign-in, the `t3-relay` JWT at
`/v1/client/dpop-token`, relay connect, Clerk accepting `t3code://app/` from this client.
Check: `cargo run -p t3-client --example connect_probe -- --email you@example.com`.
