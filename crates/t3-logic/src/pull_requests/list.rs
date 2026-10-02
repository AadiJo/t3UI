//! Rows from every environment read as one list: merging answers, involvement, grouping,
//! pending action overrides and filter facets (`pullRequestList.logic.ts:26-437,591-749,
//! 1130-1268`).

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use t3_protocol::{
    EnvironmentId,
    orchestration::{PullRequestActor, PullRequestState},
    pull_requests::{
        Involvement, ListState, PullRequestAction, PullRequestLabel, PullRequestListFilters,
        PullRequestListProjectError, PullRequestListResult, PullRequestProviderSummary,
    },
};

use super::EnvironmentEntry;

/// Signed-in login per host. Keyed `"<environmentId> <host>"` once merged, so two machines
/// signed in to one host as different people stay two people; a single environment's answer
/// is keyed by host alone.
pub type Viewers = BTreeMap<String, String>;

/// Rows of one involvement group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Authored,
    ReviewRequested,
    Others,
}

impl GroupKey {
    /// Header label (`GROUP_LABELS`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Authored => "Authored",
            Self::ReviewRequested => "Review requested",
            Self::Others => "Others",
        }
    }
}

/// A run of rows under one header. `labeled` is false for the single headerless group an
/// involvement other than "all" shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub key: GroupKey,
    pub labeled: bool,
    pub entries: Vec<Arc<EnvironmentEntry>>,
}

