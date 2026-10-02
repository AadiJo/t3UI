//! Inline chips: file mentions, skills, and terminal contexts (`composerInlineChip.ts`,
//! `ComposerPromptEditor.tsx` decorator nodes, `TerminalContextInlineChip.tsx`).
//!
//! In the editor every chip is a gpui-base `InlineToken` whose `text` is the chip's prompt
//! source (`[name](path)`, `$skill`, U+FFFC) and whose `id` says what it is (see
//! [`ChipKind::token_id`]). [`render_chip`] is the renderer the editor installs.

use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, Window,
    base::input::{InlineToken, InlineTokenContext},
    div,
    prelude::FluentBuilder as _,
    px, svg,
};
use t3_ui::{ActiveColors as _, Icon, IconName, TooltipExt as _, file_icon};

use super::style::{alpha, palette};

/// What a chip refers to, recovered from its token id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChipKind {
    File {
        path: String,
    },
    Skill {
        name: String,
    },
    /// `expired` when the context's text is gone (restored from disk).
    Terminal {
        context_id: String,
        expired: bool,
    },
}

impl ChipKind {
    /// `file:<path>`, `skill:<name>`, `terminal:<id>` or `terminal-expired:<id>`.
    pub fn token_id(&self) -> String {
        match self {
            Self::File { path } => format!("file:{path}"),
            Self::Skill { name } => format!("skill:{name}"),
            Self::Terminal {
                context_id,
                expired: false,
            } => format!("terminal:{context_id}"),
            Self::Terminal {
                context_id,
                expired: true,
            } => format!("terminal-expired:{context_id}"),
        }
    }

    pub fn from_token_id(id: &str) -> Option<Self> {
        let (kind, value) = id.split_once(':')?;
        let value = value.to_owned();
        Some(match kind {
            "file" => Self::File { path: value },
            "skill" => Self::Skill { name: value },
            "terminal" => Self::Terminal {
                context_id: value,
                expired: false,
            },
            "terminal-expired" => Self::Terminal {
                context_id: value,
                expired: true,
            },
            _ => return None,
        })
    }

    /// The editor token for this chip. `source` is its prompt text, `label` what it shows.
    pub fn token(&self, source: &str, label: &str) -> InlineToken {
        InlineToken::new(self.token_id(), source.to_owned()).with_label(label.to_owned())
    }
}

/// Renders one chip inside the editor (installed with `Textarea::token`).
pub fn render_chip(context: &InlineTokenContext, _: &mut Window, cx: &mut App) -> AnyElement {
    let token = context.token();
    let Some(kind) = ChipKind::from_token_id(token.id()) else {
        return div().child(token.label().clone()).into_any_element();
    };
    let tooltip = match &kind {
        ChipKind::File { path } => path.clone(),
        ChipKind::Skill { .. } => token.label().to_string(),
        ChipKind::Terminal { expired: true, .. } => format!(
            "Terminal context expired. Remove and re-add {} to include it in your message.",
            token.label()
        ),
        ChipKind::Terminal { .. } => token.label().to_string(),
    };
    chip(&kind, token.label().to_string(), context.is_selected(), cx)
        .id(("chip", context.range().start))
        .tooltip_text(tooltip)
        .into_any_element()
}

/// The chip box: `inline-flex gap-1 rounded-md border border/70 bg-accent/40 px-1.5 py-px`, 12px
/// medium label truncated, 14px icon at 85% opacity. Skills are fuchsia, expired terminal
/// contexts destructive.
pub fn chip(kind: &ChipKind, label: String, selected: bool, cx: &App) -> gpui_kit::Div {
    let colors = cx.colors();
    let dark = colors.is_dark;
    let (border, bg, fg) = match kind {
        ChipKind::Skill { .. } => (
            alpha(palette::FUCHSIA_500, 0.25),
            alpha(palette::FUCHSIA_500, 0.12),
            if dark {
                palette::FUCHSIA_300
            } else {
                palette::FUCHSIA_700
            },
        ),
        ChipKind::Terminal { expired: true, .. } => (
            alpha(colors.destructive, 0.35),
            alpha(colors.destructive, 0.08),
            colors.destructive,
        ),
        _ => (
            alpha(colors.border, 0.7),
            alpha(colors.accent, 0.4),
            colors.foreground,
        ),
    };
    let icon: AnyElement = match kind {
        ChipKind::File { path } => file_icon(path, dark).render(px(14.)),
        ChipKind::Skill { .. } => svg()
            .path("icons/composer/cube.svg")
            .flex_none()
            .size(px(14.))
            .text_color(fg)
            .into_any_element(),
        ChipKind::Terminal { .. } => Icon::new(IconName::Terminal)
            .size(px(14.))
            .color(fg)
            .into_any_element(),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .max_w_full()
        .gap(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .when(selected, |this| this.bg(colors.accent))
        .px(px(6.))
        .py(px(1.))
        .text_size(px(12.))
        .line_height(px(13.2))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .child(div().flex_none().opacity(0.85).child(icon))
        .child(div().min_w_0().truncate().child(label))
}
