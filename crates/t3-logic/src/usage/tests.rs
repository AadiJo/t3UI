//! Usage logic tests. Failure modes, written before the code:
//!
//! Formatting
//! 1. `formatTokens` picks the wrong suffix at a boundary (999 vs 1K, 1e6, 1e9, 1e12).
//! 2. `formatTokens` trims like JS `.replace(/\.0+$/, "")`: only all-zero decimals go ("1.50K"
//!    stays, "2.00M" becomes "2M").
//! 3. Rounding differs from JS on exact ties: `toFixed` and `Intl` round half away from zero,
//!    Rust's formatter rounds half to even (2.5 -> "3", 0.125 -> "0.13").
//! 4. `formatUsd` drops thousands separators or the sign.
//! 5. `formatPercent` prints "0.0%" for a tiny non-zero share instead of "<0.1%".
//! 6. Day and hour labels: wrong month name, 0-padded day, "0 AM" instead of "12 AM".
//! 7. Relative hour labels: today/yesterday decided in UTC instead of the viewer's zone.
//!
//! Window
//! 8. Daily window start drifts across month/year boundaries.
//! 9. Hourly window is not minute-aligned or not exactly 24h; days come from UTC, not the zone.
//! 10. Period enumeration is off by one (inclusive days, exclusive hour end) or loops on bad
//!     input.
//!
//! Merge
//! 11. An incompatible contract version is merged, or excluded with the wrong direction.
//! 12. Two environments reading one transcript directory double count it; the wrong one wins
//!     (ok beats partial, then newest readAt, then environment id).
//! 13. A newer partial scan adds cells the owner already has (double count) or loses new ones.
//! 14. Sessions are summed from buckets instead of `distinctSessions`.
//! 15. Reasoning tokens are added on top of output.
//! 16. Shares divide by zero (NaN) when totals are zero.
//! 17. Sort orders: providers by cost, models by cost then tokens, days and hours ascending.
//! 18. Unknown providers from a newer server break or leak into totals.
//!
//! Chart
//! 19. `niceScale` leaves the peak above the top tick, or accumulates float error in ticks.
//! 20. Monotone curve overshoots on spikes or flat runs.
//!
//! Limits
//! 21. Remaining percent is not clamped and rounded.
//! 22. Pace thresholds are not +-5 points; windows without a clock get a pace.
//! 23. Durations: negative times, "0m", days with hours.
//! 24. Accounts: the same email in two environments is two accounts; case matters; accounts
//!     with a notice are drawn as bars.
//! 25. Pools: windows pool across kinds, order is not session/weekly/monthly/other, restores are
//!     not rounded, columns lose an account that lacks a window.
//! 26. Notices: unsupported accounts produce a notice; the environment is named with only one
//!     environment connected.

use chrono::{FixedOffset, TimeZone as _, Utc};
use t3_protocol::usage::{
    CostSource, ServerProviderUsageLimits, UsageBucket, UsageLimitsUnavailable, UsagePricing,
    UsageProvider, UsageSource, UsageSourceFingerprint, UsageSummary, UsageTokenTotals,
    UsageWindow, UsageWindowKind,
};

use super::{
    Metric, Period, ProviderKind, UsagePreferences,
    chart::{monotone_curve, nice_scale},
    format::{
        format_count, format_date_time_short, format_day_short, format_hour_short, format_percent,
        format_relative_hour_short, format_tokens, format_usd, to_fixed,
    },
    limits::{
        LimitPace, LimitProvider, LimitsEnvironment, collect_limit_accounts, collect_limit_notices,
        collect_limit_pools, elapsed_share, format_duration, format_resets_in, pace_of,
        remaining_percent,
    },
    merge::{ContractMismatchDirection, EnvironmentUsage, merge_usage},
    window::{enumerate_days, enumerate_hour_starts, make_window},
};

// ---------------------------------------------------------------------------------------------
// Formatting

