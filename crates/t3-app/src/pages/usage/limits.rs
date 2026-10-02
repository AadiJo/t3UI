//! The Limits tab (`UsageLimitsPooled.tsx`, `UsageLimits.tsx`): one section per provider with a
//! card per pooled window, external usage links, and notices for sources without bars.
//!
//! Not ported yet: the segment popovers (plan, email, reset credit redeem), the Cursor Keychain
//! prompt, and the ChatGPT shared-usage row. Segments show their summary as a tooltip.

use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    relative,
};
use t3_logic::usage::{
    ProviderKind,
    limits::{
        CHATGPT_USAGE_URL, LimitAccount, LimitPace, LimitPool, LimitPoolWindow, PoolColumn,
        collect_external_usage_links, collect_limit_accounts, collect_limit_notices,
        collect_limit_pools, cursor_window_details, driver_label, format_resets_in,
        remaining_percent,
    },
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Colors, Icon, IconName, Logo,
    TooltipExt as _, logo,
    tokens::{radius, text},
};

use super::{UsageView, provider_color};
use crate::{chrome::TypeScale as _, pages::controls::tabular_nums};

/// `@2xl/pool`: segments carry their own labels once the bar is this wide.
const POOL_WIDE: f32 = 672.;
/// `md`: the card puts the figure beside the bar.
const MD: f32 = 768.;

/// The series color the cost chart uses for this driver (`barColor`).
fn bar_color(driver: &str, colors: &Colors) -> gpui_kit::Hsla {
    match driver {
        "codex" => provider_color(ProviderKind::Codex, colors),
        "claudeAgent" => provider_color(ProviderKind::Claude, colors),
        _ => colors.foreground,
    }
}

fn driver_logo(driver: &str) -> Option<Logo> {
    Some(match driver {
        "codex" => Logo::OpenAI,
        "claudeAgent" => Logo::ClaudeAI,
        "cursor" => Logo::CursorIcon,
        "opencode" => Logo::OpenCodeIcon,
        "grok" => Logo::GrokIcon,
        "antigravity" => Logo::AntigravityIcon,
        _ => return None,
    })
}

fn pace_icon(pace: LimitPace) -> IconName {
    match pace {
        LimitPace::Ahead => IconName::TrendingUp,
        LimitPace::On => IconName::Gauge,
        LimitPace::Under => IconName::TrendingDown,
    }
}

/// `someone@example.com` -> `SE`.
fn account_initials(email: &str) -> String {
    let (local, domain) = email.split_once('@').unwrap_or((email, ""));
    let initials: String = local
        .chars()
        .take(1)
        .chain(domain.chars().take(1))
        .collect::<String>()
        .to_uppercase();
    if initials.is_empty() {
        "?".into()
    } else {
        initials
    }
}

/// The account as the bar names it: the instance name, else initials, else the driver.
fn account_name(account: &LimitAccount) -> String {
    account
        .display_name
        .clone()
        .or_else(|| account.email.as_deref().map(account_initials))
        .unwrap_or_else(|| driver_label(&account.driver))
}

