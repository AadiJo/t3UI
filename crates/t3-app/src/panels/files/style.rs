//! Colors and metrics of the files surfaces, from `FileBrowserPanel.tsx` (`TREE_UNSAFE_CSS`
//! over `@pierre/trees`' stylesheet, compact density 0.8) and `FilePreviewPanel.tsx` (Pierre
//! `File` with the `pierre-dark` / `pierre-light` theme). Measured against the fork in Chromium.

use gpui_kit::{App, Hsla, Pixels, Rgba as GpuiRgba, SharedString, px};
use t3_diff::color::Rgba;
use t3_ui::{ActiveColors as _, Colors};

/// `--trees-density` of the compact preset.
const DENSITY: f32 = 0.8;
/// Row height of the compact preset.
pub const ROW_HEIGHT: Pixels = px(24.);
/// Tree text: sans 12px on the 24px row.
pub const TREE_TEXT: Pixels = px(12.);
/// `--trees-padding-inline`: the scroll area's inset on each side, rows included.
pub const TREE_INSET: Pixels = px(16.);
/// `--trees-item-padding-x`.
pub const ITEM_PADDING: Pixels = px(8. * DENSITY);
/// `--trees-item-row-gap`: between the indent, icon and name, and under the search box.
pub const ITEM_GAP: Pixels = px(6. * DENSITY);
/// `--trees-icon-width`.
pub const ICON: Pixels = px(16.);
/// Indent per level: the guide slot (`icon/2 - 0.5`, the 1px guide, `level-gap - 1`) plus
/// the row gap. 18.7px.
pub const LEVEL_INDENT: Pixels = px(8. - 0.5 + 1. + (8. * DENSITY - 1.) + 6. * DENSITY);
/// x of the first indent guide from the row's padding edge (`icon/2 - 0.5`, nudged -0.25px).
pub const GUIDE_OFFSET: Pixels = px(7.25);
/// `button[data-type='item'] { border-radius: 5px }`.
pub const ROW_RADIUS: Pixels = px(5.);
/// Search box: 24px content + 1px padding + 1px border on each side, 1px block margin.
pub const SEARCH_HEIGHT: Pixels = px(28.);
pub const SEARCH_RADIUS: Pixels = px(6. * DENSITY);
/// `--trees-icon-nudge`: the down chevron sits this much lower.
pub const CHEVRON_NUDGE: Pixels = px(DENSITY);

/// Code: mono 13px on 20px lines, 8px above the first and below the last line.
pub const CODE_SIZE: Pixels = px(13.);
pub const CODE_LINE_HEIGHT: Pixels = px(20.);
pub const CODE_PADDING_Y: Pixels = px(8.);
/// The number column's right border in the code background.
pub const GUTTER_BORDER: Pixels = px(2.);
/// Pierre's `tokenizeMaxLineLength`: longer lines stay plain.
pub const TOKENIZE_MAX_LINE_LENGTH: usize = 1000;
/// Pierre's `tokenizeMaxLength` in lines.
pub const TOKENIZE_MAX_LINES: usize = 100_000;
/// CSS `tab-size`.
pub const TAB_SIZE: usize = 2;

/// UI sans family (`--font-sans`).
pub const SANS: &str = t3_ui::tokens::font::SANS;

/// Resolved colors of the browser and the code view.
#[derive(Clone, Copy, Debug)]
pub struct FilesPalette {
    pub dark: bool,
    /// `--trees-fg-muted`: chevrons and the indent guides' base.
    pub tree_muted: Hsla,
    /// Row hover (`currentColor 7%`) and selection (`currentColor 12%`).
    pub row_hover: Hsla,
    pub row_selected: Hsla,
    /// Indent guide (`--trees-indent-guide-bg`) at rest-on-hover (75%) and emphasized.
    pub guide_hover: Hsla,
    pub guide_focus: Hsla,
    /// `--trees-accent`, the focus ring.
    pub focus_ring: Hsla,
    pub search_bg: Hsla,
    pub search_border: Hsla,
    pub search_placeholder: Hsla,
    /// Pierre theme editor background and plain text.
    pub code_bg: Hsla,
    pub code_text: Hsla,
    /// Line numbers (`editorLineNumber.foreground`).
    pub line_number: Hsla,
    /// The revealed line (`data-file-link-reveal`): code cell, number cell, number text.
    pub reveal_code: Hsla,
    pub reveal_number: Hsla,
    pub reveal_number_text: Hsla,
    /// Truncation banner: `amber-500/8` fill, `amber-500/20` border, amber-300/700 text.
    pub banner_bg: Hsla,
    pub banner_border: Hsla,
    pub banner_text: Hsla,
}

impl FilesPalette {
    pub fn new(colors: &Colors) -> Self {
        let dark = colors.is_dark;
        let pick =
            |light: u32, dark_value: u32| hsla(Rgba::hex(if dark { dark_value } else { light }));
        let foreground = rgba(colors.foreground);
        let muted = Rgba::rgb(0x84848a);
        let search_bg = if dark {
            Rgba::rgb(0x070707)
        } else {
            Rgba::rgb(0xf8f8f8)
        };
        let amber = Rgba::rgb(0xfe9a00);
        Self {
            dark,
            tree_muted: hsla(muted),
            row_hover: hsla(foreground.alpha(0.07)),
            row_selected: hsla(foreground.alpha(0.12)),
            guide_hover: hsla(muted.alpha(0.25 * 0.75)),
            guide_focus: hsla(muted.alpha(0.25)),
            focus_ring: hsla(Rgba::rgb(0x009fff)),
            search_bg: hsla(search_bg),
            search_border: hsla(foreground.alpha(0.14)),
            search_placeholder: hsla(foreground.mix_lab(0.65, search_bg)),
            code_bg: pick(0xffffffff, 0x0a0a0aff),
            code_text: pick(0x0a0a0aff, 0xfafafaff),
            line_number: hsla(Rgba::rgb(0x737373)),
            // `color-mix(in lab, line bg 82%/75%, selection)` and `75%/60%` for the number cell.
            reveal_code: pick(0xe1edffff, 0x1c2d40ff),
            reveal_number: pick(0xd5e6ffff, 0x224262ff),
            reveal_number_text: pick(0x20649eff, 0x72b6ffff),
            banner_bg: hsla(amber.alpha(0.08)),
            banner_border: hsla(amber.alpha(0.2)),
            banner_text: pick(0xbb4d00ff, 0xffd230ff),
        }
    }

    pub fn current(cx: &App) -> Self {
        Self::new(cx.colors())
    }
}

/// Code family (`--font-mono`): SF Mono when installed, else the bundled JetBrains Mono.
pub fn mono_family(cx: &App) -> SharedString {
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

fn hsla(color: Rgba) -> Hsla {
    GpuiRgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
    .into()
}
