//! Decodes real responses from `fixtures/features.jsonl` (recorded by
//! `examples/record_features.rs` against the e2e harness) into each method's typed result.
//!
//! Failure modes:
//! 1. A recorded response (success, stream item, or typed failure) does not decode into the
//!    method's declared type: the UI would show an error for a healthy server.
//! 2. A method was recorded but has no typed descriptor here, so its shape is unchecked.
//! 3. Typed failures lose the fields the UI words its message from (`reason`, `provider`).
//! 4. Usage limits (rate-limit windows) are dropped or the provider row is lost.

mod support;

use std::collections::BTreeMap;

use serde_json::Value;
use support::{Entry, Transcript};
use t3_protocol::{ServerError, Stream, Unary, methods::*, server::ServerConfigStreamEvent};

const FIXTURE: &str = include_str!("fixtures/features.jsonl");

/// What a recorded request produced.
#[derive(Default)]
struct Responses {
    successes: Vec<Value>,
    failures: Vec<Value>,
    items: Vec<Value>,
}

fn responses(transcript: &Transcript) -> BTreeMap<String, Responses> {
    let mut tags = BTreeMap::new();
    let mut out: BTreeMap<String, Responses> = BTreeMap::new();
    for entry in &transcript.entries {
        match entry {
            Entry::Sent(frame) if frame["_tag"] == "Request" => {
                tags.insert(
                    frame["id"].as_str().unwrap().to_owned(),
                    frame["tag"].as_str().unwrap().to_owned(),
                );
            }
            Entry::Received(frame) => {
                let Some(tag) = frame["requestId"].as_str().and_then(|id| tags.get(id)) else {
                    continue;
                };
                let slot = out.entry(tag.clone()).or_default();
                match frame["_tag"].as_str() {
                    Some("Chunk") => slot
                        .items
                        .extend(frame["values"].as_array().unwrap().iter().cloned()),
                    Some("Exit") if frame["exit"]["_tag"] == "Success" => {
                        slot.successes.push(frame["exit"]["value"].clone())
                    }
                    Some("Exit") => {
                        for cause in frame["exit"]["cause"].as_array().unwrap() {
                            if cause["_tag"] == "Fail" {
                                slot.failures.push(cause["error"].clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

fn decode<T: serde::de::DeserializeOwned>(tag: &str, what: &str, value: &Value) {
    if let Err(error) = serde_json::from_value::<T>(value.clone()) {
        panic!("{tag} {what} did not decode: {error}\n{value}");
    }
}

fn check_unary<M: Unary<Error = ServerError>>(responses: &Responses) {
    for value in &responses.successes {
        decode::<M::Success>(M::TAG, "success", value);
    }
    for value in &responses.failures {
        decode::<ServerError>(M::TAG, "failure", value);
    }
}

fn check_stream<M: Stream<Error = ServerError>>(responses: &Responses) {
    for value in &responses.items {
        decode::<M::Item>(M::TAG, "item", value);
    }
    for value in &responses.failures {
        decode::<ServerError>(M::TAG, "failure", value);
    }
}

/// Every recorded method, by type. A recorded tag missing here fails the test.
macro_rules! registry {
    (unary: [$($u:ty),* $(,)?], stream: [$($s:ty),* $(,)?]) => {
        fn check(tag: &str, responses: &Responses) -> bool {
            $( if tag == <$u as Unary>::TAG { check_unary::<$u>(responses); return true; } )*
            $( if tag == <$s as Stream>::TAG { check_stream::<$s>(responses); return true; } )*
            false
        }
    };
}

registry! {
    unary: [
        PullRequestsList, PullRequestsListStats, PullRequestsSummary, PullRequestsDetail,
        PullRequestsStack, PullRequestsLinkedThreads, PullRequestsRouting,
        PullRequestsRoutingIdentity, PullRequestsInvalidate, ServerGetUsageSummary,
        ServerRefreshUsageRates,
    ],
    stream: [SubscribeServerConfig, SubscribeShell, PullRequestsSubscribeRefreshes]
}

#[test]
fn every_recorded_response_decodes_into_its_method_type() {
    let all = responses(&Transcript::parse(FIXTURE));
    assert!(all.len() >= 13, "only {} methods recorded", all.len());
    for (tag, responses) in &all {
        assert!(
            check(tag, responses),
            "{tag} was recorded but has no typed descriptor"
        );
    }
}

#[test]
fn pull_request_errors_keep_their_reason() {
    let all = responses(&Transcript::parse(FIXTURE));
    let failure: ServerError =
        serde_json::from_value(all["pullRequests.summary"].failures[0].clone()).unwrap();
    assert_eq!(failure.tag, ServerError::PULL_REQUEST_UNAVAILABLE);
    assert_eq!(failure.reason(), Some("provider-unsupported"));
}

#[test]
fn provider_rate_limits_decode_from_the_config_snapshot() {
    let all = responses(&Transcript::parse(FIXTURE));
    let first = all["subscribeServerConfig"].items[0].clone();
    let ServerConfigStreamEvent::Snapshot { config } = serde_json::from_value(first).unwrap()
    else {
        panic!("config stream did not start with a snapshot");
    };
    let codex = config
        .providers
        .iter()
        .find(|p| p.instance_id.as_str() == "codex")
        .expect("codex provider dropped");
    let limits = codex
        .usage_limits
        .as_ref()
        .expect("codex usage limits dropped");
    assert!(limits.windows.len() >= 2, "{limits:?}");
    assert!(
        limits
            .windows
            .iter()
            .all(|w| (0.0..=100.0).contains(&w.used_percent))
    );
}
