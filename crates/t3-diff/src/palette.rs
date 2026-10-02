//! Every color the diff view and changed-files tree paint, derived from the app tokens with the
//! formulas of `@pierre/diffs`' stylesheet plus the T3 overrides in `DiffPanel.tsx`
//! (`DIFF_PANEL_UNSAFE_CSS`) and `index.css`.

use crate::color::Rgba;

/// Light or dark theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    /// Picks the light or dark value.
    fn pick<T>(self, light: T, dark: T) -> T {
        match self {
            Self::Light => light,
            Self::Dark => dark,
        }
    }
}

/// The app tokens the diff colors derive from (`docs/spec/tokens.json` > `color`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppTokens {
    pub background: Rgba,
    pub foreground: Rgba,
    pub card: Rgba,
    pub primary: Rgba,
    pub muted_foreground: Rgba,
    pub accent: Rgba,
    pub border: Rgba,
    pub input: Rgba,
    pub success: Rgba,
    pub destructive: Rgba,
}

impl AppTokens {
    /// Token values copied from `docs/spec/tokens.json`, used until `t3-ui` exposes its theme.
    pub fn from_spec(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Light => Self {
                background: Rgba::hex(0xFFFFFFFF),
                foreground: Rgba::hex(0x262626FF),
                card: Rgba::hex(0xFFFFFFFF),
                primary: Rgba::hex(0x0A0A0AFF),
                muted_foreground: Rgba::hex(0x686868FF),
                accent: Rgba::hex(0x0000000A),
                border: Rgba::hex(0x00000014),
                input: Rgba::hex(0x0000001A),
                success: Rgba::hex(0x00BC7DFF),
                destructive: Rgba::hex(0xFB2C36FF),
            },
            Appearance::Dark => Self {
                background: Rgba::hex(0x161616FF),
                foreground: Rgba::hex(0xF5F5F5FF),
                card: Rgba::hex(0x1B1B1BFF),
                primary: Rgba::hex(0xFFFFFFFF),
                muted_foreground: Rgba::hex(0x818181FF),
                accent: Rgba::hex(0xFFFFFF0A),
                border: Rgba::hex(0xFFFFFF0F),
                input: Rgba::hex(0xFFFFFF14),
                success: Rgba::hex(0x00BC7DFF),
                destructive: Rgba::hex(0xFB414AFF),
            },
        }
    }
}

/// Colors of a line kind in the code area: code cell, number cell, and their hover variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineColors {
    pub code: Rgba,
    pub number: Rgba,
    pub code_hover: Rgba,
    pub number_hover: Rgba,
    /// Line-number text.
    pub number_text: Rgba,
}

/// Resolved colors of the diff body (`DiffView`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffPalette {
    pub appearance: Appearance,
    /// `.diff-panel-viewport`: `mix(srgb, background 94%, card)`.
    pub viewport: Rgba,
    /// File card border (`--border`, translucent).
    pub card_border: Rgba,
    /// `--diffs-bg`: `mix(srgb, card 90%, background)`. Code, gutter and separator wrapper fill.
    pub surface: Rgba,
    /// Sticky file header: `mix(srgb, card 94%, foreground)`.
    pub header: Rgba,
    /// Header bottom border (`--border`).
    pub header_border: Rgba,
    /// Plain code text and header title (`--diffs-fg`, the Pierre theme foreground).
    pub text: Rgba,
    /// Header title on hover: `mix(srgb, foreground 84%, primary)`.
    pub title_hover: Rgba,
    /// Collapse button hover fill: `foreground/10`.
    pub button_hover: Rgba,
    /// Pierre theme git colors (`--diffs-addition-base` etc.).
    pub addition: Rgba,
    pub deletion: Rgba,
    pub modified: Rgba,
    /// "N unmodified lines" pill: `mix(srgb, background 95%, foreground)`.
    pub separator: Rgba,
    /// Separator label and context line numbers (`--diffs-fg-number`).
    pub separator_text: Rgba,
    pub context: LineColors,
    pub added: LineColors,
    pub deleted: LineColors,
    /// Light half of the dashed deletion bar (`--diffs-bg-deletion`).
    pub deletion_bar_gap: Rgba,
    /// Diagonal stripes of an empty split side (`--diffs-bg-buffer`).
    pub buffer_stripe: Rgba,
    /// Gutter cell next to an empty split side (`--diffs-bg-context-gutter`).
    pub buffer_gutter: Rgba,
    /// Raw-patch fallback text (`muted-foreground/90`) and reason (`muted-foreground/75`).
    pub raw_text: Rgba,
    pub raw_reason: Rgba,
    /// Raw-patch `<pre>` fill (`background/70`) and border (`border/70`).
    pub raw_fill: Rgba,
    pub raw_border: Rgba,
}

