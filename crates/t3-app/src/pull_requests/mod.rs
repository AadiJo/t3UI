//! The Pull Requests page (`/pull-requests`, spec `docs/spec/pull-requests.md`): every pull
//! request from every connected server that can list them, grouped by the reader's involvement,
//! with search, sort and filters.
//!
//! [`PullRequestsView`] owns the list scope (persisted as `pull-requests.json`), the reads
//! against each environment, and the rows held between answers. The pure decisions (grouping,
//! sorting, filtering, query parsing) are `t3_logic::pull_requests`; rendering is split into
//! [`list`] (rows and groups), [`states`] (ghost, empty, unavailable) and [`controls`]. Snapshot
//! scenes feed it through [`data::Fixture`].

mod controls;
pub mod data;
mod list;
mod render;
mod states;
mod style;

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use gpui_kit::{
    AppContext as _, Context, Entity, SharedString, Subscription, Task, Window,
    component::input::{InputEvent, InputState},
};
use t3_logic::pull_requests::{
    self as logic, DiffStats, EnvironmentEntry, Involvement, ListOverride, ListPreferences,
    ListSort, ListState, MergedList, ParsedQuery, ScopeProject, StatsPolicy, entries_by_key,
    merge_lists, parse_query,
};
use t3_protocol::{
    EnvironmentId,
    pull_requests::{PullRequestListFilters, PullRequestListInput, PullRequestListStatsInput},
};

use crate::state::{AppState, Environment, Store};

/// Rows asked of each host per read: GitHub serves a hundred per request and every provider asks
/// for one more as its "is there more" probe, so 99 costs one round trip.
const PAGE_SIZE: u32 = 99;
/// The largest page the listing accepts.
const MAX_PAGE_SIZE: u32 = 500;
/// Long enough that a keystroke does not become a request.
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const PREFERENCES_FILE: &str = "pull-requests.json";

/// One merged read across environments, and the question it answers.
#[derive(Default)]
struct Read {
    /// [`PullRequestsView::filter_key`] the read was started for.
    key: String,
    pending: bool,
    data: Option<MergedList>,
    error: Option<String>,
    _task: Option<Task<()>>,
}

/// What the page knows about the environments right now, recomputed from [`AppState`].
#[derive(Clone, Default, PartialEq)]
struct Workspace {
    /// Some environment has sent its server config, so "none can list pull requests" is known.
    capability_known: bool,
    /// Capable environments, sorted by id.
    environments: Vec<(EnvironmentId, Entity<Environment>)>,
    labels: BTreeMap<EnvironmentId, String>,
    /// Projects on capable environments.
    projects: Vec<ScopeProject>,
    /// Every capable environment's shell has data.
    projects_known: bool,
}

/// The `/pull-requests` main view.
pub struct PullRequestsView {
    app_state: Entity<AppState>,
    store: Store,
    /// The list controls, as remembered.
    scope: ListPreferences,
    search: Entity<InputState>,
    /// The search text the hosts were asked about (lags typing by [`SEARCH_DEBOUNCE`]).
    sent_query: String,
    debounce: Option<Task<()>>,
    workspace: Workspace,
    list: Read,
    /// The Authored and Reviewing groups' own reads (only for involvement "all", no search).
    authored: Read,
    reviewing: Read,
    /// The last settled answer, carried (narrowed) while a new question is in flight.
    carried: Option<MergedList>,
    /// Rows in held order for `held_key`; a refresh reuses unchanged `Arc`s.
    held: Vec<Arc<EnvironmentEntry>>,
    held_key: String,
    page_size: u32,
    stats: DiffStats,
    stats_requested: std::collections::BTreeSet<String>,
    _stats_tasks: Vec<Task<()>>,
    overrides: BTreeMap<String, ListOverride>,
    /// Entry key of the open pull request.
    selected: Option<String>,
    invalidating: bool,
    _refresh: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
    _environment_observers: Vec<Subscription>,
}

