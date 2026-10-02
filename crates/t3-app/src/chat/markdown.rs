//! The one place message text becomes elements. Until `t3-markdown` lands this draws plain
//! paragraphs with `ChatMarkdown`'s root type (14px, `leading-relaxed`, 10.4px block gaps);
//! swap these two functions for the markdown renderer.

use gpui_kit::{AnyElement, Hsla, IntoElement, ParentElement as _, Styled as _, div, px};
use t3_ui::Colors;

/// Assistant replies and plan bodies (`ChatMarkdown`, root color `foreground/80`). Single
/// newlines are soft breaks.
pub(super) fn assistant_text(text: &str, _streaming: bool, colors: &Colors) -> AnyElement {
    paragraphs(text, colors.foreground.opacity(0.8), false)
}

/// User bubbles (`ChatMarkdown lineBreaks`, color `foreground`): single newlines break lines.
pub(super) fn user_text(text: &str, colors: &Colors) -> AnyElement {
    paragraphs(text, colors.foreground, true)
}

fn paragraphs(text: &str, color: Hsla, line_breaks: bool) -> AnyElement {
    let blocks = text
        .split("\n\n")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .map(|block| {
            let block = if line_breaks {
                block.to_owned()
            } else {
                block.split('\n').collect::<Vec<_>>().join(" ")
            };
            div().child(block)
        });
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(10.4))
        .text_size(px(14.))
        .line_height(px(22.75))
        .text_color(color)
        .children(blocks)
        .into_any_element()
}
