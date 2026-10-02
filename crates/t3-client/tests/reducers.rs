//! Reducer cases the recorded transcript does not reach (no working provider in the isolated
//! server): assistant streaming, checkpoints, stable-id activities, revert, delete, paging.
//! Mirrors upstream `threadReducer.ts` / `shellReducer.ts` semantics (protocol.md 5.4).
//!
//! Failure modes:
//! 1. Streaming deltas replace instead of append; the completion event (`text: ""`) wipes the
//!    text; a completion with text does not replace it.
//! 2. An event at or below the cursor is applied again (duplicated text after a resume).
//! 3. `latest_turn` does not start on `session-set running`, or does not settle (completed /
//!    error / interrupted) when the session leaves `running`; a completed assistant message
//!    settles a turn the session is still running.
//! 4. A mid-turn `missing` checkpoint overwrites a real one; checkpoints are unsorted.
//! 5. Activities with a stable id duplicate instead of replacing; a newer resolvable
//!    `context-window.updated` does not drop the older one of the same turn.
//! 6. Revert keeps rows of dropped turns or drops system messages.
//! 7. `thread.deleted` leaves stale data visible.
//! 8. An older page merged after history was replaced (epoch changed) corrupts the thread;
//!    merged pages duplicate rows.
//! 9. Shell: an upsert appends a duplicate row, or replaces unchanged rows (breaking `Arc`
//!    sharing that views rely on); removing an unknown id panics.
//! 10. Streaming a delta clones every message instead of only the streaming one.

use std::sync::Arc;

use serde_json::{Value, json};
use t3_client::{ShellState, SyncStatus, ThreadState};
use t3_protocol::{
    ThreadId,
    orchestration::{
        CheckpointStatus, OrchestrationEvent, OrchestrationThreadDetailSnapshot, SessionStatus,
        ShellStreamItem, ThreadStreamItem, TurnState,
    },
};

const T: &str = "2026-10-01T00:00:00.000Z";

fn thread_json(messages: Value) -> Value {
    json!({
        "id": "t1", "projectId": "p1", "title": "T",
        "modelSelection": {"instanceId": "codex", "model": "gpt-5.5"},
        "runtimeMode": "full-access", "interactionMode": "default",
        "branch": null, "worktreePath": null, "pullRequests": [], "latestTurn": null,
        "createdAt": T, "updatedAt": T, "archivedAt": null, "settledOverride": null,
        "settledAt": null, "deletedAt": null, "messages": messages, "proposedPlans": [],
        "activities": [], "checkpoints": [], "session": null,
    })
}

fn snapshot(sequence: u64, messages: Value) -> OrchestrationThreadDetailSnapshot {
    serde_json::from_value(json!({
        "snapshotSequence": sequence,
        "thread": thread_json(messages),
        "page": {"beforeCursor": "c1", "hasMore": true, "snapshotSequence": sequence},
    }))
    .unwrap()
}

fn state_at(sequence: u64) -> ThreadState {
    let mut state = ThreadState::new(ThreadId::from("t1"));
    state.apply_snapshot(snapshot(sequence, json!([])));
    state
}

fn event(sequence: u64, event_type: &str, payload: Value) -> OrchestrationEvent {
    serde_json::from_value(json!({
        "sequence": sequence, "eventId": format!("e{sequence}"), "aggregateKind": "thread",
        "aggregateId": "t1", "occurredAt": T, "commandId": null, "causationEventId": null,
        "correlationId": null, "metadata": {}, "type": event_type, "payload": payload,
    }))
    .unwrap()
}

fn message(id: &str, role: &str, text: &str, turn: Option<&str>, streaming: bool) -> Value {
    json!({
        "threadId": "t1", "messageId": id, "role": role, "text": text, "turnId": turn,
        "streaming": streaming, "createdAt": T, "updatedAt": T,
    })
}

fn session(status: &str, active_turn: Option<&str>) -> Value {
    json!({"threadId": "t1", "session": {
        "threadId": "t1", "status": status, "providerName": "codex", "runtimeMode": "full-access",
        "activeTurnId": active_turn, "lastError": null, "updatedAt": "2026-10-01T00:00:09.000Z",
    }})
}

