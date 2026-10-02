//! Thread detail state: messages, activities, plans, checkpoints of one open thread. A pure
//! reducer over `orchestration.subscribeThread` items, ported from upstream
//! `packages/client-runtime/src/state/threadReducer.ts` (`applyThreadDetailEvent`) and the
//! sequence handling in `threads.ts` (protocol.md 5.4).
//!
//! Rows are `Arc`s. Appending a text delta clones only the streaming message (copy on write via
//! `Arc::make_mut`); every other row stays shared with the previous published state.

use std::{cmp::Ordering, sync::Arc};

use serde_json::Value;
use t3_protocol::{
    MessageId, ThreadId, TurnId,
    orchestration::{
        CheckpointStatus, EventBody, MessageRole, OrchestrationCheckpointSummary,
        OrchestrationEvent, OrchestrationLatestTurn, OrchestrationMessage, OrchestrationThread,
        OrchestrationThreadActivity, OrchestrationThreadDetailPage,
        OrchestrationThreadDetailSnapshot, SessionStatus, SettledOverride, ThreadStreamItem,
        TurnState, UnsettleReason,
    },
};

pub use crate::shell::SyncStatus;

/// Paging state for older turns of a windowed snapshot.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThreadPage {
    pub before_cursor: Option<String>,
    pub has_more: bool,
    pub loading_older: bool,
}

/// One open thread.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadState {
    pub thread_id: ThreadId,
    /// `None` until the first snapshot, and after the thread was deleted.
    pub thread: Option<OrchestrationThread>,
    /// Resume cursor: the newest applied snapshot or event sequence.
    pub last_sequence: u64,
    pub status: SyncStatus,
    /// The server deleted the thread.
    pub deleted: bool,
    pub page: Option<ThreadPage>,
    /// Bumped whenever history is replaced (snapshot, revert); stale older-page loads compare
    /// against it and are dropped.
    pub history_epoch: u64,
    /// The latest subscription error, cleared once data arrives again.
    pub error: Option<String>,
}

/// What an event did to a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventEffect {
    Updated,
    Deleted,
    Unchanged,
}

impl ThreadState {
    pub fn new(thread_id: ThreadId) -> Self {
        ThreadState {
            thread_id,
            thread: None,
            last_sequence: 0,
            status: SyncStatus::Empty,
            deleted: false,
            page: None,
            history_epoch: 0,
            error: None,
        }
    }

    /// The `afterSequence` to resubscribe with. Only when there is data to resume.
    pub fn resume_cursor(&self) -> Option<u64> {
        self.thread.as_ref().map(|_| self.last_sequence)
    }

    /// Replaces the thread (including merged older pages) with a snapshot.
    pub fn apply_snapshot(&mut self, snapshot: OrchestrationThreadDetailSnapshot) {
        self.last_sequence = snapshot.snapshot_sequence;
        self.page = snapshot.page.map(page_from_snapshot);
        self.thread = Some(snapshot.thread);
        self.deleted = false;
        self.error = None;
        self.history_epoch += 1;
        if self.status == SyncStatus::Empty {
            self.status = SyncStatus::Cached;
        }
    }

    /// Marks a new subscription (see [`ShellState::begin_sync`](crate::ShellState::begin_sync)).
    pub fn begin_sync(&mut self, completion_marker: bool) {
        self.status = if completion_marker {
            SyncStatus::Synchronizing
        } else {
            SyncStatus::Live
        };
    }

    pub fn end_sync(&mut self, error: Option<String>) {
        self.status = if self.thread.is_some() {
            SyncStatus::Cached
        } else {
            SyncStatus::Empty
        };
        self.error = error;
    }

