//! The header panel above the editor while the agent waits on the user: a pending approval
//! (`ComposerPendingApprovalPanel.tsx`) or questions (`ComposerPendingUserInputPanel.tsx`).
//! Both sit in a `rounded-t-[19px] border-b border/65 bg-muted/20` strip.

use gpui_kit::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, prelude::FluentBuilder as _,
    px,
};
use t3_logic::composer::pending;
use t3_ui::{ActiveColors as _, Icon, IconName};

use super::{Composer, style};

impl Composer {
    pub(super) fn render_header_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let body = if let Some(approval) = self.pending.approvals.first() {
            self.render_approval_panel(
                approval.request_kind.as_str(),
                self.pending.approvals.len(),
                cx,
            )
        } else {
            self.render_question_panel(cx)
        };
        div()
            .rounded_t(px(19.))
            .border_b_1()
            .border_color(style::alpha(colors.border, 0.65))
            .bg(colors.muted.opacity(0.2))
            .child(body)
            .into_any_element()
    }

    fn render_approval_panel(&self, kind: &str, count: usize, cx: &Context<Self>) -> AnyElement {
        let colors = cx.colors();
        div()
            .px(px(20.))
            .py(px(16.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(colors.foreground)
                    .child(style::tracked_text("PENDING APPROVAL", px(2.8)))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(pending::approval_summary(kind)),
                    )
                    .when(count > 1, |this| {
                        this.child(
                            div()
                                .text_size(px(12.))
                                .line_height(px(16.))
                                .text_color(colors.muted_foreground)
                                .child(format!("1/{count}")),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_question_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let Some(input) = self.pending.user_inputs.first().cloned() else {
            return div().into_any_element();
        };
        let index = self
            .question
            .index
            .min(input.questions.len().saturating_sub(1));
        let Some(question) = input.questions.get(index).cloned() else {
            return div().into_any_element();
        };
        let responding = self.responding.contains(&input.request_id);
        let draft = self
            .question
            .answers
            .get(&question.id)
            .cloned()
            .unwrap_or_default();
        let custom_active = !draft.custom.trim().is_empty();
        div()
            .px(px(20.))
            .py(px(12.))
            .child(
                div()
                    .mb(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        style::tracked_text(&question.header.to_uppercase(), px(1.1))
                            .text_size(px(11.))
                            .line_height(px(14.66))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(style::alpha(colors.muted_foreground, 0.55)),
                    )
                    .when(input.questions.len() > 1, |this| {
                        this.child(
                            div()
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .rounded(px(8.))
                                .bg(colors.muted.opacity(0.6))
                                .px(px(6.))
                                .text_size(px(10.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(style::alpha(colors.muted_foreground, 0.6))
                                .child(format!("{}/{}", index + 1, input.questions.len())),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(style::alpha(colors.foreground, 0.9))
                    .child(question.question.clone()),
            )
            .when(question.multi_select, |this| {
                this.child(
                    div()
                        .mt(px(4.))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .text_color(style::alpha(colors.muted_foreground, 0.65))
                        .child("Select one or more options."),
                )
            })
            .child(
                div().mt(px(12.)).flex().flex_col().gap(px(6.)).children(
                    question
                        .options
                        .iter()
                        .enumerate()
                        .map(|(option_index, option)| {
                            let selected = !custom_active && draft.selected.contains(&option.label);
                            let label = option.label.clone();
                            div()
                                .id(SharedString::from(format!(
                                    "question-option-{option_index}"
                                )))
                                .group("question-option")
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .w_full()
                                .rounded(px(10.))
                                .border_1()
                                .px(px(12.))
                                .py(px(8.))
                                .map(|this| {
                                    if selected {
                                        this.border_color(colors.primary.opacity(0.3))
                                            .bg(colors.primary.opacity(0.08))
                                            .text_color(colors.foreground)
                                    } else {
                                        this.border_color(gpui_kit::transparent_black())
                                            .bg(colors.muted.opacity(0.22))
                                            .text_color(style::alpha(colors.foreground, 0.85))
                                            .hover(|style| {
                                                style
                                                    .border_color(colors.border.opacity(0.45))
                                                    .bg(colors.muted.opacity(0.34))
                                            })
                                    }
                                })
                                .when(responding, |this| this.opacity(0.5))
                                .when(!responding, |this| {
                                    this.cursor_pointer().on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            // Focus leaves the editor, so 1-9 pick options next.
                                            this.focus_handle.focus(window, cx);
                                            this.toggle_question_option(label.clone(), cx)
                                        },
                                    ))
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.))
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .line_height(px(20.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .child(option.label.clone()),
                                        )
                                        .when(option.description != option.label, |this| {
                                            this.child(
                                                div()
                                                    .text_size(px(12.))
                                                    .line_height(px(16.))
                                                    .text_color(style::alpha(
                                                        colors.muted_foreground,
                                                        0.5,
                                                    ))
                                                    .child(option.description.clone()),
                                            )
                                        }),
                                )
                                .map(|this| {
                                    if selected {
                                        this.child(
                                            Icon::new(IconName::Check)
                                                .size(px(14.))
                                                .color(colors.primary),
                                        )
                                    } else if option_index < 9 {
                                        this.child(
                                            div()
                                                .size(px(20.))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(4.))
                                                .border_1()
                                                .border_color(colors.border.opacity(0.5))
                                                .bg(colors.background.opacity(0.35))
                                                .text_size(px(11.))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(style::alpha(
                                                    colors.muted_foreground,
                                                    0.7,
                                                ))
                                                .child((option_index + 1).to_string()),
                                        )
                                    } else {
                                        this
                                    }
                                })
                        }),
                ),
            )
            .into_any_element()
    }
}
