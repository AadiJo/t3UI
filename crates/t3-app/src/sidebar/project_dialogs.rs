//! The project "Rename" and "Group into..." dialogs (spec 2.9).

use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window,
    component::input::{InputEvent, InputState},
    div,
};
use t3_logic::{settings::ProjectGroupingMode, sidebar::ProjectMember};
use t3_ui::{
    ActiveColors as _, Button, ButtonVariant, Dialog, DialogDescription, DialogFooter,
    DialogHeader, DialogPanel, DialogTitle, Input, Select, tokens::text,
};

use super::Sidebar;
use crate::{
    chrome::TypeScale as _,
    toast::{self, Toast},
};

/// The open project dialog, if any.
pub(super) enum ProjectDialog {
    Rename {
        member: ProjectMember,
        input: Entity<InputState>,
        _events: gpui_kit::Subscription,
    },
    Grouping {
        member: ProjectMember,
        /// `None` inherits the global mode.
        selection: Option<ProjectGroupingMode>,
    },
}

const INHERIT: &str = "inherit";

fn mode_value(mode: ProjectGroupingMode) -> &'static str {
    match mode {
        ProjectGroupingMode::Repository => "repository",
        ProjectGroupingMode::RepositoryPath => "repository_path",
        ProjectGroupingMode::Separate => "separate",
    }
}

fn mode_from_value(value: &str) -> Option<ProjectGroupingMode> {
    [
        ProjectGroupingMode::Repository,
        ProjectGroupingMode::RepositoryPath,
        ProjectGroupingMode::Separate,
    ]
    .into_iter()
    .find(|mode| mode_value(*mode) == value)
}

fn mode_help(mode: ProjectGroupingMode) -> &'static str {
    match mode {
        ProjectGroupingMode::Repository => {
            "Projects from the same repository share one sidebar row."
        }
        ProjectGroupingMode::RepositoryPath => {
            "Projects group only when both the repository and repo-relative path match."
        }
        ProjectGroupingMode::Separate => "Every project path gets its own sidebar row.",
    }
}

impl Sidebar {
    /// Opens "Rename project" for a row's representative project (also used by snapshot
    /// scenes). No-op for an unknown key.
    pub fn open_project_rename(
        &mut self,
        project_key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let member = self
            .model
            .projects
            .iter()
            .find(|project| project.key == project_key)
            .map(|project| project.representative.clone());
        if let Some(member) = member {
            self.open_rename_project(member, window, cx);
        }
    }

