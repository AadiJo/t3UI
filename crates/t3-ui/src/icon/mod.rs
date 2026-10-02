//! Icons: single-color lucide/indicator icons ([`Icon`]), multi-color logos ([`logo`]) and
//! file-type icons ([`file_icon`]). Assets are exported from the reference UI; the typed
//! tables in `generated.rs` come from `tools/gen_icons.py`.

mod generated;

use gpui_kit::{
    App, Hsla, ImageSource, IntoElement, Pixels, Refineable as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Svg, Transformation, Window, img, prelude::FluentBuilder as _, px,
    svg,
};

use generated::{
    COMPLETE_EXTENSION_OVERRIDES, EXTENSION_TOKENS, FILE_NAME_TOKENS, T3_EXTENSION_ICONS,
    T3_FILE_ICONS, TOKEN_COLORS,
};
pub use generated::{IconName, Logo};

use crate::tokens::hex;

/// A single-color icon (`svg()` mask). Defaults to 16px in the inherited text color, like
/// lucide's `currentColor`.
///
/// ```ignore
/// Icon::new(IconName::Search).size(px(14.)).color(cx.colors().muted_foreground)
/// ```
#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Pixels,
    color: Option<Hsla>,
    transformation: Option<Transformation>,
    style: StyleRefinement,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: px(16.),
            color: None,
            transformation: None,
            style: StyleRefinement::default(),
        }
    }

    /// Edge length of the square icon box.
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    /// Paint color; defaults to the inherited text color.
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// Rotates or scales the glyph without changing layout (used by [`crate::Spinner`]).
    pub fn transform(mut self, transformation: Transformation) -> Self {
        self.transformation = Some(transformation);
        self
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let color = self.color.unwrap_or(window.text_style().color);
        let mut element: Svg = svg()
            .path(self.name.path())
            .flex_none()
            .size(self.size)
            .text_color(color)
            .when_some(self.transformation, |this, transformation| {
                this.with_transformation(transformation)
            });
        element.style().refine(&self.style);
        element
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Self::new(name)
    }
}

/// A brand logo for the given appearance, `size`×`size`. Multi-color logos render as an
/// `img()`; single-color ones ([`Logo::is_single_color`]) as an `svg()` mask in the inherited
/// text color, like the fork's `fill-current` icons.
pub fn logo(logo: Logo, dark: bool, size: Pixels) -> LogoImage {
    LogoImage { logo, dark, size }
}

/// Element returned by [`logo`].
#[derive(IntoElement)]
pub struct LogoImage {
    logo: Logo,
    dark: bool,
    size: Pixels,
}

impl RenderOnce for LogoImage {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let path = self.logo.path(self.dark);
        if self.logo.is_single_color() {
            svg()
                .path(path)
                .flex_none()
                .size(self.size)
                .text_color(window.text_style().color)
                .into_any_element()
        } else {
            img(path).flex_none().size(self.size).into_any_element()
        }
    }
}

/// How to draw a file-type icon resolved by [`file_icon`].
#[derive(Clone, Debug, PartialEq)]
pub enum FileIcon {
    /// Single-color Pierre icon: render with `svg().path(path).text_color(color)`.
    Mask { path: SharedString, color: Hsla },
    /// Multi-color T3 override: render with `img(path)`.
    Image { path: SharedString },
}

impl FileIcon {
    /// Renders the icon at `size` (the file tree uses 16px).
    pub fn render(&self, size: Pixels) -> gpui_kit::AnyElement {
        match self {
            Self::Mask { path, color } => svg()
                .path(path.clone())
                .flex_none()
                .size(size)
                .text_color(*color)
                .into_any_element(),
            Self::Image { path } => img(ImageSource::from(path.clone()))
                .flex_none()
                .size(size)
                .into_any_element(),
        }
    }
}

/// Muted tree icon color (`--trees-fg-muted`) for tokens without their own color.
const MUTED_FILE_ICON: Hsla = hex(0x84848AFF);

/// Resolves the icon the reference file tree shows for `file_name` (a base name or path),
/// following `@pierre/trees`' resolver with the T3 overrides from `pierre-icons.ts`.
pub fn file_icon(file_name: &str, dark: bool) -> FileIcon {
    let base = file_name.rsplit('/').next().unwrap_or(file_name);
    let lower = base.to_lowercase();

    let t3_image = |stem: &str, themed: bool| {
        let path = if themed {
            format!(
                "icons/files/{stem}.{}.svg",
                if dark { "dark" } else { "light" }
            )
        } else {
            format!("icons/files/{stem}.svg")
        };
        FileIcon::Image { path: path.into() }
    };
    // `a.test.ts` tries `test.ts`, then `ts`.
    let extensions = || lower.match_indices('.').map(|(ix, _)| &lower[ix + 1..]);

    // Pierre's resolver order: custom file names, custom extensions, then the built-ins.
    if let Some((_, stem, themed)) = T3_FILE_ICONS.iter().find(|(name, _, _)| *name == lower) {
        return t3_image(stem, *themed);
    }
    if let Some((_, stem, themed)) = extensions().find_map(|extension| {
        T3_EXTENSION_ICONS
            .iter()
            .find(|(ext, _, _)| *ext == extension)
    }) {
        return t3_image(stem, *themed);
    }

    let token = lookup(FILE_NAME_TOKENS, &lower)
        .or_else(|| {
            extensions().find_map(|extension| {
                lookup(COMPLETE_EXTENSION_OVERRIDES, extension)
                    .or_else(|| lookup(EXTENSION_TOKENS, extension))
            })
        })
        .unwrap_or("default");

    let color = TOKEN_COLORS
        .binary_search_by(|(name, _, _)| name.cmp(&token))
        .map(|ix| {
            let (_, light, dark_color) = TOKEN_COLORS[ix];
            if dark { dark_color } else { light }
        })
        .unwrap_or(MUTED_FILE_ICON);
    FileIcon::Mask {
        path: format!("icons/files/{token}.svg").into(),
        color,
    }
}

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table
        .binary_search_by(|(name, _)| (*name).cmp(key))
        .ok()
        .map(|ix| table[ix].1)
}
