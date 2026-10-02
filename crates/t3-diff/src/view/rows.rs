//! Painting of a file card's body rows: line numbers with change bars, code with syntax colors
//! and word emphasis, "N unmodified lines" separators, no-newline markers, and the hatched
//! buffers that pad the short side of a split change.

use std::ops::Range;

use gpui_kit::{
    AnyElement, Bounds, ContentMask, Context, FontStyle, FontWeight, HighlightStyle, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Path, Pixels, ScrollWheelEvent,
    SharedString, Styled as _, StyledText, UnderlineStyle, Window, canvas, div, fill, point,
    prelude::FluentBuilder as _, px, rgba,
};

use super::{
    DiffView,
    highlight::{FileHighlights, Span},
    style::{
        BAR_WIDTH, CODE_SIZE, GUTTER_BORDER, LINE_HEIGHT, Metrics, SANS, SEPARATOR_HEIGHT,
        SEPARATOR_INSET, SEPARATOR_MARGIN, SEPARATOR_RADIUS, TAB_SIZE, gpui,
    },
};
use crate::{
    color::Rgba,
    palette::{DiffPalette, LineColors},
    patch::FileDiff,
    rows::{LineCell, LineKind, Row, Side},
};

/// Text of the no-newline marker row.
const NO_NEWLINE: &str = "No newline at end of file";

/// Paints the body rows of one file. Built per frame by [`DiffView`].
pub(super) struct RowPainter<'a> {
    pub file: &'a FileDiff,
    pub file_ix: usize,
    pub rows: &'a [Row],
    pub highlights: Option<&'a FileHighlights>,
    pub palette: &'a DiffPalette,
    pub metrics: Metrics,
    pub mono: SharedString,
    /// Line-number column width without its 2px border.
    pub gutter_width: Pixels,
    pub wrap: bool,
    pub two_column: bool,
    /// Horizontal scroll of the old and new code columns (unified uses the new one).
    pub scroll_x: [Pixels; 2],
}

