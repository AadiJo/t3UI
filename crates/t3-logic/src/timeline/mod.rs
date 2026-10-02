//! The chat timeline as the web client derives it (`session-logic.ts`,
//! `MessagesTimeline.logic.ts`): work log entries from activities, timeline entries from
//! messages + plans + work, and list rows with "Worked for" folds, tool stacks, work toggles,
//! and the working / changed-files rows.
//!
//! `t3-app`'s chat view keeps one [`TimelineModel`] per open thread, feeds it every new
//! `ThreadState`, and splices its list with the returned [`RowsDiff`]:
//!
//! ```ignore
//! let diff = model.update(TimelineInput { thread: &thread, scope: &key, .. });
//! for row in model.rows() { /* render by row.kind */ }
//! ```

mod format;
pub mod plan;
mod rows;
mod text;
mod work_log;

#[cfg(test)]
mod tests;

use std::{collections::HashSet, sync::Arc};

use t3_protocol::{
    TurnId,
    orchestration::{
        OrchestrationLatestTurn, OrchestrationMessage, OrchestrationSession, OrchestrationThread,
        OrchestrationThreadActivity, SessionStatus,
    },
};

pub use format::{
    elapsed_millis, format_duration, format_short_timestamp, format_timestamp,
    format_timestamp_tooltip, format_working_timer, format_workspace_relative_path,
};
pub use rows::{
    EntryKind, MessageRow, RowKind, RowsDiff, RowsInput, TimelineEntry, TimelineRow, derive_rows,
    derive_timeline_entries, diff_rows, share_rows,
};
pub use text::normalize_compact_tool_label;
pub use work_log::{
    RequestKind, ToolItemType, ToolStatus, WorkLogEntry, WorkLogPresenter, WorkTone,
    compare_activities, derive_work_log_entries,
};

/// Provider session phase (`derivePhase`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionPhase {
    Disconnected,
    Connecting,
    Running,
    Ready,
}

impl SessionPhase {
    pub fn of(session: Option<&OrchestrationSession>) -> Self {
        match session.map(|s| &s.status) {
            None
            | Some(SessionStatus::Stopped | SessionStatus::Interrupted | SessionStatus::Error) => {
                Self::Disconnected
            }
            Some(SessionStatus::Starting) => Self::Connecting,
            Some(SessionStatus::Running) => Self::Running,
            Some(_) => Self::Ready,
        }
    }
}

/// The session's running turn, if it is running one.
pub fn running_turn_id(session: Option<&OrchestrationSession>) -> Option<&TurnId> {
    session
        .filter(|s| s.status == SessionStatus::Running)
        .and_then(|s| s.active_turn_id.as_ref())
}

/// Whether the latest turn finished and the session is not running (`isLatestTurnSettled`).
pub fn is_latest_turn_settled(
    latest: Option<&OrchestrationLatestTurn>,
    session: Option<&OrchestrationSession>,
) -> bool {
    let Some(latest) = latest else { return false };
    if latest.started_at.is_none() || latest.completed_at.is_none() {
        return false;
    }
    session.is_none_or(|s| s.status != SessionStatus::Running)
}

/// When the "Working for" timer starts (`deriveActiveWorkStartedAt`). The local send time wins
/// so the timer never resets when the server's turn arrives.
pub fn active_work_started_at(
    latest: Option<&OrchestrationLatestTurn>,
    session: Option<&OrchestrationSession>,
    send_started_at: Option<&str>,
) -> Option<String> {
    if let Some(running) = running_turn_id(session) {
        return match latest.filter(|l| &l.turn_id == running) {
            Some(latest) => send_started_at
                .map(str::to_owned)
                .or_else(|| latest.started_at.clone()),
            None => send_started_at.map(str::to_owned),
        };
    }
    if !is_latest_turn_settled(latest, session) {
        return latest
            .and_then(|l| l.started_at.clone())
            .or_else(|| send_started_at.map(str::to_owned));
    }
    send_started_at.map(str::to_owned)
}

/// One update's input.
pub struct TimelineInput<'a> {
    pub thread: &'a OrchestrationThread,
    /// Thread key (environment + thread); presentation state resets when it changes.
    pub scope: &'a str,
    /// User messages sent from this client that the server has not echoed yet.
    pub optimistic: &'a [Arc<OrchestrationMessage>],
    pub expanded_turns: &'a HashSet<TurnId>,
    pub expanded_work_groups: &'a HashSet<String>,
    /// Phase running/connecting, a local send in flight, or a revert in progress.
    pub is_working: bool,
    pub active_turn_started_at: Option<&'a str>,
}

/// Derived rows for one open thread, with the state that keeps them stable between updates:
/// work-row presentation and structurally shared rows.
#[derive(Default)]
pub struct TimelineModel {
    presenter: WorkLogPresenter,
    /// Activities the cached work entries were derived from (compared by pointer).
    activities: Option<Vec<Arc<OrchestrationThreadActivity>>>,
    work: Vec<Arc<WorkLogEntry>>,
    rows: Vec<Arc<TimelineRow>>,
}

impl TimelineModel {
    /// Re-derives the rows. Returns the changed span, or `None` when nothing changed.
    pub fn update(&mut self, input: TimelineInput) -> Option<RowsDiff> {
        let thread = input.thread;
        let activities_unchanged = self.activities.as_ref().is_some_and(|previous| {
            previous.len() == thread.activities.len()
                && previous
                    .iter()
                    .zip(&thread.activities)
                    .all(|(a, b)| Arc::ptr_eq(a, b))
        });
        if !activities_unchanged {
            let entries = self
                .presenter
                .present(input.scope, derive_work_log_entries(&thread.activities));
            self.work = entries.into_iter().map(Arc::new).collect();
            self.activities = Some(thread.activities.clone());
        }

        let echoed: HashSet<&str> = thread.messages.iter().map(|m| m.id.as_str()).collect();
        let messages = thread.messages.iter().cloned().chain(
            input
                .optimistic
                .iter()
                .filter(|m| !echoed.contains(m.id.as_str()))
                .cloned(),
        );
        let entries = derive_timeline_entries(messages, &thread.proposed_plans, &self.work);
        let rows = derive_rows(RowsInput {
            entries: &entries,
            latest_turn: thread.latest_turn.as_ref(),
            running_turn_id: running_turn_id(thread.session.as_ref()),
            expanded_turns: input.expanded_turns,
            expanded_work_groups: input.expanded_work_groups,
            is_working: input.is_working,
            active_turn_started_at: input.active_turn_started_at,
            checkpoints: &thread.checkpoints,
        });
        let rows = share_rows(&self.rows, rows);
        let diff = diff_rows(&self.rows, &rows);
        self.rows = rows;
        diff
    }

    /// The current rows.
    pub fn rows(&self) -> &[Arc<TimelineRow>] {
        &self.rows
    }
}
