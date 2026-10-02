//! Timeline derivation against threads recorded from the e2e nightly server
//! (`t3-snapshots/fixtures/threads/*.json`, the seed behind `docs/reference/*.png`) and synthetic
//! edge cases ported from `MessagesTimeline.logic.test.ts` / `session-logic.test.ts`.
//!
//! Failure modes:
//! 1. A settled turn is not folded, or the running / streaming turn is folded.
//! 2. The fold label reads the wrong timestamps (latest-turn timing vs. entry timing) or misses
//!    the interrupted wording.
//! 3. The terminal assistant message is hidden in the fold; meta (copy + time) shows on
//!    commentary or while the turn still runs.
//! 4. Mixed work groups render as tool stacks; neutral entries survive in mixed groups; the
//!    "+N previous" count is off.
//! 5. Lifecycle updates of one tool call become several rows, or separate calls merge.
//! 6. A backfilled lifecycle event changes a visible row's identity or position.
//! 7. The changed-files card appears before its turn ends or after the next user message.
//! 8. Revert counts are off by one.
//! 9. Unchanged rows are not shared between derivations, or the diff range is wrong.
//! 10. Reasoning (`system`) messages produce rows.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::{Value, json};
use t3_protocol::{
    TurnId,
    orchestration::{OrchestrationMessage, OrchestrationThread, OrchestrationThreadActivity},
};

use super::*;

/// A recorded `ThreadState` (t3-client) by fixture name; only its `thread` is needed here.
fn fixture_thread(name: &str) -> OrchestrationThread {
    let json = match name {
        "aurora-tour" => include_str!("../../../t3-snapshots/fixtures/threads/aurora-tour.json"),
        "aurora-watch" => include_str!("../../../t3-snapshots/fixtures/threads/aurora-watch.json"),
        "aurora-migrate" => {
            include_str!("../../../t3-snapshots/fixtures/threads/aurora-migrate.json")
        }
        "aurora-plan" => include_str!("../../../t3-snapshots/fixtures/threads/aurora-plan.json"),
        "borealis-persist" => {
            include_str!("../../../t3-snapshots/fixtures/threads/borealis-persist.json")
        }
        "cirrus-deploy" => {
            include_str!("../../../t3-snapshots/fixtures/threads/cirrus-deploy.json")
        }
        _ => panic!("unknown fixture {name}"),
    };
    let state: Value = serde_json::from_str(json).unwrap();
    serde_json::from_value(state["thread"].clone()).unwrap()
}

struct Expanded {
    turns: HashSet<TurnId>,
    groups: HashSet<String>,
}

impl Expanded {
    fn none() -> Self {
        Self {
            turns: HashSet::new(),
            groups: HashSet::new(),
        }
    }
}

fn rows_for(
    thread: &OrchestrationThread,
    expanded: &Expanded,
    is_working: bool,
) -> Vec<Arc<TimelineRow>> {
    let mut model = TimelineModel::default();
    let started =
        active_work_started_at(thread.latest_turn.as_ref(), thread.session.as_ref(), None);
    model.update(TimelineInput {
        thread,
        scope: "env:thread",
        optimistic: &[],
        expanded_turns: &expanded.turns,
        expanded_work_groups: &expanded.groups,
        is_working,
        active_turn_started_at: started.as_deref(),
    });
    model.rows().to_vec()
}

fn is_working(thread: &OrchestrationThread) -> bool {
    matches!(
        SessionPhase::of(thread.session.as_ref()),
        SessionPhase::Running | SessionPhase::Connecting
    )
}

/// A compact description of each row for assertions.
fn describe(rows: &[Arc<TimelineRow>]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let detail = if row.fold_detail.is_some() {
                " (fold)"
            } else {
                ""
            };
            let text = match &row.kind {
                RowKind::Message(m) => format!(
                    "{}{}: {}",
                    m.message.role.as_str(),
                    if m.show_assistant_meta { "+meta" } else { "" },
                    m.message.text.lines().next().unwrap_or("")
                ),
                RowKind::Work { entries } => format!(
                    "work: {}",
                    entries
                        .iter()
                        .map(|e| e.heading())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                RowKind::ToolStack { entries, .. } => format!(
                    "stack: {}",
                    entries
                        .iter()
                        .map(|e| e.heading())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                RowKind::WorkToggle {
                    hidden_count,
                    expanded,
                    ..
                } => format!(
                    "toggle: {hidden_count} {}",
                    if *expanded { "open" } else { "closed" }
                ),
                RowKind::TurnFold { label, .. } => format!("fold: {label}"),
                RowKind::ProposedPlan(_) => "plan".into(),
                RowKind::ChangedFiles(summary) => format!("files: {}", summary.files.len()),
                RowKind::Working { started_at } => {
                    format!("working: {}", started_at.as_deref().unwrap_or("-"))
                }
            };
            format!("{text}{detail}")
        })
        .collect()
}

