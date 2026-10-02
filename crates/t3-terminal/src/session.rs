//! Terminal state without any UI: an `alacritty_terminal::Term` plus its VT parser, fed with
//! the server's output strings. The view owns one per terminal and renders from it.

use std::{cell::RefCell, rc::Rc, time::Instant};

use alacritty_terminal::{
    Term,
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Line, Point, Side},
    selection::{Selection, SelectionType},
    term::{
        Config, Osc52,
        cell::{Cell, Flags},
    },
    vte::ansi::{CursorShape, CursorStyle, Processor, Rgb},
};

use crate::links::{self, LinkMatch};

/// Lines of scrollback, matching the drawer's xterm `scrollback: 5000`.
pub(crate) const SCROLLBACK_LINES: usize = 5000;

/// xterm's default `wordSeparator`, used for double-click word selection.
const WORD_SEPARATORS: &str = " ()[]{}',\"`";

/// Grid size in cells. Implements alacritty's `Dimensions` for `Term::new` and `resize`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

/// Requests the terminal makes of its host while parsing output.
pub(crate) enum TermRequest {
    /// Bytes for the PTY (device status and attribute replies).
    Reply(String),
    /// OSC 4/10/11/12 color query: the palette index and the reply formatter.
    Color(usize, std::sync::Arc<dyn Fn(Rgb) -> String + Send + Sync>),
}

/// Collects alacritty events; `Term` only hands them to its listener.
#[derive(Clone, Default)]
pub(crate) struct Listener(Rc<RefCell<Vec<Event>>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::PtyWrite(_) | Event::ColorRequest(..)) {
            self.0.borrow_mut().push(event);
        }
    }
}

/// A logical line: soft-wrapped rows joined, with each character's grid cell.
pub(crate) struct LogicalLine {
    pub text: String,
    /// `(byte offset in text, cell)` for every cell that contributed characters, in order.
    cells: Vec<(usize, Point)>,
}

impl LogicalLine {
    /// The cell holding the character at byte `offset`.
    fn cell_at_byte(&self, offset: usize) -> Option<Point> {
        let index = self.cells.partition_point(|(start, _)| *start <= offset);
        self.cells
            .get(index.checked_sub(1)?)
            .map(|(_, point)| *point)
    }

    /// Byte offset of the first character in `cell`.
    fn byte_at_cell(&self, cell: Point) -> Option<usize> {
        self.cells
            .iter()
            .find(|(_, point)| *point == cell)
            .map(|(start, _)| *start)
    }
}

/// A link under a cell, with the grid range it covers (inclusive).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkHit {
    pub link: LinkMatch,
    pub start: Point,
    pub end: Point,
}

/// Selected text as the fork's "Add to chat" payload (`ThreadTerminalDrawer.tsx:432-460`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectionPayload {
    pub text: String,
    /// 1-based buffer line of the selection start (0 is the oldest scrollback line).
    pub line_start: usize,
    pub line_end: usize,
}

/// One terminal's emulator state.
pub(crate) struct TerminalSession {
    term: Term<Listener>,
    parser: Processor,
    listener: Listener,
}

impl TerminalSession {
    pub(crate) fn new(size: GridSize) -> Self {
        let listener = Listener::default();
        Self {
            term: Term::new(config(), &size, listener.clone()),
            parser: Processor::new(),
            listener,
        }
    }

    pub(crate) fn term(&self) -> &Term<Listener> {
        &self.term
    }

    pub(crate) fn term_mut(&mut self) -> &mut Term<Listener> {
        &mut self.term
    }

    pub(crate) fn size(&self) -> GridSize {
        GridSize {
            cols: self.term.columns(),
            rows: self.term.screen_lines(),
        }
    }

    /// Parses live output. Replies the program asked for are queued for `take_requests`.
    pub(crate) fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    /// Replaces all state with `history` (the attach snapshot). Queries inside the history were
    /// answered when they first ran, so replies produced by the replay are dropped.
    pub(crate) fn replay(&mut self, history: &str) {
        self.reset();
        self.parser.advance(&mut self.term, history.as_bytes());
        self.listener.0.borrow_mut().clear();
    }

    /// A fresh terminal at the current size (xterm RIS: content, scrollback, modes, colors).
    pub(crate) fn reset(&mut self) {
        let size = self.size();
        self.listener.0.borrow_mut().clear();
        self.term = Term::new(config(), &size, self.listener.clone());
        self.parser = Processor::new();
    }

