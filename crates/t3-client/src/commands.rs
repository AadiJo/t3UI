//! Builders for `orchestration.dispatchCommand` payloads. Each fills a fresh `commandId` (UUID
//! v4) and `createdAt` (the server overwrites it with its receipt time). Gate lifecycle
//! commands (pin, snooze, settle, reorder) on the matching `ExecutionEnvironmentCapabilities`
//! flag. Send the result with
//! [`Environment::dispatch`](crate::Environment::dispatch).
//!
//! ```ignore
//! env.dispatch(commands::rename_thread(id, "Fix tests")).await?;
//! let turn = commands::turn_start(thread_id, "hello", RuntimeMode::FullAccess, InteractionMode::Default);
//! env.dispatch(turn.into()).await?;
//! ```

use std::collections::BTreeMap;

use serde_json::Value;
use t3_protocol::{
    ApprovalRequestId, CommandId, MessageId, ProjectId, ThreadId, TurnId,
    commands::{
        BootstrapCreateThread, ClientCommand, ThreadMetaPatch, TurnAttachment, TurnStart,
        TurnStartBootstrap, TurnStartMessage, UserReason, UserRole,
    },
    orchestration::{
        ApprovalDecision, ChatAttachment, InteractionMode, ModelSelection, RuntimeMode,
    },
};

/// The current time as ISO-8601 with milliseconds, like JS `toISOString()`.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Adds a project rooted at `workspace_root` (an absolute path on the server).
pub fn create_project(
    project_id: ProjectId,
    title: impl Into<String>,
    workspace_root: impl Into<String>,
    create_workspace_root_if_missing: bool,
) -> ClientCommand {
    ClientCommand::ProjectCreate {
        command_id: CommandId::random(),
        project_id,
        title: title.into(),
        workspace_root: workspace_root.into(),
        create_workspace_root_if_missing: create_workspace_root_if_missing.then_some(true),
        default_model_selection: None,
        created_at: now(),
    }
}

/// Removes a project. `force` also removes it when it still has threads.
pub fn remove_project(project_id: ProjectId, force: bool) -> ClientCommand {
    ClientCommand::ProjectDelete {
        command_id: CommandId::random(),
        project_id,
        force: force.then_some(true),
    }
}

/// Creates an empty thread. New threads from the composer use [`new_thread_turn`] instead.
pub fn create_thread(
    thread_id: ThreadId,
    project_id: ProjectId,
    title: impl Into<String>,
    model_selection: ModelSelection,
    runtime_mode: RuntimeMode,
    interaction_mode: InteractionMode,
) -> ClientCommand {
    ClientCommand::ThreadCreate {
        command_id: CommandId::random(),
        thread_id,
        project_id,
        title: title.into(),
        model_selection,
        runtime_mode,
        interaction_mode: Some(interaction_mode),
        branch: None,
        worktree_path: None,
        created_at: now(),
    }
}

/// Sends `text` as a new user message in an existing thread. Set `model_selection`,
/// `attachments`, or `message.context` on the result before dispatching if needed.
/// `message.message_id` identifies the optimistic local copy.
pub fn turn_start(
    thread_id: ThreadId,
    text: impl Into<String>,
    runtime_mode: RuntimeMode,
    interaction_mode: InteractionMode,
) -> TurnStart {
    TurnStart {
        command_id: CommandId::random(),
        thread_id,
        message: TurnStartMessage {
            message_id: MessageId::random(),
            role: UserRole::User,
            text: text.into(),
            attachments: Vec::<TurnAttachment>::new(),
            context: None,
        },
        model_selection: None,
        title_seed: None,
        runtime_mode,
        interaction_mode,
        bootstrap: None,
        source_proposed_plan: None,
        created_at: now(),
    }
}