#[test]
fn showcase_folds_the_settled_turn_and_ends_with_changed_files() {
    let thread = fixture_thread("aurora-tour");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    assert_eq!(
        describe(&rows),
        [
            "user: Give me a tour of this repo, then make formatBytes handle megabytes and gigabytes.",
            "fold: Worked for 1.9s",
            "assistant+meta: ## Repository tour",
            "files: 2",
        ]
    );
}

#[test]
fn expanding_the_fold_reveals_commentary_and_one_work_group() {
    let thread = fixture_thread("aurora-tour");
    let turn = thread.latest_turn.as_ref().unwrap().turn_id.clone();
    let mut expanded = Expanded::none();
    expanded.turns.insert(turn);
    let rows = rows_for(&thread, &expanded, false);
    assert_eq!(
        describe(&rows),
        [
            "user: Give me a tour of this repo, then make formatBytes handle megabytes and gigabytes.",
            "fold: Worked for 1.9s",
            "assistant: I'll start by looking around the repository to see how it is laid out. (fold)",
            "work: Plan updated (fold)",
            "toggle: 10 closed (fold)",
            "assistant+meta: ## Repository tour",
            "files: 2",
        ]
    );

    // Opening the work group shows every entry, oldest first (the work-log reference).
    let RowKind::WorkToggle { group_id, .. } = &rows[4].kind else {
        panic!("expected the work toggle")
    };
    expanded.groups.insert(group_id.clone());
    let rows = rows_for(&thread, &expanded, false);
    let work: Vec<String> = rows
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::Work { entries } => Some(entries[0].clone()),
            _ => None,
        })
        .map(|entry| match &entry.command {
            Some(command) => format!("{} {command}", entry.heading()),
            None => entry.heading(),
        })
        .collect();
    assert_eq!(
        work,
        [
            "Plan updated",
            "Ran command ls -la",
            "Ran command cat src/format.ts",
            "Ran command rg -n formatBytes",
            "Plan updated",
            "Web search",
            "Docs · lookup",
            "Plan updated",
            "File change",
            "Ran command node --test",
            "Plan updated",
        ]
    );
    assert!(matches!(
        rows.iter().rev().nth(2).unwrap().kind,
        RowKind::WorkToggle { expanded: true, .. }
    ));
}

#[test]
fn showcase_work_entries_carry_previews_and_status() {
    let thread = fixture_thread("aurora-tour");
    let entries = derive_work_log_entries(&thread.activities);
    let ls = &entries[1];
    assert_eq!(ls.command.as_deref(), Some("ls -la"));
    assert_eq!(ls.raw_command.as_deref(), Some("/bin/bash -lc 'ls -la'"));
    assert_eq!(ls.tool_call_id.as_deref(), Some("call_88634afb_3"));
    assert!(ls.indicates_success() && ls.is_tool_call());

    let file_change = entries
        .iter()
        .find(|e| e.heading() == "File change")
        .unwrap();
    assert_eq!(
        file_change.changed_files,
        [
            "/tmp/t3ui-e2e/run-nightly/repos/aurora-web/src/format.ts",
            "/tmp/t3ui-e2e/run-nightly/repos/aurora-web/test/format.test.ts"
        ]
    );
    let search = entries
        .iter()
        .find(|e| e.heading() == "Web search")
        .unwrap();
    assert_eq!(search.item_type, Some(ToolItemType::WebSearch));
    assert_eq!(
        search.detail.as_deref(),
        Some("binary vs decimal byte units KiB MiB convention")
    );
    let mcp = entries
        .iter()
        .find(|e| e.heading() == "Docs · lookup")
        .unwrap();
    assert!(mcp.tool_data.is_some());

    let plan = &entries[0];
    assert_eq!(plan.tone, WorkTone::Info);
    assert!(!plan.is_tool_like() && !plan.indicates_neutral());
}

