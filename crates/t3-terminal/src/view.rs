//! `TerminalView`: one terminal's emulator state, input handling and events. The app feeds it
//! server output and forwards its events to the terminal RPCs; `TerminalElement` paints it.

use std::{
    ops::Range,
    time::{Duration, Instant},
};

use alacritty_terminal::{
    grid::{Dimensions, Scroll},
    index::{Column, Line, Point as GridPoint, Side},
    term::TermMode,
};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    IntoElement, KeyBinding, KeyDownEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent, NoAction,
    ParentElement as _, Pixels, Point, Render, ScrollWheelEvent, SharedString, Size, Styled as _,
    Task, UTF16Selection, Window, div, point, px, size,
};
use gpui_kit::{InteractiveElement as _, MouseButton as GpuiMouseButton};

use crate::{
    element::TerminalElement,
    input::{self, KeyOutcome, Modifiers, MouseAction, MouseButton, MouseProtocol, Platform},
    links::TerminalLinkKind,
    session::{GridSize, LinkHit, TermRequest, TerminalSession},
    theme::TerminalTheme,
};
use t3_ui::{ActiveColors as _, Theme};

/// Font size of the drawer's xterm (`fontSize: 12`, `lineHeight: 1`).
pub(crate) const FONT_SIZE: Pixels = px(12.);
/// Width FitAddon reserves for xterm's scrollbar whenever scrollback is enabled.
pub(crate) const FIT_SCROLLBAR_RESERVE: Pixels = px(14.);
/// xterm's DOM cursor blink: a 1s `step-end` animation, so 500ms on and 500ms off.
const BLINK_INTERVAL: Duration = Duration::from_millis(500);
/// VS Code scrollable element `HIDE_TIMEOUT` after scrolling or mouse leave.
const SCROLLBAR_HIDE_DELAY: Duration = Duration::from_millis(500);
/// Delay before the "Add to chat" menu after a double/triple click (`MULTI_CLICK_...`).
const MULTI_CLICK_MENU_DELAY: Duration = Duration::from_millis(260);
/// `terminal.write` accepts at most 65,536 UTF-16 units; chunks stay well under that.
const MAX_INPUT_CHARS: usize = 16_384;

/// Key context of a focused terminal, for app bindings that must not fire inside it.
pub const KEY_CONTEXT: &str = "Terminal";

/// Registers the terminal's key bindings. Call once at startup, after `gpui_kit::init`.
///
/// Bindings dispatch before the view's key handler, and gpui-kit's `Root` binds Tab and
/// Shift+Tab to focus navigation and the copy shortcut (Cmd+C, or Ctrl+C off macOS, which is
/// SIGINT in a shell). Inside a terminal those keys belong to the PTY, so they are unbound in
/// [`KEY_CONTEXT`]. App shortcuts bound elsewhere still win, which is how the fork lets
/// `terminal.*` and `diff.toggle` bubble out of xterm.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NoAction, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(KEY_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", NoAction, Some(KEY_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", NoAction, Some(KEY_CONTEXT)),
    ]);
}

/// Events the app wires to RPCs and menus.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalEvent {
    /// Bytes for the PTY: send with `terminal.write {threadId, terminalId, data}`.
    Input(String),
    /// The grid size changed: send `terminal.resize {threadId, terminalId, cols, rows}`. The
    /// latest value wins, so the app may coalesce.
    Resize { cols: u16, rows: u16 },
    /// Cmd+click (Ctrl+click off macOS) on a link. URLs get the preview/browser menu; paths
    /// resolve with [`crate::resolve_path_link_target`] and open in the editor.
    LinkActivated {
        kind: TerminalLinkKind,
        text: String,
        position: Point<Pixels>,
    },
    /// A left-button selection finished: show the "Add to chat" menu at `position` (window
    /// coordinates, unclamped). Lines are 1-based buffer lines; text has LF line endings.
    SelectionMenuRequested {
        text: String,
        line_start: usize,
        line_end: usize,
        position: Point<Pixels>,
    },
}

/// Cell size for the current font and display scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellMetrics {
    pub cell: Size<Pixels>,
    pub scale: f32,
}

