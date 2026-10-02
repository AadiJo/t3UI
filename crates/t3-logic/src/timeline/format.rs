//! Labels the timeline shows: durations, the working timer, message timestamps, and
//! workspace-relative paths.

use chrono::{DateTime, Datelike as _, Local, Timelike as _};

use crate::{
    settings::TimestampFormat,
    time::{EpochMillis, parse_timestamp},
};

/// "Worked for" durations (`formatDuration`): `850ms`, `1.9s`, `10s`, `42s`, `3m`, `3m 12s`.
pub fn format_duration(millis: i64) -> String {
    if millis < 0 {
        return "0ms".to_owned();
    }
    let ms = millis as f64;
    if millis < 1_000 {
        return format!("{}ms", ms.round().max(1.0) as i64);
    }
    if millis < 10_000 {
        let tenths = (ms / 100.0).round() / 10.0;
        // 9.95s+ rounds up to the next bucket: "10s", not "10.0s".
        return if tenths >= 10.0 {
            "10s".to_owned()
        } else {
            format!("{tenths:.1}s")
        };
    }
    if millis < 60_000 {
        return format!("{}s", (ms / 1_000.0).round() as i64);
    }
    let minutes = millis / 60_000;
    let seconds = ((millis % 60_000) as f64 / 1_000.0).round() as i64;
    match seconds {
        0 => format!("{minutes}m"),
        60 => format!("{}m", minutes + 1),
        _ => format!("{minutes}m {seconds}s"),
    }
}

/// Elapsed time between two timestamps (`computeElapsedMs`), clamped at zero. `None` when either
/// does not parse.
pub fn elapsed_millis(start: &str, end: &str) -> Option<i64> {
    Some((parse_timestamp(end)? - parse_timestamp(start)?).max(0))
}

/// The "Working for …" timer (`formatWorkingTimer`): `12s`, `2m 3s`, `2m`, `1h 4m`, `1h`.
pub fn format_working_timer(started_at: &str, now: EpochMillis) -> String {
    let Some(start) = parse_timestamp(started_at) else {
        return "0s".to_owned();
    };
    let seconds = ((now - start) / 1000).max(0);
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let (hours, minutes, seconds) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if hours > 0 {
        return if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        };
    }
    if seconds > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{minutes}m")
    }
}

fn local_time(iso: &str) -> Option<DateTime<Local>> {
    DateTime::from_timestamp_millis(parse_timestamp(iso)?).map(|utc| utc.with_timezone(&Local))
}

fn clock(time: &DateTime<Local>, format: TimestampFormat) -> String {
    match format {
        // en-US `hour: "numeric", minute: "2-digit"`.
        TimestampFormat::Locale | TimestampFormat::TwelveHour => {
            let (pm, hour) = time.hour12();
            format!(
                "{hour}:{:02} {}",
                time.minute(),
                if pm { "PM" } else { "AM" }
            )
        }
        TimestampFormat::TwentyFourHour => format!("{:02}:{:02}", time.hour(), time.minute()),
    }
}

/// Message meta timestamp (`formatShortTimestamp`), in local time: `4:29 AM` / `04:29`.
pub fn format_short_timestamp(iso: &str, format: TimestampFormat) -> String {
    local_time(iso).map_or_else(String::new, |time| clock(&time, format))
}

/// Timestamp with seconds (`formatTimestamp`), in local time: `4:29:07 AM` / `04:29:07`. The plan
/// sidebar header uses it.
pub fn format_timestamp(iso: &str, format: TimestampFormat) -> String {
    local_time(iso).map_or_else(String::new, |time| {
        let minutes = clock(&time, format);
        // Insert `:SS` after the minutes, before any ` AM`/` PM`.
        let split = minutes.find(' ').unwrap_or(minutes.len());
        format!(
            "{}:{:02}{}",
            &minutes[..split],
            time.second(),
            &minutes[split..]
        )
    })
}

