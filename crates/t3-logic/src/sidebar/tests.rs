//! Failure modes this covers, written before the tests:
//!
//! 1. Grouping: projects sharing a repository must share a row in `repository` mode, split in
//!    `separate` mode, split by repo-relative path in `repository_path` mode, and a per-project
//!    override must beat the global mode.
//! 2. A grouped row must use the primary environment's member as representative and label itself
//!    from the shared repository display name.
//! 3. Presence: only non-primary members is "remote-only" with deduplicated labels; all of them
//!    desktop-local switches the badge to the sandbox variant.
//! 4. Thread order: `latestUserMessageAt` beats `updatedAt`, ties break by id descending, and
//!    archived threads never appear.
//! 5. Project order: newest thread first, empty projects fall back to their own timestamps, ties
//!    by title, and manual mode keeps `projectOrder`.
//! 6. Preview: only the first N threads render until "Show more"; the hidden-status dot reflects
//!    only hidden threads.
//! 7. Collapsed rows render nothing except a pinned route thread, and still roll up status.
//! 8. Status priority and unread (fork pill rules): approval > input > working / connecting >
//!    plan ready > completed, with no Error pill; a completion is unread only after a recorded
//!    visit older than it, and a visit 1ms before completion (mark unread) stays unread.
//!    Roll-ups keep the first thread on priority ties.
//! 9. Keyboard order: previous/next from nothing picks the ends, stops at the ends, and does
//!    nothing for a route thread outside the visible list; jumps cover only the first 9.

use std::{collections::HashSet, sync::Arc};

use serde_json::{Value, json};
use t3_protocol::{
    EnvironmentId,
    orchestration::{OrchestrationProjectShell, OrchestrationThreadShell},
};

use super::*;
use crate::{
    refs::ThreadRef,
    settings::{ClientSettings, ProjectGroupingMode, ProjectSortOrder, ThreadPreviewCount},
    ui_state::UiState,
};

fn project(
    id: &str,
    root: &str,
    repo: Option<(&str, &str)>,
    updated_at: &str,
) -> Arc<OrchestrationProjectShell> {
    let identity = repo.map(|(canonical, repo_root)| {
        json!({
            "canonicalKey": canonical,
            "locator": {"source": "git"},
            "rootPath": repo_root,
            "displayName": "acme/app",
            "name": "app",
        })
    });
    Arc::new(
        serde_json::from_value(json!({
            "id": id,
            "title": id.to_uppercase(),
            "workspaceRoot": root,
            "repositoryIdentity": identity,
            "defaultModelSelection": null,
            "scripts": [],
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": updated_at,
        }))
        .unwrap(),
    )
}

pub(super) fn thread(
    id: &str,
    project_id: &str,
    updated_at: &str,
    extra: Value,
) -> Arc<OrchestrationThreadShell> {
    let mut value = json!({
        "id": id,
        "projectId": project_id,
        "title": format!("Thread {id}"),
        "modelSelection": {"instanceId": "codex", "model": "gpt-5"},
        "runtimeMode": "full-access",
        "interactionMode": "default",
        "branch": null,
        "worktreePath": null,
        "pullRequests": null,
        "latestTurn": null,
        "createdAt": "2026-01-01T00:00:00.000Z",
        "updatedAt": updated_at,
        "archivedAt": null,
        "settledOverride": null,
        "settledAt": null,
        "session": null,
        "latestUserMessageAt": null,
        "hasPendingApprovals": false,
        "hasPendingUserInput": false,
        "hasActionableProposedPlan": false,
    });
    for (key, extra_value) in extra.as_object().cloned().unwrap_or_default() {
        value[key] = extra_value;
    }
    Arc::new(serde_json::from_value(value).unwrap())
}

fn completed_turn(completed_at: &str) -> Value {
    json!({
        "turnId": "turn",
        "state": "completed",
        "requestedAt": "2026-01-01T00:00:00.000Z",
        "startedAt": "2026-01-01T00:00:00.000Z",
        "completedAt": completed_at,
    })
}

fn session(status: &str) -> Value {
    json!({"threadId": "t", "status": status, "activeTurnId": null, "updatedAt": "2026-01-01T00:00:00.000Z"})
}

/// `(id, label, desktop_local, projects, threads)`.
type EnvFixture = (
    EnvironmentId,
    Option<&'static str>,
    bool,
    Vec<Arc<OrchestrationProjectShell>>,
    Vec<Arc<OrchestrationThreadShell>>,
);

