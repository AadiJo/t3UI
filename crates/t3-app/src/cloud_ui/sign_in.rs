//! "Sign in to T3 Connect": email, then the emailed 6-digit code; or a social provider through
//! the system browser sheet (macOS). The web app uses Clerk's hosted modal; this is the same
//! flow drawn with the coss dialog primitives.

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window,
    component::input::{InputEvent, InputState},
    div,
};
use t3_client::cloud::{EmailSignIn, OAuthProvider};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Dialog, DialogDescription, DialogFooter,
    DialogHeader, DialogPanel, DialogTitle, Input, Separator, tokens::text,
};

use super::CloudAccount;
use crate::chrome::TypeScale as _;

/// A dialog page with its inputs, for opening the dialog in a given state (snapshot scenes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInStep {
    Email {
        email: String,
        error: Option<String>,
        /// Shows "Sending code…".
        busy: bool,
    },
    Code {
        masked_email: String,
        code: String,
        error: Option<String>,
        busy: bool,
    },
}

enum Page {
    Email,
    Code {
        /// `None` only in previews.
        pending: Option<EmailSignIn>,
        masked_email: SharedString,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Busy {
    Email,
    Provider(OAuthProvider),
    Code,
    Resend,
}

/// The sign-in dialog. Owned by [`CloudAccount`] while open.
pub struct SignInDialog {
    account: Entity<CloudAccount>,
    email: Entity<InputState>,
    code: Entity<InputState>,
    page: Page,
    busy: Option<Busy>,
    error: Option<SharedString>,
    /// Confirmation under the code field ("Sent a new code.").
    notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl SignInDialog {
    pub fn new(account: Entity<CloudAccount>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("you@example.com"));
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("6-digit code"));
        email.update(cx, |input, cx| input.focus(window, cx));
        let on_enter = |submit: fn(&mut Self, &mut Window, &mut Context<Self>)| {
            move |this: &mut Self,
                  _: &Entity<InputState>,
                  event: &InputEvent,
                  window: &mut Window,
                  cx: &mut Context<Self>| {
                if let InputEvent::PressEnter { .. } = event {
                    submit(this, window, cx);
                }
            }
        };
        let subscriptions = vec![
            cx.subscribe_in(&email, window, on_enter(Self::submit_email)),
            cx.subscribe_in(&code, window, on_enter(Self::submit_code)),
        ];
        SignInDialog {
            account,
            email,
            code,
            page: Page::Email,
            busy: None,
            error: None,
            notice: None,
            _subscriptions: subscriptions,
        }
    }

    /// Shows `step` without running anything (snapshot scenes).
    pub fn preview(&mut self, step: SignInStep, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            SignInStep::Email { email, error, busy } => {
                self.email
                    .update(cx, |input, cx| input.set_value(email, window, cx));
                self.page = Page::Email;
                self.error = error.map(Into::into);
                self.busy = busy.then_some(Busy::Email);
            }
            SignInStep::Code {
                masked_email,
                code,
                error,
                busy,
            } => {
                self.code
                    .update(cx, |input, cx| input.set_value(code, window, cx));
                self.page = Page::Code {
                    pending: None,
                    masked_email: masked_email.into(),
                };
                self.error = error.map(Into::into);
                self.busy = busy.then_some(Busy::Code);
            }
        }
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.account
            .update(cx, |account, cx| account.close_sign_in(cx));
    }

    fn fail(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.busy = None;
        self.notice = None;
        self.error = Some(message.into());
        cx.notify();
    }