fn normalize(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim().to_lowercase();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Unique across hosts and environments: `"<env>:<host>:<owner/name>#<n>"`.
pub fn entry_key(entry: &EnvironmentEntry) -> String {
    format!(
        "{}:{}:{}#{}",
        entry.environment_id.0, entry.host, entry.repository, entry.number
    )
}

/// The signed-in login for the row's host, preferring the row's own environment.
pub fn entry_viewer(entry: &EnvironmentEntry, viewers: &Viewers) -> Option<String> {
    let scoped = format!("{} {}", entry.environment_id.0, entry.host);
    normalize(
        viewers
            .get(&scoped)
            .or_else(|| viewers.get(&entry.host))
            .map(String::as_str),
    )
}

fn is_authored_by_viewer(entry: &EnvironmentEntry, viewers: &Viewers) -> bool {
    entry_viewer(entry, viewers).is_some_and(|viewer| {
        normalize(entry.author.as_ref().map(|author| author.login.as_str())) == Some(viewer)
    })
}

/// The involvement tab over rows already read: reviewing = a review is requested of the viewer,
/// authored = the viewer wrote it.
pub fn filter_by_involvement(
    entries: &[Arc<EnvironmentEntry>],
    viewers: &Viewers,
    involvement: &Involvement,
) -> Vec<Arc<EnvironmentEntry>> {
    entries
        .iter()
        .filter(|entry| match involvement {
            Involvement::Reviewing => entry.viewer_review_requested,
            Involvement::Authored => is_authored_by_viewer(entry, viewers),
            _ => true,
        })
        .cloned()
        .collect()
}

/// Rows already read, kept only where new state / project / host filters would keep them, so a
/// filter change narrows the page instead of blanking it while the hosts answer.
pub fn narrow_to_filters(
    entries: &[Arc<EnvironmentEntry>],
    state: &ListState,
    project_id: Option<&t3_protocol::ProjectId>,
    host: Option<&str>,
) -> Vec<Arc<EnvironmentEntry>> {
    entries
        .iter()
        .filter(|entry| {
            state_matches(state, &entry.state)
                && project_id.is_none_or(|project| &entry.project_id == project)
                && host.is_none_or(|host| entry.host == host)
        })
        .cloned()
        .collect()
}

/// Whether a list state filter holds a pull request state.
pub fn state_matches(filter: &ListState, state: &PullRequestState) -> bool {
    match filter {
        ListState::All => true,
        ListState::Open => *state == PullRequestState::Open,
        ListState::Closed => *state == PullRequestState::Closed,
        ListState::Merged => *state == PullRequestState::Merged,
        ListState::Other(_) => true,
    }
}

/// `author:me` / `author:@me` resolved to the host's signed-in login; unchanged without one
/// (`resolvePullRequestAuthorFilter`).
pub fn resolve_author_filter(author: &str, viewer: Option<&str>) -> String {
    let trimmed = author.trim();
    let is_me = trimmed
        .strip_prefix('@')
        .unwrap_or(trimmed)
        .eq_ignore_ascii_case("me");
    match viewer {
        Some(viewer) if is_me && !viewer.trim().is_empty() => viewer.to_owned(),
        _ => trimmed.to_owned(),
    }
}

/// The narrowings a row can be judged by from its own fields (`matchesPullRequestFilters`).
/// `checks` is the host's alone: no row carries it.
pub fn matches_filters(
    entry: &EnvironmentEntry,
    filters: &PullRequestListFilters,
    viewer: Option<&str>,
) -> bool {
    let labels: Vec<String> = entry
        .labels
        .iter()
        .map(|label| label.name.trim().to_lowercase())
        .collect();
    let holds = |label: &String| labels.contains(&label.trim().to_lowercase());
    let draft_ok = filters
        .draft
        .as_deref()
        .is_none_or(|draft| entry.is_draft == (draft == "only"));
    let review_ok = filters.review.as_deref().is_none_or(|review| {
        if review == "none" {
            entry.review_decision.is_none()
        } else {
            entry
                .review_decision
                .as_ref()
                .is_some_and(|decision| decision.as_str() == review)
        }
    });
    let labels_ok = filters
        .labels
        .as_ref()
        .is_none_or(|groups| groups.iter().all(|group| group.iter().any(holds)));
    let excluded_ok = filters
        .excluded_labels
        .as_ref()
        .is_none_or(|excluded| !excluded.iter().any(holds));
    let author_ok = filters.author.as_deref().is_none_or(|author| {
        entry.author.as_ref().is_some_and(|actor| {
            actor.login.to_lowercase() == resolve_author_filter(author, viewer).to_lowercase()
        })
    });
    draft_ok && review_ok && labels_ok && excluded_ok && author_ok
}

/// Local grouping: Authored, Review requested, Others; authored wins; empty groups dropped.
pub fn group_by_involvement(entries: &[Arc<EnvironmentEntry>], viewers: &Viewers) -> Vec<Group> {
    let mut authored = Vec::new();
    let mut requested = Vec::new();
    let mut others = Vec::new();
    for entry in entries {
        if is_authored_by_viewer(entry, viewers) {
            authored.push(entry.clone());
        } else if entry.viewer_review_requested {
            requested.push(entry.clone());
        } else {
            others.push(entry.clone());
        }
    }
    labeled_groups([
        (GroupKey::Authored, authored),
        (GroupKey::ReviewRequested, requested),
        (GroupKey::Others, others),
    ])
}

fn labeled_groups(groups: [(GroupKey, Vec<Arc<EnvironmentEntry>>); 3]) -> Vec<Group> {
    groups
        .into_iter()
        .filter(|(_, entries)| !entries.is_empty())
        .map(|(key, entries)| Group {
            key,
            labeled: true,
            entries,
        })
        .collect()
}

/// The priority groups from the hosts' own authored / reviewing reads, with the feed filling
/// Others in its own order (`partitionPullRequestsWithPriority`). A feed copy of a partitioned
/// row replaces it in place; a row in both partitions is authored.
pub fn partition_with_priority(
    entries: &[Arc<EnvironmentEntry>],
    authored: &[Arc<EnvironmentEntry>],
    review_requested: &[Arc<EnvironmentEntry>],
) -> Vec<Group> {
    let mut authored_by_key: Vec<(String, Arc<EnvironmentEntry>)> = Vec::new();
    let mut seen = HashSet::new();
    for entry in authored {
        let key = entry_key(entry);
        if seen.insert(key.clone()) {
            authored_by_key.push((key, entry.clone()));
        }
    }
    let mut review_by_key: Vec<(String, Arc<EnvironmentEntry>)> = Vec::new();
    for entry in review_requested {
        let key = entry_key(entry);
        if seen.insert(key.clone()) {
            review_by_key.push((key, entry.clone()));
        }
    }
    let mut others = Vec::new();
    for entry in entries {
        let key = entry_key(entry);
        if let Some(slot) = authored_by_key.iter_mut().find(|(held, _)| *held == key) {
            slot.1 = entry.clone();
        } else if let Some(slot) = review_by_key.iter_mut().find(|(held, _)| *held == key) {
            slot.1 = entry.clone();
        } else {
            others.push(entry.clone());
        }
    }
    let by_recency = |rows: Vec<(String, Arc<EnvironmentEntry>)>| {
        let mut rows: Vec<_> = rows.into_iter().map(|(_, entry)| entry).collect();
        rows.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        rows
    };
    labeled_groups([
        (GroupKey::Authored, by_recency(authored_by_key)),
        (GroupKey::ReviewRequested, by_recency(review_by_key)),
        (GroupKey::Others, others),
    ])
}

/// One author in the Filters menu: how many in-state rows they have, and how many merges.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorFacet {
    pub actor: PullRequestActor,
    pub count: usize,
    pub merged_count: usize,
}

/// One label in the Filters menu with its in-state row count.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelFacet {
    pub label: PullRequestLabel,
    pub count: usize,
}

