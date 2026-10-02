//! Small presentation helpers shared by the shell's views: the Tailwind type scale, the window
//! drag region, and text tooltips.

use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, MouseButton, Pixels, Stateful,
    StatefulInteractiveElement as _, Styled, Window, WindowControlArea, div, px,
};

/// Tailwind font size plus line height (`t3_ui::tokens::text::*`). Arbitrary sizes such as
/// `text-[10px]` inherit the parent's ratio; [`inherit_ratio`] computes that line height.
pub trait TypeScale: Styled + Sized {
    fn type_scale(self, (size, line_height): (Pixels, Pixels)) -> Self {
        self.text_size(size).line_height(line_height)
    }
}

impl<T: Styled> TypeScale for T {}

/// `text-[Npx]` inside a `text-xs` parent: line height is N × 16/12.
pub fn under_xs(size: f32) -> (Pixels, Pixels) {
    (px(size), px(size * 16. / 12.))
}

/// The web's `.drag-region`: dragging moves the window and a double click zooms it (macOS
/// titlebar behavior). Interactive children stop propagation, so they stay clickable.
pub fn drag_region(
    id: impl Into<ElementId>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<gpui_kit::Div> {
    let id = id.into();
    let should_move = window.use_keyed_state(id.clone(), cx, |_, _| false);
    div()
        .id(id)
        .window_control_area(WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, {
            let should_move = should_move.clone();
            move |event, _, cx| {
                should_move.write(cx, event.click_count == 1);
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let should_move = should_move.clone();
            move |_, _, cx| should_move.write(cx, false)
        })
        .on_mouse_move({
            let should_move = should_move.clone();
            move |_, window, cx| {
                if *should_move.read(cx) {
                    should_move.write(cx, false);
                    window.start_window_move();
                }
            }
        })
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        })
}

/// A child slot that renders nothing (keeps `.child(...)` chains branch-free).
pub fn empty() -> impl IntoElement {
    div().hidden()
}
