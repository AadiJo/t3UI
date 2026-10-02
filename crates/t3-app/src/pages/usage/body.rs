//! The Cost / Tokens body (`UsagePage.tsx:475-770`): summary column and chart, totals, and the
//! breakdown table; plus the loading skeleton (`:1135-1191`).

use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    relative,
};
use t3_logic::usage::{
    Metric, ProviderKind,
    format::{format_count, format_day_short, format_percent, format_tokens, format_usd},
    merge::sort_models_by_tokens,
};
use t3_ui::{
    ActiveColors as _, Icon, IconName, Skeleton, TooltipExt as _, logo,
    tokens::{radius, text},
};

use super::{Breakdown, UsageView, provider_color, provider_logo};
use crate::{
    chrome::TypeScale as _,
    pages::controls::{Segment, segmented, tabular_nums},
};

/// Tailwind `lg`: the summary sits beside the chart from here up.
const LG: f32 = 1024.;
/// Tailwind `text-4xl` (36/40), the headline figure. Not in the token scale.
const TEXT_4XL: (gpui_kit::Pixels, gpui_kit::Pixels) = (px(36.), px(40.));

/// A `text-sm text-muted-foreground` paragraph.
pub(super) fn muted_paragraph(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .type_scale(text::SM)
        .text_color(cx.colors().muted_foreground)
        .child(text.into())
}

/// `h2 text-sm font-medium`.
fn heading(text: impl Into<SharedString>, cx: &App) -> gpui_kit::Div {
    div()
        .type_scale(text::SM)
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.colors().foreground)
        .child(text.into())
}

fn sessions_label(count: u64) -> String {
    format!(
        "{} {}",
        format_count(count as f64),
        if count == 1 { "session" } else { "sessions" }
    )
}

impl UsageView {
    pub(super) fn render_body(
        &mut self,
        metric: Metric,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.colors();
        let merged = self.merged.clone();
        let hourly = self.window.since_time.is_some();
        let wide = window.viewport_size().width >= px(LG);
        let active = merged.active_providers();
        let format_value = |cost: f64, tokens: u64| match metric {
            Metric::Tokens => format_tokens(tokens as f64),
            _ => format_usd(cost),
        };

        // Partial/failed/Cursor source notes without an action, once each.
        let mut source_messages: Vec<String> = Vec::new();
        for status in self.selected_statuses() {
            let Some(summary) = &status.summary else {
                continue;
            };
            for source in &summary.sources {
                let Some(message) = &source.message else {
                    continue;
                };
                let noteworthy = source.status == "partial"
                    || source.status == "failed"
                    || source.fingerprint.provider.as_str() == "cursor";
                if noteworthy && source.action.is_none() && !source_messages.contains(message) {
                    source_messages.push(message.clone());
                }
            }
        }

        // Summary column.
        let mut subline = sessions_label(merged.sessions);
        if metric == Metric::Cost {
            subline.push_str(" · API estimate");
        }
        let unpriced = merged.cost_quality.unpriced_share;
        let headline = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .type_scale(TEXT_4XL)
                    .font_weight(FontWeight::SEMIBOLD)
                    .font_features(tabular_nums())
                    .text_color(colors.foreground)
                    .child(format_value(merged.cost_usd, merged.total_tokens)),
            )
            .child(
                div()
                    .id("usage-subline")
                    .flex()
                    .items_center()
                    .gap_1()
                    .type_scale(text::XS)
                    .text_color(colors.muted_foreground)
                    .child(subline)
                    .when(metric == Metric::Cost && unpriced > 0., |this| {
                        this.child(
                            div()
                                .id("usage-unpriced")
                                .tooltip_text(format!(
                                    "API estimate excludes {} unpriced records.",
                                    format_percent(unpriced)
                                ))
                                .child(Icon::new(IconName::Info).size(px(12.))),
                        )
                    }),
            );
        let provider_rows = active.iter().map(|&provider| {
            let totals = merged.provider(provider);
            let (cost, tokens, sessions, cost_share, token_share) =
                totals.map_or((0., 0, 0, 0., 0.), |totals| {
                    (
                        totals.cost_usd,
                        totals.total_tokens,
                        totals.sessions,
                        totals.cost_share,
                        totals.token_share,
                    )
                });
            let share_line = match metric {
                Metric::Tokens => format!(
                    "{} of tokens · {}",
                    format_percent(token_share),
                    format_usd(cost)
                ),
                _ => format!(
                    "{} of cost · {} tokens",
                    format_percent(cost_share),
                    format_tokens(tokens as f64)
                ),
            };
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .justify_between()
                        .gap_4()
                        .child(
                            div()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_2()
                                .type_scale(text::SM)
                                .text_color(colors.foreground)
                                .child(
                                    div()
                                        .size_2()
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(provider_color(provider, colors)),
                                )
                                .child(logo(provider_logo(provider), colors.is_dark, px(16.)))
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .items_baseline()
                                        .gap(px(6.))
                                        .child(div().truncate().child(provider.label()))
                                        .child(
                                            div()
                                                .flex_shrink_0()
                                                .whitespace_nowrap()
                                                .type_scale(text::XS2)
                                                .font_features(tabular_nums())
                                                .text_color(colors.muted_foreground)
                                                .child(sessions_label(sessions)),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .type_scale(text::SM)
                                .font_weight(FontWeight::MEDIUM)
                                .font_features(tabular_nums())
                                .text_color(colors.foreground)
                                .child(format_value(cost, tokens)),
                        ),
                )
                .child(
                    div()
                        .type_scale(text::XS)
                        .text_color(colors.muted_foreground)
                        .child(share_line),
                )
        });
        let summary = div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_5()
            .when(wide, |this| this.w(px(288.)).flex_shrink_0())
            .child(headline)
            .children(provider_rows);

