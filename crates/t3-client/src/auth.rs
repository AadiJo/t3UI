//! Pairing and per-attempt connection preparation (connections.md 2.3, 2.4, 7.5).
//!
//! - [`pair`]: pairing credential to a saved bearer environment.
//! - [`Endpoint`]: what one connection attempt needs (descriptor check, ticket, WS URL).
//!   [`BearerEndpoint`] covers paired environments; T3 Connect provides its own.

use std::{future::Future, pin::Pin, sync::Arc};

use t3_protocol::{
    EnvironmentId,
    environment::{ExecutionEnvironmentDescriptor, ORCHESTRATION_PROTOCOL_VERSION},
};
use url::Url;

use crate::{
    connection::{BlockedReason, ConnectionFailure},
    http::{BearerAuth, EnvironmentHttp, HttpAuth},
    pairing::PairingTarget,
    store::{KnownTarget, SavedEnvironment, SavedTarget, SecretStore, StoreError},
};

/// How this client describes itself to servers (pairing labels, WS query params).
#[derive(Debug, Clone)]
pub struct ClientInfo {
    /// Shown in the host's "Authorized clients" list.
    pub label: String,
    pub app_version: String,
    /// `desktop`.
    pub device_type: &'static str,
    /// `macOS`, `Linux`, or `Windows`.
    pub os: &'static str,
}

impl Default for ClientInfo {
    fn default() -> Self {
        ClientInfo {
            label: "T3UI".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            device_type: "desktop",
            os: if cfg!(target_os = "macos") {
                "macOS"
            } else if cfg!(target_os = "windows") {
                "Windows"
            } else if cfg!(target_os = "linux") {
                "Linux"
            } else {
                "other"
            },
        }
    }
}

/// How the socket reaches the server, reported in `connectionMethod`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionMethod {
    Direct,
    Ssh,
    Relay,
}

impl ConnectionMethod {
    fn as_str(self) -> &'static str {
        match self {
            ConnectionMethod::Direct => "direct",
            ConnectionMethod::Ssh => "ssh",
            ConnectionMethod::Relay => "relay",
        }
    }
}

/// Refuses descriptors for another environment or another protocol version
/// (`connection/compatibility.ts:9-24`, `errors.ts:27-35`).
pub fn check_descriptor(
    descriptor: &ExecutionEnvironmentDescriptor,
    expected: Option<&EnvironmentId>,
) -> Result<(), ConnectionFailure> {
    if let Some(expected) = expected
        && &descriptor.environment_id != expected
    {
        return Err(ConnectionFailure::blocked(
            BlockedReason::Configuration,
            format!(
                "Connected environment {} does not match {expected}.",
                descriptor.environment_id
            ),
        ));
    }
    let version = descriptor.protocol_version();
    if version > ORCHESTRATION_PROTOCOL_VERSION {
        return Err(ConnectionFailure::blocked(
            BlockedReason::Unsupported,
            format!(
                "This client is not supported by this server. Update your app or use a compatible release to connect to {}.",
                descriptor.label
            ),
        ));
    }
    if version < ORCHESTRATION_PROTOCOL_VERSION {
        return Err(ConnectionFailure::blocked(
            BlockedReason::Unsupported,
            format!(
                "This client requires a newer server. Update T3 Code on {} to connect.",
                descriptor.label
            ),
        ));
    }
    Ok(())
}

/// The WebSocket URL for one attempt: `{ws_base}ws?wsTicket=..&client params&orchestrationProtocol=1`.
/// A root `ws_base` gets `/ws`; a non-root path (relay endpoints) is kept.
pub fn websocket_url(
    ws_base: &Url,
    ticket: &str,
    client: &ClientInfo,
    method: ConnectionMethod,
) -> Url {
    let mut url = ws_base.clone();
    if url.path().is_empty() || url.path() == "/" {
        url.set_path("/ws");
    }
    url.set_fragment(None);
    url.query_pairs_mut()
        .clear()
        .append_pair("wsTicket", ticket)
        .append_pair("clientSurface", "desktop")
        .append_pair("clientAppVersion", &client.app_version)
        .append_pair("clientDeviceType", client.device_type)
        .append_pair("clientOs", client.os)
        .append_pair("connectionMethod", method.as_str())
        .append_pair("orchestrationProtocol", "1");
    url
}

/// The result of pairing: everything needed to save and connect the environment.
#[derive(Debug, Clone)]
pub struct PairedEnvironment {
    pub descriptor: ExecutionEnvironmentDescriptor,
    pub http_base: Url,
    pub ws_base: Url,
    /// 30-day bearer token. Store it with [`SecretStore`](crate::store::SecretStore) under
    /// [`secret_key`](Self::secret_key), never in the catalog.
    pub bearer_token: String,
    pub scopes: Vec<String>,
}

impl PairedEnvironment {
    pub fn connection_id(&self) -> String {
        format!("bearer:{}", self.descriptor.environment_id)
    }

