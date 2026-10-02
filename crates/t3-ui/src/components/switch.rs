//! coss `Switch` (`ui/switch.tsx`): an 18x30 pill (thumb 16) that slides 12px when checked.
//! Controlled: pass `checked`, handle `on_change`.

use std::rc::Rc;

use gpui_kit::{
    Animation, AnimationExt as _, App, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, accesskit, base, div, prelude::FluentBuilder as _, px,
};

use super::{ChangeHandler, GROUP, Interaction, focus_ring};
use crate::{
    ActiveColors as _,
    tokens::{DISABLED_OPACITY, motion, shadow},
};

/// A toggle switch.
///
/// ```ignore
/// Switch::new("notify").checked(self.notify).on_change(cx.listener(|this, on, _, cx| {
///     this.notify = *on;
///     cx.notify();
/// }))
/// ```
/// `small()` is the 12px-thumb variant used inside menu checkbox items.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    thumb: Pixels,
    label: Option<SharedString>,
    on_change: Option<ChangeHandler<bool>>,
    preview: Interaction,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            checked: false,
            disabled: false,
            thumb: px(16.),
            label: None,
            on_change: None,
            preview: Interaction::Rest,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The 12px-thumb switch (14x22 track) from `MenuCheckboxItem variant="switch"`.
    pub fn small(mut self) -> Self {
        self.thumb = px(12.);
        self
    }

    /// Accessible name, for switches without a visible label next to them.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Receives the requested value; update your state and re-render.
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

/// Remembers the last rendered value so a change animates once and first paint doesn't.
#[derive(Default)]
struct SwitchMotion {
    last: Option<bool>,
    generation: usize,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let thumb = self.thumb;
        let track_w = thumb * 2. - px(2.);
        let track_h = thumb + px(2.);
        let travel = thumb - px(4.);
        let checked = self.checked;
        let disabled = self.disabled;

        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let focus_visible = self.preview == Interaction::FocusVisible
            || (!disabled && focus.is_focused(window) && window.last_input_was_keyboard());
        let motion_state =
            window.use_keyed_state(ElementId::from((self.id.clone(), "motion")), cx, |_, _| {
                SwitchMotion::default()
            });
        let (animate, generation) = motion_state.update(cx, |state, _| {
            let changed = state.last.is_some_and(|last| last != checked);
            if changed {
                state.generation += 1;
            }
            state.last = Some(checked);
            (changed, state.generation)
        });

        let left_rest = |on: bool| if on { px(1.) + travel } else { px(1.) };
        let pressed_width = thumb * 1.1;
        let thumb_el = div()
            .id("thumb")
            .absolute()
            .top(px(1.))
            .left(left_rest(checked))
            .size(thumb)
            .rounded_full()
            .bg(colors.background)
            .shadow(shadow::SM_5.to_vec())
            // Pressed: `scale-x-110`, growing away from the side it rests on.
            .when(!disabled, |this| {
                this.group_active(GROUP, move |style| {
                    let style = style.w(pressed_width);
                    if checked {
                        style.left(left_rest(true) - (pressed_width - thumb))
                    } else {
                        style
                    }
                })
            })
            .when(self.preview == Interaction::Pressed, |this| {
                this.w(pressed_width).when(checked, |this| {
                    this.left(left_rest(true) - (pressed_width - thumb))
                })
            });
        let thumb_el = if animate {
            let (from, to) = (left_rest(!checked), left_rest(checked));
            thumb_el
                .with_animation(
                    ElementId::NamedInteger("thumb-motion".into(), generation as u64),
                    Animation::new(motion::SWITCH_THUMB).with_easing(motion::ease_standard),
                    move |thumb, t| thumb.left(from + (to - from) * t),
                )
                .into_any_element()
        } else {
            thumb_el.into_any_element()
        };

        let on_change = self.on_change;
        base::Button::new(self.id)
            .role(Role::Switch)
            .aria_toggled(if checked {
                accesskit::Toggled::True
            } else {
                accesskit::Toggled::False
            })
            .when_some(self.label, |this, label| this.accessibility_label(label))
            .track_focus(&focus)
            .disabled(disabled)
            .group(GROUP)
            .relative()
            .flex_none()
            .w(track_w)
            .h(track_h)
            .rounded_full()
            .bg(if checked {
                colors.primary
            } else {
                colors.input
            })
            .map(|this| {
                if disabled {
                    this.opacity(DISABLED_OPACITY).cursor_not_allowed()
                } else {
                    this.cursor_pointer()
                }
            })
            .child(thumb_el)
            .when(focus_visible, |this| {
                this.children(focus_ring(track_h / 2., px(0.), colors))
            })
            .when_some(on_change, |this, on_change| {
                this.on_click(move |_, window, cx| on_change(&!checked, window, cx))
            })
    }
}
