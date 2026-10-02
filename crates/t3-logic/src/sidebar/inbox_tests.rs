//! The sectioned inbox sidebar (fork `Sidebar.tsx`, client-runtime `threadSettled.ts` and
//! `threadSort.ts`). Failure modes this covers, written before the tests:
//!
//! Snooze and settle (`threadSettled.ts`):
//! 1. A user message no turn adopted yet ("queued turn start") is detected only within the 2 min
//!    grace window on either side (clock skew), is cleared by a session error, and counts when
//!    there is no latest turn at all.
//! 2. Snoozing is refused while the agent waits on the user (approval, input, queued turn), but a
//!    running session is snoozable.
//! 3. A snooze hides the thread only while its wake time is in the future and the thread has not
//!    raised its hand (approval, input, an error newer than the snooze, a completion after the
//!    snooze). Malformed wake times never hide a thread. An error older than the snooze keeps it
//!    snoozed.
//! 4. "Woke" reports the timer wake time once it passes, nothing while snoozed, and the
//!    triggering completion time for an early wake even after the timer passes.
//! 5. Snooze presets: "This evening" only while more than an hour before 18:00; "Tomorrow" is
//!    9:00 the next calendar day; "Next week" is the next Monday 9:00 and collapses into
//!    "Tomorrow" on Sundays.
//! 6. Wake labels round minutes up and never read "0m"; past or malformed wake times read "now".
//! 7. Custom snooze rejects non-positive durations, malformed or rolled-over dates (Feb 30), and
//!    times in the past.
//!
//! Ordering (`threadSort.ts`):
//! 8. Settled rows sort by settledAt, else the newest message/turn stamp, else updatedAt, newest
//!    first with an id tiebreak; malformed stamps are skipped, not sunk to the epoch.
//! 9. Pinned rows: keyed first by key (then id, then environment), keyless below, newest created
//!    first.
//! 10. Active rows: keyless (new or reopened) rows lead, by max(createdAt, unsettledAt); keyed
//!     rows follow in key order.
//! 11. Order keys: a key between two neighbors sorts strictly between them and never ends in the
//!     minimum digit; corrupt or out-of-order neighbors give `None`; spread keys are sorted,
//!     unique, and widen for long lists.
//! 12. Reorder plans write one key when both neighbors are keyed, rewrite the section when a
//!     neighbor is keyless, never reuse a hidden row's key, and moves off either end give `None`.
//! 13. With the Working shelf on, the inbox orders by when each thread last came back to the
//!     user, including returns this client observed.
//!
//! Status (`Sidebar.logic.ts`):
//! 14. Pills: approval > input > Working (running) / Connecting (starting) > Plan Ready >
//!     background Working > Monitoring > Completed. There is no Error pill. An unseen completion
//!     beats Monitoring. A thread never visited has no unseen completion.
//! 15. Row status: a failed session beats background liveness; recede rules keep input rows, the
//!     active row, and unread or woke rows prominent.
//! 16. A plan prompt keeps a thread out of the Working shelf.
//! 17. Working duration labels: seconds, minutes, then hours and minutes.
//!
//! Sections (`build_inbox`):
//! 18. Precedence: snoozed > settled > pinned > inbox. Environments without the snooze or
//!     settlement capability never classify threads that way. Archived threads and threads
//!     outside the project scope never appear.
//! 19. The Working shelf takes only inbox threads (pins stay pinned), and only when enabled.
//! 20. Snoozed rows sort soonest wake first.
//! 21. Inbox returns: the first observation only takes a baseline; a thread that stops working is
//!     stamped and leads the inbox; deleted threads leave the map.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use chrono::{FixedOffset, TimeZone};
use serde_json::{Value, json};
use t3_protocol::{
    EnvironmentId, ProjectId, environment::ExecutionEnvironmentCapabilities,
    orchestration::OrchestrationThreadShell,
};

use super::{tests::thread, *};
use crate::{refs::ProjectRef, time::parse_timestamp, ui_state::UiState};

const NOW: &str = "2026-10-02T12:00:00.000Z";

fn ms(value: &str) -> i64 {
    parse_timestamp(value).unwrap()
}

