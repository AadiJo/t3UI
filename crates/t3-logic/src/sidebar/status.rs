//! Thread status pills and row status (fork `web/components/Sidebar.logic.ts`, shared by the
//! inbox sidebar and the legacy project sidebar).

use t3_protocol::orchestration::{
    BackgroundLiveness, InteractionMode, OrchestrationThreadShell, SessionStatus,
};

use crate::time::{EpochMillis, parse_timestamp};

/// The status pill a thread row shows (`ThreadStatusPill.label`). There is no Error pill: a
/// failed session shows on the row as [`SidebarThreadStatus::Failed`] instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadStatus {
    PendingApproval,
    AwaitingInput,
    /// Session running, or background work that is not only a watch loop.
    Working,
    /// Session starting.
    Connecting,
    PlanReady,
    /// Only watch loops are live (a parent agent babysitting checks).
    Monitoring,
    Completed,
}

impl ThreadStatus {
    /// Pill text and tooltip.
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingApproval => "Pending Approval",
            Self::AwaitingInput => "Awaiting Input",
            Self::Working => "Working",
            Self::Connecting => "Connecting",
            Self::PlanReady => "Plan Ready",
            Self::Monitoring => "Monitoring",
            Self::Completed => "Completed",
        }
    }

    /// Whether the dot pulses (`animate-pulse`).
    pub fn pulses(self) -> bool {
        matches!(self, Self::Working | Self::Connecting)
    }

    /// Priority for project roll-ups (`THREAD_STATUS_PRIORITY`): attention, active work, the
    /// plan prompt, then passive monitoring.
    pub fn priority(self) -> u8 {
        match self {
            Self::PendingApproval => 6,
            Self::AwaitingInput => 5,
            Self::Working | Self::Connecting => 4,
            Self::PlanReady => 3,
            Self::Monitoring => 2,
            Self::Completed => 1,
        }
    }
}

/// The latest turn finished and the session is not running it (`isLatestTurnSettled`).
fn latest_turn_settled(thread: &OrchestrationThreadShell) -> bool {
    let Some(turn) = &thread.latest_turn else {
        return false;
    };
    let present = |value: &Option<String>| value.as_deref().is_some_and(|value| !value.is_empty());
    present(&turn.started_at)
        && present(&turn.completed_at)
        && thread.session.as_ref().map(|session| &session.status) != Some(&SessionStatus::Running)
}

/// Whether the latest completion is newer than the last local visit (`hasUnseenCompletion`).
/// Unread state is client-local. A thread with no recorded visit has nothing unseen; an
/// unparseable visit counts as unseen. "Mark unread" stores `completedAt - 1ms`.
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
    let Some(visited) = last_visited_at else {
        return false;
    };
    parse_timestamp(visited).is_none_or(|visited| completed > visited)
}

/// The pill a thread shows, first match wins (`resolveThreadStatusPill`).
pub fn resolve_thread_status(
    thread: &OrchestrationThreadShell,
    last_visited_at: Option<&str>,
) -> Option<ThreadStatus> {
    if thread.has_pending_approvals {
        return Some(ThreadStatus::PendingApproval);
    }
    if thread.has_pending_user_input {
        return Some(ThreadStatus::AwaitingInput);
    }
    match thread.session.as_ref().map(|session| &session.status) {
        Some(SessionStatus::Running) => return Some(ThreadStatus::Working),
        Some(SessionStatus::Starting) => return Some(ThreadStatus::Connecting),
        _ => {}
    }
    // A plan prompt needs a decision; it outranks background work.
    if thread.interaction_mode == Some(InteractionMode::Plan)
        && latest_turn_settled(thread)
        && thread.has_actionable_proposed_plan
    {
        return Some(ThreadStatus::PlanReady);
    }
    let unseen = has_unseen_completion(thread, last_visited_at);
    match thread.background_liveness {
        Some(BackgroundLiveness::Working) => Some(ThreadStatus::Working),
        Some(BackgroundLiveness::Monitoring) if !unseen => Some(ThreadStatus::Monitoring),
        _ => unseen.then_some(ThreadStatus::Completed),
    }
}

