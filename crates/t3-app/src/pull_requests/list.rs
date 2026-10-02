//! The list's rows and group headers (`PullRequestRow.tsx`, `PullRequestListRow.tsx`, route
//! `PullRequestGroupHeader`). Spec section 4.1-4.2.

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, prelude::FluentBuilder as _, px,
};
use t3_logic::pull_requests::{
    EnvironmentEntry, GroupKey, StateKey, checks_state_label, conflict_label, format_count,
    label_color, provider_name, review_decision_label,
};
use t3_protocol::orchestration::PullRequestActor;
use t3_ui::{
    ActiveColors as _, Colors, Icon, IconName, TooltipExt as _,
    tokens::{radius, text},
};

use super::style;
use crate::chrome::TypeScale as _;

/// Meta line widths at which the 2nd and 3rd label chips appear (`@xl`, `@3xl`) and the author
/// login stops being screen-reader only (`@xs`).
const META_XS: Pixels = px(320.);
const META_XL: Pixels = px(576.);
const META_3XL: Pixels = px(768.);

/// What one row shows beyond its entry.
pub struct RowProps {
    pub entry: Arc<EnvironmentEntry>,
    pub selected: bool,
    pub show_provider: bool,
    pub environment_label: Option<SharedString>,
    pub matched_elsewhere: bool,
    /// "5m ago", against the page's clock.
    pub updated: SharedString,
    /// Estimated width of the second line, standing in for the row's container queries.
    pub meta_width: Pixels,
    pub mono: SharedString,
}

/// A list row (`PullRequestRow`): status glyph, number and title over author, repository and
/// labels; diff counts and time on the right.
pub fn row(
    id: impl Into<gpui_kit::ElementId>,
    props: RowProps,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.colors();
    let entry = &props.entry;
    let state = StateKey::resolve(&entry.state, entry.is_draft);
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .w_full()
        .rounded(radius::SM)
        .px_3()
        .py(px(10.))
        .cursor_pointer()
        .map(|this| {
            if props.selected {
                this.bg(colors.accent)
            } else {
                this.hover(|style| style.bg(colors.accent_60))
            }
        })
        .on_click(on_click)
        .child(glyph_column(entry, state, colors))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(title_line(entry, &props, colors))
                .child(meta_line(entry, &props, colors)),
        )
        .into_any_element()
}

/// The lifecycle glyph with the conflict badge riding its corner.
fn glyph_column(entry: &EnvironmentEntry, state: StateKey, colors: &Colors) -> impl IntoElement {
    let conflict = conflict_label(
        &entry.state,
        entry.is_draft,
        &entry.mergeability,
        Some(&entry.base_branch),
    );
    div()
        .w_4()
        .flex_shrink_0()
        .self_start()
        .mt(px(3.))
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .id("state")
                .relative()
                .flex()
                .tooltip_text(state.label())
                .child(
                    Icon::new(style::state_icon(state))
                        .size(px(16.))
                        .color(style::state_tone(state, colors)),
                )
                .when_some(conflict, |this, label| {
                    this.child(
                        div()
                            .id("conflict")
                            .absolute()
                            .right(px(-4.))
                            .bottom(px(-4.))
                            .rounded_full()
                            .bg(colors.background)
                            .tooltip_text(label)
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .size(px(12.))
                                    .color(colors.destructive),
                            ),
                    )
                }),
        )
}

fn title_line(entry: &EnvironmentEntry, props: &RowProps, colors: &Colors) -> impl IntoElement {
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.))
        .child(
            div()
                .flex_shrink_0()
                .font_family(props.mono.clone())
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(format!("#{}", entry.number)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .type_scale(text::SM)
                .text_color(colors.foreground)
                .child(entry.title.clone()),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .when_some(entry.checks_state.as_ref(), |this, checks| {
                    let (glyph, tone) = style::checks_glyph(checks, colors);
                    this.child(
                        div()
                            .id("checks")
                            .flex()
                            .tooltip_text(checks_state_label(checks))
                            .child(Icon::new(glyph).size(px(14.)).color(tone)),
                    )
                })
                .when_some(entry.review_decision.as_ref(), |this, decision| {
                    let (glyph, tone) = style::review_glyph(decision, colors);
                    this.child(
                        div()
                            .id("review")
                            .flex()
                            .tooltip_text(review_decision_label(decision))
                            .child(Icon::new(glyph).size(px(14.)).color(tone)),
                    )
                }),
        )
        .child(
            div()
                .ml_auto()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .type_scale(text::XS2)
                .child(diff_stat(
                    entry.additions,
                    entry.deletions,
                    props.mono.clone(),
                    colors,
                )),
        )
}

/// `+a -d`, or nothing when the host reported no counts.
pub fn diff_stat(
    additions: u64,
    deletions: u64,
    mono: SharedString,
    colors: &Colors,
) -> AnyElement {
    if additions == 0 && deletions == 0 {
        return div().into_any_element();
    }
    div()
        .flex()
        .gap_1()
        .font_family(mono)
        .child(
            div()
                .text_color(style::diff_addition(colors))
                .child(format!("+{}", format_count(additions))),
        )
        .child(
            div()
                .text_color(style::diff_deletion(colors))
                .child(format!("-{}", format_count(deletions))),
        )
        .into_any_element()
}