fn shell(extra: Value) -> Arc<OrchestrationThreadShell> {
    thread("t", "p", "2026-10-01T00:00:00.000Z", extra)
}

fn turn(requested: &str, started: Option<&str>, completed: Option<&str>, state: &str) -> Value {
    json!({"turnId": "turn", "state": state, "requestedAt": requested, "startedAt": started, "completedAt": completed})
}

fn session_at(status: &str, updated_at: &str) -> Value {
    json!({"threadId": "t", "status": status, "activeTurnId": null, "updatedAt": updated_at})
}

#[test]
fn queued_turn_start_needs_a_fresh_unadopted_message() {
    let now = ms(NOW);
    // Message one minute ago, no turn yet: queued.
    let fresh = shell(json!({"latestUserMessageAt": "2026-10-02T11:59:00.000Z"}));
    assert!(has_queued_turn_start(&fresh, now));
    // Older than the grace window: a failed start, not pending work.
    let stale = shell(json!({"latestUserMessageAt": "2026-10-02T11:57:00.000Z"}));
    assert!(!has_queued_turn_start(&stale, now));
    // A message from a device whose clock runs 3 minutes ahead.
    let skewed = shell(json!({"latestUserMessageAt": "2026-10-02T12:03:00.000Z"}));
    assert!(!has_queued_turn_start(&skewed, now));
    // Adopted: the turn was requested at the message time.
    let adopted = shell(json!({
        "latestUserMessageAt": "2026-10-02T11:59:00.000Z",
        "latestTurn": turn("2026-10-02T11:59:00.000Z", None, None, "running"),
    }));
    assert!(!has_queued_turn_start(&adopted, now));
    // Newer than the previous turn's stamps: queued.
    let after_turn = shell(json!({
        "latestUserMessageAt": "2026-10-02T11:59:30.000Z",
        "latestTurn": turn("2026-10-02T11:50:00.000Z", Some("2026-10-02T11:50:00.000Z"), Some("2026-10-02T11:58:00.000Z"), "completed"),
    }));
    assert!(has_queued_turn_start(&after_turn, now));
    // A failed session start clears it.
    let failed = shell(json!({
        "latestUserMessageAt": "2026-10-02T11:59:00.000Z",
        "session": session_at("error", "2026-10-02T11:59:10.000Z"),
    }));
    assert!(!has_queued_turn_start(&failed, now));
}

#[test]
fn snoozing_is_refused_while_the_agent_waits_on_the_user() {
    let now = ms(NOW);
    assert!(!can_snooze(
        &shell(json!({"hasPendingApprovals": true})),
        now
    ));
    assert!(!can_snooze(
        &shell(json!({"hasPendingUserInput": true})),
        now
    ));
    assert!(!can_snooze(
        &shell(json!({"latestUserMessageAt": "2026-10-02T11:59:00.000Z"})),
        now
    ));
    assert!(can_snooze(
        &shell(json!({"session": session_at("running", "2026-10-02T11:00:00.000Z")})),
        now
    ));
}

#[test]
fn snooze_hides_until_wake_or_a_raised_hand() {
    let now = ms(NOW);
    let snoozed = |extra: Value| {
        let mut base = json!({
            "snoozedUntil": "2026-10-02T15:00:00.000Z",
            "snoozedAt": "2026-10-02T10:00:00.000Z",
        });
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        shell(base)
    };
    assert!(effective_snoozed(&snoozed(json!({})), now));
    assert!(!effective_snoozed(
        &snoozed(json!({"snoozedUntil": "2026-10-02T11:00:00.000Z"})),
        now
    ));
    assert!(!effective_snoozed(
        &snoozed(json!({"snoozedUntil": "not a date"})),
        now
    ));
    assert!(!effective_snoozed(
        &snoozed(json!({"hasPendingApprovals": true})),
        now
    ));
    assert!(!effective_snoozed(
        &snoozed(json!({"hasPendingUserInput": true})),
        now
    ));
    // An error after the snooze raises a hand; one from before keeps the snooze.
    assert!(!effective_snoozed(
        &snoozed(json!({"session": session_at("error", "2026-10-02T11:00:00.000Z")})),
        now
    ));
    assert!(effective_snoozed(
        &snoozed(json!({"session": session_at("error", "2026-10-02T09:00:00.000Z")})),
        now
    ));
    // A run that completed after the snooze.
    let completed = turn(
        "2026-10-02T10:30:00.000Z",
        Some("2026-10-02T10:30:00.000Z"),
        Some("2026-10-02T11:00:00.000Z"),
        "completed",
    );
    assert!(!effective_snoozed(
        &snoozed(json!({"latestTurn": completed})),
        now
    ));
    assert!(!effective_snoozed(&shell(json!({})), now));
}

