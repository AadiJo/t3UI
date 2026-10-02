//! The in-app browser preview (`packages/contracts/src/preview.ts`): per-thread tabs whose
//! state the server tracks, agent-driven browser automation hosted by a client, and local dev
//! servers discovered for the URL bar. The browser itself runs in the client; the server
//! brokers state and automation requests.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{EnvironmentId, ThreadId};

/// Viewport of a preview tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag", rename_all = "lowercase")]
pub enum PreviewViewport {
    Fill,
    Freeform {
        width: u32,
        height: u32,
    },
    /// A device preset (`iphone-15-pro`, `desktop-1440x900`, ...).
    #[serde(rename_all = "camelCase")]
    Preset {
        width: u32,
        height: u32,
        preset_id: String,
    },
    #[serde(other)]
    Unknown,
}

/// Where a tab's navigation stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum PreviewNavStatus {
    Idle,
    Loading {
        url: String,
        title: String,
    },
    Success {
        url: String,
        title: String,
    },
    LoadFailed {
        url: String,
        title: String,
        code: i64,
        description: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSessionSnapshot {
    pub thread_id: ThreadId,
    pub tab_id: String,
    pub nav_status: PreviewNavStatus,
    #[serde(default)]
    pub can_go_back: bool,
    #[serde(default)]
    pub can_go_forward: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<PreviewViewport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    pub updated_at: String,
}

/// `preview.open`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewOpenInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub viewport: Option<PreviewViewport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

/// `preview.navigate`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewNavigateInput {
    pub thread_id: ThreadId,
    pub tab_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_title: Option<String>,
}

/// `preview.resize`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResizeInput {
    pub thread_id: ThreadId,
    pub tab_id: String,
    pub viewport: PreviewViewport,
}

/// `preview.refresh`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTabRef {
    pub thread_id: ThreadId,
    pub tab_id: String,
}

/// `preview.close`; no `tab_id` closes every tab of the thread.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCloseInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
}

/// `preview.list`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewListInput {
    pub thread_id: ThreadId,
}

/// `serverEpoch` + `revision` order snapshots against [`PreviewEvent`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewListResult {
    #[serde(default)]
    pub sessions: Vec<PreviewSessionSnapshot>,
    pub server_epoch: String,
    pub revision: u64,
}

/// `preview.reportStatus`: the client's browser reports navigation state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReportStatusInput {
    pub thread_id: ThreadId,
    pub tab_id: String,
    pub nav_status: PreviewNavStatus,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

/// An item of `subscribePreviewEvents`. Every event carries `thread_id`, `tab_id`,
/// `created_at`, `server_epoch`, `revision`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewEvent {
    pub thread_id: ThreadId,
    pub tab_id: String,
    pub created_at: String,
    pub server_epoch: String,
    pub revision: u64,
    #[serde(flatten)]
    pub kind: PreviewEventKind,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PreviewEventKind {
    Opened {
        snapshot: PreviewSessionSnapshot,
    },
    Navigated {
        snapshot: PreviewSessionSnapshot,
    },
    Resized {
        snapshot: PreviewSessionSnapshot,
    },
    Failed {
        url: String,
        title: String,
        code: i64,
        description: String,
    },
    Closed,
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------------------------
// Automation: a client hosts browser automation for agents.

/// `previewAutomation.connect`: offer this client as an automation host.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAutomationHost {
    pub client_id: String,
    pub environment_id: EnvironmentId,
    /// `status`, `open`, `navigate`, `snapshot`, `click`, `type`, ... ; `None` is all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported_operations: Option<Vec<String>>,
}

/// An item of `previewAutomation.connect`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PreviewAutomationStreamEvent {
    #[serde(rename_all = "camelCase")]
    Connected { connection_id: String },
    /// Perform `request` and answer with `previewAutomation.respond`.
    #[serde(rename_all = "camelCase")]
    Request {
        connection_id: String,
        request: PreviewAutomationRequest,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAutomationRequest {
    pub request_id: String,
    pub thread_id: ThreadId,
    pub tab_id: Option<String>,
    pub tab_id_explicit: Option<bool>,
    pub operation: String,
    /// Operation-specific; see upstream `previewAutomation.ts`.
    #[serde(default)]
    pub input: Value,
    pub timeout_ms: u64,
}

/// `previewAutomation.respond`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAutomationResponse {
    pub client_id: String,
    pub connection_id: String,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PreviewAutomationFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewAutomationFailure {
    #[serde(rename = "_tag")]
    pub tag: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Value>,
}

/// `previewAutomation.focusHost`: which host should take requests, and its live tabs.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewAutomationHostFocus {
    pub client_id: String,
    pub environment_id: EnvironmentId,
    pub connection_id: String,
    pub focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_tabs: Option<Vec<PreviewLiveTab>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLiveTab {
    pub thread_id: ThreadId,
    pub tab_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
}

// ---------------------------------------------------------------------------------------------
// Local servers

/// `subscribeDiscoveredLocalServers`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLocalServersInput {
    /// Also probe these URLs (from project scripts' `previewUrl`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLocalServerList {
    #[serde(default)]
    pub servers: Vec<DiscoveredLocalServer>,
    pub scanned_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_url_probing: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLocalServer {
    pub host: String,
    pub port: u16,
    pub url: String,
    pub process_name: Option<String>,
    pub pid: Option<u32>,
    /// The terminal that started it, when known.
    pub terminal: Option<DiscoveredServerTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServerTerminal {
    pub thread_id: ThreadId,
    pub terminal_id: String,
}
