//! Connections page (`ConnectionsSettings.tsx`, connections.md 6).
//!
//! - "This environment": shown when the primary environment is a server on this machine
//!   (loopback), the native stand-in for the fork's desktop backend. One "Network access" row with
//!   a disabled switch: exposure is decided where `t3 serve` is launched.
//! - "Remote environments": every other environment, sorted by label, with its connection dot
//!   and Connect / Disconnect (Disconnect forgets it, no confirm, like the fork). The header's
//!   "Add environment" opens [`AddEnvironmentDialog`]. T3 Connect rows belong under the saved
//!   rows once `cloud_ui` lands.

use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, prelude::FluentBuilder as _, px,
};
use t3_client::ConnectionStatus;
use t3_protocol::EnvironmentId;
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, Icon, IconName, Switch,
    TooltipExt as _,
    tokens::{shadow, text},
};

use super::layout::{PAGE_MAX_WIDTH, SettingsRow, SettingsSection, page};
use crate::{
    chrome::{TypeScale as _, under_xs},
    pairing::{AddEnvironmentDialog, forget_environment},
    state::{AppState, Environment, EnvironmentKind},
    toast::{self, Toast},
};

/// The Connections page.
pub struct ConnectionsPage {
    app_state: Entity<AppState>,
    dialog: Entity<AddEnvironmentDialog>,
    removing: Option<EnvironmentId>,
    _subscriptions: Vec<Subscription>,
}

impl ConnectionsPage {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dialog = cx.new(|cx| AddEnvironmentDialog::new(window, cx));
        Self {
            _subscriptions: vec![
                cx.observe(&app_state, |_, _, cx| cx.notify()),
                cx.observe(&dialog, |_, _, cx| cx.notify()),
            ],
            app_state,
            dialog,
            removing: None,
        }
    }

    /// Opens the Add Environment dialog (also used by snapshot scenes).
    pub fn open_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog.update(cx, |dialog, cx| dialog.open(window, cx));
    }

    fn connect(&mut self, environment: &Entity<Environment>, cx: &mut Context<Self>) {
        if let Some(client) = environment.read(cx).client() {
            client.retry_now();
        }
    }

    fn disconnect(&mut self, id: EnvironmentId, cx: &mut Context<Self>) {
        self.removing = Some(id.clone());
        cx.notify();
        let task = forget_environment(id, cx);
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.removing = None;
                if let Err(message) = result {
                    let message = if message.is_empty() {
                        "Failed to remove backend.".to_owned()
                    } else {
                        message
                    };
                    toast::show(
                        Toast::error("Could not remove backend")
                            .description(message)
                            .stacked(),
                        cx,
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "This environment" for a loopback primary.
    fn this_environment(&self, cx: &mut Context<Self>) -> Option<SettingsSection> {
        let primary = self.app_state.read(cx).primary_environment()?;
        let primary = primary.read(cx);
        if primary.kind() != EnvironmentKind::Local {
            return None;
        }
        let remote_reachable = primary
            .config()
            .is_some_and(|config| config.auth.policy == "remote-reachable");
        let description = if remote_reachable {
            "This backend is already configured for remote access. Network exposure changes must be made where the server is launched."
        } else {
            "This backend is only reachable on this machine. Restart it with a non-loopback host to enable remote pairing."
        };
        Some(
            SettingsSection::new("This environment").child(
                SettingsRow::new("Network access", description).control(
                    div()
                        .id("network-access")
                        .tooltip_text(
                            "Network exposure changes restart the backend and must be controlled where the server process is launched.",
                        )
                        .child(
                            Switch::new("network-access-switch")
                                .checked(remote_reachable)
                                .disabled(true),
                        ),
                ),
            ),
        )
    }

    fn saved_row(
        &self,
        environment: &Entity<Environment>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.colors();
        let entity = environment.clone();
        let environment = environment.read(cx);
        let id = environment.id().clone();
        let status = environment.status().clone();
        let removing = self.removing.as_ref() == Some(&id);
        let connecting = matches!(
            status,
            ConnectionStatus::Connecting { .. } | ConnectionStatus::Reconnecting { .. }
        );
        let connected = status.is_connected();
        let error = match &status {
            ConnectionStatus::Blocked { .. } | ConnectionStatus::Reconnecting { .. } => {
                Some(status.status_text())
            }
            ConnectionStatus::Connecting {
                last_failure: Some(_),
                ..
            } => Some(status.status_text()),
            _ => None,
        };
        let trace_id = match &status {
            ConnectionStatus::Blocked { failure }
            | ConnectionStatus::Reconnecting { failure, .. } => failure.trace_id.clone(),
            _ => None,
        };
        let label = if connected {
            if removing {
                "Disconnecting…"
            } else {
                "Disconnect"
            }
        } else if connecting {
            "Connecting…"
        } else {
            "Connect"
        };
        let button = Button::new(SharedString::from(format!("connection-{id}")))
            .variant(ButtonVariant::Outline)
            .size(ButtonSize::Xs)
            .label(label)
            .disabled(connecting || removing)
            .on_click(cx.listener(move |this, _, _, cx| {
                if connected {
                    this.disconnect(id.clone(), cx);
                } else {
                    this.connect(&entity, cx);
                }
            }));
        div()
            .px_5()
            .py_4()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .min_h_5()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(status_dot(environment.id(), &status, colors))
                            .child(
                                div()
                                    .type_scale(text::SM)
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(environment.label().clone()),
                            ),
                    )
                    .children(error.map(|error| {
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .type_scale(text::XS)
                            .text_color(colors.destructive)
                            .child(div().min_w_0().truncate().child(error))
                            .children(trace_id.map(|trace_id| {
                                div()
                                    .id("copy-trace-id")
                                    .flex_none()
                                    .underline()
                                    .cursor_pointer()
                                    .on_click(move |_, _, cx| {
                                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(
                                            trace_id.clone(),
                                        ));
                                        toast::show(
                                            Toast::success("Trace ID copied")
                                                .description(trace_id.clone()),
                                            cx,
                                        );
                                    })
                                    .child("Copy trace ID")
                            }))
                    })),
            )
            .child(div().flex_none().child(button))
    }
}