#[test]
fn woke_at_reports_the_wake_signal() {
    let base = json!({"snoozedUntil": "2026-10-02T15:00:00.000Z", "snoozedAt": "2026-10-02T10:00:00.000Z"});
    let thread = shell(base.clone());
    assert_eq!(thread_woke_at(&thread, ms(NOW)), None);
    assert_eq!(
        thread_woke_at(&thread, ms("2026-10-02T16:00:00.000Z")).as_deref(),
        Some("2026-10-02T15:00:00.000Z")
    );
    let mut early = base;
    early["latestTurn"] = turn(
        "2026-10-02T10:30:00.000Z",
        Some("2026-10-02T10:30:00.000Z"),
        Some("2026-10-02T11:00:00.000Z"),
        "completed",
    );
    let early = shell(early);
    // Still reports the completion after the timer passed, not the timer.
    assert_eq!(
        thread_woke_at(&early, ms("2026-10-02T16:00:00.000Z")).as_deref(),
        Some("2026-10-02T11:00:00.000Z")
    );
    assert_eq!(thread_woke_at(&shell(json!({})), ms(NOW)), None);
}

#[test]
fn snooze_presets_follow_the_local_calendar() {
    let zone = FixedOffset::west_opt(5 * 3600).unwrap();
    let labels = |now| {
        resolve_snooze_presets(now)
            .into_iter()
            .map(|preset| (preset.id, preset.when_label, preset.snoozed_until))
            .collect::<Vec<_>>()
    };

    // Friday 10:00 local: every preset.
    let friday = zone.with_ymd_and_hms(2026, 10, 2, 10, 0, 0).unwrap();
    let presets = labels(friday);
    let ids: Vec<_> = presets.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(
        ids,
        [
            SnoozePresetId::Hour,
            SnoozePresetId::ThreeHours,
            SnoozePresetId::Evening,
            SnoozePresetId::Tomorrow,
            SnoozePresetId::NextWeek,
        ]
    );
    assert_eq!(presets[0].1, "11:00 AM");
    assert_eq!(presets[2].1, "6:00 PM");
    assert_eq!(presets[3].2, "2026-10-03T14:00:00.000Z");
    assert_eq!(presets[4].1, "Mon 9:00 AM");
    assert_eq!(presets[4].2, "2026-10-05T14:00:00.000Z");

    // 17:30: less than an hour before evening, so no evening preset.
    let late = zone.with_ymd_and_hms(2026, 10, 2, 17, 30, 0).unwrap();
    assert!(
        !labels(late)
            .iter()
            .any(|(id, _, _)| *id == SnoozePresetId::Evening)
    );

    // Sunday: tomorrow is Monday morning, so "Next week" collapses.
    let sunday = zone.with_ymd_and_hms(2026, 10, 4, 20, 0, 0).unwrap();
    assert!(
        !labels(sunday)
            .iter()
            .any(|(id, _, _)| *id == SnoozePresetId::NextWeek)
    );
}

#[test]
fn wake_labels_round_up() {
    let now = ms(NOW);
    assert_eq!(snooze_wake_label("2026-10-02T12:00:10.000Z", now), "1m");
    assert_eq!(snooze_wake_label("2026-10-02T12:30:00.000Z", now), "30m");
    assert_eq!(snooze_wake_label("2026-10-02T14:00:01.000Z", now), "3h");
    assert_eq!(snooze_wake_label("2026-10-04T12:00:00.000Z", now), "2d");
    assert_eq!(snooze_wake_label("2026-10-02T11:00:00.000Z", now), "now");
    assert_eq!(snooze_wake_label("garbage", now), "now");
}

