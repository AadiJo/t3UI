//! Failure modes this file guards, written before the code (ported from the fork's
//! `pullRequestList.logic.test.ts`, `pullRequestProjectFilter.logic.test.ts`,
//! `pullRequestProjectAssignment.logic.test.ts` and `pullRequestPresentation.test.ts`):
//!
//! Query parsing
//! - a quoted label with spaces (`label:"needs design"`) is split or loses its words
//! - `label:a,b` becomes two AND groups instead of one OR group; `-label:a,b` keeps either row
//! - an unknown key (`size:XXL`) is searched as text instead of read as a namespaced label, or
//!   `size:S,XS` loses the namespace on its second part, or `size:S,size:XS` doubles it
//! - a pasted URL becomes a label named after its scheme
//! - `"size:XXL"` (quoted whole) is not searched as written
//! - a known key with an empty or unknown value (`status:`, `draft:maybe`) is dropped instead of
//!   searched as text; a negated `-author:` is dropped instead of searched
//! - an unbalanced quote swallows the rest of the line
//! - more than ten names per qualifier reach the wire
//!
//! Matching and grouping
//! - `#12` / `12` does not find pull request 12; matching is case sensitive
//! - `author:me` compares against the literal "me", or resolves without a viewer
//! - `review:none` matches rows with a decision; label groups use AND within a group
//! - authorship reads the host-keyed viewer before the environment-keyed one, so two machines
//!   signed in as different people file each other's work as authored
//! - a row both authored and review-requested shows up twice; empty groups render
//! - a feed copy of a partitioned row moves it into Others or leaves the stale copy in place
//! - the entry key ignores the environment, so two machines' copies collapse into one row
//!
//! Merging environments
//! - viewers from two environments overwrite each other; provider `configured` is not "any",
//!   `searchesOnHost` is not "all", project counts are not summed
//! - entries are not re-sorted newest first after merging; an empty cursor map is kept
//!
//! Sorting
//! - a conflicting row ranks as ready because its checks pass and it is approved
//! - the readiness sort reorders a search's relevance ranking
//! - an unmeasured diff sorts as the smallest
//! - `blocked` ranks the Others group when involvement is "all"
//!
//! Scoring, overrides, reuse
//! - score tiers out of order; a number query matches a title containing the number
//! - an action that does not move state writes an override; an override survives an answer that
//!   agrees, or is cleared by a stale disagreeing answer inside the trust window
//! - unchanged rows are reallocated on refresh, so every row repaints
//!
//! Scope and projects
//! - a stale project id narrows the list to nothing once projects are known, or is dropped
//!   before they are
//! - an ambiguous project id without a server picks the first match
//! - a shared repository is listed by two servers; a project without an identity is dropped
//! - two projects with one title are indistinguishable in the project menu
//!
//! Presentation
//! - a stale draft flag paints a merged pull request as a draft
//! - the checks rollup ignores cancelled runs, or claims success with no checks
//! - an unknown provider's requirement text is invented
//! - label colors accept short or malformed hex

use std::{collections::BTreeMap, sync::Arc};

use t3_protocol::{
    EnvironmentId, ProjectId,
    orchestration::{PullRequestActor, PullRequestState},
    pull_requests::{
        CheckStatus, ChecksState, ListState, Mergeability, PullRequestAction, PullRequestCheck,
        PullRequestLabel, PullRequestListEntry, PullRequestListFilters, PullRequestListResult,
        PullRequestProviderSummary, ReviewDecision, SourceControlProviderKind,
    },
};

use super::*;

fn env(id: &str) -> EnvironmentId {
    EnvironmentId(id.to_owned())
}

fn project(id: &str) -> ProjectId {
    ProjectId(id.to_owned())
}

fn actor(login: &str) -> PullRequestActor {
    PullRequestActor {
        is_bot: None,
        login: login.to_owned(),
        name: None,
        avatar_url: None,
    }
}

