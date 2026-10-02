//! Word-level emphasis inside paired changed lines, as the web client renders it.
//!
//! `DiffPanel` passes `lineDiffType: "none"`, but `@pierre/diffs` takes its render options from
//! the worker pool, whose default is `"word-alt"`; so the app emphasizes changed words. This
//! ports jsdiff 8's `diffWordsWithSpace` (tokenizer and Myers variant, for identical tie
//! breaking) and Pierre's `word-alt` span joining (`computeLineDiffDecorations`).

use std::ops::Range;

/// Pierre's `maxLineDiffLength`: longer lines get no emphasis.
pub const MAX_LINE_DIFF_LENGTH: usize = 1000;

/// Byte ranges to emphasize in a deleted line and the addition paired with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordEmphasis {
    pub deletion: Vec<Range<usize>>,
    pub addition: Vec<Range<usize>>,
}

/// Emphasis for `deletion` replaced by `addition` (lines without their newline).
pub fn word_emphasis(deletion: &str, addition: &str) -> WordEmphasis {
    if utf16_len(deletion) > MAX_LINE_DIFF_LENGTH || utf16_len(addition) > MAX_LINE_DIFF_LENGTH {
        return WordEmphasis::default();
    }
    let changes = diff_words_with_space(deletion, addition);
    let mut deletion_spans: Vec<(bool, usize)> = Vec::new();
    let mut addition_spans: Vec<(bool, usize)> = Vec::new();
    let last = changes.len().saturating_sub(1);
    for (ix, change) in changes.iter().enumerate() {
        let is_last = ix == last;
        match change.kind {
            ChangeKind::Common => {
                push_or_join(
                    &mut deletion_spans,
                    false,
                    change.text_len,
                    change.chars,
                    is_last,
                );
                push_or_join(
                    &mut addition_spans,
                    false,
                    change.text_len,
                    change.chars,
                    is_last,
                );
            }
            ChangeKind::Removed => push_or_join(
                &mut deletion_spans,
                true,
                change.text_len,
                change.chars,
                is_last,
            ),
            ChangeKind::Added => push_or_join(
                &mut addition_spans,
                true,
                change.text_len,
                change.chars,
                is_last,
            ),
        }
    }
    WordEmphasis {
        deletion: changed_ranges(&deletion_spans),
        addition: changed_ranges(&addition_spans),
    }
}

/// Pierre's `pushOrJoinSpan` with `enableJoin`: merges runs of the same kind, and folds a
/// one-character common run into a preceding changed run.
fn push_or_join(
    spans: &mut Vec<(bool, usize)>,
    changed: bool,
    len: usize,
    chars: usize,
    is_last: bool,
) {
    match spans.last_mut() {
        Some((last_changed, last_len))
            if !is_last
                && (*last_changed == changed || (!changed && chars == 1 && *last_changed)) =>
        {
            *last_len += len;
        }
        _ => spans.push((changed, len)),
    }
}

fn changed_ranges(spans: &[(bool, usize)]) -> Vec<Range<usize>> {
    let mut offset = 0;
    let mut ranges = Vec::new();
    for &(changed, len) in spans {
        if changed && len > 0 {
            ranges.push(offset..offset + len);
        }
        offset += len;
    }
    ranges
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeKind {
    Common,
    Added,
    Removed,
}

/// A run of tokens: its byte length and UTF-16 length (JavaScript's `value.length`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Change {
    kind: ChangeKind,
    text_len: usize,
    chars: usize,
}

/// jsdiff's word characters: ASCII alphanumerics, `_`, and the Latin ranges of
/// `extendedWordChars`.
fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || ch == '_'
        || matches!(ch as u32,
            0xAD | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2C6 | 0x2C8..=0x2D7 | 0x2DE..=0x2FF
            | 0x1E00..=0x1EFF)
}

/// `WordsWithSpaceDiff.tokenize`: newlines, word runs, whitespace runs, or single other chars.
fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let mut end = start + ch.len_utf8();
        if ch == '\r' && chars.peek().is_some_and(|&(_, next)| next == '\n') {
            let (ix, next) = chars.next().unwrap_or((end, '\n'));
            end = ix + next.len_utf8();
        } else if ch != '\n' {
            let class = |c: char| {
                if is_word_char(c) {
                    Some(true)
                } else if c.is_whitespace() && c != '\n' && c != '\r' {
                    Some(false)
                } else {
                    None
                }
            };
            if let Some(run) = class(ch) {
                while let Some(&(ix, next)) = chars.peek() {
                    if class(next) != Some(run) {
                        break;
                    }
                    end = ix + next.len_utf8();
                    chars.next();
                }
            }
        }
        tokens.push(&text[start..end]);
    }
    tokens
}

