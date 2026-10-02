//! Review comments on diff lines, ported from the web client's `reviewCommentContext.ts`.
//!
//! The user selects a range of lines in the diff view and writes a comment. The comment is
//! stored in the composer draft as a [`ReviewComment`] and sent with the next prompt as a
//! `<review_comment>` block ([`format_review_comment`], [`append_review_comments_to_prompt`]).
//!
//! Lines are addressed two ways, like Pierre's `@pierre/diffs`:
//! - a selection point is a line number on a side ([`Side::Old`] is Pierre's `"deletions"`,
//!   [`Side::New`] is `"additions"`), see [`LineRange`];
//! - a stored comment keeps indices into the file's flattened review lines ([`review_lines`]),
//!   so it can be placed again with [`restore_range`].

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    patch::{Block, FileDiff},
    rows::{Row, Side},
};

/// A review comment as the composer draft stores it (`ReviewCommentContext`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: String,
    /// `turn:{turnId}`, `unstaged` or `branch`.
    pub section_id: String,
    /// `Turn {n}`, `Working tree` or `Branch changes`.
    pub section_title: String,
    pub file_path: String,
    /// First and last selected line, as indices into [`review_lines`].
    pub start_index: usize,
    pub end_index: usize,
    /// `+12`, `-12`, `12 to 15`, `+12 to +15`, `line` or `N lines`.
    pub range_label: String,
    pub text: String,
    /// A mini hunk: `@@ -a,b +c,d @@` and the selected lines with their markers.
    pub diff: String,
    /// Code fence language of `diff`; `None` reads as `"diff"`.
    pub fence_language: Option<String>,
}

impl ReviewComment {
    /// Whether the diff view shows this comment for `file_path` in section `section_id`
    /// (file comments from the file preview use another fence language).
    pub fn belongs_to(&self, section_id: &str, file_path: &str) -> bool {
        self.section_id == section_id
            && self.file_path == file_path
            && self.fence_language.as_deref().unwrap_or("diff") == "diff"
    }
}

/// A selected line range (Pierre `SelectedLineRange`): from line `start` on `side` to line
/// `end` on `end_side`, in selection order (`end` may be above `start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineRange {
    pub start: u32,
    pub side: Side,
    pub end: u32,
    pub end_side: Side,
}

impl LineRange {
    /// A single line.
    pub fn line(number: u32, side: Side) -> Self {
        Self {
            start: number,
            side,
            end: number,
            end_side: side,
        }
    }

    /// The side the comment card opens on: the end's side (`annotationSide`).
    pub fn annotation_side(&self) -> Side {
        self.end_side
    }
}

/// How a review line changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Context,
    Add,
    Delete,
}

impl Change {
    fn marker(self) -> &'static str {
        match self {
            Self::Context => " ",
            Self::Add => "+",
            Self::Delete => "-",
        }
    }
}

/// One line of a file in patch order: context lines once, then each change block's deletions
/// followed by its additions (`buildDiffReviewLines`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewLine<'a> {
    pub change: Change,
    pub old_number: Option<u32>,
    pub new_number: Option<u32>,
    pub content: &'a str,
}

impl ReviewLine<'_> {
    fn number(&self, side: Side) -> Option<u32> {
        match side {
            Side::Old => self.old_number,
            Side::New => self.new_number,
        }
    }

    /// The point a stored comment restores to: deletions by their old number, everything
    /// else by its new number (`getDiffReviewSelectionPoint`).
    fn selection_point(&self) -> Option<(u32, Side)> {
        if self.change == Change::Delete
            && let Some(number) = self.old_number
        {
            return Some((number, Side::Old));
        }
        self.new_number
            .map(|number| (number, Side::New))
            .or_else(|| self.old_number.map(|number| (number, Side::Old)))
    }
}

