//! The controls row: search, Sort, Filters, Provider and refresh (route `:2544-2567`,
//! `PullRequestListFilters.tsx`). Spec section 3.4.
//!
//! Known gap: t3-ui has no submenus yet, so the Filters menu lists its groups inline under
//! group labels rather than as `MenuSub`s, and radio options are checkbox rows without the
//! option icon.

use gpui_kit::{
    AnyElement, App, Entity, Focusable as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, WeakEntity, Window,
    component::input::{Input as TextInput, InputState},
    div,
    prelude::FluentBuilder as _,
    px,
};
use t3_logic::pull_requests::{Involvement, ListSort, ListState};
use t3_ui::{
    ActiveColors as _, Align, Button, ButtonSize, ButtonVariant, DropdownMenu, Icon, IconName,
    MenuCheckboxItem, MenuGroupLabel, MenuSeparator, Spinner,
    tokens::{radius, shadow},
};

use super::PullRequestsView;

/// State tabs (`STATE_TABS`).
pub const STATES: [(ListState, &str); 4] = [
    (ListState::All, "All"),
    (ListState::Open, "Open"),
    (ListState::Closed, "Closed"),
    (ListState::Merged, "Merged"),
];

/// Involvement tabs (`INVOLVEMENT_TABS`).
pub const INVOLVEMENTS: [(Involvement, &str); 3] = [
    (Involvement::All, "All"),
    (Involvement::Reviewing, "Reviewing"),
    (Involvement::Authored, "Authored"),
];

const DRAFT_OPTIONS: [(Option<&str>, &str); 3] = [
    (None, "All"),
    (Some("only"), "Drafts only"),
    (Some("hide"), "Hide drafts"),
];
const REVIEW_OPTIONS: [(Option<&str>, &str); 5] = [
    (None, "All"),
    (Some("approved"), "Approved"),
    (Some("changes-requested"), "Changes requested"),
    (Some("review-required"), "Review required"),
    (Some("none"), "No reviews"),
];

