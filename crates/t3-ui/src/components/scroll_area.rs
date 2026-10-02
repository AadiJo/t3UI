//! coss `ScrollArea` (`ui/scroll-area.tsx`): a vertically scrolling region with gpui-base's
//! overlay scrollbar (6px thumb, hidden until hover or scroll), or no scrollbar at all.
//!
//! The thumb uses the native scrollbar colors from the theme (`#00000026` light,
//! `#FFFFFF1A` dark) rather than Base UI's `foreground/20`; GPUI themes one scrollbar style
//! app-wide.

use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window,
    component::scroll::ScrollableElement as _, div,
};

/// A scroll container. Size it with `Styled` (it is `size_full` by default) and keep its
/// parent's `min_h_0()` so it can shrink.
#[derive(IntoElement)]
pub struct ScrollArea {
    id: ElementId,
    hide_scrollbar: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl ScrollArea {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            hide_scrollbar: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// No scrollbar (the sidebar's thread list).
    pub fn hide_scrollbar(mut self) -> Self {
        self.hide_scrollbar = true;
        self
    }
}

impl ParentElement for ScrollArea {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for ScrollArea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ScrollArea {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut content = div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .children(self.children);
        gpui_kit::Refineable::refine(content.style(), &self.style);
        if self.hide_scrollbar {
            content.id(self.id).overflow_y_scroll().into_any_element()
        } else {
            content
                .overflow_y_scrollbar()
                .id(self.id)
                .into_any_element()
        }
    }
}
