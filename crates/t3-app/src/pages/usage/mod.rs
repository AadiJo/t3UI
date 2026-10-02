//! `/usage` (`components/usage/UsagePage.tsx`, spec `docs/spec/pages.md` section 3): token and
//! cost usage across every connected environment, or subscription limits.
//!
//! ```text
//! UsageView
//! ├── header      breadcrumb "Usage / <environments>", metric + period toggles, refresh
//! └── scroll      WorkspacePageContainer (wide)
//!     ├── limits  pooled windows per provider            (Metric::Limits)
//!     ├── skeleton                                       (no environment answered yet)
//!     └── body    summary + chart, totals, breakdown     (Cost / Tokens)
//! ```
//!
//! Every environment answers `server.getUsageSummary` for the current window; the view merges
//! the answers with `t3_logic::usage::merge`. Limits come from each environment's server config.

mod body;
mod chart;
mod header;
mod limits;

use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Local, Utc};
use gpui_kit::{
    App, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point,
    Render, SharedString, Styled as _, Subscription, Task, Window, div, px,
};
use t3_logic::usage::{
    Metric, Period, ProviderKind,
    format::{format_hour_short, format_relative_hour_short, format_window_label},
    limits::{LimitProvider, LimitsEnvironment},
    merge::{EnvironmentUsage, MergedUsage, merge_usage},
    window::make_window,
};
use t3_protocol::{
    EnvironmentId,
    methods::{Empty, ServerGetUsageSummary, ServerRefreshProviders, ServerRefreshUsageRates},
    server::RefreshProvidersInput,
    usage::{UsageSummary, UsageSummaryInput},
};
use t3_ui::{ActiveColors as _, ScrollArea, tokens::layout};

use super::chrome::{PageWidth, page_container, topbar_scroll_fade};
use crate::state::{AppEvent, AppState, ConnectionStatus, Environment};

/// Automatic limit probes per environment wait this long after the last one
/// (`refreshUsageLimits`, `client-runtime/src/state/usage.ts:53-84`).
const LIMITS_REFRESH_INTERVAL_MS: i64 = 5 * 60_000;

/// Which zone dates are shown and bucketed in: the viewer's, or UTC in snapshot fixtures (the
/// reference captures run in UTC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Local,
    Utc,
}

impl Zone {
    fn name(self) -> String {
        match self {
            Self::Local => iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into()),
            Self::Utc => "UTC".into(),
        }
    }

    fn window(self, period: Period, now: DateTime<Utc>) -> UsageSummaryInput {
        match self {
            Self::Local => make_window(period, now, &Local, &self.name()),
            Self::Utc => make_window(period, now, &Utc, &self.name()),
        }
    }

    fn hour(self, hour_start: &str) -> String {
        match self {
            Self::Local => format_hour_short(hour_start, &Local),
            Self::Utc => format_hour_short(hour_start, &Utc),
        }
    }

    fn relative_hour(self, hour_start: &str, reference: &str) -> String {
        match self {
            Self::Local => format_relative_hour_short(hour_start, reference, &Local),
            Self::Utc => format_relative_hour_short(hour_start, reference, &Utc),
        }
    }

    fn window_label(self, window: &UsageSummaryInput) -> String {
        match self {
            Self::Local => format_window_label(window, &Local),
            Self::Utc => format_window_label(window, &Utc),
        }
    }
}

/// The breakdown table's grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Breakdown {
    Model,
    Time,
}

/// One environment's answer for the current window (`EnvironmentUsageStatus`).
struct EnvironmentUsageStatus {
    id: EnvironmentId,
    label: SharedString,
    environment: Entity<Environment>,
    summary: Option<Arc<UsageSummary>>,
    /// A request is running (first scan or refresh).
    pending: bool,
    error: Option<SharedString>,
    request: Option<Task<()>>,
}

impl EnvironmentUsageStatus {
    /// The environment-menu status text (`UsagePage.tsx:1055-1068`).
    fn status_label(&self) -> &'static str {
        if self.error.is_some() {
            "Unavailable"
        } else if self.summary.as_ref().is_some_and(|summary| {
            !t3_logic::usage::merge::is_compatible_version(summary.contract_version)
        }) {
            "Update required"
        } else if self.summary.is_none() {
            "Scanning…"
        } else if self.pending {
            "Refreshing…"
        } else {
            "Ready"
        }
    }
}