/// One step of a path through the edit graph, linked back to the previous component.
struct Component {
    count: usize,
    kind: ChangeKind,
    previous: Option<usize>,
}

#[derive(Clone, Copy)]
struct Path {
    old_pos: isize,
    last: Option<usize>,
}

/// jsdiff's `Diff.diffWithOptionsObj` for string tokens without options.
fn diff_words_with_space(old: &str, new: &str) -> Vec<Change> {
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    let (old_len, new_len) = (old_tokens.len() as isize, new_tokens.len() as isize);
    let mut components: Vec<Component> = Vec::new();

    let extract_common =
        |path: &mut Path, diagonal: isize, components: &mut Vec<Component>| -> isize {
            let mut old_pos = path.old_pos;
            let mut new_pos = old_pos - diagonal;
            let mut common = 0;
            while new_pos + 1 < new_len
                && old_pos + 1 < old_len
                && old_tokens[(old_pos + 1) as usize] == new_tokens[(new_pos + 1) as usize]
            {
                new_pos += 1;
                old_pos += 1;
                common += 1;
            }
            if common > 0 {
                components.push(Component {
                    count: common,
                    kind: ChangeKind::Common,
                    previous: path.last,
                });
                path.last = Some(components.len() - 1);
            }
            path.old_pos = old_pos;
            new_pos
        };

    let add_to_path =
        |path: Path, kind: ChangeKind, old_inc: isize, components: &mut Vec<Component>| {
            if let Some(last) = path.last
                && components[last].kind == kind
            {
                let previous = components[last].previous;
                let count = components[last].count + 1;
                components.push(Component {
                    count,
                    kind,
                    previous,
                });
            } else {
                components.push(Component {
                    count: 1,
                    kind,
                    previous: path.last,
                });
            }
            Path {
                old_pos: path.old_pos + old_inc,
                last: Some(components.len() - 1),
            }
        };

    // Diagonals run from -(old+new) to old+new; index with an offset.
    let max_edit = (old_len + new_len) as usize;
    let offset = max_edit as isize + 1;
    let mut best: Vec<Option<Path>> = vec![None; 2 * max_edit + 3];
    let mut start = Path {
        old_pos: -1,
        last: None,
    };
    let new_pos = extract_common(&mut start, 0, &mut components);
    if start.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return build_changes(start.last, &components, &old_tokens, &new_tokens);
    }
    best[offset as usize] = Some(start);

    let (mut min_diagonal, mut max_diagonal) = (isize::MIN, isize::MAX);
    for edit_length in 1..=max_edit as isize {
        let mut diagonal = min_diagonal.max(-edit_length);
        while diagonal <= max_diagonal.min(edit_length) {
            let remove_path = best[(diagonal - 1 + offset) as usize].take();
            let add_path = best[(diagonal + 1 + offset) as usize];
            let can_add = add_path.is_some_and(|path| {
                let add_new_pos = path.old_pos - diagonal;
                0 <= add_new_pos && add_new_pos < new_len
            });
            let can_remove = remove_path.is_some_and(|path| path.old_pos + 1 < old_len);
            if !can_add && !can_remove {
                best[(diagonal + offset) as usize] = None;
                diagonal += 2;
                continue;
            }
            // Branch from the path that went furthest in the old text (jsdiff's choice).
            let use_add = match (remove_path, add_path) {
                (Some(remove), Some(add)) => {
                    !can_remove || (can_add && remove.old_pos < add.old_pos)
                }
                (None, _) => true,
                (Some(_), None) => false,
            };
            let mut path = match (use_add, add_path, remove_path) {
                (true, Some(add), _) => add_to_path(add, ChangeKind::Added, 0, &mut components),
                (false, _, Some(remove)) => {
                    add_to_path(remove, ChangeKind::Removed, 1, &mut components)
                }
                _ => {
                    diagonal += 2;
                    continue;
                }
            };
            let new_pos = extract_common(&mut path, diagonal, &mut components);
            if path.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
                return build_changes(path.last, &components, &old_tokens, &new_tokens);
            }
            best[(diagonal + offset) as usize] = Some(path);
            if path.old_pos + 1 >= old_len {
                max_diagonal = max_diagonal.min(diagonal - 1);
            }
            if new_pos + 1 >= new_len {
                min_diagonal = min_diagonal.max(diagonal + 1);
            }
            diagonal += 2;
        }
    }
    Vec::new()
}

