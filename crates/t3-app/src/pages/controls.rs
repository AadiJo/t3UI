//! Small controls the pages share that t3-ui does not have yet: the segmented `ToggleGroup`
//! (`ui/toggle-group.tsx:32-33`, `ui/toggle.tsx:24,42`) and `InlineButton` (`ui/button.tsx:103`).

use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use t3_ui::{
    ActiveColors as _, TooltipExt as _,
    tokens::{DISABLED_OPACITY, hex, radius, shadow, text},
};

use crate::chrome::TypeScale as _;

/// `shadow-xs/10`: the xs shadow (0 1px 2px) at 10% black.
fn shadow_xs_10() -> Vec<gpui_kit::BoxShadow> {
    let mut shadows = shadow::XS_5.to_vec();
    for shadow in &mut shadows {
        shadow.color = hex(0x0000_001A);
    }
    shadows
}

/// A segment's click handler.
pub type SegmentHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One option of a [`segmented`] group.
pub struct Segment {
    pub id: ElementId,
    pub label: SharedString,
    pub pressed: bool,
    pub tooltip: Option<SharedString>,
    pub on_click: SegmentHandler,
}

/// The segmented toggle group: a `bg-input/40` tray (`rounded-lg p-0.5 gap-0.5`) of 24px
/// `text-xs` items; the pressed one is a raised `background` (dark `input/72`) chip. A disabled
/// group keeps its place at 64% opacity and ignores clicks.
pub fn segmented(
    id: impl Into<ElementId>,
    segments: Vec<Segment>,
    disabled: bool,
    cx: &App,
) -> impl IntoElement {
    let colors = cx.colors();
    let dark = colors.is_dark;
    div()
        .id(id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .rounded(radius::LG)
        .bg(colors.input_40)
        .when(disabled, |this| this.opacity(DISABLED_OPACITY))
        .children(segments.into_iter().map(|segment| {
            let pressed = segment.pressed;
            let on_click = segment.on_click;
            div()
                .id(segment.id)
                .h_6()
                .px(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::MD)
                .type_scale(text::XS)
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .map(|this| {
                    if pressed {
                        this.text_color(colors.foreground)
                            .bg(if dark {
                                colors.input_72
                            } else {
                                colors.background
                            })
                            .shadow(shadow_xs_10())
                    } else {
                        this.text_color(colors.muted_foreground)
                    }
                })
                .when(!disabled, |this| {
                    this.cursor_pointer()
                        .when(!pressed, |this| {
                            this.hover(|style| {
                                style
                                    .bg(if dark {
                                        colors.input_32
                                    } else {
                                        colors.background_55
                                    })
                                    .text_color(colors.foreground)
                            })
                        })
                        .on_click(move |event, window, cx| on_click(event, window, cx))
                })
                .when_some(segment.tooltip, |this, tooltip| this.tooltip_text(tooltip))
                .child(segment.label)
        }))
}

/// `InlineButton`: text-sized, `font-medium`, underlined on hover. Callers add children and the
/// click handler. Takes the colors so menu triggers can build it without a context.
pub fn inline_button(
    id: impl Into<ElementId>,
    muted: bool,
    colors: &'static t3_ui::Colors,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(2.))
        .whitespace_nowrap()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .text_color(if muted {
            colors.muted_foreground
        } else {
            colors.foreground
        })
        .hover(move |style| {
            let style = style.underline();
            if muted {
                style.text_color(colors.foreground)
            } else {
                style
            }
        })
}

/// `tabular-nums`: the `tnum` OpenType feature, so digit columns line up. Inherited by text
/// below the element it is set on.
pub fn tabular_nums() -> gpui_kit::FontFeatures {
    gpui_kit::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]))
}
