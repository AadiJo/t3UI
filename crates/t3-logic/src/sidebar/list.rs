//! The default sidebar's rendered list: rows and section markers in display order (fork
//! `Sidebar.tsx` `sidebarListItems`), shelf collapse, settled paging, keyboard traversal, and
//! the drag-and-drop plans (`Sidebar.logic.ts` `resolveSidebarDropTarget`,
//! `planSidebarThreadDrop`, `resolveSidebarDropVerb`).
//!
//! [`build_sidebar_list`] turns an [`InboxModel`] plus the view's shelf state into the list the
//! view renders. Jump shortcuts, `thread.previous/next`, shift-click ranges, and drag targets
//! all read this order, so collapsed shelves take part in none of them.

use std::collections::{HashMap, HashSet};

use super::{
    inbox::{InboxModel, InboxThread},
    order::plan_pinned_reorder,
};
use crate::refs::ThreadRef;

/// Settled rows shown when the shelf opens (`SETTLED_TAIL_INITIAL_COUNT`).
pub const SETTLED_INITIAL_COUNT: usize = 10;
/// Rows each "Show more" adds (`SETTLED_TAIL_PAGE_COUNT`).
pub const SETTLED_PAGE_COUNT: usize = 25;

/// The section a row renders in. Pinned, Active, and Working rows are cards; Snoozed and
/// Settled rows are slim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidebarSection {
    Pinned,
    Active,
    Working,
    Snoozed,
    Settled,
}

impl SidebarSection {
    /// Card rows (82px) versus slim rows (36px).
    pub fn is_card(self) -> bool {
        matches!(self, Self::Pinned | Self::Active | Self::Working)
    }
}

/// Structural list items. The drag labels and placeholders are zero height at rest; shelf
/// headers are 32px toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SidebarMarker {
    /// "Pinned" drag label above the pinned run.
    PinnedHeader,
    /// "Active" drag label between the pinned run and the inbox.
    PinnedDivider,
    /// Drop hint for an empty inbox.
    ActivePlaceholder,
    WorkingHeader,
    SnoozedHeader,
    SettledHeader,
    /// Drop hint for an empty settled tail.
    SettledPlaceholder,
}

/// One rendered list item.
#[derive(Clone, Debug)]
pub enum SidebarListItem {
    Thread {
        section: SidebarSection,
        row: InboxThread,
    },
    Marker(SidebarMarker),
}

impl SidebarListItem {
    pub fn thread_ref(&self) -> Option<&ThreadRef> {
        match self {
            Self::Thread { row, .. } => Some(&row.thread_ref),
            Self::Marker(_) => None,
        }
    }
}

/// The view's shelf state: which shelves are open and how deep the settled tail pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShelfState {
    pub working_expanded: bool,
    pub snoozed_expanded: bool,
    pub settled_expanded: bool,
    /// Settled rows to show while expanded; starts at [`SETTLED_INITIAL_COUNT`].
    pub settled_limit: usize,
}

impl Default for ShelfState {
    fn default() -> Self {
        Self {
            working_expanded: false,
            snoozed_expanded: false,
            settled_expanded: false,
            settled_limit: SETTLED_INITIAL_COUNT,
        }
    }
}

/// The rendered list.
#[derive(Clone, Debug, Default)]
pub struct SidebarList {
    pub items: Vec<SidebarListItem>,
    /// Whole-shelf counts for the collapsed headers ("Settled (12)").
    pub working_count: usize,
    pub snoozed_count: usize,
    pub settled_count: usize,
    /// Settled rows behind "Show N more" (only while the shelf is expanded).
    pub hidden_settled: usize,
}

impl SidebarList {
    /// Thread rows in display order.
    pub fn rows(&self) -> impl Iterator<Item = (SidebarSection, &InboxThread)> {
        self.items.iter().filter_map(|item| match item {
            SidebarListItem::Thread { section, row } => Some((*section, row)),
            SidebarListItem::Marker(_) => None,
        })
    }

    /// Thread refs in display order (shift-click ranges, traversal, jump shortcuts).
    pub fn thread_refs(&self) -> Vec<ThreadRef> {
        self.rows().map(|(_, row)| row.thread_ref.clone()).collect()
    }

