//! Main window chrome (spec section 2): window options matching the Electron
//! `BrowserWindow`, the macOS `NSVisualEffectView` glass behind GPUI's layer, and the 3.5%
//! grain overlay.
//!
//! Layer stack on macOS, bottom to top: native material, the main column's
//! `app_main_glass` tint, [`NoiseOverlay`], the sidebar's `app_sidebar_glass`, popups.

use std::sync::{Arc, LazyLock};

use gpui_kit::{
    App, Bounds, Image, ImageFormat, IntoElement, ParentElement as _, RenderOnce, Styled as _,
    TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions, div, img,
    point, size,
};

use crate::tokens::layout;

/// Window title. The reference fixes it to this string (`DesktopWindow.ts:22`); it shows in
/// the Window menu and Mission Control, never in the hidden titlebar.
pub const TITLE: &str = "HOME-PC";

/// Options for the main window: 1100x780 centered, min 840x620, hidden-inset titlebar with
/// the traffic lights at (16, 18), and a transparent background on macOS so
/// [`install_glass`] shows through. Other platforms stay opaque (spec risk 12).
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
        window_background: if cfg!(target_os = "macos") {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        },
        window_min_size: Some(size(min_width, min_height)),
        ..Default::default()
    }
}

/// Inserts the window glass under GPUI's view: an `NSVisualEffectView` with material
/// `underWindowBackground`, blending `behindWindow` and state `active` (stays live while the
/// window is inactive), resized with the window. This is Electron's `vibrancy: under-window`.
///
/// GPUI's own `WindowBackgroundAppearance::Blurred` uses the `selection` material and strips
/// the desktop tint, which looks flatter, so the window must be opened with `Transparent`
/// ([`main_window_options`]). Call once per window from the `open_window` build closure. No-op
/// on other platforms.
#[cfg(target_os = "macos")]
pub fn install_glass(window: &Window) {
    use objc2::{MainThreadMarker, MainThreadOnly as _};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
        NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // `Window::window_handle` (inherent) returns GPUI's handle; we want the raw one.
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: GPUI's AppKit handle points at its live content NSView, owned by the window
    // that `window` borrows, and we are on the main thread.
    let gpui_view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    // GPUI's view is a subview of the window's content view; the glass goes beneath it.
    // SAFETY: reading the superview of a live view on the main thread.
    let Some(content_view) = (unsafe { gpui_view.superview() }) else {
        return;
    };
    let glass =
        NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), content_view.bounds());
    glass.setMaterial(NSVisualEffectMaterial::UnderWindowBackground);
    glass.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    glass.setState(NSVisualEffectState::Active);
    glass.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content_view.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
}

/// No-op off macOS: those windows are opaque.
#[cfg(not(target_os = "macos"))]
pub fn install_glass(_window: &Window) {}

static NOISE_TILE: LazyLock<Arc<Image>> = LazyLock::new(|| {
    Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../assets/noise@2x.png").to_vec(),
    ))
});

/// The web's `body::after` grain: a static 256px fractal-noise tile repeated at 3.5% opacity.
///
/// Place it as the last child of the main column (`relative` parent), after the composer and
/// titlebar controls, but under the sidebar and popups (spec risk 8). It is a static texture:
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
