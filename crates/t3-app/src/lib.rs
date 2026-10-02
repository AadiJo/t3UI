//! The T3 Code desktop app: window shell and views. `main.rs` only boots it, so the
//! snapshot renderer can mount the same views headlessly.

use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _, Window, div,
    rgb,
};

/// Root view of the main window.
pub struct Workspace;

impl Workspace {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(rgb(0x0a0a0a))
            .text_color(rgb(0xfafafa))
            .child(div().w_64().h_full().bg(rgb(0x171717)))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("T3 Code"),
            )
    }
}

/// Boots the application: assets, gpui-kit init, and the main window.
pub fn run() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            let options = gpui_kit::WindowOptions {
                titlebar: Some(gpui_kit::TitlebarOptions {
                    title: Some("T3 Code".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui_kit::point(
                        gpui_kit::px(16.),
                        gpui_kit::px(18.),
                    )),
                }),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| Workspace::new(window, cx))
            })
            .expect("failed to open main window");
            cx.activate(true);
        });
}
