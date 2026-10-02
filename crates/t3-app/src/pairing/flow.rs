//! The add-environment flow without UI (fork `ConnectionsSettings.tsx` `handleAddSavedBackend`,
//! connections.md 2.2, 2.3, 6.3.1): read the dialog's Host and Pairing code fields, pair with the
//! server, and save it.
//!
//! GPUI-free on purpose. `crates/t3-client/examples/pair_flow.rs` compiles this exact file to run
//! the dialog's logic end to end against a live `npx t3 serve`, since GPUI windows can't run on
//! the Linux dev hosts.
//!
//! ```ignore
//! let target = resolve_fields(&host, &code)?;                    // "Enter a pairing code."
//! let added = runtime::spawn(pair_and_save(target, ClientInfo::default(), stores)).await?;
//! let client = t3_client::Environment::start(EnvironmentOptions::from_saved(&added.saved, added.endpoint));
//! ```

use std::sync::Arc;

use t3_client::{
    ClientInfo, Endpoint, pair,
    pairing::{PairingTarget, parse_pairing_text, resolve_host_and_code},
    store::{CatalogStore, SavedEnvironment, SecretStore},
};

/// Host and code read out of a pasted pairing link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitPairing {
    /// Origin of the backend, e.g. `https://box.tail1.ts.net` (no trailing slash).
    pub host: String,
    pub code: String,
}

/// When the Host field holds a whole pairing link (or what `t3 serve` / `t3 pair` print), the
/// host and code inside it, so the dialog fills both fields (fork `parsePairingUrlFields`).
/// `None` for a plain host like `backend.example.com`.
pub fn split_pairing_input(input: &str) -> Option<SplitPairing> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    // The fork prefixes `https://` before parsing a scheme-less link.
    let target = parse_pairing_text(input).ok().or_else(|| {
        (!input.contains("://") && input.contains("token="))
            .then(|| parse_pairing_text(&format!("https://{input}")).ok())
            .flatten()
    })?;
    Some(SplitPairing {
        host: target.http_base.as_str().trim_end_matches('/').to_owned(),
        code: target.credential,
    })
}

/// Resolves the two fields into a pairing target (fork `parseRemotePairingFields`). A pairing
/// link in Host wins over the code field. Errors are the user-facing copy.
pub fn resolve_fields(host: &str, code: &str) -> Result<PairingTarget, String> {
    let (host, code) = match split_pairing_input(host) {
        Some(split) => (split.host, split.code),
        None => (host.trim().to_owned(), code.trim().to_owned()),
    };
    if host.is_empty() {
        return Err("Enter a backend host.".into());
    }
    if code.is_empty() {
        return Err("Enter a pairing code.".into());
    }
    resolve_host_and_code(&host, &code).map_err(|error| error.to_string())
}

/// Where a paired environment is saved: the catalog (`environments.json`) and the secret store
/// for its bearer token.
#[derive(Clone)]
pub struct PairingStores {
    pub catalog: CatalogStore,
    pub secrets: Arc<dyn SecretStore>,
}

/// A paired and saved environment, ready for `t3_client::Environment::start`.
pub struct AddedEnvironment {
    /// The catalog entry as stored (re-pairing keeps the entry's `enabled` flag).
    pub saved: SavedEnvironment,
    pub endpoint: Arc<dyn Endpoint>,
}

/// Pairs with `target` (descriptor and protocol check, then the one-time code exchange) and
/// saves the result: the bearer token first, then the catalog entry, so an entry never exists
/// without its token. Network and blocking file IO: run it on the networking runtime.
///
/// Errors are the user-facing copy, e.g. "The environment credential is invalid." for a used or
/// expired code.
pub async fn pair_and_save(
    target: PairingTarget,
    client: ClientInfo,
    stores: PairingStores,
) -> Result<AddedEnvironment, String> {
    let paired = pair(&target, &client)
        .await
        .map_err(|failure| failure.detail)?;
    let save_failed = |error: t3_client::store::StoreError| {
        tracing::warn!("failed to save paired environment: {error}");
        format!("Could not save the environment: {error}")
    };
    stores
        .secrets
        .set(&paired.secret_key(), &paired.bearer_token)
        .map_err(save_failed)?;
    let mut catalog = stores.catalog.load().map_err(save_failed)?;
    catalog.upsert(paired.saved());
    stores.catalog.save(&catalog).map_err(save_failed)?;
    let saved = catalog
        .get(&paired.descriptor.environment_id)
        .cloned()
        .unwrap_or_else(|| paired.saved());
    Ok(AddedEnvironment {
        saved,
        endpoint: paired.endpoint(),
    })
}

/// Forgets a saved environment: drops its catalog entry and bearer token (fork "Disconnect",
/// upstream `registry.remove`). Blocking file IO.
pub fn forget(
    environment_id: &t3_protocol::EnvironmentId,
    stores: &PairingStores,
) -> Result<(), String> {
    let mut catalog = stores.catalog.load().map_err(|error| error.to_string())?;
    let removed = catalog.remove(environment_id);
    stores
        .catalog
        .save(&catalog)
        .map_err(|error| error.to_string())?;
    if let Some(removed) = removed
        && let t3_client::store::SavedTarget::Known(t3_client::store::KnownTarget::Bearer {
            connection_id,
            ..
        }) = &removed.target
    {
        stores
            .secrets
            .delete(connection_id)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Failure modes:
    //! 1. A pasted pairing URL in Host is not split, so pairing uses the URL as a host.
    //! 2. A plain host is mistaken for a link and the user's code field is dropped.
    //! 3. Empty fields give the wrong message, or the code is checked before the host.
    //! 4. The `t3 serve` output pasted whole is not recognized.
    use super::*;

    #[test]
    fn splits_pairing_links_in_host() {
        let split = split_pairing_input("http://127.0.0.1:4760/pair#token=ABCD").unwrap();
        assert_eq!(split.host, "http://127.0.0.1:4760");
        assert_eq!(split.code, "ABCD");
        let split = split_pairing_input("box.tail1.ts.net/pair#token=XY").unwrap();
        assert_eq!(split.host, "https://box.tail1.ts.net");
        let serve = "T3 Code server is ready.\nConnection string: http://localhost:3773\nToken: Q1\nPairing URL: http://localhost:3773/pair#token=Q1";
        assert_eq!(split_pairing_input(serve).unwrap().code, "Q1");
        assert_eq!(split_pairing_input("backend.example.com"), None);
        assert_eq!(split_pairing_input("http://localhost:3773"), None);
    }

    #[test]
    fn validates_fields_in_order() {
        assert_eq!(
            resolve_fields(" ", "").unwrap_err(),
            "Enter a backend host."
        );
        assert_eq!(
            resolve_fields("box", " ").unwrap_err(),
            "Enter a pairing code."
        );
        assert_eq!(
            resolve_fields("ftp://box", "C").unwrap_err(),
            "Backend URL is invalid."
        );
        let target = resolve_fields("http://127.0.0.1:4760/pair#token=T", "ignored").unwrap();
        assert_eq!(target.credential, "T");
        let target = resolve_fields("studio.ts.net", "CODE").unwrap();
        assert_eq!(target.http_base.as_str(), "https://studio.ts.net/");
    }
}
