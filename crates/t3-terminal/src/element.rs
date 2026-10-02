//! The custom element that paints a `TerminalView`: merged background quads, one shaped line
//! per run of same-row cells (glyphs snapped to the cell grid), selection, cursor, IME
//! composition and the overlay scrollbar. Painting reads the grid; it never mutates it.

use alacritty_terminal::{
    index::{Column, Line, Point as GridPoint},
    selection::SelectionRange,
    term::{
        cell::{Cell, Flags},
        color::Colors,
    },
    vte::ansi::{Color, CursorShape, NamedColor},
};
use gpui_kit::{
    App, BorderStyle, Bounds, ContentMask, Corners, CursorStyle, DispatchPhase, Edges, Element,
    ElementId, ElementInputHandler, Entity, Font, FontStyle, FontWeight, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseDownEvent,
    MouseExitEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    ShapedLine, SharedString, StrikethroughStyle, Style, TextAlign, TextRun, UnderlineStyle,
    Window, fill, font, outline, point, px, quad, relative, size, transparent_black,
};

use t3_ui::ActiveColors as _;

use crate::{
    theme::{TerminalTheme, rgb_to_hsla},
    view::{FONT_SIZE, GridGeometry, TerminalView},
};

/// Paints `view` at its layout bounds. Created by `TerminalView::render`.
pub(crate) struct TerminalElement {
    view: Entity<TerminalView>,
}

impl TerminalElement {
    pub(crate) fn new(view: Entity<TerminalView>) -> Self {
        Self { view }
    }
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Everything paint needs, computed in prepaint.
pub(crate) struct Frame {
    hitbox: Hitbox,
    geometry: GridGeometry,
    background: Hsla,
    /// Cell backgrounds, selection and the block cursor, in paint order.
    under_text: Vec<PaintQuad>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    /// Bar, underline and hollow cursors.
    over_text: Vec<PaintQuad>,
    marked_text: Option<(PaintQuad, Point<Pixels>, ShapedLine)>,
    scrollbar: Option<PaintQuad>,
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let geometry = self
            .view
            .update(cx, |view, cx| view.layout_grid(bounds, window, cx));
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let view = self.view.read(cx);
        let focused = view.focus_handle_ref().is_focused(window) && window.is_window_active();
        let theme = TerminalTheme::new(cx.colors());
        FrameBuilder::new(view, &theme, geometry, focused, window).build(hitbox)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_height = frame.geometry.cell.height;
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_quad(quad(
                bounds,
                Corners::all(px(4.)),
                frame.background,
                Edges::default(),
                transparent_black(),
                BorderStyle::default(),
            ));
            for rect in frame.under_text.drain(..) {
                window.paint_quad(rect);
            }
            for (origin, line) in &frame.lines {
                line.paint(*origin, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            for rect in frame.over_text.drain(..) {
                window.paint_quad(rect);
            }
            if let Some((background, origin, line)) = frame.marked_text.take() {
                window.paint_quad(background);
                line.paint(origin, line_height, TextAlign::Left, None, window, cx)
                    .ok();
            }
            if let Some(scrollbar) = frame.scrollbar.take() {
                window.paint_quad(scrollbar);
            }
        });

        let view = self.view.read(cx);
        let mouse = window.mouse_position();
        let over_scrollbar = frame
            .geometry
            .scrollbar
            .is_some_and(|s| s.track.contains(&mouse));
        let cursor_style = if over_scrollbar || view.reports_mouse(window.modifiers()) {
            CursorStyle::Arrow
        } else if view.hovered_link().is_some() {
            CursorStyle::PointingHand
        } else {
            CursorStyle::IBeam
        };
        window.set_cursor_style(cursor_style, &frame.hitbox);
        let focus_handle = view.focus_handle_ref().clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
        self.register_mouse_listeners(&frame.hitbox, window);
        self.view.update(cx, |view, cx| view.did_paint(window, cx));
    }
}