    pub(super) fn open_rename_project(
        &mut self,
        member: ProjectMember,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = member.project.title.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        input.update(cx, |input, cx| input.focus(window, cx));
        let events = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.submit_rename_project(window, cx);
            }
        });
        self.project_dialog = Some(ProjectDialog::Rename {
            member,
            input,
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn open_project_grouping(&mut self, member: ProjectMember, cx: &mut Context<Self>) {
        let selection = self
            .app_state
            .read(cx)
            .settings()
            .sidebar_project_grouping_overrides
            .get(&member.physical_key)
            .copied();
        self.project_dialog = Some(ProjectDialog::Grouping { member, selection });
        cx.notify();
    }

    fn close_project_dialog(&mut self, cx: &mut Context<Self>) {
        if self.project_dialog.take().is_some() {
            cx.notify();
        }
    }

    /// Save: empty titles warn and keep the dialog open; unchanged titles just close.
    fn submit_rename_project(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(ProjectDialog::Rename { member, input, .. }) = &self.project_dialog else {
            return;
        };
        let title = input.read(cx).value().trim().to_owned();
        if title.is_empty() {
            toast::show(Toast::warning("Project title cannot be empty"), cx);
            return;
        }
        let member = member.clone();
        if title == member.project.title {
            self.close_project_dialog(cx);
            return;
        }
        let Some(environment) = self
            .app_state
            .read(cx)
            .environment(&member.project_ref.environment_id, cx)
        else {
            return;
        };
        let command = t3_protocol::commands::ClientCommand::ProjectMetaUpdate {
            command_id: t3_protocol::CommandId::random(),
            project_id: member.project_ref.project_id.clone(),
            patch: t3_protocol::commands::ProjectMetaPatch {
                title: Some(title),
                ..Default::default()
            },
        };
        let task = environment.read(cx).dispatch(command, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.close_project_dialog(cx),
                Err(error) => {
                    toast::show(
                        Toast::error("Failed to rename project")
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

    fn save_project_grouping(&mut self, cx: &mut Context<Self>) {
        let Some(ProjectDialog::Grouping { member, selection }) = &self.project_dialog else {
            return;
        };
        let key = member.physical_key.clone();
        let selection = *selection;
        self.app_state.update(cx, |state, cx| {
            state.update_settings(
                |settings| match selection {
                    Some(mode) => {
                        settings
                            .sidebar_project_grouping_overrides
                            .insert(key, mode);
                    }
                    None => {
                        settings.sidebar_project_grouping_overrides.remove(&key);
                    }
                },
                cx,
            )
        });
        self.close_project_dialog(cx);
    }

    /// The open dialog, rendered into the sidebar tree (it draws in a full-window layer).
    pub(super) fn render_project_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.colors();
        let dialog = self.project_dialog.as_ref()?;
        let on_open_change = cx.listener(|this, open: &bool, _, cx| {
            if !open {
                this.close_project_dialog(cx);
            }
        });
        let on_open_change = move |open: bool, window: &mut Window, cx: &mut gpui_kit::App| {
            on_open_change(&open, window, cx)
        };
        let label = |text: &'static str| {
            div()
                .type_scale(text::XS)
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(text)
        };
        let environment_line = |member: &ProjectMember| {
            member.environment_label.as_ref().map(|label| {
                div()
                    .type_scale(text::XS)
                    .text_color(colors.muted_foreground)
                    .child(format!("Environment: {label}"))
            })
        };
        let footer = |save: Button| {
            DialogFooter::new()
                .child(
                    Button::new("project-dialog-cancel")
                        .variant(ButtonVariant::Outline)
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| this.close_project_dialog(cx))),
                )
                .child(save)
        };
        let element = match dialog {
            ProjectDialog::Rename { member, input, .. } => Dialog::new("rename-project")
                .open(true)
                .on_open_change(on_open_change)
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new("Rename project"))
                        .child(DialogDescription::new(format!(
                            "Update the title for {}.",
                            member.project.workspace_root
                        ))),
                )
                .child(
                    DialogPanel::new().after_header().child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(label("Project title"))
                            .child(Input::new(input))
                            .children(environment_line(member)),
                    ),
                )
                .child(footer(
                    Button::new("project-dialog-save").label("Save").on_click(
                        cx.listener(|this, _, window, cx| this.submit_rename_project(window, cx)),
                    ),
                ))
                .into_any_element(),
            ProjectDialog::Grouping { member, selection } => {
                let global = self
                    .app_state
                    .read(cx)
                    .settings()
                    .sidebar_project_grouping_mode;
                let effective = selection.unwrap_or(global);
                let inherit_label =
                    SharedString::from(format!("Use global default ({})", global.label()));
                let items = [
                    (SharedString::from(INHERIT), inherit_label),
                    (
                        mode_value(ProjectGroupingMode::Repository).into(),
                        ProjectGroupingMode::Repository.label().into(),
                    ),
                    (
                        mode_value(ProjectGroupingMode::RepositoryPath).into(),
                        ProjectGroupingMode::RepositoryPath.label().into(),
                    ),
                    (
                        mode_value(ProjectGroupingMode::Separate).into(),
                        ProjectGroupingMode::Separate.label().into(),
                    ),
                ];
                let value = SharedString::from(selection.map_or(INHERIT, mode_value));
                Dialog::new("project-grouping")
                    .open(true)
                    .on_open_change(on_open_change)
                    .child(
                        DialogHeader::new()
                            .child(DialogTitle::new("Project grouping"))
                            .child(DialogDescription::new(format!(
                                "Choose how {} should be grouped in the sidebar.",
                                member.project.workspace_root
                            ))),
                    )
                    .child(
                        DialogPanel::new().after_header().child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(label("Grouping rule"))
                                .child(
                                    div().w_full().child(
                                        Select::new("project-grouping-rule")
                                            .items(items)
                                            .value(Some(value))
                                            .on_change(cx.listener(
                                                |this, value: &SharedString, _, cx| {
                                                    if let Some(ProjectDialog::Grouping {
                                                        selection,
                                                        ..
                                                    }) = &mut this.project_dialog
                                                    {
                                                        *selection = mode_from_value(value);
                                                        cx.notify();
                                                    }
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    div()
                                        .type_scale(text::XS)
                                        .text_color(colors.muted_foreground)
                                        .child(mode_help(effective)),
                                )
                                .children(environment_line(member)),
                        ),
                    )
                    .child(footer(
                        Button::new("project-dialog-save")
                            .label("Save")
                            .on_click(cx.listener(|this, _, _, cx| this.save_project_grouping(cx))),
                    ))
                    .into_any_element()
            }
        };
        Some(element)
    }
}
