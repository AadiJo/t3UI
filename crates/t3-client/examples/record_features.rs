//! Records one call of each feature RPC (pull requests, usage, ...) against the e2e harness so
//! `tests/features.rs` can decode real responses into their typed results. Read-only calls only:
//! nothing here mutates server state beyond caches.
//!
//! ```sh
//! e2e/run-local.sh up --server nightly --detach
//! cargo run -p t3-client --example record_features -- \
//!     [--state /tmp/t3ui-e2e/run-nightly/state.json] [--out crates/t3-client/tests/fixtures/features.jsonl]
//! ```

mod common;

use std::{path::PathBuf, time::Duration};

use anyhow::{Context as _, Result, bail};
use t3_client::{Environment, RpcError};
use t3_protocol::{
    ServerError, Stream, Unary,
    methods::*,
    preview::{DiscoveredLocalServersInput, PreviewListInput},
    providers::ProviderSetupInput,
    pull_requests::{
        ListState, PullRequestInvalidateInput, PullRequestListInput, PullRequestListStatsInput,
        PullRequestRef, PullRequestRoutingIdentityInput,
    },
    server::SubscribeServerConfigInput,
    server_ops::ResourceHistoryInput,
    usage::UsageSummaryInput,
    workspace::WorktreeSetupThreadInput,
};

fn main() -> Result<()> {
    common::init_tracing("warn");
    let mut state_path = PathBuf::from("/tmp/t3ui-e2e/run-nightly/state.json");
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/features.jsonl");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_path = args.next().context("--state needs a path")?.into(),
            "--out" => out = args.next().context("--out needs a path")?.into(),
            other => bail!("unexpected argument {other}"),
        }
    }
    let harness = common::HarnessState::load(&state_path)?;
    let recorder = common::Recorder::install();
    let label = t3_client::runtime::runtime().block_on(run(&harness))?;
    recorder
        .lock()
        .unwrap()
        .write_jsonl(&out, &harness.redactor(&label))?;
    keep_only_harness_servers(&out, &harness.base_url)?;
    println!("recorded {}", out.display());
    Ok(())
}

/// `subscribeDiscoveredLocalServers` reports every listening port on the machine (other
/// people's dev servers, daily-driver apps). Keep only the harness's own server.
fn keep_only_harness_servers(path: &std::path::Path, base_url: &str) -> Result<()> {
    let port = url::Url::parse(base_url)?.port().unwrap_or(0);
    let text = std::fs::read_to_string(path)?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut entry: serde_json::Value = serde_json::from_str(line)?;
        if let Some(values) = entry["frame"]["values"].as_array_mut() {
            for value in values {
                if let Some(servers) = value.get_mut("servers").and_then(|s| s.as_array_mut()) {
                    servers.retain(|server| server["port"].as_u64() == Some(u64::from(port)));
                }
            }
        }
        lines.push(entry.to_string());
    }
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Calls a unary method and prints a one-line outcome. Failures are recorded too.
async fn call<M: Unary<Error = ServerError>>(env: &Environment, payload: &M::Payload)
where
    M::Success: std::fmt::Debug,
{
    let outcome =
        match tokio::time::timeout(Duration::from_secs(60), env.request::<M>(payload)).await {
            Ok(Ok(value)) => format!("ok {}", truncate(&format!("{value:?}"))),
            Ok(Err(RpcError::Failed(error))) => format!("failed {error}"),
            Ok(Err(error)) => format!("error {error}"),
            Err(_) => "timed out".into(),
        };
    println!("{:<40} {outcome}", M::TAG);
}

/// Takes the first item of a stream (or notes that none came) and cancels it.
async fn first<M: Stream<Error = ServerError>>(env: &Environment, payload: &M::Payload)
where
    M::Item: std::fmt::Debug,
{
    let outcome = match env.subscribe::<M>(payload) {
        Ok(mut stream) => match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
            Ok(Some(Ok(item))) => format!("item {}", truncate(&format!("{item:?}"))),
            Ok(Some(Err(error))) => format!("error {error}"),
            Ok(None) => "ended".into(),
            Err(_) => "no item within 5s".into(),
        },
        Err(error) => format!("error {error}"),
    };
    println!("{:<40} {outcome}", M::TAG);
}