#[test]
fn custom_snooze_rejects_invalid_and_past_input() {
    let zone = FixedOffset::west_opt(5 * 3600).unwrap();
    let now = zone.with_ymd_and_hms(2026, 10, 2, 10, 0, 0).unwrap();
    let duration = |amount: &str, unit| CustomSnooze::Duration {
        amount: amount.into(),
        unit,
    };
    assert_eq!(
        resolve_custom_snooze(&duration("90", SnoozeUnit::Minutes), now).as_deref(),
        Some("2026-10-02T16:30:00.000Z")
    );
    assert_eq!(
        resolve_custom_snooze(&duration("0", SnoozeUnit::Hours), now),
        None
    );
    assert_eq!(
        resolve_custom_snooze(&duration("-1", SnoozeUnit::Days), now),
        None
    );
    assert_eq!(
        resolve_custom_snooze(&duration("soon", SnoozeUnit::Days), now),
        None
    );
    let date = |date: &str, time: &str| CustomSnooze::Date {
        date: date.into(),
        time: time.into(),
    };
    assert_eq!(
        resolve_custom_snooze(&date("2026-10-03", "08:15"), now).as_deref(),
        Some("2026-10-03T13:15:00.000Z")
    );
    assert_eq!(
        resolve_custom_snooze(&date("2026-02-30", "08:00"), now),
        None
    );
    assert_eq!(
        resolve_custom_snooze(&date("2026-10-02", "09:00"), now),
        None
    );
    assert_eq!(
        resolve_custom_snooze(&date("10/3/2026", "08:00"), now),
        None
    );
    assert_eq!(resolve_custom_snooze(&date("2026-10-03", "8am"), now), None);
}

fn ids(threads: &[Arc<OrchestrationThreadShell>]) -> Vec<&str> {
    threads.iter().map(|t| t.id.as_str()).collect()
}

fn named(id: &str, extra: Value) -> Arc<OrchestrationThreadShell> {
    thread(id, "p", "2026-10-01T00:00:00.000Z", extra)
}

#[test]
fn settled_rows_sort_by_when_work_ended() {
    let by_settled = named("a", json!({"settledAt": "2026-10-02T09:00:00.000Z"}));
    let by_turn = named(
        "b",
        json!({"latestTurn": turn("2026-10-02T08:00:00.000Z", None, Some("2026-10-02T10:00:00.000Z"), "completed")}),
    );
    let malformed = named(
        "c",
        json!({"settledAt": "nope", "latestUserMessageAt": "2026-10-02T11:00:00.000Z"}),
    );
    let fallback = named("d", json!({}));
    assert_eq!(
        resolve_settled_thread_timestamp(&malformed).as_deref(),
        Some("2026-10-02T11:00:00.000Z")
    );
    assert_eq!(
        resolve_settled_thread_timestamp(&fallback).as_deref(),
        Some("2026-10-01T00:00:00.000Z")
    );
    let tie = named("0", json!({"settledAt": "2026-10-02T09:00:00.000Z"}));
    let sorted = sort_settled_threads(vec![fallback, by_settled, malformed, tie, by_turn]);
    assert_eq!(ids(&sorted), ["c", "b", "0", "a", "d"]);
}

#[test]
fn pinned_and_active_orders() {
    let created = |id: &str, at: &str, extra: Value| {
        let mut value = extra;
        value["createdAt"] = json!(at);
        named(id, value)
    };
    let pinned = sort_pinned_threads(vec![
        created("old", "2026-09-01T00:00:00.000Z", json!({})),
        created(
            "k2",
            "2026-09-01T00:00:00.000Z",
            json!({"pinOrderKey": "t"}),
        ),
        created("new", "2026-10-01T00:00:00.000Z", json!({})),
        created(
            "k1",
            "2026-09-01T00:00:00.000Z",
            json!({"pinOrderKey": "m"}),
        ),
    ]);
    assert_eq!(ids(&pinned), ["k1", "k2", "new", "old"]);

    let active = sort_active_threads(vec![
        created(
            "keyed-b",
            "2026-10-01T00:00:00.000Z",
            json!({"activeOrderKey": "t"}),
        ),
        created("old", "2026-09-01T00:00:00.000Z", json!({})),
        created(
            "reopened",
            "2026-08-01T00:00:00.000Z",
            json!({"unsettledAt": "2026-10-02T00:00:00.000Z"}),
        ),
        created(
            "keyed-a",
            "2026-10-01T00:00:00.000Z",
            json!({"activeOrderKey": "m"}),
        ),
    ]);
    assert_eq!(ids(&active), ["reopened", "old", "keyed-a", "keyed-b"]);
}

