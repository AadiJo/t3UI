//! Environment identity and the HTTP auth endpoints (`environment.ts`, `auth.ts`,
//! `environmentHttp.ts`).

use serde::{Deserialize, Serialize};

use crate::{ids::EnvironmentId, open_enum};

/// The orchestration protocol this client speaks. A descriptor with a different
/// `orchestration_protocol_version` (missing means 1) must not be connected to.
pub const ORCHESTRATION_PROTOCOL_VERSION: u32 = 1;

open_enum! {
    pub enum PlatformOs {
        Darwin = "darwin",
        Linux = "linux",
        Windows = "windows",
        Unknown = "unknown",
    }
}

open_enum! {
    /// Icon hint for an environment.
    pub enum MachineKind {
        Server = "server",
        Cloud = "cloud",
        Linux = "linux",
        Desktop = "desktop",
        Laptop = "laptop",
        MacMini = "mac-mini",
        MacStudio = "mac-studio",
    }
}

/// `GET /.well-known/t3/environment` (no auth), also `ServerConfig.environment`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironmentDescriptor {
    pub environment_id: EnvironmentId,
    pub label: String,
    pub platform: ExecutionEnvironmentPlatform,
    pub server_version: String,
    pub orchestration_protocol_version: Option<u32>,
    #[serde(default)]
    pub capabilities: ExecutionEnvironmentCapabilities,
}

impl ExecutionEnvironmentDescriptor {
    /// The protocol version, defaulting to 1 when the server omits it.
    pub fn protocol_version(&self) -> u32 {
        self.orchestration_protocol_version.unwrap_or(1)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironmentPlatform {
    pub os: PlatformOs,
    pub arch: String,
    pub machine: Option<MachineKind>,
}

/// Feature gates. Every flag defaults to "unsupported" when missing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExecutionEnvironmentCapabilities {
    pub repository_identity: Option<bool>,
    pub connection_probe: bool,
    pub attachment_uploads: bool,
    pub question_attachments: bool,
    pub file_attachments: Option<FileAttachmentLimits>,
    pub pull_requests: bool,
    pub inline_message_context: bool,
    pub required_worktree_bootstrap: bool,
    pub thread_settlement: bool,
    pub thread_auto_settlement: bool,
    pub storage_cleanup: bool,
    pub project_worktree_cleanup: bool,
    pub thread_restart_continuation: bool,
    pub project_settings_overrides: bool,
    pub thread_snooze: bool,
    pub environment_themes: bool,
    pub usage_limit_sources: bool,
    pub usage_price_overrides: bool,
    pub thread_pinning: bool,
    pub thread_pin_reorder: bool,
    pub thread_active_reorder: bool,
    pub thread_auto_settle_opt_out: bool,
    pub thread_title_regeneration: bool,
    pub thread_pull_request_linking: bool,
    pub thread_pull_requests: bool,
    pub pull_request_stack_actions: bool,
    /// `boot-service`, `respawn`, or `desktop-managed`.
    pub server_self_update: Option<String>,
    pub server_self_update_progress: bool,
    pub server_update_thread_continuation: bool,
    pub agent_activity_publishing: bool,
    pub project_clone_tracking: bool,
    pub environment_icon: bool,
    pub desktop_app_update: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAttachmentLimits {
    pub max_upload_bytes: u64,
}

open_enum! {
    /// OAuth-style scopes granted to a session (`auth.ts:81-115`).
    pub enum AuthScope {
        OrchestrationRead = "orchestration:read",
        OrchestrationOperate = "orchestration:operate",
        TerminalOperate = "terminal:operate",
        ReviewWrite = "review:write",
        AccessRead = "access:read",
        AccessWrite = "access:write",
        RelayRead = "relay:read",
        RelayWrite = "relay:write",
    }
}

/// Scopes a regular (non-admin) client requests when pairing (`AuthStandardClientScopes`).
pub const STANDARD_CLIENT_SCOPES: &str =
    "orchestration:read orchestration:operate terminal:operate review:write relay:read";

/// `ServerAuthDescriptor`, in `ServerConfig.auth` and the session check.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAuthDescriptor {
    pub policy: String,
    #[serde(default)]
    pub bootstrap_methods: Vec<String>,
    #[serde(default)]
    pub session_methods: Vec<String>,
    pub session_cookie_name: String,
}

/// `POST /oauth/token` form body for exchanging a pairing credential for a bearer token.
#[derive(Debug, Clone, Serialize)]
pub struct TokenExchangeRequest<'a> {
    pub grant_type: &'static str,
    pub subject_token: &'a str,
    pub subject_token_type: &'static str,
    pub requested_token_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_label: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_device_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_os: Option<&'a str>,
}

impl<'a> TokenExchangeRequest<'a> {
    /// A pairing-credential exchange with the standard client scopes.
    pub fn pairing(credential: &'a str) -> Self {
        TokenExchangeRequest {
            grant_type: "urn:ietf:params:oauth:grant-type:token-exchange",
            subject_token: credential,
            subject_token_type: "urn:t3:params:oauth:token-type:environment-bootstrap",
            requested_token_type: "urn:ietf:params:oauth:token-type:access_token",
            scope: Some(STANDARD_CLIENT_SCOPES),
            client_label: None,
            client_device_type: None,
            client_os: None,
        }
    }
}

/// `POST /oauth/token` success. Bearer tokens last 30 days; there is no refresh.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AccessTokenResult {
    pub access_token: String,
    pub issued_token_type: Option<String>,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

/// `POST /api/auth/websocket-ticket`. Five-minute TTL; fetch one per connect attempt.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketTicket {
    pub ticket: String,
    pub expires_at: String,
}

/// `GET /api/auth/session`. Always 200; `authenticated: false` means the credential is bad.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionState {
    pub authenticated: bool,
    pub auth: Option<ServerAuthDescriptor>,
    #[serde(default)]
    pub scopes: Vec<AuthScope>,
    pub session_method: Option<String>,
    pub expires_at: Option<String>,
}

/// JSON body of an HTTP error from the environment (`environmentHttp.ts:66-211`), e.g.
/// `{"_tag":"EnvironmentAuthInvalidError","code":"auth_invalid","reason":"invalid_credential"}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentHttpError {
    #[serde(rename = "_tag")]
    pub tag: String,
    pub code: Option<String>,
    pub reason: Option<String>,
    pub required_scope: Option<String>,
    pub dpop_failure_reason: Option<String>,
    pub trace_id: Option<String>,
}

/// Output of `t3 auth pairing create --json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuedPairingCredential {
    pub id: String,
    pub credential: String,
    pub label: Option<String>,
    #[serde(default)]
    pub scopes: Vec<AuthScope>,
    pub expires_at: String,
    pub pair_url: Option<String>,
}
