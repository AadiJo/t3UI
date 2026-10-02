//! coss `Toast` (`ui/toast.tsx`): the toast card with its status icon, title, description,
//! `xs` action and the corner close orb. Visual primitive only: stacking, timing and
//! scoping belong to the app shell (`t3_app::toast`).

use std::rc::Rc;

use gpui_kit::{
    App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use super::{
    button::{Button, ButtonSize, ButtonVariant},
    feedback::Spinner,
    popup_surface,
};
use crate::{
    ActiveColors as _, Colors, Icon, IconName,
    tokens::{ICON_OPACITY, shadow},
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

/// One toast card (radius 10, `shadow-lg/5`, content padding 14 / 12 / 40). The shell's toast
/// stack positions it; `on_close` wires the close orb.
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

    /// Called by the close orb.
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