        let chart_title = format!(
            "{} {}",
            if hourly { "Hourly" } else { "Daily" },
            if metric == Metric::Tokens {
                "processed tokens"
            } else {
                "cost"
            }
        );
        let chart = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .gap_3()
            .child(heading(chart_title, cx))
            .child(self.render_chart(metric, &active, cx));

        let overview = div()
            .w_full()
            .flex()
            .gap_6()
            .map(|this| {
                if wide {
                    this.flex_row()
                } else {
                    this.flex_col()
                }
            })
            .child(summary)
            .child(chart);

        let mut sections: Vec<AnyElement> = Vec::new();
        if !source_messages.is_empty() {
            // `mb-4` on each message, inside the container's 24px gap.
            sections.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .mb(px(-8.))
                    .children(
                        source_messages
                            .into_iter()
                            .map(|message| muted_paragraph(message, cx)),
                    )
                    .into_any_element(),
            );
        }
        sections.push(overview.into_any_element());
        sections.push(self.render_totals(cx).into_any_element());
        sections.push(
            self.render_breakdown(metric, &active, cx)
                .into_any_element(),
        );
        sections
    }

    /// Totals (`UsagePage.tsx:611-626`): five figures, two columns below `md`.
    fn render_totals(&self, cx: &App) -> impl IntoElement {
        let colors = cx.colors();
        let merged = &self.merged;
        let metric = |label: &'static str, value: String| {
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .type_scale(text::XS)
                        .text_color(colors.muted_foreground)
                        .child(label),
                )
                .child(
                    div()
                        .type_scale(text::BASE)
                        .font_weight(FontWeight::MEDIUM)
                        .font_features(tabular_nums())
                        .text_color(colors.foreground)
                        .child(value),
                )
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading("Totals", cx))
            .child(
                div()
                    .grid()
                    .grid_cols(5)
                    .gap_x_6()
                    .gap_y_4()
                    .py_1()
                    .child(metric(
                        "Processed tokens",
                        format_tokens(merged.total_tokens as f64),
                    ))
                    .child(metric(
                        "Cached input",
                        format_tokens(merged.cached_input_tokens as f64),
                    ))
                    .child(metric(
                        "Uncached input",
                        format_tokens(merged.uncached_input_tokens as f64),
                    ))
                    .child(metric("Output", format_tokens(merged.output_tokens as f64)))
                    .child(metric(
                        "Cache savings",
                        format_usd(merged.cost_quality.cache_savings_usd),
                    )),
            )
    }

    /// Breakdown (`UsagePage.tsx:628-769`): by model or by period.
    fn render_breakdown(
        &self,
        metric: Metric,
        active: &[ProviderKind],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let hourly = self.window.since_time.is_some();
        let breakdown = self.breakdown;
        let toggle = segmented(
            "usage-breakdown",
            [
                (Breakdown::Model, "Model"),
                (Breakdown::Time, if hourly { "Hour" } else { "Day" }),
            ]
            .into_iter()
            .map(|(value, label)| Segment {
                id: SharedString::from(format!("usage-breakdown-{label}")).into(),
                label: label.into(),
                pressed: value == breakdown,
                tooltip: None,
                on_click: Box::new(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_breakdown(value, cx)),
                ),
            })
            .collect(),
            false,
            cx,
        );

        // Column widths as fractions of the row; text cells left, figures right.
        let cell = |fraction: f32, right: bool| {
            div()
                .w(relative(fraction))
                .flex_shrink_0()
                .min_w_0()
                .py_2()
                .when(right, |this| this.flex().justify_end())
        };
        let head_row = |cells: Vec<(f32, bool, SharedString)>| {
            div()
                .flex()
                .border_b_1()
                .border_color(colors.border)
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .children(
                    cells
                        .into_iter()
                        .map(|(fraction, right, label)| cell(fraction, right).child(label)),
                )
        };
        let body_row = |id: SharedString| {
            div()
                .id(id)
                .flex()
                .border_b_1()
                .border_color(colors.border_50)
                .hover(|style| style.bg(colors.muted_50))
        };
        let empty = || {
            div()
                .py_6()
                .flex()
                .justify_center()
                .text_color(colors.muted_foreground)
                .child("No activity in this window.")
        };

        let table = match breakdown {
            Breakdown::Model => {
                let models = if metric == Metric::Tokens {
                    sort_models_by_tokens(&self.merged.models)
                } else {
                    self.merged.models.clone()
                };
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .type_scale(text::SM)
                    .font_features(tabular_nums())
                    .child(head_row(vec![
                        (0.4, false, "Model".into()),
                        (0.2, true, "Cost".into()),
                        (0.2, true, "Share".into()),
                        (0.2, true, "Tokens".into()),
                    ]))
                    .when(models.is_empty(), |this| this.child(empty()))
                    .children(models.into_iter().map(|model| {
                        let unknown = model.is_cost_unknown();
                        body_row(
                            format!("usage-model-{}-{}", model.provider.label(), model.model)
                                .into(),
                        )
                        .child(
                            cell(0.4, false).child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_color(colors.foreground)
                                    .child(logo(
                                        provider_logo(model.provider),
                                        colors.is_dark,
                                        px(14.),
                                    ))
                                    .child(div().truncate().child(model.model.clone())),
                            ),
                        )
                        .child(cell(0.2, true).child(if unknown {
                            div().text_color(colors.muted_foreground).child("Unpriced")
                        } else {
                            div()
                                .text_color(colors.foreground)
                                .child(format_usd(model.cost_usd))
                        }))
                        .child(cell(0.2, true).text_color(colors.muted_foreground).child(
                            if unknown {
                                "—".to_owned()
                            } else {
                                format_percent(model.cost_share)
                            },
                        ))
                        .child(
                            cell(0.2, true)
                                .text_color(colors.muted_foreground)
                                .child(format_tokens(model.total_tokens as f64)),
                        )
                    }))
            }
            Breakdown::Time => {
                let periods: Vec<_> = if hourly {
                    self.merged.hourly.iter().rev().collect()
                } else {
                    self.merged.daily.iter().rev().collect()
                };
                let value_width = 0.6 / (active.len() + 2) as f32;
                let mut head = vec![(
                    0.4,
                    false,
                    SharedString::from(if hourly { "Hour" } else { "Day" }),
                )];
                head.extend(
                    active
                        .iter()
                        .map(|provider| (value_width, true, SharedString::from(provider.label()))),
                );
                head.push((value_width, true, "Total".into()));
                head.push((value_width, true, "Tokens".into()));
                let zone = self.zone;
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .type_scale(text::SM)
                    .font_features(tabular_nums())
                    .child(head_row(head))
                    .when(periods.is_empty(), |this| this.child(empty()))
                    .children(periods.into_iter().map(|period| {
                        let label = if hourly {
                            zone.hour(&period.period)
                        } else {
                            format_day_short(&period.day)
                        };
                        body_row(format!("usage-period-{}", period.period).into())
                            .child(cell(0.4, false).text_color(colors.foreground).child(label))
                            .children(active.iter().map(|provider| {
                                let cost = period
                                    .by_provider
                                    .get(provider)
                                    .map_or(0., |value| value.cost_usd);
                                cell(value_width, true)
                                    .text_color(colors.muted_foreground)
                                    .child(format_usd(cost))
                            }))
                            .child(
                                cell(value_width, true)
                                    .text_color(colors.foreground)
                                    .child(format_usd(period.cost_usd)),
                            )
                            .child(
                                cell(value_width, true)
                                    .text_color(colors.muted_foreground)
                                    .child(format_tokens(period.total_tokens as f64)),
                            )
                    }))
            }
        };

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(heading("Breakdown", cx))
                    .child(toggle),
            )
            .child(table)
    }

    /// The loading stand-in (`UsageSkeleton`): the loaded page's shape in skeleton bars.
    pub(super) fn render_skeleton(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.colors();
        let live = self.app_state.read(cx).clock_is_live();
        let wide = window.viewport_size().width >= px(LG);
        let bar = |id: String, width: Option<f32>, height: f32| {
            let skeleton = Skeleton::new(SharedString::from(id))
                .h(px(height))
                .map(|this| match width {
                    Some(width) => this.w(px(width)),
                    None => this.w_full(),
                });
            if live { skeleton } else { skeleton.still() }
        };
        let pill = |id: String, size: f32| bar(id, Some(size), size).rounded_full();

        let summary = div()
            .flex()
            .flex_col()
            .gap_5()
            .when(wide, |this| this.w(px(288.)).flex_shrink_0())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(bar("usage-sk-headline".into(), Some(144.), 40.))
                    .child(bar("usage-sk-subline".into(), Some(128.), 16.)),
            )
            .children(ProviderKind::ORDER.into_iter().map(|provider| {
                let key = provider.label();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .min_h_5()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(pill(format!("usage-sk-dot-{key}"), 8.))
                                    .child(pill(format!("usage-sk-mark-{key}"), 16.))
                                    .child(bar(format!("usage-sk-name-{key}"), Some(80.), 14.)),
                            )
                            .child(bar(format!("usage-sk-value-{key}"), Some(56.), 14.)),
                    )
                    .child(bar(format!("usage-sk-share-{key}"), Some(144.), 16.))
            }));
        let chart = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(bar("usage-sk-chart-title".into(), Some(96.), 20.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .pl_16()
                            .child(bar("usage-sk-chart".into(), None, 224.)),
                    )
                    .child(div().pl_16().child(bar("usage-sk-axis".into(), None, 16.))),
            );
        let totals = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(heading("Totals", cx))
            .child(
                div()
                    .grid()
                    .grid_cols(5)
                    .gap_x_6()
                    .gap_y_4()
                    .py_1()
                    .children(
                        [
                            "Processed tokens",
                            "Cached input",
                            "Uncached input",
                            "Output",
                            "Cache savings",
                        ]
                        .into_iter()
                        .map(|label| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .type_scale(text::XS)
                                        .text_color(colors.muted_foreground)
                                        .child(label),
                                )
                                .child(bar(format!("usage-sk-total-{label}"), Some(64.), 24.))
                        }),
                    ),
            );
        let breakdown = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(heading("Breakdown", cx))
                    .child(bar("usage-sk-toggle".into(), Some(112.), 28.).rounded(radius::XL)),
            )
            .child(bar("usage-sk-table".into(), None, 176.));
        vec![
            div()
                .w_full()
                .flex()
                .gap_6()
                .map(|this| {
                    if wide {
                        this.flex_row()
                    } else {
                        this.flex_col()
                    }
                })
                .child(summary)
                .child(chart)
                .into_any_element(),
            totals.into_any_element(),
            breakdown.into_any_element(),
        ]
    }
}
