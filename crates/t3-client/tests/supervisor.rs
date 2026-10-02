//! The connection supervisor (`environment.rs`) against a scripted WebSocket peer.
//!
//! Failure modes:
//! 1. A credential that opens the socket but lacks a scope fails the first config
//!    subscription with `EnvironmentAuthorizationError`; treating that as transient reconnects
//!    on the 3/4/8/16 s ladder forever instead of showing a blocked permission failure.

mod support;

use std::{sync::Arc, time::Duration};

use serde_json::json;
use support::ws::FakeServer;
use t3_client::{
    ClientInfo, Endpoint, Environment, EnvironmentHttp, EnvironmentOptions,
    auth::{BoxFuture, PreparedConnection},
    connection::{BlockedReason, ConnectionFailure, FailureKind},
};
use t3_protocol::{EnvironmentId, environment::ExecutionEnvironmentDescriptor};
use url::Url;

/// Points every attempt at the fake server; no HTTP auth involved.
struct FakeEndpoint {
    ws_url: Url,
}

impl Endpoint for FakeEndpoint {
    fn prepare(
        &self,
        expected: EnvironmentId,
        _client: ClientInfo,
    ) -> BoxFuture<Result<PreparedConnection, ConnectionFailure>> {
        let descriptor: ExecutionEnvironmentDescriptor = serde_json::from_value(json!({
            "environmentId": expected, "label": "fake", "serverVersion": "0.0.0",
            "platform": {"os": "linux", "arch": "x64"}, "capabilities": {},
        }))
        .unwrap();
        let mut http = self.ws_url.clone();
        http.set_scheme("http").unwrap();
        http.set_path("/");
        let prepared = PreparedConnection {
            descriptor,
            http: EnvironmentHttp::new(http, None),
            ws_url: self.ws_url.clone(),
        };
        Box::pin(async move { Ok(prepared) })
    }

    fn display_url(&self) -> Option<Url> {
        None
    }
}

fn start(server: &FakeServer) -> Environment {
    Environment::start(EnvironmentOptions::new(
        EnvironmentId::from("env-fake"),
        "fake",
        Arc::new(FakeEndpoint {
            ws_url: server.url.parse().unwrap(),
        }),
    ))
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_scope_on_the_config_stream_blocks_instead_of_retrying() {
    let server = FakeServer::bind().await;
    let env = start(&server);
    let mut peer = server.accept().await;
    let id = peer.expect_request("subscribeServerConfig").await;
    peer.send(json!({
        "_tag": "Exit", "requestId": id,
        "exit": {"_tag": "Failure", "cause": [{"_tag": "Fail", "error": {
            "_tag": "EnvironmentAuthorizationError",
            "message": "Missing scope orchestration:read",
            "requiredScope": "orchestration:read",
        }}]},
    }))
    .await;

    let failure = tokio::time::timeout(Duration::from_secs(6), env.wait_connected())
        .await
        .expect("the supervisor kept retrying a permission failure")
        .expect_err("connected without a config");
    assert_eq!(
        failure.kind,
        FailureKind::Blocked(BlockedReason::Permission)
    );
    assert!(
        server.accept_within(Duration::from_secs(5)).await.is_none(),
        "a blocked environment reconnected on its own"
    );
}
