//! Placeholder while the page is built.

use gpui_kit::{Context, Entity, IntoElement, Render, Subscription, Window, div};

use crate::state::AppState;

pub struct SourceControlPage {
    _app_state: Subscription,
}

impl SourceControlPage {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for SourceControlPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
