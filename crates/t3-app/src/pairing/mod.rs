//! Adding an environment by pairing (connections.md 2, 6.3.1, 6.4).
//!
//! - [`flow`]: the GPUI-free half: field parsing, pairing, saving, forgetting.
//! - [`PairingForm`]: the "Remote link" form (Host, Pairing code, "Add environment"). Pasting a
//!   pairing link or `t3 serve` output into Host fills both fields. On success the environment
//!   is saved to the catalog, started, and added to [`AppState`], so it shows in the sidebar.
//! - [`AddEnvironmentDialog`]: Settings > Connections > "Add environment".
//! - [`PairView`]: the `Route::Pair` main view, the same form on its own page.

pub mod flow;

use gpui_kit::{
    Animation, AnimationExt as _, App, AppContext as _, Context, ElementId, Entity, EventEmitter,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, base,
    component::input::{InputEvent, InputState},
    div,
    prelude::FluentBuilder as _,
    px, relative,
};
use t3_client::{
    ClientInfo, EnvironmentOptions,
    store::{CatalogStore, SecretBackend, open_secret_store},
};
use t3_protocol::EnvironmentId;
use t3_ui::{
    ActiveColors as _, Button, ButtonVariant, DialogDescription, DialogHeader, DialogPanel,
    DialogPopup, DialogTitle, Icon, IconName, Input, dialog_backdrop,
    tokens::{layout, motion, text},
};

use crate::{
    chrome::{TypeScale as _, drag_region, under_xs},
    state::{AppState, Environment, Route, boot},
    toast::{self, Toast},
    workspace::collapsed_titlebar_inset,
};

/// The app's catalog and secret store (`T3UI_DATA_DIR` or the platform data dir;
/// `T3UI_SECRET_STORE` picks the secret backend).
pub fn stores() -> flow::PairingStores {
    flow::PairingStores {
        catalog: CatalogStore::new(),
        secrets: open_secret_store(SecretBackend::from_env()),
    }
}

/// Starts a saved environment and adds it to the app, replacing a running one with the same id
/// (re-pairing).
fn start_environment(added: flow::AddedEnvironment, cx: &mut App) -> EnvironmentId {
    let kind = boot::kind_of(added.endpoint.as_ref());
    let client =
        t3_client::Environment::start(EnvironmentOptions::from_saved(&added.saved, added.endpoint));
    let id = client.id().clone();
    let environment = cx.new(|cx| Environment::connected(client, kind, cx));
    AppState::global(cx).update(cx, |state, cx| {
        state.remove_environment(&id, cx);
        state.add_environment(environment, cx);
    });
    id
}

/// Disconnects and forgets an environment (Connections "Disconnect"): stops its connection,
/// removes it from the app, and deletes its catalog entry and token. Resolves with the
/// user-facing error, if any.
pub fn forget_environment(id: EnvironmentId, cx: &mut App) -> gpui_kit::Task<Result<(), String>> {
    AppState::global(cx).update(cx, |state, cx| state.remove_environment(&id, cx));
    cx.background_spawn(async move { flow::forget(&id, &stores()) })
}

/// Emitted by [`PairingForm`].
#[derive(Clone, Debug)]
pub enum PairingFormEvent {
    /// The environment was paired, saved, and started.
    Added(EnvironmentId),
}

