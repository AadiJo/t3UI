//! The sidebar's derived models. Two layouts, picked by the client setting
//! `legacySidebarEnabled`:
//!
//! - [`build_inbox`] (default): one inbox across environments, split into Pinned, the inbox,
//!   the beta Working shelf, Snoozed, and Settled. Snooze, settle, and order-key rules are in
//!   `snooze.rs` and `order.rs` (client-runtime `threadSettled.ts` / `threadSort.ts`).
//! - [`build_sidebar`] (legacy): projects from every environment grouped into rows, ordered and
//!   sorted, each with its visible threads and status roll-ups (spec section 2.6-2.13).
//!
//! Both are pure. [`build_sidebar`] is a pure function of the shell data, settings, and UI state. The app
//! rebuilds it when any input changes and renders straight from the result, so rendering does no
//! sorting or status work.

mod inbox;
mod order;
mod pull_request;
mod selection;
mod snooze;
mod sort;
mod status;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use t3_protocol::{
    EnvironmentId, ProjectId,
    orchestration::{OrchestrationProjectShell, OrchestrationThreadShell},
};

pub use inbox::{
    InboxEnvironment, InboxInputs, InboxModel, InboxReturns, InboxThread, build_inbox,
};
pub use order::{
    MoveDirection, ThreadRow, generate_spread_pin_order_keys, pin_order_key_between,
    plan_pinned_move, plan_pinned_reorder, resolve_settled_thread_timestamp, sort_active_threads,
    sort_inbox_threads_by_return, sort_pinned_threads, sort_settled_threads,
};
pub use pull_request::{PullRequestBadge, change_request_short_name, pull_request_badge};
pub use selection::ThreadSelection;
pub use snooze::{
    CustomSnooze, QUEUED_TURN_START_GRACE_MS, SnoozePreset, SnoozePresetId, SnoozeUnit, can_snooze,
    effective_snoozed, has_queued_turn_start, local_snooze_date, local_snooze_time,
    raised_hand_while_snoozed, resolve_custom_snooze, resolve_snooze_presets, snooze_wake_label,
    thread_woke_at,
};
pub use sort::{compare_threads, locale_compare, order_by_preferred, thread_sort_timestamp};
pub use status::{
    RecedeInput, SidebarThreadStatus, ThreadStatus, format_working_duration_label,
    has_unseen_completion, highest_status, is_sidebar_thread_working,
    resolve_sidebar_thread_status, resolve_thread_status, resolve_working_started_at,
    should_recede_sidebar_thread, with_optimistic_work,
};

use crate::{
    paths::{normalize_for_comparison, repository_relative_path},
    refs::{ProjectRef, ThreadRef},
    settings::{ClientSettings, ProjectGroupingMode, ProjectSortOrder, ThreadSortOrder},
    time::parse_timestamp,
    ui_state::UiState,
};

/// Prefix of the web's pre-environment project preference keys.
const LEGACY_CWD_PREFIX: &str = "legacy-project-cwd:";

/// One environment's shell data, as the sidebar reads it.
#[derive(Clone, Copy)]
pub struct EnvironmentShell<'a> {
    pub id: &'a EnvironmentId,
    /// Display label ("HOME-PC", "staging"). Used for remote badges and member labels.
    pub label: Option<&'a str>,
    /// A desktop-local secondary backend (WSL). Shows a container badge instead of a cloud.
    pub desktop_local: bool,
    pub projects: &'a [Arc<OrchestrationProjectShell>],
    pub threads: &'a [Arc<OrchestrationThreadShell>],
}

/// Everything [`build_sidebar`] reads.
#[derive(Clone, Copy)]
pub struct SidebarInputs<'a> {
    /// Connected environments, primary first.
    pub environments: &'a [EnvironmentShell<'a>],
    pub primary_environment: Option<&'a EnvironmentId>,
    pub settings: &'a ClientSettings,
    pub ui: &'a UiState,
    /// The thread the route shows, if any.
    pub route_thread: Option<&'a ThreadRef>,
    /// Logical project keys whose list is expanded with "Show more" (in memory only).
    pub expanded_thread_lists: &'a HashSet<String>,
    /// Threads with an optimistic "work started" marker (a message was just sent).
    pub optimistic_working: &'a HashSet<ThreadRef>,
}

/// Where a project row's members live relative to the primary environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentPresence {
    LocalOnly,
    RemoteOnly,
    Mixed,
}