#[test]
fn order_keys_sort_between_neighbors() {
    assert_eq!(pin_order_key_between(None, None).as_deref(), Some("n"));
    // Exact values match the web (`Math.round` rounds .5 up).
    assert_eq!(
        pin_order_key_between(Some("b"), Some("e")).as_deref(),
        Some("d")
    );
    assert_eq!(generate_spread_pin_order_keys(1), ["nb"]);
    let mut cases = vec![
        (Some("b"), Some("c")),
        (Some("n"), None),
        (None, Some("b")),
        (Some("az"), Some("b")),
        (Some("mz"), Some("n")),
        (Some("y"), Some("z")),
        (Some("z"), None),
    ];
    // Repeatedly squeezing into the same gap keeps working.
    let mut low = "m".to_owned();
    for _ in 0..30 {
        let key = pin_order_key_between(Some(&low), Some("n")).unwrap();
        assert!(
            low.as_str() < key.as_str() && key.as_str() < "n",
            "{low} < {key} < n"
        );
        assert!(!key.ends_with('a'));
        low = key;
    }
    for (before, after) in cases.drain(..) {
        let key = pin_order_key_between(before, after).unwrap();
        assert!(
            before.is_none_or(|b| b < key.as_str()),
            "{before:?} < {key}"
        );
        assert!(after.is_none_or(|a| key.as_str() < a), "{key} < {after:?}");
        assert!(!key.ends_with('a'), "{key}");
    }
    assert_eq!(pin_order_key_between(Some("c"), Some("b")), None);
    assert_eq!(pin_order_key_between(Some("ba"), None), None);
    assert_eq!(pin_order_key_between(Some("B"), None), None);

    for count in [1, 5, 300, 2000] {
        let keys = generate_spread_pin_order_keys(count);
        assert_eq!(keys.len(), count);
        assert!(keys.windows(2).all(|pair| pair[0] < pair[1]), "{count}");
        assert!(keys.iter().all(|key| !key.ends_with('a')));
    }
}

#[test]
fn reorder_plans_write_as_little_as_possible() {
    let keys = |pairs: &[(&str, Option<&str>)]| -> HashMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(id, key)| (id.to_string(), key.map(str::to_owned)))
            .collect()
    };
    let order = |list: &[&str]| list.iter().map(|id| id.to_string()).collect::<Vec<_>>();

    // Both neighbors keyed: one write, strictly between them.
    let all_keyed = keys(&[("a", Some("d")), ("b", Some("h")), ("c", Some("p"))]);
    let plan = plan_pinned_reorder(&order(&["a", "c", "b"]), &all_keyed, "c");
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].0, "c");
    assert!("d" < plan[0].1.as_str() && plan[0].1.as_str() < "h");

    // A keyless neighbor: the visible section is rewritten in the new order.
    let mixed = keys(&[
        ("a", Some("d")),
        ("b", None),
        ("c", Some("p")),
        ("hidden", Some("n")),
    ]);
    let plan = plan_pinned_reorder(&order(&["b", "a", "c"]), &mixed, "a");
    let assigned: HashMap<_, _> = plan.iter().cloned().collect();
    let new_keys: Vec<&str> = ["b", "a", "c"]
        .iter()
        .map(|id| {
            assigned
                .get(*id)
                .map(String::as_str)
                .or(mixed[*id].as_deref())
                .unwrap()
        })
        .collect();
    assert!(
        new_keys.windows(2).all(|pair| pair[0] < pair[1]),
        "{new_keys:?}"
    );
    assert!(!new_keys.contains(&"n"), "reused the hidden row's key");
    assert!(!assigned.contains_key("hidden"));

    // Moves.
    let move_plan =
        |direction| plan_pinned_move(&order(&["a", "b", "c"]), &all_keyed, "a", direction);
    assert!(move_plan(MoveDirection::Up).is_none());
    let down = move_plan(MoveDirection::Down).unwrap();
    assert_eq!(down.len(), 1);
    assert!("h" < down[0].1.as_str() && down[0].1.as_str() < "p");
}

