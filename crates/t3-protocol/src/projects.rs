//! Workspace files, search, editor launch, filesystem browsing, assets, and attachment uploads
//! (`project.ts`, `filesystem.ts`, `assets.ts`, `attachments.ts`; protocol.md 3.5 and 8.4).

use serde::{Deserialize, Serialize};

use crate::{
    ids::{AttachmentId, ThreadId},
    open_enum,
    server::EditorId,
};

open_enum! {
    pub enum EntryKind {
        File = "file",
        Directory = "directory",
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntry {
    pub path: String,
    pub kind: EntryKind,
    pub ignored: Option<bool>,
}

/// Result of `projects.listEntries` and `projects.searchEntries`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEntries {
    #[serde(default)]
    pub entries: Vec<ProjectEntry>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListEntriesInput {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory_path: Option<String>,
}

/// `projects.searchEntries` (`@` mentions, file picker).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSearchEntriesInput {
    pub cwd: String,
    pub query: String,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<EntryKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReadFileInput {
    pub cwd: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReadFileResult {
    pub relative_path: String,
    pub contents: String,
    #[serde(default)]
    pub byte_length: u64,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWriteFileInput {
    pub cwd: String,
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWriteFileResult {
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSearchContentsInput {
    pub cwd: String,
    pub query: String,
    pub limit: u32,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub use_regex: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSearchContentsResult {
    #[serde(default)]
    pub matches: Vec<ProjectContentMatch>,
    #[serde(default)]
    pub truncated: bool,
    pub regex_fallback_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContentMatch {
    pub path: String,
    pub line_number: u64,
    pub line_content: String,
    #[serde(default)]
    pub match_ranges: Vec<MatchRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct MatchRange {
    pub start: u64,
    pub end: u64,
}

/// `shell.openInEditor`.
#[derive(Debug, Clone, Serialize)]
pub struct LaunchEditorInput {
    pub cwd: String,
    pub editor: EditorId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reveal: Option<bool>,
}

/// `filesystem.browse` (add-project path picker).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemBrowseInput {
    pub partial_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemBrowseResult {
    pub parent_path: String,
    #[serde(default)]
    pub entries: Vec<FilesystemBrowseEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesystemBrowseEntry {
    pub name: String,
    pub full_path: String,
}

/// What `assets.createUrl` should serve.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_tag", rename_all = "kebab-case")]
pub enum AssetResource {
    #[serde(rename_all = "camelCase")]
    WorkspaceFile {
        thread_id: ThreadId,
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    MediaFile {
        thread_id: ThreadId,
        path: String,
    },
    DraftWorkspaceFile {
        cwd: String,
        path: String,
    },
    #[serde(rename_all = "camelCase")]
    Attachment {
        attachment_id: AttachmentId,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
        /// `inline` or `attachment`.
        #[serde(skip_serializing_if = "Option::is_none")]
        disposition: Option<String>,
    },
    ProjectFavicon {
        cwd: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    GithubMedia {
        cwd: String,
        url: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetCreateUrlInput {
    pub resource: AssetResource,
}

/// A capability URL. Resolve `relative_url` against the environment's HTTP base and `GET` it
/// without credentials. Valid about an hour (favicons 30 min).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetCreateUrlResult {
    pub relative_url: String,
    /// Epoch milliseconds.
    pub expires_at: u64,
    pub source_path: Option<String>,
    pub image_dimensions: Option<ImageDimensions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ImageDimensions {
    pub width: f64,
    pub height: f64,
}

/// `attachments.createUploadUrl` (needs `capabilities.attachment_uploads`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateUploadUrlInput {
    #[serde(rename = "type")]
    pub kind: crate::orchestration::AttachmentKind,
    pub name: String,
    /// Images: `image/png`, `image/jpeg`, `image/gif`, or `image/webp`.
    pub mime_type: String,
    pub size_bytes: u64,
}

/// `POST` the raw bytes to `relative_url` with `Content-Type: <mime>` within 10 minutes, then
/// reference `attachment_id` in `thread.turn.start`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentCreateUploadUrlResult {
    pub attachment_id: AttachmentId,
    pub relative_url: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentDeleteInput {
    pub attachment_id: AttachmentId,
}
