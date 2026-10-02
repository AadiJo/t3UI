//! Design system for the T3 Code port: theme tokens, fonts, icons, window chrome and styled
//! primitives that reproduce the web app's coss UI (Base UI + Tailwind) components on top of
//! gpui-kit.
//!
//! Bootstrap: `gpui_kit::init(cx)` first, then [`init`]. Read colors with
//! `cx.colors()` ([`ActiveColors`]); never hard-code a color outside [`tokens`].

pub mod assets;
pub mod fonts;
pub mod icon;
pub mod theme;
pub mod tokens;

pub use assets::Assets;
pub use icon::{FileIcon, Icon, IconName, Logo, file_icon, logo};
pub use theme::{ActiveColors, Appearance, Theme, ThemeMode};
pub use tokens::Colors;

use gpui_kit::App;

/// Registers fonts and installs the theme. Call once after `gpui_kit::init(cx)`, before
/// opening windows. `mode` is the persisted [`ThemeMode`] (default `System`).
pub fn init(mode: ThemeMode, cx: &mut App) {
    if let Err(error) = fonts::register(cx) {
        tracing::error!("failed to register bundled fonts: {error:#}");
    }
    let mono_family = fonts::mono_family(cx);
    theme::init(mode, mono_family, cx);
}