/// Every line of `file` in review order.
pub fn review_lines(file: &FileDiff) -> Vec<ReviewLine<'_>> {
    let old = |index: usize| file.old_lines.get(index).map_or("", String::as_str);
    let new = |index: usize| file.new_lines.get(index).map_or("", String::as_str);
    let mut lines = Vec::new();
    for block in file.hunks.iter().flat_map(|hunk| &hunk.blocks) {
        match *block {
            Block::Context {
                new_index,
                old_line,
                new_line,
                len,
                ..
            } => lines.extend((0..len).map(|offset| ReviewLine {
                change: Change::Context,
                old_number: Some(old_line + offset as u32),
                new_number: Some(new_line + offset as u32),
                content: new(new_index + offset),
            })),
            Block::Change {
                old_index,
                new_index,
                old_line,
                new_line,
                deletions,
                additions,
            } => {
                lines.extend((0..deletions).map(|offset| ReviewLine {
                    change: Change::Delete,
                    old_number: Some(old_line + offset as u32),
                    new_number: None,
                    content: old(old_index + offset),
                }));
                lines.extend((0..additions).map(|offset| ReviewLine {
                    change: Change::Add,
                    old_number: None,
                    new_number: Some(new_line + offset as u32),
                    content: new(new_index + offset),
                }));
            }
        }
    }
    lines
}

/// The line numbers a body row shows per side, used to hit-test selections and place comment
/// cards. Unified context rows carry both numbers: Pierre finds them by either.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowPoints {
    pub old: Option<u32>,
    pub new: Option<u32>,
}

impl RowPoints {
    pub fn number(&self, side: Side) -> Option<u32> {
        match side {
            Side::Old => self.old,
            Side::New => self.new,
        }
    }
}

/// [`RowPoints`] for each of `rows` (built from `file` by [`crate::rows::build_rows`]).
pub fn row_points(file: &FileDiff, rows: &[Row]) -> Vec<RowPoints> {
    // Unified line rows follow review order one to one.
    let mut lines = review_lines(file).into_iter();
    rows.iter()
        .map(|row| match row {
            Row::Line(_) => lines
                .next()
                .map_or_else(RowPoints::default, |line| RowPoints {
                    old: line.old_number,
                    new: line.new_number,
                }),
            Row::Split { old, new } => RowPoints {
                old: old.map(|cell| cell.number),
                new: new.map(|cell| cell.number),
            },
            _ => RowPoints::default(),
        })
        .collect()
}

/// The row showing line `number` on `side` (Pierre `getLineIndex` for lines that exist).
pub fn row_for_point(points: &[RowPoints], number: u32, side: Side) -> Option<usize> {
    points
        .iter()
        .position(|points| points.number(side) == Some(number))
}

/// The review-line index of `number` on `side`, falling back to the other side's numbers
/// (`findDiffReviewLineIndex`).
fn line_index(lines: &[ReviewLine], number: u32, side: Side) -> Option<usize> {
    let other = match side {
        Side::Old => Side::New,
        Side::New => Side::Old,
    };
    lines
        .iter()
        .position(|line| line.number(side) == Some(number))
        .or_else(|| {
            lines
                .iter()
                .position(|line| line.number(other) == Some(number))
        })
}

/// `@@ -a,b +c,d @@` parts for one side: the first number present and how many lines have one.
fn hunk_range(lines: &[ReviewLine], side: Side) -> (u32, usize) {
    let mut numbers = lines.iter().filter_map(|line| line.number(side));
    let start = numbers.next();
    (start.unwrap_or(0), start.map_or(0, |_| numbers.count() + 1))
}

/// `formatDiffReviewRangeLabel`.
fn range_label(lines: &[ReviewLine]) -> String {
    let (Some(first), Some(last)) = (lines.first(), lines.last()) else {
        return "line".into();
    };
    let (Some(first_number), Some(last_number)) = (
        first.new_number.or(first.old_number),
        last.new_number.or(last.old_number),
    ) else {
        return if lines.len() == 1 {
            "line".into()
        } else {
            format!("{} lines", lines.len())
        };
    };
    let marker = if first.change != Change::Context
        && lines.iter().all(|line| line.change == first.change)
    {
        first.change.marker()
    } else {
        ""
    };
    if first_number == last_number {
        format!("{marker}{first_number}")
    } else {
        format!("{marker}{first_number} to {marker}{last_number}")
    }
}

