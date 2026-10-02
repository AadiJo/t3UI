//! T3 Connect: the account section of the Connections page and the sign-in dialog, in every
//! state the backend can publish. Built from hand-written [`CloudState`]s (no network).

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px,
};
use t3_app::{
    cloud_ui::{CloudAccount, SignInStep, T3ConnectPanel},
    state::{ConnectionStatus, Environment, EnvironmentKind, fixtures},
};
use t3_client::{
    ConnectionFailure,
    cloud::{
        Account, Availability, CloudState, DiscoveredEnvironment, Discovery, RelayEnvironment,
    },
    connection::{BlockedReason, TransientReason},
};
use t3_protocol::EnvironmentId;
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const EMPTY: &str = include_str!("../../fixtures/empty.json");
const SIZE: (f32, f32) = (1100., 760.);

fn account() -> Account {
    Account {
        user_id: "user_2xAda".into(),
        session_id: "sess_2xAda".into(),
        email: Some("ada@example.com".into()),
        name: Some("Ada Lovelace".into()),
        image_url: None,
    }
}

fn linked(id: &str, label: &str, availability: Availability) -> DiscoveredEnvironment {
    let environment: RelayEnvironment = serde_json::from_value(serde_json::json!({
        "environmentId": id,
        "label": label,
        "linkedAt": "2026-09-30T18:00:00.000Z",
        "endpoint": {
            "httpBaseUrl": format!("https://prod-{id}.example.dev/"),
            "wsBaseUrl": format!("wss://prod-{id}.example.dev/ws"),
            "providerKind": "cloudflare_tunnel",
        },
    }))
    .expect("relay record decodes");
    DiscoveredEnvironment {
        environment,
        availability,
    }
}

/// Every availability the list can show.
fn discovered() -> Vec<DiscoveredEnvironment> {
    vec![
        linked("env-studio", "studio", Availability::Online),
        linked(
            "env-mini",
            "mac-mini",
            Availability::Offline {
                reason: Some("Managed endpoint health request timed out.".into()),
            },
        ),
        linked("env-nas", "nas", Availability::Checking),
        linked(
            "env-lab",
            "lab",
            Availability::Error(ConnectionFailure::transient(
                TransientReason::RelayUnavailable,
                "Relay environment status request timed out. Your DNS or firewall may be blocking T3 Connect. Try another network, such as a phone hotspot.",
            )),
        ),
    ]
}

/// How the scene's connected T3 Connect environment looks, if it has one.
enum Connected {
    None,
    Ok,
    Blocked,
}

struct Setup {
    state: CloudState,
    connected: Connected,
    dialog: Option<SignInStep>,
}

fn build(setup: Setup, window: &mut Window, cx: &mut App) -> AnyView {
    let app_state = fixtures::load(EMPTY, cx).expect("fixture should decode");
    let mut relay = Vec::new();
    if !matches!(setup.connected, Connected::None) {
        let id = EnvironmentId::from("env-devbox");
        let status = match setup.connected {
            Connected::Blocked => ConnectionStatus::Blocked {
                failure: ConnectionFailure::blocked(
                    BlockedReason::Permission,
                    "Relay has no active link for this environment. The environment server may not have re-established its link yet.",
                )
                .with_trace_id(Some("4e9739bf811810ad30fda188358aa7cf".into())),
            },
            _ => ConnectionStatus::Connected { generation: 1 },
        };
        let environment = cx.new(|cx| {
            let mut environment = Environment::new(id.clone(), "devbox", EnvironmentKind::Remote);
            environment.set_status(status, cx);
            environment
        });
        app_state.update(cx, |state, cx| state.add_environment(environment, cx));
        relay.push(id);
    }
    let account = CloudAccount::init_fixture(&app_state, setup.state, true, relay, cx);
    if let Some(step) = setup.dialog {
        account.update(cx, |account, cx| {
            account.open_sign_in_preview(step, window, cx)
        });
    }
    let panel = cx.new(T3ConnectPanel::new);
    cx.new(|_| Page(panel)).into()
}

/// The settings content column (`max-w-3xl p-8`) on the app background.
struct Page(Entity<T3ConnectPanel>);

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .flex()
            .justify_center()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(
                div()
                    .w_full()
                    .max_w(px(768.))
                    .p(px(32.))
                    .child(self.0.clone()),
            )
    }
}

fn signed_in(discovery: Discovery) -> CloudState {
    CloudState {
        account: Some(account()),
        discovery,
    }
}

