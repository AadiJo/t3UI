//! Chat markdown: the sample message from `t3-markdown/fixtures/sample.md`, which covers every
//! element, framed like an assistant row in the chat column (768px column, 4px row padding,
//! 16px from the top). Compare with the fork rendered from the same file.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Window, base::TextSelectionLayer, div, px,
};
use t3_markdown::{Markdown, MarkdownOptions};
use t3_ui::{ActiveColors as _, ThemeMode};

use super::Scene;

const SAMPLE: &str = include_str!("../../../t3-markdown/fixtures/sample.md");

pub fn scenes() -> Vec<Scene> {
    vec![
        Scene::new("markdown-dark", ThemeMode::Dark, |window, cx| {
            complete(window, cx)
        }),
        Scene::new("markdown-light", ThemeMode::Light, |window, cx| {
            complete(window, cx)
        }),
        Scene::new("markdown-full-dark", ThemeMode::Dark, |window, cx| {
            complete(window, cx)
        })
        .size(1440., 2100.),
        Scene::new("markdown-full-light", ThemeMode::Light, |window, cx| {
            complete(window, cx)
        })
        .size(1440., 2100.),
        Scene::new("markdown-streaming-dark", ThemeMode::Dark, |window, cx| {
            streaming(window, cx)
        }),
    ]
}

fn options() -> MarkdownOptions {
    MarkdownOptions {
        cwd: Some("/Users/user/t3code".into()),
        ..MarkdownOptions::default()
    }
}

fn complete(_: &mut Window, cx: &mut App) -> AnyView {
    let markdown = cx.new(|cx| Markdown::new(SAMPLE, options(), cx));
    cx.new(|_| MessageFrame { markdown }).into()
}

/// The same message cut off inside the Rust code block, still streaming: the frozen prefix plus a
/// live tail with an unterminated fence.
fn streaming(_: &mut Window, cx: &mut App) -> AnyView {
    let cut = SAMPLE.find("println!").unwrap_or(SAMPLE.len());
    let markdown = cx.new(|cx| {
        let mut markdown = Markdown::new("", options(), cx);
        // Grow it the way deltas arrive, so the streaming paths run.
        for end in (0..cut).step_by(97).chain(std::iter::once(cut)) {
            let end = (0..=end)
                .rev()
                .find(|end| SAMPLE.is_char_boundary(*end))
                .unwrap_or(0);
            markdown.set_text(SAMPLE[..end].to_string(), true, cx);
        }
        markdown
    });
    cx.new(|_| MessageFrame { markdown }).into()
}

/// One assistant row in the chat column.
struct MessageFrame {
    markdown: Entity<Markdown>,
}

impl Render for MessageFrame {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        div()
            .size_full()
            .bg(colors.background)
            .child(TextSelectionLayer)
            .child(
                div()
                    .mx_auto()
                    .w(px(768.))
                    .pt(px(16.))
                    .child(div().px(px(4.)).py(px(2.)).child(self.markdown.clone())),
            )
    }
}
