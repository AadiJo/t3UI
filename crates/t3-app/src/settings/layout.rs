//! Settings layout primitives (`web/components/settings/settingsLayout.tsx`, spec 3.3): the
//! scrolling page container, titled sections with their card, rows, and the per-row reset
//! button. Every page builds from these:
//!
//! ```ignore
//! page("settings-general", vec![
//!     SettingsSection::new("General")
//!         .child(SettingsRow::new("Theme", "Choose how T3 Code looks across the app.").control(select))
//!         .into_any_element(),
//! ])
//! ```

use gpui_kit::{
    AnyElement, App, BoxShadow, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _,
    Styled, Window, div, point, prelude::FluentBuilder as _, px,
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, Icon, IconName, ScrollArea,
    TooltipExt as _,
    tokens::{shadow, text},
};

use crate::chrome::{TypeScale as _, under_xs};

/// Content width of most pages (`max-w-3xl`).
pub const PAGE_MAX_WIDTH: Pixels = px(768.);
/// Content width of the keybindings page (`max-w-5xl`).
pub const WIDE_PAGE_MAX_WIDTH: Pixels = px(1024.);
/// Card radius (`rounded-2xl`).
const CARD_RADIUS: Pixels = px(18.);
/// Select widths (`sm:w-40`, `sm:w-44`) and the draft input width (`sm:w-72`).
pub const SELECT_WIDTH: Pixels = px(160.);
pub const SELECT_WIDE_WIDTH: Pixels = px(176.);
pub const DRAFT_INPUT_WIDTH: Pixels = px(288.);

/// `SettingsPageContainer`: a vertical scroller with 32px padding around a centered column
/// (max 768, or `max_width`) whose sections are 32px apart.
pub fn page(
    id: impl Into<ElementId>,
    max_width: Pixels,
    sections: impl IntoIterator<Item = AnyElement>,
) -> impl IntoElement {
    ScrollArea::new(id).flex_1().min_h_0().child(
        div().w_full().p_8().flex().justify_center().child(
            div()
                .w_full()
                .max_w(max_width)
                .flex()
                .flex_col()
                .gap_8()
                .children(sections),
        ),
    )
}

/// `SettingsSection`: an uppercase 11px title (after a 12px rule and an optional icon) with an
/// optional right-aligned header action, above a bordered card. Children are stacked with a
/// `border/60` line between them (the rows' `border-t ... first:border-t-0`).
#[derive(IntoElement)]
pub struct SettingsSection {
    title: SharedString,
    icon: Option<AnyElement>,
    header_action: Option<AnyElement>,
    children: Vec<AnyElement>,
    /// Draw the divider between children (off for free-form bodies like empty states).
    dividers: bool,
}

impl SettingsSection {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            icon: None,
            header_action: None,
            children: Vec::new(),
            dividers: true,
        }
    }

    /// An icon before the title (archived: the project favicon).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Content at the right of the title row, in a 20px-tall box.
    pub fn header_action(mut self, action: impl IntoElement) -> Self {
        self.header_action = Some(action.into_any_element());
        self
    }

    /// Children are not separated by lines.
    pub fn without_dividers(mut self) -> Self {
        self.dividers = false;
        self
    }
}

impl ParentElement for SettingsSection {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsSection {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let mut body: Vec<AnyElement> = Vec::with_capacity(self.children.len() * 2);
        for (index, child) in self.children.into_iter().enumerate() {
            if index > 0 && self.dividers {
                body.push(
                    div()
                        .h(px(1.))
                        .flex_none()
                        .bg(colors.border_60)
                        .into_any_element(),
                );
            }
            body.push(child);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .px_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .type_scale(under_xs(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.foreground.opacity(0.5))
                            .child(div().w_3().h(px(1.)).bg(colors.border))
                            .children(self.icon)
                            .child(self.title.to_uppercase()),
                    )
                    .child(
                        div()
                            .h_5()
                            .min_w_5()
                            .flex()
                            .items_center()
                            .justify_end()
                            .children(self.header_action),
                    ),
            )
            .child(card(colors).children(body))
    }
}