/// The chart's pointer state: period index and the pointer within the plot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ChartHover {
    index: usize,
    position: Point<Pixels>,
}

/// The Usage page.
pub struct UsageView {
    app_state: Entity<AppState>,
    zone: Zone,
    window: UsageSummaryInput,
    breakdown: Breakdown,
    /// `None` = every environment.
    selected: Option<BTreeSet<EnvironmentId>>,
    statuses: Vec<EnvironmentUsageStatus>,
    merged: Arc<MergedUsage>,
    refreshing: Option<Task<()>>,
    /// The clock limits are computed against; advanced on explicit refresh, never ticking.
    limits_now: i64,
    limits_refreshed_at: Vec<(EnvironmentId, i64)>,
    chart_hover: Option<ChartHover>,
    _subscriptions: Vec<Subscription>,
}

impl UsageView {
    pub fn new(app_state: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = app_state.read(cx);
        // Fixtures pin the clock; their dates read in UTC like the reference captures.
        let zone = if state.clock_is_live() {
            Zone::Local
        } else {
            Zone::Utc
        };
        let now = state.now_millis();
        let period = state.ui().usage.period;
        let subscriptions = vec![
            cx.observe(&app_state, |this, _, cx| {
                this.sync_environments(cx);
                cx.notify();
            }),
            cx.subscribe(&app_state, |this, _, event, cx| {
                if let AppEvent::Command(command) = event {
                    this.on_command(&command.as_str(), cx);
                }
            }),
        ];
        let mut this = Self {
            window: zone.window(period, utc(now)),
            app_state,
            zone,
            breakdown: Breakdown::Model,
            selected: None,
            statuses: Vec::new(),
            merged: Arc::new(MergedUsage::default()),
            refreshing: None,
            limits_now: now,
            limits_refreshed_at: Vec::new(),
            chart_hover: None,
            _subscriptions: subscriptions,
        };
        this.sync_environments(cx);
        if this.metric(cx) == Metric::Limits {
            this.refresh_limits(true, cx);
        }
        this
    }

    fn metric(&self, cx: &App) -> Metric {
        self.app_state.read(cx).ui().usage.metric
    }

    fn period(&self, cx: &App) -> Period {
        self.app_state.read(cx).ui().usage.period
    }

    fn now(&self, cx: &App) -> i64 {
        self.app_state.read(cx).now_millis()
    }

    fn is_selected(&self, id: &EnvironmentId) -> bool {
        self.selected
            .as_ref()
            .is_none_or(|selected| selected.contains(id))
    }

    fn selected_statuses(&self) -> impl Iterator<Item = &EnvironmentUsageStatus> {
        self.statuses
            .iter()
            .filter(|status| self.is_selected(&status.id))
    }

    // -----------------------------------------------------------------------------------------
    // Data

    /// Follows the app's environments: adds new ones, drops removed ones, and asks every
    /// connected environment without an answer for the current window.
    fn sync_environments(&mut self, cx: &mut Context<Self>) {
        let environments: Vec<Entity<Environment>> =
            self.app_state.read(cx).environments().to_vec();
        let mut statuses = Vec::with_capacity(environments.len());
        for environment in environments {
            let (id, label) = {
                let environment = environment.read(cx);
                (environment.id().clone(), environment.label().clone())
            };
            let status = match self.statuses.iter().position(|status| status.id == id) {
                Some(index) => {
                    let mut status = self.statuses.remove(index);
                    status.label = label;
                    status
                }
                None => EnvironmentUsageStatus {
                    id,
                    label,
                    environment,
                    summary: None,
                    pending: false,
                    error: None,
                    request: None,
                },
            };
            statuses.push(status);
        }
        self.statuses = statuses;
        for index in 0..self.statuses.len() {
            let status = &self.statuses[index];
            if status.summary.is_none() && status.request.is_none() && status.error.is_none() {
                self.request_summary(index, false, cx);
            }
        }
        self.remerge();
    }

