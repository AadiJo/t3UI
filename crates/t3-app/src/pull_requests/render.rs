//! Lays out the page (spec section 3) and turns the reads into the groups on screen
//! (spec 2.3): carried rows, involvement, local filters, partitions, overrides, line counts
//! and the Sort menu's order.

use std::sync::Arc;

use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use t3_logic::{
    pull_requests::{
        self as logic, EnvironmentEntry, Group, GroupKey, Involvement, ListState, MergedList,
        ScopeProject, Viewers,
    },
    time::format_relative_time,
};
use t3_ui::{ActiveColors as _, Button, ButtonSize, ButtonVariant, IconName, ScrollArea, Theme};

use super::{
    MAX_PAGE_SIZE, PullRequestsView, controls,
    list::{self, RowProps},
    states::{self, StateAction},
};
use crate::pages::chrome::{
    PageWidth, breadcrumb, breadcrumb_item, page_container, page_header, topbar_scroll_fade,
};

/// What the list body shows (spec section 4, in precedence order).
enum Body {
    Ghost,
    Unsupported,
    Failed(String),
    Empty,
    Groups(Vec<Group>),
}

/// Everything render needs from the reads, computed once per frame.
struct Display {
    body: Body,
    shown: usize,
    viewers: Viewers,
    error: Option<String>,
    truncated: bool,
    loading_more: bool,
    show_provider: bool,
    many_environments: bool,
    filtered: bool,
}

impl PullRequestsView {
    fn display(&self) -> Display {
        let workspace = &self.workspace;
        let mut display = Display {
            body: Body::Ghost,
            shown: 0,
            viewers: Viewers::new(),
            error: None,
            truncated: false,
            loading_more: false,
            show_provider: false,
            many_environments: workspace.environments.len() > 1,
            filtered: self.is_filtered(),
        };
        if !workspace.capability_known {
            return display;
        }
        if workspace.environments.is_empty() {
            display.body = Body::Unsupported;
            return display;
        }
        let current = self.list.key == self.filter_key();
        let answered = self.list.data.as_ref().filter(|_| current);
        let carried = self
            .carried
            .as_ref()
            .map(|carried| self.narrow_carried(carried));
        let list = answered.or(carried.as_ref());
        let first_load = self.list.pending && list.is_none();
        display.error = self.list.error.clone().filter(|_| current);
        let Some(list) = list else {
            display.body = match &display.error {
                Some(error) if !first_load => Body::Failed(error.clone()),
                _ => Body::Ghost,
            };
            return display;
        };
        display.viewers = list.viewers.clone();
        display.truncated = list.truncated;
        display.loading_more = self.list.pending;
        display.show_provider = list.providers.len() > 1;

        let entries = self.visible_entries(list, answered.is_none());
        let groups = self.group(entries, &list.viewers);
        let groups = self.finish_groups(groups);
        display.shown = groups.iter().map(|group| group.entries.len()).sum();
        display.body = if display.shown > 0 {
            Body::Groups(groups)
        } else if let Some(error) = display.error.clone() {
            Body::Failed(error)
        } else if answered.is_none() && self.list.pending && self.typed_query().is_empty() {
            Body::Ghost
        } else {
            Body::Empty
        };
        display
    }

    /// The last answer, narrowed to filters a row can be judged by on its own.
    fn narrow_carried(&self, carried: &MergedList) -> MergedList {
        let (project, _) = self.scoped_project();
        MergedList {
            entries: logic::narrow_to_filters(
                &carried.entries,
                &self.scope.state,
                project.as_ref(),
                self.scope.host.as_deref(),
            ),
            ..carried.clone()
        }
    }

