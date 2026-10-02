//! The Usage page's logic, ported from the fork's `packages/shared/src/usage{Format,Merge,Limits}.ts`
//! and `apps/web/src/components/usage/*.ts` (see `docs/spec/pages.md` section 3).
//!
//! - [`format`]: number and date labels (`formatUsd`, `formatTokens`, `formatHourShort`, ...).
//! - [`window`]: the requested window (`makeWindow`) and its day/hour periods.
//! - [`merge`]: per-environment summaries merged into one view (`mergeUsage`).
//! - [`chart`]: the provider chart's scale, curves and hover columns.
//! - [`limits`]: subscription limits pooled per provider (`collectLimitPools`).
//! - [`Metric`], [`Period`], [`ProviderKind`]: page options and provider presentation order.

pub mod chart;
pub mod format;
pub mod limits;
pub mod merge;
pub mod window;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use t3_protocol::usage::UsageProvider;

/// What the page shows (`METRIC_OPTIONS`, `usageShortcuts.ts:5-10`). Persisted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Cost,
    Tokens,
    /// First-visit default: most people open the page to see quota left.
    #[default]
    Limits,
}

impl Metric {
    pub const ALL: [Self; 3] = [Self::Cost, Self::Tokens, Self::Limits];

    pub fn label(self) -> &'static str {
        match self {
            Self::Cost => "Cost",
            Self::Tokens => "Tokens",
            Self::Limits => "Limits",
        }
    }

    /// Keybinding command that selects it (`usage.cost`, ...).
    pub fn command(self) -> &'static str {
        match self {
            Self::Cost => "usage.cost",
            Self::Tokens => "usage.tokens",
            Self::Limits => "usage.limits",
        }
    }
}

/// The window length (`WINDOW_OPTIONS`, `usageShortcuts.ts:12-17`). Persisted as days.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub enum Period {
    /// Past 24h, hourly buckets.
    Day,
    Week,
    #[default]
    Month,
    Quarter,
}

impl Period {
    pub const ALL: [Self; 4] = [Self::Day, Self::Week, Self::Month, Self::Quarter];

    pub fn days(self) -> u32 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 30,
            Self::Quarter => 90,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Day => "Past 24h",
            Self::Week => "7 days",
            Self::Month => "30 days",
            Self::Quarter => "90 days",
        }
    }

    pub fn command(self) -> &'static str {
        match self {
            Self::Day => "usage.period.day",
            Self::Week => "usage.period.week",
            Self::Month => "usage.period.month",
            Self::Quarter => "usage.period.quarter",
        }
    }

    /// Hourly buckets for the past 24h, daily otherwise.
    pub fn is_hourly(self) -> bool {
        self == Self::Day
    }
}

impl TryFrom<u32> for Period {
    type Error = String;

    fn try_from(days: u32) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|period| period.days() == days)
            .ok_or_else(|| format!("unsupported usage window: {days} days"))
    }
}

impl From<Period> for u32 {
    fn from(period: Period) -> Self {
        period.days()
    }
}

/// Usage page preferences (`usagePagePreferences.ts`), stored in `ui-state.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsagePreferences {
    pub metric: Metric,
    #[serde(rename = "windowDays")]
    pub period: Period,
}

/// Decodes a persisted field, or its default when the value has the wrong shape.
pub(crate) fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// A provider the usage contract knows, in the page's reading order (`PROVIDER_PRESENTATION`,
/// `usageProviders.ts:24-47`). Unknown providers from a newer server are skipped, like the
/// web's forward-compatible decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderKind {
    Codex,
    Claude,
    Grok,
    Cursor,
    Opencode,
    Antigravity,
}

impl ProviderKind {
    /// Declaration order, reused by every chart, summary, table and tooltip.
    pub const ORDER: [Self; 6] = [
        Self::Codex,
        Self::Claude,
        Self::Grok,
        Self::Cursor,
        Self::Opencode,
        Self::Antigravity,
    ];

    pub fn from_wire(provider: &UsageProvider) -> Option<Self> {
        match provider {
            UsageProvider::Codex => Some(Self::Codex),
            UsageProvider::Claude => Some(Self::Claude),
            UsageProvider::Grok => Some(Self::Grok),
            UsageProvider::Cursor => Some(Self::Cursor),
            UsageProvider::Opencode => Some(Self::Opencode),
            UsageProvider::Antigravity => Some(Self::Antigravity),
            UsageProvider::Other(_) => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Grok => "Grok Build",
            Self::Cursor => "Cursor",
            Self::Opencode => "OpenCode",
            Self::Antigravity => "Antigravity",
        }
    }

    /// Position in [`Self::ORDER`].
    pub fn index(self) -> usize {
        self as usize
    }
}
