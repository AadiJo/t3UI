//! The prompt editor: a gpui-base multi-line text input whose value is the prompt string, with
//! chips as atomic inline tokens. Decision record: the web composer keeps a plain string and
//! rebuilds a Lexical tree from it; here the input's own text *is* that string, because a token's
//! `text` is its prompt source. So `value()` needs no serialization, trigger offsets are the
//! input's byte offsets, and caret skipping, whole-chip deletion and undo come from gpui-base.
//!
//! What the composer adds on top:
//! - [`sync_chips`] after every edit turns completed tokens (followed by whitespace) into chips,
//!   which is what typing `$skill ` or pasting `[a](a) ` does on the web.
//! - [`content_for_prompt`] rebuilds text + chips when a draft loads.
//! - [`insert_chip`] for menu selections.

use std::ops::Range;

use gpui_kit::{
    App, AppContext as _, Entity, Window,
    base::input::{InputContent, InputEditorStyle, TextareaState},
};
use t3_logic::composer::{
    draft::TerminalContextMeta,
    prompt::{self, InlineTokenKind, TERMINAL_CONTEXT_PLACEHOLDER},
    search::skill_display_name,
};
use t3_protocol::server::ProviderSkill;
use t3_ui::ActiveColors as _;

use super::{chips::ChipKind, style};

/// A new, empty prompt editor that grows from one row to [`style::EDITOR_MAX_ROWS`].
pub fn new_editor(window: &mut Window, cx: &mut App) -> Entity<TextareaState> {
    cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, style::EDITOR_MAX_ROWS))
}

/// Paints the editor with the composer's colors: foreground text and caret, the theme selection.
pub fn apply_style(editor: &Entity<TextareaState>, cx: &mut App) {
    let colors = cx.colors();
    let style = InputEditorStyle {
        foreground: colors.foreground,
        muted_foreground: style::alpha(colors.muted_foreground, 0.35),
        caret: colors.foreground,
        ..Default::default()
    };
    editor.update(cx, |state, _| state.set_editor_style(style));
}

/// The label a chip shows.
pub fn chip_label(kind: &InlineTokenKind, skills: &[ProviderSkill]) -> String {
    match kind {
        InlineTokenKind::Mention { path } => prompt::basename(path).to_owned(),
        InlineTokenKind::Skill { name } => skills
            .iter()
            .find(|skill| &skill.name == name)
            .map(skill_display_name)
            .unwrap_or_else(|| {
                let mut chars = name.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }),
    }
}

fn chip_kind(kind: &InlineTokenKind) -> ChipKind {
    match kind {
        InlineTokenKind::Mention { path } => ChipKind::File { path: path.clone() },
        InlineTokenKind::Skill { name } => ChipKind::Skill { name: name.clone() },
    }
}

/// Text plus chips for `prompt`: every recognized mention and skill, and one terminal chip per
/// U+FFFC, matched in order to `contexts` (`live` says which still have their text).
pub fn content_for_prompt(
    prompt_text: &str,
    contexts: &[TerminalContextMeta],
    live: &dyn Fn(&str) -> bool,
    skills: &[ProviderSkill],
) -> InputContent {
    let mut content = InputContent::new(prompt_text.to_owned());
    for token in prompt::collect_inline_tokens(prompt_text) {
        let source = &prompt_text[token.range.clone()];
        let chip = chip_kind(&token.kind).token(source, &chip_label(&token.kind, skills));
        content = match content.clone().with_token(token.range, chip) {
            Ok(next) => next,
            Err(_) => content,
        };
    }
    let placeholder = TERMINAL_CONTEXT_PLACEHOLDER.to_string();
    for (offset, context) in prompt::terminal_placeholder_offsets(prompt_text)
        .into_iter()
        .zip(contexts)
    {
        let kind = ChipKind::Terminal {
            context_id: context.id.clone(),
            expired: !live(&context.id),
        };
        let chip = kind.token(&placeholder, &context.label());
        let range = offset..offset + placeholder.len();
        content = match content.clone().with_token(range, chip) {
            Ok(next) => next,
            Err(_) => content,
        };
    }
    content
}

/// Turns completed tokens that are still plain text into chips, keeping the selection. Returns
/// whether anything changed. Runs after each edit; finding nothing new is the common case.
pub fn sync_chips(
    editor: &Entity<TextareaState>,
    skills: &[ProviderSkill],
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let state = editor.read(cx);
    let text = state.value().to_string();
    let existing: Vec<Range<usize>> = state.tokens().iter().map(|span| span.range()).collect();
    let pending: Vec<_> = prompt::collect_inline_tokens(&text)
        .into_iter()
        .filter(|token| {
            !existing
                .iter()
                .any(|range| range.start < token.range.end && token.range.start < range.end)
        })
        .collect();
    if pending.is_empty() {
        return false;
    }
    let selection = state.selected_range();
    editor.update(cx, |state, cx| {
        for token in pending {
            let source = &text[token.range.clone()];
            let chip = chip_kind(&token.kind).token(source, &chip_label(&token.kind, skills));
            let _ = state.replace_range_with_token(token.range, chip, window, cx);
        }
        state.set_selected_range(selection, cx);
    });
    true
}

/// Replaces `range` with a chip followed by one space, reusing a space that already follows.
/// The caret lands after the space.
pub fn insert_chip(
    editor: &Entity<TextareaState>,
    range: Range<usize>,
    kind: InlineTokenKind,
    skills: &[ProviderSkill],
    window: &mut Window,
    cx: &mut App,
) {
    let source = match &kind {
        InlineTokenKind::Mention { path } => prompt::serialize_file_link(path),
        InlineTokenKind::Skill { name } => prompt::serialize_skill(name),
    };
    let chip = chip_kind(&kind).token(&source, &chip_label(&kind, skills));
    editor.update(cx, |state, cx| {
        if state
            .replace_range_with_token(range.clone(), chip, window, cx)
            .is_err()
        {
            return;
        }
        let after = range.start + source.len();
        let text = state.value();
        if text[after..].starts_with(' ') {
            state.set_selected_range(after + 1..after + 1, cx);
        } else {
            state.set_selected_range(after..after, cx);
            state.insert(" ", window, cx);
        }
    });
}

/// Replaces `range` with plain `text` and puts the caret after it.
pub fn replace_text(
    editor: &Entity<TextareaState>,
    range: Range<usize>,
    text: &str,
    window: &mut Window,
    cx: &mut App,
) {
    editor.update(cx, |state, cx| {
        state.set_selected_range(range, cx);
        state.replace(text.to_owned(), window, cx);
    });
}

/// Whether the caret sits right after a chip: typing there opens no menu (web
/// `isCollapsedCursorAdjacentToInlineToken(.., "left")`).
pub fn caret_after_chip(editor: &TextareaState) -> bool {
    let cursor = editor.cursor();
    editor
        .tokens()
        .iter()
        .any(|span| span.range().end == cursor)
}
