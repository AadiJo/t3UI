//! Archive page (`SettingsPanels.tsx:1516-1737`, spec 3.9): archived threads from every
//! environment that has projects, grouped by project, newest first. Each row can be unarchived;
//! right-click offers Unarchive and Delete.
//!
//! Data comes from `orchestration.getArchivedShellSnapshot` per environment, kept in the global
//! [`ArchivedCache`] so revisiting the page shows the last result while it refreshes. The page
//! refetches when an environment's shell changes (a thread was archived, unarchived, or
//! deleted).

use std::{collections::HashMap, sync::Arc};

use gpui_kit::{
    Action, App, Context, Entity, Global, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window,
    component::{Icon as MenuIcon, native_menu::NativeMenu},
    div, px,
};
use t3_client::commands;
use t3_logic::time::{format_relative_time, parse_timestamp};
use t3_protocol::{
    EnvironmentId, ThreadId,
    methods::{Empty, GetArchivedShellSnapshot},
    orchestration::{OrchestrationShellSnapshot, OrchestrationThreadShell},
};
use t3_ui::{ActiveColors as _, Button, ButtonSize, ButtonVariant, Icon, IconName};

use super::layout::{PAGE_MAX_WIDTH, SettingsRow, SettingsSection, page};
use crate::{
    dialogs::confirm,
    state::{AppState, Environment},
    toast::{self, Toast},
};

/// One environment's archived snapshot, or why it is missing.
#[derive(Clone)]
pub enum ArchivedSnapshot {
    Loading,
    Loaded(Arc<OrchestrationShellSnapshot>),
    Failed(SharedString),
}

/// The latest archived snapshot per environment.
#[derive(Default)]
pub struct ArchivedCache(HashMap<EnvironmentId, ArchivedSnapshot>);

impl Global for ArchivedCache {}

impl ArchivedCache {
    /// Records a snapshot (fetch results; snapshot scenes seed it with fixtures).
    pub fn set(environment: EnvironmentId, snapshot: ArchivedSnapshot, cx: &mut App) {
        cx.default_global::<Self>().0.insert(environment, snapshot);
    }
}

/// Context menu choice on an archived row.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = settings, no_json)]
pub struct ArchivedThreadAction {
    pub environment: EnvironmentId,
    pub thread: ThreadId,
    pub delete: bool,
}

/// The Archive page.
pub struct ArchivedPage {
    app_state: Entity<AppState>,
    /// Shell sequence each environment was last fetched at.
    fetched_at: HashMap<EnvironmentId, u64>,
    focus: gpui_kit::FocusHandle,
    _app_state: Subscription,
}