/// Who and where a new comment is for.
#[derive(Clone, Copy, Debug)]
pub struct CommentTarget<'a> {
    pub id: &'a str,
    pub section_id: &'a str,
    pub section_title: &'a str,
    pub file_path: &'a str,
    pub file: &'a FileDiff,
}

/// Builds the comment for `range` of `target.file` (`buildDiffReviewComment`), or `None` when
/// an end of the range is not a line of the file. `text` is trimmed.
pub fn build_comment(target: CommentTarget, range: LineRange, text: &str) -> Option<ReviewComment> {
    let lines = review_lines(target.file);
    let start = line_index(&lines, range.start, range.side)?;
    let end = line_index(&lines, range.end, range.end_side)?;
    let (start, end) = (start.min(end), start.max(end));
    let selected = &lines[start..=end];
    let (old_start, old_count) = hunk_range(selected, Side::Old);
    let (new_start, new_count) = hunk_range(selected, Side::New);
    let mut diff = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@");
    for line in selected {
        diff.push('\n');
        diff.push_str(line.change.marker());
        diff.push_str(line.content);
    }
    Some(ReviewComment {
        id: target.id.to_owned(),
        section_id: target.section_id.to_owned(),
        section_title: target.section_title.to_owned(),
        file_path: target.file_path.to_owned(),
        start_index: start,
        end_index: end,
        range_label: range_label(selected),
        text: js_trim(text).to_owned(),
        diff,
        fence_language: Some("diff".into()),
    })
}

/// The range a stored comment covers in `file` (`restoreDiffReviewCommentRange`), or `None`
/// when its indices no longer exist.
pub fn restore_range(file: &FileDiff, comment: &ReviewComment) -> Option<LineRange> {
    let lines = review_lines(file);
    let (start, side) = lines.get(comment.start_index)?.selection_point()?;
    let (end, end_side) = lines.get(comment.end_index)?.selection_point()?;
    Some(LineRange {
        start,
        side,
        end,
        end_side,
    })
}

/// A fenced code block that no backtick run in `contents` can close early
/// (`formatReviewCommentFence`).
pub fn format_fence(language: &str, contents: &str) -> String {
    let longest_run = contents
        .split(|ch| ch != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((longest_run + 1).max(3));
    format!("{fence}{language}\n{}\n{fence}", js_trim_end(contents))
}

/// Escapes an attribute value; `&` first so the other entities stay intact.
fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The `<review_comment>` block sent to the agent (`formatReviewCommentContext`).
pub fn format_review_comment(comment: &ReviewComment) -> String {
    format!(
        "<review_comment sectionId=\"{}\" sectionTitle=\"{}\" filePath=\"{}\" startIndex=\"{}\" endIndex=\"{}\" rangeLabel=\"{}\">\n{}\n{}\n</review_comment>",
        escape_attribute(&comment.section_id),
        escape_attribute(&comment.section_title),
        escape_attribute(&comment.file_path),
        comment.start_index,
        comment.end_index,
        escape_attribute(&comment.range_label),
        js_trim(&comment.text),
        format_fence(
            comment.fence_language.as_deref().unwrap_or("diff"),
            &comment.diff
        ),
    )
}

/// `prompt` (trimmed) followed by one block per comment, separated by blank lines. Without
/// comments the prompt is returned untouched (`appendReviewCommentsToPrompt`).
pub fn append_review_comments_to_prompt(prompt: &str, comments: &[ReviewComment]) -> String {
    if comments.is_empty() {
        return prompt.to_owned();
    }
    let blocks = comments
        .iter()
        .map(format_review_comment)
        .collect::<Vec<_>>()
        .join("\n\n");
    let prompt = js_trim(prompt);
    if prompt.is_empty() {
        blocks
    } else {
        format!("{prompt}\n\n{blocks}")
    }
}

/// A fresh comment id, `file-comment-{unix millis}-{sequence}` (`nextFileCommentId`).
pub fn next_comment_id() -> String {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    format!("file-comment-{millis}-{sequence}")
}

/// JavaScript's whitespace set for `String.prototype.trim`: `WhiteSpace` (tab, VT, FF, BOM and
/// the `Zs` category) plus `LineTerminator`. It differs from Rust's `char::is_whitespace`
/// (which has U+0085 and lacks U+FEFF).
fn is_js_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
}

