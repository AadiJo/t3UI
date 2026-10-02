//! Fonts and geometry of the diff body, from `@pierre/diffs`' stylesheet with the T3 overrides
//! (`DIFF_PANEL_UNSAFE_CSS` in `DiffPanel.tsx`) and the CodeView layout options.

use gpui_kit::{App, Hsla, Pixels, Rgba as GpuiRgba, SharedString, Window, font, px};

use crate::{
    color::Rgba,
    palette::{AppTokens, Appearance},
};

/// UI sans family (`--font-sans`), registered by `t3-ui`.
pub(crate) const SANS: &str = t3_ui::tokens::font::SANS;

/// The app tokens and appearance of the active `t3-ui` theme.
pub(crate) fn theme_tokens(cx: &App) -> (AppTokens, Appearance) {
    let theme = t3_ui::Theme::global(cx);
    let colors = theme.colors();
    let appearance = if theme.appearance().is_dark() {
        Appearance::Dark
    } else {
        Appearance::Light
    };
    let tokens = AppTokens {
        background: rgba(colors.background),
        foreground: rgba(colors.foreground),
        card: rgba(colors.card),
        primary: rgba(colors.primary),
        muted_foreground: rgba(colors.muted_foreground),
        accent: rgba(colors.accent),
        border: rgba(colors.border),
        input: rgba(colors.input),
        success: rgba(colors.success),
        destructive: rgba(colors.destructive),
    };
    (tokens, appearance)
}

/// Code family (`--font-mono`): SF Mono when installed, else the bundled JetBrains Mono.
pub(crate) fn mono_family(cx: &App) -> SharedString {
    t3_ui::Theme::global(cx).mono_family().clone()
}

fn rgba(color: Hsla) -> Rgba {
    let color = GpuiRgba::from(color);
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

/// Code text: 13px on a 20px line.
pub(crate) const CODE_SIZE: Pixels = px(13.);
pub(crate) const LINE_HEIGHT: Pixels = px(20.);
/// File header: sans 12px, 6px block padding, 16px inline padding, 20px collapse button and a
/// 1px bottom border, so 33px tall.
pub(crate) const HEADER_TEXT_SIZE: Pixels = px(12.);
pub(crate) const HEADER_HEIGHT: Pixels = px(33.);
/// Header line counts: mono 11px.
pub(crate) const COUNT_SIZE: Pixels = px(11.);
/// Space above every file card (CodeView `gap` / `paddingTop`) and below the last.
pub(crate) const CARD_GAP: Pixels = px(8.);
/// Code area bottom padding inside a card.
pub(crate) const BODY_BOTTOM: Pixels = px(8.);
/// "N unmodified lines" row: 32px with 8px margins (none above a file's first hunk).
pub(crate) const SEPARATOR_HEIGHT: Pixels = px(32.);
pub(crate) const SEPARATOR_MARGIN: Pixels = px(8.);
pub(crate) const SEPARATOR_INSET: Pixels = px(8.);
pub(crate) const SEPARATOR_RADIUS: Pixels = px(6.);
/// Change bar at the left edge of a line number.
pub(crate) const BAR_WIDTH: Pixels = px(4.);
/// `border-right: 2px solid var(--diffs-bg)` between gutter and code.
pub(crate) const GUTTER_BORDER: Pixels = px(2.);
/// Pierre's `tokenizeMaxLineLength`: longer lines render as plain text.
pub(crate) const TOKENIZE_MAX_LINE_LENGTH: usize = 1000;
/// CSS `tab-size`.
pub(crate) const TAB_SIZE: usize = 2;

/// CSS `ch` widths (advance of `0`) of the fonts the diff uses, measured once per window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    /// Mono 13px: gutter padding, number width, code padding.
    pub code_ch: Pixels,
    /// Sans 13px: separator label padding.
    pub separator_ch: Pixels,
    /// Sans 12px: gap between the header's line counts.
    pub header_ch: Pixels,
}

impl Metrics {
    pub(crate) fn measure(window: &Window, mono: &SharedString) -> Self {
        let text_system = window.text_system();
        let ch = |family: SharedString, size: Pixels| {
            let font_id = text_system.resolve_font(&font(family));
            text_system.ch_advance(font_id, size).unwrap_or(size * 0.6)
        };
        Self {
            code_ch: ch(mono.clone(), CODE_SIZE),
            separator_ch: ch(SANS.into(), CODE_SIZE),
            header_ch: ch(SANS.into(), HEADER_TEXT_SIZE),
        }
    }

    /// Width of a line-number column for numbers of `digits` digits:
    /// `padding-left: 2ch`, content `digits ch`, `padding-right: 1ch`.
    pub(crate) fn gutter_width(&self, digits: usize) -> Pixels {
        self.code_ch * (digits + 3) as f32
    }
}

/// Converts a palette color for GPUI.
pub(crate) fn gpui(color: Rgba) -> GpuiRgba {
    GpuiRgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}