fn raw_entry(number: u64, updated_at: &str) -> PullRequestListEntry {
    PullRequestListEntry {
        stack: None,
        provider: SourceControlProviderKind::Github,
        host: "github.com".to_owned(),
        project_id: project("p1"),
        project_title: "aurora-web".to_owned(),
        repository: "acme/aurora-web".to_owned(),
        number,
        title: format!("Pull request {number}"),
        url: format!("https://github.com/acme/aurora-web/pull/{number}"),
        author: Some(actor("octocat")),
        head_branch: format!("feature/{number}"),
        base_branch: "main".to_owned(),
        state: PullRequestState::Open,
        is_draft: false,
        mergeability: Mergeability::Mergeable,
        additions: 0,
        deletions: 0,
        created_at: updated_at.to_owned(),
        updated_at: updated_at.to_owned(),
        observed_at: None,
        viewer_review_requested: false,
        labels: Vec::new(),
        review_decision: None,
        checks_state: None,
    }
}

fn entry(number: u64, updated_at: &str) -> Arc<EnvironmentEntry> {
    Arc::new(EnvironmentEntry {
        environment_id: env("e1"),
        entry: raw_entry(number, updated_at),
    })
}

fn with(
    entry: &Arc<EnvironmentEntry>,
    f: impl FnOnce(&mut EnvironmentEntry),
) -> Arc<EnvironmentEntry> {
    let mut next = (**entry).clone();
    f(&mut next);
    Arc::new(next)
}

fn numbers(entries: &[Arc<EnvironmentEntry>]) -> Vec<u64> {
    entries.iter().map(|entry| entry.number).collect()
}

// ---------------------------------------------------------------------------------------------
// Query parsing

