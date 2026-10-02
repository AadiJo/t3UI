//! coss `Badge` (`ui/badge.tsx`) and `Kbd` (`ui/kbd.tsx`).

use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, IntoElement, ParentElement, Pixels, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, px,
    transparent_black,
};

use crate::{ActiveColors as _, Icon, IconName, tokens::ICON_OPACITY};

/// Badge variant (`badgeVariants.variant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BadgeVariant {
    #[default]
    Default,
    Destructive,
    Error,
    Info,
    Outline,
    Secondary,
    Success,
    Warning,
}

/// Badge size: `Sm` 16px / 10px text, `Default` 18px / 12px, `Lg` 22px / 14px.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BadgeSize {
    Sm,
    #[default]
    Default,
    Lg,
}

/// A small status label.
///
/// ```ignore
/// Badge::new("Beta").variant(BadgeVariant::Info)
/// ```
#[derive(IntoElement)]
pub struct Badge {
    label: Option<SharedString>,
    icon: Option<IconName>,
    variant: BadgeVariant,
    size: BadgeSize,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: Some(label.into()),
            ..Self::empty()
        }
    }

    /// A badge with only children (e.g. a count with an icon).
    pub fn empty() -> Self {
        Self {
            label: None,
            icon: None,
            variant: BadgeVariant::Default,
            size: BadgeSize::Default,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: BadgeSize) -> Self {
        self.size = size;
        self
    }

    /// Leading 12px icon at 80% opacity.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

impl ParentElement for Badge {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Badge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = cx.colors();
        // Status badges: `x/8` in light, `x/16` in dark.
        let tint = |light: Hsla, dark: Hsla| if c.is_dark { dark } else { light };
        let none = transparent_black();
        // (background, text, border)
        let (bg, fg, border) = match self.variant {
            BadgeVariant::Default => (c.primary, c.primary_foreground, none),
            BadgeVariant::Destructive => (c.destructive, gpui_kit::white(), none),
            BadgeVariant::Error => (
                tint(c.destructive_8, c.destructive_16),
                c.destructive_foreground,
                none,
            ),
            BadgeVariant::Info => (tint(c.info_8, c.info_16), c.info_foreground, none),
            BadgeVariant::Success => (tint(c.success_8, c.success_16), c.success_foreground, none),
            BadgeVariant::Warning => (tint(c.warning_8, c.warning_16), c.warning_foreground, none),
            BadgeVariant::Outline => (
                if c.is_dark { c.input_32 } else { c.background },
                c.foreground,
                c.input,
            ),
            BadgeVariant::Secondary => (c.secondary, c.secondary_foreground, none),
        };
        let (height, padding, text, line, radius): (Pixels, Pixels, Pixels, Pixels, Pixels) =
            match self.size {
                BadgeSize::Sm => (px(16.), px(3.), px(10.), px(14.), px(4.)),
                BadgeSize::Default => (px(18.), px(3.), px(12.), px(16.), px(6.)),
                BadgeSize::Lg => (px(22.), px(5.), px(14.), px(20.), px(6.)),
            };
        let mut badge = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .h(height)
            .min_w(height)
            .px(padding)
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_color(fg)
            .text_size(text)
            .line_height(line)
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .when_some(self.icon, |this, icon| {
                this.child(Icon::new(icon).size(px(12.)).opacity(ICON_OPACITY))
            })
            .when_some(self.label, |this, label| this.child(label))
            .children(self.children);
        gpui_kit::Refineable::refine(badge.style(), &self.style);
        badge
    }
}

/// A keyboard key cap: 20px tall, `muted` fill, 12px medium DM Sans in `muted-foreground`.
///
/// ```ignore
/// h_flex().gap_1().child(Kbd::new("⌘")).child(Kbd::new("K"))
/// ```
#[derive(IntoElement)]
pub struct Kbd {
    label: SharedString,
}

impl Kbd {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
        }
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = cx.colors();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .h(px(20.))
            .min_w(px(20.))
            .px(px(4.))
            .rounded(px(4.))
            .bg(c.muted)
            .text_color(c.muted_foreground)
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .child(self.label)
    }
}
