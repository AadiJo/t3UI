//! The Usage chart's math (`UsageProviderChart.tsx`): per-period provider columns, the 1/2/5
//! scale, and monotone cubic curves. Geometry is in the web's viewBox units (960 x 260,
//! stretched to the plot), so the view only maps x and y to pixels.

use std::collections::HashMap;

use super::{Metric, ProviderKind, merge::PeriodTotals};
use crate::time::parse_timestamp;

/// viewBox width.
pub const VIEW_WIDTH: f64 = 960.;
/// viewBox height.
pub const VIEW_HEIGHT: f64 = 260.;
/// Headroom above the top gridline so a 2px stroke at the peak is not clipped.
pub const PLOT_TOP: f64 = 8.;
/// Gridline count target.
pub const TICK_COUNT: u32 = 4;

/// One period's value per provider, in [`ProviderKind::ORDER`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Column {
    pub values: [f64; ProviderKind::ORDER.len()],
    pub total: f64,
}

/// Columns for `periods` (days, or hour starts as `toISOString` strings) from the merged
/// totals; a period without data is all zeros. Hours match by instant.
pub fn period_columns(periods: &[String], totals: &[PeriodTotals], metric: Metric) -> Vec<Column> {
    let key =
        |text: &str| parse_timestamp(text).map_or_else(|| text.to_owned(), |ms| ms.to_string());
    let by_period: HashMap<String, &PeriodTotals> = totals
        .iter()
        .map(|entry| {
            let entry_key = entry
                .start_millis
                .map_or_else(|| entry.period.clone(), |ms| ms.to_string());
            (entry_key, entry)
        })
        .collect();
    periods
        .iter()
        .map(|period| {
            let mut column = Column::default();
            if let Some(entry) = by_period.get(&key(period)) {
                for (kind, value) in &entry.by_provider {
                    column.values[kind.index()] = match metric {
                        Metric::Tokens => value.total_tokens as f64,
                        _ => value.cost_usd,
                    };
                }
            }
            column.total = column.values.iter().sum();
            column
        })
        .collect()
}

/// A readable axis: the max is the first 1/2/5 x 10^n step at or above `peak / count` times
/// enough steps to reach the peak; ticks run from 0 to max by that step.
#[derive(Clone, Debug, PartialEq)]
pub struct Scale {
    pub max: f64,
    pub ticks: Vec<f64>,
}

/// `niceScale`: rounding the max *up* keeps the tallest period inside the plot.
pub fn nice_scale(peak: f64, count: u32) -> Scale {
    if peak <= 0. || !peak.is_finite() {
        return Scale {
            max: 0.,
            ticks: vec![0.],
        };
    }
    let raw_step = peak / f64::from(count);
    let magnitude = 10_f64.powf(raw_step.log10().floor());
    let normalized = raw_step / magnitude;
    let step = if normalized > 5. {
        10.
    } else if normalized > 2. {
        5.
    } else if normalized > 1. {
        2.
    } else {
        1.
    } * magnitude;
    let max = (peak / step).ceil() * step;
    let mut ticks = Vec::new();
    let mut value = 0.;
    while value <= max + step * 1e-6 {
        ticks.push(value);
        value += step;
    }
    Scale { max, ticks }
}

/// One cubic segment: `from`, two control points, `to`, as `(x, y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveSegment {
    pub from: (f64, f64),
    pub c1: (f64, f64),
    pub c2: (f64, f64),
    pub to: (f64, f64),
}