#[test]
fn parses_quoted_and_or_labels() {
    let parsed = parse_query(r#"fix label:"needs design" label:a,b -label:wip,stale"#);
    assert_eq!(parsed.text, "fix");
    assert_eq!(
        parsed.filters.labels,
        Some(vec![
            vec!["needs design".to_owned()],
            vec!["a".to_owned(), "b".to_owned()]
        ])
    );
    assert_eq!(
        parsed.filters.excluded_labels,
        Some(vec!["wip".to_owned(), "stale".to_owned()])
    );
}

#[test]
fn quoted_comma_names_one_label() {
    let parsed = parse_query(r#"label:"needs,triage""#);
    assert_eq!(
        parsed.filters.labels,
        Some(vec![vec!["needs,triage".to_owned()]])
    );
}

#[test]
fn unknown_keys_are_namespaced_labels() {
    let parsed = parse_query("size:S,XS area:web,area:api -vouch:trusted");
    assert_eq!(
        parsed.filters.labels,
        Some(vec![
            vec!["size:S".to_owned(), "size:XS".to_owned()],
            vec!["area:web".to_owned(), "area:api".to_owned()],
        ])
    );
    assert_eq!(
        parsed.filters.excluded_labels,
        Some(vec!["vouch:trusted".to_owned()])
    );
    assert_eq!(parsed.text, "");
}

#[test]
fn urls_and_quoted_qualifiers_stay_text() {
    let parsed = parse_query(r#"https://github.com/acme/x/pull/1 "size:XXL""#);
    assert_eq!(parsed.filters, PullRequestListFilters::default());
    assert_eq!(
        parsed.text,
        r#"https://github.com/acme/x/pull/1 "size:XXL""#
    );
}

#[test]
fn known_keys_with_unusable_values_stay_text() {
    let parsed = parse_query("status: draft:maybe -author:bob review:great");
    assert_eq!(parsed.filters, PullRequestListFilters::default());
    assert_eq!(parsed.text, "status: draft:maybe -author:bob review:great");
}

#[test]
fn known_qualifiers_map_to_filters() {
    let parsed = parse_query("author:octocat draft:true review:changes_requested status:failure");
    assert_eq!(parsed.text, "");
    assert_eq!(parsed.filters.author.as_deref(), Some("octocat"));
    assert_eq!(parsed.filters.draft.as_deref(), Some("only"));
    assert_eq!(parsed.filters.review.as_deref(), Some("changes-requested"));
    assert_eq!(parsed.filters.checks, Some(ChecksState::Failing));
    assert_eq!(
        parse_query("draft:false").filters.draft.as_deref(),
        Some("hide")
    );
    assert_eq!(
        parse_query("checks:passing").filters.checks,
        Some(ChecksState::Passing)
    );
}

#[test]
fn unbalanced_quote_does_not_swallow_the_line() {
    let parsed = parse_query(r#"foo"bar baz"#);
    assert_eq!(parsed.text, "foo bar baz");
}

#[test]
fn qualifier_values_are_bounded() {
    let names = (0..14).map(|index| format!("l{index}")).collect::<Vec<_>>();
    let parsed = parse_query(&format!("label:{}", names.join(",")));
    assert_eq!(parsed.filters.labels.unwrap()[0].len(), 10);
}

// ---------------------------------------------------------------------------------------------
// Matching

#[test]
fn query_matches_number_and_fields_case_insensitively() {
    let row = entry(12, "2026-10-01T00:00:00Z");
    assert!(matches_query(&row, "#12"));
    assert!(matches_query(&row, "PULL REQUEST"));
    assert!(matches_query(&row, "feature/12"));
    assert!(matches_query(&row, "OctoCat"));
    assert!(!matches_query(&row, "missing"));
    assert!(matches_query(&row, "   "));
}

#[test]
fn filters_resolve_author_me_and_review_none() {
    let row = with(&entry(1, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.labels = vec![
            PullRequestLabel {
                name: "Bug".to_owned(),
                color: None,
            },
            PullRequestLabel {
                name: "size:S".to_owned(),
                color: None,
            },
        ];
    });
    let author_me = PullRequestListFilters {
        author: Some("@me".to_owned()),
        ..Default::default()
    };
    assert!(matches_filters(&row, &author_me, Some("octocat")));
    assert!(!matches_filters(&row, &author_me, None));
    let review_none = PullRequestListFilters {
        review: Some("none".to_owned()),
        ..Default::default()
    };
    assert!(matches_filters(&row, &review_none, None));
    let reviewed = with(&row, |entry| {
        entry.entry.review_decision = Some(ReviewDecision::Approved)
    });
    assert!(!matches_filters(&reviewed, &review_none, None));
    let labels = PullRequestListFilters {
        labels: Some(vec![
            vec!["bug".to_owned()],
            vec!["size:XS".to_owned(), "size:S".to_owned()],
        ]),
        ..Default::default()
    };
    assert!(matches_filters(&row, &labels, None));
    let excluded = PullRequestListFilters {
        excluded_labels: Some(vec!["BUG".to_owned()]),
        ..Default::default()
    };
    assert!(!matches_filters(&row, &excluded, None));
}

// ---------------------------------------------------------------------------------------------
// Involvement and grouping

fn viewers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, login)| ((*key).to_owned(), (*login).to_owned()))
        .collect()
}

#[test]
fn authorship_prefers_the_environment_keyed_viewer() {
    let row = entry(1, "2026-10-01T00:00:00Z");
    let mine = viewers(&[("e1 github.com", "octocat"), ("github.com", "someone-else")]);
    let theirs = viewers(&[("e1 github.com", "someone-else"), ("github.com", "octocat")]);
    assert_eq!(entry_viewer(&row, &mine).as_deref(), Some("octocat"));
    assert_eq!(
        filter_by_involvement(std::slice::from_ref(&row), &mine, &Involvement::Authored).len(),
        1
    );
    assert!(
        filter_by_involvement(std::slice::from_ref(&row), &theirs, &Involvement::Authored)
            .is_empty()
    );
    let legacy = viewers(&[("github.com", "octocat")]);
    assert_eq!(entry_viewer(&row, &legacy).as_deref(), Some("octocat"));
}

#[test]
fn groups_authored_first_without_duplicates_or_empty_groups() {
    let authored_and_requested = with(&entry(1, "2026-10-03T00:00:00Z"), |entry| {
        entry.entry.viewer_review_requested = true
    });
    let requested = with(&entry(2, "2026-10-02T00:00:00Z"), |entry| {
        entry.entry.author = Some(actor("hubot"));
        entry.entry.viewer_review_requested = true;
    });
    let other = with(&entry(3, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.author = Some(actor("hubot"))
    });
    let viewers = viewers(&[("github.com", "octocat")]);
    let groups = group_by_involvement(
        &[other.clone(), requested, authored_and_requested],
        &viewers,
    );
    let keys: Vec<_> = groups.iter().map(|group| group.key).collect();
    assert_eq!(
        keys,
        [
            GroupKey::Authored,
            GroupKey::ReviewRequested,
            GroupKey::Others
        ]
    );
    assert_eq!(groups[0].key.label(), "Authored");
    assert_eq!(groups[1].key.label(), "Review requested");
    let only_others = group_by_involvement(&[other], &viewers);
    assert_eq!(only_others.len(), 1);
    assert_eq!(only_others[0].key, GroupKey::Others);
}

#[test]
fn partitions_replace_in_place_and_keep_feed_order_for_others() {
    let old_authored = entry(1, "2026-09-01T00:00:00Z");
    let feed_authored = with(&old_authored, |entry| {
        entry.entry.title = "Fresh title".to_owned()
    });
    let both = entry(2, "2026-09-02T00:00:00Z");
    let other_a = entry(3, "2026-10-02T00:00:00Z");
    let other_b = entry(4, "2026-10-01T00:00:00Z");
    let groups = partition_with_priority(
        &[other_a, feed_authored, other_b],
        &[old_authored, both.clone()],
        &[both],
    );
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].key, GroupKey::Authored);
    assert_eq!(numbers(&groups[0].entries), [2, 1]);
    assert_eq!(groups[0].entries[1].title, "Fresh title");
    assert_eq!(groups[1].key, GroupKey::Others);
    assert_eq!(numbers(&groups[1].entries), [3, 4]);
}

