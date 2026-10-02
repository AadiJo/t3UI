//! T3 Connect probe: runs the native T3 Connect flow end to end from a terminal.
//!
//! ```sh
//! # With your own T3 account: email code sign-in, list linked environments with relay status,
//! # connect to one, print its projects. State goes to --data-dir (default: a temp dir, so you
//! # sign in every run; pass a dir to keep the session).
//! cargo run -p t3-client --example connect_probe -- [sign-in] [--email you@example.com]
//!     [--connect <label or environment id>] [--data-dir DIR] [--sign-out]
//!
//! # No account needed:
//! cargo run -p t3-client --example connect_probe -- public          # public Clerk/relay shapes
//! cargo run -p t3-client --example connect_probe -- proofs 20       # DPoP proofs as JSON lines
//! cargo run -p t3-client --example connect_probe -- dpop \
//!     --pair-cmd "node e2e/seed.mjs --pair --state /tmp/t3ui-e2e/run-nightly/state.json" \
//!     --state /tmp/t3ui-e2e/run-nightly/state.json --record /tmp/t3ui-dpop.json
//! ```
//!
//! `dpop` exercises everything after the relay on a throwaway `t3 serve` (e2e/run-local.sh): a
//! pairing credential stands in for the relay's one-time credential, and the client redeems it
//! with a DPoP proof exactly as for a T3 Connect environment. It then revokes the session with
//! the admin token from `state.json` to force a 401 and checks the token is re-minted, and
//! writes the HTTP exchange (paths and statuses only) to `--record`.

