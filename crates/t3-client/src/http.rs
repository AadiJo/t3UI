//! HTTP requests to an environment: descriptor, pairing exchange, WebSocket tickets, session
//! check, snapshot fast paths, attachment uploads (protocol.md 3).
//!
//! Every method runs on the networking runtime, so the returned futures can be awaited from
//! any executor (GPUI included). Credentials come from an [`HttpAuth`], which lets T3 Connect
//! plug in DPoP without touching this module.

use std::{sync::Arc, sync::OnceLock, time::Duration};

use reqwest::{Method, header::HeaderMap};
use serde::de::DeserializeOwned;
use t3_protocol::{
    ThreadId,
    environment::{
        AccessTokenResult, AuthSessionState, EnvironmentHttpError, ExecutionEnvironmentDescriptor,
        TokenExchangeRequest, WebSocketTicket,
    },
    orchestration::{OrchestrationShellSnapshot, OrchestrationThreadDetailSnapshot},
};
use url::Url;

use crate::{
    auth::ClientInfo,
    connection::{BlockedReason, ConnectionFailure, TransientReason},
};

type ResponseTap = Box<dyn Fn(&Method, &Url, u16, &[u8]) + Send + Sync>;
static RESPONSE_TAP: OnceLock<ResponseTap> = OnceLock::new();

/// Installs a process-wide observer for every HTTP response (method, URL, status, body). For
/// debugging and recording golden transcripts; responses include credentials (`/oauth/token`,
/// tickets), so filter by path before persisting anything.
pub fn set_response_tap(tap: impl Fn(&Method, &Url, u16, &[u8]) + Send + Sync + 'static) {
    let _ = RESPONSE_TAP.set(Box::new(tap));
}

const DESCRIPTOR_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(15);
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(20);
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Adds credentials to requests. Implemented by [`BearerAuth`] here and by the DPoP auth of
/// T3 Connect environments.
pub trait HttpAuth: Send + Sync + 'static {
    /// Adds auth headers for one request. DPoP implementations sign `method` and `url`, so
    /// this is called once per request and never reused.
    fn authorize(&self, method: &Method, url: &Url, headers: &mut HeaderMap)
    -> Result<(), String>;
}

/// `Authorization: Bearer <token>` from pairing.
pub struct BearerAuth(pub String);

impl HttpAuth for BearerAuth {
    fn authorize(
        &self,
        _method: &Method,
        _url: &Url,
        headers: &mut HeaderMap,
    ) -> Result<(), String> {
        let value = format!("Bearer {}", self.0)
            .parse()
            .map_err(|_| "bearer token is not a valid header value".to_owned())?;
        headers.insert(reqwest::header::AUTHORIZATION, value);
        Ok(())
    }
}

/// Why an HTTP call failed.
#[derive(Debug, Clone, thiserror::Error)]
pub enum HttpError {
    #[error("the request timed out")]
    Timeout,
    #[error("{0}")]
    Network(String),
    /// A non-success status. `error` is the server's tagged error body when it sent one.
    #[error("HTTP {status}")]
    Status {
        status: u16,
        error: Option<Box<EnvironmentHttpError>>,
        body: String,
    },
    #[error("invalid response: {0}")]
    Decode(String),
    #[error("{0}")]
    Auth(String),
}

