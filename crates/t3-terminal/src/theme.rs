//! Terminal colors: the fork's xterm theme (`ThreadTerminalDrawer.tsx:128-200`) plus the
//! derived colors xterm computes from it (`ThemeService.ts`).

use alacritty_terminal::vte::ansi::Rgb;
use gpui_kit::{Hsla, Rgba, rgb, rgba};

/// Colors for one terminal. Start from [`TerminalTheme::dark`] or [`TerminalTheme::light`] and
/// pass the drawer surface's `background` / `foreground` tokens with
/// [`TerminalTheme::with_surface`], as the fork reads them from the drawer's computed style.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalTheme {
    background: Hsla,
    foreground: Hsla,
    cursor: Hsla,
    /// Text drawn on the block cursor. The fork sets none, so xterm's default black applies.
    cursor_accent: Hsla,
    selection_translucent: Rgba,
    /// `selection_translucent` blended over the background; xterm paints this opaque color.
    selection: Hsla,
    scrollbar_slider: Hsla,
    scrollbar_slider_hover: Hsla,
    scrollbar_slider_active: Hsla,
    /// 0-15 from the theme, 16-255 xterm's color cube and grayscale ramp.
    palette: [Hsla; 256],
}

/// Dark `--background` / `--foreground` (design-system.md §1.2).
const DARK_SURFACE: (u32, u32) = (0x161616, 0xf5f5f5);
/// Light `--background` / `--foreground`.
const LIGHT_SURFACE: (u32, u32) = (0xffffff, 0x262626);

const DARK_ANSI: [u32; 16] = [
    0x181e26, 0xff7a8e, 0x86e795, 0xf4cd72, 0x89beff, 0xd0b0ff, 0x7ce8ed, 0xd2dae6, 0x6e7888,
    0xffa8b4, 0xb0f5ba, 0xffe095, 0xaed2ff, 0xe5cbff, 0xa7f4f7, 0xf4f7fc,
];
const LIGHT_ANSI: [u32; 16] = [
    0x2c3542, 0xbf4657, 0x3c7e56, 0x927023, 0x4866a3, 0x845695, 0x357f8d, 0xd2d7df, 0x707b8c,
    0xd45f70, 0x55946f, 0xad852d, 0x5b7cc2, 0x996bac, 0x4695a4, 0xecf0f6,
];

impl TerminalTheme {
    /// The fork's dark terminal theme on the dark app surface.
    pub fn dark() -> Self {
        Self::build(
            DARK_SURFACE,
            0xb4cbff,
            rgba(0xb4cbff40),
            [rgba(0xffffff1a), rgba(0xffffff2e), rgba(0xffffff38)],
            DARK_ANSI,
        )
    }

    /// The fork's light terminal theme on the light app surface.
    pub fn light() -> Self {
        Self::build(
            LIGHT_SURFACE,
            0x26384e,
            rgba(0x253f6333),
            [rgba(0x00000026), rgba(0x00000040), rgba(0x0000004d)],
            LIGHT_ANSI,
        )
    }

    /// Uses the drawer surface's background and foreground, recomputing the selection blend.
    pub fn with_surface(
        mut self,
        background: impl Into<Rgba>,
        foreground: impl Into<Rgba>,
    ) -> Self {
        let background = background.into();
        self.background = background.into();
        self.foreground = foreground.into().into();
        self.selection = background.blend(self.selection_translucent).into();
        self
    }

    fn build(
        (background, foreground): (u32, u32),
        cursor: u32,
        selection: Rgba,
        [slider, slider_hover, slider_active]: [Rgba; 3],
        ansi: [u32; 16],
    ) -> Self {
        let mut palette = [Hsla::default(); 256];
        for (index, color) in palette.iter_mut().enumerate() {
            *color = match index {
                0..16 => rgb(ansi[index]).into(),
                _ => xterm_256(index as u8).into(),
            };
        }
        Self {
            background: rgb(background).into(),
            foreground: rgb(foreground).into(),
            cursor: rgb(cursor).into(),
            cursor_accent: rgb(0x000000).into(),
            selection_translucent: selection,
            selection: rgb(background).blend(selection).into(),
            scrollbar_slider: slider.into(),
            scrollbar_slider_hover: slider_hover.into(),
            scrollbar_slider_active: slider_active.into(),
            palette,
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
        if dragging {
            self.scrollbar_slider_active
        } else if hovered {
            self.scrollbar_slider_hover
        } else {
            self.scrollbar_slider
        }
    }

    pub(crate) fn palette(&self, index: u8) -> Hsla {
        self.palette[usize::from(index)]
    }

    /// The color an OSC 4/10/11/12 query reports for alacritty's color `index`
    /// (0-255 palette, 256 foreground, 257 background, 258 cursor).
    pub(crate) fn query_color(&self, index: usize) -> Rgb {
        let color = match index {
            0..=255 => self.palette[index],
            257 => self.background,
            258 => self.cursor,
            _ => self.foreground,
        };
        hsla_to_rgb(color)
    }
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self::dark()
    }
}

/// xterm's 256-color table above the 16 theme colors: a 6x6x6 cube, then 24 grays.
fn xterm_256(index: u8) -> Rgba {
    const LEVELS: [u32; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
    let hex = if index < 232 {
        let i = u32::from(index - 16);
        (LEVELS[(i / 36) as usize] << 16)
            | (LEVELS[(i / 6 % 6) as usize] << 8)
            | LEVELS[(i % 6) as usize]
    } else {
        let level = 8 + 10 * u32::from(index - 232);
        (level << 16) | (level << 8) | level
    };
    rgb(hex)
}

pub(crate) fn rgb_to_hsla(color: Rgb) -> Hsla {
    rgb((u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)).into()
}

fn hsla_to_rgb(color: Hsla) -> Rgb {
    let rgba = color.to_rgb();
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    Rgb {
        r: channel(rgba.r),
        g: channel(rgba.g),
        b: channel(rgba.b),
    }
}