impl RowPainter<'_> {
    pub fn render(&self, row_ix: usize, cx: &mut Context<DiffView>) -> AnyElement {
        let Some(&row) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let body = div()
            .w_full()
            .bg(gpui(self.palette.surface))
            .font_family(self.mono.clone())
            .text_size(CODE_SIZE)
            .line_height(LINE_HEIGHT)
            .text_color(gpui(self.palette.text));
        match row {
            Row::Separator { lines, first } => body.child(self.separator(lines, first)),
            Row::Line(cell) => body.child(self.line_half(Some(cell), Side::New, row_ix, cx)),
            Row::NoNewline(kind) => body.child(self.no_newline_half(Some(kind), Side::New, row_ix)),
            Row::Split { old, new } => body.child(self.split(
                self.line_half(old, Side::Old, row_ix, cx),
                self.line_half(new, Side::New, row_ix, cx),
            )),
            Row::SplitNoNewline { old, new } => body.child(self.split(
                self.no_newline_half(old, Side::Old, row_ix),
                self.no_newline_half(new, Side::New, row_ix),
            )),
        }
        .into_any_element()
    }

    /// Two equal columns split by `1px + 1px` of the code background.
    fn split(&self, old: AnyElement, new: AnyElement) -> impl IntoElement {
        let surface = gpui(self.palette.surface);
        div()
            .flex()
            .flex_row()
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .border_r_1()
                    .border_color(surface)
                    .child(old),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .border_l_1()
                    .border_color(surface)
                    .child(new),
            )
    }

    /// One line (or an empty buffer when `cell` is `None`): gutter plus code.
    fn line_half(
        &self,
        cell: Option<LineCell>,
        side: Side,
        row_ix: usize,
        cx: &mut Context<DiffView>,
    ) -> AnyElement {
        let Some(cell) = cell else {
            return self.buffer(side, row_ix).into_any_element();
        };
        let colors = self.palette.line(cell.kind);
        let group: SharedString = format!("diff-line-{}", side_index(side)).into();
        let (text, highlights) = self.line_text(cell);
        let offset = self.scroll_x[side_index(side)];
        let file_ix = self.file_ix;
        let code = div()
            .flex_1()
            .min_w_0()
            .min_h(LINE_HEIGHT)
            .bg(gpui(colors.code))
            .group_hover(group.clone(), |code| code.bg(gpui(colors.code_hover)))
            .child(
                div()
                    .px(self.metrics.code_ch)
                    .when(!self.wrap, |text| text.whitespace_nowrap().ml(-offset))
                    .child(StyledText::new(text).with_highlights(highlights)),
            )
            .when(!self.wrap, |code| {
                code.overflow_hidden().on_scroll_wheel(cx.listener(
                    move |this, event: &ScrollWheelEvent, _, cx| {
                        this.on_row_scroll(file_ix, side, event, cx);
                    },
                ))
            });
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .group(group.clone())
            .child(self.gutter(Some(cell.number), Some(cell.kind), colors, Some(group)))
            .child(code)
            .into_any_element()
    }

    /// A "No newline at end of file" marker tinted like `kind`, or a buffer for `None`.
    fn no_newline_half(&self, kind: Option<LineKind>, side: Side, row_ix: usize) -> AnyElement {
        let Some(kind) = kind else {
            return self.buffer(side, row_ix).into_any_element();
        };
        let colors = self.palette.line(kind);
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .child(self.gutter(None, None, colors, None))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(LINE_HEIGHT)
                    .px(self.metrics.code_ch)
                    .bg(gpui(colors.code))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().opacity(0.6).child(NO_NEWLINE)),
            )
            .into_any_element()
    }

    /// The line-number cell with its change bar and 2px right border.
    fn gutter(
        &self,
        number: Option<u32>,
        bar: Option<LineKind>,
        colors: LineColors,
        hover_group: Option<SharedString>,
    ) -> impl IntoElement {
        let palette = self.palette;
        div()
            .relative()
            .flex()
            .flex_none()
            .justify_end()
            .w(self.gutter_width + GUTTER_BORDER)
            .min_h(LINE_HEIGHT)
            .pl(self.metrics.code_ch * 2.)
            .pr(self.metrics.code_ch)
            .bg(gpui(colors.number))
            .border_r(GUTTER_BORDER)
            .border_color(gpui(palette.surface))
            .text_color(gpui(colors.number_text))
            .when_some(hover_group, |gutter, group| {
                gutter.group_hover(group, |gutter| gutter.bg(gpui(colors.number_hover)))
            })
            .children(number.map(|number| number.to_string()))
            .map(|gutter| match bar {
                Some(LineKind::Addition) => gutter.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(BAR_WIDTH)
                        .bg(gpui(palette.addition)),
                ),
                Some(LineKind::Deletion) => gutter.child(dashed_bar(palette)),
                _ => gutter,
            })
    }

    /// The hatched filler for the empty side of a split change.
    fn buffer(&self, side: Side, row_ix: usize) -> impl IntoElement {
        let palette = *self.palette;
        let run_offset = self.buffer_run_offset(side, row_ix);
        div()
            .flex()
            .flex_row()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .flex_none()
                    .w(self.gutter_width + GUTTER_BORDER)
                    .h(LINE_HEIGHT)
                    .bg(gpui(palette.buffer_gutter))
                    .border_r(GUTTER_BORDER)
                    .border_color(gpui(palette.surface)),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint_hatch(bounds, run_offset, palette, window);
                    },
                )
                .flex_1()
                .h(LINE_HEIGHT),
            )
    }

    /// Rows since the start of this buffer run, so the hatch continues across rows like the
    /// single tall buffer element Pierre draws.
    fn buffer_run_offset(&self, side: Side, row_ix: usize) -> usize {
        self.rows[..row_ix]
            .iter()
            .rev()
            .take_while(|row| {
                matches!(
                    (row, side),
                    (Row::Split { old: None, .. }, Side::Old)
                        | (Row::Split { new: None, .. }, Side::New)
                )
            })
            .count()
    }

    /// "N unmodified lines": a rounded pill inset 8px, across both columns in split view.
    fn separator(&self, lines: u32, first: bool) -> impl IntoElement {
        let palette = self.palette;
        let label = format!(
            "{lines} unmodified line{}",
            if lines == 1 { "" } else { "s" }
        );
        let pill = |label: Option<String>| {
            div()
                .flex()
                .items_center()
                .h(SEPARATOR_HEIGHT)
                .px(self.metrics.separator_ch)
                .bg(gpui(palette.separator))
                .font_family(SANS)
                .text_color(gpui(palette.separator_text))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .children(label)
        };
        let wrapper = div()
            .w_full()
            .pt(if first { px(0.) } else { SEPARATOR_MARGIN })
            .pb(SEPARATOR_MARGIN);
        if self.two_column {
            // Each column's pill ends at the 2px split, rounded only at the outer ends.
            let surface = gpui(palette.surface);
            wrapper.child(
                div()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pl(SEPARATOR_INSET)
                            .border_r_1()
                            .border_color(surface)
                            .child(pill(Some(label)).rounded_l(SEPARATOR_RADIUS)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .pr(SEPARATOR_INSET)
                            .border_l_1()
                            .border_color(surface)
                            .child(pill(None).rounded_r(SEPARATOR_RADIUS)),
                    ),
            )
        } else {
            wrapper
                .px(SEPARATOR_INSET)
                .child(pill(Some(label)).rounded(SEPARATOR_RADIUS))
        }
    }

    /// The line's display text (tabs expanded) and its syntax and emphasis highlights.
    fn line_text(&self, cell: LineCell) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
        let text = cell.text(self.file);
        let (spans, emphasis): (&[Span], &[Range<usize>]) = match self.highlights {
            Some(highlights) => {
                let (spans, emphasis) = match cell.side {
                    Side::Old => (&highlights.old, &highlights.old_emphasis),
                    Side::New => (&highlights.new, &highlights.new_emphasis),
                };
                (
                    spans.get(cell.index).map_or(&[], Vec::as_slice),
                    emphasis.get(cell.index).map_or(&[], Vec::as_slice),
                )
            }
            None => (&[], &[]),
        };
        let emphasis_color = match cell.kind {
            LineKind::Addition => Some(self.palette.addition_emphasis),
            LineKind::Deletion => Some(self.palette.deletion_emphasis),
            LineKind::Context => None,
        };
        let highlights = merge_highlights(text.len(), spans, emphasis, emphasis_color);
        expand_tabs(text, highlights)
    }
}

