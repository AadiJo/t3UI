//! Element builders for the plan surface (`PlanSidebar.tsx`): section labels, step rows and
//! their status icons, and the empty state. Sizes are the fork's, in px.

use std::{cell::Cell, f32::consts::TAU, rc::Rc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, Div, ElementId, FontWeight, IntoElement,
    ParentElement as _, Pixels, SharedString, StrikethroughStyle, Styled as _, Transformation,
    Window, canvas, div, prelude::FluentBuilder as _, px, radians,
};
use t3_ui::{Colors, Icon, IconName, tokens::motion};

use super::logic::{PlanStep, StepStatus};

/// `tracking-widest` (0.1em) at 10px.
const WIDEST_10: Pixels = px(1.);

/// `text-[10px] font-semibold tracking-widest uppercase` (the `Steps` heading and the plan
/// title), 15px lines. GPUI has no letter spacing, so each glyph is a box with the tracking as
/// the gap; words stay together and wrap as units. Callers set the color.
pub(super) fn section_label(text: &str) -> Div {
    let upper = text.to_uppercase();
    let words: Vec<&str> = upper.split_whitespace().collect();
    let last = words.len().saturating_sub(1);
    div()
        .flex()
        .flex_wrap()
        .text_size(px(10.))
        .line_height(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .children(words.iter().enumerate().map(|(index, word)| {
            div()
                .flex()
                .flex_none()
                .gap(WIDEST_10)
                .mr(WIDEST_10)
                .children(
                    word.chars()
                        .map(|ch| div().child(SharedString::from(ch.to_string()))),
                )
                // The space keeps its own tracking, like CSS letter-spacing.
                .when(index < last, |word| word.child("\u{a0}"))
        }))
}

/// One step: `flex items-center gap-2.5 rounded-lg px-2.5 py-2`, tinted by status, with a
/// 20px status circle and 13px `leading-snug` text.
pub(super) fn step_row(
    id: ElementId,
    step: &PlanStep,
    colors: &Colors,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let (tint, text_color) = match step.status {
        // `bg-emerald-500/5` and `bg-blue-500/5`: the success and info tokens' hues.
        StepStatus::Completed => (
            Some(colors.success.opacity(0.05)),
            colors.muted_foreground_50,
        ),
        StepStatus::InProgress => (
            Some(colors.info.opacity(0.05)),
            colors.foreground.opacity(0.9),
        ),
        StepStatus::Pending => (None, colors.muted_foreground_70),
    };
    let completed = step.status == StepStatus::Completed;
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(10.))
        .px(px(10.))
        .py(px(8.))
        .when_some(tint, |row, tint| row.bg(tint))
        .child(status_icon(id, step.status, colors, window, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .line_height(px(13. * 1.375))
                .text_color(text_color)
                .when(completed, |text| {
                    // `line-through decoration-muted-foreground/20`.
                    let mut text = text;
                    text.text_style().strikethrough = Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(colors.muted_foreground.opacity(0.2)),
                    });
                    text
                })
                .child(SharedString::from(step.step.clone())),
        )
}

/// The 20px status circle (`stepStatusIcon`).
fn status_icon(
    id: ElementId,
    status: StepStatus,
    colors: &Colors,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let circle = div()
        .flex()
        .flex_none()
        .size(px(20.))
        .items_center()
        .justify_center()
        .rounded_full();
    match status {
        StepStatus::Completed => circle.bg(colors.success.opacity(0.1)).child(
            Icon::new(IconName::Check)
                .size(px(12.))
                .color(colors.success_foreground),
        ),
        StepStatus::InProgress => circle
            .bg(colors.primary.opacity(0.1))
            .child(spinning_loader(id, px(12.), colors, window, cx)),
        StepStatus::Pending => circle
            .border_1()
            .border_color(colors.border_60)
            .bg(colors.muted.opacity(0.3))
            .child(
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(colors.muted_foreground.opacity(0.3)),
            ),
    }
}

/// Continuous animations tick at most this often (`t3_ui::Spinner`'s rate).
const TICK_FPS: f32 = 1000. / motion::CONTINUOUS_FRAME.as_millis() as f32;

/// lucide `loader` in `primary`, spinning once a second (`animate-spin`). Like `t3_ui::Spinner`
/// (which only draws `loader-circle`) it ticks at ~30fps and stops while clipped out of view:
/// a probe records visibility at prepaint and the next render reads it.
fn spinning_loader(
    id: ElementId,
    size: Pixels,
    colors: &Colors,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let visible = window
        .use_keyed_state(ElementId::from((id.clone(), "visible")), cx, |_, _| {
            Rc::new(Cell::new(true))
        })
        .read(cx)
        .clone();
    let icon = Icon::new(IconName::Loader).size(size).color(colors.primary);
    let icon = if visible.get() {
        icon.with_animation(
            id,
            Animation::new(Duration::from_secs(1))
                .repeat_synced()
                .with_max_fps(TICK_FPS),
            |icon, turn| icon.transform(Transformation::rotate(radians(turn * TAU))),
        )
        .into_any_element()
    } else {
        icon.into_any_element()
    };
    div()
        .relative()
        .flex_none()
        .size(size)
        .child(icon)
        .child(
            canvas(
                move |bounds: Bounds<Pixels>, window, _| {
                    visible.set(window.content_mask().bounds.intersects(&bounds));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}

/// `No active plan yet.` centered with 48px of vertical padding.
pub(super) fn empty_state(colors: &Colors) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(48.))
        .child(
            div()
                .text_size(px(13.))
                .line_height(px(19.5))
                .text_color(colors.muted_foreground.opacity(0.4))
                .child("No active plan yet."),
        )
        .child(
            div()
                .mt(px(4.))
                .text_size(px(11.))
                .line_height(px(16.5))
                .text_color(colors.muted_foreground.opacity(0.3))
                .child("Plans will appear here when generated."),
        )
}