/// One physical project inside a sidebar row.
#[derive(Clone, Debug)]
pub struct ProjectMember {
    pub project_ref: ProjectRef,
    pub project: Arc<OrchestrationProjectShell>,
    /// `<environmentId>:<normalized workspace root>`: the key for order, expansion, and grouping
    /// overrides.
    pub physical_key: String,
    pub environment_label: Option<String>,
    /// Non-archived threads in this member (for "Remove" confirmations).
    pub thread_count: usize,
}

impl ProjectMember {
    /// Label used in member pickers (spec 2.9): the title when alone, else
    /// `"<env> — <root>"` or the root.
    pub fn picker_label(&self, member_count: usize) -> String {
        if member_count <= 1 {
            return self.project.title.clone();
        }
        match &self.environment_label {
            Some(label) => format!("{label} \u{2014} {}", self.project.workspace_root),
            None => self.project.workspace_root.clone(),
        }
    }
}

/// A thread row.
#[derive(Clone, Debug)]
pub struct SidebarThread {
    pub thread_ref: ThreadRef,
    pub thread: Arc<OrchestrationThreadShell>,
    pub status: Option<ThreadStatus>,
    /// The route shows this thread.
    pub active: bool,
    /// Workspace root of the thread's own project (grouped rows can mix environments).
    pub project_root: Option<String>,
}

/// A project row with its thread list.
#[derive(Clone, Debug)]
pub struct SidebarProject {
    /// Logical key (grouping result). Stable identity of the row.
    pub key: String,
    pub display_name: String,
    /// The member in the primary environment, else the first member.
    pub representative: ProjectMember,
    pub members: Vec<ProjectMember>,
    pub presence: EnvironmentPresence,
    pub all_remote_members_desktop_local: bool,
    pub remote_environment_labels: Vec<String>,
    /// Keys whose stored value decides expansion; toggling writes all of them.
    pub expansion_keys: Vec<String>,
    pub expanded: bool,
    /// Highest-priority status over all threads; shown on the header while collapsed.
    pub status: Option<ThreadStatus>,
    /// "Show more" is active for this project.
    pub thread_list_expanded: bool,
    /// Every non-archived thread in sort order (range selection works over these).
    pub ordered_threads: Vec<ThreadRef>,
    /// Rows to render.
    pub rendered_threads: Vec<SidebarThread>,
    /// More threads than the preview count.
    pub has_overflow: bool,
    /// Highest status among threads hidden behind "Show more".
    pub hidden_status: Option<ThreadStatus>,
    /// Expanded with no threads: show "No threads yet".
    pub show_empty: bool,
    /// The thread list renders at all (expanded, or collapsed with the route thread pinned).
    pub show_thread_panel: bool,
}

impl SidebarProject {
    /// Total non-archived threads in the row.
    pub fn thread_count(&self) -> usize {
        self.ordered_threads.len()
    }
}

/// The derived sidebar.
#[derive(Clone, Debug, Default)]
pub struct SidebarModel {
    pub projects: Vec<SidebarProject>,
    /// Rendered thread rows top to bottom: `thread.previous/next` and `thread.jump.N` order.
    pub visible_threads: Vec<ThreadRef>,
    /// Physical keys of all projects in their preferred order (input to manual reordering).
    pub project_order_keys: Vec<String>,
}

impl SidebarModel {
    /// The row that contains `thread`.
    pub fn project_of(&self, thread: &ThreadRef) -> Option<&SidebarProject> {
        self.projects
            .iter()
            .find(|project| project.ordered_threads.contains(thread))
    }

    /// The thread row for `thread`, if rendered.
    pub fn rendered_thread(&self, thread: &ThreadRef) -> Option<&SidebarThread> {
        self.projects
            .iter()
            .flat_map(|project| &project.rendered_threads)
            .find(|row| &row.thread_ref == thread)
    }

    /// The thread `thread.previous` / `thread.next` moves to (web `resolveAdjacentThreadId`). With
    /// no current thread, previous picks the last and next the first; it stops at the ends, and a
    /// current thread outside the visible list goes nowhere.
    pub fn adjacent_thread(
        &self,
        current: Option<&ThreadRef>,
        forward: bool,
    ) -> Option<&ThreadRef> {
        let threads = &self.visible_threads;
        let Some(current) = current else {
            return if forward {
                threads.first()
            } else {
                threads.last()
            };
        };
        let index = threads.iter().position(|thread| thread == current)?;
        if forward {
            threads.get(index + 1)
        } else {
            index
                .checked_sub(1)
                .and_then(|previous| threads.get(previous))
        }
    }