    /// True when no thread is listed in any section, shelves included: the view shows the
    /// empty state and no markers.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The row and section for a thread, if it is rendered.
    pub fn row(&self, thread: &ThreadRef) -> Option<(SidebarSection, &InboxThread)> {
        self.rows().find(|(_, row)| &row.thread_ref == thread)
    }

    /// `thread.previous` / `thread.next` (`resolveAdjacentThreadId`): with no current thread,
    /// previous is the last row and next the first; a current thread that is not rendered, or
    /// one at the end in the direction of travel, goes nowhere.
    pub fn adjacent(&self, current: Option<&ThreadRef>, forward: bool) -> Option<ThreadRef> {
        let threads = self.thread_refs();
        let Some(current) = current else {
            return if forward {
                threads.first().cloned()
            } else {
                threads.last().cloned()
            };
        };
        let index = threads.iter().position(|thread| thread == current)?;
        if forward {
            threads.get(index + 1).cloned()
        } else {
            index
                .checked_sub(1)
                .and_then(|index| threads.get(index).cloned())
        }
    }

    /// `thread.jump.N`: the Nth rendered row (1-based).
    pub fn jump_target(&self, index: u8) -> Option<ThreadRef> {
        let index = usize::from(index).checked_sub(1)?;
        self.rows()
            .nth(index)
            .map(|(_, row)| row.thread_ref.clone())
    }

    /// The shelf header that takes `mt-auto` (pins the shelf block to the bottom): Working,
    /// else Snoozed, else Settled.
    pub fn first_shelf(&self) -> Option<SidebarMarker> {
        self.items.iter().find_map(|item| match item {
            SidebarListItem::Marker(
                marker @ (SidebarMarker::WorkingHeader
                | SidebarMarker::SnoozedHeader
                | SidebarMarker::SettledHeader),
            ) => Some(*marker),
            _ => None,
        })
    }
}

/// Inputs of [`build_sidebar_list`].
#[derive(Clone, Copy)]
pub struct SidebarListInputs<'a> {
    pub model: &'a InboxModel,
    pub shelves: ShelfState,
    pub route_thread: Option<&'a ThreadRef>,
    /// A settle-then-navigate is in flight for the route thread: keep it out of the collapsed
    /// settled shelf so it does not flash in before the navigation lands.
    pub settle_navigation_pending: bool,
}

/// Builds the rendered list. Collapsed shelves render no rows except the route thread's own
/// (the open thread never vanishes); the settled tail pages at `settled_limit`, appending the
/// route thread when it is deeper.
pub fn build_sidebar_list(inputs: SidebarListInputs) -> SidebarList {
    let model = inputs.model;
    let shelves = inputs.shelves;
    let route = inputs.route_thread;
    let mut list = SidebarList {
        working_count: model.working.len(),
        snoozed_count: model.snoozed.len(),
        settled_count: model.settled.len(),
        ..SidebarList::default()
    };
    let total = model.pinned.len()
        + model.active.len()
        + model.working.len()
        + model.snoozed.len()
        + model.settled.len();
    if total == 0 {
        return list;
    }

    let is_route = |row: &&InboxThread| route == Some(&row.thread_ref);
    let collapsed = |rows: &[InboxThread], expanded: bool| -> Vec<InboxThread> {
        if expanded {
            rows.to_vec()
        } else {
            rows.iter().filter(is_route).cloned().collect()
        }
    };

    // Settled paging: the first `limit` rows, plus the route thread when it is deeper.
    let mut visible_settled: Vec<InboxThread> = model
        .settled
        .iter()
        .take(shelves.settled_limit)
        .cloned()
        .collect();
    if model.settled.len() > shelves.settled_limit
        && let Some(route_row) = model.settled[shelves.settled_limit..].iter().find(is_route)
    {
        visible_settled.push(route_row.clone());
    }
    let rendered_settled = if shelves.settled_expanded {
        list.hidden_settled = model.settled.len() - visible_settled.len();
        visible_settled
    } else if inputs.settle_navigation_pending {
        Vec::new()
    } else {
        visible_settled.iter().filter(is_route).cloned().collect()
    };

    let push_rows = |items: &mut Vec<SidebarListItem>, rows: Vec<InboxThread>, section| {
        items.extend(
            rows.into_iter()
                .map(|row| SidebarListItem::Thread { section, row }),
        );
    };
    let items = &mut list.items;
    items.push(SidebarListItem::Marker(SidebarMarker::PinnedHeader));
    push_rows(items, model.pinned.clone(), SidebarSection::Pinned);
    items.push(SidebarListItem::Marker(SidebarMarker::PinnedDivider));
    items.push(SidebarListItem::Marker(SidebarMarker::ActivePlaceholder));
    push_rows(items, model.active.clone(), SidebarSection::Active);
    if !model.working.is_empty() {
        items.push(SidebarListItem::Marker(SidebarMarker::WorkingHeader));
        let rows = collapsed(&model.working, shelves.working_expanded);
        push_rows(items, rows, SidebarSection::Working);
    }
    if !model.snoozed.is_empty() {
        items.push(SidebarListItem::Marker(SidebarMarker::SnoozedHeader));
        let rows = collapsed(&model.snoozed, shelves.snoozed_expanded);
        push_rows(items, rows, SidebarSection::Snoozed);
    }
    items.push(SidebarListItem::Marker(SidebarMarker::SettledHeader));
    items.push(SidebarListItem::Marker(SidebarMarker::SettledPlaceholder));
    push_rows(items, rendered_settled, SidebarSection::Settled);
    list
}

