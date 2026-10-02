//! Render caching for stable blocks.
//!
//! Every top-level block of a document that no longer changes (a frozen streaming chunk, or the
//! whole message once it is complete) renders through its own [`BlockView`], embedded as a GPUI
//! cached view. A streaming update then rebuilds only the live tail: the cached blocks replay
//! their previous frame instead of rendering, laying out and painting again.
//!
//! A cached view is laid out from a definite size, so a block first renders inline once, which
//! records its margins and measures its laid-out size ([`UnitMetrics`]). From the next frame on,
//! while the message width matches that measurement, it is embedded cached at that size. GPUI's
//! cache is keyed by absolute bounds, so a block that moves (the timeline following the tail
//! when the tail grows a line) renders again, at the same cost as without caching.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui_kit::{
    AnyElement, App, Context, Entity, IntoElement, ParentElement as _, Pixels, Render, Size,
    Styled as _, WeakEntity, Window, canvas, div,
};

use crate::{Document, view::Markdown};

/// Identifies a block: its chunk's start offset and its index among the chunk's top-level blocks.
pub(crate) type UnitKey = (usize, usize);

/// A block's CSS margins and device-pixel rounding, as `render::Laid` reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LaidMetrics {
    pub own: (f32, f32),
    pub inner: (f32, f32),
    pub excess: f32,
}

/// What a block's last full render measured, shared with the elements that measure it.
#[derive(Default)]
pub(crate) struct UnitMetrics {
    pub laid: Cell<Option<LaidMetrics>>,
    /// The laid-out size from the last frame the block rendered.
    pub size: Cell<Option<Size<Pixels>>>,
    /// Text blocks before this one in the message, and how many this one contains. Text heights
    /// round alternately by this count, so a cached block is only valid at the same start.
    pub parity: Cell<(usize, usize)>,
    /// Message-global ids of the stateful blocks inside (code blocks, tables, details).
    pub block_ids: RefCell<Vec<usize>>,
}

pub(crate) struct Unit {
    pub document: Rc<Document>,
    pub view: Entity<BlockView>,
    pub metrics: Rc<UnitMetrics>,
}

/// Renders one cached block of a [`Markdown`]. All state stays on the markdown entity.
pub(crate) struct BlockView {
    pub markdown: WeakEntity<Markdown>,
    pub key: UnitKey,
}

impl Render for BlockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let key = self.key;
        let element = self.markdown.upgrade().and_then(|markdown| {
            markdown.update(cx, |markdown, cx| {
                crate::render::render_unit(markdown, key, window, cx)
            })
        });
        element.unwrap_or_else(|| div().into_any_element())
    }
}

/// Wraps a block's element so its laid-out size is recorded into `metrics`. When the size
/// changes after the block was embedded cached at the old size (the width changed), the markdown
/// renders again so the next frame uses the new size.
pub(crate) fn measured(
    element: AnyElement,
    metrics: Rc<UnitMetrics>,
    markdown: WeakEntity<Markdown>,
) -> AnyElement {
    div()
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .child(element)
        .child(
            canvas(
                move |bounds, _, cx: &mut App| {
                    let previous = metrics.size.replace(Some(bounds.size));
                    if previous.is_some_and(|previous| previous != bounds.size) {
                        cx.defer(move |cx| {
                            let _ = markdown.update(cx, |_, cx| cx.notify());
                        });
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}
