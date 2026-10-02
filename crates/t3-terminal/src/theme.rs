//! Terminal colors for one frame, derived from the active t3-ui color set: the drawer surface
//! (`background` / `foreground`), the fork's xterm theme (`colors.terminal`,
//! `ThreadTerminalDrawer.tsx:128-200`), and the colors xterm derives from it (`ThemeService.ts`).

use alacritty_terminal::vte::ansi::Rgb;
use gpui_kit::{Hsla, rgb};
use t3_ui::Colors;

/// Resolved colors. Cheap to build, so the element builds one per frame from `cx.colors()`
/// and theme switches apply on the next paint, as the fork re-reads its theme on class change.
pub(crate) struct TerminalTheme {
    background: Hsla,
    foreground: Hsla,
    cursor: Hsla,
    /// Text drawn on the block cursor. The fork sets none, so xterm's default black applies.
    cursor_accent: Hsla,
    /// The selection color blended over the background; xterm paints this opaque color.
    selection: Hsla,
    scrollbar_slider: [Hsla; 3],
    ansi: [Hsla; 16],
}

impl TerminalTheme {
    pub(crate) fn new(colors: &Colors) -> Self {
        let t = &colors.terminal;
        Self {
            background: colors.background,
            foreground: colors.foreground,
            cursor: t.cursor,
            cursor_accent: rgb(0x000000).into(),
            selection: colors.background.blend(t.selection_background),
            scrollbar_slider: [
                t.scrollbar_slider_background,
                t.scrollbar_slider_hover_background,
                t.scrollbar_slider_active_background,
            ],
            ansi: [
                t.black,
                t.red,
                t.green,
                t.yellow,
                t.blue,
                t.magenta,
                t.cyan,
                t.white,
                t.bright_black,
                t.bright_red,
                t.bright_green,
                t.bright_yellow,
                t.bright_blue,
                t.bright_magenta,
                t.bright_cyan,
                t.bright_white,
            ],
        }
    }

    pub(crate) fn background(&self) -> Hsla {
        self.background
    }

    pub(crate) fn foreground(&self) -> Hsla {
        self.foreground
    }

    pub(crate) fn cursor(&self) -> Hsla {
        self.cursor
    }

    pub(crate) fn cursor_accent(&self) -> Hsla {
        self.cursor_accent
    }

    pub(crate) fn selection(&self) -> Hsla {
        self.selection
    }

    /// Slider color for the idle, hovered and dragged states.
    pub(crate) fn scrollbar_slider(&self, hovered: bool, dragging: bool) -> Hsla {
        let [idle, hover, active] = self.scrollbar_slider;
        if dragging {
            active
        } else if hovered {
            hover
        } else {
            idle
        }
    }

    /// 0-15 from the theme, 16-255 xterm's 6x6x6 color cube and 24-step gray ramp.
    pub(crate) fn palette(&self, index: u8) -> Hsla {
        const LEVELS: [u32; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        let hex = match index {
            0..16 => return self.ansi[usize::from(index)],
            16..232 => {
                let i = u32::from(index - 16);
                (LEVELS[(i / 36) as usize] << 16)
                    | (LEVELS[(i / 6 % 6) as usize] << 8)
                    | LEVELS[(i % 6) as usize]
            }
            _ => {
                let level = 8 + 10 * u32::from(index - 232);
                (level << 16) | (level << 8) | level
            }
        };
        rgb(hex).into()
    }

    /// The color an OSC 4/10/11/12 query reports for alacritty's color `index`
    /// (0-255 palette, 256 foreground, 257 background, 258 cursor).
    pub(crate) fn query_color(&self, index: usize) -> Rgb {
        let color = match u8::try_from(index) {
            Ok(index) => self.palette(index),
            Err(_) if index == 257 => self.background,
            Err(_) if index == 258 => self.cursor,
            Err(_) => self.foreground,
        };
        let rgba = color.to_rgb();
        let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
        Rgb {
            r: channel(rgba.r),
            g: channel(rgba.g),
            b: channel(rgba.b),
        }
    }
}

pub(crate) fn rgb_to_hsla(color: Rgb) -> Hsla {
    rgb((u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)).into()
}
