//! `InlineText`: one block of inline content laid out like CSS inline formatting.
//!
//! GPUI's `StyledText` lays out a single font size, so it cannot reproduce 12px inline code
//! inside 14px text, inline-code boxes, or chips that flow with the text. This element does:
//!
//! - Text is shaped once per uniform-size piece; every char boundary gets an x position on an
//!   unbroken line (`x_at`), so any range's width is a subtraction.
//! - Lines break at UAX #14 opportunities (`unicode-linebreak`, as Chromium does), plus every
//!   character inside an external link's tail (the fork inserts `<wbr>`), never inside the link's
//!   lead, and inside over-long words when `overflow-wrap: anywhere` applies.
//! - Each line is a strut-height box with a shared baseline; mixed sizes sit on it.
//! - Atoms (chips, favicons, footnote refs) are real elements laid out as roots at their slot.
//! - The block joins the window text selection (`gpui_kit::base::TextSelection`) as one
//!   participant, paints its own highlight and copies the selected text.

use std::{cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui_kit::{
    AnyElement, App, AvailableSpace, BorderStyle, Bounds, Corners, CursorStyle, Edges, Element,
    ElementId, Font, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId,
    IntoElement, LayoutId, LineLayout, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PaintQuad, Pixels, Point, SharedString, Size, StrikethroughStyle, Style, TextAlign, TextRun,
    Window,
    base::{TextSelectionHandle, TextSelectionRegistration},
    point, px, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

/// How a block wraps (CSS `white-space` / `overflow-wrap` / `text-overflow` combinations).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextWrap {
    /// `white-space: normal; overflow-wrap: anywhere` (chat text).
    Normal,
    /// `white-space: pre`: only `\n` breaks (code with wrapping off).
    Pre,
    /// `white-space: pre-wrap; overflow-wrap: anywhere` (code with wrapping on).
    PreWrap,
    /// `white-space: nowrap; text-overflow: ellipsis` (collapsed table cells).
    Truncate,
}

/// The resolved look of a run of text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunStyle {
    pub font: Font,
    pub size: Pixels,
    pub color: Hsla,
    pub strikethrough: bool,
    /// Inline code: padded, bordered box behind the run.
    pub code_box: bool,
    /// Raises the baseline (superscript) or lowers it (negative).
    pub shift: Pixels,
}

/// The space reserved for an atom (an inline element) at its `\u{FFFC}`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AtomSlot {
    pub offset: usize,
    /// The element's size.
    pub size: Size<Pixels>,
    /// Horizontal margins around the element.
    pub margin_left: Pixels,
    pub margin_right: Pixels,
    /// Distance from the line's baseline to the element's top (negative is above).
    pub top_from_baseline: Pixels,
    /// Text copied for the atom when it is selected.
    pub copy_text: SharedString,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinkSlot {
    pub range: Range<usize>,
    pub href: SharedString,
    pub nowrap_until: Option<usize>,
}

/// Colors used for decorations.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Decorations {
    pub code_background: Hsla,
    pub code_border: Hsla,
    pub selection: Hsla,
}

/// Everything needed to lay out and paint one inline block. Built per render from the parsed
/// [`crate::document::Inline`] and the style; the layout cache keys on its identity (`Rc`).
#[derive(Debug, PartialEq)]
pub(crate) struct TextContent {
    pub text: SharedString,
    /// Runs covering `text` in order.
    pub runs: Vec<(Range<usize>, RunStyle)>,
    pub links: Vec<LinkSlot>,
    pub atoms: Vec<AtomSlot>,
    /// The block's own font, size and line height (the CSS strut).
    pub strut_font: Font,
    pub strut_size: Pixels,
    pub line_height: Pixels,
    pub wrap: TextWrap,
    /// `text-align` within the wrap width (table cells honor GFM column alignment).
    pub align: TextAlign,
    pub decorations: Decorations,
}

/// Invoked with a link's href when it is clicked.
pub(crate) type LinkHandler = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

pub(crate) struct InlineText {
    id: ElementId,
    content: Rc<TextContent>,
    atoms: Vec<AnyElement>,
    on_link: Option<LinkHandler>,
    selectable: bool,
}

impl InlineText {
    /// `atoms` must be one element per `content.atoms`, in order, sized as their slots say.
    pub fn new(id: impl Into<ElementId>, content: Rc<TextContent>, atoms: Vec<AnyElement>) -> Self {
        debug_assert_eq!(atoms.len(), content.atoms.len());
        Self {
            id: id.into(),
            content,
            atoms,
            on_link: None,
            selectable: true,
        }
    }

    pub fn on_link(mut self, handler: LinkHandler) -> Self {
        self.on_link = Some(handler);
        self
    }

    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }
}

/// A shaped run of one size: its byte range and glyph layout.
struct Piece {
    range: Range<usize>,
    run_start: usize,
    layout: Arc<LineLayout>,
}

