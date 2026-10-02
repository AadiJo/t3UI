//! Design-system gallery: the token sheet and every t3-ui primitive in every state, each page
//! captured dark and light. States that need a pointer or keyboard are forced with the
//! primitives' `preview(Interaction::..)`; overlays are shown as their static surfaces.

mod primitives;
mod tokens;

use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Window,
    component::input::{InputState, TextareaState},
    div,
    prelude::FluentBuilder as _,
    px,
};
use t3_ui::{ActiveColors as _, Colors, ThemeMode};

use super::Scene;

pub fn scenes() -> Vec<Scene> {
    let mut scenes = Vec::new();
    for (page, name_dark, name_light, height) in [
        (
            Page::Tokens,
            "gallery-tokens-dark",
            "gallery-tokens-light",
            1500.,
        ),
        (
            Page::Buttons,
            "gallery-buttons-dark",
            "gallery-buttons-light",
            1100.,
        ),
        (
            Page::Controls,
            "gallery-controls-dark",
            "gallery-controls-light",
            1200.,
        ),
        (
            Page::Overlays,
            "gallery-overlays-dark",
            "gallery-overlays-light",
            1300.,
        ),
    ] {
        for (name, theme) in [(name_dark, ThemeMode::Dark), (name_light, ThemeMode::Light)] {
            let build = match page {
                Page::Tokens => |window: &mut Window, cx: &mut gpui_kit::App| {
                    cx.new(|cx| Gallery::new(Page::Tokens, window, cx)).into()
                },
                Page::Buttons => |window: &mut Window, cx: &mut gpui_kit::App| {
                    cx.new(|cx| Gallery::new(Page::Buttons, window, cx)).into()
                },
                Page::Controls => |window: &mut Window, cx: &mut gpui_kit::App| {
                    cx.new(|cx| Gallery::new(Page::Controls, window, cx)).into()
                },
                Page::Overlays => |window: &mut Window, cx: &mut gpui_kit::App| {
                    cx.new(|cx| Gallery::new(Page::Overlays, window, cx)).into()
                },
            };
            scenes.push(Scene::new(name, theme, build).size(1440., height));
        }
    }
    scenes
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Tokens,
    Buttons,
    Controls,
    Overlays,
}

/// Input states the controls page renders.
struct Fields {
    empty: Entity<InputState>,
    filled: Entity<InputState>,
    notes: Entity<TextareaState>,
}

struct Gallery {
    page: Page,
    fields: Fields,
}

impl Gallery {
    fn new(page: Page, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let fields = Fields {
            empty: cx.new(|cx| InputState::new(window, cx).placeholder("Search threads")),
            filled: cx.new(|cx| InputState::new(window, cx).default_value("feat/design-system")),
            notes: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("Describe the change")
                    .default_value("Port the coss primitives to GPUI.\nMatch every state.")
            }),
        };
        Self { page, fields }
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .text_size(px(14.))
            .line_height(px(20.))
            .p(px(32.))
            .flex()
            .flex_col()
            .gap(px(28.))
            .map(|this| match self.page {
                Page::Tokens => this.children(tokens::tokens_page(colors, cx)),
                Page::Buttons => this.children(primitives::buttons_page(colors)),
                Page::Controls => this.children(primitives::controls_page(colors, &self.fields)),
                Page::Overlays => this.children(primitives::overlays_page(colors)),
            })
    }
}

/// An uppercase caption above a group of samples.
fn section(title: &str, colors: &Colors, body: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(SharedString::from(title.to_uppercase())),
        )
        .child(body)
        .into_any_element()
}

/// A labeled row of samples.
fn row(label: &str, colors: &Colors, items: impl IntoIterator<Item = AnyElement>) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(16.))
        .child(
            div()
                .w(px(140.))
                .flex_none()
                .text_size(px(11.))
                .text_color(colors.muted_foreground)
                .child(label.to_string()),
        )
        .children(items)
        .into_any_element()
}