#[test]
fn tokens_pick_suffixes_at_boundaries() {
    assert_eq!(format_tokens(0.), "0");
    assert_eq!(format_tokens(999.), "999");
    assert_eq!(format_tokens(1_000.), "1K");
    assert_eq!(format_tokens(804_000.), "804K");
    assert_eq!(format_tokens(76_700_000.), "76.7M");
    assert_eq!(format_tokens(19_900_000_000.), "19.9B");
    assert_eq!(format_tokens(2_000_000_000_000.), "2T");
}

#[test]
fn tokens_trim_only_all_zero_decimals() {
    assert_eq!(format_tokens(1_500.), "1.50K");
    assert_eq!(format_tokens(2_000_000.), "2M");
    assert_eq!(format_tokens(12_000.), "12K");
    assert_eq!(format_tokens(12_340.), "12.3K");
}

#[test]
fn ties_round_away_from_zero_like_js() {
    assert_eq!(to_fixed(2.5, 0), "3");
    assert_eq!(to_fixed(0.125, 2), "0.13");
    assert_eq!(to_fixed(-2.5, 0), "-3");
    // 1.005 is 1.00499999999999989... in binary: not a tie.
    assert_eq!(to_fixed(1.005, 2), "1.00");
    assert_eq!(format_usd(0.125), "$0.13");
}

#[test]
fn usd_groups_thousands_and_keeps_sign() {
    assert_eq!(format_usd(0.), "$0.00");
    assert_eq!(format_usd(1234.5), "$1,234.50");
    assert_eq!(format_usd(1_234_567.891), "$1,234,567.89");
    assert_eq!(format_usd(-3.2), "-$3.20");
    assert_eq!(format_count(1234.4), "1,234");
    assert_eq!(format_count(999.5), "1,000");
}

#[test]
fn percent_marks_tiny_shares() {
    assert_eq!(format_percent(0.), "0.0%");
    assert_eq!(format_percent(0.0004), "<0.1%");
    assert_eq!(format_percent(0.5), "50.0%");
    assert_eq!(format_percent(1.), "100.0%");
}

#[test]
fn day_and_hour_labels() {
    assert_eq!(format_day_short("2026-08-07"), "Aug 7");
    assert_eq!(format_day_short("2026-12-31"), "Dec 31");
    assert_eq!(format_day_short("garbage"), "garbage");
    assert_eq!(format_hour_short("2026-08-11T00:00:00.000Z", &Utc), "12 AM");
    assert_eq!(format_hour_short("2026-08-11T14:37:00.000Z", &Utc), "2 PM");
    assert_eq!(format_hour_short("2026-08-11T12:00:00.000Z", &Utc), "12 PM");
    assert_eq!(
        format_date_time_short("2026-08-11T14:37:00Z", &Utc),
        "Aug 11, 2 PM"
    );
    let chicago = FixedOffset::west_opt(5 * 3600).unwrap();
    assert_eq!(format_hour_short("2026-08-11T14:37:00Z", &chicago), "9 AM");
}

#[test]
fn relative_hours_use_the_viewers_calendar() {
    let reference = "2026-08-11T14:37:00Z";
    assert_eq!(
        format_relative_hour_short("2026-08-11T03:37:00Z", reference, &Utc),
        "3 AM today"
    );
    assert_eq!(
        format_relative_hour_short("2026-08-10T20:37:00Z", reference, &Utc),
        "8 PM yesterday"
    );
    assert_eq!(
        format_relative_hour_short("2026-08-09T20:37:00Z", reference, &Utc),
        "Aug 9, 8 PM"
    );
    // 01:00 UTC on the 11th is still the 10th in UTC-5.
    let chicago = FixedOffset::west_opt(5 * 3600).unwrap();
    assert_eq!(
        format_relative_hour_short("2026-08-11T01:00:00Z", "2026-08-11T20:00:00Z", &chicago),
        "8 PM yesterday"
    );
}

// ---------------------------------------------------------------------------------------------
// Window

#[test]
fn daily_window_crosses_month_and_year() {
    let now = Utc.with_ymd_and_hms(2026, 1, 3, 10, 0, 0).unwrap();
    let window = make_window(Period::Week, now, &Utc, "UTC");
    assert_eq!(window.since_day, "2025-12-28");
    assert_eq!(window.until_day, "2026-01-03");
    assert_eq!(window.time_zone, "UTC");
    assert!(window.since_time.is_none());
    let window = make_window(Period::Quarter, now, &Utc, "UTC");
    assert_eq!(window.since_day, "2025-10-06");
}