#[test]
fn entry_key_carries_environment_and_host() {
    let row = entry(7, "2026-10-01T00:00:00Z");
    assert_eq!(entry_key(&row), "e1:github.com:acme/aurora-web#7");
    let other_env = with(&row, |entry| entry.environment_id = env("e2"));
    assert_ne!(entry_key(&row), entry_key(&other_env));
}

// ---------------------------------------------------------------------------------------------
// Merging

fn provider(
    host: &str,
    configured: bool,
    searches: bool,
    count: u32,
) -> PullRequestProviderSummary {
    PullRequestProviderSummary {
        host: host.to_owned(),
        kind: SourceControlProviderKind::Github,
        searches_on_host: searches,
        project_count: count,
        configured,
        detail: (!configured).then(|| "GitHub CLI is not authenticated.".to_owned()),
    }
}

#[test]
fn merges_environment_answers() {
    let first = PullRequestListResult {
        viewers: viewers(&[("github.com", "octocat")]),
        providers: vec![provider("github.com", false, true, 1)],
        entries: vec![raw_entry(1, "2026-10-01T00:00:00Z")],
        errors: Vec::new(),
        truncated: true,
        next_cursors: BTreeMap::new(),
    };
    let second = PullRequestListResult {
        viewers: viewers(&[("github.com", "hubot")]),
        providers: vec![provider("github.com", true, false, 2)],
        entries: vec![raw_entry(2, "2026-10-02T00:00:00Z")],
        errors: Vec::new(),
        truncated: false,
        next_cursors: BTreeMap::from([("github.com acme/x".to_owned(), "c2".to_owned())]),
    };
    let merged = merge_lists(&[(env("e1"), first), (env("e2"), second)]).unwrap();
    assert_eq!(
        merged.viewers.get("e1 github.com").map(String::as_str),
        Some("octocat")
    );
    assert_eq!(
        merged.viewers.get("e2 github.com").map(String::as_str),
        Some("hubot")
    );
    assert_eq!(merged.providers.len(), 1);
    let host = &merged.providers[0];
    assert!(host.configured);
    assert!(!host.searches_on_host);
    assert_eq!(host.project_count, 3);
    assert_eq!(host.detail, None);
    assert_eq!(numbers(&merged.entries), [2, 1]);
    assert_eq!(merged.entries[0].environment_id, env("e2"));
    assert!(merged.truncated);
    assert_eq!(merged.truncated_environments, [env("e1")]);
    assert_eq!(merged.next_cursors.keys().collect::<Vec<_>>(), [&env("e2")]);
    assert!(merge_lists(&[]).is_none());
}

// ---------------------------------------------------------------------------------------------
// Sorting and scoring

