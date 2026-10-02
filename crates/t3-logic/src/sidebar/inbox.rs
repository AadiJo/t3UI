//! The default (non-legacy) sidebar: one inbox of threads from every environment, split into
//! Pinned, the active inbox, the beta Working shelf, Snoozed, and Settled (fork `Sidebar.tsx`
//! section memo). Projects are only a scope filter here; the project-grouped layout is
//! [`build_sidebar`](super::build_sidebar), used when `legacySidebarEnabled` is on.
//!
//! [`build_inbox`] is pure. Snooze classification needs a precise clock: rebuild at the next
//! wake time (the earliest `snoozed_until` in the Snoozed shelf) so woken rows leave promptly.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use t3_protocol::{
    EnvironmentId,
    environment::ExecutionEnvironmentCapabilities,
    orchestration::{OrchestrationThreadShell, SettledOverride},
};

use super::{
    order::{
        ThreadRow, sort_active_threads, sort_inbox_threads_by_return, sort_pinned_threads,
        sort_settled_threads,
    },
    snooze::{effective_snoozed, snooze_wake_label, thread_woke_at},
    status::{
        RecedeInput, SidebarThreadStatus, ThreadStatus, has_unseen_completion,
        is_sidebar_thread_working, resolve_sidebar_thread_status, resolve_thread_status,
        should_recede_sidebar_thread,
    },
};
use crate::{
    refs::{ProjectRef, ThreadRef},
    time::{EpochMillis, parse_timestamp},
    ui_state::UiState,
};

/// One environment's threads and the capabilities that gate lifecycle sections.
#[derive(Clone, Copy)]
pub struct InboxEnvironment<'a> {
    pub id: &'a EnvironmentId,
    pub threads: &'a [Arc<OrchestrationThreadShell>],
    /// `None` until the server config arrives; threads then never classify as snoozed or
    /// settled and cannot be dragged.
    pub capabilities: Option<&'a ExecutionEnvironmentCapabilities>,
}

/// Everything [`build_inbox`] reads.
#[derive(Clone, Copy)]
pub struct InboxInputs<'a> {
    pub environments: &'a [InboxEnvironment<'a>],
    /// The project scope menu's selection (members of one logical project); `None` is all.
    pub scope: Option<&'a HashSet<ProjectRef>>,
    /// Client setting `sidebarWorkingShelfEnabled` (beta Working shelf).
    pub working_shelf_enabled: bool,
    pub now: EpochMillis,
    pub ui: &'a UiState,
    pub route_thread: Option<&'a ThreadRef>,
    /// Returns observed by this client; call [`InboxReturns::observe`] before building.
    pub returns: &'a InboxReturns,
}

/// A thread row in any section.
#[derive(Clone, Debug)]
pub struct InboxThread {
    pub thread_ref: ThreadRef,
    pub thread: Arc<OrchestrationThreadShell>,
    pub status: Option<ThreadStatus>,
    pub row_status: SidebarThreadStatus,
    /// The route shows this thread.
    pub active: bool,
    /// A completion the user has not seen.
    pub unread: bool,
    /// Woke from a snooze since the last visit (and not settled): the row shows "Woke".
    pub woke: bool,
    /// "2h" for rows in the Snoozed shelf.
    pub snooze_wake_label: Option<String>,
    /// Render dimmed (multi-selection can override; see [`should_recede_sidebar_thread`]).
    pub recedes: bool,
}

impl ThreadRow for InboxThread {
    fn shell(&self) -> &OrchestrationThreadShell {
        &self.thread
    }

    fn environment(&self) -> &str {
        self.thread_ref.environment_id.as_str()
    }
}

/// The sectioned sidebar, each section already sorted.
#[derive(Clone, Debug, Default)]
pub struct InboxModel {
    /// User-arranged keys first, then keyless newest-created.
    pub pinned: Vec<InboxThread>,
    /// The inbox. With the Working shelf: newest return to the user first; otherwise new and
    /// reopened threads lead, arranged threads follow their keys.
    pub active: Vec<InboxThread>,
    /// Beta Working shelf (empty when disabled): newest return first.
    pub working: Vec<InboxThread>,
    /// Soonest wake first.
    pub snoozed: Vec<InboxThread>,
    /// Newest end of work first.
    pub settled: Vec<InboxThread>,
    /// Threads whose server supports pin reordering (drag sources).
    pub draggable: HashSet<ThreadRef>,
    /// Threads whose server supports active reordering (drop targets in the inbox).
    pub active_reorderable: HashSet<ThreadRef>,
    /// The earliest future wake time among snoozed rows: rebuild then.
    pub next_wake_at: Option<EpochMillis>,
}

enum Section {
    Pinned,
    Inbox,
    Snoozed,
    Settled,
}