#[test]
fn running_turn_is_not_folded_and_ends_with_the_working_row() {
    let thread = fixture_thread("aurora-watch");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    // The in-progress `pnpm test --watch` only has `tool.started`, which the log skips.
    assert_eq!(
        describe(&rows),
        [
            "user: Watch the test suite and fix failures as they come in.",
            "assistant: Starting the test watcher. I'll fix failures as they show up.",
            "work: Plan updated",
            "working: 2026-10-02T04:30:16.641Z",
        ]
    );
}

#[test]
fn approval_turn_stacks_tool_calls_behind_the_request() {
    let thread = fixture_thread("aurora-migrate");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    assert_eq!(
        describe(&rows)[1..],
        [
            "assistant: I'll check which migrations are pending, then apply them.",
            "stack: Ran command, Command approval requested",
            "working: 2026-10-02T04:30:15.701Z",
        ]
    );
    let RowKind::ToolStack { entries, .. } = &rows[2].kind else {
        unreachable!()
    };
    let request = entries.last().unwrap();
    assert_eq!(request.request_kind, Some(RequestKind::Command));
    assert_eq!(
        request.detail.as_deref(),
        Some("/bin/bash -lc 'pnpm db:migrate'")
    );
    assert!(request.indicates_success());
}

#[test]
fn reasoning_messages_produce_no_rows() {
    let thread = fixture_thread("borealis-persist");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    assert_eq!(
        describe(&rows),
        [
            "user: Set up persistence for the health checks.",
            "work: User input requested",
            "working: 2026-10-02T04:30:13.521Z",
        ]
    );
}

#[test]
fn failed_turn_folds_its_failed_command() {
    let thread = fixture_thread("cirrus-deploy");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    assert_eq!(
        describe(&rows),
        [
            "user: Deploy the docs to production.",
            "fold: Worked for 797ms"
        ]
    );
    let entries = derive_work_log_entries(&thread.activities);
    assert!(entries[0].indicates_failure());
}

#[test]
fn plan_turn_keeps_the_plan_card_outside_the_fold() {
    let thread = fixture_thread("aurora-plan");
    let rows = rows_for(&thread, &Expanded::none(), is_working(&thread));
    assert_eq!(
        describe(&rows),
        [
            "user: Plan the move to a pnpm monorepo with a shared ui package.",
            "fold: Worked for 997ms",
            "assistant+meta: I have enough context. Here is the plan I'd follow:",
            "plan",
        ]
    );
}

// --- Synthetic threads -------------------------------------------------------------------

fn thread(value: Value) -> OrchestrationThread {
    let mut base = json!({
        "id": "t", "projectId": "p", "title": "T",
        "modelSelection": {"instanceId": "codex", "model": "gpt"},
        "runtimeMode": "full-access", "interactionMode": "default",
        "branch": null, "worktreePath": null, "latestTurn": null,
        "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-01T00:00:00.000Z",
        "archivedAt": null, "deletedAt": null, "session": null,
        "messages": [], "activities": [], "proposedPlans": [], "checkpoints": []
    });
    base.as_object_mut()
        .unwrap()
        .extend(value.as_object().unwrap().clone());
    serde_json::from_value(base).unwrap()
}

fn at(seconds: u32) -> String {
    format!("2026-01-01T00:00:{seconds:02}.000Z")
}

fn message(id: &str, role: &str, turn: Option<&str>, seconds: u32, streaming: bool) -> Value {
    json!({"id": id, "role": role, "text": id, "turnId": turn, "streaming": streaming,
           "createdAt": at(seconds), "updatedAt": at(seconds)})
}

fn tool(id: &str, kind: &str, turn: &str, seconds: u32, call: &str, status: &str) -> Value {
    json!({"id": id, "tone": "tool", "kind": kind, "summary": "Ran command", "turnId": turn,
           "createdAt": at(seconds),
           "payload": {"itemType": "command_execution", "status": status, "title": "Ran command",
                       "data": {"toolCallId": call, "item": {"command": format!("cmd-{call}")}}}})
}

