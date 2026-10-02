//! Server config, providers, settings, keybindings, lifecycle, and auth access
//! (`packages/contracts/src/server.ts`, `settings.ts`, `keybindings.ts`, protocol.md section 7).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    environment::{AuthScope, ExecutionEnvironmentDescriptor, MachineKind, ServerAuthDescriptor},
    ids::ProviderInstanceId,
    open_enum,
    orchestration::{ModelSelection, ProjectScript, RuntimeMode, ThreadEnvMode},
    schema::forward_compatible,
};

open_enum! {
    /// Editors `shell.openInEditor` can launch. Only offer the ones in
    /// `ServerConfig.available_editors`.
    pub enum EditorId {
        Cursor = "cursor",
        Trae = "trae",
        Kiro = "kiro",
        Vscode = "vscode",
        VscodeInsiders = "vscode-insiders",
        Vscodium = "vscodium",
        Zed = "zed",
        Antigravity = "antigravity",
        Idea = "idea",
        Aqua = "aqua",
        Clion = "clion",
        Datagrip = "datagrip",
        Dataspell = "dataspell",
        Goland = "goland",
        Phpstorm = "phpstorm",
        Pycharm = "pycharm",
        Rider = "rider",
        Rubymine = "rubymine",
        Rustrover = "rustrover",
        Webstorm = "webstorm",
        FileManager = "file-manager",
    }
}

/// Everything the server tells a client about itself. Arrives as the first
/// `subscribeServerConfig` item; later items replace one slice at a time.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub environment: ExecutionEnvironmentDescriptor,
    pub auth: ServerAuthDescriptor,
    pub cwd: String,
    #[serde(default)]
    pub keybindings_config_path: String,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub keybindings: Vec<ResolvedKeybindingRule>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub issues: Vec<KeybindingsIssue>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub providers: Vec<ServerProvider>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub available_editors: Vec<EditorId>,
    pub remote_open_targets: Option<Vec<RemoteOpenTarget>>,
    pub observability: Option<ServerObservability>,
    pub settings: ServerSettings,
    /// `subscribeShell` supports `requestCompletionMarker`.
    #[serde(default)]
    pub shell_resume_completion_marker: bool,
    pub shell_reveal_in_file_manager: Option<bool>,
    pub shell_reveal_in_file_manager_kind: Option<String>,
    /// `subscribeThread` supports `requestCompletionMarker`.
    #[serde(default)]
    pub thread_resume_completion_marker: bool,
    /// Thread snapshots support `turnLimit` / `beforeCursor`.
    #[serde(default)]
    pub thread_snapshot_pagination: bool,
    /// Thread subscriptions support `reasoningMessages`.
    #[serde(default)]
    pub reasoning_messages: bool,
    pub scratch_workspace_root: Option<String>,
    pub new_projects_root: Option<String>,
    pub environment_themes: Option<Vec<Value>>,
    pub usage_limit_sources: Option<Vec<Value>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteOpenTarget {
    pub kind: String,
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerObservability {
    pub logs_directory_path: String,
    #[serde(default)]
    pub local_tracing_enabled: bool,
    pub otlp_traces_url: Option<String>,
    #[serde(default)]
    pub otlp_traces_enabled: bool,
    pub otlp_metrics_url: Option<String>,
    #[serde(default)]
    pub otlp_metrics_enabled: bool,
    pub otlp_logs_url: Option<String>,
    pub otlp_logs_enabled: Option<bool>,
}

// ---------------------------------------------------------------------------------------------
// Keybindings

/// A resolved keybinding. `command` is a fixed literal (`chat.new`, `thread.stop`, ...) or
/// `script.<id>.run`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedKeybindingRule {
    pub command: String,
    pub shortcut: KeybindingShortcut,
    pub when_ast: Option<KeybindingWhenNode>,
}

