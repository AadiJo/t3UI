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

use gpui_kit::{
    App, AppContext as _, Context, Entity, FollowMode, ListAlignment, ListState, Pixels, px,
};
use t3_client::ThreadState;
use t3_diff::{
    ChangedFilesTree,
    tree::{ChangedFile, DiffStat},
};
use t3_logic::timeline::{RowKind, TimelineInput, TimelineModel, TimelineRow};
use t3_protocol::{
    TurnId,
    orchestration::{OrchestrationCheckpointSummary, OrchestrationMessage},
};

use super::{ChatView, markdown::MarkdownCache};

/// Rows rendered beyond the viewport so short scrolls never show blank space.
const OVERDRAW: f32 = 600.;
/// Frames an anchored disclosure may spend settling (`DISCLOSURE_ANCHOR_MAX_FRAMES`).
const DISCLOSURE_ANCHOR_MAX_FRAMES: u32 = 60;

/// A disclosure toggle in progress: the row whose bottom edge stays put.
struct DisclosureAnchor {
    row_id: String,
    bottom: Pixels,
    frames: u32,
    stable: u32,
    adjusted: bool,
}

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
    /// Markdown views of messages and plans.
    pub markdown: MarkdownCache,
    /// The copy button showing its check, by key, until the 1s reset.
    pub copied: Option<String>,
    /// The user scrolled (wheel or scrollbar) since the list last followed the end. Programmatic
    /// moves and content growth never show the scroll-to-end pill (`free-scrolling` mode).
    pub manual_navigation: bool,
    /// The disclosure toggle being settled.
    anchor: Option<DisclosureAnchor>,
    /// Inputs of the last derivation, kept so local toggles can re-derive.
    last: Option<LastInput>,
}

struct LastInput {
    state: Arc<ThreadState>,
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
        // The list reports wheel scrolls only: those are manual navigation. Re-render so the
        // scroll-to-end pill follows the position.
        list.set_scroll_handler(move |_, _, cx| {
            let view = view.clone();
            cx.defer(move |cx| {
                view.update(cx, |this, cx| {
                    this.timeline.manual_navigation = true;
                    cx.notify();
                })
                .ok();
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
            markdown: MarkdownCache::default(),
            copied: None,
            manual_navigation: false,
            anchor: None,
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
    /// `state` must hold a thread (callers skip states without one).
    pub fn sync(
        &mut self,
        state: Arc<ThreadState>,
        scope: &str,
        optimistic: &[Arc<OrchestrationMessage>],
        is_working: bool,
        started_at: Option<&str>,
    ) {
        self.last = Some(LastInput {
            state,
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
        let Some(thread) = last.state.thread.as_ref() else {
            return;
        };
        let previous = self.model.rows().to_vec();
        let diff = self.model.update(TimelineInput {
            thread,
            scope: &last.scope,
            optimistic: &last.optimistic,
            expanded_turns: &self.expanded_turns,
            expanded_work_groups: &self.expanded_groups,
            is_working: last.is_working,
            active_turn_started_at: last.started_at.as_deref(),
        });
        let Some(diff) = diff else { return };
        // Rows that kept their key only change height: remeasuring keeps the scroll anchor
        // inside them, where a splice would reset it to the row's top.
        let rows = self.model.rows();
        let kept = previous[diff.old.clone()]
            .iter()
            .zip(&rows[diff.new.clone()])
            .take_while(|(old, new)| old.id == new.id)
            .count();
        if kept > 0 {
            self.list
                .remeasure_items(diff.new.start..diff.new.start + kept);
        }
        if !diff.same_keys {
            self.list
                .splice(diff.old.start + kept..diff.old.end, diff.new.len() - kept);
        }
        // Forget views and trees of rows that are gone (a revert, a new thread state).
        let mut alive: HashSet<String> = HashSet::new();
        let mut turns: HashSet<&TurnId> = HashSet::new();
        for row in self.model.rows() {
            match &row.kind {
                RowKind::Message(message) => {
                    alive.insert(message.message.id.to_string());
                }
                RowKind::ProposedPlan(plan) => {
                    alive.insert(format!("plan:{}", plan.id));
                    alive.insert(format!("plan-preview:{}", plan.id));
                }
                RowKind::ChangedFiles(summary) => {
                    turns.insert(&summary.turn_id);
                }
                _ => {}
            }
        }
        self.markdown.retain(|key| alive.contains(key));
        self.trees.retain(|turn, _| turns.contains(turn));
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

    /// Starts an anchored disclosure toggle (`applyAnchoredDisclosureToggle`): stop following
    /// the end and remember where the bottom of row `row_id` is, so [`Self::settle_anchor`]
    /// can hold it there while rows above or below it open or close.
    pub fn anchor_disclosure(&mut self, row_id: &str) {
        self.list.pause_following_tail();
        self.anchor = self.row_bottom(row_id).map(|bottom| DisclosureAnchor {
            row_id: row_id.to_owned(),
            bottom,
            frames: 0,
            stable: 0,
            adjusted: false,
        });
    }

    /// One frame of the anchored disclosure: scrolls by however far the anchor row's bottom
    /// moved. Returns whether to run again next frame. Like the web, it stops after two stable
    /// frames once it adjusted, when the anchor row is gone, or after 60 frames.
    pub fn settle_anchor(&mut self) -> bool {
        let Some(anchor) = self.anchor.as_mut() else {
            return false;
        };
        anchor.frames += 1;
        let row_id = anchor.row_id.clone();
        let Some(bottom) = self.row_bottom(&row_id) else {
            self.anchor = None;
            return false;
        };
        let anchor = self.anchor.as_mut().expect("anchor checked above");
        let delta = bottom - anchor.bottom;
        if delta.abs() >= px(0.5) {
            self.list.scroll_by(delta);
            anchor.adjusted = true;
            anchor.stable = 0;
        } else {
            anchor.stable += 1;
        }
        // The first frame after a toggle runs before the new rows are laid out, so an
        // unadjusted anchor needs a few stable frames before it counts as settled.
        let settled = if anchor.adjusted {
            anchor.stable >= 2
        } else {
            anchor.stable >= 3
        };
        if settled || anchor.frames >= DISCLOSURE_ANCHOR_MAX_FRAMES {
            self.anchor = None;
            return false;
        }
        true
    }

    /// The window-space bottom of row `row_id`, if it is laid out on screen.
    fn row_bottom(&self, row_id: &str) -> Option<Pixels> {
        let index = self.rows().iter().position(|row| row.id == row_id)?;
        Some(self.list.bounds_for_item(index)?.bottom())
    }

    /// Follows the end again (send, the scroll-to-end pill).
    pub fn follow_end(&mut self) {
        self.manual_navigation = false;
        self.list.set_follow_mode(FollowMode::Tail);
    }

    /// The scroll-to-end pill: shown after manual navigation left the live edge
    /// (`onIsAtEndChange`). Reaching the end again resumes following and resets the mode.
    pub fn show_scroll_to_end(&mut self) -> bool {
        if self.list.is_scrollbar_dragging() {
            self.manual_navigation = true;
        }
        if self.list.is_following_tail() {
            self.manual_navigation = false;
        }
        self.manual_navigation && self.list.is_scrolled_to_end() == Some(false)
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
