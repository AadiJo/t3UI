//! coss `Select` (`ui/select.tsx`): the trigger (default and ghost variants, four sizes), the
//! popup list and its items, and a controlled [`Select`] that wires them to gpui-base's
//! popover. The popup opens below the trigger (offset 4, start-aligned, at least the
//! trigger's width) with no animation.
//!
//! Not reproduced: Base UI's `alignItemWithTrigger` (macOS-style placement with the selected
//! item over the trigger); the popup always uses the dropdown fallback placement.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    AnyElement, App, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, base, base::ElementExt as _, base::actions::Cancel, div,
    prelude::FluentBuilder as _, px, transparent_black,
};

use super::{
    Callback, ChangeHandler, GROUP, Interaction, bevel, focus_ring, overhang, popup_surface,
    ring_band,
};
use crate::{
    ActiveColors as _, Icon, IconName,
    tokens::{DISABLED_OPACITY, ICON_OPACITY, shadow},
};

/// Trigger variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SelectVariant {
    /// Bordered field look.
    #[default]
    Default,
    /// Borderless `muted-foreground/70` text that fills with `accent` on hover.
    Ghost,
}

/// Trigger size: `Xs` 24, `Sm` 28, `Default` 32, `Lg` 36 (min heights).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SelectSize {
    Xs,
    Sm,
    #[default]
    Default,
    Lg,
}

/// The select trigger: value (or placeholder) and a 12px chevron at 50%.
///
/// Usually built by [`Select`]; usable alone as a styled button that opens something else.
#[derive(IntoElement)]
pub struct SelectTrigger {
    id: ElementId,
    variant: SelectVariant,
    size: SelectSize,
    value: Option<SharedString>,
    placeholder: SharedString,
    icon: Option<IconName>,
    disabled: bool,
    invalid: bool,
    pressed: bool,
    preview: Interaction,
    style: StyleRefinement,
}

