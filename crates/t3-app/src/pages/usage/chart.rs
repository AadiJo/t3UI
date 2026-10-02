//! The provider chart (`UsageProviderChart.tsx`): gridlines and tick labels, one smoothed area
//! and line per active provider, first/middle/last period labels, and a hover readout.
//!
//! The web draws an SVG with a 960x260 viewBox stretched to the plot with non-scaling strokes;
//! this paints the same geometry with GPUI paths, mapping viewBox x and y to the plot bounds.

use std::{cell::Cell, rc::Rc};

use gpui_kit::{
    Bounds, Context, InteractiveElement as _, IntoElement, MouseMoveEvent, ParentElement as _,
    PathBuilder, Pixels, Point, StatefulInteractiveElement as _, Styled as _, canvas, div, point,
    px,
};
use t3_logic::usage::{
    Metric, ProviderKind,
    chart::{ChartModel, PLOT_TOP, VIEW_HEIGHT, VIEW_WIDTH, period_columns},
    format::{format_day_short, format_tokens, format_usd},
    window::periods,
};
use t3_ui::{
    ActiveColors as _, logo,
    tokens::{radius, shadow, text},
};

use super::{ChartHover, UsageView, provider_color, provider_logo};
use crate::{chrome::TypeScale as _, pages::controls::tabular_nums};

/// Plot height (`h-56`).
const PLOT_HEIGHT: Pixels = px(224.);
/// Axis label column (`w-14`); the plot follows after an 8px gap.
const AXIS_WIDTH: Pixels = px(56.);
/// Tooltip offset from the pointer.
const TOOLTIP_GAP: f32 = 12.;
/// Tooltip width used to keep it inside the plot (`min-w-36`, a little wider for content).
const TOOLTIP_WIDTH: f32 = 168.;

fn format_value(metric: Metric, value: f64) -> String {
    match metric {
        Metric::Tokens => format_tokens(value),
        _ => format_usd(value),
    }
}

