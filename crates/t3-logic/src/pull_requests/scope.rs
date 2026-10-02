//! Which servers and projects a listing asks about (`pullRequestList.logic.ts:876-960`,
//! `pullRequestProjectAssignment.logic.ts`, `pullRequestProjectFilter.logic.ts`,
//! `pullRequestHostOf` in `packages/contracts/src/pullRequest.ts:1296`).

use std::collections::{BTreeMap, HashMap};

use t3_protocol::{
    EnvironmentId, ProjectId, orchestration::OrchestrationProjectShell,
    pull_requests::SourceControlProviderKind,
};

/// The little of a project the page needs: who holds it, what it is called, and which
/// repository it is a copy of.
#[derive(Clone, Debug, PartialEq)]
pub struct ScopeProject {
    pub id: ProjectId,
    pub environment_id: EnvironmentId,
    pub title: String,
    pub workspace_root: String,
    /// `repositoryIdentity.canonicalKey` (`host/owner/repo`), when the project has a remote.
    pub canonical_key: Option<String>,
}

impl ScopeProject {
    pub fn from_shell(environment_id: &EnvironmentId, project: &OrchestrationProjectShell) -> Self {
        Self {
            id: project.id.clone(),
            environment_id: environment_id.clone(),
            title: project.title.clone(),
            workspace_root: project.workspace_root.clone(),
            canonical_key: project
                .repository_identity
                .as_ref()
                .map(|identity| identity.canonical_key.clone()),
        }
    }

    /// Lowercased canonical key: "same repository" across machines.
    fn repository_key(&self) -> Option<String> {
        self.canonical_key
            .as_deref()
            .map(str::to_lowercase)
            .filter(|key| !key.is_empty())
    }
}

/// The project id to actually ask for: an id no known project has is dropped, but only once
/// every environment has said what it holds (`resolveProjectScope`).
pub fn resolve_project_scope(
    project_id: Option<&ProjectId>,
    projects: &[ScopeProject],
    projects_known: bool,
) -> Option<ProjectId> {
    let project_id = project_id?;
    if !projects_known || projects.iter().any(|project| &project.id == project_id) {
        Some(project_id.clone())
    } else {
        None
    }
}

/// The project an id names: exact with a server, otherwise only where one server has that id.
pub fn find_scoped_project<'a>(
    projects: &'a [ScopeProject],
    environment_id: Option<&EnvironmentId>,
    project_id: Option<&ProjectId>,
) -> Option<&'a ScopeProject> {
    let project_id = project_id?;
    let mut matches = projects.iter().filter(|project| &project.id == project_id);
    match environment_id {
        Some(environment_id) => matches.find(|project| &project.environment_id == environment_id),
        None => {
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        }
    }
}

/// Which environments to ask once the project scope is known (`resolveQueryEnvironmentIds`).
pub fn resolve_query_environment_ids(
    environment_ids: &[EnvironmentId],
    projects: &[ScopeProject],
    scoped_project: Option<&ScopeProject>,
    scoped_project_id: Option<&ProjectId>,
    projects_known: bool,
) -> Vec<EnvironmentId> {
    if let Some(scoped) = scoped_project {
        return environment_ids
            .iter()
            .filter(|id| **id == scoped.environment_id)
            .cloned()
            .collect();
    }
    let Some(project_id) = scoped_project_id.filter(|_| projects_known) else {
        return environment_ids.to_vec();
    };
    environment_ids
        .iter()
        .filter(|environment_id| {
            projects.iter().any(|project| {
                &project.id == project_id && &project.environment_id == *environment_id
            })
        })
        .cloned()
        .collect()
}

/// The server a saved selection names; a name the workspace has never heard of falls back.
pub fn resolve_selected_environment_id(
    named: Option<&EnvironmentId>,
    known: &[EnvironmentId],
    fallback: Option<&EnvironmentId>,
) -> Option<EnvironmentId> {
    match named {
        Some(named) if known.contains(named) => Some(named.clone()),
        _ => fallback.cloned(),
    }
}