#[test]
fn inbox_orders_by_return_to_the_user() {
    let created = |id: &str, extra: Value| {
        let mut value = extra;
        value["createdAt"] = json!("2026-09-01T00:00:00.000Z");
        named(id, value)
    };
    let threads = vec![
        created("old", json!({})),
        created(
            "finished",
            json!({"latestTurn": turn("2026-10-01T00:00:00.000Z", None, Some("2026-10-02T09:00:00.000Z"), "completed")}),
        ),
        created("observed", json!({})),
    ];
    let observed = HashMap::from([("observed".to_owned(), ms("2026-10-02T10:00:00.000Z"))]);
    let sorted =
        sort_inbox_threads_by_return(threads, |thread| observed.get(thread.id.as_str()).copied());
    assert_eq!(ids(&sorted), ["observed", "finished", "old"]);
}

#[test]
fn status_pills_follow_the_fork() {
    let visited = Some("2026-10-02T08:00:00.000Z");
    let done = turn(
        "2026-10-02T09:00:00.000Z",
        Some("2026-10-02T09:00:00.000Z"),
        Some("2026-10-02T10:00:00.000Z"),
        "completed",
    );
    let pill = |extra: Value, visited| resolve_thread_status(&shell(extra), visited);
    assert_eq!(
        pill(
            json!({"hasPendingApprovals": true, "hasPendingUserInput": true}),
            visited
        ),
        Some(ThreadStatus::PendingApproval)
    );
    assert_eq!(
        pill(json!({"session": session_at("running", NOW)}), visited),
        Some(ThreadStatus::Working)
    );
    assert_eq!(
        pill(json!({"session": session_at("starting", NOW)}), visited),
        Some(ThreadStatus::Connecting)
    );
    // No Error pill: a failed session with nothing else shows nothing.
    assert_eq!(
        pill(json!({"session": session_at("error", NOW)}), visited),
        None
    );
    assert_eq!(
        pill(json!({"backgroundLiveness": "working"}), visited),
        Some(ThreadStatus::Working)
    );
    assert_eq!(
        pill(json!({"backgroundLiveness": "monitoring"}), visited),
        Some(ThreadStatus::Monitoring)
    );
    // An unseen completion beats Monitoring.
    assert_eq!(
        pill(
            json!({"backgroundLiveness": "monitoring", "latestTurn": done.clone()}),
            visited
        ),
        Some(ThreadStatus::Completed)
    );
    // Never visited: no unseen completion (fork `hasUnseenCompletion`).
    assert_eq!(pill(json!({"latestTurn": done.clone()}), None), None);
    // Plan Ready outranks background work.
    assert_eq!(
        pill(
            json!({"interactionMode": "plan", "hasActionableProposedPlan": true, "latestTurn": done, "backgroundLiveness": "working"}),
            visited
        ),
        Some(ThreadStatus::PlanReady)
    );
    assert_eq!(
        highest_status([
            Some(ThreadStatus::Monitoring),
            Some(ThreadStatus::PlanReady),
            Some(ThreadStatus::Completed)
        ]),
        Some(ThreadStatus::PlanReady)
    );
}

