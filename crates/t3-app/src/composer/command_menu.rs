//! The `@` / `$` / `/` command menu floating above the editor (`chat.md` 4.5,
//! `ComposerCommandMenu.tsx`). Rows come from `t3_logic::composer::menu`; this only draws them
//! and forwards hover and clicks. Mouse-down does not take focus from the editor.

use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px, svg,
};
use t3_logic::composer::{
    menu::{self, MenuIcon},
    prompt::TriggerKind,
};
use t3_ui::{ActiveColors as _, Icon, IconName, file_icon, tokens::shadow};

use super::{Composer, style};

impl Composer {
    pub(super) fn render_command_menu(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let Some(trigger) = self.trigger.clone() else {
            return div().into_any_element();
        };
        let items = self.menu_items(cx);
        let active = self.active_menu_item(&items).map(|item| item.id);
        let groups = menu::group_items(&items, &trigger);
        let loading =
            trigger.kind == TriggerKind::Path && self.path_search.loading && !trigger.query.is_empty();
        let muted = style::alpha(colors.muted_foreground, 0.7);
        let group_label = |text: &str| {
            div()
                .px(px(12.))
                .pt(px(8.))
                .pb(px(4.))
                .child(
                    style::tracked_text(&text.to_uppercase(), px(0.8))
                        .text_size(px(10.))
                        .line_height(px(13.33))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(style::alpha(colors.muted_foreground, 0.55)),
                )
        };

        let body: AnyElement = if items.is_empty() {
            div()
                .px(px(20.))
                .py(px(14.))
                .when(trigger.kind == TriggerKind::Skill, |this| {
                    this.child(group_label("Skills").px_0().pt_0())
                })
                .child(
                    div()
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(muted)
                        .child(menu::empty_text(trigger.kind, loading)),
                )
                .into_any_element()
        } else {
            div()
                .id("composer-menu-list")
                .max_h(px(288.))
                .overflow_y_scroll()
                .p(px(8.))
                .children(groups.into_iter().enumerate().map(|(index, group)| {
                    div()
                        .when(index > 0, |this| {
                            this.child(div().mx(px(8.)).my(px(2.)).h(px(1.)).bg(colors.border))
                        })
                        .when_some(group.label, |this, label| this.child(group_label(label)))
                        .children(group.items.into_iter().map(|item| {
                            let is_active = active.as_deref() == Some(item.id.as_str());
                            let id = item.id.clone();
                            let selected = item.clone();
                            let icon: AnyElement = match item.icon() {
                                MenuIcon::Entry { path, directory } => {
                                    if directory {
                                        Icon::new(IconName::Folder)
                                            .size(px(16.))
                                            .color(style::alpha(colors.muted_foreground, 0.8))
                                            .into_any_element()
                                    } else {
                                        file_icon(&path, colors.is_dark).render(px(16.))
                                    }
                                }
                                MenuIcon::Bot => Icon::new(IconName::Bot)
                                    .size(px(16.))
                                    .color(style::alpha(colors.muted_foreground, 0.8))
                                    .into_any_element(),
                                MenuIcon::Cube => div()
                                    .size(px(16.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/composer/cube.svg")
                                            .size(px(14.))
                                            .text_color(style::alpha(colors.muted_foreground, 0.8)),
                                    )
                                    .into_any_element(),
                            };
                            div()
                                .id(SharedString::from(format!("menu-{}", item.id)))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .min_h(px(28.))
                                .px(px(8.))
                                .py(px(6.))
                                .rounded(px(6.))
                                .text_size(px(14.))
                                .line_height(px(20.))
                                .cursor_pointer()
                                .text_color(colors.foreground)
                                .when(is_active, |this| {
                                    this.bg(colors.accent).text_color(colors.accent_foreground)
                                })
                                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                                    let current = this.highlight.as_ref().map(|(id, _)| id.as_str());
                                    if current != Some(id.as_str()) {
                                        this.set_highlight(Some(id.clone()), cx);
                                    }
                                }))
                                .on_mouse_down(MouseButton::Left, |_, window, _| {
                                    window.prevent_default()
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_menu_item(&selected, window, cx)
                                }))
                                .child(div().flex_none().child(icon))
                                .child(
                                    div()
                                        .flex()
                                        .flex_1()
                                        .min_w_0()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(div().flex_none().child(item.label.clone()))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_size(px(12.))
                                                .line_height(px(16.))
                                                .text_color(muted)
                                                .child(item.description.clone()),
                                        ),
                                )
                                .when_some(item.source.clone(), |this, source| {
                                    this.child(
                                        div()
                                            .flex_none()
                                            .pl(px(8.))
                                            .text_size(px(12.))
                                            .text_color(muted)
                                            .child(source),
                                    )
                                })
                        }))
                }))
                .into_any_element()
        };

        let mut menu_shadow = shadow::LG_5.to_vec();
        for layer in &mut menu_shadow {
            layer.color = layer.color.opacity(0.08 / 0.05);
        }
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom_full()
            .mb(px(8.))
            .child(
                div()
                    .id("composer-menu")
                    .occlude()
                    .w_full()
                    .overflow_hidden()
                    .rounded(px(20.))
                    .border_1()
                    .border_color(colors.border_80)
                    .bg(colors.popover.opacity(0.96))
                    .shadow(menu_shadow)
                    .child(body),
            )
            .into_any_element()
    }
}
