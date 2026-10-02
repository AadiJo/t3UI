//! Orchestration read model, events, and stream items (`packages/contracts/src/orchestration.ts`).
//!
//! Names match upstream so spec and source pointers map 1:1. Row collections are
//! `Vec<Arc<Row>>`: reducers replace only the rows that changed, so a published state snapshot
//! shares every untouched row with the previous one (views can compare with `Arc::ptr_eq`).
//!
//! Field conventions (protocol.md section 2): `NullOr` fields are `Option<T>`; `optional` keys
//! are `Option<T>` with `skip_serializing_if`; `optional(NullOr)` patch fields where absent and
//! `null` differ are `Option<Option<T>>`.

use std::{collections::BTreeMap, sync::Arc};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::{
    ids::{
        ActivityId, ApprovalRequestId, AttachmentId, CommandId, EventId, MessageId, PlanId,
        ProjectId, ProviderInstanceId, ThreadId, TurnId,
    },
    open_enum,
    schema::{double_option, null_as_empty},
};

open_enum! {
    /// How much the agent may do without asking.
    pub enum RuntimeMode {
        ApprovalRequired = "approval-required",
        AutoAcceptEdits = "auto-accept-edits",
        Auto = "auto",
        FullAccess = "full-access",
    }
}

open_enum! {
    /// Default chat, or plan mode.
    pub enum InteractionMode {
        Default = "default",
        Plan = "plan",
    }
}

open_enum! {
    pub enum MessageRole {
        User = "user",
        Assistant = "assistant",
        System = "system",
        /// Only sent when the subscription asked for `reasoningMessages`; otherwise rewritten to
        /// `system` by the server.
        Reasoning = "reasoning",
    }
}

open_enum! {
    /// Provider session status. The server never produces `idle` (protocol.md 5.7).
    pub enum SessionStatus {
        Idle = "idle",
        Starting = "starting",
        Running = "running",
        Ready = "ready",
        Interrupted = "interrupted",
        Stopped = "stopped",
        Error = "error",
    }
}

open_enum! {
    pub enum TurnState {
        Running = "running",
        Interrupted = "interrupted",
        Completed = "completed",
        Error = "error",
    }
}

open_enum! {
    pub enum ActivityTone {
        Info = "info",
        Tool = "tool",
        Approval = "approval",
        Error = "error",
    }
}

open_enum! {
    /// `missing` is a mid-turn placeholder; the real capture follows with `ready`.
    pub enum CheckpointStatus {
        Ready = "ready",
        Missing = "missing",
        Error = "error",
    }
}

open_enum! {
    pub enum SettledOverride {
        Settled = "settled",
        Active = "active",
    }
}

open_enum! {
    pub enum ThreadEnvMode {
        Local = "local",
        Worktree = "worktree",
    }
}

open_enum! {
    pub enum ApprovalDecision {
        Accept = "accept",
        AcceptForSession = "acceptForSession",
        AcceptAlways = "acceptAlways",
        Decline = "decline",
        Cancel = "cancel",
    }
}

open_enum! {
    pub enum PullRequestState {
        Open = "open",
        Closed = "closed",
        Merged = "merged",
    }
}

open_enum! {
    /// Who created a thread to pull request link. `stack-dismissed` is a tombstone; hide it.
    pub enum PullRequestLinkSource {
        Manual = "manual",
        Created = "created",
        Agent = "agent",
        Stack = "stack",
        StackDismissed = "stack-dismissed",
    }
}

open_enum! {
    pub enum ProjectScriptIcon {
        Play = "play",
        Test = "test",
        Lint = "lint",
        Configure = "configure",
        Build = "build",
        Debug = "debug",
    }
}

open_enum! {
    pub enum BackgroundLiveness {
        Working = "working",
        Monitoring = "monitoring",
    }
}

open_enum! {
    pub enum AttachmentKind {
        Image = "image",
        File = "file",
    }
}

open_enum! {
    pub enum UnsettleReason {
        User = "user",
        Activity = "activity",
    }
}

open_enum! {
    pub enum AggregateKind {
        Project = "project",
        Thread = "thread",
    }
}

open_enum! {
    pub enum TitleSource {
        Manual = "manual",
        Generated = "generated",
    }
}

// ---------------------------------------------------------------------------------------------
// Model selection