#[test]
fn daily_window_days_follow_the_zone() {
    // 03:00 UTC on the 3rd is still the 2nd in UTC-5.
    let now = Utc.with_ymd_and_hms(2026, 3, 3, 3, 0, 0).unwrap();
    let zone = FixedOffset::west_opt(5 * 3600).unwrap();
    let window = make_window(Period::Month, now, &zone, "America/Chicago");
    assert_eq!(window.until_day, "2026-03-02");
    assert_eq!(window.since_day, "2026-02-01");
}

#[test]
fn hourly_window_is_minute_aligned_24h() {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 11, 14, 37, 42)
        .unwrap()
        .checked_add_signed(chrono::TimeDelta::milliseconds(512))
        .unwrap();
    let window = make_window(Period::Day, now, &Utc, "UTC");
    assert_eq!(
        window.since_time.as_deref(),
        Some("2026-08-10T14:37:00.000Z")
    );
    assert_eq!(
        window.until_time.as_deref(),
        Some("2026-08-11T14:37:00.000Z")
    );
    assert_eq!(window.since_day, "2026-08-10");
    assert_eq!(window.until_day, "2026-08-11");
}

#[test]
fn period_enumeration_bounds() {
    assert_eq!(
        enumerate_days("2026-02-27", "2026-03-01"),
        ["2026-02-27", "2026-02-28", "2026-03-01"]
    );
    assert!(enumerate_days("2026-03-01", "2026-02-27").is_empty());
    assert!(enumerate_days("bad", "2026-02-27").is_empty());
    let hours = enumerate_hour_starts("2026-08-10T14:37:00.000Z", "2026-08-11T14:37:00.000Z");
    assert_eq!(hours.len(), 24);
    assert_eq!(hours[0], "2026-08-10T14:37:00.000Z");
    assert_eq!(hours[23], "2026-08-11T13:37:00.000Z");
    assert!(enumerate_hour_starts("2026-08-11T14:37:00Z", "2026-08-11T14:37:00Z").is_empty());
}

#[test]
fn preferences_round_trip_and_default_to_limits() {
    let defaults = UsagePreferences::default();
    assert_eq!(defaults.metric, Metric::Limits);
    assert_eq!(defaults.period, Period::Month);
    let json = serde_json::to_string(&UsagePreferences {
        metric: Metric::Tokens,
        period: Period::Day,
    })
    .unwrap();
    assert_eq!(json, r#"{"metric":"tokens","windowDays":1}"#);
    let bad: Result<UsagePreferences, _> = serde_json::from_str(r#"{"windowDays":5}"#);
    assert!(bad.is_err());
}

// ---------------------------------------------------------------------------------------------
// Merge

fn fingerprint(host: &str, provider: UsageProvider, path: &str) -> UsageSourceFingerprint {
    UsageSourceFingerprint {
        host_id: host.into(),
        provider,
        resolved_home_path: path.into(),
        volume_id: "1:2".into(),
    }
}

fn source(fingerprint: UsageSourceFingerprint, status: &str, sessions: u64) -> UsageSource {
    UsageSource {
        fingerprint,
        status: status.into(),
        scanned_files: 1,
        skipped_files: 0,
        malformed_records: 0,
        distinct_sessions: sessions,
        message: None,
        action: None,
    }
}

fn bucket(day: &str, provider: UsageProvider, model: &str, tokens: u64, cost: f64) -> UsageBucket {
    UsageBucket {
        day: day.into(),
        hour_start: None,
        provider,
        model: model.into(),
        source_path: None,
        totals: UsageTokenTotals {
            uncached_input_tokens: tokens,
            cached_input_tokens: 0,
            cache_creation_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
        },
        cost_usd: cost,
        cache_savings_usd: 0.,
        cost_source: CostSource::ModelPriced,
        records: 1,
        unpriced_records: 0,
        sessions: 1,
    }
}

fn summary(
    version: u32,
    read_at: &str,
    buckets: Vec<UsageBucket>,
    sources: Vec<UsageSource>,
) -> UsageSummary {
    UsageSummary {
        contract_version: version,
        read_at: read_at.into(),
        time_zone: "UTC".into(),
        since_day: "2026-08-01".into(),
        until_day: "2026-08-30".into(),
        buckets,
        sources,
        pricing: UsagePricing {
            status: "fresh".into(),
            source: "litellm".into(),
            fetched_at: None,
            known_models: 0,
        },
        scan_duration_ms: 0.,
    }
}

fn env(id: &str, summary: UsageSummary) -> EnvironmentUsage {
    EnvironmentUsage {
        environment_id: id.into(),
        label: id.to_uppercase(),
        summary,
    }
}

#[test]
fn incompatible_versions_are_excluded_with_direction() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let merged = merge_usage(&[
        env(
            "old",
            summary(
                3,
                "2026-08-30T00:00:00Z",
                vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 10, 1.)],
                vec![source(codex.clone(), "ok", 1)],
            ),
        ),
        env(
            "new",
            summary(
                7,
                "2026-08-30T00:00:00Z",
                vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 10, 1.)],
                vec![source(codex, "ok", 1)],
            ),
        ),
    ]);
    assert_eq!(merged.total_tokens, 0);
    let directions: Vec<_> = merged
        .contract_mismatches
        .iter()
        .map(|mismatch| (mismatch.environment_id.as_str(), mismatch.direction))
        .collect();
    assert_eq!(
        directions,
        [
            ("old", ContractMismatchDirection::ServerBehind),
            ("new", ContractMismatchDirection::ClientBehind)
        ]
    );
}

