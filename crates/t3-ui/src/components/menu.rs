//! coss `Menu` (`ui/menu.tsx`): the menu surface, items, checkbox items, labels, separators
//! and shortcuts, plus [`DropdownMenu`] (click a trigger) and [`ContextMenu`] (right-click,
//! opens at the cursor). Menus open and close instantly, like the reference.
//!
//! Items highlight on hover (`data-highlighted`: `accent` fill). Activating an item runs its
//! handler and closes the menu by dispatching `gpui_kit::base::actions::Cancel`.

use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement, Pixels, Point, RenderOnce, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, anchored, base,
    base::actions::Cancel, deferred, div, prelude::FluentBuilder as _, px,
};

use super::{
    ChangeHandler, ClickHandler, Interaction, overhang, popover::Align, popup_surface,
    switch::Switch,
};
use crate::{ActiveColors as _, Icon, IconName, tokens::DISABLED_OPACITY};

/// Builds a context menu's rows each time it renders.
type ItemsFn = Rc<dyn Fn(&mut Window, &mut App) -> Vec<AnyElement>>;

/// The menu surface: min width 128, radius 10, `shadow-lg/5`, 4px inner padding.
#[derive(IntoElement, Default)]
pub struct MenuPopup {
    compact: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl MenuPopup {
    pub fn new() -> Self {
        Self::default()
    }

    /// `rounded-md` (8px) surface for compact menus opened from `xs` triggers.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
}

impl ParentElement for MenuPopup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for MenuPopup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MenuPopup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let radius = if self.compact { px(8.) } else { px(10.) };
        let mut surface = popup_surface(radius, cx.colors())
            .min_w(px(128.))
            .flex()
            .flex_col()
            .p(px(4.))
            .text_size(px(14.))
            .line_height(px(20.))
            .children(self.children);
        gpui_kit::Refineable::refine(surface.style(), &self.style);
        surface
    }
}

/// Shared item row: min height 28, radius 6, `accent` while highlighted.
fn item_row(
    id: ElementId,
    disabled: bool,
    highlighted: bool,
    compact: bool,
    cx: &App,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let colors = cx.colors();
    let (min_height, padding_y, text, line) = if compact {
        (px(24.), px(2.), px(12.), px(16.))
    } else {
        (px(28.), px(4.), px(14.), px(20.))
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .min_h(min_height)
        .py(padding_y)
        .rounded(px(6.))
        .text_size(text)
        .line_height(line)
        .text_color(colors.foreground)
        .cursor_default()
        .when(highlighted, |this| {
            this.bg(colors.accent).text_color(colors.accent_foreground)
        })
        .when(!disabled && !highlighted, |this| {
            this.hover(|style| style.bg(colors.accent).text_color(colors.accent_foreground))
        })
        .when(disabled, |this| this.opacity(DISABLED_OPACITY))
}

fn activate(handler: Option<ClickHandler>) -> impl Fn(&ClickEvent, &mut Window, &mut App) {
    move |event, window, cx| {
        if let Some(handler) = handler.as_ref() {
            handler(event, window, cx);
        }
        window.dispatch_action(Box::new(Cancel), cx);
    }
}

/// A menu command.
///
/// ```ignore
/// MenuItem::new("rename", "Rename").icon(IconName::SquarePen).shortcut("⌘R").on_click(..)
/// ```
#[derive(IntoElement)]
pub struct MenuItem {
    id: ElementId,
    label: SharedString,
    icon: Option<IconName>,
    shortcut: Option<SharedString>,
    destructive: bool,
    inset: bool,
    compact: bool,
    disabled: bool,
    on_click: Option<ClickHandler>,
    preview: Interaction,
}

impl MenuItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            shortcut: None,
            destructive: false,
            inset: false,
            compact: false,
            disabled: false,
            on_click: None,
            preview: Interaction::Rest,
        }
    }

    /// Leading 16px icon (80%, `muted-foreground`).
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Trailing [`MenuShortcut`] text, e.g. `⌘K`.
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// `variant="destructive"`: text and icon in `destructive-foreground`.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// `inset`: 32px start padding to line up with icon items.
    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }

    /// The composer's compact row (`min-h-6 text-xs`, 12px icons) for `xs`-triggered menus.
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }

    /// `Hover` shows the highlighted look.
    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl RenderOnce for MenuItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let highlighted = self.preview != Interaction::Rest;
        let fg = self.destructive.then_some(colors.destructive_foreground);
        let icon_color = fg.unwrap_or(colors.muted_foreground);
        let compact = self.compact;
        let (gap, start, end, icon_size) = if compact {
            (px(6.), px(6.), px(8.), px(12.))
        } else {
            (px(8.), px(8.), px(8.), px(16.))
        };
        item_row(self.id, self.disabled, highlighted, compact, cx)
            .gap(gap)
            .pl(if self.inset { px(32.) } else { start })
            .pr(end)
            .when_some(fg, |this, fg| this.text_color(fg))
            .when_some(self.icon, |this, icon| {
                this.child(overhang(
                    Icon::new(icon)
                        .color(icon_color)
                        .opacity(crate::tokens::ICON_OPACITY),
                    icon_size,
                    px(2.),
                    px(2.),
                ))
            })
            .child(div().flex_1().min_w_0().truncate().child(self.label))
            .when_some(self.shortcut, |this, shortcut| {
                this.child(MenuShortcut::new(shortcut))
            })
            .when(!self.disabled, |this| {
                this.on_click(activate(self.on_click))
            })
    }
}

