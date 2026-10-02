//! Shared by the examples: recording frames and HTTP responses to JSONL, redacting
//! machine-specific strings, and connecting to an `e2e/run-local.sh` harness.
#![allow(dead_code)] // Each example uses a different subset.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use t3_client::{
    BearerEndpoint, Environment, EnvironmentOptions,
    rpc::{FrameDirection, set_frame_tap},
};
use t3_protocol::{EnvironmentId, ProjectId, ThreadId};

/// Collects WebSocket frames and orchestration HTTP responses. Install once per process.
#[derive(Default)]
pub struct Recorder {
    started: Option<Instant>,
    lines: Vec<(u128, Recorded)>,
}

pub enum Recorded {
    Frame(FrameDirection, String),
    /// Only `/api/orchestration/*` responses; auth endpoints carry credentials.
    Http {
        path: String,
        status: u16,
        body: String,
    },
}

impl Recorder {
    /// Installs the process-wide frame and HTTP taps, skipping keepalives.
    pub fn install() -> Arc<Mutex<Recorder>> {
        let recorder = Arc::new(Mutex::new(Recorder::default()));
        let frames = recorder.clone();
        set_frame_tap(move |direction, text| {
            if text == r#"{"_tag":"Ping"}"# || text == r#"{"_tag":"Pong"}"# {
                return;
            }
            frames
                .lock()
                .unwrap()
                .push(Recorded::Frame(direction, text.to_owned()));
        });
        let responses = recorder.clone();
        t3_client::http::set_response_tap(move |_method, url, status, body| {
            if !url.path().starts_with("/api/orchestration/") {
                return;
            }
            let path = match url.query() {
                Some(query) => format!("{}?{query}", url.path()),
                None => url.path().to_owned(),
            };
            responses.lock().unwrap().push(Recorded::Http {
                path,
                status,
                body: String::from_utf8_lossy(body).into_owned(),
            });
        });
        recorder
    }

    fn push(&mut self, entry: Recorded) {
        let started = *self.started.get_or_insert_with(Instant::now);
        self.lines.push((started.elapsed().as_millis(), entry));
    }

    /// Drops everything recorded so far (e.g. connection setup you do not want in a fixture).
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Writes `{"ms","dir":"sent"|"received","frame"}` and `{"ms","dir":"http","path",
    /// "status","body"}` lines, redacted.
    pub fn write_jsonl(&self, path: &Path, redactor: &Redactor) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        for (ms, entry) in &self.lines {
            let line = match entry {
                Recorded::Frame(direction, frame) => serde_json::json!({
                    "ms": ms,
                    "dir": match direction {
                        FrameDirection::Sent => "sent",
                        FrameDirection::Received => "received",
                    },
                    "frame": serde_json::from_str::<serde_json::Value>(&redactor.redact(frame))?,
                }),
                Recorded::Http { path, status, body } => serde_json::json!({
                    "ms": ms,
                    "dir": "http",
                    "path": path,
                    "status": status,
                    "body": serde_json::from_str::<serde_json::Value>(&redactor.redact(body))
                        .unwrap_or_else(|_| serde_json::Value::String(redactor.redact(body))),
                }),
            };
            writeln!(file, "{line}")?;
        }
        Ok(())
    }
}

/// Replaces machine-specific strings: the host label, tailnet names, home and checkout paths.
pub struct Redactor {
    replacements: Vec<(String, String)>,
}

impl Redactor {
    /// `label` is the environment label (the hostname). Extra replacements apply first, so
    /// pass the most specific paths (run dir, checkout) there.
    pub fn new(label: &str, extra: Vec<(String, String)>) -> Self {
        let mut replacements = extra;
        if !label.is_empty() {
            replacements.push((format!("\"{label}\""), "\"fixture-host\"".into()));
        }
        if let Ok(home) = std::env::var("HOME")
            && home.len() > 1
        {
            replacements.push((home, "/home/user".into()));
        }
        if let Ok(user) = std::env::var("USER")
            && user.len() > 2
        {
            replacements.push((format!("/{user}/"), "/user/".into()));
        }
        Redactor { replacements }
    }

    pub fn redact(&self, text: &str) -> String {
        let mut text = text.to_owned();
        for (from, to) in &self.replacements {
            text = text.replace(from, to);
        }
        redact_tailnet_hosts(&text)
    }
}

/// Replaces `<machine>.<tailnet>.ts.net` hostnames, which identify the recording machine.
fn redact_tailnet_hosts(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(".ts.net") {
        let head = &rest[..index];
        let start = head
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-'))
            .map_or(0, |i| i + 1);
        out.push_str(&head[..start]);
        out.push_str("fixture-host.tailnet.ts.net");
        rest = &rest[index + ".ts.net".len()..];
    }
    out.push_str(rest);
    out
}

/// `state.json` written by `e2e/seed.mjs`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessState {
    pub base_url: String,
    pub server_version: String,
    pub environment_id: EnvironmentId,
    pub access_token: String,
    pub projects: Vec<HarnessProject>,
    pub threads: Vec<HarnessThread>,
    pub seeded_at: String,
    /// Directory of `state.json` (the run dir); filled in by [`HarnessState::load`].
    #[serde(skip)]
    pub run_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessProject {
    pub key: String,
    pub id: ProjectId,
    pub title: String,
    pub workspace_root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessThread {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub scenario: String,
    pub prompt: String,
    pub title: String,
    pub expect: String,
    pub archived: bool,
    pub runtime_mode: String,
    pub interaction_mode: String,
}

impl HarnessState {
    pub fn load(path: &Path) -> Result<Self> {
        let mut state: HarnessState = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("reading {}", path.display()))?,
        )?;
        state.run_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(state)
    }

    /// Starts an environment using the harness's bearer token (no pairing).
    pub fn connect(&self) -> Result<Environment> {
        let http: url::Url = self.base_url.parse()?;
        let ws = t3_client::pairing::ws_base_url(&http);
        Ok(Environment::start(EnvironmentOptions::new(
            self.environment_id.clone(),
            "e2e",
            Arc::new(BearerEndpoint::new(http, ws, self.access_token.clone())),
        )))
    }

    /// Redactions that make fixtures identical across machines and run dirs: the run dir maps
    /// to the harness default, the checkout to `/home/user/t3UI`.
    pub fn redactor(&self, label: &str) -> Redactor {
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Redactor::new(
            label,
            vec![
                (
                    self.run_dir.to_string_lossy().into_owned(),
                    "/tmp/t3ui-e2e/run-nightly".into(),
                ),
                (checkout, "/home/user/t3UI".into()),
            ],
        )
    }
}

/// Waits until `ready` holds for the watched value, or `limit` passes.
pub async fn wait_for<T: Clone>(
    rx: &mut tokio::sync::watch::Receiver<T>,
    limit: Duration,
    ready: impl Fn(&T) -> bool,
) -> Option<T> {
    tokio::time::timeout(limit, async {
        loop {
            let value = rx.borrow_and_update().clone();
            if ready(&value) {
                return Some(value);
            }
            rx.changed().await.ok()?;
        }
    })
    .await
    .ok()
    .flatten()
}

/// Logs to stderr; `RUST_LOG` overrides the default filter.
pub fn init_tracing(default: &str) {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| default.into()),
        )
        .with_writer(std::io::stderr)
        .init();
}
