//! Syntax colors for a file's lines, computed with `t3-highlight` the way `@pierre/diffs`
//! highlights partial patches: each hunk's old side (context and deletions) and new side
//! (context and additions) are highlighted as separate snippets, so grammar state restarts at
//! every hunk.

use std::ops::Range;

use t3_highlight::{Language, Style, Theme, highlight};

use super::style::TOKENIZE_MAX_LINE_LENGTH;
use crate::{
    palette::Appearance,
    patch::{Block, FileDiff, Hunk},
    word_diff::word_emphasis,
};

/// Pierre's `tokenizeMaxLength`: files with more lines than this render plain.
const TOKENIZE_MAX_LINES: usize = 100_000;

/// A styled byte range of one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// Syntax spans and word emphasis of every line of a file, indexed like
/// `FileDiff::{old,new}_lines`. An empty list means plain text / no emphasis.
#[derive(Debug, Default)]
pub(crate) struct FileHighlights {
    pub old: Vec<Vec<Span>>,
    pub new: Vec<Vec<Span>>,
    pub old_emphasis: Vec<Vec<Range<usize>>>,
    pub new_emphasis: Vec<Vec<Range<usize>>>,
}

pub(crate) fn theme(appearance: Appearance) -> Theme {
    match appearance {
        Appearance::Light => Theme::Light,
        Appearance::Dark => Theme::Dark,
    }
}

/// Highlights `file` for `theme` and computes word emphasis. Runs on a background thread.
pub(crate) fn highlight_file(file: &FileDiff, theme: Theme) -> FileHighlights {
    let mut highlights = FileHighlights {
        old: vec![Vec::new(); file.old_lines.len()],
        new: vec![Vec::new(); file.new_lines.len()],
        old_emphasis: vec![Vec::new(); file.old_lines.len()],
        new_emphasis: vec![Vec::new(); file.new_lines.len()],
    };
    // Pierre pairs the i-th deletion of a change run with its i-th addition.
    for block in file.hunks.iter().flat_map(|hunk| &hunk.blocks) {
        if let Block::Change {
            old_index,
            new_index,
            deletions,
            additions,
            ..
        } = *block
        {
            for offset in 0..deletions.min(additions) {
                let (old, new) = (old_index + offset, new_index + offset);
                let (Some(deleted), Some(added)) =
                    (file.old_lines.get(old), file.new_lines.get(new))
                else {
                    continue;
                };
                let emphasis = word_emphasis(deleted, added);
                highlights.old_emphasis[old] = emphasis.deletion;
                highlights.new_emphasis[new] = emphasis.addition;
            }
        }
    }
    let language = Language::from_path(&file.path);
    if language.is_plain() || file.old_lines.len().max(file.new_lines.len()) > TOKENIZE_MAX_LINES {
        return highlights;
    }
    for hunk in &file.hunks {
        let (old, new) = hunk_ranges(hunk);
        highlight_range(&file.old_lines, old, language, theme, &mut highlights.old);
        highlight_range(&file.new_lines, new, language, theme, &mut highlights.new);
    }
    highlights
}

fn highlight_range(
    lines: &[String],
    range: Range<usize>,
    language: Language,
    theme: Theme,
    out: &mut [Vec<Span>],
) {
    let Some(slice) = lines.get(range.clone()) else {
        return;
    };
    if slice.is_empty() {
        return;
    }
    let text = slice.join("\n");
    let highlighted = highlight(&text, language, theme);
    for (offset, line) in highlighted.lines().iter().enumerate() {
        let index = range.start + offset;
        let line_range = line.range();
        if line_range.len() > TOKENIZE_MAX_LINE_LENGTH {
            continue;
        }
        let Some(target) = out.get_mut(index) else {
            break;
        };
        *target = line
            .tokens()
            .iter()
            .map(|token| Span {
                start: token.range.start - line_range.start,
                end: token.range.end - line_range.start,
                style: token.style,
            })
            .collect();
    }
}

/// Indices of a hunk's lines in `old_lines` and `new_lines`.
fn hunk_ranges(hunk: &Hunk) -> (Range<usize>, Range<usize>) {
    let mut old: Option<Range<usize>> = None;
    let mut new: Option<Range<usize>> = None;
    let extend = |range: &mut Option<Range<usize>>, start: usize, len: usize| {
        let end = start + len;
        *range = Some(match range.take() {
            Some(existing) => existing.start.min(start)..existing.end.max(end),
            None => start..end,
        });
    };
    for block in &hunk.blocks {
        match *block {
            Block::Context {
                old_index,
                new_index,
                len,
                ..
            } => {
                extend(&mut old, old_index, len);
                extend(&mut new, new_index, len);
            }
            Block::Change {
                old_index,
                new_index,
                deletions,
                additions,
                ..
            } => {
                extend(&mut old, old_index, deletions);
                extend(&mut new, new_index, additions);
            }
        }
    }
    (old.unwrap_or_default(), new.unwrap_or_default())
}
