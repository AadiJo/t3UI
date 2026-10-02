//! Server operations: self-update, source control tool discovery, diagnostics, resource
//! telemetry, process signals, client activity leases, and the T3 Connect relay client
//! (`packages/contracts/src/server.ts`, `diagnostics.ts`, `resourceTelemetry.ts`,
//! `background.ts`, `relay.ts`; Settings > Diagnostics / About).
//!
//! Fields typed `Option` with `effect_option` are `Schema.Option` on the wire
//! (`{"_tag":"Some","value":..}`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EnvironmentId, open_enum};

// ---------------------------------------------------------------------------------------------
// Self-update

/// `server.updateServer` / `server.updateServerWithProgress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSelfUpdateInput {
    pub target_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continue_running_threads: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSelfUpdateResult {
    pub target_version: String,
    /// `boot-service`, `respawn`, or `desktop-app`.
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_id: Option<String>,
    /// For `desktop-app`: pass to `server.commitDesktopUpdate` after installing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_update_token: Option<String>,
}

/// An item of `server.updateServerWithProgress`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerSelfUpdateProgressEvent {
    /// `downloading` or `installing`.
    Progress {
        stage: String,
    },
    Complete {
        result: ServerSelfUpdateResult,
    },
    #[serde(other)]
    Unknown,
}

/// `server.commitDesktopUpdate`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopUpdateCommitInput {
    pub request_id: String,
}

// ---------------------------------------------------------------------------------------------
// Source control discovery