impl TerminalElement {
    fn register_mouse_listeners(&self, hitbox: &Hitbox, window: &mut Window) {
        window.on_mouse_event({
            let (view, hitbox) = (self.view.clone(), hitbox.clone());
            move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    view.update(cx, |view, cx| view.on_mouse_down(event, window, cx));
                }
            }
        });
        window.on_mouse_event({
            let (view, hitbox) = (self.view.clone(), hitbox.clone());
            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let hovered = hitbox.is_hovered(window);
                    view.update(cx, |view, cx| view.on_mouse_move(event, hovered, cx));
                }
            }
        });
        window.on_mouse_event({
            let view = self.view.clone();
            move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    view.update(cx, |view, cx| view.on_mouse_up(event, cx));
                }
            }
        });
        window.on_mouse_event({
            let view = self.view.clone();
            move |event: &MouseExitEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let moved = MouseMoveEvent {
                        position: event.position,
                        pressed_button: event.pressed_button,
                        modifiers: event.modifiers,
                    };
                    view.update(cx, |view, cx| view.on_mouse_move(&moved, false, cx));
                }
            }
        });
        window.on_mouse_event({
            let (view, hitbox) = (self.view.clone(), hitbox.clone());
            move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                    view.update(cx, |view, cx| view.on_scroll_wheel(event, cx));
                    cx.stop_propagation();
                }
            }
        });
    }
}

/// Text attributes that split shaped runs.
#[derive(Clone, PartialEq)]
struct TextStyle {
    /// Index into `FrameBuilder::fonts`: regular, bold, italic, bold italic.
    font: usize,
    color: Hsla,
    underline: Option<UnderlineStyle>,
    strikethrough: Option<StrikethroughStyle>,
}

/// Consecutive narrow cells of one row, shaped as one line with glyphs snapped to the grid.
struct Segment {
    start_col: usize,
    text: String,
    runs: Vec<(TextStyle, usize)>,
    /// Blank cells seen after the last glyph; emitted as spaces only if more text follows.
    pending_blanks: usize,
}

impl Segment {
    fn new(start_col: usize) -> Self {
        Self {
            start_col,
            text: String::new(),
            runs: Vec::new(),
            pending_blanks: 0,
        }
    }

    fn push(&mut self, text: &str, style: &TextStyle) {
        if self.pending_blanks > 0 {
            let blank_style = TextStyle {
                underline: None,
                strikethrough: None,
                ..self
                    .runs
                    .last()
                    .map_or_else(|| style.clone(), |(last, _)| last.clone())
            };
            let blanks = " ".repeat(self.pending_blanks);
            self.pending_blanks = 0;
            self.append(&blanks, &blank_style);
        }
        self.append(text, style);
    }

    fn append(&mut self, text: &str, style: &TextStyle) {
        match self.runs.last_mut() {
            Some((last, len)) if last == style => *len += text.len(),
            _ => self.runs.push((style.clone(), text.len())),
        }
        self.text.push_str(text);
    }
}

/// Builds a `Frame` from the terminal grid.
struct FrameBuilder<'a> {
    view: &'a TerminalView,
    theme: &'a TerminalTheme,
    colors: &'a Colors,
    geometry: GridGeometry,
    focused: bool,
    window: &'a Window,
    fonts: [Font; 4],
    scale: f32,
    under_text: Vec<PaintQuad>,
    lines: Vec<(Point<Pixels>, ShapedLine)>,
    over_text: Vec<PaintQuad>,
}

impl<'a> FrameBuilder<'a> {
    fn new(
        view: &'a TerminalView,
        theme: &'a TerminalTheme,
        geometry: GridGeometry,
        focused: bool,
        window: &'a Window,
    ) -> Self {
        let family = view.font_family().clone();
        let variant = |weight, style| Font {
            weight,
            style,
            ..font(family.clone())
        };
        Self {
            view,
            theme,
            colors: view.session().term().colors(),
            geometry,
            focused,
            window,
            fonts: [
                variant(FontWeight::NORMAL, FontStyle::Normal),
                variant(FontWeight::BOLD, FontStyle::Normal),
                variant(FontWeight::NORMAL, FontStyle::Italic),
                variant(FontWeight::BOLD, FontStyle::Italic),
            ],
            scale: window.scale_factor(),
            under_text: Vec::new(),
            lines: Vec::new(),
            over_text: Vec::new(),
        }
    }