/// Shape-preserving tangents (Fritsch-Carlson) that cannot overshoot spiky usage data.
fn monotone_tangents(points: &[(f64, f64)]) -> Vec<f64> {
    let count = points.len();
    if count < 2 {
        return vec![0.; count];
    }
    let slopes: Vec<f64> = points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].0 - pair[0].0;
            if dx == 0. {
                0.
            } else {
                (pair[1].1 - pair[0].1) / dx
            }
        })
        .collect();
    let mut tangents = vec![0.; count];
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        let (previous, next) = (slopes[index - 1], slopes[index]);
        tangents[index] = if previous * next <= 0. {
            0.
        } else {
            (previous + next) / 2.
        };
    }
    for index in 0..count - 1 {
        let slope = slopes[index];
        if slope == 0. {
            tangents[index] = 0.;
            tangents[index + 1] = 0.;
            continue;
        }
        let a = tangents[index] / slope;
        let b = tangents[index + 1] / slope;
        let magnitude = a * a + b * b;
        if magnitude > 9. {
            let scale = 3. / magnitude.sqrt();
            tangents[index] = scale * a * slope;
            tangents[index + 1] = scale * b * slope;
        }
    }
    tangents
}

/// `smoothCurve`: cubic segments through `points` (sorted by x).
pub fn monotone_curve(points: &[(f64, f64)]) -> Vec<CurveSegment> {
    if points.len() < 2 {
        return Vec::new();
    }
    let tangents = monotone_tangents(points);
    points
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let (from, to) = (pair[0], pair[1]);
            let dx = to.0 - from.0;
            CurveSegment {
                from,
                c1: (from.0 + dx / 3., from.1 + tangents[index] * dx / 3.),
                c2: (to.0 - dx / 3., to.1 - tangents[index + 1] * dx / 3.),
                to,
            }
        })
        .collect()
}

/// A provider's series in viewBox units, ready to stroke and fill.
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub provider: ProviderKind,
    pub total: f64,
    pub segments: Vec<CurveSegment>,
}

/// The whole chart for the active providers (`UsageProviderChart.tsx:196-248`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChartModel {
    pub columns: Vec<Column>,
    pub scale: Scale,
    /// x distance between periods in viewBox units (0 for a single period).
    pub step_x: f64,
    /// Heaviest total first, so lighter series paint on top.
    pub series: Vec<Series>,
}

impl ChartModel {
    pub fn new(columns: Vec<Column>, providers: &[ProviderKind]) -> Self {
        if columns.is_empty() {
            return Self {
                columns,
                scale: nice_scale(0., TICK_COUNT),
                step_x: 0.,
                series: Vec::new(),
            };
        }
        // Series layer from zero, so the scale tops out at the largest single value.
        let peak = columns
            .iter()
            .flat_map(|column| column.values)
            .fold(0., f64::max);
        let scale = nice_scale(peak, TICK_COUNT);
        let step_x = if columns.len() == 1 {
            0.
        } else {
            VIEW_WIDTH / (columns.len() - 1) as f64
        };
        let mut series: Vec<Series> = providers
            .iter()
            .map(|&provider| {
                let points: Vec<(f64, f64)> = columns
                    .iter()
                    .enumerate()
                    .map(|(index, column)| {
                        (
                            index as f64 * step_x,
                            y_for(column.values[provider.index()], scale.max),
                        )
                    })
                    .collect();
                Series {
                    provider,
                    total: columns
                        .iter()
                        .map(|column| column.values[provider.index()])
                        .sum(),
                    segments: monotone_curve(&points),
                }
            })
            .collect();
        series.sort_by(|a, b| b.total.total_cmp(&a.total));
        Self {
            columns,
            scale,
            step_x,
            series,
        }
    }

    /// viewBox y of `value`.
    pub fn y(&self, value: f64) -> f64 {
        y_for(value, self.scale.max)
    }

    /// The period nearest `fraction` (0..1 across the plot), as the hover readout picks it.
    pub fn hover_index(&self, fraction: f64) -> Option<usize> {
        let count = self.columns.len();
        if count == 0 {
            return None;
        }
        let index = (fraction.clamp(0., 1.) * (count - 1) as f64).round() as usize;
        Some(index.min(count - 1))
    }
}

fn y_for(value: f64, max: f64) -> f64 {
    if max == 0. {
        VIEW_HEIGHT
    } else {
        VIEW_HEIGHT - value / max * (VIEW_HEIGHT - PLOT_TOP)
    }
}
