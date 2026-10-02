//! Subscription limits pooled per provider for the Usage page's Limits tab
//! (`packages/shared/src/usageLimits.ts`, `components/usage/UsageLimitsPooled.logic.ts`).
//!
//! Inputs are plain per-environment snapshots ([`LimitsEnvironment`]) so this stays testable;
//! `t3-app` builds them from each environment's `ServerConfig`. Times are epoch milliseconds.

use t3_protocol::{
    server::{ProviderAuthStatus, ServerProvider},
    usage::{
        ConsumeResetCreditInput, ExternalUsage, ServerProviderUsageLimits,
        UsageLimitSourceSnapshot, UsageWindow, UsageWindowKind,
    },
};

use crate::time::parse_timestamp;

const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

/// One environment's limit sources: its providers that report limits, and the usage-limit hubs
/// it is subscribed to.
#[derive(Clone, Debug, Default)]
pub struct LimitsEnvironment {
    pub environment_id: String,
    pub label: String,
    /// Already filtered by [`LimitProvider::from_server`].
    pub providers: Vec<LimitProvider>,
    pub sources: Vec<UsageLimitSourceSnapshot>,
}

/// A provider instance that belongs on the Limits view (`providersWithLimits`).
#[derive(Clone, Debug)]
pub struct LimitProvider {
    pub instance_id: String,
    /// `codex`, `claudeAgent`, `cursor`, ...
    pub driver: String,
    pub display_name: Option<String>,
    pub accent_color: Option<String>,
    pub email: Option<String>,
    /// `auth.label`, the plan as the provider names it.
    pub plan: Option<String>,
    pub authenticated: bool,
    pub limits: Option<ServerProviderUsageLimits>,
}

impl LimitProvider {
    /// The provider when it is enabled, installed, available and reports limits at all.
    pub fn from_server(provider: &ServerProvider) -> Option<Self> {
        let available = provider.availability.as_deref() != Some("unavailable");
        let limits = provider.usage_limits.clone()?;
        (provider.enabled && provider.installed && available).then(|| Self {
            instance_id: provider.instance_id.to_string(),
            driver: provider.driver.clone(),
            display_name: provider.display_name.clone(),
            accent_color: provider.accent_color.clone(),
            email: provider.auth.email.clone(),
            plan: provider.auth.label.clone(),
            authenticated: provider.auth.status == ProviderAuthStatus::Authenticated,
            limits: Some(limits),
        })
    }

    fn name(&self) -> String {
        self.display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(&self.driver)
            .to_owned()
    }
}

/// An environment an account is signed in on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitEnvironment {
    pub environment_id: String,
    pub label: String,
}

/// Where a reset credit is redeemed.
#[derive(Clone, Debug)]
pub struct Redeem {
    pub environment_id: String,
    pub input: ConsumeResetCreditInput,
}

impl Redeem {
    fn is_hub(&self) -> bool {
        matches!(self.input, ConsumeResetCreditInput::Source { .. })
    }
}

/// One subscription account, however it was reported (`LimitAccount`).
#[derive(Clone, Debug)]
pub struct LimitAccount {
    pub key: String,
    pub driver: String,
    /// The instance's configured name; `None` for hub accounts with an email.
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub accent_color: Option<String>,
    /// Empty when only a hub reports it.
    pub environments: Vec<LimitEnvironment>,
    /// The hub that reported it, when no environment has it natively.
    pub source_label: Option<String>,
    pub redeem: Option<Redeem>,
    pub limits: ServerProviderUsageLimits,
}

impl LimitAccount {
    fn checked_at(&self) -> i64 {
        parse_timestamp(&self.limits.checked_at).unwrap_or(i64::MIN)
    }

    fn sort_name(&self) -> String {
        self.display_name
            .as_deref()
            .or(self.email.as_deref())
            .unwrap_or(&self.key)
            .to_lowercase()
    }
}

/// The line under a provider when it has no bars (`limitsNotice`).
pub fn limits_notice(limits: &ServerProviderUsageLimits) -> Option<String> {
    if let Some(unavailable) = &limits.unavailable {
        match unavailable.reason.as_str() {
            "unsupported" => {
                return Some(
                    unavailable
                        .message
                        .clone()
                        .unwrap_or_else(|| "This account has no subscription limits.".into()),
                );
            }
            "probeFailed" => {
                return Some(
                    unavailable
                        .message
                        .clone()
                        .unwrap_or_else(|| "Could not read limits.".into()),
                );
            }
            _ => {}
        }
    }
    limits
        .windows
        .is_empty()
        .then(|| "No limits reported.".into())
}

