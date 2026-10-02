//! Effect RPC wire frames, JSON serialization.
//!
//! Mirrors `effect/unstable/rpc/RpcMessage.ts` (`FromClientEncoded` / `FromServerEncoded`).
//! Each WebSocket text frame carries one JSON-encoded message. Request ids are decimal
//! strings of a client-side bigint counter.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// A message the client sends to the server.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_tag")]
pub enum ClientFrame {
    Request {
        id: String,
        tag: String,
        payload: Box<RawValue>,
        headers: Vec<(String, String)>,
    },
    /// Acknowledges one stream chunk so the server may send the next one.
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
    Eof,
}

/// A message the server sends to the client.
#[derive(Debug)]
pub enum ServerFrame {
    /// One or more stream values for a streaming request.
    Chunk {
        request_id: String,
        values: Vec<Box<RawValue>>,
    },
    /// Terminal result for a request (unary success/failure, or end of stream).
    Exit {
        request_id: String,
        exit: ExitEncoded,
    },
    Defect {
        defect: serde_json::Value,
    },
    Pong,
    ClientProtocolError {
        error: serde_json::Value,
    },
    /// A frame with a `_tag` this client does not know. Ignored by callers.
    Unknown {
        tag: String,
    },
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
    Fail {
        error: Box<RawValue>,
    },
    /// An unexpected defect (thrown error, bug).
    Die {
        defect: serde_json::Value,
    },
    Interrupt,
}

// serde cannot buffer `RawValue` inside internally tagged enums, so frames decode through
// flat structs keyed by `_tag` and are converted afterwards.
#[derive(Deserialize)]
struct WireServerFrame {
    #[serde(rename = "_tag")]
    tag: String,
    #[serde(rename = "requestId")]
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

/// Why a server frame could not be decoded.
#[derive(Debug)]
pub struct FrameError(pub String);

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed rpc frame: {}", self.0)
    }
}

impl std::error::Error for FrameError {}

impl ServerFrame {
    /// Decodes one WebSocket text frame.
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
                        value: exit.value.unwrap_or_else(|| {
                            RawValue::from_string("null".into()).expect("null is valid json")
                        }),
                    },
                    _ => ExitEncoded::Failure {
                        cause: exit
                            .cause
                            .unwrap_or_default()
                            .into_iter()
                            .map(|c| match c.tag.as_str() {
                                "Fail" => CauseReason::Fail {
                                    error: c.error.unwrap_or_else(|| {
                                        RawValue::from_string("null".into())
                                            .expect("null is valid json")
                                    }),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_frame_matches_effect_encoding() {
        let frame = ClientFrame::Request {
            id: "1".into(),
            tag: "server.getConfig".into(),
            payload: RawValue::from_string("{}".into()).unwrap(),
            headers: vec![],
        };
        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"_tag":"Request","id":"1","tag":"server.getConfig","payload":{},"headers":[]}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientFrame::Ack {
                request_id: "7".into()
            })
            .unwrap(),
            r#"{"_tag":"Ack","requestId":"7"}"#
        );
    }

    #[test]
    fn exit_failure_decodes() {
        let frame = ServerFrame::decode(
            r#"{"_tag":"Exit","requestId":"3","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":{"_tag":"X","message":"m"}}]}}"#,
        )
        .unwrap();
        let ServerFrame::Exit {
            request_id,
            exit: ExitEncoded::Failure { cause },
        } = frame
        else {
            panic!("expected failure exit");
        };
        assert_eq!(request_id, "3");
        assert!(matches!(cause.as_slice(), [CauseReason::Fail { .. }]));
    }
}