/// Applies the optimistic "just sent a message" override: Working, unless the thread needs the
/// user.
pub fn with_optimistic_work(
    status: Option<ThreadStatus>,
    work_started: bool,
) -> Option<ThreadStatus> {
    match status {
        Some(ThreadStatus::PendingApproval | ThreadStatus::AwaitingInput) => status,
        _ if work_started => Some(ThreadStatus::Working),
        _ => status,
    }
}

/// The highest-priority status; on ties the first one wins (`resolveProjectStatusIndicator`).
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

/// What a row is doing, for its icon and whether it recedes (`SidebarThreadStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidebarThreadStatus {
    Approval,
    Input,
    Working,
    Monitoring,
    Failed,
    Ready,
}

/// `resolveSidebarThreadStatus`: attention first, then live work, then a failure (which beats
/// lingering background liveness), then background work.
pub fn resolve_sidebar_thread_status(thread: &OrchestrationThreadShell) -> SidebarThreadStatus {
    if thread.has_pending_approvals {
        return SidebarThreadStatus::Approval;
    }
    if thread.has_pending_user_input {
        return SidebarThreadStatus::Input;
    }
    match thread.session.as_ref().map(|session| &session.status) {
        Some(SessionStatus::Running | SessionStatus::Starting) => SidebarThreadStatus::Working,
        Some(SessionStatus::Error) => SidebarThreadStatus::Failed,
        _ => match thread.background_liveness {
            Some(BackgroundLiveness::Working) => SidebarThreadStatus::Working,
            Some(BackgroundLiveness::Monitoring) => SidebarThreadStatus::Monitoring,
            _ => SidebarThreadStatus::Ready,
        },
    }
}

/// Inputs of [`should_recede_sidebar_thread`].
#[derive(Clone, Copy, Debug)]
pub struct RecedeInput {
    pub status: SidebarThreadStatus,
    /// [`has_unseen_completion`].
    pub is_unread: bool,
    /// Woke from snooze since the last visit, and not settled.
    pub is_woke: bool,
    /// The route shows this thread.
    pub is_active: bool,
    pub is_selected: bool,
}

/// Whether the row renders dimmed (`shouldRecedeSidebarThread`): background work recedes
/// unless active or selected; ready and approval rows keep prominence while unread or woke;
/// input rows and failures never recede.
pub fn should_recede_sidebar_thread(input: RecedeInput) -> bool {
    if input.is_active || input.is_selected || input.status == SidebarThreadStatus::Input {
        return false;
    }
    match input.status {
        SidebarThreadStatus::Working => true,
        SidebarThreadStatus::Monitoring => !input.is_unread,
        SidebarThreadStatus::Ready | SidebarThreadStatus::Approval => {
            !input.is_unread && !input.is_woke
        }
        SidebarThreadStatus::Input | SidebarThreadStatus::Failed => false,
    }
}

/// Belongs in the beta Working shelf: busy with work that does not need the user. A plan
/// prompt keeps it in the inbox (`isSidebarThreadWorking`).
pub fn is_sidebar_thread_working(thread: &OrchestrationThreadShell) -> bool {
    matches!(
        resolve_sidebar_thread_status(thread),
        SidebarThreadStatus::Working | SidebarThreadStatus::Monitoring
    ) && resolve_thread_status(thread, None) != Some(ThreadStatus::PlanReady)
}

/// What a working row's elapsed label counts from: the running turn's start (or request until
/// adoption), else the session's last transition. Malformed stamps fall through.
pub fn resolve_working_started_at(thread: &OrchestrationThreadShell) -> Option<String> {
    let valid = |value: Option<&str>| {
        value
            .filter(|v| parse_timestamp(v).is_some())
            .map(str::to_owned)
    };
    let session_at = thread.session.as_ref().map(|s| s.updated_at.as_str());
    match &thread.latest_turn {
        Some(turn) if turn.completed_at.is_none() => valid(turn.started_at.as_deref())
            .or_else(|| valid(Some(&turn.requested_at)))
            .or_else(|| valid(session_at)),
        _ => valid(session_at),
    }
}

/// "42s", "5m", "2h 5m" (`formatWorkingDurationLabel`).
pub fn format_working_duration_label(elapsed: EpochMillis) -> String {
    let seconds = elapsed.max(0) / 1_000;
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}
