//! coss `Toast` (`ui/toast.tsx`): the toast card with its status icon, title, description,
//! `xs` actions and the corner close orb, and [`Toaster`], the top-right stack (limit 3,
//! 5s timeout, collapsed with 12px peeks, expanded with 12px gaps on hover).
//!
//! Approximations: GPUI has no transforms, so collapsed toasts behind the front one are
//! drawn as narrower, shorter cards instead of scaled ones; enter/exit is a 500ms fade
//! with a horizontal slide on enter only; swipe-to-dismiss is not implemented.

use std::rc::Rc;

use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, ClickEvent, Context, ElementId, FontWeight,
    Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, Task, Window, deferred, div,
    prelude::FluentBuilder as _, px,
};

use super::{
    button::{Button, ButtonSize, ButtonVariant},
    feedback::Spinner,
    popup_surface,
};
use crate::{
    ActiveColors as _, Colors, Icon, IconName,
    tokens::{ICON_OPACITY, layout, motion, shadow},
};

/// Toast type: selects the leading icon and its color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToastKind {
    #[default]
    Default,
    Error,
    Info,
    Success,
    Warning,
    /// Spinning `loader-circle` at 80%; never times out.
    Loading,
}

use super::ClickHandler as Handler;

/// One toast. Show it with [`Toaster::push`], or render it directly for previews.
#[derive(Clone, IntoElement)]
pub struct Toast {
    id: ElementId,
    kind: ToastKind,
    title: SharedString,
    description: Option<SharedString>,
    action: Option<(SharedString, Handler)>,
    on_close: Option<Handler>,
}

impl Toast {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            id: ElementId::Name("toast".into()),
            kind: ToastKind::Default,
            title: title.into(),
            description: None,
            action: None,
            on_close: None,
        }
    }

    pub fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// A trailing primary `xs` button.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(handler)));
        self
    }

    /// Called by the close orb. [`Toaster`] sets this to dismiss the toast.
    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Rc::new(handler));
        self
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }

    fn icon(&self, colors: &Colors) -> Option<(IconName, Hsla)> {
        match self.kind {
            ToastKind::Default | ToastKind::Loading => None,
            ToastKind::Error => Some((IconName::CircleAlert, colors.destructive)),
            ToastKind::Info => Some((IconName::Info, colors.info)),
            ToastKind::Success => Some((IconName::CircleCheck, colors.success)),
            ToastKind::Warning => Some((IconName::TriangleAlert, colors.warning)),
        }
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let icon = self.icon(colors);
        let loading = self.kind == ToastKind::Loading;
        let has_action = self.action.is_some();
        let id = self.id.clone();
        popup_surface(px(10.), colors)
            .w_full()
            .text_size(px(14.))
            .line_height(px(20.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.))
                    .py(px(12.))
                    .pl(px(14.))
                    .pr(if has_action { px(24.) } else { px(40.) })
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(8.))
                            .when_some(icon, |this, (icon, color)| {
                                this.child(
                                    div()
                                        .h(px(20.))
                                        .flex()
                                        .items_center()
                                        .child(Icon::new(icon).color(color)),
                                )
                            })
                            .when(loading, |this| {
                                this.child(
                                    div()
                                        .h(px(20.))
                                        .flex()
                                        .items_center()
                                        .opacity(ICON_OPACITY)
                                        .child(Spinner::new(ElementId::from((
                                            id.clone(),
                                            "spinner",
                                        )))),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.popover_foreground)
                                            .child(self.title),
                                    )
                                    .when_some(self.description, |this, description| {
                                        this.child(
                                            div()
                                                .text_color(colors.muted_foreground)
                                                .line_clamp(4)
                                                .child(description),
                                        )
                                    }),
                            ),
                    )
                    .when_some(self.action, |this, (label, handler)| {
                        this.child(
                            Button::new(ElementId::from((id.clone(), "action")))
                                .size(ButtonSize::Xs)
                                .variant(ButtonVariant::Default)
                                .label(label)
                                .on_click(move |event, window, cx| handler(event, window, cx)),
                        )
                    }),
            )
            .child(close_orb(
                ElementId::from((id, "close")),
                self.on_close,
                colors,
            ))
    }
}

/// The 24px circular dismiss control overlapping the top-right corner.
fn close_orb(
    id: ElementId,
    on_close: Option<Handler>,
    colors: &'static Colors,
) -> impl IntoElement {
    div()
        .id(id)
        .absolute()
        .top(px(-6.))
        .right(px(-6.))
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(colors.border_60)
        .bg(colors.popover_92)
        .text_color(colors.muted_foreground)
        .shadow(shadow::SM.to_vec())
        .cursor_pointer()
        .hover(|style| style.bg(colors.popover).text_color(colors.foreground))
        .child(Icon::new(IconName::XBold).size(px(12.)))
        .when_some(on_close, |this, on_close| {
            this.on_click(move |event, window, cx| on_close(event, window, cx))
        })
}

