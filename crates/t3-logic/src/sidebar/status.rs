//! Thread status pills (`web/components/Sidebar.logic.ts`, `packages/shared/src/threadStatus.ts`).

use t3_protocol::orchestration::{
    InteractionMode, OrchestrationThreadShell, SessionStatus, TurnState,
};

use crate::time::parse_timestamp;

/// The status pill a thread row shows. "Connecting" (session starting) reuses the Working pill.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadStatus {
    PendingApproval,
    AwaitingInput,
    Error,
    Working,
    PlanReady,
    Completed,
}

impl ThreadStatus {
    /// Pill text and tooltip.
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingApproval => "Pending Approval",
            Self::AwaitingInput => "Awaiting Input",
            Self::Error => "Error",
            Self::Working => "Working",
            Self::PlanReady => "Plan Ready",
            Self::Completed => "Completed",
        }
    }

    /// Whether the dot pulses (`animate-pulse`).
    pub fn pulses(self) -> bool {
        self == Self::Working
    }

    /// Priority for project and "Show more" roll-ups (`THREAD_STATUS_PRIORITY`).
    pub fn priority(self) -> u8 {
        match self {
            Self::PendingApproval | Self::Error => 5,
            Self::AwaitingInput => 4,
            Self::Working => 3,
            Self::PlanReady => 2,
            Self::Completed => 1,
        }
    }
}

fn latest_turn_settled(thread: &OrchestrationThreadShell) -> bool {
    let Some(turn) = &thread.latest_turn else {
        return false;
    };
    let present = |value: &Option<String>| value.as_deref().is_some_and(|value| !value.is_empty());
    present(&turn.started_at)
        && present(&turn.completed_at)
        && thread.session.as_ref().map(|session| &session.status) != Some(&SessionStatus::Running)
}

/// Whether the latest completion happened after the last local visit. Upstream has no server-side
/// acknowledgement, so `last_visited_at` is the only source. A visit exactly 1ms before completion
/// is the "mark unread" marker and always counts as unseen.
pub fn has_unseen_completion(
    thread: &OrchestrationThreadShell,
    last_visited_at: Option<&str>,
) -> bool {
    let Some(completed) = thread
        .latest_turn
        .as_ref()
        .and_then(|turn| turn.completed_at.as_deref())
        .and_then(parse_timestamp)
    else {
        return false;
    };
    match last_visited_at.and_then(parse_timestamp) {
        Some(visited) => completed - visited == 1 || completed > visited,
        None => true,
    }
}

/// The status a thread row shows, first match wins (spec 2.12.2).
pub fn resolve_thread_status(
    thread: &OrchestrationThreadShell,
    last_visited_at: Option<&str>,
) -> Option<ThreadStatus> {
    let session_status = thread.session.as_ref().map(|session| &session.status);
    if thread.has_pending_approvals {
        return Some(ThreadStatus::PendingApproval);
    }
    if thread.has_pending_user_input {
        return Some(ThreadStatus::AwaitingInput);
    }
    let turn_errored = thread
        .latest_turn
        .as_ref()
        .is_some_and(|turn| turn.state == TurnState::Error);
    if session_status == Some(&SessionStatus::Error) || turn_errored {
        return Some(ThreadStatus::Error);
    }
    if matches!(
        session_status,
        Some(SessionStatus::Running | SessionStatus::Starting)
    ) {
        return Some(ThreadStatus::Working);
    }
    if thread.interaction_mode == Some(InteractionMode::Plan)
        && latest_turn_settled(thread)
        && thread.has_actionable_proposed_plan
    {
        return Some(ThreadStatus::PlanReady);
    }
    has_unseen_completion(thread, last_visited_at).then_some(ThreadStatus::Completed)
}

/// Applies the optimistic "just sent a message" override: Working, unless the thread needs the
/// user (approval, input) or errored.
pub fn with_optimistic_work(
    status: Option<ThreadStatus>,
    work_started: bool,
) -> Option<ThreadStatus> {
    match status {
        Some(ThreadStatus::PendingApproval | ThreadStatus::AwaitingInput | ThreadStatus::Error) => {
            status
        }
        _ if work_started => Some(ThreadStatus::Working),
        _ => status,
    }
}

/// The highest-priority status; on ties the first one wins (web `resolveProjectStatusIndicator`).
pub fn highest_status(
    statuses: impl IntoIterator<Item = Option<ThreadStatus>>,
) -> Option<ThreadStatus> {
    statuses
        .into_iter()
        .flatten()
        .fold(None, |best: Option<ThreadStatus>, status| match best {
            Some(best) if status.priority() <= best.priority() => Some(best),
            _ => Some(status),
        })
}