/// Where the grid was last laid out, for mapping pointer positions to cells.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GridGeometry {
    pub bounds: Bounds<Pixels>,
    pub cell: Size<Pixels>,
    pub cols: usize,
    pub rows: usize,
    pub scrollbar: Option<ScrollbarGeometry>,
}

/// The overlay scrollbar's track and slider, when the buffer has scrollback.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollbarGeometry {
    pub track: Bounds<Pixels>,
    pub thumb: Bounds<Pixels>,
    /// Slider pixels per pixel of scrolled content.
    pub ratio: f32,
}

impl GridGeometry {
    /// Viewport cell under `position`, clamped to the grid, and which half of it was hit.
    fn viewport_cell(&self, position: Point<Pixels>) -> (usize, usize, Side) {
        let x = (position.x - self.bounds.origin.x) / self.cell.width;
        let y = (position.y - self.bounds.origin.y) / self.cell.height;
        let col = (x.max(0.) as usize).min(self.cols - 1);
        let row = (y.max(0.) as usize).min(self.rows - 1);
        let side = if x >= self.cols as f32 || x.fract() >= 0.5 {
            Side::Right
        } else {
            Side::Left
        };
        (col, row, side)
    }

    /// Bounds of a viewport cell in window coordinates.
    pub(crate) fn cell_bounds(&self, col: usize, row: usize, width_cells: usize) -> Bounds<Pixels> {
        Bounds::new(
            self.bounds.origin + point(self.cell.width * col as f32, self.cell.height * row as f32),
            size(self.cell.width * width_cells as f32, self.cell.height),
        )
    }
}

enum Drag {
    /// Selecting text with the left button.
    Select,
    /// Dragging the scrollbar slider, grabbed this far below its top.
    Scrollbar { grab: Pixels },
    /// A button press was reported to the application.
    Report,
}

/// A terminal session's view. Create one per `(thread, terminalId)` and keep it alive while
/// the thread's drawer is hidden so scrollback survives.
///
/// Feed it the `terminal.attach` stream: `snapshot`/`restarted` → [`Self::feed_snapshot`],
/// `output` → [`Self::feed_output`], `cleared` → [`Self::reset`], and status changes →
/// [`Self::write_system_message`]. Subscribe to [`TerminalEvent`] for input, resize, links and
/// the selection menu.
pub struct TerminalView {
    session: TerminalSession,
    focus_handle: FocusHandle,
    font_family: SharedString,
    metrics: Option<CellMetrics>,
    geometry: Option<GridGeometry>,
    /// Whether the cursor is in its visible blink phase.
    blink_on: bool,
    blink_task: Option<Task<()>>,
    /// Whether `blink_task` is still ticking; it stops itself once the view is not painted.
    blinking: bool,
    /// Set by paint and cleared by each blink tick, so blinking stops while hidden.
    painted_since_blink: bool,
    drag: Option<Drag>,
    /// A Cmd+pressed link, activated if the button is released on it.
    pending_link: Option<LinkHit>,
    hovered_link: Option<LinkHit>,
    hovered: bool,
    scrollbar_hovered: bool,
    scrollbar_revealed_until: Option<Instant>,
    scrollbar_hide_task: Option<Task<()>>,
    /// Sub-line wheel distance not yet scrolled, in pixels.
    scroll_remainder: f32,
    /// xterm `_wheelPartialScroll` for alt-screen arrows and wheel reports, in lines.
    wheel_partial: f32,
    last_motion_report: Option<(usize, usize)>,
    /// IME composition text, shown at the cursor until committed.
    marked_text: Option<String>,
    selection_menu_task: Option<Task<()>>,
    sync_task: Option<Task<()>>,
    focused: bool,
    _subscriptions: Vec<gpui_kit::Subscription>,
}