struct Fixture {
    environments: Vec<EnvFixture>,
    settings: ClientSettings,
    ui: UiState,
    route: Option<ThreadRef>,
    expanded_lists: HashSet<String>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            environments: Vec::new(),
            settings: ClientSettings::default(),
            ui: UiState::default(),
            route: None,
            expanded_lists: HashSet::new(),
        }
    }

    fn env(
        mut self,
        id: &str,
        label: Option<&'static str>,
        projects: Vec<Arc<OrchestrationProjectShell>>,
        threads: Vec<Arc<OrchestrationThreadShell>>,
    ) -> Self {
        self.environments
            .push((EnvironmentId::from(id), label, false, projects, threads));
        self
    }

    fn build(&self) -> SidebarModel {
        let shells: Vec<EnvironmentShell<'_>> = self
            .environments
            .iter()
            .map(
                |(id, label, desktop_local, projects, threads)| EnvironmentShell {
                    id,
                    label: *label,
                    desktop_local: *desktop_local,
                    projects,
                    threads,
                },
            )
            .collect();
        build_sidebar(&SidebarInputs {
            environments: &shells,
            primary_environment: self.environments.first().map(|(id, ..)| id),
            settings: &self.settings,
            ui: &self.ui,
            route_thread: self.route.as_ref(),
            expanded_thread_lists: &self.expanded_lists,
            optimistic_working: &HashSet::new(),
        })
    }
}

fn names(model: &SidebarModel) -> Vec<&str> {
    model
        .projects
        .iter()
        .map(|project| project.display_name.as_str())
        .collect()
}

fn thread_ids(project: &SidebarProject) -> Vec<&str> {
    project
        .rendered_threads
        .iter()
        .map(|row| row.thread.id.as_str())
        .collect()
}

fn tref(env: &str, id: &str) -> ThreadRef {
    ThreadRef::new(EnvironmentId::from(env), id.into())
}

const T0: &str = "2026-01-01T00:00:00.000Z";

#[test]
fn repositories_group_across_environments() {
    let repo = Some(("github.com/acme/app", "/w/app"));
    let fixture = Fixture::new()
        .env(
            "local",
            Some("HOME-PC"),
            vec![project("a", "/w/app", repo, T0)],
            vec![],
        )
        .env(
            "remote",
            Some("devbox"),
            vec![project(
                "b",
                "/srv/app",
                Some(("github.com/acme/app", "/srv/app")),
                T0,
            )],
            vec![],
        );
    let model = fixture.build();
    assert_eq!(model.projects.len(), 1);
    let row = &model.projects[0];
    assert_eq!(row.display_name, "acme/app");
    assert_eq!(row.members.len(), 2);
    assert_eq!(row.representative.project.id.as_str(), "a");
    assert_eq!(row.presence, EnvironmentPresence::Mixed);
    assert_eq!(row.key, "github.com/acme/app");
    assert_eq!(row.members[1].picker_label(2), "devbox \u{2014} /srv/app");

    let mut separate = fixture;
    separate.settings.sidebar_project_grouping_mode = ProjectGroupingMode::Separate;
    assert_eq!(names(&separate.build()), vec!["A", "B"]);
    separate.settings.sidebar_project_grouping_mode = ProjectGroupingMode::Repository;
    separate
        .settings
        .sidebar_project_grouping_overrides
        .insert("remote:/srv/app".into(), ProjectGroupingMode::Separate);
    assert_eq!(names(&separate.build()).len(), 2);
}

#[test]
fn repository_path_mode_splits_subdirectories() {
    let repo = Some(("github.com/acme/mono", "/w/mono"));
    let mut fixture = Fixture::new().env(
        "local",
        None,
        vec![
            project("web", "/w/mono/web", repo, T0),
            project("api", "/w/mono/api", repo, T0),
        ],
        vec![],
    );
    assert_eq!(fixture.build().projects.len(), 1);
    fixture.settings.sidebar_project_grouping_mode = ProjectGroupingMode::RepositoryPath;
    let model = fixture.build();
    let keys: Vec<&str> = model
        .projects
        .iter()
        .map(|project| project.key.as_str())
        .collect();
    // Equal timestamps, so rows fall back to title order.
    assert_eq!(
        keys,
        vec!["github.com/acme/mono::api", "github.com/acme/mono::web"]
    );
}

#[test]
fn remote_only_rows_badge_their_environments() {
    let mut fixture = Fixture::new()
        .env("local", Some("HOME-PC"), vec![], vec![])
        .env(
            "wsl",
            Some("Ubuntu"),
            vec![project("x", "/home/x", None, T0)],
            vec![],
        );
    fixture.environments[1].2 = true;
    let row = &fixture.build().projects[0];
    assert_eq!(row.presence, EnvironmentPresence::RemoteOnly);
    assert_eq!(row.remote_environment_labels, vec!["Ubuntu".to_owned()]);
    assert!(row.all_remote_members_desktop_local);
}