// -------------------------------------------------------------------------------------------
// Drag and drop

/// What dropping a thread lifted from one section into another does, shown as a badge on the
/// lifted row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarDropVerb {
    Pin,
    Unpin,
    Settle,
    Unsettle,
    Wake,
}

impl SidebarDropVerb {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pin => "Pin",
            Self::Unpin => "Unpin",
            Self::Settle => "Settle",
            Self::Unsettle => "Un-settle",
            Self::Wake => "Wake",
        }
    }
}

/// `resolveSidebarDropVerb`: nothing inside one section, nothing for the Working and Snoozed
/// shelves (never drop targets).
pub fn resolve_sidebar_drop_verb(
    from: SidebarSection,
    to: Option<SidebarSection>,
) -> Option<SidebarDropVerb> {
    let to = to?;
    if to == from || matches!(to, SidebarSection::Working | SidebarSection::Snoozed) {
        return None;
    }
    Some(match (from, to) {
        (_, SidebarSection::Pinned) => SidebarDropVerb::Pin,
        (_, SidebarSection::Settled) => SidebarDropVerb::Settle,
        (SidebarSection::Pinned, _) => SidebarDropVerb::Unpin,
        (SidebarSection::Settled, _) => SidebarDropVerb::Unsettle,
        _ => SidebarDropVerb::Wake,
    })
}

/// The section a slot belongs to, read off the markers above it.
fn section_at_slot(items: &[SlotItem<'_>], index: usize) -> SidebarSection {
    let mut section = SidebarSection::Pinned;
    for item in items.iter().take(index) {
        match item {
            SlotItem::Marker(SidebarMarker::PinnedDivider) => section = SidebarSection::Active,
            SlotItem::Marker(SidebarMarker::WorkingHeader) => section = SidebarSection::Working,
            SlotItem::Marker(SidebarMarker::SnoozedHeader) => section = SidebarSection::Snoozed,
            SlotItem::Marker(SidebarMarker::SettledHeader) => section = SidebarSection::Settled,
            _ => {}
        }
    }
    section
}

#[derive(Clone, Copy)]
enum SlotItem<'a> {
    Thread(&'a ThreadRef),
    Marker(SidebarMarker),
}

/// Where a drop lands and the pinned / active orders it implies (`SidebarDropTarget`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarDropTarget {
    /// Pinned, Active, or Settled (the only destinations).
    pub section: SidebarSection,
    pub pinned_order: Vec<ThreadRef>,
    pub active_order: Vec<ThreadRef>,
}

