//! Where client state files live: `t3_client::store::data_dir()` (`~/Library/Application
//! Support/T3UI/` on macOS, overridable with `T3UI_DATA_DIR`), next to `environments.json`.
//!
//! Reads happen synchronously at startup (small JSON only); writes go to the background executor
//! and replace the file atomically.

use std::path::PathBuf;

use gpui_kit::{App, AppContext as _, Task};

/// File storage for persisted client state. [`Store::Memory`] keeps nothing, for snapshot scenes
/// and tests.
#[derive(Clone, Debug)]
pub enum Store {
    Disk(PathBuf),
    Memory,
}

impl Store {
    /// The app's data directory (shared with the environment catalog).
    pub fn user_data() -> Self {
        Self::Disk(t3_client::store::data_dir())
    }

    /// Reads `name`, or `None` if it does not exist or is unreadable.
    pub fn read(&self, name: &str) -> Option<String> {
        match self {
            Self::Disk(dir) => std::fs::read_to_string(dir.join(name)).ok(),
            Self::Memory => None,
        }
    }

    /// Writes `name` on the background executor (temp file + rename).
    pub fn write(&self, name: &'static str, contents: String, cx: &App) -> Task<()> {
        let Self::Disk(dir) = self.clone() else {
            return Task::ready(());
        };
        cx.background_spawn(async move {
            if let Err(error) = write_atomic(&dir, name, &contents) {
                tracing::warn!("failed to write {name}: {error}");
            }
        })
    }

    /// Writes `name` on the calling thread. Only for flushing at quit.
    pub fn write_now(&self, name: &str, contents: &str) {
        if let Self::Disk(dir) = self
            && let Err(error) = write_atomic(dir, name, contents)
        {
            tracing::warn!("failed to write {name}: {error}");
        }
    }
}

fn write_atomic(dir: &std::path::Path, name: &str, contents: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let temp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&temp, contents)?;
    std::fs::rename(temp, dir.join(name))
}