#[test]
fn threads_sort_newest_user_message_first() {
    let model = Fixture::new()
        .env(
            "e",
            None,
            vec![project("p", "/p", None, T0)],
            vec![
                thread("a", "p", "2026-03-01T00:00:00.000Z", json!({})),
                thread(
                    "b",
                    "p",
                    "2026-01-01T00:00:00.000Z",
                    json!({"latestUserMessageAt": "2026-04-01T00:00:00.000Z"}),
                ),
                thread("c", "p", "2026-03-01T00:00:00.000Z", json!({})),
                thread(
                    "z",
                    "p",
                    "2026-05-01T00:00:00.000Z",
                    json!({"archivedAt": "2026-05-02T00:00:00.000Z"}),
                ),
            ],
        )
        .build();
    assert_eq!(thread_ids(&model.projects[0]), vec!["b", "c", "a"]);
    assert_eq!(model.projects[0].thread_count(), 3);
}

#[test]
fn projects_sort_by_newest_thread_then_title() {
    let mut fixture = Fixture::new().env(
        "e",
        None,
        vec![
            project("old", "/old", None, "2026-02-01T00:00:00.000Z"),
            project("busy", "/busy", None, T0),
            project("empty", "/empty", None, "2026-06-01T00:00:00.000Z"),
            project("tie", "/tie", None, "2026-02-01T00:00:00.000Z"),
        ],
        vec![thread("t", "busy", "2026-05-01T00:00:00.000Z", json!({}))],
    );
    assert_eq!(names(&fixture.build()), vec!["EMPTY", "BUSY", "OLD", "TIE"]);

    fixture.settings.sidebar_project_sort_order = ProjectSortOrder::Manual;
    fixture.ui.project_order = vec!["e:/tie".into(), "e:/busy".into()];
    assert_eq!(names(&fixture.build()), vec!["TIE", "BUSY", "OLD", "EMPTY"]);
}

#[test]
fn previews_hide_overflow_until_show_more() {
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let extra = if index == 7 {
                json!({"hasPendingUserInput": true})
            } else {
                json!({})
            };
            thread(
                &format!("t{index}"),
                "p",
                &format!("2026-01-0{}T00:00:00.000Z", 9 - index),
                extra,
            )
        })
        .collect();
    let mut fixture = Fixture::new().env("e", None, vec![project("p", "/p", None, T0)], threads);
    fixture.settings.sidebar_thread_preview_count = ThreadPreviewCount::new(3).unwrap();
    let model = fixture.build();
    let row = &model.projects[0];
    assert_eq!(thread_ids(row), vec!["t0", "t1", "t2"]);
    assert!(row.has_overflow);
    assert_eq!(row.hidden_status, Some(ThreadStatus::AwaitingInput));
    assert_eq!(row.status, Some(ThreadStatus::AwaitingInput));
    assert_eq!(model.visible_threads.len(), 3);

    fixture.expanded_lists.insert(row.key.clone());
    let model = fixture.build();
    assert_eq!(model.projects[0].rendered_threads.len(), 8);
    assert_eq!(model.projects[0].hidden_status, None);
}

#[test]
fn collapsed_rows_pin_only_the_route_thread() {
    let mut fixture = Fixture::new().env(
        "e",
        None,
        vec![project("p", "/p", None, T0), project("q", "/q", None, T0)],
        vec![
            thread(
                "a",
                "p",
                "2026-01-03T00:00:00.000Z",
                json!({"session": session("running")}),
            ),
            thread("b", "p", "2026-01-02T00:00:00.000Z", json!({})),
            thread("c", "q", "2026-01-02T00:00:00.000Z", json!({})),
        ],
    );
    fixture
        .ui
        .project_expanded_by_id
        .insert("e:/p".into(), false);
    let model = fixture.build();
    let p = model
        .projects
        .iter()
        .find(|project| project.key == "e:/p")
        .unwrap();
    assert!(!p.expanded && !p.show_thread_panel && p.rendered_threads.is_empty());
    assert_eq!(p.status, Some(ThreadStatus::Working));
    assert_eq!(model.visible_threads, vec![tref("e", "c")]);

    fixture.route = Some(tref("e", "b"));
    let model = fixture.build();
    let p = model
        .projects
        .iter()
        .find(|project| project.key == "e:/p")
        .unwrap();
    assert!(p.show_thread_panel && !p.show_empty);
    assert_eq!(thread_ids(p), vec!["b"]);
    assert!(p.rendered_threads[0].active);
    assert_eq!(model.visible_threads, vec![tref("e", "b"), tref("e", "c")]);
}

