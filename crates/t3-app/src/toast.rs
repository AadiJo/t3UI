//! The top-right toast stack (spec 5.1, `web/components/ui/toast.tsx`).
//!
//! Anything can raise a toast with [`show`]:
//!
//! ```ignore
//! toast::show(Toast::success("Path copied").description(path), cx);
//! toast::show(Toast::error("Failed to archive thread").description(message).stacked(), cx);
//! ```
//!
//! At most three toasts show. The newest is in front; older ones peek 12px below it, scaled down
//! by 10% each. Toasts close after 5s unless [`Toast::persistent`]; thread-scoped toasts only show
//! while their thread is the route.

use std::{rc::Rc, time::Duration};

use gpui_kit::{
    App, AppContext as _, Context, Entity, FontWeight, Global, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Window, div, prelude::FluentBuilder as _,
    px, relative,
};
use t3_logic::ThreadRef;
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, Icon, IconName,
    tokens::{motion, radius, shadow, text},
};

use crate::{chrome::TypeScale as _, state::AppState};

/// Visible toasts (Base UI `limit`).
const LIMIT: usize = 3;
/// Px each toast behind the front one peeks out.
const PEEK: f32 = 12.;

/// Toast type: picks the leading icon and its color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
    Warning,
    Loading,
}

/// Button style of a toast action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastActionStyle {
    Default,
    Outline,
    Destructive,
}

type ActionHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A button in a toast. Clicking it runs the handler and closes the toast.
#[derive(Clone)]
pub struct ToastAction {
    label: SharedString,
    style: ToastActionStyle,
    handler: ActionHandler,
}

/// A toast to show.
#[derive(Clone)]
pub struct Toast {
    kind: ToastKind,
    title: SharedString,
    description: Option<SharedString>,
    stacked: bool,
    actions: Vec<ToastAction>,
    timeout: Option<Duration>,
    thread: Option<ThreadRef>,
}

impl Toast {
    fn new(kind: ToastKind, title: impl Into<SharedString>) -> Self {
        Self {
            kind,
            title: title.into(),
            description: None,
            stacked: false,
            actions: Vec::new(),
            timeout: Some(motion::TOAST_TIMEOUT),
            thread: None,
        }
    }

    pub fn success(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Success, title)
    }

    pub fn error(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Error, title)
    }

    pub fn info(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Info, title)
    }

    pub fn warning(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Warning, title)
    }

    pub fn loading(title: impl Into<SharedString>) -> Self {
        Self::new(ToastKind::Loading, title).persistent()
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// `stackedThreadToast()`: body first, actions on their own right-aligned row.
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    /// Adds an action button. The first one added is the primary action and renders last.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        style: ToastActionStyle,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.actions.push(ToastAction {
            label: label.into(),
            style,
            handler: Rc::new(handler),
        });
        self
    }

    /// Stays until dismissed (`timeout: 0`).
    pub fn persistent(mut self) -> Self {
        self.timeout = None;
        self
    }

    /// Shows only while `thread` is the route thread.
    pub fn for_thread(mut self, thread: ThreadRef) -> Self {
        self.thread = Some(thread);
        self
    }
}

/// Id of a shown toast, for [`dismiss`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ToastId(u64);

struct Entry {
    id: ToastId,
    toast: Toast,
    _timer: Option<Task<()>>,
}

/// The toast stack. One per app; the workspace renders it.
pub struct ToastLayer {
    entries: Vec<Entry>,
    next_id: u64,
}

struct GlobalToasts(Entity<ToastLayer>);

impl Global for GlobalToasts {}

impl ToastLayer {
    /// The app's toast layer, created on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(layer) = cx.try_global::<GlobalToasts>() {
            return layer.0.clone();
        }
        let layer = cx.new(|cx| {
            // Thread-scoped toasts follow the route.
            let app_state = AppState::global(cx);
            cx.observe(&app_state, |_, _, cx| cx.notify()).detach();
            Self {
                entries: Vec::new(),
                next_id: 0,
            }
        });
        cx.set_global(GlobalToasts(layer.clone()));
        layer
    }

    fn push(&mut self, toast: Toast, cx: &mut Context<Self>) -> ToastId {
        let id = ToastId(self.next_id);
        self.next_id += 1;
        let timer = toast.timeout.map(|timeout| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(timeout).await;
                this.update(cx, |this, cx| this.remove(id, cx)).ok();
            })
        });
        self.entries.push(Entry {
            id,
            toast,
            _timer: timer,
        });
        cx.notify();
        id
    }

    fn remove(&mut self, id: ToastId, cx: &mut Context<Self>) {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        if self.entries.len() != before {
            cx.notify();
        }
    }
}

/// Shows a toast.
pub fn show(toast: Toast, cx: &mut App) -> ToastId {
    ToastLayer::global(cx).update(cx, |layer, cx| layer.push(toast, cx))
}

/// Closes a toast early.
pub fn dismiss(id: ToastId, cx: &mut App) {
    ToastLayer::global(cx).update(cx, |layer, cx| layer.remove(id, cx));
}

