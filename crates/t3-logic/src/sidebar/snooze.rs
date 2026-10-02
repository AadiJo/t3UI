//! Snooze and settle rules (client-runtime `state/threadSettled.ts`, identical in upstream
//! b33eda13 and the fork). Snooze is an overlay on the active state: the server keeps
//! `snoozedUntil`/`snoozedAt`, and these functions decide whether a thread currently reads as
//! snoozed, may be snoozed, or just woke.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone};
use t3_protocol::orchestration::{OrchestrationThreadShell, SessionStatus, TurnState};

use crate::time::{EpochMillis, format_timestamp, parse_timestamp};

/// How long a user message may wait for a turn to adopt it before it counts as a failed start
/// rather than pending work.
pub const QUEUED_TURN_START_GRACE_MS: EpochMillis = 2 * 60 * 1_000;
const HOUR_MS: EpochMillis = 60 * 60 * 1_000;
const DAY_MS: EpochMillis = 24 * HOUR_MS;
const EVENING_HOUR: u32 = 18;
const MORNING_HOUR: u32 = 9;

fn stamp(value: Option<&str>) -> Option<EpochMillis> {
    value.and_then(parse_timestamp)
}

/// A user message no turn has picked up yet: newer than every stamp on the latest turn, and
/// within the grace window on either side of `now` (message times come from the sending
/// device's clock). A session error clears it; the failure is already visible.
pub fn has_queued_turn_start(thread: &OrchestrationThreadShell, now: EpochMillis) -> bool {
    let Some(message_at) = stamp(thread.latest_user_message_at.as_deref()) else {
        return false;
    };
    if thread.session.as_ref().map(|s| &s.status) == Some(&SessionStatus::Error) {
        return false;
    }
    if (now - message_at).abs() > QUEUED_TURN_START_GRACE_MS {
        return false;
    }
    let Some(turn) = &thread.latest_turn else {
        return true;
    };
    [
        Some(turn.requested_at.as_str()),
        turn.started_at.as_deref(),
        turn.completed_at.as_deref(),
    ]
    .into_iter()
    .flatten()
    // An unparseable stamp fails the check (`Date.parse` NaN comparisons are false).
    .all(|candidate| stamp(Some(candidate)).is_some_and(|at| at < message_at))
}

/// A run that completed after the snooze was set.
fn completed_after_snooze(thread: &OrchestrationThreadShell) -> Option<&str> {
    let snoozed_at = stamp(thread.snoozed_at.as_deref())?;
    let turn = thread.latest_turn.as_ref()?;
    let completed_at = turn.completed_at.as_deref()?;
    (turn.state == TurnState::Completed && stamp(Some(completed_at))? > snoozed_at)
        .then_some(completed_at)
}

/// Something outranks the user's snooze: the agent waits on them, the session failed after the
/// snooze, or a run completed after it. Raising a hand never clears the server-side fields.
pub fn raised_hand_while_snoozed(thread: &OrchestrationThreadShell) -> bool {
    if thread.has_pending_approvals || thread.has_pending_user_input {
        return true;
    }
    if let Some(session) = &thread.session
        && session.status == SessionStatus::Error
    {
        // Only a fresh failure: a thread snoozed while already failed stays snoozed.
        let fresh = match thread.snoozed_at.as_deref() {
            None => true,
            Some(snoozed_at) => matches!(
                (stamp(Some(&session.updated_at)), stamp(Some(snoozed_at))),
                (Some(failed), Some(snoozed)) if failed > snoozed
            ),
        };
        if fresh {
            return true;
        }
    }
    completed_after_snooze(thread).is_some()
}

/// Whether the user may snooze the thread: not while the agent waits on them (approval, input,
/// a queued turn). A running session is snoozable; snooze only affects visibility.
pub fn can_snooze(thread: &OrchestrationThreadShell, now: EpochMillis) -> bool {
    !(thread.has_pending_approvals
        || thread.has_pending_user_input
        || has_queued_turn_start(thread, now))
}

