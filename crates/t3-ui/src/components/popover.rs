//! coss `Popover` (`ui/popover.tsx`): a bordered popover surface below its trigger (offset 4,
//! centered), fading in over 150ms and closing instantly. Open state, dismissal (Escape,
//! outside click) and focus restore come from gpui-base's `Popover`.

use gpui_kit::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, ElementId, FontWeight, IntoElement,
    ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window, base, div,
    prelude::FluentBuilder as _, px,
};

use super::popup_surface;
use crate::{
    ActiveColors as _,
    tokens::{motion, shadow},
};

/// Horizontal alignment of a popup against its trigger (Base UI `align`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
}

impl Align {
    /// Corner of the popup pinned below the trigger (side `bottom`).
    pub(crate) fn below(self) -> Anchor {
        match self {
            Self::Start => Anchor::TopLeft,
            Self::Center => Anchor::TopCenter,
            Self::End => Anchor::TopRight,
        }
    }
}

type TriggerBuilder = Box<dyn FnOnce(bool) -> AnyElement>;
type ContentBuilder = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// A click-to-open popover.
///
/// ```ignore
/// Popover::new("account")
///     .trigger(|open| Button::new("account-btn").label("Account").pressed(open).into_any_element())
///     .content(|_, _| PopoverPopup::new().child(PopoverTitle::new("Signed in")))
/// ```
/// The trigger closure receives whether the popup is open, so it can hold its pressed look.
/// `content` should return a [`PopoverPopup`] (or any surface); it is shown with the enter
/// fade. Elements inside can close the popover by dispatching `gpui_kit::base::actions::Cancel`.
#[derive(IntoElement)]
pub struct Popover {
    id: ElementId,
    align: Align,
    trigger: Option<TriggerBuilder>,
    content: Option<ContentBuilder>,
}

impl Popover {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            align: Align::Center,
            trigger: None,
            content: None,
        }
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    pub fn trigger(mut self, trigger: impl FnOnce(bool) -> AnyElement + 'static) -> Self {
        self.trigger = Some(Box::new(trigger));
        self
    }

    pub fn content<E: IntoElement>(
        mut self,
        content: impl FnOnce(&mut Window, &mut App) -> E + 'static,
    ) -> Self {
        self.content = Some(Box::new(move |window, cx| {
            content(window, cx).into_any_element()
        }));
        self
    }
}

impl RenderOnce for Popover {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let trigger = self.trigger;
        let content = self.content;
        base::Popover::new(self.id)
            .anchor(self.align.below())
            .offset(px(4.))
            .when_some(trigger, |this, trigger| {
                this.trigger_with(move |open, _, _| trigger(open))
            })
            .when_some(content, |this, content| {
                this.content(move |_, window, cx| {
                    div().child(content(window, cx)).with_animation(
                        ElementId::from((id, "enter")),
                        Animation::new(motion::POPUP_ENTER).with_easing(motion::ease_standard),
                        |this, delta| this.opacity(delta),
                    )
                })
            })
    }
}

/// The popover surface (radius 10, `shadow-lg/5`, 16px padding), or the compact
/// `tooltip_style` variant (radius 8, 12px text, `shadow-md/5`, 8x4 padding).
#[derive(IntoElement, Default)]
pub struct PopoverPopup {
    tooltip_style: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl PopoverPopup {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tooltip_style(mut self) -> Self {
        self.tooltip_style = true;
        self
    }
}

impl ParentElement for PopoverPopup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for PopoverPopup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for PopoverPopup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let mut surface = if self.tooltip_style {
            popup_surface(px(8.), colors)
                .shadow(shadow::MD_5.to_vec())
                .px(px(8.))
                .py(px(4.))
                .text_size(px(12.))
                .line_height(px(16.))
        } else {
            popup_surface(px(10.), colors)
                .p(px(16.))
                .text_size(px(14.))
                .line_height(px(20.))
        }
        .flex()
        .flex_col()
        .children(self.children);
        gpui_kit::Refineable::refine(surface.style(), &self.style);
        surface
    }
}

/// `PopoverTitle`: 18px semibold, `leading-none`.
#[derive(IntoElement)]
pub struct PopoverTitle(SharedString);

impl PopoverTitle {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for PopoverTitle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .text_size(px(18.))
            .line_height(px(18.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(self.0)
    }
}

/// `PopoverDescription`: 14px `muted-foreground`.
#[derive(IntoElement)]
pub struct PopoverDescription(SharedString);

impl PopoverDescription {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for PopoverDescription {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(cx.colors().muted_foreground)
            .child(self.0)
    }
}
