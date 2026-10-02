//! Message text as `t3_markdown::Markdown` views (the fork's `ChatMarkdown`), one per message
//! or plan, kept across renders so streaming deltas only re-parse the live tail.

use std::collections::HashMap;

use gpui_kit::{App, AppContext as _, Entity};
use t3_markdown::{Markdown, MarkdownOptions};

/// Markdown views by key (message id, or `plan:{id}` / `plan-preview:{id}`).
#[derive(Default)]
pub(super) struct MarkdownCache {
    views: HashMap<String, Entity<Markdown>>,
}

/// How a piece of text renders.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TextKind {
    /// Assistant replies and plan bodies: `foreground/80`, soft line breaks.
    Assistant,
    /// User bubbles: full `foreground`, single newlines break lines.
    User,
}

impl MarkdownCache {
    /// The view for `key`, created on first use and updated to `text`.
    pub fn view(
        &mut self,
        key: &str,
        text: &str,
        streaming: bool,
        kind: TextKind,
        cwd: Option<&str>,
        cx: &mut App,
    ) -> Entity<Markdown> {
        if let Some(view) = self.views.get(key) {
            if view.read(cx).text().as_ref() != text || view.read(cx).is_streaming() != streaming {
                let text = text.to_owned();
                view.update(cx, |view, cx| view.set_text(text, streaming, cx));
            }
            return view.clone();
        }
        let options = MarkdownOptions {
            cwd: cwd.map(str::to_owned),
            line_breaks: kind == TextKind::User,
            full_foreground: kind == TextKind::User,
            ..MarkdownOptions::default()
        };
        let text = text.to_owned();
        let view = cx.new(|cx| {
            let mut view = Markdown::new(text, options, cx);
            view.set_streaming(streaming, cx);
            view
        });
        self.views.insert(key.to_owned(), view.clone());
        view
    }

    /// Drops the views whose rows left the timeline.
    pub fn retain(&mut self, alive: impl Fn(&str) -> bool) {
        self.views.retain(|key, _| alive(key));
    }
}