fn scene(
    name: &'static str,
    theme: ThemeMode,
    build: fn(&mut Window, &mut App) -> AnyView,
) -> Scene {
    Scene::new(name, theme, build).size(SIZE.0, SIZE.1)
}

fn signed_out(window: &mut Window, cx: &mut App) -> AnyView {
    build(
        Setup {
            state: CloudState::default(),
            connected: Connected::None,
            dialog: None,
        },
        window,
        cx,
    )
}

fn linked_list(window: &mut Window, cx: &mut App) -> AnyView {
    build(
        Setup {
            state: signed_in(Discovery {
                refreshing: false,
                error: None,
                environments: discovered(),
            }),
            connected: Connected::Ok,
            dialog: None,
        },
        window,
        cx,
    )
}

fn email_step(email: &str, error: Option<&str>, busy: bool) -> SignInStep {
    SignInStep::Email {
        email: email.into(),
        error: error.map(Into::into),
        busy,
    }
}

pub fn scenes() -> Vec<Scene> {
    vec![
        scene("t3-connect-signed-out-dark", ThemeMode::Dark, signed_out),
        scene("t3-connect-signed-out-light", ThemeMode::Light, signed_out),
        scene("t3-connect-linked-dark", ThemeMode::Dark, linked_list),
        scene("t3-connect-linked-light", ThemeMode::Light, linked_list),
        scene("t3-connect-blocked-dark", ThemeMode::Dark, |window, cx| {
            build(
                Setup {
                    state: signed_in(Discovery {
                        refreshing: false,
                        error: None,
                        environments: vec![linked("env-studio", "studio", Availability::Online)],
                    }),
                    connected: Connected::Blocked,
                    dialog: None,
                },
                window,
                cx,
            )
        }),
        scene("t3-connect-loading-dark", ThemeMode::Dark, |window, cx| {
            build(
                Setup {
                    state: signed_in(Discovery {
                        refreshing: true,
                        error: None,
                        environments: Vec::new(),
                    }),
                    connected: Connected::None,
                    dialog: None,
                },
                window,
                cx,
            )
        }),
        scene(
            "t3-connect-list-error-dark",
            ThemeMode::Dark,
            |window, cx| {
                build(
                    Setup {
                        state: signed_in(Discovery {
                            refreshing: false,
                            error: Some(ConnectionFailure::blocked(
                                BlockedReason::Authentication,
                                "Relay rejected the cloud session token.",
                            )),
                            environments: Vec::new(),
                        }),
                        connected: Connected::None,
                        dialog: None,
                    },
                    window,
                    cx,
                )
            },
        ),
        scene("t3-connect-empty-dark", ThemeMode::Dark, |window, cx| {
            build(
                Setup {
                    state: signed_in(Discovery::default()),
                    connected: Connected::None,
                    dialog: None,
                },
                window,
                cx,
            )
        }),
        scene(
            "t3-connect-sign-in-email-dark",
            ThemeMode::Dark,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(email_step("", None, false)),
                    },
                    window,
                    cx,
                )
            },
        ),
        scene(
            "t3-connect-sign-in-email-light",
            ThemeMode::Light,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(email_step("ada@example.com", None, false)),
                    },
                    window,
                    cx,
                )
            },
        ),
        scene(
            "t3-connect-sign-in-sending-dark",
            ThemeMode::Dark,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(email_step("ada@example.com", None, true)),
                    },
                    window,
                    cx,
                )
            },
        ),
        scene(
            "t3-connect-sign-in-error-dark",
            ThemeMode::Dark,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(email_step(
                            "nobody@example.com",
                            Some(
                                "No T3 account uses this email. Create one at accounts.t3.codes, then sign in here.",
                            ),
                            false,
                        )),
                    },
                    window,
                    cx,
                )
            },
        ),
        scene(
            "t3-connect-sign-in-code-dark",
            ThemeMode::Dark,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(SignInStep::Code {
                            masked_email: "a***@example.com".into(),
                            code: "".into(),
                            error: None,
                            busy: false,
                        }),
                    },
                    window,
                    cx,
                )
            },
        ),
        scene(
            "t3-connect-sign-in-code-error-light",
            ThemeMode::Light,
            |window, cx| {
                build(
                    Setup {
                        state: CloudState::default(),
                        connected: Connected::None,
                        dialog: Some(SignInStep::Code {
                            masked_email: "a***@example.com".into(),
                            code: "123465".into(),
                            error: Some("Incorrect code".into()),
                            busy: false,
                        }),
                    },
                    window,
                    cx,
                )
            },
        ),
    ]
}