/// Builds the sectioned sidebar. Precedence: snoozed (needs `threadSnooze`) > settled (needs
/// `threadSettlement`) > pinned > the inbox, where the Working shelf, when enabled, takes
/// threads busy with work that does not need the user. Archived threads and threads outside
/// the scope are dropped.
pub fn build_inbox(inputs: &InboxInputs) -> InboxModel {
    let mut model = InboxModel::default();
    let (mut pinned, mut active, mut working, mut snoozed, mut settled) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());

    for environment in inputs.environments {
        let capabilities = environment.capabilities;
        let supports =
            |flag: fn(&ExecutionEnvironmentCapabilities) -> bool| capabilities.is_some_and(flag);
        for thread in environment.threads {
            if thread.archived_at.is_some() {
                continue;
            }
            if let Some(scope) = inputs.scope
                && !scope.contains(&ProjectRef::new(
                    environment.id.clone(),
                    thread.project_id.clone(),
                ))
            {
                continue;
            }
            let thread_ref = ThreadRef::new(environment.id.clone(), thread.id.clone());
            if supports(|c| c.thread_active_reorder) {
                model.active_reorderable.insert(thread_ref.clone());
            }
            if supports(|c| c.thread_pinning && c.thread_pin_reorder) {
                model.draggable.insert(thread_ref.clone());
            }

            let section = if supports(|c| c.thread_snooze) && effective_snoozed(thread, inputs.now)
            {
                Section::Snoozed
            } else if supports(|c| c.thread_settlement) && is_settled(thread) {
                Section::Settled
            } else if thread.pinned_at.is_some() {
                Section::Pinned
            } else {
                Section::Inbox
            };
            let row = row(
                inputs,
                thread_ref,
                thread,
                matches!(section, Section::Snoozed),
            );
            match section {
                Section::Snoozed => snoozed.push(row),
                Section::Settled => settled.push(row),
                Section::Pinned => pinned.push(row),
                Section::Inbox
                    if inputs.working_shelf_enabled && is_sidebar_thread_working(thread) =>
                {
                    working.push(row)
                }
                Section::Inbox => active.push(row),
            }
        }
    }

    model.pinned = sort_pinned_threads(pinned);
    model.active = if inputs.working_shelf_enabled {
        sort_inbox_threads_by_return(active, |row| inputs.returns.returned_at(&row.thread_ref))
    } else {
        sort_active_threads(active)
    };
    model.working = sort_inbox_threads_by_return(working, |_| None);
    // Stable: equal or malformed wake times keep classification order.
    snoozed.sort_by_key(|row| {
        row.thread
            .snoozed_until
            .as_deref()
            .and_then(parse_timestamp)
            .unwrap_or(0)
    });
    model.next_wake_at = snoozed
        .iter()
        .filter_map(|row| {
            row.thread
                .snoozed_until
                .as_deref()
                .and_then(parse_timestamp)
        })
        .filter(|&at| at > inputs.now)
        .min();
    model.snoozed = snoozed;
    model.settled = sort_settled_threads(settled);
    model
}

fn is_settled(thread: &OrchestrationThreadShell) -> bool {
    thread.settled_override == Some(SettledOverride::Settled)
}

fn row(
    inputs: &InboxInputs,
    thread_ref: ThreadRef,
    thread: &Arc<OrchestrationThreadShell>,
    in_snoozed_shelf: bool,
) -> InboxThread {
    let last_visited = inputs.ui.last_visited_at(&thread_ref.key());
    let unread = has_unseen_completion(thread, last_visited);
    let row_status = resolve_sidebar_thread_status(thread);
    let woke_at = thread_woke_at(thread, inputs.now).and_then(|at| parse_timestamp(&at));
    // An unparseable visit counts as never visited, so corrupt local data cannot eat the wake.
    let visited = last_visited.and_then(parse_timestamp);
    let woke = woke_at.is_some_and(|woke_at| visited.is_none_or(|visited| visited < woke_at))
        && !is_settled(thread);
    let active = inputs.route_thread == Some(&thread_ref);
    InboxThread {
        status: resolve_thread_status(thread, last_visited),
        row_status,
        active,
        unread,
        woke,
        snooze_wake_label: in_snoozed_shelf
            .then(|| thread.snoozed_until.as_deref())
            .flatten()
            .map(|until| snooze_wake_label(until, inputs.now)),
        recedes: should_recede_sidebar_thread(RecedeInput {
            status: row_status,
            is_unread: unread,
            is_woke: woke,
            is_active: active,
            is_selected: false,
        }),
        thread_ref,
        thread: thread.clone(),
    }
}

/// When this client saw each thread leave the Working shelf. Upstream keeps it in module
/// scope so the inbox order survives remounts; keep one per window and call
/// [`observe`](Self::observe) with every thread before each [`build_inbox`].
#[derive(Clone, Debug, Default)]
pub struct InboxReturns {
    last_working: Option<HashSet<ThreadRef>>,
    observed: HashMap<ThreadRef, EpochMillis>,
}

impl InboxReturns {
    /// Stamps threads that stopped working since the last call. The first call only takes a
    /// baseline, so opening the app never reshuffles the inbox. Deleted threads are forgotten.
    pub fn observe<'a>(
        &mut self,
        threads: impl IntoIterator<Item = (&'a EnvironmentId, &'a Arc<OrchestrationThreadShell>)>,
        now: EpochMillis,
    ) {
        let mut present = HashSet::new();
        let mut working = HashSet::new();
        for (environment, thread) in threads {
            let thread_ref = ThreadRef::new(environment.clone(), thread.id.clone());
            if is_sidebar_thread_working(thread) {
                working.insert(thread_ref.clone());
            }
            present.insert(thread_ref);
        }
        self.observed.retain(|thread, _| present.contains(thread));
        for thread in self.last_working.iter().flatten() {
            if present.contains(thread) && !working.contains(thread) {
                self.observed.insert(thread.clone(), now);
            }
        }
        self.last_working = Some(working);
    }

    /// Forgets everything (the Working shelf was turned off).
    pub fn reset(&mut self) {
        self.last_working = None;
        self.observed.clear();
    }

    pub fn returned_at(&self, thread: &ThreadRef) -> Option<EpochMillis> {
        self.observed.get(thread).copied()
    }
}
