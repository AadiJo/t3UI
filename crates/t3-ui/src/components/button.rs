//! coss `Button` (`ui/button.tsx`): every variant and size, with the solid-button inset
//! highlight, the outline bevel, and the keyboard focus ring. Behavior (pointer, Enter/Space,
//! focus, accessibility) is gpui-base's `Button`.

use gpui_kit::{
    AnyElement, App, BoxShadow, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, Pixels, Refineable as _, RenderOnce, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, base, div,
    prelude::FluentBuilder as _, px, transparent_black,
};

use std::rc::Rc;

use super::{
    ClickHandler, GROUP, Interaction, TooltipExt as _, bevel, focus_ring, inset_line, outer_shadow,
    overhang,
};
use crate::{
    ActiveColors as _, Colors, Icon, IconName,
    tokens::{DISABLED_OPACITY, ICON_OPACITY, shadow},
};

/// Visual variant (`buttonVariants.variant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonVariant {
    /// Solid `primary`.
    #[default]
    Default,
    /// Solid `destructive` with white text.
    Destructive,
    /// Outline with `destructive-foreground` text.
    DestructiveOutline,
    /// Bordered `popover` (light) / `input/32` (dark) surface.
    Outline,
    /// Borderless `secondary` fill.
    Secondary,
    /// Transparent until hovered.
    Ghost,
    /// Text that underlines on hover.
    Link,
}

/// Size (`buttonVariants.size`, desktop values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonSize {
    /// 24px, 12px text.
    Xs,
    /// 28px.
    Sm,
    /// 32px.
    #[default]
    Default,
    /// 36px.
    Lg,
    /// 40px, 16px text.
    Xl,
    /// 24px square.
    IconXs,
    /// 28px square.
    IconSm,
    /// 32px square.
    Icon,
    /// 36px square.
    IconLg,
    /// 40px square.
    IconXl,
}

struct Metrics {
    height: Pixels,
    /// `None` for square icon buttons.
    padding_x: Option<Pixels>,
    gap: Pixels,
    text: (Pixels, Pixels),
    radius: Pixels,
    icon: Pixels,
}

impl ButtonSize {
    fn metrics(self) -> Metrics {
        let base = |height: f32, padding: Option<f32>| Metrics {
            height: px(height),
            padding_x: padding.map(px),
            gap: px(8.),
            text: (px(14.), px(20.)),
            radius: px(10.),
            icon: px(16.),
        };
        match self {
            Self::Xs => Metrics {
                gap: px(4.),
                text: (px(12.), px(16.)),
                radius: px(8.),
                icon: px(14.),
                ..base(24., Some(7.))
            },
            Self::Sm => Metrics {
                gap: px(6.),
                ..base(28., Some(9.))
            },
            Self::Default => base(32., Some(11.)),
            Self::Lg => base(36., Some(13.)),
            Self::Xl => Metrics {
                text: (px(16.), px(24.)),
                icon: px(18.),
                ..base(40., Some(15.))
            },
            Self::IconXs => Metrics {
                radius: px(8.),
                icon: px(14.),
                ..base(24., None)
            },
            Self::IconSm => base(28., None),
            Self::Icon => base(32., None),
            Self::IconLg => base(36., None),
            Self::IconXl => Metrics {
                icon: px(18.),
                ..base(40., None)
            },
        }
    }
}

/// Paint for one interaction state.
#[derive(Clone)]
struct Look {
    bg: Hsla,
    border: Hsla,
    shadow: Vec<BoxShadow>,
    /// Solid buttons' 1px inner top line.
    inset: Option<Hsla>,
    /// Outline buttons' edge bevel color.
    bevel: Option<Hsla>,
    underline: bool,
}

struct Looks {
    fg: Hsla,
    icon: Option<Hsla>,
    rest: Look,
    hover: Look,
    pressed: Look,
}

