//! Saved environments and secrets on disk (connections.md 7.1, 7.2).
//!
//! - `environments.json`: the non-secret catalog, written atomically (temp file + rename).
//! - [`SecretStore`]: bearer tokens and other secrets. [`FileSecretStore`] keeps them in a 0600
//!   JSON file; a Keychain store can replace it later (ad-hoc-signed dev builds would prompt on
//!   every rebuild).
//!
//! Both live in [`data_dir`]: `~/Library/Application Support/T3UI` on macOS, overridable with
//! `T3UI_DATA_DIR` (tests, isolated end-to-end runs).

use std::{
    collections::BTreeMap,
    io::Write as _,
    path::{Path, PathBuf},
};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use t3_protocol::EnvironmentId;
use url::Url;

/// Where T3UI keeps its files. Created on first write.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("T3UI_DATA_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("", "", "T3UI")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".t3ui"))
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

/// The persisted list of environments. Unknown top-level keys (e.g. `cloud`) round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentCatalog {
    pub schema_version: u32,
    #[serde(default)]
    pub environments: Vec<SavedEnvironment>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Default for EnvironmentCatalog {
    fn default() -> Self {
        EnvironmentCatalog {
            schema_version: 1,
            environments: Vec::new(),
            other: Map::new(),
        }
    }
}

impl EnvironmentCatalog {
    pub fn get(&self, id: &EnvironmentId) -> Option<&SavedEnvironment> {
        self.environments.iter().find(|e| &e.environment_id == id)
    }

    /// Adds or replaces the entry for `entry.environment_id`. Re-adding keeps the existing
    /// `enabled` flag and position (upstream `storageDocument.ts:74-105`).
    pub fn upsert(&mut self, mut entry: SavedEnvironment) {
        match self
            .environments
            .iter_mut()
            .find(|e| e.environment_id == entry.environment_id)
        {
            Some(existing) => {
                entry.enabled = existing.enabled;
                entry.added_at = existing.added_at.clone();
                *existing = entry;
            }
            None => self.environments.push(entry),
        }
    }

    /// Removes an entry. The caller also deletes its secret.
    pub fn remove(&mut self, id: &EnvironmentId) -> Option<SavedEnvironment> {
        let index = self
            .environments
            .iter()
            .position(|e| &e.environment_id == id)?;
        Some(self.environments.remove(index))
    }
}

/// One saved environment. Unknown fields round-trip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEnvironment {
    pub environment_id: EnvironmentId,
    /// The descriptor label at registration.
    pub label: String,
    /// `false`: saved but never connects.
    pub enabled: bool,
    pub added_at: String,
    /// Set when a connection was blocked as unsupported; the entry stays disabled until then.
    #[serde(default)]
    pub unsupported_reason: Option<String>,
    pub target: SavedTarget,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl SavedEnvironment {
    pub fn new(environment_id: EnvironmentId, label: String, target: SavedTarget) -> Self {
        SavedEnvironment {
            environment_id,
            label,
            enabled: true,
            added_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            unsupported_reason: None,
            target,
            other: Map::new(),
        }
    }
}

/// How a saved environment is reached. Kinds this build does not know are kept verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedTarget {
    Known(KnownTarget),
    Unknown(Value),
}

/// The known target kinds, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KnownTarget {
    /// Paired directly. The token is in the secret store under `connection_id`.
    #[serde(rename_all = "camelCase")]
    Bearer {
        connection_id: String,
        http_base_url: Url,
        ws_base_url: Url,
    },
    /// Linked through T3 Connect by the account `account_id`.
    #[serde(rename_all = "camelCase")]
    Relay { account_id: String },
}

/// Reads and writes `environments.json`.
#[derive(Debug, Clone)]
pub struct CatalogStore {
    path: PathBuf,
}

impl CatalogStore {
    /// The catalog at `<data_dir>/environments.json`.
    pub fn new() -> Self {
        Self::at(data_dir().join("environments.json"))
    }

    pub fn at(path: PathBuf) -> Self {
        CatalogStore { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the catalog; a missing file is an empty catalog. Small file, fine on the main
    /// thread at startup.
    pub fn load(&self) -> Result<EnvironmentCatalog, StoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| StoreError::Json {
                path: self.path.clone(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(EnvironmentCatalog::default())
            }
            Err(source) => Err(StoreError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }

    pub fn save(&self, catalog: &EnvironmentCatalog) -> Result<(), StoreError> {
        let json = serde_json::to_vec_pretty(catalog).map_err(|source| StoreError::Json {
            path: self.path.clone(),
            source,
        })?;
        write_atomic(&self.path, &json, false)
    }
}

impl Default for CatalogStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Secret storage keyed by name, e.g. `bearer:<environmentId>`.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, StoreError>;
    fn set(&self, key: &str, value: &str) -> Result<(), StoreError>;
    fn delete(&self, key: &str) -> Result<(), StoreError>;
}

/// Secrets in one JSON object file with mode 0600 (`<data_dir>/secrets.json`).
pub struct FileSecretStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileSecretStore {
    pub fn new() -> Self {
        Self::at(data_dir().join("secrets.json"))
    }

    pub fn at(path: PathBuf) -> Self {
        FileSecretStore {
            path,
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<BTreeMap<String, String>, StoreError> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|source| StoreError::Json {
                path: self.path.clone(),
                source,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(source) => Err(StoreError::Io {
                path: self.path.clone(),
                source,
            }),
        }
    }

    fn write(&self, secrets: &BTreeMap<String, String>) -> Result<(), StoreError> {
        let json = serde_json::to_vec_pretty(secrets).map_err(|source| StoreError::Json {
            path: self.path.clone(),
            source,
        })?;
        write_atomic(&self.path, &json, true)
    }
}

impl Default for FileSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
        let _guard = self.lock.lock();
        Ok(self.read()?.remove(key))
    }

    fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
        let _guard = self.lock.lock();
        let mut secrets = self.read()?;
        secrets.insert(key.to_owned(), value.to_owned());
        self.write(&secrets)
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        let _guard = self.lock.lock();
        let mut secrets = self.read()?;
        if secrets.remove(key).is_some() {
            self.write(&secrets)?;
        }
        Ok(())
    }
}

