//! Replays `fixtures/stream-scenarios.jsonl`, recorded by `examples/record_scenarios.rs` from
//! the e2e harness (fake Codex, server in `token` streaming mode): four new threads running the
//! `showcase`, `approval`, `question`, and `plan` scenarios while the client was subscribed,
//! each followed by a fresh subscription whose snapshot is the server's own view.
//!
//! Failure modes:
//! 1. Token-level assistant deltas do not reassemble into the server's final text, or a
//!    message mid-stream is not a prefix of its final text (append/replace/completion bugs).
//! 2. Reasoning messages are lost or merged into assistant messages.
//! 3. Work-log activities, checkpoints, plans, the session, or the latest turn differ from the
//!    server's snapshot after the turn.
//! 4. An approval does not show as pending while open, or stays pending after it is resolved.
//! 5. A question does not show as pending with its options, or stays pending once answered.
//! 6. `settingsUpdated` config events are not applied (the recording switches the streaming
//!    mode to `token` and back).
//! 7. A recorded item decodes as `Unknown` although its kind is known.

mod support;

use std::collections::HashMap;

use support::{Transcript, assert_matches_server};
use t3_client::{SyncStatus, ThreadState};
use t3_protocol::{
    ThreadId,
    orchestration::{
        EventBody, InteractionMode, MessageRole, ShellStreamItem, ThreadStreamItem, TurnState,
    },
    server::{ResponseStreamingMode, ServerConfigStreamEvent},
};

const FIXTURE: &str = include_str!("fixtures/stream-scenarios.jsonl");

fn transcript() -> Transcript {
    Transcript::parse(FIXTURE)
}

fn rec(scenario: &str) -> ThreadId {
    ThreadId::from(format!("thread-rec-{scenario}"))
}

/// Replays a scenario thread, checking after every item that each message's text only grows
/// by appending until it completes (the final text may replace it on completion).
fn replay(
    transcript: &Transcript,
    scenario: &str,
    mut on_step: impl FnMut(&ThreadState),
) -> ThreadState {
    let mut seen: HashMap<String, Vec<String>> = HashMap::new();
    let (state, server) = transcript.replay_thread(&rec(scenario), |state| {
        for message in &state.thread.as_ref().unwrap().messages {
            seen.entry(message.id.to_string())
                .or_default()
                .push(message.text.clone());
        }
        on_step(state);
    });
    assert_eq!(state.status, SyncStatus::Live);
    assert_matches_server(&state, &server);
    for message in &state.thread.as_ref().unwrap().messages {
        let final_text = &message.text;
        for text in &seen[message.id.as_str()] {
            assert!(
                final_text.starts_with(text.as_str()),
                "{scenario}: {} streamed {text:?}, which is not a prefix of its final text",
                message.id
            );
        }
    }
    assert_eq!(
        state
            .thread
            .as_ref()
            .unwrap()
            .latest_turn
            .as_ref()
            .map(|t| &t.state),
        Some(&TurnState::Completed),
        "{scenario}: turn did not complete"
    );
    state
}

#[test]
fn every_recorded_item_decodes_as_a_known_kind() {
    let transcript = transcript();
    for (id, _) in transcript.requests("orchestration.subscribeThread") {
        for item in transcript.items::<ThreadStreamItem>(&id) {
            match item {
                ThreadStreamItem::Unknown { kind } => panic!("unknown thread item {kind}"),
                ThreadStreamItem::Event(event) => {
                    assert!(
                        !matches!(event.body, EventBody::Unknown { .. }),
                        "{event:?}"
                    )
                }
                _ => {}
            }
        }
    }
    for (id, _) in transcript.requests("orchestration.subscribeShell") {
        for item in transcript.items::<ShellStreamItem>(&id) {
            assert!(!matches!(item, ShellStreamItem::Unknown { .. }), "{item:?}");
        }
    }
    for (id, _) in transcript.requests("subscribeServerConfig") {
        for item in transcript.items::<ServerConfigStreamEvent>(&id) {
            assert_ne!(item, ServerConfigStreamEvent::Unknown);
        }
    }
    let followed = transcript.followed_threads();
    for scenario in ["showcase", "approval", "question", "plan"] {
        assert!(
            followed.contains(&rec(scenario)),
            "{scenario} was not followed live"
        );
    }
}

