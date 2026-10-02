//! `ClientOrchestrationCommand`: the payload of `orchestration.dispatchCommand`
//! (`packages/contracts/src/orchestration.ts:1461`, protocol.md section 6).
//!
//! Every command carries a fresh `commandId`; re-sending the same id is idempotent. Optional
//! keys are omitted rather than sent as `null`. `t3_client::commands` builds these with fresh
//! ids and timestamps.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

use crate::{
    ids::{ApprovalRequestId, AttachmentId, CommandId, MessageId, ProjectId, ThreadId, TurnId},
    orchestration::{
        ApprovalDecision, AttachmentKind, ChatAttachment, InteractionMode, ModelSelection,
        OrchestrationMessageContext, ProjectIconOverride, ProjectScript, RuntimeMode,
        SourceProposedPlanReference, ThreadEnvMode, ThreadLinkedPullRequest,
    },
};

/// A command for `orchestration.dispatchCommand`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ClientCommand {
    #[serde(rename = "project.create", rename_all = "camelCase")]
    ProjectCreate {
        command_id: CommandId,
        project_id: ProjectId,
        title: String,
        workspace_root: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        create_workspace_root_if_missing: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_model_selection: Option<ModelSelection>,
        created_at: String,
    },
    /// Partial project update. `scripts` replaces the whole list.
    #[serde(rename = "project.meta.update", rename_all = "camelCase")]
    ProjectMetaUpdate {
        command_id: CommandId,
        project_id: ProjectId,
        #[serde(flatten)]
        patch: ProjectMetaPatch,
    },
    #[serde(rename = "project.delete", rename_all = "camelCase")]
    ProjectDelete {
        command_id: CommandId,
        project_id: ProjectId,
        #[serde(skip_serializing_if = "Option::is_none")]
        force: Option<bool>,
    },
    /// Creates an empty thread. Upstream UI prefers `thread.turn.start` with
    /// `bootstrap.create_thread`.
    #[serde(rename = "thread.create", rename_all = "camelCase")]
    ThreadCreate {
        command_id: CommandId,
        thread_id: ThreadId,
        project_id: ProjectId,
        title: String,
        model_selection: ModelSelection,
        runtime_mode: RuntimeMode,
        interaction_mode: Option<InteractionMode>,
        branch: Option<String>,
        worktree_path: Option<String>,
        created_at: String,
    },
    #[serde(rename = "thread.delete", rename_all = "camelCase")]
    ThreadDelete {
        command_id: CommandId,
        thread_id: ThreadId,
    },
    /// Also stops a live session and closes the thread's terminals.
    #[serde(rename = "thread.archive", rename_all = "camelCase")]
    ThreadArchive {
        command_id: CommandId,
        thread_id: ThreadId,
    },
    #[serde(rename = "thread.unarchive", rename_all = "camelCase")]
    ThreadUnarchive {
        command_id: CommandId,
        thread_id: ThreadId,
    },
    #[serde(rename = "thread.settle", rename_all = "camelCase")]
    ThreadSettle {
        command_id: CommandId,
        thread_id: ThreadId,
    },
    /// `reason` is always `"user"` from a client.
    #[serde(rename = "thread.unsettle", rename_all = "camelCase")]
    ThreadUnsettle {
        command_id: CommandId,
        thread_id: ThreadId,
        reason: UserReason,
    },
    #[serde(rename = "thread.snooze", rename_all = "camelCase")]
    ThreadSnooze {
        command_id: CommandId,
        thread_id: ThreadId,
        snoozed_until: String,
    },
    #[serde(rename = "thread.unsnooze", rename_all = "camelCase")]
    ThreadUnsnooze {
        command_id: CommandId,
        thread_id: ThreadId,
        reason: UserReason,
    },
    #[serde(rename = "thread.pin", rename_all = "camelCase")]
    ThreadPin {
        command_id: CommandId,
        thread_id: ThreadId,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_key: Option<String>,
    },
    #[serde(rename = "thread.unpin", rename_all = "camelCase")]
    ThreadUnpin {
        command_id: CommandId,
        thread_id: ThreadId,
    },
    #[serde(rename = "thread.pin.reorder", rename_all = "camelCase")]
    ThreadPinReorder {
        command_id: CommandId,
        thread_id: ThreadId,
        order_key: String,
    },
    #[serde(rename = "thread.auto-settle.set", rename_all = "camelCase")]
    ThreadAutoSettleSet {
        command_id: CommandId,
        thread_id: ThreadId,
        enabled: bool,
    },
    #[serde(rename = "thread.active.reorder", rename_all = "camelCase")]
    ThreadActiveReorder {
        command_id: CommandId,
        thread_id: ThreadId,
        order_key: String,
    },
    /// Partial thread update. Rename is `title` only.
    #[serde(rename = "thread.meta.update", rename_all = "camelCase")]
    ThreadMetaUpdate {
        command_id: CommandId,
        thread_id: ThreadId,
        #[serde(flatten)]
        patch: ThreadMetaPatch,
    },
    #[serde(rename = "thread.runtime-mode.set", rename_all = "camelCase")]
    ThreadRuntimeModeSet {
        command_id: CommandId,
        thread_id: ThreadId,
        runtime_mode: RuntimeMode,
        created_at: String,
    },
    #[serde(rename = "thread.interaction-mode.set", rename_all = "camelCase")]
    ThreadInteractionModeSet {
        command_id: CommandId,
        thread_id: ThreadId,
        interaction_mode: InteractionMode,
        created_at: String,
    },
    /// Sends a user message and starts a turn (protocol.md 6.3).
    #[serde(rename = "thread.turn.start")]
    ThreadTurnStart(Box<TurnStart>),
    /// The stop button.
    #[serde(rename = "thread.turn.interrupt", rename_all = "camelCase")]
    ThreadTurnInterrupt {
        command_id: CommandId,
        thread_id: ThreadId,
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<TurnId>,
        created_at: String,
    },
    #[serde(rename = "thread.approval.respond", rename_all = "camelCase")]
    ThreadApprovalRespond {
        command_id: CommandId,
        thread_id: ThreadId,
        request_id: ApprovalRequestId,
        decision: ApprovalDecision,
        created_at: String,
    },
    /// `answers` is keyed by question id; values are JSON (string, array of strings, ...).
    #[serde(rename = "thread.user-input.respond", rename_all = "camelCase")]
    ThreadUserInputRespond {
        command_id: CommandId,
        thread_id: ThreadId,
        request_id: ApprovalRequestId,
        answers: BTreeMap<String, Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attachments_by_question_id: Option<BTreeMap<String, Vec<ChatAttachment>>>,
        created_at: String,
    },
    /// Only async questions (`responseMode: "message"`) can be dismissed.
    #[serde(rename = "thread.user-input.dismiss", rename_all = "camelCase")]
    ThreadUserInputDismiss {
        command_id: CommandId,
        thread_id: ThreadId,
        request_id: ApprovalRequestId,
        created_at: String,
    },
    /// Restores files and history to `turn_count`.
    #[serde(rename = "thread.checkpoint.revert", rename_all = "camelCase")]
    ThreadCheckpointRevert {
        command_id: CommandId,
        thread_id: ThreadId,
        turn_count: u32,
        created_at: String,
    },
    /// History only; files untouched.
    #[serde(rename = "thread.conversation.revert", rename_all = "camelCase")]
    ThreadConversationRevert {
        command_id: CommandId,
        thread_id: ThreadId,
        turn_count: u32,
        created_at: String,
    },
    #[serde(rename = "thread.session.stop", rename_all = "camelCase")]
    ThreadSessionStop {
        command_id: CommandId,
        thread_id: ThreadId,
        created_at: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        only_if_settled: Option<bool>,
    },
}

