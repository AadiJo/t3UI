//! Bundled fonts: DM Sans (opsz 14 static instances, 400/500/600/700) and JetBrains Mono
//! (400/500), built by `crates/t3-ui/tools/build_fonts.py`.
//!
//! Mono font decision (spec risk 4): the web stack starts with "SF Mono", which only matches
//! when the user installed it as a regular font; macOS doesn't expose the system copy by that
//! name. So a default Mac renders the bundled JetBrains Mono, and [`mono_family`] reproduces
//! the stack by preferring "SF Mono" when it is installed.

use gpui_kit::{App, SharedString};

use crate::{assets::Embedded, tokens::font};

const FONT_FILES: &[&str] = &[
    "fonts/DMSans-Regular.ttf",
    "fonts/DMSans-Medium.ttf",
    "fonts/DMSans-SemiBold.ttf",
    "fonts/DMSans-Bold.ttf",
    "fonts/JetBrainsMono-Regular.ttf",
    "fonts/JetBrainsMono-Medium.ttf",
];

/// Registers the bundled fonts with the app's text system. Called by [`crate::init`].
pub fn register(cx: &mut App) -> anyhow::Result<()> {
    let fonts = FONT_FILES
        .iter()
        .map(|path| {
            Embedded::get(path)
                .map(|file| file.data)
                .ok_or_else(|| anyhow::anyhow!("missing bundled font {path}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    cx.text_system().add_fonts(fonts)
}

/// The monospace family to use: "SF Mono" if the user installed it, else JetBrains Mono.
/// Enumerates system fonts, so call it once at startup and keep the result.
pub fn mono_family(cx: &App) -> SharedString {
    let installed = cx
        .text_system()
        .all_font_names()
        .into_iter()
        .any(|name| name == font::MONO_PREFERRED);
    if installed {
        font::MONO_PREFERRED.into()
    } else {
        font::MONO.into()
    }
}