impl UsageView {
    pub(super) fn render_chart(
        &self,
        metric: Metric,
        providers: &[ProviderKind],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let hourly = self.window.since_time.is_some();
        let periods = periods(&self.window);
        let totals = if hourly {
            &self.merged.hourly
        } else {
            &self.merged.daily
        };
        let model = ChartModel::new(period_columns(&periods, totals, metric), providers);
        let zone = self.zone;
        let format_period = |period: &str| {
            if hourly {
                zone.hour(period)
            } else {
                format_day_short(period)
            }
        };

        // Tick labels, vertically centered on their gridlines.
        let ticks = div()
            .relative()
            .w(AXIS_WIDTH)
            .h(PLOT_HEIGHT)
            .flex_shrink_0()
            .children(model.scale.ticks.iter().map(|&tick| {
                let top = model.y(tick) / VIEW_HEIGHT;
                let label = if tick == 0. {
                    "0".to_owned()
                } else {
                    format_value(metric, tick)
                };
                div()
                    .absolute()
                    .right_0()
                    .top(PLOT_HEIGHT * top as f32 - px(7.))
                    .type_scale(text::XS3)
                    .font_features(tabular_nums())
                    .text_color(colors.muted_foreground)
                    .child(label)
            }));

        let bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
        let hover = self.chart_hover;
        let paint_model = model.clone();
        let paint_bounds = bounds_cell.clone();
        let series_colors: Vec<_> = paint_model
            .series
            .iter()
            .map(|series| provider_color(series.provider, colors))
            .collect();
        let hover_index = hover.map(|hover| hover.index);
        let plot_canvas = canvas(
            move |bounds, _, _| {
                paint_bounds.set(Some(bounds));
                bounds
            },
            move |bounds, _, window, _| {
                let map = |x: f64, y: f64| -> Point<Pixels> {
                    point(
                        bounds.origin.x + bounds.size.width * (x / VIEW_WIDTH) as f32,
                        bounds.origin.y + bounds.size.height * (y / VIEW_HEIGHT) as f32,
                    )
                };
                // Gridlines.
                for &tick in &paint_model.scale.ticks {
                    let y = map(0., paint_model.y(tick)).y.round();
                    window.paint_quad(gpui_kit::fill(
                        Bounds::new(
                            point(bounds.origin.x, y),
                            gpui_kit::size(bounds.size.width, px(1.)),
                        ),
                        colors.border,
                    ));
                }
                // Fills first, then every stroke, so no series covers another's line.
                for (series, color) in paint_model.series.iter().zip(&series_colors) {
                    let Some(first) = series.segments.first() else {
                        continue;
                    };
                    let mut path = PathBuilder::fill();
                    path.move_to(map(first.from.0, first.from.1));
                    for segment in &series.segments {
                        path.cubic_bezier_to(
                            map(segment.to.0, segment.to.1),
                            map(segment.c1.0, segment.c1.1),
                            map(segment.c2.0, segment.c2.1),
                        );
                    }
                    path.line_to(map(VIEW_WIDTH, VIEW_HEIGHT));
                    path.line_to(map(0., VIEW_HEIGHT));
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, color.opacity(0.12));
                    }
                }
                for (series, color) in paint_model.series.iter().zip(&series_colors) {
                    let Some(first) = series.segments.first() else {
                        continue;
                    };
                    let mut path = PathBuilder::stroke(px(2.));
                    path.move_to(map(first.from.0, first.from.1));
                    for segment in &series.segments {
                        path.cubic_bezier_to(
                            map(segment.to.0, segment.to.1),
                            map(segment.c1.0, segment.c1.1),
                            map(segment.c2.0, segment.c2.1),
                        );
                    }
                    if let Ok(path) = path.build() {
                        window.paint_path(path, *color);
                    }
                }
                if let Some(index) = hover_index {
                    let x = map(index as f64 * paint_model.step_x, 0.).x.round();
                    let top = map(0., PLOT_TOP).y;
                    window.paint_quad(gpui_kit::fill(
                        Bounds::new(
                            point(x, top),
                            gpui_kit::size(px(1.), bounds.origin.y + bounds.size.height - top),
                        ),
                        colors.muted_foreground,
                    ));
                }
            },
        )
        .size_full();

        let tooltip = hover.and_then(|hover| {
            let column = model.columns.get(hover.index)?;
            let period = periods.get(hover.index)?;
            let label = match (&self.window.until_time, hourly) {
                (Some(reference), true) => zone.relative_hour(period, reference),
                _ => format_period(period),
            };
            let bounds = bounds_cell.get();
            let plot_width = bounds.map_or(px(600.), |bounds| bounds.size.width);
            let x = f32::from(hover.position.x);
            let left = if x + TOOLTIP_GAP + TOOLTIP_WIDTH <= f32::from(plot_width) {
                x + TOOLTIP_GAP
            } else {
                x - TOOLTIP_GAP - TOOLTIP_WIDTH
            };
            let rows = 2. + providers.len() as f32;
            let height = 16. + rows * 16. + 8.;
            let y = f32::from(hover.position.y);
            let top = if y + TOOLTIP_GAP + height <= f32::from(PLOT_HEIGHT) {
                y + TOOLTIP_GAP
            } else {
                y - TOOLTIP_GAP - height
            };
            let left = left.clamp(0., (f32::from(plot_width) - TOOLTIP_WIDTH).max(0.));
            let top = top.clamp(0., (f32::from(PLOT_HEIGHT) - height).max(0.));
            Some(
                div()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .min_w(px(144.))
                    .px(px(10.))
                    .py_2()
                    .rounded(radius::XL)
                    .border_1()
                    .border_color(colors.border_50)
                    .bg(colors.popover_95)
                    .shadow(shadow::LG.to_vec())
                    .type_scale(text::XS)
                    .font_features(tabular_nums())
                    .child(
                        div()
                            .mb_1()
                            .text_color(colors.muted_foreground)
                            .child(label),
                    )
                    .children(providers.iter().map(|&provider| {
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_color(colors.muted_foreground)
                                    .child(logo(provider_logo(provider), colors.is_dark, px(12.)))
                                    .child(provider.label()),
                            )
                            .child(
                                div()
                                    .text_color(colors.foreground)
                                    .child(format_value(metric, column.values[provider.index()])),
                            )
                    }))
                    .child(
                        div()
                            .mt_1()
                            .pt_1()
                            .border_t_1()
                            .border_color(colors.border)
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(div().text_color(colors.muted_foreground).child("Total"))
                            .child(
                                div()
                                    .text_color(colors.foreground)
                                    .child(format_value(metric, column.total)),
                            ),
                    ),
            )
        });

        let move_bounds = bounds_cell.clone();
        let count = model.columns.len();
        let plot = div()
            .id("usage-chart-plot")
            .relative()
            .flex_1()
            .min_w_0()
            .h(PLOT_HEIGHT)
            .child(plot_canvas)
            .children(tooltip)
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                let Some(bounds) = move_bounds.get() else {
                    return;
                };
                if count == 0 || bounds.size.width <= px(0.) {
                    return;
                }
                if !bounds.contains(&event.position) {
                    if this.chart_hover.take().is_some() {
                        cx.notify();
                    }
                    return;
                }
                let local = event.position - bounds.origin;
                let fraction = f64::from(f32::from(local.x) / f32::from(bounds.size.width));
                let index = ((fraction * (count - 1) as f64).round() as usize).min(count - 1);
                let next = Some(ChartHover {
                    index,
                    position: local,
                });
                if this.chart_hover != next {
                    this.chart_hover = next;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered && this.chart_hover.take().is_some() {
                    cx.notify();
                }
            }));

        let first = periods.first().map(|period| format_period(period));
        let middle = periods
            .get(periods.len() / 2)
            .map(|period| format_period(period));
        let last = periods.last().map(|period| format_period(period));
        let axis = div()
            .pl(AXIS_WIDTH + px(8.))
            .flex()
            .justify_between()
            .type_scale(text::XS3)
            .text_color(colors.muted_foreground)
            .children(
                [first, middle, last]
                    .into_iter()
                    .map(|label| div().child(label.unwrap_or_default().to_uppercase())),
            );

        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().flex().gap_2().child(ticks).child(plot))
            .child(axis)
    }
}
