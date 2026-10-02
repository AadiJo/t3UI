//! The thread error banner under the header (`ThreadErrorBanner.tsx`, spec 1.1).

use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    div, prelude::FluentBuilder as _, px,
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Icon, IconName, TooltipExt as _,
};

use super::{ChatTarget, ChatView};

/// Width of the box the fork squeezes the message into (see [`ChatView::render_error_banner`]).
const SQUEEZED_TEXT_WIDTH: f32 = 16.;

impl ChatView {
    /// The local send error, else the session's last error (server threads).
    fn thread_error(&self) -> Option<String> {
        if let Some(error) = &self.local_error {
            return Some(error.clone());
        }
        if !matches!(self.target, ChatTarget::Thread(_)) {
            return None;
        }
        self.orchestration_thread()?
            .session
            .as_ref()?
            .last_error
            .clone()
    }

    /// The error alert, reproducing how the fork actually renders it: `Alert` files the
    /// description's tooltip wrapper under its icon slot, so the message lands in the 16px icon
    /// box next to the (collapsed) icon. The box shrink-wraps to ~78px, each word gets its own
    /// line clipped at 16px, three lines at most, the third with an ellipsis, overflowing the
    /// box vertically. The full message is in the tooltip. Dismiss clears only a local error.
    pub(super) fn render_error_banner(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let error = self.thread_error()?;
        let colors = cx.colors();
        let words: Vec<&str> = error.split_whitespace().collect();
        let lines = words.iter().take(3).enumerate().map(|(index, word)| {
            let last = index == 2 && words.len() > 3;
            div()
                .w(px(SQUEEZED_TEXT_WIDTH))
                .overflow_hidden()
                .whitespace_nowrap()
                .when(last, |this| this.text_ellipsis())
                .child(word.to_string())
        });
        let text = div()
            .id("thread-error-text")
            .flex_none()
            .flex()
            .flex_col()
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(colors.muted_foreground)
            .children(lines)
            .tooltip_text(error.clone());
        Some(
            div()
                .pt(px(12.))
                .flex()
                .justify_center()
                .child(
                    div()
                        .rounded(px(14.))
                        .border_1()
                        .border_color(colors.destructive_32)
                        .bg(colors.destructive_4)
                        .px(px(14.))
                        .py(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .size(px(16.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(text),
                        )
                        .child(
                            Button::new("thread-error-dismiss")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconXs)
                                .child(
                                    Icon::new(IconName::X)
                                        .size(px(14.))
                                        .color(colors.destructive),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.local_error = None;
                                    cx.notify();
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
