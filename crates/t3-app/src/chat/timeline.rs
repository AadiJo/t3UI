//! The timeline list (`MessagesTimeline.tsx`): rows from `t3_logic::timeline`, the GPUI list
//! that virtualizes them, local disclosure state, and the scroll rules (spec 3.12).
//!
//! Scrolling maps onto `ListState`'s tail following:
//! - following-end: `FollowMode::Tail` while following; content growth keeps the end in view.
//! - free-scrolling: wheel-up or a scrollbar drag stops following (GPUI does this); scrolling
//!   back to the bottom resumes it. The scroll-to-end pill shows while not following and not
//!   at the end.
//! - anchored disclosures: expanding a fold, stack, or toggle pauses following first, so the
//!   row the user clicked stays put instead of the list jumping to the new end.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use gpui_kit::{App, AppContext as _, Context, Entity, FollowMode, ListAlignment, ListState, px};
use t3_diff::{
    ChangedFilesTree,
    tree::{ChangedFile, DiffStat},
};
use t3_logic::timeline::{RowKind, TimelineInput, TimelineModel, TimelineRow};
use t3_protocol::{
    TurnId,
    orchestration::{OrchestrationCheckpointSummary, OrchestrationMessage, OrchestrationThread},
};

use super::ChatView;

/// Rows rendered beyond the viewport so short scrolls never show blank space.
const OVERDRAW: f32 = 600.;

/// The list state, rows, and per-row UI state of one thread's timeline.
pub(super) struct Timeline {
    pub list: ListState,
    model: TimelineModel,
    /// "Worked for" folds the user opened.
    pub expanded_turns: HashSet<TurnId>,
    /// Work groups and tool stacks the user opened (`work-group:{entry}`).
    pub expanded_groups: HashSet<String>,
    /// Work entries showing their command/detail body, by presentation key.
    pub expanded_entries: HashSet<String>,
    /// User messages showing their full text.
    pub expanded_messages: HashSet<String>,
    /// Plan cards showing the whole plan.
    pub expanded_plans: HashSet<String>,
    /// "Collapse all" / "Expand all" on a changed-files card, by turn.
    pub changed_files_expanded: HashMap<TurnId, bool>,
    /// Changed-files trees, by turn.
    trees: HashMap<
        TurnId,
        (
            Arc<OrchestrationCheckpointSummary>,
            Entity<ChangedFilesTree>,
        ),
    >,
    /// The copy button showing its check, by key, until the 1s reset.
    pub copied: Option<String>,
    /// Inputs of the last derivation, kept so local toggles can re-derive.
    last: Option<LastInput>,
}

struct LastInput {
    thread: OrchestrationThread,
    scope: String,
    optimistic: Vec<Arc<OrchestrationMessage>>,
    is_working: bool,
    started_at: Option<String>,
}