impl ArchivedPage {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let mut page = Self {
            _app_state: cx.observe(&app_state, |this, _, cx| {
                this.refresh_changed(cx);
                cx.notify();
            }),
            app_state,
            fetched_at: HashMap::new(),
            focus: cx.focus_handle(),
        };
        page.refresh_changed(cx);
        page
    }

    /// Environments with projects whose shell moved since the last fetch.
    fn refresh_changed(&mut self, cx: &mut Context<Self>) {
        let environments: Vec<Entity<Environment>> =
            self.app_state.read(cx).environments().to_vec();
        for environment in environments {
            let (id, sequence, has_projects, client) = {
                let environment = environment.read(cx);
                (
                    environment.id().clone(),
                    environment.shell().snapshot_sequence,
                    !environment.projects().is_empty(),
                    environment.client().cloned(),
                )
            };
            let Some(client) = client.filter(|client| has_projects && client.session().is_some())
            else {
                continue;
            };
            if self.fetched_at.get(&id) == Some(&sequence) {
                continue;
            }
            self.fetched_at.insert(id.clone(), sequence);
            if !cx.default_global::<ArchivedCache>().0.contains_key(&id) {
                ArchivedCache::set(id.clone(), ArchivedSnapshot::Loading, cx);
            }
            cx.spawn(async move |this, cx| {
                let result = client.request::<GetArchivedShellSnapshot>(&Empty {}).await;
                this.update(cx, |_, cx| {
                    let snapshot = match result {
                        Ok(snapshot) => ArchivedSnapshot::Loaded(Arc::new(snapshot)),
                        Err(error) => ArchivedSnapshot::Failed(error.to_string().into()),
                    };
                    ArchivedCache::set(id, snapshot, cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn run(&mut self, action: &ArchivedThreadAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(environment) = self.app_state.read(cx).environment(&action.environment, cx) else {
            return;
        };
        let thread = action.thread.clone();
        if !action.delete {
            let task = environment
                .read(cx)
                .dispatch(commands::unarchive_thread(thread), cx);
            self.after(task, "Failed to unarchive thread", &action.environment, cx);
            return;
        }
        let title = self
            .thread(&action.environment, &thread, cx)
            .map(|thread| thread.title.clone())
            .unwrap_or_default();
        let confirm_first = self.app_state.read(cx).settings().confirm_thread_delete;
        let confirmed = confirm_first.then(|| {
            confirm(
                &format!(
                    "Delete thread \"{title}\"?\nThis permanently clears conversation history for this thread."
                ),
                window,
                cx,
            )
        });
        let environment_id = action.environment.clone();
        cx.spawn(async move |this, cx| {
            if let Some(confirmed) = confirmed
                && !confirmed.await
            {
                return;
            }
            this.update(cx, |this, cx| {
                let task = environment
                    .read(cx)
                    .dispatch(commands::delete_thread(thread), cx);
                this.after(task, "Failed to delete thread", &environment_id, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Awaits a dispatched command, toasts a failure, and refetches the environment.
    fn after(
        &mut self,
        task: gpui_kit::Task<anyhow::Result<()>>,
        failure: &'static str,
        environment: &EnvironmentId,
        cx: &mut Context<Self>,
    ) {
        let environment = environment.clone();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                if let Err(error) = result {
                    toast::show(
                        Toast::error(failure)
                            .description(error.to_string())
                            .stacked(),
                        cx,
                    );
                }
                this.fetched_at.remove(&environment);
                this.refresh_changed(cx);
            })
            .ok();
        })
        .detach();
    }

    fn thread(
        &self,
        environment: &EnvironmentId,
        thread: &ThreadId,
        cx: &App,
    ) -> Option<Arc<OrchestrationThreadShell>> {
        match cx.try_global::<ArchivedCache>()?.0.get(environment)? {
            ArchivedSnapshot::Loaded(snapshot) => snapshot
                .threads
                .iter()
                .find(|candidate| &candidate.id == thread)
                .cloned(),
            _ => None,
        }
    }
}

/// Archived threads of one project, newest archive first.
struct ProjectGroup {
    title: String,
    threads: Vec<(EnvironmentId, Arc<OrchestrationThreadShell>)>,
}

fn archive_sort_key(thread: &OrchestrationThreadShell) -> i64 {
    thread
        .archived_at
        .as_deref()
        .and_then(parse_timestamp)
        .or_else(|| parse_timestamp(&thread.created_at))
        .unwrap_or(0)
}

impl Render for ArchivedPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let state = self.app_state.read(cx);
        let now = state.now_millis();
        let environment_ids: Vec<EnvironmentId> = state
            .environments()
            .iter()
            .filter(|environment| !environment.read(cx).projects().is_empty())
            .map(|environment| environment.read(cx).id().clone())
            .collect();
        let cache = cx.try_global::<ArchivedCache>();
        let snapshots: Vec<(EnvironmentId, ArchivedSnapshot)> = environment_ids
            .iter()
            .map(|id| {
                let snapshot = cache
                    .and_then(|cache| cache.0.get(id).cloned())
                    .unwrap_or(ArchivedSnapshot::Loading);
                (id.clone(), snapshot)
            })
            .collect();
        let mut groups: Vec<ProjectGroup> = Vec::new();
        for (environment, snapshot) in &snapshots {
            let ArchivedSnapshot::Loaded(snapshot) = snapshot else {
                continue;
            };
            for project in &snapshot.projects {
                let mut threads: Vec<_> = snapshot
                    .threads
                    .iter()
                    .filter(|thread| {
                        thread.project_id == project.id && thread.archived_at.is_some()
                    })
                    .map(|thread| (environment.clone(), thread.clone()))
                    .collect();
                if threads.is_empty() {
                    continue;
                }
                threads.sort_by(|(_, left), (_, right)| {
                    archive_sort_key(right)
                        .cmp(&archive_sort_key(left))
                        .then_with(|| left.id.as_str().cmp(right.id.as_str()))
                });
                groups.push(ProjectGroup {
                    title: project.title.clone(),
                    threads,
                });
            }
        }

        let sections: Vec<gpui_kit::AnyElement> = if groups.is_empty() {
            let loading = snapshots
                .iter()
                .any(|(_, snapshot)| matches!(snapshot, ArchivedSnapshot::Loading));
            let failure = snapshots.iter().find_map(|(_, snapshot)| match snapshot {
                ArchivedSnapshot::Failed(error) => Some(error.clone()),
                _ => None,
            });
            let (icon, title, description): (IconName, &str, SharedString) =
                if loading && !environment_ids.is_empty() {
                    (
                        IconName::Loader,
                        "Loading archived threads",
                        "Checking connected environments.".into(),
                    )
                } else if let Some(error) = failure {
                    (
                        IconName::CircleAlert,
                        "Could not load archived threads",
                        error,
                    )
                } else {
                    (
                        IconName::Archive,
                        "No archived threads",
                        "Archived threads will appear here.".into(),
                    )
                };
            vec![
                SettingsSection::new("Archived threads")
                    .child(SettingsRow::new(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Icon::new(icon).size(px(14.)))
                            .child(title),
                        description,
                    ))
                    .into_any_element(),
            ]
        } else {
            groups
                .into_iter()
                .map(|group| {
                    let rows = group.threads.into_iter().map(|(environment, thread)| {
                        let archived = thread
                            .archived_at
                            .as_deref()
                            .map(|at| format_relative_time(at, now))
                            .unwrap_or_default();
                        let created = format_relative_time(&thread.created_at, now);
                        let action = ArchivedThreadAction {
                            environment: environment.clone(),
                            thread: thread.id.clone(),
                            delete: false,
                        };
                        let menu_action = action.clone();
                        let focus = self.focus.clone();
                        div()
                            .id(SharedString::from(format!("archived-{}", thread.id)))
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                window.focus(&focus, cx);
                                let delete = ArchivedThreadAction {
                                    delete: true,
                                    ..menu_action.clone()
                                };
                                NativeMenu::new()
                                    .menu("Unarchive", Box::new(menu_action.clone()))
                                    .separator()
                                    .menu_with_icon(
                                        "Delete",
                                        MenuIcon::default().path(IconName::Trash2.path()),
                                        Box::new(delete),
                                    )
                                    .show(event.position, window, cx);
                            })
                            .child(
                                SettingsRow::new(
                                    thread.title.clone(),
                                    format!("Archived {archived} · Created {created}"),
                                )
                                .control(
                                    Button::new(SharedString::from(format!(
                                        "unarchive-{}",
                                        thread.id
                                    )))
                                    .variant(ButtonVariant::Outline)
                                    .size(ButtonSize::Sm)
                                    .icon(IconName::ArchiveX)
                                    .label("Unarchive")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.run(&action, window, cx)
                                    })),
                                ),
                            )
                    });
                    SettingsSection::new(group.title)
                        .icon(
                            Icon::new(IconName::Folder)
                                .size(px(14.))
                                .color(colors.muted_foreground_60),
                        )
                        .children(rows)
                        .into_any_element()
                })
                .collect()
        };
        div()
            .id("settings-archived")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::run))
            .size_full()
            .flex()
            .flex_col()
            .child(page("settings-archived-page", PAGE_MAX_WIDTH, sections))
    }
}
