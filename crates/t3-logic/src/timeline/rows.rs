//! Timeline entries and rows (`session-logic.ts` `deriveTimelineEntries`,
//! `MessagesTimeline.logic.ts` `deriveMessagesTimelineRows` and
//! `computeStableMessagesTimelineRows`).

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

use t3_protocol::{
    MessageId, TurnId,
    orchestration::{
        MessageRole, OrchestrationCheckpointSummary, OrchestrationLatestTurn, OrchestrationMessage,
        OrchestrationProposedPlan, TurnState,
    },
};

use super::{
    format::{elapsed_millis, format_duration},
    work_log::WorkLogEntry,
};
use crate::time::parse_timestamp;

/// Collapsed work groups show only their latest entry (`MAX_VISIBLE_WORK_LOG_ENTRIES`).
const MAX_VISIBLE_WORK_LOG_ENTRIES: usize = 1;

/// What one timeline entry is.
#[derive(Clone, Debug, PartialEq)]
pub enum EntryKind {
    Message(Arc<OrchestrationMessage>),
    ProposedPlan(Arc<OrchestrationProposedPlan>),
    Work(Arc<WorkLogEntry>),
}

/// A message, proposed plan, or work entry in chronological order.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineEntry {
    /// Message id, plan id, or the work entry's presentation key.
    pub id: String,
    pub created_at: String,
    pub kind: EntryKind,
}

impl TimelineEntry {
    fn message(&self) -> Option<&OrchestrationMessage> {
        match &self.kind {
            EntryKind::Message(message) => Some(message),
            _ => None,
        }
    }

    /// The turn an entry is grouped into for folding: assistant messages and work entries only.
    fn fold_turn(&self) -> Option<&TurnId> {
        match &self.kind {
            EntryKind::Message(m) if m.role == MessageRole::Assistant => m.turn_id.as_ref(),
            EntryKind::Work(entry) => entry.turn_id.as_ref(),
            _ => None,
        }
    }
}

/// Merges messages, plans, and work entries by `created_at` (stable, so ties keep messages
/// before plans before work).
///
/// Only user and assistant messages are kept. The fork renders any other role as an empty
/// 16px row, but its server never sends them; nightly sends reasoning as `system` messages,
/// which would add blank gaps the reference UI does not have.
pub fn derive_timeline_entries(
    messages: impl IntoIterator<Item = Arc<OrchestrationMessage>>,
    plans: &[Arc<OrchestrationProposedPlan>],
    work: &[Arc<WorkLogEntry>],
) -> Vec<TimelineEntry> {
    let mut entries: Vec<TimelineEntry> = messages
        .into_iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .map(|message| TimelineEntry {
            id: message.id.to_string(),
            created_at: message.created_at.clone(),
            kind: EntryKind::Message(message),
        })
        .chain(plans.iter().map(|plan| TimelineEntry {
            id: plan.id.to_string(),
            created_at: plan.created_at.clone(),
            kind: EntryKind::ProposedPlan(plan.clone()),
        }))
        .chain(work.iter().map(|entry| TimelineEntry {
            id: entry.presentation_key(),
            created_at: entry.created_at.clone(),
            kind: EntryKind::Work(entry.clone()),
        }))
        .collect();
    entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    entries
}

/// A user or assistant message row.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageRow {
    pub message: Arc<OrchestrationMessage>,
    /// Copy button + timestamp under an assistant message: only the terminal message of a
    /// settled turn.
    pub show_assistant_meta: bool,
    /// The message or its turn is still streaming; the copy button stays hidden.
    pub assistant_copy_streaming: bool,
    /// The checkpoint to revert to from this user message ("Revert to this message").
    pub revert_turn_count: Option<u32>,
}

/// What a row renders.
#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    Message(MessageRow),
    /// Work entries under an optional "Work Log" label.
    Work {
        entries: Vec<Arc<WorkLogEntry>>,
    },
    /// A run of tool calls showing only the latest; expandable to the full history.
    ToolStack {
        group_id: String,
        entries: Vec<Arc<WorkLogEntry>>,
        expanded: bool,
    },
    /// "+N previous log entries" / "Show fewer log entries".
    WorkToggle {
        group_id: String,
        hidden_count: usize,
        expanded: bool,
        only_tool_entries: bool,
    },
    /// "Worked for 1.9s" hiding a settled turn's commentary and tool activity.
    TurnFold {
        turn_id: TurnId,
        label: String,
        expanded: bool,
    },
    ProposedPlan(Arc<OrchestrationProposedPlan>),
    ChangedFiles(Arc<OrchestrationCheckpointSummary>),
    /// "Working for 12s", timed from `started_at`.
    Working {
        started_at: Option<String>,
    },
}

