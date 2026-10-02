//! The sidebar footer's utility row (`SidebarChrome.tsx` `SidebarUtilityMenu`), shared by the
//! default and the legacy sidebar: Settings, Pull Requests (when any connected server offers
//! them), and Usage as 32px icon buttons; on utility pages a single "Back" button that returns
//! to the last main-app route instead.
//!
//! Colors use the current tokens; the new sidebar palette (`sidebar-row-hover`,
//! `--sidebar-icon-color`) replaces them when the regenerated tokens land.

use gpui_kit::{
    AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use t3_ui::{ActiveColors as _, Icon, IconName, TooltipExt as _, tokens::text};

use crate::{
    chrome::TypeScale as _,
    state::{AppState, Route, SettingsPage},
};

/// `--control-radius`.
const CONTROL_RADIUS: f32 = 8.;

/// True when a connected server advertises pull requests (`capabilities.pullRequests`).
fn pull_requests_supported(app_state: &AppState, cx: &App) -> bool {
    app_state.environments().iter().any(|environment| {
        environment
            .read(cx)
            .config()
            .is_some_and(|config| config.environment.capabilities.pull_requests)
    })
}

fn utility_button(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    route: Route,
    app_state: &Entity<AppState>,
    cx: &App,
) -> AnyElement {
    let colors = cx.colors();
    let app_state = app_state.clone();
    div()
        .id(id)
        .size_8()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(CONTROL_RADIUS))
        .cursor_pointer()
        .text_color(colors.muted_foreground_60)
        .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
        .tooltip_text(label)
        .on_click(move |_, _, cx| {
            let route = route.clone();
            app_state.update(cx, |state, cx| state.navigate(route, cx));
        })
        .child(Icon::new(icon).size(px(16.)))
        .into_any_element()
}

/// The footer (`SidebarFooter`: px 8, py 4, column gap 8) with the utility row.
pub(super) fn utility_footer(app_state: &Entity<AppState>, cx: &App) -> impl IntoElement {
    let colors = cx.colors();
    let state = app_state.read(cx);
    let row = div().w_full().flex().flex_row().items_center().gap_1();
    let row = if state.route().is_utility_page() {
        let app_state = app_state.clone();
        row.child(
            div()
                .id("sidebar-back")
                .flex_1()
                .min_w_0()
                .h_8()
                .px(px(10.))
                .gap_2()
                .flex()
                .items_center()
                .rounded(px(CONTROL_RADIUS))
                .cursor_pointer()
                .type_scale(text::SM)
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground_80)
                .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                .on_click(move |_, _, cx| {
                    app_state.update(cx, |state, cx| state.navigate_to_main_app(cx));
                })
                .child(Icon::new(IconName::ArrowLeft).size(px(16.)))
                .child(SharedString::from("Back")),
        )
    } else {
        let supported = pull_requests_supported(state, cx);
        let settings = utility_button(
            "sidebar-settings",
            IconName::Settings,
            "Settings",
            Route::Settings(SettingsPage::General),
            app_state,
            cx,
        );
        let pull_requests = supported.then(|| {
            utility_button(
                "sidebar-pull-requests",
                IconName::GitPullRequest,
                "Pull Requests",
                Route::PullRequests,
                app_state,
                cx,
            )
        });
        // Stand-in glyph until the icon export includes lucide `chart-no-axes-column`.
        let usage = utility_button(
            "sidebar-usage",
            IconName::Rows3,
            "Usage",
            Route::Usage,
            app_state,
            cx,
        );
        row.child(settings).children(pull_requests).child(usage)
    };
    div()
        .px_2()
        .py_1()
        .flex()
        .flex_col()
        .gap_2()
        .flex_shrink_0()
        .child(row)
}
