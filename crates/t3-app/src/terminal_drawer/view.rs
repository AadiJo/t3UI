//! The drawer view (panels.md 3.2-3.4, `ThreadTerminalDrawer.tsx:807-1551`).
//!
//! ```text
//! drawer             relative, clips; height animates 0 ↔ H (180ms), opacity fades
//! └── content        absolute top 0, height H (laid out at its final size, clipped while it
//!     │              animates, so the grid never refits mid-animation), border-t, background
//!     ├── body       active group: one viewport, or a split (columns or rows, up to 4)
//!     │   └── sidebar  144px, once the thread has more than one terminal
//!     ├── toolbar    floating split/split/new/close, top-right, without a sidebar
//!     └── handle     6px row-resize strip along the top edge
//! ```

use std::time::Instant;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, CursorStyle, Entity, EventEmitter, FocusHandle,
    Focusable as _, FontWeight, InteractiveElement as _, IntoElement, MouseButton, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, anchored, base,
    base::actions::Cancel, canvas, deferred, div, prelude::FluentBuilder as _, px,
};
use t3_logic::{
    ThreadRef,
    keybindings::{Command, Platform, ShortcutContext},
    terminal_layout::{SplitDirection, ThreadTerminalLayout, clamp_drawer_height},
};
use t3_ui::{
    ActiveColors as _, Icon, IconName, MenuItem, MenuPopup,
    tokens::{motion, shadow},
};

use super::{
    TerminalContextSelection, ThreadWorkspace,
    store::{DrawerRequest, TerminalStore},
};
use crate::{
    chrome::TypeScale as _,
    header_tools::{
        open_in,
        widgets::{HoverPlacement, hover_label},
    },
    keybindings::ShortcutScope,
    state::{AppEvent, AppState},
};

/// Drawer opacity fades in over 140ms after a 30ms delay and out over 100ms.
const FADE_IN_MS: f32 = 140.;
const FADE_IN_DELAY_MS: f32 = 30.;
const FADE_OUT_MS: f32 = 100.;

/// What the drawer asks of its chat view.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalDrawerEvent {
    /// "Add to chat" on a terminal selection: add it to the composer as terminal context.
    AddToChat(TerminalContextSelection),
    /// The drawer closed: the composer takes focus back.
    Closed,
}

/// An open/close transition in flight.
struct Transition {
    started: Instant,
    opening: bool,
    from_height: f32,
    from_opacity: f32,
}

struct ResizeDrag {
    start_y: Pixels,
    start_height: f32,
    height: f32,
    moved: bool,
}

