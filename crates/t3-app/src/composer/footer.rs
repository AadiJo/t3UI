//! The composer footer (`chat.md` 4.11): model picker and traits on the left, the plan toggle,
//! then the context meter and the primary action (send, stop, or question navigation) on the
//! right. With a pending approval the footer is the four approval buttons instead.

use std::{cell::Cell, f32::consts::PI};

use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, PathBuilder, Pixels, Point, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, canvas, div, point,
    prelude::FluentBuilder as _, px, svg,
};
use t3_logic::composer::{
    pending::{self, ContextWindow},
    send,
};
use t3_protocol::orchestration::ApprovalDecision;
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Icon, IconName, Spinner,
    TooltipExt as _, tokens::shadow,
};

use super::{Composer, ComposerEvent, style};

/// Form width under which the footer collapses traits and the plan toggle into a menu.
const COMPACT_WIDTH: f32 = 620.;
/// The same threshold while the footer shows wide actions (question navigation).
const COMPACT_WIDTH_WIDE_ACTIONS: f32 = 780.;

thread_local! {
    /// Last measured composer form width, shared across composers (only one renders at a time).
    static FORM_WIDTH: Cell<f32> = const { Cell::new(768.) };
}

impl Composer {
    /// Whether the footer is compact at the current form width (`composerFooterLayout.ts`).
    fn footer_compact(&self) -> (bool, bool) {
        let width = FORM_WIDTH.with(Cell::get);
        let wide_actions = !self.pending.user_inputs.is_empty();
        let footer = width < if wide_actions {
            COMPACT_WIDTH_WIDE_ACTIONS
        } else {
            COMPACT_WIDTH
        };
        let actions = wide_actions && width < COMPACT_WIDTH_WIDE_ACTIONS;
        (footer, actions)
    }