impl SelectTrigger {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: SelectVariant::Default,
            size: SelectSize::Default,
            value: None,
            placeholder: SharedString::default(),
            icon: None,
            disabled: false,
            invalid: false,
            pressed: false,
            preview: Interaction::Rest,
            style: StyleRefinement::default(),
        }
    }

    pub fn variant(mut self, variant: SelectVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: SelectSize) -> Self {
        self.size = size;
        self
    }

    /// The selected item's label; `None` shows the placeholder.
    pub fn value(mut self, value: Option<SharedString>) -> Self {
        self.value = value;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Leading icon (80%, `muted-foreground`).
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// Popup open (`data-pressed`): drops the shadow and bevel.
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl Styled for SelectTrigger {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for SelectTrigger {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let (height, padding, gap, text, line, radius, icon_size) = match self.size {
            SelectSize::Xs => (px(24.), px(7.), px(4.), px(12.), px(16.), px(8.), px(14.)),
            SelectSize::Sm => (px(28.), px(9.), px(6.), px(14.), px(20.), px(10.), px(16.)),
            SelectSize::Default => (px(32.), px(11.), px(8.), px(14.), px(20.), px(10.), px(16.)),
            SelectSize::Lg => (px(36.), px(11.), px(8.), px(14.), px(20.), px(10.), px(16.)),
        };
        let focus = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let disabled = self.disabled;
        let focus_visible = self.preview == Interaction::FocusVisible
            || (!disabled && focus.is_focused(window) && window.last_input_was_keyboard());
        let pressed = self.pressed || self.preview == Interaction::Pressed;
        let hovered = self.preview == Interaction::Hover;
        let ghost = self.variant == SelectVariant::Ghost;
        let placeholder = self.value.is_none();

        let fg = if ghost {
            if pressed || hovered {
                colors.foreground.opacity(0.8)
            } else {
                colors.muted_foreground_70
            }
        } else {
            colors.foreground
        };
        let (bg, border): (Hsla, Hsla) = if ghost {
            (
                if pressed || hovered {
                    colors.accent
                } else {
                    transparent_black()
                },
                transparent_black(),
            )
        } else {
            (
                if colors.is_dark {
                    colors.input_32
                } else {
                    colors.background
                },
                if focus_visible {
                    if self.invalid {
                        colors.destructive_64
                    } else {
                        colors.ring
                    }
                } else if self.invalid {
                    colors.destructive_36
                } else {
                    colors.input
                },
            )
        };
        let quiet = disabled || focus_visible || self.invalid || pressed;

        let mut trigger = base::Button::new(self.id)
            .track_focus(&focus)
            .disabled(disabled)
            .group(GROUP)
            .relative()
            .justify_between()
            .gap(gap)
            .h(height)
            .px(padding)
            .rounded(radius)
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(text)
            .line_height(line)
            .text_color(fg)
            .cursor_pointer()
            .when(!ghost, |this| this.w_full().min_w(px(144.)))
            .when(!ghost && !quiet, |this| {
                // Dark fills are `input/32`, too transparent for a drop shadow.
                this.when(!colors.is_dark, |this| this.shadow(shadow::XS_5.to_vec()))
                    .child(bevel(radius, colors.bevel, colors))
            })
            .when(ghost && !disabled && !pressed, |this| {
                this.hover(|style| {
                    style
                        .bg(colors.accent)
                        .text_color(colors.foreground.opacity(0.8))
                })
            })
            .when(disabled, |this| this.opacity(DISABLED_OPACITY))
            .when(focus_visible, |this| {
                if ghost {
                    this.children(focus_ring(radius, px(1.), colors))
                } else {
                    let ring = if self.invalid {
                        if colors.is_dark {
                            colors.destructive_24
                        } else {
                            colors.destructive_16
                        }
                    } else {
                        colors.ring_24
                    };
                    this.child(ring_band(radius, px(1.), px(0.), px(3.), ring))
                }
            })
            .when_some(self.icon, |this, icon| {
                this.child(
                    Icon::new(icon)
                        .size(icon_size)
                        .opacity(ICON_OPACITY)
                        .color(colors.muted_foreground),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(placeholder, |this| this.text_color(colors.muted_foreground))
                    .child(self.value.unwrap_or(self.placeholder)),
            )
            .child(overhang(
                Icon::new(IconName::ChevronDown)
                    .opacity(0.5)
                    .when(!ghost, |this| this.color(colors.muted_foreground)),
                px(12.),
                px(0.),
                px(4.),
            ));
        gpui_kit::Refineable::refine(trigger.style(), &self.style);
        trigger
    }
}

/// The select popup surface: radius 10, `shadow-lg/5`, a 4px-padded list.
#[derive(IntoElement, Default)]
pub struct SelectPopup {
    min_width: Option<Pixels>,
    children: Vec<AnyElement>,
}

impl SelectPopup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Minimum width (the trigger's, `min-w-(--anchor-width)`).
    pub fn min_width(mut self, width: Pixels) -> Self {
        self.min_width = Some(width);
        self
    }
}

impl ParentElement for SelectPopup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SelectPopup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        popup_surface(px(10.), cx.colors())
            .when_some(self.min_width, |this, width| this.min_w(width))
            .flex()
            .flex_col()
            .p(px(4.))
            .children(self.children)
    }
}

/// A select option: 16px check column, min height 28, radius 6, `accent` when highlighted.
#[derive(IntoElement)]
pub struct SelectItem {
    id: ElementId,
    label: SharedString,
    selected: bool,
    disabled: bool,
    on_click: Option<Callback>,
    preview: Interaction,
}

impl SelectItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected: false,
            disabled: false,
            on_click: None,
            preview: Interaction::Rest,
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl RenderOnce for SelectItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let highlighted = self.preview != Interaction::Rest;
        div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(8.))
            .min_h(px(28.))
            .py(px(4.))
            .pl(px(8.))
            .pr(px(16.))
            .rounded(px(6.))
            .text_size(px(14.))
            .line_height(px(20.))
            .font_weight(FontWeight::NORMAL)
            .text_color(colors.foreground)
            .cursor_default()
            .when(highlighted, |this| this.bg(colors.accent))
            .when(!self.disabled && !highlighted, |this| {
                this.hover(|style| style.bg(colors.accent).text_color(colors.accent_foreground))
            })
            .when(self.disabled, |this| this.opacity(DISABLED_OPACITY))
            .child(div().size(px(16.)).flex_none().when(self.selected, |this| {
                this.child(Icon::new(IconName::CheckIndicator))
            }))
            .child(div().flex_1().min_w_0().child(self.label))
            .when_some(
                self.on_click.filter(|_| !self.disabled),
                |this, on_click| {
                    this.on_click(move |_, window, cx| {
                        on_click(window, cx);
                        window.dispatch_action(Box::new(Cancel), cx);
                    })
                },
            )
    }
}