#[test]
fn shared_directory_counts_once_and_complete_scans_win() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let partial_newer = env(
        "a",
        summary(
            6,
            "2026-08-30T10:00:00Z",
            vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 100, 1.)],
            vec![source(codex.clone(), "partial", 3)],
        ),
    );
    let complete_older = env(
        "b",
        summary(
            6,
            "2026-08-30T09:00:00Z",
            vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 40, 0.5)],
            vec![source(codex, "ok", 2)],
        ),
    );
    let merged = merge_usage(&[partial_newer, complete_older]);
    // The complete scan owns the directory; the partial one adds no missing cells.
    assert_eq!(merged.total_tokens, 40);
    assert_eq!(merged.contributing_environments, ["b"]);
    assert_eq!(merged.duplicate_sources, ["A: /h/.codex"]);
    assert_eq!(merged.sessions, 2);
}

#[test]
fn newer_partial_scan_adds_only_missing_cells() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let merged = merge_usage(&[
        env(
            "a",
            summary(
                6,
                "2026-08-30T10:00:00Z",
                vec![
                    bucket("2026-08-01", UsageProvider::Codex, "gpt", 100, 1.),
                    bucket("2026-08-02", UsageProvider::Codex, "gpt", 7, 0.1),
                ],
                vec![source(codex.clone(), "partial", 5)],
            ),
        ),
        env(
            "b",
            summary(
                6,
                "2026-08-30T09:00:00Z",
                vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 40, 0.5)],
                vec![source(codex, "ok", 2)],
            ),
        ),
    ]);
    assert_eq!(merged.total_tokens, 47);
    assert_eq!(merged.sessions, 5);
    assert_eq!(merged.daily.len(), 2);
}

#[test]
fn ties_break_by_environment_id() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let at = "2026-08-30T10:00:00Z";
    let merged = merge_usage(&[
        env(
            "z",
            summary(
                6,
                at,
                vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 1, 0.)],
                vec![source(codex.clone(), "ok", 1)],
            ),
        ),
        env(
            "a",
            summary(
                6,
                at,
                vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 2, 0.)],
                vec![source(codex, "ok", 1)],
            ),
        ),
    ]);
    assert_eq!(merged.total_tokens, 2);
}