impl PullRequestsView {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = app_state.read(cx).store().clone();
        let scope = store
            .read(PREFERENCES_FILE)
            .map(|json| ListPreferences::from_json(&json))
            .unwrap_or_default();
        let initial_query = scope.q.clone().unwrap_or_default();
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search pull requests, or label:bug")
                .default_value(initial_query.clone())
        });
        let subscriptions = vec![
            cx.observe(&app_state, |this, _, cx| this.sync_workspace(cx)),
            cx.subscribe_in(
                &search,
                window,
                |this, search, event: &InputEvent, _, cx| {
                    if let InputEvent::Change = event {
                        let text = search.read(cx).value().to_string();
                        this.set_query(text, cx);
                    }
                },
            ),
        ];
        let mut view = Self {
            app_state,
            store,
            scope,
            search,
            sent_query: initial_query.trim().to_owned(),
            debounce: None,
            workspace: Workspace::default(),
            list: Read::default(),
            authored: Read::default(),
            reviewing: Read::default(),
            carried: None,
            held: Vec::new(),
            held_key: String::new(),
            page_size: PAGE_SIZE,
            stats: DiffStats::default(),
            stats_requested: Default::default(),
            _stats_tasks: Vec::new(),
            overrides: BTreeMap::new(),
            selected: None,
            invalidating: false,
            _refresh: None,
            _subscriptions: subscriptions,
            _environment_observers: Vec::new(),
        };
        view.sync_workspace(cx);
        view
    }

    // -----------------------------------------------------------------------------------------
    // Workspace

    /// Re-reads environments and projects from app state; starts reads when the answerable
    /// question changed.
    fn sync_workspace(&mut self, cx: &mut Context<Self>) {
        let environments = self.app_state.read(cx).environments().to_vec();
        let mut next = Workspace {
            projects_known: true,
            ..Workspace::default()
        };
        for entity in &environments {
            let environment = entity.read(cx);
            let Some(config) = environment.config() else {
                continue;
            };
            next.capability_known = true;
            if !config.environment.capabilities.pull_requests {
                continue;
            }
            let id = environment.id().clone();
            next.labels
                .insert(id.clone(), environment.label().to_string());
            next.projects_known &= environment.shell().has_data();
            next.projects.extend(
                environment
                    .projects()
                    .iter()
                    .map(|project| ScopeProject::from_shell(&id, project)),
            );
            next.environments.push((id, entity.clone()));
        }
        next.environments
            .sort_by(|left, right| left.0.cmp(&right.0));
        if self._environment_observers.len() != environments.len() {
            self._environment_observers = environments
                .iter()
                .map(|entity| cx.observe(entity, |this, _, cx| this.sync_workspace(cx)))
                .collect();
        }
        if next != self.workspace {
            self.workspace = next;
            self.ensure_reads(cx);
        }
        cx.notify();
    }

    fn environment_ids(&self) -> Vec<EnvironmentId> {
        let scoped = self.scope.environment_id.as_ref().filter(|id| {
            self.workspace
                .environments
                .iter()
                .any(|(held, _)| held == *id)
        });
        self.workspace
            .environments
            .iter()
            .map(|(id, _)| id.clone())
            .filter(|id| scoped.is_none_or(|scoped| scoped == id))
            .collect()
    }

    /// The project scope actually asked for, and the project it names.
    fn scoped_project(&self) -> (Option<t3_protocol::ProjectId>, Option<ScopeProject>) {
        let projects: Vec<ScopeProject> = self.capable_projects();
        let project_id = logic::resolve_project_scope(
            self.scope.project_id.as_ref(),
            &projects,
            self.workspace.projects_known,
        );
        let project = logic::find_scoped_project(
            &projects,
            self.scope.environment_id.as_ref(),
            project_id.as_ref(),
        )
        .cloned();
        (project_id, project)
    }

    fn capable_projects(&self) -> Vec<ScopeProject> {
        let ids = self.environment_ids();
        self.workspace
            .projects
            .iter()
            .filter(|project| ids.contains(&project.environment_id))
            .cloned()
            .collect()
    }

    /// Which environments are asked, and about which projects (`environmentQueries`).
    fn environment_queries(&self) -> Vec<(EnvironmentId, Option<Vec<t3_protocol::ProjectId>>)> {
        let projects = self.capable_projects();
        let (project_id, project) = self.scoped_project();
        let ids = logic::resolve_query_environment_ids(
            &self.environment_ids(),
            &projects,
            project.as_ref(),
            project_id.as_ref(),
            self.workspace.projects_known,
        );
        if !self.workspace.projects_known || project_id.is_some() {
            return ids.into_iter().map(|id| (id, None)).collect();
        }
        let assignment = logic::assign_projects_to_environments(&projects, &ids, ids.first());
        ids.into_iter()
            .filter_map(|id| {
                let assigned = assignment.get(&id)?;
                let total = projects
                    .iter()
                    .filter(|project| project.environment_id == id)
                    .count();
                Some(if assigned.len() == total {
                    (id, None)
                } else {
                    (id, Some(assigned.clone()))
                })
            })
            .collect()
    }

    // -----------------------------------------------------------------------------------------
    // Scope

    /// The menu filters as the listing takes them.
    fn menu_filters(&self) -> PullRequestListFilters {
        PullRequestListFilters {
            draft: self.scope.draft.clone(),
            review: self.scope.review.clone(),
            checks: self.scope.checks.clone(),
            labels: (!self.scope.labels.is_empty()).then(|| {
                self.scope
                    .labels
                    .iter()
                    .map(|label| vec![label.clone()])
                    .collect()
            }),
            excluded_labels: None,
            author: self.scope.author.clone(),
        }
    }

    fn typed_query(&self) -> String {
        self.scope.q.clone().unwrap_or_default().trim().to_owned()
    }

    fn sent_parsed(&self) -> ParsedQuery {
        parse_query(&self.sent_query)
    }

    /// Filters for the request: the menu's, with typed qualifiers winning.
    fn request_filters(&self, parsed: &ParsedQuery) -> PullRequestListFilters {
        merge_filters(self.menu_filters(), &parsed.filters)
    }

    /// Everything the answer depends on except page size (`filterKey`).
    fn filter_key(&self) -> String {
        let (project, _) = self.scoped_project();
        format!(
            "{:?}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}",
            self.environment_queries(),
            self.scope.state,
            self.scope.involvement,
            project,
            self.scope.host,
            self.scope.draft,
            self.scope.review,
            self.scope.checks,
            self.scope.author,
            self.scope.labels,
            self.sent_query,
        )
    }

    /// Applies a list-control change, remembers the whole scope, and re-reads.
    fn update_scope(&mut self, edit: impl FnOnce(&mut ListPreferences), cx: &mut Context<Self>) {
        edit(&mut self.scope);
        self.scope = std::mem::take(&mut self.scope).normalized();
        self.store
            .write(PREFERENCES_FILE, self.scope.to_json(), cx)
            .detach();
        self.ensure_reads(cx);
        cx.notify();
    }

    fn set_query(&mut self, text: String, cx: &mut Context<Self>) {
        if self.scope.q.as_deref().unwrap_or_default() == text {
            return;
        }
        self.update_scope(|scope| scope.q = Some(text), cx);
        let typed = self.typed_query();
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update(cx, |this, cx| {
                this.sent_query = typed;
                this.ensure_reads(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    fn clear_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |search, cx| search.set_value("", window, cx));
        self.sent_query.clear();
        self.debounce = None;
        self.update_scope(|scope| scope.q = None, cx);
    }

    fn set_sort(&mut self, sort: ListSort, cx: &mut Context<Self>) {
        self.update_scope(|scope| scope.sort = Some(sort), cx);
        self.request_stats(cx);
    }

    fn set_state(&mut self, state: ListState, cx: &mut Context<Self>) {
        self.update_scope(|scope| scope.state = state, cx);
    }

    fn set_involvement(&mut self, involvement: Involvement, cx: &mut Context<Self>) {
        self.update_scope(|scope| scope.involvement = involvement, cx);
    }

    fn set_host(&mut self, host: Option<String>, cx: &mut Context<Self>) {
        self.update_scope(|scope| scope.host = host, cx);
    }

    // -----------------------------------------------------------------------------------------
    // Reads

    fn list_input(
        &self,
        involvement: Involvement,
        projects: Option<Vec<t3_protocol::ProjectId>>,
        limit: u32,
        with_query: bool,
    ) -> PullRequestListInput {
        let parsed = self.sent_parsed();
        let filters = if with_query {
            self.request_filters(&parsed)
        } else {
            self.menu_filters()
        };
        let (project_id, _) = self.scoped_project();
        let mut input = PullRequestListInput::new(self.scope.state.clone());
        input.involvement = Some(involvement);
        input.limit = Some(limit);
        input.project_id = project_id;
        input.project_ids = projects;
        input.host = self.scope.host.clone();
        input.filters = (filters != PullRequestListFilters::default()).then_some(filters);
        input.query = (with_query && !parsed.text.is_empty()).then(|| parsed.text.clone());
        input
    }

    /// Starts the reads the current scope needs, if they are not already answering it.
    fn ensure_reads(&mut self, cx: &mut Context<Self>) {
        let key = self.filter_key();
        if self.list.key != key {
            self.page_size = PAGE_SIZE;
            self.start_list(key.clone(), cx);
        }
        let partitions_wanted =
            self.scope.involvement == Involvement::All && self.typed_query().is_empty();
        let partition_key = format!("{key}|partitions");
        if partitions_wanted && self.authored.key != partition_key {
            self.authored = self.start_read(partition_key.clone(), Involvement::Authored, cx);
            self.reviewing = self.start_read(partition_key, Involvement::Reviewing, cx);
        }
    }

    fn start_list(&mut self, key: String, cx: &mut Context<Self>) {
        let involvement = self.scope.involvement.clone();
        self.list = self.start_read(key, involvement, cx);
    }

    /// Reads every queried environment and merges the answers into one [`Read`].
    fn start_read(&self, key: String, involvement: Involvement, cx: &mut Context<Self>) -> Read {
        let queries = self.environment_queries();
        if queries.is_empty() {
            return Read {
                key,
                ..Read::default()
            };
        }
        let with_query = involvement == self.scope.involvement;
        let limit = if with_query {
            self.page_size
        } else {
            PAGE_SIZE
        };
        let tasks: Vec<_> = queries
            .into_iter()
            .filter_map(|(id, projects)| {
                let entity = self
                    .workspace
                    .environments
                    .iter()
                    .find(|(held, _)| *held == id)?
                    .1
                    .clone();
                let input = self.list_input(involvement.clone(), projects, limit, with_query);
                Some((id, data::list(&entity, input, cx)))
            })
            .collect();
        let slot = involvement.clone();
        let read_key = key.clone();
        let is_list = with_query;
        let task = cx.spawn(async move |this, cx| {
            let mut answers = Vec::new();
            let mut error = None;
            for (id, task) in tasks {
                match task.await {
                    Ok(answer) => answers.push((id, answer)),
                    Err(message) => {
                        error.get_or_insert(message);
                    }
                }
            }
            this.update(cx, |this, cx| {
                let read = if is_list {
                    &mut this.list
                } else if slot == Involvement::Authored {
                    &mut this.authored
                } else {
                    &mut this.reviewing
                };
                if read.key != read_key {
                    return;
                }
                read.pending = false;
                read.data = merge_lists(&answers);
                read.error = error;
                if is_list {
                    this.settle_list(cx);
                }
                cx.notify();
            })
            .ok();
        });
        Read {
            key,
            pending: true,
            data: None,
            error: None,
            _task: Some(task),
        }
    }

    /// A list answer landed: order it, clear overrides it confirms, ask for its line counts.
    fn settle_list(&mut self, cx: &mut Context<Self>) {
        let Some(answer) = self.list.data.clone() else {
            return;
        };
        let text = self.sent_parsed().text;
        let ranked = logic::rank_matches(&answer.entries, &text);
        self.held = if self.held_key == self.list.key {
            logic::reuse_entries(&self.held, ranked)
        } else {
            ranked
        };
        self.held_key = self.list.key.clone();
        self.overrides = logic::settle_overrides(
            &self.overrides,
            &answer.entries,
            self.app_state.read(cx).now_millis(),
        );
        self.carried = Some(answer);
        self.request_stats(cx);
    }

    /// Reads line counts for rows that came without them.
    fn request_stats(&mut self, cx: &mut Context<Self>) {
        let rows = entries_by_key(self.held.iter());
        let candidates = rows.keys().cloned().collect();
        // Rows near the viewport are not tracked yet, so every sort reads eagerly.
        let _policy = if self.scope.sort().needs_all_stats() {
            StatsPolicy::Eager
        } else {
            StatsPolicy::Visible
        };
        let batches =
            logic::stats_request_batches(&rows, &candidates, StatsPolicy::Eager, &[], &self.stats)
                .into_iter()
                .filter(|batch| {
                    !batch
                        .keys
                        .iter()
                        .all(|key| self.stats_requested.contains(key))
                })
                .collect::<Vec<_>>();
        for batch in batches {
            self.stats_requested.extend(batch.keys.iter().cloned());
            let Some(entity) = self
                .workspace
                .environments
                .iter()
                .find(|(id, _)| *id == batch.environment_id)
                .map(|(_, entity)| entity.clone())
            else {
                continue;
            };
            let task =
                data::list_stats(&entity, PullRequestListStatsInput { refs: batch.refs }, cx);
            let environment_id = batch.environment_id;
            self._stats_tasks.push(cx.spawn(async move |this, cx| {
                if let Ok(stats) = task.await {
                    this.update(cx, |this, cx| {
                        logic::merge_diff_stats(&mut this.stats, &environment_id, &stats);
                        cx.notify();
                    })
                    .ok();
                }
            }));
        }
    }

    /// The header's refresh: invalidate every queried environment, then read again.
    fn refresh_from_host(&mut self, cx: &mut Context<Self>) {
        if self.invalidating {
            return;
        }
        self.invalidating = true;
        let tasks: Vec<_> = self
            .environment_queries()
            .into_iter()
            .filter_map(|(id, _)| {
                let entity = self
                    .workspace
                    .environments
                    .iter()
                    .find(|(held, _)| *held == id)?
                    .1
                    .clone();
                Some(data::invalidate(&entity, cx))
            })
            .collect();
        self._refresh = Some(cx.spawn(async move |this, cx| {
            for task in tasks {
                task.await;
            }
            this.update(cx, |this, cx| {
                this.invalidating = false;
                this.reread(cx);
            })
            .ok();
        }));
        cx.notify();
    }

    /// Re-reads the current question, keeping the rows on screen until it answers.
    fn reread(&mut self, cx: &mut Context<Self>) {
        let key = self.filter_key();
        self.start_list(key.clone(), cx);
        if !self.authored.key.is_empty() {
            let partition_key = format!("{key}|partitions");
            self.authored = self.start_read(partition_key.clone(), Involvement::Authored, cx);
            self.reviewing = self.start_read(partition_key, Involvement::Reviewing, cx);
        }
        self.stats_requested.clear();
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        self.page_size = (self.page_size + PAGE_SIZE).min(MAX_PAGE_SIZE);
        let key = self.filter_key();
        self.start_list(key, cx);
        cx.notify();
    }

    fn select(&mut self, key: String, cx: &mut Context<Self>) {
        self.selected = Some(key);
        cx.notify();
    }
}

/// The menu's filters with the typed qualifiers written over them.
fn merge_filters(
    menu: PullRequestListFilters,
    typed: &PullRequestListFilters,
) -> PullRequestListFilters {
    PullRequestListFilters {
        draft: typed.draft.clone().or(menu.draft),
        review: typed.review.clone().or(menu.review),
        checks: typed.checks.clone().or(menu.checks),
        labels: typed.labels.clone().or(menu.labels),
        excluded_labels: typed.excluded_labels.clone().or(menu.excluded_labels),
        author: typed.author.clone().or(menu.author),
    }
}

/// Label for the provider menu trigger and options.
fn host_options(view: &PullRequestsView) -> Vec<(String, SharedString)> {
    let providers = view
        .list
        .data
        .as_ref()
        .or(view.carried.as_ref())
        .map(|list| list.providers.clone())
        .unwrap_or_default();
    let hosts: Vec<_> = providers
        .iter()
        .map(|provider| (provider.host.clone(), provider.kind.clone()))
        .collect();
    providers
        .iter()
        .map(|provider| {
            (
                provider.host.clone(),
                logic::host_label(&hosts, &provider.host, &provider.kind).into(),
            )
        })
        .collect()
}
