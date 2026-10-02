//! T3 Connect rows in the fork's Connections style (`settingsLayout.tsx`, `itemRows.ts`,
//! `CloudEnvironmentConnectList.tsx`, `ConnectionStatusDot.tsx`; connections.md 6.5, 6.6).
//!
//! The fork's checking/connecting status dots ping continuously; here they get a static halo.

use std::collections::HashSet;

use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, RenderOnce, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use t3_client::{
    ConnectionStatus,
    cloud::{Availability, DiscoveredEnvironment},
};
use t3_protocol::EnvironmentId;
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, IconName, Skeleton,
    TooltipExt as _,
    tokens::{radius, shadow, text},
};

use super::CloudAccount;
use crate::{
    chrome::TypeScale as _,
    state::Environment,
    toast::{self, Toast},
};

/// The "T3 Connect" settings section: account row, connected T3 Connect environments, and
/// the account's other linked environments with Connect. Refreshes the list when created.
pub struct T3ConnectPanel {
    account: Option<Entity<CloudAccount>>,
}

impl T3ConnectPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let account = CloudAccount::global(cx);
        if let Some(account) = &account {
            cx.observe(account, |_, _, cx| cx.notify()).detach();
            let app_state = account.read(cx).app_state().clone();
            cx.observe(&app_state, |_, _, cx| cx.notify()).detach();
            account.update(cx, |account, cx| account.refresh(cx));
        }
        T3ConnectPanel { account }
    }
}

impl Render for T3ConnectPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(account) = self.account.clone() else {
            return div().into_any_element();
        };
        let colors = cx.colors();
        let model = account.read(cx);
        let signed_in = model.account().is_some();
        let dialog = model.sign_in_dialog();
        let app_state = model.app_state().clone();
        let relay: Vec<Entity<Environment>> = model
            .relay_environments()
            .iter()
            .filter_map(|id| app_state.read(cx).environment(id, cx))
            .collect();
        let standalone = relay.is_empty();
        let mut card = settings_card(colors).child(AccountRow::new(account.clone()));
        if signed_in {
            card = card
                .children(
                    relay
                        .into_iter()
                        .map(|environment| RelayEnvironmentRow::new(environment, account.clone())),
                )
                .child(CloudEnvironmentRows::new(account.clone()).standalone(standalone));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(section_title("T3 Connect", colors))
            .child(card)
            .children(dialog)
            .into_any_element()
    }
}

/// `SettingsSection` heading: hairline, then 11px semibold uppercase `foreground/50`.
fn section_title(title: &str, colors: &Colors) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(11.))
                .line_height(px(16.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.foreground.alpha(0.5))
                .child(div().w(px(12.)).h(px(1.)).bg(colors.border))
                .child(SharedString::from(title.to_uppercase())),
        )
        .child(div().h(px(20.)))
}

/// `SettingsSection` body: `rounded-2xl border bg-card shadow-sm/4` with the coss bevel.
fn settings_card(colors: &Colors) -> gpui_kit::Div {
    div()
        .relative()
        .flex()
        .flex_col()
        .rounded(radius::XL2)
        .border_1()
        .border_color(colors.border)
        .bg(colors.card)
        .text_color(colors.card_foreground)
        .when(!colors.is_dark, |this| this.shadow(shadow::XS_5.to_vec()))
}

/// One item row: `border-t border-border/60 px-5 py-4` (none on the first row).
fn item_row(first: bool, colors: &Colors) -> gpui_kit::Div {
    div().px(px(20.)).py(px(16.)).when(!first, |this| {
        this.border_t_1().border_color(colors.border_60)
    })
}

/// The 12px status dot (8px dot), with a tooltip. `halo` replaces the fork's ping.
fn status_dot(
    id: SharedString,
    color: Hsla,
    halo: Option<Hsla>,
    tooltip: SharedString,
) -> impl IntoElement {
    div()
        .id(id)
        .relative()
        .flex()
        .flex_none()
        .size(px(12.))
        .items_center()
        .justify_center()
        .rounded_full()
        .when_some(halo, |this, halo| {
            this.child(div().absolute().inset_0().rounded_full().bg(halo))
        })
        .child(div().relative().size(px(8.)).rounded_full().bg(color))
        .tooltip_text(tooltip)
}