/// Which provider instance and model a thread uses, plus option picks ("traits").
///
/// Wire: `{"instanceId":"codex","model":"gpt-5.5","options":[{"id":"effort","value":"high"}]}`.
/// Decoding also accepts the legacy `provider` key and the legacy object form of `options`
/// (`model.ts:55-115`); encoding always writes the array form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub instance_id: ProviderInstanceId,
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ProviderOptionSelection>,
}

/// One option pick, e.g. `{"id":"effort","value":"high"}` or `{"id":"fastMode","value":true}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOptionSelection {
    pub id: String,
    pub value: ProviderOptionValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProviderOptionValue {
    Bool(bool),
    String(String),
}

impl<'de> Deserialize<'de> for ModelSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            instance_id: Option<ProviderInstanceId>,
            provider: Option<ProviderInstanceId>,
            model: String,
            #[serde(default)]
            options: Option<Value>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let instance_id = wire
            .instance_id
            .or(wire.provider)
            .ok_or_else(|| serde::de::Error::missing_field("instanceId"))?;
        let options = match wire.options {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(items)) => items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
            Some(Value::Object(map)) => map
                .into_iter()
                .filter_map(|(id, value)| {
                    let id = id.trim().to_owned();
                    let value = match value {
                        Value::String(s) if !s.trim().is_empty() => {
                            ProviderOptionValue::String(s.trim().to_owned())
                        }
                        Value::Bool(b) => ProviderOptionValue::Bool(b),
                        _ => return None,
                    };
                    (!id.is_empty()).then_some(ProviderOptionSelection { id, value })
                })
                .collect(),
            Some(_) => Vec::new(),
        };
        Ok(ModelSelection {
            instance_id,
            model: wire.model,
            options,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Projects

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScript {
    pub id: String,
    pub name: String,
    pub command: String,
    pub icon: ProjectScriptIcon,
    pub run_on_worktree_create: bool,
    #[serde(rename = "async", default, skip_serializing_if = "Option::is_none")]
    pub run_async: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_open_preview: Option<bool>,
}

/// User-chosen project icon (`ProjectIconOverride`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProjectIconOverride {
    #[serde(rename_all = "camelCase")]
    Lucide {
        name: String,
        color: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        monogram_text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        monogram: Option<String>,
    },
    Emoji {
        emoji: String,
    },
    Monogram {
        text: String,
        color: String,
    },
    /// An icon kind this client does not know. Never send it.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdentity {
    pub canonical_key: String,
    pub locator: RepositoryIdentityLocator,
    pub web_url: Option<String>,
    pub root_path: Option<String>,
    pub display_name: Option<String>,
    pub provider: Option<String>,
    pub owner: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdentityLocator {
    pub source: String,
    pub remote_name: Option<String>,
    pub remote_url: Option<String>,
}

/// A project row in the shell (sidebar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationProjectShell {
    pub id: ProjectId,
    pub title: String,
    pub workspace_root: String,
    pub repository_identity: Option<RepositoryIdentity>,
    pub default_model_selection: Option<ModelSelection>,
    pub default_thread_env_mode: Option<ThreadEnvMode>,
    pub auto_pull: Option<bool>,
    pub favicon_path: Option<String>,
    pub project_icon: Option<ProjectIconOverride>,
    #[serde(default)]
    pub scripts: Vec<ProjectScript>,
    pub created_at: String,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------------------------
// Threads

/// Legacy single pull request link, still emitted as the thread's current pull request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadLinkedPullRequest {
    pub project_id: ProjectId,
    pub repository: String,
    pub number: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestLink {
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub url: String,
    pub source: PullRequestLinkSource,
    pub linked_at: String,
    pub snapshot: Option<ThreadPullRequestSnapshot>,
    pub stack: Option<ThreadPullRequestStack>,
}

impl ThreadPullRequestLink {
    /// Link identity: host and repository compared case-insensitively, plus the number
    /// (`packages/shared/src/threadPullRequests.ts:67`).
    pub fn same_key(&self, host: &str, repository: &str, number: u64) -> bool {
        self.number == number
            && self.host.trim().eq_ignore_ascii_case(host.trim())
            && self
                .repository
                .trim()
                .eq_ignore_ascii_case(repository.trim())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestSnapshot {
    pub state: PullRequestState,
    pub title: String,
    pub head_branch: String,
    pub base_branch: String,
    #[serde(default)]
    pub is_draft: bool,
    pub updated_at: Option<String>,
    pub synced_at: String,
    pub closed_at: Option<String>,
    pub merged_at: Option<String>,
    pub author: Option<PullRequestActor>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub changed_files: Option<u64>,
    pub review_decision: Option<String>,
    pub checks_state: Option<String>,
    pub mergeability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestActor {
    pub is_bot: Option<bool>,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestStack {
    pub kind: String,
    pub id: String,
    pub number: u64,
    pub url: String,
    pub base: String,
    #[serde(default)]
    pub layers: Vec<ThreadPullRequestStackLayer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestStackLayer {
    pub number: u64,
    pub head_branch: String,
    pub state: PullRequestState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProposedPlanReference {
    pub thread_id: ThreadId,
    pub plan_id: PlanId,
}

/// The newest turn of a thread. `state` is authoritative on the shell row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationLatestTurn {
    pub turn_id: TurnId,
    pub state: TurnState,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub assistant_message_id: Option<MessageId>,
    pub source_proposed_plan: Option<SourceProposedPlanReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTitleRegeneration {
    pub request_id: CommandId,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTitleState {
    pub source: TitleSource,
    pub version: CommandId,
    pub needs_refinement: bool,
}

/// The provider session attached to a thread.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationSession {
    pub thread_id: ThreadId,
    pub status: SessionStatus,
    pub provider_name: Option<String>,
    pub provider_instance_id: Option<ProviderInstanceId>,
    pub runtime_mode: Option<RuntimeMode>,
    pub active_turn_id: Option<TurnId>,
    pub last_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanProgress {
    pub step: String,
    pub completed_steps: u32,
    pub total_steps: u32,
}

/// A thread row in the shell: everything the sidebar and header need, no messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationThreadShell {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub title: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: RuntimeMode,
    pub interaction_mode: Option<InteractionMode>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub linked_pull_request: Option<ThreadLinkedPullRequest>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub pull_requests: Vec<ThreadPullRequestLink>,
    pub branch_pull_request: Option<ThreadLinkedPullRequest>,
    pub latest_turn: Option<OrchestrationLatestTurn>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub settled_override: Option<SettledOverride>,
    pub settled_at: Option<String>,
    pub unsettled_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub snoozed_at: Option<String>,
    pub pinned_at: Option<String>,
    pub pin_order_key: Option<String>,
    pub active_order_key: Option<String>,
    pub auto_settle_disabled_at: Option<String>,
    pub title_regeneration: Option<ThreadTitleRegeneration>,
    pub title_state: Option<ThreadTitleState>,
    pub session: Option<OrchestrationSession>,
    pub latest_user_message_at: Option<String>,
    #[serde(default)]
    pub has_pending_approvals: bool,
    #[serde(default)]
    pub has_pending_user_input: bool,
    #[serde(default)]
    pub has_actionable_proposed_plan: bool,
    pub background_liveness: Option<BackgroundLiveness>,
    pub plan_progress: Option<PlanProgress>,
}

/// An attachment on a message. Image, file, and unknown types share these base fields
/// (`ChatUnknownAttachment` keeps new types decodable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAttachment {
    #[serde(rename = "type")]
    pub kind: AttachmentKind,
    pub id: AttachmentId,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    /// `SnapShotSource` for images, `{"_tag":"pasted-text"}` for files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
}

/// Structured context attached to a user message (`OrchestrationMessageContext`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationMessageContext {
    pub version: u32,
    #[serde(default)]
    pub records: Vec<ContextRecord>,
}

/// One composer context chip (`ComposerContextRecord`): mention, skill, file, image, terminal,
/// element, preview annotation, review comment, or a newer kind. The common fields are typed;
/// kind-specific fields (`path`, `name`, `attachmentId`, `text`, ...) stay in `fields`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRecord {
    pub version: u32,
    pub context_id: String,
    pub label: String,
    pub kind: String,
    #[serde(flatten)]
    pub fields: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationMessage {
    pub id: MessageId,
    pub role: MessageRole,
    pub text: String,
    pub attachments: Option<Vec<ChatAttachment>>,
    pub context: Option<OrchestrationMessageContext>,
    pub turn_id: Option<TurnId>,
    #[serde(default)]
    pub streaming: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A work-log row. `kind` is open and `payload` is untyped by design; interpret per kind
/// (protocol.md 5.5). `sequence` is the provider runtime's sequence, not the event sequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationThreadActivity {
    pub id: ActivityId,
    pub tone: ActivityTone,
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub payload: Value,
    pub turn_id: Option<TurnId>,
    pub sequence: Option<i64>,
    pub created_at: String,
}

impl OrchestrationThreadActivity {
    /// The `payload.requestId` of approval and user-input activities.
    pub fn request_id(&self) -> Option<ApprovalRequestId> {
        self.payload
            .get("requestId")
            .and_then(Value::as_str)
            .map(ApprovalRequestId::from)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationCheckpointFile {
    pub path: String,
    pub kind: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationCheckpointSummary {
    pub turn_id: TurnId,
    pub checkpoint_turn_count: u32,
    pub checkpoint_ref: String,
    pub status: CheckpointStatus,
    #[serde(default)]
    pub files: Vec<OrchestrationCheckpointFile>,
    pub assistant_message_id: Option<MessageId>,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationProposedPlan {
    pub id: PlanId,
    pub turn_id: Option<TurnId>,
    pub plan_markdown: String,
    pub implemented_at: Option<String>,
    pub implementation_thread_id: Option<ThreadId>,
    pub created_at: String,
    pub updated_at: String,
}

/// A thread with its detail: messages, activities, plans, checkpoints. The chat view's data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationThread {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub title: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: RuntimeMode,
    pub interaction_mode: Option<InteractionMode>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub linked_pull_request: Option<ThreadLinkedPullRequest>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub pull_requests: Vec<ThreadPullRequestLink>,
    pub branch_pull_request: Option<ThreadLinkedPullRequest>,
    pub latest_turn: Option<OrchestrationLatestTurn>,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub settled_override: Option<SettledOverride>,
    pub settled_at: Option<String>,
    pub unsettled_at: Option<String>,
    pub snoozed_until: Option<String>,
    pub snoozed_at: Option<String>,
    pub pinned_at: Option<String>,
    pub pin_order_key: Option<String>,
    pub active_order_key: Option<String>,
    pub auto_settle_disabled_at: Option<String>,
    pub title_regeneration: Option<ThreadTitleRegeneration>,
    pub title_state: Option<ThreadTitleState>,
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub messages: Vec<Arc<OrchestrationMessage>>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub proposed_plans: Vec<Arc<OrchestrationProposedPlan>>,
    #[serde(default)]
    pub activities: Vec<Arc<OrchestrationThreadActivity>>,
    #[serde(default)]
    pub checkpoints: Vec<Arc<OrchestrationCheckpointSummary>>,
    pub session: Option<OrchestrationSession>,
}

// ---------------------------------------------------------------------------------------------
// Snapshots and stream items

/// Every project plus every active thread (`GET /api/orchestration/shell` and the socket's
/// shell snapshot item). `snapshot_sequence` is the resume cursor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationShellSnapshot {
    pub snapshot_sequence: u64,
    #[serde(default)]
    pub projects: Vec<Arc<OrchestrationProjectShell>>,
    #[serde(default)]
    pub threads: Vec<Arc<OrchestrationThreadShell>>,
    pub updated_at: String,
}

/// Paging cursor for older turns of a windowed thread snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationThreadDetailPage {
    pub before_cursor: Option<String>,
    #[serde(default)]
    pub has_more: bool,
    pub snapshot_sequence: u64,
    pub thread_sequence: Option<u64>,
}

/// One thread with detail (`GET /api/orchestration/threads/:id` and the thread snapshot item).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationThreadDetailSnapshot {
    pub snapshot_sequence: u64,
    pub thread: OrchestrationThread,
    pub page: Option<OrchestrationThreadDetailPage>,
}

/// Payload of `orchestration.subscribeShell`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeShellInput {
    /// Resume cursor. Omit for a fresh snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<u64>,
    /// Ask for a `synchronized` item once caught up. Only when
    /// `ServerConfig.shell_resume_completion_marker`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_completion_marker: Option<bool>,
}

/// Payload of `orchestration.subscribeThread`. Only set the optional flags the server
/// advertises in `ServerConfig` (protocol.md 5.3).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeThreadInput {
    pub thread_id: ThreadId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_messages: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_completion_marker: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_limit: Option<u32>,
}

/// An item of `orchestration.subscribeShell`.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellStreamItem {
    Snapshot(OrchestrationShellSnapshot),
    /// Everything buffered during the snapshot or replay has been delivered.
    Synchronized,
    ProjectUpserted {
        sequence: u64,
        project: Arc<OrchestrationProjectShell>,
    },
    ProjectRemoved {
        sequence: u64,
        project_id: ProjectId,
    },
    ThreadUpserted {
        sequence: u64,
        thread: Arc<OrchestrationThreadShell>,
    },
    ThreadRemoved {
        sequence: u64,
        thread_id: ThreadId,
    },
    /// A `kind` this client does not know. Ignore it.
    Unknown {
        kind: String,
    },
}

// Stream items decode through a flat struct of optional fields instead of an internally tagged
// derive, which would buffer the whole (possibly large) snapshot before decoding it.
impl<'de> Deserialize<'de> for ShellStreamItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            kind: String,
            sequence: Option<u64>,
            snapshot: Option<OrchestrationShellSnapshot>,
            project: Option<Arc<OrchestrationProjectShell>>,
            project_id: Option<ProjectId>,
            thread: Option<Arc<OrchestrationThreadShell>>,
            thread_id: Option<ThreadId>,
        }
        use serde::de::Error;
        let wire = Wire::deserialize(deserializer)?;
        let sequence = || {
            wire.sequence
                .ok_or_else(|| D::Error::missing_field("sequence"))
        };
        Ok(match wire.kind.as_str() {
            "snapshot" => Self::Snapshot(
                wire.snapshot
                    .ok_or_else(|| D::Error::missing_field("snapshot"))?,
            ),
            "synchronized" => Self::Synchronized,
            "project-upserted" => Self::ProjectUpserted {
                sequence: sequence()?,
                project: wire
                    .project
                    .ok_or_else(|| D::Error::missing_field("project"))?,
            },
            "project-removed" => Self::ProjectRemoved {
                sequence: sequence()?,
                project_id: wire
                    .project_id
                    .ok_or_else(|| D::Error::missing_field("projectId"))?,
            },
            "thread-upserted" => Self::ThreadUpserted {
                sequence: sequence()?,
                thread: wire
                    .thread
                    .ok_or_else(|| D::Error::missing_field("thread"))?,
            },
            "thread-removed" => Self::ThreadRemoved {
                sequence: sequence()?,
                thread_id: wire
                    .thread_id
                    .ok_or_else(|| D::Error::missing_field("threadId"))?,
            },
            _ => Self::Unknown { kind: wire.kind },
        })
    }
}