#[test]
fn row_status_and_recede() {
    let status = |extra: Value| resolve_sidebar_thread_status(&shell(extra));
    assert_eq!(
        status(json!({"session": session_at("error", NOW), "backgroundLiveness": "working"})),
        SidebarThreadStatus::Failed
    );
    assert_eq!(
        status(json!({"session": session_at("starting", NOW)})),
        SidebarThreadStatus::Working
    );
    assert_eq!(
        status(json!({"backgroundLiveness": "monitoring"})),
        SidebarThreadStatus::Monitoring
    );
    assert_eq!(status(json!({})), SidebarThreadStatus::Ready);

    let recede = |status, unread, woke, active| {
        should_recede_sidebar_thread(RecedeInput {
            status,
            is_unread: unread,
            is_woke: woke,
            is_active: active,
            is_selected: false,
        })
    };
    assert!(recede(SidebarThreadStatus::Working, false, false, false));
    assert!(!recede(SidebarThreadStatus::Working, false, false, true));
    assert!(!recede(SidebarThreadStatus::Input, false, false, false));
    assert!(!recede(SidebarThreadStatus::Monitoring, true, false, false));
    assert!(recede(SidebarThreadStatus::Ready, false, false, false));
    assert!(!recede(SidebarThreadStatus::Ready, false, true, false));
    assert!(!recede(SidebarThreadStatus::Failed, false, false, false));
}

#[test]
fn working_shelf_membership_and_duration() {
    let done = turn(
        "2026-10-02T09:00:00.000Z",
        Some("2026-10-02T09:00:00.000Z"),
        Some("2026-10-02T10:00:00.000Z"),
        "completed",
    );
    assert!(is_sidebar_thread_working(&shell(
        json!({"backgroundLiveness": "monitoring"})
    )));
    assert!(!is_sidebar_thread_working(&shell(json!({
        "backgroundLiveness": "working", "interactionMode": "plan", "hasActionableProposedPlan": true, "latestTurn": done,
    }))));
    assert!(!is_sidebar_thread_working(&shell(
        json!({"hasPendingApprovals": true, "session": session_at("running", NOW)})
    )));
    assert_eq!(format_working_duration_label(42_900), "42s");
    assert_eq!(format_working_duration_label(5 * 60_000), "5m");
    assert_eq!(format_working_duration_label(125 * 60_000), "2h 5m");
    assert_eq!(format_working_duration_label(-5), "0s");
}

// ---------------------------------------------------------------------------------------------
// Sections

fn caps(settle: bool, snooze: bool) -> ExecutionEnvironmentCapabilities {
    serde_json::from_value(json!({
        "threadSettlement": settle, "threadSnooze": snooze, "threadPinning": true,
        "threadPinReorder": true, "threadActiveReorder": true,
    }))
    .unwrap()
}

fn inbox(
    threads: Vec<Arc<OrchestrationThreadShell>>,
    capabilities: ExecutionEnvironmentCapabilities,
    working_shelf: bool,
    scope: Option<HashSet<ProjectRef>>,
    returns: &InboxReturns,
) -> InboxModel {
    let env = EnvironmentId::from("e");
    let environments = [InboxEnvironment {
        id: &env,
        threads: &threads,
        capabilities: Some(&capabilities),
    }];
    let ui = UiState::default();
    build_inbox(&InboxInputs {
        environments: &environments,
        scope: scope.as_ref(),
        working_shelf_enabled: working_shelf,
        now: ms(NOW),
        ui: &ui,
        route_thread: None,
        returns,
    })
}

fn section_ids(rows: &[InboxThread]) -> Vec<&str> {
    rows.iter().map(|row| row.thread.id.as_str()).collect()
}

