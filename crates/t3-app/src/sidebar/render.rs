//! Sidebar layout. Geometry follows spec 2.2-2.14; comments name the Tailwind classes ported.

use gpui_kit::component::input::Input;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, MouseButton, ObjectFit, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _, Transformation,
    Window, div, img, prelude::FluentBuilder as _, px, radians,
};
use t3_logic::{
    keybindings::Command,
    paths::display_basename,
    settings::ProjectSortOrder,
    sidebar::{
        EnvironmentPresence, SidebarProject, SidebarThread, ThreadStatus, pull_request_badge,
    },
    time::format_relative_time,
};
use t3_protocol::orchestration::PullRequestState;
use t3_ui::{
    ActiveColors as _, Colors, Icon, IconName, TooltipExt as _,
    tokens::{StatusColor, layout, radius, text},
};

use super::{
    Sidebar,
    drag::{ProjectDrag, ProjectDragPreview},
    footer::utility_footer,
    pulse_opacity,
    sort_menu::sort_menu,
};
use crate::{
    chrome::{TypeScale as _, drag_region, under_xs},
    keybindings::shortcut_label,
};

/// Per-render inputs of a thread row.
struct RowFrame {
    /// Clock for the relative time.
    now: i64,
    /// Pulsing dots follow the shared clock (false in pinned-clock captures).
    animate: bool,
    /// Jump-hint label while the modifier is held.
    jump: Option<String>,
}

/// `group/project-header` and `group/menu-sub-item` hover groups.
const PROJECT_HEADER_GROUP: &str = "project-header";
const THREAD_ROW_GROUP: &str = "thread-row";

fn status_color(colors: &Colors, status: ThreadStatus) -> StatusColor {
    match status {
        ThreadStatus::PendingApproval => colors.status.pending_approval,
        ThreadStatus::AwaitingInput => colors.status.awaiting_input,
        ThreadStatus::Error => colors.status.error,
        ThreadStatus::Working => colors.status.working,
        ThreadStatus::PlanReady => colors.status.plan_ready,
        ThreadStatus::Completed => colors.status.completed,
    }
}

/// Opacity for a status dot: pulsing statuses follow the shared clock.
fn dot_opacity(status: ThreadStatus, animate: bool) -> f32 {
    if status.pulses() && animate {
        pulse_opacity()
    } else {
        1.0
    }
}

impl Render for Sidebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on_settings = self.app_state.read(cx).route().is_settings();
        div()
            .id("sidebar")
            .key_context("Sidebar")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_thread_menu))
            .on_action(cx.listener(Self::on_selection_menu))
            .on_action(cx.listener(Self::on_project_menu))
            .on_action(cx.listener(Self::on_new_thread_in_member))
            .size_full()
            .flex()
            .flex_col()
            // SidebarChromeHeader: an empty topbar-height drag strip (the brand is hidden).
            .child(
                drag_region("sidebar-header", window, cx)
                    .h(layout::TOPBAR_HEIGHT)
                    .w_full()
                    .flex_shrink_0(),
            )
            .map(|this| {
                if on_settings {
                    // MOUNT POINT: on /settings the settings nav (spec 3.2, owned by
                    // settings/) replaces the projects content, separator, and footer.
                    this.child(div().flex_1().min_h_0())
                } else {
                    this.child(self.render_content(window, cx))
                        // SidebarSeparator: an invisible 1px spacer (spec section 0).
                        .child(div().h(px(1.)).mx_2().flex_shrink_0())
                        .child(utility_footer(&self.app_state, cx))
                }
            })
            .children(self.render_project_dialog(cx))
    }
}