    /// Target of `thread.jump.N` (1-based).
    pub fn jump_target(&self, index: u8) -> Option<&ThreadRef> {
        let index = usize::from(index).checked_sub(1)?;
        (index < 9)
            .then(|| self.visible_threads.get(index))
            .flatten()
    }
}

/// Physical project key: `<environmentId>:<normalized path>`.
pub fn physical_project_key(environment_id: &EnvironmentId, workspace_root: &str) -> String {
    format!(
        "{environment_id}:{}",
        normalize_for_comparison(workspace_root)
    )
}

/// The web's legacy per-path preference key.
pub fn legacy_cwd_key(workspace_root: &str) -> String {
    format!(
        "{LEGACY_CWD_PREFIX}{}",
        normalize_for_comparison(workspace_root)
    )
}

/// Logical project key under `mode` (`projectGrouping.ts` `deriveLogicalProjectKey`).
pub fn logical_project_key(
    environment_id: &EnvironmentId,
    project: &OrchestrationProjectShell,
    mode: ProjectGroupingMode,
) -> String {
    let physical = || physical_project_key(environment_id, &project.workspace_root);
    if mode == ProjectGroupingMode::Separate {
        return physical();
    }
    let Some(identity) = project
        .repository_identity
        .as_ref()
        .filter(|identity| !identity.canonical_key.is_empty())
    else {
        return physical();
    };
    if mode == ProjectGroupingMode::Repository {
        return identity.canonical_key.clone();
    }
    match repository_relative_path(&project.workspace_root, identity.root_path.as_deref()) {
        Some(relative) if !relative.is_empty() => format!("{}::{relative}", identity.canonical_key),
        _ => identity.canonical_key.clone(),
    }
}

/// Row label for a group of several members: a shared repository display name, else a shared
/// repository name, else the representative's title.
fn group_label(representative: &ProjectMember, members: &[ProjectMember]) -> String {
    let shared = |pick: fn(&OrchestrationProjectShell) -> Option<&str>| {
        let mut unique: Vec<&str> = Vec::new();
        for value in members.iter().filter_map(|member| pick(&member.project)) {
            let value = value.trim();
            if !value.is_empty() && !unique.contains(&value) {
                unique.push(value);
            }
        }
        (unique.len() == 1).then(|| unique[0].to_owned())
    };
    shared(|project| {
        project
            .repository_identity
            .as_ref()?
            .display_name
            .as_deref()
    })
    .or_else(|| shared(|project| project.repository_identity.as_ref()?.name.as_deref()))
    .unwrap_or_else(|| representative.project.title.clone())
}

struct ProjectEntry<'a> {
    environment: &'a EnvironmentShell<'a>,
    project: &'a Arc<OrchestrationProjectShell>,
    physical_key: String,
}

