//! Settings (`web/routes/settings.tsx`, `web/components/settings/*`, spec 3).
//!
//! [`SettingsView`] is the main-column view for every `Route::Settings(page)`: the 52px header
//! strip ("Settings", plus "Restore defaults" on General) above the active page. Pages are
//! separate entities from the [`pages`] registry, built the first time they show and kept while
//! settings stays open, so switching pages keeps their local state (open provider cards, draft
//! inputs). The sidebar swaps its content for [`SettingsNav`] on these routes.
//!
//! Escape anywhere in settings goes back (`settings.tsx:50-63`).

pub mod archived;
mod connections;
mod general;
mod keybindings;
pub mod layout;
mod nav;
pub mod pages;
mod providers;
pub mod server;
mod source_control;

use std::collections::HashMap;

use gpui_kit::{
    AnyView, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _,
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, IconName,
    tokens::{layout as chrome_layout, text},
};

pub use nav::SettingsNav;

use crate::{
    chrome::{TypeScale as _, drag_region},
    state::{AppState, Route, SettingsPage},
    workspace::collapsed_titlebar_inset,
};

/// The settings route's view.
pub struct SettingsView {
    app_state: Entity<AppState>,
    pages: HashMap<SettingsPage, AnyView>,
    focus: FocusHandle,
    _app_state: Subscription,
}

impl SettingsView {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
            app_state,
            pages: HashMap::new(),
            focus,
        }
    }

    fn page(&self, cx: &Context<Self>) -> SettingsPage {
        match self.app_state.read(cx).route() {
            Route::Settings(page) => *page,
            _ => SettingsPage::General,
        }
    }

    fn page_view(
        &mut self,
        page: SettingsPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let app_state = self.app_state.clone();
        self.pages
            .entry(page)
            .or_insert_with(|| pages::build(page, app_state, window, cx))
            .clone()
    }

    /// Expands a provider card on the Providers page (snapshot scenes).
    pub fn expand_provider(
        &mut self,
        instance_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(page) = self
            .page_view(SettingsPage::Providers, window, cx)
            .downcast::<providers::ProvidersPage>()
        {
            page.update(cx, |page, cx| {
                page.set_expanded(instance_id, true, window, cx)
            });
        }
    }

    /// Opens the Add Environment dialog on the Connections page.
    pub fn open_add_environment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(page) = self
            .page_view(SettingsPage::Connections, window, cx)
            .downcast::<connections::ConnectionsPage>()
        {
            page.update(cx, |page, cx| page.open_add_dialog(window, cx));
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            self.app_state.update(cx, |state, cx| state.go_back(cx));
        }
    }

    fn restore_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let labels = general::changed_labels(cx);
        if labels.is_empty() {
            return;
        }
        let message = format!(
            "Restore default settings?\nThis will reset: {}.",
            labels.join(", ")
        );
        let confirmed = crate::dialogs::confirm(&message, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            if !confirmed.await {
                return;
            }
            this.update(cx, |this, cx| {
                general::restore_defaults(cx);
                // The web remounts the page so draft inputs show the restored values.
                this.pages.remove(&SettingsPage::General);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let page = self.page(cx);
        let body = self.page_view(page, window, cx);
        let inset = collapsed_titlebar_inset(cx);
        let restore = (page == SettingsPage::General).then(|| {
            let changed = !general::changed_labels(cx).is_empty();
            div().ml_auto().flex().items_center().gap_2().child(
                Button::new("settings-restore-defaults")
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Xs)
                    .icon(IconName::RotateCcw)
                    .label("Restore defaults")
                    .disabled(!changed)
                    .on_click(cx.listener(|this, _, window, cx| this.restore_defaults(window, cx))),
            )
        });
        div()
            .id("settings")
            .key_context("Settings")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_color(colors.foreground)
            .child(
                drag_region("settings-header", window, cx)
                    .h(chrome_layout::TOPBAR_HEIGHT)
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .px_5()
                    .when_some(inset, |this, inset| this.pl(inset))
                    .child(
                        div()
                            .type_scale(text::XS)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.muted_foreground_70)
                            .child("Settings"),
                    )
                    .children(restore),
            )
            .child(div().flex_1().min_h_0().flex().flex_col().child(body))
    }
}