/// An item of `orchestration.subscribeThread`.
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadStreamItem {
    Snapshot(Box<OrchestrationThreadDetailSnapshot>),
    Synchronized,
    Event(Box<OrchestrationEvent>),
    Unknown { kind: String },
}

impl<'de> Deserialize<'de> for ThreadStreamItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            kind: String,
            snapshot: Option<Box<OrchestrationThreadDetailSnapshot>>,
            event: Option<Box<OrchestrationEvent>>,
        }
        use serde::de::Error;
        let wire = Wire::deserialize(deserializer)?;
        Ok(match wire.kind.as_str() {
            "snapshot" => Self::Snapshot(
                wire.snapshot
                    .ok_or_else(|| D::Error::missing_field("snapshot"))?,
            ),
            "synchronized" => Self::Synchronized,
            "event" => Self::Event(wire.event.ok_or_else(|| D::Error::missing_field("event"))?),
            _ => Self::Unknown { kind: wire.kind },
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Events

/// Event metadata. Only the fields a client may read are typed.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationEventMetadata {
    pub provider_turn_id: Option<String>,
    pub provider_item_id: Option<String>,
    pub request_id: Option<String>,
    pub ingested_at: Option<String>,
    pub history_import: Option<bool>,
    pub deferred_turn: Option<bool>,
}

/// One orchestration event: the envelope plus a typed body. Events with a `type` this client
/// does not know decode as [`EventBody::Unknown`] so the envelope (`sequence`) stays usable.
#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationEvent {
    pub sequence: u64,
    pub event_id: EventId,
    pub aggregate_kind: AggregateKind,
    pub aggregate_id: String,
    pub occurred_at: String,
    pub command_id: Option<CommandId>,
    pub causation_event_id: Option<EventId>,
    pub correlation_id: Option<CommandId>,
    pub metadata: OrchestrationEventMetadata,
    pub body: EventBody,
}