/// Which [`SecretStore`] to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretBackend {
    /// `<data_dir>/secrets.json`, mode 0600.
    #[default]
    File,
    /// The macOS login Keychain. Falls back to the file elsewhere.
    Keychain,
}

impl SecretBackend {
    /// `T3UI_SECRET_STORE=keychain` opts into the Keychain; anything else is the file store.
    pub fn from_env() -> Self {
        match std::env::var("T3UI_SECRET_STORE").as_deref() {
            Ok("keychain") => SecretBackend::Keychain,
            _ => SecretBackend::File,
        }
    }
}

/// Opens the secret store for `backend`. Switching backends does not move existing secrets:
/// environments paired under the other backend have to pair again.
pub fn open_secret_store(backend: SecretBackend) -> std::sync::Arc<dyn SecretStore> {
    match backend {
        #[cfg(target_os = "macos")]
        SecretBackend::Keychain => std::sync::Arc::new(KeychainSecretStore::new()),
        #[cfg(not(target_os = "macos"))]
        SecretBackend::Keychain => {
            tracing::warn!("the keychain secret store is macOS-only; using the file store");
            std::sync::Arc::new(FileSecretStore::new())
        }
        SecretBackend::File => std::sync::Arc::new(FileSecretStore::new()),
    }
}

#[cfg(target_os = "macos")]
pub use keychain::KeychainSecretStore;

#[cfg(target_os = "macos")]
mod keychain {
    use std::collections::BTreeMap;

    use parking_lot::Mutex;
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    };

    use super::{SecretStore, StoreError};

    /// The app's bundle id (`script/bundle-macos.sh`).
    const SERVICE: &str = "com.aadijo.t3ui";
    const ACCOUNT: &str = "secrets-v1";
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

    /// Every secret in one generic-password item holding a JSON object (connections.md 7.2),
    /// read once and cached. One item means at most one access prompt per launch, instead of
    /// one per environment.
    pub struct KeychainSecretStore {
        service: String,
        account: String,
        cache: Mutex<Option<BTreeMap<String, String>>>,
    }

    impl KeychainSecretStore {
        pub fn new() -> Self {
            Self::with_item(SERVICE, ACCOUNT)
        }

        /// A store backed by a specific item (tests, side-by-side builds).
        pub fn with_item(service: &str, account: &str) -> Self {
            KeychainSecretStore {
                service: service.to_owned(),
                account: account.to_owned(),
                cache: Mutex::new(None),
            }
        }

        fn lookup(&self) -> PasswordOptions {
            PasswordOptions::new_generic_password(&self.service, &self.account)
        }

        fn read(&self) -> Result<BTreeMap<String, String>, StoreError> {
            match generic_password(self.lookup()) {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .map_err(|e| StoreError::Keychain(format!("corrupt secrets item: {e}"))),
                Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(BTreeMap::new()),
                Err(error) => Err(StoreError::Keychain(error.to_string())),
            }
        }

        fn write(&self, secrets: &BTreeMap<String, String>) -> Result<(), StoreError> {
            if secrets.is_empty() {
                return match delete_generic_password_options(self.lookup()) {
                    Ok(()) => Ok(()),
                    Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
                    Err(error) => Err(StoreError::Keychain(error.to_string())),
                };
            }
            let json =
                serde_json::to_vec(secrets).map_err(|e| StoreError::Keychain(e.to_string()))?;
            let mut options = self.lookup();
            options.set_label("T3UI");
            options.set_description("T3UI environment credentials");
            set_generic_password_options(&json, options)
                .map_err(|error| StoreError::Keychain(error.to_string()))
        }

        /// Runs `f` on the cached secrets, loading them from the Keychain on first use.
        fn with_cache<T>(
            &self,
            f: impl FnOnce(&mut BTreeMap<String, String>) -> Result<T, StoreError>,
        ) -> Result<T, StoreError> {
            let mut cache = self.cache.lock();
            if cache.is_none() {
                *cache = Some(self.read()?);
            }
            f(cache.as_mut().expect("loaded above"))
        }
    }

    impl Default for KeychainSecretStore {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SecretStore for KeychainSecretStore {
        fn get(&self, key: &str) -> Result<Option<String>, StoreError> {
            self.with_cache(|secrets| Ok(secrets.get(key).cloned()))
        }

        fn set(&self, key: &str, value: &str) -> Result<(), StoreError> {
            self.with_cache(|secrets| {
                let mut next = secrets.clone();
                next.insert(key.to_owned(), value.to_owned());
                self.write(&next)?;
                *secrets = next;
                Ok(())
            })
        }

        fn delete(&self, key: &str) -> Result<(), StoreError> {
            self.with_cache(|secrets| {
                if !secrets.contains_key(key) {
                    return Ok(());
                }
                let mut next = secrets.clone();
                next.remove(key);
                self.write(&next)?;
                *secrets = next;
                Ok(())
            })
        }
    }
}

/// Writes `bytes` to a temp file next to `path`, then renames it over `path`.
fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> Result<(), StoreError> {
    let io = |source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io)?;
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = private;
    let mut file = options.open(&temp).map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);
    std::fs::rename(&temp, path).map_err(io)
}
