//! Sidebar context menus and the thread actions behind them (spec 2.9, 2.10, 2.12.3-4).
//!
//! Menus are native (gpui-kit `NativeMenu`). Their items dispatch the actions below to the focused
//! element, so a right-click focuses the sidebar first (a browser focuses a focusable row on any
//! mouse down too) and the sidebar root handles them.

use gpui_kit::{
    Action, App, AppContext as _, ClipboardItem, Context, Entity, Pixels, Point, Window,
    component::{
        Icon as MenuIcon,
        input::{InputEvent, InputState},
        native_menu::NativeMenu,
    },
};
use t3_logic::{ProjectRef, ThreadRef};
use t3_protocol::orchestration::OrchestrationThreadShell;
use t3_ui::IconName;

use super::Sidebar;
use crate::{
    dialogs::confirm,
    state::{AppState, Route},
    toast::{self, Toast},
};

/// An item of the single-thread menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadMenuItem {
    Rename,
    MarkUnread,
    CopyPath,
    CopyThreadId,
    Delete,
}

/// Single-thread menu choice.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = sidebar, no_json)]
pub struct ThreadMenuAction {
    pub thread: ThreadRef,
    pub item: ThreadMenuItem,
}

/// An item of the multi-selection menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMenuItem {
    MarkUnread,
    Delete,
}

/// Multi-selection menu choice.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = sidebar, no_json)]
pub struct SelectionMenuAction {
    pub item: SelectionMenuItem,
}

/// An item of the project menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectMenuItem {
    Rename,
    GroupInto,
    CopyPath,
    Remove,
}

/// Project menu choice for one member project.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = sidebar, no_json)]
pub struct ProjectMenuAction {
    pub member: ProjectRef,
    pub item: ProjectMenuItem,
}

/// Member picked for a new thread in a grouped project.
#[derive(Clone, Debug, PartialEq, Action)]
#[action(namespace = sidebar, no_json)]
pub struct NewThreadInMember {
    pub project: ProjectRef,
}

/// Inline rename state for one thread row.
pub(super) struct Rename {
    pub thread: ThreadRef,
    pub input: Entity<InputState>,
    original: String,
    finished: bool,
    _events: gpui_kit::Subscription,
}

fn trash_icon() -> MenuIcon {
    MenuIcon::default().path(IconName::Trash2.path())
}