fn settled(turn: &str, started: u32, completed: u32, state: &str) -> Value {
    json!({"turnId": turn, "state": state, "requestedAt": at(started), "startedAt": at(started),
           "completedAt": at(completed), "assistantMessageId": null})
}

#[test]
fn interrupted_latest_turn_reads_you_stopped() {
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false), message("a1", "assistant", Some("t1"), 2, false)],
        "activities": [tool("x1", "tool.completed", "t1", 1, "c1", "completed")],
        "latestTurn": settled("t1", 0, 5, "interrupted"),
    }));
    let rows = rows_for(&thread, &Expanded::none(), false);
    assert_eq!(describe(&rows)[1], "fold: You stopped after 5.0s");
}

#[test]
fn running_session_turn_is_not_folded_even_if_latest_turn_lags() {
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false), message("a1", "assistant", Some("t2"), 2, false)],
        "activities": [tool("x1", "tool.completed", "t2", 1, "c1", "completed")],
        "latestTurn": settled("t1", 0, 1, "completed"),
        "session": {"threadId": "t", "status": "running", "activeTurnId": "t2",
                    "updatedAt": at(0), "providerName": null, "providerInstanceId": null,
                    "runtimeMode": null, "lastError": null},
    }));
    let rows = rows_for(&thread, &Expanded::none(), true);
    assert_eq!(
        describe(&rows)[1..3],
        ["stack: Ran command", "assistant: a1"],
        "no fold and no meta while the turn runs"
    );
}

#[test]
fn streaming_turn_is_not_folded() {
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false),
                     message("a1", "assistant", Some("t1"), 1, false),
                     message("a2", "assistant", Some("t1"), 3, true)],
        "latestTurn": settled("t1", 0, 4, "completed"),
    }));
    let rows = rows_for(&thread, &Expanded::none(), false);
    assert!(
        rows.iter()
            .all(|row| !matches!(row.kind, RowKind::TurnFold { .. }))
    );
}

#[test]
fn fold_without_latest_turn_timing_spans_user_message_to_last_entry() {
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false),
                     message("a1", "assistant", Some("t1"), 3, false),
                     message("a2", "assistant", Some("t1"), 7, false),
                     message("u2", "user", None, 20, false)],
        "latestTurn": settled("t9", 20, 21, "completed"),
    }));
    let rows = rows_for(&thread, &Expanded::none(), false);
    assert_eq!(
        describe(&rows)[1..3],
        ["fold: Worked for 7.0s", "assistant+meta: a2"]
    );
}

#[test]
fn changed_files_flush_before_the_next_user_message_and_revert_counts() {
    let files = json!([{"path": "a.ts", "kind": "modified", "additions": 1, "deletions": 0}]);
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false), message("a1", "assistant", Some("t1"), 1, false),
                     message("u2", "user", None, 5, false), message("a2", "assistant", Some("t2"), 6, false)],
        "checkpoints": [
            {"turnId": "t1", "checkpointTurnCount": 1, "checkpointRef": "r1", "status": "ready",
             "files": files, "assistantMessageId": "a1", "completedAt": at(2)},
            {"turnId": "t2", "checkpointTurnCount": 2, "checkpointRef": "r2", "status": "ready",
             "files": [], "assistantMessageId": "a2", "completedAt": at(7)}],
        "latestTurn": settled("t2", 5, 7, "completed"),
    }));
    let rows = rows_for(&thread, &Expanded::none(), false);
    assert_eq!(
        describe(&rows),
        [
            "user: u1",
            "assistant+meta: a1",
            "files: 1",
            "user: u2",
            "assistant+meta: a2"
        ]
    );
    let reverts: Vec<Option<u32>> = rows
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::Message(m) if m.message.role.as_str() == "user" => Some(m.revert_turn_count),
            _ => None,
        })
        .collect();
    assert_eq!(reverts, [Some(0), Some(1)]);
}