impl ClientCommand {
    /// The command's id, as echoed on the events it produces.
    pub fn command_id(&self) -> &CommandId {
        match self {
            Self::ThreadTurnStart(turn) => &turn.command_id,
            Self::ProjectCreate { command_id, .. }
            | Self::ProjectMetaUpdate { command_id, .. }
            | Self::ProjectDelete { command_id, .. }
            | Self::ThreadCreate { command_id, .. }
            | Self::ThreadDelete { command_id, .. }
            | Self::ThreadArchive { command_id, .. }
            | Self::ThreadUnarchive { command_id, .. }
            | Self::ThreadSettle { command_id, .. }
            | Self::ThreadUnsettle { command_id, .. }
            | Self::ThreadSnooze { command_id, .. }
            | Self::ThreadUnsnooze { command_id, .. }
            | Self::ThreadPin { command_id, .. }
            | Self::ThreadUnpin { command_id, .. }
            | Self::ThreadPinReorder { command_id, .. }
            | Self::ThreadAutoSettleSet { command_id, .. }
            | Self::ThreadActiveReorder { command_id, .. }
            | Self::ThreadMetaUpdate { command_id, .. }
            | Self::ThreadRuntimeModeSet { command_id, .. }
            | Self::ThreadInteractionModeSet { command_id, .. }
            | Self::ThreadTurnInterrupt { command_id, .. }
            | Self::ThreadApprovalRespond { command_id, .. }
            | Self::ThreadUserInputRespond { command_id, .. }
            | Self::ThreadUserInputDismiss { command_id, .. }
            | Self::ThreadCheckpointRevert { command_id, .. }
            | Self::ThreadConversationRevert { command_id, .. }
            | Self::ThreadSessionStop { command_id, .. } => command_id,
        }
    }
}

