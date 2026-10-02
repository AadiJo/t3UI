//! Replays a transcript recorded from a real `t3@nightly` server (`examples/probe.rs
//! --record`) through the decoders and reducers.
//!
//! `fixtures/turn-error-session.jsonl`: pair, connect, create a project and a thread, start a
//! turn that fails (Codex unauthenticated), then open fresh shell and thread subscriptions so
//! the server sends full snapshots. The live subscriptions started from the recorded HTTP
//! snapshots, so "HTTP snapshot + live items" must equal the server's later snapshot.
//!
//! Failure modes:
//! 1. A real frame or stream item fails to decode, or decodes as `Unknown` although its kind is
//!    known (silent data loss on every update of that kind).
//! 2. The shell reducer diverges from the server (wrong upsert/remove, cursor handling).
//! 3. The thread reducer diverges from the server: user message, session transitions, the
//!    latest turn, or activities differ from the server's own snapshot.
//! 4. Re-delivered items (resume overlap) are applied twice.
//! 5. The completion marker does not move the status to live.

mod support;

use std::collections::HashMap;

use serde_json::Value;
use support::{Entry, Transcript, assert_matches_server};
use t3_client::{ShellState, SyncStatus};
use t3_protocol::{
    orchestration::{
        EventBody, OrchestrationShellSnapshot, SessionStatus, ShellStreamItem, ThreadStreamItem,
        TurnState,
    },
    rpc::ServerFrame,
    server::ServerConfigStreamEvent,
};

const FIXTURE: &str = include_str!("fixtures/turn-error-session.jsonl");

impl Transcript {
    fn load() -> Self {
        Transcript::parse(FIXTURE)
    }
}

#[test]
fn every_recorded_frame_and_item_decodes_as_a_known_kind() {
    let transcript = Transcript::load();
    let frames = transcript.received_frames();
    assert!(!frames.is_empty());
    assert!(
        !frames
            .iter()
            .any(|f| matches!(f, ServerFrame::Unknown { .. }))
    );

    let config_id = transcript.request_id("subscribeServerConfig", |_| true);
    let config: Vec<ServerConfigStreamEvent> = transcript.items(&config_id);
    let Some(ServerConfigStreamEvent::Snapshot { config }) = config.first() else {
        panic!("first config item is not a snapshot");
    };
    assert_eq!(config.providers.len(), 6, "a provider was dropped");
    assert!(!config.keybindings.is_empty());
    assert!(config.shell_resume_completion_marker && config.thread_resume_completion_marker);
    assert_eq!(config.environment.protocol_version(), 1);

    for (id, _) in transcript.requests("orchestration.subscribeShell") {
        for item in transcript.items::<ShellStreamItem>(&id) {
            assert!(!matches!(item, ShellStreamItem::Unknown { .. }), "{item:?}");
        }
    }
    let mut event_types = Vec::new();
    for (id, _) in transcript.requests("orchestration.subscribeThread") {
        for item in transcript.items::<ThreadStreamItem>(&id) {
            match item {
                ThreadStreamItem::Unknown { kind } => panic!("unknown thread item {kind}"),
                ThreadStreamItem::Event(event) => {
                    assert!(
                        !matches!(event.body, EventBody::Unknown { .. }),
                        "{event:?}"
                    );
                    event_types.push(event.body.event_type().to_owned());
                }
                _ => {}
            }
        }
    }
    for expected in [
        "thread.message-sent",
        "thread.session-set",
        "thread.activity-appended",
    ] {
        assert!(event_types.iter().any(|t| t == expected), "no {expected}");
    }
}