/// The section card: radius 18, border, `card` fill, a light-mode shadow, and the coss edge
/// bevel (a 1px line on the bottom border row in light mode, the top border row in dark).
fn card(colors: &Colors) -> gpui_kit::Div {
    let dark = colors.is_dark;
    let shift = if dark { px(-1.) } else { px(1.) };
    div()
        .relative()
        .flex()
        .flex_col()
        .rounded(CARD_RADIUS)
        .border_1()
        .border_color(colors.border)
        .bg(colors.card)
        .text_color(colors.card_foreground)
        .when(!dark, |this| this.shadow(shadow::XS_5.to_vec()))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(shift)
                .bottom(-shift)
                .rounded(CARD_RADIUS - px(1.))
                .shadow(vec![inset_line(-shift, colors.bevel)]),
        )
}

fn inset_line(dy: Pixels, color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), dy),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: true,
    }
}

/// `SettingsRow`: title (13px semibold) with a 20px reset slot, a 12px description, an optional
/// 11px status line, and a control on the right. Rows with `children` (a textarea, a sub-list)
/// keep the top padding only.
#[derive(IntoElement)]
pub struct SettingsRow {
    title: AnyElement,
    description: Option<AnyElement>,
    status: Option<AnyElement>,
    reset: Option<AnyElement>,
    control: Option<AnyElement>,
    children: Vec<AnyElement>,
    nested: bool,
}

impl SettingsRow {
    pub fn new(title: impl IntoElement, description: impl IntoElement) -> Self {
        Self {
            title: title.into_any_element(),
            description: Some(description.into_any_element()),
            status: None,
            reset: None,
            control: None,
            children: Vec::new(),
            nested: false,
        }
    }

    /// The reset button, shown when the setting differs from its default.
    pub fn reset(mut self, reset: Option<impl IntoElement>) -> Self {
        self.reset = reset.map(IntoElement::into_any_element);
        self
    }

    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.control = Some(control.into_any_element());
        self
    }

    pub fn status(mut self, status: impl IntoElement) -> Self {
        self.status = Some(status.into_any_element());
        self
    }

    /// A dependent row (General > Start from origin): `muted/20` fill and a 36px left inset.
    pub fn nested(mut self) -> Self {
        self.nested = true;
        self
    }
}

impl ParentElement for SettingsRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SettingsRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let has_children = !self.children.is_empty();
        div()
            .px_5()
            .pt(px(14.))
            .when(!has_children, |this| this.pb(px(14.)))
            .when(self.nested, |this| {
                this.pl_9().bg(colors.muted.opacity(0.2))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .min_h_5()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .type_scale(under_xs(13.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(colors.foreground)
                                            .child(self.title),
                                    )
                                    .child(
                                        div()
                                            .size_5()
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .children(self.reset),
                                    ),
                            )
                            .children(self.description.map(|description| {
                                div()
                                    .type_scale(text::XS)
                                    .text_color(colors.muted_foreground_80)
                                    .child(description)
                            }))
                            .children(self.status.map(|status| {
                                div()
                                    .pt(px(2.))
                                    .type_scale(under_xs(11.))
                                    .text_color(colors.muted_foreground)
                                    .child(status)
                            })),
                    )
                    .children(self.control.map(|control| {
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(control)
                    })),
            )
            .children(self.children)
    }
}

/// `SettingResetButton`: a 20px ghost `Undo2` button (muted, `foreground` on hover) with the
/// "Reset to default" tooltip.
pub fn reset_button(
    id: impl Into<ElementId>,
    colors: &Colors,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let (hover_fg, hover_bg) = (colors.foreground, colors.accent);
    div()
        .id(id.into())
        .size_5()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(colors.muted_foreground)
        .hover(move |style| style.text_color(hover_fg).bg(hover_bg))
        .tooltip_text("Reset to default")
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            on_click(window, cx);
        })
        .child(Icon::new(IconName::Undo2).size(px(12.)))
}

/// A muted/foreground ghost icon button used in section header actions (refresh, add).
pub fn header_icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::IconXs)
        .icon(icon)
        .tooltip(tooltip)
}