impl DiffPalette {
    pub fn new(tokens: &AppTokens, appearance: Appearance) -> Self {
        let weight = |light: f32, dark: f32| appearance.pick(light, dark);
        let pick = |light: Rgba, dark: Rgba| appearance.pick(light, dark);
        let bg = tokens.background;
        let fg = tokens.foreground;
        // `pierre-light` / `pierre-dark`: editor.foreground and gitDecoration colors.
        let text = pick(Rgba::rgb(0x0a0a0a), Rgba::rgb(0xfafafa));
        let addition = pick(Rgba::rgb(0x18a46c), Rgba::rgb(0x07c480));
        let deletion = pick(Rgba::rgb(0xd52c36), Rgba::rgb(0xff2e3f));
        let modified = Rgba::rgb(0x009fff);

        let surface = tokens.card.mix_srgb(0.90, bg);
        let number_text = text.mix_lab(0.65, surface);

        let hover_target = bg.mix_srgb(0.94, fg);
        let context_hover = surface.mix_lab(weight(0.97, 0.91), hover_target);
        let context = LineColors {
            code: surface,
            number: surface,
            code_hover: context_hover,
            number_hover: context_hover,
            number_text,
        };

        let change_colors = |tint: Rgba, base: Rgba, hover_weight: f32| {
            let code_target = bg.mix_srgb(0.92, tint);
            let number_target = bg.mix_srgb(0.88, tint);
            LineColors {
                code: surface.mix_lab(weight(0.88, 0.80), code_target),
                number: surface.mix_lab(weight(0.91, 0.85), number_target),
                code_hover: surface.mix_lab(hover_weight, code_target),
                number_hover: surface.mix_lab(hover_weight, number_target),
                number_text: base,
            }
        };

        let context_bg = bg.mix_srgb(0.97, fg);
        Self {
            appearance,
            viewport: bg.mix_srgb(0.94, tokens.card),
            card_border: tokens.border,
            surface,
            header: tokens.card.mix_srgb(0.94, fg),
            header_border: tokens.border,
            text,
            title_hover: fg.mix_srgb(0.84, tokens.primary),
            button_hover: fg.alpha(0.10),
            addition,
            deletion,
            modified,
            separator: bg.mix_srgb(0.95, fg),
            separator_text: number_text,
            context,
            added: change_colors(tokens.success, addition, weight(0.80, 0.70)),
            deleted: change_colors(tokens.destructive, deletion, weight(0.80, 0.75)),
            deletion_bar_gap: bg.mix_srgb(0.92, tokens.destructive),
            buffer_stripe: bg.mix_srgb(0.90, fg),
            buffer_gutter: context_bg.mix_lab(weight(0.90, 0.45), surface),
            raw_text: tokens.muted_foreground.alpha(0.90),
            raw_reason: tokens.muted_foreground.alpha(0.75),
            raw_fill: bg.alpha(0.70),
            raw_border: tokens.border.alpha(0.70),
        }
    }

    /// Colors of the spec fallback tokens.
    pub fn from_spec(appearance: Appearance) -> Self {
        Self::new(&AppTokens::from_spec(appearance), appearance)
    }
}