#[test]
fn readiness_ranks_conflicts_last_and_measured_first() {
    let ready = with(&entry(1, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.checks_state = Some(ChecksState::Passing);
        entry.entry.review_decision = Some(ReviewDecision::Approved);
        entry.entry.additions = 50;
    });
    let conflicting = with(&ready, |entry| {
        entry.entry.number = 2;
        entry.entry.mergeability = Mergeability::Conflicting;
    });
    let passing = with(&entry(3, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.checks_state = Some(ChecksState::Passing)
    });
    let draft = with(&passing, |entry| {
        entry.entry.number = 4;
        entry.entry.is_draft = true;
    });
    let merged = with(&entry(5, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.state = PullRequestState::Merged
    });
    let small = with(&entry(6, "2026-09-01T00:00:00Z"), |entry| {
        entry.entry.checks_state = Some(ChecksState::Passing);
        entry.entry.review_decision = Some(ReviewDecision::Approved);
        entry.entry.additions = 3;
    });
    let ranked = rank_by_merge_readiness(
        &[conflicting, merged, draft, passing, ready, small],
        &|entry| entry.additions + entry.deletions > 0,
    );
    assert_eq!(numbers(&ranked), [6, 1, 3, 4, 5, 2]);
}

#[test]
fn readiness_leaves_a_search_alone_and_blocked_skips_others_for_all() {
    let a = entry(1, "2026-10-01T00:00:00Z");
    let b = with(&entry(2, "2026-10-02T00:00:00Z"), |entry| {
        entry.entry.checks_state = Some(ChecksState::Passing)
    });
    let groups = vec![Group {
        key: GroupKey::Others,
        labeled: true,
        entries: vec![a, b],
    }];
    let measured = |entry: &EnvironmentEntry| entry.additions + entry.deletions > 0;
    let searched = sort_groups(
        &groups,
        ListSort::Ready,
        "fix",
        &measured,
        &Involvement::All,
    );
    assert_eq!(numbers(&searched[0].entries), [1, 2]);
    let ready = sort_groups(&groups, ListSort::Ready, "", &measured, &Involvement::All);
    assert_eq!(numbers(&ready[0].entries), [2, 1]);
    let blocked = sort_groups(&groups, ListSort::Blocked, "", &measured, &Involvement::All);
    assert_eq!(numbers(&blocked[0].entries), [1, 2]);
}

#[test]
fn size_sorts_put_unmeasured_last() {
    let big = with(&entry(1, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.additions = 500
    });
    let small = with(&entry(2, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.deletions = 5
    });
    let unknown = entry(3, "2026-10-02T00:00:00Z");
    let groups = vec![Group {
        key: GroupKey::Others,
        labeled: false,
        entries: vec![unknown, small, big],
    }];
    let measured = |entry: &EnvironmentEntry| entry.additions + entry.deletions > 0;
    let smallest = sort_groups(
        &groups,
        ListSort::Smallest,
        "",
        &measured,
        &Involvement::All,
    );
    assert_eq!(numbers(&smallest[0].entries), [2, 1, 3]);
    let largest = sort_groups(&groups, ListSort::Largest, "", &measured, &Involvement::All);
    assert_eq!(numbers(&largest[0].entries), [1, 2, 3]);
}

#[test]
fn score_tiers() {
    let row = with(&entry(42, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.title = "Add welcome wizard".to_owned();
        entry.entry.head_branch = "onboarding-flow".to_owned();
    });
    assert_eq!(score_match(&row, "#42"), 100);
    assert_eq!(score_match(&row, "4"), 0);
    assert_eq!(score_match(&row, "add welcome wizard"), 90);
    assert_eq!(score_match(&row, "welcome"), 80);
    assert_eq!(score_match(&row, "wizard welcome"), 70);
    assert_eq!(score_match(&row, "onboarding"), 60);
    assert_eq!(score_match(&row, "octo"), 50);
    assert_eq!(score_match(&row, "aurora"), 40);
    assert_eq!(score_match(&row, "wizard zebra"), 30);
    assert_eq!(score_match(&row, "zebra"), 10);
}

// ---------------------------------------------------------------------------------------------
// Overrides and reuse

