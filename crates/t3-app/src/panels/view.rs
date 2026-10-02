//! [`RightPanel`]: the right panel beside the chat column (spec 1.3-1.10). Inline above a
//! 980px window (a clipping wrapper whose width animates 0 to W over 180ms, with a resize
//! handle), or an overlay sheet at or below it. The tab bar, empty state and titlebar
//! controls live here; surfaces are created lazily per thread and tab.

use std::{collections::HashMap, time::Instant};

use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, ClickEvent, Context, CursorStyle, Entity,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement as _, Pixels, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, anchored, deferred, div, point, prelude::FluentBuilder as _, px,
};
use t3_logic::{ThreadRef, keybindings::Command};
use t3_ui::{
    ActiveColors as _, Align, Button, ButtonSize, ButtonVariant, ContextMenu, DropdownMenu, Icon,
    IconName, MenuItem, MenuSeparator,
    tokens::{font, motion},
};

use super::{
    context::PanelContext,
    diff_panel::DiffPanel,
    files::{FilePreview, FilesSurface},
    model::{Surface, SurfaceId, SurfaceKind},
    plan::PlanSurface,
    store::{RightPanels, clamp_width},
};
use crate::state::{AppEvent, AppState, Route};

/// Inline layout needs a window wider than this (`RIGHT_PANEL_INLINE_LAYOUT_MEDIA_QUERY`).
const INLINE_MIN_WINDOW: Pixels = px(980.);
const TRANSITION_MS: f32 = 180.;
/// The maximize button appears this long after opening starts.
const DELAYED_CONTROL_MS: f32 = 120.;
const TOPBAR_HEIGHT: Pixels = px(52.);

/// A created surface view.
enum SurfaceView {
    Diff(Entity<DiffPanel>),
    Files(Entity<FilesSurface>),
    File(Entity<FilePreview>, u64),
    Plan(Entity<PlanSurface>),
}

impl SurfaceView {
    fn view(&self) -> AnyView {
        match self {
            Self::Diff(view) => view.clone().into(),
            Self::Files(view) => view.clone().into(),
            Self::File(view, _) => view.clone().into(),
            Self::Plan(view) => view.clone().into(),
        }
    }
}

/// Open/close transition state (`usePanelTransitionPresence`).
struct Presence {
    open: bool,
    started: Option<Instant>,
    /// Eased progress when the current transition started.
    from: f32,
}

struct WidthDrag {
    start_x: Pixels,
    start_width: Pixels,
    width: Pixels,
}

/// The right panel of the routed thread.
pub struct RightPanel {
    app_state: Entity<AppState>,
    panels: Entity<RightPanels>,
    thread: Option<ThreadRef>,
    surfaces: HashMap<(ThreadRef, SurfaceId), SurfaceView>,
    presence: Presence,
    drag: Option<WidthDrag>,
    _subscriptions: Vec<Subscription>,
}

