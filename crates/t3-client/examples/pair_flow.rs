//! End-to-end check of the Add environment dialog's logic against a live `t3 serve`.
//!
//! The dialog's non-UI half lives in `crates/t3-app/src/pairing/flow.rs` and is compiled here
//! unchanged (`#[path]` below), because GPUI can't build on the Linux dev hosts. The run:
//!
//! 1. pastes the pairing link into the Host field (field splitting),
//! 2. pairs and saves into a fresh data dir (catalog + secret store),
//! 3. starts the saved environment the way the app does at launch and waits for `Connected`,
//!    then reads the shell (projects arrive),
//! 4. checks the error paths: re-using the one-time code, an unreachable host, empty fields,
//! 5. forgets the environment ("Disconnect") and checks the catalog and secret are gone.
//!
//! Every step is written to `<data-dir>/pair-flow-report.json`, the artifact to inspect.
//!
//! ```sh
//! e2e/run-local.sh up --server nightly --detach
//! LINK=$(node e2e/seed.mjs --pair --state /tmp/t3ui-e2e/run-nightly/state.json | tail -1)
//! cargo run -p t3-client --example pair_flow -- "$LINK" [--data-dir /tmp/pair-flow]
//! ```

#[path = "../../t3-app/src/pairing/flow.rs"]
#[allow(dead_code)]
mod flow;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, Result, bail, ensure};
use serde_json::{Value, json};
use t3_client::{
    ClientInfo, Environment, EnvironmentOptions, saved_bearer_endpoint,
    store::{CatalogStore, FileSecretStore},
};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut link = None;
    let mut data_dir = std::env::temp_dir().join(format!("t3ui-pair-flow-{}", std::process::id()));
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => {
                data_dir = PathBuf::from(args.next().context("--data-dir needs a path")?)
            }
            _ if link.is_none() => link = Some(arg),
            _ => bail!("unexpected argument {arg}"),
        }
    }
    let link = link.context("usage: pair_flow <pairing link> [--data-dir DIR]")?;
    if data_dir.exists() {
        std::fs::remove_dir_all(&data_dir)?;
    }
    std::fs::create_dir_all(&data_dir)?;
    let runtime = t3_client::runtime::runtime();
    let mut report = Vec::<Value>::new();
    let result = runtime.block_on(run(&link, &data_dir, &mut report));
    let outcome = match &result {
        Ok(()) => json!({"ok": true}),
        Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
    };
    report.push(json!({"step": "done", "result": outcome}));
    let path = data_dir.join("pair-flow-report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report)?)?;
    println!("report: {}", path.display());
    result
}

fn stores(data_dir: &std::path::Path) -> flow::PairingStores {
    flow::PairingStores {
        catalog: CatalogStore::at(data_dir.join("environments.json")),
        secrets: Arc::new(FileSecretStore::at(data_dir.join("secrets.json"))),
    }
}

fn step(report: &mut Vec<Value>, name: &str, detail: Value) {
    println!("{name}: {detail}");
    report.push(json!({"step": name, "detail": detail}));
}

async fn run(link: &str, data_dir: &std::path::Path, report: &mut Vec<Value>) -> Result<()> {
    let stores = stores(data_dir);

    // 1. Paste into Host: the dialog splits the link into both fields.
    let split = flow::split_pairing_input(link).context("pairing link was not recognized")?;
    step(
        report,
        "split",
        json!({"host": split.host, "code": "<redacted>"}),
    );

    // Validation copy for empty fields.
    let empty_host = flow::resolve_fields("", "").unwrap_err();
    let empty_code = flow::resolve_fields(&split.host, "").unwrap_err();
    ensure!(
        empty_host == "Enter a backend host.",
        "empty host: {empty_host}"
    );
    ensure!(
        empty_code == "Enter a pairing code.",
        "empty code: {empty_code}"
    );
    step(
        report,
        "validation",
        json!({"emptyHost": empty_host, "emptyCode": empty_code}),
    );

    // 2. Pair and save.
    let target = flow::resolve_fields(&split.host, &split.code).map_err(anyhow::Error::msg)?;
    let added = flow::pair_and_save(target.clone(), ClientInfo::default(), stores.clone())
        .await
        .map_err(anyhow::Error::msg)?;
    let catalog = stores.catalog.load()?;
    ensure!(
        catalog.environments.len() == 1,
        "catalog should hold one entry"
    );
    let connection_id = format!("bearer:{}", added.saved.environment_id);
    ensure!(
        stores.secrets.get(&connection_id)?.is_some(),
        "bearer token missing from the secret store"
    );
    step(
        report,
        "paired",
        json!({
            "environmentId": added.saved.environment_id.to_string(),
            "label": added.saved.label,
            "catalogEntries": catalog.environments.len(),
            "tokenSaved": true,
        }),
    );

    // 3. Next launch: start from disk exactly like `state::boot`, and wait for the shell.
    let saved = catalog.environments[0].clone();
    let endpoint = saved_bearer_endpoint(&saved, stores.secrets.as_ref())?
        .context("saved entry has no endpoint")?;
    let environment = Environment::start(EnvironmentOptions::from_saved(&saved, endpoint));
    let session = tokio::time::timeout(Duration::from_secs(30), environment.wait_connected())
        .await
        .context("timed out waiting for Connected")?
        .map_err(|failure| anyhow::anyhow!("connection blocked: {}", failure.detail))?;
    let mut shell = environment.shell();
    let projects = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let projects: Vec<String> = shell
                .borrow_and_update()
                .projects
                .iter()
                .map(|project| project.title.clone())
                .collect();
            if !projects.is_empty() {
                return projects;
            }
            if shell.changed().await.is_err() {
                return projects;
            }
        }
    })
    .await
    .context("timed out waiting for the shell")?;
    ensure!(!projects.is_empty(), "shell has no projects");
    step(
        report,
        "connected",
        json!({
            "status": environment.status().borrow().status_text(),
            "serverVersion": session.descriptor.server_version,
            "projects": projects,
        }),
    );
    drop(environment);

    // 4. Error paths the dialog shows inline.
    let reused = flow::pair_and_save(target, ClientInfo::default(), stores.clone())
        .await
        .err()
        .context("re-using a one-time code should fail")?;
    ensure!(
        reused == "The environment credential is invalid.",
        "reused code: {reused}"
    );
    let unreachable =
        flow::resolve_fields("http://127.0.0.1:9", "CODE").map_err(anyhow::Error::msg)?;
    let unreachable = flow::pair_and_save(unreachable, ClientInfo::default(), stores.clone())
        .await
        .err()
        .context("an unreachable host should fail")?;
    step(
        report,
        "errors",
        json!({"reusedCode": reused, "unreachableHost": unreachable}),
    );

    // 5. Disconnect forgets the environment.
    flow::forget(&saved.environment_id, &stores).map_err(anyhow::Error::msg)?;
    ensure!(
        stores.catalog.load()?.environments.is_empty(),
        "catalog not emptied"
    );
    ensure!(
        stores.secrets.get(&connection_id)?.is_none(),
        "token not deleted"
    );
    step(
        report,
        "forgotten",
        json!({"catalogEntries": 0, "tokenDeleted": true}),
    );
    Ok(())
}
