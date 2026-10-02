//! The window's root view (shell spec 2.3): the offcanvas sidebar with its resize rail, the
//! main column, the fixed sidebar toggle, and the toast layer. It also hosts the root keyboard
//! dispatcher.
//!
//! The window is opaque (no vibrancy): the root paints `--app-chrome-background`, and each
//! surface paints its own background with the surface grain *behind* its content.
//!
//! ```text
//! Workspace  (flex row, full window, app-chrome-background)
//! ├── sidebar gap      in-flow spacer, width = sidebar width × open progress
//! ├── main column      flex-1, background + grain       ← main_column::build_main_view
//! ├── sidebar container absolute, left = -(1 - progress) × width, sidebar + grain, border-r
//! │   ├── Sidebar
//! │   └── rail         16px resize strip straddling the right edge
//! ├── sidebar toggle   absolute at (83, 12), 28×28
//! └── toasts           top-right stack
//! ```

mod index_view;
pub mod main_column;

use std::time::Instant;

use gpui_kit::{
    AnyView, AppContext as _, ClickEvent, Context, CursorStyle, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyDownEvent, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement as _, Pixels, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, base::TextSelectionLayer,
    div, prelude::FluentBuilder as _, px,
};
use t3_logic::keybindings::{Command, ShortcutContext};
use t3_ui::{
    ActiveColors as _, Icon, IconName, TooltipExt as _,
    tokens::{font, layout, motion, radius},
    window::NoiseOverlay,
};

pub use main_column::{MainViewKey, build_main_view, collapsed_titlebar_inset};

use crate::{
    keybindings::{ShortcutScope, resolve_key_down, shortcut_label},
    notifications::KeybindingsNotifier,
    sidebar::Sidebar,
    state::AppState,
    toast::ToastLayer,
};

/// A drag step is rejected if it would leave the main column narrower than this.
const MAIN_MIN_WIDTH: Pixels = px(640.);
/// Width of the resize hit strip; half of it overhangs the sidebar edge.
const RAIL_WIDTH: Pixels = px(16.);
/// Movement before a rail press counts as a drag.
const DRAG_THRESHOLD: f32 = 2.;
/// `--workspace-controls-left` on macOS: the traffic lights end at 70pt, plus 12
/// (`--desktop-window-controls-inset`). The toggle sits 1px right of it (`ml-px`).
const CONTROLS_LEFT: Pixels = px(82.);
/// The toggle's top inside the 52px topbar: (52 - 28) / 2.
const TOGGLE_TOP: Pixels = px(12.);

/// Sidebar open/close duration (`panelAnimationDurationMs`). The fork defaults it to 0, so
/// collapse is instant; the client setting is not ported yet.
fn panel_animation_duration() -> std::time::Duration {
    std::time::Duration::ZERO
}

struct RailDrag {
    start_x: Pixels,
    start_width: Pixels,
    moved: bool,
}

/// Sidebar open/close transition over [`panel_animation_duration`], `ease-out`, from `from` to
/// the target.
struct Collapse {
    started: Instant,
    from: f32,
}