#[test]
fn sections_classify_with_capabilities_and_scope() {
    let threads = vec![
        named(
            "snoozed-settled",
            json!({"snoozedUntil": "2026-10-02T15:00:00.000Z", "snoozedAt": "2026-10-02T10:00:00.000Z", "settledOverride": "settled"}),
        ),
        named(
            "settled-pinned",
            json!({"settledOverride": "settled", "pinnedAt": "2026-10-01T00:00:00.000Z"}),
        ),
        named("pinned", json!({"pinnedAt": "2026-10-01T00:00:00.000Z"})),
        named("plain", json!({})),
        named(
            "archived",
            json!({"archivedAt": "2026-10-01T00:00:00.000Z"}),
        ),
        thread("other-project", "q", "2026-10-01T00:00:00.000Z", json!({})),
    ];
    let returns = InboxReturns::default();
    let model = inbox(threads.clone(), caps(true, true), false, None, &returns);
    assert_eq!(section_ids(&model.snoozed), ["snoozed-settled"]);
    assert_eq!(section_ids(&model.settled), ["settled-pinned"]);
    assert_eq!(section_ids(&model.pinned), ["pinned"]);
    assert_eq!(model.active.len(), 2);
    assert!(model.working.is_empty());

    // Without the capabilities nothing is snoozed or settled.
    let model = inbox(threads.clone(), caps(false, false), false, None, &returns);
    assert!(model.snoozed.is_empty() && model.settled.is_empty());
    assert_eq!(section_ids(&model.pinned), ["pinned", "settled-pinned"]);
    assert!(section_ids(&model.active).contains(&"snoozed-settled"));

    // Scoped to project q.
    let scope = HashSet::from([ProjectRef::new(
        EnvironmentId::from("e"),
        ProjectId::from("q"),
    )]);
    let model = inbox(threads, caps(true, true), false, Some(scope), &returns);
    assert_eq!(section_ids(&model.active), ["other-project"]);
    assert!(model.pinned.is_empty() && model.settled.is_empty() && model.snoozed.is_empty());
}

#[test]
fn working_shelf_takes_only_inbox_threads() {
    let threads = vec![
        named("busy", json!({"session": session_at("running", NOW)})),
        named(
            "pinned-busy",
            json!({"session": session_at("running", NOW), "pinnedAt": "2026-10-01T00:00:00.000Z"}),
        ),
        named(
            "asks",
            json!({"session": session_at("running", NOW), "hasPendingApprovals": true}),
        ),
        named("idle", json!({})),
    ];
    let returns = InboxReturns::default();
    let off = inbox(threads.clone(), caps(true, true), false, None, &returns);
    assert!(off.working.is_empty());
    assert_eq!(off.active.len(), 3);
    let on = inbox(threads, caps(true, true), true, None, &returns);
    assert_eq!(section_ids(&on.working), ["busy"]);
    assert_eq!(section_ids(&on.pinned), ["pinned-busy"]);
    let mut active = section_ids(&on.active);
    active.sort();
    assert_eq!(active, ["asks", "idle"]);
}

#[test]
fn snoozed_shelf_sorts_soonest_wake_first() {
    let threads = vec![
        named(
            "later",
            json!({"snoozedUntil": "2026-10-03T09:00:00.000Z", "snoozedAt": "2026-10-02T10:00:00.000Z"}),
        ),
        named(
            "sooner",
            json!({"snoozedUntil": "2026-10-02T13:00:00.000Z", "snoozedAt": "2026-10-02T10:00:00.000Z"}),
        ),
    ];
    let model = inbox(
        threads,
        caps(true, true),
        false,
        None,
        &InboxReturns::default(),
    );
    assert_eq!(section_ids(&model.snoozed), ["sooner", "later"]);
    assert_eq!(model.snoozed[0].snooze_wake_label.as_deref(), Some("1h"));
}

#[test]
fn inbox_returns_stamp_threads_that_stop_working() {
    let env = EnvironmentId::from("e");
    let busy = named("t1", json!({"session": session_at("running", NOW)}));
    let idle = named("t1", json!({}));
    let other = named("t2", json!({}));
    let mut returns = InboxReturns::default();
    // Baseline only: nothing stamped even though t2 is idle.
    returns.observe([(&env, &busy), (&env, &other)], 1_000);
    assert!(
        returns
            .returned_at(&ThreadRef::new(env.clone(), "t2".into()))
            .is_none()
    );
    // t1 stops working.
    returns.observe([(&env, &idle), (&env, &other)], 2_000);
    assert_eq!(
        returns.returned_at(&ThreadRef::new(env.clone(), "t1".into())),
        Some(2_000)
    );
    // t1 deleted: dropped from the map.
    returns.observe([(&env, &other)], 3_000);
    assert!(
        returns
            .returned_at(&ThreadRef::new(env.clone(), "t1".into()))
            .is_none()
    );
    // Resetting (shelf turned off) clears the baseline.
    returns.reset();
    returns.observe([(&env, &idle)], 4_000);
    assert!(
        returns
            .returned_at(&ThreadRef::new(env, "t1".into()))
            .is_none()
    );
}