/// One fragment of a laid-out line.
#[derive(Clone, Debug)]
pub(crate) struct Fragment {
    range: Range<usize>,
    x: Pixels,
    width: Pixels,
    kind: FragmentKind,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FragmentKind {
    Text { run: usize },
    Atom { atom: usize },
}

#[derive(Clone, Debug)]
pub(crate) struct LineBox {
    /// Bytes on the line, without a terminating `\n`.
    range: Range<usize>,
    top: Pixels,
    fragments: Vec<Fragment>,
    width: Pixels,
    /// For truncated lines: where the ellipsis is drawn and the run it takes its style from.
    ellipsis: Option<(Pixels, usize)>,
    /// `text-align` shift of the whole line (already included in fragment and ellipsis x).
    x_offset: Pixels,
}

/// A computed layout for one wrap width.
pub(crate) struct TextLayout {
    wrap_width: Option<Pixels>,
    lines: Vec<LineBox>,
    size: Size<Pixels>,
    /// Baseline offset from a line's top.
    baseline: Pixels,
    /// x on the unbroken line for every byte offset (`len + 1` entries).
    x_at: Vec<f32>,
    /// Ascent/descent per run, for code boxes and decorations.
    run_metrics: Vec<(Pixels, Pixels)>,
}

impl TextLayout {
    fn compute(
        content: &TextContent,
        wrap_width: Option<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let text = content.text.as_ref();
        let text_system = cx.text_system().clone();
        let metrics = |font: &Font, size: Pixels| font_metrics(&text_system, font, size);
        let (ascent, descent) = metrics(&content.strut_font, content.strut_size);
        let line_height = content.line_height;
        let baseline = (line_height - ascent - descent) / 2. + ascent;
        let run_metrics: Vec<_> = content
            .runs
            .iter()
            .map(|(_, style)| metrics(&style.font, style.size))
            .collect();

        let x_at = shape_positions(content, window);
        let width_of = |range: Range<usize>| x_at[range.end] - x_at[range.start];

        let breaks = break_opportunities(content);
        let can_split_words = matches!(content.wrap, TextWrap::Normal | TextWrap::PreWrap);
        let preserve_spaces = matches!(content.wrap, TextWrap::Pre | TextWrap::PreWrap);
        let limit = wrap_width
            .filter(|_| !matches!(content.wrap, TextWrap::Pre | TextWrap::Truncate))
            .map(f32::from);

        // Greedy fill between break opportunities.
        let mut line_ranges: Vec<Range<usize>> = Vec::new();
        let mut line_start = 0;
        let mut previous = 0;
        for (index, mandatory) in breaks {
            let segment = previous..index;
            previous = index;
            if let Some(limit) = limit {
                // Trailing spaces hang past the edge and never force a wrap.
                let content_end = if preserve_spaces {
                    strip_newline(text, segment.end)
                } else {
                    trim_trailing_spaces(text, segment.clone())
                };
                let fits = |start: usize| width_of(start..content_end.max(start)) <= limit + 0.01;
                if !fits(line_start) && segment.start > line_start {
                    line_ranges.push(line_start..segment.start);
                    line_start = segment.start;
                }
                if !fits(line_start) && can_split_words {
                    // `overflow-wrap: anywhere`: split the word at grapheme boundaries.
                    let mut cursor = line_start;
                    for (offset, grapheme) in
                        text[segment.start..content_end.max(segment.start)].grapheme_indices(true)
                    {
                        let start = segment.start + offset;
                        let end = start + grapheme.len();
                        if width_of(cursor..end) > limit + 0.01 && start > cursor {
                            line_ranges.push(cursor..start);
                            cursor = start;
                        }
                    }
                    line_start = cursor;
                }
            }
            if mandatory {
                line_ranges.push(line_start..index);
                line_start = index;
            }
        }
        // A final `\n` ends the last line; it does not start an empty one.
        if line_start < text.len() || line_ranges.is_empty() {
            line_ranges.push(line_start..text.len());
        }

        let mut lines = Vec::with_capacity(line_ranges.len());
        let mut top = Pixels::ZERO;
        let mut max_width = Pixels::ZERO;
        for range in line_ranges {
            let range = range.start..strip_newline(text, range.end);
            let visible_end = if preserve_spaces {
                range.end
            } else {
                trim_trailing_spaces(text, range.clone())
            };
            let mut line = LineBox {
                fragments: fragments(content, &x_at, range.start..visible_end),
                width: px(width_of(range.start..visible_end)),
                range,
                top,
                ellipsis: None,
                x_offset: Pixels::ZERO,
            };
            if content.wrap == TextWrap::Truncate
                && let Some(limit) = wrap_width
            {
                truncate_line(content, &x_at, &mut line, limit, window);
            }
            if let Some(limit) = wrap_width
                && content.align != TextAlign::Left
            {
                let free = (limit - line.width).max(Pixels::ZERO);
                let offset = if content.align == TextAlign::Center {
                    free / 2.
                } else {
                    free
                };
                line.x_offset = offset;
                for fragment in &mut line.fragments {
                    fragment.x += offset;
                }
                if let Some((x, _)) = &mut line.ellipsis {
                    *x += offset;
                }
            }
            max_width = max_width.max(line.width);
            top += line_height;
            lines.push(line);
        }
        Self {
            wrap_width,
            lines,
            size: size(max_width.ceil(), top),
            baseline,
            x_at,
            run_metrics,
        }
    }

