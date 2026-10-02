//! Labels for the Usage page (`packages/shared/src/usageFormat.ts`). Number formatting matches
//! JS `toFixed` / `Intl.NumberFormat("en-US")`, which round exact ties away from zero (Rust's
//! formatter rounds them to even). Dates take the viewer's zone as a chrono [`TimeZone`]
//! (`Local` in the app, a fixed zone in tests and snapshots).

use chrono::{DateTime, Datelike as _, NaiveDate, TimeZone, Timelike as _};

use crate::time::parse_timestamp;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// JS `Number.prototype.toFixed(digits)`: exact decimal rounding, ties away from zero.
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let magnitude = value.abs();
    // Rust prints the exact binary value correctly rounded, ties to even. An exact tie shows as
    // a 5 followed only by zeros in a longer expansion; nudge those up one ulp.
    let long = format!("{magnitude:.*}", digits + 30);
    let tail = &long[long.len() - 30..];
    let is_tie = tail.starts_with('5') && tail[1..].bytes().all(|byte| byte == b'0');
    let rounded = if is_tie {
        format!("{:.*}", digits, magnitude.next_up())
    } else {
        format!("{magnitude:.digits$}")
    };
    let is_zero = rounded.bytes().all(|byte| matches!(byte, b'0' | b'.'));
    if value.is_sign_negative() && !is_zero {
        format!("-{rounded}")
    } else {
        rounded
    }
}

/// Groups the integer part of a non-negative decimal string with commas (`1234.50` ->
/// `1,234.50`).
fn group_thousands(decimal: &str) -> String {
    let (integer, fraction) = decimal.split_once('.').unwrap_or((decimal, ""));
    let mut grouped = String::with_capacity(decimal.len() + integer.len() / 3);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if !fraction.is_empty() {
        grouped.push('.');
        grouped.push_str(fraction);
    }
    grouped
}

/// `$1,234.50` (`Intl` en-US currency, two decimals); `-$3.20` below zero.
pub fn format_usd(value: f64) -> String {
    let fixed = to_fixed(value.abs(), 2);
    let sign = if value < 0. && fixed != "0.00" {
        "-"
    } else {
        ""
    };
    format!("{sign}${}", group_thousands(&fixed))
}

/// `1,234`: rounded like `Math.round` (half up) and grouped.
pub fn format_count(value: f64) -> String {
    let rounded = (value + 0.5).floor();
    let text = to_fixed(rounded.abs(), 0);
    let sign = if rounded < 0. { "-" } else { "" };
    format!("{sign}{}", group_thousands(&text))
}

/// Three significant figures with a unit suffix so columns line up (`19.9B`, `76.7M`, `804K`).
/// Only all-zero decimals are trimmed, as the web does: 1500 is `1.50K`.
pub fn format_tokens(value: f64) -> String {
    let abs = value.abs();
    let (scaled, suffix) = if abs >= 1e12 {
        (value / 1e12, "T")
    } else if abs >= 1e9 {
        (value / 1e9, "B")
    } else if abs >= 1e6 {
        (value / 1e6, "M")
    } else if abs >= 1e3 {
        (value / 1e3, "K")
    } else {
        return format_count(value);
    };
    let digits = if scaled.abs() >= 100. {
        0
    } else if scaled.abs() >= 10. {
        1
    } else {
        2
    };
    let fixed = to_fixed(scaled, digits);
    let trimmed = match fixed.split_once('.') {
        Some((integer, fraction)) if fraction.bytes().all(|byte| byte == b'0') => integer,
        _ => fixed.as_str(),
    };
    format!("{trimmed}{suffix}")
}

/// `12.5%`; a share too small to show at one decimal reads `<0.1%`.
pub fn format_percent(share: f64) -> String {
    let percent = share * 100.;
    if percent > 0. && percent < 0.1 {
        return "<0.1%".to_owned();
    }
    format!("{}%", to_fixed(percent, 1))
}

/// `2026-08-07` -> `Aug 7`. Anything else comes back unchanged.
pub fn format_day_short(day: &str) -> String {
    match NaiveDate::parse_from_str(day, "%Y-%m-%d") {
        Ok(date) => format!("{} {}", MONTHS[date.month0() as usize], date.day()),
        Err(_) => day.to_owned(),
    }
}

fn in_zone<Tz: TimeZone>(instant: &str, zone: &Tz) -> Option<DateTime<Tz>> {
    let millis = parse_timestamp(instant)?;
    zone.timestamp_millis_opt(millis).single()
}

fn hour_label<Tz: TimeZone>(time: &DateTime<Tz>) -> String {
    let (pm, hour) = time.hour12();
    format!("{hour} {}", if pm { "PM" } else { "AM" })
}

/// A bucket start as `2 PM` in the viewer's zone. The web adds the zone abbreviation for the
/// repeated hour of a fall-back transition; chrono has no abbreviations, so that hour reads
/// the same twice.
pub fn format_hour_short<Tz: TimeZone>(hour_start: &str, zone: &Tz) -> String {
    in_zone(hour_start, zone).map_or_else(|| hour_start.to_owned(), |time| hour_label(&time))
}

/// `2026-08-11T14:37:00Z` -> `Aug 11, 2 PM` in the viewer's zone.
pub fn format_date_time_short<Tz: TimeZone>(instant: &str, zone: &Tz) -> String {
    in_zone(instant, zone).map_or_else(
        || instant.to_owned(),
        |time| {
            format!(
                "{} {}, {}",
                MONTHS[time.month0() as usize],
                time.day(),
                hour_label(&time)
            )
        },
    )
}

/// An hourly tooltip label relative to the window's end: `2 PM today`, `8 PM yesterday`, else
/// `Aug 9, 8 PM`. Calendar days are the viewer's.
pub fn format_relative_hour_short<Tz: TimeZone>(
    hour_start: &str,
    relative_to: &str,
    zone: &Tz,
) -> String {
    let (Some(instant), Some(reference)) = (in_zone(hour_start, zone), in_zone(relative_to, zone))
    else {
        return format_date_time_short(hour_start, zone);
    };
    let days_ago = (reference.date_naive() - instant.date_naive()).num_days();
    let hour = hour_label(&instant);
    match days_ago {
        0 => format!("{hour} today"),
        1 => format!("{hour} yesterday"),
        _ => format_date_time_short(hour_start, zone),
    }
}

/// The header's window label: `Aug 7 to Aug 11`, or `Aug 10, 2 PM to Aug 11, 2 PM` for the
/// rolling 24 hours.
pub fn format_window_label<Tz: TimeZone>(
    window: &t3_protocol::usage::UsageSummaryInput,
    zone: &Tz,
) -> String {
    match (&window.since_time, &window.until_time) {
        (Some(since), Some(until)) => format!(
            "{} to {}",
            format_date_time_short(since, zone),
            format_date_time_short(until, zone)
        ),
        _ => format!(
            "{} to {}",
            format_day_short(&window.since_day),
            format_day_short(&window.until_day)
        ),
    }
}
