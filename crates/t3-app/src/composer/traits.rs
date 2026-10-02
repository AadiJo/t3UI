//! The traits picker (`chat.md` 5.3): reasoning effort, service tier, fast mode, and other model
//! options as radio sections in a compact menu, opened from a ghost `xs` trigger that reads
//! e.g. "Medium · Standard". The compact footer folds it (and the plan toggle) into an
//! ellipsis menu.

use gpui_kit::{
    AnyElement, App, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    deferred, div, prelude::FluentBuilder as _, px,
};
use t3_logic::composer::{
    prompt,
    providers::{self, TraitLabelPart, select_current, trait_option_label},
};
use t3_protocol::{
    orchestration::ProviderOptionValue,
    server::{ProviderOptionDescriptor, ServerProviderModel},
};
use t3_ui::{
    ActiveColors as _, Icon, IconName, MenuCheckboxItem, MenuItem, MenuPopup, MenuSeparator,
};

use super::{Composer, ComposerEvent, style};

impl Composer {
    /// The selected model and its descriptors with the composer's picks applied.
    fn trait_descriptors(&self, cx: &App) -> Option<(ServerProviderModel, Vec<ProviderOptionDescriptor>)> {
        let (config, resolved) = self.resolved_model(cx)?;
        let provider = Self::selected_provider(&config, &resolved)?;
        let model = providers::resolve_model(&provider.models, &resolved.model)?.clone();
        let descriptors = providers::option_descriptors(Some(&model), &resolved.options);
        Some((model, descriptors))
    }

    /// Whether the prompt controls the primary effort ("Ultrathink:").
    fn ultrathink(&self, descriptors: &[ProviderOptionDescriptor], cx: &App) -> bool {
        let injectable = descriptors.iter().find_map(|descriptor| match descriptor {
            ProviderOptionDescriptor::Select {
                prompt_injected_values,
                ..
            } => Some(!prompt_injected_values.is_empty()),
            _ => None,
        });
        injectable == Some(true) && prompt::mentions_ultrathink(&self.prompt(cx))
    }

    /// The traits trigger with its menu, or `None` when the model has no options.
    pub(super) fn render_traits(
        &mut self,
        compact: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (_, descriptors) = self.trait_descriptors(cx)?;
        if descriptors.is_empty() || compact {
            return (!descriptors.is_empty()).then(|| div().into_any_element());
        }
        let colors = cx.colors();
        let parts = providers::traits_label(&descriptors, self.ultrathink(&descriptors, cx));
        let open = self.traits_open;
        let label = div()
            .flex()
            .min_w_0()
            .flex_1()
            .truncate()
            .children(parts.iter().enumerate().flat_map(|(index, part)| {
                let separator = (index > 0).then(|| div().child(" · ").into_any_element());
                separator.into_iter().chain(std::iter::once(trait_part(part)))
            }));
        let trigger = style::ghost_trigger("composer-traits-trigger", open, colors)
            .min_w_0()
            .max_w(px(192.))
            .child(label)
            .child(Icon::new(IconName::ChevronDown).size(px(12.)).opacity(0.6))
            .on_click(cx.listener(|this, _, _, cx| {
                this.traits_open = !this.traits_open;
                cx.notify();
            }));
        Some(
            div()
                .relative()
                .min_w_0()
                .child(trigger)
                .when(open, |this| {
                    this.child(
                        deferred(
                            div()
                                .absolute()
                                .bottom(px(28.))
                                .left_0()
                                .child(self.render_traits_menu(&descriptors, cx)),
                        )
                        .with_priority(2),
                    )
                })
                .into_any_element(),
        )
    }