use std::{
    io::Write as _,
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result, bail, ensure};
use serde_json::{Value, json};
use t3_client::{
    ClientInfo, ConnectionFailure, Environment, EnvironmentOptions,
    auth::BoxFuture,
    cloud::{
        Availability, Bootstrap, BootstrapSource, CloudConfig, DpopEndpoint, T3Connect,
        clerk::ClerkClient, dpop::DpopKey, relay::RelayErrorBody,
    },
    pairing::parse_pairing_text,
    store::{CatalogStore, FileSecretStore, SecretStore},
};
use t3_protocol::EnvironmentId;
use url::Url;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,t3_client=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let command = match args.first().map(String::as_str) {
        Some("public" | "proofs" | "dpop" | "sign-in") => args.remove(0),
        _ => "sign-in".to_owned(),
    };
    let runtime = t3_client::runtime::runtime();
    match command.as_str() {
        "public" => runtime.block_on(public()),
        "proofs" => proofs(args.first().map(|n| n.parse()).transpose()?.unwrap_or(10)),
        "dpop" => runtime.block_on(dpop(&args)),
        _ => runtime.block_on(sign_in(&args)),
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn prompt(question: &str) -> Result<String> {
    print!("{question}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

// ---------------------------------------------------------------------------------------------
// sign-in: the real flow, with the user's account.

async fn sign_in(args: &[String]) -> Result<()> {
    let data_dir = flag(args, "--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("t3ui-connect-probe-{}", std::process::id()))
        });
    let secrets: Arc<dyn SecretStore> =
        Arc::new(FileSecretStore::at(data_dir.join("secrets.json")));
    let catalog_store = CatalogStore::at(data_dir.join("environments.json"));
    let connect = T3Connect::new(CloudConfig::production(), secrets)?;
    println!(
        "state in {} (key {})",
        data_dir.display(),
        connect.key_thumbprint()
    );

    if connect.account().is_some() {
        connect.restore().await?;
    }
    let account = match connect.account() {
        Some(account) => account,
        None => {
            let email = match flag(args, "--email") {
                Some(email) => email,
                None => prompt("Email: ")?,
            };
            let pending = connect.start_email_sign_in(&email).await?;
            println!("Clerk sent a code to {}", pending.masked_email());
            loop {
                let code = prompt("Code (or 'resend'): ")?;
                if code == "resend" {
                    pending.resend().await?;
                    println!("sent again");
                    continue;
                }
                match pending.verify(&code).await {
                    Ok(account) => break account,
                    Err(error) => println!("  {error}"),
                }
            }
        }
    };
    println!(
        "signed in as {} ({}, session {})",
        account.display_name(),
        account.user_id,
        account.session_id
    );

    connect.refresh_now().await;
    let state = connect.state().borrow().clone();
    if let Some(error) = &state.discovery.error {
        bail!("could not list environments: {error}");
    }
    println!(
        "{} linked environment(s):",
        state.discovery.environments.len()
    );
    for entry in &state.discovery.environments {
        let status = match &entry.availability {
            Availability::Checking => "checking".to_owned(),
            Availability::Online => "online".to_owned(),
            Availability::Offline { reason } => {
                format!(
                    "offline{}",
                    reason
                        .as_ref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                )
            }
            Availability::Error(failure) => format!("error: {failure}"),
        };
        println!(
            "  {} [{}] {} via {} - {status}",
            entry.environment.label,
            entry.environment.environment_id,
            entry.environment.endpoint.http_base_url,
            entry.environment.endpoint.provider_kind,
        );
    }

    let choice = match flag(args, "--connect") {
        Some(choice) => Some(choice),
        None if !state.discovery.environments.is_empty() => {
            Some(prompt("Connect to (label or id, empty to skip): ")?).filter(|c| !c.is_empty())
        }
        None => None,
    };
    if let Some(choice) = choice {
        let entry = state
            .discovery
            .environments
            .iter()
            .find(|e| {
                e.environment.label == choice || e.environment.environment_id.as_str() == choice
            })
            .with_context(|| format!("no linked environment {choice}"))?;
        let saved = connect.saved_environment(&entry.environment)?;
        let mut catalog = catalog_store.load()?;
        catalog.upsert(saved.clone());
        catalog_store.save(&catalog)?;
        let endpoint = connect
            .saved_endpoint(&saved)
            .context("saved relay entry has no endpoint")?;
        let environment = Environment::start(EnvironmentOptions::from_saved(&saved, endpoint));
        println!("connecting to {} ...", saved.label);
        let session = tokio::time::timeout(Duration::from_secs(60), environment.wait_connected())
            .await
            .context("timed out")??;
        println!(
            "connected: {} server {} (generation {})",
            session.descriptor.label, session.descriptor.server_version, session.generation
        );
        let shell = session.http.shell_snapshot().await?;
        println!(
            "{} project(s), {} thread(s) over DPoP HTTP",
            shell.projects.len(),
            shell.threads.len()
        );
    }

    if args.iter().any(|a| a == "--sign-out") {
        connect.sign_out().await?;
        let mut catalog = catalog_store.load()?;
        let removed = t3_client::cloud::remove_relay_environments(&mut catalog);
        catalog_store.save(&catalog)?;
        println!("signed out; removed {} relay environment(s)", removed.len());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// public: unauthenticated endpoints, decoded with this crate's types.

async fn public() -> Result<()> {
    let config = CloudConfig::production();
    let http = reqwest::Client::new();

    let mut url = config.clerk_frontend_api.join("/v1/environment")?;
    url.query_pairs_mut().append_pair("_is_native", "1");
    let environment: Value = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let auth = &environment["auth_config"];
    ensure!(
        auth["native_settings"]["api_enabled"] == json!(true),
        "Clerk native API is disabled"
    );
    let first_factors: Vec<&str> = auth["first_factors"]
        .as_array()
        .context("first_factors")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    ensure!(
        first_factors.contains(&"email_code"),
        "email_code sign-in is off"
    );
    for provider in t3_client::cloud::OAuthProvider::ALL {
        let enabled = &environment["user_settings"]["social"][provider.strategy()]["enabled"];
        ensure!(
            enabled == &json!(true),
            "{} is not enabled",
            provider.label()
        );
    }
    println!(
        "clerk: native api on, first factors {first_factors:?}, single session {}, captcha on sign-up {}",
        auth["single_session_mode"], environment["user_settings"]["sign_up"]["captcha_enabled"],
    );

    // Our FAPI transport against the live instance: an anonymous client, which Clerk creates
    // and returns a client token for (what any visit to app.t3.codes does). No sign-in.
    let dir = std::env::temp_dir().join(format!("t3ui-connect-public-{}", std::process::id()));
    let secrets: Arc<dyn SecretStore> = Arc::new(FileSecretStore::at(dir.join("secrets.json")));
    let clerk = ClerkClient::new(&config.clerk_frontend_api, secrets.clone())?;
    let client = clerk.client().await?.context("Clerk returned no client")?;
    ensure!(clerk.has_client(), "Clerk issued no client token");
    ensure!(
        client.active_session().is_none(),
        "anonymous client has a session"
    );
    println!(
        "clerk: anonymous client {} with a stored client token",
        client.id
    );
    let _ = std::fs::remove_dir_all(&dir);

    let metadata: Value = http
        .get(
            config
                .relay_url
                .join("/.well-known/oauth-authorization-server")?,
        )
        .send()
        .await?
        .json()
        .await?;
    let issuer = config.relay_url.origin().ascii_serialization();
    ensure!(
        metadata["issuer"] == json!(issuer),
        "relay issuer differs from the token exchange resource"
    );
    ensure!(metadata["dpop_signing_alg_values_supported"] == json!(["ES256"]));
    ensure!(metadata["token_endpoint"] == json!(format!("{issuer}/v1/client/dpop-token")));
    println!("relay: issuer and DPoP token endpoint as expected");

    let unauthenticated = http
        .get(config.relay_url.join("/v1/environments")?)
        .send()
        .await?;
    ensure!(unauthenticated.status() == 401);
    let body: RelayErrorBody = unauthenticated.json().await?;
    ensure!(body.tag == "RelayAuthInvalidError" && body.trace_id.is_some());
    println!(
        "relay: unauthenticated listing -> {} ({})",
        body.tag,
        body.reason.unwrap_or_default()
    );
    println!("public: ok");
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// proofs: input for e2e/dpop-crosscheck.mjs.

fn proofs(count: usize) -> Result<()> {
    let key = DpopKey::generate();
    let cases = [
        ("POST", "https://relay.t3.codes/v1/client/dpop-token", None),
        (
            "POST",
            "https://relay.t3.codes/v1/environments/env-1/connect",
            Some("relay-access-token"),
        ),
        ("POST", "http://127.0.0.1:4710/oauth/token", None),
        (
            "GET",
            "https://prod-abc.example.dev/api/orchestration/threads/t-1?turnLimit=10",
            Some("env-token"),
        ),
    ];
    for i in 0..count {
        let (method, url, token) = cases[i % cases.len()];
        let url = Url::parse(url)?;
        println!(
            "{}",
            json!({
                "method": method,
                "url": url.as_str(),
                "accessToken": token,
                "thumbprint": key.thumbprint(),
                "proof": key.proof(method, &url, token),
            })
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// dpop: the environment half of a relay connection, against a throwaway server.

/// Runs `--pair-cmd` for every credential, like the relay minting one per connect.
struct CommandSource {
    command: String,
    mints: AtomicUsize,
}

impl BootstrapSource for CommandSource {
    fn identity(&self) -> Option<String> {
        Some("probe".into())
    }

    fn bootstrap(
        &self,
        _environment_id: EnvironmentId,
    ) -> BoxFuture<Result<Bootstrap, ConnectionFailure>> {
        self.mints.fetch_add(1, Ordering::SeqCst);
        let command = self.command.clone();
        Box::pin(async move {
            let fail = |detail: String| {
                ConnectionFailure::blocked(
                    t3_client::connection::BlockedReason::Configuration,
                    detail,
                )
            };
            let output = tokio::task::spawn_blocking(move || {
                Command::new("sh").arg("-c").arg(&command).output()
            })
            .await
            .map_err(|e| fail(e.to_string()))?
            .map_err(|e| fail(e.to_string()))?;
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            // seed.mjs prints {"credential","pairingUrl",..}; anything `t3` prints works too.
            let text = serde_json::from_str::<Value>(stdout.trim())
                .ok()
                .and_then(|v| v["pairingUrl"].as_str().map(str::to_owned))
                .unwrap_or(stdout);
            let target = parse_pairing_text(&text).map_err(|e| fail(e.to_string()))?;
            Ok(Bootstrap {
                http_base: target.http_base,
                ws_base: target.ws_base,
                credential: target.credential,
            })
        })
    }
}

async fn dpop(args: &[String]) -> Result<()> {
    let command = flag(args, "--pair-cmd").context("--pair-cmd is required")?;
    let state: Option<Value> = flag(args, "--state")
        .map(|path| -> Result<Value> { Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?) })
        .transpose()?;
    let exchanges = Arc::new(Mutex::new(Vec::<Value>::new()));
    {
        let exchanges = exchanges.clone();
        t3_client::http::set_response_tap(move |method, url, status, _body| {
            exchanges
                .lock()
                .unwrap()
                .push(json!({ "method": method.as_str(), "path": url.path(), "status": status }));
        });
    }
    let mut report = serde_json::Map::new();
    let mut step = |name: &str, value: Value| {
        println!("{name}: {value}");
        report.insert(name.to_owned(), value);
    };

    let source = Arc::new(CommandSource {
        command,
        mints: AtomicUsize::new(0),
    });
    // The environment id comes from a first credential's host; that credential goes unused.
    let probe_target = source.bootstrap("unknown".into()).await?;
    source.mints.store(0, Ordering::SeqCst);
    let descriptor = t3_client::EnvironmentHttp::new(probe_target.http_base.clone(), None)
        .descriptor()
        .await?;
    step(
        "environment",
        json!({ "id": descriptor.environment_id.as_str(), "server": descriptor.server_version }),
    );

    let key = Arc::new(DpopKey::generate());
    let client = ClientInfo {
        label: "T3UI connect probe".into(),
        ..ClientInfo::default()
    };
    let endpoint = Arc::new(DpopEndpoint::new(
        descriptor.environment_id.clone(),
        key.clone(),
        source.clone(),
        client,
    ));
    let environment = Environment::start(EnvironmentOptions::new(
        descriptor.environment_id.clone(),
        descriptor.label.clone(),
        endpoint,
    ));
    let session = tokio::time::timeout(Duration::from_secs(60), environment.wait_connected())
        .await
        .context("timed out connecting")??;
    step(
        "connected",
        json!({ "generation": session.generation, "mints": source.mints.load(Ordering::SeqCst) }),
    );

    let auth = session.http.session().await?;
    ensure!(auth.authenticated, "session check says unauthenticated");
    ensure!(
        auth.session_method.as_deref() == Some("dpop-access-token"),
        "session method is {:?}, not dpop-access-token",
        auth.session_method
    );
    step(
        "session",
        json!({ "method": auth.session_method, "scopes": auth.scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>() }),
    );

    let shell = session.http.shell_snapshot().await?;
    step(
        "shell over DPoP",
        json!({ "projects": shell.projects.len(), "threads": shell.threads.len() }),
    );

    if let Some(state) = &state {
        let base = Url::parse(state["baseUrl"].as_str().context("state.baseUrl")?)?;
        let admin = state["accessToken"].as_str().context("state.accessToken")?;
        let http = reqwest::Client::new();
        let clients: Vec<Value> = http
            .get(base.join("/api/auth/clients")?)
            .bearer_auth(admin)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ours: Vec<&str> = clients
            .iter()
            .filter(|c| c["method"] == json!("dpop-access-token"))
            .filter_map(|c| c["sessionId"].as_str())
            .collect();
        ensure!(!ours.is_empty(), "the server lists no DPoP session");
        for session_id in &ours {
            http.post(base.join("/api/auth/clients/revoke")?)
                .bearer_auth(admin)
                .json(&json!({ "sessionId": session_id }))
                .send()
                .await?
                .error_for_status()?;
        }
        step("revoked", json!({ "sessions": ours.len() }));

        // The next request presents the revoked token, gets 401, re-mints, and retries.
        let before = source.mints.load(Ordering::SeqCst);
        let shell = session.http.shell_snapshot().await?;
        let after = source.mints.load(Ordering::SeqCst);
        ensure!(
            after == before + 1,
            "expected one re-mint after the 401, got {}",
            after - before
        );
        step(
            "shell after revoke",
            json!({ "projects": shell.projects.len(), "mints": after }),
        );

        // A new socket on the re-minted (cached) token: no further mint.
        let previous = session.generation;
        let mut sessions = environment.sessions();
        environment.retry_now();
        let session = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let Some(session) = sessions.borrow_and_update().clone()
                    && session.generation > previous
                {
                    return Ok(session);
                }
                sessions.changed().await?;
            }
        })
        .await
        .context("timed out reconnecting")?
        .map_err(|_: tokio::sync::watch::error::RecvError| anyhow::anyhow!("environment closed"))?;
        let mints = source.mints.load(Ordering::SeqCst);
        ensure!(
            mints == after,
            "reconnecting minted again instead of reusing the token"
        );
        step(
            "reconnected",
            json!({ "generation": session.generation, "mints": mints }),
        );
    }

    let log = exchanges.lock().unwrap().clone();
    // 401 on a request, then a successful /oauth/token, then the same request succeeding.
    let unauthorized_then_ok = log.iter().enumerate().any(|(i, rejected)| {
        rejected["status"] == json!(401)
            && log[i + 1..].iter().enumerate().any(|(j, minted)| {
                minted["path"] == json!("/oauth/token")
                    && minted["status"] == json!(200)
                    && log[i + 1 + j + 1..].iter().any(|retried| {
                        retried["path"] == rejected["path"] && retried["status"] == json!(200)
                    })
            })
    });
    if state.is_some() {
        ensure!(
            unauthorized_then_ok,
            "no 401 -> /oauth/token -> retry sequence in the HTTP log"
        );
    }
    report.insert("http".into(), Value::Array(log));
    report.insert("keyThumbprint".into(), json!(key.thumbprint()));
    if let Some(path) = flag(args, "--record") {
        std::fs::write(&path, serde_json::to_string_pretty(&Value::Object(report))?)?;
        println!("recorded {path}");
    }
    println!("dpop: ok");
    Ok(())
}