/// One list row.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineRow {
    /// Stable key (`turn-fold:{turn}`, `tool-stack:{entry}`, message id, ...).
    pub id: String,
    /// Set on entries an expanded "Worked for" row reveals; drawn with a left rule.
    pub fold_detail: Option<TurnId>,
    pub kind: RowKind,
}

impl TimelineRow {
    fn new(id: impl Into<String>, kind: RowKind) -> Self {
        Self {
            id: id.into(),
            fold_detail: None,
            kind,
        }
    }

    /// Rows that sit tighter (8px bottom padding instead of 16): work rows and assistant
    /// commentary without a meta row.
    pub fn is_compact(&self) -> bool {
        match &self.kind {
            RowKind::Message(row) => {
                row.message.role == MessageRole::Assistant && !row.show_assistant_meta
            }
            RowKind::Work { .. } | RowKind::ToolStack { .. } | RowKind::WorkToggle { .. } => true,
            _ => false,
        }
    }
}

/// Everything row derivation reads besides the entries.
#[derive(Clone, Copy, Debug)]
pub struct RowsInput<'a> {
    pub entries: &'a [TimelineEntry],
    pub latest_turn: Option<&'a OrchestrationLatestTurn>,
    /// `session.activeTurnId` while the session is running.
    pub running_turn_id: Option<&'a TurnId>,
    pub expanded_turns: &'a HashSet<TurnId>,
    pub expanded_work_groups: &'a HashSet<String>,
    pub is_working: bool,
    pub active_turn_started_at: Option<&'a str>,
    pub checkpoints: &'a [Arc<OrchestrationCheckpointSummary>],
}

/// The last assistant message of every response (per turn, or per user message for messages
/// without a turn).
fn terminal_assistant_message_ids(entries: &[TimelineEntry]) -> HashSet<&str> {
    let mut by_response: HashMap<String, &str> = HashMap::new();
    let mut unkeyed_index = 0usize;
    for message in entries.iter().filter_map(TimelineEntry::message) {
        match message.role {
            MessageRole::User => unkeyed_index += 1,
            MessageRole::Assistant => {
                let key = match &message.turn_id {
                    Some(turn) => format!("turn:{turn}"),
                    None => format!("unkeyed:{unkeyed_index}"),
                };
                by_response.insert(key, message.id.as_str());
            }
            _ => {}
        }
    }
    by_response.into_values().collect()
}

/// The turn that must not fold: the session's running turn, else a latest turn that has not
/// settled. Keyed on turn lifecycle so folding does not flicker right after a send.
fn unsettled_turn_id<'a>(
    latest: Option<&'a OrchestrationLatestTurn>,
    running: Option<&'a TurnId>,
) -> Option<&'a TurnId> {
    if running.is_some() {
        return running;
    }
    let latest = latest?;
    let settled = latest.completed_at.is_some() && latest.state != TurnState::Running;
    (!settled).then_some(&latest.turn_id)
}

struct TurnFold<'a> {
    turn_id: &'a TurnId,
    hidden: HashSet<&'a str>,
    label: String,
}

/// Later of two timestamps; an unparseable one loses (`maxIsoTimestamp`).
fn later<'a>(a: Option<&'a str>, b: &'a str) -> &'a str {
    let Some(a) = a else { return b };
    match (parse_timestamp(a), parse_timestamp(b)) {
        (None, _) => b,
        (_, None) => a,
        (Some(x), Some(y)) => {
            if y > x {
                b
            } else {
                a
            }
        }
    }
}