/// Account status: "Sign in" when signed out, name, email and "Sign out" when signed in.
#[derive(IntoElement)]
pub struct AccountRow {
    account: Entity<CloudAccount>,
}

impl AccountRow {
    pub fn new(account: Entity<CloudAccount>) -> Self {
        AccountRow { account }
    }
}

impl RenderOnce for AccountRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let model = self.account.read(cx);
        let signing_out = model.signing_out();
        let signed_in = model.account().cloned();
        let account = self.account.clone();
        let (title, description, avatar, control): (SharedString, SharedString, _, AnyElement) =
            match &signed_in {
                None => (
                    "T3 Connect".into(),
                    "Sign in to reach the environments linked to your T3 account from this device."
                        .into(),
                    None,
                    Button::new("t3-connect-sign-in")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .icon(IconName::LogIn)
                        .label("Sign in")
                        .on_click(move |_, window, cx| {
                            account.update(cx, |account, cx| account.open_sign_in(window, cx))
                        })
                        .into_any_element(),
                ),
                Some(signed_in) => (
                    signed_in.display_name().to_owned().into(),
                    signed_in
                        .email
                        .clone()
                        .filter(|email| Some(email.as_str()) != signed_in.name.as_deref())
                        .map(|email| format!("Signed in to T3 Connect as {email}"))
                        .unwrap_or_else(|| "Signed in to T3 Connect".into())
                        .into(),
                    Some(avatar(signed_in.display_name(), colors)),
                    Button::new("t3-connect-sign-out")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Sm)
                        .label(if signing_out {
                            "Signing out…"
                        } else {
                            "Sign out"
                        })
                        .disabled(signing_out)
                        .on_click(move |_, _, cx| {
                            account.update(cx, |account, cx| account.sign_out(cx))
                        })
                        .into_any_element(),
                ),
            };
        div()
            .px(px(20.))
            .py(px(14.))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .min_w_0()
                    .flex_1()
                    .children(avatar)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .truncate()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .type_scale(text::XS)
                                    .text_color(colors.muted_foreground_80)
                                    .child(description),
                            ),
                    ),
            )
            .child(div().flex_none().child(control))
    }
}

/// A 28px circle with the account's initial.
fn avatar(name: &str, colors: &Colors) -> AnyElement {
    let initial: String = name
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    div()
        .flex()
        .flex_none()
        .size(px(28.))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(colors.muted)
        .text_color(colors.foreground)
        .type_scale(text::XS)
        .font_weight(FontWeight::SEMIBOLD)
        .child(initial)
        .into_any_element()
}

/// The account's linked environments that are not connected here, each with Connect
/// (`CloudEnvironmentConnectRows`). `standalone`: the card has no other environment rows, so
/// loading, error and empty states show too.
#[derive(IntoElement)]
pub struct CloudEnvironmentRows {
    account: Entity<CloudAccount>,
    standalone: bool,
    first: bool,
}

impl CloudEnvironmentRows {
    pub fn new(account: Entity<CloudAccount>) -> Self {
        CloudEnvironmentRows {
            account,
            standalone: true,
            first: false,
        }
    }

    pub fn standalone(mut self, standalone: bool) -> Self {
        self.standalone = standalone;
        self
    }

    /// The first row has no top border (it is the first row of its card).
    pub fn first(mut self, first: bool) -> Self {
        self.first = first;
        self
    }
}

impl RenderOnce for CloudEnvironmentRows {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let model = self.account.read(cx);
        let app_state = model.app_state().read(cx);
        let live_clock = app_state.clock_is_live();
        let present: HashSet<EnvironmentId> = app_state
            .environments()
            .iter()
            .map(|environment| environment.read(cx).id().clone())
            .collect();
        let discovery = &model.state().discovery;
        let visible: Vec<&DiscoveredEnvironment> = discovery
            .environments
            .iter()
            .filter(|entry| !present.contains(&entry.environment.environment_id))
            .collect();
        let first = self.first;

