//! Creating projects and workspaces: source control lookup/clone/publish, tracked project
//! clones, scratch and new projects, importing existing Claude/Codex sessions, worktree setup
//! progress, review file contents, and workflow scripts.

use serde::{Deserialize, Serialize};

use crate::{ProjectId, ThreadId, pull_requests::SourceControlProviderKind};

/// `sourceControl.lookupRepository`.
#[derive(Debug, Clone, Serialize)]
pub struct SourceControlRepositoryLookupInput {
    pub provider: SourceControlProviderKind,
    /// `owner/name`.
    pub repository: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlRepositoryInfo {
    pub provider: SourceControlProviderKind,
    pub name_with_owner: String,
    pub url: String,
    pub ssh_url: String,
}

/// Where to clone from: a hosted repository, or a remote URL. Shared by
/// `sourceControl.cloneRepository` and `projectClone.start`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<SourceControlProviderKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// `auto`, `ssh`, or `https`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

/// `sourceControl.cloneRepository`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlCloneRepositoryInput {
    #[serde(flatten)]
    pub source: CloneSource,
    pub destination_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlCloneRepositoryResult {
    pub cwd: String,
    pub remote_url: String,
    pub repository: Option<SourceControlRepositoryInfo>,
}

/// `sourceControl.publishRepository`: create a hosted repository for `cwd` and push.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlPublishRepositoryInput {
    pub cwd: String,
    pub provider: SourceControlProviderKind,
    pub repository: String,
    /// `private` or `public`.
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlPublishRepositoryResult {
    pub repository: SourceControlRepositoryInfo,
    pub remote_name: String,
    pub remote_url: String,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_branch: Option<String>,
    /// `pushed` or `remote_added`.
    pub status: String,
}

/// `projectClone.start`: create a project whose workspace is cloned in the background
/// (progress on `subscribeProjectClones`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloneStartInput {
    /// Client-chosen.
    pub project_id: ProjectId,
    pub title: String,
    pub created_at: String,
    #[serde(flatten)]
    pub source: CloneSource,
    pub destination_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloneStartResult {
    pub project_id: ProjectId,
    pub cwd: String,
    pub remote_url: String,
    pub repository: Option<SourceControlRepositoryInfo>,
}

/// `projectClone.cancel` / `projectClone.retry`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloneActionInput {
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCloneActionResult {
    #[serde(default)]
    pub applied: bool,
}

/// One tracked clone. `subscribeProjectClones` sends the full list on every change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloneSnapshot {
    pub project_id: ProjectId,
    pub remote_url: String,
    pub destination_path: String,
    pub repository: Option<SourceControlRepositoryInfo>,
    /// `running`, `done`, `failed`, or `cancelled`.
    pub phase: String,
    /// `connecting`, `counting`, `receiving`, `resolving`, or `checkout`.
    pub stage: String,
    pub percent: Option<f64>,
    pub detail: Option<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub sequence: u64,
}

/// `projects.ensureScratch`: the scratch project (created on first use).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectEnsureScratchResult {
    pub project_id: ProjectId,
}

/// `projects.createNew`: a new empty project under `ServerConfig.new_projects_root`.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectCreateNewInput {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateNewResult {
    pub project_id: ProjectId,
    pub workspace_root: String,
    /// The initial commit failed; the project still exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_error: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Importing agent sessions (onboarding)

/// `agentSessions.scan`: folders with Claude/Codex history that could become projects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionScanResult {
    #[serde(default)]
    pub candidates: Vec<AgentSessionProjectCandidate>,
    pub scanned_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionProjectCandidate {
    pub path: String,
    pub title: String,
    /// Set when a project already exists for `path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// `claudeAgent`, `codex`.
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub thread_count: u64,
    pub last_active_at: Option<String>,
    #[serde(default)]
    pub already_imported: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<AgentSessionProjectGit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionProjectGit {
    pub remote_key: Option<String>,
    pub repository: Option<String>,
}

/// `agentSessions.import`: import a project's sessions as threads. `expected_workspace_root`
/// guards against the project moving in between (`AgentSessionImportProjectChangedError`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionImportInput {
    pub project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_workspace_root: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionImportResult {
    pub imported_count: u64,
    pub skipped_count: u64,
}

// ---------------------------------------------------------------------------------------------
// Worktree setup

/// `subscribeWorktreeSetup` / `worktreeSetup.cancel`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupThreadInput {
    pub thread_id: ThreadId,
}

/// Progress of preparing a thread's worktree. `subscribeWorktreeSetup` items are
/// `Option<WorktreeSetupSnapshot>` (`None`: no setup for this thread).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupSnapshot {
    pub thread_id: ThreadId,
    /// `running`, `done`, `failed`, or `cancelled`.
    pub phase: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub branch: Option<String>,
    pub base_ref: Option<String>,
    pub worktree_path: Option<String>,
    pub setup_script: Option<WorktreeSetupScript>,
    #[serde(default)]
    pub stages: Vec<WorktreeSetupStage>,
    pub error: Option<String>,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupScript {
    pub name: String,
    pub command: String,
    pub terminal_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupStage {
    /// `fetch`, `checkout`, `submodules`, `setup-script`, or `agent`.
    pub id: String,
    /// `pending`, `running`, `done`, `skipped`, `warning`, or `failed`.
    pub status: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub percent: Option<f64>,
    pub detail: Option<String>,
    /// Last output lines.
    #[serde(default)]
    pub tail: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSetupCancelResult {
    #[serde(default)]
    pub cancelled: bool,
}

// ---------------------------------------------------------------------------------------------
// Review file contents and workflow scripts

/// `review.getDiffFileContents`: both sides of one file in a review diff.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffFileContentsInput {
    pub cwd: String,
    /// `working-tree` or `branch-range`.
    pub source_kind: String,
    pub change_type: crate::pull_requests::DiffChangeType,
    pub base_ref: Option<String>,
    pub head_ref: Option<String>,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentsPair {
    #[serde(default)]
    pub old_contents: String,
    #[serde(default)]
    pub new_contents: String,
}

/// `orchestration.getWorkflowScript`: a workflow script a subagent ran.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWorkflowScriptInput {
    pub thread_id: ThreadId,
    pub script_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowScript {
    pub script_path: String,
    pub contents: String,
    #[serde(default)]
    pub truncated: bool,
}
