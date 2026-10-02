//! coss `Input` and `Textarea` (`ui/input.tsx`, `ui/textarea.tsx`): the bordered
//! `input-control` frame around gpui-component's text editing (`InputState`).
//!
//! Focus turns the border `ring` and adds a 3px `ring/24` halo; invalid fields use the
//! destructive border. The placeholder uses gpui-component's `muted-foreground` (the
//! reference dims it to 72%).

use gpui_kit::{
    AnyElement, App, Entity, Focusable as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    StyleRefinement, Styled, Window,
    component::input::{Input as TextInput, InputState, Textarea as TextArea, TextareaState},
    div,
    prelude::FluentBuilder as _,
    px,
};

use super::{Interaction, bevel, ring_band};
use crate::{
    ActiveColors as _,
    tokens::{DISABLED_OPACITY, shadow},
};

/// Field size: inner heights 26 / 30 / 34 (28 / 32 / 36 with the border).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum InputSize {
    Sm,
    #[default]
    Default,
    Lg,
}

/// A single-line text field.
///
/// ```ignore
/// // in the owning view's constructor:
/// let name = cx.new(|cx| InputState::new(window, cx).placeholder("Thread title"));
/// // in render:
/// Input::new(&self.name)
/// ```
/// The editing state behind a field.
enum Field {
    Line(Entity<InputState>),
    Multi(Entity<TextareaState>),
}

#[derive(IntoElement)]
pub struct Input {
    field: Field,
    size: InputSize,
    disabled: bool,
    invalid: bool,
    preview: Interaction,
    style: StyleRefinement,
}

impl Input {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            field: Field::Line(state.clone()),
            size: InputSize::Default,
            disabled: false,
            invalid: false,
            preview: Interaction::Rest,
            style: StyleRefinement::default(),
        }
    }

    pub fn size(mut self, size: InputSize) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// `aria-invalid`: destructive border (and ring while focused).
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// `FocusVisible` shows the focused look without focus.
    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A multi-line field (min height 70 / 66 / 74, grows with content when its state is
/// `auto_grow`). Same frame and states as [`Input`].
///
/// ```ignore
/// let notes = cx.new(|cx| TextareaState::new(window, cx));
/// Textarea::new(&notes)
/// ```
pub struct Textarea;

impl Textarea {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(state: &Entity<TextareaState>) -> Input {
        Input {
            field: Field::Multi(state.clone()),
            size: InputSize::Default,
            disabled: false,
            invalid: false,
            preview: Interaction::Rest,
            style: StyleRefinement::default(),
        }
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let radius = px(10.);
        let focus_handle = match &self.field {
            Field::Line(state) => state.read(cx).focus_handle(cx),
            Field::Multi(state) => state.read(cx).focus_handle(cx),
        };
        let focused = self.preview == Interaction::FocusVisible
            || (!self.disabled && focus_handle.is_focused(window));
        let invalid = self.invalid;
        let border = match (focused, invalid) {
            (true, true) => colors.destructive_64,
            (true, false) => colors.ring,
            (false, true) => colors.destructive_36,
            (false, false) => colors.input,
        };
        let ring = if invalid {
            if colors.is_dark {
                colors.destructive_24
            } else {
                colors.destructive_16
            }
        } else {
            colors.ring_24
        };
        let quiet = focused || invalid || self.disabled;

        let (inner, padding_x): (Pixels, Pixels) = match self.size {
            InputSize::Sm => (px(26.), px(9.)),
            InputSize::Default => (px(30.), px(11.)),
            InputSize::Lg => (px(34.), px(11.)),
        };
        let text: AnyElement = match self.field {
            Field::Line(state) => TextInput::new(&state)
                .appearance(false)
                .disabled(self.disabled)
                .text_size(px(14.))
                .px(padding_x)
                .py(px(0.))
                .h(inner)
                .line_height(inner)
                .into_any_element(),
            Field::Multi(state) => {
                let (min_height, padding_y) = match self.size {
                    InputSize::Sm => (px(66.), px(3.)),
                    InputSize::Default => (px(70.), px(5.)),
                    InputSize::Lg => (px(74.), px(7.)),
                };
                TextArea::new(&state)
                    .appearance(false)
                    .disabled(self.disabled)
                    .text_size(px(14.))
                    .px(padding_x)
                    .py(padding_y)
                    .min_h(min_height)
                    .line_height(px(20.))
                    .into_any_element()
            }
        };

        let mut frame = div()
            .relative()
            .flex()
            .w_full()
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(if colors.is_dark {
                colors.input_32
            } else {
                colors.background
            })
            .text_color(colors.foreground)
            .text_size(px(14.))
            .when(!quiet, |this| {
                this.shadow(shadow::XS_5.to_vec())
                    .child(bevel(radius, colors.bevel, colors))
            })
            .when(focused, |this| {
                this.child(ring_band(radius, px(1.), px(0.), px(3.), ring))
            })
            .when(self.disabled, |this| this.opacity(DISABLED_OPACITY))
            .child(text);
        gpui_kit::Refineable::refine(frame.style(), &self.style);
        frame
    }
}
