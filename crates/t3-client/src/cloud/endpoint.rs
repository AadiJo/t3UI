//! [`DpopEndpoint`]: reaching an environment with a DPoP-bound access token, the way T3 Connect
//! environments are reached (connections.md 3.5, upstream `authorization/service.ts:251-493`).
//!
//! Minting a token: get a one-time bootstrap credential bound to our key from a
//! [`BootstrapSource`] (the relay, for T3 Connect), check the environment's descriptor, then
//! `POST /oauth/token` with a DPoP proof. The token lasts an hour and is kept in memory. Every
//! request presents it as `Authorization: DPoP <token>` with a fresh proof.
//!
//! Per attempt ([`Endpoint::prepare`]): reuse the cached token if it has more than a minute
//! left, fetch a WebSocket ticket with it (3 s budget), and re-mint once if that fails. HTTP
//! requests renew an expiring token first and re-mint once after a 401, through
//! [`HttpAuth::renew`]. One mint runs at a time per environment, bounded by 30 s.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use reqwest::{Method, header::HeaderMap};
use t3_protocol::{EnvironmentId, environment::ExecutionEnvironmentDescriptor};
use url::Url;

use super::{
    dpop::DpopKey,
    relay::{NETWORK_BLOCKING_HINT, dpop_failure_hint},
};
use crate::{
    auth::{
        BoxFuture, ClientInfo, ConnectionMethod, Endpoint, PreparedConnection, check_descriptor,
        websocket_url,
    },
    connection::{BlockedReason, ConnectionFailure, TransientReason},
    http::{EnvironmentHttp, HttpAuth, HttpError},
};

/// Tokens are renewed when they have less than this left (`DPOP_ACCESS_TOKEN_REFRESH_SKEW_MS`).
const REFRESH_SKEW: Duration = Duration::from_secs(60);
/// Bound on one whole mint (`DPOP_AUTHORIZATION_TIMEOUT_MS`).
const MINT_TIMEOUT: Duration = Duration::from_secs(30);
/// Budget for a ticket with a cached token before re-minting (`CACHED_ENDPOINT_SOCKET_TIMEOUT_MS`).
const CACHED_TICKET_TIMEOUT: Duration = Duration::from_secs(3);
/// Assumed lifetime when the server omits `expires_in` (environment DPoP tokens last an hour).
const DEFAULT_TOKEN_LIFETIME: Duration = Duration::from_secs(3600);

/// A one-time credential and where to redeem it.
#[derive(Debug, Clone)]
pub struct Bootstrap {
    pub http_base: Url,
    pub ws_base: Url,
    /// Single use and short-lived. The relay's are bound to our key's thumbprint; a plain
    /// pairing credential is unbound, and the environment binds the token to our key either way.
    pub credential: String,
}

/// Where bootstrap credentials come from. T3 Connect asks the relay; the end-to-end probe passes
/// a pairing credential from a throwaway server.
pub trait BootstrapSource: Send + Sync + 'static {
    /// Who tokens are minted for (the signed-in account). A cached token minted for someone
    /// else is dropped; `None` means nobody is signed in and nothing can be minted.
    fn identity(&self) -> Option<String>;

    /// A fresh credential for `environment_id`.
    fn bootstrap(
        &self,
        environment_id: EnvironmentId,
    ) -> BoxFuture<Result<Bootstrap, ConnectionFailure>>;
}

/// The [`Endpoint`] of an environment reached with DPoP tokens. Build one per environment and
/// pass it to [`EnvironmentOptions`](crate::EnvironmentOptions).
pub struct DpopEndpoint {
    inner: Arc<Inner>,
}

struct Inner {
    environment_id: EnvironmentId,
    key: Arc<DpopKey>,
    source: Arc<dyn BootstrapSource>,
    /// Labels the session in the host's "Authorized clients" list.
    client: ClientInfo,
    token: Mutex<Option<EnvToken>>,
    mint: tokio::sync::Mutex<()>,
}

#[derive(Debug, Clone)]
struct EnvToken {
    identity: String,
    http_base: Url,
    ws_base: Url,
    access_token: String,
    expires_at: Instant,
}

impl DpopEndpoint {
    pub fn new(
        environment_id: EnvironmentId,
        key: Arc<DpopKey>,
        source: Arc<dyn BootstrapSource>,
        client: ClientInfo,
    ) -> Self {
        DpopEndpoint {
            inner: Arc::new(Inner {
                environment_id,
                key,
                source,
                client,
                token: Mutex::new(None),
                mint: tokio::sync::Mutex::new(()),
            }),
        }
    }
}

