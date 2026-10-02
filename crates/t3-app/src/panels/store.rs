//! [`RightPanels`]: the global per-thread right panel state, the shared panel width, and the
//! maximized thread. Views observe it; the chat header, keybindings, and surfaces call its
//! methods (spec 1.2, 1.5, 1.8).

use std::collections::HashMap;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Pixels, px};
use t3_logic::ThreadRef;
use t3_protocol::TurnId;

use super::{
    context::PanelContext,
    model::{SurfaceKind, ThreadPanel},
    thread_detail::ThreadDetail,
};
use crate::state::AppState;

/// Inline layout needs a window wider than this (`RIGHT_PANEL_INLINE_LAYOUT_MEDIA_QUERY`); at or
/// below it the panel is an overlay sheet.
pub const INLINE_MIN_WINDOW: Pixels = px(980.);

/// Default inline width (`PreviewPanelShell.tsx`).
pub const DEFAULT_WIDTH: Pixels = px(540.);
pub const MIN_WIDTH: Pixels = px(360.);
const MAX_WIDTH: Pixels = px(1400.);

/// Right panels of every thread.
pub struct RightPanels {
    app_state: Entity<AppState>,
    threads: HashMap<ThreadRef, ThreadPanel>,
    /// The thread whose panel fills the window (not persisted).
    maximized: Option<ThreadRef>,
    /// Thread detail per thread, shared by its surfaces.
    details: HashMap<ThreadRef, Entity<ThreadDetail>>,
    /// "View diff" requests the diff surface applies when it next renders.
    turn_requests: HashMap<ThreadRef, (TurnId, Option<String>)>,
}

struct GlobalRightPanels(Entity<RightPanels>);

impl Global for GlobalRightPanels {}