/// `PullRequestSearchInput`: an input group with the search glyph (a spinner while a search
/// is on its way): a 32px input inside a 1px border.
pub fn search_field(
    state: &Entity<InputState>,
    busy: bool,
    window: &Window,
    cx: &App,
) -> impl IntoElement {
    let colors = cx.colors();
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    div()
        .relative()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .h(px(34.))
        .rounded(radius::CONTROL)
        .border_1()
        .border_color(if focused { colors.ring } else { colors.input })
        .bg(if colors.is_dark {
            colors.input_32
        } else {
            colors.background
        })
        .map(|this| {
            if focused {
                this.shadow(vec![gpui_kit::BoxShadow {
                    color: colors.ring_24,
                    offset: gpui_kit::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            } else if colors.is_dark {
                this
            } else {
                this.shadow(shadow::XS_5.to_vec())
            }
        })
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .pl(px(11.))
                .text_color(colors.foreground)
                .map(|this| {
                    if busy {
                        this.child(
                            div()
                                .mx(px(-2.))
                                .child(Spinner::new("pr-search-busy").size(px(16.))),
                        )
                    } else {
                        this.child(
                            Icon::new(IconName::Search)
                                .size(px(16.))
                                .mx(px(-2.))
                                .opacity(0.8),
                        )
                    }
                }),
        )
        .child(
            TextInput::new(state)
                .appearance(false)
                .text_size(px(14.))
                .pl(px(8.))
                .pr(px(11.))
                .h(px(32.))
                .line_height(px(32.))
                .flex_1(),
        )
}

/// An outline control in the row: icon and label, 32px.
fn outline_trigger(
    id: &'static str,
    icon: IconName,
    label: Option<SharedString>,
    open: bool,
) -> AnyElement {
    Button::new(id)
        .variant(ButtonVariant::Outline)
        .size(if label.is_some() {
            ButtonSize::Default
        } else {
            ButtonSize::Icon
        })
        .icon(icon)
        .when_some(label, |this, label| this.label(label))
        .pressed(open)
        .into_any_element()
}

fn radio_row(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    view: WeakEntity<PullRequestsView>,
    apply: impl Fn(&mut PullRequestsView, &mut gpui_kit::Context<PullRequestsView>) + 'static,
) -> AnyElement {
    MenuCheckboxItem::new(id, label)
        .checked(checked)
        .on_change(move |_, _, cx| {
            view.update(cx, |view, cx| apply(view, cx)).ok();
        })
        .into_any_element()
}

/// The Sort menu (`SORT_OPTIONS`).
pub fn sort_menu(
    view: &PullRequestsView,
    cx: &gpui_kit::Context<PullRequestsView>,
) -> impl IntoElement {
    let current = view.scope.sort();
    let weak = cx.weak_entity();
    DropdownMenu::new("pr-sort")
        .align(Align::Start)
        .trigger(|open| {
            outline_trigger(
                "pr-sort-trigger",
                IconName::ArrowDownUp,
                Some("Sort".into()),
                open,
            )
        })
        .items(move |_, _| {
            ListSort::ALL
                .into_iter()
                .map(|sort| {
                    radio_row(
                        ("pr-sort", sort as usize),
                        sort.label(),
                        sort == current,
                        weak.clone(),
                        move |view, cx| view.set_sort(sort, cx),
                    )
                })
                .collect()
        })
}

/// How many filters are off their defaults (the Filters trigger's count).
pub fn filter_count(view: &PullRequestsView) -> usize {
    let scope = &view.scope;
    [
        scope.state != ListState::Open,
        scope.involvement != Involvement::All,
        scope.host.is_some(),
        scope.environment_id.is_some(),
        scope.project_id.is_some(),
        scope.draft.is_some(),
        scope.review.is_some(),
        scope.checks.is_some(),
        scope.author.is_some(),
    ]
    .into_iter()
    .filter(|on| *on)
    .count()
        + scope.labels.len()
}

/// The Filters menu: state, involvement, draft, review, checks and project.
pub fn filters_menu(
    view: &PullRequestsView,
    cx: &gpui_kit::Context<PullRequestsView>,
) -> impl IntoElement {
    let count = filter_count(view);
    let scope = view.scope.clone();
    let projects = super::render::project_options(view);
    let weak = cx.weak_entity();
    let (pill_bg, pill_fg) = (cx.colors().muted, cx.colors().muted_foreground);
    DropdownMenu::new("pr-filters")
        .align(Align::End)
        .min_width(px(224.))
        .trigger(move |open| {
            Button::new("pr-filters-trigger")
                .variant(ButtonVariant::Outline)
                .size(ButtonSize::Default)
                .icon(IconName::ListFilter)
                .label("Filters")
                .pressed(open)
                .when(count > 0, |this| {
                    this.child(
                        div()
                            .rounded_full()
                            .bg(pill_bg)
                            .px(px(6.))
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(pill_fg)
                            .child(count.to_string()),
                    )
                })
                .into_any_element()
        })
        .items(move |_, _| {
            let mut items: Vec<AnyElement> = Vec::new();
            items.push(MenuGroupLabel::new("State").into_any_element());
            for (index, (state, label)) in STATES.into_iter().enumerate() {
                let checked = scope.state == state;
                items.push(radio_row(
                    ("pr-state", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| view.set_state(state.clone(), cx),
                ));
            }
            items.push(MenuSeparator.into_any_element());
            items.push(MenuGroupLabel::new("Involvement").into_any_element());
            for (index, (involvement, label)) in INVOLVEMENTS.into_iter().enumerate() {
                let checked = scope.involvement == involvement;
                items.push(radio_row(
                    ("pr-involvement", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| view.set_involvement(involvement.clone(), cx),
                ));
            }
            items.push(MenuSeparator.into_any_element());
            items.push(MenuGroupLabel::new("Draft").into_any_element());
            for (index, (value, label)) in DRAFT_OPTIONS.into_iter().enumerate() {
                let checked = scope.draft.as_deref() == value;
                items.push(radio_row(
                    ("pr-draft", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| {
                        view.update_scope(|scope| scope.draft = value.map(str::to_owned), cx)
                    },
                ));
            }
            items.push(MenuGroupLabel::new("Review").into_any_element());
            for (index, (value, label)) in REVIEW_OPTIONS.into_iter().enumerate() {
                let checked = scope.review.as_deref() == value;
                items.push(radio_row(
                    ("pr-review", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| {
                        view.update_scope(|scope| scope.review = value.map(str::to_owned), cx)
                    },
                ));
            }
            items.push(MenuSeparator.into_any_element());
            items.push(MenuGroupLabel::new("Project").into_any_element());
            items.push(radio_row(
                "pr-project-all",
                "All projects",
                scope.project_id.is_none(),
                weak.clone(),
                |view, cx| view.update_scope(|scope| scope.project_id = None, cx),
            ));
            for (index, project) in projects.into_iter().enumerate() {
                let checked = scope.project_id.as_ref() == Some(&project.id)
                    && scope
                        .environment_id
                        .as_ref()
                        .is_none_or(|id| *id == project.environment_id);
                let label = project.title.clone();
                items.push(radio_row(
                    ("pr-project", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| {
                        let (id, environment) =
                            (project.id.clone(), project.environment_id.clone());
                        view.update_scope(
                            |scope| {
                                scope.project_id = Some(id);
                                scope.environment_id = Some(environment);
                            },
                            cx,
                        )
                    },
                ));
            }
            items
        })
}

/// The Provider menu: "All" plus every host the workspace has.
pub fn provider_menu(
    view: &PullRequestsView,
    cx: &gpui_kit::Context<PullRequestsView>,
) -> impl IntoElement {
    let hosts = super::host_options(view);
    let current = view.scope.host.clone();
    let weak = cx.weak_entity();
    let icon_only = current.is_some();
    DropdownMenu::new("pr-provider")
        .align(Align::Start)
        .trigger(move |open| {
            outline_trigger(
                "pr-provider-trigger",
                IconName::Plug2,
                (!icon_only).then(|| "All".into()),
                open,
            )
        })
        .items(move |_, _| {
            let mut items = vec![radio_row(
                "pr-host-all",
                "All",
                current.is_none(),
                weak.clone(),
                |view, cx| view.set_host(None, cx),
            )];
            for (index, (host, label)) in hosts.into_iter().enumerate() {
                let checked = current.as_deref() == Some(host.as_str());
                items.push(radio_row(
                    ("pr-host", index),
                    label,
                    checked,
                    weak.clone(),
                    move |view, cx| view.set_host(Some(host.clone()), cx),
                ));
            }
            items
        })
}

/// The refresh button: outline 32px in the controls row, ghost 28px in the condensed topbar.
pub fn refresh_button(
    id: &'static str,
    compact: bool,
    refreshing: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .variant(if compact {
            ButtonVariant::Ghost
        } else {
            ButtonVariant::Outline
        })
        .size(if compact {
            ButtonSize::IconSm
        } else {
            ButtonSize::Icon
        })
        .icon(IconName::RefreshCw)
        .disabled(refreshing)
        .tooltip("Refresh pull requests")
        .on_click(on_click)
}