/// Folds keyed by the id of the turn's first entry. Every settled, non-streaming turn hides all
/// entries but its terminal assistant message.
fn turn_folds<'a>(
    entries: &'a [TimelineEntry],
    terminal_ids: &HashSet<&str>,
    latest: Option<&OrchestrationLatestTurn>,
    unsettled: Option<&TurnId>,
) -> HashMap<&'a str, TurnFold<'a>> {
    struct Group<'a> {
        entries: Vec<&'a TimelineEntry>,
        terminal: Option<&'a OrchestrationMessage>,
        streaming: bool,
        /// The user message that started the turn; entry timestamps alone undercount.
        start: Option<&'a str>,
    }
    let mut order: Vec<&TurnId> = Vec::new();
    let mut groups: HashMap<&TurnId, Group> = HashMap::new();
    let mut pending_user: Option<&str> = None;
    for entry in entries {
        if let Some(message) = entry.message()
            && message.role == MessageRole::User
        {
            pending_user = Some(&message.created_at);
            continue;
        }
        let Some(turn) = entry.fold_turn() else {
            continue;
        };
        let group = groups.entry(turn).or_insert_with(|| {
            order.push(turn);
            // Each user message starts at most one turn; a later turn after the same message
            // (a steer continuation) times from its own first entry.
            Group {
                entries: Vec::new(),
                terminal: None,
                streaming: false,
                start: pending_user.take(),
            }
        });
        group.entries.push(entry);
        if let Some(message) = entry.message() {
            if terminal_ids.contains(message.id.as_str()) {
                group.terminal = Some(message);
            }
            group.streaming |= message.streaming;
        }
    }

    let mut folds = HashMap::new();
    for turn in order {
        let group = &groups[turn];
        if Some(turn) == unsettled || group.streaming {
            continue;
        }
        let terminal_id = group.terminal.map(|m| m.id.as_str());
        let hidden: HashSet<&str> = group
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .filter(|id| Some(*id) != terminal_id)
            .collect();
        let (Some(first), Some(last)) = (group.entries.first(), group.entries.last()) else {
            continue;
        };
        if hidden.is_empty() {
            continue;
        }
        let latest_for_turn = latest.filter(|latest| &latest.turn_id == turn);
        let last_end = last.message().map_or(&last.created_at, |m| &m.updated_at);
        let elapsed = match latest_for_turn
            .and_then(|l| Some((l.started_at.as_deref()?, l.completed_at.as_deref()?)))
        {
            Some((started, completed)) => elapsed_millis(started, completed),
            None => elapsed_millis(
                group.start.unwrap_or(&first.created_at),
                later(group.terminal.map(|m| m.updated_at.as_str()), last_end),
            ),
        };
        let duration = elapsed.map(format_duration);
        let interrupted = latest_for_turn.is_some_and(|l| l.state == TurnState::Interrupted);
        let label = match (interrupted, duration) {
            (true, Some(d)) => format!("You stopped after {d}"),
            (true, None) => "You stopped this response".to_owned(),
            (false, Some(d)) => format!("Worked for {d}"),
            (false, None) => "Worked".to_owned(),
        };
        folds.insert(
            first.id.as_str(),
            TurnFold {
                turn_id: turn,
                hidden,
                label,
            },
        );
    }
    folds
}

/// The checkpoint to revert to from each user message: the first assistant message after it
/// (before the next user message) that has a checkpoint, minus one.
fn revert_turn_counts(
    entries: &[TimelineEntry],
    by_assistant: &HashMap<&MessageId, &Arc<OrchestrationCheckpointSummary>>,
) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(user) = entry.message().filter(|m| m.role == MessageRole::User) else {
            continue;
        };
        for message in entries[index + 1..]
            .iter()
            .filter_map(TimelineEntry::message)
        {
            if message.role == MessageRole::User {
                break;
            }
            if let Some(summary) = by_assistant.get(&message.id) {
                counts.insert(
                    user.id.to_string(),
                    summary.checkpoint_turn_count.saturating_sub(1),
                );
                break;
            }
        }
    }
    counts
}

