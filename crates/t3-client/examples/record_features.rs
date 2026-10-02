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
    pull_requests::{
        ListState, PullRequestInvalidateInput, PullRequestListInput, PullRequestListStatsInput,
        PullRequestRef, PullRequestRoutingIdentityInput,
    },
    server::SubscribeServerConfigInput,
    usage::UsageSummaryInput,
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
    println!("recorded {}", out.display());
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

    drop(env);
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(label)
}
