//! ISO-8601 timestamps as the web client reads and writes them.

use chrono::{DateTime, SecondsFormat, Utc};

/// Milliseconds since the Unix epoch, the unit the web client compares (`Date.parse`).
pub type EpochMillis = i64;

/// Parses a server timestamp (`2026-09-30T12:00:00.000Z`). `None` when unparseable, which the web
/// treats as "no timestamp" (`Date.parse` gives `NaN`).
pub fn parse_timestamp(value: &str) -> Option<EpochMillis> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|parsed| parsed.timestamp_millis())
}

/// Formats milliseconds the way `Date.prototype.toISOString` does
/// (`2026-09-30T12:00:00.000Z`).
pub fn format_timestamp(millis: EpochMillis) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(millis)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// The current time in epoch milliseconds.
pub fn now_millis() -> EpochMillis {
    Utc::now().timestamp_millis()
}

/// Sidebar relative time (`web/timestampFormat.ts` `formatRelativeTimeLabel`): "just now" under a
/// minute (or in the future), then "5m ago", "3h ago", "12d ago". There are no weeks or months.
/// An unparseable timestamp reads as "just now", like the web's `NaN` comparisons.
pub fn format_relative_time(value: &str, now: EpochMillis) -> String {
    let Some(then) = parse_timestamp(value) else {
        return "just now".to_owned();
    };
    let diff = now - then;
    if diff < 0 {
        return "just now".to_owned();
    }
    let seconds = diff / 1000;
    if seconds < 60 {
        return "just now".to_owned();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

#[cfg(test)]
mod tests {
    //! Failure modes: off-by-one at the 60s/60m/24h boundaries, future timestamps, unparseable
    //! input, and `toISOString` formatting (always millis, always `Z`).
    use super::*;

    const NOW: &str = "2026-10-01T12:00:00.000Z";

    fn rel(value: &str) -> String {
        format_relative_time(value, parse_timestamp(NOW).unwrap())
    }

    #[test]
    fn relative_time_boundaries() {
        assert_eq!(rel("2026-10-01T11:59:00.001Z"), "just now");
        assert_eq!(rel("2026-10-01T11:59:00.000Z"), "1m ago");
        assert_eq!(rel("2026-10-01T11:00:00.001Z"), "59m ago");
        assert_eq!(rel("2026-10-01T11:00:00.000Z"), "1h ago");
        assert_eq!(rel("2026-09-30T12:00:00.001Z"), "23h ago");
        assert_eq!(rel("2026-09-30T12:00:00.000Z"), "1d ago");
        assert_eq!(rel("2026-08-17T12:00:00.000Z"), "45d ago");
        assert_eq!(rel("2026-10-02T12:00:00.000Z"), "just now");
        assert_eq!(rel("garbage"), "just now");
    }

    #[test]
    fn iso_round_trip() {
        let millis = parse_timestamp("2026-10-01T12:00:00Z").unwrap();
        assert_eq!(
            format_timestamp(millis - 1).unwrap(),
            "2026-10-01T11:59:59.999Z"
        );
        assert_eq!(parse_timestamp("2026-10-01T14:00:00+02:00"), Some(millis));
        assert_eq!(parse_timestamp(""), None);
    }
}