    /// Applies one stream item. Returns whether the state changed.
    pub fn apply(&mut self, item: ThreadStreamItem) -> bool {
        match item {
            ThreadStreamItem::Snapshot(snapshot) => {
                self.apply_snapshot(*snapshot);
                true
            }
            ThreadStreamItem::Synchronized => {
                let changed = self.status != SyncStatus::Live;
                self.status = SyncStatus::Live;
                changed
            }
            ThreadStreamItem::Event(event) => self.apply_event(*event),
            ThreadStreamItem::Unknown { .. } => false,
        }
    }

    /// Applies one event. Events at or below the cursor are dropped; anything newer advances
    /// it, even when the event does not change this thread.
    pub fn apply_event(&mut self, event: OrchestrationEvent) -> bool {
        if event.sequence <= self.last_sequence {
            return false;
        }
        self.last_sequence = event.sequence;
        let reverted = matches!(event.body, EventBody::ThreadReverted(_));
        // Fresh subscriptions always start with a snapshot, so events without data only
        // happen after a deletion; there is nothing to apply them to.
        let Some(thread) = &mut self.thread else {
            return false;
        };
        match apply_thread_event(thread, event) {
            EventEffect::Updated => {
                if reverted {
                    self.history_epoch += 1;
                }
                true
            }
            EventEffect::Deleted => {
                self.thread = None;
                self.deleted = true;
                self.page = None;
                true
            }
            EventEffect::Unchanged => false,
        }
    }

    /// Prepends an older page fetched over HTTP. Returns `false` (and changes nothing) when
    /// history was replaced since the fetch started (`epoch` differs) or the page is older than
    /// the current state (upstream `threads.ts:589-716`).
    pub fn merge_older_page(
        &mut self,
        epoch: u64,
        snapshot: OrchestrationThreadDetailSnapshot,
    ) -> bool {
        if epoch != self.history_epoch || snapshot.snapshot_sequence < self.last_sequence {
            if let Some(page) = &mut self.page {
                page.loading_older = false;
            }
            return false;
        }
        let Some(thread) = &mut self.thread else {
            return false;
        };
        let older = snapshot.thread;
        prepend_unique(&mut thread.messages, older.messages, |m| &m.id);
        prepend_unique(&mut thread.activities, older.activities, |a| &a.id);
        prepend_unique(&mut thread.proposed_plans, older.proposed_plans, |p| &p.id);
        prepend_unique(&mut thread.checkpoints, older.checkpoints, |c| &c.turn_id);
        self.page = snapshot.page.map(page_from_snapshot);
        true
    }
}

fn page_from_snapshot(page: OrchestrationThreadDetailPage) -> ThreadPage {
    ThreadPage {
        before_cursor: page.before_cursor,
        has_more: page.has_more,
        loading_older: false,
    }
}

fn prepend_unique<T, K: PartialEq>(
    rows: &mut Vec<Arc<T>>,
    older: Vec<Arc<T>>,
    key: impl Fn(&T) -> &K,
) {
    let mut merged: Vec<Arc<T>> = older
        .into_iter()
        .filter(|row| !rows.iter().any(|existing| key(existing) == key(row)))
        .collect();
    merged.append(rows);
    *rows = merged;
}