/// The Filters menu's author and label choices (`collectPullRequestListFacets`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facets {
    pub authors: Vec<AuthorFacet>,
    pub labels: Vec<LabelFacet>,
}

/// Authors and labels from rows already read, counted for the state in view.
pub fn collect_facets(entries: &[Arc<EnvironmentEntry>], state: &ListState) -> Facets {
    let mut unique: Vec<&Arc<EnvironmentEntry>> = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries.iter().rev() {
        if seen.insert(entry_key(entry)) {
            unique.push(entry);
        }
    }
    unique.reverse();
    let mut authors: Vec<(String, AuthorFacet)> = Vec::new();
    let mut labels: Vec<(String, LabelFacet)> = Vec::new();
    for entry in unique {
        let in_state = state_matches(state, &entry.state);
        if let Some(actor) = &entry.author
            && let Some(key) = normalize(Some(&actor.login))
        {
            match authors.iter_mut().find(|(held, _)| *held == key) {
                Some((_, facet)) => {
                    facet.count += usize::from(in_state);
                    facet.merged_count += usize::from(entry.state == PullRequestState::Merged);
                }
                None => authors.push((
                    key,
                    AuthorFacet {
                        actor: actor.clone(),
                        count: usize::from(in_state),
                        merged_count: usize::from(entry.state == PullRequestState::Merged),
                    },
                )),
            }
        }
        if !in_state {
            continue;
        }
        for label in &entry.labels {
            let Some(key) = normalize(Some(&label.name)) else {
                continue;
            };
            match labels.iter_mut().find(|(held, _)| *held == key) {
                Some((_, facet)) => {
                    facet.count += 1;
                    if facet.label.color.is_none() {
                        facet.label.color = label.color.clone();
                    }
                }
                None => labels.push((
                    key,
                    LabelFacet {
                        label: label.clone(),
                        count: 1,
                    },
                )),
            }
        }
    }
    let mut authors: Vec<AuthorFacet> = authors
        .into_iter()
        .map(|(_, facet)| facet)
        .filter(|facet| facet.count > 0)
        .collect();
    authors.sort_by(|left, right| {
        right
            .merged_count
            .cmp(&left.merged_count)
            .then(right.count.cmp(&left.count))
            .then_with(|| left.actor.login.cmp(&right.actor.login))
    });
    let mut labels: Vec<LabelFacet> = labels.into_iter().map(|(_, facet)| facet).collect();
    labels.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.name.cmp(&right.label.name))
    });
    Facets { authors, labels }
}

/// A project error with the environment that reported it.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvironmentProjectError {
    pub environment_id: EnvironmentId,
    pub error: PullRequestListProjectError,
}

/// Every environment's listing read as one (`MergedPullRequestList`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MergedList {
    pub viewers: Viewers,
    /// One per host: readable if any environment could read it, searched on the host only if
    /// every environment's answer was.
    pub providers: Vec<PullRequestProviderSummary>,
    /// Newest update first.
    pub entries: Vec<Arc<EnvironmentEntry>>,
    pub errors: Vec<EnvironmentProjectError>,
    pub truncated: bool,
    /// Per environment, each value the `cursors` to send back to it.
    pub next_cursors: BTreeMap<EnvironmentId, BTreeMap<String, String>>,
    /// Environments with rows still on their hosts.
    pub truncated_environments: Vec<EnvironmentId>,
}

/// Folds environment answers into one list; `None` when nothing has answered.
pub fn merge_lists(answers: &[(EnvironmentId, PullRequestListResult)]) -> Option<MergedList> {
    if answers.is_empty() {
        return None;
    }
    let mut merged = MergedList::default();
    for (environment_id, answer) in answers {
        for (host, login) in &answer.viewers {
            merged
                .viewers
                .insert(format!("{} {host}", environment_id.0), login.clone());
        }
        for provider in &answer.providers {
            match merged
                .providers
                .iter_mut()
                .find(|held| held.host == provider.host)
            {
                None => merged.providers.push(provider.clone()),
                Some(held) => {
                    let mut next = if held.configured {
                        held.clone()
                    } else {
                        provider.clone()
                    };
                    next.project_count = held.project_count + provider.project_count;
                    next.searches_on_host = held.searches_on_host && provider.searches_on_host;
                    next.configured = held.configured || provider.configured;
                    *held = next;
                }
            }
        }
        merged.entries.extend(answer.entries.iter().map(|entry| {
            Arc::new(EnvironmentEntry {
                environment_id: environment_id.clone(),
                entry: entry.clone(),
            })
        }));
        merged
            .errors
            .extend(answer.errors.iter().map(|error| EnvironmentProjectError {
                environment_id: environment_id.clone(),
                error: error.clone(),
            }));
        merged.truncated |= answer.truncated;
        if answer.truncated {
            merged.truncated_environments.push(environment_id.clone());
        }
        if !answer.next_cursors.is_empty() {
            merged
                .next_cursors
                .insert(environment_id.clone(), answer.next_cursors.clone());
        }
    }
    merged
        .entries
        .sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Some(merged)
}