    /// Steps 1-4 of spec 2.3: held order, involvement, local filters, text.
    fn visible_entries(&self, list: &MergedList, carrying: bool) -> Vec<Arc<EnvironmentEntry>> {
        let known = if !carrying && self.held_key == self.list.key {
            self.held.clone()
        } else {
            list.entries.clone()
        };
        let involved = logic::filter_by_involvement(&known, &list.viewers, &self.scope.involvement);
        let typed = logic::parse_query(&self.typed_query());
        let filters = self.request_filters(&typed);
        let narrowed: Vec<_> = involved
            .into_iter()
            .filter(|entry| {
                logic::matches_filters(
                    entry,
                    &filters,
                    logic::entry_viewer(entry, &list.viewers).as_deref(),
                )
            })
            .collect();
        if typed.text.is_empty() {
            let mut sorted = narrowed;
            sorted.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            return sorted;
        }
        let settled = self.sent_query == self.typed_query() && !carrying;
        let searching_hosts: Vec<&str> = list
            .providers
            .iter()
            .filter(|provider| provider.searches_on_host)
            .map(|provider| provider.host.as_str())
            .collect();
        narrowed
            .into_iter()
            .filter(|entry| {
                (settled && searching_hosts.contains(&entry.host.as_str()))
                    || logic::matches_query(entry, &typed.text)
            })
            .collect()
    }

    /// Step 5: the involvement groups, from the partitions' own reads where both answered.
    fn group(&self, entries: Vec<Arc<EnvironmentEntry>>, viewers: &Viewers) -> Vec<Group> {
        if self.scope.involvement != Involvement::All {
            return vec![Group {
                key: GroupKey::Others,
                labeled: false,
                entries,
            }];
        }
        let partitions = self.typed_query().is_empty()
            && self.authored.key.ends_with("|partitions")
            && self.authored.key.starts_with(&self.filter_key());
        match (
            partitions.then_some(self.authored.data.as_ref()).flatten(),
            partitions.then_some(self.reviewing.data.as_ref()).flatten(),
        ) {
            (Some(authored), Some(reviewing)) => {
                logic::partition_with_priority(&entries, &authored.entries, &reviewing.entries)
            }
            _ => logic::group_by_involvement(&entries, viewers),
        }
    }

    /// Steps 6-7: line counts, pending overrides, the Sort menu's order.
    fn finish_groups(&self, groups: Vec<Group>) -> Vec<Group> {
        let enriched: Vec<Group> = groups
            .into_iter()
            .map(|group| {
                let with_stats: Vec<_> = group
                    .entries
                    .iter()
                    .map(|entry| logic::with_diff_stat(entry, &self.stats))
                    .collect();
                Group {
                    entries: logic::apply_overrides(
                        &with_stats,
                        &self.overrides,
                        &self.scope.state,
                    ),
                    ..group
                }
            })
            .filter(|group| !group.entries.is_empty())
            .collect();
        let text = logic::parse_query(&self.typed_query()).text;
        logic::sort_groups(
            &enriched,
            self.scope.sort(),
            &text,
            &|entry| entry.additions + entry.deletions > 0,
            &self.scope.involvement,
        )
    }

    /// "Filtered" for the empty state: any narrowing beyond the defaults.
    fn is_filtered(&self) -> bool {
        let scope = &self.scope;
        scope.state != ListState::Open
            || scope.involvement != Involvement::All
            || scope.project_id.is_some()
            || scope.host.is_some()
            || scope.draft.is_some()
            || scope.review.is_some()
            || scope.checks.is_some()
            || scope.author.is_some()
            || !scope.labels.is_empty()
    }

