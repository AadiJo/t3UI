//! The T3 Connect relay (`relay.t3.codes`): listing linked environments, DPoP access tokens,
//! status checks, and one-time environment credentials (connections.md 3.3, 3.8). Port of
//! upstream `packages/client-runtime/src/relay/managedRelay.ts` for the calls a client needs.
//!
//! The relay authenticates two ways. `GET /v1/environments` takes the Clerk `t3-relay` session
//! JWT as a bearer token. Status and connect take a relay DPoP access token (30 minutes, from
//! `POST /v1/client/dpop-token`) plus a fresh proof per request. [`RelayClient`] caches that token
//! per account and key and retries once with a new one when the relay calls it invalid.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use t3_protocol::EnvironmentId;
use url::Url;

use super::{
    dpop::DpopKey,
    jwt,
    net::{self, NetError},
};
use crate::connection::{BlockedReason, ConnectionFailure, TransientReason};

/// Upstream's request timeout for every relay call (`MANAGED_RELAY_REQUEST_TIMEOUT_MS`).
const TIMEOUT: Duration = Duration::from_secs(10);
/// Both scopes in one token, so status checks and connects share it (`discovery.ts:156-160`).
const SCOPES: &str = "environment:status environment:connect";
/// Cached tokens are reused while they have this much life left (`managedRelay.ts:380`).
const TOKEN_SKEW: Duration = Duration::from_secs(5);

/// Shown with every relay transport failure (`errors/network.ts`).
pub const NETWORK_BLOCKING_HINT: &str = "Your DNS or firewall may be blocking T3 Connect. Try another network, such as a phone hotspot.";

/// A linked environment from `GET /v1/environments` (`RelayClientEnvironmentRecord`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEnvironment {
    pub environment_id: EnvironmentId,
    pub label: String,
    pub endpoint: RelayManagedEndpoint,
    #[serde(default)]
    pub linked_at: String,
}

/// Where the environment's tunnel is (`https://<host>/`, `wss://<host>/ws`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayManagedEndpoint {
    pub http_base_url: String,
    pub ws_base_url: String,
    /// `cloudflare_tunnel`, `t3_relay`, or `manual` (publish-only; cannot be connected).
    pub provider_kind: String,
}

/// `POST /v1/environments/{id}/status`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEnvironmentStatus {
    pub environment_id: EnvironmentId,
    pub endpoint: RelayManagedEndpoint,
    /// `online` or `offline`.
    pub status: String,
    #[serde(default)]
    pub checked_at: String,
    /// The environment's descriptor when online. Kept loose: only its id is checked.
    pub descriptor: Option<Value>,
    /// Why it is offline, e.g. "Managed endpoint health request timed out."
    pub error: Option<String>,
    pub trace_id: Option<String>,
}

/// `POST /v1/environments/{id}/connect`: a one-time credential (2 minutes) bound to our key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConnectResponse {
    pub environment_id: EnvironmentId,
    pub endpoint: RelayManagedEndpoint,
    pub credential: String,
    #[serde(default)]
    pub expires_at: String,
}

/// A relay error body (`RelayProtectedError`, `relay.ts:392-609`). Unknown tags keep their tag.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayErrorBody {
    #[serde(rename = "_tag")]
    pub tag: String,
    pub code: Option<String>,
    pub reason: Option<String>,
    pub dpop_failure_reason: Option<String>,
    pub max_tunnels: Option<u64>,
    pub trace_id: Option<String>,
}

