//! Colors and typography for chat markdown: `t3-ui` tokens plus the fixed geometry of the fork's
//! `.chat-markdown` rules (`index.css:696-1038`).

use gpui_kit::{App, Hsla, Pixels, SharedString, px, rgba};
use t3_ui::{Colors, Theme, tokens};

/// Every color the markdown renderer paints, for one appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownColors {
    pub is_dark: bool,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub border: Hsla,
    pub accent: Hsla,
    pub info_foreground: Hsla,
    /// `color-mix(in srgb, var(--border) 60%, transparent)`: table row rules.
    pub table_rule: Hsla,
    /// `color-mix(in srgb, var(--muted) 78%, var(--background))`: code block body.
    pub code_block_background: Hsla,
    /// `color-mix(in srgb, var(--foreground) 8%, transparent)`: pressed chrome buttons.
    pub chrome_pressed: Hsla,
    /// The browser's default text selection: there is no `::selection` rule, so Chromium paints
    /// the macOS highlight for the blue accent (design-system spec §1.6).
    pub selection: Hsla,
}

impl MarkdownColors {
    pub fn from_tokens(colors: &Colors) -> Self {
        Self {
            is_dark: colors.is_dark,
            foreground: colors.foreground,
            muted: colors.muted,
            muted_foreground: colors.muted_foreground,
            border: colors.border,
            accent: colors.accent,
            info_foreground: colors.info_foreground,
            table_rule: colors.border_mix_60,
            code_block_background: colors.codeblock_bg,
            chrome_pressed: colors.foreground_mix_8,
            selection: if colors.is_dark {
                rgba(0x3f638bff).into()
            } else {
                rgba(0xb3d7ffff).into()
            },
        }
    }
}

/// Colors plus font families. One per appearance; cheap to clone.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownStyle {
    pub colors: MarkdownColors,
    pub sans_family: SharedString,
    pub mono_family: SharedString,
}

impl MarkdownStyle {
    /// The active `t3-ui` theme's colors and fonts.
    pub fn from_theme(cx: &App) -> Self {
        let theme = Theme::global(cx);
        Self {
            colors: MarkdownColors::from_tokens(theme.colors()),
            sans_family: tokens::font::SANS.into(),
            mono_family: theme.mono_family().clone(),
        }
    }

    /// A fixed appearance, independent of the app theme (e.g. a preview of the other mode).
    pub fn for_appearance(dark: bool) -> Self {
        Self {
            colors: MarkdownColors::from_tokens(if dark { &tokens::DARK } else { &tokens::LIGHT }),
            sans_family: tokens::font::SANS.into(),
            mono_family: tokens::font::MONO.into(),
        }
    }
}

/// Fixed geometry from the fork's CSS, in px (1rem = 16px).
pub(crate) mod metrics {
    use super::*;

    /// `text-sm leading-relaxed`: the root font size and line height.
    pub const BODY_SIZE: Pixels = px(14.);
    pub const RELAXED: f32 = 1.625;
    /// `margin: 0.65rem 0` on p, ul, ol, blockquote, pre and table containers.
    pub const BLOCK_MARGIN: f32 = 10.4;
    /// Headings: `margin: 1.25rem 0 0.5rem`, line-height 1.3, weight 600.
    pub const HEADING_MARGIN_TOP: f32 = 20.;
    pub const HEADING_MARGIN_BOTTOM: f32 = 8.;
    pub const HEADING_LINE_HEIGHT: f32 = 1.3;
    /// `padding-left: 1.25rem` on lists.
    pub const LIST_INDENT: f32 = 20.;
    /// `li + li { margin-top: 0.25rem }`.
    pub const LIST_ITEM_GAP: f32 = 4.;
    /// Blockquote: `border-left: 2px`, `padding-left: 0.8rem`.
    pub const QUOTE_BORDER: f32 = 2.;
    pub const QUOTE_PADDING: f32 = 12.8;
    /// Inline code: 12px, `padding: 0.1rem 0.35rem`, 1px border, radius 6.
    pub const INLINE_CODE_SIZE: Pixels = px(12.);
    pub const INLINE_CODE_PADDING_X: f32 = 5.6;
    pub const INLINE_CODE_PADDING_Y: f32 = 1.6;
    pub const INLINE_CODE_RADIUS: f32 = 6.;
    /// Code block: radius 12, header `padding: 0.25rem 0.375rem 0.25rem 0.7rem`, 11px mono title,
    /// pre `padding: 0.8rem 0.9rem`, 12px code, `leading-snug` (1.375).
    pub const CODE_BLOCK_RADIUS: f32 = 12.;
    pub const CODE_HEADER_PADDING: [f32; 4] = [4., 6., 4., 11.2];
    pub const CODE_TITLE_SIZE: Pixels = px(11.);
    pub const CODE_TITLE_GAP: f32 = 6.4;
    pub const CODE_PADDING_Y: f32 = 12.8;
    pub const CODE_PADDING_X: f32 = 14.4;
    pub const CODE_SIZE: Pixels = px(12.);
    pub const SNUG: f32 = 1.375;
    /// Tables: 12px text, cells `padding: 0.45rem 0.75rem`, header `padding-block: 0.55rem`,
    /// cells capped at 24rem.
    pub const TABLE_SIZE: Pixels = px(12.);
    pub const CELL_PADDING_X: f32 = 12.;
    pub const CELL_PADDING_Y: f32 = 7.2;
    pub const HEAD_PADDING_Y: f32 = 8.8;
    pub const CELL_MAX_WIDTH: f32 = 384.;
    /// File chips: `px-1.5 py-px gap-1 rounded-md border`, 12px medium label, 14px icon.
    pub const CHIP_PADDING_X: f32 = 6.;
    pub const CHIP_PADDING_Y: f32 = 1.;
    pub const CHIP_GAP: f32 = 4.;
    pub const CHIP_ICON: f32 = 14.;
    pub const CHIP_LABEL_SIZE: Pixels = px(12.);
    pub const CHIP_RADIUS: f32 = 8.;
    /// Favicon: 14px, `margin-inline: 0.25em 0.2em`, `vertical-align: -0.125em`.
    pub const FAVICON: f32 = 14.;
}
