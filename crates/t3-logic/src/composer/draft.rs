//! Persisted composer drafts (`drafts.json`), in the shape of the web's
//! `t3code:composer-drafts:v1` store (`composerDraftStore.ts`).
//!
//! - `draftsByThreadKey`: composer content per target. The key is a draft id for a new thread
//!   or `<environmentId>:<threadId>` for a server thread.
//! - `draftThreadsByThreadKey`: draft sessions (threads that have not started), by draft id.
//! - `logicalProjectDraftThreadKeyByLogicalProjectKey`: the reusable draft per project.
//! - `stickyModelSelectionByProvider` / `stickyActiveProvider`: the last model picked anywhere,
//!   which seeds new drafts.
//!
//! Terminal contexts keep only their metadata, so they come back "expired" after a restart.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use t3_protocol::{
    EnvironmentId, ProjectId, ProviderInstanceId, ThreadId, orchestration::ModelSelection,
};

/// Version written to `drafts.json`. Unknown or unreadable files start empty.
pub const DRAFTS_VERSION: u32 = 1;

/// The whole file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DraftsFile {
    pub version: u32,
    pub drafts_by_thread_key: BTreeMap<String, ComposerDraft>,
    pub draft_threads_by_thread_key: BTreeMap<String, DraftThread>,
    pub logical_project_draft_thread_key_by_logical_project_key: BTreeMap<String, String>,
    pub sticky_model_selection_by_provider: BTreeMap<String, ModelSelection>,
    pub sticky_active_provider: Option<ProviderInstanceId>,
}

impl DraftsFile {
    /// Decodes `drafts.json`; anything unreadable yields an empty store.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Self>(json)
            .ok()
            .filter(|file| file.version == DRAFTS_VERSION)
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        let mut file = self.clone();
        file.version = DRAFTS_VERSION;
        file.drafts_by_thread_key
            .retain(|_, draft| !draft.is_empty());
        serde_json::to_string_pretty(&file).unwrap_or_default()
    }
}

/// Composer content for one target.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComposerDraft {
    pub prompt: String,
    pub attachments: Vec<DraftImage>,
    pub terminal_contexts: Vec<TerminalContextMeta>,
    /// Last model picked per instance in this composer.
    pub model_selection_by_provider: BTreeMap<String, ModelSelection>,
    /// The instance picked in this composer.
    pub active_provider: Option<ProviderInstanceId>,
}

impl ComposerDraft {
    /// Nothing worth keeping on disk.
    pub fn is_empty(&self) -> bool {
        self.prompt.is_empty()
            && self.attachments.is_empty()
            && self.terminal_contexts.is_empty()
            && self.model_selection_by_provider.is_empty()
            && self.active_provider.is_none()
    }

    /// Clears what a send consumes, keeping the model choice.
    pub fn clear_content(&mut self) {
        self.prompt.clear();
        self.attachments.clear();
        self.terminal_contexts.clear();
    }
}

/// An attached image, kept as a data URL so it survives a restart.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftImage {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub data_url: String,
}

/// A terminal selection referenced by a U+FFFC chip. `text` is never persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalContextMeta {
    pub id: String,
    pub thread_id: String,
    pub created_at: String,
    pub terminal_id: String,
    pub terminal_label: String,
    pub line_start: u32,
    pub line_end: u32,
}

impl TerminalContextMeta {
    /// "{label} line N" or "{label} lines A-B".
    pub fn label(&self) -> String {
        if self.line_start == self.line_end {
            format!("{} line {}", self.terminal_label, self.line_start)
        } else {
            format!(
                "{} lines {}-{}",
                self.terminal_label, self.line_start, self.line_end
            )
        }
    }
}

/// Workspace mode of a draft thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftEnvMode {
    #[default]
    Local,
    Worktree,
}

/// A thread that has not started yet. Its environment, workspace mode, and branch stay editable
/// until the first send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftThread {
    pub thread_id: ThreadId,
    pub environment_id: EnvironmentId,
    pub project_id: ProjectId,
    #[serde(default)]
    pub logical_project_key: String,
    pub created_at: String,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub env_mode: DraftEnvMode,
    #[serde(default)]
    pub start_from_origin: bool,
    /// The server thread this draft became once its first turn started.
    #[serde(default)]
    pub promoted_to: Option<PromotedThread>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromotedThread {
    pub environment_id: EnvironmentId,
    pub thread_id: ThreadId,
}
