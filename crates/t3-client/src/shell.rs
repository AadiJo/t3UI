//! Shell state: every project and active thread of an environment (sidebar data). A pure
//! reducer over `orchestration.subscribeShell` items, mirroring upstream
//! `packages/client-runtime/src/state/shellReducer.ts` and `shell.ts`.

use std::sync::Arc;

use t3_protocol::{
    ProjectId, ThreadId,
    orchestration::{
        OrchestrationProjectShell, OrchestrationShellSnapshot, OrchestrationThreadShell,
        ShellStreamItem,
    },
};

/// How fresh shell or thread data is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// No data yet.
    #[default]
    Empty,
    /// Data from an earlier session; not subscribed right now.
    Cached,
    /// Subscribed, waiting for the server's `synchronized` marker.
    Synchronizing,
    /// Caught up and receiving live updates.
    Live,
}

/// Projects and active threads. Rows are `Arc`s shared with previous states, so a view can
/// skip rows where `Arc::ptr_eq` holds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShellState {
    /// Resume cursor: the sequence of the newest applied snapshot or event.
    pub snapshot_sequence: u64,
    pub projects: Vec<Arc<OrchestrationProjectShell>>,
    pub threads: Vec<Arc<OrchestrationThreadShell>>,
    pub status: SyncStatus,
}

impl ShellState {
    pub fn has_data(&self) -> bool {
        self.status != SyncStatus::Empty
    }

    /// The `afterSequence` to resubscribe with, if there is data to resume from.
    pub fn resume_cursor(&self) -> Option<u64> {
        self.has_data().then_some(self.snapshot_sequence)
    }

    pub fn project(&self, id: &ProjectId) -> Option<&Arc<OrchestrationProjectShell>> {
        self.projects.iter().find(|p| &p.id == id)
    }

    pub fn thread(&self, id: &ThreadId) -> Option<&Arc<OrchestrationThreadShell>> {
        self.threads.iter().find(|t| &t.id == id)
    }

    /// Threads of one project, in server order.
    pub fn project_threads<'a>(
        &'a self,
        project: &'a ProjectId,
    ) -> impl Iterator<Item = &'a Arc<OrchestrationThreadShell>> + 'a {
        self.threads.iter().filter(move |t| &t.project_id == project)
    }

    /// Replaces everything with a snapshot (HTTP fast path or socket snapshot item). Keeps the
    /// sync status unless there was no data.
    pub fn apply_snapshot(&mut self, snapshot: OrchestrationShellSnapshot) {
        self.snapshot_sequence = snapshot.snapshot_sequence;
        self.projects = snapshot.projects;
        self.threads = snapshot.threads;
        if self.status == SyncStatus::Empty {
            self.status = SyncStatus::Cached;
        }
    }

    /// Marks a new subscription. Without completion-marker support the data counts as live
    /// right away (upstream does the same).
    pub fn begin_sync(&mut self, completion_marker: bool) {
        self.status = if completion_marker {
            SyncStatus::Synchronizing
        } else {
            SyncStatus::Live
        };
    }

    /// Marks the subscription as gone; data stays as a cache.
    pub fn end_sync(&mut self) {
        if self.status != SyncStatus::Empty {
            self.status = SyncStatus::Cached;
        }
    }

    /// Applies one stream item. Returns whether the state changed.
    ///
    /// Events at or below the cursor are dropped (the server may overlap replay and live
    /// tail). Gaps are normal: other aggregates and coalescing skip sequences.
    pub fn apply(&mut self, item: ShellStreamItem) -> bool {
        match item {
            ShellStreamItem::Snapshot(snapshot) => {
                self.apply_snapshot(snapshot);
                true
            }
            ShellStreamItem::Synchronized => {
                let changed = self.status != SyncStatus::Live;
                self.status = SyncStatus::Live;
                changed
            }
            ShellStreamItem::ProjectUpserted { sequence, project } => {
                self.advance(sequence, |state| upsert(&mut state.projects, project, |p| &p.id))
            }
            ShellStreamItem::ProjectRemoved {
                sequence,
                project_id,
            } => self.advance(sequence, |state| {
                state.projects.retain(|p| p.id != project_id)
            }),
            ShellStreamItem::ThreadUpserted { sequence, thread } => {
                self.advance(sequence, |state| upsert(&mut state.threads, thread, |t| &t.id))
            }
            ShellStreamItem::ThreadRemoved {
                sequence,
                thread_id,
            } => self.advance(sequence, |state| {
                state.threads.retain(|t| t.id != thread_id)
            }),
            ShellStreamItem::Unknown { .. } => false,
        }
    }

    fn advance(&mut self, sequence: u64, change: impl FnOnce(&mut Self)) -> bool {
        if self.has_data() && sequence <= self.snapshot_sequence {
            return false;
        }
        change(self);
        self.snapshot_sequence = sequence;
        if self.status == SyncStatus::Empty {
            self.status = SyncStatus::Cached;
        }
        true
    }
}

/// Replaces the row with the same id in place, or appends it.
fn upsert<T, K: PartialEq>(rows: &mut Vec<Arc<T>>, row: Arc<T>, key: impl Fn(&T) -> &K) {
    match rows.iter_mut().find(|existing| key(existing) == key(&row)) {
        Some(existing) => *existing = row,
        None => rows.push(row),
    }
}
