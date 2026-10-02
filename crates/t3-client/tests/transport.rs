//! The Effect RPC transport (`rpc.rs`) against a scripted WebSocket peer.
//!
//! Failure modes:
//! 1. After a connection-level `Defect`, in-flight requests are failed but the socket stays
//!    open: streams that survived on the server stay allocated and un-acked while followers
//!    resubscribe on top, so the connection must close (and the supervisor reconnect).
//! 2. A peer that stops reading fills the TCP send buffer; an unbounded write then blocks the
//!    socket task forever, freezing keepalive, interrupts, and close. Every write and the close
//!    handshake need a deadline that ends the session.
//! 3. Acking a chunk on receipt (before the consumer pulls it) tells the server to keep
//!    sending, so a slow or stalled consumer buffers without bound. The Ack must wait until the
//!    consumer takes the chunk (`next` or `try_next`), so at most one chunk per stream is
//!    queued and the server's own backpressure applies.

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::ws::FakeServer;
use t3_client::{CloseReason, RpcConnection, RpcError};
use t3_protocol::{ServerError, methods::Empty};

t3_protocol::stream!(TestStream, "test.stream", Empty => Value, ServerError);
t3_protocol::unary!(TestUnary, "test.unary", Empty => Value, ServerError);
t3_protocol::unary!(TestBig, "test.big", Value => Value, ServerError);

async fn connect(server: &FakeServer) -> (RpcConnection, support::ws::FakePeer) {
    let (conn, peer) = tokio::join!(RpcConnection::connect(server.url.clone()), server.accept());
    (conn.unwrap(), peer)
}

#[tokio::test(flavor = "multi_thread")]
async fn defect_fails_in_flight_requests_and_closes_the_connection() {
    let server = FakeServer::bind().await;
    let (conn, mut peer) = connect(&server).await;

    let mut stream = conn.subscribe::<TestStream>(&Empty {});
    let stream_id = peer.expect_request("test.stream").await;
    peer.send(json!({"_tag": "Chunk", "requestId": stream_id, "values": [1]}))
        .await;
    assert_eq!(stream.next().await.unwrap().unwrap(), json!(1));

    let unary = {
        let conn = conn.clone();
        tokio::spawn(async move { conn.request::<TestUnary>(&Empty {}).await })
    };
    peer.expect_request("test.unary").await;
    peer.send(json!({"_tag": "Defect", "defect": {"name": "Error", "message": "boom"}}))
        .await;

    assert!(
        matches!(unary.await.unwrap(), Err(RpcError::Defect(ref m)) if m == "boom"),
        "unary request was not failed with the defect"
    );
    assert!(matches!(
        stream.next().await,
        Some(Err(RpcError::Defect(_)))
    ));

    let reason = tokio::time::timeout(Duration::from_secs(3), conn.closed())
        .await
        .expect("the connection stayed open after a defect");
    assert!(reason.to_string().contains("boom"), "{reason}");
    assert!(
        peer.closed_within(Duration::from_secs(3)).await,
        "the client did not close the socket"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_peer_that_stops_reading_cannot_freeze_the_connection() {
    let server = FakeServer::bind().await;
    // The peer completes the handshake and then never reads again.
    let (conn, _silent_peer) = connect(&server).await;

    // 64 MiB in 4 MiB frames: far more than the loopback socket buffers hold, so a write
    // blocks, while each frame stays under tungstenite's size limits.
    let blocked: Vec<_> = (0..16)
        .map(|_| {
            let conn = conn.clone();
            let chunk = Value::String("x".repeat(4 * 1024 * 1024));
            tokio::spawn(async move { conn.request::<TestBig>(&chunk).await })
        })
        .collect();
    tokio::time::sleep(Duration::from_secs(2)).await;
    conn.close();

    let reason = tokio::time::timeout(Duration::from_secs(20), conn.closed())
        .await
        .expect("a blocked write froze the connection");
    assert!(
        matches!(reason, CloseReason::WriteTimeout | CloseReason::Closed),
        "{reason}"
    );
    for request in blocked {
        assert!(matches!(
            request.await.unwrap(),
            Err(RpcError::Disconnected(_))
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn chunks_are_acked_only_when_the_consumer_pulls_them() {
    let server = FakeServer::bind().await;
    let (conn, mut peer) = connect(&server).await;
    let mut stream = conn.subscribe::<TestStream>(&Empty {});
    let id = peer.expect_request("test.stream").await;

    peer.send(json!({"_tag": "Chunk", "requestId": id, "values": [1, 2]}))
        .await;
    assert!(
        peer.recv_within(Duration::from_millis(500)).await.is_err(),
        "acked a chunk the consumer has not pulled"
    );

    // Pulling the first item takes the whole chunk and acks it once.
    assert_eq!(stream.next().await.unwrap().unwrap(), json!(1));
    let ack = peer
        .recv_within(Duration::from_secs(2))
        .await
        .expect("no ack after the consumer pulled the chunk")
        .unwrap();
    assert_eq!(ack, json!({"_tag": "Ack", "requestId": id}));
    assert_eq!(stream.next().await.unwrap().unwrap(), json!(2));
    assert!(peer.recv_within(Duration::from_millis(300)).await.is_err());

    // `try_next` acks too.
    peer.send(json!({"_tag": "Chunk", "requestId": id, "values": [3]}))
        .await;
    let item = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(item) = stream.try_next() {
                return item;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(item.unwrap().unwrap(), json!(3));
    let ack = peer
        .recv_within(Duration::from_secs(2))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ack["_tag"], "Ack");

    // Dropping the subscription interrupts it on the server.
    drop(stream);
    let interrupt = peer
        .recv_within(Duration::from_secs(2))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(interrupt, json!({"_tag": "Interrupt", "requestId": id}));
}