/// Appends a destructive item, separated from what precedes it (Electron menu behavior).
fn destructive(
    menu: NativeMenu,
    label: impl Into<gpui_kit::SharedString>,
    action: Box<dyn Action>,
) -> NativeMenu {
    let menu = if menu.is_empty() {
        menu
    } else {
        menu.separator()
    };
    menu.menu_with_icon(label, trash_icon(), action)
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

impl Sidebar {
    fn thread_shell(&self, thread: &ThreadRef) -> Option<std::sync::Arc<OrchestrationThreadShell>> {
        self.model
            .projects
            .iter()
            .flat_map(|project| &project.rendered_threads)
            .find(|row| &row.thread_ref == thread)
            .map(|row| row.thread.clone())
    }

    /// Right-click on a thread row.
    pub(super) fn show_thread_menu(
        &mut self,
        thread: &ThreadRef,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        if !self.selection.is_empty() && self.selection.contains(thread) {
            let count = self.selection.len();
            let menu = NativeMenu::new().menu(
                format!("Mark unread ({count})"),
                Box::new(SelectionMenuAction {
                    item: SelectionMenuItem::MarkUnread,
                }),
            );
            destructive(
                menu,
                format!("Delete ({count})"),
                Box::new(SelectionMenuAction {
                    item: SelectionMenuItem::Delete,
                }),
            )
            .show(position, window, cx);
            return;
        }
        self.clear_selection(cx);
        let item = |item| -> Box<dyn Action> {
            Box::new(ThreadMenuAction {
                thread: thread.clone(),
                item,
            })
        };
        let menu = NativeMenu::new()
            .menu("Rename thread", item(ThreadMenuItem::Rename))
            .menu("Mark unread", item(ThreadMenuItem::MarkUnread))
            .menu("Copy Path", item(ThreadMenuItem::CopyPath))
            .menu("Copy Thread ID", item(ThreadMenuItem::CopyThreadId));
        destructive(menu, "Delete", item(ThreadMenuItem::Delete)).show(position, window, cx);
    }

    /// Right-click on a project header. Grouped rows show each item as a submenu of members.
    pub(super) fn show_project_menu(
        &mut self,
        key: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .model
            .projects
            .iter()
            .find(|project| project.key == key)
        else {
            return;
        };
        window.focus(&self.focus, cx);
        let members: Vec<_> = project
            .members
            .iter()
            .map(|member| {
                (
                    member.picker_label(project.members.len()),
                    member.project_ref.clone(),
                )
            })
            .collect();
        let entry = |menu: NativeMenu, label: &str, item: ProjectMenuItem| {
            let action = |member: &ProjectRef| -> Box<dyn Action> {
                Box::new(ProjectMenuAction {
                    member: member.clone(),
                    item,
                })
            };
            if members.len() == 1 {
                let action = action(&members[0].1);
                return if item == ProjectMenuItem::Remove {
                    destructive(menu, label.to_owned(), action)
                } else {
                    menu.menu(label.to_owned(), action)
                };
            }
            let submenu =
                members
                    .iter()
                    .fold(NativeMenu::new(), |submenu, (member_label, member)| {
                        submenu.menu(member_label.clone(), action(member))
                    });
            let menu = if item == ProjectMenuItem::Remove {
                menu.separator()
            } else {
                menu
            };
            menu.submenu(label.to_owned(), submenu)
        };
        let menu = entry(NativeMenu::new(), "Rename", ProjectMenuItem::Rename);
        let menu = entry(menu, "Group into...", ProjectMenuItem::GroupInto);
        let menu = entry(menu, "Copy Path", ProjectMenuItem::CopyPath);
        entry(menu, "Remove", ProjectMenuItem::Remove).show(position, window, cx);
    }

    /// New thread in a grouped project: pick the member first (spec 2.10).
    pub(super) fn show_member_picker(
        &mut self,
        members: Vec<(String, ProjectRef)>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        members
            .into_iter()
            .fold(NativeMenu::new(), |menu, (label, project)| {
                menu.menu(label, Box::new(NewThreadInMember { project }))
            })
            .show(position, window, cx);
    }

    // -------------------------------------------------------------------------------------------
    // Action handlers (registered on the sidebar root).

    pub(super) fn on_thread_menu(
        &mut self,
        action: &ThreadMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thread = &action.thread;
        match action.item {
            ThreadMenuItem::Rename => self.start_rename(thread, window, cx),
            ThreadMenuItem::MarkUnread => self.mark_unread(std::slice::from_ref(thread), cx),
            ThreadMenuItem::CopyPath => {
                let path = self.thread_shell(thread).and_then(|shell| {
                    shell.worktree_path.clone().or_else(|| {
                        self.model
                            .rendered_thread(thread)
                            .and_then(|row| row.project_root.clone())
                    })
                });
                match path {
                    Some(path) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                        toast::show(Toast::success("Path copied").description(path), cx);
                    }
                    None => {
                        toast::show(
                            Toast::error("Path unavailable")
                                .description("This thread does not have a workspace path to copy.")
                                .stacked(),
                            cx,
                        );
                    }
                }
            }
            ThreadMenuItem::CopyThreadId => {
                let id = thread.thread_id.to_string();
                cx.write_to_clipboard(ClipboardItem::new_string(id.clone()));
                toast::show(Toast::success("Thread ID copied").description(id), cx);
            }
            ThreadMenuItem::Delete => self.delete_threads(vec![thread.clone()], window, cx),
        }
    }

    pub(super) fn on_selection_menu(
        &mut self,
        action: &SelectionMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected: Vec<ThreadRef> = self
            .selection
            .in_order(&self.model.visible_threads)
            .cloned()
            .collect();
        match action.item {
            SelectionMenuItem::MarkUnread => {
                self.mark_unread(&selected, cx);
                self.clear_selection(cx);
            }
            SelectionMenuItem::Delete => self.delete_threads(selected, window, cx),
        }
    }

    pub(super) fn on_project_menu(
        &mut self,
        action: &ProjectMenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let member = self
            .model
            .projects
            .iter()
            .flat_map(|project| &project.members)
            .find(|member| member.project_ref == action.member)
            .cloned();
        let Some(member) = member else {
            return;
        };
        match action.item {
            ProjectMenuItem::CopyPath => {
                let path = member.project.workspace_root.clone();
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                toast::show(Toast::success("Path copied").description(path), cx);
            }
            ProjectMenuItem::Remove => self.remove_project(member, window, cx),
            ProjectMenuItem::Rename => self.open_rename_project(member, window, cx),
            ProjectMenuItem::GroupInto => self.open_project_grouping(member, cx),
        }
    }

    pub(super) fn on_new_thread_in_member(
        &mut self,
        action: &NewThreadInMember,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.new_thread_in(action.project.clone(), cx);
    }

    // -------------------------------------------------------------------------------------------
    // Thread operations

    fn mark_unread(&mut self, threads: &[ThreadRef], cx: &mut Context<Self>) {
        let marks: Vec<(String, Option<String>)> = threads
            .iter()
            .filter_map(|thread| {
                let shell = self.thread_shell(thread)?;
                let completed = shell.latest_turn.as_ref()?.completed_at.clone();
                Some((thread.key(), completed))
            })
            .collect();
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| {
                    marks.iter().fold(false, |changed, (key, completed)| {
                        ui.mark_thread_unread(key, completed.as_deref()) || changed
                    })
                },
                cx,
            )
        });
    }

    /// Archives a thread. Archiving the route thread opens a new draft in its project.
    pub(super) fn archive_thread(
        &mut self,
        thread: &ThreadRef,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.app_state.read(cx);
        let Some(environment) = state.environment(&thread.environment_id, cx) else {
            return;
        };
        let was_active = state.route().thread() == Some(thread);
        let project = self
            .thread_shell(thread)
            .map(|shell| ProjectRef::new(thread.environment_id.clone(), shell.project_id.clone()));
        let task = environment.read(cx).dispatch(
            t3_client::commands::archive_thread(thread.thread_id.clone()),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => {
                    if was_active && let Some(project) = project {
                        this.new_thread_in(project, cx);
                    }
                }
                Err(error) => {
                    toast::show(
                        Toast::error("Failed to archive thread")
                            .description(error.to_string())
                            .stacked(),
                        cx,
                    );
                }
            })
            .ok();
        })
        .detach();
    }

    /// Deletes threads one at a time, after the confirmation when `confirmThreadDelete` is on.
    /// Deleting the route thread moves to the newest remaining thread of its project, else `/`.
    fn delete_threads(
        &mut self,
        threads: Vec<ThreadRef>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if threads.is_empty() {
            return;
        }
        let confirm_first = self.app_state.read(cx).settings().confirm_thread_delete;
        let message = match threads.as_slice() {
            [single] => {
                let title = self
                    .thread_shell(single)
                    .map_or_else(String::new, |shell| shell.title.clone());
                format!(
                    "Delete thread \"{title}\"?\nThis permanently clears conversation history for this thread."
                )
            }
            many => format!(
                "Delete {} thread{}?\nThis permanently clears conversation history for these threads.",
                many.len(),
                plural(many.len())
            ),
        };
        let confirmed = confirm_first.then(|| confirm(&message, window, cx));
        let single = threads.len() == 1;
        cx.spawn(async move |this, cx| {
            if let Some(confirmed) = confirmed
                && !confirmed.await
            {
                return;
            }
            for thread in threads {
                let Ok(task) = this.update(cx, |this, cx| this.dispatch_delete(&thread, cx)) else {
                    return;
                };
                let Some(task) = task else { continue };
                if let Err(error) = task.await {
                    this.update(cx, |_, cx| {
                        let title = if single {
                            "Failed to delete thread"
                        } else {
                            "Failed to delete threads"
                        };
                        toast::show(
                            Toast::error(title).description(error.to_string()).stacked(),
                            cx,
                        );
                    })
                    .ok();
                    return;
                }
                this.update(cx, |this, cx| this.after_delete(&thread, cx))
                    .ok();
            }
        })
        .detach();
    }

    fn dispatch_delete(
        &self,
        thread: &ThreadRef,
        cx: &App,
    ) -> Option<gpui_kit::Task<anyhow::Result<()>>> {
        let environment = self
            .app_state
            .read(cx)
            .environment(&thread.environment_id, cx)?;
        Some(environment.read(cx).dispatch(
            t3_client::commands::delete_thread(thread.thread_id.clone()),
            cx,
        ))
    }

    fn after_delete(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) {
        self.selection.remove(std::slice::from_ref(thread));
        if self.app_state.read(cx).route().thread() != Some(thread) {
            return;
        }
        let fallback = self.model.project_of(thread).and_then(|project| {
            project
                .ordered_threads
                .iter()
                .find(|candidate| *candidate != thread)
                .cloned()
        });
        let route = fallback.map_or(Route::Index, Route::Thread);
        self.app_state
            .update(cx, |state, cx| state.replace_route(route, cx));
    }

    // -------------------------------------------------------------------------------------------
    // Projects

    /// "Remove" (spec 2.9): non-empty projects first warn with a "Delete anyway" toast.
    fn remove_project(
        &mut self,
        member: t3_logic::sidebar::ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = member.project.title.clone();
        let root = member.project.workspace_root.clone();
        let environment_line = member
            .environment_label
            .as_ref()
            .map(|label| format!("Environment: {label}"));
        let count = member.thread_count;
        let mut lines = Vec::new();
        if count > 0 {
            lines.push(format!(
                "Remove project \"{title}\" and delete its {count} thread{}?",
                plural(count)
            ));
        } else {
            lines.push(format!("Remove project \"{title}\"?"));
        }
        lines.push(format!("Path: {root}"));
        lines.extend(environment_line);
        if count > 0 {
            lines.push("This permanently clears conversation history for those threads.".into());
        }
        lines.push("This removes only this project entry.".into());
        if count > 0 {
            lines.push("This action cannot be undone.".into());
        }
        let message = lines.join("\n");
        let project = member.project_ref.clone();
        let run = move |window: &mut Window, cx: &mut App| {
            let confirmed = confirm(&message, window, cx);
            let project = project.clone();
            let title = title.clone();
            window
                .spawn(cx, async move |cx| {
                    if !confirmed.await {
                        return;
                    }
                    let task = cx
                        .update(|_, cx| {
                            let environment = AppState::global(cx)
                                .read(cx)
                                .environment(&project.environment_id, cx)?;
                            Some(environment.read(cx).dispatch(
                                t3_client::commands::remove_project(
                                    project.project_id.clone(),
                                    count > 0,
                                ),
                                cx,
                            ))
                        })
                        .ok()
                        .flatten();
                    let Some(task) = task else { return };
                    if let Err(error) = task.await {
                        cx.update(|_, cx| {
                            toast::show(
                                Toast::error(format!("Failed to remove \"{title}\""))
                                    .description(error.to_string())
                                    .stacked(),
                                cx,
                            );
                        })
                        .ok();
                    }
                })
                .detach();
        };
        if count == 0 {
            run(window, cx);
            return;
        }
        toast::show(
            Toast::warning("Project is not empty")
                .description("Delete all threads in this project before removing it.")
                .stacked()
                .action(
                    "Delete anyway",
                    toast::ToastActionStyle::Destructive,
                    move |window, cx| {
                        // The toast closes, then the confirmation opens after its exit (180ms).
                        let run = run.clone();
                        window
                            .spawn(cx, async move |cx| {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_millis(180))
                                    .await;
                                cx.update(|window, cx| run(window, cx)).ok();
                            })
                            .detach();
                    },
                ),
            cx,
        );
    }

    // -------------------------------------------------------------------------------------------
    // Inline rename (spec 2.12.4)

    /// Replaces the row title with a focused input holding the title, all selected.
    pub(super) fn start_rename(
        &mut self,
        thread: &ThreadRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(shell) = self.thread_shell(thread) else {
            return;
        };
        let original = shell.title.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(original.clone()));
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        let events =
            cx.subscribe_in(
                &input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.commit_rename(window, cx)
                    }
                    _ => {}
                },
            );
        self.rename = Some(Rename {
            thread: thread.clone(),
            input,
            original,
            finished: false,
            _events: events,
        });
        cx.notify();
    }

    /// Escape: ends the rename without saving.
    pub(super) fn cancel_rename(&mut self, cx: &mut Context<Self>) {
        if self.rename.take().is_some() {
            cx.notify();
        }
    }

    fn commit_rename(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.rename.as_mut() else {
            return;
        };
        if rename.finished {
            return;
        }
        rename.finished = true;
        let title = rename.input.read(cx).value().trim().to_owned();
        let thread = rename.thread.clone();
        let original = rename.original.clone();
        self.rename = None;
        cx.notify();
        if title.is_empty() {
            toast::show(Toast::warning("Thread title cannot be empty"), cx);
            return;
        }
        if title == original {
            return;
        }
        let Some(environment) = self
            .app_state
            .read(cx)
            .environment(&thread.environment_id, cx)
        else {
            return;
        };
        let task = environment.read(cx).dispatch(
            t3_client::commands::rename_thread(thread.thread_id.clone(), title),
            cx,
        );
        cx.spawn(async move |_, cx| {
            if let Err(error) = task.await {
                cx.update(|cx| {
                    toast::show(
                        Toast::error("Failed to rename thread")
                            .description(error.to_string())
                            .stacked(),
                        cx,
                    );
                });
            }
        })
        .detach();
    }
}