impl RightPanels {
    /// The global store, created on first use over the global [`AppState`].
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(global) = cx.try_global::<GlobalRightPanels>() {
            return global.0.clone();
        }
        let app_state = AppState::global(cx);
        let store = cx.new(|cx| {
            cx.observe(&app_state, |_, _, cx| cx.notify()).detach();
            Self {
                app_state,
                threads: HashMap::new(),
                maximized: None,
                details: HashMap::new(),
                turn_requests: HashMap::new(),
            }
        });
        cx.set_global(GlobalRightPanels(store.clone()));
        store
    }

    /// The thread's panel state; `None` when it never had one.
    pub fn panel(&self, thread: &ThreadRef) -> Option<&ThreadPanel> {
        self.threads.get(thread)
    }

    /// Whether `thread`'s panel is open (drives the chat header's right-panel toggle).
    pub fn is_open(&self, thread: &ThreadRef) -> bool {
        self.panel(thread).is_some_and(|panel| panel.is_open)
    }

    /// Whether the titlebar toggles (terminal, right panel) float over the panel's tab bar
    /// instead of sitting in the chat header: the panel is open inline (spec 1.8). The chat
    /// header drops its 60px control reserve while this holds.
    pub fn controls_in_panel(&self, thread: &ThreadRef, window_width: Pixels) -> bool {
        self.is_open(thread) && window_width > INLINE_MIN_WINDOW
    }

    /// Applies `edit` to `thread`'s panel; drops entries that end up empty.
    pub fn update_thread(
        &mut self,
        thread: &ThreadRef,
        edit: impl FnOnce(&mut ThreadPanel),
        cx: &mut Context<Self>,
    ) {
        let panel = self.threads.entry(thread.clone()).or_default();
        let before = panel.clone();
        edit(panel);
        let changed = *panel != before;
        let closed = !panel.is_open;
        if panel.is_empty() {
            self.threads.remove(thread);
        }
        if closed && self.maximized.as_ref() == Some(thread) {
            self.maximized = None;
        }
        if changed {
            cx.notify();
        }
    }

    pub fn open(&mut self, thread: &ThreadRef, kind: SurfaceKind, cx: &mut Context<Self>) {
        self.update_thread(thread, |panel| panel.open(kind), cx);
    }

    /// Opens `path` as a file tab (file links, diff header titles).
    pub fn open_file(
        &mut self,
        thread: &ThreadRef,
        path: &str,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        self.update_thread(thread, |panel| panel.open_file(path, line), cx);
    }

    pub fn toggle(&mut self, thread: &ThreadRef, kind: SurfaceKind, cx: &mut Context<Self>) {
        self.update_thread(thread, |panel| panel.toggle(kind), cx);
    }

    /// The chat header's right-panel toggle and `rightPanel.toggle`.
    pub fn toggle_visibility(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) {
        self.update_thread(thread, ThreadPanel::toggle_visibility, cx);
    }

    pub fn close(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) {
        self.update_thread(thread, |panel| panel.is_open = false, cx);
    }

    pub fn is_maximized(&self, thread: &ThreadRef) -> bool {
        self.maximized.as_ref() == Some(thread)
    }

    /// Maximizes or restores `thread`'s open panel (`rightPanel.toggleMaximized`).
    pub fn set_maximized(&mut self, thread: &ThreadRef, maximized: bool, cx: &mut Context<Self>) {
        let next = (maximized && self.is_open(thread)).then(|| thread.clone());
        if self.maximized != next {
            self.maximized = next;
            cx.notify();
        }
    }

    /// The thread's detail, subscribing on first use.
    pub fn detail(&mut self, thread: &ThreadRef, cx: &mut Context<Self>) -> Entity<ThreadDetail> {
        if let Some(detail) = self.details.get(thread) {
            return detail.clone();
        }
        let context = PanelContext::new(self.app_state.clone(), thread.clone());
        let detail = cx.new(|cx| ThreadDetail::live(&context, cx));
        self.details.insert(thread.clone(), detail.clone());
        detail
    }

    /// Supplies a thread's detail (snapshot fixtures).
    pub fn set_detail(&mut self, thread: ThreadRef, detail: Entity<ThreadDetail>) {
        self.details.insert(thread, detail);
    }

    /// Opens the diff tab on `turn_id`, scrolled to `file_path` (timeline "View diff",
    /// `selectTurn` + `open("diff")`).
    pub fn open_turn_diff(
        &mut self,
        thread: &ThreadRef,
        turn_id: TurnId,
        file_path: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.turn_requests
            .insert(thread.clone(), (turn_id, file_path));
        self.open(thread, SurfaceKind::Diff, cx);
        cx.notify();
    }

    /// Takes the pending "View diff" request for `thread`.
    pub fn take_turn_request(&mut self, thread: &ThreadRef) -> Option<(TurnId, Option<String>)> {
        self.turn_requests.remove(thread)
    }

    /// The stored panel width, clamped to the window (`360..=min(1400, 70% of window)`).
    pub fn width(&self, window_width: Pixels, cx: &App) -> Pixels {
        let stored = self
            .app_state
            .read(cx)
            .ui()
            .right_panel_width
            .map_or(DEFAULT_WIDTH, px);
        clamp_width(stored, window_width)
    }

    /// Persists a new width (written once a drag ends).
    pub fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = f32::from(width).round();
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| {
                    let changed = ui.right_panel_width != Some(width);
                    ui.right_panel_width = Some(width);
                    changed
                },
                cx,
            )
        });
    }
}

/// Clamps a panel width to `MIN_WIDTH..=min(1400, floor(0.7 × window))`.
pub fn clamp_width(width: Pixels, window_width: Pixels) -> Pixels {
    let max = MAX_WIDTH.min(px((f32::from(window_width) * 0.7).floor()));
    width.min(max).max(MIN_WIDTH)
}

#[cfg(test)]
mod tests {
    //! Failure modes: the 70% cap rounding instead of flooring, the 1400px cap missing on wide
    //! windows, and tiny windows letting the cap drop below the 360px minimum (the fork's
    //! `max(min, min(max, value))` keeps the minimum).
    use super::*;

    #[test]
    fn width_bounds_follow_the_window() {
        assert_eq!(clamp_width(DEFAULT_WIDTH, px(1440.)), px(540.));
        assert_eq!(clamp_width(px(100.), px(1440.)), MIN_WIDTH);
        assert_eq!(clamp_width(px(5000.), px(1441.)), px(1008.));
        assert_eq!(clamp_width(px(5000.), px(3000.)), MAX_WIDTH);
        assert_eq!(clamp_width(px(500.), px(400.)), MIN_WIDTH);
    }
}