impl RightPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_state = AppState::global(cx);
        let panels = RightPanels::global(cx);
        let subscriptions = vec![
            cx.observe_in(&panels, window, |this, _, window, cx| this.sync(window, cx)),
            cx.observe_in(&app_state, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
            cx.subscribe(&app_state, |this, _, event, cx| {
                if let AppEvent::Command(command) = event {
                    this.handle_command(command, cx);
                }
            }),
        ];
        let mut this = Self {
            app_state,
            panels,
            thread: None,
            surfaces: HashMap::new(),
            presence: Presence {
                open: false,
                started: None,
                from: 0.,
            },
            drag: None,
            _subscriptions: subscriptions,
        };
        this.sync(window, cx);
        // Already open at startup (or in a scene): no opening transition.
        this.presence.started = None;
        this
    }

    /// Whether the routed thread's panel is maximized inline, so the workspace collapses the
    /// chat column to zero width.
    pub fn hides_main_view(&self, window: &Window, cx: &App) -> bool {
        self.thread.as_ref().is_some_and(|thread| {
            self.panels.read(cx).is_maximized(thread)
                && window.viewport_size().width > INLINE_MIN_WINDOW
        })
    }

    fn handle_command(&mut self, command: &Command, cx: &mut Context<Self>) {
        let Some(thread) = self.thread.clone() else {
            return;
        };
        self.panels.update(cx, |panels, cx| match command {
            Command::RightPanelToggle => {
                if panels.is_open(&thread) {
                    panels.close(&thread, cx);
                } else {
                    panels.toggle_visibility(&thread, cx);
                }
            }
            Command::RightPanelToggleMaximized => {
                let maximized = panels.is_maximized(&thread);
                panels.set_maximized(&thread, !maximized, cx);
            }
            Command::RightPanelClose => panels.close(&thread, cx),
            Command::DiffToggle => panels.toggle(&thread, SurfaceKind::Diff, cx),
            _ => {}
        });
    }

    /// Follows the route and the store: the routed thread, its surfaces, the open state.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.thread = match self.app_state.read(cx).route() {
            Route::Thread(thread) => Some(thread.clone()),
            _ => None,
        };
        let open = self
            .thread
            .as_ref()
            .is_some_and(|thread| self.panels.read(cx).is_open(thread));
        if open != self.presence.open {
            let animate = self.app_state.read(cx).clock_is_live();
            self.presence = Presence {
                from: self.progress(),
                open,
                started: animate.then(Instant::now),
            };
        }
        if let Some(thread) = self.thread.clone() {
            self.ensure_surfaces(&thread, window, cx);
        }
        cx.notify();
    }

    /// Creates views for new tabs, drops closed ones, and forwards reveal requests.
    fn ensure_surfaces(&mut self, thread: &ThreadRef, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panels.read(cx).panel(thread).cloned() else {
            self.surfaces.retain(|(owner, _), _| owner != thread);
            return;
        };
        let live: Vec<SurfaceId> = panel.surfaces.iter().map(Surface::id).collect();
        self.surfaces
            .retain(|(owner, id), _| owner != thread || live.contains(id));
        let context = PanelContext::new(self.app_state.clone(), thread.clone());
        for surface in &panel.surfaces {
            let key = (thread.clone(), surface.id());
            match (surface, self.surfaces.get_mut(&key)) {
                (
                    Surface::File {
                        reveal_line,
                        reveal_request,
                        ..
                    },
                    Some(SurfaceView::File(view, seen)),
                ) => {
                    if *seen != *reveal_request {
                        *seen = *reveal_request;
                        view.update(cx, |view, cx| {
                            view.reveal(*reveal_line, *reveal_request, cx)
                        });
                    }
                }
                (_, Some(_)) => {}
                (surface, None) => {
                    let created = match surface {
                        Surface::Diff => {
                            let detail = self
                                .panels
                                .update(cx, |panels, cx| panels.detail(thread, cx));
                            let context = context.clone();
                            Some(SurfaceView::Diff(
                                cx.new(|cx| DiffPanel::new(context, detail, window, cx)),
                            ))
                        }
                        Surface::Files => {
                            let context = context.clone();
                            Some(SurfaceView::Files(
                                cx.new(|cx| FilesSurface::new(context, window, cx)),
                            ))
                        }
                        Surface::File {
                            path,
                            reveal_line,
                            reveal_request,
                        } => {
                            let (context, path, line) =
                                (context.clone(), path.clone(), *reveal_line);
                            Some(SurfaceView::File(
                                cx.new(|cx| FilePreview::new(context, path, line, window, cx)),
                                *reveal_request,
                            ))
                        }
                        Surface::Plan => {
                            let detail = self
                                .panels
                                .update(cx, |panels, cx| panels.detail(thread, cx));
                            let context = context.clone();
                            Some(SurfaceView::Plan(
                                cx.new(|cx| PlanSurface::new(context, detail, window, cx)),
                            ))
                        }
                        // Owned by the terminal and preview surfaces (later phases).
                        Surface::Terminal { .. } | Surface::Preview { .. } => None,
                    };
                    if let Some(created) = created {
                        self.surfaces.insert(key, created);
                    }
                }
            }
        }
        let request = self
            .panels
            .update(cx, |panels, _| panels.take_turn_request(thread));
        if let (Some((turn_id, path)), Some(SurfaceView::Diff(diff))) = (
            request,
            self.surfaces.get(&(thread.clone(), SurfaceId::Diff)),
        ) {
            diff.update(cx, |diff, cx| diff.select_turn(turn_id, path, cx));
        }
    }

    /// Eased open progress in `0..=1`.
    fn progress(&self) -> f32 {
        let target = if self.presence.open { 1. } else { 0. };
        let Some(started) = self.presence.started else {
            return target;
        };
        let t = (started.elapsed().as_secs_f32() * 1000. / TRANSITION_MS).min(1.);
        let eased = motion::ease_standard(t);
        self.presence.from + (target - self.presence.from) * eased
    }

    fn animating(&self) -> bool {
        self.presence
            .started
            .is_some_and(|started| started.elapsed().as_secs_f32() * 1000. < TRANSITION_MS)
    }

    /// Viewport opacity: fades in over 140ms after 30ms, out over 100ms.
    fn opacity(&self) -> f32 {
        let Some(started) = self.presence.started else {
            return 1.;
        };
        let ms = started.elapsed().as_secs_f32() * 1000.;
        if self.presence.open {
            ((ms - 30.) / 140.).clamp(0., 1.)
        } else {
            1. - (ms / 100.).clamp(0., 1.)
        }
    }

    fn entered(&self) -> bool {
        self.presence.open
            && self
                .presence
                .started
                .is_none_or(|started| started.elapsed().as_secs_f32() * 1000. >= DELAYED_CONTROL_MS)
    }

    fn start_drag(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let width = self.panels.read(cx).width(window.viewport_size().width, cx);
        self.drag = Some(WidthDrag {
            start_x: event.position.x,
            start_width: width,
            width,
        });
        cx.notify();
    }

    fn drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let window_width = window.viewport_size().width;
        if let Some(drag) = &mut self.drag {
            drag.width = clamp_width(
                drag.start_width + (drag.start_x - event.position.x),
                window_width,
            );
            cx.notify();
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take() {
            self.panels
                .update(cx, |panels, cx| panels.set_width(drag.width, cx));
        }
    }

    fn render_tab(
        &self,
        thread: &ThreadRef,
        surface: &Surface,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let id = surface.id();
        let (title, icon) = surface_title_icon(surface, cx);
        let panels = self.panels.clone();
        let close = {
            let (panels, thread, id) = (panels.clone(), thread.clone(), id.clone());
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                panels.update(cx, |panels, cx| {
                    panels.update_thread(&thread, |panel| panel.close_surface(&id), cx)
                })
            }
        };
        let element_id = SharedString::from(format!("panel-tab-{id:?}"));
        let tab = div()
            .id(element_id.clone())
            .group("panel-tab")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .h(px(28.))
            .min_w(px(100.))
            .max_w(px(176.))
            .px(px(8.))
            .rounded(px(8.))
            .text_size(px(14.))
            .map(|tab| {
                if active {
                    tab.bg(colors.accent).text_color(colors.foreground)
                } else {
                    tab.text_color(colors.muted_foreground).hover(|tab| {
                        tab.bg(colors.accent.opacity(0.6))
                            .text_color(colors.foreground)
                    })
                }
            })
            .on_click({
                let (panels, thread, id) = (panels.clone(), thread.clone(), id.clone());
                move |_, _, cx| {
                    panels.update(cx, |panels, cx| {
                        panels.update_thread(&thread, |panel| panel.activate(&id), cx)
                    })
                }
            })
            .on_aux_click(close.clone())
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(icon)
                    .child(div().min_w_0().truncate().child(title.clone())),
            )
            .child(
                div()
                    .id(SharedString::from(format!("{element_id}-close")))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(16.))
                    .rounded(px(4.))
                    .opacity(0.)
                    .group_hover("panel-tab", |close| close.opacity(1.))
                    .hover(|close| close.bg(colors.muted))
                    .on_click(close)
                    .child(Icon::new(IconName::X).size(px(12.))),
            );
        let menu_thread = thread.clone();
        let menu_panels = panels;
        ContextMenu::new(SharedString::from(format!("{element_id}-menu")), tab)
            .items(move |_, cx| {
                let count = menu_panels
                    .read(cx)
                    .panel(&menu_thread)
                    .map_or(0, |panel| panel.surfaces.len());
                let last = menu_panels
                    .read(cx)
                    .panel(&menu_thread)
                    .and_then(|panel| panel.surfaces.last().map(Surface::id))
                    == Some(id.clone());
                let action =
                    |item_id: &'static str,
                     label: &'static str,
                     edit: fn(&mut super::ThreadPanel, &SurfaceId)| {
                        let (panels, thread, id) =
                            (menu_panels.clone(), menu_thread.clone(), id.clone());
                        MenuItem::new(item_id, label).on_click(move |_, _, cx| {
                            panels.update(cx, |panels, cx| {
                                panels.update_thread(&thread, |panel| edit(panel, &id), cx)
                            })
                        })
                    };
                vec![
                    action("tab-close", "Close", |panel, id| panel.close_surface(id))
                        .into_any_element(),
                    action("tab-close-others", "Close others", |panel, id| {
                        panel.close_others(id)
                    })
                    .disabled(count <= 1)
                    .into_any_element(),
                    action("tab-close-right", "Close to the right", |panel, id| {
                        panel.close_to_right(id)
                    })
                    .disabled(last)
                    .into_any_element(),
                    MenuSeparator.into_any_element(),
                    action("tab-close-all", "Close all", |panel, _| panel.close_all())
                        .disabled(count == 0)
                        .into_any_element(),
                ]
            })
            .into_any_element()
    }

    fn render_add_menu(&self, thread: &ThreadRef) -> impl IntoElement {
        let panels = self.panels.clone();
        let thread = thread.clone();
        DropdownMenu::new("panel-add")
            .align(Align::Start)
            .min_width(px(176.))
            .trigger(|open| {
                Button::new("panel-add-trigger")
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::IconSm)
                    .icon(IconName::Plus)
                    .pressed(open)
                    .into_any_element()
            })
            .items(move |_, _| {
                let open =
                    |id: &'static str, label: &'static str, icon: IconName, kind: SurfaceKind| {
                        let (panels, thread) = (panels.clone(), thread.clone());
                        MenuItem::new(id, label)
                            .icon(icon)
                            .on_click(move |_, _, cx| {
                                panels.update(cx, |panels, cx| panels.open(&thread, kind, cx))
                            })
                            .into_any_element()
                    };
                vec![
                    open(
                        "add-browser",
                        "Browser",
                        IconName::Globe,
                        SurfaceKind::Preview,
                    ),
                    open(
                        "add-terminal",
                        "Terminal",
                        IconName::SquareTerminal,
                        SurfaceKind::Terminal,
                    ),
                    open("add-files", "Files", IconName::Files, SurfaceKind::Files),
                    open("add-diff", "Diff", IconName::FileDiff, SurfaceKind::Diff),
                ]
            })
    }

    fn render_empty_state(&self, thread: &ThreadRef, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let card = |id: &'static str,
                    icon: IconName,
                    label: &'static str,
                    description: &'static str,
                    kind: SurfaceKind| {
            let (panels, thread) = (self.panels.clone(), thread.clone());
            div()
                .id(id)
                .flex()
                .flex_col()
                .items_start()
                .min_h(px(112.))
                .w_full()
                .p(px(16.))
                .rounded(px(10.))
                .border_1()
                .border_color(colors.border.opacity(0.8))
                .bg(colors.card.opacity(0.4))
                .hover(|card| {
                    card.border_color(colors.border)
                        .bg(colors.accent.opacity(0.6))
                })
                .on_click(move |_, _, cx| {
                    panels.update(cx, |panels, cx| panels.open(&thread, kind, cx))
                })
                .child(Icon::new(icon).size(px(20.)).mb(px(12.)))
                .child(
                    div()
                        .text_size(px(14.))
                        .font_weight(gpui_kit::FontWeight::MEDIUM)
                        .child(label),
                )
                .child(
                    div()
                        .mt(px(4.))
                        .text_size(px(12.))
                        .line_height(px(12. * 1.625))
                        .text_color(colors.muted_foreground)
                        .child(description),
                )
        };
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.))
            .child(
                div()
                    .w_full()
                    .max_w(px(576.))
                    .child(
                        div()
                            .mb(px(20.))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child("Open a surface"),
                            )
                            .child(
                                div()
                                    .mt(px(4.))
                                    .text_size(px(12.))
                                    .text_color(colors.muted_foreground)
                                    .child("Choose what to show in the right panel."),
                            ),
                    )
                    .child(
                        div()
                            .grid()
                            .grid_cols(2)
                            .gap(px(8.))
                            .child(card(
                                "empty-browser",
                                IconName::Globe,
                                "Browser",
                                "Open a local app or URL.",
                                SurfaceKind::Preview,
                            ))
                            .child(card(
                                "empty-terminal",
                                IconName::SquareTerminal,
                                "Terminal",
                                "Start a shell in this workspace.",
                                SurfaceKind::Terminal,
                            ))
                            .child(card(
                                "empty-files",
                                IconName::Files,
                                "Files",
                                "Browse and read workspace files.",
                                SurfaceKind::Files,
                            ))
                            .child(card(
                                "empty-diff",
                                IconName::FileDiff,
                                "Diff",
                                "Review changes in this thread.",
                                SurfaceKind::Diff,
                            )),
                    ),
            )
            .into_any_element()
    }

    /// Maximize, terminal and right-panel toggles over the tab bar's right end (spec 1.8).
    fn render_titlebar_controls(
        &self,
        thread: &ThreadRef,
        inline: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let maximized = self.panels.read(cx).is_maximized(thread);
        let app_state = self.app_state.clone();
        let toggle = |id: &'static str, icon: IconName, tooltip: &'static str, command: Command| {
            let app_state = app_state.clone();
            Button::new(id)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::IconSm)
                .icon(icon)
                .tooltip(tooltip)
                .on_click(move |_, _, cx| {
                    app_state.update(cx, |state, cx| state.dispatch_command(command.clone(), cx))
                })
        };
        div()
            .absolute()
            .top_0()
            .right(px(12.))
            .h(TOPBAR_HEIGHT)
            .flex()
            .items_center()
            .gap(px(4.))
            .when(inline && self.entered(), |controls| {
                controls.child(toggle(
                    "panel-maximize",
                    if maximized {
                        IconName::Minimize2
                    } else {
                        IconName::Maximize2
                    },
                    if maximized {
                        "Restore panel size"
                    } else {
                        "Maximize panel"
                    },
                    Command::RightPanelToggleMaximized,
                ))
            })
            .child(toggle(
                "panel-terminal",
                IconName::PanelBottomOpen,
                "Toggle terminal drawer (⌘J)",
                Command::TerminalToggle,
            ))
            .child(toggle(
                "panel-toggle",
                IconName::PanelRightClose,
                "Toggle right panel (⌥⌘B)",
                Command::RightPanelToggle,
            ))
    }

    /// Tab bar plus the active surface, at a fixed `width`.
    fn render_panel(
        &self,
        thread: &ThreadRef,
        width: Option<Pixels>,
        inline: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let panel = self
            .panels
            .read(cx)
            .panel(thread)
            .cloned()
            .unwrap_or_default();
        let tabs: Vec<AnyElement> = panel
            .surfaces
            .iter()
            .map(|surface| {
                let active = panel.active.as_ref() == Some(&surface.id());
                self.render_tab(thread, surface, active, cx)
            })
            .collect();
        let has_tabs = !tabs.is_empty();
        let content = match panel
            .active
            .as_ref()
            .and_then(|active| self.surfaces.get(&(thread.clone(), active.clone())))
        {
            Some(surface) => div()
                .flex_1()
                .min_h_0()
                .child(surface.view())
                .into_any_element(),
            None if panel.active.is_none() => self.render_empty_state(thread, cx),
            None => div().flex_1().into_any_element(),
        };
        div()
            .relative()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(colors.background)
            .border_l_1()
            .border_color(colors.border)
            .font_family(font::SANS)
            .text_color(colors.foreground)
            .map(|panel| match width {
                Some(width) => panel.w(width).min_w(width).flex_none(),
                None => panel.flex_1(),
            })
            .child(
                div()
                    .id("panel-tabs")
                    .flex_none()
                    .h(TOPBAR_HEIGHT)
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pl(px(8.))
                    .pr(if inline { px(112.) } else { px(12.) })
                    .overflow_x_scroll()
                    .children(tabs)
                    .when(has_tabs, |bar| bar.child(self.render_add_menu(thread))),
            )
            .child(div().flex_1().min_h_0().flex().flex_col().child(content))
            .child(self.render_titlebar_controls(thread, inline, cx))
            .into_any_element()
    }
}

