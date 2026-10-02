//! Turns a parsed file into the rows the diff view paints, in unified or split style, in the
//! order `@pierre/diffs`' `DiffHunksRenderer` emits them for partial patches.

use crate::patch::{Block, ChangeKind, FileDiff, Hunk};

/// Stacked (Pierre `unified`) or side-by-side (`split`) layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DiffStyle {
    #[default]
    Unified,
    Split,
}

/// Which side of the diff a line comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Old,
    New,
}

/// How a line changed. Drives backgrounds, number colors and the gutter bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

/// One line on one side: its text is `FileDiff::{old,new}_lines[index]` for `side`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCell {
    pub kind: LineKind,
    pub side: Side,
    pub index: usize,
    pub number: u32,
}

impl LineCell {
    /// The line's text in `file`.
    pub fn text<'a>(&self, file: &'a FileDiff) -> &'a str {
        let lines = match self.side {
            Side::Old => &file.old_lines,
            Side::New => &file.new_lines,
        };
        lines.get(self.index).map_or("", String::as_str)
    }
}

/// A row of a file's body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// "N unmodified lines" before a hunk. `first` hunks have no top margin.
    Separator { lines: u32, first: bool },
    /// A unified (single-column) line.
    Line(LineCell),
    /// Unified "No newline at end of file" marker, tinted like `kind`.
    NoNewline(LineKind),
    /// A split row. A `None` side is an empty, hatched buffer.
    Split {
        old: Option<LineCell>,
        new: Option<LineCell>,
    },
    /// Split "No newline at end of file" markers; a `None` side is a buffer.
    SplitNoNewline {
        old: Option<LineKind>,
        new: Option<LineKind>,
    },
}

/// Whether `file` renders as two columns. Pierre drops the empty side of added and deleted
/// files, so they render as a single column even in split style.
pub fn is_two_column(file: &FileDiff, style: DiffStyle) -> bool {
    style == DiffStyle::Split && !matches!(file.kind, ChangeKind::Added | ChangeKind::Deleted)
}

/// Every body row of `file` in `style`.
pub fn build_rows(file: &FileDiff, style: DiffStyle) -> Vec<Row> {
    let mut rows = Vec::new();
    let two_column = is_two_column(file, style);
    for (hunk_ix, hunk) in file.hunks.iter().enumerate() {
        if hunk.collapsed_before > 0 {
            rows.push(Row::Separator {
                lines: hunk.collapsed_before,
                first: hunk_ix == 0,
            });
        }
        if two_column {
            push_split_hunk(&mut rows, hunk);
        } else {
            push_unified_hunk(&mut rows, hunk);
        }
    }
    rows
}

fn push_unified_hunk(rows: &mut Vec<Row>, hunk: &Hunk) {
    let last_block = hunk.blocks.len().saturating_sub(1);
    for (block_ix, block) in hunk.blocks.iter().enumerate() {
        let is_last = block_ix == last_block;
        match *block {
            Block::Context {
                new_index,
                new_line,
                len,
                ..
            } => {
                // Unified context rows show the new side's number and text.
                rows.extend((0..len).map(|offset| {
                    Row::Line(LineCell {
                        kind: LineKind::Context,
                        side: Side::New,
                        index: new_index + offset,
                        number: new_line + offset as u32,
                    })
                }));
                // Pierre emits one marker per side, so a context line missing its newline on
                // both sides gets two marker rows.
                if is_last && hunk.old_missing_newline {
                    rows.push(Row::NoNewline(LineKind::Context));
                }
                if is_last && hunk.new_missing_newline {
                    rows.push(Row::NoNewline(LineKind::Context));
                }
            }
            Block::Change {
                old_index,
                new_index,
                old_line,
                new_line,
                deletions,
                additions,
            } => {
                rows.extend((0..deletions).map(|offset| {
                    Row::Line(LineCell {
                        kind: LineKind::Deletion,
                        side: Side::Old,
                        index: old_index + offset,
                        number: old_line + offset as u32,
                    })
                }));
                if is_last && deletions > 0 && hunk.old_missing_newline {
                    rows.push(Row::NoNewline(LineKind::Deletion));
                }
                rows.extend((0..additions).map(|offset| {
                    Row::Line(LineCell {
                        kind: LineKind::Addition,
                        side: Side::New,
                        index: new_index + offset,
                        number: new_line + offset as u32,
                    })
                }));
                if is_last && additions > 0 && hunk.new_missing_newline {
                    rows.push(Row::NoNewline(LineKind::Addition));
                }
            }
        }
    }
}