/// Builds the list rows: folds settled turns, groups consecutive work entries, and appends the
/// working and changed-files rows.
pub fn derive_rows(input: RowsInput) -> Vec<TimelineRow> {
    let entries = input.entries;
    let terminal_ids = terminal_assistant_message_ids(entries);
    let unsettled = unsettled_turn_id(input.latest_turn, input.running_turn_id);
    let folds = turn_folds(entries, &terminal_ids, input.latest_turn, unsettled);

    let mut collapsed: HashSet<&str> = HashSet::new();
    let mut revealed: HashMap<&str, &TurnId> = HashMap::new();
    for fold in folds.values() {
        if input.expanded_turns.contains(fold.turn_id) {
            revealed.extend(fold.hidden.iter().map(|id| (*id, fold.turn_id)));
        } else {
            collapsed.extend(fold.hidden.iter().copied());
        }
    }

    let by_assistant: HashMap<&MessageId, &Arc<OrchestrationCheckpointSummary>> = input
        .checkpoints
        .iter()
        .filter_map(|c| Some((c.assistant_message_id.as_ref()?, c)))
        .collect();
    let revert_counts = revert_turn_counts(entries, &by_assistant);

    let mut rows: Vec<TimelineRow> = Vec::new();
    let mut pending_files: Option<&Arc<OrchestrationCheckpointSummary>> = None;
    let flush = |rows: &mut Vec<TimelineRow>,
                 pending: &mut Option<&Arc<OrchestrationCheckpointSummary>>| {
        if let Some(summary) = pending.take() {
            rows.push(TimelineRow::new(
                format!("changed-files:{}", summary.turn_id),
                RowKind::ChangedFiles(summary.clone()),
            ));
        }
    };

    let mut index = 0;
    while index < entries.len() {
        let entry = &entries[index];
        let entry_turn = match &entry.kind {
            EntryKind::Message(m) if m.role == MessageRole::Assistant => m.turn_id.as_ref(),
            EntryKind::Message(_) => None,
            EntryKind::Work(work) => work.turn_id.as_ref(),
            EntryKind::ProposedPlan(plan) => plan.turn_id.as_ref(),
        };
        if pending_files.is_some_and(|summary| Some(&summary.turn_id) != entry_turn) {
            flush(&mut rows, &mut pending_files);
        }

        if let Some(fold) = folds.get(entry.id.as_str()) {
            rows.push(TimelineRow::new(
                format!("turn-fold:{}", fold.turn_id),
                RowKind::TurnFold {
                    turn_id: fold.turn_id.clone(),
                    label: fold.label.clone(),
                    expanded: input.expanded_turns.contains(fold.turn_id),
                },
            ));
        }
        if collapsed.contains(entry.id.as_str()) {
            index += 1;
            continue;
        }
        let fold_detail = revealed.get(entry.id.as_str()).map(|turn| (*turn).clone());
        let push = |rows: &mut Vec<TimelineRow>, mut row: TimelineRow| {
            row.fold_detail = fold_detail.clone();
            rows.push(row);
        };

        match &entry.kind {
            EntryKind::Work(first) => {
                let mut group = vec![first.clone()];
                let mut cursor = index + 1;
                while let Some(next) = entries.get(cursor) {
                    let EntryKind::Work(work) = &next.kind else {
                        break;
                    };
                    if collapsed.contains(next.id.as_str()) || folds.contains_key(next.id.as_str())
                    {
                        break;
                    }
                    group.push(work.clone());
                    cursor += 1;
                }
                let group_id = format!("work-group:{}", entry.id);
                if group.iter().all(|work| work.is_tool_call()) {
                    // Tool-only stacks keep in-progress entries so the current call replaces
                    // the previous one immediately.
                    let expanded = input.expanded_work_groups.contains(&group_id);
                    push(
                        &mut rows,
                        TimelineRow::new(
                            format!("tool-stack:{}", entry.id),
                            RowKind::ToolStack {
                                group_id,
                                entries: group,
                                expanded,
                            },
                        ),
                    );
                } else {
                    let visible: Vec<_> = group
                        .into_iter()
                        .filter(|work| !work.indicates_neutral())
                        .collect();
                    if visible.len() <= MAX_VISIBLE_WORK_LOG_ENTRIES {
                        if !visible.is_empty() {
                            push(
                                &mut rows,
                                TimelineRow::new(
                                    entry.id.clone(),
                                    RowKind::Work { entries: visible },
                                ),
                            );
                        }
                    } else {
                        let expanded = input.expanded_work_groups.contains(&group_id);
                        let split = visible.len() - MAX_VISIBLE_WORK_LOG_ENTRIES;
                        let shown = if expanded {
                            &visible[..]
                        } else {
                            &visible[split..]
                        };
                        for work in shown {
                            push(
                                &mut rows,
                                TimelineRow::new(
                                    work.id.clone(),
                                    RowKind::Work {
                                        entries: vec![work.clone()],
                                    },
                                ),
                            );
                        }
                        push(
                            &mut rows,
                            TimelineRow::new(
                                format!("work-toggle:{}", entry.id),
                                RowKind::WorkToggle {
                                    group_id,
                                    hidden_count: split,
                                    expanded,
                                    only_tool_entries: false,
                                },
                            ),
                        );
                    }
                }
                index = cursor;
                continue;
            }
            EntryKind::ProposedPlan(plan) => {
                push(
                    &mut rows,
                    TimelineRow::new(entry.id.clone(), RowKind::ProposedPlan(plan.clone())),
                );
            }
            EntryKind::Message(message) => {
                let is_assistant = message.role == MessageRole::Assistant;
                let turn_in_progress =
                    is_assistant && unsettled.is_some() && message.turn_id.as_ref() == unsettled;
                // While the turn runs, the latest assistant message is only provisionally
                // terminal; withhold its meta row until the turn settles.
                let show_assistant_meta =
                    is_assistant && terminal_ids.contains(message.id.as_str()) && !turn_in_progress;
                push(
                    &mut rows,
                    TimelineRow::new(
                        entry.id.clone(),
                        RowKind::Message(MessageRow {
                            message: message.clone(),
                            show_assistant_meta,
                            assistant_copy_streaming: message.streaming || turn_in_progress,
                            revert_turn_count: (message.role == MessageRole::User)
                                .then(|| revert_counts.get(message.id.as_str()).copied())
                                .flatten(),
                        }),
                    ),
                );
                if is_assistant
                    && let Some(summary) = by_assistant.get(&message.id)
                    && !summary.files.is_empty()
                {
                    pending_files = Some(summary);
                }
            }
        }
        index += 1;
    }

    if input.is_working {
        rows.push(TimelineRow::new(
            "working-indicator-row",
            RowKind::Working {
                started_at: input.active_turn_started_at.map(str::to_owned),
            },
        ));
    }
    // Flushed after the working row, so a changed-files card can sit below "Working…".
    flush(&mut rows, &mut pending_files);
    rows
}