impl HttpError {
    pub fn status(&self) -> Option<u16> {
        match self {
            HttpError::Status { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Maps the failure to connection semantics, like upstream's `mapRemoteEnvironmentError`
    /// (`connection/errors.ts:115-172`).
    pub fn connection_failure(&self) -> ConnectionFailure {
        match self {
            HttpError::Timeout => {
                ConnectionFailure::transient(TransientReason::Timeout, "The request timed out.")
            }
            HttpError::Network(message) => {
                ConnectionFailure::transient(TransientReason::Network, message.clone())
            }
            HttpError::Decode(message) => {
                ConnectionFailure::transient(TransientReason::RemoteUnavailable, message.clone())
            }
            HttpError::Auth(message) => {
                ConnectionFailure::blocked(BlockedReason::Authentication, message.clone())
            }
            HttpError::Status { status, error, .. } => {
                let trace_id = error.as_ref().and_then(|e| e.trace_id.clone());
                let tag = error.as_ref().map(|e| e.tag.as_str());
                let failure = match tag {
                    Some("EnvironmentAuthInvalidError") => ConnectionFailure::blocked(
                        BlockedReason::Authentication,
                        "The environment credential is invalid.",
                    ),
                    Some("EnvironmentScopeRequiredError" | "EnvironmentOperationForbiddenError") => {
                        ConnectionFailure::blocked(
                            BlockedReason::Permission,
                            "The environment credential does not grant the required access.",
                        )
                    }
                    Some("EnvironmentRequestInvalidError") => ConnectionFailure::blocked(
                        BlockedReason::Configuration,
                        "The environment rejected the authentication request.",
                    ),
                    Some("EnvironmentResourceNotFoundError") => ConnectionFailure::blocked(
                        BlockedReason::Configuration,
                        "The environment endpoint could not be found.",
                    ),
                    Some("EnvironmentInternalError") => ConnectionFailure::transient(
                        TransientReason::RemoteUnavailable,
                        "The environment could not authorize the connection.",
                    ),
                    _ => ConnectionFailure::transient(
                        TransientReason::RemoteUnavailable,
                        format!("The environment responded with HTTP {status}."),
                    ),
                };
                failure.with_trace_id(trace_id)
            }
        }
    }
}

/// Options for `GET /api/orchestration/threads/:id`. Only set what `ServerConfig` advertises.
#[derive(Debug, Clone, Default)]
pub struct ThreadSnapshotQuery {
    pub turn_limit: Option<u32>,
    pub reasoning_messages: bool,
    pub before_cursor: Option<String>,
}

/// HTTP client for one environment. Cheap to clone.
#[derive(Clone)]
pub struct EnvironmentHttp {
    base: Url,
    auth: Option<Arc<dyn HttpAuth>>,
}

impl std::fmt::Debug for EnvironmentHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvironmentHttp")
            .field("base", &self.base.as_str())
            .field("authenticated", &self.auth.is_some())
            .finish()
    }
}

enum Body {
    Empty,
    Form(String),
    Bytes {
        content_type: String,
        bytes: Vec<u8>,
    },
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("T3UI/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build the http client")
    })
}

