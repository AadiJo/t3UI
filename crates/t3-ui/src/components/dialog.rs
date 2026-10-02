//! coss `Dialog`, `AlertDialog` (`ui/dialog.tsx`, `ui/alert-dialog.tsx`) and `Sheet`
//! (`ui/sheet.tsx`). Modal behavior (focus trap, Escape, backdrop press, Enter to confirm)
//! is gpui-base's `Dialog`; these add the coss surfaces and a 200ms (dialog) / 180ms (sheet)
//! enter fade. Exit is instant: the host stops rendering the dialog when `open` is false.
//!
//! The backdrop is `background/60`; the reference also blurs 4px behind it, which GPUI
//! cannot do.

use std::rc::Rc;

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, Div, ElementId, FontWeight, IntoElement,
    ParentElement, RenderOnce, SharedString, StyleRefinement, Styled, Window, base,
    base::DialogChangeReason, div, prelude::FluentBuilder as _, px,
};

use super::{
    bevel,
    button::{Button, ButtonSize, ButtonVariant},
};
use crate::{
    ActiveColors as _, IconName,
    tokens::{motion, shadow},
};

type OpenChange = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The `background/60` modal backdrop, filling its parent.
pub fn dialog_backdrop(cx: &App) -> Div {
    div().absolute().inset_0().bg(cx.colors().background_60)
}

/// The dialog card: up to 512px wide, radius 18, `shadow-lg/5`, edge bevel.
#[derive(IntoElement, Default)]
pub struct DialogPopup {
    close_button: Option<OpenChange>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl DialogPopup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the top-right ghost `x` button (Dialog only; AlertDialog has none).
    pub fn close_button(
        mut self,
        on_close: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close_button = Some(Rc::new(on_close));
        self
    }
}

impl ParentElement for DialogPopup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for DialogPopup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DialogPopup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let radius = px(18.);
        let mut popup = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .max_w(px(512.))
            .rounded(radius)
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .shadow(shadow::LG_5.to_vec())
            .child(bevel(radius, colors.bevel, colors))
            .children(self.children)
            .when_some(self.close_button, |this, on_close| {
                this.child(
                    div().absolute().top(px(8.)).right(px(8.)).child(
                        Button::new("dialog-close")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Icon)
                            .icon(IconName::X)
                            .on_click(move |_, window, cx| on_close(false, window, cx)),
                    ),
                )
            });
        gpui_kit::Refineable::refine(popup.style(), &self.style);
        popup
    }
}

/// Header: padding 24 (bottom 12 when a panel follows), 8px gap.
#[derive(IntoElement, Default)]
pub struct DialogHeader {
    children: Vec<AnyElement>,
    before_panel: bool,
}

impl DialogHeader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tightens the bottom padding when a [`DialogPanel`] follows.
    pub fn before_panel(mut self) -> Self {
        self.before_panel = true;
        self
    }
}

impl ParentElement for DialogHeader {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for DialogHeader {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(24.))
            .when(self.before_panel, |this| this.pb(px(12.)))
            .children(self.children)
    }
}

/// Title: 20px semibold, `leading-none`.
#[derive(IntoElement)]
pub struct DialogTitle(SharedString);

impl DialogTitle {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for DialogTitle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .text_size(px(20.))
            .line_height(px(20.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(self.0)
    }
}

/// Description: 14px `muted-foreground`.
#[derive(IntoElement)]
pub struct DialogDescription(SharedString);

impl DialogDescription {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self(text.into())
    }
}

impl RenderOnce for DialogDescription {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .text_size(px(14.))
            .line_height(px(20.))
            .text_color(cx.colors().muted_foreground)
            .child(self.0)
    }
}

/// Body: padding 24 (top 4 after a header).
#[derive(IntoElement, Default)]
pub struct DialogPanel {
    children: Vec<AnyElement>,
    after_header: bool,
}

impl DialogPanel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn after_header(mut self) -> Self {
        self.after_header = true;
        self
    }
}

impl ParentElement for DialogPanel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for DialogPanel {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .p(px(24.))
            .when(self.after_header, |this| this.pt(px(4.)))
            .children(self.children)
    }
}

/// Footer variant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FooterVariant {
    /// Top border, `muted/72` fill, padding 24x16.
    #[default]
    Default,
    /// No fill: padding-top 16, padding-bottom 24.
    Bare,
}

/// Footer: right-aligned action row with an 8px gap.
#[derive(IntoElement, Default)]
pub struct DialogFooter {
    variant: FooterVariant,
    children: Vec<AnyElement>,
}