#[test]
fn totals_sessions_shares_and_order() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let claude = fingerprint("mac", UsageProvider::Claude, "/h/.claude");
    let mut reasoning = bucket("2026-08-02", UsageProvider::Codex, "gpt-5", 0, 2.);
    reasoning.totals.output_tokens = 30;
    reasoning.totals.reasoning_tokens = 20;
    reasoning.sessions = 9;
    let mut unpriced = bucket("2026-08-01", UsageProvider::Claude, "opus", 50, 0.);
    unpriced.cost_source = CostSource::Unpriced;
    unpriced.unpriced_records = 1;
    let merged = merge_usage(&[env(
        "a",
        summary(
            6,
            "2026-08-30T10:00:00Z",
            vec![
                reasoning,
                unpriced,
                bucket("2026-08-01", UsageProvider::Codex, "gpt-5", 20, 1.),
                bucket(
                    "2026-08-01",
                    UsageProvider::Other("future".into()),
                    "x",
                    999,
                    9.,
                ),
            ],
            vec![source(codex, "ok", 4), source(claude, "ok", 1)],
        ),
    )]);
    assert_eq!(merged.total_tokens, 100);
    assert_eq!(merged.output_tokens, 30);
    assert_eq!(merged.reasoning_tokens, 20);
    assert_eq!(merged.sessions, 5);
    assert!((merged.cost_usd - 3.).abs() < 1e-9);
    let providers: Vec<_> = merged.providers.iter().map(|p| p.provider).collect();
    assert_eq!(providers, [ProviderKind::Codex, ProviderKind::Claude]);
    assert_eq!(merged.providers[1].cost_share, 0.);
    assert!((merged.providers[1].token_share - 0.5).abs() < 1e-9);
    assert_eq!(merged.providers[0].sessions, 4);
    let models: Vec<_> = merged.models.iter().map(|m| m.model.as_str()).collect();
    assert_eq!(models, ["gpt-5", "opus"]);
    assert!(merged.models[1].is_cost_unknown());
    let days: Vec<_> = merged.daily.iter().map(|d| d.day.as_str()).collect();
    assert_eq!(days, ["2026-08-01", "2026-08-02"]);
    assert!((merged.cost_quality.unpriced_share - 1. / 3.).abs() < 1e-9);
}

#[test]
fn empty_and_zero_totals_have_no_nan() {
    let merged = merge_usage(&[]);
    assert_eq!(merged.total_tokens, 0);
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let merged = merge_usage(&[env(
        "a",
        summary(
            6,
            "2026-08-30T10:00:00Z",
            vec![bucket("2026-08-01", UsageProvider::Codex, "gpt", 0, 0.)],
            vec![source(codex, "ok", 1)],
        ),
    )]);
    assert!(
        merged
            .providers
            .iter()
            .all(|p| p.cost_share == 0. && p.token_share == 0.)
    );
    assert_eq!(merged.cost_quality.unpriced_share, 0.);
}

#[test]
fn hourly_buckets_merge_by_instant() {
    let codex = fingerprint("mac", UsageProvider::Codex, "/h/.codex");
    let mut first = bucket("2026-08-11", UsageProvider::Codex, "gpt", 5, 0.);
    first.hour_start = Some("2026-08-11T13:37:00.000Z".into());
    let mut second = bucket("2026-08-11", UsageProvider::Codex, "gpt-mini", 6, 0.);
    second.hour_start = Some("2026-08-11T13:37:00Z".into());
    let mut earlier = bucket("2026-08-11", UsageProvider::Codex, "gpt", 1, 0.);
    earlier.hour_start = Some("2026-08-11T12:37:00.000Z".into());
    let merged = merge_usage(&[env(
        "a",
        summary(
            6,
            "2026-08-30T10:00:00Z",
            vec![first, second, earlier],
            vec![source(codex, "ok", 1)],
        ),
    )]);
    let hours: Vec<_> = merged.hourly.iter().map(|h| h.total_tokens).collect();
    assert_eq!(hours, [1, 11]);
}

// ---------------------------------------------------------------------------------------------
// Chart