impl<'de> Deserialize<'de> for OrchestrationEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            sequence: u64,
            event_id: EventId,
            aggregate_kind: AggregateKind,
            aggregate_id: String,
            occurred_at: String,
            command_id: Option<CommandId>,
            causation_event_id: Option<EventId>,
            correlation_id: Option<CommandId>,
            #[serde(default)]
            metadata: OrchestrationEventMetadata,
            #[serde(rename = "type")]
            event_type: String,
            #[serde(default)]
            payload: Value,
        }
        let wire = Wire::deserialize(deserializer)?;
        let body = EventBody::decode(wire.event_type, wire.payload)
            .map_err(<D::Error as serde::de::Error>::custom)?;
        Ok(OrchestrationEvent {
            sequence: wire.sequence,
            event_id: wire.event_id,
            aggregate_kind: wire.aggregate_kind,
            aggregate_id: wire.aggregate_id,
            occurred_at: wire.occurred_at,
            command_id: wire.command_id,
            causation_event_id: wire.causation_event_id,
            correlation_id: wire.correlation_id,
            metadata: wire.metadata,
            body,
        })
    }
}

macro_rules! event_body {
    ($( $(#[$meta:meta])* $variant:ident($payload:ty) = $tag:literal, )*) => {
        /// The typed `type` + `payload` of an [`OrchestrationEvent`].
        #[derive(Debug, Clone, PartialEq)]
        pub enum EventBody {
            $( $(#[$meta])* $variant($payload), )*
            /// An event type this client does not know.
            Unknown { event_type: String, payload: Value },
        }

        impl EventBody {
            /// The wire `type` string.
            pub fn event_type(&self) -> &str {
                match self {
                    $( Self::$variant(_) => $tag, )*
                    Self::Unknown { event_type, .. } => event_type,
                }
            }

            fn decode(event_type: String, payload: Value) -> Result<Self, serde_json::Error> {
                Ok(match event_type.as_str() {
                    $( $tag => Self::$variant(serde_json::from_value(payload)?), )*
                    _ => Self::Unknown { event_type, payload },
                })
            }
        }
    };
}

event_body! {
    ProjectCreated(ProjectCreatedPayload) = "project.created",
    ProjectMetaUpdated(ProjectMetaUpdatedPayload) = "project.meta-updated",
    ProjectDeleted(ProjectDeletedPayload) = "project.deleted",
    ThreadCreated(ThreadCreatedPayload) = "thread.created",
    ThreadDeleted(ThreadDeletedPayload) = "thread.deleted",
    ThreadArchived(ThreadArchivedPayload) = "thread.archived",
    ThreadUnarchived(ThreadTimestampPayload) = "thread.unarchived",
    ThreadSettled(ThreadSettledPayload) = "thread.settled",
    ThreadUnsettled(ThreadReasonPayload) = "thread.unsettled",
    ThreadSnoozed(ThreadSnoozedPayload) = "thread.snoozed",
    ThreadUnsnoozed(ThreadReasonPayload) = "thread.unsnoozed",
    ThreadPinned(ThreadPinnedPayload) = "thread.pinned",
    ThreadUnpinned(ThreadTimestampPayload) = "thread.unpinned",
    ThreadPinReordered(ThreadPinReorderedPayload) = "thread.pin-reordered",
    ThreadAutoSettleSet(ThreadAutoSettleSetPayload) = "thread.auto-settle-set",
    ThreadMetaUpdated(ThreadMetaUpdatedPayload) = "thread.meta-updated",
    ThreadPullRequestLinked(ThreadPullRequestLinkedPayload) = "thread.pull-request-linked",
    ThreadPullRequestUnlinked(ThreadPullRequestUnlinkedPayload) = "thread.pull-request-unlinked",
    ThreadPullRequestSynced(ThreadPullRequestSyncedPayload) = "thread.pull-request-synced",
    ThreadRuntimeModeSet(ThreadRuntimeModeSetPayload) = "thread.runtime-mode-set",
    ThreadInteractionModeSet(ThreadInteractionModeSetPayload) = "thread.interaction-mode-set",
    /// A message was added, or (streaming) a text delta was appended.
    ThreadMessageSent(ThreadMessageSentPayload) = "thread.message-sent",
    ThreadTurnStartRequested(ThreadTurnStartRequestedPayload) = "thread.turn-start-requested",
    ThreadTurnInterruptRequested(ThreadTurnInterruptRequestedPayload) = "thread.turn-interrupt-requested",
    ThreadApprovalResponseRequested(ThreadApprovalResponseRequestedPayload) = "thread.approval-response-requested",
    ThreadUserInputResponseRequested(ThreadUserInputResponseRequestedPayload) = "thread.user-input-response-requested",
    ThreadCheckpointRevertRequested(ThreadCheckpointRevertRequestedPayload) = "thread.checkpoint-revert-requested",
    ThreadReverted(ThreadRevertedPayload) = "thread.reverted",
    ThreadSessionStopRequested(ThreadSessionStopRequestedPayload) = "thread.session-stop-requested",
    ThreadSessionSet(ThreadSessionSetPayload) = "thread.session-set",
    ThreadProposedPlanUpserted(ThreadProposedPlanUpsertedPayload) = "thread.proposed-plan-upserted",
    ThreadTurnDiffCompleted(ThreadTurnDiffCompletedPayload) = "thread.turn-diff-completed",
    ThreadActivityAppended(ThreadActivityAppendedPayload) = "thread.activity-appended",
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreatedPayload {
    pub project_id: ProjectId,
    pub title: String,
    pub workspace_root: String,
    pub repository_identity: Option<RepositoryIdentity>,
    pub default_model_selection: Option<ModelSelection>,
    pub favicon_path: Option<String>,
    pub project_icon: Option<ProjectIconOverride>,
    #[serde(default)]
    pub scripts: Vec<ProjectScript>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetaUpdatedPayload {
    pub project_id: ProjectId,
    pub title: Option<String>,
    pub workspace_root: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub repository_identity: Option<Option<RepositoryIdentity>>,
    #[serde(default, deserialize_with = "double_option")]
    pub default_model_selection: Option<Option<ModelSelection>>,
    #[serde(default, deserialize_with = "double_option")]
    pub default_thread_env_mode: Option<Option<ThreadEnvMode>>,
    pub auto_pull: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub favicon_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub project_icon: Option<Option<ProjectIconOverride>>,
    pub scripts: Option<Vec<ProjectScript>>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDeletedPayload {
    pub project_id: ProjectId,
    pub deleted_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCreatedPayload {
    pub thread_id: ThreadId,
    pub project_id: ProjectId,
    pub title: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: Option<RuntimeMode>,
    pub interaction_mode: Option<InteractionMode>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadDeletedPayload {
    pub thread_id: ThreadId,
    pub deleted_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArchivedPayload {
    pub thread_id: ThreadId,
    pub archived_at: String,
    pub updated_at: String,
}

/// Payload of events that only carry the thread and a timestamp (`thread.unarchived`,
/// `thread.unpinned`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTimestampPayload {
    pub thread_id: ThreadId,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSettledPayload {
    pub thread_id: ThreadId,
    pub settled_at: String,
    pub updated_at: String,
}

/// Payload of `thread.unsettled` and `thread.unsnoozed`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReasonPayload {
    pub thread_id: ThreadId,
    pub reason: UnsettleReason,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSnoozedPayload {
    pub thread_id: ThreadId,
    pub snoozed_until: String,
    pub snoozed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPinnedPayload {
    pub thread_id: ThreadId,
    pub pinned_at: String,
    pub pin_order_key: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPinReorderedPayload {
    pub thread_id: ThreadId,
    pub order_key: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAutoSettleSetPayload {
    pub thread_id: ThreadId,
    pub auto_settle_disabled_at: Option<String>,
    pub updated_at: String,
}

/// Partial thread update. `Option<Option<T>>` fields: `None` no change, `Some(None)` clear.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMetaUpdatedPayload {
    pub thread_id: ThreadId,
    #[serde(default, deserialize_with = "double_option")]
    pub active_order_key: Option<Option<String>>,
    pub title: Option<String>,
    pub regenerate_title: Option<bool>,
    pub previous_title: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub title_regeneration: Option<Option<ThreadTitleRegeneration>>,
    #[serde(default, deserialize_with = "double_option")]
    pub title_state: Option<Option<ThreadTitleState>>,
    pub model_selection: Option<ModelSelection>,
    #[serde(default, deserialize_with = "double_option")]
    pub branch: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub worktree_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub linked_pull_request: Option<Option<ThreadLinkedPullRequest>>,
    #[serde(default, deserialize_with = "double_option")]
    pub branch_pull_request: Option<Option<ThreadLinkedPullRequest>>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestLinkedPayload {
    pub thread_id: ThreadId,
    pub link: ThreadPullRequestLink,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestUnlinkedPayload {
    pub thread_id: ThreadId,
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPullRequestSyncedPayload {
    pub thread_id: ThreadId,
    pub host: String,
    pub repository: String,
    pub number: u64,
    pub snapshot: ThreadPullRequestSnapshot,
    pub stack: Option<ThreadPullRequestStack>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRuntimeModeSetPayload {
    pub thread_id: ThreadId,
    pub runtime_mode: RuntimeMode,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInteractionModeSetPayload {
    pub thread_id: ThreadId,
    pub interaction_mode: Option<InteractionMode>,
    pub updated_at: String,
}

/// Deltas arrive as `{text: <delta>, streaming: true}`; completion as `{text: "", streaming:
/// false}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMessageSentPayload {
    pub thread_id: ThreadId,
    pub message_id: MessageId,
    pub role: MessageRole,
    pub text: String,
    pub attachments: Option<Vec<ChatAttachment>>,
    pub context: Option<OrchestrationMessageContext>,
    pub turn_id: Option<TurnId>,
    #[serde(default)]
    pub streaming: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnStartRequestedPayload {
    pub thread_id: ThreadId,
    pub message_id: MessageId,
    pub model_selection: Option<ModelSelection>,
    pub title_seed: Option<String>,
    pub runtime_mode: Option<RuntimeMode>,
    pub interaction_mode: Option<InteractionMode>,
    pub source_proposed_plan: Option<SourceProposedPlanReference>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnInterruptRequestedPayload {
    pub thread_id: ThreadId,
    pub turn_id: Option<TurnId>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadApprovalResponseRequestedPayload {
    pub thread_id: ThreadId,
    pub request_id: ApprovalRequestId,
    pub decision: ApprovalDecision,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadUserInputResponseRequestedPayload {
    pub thread_id: ThreadId,
    pub request_id: ApprovalRequestId,
    #[serde(default)]
    pub answers: BTreeMap<String, Value>,
    pub attachments_by_question_id: Option<BTreeMap<String, Vec<ChatAttachment>>>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadCheckpointRevertRequestedPayload {
    pub thread_id: ThreadId,
    pub turn_count: u32,
    pub restore_files: Option<bool>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRevertedPayload {
    pub thread_id: ThreadId,
    pub turn_count: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSessionStopRequestedPayload {
    pub thread_id: ThreadId,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSessionSetPayload {
    pub thread_id: ThreadId,
    pub session: OrchestrationSession,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadProposedPlanUpsertedPayload {
    pub thread_id: ThreadId,
    pub proposed_plan: Arc<OrchestrationProposedPlan>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnDiffCompletedPayload {
    pub thread_id: ThreadId,
    pub turn_id: TurnId,
    pub checkpoint_turn_count: u32,
    pub checkpoint_ref: String,
    pub status: CheckpointStatus,
    #[serde(default)]
    pub files: Vec<OrchestrationCheckpointFile>,
    pub assistant_message_id: Option<MessageId>,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadActivityAppendedPayload {
    pub thread_id: ThreadId,
    pub activity: Arc<OrchestrationThreadActivity>,
}

// ---------------------------------------------------------------------------------------------
// Unary results

/// Result of `orchestration.dispatchCommand`: the sequence of the last event the command
/// produced. The events themselves arrive on the subscriptions, in either order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct DispatchResult {
    pub sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTurnDiffInput {
    pub thread_id: ThreadId,
    pub from_turn_count: u32,
    pub to_turn_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_whitespace: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFullThreadDiffInput {
    pub thread_id: ThreadId,
    pub to_turn_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_whitespace: Option<bool>,
}

/// Unified diff text between two checkpoints of a thread.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadTurnDiff {
    pub thread_id: ThreadId,
    pub from_turn_count: u32,
    pub to_turn_count: u32,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchThreadsInput {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchThreadsResult {
    #[serde(default)]
    pub matches: Vec<ThreadSearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSearchMatch {
    pub thread_id: ThreadId,
    pub project_id: ProjectId,
    pub source: MessageRole,
    pub snippet: String,
    pub message_created_at: Option<String>,
}