/// The terminal drawer of one thread. See the module docs.
pub struct TerminalDrawer {
    workspace: Option<ThreadWorkspace>,
    store: Entity<TerminalStore>,
    open: bool,
    transition: Option<Transition>,
    drag: Option<ResizeDrag>,
    focus_seen: u64,
    pending_focus: bool,
    selection_menu: Option<(TerminalContextSelection, Point<Pixels>)>,
    menu_focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TerminalDrawerEvent> for TerminalDrawer {}

impl TerminalDrawer {
    /// A drawer for `workspace`'s thread. `None` (no project yet) renders nothing.
    pub fn new(workspace: Option<ThreadWorkspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = TerminalStore::global(cx);
        let app_state = AppState::global(cx);
        let open = workspace
            .as_ref()
            .is_some_and(|workspace| store.read(cx).layout(&workspace.thread).terminal_open);
        let subscriptions = vec![
            cx.observe_in(&store, window, Self::on_store_changed),
            cx.subscribe_in(&app_state, window, Self::on_app_event),
        ];
        Self {
            focus_seen: store.read(cx).focus_request(),
            workspace,
            store,
            open,
            transition: None,
            drag: None,
            pending_focus: open,
            selection_menu: None,
            menu_focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Points the drawer at another workspace (a draft picked a worktree, the shell caught up).
    pub fn set_workspace(&mut self, workspace: Option<ThreadWorkspace>, cx: &mut Context<Self>) {
        if self.workspace != workspace {
            self.open = workspace
                .as_ref()
                .is_some_and(|workspace| self.store.read(cx).layout(&workspace.thread).terminal_open);
            self.transition = None;
            self.workspace = workspace;
            cx.notify();
        }
    }

    fn thread(&self) -> Option<&ThreadRef> {
        self.workspace.as_ref().map(|workspace| &workspace.thread)
    }

    /// Whether this is the drawer the user sees (the last one rendered).
    fn is_front(&self, cx: &App) -> bool {
        self.thread()
            .is_some_and(|thread| self.store.read(cx).is_front(thread))
    }

    fn on_store_changed(
        &mut self,
        store: Entity<TerminalStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(thread) = self.thread().cloned() else {
            return;
        };
        let open = store.read(cx).layout(&thread).terminal_open;
        if open != self.open {
            self.start_transition(open);
            if !open {
                cx.emit(TerminalDrawerEvent::Closed);
            }
        }
        let focus_request = store.read(cx).focus_request();
        if focus_request != self.focus_seen {
            self.focus_seen = focus_request;
            self.pending_focus = true;
        }
        if self.is_front(cx) {
            let requests = store.update(cx, |store, _| store.take_requests());
            for request in requests {
                self.handle_request(request, window, cx);
            }
        }
        cx.notify();
    }

    fn start_transition(&mut self, opening: bool) {
        let (height, opacity) = self.current_frame();
        self.open = opening;
        self.transition = Some(Transition {
            started: Instant::now(),
            opening,
            from_height: height,
            from_opacity: opacity,
        });
    }

    /// Height fraction and opacity right now, without advancing anything.
    fn current_frame(&self) -> (f32, f32) {
        let target = if self.open { 1. } else { 0. };
        let Some(transition) = &self.transition else {
            return (target, target);
        };
        let elapsed = transition.started.elapsed().as_secs_f32() * 1000.;
        let height_t = (elapsed / motion::PANEL.as_millis() as f32).min(1.);
        let height = transition.from_height
            + (target - transition.from_height) * motion::ease_standard(height_t);
        let opacity_t = if transition.opening {
            ((elapsed - FADE_IN_DELAY_MS) / FADE_IN_MS).clamp(0., 1.)
        } else {
            (elapsed / FADE_OUT_MS).min(1.)
        };
        let eased = if transition.opening {
            motion::cubic_bezier(0., 0., 0.58, 1., opacity_t)
        } else {
            motion::cubic_bezier(0.42, 0., 1., 1., opacity_t)
        };
        let opacity = transition.from_opacity + (target - transition.from_opacity) * eased;
        (height, opacity)
    }

    /// The frame to paint; requests the next frame while a transition runs.
    fn frame(&mut self, window: &mut Window) -> (f32, f32) {
        let frame = self.current_frame();
        if let Some(transition) = &self.transition {
            let elapsed = transition.started.elapsed().as_secs_f32() * 1000.;
            let total = (motion::PANEL.as_millis() as f32).max(FADE_IN_DELAY_MS + FADE_IN_MS);
            if elapsed >= total {
                self.transition = None;
                let target = if self.open { 1. } else { 0. };
                return (target, target);
            }
            window.request_animation_frame();
        }
        frame
    }

    fn on_app_event(
        &mut self,
        _: &Entity<AppState>,
        event: &AppEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let AppEvent::Command(command) = event else {
            return;
        };
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        if !self.is_front(cx) {
            return;
        }
        self.store.update(cx, |store, cx| match command {
            Command::TerminalToggle => store.toggle(&workspace, cx),
            Command::TerminalNew => store.new_terminal(&workspace, cx),
            Command::TerminalSplit => store.split(&workspace, SplitDirection::Horizontal, cx),
            Command::TerminalSplitVertical => {
                store.split(&workspace, SplitDirection::Vertical, cx)
            }
            Command::TerminalClose => {
                let layout = store.layout(&workspace.thread);
                if layout.terminal_open && !layout.active_terminal_id.is_empty() {
                    store.close(&workspace.thread, &layout.active_terminal_id, cx);
                }
            }
            _ => {}
        });
    }

    fn handle_request(&mut self, request: DrawerRequest, window: &mut Window, cx: &mut Context<Self>) {
        match request {
            DrawerRequest::OpenLink { url: Some(url), .. } => cx.open_url(&url),
            DrawerRequest::OpenLink {
                thread,
                path: Some(path),
                ..
            } => {
                open_in::open_in_preferred_editor(&thread.environment_id, path, cx).detach();
            }
            DrawerRequest::OpenLink { .. } => {}
            DrawerRequest::SelectionMenu(selection, position) => {
                self.selection_menu = Some((selection, position));
                window.focus(&self.menu_focus, cx);
                cx.notify();
            }
        }
    }

    fn choose_add_to_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((selection, _)) = self.selection_menu.take() else {
            return;
        };
        let session = self
            .store
            .read(cx)
            .find_session(&selection.thread, &selection.terminal_id);
        cx.emit(TerminalDrawerEvent::AddToChat(selection));
        if let Some(session) = session {
            let view = session.read(cx).view().clone();
            view.update(cx, |view, cx| view.clear_selection(cx));
            window.focus(&view.focus_handle(cx), cx);
        }
        cx.notify();
    }

    fn close_selection_menu(&mut self, cx: &mut Context<Self>) {
        if self.selection_menu.take().is_some() {
            cx.notify();
        }
    }

    // ---------------------------------------------------------------------------------------
    // Resize

    fn start_resize(&mut self, y: Pixels, height: f32, cx: &mut Context<Self>) {
        self.drag = Some(ResizeDrag {
            start_y: y,
            start_height: height,
            height,
            moved: false,
        });
        cx.notify();
    }

    fn drag_resize(&mut self, y: Pixels, window_height: f32, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let next = clamp_drawer_height(
            drag.start_height + f32::from(drag.start_y - y),
            window_height,
        );
        if next != drag.height {
            drag.height = next;
            drag.moved = true;
            cx.notify();
        }
    }

    fn end_resize(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        if drag.moved
            && let Some(thread) = self.thread().cloned()
        {
            self.store
                .update(cx, |store, cx| store.set_height(&thread, drag.height, cx));
        }
        cx.notify();
    }

    // ---------------------------------------------------------------------------------------
    // Render pieces

    fn render_viewport(
        &mut self,
        workspace: &ThreadWorkspace,
        terminal_id: &str,
        focus_target: &mut Option<FocusHandle>,
        is_active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let session = self
            .store
            .update(cx, |store, cx| store.session(workspace, terminal_id, window, cx));
        let view = session.read(cx).view().clone();
        if is_active {
            *focus_target = Some(view.focus_handle(cx));
        }
        // `h-full p-1` around `relative h-full w-full overflow-hidden rounded-[4px] bg-background`.
        div().size_full().p(px(4.)).child(
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .rounded(px(4.))
                .bg(cx.colors().background)
                .child(view),
        )
    }

    fn render_body(
        &mut self,
        workspace: &ThreadWorkspace,
        layout: &ThreadTerminalLayout,
        focus_target: &mut Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let visible: Vec<String> = layout.visible_terminal_ids().to_vec();
        let active = layout.active_terminal_id.clone();
        let vertical = layout
            .active_group()
            .is_some_and(|group| group.split_direction == SplitDirection::Vertical);
        if visible.len() <= 1 {
            let id = visible.first().cloned().unwrap_or(active);
            return self
                .render_viewport(workspace, &id, focus_target, true, window, cx)
                .into_any_element();
        }
        let mut panes = Vec::new();
        for (index, terminal_id) in visible.iter().enumerate() {
            let is_active = *terminal_id == active;
            let viewport =
                self.render_viewport(workspace, terminal_id, focus_target, is_active, window, cx);
            let thread = workspace.thread.clone();
            let id = terminal_id.clone();
            panes.push(
                div()
                    .id(SharedString::from(format!("terminal-pane-{terminal_id}")))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .border_color(if is_active {
                        colors.border
                    } else {
                        colors.border.opacity(0.7)
                    })
                    .when(index > 0, |this| {
                        if vertical {
                            this.border_t_1()
                        } else {
                            this.border_l_1()
                        }
                    })
                    .when(!is_active, |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, _, cx| {
                                this.store
                                    .update(cx, |store, cx| store.set_active(&thread, &id, cx));
                            }),
                        )
                    })
                    .child(viewport),
            );
        }
        div()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .when(vertical, |this| this.flex_col())
            .children(panes)
            .into_any_element()
    }

    /// The split/split/new/close actions, as the floating toolbar or the sidebar header.
    fn render_actions(
        &self,
        workspace: &ThreadWorkspace,
        layout: &ThreadTerminalLayout,
        in_sidebar: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.colors();
        let at_limit = layout.split_limit_reached();
        let labels = ActionLabels::new(at_limit, cx);
        let active = layout.active_terminal_id.clone();
        let actions: [(IconName, SharedString, bool, Action); 4] = [
            (
                IconName::SquareSplitHorizontal,
                labels.split,
                at_limit,
                Action::Split(SplitDirection::Horizontal),
            ),
            (
                IconName::SquareSplitVertical,
                labels.split_vertical,
                at_limit,
                Action::Split(SplitDirection::Vertical),
            ),
            (IconName::Plus, labels.new, false, Action::New),
            (IconName::Trash2, labels.close, false, Action::Close),
        ];
        let mut elements = Vec::new();
        for (index, (icon, label, disabled, action)) in actions.into_iter().enumerate() {
            if index > 0 && !in_sidebar {
                elements.push(
                    div()
                        .h(px(16.))
                        .w(px(1.))
                        .bg(colors.border.opacity(0.8))
                        .into_any_element(),
                );
            }
            let workspace = workspace.clone();
            let active = active.clone();
            let hover = if in_sidebar {
                colors.accent.opacity(0.7)
            } else {
                colors.accent
            };
            let button = div()
                .id(SharedString::from(format!(
                    "terminal-action-{}-{index}",
                    if in_sidebar { "sidebar" } else { "toolbar" }
                )))
                .flex()
                .items_center()
                .text_color(colors.foreground.opacity(0.9))
                .map(|this| {
                    if in_sidebar {
                        this.h_full()
                            .px(px(4.))
                            .when(index > 0, |this| {
                                this.border_l_1().border_color(colors.border.opacity(0.7))
                            })
                    } else {
                        this.p(px(4.))
                    }
                })
                .map(|this| {
                    if disabled {
                        this.cursor(CursorStyle::OperationNotAllowed).opacity(0.45)
                    } else {
                        this.cursor_pointer().hover(move |style| style.bg(hover))
                    }
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if disabled {
                        return;
                    }
                    this.store.update(cx, |store, cx| match action {
                        Action::Split(direction) => store.split(&workspace, direction, cx),
                        Action::New => store.new_terminal(&workspace, cx),
                        Action::Close => store.close(&workspace.thread, &active, cx),
                    });
                }))
                .child(Icon::new(icon).size(px(13.)));
            elements.push(hover_label(button, label, HoverPlacement::DRAWER).into_any_element());
        }
        elements
    }

    fn render_sidebar(
        &self,
        workspace: &ThreadWorkspace,
        layout: &ThreadTerminalLayout,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.colors();
        let store = self.store.read(cx);
        let show_headers = layout.shows_group_headers();
        let close_label = shortcut(&Command::TerminalClose, layout.terminal_open, cx);
        let many = layout.terminal_ids.len() > 1;
        let mut groups = Vec::new();
        for (group_index, group) in layout.terminal_groups.iter().enumerate() {
            let group_active = group.terminal_ids.contains(&layout.active_terminal_id);
            let group_target = if group_active {
                layout.active_terminal_id.clone()
            } else {
                group.terminal_ids.first().cloned().unwrap_or_default()
            };
            let header = show_headers.then(|| {
                let thread = workspace.thread.clone();
                div()
                    .id(SharedString::from(format!("terminal-group-{}", group.id)))
                    .flex()
                    .w_full()
                    .items_center()
                    .rounded(px(4.))
                    .px(px(4.))
                    .py(px(2.))
                    .text_size(px(10.))
                    .line_height(px(15.))
                    .cursor_pointer()
                    .map(|this| {
                        if group_active {
                            this.bg(colors.accent.opacity(0.7))
                                .text_color(colors.foreground)
                        } else {
                            this.text_color(colors.muted_foreground).hover(|style| {
                                style.bg(colors.accent_50).text_color(colors.foreground)
                            })
                        }
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store
                            .update(cx, |store, cx| store.set_active(&thread, &group_target, cx));
                    }))
                    .child(format!("GROUP {}", group_index + 1))
            });
            let rows = group.terminal_ids.iter().map(|terminal_id| {
                let is_active = *terminal_id == layout.active_terminal_id;
                let label = store.label(&workspace.thread, terminal_id);
                let close = match (&close_label, is_active) {
                    (Some(shortcut), true) => format!("Close {label} ({shortcut})"),
                    _ => format!("Close {label}"),
                };
                let row_group = SharedString::from(format!("terminal-row-{terminal_id}"));
                let select_thread = workspace.thread.clone();
                let select_id = terminal_id.clone();
                let close_thread = workspace.thread.clone();
                let close_id = terminal_id.clone();
                div()
                    .id(SharedString::from(format!("terminal-row-{terminal_id}")))
                    .group(row_group.clone())
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(4.))
                    .px(px(4.))
                    .py(px(2.))
                    .text_size(px(11.))
                    .line_height(px(16.5))
                    .map(|this| {
                        if is_active {
                            this.bg(colors.accent).text_color(colors.foreground)
                        } else {
                            this.text_color(colors.muted_foreground).hover(|style| {
                                style.bg(colors.accent_50).text_color(colors.foreground)
                            })
                        }
                    })
                    .when(show_headers, |this| {
                        this.child(
                            div()
                                .text_size(px(10.))
                                .text_color(colors.muted_foreground.opacity(0.8))
                                .child("└"),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("terminal-select-{terminal_id}")))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(4.))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.store.update(cx, |store, cx| {
                                    store.set_active(&select_thread, &select_id, cx)
                                });
                            }))
                            .child(Icon::new(IconName::SquareTerminal).size(px(12.)))
                            .child(div().min_w_0().truncate().child(label)),
                    )
                    .when(many, |this| {
                        let close_button = div()
                            .id(SharedString::from(format!("terminal-close-{terminal_id}")))
                            .size(px(14.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .text_color(colors.muted_foreground)
                            .opacity(0.)
                            .cursor_pointer()
                            .group_hover(row_group, |style| style.opacity(1.))
                            .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.store.update(cx, |store, cx| {
                                    store.close(&close_thread, &close_id, cx)
                                });
                            }))
                            .child(Icon::new(IconName::X).size(px(10.)));
                        this.child(hover_label(close_button, close, HoverPlacement::DRAWER))
                    })
            });
            groups.push(
                div()
                    .pb(px(2.))
                    .children(header)
                    .child(
                        div()
                            .when(show_headers, |this| {
                                this.ml(px(4.))
                                    .border_l_1()
                                    .border_color(colors.border_60)
                                    .pl(px(6.))
                            })
                            .children(rows),
                    ),
            );
        }
        div()
            .w(px(144.))
            .min_w(px(144.))
            .h_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(colors.border.opacity(0.7))
            .bg(colors.muted.opacity(0.1))
            .child(
                div()
                    .h(px(22.))
                    .flex_none()
                    .flex()
                    .justify_end()
                    .border_b_1()
                    .border_color(colors.border.opacity(0.7))
                    .child(
                        div()
                            .h_full()
                            .flex()
                            .children(self.render_actions(workspace, layout, true, cx)),
                    ),
            )
            .child(
                div()
                    .id("terminal-sidebar-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(4.))
                    .children(groups),
            )
    }

    fn render_empty(&self, workspace: &ThreadWorkspace, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.colors();
        let label = match shortcut(&Command::TerminalNew, true, cx) {
            Some(shortcut) => format!("New Terminal ({shortcut})"),
            None => "New Terminal".to_owned(),
        };
        let workspace = workspace.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .px(px(16.))
            .py(px(24.))
            .type_scale(t3_ui::tokens::text::SM)
            .text_color(colors.muted_foreground)
            .child("No terminal sessions for this thread yet.")
            .child(
                div()
                    .id("terminal-empty-new")
                    .rounded(px(8.))
                    .border_1()
                    .border_color(colors.border_80)
                    .bg(colors.background)
                    .px(px(12.))
                    .py(px(6.))
                    .type_scale(t3_ui::tokens::text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store
                            .update(cx, |store, cx| store.new_terminal(&workspace, cx));
                    }))
                    .child(label),
            )
    }

    fn render_selection_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (_, position) = self.selection_menu.as_ref()?;
        let entity = cx.entity();
        let close_entity = cx.entity();
        let outside_entity = cx.entity();
        Some(
            deferred(
                anchored()
                    .position(*position)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("terminal-selection-menu")
                            .occlude()
                            .track_focus(&self.menu_focus)
                            .key_context("Popover")
                            .on_action(move |_: &Cancel, _, cx| {
                                close_entity.update(cx, |this, cx| this.close_selection_menu(cx));
                            })
                            .on_mouse_down_out(move |_, _, cx| {
                                outside_entity
                                    .update(cx, |this, cx| this.close_selection_menu(cx));
                            })
                            .child(MenuPopup::new().child(
                                MenuItem::new("terminal-add-to-chat", "Add to chat").on_click(
                                    move |_, window, cx| {
                                        entity.update(cx, |this, cx| {
                                            this.choose_add_to_chat(window, cx)
                                        });
                                    },
                                ),
                            )),
                    ),
            )
            .with_priority(base::POPUP_PRIORITY),
        )
    }
}

