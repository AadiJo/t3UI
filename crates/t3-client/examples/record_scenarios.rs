//! Records a streaming golden transcript from the e2e harness (fake Codex): new threads that
//! run the `showcase`, `approval`, `question`, and `plan` scenarios while this client is
//! subscribed, answering the approval and the question along the way.
//!
//! ```sh
//! e2e/run-local.sh up --server nightly --detach
//! cargo run -p t3-client --example record_scenarios -- \
//!     [--state /tmp/t3ui-e2e/run-nightly/state.json] \
//!     [--out crates/t3-client/tests/fixtures/stream-scenarios.jsonl] [--only showcase,plan]
//! ```
//!
//! Per scenario: `thread.create` (fixed id `thread-rec-<scenario>`), open the thread (HTTP
//! snapshot + live subscription), `thread.turn.start`, follow until the turn settles, then a
//! fresh subscription without a cursor so the transcript ends with the server's own snapshot to
//! compare against. Frames and orchestration HTTP responses are recorded and redacted like
//! `probe --record`.
//!
//! The server's `responseStreamingMode` is set to `token` for the recording (one delta per fake
//! chunk instead of per paragraph) through `server.updateSettings`, then restored. Thread ids
//! are fixed, so record against a freshly seeded harness.

mod common;

use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, Result, bail};
use t3_client::{Environment, SyncStatus, ThreadState, commands};
use t3_protocol::{
    ProjectId, ThreadId,
    methods::{Empty, ServerGetSettings, ServerUpdateSettings, SubscribeThread},
    orchestration::{
        ApprovalDecision, InteractionMode, ModelSelection, RuntimeMode, SessionStatus,
        SubscribeThreadInput, ThreadStreamItem, TurnState,
    },
    server::{ResponseStreamingMode, ServerSettingsPatch, UpdateSettingsInput},
};

struct Scenario {
    id: &'static str,
    prompt: &'static str,
    runtime_mode: RuntimeMode,
    interaction_mode: InteractionMode,
}

const SCENARIOS: [Scenario; 4] = [
    Scenario {
        id: "showcase",
        prompt: "Give me a tour of this repo, then make formatBytes handle megabytes and gigabytes.",
        runtime_mode: RuntimeMode::FullAccess,
        interaction_mode: InteractionMode::Default,
    },
    Scenario {
        id: "approval",
        prompt: "Run the database migration against the local dev database.",
        runtime_mode: RuntimeMode::ApprovalRequired,
        interaction_mode: InteractionMode::Default,
    },
    Scenario {
        id: "question",
        prompt: "Set up persistence for the health checks.",
        runtime_mode: RuntimeMode::FullAccess,
        interaction_mode: InteractionMode::Default,
    },
    Scenario {
        id: "plan",
        prompt: "Plan the move to a pnpm monorepo with a shared ui package.",
        runtime_mode: RuntimeMode::FullAccess,
        interaction_mode: InteractionMode::Plan,
    },
];

fn main() -> Result<()> {
    common::init_tracing("warn");
    let mut state_path = PathBuf::from("/tmp/t3ui-e2e/run-nightly/state.json");
    let mut out =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stream-scenarios.jsonl");
    let mut only: Option<Vec<String>> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--state" => state_path = args.next().context("--state needs a path")?.into(),
            "--out" => out = args.next().context("--out needs a path")?.into(),
            "--only" => {
                only = Some(
                    args.next()
                        .context("--only needs a list")?
                        .split(',')
                        .map(str::to_owned)
                        .collect(),
                )
            }
            other => bail!("unexpected argument {other}"),
        }
    }
    let harness = common::HarnessState::load(&state_path)?;
    let recorder = common::Recorder::install();
    let label = t3_client::runtime::runtime().block_on(run(&harness, only.as_deref()))?;
    recorder
        .lock()
        .unwrap()
        .write_jsonl(&out, &harness.redactor(&label))?;
    println!("recorded {}", out.display());
    Ok(())
}

async fn run(harness: &common::HarnessState, only: Option<&[String]>) -> Result<String> {
    let env = harness.connect()?;
    env.wait_connected().await?;
    let label = env
        .config()
        .borrow()
        .as_ref()
        .map(|c| c.environment.label.clone())
        .unwrap_or_default();
    let mut shell = env.shell();
    let shell_state = common::wait_for(&mut shell, Duration::from_secs(20), |s| {
        s.status == SyncStatus::Live
    })
    .await
    .context("shell never became live")?;
    if shell_state
        .threads
        .iter()
        .any(|t| t.id.as_str().starts_with("thread-rec-"))
    {
        bail!("this harness already has recorded threads; restart it (e2e/run-local.sh down/up)");
    }

    let previous_mode = env
        .request::<ServerGetSettings>(&Empty {})
        .await
        .map_err(|e| anyhow::anyhow!("getSettings: {e}"))?
        .response_streaming_mode;
    set_streaming_mode(&env, ResponseStreamingMode::Token).await?;
    let mut config = env.config();
    common::wait_for(&mut config, Duration::from_secs(10), |c| {
        c.as_ref()
            .and_then(|c| c.settings.response_streaming_mode.as_ref())
            == Some(&ResponseStreamingMode::Token)
    })
    .await
    .context("settingsUpdated never arrived")?;
    let project = harness
        .projects
        .iter()
        .find(|p| p.key == "aurora")
        .context("no aurora project in the harness state")?;
    let model = ModelSelection {
        instance_id: "codex".into(),
        model: "gpt-5.4".into(),
        options: Vec::new(),
    };

    for scenario in &SCENARIOS {
        if only.is_some_and(|only| !only.iter().any(|id| id == scenario.id)) {
            continue;
        }
        println!("== {}", scenario.id);
        record(&env, &project.id, &model, scenario).await?;
    }
    if let Some(mode) = previous_mode {
        set_streaming_mode(&env, mode).await?;
    }
    Ok(label)
}