/// Builds the sidebar model.
pub fn build_sidebar(inputs: &SidebarInputs<'_>) -> SidebarModel {
    let settings = inputs.settings;

    // 1. Every project from every environment, in preferred (manual) order.
    let entries: Vec<ProjectEntry<'_>> = inputs
        .environments
        .iter()
        .flat_map(|environment| {
            environment
                .projects
                .iter()
                .map(move |project| ProjectEntry {
                    environment,
                    project,
                    physical_key: physical_project_key(environment.id, &project.workspace_root),
                })
        })
        .collect();
    let entries = order_by_preferred(entries, &inputs.ui.project_order, |entry| {
        vec![
            entry.physical_key.clone(),
            legacy_cwd_key(&entry.project.workspace_root),
        ]
    });
    let project_order_keys: Vec<String> = entries
        .iter()
        .map(|entry| entry.physical_key.clone())
        .collect();

    // 2. Physical and logical keys per project ref.
    let mut physical_by_ref: HashMap<(&EnvironmentId, &ProjectId), &str> = HashMap::new();
    let mut logical_by_physical: HashMap<&str, String> = HashMap::new();
    for entry in &entries {
        physical_by_ref.insert(
            (entry.environment.id, &entry.project.id),
            &entry.physical_key,
        );
        let mode = settings.grouping_mode_for(&entry.physical_key);
        logical_by_physical.insert(
            &entry.physical_key,
            logical_project_key(entry.environment.id, entry.project, mode),
        );
    }
    let logical_key_of = |environment_id: &EnvironmentId, project_id: &ProjectId| -> String {
        let physical = physical_by_ref
            .get(&(environment_id, project_id))
            .map(|key| (*key).to_owned())
            .unwrap_or_else(|| format!("{environment_id}:{project_id}"));
        logical_by_physical
            .get(physical.as_str())
            .cloned()
            .unwrap_or(physical)
    };

    // 3. Non-archived threads by logical key, plus per-member counts and roots.
    let thread_order = settings.sidebar_thread_sort_order;
    let mut threads_by_logical: HashMap<
        String,
        Vec<(&EnvironmentShell<'_>, &Arc<OrchestrationThreadShell>)>,
    > = HashMap::new();
    let mut count_by_physical: HashMap<String, usize> = HashMap::new();
    let mut root_by_ref: HashMap<(&EnvironmentId, &ProjectId), &str> = HashMap::new();
    for entry in &entries {
        root_by_ref.insert(
            (entry.environment.id, &entry.project.id),
            &entry.project.workspace_root,
        );
    }
    for environment in inputs.environments {
        for thread in environment
            .threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
        {
            threads_by_logical
                .entry(logical_key_of(environment.id, &thread.project_id))
                .or_default()
                .push((environment, thread));
            if let Some(physical) = physical_by_ref.get(&(environment.id, &thread.project_id)) {
                *count_by_physical.entry((*physical).to_owned()).or_default() += 1;
            }
        }
    }
    for threads in threads_by_logical.values_mut() {
        threads.sort_by(|(_, left), (_, right)| compare_threads(left, right, thread_order));
    }

    // 4. One row per logical key, in first-seen order.
    let mut groups: Vec<(String, Vec<ProjectMember>)> = Vec::new();
    for entry in &entries {
        let key = logical_by_physical[entry.physical_key.as_str()].clone();
        let member = ProjectMember {
            project_ref: ProjectRef::new(entry.environment.id.clone(), entry.project.id.clone()),
            project: entry.project.clone(),
            physical_key: entry.physical_key.clone(),
            environment_label: entry.environment.label.map(str::to_owned),
            thread_count: count_by_physical
                .get(&entry.physical_key)
                .copied()
                .unwrap_or(0),
        };
        match groups.iter_mut().find(|(group_key, _)| *group_key == key) {
            Some((_, members)) => members.push(member),
            None => groups.push((key, vec![member])),
        }
    }
    let desktop_local: HashSet<&EnvironmentId> = inputs
        .environments
        .iter()
        .filter(|environment| environment.desktop_local)
        .map(|environment| environment.id)
        .collect();

    let preview_count = settings.sidebar_thread_preview_count.get();
    let route_thread = inputs.route_thread;
    let mut projects: Vec<SidebarProject> = groups
        .into_iter()
        .map(|(key, members)| {
            let primary = inputs.primary_environment;
            let representative = primary
                .and_then(|primary| {
                    members
                        .iter()
                        .find(|member| &member.project_ref.environment_id == primary)
                })
                .unwrap_or(&members[0])
                .clone();
            let (has_local, remote_members): (bool, Vec<&ProjectMember>) = match primary {
                Some(primary) => (
                    members
                        .iter()
                        .any(|member| &member.project_ref.environment_id == primary),
                    members
                        .iter()
                        .filter(|member| &member.project_ref.environment_id != primary)
                        .collect(),
                ),
                None => (false, Vec::new()),
            };
            let presence = match (has_local, !remote_members.is_empty()) {
                (true, true) => EnvironmentPresence::Mixed,
                (false, true) => EnvironmentPresence::RemoteOnly,
                _ => EnvironmentPresence::LocalOnly,
            };
            let mut remote_environment_labels: Vec<String> = Vec::new();
            for label in remote_members
                .iter()
                .filter_map(|member| member.environment_label.as_ref())
            {
                if !remote_environment_labels.contains(label) {
                    remote_environment_labels.push(label.clone());
                }
            }
            let all_remote_members_desktop_local = !remote_members.is_empty()
                && remote_members
                    .iter()
                    .all(|member| desktop_local.contains(&member.project_ref.environment_id));
            let display_name = if members.len() > 1 {
                group_label(&representative, &members)
            } else {
                representative.project.title.clone()
            };
            let expansion_keys: Vec<String> = std::iter::once(key.clone())
                .chain(members.iter().map(|member| member.physical_key.clone()))
                .chain(
                    members
                        .iter()
                        .map(|member| legacy_cwd_key(&member.project.workspace_root)),
                )
                .collect();
            let expanded = inputs.ui.project_expanded(&expansion_keys);

            let threads = threads_by_logical
                .get(&key)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let rows: Vec<SidebarThread> = threads
                .iter()
                .map(|(environment, thread)| {
                    let thread_ref = ThreadRef::new(environment.id.clone(), thread.id.clone());
                    let status = with_optimistic_work(
                        resolve_thread_status(thread, inputs.ui.last_visited_at(&thread_ref.key())),
                        inputs.optimistic_working.contains(&thread_ref),
                    );
                    SidebarThread {
                        active: route_thread == Some(&thread_ref),
                        project_root: root_by_ref
                            .get(&(environment.id, &thread.project_id))
                            .map(|root| (*root).to_owned()),
                        thread_ref,
                        thread: (*thread).clone(),
                        status,
                    }
                })
                .collect();
            let status = highest_status(rows.iter().map(|row| row.status));
            let ordered_threads: Vec<ThreadRef> =
                rows.iter().map(|row| row.thread_ref.clone()).collect();
            let thread_list_expanded = inputs.expanded_thread_lists.contains(&key);
            let has_overflow = rows.len() > preview_count;
            let pinned = (!expanded)
                .then(|| rows.iter().position(|row| row.active))
                .flatten();
            let visible_count = if thread_list_expanded || !has_overflow {
                rows.len()
            } else {
                preview_count
            };
            let hidden_status =
                highest_status(rows.iter().enumerate().filter_map(|(index, row)| {
                    let visible = index < visible_count || Some(index) == pinned;
                    (!visible).then_some(row.status)
                }));
            let rendered_threads = match pinned {
                Some(index) => vec![rows[index].clone()],
                None if expanded => rows[..visible_count].to_vec(),
                None => Vec::new(),
            };
            SidebarProject {
                display_name,
                presence,
                all_remote_members_desktop_local,
                remote_environment_labels,
                expanded,
                status,
                thread_list_expanded,
                has_overflow,
                hidden_status,
                show_empty: expanded && rows.is_empty(),
                show_thread_panel: expanded || pinned.is_some(),
                ordered_threads,
                rendered_threads,
                expansion_keys,
                representative,
                members,
                key,
            }
        })
        .collect();

    // 5. Project sort.
    if settings.sidebar_project_sort_order != ProjectSortOrder::Manual {
        let order = match settings.sidebar_project_sort_order {
            ProjectSortOrder::CreatedAt => ThreadSortOrder::CreatedAt,
            _ => ThreadSortOrder::UpdatedAt,
        };
        let timestamp = |project: &SidebarProject| -> Option<i64> {
            let threads = threads_by_logical.get(&project.key);
            match threads.filter(|threads| !threads.is_empty()) {
                Some(threads) => threads
                    .iter()
                    .map(|(_, thread)| thread_sort_timestamp(thread, order))
                    .max()
                    .flatten(),
                None => {
                    let shell = &project.representative.project;
                    match order {
                        ThreadSortOrder::CreatedAt => parse_timestamp(&shell.created_at),
                        ThreadSortOrder::UpdatedAt => parse_timestamp(&shell.updated_at),
                    }
                }
            }
        };
        let mut keyed: Vec<(Option<i64>, SidebarProject)> = projects
            .into_iter()
            .map(|project| (timestamp(&project), project))
            .collect();
        keyed.sort_by(|(left_time, left), (right_time, right)| {
            right_time
                .cmp(left_time)
                .then_with(|| {
                    locale_compare(
                        &left.representative.project.title,
                        &right.representative.project.title,
                    )
                })
                .then_with(|| locale_compare(&left.key, &right.key))
        });
        projects = keyed.into_iter().map(|(_, project)| project).collect();
    }

    let visible_threads = projects
        .iter()
        .flat_map(|project| {
            project
                .rendered_threads
                .iter()
                .map(|row| row.thread_ref.clone())
        })
        .collect();
    SidebarModel {
        projects,
        visible_threads,
        project_order_keys,
    }
}

#[cfg(test)]
mod inbox_tests;
#[cfg(test)]
mod tests;
