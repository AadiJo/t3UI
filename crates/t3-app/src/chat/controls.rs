//! Small pieces the chat view composes from `t3_ui` primitives: icons inside buttons, the
//! panel toggles, and the stand-ins drawn while a slot (header actions, composer, branch
//! toolbar) has no registered view.

use gpui_kit::{
    AnyElement, App, ElementId, IntoElement, ParentElement as _, Pixels, SharedString, Styled as _,
    div, px,
};
use t3_ui::{ActiveColors as _, Button, ButtonSize, ButtonVariant, Icon, IconName, Separator};

/// An icon sized for a button child, drawn 2px wider than its box on each side like the web's
/// `[&_svg]:-mx-0.5`, at the buttons' 80% icon opacity.
pub(super) fn small_icon(name: IconName, size: Pixels) -> AnyElement {
    div()
        .relative()
        .flex_none()
        .w(size - px(4.))
        .h(size)
        .child(
            Icon::new(name)
                .size(size)
                .opacity(t3_ui::tokens::ICON_OPACITY)
                .absolute()
                .top_0()
                .left(px(-2.)),
        )
        .into_any_element()
}

/// A ghost `Toggle size="sm"` (28px) holding a 14px icon, accent fill while pressed.
pub(super) fn panel_toggle(
    id: impl Into<ElementId>,
    icon: IconName,
    pressed: bool,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconSm)
        .pressed(pressed)
        .tooltip(tooltip)
        .child(Icon::new(icon).size(px(14.)))
}

/// What the header shows until the scripts / Open in / Git controls are registered: their
/// resting look for a project without scripts ("Add action", "Open", "Commit").
pub(super) fn header_actions_placeholder(cx: &App) -> AnyElement {
    let colors = cx.colors();
    let split = |id: &'static str, icon: AnyElement, label: &'static str| {
        div()
            .flex()
            .items_center()
            .child(
                Button::new(id)
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .rounded_r(px(0.))
                    .border_r_0()
                    .child(icon)
                    .child(label),
            )
            .child(Separator::vertical())
            .child(
                Button::new(SharedString::from(format!("{id}-menu")))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::IconXs)
                    .rounded_l(px(0.))
                    .border_l_0()
                    .child(Icon::new(IconName::ChevronDown).size(px(16.)).opacity(0.8)),
            )
    };
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .text_color(colors.foreground)
        .child(
            Button::new("header-add-action")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .icon(IconName::Plus)
                .label("Add action"),
        )
        .child(split(
            "header-open-in",
            t3_ui::logo(t3_ui::Logo::CursorIcon, colors.is_dark, px(14.)).into_any_element(),
            "Open",
        ))
        .child(split(
            "header-git",
            small_icon(IconName::GitCommitHorizontal, px(14.)),
            "Commit",
        ))
        .into_any_element()
}