/// Gives each shared repository to one server (the preferred one where it has it, else the
/// first by rank), so its pull requests are listed once. Projects with no identity are never
/// de-duplicated. Servers left with nothing are absent (`assignProjectsToEnvironments`).
pub fn assign_projects_to_environments(
    projects: &[ScopeProject],
    environment_ids: &[EnvironmentId],
    preferred: Option<&EnvironmentId>,
) -> BTreeMap<EnvironmentId, Vec<ProjectId>> {
    let rank: HashMap<&EnvironmentId, usize> = environment_ids
        .iter()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect();
    let mut owner: HashMap<String, &EnvironmentId> = HashMap::new();
    for project in projects {
        let Some(key) = project.repository_key() else {
            continue;
        };
        let Some(project_rank) = rank.get(&project.environment_id) else {
            continue;
        };
        match owner.get(&key) {
            None => {
                owner.insert(key, &project.environment_id);
            }
            Some(current) if Some(*current) == preferred => {}
            Some(current) => {
                if Some(&project.environment_id) == preferred
                    || *project_rank < rank.get(current).copied().unwrap_or(usize::MAX)
                {
                    owner.insert(key, &project.environment_id);
                }
            }
        }
    }
    let mut assignment: BTreeMap<EnvironmentId, Vec<ProjectId>> = BTreeMap::new();
    for project in projects {
        if !rank.contains_key(&project.environment_id) {
            continue;
        }
        if let Some(key) = project.repository_key()
            && owner.get(&key) != Some(&&project.environment_id)
        {
            continue;
        }
        assignment
            .entry(project.environment_id.clone())
            .or_default()
            .push(project.id.clone());
    }
    assignment
}

/// The Filters menu's projects: one per repository per server (keeping the selected checkout),
/// duplicate titles suffixed with the server label, then the path, then the environment id,
/// then the project id; sorted by title (`pullRequestFilterProjects`).
pub fn filter_projects(
    projects: &[ScopeProject],
    environment_labels: &BTreeMap<EnvironmentId, String>,
    selected: Option<(&ProjectId, &EnvironmentId)>,
) -> Vec<ScopeProject> {
    let mut by_repository: Vec<(String, ScopeProject)> = Vec::new();
    for project in projects {
        let key = match project.repository_key() {
            Some(repository) => format!("{}\0repository\0{repository}", project.environment_id.0),
            None => format!("{}\0project\0{}", project.environment_id.0, project.id.0),
        };
        let is_selected = selected.is_some_and(|(id, environment)| {
            &project.id == id && &project.environment_id == environment
        });
        match by_repository.iter_mut().find(|(held, _)| *held == key) {
            None => by_repository.push((key, project.clone())),
            Some(slot) if is_selected => slot.1 = project.clone(),
            Some(_) => {}
        }
    }
    let projects: Vec<ScopeProject> = by_repository
        .into_iter()
        .map(|(_, project)| project)
        .collect();
    let projects = distinguish_titles(projects, |project| {
        environment_labels
            .get(&project.environment_id)
            .cloned()
            .unwrap_or_else(|| project.environment_id.0.clone())
    });
    let projects = distinguish_titles(projects, |project| project.workspace_root.clone());
    let projects = distinguish_titles(projects, |project| project.environment_id.0.clone());
    let mut projects = distinguish_titles(projects, |project| project.id.0.clone());
    projects.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.title.cmp(&right.title))
    });
    projects
}

fn distinguish_titles(
    projects: Vec<ScopeProject>,
    suffix: impl Fn(&ScopeProject) -> String,
) -> Vec<ScopeProject> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for project in &projects {
        *counts.entry(project.title.clone()).or_default() += 1;
    }
    projects
        .into_iter()
        .map(|mut project| {
            if counts.get(&project.title).copied().unwrap_or(0) > 1 {
                project.title = format!("{} · {}", project.title, suffix(&project));
            }
            project
        })
        .collect()
}

/// The host a project's repository lives under: the canonical key's first segment, or for
/// Forgejo an http(s) remote's host; the provider kind when neither is known.
pub fn host_of(
    canonical_key: Option<&str>,
    remote_url: Option<&str>,
    kind: &SourceControlProviderKind,
) -> String {
    if *kind == SourceControlProviderKind::Forgejo
        && let Some(host) = remote_url.and_then(http_host)
    {
        return host;
    }
    canonical_key
        .and_then(|key| key.split('/').next())
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .map(str::to_lowercase)
        .unwrap_or_else(|| kind.as_str().to_owned())
}

/// `host[:port]` of an http(s) URL, lowercased.
fn http_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}
