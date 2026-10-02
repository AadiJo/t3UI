//! Effect RPC wire frames, JSON serialization.
//!
//! Mirrors `effect/unstable/rpc/RpcMessage.ts` (`FromClientEncoded` / `FromServerEncoded`).
//! Each WebSocket text frame carries one JSON value: an object is one message, an array is a
//! batch. The client sends request ids as decimal strings of a `u64` counter (works against
//! both effect versions, protocol.md 1.4) and accepts string or numeric ids back.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

/// A message the client sends to the server.
///
/// There is deliberately no `Eof` variant: sending it wedges the connection (protocol.md 1.8).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_tag")]
pub enum ClientFrame {
    Request {
        id: String,
        tag: String,
        payload: Box<RawValue>,
        headers: Vec<(String, String)>,
    },
    /// Acknowledges one stream chunk so the server may send the next one. Must reuse the exact
    /// id string of the request.
    Ack {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    /// Cancels an in-flight request or stream subscription.
    Interrupt {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    Ping,
}

/// A message the server sends to the client.
#[derive(Debug)]
pub enum ServerFrame {
    /// One or more stream values for a streaming request. Ack once per chunk.
    Chunk {
        request_id: String,
        values: Vec<Box<RawValue>>,
    },
    /// Terminal result for a request (unary success/failure, or end of stream).
    Exit { request_id: String, exit: ExitEncoded },
    /// Connection-level failure not tied to a request. Every in-flight request is lost.
    Defect { defect: serde_json::Value },
    Pong,
    ClientProtocolError { error: serde_json::Value },
    /// A frame with a `_tag` this client does not know. Ignored by callers.
    Unknown { tag: String },
}

/// Encoded `Exit<A, E>`.
#[derive(Debug)]
pub enum ExitEncoded {
    Success { value: Box<RawValue> },
    Failure { cause: Vec<CauseReason> },
}

/// One reason inside an encoded `Cause`.
#[derive(Debug)]
pub enum CauseReason {
    /// An expected, typed failure. `error` decodes into the method's error schema.
    Fail { error: Box<RawValue> },
    /// An unexpected defect (thrown error, bug, payload schema mismatch).
    Die { defect: serde_json::Value },
    Interrupt,
}

// serde cannot buffer `RawValue` inside internally tagged enums, so frames decode through
// flat structs keyed by `_tag` and are converted afterwards.
#[derive(Deserialize)]
struct WireServerFrame {
    #[serde(rename = "_tag")]
    tag: String,
    #[serde(rename = "requestId", default, deserialize_with = "request_id")]
    request_id: Option<String>,
    values: Option<Vec<Box<RawValue>>>,
    exit: Option<WireExit>,
    defect: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WireExit {
    #[serde(rename = "_tag")]
    tag: String,
    value: Option<Box<RawValue>>,
    cause: Option<Vec<WireCause>>,
}

#[derive(Deserialize)]
struct WireCause {
    #[serde(rename = "_tag")]
    tag: String,
    error: Option<Box<RawValue>>,
    defect: Option<serde_json::Value>,
}

/// Request ids come back as the JSON type they were sent with (rc.115) or always as strings
/// (beta.78). Normalize both to the decimal string form.
fn request_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        String(String),
        Number(u64),
    }
    Ok(Option::<Id>::deserialize(deserializer)?.map(|id| match id {
        Id::String(id) => id,
        Id::Number(id) => id.to_string(),
    }))
}

/// Why a server frame could not be decoded.
#[derive(Debug)]
pub struct FrameError(pub String);

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed rpc frame: {}", self.0)
    }
}

impl std::error::Error for FrameError {}

fn null_raw() -> Box<RawValue> {
    RawValue::from_string("null".into()).expect("null is valid json")
}

impl ServerFrame {
    /// Decodes one WebSocket text frame, which may hold one message or a batch array.
    pub fn decode_all(text: &str) -> Result<Vec<Self>, FrameError> {
        if text.trim_start().starts_with('[') {
            let messages: Vec<Box<RawValue>> =
                serde_json::from_str(text).map_err(|e| FrameError(e.to_string()))?;
            messages.iter().map(|m| Self::decode(m.get())).collect()
        } else {
            Self::decode(text).map(|frame| vec![frame])
        }
    }

    /// Decodes one message object.
    pub fn decode(text: &str) -> Result<Self, FrameError> {
        let wire: WireServerFrame =
            serde_json::from_str(text).map_err(|e| FrameError(e.to_string()))?;
        let missing = |field: &str| FrameError(format!("{} frame without `{field}`", wire.tag));
        Ok(match wire.tag.as_str() {
            "Chunk" => ServerFrame::Chunk {
                request_id: wire
                    .request_id
                    .clone()
                    .ok_or_else(|| missing("requestId"))?,
                values: wire.values.ok_or_else(|| missing("values"))?,
            },
            "Exit" => {
                let request_id = wire
                    .request_id
                    .clone()
                    .ok_or_else(|| missing("requestId"))?;
                let exit = wire.exit.ok_or_else(|| missing("exit"))?;
                let exit = match exit.tag.as_str() {
                    "Success" => ExitEncoded::Success {
                        value: exit.value.unwrap_or_else(null_raw),
                    },
                    _ => ExitEncoded::Failure {
                        cause: exit
                            .cause
                            .unwrap_or_default()
                            .into_iter()
                            .map(|c| match c.tag.as_str() {
                                "Fail" => CauseReason::Fail {
                                    error: c.error.unwrap_or_else(null_raw),
                                },
                                "Die" => CauseReason::Die {
                                    defect: c.defect.unwrap_or_default(),
                                },
                                _ => CauseReason::Interrupt,
                            })
                            .collect(),
                    },
                };
                ServerFrame::Exit { request_id, exit }
            }
            "Defect" => ServerFrame::Defect {
                defect: wire.defect.unwrap_or_default(),
            },
            "Pong" => ServerFrame::Pong,
            "ClientProtocolError" => ServerFrame::ClientProtocolError {
                error: wire.error.unwrap_or_default(),
            },
            _ => ServerFrame::Unknown { tag: wire.tag },
        })
    }
}

/// A human-readable message for an encoded defect (`Schema.Defect()`): the `message` of an
/// encoded `Error`, a plain string as is, anything else as JSON.
pub fn defect_message(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        _ => value
            .get("message")
            .and_then(|m| m.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string()),
    }
}
