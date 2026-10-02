//! syntect integration: the lazily loaded grammar set and themes, line tokenization, and the
//! incremental highlighter for streaming code.

use std::{io::Cursor, ops::Range, sync::Arc, sync::LazyLock};

use syntect::{
    highlighting::{
        FontStyle, HighlightState, Highlighter, RangedHighlightIterator, Theme as SyntectTheme,
        ThemeSet,
    },
    parsing::{
        ParseState, ScopeStack, SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder,
    },
};

use crate::{Highlighted, Language, Line, Style, Theme, Token, language::Grammar};

/// Lines longer than this (in bytes) are not tokenized and render in the default style, so a
/// minified file cannot stall the UI. Shiki's chat path has no limit; this is a safety valve.
const MAX_TOKENIZED_LINE_LEN: usize = 20_000;

/// bat's grammar set (two-face, fancy-regex build).
static BAT_SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// Shiki's grammars translated by `tools/export-assets.mjs`. Small, so built at first use.
static SHIKI_SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(|| {
    const SOURCES: &[(&str, &str)] = &[
        (
            "diff",
            include_str!("../assets/grammars/diff.sublime-syntax"),
        ),
        (
            "json",
            include_str!("../assets/grammars/json.sublime-syntax"),
        ),
        (
            "python",
            include_str!("../assets/grammars/python.sublime-syntax"),
        ),
        (
            "rust",
            include_str!("../assets/grammars/rust.sublime-syntax"),
        ),
    ];
    let mut builder = SyntaxSetBuilder::new();
    for (name, yaml) in SOURCES {
        let mut definition = SyntaxDefinition::load_from_str(yaml, true, Some(name))
            .unwrap_or_else(|error| panic!("bundled grammar {name}: {error}"));
        // Look grammars up by their Shiki id rather than their display name.
        definition.name = (*name).to_string();
        builder.add(definition);
    }
    builder.build()
});

static THEMES: LazyLock<[SyntectTheme; 2]> = LazyLock::new(|| {
    let load = |bytes: &[u8]| {
        ThemeSet::load_from_reader(&mut Cursor::new(bytes)).expect("bundled theme is valid")
    };
    [
        load(include_bytes!("../assets/pierre-dark.tmTheme")),
        load(include_bytes!("../assets/pierre-light.tmTheme")),
    ]
});

fn theme(theme: Theme) -> &'static SyntectTheme {
    match theme {
        Theme::Dark => &THEMES[0],
        Theme::Light => &THEMES[1],
    }
}

fn index_by_name(set: &SyntaxSet, name: &str) -> Option<u16> {
    set.syntaxes()
        .iter()
        .position(|syntax| syntax.name == name)
        .and_then(|index| u16::try_from(index).ok())
}

pub(crate) fn bat_syntax_index(name: &str) -> Option<u16> {
    index_by_name(&BAT_SYNTAXES, name)
}

pub(crate) fn shiki_syntax_index(name: &str) -> Option<u16> {
    index_by_name(&SHIKI_SYNTAXES, name)
}

/// The syntax set a grammar lives in, and its definition.
pub(crate) fn syntax(grammar: Grammar) -> (&'static SyntaxSet, &'static SyntaxReference) {
    let (set, index) = match grammar {
        Grammar::Shiki(index) => (&*SHIKI_SYNTAXES, index),
        Grammar::Bat(index) => (&*BAT_SYNTAXES, index),
    };
    (set, &set.syntaxes()[usize::from(index)])
}

pub(crate) fn preload() {
    LazyLock::force(&BAT_SYNTAXES);
    LazyLock::force(&SHIKI_SYNTAXES);
    LazyLock::force(&THEMES);
}

pub(crate) fn default_style(theme: Theme) -> Style {
    convert_style(Highlighter::new(self::theme(theme)).get_default())
}

fn convert_style(style: syntect::highlighting::Style) -> Style {
    let color = style.foreground;
    Style {
        color: u32::from_be_bytes([color.r, color.g, color.b, color.a]),
        bold: style.font_style.contains(FontStyle::BOLD),
        italic: style.font_style.contains(FontStyle::ITALIC),
        underline: style.font_style.contains(FontStyle::UNDERLINE),
    }
}

/// Parser + style state between two lines. No parser means plain text.
#[derive(Clone)]
struct LineState {
    parse: Option<(ParseState, &'static SyntaxSet)>,
    highlight: HighlightState,
}

/// Tokenizes lines in order for one (language, theme).
struct LineTokenizer {
    highlighter: Highlighter<'static>,
    default_style: Style,
}

impl LineTokenizer {
    fn new(theme: Theme) -> Self {
        let highlighter = Highlighter::new(self::theme(theme));
        let default_style = convert_style(highlighter.get_default());
        Self {
            highlighter,
            default_style,
        }
    }

    fn initial_state(&self, language: Language) -> LineState {
        let parse = language.grammar().map(|grammar| {
            let (set, syntax) = syntax(grammar);
            (ParseState::new(syntax), set)
        });
        LineState {
            parse,
            highlight: HighlightState::new(&self.highlighter, ScopeStack::new()),
        }
    }