/// Why a relay call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RelayError {
    /// The request did not finish within 10 s.
    #[error("{activity} timed out. {NETWORK_BLOCKING_HINT}")]
    Timeout { activity: &'static str },
    /// DNS, TLS, or connection failure.
    #[error("Could not {action}. {NETWORK_BLOCKING_HINT}")]
    Network { action: &'static str, cause: String },
    /// The relay answered with an error.
    #[error("{}", relay_error_message(body.as_deref(), *status, action))]
    Status {
        action: &'static str,
        status: u16,
        body: Option<Box<RelayErrorBody>>,
    },
    #[error("Could not {action}.")]
    Decode { action: &'static str, cause: String },
    #[error("Relay granted unexpected DPoP access token scopes.")]
    UnexpectedScopes { granted: String },
}

impl RelayError {
    pub fn trace_id(&self) -> Option<&str> {
        match self {
            RelayError::Status { body, .. } => body.as_ref().and_then(|b| b.trace_id.as_deref()),
            _ => None,
        }
    }

    /// `RelayAuthInvalidError{reason: invalid_bearer}`: the relay no longer accepts the token.
    fn is_rejected_token(&self) -> bool {
        matches!(self, RelayError::Status { body: Some(body), .. }
            if body.tag == "RelayAuthInvalidError" && body.reason.as_deref() == Some("invalid_bearer"))
    }

    /// Connection semantics, as upstream's `mapManagedRelayError` (`connection/errors.ts:37-113`).
    pub fn connection_failure(&self) -> ConnectionFailure {
        let detail = self.to_string();
        let failure = match self {
            RelayError::Timeout { .. } => {
                ConnectionFailure::transient(TransientReason::Timeout, detail)
            }
            RelayError::Network { .. } | RelayError::Decode { .. } => {
                ConnectionFailure::transient(TransientReason::RelayUnavailable, detail)
            }
            RelayError::UnexpectedScopes { .. } => {
                ConnectionFailure::blocked(BlockedReason::Permission, detail)
            }
            RelayError::Status { body: None, .. } => {
                ConnectionFailure::transient(TransientReason::RelayUnavailable, detail)
            }
            RelayError::Status {
                body: Some(body), ..
            } => match body.tag.as_str() {
                "RelayAuthInvalidError"
                | "RelayEnvironmentLinkProofExpiredError"
                | "RelayAgentActivityPublishProofExpiredError"
                | "RelayAgentActivityPublishProofInvalidError" => {
                    ConnectionFailure::blocked(BlockedReason::Authentication, detail)
                }
                "RelayEnvironmentConnectNotAuthorizedError"
                | "RelayEnvironmentLinkProofInvalidError"
                | "RelayEnvironmentLinkLimitExceededError" => {
                    ConnectionFailure::blocked(BlockedReason::Permission, detail)
                }
                "RelayEnvironmentEndpointTimedOutError" => {
                    ConnectionFailure::transient(TransientReason::Timeout, detail)
                }
                "RelayEnvironmentEndpointUnavailableError"
                | "RelayEnvironmentLinkUnavailableError" => {
                    ConnectionFailure::transient(TransientReason::EndpointUnavailable, detail)
                }
                _ => ConnectionFailure::transient(TransientReason::RelayUnavailable, detail),
            },
        };
        failure.with_trace_id(self.trace_id().map(str::to_owned))
    }
}

/// Hint appended to DPoP failures (`relay/errorPresentation.ts:4-22`).
pub fn dpop_failure_hint(reason: Option<&str>) -> &'static str {
    match reason {
        Some("time_window") => {
            "Hint: Check that automatic date and time is enabled on both devices, then try again."
        }
        None => {
            "Hint: Try again. If it still fails, clock skew may be the cause; check that automatic date and time is enabled on both devices."
        }
        Some(_) => "Hint: Try again. If the problem continues, copy the trace ID.",
    }
}

/// `relayProtectedErrorMessage` (`relay/errorPresentation.ts:24-66`), with a fallback for
/// bodies this build does not know.
fn relay_error_message(body: Option<&RelayErrorBody>, status: u16, action: &str) -> String {
    let Some(body) = body else {
        return format!("Could not {action} (HTTP {status}).");
    };
    let reason = body.reason.as_deref();
    match body.tag.as_str() {
        "RelayAuthInvalidError" => match reason {
            Some("invalid_dpop") => format!(
                "Relay rejected the DPoP proof. {}",
                dpop_failure_hint(body.dpop_failure_reason.as_deref())
            ),
            Some("not_authorized") => "Relay rejected the authenticated request.".into(),
            _ => "Relay rejected the cloud session token.".into(),
        },
        "RelayEnvironmentConnectNotAuthorizedError" => match reason {
            Some("environment_link_not_found") => "Relay has no active link for this environment. The environment server may not have re-established its link yet.".into(),
            Some(reason) => format!("Relay rejected the environment connection request ({reason})."),
            None => "Relay rejected the environment connection request.".into(),
        },
        "RelayEnvironmentEndpointUnavailableError" => format!(
            "Relay could not reach the environment endpoint ({}).",
            reason.unwrap_or("unknown")
        ),
        "RelayEnvironmentEndpointTimedOutError" => {
            "Relay timed out while contacting the environment endpoint.".into()
        }
        "RelayEnvironmentLinkLimitExceededError" => format!(
            "Relay refused the link: this account already has its maximum of {} managed tunnels. Unlink an environment to free one up.",
            body.max_tunnels.unwrap_or_default()
        ),
        "RelayInternalError" => format!(
            "Relay encountered an internal error ({}).",
            reason.unwrap_or("internal_error")
        ),
        _ => format!("Could not {action} ({}).", body.code.as_deref().unwrap_or(&body.tag)),
    }
}

#[derive(Debug, Clone)]
struct AccessToken {
    account_id: String,
    token: String,
    expires_at: Instant,
}

/// Client for one relay. Holds the install's DPoP key and caches the relay access token.
pub struct RelayClient {
    /// The issuer (`https://relay.t3.codes`, no trailing slash). Token exchanges send it as
    /// `resource`, which must match exactly.
    issuer: String,
    client_id: &'static str,
    key: Arc<DpopKey>,
    /// Single-flight: holding the lock while exchanging keeps concurrent status checks from
    /// minting one token each.
    token: tokio::sync::Mutex<Option<AccessToken>>,
}

impl std::fmt::Debug for RelayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelayClient")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

impl RelayClient {
    /// `issuer` is the relay origin; any trailing slash is dropped. `client_id` is `t3-web`.
    pub fn new(issuer: &Url, client_id: &'static str, key: Arc<DpopKey>) -> Self {
        RelayClient {
            issuer: issuer.origin().ascii_serialization(),
            client_id,
            key,
            token: tokio::sync::Mutex::new(None),
        }
    }

    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    fn url(&self, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.issuer)).expect("relay URLs are valid")
    }

    /// Drops the cached access token (sign-out, account change).
    pub async fn reset(&self) {
        *self.token.lock().await = None;
    }

    /// `GET /v1/environments` with the Clerk session JWT.
    pub async fn list_environments(
        &self,
        clerk_jwt: &str,
    ) -> Result<Vec<RelayEnvironment>, RelayError> {
        #[derive(Deserialize)]
        struct List {
            environments: Vec<RelayEnvironment>,
        }
        let request = net::client()
            .get(self.url("/v1/environments"))
            .bearer_auth(clerk_jwt);
        let list: List = send(
            request,
            "list relay-managed environments",
            "Relay environment listing",
        )
        .await?;
        Ok(list.environments)
    }

    /// `POST /v1/environments/{id}/status`, validated against the listed record like
    /// upstream's discovery (`discovery.ts:57-93`).
    pub async fn environment_status(
        &self,
        clerk_jwt: &str,
        environment: &RelayEnvironment,
    ) -> Result<RelayEnvironmentStatus, ConnectionFailure> {
        let url = self.environment_url(&environment.environment_id, "status");
        let status: RelayEnvironmentStatus = self
            .with_access_token(clerk_jwt, |token| {
                let proof = self.key.proof("POST", &url, Some(&token));
                send(
                    net::client()
                        .post(url.clone())
                        .header(reqwest::header::AUTHORIZATION, format!("DPoP {token}"))
                        .header("DPoP", proof),
                    "get relay environment status",
                    "Relay environment status request",
                )
            })
            .await
            .map_err(|e| e.connection_failure())?;
        let mismatch = |detail: &str| {
            Err(ConnectionFailure::blocked(
                BlockedReason::Configuration,
                detail,
            ))
        };
        if status.environment_id != environment.environment_id {
            return mismatch("Relay returned status for a different environment.");
        }
        if status.endpoint != environment.endpoint {
            return mismatch("Relay returned status for a different environment endpoint.");
        }
        if let Some(descriptor) = &status.descriptor
            && descriptor.get("environmentId").and_then(Value::as_str)
                != Some(environment.environment_id.as_str())
        {
            return mismatch("Relay returned a descriptor for a different environment.");
        }
        Ok(status)
    }

    /// `POST /v1/environments/{id}/connect`: asks the environment (through the relay) for a
    /// one-time credential bound to our key's thumbprint.
    pub async fn connect_environment(
        &self,
        clerk_jwt: &str,
        environment_id: &EnvironmentId,
    ) -> Result<RelayConnectResponse, RelayError> {
        let url = self.environment_url(environment_id, "connect");
        let body = serde_json::json!({ "clientKeyThumbprint": self.key.thumbprint() });
        self.with_access_token(clerk_jwt, |token| {
            let proof = self.key.proof("POST", &url, Some(&token));
            send(
                net::client()
                    .post(url.clone())
                    .header(reqwest::header::AUTHORIZATION, format!("DPoP {token}"))
                    .header("DPoP", proof)
                    .json(&body),
                "connect relay environment",
                "Relay environment connection",
            )
        })
        .await
    }

    fn environment_url(&self, environment_id: &EnvironmentId, action: &str) -> Url {
        let mut url = self.url("/v1/environments/");
        url.path_segments_mut()
            .expect("https URLs have path segments")
            .pop_if_empty()
            .push(environment_id.as_str())
            .push(action);
        url
    }

    /// Runs `request` with a cached or fresh access token. When the relay rejects a token as
    /// invalid it is dropped and the request retried once with a new one (`managedRelay.ts:655-692`).
    async fn with_access_token<T, F, Fut>(
        &self,
        clerk_jwt: &str,
        request: F,
    ) -> Result<T, RelayError>
    where
        F: Fn(String) -> Fut,
        Fut: Future<Output = Result<T, RelayError>>,
    {
        let mut retried = false;
        loop {
            let token = self.access_token(clerk_jwt).await?;
            match request(token.clone()).await {
                Err(error) if error.is_rejected_token() => {
                    self.invalidate(&token).await;
                    if retried {
                        return Err(error);
                    }
                    tracing::warn!("relay rejected a cached DPoP access token; refreshing it once");
                    retried = true;
                }
                result => return result,
            }
        }
    }

    async fn invalidate(&self, token: &str) {
        let mut cached = self.token.lock().await;
        if cached.as_ref().is_some_and(|c| c.token == token) {
            *cached = None;
        }
    }

    /// The cached token for the Clerk JWT's account, or a new one from `/v1/client/dpop-token`.
    async fn access_token(&self, clerk_jwt: &str) -> Result<String, RelayError> {
        let account_id = jwt::subject(clerk_jwt).unwrap_or_default();
        let mut cached = self.token.lock().await;
        if let Some(token) = cached.as_ref()
            && !account_id.is_empty()
            && token.account_id == account_id
            && token.expires_at > Instant::now() + TOKEN_SKEW
        {
            return Ok(token.token.clone());
        }
        let issued = self.exchange(clerk_jwt).await?;
        let token = issued.access_token.clone();
        *cached = (!account_id.is_empty()).then(|| AccessToken {
            account_id,
            token: issued.access_token,
            expires_at: Instant::now() + Duration::from_secs(issued.expires_in),
        });
        Ok(token)
    }

    /// `POST /v1/client/dpop-token`: Clerk session JWT for a 30-minute DPoP-bound relay token.
    /// The proof carries no `ath`; the response scopes must equal the requested set.
    async fn exchange(&self, clerk_jwt: &str) -> Result<IssuedToken, RelayError> {
        let url = self.url("/v1/client/dpop-token");
        let proof = self.key.proof("POST", &url, None);
        let form = [
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:token-exchange",
            ),
            ("subject_token", clerk_jwt),
            ("subject_token_type", "urn:ietf:params:oauth:token-type:jwt"),
            (
                "requested_token_type",
                "urn:ietf:params:oauth:token-type:access_token",
            ),
            ("resource", self.issuer.as_str()),
            ("scope", SCOPES),
            ("client_id", self.client_id),
        ];
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        let issued: IssuedToken = send(
            net::client()
                .post(url)
                .header("DPoP", proof)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body),
            "exchange relay DPoP access token",
            "Relay DPoP access token exchange",
        )
        .await?;
        if !scope_set_equals(&issued.scope, SCOPES) {
            return Err(RelayError::UnexpectedScopes {
                granted: issued.scope,
            });
        }
        Ok(issued)
    }
}