impl EventEmitter<TerminalEvent> for TerminalView {}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    /// A blank 80x24 terminal (xterm's size before the first fit).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, |this, window, cx| {
                this.sync_focus(window, cx)
            }),
            cx.on_blur(&focus_handle, window, |this, window, cx| {
                this.sync_focus(window, cx)
            }),
            cx.observe_window_activation(window, |this, window, cx| this.sync_focus(window, cx)),
        ];
        Self {
            session: TerminalSession::new(GridSize { cols: 80, rows: 24 }),
            focus_handle,
            font_family: Theme::global(cx).mono_family().clone(),
            metrics: None,
            geometry: None,
            blink_on: true,
            blink_task: None,
            blinking: false,
            painted_since_blink: false,
            drag: None,
            pending_link: None,
            hovered_link: None,
            hovered: false,
            scrollbar_hovered: false,
            scrollbar_revealed_until: None,
            scrollbar_hide_task: None,
            scroll_remainder: 0.,
            wheel_partial: 0.,
            last_motion_report: None,
            marked_text: None,
            selection_menu_task: None,
            sync_task: None,
            focused: false,
            _subscriptions: subscriptions,
        }
    }

    /// Replaces all content with the attach snapshot's `history` (also used for `restarted`).
    pub fn feed_snapshot(&mut self, history: &str, cx: &mut Context<Self>) {
        self.session.replay(history);
        self.content_changed(cx);
    }

    /// Appends live output. Like the fork, any output clears the selection.
    pub fn feed_output(&mut self, data: &str, cx: &mut Context<Self>) {
        self.session.advance(data.as_bytes());
        self.session.clear_selection();
        for request in self.session.take_requests() {
            let reply = match request {
                TermRequest::Reply(text) => text,
                TermRequest::Color(index, format) => {
                    format(TerminalTheme::new(cx.colors()).query_color(index))
                }
            };
            cx.emit(TerminalEvent::Input(reply));
        }
        self.schedule_sync_flush(cx);
        self.content_changed(cx);
    }

    /// Clears content, scrollback and modes (`cleared` event; xterm RIS).
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.session.reset();
        self.content_changed(cx);
    }

    /// Prints `[terminal] {message}` on its own line, as the fork does for write failures,
    /// stream errors, and `Process exited` / `Terminal closed`.
    pub fn write_system_message(&mut self, message: &str, cx: &mut Context<Self>) {
        self.feed_output(&format!("\r\n[terminal] {message}\r\n"), cx);
    }

    /// Grid size as `(cols, rows)`, the values last sent in [`TerminalEvent::Resize`].
    pub fn grid_size(&self) -> (u16, u16) {
        let size = self.session.size();
        (size.cols as u16, size.rows as u16)
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.session.clear_selection();
        self.selection_menu_task = None;
        cx.notify();
    }

    /// Window position of viewport cell `(col, row)`'s top-left corner, once the view has
    /// been laid out. For anchoring popovers to terminal content.
    pub fn cell_origin(&self, col: usize, row: usize) -> Option<Point<Pixels>> {
        Some(self.geometry?.cell_bounds(col, row, 1).origin)
    }

    pub(crate) fn session(&self) -> &TerminalSession {
        &self.session
    }

    pub(crate) fn font_family(&self) -> &SharedString {
        &self.font_family
    }

    pub(crate) fn focus_handle_ref(&self) -> &FocusHandle {
        &self.focus_handle
    }

    pub(crate) fn blink_on(&self) -> bool {
        self.blink_on
    }

    pub(crate) fn hovered_link(&self) -> Option<&LinkHit> {
        self.hovered_link.as_ref()
    }

    pub(crate) fn marked_text(&self) -> Option<&str> {
        self.marked_text.as_deref()
    }

    /// Whether the scrollbar slider shows: on hover, while dragging, and briefly after a scroll.
    pub(crate) fn scrollbar_state(&self) -> Option<(bool, bool)> {
        let dragging = matches!(self.drag, Some(Drag::Scrollbar { .. }));
        let revealed = self
            .scrollbar_revealed_until
            .is_some_and(|until| Instant::now() < until);
        (self.hovered || dragging || revealed).then_some((self.scrollbar_hovered, dragging))
    }

    /// Whether mouse events go to the application instead of selection (DECSET 1000-1003).
    /// xterm only lets Shift force a selection off macOS.
    pub(crate) fn reports_mouse(&self, modifiers: gpui_kit::Modifiers) -> bool {
        self.session.term().mode().intersects(TermMode::MOUSE_MODE)
            && !(Platform::current() == Platform::Other && modifiers.shift)
    }

    /// Measures cells, fits the grid to `bounds` like `@xterm/addon-fit`, and emits a resize
    /// when the size changed. Called by the element before painting.
    pub(crate) fn layout_grid(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> GridGeometry {
        let scale = window.scale_factor();
        let metrics = match self.metrics {
            Some(metrics) if metrics.scale == scale => metrics,
            _ => {
                let metrics = measure_cell(&self.font_family, window);
                self.metrics = Some(metrics);
                metrics
            }
        };
        let cell = metrics.cell;
        let cols =
            (((bounds.size.width - FIT_SCROLLBAR_RESERVE) / cell.width).floor() as usize).max(2);
        let rows = ((bounds.size.height / cell.height).floor() as usize).max(1);
        if self.session.size() != (GridSize { cols, rows }) {
            self.session.resize(GridSize { cols, rows });
            self.hovered_link = None;
            cx.emit(TerminalEvent::Resize {
                cols: cols as u16,
                rows: rows as u16,
            });
        }
        let geometry = GridGeometry {
            bounds,
            cell,
            cols,
            rows,
            scrollbar: self.scrollbar_geometry(bounds, cell, rows),
        };
        self.geometry = Some(geometry);
        geometry
    }

    /// Called after painting: catches focus changes the listeners have not reported yet and
    /// restarts blinking if it stopped while the view was hidden.
    pub(crate) fn did_paint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_focus(window, cx);
        self.painted_since_blink = true;
        if !self.blinking && self.should_blink() {
            self.restart_blink(cx);
        }
    }

    fn scrollbar_geometry(
        &self,
        bounds: Bounds<Pixels>,
        cell: Size<Pixels>,
        rows: usize,
    ) -> Option<ScrollbarGeometry> {
        // VS Code ScrollbarState (MIT): slider size and position over the visible height.
        let term = self.session.term();
        let history = term.history_size();
        if history == 0 {
            return None;
        }
        let line = f32::from(cell.height);
        let visible = line * rows as f32;
        let scroll_size = line * (history + rows) as f32;
        let slider = (visible * visible / scroll_size)
            .floor()
            .max(20.)
            .round()
            .min(visible);
        let ratio = (visible - slider) / (scroll_size - visible);
        let scroll_top = line * (history - term.grid().display_offset()) as f32;
        let track = Bounds::new(
            point(bounds.right() - px(6.), bounds.top()),
            size(px(6.), px(visible)),
        );
        let thumb = Bounds::new(
            point(track.left(), track.top() + px(scroll_top * ratio)),
            size(px(6.), px(slider)),
        );
        Some(ScrollbarGeometry {
            track,
            thumb,
            ratio,
        })
    }

    fn content_changed(&mut self, cx: &mut Context<Self>) {
        self.hovered_link = None;
        self.pending_link = None;
        self.restart_blink(cx);
        cx.notify();
    }

    /// Sends user-originated input: like xterm, it clears the selection and scrolls to the
    /// bottom first.
    fn user_input(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.session.clear_selection();
        self.selection_menu_task = None;
        self.session.term_mut().scroll_display(Scroll::Bottom);
        let mut rest = text;
        while !rest.is_empty() {
            let split = rest
                .char_indices()
                .nth(MAX_INPUT_CHARS)
                .map_or(rest.len(), |(i, _)| i);
            cx.emit(TerminalEvent::Input(rest[..split].to_owned()));
            rest = &rest[split..];
        }
        self.restart_blink(cx);
        cx.notify();
    }

    fn sync_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focused = self.focus_handle.is_focused(window) && window.is_window_active();
        if focused == self.focused {
            return;
        }
        self.focused = focused;
        if self.session.term().mode().contains(TermMode::FOCUS_IN_OUT) {
            cx.emit(TerminalEvent::Input(
                if focused { "\x1b[I" } else { "\x1b[O" }.into(),
            ));
        }
        self.session.term_mut().is_focused = focused;
        self.restart_blink(cx);
        cx.notify();
    }

    fn should_blink(&self) -> bool {
        self.focused && self.session.term().cursor_style().blinking
    }

    /// Shows the cursor and restarts its blink cycle (xterm re-renders the cursor on change,
    /// restarting the CSS animation). The timer only runs while focused and painted.
    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_on = true;
        self.blink_task = None;
        self.blinking = self.should_blink();
        if !self.blinking {
            return;
        }
        self.painted_since_blink = true;
        self.blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let keep_going = this.update(cx, |view, cx| {
                    if !view.painted_since_blink || !view.should_blink() {
                        view.blink_on = true;
                        view.blinking = false;
                        return false;
                    }
                    view.painted_since_blink = false;
                    view.blink_on = !view.blink_on;
                    cx.notify();
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// Flushes a synchronized update (DEC 2026) if the program never ends it.
    fn schedule_sync_flush(&mut self, cx: &mut Context<Self>) {
        if self.sync_task.is_some() || self.session.sync_deadline().is_none() {
            return;
        }
        self.sync_task = Some(cx.spawn(async move |this, cx| {
            // Sleeps until each pending deadline; ends once no update is buffering.
            while let Ok(Some(deadline)) =
                this.read_with(cx, |view, _| view.session.sync_deadline())
            {
                let wait = deadline.saturating_duration_since(Instant::now());
                cx.background_executor().timer(wait).await;
                let flushed = this.update(cx, |view, cx| {
                    if view
                        .session
                        .sync_deadline()
                        .is_some_and(|d| d <= Instant::now())
                    {
                        view.session.flush_sync();
                        cx.notify();
                    }
                });
                if flushed.is_err() {
                    return;
                }
            }
            this.update(cx, |view, _| view.sync_task = None).ok();
        }));
    }

    fn reveal_scrollbar(&mut self, cx: &mut Context<Self>) {
        self.scrollbar_revealed_until = Some(Instant::now() + SCROLLBAR_HIDE_DELAY);
        self.scrollbar_hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SCROLLBAR_HIDE_DELAY).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    fn scroll_lines(&mut self, lines: i32, cx: &mut Context<Self>) {
        if lines != 0 {
            self.session.term_mut().scroll_display(Scroll::Delta(lines));
            self.hovered_link = None;
            cx.notify();
        }
    }

    pub(crate) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let mods = modifiers(&keystroke.modifiers);
        let platform = Platform::current();
        let (copy, paste) = match platform {
            Platform::Mac => {
                let cmd_only = Modifiers {
                    platform: true,
                    ..Modifiers::default()
                };
                (
                    mods == cmd_only && keystroke.key == "c",
                    mods == cmd_only && keystroke.key == "v",
                )
            }
            Platform::Other => {
                let ctrl_shift = Modifiers {
                    control: true,
                    shift: true,
                    ..Modifiers::default()
                };
                (
                    mods == ctrl_shift && keystroke.key == "c",
                    mods == ctrl_shift && keystroke.key == "v",
                )
            }
        };
        if copy {
            if let Some(text) = self.session.selection_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                cx.stop_propagation();
            }
            return;
        }
        if paste {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let bracketed = self
                    .session
                    .term()
                    .mode()
                    .contains(TermMode::BRACKETED_PASTE);
                self.user_input(&input::paste_text(&text, bracketed), cx);
            }
            cx.stop_propagation();
            return;
        }

        let app_cursor = self.session.term().mode().contains(TermMode::APP_CURSOR);
        let page = self.session.size().rows.saturating_sub(1) as i32;
        match input::encode_key(&keystroke.key, mods, app_cursor, platform) {
            Some(KeyOutcome::Send(bytes)) => self.user_input(&bytes, cx),
            Some(KeyOutcome::ScrollPageUp) => self.scroll_lines(page, cx),
            Some(KeyOutcome::ScrollPageDown) => self.scroll_lines(-page, cx),
            Some(KeyOutcome::SelectAll) => {
                self.session.select_all();
                cx.notify();
            }
            None => return,
        }
        cx.stop_propagation();
    }

    pub(crate) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.geometry else {
            return;
        };
        self.selection_menu_task = None;
        window.focus(&self.focus_handle, cx);

        if let Some(scrollbar) = geometry
            .scrollbar
            .filter(|s| s.track.contains(&event.position))
        {
            let grab = if scrollbar.thumb.contains(&event.position) {
                event.position.y - scrollbar.thumb.top()
            } else {
                scrollbar.thumb.size.height / 2.
            };
            self.drag = Some(Drag::Scrollbar { grab });
            self.drag_scrollbar(event.position.y - grab, cx);
            return;
        }
        if self.reports_mouse(event.modifiers) {
            if let Some(button) = report_button(event.button) {
                self.report_mouse(
                    button,
                    MouseAction::Press,
                    event.position,
                    &event.modifiers,
                    cx,
                );
            }
            self.drag = Some(Drag::Report);
            return;
        }
        if event.button != GpuiMouseButton::Left {
            return;
        }
        let (col, row, side) = geometry.viewport_cell(event.position);
        let grid_point = self.grid_point(col, row);
        self.pending_link = self
            .hovered_link
            .clone()
            .filter(|hit| is_link_activation(&event.modifiers) && link_contains(hit, grid_point));
        if event.modifiers.shift && self.session.has_selection() {
            self.session.update_selection(grid_point, side);
        } else {
            self.session
                .start_selection(grid_point, side, event.click_count);
        }
        self.drag = Some(Drag::Select);
        cx.notify();
    }

    pub(crate) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.geometry else {
            return;
        };
        if hovered != self.hovered {
            self.hovered = hovered;
            if !hovered {
                self.reveal_scrollbar(cx);
            }
            cx.notify();
        }
        let over_thumb = geometry
            .scrollbar
            .is_some_and(|s| s.thumb.contains(&event.position));
        if over_thumb != self.scrollbar_hovered {
            self.scrollbar_hovered = over_thumb;
            cx.notify();
        }

        match self.drag {
            Some(Drag::Scrollbar { grab }) => {
                self.drag_scrollbar(event.position.y - grab, cx);
                return;
            }
            Some(Drag::Select) if event.pressed_button == Some(GpuiMouseButton::Left) => {
                // Dragging past the top or bottom edge scrolls one line per move.
                let top = geometry.bounds.top();
                let bottom = top + geometry.cell.height * geometry.rows as f32;
                if event.position.y < top {
                    self.session.term_mut().scroll_display(Scroll::Delta(1));
                } else if event.position.y >= bottom {
                    self.session.term_mut().scroll_display(Scroll::Delta(-1));
                }
                let (col, row, side) = geometry.viewport_cell(event.position);
                let grid_point = self.grid_point(col, row);
                self.session.update_selection(grid_point, side);
                cx.notify();
                return;
            }
            Some(Drag::Report) => {
                if let Some(button) = event.pressed_button.and_then(report_button) {
                    self.report_mouse(
                        button,
                        MouseAction::Move,
                        event.position,
                        &event.modifiers,
                        cx,
                    );
                }
                return;
            }
            _ => {}
        }

        if hovered && self.reports_mouse(event.modifiers) {
            self.report_mouse(
                MouseButton::None,
                MouseAction::Move,
                event.position,
                &event.modifiers,
                cx,
            );
            return;
        }
        let hit = if hovered
            && !geometry
                .scrollbar
                .is_some_and(|s| s.track.contains(&event.position))
        {
            let (col, row, _) = geometry.viewport_cell(event.position);
            self.session.link_at(self.grid_point(col, row))
        } else {
            None
        };
        if hit != self.hovered_link {
            self.hovered_link = hit;
            cx.notify();
        }
    }

    pub(crate) fn on_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else { return };
        let pending_link = self.pending_link.take();
        match drag {
            Drag::Scrollbar { .. } => {
                self.reveal_scrollbar(cx);
                cx.notify();
            }
            Drag::Report => {
                if let Some(button) = report_button(event.button) {
                    self.report_mouse(
                        button,
                        MouseAction::Release,
                        event.position,
                        &event.modifiers,
                        cx,
                    );
                }
            }
            Drag::Select => {
                if event.button != GpuiMouseButton::Left {
                    return;
                }
                if let Some(hit) = pending_link
                    && !self.session.has_selection()
                    && is_link_activation(&event.modifiers)
                    && self.hovered_link.as_ref() == Some(&hit)
                {
                    cx.emit(TerminalEvent::LinkActivated {
                        kind: hit.link.kind,
                        text: hit.link.text,
                        position: event.position,
                    });
                    return;
                }
                if !self.session.has_selection() {
                    return;
                }
                let delay = if event.click_count >= 2 {
                    MULTI_CLICK_MENU_DELAY
                } else {
                    Duration::ZERO
                };
                let position = event.position;
                self.selection_menu_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    this.update(cx, |view, cx| {
                        view.selection_menu_task = None;
                        if let Some(payload) = view.session.selection_payload() {
                            cx.emit(TerminalEvent::SelectionMenuRequested {
                                text: payload.text,
                                line_start: payload.line_start,
                                line_end: payload.line_end,
                                position,
                            });
                        }
                    })
                    .ok();
                }));
            }
        }
    }

    pub(crate) fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(geometry) = self.geometry else {
            return;
        };
        // DOM wheel convention: positive scrolls toward newer output.
        let delta_y = -f32::from(event.delta.pixel_delta(geometry.cell.height).y);
        if delta_y == 0. {
            return;
        }
        let mode = *self.session.term().mode();
        if self.reports_mouse(event.modifiers) || mode.contains(TermMode::ALT_SCREEN) {
            // xterm `consumeWheelEvent`: one report or arrow per event once a whole line accrues.
            if event.modifiers.shift {
                return;
            }
            let mut amount = delta_y / f32::from(geometry.cell.height);
            if delta_y.abs() < 50. {
                amount *= 0.3;
            }
            self.wheel_partial += amount;
            let lines = self.wheel_partial.trunc();
            self.wheel_partial = self.wheel_partial.fract();
            if lines == 0. {
                return;
            }
            let up = delta_y < 0.;
            if self.reports_mouse(event.modifiers) {
                let button = if up {
                    MouseButton::WheelUp
                } else {
                    MouseButton::WheelDown
                };
                self.report_mouse(
                    button,
                    MouseAction::Press,
                    event.position,
                    &event.modifiers,
                    cx,
                );
            } else {
                let arrow = input::wheel_arrow(up, mode.contains(TermMode::APP_CURSOR));
                self.user_input(arrow, cx);
            }
            return;
        }
        self.scroll_remainder += delta_y;
        let lines = (self.scroll_remainder / f32::from(geometry.cell.height)).trunc();
        self.scroll_remainder -= lines * f32::from(geometry.cell.height);
        self.scroll_lines(-(lines as i32), cx);
        self.reveal_scrollbar(cx);
    }

    fn drag_scrollbar(&mut self, thumb_top: Pixels, cx: &mut Context<Self>) {
        let Some(scrollbar) = self.geometry.and_then(|g| g.scrollbar) else {
            return;
        };
        let Some(cell_height) = self.metrics.map(|m| m.cell.height) else {
            return;
        };
        if scrollbar.ratio <= 0. {
            return;
        }
        let term = self.session.term();
        let history = term.history_size() as i32;
        let scroll_top = (thumb_top - scrollbar.track.top()) / scrollbar.ratio;
        let line = ((scroll_top / cell_height).round() as i32).clamp(0, history);
        let target_offset = history - line;
        let current = term.grid().display_offset() as i32;
        self.scroll_lines(target_offset - current, cx);
    }

    fn report_mouse(
        &mut self,
        button: MouseButton,
        action: MouseAction,
        position: Point<Pixels>,
        mods: &gpui_kit::Modifiers,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.geometry else {
            return;
        };
        let mode = *self.session.term().mode();
        let protocol = if mode.contains(TermMode::MOUSE_MOTION) {
            MouseProtocol::Any
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            MouseProtocol::Drag
        } else {
            MouseProtocol::Click
        };
        let (col, row, _) = geometry.viewport_cell(position);
        if action == MouseAction::Move {
            if self.last_motion_report == Some((col, row)) {
                return;
            }
            self.last_motion_report = Some((col, row));
        }
        let mods = Modifiers {
            shift: mods.shift,
            alt: mods.alt,
            control: mods.control,
            platform: false,
        };
        let sgr = mode.contains(TermMode::SGR_MOUSE);
        if let Some(report) = input::encode_mouse(button, action, col, row, mods, protocol, sgr) {
            cx.emit(TerminalEvent::Input(report));
        }
    }

    fn grid_point(&self, col: usize, row: usize) -> GridPoint {
        let offset = self.session.term().grid().display_offset() as i32;
        GridPoint::new(Line(row as i32 - offset), Column(col))
    }

    /// Viewport `(col, row)` of the cursor, if it is on screen.
    fn cursor_cell(&self) -> Option<(usize, usize)> {
        let term = self.session.term();
        let cursor = term.grid().cursor.point;
        let row = cursor.line.0 + term.grid().display_offset() as i32;
        (0..term.screen_lines() as i32)
            .contains(&row)
            .then_some((cursor.column.0, row as usize))
    }
}

