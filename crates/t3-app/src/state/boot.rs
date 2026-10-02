//! Startup: connects every saved environment from `environments.json`.

use gpui_kit::{App, AppContext as _, Entity};
use t3_client::{
    Endpoint, EnvironmentOptions, saved_bearer_endpoint,
    store::{CatalogStore, FileSecretStore},
};

use super::{AppState, Environment, EnvironmentKind};

/// Starts each saved, enabled environment that has credentials and adds it to `app_state`.
/// Loopback servers are local and come first, so the first one is the primary environment.
/// Entries without a usable endpoint (missing token, relay targets) are skipped; pairing and
/// T3 Connect add them later.
pub fn start_saved_environments(app_state: &Entity<AppState>, cx: &mut App) {
    let catalog = match CatalogStore::new().load() {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::warn!("failed to load saved environments: {error}");
            return;
        }
    };
    let secrets = FileSecretStore::new();
    let mut entries: Vec<(EnvironmentKind, EnvironmentOptions)> = catalog
        .environments
        .iter()
        .filter_map(|saved| match saved_bearer_endpoint(saved, &secrets) {
            Ok(Some(endpoint)) => Some((
                kind_of(endpoint.as_ref()),
                EnvironmentOptions::from_saved(saved, endpoint),
            )),
            Ok(None) => None,
            Err(error) => {
                tracing::warn!("no credentials for {}: {error}", saved.label);
                None
            }
        })
        .collect();
    entries.sort_by_key(|(kind, _)| *kind != EnvironmentKind::Local);
    for (kind, options) in entries {
        let client = t3_client::Environment::start(options);
        let environment = cx.new(|cx| Environment::connected(client, kind, cx));
        app_state.update(cx, |state, cx| state.add_environment(environment, cx));
    }
}

fn kind_of(endpoint: &dyn Endpoint) -> EnvironmentKind {
    let loopback = endpoint.display_url().is_some_and(|url| {
        matches!(
            url.host_str(),
            Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
        )
    });
    if loopback {
        EnvironmentKind::Local
    } else {
        EnvironmentKind::Remote
    }
}