/// A checkable menu item (`grid-cols-[1rem_1fr]`, check indicator in the first column), or
/// with `switch()` a label with a trailing small [`Switch`]. Radio items look the same: use
/// this with `checked` set on the selected option.
#[derive(IntoElement)]
pub struct MenuCheckboxItem {
    id: ElementId,
    label: SharedString,
    checked: bool,
    switch: bool,
    compact: bool,
    disabled: bool,
    on_change: Option<ChangeHandler<bool>>,
    preview: Interaction,
}

impl MenuCheckboxItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked: false,
            switch: false,
            compact: false,
            disabled: false,
            on_change: None,
            preview: Interaction::Rest,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// `variant="switch"`.
    pub fn switch(mut self) -> Self {
        self.switch = true;
        self
    }

    /// The composer's compact row (`min-h-6 text-xs`, 12px check column).
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Receives the requested value. Checkbox items keep the menu open.
    pub fn on_change(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    #[doc(hidden)]
    pub fn preview(mut self, interaction: Interaction) -> Self {
        self.preview = interaction;
        self
    }
}

impl RenderOnce for MenuCheckboxItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let highlighted = self.preview != Interaction::Rest;
        let checked = self.checked;
        let compact = self.compact;
        let row = item_row(self.id.clone(), self.disabled, highlighted, compact, cx);
        let row = if self.switch {
            row.gap(px(16.))
                .pl(px(8.))
                .pr(px(6.))
                .child(div().flex_1().min_w_0().child(self.label))
                .child(
                    Switch::new(ElementId::from((self.id, "switch")))
                        .small()
                        .checked(checked),
                )
        } else {
            let (gap, start, end, check) = if compact {
                (px(6.), px(6.), px(8.), px(12.))
            } else {
                (px(8.), px(8.), px(16.), px(16.))
            };
            row.gap(gap)
                .pl(start)
                .pr(end)
                .child(div().size(check).flex_none().when(checked, |this| {
                    this.child(Icon::new(IconName::CheckIndicator).size(check))
                }))
                .child(div().flex_1().min_w_0().child(self.label))
        };
        row.when_some(
            self.on_change.filter(|_| !self.disabled),
            |this, on_change| this.on_click(move |_, window, cx| on_change(&!checked, window, cx)),
        )
    }
}

/// Group label: 12px medium `muted-foreground`, padding 8x6 (32px start when inset).
#[derive(IntoElement)]
pub struct MenuGroupLabel {
    text: SharedString,
    inset: bool,
}

impl MenuGroupLabel {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            inset: false,
        }
    }

    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }
}

impl RenderOnce for MenuGroupLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .pl(if self.inset { px(32.) } else { px(8.) })
            .pr(px(8.))
            .py(px(6.))
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.colors().muted_foreground)
            .child(self.text)
    }
}

/// 1px `border` rule with 8x4 margins.
#[derive(IntoElement, Default)]
pub struct MenuSeparator;

impl RenderOnce for MenuSeparator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div().mx(px(8.)).my(px(4.)).h(px(1.)).bg(cx.colors().border)
    }
}

/// Trailing shortcut text: 12px medium sans, `muted-foreground/72`. (The reference's
/// `tracking-widest` isn't reproduced: GPUI has no letter spacing.)
#[derive(IntoElement)]
pub struct MenuShortcut(SharedString);

impl MenuShortcut {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for MenuShortcut {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .ml_auto()
            .pl(px(8.))
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.colors().muted_foreground_72)
            .child(self.0)
    }
}