    /// Key of the bearer token in the secret store.
    pub fn secret_key(&self) -> String {
        self.connection_id()
    }

    /// The catalog entry to persist (no secrets).
    pub fn saved(&self) -> SavedEnvironment {
        SavedEnvironment::new(
            self.descriptor.environment_id.clone(),
            self.descriptor.label.clone(),
            SavedTarget::Known(KnownTarget::Bearer {
                connection_id: self.connection_id(),
                http_base_url: self.http_base.clone(),
                ws_base_url: self.ws_base.clone(),
            }),
        )
    }

    pub fn endpoint(&self) -> Arc<dyn Endpoint> {
        Arc::new(BearerEndpoint::new(
            self.http_base.clone(),
            self.ws_base.clone(),
            self.bearer_token.clone(),
        ))
    }
}

/// Pairs with an environment: descriptor and protocol check, then `POST /oauth/token`.
/// Pairing credentials are single use and expire after 5 minutes; a used or expired one fails
/// with "The environment credential is invalid."
pub async fn pair(
    target: &PairingTarget,
    client: &ClientInfo,
) -> Result<PairedEnvironment, ConnectionFailure> {
    let http = EnvironmentHttp::new(target.http_base.clone(), None);
    let descriptor = http
        .descriptor()
        .await
        .map_err(|e| e.connection_failure())?;
    check_descriptor(&descriptor, None)?;
    let token = http
        .exchange_pairing_credential(&target.credential, client)
        .await
        .map_err(|e| e.connection_failure())?;
    Ok(PairedEnvironment {
        descriptor,
        http_base: target.http_base.clone(),
        ws_base: target.ws_base.clone(),
        bearer_token: token.access_token,
        scopes: token
            .scope
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
    })
}

/// What one connection attempt uses.
#[derive(Debug, Clone)]
pub struct PreparedConnection {
    pub descriptor: ExecutionEnvironmentDescriptor,
    /// Authenticated HTTP client for snapshots, uploads, assets.
    pub http: EnvironmentHttp,
    /// The WebSocket URL including a fresh ticket.
    pub ws_url: Url,
}

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

/// Prepares connection attempts for one environment. Called once per attempt on the
/// networking runtime; implementations fetch fresh tickets each time.
pub trait Endpoint: Send + Sync + 'static {
    fn prepare(
        &self,
        expected: EnvironmentId,
        client: ClientInfo,
    ) -> BoxFuture<Result<PreparedConnection, ConnectionFailure>>;

    /// Base URL shown in the UI (`None` for relay environments).
    fn display_url(&self) -> Option<Url>;
}

/// The endpoint for a saved bearer environment, with its token from `secrets`. `Ok(None)` for
/// other target kinds (T3 Connect builds relay endpoints) or when the token is missing (the
/// user has to pair again).
pub fn saved_bearer_endpoint(
    saved: &SavedEnvironment,
    secrets: &dyn SecretStore,
) -> Result<Option<Arc<dyn Endpoint>>, StoreError> {
    let SavedTarget::Known(KnownTarget::Bearer {
        connection_id,
        http_base_url,
        ws_base_url,
    }) = &saved.target
    else {
        return Ok(None);
    };
    Ok(secrets.get(connection_id)?.map(|token| {
        Arc::new(BearerEndpoint::new(
            http_base_url.clone(),
            ws_base_url.clone(),
            token,
        )) as Arc<dyn Endpoint>
    }))
}

/// A directly paired environment authenticated with a bearer token.
pub struct BearerEndpoint {
    http_base: Url,
    ws_base: Url,
    auth: Arc<dyn HttpAuth>,
}

impl BearerEndpoint {
    pub fn new(http_base: Url, ws_base: Url, token: String) -> Self {
        BearerEndpoint {
            http_base,
            ws_base,
            auth: Arc::new(BearerAuth(token)),
        }
    }
}

impl Endpoint for BearerEndpoint {
    fn prepare(
        &self,
        expected: EnvironmentId,
        client: ClientInfo,
    ) -> BoxFuture<Result<PreparedConnection, ConnectionFailure>> {
        let http = EnvironmentHttp::new(self.http_base.clone(), Some(self.auth.clone()));
        let ws_base = self.ws_base.clone();
        Box::pin(async move {
            let descriptor = http
                .descriptor()
                .await
                .map_err(|e| e.connection_failure())?;
            check_descriptor(&descriptor, Some(&expected))?;
            let ticket = http
                .websocket_ticket()
                .await
                .map_err(|e| e.connection_failure())?;
            let ws_url = websocket_url(&ws_base, &ticket.ticket, &client, ConnectionMethod::Direct);
            Ok(PreparedConnection {
                descriptor,
                http,
                ws_url,
            })
        })
    }

    fn display_url(&self) -> Option<Url> {
        Some(self.http_base.clone())
    }
}