        if visible.is_empty() {
            if !self.standalone {
                return div().into_any_element();
            }
            if discovery.refreshing && discovery.environments.is_empty() {
                return skeleton_row(first, live_clock, colors).into_any_element();
            }
            if let Some(error) = discovery.error.as_ref().filter(|_| !discovery.refreshing) {
                let account = self.account.clone();
                return item_row(first, colors)
                    .child(
                        div()
                            .type_scale(text::SM)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.destructive)
                            .child("Could not load T3 Connect environments"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .type_scale(text::XS)
                            .text_color(colors.muted_foreground)
                            .child(error.detail.clone()),
                    )
                    .child(
                        div().mt_3().child(
                            Button::new("t3-connect-retry")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Sm)
                                .label("Try again")
                                .on_click(move |_, _, cx| {
                                    account.update(cx, |account, cx| account.refresh(cx))
                                }),
                        ),
                    )
                    .into_any_element();
            }
            return item_row(first, colors)
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(
                    "No other environments are published to your account yet. Run `t3 connect` on a machine to publish it, and it will show up here.",
                )
                .into_any_element();
        }

        let busy = model.connecting().cloned();
        div()
            .flex()
            .flex_col()
            .children(visible.into_iter().enumerate().map(|(index, entry)| {
                discovered_row(
                    entry,
                    first && index == 0,
                    busy.as_ref(),
                    &self.account,
                    colors,
                )
            }))
            .into_any_element()
    }
}

fn skeleton_row(first: bool, live_clock: bool, colors: &Colors) -> impl IntoElement {
    let skeleton = |id: &'static str| {
        let skeleton = Skeleton::new(id);
        if live_clock {
            skeleton
        } else {
            skeleton.still()
        }
    };
    item_row(first, colors).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .child(
                        skeleton("t3-connect-skeleton-title")
                            .h(px(16.))
                            .w(px(128.))
                            .rounded_full(),
                    )
                    .child(
                        skeleton("t3-connect-skeleton-subtitle")
                            .h(px(12.))
                            .w(px(80.))
                            .rounded_full(),
                    ),
            )
            .child(
                skeleton("t3-connect-skeleton-button")
                    .h(px(28.))
                    .w(px(64.))
                    .rounded(radius::MD),
            ),
    )
}

fn discovered_row(
    entry: &DiscoveredEnvironment,
    first: bool,
    connecting: Option<&EnvironmentId>,
    account: &Entity<CloudAccount>,
    colors: &Colors,
) -> AnyElement {
    let environment = &entry.environment;
    let id = environment.environment_id.clone();
    let (dot, halo, tooltip, subtitle, failed): (
        Hsla,
        Option<Hsla>,
        SharedString,
        SharedString,
        bool,
    ) = match &entry.availability {
        Availability::Online => (
            colors.success,
            None,
            "Relay online".into(),
            "Available · Relay online".into(),
            false,
        ),
        Availability::Offline { reason } => (
            colors.muted_foreground.alpha(0.35),
            None,
            reason
                .clone()
                .unwrap_or_else(|| "Relay offline".into())
                .into(),
            "Available · Relay offline".into(),
            false,
        ),
        Availability::Checking => (
            colors.warning,
            Some(colors.warning_32),
            "Checking relay status".into(),
            "Available · Checking relay status…".into(),
            false,
        ),
        Availability::Error(failure) => (
            colors.destructive,
            None,
            failure.detail.clone().into(),
            failure.detail.clone().into(),
            true,
        ),
    };
    let this_connecting = connecting == Some(&id);
    let account = account.clone();
    let record = environment.clone();
    item_row(first, colors)
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(status_dot(
                                    SharedString::from(format!("t3-connect-dot-{id}")),
                                    dot,
                                    halo,
                                    tooltip,
                                ))
                                .child(
                                    div()
                                        .type_scale(text::SM)
                                        .font_weight(FontWeight::MEDIUM)
                                        .truncate()
                                        .child(environment.label.clone()),
                                ),
                        )
                        .child(
                            div()
                                .mt_1()
                                .type_scale(text::XS)
                                .truncate()
                                .text_color(if failed {
                                    colors.destructive_foreground
                                } else {
                                    colors.muted_foreground
                                })
                                .child(subtitle),
                        ),
                )
                .child(
                    Button::new(SharedString::from(format!("t3-connect-connect-{id}")))
                        .size(ButtonSize::Sm)
                        .label(if this_connecting {
                            "Connecting…"
                        } else {
                            "Connect"
                        })
                        .disabled(connecting.is_some())
                        .on_click(move |_, _, cx| {
                            account
                                .update(cx, |account, cx| account.connect_environment(&record, cx))
                        }),
                ),
        )
        .into_any_element()
}