/// `mod_key` means Cmd on macOS and Ctrl elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingShortcut {
    pub key: String,
    #[serde(default)]
    pub meta_key: bool,
    #[serde(default)]
    pub ctrl_key: bool,
    #[serde(default)]
    pub shift_key: bool,
    #[serde(default)]
    pub alt_key: bool,
    #[serde(default)]
    pub mod_key: bool,
}

/// A `when` clause.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum KeybindingWhenNode {
    Identifier {
        name: String,
    },
    Not {
        node: Box<KeybindingWhenNode>,
    },
    And {
        left: Box<KeybindingWhenNode>,
        right: Box<KeybindingWhenNode>,
    },
    Or {
        left: Box<KeybindingWhenNode>,
        right: Box<KeybindingWhenNode>,
    },
    /// A node type this client does not know. Treat the binding as never active.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingsIssue {
    pub kind: String,
    pub message: String,
    pub index: Option<u32>,
}

/// `server.upsertKeybinding`. `key` grammar: lowercase tokens joined by `+`, e.g. `mod+shift+k`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertKeybindingInput {
    pub key: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<RemoveKeybindingInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveKeybindingInput {
    pub key: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// Result of keybinding edits and the `keybindingsUpdated` config event.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingsUpdated {
    #[serde(default, deserialize_with = "forward_compatible")]
    pub keybindings: Vec<ResolvedKeybindingRule>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub issues: Vec<KeybindingsIssue>,
}

// ---------------------------------------------------------------------------------------------
// Providers

open_enum! {
    pub enum ProviderStatus {
        Ready = "ready",
        Warning = "warning",
        Error = "error",
        Disabled = "disabled",
    }
}

open_enum! {
    pub enum ProviderAuthStatus {
        Authenticated = "authenticated",
        Unauthenticated = "unauthenticated",
        Unknown = "unknown",
    }
}