/// Hidden in the Snoozed shelf: the wake time is in the future and the thread has not raised
/// its hand. Malformed wake times never hide a thread.
pub fn effective_snoozed(thread: &OrchestrationThreadShell, now: EpochMillis) -> bool {
    match stamp(thread.snoozed_until.as_deref()) {
        Some(wake_at) if wake_at > now => !raised_hand_while_snoozed(thread),
        _ => false,
    }
}

/// When a snoozed thread woke, for the "Woke" indicator; `None` if it never snoozed or still
/// is. Early wakes report the triggering time even after the timer passes, so a visit made
/// before the early wake does not clear the indicator.
pub fn thread_woke_at(thread: &OrchestrationThreadShell, now: EpochMillis) -> Option<String> {
    let snoozed_until = thread.snoozed_until.as_deref()?;
    let wake_at = stamp(Some(snoozed_until))?;
    if raised_hand_while_snoozed(thread) {
        if let Some(completed_at) = completed_after_snooze(thread) {
            return Some(completed_at.to_owned());
        }
        return thread
            .session
            .as_ref()
            .map(|session| session.updated_at.clone())
            .or_else(|| thread.snoozed_at.clone());
    }
    (wake_at <= now).then(|| snoozed_until.to_owned())
}

/// The snooze menu's choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnoozePresetId {
    Hour,
    ThreeHours,
    Evening,
    Tomorrow,
    NextWeek,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnoozePreset {
    pub id: SnoozePresetId,
    /// "In 1 hour", "This evening", ...
    pub label: &'static str,
    /// The menu row's time column ("9:00 AM", "Mon 9:00 AM"): complements the label.
    pub when_label: String,
    /// ISO wake time for `commands::snooze_thread`.
    pub snoozed_until: String,
}

fn time_of_day_label<Tz: TimeZone>(at: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.format("%-I:%M %p").to_string()
}

/// `date` at `hour:00` local, the earliest instant if the clock repeats (DST fall back).
fn at_local_hour<Tz: TimeZone>(zone: &Tz, date: NaiveDate, hour: u32) -> Option<DateTime<Tz>> {
    zone.from_local_datetime(&date.and_hms_opt(hour, 0, 0)?)
        .earliest()
}

fn iso<Tz: TimeZone>(at: &DateTime<Tz>) -> String {
    format_timestamp(at.timestamp_millis()).unwrap_or_default()
}

/// Snooze menu presets for `now` in the user's time zone. "This evening" appears only while
/// more than an hour before 18:00. Calendar presets advance by calendar day, not 24 h, so DST
/// transitions land on the right day. On Sundays "Next week" equals "Tomorrow" and is dropped.
pub fn resolve_snooze_presets<Tz: TimeZone>(now: DateTime<Tz>) -> Vec<SnoozePreset>
where
    Tz::Offset: std::fmt::Display,
{
    let zone = now.timezone();
    let in_an_hour = now.clone() + Duration::milliseconds(HOUR_MS);
    let in_three_hours = now.clone() + Duration::milliseconds(3 * HOUR_MS);
    let mut presets = vec![
        SnoozePreset {
            id: SnoozePresetId::Hour,
            label: "In 1 hour",
            when_label: time_of_day_label(&in_an_hour),
            snoozed_until: iso(&in_an_hour),
        },
        SnoozePreset {
            id: SnoozePresetId::ThreeHours,
            label: "In 3 hours",
            when_label: time_of_day_label(&in_three_hours),
            snoozed_until: iso(&in_three_hours),
        },
    ];
    let today = now.date_naive();
    if let Some(evening) = at_local_hour(&zone, today, EVENING_HOUR)
        && evening.timestamp_millis() - now.timestamp_millis() > HOUR_MS
    {
        presets.push(SnoozePreset {
            id: SnoozePresetId::Evening,
            label: "This evening",
            when_label: time_of_day_label(&evening),
            snoozed_until: iso(&evening),
        });
    }
    let tomorrow = today
        .succ_opt()
        .and_then(|date| at_local_hour(&zone, date, MORNING_HOUR));
    if let Some(tomorrow) = &tomorrow {
        presets.push(SnoozePreset {
            id: SnoozePresetId::Tomorrow,
            label: "Tomorrow",
            when_label: time_of_day_label(tomorrow),
            snoozed_until: iso(tomorrow),
        });
    }
    let from_sunday = now.weekday().num_days_from_sunday() as i64;
    let days_until_monday = match (1 - from_sunday + 7) % 7 {
        0 => 7,
        days => days,
    };
    let next_week = today
        .checked_add_signed(Duration::days(days_until_monday))
        .and_then(|date| at_local_hour(&zone, date, MORNING_HOUR));
    if let Some(next_week) = next_week
        && tomorrow.as_ref().map(DateTime::timestamp_millis) != Some(next_week.timestamp_millis())
    {
        presets.push(SnoozePreset {
            id: SnoozePresetId::NextWeek,
            label: "Next week",
            when_label: format!(
                "{} {}",
                next_week.format("%a"),
                time_of_day_label(&next_week)
            ),
            snoozed_until: iso(&next_week),
        });
    }
    presets
}

