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