fn js_trim_end(value: &str) -> &str {
    value.trim_end_matches(is_js_whitespace)
}

#[cfg(test)]
mod tests {
    //! Failure modes, each checked against outputs of the web client's own functions
    //! (`reviewCommentContext.ts` run in node on `fixtures/multi-file.patch`):
    //! - the mini-hunk header counts lines without a number on that side, or starts at the
    //!   wrong line (`-0,0` when the selection has no old lines);
    //! - the range label loses its `+`/`-` marker for uniform changes, keeps it for mixed or
    //!   context selections, or prints `a to a` for one line;
    //! - a line number is looked up on the wrong side, or the fallback to the other side's
    //!   numbers is missing (old-side context lines then fail or label by the old number);
    //! - a reversed selection is not normalized, or an out-of-file line yields a comment;
    //! - restoring a comment picks the old number for context lines;
    //! - serialization escapes `&` after the other entities (double escaping), sizes the fence
    //!   below the longest backtick run, keeps trailing whitespace of the diff, or trims with
    //!   Rust's whitespace set instead of JavaScript's;
    //! - prompt assembly trims a prompt that has no comments, or adds blank lines around an
    //!   empty prompt.
    use super::*;
    use crate::patch::{RenderablePatch, renderable_patch};

    fn file(path: &str) -> FileDiff {
        let RenderablePatch::Files(files) =
            renderable_patch(include_str!("../fixtures/multi-file.patch"))
        else {
            panic!("fixture parses")
        };
        files
            .into_iter()
            .find(|file| file.path == path)
            .expect("file in fixture")
    }

    const CHAT: &str = "apps/web/src/components/ChatView.tsx";
    const STATS: &str = "apps/web/src/lib/diffStats.ts";
    const LEGACY: &str = "apps/server/src/legacy/checkpointPaths.ts";
    const README: &str = "README.md";

    use Side::{New as Additions, Old as Deletions};

    struct Expected {
        start_index: usize,
        end_index: usize,
        range_label: &'static str,
        diff: &'static str,
        restored: (u32, Side, u32, Side),
    }

    struct Case {
        path: &'static str,
        start: (u32, Side),
        end: (u32, Side),
        expected: Option<Expected>,
    }