#[test]
fn changed_files_render_below_the_working_row_at_the_end() {
    let files = json!([{"path": "a.ts", "kind": "modified", "additions": 1, "deletions": 0}]);
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false), message("a1", "assistant", Some("t1"), 1, false)],
        "checkpoints": [{"turnId": "t1", "checkpointTurnCount": 1, "checkpointRef": "r1",
                         "status": "ready", "files": files, "assistantMessageId": "a1",
                         "completedAt": at(2)}],
        "latestTurn": settled("t1", 0, 2, "completed"),
    }));
    let rows = rows_for(&thread, &Expanded::none(), true);
    assert_eq!(describe(&rows)[2..], ["working: -", "files: 1"]);
}

#[test]
fn mixed_groups_drop_neutral_entries_and_count_hidden_ones() {
    let mut activities = vec![
        json!({"id": "p1", "tone": "info", "kind": "turn.plan.updated", "summary": "Plan updated",
               "turnId": "t1", "createdAt": at(1), "payload": {}}),
        tool("x1", "tool.completed", "t1", 2, "c1", "completed"),
        // In progress: neutral, dropped from a mixed group.
        tool("x2", "tool.updated", "t1", 3, "c2", "inProgress"),
    ];
    activities.push(json!({"id": "p2", "tone": "info", "kind": "turn.plan.updated",
                           "summary": "Plan updated", "turnId": "t1", "createdAt": at(4), "payload": {}}));
    let thread = thread(json!({
        "messages": [message("u1", "user", None, 0, false)],
        "activities": activities,
        "latestTurn": {"turnId": "t1", "state": "running", "requestedAt": at(0), "startedAt": at(0),
                       "completedAt": null, "assistantMessageId": null},
    }));
    let rows = rows_for(&thread, &Expanded::none(), true);
    assert_eq!(
        describe(&rows)[1..3],
        ["work: Plan updated", "toggle: 2 closed"]
    );
}

#[test]
fn lifecycle_updates_collapse_into_one_entry_but_repeat_calls_do_not() {
    let activities: Vec<Arc<OrchestrationThreadActivity>> = [
        tool("x1", "tool.updated", "t1", 1, "c1", "inProgress"),
        tool("x2", "tool.completed", "t1", 2, "c1", "completed"),
        tool("x3", "tool.updated", "t1", 3, "c1", "inProgress"),
    ]
    .into_iter()
    .map(|value| Arc::new(serde_json::from_value(value).unwrap()))
    .collect();
    let entries = derive_work_log_entries(&activities);
    assert_eq!(
        entries.len(),
        2,
        "a completed call never absorbs a later update"
    );
    assert_eq!(entries[0].id, "x1");
    assert_eq!(entries[0].created_at, at(1));
    assert_eq!(entries[0].lifecycle_status, Some(ToolStatus::Completed));
}

#[test]
fn backfilled_lifecycle_events_keep_the_presented_identity() {
    let make = |values: Vec<Value>| -> Vec<Arc<OrchestrationThreadActivity>> {
        values
            .into_iter()
            .map(|value| Arc::new(serde_json::from_value(value).unwrap()))
            .collect()
    };
    let mut presenter = WorkLogPresenter::default();
    let first = presenter.present(
        "scope",
        derive_work_log_entries(&make(vec![tool(
            "x2",
            "tool.completed",
            "t1",
            5,
            "c1",
            "completed",
        )])),
    );
    assert_eq!(first[0].presentation_key(), "tool:c1");
    // A reconnect delivers the earlier update: the row keeps its first timestamp.
    let second = presenter.present(
        "scope",
        derive_work_log_entries(&make(vec![
            tool("x1", "tool.updated", "t1", 1, "c1", "inProgress"),
            tool("x2", "tool.completed", "t1", 5, "c1", "completed"),
        ])),
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].presentation_key(), "tool:c1");
    assert_eq!(second[0].created_at, at(5));
    // A new scope starts over.
    let third = presenter.present(
        "other",
        derive_work_log_entries(&make(vec![
            tool("x1", "tool.updated", "t1", 1, "c1", "inProgress"),
            tool("x2", "tool.completed", "t1", 5, "c1", "completed"),
        ])),
    );
    assert_eq!(third[0].created_at, at(1));
}