#[test]
fn overrides_follow_actions_and_settle_on_agreement() {
    let row = entry(1, "2026-10-01T00:00:00Z");
    assert!(override_after_action(&row, &PullRequestAction::UpdateBranch, 0, 1).is_none());
    let closed = override_after_action(&row, &PullRequestAction::Close, 1_000, 1).unwrap();
    assert_eq!(closed.state, PullRequestState::Closed);
    let draft = override_after_action(&row, &PullRequestAction::Draft, 1_000, 2).unwrap();
    assert_eq!(draft.state, PullRequestState::Open);
    assert_eq!(draft.is_draft, Some(true));

    let key = entry_key(&row);
    let overrides = BTreeMap::from([(key.clone(), closed)]);
    assert!(apply_overrides(std::slice::from_ref(&row), &overrides, &ListState::Open).is_empty());
    let applied = apply_overrides(std::slice::from_ref(&row), &overrides, &ListState::All);
    assert_eq!(applied[0].state, PullRequestState::Closed);

    let stale = settle_overrides(&overrides, std::slice::from_ref(&row), 30_000);
    assert_eq!(
        stale.len(),
        1,
        "a disagreeing answer inside the window is a stale read"
    );
    let news = settle_overrides(&overrides, std::slice::from_ref(&row), 62_000);
    assert!(news.is_empty(), "after a minute the host's answer is news");
    let agreeing = with(&row, |entry| entry.entry.state = PullRequestState::Closed);
    assert!(settle_overrides(&overrides, &[agreeing], 2_000).is_empty());
    assert_eq!(settle_overrides(&overrides, &[], 90_000).len(), 1);
}

#[test]
fn reuse_keeps_unchanged_rows() {
    let a = entry(1, "2026-10-01T00:00:00Z");
    let b = entry(2, "2026-10-02T00:00:00Z");
    let previous = vec![a.clone(), b.clone()];
    let same = reuse_entries(
        &previous,
        vec![
            entry(1, "2026-10-01T00:00:00Z"),
            entry(2, "2026-10-02T00:00:00Z"),
        ],
    );
    assert!(Arc::ptr_eq(&same[0], &a) && Arc::ptr_eq(&same[1], &b));
    let changed = reuse_entries(
        &previous,
        vec![
            entry(2, "2026-10-02T00:00:00Z"),
            with(&a, |entry| entry.entry.title = "x".into()),
        ],
    );
    assert!(Arc::ptr_eq(&changed[0], &b));
    assert!(!Arc::ptr_eq(&changed[1], &a));
}

// ---------------------------------------------------------------------------------------------
// Scope and projects

fn scope_project(id: &str, environment: &str, title: &str, key: Option<&str>) -> ScopeProject {
    ScopeProject {
        id: project(id),
        environment_id: env(environment),
        title: title.to_owned(),
        workspace_root: format!("/work/{environment}/{id}"),
        canonical_key: key.map(str::to_owned),
    }
}

#[test]
fn project_scope_waits_for_projects_and_drops_stale_ids() {
    let projects = [scope_project("p1", "e1", "Aurora", None)];
    assert_eq!(
        resolve_project_scope(Some(&project("gone")), &projects, false),
        Some(project("gone"))
    );
    assert_eq!(
        resolve_project_scope(Some(&project("gone")), &projects, true),
        None
    );
    assert_eq!(
        resolve_project_scope(Some(&project("p1")), &projects, true),
        Some(project("p1"))
    );
}

#[test]
fn ambiguous_project_ids_need_a_server() {
    let projects = [
        scope_project("p1", "e1", "Aurora", None),
        scope_project("p1", "e2", "Aurora", None),
    ];
    assert!(find_scoped_project(&projects, None, Some(&project("p1"))).is_none());
    assert_eq!(
        find_scoped_project(&projects, Some(&env("e2")), Some(&project("p1")))
            .map(|p| &p.environment_id),
        Some(&env("e2"))
    );
    let ids = [env("e1"), env("e2"), env("e3")];
    assert_eq!(
        resolve_query_environment_ids(&ids, &projects, None, Some(&project("p1")), true),
        [env("e1"), env("e2")]
    );
    assert_eq!(
        resolve_query_environment_ids(&ids, &projects, None, Some(&project("p1")), false),
        ids
    );
}