    /// Asks one environment for the current window. `refresh` refetches the rate table first
    /// (`refreshUsage`). A fixture environment answers at once; a disconnected one stays
    /// pending until it connects.
    fn request_summary(&mut self, index: usize, refresh: bool, cx: &mut Context<Self>) {
        let status = &mut self.statuses[index];
        let environment = status.environment.read(cx);
        if environment.client().is_none() {
            if let Some(summary) = environment.usage_fixture() {
                status.summary = Some(summary.clone());
            }
            return;
        }
        if !matches!(environment.status(), ConnectionStatus::Connected { .. }) {
            return;
        }
        let id = status.id.clone();
        let entity = status.environment.clone();
        let input = self.window.clone();
        let rates = refresh.then(|| environment.request::<ServerRefreshUsageRates>(Empty {}, cx));
        status.pending = true;
        status.request = Some(cx.spawn(async move |this, cx| {
            if let Some(rates) = rates {
                rates.await.ok();
            }
            let Ok(summary) = this.update(cx, |_, cx| {
                entity
                    .read(cx)
                    .request::<ServerGetUsageSummary>(input.clone(), cx)
            }) else {
                return;
            };
            let result = summary.await;
            this.update(cx, |this, cx| {
                // A window change while this ran started a newer request; drop this answer.
                if !same_window(&this.window, &input) {
                    return;
                }
                let Some(status) = this.statuses.iter_mut().find(|status| status.id == id) else {
                    return;
                };
                status.pending = false;
                status.request = None;
                match result {
                    Ok(summary) => {
                        status.summary = Some(Arc::new(summary));
                        status.error = None;
                    }
                    Err(error) => {
                        tracing::warn!("usage summary failed: {error:#}");
                        status.error = Some("This environment could not report usage.".into());
                    }
                }
                this.remerge();
                cx.notify();
            })
            .ok();
        }));
    }

    /// Re-merges the selected environments' answers.
    fn remerge(&mut self) {
        let answered: Vec<EnvironmentUsage> = self
            .selected_statuses()
            .filter_map(|status| {
                status.summary.as_ref().map(|summary| EnvironmentUsage {
                    environment_id: status.id.to_string(),
                    label: status.label.to_string(),
                    summary: (**summary).clone(),
                })
            })
            .collect();
        self.merged = Arc::new(merge_usage(&answered));
    }

    /// No selected environment has answered and one is still reporting.
    fn is_pending(&self) -> bool {
        let answered = self
            .selected_statuses()
            .any(|status| status.summary.is_some());
        !answered && self.still_reporting() > 0
    }

    /// Some answered while others still report (failed ones do not count).
    fn is_partial(&self) -> bool {
        self.selected_statuses()
            .any(|status| status.summary.is_some())
            && self.still_reporting() > 0
    }

    fn still_reporting(&self) -> usize {
        self.selected_statuses()
            .filter(|status| status.summary.is_none() && status.error.is_none())
            .count()
    }

    /// Selected environments still scanning (the header's dashed circle).
    fn scanning_count(&self) -> usize {
        self.selected_statuses()
            .filter(|status| status.error.is_none() && (status.pending || status.summary.is_none()))
            .count()
    }

    /// Starts over for a new window: every environment scans again.
    fn set_window(&mut self, window: UsageSummaryInput, cx: &mut Context<Self>) {
        if same_window(&self.window, &window) {
            return;
        }
        self.window = window;
        self.chart_hover = None;
        for status in &mut self.statuses {
            status.summary = None;
            status.error = None;
            status.pending = false;
            status.request = None;
        }
        self.sync_environments(cx);
    }

    // -----------------------------------------------------------------------------------------
    // Actions