/// The merge key: the email (case-insensitive), else an identical credential.
fn account_key(
    driver: &str,
    email: Option<&str>,
    limits: &ServerProviderUsageLimits,
) -> Option<String> {
    if let Some(email) = email
        .map(|email| email.trim().to_lowercase())
        .filter(|email| !email.is_empty())
    {
        return Some(format!("{driver}:{email}"));
    }
    limits
        .credential_fingerprint
        .as_ref()
        .map(|fingerprint| format!("{driver}:credential:{fingerprint}"))
}

/// Every account with usable windows across `environments`, one entry per distinct account
/// (`collectLimitAccounts`). The freshest read supplies windows; the freshest credit read supplies
/// credits; native instances supply names and environment labels.
pub fn collect_limit_accounts(environments: &[LimitsEnvironment]) -> Vec<LimitAccount> {
    let mut accounts: Vec<(String, LimitAccount)> = Vec::new();
    let mut credit_sources: Vec<(String, LimitAccount)> = Vec::new();
    let mut hub_redeems: Vec<(String, LimitAccount)> = Vec::new();

    let mut merge = |key: String, next: LimitAccount| {
        // A hub redeem also clears the hub's routing cooldown, so it wins the redemption.
        if next.redeem.as_ref().is_some_and(Redeem::is_hub) {
            match hub_redeems.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, previous)) if next.checked_at() > previous.checked_at() => {
                    *previous = next.clone();
                }
                Some(_) => {}
                None => hub_redeems.push((key.clone(), next.clone())),
            }
        }
        if next.limits.reset_credits.is_some() {
            match credit_sources.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, previous)) if next.checked_at() > previous.checked_at() => {
                    *previous = next.clone();
                }
                Some(_) => {}
                None => credit_sources.push((key.clone(), next.clone())),
            }
        }
        let Some((_, previous)) = accounts.iter_mut().find(|(seen, _)| *seen == key) else {
            accounts.push((key, next));
            return;
        };
        let mut environments = previous.environments.clone();
        for candidate in &next.environments {
            if !environments
                .iter()
                .any(|seen| seen.environment_id == candidate.environment_id)
            {
                environments.push(candidate.clone());
            }
        }
        let winner = if next.checked_at() > previous.checked_at() {
            &next
        } else {
            &*previous
        };
        let credit_source = credit_sources
            .iter()
            .find(|(seen, _)| *seen == key)
            .map(|(_, account)| account);
        let redeem = hub_redeems
            .iter()
            .find(|(seen, _)| *seen == key)
            .and_then(|(_, account)| account.redeem.clone())
            .or_else(|| match credit_source {
                Some(source) => source.redeem.clone(),
                None => winner
                    .redeem
                    .clone()
                    .or_else(|| previous.redeem.clone())
                    .or_else(|| next.redeem.clone()),
            });
        let mut limits = winner.limits.clone();
        limits.reset_credits = credit_source.and_then(|source| source.limits.reset_credits.clone());
        let source_label = if environments.is_empty() {
            previous.source_label.clone().or(next.source_label.clone())
        } else {
            None
        };
        *previous = LimitAccount {
            key: previous.key.clone(),
            driver: previous.driver.clone(),
            display_name: previous.display_name.clone().or(next.display_name.clone()),
            email: previous.email.clone(),
            plan: previous.plan.clone().or(next.plan.clone()),
            accent_color: previous.accent_color.clone().or(next.accent_color.clone()),
            environments,
            source_label,
            redeem,
            limits,
        };
    };

    for environment in environments {
        for provider in &environment.providers {
            let Some(limits) = &provider.limits else {
                continue;
            };
            if limits_notice(limits).is_some() {
                continue;
            }
            let instance_key = format!("{}:{}", environment.environment_id, provider.instance_id);
            merge(
                account_key(&provider.driver, provider.email.as_deref(), limits)
                    .unwrap_or_else(|| instance_key.clone()),
                LimitAccount {
                    key: instance_key,
                    driver: provider.driver.clone(),
                    display_name: provider
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(Into::into),
                    email: provider.email.clone(),
                    plan: provider.plan.clone(),
                    accent_color: provider.accent_color.clone(),
                    environments: vec![LimitEnvironment {
                        environment_id: environment.environment_id.clone(),
                        label: environment.label.clone(),
                    }],
                    source_label: None,
                    redeem: Some(Redeem {
                        environment_id: environment.environment_id.clone(),
                        input: ConsumeResetCreditInput::Provider {
                            instance_id: provider.instance_id.as_str().into(),
                        },
                    }),
                    limits: limits.clone(),
                },
            );
        }
    }
    let label_environment = environments.len() > 1;
    for environment in environments {
        for source in &environment.sources {
            let source_label = if label_environment {
                format!("{} · {}", environment.label, source.label)
            } else {
                source.label.clone()
            };
            for account in &source.accounts {
                if limits_notice(&account.usage_limits).is_some() {
                    continue;
                }
                let source_key = format!("{}:{}", source.id, account.id);
                let redeem = account
                    .usage_limits
                    .reset_credits
                    .as_ref()
                    .and_then(|credits| credits.next_credit_id.clone())
                    .map(|credit_id| Redeem {
                        environment_id: environment.environment_id.clone(),
                        input: ConsumeResetCreditInput::Source {
                            source_id: source.id.clone(),
                            account_id: account.id.clone(),
                            credit_id,
                        },
                    });
                merge(
                    account_key(
                        &account.driver,
                        account.email.as_deref(),
                        &account.usage_limits,
                    )
                    .unwrap_or_else(|| source_key.clone()),
                    LimitAccount {
                        key: source_key,
                        driver: account.driver.clone(),
                        display_name: account.email.is_none().then(|| {
                            account
                                .id
                                .strip_suffix(".json")
                                .or_else(|| account.id.strip_suffix(".JSON"))
                                .unwrap_or(&account.id)
                                .to_owned()
                        }),
                        email: account.email.clone(),
                        plan: account.plan.clone(),
                        accent_color: None,
                        environments: Vec::new(),
                        source_label: Some(source_label.clone()),
                        redeem,
                        limits: account.usage_limits.clone(),
                    },
                );
            }
        }
    }
    collapse_identical_accounts(accounts.into_iter().map(|(_, account)| account).collect())
}