async fn set_streaming_mode(env: &Environment, mode: ResponseStreamingMode) -> Result<()> {
    env.request::<ServerUpdateSettings>(&UpdateSettingsInput {
        patch: ServerSettingsPatch {
            response_streaming_mode: Some(mode),
            ..Default::default()
        },
    })
    .await
    .map_err(|e| anyhow::anyhow!("updateSettings: {e}"))?;
    Ok(())
}

async fn record(
    env: &Environment,
    project: &ProjectId,
    model: &ModelSelection,
    scenario: &Scenario,
) -> Result<()> {
    let thread_id = ThreadId::from(format!("thread-rec-{}", scenario.id));
    env.dispatch(commands::create_thread(
        thread_id.clone(),
        project.clone(),
        format!("Recording: {}", scenario.id),
        model.clone(),
        scenario.runtime_mode.clone(),
        scenario.interaction_mode.clone(),
    ))
    .await?;
    let handle = env.open_thread(thread_id.clone());
    let mut state = handle.state();
    common::wait_for(&mut state, Duration::from_secs(20), |s| {
        s.status == SyncStatus::Live && s.thread.is_some()
    })
    .await
    .context("thread never became live")?;

    let mut turn = commands::turn_start(
        thread_id.clone(),
        scenario.prompt,
        scenario.runtime_mode.clone(),
        scenario.interaction_mode.clone(),
    );
    turn.model_selection = Some(model.clone());
    env.dispatch(turn.into()).await?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    let mut answered = Vec::new();
    loop {
        let current: Arc<ThreadState> = state.borrow_and_update().clone();
        let pending = current.pending_requests();
        for approval in &pending.approvals {
            if !answered.contains(&approval.request_id) {
                println!(
                    "  approving {} ({})",
                    approval.request_id, approval.request_kind
                );
                env.dispatch(commands::respond_to_approval(
                    thread_id.clone(),
                    approval.request_id.clone(),
                    ApprovalDecision::Accept,
                ))
                .await?;
                answered.push(approval.request_id.clone());
            }
        }
        for input in &pending.user_inputs {
            if !answered.contains(&input.request_id) {
                // Pick the second option ("SQLite") for every question.
                let answers: BTreeMap<String, serde_json::Value> = input
                    .questions
                    .iter()
                    .map(|q| {
                        let option = q.options.get(1).or(q.options.first());
                        (
                            q.id.clone(),
                            serde_json::Value::String(
                                option.map(|o| o.label.clone()).unwrap_or_default(),
                            ),
                        )
                    })
                    .collect();
                println!("  answering {} with {answers:?}", input.request_id);
                env.dispatch(commands::respond_to_user_input(
                    thread_id.clone(),
                    input.request_id.clone(),
                    answers,
                    None,
                ))
                .await?;
                answered.push(input.request_id.clone());
            }
        }
        if settled(&current) && pending.is_empty() {
            break;
        }
        tokio::select! {
            changed = state.changed() => changed?,
            _ = tokio::time::sleep_until(deadline) => bail!("{} did not settle", scenario.id),
        }
    }
    // Let trailing events (usage, checkpoints) land on the live subscription.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let current = state.borrow().clone();
    let detail = current.thread.as_ref().context("thread vanished")?;
    println!(
        "  settled at {}: {} messages, {} activities, {} plans, {} checkpoints, turn {:?}",
        current.last_sequence,
        detail.messages.len(),
        detail.activities.len(),
        detail.proposed_plans.len(),
        detail.checkpoints.len(),
        detail.latest_turn.as_ref().map(|t| t.state.as_str()),
    );

    // The server's own view to compare against: a fresh subscription without a cursor.
    let session = env.session().context("disconnected")?;
    let mut fresh = session
        .rpc
        .subscribe::<SubscribeThread>(&SubscribeThreadInput {
            thread_id: thread_id.clone(),
            reasoning_messages: Some(true),
            after_sequence: None,
            request_completion_marker: Some(true),
            turn_limit: Some(10),
        });
    loop {
        match tokio::time::timeout(Duration::from_secs(10), fresh.next()).await {
            Ok(Some(Ok(ThreadStreamItem::Synchronized))) => break,
            Ok(Some(Ok(_))) => {}
            other => bail!("fresh subscription ended early: {other:?}"),
        }
    }
    drop(fresh);
    drop(handle);
    Ok(())
}

/// The latest turn finished and the session is not working on it anymore.
fn settled(state: &ThreadState) -> bool {
    let Some(thread) = &state.thread else {
        return false;
    };
    let turn_done = thread
        .latest_turn
        .as_ref()
        .is_some_and(|t| t.state != TurnState::Running);
    let session_idle = thread
        .session
        .as_ref()
        .is_none_or(|s| !matches!(s.status, SessionStatus::Running | SessionStatus::Starting));
    turn_done && session_idle
}