impl Render for TerminalView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context(KEY_CONTEXT)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(TerminalElement::new(cx.entity()))
    }
}

/// IME and text input: committed text goes to the PTY; composition shows at the cursor.
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let marked = self.marked_text.as_ref()?;
        let utf16: Vec<u16> = marked.encode_utf16().collect();
        let range = range.start.min(utf16.len())..range.end.min(utf16.len());
        *adjusted_range = Some(range.clone());
        Some(String::from_utf16_lossy(&utf16[range]))
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let len = self
            .marked_text
            .as_ref()
            .map_or(0, |text| text.encode_utf16().count());
        Some(UTF16Selection {
            range: len..len,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_text
            .as_ref()
            .map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked_text.take().is_some() {
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked_text = None;
        self.user_input(text, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        new_text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked_text = (!new_text.is_empty()).then(|| new_text.to_owned());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let geometry = self.geometry?;
        let (col, row) = self.cursor_cell()?;
        Some(geometry.cell_bounds(col, row, 1))
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

fn modifiers(mods: &gpui_kit::Modifiers) -> Modifiers {
    Modifiers {
        control: mods.control,
        alt: mods.alt,
        shift: mods.shift,
        platform: mods.platform,
    }
}

fn report_button(button: GpuiMouseButton) -> Option<MouseButton> {
    match button {
        GpuiMouseButton::Left => Some(MouseButton::Left),
        GpuiMouseButton::Middle => Some(MouseButton::Middle),
        GpuiMouseButton::Right => Some(MouseButton::Right),
        GpuiMouseButton::Navigate(_) => None,
    }
}

/// `isTerminalLinkActivation`: Cmd (not Ctrl) on macOS, Ctrl (not Cmd) elsewhere.
fn is_link_activation(mods: &gpui_kit::Modifiers) -> bool {
    match Platform::current() {
        Platform::Mac => mods.platform && !mods.control,
        Platform::Other => mods.control && !mods.platform,
    }
}

fn link_contains(hit: &LinkHit, point: GridPoint) -> bool {
    hit.start <= point && point <= hit.end
}

/// Cell size the way xterm measures it: the advance of `W`, and the font's ascent + descent
/// rounded up to whole device pixels (`DomRenderer._updateDimensions`, `lineHeight: 1`).
fn measure_cell(family: &SharedString, window: &Window) -> CellMetrics {
    let text_system = window.text_system();
    let font_id = text_system.resolve_font(&gpui_kit::font(family.clone()));
    let width = text_system
        .advance(font_id, FONT_SIZE, 'W')
        .map_or(FONT_SIZE * 0.6, |advance| advance.width);
    let line_box =
        text_system.ascent(font_id, FONT_SIZE) + text_system.descent(font_id, FONT_SIZE).abs();
    let scale = window.scale_factor();
    let height = px((f32::from(line_box) * scale).ceil() / scale);
    CellMetrics {
        cell: size(width, height),
        scale,
    }
}