fn push_split_hunk(rows: &mut Vec<Row>, hunk: &Hunk) {
    let mut last_kinds = (LineKind::Context, LineKind::Context);
    for block in &hunk.blocks {
        match *block {
            Block::Context {
                old_index,
                new_index,
                old_line,
                new_line,
                len,
            } => {
                rows.extend((0..len).map(|offset| Row::Split {
                    old: Some(LineCell {
                        kind: LineKind::Context,
                        side: Side::Old,
                        index: old_index + offset,
                        number: old_line + offset as u32,
                    }),
                    new: Some(LineCell {
                        kind: LineKind::Context,
                        side: Side::New,
                        index: new_index + offset,
                        number: new_line + offset as u32,
                    }),
                }));
                last_kinds = (LineKind::Context, LineKind::Context);
            }
            Block::Change {
                old_index,
                new_index,
                old_line,
                new_line,
                deletions,
                additions,
            } => {
                rows.extend((0..deletions.max(additions)).map(|offset| Row::Split {
                    old: (offset < deletions).then(|| LineCell {
                        kind: LineKind::Deletion,
                        side: Side::Old,
                        index: old_index + offset,
                        number: old_line + offset as u32,
                    }),
                    new: (offset < additions).then(|| LineCell {
                        kind: LineKind::Addition,
                        side: Side::New,
                        index: new_index + offset,
                        number: new_line + offset as u32,
                    }),
                }));
                last_kinds = (LineKind::Deletion, LineKind::Addition);
            }
        }
    }
    if hunk.old_missing_newline || hunk.new_missing_newline {
        rows.push(Row::SplitNoNewline {
            old: hunk.old_missing_newline.then_some(last_kinds.0),
            new: hunk.new_missing_newline.then_some(last_kinds.1),
        });
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: separators missing or doubled (only hunks with hidden lines get one, the
    //! first without a top margin); unified rows showing the old number for context lines;
    //! deletions and additions interleaved instead of grouped; split rows misaligned when a
    //! change block is lopsided (the short side must become buffers, top-aligned); added and
    //! deleted files rendered with an empty second column; no-newline markers on the wrong side
    //! or in the wrong place.
    use super::*;
    use crate::patch::{RenderablePatch, renderable_patch};

    fn files() -> Vec<FileDiff> {
        match renderable_patch(include_str!("../fixtures/multi-file.patch")) {
            RenderablePatch::Files(files) => files,
            other => panic!("{other:?}"),
        }
    }

    fn file(path: &str) -> FileDiff {
        files()
            .into_iter()
            .find(|file| file.path == path)
            .expect("file in fixture")
    }

    fn line(kind: LineKind, side: Side, index: usize, number: u32) -> LineCell {
        LineCell {
            kind,
            side,
            index,
            number,
        }
    }

    #[test]
    fn unified_rows_group_changes_and_add_separators() {
        let chat = file("apps/web/src/components/ChatView.tsx");
        let rows = build_rows(&chat, DiffStyle::Unified);
        assert_eq!(
            rows[0],
            Row::Separator {
                lines: 11,
                first: true
            }
        );
        assert_eq!(
            rows[1],
            Row::Line(line(LineKind::Context, Side::New, 0, 12))
        );
        assert_eq!(
            rows[4],
            Row::Line(line(LineKind::Deletion, Side::Old, 3, 15))
        );
        assert_eq!(
            rows[5],
            Row::Line(line(LineKind::Addition, Side::New, 3, 15))
        );
        assert_eq!(
            rows[6],
            Row::Line(line(LineKind::Addition, Side::New, 4, 16))
        );
        assert_eq!(
            rows[10],
            Row::Separator {
                lines: 121,
                first: false
            }
        );
        let separators = rows
            .iter()
            .filter(|row| matches!(row, Row::Separator { .. }))
            .count();
        assert_eq!(separators, 3);
        // 3 separators + 9 + 10 + 10 lines.
        assert_eq!(rows.len(), 3 + 9 + 10 + 10);
        assert_eq!(
            rows[5].clone_text(&chat),
            "import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";"
        );
    }

    #[test]
    fn split_rows_pad_the_short_side() {
        let chat = file("apps/web/src/components/ChatView.tsx");
        let rows = build_rows(&chat, DiffStyle::Split);
        // Second hunk: 3 deletions against 1 addition.
        let change: Vec<_> = rows
            .iter()
            .filter(|row| {
                matches!(row, Row::Split { old: Some(cell), .. } if cell.kind == LineKind::Deletion && cell.number >= 143)
            })
            .collect();
        assert_eq!(
            change,
            [
                &Row::Split {
                    old: Some(line(LineKind::Deletion, Side::Old, 10, 143)),
                    new: Some(line(LineKind::Addition, Side::New, 11, 144)),
                },
                &Row::Split {
                    old: Some(line(LineKind::Deletion, Side::Old, 11, 144)),
                    new: None,
                },
                &Row::Split {
                    old: Some(line(LineKind::Deletion, Side::Old, 12, 145)),
                    new: None,
                },
            ]
        );
        // Third hunk: additions only, so the old side is all buffer.
        assert!(rows.iter().any(|row| matches!(
            row,
            Row::Split { old: None, new: Some(cell) } if cell.number == 200
        )));
    }

    #[test]
    fn added_and_deleted_files_stay_single_column() {
        let added = file("apps/web/src/lib/diffStats.ts");
        assert!(!is_two_column(&added, DiffStyle::Split));
        let rows = build_rows(&added, DiffStyle::Split);
        assert_eq!(rows.len(), 15);
        assert_eq!(
            rows[0],
            Row::Line(line(LineKind::Addition, Side::New, 0, 1))
        );
        assert_eq!(rows[14], Row::NoNewline(LineKind::Addition));
        let deleted = file("apps/server/src/legacy/checkpointPaths.ts");
        assert!(
            build_rows(&deleted, DiffStyle::Split)
                .iter()
                .all(|row| matches!(row, Row::Line(cell) if cell.kind == LineKind::Deletion))
        );
    }

    #[test]
    fn missing_newline_markers_follow_their_side() {
        let patch = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,3 @@\n a\n-b\n\\ No newline at end of file\n+b\n+c\n\\ No newline at end of file\n";
        let RenderablePatch::Files(files) = renderable_patch(patch) else {
            panic!()
        };
        let unified = build_rows(&files[0], DiffStyle::Unified);
        assert_eq!(
            unified,
            [
                Row::Line(line(LineKind::Context, Side::New, 0, 1)),
                Row::Line(line(LineKind::Deletion, Side::Old, 1, 2)),
                Row::NoNewline(LineKind::Deletion),
                Row::Line(line(LineKind::Addition, Side::New, 1, 2)),
                Row::Line(line(LineKind::Addition, Side::New, 2, 3)),
                Row::NoNewline(LineKind::Addition),
            ]
        );
        let split = build_rows(&files[0], DiffStyle::Split);
        assert_eq!(
            split.last(),
            Some(&Row::SplitNoNewline {
                old: Some(LineKind::Deletion),
                new: Some(LineKind::Addition),
            })
        );

        let context = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-a\n+b\n c\n\\ No newline at end of file\n";
        let RenderablePatch::Files(files) = renderable_patch(context) else {
            panic!()
        };
        let unified = build_rows(&files[0], DiffStyle::Unified);
        assert_eq!(
            &unified[3..],
            [
                Row::NoNewline(LineKind::Context),
                Row::NoNewline(LineKind::Context)
            ]
        );
    }

    #[test]
    fn files_without_hunks_have_no_rows() {
        let script = file("scripts/release.sh");
        assert!(build_rows(&script, DiffStyle::Unified).is_empty());
        assert!(build_rows(&script, DiffStyle::Split).is_empty());
    }

    impl Row {
        fn clone_text(&self, file: &FileDiff) -> String {
            match self {
                Row::Line(cell) => cell.text(file).to_owned(),
                _ => String::new(),
            }
        }
    }
}