/// Tab title and 14px icon (spec 1.7 tables).
fn surface_title_icon(surface: &Surface, cx: &App) -> (SharedString, AnyElement) {
    let dark = cx.colors().is_dark;
    let icon = |name: IconName| Icon::new(name).size(px(14.)).into_any_element();
    match surface {
        Surface::Diff => ("Diff".into(), icon(IconName::FileDiff)),
        Surface::Files => ("Files".into(), icon(IconName::Files)),
        Surface::File { path, .. } => (
            path.rsplit('/').next().unwrap_or(path).to_owned().into(),
            t3_ui::file_icon(path, dark).render(px(14.)),
        ),
        Surface::Plan => ("Plan".into(), icon(IconName::ClipboardList)),
        Surface::Terminal {
            active_terminal_id, ..
        } => {
            let number = active_terminal_id
                .strip_prefix("term-")
                .unwrap_or(active_terminal_id);
            (
                format!("Terminal {number}").into(),
                icon(IconName::SquareTerminal),
            )
        }
        Surface::Preview { .. } => ("Browser".into(), icon(IconName::Globe)),
    }
}

impl Render for RightPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(thread) = self.thread.clone() else {
            return div().into_any_element();
        };
        let progress = self.progress();
        if self.animating() || self.entered() != self.presence.open {
            window.request_animation_frame();
        }
        if progress <= 0. && !self.presence.open {
            return div().into_any_element();
        }
        let colors = cx.colors();
        let window_size = window.viewport_size();
        let inline = window_size.width > INLINE_MIN_WINDOW;
        if !inline {
            // Sheet: over the whole window, `min(42vw, 448px)` (`min(88vw, 384px)` under 760px).
            let narrow = window_size.width < px(760.);
            let width = if narrow {
                (window_size.width * 0.88).min(px(384.))
            } else {
                (window_size.width * 0.42).min(px(448.)).max(px(320.))
            };
            let panels = self.panels.clone();
            let close_thread = thread.clone();
            let panel = self.render_panel(&thread, Some(width), false, cx);
            return deferred(
                anchored().position(point(px(0.), px(0.))).child(
                    div()
                        .id("panel-sheet")
                        .w(window_size.width)
                        .h(window_size.height)
                        .opacity(progress)
                        .child(
                            div()
                                .id("panel-sheet-backdrop")
                                .absolute()
                                .inset_0()
                                .bg(colors.background.opacity(0.6))
                                .on_click(move |_, _, cx| {
                                    panels.update(cx, |panels, cx| panels.close(&close_thread, cx))
                                }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right(px(-32.) * (1. - progress))
                                .w(width)
                                .child(panel),
                        ),
                ),
            )
            .with_priority(1)
            .into_any_element();
        }
        let maximized = self.panels.read(cx).is_maximized(&thread);
        if maximized {
            return self.render_panel(&thread, None, true, cx);
        }
        let width = self.drag.as_ref().map_or_else(
            || self.panels.read(cx).width(window_size.width, cx),
            |drag| drag.width,
        );
        let dragging = self.drag.is_some();
        let opacity = self.opacity();
        let panel = self.render_panel(&thread, Some(width), true, cx);
        div()
            .id("right-panel")
            .relative()
            .h_full()
            .flex_none()
            .w(width * progress)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .overflow_hidden()
                    .opacity(opacity)
                    .child(div().absolute().top_0().right_0().h_full().child(panel)),
            )
            .child(
                div()
                    .id("right-panel-resize")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-4.))
                    .w(px(8.))
                    .flex()
                    .justify_center()
                    .cursor(CursorStyle::ResizeColumn)
                    .group("panel-resize")
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_drag))
                    .child(
                        div()
                            .w(px(1.))
                            .h_full()
                            .when(dragging, |line| line.bg(colors.primary.opacity(0.6)))
                            .when(!dragging, |line| {
                                line.group_hover("panel-resize", |line| line.bg(colors.border))
                            }),
                    ),
            )
            .when(dragging, |this| {
                this.child(
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .id("right-panel-drag")
                                .w(window_size.width)
                                .h(window_size.height)
                                .cursor(CursorStyle::ResizeColumn)
                                .on_mouse_move(cx.listener(Self::drag))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| this.end_drag(cx)),
                                ),
                        ),
                    )
                    .with_priority(2),
                )
            })
            .into_any_element()
    }
}