#[test]
fn shared_repositories_are_listed_once() {
    let projects = [
        scope_project("a", "e1", "Aurora", Some("github.com/acme/aurora")),
        scope_project("b", "e2", "Aurora", Some("GITHUB.COM/acme/aurora")),
        scope_project("c", "e2", "Local", None),
        scope_project("d", "e3", "Elsewhere", Some("github.com/acme/other")),
    ];
    let assignment = assign_projects_to_environments(&projects, &[env("e1"), env("e2")], None);
    assert_eq!(assignment.get(&env("e1")), Some(&vec![project("a")]));
    assert_eq!(assignment.get(&env("e2")), Some(&vec![project("c")]));
    assert!(!assignment.contains_key(&env("e3")));
    let preferred =
        assign_projects_to_environments(&projects, &[env("e1"), env("e2")], Some(&env("e2")));
    assert_eq!(
        preferred.get(&env("e2")),
        Some(&vec![project("b"), project("c")])
    );
    assert!(!preferred.contains_key(&env("e1")));
}

#[test]
fn filter_projects_distinguish_duplicate_titles() {
    let projects = [
        scope_project("a", "e1", "Aurora", Some("github.com/acme/aurora")),
        scope_project("a2", "e1", "Aurora", Some("github.com/acme/aurora")),
        scope_project("b", "e2", "Aurora", Some("github.com/acme/aurora")),
        scope_project("c", "e1", "Borealis", None),
    ];
    let labels = BTreeMap::from([
        (env("e1"), "Laptop".to_owned()),
        (env("e2"), "Server".to_owned()),
    ]);
    let filtered = filter_projects(&projects, &labels, Some((&project("a2"), &env("e1"))));
    let titles: Vec<_> = filtered
        .iter()
        .map(|p| (p.id.0.as_str(), p.title.as_str()))
        .collect();
    assert_eq!(
        titles,
        [
            ("a2", "Aurora · Laptop"),
            ("b", "Aurora · Server"),
            ("c", "Borealis")
        ]
    );
}

#[test]
fn host_of_reads_the_canonical_key() {
    assert_eq!(
        host_of(
            Some("GitHub.com/acme/x"),
            None,
            &SourceControlProviderKind::Github
        ),
        "github.com"
    );
    assert_eq!(
        host_of(None, None, &SourceControlProviderKind::Gitlab),
        "gitlab"
    );
    assert_eq!(
        host_of(
            Some("codeberg.org/a/b"),
            Some("https://Code.Example.org:3000/a/b.git"),
            &SourceControlProviderKind::Forgejo
        ),
        "code.example.org:3000"
    );
}

// ---------------------------------------------------------------------------------------------
// Facets, stats, preferences

#[test]
fn facets_count_only_the_state_in_view() {
    let open = with(&entry(1, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.labels = vec![PullRequestLabel {
            name: "bug".into(),
            color: None,
        }]
    });
    let merged = with(&entry(2, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.state = PullRequestState::Merged;
        entry.entry.labels = vec![PullRequestLabel {
            name: "Bug".into(),
            color: None,
        }];
    });
    let facets = collect_facets(&[open.clone(), merged, open], &ListState::Open);
    assert_eq!(facets.authors.len(), 1);
    assert_eq!(facets.authors[0].count, 1);
    assert_eq!(facets.authors[0].merged_count, 1);
    assert_eq!(facets.labels.len(), 1);
    assert_eq!(facets.labels[0].count, 1);
}

#[test]
fn stats_batches_skip_measured_and_split_at_the_cap() {
    let mut by_key = BTreeMap::new();
    for number in 1..=501 {
        let row = entry(number, "2026-10-01T00:00:00Z");
        by_key.insert(entry_key(&row), row);
    }
    let measured = with(&entry(900, "2026-10-01T00:00:00Z"), |entry| {
        entry.entry.additions = 1
    });
    by_key.insert(entry_key(&measured), measured);
    let batches = stats_request_batches(
        &by_key,
        &Default::default(),
        StatsPolicy::Eager,
        &[],
        &Default::default(),
    );
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].refs.len() + batches[1].refs.len(), 501);
    let again = stats_request_batches(
        &by_key,
        &Default::default(),
        StatsPolicy::Eager,
        &batches,
        &Default::default(),
    );
    assert!(again.is_empty());
}

