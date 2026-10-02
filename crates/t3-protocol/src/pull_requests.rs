//! Pull request browsing, review, and actions (`packages/contracts/src/pullRequest.ts`, the
//! fork's `/pull-requests` route). Every method takes a [`PullRequestRef`] (most inputs embed it)
//! and fails with `PullRequestUnavailableError` (provider CLI missing or signed out),
//! `PullRequestOperationError`, or `EnvironmentAuthorizationError`.
//!
//! Responses are `Serialize` too, so views can keep JSON fixtures of them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    ProjectId, ThreadId, open_enum,
    orchestration::{PullRequestActor, PullRequestState},
};

open_enum! {
    /// Source control host family.
    pub enum SourceControlProviderKind {
        Github = "github",
        Gitlab = "gitlab",
        Forgejo = "forgejo",
        AzureDevops = "azure-devops",
        Bitbucket = "bitbucket",
        Unknown = "unknown",
    }
}

open_enum! {
    pub enum ReviewDecision {
        Approved = "approved",
        ChangesRequested = "changes-requested",
        ReviewRequired = "review-required",
    }
}

open_enum! {
    pub enum ChecksState {
        Passing = "passing",
        Failing = "failing",
        Pending = "pending",
    }
}

open_enum! {
    pub enum Mergeability {
        Mergeable = "mergeable",
        Conflicting = "conflicting",
        Unknown = "unknown",
    }
}

open_enum! {
    pub enum MergeMethod {
        Merge = "merge",
        Squash = "squash",
        Rebase = "rebase",
    }
}

open_enum! {
    /// How `update-branch` brings the head up to date.
    pub enum UpdateMethod {
        Merge = "merge",
        Rebase = "rebase",
    }
}

open_enum! {
    pub enum PullRequestAction {
        Merge = "merge",
        Ready = "ready",
        Draft = "draft",
        Close = "close",
        Reopen = "reopen",
        UpdateBranch = "update-branch",
        EnableAutoMerge = "enable-auto-merge",
        DisableAutoMerge = "disable-auto-merge",
        Revert = "revert",
        ApproveWorkflows = "approve-workflows",
    }
}

open_enum! {
    pub enum ReviewVerdict {
        Comment = "comment",
        Approve = "approve",
        RequestChanges = "request-changes",
    }
}

open_enum! {
    pub enum ReactionContent {
        ThumbsUp = "thumbs-up",
        ThumbsDown = "thumbs-down",
        Laugh = "laugh",
        Hooray = "hooray",
        Confused = "confused",
        Heart = "heart",
        Rocket = "rocket",
        Eyes = "eyes",
    }
}

open_enum! {
    pub enum CheckStatus {
        Pending = "pending",
        ActionRequired = "action-required",
        Success = "success",
        Failure = "failure",
        Skipped = "skipped",
        Neutral = "neutral",
        Cancelled = "cancelled",
    }
}

open_enum! {
    pub enum DiffSide {
        Left = "left",
        Right = "right",
    }
}

open_enum! {
    pub enum ReviewerKind {
        User = "user",
        Team = "team",
    }
}

open_enum! {
    pub enum CommentKind {
        IssueComment = "issue-comment",
        ReviewComment = "review-comment",
        Review = "review",
    }
}

open_enum! {
    pub enum FileViewedState {
        Unviewed = "unviewed",
        Viewed = "viewed",
        Dismissed = "dismissed",
    }
}

open_enum! {
    pub enum DiffChangeType {
        Change = "change",
        RenamePure = "rename-pure",
        RenameChanged = "rename-changed",
        New = "new",
        Deleted = "deleted",
    }
}

open_enum! {
    pub enum ListState {
        All = "all",
        Open = "open",
        Closed = "closed",
        Merged = "merged",
    }
}

open_enum! {
    pub enum Involvement {
        All = "all",
        Reviewing = "reviewing",
        Authored = "authored",
    }
}

// ---------------------------------------------------------------------------------------------
// Reference and list

