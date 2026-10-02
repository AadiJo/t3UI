//! `/`: "No active thread" (`web/components/NoActiveThreadState.tsx`, spec 1.3).

use gpui_kit::{
    Context, Entity, FontWeight, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _,
};
use t3_ui::{
    ActiveColors as _,
    tokens::{layout, text},
};

use super::main_column::collapsed_titlebar_inset;
use crate::{
    chrome::{TypeScale as _, drag_region},
    state::AppState,
};

/// The index route's view.
pub struct IndexView {
    _app_state: Subscription,
}

impl IndexView {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            // Re-render for the collapsed-sidebar header inset.
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for IndexView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let inset = collapsed_titlebar_inset(cx);
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_color(colors.foreground)
            // Header: topbar height, border-bottom, px 20, drag region.
            .child(
                drag_region("index-header", window, cx)
                    .h(layout::TOPBAR_HEIGHT)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .px_5()
                    .when_some(inset, |this, inset| this.pl(inset))
                    .child(
                        div()
                            .type_scale(text::XS)
                            .text_color(colors.muted_foreground_50)
                            .child("No active thread"),
                    ),
            )
            // Empty: flex-1, centered, p 48; inner block max-w 512, px 32, py 48.
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .p_12()
                    .child(
                        div()
                            .w_full()
                            .max_w_128()
                            .px_8()
                            .py_12()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .type_scale(text::XL)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child("Pick a thread to continue"),
                            )
                            .child(
                                div()
                                    .mt_2()
                                    .type_scale(text::SM)
                                    .text_color(colors.muted_foreground.opacity(0.78))
                                    .child("Select an existing thread or create a new one to get started."),
                            ),
                    ),
            )
    }
}