fn activity(id: &str, kind: &str, turn: &str, sequence: Option<i64>, payload: Value) -> Value {
    json!({"threadId": "t1", "activity": {
        "id": id, "tone": "info", "kind": kind, "summary": kind, "payload": payload,
        "turnId": turn, "sequence": sequence, "createdAt": T,
    }})
}

fn diff(turn: &str, count: u32, status: &str) -> Value {
    json!({
        "threadId": "t1", "turnId": turn, "checkpointTurnCount": count, "checkpointRef": "r",
        "status": status, "files": [], "assistantMessageId": null, "completedAt": T,
    })
}

fn text_of(state: &ThreadState, id: &str) -> String {
    let thread = state.thread.as_ref().unwrap();
    thread
        .messages
        .iter()
        .find(|m| m.id.as_str() == id)
        .map(|m| m.text.clone())
        .unwrap()
}

#[test]
fn streaming_appends_and_completion_keeps_or_replaces_text() {
    let mut state = state_at(1);
    state.apply_event(event(2, "thread.session-set", session("running", Some("turn-1"))));
    let latest = |s: &ThreadState| s.thread.as_ref().unwrap().latest_turn.clone().unwrap();
    assert_eq!(latest(&state).state, TurnState::Running);

    for (sequence, delta) in [(3, "Hel"), (4, "lo"), (5, " world")] {
        state.apply_event(event(
            sequence,
            "thread.message-sent",
            message("assistant:1", "assistant", delta, Some("turn-1"), true),
        ));
    }
    assert_eq!(text_of(&state, "assistant:1"), "Hello world");

    // A re-delivered delta (resume overlap) is ignored.
    assert!(!state.apply_event(event(
        4,
        "thread.message-sent",
        message("assistant:1", "assistant", "lo", Some("turn-1"), true)
    )));
    assert_eq!(text_of(&state, "assistant:1"), "Hello world");

    // Completion with "" keeps the text; the session still runs the turn, so it stays running.
    state.apply_event(event(
        6,
        "thread.message-sent",
        message("assistant:1", "assistant", "", Some("turn-1"), false),
    ));
    assert_eq!(text_of(&state, "assistant:1"), "Hello world");
    assert_eq!(latest(&state).state, TurnState::Running);
    assert!(!state.thread.as_ref().unwrap().messages[0].streaming);

    // Completion with text replaces it.
    state.apply_event(event(
        7,
        "thread.message-sent",
        message("assistant:1", "assistant", "Hello, world.", Some("turn-1"), false),
    ));
    assert_eq!(text_of(&state, "assistant:1"), "Hello, world.");

    // Leaving `running` settles the turn.
    state.apply_event(event(8, "thread.session-set", session("ready", None)));
    let turn = latest(&state);
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(turn.completed_at.as_deref(), Some("2026-10-01T00:00:09.000Z"));
    assert_eq!(state.last_sequence, 8);
}

#[test]
fn session_errors_and_interrupts_settle_a_running_turn() {
    for (status, expected) in [
        ("error", TurnState::Error),
        ("interrupted", TurnState::Interrupted),
        ("stopped", TurnState::Interrupted),
    ] {
        let mut state = state_at(1);
        state.apply_event(event(2, "thread.session-set", session("running", Some("turn-1"))));
        state.apply_event(event(3, "thread.session-set", session(status, None)));
        let thread = state.thread.as_ref().unwrap();
        assert_eq!(thread.latest_turn.as_ref().unwrap().state, expected, "{status}");
        assert_eq!(
            thread.session.as_ref().unwrap().status,
            SessionStatus::from(status)
        );
    }
}

#[test]
fn delta_clones_only_the_streaming_message() {
    let mut state = ThreadState::new(ThreadId::from("t1"));
    state.apply_snapshot(snapshot(
        1,
        json!([
            {"id": "u1", "role": "user", "text": "hi", "turnId": null, "streaming": false, "createdAt": T, "updatedAt": T},
            {"id": "a1", "role": "assistant", "text": "x", "turnId": "turn-1", "streaming": true, "createdAt": T, "updatedAt": T},
        ]),
    ));
    let published = state.clone();
    state.apply_event(event(
        2,
        "thread.message-sent",
        message("a1", "assistant", "y", Some("turn-1"), true),
    ));
    let before = &published.thread.as_ref().unwrap().messages;
    let after = &state.thread.as_ref().unwrap().messages;
    assert!(Arc::ptr_eq(&before[0], &after[0]), "unchanged message was cloned");
    assert!(!Arc::ptr_eq(&before[1], &after[1]));
    assert_eq!(before[1].text, "x", "published state was mutated");
    assert_eq!(after[1].text, "xy");
}