#[test]
fn status_priority_and_unread() {
    let visited = |completed: &str, visited: Option<&str>| {
        resolve_thread_status(
            &thread(
                "t",
                "p",
                T0,
                json!({"latestTurn": completed_turn(completed)}),
            ),
            visited,
        )
    };
    // Never visited: nothing unseen (fork `hasUnseenCompletion`).
    assert_eq!(visited("2026-01-02T00:00:00.000Z", None), None);
    assert_eq!(
        visited("2026-01-02T00:00:00.000Z", Some("2026-01-01T00:00:00.000Z")),
        Some(ThreadStatus::Completed)
    );
    assert_eq!(
        visited("2026-01-02T00:00:00.000Z", Some("2026-01-02T00:00:00.000Z")),
        None
    );
    assert_eq!(
        visited("2026-01-02T00:00:00.000Z", Some("2026-01-01T23:59:59.999Z")),
        Some(ThreadStatus::Completed)
    );

    let status = |extra: Value| resolve_thread_status(&thread("t", "p", T0, extra), None);
    assert_eq!(
        status(
            json!({"hasPendingApprovals": true, "hasPendingUserInput": true, "session": session("error")})
        ),
        Some(ThreadStatus::PendingApproval)
    );
    assert_eq!(
        status(json!({"hasPendingUserInput": true, "session": session("error")})),
        Some(ThreadStatus::AwaitingInput)
    );
    // No Error pill in the fork: a failure shows as the row status, not a pill.
    assert_eq!(status(json!({"session": session("error")})), None);
    assert_eq!(
        status(json!({"session": session("starting")})),
        Some(ThreadStatus::Connecting)
    );
    assert_eq!(
        status(json!({
            "interactionMode": "plan",
            "hasActionableProposedPlan": true,
            "latestTurn": completed_turn("2026-01-02T00:00:00.000Z"),
            "session": session("ready"),
        })),
        Some(ThreadStatus::PlanReady)
    );
    assert_eq!(
        status(json!({
            "interactionMode": "plan",
            "hasActionableProposedPlan": true,
            "latestTurn": completed_turn("2026-01-02T00:00:00.000Z"),
            "session": session("running"),
        })),
        Some(ThreadStatus::Working)
    );
    assert_eq!(status(json!({"session": session("ready")})), None);

    assert_eq!(
        highest_status([
            Some(ThreadStatus::Working),
            Some(ThreadStatus::PendingApproval)
        ]),
        Some(ThreadStatus::PendingApproval)
    );
    assert_eq!(
        highest_status([
            None,
            Some(ThreadStatus::Completed),
            Some(ThreadStatus::Working)
        ]),
        Some(ThreadStatus::Working)
    );
    assert_eq!(
        with_optimistic_work(Some(ThreadStatus::Completed), true),
        Some(ThreadStatus::Working)
    );
    assert_eq!(
        with_optimistic_work(Some(ThreadStatus::AwaitingInput), true),
        Some(ThreadStatus::AwaitingInput)
    );
}

#[test]
fn keyboard_traversal_and_jumps() {
    let threads: Vec<_> = (0..11)
        .map(|index| {
            thread(
                &format!("t{index:02}"),
                "p",
                &format!("2026-01-{:02}T00:00:00.000Z", 20 - index),
                json!({}),
            )
        })
        .collect();
    let mut fixture = Fixture::new().env("e", None, vec![project("p", "/p", None, T0)], threads);
    fixture.settings.sidebar_thread_preview_count = ThreadPreviewCount::new(10).unwrap();
    let model = fixture.build();
    assert_eq!(model.visible_threads.len(), 10);
    assert_eq!(model.adjacent_thread(None, true), Some(&tref("e", "t00")));
    assert_eq!(model.adjacent_thread(None, false), Some(&tref("e", "t09")));
    assert_eq!(model.adjacent_thread(Some(&tref("e", "t00")), false), None);
    assert_eq!(
        model.adjacent_thread(Some(&tref("e", "t00")), true),
        Some(&tref("e", "t01"))
    );
    assert_eq!(model.adjacent_thread(Some(&tref("e", "t09")), true), None);
    assert_eq!(model.adjacent_thread(Some(&tref("e", "t10")), true), None);
    assert_eq!(model.jump_target(1), Some(&tref("e", "t00")));
    assert_eq!(model.jump_target(9), Some(&tref("e", "t08")));
    assert_eq!(model.jump_target(10), None);
    assert_eq!(model.jump_target(0), None);
}