impl UsageView {
    pub(super) fn render_limits(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let environments = self.limit_environments(cx);
        let accounts = collect_limit_accounts(&environments);
        let pools = collect_limit_pools(&accounts, self.limits_now);
        let notices: Vec<String> = collect_limit_notices(&environments)
            .into_iter()
            .filter(|notice| {
                !notice.ends_with("Codex: Codex could not read usage (JSON-RPC -32600).")
            })
            .collect();
        let links = collect_external_usage_links(&environments);
        let content_width = self.content_width(window, cx);
        let card_beside = window.viewport_size().width >= px(MD);
        // The bar's width: the card's inner width minus the 11rem figure column and its gap.
        let bar_width = content_width - px(34.) - if card_beside { px(176. + 24.) } else { px(0.) };
        let wide_bar = bar_width >= px(POOL_WIDE);

        let mut sections: Vec<AnyElement> = Vec::new();
        if pools.is_empty() && notices.is_empty() && links.is_empty() {
            sections.push(
                div()
                    .type_scale(text::SM)
                    .text_color(colors.muted_foreground)
                    .child("No provider on the selected environments reports subscription limits.")
                    .into_any_element(),
            );
        }
        for pool in &pools {
            sections.push(
                self.pool_section(pool, card_beside, wide_bar, cx)
                    .into_any_element(),
            );
        }
        for link in links {
            let chatgpt = link.url == CHATGPT_USAGE_URL;
            let url = link.url.clone();
            let detail = if chatgpt {
                Some("View usage in ChatGPT with your connected account.".to_owned())
            } else {
                link.message.clone()
            };
            sections.push(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .rounded(radius::XL)
                    .border_1()
                    .border_color(colors.border)
                    .p_4()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_3()
                            .when(chatgpt, |this| {
                                this.child(logo(Logo::OpenAI, colors.is_dark, px(20.)))
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        div()
                                            .type_scale(text::SM)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(link.label.clone()),
                                    )
                                    .children(detail.map(|detail| {
                                        div()
                                            .max_w(px(576.))
                                            .type_scale(text::XS)
                                            .text_color(colors.muted_foreground)
                                            .child(detail)
                                    })),
                            ),
                    )
                    .child(
                        Button::new(SharedString::from(format!("usage-manage-{url}")))
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Xs)
                            .label("Manage usage")
                            .icon_end(IconName::ExternalLink)
                            .on_click(move |_: &ClickEvent, _, cx: &mut App| cx.open_url(&url)),
                    )
                    .into_any_element(),
            );
        }
        if !notices.is_empty() {
            sections.push(
                div()
                    .flex()
                    .gap_2()
                    .rounded(radius::XL)
                    .border_1()
                    .border_color(colors.warning_32)
                    .bg(colors.warning_4)
                    .px(px(14.))
                    .py_3()
                    .type_scale(text::SM)
                    .child(
                        Icon::new(IconName::TriangleAlert)
                            .size(px(16.))
                            .color(colors.warning),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .font_weight(FontWeight::MEDIUM)
                            .children(notices),
                    )
                    .into_any_element(),
            );
        }
        div().flex().flex_col().gap_8().children(sections)
    }

    fn pool_section(
        &self,
        pool: &LimitPool,
        card_beside: bool,
        wide_bar: bool,
        cx: &App,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let color = bar_color(&pool.driver, colors);
        let label = driver_label(&pool.driver);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .type_scale(text::SM)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(
                        div()
                            .size_5()
                            .flex()
                            .items_center()
                            .justify_center()
                            .children(
                                driver_logo(&pool.driver)
                                    .map(|mark| logo(mark, colors.is_dark, px(16.))),
                            ),
                    )
                    .child(label),
            )
            .children(pool.display_windows().into_iter().map(|window| {
                let details = (pool.driver == "cursor")
                    .then(|| cursor_window_details(&window.id))
                    .flatten();
                self.pool_card(window, details, color, card_beside, wide_bar, cx)
            }))
    }

    /// One pooled window (`PoolWindowCard`).
    fn pool_card(
        &self,
        pool: &LimitPoolWindow,
        details: Option<(&'static str, &'static str)>,
        color: gpui_kit::Hsla,
        card_beside: bool,
        wide_bar: bool,
        cx: &App,
    ) -> AnyElement {
        let colors = cx.colors();
        let label = details.map_or_else(|| pool.label.clone(), |(label, _)| label.to_owned());
        let next_refill = pool.next_refill().filter(|_| pool.columns.len() > 1);
        let figure = div()
            .flex()
            .flex_col()
            .gap_1()
            .when(card_beside, |this| this.w(px(176.)).flex_shrink_0())
            .child(
                div()
                    .type_scale(text::SM)
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(
                        div()
                            .type_scale(text::XL3)
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(tabular_nums())
                            .text_color(colors.foreground)
                            .child(format!("{}%", pool.remaining_percent)),
                    )
                    .child(
                        div()
                            .type_scale(text::SM)
                            .text_color(colors.muted_foreground)
                            .child("left"),
                    )
                    .children(pool.pace.map(|pace| {
                        div()
                            .id(SharedString::from(format!("usage-pace-{}", pool.id)))
                            .tooltip_text(pace.label())
                            .child(
                                Icon::new(pace_icon(pace))
                                    .size(px(14.))
                                    .color(colors.muted_foreground),
                            )
                    })),
            )
            .children(next_refill.map(|reset| {
                div()
                    .type_scale(text::XS)
                    .font_weight(FontWeight::MEDIUM)
                    .font_features(tabular_nums())
                    .text_color(colors.foreground)
                    .child(format!("↻ +{}%", reset.restores_percent))
            }));
        let bar = self.pool_bar(pool, color, wide_bar, cx);
        div()
            .rounded(radius::LG)
            .border_1()
            .border_color(colors.border_60)
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_x_6()
                    .gap_y_3()
                    .map(|this| {
                        if card_beside {
                            this.flex_row()
                        } else {
                            this.flex_col().items_start()
                        }
                    })
                    .child(figure)
                    .child(div().min_w_0().flex_1().w_full().child(bar)),
            )
            .children(details.map(|(_, description)| {
                div()
                    .type_scale(text::XS)
                    .text_color(colors.muted_foreground)
                    .child(description)
            }))
            .into_any_element()
    }

    /// Equal-width segments, one per account (`PoolBar`); narrow bars add legend rows.
    fn pool_bar(
        &self,
        pool: &LimitPoolWindow,
        color: gpui_kit::Hsla,
        wide: bool,
        cx: &App,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let now = self.limits_now;
        let show_names = pool.columns.len() > 1;
        let segments = pool.columns.iter().enumerate().map(|(position, column)| {
            self.segment(pool, column, position + 1, color, wide, show_names, cx)
        });
        let legend = (!wide).then(|| {
            pool.columns
                .iter()
                .enumerate()
                .filter_map(|(position, column)| {
                    let window = column.window.as_ref()?;
                    let resets = format_resets_in(window, now)
                        .map(|text| text.replace("resets in ", "↻ "))
                        .unwrap_or_default();
                    Some(
                        div()
                            .h_7()
                            .px_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(radius::MD)
                            .type_scale(text::XS)
                            .font_features(tabular_nums())
                            .child(
                                div()
                                    .relative()
                                    .size_4()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(radius::SM)
                                    .type_scale(text::XS3)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground_80)
                                    .child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .rounded(radius::SM)
                                            .bg(color.opacity(0.35)),
                                    )
                                    .child(div().relative().child((position + 1).to_string())),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(colors.foreground)
                                    .child(account_name(&column.account)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(format!("{}%", remaining_percent(window))),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .flex_shrink_0()
                                    .type_scale(text::XS2)
                                    .text_color(colors.muted_foreground)
                                    .child(resets),
                            )
                            .into_any_element(),
                    )
                })
                .collect::<Vec<_>>()
        });
        div()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().w_full().flex().gap_1().children(segments))
            .children(legend.into_iter().flatten())
    }

    #[allow(clippy::too_many_arguments)]
    fn segment(
        &self,
        pool: &LimitPoolWindow,
        column: &PoolColumn,
        index: usize,
        color: gpui_kit::Hsla,
        wide: bool,
        show_name: bool,
        cx: &App,
    ) -> AnyElement {
        let colors = cx.colors();
        let Some(window) = &column.window else {
            return div().flex_1().min_w_0().into_any_element();
        };
        let now = self.limits_now;
        let remaining = remaining_percent(window);
        let resets_in = format_resets_in(window, now);
        let credits = column
            .account
            .limits
            .reset_credits
            .as_ref()
            .map_or(0, |credits| credits.available_count);
        let has_reset = pool.reset_for(&column.account.key).is_some();
        let name = account_name(&column.account);
        let summary = format!(
            "{name}: {remaining}% left{}{}",
            resets_in
                .as_ref()
                .map(|text| format!(", {text}"))
                .unwrap_or_default(),
            if credits > 0 {
                format!(
                    ", {credits} reset {} banked",
                    if credits == 1 { "credit" } else { "credits" }
                )
            } else {
                String::new()
            }
        );
        div()
            .id(SharedString::from(format!(
                "usage-segment-{}-{}",
                pool.id, column.account.key
            )))
            .relative()
            .flex_1()
            .min_w_0()
            .h(if wide { px(32.) } else { px(20.) })
            .overflow_hidden()
            .rounded(radius::MD)
            .bg(colors.muted)
            .cursor_pointer()
            .tooltip_text(summary)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(remaining as f32 / 100.))
                    .rounded(radius::MD)
                    .bg(color.opacity(0.35)),
            )
            // The spent share is hatched in the web (135deg, 1px lines every 5px at 20%); a
            // flat 8% wash stands in until hatching is painted.
            .when(remaining < 100 && has_reset, |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(relative((100 - remaining) as f32 / 100.))
                        .bg(color.opacity(0.08)),
                )
            })
            .map(|this| {
                if wide {
                    this.child(
                        div()
                            .relative()
                            .h_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px_2()
                            .type_scale(text::XS)
                            .font_features(tabular_nums())
                            .when(show_name, |this| {
                                this.child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground)
                                        .child(name.clone()),
                                )
                            })
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(format!("{remaining}%")),
                            )
                            .child(
                                div()
                                    .ml_auto()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .rounded(radius::SM)
                                    .bg(colors.background_85)
                                    .px(px(6.))
                                    .py(px(2.))
                                    .type_scale(text::XS2)
                                    .text_color(colors.foreground)
                                    .child(
                                        resets_in
                                            .map(|text| text.replace("resets in ", "↻ "))
                                            .unwrap_or_default(),
                                    )
                                    .when(credits > 0, |this| {
                                        this.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(2.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(Icon::new(IconName::Ticket).size(px(12.)))
                                                .child(credits.to_string()),
                                        )
                                    }),
                            ),
                    )
                } else {
                    this.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .type_scale(text::XS3)
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(tabular_nums())
                            .text_color(colors.foreground_80)
                            .child(index.to_string()),
                    )
                }
            })
            .into_any_element()
    }
}