impl DialogFooter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn variant(mut self, variant: FooterVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl ParentElement for DialogFooter {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for DialogFooter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .px(px(24.))
            .map(|this| match self.variant {
                FooterVariant::Default => this
                    .py(px(16.))
                    .border_t_1()
                    .border_color(colors.border)
                    .bg(colors.muted_72)
                    .rounded_b(px(17.)),
                FooterVariant::Bare => this.pt(px(16.)).pb(px(24.)),
            })
            .children(self.children)
    }
}

/// A modal dialog, controlled by `open`.
///
/// ```ignore
/// Dialog::new("rename").open(self.renaming).on_open_change(cx.listener(|this, open, _, cx| {
///     this.renaming = *open; cx.notify();
/// })).child(DialogHeader::new().child(DialogTitle::new("Rename thread")))
/// ```
/// Escape, a backdrop press and the close button request `false`. Place it anywhere in the
/// tree: it renders into a deferred full-window layer.
#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    open: bool,
    alert: bool,
    on_open_change: Option<OpenChange>,
    children: Vec<AnyElement>,
}

/// An `AlertDialog`: a [`Dialog`] without the close button whose backdrop press does nothing.
pub struct AlertDialog;

impl AlertDialog {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(id: impl Into<ElementId>) -> Dialog {
        Dialog {
            alert: true,
            ..Dialog::new(id)
        }
    }
}

impl Dialog {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            alert: false,
            on_open_change: None,
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Dialog {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_change = self.on_open_change.clone();
        let mut popup = DialogPopup::new().children(self.children);
        if !self.alert
            && let Some(on_change) = self.on_open_change.clone()
        {
            popup = popup.close_button(move |open, window, cx| on_change(open, window, cx));
        }
        base::Dialog::new(cx)
            .open(self.open)
            .close_on_backdrop_press(!self.alert)
            .on_open_change(move |open, _: DialogChangeReason, window, cx| {
                if let Some(on_change) = on_change.as_ref() {
                    on_change(open, window, cx);
                }
            })
            .p(px(16.))
            .backdrop(dialog_backdrop(cx).with_animation(
                ElementId::from((self.id.clone(), "backdrop")),
                Animation::new(motion::DIALOG).with_easing(motion::ease_standard),
                |this, t| this.opacity(t),
            ))
            .popup(base::DialogPopup::new().w_full().max_w(px(512.)).child(
                div().w_full().child(popup).with_animation(
                    ElementId::from((self.id, "popup")),
                    Animation::new(motion::DIALOG).with_easing(motion::ease_standard),
                    |this, t| this.opacity(t),
                ),
            ))
    }
}

/// The right-side sheet surface: `min(100% - 48px, 448px)` wide, start border, `popover`
/// fill, `shadow-lg/5`.
#[derive(IntoElement, Default)]
pub struct SheetPopup {
    children: Vec<AnyElement>,
}

impl SheetPopup {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for SheetPopup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SheetPopup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let width = (window.viewport_size().width - px(48.)).min(px(448.));
        div()
            .relative()
            .flex()
            .flex_col()
            .h_full()
            .w(width)
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .shadow(shadow::LG_5.to_vec())
            .children(self.children)
    }
}

/// A modal sheet sliding in from the right (translate 32px + fade, 180ms). Controlled like
/// [`Dialog`].
#[derive(IntoElement)]
pub struct Sheet {
    id: ElementId,
    open: bool,
    on_open_change: Option<OpenChange>,
    children: Vec<AnyElement>,
}

impl Sheet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            on_open_change: None,
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn on_open_change(
        mut self,
        handler: impl Fn(bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_change = Some(Rc::new(handler));
        self
    }
}

impl ParentElement for Sheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Sheet {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let on_change = self.on_open_change;
        base::Dialog::new(cx)
            .open(self.open)
            .on_open_change(move |open, _: DialogChangeReason, window, cx| {
                if let Some(on_change) = on_change.as_ref() {
                    on_change(open, window, cx);
                }
            })
            .justify_end()
            .items_stretch()
            .backdrop(dialog_backdrop(cx).with_animation(
                ElementId::from((self.id.clone(), "backdrop")),
                Animation::new(motion::SHEET).with_easing(motion::ease_standard),
                |this, t| this.opacity(t),
            ))
            .popup(
                base::DialogPopup::new().h_full().child(
                    div()
                        .relative()
                        .h_full()
                        .child(SheetPopup::new().children(self.children))
                        .with_animation(
                            ElementId::from((self.id, "popup")),
                            Animation::new(motion::SHEET).with_easing(motion::ease_standard),
                            |this, t| this.opacity(t).left(px(32.) * (1. - t)),
                        ),
                ),
            )
    }
}