impl From<TurnStart> for ClientCommand {
    fn from(turn: TurnStart) -> Self {
        ClientCommand::ThreadTurnStart(Box::new(turn))
    }
}

/// The literal `"user"`, the only reason a client sends for unsettle/unsnooze.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserReason {
    #[default]
    User,
}

/// Fields of `project.meta.update`. Unset fields are left unchanged. `Some(None)` clears a
/// nullable field.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetaPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model_selection: Option<Option<ModelSelection>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_env_mode: Option<Option<ThreadEnvMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_pull: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon_path: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_icon: Option<Option<ProjectIconOverride>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<Vec<ProjectScript>>,
}

/// Fields of `thread.meta.update`. Unset fields are left unchanged. `Some(None)` clears a
/// nullable field.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadMetaPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Asks the server to generate a new title. Serializes as `true` when set.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub regenerate_title: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_selection: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<Option<String>>,
    /// Only apply the branch change if the thread is still on this branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_branch: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_path: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_pull_request: Option<Option<ThreadLinkedPullRequest>>,
}

/// `thread.turn.start`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStart {
    pub command_id: CommandId,
    pub thread_id: ThreadId,
    pub message: TurnStartMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_selection: Option<ModelSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_seed: Option<String>,
    pub runtime_mode: RuntimeMode,
    pub interaction_mode: InteractionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<TurnStartBootstrap>,
    /// "Implement this plan".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_proposed_plan: Option<SourceProposedPlanReference>,
    pub created_at: String,
}

/// The user message of a turn. `message_id` is client-chosen; the user message event carries
/// it, which is how an optimistic local copy is matched and replaced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartMessage {
    pub message_id: MessageId,
    /// Always `"user"`.
    pub role: UserRole,
    pub text: String,
    pub attachments: Vec<TurnAttachment>,
    /// Only when `capabilities.inline_message_context`; otherwise fold context into `text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<OrchestrationMessageContext>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    #[default]
    User,
}

/// An attachment on a new message: either pre-uploaded (`id` from
/// `attachments.createUploadUrl`) or an inline image (`data_url`, max 10 MiB).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnAttachment {
    #[serde(rename = "type")]
    pub kind: AttachmentKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<AttachmentId>,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_url: Option<String>,
}

/// Creates (and optionally prepares a worktree for) the thread in the same command.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartBootstrap {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_thread: Option<BootstrapCreateThread>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_worktree: Option<BootstrapPrepareWorktree>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_setup_script: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapCreateThread {
    pub project_id: ProjectId,
    pub title: String,
    pub model_selection: ModelSelection,
    pub runtime_mode: RuntimeMode,
    pub interaction_mode: InteractionMode,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPrepareWorktree {
    pub project_cwd: String,
    pub base_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_from_origin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_worktree: Option<bool>,
}