    /// The byte offset nearest to `position` (relative to the element's origin), clamped.
    fn index_for_position(&self, position: Point<Pixels>, text: &str) -> usize {
        let Some(first) = self.lines.first() else {
            return 0;
        };
        if position.y < first.top {
            return 0;
        }
        let line = self
            .lines
            .iter()
            .find(|line| position.y < line.top + self.line_height())
            .unwrap_or_else(|| self.lines.last().expect("non-empty"));
        if position.y >= self.size.height {
            return text.len();
        }
        let origin = self.x_at[line.range.start];
        let target = f32::from(position.x - line.x_offset) + origin;
        let mut best = line.range.start;
        for (offset, ch) in text[line.range.clone()].char_indices() {
            let start = line.range.start + offset;
            let end = start + ch.len_utf8();
            let mid = (self.x_at[start] + self.x_at[end]) / 2.;
            if target < mid {
                return start;
            }
            best = end;
        }
        best
    }

    fn line_height(&self) -> Pixels {
        self.lines
            .get(1)
            .map_or(self.size.height, |second| second.top - self.lines[0].top)
    }

    /// x of a byte offset within its line, relative to the line start.
    fn x_in_line(&self, line: &LineBox, index: usize) -> Pixels {
        line.x_offset
            + px(self.x_at[index.clamp(line.range.start, line.range.end)]
                - self.x_at[line.range.start])
    }
}

fn strip_newline(text: &str, end: usize) -> usize {
    if text[..end].ends_with('\n') {
        let end = end - 1;
        if text[..end].ends_with('\r') {
            end - 1
        } else {
            end
        }
    } else {
        end
    }
}

fn trim_trailing_spaces(text: &str, range: Range<usize>) -> usize {
    let end = strip_newline(text, range.end).max(range.start);
    range.start + text[range.start..end].trim_end_matches(' ').len()
}

/// Shapes every uniform-size piece and returns the unbroken-line x of every byte offset.
fn shape_positions(content: &TextContent, window: &mut Window) -> Vec<f32> {
    let text = content.text.as_ref();
    let mut x_at = vec![0f32; text.len() + 1];
    let mut x = 0f32;
    let mut cursor = 0;
    for piece in pieces(content, window) {
        // Bytes before this piece are atoms or newlines: give them their widths.
        while cursor < piece.range.start {
            x_at[cursor] = x;
            x += atom_advance(content, cursor);
            cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
        }
        let mut glyphs = piece
            .layout
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| (glyph.index, f32::from(glyph.position.x)))
            .peekable();
        let mut last_x = 0f32;
        for offset in 0..piece.range.len() {
            while let Some(&(index, glyph_x)) = glyphs.peek() {
                if index <= offset {
                    last_x = glyph_x;
                    glyphs.next();
                } else {
                    break;
                }
            }
            x_at[piece.range.start + offset] = x + last_x;
        }
        // Code boxes add their padding and border at the span's edges.
        let pad = f32::from(code_box_pad());
        let (run_range, style) = &content.runs[piece.run_start];
        let mut width = f32::from(piece.layout.width);
        if style.code_box {
            if piece.range.start == run_range.start
                && !is_code_continuation(content, piece.run_start)
            {
                for slot in &mut x_at[piece.range.start + 1..=piece.range.end - 1] {
                    *slot += pad;
                }
                width += pad;
            }
            let ends_span = piece.range.end == span_end(content, piece.run_start);
            if ends_span {
                width += pad;
            }
        }
        x += width;
        cursor = piece.range.end;
    }
    while cursor < text.len() {
        x_at[cursor] = x;
        x += atom_advance(content, cursor);
        cursor += text[cursor..].chars().next().map_or(1, char::len_utf8);
    }
    x_at[text.len()] = x;
    x_at
}

/// Horizontal padding plus border of an inline-code box.
fn code_box_pad() -> Pixels {
    px(crate::style::metrics::INLINE_CODE_PADDING_X + 1.)
}

/// Whether run `index` continues a code span started by the previous run.
fn is_code_continuation(content: &TextContent, index: usize) -> bool {
    index > 0
        && content.runs[index - 1].1.code_box
        && content.runs[index - 1].0.end == content.runs[index].0.start
}

/// The end of the code span containing run `index`.
fn span_end(content: &TextContent, index: usize) -> usize {
    let mut end = content.runs[index].0.end;
    for (range, style) in &content.runs[index + 1..] {
        if style.code_box && range.start == end {
            end = range.end;
        } else {
            break;
        }
    }
    // A span broken by a newline or atom ends where its text does.
    end
}