/// Creates a thread and sends its first message in one command (what the upstream composer
/// does for a draft). The new thread id is `result.thread_id`.
pub fn new_thread_turn(
    project_id: ProjectId,
    title: impl Into<String>,
    text: impl Into<String>,
    model_selection: ModelSelection,
    runtime_mode: RuntimeMode,
    interaction_mode: InteractionMode,
) -> TurnStart {
    let text = text.into();
    let mut turn = turn_start(
        ThreadId::random(),
        text.clone(),
        runtime_mode.clone(),
        interaction_mode.clone(),
    );
    turn.model_selection = Some(model_selection.clone());
    turn.title_seed = Some(text);
    turn.bootstrap = Some(TurnStartBootstrap {
        create_thread: Some(BootstrapCreateThread {
            project_id,
            title: title.into(),
            model_selection,
            runtime_mode,
            interaction_mode,
            branch: None,
            worktree_path: None,
            created_at: turn.created_at.clone(),
        }),
        prepare_worktree: None,
        run_setup_script: None,
    });
    turn
}

/// "Revert to this message": restores files and history to checkpoint `turn_count`.
pub fn revert_checkpoint(thread_id: ThreadId, turn_count: u32) -> ClientCommand {
    ClientCommand::ThreadCheckpointRevert {
        command_id: CommandId::random(),
        thread_id,
        turn_count,
        created_at: now(),
    }
}

/// The stop button. `turn_id` targets a specific turn; `None` interrupts whatever runs.
pub fn interrupt_turn(thread_id: ThreadId, turn_id: Option<TurnId>) -> ClientCommand {
    ClientCommand::ThreadTurnInterrupt {
        command_id: CommandId::random(),
        thread_id,
        turn_id,
        created_at: now(),
    }
}

pub fn respond_to_approval(
    thread_id: ThreadId,
    request_id: ApprovalRequestId,
    decision: ApprovalDecision,
) -> ClientCommand {
    ClientCommand::ThreadApprovalRespond {
        command_id: CommandId::random(),
        thread_id,
        request_id,
        decision,
        created_at: now(),
    }
}

/// Answers a user-input request. `answers` is keyed by question id (the chosen option label,
/// a list of labels for multi-select, or free text). `attachments_by_question_id` carries
/// files attached to individual answers (`capabilities.question_attachments`).
pub fn respond_to_user_input(
    thread_id: ThreadId,
    request_id: ApprovalRequestId,
    answers: BTreeMap<String, Value>,
    attachments_by_question_id: Option<BTreeMap<String, Vec<ChatAttachment>>>,
) -> ClientCommand {
    ClientCommand::ThreadUserInputRespond {
        command_id: CommandId::random(),
        thread_id,
        request_id,
        answers,
        attachments_by_question_id,
        created_at: now(),
    }
}

/// Dismisses an async question (`responseMode: "message"`).
pub fn dismiss_user_input(thread_id: ThreadId, request_id: ApprovalRequestId) -> ClientCommand {
    ClientCommand::ThreadUserInputDismiss {
        command_id: CommandId::random(),
        thread_id,
        request_id,
        created_at: now(),
    }
}

pub fn rename_thread(thread_id: ThreadId, title: impl Into<String>) -> ClientCommand {
    update_thread(
        thread_id,
        ThreadMetaPatch {
            title: Some(title.into().trim().to_owned()),
            ..Default::default()
        },
    )
}

/// Any `thread.meta.update` (model, branch, worktree, title regeneration).
pub fn update_thread(thread_id: ThreadId, patch: ThreadMetaPatch) -> ClientCommand {
    ClientCommand::ThreadMetaUpdate {
        command_id: CommandId::random(),
        thread_id,
        patch,
    }
}

pub fn archive_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadArchive {
        command_id: CommandId::random(),
        thread_id,
    }
}

pub fn unarchive_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadUnarchive {
        command_id: CommandId::random(),
        thread_id,
    }
}

pub fn delete_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadDelete {
        command_id: CommandId::random(),
        thread_id,
    }
}

pub fn settle_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadSettle {
        command_id: CommandId::random(),
        thread_id,
    }
}

pub fn unsettle_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadUnsettle {
        command_id: CommandId::random(),
        thread_id,
        reason: UserReason::User,
    }
}

