//! coss `Checkbox` (`ui/checkbox.tsx`): a 16px box (radius 4) whose checked indicator covers
//! the border with `primary`. Default cursor, like the reference. Controlled.

use std::rc::Rc;

use gpui_kit::{
    App, ElementId, FontWeight, IntoElement, ParentElement as _, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, accesskit, base, div,
    prelude::FluentBuilder as _, px,
};

use super::{ChangeHandler, Interaction, bevel, focus_ring};
use crate::{
    ActiveColors as _, Icon, IconName,
    tokens::{DISABLED_OPACITY, shadow},
};

/// Checkbox value, including the mixed state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CheckedState {
    #[default]
    Unchecked,
    Checked,
    Indeterminate,
}

impl From<bool> for CheckedState {
    fn from(checked: bool) -> Self {
        if checked {
            Self::Checked
        } else {
            Self::Unchecked
        }
    }
}

/// A checkbox with an optional label (14px medium, 8px gap, part of the hit target).
///
/// ```ignore
/// Checkbox::new("hidden").checked(self.show_hidden).label("Show hidden files")
///     .on_change(cx.listener(|this, checked, _, cx| { this.show_hidden = *checked; cx.notify(); }))
/// ```
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    state: CheckedState,
    disabled: bool,
    invalid: bool,
    label: Option<SharedString>,
    on_change: Option<ChangeHandler<bool>>,
    preview: Interaction,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            state: CheckedState::Unchecked,
            disabled: false,
            invalid: false,
            label: None,
            on_change: None,
            preview: Interaction::Rest,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.state = checked.into();
        self
    }

    /// Sets checked, unchecked or indeterminate.
    pub fn state(mut self, state: CheckedState) -> Self {
        self.state = state;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// `aria-invalid`: destructive border.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Receives the requested value (`true` when an unchecked or mixed box is clicked).
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let disabled = self.disabled;
        let state = self.state;
        let checked = state == CheckedState::Checked;
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let focus_visible = self.preview == Interaction::FocusVisible
            || (!disabled && focus.is_focused(window) && window.last_input_was_keyboard());
        let radius = px(4.);
        let quiet = disabled || checked || self.invalid;

        let mut box_el = div()
            .relative()
            .flex_none()
            .size(px(16.))
            .rounded(radius)
            .border_1()
            .border_color(if self.invalid {
                colors.destructive_36
            } else {
                colors.input
            })
            .bg(if colors.is_dark && !checked {
                colors.input_32
            } else {
                colors.background
            })
            .when(!quiet, |this| {
                // Dark fills are `input/32`, too transparent for a drop shadow.
                this.when(!colors.is_dark, |this| this.shadow(shadow::XS_5.to_vec()))
                    .child(bevel(radius, colors.bevel, colors))
            });
        box_el = match state {
            CheckedState::Unchecked => box_el,
            CheckedState::Checked => box_el.child(
                div()
                    .absolute()
                    .inset(px(-1.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius)
                    .bg(colors.primary)
                    .child(
                        Icon::new(IconName::CheckIndicatorBold)
                            .size(px(12.))
                            .color(colors.primary_foreground),
                    ),
            ),
            CheckedState::Indeterminate => box_el.child(
                div()
                    .absolute()
                    .inset(px(-1.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::IndeterminateBold)
                            .size(px(12.))
                            .color(colors.foreground),
                    ),
            ),
        };
        if focus_visible {
            let ring_color = if self.invalid {
                if colors.is_dark {
                    colors.destructive_24
                } else {
                    colors.destructive_48
                }
            } else {
                colors.ring
            };
            let [band, ring] = focus_ring(radius, px(1.), colors);
            box_el = box_el.child(band).child(ring.border_color(ring_color));
        }

        let on_change = self.on_change;
        base::Button::new(self.id)
            .role(Role::CheckBox)
            .aria_toggled(match state {
                CheckedState::Checked => accesskit::Toggled::True,
                CheckedState::Unchecked => accesskit::Toggled::False,
                CheckedState::Indeterminate => accesskit::Toggled::Mixed,
            })
            .track_focus(&focus)
            .disabled(disabled)
            .justify_start()
            .gap(px(8.))
            .when(disabled, |this| this.opacity(DISABLED_OPACITY))
            .child(box_el)
            .when_some(self.label, |this, label| {
                this.when_some(Some(label.clone()), |this, label| {
                    this.accessibility_label(label)
                })
                .child(
                    div()
                        .text_size(px(14.))
                        .line_height(px(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(label),
                )
            })
            .when_some(on_change, |this, on_change| {
                this.on_click(move |_, window, cx| on_change(&!checked, window, cx))
            })
    }
}