/// Resolved colors of the changed-files tree and its card (`ChangedFilesTree.tsx`,
/// `MessagesTimeline.tsx` `AssistantChangedFilesSectionInner`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreePalette {
    pub appearance: Appearance,
    /// Row hover fill: `accent/60`.
    pub row_hover: Rgba,
    /// Chevron (`muted-foreground/70`), and on row hover (`foreground/80`).
    pub chevron: Rgba,
    pub chevron_hover: Rgba,
    /// Folder icon: `muted-foreground/75`.
    pub folder: Rgba,
    /// Directory name `muted-foreground/90`, file name `muted-foreground/80`, both
    /// `foreground/90` on hover.
    pub directory_name: Rgba,
    pub file_name: Rgba,
    pub name_hover: Rgba,
    /// Fallback file icon (`muted-foreground/70`).
    pub file_icon: Rgba,
    /// `+N` (`success`) and `-N` (`destructive`).
    pub additions: Rgba,
    pub deletions: Rgba,
    /// Card: `border/80` border, `card/45` fill; header label `muted-foreground/65`.
    pub card_border: Rgba,
    pub card_fill: Rgba,
    pub card_label: Rgba,
}

impl TreePalette {
    pub fn new(tokens: &AppTokens, appearance: Appearance) -> Self {
        Self {
            appearance,
            row_hover: tokens.accent.alpha(0.60),
            chevron: tokens.muted_foreground.alpha(0.70),
            chevron_hover: tokens.foreground.alpha(0.80),
            folder: tokens.muted_foreground.alpha(0.75),
            directory_name: tokens.muted_foreground.alpha(0.90),
            file_name: tokens.muted_foreground.alpha(0.80),
            name_hover: tokens.foreground.alpha(0.90),
            file_icon: tokens.muted_foreground.alpha(0.70),
            additions: tokens.success,
            deletions: tokens.destructive,
            card_border: tokens.border.alpha(0.80),
            card_fill: tokens.card.alpha(0.45),
            card_label: tokens.muted_foreground.alpha(0.65),
        }
    }

    /// Colors of the spec fallback tokens.
    pub fn from_spec(appearance: Appearance) -> Self {
        Self::new(&AppTokens::from_spec(appearance), appearance)
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: a formula wired to the wrong token (e.g. card vs background), light/dark
    //! weights swapped, or a translucent token pre-composited by accident. The expected values
    //! are the ones `docs/spec/tokens.json` and Pierre's stylesheet resolve to in Chromium.
    use super::*;

    /// The tokens are 8-bit roundings of Chromium's float values, so derived colors may land
    /// one step away from the browser's.
    #[track_caller]
    fn assert_close(color: Rgba, expected: u32) {
        let actual = color.to_hex();
        let close = (0..4).all(|ix| {
            let channel = |value: u32| ((value >> (ix * 8)) & 0xff) as i32;
            (channel(actual) - channel(expected)).abs() <= 1
        });
        assert!(close, "{actual:08x} is not within 1 of {expected:08x}");
    }

    #[test]
    fn dark_surfaces_match_spec() {
        let palette = DiffPalette::from_spec(Appearance::Dark);
        assert_close(palette.viewport, 0x171717ff);
        assert_close(palette.surface, 0x1a1a1aff);
        assert_close(palette.header, 0x282828ff);
        assert_close(palette.card_border, 0xffffff0f);
        assert_close(palette.added.code, 0x191c1bff);
    }

    #[test]
    fn light_surfaces_match_spec() {
        let palette = DiffPalette::from_spec(Appearance::Light);
        assert_close(palette.viewport, 0xffffffff);
        assert_close(palette.surface, 0xffffffff);
        assert_close(palette.deleted.code, 0xfffdfdff);
        assert_close(palette.text, 0x0a0a0aff);
    }

    #[test]
    fn hover_is_stronger_than_rest() {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let palette = DiffPalette::from_spec(appearance);
            for colors in [palette.added, palette.deleted] {
                let distance = |color: Rgba| {
                    (color.r - palette.surface.r).abs()
                        + (color.g - palette.surface.g).abs()
                        + (color.b - palette.surface.b).abs()
                };
                assert!(distance(colors.code_hover) > distance(colors.code));
            }
        }
    }
}