fn atom_advance(content: &TextContent, offset: usize) -> f32 {
    content
        .atoms
        .iter()
        .find(|atom| atom.offset == offset)
        .map_or(0., |atom| {
            f32::from(atom.margin_left + atom.size.width + atom.margin_right)
        })
}

/// A font's ascent and descent (both positive) at `size`, rounded to whole pixels like Blink
/// does for line layout. font-kit reports descent as a negative distance.
pub(crate) fn font_metrics(
    text_system: &gpui_kit::TextSystem,
    font: &Font,
    size: Pixels,
) -> (Pixels, Pixels) {
    let id = text_system.resolve_font(font);
    let ascent = f32::from(text_system.ascent(id, size)).round();
    let descent = f32::from(text_system.descent(id, size)).abs().round();
    (px(ascent), px(descent))
}

/// Splits the text into shaping pieces: maximal ranges inside one run without `\n` or atoms.
/// Adjacent runs of the same size and shift are shaped together so kerning matches.
fn pieces(content: &TextContent, window: &mut Window) -> Vec<Piece> {
    let text = content.text.as_ref();
    let mut out = Vec::new();
    let mut run_index = 0;
    let mut start = 0;
    while start < text.len() {
        while run_index < content.runs.len() && content.runs[run_index].0.end <= start {
            run_index += 1;
        }
        let Some((run_range, style)) = content.runs.get(run_index) else {
            break;
        };
        let ch = text[start..].chars().next().expect("in bounds");
        if ch == '\n' || ch == crate::document::ATOM_CHAR || ch == '\r' {
            start += ch.len_utf8();
            continue;
        }
        // Extend through following runs with the same size, shift and code-box state.
        let mut end = run_range.end;
        let mut last_run = run_index;
        while let Some((next_range, next)) = content.runs.get(last_run + 1) {
            if next_range.start == end
                && next.size == style.size
                && next.shift == style.shift
                && next.code_box == style.code_box
                && !style.code_box
            {
                end = next_range.end;
                last_run += 1;
            } else {
                break;
            }
        }
        let stop = text[start..end]
            .find(['\n', '\r', crate::document::ATOM_CHAR])
            .map_or(end, |offset| start + offset);
        let range = start..stop;
        let runs = text_runs(content, range.clone());
        let layout =
            window
                .text_system()
                .layout_line(&text[range.clone()], style.size, &runs, None);
        out.push(Piece {
            range: range.clone(),
            run_start: run_index,
            layout,
        });
        start = stop;
    }
    out
}

/// GPUI text runs for `range`, which must lie in runs of one size.
fn text_runs(content: &TextContent, range: Range<usize>) -> Vec<TextRun> {
    content
        .runs
        .iter()
        .filter(|(run_range, _)| run_range.start < range.end && run_range.end > range.start)
        .map(|(run_range, style)| {
            let start = run_range.start.max(range.start);
            let end = run_range.end.min(range.end);
            TextRun {
                len: end - start,
                font: style.font.clone(),
                color: style.color,
                background_color: None,
                underline: None,
                strikethrough: style.strikethrough.then(|| StrikethroughStyle {
                    thickness: px(1.),
                    color: Some(style.color),
                }),
            }
        })
        .collect()
}

/// Break opportunities as (byte offset of the next segment start, mandatory), ending at the
/// text length. Mirrors Chromium: UAX #14, plus the fork's `<wbr>` and `white-space` rules.
fn break_opportunities(content: &TextContent) -> Vec<(usize, bool)> {
    let text = content.text.as_ref();
    let mut breaks: Vec<(usize, bool)> = match content.wrap {
        TextWrap::Pre | TextWrap::Truncate => text
            .match_indices('\n')
            .map(|(index, _)| (index + 1, true))
            .collect(),
        TextWrap::Normal | TextWrap::PreWrap => unicode_linebreak::linebreaks(text)
            .map(|(index, opportunity)| {
                (
                    index,
                    opportunity == unicode_linebreak::BreakOpportunity::Mandatory
                        && text[..index].ends_with('\n'),
                )
            })
            .collect(),
    };
    if matches!(content.wrap, TextWrap::Normal | TextWrap::PreWrap) {
        // Blink's ASCII line breaker never breaks after `/` (long paths and URLs stay whole
        // unless `overflow-wrap` splits them); UAX #14 alone would.
        breaks.retain(|(index, mandatory)| {
            *mandatory
                || !text[..*index].ends_with('/')
                || text[*index..].starts_with(char::is_whitespace)
        });
    }
    if content.wrap == TextWrap::Normal {
        for link in &content.links {
            let Some(lead_end) = link.nowrap_until else {
                continue;
            };
            // No breaks inside the favicon + protocol lead.
            breaks.retain(|(index, mandatory)| {
                *mandatory || !(*index > link.range.start && *index < lead_end)
            });
            // `<wbr>` after every character of the tail.
            breaks.extend(
                text[lead_end..link.range.end]
                    .char_indices()
                    .map(|(offset, ch)| (lead_end + offset + ch.len_utf8(), false))
                    .filter(|(index, _)| *index < link.range.end),
            );
            if lead_end < link.range.end {
                breaks.push((lead_end, false));
            }
        }
        breaks.sort_by_key(|(index, mandatory)| (*index, !*mandatory));
        breaks.dedup_by_key(|(index, _)| *index);
    }
    if breaks.last().is_none_or(|(index, _)| *index != text.len()) {
        breaks.push((text.len(), false));
    }
    breaks
}