/// What an account shows, so two snapshots of it can collapse (`displayedLimitSignature`).
fn displayed_signature(account: &LimitAccount) -> String {
    let mut windows: Vec<String> = account
        .limits
        .windows
        .iter()
        .map(|window| {
            format!(
                "{}|{}|{}|{}|{:?}|{:?}",
                window.id,
                window.kind,
                window.label,
                window.used_percent,
                window.resets_at,
                window.window_duration_mins
            )
        })
        .collect();
    windows.sort();
    let credits = account
        .limits
        .reset_credits
        .as_ref()
        .map(|credits| (credits.available_count, credits.next_expires_at.clone()));
    format!(
        "{}\u{0}{:?}\u{0}{:?}\u{0}{:?}\u{0}{:?}\u{0}{:?}\u{0}{:?}\u{0}{:?}",
        account.driver,
        account.display_name,
        account.email,
        account.plan,
        account.accent_color,
        account.source_label,
        windows,
        credits
    )
}

/// Collapses duplicate snapshots without hiding a visible account or quota difference
/// (`collapseIdenticalLimitAccounts`).
pub fn collapse_identical_accounts(accounts: Vec<LimitAccount>) -> Vec<LimitAccount> {
    let mut collapsed: Vec<(String, LimitAccount)> = Vec::new();
    for account in accounts {
        let signature = displayed_signature(&account);
        let Some((_, previous)) = collapsed.iter_mut().find(|(seen, _)| *seen == signature) else {
            collapsed.push((signature, account));
            continue;
        };
        for candidate in &account.environments {
            if !previous
                .environments
                .iter()
                .any(|seen| seen.environment_id == candidate.environment_id)
            {
                previous.environments.push(candidate.clone());
            }
        }
        let fresher = account.checked_at() > previous.checked_at();
        let redeem = if fresher {
            account.redeem.clone()
        } else {
            previous.redeem.clone()
        };
        previous.redeem = redeem
            .or(previous.redeem.clone())
            .or(account.redeem.clone());
        if fresher {
            previous.limits = account.limits;
        }
    }
    collapsed.into_iter().map(|(_, account)| account).collect()
}