/// `resolveSidebarDropTarget`: moves `active` to `over_index` (an index into `items`, as
/// dnd-kit's `arrayMove`) and reads the destination off the markers around it. `None` when
/// the slot is in the Working or Snoozed shelf or `active` is not a rendered row.
pub fn resolve_sidebar_drop_target(
    items: &[SidebarListItem],
    active: &ThreadRef,
    over_index: usize,
) -> Option<SidebarDropTarget> {
    let active_index = items
        .iter()
        .position(|item| item.thread_ref() == Some(active))?;
    if over_index >= items.len() {
        return None;
    }
    let mut moved: Vec<SlotItem<'_>> = items
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != active_index)
        .map(|(_, item)| match item {
            SidebarListItem::Thread { row, .. } => SlotItem::Thread(&row.thread_ref),
            SidebarListItem::Marker(marker) => SlotItem::Marker(*marker),
        })
        .collect();
    moved.insert(over_index, SlotItem::Thread(active));
    let section = section_at_slot(&moved, over_index);
    if matches!(section, SidebarSection::Working | SidebarSection::Snoozed) {
        return None;
    }
    let mut pinned_order = Vec::new();
    let mut active_order = Vec::new();
    let mut current = SidebarSection::Pinned;
    for item in moved {
        match item {
            SlotItem::Marker(SidebarMarker::PinnedDivider) => current = SidebarSection::Active,
            SlotItem::Marker(
                SidebarMarker::WorkingHeader
                | SidebarMarker::SnoozedHeader
                | SidebarMarker::SettledHeader,
            ) => break,
            SlotItem::Marker(_) => {}
            SlotItem::Thread(thread) if current == SidebarSection::Pinned => {
                pinned_order.push(thread.clone())
            }
            SlotItem::Thread(thread) => active_order.push(thread.clone()),
        }
    }
    Some(SidebarDropTarget {
        section,
        pinned_order,
        active_order,
    })
}

/// One `(thread, order key)` write.
pub type OrderAssignment = (ThreadRef, String);

/// What a drop does (`SidebarThreadDropPlan`). Commands run in sequence and stop at the first
/// failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarDropPlan {
    None,
    /// Reorder inside the pinned block (`thread.pin.reorder` writes).
    ReorderPinned {
        order: Vec<ThreadRef>,
        assignments: Vec<OrderAssignment>,
    },
    /// Pin from another section: `thread.pin` with `order_key`, then `extra_assignments`.
    Pin {
        order: Vec<ThreadRef>,
        order_key: Option<String>,
        extra_assignments: Vec<OrderAssignment>,
    },
    /// Into the inbox: unpin / un-settle / wake as needed, then `thread.active.reorder`
    /// writes. `order` is `None` when the inbox is time-ordered (Working beta).
    MoveActive {
        order: Option<Vec<ThreadRef>>,
        assignments: Vec<OrderAssignment>,
        unpin: bool,
        unsettle: bool,
        unsnooze: bool,
    },
    Settle,
}

/// Inputs of [`plan_sidebar_thread_drop`].
pub struct SidebarDropInputs<'a> {
    pub active: &'a ThreadRef,
    pub active_section: SidebarSection,
    /// Snoozed threads can stay pinned or settled beneath the shelf.
    pub active_pinned: bool,
    pub active_settled: bool,
    pub supports_settlement: bool,
    pub target: &'a SidebarDropTarget,
    /// Pinned rows in display order before the drop.
    pub pinned_order: &'a [ThreadRef],
    /// `pinOrderKey` of every pinned thread, hidden snoozed pins included.
    pub pinned_keys: &'a HashMap<ThreadRef, Option<String>>,
    /// Threads whose server supports pin reordering.
    pub pin_reorderable: &'a HashSet<ThreadRef>,
    pub active_order: &'a [ThreadRef],
    pub active_keys: &'a HashMap<ThreadRef, Option<String>>,
    pub active_reorderable: &'a HashSet<ThreadRef>,
    /// Working beta: the inbox sorts by time, so drops only change lifecycle.
    pub active_time_ordered: bool,
}

/// Plans the order-key writes for `order` after `moved` moved, keyed by scoped thread key.
fn plan_reorder(
    order: &[ThreadRef],
    keys: &HashMap<ThreadRef, Option<String>>,
    moved: &ThreadRef,
) -> Vec<OrderAssignment> {
    let by_key: HashMap<String, &ThreadRef> = keys
        .keys()
        .chain(order.iter())
        .map(|thread| (thread.key(), thread))
        .collect();
    let ids: Vec<String> = order.iter().map(ThreadRef::key).collect();
    let keys_by_id: HashMap<String, Option<String>> = keys
        .iter()
        .map(|(thread, key)| (thread.key(), key.clone()))
        .collect();
    plan_pinned_reorder(&ids, &keys_by_id, &moved.key())
        .into_iter()
        .filter_map(|(id, key)| by_key.get(&id).map(|thread| ((*thread).clone(), key)))
        .collect()
}

