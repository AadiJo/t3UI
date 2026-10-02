//! "Sidebar options" (`ProjectSortMenu`, spec 2.5): project sort, thread sort, visible thread
//! count, and project grouping. Every choice writes a client setting.

use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _, px,
};
use t3_logic::settings::{
    ClientSettings, ProjectGroupingMode, ProjectSortOrder, ThreadPreviewCount, ThreadSortOrder,
};
use t3_ui::{
    ActiveColors as _, Align, DropdownMenu, Icon, IconName, MenuCheckboxItem, MenuGroupLabel,
    MenuSeparator, TooltipExt as _,
    tokens::{radius, text},
};

use crate::{chrome::TypeScale as _, state::AppState};

/// Writes one client setting from a menu handler.
fn update(cx: &mut App, edit: impl FnOnce(&mut ClientSettings) + 'static) {
    AppState::global(cx).update(cx, |state, cx| state.update_settings(edit, cx));
}

/// A radio row: checked when `selected`, choosing it writes `apply`.
fn radio<T: Copy + PartialEq + 'static>(
    id: &'static str,
    label: &'static str,
    value: T,
    selected: T,
    apply: fn(&mut ClientSettings, T),
) -> AnyElement {
    MenuCheckboxItem::new(id, label)
        .checked(value == selected)
        .on_change(move |_, _, cx| update(cx, move |settings| apply(settings, value)))
        .into_any_element()
}

/// The `−` / count / `+` stepper (NumberField 1-15, width 112).
fn preview_count_field(count: ThreadPreviewCount, cx: &App) -> AnyElement {
    let colors = cx.colors();
    let step = |id: &'static str, icon: IconName, delta: i16| {
        let next = (count.get() as i16 + delta)
            .try_into()
            .ok()
            .and_then(ThreadPreviewCount::new);
        div()
            .id(id)
            .h_full()
            .px_2()
            .flex()
            .items_center()
            .text_color(colors.foreground)
            .when(next.is_none(), |this| this.opacity(0.64))
            .when_some(next, |this, next| {
                this.cursor_pointer()
                    .hover(|style| style.bg(colors.accent))
                    .on_click(move |_, _, cx| {
                        update(cx, move |settings| {
                            settings.sidebar_thread_preview_count = next
                        })
                    })
            })
            .child(Icon::new(icon).size(px(14.)))
    };
    div()
        .px_2()
        .pb_1()
        .child(
            div()
                .w(px(112.))
                .h(px(26.))
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded(radius::MD)
                .border_1()
                .border_color(colors.input)
                .child(step("sidebar-preview-minus", IconName::Minus, -1))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .border_x_1()
                        .border_color(colors.input)
                        .type_scale(text::XS)
                        .text_color(colors.foreground)
                        .child(SharedString::from(count.get().to_string())),
                )
                .child(step("sidebar-preview-plus", IconName::Plus, 1)),
        )
        .into_any_element()
}

/// The sort button and its menu.
pub(super) fn sort_menu(settings: &ClientSettings, cx: &App) -> impl IntoElement {
    let colors = cx.colors();
    let project_order = settings.sidebar_project_sort_order;
    let thread_order = settings.sidebar_thread_sort_order;
    let grouping = settings.sidebar_project_grouping_mode;
    let preview = settings.sidebar_thread_preview_count;
    DropdownMenu::new("sidebar-sort-menu")
        .align(Align::End)
        .min_width(px(208.))
        .trigger(move |open| {
            div()
                .id("sidebar-sort-options")
                .h_6()
                .min_w_6()
                .px(px(3.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::MD)
                .cursor_pointer()
                .text_color(colors.muted_foreground_60)
                .when(open, |this| {
                    this.bg(colors.accent).text_color(colors.foreground)
                })
                .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                .tooltip_text("Sidebar options")
                .child(Icon::new(IconName::ArrowUpDown).size(px(14.)))
                .into_any_element()
        })
        .items(move |_, cx| {
            let label = |text: &'static str| MenuGroupLabel::new(text).into_any_element();
            vec![
                label("Sort projects"),
                radio(
                    "sort-projects-updated",
                    "Last user message",
                    ProjectSortOrder::UpdatedAt,
                    project_order,
                    |s, v| s.sidebar_project_sort_order = v,
                ),
                radio(
                    "sort-projects-created",
                    "Created at",
                    ProjectSortOrder::CreatedAt,
                    project_order,
                    |s, v| s.sidebar_project_sort_order = v,
                ),
                radio(
                    "sort-projects-manual",
                    "Manual",
                    ProjectSortOrder::Manual,
                    project_order,
                    |s, v| s.sidebar_project_sort_order = v,
                ),
                label("Sort threads"),
                radio(
                    "sort-threads-updated",
                    "Last user message",
                    ThreadSortOrder::UpdatedAt,
                    thread_order,
                    |s, v| s.sidebar_thread_sort_order = v,
                ),
                radio(
                    "sort-threads-created",
                    "Created at",
                    ThreadSortOrder::CreatedAt,
                    thread_order,
                    |s, v| s.sidebar_thread_sort_order = v,
                ),
                label("Visible threads"),
                preview_count_field(preview, cx),
                MenuSeparator.into_any_element(),
                label("Group projects"),
                radio(
                    "group-repository",
                    ProjectGroupingMode::Repository.label(),
                    ProjectGroupingMode::Repository,
                    grouping,
                    |s, v| s.sidebar_project_grouping_mode = v,
                ),
                radio(
                    "group-repository-path",
                    ProjectGroupingMode::RepositoryPath.label(),
                    ProjectGroupingMode::RepositoryPath,
                    grouping,
                    |s, v| s.sidebar_project_grouping_mode = v,
                ),
                radio(
                    "group-separate",
                    ProjectGroupingMode::Separate.label(),
                    ProjectGroupingMode::Separate,
                    grouping,
                    |s, v| s.sidebar_project_grouping_mode = v,
                ),
            ]
        })
}