/// Lines for sources that could not be drawn as bars (`collectLimitNotices`). Accounts that can
/// never report (API keys) are left out. Environments are named only when several are shown.
pub fn collect_limit_notices(environments: &[LimitsEnvironment]) -> Vec<String> {
    let several = environments.len() > 1;
    let label = |environment: &str, subject: &str| {
        if several {
            format!("{environment} · {subject}")
        } else {
            subject.to_owned()
        }
    };
    let mut notices = Vec::new();
    for environment in environments {
        for provider in &environment.providers {
            let Some(limits) = &provider.limits else {
                continue;
            };
            if limits
                .unavailable
                .as_ref()
                .is_some_and(|unavailable| unavailable.reason == "unsupported")
            {
                continue;
            }
            if let Some(notice) = limits_notice(limits) {
                notices.push(format!(
                    "{}: {notice}",
                    label(&environment.label, &provider.name())
                ));
            }
        }
        for source in &environment.sources {
            if let Some(error) = &source.error {
                notices.push(format!(
                    "{}: {error}",
                    label(&environment.label, &source.label)
                ));
            } else if source.accounts.is_empty() {
                notices.push(format!(
                    "{}: No accounts reported.",
                    label(&environment.label, &source.label)
                ));
            }
        }
    }
    notices
}

/// A provider-owned usage page (`collectExternalUsageLinks`), one per URL.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalUsageLink {
    pub label: String,
    pub url: String,
    pub message: Option<String>,
}

/// The ChatGPT usage page; its link row gets the OpenAI mark and fixed copy.
pub const CHATGPT_USAGE_URL: &str = "https://chatgpt.com/#settings/Usage";

pub fn collect_external_usage_links(environments: &[LimitsEnvironment]) -> Vec<ExternalUsageLink> {
    let mut links: Vec<ExternalUsageLink> = Vec::new();
    for provider in environments
        .iter()
        .flat_map(|environment| &environment.providers)
    {
        let Some(limits) = &provider.limits else {
            continue;
        };
        let Some(ExternalUsage { label, url }) = &limits.external_usage else {
            continue;
        };
        if !provider.authenticated {
            continue;
        }
        let link = ExternalUsageLink {
            label: label.clone(),
            url: url.clone(),
            message: limits
                .unavailable
                .as_ref()
                .and_then(|unavailable| unavailable.message.clone()),
        };
        match links.iter_mut().find(|seen| seen.url == *url) {
            Some(seen) => *seen = link,
            None => links.push(link),
        }
    }
    links
}

/// Spending against the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitPace {
    /// Spending faster than the window elapses.
    Ahead,
    On,
    /// Headroom left for the rest of the window.
    Under,
}

impl LimitPace {
    /// The tooltip (`UsageLimits.tsx:41-45`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Ahead => "Ahead of pace: spending faster than the window elapses",
            Self::On => "On pace with the window",
            Self::Under => "Under pace: headroom left for the rest of the window",
        }
    }
}

/// JS `Math.round`: half up.
fn round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// Quota left, 0..=100.
pub fn remaining_percent(window: &UsageWindow) -> i64 {
    round(100. - window.used_percent.clamp(0., 100.))
}

fn reset_millis(window: &UsageWindow) -> Option<i64> {
    window.resets_at.as_deref().and_then(parse_timestamp)
}

/// Elapsed share of the window, 0..=1, or `None` without a reset time and length.
pub fn elapsed_share(window: &UsageWindow, now: i64) -> Option<f64> {
    let resets_at = reset_millis(window)?;
    let length = i64::try_from(window.window_duration_mins?).ok()? * MINUTE;
    if length <= 0 {
        return None;
    }
    Some(((length - (resets_at - now)) as f64 / length as f64).clamp(0., 1.))
}

fn pace_of_shares(used_percent: f64, elapsed: f64) -> LimitPace {
    let gap = used_percent - elapsed * 100.;
    if gap > 5. {
        LimitPace::Ahead
    } else if gap < -5. {
        LimitPace::Under
    } else {
        LimitPace::On
    }
}

/// Within five points of even spending is on pace.
pub fn pace_of(window: &UsageWindow, now: i64) -> Option<LimitPace> {
    elapsed_share(window, now).map(|elapsed| pace_of_shares(window.used_percent, elapsed))
}