    fn header(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let supported = !self.workspace.environments.is_empty();
        page_header("pull-requests-header", window, cx)
            .relative()
            .child(
                breadcrumb().child(
                    breadcrumb_item(true, cx).child(div().truncate().child("Pull Requests")),
                ),
            )
            .child(div().flex_1().min_w_0())
            .when(supported, |this| {
                // Footprint reserve so refresh never slides under the floating toggle.
                this.child(div().flex_shrink_0().w_5()).child(
                    div()
                        .absolute()
                        .top_0()
                        .right(px(13.))
                        .h_full()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("pr-right-panel")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::IconSm)
                                .icon(IconName::PanelRight)
                                .disabled(self.selected.is_none())
                                .tooltip(if self.selected.is_some() {
                                    "Toggle right panel"
                                } else {
                                    "Select a pull request first"
                                }),
                        ),
                )
            })
    }

    fn controls_row(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let typed = self.typed_query();
        let busy = !typed.is_empty() && (typed != self.sent_query || self.list.pending);
        let refreshing = self.invalidating || self.list.pending;
        div().flex().flex_col().gap_3().child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(div().min_w_0().flex_1().child(controls::search_field(
                    &self.search,
                    busy,
                    window,
                    cx,
                )))
                .child(controls::sort_menu(self, cx))
                .child(controls::filters_menu(self, cx))
                .child(controls::provider_menu(self, cx))
                .child(controls::refresh_button(
                    "pr-refresh",
                    false,
                    refreshing,
                    cx.listener(|this, _, _, cx| this.refresh_from_host(cx)),
                )),
        )
    }

    fn list_body(
        &self,
        mut display: Display,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let refreshing = self.invalidating || self.list.pending;
        let check_again = |cx: &mut Context<Self>| {
            StateAction::outline(
                "pr-check-again",
                if refreshing {
                    "Checking..."
                } else {
                    "Check again"
                },
                Some(IconName::RefreshCw),
                cx.listener(|this, _, _, cx| this.refresh_from_host(cx)),
            )
            .disabled(refreshing)
        };
        match std::mem::replace(&mut display.body, Body::Ghost) {
            Body::Ghost => states::list_ghost(7, None, cx),
            Body::Unsupported => states::unavailable_state(
                "Pull requests unavailable",
                "Update your T3 Code servers to browse pull requests.",
                Vec::new(),
                cx,
            ),
            Body::Failed(error) => states::unavailable_state(
                "Could not load pull requests",
                error,
                vec![
                    StateAction::outline(
                        "pr-retry",
                        "Retry",
                        Some(IconName::RefreshCw),
                        cx.listener(|this, _, _, cx| this.reread(cx)),
                    )
                    .disabled(self.list.pending),
                ],
                cx,
            ),
            Body::Empty => {
                self.empty_body(display.filtered, display.truncated, check_again(cx), cx)
            }
            Body::Groups(groups) => {
                let rows = self.groups(groups, &display, window, cx);
                let mut column = div().flex().flex_col().gap_3().children(rows);
                if let Some(error) = display.error.as_deref() {
                    column = column.child(states::error_banner(
                        error,
                        StateAction::outline(
                            "pr-banner-retry",
                            "Retry",
                            None,
                            cx.listener(|this, _, _, cx| this.reread(cx)),
                        ),
                        cx,
                    ));
                }
                if display.truncated {
                    let content = if display.loading_more {
                        div().child("Loading more").into_any_element()
                    } else if self.page_size < MAX_PAGE_SIZE {
                        Button::new("pr-load-more")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .label("Load more pull requests")
                            .on_click(cx.listener(|this, _, _, cx| this.load_more(cx)))
                            .into_any_element()
                    } else {
                        div()
                            .child("Narrow your search to find more pull requests.")
                            .into_any_element()
                    };
                    column = column.child(states::load_more_footer(content, cx));
                }
                column.into_any_element()
            }
        }
    }

    fn empty_body(
        &self,
        filtered: bool,
        truncated: bool,
        check_again: StateAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_projects = !self.workspace.projects_known || !self.capable_projects().is_empty();
        if !has_projects {
            return states::empty_state(
                "No projects in this workspace",
                "Add a project, and the pull requests from its repository appear here.",
                vec![
                    StateAction::outline(
                        "pr-add-project",
                        "Add project",
                        Some(IconName::Plus),
                        |_, _, _| {},
                    )
                    .variant(ButtonVariant::Default),
                ],
                cx,
            );
        }
        let typed = self.typed_query();
        if !typed.is_empty() {
            if typed != self.sent_query || self.list.pending {
                return states::list_ghost(
                    5,
                    Some(
                        format!("Searching every host for \u{201c}{}\u{201d}", clip(&typed)).into(),
                    ),
                    cx,
                );
            }
            return states::empty_state(
                format!("Nothing matches \u{201c}{}\u{201d}", clip(&typed)),
                "The hosts were searched for it. Try fewer words, or search by number, author or branch.",
                vec![
                    StateAction::outline(
                        "pr-clear-search",
                        "Clear search",
                        Some(IconName::Search),
                        cx.listener(|this, _, window, cx| this.clear_query(window, cx)),
                    ),
                    check_again,
                ],
                cx,
            );
        }
        let mut buttons = Vec::new();
        if truncated && self.page_size < MAX_PAGE_SIZE {
            buttons.push(
                StateAction::outline(
                    "pr-empty-load-more",
                    if self.list.pending {
                        "Loading..."
                    } else {
                        "Load more pull requests"
                    },
                    None,
                    cx.listener(|this, _, _, cx| this.load_more(cx)),
                )
                .disabled(self.list.pending),
            );
        }
        buttons.push(check_again);
        let (title, description) = if filtered {
            (
                "Nothing under these filters",
                "Widen the state, involvement or project filter to see more.",
            )
        } else {
            (
                "No pull requests",
                "Pull requests from every project in this workspace appear here.",
            )
        };
        states::empty_state(title, description, buttons, cx)
    }

    fn groups(
        &self,
        groups: Vec<Group>,
        display: &Display,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.colors();
        let now = self.app_state.read(cx).now_millis();
        let mono: SharedString = Theme::global(cx).mono_family().clone();
        let typed = logic::parse_query(&self.typed_query()).text;
        let meta_width = self.meta_width(window, cx);
        groups
            .into_iter()
            .map(|group| {
                let count = group.entries.len();
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .when(group.labeled, |this| {
                        this.child(list::group_header(group.key, count, colors))
                    })
                    .children(group.entries.into_iter().map(|entry| {
                        let key = logic::entry_key(&entry);
                        let props = RowProps {
                            selected: self.selected.as_deref() == Some(key.as_str()),
                            show_provider: display.show_provider,
                            environment_label: display
                                .many_environments
                                .then(|| self.workspace.labels.get(&entry.environment_id).cloned())
                                .flatten()
                                .map(SharedString::from),
                            matched_elsewhere: !typed.is_empty()
                                && logic::score_match(&entry, &typed)
                                    <= logic::MATCHED_ELSEWHERE_SCORE,
                            updated: format_relative_time(&entry.updated_at, now).into(),
                            meta_width,
                            mono: mono.clone(),
                            entry,
                        };
                        let select = key.clone();
                        list::row(
                            SharedString::from(key),
                            props,
                            cx.listener(move |this, _, _, cx| this.select(select.clone(), cx)),
                            cx,
                        )
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// The second line's width, standing in for the row's container queries: the main column
    /// (window less the open sidebar), capped at the content frame, less paddings and glyph.
    fn meta_width(&self, window: &Window, cx: &Context<Self>) -> Pixels {
        let sidebar = if self.app_state.read(cx).sidebar_open() {
            t3_ui::tokens::layout::SIDEBAR_WIDTH
        } else {
            px(0.)
        };
        let column = window.viewport_size().width - sidebar;
        column.min(PageWidth::Expanded.max_width()) - px(48.) - px(24.) - px(24.)
    }
}

/// A search echoed in a title, cut at 48 characters.
fn clip(query: &str) -> String {
    if query.chars().count() > 48 {
        format!("{}\u{2026}", query.chars().take(48).collect::<String>())
    } else {
        query.to_owned()
    }
}

/// The Filters menu's projects (`pullRequestFilterProjects`).
pub fn project_options(view: &PullRequestsView) -> Vec<ScopeProject> {
    let (project, _) = view.scoped_project();
    let environment = view.scope.environment_id.clone();
    let selected = project.as_ref().zip(environment.as_ref());
    logic::filter_projects(&view.capable_projects(), &view.workspace.labels, selected)
}

impl Render for PullRequestsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let display = self.display();
        let body = self.list_body(display, window, cx);
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(self.header(window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        ScrollArea::new("pull-requests-scroll").child(
                            page_container(PageWidth::Expanded)
                                .gap_4()
                                .min_h_full()
                                .child(self.controls_row(window, cx))
                                .child(body),
                        ),
                    )
                    .child(topbar_scroll_fade(cx)),
            )
    }
}
