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

/// Boots the application: bundled assets, gpui-kit and t3-ui init, and the main window
/// with its native glass. Owned by the design system; views are built by `Workspace::new`.
pub fn run() {
    gpui_kit::application()
        .with_assets(t3_ui::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            // TODO(settings): load the persisted theme preference instead of `System`.
            t3_ui::init(t3_ui::ThemeMode::System, cx);
            t3_ui::theme::enable_native_appearance(cx);
            let options = t3_ui::window::main_window_options(cx);
            gpui_kit::open_window(options, cx, |window, cx| {
                t3_ui::window::install_glass(window);
                t3_ui::theme::observe_system_appearance(window, cx).detach();
                cx.new(|cx| Workspace::new(window, cx))
            })
            .expect("failed to open main window");
            cx.activate(true);
        });
}