    fn select_metric(&mut self, metric: Metric, cx: &mut Context<Self>) {
        if metric == Metric::Limits {
            self.limits_now = self.now(cx);
        }
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| {
                    let changed = ui.usage.metric != metric;
                    ui.usage.metric = metric;
                    changed
                },
                cx,
            );
        });
        if metric == Metric::Limits {
            self.refresh_limits(true, cx);
        }
        cx.notify();
    }

    fn select_period(&mut self, period: Period, cx: &mut Context<Self>) {
        self.app_state.update(cx, |state, cx| {
            state.update_ui(
                |ui| {
                    let changed = ui.usage.period != period;
                    ui.usage.period = period;
                    changed
                },
                cx,
            );
        });
        let window = self.zone.window(period, utc(self.now(cx)));
        self.set_window(window, cx);
        cx.notify();
    }

    fn set_breakdown(&mut self, breakdown: Breakdown, cx: &mut Context<Self>) {
        self.breakdown = breakdown;
        cx.notify();
    }

    fn set_selection(&mut self, selected: Option<BTreeSet<EnvironmentId>>, cx: &mut Context<Self>) {
        self.selected = selected;
        self.remerge();
        if self.metric(cx) == Metric::Limits {
            self.refresh_limits(true, cx);
        }
        cx.notify();
    }

    /// The header's refresh button (`refreshWindow`, `UsagePage.tsx:259-286`).
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.refreshing.is_some() {
            return;
        }
        if self.metric(cx) == Metric::Limits {
            self.refresh_limits(false, cx);
            return;
        }
        let window = self.zone.window(self.period(cx), utc(self.now(cx)));
        if !same_window(&window, &self.window) {
            self.set_window(window, cx);
            return;
        }
        for index in 0..self.statuses.len() {
            if self.is_selected(&self.statuses[index].id) {
                self.request_summary(index, true, cx);
            }
        }
        cx.notify();
    }

    /// Re-probes providers on every selected connected environment (`refreshLimits`). Automatic
    /// probes skip environments probed in the last five minutes.
    fn refresh_limits(&mut self, automatic: bool, cx: &mut Context<Self>) {
        let now = self.now(cx);
        let mut tasks = Vec::new();
        for status in &self.statuses {
            if !self.is_selected(&status.id) {
                continue;
            }
            let environment = status.environment.read(cx);
            if environment.client().is_none() || environment.config().is_none() {
                continue;
            }
            let last = self
                .limits_refreshed_at
                .iter()
                .find(|(id, _)| *id == status.id)
                .map(|(_, at)| *at);
            if automatic && last.is_some_and(|last| now - last < LIMITS_REFRESH_INTERVAL_MS) {
                continue;
            }
            tasks.push(
                environment.request::<ServerRefreshProviders>(RefreshProvidersInput::default(), cx),
            );
            self.limits_refreshed_at.retain(|(id, _)| *id != status.id);
            self.limits_refreshed_at.push((status.id.clone(), now));
        }
        if tasks.is_empty() {
            self.limits_now = now;
            return;
        }
        self.refreshing = Some(cx.spawn(async move |this, cx| {
            for task in tasks {
                task.await.ok();
            }
            this.update(cx, |this, cx| {
                this.refreshing = None;
                this.limits_now = this.now(cx);
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// Page shortcuts (`usage.*`, resolved by the workspace while this page shows).
    fn on_command(&mut self, command: &str, cx: &mut Context<Self>) {
        if let Some(metric) = Metric::ALL
            .into_iter()
            .find(|metric| metric.command() == command)
        {
            self.select_metric(metric, cx);
        } else if let Some(period) = Period::ALL
            .into_iter()
            .find(|period| period.command() == command)
            && self.metric(cx) != Metric::Limits
        {
            self.select_period(period, cx);
        }
    }

    /// Whether a refresh is running: the limits probe, or any selected summary request.
    fn is_refreshing(&self, cx: &App) -> bool {
        if self.metric(cx) == Metric::Limits {
            return self.refreshing.is_some();
        }
        self.selected_statuses()
            .any(|status| status.pending && status.summary.is_some())
    }

    /// Limit snapshots of the selected environments.
    fn limit_environments(&self, cx: &App) -> Vec<LimitsEnvironment> {
        self.selected_statuses()
            .filter_map(|status| {
                let environment = status.environment.read(cx);
                let config = environment.config()?;
                Some(LimitsEnvironment {
                    environment_id: status.id.to_string(),
                    label: status.label.to_string(),
                    providers: config
                        .providers
                        .iter()
                        .filter_map(LimitProvider::from_server)
                        .collect(),
                    sources: config.usage_limit_sources.clone().unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Width the content column gets: the main column (window minus the open sidebar) capped
    /// at the container, minus its 24px padding on each side.
    fn content_width(&self, window: &Window, cx: &App) -> Pixels {
        let state = self.app_state.read(cx);
        let sidebar = if state.sidebar_open() {
            state
                .ui()
                .sidebar_width
                .map_or(layout::SIDEBAR_WIDTH, |width| {
                    px(width).max(layout::SIDEBAR_MIN_WIDTH)
                })
        } else {
            px(0.)
        };
        let main = window.viewport_size().width - sidebar;
        main.min(PageWidth::Wide.max_width()) - px(48.)
    }
}

/// Two requests for the same window (the protocol type has no `PartialEq`).
fn same_window(a: &UsageSummaryInput, b: &UsageSummaryInput) -> bool {
    a.since_day == b.since_day
        && a.until_day == b.until_day
        && a.time_zone == b.time_zone
        && a.resolution == b.resolution
        && a.since_time == b.since_time
        && a.until_time == b.until_time
}

fn utc(millis: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or_default()
}

/// Provider presentation shared by the summary, chart and tables.
fn provider_logo(provider: ProviderKind) -> t3_ui::Logo {
    match provider {
        ProviderKind::Codex => t3_ui::Logo::OpenAI,
        ProviderKind::Claude => t3_ui::Logo::ClaudeAI,
        ProviderKind::Grok => t3_ui::Logo::GrokIcon,
        ProviderKind::Cursor => t3_ui::Logo::CursorIcon,
        ProviderKind::Opencode => t3_ui::Logo::OpenCodeIcon,
        ProviderKind::Antigravity => t3_ui::Logo::AntigravityIcon,
    }
}

/// Series color (`PROVIDER_PRESENTATION[provider].color`, `usageProviders.ts:24-44`).
fn provider_color(provider: ProviderKind, colors: &t3_ui::Colors) -> gpui_kit::Hsla {
    use t3_ui::tokens::hex;
    match provider {
        ProviderKind::Codex => colors.contrast_foreground,
        ProviderKind::Claude => hex(0xD977_57FF),
        // color-mix(in oklab, contrast-foreground 72%, background).
        ProviderKind::Grok => mix(colors.contrast_foreground, colors.background, 0.72),
        ProviderKind::Cursor => hex(0x8B8B_8BFF),
        ProviderKind::Opencode => hex(0x5B9B_BDFF),
        ProviderKind::Antigravity => hex(0x8C7B_D1FF),
    }
}

/// `color-mix(in srgb, a share, b)`; close enough to oklab for the neutral Grok series.
fn mix(a: gpui_kit::Hsla, b: gpui_kit::Hsla, share: f32) -> gpui_kit::Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    gpui_kit::Rgba {
        r: a.r * share + b.r * (1. - share),
        g: a.g * share + b.g * (1. - share),
        b: a.b * share + b.b * (1. - share),
        a: 1.,
    }
    .into()
}

impl Render for UsageView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let metric = self.metric(cx);
        let content: Vec<gpui_kit::AnyElement> = if self.selected_statuses().next().is_none() {
            let noun = if metric == Metric::Limits {
                "limits"
            } else {
                "usage"
            };
            let text = if self.statuses.is_empty() {
                format!("Connect an environment to see {noun}.")
            } else {
                format!("Select an environment to see {noun}.")
            };
            vec![body::muted_paragraph(text, cx).into_any_element()]
        } else if metric == Metric::Limits {
            vec![self.render_limits(window, cx).into_any_element()]
        } else if self.is_pending() {
            self.render_skeleton(window, cx)
        } else {
            self.render_body(metric, window, cx)
        };
        div()
            .id("usage-page")
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_color(cx.colors().foreground)
            .child(self.render_header(metric, window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        ScrollArea::new("usage-scroll")
                            .child(page_container(PageWidth::Wide).children(content)),
                    )
                    .child(topbar_scroll_fade(cx)),
            )
    }
}