/// `ConnectionStatusDot`: an 8px dot in a 12px box with the status text as tooltip. The fork's
/// ping animation while connecting is replaced by a static ring (no continuous repaint).
fn status_dot(
    id: &EnvironmentId,
    status: &ConnectionStatus,
    colors: &Colors,
) -> impl IntoElement + use<> {
    let (dot, ring) = match status {
        ConnectionStatus::Connected { .. } => (colors.success, None),
        ConnectionStatus::Connecting { .. } | ConnectionStatus::Reconnecting { .. } => {
            (colors.warning, Some(colors.warning.opacity(0.3)))
        }
        ConnectionStatus::Blocked { .. } => (colors.destructive, None),
        ConnectionStatus::Available | ConnectionStatus::Offline => {
            (colors.muted_foreground.opacity(0.4), None)
        }
    };
    div()
        .id(SharedString::from(format!("status-dot-{id}")))
        .relative()
        .size_3()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .tooltip_text(status.status_text())
        .when_some(ring, |this, ring| this.bg(ring))
        .child(div().size_2().rounded_full().bg(dot))
}

/// `EmptyRemoteEnvironments`: icon tile, title, and hint, 48px padding.
fn empty_remote(colors: &Colors) -> impl IntoElement + use<> {
    div()
        .min_h(px(208.))
        .p_12()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_6()
        .child(
            div()
                .mb_6()
                .size_9()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .border_1()
                .border_color(colors.border)
                .bg(colors.card)
                .text_color(colors.foreground)
                .when(!colors.is_dark, |this| this.shadow(shadow::SM_5.to_vec()))
                .child(Icon::new(IconName::ChevronsLeftRightEllipsis).size(px(18.))),
        )
        .child(
            div()
                .max_w(px(384.))
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .type_scale(text::XL)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                        .child("No saved remote environments"),
                )
                .child(
                    div()
                        .mt_1()
                        .type_scale(text::SM)
                        .text_color(colors.muted_foreground)
                        .child(
                            "Click \u{201c}Add environment\u{201d} to pair another environment.",
                        ),
                ),
        )
}

impl Render for ConnectionsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let this_environment = self.this_environment(cx);
        let state = self.app_state.read(cx);
        let local_primary = state
            .primary_environment()
            .filter(|primary| primary.read(cx).kind() == EnvironmentKind::Local)
            .map(|primary| primary.read(cx).id().clone());
        let mut remote: Vec<Entity<Environment>> = state
            .environments()
            .iter()
            .filter(|environment| Some(environment.read(cx).id()) != local_primary.as_ref())
            .cloned()
            .collect();
        remote.sort_by_cached_key(|environment| environment.read(cx).label().to_lowercase());
        let rows: Vec<_> = remote
            .iter()
            .map(|environment| self.saved_row(environment, cx).into_any_element())
            .collect();
        let add_action = div()
            .id("add-environment")
            .h_5()
            .px_1()
            .gap_1()
            .flex()
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .type_scale(under_xs(11.))
            .text_color(colors.muted_foreground_60)
            .hover(|style| style.text_color(colors.muted_foreground))
            .tooltip_text("Add environment")
            .on_click(cx.listener(|this, _, window, cx| this.open_add_dialog(window, cx)))
            .child(Icon::new(IconName::Plus).size(px(12.)))
            .child("Add environment");
        let remote_section = if rows.is_empty() {
            SettingsSection::new("Remote environments")
                .header_action(add_action)
                .child(empty_remote(colors))
        } else {
            SettingsSection::new("Remote environments")
                .header_action(add_action)
                .children(rows)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(page(
                "settings-connections",
                PAGE_MAX_WIDTH,
                this_environment
                    .map(IntoElement::into_any_element)
                    .into_iter()
                    .chain([remote_section.into_any_element()]),
            ))
            .child(self.dialog.clone())
    }
}
