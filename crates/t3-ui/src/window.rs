//! Main window chrome: window options matching the fork's Electron `BrowserWindow`
//! (`apps/desktop/src/window/DesktopWindow.ts`) and the 3.5% surface grain.
//!
//! The window is opaque: the fork dropped vibrancy, and paints `--app-chrome-background` with
//! the grain layered behind content.

use std::sync::{Arc, LazyLock};

use gpui_kit::{
    App, Bounds, Image, ImageFormat, IntoElement, ParentElement as _, RenderOnce, Styled as _,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img,
    point, size,
};

use crate::tokens::layout;

/// Window title (the fork uses the build's display name); shown in the Window menu and Mission
/// Control, never in the hidden titlebar.
pub const TITLE: &str = "T3 Code";

/// Options for the main window: 1100x780 centered, min 840x620, opaque, hidden-inset titlebar
/// with the traffic lights at (16, 19), vertically centered in the 52px topbar.
pub fn main_window_options(cx: &App) -> WindowOptions {
    let (width, height) = layout::WINDOW_DEFAULT;
    let (min_width, min_height) = layout::WINDOW_MIN;
    let (lights_x, lights_y) = layout::TRAFFIC_LIGHTS;
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(width, height),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(lights_x, lights_y)),
        }),
        window_background: WindowBackgroundAppearance::Opaque,
        window_min_size: Some(size(min_width, min_height)),
        ..Default::default()
    }
}

static NOISE_TILE: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../assets/noise@2x.png").to_vec(),
    ))
});

/// The fork's `--surface-grain`: a static 256px fractal-noise tile repeated at 3.5% opacity.
///
/// The fork paints it *behind* content: as the body's background image and through the
/// `surface-grain` utility on surfaces that float over the body. So place this as the first
/// child of an opaque surface (`relative` parent), before its content. It is a static texture:
/// no animation, and the tiles share one GPU sprite.
#[derive(IntoElement, Default)]
pub struct NoiseOverlay;

impl RenderOnce for NoiseOverlay {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let viewport = window.viewport_size();
        let tile = layout::NOISE_TILE;
        let columns = (viewport.width / tile).ceil() as usize;
        let rows = (viewport.height / tile).ceil() as usize;
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .overflow_hidden()
            .opacity(layout::NOISE_OPACITY)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .w(tile * columns as f32)
                    .children((0..columns * rows).map(|_| img(NOISE_TILE.clone()).size(tile))),
            )
    }
}
