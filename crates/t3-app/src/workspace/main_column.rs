//! What the main column shows for each route.
//!
//! MOUNT POINT: [`build_main_view`] is where the chat, settings, and pairing views plug in.
//! Replace a placeholder arm with your view's constructor; the workspace keeps one view alive
//! per [`MainViewKey`], so a view is rebuilt only when its key changes (switching threads builds a
//! new chat view; moving between settings pages keeps the settings view and it reads the page
//! from the route). Views should render their own topbar header (52px, border-bottom) and apply
//! [`collapsed_titlebar_inset`] to its left padding.

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Pixels,
    Render, SharedString, Styled as _, Window, div, px,
};
use t3_logic::ThreadRef;
use t3_ui::{
    ActiveColors as _,
    tokens::{layout, text},
};

use super::index_view::IndexView;
use crate::{
    chat::{ChatTarget, ChatView},
    chrome::{TypeScale as _, drag_region},
    state::{AppState, DraftId, Route},
};

/// Identity of the main view. A different key builds a new view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MainViewKey {
    Index,
    Thread(ThreadRef),
    Draft(DraftId),
    /// All settings pages share one view.
    Settings,
    PullRequests,
    Usage,
    Welcome,
    Project(SharedString),
    Pair,
}

impl MainViewKey {
    pub fn for_route(route: &Route) -> Self {
        match route {
            Route::Index => Self::Index,
            Route::Thread(thread) => Self::Thread(thread.clone()),
            Route::Draft(draft) => Self::Draft(draft.clone()),
            Route::Settings(_) => Self::Settings,
            Route::PullRequests => Self::PullRequests,
            Route::Usage => Self::Usage,
            Route::Welcome => Self::Welcome,
            Route::Project(key) => Self::Project(key.clone()),
            Route::Pair => Self::Pair,
        }
    }
}

/// Builds the view for `route`.
pub fn build_main_view(
    route: &Route,
    app_state: &Entity<AppState>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    match route {
        Route::Index => cx.new(|cx| IndexView::new(app_state.clone(), cx)).into(),
        Route::Thread(thread) => {
            let target = ChatTarget::Thread(thread.clone());
            cx.new(|cx| ChatView::new(target, app_state.clone(), window, cx))
                .into()
        }
        // The draft store (composer) will name the draft's project; until then the header
        // shows no project.
        Route::Draft(id) => {
            let target = ChatTarget::Draft {
                id: id.clone(),
                project: None,
            };
            cx.new(|cx| ChatView::new(target, app_state.clone(), window, cx))
                .into()
        }
        // settings/: SettingsView (reads the page from the route).
        Route::Settings(_) => cx
            .new(|cx| Placeholder::new("Settings", app_state.clone(), cx))
            .into(),
        // pull_requests/: PullRequestsView::new(app_state, window, cx).
        Route::PullRequests => cx
            .new(|cx| Placeholder::new("Pull Requests", app_state.clone(), cx))
            .into(),
        // usage/: UsageView::new(app_state, window, cx).
        Route::Usage => cx
            .new(|cx| Placeholder::new("Usage", app_state.clone(), cx))
            .into(),
        // pages: the welcome wizard over the no-projects hero.
        Route::Welcome => cx
            .new(|cx| Placeholder::new("Welcome", app_state.clone(), cx))
            .into(),
        // pages: project links (the web redirects to /settings/projects?project=<key>).
        Route::Project(_) => cx
            .new(|cx| Placeholder::new("Project", app_state.clone(), cx))
            .into(),
        // connections: the pairing flow.
        Route::Pair => cx
            .new(|cx| Placeholder::new("Pair", app_state.clone(), cx))
            .into(),
    }
}

/// Left padding for a main-column header while the sidebar is collapsed, so the title clears the
/// traffic lights and the sidebar toggle (`--workspace-titlebar-content-left`, 130px on macOS).
/// `None` while the sidebar is open: keep the header's normal 20px padding.
pub fn collapsed_titlebar_inset(cx: &App) -> Option<Pixels> {
    let open = AppState::global(cx).read(cx).sidebar_open();
    (!open).then(|| layout::CONTROLS_LEFT + layout::TITLEBAR_CONTROL + px(12.))
}

/// Stand-in for views other modules own. Shows the route's header strip only.
struct Placeholder {
    title: SharedString,
    _app_state: gpui_kit::Subscription,
}

impl Placeholder {
    fn new(title: &'static str, app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            title: title.into(),
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for Placeholder {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let inset = collapsed_titlebar_inset(cx);
        div().size_full().flex().flex_col().child(
            drag_region("placeholder-header", window, cx)
                .h(layout::TOPBAR_HEIGHT)
                .flex_shrink_0()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(colors.border)
                .px_5()
                .map(|this| match inset {
                    Some(inset) => this.pl(inset),
                    None => this,
                })
                .type_scale(text::XS)
                .text_color(colors.muted_foreground_50)
                .child(self.title.clone()),
        )
    }
}

use gpui_kit::prelude::FluentBuilder as _;
