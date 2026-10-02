//! Line counts read after the listing (`pullRequests.listStats`, `pullRequestList.logic.ts:
//! 438-589,636-673,1130-1144`). GitHub's listing leaves them out because they cost 40-60% of
//! the read; the rows draw without them and fill in when they arrive.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

use t3_protocol::{
    EnvironmentId, ProjectId,
    pull_requests::{PullRequestDiffStat, PullRequestRef},
};

use super::{EnvironmentEntry, entry_key};

/// The most refs one `listStats` read may carry.
pub const MAX_STATS_REFS: usize = 500;

/// Whether stats are read for every loaded row or only rows near the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsPolicy {
    Visible,
    Eager,
}

/// `(additions, deletions)` by [`diff_stat_key`].
pub type DiffStats = HashMap<String, (u64, u64)>;

/// A project id only names a project within its environment, so the key carries both.
pub fn diff_stat_key(
    environment_id: &EnvironmentId,
    project_id: &ProjectId,
    number: u64,
) -> String {
    format!("{} {} {number}", environment_id.0, project_id.0)
}

/// One `listStats` read: the refs to ask for and the row keys they answer.
#[derive(Clone, Debug, PartialEq)]
pub struct StatsBatch {
    pub environment_id: EnvironmentId,
    pub refs: Vec<PullRequestRef>,
    pub keys: BTreeSet<String>,
}

/// Rows that still need counts: no counts in the listing, not already asked for, not cached.
pub fn stats_keys_to_request(
    entries_by_key: &BTreeMap<String, Arc<EnvironmentEntry>>,
    entered: &BTreeSet<String>,
    batches: &[StatsBatch],
    stats: &DiffStats,
) -> BTreeSet<String> {
    entered
        .iter()
        .filter(|key| {
            entries_by_key.get(*key).is_some_and(|entry| {
                entry.additions == 0
                    && entry.deletions == 0
                    && !batches.iter().any(|batch| batch.keys.contains(*key))
                    && !stats.contains_key(&diff_stat_key(
                        &entry.environment_id,
                        &entry.project_id,
                        entry.number,
                    ))
            })
        })
        .cloned()
        .collect()
}

/// Groups rows into reads of at most [`MAX_STATS_REFS`] per environment.
pub fn stats_batches(
    entries_by_key: &BTreeMap<String, Arc<EnvironmentEntry>>,
    keys: &BTreeSet<String>,
) -> Vec<StatsBatch> {
    let mut by_environment: BTreeMap<EnvironmentId, Vec<(String, PullRequestRef)>> =
        BTreeMap::new();
    for key in keys {
        let Some(entry) = entries_by_key.get(key) else {
            continue;
        };
        by_environment
            .entry(entry.environment_id.clone())
            .or_default()
            .push((
                key.clone(),
                PullRequestRef::new(
                    entry.project_id.clone(),
                    entry.repository.clone(),
                    entry.number,
                ),
            ));
    }
    by_environment
        .into_iter()
        .flat_map(|(environment_id, rows)| {
            rows.chunks(MAX_STATS_REFS)
                .map(|chunk| StatsBatch {
                    environment_id: environment_id.clone(),
                    refs: chunk
                        .iter()
                        .map(|(_, reference)| reference.clone())
                        .collect(),
                    keys: chunk.iter().map(|(key, _)| key.clone()).collect(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The next reads: every loaded row when eager, else the rows that entered the viewport.
pub fn stats_request_batches(
    entries_by_key: &BTreeMap<String, Arc<EnvironmentEntry>>,
    candidates: &BTreeSet<String>,
    policy: StatsPolicy,
    active: &[StatsBatch],
    stats: &DiffStats,
) -> Vec<StatsBatch> {
    let requested: BTreeSet<String> = match policy {
        StatsPolicy::Eager => entries_by_key.keys().cloned().collect(),
        StatsPolicy::Visible => candidates.clone(),
    };
    let keys = stats_keys_to_request(entries_by_key, &requested, active, stats);
    stats_batches(entries_by_key, &keys)
}

/// Counts merged onto what is held, so a new read never blanks counts already showing.
pub fn merge_diff_stats(
    stats: &mut DiffStats,
    environment_id: &EnvironmentId,
    arrived: &[PullRequestDiffStat],
) {
    for stat in arrived {
        stats.insert(
            diff_stat_key(environment_id, &stat.project_id, stat.number),
            (stat.additions, stat.deletions),
        );
    }
}

/// The row with counts that arrived after it, only where the listing had none.
pub fn with_diff_stat(entry: &Arc<EnvironmentEntry>, stats: &DiffStats) -> Arc<EnvironmentEntry> {
    if entry.additions != 0 || entry.deletions != 0 {
        return entry.clone();
    }
    match stats.get(&diff_stat_key(
        &entry.environment_id,
        &entry.project_id,
        entry.number,
    )) {
        Some(&(additions, deletions)) => {
            let mut next = (**entry).clone();
            next.entry.additions = additions;
            next.entry.deletions = deletions;
            Arc::new(next)
        }
        None => entry.clone(),
    }
}

/// Rows keyed by [`entry_key`], for the stats reads.
pub fn entries_by_key<'a>(
    entries: impl IntoIterator<Item = &'a Arc<EnvironmentEntry>>,
) -> BTreeMap<String, Arc<EnvironmentEntry>> {
    entries
        .into_iter()
        .map(|entry| (entry_key(entry), entry.clone()))
        .collect()
}