/// `server.discoverSourceControl`: which VCS and host CLIs the server can use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlDiscoveryResult {
    #[serde(default)]
    pub version_control_systems: Vec<VcsDiscoveryItem>,
    #[serde(default)]
    pub source_control_providers: Vec<SourceControlProviderDiscoveryItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsDiscoveryItem {
    /// `git`, `jj`, or `unknown`.
    pub kind: String,
    #[serde(default)]
    pub implemented: bool,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    /// `available` or `missing`.
    pub status: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub version: Option<String>,
    pub install_hint: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlProviderDiscoveryItem {
    pub kind: crate::pull_requests::SourceControlProviderKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    pub status: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub version: Option<String>,
    pub install_hint: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub detail: Option<String>,
    pub auth: SourceControlProviderAuth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceControlProviderAuth {
    /// `authenticated`, `unauthenticated`, or `unknown`.
    pub status: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub account: Option<String>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub host: Option<String>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Diagnostics

/// `server.getTraceDiagnostics`: a summary of the server's local trace file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTraceDiagnostics {
    pub trace_file_path: String,
    #[serde(default)]
    pub scanned_file_paths: Vec<String>,
    pub read_at: String,
    #[serde(default)]
    pub record_count: u64,
    #[serde(default)]
    pub parse_error_count: u64,
    #[serde(default, with = "crate::schema::effect_option")]
    pub first_span_at: Option<String>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub last_span_at: Option<String>,
    #[serde(default)]
    pub failure_count: u64,
    #[serde(default)]
    pub interruption_count: u64,
    #[serde(default)]
    pub slow_span_threshold_ms: f64,
    #[serde(default)]
    pub slow_span_count: u64,
    #[serde(default)]
    pub log_level_counts: BTreeMap<String, u64>,
    #[serde(default)]
    pub top_spans_by_count: Vec<TraceSpanSummary>,
    #[serde(default)]
    pub slowest_spans: Vec<TraceSpanOccurrence>,
    #[serde(default)]
    pub common_failures: Vec<TraceFailureSummary>,
    #[serde(default)]
    pub latest_failures: Vec<TraceRecentFailure>,
    #[serde(default)]
    pub latest_warning_and_error_logs: Vec<TraceLogEvent>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub partial_failure: Option<bool>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub error: Option<TraceDiagnosticsError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceDiagnosticsError {
    /// `trace-file-not-found` or `trace-file-read-failed`.
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSpanSummary {
    pub name: String,
    pub count: u64,
    pub failure_count: u64,
    pub total_duration_ms: f64,
    pub average_duration_ms: f64,
    pub max_duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSpanOccurrence {
    pub name: String,
    pub duration_ms: f64,
    pub ended_at: String,
    pub trace_id: String,
    pub span_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceFailureSummary {
    pub name: String,
    pub cause: String,
    pub count: u64,
    pub last_seen_at: String,
    pub trace_id: String,
    pub span_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRecentFailure {
    pub name: String,
    pub cause: String,
    pub duration_ms: f64,
    pub ended_at: String,
    pub trace_id: String,
    pub span_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceLogEvent {
    pub span_name: String,
    pub level: String,
    pub message: String,
    pub seen_at: String,
    pub trace_id: String,
    pub span_id: String,
}

/// `server.getProcessDiagnostics`: the server's process tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProcessDiagnostics {
    pub server_pid: u32,
    pub read_at: String,
    #[serde(default)]
    pub process_count: u64,
    #[serde(default)]
    pub total_rss_bytes: u64,
    #[serde(default)]
    pub total_cpu_percent: f64,
    #[serde(default)]
    pub processes: Vec<ServerProcessEntry>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub error: Option<MessageOnly>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageOnly {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProcessEntry {
    pub pid: u32,
    /// With `pid`, identifies the process for `server.signalProcess`.
    pub start_time_ms: f64,
    pub ppid: u32,
    #[serde(default, with = "crate::schema::effect_option")]
    pub pgid: Option<u32>,
    pub status: String,
    #[serde(default)]
    pub cpu_percent: f64,
    #[serde(default)]
    pub rss_bytes: u64,
    pub elapsed: String,
    pub command: String,
    #[serde(default)]
    pub depth: u32,
    #[serde(default)]
    pub child_pids: Vec<u32>,
}

/// `server.getHostResources`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostResourcesSnapshot {
    /// Epoch ms.
    pub sampled_at: f64,
    /// 0 to 1, `None` on the first sample.
    pub cpu_utilization: Option<f64>,
    pub cpu_count: u32,
    pub available_memory_bytes: u64,
    pub total_memory_bytes: u64,
}

/// `server.getProcessResourceHistory` and `server.getResourceTelemetryHistory`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceHistoryInput {
    pub window_ms: u64,
    pub bucket_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProcessResourceHistory {
    pub read_at: String,
    pub window_ms: u64,
    pub bucket_ms: u64,
    pub sample_interval_ms: u64,
    #[serde(default)]
    pub retained_sample_count: u64,
    #[serde(default)]
    pub total_cpu_seconds_approx: f64,
    #[serde(default)]
    pub buckets: Vec<ProcessHistoryBucket>,
    #[serde(default)]
    pub top_processes: Vec<ProcessHistorySummary>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub error: Option<ProcessHistoryError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessHistoryError {
    pub failure_tag: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessHistoryBucket {
    pub started_at: String,
    pub ended_at: String,
    pub avg_cpu_percent: f64,
    pub max_cpu_percent: f64,
    pub max_rss_bytes: u64,
    pub max_process_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessHistorySummary {
    pub process_key: String,
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
    pub depth: u32,
    #[serde(default)]
    pub is_server_root: bool,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub current_cpu_percent: f64,
    pub avg_cpu_percent: f64,
    pub max_cpu_percent: f64,
    pub cpu_seconds_approx: f64,
    pub current_rss_bytes: u64,
    pub max_rss_bytes: u64,
    pub sample_count: u64,
}

open_enum! {
    pub enum Signal {
        Interrupt = "SIGINT",
        Kill = "SIGKILL",
    }
}

/// `server.signalProcess`: only descendants of the server can be signaled.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSignalProcessInput {
    pub pid: u32,
    pub start_time_ms: f64,
    pub signal: Signal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSignalProcessResult {
    pub pid: u32,
    pub signal: Signal,
    #[serde(default)]
    pub signaled: bool,
    #[serde(default, with = "crate::schema::effect_option")]
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Resource telemetry (Settings > Diagnostics resource monitor)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTelemetryHistory {
    pub read_at: String,
    pub window_ms: u64,
    pub bucket_ms: u64,
    pub sample_interval_ms: u64,
    #[serde(default)]
    pub retained_sample_count: u64,
    #[serde(default)]
    pub buckets: Vec<TelemetryHistoryBucket>,
    #[serde(default)]
    pub top_processes: Vec<TelemetryProcessSummary>,
    pub health: TelemetryHealth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryHistoryBucket {
    pub started_at: String,
    pub ended_at: String,
    pub avg_cpu_percent: f64,
    pub max_cpu_percent: f64,
    pub max_rss_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub max_process_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryProcessSummary {
    pub identity: ProcessIdentity,
    pub ppid: u32,
    pub depth: u32,
    pub name: String,
    pub command: String,
    /// `server`, `server-child`, `provider-root`, `terminal-root`, `electron-*`, ...
    pub category: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub current_cpu_percent: f64,
    pub avg_cpu_percent: f64,
    pub max_cpu_percent: f64,
    pub cpu_time_ms: f64,
    pub current_rss_bytes: u64,
    pub peak_rss_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    /// `storage`, `logical`, `all-io`, or `unavailable`.
    pub io_semantics: String,
    pub sample_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryHealth {
    pub native: TelemetrySourceHealth,
    pub desktop: TelemetrySourceHealth,
    #[serde(default, with = "crate::schema::effect_option")]
    pub sidecar_version: Option<String>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub sidecar_pid: Option<u32>,
    #[serde(default)]
    pub restart_count: u64,
    #[serde(default)]
    pub collection_duration_micros: f64,
    #[serde(default)]
    pub scanned_process_count: u64,
    #[serde(default)]
    pub retained_process_count: u64,
    #[serde(default)]
    pub inaccessible_process_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySourceHealth {
    /// `starting`, `healthy`, `degraded`, `unavailable`, or `stopped`.
    pub status: String,
    #[serde(default, with = "crate::schema::effect_option")]
    pub last_sample_at: Option<String>,
    #[serde(default, with = "crate::schema::effect_option")]
    pub last_error: Option<String>,
}

/// An item of `subscribeResourceTelemetry`; also in `server.retryResourceTelemetry`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTelemetrySnapshot {
    pub read_at: String,
    pub sample_interval_ms: u64,
    #[serde(default)]
    pub processes: Vec<TelemetryProcess>,
    pub groups: TelemetryGroups,
    pub power: HostPowerSnapshot,
    /// macOS thermal speed limit.
    #[serde(default, with = "crate::schema::effect_option")]
    pub speed_limit_percent: Option<f64>,
    pub attribution: ResourceAttributionSnapshot,
    pub health: TelemetryHealth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryProcess {
    pub identity: ProcessIdentity,
    pub ppid: u32,
    #[serde(default)]
    pub child_pids: Vec<u32>,
    pub depth: u32,
    pub name: String,
    pub command: String,
    pub status: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electron_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub electron_service_name: Option<String>,
    pub cpu_percent: f64,
    pub cpu_time_ms: f64,
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub virtual_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub io_read_bytes_per_second: f64,
    pub io_write_bytes_per_second: f64,
    pub io_semantics: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idle_wakeups_per_second: Option<f64>,
    pub run_time_ms: f64,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryGroups {
    pub backend: TelemetryAggregate,
    pub electron: TelemetryAggregate,
    pub monitor: TelemetryAggregate,
    #[serde(rename = "allT3")]
    pub all_t3: TelemetryAggregate,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TelemetryAggregate {
    pub process_count: u64,
    pub current_cpu_percent: f64,
    pub cpu_time_ms: f64,
    pub current_rss_bytes: u64,
    pub peak_rss_bytes: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub io_read_bytes_per_second: f64,
    pub io_write_bytes_per_second: f64,
    pub process_starts: u64,
    pub process_exits: u64,
}

/// Tri-state flags (`"true"`, `"false"`, `"unknown"`) are kept as strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostPowerSnapshot {
    pub source: String,
    pub idle: String,
    pub idle_seconds: Option<f64>,
    pub locked: String,
    #[serde(default)]
    pub suspended: bool,
    pub on_battery: String,
    pub low_power_mode: String,
    /// `unknown`, `nominal`, `fair`, `serious`, or `critical`.
    pub thermal_state: String,
    #[serde(default)]
    pub stale: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAttributionSnapshot {
    pub read_at: String,
    #[serde(default)]
    pub entries: Vec<ResourceAttributionEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAttributionEntry {
    pub component: String,
    pub operation: String,
    pub logical_read_bytes: u64,
    pub logical_write_bytes: u64,
    pub count: u64,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceTelemetryRetryResult {
    #[serde(default)]
    pub accepted: bool,
    pub snapshot: ResourceTelemetrySnapshot,
}

// ---------------------------------------------------------------------------------------------
// Client activity (background policy leases)

/// `server.reportClientActivity`: tells the server this client is visible/focused so its
/// background work (git fetch, provider health) keeps running for `scopes`. Send on focus and
/// visibility changes, and again before `ttl_ms` runs out.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientActivityReportInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<EnvironmentId>,
    /// Stable per client install.
    pub client_id: String,
    /// `web`, `desktop-renderer`, `mobile`, or `unknown`.
    pub client_kind: String,
    pub visible: bool,
    pub focused: bool,
    pub recently_interacted: bool,
    /// `active`, `inactive`, `background`, or `unknown`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low_power_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_type: Option<String>,
    pub scopes: Vec<BackgroundScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u64>,
    pub observed_at: String,
}

/// What a client lease keeps fresh.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BackgroundScope {
    ServerConfig,
    #[serde(rename_all = "camelCase")]
    ProviderStatus {
        #[serde(skip_serializing_if = "Option::is_none")]
        instance_id: Option<String>,
    },
    VcsStatus {
        cwd: String,
    },
    GitRefs {
        cwd: String,
    },
    Diagnostics,
    #[serde(rename_all = "camelCase")]
    Thread {
        thread_id: crate::ThreadId,
    },
}

// ---------------------------------------------------------------------------------------------
// T3 Connect relay client

/// `cloud.getRelayClientStatus` and the final item of `cloud.installRelayClient`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum RelayClientStatus {
    #[serde(rename_all = "camelCase")]
    Available {
        executable_path: String,
        /// `override`, `managed`, or `path`.
        source: String,
        version: String,
    },
    Missing {
        version: String,
    },
    Unsupported {
        platform: String,
        arch: String,
        version: String,
    },
    #[serde(other)]
    Unknown,
}

/// An item of `cloud.installRelayClient`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RelayClientInstallProgress {
    /// `checking`, `waiting_for_lock`, `downloading`, `verifying`, `installing`, ...
    Progress {
        stage: String,
    },
    Complete {
        status: RelayClientStatus,
    },
    #[serde(other)]
    Unknown,
}