/// Splits `range` of one line into text and atom fragments with line-relative x.
fn fragments(content: &TextContent, x_at: &[f32], range: Range<usize>) -> Vec<Fragment> {
    let text = content.text.as_ref();
    let origin = x_at[range.start];
    let mut out = Vec::new();
    let mut start = range.start;
    while start < range.end {
        if let Some(atom) = content.atoms.iter().position(|atom| atom.offset == start) {
            let end = start + crate::document::ATOM_CHAR.len_utf8();
            out.push(Fragment {
                range: start..end,
                x: px(x_at[start] - origin),
                width: px(x_at[end] - x_at[start]),
                kind: FragmentKind::Atom { atom },
            });
            start = end;
            continue;
        }
        let Some(run) = content
            .runs
            .iter()
            .position(|(run_range, _)| run_range.contains(&start))
        else {
            break;
        };
        let run_end = content.runs[run].0.end.min(range.end);
        let end = text[start..run_end]
            .find(crate::document::ATOM_CHAR)
            .map_or(run_end, |offset| start + offset);
        if end > start {
            out.push(Fragment {
                range: start..end,
                x: px(x_at[start] - origin),
                width: px(x_at[end] - x_at[start]),
                kind: FragmentKind::Text { run },
            });
        }
        start = end.max(start + 1);
    }
    out
}