type ItemsBuilder = Box<dyn FnOnce(&mut Window, &mut App) -> Vec<AnyElement>>;

/// A menu opened by clicking its trigger (side bottom, offset 4, centered by default).
///
/// ```ignore
/// DropdownMenu::new("thread-actions")
///     .trigger(|open| Button::new("more").variant(ButtonVariant::Ghost).size(ButtonSize::IconSm)
///         .icon(IconName::Ellipsis).pressed(open).into_any_element())
///     .items(|_, _| vec![MenuItem::new("rename", "Rename").into_any_element()])
/// ```
#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    align: Align,
    trigger: Option<Box<dyn FnOnce(bool) -> AnyElement>>,
    items: Option<ItemsBuilder>,
    min_width: Option<Pixels>,
}

impl DropdownMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            align: Align::Center,
            trigger: None,
            items: None,
            min_width: None,
        }
    }

    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Builds the trigger; receives whether the menu is open (render it `pressed`).
    pub fn trigger(mut self, trigger: impl FnOnce(bool) -> AnyElement + 'static) -> Self {
        self.trigger = Some(Box::new(trigger));
        self
    }

    /// Builds the menu rows when it opens.
    pub fn items(
        mut self,
        items: impl FnOnce(&mut Window, &mut App) -> Vec<AnyElement> + 'static,
    ) -> Self {
        self.items = Some(Box::new(items));
        self
    }

    pub fn min_width(mut self, width: Pixels) -> Self {
        self.min_width = Some(width);
        self
    }
}

impl RenderOnce for DropdownMenu {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let items = self.items;
        let min_width = self.min_width;
        base::Popover::new(self.id)
            .anchor(self.align.below())
            .offset(px(4.))
            .when_some(self.trigger, |this, trigger| {
                this.trigger_with(move |open, _, _| trigger(open))
            })
            .when_some(items, |this, items| {
                this.content(move |_, window, cx| {
                    MenuPopup::new()
                        .when_some(min_width, |this, width| this.min_w(width))
                        .children(items(window, cx))
                })
            })
    }
}

/// Right-click menu for `child`, opening at the cursor (Base UI `ContextMenu`).
#[derive(IntoElement)]
pub struct ContextMenu {
    id: ElementId,
    child: AnyElement,
    items: Option<ItemsFn>,
}

impl ContextMenu {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self {
            id: id.into(),
            child: child.into_any_element(),
            items: None,
        }
    }

    pub fn items(
        mut self,
        items: impl Fn(&mut Window, &mut App) -> Vec<AnyElement> + 'static,
    ) -> Self {
        self.items = Some(Rc::new(items));
        self
    }
}

#[derive(Default)]
struct ContextMenuState {
    position: Option<Point<Pixels>>,
    previous_focus: Option<FocusHandle>,
}

impl RenderOnce for ContextMenu {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ContextMenuState::default());
        let focus = window
            .use_keyed_state(ElementId::from((self.id.clone(), "focus")), cx, |_, cx| {
                cx.focus_handle()
            })
            .read(cx)
            .clone();
        let position = state.read(cx).position;
        let close = {
            let state = state.clone();
            move |window: &mut Window, cx: &mut App| {
                let previous = state.update(cx, |state, cx| {
                    state.position = None;
                    cx.notify();
                    state.previous_focus.take()
                });
                if let Some(previous) = previous {
                    previous.focus(window, cx);
                }
            }
        };

        let open_state = state.clone();
        let open_focus = focus.clone();
        div()
            .id(self.id)
            .child(self.child)
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                let previous = window.focused(cx);
                open_state.update(cx, |state, cx| {
                    state.position = Some(event.position);
                    state.previous_focus = previous;
                    cx.notify();
                });
                open_focus.focus(window, cx);
                cx.stop_propagation();
            })
            .when_some(position.zip(self.items), |this, (position, items)| {
                let close_out = close.clone();
                this.child(
                    deferred(
                        anchored()
                            .position(position)
                            .snap_to_window_with_margin(px(8.))
                            .child(
                                div()
                                    .id("context-menu")
                                    .occlude()
                                    .track_focus(&focus)
                                    .key_context("Popover")
                                    .on_action(move |_: &Cancel, window, cx| close(window, cx))
                                    .on_mouse_down_out(move |_, window, cx| close_out(window, cx))
                                    .child(MenuPopup::new().children(items(window, cx))),
                            ),
                    )
                    .with_priority(base::POPUP_PRIORITY),
                )
            })
    }
}