impl ButtonVariant {
    fn looks(self, c: &Colors) -> Looks {
        let none = transparent_black();
        let plain = |bg: Hsla, border: Hsla| Look {
            bg,
            border,
            shadow: Vec::new(),
            inset: None,
            bevel: None,
            underline: false,
        };
        let solid = |fill: Hsla, fill_90: Hsla, fg: Hsla, shadow_color: Hsla| {
            let mut outer = shadow::XS_5.to_vec();
            outer[0].color = shadow_color;
            Looks {
                fg,
                icon: None,
                rest: Look {
                    shadow: outer,
                    inset: Some(c.inset_highlight),
                    ..plain(fill, fill)
                },
                hover: Look {
                    shadow: {
                        let mut outer = shadow::XS_5.to_vec();
                        outer[0].color = shadow_color;
                        outer
                    },
                    inset: Some(c.inset_highlight),
                    ..plain(fill_90, fill)
                },
                pressed: Look {
                    inset: Some(c.inset_press),
                    ..plain(fill_90, fill)
                },
            }
        };
        let outline = |fg: Hsla, hover_bg: Hsla, hover_border: Hsla| {
            let rest_bg = if c.is_dark { c.input_32 } else { c.popover };
            Looks {
                fg,
                icon: Some(c.muted_foreground),
                rest: Look {
                    shadow: shadow::XS_5.to_vec(),
                    bevel: Some(c.bevel),
                    ..plain(rest_bg, c.input)
                },
                hover: Look {
                    shadow: shadow::XS_5.to_vec(),
                    bevel: Some(c.bevel),
                    ..plain(hover_bg, hover_border)
                },
                pressed: Look {
                    bevel: Some(c.bevel_pressed),
                    ..plain(hover_bg, hover_border)
                },
            }
        };
        let looks = match self {
            Self::Default => solid(c.primary, c.primary_90, c.primary_foreground, c.primary_24),
            Self::Destructive => solid(
                c.destructive,
                c.destructive_90,
                gpui_kit::white(),
                c.destructive_24,
            ),
            Self::Outline => outline(
                c.foreground,
                if c.is_dark { c.input_64 } else { c.accent_50 },
                c.input,
            ),
            Self::DestructiveOutline => {
                outline(c.destructive_foreground, c.destructive_4, c.destructive_32)
            }
            Self::Secondary => Looks {
                fg: c.secondary_foreground,
                icon: None,
                rest: plain(c.secondary, none),
                hover: plain(c.secondary_90, none),
                pressed: plain(c.secondary_80, none),
            },
            Self::Ghost => Looks {
                fg: c.foreground,
                icon: Some(c.muted_foreground),
                rest: plain(none, none),
                hover: plain(c.accent, none),
                pressed: plain(c.accent, none),
            },
            Self::Link => Looks {
                fg: c.foreground,
                icon: None,
                rest: plain(none, none),
                hover: Look {
                    underline: true,
                    ..plain(none, none)
                },
                pressed: Look {
                    underline: true,
                    ..plain(none, none)
                },
            },
        };
        let settle = |mut look: Look| {
            look.shadow = outer_shadow(look.bg, &look.shadow);
            look
        };
        Looks {
            rest: settle(looks.rest),
            hover: settle(looks.hover),
            pressed: settle(looks.pressed),
            ..looks
        }
    }
}

