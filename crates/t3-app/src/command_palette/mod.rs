//! The command palette (`web/components/CommandPalette.tsx`, `CommandPaletteResults.tsx`,
//! spec 4).
//!
//! One [`CommandPalette`] entity per window, drawn as a full-window overlay; the workspace mounts
//! it as its last child. It opens on the `commandPalette.toggle` command (routed through
//! [`AppState`]) or [`toggle`] / [`open_add_project`] from anywhere (the sidebar's Search row and
//! Add project button).
//!
//! Root view: "Actions" (new thread in the active project, "New thread in..." submenu, Add
//! project, Open settings) and "Recent Threads". Typing searches with
//! [`t3_logic::command_palette::filter_groups`]; a leading `>` limits results to actions. Add
//! project picks an environment (when there are several), then browses that server's filesystem
//! (`filesystem.browse`) and adds the folder with `project.create`.

use std::{rc::Rc, sync::Arc};

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, FontWeight, Global,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, ParentElement as _, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task,
    WeakEntity, Window,
    component::input::{Input as TextInput, InputEvent, InputState},
    div,
    prelude::FluentBuilder as _,
    px,
};
use t3_client::commands;
use t3_logic::{
    ProjectRef, ThreadRef,
    command_palette::{
        PaletteGroup, PaletteMode, PaletteSearch, RECENT_THREAD_LIMIT, browse, empty_text,
        filter_groups, group,
    },
    keybindings::Command,
    sidebar::{compare_threads, resolve_thread_status},
    time::format_relative_time,
};
use t3_protocol::{
    EnvironmentId, ProjectId,
    methods::FilesystemBrowse,
    orchestration::{OrchestrationProjectShell, OrchestrationThreadShell},
    projects::{FilesystemBrowseInput, FilesystemBrowseResult},
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Icon, IconName, Kbd,
    tokens::{shadow, text},
};

use crate::{
    chrome::{TypeScale as _, under_xs},
    keybindings::{ShortcutScope, shortcut_label},
    state::{AppEvent, AppState, Environment, NewThreadRequest, Route, SettingsPage},
    toast::{self, Toast},
};

type Run = Rc<dyn Fn(&mut Window, &mut Context<CommandPalette>)>;

/// What an item does.
#[derive(Clone)]
enum ItemKind {
    /// Runs and closes the palette (unless `keep_open`).
    Action { run: Run, keep_open: bool },
    /// Opens a nested view.
    Submenu { groups: Vec<PaletteGroup<Item>> },
}

/// The title, optionally ending in a bold part ("New thread in **aurora-web**").
#[derive(Clone)]
struct Title {
    text: SharedString,
    bold: Option<SharedString>,
}

impl Title {
    fn plain(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            bold: None,
        }
    }
}

/// One row (`CommandPaletteActionItem` / `CommandPaletteSubmenuItem`).
#[derive(Clone)]
struct Item {
    value: SharedString,
    search_terms: Vec<String>,
    title: Title,
    description: Option<SharedString>,
    timestamp: Option<SharedString>,
    icon: IconName,
    /// Thread status shown before the title (`ThreadRowLeadingStatus`).
    status: Option<t3_logic::sidebar::ThreadStatus>,
    shortcut: Option<Command>,
    disabled: bool,
    kind: ItemKind,
}

impl PaletteSearch for Item {
    fn search_terms(&self) -> &[String] {
        &self.search_terms
    }
}

impl Item {
    fn action(
        value: impl Into<SharedString>,
        title: Title,
        icon: IconName,
        search_terms: &[&str],
        run: impl Fn(&mut Window, &mut Context<CommandPalette>) + 'static,
    ) -> Self {
        Self {
            value: value.into(),
            search_terms: search_terms.iter().map(|term| (*term).to_owned()).collect(),
            title,
            description: None,
            timestamp: None,
            icon,
            status: None,
            shortcut: None,
            disabled: false,
            kind: ItemKind::Action {
                run: Rc::new(run),
                keep_open: false,
            },
        }
    }

    fn keep_open(mut self) -> Self {
        if let ItemKind::Action { keep_open, .. } = &mut self.kind {
            *keep_open = true;
        }
        self
    }

    fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A pushed submenu view.
#[derive(Clone)]
struct View {
    groups: Vec<PaletteGroup<Item>>,
}

/// Add-project browsing state: the environment being browsed and the last listing.
struct Browse {
    environment: EnvironmentId,
    result: Option<(String, Arc<FilesystemBrowseResult>)>,
    pending: Option<Task<()>>,
}

/// The palette overlay.
pub struct CommandPalette {
    app_state: Entity<AppState>,
    open: bool,
    input: Entity<InputState>,
    focus: FocusHandle,
    views: Vec<View>,
    browse: Option<Browse>,
    highlighted: Option<SharedString>,
    scroll: ScrollHandle,
    restore_focus: Option<FocusHandle>,
    placeholder_mode: PaletteMode,
    _subscriptions: Vec<Subscription>,
}

struct GlobalPalette(WeakEntity<CommandPalette>);

impl Global for GlobalPalette {}

/// Toggles the palette (sidebar Search row).
pub fn toggle(window: &mut Window, cx: &mut App) {
    if let Some(palette) = cx
        .try_global::<GlobalPalette>()
        .and_then(|global| global.0.upgrade())
    {
        palette.update(cx, |palette, cx| palette.toggle(window, cx));
    }
}

/// Opens the palette straight into the add-project flow (sidebar "Add project").
pub fn open_add_project(window: &mut Window, cx: &mut App) {
    if let Some(palette) = cx
        .try_global::<GlobalPalette>()
        .and_then(|global| global.0.upgrade())
    {
        palette.update(cx, |palette, cx| {
            palette.set_open(true, window, cx);
            palette.start_add_project(window, cx);
        });
    }
}

impl CommandPalette {
    /// Creates the window's palette and registers it for [`toggle`]. Mount the returned entity
    /// as the workspace's last child.
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder(PaletteMode::Root.placeholder()));
        let weak = cx.weak_entity();
        cx.set_global(GlobalPalette(weak));
        let subscriptions = vec![
            cx.subscribe_in(&app_state, window, |this, _, event, window, cx| {
                if event == &AppEvent::Command(Command::CommandPaletteToggle) {
                    this.toggle(window, cx);
                }
            }),
            cx.subscribe_in(&input, window, |this, _, event, window, cx| {
                if let InputEvent::Change = event {
                    this.on_query_changed(window, cx);
                }
            }),
            cx.observe(&app_state, |_, _, cx| cx.notify()),
        ];
        Self {
            app_state,
            open: false,
            input,
            focus: cx.focus_handle(),
            views: Vec::new(),
            browse: None,
            highlighted: None,
            scroll: ScrollHandle::new(),
            restore_focus: None,
            placeholder_mode: PaletteMode::Root,
            _subscriptions: subscriptions,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = !self.open;
        self.set_open(open, window, cx);
    }

