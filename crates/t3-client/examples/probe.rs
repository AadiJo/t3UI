//! End-to-end probe against a real T3 Code server.
//!
//! Pairs, connects, prints the environment and shell, creates a project (a temp git repo unless
//! `--workspace` is given) and a thread, optionally sends a message, and follows the thread.
//!
//! ```sh
//! # isolated server (never the daily-driver one):
//! npx -y t3@nightly serve --port 4810 --host 127.0.0.1 --base-dir /tmp/t3ui-client/t3
//! cargo run -p t3-client --example probe -- 'http://127.0.0.1:4810/pair#token=XXXX' \
//!     [--turn "hello"] [--workspace /abs/repo] [--record path.jsonl] [--follow-secs 60]
//!     [--watch-secs 90]
//! ```
//!
//! `--watch-secs` keeps the connection and thread open at the end and prints every status and
//! sequence change: restart the server meanwhile to watch backoff, reconnect, and resume.
//!
//! The first argument is anything `t3 serve`, `t3 pair`, or `t3 auth pairing create` prints
//! (`-` reads it from stdin). `--record` writes every non-keepalive frame (`{"ms","dir":
//! "sent"|"received","frame"}`) and every `/api/orchestration/*` response (`{"ms","dir":"http",
//! "path","status","body"}`) as JSONL, with the host label, tailnet name, and home directory
//! redacted; that is how `tests/fixtures/*.jsonl` are made. Saved state goes to `T3UI_DATA_DIR` (default: a temp dir).

mod common;