impl Timeline {
    pub fn new(cx: &mut Context<ChatView>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(OVERDRAW));
        list.set_follow_mode(FollowMode::Tail);
        let view = cx.weak_entity();
        // Re-render on scroll so the scroll-to-end pill follows the position.
        list.set_scroll_handler(move |_, _, cx| {
            let view = view.clone();
            cx.defer(move |cx| {
                view.update(cx, |_, cx| cx.notify()).ok();
            });
        });
        Self {
            list,
            model: TimelineModel::default(),
            expanded_turns: HashSet::new(),
            expanded_groups: HashSet::new(),
            expanded_entries: HashSet::new(),
            expanded_messages: HashSet::new(),
            expanded_plans: HashSet::new(),
            changed_files_expanded: HashMap::new(),
            trees: HashMap::new(),
            copied: None,
            last: None,
        }
    }

    pub fn rows(&self) -> &[Arc<TimelineRow>] {
        self.model.rows()
    }

    /// Whether a "Working for" row is showing (last, or before a trailing changed-files card).
    pub fn has_working_row(&self) -> bool {
        self.rows()
            .iter()
            .rev()
            .take(2)
            .any(|row| matches!(row.kind, RowKind::Working { .. }))
    }

    /// Derives rows for a new thread state and applies the change to the list.
    pub fn sync(
        &mut self,
        thread: &OrchestrationThread,
        scope: &str,
        optimistic: &[Arc<OrchestrationMessage>],
        is_working: bool,
        started_at: Option<&str>,
    ) {
        self.last = Some(LastInput {
            thread: thread.clone(),
            scope: scope.to_owned(),
            optimistic: optimistic.to_vec(),
            is_working,
            started_at: started_at.map(str::to_owned),
        });
        self.rederive();
    }

    /// Re-derives with the last thread after a local toggle (fold, group).
    pub fn rederive(&mut self) {
        let Some(last) = &self.last else { return };
        let diff = self.model.update(TimelineInput {
            thread: &last.thread,
            scope: &last.scope,
            optimistic: &last.optimistic,
            expanded_turns: &self.expanded_turns,
            expanded_work_groups: &self.expanded_groups,
            is_working: last.is_working,
            active_turn_started_at: last.started_at.as_deref(),
        });
        if let Some(diff) = diff {
            if diff.same_keys {
                self.list.remeasure_items(diff.new);
            } else {
                self.list.splice(diff.old, diff.new.len());
            }
        }
    }

    /// Remeasures the row with `id` after its own UI state changed height.
    pub fn remeasure_row(&self, id: &str) {
        if let Some(index) = self.rows().iter().position(|row| row.id == id) {
            self.list.remeasure_items(index..index + 1);
        }
    }

    /// Remeasures every row that shows the work entry `key` (its body opened or closed).
    pub fn remeasure_entry(&self, key: &str) {
        for (index, row) in self.rows().iter().enumerate() {
            let entries = match &row.kind {
                RowKind::Work { entries } | RowKind::ToolStack { entries, .. } => entries,
                _ => continue,
            };
            if entries.iter().any(|entry| entry.presentation_key() == key) {
                self.list.remeasure_items(index..index + 1);
            }
        }
    }

    /// The changed-files tree already built for `turn`.
    pub fn tree_for(&self, turn: &TurnId) -> Option<Entity<ChangedFilesTree>> {
        self.trees.get(turn).map(|(_, tree)| tree.clone())
    }

    /// Keeps the clicked row in place for a disclosure toggle: stop auto-following the end
    /// before the content below it changes.
    pub fn anchor_disclosure(&self) {
        self.list.pause_following_tail();
    }

    /// Follows the end again (send, the scroll-to-end pill).
    pub fn follow_end(&self) {
        self.list.set_follow_mode(FollowMode::Tail);
    }

    /// The scroll-to-end pill: shown once the user left the live edge.
    pub fn show_scroll_to_end(&self) -> bool {
        !self.list.is_following_tail() && self.list.is_scrolled_to_end() == Some(false)
    }

    /// The changed-files tree for a checkpoint, created on first use and updated when the
    /// checkpoint changes.
    pub fn tree(
        &mut self,
        summary: &Arc<OrchestrationCheckpointSummary>,
        cx: &mut App,
    ) -> Entity<ChangedFilesTree> {
        let files = || -> Vec<ChangedFile> {
            summary
                .files
                .iter()
                .map(|file| ChangedFile {
                    path: file.path.clone(),
                    stat: Some(DiffStat {
                        additions: file.additions,
                        deletions: file.deletions,
                    }),
                })
                .collect()
        };
        let all_expanded = self.changed_files_expanded.get(&summary.turn_id).copied();
        match self.trees.get_mut(&summary.turn_id) {
            Some((current, tree)) => {
                if !Arc::ptr_eq(current, summary) {
                    *current = summary.clone();
                    let files = files();
                    tree.update(cx, |tree, cx| tree.set_files(&files, cx));
                }
                tree.clone()
            }
            None => {
                let files = files();
                let tree = cx.new(|cx| {
                    let mut tree = ChangedFilesTree::new(&files, cx);
                    tree.set_all_expanded(all_expanded, cx);
                    tree
                });
                self.trees
                    .insert(summary.turn_id.clone(), (summary.clone(), tree.clone()));
                tree
            }
        }
    }
}
