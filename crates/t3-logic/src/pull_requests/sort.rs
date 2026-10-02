//! The Sort menu (`pullRequestList.logic.ts:1010-1128`, route `SORT_OPTIONS`).

use std::{cmp::Ordering, sync::Arc};

use serde::{Deserialize, Serialize};
use t3_protocol::{
    orchestration::PullRequestState,
    pull_requests::{ChecksState, Involvement, Mergeability, ReviewDecision},
};

use super::{EnvironmentEntry, Group, GroupKey};
use crate::time::parse_timestamp;

/// One Sort menu choice. `Ready` is the default and is left out of saved preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ListSort {
    #[default]
    Ready,
    Blocked,
    Updated,
    Newest,
    Oldest,
    Largest,
    Smallest,
}

impl ListSort {
    /// Menu order.
    pub const ALL: [Self; 7] = [
        Self::Ready,
        Self::Blocked,
        Self::Updated,
        Self::Newest,
        Self::Oldest,
        Self::Largest,
        Self::Smallest,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Merge readiness",
            Self::Blocked => "Blocked on me",
            Self::Updated => "Recently updated",
            Self::Newest => "Newest shown",
            Self::Oldest => "Oldest shown",
            Self::Largest => "Largest shown",
            Self::Smallest => "Smallest shown",
        }
    }

    /// Size and readiness orders need every row's line counts before they settle, so their
    /// stats are read eagerly; the date orders read only rows near the viewport.
    pub fn needs_all_stats(self) -> bool {
        matches!(self, Self::Ready | Self::Largest | Self::Smallest)
    }
}

/// Whether a row's diff size is known: the listing carried counts, or the stats read did.
pub type MeasuredFn<'a> = &'a dyn Fn(&EnvironmentEntry) -> bool;

/// One group's reordering.
type RankFn<'a> = &'a dyn Fn(&[Arc<EnvironmentEntry>]) -> Vec<Arc<EnvironmentEntry>>;

fn size(entry: &EnvironmentEntry) -> u64 {
    entry.additions + entry.deletions
}

fn passing(entry: &EnvironmentEntry) -> bool {
    entry.checks_state == Some(ChecksState::Passing)
}

fn approved(entry: &EnvironmentEntry) -> bool {
    entry.review_decision == Some(ReviewDecision::Approved)
}