#[test]
fn unchanged_rows_are_shared_and_the_diff_covers_only_changes() {
    let base = json!({
        "messages": [message("u1", "user", None, 0, false), message("a1", "assistant", Some("t1"), 1, true)],
        "latestTurn": {"turnId": "t1", "state": "running", "requestedAt": at(0), "startedAt": at(0),
                       "completedAt": null, "assistantMessageId": null},
    });
    let expanded = Expanded::none();
    let mut model = TimelineModel::default();
    let first = thread(base.clone());
    fn input<'a>(thread: &'a OrchestrationThread, expanded: &'a Expanded) -> TimelineInput<'a> {
        TimelineInput {
            thread,
            scope: "s",
            optimistic: &[],
            expanded_turns: &expanded.turns,
            expanded_work_groups: &expanded.groups,
            is_working: true,
            active_turn_started_at: None,
        }
    }
    let diff = model.update(input(&first, &expanded)).unwrap();
    assert_eq!((diff.old, diff.new), (0..0, 0..3));
    let before = model.rows().to_vec();

    // Same content again: nothing changes.
    assert_eq!(model.update(input(&thread(base.clone()), &expanded)), None);

    // A streaming delta replaces only the assistant row, and keeps the keys.
    let mut streamed = thread(base);
    Arc::make_mut(&mut streamed.messages[1])
        .text
        .push_str(" more");
    let diff = model.update(input(&streamed, &expanded)).unwrap();
    assert_eq!(
        (diff.old.clone(), diff.new.clone(), diff.same_keys),
        (1..2, 1..2, true)
    );
    assert!(Arc::ptr_eq(&before[0], &model.rows()[0]));
    assert!(Arc::ptr_eq(&before[2], &model.rows()[2]));
}

#[test]
fn optimistic_messages_show_until_the_server_echoes_them() {
    let echoed: Arc<OrchestrationMessage> =
        Arc::new(serde_json::from_value(message("u1", "user", None, 0, false)).unwrap());
    let pending: Arc<OrchestrationMessage> =
        Arc::new(serde_json::from_value(message("u2", "user", None, 9, false)).unwrap());
    let thread = thread(json!({"messages": [message("u1", "user", None, 0, false)]}));
    let expanded = Expanded::none();
    let mut model = TimelineModel::default();
    model.update(TimelineInput {
        thread: &thread,
        scope: "s",
        optimistic: &[echoed, pending],
        expanded_turns: &expanded.turns,
        expanded_work_groups: &expanded.groups,
        is_working: true,
        active_turn_started_at: Some("2026-01-01T00:00:09.000Z"),
    });
    assert_eq!(
        describe(model.rows()),
        ["user: u1", "user: u2", "working: 2026-01-01T00:00:09.000Z"]
    );
}

#[test]
fn active_work_start_prefers_the_local_send() {
    let running = thread(json!({
        "latestTurn": {"turnId": "t1", "state": "running", "requestedAt": at(1), "startedAt": at(1),
                       "completedAt": null, "assistantMessageId": null},
        "session": {"threadId": "t", "status": "running", "activeTurnId": "t1", "updatedAt": at(1),
                    "providerName": null, "providerInstanceId": null, "runtimeMode": null, "lastError": null},
    }));
    let latest = running.latest_turn.as_ref();
    let session = running.session.as_ref();
    assert_eq!(active_work_started_at(latest, session, None), Some(at(1)));
    assert_eq!(
        active_work_started_at(latest, session, Some(&at(0))),
        Some(at(0))
    );
    let done = thread(json!({"latestTurn": settled("t1", 1, 2, "completed")}));
    assert_eq!(
        active_work_started_at(done.latest_turn.as_ref(), None, None),
        None
    );
    assert!(is_latest_turn_settled(done.latest_turn.as_ref(), None));
}

#[test]
fn tool_heading_and_failure_status() {
    let failed: OrchestrationThreadActivity = serde_json::from_value(json!({
        "id": "x", "tone": "tool", "kind": "tool.completed", "summary": "Read file completed",
        "turnId": "t", "createdAt": at(0),
        "payload": {"detail": "ENOENT: no such file or directory, open 'a.ts'"}
    }))
    .unwrap();
    let entries = derive_work_log_entries(&[Arc::new(failed)]);
    assert_eq!(entries[0].heading(), "Read file");
    assert!(entries[0].indicates_failure());
    assert!(!entries[0].indicates_success());
    let _ = HashMap::<(), ()>::new();
}