fn meta_line(entry: &EnvironmentEntry, props: &RowProps, colors: &Colors) -> impl IntoElement {
    let label_slots = if props.meta_width >= META_3XL {
        3
    } else if props.meta_width >= META_XL {
        2
    } else {
        1
    };
    let shown = entry.labels.len().min(label_slots);
    let remaining = entry.labels.len().saturating_sub(shown);
    div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(6.))
        .overflow_hidden()
        .type_scale(text::XS2)
        .text_color(colors.muted_foreground)
        .when(props.matched_elsewhere, |this| {
            this.child(
                div()
                    .id("matched")
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .rounded_full()
                    .border_1()
                    .border_color(colors.border_60)
                    .px_1()
                    .type_scale(text::XS3)
                    .tooltip_text("Matched in the description")
                    .child(Icon::new(IconName::Search).size(px(12.)))
                    .when(props.meta_width >= META_XS, |this| {
                        this.child("matched in the description")
                    }),
            )
        })
        .when(props.show_provider, |this| {
            this.child(
                div()
                    .id("provider")
                    .flex()
                    .flex_shrink_0()
                    .tooltip_text(provider_name(&entry.provider))
                    .when_some(style::provider_logo(&entry.provider), |this, mark| {
                        this.child(t3_ui::logo(mark, colors.is_dark, px(12.)))
                    }),
            )
        })
        .child(author(
            entry.author.as_ref(),
            props.meta_width >= META_XS,
            colors,
        ))
        .child(div().min_w_0().truncate().child(entry.repository.clone()))
        .when_some(props.environment_label.clone(), |this, label| {
            this.child(div().min_w_0().max_w(px(128.)).truncate().child(label))
        })
        .when(shown > 0, |this| {
            this.child(
                div().flex().min_w_0().items_center().gap_1().children(
                    entry
                        .labels
                        .iter()
                        .take(shown)
                        .enumerate()
                        .map(|(index, label)| {
                            let overflow =
                                (index + 1 == shown && remaining > 0).then_some(remaining);
                            label_chip(&label.name, label.color.as_deref(), overflow, colors)
                        }),
                ),
            )
        })
        .child(
            div()
                .ml_auto()
                .flex_shrink_0()
                .whitespace_nowrap()
                .child(props.updated.clone()),
        )
}

/// Avatar and login at the meta line's size; the login is hidden on narrow rows.
fn author(actor: Option<&PullRequestActor>, show_login: bool, colors: &Colors) -> impl IntoElement {
    let login = actor
        .map_or("ghost", |actor| actor.login.as_str())
        .to_owned();
    let tooltip = match actor.and_then(|actor| actor.name.as_deref()) {
        Some(name) if name != login => format!("{name} (@{login})"),
        _ => login.clone(),
    };
    div()
        .id("author")
        .flex()
        .min_w(px(14.))
        .max_w(px(160.))
        .items_center()
        .gap_1()
        .tooltip_text(tooltip)
        .child(avatar(&login, px(14.), colors))
        .when(show_login, |this| {
            this.child(div().min_w_0().truncate().child(login))
        })
}

/// The initial-letter avatar the fork falls back to without an image.
pub fn avatar(login: &str, size: Pixels, colors: &Colors) -> impl IntoElement {
    let initial = login
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .flex()
        .flex_shrink_0()
        .size(size)
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(colors.muted)
        .type_scale(text::XS3)
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .child(initial)
}

/// A host label as a tinted tag (`PullRequestLabelChip`, badge `sm`), with an optional "+N".
pub fn label_chip(
    name: &str,
    color: Option<&str>,
    overflow: Option<usize>,
    colors: &Colors,
) -> impl IntoElement {
    let (background, foreground): (Hsla, Hsla) = match label_color(color) {
        Some(rgb) => style::label_chip_colors(rgb, colors),
        None => (colors.secondary, colors.secondary_foreground),
    };
    div()
        .flex()
        .min_w_0()
        .max_w(px(160.))
        .h_4()
        .min_w_4()
        .items_center()
        .gap_1()
        .px(px(3.))
        .rounded(radius::ROUNDED)
        .bg(background)
        .text_color(foreground)
        .text_size(px(10.))
        .line_height(px(10.))
        .font_weight(FontWeight::MEDIUM)
        .child(div().min_w_0().truncate().child(name.to_owned()))
        .when_some(overflow, |this, count| {
            this.child(div().flex_shrink_0().child(format!("+{count}")))
        })
}

/// A group's header: glyph, name, count, then a rule to the edge.
pub fn group_header(key: GroupKey, count: usize, colors: &Colors) -> impl IntoElement {
    let glyph = match key {
        GroupKey::Authored => IconName::PenLine,
        GroupKey::ReviewRequested => IconName::Eye,
        GroupKey::Others => IconName::Users,
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .pb_1()
        .type_scale(text::XS)
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground_70)
        .child(Icon::new(glyph).size(px(14.)))
        .child(div().flex_shrink_0().child(key.label()))
        .child(
            div()
                .flex_shrink_0()
                .text_color(colors.muted_foreground_50)
                .child(count.to_string()),
        )
        .child(div().flex_1().min_w_2().h_px().bg(colors.border))
}
