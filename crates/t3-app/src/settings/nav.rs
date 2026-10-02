//! The settings sidebar nav (`web/components/settings/SettingsSidebarNav.tsx`, spec 3.2). The
//! sidebar renders it in place of the projects list and footer while the route is a settings
//! page.

use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use t3_ui::{ActiveColors as _, Icon, IconName, tokens::text};

use crate::{
    chrome::{TypeScale as _, under_xs},
    state::{AppState, Route, SettingsPage},
};

/// Nav items in order. Diagnostics has no item; General > About links to it.
pub const NAV_ITEMS: [(SettingsPage, IconName); 6] = [
    (SettingsPage::General, IconName::Settings2),
    (SettingsPage::Keybindings, IconName::Keyboard),
    (SettingsPage::Providers, IconName::Bot),
    (SettingsPage::SourceControl, IconName::GitBranch),
    (SettingsPage::Connections, IconName::Link2),
    (SettingsPage::Archived, IconName::Archive),
];

/// Nav items (13px, no fill: the fork's `sidebar-*` colors render nothing) and the footer's
/// "Back" button. Fills the sidebar below its header strip.
#[derive(IntoElement)]
pub struct SettingsNav {
    app_state: Entity<AppState>,
}

impl SettingsNav {
    pub fn new(app_state: Entity<AppState>) -> Self {
        Self { app_state }
    }
}

impl RenderOnce for SettingsNav {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let active = match self.app_state.read(cx).route() {
            Route::Settings(page) => Some(*page),
            _ => None,
        };
        let items = NAV_ITEMS.into_iter().map(|(page, icon)| {
            let is_active = active == Some(page);
            let app_state = self.app_state.clone();
            let hover_fg = colors.foreground.opacity(0.8);
            div()
                .id(page.label())
                .h_7()
                .w_full()
                .px(px(10.))
                .gap(px(10.))
                .flex()
                .items_center()
                .rounded(px(10.))
                .cursor_pointer()
                .type_scale(under_xs(13.))
                .map(|this| {
                    if is_active {
                        this.text_color(colors.foreground)
                            .font_weight(FontWeight::MEDIUM)
                    } else {
                        this.text_color(colors.muted_foreground_70)
                            .hover(move |style| style.text_color(hover_fg))
                    }
                })
                .on_click(move |_, _, cx| {
                    app_state.update(cx, |state, cx| {
                        state.replace_route(Route::Settings(page), cx)
                    });
                })
                .child(Icon::new(icon).size(px(16.)).color(if is_active {
                    colors.foreground
                } else {
                    colors.muted_foreground_60
                }))
                .child(div().truncate().child(page.label()))
        });
        let app_state = self.app_state.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .py_3()
                    .child(div().flex().flex_col().gap_1().children(items)),
            )
            // SidebarSeparator: invisible 1px spacer.
            .child(div().h(px(1.)).mx_2().flex_shrink_0())
            .child(
                div().p_2().flex_shrink_0().child(
                    div()
                        .id("settings-back")
                        .h_7()
                        .w_full()
                        .px_2()
                        .gap_2()
                        .flex()
                        .items_center()
                        .rounded(px(10.))
                        .cursor_pointer()
                        .type_scale(text::XS)
                        .text_color(colors.muted_foreground)
                        .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                        .on_click(move |_, _, cx| {
                            app_state.update(cx, |state, cx| state.go_back(cx));
                        })
                        .child(Icon::new(IconName::ArrowLeft).size(px(16.)))
                        .child("Back"),
                ),
            )
    }
}