#[test]
fn preferences_round_trip_and_reject_garbage() {
    assert_eq!(
        ListPreferences::from_json("{nope"),
        ListPreferences::default()
    );
    let preferences = ListPreferences {
        state: ListState::Merged,
        sort: Some(ListSort::Ready),
        labels: (0..12).map(|index| format!("l{index}")).collect(),
        ..ListPreferences::default()
    }
    .normalized();
    assert_eq!(preferences.sort, None);
    assert_eq!(preferences.labels.len(), 10);
    let json = preferences.to_json();
    assert_eq!(ListPreferences::from_json(&json), preferences);
}

// ---------------------------------------------------------------------------------------------
// Presentation

fn check(status: CheckStatus, url: Option<&str>) -> PullRequestCheck {
    PullRequestCheck {
        name: "ci".to_owned(),
        status,
        description: None,
        url: url.map(str::to_owned),
    }
}

#[test]
fn state_presentation_prefers_terminal_states() {
    assert_eq!(
        StateKey::resolve(&PullRequestState::Merged, true),
        StateKey::Merged
    );
    assert_eq!(
        StateKey::resolve(&PullRequestState::Open, true),
        StateKey::Draft
    );
    assert_eq!(
        StateKey::resolve(&PullRequestState::Open, false).label(),
        "Open"
    );
    assert_eq!(
        conflict_label(
            &PullRequestState::Open,
            false,
            &Mergeability::Conflicting,
            Some("main")
        )
        .as_deref(),
        Some("Conflicts with main")
    );
    assert!(
        conflict_label(
            &PullRequestState::Open,
            true,
            &Mergeability::Conflicting,
            None
        )
        .is_none()
    );
}

#[test]
fn checks_rollup_and_summary() {
    assert_eq!(checks_state(&[]), None);
    assert_eq!(
        checks_state(&[
            check(CheckStatus::Success, None),
            check(CheckStatus::Cancelled, None)
        ]),
        Some(ChecksState::Failing)
    );
    assert_eq!(checks_state(&[check(CheckStatus::Skipped, None)]), None);
    assert_eq!(summarize_checks(&[]), "No checks reported");
    assert_eq!(
        summarize_checks(&[
            check(CheckStatus::Failure, None),
            check(CheckStatus::Success, None)
        ]),
        "1 of 2 failing"
    );
    let approval = check(
        CheckStatus::ActionRequired,
        Some("https://github.com/a/b/actions/runs/12"),
    );
    assert_eq!(
        summarize_checks(std::slice::from_ref(&approval)),
        "1 workflow awaiting approval"
    );
    assert_eq!(check_status_label(&approval), "Awaiting approval");
    assert_eq!(
        summarize_checks(&[
            check(CheckStatus::Success, None),
            check(CheckStatus::Skipped, None)
        ]),
        "1 of 2 passing"
    );
}

#[test]
fn provider_messages() {
    assert_eq!(
        unavailable_message(
            "cli-unauthenticated",
            Some(&SourceControlProviderKind::Github)
        ),
        "GitHub CLI is not authenticated. Run `gh auth login` and retry."
    );
    assert_eq!(
        unavailable_message("cli-missing", None),
        "The tool this host is read through is not installed or set up."
    );
    assert_eq!(
        unavailable_message(
            "provider-unsupported",
            Some(&SourceControlProviderKind::Github)
        ),
        "Change requests cannot be browsed for this project's host yet."
    );
    assert_eq!(
        provider_name(&SourceControlProviderKind::AzureDevops),
        "Azure DevOps"
    );
}

#[test]
fn label_colors_need_six_hex_digits() {
    assert_eq!(label_color(Some("#0e8a16")), Some(0x0e8a16));
    assert_eq!(label_color(Some(" D73A4A ")), Some(0xd73a4a));
    assert_eq!(label_color(Some("fff")), None);
    assert_eq!(label_color(Some("zzzzzz")), None);
    assert_eq!(label_color(None), None);
}

#[test]
fn counts_group_thousands() {
    assert_eq!(format_count(0), "0");
    assert_eq!(format_count(999), "999");
    assert_eq!(format_count(1_234_567), "1,234,567");
}