/// `2h 13m`, `3d 4h`, `12m`.
pub fn format_duration(ms: i64) -> String {
    let remaining = ms.max(0);
    let days = remaining / DAY;
    let hours = (remaining % DAY) / HOUR;
    let minutes = (remaining % HOUR) / MINUTE;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// `resets in 2h 13m`, `resets now` once past, `None` without a reset.
pub fn format_resets_in(window: &UsageWindow, now: i64) -> Option<String> {
    let resets_at = reset_millis(window)?;
    Some(if resets_at <= now {
        "resets now".to_owned()
    } else {
        format!("resets in {}", format_duration(resets_at - now))
    })
}

/// One account's window in a pool.
#[derive(Clone, Debug)]
pub struct PoolColumn {
    pub account: LimitAccount,
    /// `None` leaves a gap: the account does not report this window.
    pub window: Option<UsageWindow>,
}

/// A reset in a pool, soonest first.
#[derive(Clone, Debug)]
pub struct PoolReset {
    pub account_key: String,
    pub at: i64,
    /// Points of the pool it restores: the member's used share over the member count.
    pub restores_percent: i64,
}

/// One window id pooled across every account that reports it (`LimitPoolWindow`).
#[derive(Clone, Debug)]
pub struct LimitPoolWindow {
    pub id: String,
    pub kind: UsageWindowKind,
    pub label: String,
    pub columns: Vec<PoolColumn>,
    pub remaining_percent: i64,
    pub used_percent: i64,
    pub pace: Option<LimitPace>,
    pub resets: Vec<PoolReset>,
}

impl LimitPoolWindow {
    /// The reset for `account_key`, if it has one.
    pub fn reset_for(&self, account_key: &str) -> Option<&PoolReset> {
        self.resets
            .iter()
            .find(|reset| reset.account_key == account_key)
    }

    /// The soonest reset that hands anything back.
    pub fn next_refill(&self) -> Option<&PoolReset> {
        self.resets.iter().find(|reset| reset.restores_percent > 0)
    }
}

/// Accounts of one driver and their pooled windows (`LimitPool`).
#[derive(Clone, Debug)]
pub struct LimitPool {
    pub driver: String,
    pub accounts: Vec<LimitAccount>,
    pub windows: Vec<LimitPoolWindow>,
}

/// Cursor's fixed window labels and descriptions (`CURSOR_USAGE_WINDOWS`).
pub const CURSOR_USAGE_WINDOWS: [(&str, &str, &str); 3] = [
    (
        "totalPercentUsed",
        "Overall",
        "Combined usage across both allowances, not a third quota.",
    ),
    (
        "autoPercentUsed",
        "Cursor Models",
        "Grok and Composer use this first. Auto can use either pool.",
    ),
    (
        "apiPercentUsed",
        "Other Models",
        "Claude, GPT, and Gemini use this pool. Grok and Composer fall back here.",
    ),
];

/// `(label, description)` for a Cursor window id.
pub fn cursor_window_details(id: &str) -> Option<(&'static str, &'static str)> {
    CURSOR_USAGE_WINDOWS
        .iter()
        .find(|(window, _, _)| *window == id)
        .map(|(_, label, description)| (*label, *description))
}

impl LimitPool {
    /// The windows to draw: Cursor shows its two pools instead of the combined percentage when
    /// both are reported (`displayLimitWindows`).
    pub fn display_windows(&self) -> Vec<&LimitPoolWindow> {
        if self.driver != "cursor" {
            return self.windows.iter().collect();
        }
        let has = |id: &str| self.windows.iter().any(|window| window.id == id);
        let both = has("autoPercentUsed") && has("apiPercentUsed");
        let rank = |id: &str| {
            CURSOR_USAGE_WINDOWS
                .iter()
                .position(|(window, _, _)| *window == id)
                .unwrap_or(CURSOR_USAGE_WINDOWS.len())
        };
        let mut windows: Vec<_> = self
            .windows
            .iter()
            .filter(|window| !both || window.id != "totalPercentUsed")
            .collect();
        windows.sort_by_key(|window| rank(&window.id));
        windows
    }
}

fn kind_order(kind: &UsageWindowKind) -> u8 {
    match kind {
        UsageWindowKind::Session => 0,
        UsageWindowKind::Weekly => 1,
        UsageWindowKind::Monthly => 2,
        _ => 3,
    }
}

/// Accounts grouped by driver, each with its windows pooled by kind and id (`collectLimitPools`).
pub fn collect_limit_pools(accounts: &[LimitAccount], now: i64) -> Vec<LimitPool> {
    let mut by_driver: Vec<(String, Vec<LimitAccount>)> = Vec::new();
    for account in accounts {
        match by_driver
            .iter_mut()
            .find(|(driver, _)| *driver == account.driver)
        {
            Some((_, members)) => members.push(account.clone()),
            None => by_driver.push((account.driver.clone(), vec![account.clone()])),
        }
    }
    by_driver
        .into_iter()
        .map(|(driver, mut members)| {
            let mut all_windows: Vec<&UsageWindow> = members
                .iter()
                .flat_map(|account| &account.limits.windows)
                .collect();
            all_windows.sort_by_key(|window| kind_order(&window.kind));
            let order = all_windows
                .first()
                .map(|window| (window.kind.clone(), window.id.clone()));
            let order_reset = |account: &LimitAccount| {
                order
                    .as_ref()
                    .and_then(|(kind, id)| {
                        account
                            .limits
                            .windows
                            .iter()
                            .find(|window| window.kind == *kind && window.id == *id)
                    })
                    .and_then(reset_millis)
                    .unwrap_or(i64::MAX)
            };
            members.sort_by(|left, right| {
                order_reset(left)
                    .cmp(&order_reset(right))
                    .then_with(|| left.sort_name().cmp(&right.sort_name()))
                    .then_with(|| left.key.cmp(&right.key))
            });
            let windows = pool_windows(&members, now);
            LimitPool {
                driver,
                accounts: members,
                windows,
            }
        })
        .collect()
}

fn pool_windows(accounts: &[LimitAccount], now: i64) -> Vec<LimitPoolWindow> {
    /// `(kind, id)` and the `(account index, window)` members reporting it.
    type PoolMembers<'a> = ((UsageWindowKind, String), Vec<(usize, &'a UsageWindow)>);
    let mut by_key: Vec<PoolMembers> = Vec::new();
    for (index, account) in accounts.iter().enumerate() {
        for window in &account.limits.windows {
            let key = (window.kind.clone(), window.id.clone());
            match by_key.iter_mut().find(|(seen, _)| *seen == key) {
                Some((_, members)) => members.push((index, window)),
                None => by_key.push((key, vec![(index, window)])),
            }
        }
    }
    let mut pools: Vec<LimitPoolWindow> = by_key
        .into_iter()
        .map(|(_, members)| {
            let first = members[0].1;
            let count = members.len() as f64;
            let used = members
                .iter()
                .map(|(_, window)| window.used_percent)
                .sum::<f64>()
                / count;
            // Pace only over members with a clock.
            let timed: Vec<(f64, f64)> = members
                .iter()
                .filter_map(|(_, window)| {
                    elapsed_share(window, now).map(|share| (window.used_percent, share))
                })
                .collect();
            let pace = (!timed.is_empty()).then(|| {
                let n = timed.len() as f64;
                let timed_used = timed.iter().map(|(used, _)| used).sum::<f64>() / n;
                let mean_elapsed = timed.iter().map(|(_, elapsed)| elapsed).sum::<f64>() / n;
                pace_of_shares(timed_used, mean_elapsed)
            });
            let mut resets: Vec<PoolReset> = members
                .iter()
                .filter_map(|(index, window)| {
                    reset_millis(window).map(|at| PoolReset {
                        account_key: accounts[*index].key.clone(),
                        at,
                        restores_percent: round(window.used_percent / count),
                    })
                })
                .collect();
            resets.sort_by_key(|reset| reset.at);
            let columns = accounts
                .iter()
                .enumerate()
                .map(|(index, account)| PoolColumn {
                    account: account.clone(),
                    window: members
                        .iter()
                        .find(|(member, _)| *member == index)
                        .map(|(_, window)| (*window).clone()),
                })
                .collect();
            LimitPoolWindow {
                id: first.id.clone(),
                kind: first.kind.clone(),
                label: first.label.clone(),
                columns,
                used_percent: round(used),
                remaining_percent: round(100. - used),
                pace,
                resets,
            }
        })
        .collect();
    pools.sort_by_key(|pool| kind_order(&pool.kind));
    pools
}

/// The provider label for a driver (`getDriverOption(driver).label`), for pool headings.
pub fn driver_label(driver: &str) -> String {
    match driver {
        "codex" => "Codex".into(),
        "claudeAgent" => "Claude".into(),
        "cursor" => "Cursor".into(),
        "opencode" => "OpenCode".into(),
        "grok" => "Grok".into(),
        "antigravity" => "Antigravity".into(),
        other => other.into(),
    }
}