    /// Tokenizes one line (`range` into `source`, terminator excluded), advancing `state`.
    fn line(&self, state: &mut LineState, source: &str, range: Range<usize>) -> Line {
        let content = &source[range.clone()];
        let plain = |range: Range<usize>| Line {
            tokens: (!range.is_empty())
                .then(|| Token {
                    range: range.clone(),
                    style: self.default_style,
                })
                .into_iter()
                .collect(),
            range,
        };
        let Some((parse, set)) = state.parse.as_mut() else {
            return plain(range);
        };
        if content.len() > MAX_TOKENIZED_LINE_LEN {
            return plain(range);
        }
        // The grammars are compiled for lines that end in `\n` (as Shiki's do), including the
        // last one, so a block highlights the same whether or not its final newline arrived.
        let mut buffer = String::with_capacity(content.len() + 1);
        buffer.push_str(content);
        buffer.push('\n');
        let Ok(ops) = parse.parse_line(&buffer, set) else {
            return plain(range);
        };
        let mut tokens: Vec<Token> = Vec::new();
        for (style, _, token_range) in
            RangedHighlightIterator::new(&mut state.highlight, &ops, &buffer, &self.highlighter)
        {
            let end = token_range.end.min(content.len());
            if token_range.start >= end {
                continue;
            }
            let style = convert_style(style);
            let absolute = range.start + token_range.start..range.start + end;
            match tokens.last_mut() {
                Some(last) if last.style == style && last.range.end == absolute.start => {
                    last.range.end = absolute.end;
                }
                _ => tokens.push(Token {
                    range: absolute,
                    style,
                }),
            }
        }
        Line { range, tokens }
    }
}

/// Splits `source[from..]` into lines like `str::lines`, yielding each line's absolute content
/// range and whether a `\n` terminated it.
fn split_lines(source: &str, from: usize) -> impl Iterator<Item = (Range<usize>, bool)> + '_ {
    let bytes = source.as_bytes();
    let mut start = from;
    std::iter::from_fn(move || {
        if start >= source.len() {
            return None;
        }
        match source[start..].find('\n') {
            Some(offset) => {
                let newline = start + offset;
                let end = if newline > start && bytes[newline - 1] == b'\r' {
                    newline - 1
                } else {
                    newline
                };
                let line = start..end;
                start = newline + 1;
                Some((line, true))
            }
            None => {
                let line = start..source.len();
                start = source.len();
                Some((line, false))
            }
        }
    })
}

pub(crate) fn highlight_all(code: &str, language: Language, theme: Theme) -> Highlighted {
    let tokenizer = LineTokenizer::new(theme);
    let mut state = tokenizer.initial_state(language);
    let lines = split_lines(code, 0)
        .map(|(range, _)| tokenizer.line(&mut state, code, range))
        .collect();
    Highlighted {
        len: code.len(),
        default_style: tokenizer.default_style,
        lines,
    }
}

/// Highlights code that grows over time (a code block in a streaming message).
///
/// Each [`Self::update`] reuses the tokenizer state after the last complete line it saw, so the
/// cost is proportional to the new lines plus the unfinished last line. Passing text that does
/// not extend the previous text (an edit, or a different block) starts over. The result is always
/// identical to [`crate::highlight`] on the same text.
pub struct StreamingHighlighter {
    language: Language,
    tokenizer: LineTokenizer,
    /// Source of the committed lines, terminators included; always ends with `\n` or is empty.
    committed_text: String,
    committed_lines: Vec<Line>,
    /// Tokenizer state after the committed lines.
    state: LineState,
}

impl StreamingHighlighter {
    pub fn new(language: Language, theme: Theme) -> Self {
        let tokenizer = LineTokenizer::new(theme);
        let state = tokenizer.initial_state(language);
        Self {
            language,
            tokenizer,
            committed_text: String::new(),
            committed_lines: Vec::new(),
            state,
        }
    }

    /// The language this highlighter was created for.
    pub fn language(&self) -> Language {
        self.language
    }

    /// Highlights `code`, reusing work from previous calls when `code` extends their text.
    pub fn update(&mut self, code: &str) -> Arc<Highlighted> {
        if !code.starts_with(self.committed_text.as_str()) {
            self.committed_text.clear();
            self.committed_lines.clear();
            self.state = self.tokenizer.initial_state(self.language);
        }
        let mut partial = None;
        for (range, terminated) in split_lines(code, self.committed_text.len()) {
            if terminated {
                let line = self.tokenizer.line(&mut self.state, code, range);
                let next_start = code[line.range.end..]
                    .find('\n')
                    .map_or(code.len(), |offset| line.range.end + offset + 1);
                self.committed_text
                    .push_str(&code[self.committed_text.len()..next_start]);
                self.committed_lines.push(line);
            } else {
                let mut scratch = self.state.clone();
                partial = Some(self.tokenizer.line(&mut scratch, code, range));
            }
        }
        let mut lines = Vec::with_capacity(self.committed_lines.len() + 1);
        lines.extend_from_slice(&self.committed_lines);
        lines.extend(partial);
        Arc::new(Highlighted {
            len: code.len(),
            default_style: self.tokenizer.default_style,
            lines,
        })
    }
}