#[test]
fn missing_checkpoint_never_overwrites_a_real_one() {
    let mut state = state_at(1);
    state.apply_event(event(2, "thread.turn-diff-completed", diff("turn-2", 2, "ready")));
    state.apply_event(event(3, "thread.turn-diff-completed", diff("turn-1", 1, "ready")));
    assert!(!state.apply_event(event(
        4,
        "thread.turn-diff-completed",
        diff("turn-2", 2, "missing")
    )));
    let checkpoints = &state.thread.as_ref().unwrap().checkpoints;
    let counts: Vec<_> = checkpoints.iter().map(|c| c.checkpoint_turn_count).collect();
    assert_eq!(counts, vec![1, 2]);
    assert!(checkpoints.iter().all(|c| c.status == CheckpointStatus::Ready));
    // Unchanged events still advance the cursor.
    assert_eq!(state.last_sequence, 4);
}

#[test]
fn activities_upsert_by_id_and_context_window_supersedes() {
    let mut state = state_at(1);
    state.apply_event(event(
        2,
        "thread.activity-appended",
        activity("task-progress:t1:a", "task.progress", "turn-1", Some(1), json!({"step": 1})),
    ));
    state.apply_event(event(
        3,
        "thread.activity-appended",
        activity("cw-1", "context-window.updated", "turn-1", Some(2), json!({"usedTokens": 10})),
    ));
    state.apply_event(event(
        4,
        "thread.activity-appended",
        activity("task-progress:t1:a", "task.progress", "turn-1", Some(3), json!({"step": 2})),
    ));
    state.apply_event(event(
        5,
        "thread.activity-appended",
        activity("cw-2", "context-window.updated", "turn-1", Some(4), json!({"usedTokens": 20})),
    ));
    // An unresolvable usage row does not replace a resolvable one.
    state.apply_event(event(
        6,
        "thread.activity-appended",
        activity("cw-3", "context-window.updated", "turn-1", Some(5), json!({})),
    ));
    let activities = &state.thread.as_ref().unwrap().activities;
    let ids: Vec<_> = activities.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["task-progress:t1:a", "cw-2", "cw-3"]);
    assert_eq!(activities[0].payload["step"], 2);
}

#[test]
fn activities_without_sequence_sort_last_and_out_of_order_rows_are_placed() {
    let mut state = state_at(1);
    state.apply_event(event(2, "thread.activity-appended", activity("b", "tool.completed", "turn-1", Some(5), json!({}))));
    state.apply_event(event(3, "thread.activity-appended", activity("n", "checkpoint.captured", "turn-1", None, json!({}))));
    state.apply_event(event(4, "thread.activity-appended", activity("a", "tool.completed", "turn-1", Some(2), json!({}))));
    let ids: Vec<_> = state.thread.as_ref().unwrap().activities.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b", "n"]);
}

#[test]
fn revert_drops_rows_of_later_turns() {
    let mut state = state_at(1);
    let events = [
        ("thread.message-sent", message("u1", "user", "one", None, false)),
        ("thread.message-sent", message("a1", "assistant", "r1", Some("turn-1"), false)),
        ("thread.turn-diff-completed", diff("turn-1", 1, "ready")),
        ("thread.message-sent", message("sys", "system", "note", Some("turn-2"), false)),
        ("thread.message-sent", message("u2", "user", "two", None, false)),
        ("thread.message-sent", message("a2", "assistant", "r2", Some("turn-2"), false)),
        ("thread.turn-diff-completed", diff("turn-2", 2, "ready")),
        ("thread.activity-appended", activity("act-2", "tool.completed", "turn-2", Some(1), json!({}))),
    ];
    for (offset, (event_type, payload)) in events.into_iter().enumerate() {
        state.apply_event(event(2 + offset as u64, event_type, payload));
    }
    let epoch = state.history_epoch;
    state.apply_event(event(20, "thread.reverted", json!({"threadId": "t1", "turnCount": 1})));
    let thread = state.thread.as_ref().unwrap();
    let ids: Vec<_> = thread.messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "a1", "sys"]);
    assert!(thread.activities.is_empty());
    assert_eq!(thread.checkpoints.len(), 1);
    assert_eq!(thread.latest_turn.as_ref().unwrap().turn_id.as_str(), "turn-1");
    assert_eq!(state.history_epoch, epoch + 1);
}