impl Sidebar {
    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let animate = self.app_state.read(cx).clock_is_live();
        let projects: Vec<AnyElement> = self
            .model
            .projects
            .iter()
            .map(|project| self.render_project(project, animate, window, cx))
            .collect();
        let empty = self.model.projects.is_empty();
        div()
            .id("sidebar-content")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    // Search group: px 8, pt 8, pb 4.
                    .child(div().px_2().pt_2().pb_1().child(self.render_search_row(cx)))
                    // Projects group: p 8.
                    .child(
                        div()
                            .p_2()
                            .flex()
                            .flex_col()
                            .child(self.render_projects_header(cx))
                            .child(div().flex().flex_col().gap_1().children(projects))
                            .when(empty, |this| {
                                this.child(
                                    div()
                                        .px_2()
                                        .pt_4()
                                        .w_full()
                                        .flex()
                                        .justify_center()
                                        .type_scale(text::XS)
                                        .text_color(colors.muted_foreground_60)
                                        .child("No projects yet"),
                                )
                            }),
                    ),
            )
    }

    /// The "Search" row that opens the command palette (spec 2.3).
    fn render_search_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let shortcut = shortcut_label(&Command::CommandPaletteToggle, cx);
        div()
            .id("sidebar-search")
            .h_7()
            .w_full()
            .px_2()
            .gap_2()
            .flex()
            .items_center()
            .rounded(radius::LG)
            .cursor_pointer()
            .type_scale(text::XS)
            .text_color(colors.muted_foreground_70)
            .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
            .on_click(cx.listener(|this, _, _, cx| {
                this.app_state.update(cx, |state, cx| {
                    state.dispatch_command(Command::CommandPaletteToggle, cx)
                });
            }))
            .child(
                Icon::new(IconName::Search)
                    .size(px(14.))
                    .color(colors.muted_foreground_70),
            )
            .child(div().flex_1().min_w_0().truncate().child("Search"))
            .when_some(shortcut, |this, label| {
                // Kbd: h 16, px 6, rounded-sm, bg muted, text-[10px] medium muted-foreground.
                this.child(
                    div()
                        .h_4()
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(radius::SM)
                        .bg(colors.muted)
                        .type_scale(under_xs(10.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.muted_foreground)
                        .child(label),
                )
            })
    }

    /// "Projects" label with the sort menu and add-project buttons (spec 2.5).
    fn render_projects_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let icon_button = |id: &'static str, icon: IconName, tooltip: &'static str| {
            div()
                .id(id)
                .h_6()
                .min_w_6()
                .px(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::MD)
                .cursor_pointer()
                .text_color(colors.muted_foreground_60)
                .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                .tooltip_text(tooltip)
                .child(Icon::new(icon).size(px(14.)))
        };
        div()
            .mb_1()
            .pl_2()
            .pr(px(6.))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .type_scale(under_xs(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground_60)
                    .child("PROJECTS"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(sort_menu(self.app_state.read(cx).settings(), cx))
                    .child(
                        icon_button("sidebar-add-project", IconName::FolderPlus, "Add project")
                            .on_click(cx.listener(|this, _, _, cx| {
                                // The palette's add-project flow (spec 4.5) handles this.
                                this.app_state.update(cx, |state, cx| {
                                    state.dispatch_command(Command::Other("project.add".into()), cx)
                                });
                            })),
                    ),
            )
    }

    fn render_project(
        &self,
        project: &SidebarProject,
        animate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let manual = self
            .app_state
            .read(cx)
            .settings()
            .sidebar_project_sort_order
            == ProjectSortOrder::Manual;
        let colors = cx.colors();
        let key = project.key.clone();
        div()
            .id(SharedString::from(format!("project-{}", project.key)))
            .relative()
            .rounded(radius::MD)
            .flex()
            .flex_col()
            .child(self.render_project_header(project, animate, manual, window, cx))
            .when(manual, |this| {
                // Drop target: a 1px `primary/40` ring while a project is dragged over it.
                this.child(
                    div()
                        .id(SharedString::from(format!("project-drop-{}", project.key)))
                        .absolute()
                        .inset_0()
                        .rounded(radius::MD)
                        .drag_over::<ProjectDrag>(move |style, _, _, _| {
                            style.border_1().border_color(colors.primary_40)
                        })
                        .on_drop(cx.listener(move |this, drag: &ProjectDrag, _, cx| {
                            this.drop_project(drag, &key, cx)
                        })),
                )
            })
            .when(project.show_thread_panel, |this| {
                this.child(self.render_thread_list(project, animate, window, cx))
            })
            .into_any_element()
    }

    /// Project header row (spec 2.8).
    fn render_project_header(
        &self,
        project: &SidebarProject,
        animate: bool,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let key = project.key.clone();
        let favicon = self.favicons.read(cx).loaded(
            &project.representative.project_ref.environment_id,
            &project.representative.project.workspace_root,
        );
        // The header is a `button`: a Tab stop that Enter or Space toggles.
        let focus = window
            .use_keyed_state(
                SharedString::from(format!("project-focus-{}", project.key)),
                cx,
                |_, cx| cx.focus_handle().tab_stop(true),
            )
            .read(cx)
            .clone();
        let collapsed_status = (!project.expanded).then_some(project.status).flatten();
        let chevron = |rotated: bool| {
            Icon::new(IconName::ChevronRight)
                .size(px(14.))
                .color(colors.muted_foreground_70)
                .when(rotated, |icon| {
                    icon.transform(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)))
                })
        };
        let leading = div()
            .id(SharedString::from(format!(
                "project-leading-{}",
                project.key
            )))
            .relative()
            .size(px(14.))
            .ml(px(-2.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .map(|this| match collapsed_status {
                Some(status) => {
                    let color = status_color(colors, status);
                    this.child(
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .group_hover(PROJECT_HEADER_GROUP, |style| style.opacity(0.))
                            .child(
                                div()
                                    .size(px(9.))
                                    .rounded_full()
                                    .bg(color.dot)
                                    .opacity(dot_opacity(status, animate)),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .size_full()
                            .opacity(0.)
                            .group_hover(PROJECT_HEADER_GROUP, |style| style.opacity(1.))
                            .child(chevron(false)),
                    )
                    .tooltip_text(status.label())
                }
                None => this.child(chevron(project.expanded)),
            });
        let new_thread_label = shortcut_label(&Command::ChatNewLocal, cx)
            .or_else(|| shortcut_label(&Command::ChatNew, cx))
            .map_or_else(
                || "New thread".to_owned(),
                |label| format!("New thread ({label})"),
            );
        let representative = project.representative.project_ref.clone();
        let multiple_members = project.members.len() > 1;
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

        div()
            .group(PROJECT_HEADER_GROUP)
            .relative()
            .child(
                div()
                    .id(SharedString::from(format!(
                        "project-header-{}",
                        project.key
                    )))
                    .when(manual, |this| {
                        let drag = ProjectDrag {
                            members: project
                                .members
                                .iter()
                                .map(|member| member.physical_key.clone())
                                .collect(),
                            label: project.display_name.clone().into(),
                        };
                        this.on_drag(drag, |drag, _, window, cx| {
                            let width = window.viewport_size().width.min(px(256.)) - px(16.);
                            cx.new(|_| ProjectDragPreview {
                                label: drag.label.clone(),
                                width,
                            })
                        })
                    })
                    .h_7()
                    .w_full()
                    .pl_2()
                    .pr_8()
                    .py(px(6.))
                    .gap_2()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(radius::LG)
                    .cursor_pointer()
                    .type_scale(text::XS)
                    .group_hover(PROJECT_HEADER_GROUP, |style| style.bg(colors.accent))
                    .track_focus(&focus)
                    .on_key_down({
                        let key = project.key.clone();
                        cx.listener(move |this, event: &gpui_kit::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.toggle_project(&key, cx);
                            }
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_project(&key, cx)))
                    .on_mouse_down(MouseButton::Right, {
                        let key = project.key.clone();
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            this.show_project_menu(&key, event.position, window, cx);
                        })
                    })
                    .child(leading)
                    // ProjectFavicon: the folder fallback shows until the favicon loads, and for
                    // projects without one.
                    .map(|this| match favicon {
                        Some(image) => this.child(
                            img(image)
                                .flex_none()
                                .size(px(14.))
                                .rounded(radius::SM)
                                .object_fit(ObjectFit::Contain),
                        ),
                        None => this.child(
                            Icon::new(IconName::Folder)
                                .size(px(14.))
                                .color(colors.muted_foreground_50),
                        ),
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground.opacity(0.9))
                                    .child(project.display_name.clone()),
                            )
                            .when(project.members.len() > 1, |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .type_scale(under_xs(10.))
                                        .text_color(colors.muted_foreground_60)
                                        .child(format!("{} projects", project.members.len())),
                                )
                            }),
                    ),
            )
            .when(
                project.presence == EnvironmentPresence::RemoteOnly,
                |this| {
                    let (icon, tooltip) = if project.all_remote_members_desktop_local {
                        (IconName::Container, "Local sandbox")
                    } else {
                        (IconName::Cloud, "Remote environment")
                    };
                    let tooltip = format!(
                        "{tooltip}: {}",
                        project.remote_environment_labels.join(", ")
                    );
                    this.child(
                        div()
                            .id(SharedString::from(format!("project-env-{}", project.key)))
                            .absolute()
                            .top_1()
                            .right(px(6.))
                            .size_5()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::MD)
                            .text_color(colors.muted_foreground_60)
                            .group_hover(PROJECT_HEADER_GROUP, |style| style.opacity(0.))
                            .tooltip_text(tooltip)
                            .child(Icon::new(icon).size(px(12.))),
                    )
                },
            )
            .child(
                div()
                    .absolute()
                    .top(px(1.))
                    .bottom_0()
                    .right(px(2.))
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover(PROJECT_HEADER_GROUP, |style| style.opacity(1.))
                    .child(
                        self.icon_action_button(
                            SharedString::from(format!("project-new-thread-{}", project.key)),
                            IconName::SquarePen,
                            cx,
                        )
                        .tooltip_text(new_thread_label)
                        .on_click(cx.listener(
                            move |this, event: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                if multiple_members {
                                    this.show_member_picker(
                                        members.clone(),
                                        event.position(),
                                        window,
                                        cx,
                                    );
                                } else {
                                    this.new_thread_in(representative.clone(), cx);
                                }
                            },
                        )),
                    ),
            )
    }

    /// `SIDEBAR_ICON_ACTION_BUTTON_CLASS`: 24px, rounded-md, px 3, muted/60 → foreground.
    fn icon_action_button(
        &self,
        id: SharedString,
        icon: IconName,
        cx: &App,
    ) -> gpui_kit::Stateful<gpui_kit::Div> {
        let colors = cx.colors();
        div()
            .id(id)
            .h_6()
            .min_w_6()
            .px(px(3.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::MD)
            .cursor_pointer()
            .text_color(colors.muted_foreground_60)
            .hover(|style| style.text_color(colors.foreground))
            .child(Icon::new(icon).size(px(14.)))
    }

    /// Thread list under a project (spec 2.11).
    fn render_thread_list(
        &self,
        project: &SidebarProject,
        animate: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let now = self.app_state.read(cx).now_millis();
        let jump_labels = self.jump_labels(cx);
        let rows: Vec<AnyElement> = project
            .rendered_threads
            .iter()
            .map(|row| {
                let jump = jump_labels
                    .iter()
                    .find(|(thread, _)| *thread == &row.thread_ref)
                    .map(|(_, label)| label.clone());
                let frame = RowFrame { now, animate, jump };
                self.render_thread_row(row, &project.ordered_threads, frame, window, cx)
            })
            .collect();
        let small_row = |id: SharedString| {
            div()
                .id(id)
                .h_6()
                .w_full()
                .px_2()
                .flex()
                .items_center()
                .rounded(radius::LG)
                .cursor_pointer()
                .type_scale(under_xs(10.))
                .text_color(colors.muted_foreground_60)
                .hover(|style| {
                    style
                        .bg(colors.accent)
                        .text_color(colors.muted_foreground_80)
                })
        };
        let key = project.key.clone();
        div()
            // SidebarMenuSub: ml 4, w-full, px 6, gap 2, overflow hidden, border-l.
            .ml_1()
            .w_full()
            .px(px(6.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .overflow_hidden()
            .border_l_1()
            .border_color(colors.border)
            .when(project.show_empty, |this| {
                this.child(
                    div()
                        .h_6()
                        .w_full()
                        .px_2()
                        .flex()
                        .items_center()
                        .type_scale(under_xs(10.))
                        .text_color(colors.muted_foreground_60)
                        .child("No threads yet"),
                )
            })
            .children(rows)
            .when(project.expanded && project.has_overflow, |this| {
                let expanded = project.thread_list_expanded;
                let id = SharedString::from(format!(
                    "project-{}-{}",
                    if expanded { "less" } else { "more" },
                    project.key
                ));
                this.child(
                    small_row(id)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_thread_list_expanded(&key, !expanded, cx)
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .when_some(
                                    (!expanded).then_some(project.hidden_status).flatten(),
                                    |this, status| {
                                        this.child(
                                            div()
                                                .size(px(14.))
                                                .flex_shrink_0()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .size(px(9.))
                                                        .rounded_full()
                                                        .bg(status_color(colors, status).dot)
                                                        .opacity(dot_opacity(status, animate)),
                                                ),
                                        )
                                    },
                                )
                                .child(if expanded { "Show less" } else { "Show more" }),
                        ),
                )
            })
    }

    /// Jump-hint labels for the first nine visible threads, while the hints are showing.
    fn jump_labels(&self, cx: &App) -> Vec<(&t3_logic::ThreadRef, String)> {
        if !self.jump_hints_visible {
            return Vec::new();
        }
        self.model
            .visible_threads
            .iter()
            .take(9)
            .zip(1u8..)
            .filter_map(|(thread, index)| {
                shortcut_label(&Command::ThreadJump(index), cx).map(|label| (thread, label))
            })
            .collect()
    }

    /// One thread row (spec 2.12).
    fn render_thread_row(
        &self,
        row: &SidebarThread,
        project_threads: &[t3_logic::ThreadRef],
        frame: RowFrame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let RowFrame {
            now,
            animate,
            jump: jump_label,
        } = frame;
        let colors = cx.colors();
        let dark = colors.is_dark;
        // `div[role=button tabindex=0]`: rows are Tab stops; Enter or Space opens the thread.
        let focus = window
            .use_keyed_state(
                SharedString::from(format!("thread-focus-{}", row.thread_ref.key())),
                cx,
                |_, cx| cx.focus_handle().tab_stop(true),
            )
            .read(cx)
            .clone();
        let selected = self.selection.contains(&row.thread_ref);
        let active = row.active;
        let thread = &row.thread;
        // resolveThreadRowClassName.
        let (background, hover_background) = match (selected, active) {
            (true, true) => (
                colors.primary.opacity(if dark { 0.30 } else { 0.22 }),
                colors.primary.opacity(if dark { 0.36 } else { 0.26 }),
            ),
            (true, false) => (
                colors.primary.opacity(if dark { 0.22 } else { 0.15 }),
                colors.primary.opacity(if dark { 0.28 } else { 0.19 }),
            ),
            (false, true) => (
                colors.accent.opacity(if dark { 0.55 } else { 0.85 }),
                colors.accent.opacity(if dark { 0.70 } else { 1.0 }),
            ),
            (false, false) => (Hsla::transparent_black(), colors.accent),
        };
        let highlighted = selected || active;
        let text_color = if highlighted {
            colors.foreground
        } else {
            colors.muted_foreground
        };
        let running = thread.session.as_ref().is_some_and(|session| {
            session.status == t3_protocol::orchestration::SessionStatus::Running
                && session.active_turn_id.is_some()
        });
        let timestamp = thread
            .latest_user_message_at
            .as_deref()
            .unwrap_or(&thread.updated_at);
        let relative = format_relative_time(timestamp, now);
        let thread_ref = row.thread_ref.clone();
        let ordered = project_threads.to_vec();
        let worktree = thread
            .worktree_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| {
                let name = display_basename(path);
                match &thread.branch {
                    Some(branch) => format!("Worktree: {name} ({branch})"),
                    None => format!("Worktree: {name}"),
                }
            });
        let id_suffix = row.thread_ref.key();
        let renaming = self
            .rename
            .as_ref()
            .filter(|rename| rename.thread == row.thread_ref)
            .map(|rename| rename.input.clone());
        let is_renaming = renaming.is_some();
        let pull_request = Self::vcs_key(row).and_then(|key| {
            let status = self.vcs.read(cx).status(&key)?;
            pull_request_badge(
                thread.branch.as_deref(),
                status.local.as_ref(),
                status.remote.as_ref(),
            )
        });
        let confirm_archive = self.app_state.read(cx).settings().confirm_thread_archive;
        let confirming = !running && self.confirming_archive.as_ref() == Some(&row.thread_ref);

        div()
            .id(SharedString::from(format!("thread-item-{id_suffix}")))
            .group(THREAD_ROW_GROUP)
            .relative()
            .w_full()
            .on_hover({
                let thread_ref = row.thread_ref.clone();
                cx.listener(move |this, hovered: &bool, _, cx| {
                    // Leaving the row cancels a pending archive confirmation.
                    if !hovered && this.confirming_archive.as_ref() == Some(&thread_ref) {
                        this.confirming_archive = None;
                        cx.notify();
                    }
                })
            })
            .child(
                div()
                    .id(SharedString::from(format!("thread-row-{id_suffix}")))
                    .track_focus(&focus)
                    .on_key_down({
                        let thread_ref = row.thread_ref.clone();
                        cx.listener(move |this, event: &gpui_kit::KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                this.navigate_to_thread(&thread_ref, cx);
                            }
                        })
                    })
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .focus_visible(|style| style.border_color(colors.ring))
                    .h_7()
                    .w_full()
                    .px(px(7.))
                    .gap_2()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .rounded(radius::LG)
                    .cursor_pointer()
                    .type_scale(text::XS)
                    .bg(background)
                    .text_color(text_color)
                    .when(active, |this| this.font_weight(FontWeight::MEDIUM))
                    .hover(move |style| style.bg(hover_background).text_color(colors.foreground))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        // Thread rows are selection-safe: the workspace's outside-click clear
                        // must not see this press.
                        cx.stop_propagation()
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        let click_count = match event {
                            ClickEvent::Mouse(_) => event.click_count(),
                            _ => 1,
                        };
                        let modifiers = event.modifiers();
                        if click_count == 2 && !modifiers.modified() && !is_renaming {
                            this.start_rename(&thread_ref, window, cx);
                            return;
                        }
                        this.click_thread(&thread_ref, &ordered, modifiers, click_count, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, {
                        let thread_ref = row.thread_ref.clone();
                        cx.listener(move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            this.show_thread_menu(&thread_ref, event.position, window, cx);
                        })
                    })
                    // Left: status pill + title.
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.))
                            .flex()
                            .items_center()
                            .when_some(pull_request, |this, badge| {
                                let color = match badge.state {
                                    PullRequestState::Merged => colors.status.pr_merged.text,
                                    PullRequestState::Closed => colors.status.pr_closed.text,
                                    _ => colors.status.completed.text,
                                };
                                let url = badge.url.clone();
                                this.child(
                                    div()
                                        .id(SharedString::from(format!("thread-pr-{id_suffix}")))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(radius::SM)
                                        .cursor_pointer()
                                        .text_color(color)
                                        .tooltip_text(badge.tooltip.clone())
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            cx.open_url(&url);
                                        })
                                        .child(Icon::new(IconName::GitPullRequest).size(px(12.))),
                                )
                            })
                            .when_some(row.status, |this, status| {
                                let color = status_color(colors, status);
                                this.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "thread-status-{id_suffix}"
                                        )))
                                        .flex_shrink_0()
                                        .gap_1()
                                        .flex()
                                        .items_center()
                                        .type_scale(under_xs(10.))
                                        .font_weight(FontWeight::NORMAL)
                                        .text_color(color.text)
                                        .tooltip_text(status.label())
                                        .child(
                                            div()
                                                .size(px(6.))
                                                .rounded_full()
                                                .bg(color.dot)
                                                .opacity(dot_opacity(status, animate)),
                                        )
                                        .child(status.label()),
                                )
                            })
                            .map(|this| match renaming {
                                Some(input) => this.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "thread-rename-{id_suffix}"
                                        )))
                                        .flex_1()
                                        .min_w_0()
                                        .border_1()
                                        .border_color(colors.ring)
                                        .rounded(radius::ROUNDED)
                                        .px(px(2.))
                                        .capture_action(cx.listener(
                                            |this,
                                             _: &gpui_kit::component::input::Escape,
                                             _,
                                             cx| {
                                                this.cancel_rename(cx);
                                                cx.stop_propagation();
                                            },
                                        ))
                                        .on_click(|_, _, cx| cx.stop_propagation())
                                        .child(gpui_kit::Styled::h(
                                            Input::new(&input)
                                                .appearance(false)
                                                .px_0()
                                                .py_0()
                                                .type_scale(text::XS),
                                            px(16.),
                                        )),
                                ),
                                None => this.child(
                                    div()
                                        .id(SharedString::from(format!("thread-title-{id_suffix}")))
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .tooltip_text(thread.title.clone())
                                        .child(thread.title.clone()),
                                ),
                            }),
                    )
                    // Right: worktree hint + meta.
                    .child(
                        div()
                            .ml_auto()
                            .flex_shrink_0()
                            .gap(px(6.))
                            .flex()
                            .items_center()
                            .when_some(worktree, |this, tooltip| {
                                this.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "thread-worktree-{id_suffix}"
                                        )))
                                        .flex()
                                        .items_center()
                                        .tooltip_text(tooltip)
                                        .child(
                                            Icon::new(IconName::FolderGit2)
                                                .size(px(12.))
                                                .color(colors.muted_foreground.opacity(0.4)),
                                        ),
                                )
                            })
                            .child(
                                div().min_w_12().flex().justify_end().child(
                                    div()
                                        .when(confirming, |this| this.opacity(0.))
                                        .when(!running, |this| {
                                            this.group_hover(THREAD_ROW_GROUP, |style| {
                                                style.opacity(0.)
                                            })
                                        })
                                        .map(|this| match jump_label {
                                            Some(label) => this.child(jump_pill(label, colors)),
                                            None => this.child(
                                                div()
                                                    .type_scale(under_xs(10.))
                                                    .font_weight(FontWeight::NORMAL)
                                                    .text_color(if highlighted {
                                                        colors.foreground.opacity(if dark {
                                                            0.82
                                                        } else {
                                                            0.72
                                                        })
                                                    } else {
                                                        colors.muted_foreground.opacity(0.4)
                                                    })
                                                    .child(relative),
                                            ),
                                        }),
                                ),
                            ),
                    ),
            )
            .map(|this| {
                let thread_ref = row.thread_ref.clone();
                if confirming {
                    // "Confirm" pill (confirmThreadArchive): a second click archives.
                    return this.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_1()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "thread-archive-confirm-{id_suffix}"
                                    )))
                                    .h_5()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .rounded(radius::MD)
                                    .bg(colors.destructive.opacity(0.12))
                                    .hover(|style| style.bg(colors.destructive.opacity(0.18)))
                                    .type_scale(under_xs(10.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.destructive)
                                    .cursor_pointer()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.confirming_archive = None;
                                        this.archive_thread(&thread_ref, window, cx);
                                    }))
                                    .child("Confirm"),
                            ),
                    );
                }
                if running {
                    return this;
                }
                // Archive control: fades in over the meta on row hover.
                let tooltip = (!confirm_archive).then_some("Archive");
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right(px(2.))
                        .flex()
                        .items_center()
                        .opacity(0.)
                        .group_hover(THREAD_ROW_GROUP, |style| style.opacity(1.))
                        .child(
                            self.icon_action_button(
                                SharedString::from(format!("thread-archive-{id_suffix}")),
                                IconName::Archive,
                                cx,
                            )
                            .when_some(tooltip, |this, tooltip| this.tooltip_text(tooltip))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    if confirm_archive {
                                        this.confirming_archive = Some(thread_ref.clone());
                                        cx.notify();
                                    } else {
                                        this.archive_thread(&thread_ref, window, cx);
                                    }
                                },
                            )),
                        ),
                )
            })
            .into_any_element()
    }
}

/// Jump-hint pill (spec 2.12.3): h 20, rounded-full, border/80, background/90, mono 10px medium.
fn jump_pill(label: String, colors: &Colors) -> impl IntoElement {
    div()
        .h_5()
        .px(px(6.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(colors.border_80)
        .bg(colors.background.opacity(0.9))
        .font_family(t3_ui::tokens::font::MONO)
        .type_scale(under_xs(10.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.foreground)
        .child(label)
}