/// Sorted, comma-joined environment ids: names the set a snapshot was read from.
pub fn environment_set_key(environment_ids: &[EnvironmentId]) -> String {
    let mut ids: Vec<&str> = environment_ids.iter().map(|id| id.0.as_str()).collect();
    ids.sort_unstable();
    ids.join(",")
}

/// What a row says the moment an action is sent, before any host confirms it.
#[derive(Clone, Debug, PartialEq)]
pub struct ListOverride {
    pub state: PullRequestState,
    pub is_draft: Option<bool>,
    pub updated_at: String,
    /// Which action wrote it, so a failure takes back only its own note.
    pub token: u64,
    /// When it was written, epoch millis.
    pub at: i64,
}

/// The override an action writes, or `None` for actions that do not move state
/// (`pullRequestOverrideAfterAction`).
pub fn override_after_action(
    entry: &EnvironmentEntry,
    action: &PullRequestAction,
    now: i64,
    token: u64,
) -> Option<ListOverride> {
    let (state, is_draft) = match action {
        PullRequestAction::Close => (PullRequestState::Closed, None),
        PullRequestAction::Reopen => (PullRequestState::Open, None),
        PullRequestAction::Merge => (PullRequestState::Merged, None),
        PullRequestAction::Draft => (entry.state.clone(), Some(true)),
        PullRequestAction::Ready => (entry.state.clone(), Some(false)),
        _ => return None,
    };
    Some(ListOverride {
        state,
        is_draft,
        updated_at: crate::time::format_timestamp(now).unwrap_or_default(),
        token,
        at: now,
    })
}

/// Rows with their pending overrides written on; rows the state filter no longer holds are
/// dropped.
pub fn apply_overrides(
    entries: &[Arc<EnvironmentEntry>],
    overrides: &BTreeMap<String, ListOverride>,
    state: &ListState,
) -> Vec<Arc<EnvironmentEntry>> {
    if overrides.is_empty() {
        return entries.to_vec();
    }
    entries
        .iter()
        .filter_map(|entry| match overrides.get(&entry_key(entry)) {
            None => Some(entry.clone()),
            Some(pending) if !state_matches(state, &pending.state) => None,
            Some(pending) => {
                let mut next = (**entry).clone();
                next.entry.state = pending.state.clone();
                if let Some(is_draft) = pending.is_draft {
                    next.entry.is_draft = is_draft;
                }
                next.entry.updated_at = pending.updated_at.clone();
                Some(Arc::new(next))
            }
        })
        .collect()
}

/// How long a disagreeing read is taken for a stale one rather than for news.
const OVERRIDE_TRUST_MS: i64 = 60_000;

/// Overrides an answer confirmed are dropped, the rest kept (`settlePullRequestOverrides`). An
/// absent row confirms nothing; a disagreeing row is stale for a minute, then news.
pub fn settle_overrides(
    overrides: &BTreeMap<String, ListOverride>,
    answered: &[Arc<EnvironmentEntry>],
    now: i64,
) -> BTreeMap<String, ListOverride> {
    if overrides.is_empty() {
        return BTreeMap::new();
    }
    let by_key: HashMap<String, &Arc<EnvironmentEntry>> = answered
        .iter()
        .map(|entry| (entry_key(entry), entry))
        .collect();
    overrides
        .iter()
        .filter(|(key, pending)| match by_key.get(*key) {
            None => true,
            Some(row) => {
                let agrees = row.state == pending.state
                    && pending.is_draft.is_none_or(|draft| row.is_draft == draft);
                !agrees && now - pending.at <= OVERRIDE_TRUST_MS
            }
        })
        .map(|(key, pending)| (key.clone(), pending.clone()))
        .collect()
}

/// A fresh answer with unchanged rows handed back as the `Arc`s already held, so only the rows
/// that changed re-render (`reusePullRequestEntries`).
pub fn reuse_entries(
    previous: &[Arc<EnvironmentEntry>],
    next: Vec<Arc<EnvironmentEntry>>,
) -> Vec<Arc<EnvironmentEntry>> {
    if previous.is_empty() {
        return next;
    }
    let held: HashMap<String, &Arc<EnvironmentEntry>> = previous
        .iter()
        .map(|entry| (entry_key(entry), entry))
        .collect();
    next.into_iter()
        .map(|entry| match held.get(&entry_key(&entry)) {
            Some(before) if ***before == *entry => (*before).clone(),
            _ => entry,
        })
        .collect()
}