/// A coss button.
///
/// ```ignore
/// Button::new("save").label("Save").on_click(|_, window, cx| { .. })
/// Button::new("more").variant(ButtonVariant::Ghost).size(ButtonSize::IconSm).icon(IconName::Ellipsis)
/// ```
///
/// `pressed(true)` holds the pressed look (Base UI's `data-pressed`, e.g. while the button's
/// menu is open). Disabled buttons drop to 64% opacity and ignore input. Focus shows a ring
/// only after keyboard input, like `:focus-visible`.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    variant: ButtonVariant,
    size: ButtonSize,
    label: Option<SharedString>,
    icon: Option<IconName>,
    icon_end: Option<IconName>,
    children: Vec<AnyElement>,
    disabled: bool,
    pressed: bool,
    tooltip: Option<SharedString>,
    on_click: Option<ClickHandler>,
    preview: Interaction,
    style: StyleRefinement,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: ButtonVariant::Default,
            size: ButtonSize::Default,
            label: None,
            icon: None,
            icon_end: None,
            children: Vec::new(),
            disabled: false,
            pressed: false,
            tooltip: None,
            on_click: None,
            preview: Interaction::Rest,
            style: StyleRefinement::default(),
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Leading icon (16px, 14px for `Xs`, 18px for `Xl`; 80% opacity).
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Trailing icon.
    pub fn icon_end(mut self, icon: IconName) -> Self {
        self.icon_end = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Holds the pressed look (`data-pressed`).
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    /// Tooltip shown after the 600ms delay.
    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(text.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// Forces an interaction look for previews.
    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let metrics = self.size.metrics();
        let looks = self.variant.looks(colors);
        let disabled = self.disabled;
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let focus_visible = self.preview == Interaction::FocusVisible
            || (!disabled && focus.is_focused(window) && window.last_input_was_keyboard());

        // The resting look, or the forced/held one. Live hover and press refine over it.
        let held = match self.preview {
            Interaction::Pressed => Some(&looks.pressed),
            Interaction::Hover => Some(&looks.hover),
            _ if self.pressed => Some(&looks.pressed),
            _ => None,
        };
        let base_look = held.unwrap_or(&looks.rest).clone();
        let live = held.is_none() && !disabled;
        let radius = metrics.radius;
        let icon_color = looks.icon;
        let icon = |name: IconName| {
            overhang(
                Icon::new(name)
                    .opacity(ICON_OPACITY)
                    .when_some(icon_color, |this, color| this.color(color)),
                metrics.icon,
                px(2.),
                px(2.),
            )
        };

        let mut button = base::Button::new(self.id)
            .track_focus(&focus)
            .disabled(disabled)
            .group(GROUP)
            .relative()
            .flex_none()
            .gap(metrics.gap)
            .h(metrics.height)
            .map(|this| match metrics.padding_x {
                Some(padding) => this.px(padding),
                None => this.w(metrics.height),
            })
            .rounded(radius)
            .border_1()
            .whitespace_nowrap()
            .text_size(metrics.text.0)
            .line_height(metrics.text.1)
            .font_weight(FontWeight::MEDIUM)
            .text_color(looks.fg)
            .cursor_pointer()
            .bg(base_look.bg)
            .border_color(base_look.border)
            .shadow(if disabled {
                Vec::new()
            } else {
                base_look.shadow.clone()
            })
            .when(base_look.underline, |this| this.underline())
            .when(live, |this| {
                let hover = looks.hover.clone();
                let pressed = looks.pressed.clone();
                this.hover(move |style| {
                    let style = style
                        .bg(hover.bg)
                        .border_color(hover.border)
                        .shadow(hover.shadow);
                    if hover.underline {
                        style.underline()
                    } else {
                        style
                    }
                })
                .active(move |style| {
                    style
                        .bg(pressed.bg)
                        .border_color(pressed.border)
                        .shadow(pressed.shadow)
                })
            })
            .when(disabled, |this| this.opacity(DISABLED_OPACITY))
            // Solid buttons' inner highlight (`inset-shadow-[0_1px_white/16]`).
            .when_some(base_look.inset.filter(|_| !disabled), |this, color| {
                let hover = looks.hover.inset;
                let pressed = looks.pressed.inset;
                this.child(
                    div()
                        .id("inset")
                        .absolute()
                        .inset_0()
                        .rounded(radius - px(1.))
                        .shadow(vec![inset_line(px(1.), color)])
                        .when(live, |this| {
                            this.group_hover(GROUP, move |style| {
                                style.shadow(
                                    hover
                                        .map(|c| vec![inset_line(px(1.), c)])
                                        .unwrap_or_default(),
                                )
                            })
                            .group_active(GROUP, move |style| {
                                style.shadow(
                                    pressed
                                        .map(|c| vec![inset_line(px(1.), c)])
                                        .unwrap_or_default(),
                                )
                            })
                        }),
                )
            })
            // Outline buttons' edge bevel, hidden or dimmed while pressed.
            .when_some(base_look.bevel.filter(|_| !disabled), |this, color| {
                let pressed = looks.pressed.bevel;
                let mut line = bevel(radius, color, colors).id("bevel");
                if live {
                    line = line.group_active(GROUP, move |style| {
                        let mut style = style;
                        let shadow = pressed.map(|c| {
                            let dy = if colors.is_dark { px(1.) } else { px(-1.) };
                            vec![inset_line(dy, c)]
                        });
                        style = style.shadow(shadow.unwrap_or_default());
                        style
                    });
                }
                this.child(line)
            })
            .when_some(self.icon, |this, name| this.child(icon(name)))
            .when_some(self.label, |this, label| this.child(label))
            .children(self.children)
            .when_some(self.icon_end, |this, name| this.child(icon(name)))
            .when(focus_visible, |this| {
                this.children(focus_ring(radius, px(1.), colors))
            })
            .when_some(self.tooltip, |this, text| this.tooltip_text(text));

        if let Some(on_click) = self.on_click {
            button = button.on_click(move |event, window, cx| on_click(event, window, cx));
        }
        button.style().refine(&self.style);
        button
    }
}