/// Compact "wakes in" label for snoozed rows: "45m", "3h", "2d". Minutes round up so a hidden
/// row never reads "0m"; past or malformed times read "now".
pub fn snooze_wake_label(snoozed_until: &str, now: EpochMillis) -> String {
    let Some(wake) = parse_timestamp(snoozed_until) else {
        return "now".into();
    };
    let remaining = wake - now;
    let ceil = |unit: EpochMillis| (remaining + unit - 1) / unit;
    if remaining <= 0 {
        "now".into()
    } else if remaining < HOUR_MS {
        format!("{}m", ceil(60_000).max(1))
    } else if remaining < DAY_MS {
        format!("{}h", ceil(HOUR_MS))
    } else {
        format!("{}d", ceil(DAY_MS))
    }
}

/// Units of a custom snooze duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnoozeUnit {
    Minutes,
    Hours,
    Days,
}

/// The custom snooze dialog's input, as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CustomSnooze {
    /// Local `YYYY-MM-DD` and `HH:MM`.
    Date {
        date: String,
        time: String,
    },
    Duration {
        amount: String,
        unit: SnoozeUnit,
    },
}

/// The ISO wake time for custom input, or `None` for invalid input, nonexistent local times
/// (DST gaps), and anything not in the future.
pub fn resolve_custom_snooze<Tz: TimeZone>(
    input: &CustomSnooze,
    now: DateTime<Tz>,
) -> Option<String> {
    let wake_ms = match input {
        CustomSnooze::Duration { amount, unit } => {
            let amount: f64 = amount.trim().parse().ok()?;
            if !amount.is_finite() || amount <= 0.0 {
                return None;
            }
            let unit_ms = match unit {
                SnoozeUnit::Minutes => 60_000.0,
                SnoozeUnit::Hours => HOUR_MS as f64,
                SnoozeUnit::Days => DAY_MS as f64,
            };
            now.timestamp_millis() + (amount * unit_ms) as EpochMillis
        }
        CustomSnooze::Date { date, time } => {
            let digits = |text: &str, pattern: &[usize]| {
                let parts: Vec<&str> = text.split(['-', ':']).collect();
                parts.len() == pattern.len()
                    && parts.iter().zip(pattern).all(|(part, len)| {
                        part.len() == *len && part.bytes().all(|b| b.is_ascii_digit())
                    })
            };
            if !digits(date, &[4, 2, 2])
                || !digits(time, &[2, 2])
                || !time.contains(':')
                || !date.contains('-')
            {
                return None;
            }
            let date = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            let time = NaiveTime::parse_from_str(time, "%H:%M").ok()?;
            now.timezone()
                .from_local_datetime(&date.and_time(time))
                .earliest()?
                .timestamp_millis()
        }
    };
    (wake_ms > now.timestamp_millis())
        .then(|| format_timestamp(wake_ms))
        .flatten()
}

/// `YYYY-MM-DD` of a local time, to prefill the custom snooze dialog.
pub fn local_snooze_date<Tz: TimeZone>(at: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.format("%Y-%m-%d").to_string()
}

/// `HH:MM` of a local time, to prefill the custom snooze dialog.
pub fn local_snooze_time<Tz: TimeZone>(at: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.format("%H:%M").to_string()
}