pub fn set_runtime_mode(thread_id: ThreadId, runtime_mode: RuntimeMode) -> ClientCommand {
    ClientCommand::ThreadRuntimeModeSet {
        command_id: CommandId::random(),
        thread_id,
        runtime_mode,
        created_at: now(),
    }
}

pub fn set_interaction_mode(
    thread_id: ThreadId,
    interaction_mode: InteractionMode,
) -> ClientCommand {
    ClientCommand::ThreadInteractionModeSet {
        command_id: CommandId::random(),
        thread_id,
        interaction_mode,
        created_at: now(),
    }
}

/// Pins a thread to the top of the sidebar (`capabilities.thread_pinning`). `order_key`
/// places it among other pins; `None` appends.
pub fn pin_thread(thread_id: ThreadId, order_key: Option<String>) -> ClientCommand {
    ClientCommand::ThreadPin {
        command_id: CommandId::random(),
        thread_id,
        order_key,
    }
}

pub fn unpin_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadUnpin {
        command_id: CommandId::random(),
        thread_id,
    }
}

/// Moves a pinned thread (`capabilities.thread_pin_reorder`).
pub fn reorder_pinned_thread(thread_id: ThreadId, order_key: impl Into<String>) -> ClientCommand {
    ClientCommand::ThreadPinReorder {
        command_id: CommandId::random(),
        thread_id,
        order_key: order_key.into(),
    }
}

/// Moves an active (unsettled) thread (`capabilities.thread_active_reorder`).
pub fn reorder_active_thread(thread_id: ThreadId, order_key: impl Into<String>) -> ClientCommand {
    ClientCommand::ThreadActiveReorder {
        command_id: CommandId::random(),
        thread_id,
        order_key: order_key.into(),
    }
}

/// Hides a thread until `snoozed_until` (ISO time; `capabilities.thread_snooze`).
pub fn snooze_thread(thread_id: ThreadId, snoozed_until: impl Into<String>) -> ClientCommand {
    ClientCommand::ThreadSnooze {
        command_id: CommandId::random(),
        thread_id,
        snoozed_until: snoozed_until.into(),
    }
}

pub fn unsnooze_thread(thread_id: ThreadId) -> ClientCommand {
    ClientCommand::ThreadUnsnooze {
        command_id: CommandId::random(),
        thread_id,
        reason: UserReason::User,
    }
}

/// Opts a thread in or out of automatic settling (`capabilities.thread_auto_settle_opt_out`).
pub fn set_auto_settle(thread_id: ThreadId, enabled: bool) -> ClientCommand {
    ClientCommand::ThreadAutoSettleSet {
        command_id: CommandId::random(),
        thread_id,
        enabled,
    }
}

/// Stops the thread's provider session. `only_if_settled` skips threads that are working.
pub fn stop_session(thread_id: ThreadId, only_if_settled: bool) -> ClientCommand {
    ClientCommand::ThreadSessionStop {
        command_id: CommandId::random(),
        thread_id,
        created_at: now(),
        only_if_settled: only_if_settled.then_some(true),
    }
}

/// Reverts history (and files when `restore_files`) to `turn_count` turns.
pub fn revert_thread(thread_id: ThreadId, turn_count: u32, restore_files: bool) -> ClientCommand {
    if restore_files {
        ClientCommand::ThreadCheckpointRevert {
            command_id: CommandId::random(),
            thread_id,
            turn_count,
            created_at: now(),
        }
    } else {
        ClientCommand::ThreadConversationRevert {
            command_id: CommandId::random(),
            thread_id,
            turn_count,
            created_at: now(),
        }
    }
}

/// Edits a project (title, scripts, icon, defaults); see
/// [`ProjectMetaPatch`](t3_protocol::commands::ProjectMetaPatch).
pub fn update_project(
    project_id: ProjectId,
    patch: t3_protocol::commands::ProjectMetaPatch,
) -> ClientCommand {
    ClientCommand::ProjectMetaUpdate {
        command_id: CommandId::random(),
        project_id,
        patch,
    }
}