/// A controlled select.
///
/// ```ignore
/// Select::new("mode")
///     .items([("light", "Light"), ("dark", "Dark"), ("system", "System")])
///     .value(Some("system".into()))
///     .on_change(cx.listener(|this, value: &SharedString, _, cx| { .. }))
/// ```
#[derive(IntoElement)]
pub struct Select {
    id: ElementId,
    items: Vec<(SharedString, SharedString)>,
    value: Option<SharedString>,
    placeholder: SharedString,
    variant: SelectVariant,
    size: SelectSize,
    disabled: bool,
    on_change: Option<ChangeHandler<SharedString>>,
}

impl Select {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            value: None,
            placeholder: SharedString::default(),
            variant: SelectVariant::Default,
            size: SelectSize::Default,
            disabled: false,
            on_change: None,
        }
    }

    /// `(value, label)` pairs.
    pub fn items<V: Into<SharedString>, L: Into<SharedString>>(
        mut self,
        items: impl IntoIterator<Item = (V, L)>,
    ) -> Self {
        self.items = items
            .into_iter()
            .map(|(v, l)| (v.into(), l.into()))
            .collect();
        self
    }

    pub fn value(mut self, value: Option<SharedString>) -> Self {
        self.value = value;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn variant(mut self, variant: SelectVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: SelectSize) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(
        mut self,
        handler: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Trigger width from the previous frame, for the popup's minimum width.
        let trigger_width = window
            .use_keyed_state(ElementId::from((self.id.clone(), "width")), cx, |_, _| {
                Rc::new(Cell::new(px(0.)))
            })
            .read(cx)
            .clone();
        let label = self
            .value
            .as_ref()
            .and_then(|value| self.items.iter().find(|(v, _)| v == value))
            .map(|(_, label)| label.clone());
        let trigger_id = ElementId::from((self.id.clone(), "trigger"));
        let (variant, size, disabled, placeholder) =
            (self.variant, self.size, self.disabled, self.placeholder);
        let width_writer = trigger_width.clone();
        let items = self.items;
        let value = self.value;
        let on_change = self.on_change;
        base::Popover::new(self.id)
            .anchor(super::popover::Align::Start.below())
            .offset(px(4.))
            .trigger_with(move |open, _, _| {
                div()
                    .child(
                        SelectTrigger::new(trigger_id)
                            .variant(variant)
                            .size(size)
                            .value(label)
                            .placeholder(placeholder)
                            .disabled(disabled)
                            .pressed(open),
                    )
                    .on_prepaint(move |bounds, _, _| width_writer.set(bounds.size.width))
                    .into_any_element()
            })
            .content(move |_, _, _| {
                SelectPopup::new()
                    .min_width(trigger_width.get())
                    .children(items.into_iter().map(|(item_value, item_label)| {
                        let selected = value.as_ref() == Some(&item_value);
                        let on_change = on_change.clone();
                        SelectItem::new(ElementId::Name(item_value.clone()), item_label)
                            .selected(selected)
                            .on_click(move |window, cx| {
                                if let Some(on_change) = on_change.as_ref() {
                                    on_change(&item_value, window, cx);
                                }
                            })
                    }))
            })
    }
}