/// The "Remote link" form. Owns the two fields and the add request.
pub struct PairingForm {
    host: Entity<InputState>,
    code: Entity<InputState>,
    error: Option<SharedString>,
    adding: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<PairingFormEvent> for PairingForm {}

impl PairingForm {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let host = cx.new(|cx| InputState::new(window, cx).placeholder("backend.example.com"));
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("PAIRCODE"));
        let subscriptions = vec![
            cx.subscribe_in(&host, window, |this, host, event, window, cx| match event {
                // A pasted pairing link fills both fields (`handleSavedBackendHostChange`).
                InputEvent::Change => {
                    let value = host.read(cx).value();
                    if let Some(split) = flow::split_pairing_input(&value) {
                        host.update(cx, |host, cx| host.set_value(split.host, window, cx));
                        this.code
                            .update(cx, |code, cx| code.set_value(split.code, window, cx));
                    }
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&code, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(window, cx);
                }
            }),
        ];
        Self {
            host,
            code,
            error: None,
            adding: false,
            _subscriptions: subscriptions,
        }
    }

    /// Clears both fields and the error.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.host
            .update(cx, |host, cx| host.set_value("", window, cx));
        self.code
            .update(cx, |code, cx| code.set_value("", window, cx));
        self.error = None;
        cx.notify();
    }

    /// Focuses the Host field.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.host.update(cx, |host, cx| host.focus(window, cx));
    }

    /// Pairs with the entered environment (`handleAddSavedBackend`, remote mode). Errors show
    /// inline and as a "Could not add backend" toast.
    pub fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.adding {
            return;
        }
        let host = self.host.read(cx).value();
        let code = self.code.read(cx).value();
        let target = match flow::resolve_fields(&host, &code) {
            Ok(target) => target,
            Err(message) => return self.fail(message, cx),
        };
        self.adding = true;
        self.error = None;
        cx.notify();
        let pairing =
            t3_client::runtime::spawn(flow::pair_and_save(target, ClientInfo::default(), stores()));
        cx.spawn_in(window, async move |this, cx| {
            let result = pairing.await;
            this.update_in(cx, |this, window, cx| {
                this.adding = false;
                match result {
                    Ok(added) => {
                        let id = start_environment(added, cx);
                        this.reset(window, cx);
                        toast::show(
                            Toast::success("Backend added").description(
                                "The environment is saved and will reconnect on app startup.",
                            ),
                            cx,
                        );
                        cx.emit(PairingFormEvent::Added(id));
                    }
                    Err(message) => this.fail(message, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        let message = if message.is_empty() {
            "Failed to add backend.".to_owned()
        } else {
            message
        };
        toast::show(
            Toast::error("Could not add backend")
                .description(message.clone())
                .stacked(),
            cx,
        );
        self.error = Some(message.into());
        cx.notify();
    }
}

impl Render for PairingForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let field = |label: &'static str, input: &Entity<InputState>, adding: bool| {
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(
                    div()
                        .mb(px(6.))
                        .type_scale(text::XS)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child(label),
                )
                .child(Input::new(input).disabled(adding))
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(div().flex_1().min_w_0().child(field(
                                "Host",
                                &self.host,
                                self.adding,
                            )))
                            .child(div().w(px(160.)).flex_none().child(field(
                                "Pairing code",
                                &self.code,
                                self.adding,
                            ))),
                    )
                    .child(
                        div()
                            .mt_1()
                            .type_scale(under_xs(11.))
                            .text_color(colors.muted_foreground)
                            .child(
                                "Paste a full pairing URL here to fill both fields automatically.",
                            ),
                    ),
            )
            .children(self.error.clone().map(|error| {
                div()
                    .type_scale(text::XS)
                    .text_color(colors.destructive)
                    .child(error)
            }))
            .child(
                Button::new("add-environment-submit")
                    .variant(ButtonVariant::Outline)
                    .icon(IconName::Plus)
                    .label(if self.adding {
                        "Adding…"
                    } else {
                        "Add environment"
                    })
                    .disabled(self.adding)
                    .w_full()
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            )
    }
}

/// Settings > Connections "Add Environment" dialog (`ConnectionsSettings.tsx:3294-3352`):
/// a 768px dialog with the "Remote link" mode card and the [`PairingForm`]. The SSH mode is not
/// offered (no native SSH tunnels yet), as in the fork's non-desktop build.
pub struct AddEnvironmentDialog {
    open: bool,
    form: Entity<PairingForm>,
    _form_events: Subscription,
}

impl AddEnvironmentDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let form = cx.new(|cx| PairingForm::new(window, cx));
        let events = cx.subscribe(&form, |this, _, event, cx| match event {
            PairingFormEvent::Added(_) => this.set_open(false, cx),
        });
        Self {
            open: false,
            form,
            _form_events: events,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens the dialog with the Host field focused.
    pub fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = true;
        self.form.update(cx, |form, cx| form.focus(window, cx));
        cx.notify();
    }

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.open != open {
            self.open = open;
            if !open {
                self.form.update(cx, |form, cx| {
                    form.error = None;
                    cx.notify();
                });
            }
            cx.notify();
        }
    }
}