/// Cuts a line to `limit`, reserving room for an ellipsis in the style of the last kept run.
fn truncate_line(
    content: &TextContent,
    x_at: &[f32],
    line: &mut LineBox,
    limit: Pixels,
    window: &mut Window,
) {
    if line.width <= limit + px(0.01) {
        return;
    }
    let text = content.text.as_ref();
    let origin = x_at[line.range.start];
    let run_at = |index: usize| {
        content
            .runs
            .iter()
            .position(|(range, _)| range.contains(&index))
            .unwrap_or(0)
    };
    let ellipsis_width = |run: usize| {
        let style = &content.runs[run].1;
        let runs = [TextRun {
            len: '…'.len_utf8(),
            font: style.font.clone(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        window
            .text_system()
            .layout_line("…", style.size, &runs, None)
            .width
    };
    let mut cut = line.range.start;
    for (offset, ch) in text[line.range.clone()].char_indices() {
        let end = line.range.start + offset + ch.len_utf8();
        let run = run_at(line.range.start + offset);
        if px(x_at[end] - origin) + ellipsis_width(run) > limit {
            break;
        }
        cut = end;
    }
    let run = run_at(cut.saturating_sub(1).max(line.range.start));
    line.fragments = fragments(content, x_at, line.range.start..cut);
    line.ellipsis = Some((px(x_at[cut] - origin), run));
    line.width = limit;
}

/// Element state kept across frames.
#[derive(Default)]
struct InlineTextState {
    layout: Option<(Rc<TextContent>, Rc<TextLayout>)>,
    selection: Option<TextSelectionHandle>,
    /// What the selection currently covers, read by the copy callback.
    selected: Rc<RefCell<Option<SharedString>>>,
    /// Shared with mouse listeners, which run outside paint and cannot touch element state.
    pointer: Rc<RefCell<Pointer>>,
}

#[derive(Default)]
struct Pointer {
    hovered_link: Option<usize>,
    pressed_link: Option<usize>,
}

pub(crate) struct PrepaintState {
    layout: Rc<TextLayout>,
    hitbox: Hitbox,
    link_hitboxes: Vec<(usize, Hitbox)>,
}

impl IntoElement for InlineText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

type SharedLayout = Rc<RefCell<Option<(Rc<TextContent>, Rc<TextLayout>)>>>;

impl Element for InlineText {
    type RequestLayoutState = SharedLayout;
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let cached = window.with_element_state(
            global_id.expect("InlineText has an id"),
            |state: Option<InlineTextState>, _| {
                let state = state.unwrap_or_default();
                (state.layout.clone(), state)
            },
        );
        let shared: SharedLayout = Rc::new(RefCell::new(
            cached.filter(|(content, _)| **content == *self.content),
        ));
        let content = self.content.clone();
        let measured = shared.clone();
        let mut style = Style::default();
        // Block-level: fill the line box width unless the text is wider (nowrap).
        style.size.width = gpui_kit::relative(1.).into();
        if matches!(content.wrap, TextWrap::Pre) {
            style.size.width = gpui_kit::Length::Auto;
        }
        let layout_id =
            window.request_measured_layout(style, move |known, available, window, cx| {
                let wrap_width = known.width.or(match available.width {
                    AvailableSpace::Definite(width) => Some(width),
                    _ => None,
                });
                if let Some((_, layout)) = measured.borrow().as_ref()
                    && layout.wrap_width == wrap_width
                {
                    return layout_size(&content, layout, known);
                }
                let layout = Rc::new(TextLayout::compute(&content, wrap_width, window, cx));
                let size = layout_size(&content, &layout, known);
                *measured.borrow_mut() = Some((content.clone(), layout));
                size
            });
        (layout_id, shared)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        shared: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // The final width can differ from the last measurement; lay out again if so.
        let width = Some(bounds.size.width);
        let layout = match shared.borrow().as_ref() {
            Some((_, layout))
                if layout.wrap_width == width || matches!(self.content.wrap, TextWrap::Pre) =>
            {
                Some(layout.clone())
            }
            _ => None,
        };
        let layout = layout
            .unwrap_or_else(|| Rc::new(TextLayout::compute(&self.content, width, window, cx)));
        let content = self.content.clone();
        window.with_element_state(
            global_id.expect("InlineText has an id"),
            |state: Option<InlineTextState>, _| {
                let mut state = state.unwrap_or_default();
                state.layout = Some((content, layout.clone()));
                ((), state)
            },
        );

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let mut link_hitboxes = Vec::new();
        for line in &layout.lines {
            for fragment in &line.fragments {
                if let Some(link) = self
                    .content
                    .links
                    .iter()
                    .position(|link| link.range.contains(&fragment.range.start))
                {
                    let fragment_bounds = Bounds::new(
                        bounds.origin + point(fragment.x, line.top),
                        size(fragment.width, layout.line_height()),
                    );
                    link_hitboxes.push((
                        link,
                        window.insert_hitbox(fragment_bounds, HitboxBehavior::Normal),
                    ));
                }
            }
        }

        for (index, element) in self.atoms.iter_mut().enumerate() {
            let slot = &self.content.atoms[index];
            let Some((line, fragment)) = layout.lines.iter().find_map(|line| {
                line.fragments
                    .iter()
                    .find(|fragment| matches!(fragment.kind, FragmentKind::Atom { atom } if atom == index))
                    .map(|fragment| (line, fragment))
            }) else {
                continue;
            };
            let origin = bounds.origin
                + point(
                    fragment.x + slot.margin_left,
                    line.top + layout.baseline + slot.top_from_baseline,
                );
            element.layout_as_root(
                size(
                    AvailableSpace::Definite(slot.size.width),
                    AvailableSpace::Definite(slot.size.height),
                ),
                window,
                cx,
            );
            element.prepaint_at(origin, window, cx);
        }

        PrepaintState {
            layout,
            hitbox,
            link_hitboxes,
        }
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let global_id = global_id.expect("InlineText has an id");
        let content = self.content.clone();
        let layout = prepaint.layout.clone();
        let text = content.text.clone();

        // Selection participant, created once per element.
        let (selection, selected_text, pointer) =
            window.with_element_state(global_id, |state: Option<InlineTextState>, _| {
                let mut state = state.unwrap_or_default();
                let selection = state
                    .selection
                    .get_or_insert_with(|| {
                        let handle = TextSelectionHandle::new(text.to_string(), cx);
                        let selected = state.selected.clone();
                        handle.copy_with(
                            move |_| {
                                selected
                                    .borrow()
                                    .as_ref()
                                    .map_or_else(String::new, ToString::to_string)
                            },
                            cx,
                        );
                        handle
                    })
                    .clone();
                let result = (selection, state.selected.clone(), state.pointer.clone());
                (result, state)
            });

        // Selected range from the window selection's endpoints.
        let selected = self
            .selectable
            .then(|| selection.snapshot(cx))
            .flatten()
            .and_then(|snapshot| snapshot.window_points())
            .map(|points| {
                let a = layout.index_for_position(points.anchor() - bounds.origin, &text);
                let b = layout.index_for_position(points.cursor() - bounds.origin, &text);
                a.min(b)..a.max(b)
            })
            .filter(|range| !range.is_empty());
        *selected_text.borrow_mut() = selected
            .clone()
            .map(|range| SharedString::from(copy_text(&content, range)));

        // Selection highlight, under everything.
        if let Some(range) = &selected {
            for line in &layout.lines {
                let start = range.start.max(line.range.start);
                let end = range.end.min(line.range.end);
                let reaches_next_line = range.end > line.range.end && range.start <= line.range.end;
                if start >= end && !reaches_next_line {
                    continue;
                }
                let x0 = layout.x_in_line(line, start);
                let mut x1 = layout.x_in_line(line, end);
                if reaches_next_line {
                    // A selection continuing past the line covers its trailing space.
                    x1 = x1.max(line.x_offset + line.width);
                }
                window.paint_quad(PaintQuad {
                    bounds: Bounds::new(
                        bounds.origin + point(x0, line.top),
                        size(x1 - x0, layout.line_height()),
                    ),
                    corner_radii: Corners::default(),
                    background: content.decorations.selection.into(),
                    border_widths: Edges::default(),
                    border_color: Hsla::transparent_black(),
                    border_style: BorderStyle::default(),
                });
            }
        }

        // Only lines inside the visible area are painted (long code blocks, scrolled views).
        let mask = window.content_mask().bounds;
        let line_height = layout.line_height();
        let visible = |line: &&LineBox| {
            let top = bounds.origin.y + line.top;
            top + line_height >= mask.origin.y && top <= mask.origin.y + mask.size.height
        };

        // Inline-code boxes.
        for line in layout.lines.iter().filter(visible) {
            paint_code_boxes(&content, &layout, line, bounds, window);
        }

        // Glyphs.
        for line in layout.lines.iter().filter(visible) {
            for fragment in &line.fragments {
                let FragmentKind::Text { run } = fragment.kind else {
                    continue;
                };
                let style = &content.runs[run].1;
                let fragment_text = &text[fragment.range.clone()];
                if fragment_text.trim_matches([' ', '\n', '\r']).is_empty() {
                    continue;
                }
                let runs = text_runs(&content, fragment.range.clone());
                let shaped = window.text_system().shape_line(
                    SharedString::from(fragment_text.to_string()),
                    style.size,
                    &runs,
                    None,
                );
                let pad = if style.code_box
                    && content.runs[run].0.start == fragment.range.start
                    && !is_code_continuation(&content, run)
                {
                    code_box_pad()
                } else {
                    Pixels::ZERO
                };
                let origin = bounds.origin
                    + point(
                        fragment.x + pad,
                        line.top + layout.baseline - shaped.ascent - style.shift,
                    );
                let _ = shaped.paint(
                    origin,
                    shaped.ascent + shaped.descent,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            if let Some((x, run)) = line.ellipsis {
                let style = &content.runs[run].1;
                let runs = [TextRun {
                    len: '…'.len_utf8(),
                    font: style.font.clone(),
                    color: style.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }];
                let shaped = window
                    .text_system()
                    .shape_line("…".into(), style.size, &runs, None);
                let origin = bounds.origin + point(x, line.top + layout.baseline - shaped.ascent);
                let _ = shaped.paint(
                    origin,
                    shaped.ascent + shaped.descent,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }

        // Hovered link: dotted underline (`radial-gradient` dots, 4x2px tiles at the bottom).
        let hovered_link = pointer.borrow().hovered_link;
        if let Some(link) = hovered_link.and_then(|index| content.links.get(index)) {
            paint_link_dots(&content, &layout, link, bounds, window);
        }

        for element in &mut self.atoms {
            element.paint(window, cx);
        }

        // Cursor: I-beam over text, pointer over links.
        window.set_cursor_style(CursorStyle::IBeam, &prepaint.hitbox);
        for (_, hitbox) in &prepaint.link_hitboxes {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }

        // Link hover and click.
        let link_hitboxes = prepaint.link_hitboxes.clone();
        {
            let pointer = pointer.clone();
            let link_hitboxes = link_hitboxes.clone();
            window.on_mouse_event(move |_: &MouseMoveEvent, phase, window, _| {
                if !phase.bubble() {
                    return;
                }
                let hovered = link_hitboxes
                    .iter()
                    .find(|(_, hitbox)| hitbox.is_hovered(window))
                    .map(|(link, _)| *link);
                let changed = {
                    let mut pointer = pointer.borrow_mut();
                    std::mem::replace(&mut pointer.hovered_link, hovered) != hovered
                };
                if changed {
                    window.refresh();
                }
            });
        }
        {
            let pointer = pointer.clone();
            let link_hitboxes = link_hitboxes.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
                if phase.bubble() && event.button == MouseButton::Left {
                    pointer.borrow_mut().pressed_link = link_hitboxes
                        .iter()
                        .find(|(_, hitbox)| hitbox.is_hovered(window))
                        .map(|(link, _)| *link);
                }
            });
        }
        if let Some(on_link) = self.on_link.clone() {
            let content = content.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if !phase.bubble() || event.button != MouseButton::Left {
                    return;
                }
                let released = link_hitboxes
                    .iter()
                    .find(|(_, hitbox)| hitbox.is_hovered(window))
                    .map(|(link, _)| *link);
                let pressed = pointer.borrow_mut().pressed_link.take();
                if let (Some(pressed), Some(released)) = (pressed, released)
                    && pressed == released
                    && let Some(link) = content.links.get(released)
                {
                    on_link(&link.href, window, cx);
                }
            });
        }

        // Register with the window selection after painting, as the base participants do.
        if self.selectable {
            let text_bounds = layout
                .lines
                .iter()
                .map(|line| {
                    Bounds::new(
                        bounds.origin + point(line.x_offset, line.top),
                        size(line.width.max(px(1.)), layout.line_height()),
                    )
                })
                .collect();
            let order = document_order(bounds.origin);
            let registration = TextSelectionRegistration::new(prepaint.hitbox.clone(), bounds)
                .with_document_order(order)
                .with_text_bounds(text_bounds)
                .with_rendered_element(&selection, window, cx);
            selection.register(registration, window, cx);
        }
    }
}

/// Window-position reading order. The base crate orders its own participants by paint order,
/// which is private; for the chat column (one column, top to bottom) position gives the same
/// order and stays consistent between frames.
fn document_order(origin: Point<Pixels>) -> u64 {
    let y = (f32::from(origin.y) + 1_000_000.).max(0.) as u64;
    let x = (f32::from(origin.x) + 100_000.).max(0.) as u64;
    y * 1_000_000 + x
}

/// The text copied for `range`: atoms copy their own text.
fn copy_text(content: &TextContent, range: Range<usize>) -> String {
    let text = &content.text[range.clone()];
    let mut out = String::with_capacity(text.len());
    for (offset, ch) in text.char_indices() {
        if ch == crate::document::ATOM_CHAR {
            if let Some(atom) = content
                .atoms
                .iter()
                .find(|atom| atom.offset == range.start + offset)
            {
                out.push_str(&atom.copy_text);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn layout_size(
    content: &TextContent,
    layout: &TextLayout,
    known: Size<Option<Pixels>>,
) -> Size<Pixels> {
    let width = match content.wrap {
        TextWrap::Pre => layout.size.width,
        _ => known.width.unwrap_or(layout.size.width),
    };
    size(width, known.height.unwrap_or(layout.size.height))
}

fn paint_code_boxes(
    content: &TextContent,
    layout: &TextLayout,
    line: &LineBox,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    let mut fragments = line
        .fragments
        .iter()
        .filter_map(|fragment| match fragment.kind {
            FragmentKind::Text { run } if content.runs[run].1.code_box => Some((fragment, run)),
            _ => None,
        })
        .peekable();
    while let Some((first, run)) = fragments.next() {
        let mut last = first;
        while let Some((next, _)) = fragments.peek() {
            if next.range.start == last.range.end {
                last = next;
                fragments.next();
            } else {
                break;
            }
        }
        let (ascent, descent) = layout.run_metrics[run];
        let pad_y = px(crate::style::metrics::INLINE_CODE_PADDING_Y + 1.);
        let top = line.top + layout.baseline - ascent - pad_y;
        let bottom = line.top + layout.baseline + descent + pad_y;
        let starts_span =
            content.runs[run].0.start == first.range.start && !is_code_continuation(content, run);
        let ends_span = {
            let last_run = content
                .runs
                .iter()
                .position(|(range, _)| range.contains(&(last.range.end - 1)))
                .unwrap_or(run);
            last.range.end == span_end(content, last_run)
        };
        let left = first.x;
        let right = last.x + last.width;
        let radius = px(crate::style::metrics::INLINE_CODE_RADIUS);
        let corner = |rounded: bool| if rounded { radius } else { Pixels::ZERO };
        window.paint_quad(PaintQuad {
            bounds: Bounds::from_corners(
                bounds.origin + point(left, top),
                bounds.origin + point(right, bottom),
            ),
            corner_radii: Corners {
                top_left: corner(starts_span),
                bottom_left: corner(starts_span),
                top_right: corner(ends_span),
                bottom_right: corner(ends_span),
            },
            background: content.decorations.code_background.into(),
            border_widths: Edges {
                top: px(1.),
                bottom: px(1.),
                left: if starts_span { px(1.) } else { Pixels::ZERO },
                right: if ends_span { px(1.) } else { Pixels::ZERO },
            },
            border_color: content.decorations.code_border,
            border_style: BorderStyle::Solid,
        });
    }
}

fn paint_link_dots(
    content: &TextContent,
    layout: &TextLayout,
    link: &LinkSlot,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    for line in &layout.lines {
        for fragment in &line.fragments {
            let FragmentKind::Text { run } = fragment.kind else {
                continue;
            };
            if !(link.range.start <= fragment.range.start && fragment.range.end <= link.range.end) {
                continue;
            }
            let style = &content.runs[run].1;
            let (_, descent) = layout.run_metrics[run];
            let bottom = line.top + layout.baseline + descent;
            let mut x = fragment.x;
            while x + px(1.25) < fragment.x + fragment.width {
                window.paint_quad(PaintQuad {
                    bounds: Bounds::new(
                        bounds.origin + point(x + px(1.25), bottom - px(1.75)),
                        size(px(1.5), px(1.5)),
                    ),
                    corner_radii: Corners::all(px(0.75)),
                    background: style.color.into(),
                    border_widths: Edges::default(),
                    border_color: Hsla::transparent_black(),
                    border_style: BorderStyle::default(),
                });
                x += px(4.);
            }
        }
    }
}
