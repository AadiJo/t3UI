//! Composer geometry and the few Tailwind palette colors the composer uses that are not theme
//! tokens (chip and badge accents). Values are the reference CSS (`chat.md` sections 4-6).

use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, Stateful, Styled as _, div,
    prelude::FluentBuilder as _, px,
};
use t3_ui::{ActiveColors as _, Colors, tokens::hex};

/// `max-w-3xl`: the composer, banners, and branch toolbar column.
pub const COLUMN_MAX_WIDTH: Pixels = px(768.);
/// Editor font size and `leading-relaxed` line height.
pub const EDITOR_TEXT: Pixels = px(14.);
pub const EDITOR_LINE_HEIGHT: Pixels = px(22.75);
/// `min-h-11`.
pub const EDITOR_MIN_HEIGHT: Pixels = px(44.);
/// `max-h-50` is 200px; the editor grows by whole rows up to 8 (182px), then scrolls.
pub const EDITOR_MAX_ROWS: usize = 8;

/// Tailwind v4 palette colors the composer uses directly (sRGB of the oklch values).
pub mod palette {
    use super::{Hsla, hex};

    pub const FUCHSIA_300: Hsla = hex(0xF4A8FFFF);
    pub const FUCHSIA_500: Hsla = hex(0xE12AFBFF);
    pub const FUCHSIA_700: Hsla = hex(0xA800B7FF);
    pub const BLUE_400: Hsla = hex(0x51A2FFFF);
    pub const YELLOW_500: Hsla = hex(0xF0B100FF);
    pub const PURPLE_400: Hsla = hex(0xC27AFFFF);
    pub const RED_500: Hsla = hex(0xFB2C36FF);
}

/// `color/NN`: the token at NN% of its own alpha.
pub fn alpha(color: Hsla, factor: f32) -> Hsla {
    color.opacity(factor)
}

/// Text with letter spacing, which GPUI lacks: one box per character with `tracking` between
/// them (the panel's `tracking-[0.2em]` "PENDING APPROVAL"). Spaces keep their own width.
pub fn tracked_text(text: &str, tracking: Pixels) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .gap(tracking)
        .children(text.chars().map(|ch| {
            if ch == ' ' {
                div().w(tracking).into_any_element()
            } else {
                div()
                    .child(SharedString::from(ch.to_string()))
                    .into_any_element()
            }
        }))
}

/// The composer's ghost `xs` trigger (model, traits, branch, workspace): h24, radius 8, 12px
/// `muted-foreground/70`, hover `accent` fill with `foreground/80` text. `open` holds the
/// pressed look.
pub fn ghost_trigger(
    id: impl Into<gpui_kit::ElementId>,
    open: bool,
    colors: &Colors,
) -> Stateful<Div> {
    let rest = alpha(colors.muted_foreground, 0.7);
    let hover = alpha(colors.foreground, 0.8);
    let fill = colors.accent;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .h(px(24.))
        .px(px(8.))
        .gap(px(8.))
        .rounded(px(8.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(if open { hover } else { rest })
        .cursor_pointer()
        .when(open, |this| this.bg(fill))
        .hover(move |style| style.bg(fill).text_color(hover))
}

/// A vertical 1px rule `h` tall (`Separator orientation="vertical"`).
pub fn vertical_rule(height: Pixels, cx: &App) -> AnyElement {
    div()
        .flex_none()
        .w(px(1.))
        .h(height)
        .bg(cx.colors().border)
        .into_any_element()
}