/// Green and approved, then green, then other open work and drafts, then finished work; a known
/// conflict always last. Smaller measured diffs first within a tier, then recency.
pub fn rank_by_merge_readiness(
    entries: &[Arc<EnvironmentEntry>],
    measured: MeasuredFn,
) -> Vec<Arc<EnvironmentEntry>> {
    let tier = |entry: &EnvironmentEntry| {
        if entry.mergeability == Mergeability::Conflicting {
            4
        } else if entry.state != PullRequestState::Open {
            3
        } else if entry.is_draft {
            2
        } else if passing(entry) && approved(entry) {
            0
        } else if passing(entry) {
            1
        } else {
            2
        }
    };
    let mut ranked = entries.to_vec();
    ranked.sort_by(|left, right| {
        tier(left)
            .cmp(&tier(right))
            .then_with(|| measured(right).cmp(&measured(left)))
            .then_with(|| size(left).cmp(&size(right)))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    ranked
}

fn rank_by_tier_then_recency(
    entries: &[Arc<EnvironmentEntry>],
    tier: impl Fn(&EnvironmentEntry) -> u8,
) -> Vec<Arc<EnvironmentEntry>> {
    let mut ranked = entries.to_vec();
    ranked.sort_by(|left, right| {
        tier(left).cmp(&tier(right)).then_with(|| {
            match (
                parse_timestamp(&left.updated_at),
                parse_timestamp(&right.updated_at),
            ) {
                (Some(left), Some(right)) => right.cmp(&left),
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (None, None) => Ordering::Equal,
            }
        })
    });
    ranked
}

/// The author's queue: what is stuck on them first.
pub fn rank_blocked_on_author(entries: &[Arc<EnvironmentEntry>]) -> Vec<Arc<EnvironmentEntry>> {
    rank_by_tier_then_recency(entries, |entry| {
        if entry.state != PullRequestState::Open {
            6
        } else if entry.mergeability == Mergeability::Conflicting {
            0
        } else if entry.review_decision == Some(ReviewDecision::ChangesRequested) {
            1
        } else if entry.checks_state == Some(ChecksState::Failing) {
            2
        } else if entry.is_draft {
            3
        } else if passing(entry) && approved(entry) {
            5
        } else {
            4
        }
    })
}

/// The reviewer's queue: open work first.
pub fn rank_blocked_on_reviewer(entries: &[Arc<EnvironmentEntry>]) -> Vec<Arc<EnvironmentEntry>> {
    rank_by_tier_then_recency(entries, |entry| {
        u8::from(entry.state != PullRequestState::Open)
    })
}

/// Applies `sort` inside every group, keeping the groups' order (`sortPullRequestGroups`).
/// Readiness and blocked orders leave a search's relevance ranking alone.
pub fn sort_groups(
    groups: &[Group],
    sort: ListSort,
    search_text: &str,
    measured: MeasuredFn,
    involvement: &Involvement,
) -> Vec<Group> {
    let searching = !search_text.trim().is_empty();
    let within = |rank: RankFn| {
        groups
            .iter()
            .map(|group| Group {
                entries: rank(&group.entries),
                ..group.clone()
            })
            .collect()
    };
    match sort {
        ListSort::Ready if searching => groups.to_vec(),
        ListSort::Ready => within(&|entries| rank_by_merge_readiness(entries, measured)),
        ListSort::Blocked if searching => groups.to_vec(),
        ListSort::Blocked => groups
            .iter()
            .map(|group| {
                let role = match group.key {
                    GroupKey::Authored => Involvement::Authored,
                    GroupKey::ReviewRequested => Involvement::Reviewing,
                    GroupKey::Others => involvement.clone(),
                };
                let entries = match role {
                    Involvement::Authored => rank_blocked_on_author(&group.entries),
                    Involvement::Reviewing => rank_blocked_on_reviewer(&group.entries),
                    _ => return group.clone(),
                };
                Group {
                    entries,
                    ..group.clone()
                }
            })
            .collect(),
        ListSort::Updated => groups.to_vec(),
        ListSort::Newest | ListSort::Oldest => within(&|entries| {
            let mut ranked = entries.to_vec();
            ranked.sort_by(|left, right| {
                let left_created = parse_timestamp(&left.created_at);
                let right_created = parse_timestamp(&right.created_at);
                right_created
                    .is_some()
                    .cmp(&left_created.is_some())
                    .then_with(|| {
                        let dated = left_created.unwrap_or(0).cmp(&right_created.unwrap_or(0));
                        if sort == ListSort::Newest {
                            dated.reverse()
                        } else {
                            dated
                        }
                    })
                    .then_with(|| recency(right).cmp(&recency(left)))
            });
            ranked
        }),
        ListSort::Largest | ListSort::Smallest => within(&|entries| {
            let mut ranked = entries.to_vec();
            ranked.sort_by(|left, right| {
                measured(right)
                    .cmp(&measured(left))
                    .then_with(|| {
                        let sized = size(left).cmp(&size(right));
                        if sort == ListSort::Largest {
                            sized.reverse()
                        } else {
                            sized
                        }
                    })
                    .then_with(|| recency(right).cmp(&recency(left)))
            });
            ranked
        }),
    }
}

fn recency(entry: &EnvironmentEntry) -> i64 {
    parse_timestamp(&entry.updated_at)
        .or_else(|| parse_timestamp(&entry.created_at))
        .unwrap_or(0)
}