#[derive(Deserialize)]
struct IssuedToken {
    access_token: String,
    expires_in: u64,
    scope: String,
}

/// RFC 6749 scope strings compared as sets (`oauthScopeSetEquals`).
fn scope_set_equals(granted: &str, requested: &str) -> bool {
    let mut granted: Vec<&str> = granted.split(' ').collect();
    let mut requested: Vec<&str> = requested.split(' ').collect();
    if granted.iter().any(|scope| scope.is_empty()) {
        return false;
    }
    granted.sort_unstable();
    granted.dedup();
    requested.sort_unstable();
    granted == requested
}

async fn send<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    action: &'static str,
    activity: &'static str,
) -> Result<T, RelayError> {
    let reply = net::send(request, TIMEOUT)
        .await
        .map_err(|error| match error {
            NetError::Timeout => RelayError::Timeout { activity },
            NetError::Network(cause) => RelayError::Network { action, cause },
        })?;
    if !(200..300).contains(&reply.status) {
        return Err(RelayError::Status {
            action,
            status: reply.status,
            body: serde_json::from_slice(&reply.body).ok().map(Box::new),
        });
    }
    serde_json::from_slice(&reply.body).map_err(|e| RelayError::Decode {
        action,
        cause: e.to_string(),
    })
}

// Failure modes:
//  1. Unknown `_tag`s or missing optional fields in error bodies (older/newer relays) must still
//     produce a message and a retry class, never a decode failure.
//  2. `invalid_bearer` must be retryable once with a fresh token; `invalid_dpop` must not.
//  3. The DPoP hint must distinguish clock skew from other proof failures.
//  4. Granted scopes compared as an ordered string would reject a reordered but equal set.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::FailureKind;

    fn status_error(body: &str, status: u16) -> RelayError {
        RelayError::Status {
            action: "connect relay environment",
            status,
            body: serde_json::from_str(body).ok().map(Box::new),
        }
    }

    #[test]
    fn recorded_unauthenticated_listing() {
        // GET https://relay.t3.codes/v1/environments without a token (verified live).
        let error = status_error(
            r#"{"_tag":"RelayAuthInvalidError","code":"auth_invalid","reason":"invalid_bearer","traceId":"abc"}"#,
            401,
        );
        assert!(error.is_rejected_token());
        assert_eq!(error.to_string(), "Relay rejected the cloud session token.");
        let failure = error.connection_failure();
        assert_eq!(
            failure.kind,
            FailureKind::Blocked(BlockedReason::Authentication)
        );
        assert_eq!(failure.trace_id.as_deref(), Some("abc"));
    }

    #[test]
    fn dpop_and_connect_errors() {
        let skew = status_error(
            r#"{"_tag":"RelayAuthInvalidError","code":"auth_invalid","reason":"invalid_dpop","dpopFailureReason":"time_window","traceId":"t"}"#,
            401,
        );
        assert!(!skew.is_rejected_token());
        assert!(skew.to_string().contains("automatic date and time"));

        let missing_link = status_error(
            r#"{"_tag":"RelayEnvironmentConnectNotAuthorizedError","code":"environment_connect_not_authorized","reason":"environment_link_not_found","traceId":"t"}"#,
            403,
        );
        assert!(
            missing_link
                .to_string()
                .starts_with("Relay has no active link")
        );
        assert_eq!(
            missing_link.connection_failure().kind,
            FailureKind::Blocked(BlockedReason::Permission)
        );

        let unreachable = status_error(
            r#"{"_tag":"RelayEnvironmentEndpointUnavailableError","code":"environment_endpoint_unavailable","reason":"endpoint_request_failed","traceId":"t"}"#,
            502,
        );
        assert_eq!(
            unreachable.connection_failure().kind,
            FailureKind::Transient(TransientReason::EndpointUnavailable)
        );

        let future = status_error(
            r#"{"_tag":"RelayShinyNewError","code":"shiny","traceId":"t"}"#,
            418,
        );
        assert_eq!(
            future.to_string(),
            "Could not connect relay environment (shiny)."
        );
        assert!(!future.connection_failure().is_blocked());

        let html = status_error("<html>", 502);
        assert_eq!(
            html.to_string(),
            "Could not connect relay environment (HTTP 502)."
        );
    }

    #[test]
    fn transport_messages_carry_the_network_hint() {
        let timeout = RelayError::Timeout {
            activity: "Relay environment connection",
        };
        assert_eq!(
            timeout.to_string(),
            format!("Relay environment connection timed out. {NETWORK_BLOCKING_HINT}")
        );
    }

    #[test]
    fn scopes_compare_as_sets() {
        assert!(scope_set_equals(
            "environment:connect environment:status",
            SCOPES
        ));
        assert!(scope_set_equals(SCOPES, SCOPES));
        assert!(!scope_set_equals("environment:connect", SCOPES));
        assert!(!scope_set_equals(
            "environment:connect  environment:status",
            SCOPES
        ));
        assert!(!scope_set_equals(
            "environment:connect environment:status mobile:registration",
            SCOPES
        ));
    }

    #[test]
    fn records_decode_with_unknown_fields() {
        let list: RelayEnvironment = serde_json::from_str(
            r#"{"environmentId":"env-1","label":"devbox","linkedAt":"2026-10-01T00:00:00Z","new":true,
                "endpoint":{"httpBaseUrl":"https://prod-a.example/","wsBaseUrl":"wss://prod-a.example/ws","providerKind":"cloudflare_tunnel"}}"#,
        )
        .unwrap();
        assert_eq!(list.label, "devbox");
        let key = Arc::new(DpopKey::generate());
        let relay = RelayClient::new(
            &Url::parse("https://relay.t3.codes/").unwrap(),
            "t3-web",
            key,
        );
        assert_eq!(relay.issuer(), "https://relay.t3.codes");
        assert_eq!(
            relay
                .environment_url(&list.environment_id, "connect")
                .as_str(),
            "https://relay.t3.codes/v1/environments/env-1/connect"
        );
    }
}
