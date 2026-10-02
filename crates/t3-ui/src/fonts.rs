//! Font families. The fork renders its UI in the system font stack (`-apple-system, ...`) and
//! code in `ui-monospace, "SF Mono", ... Menlo` (`apps/web/src/index.css` `--font-sans` /
//! `--font-mono`, mirrored in `appearanceFonts.ts`), so nothing is bundled: on macOS the UI is
//! SF Pro and code is the system monospaced font (SF Mono), like Chromium's `ui-monospace`.

use gpui_kit::{App, SharedString, font};

use crate::tokens::font as families;

/// The monospace family to use: the first of [`families::MONO_CANDIDATES`] the text system
/// actually loads (the system monospaced font, then SF Mono, then Menlo). Resolving a family
/// loads it, so call this once at startup and keep the result ([`crate::Theme::mono_family`]).
pub fn mono_family(cx: &App) -> SharedString {
    let text_system = cx.text_system();
    families::MONO_CANDIDATES
        .iter()
        .find(|family| {
            let id = text_system.resolve_font(&font(**family));
            text_system
                .get_font_for_id(id)
                .is_some_and(|resolved| resolved.family.as_ref() == **family)
        })
        .copied()
        .unwrap_or(families::MONO)
        .into()
}
