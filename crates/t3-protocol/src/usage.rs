//! Token usage, cost, and provider rate limits (`packages/contracts/src/usage.ts`,
//! `providerUsageLimits.ts`; the fork's `/usage` route and the composer's limit meters).
//!
//! - `server.getUsageSummary`: bucketed token totals and cost for a day range.
//! - `server.refreshUsageRates`: refetch model prices.
//! - Rate limits arrive on `ServerProvider.usage_limits` and, for proxied accounts, as
//!   [`UsageLimitSourceSnapshot`]s on the config stream (`usageLimitSources: true`).
//! - `provider.consumeResetCredit`: spend a limit reset credit.

use serde::{Deserialize, Serialize};

use crate::{ProviderInstanceId, open_enum, schema::forward_compatible};

open_enum! {
    /// Provider whose local logs a usage bucket was read from.
    pub enum UsageProvider {
        Claude = "claude",
        Codex = "codex",
        Grok = "grok",
        Cursor = "cursor",
        Opencode = "opencode",
        Antigravity = "antigravity",
    }
}

open_enum! {
    pub enum UsageResolution {
        Day = "day",
        Hour = "hour",
    }
}

open_enum! {
    /// How a bucket's cost was determined.
    pub enum CostSource {
        ProviderReported = "providerReported",
        ModelPriced = "modelPriced",
        Unpriced = "unpriced",
    }
}

/// `server.getUsageSummary`. Days are `YYYY-MM-DD` in `time_zone` (IANA name).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryInput {
    pub since_day: String,
    pub until_day: String,
    pub time_zone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<UsageResolution>,
    /// ISO times narrowing hour-resolution queries.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub contract_version: u32,
    pub read_at: String,
    pub time_zone: String,
    pub since_day: String,
    pub until_day: String,
    #[serde(default)]
    pub buckets: Vec<UsageBucket>,
    #[serde(default)]
    pub sources: Vec<UsageSource>,
    pub pricing: UsagePricing,
    #[serde(default)]
    pub scan_duration_ms: f64,
}

/// Totals for one (day or hour, provider, model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub day: String,
    /// Set at hour resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hour_start: Option<String>,
    pub provider: UsageProvider,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub totals: UsageTokenTotals,
    #[serde(default)]
    pub cost_usd: f64,
    #[serde(default)]
    pub cache_savings_usd: f64,
    pub cost_source: CostSource,
    #[serde(default)]
    pub records: u64,
    #[serde(default)]
    pub unpriced_records: u64,
    #[serde(default)]
    pub sessions: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageTokenTotals {
    pub uncached_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}

/// One scanned log location and how the scan went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSource {
    pub fingerprint: UsageSourceFingerprint,
    /// `ok`, `missing`, `partial`, or `failed`.
    pub status: String,
    #[serde(default)]
    pub scanned_files: u64,
    #[serde(default)]
    pub skipped_files: u64,
    #[serde(default)]
    pub malformed_records: u64,
    #[serde(default)]
    pub distinct_sessions: u64,
    pub message: Option<String>,
    /// `enableCursorKeychain`: the UI offers a button that sets
    /// `cursorKeychainUsageEnabled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSourceFingerprint {
    pub host_id: String,
    pub provider: UsageProvider,
    pub resolved_home_path: String,
    pub volume_id: String,
}

/// Model price table state. Also the result of `server.refreshUsageRates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePricing {
    /// `fresh`, `cached`, or `unavailable`.
    pub status: String,
    pub source: String,
    pub fetched_at: Option<String>,
    #[serde(default)]
    pub known_models: u64,
}

// ---------------------------------------------------------------------------------------------
// Rate limits

open_enum! {
    pub enum UsageWindowKind {
        Session = "session",
        Weekly = "weekly",
        Monthly = "monthly",
        /// The literal `"other"` (distinct from an unknown kind, which is `Other(..)`).
        Misc = "other",
    }
}

/// A provider's rate limits (`ServerProvider.usage_limits`, and per proxied account).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProviderUsageLimits {
    pub checked_at: String,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub windows: Vec<UsageWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits: Option<ResetCredits>,
    /// A link to the provider's own usage page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_usage: Option<ExternalUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<UsageLimitsUnavailable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub id: String,
    pub kind: UsageWindowKind,
    pub label: String,
    /// 0 to 100.
    pub used_percent: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_duration_mins: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetCredits {
    pub available_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_expires_at: Option<String>,
    /// Pass to `provider.consumeResetCredit` for proxied accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_credit_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalUsage {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageLimitsUnavailable {
    /// `unsupported` or `probeFailed`.
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A rate-limit proxy (`cliproxy`) and the accounts behind it. Arrives in
/// `ServerConfig.usage_limit_sources` and `usageLimitSourcesUpdated` when subscribed with
/// `usageLimitSources: true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitSourceSnapshot {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub checked_at: String,
    #[serde(default, deserialize_with = "forward_compatible")]
    pub accounts: Vec<UsageLimitSourceAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimitSourceAccount {
    pub id: String,
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    pub usage_limits: ServerProviderUsageLimits,
}

/// `provider.consumeResetCredit`: for a provider instance, or for an account behind a
/// usage-limit source.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ConsumeResetCreditInput {
    #[serde(rename_all = "camelCase")]
    Provider { instance_id: ProviderInstanceId },
    #[serde(rename_all = "camelCase")]
    Source {
        source_id: String,
        account_id: String,
        credit_id: String,
    },
}

open_enum! {
    pub enum ResetCreditOutcome {
        Reset = "reset",
        NothingToReset = "nothingToReset",
        NoCredit = "noCredit",
        AlreadyRedeemed = "alreadyRedeemed",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumeResetCreditResult {
    pub outcome: ResetCreditOutcome,
    pub warning: Option<String>,
}