    /// Requests produced since the last call.
    pub(crate) fn take_requests(&mut self) -> Vec<TermRequest> {
        self.listener
            .0
            .borrow_mut()
            .drain(..)
            .filter_map(|event| match event {
                Event::PtyWrite(text) => Some(TermRequest::Reply(text)),
                Event::ColorRequest(index, format) => Some(TermRequest::Color(index, format)),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn resize(&mut self, size: GridSize) {
        if size != self.size() {
            self.term.resize(size);
        }
    }

    /// When a synchronized update (DEC 2026) is buffering output, the time it must be flushed.
    pub(crate) fn sync_deadline(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    /// Ends a synchronized update whose deadline passed, applying the buffered output.
    pub(crate) fn flush_sync(&mut self) {
        self.parser.stop_sync(&mut self.term);
    }

    /// The logical line containing grid `line`: joins rows linked by soft wraps. Trailing blanks
    /// are dropped only from the last row, like xterm's `translateToString(trimRight)`.
    pub(crate) fn logical_line(&self, line: Line) -> LogicalLine {
        let grid = self.term.grid();
        let last_column = grid.last_column();
        let wraps = |line: Line| grid[line][last_column].flags.contains(Flags::WRAPLINE);
        let mut start = line;
        while start > grid.topmost_line() && wraps(start - 1) {
            start -= 1;
        }

        let mut text = String::new();
        let mut cells = Vec::new();
        let mut line = start;
        loop {
            let row_start = text.len();
            let row_cells = cells.len();
            for column in 0..grid.columns() {
                let point = Point::new(line, Column(column));
                let cell: &Cell = &grid[point];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                cells.push((text.len(), point));
                text.push(cell.c);
                if let Some(extra) = cell.zerowidth() {
                    text.extend(extra);
                }
            }
            if wraps(line) && line < grid.bottommost_line() {
                line += 1;
                continue;
            }
            let trimmed = text[row_start..].trim_end_matches(' ').len() + row_start;
            text.truncate(trimmed);
            let keep = cells[row_cells..].partition_point(|(start, _)| *start < trimmed);
            cells.truncate(row_cells + keep);
            break;
        }
        LogicalLine { text, cells }
    }

    /// The URL or path link covering `point`, if any.
    pub(crate) fn link_at(&self, point: Point) -> Option<LinkHit> {
        let line = self.logical_line(point.line);
        let offset = line.byte_at_cell(point)?;
        let link = links::extract_links(&line.text)
            .into_iter()
            .find(|link| (link.start..link.end).contains(&offset))?;
        let start = line.cell_at_byte(link.start)?;
        let end = line.cell_at_byte(link.end - 1)?;
        Some(LinkHit { link, start, end })
    }

    /// Starts a selection at `point`. Click count picks the kind: 1 simple, 2 word, 3 line.
    pub(crate) fn start_selection(&mut self, point: Point, side: Side, click_count: usize) {
        let kind = match click_count {
            0 | 1 => SelectionType::Simple,
            2 => SelectionType::Semantic,
            _ => SelectionType::Lines,
        };
        self.term.selection = Some(Selection::new(kind, point, side));
    }

    /// Moves the selection's moving end. Returns whether a selection exists.
    pub(crate) fn update_selection(&mut self, point: Point, side: Side) -> bool {
        match &mut self.term.selection {
            Some(selection) => {
                selection.update(point, side);
                true
            }
            None => false,
        }
    }

    /// Selects every line of the buffer (xterm `selectAll`).
    pub(crate) fn select_all(&mut self) {
        let start = Point::new(self.term.topmost_line(), Column(0));
        let end = Point::new(self.term.bottommost_line(), self.term.last_column());
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        self.term.selection = Some(selection);
    }

    pub(crate) fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.term
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
    }

    /// Selected text (for copy).
    pub(crate) fn selection_text(&self) -> Option<String> {
        self.has_selection()
            .then(|| self.term.selection_to_string())
            .flatten()
    }

    /// The fork's "Add to chat" payload: CRLF normalized, surrounding newlines trimmed, and
    /// 1-based buffer lines. `None` when nothing non-blank is selected.
    pub(crate) fn selection_payload(&self) -> Option<SelectionPayload> {
        let range = self.term.selection.as_ref()?.to_range(&self.term)?;
        let text = self.selection_text()?.replace("\r\n", "\n");
        let text = text.trim_matches('\n');
        if text.is_empty() {
            return None;
        }
        let line_start = (range.start.line.0 + self.term.history_size() as i32) as usize + 1;
        let line_count = text.split('\n').count();
        Some(SelectionPayload {
            text: text.to_owned(),
            line_start,
            line_end: line_start + line_count - 1,
        })
    }
}

fn config() -> Config {
    Config {
        scrolling_history: SCROLLBACK_LINES,
        default_cursor_style: CursorStyle {
            shape: CursorShape::Block,
            blinking: true,
        },
        semantic_escape_chars: WORD_SEPARATORS.to_owned(),
        osc52: Osc52::Disabled,
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes covered, listed before the tests were written:
    //! 1. Replaying a snapshot whose history contains a status query (`CSI 6n`) answers it
    //!    again, injecting stale replies into the shell; live output must still be answered.
    //! 2. A snapshot leaves earlier content, scrollback or modes (alt screen, bracketed paste)
    //!    behind.
    //! 3. An escape sequence split across two output chunks is lost or printed.
    //! 4. Scrollback grows past 5000 lines.
    //! 5. Link lookup ignores soft wraps (a URL wrapped onto the next row is cut), joins hard
    //!    newlines, or maps the range to the wrong cells after wide characters.
    //! 6. The "Add to chat" payload keeps CRLF or surrounding blank lines, or reports viewport
    //!    rows instead of 1-based buffer lines.
    use alacritty_terminal::term::TermMode;

    use super::*;

    fn session(cols: usize, rows: usize) -> TerminalSession {
        TerminalSession::new(GridSize { cols, rows })
    }

    fn row_text(session: &TerminalSession, line: i32) -> String {
        session.logical_line(Line(line)).text
    }

    fn replies(session: &mut TerminalSession) -> Vec<String> {
        session
            .take_requests()
            .into_iter()
            .filter_map(|request| match request {
                TermRequest::Reply(text) => Some(text),
                TermRequest::Color(..) => None,
            })
            .collect()
    }

    #[test]
    fn replay_does_not_answer_old_queries() {
        let mut s = session(20, 5);
        s.replay("$ \x1b[6n");
        assert!(replies(&mut s).is_empty());
        s.advance(b"\x1b[6n");
        assert_eq!(replies(&mut s), ["\x1b[1;3R"]);
    }

    #[test]
    fn replay_replaces_everything() {
        let mut s = session(20, 3);
        s.advance(b"old\r\n1\r\n2\r\n3\r\n4\x1b[?2004h\x1b[?1049h");
        assert!(
            s.term()
                .mode()
                .contains(TermMode::ALT_SCREEN | TermMode::BRACKETED_PASTE)
        );
        s.replay("new");
        assert_eq!(row_text(&s, 0), "new");
        assert_eq!(s.term().history_size(), 0);
        assert!(
            !s.term()
                .mode()
                .intersects(TermMode::ALT_SCREEN | TermMode::BRACKETED_PASTE)
        );
    }

    #[test]
    fn split_escape_sequences_apply() {
        let mut s = session(20, 3);
        s.advance(b"a\x1b[3");
        s.advance(b"1mb");
        assert_eq!(row_text(&s, 0), "ab");
        let cell = &s.term().grid()[Point::new(Line(0), Column(1))];
        assert_eq!(
            cell.fg,
            alacritty_terminal::vte::ansi::Color::Named(
                alacritty_terminal::vte::ansi::NamedColor::Red
            )
        );
    }

    #[test]
    fn scrollback_is_capped() {
        let mut s = session(10, 4);
        let output: String = (0..6000).map(|i| format!("{i}\r\n")).collect();
        s.advance(output.as_bytes());
        assert_eq!(s.term().history_size(), SCROLLBACK_LINES);
    }

    #[test]
    fn links_follow_soft_wraps_only() {
        let mut s = session(10, 4);
        s.advance("✓ https://t3.dev/a\r\nnext/line".as_bytes());
        // "✓ https://" fills row 0 (✓ is narrow), "t3.dev/a" wraps onto row 1.
        let hit = s
            .link_at(Point::new(Line(1), Column(3)))
            .expect("link on wrapped row");
        assert_eq!(hit.link.text, "https://t3.dev/a");
        assert_eq!(hit.start, Point::new(Line(0), Column(2)));
        assert_eq!(hit.end, Point::new(Line(1), Column(7)));
        let path = s
            .link_at(Point::new(Line(2), Column(0)))
            .expect("path on next line");
        assert_eq!(path.link.text, "next/line");
        assert_eq!(s.link_at(Point::new(Line(1), Column(9))), None);
    }

    #[test]
    fn links_after_wide_characters() {
        let mut s = session(20, 2);
        s.advance("日本 ./a.rs".as_bytes());
        let hit = s.link_at(Point::new(Line(0), Column(6))).expect("link");
        assert_eq!(hit.link.text, "./a.rs");
        assert_eq!(hit.start, Point::new(Line(0), Column(5)));
        assert_eq!(hit.end, Point::new(Line(0), Column(10)));
    }

    #[test]
    fn selection_payload_uses_buffer_lines() {
        let mut s = session(10, 3);
        s.advance(b"a\r\nb\r\nc\r\nd\r\n");
        // Two lines scrolled into history: "a" is buffer line 1, "c" is the first visible row.
        assert_eq!(s.term().history_size(), 2);
        s.start_selection(Point::new(Line(-1), Column(0)), Side::Left, 1);
        s.update_selection(Point::new(Line(1), Column(9)), Side::Right);
        let payload = s.selection_payload().expect("payload");
        assert_eq!(payload.text, "b\nc\nd");
        assert_eq!((payload.line_start, payload.line_end), (2, 4));
        s.clear_selection();
        assert_eq!(s.selection_payload(), None);
    }
}