#[derive(Clone, Copy)]
enum Action {
    Split(SplitDirection),
    New,
    Close,
}

/// The four action labels with their shortcuts (`Split Terminal Horizontally (⌘D)`).
struct ActionLabels {
    split: SharedString,
    split_vertical: SharedString,
    new: SharedString,
    close: SharedString,
}

impl ActionLabels {
    fn new(at_limit: bool, cx: &App) -> Self {
        let with = |base: &str, command: Command| match shortcut(&command, true, cx) {
            Some(shortcut) => SharedString::from(format!("{base} ({shortcut})")),
            None => SharedString::from(base.to_owned()),
        };
        let limited = |base: &str, command: Command| {
            if at_limit {
                SharedString::from(format!(
                    "{base} (max {} per group)",
                    t3_logic::terminal_layout::MAX_TERMINALS_PER_GROUP
                ))
            } else {
                with(base, command)
            }
        };
        Self {
            split: limited("Split Terminal Horizontally", Command::TerminalSplit),
            split_vertical: limited("Split Terminal Vertically", Command::TerminalSplitVertical),
            new: with("New Terminal", Command::TerminalNew),
            close: with("Close Terminal", Command::TerminalClose),
        }
    }
}

/// A drawer shortcut label, resolved with the terminal focused (the drawer's `when` context).
fn shortcut(command: &Command, terminal_open: bool, cx: &App) -> Option<String> {
    let rules = AppState::global(cx).read(cx).keybindings(cx);
    let context = ShortcutContext {
        terminal_focus: true,
        terminal_open,
        ..ShortcutContext::default()
    };
    t3_logic::keybindings::shortcut_label(&rules, command, &context, Platform::current())
}