    /// Generated by `buildDiffReviewComment` + `restoreDiffReviewCommentRange`.
    fn cases() -> Vec<Case> {
        vec![
            Case {
                path: CHAT,
                start: (15, Additions),
                end: (15, Additions),
                expected: Some(Expected {
                    start_index: 4,
                    end_index: 4,
                    range_label: "+15",
                    diff: "@@ -0,0 +15,1 @@\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";",
                    restored: (15, Additions, 15, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (15, Deletions),
                end: (15, Deletions),
                expected: Some(Expected {
                    start_index: 3,
                    end_index: 3,
                    range_label: "-15",
                    diff: "@@ -15,1 +0,0 @@\n-import { DiffPanel } from \"./DiffPanel\";",
                    restored: (15, Deletions, 15, Deletions),
                }),
            },
            Case {
                path: CHAT,
                start: (15, Additions),
                end: (16, Additions),
                expected: Some(Expected {
                    start_index: 4,
                    end_index: 5,
                    range_label: "+15 to +16",
                    diff: "@@ -0,0 +15,2 @@\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";\n+import { useRightPanelStore } from \"../rightPanelStore\";",
                    restored: (15, Additions, 16, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (12, Additions),
                end: (19, Additions),
                expected: Some(Expected {
                    start_index: 0,
                    end_index: 8,
                    range_label: "12 to 19",
                    diff: "@@ -12,7 +12,8 @@\n import { ThreadHeader } from \"./ThreadHeader\";\n import { Timeline } from \"./timeline/Timeline\";\n import { Composer } from \"./composer/Composer\";\n-import { DiffPanel } from \"./DiffPanel\";\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";\n+import { useRightPanelStore } from \"../rightPanelStore\";\n import { cn } from \"~/lib/utils\";\n \n export interface ChatViewProps {",
                    restored: (12, Additions, 19, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (15, Deletions),
                end: (16, Additions),
                expected: Some(Expected {
                    start_index: 3,
                    end_index: 5,
                    range_label: "15 to 16",
                    diff: "@@ -15,1 +15,2 @@\n-import { DiffPanel } from \"./DiffPanel\";\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";\n+import { useRightPanelStore } from \"../rightPanelStore\";",
                    restored: (15, Deletions, 16, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (16, Additions),
                end: (14, Additions),
                expected: Some(Expected {
                    start_index: 2,
                    end_index: 5,
                    range_label: "14 to 16",
                    diff: "@@ -14,2 +14,3 @@\n import { Composer } from \"./composer/Composer\";\n-import { DiffPanel } from \"./DiffPanel\";\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";\n+import { useRightPanelStore } from \"../rightPanelStore\";",
                    restored: (14, Additions, 16, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (143, Deletions),
                end: (145, Deletions),
                expected: Some(Expected {
                    start_index: 12,
                    end_index: 14,
                    range_label: "-143 to -145",
                    diff: "@@ -143,3 +0,0 @@\n-  const isDiffOpen = panel?.activeSurfaceId === \"diff\";\n-  const isPlanOpen = panel?.activeSurfaceId === \"plan\";\n-  const isFilesOpen = panel?.activeSurfaceId === \"files\";",
                    restored: (143, Deletions, 145, Deletions),
                }),
            },
            Case {
                path: CHAT,
                start: (14, Additions),
                end: (144, Additions),
                expected: Some(Expected {
                    start_index: 2,
                    end_index: 15,
                    range_label: "14 to 144",
                    diff: "@@ -14,11 +14,10 @@\n import { Composer } from \"./composer/Composer\";\n-import { DiffPanel } from \"./DiffPanel\";\n+import { DiffPanel, DiffWorkerPoolProvider } from \"./DiffPanel\";\n+import { useRightPanelStore } from \"../rightPanelStore\";\n import { cn } from \"~/lib/utils\";\n \n export interface ChatViewProps {\n   const thread = useThread(threadId);\n   const panel = useRightPanelStore((state) => state.byThread[threadId]);\n \n-  const isDiffOpen = panel?.activeSurfaceId === \"diff\";\n-  const isPlanOpen = panel?.activeSurfaceId === \"plan\";\n-  const isFilesOpen = panel?.activeSurfaceId === \"files\";\n+  const activeSurface = panel?.isOpen ? panel.activeSurfaceId : null;",
                    restored: (14, Additions, 144, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (141, Deletions),
                end: (141, Deletions),
                expected: Some(Expected {
                    start_index: 10,
                    end_index: 10,
                    range_label: "142",
                    diff: "@@ -141,1 +142,1 @@\n   const panel = useRightPanelStore((state) => state.byThread[threadId]);",
                    restored: (142, Additions, 142, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (206, Deletions),
                end: (206, Deletions),
                expected: Some(Expected {
                    start_index: 28,
                    end_index: 28,
                    range_label: "206",
                    diff: "@@ -205,1 +206,1 @@\n }",
                    restored: (206, Additions, 206, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (140, Additions),
                end: (140, Additions),
                expected: Some(Expected {
                    start_index: 9,
                    end_index: 9,
                    range_label: "141",
                    diff: "@@ -140,1 +141,1 @@\n   const thread = useThread(threadId);",
                    restored: (141, Additions, 141, Additions),
                }),
            },
            Case {
                path: CHAT,
                start: (999, Additions),
                end: (999, Additions),
                expected: None,
            },
            Case {
                path: CHAT,
                start: (145, Deletions),
                end: (143, Deletions),
                expected: Some(Expected {
                    start_index: 12,
                    end_index: 14,
                    range_label: "-143 to -145",
                    diff: "@@ -143,3 +0,0 @@\n-  const isDiffOpen = panel?.activeSurfaceId === \"diff\";\n-  const isPlanOpen = panel?.activeSurfaceId === \"plan\";\n-  const isFilesOpen = panel?.activeSurfaceId === \"files\";",
                    restored: (143, Deletions, 145, Deletions),
                }),
            },
            Case {
                path: CHAT,
                start: (200, Additions),
                end: (204, Additions),
                expected: Some(Expected {
                    start_index: 22,
                    end_index: 26,
                    range_label: "+200 to +204",
                    diff: "@@ -0,0 +200,5 @@\n+    {activeSurface === \"diff\" ? (\n+      <DiffWorkerPoolProvider>\n+        <DiffPanel mode=\"embedded\" composerDraftTarget={{ environmentId: thread.environmentId, threadId: thread.id }} />\n+      </DiffWorkerPoolProvider>\n+    ) : null}",
                    restored: (200, Additions, 204, Additions),
                }),
            },
            Case {
                path: STATS,
                start: (1, Additions),
                end: (14, Additions),
                expected: Some(Expected {
                    start_index: 0,
                    end_index: 13,
                    range_label: "+1 to +14",
                    diff: "@@ -0,0 +1,14 @@\n+export interface DiffStat {\n+  additions: number;\n+  deletions: number;\n+}\n+\n+export function sumDiffStats(stats: ReadonlyArray<DiffStat>): DiffStat {\n+  return stats.reduce(\n+    (total, stat) => ({\n+      additions: total.additions + stat.additions,\n+      deletions: total.deletions + stat.deletions,\n+    }),\n+    { additions: 0, deletions: 0 },\n+  );\n+}",
                    restored: (1, Additions, 14, Additions),
                }),
            },
            Case {
                path: STATS,
                start: (14, Additions),
                end: (14, Additions),
                expected: Some(Expected {
                    start_index: 13,
                    end_index: 13,
                    range_label: "+14",
                    diff: "@@ -0,0 +14,1 @@\n+}",
                    restored: (14, Additions, 14, Additions),
                }),
            },
            Case {
                path: LEGACY,
                start: (2, Deletions),
                end: (2, Deletions),
                expected: Some(Expected {
                    start_index: 1,
                    end_index: 1,
                    range_label: "-2",
                    diff: "@@ -2,1 +0,0 @@\n-",
                    restored: (2, Deletions, 2, Deletions),
                }),
            },
            Case {
                path: LEGACY,
                start: (1, Deletions),
                end: (7, Deletions),
                expected: Some(Expected {
                    start_index: 0,
                    end_index: 6,
                    range_label: "-1 to -7",
                    diff: "@@ -1,7 +0,0 @@\n-import path from \"node:path\";\n-\n-export function checkpointDir(root: string, threadId: string): string {\n-  return path.join(root, \".t3\", \"checkpoints\", threadId);\n-}\n-\n-export const CHECKPOINT_REF_PREFIX = \"refs/t3/checkpoints\";",
                    restored: (1, Deletions, 7, Deletions),
                }),
            },
            Case {
                path: README,
                start: (3, Deletions),
                end: (3, Additions),
                expected: Some(Expected {
                    start_index: 2,
                    end_index: 3,
                    range_label: "3",
                    diff: "@@ -3,1 +3,1 @@\n-T3 Code is a minimal web GUI for coding agents.\n+T3 Code is a minimal desktop and web GUI for coding agents like Codex and Claude.",
                    restored: (3, Deletions, 3, Additions),
                }),
            },
            Case {
                path: README,
                start: (5, Additions),
                end: (1, Deletions),
                expected: Some(Expected {
                    start_index: 0,
                    end_index: 5,
                    range_label: "1 to 5",
                    diff: "@@ -1,5 +1,5 @@\n # T3 Code\n \n-T3 Code is a minimal web GUI for coding agents.\n+T3 Code is a minimal desktop and web GUI for coding agents like Codex and Claude.\n \n ## Getting started",
                    restored: (1, Additions, 5, Additions),
                }),
            },
        ]
    }

    #[test]
    fn comments_match_the_web_client() {
        for case in cases() {
            let file = file(case.path);
            let range = LineRange {
                start: case.start.0,
                side: case.start.1,
                end: case.end.0,
                end_side: case.end.1,
            };
            let target = CommentTarget {
                id: "c",
                section_id: "branch",
                section_title: "Branch changes",
                file_path: case.path,
                file: &file,
            };
            let comment = build_comment(target, range, "  note  ");
            let label = format!("{} {range:?}", case.path);
            let Some(expected) = case.expected else {
                assert_eq!(comment, None, "{label}");
                continue;
            };
            let comment = comment.unwrap_or_else(|| panic!("{label}: no comment"));
            assert_eq!(comment.start_index, expected.start_index, "{label}");
            assert_eq!(comment.end_index, expected.end_index, "{label}");
            assert_eq!(comment.range_label, expected.range_label, "{label}");
            assert_eq!(comment.diff, expected.diff, "{label}");
            assert_eq!(comment.text, "note");
            assert_eq!(comment.fence_language.as_deref(), Some("diff"));
            let (start, side, end, end_side) = expected.restored;
            assert_eq!(
                restore_range(&file, &comment),
                Some(LineRange {
                    start,
                    side,
                    end,
                    end_side
                }),
                "{label}"
            );
        }
    }

    #[test]
    fn restore_fails_for_missing_indices() {
        let file = file(README);
        let target = CommentTarget {
            id: "c",
            section_id: "branch",
            section_title: "Branch changes",
            file_path: README,
            file: &file,
        };
        let mut comment = build_comment(target, LineRange::line(1, Additions), "x").unwrap();
        comment.end_index = 99;
        assert_eq!(restore_range(&file, &comment), None);
    }

    /// The comment the serialization cases share: attributes needing every escape, text with
    /// surrounding whitespace, and a diff with a 4-backtick run and trailing blank lines.
    fn tricky() -> ReviewComment {
        ReviewComment {
            id: "c1".into(),
            section_id: "turn:a&b".into(),
            section_title: "Turn \"3\" <x>".into(),
            file_path: "src/a&b <c>.ts".into(),
            start_index: 4,
            end_index: 9,
            range_label: "+12 to +15".into(),
            text: "  Rename this\n  please \n".into(),
            diff: "@@ -1,1 +1,1 @@\n-a ``` b\n+a ```` b\n\n".into(),
            fence_language: None,
        }
    }

    fn plain() -> ReviewComment {
        ReviewComment {
            id: "c2".into(),
            file_path: "README.md".into(),
            range_label: "-3".into(),
            text: "x".into(),
            diff: "@@ -3,1 +0,0 @@\n-y".into(),
            fence_language: Some("diff".into()),
            ..tricky()
        }
    }

    const TRICKY_BLOCK: &str = "<review_comment sectionId=\"turn:a&amp;b\" sectionTitle=\"Turn &quot;3&quot; &lt;x&gt;\" filePath=\"src/a&amp;b &lt;c&gt;.ts\" startIndex=\"4\" endIndex=\"9\" rangeLabel=\"+12 to +15\">\nRename this\n  please\n`````diff\n@@ -1,1 +1,1 @@\n-a ``` b\n+a ```` b\n`````\n</review_comment>";
    const PLAIN_BLOCK: &str = "<review_comment sectionId=\"turn:a&amp;b\" sectionTitle=\"Turn &quot;3&quot; &lt;x&gt;\" filePath=\"README.md\" startIndex=\"4\" endIndex=\"9\" rangeLabel=\"-3\">\nx\n```diff\n@@ -3,1 +0,0 @@\n-y\n```\n</review_comment>";

    #[test]
    fn blocks_match_the_web_client() {
        assert_eq!(format_review_comment(&tricky()), TRICKY_BLOCK);
        assert_eq!(format_review_comment(&plain()), PLAIN_BLOCK);
        let typescript = ReviewComment {
            fence_language: Some("ts".into()),
            diff: "const a = `b`;".into(),
            ..plain()
        };
        assert_eq!(
            format_review_comment(&typescript),
            "<review_comment sectionId=\"turn:a&amp;b\" sectionTitle=\"Turn &quot;3&quot; &lt;x&gt;\" filePath=\"README.md\" startIndex=\"4\" endIndex=\"9\" rangeLabel=\"-3\">\nx\n```ts\nconst a = `b`;\n```\n</review_comment>"
        );
    }

    #[test]
    fn prompts_match_the_web_client() {
        assert_eq!(
            append_review_comments_to_prompt("  hello  ", &[tricky(), plain()]),
            format!("hello\n\n{TRICKY_BLOCK}\n\n{PLAIN_BLOCK}")
        );
        assert_eq!(
            append_review_comments_to_prompt("  \n ", &[plain()]),
            PLAIN_BLOCK
        );
        assert_eq!(
            append_review_comments_to_prompt("  hello  ", &[]),
            "  hello  "
        );
    }

    #[test]
    fn trims_like_javascript() {
        // BOM and ideographic space are JavaScript whitespace; NEL (U+0085) is not.
        assert_eq!(js_trim("\u{feff}\u{3000} a \u{2028}"), "a");
        assert_eq!(js_trim("\u{85}a\u{85}"), "\u{85}a\u{85}");
        assert_eq!(format_fence("diff", "x\u{feff}\n"), "```diff\nx\n```");
    }

    #[test]
    fn comments_belong_to_their_section_and_file() {
        let comment = plain();
        assert!(comment.belongs_to("turn:a&b", "README.md"));
        assert!(!comment.belongs_to("branch", "README.md"));
        assert!(!comment.belongs_to("turn:a&b", "other.md"));
        let file_comment = ReviewComment {
            fence_language: Some("ts".into()),
            ..plain()
        };
        assert!(!file_comment.belongs_to("turn:a&b", "README.md"));
        let legacy = ReviewComment {
            fence_language: None,
            ..plain()
        };
        assert!(legacy.belongs_to("turn:a&b", "README.md"));
    }

    #[test]
    fn row_points_follow_rows() {
        use crate::rows::{DiffStyle, build_rows};
        let chat = file(CHAT);
        let unified = build_rows(&chat, DiffStyle::Unified);
        let points = row_points(&chat, &unified);
        // Row 0 is the separator, then context 12..14, deletion 15, additions 15 and 16.
        assert_eq!(points[0], RowPoints::default());
        assert_eq!(
            points[1],
            RowPoints {
                old: Some(12),
                new: Some(12)
            }
        );
        assert_eq!(row_for_point(&points, 15, Deletions), Some(4));
        assert_eq!(row_for_point(&points, 15, Additions), Some(5));
        // Old-side numbers find unified context rows too (line 141 shows new number 142).
        assert_eq!(
            row_for_point(&points, 141, Deletions),
            row_for_point(&points, 142, Additions)
        );
        assert_eq!(row_for_point(&points, 999, Additions), None);

        let split = build_rows(&chat, DiffStyle::Split);
        let points = row_points(&chat, &split);
        let row = row_for_point(&points, 15, Deletions).unwrap();
        assert_eq!(row_for_point(&points, 15, Additions), Some(row));
        // The lopsided change: deletion 145 has no addition beside it.
        let row = row_for_point(&points, 145, Deletions).unwrap();
        assert_eq!(points[row].new, None);
    }

    #[test]
    fn ids_are_unique() {
        let first = next_comment_id();
        let second = next_comment_id();
        assert!(first.starts_with("file-comment-"));
        assert_ne!(first, second);
    }
}