/// One configured provider instance with its health and models (model picker data).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProvider {
    pub instance_id: ProviderInstanceId,
    /// Implementation: `codex`, `claudeAgent`, `cursor`, `opencode`, ...
    pub driver: String,
    pub display_name: Option<String>,
    pub accent_color: Option<String>,
    pub badge_label: Option<String>,
    pub continuation: Option<ProviderContinuation>,
    #[serde(default)]
    pub show_interaction_mode_toggle: bool,
    #[serde(default)]
    pub reports_context_window: bool,
    #[serde(default)]
    pub requires_new_thread_for_model_change: bool,
    #[serde(default)]
    pub supports_conversation_rollback: bool,
    #[serde(default)]
    pub supports_text_generation: bool,
    pub setup: Option<ProviderSetup>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub installed: bool,
    pub version: Option<String>,
    pub status: ProviderStatus,
    pub auth: ProviderAuth,
    pub checked_at: String,
    pub message: Option<String>,
    /// `available` / `unavailable`.
    pub availability: Option<String>,
    pub unavailable_reason: Option<String>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub models: Vec<ServerProviderModel>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub slash_commands: Vec<ProviderSlashCommand>,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub skills: Vec<ProviderSkill>,
    pub workspace_snapshots: Option<Value>,
    pub usage_limits: Option<Value>,
    pub version_advisory: Option<Value>,
    pub compatibility_advisory: Option<Value>,
    pub update_state: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderContinuation {
    pub group_key: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetup {
    #[serde(default)]
    pub can_authenticate: bool,
    #[serde(default)]
    pub can_install: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuth {
    pub status: ProviderAuthStatus,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub label: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderModel {
    /// Goes in `ModelSelection.model`.
    pub slug: String,
    pub name: String,
    pub short_name: Option<String>,
    pub sub_provider: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// `"new"` when set.
    pub badge: Option<String>,
    #[serde(default)]
    pub is_custom: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub is_legacy: bool,
    pub capabilities: Option<ModelCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapabilities {
    #[serde(default, deserialize_with = "forward_compatible")]
    pub option_descriptors: Vec<ProviderOptionDescriptor>,
}

/// A model option ("trait") the picker offers. Picks become `ModelSelection.options`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderOptionDescriptor {
    #[serde(rename_all = "camelCase")]
    Select {
        id: String,
        label: String,
        description: Option<String>,
        #[serde(default)]
        options: Vec<ProviderOptionChoice>,
        current_value: Option<String>,
        #[serde(default)]
        prompt_injected_values: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Boolean {
        id: String,
        label: String,
        description: Option<String>,
        current_value: Option<bool>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOptionChoice {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSlashCommand {
    pub name: String,
    pub description: Option<String>,
    pub input: Option<ProviderSlashCommandInput>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSlashCommandInput {
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSkill {
    pub name: String,
    pub description: Option<String>,
    pub path: String,
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    pub display_name: Option<String>,
    pub short_description: Option<String>,
    #[serde(default)]
    pub user_invocation_only: bool,
    pub user_invocable: Option<bool>,
}

/// `server.refreshProviders`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProvidersInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<ProviderInstanceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_models: Option<bool>,
}

/// `server.updateProvider`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderInput {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<ProviderInstanceId>,
}

/// Result of provider refresh/update and the `providerStatuses` config event.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersUpdated {
    #[serde(default, deserialize_with = "forward_compatible")]
    pub providers: Vec<ServerProvider>,
}

// ---------------------------------------------------------------------------------------------
// Settings

open_enum! {
    pub enum ResponseStreamingMode {
        Turn = "turn",
        Paragraph = "paragraph",
        Token = "token",
    }
}

/// Server-side settings (`settings.ts:1109`), secrets redacted. The fields the UI reads are
/// typed; the rest (provider configs, cleanup rules, overrides, ...) stay in `other` as raw
/// JSON.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettings {
    pub response_streaming_mode: Option<ResponseStreamingMode>,
    pub enable_provider_update_checks: Option<bool>,
    pub continue_threads_after_server_update: Option<bool>,
    pub enable_agent_browser_access: Option<bool>,
    pub default_auto_pull: Option<bool>,
    pub default_project_scripts: Option<Vec<ProjectScript>>,
    pub default_model_selection: Option<ModelSelection>,
    pub default_runtime_mode: Option<RuntimeMode>,
    pub sidebar_auto_settle_after_days: Option<f64>,
    pub sidebar_auto_settle_on_merge: Option<bool>,
    pub default_theme: Option<String>,
    pub environment_icon: Option<MachineKind>,
    pub default_thread_env_mode: Option<ThreadEnvMode>,
    pub new_worktrees_start_from_origin: Option<bool>,
    pub worktree_submodules: Option<String>,
    pub add_project_base_directory: Option<String>,
    pub text_generation_model_selection: Option<ModelSelection>,
    pub source_control_writer_model_selection: Option<ModelSelection>,
    pub pull_request_merge_method: Option<String>,
    pub provider_instances: Option<BTreeMap<String, ProviderInstanceConfig>>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstanceConfig {
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Value>,
}

/// `server.updateSettings {"patch": ...}`: a deep partial the server merges. Unset fields are
/// unchanged; `Some(None)` writes `null` for nullable fields. Anything not typed here goes in
/// `other` as raw JSON.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_streaming_mode: Option<ResponseStreamingMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_provider_update_checks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_auto_pull: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model_selection: Option<Option<ModelSelection>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_runtime_mode: Option<RuntimeMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_env_mode: Option<Option<ThreadEnvMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_worktrees_start_from_origin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_generation_model_selection: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_project_base_directory: Option<String>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateSettingsInput {
    pub patch: ServerSettingsPatch,
}

// ---------------------------------------------------------------------------------------------
// Config stream

/// Payload of `subscribeServerConfig`. Each flag opts into one extra event type that older
/// clients would fail to decode.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeServerConfigInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_themes: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_limit_sources: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_limits_command: Option<bool>,
}

/// An item of `subscribeServerConfig`. Each event replaces one slice of the config.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerConfigStreamEvent {
    /// Always first.
    Snapshot { config: Box<ServerConfig> },
    KeybindingsUpdated { payload: KeybindingsUpdated },
    ProviderStatuses { payload: ProvidersUpdated },
    SettingsUpdated { payload: Box<SettingsUpdatedPayload> },
    EnvironmentThemesUpdated { payload: Value },
    UsageLimitSourcesUpdated { payload: Value },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SettingsUpdatedPayload {
    pub settings: ServerSettings,
}

impl ServerConfig {
    /// Applies a config stream event. Returns `false` for a snapshot or unknown event (callers
    /// replace the whole config on snapshot).
    pub fn apply(&mut self, event: ServerConfigStreamEvent) -> bool {
        match event {
            ServerConfigStreamEvent::Snapshot { config } => {
                *self = *config;
                true
            }
            ServerConfigStreamEvent::KeybindingsUpdated { payload } => {
                self.keybindings = payload.keybindings;
                self.issues = payload.issues;
                true
            }
            ServerConfigStreamEvent::ProviderStatuses { payload } => {
                self.providers = payload.providers;
                true
            }
            ServerConfigStreamEvent::SettingsUpdated { payload } => {
                self.settings = payload.settings;
                true
            }
            ServerConfigStreamEvent::EnvironmentThemesUpdated { payload } => {
                self.environment_themes = payload
                    .get("themes")
                    .and_then(Value::as_array)
                    .cloned();
                true
            }
            ServerConfigStreamEvent::UsageLimitSourcesUpdated { payload } => {
                self.usage_limit_sources = payload
                    .get("sources")
                    .and_then(Value::as_array)
                    .cloned();
                true
            }
            ServerConfigStreamEvent::Unknown => false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Lifecycle

/// An item of `subscribeServerLifecycle`: buffered events replayed by `sequence`, then live.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerLifecycleStreamEvent {
    Welcome {
        sequence: u64,
        payload: LifecycleWelcome,
    },
    Ready {
        sequence: u64,
        payload: LifecycleReady,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleWelcome {
    pub environment: ExecutionEnvironmentDescriptor,
    pub cwd: String,
    pub project_name: String,
    pub bootstrap_status: Option<String>,
    pub bootstrap_project_id: Option<String>,
    pub bootstrap_thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleReady {
    pub at: String,
    pub environment: ExecutionEnvironmentDescriptor,
    pub update_outcome: Option<ServerSelfUpdateOutcome>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSelfUpdateOutcome {
    pub id: String,
    pub from_version: String,
    pub target_version: String,
    /// `committed`, `rolled-back`, or `failed`.
    pub status: String,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Auth access (settings > devices; needs `access:read`)

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthPairingLink {
    pub id: String,
    #[serde(default)]
    pub scopes: Vec<AuthScope>,
    pub subject: String,
    pub label: Option<String>,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthClientSession {
    pub session_id: String,
    pub subject: String,
    #[serde(default)]
    pub scopes: Vec<AuthScope>,
    pub method: String,
    pub client: AuthClientMetadata,
    pub issued_at: String,
    pub expires_at: String,
    pub last_connected_at: Option<String>,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthClientMetadata {
    pub label: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub device_type: String,
    pub os: Option<String>,
    pub browser: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthAccessSnapshot {
    #[serde(default)]
    pub pairing_links: Vec<AuthPairingLink>,
    #[serde(default)]
    pub client_sessions: Vec<AuthClientSession>,
}

/// An item of `subscribeAuthAccess`, each with an incrementing `revision`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthAccessStreamEvent {
    Snapshot {
        revision: u64,
        payload: AuthAccessSnapshot,
    },
    PairingLinkUpserted {
        revision: u64,
        payload: AuthPairingLink,
    },
    PairingLinkRemoved {
        revision: u64,
        payload: IdPayload,
    },
    ClientUpserted {
        revision: u64,
        payload: AuthClientSession,
    },
    ClientRemoved {
        revision: u64,
        payload: SessionIdPayload,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IdPayload {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdPayload {
    pub session_id: String,
}
