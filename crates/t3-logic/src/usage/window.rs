//! The window the Usage page requests (`makeWindow`, `usageFormat.ts:208-263`) and the periods
//! its chart and tables walk (`enumerateDays`, `enumerateHourStarts`).

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use t3_protocol::usage::{UsageResolution, UsageSummaryInput};

use super::Period;
use crate::time::{format_timestamp, parse_timestamp};

const HOUR_MS: i64 = 3_600_000;

/// The request for `period` ending `now`, with days in `zone` (named `zone_name`, an IANA id the
/// server buckets by). Daily windows are calendar arithmetic on the zone's end day, so DST never
/// shifts them; the past 24h is minute-aligned and exactly 24 hours long.
pub fn make_window<Tz: TimeZone>(
    period: Period,
    now: DateTime<Utc>,
    zone: &Tz,
    zone_name: &str,
) -> UsageSummaryInput {
    let day_in_zone = |instant: DateTime<Utc>| {
        instant
            .with_timezone(zone)
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    };
    if period.is_hourly() {
        let until_ms = now.timestamp_millis().div_euclid(60_000) * 60_000;
        let since_ms = until_ms - 24 * HOUR_MS;
        let until = DateTime::<Utc>::from_timestamp_millis(until_ms).unwrap_or(now);
        let since = DateTime::<Utc>::from_timestamp_millis(since_ms).unwrap_or(now);
        return UsageSummaryInput {
            since_day: day_in_zone(since),
            until_day: day_in_zone(until),
            time_zone: zone_name.to_owned(),
            resolution: Some(UsageResolution::Hour),
            since_time: format_timestamp(since_ms),
            until_time: format_timestamp(until_ms),
        };
    }
    let until = now.with_timezone(zone).date_naive();
    let since = until - Duration::days(i64::from(period.days()) - 1);
    UsageSummaryInput {
        since_day: since.format("%Y-%m-%d").to_string(),
        until_day: until.format("%Y-%m-%d").to_string(),
        time_zone: zone_name.to_owned(),
        resolution: Some(UsageResolution::Day),
        since_time: None,
        until_time: None,
    }
}

/// Every `YYYY-MM-DD` from `since_day` to `until_day` inclusive; empty when either is malformed
/// or the range is backwards.
pub fn enumerate_days(since_day: &str, until_day: &str) -> Vec<String> {
    let parse = |day: &str| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok();
    let (Some(start), Some(end)) = (parse(since_day), parse(until_day)) else {
        return Vec::new();
    };
    start
        .iter_days()
        .take_while(|day| *day <= end)
        .map(|day| day.format("%Y-%m-%d").to_string())
        .collect()
}

/// Every hour start from `since_time` (inclusive) to `until_time` (exclusive), as
/// `toISOString` strings. Empty when malformed or not increasing.
pub fn enumerate_hour_starts(since_time: &str, until_time: &str) -> Vec<String> {
    let (Some(start), Some(end)) = (parse_timestamp(since_time), parse_timestamp(until_time))
    else {
        return Vec::new();
    };
    (start..end)
        .step_by(HOUR_MS as usize)
        .filter_map(format_timestamp)
        .collect()
}

/// The periods of `window`: hour starts for the rolling 24h, days otherwise.
pub fn periods(window: &UsageSummaryInput) -> Vec<String> {
    match (&window.since_time, &window.until_time) {
        (Some(since), Some(until)) => enumerate_hour_starts(since, until),
        _ => enumerate_days(&window.since_day, &window.until_day),
    }
}