#[test]
fn nice_scale_rounds_the_peak_up_to_a_step() {
    let scale = nice_scale(0., 4);
    assert_eq!(scale.max, 0.);
    assert_eq!(scale.ticks, [0.]);
    let scale = nice_scale(7., 4);
    assert_eq!(scale.max, 8.);
    assert_eq!(scale.ticks, [0., 2., 4., 6., 8.]);
    let scale = nice_scale(0.31, 4);
    assert_eq!(scale.ticks.len(), 5);
    assert!((scale.max - 0.4).abs() < 1e-12);
    let scale = nice_scale(1_234_567., 4);
    assert_eq!(scale.max, 1_500_000.);
}

#[test]
fn monotone_curve_does_not_overshoot() {
    let points = [(0., 10.), (1., 10.), (2., 0.), (3., 10.)];
    let segments = monotone_curve(&points);
    assert_eq!(segments.len(), 3);
    // A flat run keeps flat control points.
    assert_eq!(segments[0].c1.1, 10.);
    assert_eq!(segments[0].c2.1, 10.);
    for segment in &segments {
        for y in [segment.c1.1, segment.c2.1] {
            assert!((0. ..=10.).contains(&y), "control point {y} overshoots");
        }
    }
    assert!(monotone_curve(&[(0., 1.)]).is_empty());
}

// ---------------------------------------------------------------------------------------------
// Limits

fn window(id: &str, kind: UsageWindowKind, used: f64, resets_at: Option<&str>) -> UsageWindow {
    UsageWindow {
        id: id.into(),
        kind,
        label: id.into(),
        used_percent: used,
        resets_at: resets_at.map(Into::into),
        window_duration_mins: Some(300),
    }
}

fn limits(checked_at: &str, windows: Vec<UsageWindow>) -> ServerProviderUsageLimits {
    ServerProviderUsageLimits {
        checked_at: checked_at.into(),
        windows,
        credential_fingerprint: None,
        reset_credits: None,
        external_usage: None,
        unavailable: None,
    }
}

/// 2026-08-11T10:06:40Z.
const NOW: i64 = 1_786_450_000_000;

fn at(offset_minutes: i64) -> String {
    crate::time::format_timestamp(NOW + offset_minutes * 60_000).unwrap()
}

#[test]
fn remaining_elapsed_and_pace() {
    let w = window("five_hour", UsageWindowKind::Session, 37.4, Some(&at(150)));
    assert_eq!(remaining_percent(&w), 63);
    assert_eq!(
        remaining_percent(&window("x", UsageWindowKind::Session, 120., None)),
        0
    );
    // Half of a 300 minute window elapsed.
    assert!((elapsed_share(&w, NOW).unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(pace_of(&w, NOW), Some(LimitPace::Under));
    let on = window("x", UsageWindowKind::Session, 54., Some(&at(150)));
    assert_eq!(pace_of(&on, NOW), Some(LimitPace::On));
    let ahead = window("x", UsageWindowKind::Session, 56., Some(&at(150)));
    assert_eq!(pace_of(&ahead, NOW), Some(LimitPace::Ahead));
    let no_clock = window("x", UsageWindowKind::Session, 56., None);
    assert_eq!(pace_of(&no_clock, NOW), None);
}

#[test]
fn durations_and_resets() {
    assert_eq!(format_duration(-5), "0m");
    assert_eq!(format_duration(0), "0m");
    assert_eq!(format_duration((2 * 60 + 13) * 60_000), "2h 13m");
    assert_eq!(format_duration((3 * 24 + 4) * 3_600_000 + 59_000), "3d 4h");
    let past = window("x", UsageWindowKind::Session, 1., Some(&at(-1)));
    assert_eq!(format_resets_in(&past, NOW).as_deref(), Some("resets now"));
    let future = window("x", UsageWindowKind::Session, 1., Some(&at(133)));
    assert_eq!(
        format_resets_in(&future, NOW).as_deref(),
        Some("resets in 2h 13m")
    );
}

fn environment(id: &str, providers: Vec<LimitProvider>) -> LimitsEnvironment {
    LimitsEnvironment {
        environment_id: id.into(),
        label: id.to_uppercase(),
        providers,
        sources: Vec::new(),
    }
}

fn provider(
    instance: &str,
    email: Option<&str>,
    limits: ServerProviderUsageLimits,
) -> LimitProvider {
    LimitProvider {
        instance_id: instance.into(),
        driver: "codex".into(),
        display_name: None,
        accent_color: None,
        email: email.map(Into::into),
        plan: Some("Pro".into()),
        authenticated: true,
        limits: Some(limits),
    }
}

#[test]
fn same_email_across_environments_is_one_account() {
    let a = provider(
        "codex",
        Some("Me@Example.com"),
        limits(
            "2026-08-11T10:00:00Z",
            vec![window("primary", UsageWindowKind::Session, 10., None)],
        ),
    );
    let b = provider(
        "codex",
        Some("me@example.com"),
        limits(
            "2026-08-11T10:05:00Z",
            vec![window("primary", UsageWindowKind::Session, 20., None)],
        ),
    );
    let accounts = collect_limit_accounts(&[environment("a", vec![a]), environment("b", vec![b])]);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].environments.len(), 2);
    // The fresher read supplies the windows.
    assert_eq!(accounts[0].limits.windows[0].used_percent, 20.);
}