fn icon_for(kind: ToastKind, colors: &Colors) -> Icon {
    let (name, color) = match kind {
        ToastKind::Success => (IconName::CircleCheck, colors.success),
        ToastKind::Error => (IconName::CircleAlert, colors.destructive),
        ToastKind::Info => (IconName::Info, colors.info),
        ToastKind::Warning => (IconName::TriangleAlert, colors.warning),
        ToastKind::Loading => (IconName::LoaderCircle, colors.foreground.opacity(0.8)),
    };
    Icon::new(name).size(px(16.)).color(color)
}

impl ToastLayer {
    /// One toast card. Cards behind the front one (`index > 0`) fill their slab and hide their
    /// content, like the collapsed Base UI stack.
    fn render_card(&self, entry: &Entry, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let id = entry.id;
        let toast = &entry.toast;
        let hidden_content = index > 0;
        let has_actions = !toast.actions.is_empty();
        let body = div()
            .flex()
            .min_w_0()
            .gap_2()
            .when(!toast.stacked, |this| this.flex_1())
            .child(
                div()
                    .h(px(20.))
                    .w_4()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .child(icon_for(toast.kind, colors)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .when(toast.stacked, |this| this.pr_5())
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(toast.title.clone()),
                    )
                    .when_some(toast.description.clone(), |this, description| {
                        this.child(div().text_color(colors.muted_foreground).child(description))
                    }),
            );
        let actions = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .map(|this| {
                if toast.stacked {
                    this.w_full().justify_end()
                } else {
                    this.flex_shrink_0()
                }
            })
            .children(
                toast
                    .actions
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(position, action)| {
                        let handler = action.handler.clone();
                        let layer = cx.entity().downgrade();
                        let variant = match action.style {
                            ToastActionStyle::Default => ButtonVariant::Default,
                            ToastActionStyle::Outline => ButtonVariant::Outline,
                            ToastActionStyle::Destructive => ButtonVariant::Destructive,
                        };
                        Button::new(SharedString::from(format!(
                            "toast-{}-action-{position}",
                            id.0
                        )))
                        .size(ButtonSize::Xs)
                        .variant(variant)
                        .label(action.label.clone())
                        .on_click(move |_, window, cx| {
                            handler(window, cx);
                            layer.update(cx, |this, cx| this.remove(id, cx)).ok();
                        })
                    }),
            );
        let content = div()
            .pl(px(14.))
            .type_scale(text::SM)
            .when(hidden_content, |this| this.opacity(0.))
            .map(|this| {
                if toast.stacked {
                    this.py(px(10.)).pr(px(14.)).flex().flex_col().gap_2()
                } else {
                    this.py_3()
                        .pr(if has_actions { px(24.) } else { px(40.) })
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.))
                }
            })
            .child(body)
            .when(has_actions, |this| this.child(actions));

        div()
            .relative()
            .w_full()
            .when(hidden_content, |this| this.h_full())
            .rounded(radius::LG)
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow(shadow::LG_5.to_vec())
            .child(content)
            .when(!hidden_content, |this| {
                // Close orb: top -6, right -6, 24px circle.
                this.child(
                    div()
                        .id(SharedString::from(format!("toast-{}-close", id.0)))
                        .absolute()
                        .top(px(-6.))
                        .right(px(-6.))
                        .size_6()
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
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| this.remove(id, cx)))
                        .child(Icon::new(IconName::X).size(px(12.))),
                )
            })
    }
}

impl Render for ToastLayer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let route_thread = AppState::global(cx).read(cx).route().thread().cloned();
        let visible: Vec<&Entry> = self
            .entries
            .iter()
            .rev()
            .filter(|entry| {
                entry
                    .toast
                    .thread
                    .as_ref()
                    .is_none_or(|thread| Some(thread) == route_thread.as_ref())
            })
            .take(LIMIT)
            .collect();
        if visible.is_empty() {
            return div().into_any_element();
        }
        let inset = px(32.);
        let width = (window.viewport_size().width - inset * 2.).min(px(360.));
        let front = visible[0];
        // The front toast is in flow and sizes the stack; older ones are slabs behind it.
        let behind: Vec<_> = visible
            .iter()
            .enumerate()
            .skip(1)
            .rev()
            .map(|(index, entry)| {
                let shrink = index as f32 * 0.1;
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(PEEK * index as f32))
                    .bottom(px(-PEEK * index as f32))
                    .child(
                        div()
                            .absolute()
                            .top(relative(shrink))
                            .bottom_0()
                            .left(relative(shrink / 2.))
                            .right(relative(shrink / 2.))
                            .child(self.render_card(entry, index, cx)),
                    )
            })
            .collect();
        div()
            .absolute()
            .top(inset + t3_ui::tokens::layout::TOPBAR_HEIGHT)
            .right(inset)
            .w(width)
            .child(
                div()
                    .relative()
                    .w_full()
                    .children(behind)
                    .child(self.render_card(front, 0, cx)),
            )
            .into_any_element()
    }
}
