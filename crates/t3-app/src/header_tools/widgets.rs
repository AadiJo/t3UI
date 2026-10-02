//! Small presentation pieces the header controls and the terminal drawer share: hover popovers
//! placed on any side, the coss `Group` join (two buttons and a separator), and menu rows with
//! custom content.

use std::{cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
    App, AppContext as _, Bounds, Context, Div, ElementId, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Render, SharedString, Stateful, StatefulInteractiveElement, Styled,
    Window,
    base::{Align, ElementExt as _, Placement, Positioner, actions::Cancel},
    div, prelude::FluentBuilder as _, px,
};
use t3_ui::{ActiveColors as _, Button, TooltipPopup, tokens::DISABLED_OPACITY};

/// Hover delay of Base UI `PopoverTrigger openOnHover` (the drawer's action labels, disabled
/// git actions).
pub(crate) const HOVER_POPOVER_DELAY: Duration = Duration::from_millis(300);
/// Base UI tooltip delay.
pub(crate) const TOOLTIP_DELAY: Duration = Duration::from_millis(600);

/// Where a hover label sits against its trigger.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HoverPlacement {
    pub side: Placement,
    pub align: Align,
    pub offset: Pixels,
    pub delay: Duration,
}

impl HoverPlacement {
    /// `side="bottom" sideOffset={6}`, the drawer's action buttons.
    pub(crate) const DRAWER: Self = Self {
        side: Placement::Bottom,
        align: Align::Center,
        offset: px(6.),
        delay: HOVER_POPOVER_DELAY,
    };

    /// Tooltip side top (header buttons).
    pub(crate) const TOP: Self = Self {
        side: Placement::Top,
        align: Align::Center,
        offset: px(4.),
        delay: TOOLTIP_DELAY,
    };

    /// Tooltip side bottom (titlebar toggles).
    pub(crate) const BOTTOM: Self = Self {
        side: Placement::Bottom,
        align: Align::Center,
        offset: px(4.),
        delay: TOOLTIP_DELAY,
    };
}

/// Adds a tooltip-styled hover label at `placement`.
pub(crate) fn hover_label<E>(element: E, text: impl Into<SharedString>, placement: HoverPlacement) -> E
where
    E: StatefulInteractiveElement + ParentElement,
{
    let text = text.into();
    let trigger: Rc<Cell<Bounds<Pixels>>> = Rc::default();
    let writer = trigger.clone();
    element
        .on_prepaint(move |bounds, _, _| writer.set(bounds))
        .tooltip(move |_, cx| {
            let view = HoverLabel {
                text: text.clone(),
                trigger: trigger.get(),
                placement,
            };
            cx.new(|_| view).into()
        })
        .tooltip_show_delay(placement.delay)
}

struct HoverLabel {
    text: SharedString,
    trigger: Bounds<Pixels>,
    placement: HoverPlacement,
}

impl Render for HoverLabel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Positioner::side(self.trigger)
            .placement(self.placement.side)
            .align(self.placement.align)
            .offset(self.placement.offset)
            .margin(px(8.))
            .child(TooltipPopup::new(self.text.clone()))
    }
}

/// The left button of a joined `Group`: square right corners, no right border.
pub(crate) fn join_left(button: Button) -> Button {
    button.rounded_r(px(0.)).border_r(px(0.))
}

/// The right button of a joined `Group`: square left corners, no left border.
pub(crate) fn join_right(button: Button) -> Button {
    button.rounded_l(px(0.)).border_l(px(0.))
}

/// `GroupSeparator`: a 1px `input` rule (dark mode layers `input/32` over it).
pub(crate) fn group_separator(cx: &App) -> Div {
    let colors = cx.colors();
    div()
        .relative()
        .flex_none()
        .w(px(1.))
        .h(px(24.))
        .bg(colors.input)
        .when(colors.is_dark, |this| {
            this.child(div().absolute().inset_0().bg(colors.input_32))
        })
}

/// A menu row like `MenuItem` (min height 28, 8×4 padding, radius 6, 14px, `accent` on hover)
/// for rows with custom content. `disabled` rows dim and ignore the pointer.
pub(crate) fn menu_row(id: impl Into<ElementId>, disabled: bool, cx: &App) -> Stateful<Div> {
    let colors = cx.colors();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(8.))
        .min_h(px(28.))
        .px(px(8.))
        .py(px(4.))
        .rounded(px(6.))
        .text_size(px(14.))
        .line_height(px(20.))
        .text_color(colors.foreground)
        .cursor_default()
        .when(!disabled, |this| {
            this.hover(|style| style.bg(colors.accent).text_color(colors.accent_foreground))
        })
        .when(disabled, |this| this.opacity(DISABLED_OPACITY))
}

/// Closes the open menu or popover that contains the focused element.
pub(crate) fn close_menu(window: &mut Window, cx: &mut App) {
    window.dispatch_action(Box::new(Cancel), cx);
}
