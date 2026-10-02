//! The Usage header (`UsagePage.tsx:304-442`): breadcrumb with the environment filter, the
//! window label (2xl), metric and period toggles (xl) or compact selects, and refresh.

use std::collections::BTreeSet;

use gpui_kit::{
    AnyElement, ClickEvent, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use t3_logic::usage::{Metric, Period};
use t3_ui::{
    ActiveColors as _, Align, Button, ButtonSize, ButtonVariant, DropdownMenu, Icon, IconName,
    MenuCheckboxItem, MenuItem, MenuSeparator, Select, SelectSize, SelectVariant, tokens::text,
};

use super::UsageView;
use crate::{
    chrome::TypeScale as _,
    keybindings::shortcut_label,
    pages::{
        chrome::{breadcrumb, breadcrumb_item, breadcrumb_separator, page_header},
        controls::{Segment, inline_button, segmented},
    },
};

/// Tailwind `xl`: the toggles sit inline from here up.
const XL: f32 = 1280.;
/// Tailwind `2xl`: the window label shows from here up.
const XL2: f32 = 1536.;

/// `"<label> (<shortcut>)"` (`shortcutTitle`, `UsagePage.tsx:108-115`).
fn shortcut_title(label: &str, command: &str, cx: &gpui_kit::App) -> SharedString {
    let command = t3_logic::keybindings::Command::parse(command);
    match shortcut_label(&command, cx) {
        Some(shortcut) => format!("{label} ({shortcut})").into(),
        None => label.to_owned().into(),
    }
}

impl UsageView {
    pub(super) fn render_header(
        &mut self,
        metric: Metric,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let viewport = window.viewport_size().width;
        let wide = viewport >= px(XL);
        let colors = cx.colors();
        let period = self.period(cx);
        let showing_limits = metric == Metric::Limits;

        let crumbs = breadcrumb()
            .when(!wide, |this| this.w_full())
            .child(breadcrumb_item(true, cx).child("Usage"))
            .child(breadcrumb_separator(cx))
            .child(
                breadcrumb_item(true, cx)
                    .min_w(px(40.))
                    .child(self.environment_filter(showing_limits, cx)),
            );
        let window_label = (!showing_limits && viewport >= px(XL2)).then(|| {
            div()
                .min_w_0()
                .truncate()
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(self.zone.window_label(&self.window))
        });
        let refreshing = self.is_refreshing(cx);
        let refresh = Button::new("usage-refresh")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .icon(IconName::RefreshCw)
            .disabled(refreshing)
            .tooltip(if showing_limits {
                "Refresh limits"
            } else {
                "Refresh usage"
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh(cx)));

        let controls = if wide {
            div()
                .ml_auto()
                .min_w_0()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(segmented(
                    "usage-metric",
                    Metric::ALL
                        .into_iter()
                        .map(|option| Segment {
                            id: option.command().into(),
                            label: option.label().into(),
                            pressed: option == metric,
                            tooltip: Some(shortcut_title(option.label(), option.command(), cx)),
                            on_click: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.select_metric(option, cx)
                            })),
                        })
                        .collect(),
                    false,
                    cx,
                ))
                .child(segmented(
                    "usage-period",
                    Period::ALL
                        .into_iter()
                        .map(|option| Segment {
                            id: option.command().into(),
                            label: option.label().into(),
                            pressed: option == period,
                            tooltip: Some(shortcut_title(option.label(), option.command(), cx)),
                            on_click: Box::new(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.select_period(option, cx)
                            })),
                        })
                        .collect(),
                    // The period does not apply to Limits; it stays put but disabled.
                    showing_limits,
                    cx,
                ))
                .child(refresh)
                .into_any_element()
        } else {
            div()
                .w_full()
                .ml_auto()
                .min_w_0()
                .flex()
                .items_center()
                .justify_end()
                .gap_1()
                .child(
                    Select::new("usage-metric-select")
                        .variant(SelectVariant::Ghost)
                        .size(SelectSize::Sm)
                        .items(
                            Metric::ALL
                                .into_iter()
                                .map(|option| (option.command(), option.label())),
                        )
                        .value(Some(metric.command().into()))
                        .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                            if let Some(metric) = Metric::ALL
                                .into_iter()
                                .find(|m| m.command() == value.as_ref())
                            {
                                this.select_metric(metric, cx);
                            }
                        })),
                )
                .child(
                    Select::new("usage-period-select")
                        .variant(SelectVariant::Ghost)
                        .size(SelectSize::Sm)
                        .disabled(showing_limits)
                        .items(
                            Period::ALL
                                .into_iter()
                                .map(|option| (option.command(), option.label())),
                        )
                        .value(Some(period.command().into()))
                        .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                            if let Some(period) = Period::ALL
                                .into_iter()
                                .find(|p| p.command() == value.as_ref())
                            {
                                this.select_period(period, cx);
                            }
                        })),
                )
                .child(refresh)
                .into_any_element()
        };

        page_header("usage-header", window, cx)
            .h_auto()
            .py_2()
            .map(|this| {
                if wide {
                    this.child(crumbs).children(window_label).child(controls)
                } else {
                    // Two rows: the breadcrumb, then the compact controls on the right.
                    this.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(crumbs)
                            .child(controls),
                    )
                }
            })
    }

    /// The breadcrumb's environment menu (`UsageEnvironmentFilter`, `UsagePage.tsx:977-1128`).
    fn environment_filter(&self, showing_limits: bool, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let selected: Vec<_> = self.selected_statuses().collect();
        let label: SharedString = match (&self.selected, selected.as_slice()) {
            (None, _) => "All environments".into(),
            (Some(_), [only]) => only.label.clone(),
            (Some(_), many) => format!("{} environments", many.len()).into(),
        };
        let show_status = !showing_limits;
        let scanning = show_status && self.scanning_count() > 0;
        let has_issue = show_status
            && (selected.iter().any(|status| status.error.is_some())
                || !self.merged.contract_mismatches.is_empty());
        let view = cx.entity();
        DropdownMenu::new("usage-environments")
            .align(Align::Start)
            .min_width(px(224.))
            .trigger(move |open| {
                let indicator = if scanning {
                    Icon::new(IconName::CircleDashed)
                        .size(px(14.))
                        .color(colors.muted_foreground)
                        .into_any_element()
                } else if has_issue {
                    Icon::new(IconName::CircleAlert)
                        .size(px(14.))
                        .color(colors.warning_foreground)
                        .into_any_element()
                } else {
                    div()
                        .when(!open, |this| this.opacity(0.))
                        .group_hover("usage-environment", |style| style.opacity(1.))
                        .child(
                            Icon::new(IconName::ChevronDown)
                                .size(px(14.))
                                .color(colors.muted_foreground),
                        )
                        .into_any_element()
                };
                inline_button("usage-environment-trigger", false, colors)
                    .group("usage-environment")
                    .min_w_0()
                    .max_w_full()
                    .gap(px(2.))
                    .child(div().min_w_0().truncate().child(label))
                    .child(
                        div()
                            .size(px(14.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(indicator),
                    )
                    .into_any_element()
            })
            .items(move |_, cx| view.read(cx).environment_menu_items(show_status, &view, cx))
            .into_any_element()
    }

    /// Rows of the environment menu.
    fn environment_menu_items(
        &self,
        show_status: bool,
        view: &gpui_kit::Entity<Self>,
        cx: &gpui_kit::App,
    ) -> Vec<AnyElement> {
        let colors = cx.colors();
        let all_ids: BTreeSet<_> = self
            .statuses
            .iter()
            .map(|status| status.id.clone())
            .collect();
        let mut items = Vec::new();
        items.push(
            MenuCheckboxItem::new("usage-env-all", "All environments")
                .checked(self.selected.is_none())
                .on_change({
                    let view = view.clone();
                    move |checked, _, cx| {
                        let next = if *checked {
                            None
                        } else {
                            Some(BTreeSet::new())
                        };
                        view.update(cx, |this, cx| this.set_selection(next, cx));
                    }
                })
                .into_any_element(),
        );
        items.push(MenuSeparator.into_any_element());
        for status in &self.statuses {
            let id = status.id.clone();
            let label = if show_status {
                format!("{}  {}", status.label, status.status_label())
            } else {
                status.label.to_string()
            };
            let selected_ids: BTreeSet<_> = self
                .selected_statuses()
                .map(|status| status.id.clone())
                .collect();
            let all_ids = all_ids.clone();
            items.push(
                MenuCheckboxItem::new(SharedString::from(format!("usage-env-{id}")), label)
                    .checked(self.is_selected(&id))
                    .on_change({
                        let view = view.clone();
                        move |checked, _, cx| {
                            let mut next = selected_ids.clone();
                            if *checked {
                                next.insert(id.clone());
                            } else {
                                next.remove(&id);
                            }
                            let next = (next != all_ids).then_some(next);
                            view.update(cx, |this, cx| this.set_selection(next, cx));
                        }
                    })
                    .into_any_element(),
            );
        }
        let note = |text: String| {
            div()
                .px_2()
                .py_2()
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(text)
                .into_any_element()
        };
        if self.statuses.is_empty() {
            items.push(note("No environments connected.".into()));
        }
        if show_status && self.is_partial() {
            items.push(note(
                "Totals are partial while selected environments scan.".into(),
            ));
        }
        if show_status {
            let mut lines = Vec::new();
            for status in self.selected_statuses() {
                if status.error.is_some() {
                    lines.push(format!("{} could not report usage.", status.label));
                }
            }
            for mismatch in &self.merged.contract_mismatches {
                if let Some(status) = self
                    .statuses
                    .iter()
                    .find(|status| status.id.as_str() == mismatch.environment_id)
                {
                    lines.push(mismatch.message(&status.label));
                }
            }
            if !self.merged.duplicate_sources.is_empty() {
                lines.push(format!(
                    "Counted once across environments sharing a transcript directory: {}",
                    self.merged.duplicate_sources.join(", ")
                ));
            }
            if !lines.is_empty() {
                items.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .border_t_1()
                        .border_color(colors.border)
                        .px_2()
                        .py_2()
                        .type_scale(text::XS)
                        .text_color(colors.muted_foreground)
                        .children(lines)
                        .into_any_element(),
                );
            }
        }
        items.push(MenuSeparator.into_any_element());
        // The model price overrides dialog is not ported yet.
        items.push(
            MenuItem::new("usage-model-prices", "Model prices")
                .icon(IconName::SlidersHorizontal)
                .disabled(true)
                .into_any_element(),
        );
        items
    }
}