    /// Records the form width after layout; re-renders when it crosses a threshold.
    pub(super) fn measure_form(&self, cx: &mut Context<Self>) -> impl Fn(gpui_kit::Bounds<Pixels>, &mut Window, &mut App) + 'static {
        let entity = cx.entity_id();
        move |bounds, _, cx| {
            let width = f32::from(bounds.size.width);
            let previous = FORM_WIDTH.with(|cell| cell.replace(width));
            let crossed = |threshold: f32| (previous < threshold) != (width < threshold);
            if crossed(COMPACT_WIDTH) || crossed(COMPACT_WIDTH_WIDE_ACTIONS) {
                cx.notify(entity);
            }
        }
    }

    pub(super) fn render_footer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(approval) = self.pending.approvals.first() {
            let request_id = approval.request_id.clone();
            let responding = self.responding.contains(&request_id);
            let button = |id: &'static str, label: &'static str, variant: ButtonVariant, decision: ApprovalDecision, cx: &mut Context<Self>| {
                let request_id = request_id.clone();
                Button::new(id)
                    .variant(variant)
                    .size(ButtonSize::Sm)
                    .label(label)
                    .disabled(responding)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.respond_to_approval(request_id.clone(), decision.clone(), cx)
                    }))
            };
            return div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(8.))
                .px(px(12.))
                .pb(px(12.))
                .child(button("approval-cancel", "Cancel turn", ButtonVariant::Ghost, ApprovalDecision::Cancel, cx))
                .child(button("approval-decline", "Decline", ButtonVariant::DestructiveOutline, ApprovalDecision::Decline, cx))
                .child(button(
                    "approval-session",
                    "Always allow this session",
                    ButtonVariant::Outline,
                    ApprovalDecision::AcceptForSession,
                    cx,
                ))
                .child(button("approval-accept", "Approve once", ButtonVariant::Default, ApprovalDecision::Accept, cx))
                .into_any_element();
        }

        let (compact, actions_compact) = self.footer_compact();
        let has_question = !self.pending.user_inputs.is_empty();
        let traits = self.render_traits(compact, window, cx);
        let plan = self.plan_toggle(cx);
        let left = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(4.))
            .child(self.render_model_trigger(compact, window, cx))
            .map(|this| {
                if compact && (traits.is_some() || plan.is_some()) {
                    this.child(self.render_compact_menu(traits.is_some(), plan.clone(), window, cx))
                } else {
                    this.when_some(traits, |this, traits| {
                        this.child(div().mx(px(2.)).child(style::vertical_rule(px(16.), cx)))
                            .child(traits)
                    })
                    .when_some(plan, |this, (label, open)| {
                        this.child(div().mx(px(2.)).child(style::vertical_rule(px(16.), cx)))
                            .child(self.render_plan_toggle(label, open, cx))
                    })
                }
            });

        div()
            .flex()
            .flex_nowrap()
            .items_center()
            .justify_between()
            .px(px(12.))
            .pb(px(12.))
            .when(has_question, |this| this.pt(px(8.)))
            .when(compact, |this| this.gap(px(6.)))
            .child(left)
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .when_some(self.context_window, |this, usage| {
                        this.child(self.render_context_meter(usage, cx))
                    })
                    .child(self.render_primary_action(actions_compact, cx)),
            )
            .into_any_element()
    }

    /// `(label, open)` when the plan toggle shows: an active plan or a proposed plan in the
    /// latest turn, or the plan panel is open.
    pub(super) fn plan_toggle(&self, _: &App) -> Option<(SharedString, bool)> {
        let thread = self.thread.as_ref().and_then(|state| state.thread.as_ref());
        let latest_turn = thread.and_then(|thread| thread.latest_turn.as_ref()).map(|turn| &turn.turn_id);
        let active_plan = thread.is_some_and(|thread| {
            thread.activities.iter().any(|activity| {
                activity.kind == "turn.plan.updated" && activity.turn_id.as_ref() == latest_turn
            })
        });
        let proposed = thread.is_some_and(|thread| {
            thread
                .proposed_plans
                .iter()
                .any(|plan| plan.turn_id.as_ref() == latest_turn)
        });
        (active_plan || proposed || self.plan_panel_open).then(|| {
            (
                SharedString::from(if proposed { "Plan" } else { "Tasks" }),
                self.plan_panel_open,
            )
        })
    }

    fn render_plan_toggle(&self, label: SharedString, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let rest = style::alpha(colors.muted_foreground, 0.7);
        let hover = style::alpha(colors.foreground, 0.8);
        let fill = colors.accent;
        let tooltip = format!(
            "{} {} sidebar",
            if open { "Hide" } else { "Show" },
            label.to_lowercase()
        );
        div()
            .id("composer-plan-toggle")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .h(px(28.))
            .px(px(12.))
            .rounded(px(10.))
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .map(|this| {
                if open {
                    this.bg(fill).text_color(colors.foreground)
                } else {
                    this.text_color(rest).hover(move |style| style.bg(fill).text_color(hover))
                }
            })
            .child(
                Icon::new(IconName::ListTodo)
                    .size(px(16.))
                    .when(!open, |icon| icon.opacity(0.8)),
            )
            .child(label)
            .tooltip_text(tooltip)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ComposerEvent::TogglePlanSidebar)))
            .into_any_element()
    }

    /// The primary action at the right of the footer (`ComposerPrimaryActions`).
    fn render_primary_action(&mut self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        if let Some(progress) = self.question_progress() {
            let responding = self
                .pending
                .user_inputs
                .first()
                .is_some_and(|input| self.responding.contains(&input.request_id));
            let unavailable = !self.connected(cx);
            let enabled = !unavailable
                && !responding
                && if progress.is_last {
                    progress.is_complete
                } else {
                    progress.can_advance
                };
            let label = pending::primary_action_label(
                compact,
                progress.is_last,
                responding,
                progress.question_index,
            );
            return div()
                .flex()
                .items_center()
                .gap(if compact { px(6.) } else { px(8.) })
                .when(progress.question_index > 0, |this| {
                    let previous = Button::new("question-previous")
                        .variant(ButtonVariant::Outline)
                        .disabled(responding)
                        .rounded_full()
                        .on_click(cx.listener(|this, _, _, cx| this.previous_question(cx)));
                    this.child(if compact {
                        previous
                            .size(ButtonSize::IconSm)
                            .child(Icon::new(IconName::ChevronLeft).size(px(14.)))
                    } else {
                        previous.size(ButtonSize::Sm).label("Previous")
                    })
                })
                .child(
                    Button::new("question-submit")
                        .size(ButtonSize::Sm)
                        .label(label)
                        .disabled(!enabled)
                        .rounded_full()
                        .px(if compact { px(12.) } else { px(16.) })
                        .on_click(cx.listener(|this, _, _, cx| this.advance_question(cx))),
                )
                .into_any_element();
        }

        let (fill, fill_hover) = (colors.primary_90, colors.primary);
        let circle = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(28.))
                .rounded_full()
                .bg(fill)
        };
        let mut highlight = shadow::XS_5.to_vec();
        highlight[0].color = colors.primary_24;
        highlight.push(gpui_kit::BoxShadow {
            color: colors.inset_highlight,
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        });

        if self.is_running(cx) {
            return circle("composer-stop")
                .text_color(colors.card)
                .shadow(highlight)
                .cursor_pointer()
                .hover(move |style| style.bg(fill_hover))
                .child(svg().path("icons/composer/stop.svg").size(px(11.)).text_color(colors.card))
                .tooltip_text("Stop generation")
                .on_click(cx.listener(|this, _, _, cx| this.interrupt(cx)))
                .into_any_element();
        }

        let unavailable = !self.connected(cx);
        let connecting = self.is_connecting(cx);
        let busy = self.sending || connecting;
        let enabled = !busy && !unavailable && self.has_sendable_content(cx);
        let label = send::send_button_label(unavailable, connecting, false, self.sending);
        circle("composer-send")
            .text_color(colors.primary_foreground)
            .when(enabled, |this| {
                this.shadow(highlight)
                    .cursor_pointer()
                    .hover(move |style| style.bg(fill_hover))
                    .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
            })
            .when(!enabled, |this| this.opacity(0.3))
            .child(if busy {
                Spinner::new("composer-send-spinner")
                    .size(px(14.))
                    .text_color(colors.primary_foreground)
                    .into_any_element()
            } else {
                svg()
                    .path("icons/composer/send.svg")
                    .size(px(13.))
                    .text_color(colors.primary_foreground)
                    .into_any_element()
            })
            .tooltip_text(label)
            .into_any_element()
    }

    /// The 16px usage ring (`ContextWindowMeter`), tooltip with the numbers.
    fn render_context_meter(&self, usage: ContextWindow, cx: &App) -> AnyElement {
        let colors = cx.colors();
        let fraction = usage.used_percentage.unwrap_or(0.0) as f32 / 100.0;
        let track = style::alpha(colors.muted_foreground, 0.35);
        let progress = if fraction > 0.9 {
            style::palette::RED_500
        } else {
            colors.primary
        };
        let tooltip = match (usage.max_tokens, usage.used_percentage) {
            (Some(max), Some(percent)) => format!(
                "Context Window · {}% · {}/{}",
                percent.round() as i64,
                pending::format_tokens(usage.used_tokens),
                pending::format_tokens(max)
            ),
            _ => format!("Context Window · {}", pending::format_tokens(usage.used_tokens)),
        };
        div()
            .id("composer-context-meter")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(24.))
            .rounded_full()
            .hover(|style| style.bg(colors.accent))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        // viewBox 24 at 16px: radius 9.75 → 6.5px, stroke 3 → 2px.
                        let scale = f32::from(bounds.size.width) / 24.0;
                        let center = bounds.center();
                        let radius = 9.75 * scale;
                        let width = px(3.0 * scale);
                        if let Some(path) = arc_path(center, radius, 1.0, width) {
                            window.paint_path(path, track);
                        }
                        if fraction > 0.0
                            && let Some(path) = arc_path(center, radius, fraction.min(1.0), width)
                        {
                            window.paint_path(path, progress);
                        }
                    },
                )
                .size(px(16.)),
            )
            .tooltip_text_with_delay(tooltip, std::time::Duration::from_millis(150))
            .into_any_element()
    }
}

/// A stroked arc starting at 12 o'clock, clockwise over `fraction` of the circle, approximated
/// with short segments.
fn arc_path(
    center: Point<Pixels>,
    radius: f32,
    fraction: f32,
    width: Pixels,
) -> Option<gpui_kit::Path<Pixels>> {
    let steps = (fraction * 64.0).ceil().max(2.0) as usize;
    let mut builder = PathBuilder::stroke(width);
    for step in 0..=steps {
        let angle = -PI / 2.0 + 2.0 * PI * fraction * step as f32 / steps as f32;
        let at = point(
            center.x + px(radius * angle.cos()),
            center.y + px(radius * angle.sin()),
        );
        if step == 0 {
            builder.move_to(at);
        } else {
            builder.line_to(at);
        }
    }
    builder.build().ok()
}
