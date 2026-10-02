//! A scriptable WebSocket peer for transport and supervisor tests: the test plays the server
//! one frame at a time, so ordering is explicit.

use std::time::Duration;

use futures::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

pub struct FakeServer {
    pub url: String,
    listener: TcpListener,
}

impl FakeServer {
    pub async fn bind() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        FakeServer { url, listener }
    }

    /// Accepts the next WebSocket connection.
    pub async fn accept(&self) -> FakePeer {
        let (tcp, _) = self.listener.accept().await.unwrap();
        FakePeer {
            ws: accept_async(tcp).await.unwrap(),
        }
    }

    /// Accepts the next connection, or `None` if none arrives within `limit`.
    pub async fn accept_within(&self, limit: Duration) -> Option<FakePeer> {
        tokio::time::timeout(limit, self.accept()).await.ok()
    }
}

pub struct FakePeer {
    ws: WebSocketStream<TcpStream>,
}

impl FakePeer {
    /// The next client message, answering Pings with Pongs. `None` when the client closed.
    pub async fn recv(&mut self) -> Option<Value> {
        loop {
            match self.ws.next().await? {
                Ok(Message::Text(text)) => {
                    let value: Value = serde_json::from_str(text.as_str()).unwrap();
                    if value["_tag"] == "Ping" {
                        self.send(json!({"_tag": "Pong"})).await;
                        continue;
                    }
                    return Some(value);
                }
                Ok(Message::Close(_)) | Err(_) => return None,
                Ok(_) => {}
            }
        }
    }

    /// Like [`recv`](Self::recv), but `Err(())` if nothing arrives within `limit`.
    pub async fn recv_within(&mut self, limit: Duration) -> Result<Option<Value>, ()> {
        tokio::time::timeout(limit, self.recv()).await.map_err(drop)
    }

    /// Waits for a `Request` with `tag` (skipping Acks) and returns its id.
    pub async fn expect_request(&mut self, tag: &str) -> String {
        loop {
            let frame = self.recv().await.expect("client closed before the request");
            if frame["_tag"] == "Ack" {
                continue;
            }
            assert_eq!(frame["_tag"], "Request", "unexpected frame {frame}");
            assert_eq!(frame["tag"], tag, "unexpected request {frame}");
            return frame["id"].as_str().unwrap().to_owned();
        }
    }

    pub async fn send(&mut self, frame: Value) {
        self.ws
            .send(Message::text(frame.to_string()))
            .await
            .unwrap();
    }

    /// Whether the client closed the socket within `limit`.
    pub async fn closed_within(&mut self, limit: Duration) -> bool {
        tokio::time::timeout(limit, async { while self.recv().await.is_some() {} })
            .await
            .is_ok()
    }
}