/// `planSidebarThreadDrop`.
pub fn plan_sidebar_thread_drop(inputs: SidebarDropInputs) -> SidebarDropPlan {
    let target = inputs.target;
    if !inputs.supports_settlement
        && (target.section == SidebarSection::Settled || inputs.active_settled)
    {
        return SidebarDropPlan::None;
    }
    match target.section {
        SidebarSection::Active => {
            if inputs.active_time_ordered {
                return if inputs.active_section == SidebarSection::Active {
                    SidebarDropPlan::None
                } else {
                    SidebarDropPlan::MoveActive {
                        order: None,
                        assignments: Vec::new(),
                        unpin: inputs.active_pinned,
                        unsettle: inputs.active_settled,
                        unsnooze: inputs.active_section == SidebarSection::Snoozed,
                    }
                };
            }
            let order = &target.active_order;
            if inputs.active_section == SidebarSection::Active && order == inputs.active_order {
                return SidebarDropPlan::None;
            }
            let assignments = plan_reorder(order, inputs.active_keys, inputs.active);
            if assignments
                .iter()
                .any(|(thread, _)| !inputs.active_reorderable.contains(thread))
            {
                return SidebarDropPlan::None;
            }
            SidebarDropPlan::MoveActive {
                order: Some(order.clone()),
                assignments,
                unpin: inputs.active_pinned,
                unsettle: inputs.active_settled,
                unsnooze: inputs.active_section == SidebarSection::Snoozed,
            }
        }
        SidebarSection::Settled => {
            if inputs.active_section == SidebarSection::Settled {
                SidebarDropPlan::None
            } else {
                SidebarDropPlan::Settle
            }
        }
        SidebarSection::Pinned => {
            let order = &target.pinned_order;
            if inputs.active_section == SidebarSection::Pinned && order == inputs.pinned_order {
                return SidebarDropPlan::None;
            }
            let assignments = plan_reorder(order, inputs.pinned_keys, inputs.active);
            if assignments
                .iter()
                .any(|(thread, _)| !inputs.pin_reorderable.contains(thread))
            {
                return SidebarDropPlan::None;
            }
            if inputs.active_section == SidebarSection::Pinned {
                return if assignments.is_empty() {
                    SidebarDropPlan::None
                } else {
                    SidebarDropPlan::ReorderPinned {
                        order: order.clone(),
                        assignments,
                    }
                };
            }
            let order_key = assignments
                .iter()
                .find(|(thread, _)| thread == inputs.active)
                .map(|(_, key)| key.clone());
            let extra_assignments = if inputs.active_pinned {
                assignments
            } else {
                assignments
                    .into_iter()
                    .filter(|(thread, _)| thread != inputs.active)
                    .collect()
            };
            SidebarDropPlan::Pin {
                order: order.clone(),
                order_key,
                extra_assignments,
            }
        }
        SidebarSection::Working | SidebarSection::Snoozed => SidebarDropPlan::None,
    }
}

// -------------------------------------------------------------------------------------------
// Forward navigation

/// Where to go after parking the route thread (`planForwardNavigation`): the next rendered row
/// after it (wrapping) that is not settled, not snoozed, and not parking in the same batch.
/// `None` means "a new draft in the thread's project, else `/`".
pub fn plan_forward_navigation(
    list: &SidebarList,
    route_thread: &ThreadRef,
    parking: &HashSet<ThreadRef>,
) -> Option<ThreadRef> {
    let rows: Vec<(SidebarSection, &InboxThread)> = list.rows().collect();
    let start = rows
        .iter()
        .position(|(_, row)| &row.thread_ref == route_thread)?;
    (1..rows.len())
        .map(|offset| rows[(start + offset) % rows.len()])
        .find(|(section, row)| {
            !matches!(section, SidebarSection::Settled | SidebarSection::Snoozed)
                && !parking.contains(&row.thread_ref)
                && &row.thread_ref != route_thread
        })
        .map(|(_, row)| row.thread_ref.clone())
}