    /// The menu body: one radio section per select, On/Off per boolean (`TraitsMenuContent`).
    fn render_traits_menu(
        &self,
        descriptors: &[ProviderOptionDescriptor],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.colors();
        let ultrathink = self.ultrathink(descriptors, cx);
        let body_mentions = ultrathink
            && prompt::mentions_ultrathink(prompt::strip_ultrathink_prefix(&self.prompt(cx)));
        let section_label = |text: &str| {
            div()
                .px(px(8.))
                .pt(px(4.))
                .pb(px(2.))
                .text_size(px(11.))
                .line_height(px(14.66))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(text.to_owned())
                .into_any_element()
        };
        let mut children: Vec<AnyElement> = Vec::new();
        let primary = descriptors
            .iter()
            .find(|descriptor| matches!(descriptor, ProviderOptionDescriptor::Select { .. }))
            .and_then(providers::descriptor_id)
            .map(str::to_owned);
        for (index, descriptor) in descriptors.iter().enumerate() {
            if index > 0 {
                children.push(div().mx(px(-2.)).child(MenuSeparator).into_any_element());
            }
            match descriptor {
                ProviderOptionDescriptor::Select {
                    id,
                    label,
                    options,
                    current_value,
                    ..
                } => {
                    let is_primary = primary.as_deref() == Some(id.as_str());
                    children.push(section_label(label));
                    if body_mentions && is_primary {
                        children.push(
                            div()
                                .px(px(8.))
                                .pb(px(6.))
                                .text_size(px(12.))
                                .text_color(colors.muted_foreground.opacity(0.8))
                                .child("Your prompt contains \"ultrathink\" in the text. Remove it to change this option.")
                                .into_any_element(),
                        );
                    }
                    let current = if ultrathink && is_primary {
                        Some("ultrathink")
                    } else {
                        select_current(current_value, options)
                    };
                    for option in options {
                        let text = format!(
                            "{}{}",
                            trait_option_label(id, label, option),
                            if option.is_default { " (default)" } else { "" }
                        );
                        let (descriptor_id, value) = (id.clone(), option.id.clone());
                        children.push(
                            MenuCheckboxItem::new(
                                SharedString::from(format!("trait-{id}-{}", option.id)),
                                text,
                            )
                            .compact()
                            .checked(current == Some(option.id.as_str()))
                            .disabled(body_mentions && is_primary)
                            .on_change(self.trait_listener(
                                descriptor_id,
                                ProviderOptionValue::String(value),
                                cx,
                            ))
                            .into_any_element(),
                        );
                    }
                }
                ProviderOptionDescriptor::Boolean {
                    id,
                    label,
                    current_value,
                    ..
                } => {
                    children.push(section_label(label));
                    for (text, value) in [("On", true), ("Off", false)] {
                        children.push(
                            MenuCheckboxItem::new(
                                SharedString::from(format!("trait-{id}-{text}")),
                                text,
                            )
                            .compact()
                            .checked(current_value.unwrap_or(false) == value)
                            .on_change(self.trait_listener(
                                id.clone(),
                                ProviderOptionValue::Bool(value),
                                cx,
                            ))
                            .into_any_element(),
                        );
                    }
                }
                ProviderOptionDescriptor::Unknown => {}
            }
        }
        div()
            .id("composer-traits-menu")
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.traits_open = false;
                cx.notify();
            }))
            .child(MenuPopup::new().compact().w(px(160.)).children(children))
    }

    /// Applies a trait pick; prompt-injected values (Claude "ultrathink") edit the prompt instead.
    fn trait_listener(
        &self,
        descriptor_id: String,
        value: ProviderOptionValue,
        cx: &mut Context<Self>,
    ) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        let entity = cx.entity();
        move |_, window, cx| {
            entity.update(cx, |this, cx| {
                this.traits_open = false;
                let Some((_, descriptors)) = this.trait_descriptors(cx) else {
                    return;
                };
                let injected = descriptors.iter().any(|descriptor| {
                    matches!(descriptor, ProviderOptionDescriptor::Select { id, prompt_injected_values, .. }
                        if *id == descriptor_id
                            && matches!(&value, ProviderOptionValue::String(v) if prompt_injected_values.contains(v)))
                });
                let text = this.prompt(cx);
                if injected {
                    let next = if text.trim().is_empty() {
                        prompt::ULTRATHINK_PREFIX.to_owned()
                    } else {
                        format!(
                            "{}{}",
                            prompt::ULTRATHINK_PREFIX,
                            prompt::strip_ultrathink_prefix(&text)
                        )
                    };
                    this.editor.update(cx, |state, cx| state.set_value(next, window, cx));
                    this.on_prompt_changed(window, cx);
                    return;
                }
                if this.ultrathink(&descriptors, cx) {
                    let stripped = prompt::strip_ultrathink_prefix(&text).to_owned();
                    this.editor
                        .update(cx, |state, cx| state.set_value(stripped, window, cx));
                    this.on_prompt_changed(window, cx);
                }
                let next = providers::with_descriptor_value(&descriptors, &descriptor_id, &value);
                this.set_model_options(providers::selections_from_descriptors(&next), cx);
                cx.notify();
            });
        }
    }

    /// The compact footer's ellipsis menu: traits, then the plan item.
    pub(super) fn render_compact_menu(
        &mut self,
        has_traits: bool,
        plan: Option<(SharedString, bool)>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let open = self.traits_open;
        let descriptors = self
            .trait_descriptors(cx)
            .map(|(_, descriptors)| descriptors)
            .unwrap_or_default();
        div()
            .relative()
            .child(
                style::ghost_trigger("composer-compact-menu", open, colors)
                    .h(px(28.))
                    .child(Icon::new(IconName::Ellipsis).size(px(16.)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.traits_open = !this.traits_open;
                        cx.notify();
                    })),
            )
            .when(open, |this| {
                let menu = div()
                    .flex()
                    .flex_col()
                    .when(has_traits, |this| this.child(self.render_traits_menu(&descriptors, cx)))
                    .when_some(plan, |this, (label, open)| {
                        this.child(
                            MenuPopup::new().compact().child(
                                MenuItem::new("compact-plan", format!("{} {} sidebar", if open { "Hide" } else { "Show" }, label.to_lowercase()))
                                    .icon(IconName::ListTodo)
                                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ComposerEvent::TogglePlanSidebar))),
                            ),
                        )
                    });
                this.child(deferred(div().absolute().bottom(px(32.)).left_0().child(menu)).with_priority(2))
            })
            .into_any_element()
    }
}

/// One label part; reasoning "Ultra" is purple.
fn trait_part(part: &TraitLabelPart) -> AnyElement {
    div()
        .when(part.ultra, |this| this.text_color(style::palette::PURPLE_400))
        .child(part.label.clone())
        .into_any_element()
}