use std::{
    io::Read as _,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use t3_client::{
    ClientInfo, Environment, EnvironmentOptions, SyncStatus, commands,
    pairing::parse_pairing_text,
    store::{CatalogStore, SecretBackend, data_dir, open_secret_store},
};
use t3_protocol::{
    ProjectId, ProviderInstanceId, ThreadId,
    orchestration::{InteractionMode, ModelSelection, RuntimeMode},
};

struct Args {
    pairing: String,
    workspace: Option<PathBuf>,
    record: Option<PathBuf>,
    turn: Option<String>,
    model: Option<String>,
    follow: Duration,
    watch: Option<Duration>,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut parsed = Args {
        pairing: String::new(),
        workspace: None,
        record: None,
        turn: None,
        model: None,
        follow: Duration::from_secs(45),
        watch: None,
    };
    while let Some(arg) = args.next() {
        let mut value = || args.next().context(format!("{arg} needs a value"));
        match arg.as_str() {
            "--workspace" => parsed.workspace = Some(value()?.into()),
            "--record" => parsed.record = Some(value()?.into()),
            "--turn" => parsed.turn = Some(value()?),
            "--model" => parsed.model = Some(value()?),
            "--follow-secs" => parsed.follow = Duration::from_secs(value()?.parse()?),
            "--watch-secs" => parsed.watch = Some(Duration::from_secs(value()?.parse()?)),
            "-" => std::io::stdin()
                .read_to_string(&mut parsed.pairing)
                .map(drop)?,
            _ if parsed.pairing.is_empty() => parsed.pairing = arg,
            _ => bail!("unexpected argument {arg}"),
        }
    }
    if parsed.pairing.is_empty() {
        bail!(
            "usage: probe <pairing link | t3 serve output | -> [--turn TEXT] [--workspace DIR] [--record FILE]"
        );
    }
    Ok(parsed)
}

fn main() -> Result<()> {
    common::init_tracing("info,t3_client=debug");
    let args = parse_args()?;
    if std::env::var_os("T3UI_DATA_DIR").is_none() {
        let dir = std::env::temp_dir().join(format!("t3ui-probe-{}", std::process::id()));
        // SAFETY: no other threads exist yet (the runtime starts below).
        unsafe { std::env::set_var("T3UI_DATA_DIR", &dir) };
    }
    let recorder = args.record.is_some().then(common::Recorder::install);

    let result = t3_client::runtime::runtime().block_on(run(&args));
    if let (Some(path), Some(recorder)) = (&args.record, recorder) {
        let label = result.as_ref().ok().cloned().unwrap_or_default();
        recorder
            .lock()
            .unwrap()
            .write_jsonl(path, &common::Redactor::new(&label, Vec::new()))?;
        println!("recorded {}", path.display());
    }
    result.map(drop)
}

/// Runs the probe; returns the environment label (for redaction).
async fn run(args: &Args) -> Result<String> {
    let client = ClientInfo {
        label: "T3UI probe".into(),
        ..ClientInfo::default()
    };

    // 1. Pair and persist (catalog without secrets, token in the secret store).
    let target = parse_pairing_text(&args.pairing)?;
    println!("pairing with {}", target.http_base);
    let paired = t3_client::pair(&target, &client).await?;
    let descriptor = &paired.descriptor;
    println!(
        "environment {} \"{}\" server {} protocol {} on {}/{}",
        descriptor.environment_id,
        descriptor.label,
        descriptor.server_version,
        descriptor.protocol_version(),
        descriptor.platform.os,
        descriptor.platform.arch,
    );
    println!("granted scopes: {}", paired.scopes.join(" "));
    let store = CatalogStore::new();
    let mut catalog = store.load()?;
    catalog.upsert(paired.saved());
    store.save(&catalog)?;
    // File store by default; T3UI_SECRET_STORE=keychain uses the macOS Keychain.
    let secrets = open_secret_store(SecretBackend::from_env());
    secrets.set(&paired.secret_key(), &paired.bearer_token)?;
    println!("saved to {}", data_dir().display());

    // 2. Connect the way the app does at startup: catalog entry + token from the secret store.
    let saved = CatalogStore::new()
        .load()?
        .get(&descriptor.environment_id)
        .cloned()
        .context("paired environment missing from the catalog")?;
    let endpoint = t3_client::saved_bearer_endpoint(&saved, secrets.as_ref())?
        .context("no bearer token in the secret store")?;
    let mut options = EnvironmentOptions::from_saved(&saved, endpoint);
    options.client = client;
    let env = Environment::start(options);
    let session = env.wait_connected().await?;
    println!("connected (generation {})", session.generation);
    let config = env
        .config()
        .borrow()
        .clone()
        .context("connected without a server config")?;
    println!(
        "config: cwd {} | {} providers | completion markers shell={} thread={} | pagination={} reasoning={}",
        config.cwd,
        config.providers.len(),
        config.shell_resume_completion_marker,
        config.thread_resume_completion_marker,
        config.thread_snapshot_pagination,
        config.reasoning_messages,
    );
    for provider in &config.providers {
        println!(
            "  provider {} ({}) status={} installed={} auth={} models={}",
            provider.instance_id,
            provider.driver,
            provider.status,
            provider.installed,
            provider.auth.status,
            provider.models.len()
        );
    }

    // A wrong credential must block (no retry loop) with upstream's copy.
    let bad = Environment::start(EnvironmentOptions::new(
        descriptor.environment_id.clone(),
        descriptor.label.clone(),
        std::sync::Arc::new(t3_client::BearerEndpoint::new(
            paired.http_base.clone(),
            paired.ws_base.clone(),
            "not-a-token".into(),
        )),
    ));
    match bad.wait_connected().await {
        Ok(_) => bail!("a bad credential connected"),
        Err(failure) => println!(
            "bad credential: {:?} -> {}",
            failure.kind,
            bad.status().borrow().status_text()
        ),
    }
    drop(bad);

    // 3. Shell.
    let mut shell = env.shell();
    let state = common::wait_for(&mut shell, Duration::from_secs(20), |s| {
        s.status == SyncStatus::Live
    })
    .await
    .context("shell never became live")?;
    println!(
        "shell live at sequence {}: {} projects, {} threads",
        state.snapshot_sequence,
        state.projects.len(),
        state.threads.len()
    );

    // 4. Project on a fresh git repo.
    let workspace = match &args.workspace {
        Some(dir) => dir.clone(),
        None => make_repo()?,
    };
    let project_id = ProjectId::random();
    let result = env
        .dispatch(commands::create_project(
            project_id.clone(),
            "probe repo",
            workspace.to_string_lossy(),
            false,
        ))
        .await?;
    println!(
        "project.create -> sequence {} ({})",
        result.sequence,
        workspace.display()
    );
    let state = common::wait_for(&mut shell, Duration::from_secs(10), |s| {
        s.project(&project_id).is_some()
    })
    .await
    .context("project never appeared in the shell")?;
    println!("shell has project at sequence {}", state.snapshot_sequence);

    // 5. Thread.
    let model = model_selection(args.model.as_deref(), &config.providers);
    println!("model {}/{}", model.instance_id, model.model);
    let thread_id = ThreadId::random();
    let result = env
        .dispatch(commands::create_thread(
            thread_id.clone(),
            project_id.clone(),
            "probe thread",
            model.clone(),
            RuntimeMode::FullAccess,
            InteractionMode::Default,
        ))
        .await?;
    println!("thread.create -> sequence {}", result.sequence);
    common::wait_for(&mut shell, Duration::from_secs(10), |s| {
        s.thread(&thread_id).is_some()
    })
    .await
    .context("thread never appeared in the shell")?;

    let handle = env.open_thread(thread_id.clone());
    let mut thread = handle.state();
    let state = common::wait_for(&mut thread, Duration::from_secs(20), |s| {
        s.status == SyncStatus::Live && s.thread.is_some()
    })
    .await
    .context("thread never became live")?;
    println!(
        "thread live at sequence {}: \"{}\" {} messages",
        state.last_sequence,
        state
            .thread
            .as_ref()
            .map(|t| t.title.as_str())
            .unwrap_or(""),
        state.thread.as_ref().map_or(0, |t| t.messages.len())
    );

    // 6. Optional turn: follow the thread until the turn settles.
    if let Some(text) = &args.turn {
        let mut turn = commands::turn_start(
            thread_id.clone(),
            text.clone(),
            RuntimeMode::FullAccess,
            InteractionMode::Default,
        );
        turn.model_selection = Some(model);
        if config.environment.capabilities.attachment_uploads {
            let attachment = env
                .upload_attachment(
                    t3_protocol::orchestration::AttachmentKind::File,
                    "notes.txt",
                    "text/plain",
                    b"probe attachment\n".to_vec(),
                )
                .await?;
            println!("uploaded attachment {:?}", attachment.id);
            turn.message.attachments.push(attachment);
        }
        match env.dispatch(turn.into()).await {
            Ok(result) => println!("thread.turn.start -> sequence {}", result.sequence),
            Err(error) => println!("thread.turn.start failed: {error}"),
        }
        follow_thread(&mut thread, args.follow).await;
    }

    // 7. Fresh subscriptions without a cursor, so the recording also holds socket snapshots.
    if let Some(session) = env.session() {
        let mut shell_stream = session
            .rpc
            .subscribe::<t3_protocol::methods::SubscribeShell>(&Default::default());
        if let Ok(Some(Ok(item))) =
            tokio::time::timeout(Duration::from_secs(10), shell_stream.next()).await
        {
            println!("socket shell item: {}", item_kind_shell(&item));
        }
        let mut thread_stream = session
            .rpc
            .subscribe::<t3_protocol::methods::SubscribeThread>(
                &t3_protocol::orchestration::SubscribeThreadInput {
                    thread_id: thread_id.clone(),
                    reasoning_messages: Some(true),
                    after_sequence: None,
                    request_completion_marker: Some(true),
                    turn_limit: Some(10),
                },
            );
        for _ in 0..2 {
            match tokio::time::timeout(Duration::from_secs(10), thread_stream.next()).await {
                Ok(Some(Ok(t3_protocol::orchestration::ThreadStreamItem::Snapshot(snapshot)))) => {
                    println!(
                        "socket thread snapshot at {}: {} messages, {} activities",
                        snapshot.snapshot_sequence,
                        snapshot.thread.messages.len(),
                        snapshot.thread.activities.len()
                    )
                }
                Ok(Some(Ok(other))) => println!("socket thread item: {other:?}"),
                other => {
                    println!("socket thread stream: {other:?}");
                    break;
                }
            }
        }
    }

    let shell_state = shell.borrow().clone();
    if let Some(row) = shell_state.thread(&thread_id) {
        println!(
            "shell row: session={:?} latest_turn={:?} pending_approvals={} pending_input={}",
            row.session.as_ref().map(|s| s.status.as_str()),
            row.latest_turn.as_ref().map(|t| t.state.as_str()),
            row.has_pending_approvals,
            row.has_pending_user_input,
        );
    }
    if let Some(limit) = args.watch {
        watch_connection(&env, &mut shell, &mut thread, limit).await;
    }
    drop(handle);
    drop(env);
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(descriptor.label.clone())
}

fn item_kind_shell(item: &t3_protocol::orchestration::ShellStreamItem) -> String {
    use t3_protocol::orchestration::ShellStreamItem::*;
    match item {
        Snapshot(s) => format!(
            "snapshot at {} ({} projects, {} threads)",
            s.snapshot_sequence,
            s.projects.len(),
            s.threads.len()
        ),
        other => format!("{other:?}"),
    }
}

/// Prints thread state changes until the latest turn settles or `limit` passes.
async fn follow_thread(
    thread: &mut tokio::sync::watch::Receiver<Arc<t3_client::ThreadState>>,
    limit: Duration,
) {
    let deadline = tokio::time::Instant::now() + limit;
    let mut last = String::new();
    loop {
        let state = thread.borrow_and_update().clone();
        if let Some(detail) = &state.thread {
            let line = format!(
                "seq {} | {} msgs | {} activities | session {} | turn {}{}",
                state.last_sequence,
                detail.messages.len(),
                detail.activities.len(),
                detail.session.as_ref().map_or("-", |s| s.status.as_str()),
                detail
                    .latest_turn
                    .as_ref()
                    .map_or("-", |t| t.state.as_str()),
                detail
                    .messages
                    .last()
                    .map(|m| format!(
                        " | last {} {}{:?}",
                        m.role,
                        if m.streaming { "(streaming) " } else { "" },
                        m.text.chars().take(60).collect::<String>()
                    ))
                    .unwrap_or_default(),
            );
            if line != last {
                println!("  {line}");
                for activity in detail.activities.iter().rev().take(1) {
                    println!(
                        "    activity {} [{}] {}",
                        activity.kind, activity.tone, activity.summary
                    );
                }
                last = line;
            }
            let settled = detail
                .latest_turn
                .as_ref()
                .is_some_and(|t| t.state != t3_protocol::orchestration::TurnState::Running)
                && detail.session.as_ref().is_none_or(|s| {
                    !matches!(
                        s.status,
                        t3_protocol::orchestration::SessionStatus::Running
                            | t3_protocol::orchestration::SessionStatus::Starting
                    )
                });
            if settled {
                println!("  turn settled");
                return;
            }
        }
        tokio::select! {
            changed = thread.changed() => if changed.is_err() { return },
            _ = tokio::time::sleep_until(deadline) => {
                println!("  stopped following after {}s", limit.as_secs());
                return;
            }
        }
    }
}

/// Prints connection status, shell, and thread sequence changes for `limit`.
async fn watch_connection(
    env: &Environment,
    shell: &mut tokio::sync::watch::Receiver<Arc<t3_client::ShellState>>,
    thread: &mut tokio::sync::watch::Receiver<Arc<t3_client::ThreadState>>,
    limit: Duration,
) {
    println!("watching for {}s (restart the server now)", limit.as_secs());
    let started = Instant::now();
    let deadline = tokio::time::Instant::now() + limit;
    let mut status = env.status();
    loop {
        let shell_state = shell.borrow_and_update().clone();
        let thread_state = thread.borrow_and_update().clone();
        let line = format!(
            "[{:>5.1}s] {} | shell {:?} @{} | thread {:?} @{}",
            started.elapsed().as_secs_f32(),
            status.borrow_and_update().status_text(),
            shell_state.status,
            shell_state.snapshot_sequence,
            thread_state.status,
            thread_state.last_sequence,
        );
        println!("  {line}");
        tokio::select! {
            _ = status.changed() => {}
            _ = shell.changed() => {}
            _ = thread.changed() => {}
            _ = tokio::time::sleep_until(deadline) => return,
        }
    }
}

/// `--model instance/model`, else the first provider's default model, else codex.
fn model_selection(
    requested: Option<&str>,
    providers: &[t3_protocol::server::ServerProvider],
) -> ModelSelection {
    if let Some((instance, model)) = requested.and_then(|m| m.split_once('/')) {
        return ModelSelection {
            instance_id: ProviderInstanceId::from(instance),
            model: model.to_owned(),
            options: Vec::new(),
        };
    }
    providers
        .iter()
        .find(|p| p.enabled)
        .and_then(|p| {
            let model = p
                .models
                .iter()
                .find(|m| m.is_default)
                .or(p.models.first())?;
            Some(ModelSelection {
                instance_id: p.instance_id.clone(),
                model: model.slug.clone(),
                options: Vec::new(),
            })
        })
        .unwrap_or_else(|| ModelSelection {
            instance_id: ProviderInstanceId::from("codex"),
            model: "gpt-5.5".into(),
            options: Vec::new(),
        })
}

/// A throwaway git repo with one commit.
fn make_repo() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("t3ui-probe-repo-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("README.md"), "# probe repo\n")?;
    let git = |args: &[&str]| -> Result<()> {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .env("GIT_AUTHOR_NAME", "probe")
            .env("GIT_AUTHOR_EMAIL", "probe@example.com")
            .env("GIT_COMMITTER_NAME", "probe")
            .env("GIT_COMMITTER_EMAIL", "probe@example.com")
            .output()?;
        if !status.status.success() {
            bail!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&status.stderr)
            );
        }
        Ok(())
    };
    git(&["init", "-q", "-b", "main"])?;
    git(&["add", "."])?;
    git(&["commit", "-q", "-m", "init"])?;
    Ok(dir)
}
