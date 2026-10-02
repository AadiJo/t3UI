//! coss `Separator`, `Skeleton` and `Spinner` (`ui/separator.tsx`, `ui/skeleton.tsx`,
//! `ui/spinner.tsx`).

use std::{cell::Cell, f32::consts::TAU, rc::Rc, time::Duration};

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, ElementId, IntoElement,
    ParentElement as _, Pixels, RenderOnce, StyleRefinement, Styled, Transformation, Window,
    canvas, div, linear_color_stop, linear_gradient, px, radians, transparent_black,
};

use crate::{ActiveColors as _, Icon, IconName, tokens::motion};

/// A 1px `border`-colored rule. Horizontal fills the width; vertical stretches to the
/// parent's height.
#[derive(IntoElement, Default)]
pub struct Separator {
    vertical: bool,
}

impl Separator {
    pub fn horizontal() -> Self {
        Self { vertical: false }
    }

    pub fn vertical() -> Self {
        Self { vertical: true }
    }
}

impl RenderOnce for Separator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = div().flex_none().bg(cx.colors().border);
        if self.vertical {
            line.w(px(1.)).self_stretch()
        } else {
            line.h(px(1.)).w_full()
        }
    }
}

/// Continuous animations (spinner, skeleton) tick at most [`TICK_FPS`] and only while their
/// element was inside its clip region on the previous frame: GPUI's throttled animation
/// re-renders the owning view each tick, so off-screen ones must stop scheduling ticks.
const TICK_FPS: f32 = 1000. / motion::CONTINUOUS_FRAME.as_millis() as f32;

/// Visibility flag written during prepaint, read on the next render.
fn visibility(id: &ElementId, window: &mut Window, cx: &mut App) -> (bool, Rc<Cell<bool>>) {
    let cell = window
        .use_keyed_state(ElementId::from((id.clone(), "visible")), cx, |_, _| {
            Rc::new(Cell::new(true))
        })
        .read(cx)
        .clone();
    (cell.get(), cell)
}

/// Absolute child that records whether its parent intersects the current clip.
fn visibility_probe(cell: Rc<Cell<bool>>) -> impl IntoElement {
    canvas(
        move |bounds: Bounds<Pixels>, window, _| {
            cell.set(window.content_mask().bounds.intersects(&bounds));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// The lucide `loader-circle` spinning once per second (`animate-spin`), 16px by default.
///
/// Frame-throttled to ~30 fps and paused while clipped out of view (see [`TICK_FPS`]).
#[derive(IntoElement)]
pub struct Spinner {
    id: ElementId,
    size: Pixels,
    style: StyleRefinement,
}

impl Spinner {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            size: px(16.),
            style: StyleRefinement::default(),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Spinner {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (visible, cell) = visibility(&self.id, window, cx);
        let icon = Icon::new(IconName::LoaderCircle).size(self.size);
        let icon: AnyElement = if visible {
            icon.with_animation(
                self.id.clone(),
                Animation::new(Duration::from_secs(1))
                    .repeat_synced()
                    .with_max_fps(TICK_FPS),
                |icon, turn| icon.transform(Transformation::rotate(radians(turn * TAU))),
            )
            .into_any_element()
        } else {
            icon.into_any_element()
        };
        let mut root = div()
            .relative()
            .flex_none()
            .size(self.size)
            .child(icon)
            .child(visibility_probe(cell));
        gpui_kit::Refineable::refine(root.style(), &self.style);
        root
    }
}

/// A loading placeholder: `rounded-sm` (6px) `muted` block with the coss highlight sweep
/// (`linear-gradient(120deg, transparent 40%, highlight, transparent 60%)`, 2s linear).
///
/// The sweep runs on the app's shared animation clock so every skeleton is in phase, like
/// the reference's viewport-fixed gradient, but each skeleton carries its own vertical band
/// (the 120deg tilt is not reproduced). It is
/// frame-throttled to ~30 fps and paused while clipped out of view (see [`TICK_FPS`]). Size
/// it with `Styled` (`.w(..).h(..)`).
#[derive(IntoElement)]
pub struct Skeleton {
    id: ElementId,
    style: StyleRefinement,
    animated: bool,
}

impl Skeleton {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            animated: true,
        }
    }

    /// Static `muted` fill without the sweep.
    pub fn still(mut self) -> Self {
        self.animated = false;
        self
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let (visible, cell) = visibility(&self.id, window, cx);
        // The CSS gradient is 200% of the viewport wide with the highlight in its middle
        // fifth, sliding one full tile (2 viewports) per cycle.
        let viewport = window.viewport_size().width;
        let band = viewport * 0.4;
        let half = |from, to| {
            div().flex_1().h_full().bg(linear_gradient(
                90.,
                linear_color_stop(from, 0.),
                linear_color_stop(to, 1.),
            ))
        };
        let highlight = colors.skeleton_highlight;
        let band_el = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(band)
            .flex()
            .child(half(transparent_black(), highlight))
            .child(half(highlight, transparent_black()));
        let sweep: Option<AnyElement> = (self.animated && visible).then(|| {
            band_el
                .with_animation(
                    self.id.clone(),
                    Animation::new(Duration::from_secs(2))
                        .repeat_synced()
                        .with_max_fps(TICK_FPS),
                    move |band_el, t| band_el.left(viewport * (2. * t - 1.) - band / 2.),
                )
                .into_any_element()
        });
        let mut root = div()
            .relative()
            .overflow_hidden()
            .rounded(px(6.))
            .bg(colors.muted)
            .children(sweep)
            .child(visibility_probe(cell));
        gpui_kit::Refineable::refine(root.style(), &self.style);
        root
    }
}