#[test]
fn pools_group_by_kind_and_keep_columns() {
    let one = provider(
        "one",
        Some("one@x.com"),
        limits(
            "2026-08-11T10:00:00Z",
            vec![
                window("weekly", UsageWindowKind::Weekly, 50., Some(&at(600))),
                window("primary", UsageWindowKind::Session, 31., Some(&at(60))),
            ],
        ),
    );
    let two = provider(
        "two",
        Some("two@x.com"),
        limits(
            "2026-08-11T10:00:00Z",
            vec![window(
                "primary",
                UsageWindowKind::Session,
                60.,
                Some(&at(30)),
            )],
        ),
    );
    let accounts = collect_limit_accounts(&[environment("a", vec![one, two])]);
    let pools = collect_limit_pools(&accounts, NOW);
    assert_eq!(pools.len(), 1);
    let windows = &pools[0].windows;
    let kinds: Vec<_> = windows.iter().map(|w| w.kind.clone()).collect();
    assert_eq!(kinds, [UsageWindowKind::Session, UsageWindowKind::Weekly]);
    // Mean used 45.5 -> 54.5 left -> rounds to 55 (JS Math.round).
    assert_eq!(windows[0].remaining_percent, 55);
    // Accounts ordered by the session reset: "two" resets first.
    assert_eq!(
        windows[0].columns[0].account.email.as_deref(),
        Some("two@x.com")
    );
    // Restores: 60/2 = 30 and 31/2 = 15.5 -> 16.
    let restores: Vec<_> = windows[0]
        .resets
        .iter()
        .map(|r| r.restores_percent)
        .collect();
    assert_eq!(restores, [30, 16]);
    // The weekly pool keeps a gap for the account without a weekly window.
    assert_eq!(windows[1].columns.len(), 2);
    assert!(windows[1].columns[0].window.is_none());
}

#[test]
fn notices_skip_unsupported_and_name_environments_only_when_several() {
    let mut failed = limits("2026-08-11T10:00:00Z", vec![]);
    failed.unavailable = Some(UsageLimitsUnavailable {
        reason: "probeFailed".into(),
        message: None,
    });
    let mut unsupported = limits("2026-08-11T10:00:00Z", vec![]);
    unsupported.unavailable = Some(UsageLimitsUnavailable {
        reason: "unsupported".into(),
        message: None,
    });
    let a = environment(
        "a",
        vec![
            provider("codex", None, failed.clone()),
            provider("api", None, unsupported),
        ],
    );
    assert_eq!(
        collect_limit_notices(std::slice::from_ref(&a)),
        ["codex: Could not read limits."]
    );
    let b = environment("b", vec![provider("codex", None, failed)]);
    assert_eq!(
        collect_limit_notices(&[a.clone(), b]),
        [
            "A · codex: Could not read limits.",
            "B · codex: Could not read limits."
        ]
    );
    // Accounts with a notice never become bars.
    assert!(collect_limit_accounts(&[a]).is_empty());
}
