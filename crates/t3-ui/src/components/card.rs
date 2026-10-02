//! coss `Card` (`ui/card.tsx`): radius 18, `card` fill, `shadow-xs/5`, edge bevel, with
//! header / panel / footer parts whose padding collapses where parts meet.

use gpui_kit::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
};

use super::bevel;
use crate::{ActiveColors as _, tokens::shadow};

/// A card.
///
/// ```ignore
/// Card::new()
///     .header(CardHeader::new().title("Providers").description("Installed agents"))
///     .panel(CardPanel::new().child(list))
///     .footer(CardFooter::new().child(Button::new("add").label("Add")))
/// ```
#[derive(IntoElement, Default)]
pub struct Card {
    header: Option<CardHeader>,
    panel: Option<CardPanel>,
    footer: Option<CardFooter>,
    style: StyleRefinement,
}

impl Card {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn header(mut self, header: CardHeader) -> Self {
        self.header = Some(header);
        self
    }

    pub fn panel(mut self, panel: CardPanel) -> Self {
        self.panel = Some(panel);
        self
    }

    pub fn footer(mut self, footer: CardFooter) -> Self {
        self.footer = Some(footer);
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let radius = px(18.);
        let (has_header, has_panel, has_footer) = (
            self.header.is_some(),
            self.panel.is_some(),
            self.footer.is_some(),
        );
        let mut card = div()
            .relative()
            .flex()
            .flex_col()
            .rounded(radius)
            .border_1()
            .border_color(colors.border)
            .bg(colors.card)
            .text_color(colors.card_foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .shadow(shadow::XS_5.to_vec())
            .child(bevel(radius, colors.bevel, colors))
            .when_some(self.header, |this, header| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .p(px(24.))
                        .when(has_panel, |this| this.pb(px(16.)))
                        .when_some(header.title, |this, title| {
                            this.child(
                                div()
                                    .text_size(px(18.))
                                    .line_height(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                        })
                        .when_some(header.description, |this, description| {
                            this.child(div().text_color(colors.muted_foreground).child(description))
                        })
                        .children(header.children),
                )
            })
            .when_some(self.panel, |this, panel| {
                this.child(
                    div()
                        .flex_1()
                        .p(px(24.))
                        .when(has_header, |this| this.pt(px(0.)))
                        .when(has_footer, |this| this.pb(px(0.)))
                        .children(panel.children),
                )
            })
            .when_some(self.footer, |this, footer| {
                this.child(
                    div()
                        .flex()
                        .items_center()
                        .p(px(24.))
                        .when(has_panel, |this| this.pt(px(16.)))
                        .children(footer.children),
                )
            });
        gpui_kit::Refineable::refine(card.style(), &self.style);
        card
    }
}

/// Card header: title (18px semibold), description (14px muted), 6px gap.
#[derive(Default)]
pub struct CardHeader {
    title: Option<SharedString>,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl CardHeader {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl ParentElement for CardHeader {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Card body.
#[derive(Default)]
pub struct CardPanel {
    children: Vec<AnyElement>,
}

impl CardPanel {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for CardPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// Card footer (row, items centered).
#[derive(Default)]
pub struct CardFooter {
    children: Vec<AnyElement>,
}

impl CardFooter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for CardFooter {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}