/// The range of rows that changed between two derivations (rows compared with `Arc::ptr_eq`
/// after structural sharing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowsDiff {
    pub old: Range<usize>,
    pub new: Range<usize>,
    /// Same keys in the same order: the rows only need remeasuring, which keeps the scroll
    /// anchor. Otherwise rows were inserted or removed and the range must be spliced.
    pub same_keys: bool,
}

/// Reuses each previous row whose content is unchanged (`computeStableMessagesTimelineRows`), so
/// views can skip unchanged rows with `Arc::ptr_eq`.
pub fn share_rows(previous: &[Arc<TimelineRow>], rows: Vec<TimelineRow>) -> Vec<Arc<TimelineRow>> {
    let by_id: HashMap<&str, &Arc<TimelineRow>> =
        previous.iter().map(|row| (row.id.as_str(), row)).collect();
    rows.into_iter()
        .map(|row| match by_id.get(row.id.as_str()) {
            Some(existing) if ***existing == row => Arc::clone(existing),
            _ => Arc::new(row),
        })
        .collect()
}

/// The changed span between two shared row lists, or `None` when identical.
pub fn diff_rows(old: &[Arc<TimelineRow>], new: &[Arc<TimelineRow>]) -> Option<RowsDiff> {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(a, b)| Arc::ptr_eq(a, b))
        .count();
    if prefix == old.len() && prefix == new.len() {
        return None;
    }
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| Arc::ptr_eq(a, b))
        .count();
    let old_range = prefix..old.len() - suffix;
    let new_range = prefix..new.len() - suffix;
    let same_keys = old_range.len() == new_range.len()
        && old[old_range.clone()]
            .iter()
            .zip(&new[new_range.clone()])
            .all(|(a, b)| a.id == b.id);
    Some(RowsDiff {
        old: old_range,
        new: new_range,
        same_keys,
    })
}