#[test]
fn deletion_clears_the_thread() {
    let mut state = state_at(1);
    state.apply_event(event(2, "thread.deleted", json!({"threadId": "t1", "deletedAt": T})));
    assert!(state.deleted);
    assert!(state.thread.is_none());
    assert!(state.page.is_none());
}

#[test]
fn older_pages_merge_once_and_never_after_history_changed() {
    let mut state = ThreadState::new(ThreadId::from("t1"));
    state.apply_snapshot(snapshot(
        5,
        json!([{"id": "u2", "role": "user", "text": "two", "turnId": null, "streaming": false, "createdAt": T, "updatedAt": T}]),
    ));
    let epoch = state.history_epoch;
    let older = serde_json::from_value::<OrchestrationThreadDetailSnapshot>(json!({
        "snapshotSequence": 5,
        "thread": thread_json(json!([
            {"id": "u1", "role": "user", "text": "one", "turnId": null, "streaming": false, "createdAt": T, "updatedAt": T},
            {"id": "u2", "role": "user", "text": "two", "turnId": null, "streaming": false, "createdAt": T, "updatedAt": T},
        ])),
        "page": {"beforeCursor": null, "hasMore": false, "snapshotSequence": 5},
    }))
    .unwrap();

    let mut stale = state.clone();
    stale.apply_snapshot(snapshot(6, json!([])));
    assert!(!stale.merge_older_page(epoch, older.clone()));

    assert!(state.merge_older_page(epoch, older));
    let ids: Vec<_> = state.thread.as_ref().unwrap().messages.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["u1", "u2"]);
    assert!(!state.page.as_ref().unwrap().has_more);
}

#[test]
fn shell_upserts_in_place_and_keeps_unchanged_rows_shared() {
    let project = |id: &str, title: &str| {
        json!({"id": id, "title": title, "workspaceRoot": "/r", "defaultModelSelection": null,
               "scripts": [], "createdAt": T, "updatedAt": T})
    };
    let mut state = ShellState::default();
    state.apply(serde_json::from_value::<ShellStreamItem>(json!({
        "kind": "snapshot",
        "snapshot": {"snapshotSequence": 10, "projects": [project("p1", "A"), project("p2", "B")],
                     "threads": [], "updatedAt": T},
    })).unwrap());
    state.begin_sync(true);
    let before = state.clone();

    let upsert = |sequence: u64, title: &str| {
        serde_json::from_value::<ShellStreamItem>(json!({
            "kind": "project-upserted", "sequence": sequence, "project": project("p2", title),
        }))
        .unwrap()
    };
    assert!(!state.apply(upsert(9, "stale")), "an event below the cursor applied");
    assert!(state.apply(upsert(11, "B2")));
    assert_eq!(state.projects.len(), 2);
    assert_eq!(state.projects[1].title, "B2");
    assert!(Arc::ptr_eq(&before.projects[0], &state.projects[0]));

    assert!(state.apply(serde_json::from_value::<ShellStreamItem>(json!({
        "kind": "thread-removed", "sequence": 12, "threadId": "nope",
    })).unwrap()));
    assert_eq!(state.snapshot_sequence, 12);
    assert!(state.apply(ShellStreamItem::Synchronized));
    assert_eq!(state.status, SyncStatus::Live);
    assert_eq!(state.resume_cursor(), Some(12));
}

#[test]
fn thread_snapshot_item_replaces_merged_history() {
    let mut state = state_at(3);
    state.apply_event(event(4, "thread.message-sent", message("u1", "user", "x", None, false)));
    let epoch = state.history_epoch;
    let item: ThreadStreamItem = serde_json::from_value(json!({
        "kind": "snapshot",
        "snapshot": {"snapshotSequence": 9, "thread": thread_json(json!([]))},
    }))
    .unwrap();
    assert!(state.apply(item));
    assert_eq!(state.last_sequence, 9);
    assert!(state.thread.as_ref().unwrap().messages.is_empty());
    assert_eq!(state.history_epoch, epoch + 1);
    assert!(state.page.is_none());
}
