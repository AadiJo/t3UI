//! Manual project ordering by drag and drop (spec 2.15). Active only with the "Manual" project
//! sort. The header row is the handle; dropping on another row moves every member of the
//! dragged row to the target's position in `projectOrder`.

use gpui_kit::{
    Context, FontWeight, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Window, div, px,
};
use t3_ui::{
    ActiveColors as _, Icon, IconName,
    tokens::{radius, text},
};

use super::Sidebar;
use crate::chrome::TypeScale as _;

/// What a project drag carries.
#[derive(Clone)]
pub(super) struct ProjectDrag {
    /// Physical keys of the dragged row's members.
    pub members: Vec<String>,
    pub label: SharedString,
}

/// The floating copy of the dragged header (opacity .8, like the dragged item in the web).
pub(super) struct ProjectDragPreview {
    pub label: SharedString,
    pub width: gpui_kit::Pixels,
}

impl Render for ProjectDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .w(self.width)
            .h_7()
            .pl_2()
            .gap_2()
            .flex()
            .items_center()
            .rounded(radius::LG)
            .bg(colors.accent)
            .opacity(0.8)
            .type_scale(text::XS)
            .child(div().size(px(14.)))
            .child(
                Icon::new(IconName::Folder)
                    .size(px(14.))
                    .color(colors.muted_foreground_50),
            )
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground.opacity(0.9))
                    .child(self.label.clone()),
            )
    }
}

impl Sidebar {
    /// Drop handler: reorders `projectOrder` so the dragged members land at the target row.
    pub(super) fn drop_project(
        &mut self,
        drag: &ProjectDrag,
        target_key: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .model
            .projects
            .iter()
            .find(|project| project.key == target_key)
        else {
            return;
        };
        let target_members: Vec<String> = target
            .members
            .iter()
            .map(|member| member.physical_key.clone())
            .collect();
        let order = self.model.project_order_keys.clone();
        let dragged = drag.members.clone();
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| ui.reorder_projects(&order, &dragged, &target_members),
                cx,
            )
        });
    }
}
