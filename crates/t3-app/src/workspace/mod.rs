//! The window's root view (spec 1.3, design-system 7): the offcanvas sidebar with its resize
//! rail, the main column, the fixed sidebar toggle, and the toast layer. It also hosts the root
//! keyboard dispatcher.
//!
//! ```text
//! Workspace  (flex row, full window)
//! ├── sidebar gap      in-flow spacer, width = sidebar width × open progress
//! ├── main column      flex-1, app_main_glass, noise overlay last        ← main_column::build_main_view
//! ├── sidebar container absolute, left = -(1 - progress) × width, app_sidebar_glass, border-r
//! │   ├── Sidebar
//! │   └── rail         16px resize strip straddling the right edge
//! ├── sidebar toggle   absolute at (90, 0), 28×28 in the 52px topbar strip
//! └── toasts           top-right stack
//! ```

mod index_view;
pub mod main_column;

use std::time::Instant;

use gpui_kit::{
    AnyView, AppContext as _, Context, CursorStyle, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyDownEvent, ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, prelude::FluentBuilder as _, px,
};
use t3_logic::keybindings::{Command, ShortcutContext};
use t3_ui::{
    ActiveColors as _, Icon, IconName,
    tokens::{font, layout, motion, radius},
    window::NoiseOverlay,
};

pub use main_column::{MainViewKey, build_main_view, collapsed_titlebar_inset};

use crate::{
    chrome::text_tooltip,
    keybindings::{ShortcutScope, resolve_key_down, shortcut_label},
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

struct RailDrag {
    start_x: Pixels,
    start_width: Pixels,
    moved: bool,
}

/// Sidebar open/close transition: 180ms `cubic-bezier(.4,0,.2,1)` from `from` to the target.
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
        let subscriptions = vec![
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
            focus: cx.focus_handle(),
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
        let target = if app_state.read(cx).sidebar_open() {
            1.
        } else {
            0.
        };
        if self.target_progress() != target {
            self.collapse = Some(Collapse {
                started: Instant::now(),
                from: self.open_progress,
            });
            self.open_progress = target;
        }
        cx.notify();
    }

    fn target_progress(&self) -> f32 {
        self.open_progress
    }

    /// The eased open fraction for this frame; requests another frame while animating.
    fn visible_progress(&mut self, window: &mut Window) -> f32 {
        let Some(collapse) = &self.collapse else {
            return self.open_progress;
        };
        let t = collapse.started.elapsed().as_secs_f32() / motion::PANEL.as_secs_f32();
        if t >= 1. {
            self.collapse = None;
            return self.open_progress;
        }
        window.request_animation_frame();
        let eased = motion::ease_standard(t);
        collapse.from + (self.open_progress - collapse.from) * eased
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
            Command::ThreadPrevious | Command::ThreadNext | Command::ThreadJump(_)
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
        if window.viewport_size().width - next >= MAIN_MIN_WIDTH && next != self.sidebar_width {
            self.sidebar_width = next;
            cx.notify();
        }
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

    fn render_sidebar_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let open = self.app_state.read(cx).sidebar_open();
        let tooltip = match shortcut_label(&Command::SidebarToggle, cx) {
            Some(label) => format!("Toggle main sidebar ({label})"),
            None => "Toggle main sidebar".to_owned(),
        };
        let left = if cfg!(target_os = "macos") {
            layout::CONTROLS_LEFT
        } else {
            px(12.)
        };
        div()
            .absolute()
            .top_0()
            .left(left)
            .h(layout::TOPBAR_HEIGHT)
            .flex()
            .items_center()
            .child(
                div()
                    .id("sidebar-toggle")
                    .size(layout::TITLEBAR_CONTROL)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::LG)
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .cursor_pointer()
                    .text_color(colors.foreground)
                    .when(open, |this| this.bg(colors.accent))
                    .hover(|style| style.bg(colors.accent))
                    .tooltip(text_tooltip(tooltip))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx)))
                    .child(
                        Icon::new(if open {
                            IconName::PanelLeftClose
                        } else {
                            IconName::PanelLeftOpen
                        })
                        .size(px(14.))
                        .opacity(0.8),
                    ),
            )
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let progress = self.visible_progress(window);
        let width = self.sidebar_width;
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
            .when(dragging, |this| this.cursor(CursorStyle::ResizeLeftRight))
            // Sidebar gap: reserves flex space for the fixed sidebar.
            .child(div().h_full().flex_shrink_0().w(width * progress))
            // Main column.
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .bg(colors.app_main_glass)
                    .child(
                        self.main_view
                            .clone()
                            .cached(gpui_kit::StyleRefinement::default().size_full()),
                    )
                    .child(NoiseOverlay),
            )
            // Sidebar container.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(-(width * (1. - progress)))
                    .w(width)
                    .bg(colors.app_sidebar_glass)
                    .border_r_1()
                    .border_color(colors.border)
                    .child(self.sidebar.clone())
                    .child(
                        div()
                            .id("sidebar-rail")
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right(-(RAIL_WIDTH / 2.))
                            .w(RAIL_WIDTH)
                            .when(open, |this| {
                                this.cursor(CursorStyle::ResizeLeft)
                                    .tooltip(text_tooltip("Drag to resize sidebar"))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::start_rail_drag),
                                    )
                            }),
                    ),
            )
            .child(self.render_sidebar_toggle(cx))
            .child(self.toasts.clone())
    }
}