fn side_index(side: Side) -> usize {
    match side {
        Side::Old => 0,
        Side::New => 1,
    }
}

/// Splits syntax spans and emphasis ranges into ordered, non-overlapping highlights.
fn merge_highlights(
    len: usize,
    spans: &[Span],
    emphasis: &[Range<usize>],
    emphasis_color: Option<Rgba>,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut cuts: Vec<usize> = spans
        .iter()
        .flat_map(|span| [span.start, span.end])
        .chain(emphasis.iter().flat_map(|range| [range.start, range.end]))
        .filter(|&cut| cut <= len)
        .collect();
    cuts.push(0);
    cuts.push(len);
    cuts.sort_unstable();
    cuts.dedup();
    let background = emphasis_color.map(|color| Hsla::from(gpui(color)));
    cuts.windows(2)
        .filter_map(|window| {
            let range = window[0]..window[1];
            let syntax = spans
                .iter()
                .find(|span| span.start <= range.start && range.end <= span.end);
            let emphasized = emphasis
                .iter()
                .any(|emphasis| emphasis.start <= range.start && range.end <= emphasis.end);
            let mut style = HighlightStyle::default();
            if let Some(span) = syntax {
                style.color = Some(rgba(span.style.color).into());
                if span.style.bold {
                    style.font_weight = Some(FontWeight::BOLD);
                }
                if span.style.italic {
                    style.font_style = Some(FontStyle::Italic);
                }
                if span.style.underline {
                    style.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        ..UnderlineStyle::default()
                    });
                }
            }
            if emphasized {
                style.background_color = background;
            }
            (style != HighlightStyle::default()).then_some((range, style))
        })
        .collect()
}