impl Endpoint for DpopEndpoint {
    fn prepare(
        &self,
        expected: EnvironmentId,
        client: ClientInfo,
    ) -> BoxFuture<Result<PreparedConnection, ConnectionFailure>> {
        let inner = self.inner.clone();
        Box::pin(async move {
            if expected != inner.environment_id {
                return Err(ConnectionFailure::blocked(
                    BlockedReason::Configuration,
                    format!(
                        "Connected environment {} does not match {expected}.",
                        inner.environment_id
                    ),
                ));
            }
            let (mut token, mut descriptor) = inner.token(None).await?;
            let mut http = inner.http(&token);
            let ticket = if descriptor.is_none() {
                // Cached token: a quick ticket proves it still works; otherwise re-mint once.
                match tokio::time::timeout(CACHED_TICKET_TIMEOUT, http.websocket_ticket()).await {
                    Ok(Ok(ticket)) => Ok(ticket),
                    _ => {
                        (token, descriptor) = inner.token(Some(&token.access_token)).await?;
                        http = inner.http(&token);
                        http.websocket_ticket().await
                    }
                }
            } else {
                http.websocket_ticket().await
            };
            let ticket = match ticket {
                Ok(ticket) => ticket,
                Err(error) => {
                    inner.drop_token(&token.access_token);
                    return Err(dpop_http_failure(&error));
                }
            };
            let descriptor = match descriptor {
                Some(descriptor) => descriptor,
                None => {
                    let descriptor = http
                        .descriptor()
                        .await
                        .map_err(|e| relay_http_failure(&e))?;
                    check_descriptor(&descriptor, Some(&inner.environment_id))?;
                    descriptor
                }
            };
            let ws_url = websocket_url(
                &token.ws_base,
                &ticket.ticket,
                &client,
                ConnectionMethod::Relay,
            );
            Ok(PreparedConnection {
                descriptor,
                http,
                ws_url,
            })
        })
    }

    fn display_url(&self) -> Option<Url> {
        None
    }
}

impl Inner {
    /// An HTTP client for the token's endpoint whose auth always presents the latest token.
    fn http(self: &Arc<Self>, token: &EnvToken) -> EnvironmentHttp {
        EnvironmentHttp::new(
            token.http_base.clone(),
            Some(Arc::new(DpopAuth(self.clone()))),
        )
    }

    /// The cached token if it is for `identity` and has a minute left.
    fn cached(&self, identity: &str) -> Option<EnvToken> {
        self.token
            .lock()
            .clone()
            .filter(|t| t.identity == identity && t.expires_at > Instant::now() + REFRESH_SKEW)
    }

    fn drop_token(&self, access_token: &str) {
        let mut token = self.token.lock();
        if token
            .as_ref()
            .is_some_and(|t| t.access_token == access_token)
        {
            *token = None;
        }
    }

    /// A usable token: the cached one unless it is `rejected`, else a new one. A freshly minted
    /// token comes with the descriptor checked while minting.
    async fn token(
        self: &Arc<Self>,
        rejected: Option<&str>,
    ) -> Result<(EnvToken, Option<ExecutionEnvironmentDescriptor>), ConnectionFailure> {
        let identity = self.source.identity().ok_or_else(signed_out)?;
        let usable = |token: &EnvToken| Some(token.access_token.as_str()) != rejected;
        if let Some(token) = self.cached(&identity).filter(usable) {
            return Ok((token, None));
        }
        let _mint = self.mint.lock().await;
        // Another request may have minted while we waited.
        if let Some(token) = self.cached(&identity).filter(usable) {
            return Ok((token, None));
        }
        if let Some(rejected) = rejected {
            self.drop_token(rejected);
        }
        let (token, descriptor) = tokio::time::timeout(MINT_TIMEOUT, self.mint(identity))
            .await
            .map_err(|_| {
                ConnectionFailure::transient(
                    TransientReason::Timeout,
                    "Timed out renewing the environment credential.",
                )
            })??;
        *self.token.lock() = Some(token.clone());
        Ok((token, Some(descriptor)))
    }

