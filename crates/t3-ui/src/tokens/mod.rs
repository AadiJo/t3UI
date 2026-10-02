//! Design tokens transcribed from the reference web UI (`docs/spec/tokens.json`).
//!
//! Colors, radii, shadows and the type scale are generated into [`generated`] by
//! `python3 crates/t3-ui/tools/gen_tokens.py`; motion and layout constants live here.
//! Read colors through [`crate::ActiveColors`] (`cx.colors()`) so they follow the theme.

mod generated;

use std::time::Duration;

use gpui_kit::{Hsla, Pixels, px};

pub use generated::{
    Colors, DARK, LIGHT, PROVIDER_ACCENTS, StatusColor, StatusColors, TerminalColors, radius,
    shadow, text,
};

/// Converts `0xRRGGBBAA` (sRGB, straight alpha) to [`Hsla`] exactly as GPUI's
/// `Hsla::from(rgba(..))` does, but usable in `const` items.
pub const fn hex(rgba: u32) -> Hsla {
    let r = ((rgba >> 24) & 0xFF) as f32 / 255.0;
    let g = ((rgba >> 16) & 0xFF) as f32 / 255.0;
    let b = ((rgba >> 8) & 0xFF) as f32 / 255.0;
    let a = (rgba & 0xFF) as f32 / 255.0;

    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let delta = max - min;
    let l = (max + min) / 2.0;
    let s = if l == 0.0 || l == 1.0 {
        0.0
    } else if l < 0.5 {
        delta / (2.0 * l)
    } else {
        delta / (2.0 - 2.0 * l)
    };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        let sector = (g - b) / delta;
        (if sector < 0.0 { sector + 6.0 } else { sector }) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    Hsla { h, s, l, a }
}

/// Font families registered by [`crate::fonts::register`].
pub mod font {
    /// UI sans family (DM Sans, opsz 14 static instances).
    pub const SANS: &str = "DM Sans";
    /// Bundled monospace family. [`crate::fonts::mono_family`] prefers SF Mono when the
    /// user installed it, matching the web font stack.
    pub const MONO: &str = "JetBrains Mono";
    /// First choice of the web mono stack; only present if the user installed it.
    pub const MONO_PREFERRED: &str = "SF Mono";
}

/// Motion timings (spec section 8). Durations are the reference CSS values.
pub mod motion {
    use super::Duration;

    /// Tailwind default transition: 150ms `cubic-bezier(.4,0,.2,1)`.
    pub const DEFAULT: Duration = Duration::from_millis(150);
    /// Workspace panels and sidebar collapse: 180ms, same curve.
    pub const PANEL: Duration = Duration::from_millis(180);
    /// Tooltip open delay (Base UI default). Close delay is 0.
    pub const TOOLTIP_DELAY: Duration = Duration::from_millis(600);
    /// Tooltip / popover enter (scale 0.98 + fade).
    pub const POPUP_ENTER: Duration = Duration::from_millis(150);
    /// Dialog popup and backdrop enter/exit.
    pub const DIALOG: Duration = Duration::from_millis(200);
    /// Sheet slide (32px) + fade.
    pub const SHEET: Duration = Duration::from_millis(180);
    /// Toast transform/opacity.
    pub const TOAST: Duration = Duration::from_millis(500);
    /// Toast auto-dismiss timeout.
    pub const TOAST_TIMEOUT: Duration = Duration::from_millis(5000);
    /// Switch track color.
    pub const SWITCH_TRACK: Duration = Duration::from_millis(200);
    /// Switch thumb translate.
    pub const SWITCH_THUMB: Duration = Duration::from_millis(150);
    /// Scale applied to popups at the start of their enter animation.
    pub const POPUP_FROM_SCALE: f32 = 0.98;
    /// Frame interval for continuous animations (spinner, skeleton): ~30 fps.
    pub const CONTINUOUS_FRAME: Duration = Duration::from_millis(33);

    /// `cubic-bezier(.4,0,.2,1)`, Tailwind's default easing.
    pub fn ease_standard(t: f32) -> f32 {
        cubic_bezier(0.4, 0.0, 0.2, 1.0, t)
    }

    /// `cubic-bezier(.22,1,.36,1)`, the toast curve.
    pub fn ease_toast(t: f32) -> f32 {
        cubic_bezier(0.22, 1.0, 0.36, 1.0, t)
    }

    /// Evaluates a CSS `cubic-bezier(x1, y1, x2, y2)` timing function at progress `t`.
    pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let bezier = |a: f32, b: f32, s: f32| {
            let inv = 1.0 - s;
            3.0 * inv * inv * s * a + 3.0 * inv * s * s * b + s * s * s
        };
        // Solve x(s) = t for s with bisection; 20 steps is far below a pixel.
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if bezier(x1, x2, mid) < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        bezier(y1, y2, (lo + hi) / 2.0)
    }
}

/// Window and chrome geometry (spec sections 2 and 7).
pub mod layout {
    use super::{Pixels, px};

    /// Default main window size.
    pub const WINDOW_DEFAULT: (Pixels, Pixels) = (px(1100.), px(780.));
    /// Minimum main window size.
    pub const WINDOW_MIN: (Pixels, Pixels) = (px(840.), px(620.));
    /// macOS traffic light origin (`trafficLightPosition`).
    pub const TRAFFIC_LIGHTS: (Pixels, Pixels) = (px(16.), px(18.));
    /// Topbar height on macOS (`--workspace-topbar-height`).
    pub const TOPBAR_HEIGHT: Pixels = px(52.);
    /// Sidebar toggle x on macOS, clearing the traffic lights.
    pub const CONTROLS_LEFT: Pixels = px(90.);
    /// Titlebar toggle size.
    pub const TITLEBAR_CONTROL: Pixels = px(28.);
    /// Default sidebar width.
    pub const SIDEBAR_WIDTH: Pixels = px(256.);
    /// Minimum sidebar width.
    pub const SIDEBAR_MIN_WIDTH: Pixels = px(208.);
    /// Subheader row height (`.surface-subheader`).
    pub const SUBHEADER_HEIGHT: Pixels = px(40.);
    /// Noise overlay opacity over the main column.
    pub const NOISE_OPACITY: f32 = 0.035;
    /// Noise tile edge length.
    pub const NOISE_TILE: Pixels = px(256.);
}

/// Opacity of disabled coss primitives.
pub const DISABLED_OPACITY: f32 = 0.64;
/// Opacity of icons inside buttons, menus, inputs and badges.
pub const ICON_OPACITY: f32 = 0.8;
/// 1px, the border width of every control.
pub const HAIRLINE: Pixels = px(1.);
