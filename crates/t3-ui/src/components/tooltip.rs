//! coss `Tooltip` (`ui/tooltip.tsx`): a small popover-colored label above the trigger,
//! offset 4px, centered, after a 600ms hover delay. Close delay is 0.
//!
//! Built on GPUI's native tooltip timing (`.tooltip()` + `.tooltip_show_delay()`), with the
//! popup placed by gpui-base's `Positioner` against the trigger's bounds instead of the mouse.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt as _, App, AppContext as _, Bounds, Context, FontWeight, IntoElement,
    ParentElement, Pixels, Render, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window,
    base::{ElementExt as _, Placement, Positioner},
    div, px,
};

use super::popup_surface;
use crate::{
    ActiveColors as _,
    tokens::{motion, shadow},
};

/// Adds a coss tooltip to any interactive element with an id.
///
/// ```ignore
/// div().id("info").tooltip_text("Drag to resize sidebar")
/// button.tooltip_text_with_delay("Context window", Duration::from_millis(150))
/// ```
/// Delay overrides in the reference: model picker 0ms, context meter 150ms, connection
/// settings 250ms.
pub trait TooltipExt: StatefulInteractiveElement + ParentElement + Sized {
    /// Tooltip after the default 600ms delay.
    fn tooltip_text(self, text: impl Into<SharedString>) -> Self {
        self.tooltip_text_with_delay(text, motion::TOOLTIP_DELAY)
    }

    /// Tooltip after `delay`.
    fn tooltip_text_with_delay(self, text: impl Into<SharedString>, delay: Duration) -> Self {
        let text = text.into();
        let trigger: Rc<Cell<Bounds<Pixels>>> = Rc::default();
        let writer = trigger.clone();
        self.on_prepaint(move |bounds, _, _| writer.set(bounds))
            .tooltip(move |_, cx| {
                let view = TooltipView {
                    text: text.clone(),
                    trigger: trigger.get(),
                };
                cx.new(|_| view).into()
            })
            .tooltip_show_delay(delay)
    }
}

impl<E: StatefulInteractiveElement + ParentElement> TooltipExt for E {}

/// The live tooltip: positioned above the trigger and faded in over 150ms.
struct TooltipView {
    text: SharedString,
    trigger: Bounds<Pixels>,
}

impl Render for TooltipView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Positioner::side(self.trigger)
            .placement(Placement::Top)
            .offset(px(4.))
            .margin(px(8.))
            .child(
                // Base UI also scales from 0.98; GPUI has no transform for divs, so only the
                // opacity animates.
                TooltipPopup::new(self.text.clone()).with_animation(
                    "tooltip-enter",
                    Animation::new(motion::POPUP_ENTER).with_easing(motion::ease_standard),
                    |popup, delta| popup.opacity(delta),
                ),
            )
    }
}

/// The tooltip surface on its own (radius 8, 12/16 text, padding 8x4, `shadow-md/5`).
/// Used by [`TooltipExt`] and for static previews.
#[derive(IntoElement)]
pub struct TooltipPopup {
    text: SharedString,
    style: gpui_kit::StyleRefinement,
}

impl TooltipPopup {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            style: Default::default(),
        }
    }
}

impl Styled for TooltipPopup {
    fn style(&mut self) -> &mut gpui_kit::StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TooltipPopup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let mut surface = popup_surface(px(8.), colors)
            .shadow(shadow::MD_5.to_vec())
            .flex()
            .px(px(8.))
            .py(px(4.))
            .max_w(px(320.))
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::NORMAL)
            .child(div().child(self.text));
        gpui_kit::Refineable::refine(surface.style(), &self.style);
        surface
    }
}