/// A connection mode card (`renderConnectionModeCard`), always selected here.
fn mode_card(cx: &App) -> impl IntoElement {
    let colors = cx.colors();
    div()
        .min_h_24()
        .flex()
        .items_start()
        .gap_3()
        .rounded(px(10.))
        .border_1()
        .border_color(colors.primary.opacity(0.5))
        .bg(colors.primary.opacity(0.05))
        .p_4()
        .child(
            div()
                .mt(px(2.))
                .size_8()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .border_1()
                .border_color(colors.primary.opacity(0.3))
                .bg(colors.primary.opacity(0.1))
                .text_color(colors.primary)
                .child(Icon::new(IconName::ChevronsLeftRightEllipsis).size(px(16.))),
        )
        .child(
            div()
                .min_w_0()
                .child(
                    div()
                        .type_scale(text::SM)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(colors.foreground)
                        .child("Remote link"),
                )
                .child(
                    div()
                        .mt_1()
                        .text_size(px(12.))
                        .line_height(relative(1.625))
                        .text_color(colors.muted_foreground)
                        .child("Enter a backend host and pairing code."),
                ),
        )
}

impl Render for AddEnvironmentDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let max_height = window.viewport_size().height * 0.8;
        let entity = cx.entity();
        let popup = DialogPopup::new()
            .max_w(px(768.))
            .max_h(max_height)
            .close_button({
                let entity = entity.clone();
                move |_, _, cx| entity.update(cx, |this, cx| this.set_open(false, cx))
            })
            .child(
                DialogHeader::new()
                    .before_panel()
                    .child(DialogTitle::new("Add Environment"))
                    .child(DialogDescription::new(
                        "Pair another environment to this client.",
                    )),
            )
            .child(
                DialogPanel::new().after_header().child(
                    div()
                        .id("add-environment-body")
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .gap_3()
                                .child(div().flex_1().child(mode_card(cx)))
                                .child(div().flex_1()),
                        )
                        .child(self.form.clone()),
                ),
            );
        base::Dialog::new(cx)
            .open(true)
            .close_on_backdrop_press(true)
            .on_open_change(move |open, _, _, cx| {
                entity.update(cx, |this, cx| this.set_open(open, cx))
            })
            .p(px(16.))
            .backdrop(dialog_backdrop(cx).with_animation(
                "add-environment-backdrop",
                Animation::new(motion::DIALOG).with_easing(motion::ease_standard),
                |this, t| this.opacity(t),
            ))
            .popup(base::DialogPopup::new().w_full().max_w(px(768.)).child(
                div().w_full().child(popup).with_animation(
                    ElementId::from("add-environment-popup"),
                    Animation::new(motion::DIALOG).with_easing(motion::ease_standard),
                    |this, t| this.opacity(t),
                ),
            ))
            .into_any_element()
    }
}

/// `Route::Pair`: the pairing form as a page, for adding an environment outside settings
/// (first launch, deep links). Navigates to the index once paired.
pub struct PairView {
    form: Entity<PairingForm>,
    _subscriptions: Vec<Subscription>,
}

impl PairView {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let form = cx.new(|cx| PairingForm::new(window, cx));
        form.update(cx, |form, cx| form.focus(window, cx));
        let subscriptions = vec![
            cx.observe(&app_state, |_, _, cx| cx.notify()),
            cx.subscribe(&form, move |_, _, event, cx| match event {
                PairingFormEvent::Added(_) => AppState::global(cx)
                    .update(cx, |state, cx| state.replace_route(Route::Index, cx)),
            }),
        ];
        Self {
            form,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for PairView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let inset = collapsed_titlebar_inset(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                drag_region("pair-header", window, cx)
                    .h(layout::TOPBAR_HEIGHT)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .px_5()
                    .when_some(inset, |this, inset| this.pl(inset))
                    .type_scale(text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground_70)
                    .child("Add Environment"),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .p_8()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(576.))
                            .rounded(px(18.))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.card)
                            .p_6()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(DialogTitle::new("Pair with an environment"))
                                    .child(DialogDescription::new(
                                        "Paste the pairing URL that t3 serve or t3 pair prints, or enter its host and pairing code.",
                                    )),
                            )
                            .child(self.form.clone()),
                    ),
            )
    }
}