fn truncate(text: &str) -> String {
    text.chars().take(100).collect()
}

async fn run(harness: &common::HarnessState) -> Result<String> {
    let env = harness.connect()?;
    env.wait_connected().await?;
    let label = env
        .config()
        .borrow()
        .as_ref()
        .map(|c| c.environment.label.clone())
        .unwrap_or_default();
    let project = &harness.projects[0];

    // Config stream with every opt-in event type.
    first::<SubscribeServerConfig>(
        &env,
        &SubscribeServerConfigInput {
            environment_themes: Some(true),
            usage_limit_sources: Some(true),
            usage_limits_command: Some(true),
        },
    )
    .await;

    // Pull requests: the seeded repos have no remote, so these exercise the empty and
    // unavailable shapes.
    call::<PullRequestsList>(&env, &PullRequestListInput::new(ListState::Open)).await;
    call::<PullRequestsListStats>(&env, &PullRequestListStatsInput { refs: Vec::new() }).await;
    let reference = PullRequestRef::new(project.id.clone(), "fixture/aurora-web", 1);
    call::<PullRequestsSummary>(&env, &reference).await;
    call::<PullRequestsDetail>(&env, &reference).await;
    call::<PullRequestsStack>(&env, &reference).await;
    call::<PullRequestsLinkedThreads>(&env, &reference).await;
    call::<PullRequestsRouting>(&env, &reference).await;
    call::<PullRequestsRoutingIdentity>(
        &env,
        &PullRequestRoutingIdentityInput {
            host: "github.com".into(),
        },
    )
    .await;
    call::<PullRequestsInvalidate>(&env, &PullRequestInvalidateInput::default()).await;
    first::<PullRequestsSubscribeRefreshes>(&env, &Empty {}).await;

    // Usage.
    let today = chrono::Utc::now().date_naive();
    call::<ServerGetUsageSummary>(
        &env,
        &UsageSummaryInput {
            since_day: (today - chrono::Duration::days(7)).to_string(),
            until_day: today.to_string(),
            time_zone: "UTC".into(),
            resolution: None,
            since_time: None,
            until_time: None,
        },
    )
    .await;
    call::<ServerRefreshUsageRates>(&env, &Empty {}).await;

    // Server operations (read-only).
    call::<ServerDiscoverSourceControl>(&env, &Empty {}).await;
    call::<ServerGetTraceDiagnostics>(&env, &Empty {}).await;
    call::<ServerGetProcessDiagnostics>(&env, &Empty {}).await;
    call::<ServerGetHostResources>(&env, &Empty {}).await;
    let window = ResourceHistoryInput {
        window_ms: 60_000,
        bucket_ms: 10_000,
    };
    call::<ServerGetProcessResourceHistory>(&env, &window).await;
    call::<ServerGetResourceTelemetryHistory>(&env, &window).await;
    first::<SubscribeResourceTelemetry>(&env, &Empty {}).await;
    call::<CloudGetRelayClientStatus>(&env, &Empty {}).await;

    // Providers (read-only subscriptions).
    let codex = ProviderSetupInput {
        instance_id: "codex".into(),
    };
    first::<ProviderAuthSubscribe>(&env, &codex).await;
    first::<ProviderInstallSubscribe>(&env, &codex).await;

    // Workspaces.
    call::<AgentSessionsScan>(&env, &Empty {}).await;
    first::<SubscribeProjectClones>(&env, &Empty {}).await;
    if let Some(thread) = harness.threads.iter().find(|t| !t.archived) {
        first::<SubscribeWorktreeSetup>(
            &env,
            &WorktreeSetupThreadInput {
                thread_id: thread.id.clone(),
            },
        )
        .await;
        call::<PreviewList>(
            &env,
            &PreviewListInput {
                thread_id: thread.id.clone(),
            },
        )
        .await;
    }
    first::<SubscribePreviewEvents>(&env, &Empty {}).await;
    first::<SubscribeDiscoveredLocalServers>(&env, &DiscoveredLocalServersInput::default()).await;
    first::<SubscribeDeviceState>(&env, &Empty {}).await;

    drop(env);
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(label)
}