/// Addresses one pull request. Most inputs flatten this in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRef {
    pub project_id: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Fail instead of answering for a different signed-in account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_account_id: Option<String>,
    /// Accept a cached answer while the host is unreachable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_stale: Option<bool>,
    pub repository: String,
    pub number: u64,
}

impl PullRequestRef {
    pub fn new(project_id: ProjectId, repository: impl Into<String>, number: u64) -> Self {
        PullRequestRef {
            project_id,
            host: None,
            expected_account_id: None,
            allow_stale: None,
            repository: repository.into(),
            number,
        }
    }
}

/// `pullRequests.list`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListInput {
    pub state: ListState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub involvement: Option<Involvement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<PullRequestListFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_ids: Option<Vec<ProjectId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Per-host pagination cursors from the previous page's `next_cursors`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursors: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

impl PullRequestListInput {
    pub fn new(state: ListState) -> Self {
        PullRequestListInput {
            state,
            involvement: None,
            filters: None,
            project_id: None,
            project_ids: None,
            host: None,
            limit: None,
            cursors: None,
            query: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListFilters {
    /// `only` or `hide`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// A review decision, or `none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<ChecksState>,
    /// OR of ANDs: each inner list must all apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excluded_labels: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListResult {
    /// Viewer login per host.
    #[serde(default)]
    pub viewers: BTreeMap<String, String>,
    #[serde(default)]
    pub providers: Vec<PullRequestProviderSummary>,
    #[serde(default)]
    pub entries: Vec<PullRequestListEntry>,
    /// Projects whose listing failed (shown inline, the rest still render).
    #[serde(default)]
    pub errors: Vec<PullRequestListProjectError>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub next_cursors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestProviderSummary {
    pub host: String,
    pub kind: SourceControlProviderKind,
    #[serde(default)]
    pub searches_on_host: bool,
    #[serde(default)]
    pub project_count: u32,
    #[serde(default)]
    pub configured: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListEntry {
    pub stack: Option<PullRequestStackMembership>,
    pub provider: SourceControlProviderKind,
    pub host: String,
    pub project_id: ProjectId,
    pub project_title: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<PullRequestActor>,
    pub head_branch: String,
    pub base_branch: String,
    pub state: PullRequestState,
    #[serde(default)]
    pub is_draft: bool,
    pub mergeability: Mergeability,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    pub created_at: String,
    pub updated_at: String,
    pub observed_at: Option<f64>,
    #[serde(default)]
    pub viewer_review_requested: bool,
    #[serde(default)]
    pub labels: Vec<PullRequestLabel>,
    pub review_decision: Option<ReviewDecision>,
    pub checks_state: Option<ChecksState>,
}

/// Where a pull request sits in a stack (1-based `position` of `size`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestStackMembership {
    pub number: u64,
    pub position: u32,
    pub size: u32,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListProjectError {
    pub project_id: ProjectId,
    pub project_title: String,
    pub message: String,
}

/// `pullRequests.listStats`: diff stats for list rows, fetched lazily.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestListStatsInput {
    pub refs: Vec<PullRequestRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestListStatsResult {
    #[serde(default)]
    pub stats: Vec<PullRequestDiffStat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDiffStat {
    pub project_id: ProjectId,
    pub repository: String,
    pub number: u64,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestLabel {
    pub name: String,
    /// Hex without `#`, when the host has one.
    pub color: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Summary, routing, stack, linked threads

/// `pullRequests.summary`: the compact card shown for linked pull requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestSummary {
    pub provider: SourceControlProviderKind,
    pub project_id: ProjectId,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: PullRequestState,
    pub is_draft: Option<bool>,
    pub head_branch: String,
    pub base_branch: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
    pub updated_at: String,
    pub observed_at: Option<f64>,
    pub author: Option<PullRequestActor>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub changed_files: Option<u64>,
    pub review_decision: Option<ReviewDecision>,
    pub checks_state: Option<ChecksState>,
    pub mergeability: Option<Mergeability>,
    pub stack: Option<PullRequestStackMembership>,
}

/// `pullRequests.routing`: which signed-in account and project serve a pull request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRoutingResult {
    pub account_id: String,
    pub host: String,
    pub provider: SourceControlProviderKind,
    pub viewer: String,
    pub project_title: String,
    pub workspace_root: String,
}

/// `pullRequests.routingIdentity`.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestRoutingIdentityInput {
    pub host: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestRoutingIdentityResult {
    pub account_id: String,
    pub host: String,
    pub provider: SourceControlProviderKind,
    pub viewer: String,
}

/// `pullRequests.stack` (`null` when the pull request is not stacked).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestStack {
    pub id: String,
    pub number: u64,
    pub url: String,
    pub base: String,
    #[serde(default)]
    pub layers: Vec<PullRequestStackLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestStackLayer {
    pub number: u64,
    pub title: Option<String>,
    pub is_draft: Option<bool>,
    pub head_sha: Option<String>,
    pub head_branch: String,
    pub state: PullRequestState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestLinkedThreadsResult {
    #[serde(default)]
    pub threads: Vec<PullRequestLinkedThread>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestLinkedThread {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub title: String,
    pub archived_at: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Detail and preview

/// `pullRequests.detail`: everything the detail header and merge box need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDetail {
    pub provider: SourceControlProviderKind,
    pub capabilities: PullRequestCapabilities,
    pub viewer_permissions: PullRequestViewerPermissions,
    pub project_id: ProjectId,
    pub project_title: String,
    pub workspace_root: String,
    pub repository: String,
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub url: String,
    pub author: Option<PullRequestActor>,
    pub state: PullRequestState,
    #[serde(default)]
    pub is_draft: bool,
    pub mergeability: Mergeability,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub changed_files: u64,
    pub head_branch: String,
    pub head_repository_name_with_owner: Option<String>,
    pub base_branch: String,
    pub created_at: String,
    pub updated_at: String,
    pub observed_at: Option<f64>,
    pub merged_at: Option<String>,
    pub closed_at: Option<String>,
    #[serde(default)]
    pub reviewers: Vec<PullRequestActor>,
    #[serde(default)]
    pub labels: Vec<PullRequestLabel>,
    #[serde(default)]
    pub checks: Vec<PullRequestCheck>,
    pub merge_capabilities: PullRequestMergeCapabilities,
    pub viewer: Option<String>,
    /// `up-to-date`, `behind`, or `unknown`.
    pub base_comparison: Option<String>,
    pub behind_by: Option<u64>,
    pub auto_merge_enabled: Option<bool>,
    pub auto_merge_method: Option<MergeMethod>,
    pub workflow_approvals_required: Option<u64>,
}

/// What the host supports for this pull request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCapabilities {
    #[serde(default)]
    pub diff: bool,
    #[serde(default)]
    pub comment: bool,
    #[serde(default)]
    pub actions: Vec<PullRequestAction>,
    #[serde(default)]
    pub merge_methods: Vec<MergeMethod>,
    pub update_methods: Option<Vec<UpdateMethod>>,
    #[serde(default)]
    pub search: bool,
    pub reactions: Option<bool>,
    /// `host` or `environment`: where "viewed" file state is stored.
    pub viewed_files: Option<String>,
    pub review: PullRequestReviewCapabilities,
    pub reviewers: PullRequestReviewerCapabilities,
    pub edit: Option<PullRequestEditCapabilities>,
    pub stacks: Option<bool>,
    pub stack_actions: Option<bool>,
    pub labels: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewCapabilities {
    #[serde(default)]
    pub inline_comment: bool,
    #[serde(default)]
    pub reply: bool,
    #[serde(default)]
    pub resolve: bool,
    #[serde(default)]
    pub verdicts: Vec<ReviewVerdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewerCapabilities {
    #[serde(default)]
    pub request: bool,
    #[serde(default)]
    pub list_candidates: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestEditCapabilities {
    #[serde(default)]
    pub change_request: bool,
    #[serde(default)]
    pub comment: bool,
}

/// What the signed-in viewer may do (capabilities minus permissions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestViewerPermissions {
    pub stack_rebase: Option<bool>,
    #[serde(default)]
    pub actions: Vec<PullRequestAction>,
    #[serde(default)]
    pub comment: bool,
    #[serde(default)]
    pub resolve: bool,
    #[serde(default)]
    pub verdicts: Vec<ReviewVerdict>,
    #[serde(default)]
    pub request_reviewers: bool,
    pub update_methods: Option<Vec<UpdateMethod>>,
    pub labels: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCheck {
    pub name: String,
    pub status: CheckStatus,
    pub description: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestMergeCapabilities {
    #[serde(default)]
    pub merge: bool,
    #[serde(default)]
    pub squash: bool,
    #[serde(default)]
    pub rebase: bool,
}

/// `pullRequests.preview`: the hover card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestPreview {
    pub project_id: ProjectId,
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: Option<PullRequestActor>,
    pub state: PullRequestState,
    #[serde(default)]
    pub is_draft: bool,
    pub created_at: String,
}

// ---------------------------------------------------------------------------------------------
// Activity (conversation tab)

/// `pullRequests.activity`: comments, review threads, commits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestActivity {
    pub author: Option<PullRequestActor>,
    pub reviewers: Option<Vec<PullRequestActor>>,
    #[serde(default)]
    pub comments: Vec<PullRequestComment>,
    #[serde(default)]
    pub comment_count: u64,
    #[serde(default)]
    pub comments_truncated: bool,
    #[serde(default)]
    pub review_threads: Vec<PullRequestReviewThread>,
    #[serde(default)]
    pub commits: Vec<PullRequestCommit>,
    pub reactions: Option<Vec<PullRequestReaction>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestComment {
    pub id: String,
    pub kind: CommentKind,
    pub author: Option<PullRequestActor>,
    #[serde(default)]
    pub body: String,
    pub created_at: String,
    pub url: Option<String>,
    pub path: Option<String>,
    pub review_state: Option<String>,
    pub reactions: Option<Vec<PullRequestReaction>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewThread {
    pub id: String,
    pub path: String,
    pub line: Option<u64>,
    pub side: DiffSide,
    #[serde(default)]
    pub is_resolved: bool,
    #[serde(default)]
    pub is_outdated: bool,
    #[serde(default)]
    pub comments: Vec<PullRequestThreadComment>,
    pub comment_count: Option<u64>,
    /// More comments via `pullRequests.threadComments`.
    pub next_comments_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadComment {
    pub id: String,
    pub author: Option<PullRequestActor>,
    #[serde(default)]
    pub body: String,
    pub created_at: String,
    pub url: Option<String>,
    pub reactions: Option<Vec<PullRequestReaction>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommit {
    pub oid: String,
    pub message_headline: String,
    pub committed_date: String,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub authors: Option<Vec<PullRequestActor>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReaction {
    pub content: ReactionContent,
    pub count: u64,
    #[serde(default)]
    pub actors: Vec<String>,
    #[serde(default)]
    pub viewer_has_reacted: bool,
}

/// `pullRequests.threadComments`: the next page of a review thread.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadCommentsInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub thread_id: String,
    pub cursor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadCommentsResult {
    #[serde(default)]
    pub comments: Vec<PullRequestThreadComment>,
    pub next_cursor: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Files tab

/// `pullRequests.diffFileContents`: both sides of one file for the diff viewer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDiffFileContentsInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub change_type: DiffChangeType,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDiffFileContentsResult {
    #[serde(default)]
    pub old_contents: String,
    #[serde(default)]
    pub new_contents: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestFilesViewedResult {
    #[serde(default)]
    pub files: Vec<PullRequestFileViewed>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestFileViewed {
    pub path: String,
    pub state: FileViewedState,
}

/// `pullRequests.setFilesViewed`.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestSetFilesViewedInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub files: Vec<PullRequestFileViewedChange>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullRequestFileViewedChange {
    pub path: String,
    pub viewed: bool,
}

/// `POST /api/pull-requests/diff` (HTTP, not RPC: patches are large and compress well). Ask
/// again with `next_cursor` until it is `None`.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestDiffInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// One commit's diff instead of the whole change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDiffResult {
    /// Unified diff text for this slice.
    pub patch: String,
    /// Something in this slice could not be shown (binary file, withheld hunk).
    #[serde(default)]
    pub truncated: bool,
    pub next_cursor: Option<String>,
    /// Host counts for files whose hunks were withheld.
    pub omitted_file_stats: Option<Vec<PullRequestOmittedFileStat>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestOmittedFileStat {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

// ---------------------------------------------------------------------------------------------
// Mutations

/// `pullRequests.runAction`: merge, ready/draft, close/reopen, update branch, auto-merge, ...
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestActionInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub action: PullRequestAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_method: Option<MergeMethod>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_method: Option<UpdateMethod>,
    /// Act on the whole stack ending at this number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_number: Option<u64>,
    /// Refuse if any layer's head moved since the UI loaded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_stack_heads: Option<Vec<PullRequestStackHead>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestStackHead {
    pub number: u64,
    pub head_sha: String,
}

/// `pullRequests.update`: edit title and/or body.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestUpdateInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// `pullRequests.comment`: a top-level conversation comment.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestCommentInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub body: String,
}

/// `pullRequests.updateComment`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestCommentUpdateInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub comment_id: String,
    /// `issue-comment` or `review-comment`.
    pub kind: CommentKind,
    pub body: String,
}

/// `pullRequests.submitReview`.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestSubmitReviewInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub verdict: ReviewVerdict,
    pub body: String,
    pub comments: Vec<PullRequestReviewCommentDraft>,
}

/// A pending inline comment.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewCommentDraft {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub position: PullRequestReviewPosition,
    pub body: String,
}

/// Which diff line an inline comment attaches to.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PullRequestReviewPosition {
    #[serde(rename_all = "camelCase")]
    Added { new_line: u64 },
    #[serde(rename_all = "camelCase")]
    Deleted { old_line: u64 },
    #[serde(rename_all = "camelCase")]
    Context {
        old_line: u64,
        new_line: u64,
        side: DiffSide,
    },
}

/// `pullRequests.replyToThread`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadReplyInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub thread_id: String,
    pub body: String,
}

/// `pullRequests.setThreadResolution`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestThreadResolutionInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub thread_id: String,
    pub resolved: bool,
}

/// `pullRequests.setReaction`. No `subject_id` reacts to the pull request itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReactionInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    pub content: ReactionContent,
    pub reacted: bool,
}

/// `pullRequests.invalidate`: drop server caches (one pull request, or everything).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestInvalidateInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<PullRequestRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_viewed_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestReviewerCandidateList {
    #[serde(default)]
    pub candidates: Vec<PullRequestReviewerCandidate>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestReviewerCandidate {
    pub is_bot: Option<bool>,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub id: String,
    pub kind: ReviewerKind,
    #[serde(default)]
    pub is_requested: bool,
}

/// `pullRequests.requestReviewers`: add (`requested: true`) or remove reviewers.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestReviewerRequestInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub reviewers: Vec<PullRequestReviewerChoice>,
    pub requested: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PullRequestReviewerChoice {
    pub id: String,
    pub kind: ReviewerKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullRequestLabelCandidateList {
    #[serde(default)]
    pub candidates: Vec<PullRequestLabelCandidate>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestLabelCandidate {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub is_applied: bool,
}

/// `pullRequests.setLabels`: apply (`applied: true`) or remove labels.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestLabelChangeInput {
    #[serde(flatten)]
    pub reference: PullRequestRef,
    pub labels: Vec<String>,
    pub applied: bool,
}