#[test]
fn showcase_token_deltas_reassemble_into_the_final_answer() {
    let transcript = transcript();
    let mut streaming_steps = 0;
    let state = replay(&transcript, "showcase", |state| {
        let thread = state.thread.as_ref().unwrap();
        if thread
            .messages
            .iter()
            .any(|m| m.role == MessageRole::Assistant && m.streaming)
        {
            streaming_steps += 1;
        }
    });
    assert!(
        streaming_steps > 20,
        "only {streaming_steps} streaming steps"
    );
    let thread = state.thread.as_ref().unwrap();
    assert!(thread.messages.iter().all(|m| !m.streaming));
    assert!(
        thread
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Reasoning)
            .count()
            >= 2
    );
    let answer = thread
        .messages
        .iter()
        .rfind(|m| m.role == MessageRole::Assistant)
        .unwrap();
    assert!(answer.text.len() > 1000 && answer.text.contains("```"));
    let kinds: Vec<_> = thread.activities.iter().map(|a| a.kind.as_str()).collect();
    for kind in [
        "tool.started",
        "tool.completed",
        "turn.plan.updated",
        "context-window.updated",
    ] {
        assert!(kinds.contains(&kind), "no {kind} activity");
    }
    assert_eq!(thread.checkpoints.len(), 1);
}

#[test]
fn approval_is_pending_until_resolved() {
    let transcript = transcript();
    let mut pending_seen = Vec::new();
    let state = replay(&transcript, "approval", |state| {
        let pending = state.pending_requests();
        if let [approval] = pending.approvals.as_slice() {
            pending_seen.push((approval.request_kind.clone(), approval.detail.clone()));
        }
    });
    let (kind, detail) = pending_seen
        .first()
        .expect("the approval was never pending");
    assert_eq!(kind.as_str(), "command");
    assert!(detail.as_deref().is_some_and(|d| !d.is_empty()));
    assert!(state.pending_requests().is_empty());
    let kinds: Vec<_> = state
        .thread
        .as_ref()
        .unwrap()
        .activities
        .iter()
        .map(|a| a.kind.as_str())
        .collect();
    assert!(kinds.contains(&"approval.requested") && kinds.contains(&"approval.resolved"));
}

#[test]
fn question_is_pending_with_options_until_answered() {
    let transcript = transcript();
    let mut questions_seen = Vec::new();
    let state = replay(&transcript, "question", |state| {
        if let [input] = state.pending_requests().user_inputs.as_slice() {
            questions_seen.push(input.questions.clone());
        }
    });
    let questions = questions_seen
        .first()
        .expect("the question was never pending");
    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].id, "database");
    let labels: Vec<_> = questions[0]
        .options
        .iter()
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(labels, ["Postgres", "SQLite"]);
    assert!(state.pending_requests().is_empty());
    let last = state.thread.as_ref().unwrap().messages.last().unwrap();
    assert!(last.text.contains("SQLite"), "{}", last.text);
}

#[test]
fn plan_scenario_ends_with_a_proposed_plan() {
    let transcript = transcript();
    let state = replay(&transcript, "plan", |_| {});
    let thread = state.thread.as_ref().unwrap();
    assert_eq!(thread.interaction_mode, Some(InteractionMode::Plan));
    let [plan] = thread.proposed_plans.as_slice() else {
        panic!("expected one plan, got {}", thread.proposed_plans.len());
    };
    assert!(plan.plan_markdown.len() > 100);
    assert!(plan.implemented_at.is_none());
}

#[test]
fn config_stream_applies_settings_updates() {
    let transcript = transcript();
    let id = transcript.request_id("subscribeServerConfig", |_| true);
    let mut items = transcript.items::<ServerConfigStreamEvent>(&id).into_iter();
    let Some(ServerConfigStreamEvent::Snapshot { config }) = items.next() else {
        panic!("config stream did not start with a snapshot");
    };
    let mut config = *config;
    let initial = config.settings.response_streaming_mode.clone();
    let mut modes = Vec::new();
    for item in items {
        if config.apply(item) {
            modes.push(config.settings.response_streaming_mode.clone());
        }
    }
    assert!(
        modes.contains(&Some(ResponseStreamingMode::Token)),
        "{modes:?}"
    );
    assert_eq!(
        config.settings.response_streaming_mode, initial,
        "mode was not restored"
    );
}
