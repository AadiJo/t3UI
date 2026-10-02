//! Provider setup: sign-in flows, managed CLI installs, ChatGPT profile transfer, and feedback
//! uploads (`packages/contracts/src/providerSetup.ts` and friends; Settings > Providers and the
//! onboarding cards). Methods fail with `ProviderSetupError` (`instanceId`, `operation`,
//! `detail`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EnvironmentId, ProviderInstanceId, ThreadId, open_enum};

/// Payload of methods that only name a provider instance (`provider.auth.logout`,
/// `provider.auth.subscribe`, `provider.install.*`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupInput {
    pub instance_id: ProviderInstanceId,
}

open_enum! {
    pub enum AuthPhase {
        Idle = "idle",
        Starting = "starting",
        /// Waiting on the user (browser, device code, terminal, credentials).
        Waiting = "waiting",
        Verifying = "verifying",
        Succeeded = "succeeded",
        Failed = "failed",
        Cancelled = "cancelled",
    }
}

/// `provider.auth.start`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthStartInput {
    pub instance_id: ProviderInstanceId,
    /// One of the provider's `ProviderAuthState.methods`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_url: Option<String>,
    /// `server` (the server receives the OAuth callback) or `client`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_mode: Option<String>,
}

/// A sign-in flow's state. Returned by every `provider.auth.*` call and streamed by
/// `provider.auth.subscribe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthState {
    pub instance_id: ProviderInstanceId,
    pub phase: AuthPhase,
    pub flow_id: Option<String>,
    pub authorization_url: Option<String>,
    pub expires_at: Option<String>,
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub methods: Option<Vec<ProviderAuthMethod>>,
    /// What the user has to do now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction: Option<ProviderAuthInteraction>,
    /// `provider` or `t3`: who holds the resulting credential.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_owner: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthMethod {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    pub name: String,
    pub description: Option<String>,
    /// `agent`, `terminal`, or `credentials`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// The step a sign-in flow is waiting on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProviderAuthInteraction {
    /// Open `url` (after consent when `requires_consent`).
    #[serde(rename_all = "camelCase")]
    Browser {
        id: String,
        url: String,
        #[serde(default)]
        requires_consent: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accepts_callback: Option<bool>,
    },
    /// Show `user_code` and open `url`.
    #[serde(rename_all = "camelCase")]
    DeviceCode {
        id: String,
        url: String,
        user_code: String,
    },
    /// An interactive CLI login: render `output` (from `output_offset`) in a terminal.
    #[serde(rename_all = "camelCase")]
    Terminal {
        id: String,
        #[serde(default)]
        output: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_offset: Option<u64>,
    },
    Credentials {
        id: String,
        #[serde(default)]
        fields: Vec<CredentialField>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialField {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub secret: bool,
}

/// `provider.auth.complete`: hand the server the OAuth callback URL (client callback mode).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthCompleteInput {
    pub instance_id: ProviderInstanceId,
    pub flow_id: String,
    pub callback_url: String,
}

/// `provider.auth.respond`: answer the current interaction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthRespondInput {
    pub instance_id: ProviderInstanceId,
    pub flow_id: String,
    pub interaction_id: String,
    pub response: ProviderAuthResponse,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProviderAuthResponse {
    /// `accept` or `decline`.
    Browser {
        action: String,
    },
    /// Keystrokes for a terminal login, optionally with the terminal's size.
    Terminal {
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<TerminalSize>,
    },
    Credentials {
        values: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

/// `provider.auth.cancel`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthCancelInput {
    pub instance_id: ProviderInstanceId,
    pub flow_id: String,
}

// ---------------------------------------------------------------------------------------------
// ChatGPT / Codex sign-in transfer

/// `provider.chatgpt.reconnect-profile`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptReconnectProfileInput {
    pub instance_id: ProviderInstanceId,
    pub method_id: String,
}

/// A ChatGPT OAuth client registration that can be reused to reconnect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptReconnectProfile {
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sharing_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token_hint: Option<String>,
}

/// A signed-in ChatGPT profile moved between environments. Holds live credentials: never log
/// or persist it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptTransferredProfile {
    pub registration: ChatGptReconnectProfile,
    pub credentials: ChatGptCredentials,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptCredentials {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: String,
    pub issuer: String,
    pub expires_at: f64,
    pub earliest_refresh_at: Option<f64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub subject: String,
    pub email: Option<String>,
}

/// `provider.chatgpt.import-profile`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptImportProfileInput {
    pub instance_id: ProviderInstanceId,
    pub profile: ChatGptTransferredProfile,
}

/// `provider.chatgpt.handoff.subscribe`: sign in on this environment and hand the profile to
/// another.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptHandoffInput {
    pub instance_id: ProviderInstanceId,
    pub environment_id: EnvironmentId,
    pub attempt_id: String,
    pub return_url: String,
    pub profile: Option<ChatGptReconnectProfile>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum ChatGptHandoffState {
    Auth {
        state: Box<ProviderAuthState>,
    },
    Finished {
        profile: Box<ChatGptTransferredProfile>,
    },
    #[serde(other)]
    Unknown,
}

/// `provider.codex.auth-callback.subscribe`: relay a Codex OAuth callback through the client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAuthCallbackInput {
    pub authorization_url: String,
    pub return_url: String,
    pub environment_id: EnvironmentId,
    pub instance_id: ProviderInstanceId,
    pub flow_id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum CodexAuthCallbackState {
    Ready,
    #[serde(rename_all = "camelCase")]
    Finished {
        callback_url: String,
    },
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------------------------
// Managed installs

open_enum! {
    pub enum InstallPhase {
        Idle = "idle",
        Downloading = "downloading",
        Extracting = "extracting",
        Verifying = "verifying",
        Succeeded = "succeeded",
        Failed = "failed",
        Cancelled = "cancelled",
    }
}

/// A managed provider CLI install. Returned by `provider.install.*` and streamed by
/// `provider.install.subscribe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstallState {
    pub driver: String,
    pub operation_id: Option<String>,
    pub phase: InstallPhase,
    #[serde(default)]
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub version: Option<String>,
    pub installed_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    /// `managed` or `local`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub can_remove: bool,
    pub message: Option<String>,
}

/// `provider.install.cancel`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstallCancelInput {
    pub instance_id: ProviderInstanceId,
    pub operation_id: String,
}

// ---------------------------------------------------------------------------------------------
// Feedback

/// `provider.uploadFeedback`: send a thread's provider logs to the provider.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUploadFeedbackInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUploadFeedbackResult {
    pub feedback_id: String,
}