/// Timestamp tooltip (`formatChatTimestampTooltip`): `4:29 AM, 2nd October 2026`.
pub fn format_timestamp_tooltip(iso: &str, format: TimestampFormat) -> String {
    let Some(time) = local_time(iso) else {
        return String::new();
    };
    let day = time.day();
    let suffix = match (day % 100, day % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!(
        "{}, {day}{suffix} {} {}",
        clock(&time, format),
        time.format("%B"),
        time.year()
    )
}

/// Splits a trailing `:line[:column]` off a path (`splitPathAndPosition`).
fn split_path_and_position(value: &str) -> (&str, Option<&str>, Option<&str>) {
    fn trailing_number(value: &str) -> Option<(&str, &str)> {
        let (head, digits) = value.rsplit_once(':')?;
        (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some((head, digits))
    }
    let Some((head, last)) = trailing_number(value) else {
        return (value, None, None);
    };
    match trailing_number(head) {
        Some((path, line)) => (path, Some(line), Some(last)),
        None => (head, Some(last), None),
    }
}

/// A path as the work log and changed-files rows show it (`formatWorkspaceRelativePath`):
/// paths inside the workspace become `<workspace-name>/<relative>`, relative paths get the
/// workspace name prefixed, anything else stays as is. Keeps a `:line[:col]` suffix.
pub fn format_workspace_relative_path(value: &str, workspace_root: Option<&str>) -> String {
    let (path, line, column) = split_path_and_position(value);
    let canonical = |p: &str| -> String {
        let p = p.replace('\\', "/");
        let bytes = p.as_bytes();
        // `/C:/x` → `C:/x`
        if bytes.len() >= 4
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
            && bytes[3] == b'/'
        {
            p[1..].to_owned()
        } else {
            p
        }
    };
    let normalized = canonical(path);
    let mut display = normalized.clone();
    if let Some(root) = workspace_root.filter(|root| !root.is_empty()) {
        let root = canonical(root.trim_end_matches(['/', '\\']));
        let label = root.rsplit('/').next().unwrap_or(&root).to_owned();
        let path_lower = normalized.to_lowercase();
        let root_lower = root.to_lowercase();
        if path_lower == root_lower {
            display = label;
        } else if path_lower.starts_with(&format!("{root_lower}/")) {
            display = format!("{label}/{}", &normalized[root.len() + 1..]);
        } else if !normalized.starts_with('/') {
            let relative = normalized
                .trim_start_matches("./")
                .trim_start_matches('/')
                .to_owned();
            display = if path_lower.starts_with(&format!("{}/", label.to_lowercase())) {
                normalized
            } else {
                format!("{label}/{relative}")
            };
        }
    }
    match (line, column) {
        (Some(line), Some(column)) => format!("{display}:{line}:{column}"),
        (Some(line), None) => format!("{display}:{line}"),
        _ => display,
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: rounding at the 1s/10s/60s bucket edges ("10.0s", "1m 60s"), negative or
    //! unparseable times, timer units past an hour, and workspace prefixes that only share a
    //! name prefix (`/repo-other` under `/repo`).
    use super::*;
    use crate::time::parse_timestamp;

    #[test]
    fn durations_use_the_web_buckets() {
        assert_eq!(format_duration(0), "1ms");
        assert_eq!(format_duration(795), "795ms");
        assert_eq!(format_duration(1_982), "2.0s");
        assert_eq!(format_duration(1_940), "1.9s");
        assert_eq!(format_duration(9_960), "10s");
        assert_eq!(format_duration(42_400), "42s");
        assert_eq!(format_duration(180_000), "3m");
        assert_eq!(format_duration(192_000), "3m 12s");
        assert_eq!(format_duration(239_700), "4m");
        assert_eq!(format_duration(-5), "0ms");
    }

    #[test]
    fn working_timer() {
        let start = "2026-10-02T04:29:34.992Z";
        let at = |offset_ms: i64| parse_timestamp(start).unwrap() + offset_ms;
        assert_eq!(format_working_timer(start, at(-500)), "0s");
        assert_eq!(format_working_timer(start, at(59_999)), "59s");
        assert_eq!(format_working_timer(start, at(122_000)), "2m 2s");
        assert_eq!(format_working_timer(start, at(120_000)), "2m");
        assert_eq!(format_working_timer(start, at(3_900_000)), "1h 5m");
        assert_eq!(format_working_timer("nope", at(0)), "0s");
    }

    #[test]
    fn elapsed_is_clamped() {
        assert_eq!(
            elapsed_millis("2026-10-02T04:29:35.804Z", "2026-10-02T04:29:37.786Z"),
            Some(1_982)
        );
        assert_eq!(
            elapsed_millis("2026-10-02T04:29:37.786Z", "2026-10-02T04:29:35.804Z"),
            Some(0)
        );
        assert_eq!(elapsed_millis("x", "2026-10-02T04:29:35.804Z"), None);
    }

    #[test]
    fn workspace_relative_paths() {
        let root = Some("/tmp/repos/aurora-web");
        assert_eq!(
            format_workspace_relative_path("/tmp/repos/aurora-web/src/format.ts", root),
            "aurora-web/src/format.ts"
        );
        assert_eq!(
            format_workspace_relative_path("/tmp/repos/aurora-web", root),
            "aurora-web"
        );
        assert_eq!(
            format_workspace_relative_path("/tmp/repos/aurora-web-2/x.ts", root),
            "/tmp/repos/aurora-web-2/x.ts"
        );
        assert_eq!(
            format_workspace_relative_path("./src/a.ts", root),
            "aurora-web/src/a.ts"
        );
        assert_eq!(
            format_workspace_relative_path("aurora-web/a.ts", root),
            "aurora-web/a.ts"
        );
        assert_eq!(
            format_workspace_relative_path("/tmp/repos/aurora-web/src/a.ts:12:3", root),
            "aurora-web/src/a.ts:12:3"
        );
        assert_eq!(
            format_workspace_relative_path("src/a.ts:7", None),
            "src/a.ts:7"
        );
    }

    /// `format_timestamp` adds `:SS` to the short clock: after the minutes and before the
    /// meridiem in 12-hour formats, at the end in 24-hour. Checked against the short form so
    /// the test holds in any local time zone.
    #[test]
    fn timestamp_with_seconds_extends_the_short_clock() {
        let iso = "2026-10-02T04:30:17.000Z";
        for format in [TimestampFormat::TwelveHour, TimestampFormat::TwentyFourHour] {
            let short = format_short_timestamp(iso, format);
            let (clock, meridiem) = short.split_at(short.find(' ').unwrap_or(short.len()));
            assert_eq!(
                format_timestamp(iso, format),
                format!("{clock}:17{meridiem}")
            );
        }
        assert_eq!(
            format_timestamp("not a date", TimestampFormat::TwelveHour),
            ""
        );
    }
}
