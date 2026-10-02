//! The model picker (`chat.md` 5.1-5.2): the footer trigger and the 400×384 panel with the
//! provider rail, search, and model list.
//!
//! The panel is an entity owned by the composer while open. Keys: ↑/↓ move the highlight,
//! Enter picks it, Escape closes, ⌘1-9 (`modelPicker.jump.N`, routed by the composer) pick the
//! Nth row.

use std::sync::Arc;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    FontWeight, InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    base::input::{Input, InputEvent, InputState},
    deferred, div,
    prelude::FluentBuilder as _,
    px, svg,
};
use t3_logic::{
    composer::providers::{self, PickerModel, PickerRail, ProviderEntry},
    keybindings::{Command, Platform, ShortcutContext, shortcut_label},
    settings::ModelFavorite,
};
use t3_protocol::{ProviderInstanceId, orchestration::ModelSelection, server::ServerConfig};
use t3_ui::{ActiveColors as _, Icon, IconName, Logo, TooltipExt as _, logo, tokens::shadow};

use super::{Composer, style};
use crate::state::AppState;

/// What the panel reports to the composer.
pub enum ModelPickerEvent {
    Selected(ModelSelection),
    Dismissed,
}

/// The open picker panel.
pub struct ModelPicker {
    config: Arc<ServerConfig>,
    active_instance: ProviderInstanceId,
    active_model: String,
    favorites: Vec<ModelFavorite>,
    rail: PickerRail,
    search: Entity<InputState>,
    highlighted: Option<String>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ModelPickerEvent> for ModelPicker {}

impl Focusable for ModelPicker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModelPicker {
    pub fn new(
        config: Arc<ServerConfig>,
        active_instance: ProviderInstanceId,
        active_model: String,
        favorites: Vec<ModelFavorite>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search models..."));
        let app_state = AppState::global(cx);
        let subscriptions = vec![
            cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.highlighted = None;
                    cx.notify();
                }
            }),
            cx.observe(&app_state, |this, state, cx| {
                this.favorites = state.read(cx).settings().favorites.clone();
                cx.notify();
            }),
        ];
        let rail = providers::initial_rail(&favorites, &active_instance);
        Self {
            config,
            active_instance,
            active_model,
            favorites,
            rail,
            search,
            highlighted: None,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |search, cx| search.focus(window, cx));
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    fn rows<'a>(&'a self, query: &str) -> Vec<PickerModel<'a>> {
        providers::picker_models(&self.config.providers, &self.favorites, &self.rail, query)
    }

    fn highlighted_key(&self, rows: &[PickerModel<'_>]) -> Option<String> {
        self.highlighted
            .clone()
            .filter(|key| rows.iter().any(|row| &row.key() == key))
            .or_else(|| rows.first().map(PickerModel::key))
    }

    fn select(instance_id: ProviderInstanceId, model: String, cx: &mut Context<Self>) {
        cx.emit(ModelPickerEvent::Selected(ModelSelection {
            instance_id,
            model,
            options: Vec::new(),
        }));
    }

    fn picked(row: &PickerModel<'_>) -> (ProviderInstanceId, String) {
        (row.entry.instance_id().clone(), row.model.slug.clone())
    }

    /// Picks the `index`th row (`modelPicker.jump.N`).
    pub fn jump(&mut self, index: usize, cx: &mut Context<Self>) {
        let query = self.query(cx);
        let picked = self.rows(&query).get(index).map(Self::picked);
        if let Some((instance, model)) = picked {
            Self::select(instance, model, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let query = self.query(cx);
        let rows = self.rows(&query);
        let current = self.highlighted_key(&rows);
        let position = current
            .as_ref()
            .and_then(|key| rows.iter().position(|row| &row.key() == key));
        match event.keystroke.key.as_str() {
            "down" | "up" if !rows.is_empty() => {
                let len = rows.len();
                let next = match (event.keystroke.key.as_str(), position) {
                    ("down", Some(index)) => (index + 1).min(len - 1),
                    ("up", Some(index)) => index.saturating_sub(1),
                    _ => 0,
                };
                self.highlighted = Some(rows[next].key());
                cx.stop_propagation();
                cx.notify();
            }
            "enter" => {
                if let Some((instance, model)) =
                    position.and_then(|index| rows.get(index)).map(Self::picked)
                {
                    Self::select(instance, model, cx);
                }
                cx.stop_propagation();
            }
            "escape" => {
                cx.emit(ModelPickerEvent::Dismissed);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn toggle_favorite(&self, instance: String, model: String, cx: &mut Context<Self>) {
        AppState::global(cx).update(cx, |state, cx| {
            state.update_settings(
                |settings| providers::toggle_favorite(&mut settings.favorites, &instance, &model),
                cx,
            )
        });
    }
}

/// The provider glyph for a driver, or `None` for drivers without a logo.
pub fn driver_logo(driver: &str) -> Option<Logo> {
    Some(match driver {
        "codex" => Logo::OpenAI,
        "claudeAgent" => Logo::ClaudeAI,
        "opencode" => Logo::OpenCodeIcon,
        "cursor" => Logo::CursorIcon,
        "grok" => Logo::GrokIcon,
        _ => return None,
    })
}

/// The provider icon at `size`, or the instance's initials for unknown drivers
/// (`ProviderInstanceIcon`).
pub fn provider_icon(entry: &ProviderEntry<'_>, size: gpui_kit::Pixels, cx: &App) -> AnyElement {
    let dark = cx.colors().is_dark;
    match driver_logo(entry.driver()) {
        Some(glyph) => logo(glyph, dark, size).into_any_element(),
        None => div()
            .text_size(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(providers::instance_initials(&entry.display_name()))
            .into_any_element(),
    }
}

impl Render for ModelPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let query = self.query(cx);
        let searching = !query.trim().is_empty();
        let entries = providers::provider_entries(&self.config.providers);
        let rail_entries: Vec<ProviderEntry<'_>> = entries
            .into_iter()
            .filter(ProviderEntry::picker_visible)
            .collect();
        let show_rail = !searching && !rail_entries.is_empty();
        let rows = self.rows(&query);
        let highlighted = self.highlighted_key(&rows);
        let keybindings = AppState::global(cx).read(cx).keybindings(cx);
        let jump_context = ShortcutContext {
            model_picker_open: true,
            ..Default::default()
        };
        let jump_label = |index: usize| {
            (index < 9)
                .then(|| {
                    shortcut_label(
                        &keybindings,
                        &Command::ModelPickerJump(index as u8 + 1),
                        &jump_context,
                        Platform::current(),
                    )
                })
                .flatten()
        };

        let rail = show_rail.then(|| {
            let selected = &self.rail;
            div()
                .id("model-picker-rail")
                .w(px(48.))
                .flex_none()
                .h_full()
                .bg(colors.muted.opacity(0.3))
                .overflow_y_scroll()
                .child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .px(px(4.))
                        .pb(px(4.))
                        .pt(px(2.))
                        .child(
                            div()
                                .mb(px(4.))
                                .pb(px(4.))
                                .border_b_1()
                                .border_color(colors.border)
                                .child(
                                    rail_button(
                                        "favorites",
                                        *selected == PickerRail::Favorites,
                                        false,
                                        cx,
                                    )
                                    .child(
                                        svg()
                                            .path("icons/composer/star-filled.svg")
                                            .size(px(20.))
                                            .text_color(colors.foreground),
                                    )
                                    .tooltip_text("Favorites")
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.rail = PickerRail::Favorites;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                        .children(rail_entries.iter().map(|entry| {
                            let id = entry.instance_id().clone();
                            let disabled = !entry.picker_ready();
                            let selected_here = *selected == PickerRail::Instance(id.clone());
                            let tooltip = if disabled {
                                entry.unavailable_reason()
                            } else {
                                entry.display_name()
                            };
                            rail_button(
                                SharedString::from(format!("rail-{id}")),
                                selected_here,
                                disabled,
                                cx,
                            )
                            .child(
                                div()
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(provider_icon(entry, px(20.), cx)),
                            )
                            .tooltip_text(tooltip)
                            .when(!disabled, |this| {
                                this.on_click(cx.listener(move |this, _, _, cx| {
                                    this.rail = PickerRail::Instance(id.clone());
                                    cx.notify();
                                }))
                            })
                        })),
                )
                .into_any_element()
        });

        let list = div()
            .id("model-picker-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(px(6.))
            .px(px(4.))
            .when(rows.is_empty(), |this| {
                this.child(
                    div()
                        .py(px(24.))
                        .text_size(px(12.))
                        .line_height(px(16.5))
                        .text_center()
                        .text_color(colors.muted_foreground)
                        .child("No models found"),
                )
            })
            .children(rows.iter().enumerate().map(|(index, row)| {
                let key = row.key();
                let selected = row.entry.instance_id() == &self.active_instance
                    && row.model.slug == self.active_model;
                let is_highlighted = highlighted.as_deref() == Some(key.as_str());
                let favorite = row.favorite;
                let instance = row.entry.instance_id().to_string();
                let slug = row.model.slug.clone();
                let display = row.entry.display_name();
                let provider_label = match &row.model.sub_provider {
                    Some(sub) => format!("{display} · {sub}"),
                    None => display,
                };
                let (picked_instance, picked_model) = Self::picked(row);
                div()
                    .id(SharedString::from(format!("model-{key}")))
                    .group("model-row")
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .w_full()
                    .px(px(8.))
                    .py(px(10.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(is_highlighted, |this| this.bg(colors.muted.opacity(0.56)))
                    .hover(|style| style.bg(colors.muted.opacity(0.56)))
                    .on_mouse_move(cx.listener({
                        let key = key.clone();
                        move |this, _, _, cx| {
                            if this.highlighted.as_deref() != Some(key.as_str()) {
                                this.highlighted = Some(key.clone());
                                cx.notify();
                            }
                        }
                    }))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        Self::select(picked_instance.clone(), picked_model.clone(), cx)
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(px(12.))
                                            .line_height(px(16.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(colors.foreground)
                                            .child(providers::model_display_name(row.model, true)),
                                    )
                                    .when(selected, |this| {
                                        this.child(
                                            Icon::new(IconName::Check)
                                                .size(px(14.))
                                                .color(style::palette::BLUE_400),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .mt(px(4.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(provider_icon(&row.entry, px(12.), cx))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(12.))
                                            .line_height(px(16.5))
                                            .text_color(style::alpha(colors.muted_foreground, 0.7))
                                            .child(provider_label),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(6.))
                            .when_some(jump_label(index), |this, label| {
                                this.child(
                                    div()
                                        .h(px(16.))
                                        .px(px(6.))
                                        .rounded(px(6.))
                                        .bg(colors.muted)
                                        .flex()
                                        .items_center()
                                        .text_size(px(10.))
                                        .line_height(px(16.))
                                        .text_color(colors.muted_foreground)
                                        .child(label),
                                )
                            })
                            .child(
                                div()
                                    .id(SharedString::from(format!("favorite-{key}")))
                                    .size(px(24.))
                                    .mr(px(-4.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|style| style.bg(colors.accent))
                                    .map(|this| {
                                        if favorite {
                                            this.child(
                                                svg()
                                                    .path("icons/composer/star-filled.svg")
                                                    .size(px(12.))
                                                    .text_color(style::palette::YELLOW_500),
                                            )
                                        } else {
                                            this.opacity(0.64).child(
                                                Icon::new(IconName::Star).size(px(12.)).color(
                                                    style::alpha(colors.muted_foreground, 0.7),
                                                ),
                                            )
                                        }
                                    })
                                    .tooltip_text(if favorite {
                                        "Remove from favorites"
                                    } else {
                                        "Add to favorites"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.toggle_favorite(instance.clone(), slug.clone(), cx)
                                    })),
                            ),
                    )
            }));

        let main = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(colors.muted.opacity(0.4))
            .when(show_rail, |this| {
                this.border_l_1().border_color(colors.border)
            })
            .child(
                div().px(px(16.)).pt(px(10.)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .pb(px(10.))
                        .border_b_1()
                        .border_color(colors.ring)
                        .child(
                            Icon::new(IconName::Search)
                                .size(px(16.))
                                .color(style::alpha(colors.muted_foreground, 0.55)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .h(px(26.))
                                .text_size(px(14.))
                                .child(Input::new(&self.search)),
                        ),
                ),
            )
            .child(list);

        div()
            .id("model-picker")
            .key_context("ModelPicker")
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(ModelPickerEvent::Dismissed)))
            .occlude()
            .relative()
            .flex()
            .flex_row()
            .w(px(400.))
            .h(px(384.))
            .overflow_hidden()
            .rounded(px(10.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.popover)
            .text_color(colors.popover_foreground)
            .shadow(shadow::LG_5.to_vec())
            .children(rail)
            .child(main)
    }
}

/// A square rail button (≈40px, radius 8, hover `muted`). A selected one gets the 3×20 primary
/// bar on the rail's right edge.
fn rail_button(
    id: impl Into<gpui_kit::ElementId>,
    selected: bool,
    disabled: bool,
    cx: &App,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    let colors = cx.colors();
    div()
        .id(id)
        .relative()
        .w_full()
        .h(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .when(!disabled, |this| {
            this.cursor_pointer().hover(|style| style.bg(colors.muted))
        })
        .when(disabled, |this| this.opacity(0.5))
        .when(selected, |this| {
            this.child(
                div()
                    .absolute()
                    .right(px(-4.))
                    .top(px(10.))
                    .w(px(3.))
                    .h(px(20.))
                    .rounded_l_full()
                    .bg(colors.primary),
            )
        })
}

impl Composer {
    /// The footer trigger: provider glyph, model name, chevron. Opens the picker above it.
    pub(super) fn render_model_trigger(
        &mut self,
        compact: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let Some((config, resolved)) = self.resolved_model(cx) else {
            return div().into_any_element();
        };
        let entries = providers::provider_entries(&config.providers);
        let entry = resolved
            .provider_index
            .and_then(|index| entries.get(index).copied());
        let model = entry.and_then(|entry| {
            entry
                .models()
                .iter()
                .find(|model| model.slug == resolved.model)
                .or_else(|| entry.models().first())
        });
        let label = model
            .map(|model| providers::model_display_name(model, true))
            .unwrap_or_else(|| resolved.model.clone());
        let open = self.model_picker.is_some();
        let trigger = style::ghost_trigger("composer-model-trigger", open, colors)
            .min_w_0()
            .max_w(if compact { px(168.) } else { px(224.) })
            .when_some(entry, |this, entry| {
                this.child(div().flex_none().child(provider_icon(&entry, px(16.), cx)))
            })
            .child(div().min_w_0().flex_1().truncate().child(label.clone()))
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(12.))
                    .opacity(0.6)
                    .mr(px(-4.)),
            )
            .tooltip_text_with_delay(label, std::time::Duration::ZERO)
            .on_click(cx.listener(|this, _, window, cx| {
                if this.model_picker.is_some() {
                    this.close_model_picker(window, cx);
                } else {
                    this.open_model_picker(window, cx);
                }
            }));
        div()
            .relative()
            .min_w_0()
            .child(trigger)
            .when_some(self.model_picker.clone(), |this, picker| {
                this.child(
                    deferred(div().absolute().bottom(px(28.)).left_0().child(picker))
                        .with_priority(2),
                )
            })
            .into_any_element()
    }
}