impl Render for TerminalDrawer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(workspace) = self.workspace.clone() else {
            return div().into_any_element();
        };
        let thread = workspace.thread.clone();
        self.store
            .update(cx, |store, cx| store.note_shown(&thread, cx));
        let layout = self.store.read(cx).layout(&thread);
        ShortcutScope::update(cx, |scope| scope.terminal_open = layout.terminal_open);

        let (fraction, opacity) = self.frame(window);
        // Closed: the fork's `height: 0` aside still has its 1px top border (invisible).
        if fraction <= 0. && self.transition.is_none() {
            return div().h(px(1.)).flex_none().into_any_element();
        }
        let colors = cx.colors();
        let window_height = f32::from(window.viewport_size().height);
        let height = clamp_drawer_height(
            self.drag
                .as_ref()
                .map_or(layout.terminal_height, |drag| drag.height),
            window_height,
        );

        let mut focus_target = None;
        let has_terminals = !layout.terminal_ids.is_empty();
        let has_sidebar = layout.terminal_ids.len() > 1;
        let body = if has_terminals {
            let main = self.render_body(&workspace, &layout, &mut focus_target, window, cx);
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .when(has_sidebar, |this| this.gap(px(6.)))
                .child(div().min_w_0().flex_1().h_full().child(main))
                .when(has_sidebar, |this| {
                    this.child(self.render_sidebar(&workspace, &layout, cx))
                })
                .into_any_element()
        } else {
            self.render_empty(&workspace, cx).into_any_element()
        };
        if self.pending_focus && self.open {
            self.pending_focus = false;
            if let Some(handle) = focus_target {
                cx.defer_in(window, move |_, window, cx| window.focus(&handle, cx));
            }
        }

        let resizing = self.drag.is_some();
        let resize_handle = div()
            .id("terminal-drawer-resize")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(6.))
            .cursor(CursorStyle::ResizeRow)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui_kit::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.start_resize(event.position.y, height, cx);
                }),
            );
        // While dragging, follow the pointer anywhere in the window.
        let drag_tracker = resizing.then(|| {
            let entity = cx.entity();
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    window.set_window_cursor_style(CursorStyle::ResizeRow);
                    let moved = entity.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase.capture() {
                            let window_height = f32::from(window.viewport_size().height);
                            moved.update(cx, |this, cx| {
                                this.drag_resize(event.position.y, window_height, cx)
                            });
                        }
                    });
                    let released = entity.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase.capture() {
                            released.update(cx, |this, cx| this.end_resize(cx));
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        });

        let toolbar = (has_terminals && !has_sidebar).then(|| {
            div()
                .absolute()
                .top(px(8.))
                .right(px(8.))
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded(px(8.))
                .border_1()
                .border_color(colors.border_80)
                .bg(colors.background)
                .shadow(shadow::SM.to_vec())
                .children(self.render_actions(&workspace, &layout, false, cx))
        });

        div()
            .id("terminal-drawer")
            .relative()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(height * fraction).max(px(1.)))
            .overflow_hidden()
            .opacity(opacity)
            .child(
                div()
                    .id("terminal-drawer-content")
                    .track_focus(self.store.read(cx).focus_handle())
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(height))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_t_1()
                    .border_color(colors.border_80)
                    .bg(colors.background)
                    .child(body)
                    .children(toolbar)
                    .child(resize_handle)
                    .children(drag_tracker),
            )
            .children(self.render_selection_menu(cx))
            .into_any_element()
    }
}