    fn build(mut self, hitbox: Hitbox) -> Frame {
        let view = self.view;
        let term = view.session().term();
        let content = term.renderable_content();
        let display_offset = content.display_offset as i32;
        let selection = content.selection;
        let cursor = self.cursor(content.cursor.shape, content.cursor.point, display_offset);
        let grid = term.grid();

        for row in 0..self.geometry.rows {
            let line = Line(row as i32 - display_offset);
            self.build_row(&grid[line], line, row, cursor.as_ref());
            if let Some(range) = &selection {
                self.selection_rect(range, line, row);
            }
        }
        if let Some(cursor) = &cursor {
            self.paint_cursor(cursor);
        }

        let marked_text = self.marked_text(cursor.as_ref());
        let scrollbar = self.scrollbar();
        Frame {
            hitbox,
            geometry: self.geometry,
            background: self.theme.background(),
            under_text: self.under_text,
            lines: self.lines,
            over_text: self.over_text,
            marked_text,
            scrollbar,
        }
    }

    fn build_row(
        &mut self,
        cells: &alacritty_terminal::grid::Row<Cell>,
        line: Line,
        row: usize,
        cursor: Option<&CursorPaint>,
    ) {
        let y = self.geometry.bounds.top() + self.geometry.cell.height * row as f32;
        let link = self.view.hovered_link().map(|hit| (hit.start, hit.end));
        let mut background: Option<(usize, usize, Hsla)> = None;
        let mut segment: Option<Segment> = None;
        let mut col = 0;
        while col < self.geometry.cols {
            let cell = &cells[Column(col)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                col += 1;
                continue;
            }
            let width = if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
            let point = GridPoint::new(line, Column(col));
            let (mut fg, bg) = self.cell_colors(cell);

            match (bg, &mut background) {
                (Some(color), Some((_, end, run))) if *end == col && *run == color => {
                    *end = col + width
                }
                (Some(color), _) => {
                    self.background_rect(background.take(), y);
                    background = Some((col, col + width, color));
                }
                (None, _) => self.background_rect(background.take(), y),
            }

            if let Some(cursor) = cursor.filter(|cursor| cursor.point == point) {
                fg = cursor.text_color.unwrap_or(fg);
            }
            let in_link = link.is_some_and(|(start, end)| start <= point && point <= end);
            let style = self.text_style(cell, fg, in_link);
            let blank = cell.flags.contains(Flags::HIDDEN)
                || (cell.c == ' ' && cell.zerowidth().is_none())
                || cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER);
            let blank = blank && style.underline.is_none() && style.strikethrough.is_none();

            if width == 2 {
                self.flush_segment(segment.take(), y);
                if !blank {
                    let mut segment = Segment::new(col);
                    segment.push(&cell_text(cell), &style);
                    self.shape_segment(segment, y, false);
                }
            } else if blank {
                if let Some(segment) = &mut segment {
                    segment.pending_blanks += 1;
                }
            } else {
                let text = if cell.flags.contains(Flags::HIDDEN) {
                    " ".into()
                } else {
                    cell_text(cell)
                };
                segment
                    .get_or_insert_with(|| Segment::new(col))
                    .push(&text, &style);
            }
            col += width;
        }
        self.background_rect(background, y);
        self.flush_segment(segment, y);
    }

    /// Resolves a cell's colors like xterm's DOM renderer: bold draws palette colors 0-7 in
    /// their bright variant, inverse swaps (defaults included), dim halves the text alpha.
    /// The background is `None` when it is the default.
    fn cell_colors(&self, cell: &Cell) -> (Hsla, Option<Hsla>) {
        let bold = cell.flags.contains(Flags::BOLD);
        let mut fg = self.color(cell.fg, bold);
        let mut bg =
            (cell.bg != Color::Named(NamedColor::Background)).then(|| self.color(cell.bg, false));
        if cell.flags.contains(Flags::INVERSE) {
            let swapped_fg = bg.unwrap_or_else(|| self.theme.background());
            bg = Some(fg);
            fg = swapped_fg;
        }
        if cell.flags.contains(Flags::DIM) {
            fg.a *= 0.5;
        }
        (fg, bg)
    }

    fn color(&self, color: Color, bold: bool) -> Hsla {
        let bright = |index: usize| if bold && index < 8 { index + 8 } else { index };
        match color {
            Color::Spec(rgb) => rgb_to_hsla(rgb),
            Color::Indexed(index) => self.indexed(bright(usize::from(index))),
            Color::Named(named) => match named as usize {
                index @ 0..16 => self.indexed(bright(index)),
                index @ 259..267 => self.indexed(index - 259),
                index => {
                    let fallback = match named {
                        NamedColor::Background => self.theme.background(),
                        NamedColor::Cursor => self.theme.cursor(),
                        _ => self.theme.foreground(),
                    };
                    let index = if index > 258 {
                        NamedColor::Foreground as usize
                    } else {
                        index
                    };
                    self.colors[index].map_or(fallback, rgb_to_hsla)
                }
            },
        }
    }

    /// Palette color, honoring OSC 4 overrides.
    fn indexed(&self, index: usize) -> Hsla {
        self.colors[index].map_or_else(|| self.theme.palette(index as u8), rgb_to_hsla)
    }

    fn text_style(&self, cell: &Cell, color: Hsla, in_link: bool) -> TextStyle {
        let flags = cell.flags;
        let font = usize::from(flags.contains(Flags::BOLD))
            + 2 * usize::from(flags.contains(Flags::ITALIC));
        let underline = if flags.intersects(Flags::ALL_UNDERLINES) || in_link {
            Some(UnderlineStyle {
                thickness: px(1.),
                color: cell
                    .underline_color()
                    .filter(|_| !in_link)
                    .map(|c| self.color(c, false)),
                wavy: flags.contains(Flags::UNDERCURL) && !in_link,
            })
        } else {
            None
        };
        let strikethrough = flags
            .contains(Flags::STRIKEOUT)
            .then_some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            });
        TextStyle {
            font,
            color,
            underline,
            strikethrough,
        }
    }

    fn flush_segment(&mut self, segment: Option<Segment>, y: Pixels) {
        if let Some(segment) = segment.filter(|segment| !segment.text.is_empty()) {
            self.shape_segment(segment, y, true);
        }
    }

    /// Shapes a segment at its starting cell. Narrow runs snap each glyph to the cell grid
    /// (`force_width`), so fallback glyphs and bold faces cannot drift off their columns.
    fn shape_segment(&mut self, segment: Segment, y: Pixels, snap_to_grid: bool) {
        let runs: Vec<TextRun> = segment
            .runs
            .into_iter()
            .map(|(style, len)| TextRun {
                len,
                font: self.fonts[style.font].clone(),
                color: style.color,
                background_color: None,
                underline: style.underline,
                strikethrough: style.strikethrough,
            })
            .collect();
        let cell_width = self.geometry.cell.width;
        let line = self.window.text_system().shape_line(
            SharedString::from(segment.text),
            FONT_SIZE,
            &runs,
            snap_to_grid.then_some(cell_width),
        );
        let x = self.geometry.bounds.left() + cell_width * segment.start_col as f32;
        self.lines.push((point(x, y), line));
    }

    /// Rounds to whole device pixels so adjacent quads meet without seams.
    fn snap(&self, value: Pixels) -> Pixels {
        px((f32::from(value) * self.scale).round() / self.scale)
    }

    fn snapped_cells(&self, start_col: usize, end_col: usize, y: Pixels) -> Bounds<Pixels> {
        let left = self.geometry.bounds.left();
        let width = self.geometry.cell.width;
        Bounds::from_corners(
            point(self.snap(left + width * start_col as f32), y),
            point(
                self.snap(left + width * end_col as f32),
                y + self.geometry.cell.height,
            ),
        )
    }

    fn background_rect(&mut self, run: Option<(usize, usize, Hsla)>, y: Pixels) {
        if let Some((start, end, color)) = run {
            self.under_text
                .push(fill(self.snapped_cells(start, end, y), color));
        }
    }

    /// The selected columns of one row, painted in xterm's opaque selection color.
    fn selection_rect(&mut self, range: &SelectionRange, line: Line, row: usize) {
        if line < range.start.line || line > range.end.line {
            return;
        }
        let last = self.geometry.cols - 1;
        let (start, end) = if range.is_block {
            let (a, b) = (range.start.column.0, range.end.column.0);
            (a.min(b), a.max(b))
        } else {
            let start = if line == range.start.line {
                range.start.column.0
            } else {
                0
            };
            let end = if line == range.end.line {
                range.end.column.0
            } else {
                last
            };
            (start, end)
        };
        if start > end.min(last) {
            return;
        }
        let y = self.geometry.bounds.top() + self.geometry.cell.height * row as f32;
        let bounds = self.snapped_cells(start, end.min(last) + 1, y);
        self.under_text.push(fill(bounds, self.theme.selection()));
    }

    /// Where and how the cursor paints, following xterm's DOM renderer: focused cursors use
    /// the terminal's shape and blink; unfocused ones draw a 1px outline.
    fn cursor(
        &self,
        shape: CursorShape,
        point: GridPoint,
        display_offset: i32,
    ) -> Option<CursorPaint> {
        if shape == CursorShape::Hidden {
            return None;
        }
        let row = point.line.0 + display_offset;
        if !(0..self.geometry.rows as i32).contains(&row) || point.column.0 >= self.geometry.cols {
            return None;
        }
        let term = self.view.session().term();
        let wide = term.grid()[point].flags.contains(Flags::WIDE_CHAR);
        let y = self.geometry.bounds.top() + self.geometry.cell.height * row as f32;
        let bounds =
            self.snapped_cells(point.column.0, point.column.0 + if wide { 2 } else { 1 }, y);
        let visible = !term.cursor_style().blinking || self.view.blink_on();
        let shape = if !self.focused {
            CursorPaintShape::Outline
        } else {
            match shape {
                CursorShape::Beam => CursorPaintShape::Bar,
                CursorShape::Underline => CursorPaintShape::Underline,
                _ => CursorPaintShape::Block,
            }
        };
        // The block's blink keyframes swap the glyph between the accent color (on) and the
        // cursor color (off).
        let text_color = (shape == CursorPaintShape::Block).then(|| {
            if visible {
                self.theme.cursor_accent()
            } else {
                self.theme.cursor()
            }
        });
        Some(CursorPaint {
            point,
            bounds,
            shape,
            visible,
            text_color,
        })
    }

    fn paint_cursor(&mut self, cursor: &CursorPaint) {
        let color = self.theme.cursor();
        let bounds = cursor.bounds;
        let one = px(1.);
        match cursor.shape {
            CursorPaintShape::Outline => {
                self.over_text
                    .push(outline(bounds, color, BorderStyle::Solid));
            }
            _ if !cursor.visible => {}
            CursorPaintShape::Block => self.under_text.push(fill(bounds, color)),
            CursorPaintShape::Bar => {
                self.over_text.push(fill(
                    Bounds::new(bounds.origin, size(one, bounds.size.height)),
                    color,
                ));
            }
            CursorPaintShape::Underline => {
                let origin = point(bounds.left(), bounds.bottom() - one);
                self.over_text.push(fill(
                    Bounds::new(origin, size(bounds.size.width, one)),
                    color,
                ));
            }
        }
    }

    /// IME composition text over the cursor, underlined on the terminal background.
    fn marked_text(
        &self,
        cursor: Option<&CursorPaint>,
    ) -> Option<(PaintQuad, Point<Pixels>, ShapedLine)> {
        let text = self.view.marked_text()?;
        let origin = cursor?.bounds.origin;
        let color = self.theme.foreground();
        let run = TextRun {
            len: text.len(),
            font: self.fonts[0].clone(),
            color,
            background_color: None,
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: None,
        };
        let line =
            self.window
                .text_system()
                .shape_line(text.to_owned().into(), FONT_SIZE, &[run], None);
        let background = fill(
            Bounds::new(origin, size(line.width(), self.geometry.cell.height)),
            self.theme.background(),
        );
        Some((background, origin, line))
    }

    fn scrollbar(&self) -> Option<PaintQuad> {
        let scrollbar = self.geometry.scrollbar?;
        let (hovered, dragging) = self.view.scrollbar_state()?;
        Some(quad(
            scrollbar.thumb,
            Corners::all(px(3.)),
            self.theme.scrollbar_slider(hovered, dragging),
            Edges::default(),
            transparent_black(),
            BorderStyle::default(),
        ))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorPaintShape {
    Block,
    Bar,
    Underline,
    Outline,
}

struct CursorPaint {
    point: GridPoint,
    bounds: Bounds<Pixels>,
    shape: CursorPaintShape,
    /// The current blink phase (always true for steady cursors).
    visible: bool,
    /// Glyph color override for the cell under a block cursor.
    text_color: Option<Hsla>,
}

/// The cell's character plus any combining characters.
fn cell_text(cell: &Cell) -> String {
    let mut text = String::from(cell.c);
    if let Some(extra) = cell.zerowidth() {
        text.extend(extra);
    }
    text
}
