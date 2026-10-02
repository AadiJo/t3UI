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

use std::collections::HashMap;

use serde_json::Value;
use t3_client::{ShellState, SyncStatus, ThreadState};
use t3_protocol::{
    ThreadId,
    orchestration::{
        EventBody, OrchestrationShellSnapshot, OrchestrationThreadDetailSnapshot, SessionStatus,
        ShellStreamItem, ThreadStreamItem, TurnState,
    },
    rpc::ServerFrame,
    server::ServerConfigStreamEvent,
};

const FIXTURE: &str = include_str!("fixtures/turn-error-session.jsonl");

/// One recorded line.
enum Entry {
    Sent(Value),
    Received(Value),
    Http { path: String, body: Value },
}

struct Transcript {
    entries: Vec<Entry>,
}

impl Transcript {
    fn load() -> Self {
        let entries = FIXTURE
            .lines()
            .map(|line| {
                let mut value: Value = serde_json::from_str(line).unwrap();
                match value["dir"].as_str().unwrap() {
                    "sent" => Entry::Sent(value["frame"].take()),
                    "received" => Entry::Received(value["frame"].take()),
                    "http" => Entry::Http {
                        path: value["path"].as_str().unwrap().to_owned(),
                        body: value["body"].take(),
                    },
                    other => panic!("unknown direction {other}"),
                }
            })
            .collect();
        Transcript { entries }
    }

    /// Request ids of the requests for `tag`, in order, with their payloads.
    fn requests(&self, tag: &str) -> Vec<(String, Value)> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Sent(frame) if frame["_tag"] == "Request" && frame["tag"] == tag => {
                    Some((frame["id"].as_str()?.to_owned(), frame["payload"].clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// The request id of the `tag` request whose payload satisfies `pick`.
    fn request_id(&self, tag: &str, pick: impl Fn(&Value) -> bool) -> String {
        self.requests(tag)
            .into_iter()
            .find(|(_, payload)| pick(payload))
            .unwrap_or_else(|| panic!("no {tag} request matched"))
            .0
    }

    /// Every stream item delivered for `request_id`, decoded as `T`, in order.
    fn items<T: serde::de::DeserializeOwned>(&self, request_id: &str) -> Vec<T> {
        self.received_frames()
            .into_iter()
            .filter_map(|frame| match frame {
                ServerFrame::Chunk {
                    request_id: id,
                    values,
                } if id == request_id => Some(values),
                _ => None,
            })
            .flatten()
            .map(|raw| {
                serde_json::from_str(raw.get())
                    .unwrap_or_else(|e| panic!("item failed to decode: {e}\n{}", raw.get()))
            })
            .collect()
    }

    /// Received frames decoded from their exact text (the wire codec, not `serde_json::Value`).
    fn received_frames(&self) -> Vec<ServerFrame> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Received(frame) => Some(
                    ServerFrame::decode(&frame.to_string())
                        .unwrap_or_else(|e| panic!("frame failed to decode: {e}\n{frame}")),
                ),
                _ => None,
            })
            .collect()
    }

    fn http<T: serde::de::DeserializeOwned>(&self, path_prefix: &str) -> T {
        self.entries
            .iter()
            .find_map(|entry| match entry {
                Entry::Http { path, body } if path.starts_with(path_prefix) => {
                    Some(serde_json::from_value(body.clone()).unwrap())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no http response for {path_prefix}"))
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
    let live_id = transcript.request_id("orchestration.subscribeThread", |p| {
        p.get("afterSequence").is_some()
    });
    let fresh_id = transcript.request_id("orchestration.subscribeThread", |p| {
        p.get("afterSequence").is_none()
    });

    let initial: OrchestrationThreadDetailSnapshot = transcript.http("/api/orchestration/threads/");
    let thread_id: ThreadId = initial.thread.id.clone();
    let mut state = ThreadState::new(thread_id);
    state.apply_snapshot(initial);
    state.begin_sync(true);
    let live: Vec<ThreadStreamItem> = transcript.items(&live_id);
    for item in live.clone() {
        state.apply(item);
    }
    assert_eq!(state.status, SyncStatus::Live);

    let Some(ThreadStreamItem::Snapshot(server)) = transcript.items(&fresh_id).into_iter().next()
    else {
        panic!("fresh subscription did not start with a snapshot");
    };
    let ours = state.thread.as_ref().unwrap();
    let theirs = &server.thread;
    assert_eq!(state.last_sequence, server.snapshot_sequence);
    assert_eq!(ours.messages, theirs.messages);
    assert_eq!(ours.session, theirs.session);
    assert_eq!(ours.checkpoints, theirs.checkpoints);
    assert_eq!(ours.title, theirs.title);
    assert_eq!(ours.model_selection, theirs.model_selection);
    let turn = |t: &t3_protocol::orchestration::OrchestrationThread| {
        t.latest_turn
            .as_ref()
            .map(|turn| (turn.turn_id.clone(), turn.state.clone()))
    };
    assert_eq!(turn(ours), turn(theirs));
    assert_eq!(
        ours.latest_turn.as_ref().map(|t| &t.state),
        Some(&TurnState::Error)
    );
    assert_eq!(
        ours.session.as_ref().map(|s| &s.status),
        Some(&SessionStatus::Error)
    );
    // Activity order is the reducer's (provider sequence, time, id); the snapshot is DB order.
    let ids = |t: &t3_protocol::orchestration::OrchestrationThread| {
        let mut ids: Vec<_> = t.activities.iter().map(|a| a.id.clone()).collect();
        ids.sort();
        ids
    };
    assert_eq!(ids(ours), ids(theirs));
    assert_eq!(ours.activities.len(), 10);

    let before = state.clone();
    for item in live {
        assert!(
            matches!(item, ThreadStreamItem::Synchronized) || !state.apply(item),
            "a re-delivered item changed the state"
        );
    }
    assert_eq!(state, before);
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