/// Root view of the main window.
pub struct Workspace {
    app_state: Entity<AppState>,
    sidebar: Entity<Sidebar>,
    main_key: MainViewKey,
    main_view: AnyView,
    sidebar_width: Pixels,
    /// 1.0 open, 0.0 collapsed (before easing).
    open_progress: f32,
    collapse: Option<Collapse>,
    rail_drag: Option<RailDrag>,
    toasts: Entity<ToastLayer>,
    keybindings_notifier: KeybindingsNotifier,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    /// Builds the workspace over the global [`AppState`] (call [`AppState::init`] or
    /// `state::fixtures::load` first).
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_state = AppState::global(cx);
        let sidebar = cx.new(|cx| Sidebar::new(app_state.clone(), window, cx));
        let route = app_state.read(cx).route().clone();
        let main_view = build_main_view(&route, &app_state, window, cx);
        let sidebar_width = app_state
            .read(cx)
            .ui()
            .sidebar_width
            .map_or(layout::SIDEBAR_WIDTH, |width| {
                px(width).max(layout::SIDEBAR_MIN_WIDTH)
            });
        let open = app_state.read(cx).sidebar_open();
        // Root shortcuts listen on the workspace element, which is only on the key dispatch
        // path while focus is inside it. Start focused there and return there whenever the
        // focused element goes away (closed dialog, finished rename).
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let subscriptions = vec![
            cx.on_focus_lost(window, |this, window, cx| window.focus(&this.focus, cx)),
            cx.observe_in(&app_state, window, Self::on_app_state_changed),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.sync_jump_hints(Default::default(), window, cx);
                }
            }),
        ];
        Self {
            app_state,
            sidebar,
            main_key: MainViewKey::for_route(&route),
            main_view,
            sidebar_width,
            open_progress: if open { 1. } else { 0. },
            collapse: None,
            rail_drag: None,
            toasts: ToastLayer::global(cx),
            keybindings_notifier: KeybindingsNotifier::default(),
            focus,
            _subscriptions: subscriptions,
        }
    }

    fn on_app_state_changed(
        &mut self,
        app_state: Entity<AppState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = app_state.read(cx);
        let key = MainViewKey::for_route(state.route());
        if key != self.main_key {
            let route = state.route().clone();
            self.main_view = build_main_view(&route, &app_state, window, cx);
            self.main_key = key;
        }
        self.keybindings_notifier.check(cx);
        let target = if app_state.read(cx).sidebar_open() {
            1.
        } else {
            0.
        };
        if self.open_progress != target {
            self.collapse = (!panel_animation_duration().is_zero()).then(|| Collapse {
                started: Instant::now(),
                from: self.open_progress,
            });
            self.open_progress = target;
        }
        cx.notify();
    }

    /// The eased open fraction for this frame; requests another frame while animating.
    fn visible_progress(&mut self, window: &mut Window) -> f32 {
        let Some(collapse) = &self.collapse else {
            return self.open_progress;
        };
        let t = collapse.started.elapsed().as_secs_f32() / panel_animation_duration().as_secs_f32();
        if !t.is_finite() || t >= 1. {
            self.collapse = None;
            return self.open_progress;
        }
        window.request_animation_frame();
        let eased = motion::cubic_bezier(0., 0., 0.58, 1., t);
        collapse.from + (self.open_progress - collapse.from) * eased
    }

    /// The sidebar view.
    pub fn sidebar(&self) -> &Entity<Sidebar> {
        &self.sidebar
    }

    /// Toggles the sidebar (`sidebar.toggle`, the titlebar toggle).
    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            let open = state.sidebar_open();
            state.set_sidebar_open(!open, cx);
        });
    }

    // -------------------------------------------------------------------------------------------
    // Keyboard

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shortcut) = resolve_key_down(event, window, cx) else {
            if event.keystroke.key == "escape"
                && self
                    .sidebar
                    .update(cx, |sidebar, cx| sidebar.clear_selection(cx))
            {
                cx.stop_propagation();
            }
            return;
        };
        let handled = match shortcut.command {
            Command::SidebarToggle => {
                self.toggle_sidebar(cx);
                true
            }
            // Key repeat never re-fires traversal, undo, or the palette's theme commands.
            Command::ThreadPrevious
            | Command::ThreadNext
            | Command::ThreadJump(_)
            | Command::ThreadUndo
            | Command::ThemeSelect
            | Command::AppearanceCycle
                if shortcut.repeat =>
            {
                false
            }
            Command::ThreadPrevious => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.navigate_adjacent(false, cx)),
            Command::ThreadNext => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.navigate_adjacent(true, cx)),
            Command::ThreadJump(index) => self
                .sidebar
                .update(cx, |sidebar, cx| sidebar.jump_to(index, cx)),
            Command::ChatNew | Command::ChatNewLocal
                if self.app_state.read(cx).route().draft().is_none() =>
            {
                let carry = shortcut.command == Command::ChatNew;
                self.sidebar.update(cx, |sidebar, cx| {
                    sidebar.new_thread_from_shortcut(carry, cx)
                })
            }
            Command::NavigationBack => {
                self.app_state.update(cx, |state, cx| state.go_back(cx));
                true
            }
            Command::NavigationForward => {
                self.app_state.update(cx, |state, cx| state.go_forward(cx));
                true
            }
            Command::Other(_) => false,
            command => {
                self.app_state
                    .update(cx, |state, cx| state.dispatch_command(command, cx));
                true
            }
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn sync_jump_hints(
        &mut self,
        modifiers: t3_logic::keybindings::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let context: ShortcutContext = ShortcutScope::context(window, cx);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar.sync_jump_hints(modifiers, context, cx)
        });
    }

    // -------------------------------------------------------------------------------------------
    // Rail

    fn start_rail_drag(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.app_state.read(cx).sidebar_open() {
            return;
        }
        self.rail_drag = Some(RailDrag {
            start_x: event.position.x,
            start_width: self.sidebar_width,
            moved: false,
        });
        cx.stop_propagation();
    }

    fn drag_rail(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.rail_drag else {
            return;
        };
        let dx = event.position.x - drag.start_x;
        if !drag.moved && f32::from(dx).abs() <= DRAG_THRESHOLD {
            return;
        }
        drag.moved = true;
        let next = (drag.start_width + dx).max(layout::SIDEBAR_MIN_WIDTH);
        // A width is accepted only if it shrinks or leaves the main column >= 640px.
        let shrinks = next < self.sidebar_width;
        if (shrinks || window.viewport_size().width - next >= MAIN_MIN_WIDTH)
            && next != self.sidebar_width
        {
            self.sidebar_width = next;
            cx.notify();
        }
    }

    /// Double-click on the rail: back to the default width (the stored width is removed).
    fn reset_sidebar_width(&mut self, cx: &mut Context<Self>) {
        self.sidebar_width = layout::SIDEBAR_WIDTH;
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| {
                    let changed = ui.sidebar_width.is_some();
                    ui.sidebar_width = None;
                    changed
                },
                cx,
            )
        });
        cx.notify();
    }

    /// The width to lay out: the stored width clamped to [208, viewport - 640], re-clamped live
    /// as the window resizes.
    fn clamped_sidebar_width(&self, window: &Window) -> Pixels {
        let max = (window.viewport_size().width - MAIN_MIN_WIDTH).max(layout::SIDEBAR_MIN_WIDTH);
        self.sidebar_width.clamp(layout::SIDEBAR_MIN_WIDTH, max)
    }

    fn end_rail_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.rail_drag.take() else {
            return;
        };
        if drag.moved {
            let width = f32::from(self.sidebar_width);
            self.app_state.update(cx, |state, cx| {
                state.update_ui(
                    |ui| {
                        let changed = ui.sidebar_width != Some(width);
                        ui.sidebar_width = Some(width);
                        changed
                    },
                    cx,
                )
            });
        }
        cx.notify();
    }

    // -------------------------------------------------------------------------------------------
    // Render pieces

    /// `SidebarControl` (shell spec 2.5): a 28px panel toggle at (83, 12), root tokens. Over the
    /// sidebar's stage art it takes the media-navigation look (white icon, white/10 hover).
    fn render_sidebar_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let state = self.app_state.read(cx);
        let open = state.sidebar_open();
        let over_art = open
            && !state.route().is_settings()
            && crate::sidebar::stage_backdrop(state, cx).is_some();
        let tooltip = match shortcut_label(&Command::SidebarToggle, cx) {
            Some(label) => format!("Toggle main sidebar ({label})"),
            None => "Toggle main sidebar".to_owned(),
        };
        let left = if cfg!(target_os = "macos") {
            CONTROLS_LEFT + px(1.)
        } else {
            px(13.)
        };
        let white = gpui_kit::white();
        let (text, hover_bg, hover_text, icon_opacity) = if over_art {
            (white.opacity(0.9), white.opacity(0.1), white, 1.)
        } else {
            (colors.foreground, colors.accent, colors.foreground, 0.8)
        };
        div()
            .id("sidebar-toggle")
            .absolute()
            .top(TOGGLE_TOP)
            .left(left)
            .size(layout::TITLEBAR_CONTROL)
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::LG)
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .cursor_pointer()
            .text_color(text)
            .hover(move |style| style.bg(hover_bg).text_color(hover_text))
            .tooltip_text(tooltip)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
            .child(
                Icon::new(if open {
                    IconName::PanelLeftClose
                } else {
                    IconName::PanelLeft
                })
                .size(px(16.))
                .opacity(icon_opacity),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let sidebar_colors = cx.sidebar_colors();
        let progress = self.visible_progress(window);
        let width = self.clamped_sidebar_width(window);
        let open = self.app_state.read(cx).sidebar_open();
        let dragging = self.rail_drag.as_ref().is_some_and(|drag| drag.moved);

        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .font_family(font::SANS)
            .bg(colors.app_chrome_background)
            .text_color(colors.foreground)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_modifiers_changed(
                cx.listener(|this, event: &ModifiersChangedEvent, window, cx| {
                    let modifiers = t3_logic::keybindings::Modifiers {
                        meta: event.modifiers.platform,
                        ctrl: event.modifiers.control,
                        shift: event.modifiers.shift,
                        alt: event.modifiers.alt,
                    };
                    this.sync_jump_hints(modifiers, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.sidebar
                        .update(cx, |sidebar, cx| sidebar.clear_selection(cx));
                }),
            )
            .on_mouse_move(cx.listener(Self::drag_rail))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.end_rail_drag(cx)),
            )
            .when(dragging, |this| this.cursor(CursorStyle::ResizeColumn))
            // Window text selection (markdown drag-select and copy). Zero-sized; must stay the
            // first child so its element id keeps the selection alive between frames.
            .child(TextSelectionLayer)
            // Sidebar gap: reserves flex space for the fixed sidebar.
            .child(div().h_full().flex_shrink_0().w(width * progress))
            // Main column (`SidebarInset`): background with the grain behind the view.
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(colors.background)
                    .child(NoiseOverlay)
                    .child(
                        self.main_view
                            .clone()
                            .cached(gpui_kit::StyleRefinement::default().size_full()),
                    ),
            )
            // Sidebar container: `--sidebar` with the grain behind the content.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(-(width * (1. - progress)))
                    .w(width)
                    .bg(sidebar_colors.sidebar)
                    .border_r_1()
                    .border_color(sidebar_colors.border)
                    .child(NoiseOverlay)
                    .child(self.sidebar.clone())
                    .child(self.render_rail(open, cx)),
            )
            .child(self.render_sidebar_toggle(cx))
            .child(self.toasts.clone())
    }
}

impl Workspace {
    /// `SidebarRail` (sidebar spec 1.4): a 16px strip centered on the sidebar's right edge. Its
    /// 2px center line shows `--sidebar-border` on hover; a double click resets the width.
    fn render_rail(&self, open: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let line = cx.sidebar_colors().sidebar_border;
        div()
            .id("sidebar-rail")
            .group("sidebar-rail")
            .absolute()
            .top_0()
            .bottom_0()
            .right(-(RAIL_WIDTH / 2.))
            .w(RAIL_WIDTH)
            .when(open, |this| {
                this.cursor(CursorStyle::ResizeColumn)
                    .tooltip_text("Drag to resize sidebar")
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_rail_drag))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                        if event.click_count() == 2 {
                            this.reset_sidebar_width(cx);
                        }
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(RAIL_WIDTH / 2. - px(1.))
                            .w(px(2.))
                            .group_hover("sidebar-rail", move |style| style.bg(line)),
                    )
            })
    }
}