/// jsdiff's `buildValues`: walks the component chain and measures each run's text.
fn build_changes(
    last: Option<usize>,
    components: &[Component],
    old_tokens: &[&str],
    new_tokens: &[&str],
) -> Vec<Change> {
    let mut chain = Vec::new();
    let mut cursor = last;
    while let Some(ix) = cursor {
        chain.push(ix);
        cursor = components[ix].previous;
    }
    chain.reverse();
    let (mut old_pos, mut new_pos) = (0, 0);
    chain
        .into_iter()
        .map(|ix| {
            let component = &components[ix];
            let tokens = match component.kind {
                ChangeKind::Removed => {
                    let tokens = &old_tokens[old_pos..old_pos + component.count];
                    old_pos += component.count;
                    tokens
                }
                ChangeKind::Added => {
                    let tokens = &new_tokens[new_pos..new_pos + component.count];
                    new_pos += component.count;
                    tokens
                }
                ChangeKind::Common => {
                    let tokens = &new_tokens[new_pos..new_pos + component.count];
                    new_pos += component.count;
                    old_pos += component.count;
                    tokens
                }
            };
            Change {
                kind: component.kind,
                text_len: tokens.iter().map(|token| token.len()).sum(),
                chars: tokens.iter().map(|token| utf16_len(token)).sum(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Failure modes: tokenizing differently from jsdiff (word vs punctuation vs whitespace
    //! runs, Latin-1 letters), Myers tie-breaking that picks a different but equally short
    //! script, the `word-alt` join rules (single-char common runs folded into the preceding
    //! change, never across the last item), ranges counted in chars instead of bytes, and
    //! panics on empty or identical lines. Expectations come from jsdiff 8.0.3 plus Pierre's
    //! joining run in node; they are UTF-16 offsets, equal to bytes for these ASCII cases.
    #![allow(clippy::single_range_in_vec_init)]
    use super::*;

    fn case(deletion: &str, addition: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
        let emphasis = word_emphasis(deletion, addition);
        (emphasis.deletion, emphasis.addition)
    }

    #[test]
    fn matches_jsdiff_and_pierre() {
        assert_eq!(
            case(
                r#"import { DiffPanel } from "./DiffPanel";"#,
                r#"import { DiffPanel, DiffWorkerPoolProvider } from "./DiffPanel";"#
            ),
            (vec![], vec![18..43])
        );
        assert_eq!(
            case(
                r#"  const isDiffOpen = panel?.activeSurfaceId === "diff";"#,
                r#"  const activeSurface = panel?.isOpen ? panel.activeSurfaceId : null;"#
            ),
            (vec![8..18, 44..54], vec![8..21, 31..46, 62..68])
        );
        assert_eq!(
            case(
                "T3 Code is a minimal web GUI for coding agents.",
                "T3 Code is a minimal desktop and web GUI for coding agents like Codex and Claude."
            ),
            (vec![], vec![21..33, 58..80])
        );
        assert_eq!(
            case(
                r#"import type { FileDiffMetadata } from "@pierre/diffs/types";"#,
                r#"import type { FileDiffMetadata } from "@pierre/diffs";"#
            ),
            (vec![52..58], vec![])
        );
        assert_eq!(case("foo bar baz", "foo qux baz"), (vec![4..7], vec![4..7]));
        assert_eq!(case("a", "b"), (vec![0..1], vec![0..1]));
    }

    #[test]
    fn identical_and_empty_lines() {
        assert_eq!(case("same", "same"), (vec![], vec![]));
        assert_eq!(case("", "added"), (vec![], vec![0..5]));
        assert_eq!(case("", ""), (vec![], vec![]));
    }

    #[test]
    fn latin_letters_are_word_chars_and_ranges_are_bytes() {
        // jsdiff reports UTF-16 [6, 11) for both; "wörld" is 6 bytes.
        assert_eq!(
            case("héllo wörld", "héllo world"),
            (vec![7..13], vec![7..12])
        );
    }

    #[test]
    fn long_lines_get_no_emphasis() {
        let long = "x".repeat(MAX_LINE_DIFF_LENGTH + 1);
        assert_eq!(case(&long, "y"), (vec![], vec![]));
    }
}