impl EnvironmentHttp {
    /// `base` is the environment's `http(s)://host[:port]/`. Paths are resolved from the
    /// origin; any path prefix on `base` is ignored, as upstream does.
    pub fn new(base: Url, auth: Option<Arc<dyn HttpAuth>>) -> Self {
        EnvironmentHttp { base, auth }
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Resolves a server-relative URL such as an asset's `relative_url`.
    pub fn resolve(&self, relative: &str) -> Url {
        self.base.join(relative).unwrap_or_else(|_| self.base.clone())
    }

    /// `GET /.well-known/t3/environment` (no auth).
    pub async fn descriptor(&self) -> Result<ExecutionEnvironmentDescriptor, HttpError> {
        self.json(
            Method::GET,
            self.resolve("/.well-known/t3/environment"),
            Body::Empty,
            false,
            DESCRIPTOR_TIMEOUT,
        )
        .await
    }

    /// `POST /oauth/token`: trades a one-time pairing credential for a 30-day bearer token
    /// with the standard client scopes.
    pub async fn exchange_pairing_credential(
        &self,
        credential: &str,
        client: &ClientInfo,
    ) -> Result<AccessTokenResult, HttpError> {
        let request = TokenExchangeRequest {
            client_label: Some(&client.label),
            client_device_type: Some(client.device_type),
            client_os: Some(client.os),
            ..TokenExchangeRequest::pairing(credential)
        };
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", request.grant_type)
            .append_pair("subject_token", request.subject_token)
            .append_pair("subject_token_type", request.subject_token_type)
            .append_pair("requested_token_type", request.requested_token_type);
        for (key, value) in [
            ("scope", request.scope),
            ("client_label", request.client_label),
            ("client_device_type", request.client_device_type),
            ("client_os", request.client_os),
        ] {
            if let Some(value) = value {
                form.append_pair(key, value);
            }
        }
        self.json(
            Method::POST,
            self.resolve("/oauth/token"),
            Body::Form(form.finish()),
            false,
            AUTH_TIMEOUT,
        )
        .await
    }

    /// `POST /api/auth/websocket-ticket`. Five-minute TTL; fetch one per connect attempt.
    pub async fn websocket_ticket(&self) -> Result<WebSocketTicket, HttpError> {
        self.json(
            Method::POST,
            self.resolve("/api/auth/websocket-ticket"),
            Body::Empty,
            true,
            AUTH_TIMEOUT,
        )
        .await
    }

    /// `GET /api/auth/session`: whether the credential is still valid, and its scopes.
    pub async fn session(&self) -> Result<AuthSessionState, HttpError> {
        self.json(
            Method::GET,
            self.resolve("/api/auth/session"),
            Body::Empty,
            true,
            AUTH_TIMEOUT,
        )
        .await
    }

    /// `GET /api/orchestration/shell`: the fast initial shell load.
    pub async fn shell_snapshot(&self) -> Result<OrchestrationShellSnapshot, HttpError> {
        self.json(
            Method::GET,
            self.resolve("/api/orchestration/shell"),
            Body::Empty,
            true,
            SNAPSHOT_TIMEOUT,
        )
        .await
    }

    /// `GET /api/orchestration/threads/:id`: the fast thread open and older pages. `Ok(None)`
    /// when the server does not have the thread (upstream then defers to the socket).
    pub async fn thread_snapshot(
        &self,
        thread_id: &ThreadId,
        query: &ThreadSnapshotQuery,
    ) -> Result<Option<OrchestrationThreadDetailSnapshot>, HttpError> {
        let mut url = self.resolve("/api/orchestration/threads/");
        url.path_segments_mut()
            .expect("http urls have path segments")
            .pop_if_empty()
            .push(thread_id.as_str());
        {
            let mut pairs = url.query_pairs_mut();
            if let Some(limit) = query.turn_limit {
                pairs.append_pair("turnLimit", &limit.to_string());
            }
            if query.reasoning_messages {
                pairs.append_pair("reasoningMessages", "true");
            }
            if let Some(cursor) = &query.before_cursor {
                pairs.append_pair("beforeCursor", cursor);
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        match self
            .json(Method::GET, url, Body::Empty, true, SNAPSHOT_TIMEOUT)
            .await
        {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(HttpError::Status { status: 404, .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Uploads attachment bytes to the `relative_url` from `attachments.createUploadUrl`.
    pub async fn upload_attachment(
        &self,
        relative_url: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), HttpError> {
        self.send(
            Method::POST,
            self.resolve(relative_url),
            Body::Bytes {
                content_type: content_type.to_owned(),
                bytes,
            },
            false,
            UPLOAD_TIMEOUT,
        )
        .await
        .map(drop)
    }

    async fn json<T: DeserializeOwned + Send + 'static>(
        &self,
        method: Method,
        url: Url,
        body: Body,
        authenticated: bool,
        timeout: Duration,
    ) -> Result<T, HttpError> {
        let bytes = self
            .send(method, url, body, authenticated, timeout)
            .await?;
        serde_json::from_slice(&bytes).map_err(|e| HttpError::Decode(e.to_string()))
    }

    /// Runs one request on the networking runtime and returns the body of a 2xx response.
    async fn send(
        &self,
        method: Method,
        url: Url,
        body: Body,
        authenticated: bool,
        timeout: Duration,
    ) -> Result<Vec<u8>, HttpError> {
        let auth = if authenticated { self.auth.clone() } else { None };
        crate::runtime::spawn(async move {
            let mut headers = HeaderMap::new();
            if let Some(auth) = &auth {
                auth.authorize(&method, &url, &mut headers)
                    .map_err(HttpError::Auth)?;
            }
            let mut request = client()
                .request(method.clone(), url.clone())
                .headers(headers)
                .timeout(timeout);
            request = match body {
                Body::Empty => request,
                Body::Form(form) => request
                    .header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(form),
                Body::Bytes {
                    content_type,
                    bytes,
                } => request
                    .header(reqwest::header::CONTENT_TYPE, content_type)
                    .body(bytes),
            };
            let response = request.send().await.map_err(map_reqwest_error)?;
            let status = response.status();
            let bytes = response.bytes().await.map_err(map_reqwest_error)?;
            if let Some(tap) = RESPONSE_TAP.get() {
                tap(&method, &url, status.as_u16(), &bytes);
            }
            if status.is_success() {
                Ok(bytes.to_vec())
            } else {
                Err(HttpError::Status {
                    status: status.as_u16(),
                    error: serde_json::from_slice(&bytes).ok().map(Box::new),
                    body: String::from_utf8_lossy(&bytes).into_owned(),
                })
            }
        })
        .await
    }
}

fn map_reqwest_error(error: reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::Timeout
    } else if error.is_decode() {
        HttpError::Decode(error.to_string())
    } else {
        HttpError::Network(error_chain(&error))
    }
}

/// reqwest's top-level message is generic ("error sending request"); include the causes.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}
