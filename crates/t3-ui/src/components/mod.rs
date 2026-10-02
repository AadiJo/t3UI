//! Styled primitives reproducing the reference UI's coss components (`apps/web/src/components/ui`).
//! Behavior comes from gpui-base / gpui-component where it fits; every visual value comes
//! from [`crate::tokens`].
//!
//! Shared drawing helpers live here: the 1px edge bevel every coss surface draws with
//! `::before`, focus rings, and the popup surface.

mod badge;
mod button;
mod card;
mod checkbox;
mod dialog;
mod feedback;
mod input;
mod menu;
mod popover;
mod scroll_area;
mod select;
mod switch;
mod toast;
mod tooltip;

pub use badge::{Badge, BadgeSize, BadgeVariant, Kbd};
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::{Card, CardFooter, CardHeader, CardPanel};
pub use checkbox::{Checkbox, CheckedState};
pub use dialog::{
    AlertDialog, Dialog, DialogDescription, DialogFooter, DialogHeader, DialogPanel, DialogPopup,
    DialogTitle, FooterVariant, Sheet, SheetPopup, dialog_backdrop,
};
pub use feedback::{Separator, Skeleton, Spinner};
pub use input::{Input, InputSize, Textarea};
pub use menu::{
    ContextMenu, DropdownMenu, MenuCheckboxItem, MenuGroupLabel, MenuItem, MenuPopup,
    MenuSeparator, MenuShortcut,
};
pub use popover::{Align, Popover, PopoverDescription, PopoverPopup, PopoverTitle};
pub use scroll_area::ScrollArea;
pub use select::{Select, SelectItem, SelectPopup, SelectSize, SelectTrigger, SelectVariant};
pub use switch::Switch;
pub use toast::{Toast, ToastKind};
pub use tooltip::{TooltipExt, TooltipPopup};

use std::rc::Rc;

use gpui_kit::{
    App, BoxShadow, ClickEvent, Div, Hsla, ParentElement as _, Pixels, Styled as _, Window, div,
    point, px,
};

/// Click callback shared by pressable primitives.
pub(crate) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
/// Requested-value callback (`on_change`) of controlled primitives.
pub(crate) type ChangeHandler<T> = Rc<dyn Fn(&T, &mut Window, &mut App)>;
/// Callback without arguments beyond the contexts.
pub(crate) type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

use crate::{Colors, Icon, tokens::shadow};

/// A forced interaction state, so the gallery and snapshots can show hover, pressed and
/// focus-visible looks without a pointer or keyboard. Live UI leaves it at `Rest` and gets
/// the real states from GPUI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Interaction {
    #[default]
    Rest,
    Hover,
    Pressed,
    FocusVisible,
}

/// An icon whose layout box is `left + right` narrower than the glyph, which spills over by
/// those amounts: the CSS `-mx-0.5` / `-me-1` on icons inside controls. Negative margins
/// can't be used directly: taffy then drops the content width of a fit-content parent, and
/// the control collapses to its padding.
pub(crate) fn overhang(icon: Icon, size: Pixels, left: Pixels, right: Pixels) -> Div {
    div()
        .relative()
        .flex_none()
        .w(size - left - right)
        .h(size)
        .child(icon.size(size).absolute().top_0().left(-left))
}

/// Outer shadows for a control whose fill is `fill`. GPUI paints drop shadows under the
/// element instead of clipping them to its outside like CSS, so a mostly transparent fill
/// would show the shadow as a gray tint; such controls lose the (barely visible) shadow.
pub(crate) fn outer_shadow(fill: Hsla, shadows: &[BoxShadow]) -> Vec<BoxShadow> {
    if fill.a < 0.5 {
        Vec::new()
    } else {
        shadows.to_vec()
    }
}

/// GPUI group name shared by interactive primitives, so their bevel and inset children can
/// follow the root's hover/active state.
pub(crate) const GROUP: &str = "t3-control";

/// The coss edge bevel: a `::before` overlay (inset 0, radius r-1) with
/// `box-shadow: 0 1px black/4` in light mode (lands on the bottom border row) or
/// `0 -1px white/6` in dark mode (top border row).
///
/// Drawn as a child shifted one pixel past the padding box with an inset shadow whose hole is
/// the padding box, which leaves exactly the 1px crescent CSS paints, curved ends included.
/// `radius` is the parent's outer radius; `border` its border width.
pub(crate) fn bevel(radius: Pixels, color: Hsla, colors: &Colors) -> Div {
    let dark = colors.is_dark;
    let shift = if dark { px(-1.) } else { px(1.) };
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(shift)
        .bottom(-shift)
        .rounded((radius - px(1.)).max(px(0.)))
        .shadow(vec![inset_line(-shift, color)])
}

/// A 1px inset line offset by `dy` (positive: top edge), e.g. the solid button highlight.
pub(crate) fn inset_line(dy: Pixels, color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), dy),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: true,
    }
}

/// `focus-visible:ring-2 ring-ring ring-offset-1 ring-offset-background`: a 1px band of
/// background then a 2px ring, outside the border box. Border-only children, so translucent
/// controls keep their fill. `radius` / `border` are the control's.
pub(crate) fn focus_ring(radius: Pixels, border: Pixels, colors: &Colors) -> [Div; 2] {
    [
        ring_band(radius, border, px(0.), px(1.), colors.background),
        ring_band(radius, border, px(1.), px(2.), colors.ring),
    ]
}

/// A ring `width` wide starting `gap` outside the border box (`ring-[3px] ring-ring/24` on
/// inputs is `gap` 0, `width` 3).
pub(crate) fn ring_band(
    radius: Pixels,
    border: Pixels,
    gap: Pixels,
    width: Pixels,
    color: Hsla,
) -> Div {
    let outset = border + gap + width;
    div()
        .absolute()
        .top(-outset)
        .bottom(-outset)
        .left(-outset)
        .right(-outset)
        .rounded(radius + gap + width)
        .border(width)
        .border_color(color)
}

/// The shared popup surface (menu, select, popover, toast): radius, 1px border, popover
/// background, `shadow-lg/5` and the edge bevel. Caller adds content and padding.
pub(crate) fn popup_surface(radius: Pixels, colors: &Colors) -> Div {
    div()
        .relative()
        .rounded(radius)
        .border_1()
        .border_color(colors.border)
        .bg(colors.popover)
        .text_color(colors.popover_foreground)
        .shadow(shadow::LG_5.to_vec())
        .child(bevel(radius, colors.bevel, colors))
}
