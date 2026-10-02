//! VCS status, refs, worktrees, git actions, and review diffs (`git.ts`, `vcs.ts`, `review.ts`,
//! protocol.md 8.3).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ids::ThreadId, open_enum, orchestration::PullRequestState};

/// Payload of `subscribeVcsStatus` and `vcs.refreshStatus`; also `vcs.pull`.
#[derive(Debug, Clone, Serialize)]
pub struct VcsCwdInput {
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceControlProviderInfo {
    pub kind: String,
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingTreeFile {
    pub path: String,
    #[serde(default)]
    pub insertions: u64,
    #[serde(default)]
    pub deletions: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingTree {
    #[serde(default)]
    pub files: Vec<WorkingTreeFile>,
    #[serde(default)]
    pub insertions: u64,
    #[serde(default)]
    pub deletions: u64,
}

/// The local half of VCS status.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsStatusLocal {
    pub is_repo: bool,
    pub source_control_provider: Option<SourceControlProviderInfo>,
    #[serde(default)]
    pub has_primary_remote: bool,
    #[serde(default)]
    pub is_default_ref: bool,
    pub ref_name: Option<String>,
    #[serde(default)]
    pub has_working_tree_changes: bool,
    #[serde(default)]
    pub working_tree: WorkingTree,
}

/// The remote half of VCS status.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsStatusRemote {
    #[serde(default)]
    pub has_upstream: bool,
    #[serde(default)]
    pub ahead_count: u64,
    #[serde(default)]
    pub behind_count: u64,
    pub ahead_of_default_count: Option<u64>,
    pub pr: Option<VcsPullRequestSummary>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsPullRequestSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub base_ref: String,
    pub head_ref: String,
    pub state: PullRequestState,
    pub is_draft: Option<bool>,
    pub updated_at: Option<String>,
}

/// Result of `vcs.refreshStatus`: local and remote merged into one object.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VcsStatusResult {
    #[serde(flatten)]
    pub local: VcsStatusLocal,
    #[serde(flatten)]
    pub remote: VcsStatusRemote,
}

/// An item of `subscribeVcsStatus`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "_tag", rename_all = "camelCase")]
pub enum VcsStatusStreamEvent {
    Snapshot {
        local: VcsStatusLocal,
        remote: Option<VcsStatusRemote>,
    },
    LocalUpdated {
        local: VcsStatusLocal,
    },
    RemoteUpdated {
        remote: Option<VcsStatusRemote>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsPullResult {
    /// `pulled` or `skipped_up_to_date`.
    pub status: String,
    pub ref_name: String,
    pub upstream_ref: Option<String>,
}

open_enum! {
    pub enum RefKind {
        All = "all",
        Local = "local",
        Remote = "remote",
    }
}

/// `vcs.listRefs` (branch picker).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsListRefsInput {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_matching_remote_refs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_kind: Option<RefKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsListRefsResult {
    #[serde(default)]
    pub refs: Vec<VcsRef>,
    #[serde(default)]
    pub is_repo: bool,
    #[serde(default)]
    pub has_primary_remote: bool,
    pub next_cursor: Option<u64>,
    #[serde(default)]
    pub total_count: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsRef {
    pub name: String,
    pub is_remote: Option<bool>,
    pub remote_name: Option<String>,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub is_default: bool,
    pub worktree_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsCreateWorktreeInput {
    pub cwd: String,
    pub ref_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_ref_name: Option<String>,
    /// `None` lets the server choose.
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VcsCreateWorktreeResult {
    pub worktree: VcsWorktree,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsWorktree {
    pub path: String,
    pub ref_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsRemoveWorktreeInput {
    pub cwd: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsCreateRefInput {
    pub cwd: String,
    pub ref_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch_ref: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsCreateRefResult {
    pub ref_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsSwitchRefInput {
    pub cwd: String,
    pub ref_name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VcsSwitchRefResult {
    pub ref_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VcsInitInput {
    pub cwd: String,
    /// `git` or `jj`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

open_enum! {
    pub enum GitStackedAction {
        Commit = "commit",
        Push = "push",
        CreatePr = "create_pr",
        CommitPush = "commit_push",
        CommitPushPr = "commit_push_pr",
    }
}

/// `git.runStackedAction` (stream of [`GitActionProgressEvent`]).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRunStackedActionInput {
    /// Client-chosen id echoed on every progress event.
    pub action_id: String,
    pub cwd: String,
    pub action: GitStackedAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_branch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
}

/// Progress of a stacked git action. The final item is `ActionFinished` or `ActionFailed`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitActionProgressEvent {
    ActionStarted {
        #[serde(default)]
        phases: Vec<String>,
    },
    PhaseStarted {
        phase: String,
        label: String,
    },
    #[serde(rename_all = "camelCase")]
    HookStarted {
        hook_name: String,
    },
    #[serde(rename_all = "camelCase")]
    HookOutput {
        hook_name: Option<String>,
        stream: String,
        text: String,
    },
    #[serde(rename_all = "camelCase")]
    HookFinished {
        hook_name: String,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
    },
    /// `result` is `GitRunStackedActionResult` (branch/commit/push/pr outcomes plus a toast).
    ActionFinished {
        result: Value,
    },
    ActionFailed {
        phase: Option<String>,
        message: String,
    },
    #[serde(other)]
    Unknown,
}

/// `git.resolvePullRequest`.
#[derive(Debug, Clone, Serialize)]
pub struct GitPullRequestRefInput {
    pub cwd: String,
    /// A number, URL, or branch.
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPullRequest {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub base_branch: String,
    pub head_branch: String,
    pub state: PullRequestState,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitResolvePullRequestResult {
    pub pull_request: GitPullRequest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPreparePullRequestThreadInput {
    pub cwd: String,
    pub reference: String,
    /// `local` or `worktree`.
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPreparePullRequestThreadResult {
    pub pull_request: GitPullRequest,
    pub branch: String,
    pub worktree_path: Option<String>,
    #[serde(default)]
    pub is_on_pull_request_head: bool,
}

/// `review.getDiffPreview` (needs `review:write`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffPreviewInput {
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_whitespace: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<ReviewDiffFileRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffFileRef {
    pub path: String,
    pub previous_path: Option<String>,
    /// `working-tree` or `branch-range`.
    pub source_kind: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffPreviewResult {
    pub cwd: String,
    pub generated_at: String,
    #[serde(default)]
    pub sources: Vec<ReviewDiffPreviewSource>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffPreviewSource {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub base_ref: Option<String>,
    pub head_ref: Option<String>,
    pub diff: String,
    pub diff_hash: String,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub files: Vec<ReviewDiffFileStat>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDiffFileStat {
    pub path: String,
    pub previous_path: Option<String>,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
}