#[test]
fn shell_replay_matches_the_servers_later_snapshot() {
    let transcript = Transcript::load();
    let live_id = transcript.request_id("orchestration.subscribeShell", |p| {
        p.get("afterSequence").is_some()
    });
    let fresh_id = transcript.request_id("orchestration.subscribeShell", |p| {
        p.as_object().unwrap().is_empty()
    });

    let mut state = ShellState::default();
    state.apply_snapshot(transcript.http::<OrchestrationShellSnapshot>("/api/orchestration/shell"));
    state.begin_sync(true);
    assert_eq!(state.status, SyncStatus::Synchronizing);
    let live: Vec<ShellStreamItem> = transcript.items(&live_id);
    for item in live.clone() {
        state.apply(item);
    }
    assert_eq!(state.status, SyncStatus::Live);

    let Some(ShellStreamItem::Snapshot(server)) = transcript.items(&fresh_id).into_iter().next()
    else {
        panic!("fresh subscription did not start with a snapshot");
    };
    assert_eq!(state.snapshot_sequence, server.snapshot_sequence);
    assert_eq!(state.projects, server.projects);

    // The server builds live shell items by re-reading the current row (`ws.ts:841-973`), so
    // the live upsert labeled 21 already carries event 22 (the final `session-set`), while the
    // fresh snapshot was cut at 21. Rows match except for what event 22 changed, and the row's
    // session matches the thread snapshot taken at 22.
    let without_session =
        |rows: &[std::sync::Arc<t3_protocol::orchestration::OrchestrationThreadShell>]| {
            rows.iter()
                .map(|row| {
                    let mut row = (**row).clone();
                    row.session = None;
                    row.updated_at.clear();
                    row
                })
                .collect::<Vec<_>>()
        };
    assert_eq!(
        without_session(&state.threads),
        without_session(&server.threads)
    );
    let thread_fresh_id = transcript.request_id("orchestration.subscribeThread", |p| {
        p.get("afterSequence").is_none()
    });
    let Some(ThreadStreamItem::Snapshot(detail)) =
        transcript.items(&thread_fresh_id).into_iter().next()
    else {
        panic!("fresh thread subscription did not start with a snapshot");
    };
    assert_eq!(detail.snapshot_sequence, 22);
    assert_eq!(state.threads[0].session, detail.thread.session);

    // Re-delivery (resume overlap) changes nothing.
    let before = state.clone();
    for item in live {
        assert!(
            matches!(item, ShellStreamItem::Synchronized) || !state.apply(item),
            "a re-delivered item changed the state"
        );
    }
    assert_eq!(state, before);
}

#[test]
fn thread_replay_matches_the_servers_later_snapshot() {
    let transcript = Transcript::load();
    let [thread_id] = transcript.followed_threads().try_into().unwrap();
    let (state, server) = transcript.replay_thread(&thread_id, |_| {});
    assert_eq!(state.status, SyncStatus::Live);
    assert_matches_server(&state, &server);
    let ours = state.thread.as_ref().unwrap();
    assert_eq!(
        ours.latest_turn.as_ref().map(|t| &t.state),
        Some(&TurnState::Error)
    );
    assert_eq!(
        ours.session.as_ref().map(|s| &s.status),
        Some(&SessionStatus::Error)
    );
    assert_eq!(ours.activities.len(), 10);

    // Re-delivery (resume overlap) changes nothing.
    let live_id = transcript.request_id("orchestration.subscribeThread", |p| {
        p.get("afterSequence").is_some()
    });
    let mut again = state.clone();
    for item in transcript.items::<ThreadStreamItem>(&live_id) {
        assert!(
            matches!(item, ThreadStreamItem::Synchronized) || !again.apply(item),
            "a re-delivered item changed the state"
        );
    }
    assert_eq!(again, state);
}

#[test]
fn request_ids_are_unique_decimal_strings() {
    let transcript = Transcript::load();
    let mut seen = HashMap::new();
    for entry in &transcript.entries {
        if let Entry::Sent(frame) = entry
            && frame["_tag"] == "Request"
        {
            let id = frame["id"].as_str().expect("ids are sent as strings");
            assert!(id.parse::<u64>().is_ok());
            assert!(seen.insert(id.to_owned(), ()).is_none(), "id {id} reused");
            assert_eq!(frame["headers"], Value::Array(vec![]));
        }
    }
}
