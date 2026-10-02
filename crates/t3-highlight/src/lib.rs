//! Syntax highlighting that reproduces the fork's code colors without GPUI.
//!
//! The fork highlights with Shiki and the `pierre-dark` / `pierre-light` themes. This crate runs
//! syntect (pure-Rust regex) with those themes converted to TextMate plists, and resolves
//! languages the way Shiki and `@pierre/diffs` do. `tools/export-assets.mjs` regenerates
//! `assets/` from the fork.
//!
//! ```
//! use t3_highlight::{Language, Theme, highlight};
//!
//! let code = "const a = 1;\n";
//! let highlighted = highlight(code, Language::from_fence("ts"), Theme::Dark);
//! for (range, style) in highlighted.spans() {
//!     let _ = (&code[range], style.color); // 0xRRGGBBAA, ready for `gpui::rgba`
//! }
//! ```
//!
//! - [`highlight`] is cached by (content hash, language, theme); call it freely.
//! - [`StreamingHighlighter`] re-highlights a growing block in time proportional to the new
//!   lines, for code that is still streaming in.
//! - [`Highlighted::lines`] gives per-line tokens (e.g. for a diff view).
//!
//! The first call loads the syntax set (~1 MB) and themes; later calls are fast. All entry points
//! are `Send + Sync` and can run on a background thread.

mod cache;
mod engine;
mod language;

use std::{ops::Range, sync::Arc};

pub use engine::StreamingHighlighter;
pub use language::Language;

/// Which Pierre theme to color with. Matches the app's resolved light/dark appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Theme {
    /// `pierre-dark`.
    Dark,
    /// `pierre-light`.
    Light,
}

/// How one token is drawn. `color` is `0xRRGGBBAA`, so `gpui::rgba(style.color)` works directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub color: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// A styled byte range. Ranges are absolute offsets into the highlighted source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub range: Range<usize>,
    pub style: Style,
}

/// One source line: its byte range (without the `\n` / `\r\n` terminator) and the tokens that
/// cover it exactly, in order. Adjacent tokens with the same style are merged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    range: Range<usize>,
    tokens: Vec<Token>,
}

impl Line {
    /// Absolute byte range of the line's content, terminator excluded.
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// Tokens covering [`Self::range`] with no gaps. Empty for an empty line.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
}

/// The highlighted form of a piece of code. Lines follow `str::lines` (a trailing newline does
/// not start an empty last line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Highlighted {
    len: usize,
    default_style: Style,
    lines: Vec<Line>,
}

impl Highlighted {
    /// Per-line tokens, in order.
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// The theme's plain-text style (Shiki's `<pre>` color), used for unstyled bytes.
    pub fn default_style(&self) -> Style {
        self.default_style
    }

    /// Length in bytes of the source this was computed from.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the source was empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Contiguous `(range, style)` spans covering the whole source, line terminators included
    /// (in the default style). Convenient for building text runs.
    pub fn spans(&self) -> impl Iterator<Item = (Range<usize>, Style)> + '_ {
        let default = self.default_style;
        let mut cursor = 0;
        let mut lines = self.lines.iter();
        let mut tokens: std::slice::Iter<'_, Token> = [].iter();
        let len = self.len;
        std::iter::from_fn(move || {
            loop {
                if let Some(token) = tokens.next() {
                    cursor = token.range.end;
                    return Some((token.range.clone(), token.style));
                }
                match lines.next() {
                    Some(line) => {
                        tokens = line.tokens.iter();
                        if line.range.start > cursor {
                            let gap = cursor..line.range.start;
                            cursor = line.range.start;
                            return Some((gap, default));
                        }
                    }
                    None if cursor < len => {
                        let tail = cursor..len;
                        cursor = len;
                        return Some((tail, default));
                    }
                    None => return None,
                }
            }
        })
    }

    /// Rough heap size, used to bound the cache.
    fn cost(&self) -> usize {
        self.lines.len() * std::mem::size_of::<Line>()
            + self
                .lines
                .iter()
                .map(|line| line.tokens.len() * std::mem::size_of::<Token>())
                .sum::<usize>()
    }
}

/// Highlights `code`, returning a shared result from the cache when the same
/// (code, language, theme) was highlighted before.
///
/// Never fails: unknown languages and grammar errors produce plain text in the theme's
/// foreground, like the fork's fallback to Shiki's `text`.
pub fn highlight(code: &str, language: Language, theme: Theme) -> Arc<Highlighted> {
    let key = cache::Key::new(code, language, theme);
    if let Some(hit) = cache::get(&key) {
        return hit;
    }
    let highlighted = Arc::new(engine::highlight_all(code, language, theme));
    cache::insert(key, highlighted.clone(), highlighted.cost());
    highlighted
}

/// Returns the cached result for (code, language, theme) without highlighting on a miss. Lets a
/// UI render synchronously when it can and schedule [`highlight`] in the background otherwise.
pub fn cached(code: &str, language: Language, theme: Theme) -> Option<Arc<Highlighted>> {
    cache::get(&cache::Key::new(code, language, theme))
}

/// Loads the grammar sets and themes (~130ms the first time). Call it on a background thread at
/// startup so the first highlight on the UI thread does not pay for it.
pub fn preload() {
    engine::preload();
}

/// The theme's plain-text style, without highlighting anything.
pub fn default_style(theme: Theme) -> Style {
    engine::default_style(theme)
}