/// A connected T3 Connect environment (`SavedBackendListRow` for a relay target): status,
/// "T3 Connect", errors with "Copy trace ID", and Connect / Connecting… / Disconnect.
#[derive(IntoElement)]
pub struct RelayEnvironmentRow {
    environment: Entity<Environment>,
    account: Entity<CloudAccount>,
    first: bool,
}

impl RelayEnvironmentRow {
    pub fn new(environment: Entity<Environment>, account: Entity<CloudAccount>) -> Self {
        RelayEnvironmentRow {
            environment,
            account,
            first: false,
        }
    }

    pub fn first(mut self, first: bool) -> Self {
        self.first = first;
        self
    }
}

/// The UI phase of a connection status (`presentation.ts`): dot color and halo.
fn status_paint(status: &ConnectionStatus, colors: &Colors) -> (Hsla, Option<Hsla>) {
    match status {
        ConnectionStatus::Available | ConnectionStatus::Offline => {
            (colors.muted_foreground.alpha(0.4), None)
        }
        ConnectionStatus::Connecting { .. } | ConnectionStatus::Reconnecting { .. } => {
            (colors.warning, Some(colors.warning_32))
        }
        ConnectionStatus::Connected { .. } => (colors.success, None),
        ConnectionStatus::Blocked { .. } => (colors.destructive, None),
    }
}

impl RenderOnce for RelayEnvironmentRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.colors();
        let environment = self.environment.read(cx);
        let id = environment.id().clone();
        let status = environment.status().clone();
        let label = environment.label().clone();
        let (dot, halo) = status_paint(&status, colors);
        let status_text: SharedString = status.status_text().into();
        let failure = match &status {
            ConnectionStatus::Blocked { failure }
            | ConnectionStatus::Reconnecting { failure, .. } => Some(failure.clone()),
            _ => None,
        };
        let disconnecting = self.account.read(cx).disconnecting() == Some(&id);
        let account = self.account.clone();
        let client = environment.client().cloned();
        let action = match &status {
            ConnectionStatus::Connected { .. } => {
                let id = id.clone();
                Button::new(SharedString::from(format!("t3-connect-disconnect-{id}")))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .label(if disconnecting {
                        "Disconnecting…"
                    } else {
                        "Disconnect"
                    })
                    .disabled(disconnecting)
                    .on_click(move |_, _, cx| {
                        account.update(cx, |account, cx| account.disconnect_environment(&id, cx))
                    })
            }
            ConnectionStatus::Connecting { .. } => {
                Button::new(SharedString::from(format!("t3-connect-connecting-{id}")))
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .label("Connecting…")
                    .disabled(true)
            }
            _ => Button::new(SharedString::from(format!("t3-connect-reconnect-{id}")))
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Xs)
                .label("Connect")
                .on_click(move |_, _, _| {
                    if let Some(client) = &client {
                        client.connect();
                        client.retry_now();
                    }
                }),
        };
        let trace_id = failure.as_ref().and_then(|f| f.trace_id.clone());
        item_row(self.first, colors).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(status_dot(
                                    SharedString::from(format!("t3-connect-saved-dot-{id}")),
                                    dot,
                                    halo,
                                    status_text.clone(),
                                ))
                                .child(
                                    div()
                                        .type_scale(text::SM)
                                        .font_weight(FontWeight::MEDIUM)
                                        .truncate()
                                        .child(label),
                                ),
                        )
                        .child(
                            div()
                                .mt_1()
                                .type_scale(text::XS)
                                .text_color(colors.muted_foreground)
                                .child("T3 Connect"),
                        )
                        .when(failure.is_some(), |this| {
                            this.child(
                                div()
                                    .mt_1()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .min_w_0()
                                    .type_scale(text::XS)
                                    .text_color(colors.destructive_foreground)
                                    .child(div().min_w_0().truncate().child(status_text))
                                    .when_some(trace_id, |this, trace_id| {
                                        this.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "t3-connect-trace-{id}"
                                                )))
                                                .flex_none()
                                                .underline()
                                                .cursor_pointer()
                                                .child("Copy trace ID")
                                                .on_click(move |_, _, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(trace_id.clone()),
                                                    );
                                                    toast::show(
                                                        Toast::success("Trace ID copied"),
                                                        cx,
                                                    );
                                                }),
                                        )
                                    }),
                            )
                        }),
                )
                .child(action),
        )
    }
}
