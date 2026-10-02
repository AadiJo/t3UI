//! Loading recorded transcripts (`examples/probe.rs --record`,
//! `examples/record_scenarios.rs`) and replaying threads through the reducer.
#![allow(dead_code)] // Each test file uses a different subset.

pub mod ws;

use serde::de::DeserializeOwned;
use serde_json::Value;
use t3_client::ThreadState;
use t3_protocol::{
    ThreadId,
    orchestration::{OrchestrationThreadDetailSnapshot, ThreadStreamItem},
    rpc::ServerFrame,
};

/// One recorded line.
pub enum Entry {
    Sent(Value),
    Received(Value),
    Http { path: String, body: Value },
}

pub struct Transcript {
    pub entries: Vec<Entry>,
}

impl Transcript {
    pub fn parse(jsonl: &str) -> Self {
        let entries = jsonl
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
    pub fn requests(&self, tag: &str) -> Vec<(String, Value)> {
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
    pub fn request_id(&self, tag: &str, pick: impl Fn(&Value) -> bool) -> String {
        self.requests(tag)
            .into_iter()
            .find(|(_, payload)| pick(payload))
            .unwrap_or_else(|| panic!("no {tag} request matched"))
            .0
    }

    /// Every stream item delivered for `request_id`, decoded as `T`, in order.
    pub fn items<T: DeserializeOwned>(&self, request_id: &str) -> Vec<T> {
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
    pub fn received_frames(&self) -> Vec<ServerFrame> {
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

    /// The first HTTP response whose path starts with `path_prefix`.
    pub fn http<T: DeserializeOwned>(&self, path_prefix: &str) -> T {
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

    /// Threads followed live: an HTTP snapshot, then a subscription resuming from it.
    pub fn followed_threads(&self) -> Vec<ThreadId> {
        self.requests("orchestration.subscribeThread")
            .into_iter()
            .filter(|(_, payload)| payload.get("afterSequence").is_some())
            .map(|(_, payload)| ThreadId::from(payload["threadId"].as_str().unwrap()))
            .collect()
    }

    /// Replays one followed thread: its HTTP snapshot, then every item of its live
    /// subscription, calling `on_step` after each item. Returns the final state and the
    /// snapshot from the later fresh subscription (no cursor) for the same thread.
    pub fn replay_thread(
        &self,
        thread_id: &ThreadId,
        mut on_step: impl FnMut(&ThreadState),
    ) -> (ThreadState, OrchestrationThreadDetailSnapshot) {
        let is_thread = |p: &Value| p["threadId"] == thread_id.as_str();
        let live = self.request_id("orchestration.subscribeThread", |p| {
            is_thread(p) && p.get("afterSequence").is_some()
        });
        let fresh = self.request_id("orchestration.subscribeThread", |p| {
            is_thread(p) && p.get("afterSequence").is_none()
        });
        let initial: OrchestrationThreadDetailSnapshot =
            self.http(&format!("/api/orchestration/threads/{thread_id}"));
        let mut state = ThreadState::new(thread_id.clone());
        state.apply_snapshot(initial);
        state.begin_sync(true);
        for item in self.items::<ThreadStreamItem>(&live) {
            state.apply(item);
            on_step(&state);
        }
        let Some(ThreadStreamItem::Snapshot(server)) =
            self.items::<ThreadStreamItem>(&fresh).into_iter().next()
        else {
            panic!("fresh subscription for {thread_id} did not start with a snapshot");
        };
        (state, *server)
    }
}

/// Asserts the replayed thread matches the server's snapshot taken after it. Activity order is
/// the reducer's (provider sequence, time, id) while the snapshot uses DB order, so activities
/// compare as sets.
pub fn assert_matches_server(ours: &ThreadState, server: &OrchestrationThreadDetailSnapshot) {
    let mine = ours.thread.as_ref().expect("replayed thread has data");
    let theirs = &server.thread;
    let id = &theirs.id;
    assert_eq!(ours.last_sequence, server.snapshot_sequence, "{id}: cursor");
    assert_eq!(mine.messages, theirs.messages, "{id}: messages");
    assert_eq!(mine.session, theirs.session, "{id}: session");
    assert_eq!(mine.checkpoints, theirs.checkpoints, "{id}: checkpoints");
    assert_eq!(mine.proposed_plans, theirs.proposed_plans, "{id}: plans");
    assert_eq!(mine.title, theirs.title, "{id}: title");
    assert_eq!(mine.model_selection, theirs.model_selection, "{id}: model");
    let turn = |t: &t3_protocol::orchestration::OrchestrationThread| {
        t.latest_turn
            .as_ref()
            .map(|turn| (turn.turn_id.clone(), turn.state.clone()))
    };
    assert_eq!(turn(mine), turn(theirs), "{id}: latest turn");
    let sorted = |t: &t3_protocol::orchestration::OrchestrationThread| {
        let mut rows: Vec<_> = t.activities.iter().map(|a| (**a).clone()).collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    };
    assert_eq!(sorted(mine), sorted(theirs), "{id}: activities");
}