const LIMIT: usize = 3;
const GAP: f32 = 12.;
/// Collapsed height used for the cards behind the front toast before it is measured.
const COLLAPSED_HEIGHT: f32 = 46.;

struct Entry {
    key: u64,
    toast: Toast,
    _timeout: Option<Task<()>>,
}

/// The window's toast stack, top-right below the 52px topbar.
///
/// Create one per window, render it as the last child of the root view, and push toasts:
///
/// ```ignore
/// let toaster = cx.new(|_| Toaster::new());
/// toaster.update(cx, |t, cx| t.push(Toast::new("Copied").kind(ToastKind::Success), window, cx));
/// ```
pub struct Toaster {
    entries: Vec<Entry>,
    next_key: u64,
    expanded: bool,
}

impl Default for Toaster {
    fn default() -> Self {
        Self::new()
    }
}

impl Toaster {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_key: 0,
            expanded: false,
        }
    }

    /// Shows `toast` at the front of the stack. Non-loading toasts close after 5s; the oldest
    /// is dropped beyond 3.
    pub fn push(&mut self, toast: Toast, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.next_key;
        self.next_key += 1;
        let timeout = (toast.kind != ToastKind::Loading).then(|| {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(motion::TOAST_TIMEOUT).await;
                this.update(cx, |this, cx| this.dismiss(key, cx)).ok();
            })
        });
        let entity = cx.entity().downgrade();
        let toast = toast
            .id(ElementId::from(("toast", key)))
            .on_close(move |_, _, cx| {
                entity.update(cx, |this, cx| this.dismiss(key, cx)).ok();
            });
        self.entries.insert(
            0,
            Entry {
                key,
                toast,
                _timeout: timeout,
            },
        );
        self.entries.truncate(LIMIT);
        cx.notify();
    }

    fn dismiss(&mut self, key: u64, cx: &mut Context<Self>) {
        self.entries.retain(|entry| entry.key != key);
        if self.entries.is_empty() {
            self.expanded = false;
        }
        cx.notify();
    }
}

impl Render for Toaster {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.entries.is_empty() {
            return div().into_any_element();
        }
        let colors = cx.colors();
        let viewport = window.viewport_size().width;
        let width = (viewport - px(64.)).min(px(360.));
        let expanded = self.expanded;
        let enter = |key: u64| {
            (
                ElementId::from(("toast-enter", key)),
                Animation::new(motion::TOAST).with_easing(motion::ease_toast),
            )
        };
        let mut stack = div()
            .id("toaster")
            .absolute()
            .top(px(32.) + layout::TOPBAR_HEIGHT)
            .right(px(32.))
            .w(width)
            .flex()
            .flex_col()
            .gap(px(GAP))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.expanded = *hovered;
                cx.notify();
            }));
        if expanded {
            stack = stack.children(self.entries.iter().map(|entry| {
                let (id, animation) = enter(entry.key);
                div()
                    .child(entry.toast.clone())
                    .with_animation(id, animation, |this, t| this.opacity(t))
            }));
        } else {
            // Cards behind the front toast first, so the front one paints over them.
            stack = stack.children(
                self.entries
                    .iter()
                    .enumerate()
                    .skip(1)
                    .rev()
                    .map(|(ix, _)| collapsed_card(ix, width, colors)),
            );
            let front = &self.entries[0];
            let (id, animation) = enter(front.key);
            stack = stack.child(div().relative().child(front.toast.clone()).with_animation(
                id,
                animation,
                move |this, t| this.opacity(t).left((width + px(32.)) * (1. - t)),
            ));
        }
        deferred(stack).with_priority(200).into_any_element()
    }
}

/// A collapsed toast behind the front one: `scale(1 - 0.1 * ix)` drawn as a smaller card
/// whose bottom edge peeks 12px per index below the front toast.
fn collapsed_card(ix: usize, width: Pixels, colors: &'static Colors) -> AnyElement {
    let scale = 1. - 0.1 * ix as f32;
    let height = px(COLLAPSED_HEIGHT);
    popup_surface(px(10.) * scale, colors)
        .absolute()
        .top(height + px(GAP) * ix as f32 - height * scale)
        .left(width * (1. - scale) / 2.)
        .w(width * scale)
        .h(height * scale)
        .into_any_element()
}