    /// Opens at the root view with an empty query, or closes and returns focus.
    pub fn set_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.open == open {
            return;
        }
        self.open = open;
        ShortcutScope::update(cx, |scope| scope.command_palette_open = open);
        if open {
            self.restore_focus = window.focused(cx);
            self.views.clear();
            self.browse = None;
            self.set_query("", window, cx);
            self.input.update(cx, |input, cx| input.focus(window, cx));
            self.highlight_first(cx);
        } else if let Some(focus) = self.restore_focus.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Replaces the query, as if typed.
    pub fn search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.set_query(query, window, cx);
    }

    /// Runs the visible item with `value` (e.g. `"action:new-thread-in"`), as if clicked.
    pub fn run_item(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = self
            .visible_items(cx)
            .into_iter()
            .find(|item| item.value.as_ref() == value)
        {
            self.execute(item, window, cx);
        }
    }

    /// Sets the query (`set_value` emits no change event, so the dependent state updates here).
    fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_owned(), window, cx)
        });
        self.on_query_changed(window, cx);
    }

    fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn on_query_changed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let query = self.query(cx);
        if self.browse.is_some() {
            self.highlighted = None;
            self.fetch_browse(&query, cx);
        } else {
            self.highlight_first(cx);
        }
        cx.notify();
    }

    // -------------------------------------------------------------------------------------------
    // Items

    fn active_thread(
        &self,
        cx: &App,
    ) -> Option<(Entity<Environment>, Arc<OrchestrationThreadShell>)> {
        let state = self.app_state.read(cx);
        let thread = state.route().thread()?;
        let environment = state.environment(&thread.environment_id, cx)?;
        let shell = environment.read(cx).thread(&thread.thread_id)?.clone();
        Some((environment, shell))
    }

    fn all_projects(&self, cx: &App) -> Vec<(EnvironmentId, Arc<OrchestrationProjectShell>)> {
        self.app_state
            .read(cx)
            .environments()
            .iter()
            .flat_map(|environment| {
                let environment = environment.read(cx);
                let id = environment.id().clone();
                environment
                    .projects()
                    .iter()
                    .map(move |project| (id.clone(), project.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Thread rows (`buildThreadActionItems`), newest first, at most `limit`.
    fn thread_items(&self, limit: Option<usize>, cx: &App) -> Vec<Item> {
        let state = self.app_state.read(cx);
        let order = state.settings().sidebar_thread_sort_order;
        let now = state.now_millis();
        let active = state.route().thread().cloned();
        let mut threads: Vec<(EnvironmentId, Arc<OrchestrationThreadShell>, String)> = Vec::new();
        for environment in state.environments() {
            let environment = environment.read(cx);
            for thread in environment.threads() {
                if thread.archived_at.is_some() {
                    continue;
                }
                let project = environment
                    .project(&thread.project_id)
                    .map(|project| project.title.clone())
                    .unwrap_or_default();
                threads.push((environment.id().clone(), thread.clone(), project));
            }
        }
        threads.sort_by(|left, right| compare_threads(&left.1, &right.1, order));
        threads
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(environment, thread, project)| {
                let thread_ref = ThreadRef::new(environment, thread.id.clone());
                let mut description = Vec::new();
                if !project.is_empty() {
                    description.push(project.clone());
                }
                if let Some(branch) = &thread.branch {
                    description.push(format!("#{branch}"));
                }
                if active.as_ref() == Some(&thread_ref) {
                    description.push("Current thread".to_owned());
                }
                let last_visited = state.ui().last_visited_at(&thread_ref.key());
                let stamp = thread
                    .latest_user_message_at
                    .clone()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| thread.updated_at.clone());
                let target = thread_ref.clone();
                Item {
                    value: format!("thread:{}", thread.id).into(),
                    search_terms: vec![
                        thread.title.clone(),
                        project,
                        thread.branch.clone().unwrap_or_default(),
                    ],
                    title: Title::plain(thread.title.clone()),
                    description: Some(description.join(" · ").into()),
                    timestamp: Some(format_relative_time(&stamp, now).into()),
                    icon: IconName::MessageSquare,
                    status: resolve_thread_status(&thread, last_visited),
                    shortcut: None,
                    disabled: false,
                    kind: ItemKind::Action {
                        run: Rc::new(move |_, cx| {
                            let route = Route::Thread(target.clone());
                            AppState::global(cx).update(cx, |state, cx| state.navigate(route, cx));
                        }),
                        keep_open: false,
                    },
                }
            })
            .collect()
    }

    fn project_items(&self, prefix: &str, with_shortcut: bool, cx: &App) -> Vec<Item> {
        let carry = self.active_thread(cx).map(|(_, thread)| thread);
        self.all_projects(cx)
            .into_iter()
            .map(|(environment, project)| {
                let project_ref = ProjectRef::new(environment.clone(), project.id.clone());
                let carry = carry.clone();
                let opens_latest = prefix == "project";
                let mut item = Item::action(
                    format!("{prefix}:{environment}:{}", project.id),
                    Title::plain(project.title.clone()),
                    IconName::Folder,
                    &[],
                    move |_, cx| {
                        if opens_latest {
                            open_project(&project_ref, cx);
                        } else {
                            new_thread(project_ref.clone(), carry.as_deref(), cx);
                        }
                    },
                )
                .description(project.workspace_root.clone());
                item.search_terms = vec![project.title.clone(), project.workspace_root.clone()];
                if with_shortcut {
                    item.shortcut = Some(Command::ChatNew);
                }
                item
            })
            .collect()
    }

    fn root_groups(&self, cx: &App) -> Vec<PaletteGroup<Item>> {
        let mut actions = Vec::new();
        let projects = self.all_projects(cx);
        if !projects.is_empty() {
            if let Some((environment, thread)) = self.active_thread(cx) {
                let project = environment
                    .read(cx)
                    .project(&thread.project_id)
                    .map(|project| project.title.clone());
                if let Some(project) = project {
                    let project_ref = ProjectRef::new(
                        environment.read(cx).id().clone(),
                        thread.project_id.clone(),
                    );
                    let mut item = Item::action(
                        "action:new-thread",
                        Title {
                            text: "New thread in ".into(),
                            bold: Some(project.into()),
                        },
                        IconName::SquarePen,
                        &["new thread", "chat", "create", "draft"],
                        move |_, cx| new_thread(project_ref.clone(), Some(&thread), cx),
                    );
                    item.shortcut = Some(Command::ChatNew);
                    actions.push(item);
                }
            }
            actions.push(Item {
                value: "action:new-thread-in".into(),
                search_terms: ["new thread", "project", "pick", "choose", "select"]
                    .map(str::to_owned)
                    .to_vec(),
                title: Title::plain("New thread in..."),
                description: None,
                timestamp: None,
                icon: IconName::SquarePen,
                status: None,
                shortcut: None,
                disabled: false,
                kind: ItemKind::Submenu {
                    groups: vec![PaletteGroup {
                        value: "projects",
                        label: "Projects",
                        items: self.project_items("new-thread-in", true, cx),
                    }],
                },
            });
        }
        actions.push(
            Item::action(
                "action:add-project",
                Title::plain("Add project"),
                IconName::FolderPlus,
                &[
                    "add project",
                    "folder",
                    "directory",
                    "browse",
                    "clone",
                    "remote",
                    "repository",
                    "repo",
                    "git",
                    "github",
                    "gitlab",
                    "bitbucket",
                    "azure",
                    "devops",
                    "url",
                    "environment",
                ],
                |window, cx| {
                    cx.defer_in(window, |this, window, cx| {
                        this.start_add_project(window, cx)
                    })
                },
            )
            .keep_open(),
        );
        actions.push(Item::action(
            "action:settings",
            Title::plain("Open settings"),
            IconName::Settings,
            &["settings", "preferences", "configuration", "keybindings"],
            |_, cx| {
                AppState::global(cx).update(cx, |state, cx| {
                    state.navigate(Route::Settings(SettingsPage::General), cx)
                })
            },
        ));
        let mut groups = vec![PaletteGroup {
            value: group::ACTIONS,
            label: "Actions",
            items: actions,
        }];
        let recent = self.thread_items(Some(RECENT_THREAD_LIMIT), cx);
        if !recent.is_empty() {
            groups.push(PaletteGroup {
                value: group::RECENT_THREADS,
                label: "Recent Threads",
                items: recent,
            });
        }
        groups
    }

    /// The groups shown for the current view and query.
    fn displayed_groups(&self, cx: &App) -> Vec<PaletteGroup<Item>> {
        let query = self.query(cx);
        if let Some(browse) = &self.browse {
            return self.browse_groups(browse, &query);
        }
        let active = match self.views.last() {
            Some(view) => view.groups.clone(),
            None => self.root_groups(cx),
        };
        let in_submenu = !self.views.is_empty();
        let (projects, threads) = if in_submenu || query.trim().is_empty() {
            (Vec::new(), Vec::new())
        } else {
            (
                self.project_items("project", false, cx),
                self.thread_items(None, cx),
            )
        };
        filter_groups(&active, &query, in_submenu, &projects, &threads)
    }

    fn visible_items(&self, cx: &App) -> Vec<Item> {
        self.displayed_groups(cx)
            .into_iter()
            .flat_map(|group| group.items)
            .filter(|item| !item.disabled)
            .collect()
    }

    fn highlight_first(&mut self, cx: &App) {
        self.highlighted = self
            .visible_items(cx)
            .first()
            .map(|item| item.value.clone());
        self.scroll.scroll_to_item(0);
    }

    fn move_highlight(&mut self, delta: isize, cx: &mut Context<Self>) {
        let items = self.visible_items(cx);
        if items.is_empty() {
            return;
        }
        let current = self
            .highlighted
            .as_ref()
            .and_then(|value| items.iter().position(|item| &item.value == value));
        let next = match current {
            Some(index) => (index as isize + delta).rem_euclid(items.len() as isize) as usize,
            None if delta > 0 => 0,
            None => items.len() - 1,
        };
        self.highlighted = Some(items[next].value.clone());
        if let Some(row) = self.row_index_of(&items[next].value, cx) {
            self.scroll.scroll_to_item(row);
        }
        cx.notify();
    }

    /// Index among the list's direct children (group labels count) for scrolling.
    fn row_index_of(&self, value: &SharedString, cx: &App) -> Option<usize> {
        let mut index = 0;
        for group in self.displayed_groups(cx) {
            index += 1;
            for item in group.items {
                if &item.value == value {
                    return Some(index);
                }
                index += 1;
            }
        }
        None
    }

    fn execute(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        if item.disabled {
            return;
        }
        match item.kind {
            ItemKind::Submenu { groups } => {
                self.views.push(View { groups });
                self.set_query("", window, cx);
            }
            ItemKind::Action { run, keep_open } => {
                if !keep_open {
                    self.set_open(false, window, cx);
                }
                run(window, cx);
            }
        }
        cx.notify();
    }

    /// Back: leaves browse mode (to the environment picker, if it opened browsing), else pops
    /// the submenu.
    fn pop_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.browse.take().is_none() {
            self.views.pop();
        }
        self.set_query("", window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let modifiers = &event.keystroke.modifiers;
        match key {
            "escape" => self.set_open(false, window, cx),
            "up" => self.move_highlight(-1, cx),
            "down" => self.move_highlight(1, cx),
            "enter" => {
                let query = self.query(cx);
                let submit_path =
                    self.browse.is_some() && (self.highlighted.is_none() || modifiers.secondary());
                if submit_path {
                    self.add_project_at(&query, window, cx);
                } else if let Some(item) = self.highlighted.clone().and_then(|value| {
                    self.visible_items(cx)
                        .into_iter()
                        .find(|item| item.value == value)
                }) {
                    self.execute(item, window, cx);
                }
            }
            "backspace" if self.query(cx).is_empty() && !self.views.is_empty() => {
                self.pop_view(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    // -------------------------------------------------------------------------------------------
    // Add project

    /// "Add project": pick an environment when there are several, then browse it.
    fn start_add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.app_state.read(cx);
        let environments = state.environments().to_vec();
        let primary = state
            .primary_environment()
            .map(|primary| primary.read(cx).id().clone());
        match environments.len() {
            0 => {
                toast::show(
                    Toast::error("Unable to browse projects")
                        .description("No environment is available.")
                        .stacked(),
                    cx,
                );
            }
            1 => {
                let id = environments[0].read(cx).id().clone();
                self.start_browse(id, window, cx);
            }
            _ => {
                let mut options: Vec<(bool, String, EnvironmentId)> = environments
                    .iter()
                    .map(|environment| {
                        let environment = environment.read(cx);
                        let is_primary = Some(environment.id()) == primary.as_ref();
                        (
                            is_primary,
                            environment.label().to_string(),
                            environment.id().clone(),
                        )
                    })
                    .collect();
                options.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
                let items = options
                    .into_iter()
                    .map(|(is_primary, label, id)| {
                        let target = id.clone();
                        Item::action(
                            format!("environment:{id}"),
                            Title::plain(label.clone()),
                            IconName::Monitor,
                            &[],
                            move |window, cx| {
                                let target = target.clone();
                                cx.defer_in(window, move |this, window, cx| {
                                    this.start_browse(target, window, cx)
                                });
                            },
                        )
                        .keep_open()
                        .description(if is_primary {
                            "This device".to_owned()
                        } else {
                            id.to_string()
                        })
                    })
                    .map(|mut item| {
                        item.search_terms = vec![item.title.text.to_string()];
                        item
                    })
                    .collect();
                self.views.push(View {
                    groups: vec![PaletteGroup {
                        value: "environments",
                        label: "Environments",
                        items,
                    }],
                });
                self.set_query("", window, cx);
            }
        }
        cx.notify();
    }

    /// Enters browse mode for `environment` at its `addProjectBaseDirectory`, else `~/`.
    fn start_browse(
        &mut self,
        environment: EnvironmentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let base = self
            .app_state
            .read(cx)
            .environment(&environment, cx)
            .and_then(|environment| environment.read(cx).config().cloned())
            .and_then(|config| config.settings.add_project_base_directory.clone())
            .filter(|base| !base.trim().is_empty())
            .map(|base| browse::ensure_directory(&base))
            .unwrap_or_else(|| "~/".to_owned());
        self.browse = Some(Browse {
            environment,
            result: None,
            pending: None,
        });
        self.set_query(&base, window, cx);
    }

    fn fetch_browse(&mut self, query: &str, cx: &mut Context<Self>) {
        let directory = browse::directory_path(query).to_owned();
        let Some(browse) = self.browse.as_mut() else {
            return;
        };
        if directory.is_empty()
            || browse
                .result
                .as_ref()
                .is_some_and(|(listed, _)| *listed == directory)
        {
            return;
        }
        let client = self
            .app_state
            .read(cx)
            .environment(&browse.environment, cx)
            .and_then(|environment| environment.read(cx).client().cloned());
        let Some(client) = client else {
            return;
        };
        browse.pending = Some(cx.spawn(async move |this, cx| {
            let result = client
                .request::<FilesystemBrowse>(&FilesystemBrowseInput {
                    partial_path: directory.clone(),
                    cwd: None,
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(browse) = this.browse.as_mut() {
                    browse.pending = None;
                    if let Ok(result) = result {
                        browse.result = Some((directory, Arc::new(result)));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn browse_groups(&self, browse: &Browse, query: &str) -> Vec<PaletteGroup<Item>> {
        let directory = browse::directory_path(query);
        let leaf = if browse::has_trailing_separator(query) {
            ""
        } else {
            browse::leaf(query)
        };
        let mut items = Vec::new();
        if browse::parent_path(directory).is_some() {
            items.push(
                Item::action(
                    "browse:up",
                    Title::plain(".."),
                    IconName::CornerLeftUp,
                    &[],
                    |window, cx| {
                        cx.defer_in(window, |this, window, cx| {
                            let query = this.query(cx);
                            if let Some(parent) =
                                browse::parent_path(browse::directory_path(&query))
                            {
                                this.set_query(&parent, window, cx);
                            }
                        })
                    },
                )
                .keep_open(),
            );
        }
        if let Some((listed, result)) = &browse.result
            && listed == directory
        {
            for entry in result
                .entries
                .iter()
                .filter(|entry| browse::matches(&entry.name, leaf))
            {
                let name = entry.name.clone();
                items.push(
                    Item::action(
                        format!("browse:{}", entry.full_path),
                        Title::plain(entry.name.clone()),
                        IconName::Folder,
                        &[],
                        move |window, cx| {
                            let name = name.clone();
                            cx.defer_in(window, move |this, window, cx| {
                                let query = this.query(cx);
                                let next = format!("{}{name}/", browse::directory_path(&query));
                                this.set_query(&next, window, cx);
                            })
                        },
                    )
                    .keep_open(),
                );
            }
        }
        vec![PaletteGroup {
            value: "directories",
            label: "Directories",
            items,
        }]
    }

    /// The path Enter adds: the listed directory itself (trailing `/`), an exact entry, or the
    /// typed text (`resolvedAddProjectPath`).
    fn resolved_add_path(&self, query: &str) -> String {
        let Some((listed, result)) = self
            .browse
            .as_ref()
            .and_then(|browse| browse.result.as_ref())
        else {
            return query.trim().to_owned();
        };
        if browse::has_trailing_separator(query) {
            if listed == query {
                return result.parent_path.clone();
            }
            return query.trim().to_owned();
        }
        let leaf = browse::leaf(query);
        result
            .entries
            .iter()
            .find(|entry| entry.name == leaf && listed == browse::directory_path(query))
            .map_or_else(|| query.trim().to_owned(), |entry| entry.full_path.clone())
    }

    fn will_create(&self, query: &str) -> bool {
        let Some(browse) = &self.browse else {
            return false;
        };
        if query.trim().is_empty() || self.highlighted.is_some() || browse.pending.is_some() {
            return false;
        }
        match &browse.result {
            Some((listed, _)) if browse::has_trailing_separator(query) => listed != query,
            Some((listed, result)) => {
                listed != browse::directory_path(query)
                    || !result
                        .entries
                        .iter()
                        .any(|entry| entry.name == browse::leaf(query))
            }
            None => true,
        }
    }

    /// Adds the project at `query` in the browsed environment, or opens it if it exists
    /// (`handleAddProjectForEnvironment`).
    fn add_project_at(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(browse) = &self.browse else {
            return;
        };
        let path = self.resolved_add_path(query);
        let path = path.trim_end_matches('/').to_owned();
        if path.is_empty() {
            return;
        }
        if path.starts_with("./") || path.starts_with("../") {
            toast::show(
                Toast::error("Failed to add project")
                    .description("Relative paths require an active project.")
                    .stacked(),
                cx,
            );
            return;
        }
        let Some(environment) = self.app_state.read(cx).environment(&browse.environment, cx) else {
            return;
        };
        let environment_id = browse.environment.clone();
        let existing = environment
            .read(cx)
            .projects()
            .iter()
            .find(|project| project.workspace_root.trim_end_matches('/') == path)
            .map(|project| project.id.clone());
        self.set_open(false, window, cx);
        if let Some(project_id) = existing {
            open_project(&ProjectRef::new(environment_id, project_id), cx);
            return;
        }
        let project_id = ProjectId::random();
        let title = path
            .rsplit('/')
            .find(|segment| !segment.is_empty())
            .unwrap_or(&path)
            .to_owned();
        let task = environment.read(cx).dispatch(
            commands::create_project(project_id.clone(), title, path, true),
            cx,
        );
        cx.spawn(async move |_, cx| {
            let result = task.await;
            cx.update(|cx| match result {
                Ok(()) => new_thread(ProjectRef::new(environment_id, project_id), None, cx),
                Err(error) => {
                    toast::show(
                        Toast::error("Failed to add project")
                            .description(error.to_string())
                            .stacked(),
                        cx,
                    );
                }
            })
        })
        .detach();
    }
}

/// Asks the draft owner for a new thread in `project`, carrying the active thread's context.
fn new_thread(project: ProjectRef, carry: Option<&OrchestrationThreadShell>, cx: &mut App) {
    let request = NewThreadRequest {
        project,
        branch: carry.and_then(|thread| thread.branch.clone()),
        worktree_path: carry.and_then(|thread| thread.worktree_path.clone()),
        env_mode: None,
        start_from_origin: None,
    };
    AppState::global(cx).update(cx, |state, cx| state.request_new_thread(request, cx));
}

/// Opens the project's latest thread, else a new draft (`openProjectFromSearch`).
fn open_project(project: &ProjectRef, cx: &mut App) {
    let app_state = AppState::global(cx);
    let state = app_state.read(cx);
    let order = state.settings().sidebar_thread_sort_order;
    let latest = state
        .environment(&project.environment_id, cx)
        .and_then(|environment| {
            environment
                .read(cx)
                .threads()
                .iter()
                .filter(|thread| {
                    thread.project_id == project.project_id && thread.archived_at.is_none()
                })
                .min_by(|left, right| compare_threads(left, right, order))
                .map(|thread| thread.id.clone())
        });
    match latest {
        Some(thread) => app_state.update(cx, |state, cx| {
            state.navigate(
                Route::Thread(ThreadRef::new(project.environment_id.clone(), thread)),
                cx,
            )
        }),
        None => new_thread(project.clone(), None, cx),
    }
}

// -----------------------------------------------------------------------------------------------
// Render

/// A key cap with an icon (`Kbd` holding `ArrowUp`).
fn icon_kbd(icon: IconName, cx: &App) -> impl IntoElement {
    let colors = cx.colors();
    div()
        .h_5()
        .min_w_5()
        .px_1()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .bg(colors.muted)
        .text_color(colors.muted_foreground)
        .child(Icon::new(icon).size(px(12.)))
}

impl CommandPalette {
    fn render_item(&self, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let highlighted = self.highlighted.as_ref() == Some(&item.value);
        let shortcut = item
            .shortcut
            .as_ref()
            .and_then(|command| shortcut_label(command, cx));
        let status = item.status.map(|status| {
            let palette = match status {
                t3_logic::sidebar::ThreadStatus::PendingApproval => colors.status.pending_approval,
                t3_logic::sidebar::ThreadStatus::AwaitingInput => colors.status.awaiting_input,
                t3_logic::sidebar::ThreadStatus::Error => colors.status.error,
                t3_logic::sidebar::ThreadStatus::Working => colors.status.working,
                t3_logic::sidebar::ThreadStatus::PlanReady => colors.status.plan_ready,
                t3_logic::sidebar::ThreadStatus::Completed => colors.status.completed,
            };
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_1()
                .type_scale(under_xs(10.))
                .text_color(palette.text)
                .child(div().size(px(6.)).rounded_full().bg(palette.dot))
                .child(status.label())
        });
        let title = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .type_scale(text::SM)
            .text_color(colors.foreground)
            .children(status)
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(div().flex_none().child(item.title.text.clone()))
                    .children(item.title.bold.clone().map(|bold| {
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(bold)
                    })),
            );
        let body = match &item.description {
            Some(description) => div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(title)
                .child(
                    div()
                        .truncate()
                        .type_scale(text::XS)
                        .text_color(colors.muted_foreground_70)
                        .child(description.clone()),
                ),
            None => div().min_w_0().flex_1().flex().items_center().child(title),
        };
        let hover_value = item.value.clone();
        let run = item.clone();
        div()
            .id(item.value.clone())
            .min_h_7()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .px_2()
            .py(px(6.))
            .cursor_pointer()
            .when(highlighted, |this| {
                this.bg(colors.accent).text_color(colors.accent_foreground)
            })
            .when(item.disabled, |this| this.opacity(0.64))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.highlighted.as_ref() != Some(&hover_value) {
                    this.highlighted = Some(hover_value.clone());
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(cx.listener(move |this, _, window, cx| this.execute(run.clone(), window, cx)))
            .child(
                Icon::new(item.icon)
                    .size(px(16.))
                    .color(colors.muted_foreground_80),
            )
            .child(body)
            .children(item.timestamp.clone().map(|timestamp| {
                div()
                    .min_w_12()
                    .flex_none()
                    .flex()
                    .justify_end()
                    .type_scale(under_xs(10.))
                    .text_color(colors.muted_foreground_70)
                    .child(timestamp)
            }))
            .children(shortcut.map(|shortcut| {
                div()
                    .ml_auto()
                    .flex_none()
                    .type_scale(text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground_72)
                    .child(shortcut)
            }))
            .when(matches!(item.kind, ItemKind::Submenu { .. }), |this| {
                this.child(
                    div().ml_auto().flex_none().child(
                        Icon::new(IconName::ChevronRight)
                            .size(px(16.))
                            .color(colors.muted_foreground.opacity(0.5)),
                    ),
                )
            })
            .into_any_element()
    }
}

impl Render for CommandPalette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let colors = cx.colors();
        let query = self.query(cx);
        let groups = self.displayed_groups(cx);
        let browsing = self.browse.is_some();
        let in_submenu = !self.views.is_empty();
        let mode = PaletteMode::new(in_submenu, browsing);
        if self.placeholder_mode != mode {
            self.placeholder_mode = mode;
            self.input.update(cx, |input, cx| {
                input.set_placeholder(mode.placeholder(), window, cx)
            });
        }
        let addon: AnyElement = if in_submenu {
            div()
                .id("palette-back")
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.pop_view(window, cx)))
                .child(Icon::new(IconName::ArrowLeft).size(px(16.)))
                .into_any_element()
        } else if browsing {
            Icon::new(IconName::FolderPlus)
                .size(px(16.))
                .into_any_element()
        } else {
            Icon::new(IconName::Search).size(px(16.)).into_any_element()
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, group) in groups.iter().enumerate() {
            rows.push(
                div()
                    .px_2()
                    .py(px(6.))
                    .when(index > 0, |this| this.mt(px(6.)))
                    .type_scale(text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child(group.label)
                    .into_any_element(),
            );
            for item in &group.items {
                rows.push(self.render_item(item, cx));
            }
        }
        let empty = groups.is_empty().then(|| {
            let message = if browsing && self.will_create(&query) {
                "Press Enter to create this folder and add it as a project."
            } else {
                empty_text(&query)
            };
            div()
                .py_10()
                .flex()
                .justify_center()
                .type_scale(text::SM)
                .text_color(colors.muted_foreground)
                .child(message)
        });
        let add_button = browsing.then(|| {
            let label = if self.will_create(&query) {
                "Create & Add"
            } else {
                "Add"
            };
            div()
                .absolute()
                .right(px(10.))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .child(
                    Button::new("palette-add-project")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Xs)
                        .label(label)
                        .child(Kbd::new(if self.highlighted.is_some() {
                            if cfg!(target_os = "macos") {
                                "⌘ Enter"
                            } else {
                                "Ctrl Enter"
                            }
                        } else {
                            "Enter"
                        }))
                        .on_click(cx.listener(|this, _, window, cx| {
                            let query = this.query(cx);
                            this.add_project_at(&query, window, cx);
                        })),
                )
        });
        let footer_pair = |key: AnyElement, label: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(key)
                .child(div().text_color(colors.muted_foreground_80).child(label))
        };
        let footer = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .border_t_1()
            .border_color(colors.border)
            .px_5()
            .py_3()
            .type_scale(text::XS)
            .text_color(colors.muted_foreground)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icon_kbd(IconName::ArrowUp, cx))
                            .child(icon_kbd(IconName::ArrowDown, cx))
                            .child(
                                div()
                                    .text_color(colors.muted_foreground_80)
                                    .child("Navigate"),
                            ),
                    )
                    .when(!browsing || self.highlighted.is_some(), |this| {
                        this.child(footer_pair(Kbd::new("Enter").into_any_element(), "Select"))
                    })
                    .when(in_submenu, |this| {
                        this.child(footer_pair(
                            Kbd::new("Backspace").into_any_element(),
                            "Back",
                        ))
                    })
                    .child(footer_pair(Kbd::new("Esc").into_any_element(), "Close")),
            );
        let viewport_height = window.viewport_size().height;
        let popup = div()
            .id("command-palette")
            .relative()
            .w_full()
            .max_w(px(576.))
            .max_h(px(420.))
            .flex()
            .flex_col()
            .rounded(px(18.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow(shadow::LG_5.to_vec())
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // `::before`: a `muted/72` wash behind the input row.
            .child(div().absolute().inset_0().bg(colors.muted_72))
            .child(
                div().relative().px(px(10.)).py(px(6.)).child(
                    div()
                        .relative()
                        .h(px(34.))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .absolute()
                                .left(px(2.))
                                .top_0()
                                .bottom_0()
                                .flex()
                                .items_center()
                                .text_color(colors.foreground)
                                .opacity(0.8)
                                .child(addon),
                        )
                        .child(
                            div()
                                .flex_1()
                                .pl(px(21.))
                                .when(browsing, |this| this.pr(px(112.)))
                                .type_scale(text::SM)
                                .child(TextInput::new(&self.input).appearance(false)),
                        )
                        .children(add_button),
                ),
            )
            .child(
                div()
                    .relative()
                    .mx(px(-1.))
                    .flex_1()
                    .min_h_0()
                    .max_h((viewport_height * 0.7).min(px(448.)))
                    .border_1()
                    .border_b_0()
                    .border_color(colors.border)
                    .rounded_t(px(14.))
                    .bg(colors.popover)
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .map(|this| match empty {
                        Some(empty) => this.child(empty),
                        None => this.child(
                            div()
                                .id("command-palette-list")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .track_scroll(&self.scroll)
                                .p_2()
                                .flex()
                                .flex_col()
                                .children(rows),
                        ),
                    }),
            )
            .child(div().relative().bg(colors.popover).child(footer));
        div()
            .id("command-palette-layer")
            .key_context("CommandPalette")
            .track_focus(&self.focus)
            .absolute()
            .inset_0()
            .capture_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .id("command-palette-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(colors.background_60)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| this.set_open(false, window, cx)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .px_4()
                    .py(viewport_height * 0.1)
                    .child(popup),
            )
            .into_any_element()
    }
}