/// Applies one event to a thread (upstream `applyThreadDetailEvent`). Pure: no IO, no clock.
pub fn apply_thread_event(
    thread: &mut OrchestrationThread,
    event: OrchestrationEvent,
) -> EventEffect {
    let occurred_at = event.occurred_at;
    match event.body {
        EventBody::ProjectCreated(_)
        | EventBody::ProjectMetaUpdated(_)
        | EventBody::ProjectDeleted(_)
        | EventBody::ThreadApprovalResponseRequested(_)
        | EventBody::ThreadUserInputResponseRequested(_)
        | EventBody::ThreadCheckpointRevertRequested(_)
        | EventBody::Unknown { .. } => return EventEffect::Unchanged,

        EventBody::ThreadCreated(p) => {
            *thread = OrchestrationThread {
                id: p.thread_id,
                project_id: p.project_id,
                title: p.title,
                model_selection: p.model_selection,
                runtime_mode: p.runtime_mode.unwrap_or(thread.runtime_mode.clone()),
                interaction_mode: p.interaction_mode,
                branch: p.branch,
                worktree_path: p.worktree_path,
                linked_pull_request: None,
                pull_requests: Vec::new(),
                branch_pull_request: None,
                latest_turn: None,
                created_at: p.created_at,
                updated_at: p.updated_at,
                archived_at: None,
                settled_override: None,
                settled_at: None,
                unsettled_at: None,
                snoozed_until: None,
                snoozed_at: None,
                pinned_at: None,
                pin_order_key: None,
                active_order_key: None,
                auto_settle_disabled_at: None,
                title_regeneration: None,
                title_state: None,
                deleted_at: None,
                messages: Vec::new(),
                proposed_plans: Vec::new(),
                activities: Vec::new(),
                checkpoints: Vec::new(),
                session: None,
            };
        }
        EventBody::ThreadDeleted(_) => return EventEffect::Deleted,
        EventBody::ThreadArchived(p) => {
            thread.archived_at = Some(p.archived_at);
            thread.title_regeneration = None;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadUnarchived(p) => {
            thread.archived_at = None;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadSettled(p) => {
            thread.settled_override = Some(SettledOverride::Settled);
            thread.settled_at = Some(p.settled_at);
            thread.unsettled_at = None;
            thread.active_order_key = None;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadUnsettled(p) => {
            let was_active = thread.settled_override == Some(SettledOverride::Active);
            thread.settled_override =
                (p.reason == UnsettleReason::User).then_some(SettledOverride::Active);
            thread.settled_at = None;
            // A thread already pinned active keeps its re-entry stamp.
            if !was_active {
                thread.unsettled_at = Some(p.updated_at.clone());
            }
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadSnoozed(p) => {
            thread.snoozed_until = Some(p.snoozed_until);
            thread.snoozed_at = Some(p.snoozed_at);
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadUnsnoozed(p) => {
            thread.snoozed_until = None;
            thread.snoozed_at = None;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadPinned(p) => {
            thread.pinned_at = Some(p.pinned_at);
            if p.pin_order_key.is_some() {
                thread.pin_order_key = p.pin_order_key;
            }
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadUnpinned(p) => {
            thread.pinned_at = None;
            thread.pin_order_key = None;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadPinReordered(p) => {
            thread.pin_order_key = Some(p.order_key);
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadAutoSettleSet(p) => {
            thread.auto_settle_disabled_at = p.auto_settle_disabled_at;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadMetaUpdated(p) => {
            if let Some(title) = p.title {
                thread.title = title;
            }
            if let Some(state) = p.title_state {
                thread.title_state = state;
            }
            if let Some(regeneration) = p.title_regeneration {
                thread.title_regeneration = regeneration;
            }
            if let Some(model) = p.model_selection {
                thread.model_selection = model;
            }
            if let Some(branch) = p.branch {
                thread.branch = branch;
            }
            if let Some(path) = p.worktree_path {
                thread.worktree_path = path;
            }
            if let Some(linked) = p.linked_pull_request {
                thread.linked_pull_request = linked;
            }
            if let Some(branch_pr) = p.branch_pull_request {
                thread.branch_pull_request = branch_pr;
            }
            if let Some(key) = p.active_order_key {
                thread.active_order_key = key;
            }
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadPullRequestLinked(p) => {
            let link = p.link;
            thread
                .pull_requests
                .retain(|existing| !existing.same_key(&link.host, &link.repository, link.number));
            thread.pull_requests.push(link);
            sync_linked_pull_request(thread, p.updated_at);
        }
        EventBody::ThreadPullRequestUnlinked(p) => {
            thread
                .pull_requests
                .retain(|existing| !existing.same_key(&p.host, &p.repository, p.number));
            sync_linked_pull_request(thread, p.updated_at);
        }
        EventBody::ThreadPullRequestSynced(p) => {
            let Some(link) = thread
                .pull_requests
                .iter_mut()
                .find(|existing| existing.same_key(&p.host, &p.repository, p.number))
            else {
                return EventEffect::Unchanged;
            };
            link.snapshot = Some(p.snapshot);
            link.stack = p.stack;
            sync_linked_pull_request(thread, p.updated_at);
        }
        EventBody::ThreadRuntimeModeSet(p) => {
            thread.runtime_mode = p.runtime_mode;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadInteractionModeSet(p) => {
            thread.interaction_mode = p.interaction_mode;
            thread.updated_at = p.updated_at;
        }
        EventBody::ThreadTurnStartRequested(p) => {
            if let Some(model) = p.model_selection {
                thread.model_selection = model;
            }
            if let Some(mode) = p.runtime_mode {
                thread.runtime_mode = mode;
            }
            thread.interaction_mode = p.interaction_mode;
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadTurnInterruptRequested(p) => {
            let Some(turn_id) = p.turn_id else {
                return EventEffect::Unchanged;
            };
            let Some(latest) = &mut thread.latest_turn else {
                return EventEffect::Unchanged;
            };
            if latest.turn_id != turn_id {
                return EventEffect::Unchanged;
            }
            latest.state = TurnState::Interrupted;
            latest
                .started_at
                .get_or_insert_with(|| p.created_at.clone());
            latest.completed_at.get_or_insert(p.created_at);
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadMessageSent(p) => {
            apply_message_sent(thread, p);
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadSessionSet(p) => {
            let session = p.session;
            let latest = &mut thread.latest_turn;
            match (&session.status, &session.active_turn_id) {
                (SessionStatus::Running, Some(active)) => {
                    let same = latest.as_ref().filter(|t| &t.turn_id == active);
                    let next = OrchestrationLatestTurn {
                        turn_id: active.clone(),
                        state: TurnState::Running,
                        requested_at: same
                            .map(|t| t.requested_at.clone())
                            .unwrap_or_else(|| session.updated_at.clone()),
                        started_at: Some(
                            same.and_then(|t| t.started_at.clone())
                                .unwrap_or_else(|| session.updated_at.clone()),
                        ),
                        completed_at: None,
                        assistant_message_id: same.and_then(|t| t.assistant_message_id.clone()),
                        source_proposed_plan: same.and_then(|t| t.source_proposed_plan.clone()),
                    };
                    *latest = Some(next);
                }
                _ => {
                    // Leaving `running` ends the turn: settle a still-running latest turn.
                    if let (Some(turn), Some(state)) =
                        (latest.as_mut(), settled_turn_state(&session.status))
                        && turn.state == TurnState::Running
                    {
                        turn.state = state;
                        turn.completed_at = Some(session.updated_at.clone());
                    }
                }
            }
            thread.session = Some(session);
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadSessionStopRequested(p) => {
            let Some(session) = &mut thread.session else {
                return EventEffect::Unchanged;
            };
            session.status = SessionStatus::Stopped;
            session.active_turn_id = None;
            session.updated_at = p.created_at;
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadProposedPlanUpserted(p) => {
            let plan = p.proposed_plan;
            thread
                .proposed_plans
                .retain(|existing| existing.id != plan.id);
            thread.proposed_plans.push(plan);
            thread.proposed_plans.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadTurnDiffCompleted(p) => {
            let existing = thread.checkpoints.iter().find(|c| c.turn_id == p.turn_id);
            // Never overwrite a real checkpoint with a mid-turn placeholder.
            if existing.is_some_and(|c| c.status != CheckpointStatus::Missing)
                && p.status == CheckpointStatus::Missing
            {
                return EventEffect::Unchanged;
            }
            let checkpoint = OrchestrationCheckpointSummary {
                turn_id: p.turn_id.clone(),
                checkpoint_turn_count: p.checkpoint_turn_count,
                checkpoint_ref: p.checkpoint_ref,
                status: p.status.clone(),
                files: p.files,
                assistant_message_id: p.assistant_message_id.clone(),
                completed_at: p.completed_at.clone(),
            };
            thread.checkpoints.retain(|c| c.turn_id != p.turn_id);
            thread.checkpoints.push(Arc::new(checkpoint));
            thread.checkpoints.sort_by_key(|c| c.checkpoint_turn_count);
            // Mid-turn diffs record the checkpoint but must not settle a running turn.
            let still_running = session_running_turn(thread, &p.turn_id);
            let applies = thread
                .latest_turn
                .as_ref()
                .is_none_or(|t| t.turn_id == p.turn_id);
            if !still_running && applies {
                let previous = thread.latest_turn.take();
                let state = if previous
                    .as_ref()
                    .is_some_and(|t| t.state == TurnState::Interrupted)
                {
                    TurnState::Interrupted
                } else {
                    checkpoint_turn_state(&p.status)
                };
                thread.latest_turn = Some(OrchestrationLatestTurn {
                    turn_id: p.turn_id,
                    state,
                    requested_at: previous
                        .as_ref()
                        .map(|t| t.requested_at.clone())
                        .unwrap_or_else(|| p.completed_at.clone()),
                    started_at: Some(
                        previous
                            .as_ref()
                            .and_then(|t| t.started_at.clone())
                            .unwrap_or_else(|| p.completed_at.clone()),
                    ),
                    completed_at: Some(p.completed_at),
                    assistant_message_id: p.assistant_message_id,
                    source_proposed_plan: previous.and_then(|t| t.source_proposed_plan),
                });
            }
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadReverted(p) => {
            apply_revert(thread, p.turn_count);
            thread.updated_at = occurred_at;
        }
        EventBody::ThreadActivityAppended(p) => {
            apply_activity(thread, p.activity);
            thread.updated_at = occurred_at;
        }
    }
    EventEffect::Updated
}

/// Keeps the legacy `linked_pull_request` only while a matching link still exists.
fn sync_linked_pull_request(thread: &mut OrchestrationThread, updated_at: String) {
    let keep = thread.linked_pull_request.as_ref().is_some_and(|linked| {
        thread.pull_requests.iter().any(|link| {
            link.source != t3_protocol::orchestration::PullRequestLinkSource::StackDismissed
                && link.url == linked.url
        })
    });
    if !keep {
        thread.linked_pull_request = None;
    }
    thread.updated_at = updated_at;
}

fn session_running_turn(thread: &OrchestrationThread, turn_id: &TurnId) -> bool {
    thread.session.as_ref().is_some_and(|s| {
        s.status == SessionStatus::Running && s.active_turn_id.as_ref() == Some(turn_id)
    })
}

/// Turn state for a session that left `running`, or `None` while it is (re)starting/running.
fn settled_turn_state(status: &SessionStatus) -> Option<TurnState> {
    match status {
        SessionStatus::Idle | SessionStatus::Ready => Some(TurnState::Completed),
        SessionStatus::Error => Some(TurnState::Error),
        SessionStatus::Interrupted | SessionStatus::Stopped => Some(TurnState::Interrupted),
        SessionStatus::Starting | SessionStatus::Running | SessionStatus::Other(_) => None,
    }
}

fn checkpoint_turn_state(status: &CheckpointStatus) -> TurnState {
    match status {
        CheckpointStatus::Error => TurnState::Error,
        _ => TurnState::Completed,
    }
}

/// `thread.message-sent`: append a streaming delta, replace on completion (completion events
/// carry `""`, which keeps the accumulated text), or add a new message. Assistant messages
/// bound to a turn update `latest_turn` and rebind the turn's checkpoint.
fn apply_message_sent(
    thread: &mut OrchestrationThread,
    p: t3_protocol::orchestration::ThreadMessageSentPayload,
) {
    // Recent messages are at the end; search from the back.
    match thread
        .messages
        .iter_mut()
        .rev()
        .find(|m| m.id == p.message_id)
    {
        Some(existing) => {
            let message = Arc::make_mut(existing);
            if p.streaming {
                message.text.push_str(&p.text);
            } else if !p.text.is_empty() {
                message.text = p.text.clone();
            }
            message.streaming = p.streaming;
            message.turn_id = p.turn_id.clone();
            if !p.streaming {
                message.updated_at = p.updated_at.clone();
            }
            if p.attachments.is_some() {
                message.attachments = p.attachments.clone();
            }
            if p.context.is_some() {
                message.context = p.context.clone();
            }
        }
        None => thread.messages.push(Arc::new(OrchestrationMessage {
            id: p.message_id.clone(),
            role: p.role.clone(),
            text: p.text.clone(),
            attachments: p.attachments.clone(),
            context: p.context.clone(),
            turn_id: p.turn_id.clone(),
            streaming: p.streaming,
            created_at: p.created_at.clone(),
            updated_at: p.updated_at.clone(),
        })),
    }

    let (MessageRole::Assistant, Some(turn_id)) = (&p.role, &p.turn_id) else {
        return;
    };
    let turn_still_running = session_running_turn(thread, turn_id);
    let settles_turn = !p.streaming && !turn_still_running;
    let latest = &mut thread.latest_turn;
    if latest.as_ref().is_none_or(|t| &t.turn_id == turn_id) {
        let same = latest.as_ref();
        let state = if settles_turn {
            match same.map(|t| &t.state) {
                Some(TurnState::Interrupted) => TurnState::Interrupted,
                Some(TurnState::Error) => TurnState::Error,
                _ => TurnState::Completed,
            }
        } else {
            TurnState::Running
        };
        let next = OrchestrationLatestTurn {
            turn_id: turn_id.clone(),
            state,
            requested_at: same
                .map(|t| t.requested_at.clone())
                .unwrap_or_else(|| p.created_at.clone()),
            started_at: Some(
                same.and_then(|t| t.started_at.clone())
                    .unwrap_or_else(|| p.created_at.clone()),
            ),
            completed_at: if settles_turn {
                Some(p.updated_at.clone())
            } else {
                same.and_then(|t| t.completed_at.clone())
            },
            assistant_message_id: Some(p.message_id.clone()),
            source_proposed_plan: same.and_then(|t| t.source_proposed_plan.clone()),
        };
        if latest.as_ref() != Some(&next) {
            *latest = Some(next);
        }
    }
    rebind_checkpoint(&mut thread.checkpoints, turn_id, &p.message_id);
}

fn rebind_checkpoint(
    checkpoints: &mut [Arc<OrchestrationCheckpointSummary>],
    turn_id: &TurnId,
    message_id: &MessageId,
) {
    for checkpoint in checkpoints.iter_mut() {
        if &checkpoint.turn_id == turn_id
            && checkpoint.assistant_message_id.as_ref() != Some(message_id)
        {
            Arc::make_mut(checkpoint).assistant_message_id = Some(message_id.clone());
        }
    }
}

/// `thread.reverted`: keep checkpoints up to `turn_count` and the rows of retained turns.
fn apply_revert(thread: &mut OrchestrationThread, turn_count: u32) {
    thread
        .checkpoints
        .retain(|c| c.checkpoint_turn_count <= turn_count);
    thread.checkpoints.sort_by_key(|c| c.checkpoint_turn_count);
    let retained: Vec<TurnId> = thread
        .checkpoints
        .iter()
        .map(|c| c.turn_id.clone())
        .collect();
    let retained_turn = |turn: &Option<TurnId>| turn.as_ref().is_none_or(|t| retained.contains(t));

    thread.messages = retain_messages_after_revert(&thread.messages, &retained, turn_count);
    thread.proposed_plans.retain(|p| retained_turn(&p.turn_id));
    thread.activities.retain(|a| retained_turn(&a.turn_id));
    thread.latest_turn = thread.checkpoints.last().map(|c| OrchestrationLatestTurn {
        turn_id: c.turn_id.clone(),
        state: checkpoint_turn_state(&c.status),
        requested_at: c.completed_at.clone(),
        started_at: Some(c.completed_at.clone()),
        completed_at: Some(c.completed_at.clone()),
        assistant_message_id: c.assistant_message_id.clone(),
        source_proposed_plan: None,
    });
}

fn is_imported_message(id: &MessageId) -> bool {
    id.as_str().starts_with("import:")
}

/// Upstream `retainMessagesAfterRevert`: system and imported messages always stay; turn-bound
/// messages stay with their turn; then, per role, the oldest unbound messages fill up to
/// `turn_count` (user messages from `thread.turn.start` have no turn id).
fn retain_messages_after_revert(
    messages: &[Arc<OrchestrationMessage>],
    retained_turns: &[TurnId],
    turn_count: u32,
) -> Vec<Arc<OrchestrationMessage>> {
    let mut keep: Vec<&MessageId> = messages
        .iter()
        .filter(|m| {
            m.role == MessageRole::System
                || is_imported_message(&m.id)
                || m.turn_id
                    .as_ref()
                    .is_some_and(|t| retained_turns.contains(t))
        })
        .map(|m| &m.id)
        .collect();
    for role in [MessageRole::User, MessageRole::Assistant] {
        let retained = messages
            .iter()
            .filter(|m| m.role == role && !is_imported_message(&m.id) && keep.contains(&&m.id))
            .count();
        let missing = (turn_count as usize).saturating_sub(retained);
        let mut fallback: Vec<&Arc<OrchestrationMessage>> = messages
            .iter()
            .filter(|m| {
                m.role == role
                    && !keep.contains(&&m.id)
                    && m.turn_id
                        .as_ref()
                        .is_none_or(|t| retained_turns.contains(t))
            })
            .collect();
        fallback.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        keep.extend(fallback.into_iter().take(missing).map(|m| &m.id));
    }
    messages
        .iter()
        .filter(|m| keep.contains(&&m.id))
        .cloned()
        .collect()
}

/// Activity order: provider sequence (missing last), then `created_at`, then id.
fn activity_order(a: &OrchestrationThreadActivity, b: &OrchestrationThreadActivity) -> Ordering {
    a.sequence
        .unwrap_or(i64::MAX)
        .cmp(&b.sequence.unwrap_or(i64::MAX))
        .then_with(|| a.created_at.cmp(&b.created_at))
        .then_with(|| a.id.cmp(&b.id))
}

/// A `context-window.updated` row with a usable `usedTokens`. A newer one replaces earlier
/// ones of the same turn.
fn is_resolvable_context_window(activity: &OrchestrationThreadActivity) -> bool {
    activity.kind == "context-window.updated"
        && activity
            .payload
            .get("usedTokens")
            .and_then(Value::as_f64)
            .is_some_and(|tokens| tokens.is_finite() && tokens >= 0.0)
}

/// `thread.activity-appended`: upsert by id (stable ids replace earlier rows), keep sorted.
fn apply_activity(thread: &mut OrchestrationThread, activity: Arc<OrchestrationThreadActivity>) {
    let activities = &mut thread.activities;
    let supersedes = is_resolvable_context_window(&activity);
    let known = activities.iter().any(|a| a.id == activity.id);
    // Live streams append in order; skip the re-sort for that common case.
    if !supersedes
        && !known
        && activities
            .last()
            .is_none_or(|last| activity_order(last, &activity) != Ordering::Greater)
    {
        activities.push(activity);
        return;
    }
    activities.retain(|existing| {
        existing.id != activity.id
            && !(supersedes
                && existing.turn_id == activity.turn_id
                && is_resolvable_context_window(existing))
    });
    activities.push(activity);
    activities.sort_by(|a, b| activity_order(a, b));
}