/// Expands tabs to the next multiple of [`TAB_SIZE`] columns, remapping highlight ranges.
fn expand_tabs(
    text: &str,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
    if !text.contains('\t') {
        return (SharedString::from(text.to_owned()), highlights);
    }
    let mut expanded = String::with_capacity(text.len() + 8);
    // (original byte offset, expanded byte offset) at every char start, plus the end.
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut column = 0;
    for (ix, ch) in text.char_indices() {
        map.push((ix, expanded.len()));
        if ch == '\t' {
            let spaces = TAB_SIZE - column % TAB_SIZE;
            expanded.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            expanded.push(ch);
            column += 1;
        }
    }
    map.push((text.len(), expanded.len()));
    let translate = |offset: usize| {
        map.binary_search_by_key(&offset, |&(original, _)| original)
            .map_or(expanded.len(), |found| map[found].1)
    };
    let highlights = highlights
        .into_iter()
        .map(|(range, style)| (translate(range.start)..translate(range.end), style))
        .collect();
    (expanded.into(), highlights)
}

/// Width of a line in columns once tabs are expanded.
pub(super) fn display_width(line: &str) -> usize {
    line.chars().fold(0, |column, ch| {
        if ch == '\t' {
            column + TAB_SIZE - column % TAB_SIZE
        } else {
            column + 1
        }
    })
}

/// The deletion change bar: 1px stripes of the deletion color every 2px over the light half
/// (`linear-gradient(0deg, bg-deletion 50%, deletion-base 50%)` at `2px 2px`).
fn dashed_bar(palette: &DiffPalette) -> impl IntoElement {
    let (stripe, gap) = (gpui(palette.deletion), gpui(palette.deletion_bar_gap));
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(fill(bounds, gap));
            let mut y = px(0.);
            while y < bounds.size.height {
                let stripe_bounds = Bounds::new(
                    point(bounds.left(), bounds.top() + y),
                    gpui_kit::size(bounds.size.width, px(1.)),
                );
                window.paint_quad(fill(stripe_bounds, stripe));
                y += px(2.);
            }
        },
    )
    .absolute()
    .left_0()
    .top_0()
    .bottom_0()
    .w(BAR_WIDTH)
}

/// Pierre's `[data-content-buffer]`: `repeating-linear-gradient(-45deg, transparent 0 4.242px,
/// buffer 4.242px 5.656px)` in an 8px tile at `5px 0`. That is a 2px-wide `/` stripe wherever
/// `(x - 5 + y) mod 8` falls in `(0, 2]`, measured from the top of the buffer run.
fn paint_hatch(
    bounds: Bounds<Pixels>,
    run_offset: usize,
    palette: DiffPalette,
    window: &mut Window,
) {
    window.paint_quad(fill(bounds, gpui(palette.surface)));
    let phase = (run_offset as f32 * f32::from(LINE_HEIGHT)) % 8.;
    let height = f32::from(bounds.size.height);
    let width = f32::from(bounds.size.width);
    let (left, top) = (f32::from(bounds.left()), f32::from(bounds.top()));
    let mut path: Option<Path<Pixels>> = None;
    // Stripe k covers (x' + y') in (8k, 8k + 2], with x' = x - left - 5 and y' = y - top + phase.
    let first = ((-5. + phase) / 8.).floor() as i32 - 1;
    let last = ((width - 5. + height + phase) / 8.).ceil() as i32 + 1;
    for k in first..=last {
        let base = 8. * k as f32;
        // x at the top (y' = phase) and the bottom (y' = phase + height) of each edge line.
        let x_at = |sum: f32, y_rel: f32| left + 5. + sum - y_rel;
        let corners = [
            point(px(x_at(base, phase)), px(top)),
            point(px(x_at(base + 2., phase)), px(top)),
            point(px(x_at(base + 2., phase + height)), px(top + height)),
            point(px(x_at(base, phase + height)), px(top + height)),
        ];
        let path = path.get_or_insert_with(|| Path::new(corners[0]));
        path.move_to(corners[0]);
        for corner in &corners[1..] {
            path.line_to(*corner);
        }
    }
    if let Some(path) = path {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_path(path, gpui(palette.buffer_stripe));
        });
    }
}