    async fn mint(
        &self,
        identity: String,
    ) -> Result<(EnvToken, ExecutionEnvironmentDescriptor), ConnectionFailure> {
        let bootstrap = self.source.bootstrap(self.environment_id.clone()).await?;
        let http = EnvironmentHttp::new(
            bootstrap.http_base.clone(),
            Some(Arc::new(ProofOnly(self.key.clone()))),
        );
        let descriptor = http
            .descriptor()
            .await
            .map_err(|e| relay_http_failure(&e))?;
        check_descriptor(&descriptor, Some(&self.environment_id))?;
        let issued = http
            .exchange_bootstrap_credential(&bootstrap.credential, &self.client)
            .await
            .map_err(|e| dpop_http_failure(&e))?;
        if !issued.token_type.eq_ignore_ascii_case("DPoP") {
            return Err(ConnectionFailure::blocked(
                BlockedReason::Configuration,
                "The environment did not bind its credential to this device.",
            ));
        }
        if self.source.identity().as_deref() != Some(identity.as_str()) {
            return Err(ConnectionFailure::blocked(
                BlockedReason::Authentication,
                "Your cloud sign-in changed. Sign in again to authorize the environment.",
            ));
        }
        let lifetime = issued
            .expires_in
            .map(Duration::from_secs)
            .unwrap_or(DEFAULT_TOKEN_LIFETIME);
        Ok((
            EnvToken {
                identity,
                http_base: bootstrap.http_base,
                ws_base: bootstrap.ws_base,
                access_token: issued.access_token,
                expires_at: Instant::now() + lifetime,
            },
            descriptor,
        ))
    }
}

/// Presents the current token with a fresh proof per request.
struct DpopAuth(Arc<Inner>);

impl HttpAuth for DpopAuth {
    fn authorize(&self, method: &Method, url: &Url, headers: &mut HeaderMap) -> Result<(), String> {
        let token = self
            .0
            .token
            .lock()
            .clone()
            .ok_or_else(|| signed_out().detail)?;
        let proof = self
            .0
            .key
            .proof(method.as_str(), url, Some(&token.access_token));
        insert(
            headers,
            reqwest::header::AUTHORIZATION,
            format!("DPoP {}", token.access_token),
        )?;
        insert(
            headers,
            reqwest::header::HeaderName::from_static("dpop"),
            proof,
        )
    }

    fn renew(&self, rejected: bool) -> BoxFuture<Result<bool, String>> {
        let inner = self.0.clone();
        Box::pin(async move {
            let current = inner.token.lock().clone();
            let rejected_token = current
                .as_ref()
                .filter(|_| rejected)
                .map(|t| t.access_token.clone());
            if !rejected
                && let Some(identity) = inner.source.identity()
                && inner.cached(&identity).is_some()
            {
                return Ok(false);
            }
            if rejected && rejected_token.is_none() {
                return Ok(false);
            }
            inner
                .token(rejected_token.as_deref())
                .await
                .map(|(_, minted)| minted.is_some())
                .map_err(|failure| failure.detail)
        })
    }
}

/// Adds only a proof without `ath`, for redeeming a bootstrap credential at `/oauth/token`.
struct ProofOnly(Arc<DpopKey>);

impl HttpAuth for ProofOnly {
    fn authorize(&self, method: &Method, url: &Url, headers: &mut HeaderMap) -> Result<(), String> {
        insert(
            headers,
            reqwest::header::HeaderName::from_static("dpop"),
            self.0.proof(method.as_str(), url, None),
        )
    }
}

fn insert(
    headers: &mut HeaderMap,
    name: reqwest::header::HeaderName,
    value: String,
) -> Result<(), String> {
    let value = value
        .parse()
        .map_err(|_| "credential is not a valid header value".to_owned())?;
    headers.insert(name, value);
    Ok(())
}

fn signed_out() -> ConnectionFailure {
    ConnectionFailure::blocked(
        BlockedReason::Authentication,
        "Sign in to T3 Connect to connect this environment.",
    )
}

/// An environment HTTP failure on a relay connection (`mapRemoteEnvironmentError(.., "relay")`):
/// transport failures carry the network hint.
fn relay_http_failure(error: &HttpError) -> ConnectionFailure {
    let mut failure = error.connection_failure();
    if matches!(error, HttpError::Network(_) | HttpError::Timeout) {
        failure.detail = format!("{} {NETWORK_BLOCKING_HINT}", failure.detail);
    }
    failure
}

/// A failure of a DPoP-authenticated request (`mapRemoteDpopEnvironmentError`): an invalid
/// credential gets a hint about what DPoP check failed.
fn dpop_http_failure(error: &HttpError) -> ConnectionFailure {
    if let HttpError::Status {
        error: Some(body), ..
    } = error
        && body.tag == "EnvironmentAuthInvalidError"
        && body.reason.as_deref() == Some("invalid_credential")
    {
        return ConnectionFailure::blocked(
            BlockedReason::Authentication,
            format!(
                "The environment credential is invalid. {}",
                dpop_failure_hint(body.dpop_failure_reason.as_deref())
            ),
        )
        .with_trace_id(body.trace_id.clone());
    }
    relay_http_failure(error)
}