    /// "Continue": creates the sign-in and has Clerk email a code.
    fn submit_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_some() || !matches!(self.page, Page::Email) {
            return;
        }
        let email = self.email.read(cx).value().trim().to_owned();
        if email.is_empty() {
            self.fail("Enter your email address.", cx);
            return;
        }
        let Some(connect) = self.account.read(cx).connect().cloned() else {
            return;
        };
        self.busy = Some(Busy::Email);
        self.error = None;
        cx.notify();
        let code = self.code.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = connect.start_email_sign_in(&email).await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(pending) => {
                    this.busy = None;
                    this.error = None;
                    this.page = Page::Code {
                        masked_email: pending.masked_email().to_owned().into(),
                        pending: Some(pending),
                    };
                    code.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                        input.focus(window, cx);
                    });
                    cx.notify();
                }
                Err(error) => this.fail(error.message, cx),
            })
            .ok();
        })
        .detach();
    }

    /// "Verify": submits the code; success closes the dialog.
    fn submit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Page::Code {
            pending: Some(pending),
            ..
        } = &self.page
        else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let code = self.code.read(cx).value().to_string();
        let pending = pending.clone();
        self.busy = Some(Busy::Code);
        self.error = None;
        self.notice = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = pending.verify(&code).await;
            this.update(cx, |this, cx| match result {
                Ok(_) => this.close(cx),
                Err(error) => this.fail(error.message, cx),
            })
            .ok();
        })
        .detach();
    }

    fn resend(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Page::Code {
            pending: Some(pending),
            ..
        } = &self.page
        else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        let pending = pending.clone();
        self.busy = Some(Busy::Resend);
        self.error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = pending.resend().await;
            this.update(cx, |this, cx| match result {
                Ok(()) => {
                    this.busy = None;
                    this.notice = Some("Sent a new code.".into());
                    cx.notify();
                }
                Err(error) => this.fail(error.message, cx),
            })
            .ok();
        })
        .detach();
    }

    fn use_different_email(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = Page::Email;
        self.busy = None;
        self.error = None;
        self.notice = None;
        self.email.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// "Continue with <provider>": the browser sheet, then back here signed in.
    fn sign_in_with(&mut self, provider: OAuthProvider, cx: &mut Context<Self>) {
        if self.busy.is_some() {
            return;
        }
        let account = self.account.read(cx);
        let (Some(connect), Some(authenticator)) =
            (account.connect().cloned(), account.authenticator())
        else {
            return;
        };
        self.busy = Some(Busy::Provider(provider));
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = connect.sign_in_with(provider, authenticator).await;
            this.update(cx, |this, cx| match result {
                Ok(_) => this.close(cx),
                Err(error) if error.cancelled => {
                    this.busy = None;
                    cx.notify();
                }
                Err(error) => this.fail(error.message, cx),
            })
            .ok();
        })
        .detach();
    }

    fn label(text: &'static str, cx: &App) -> impl IntoElement {
        div()
            .type_scale(text::XS)
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.colors().foreground)
            .child(text)
    }

    /// The error (destructive) or notice (muted) line under a field.
    fn message(&self, cx: &App) -> Option<AnyElement> {
        let colors = cx.colors();
        if let Some(error) = &self.error {
            return Some(
                div()
                    .type_scale(text::XS)
                    .text_color(colors.destructive_foreground)
                    .child(error.clone())
                    .into_any_element(),
            );
        }
        self.notice.as_ref().map(|notice| {
            div()
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(notice.clone())
                .into_any_element()
        })
    }

    fn providers(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.account.read(cx).providers_available() {
            return None;
        }
        let colors = cx.colors();
        let busy = self.busy;
        let button = |provider: OAuthProvider, cx: &mut Context<Self>| {
            let label = if busy == Some(Busy::Provider(provider)) {
                "Waiting for browser…".to_owned()
            } else {
                provider.label().to_owned()
            };
            Button::new(SharedString::from(format!(
                "sign-in-{}",
                provider.strategy()
            )))
            .variant(ButtonVariant::Outline)
            .label(label)
            .disabled(busy.is_some())
            .flex_1()
            .on_click(cx.listener(move |this, _, _, cx| this.sign_in_with(provider, cx)))
        };
        let [first, second, third, fourth] = OAuthProvider::ALL;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(button(first, cx))
                        .child(button(second, cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(button(third, cx))
                        .child(button(fourth, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .py_1()
                        .child(div().flex_1().child(Separator::horizontal()))
                        .child(
                            div()
                                .type_scale(text::XS)
                                .text_color(colors.muted_foreground)
                                .child("or"),
                        )
                        .child(div().flex_1().child(Separator::horizontal())),
                )
                .into_any_element(),
        )
    }

    fn email_page(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = cx.colors();
        let sending = self.busy == Some(Busy::Email);
        let sign_up = self
            .account
            .read(cx)
            .connect()
            .map(|c| c.config().sign_up_url().to_string())
            .unwrap_or_else(|| "https://accounts.t3.codes/sign-up".into());
        vec![
            DialogHeader::new()
                .child(DialogTitle::new("Sign in to T3 Connect"))
                .child(DialogDescription::new(
                    "Use your T3 account to reach the environments you linked with T3 Connect.",
                ))
                .into_any_element(),
            DialogPanel::new()
                .after_header()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(self.providers(cx))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(Self::label("Email address", cx))
                                .child(
                                    Input::new(&self.email)
                                        .disabled(self.busy.is_some())
                                        .invalid(self.error.is_some()),
                                )
                                .children(self.message(cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .type_scale(text::XS)
                                .text_color(colors.muted_foreground)
                                .child("No T3 account yet?")
                                .child(
                                    Button::new("sign-in-create-account")
                                        .variant(ButtonVariant::Link)
                                        .size(ButtonSize::Xs)
                                        .label("Create one at accounts.t3.codes")
                                        .on_click(move |_, _, cx| cx.open_url(&sign_up)),
                                ),
                        ),
                )
                .into_any_element(),
            DialogFooter::new()
                .child(
                    Button::new("sign-in-cancel")
                        .variant(ButtonVariant::Outline)
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                )
                .child(
                    Button::new("sign-in-continue")
                        .label(if sending {
                            "Sending code…"
                        } else {
                            "Continue"
                        })
                        .disabled(self.busy.is_some())
                        .on_click(cx.listener(|this, _, window, cx| this.submit_email(window, cx))),
                )
                .into_any_element(),
        ]
    }

    fn code_page(&self, masked_email: &SharedString, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let verifying = self.busy == Some(Busy::Code);
        let resending = self.busy == Some(Busy::Resend);
        vec![
            DialogHeader::new()
                .child(DialogTitle::new("Check your email"))
                .child(DialogDescription::new(format!(
                    "Enter the 6-digit code we sent to {masked_email}."
                )))
                .into_any_element(),
            DialogPanel::new()
                .after_header()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Self::label("Verification code", cx))
                        .child(
                            Input::new(&self.code)
                                .disabled(self.busy.is_some())
                                .invalid(self.error.is_some()),
                        )
                        .children(self.message(cx))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    Button::new("sign-in-different-email")
                                        .variant(ButtonVariant::Link)
                                        .size(ButtonSize::Xs)
                                        .label("Use a different email")
                                        .disabled(self.busy.is_some())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.use_different_email(window, cx)
                                        })),
                                )
                                .child(
                                    Button::new("sign-in-resend")
                                        .variant(ButtonVariant::Link)
                                        .size(ButtonSize::Xs)
                                        .label(if resending {
                                            "Sending…"
                                        } else {
                                            "Resend code"
                                        })
                                        .disabled(self.busy.is_some())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.resend(window, cx)
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
            DialogFooter::new()
                .child(
                    Button::new("sign-in-cancel")
                        .variant(ButtonVariant::Outline)
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                )
                .child(
                    Button::new("sign-in-verify")
                        .label(if verifying { "Verifying…" } else { "Verify" })
                        .disabled(self.busy.is_some())
                        .on_click(cx.listener(|this, _, window, cx| this.submit_code(window, cx))),
                )
                .into_any_element(),
        ]
    }
}

impl Render for SignInDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let children = match &self.page {
            Page::Email => self.email_page(cx),
            Page::Code { masked_email, .. } => {
                let masked_email = masked_email.clone();
                self.code_page(&masked_email, cx)
            }
        };
        let on_open_change = cx.listener(|this, open: &bool, _, cx| {
            if !open {
                this.close(cx);
            }
        });
        Dialog::new("t3-connect-sign-in")
            .open(true)
            .on_open_change(move |open, window, cx| on_open_change(&open, window, cx))
            .children(children)
    }
}
