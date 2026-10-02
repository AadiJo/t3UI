//! The Effect RPC transport (`rpc.rs`) against a scripted WebSocket peer.
//!
//! Failure modes:
//! 1. After a connection-level `Defect`, in-flight requests are failed but the socket stays
//!    open: streams that survived on the server stay allocated and un-acked while followers
//!    resubscribe on top, so the connection must close (and the supervisor reconnect).

mod support;

use std::time::Duration;

use serde_json::{Value, json};
use support::ws::FakeServer;
use t3_client::{RpcConnection, RpcError};
use t3_protocol::{ServerError, methods::Empty};

t3_protocol::stream!(TestStream, "test.stream", Empty => Value, ServerError);
t3_protocol::unary!(TestUnary, "test.unary", Empty => Value, ServerError);

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
