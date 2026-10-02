//! Provider instances, model choice, and model options ("traits") for the composer
//! (`providerInstances.ts`, `ChatComposer.tsx` instance resolution, `composerDraftStore.ts`
//! `deriveEffectiveComposerModelState`, `ModelPickerContent.tsx`, `TraitsPicker.tsx`,
//! `shared/model.ts`).

use std::collections::BTreeSet;

use t3_protocol::{
    ProviderInstanceId,
    orchestration::{ModelSelection, ProviderOptionSelection, ProviderOptionValue},
    server::{
        ProviderOptionChoice, ProviderOptionDescriptor, ProviderStatus, ServerProvider,
        ServerProviderModel,
    },
};

use super::search::{ModelSearchFields, score_model};
use crate::settings::ModelFavorite;

/// The instance every composer falls back to when nothing else resolves.
pub const FALLBACK_INSTANCE: &str = "codex";

/// One configured provider instance as the pickers see it (`ProviderInstanceEntry`).
#[derive(Clone, Copy, Debug)]
pub struct ProviderEntry<'a> {
    pub provider: &'a ServerProvider,
    /// The default instance of its driver (instance id == driver).
    pub is_default: bool,
}

impl<'a> ProviderEntry<'a> {
    pub fn instance_id(&self) -> &'a ProviderInstanceId {
        &self.provider.instance_id
    }

    pub fn driver(&self) -> &'a str {
        &self.provider.driver
    }

    pub fn models(&self) -> &'a [ServerProviderModel] {
        &self.provider.models
    }

    pub fn enabled(&self) -> bool {
        self.provider.enabled
    }

    /// `availability` is absent or not `unavailable`.
    pub fn is_available(&self) -> bool {
        self.provider.availability.as_deref() != Some("unavailable")
    }

    /// Can contribute models to the picker (`isProviderInstancePickerReady`).
    pub fn picker_ready(&self) -> bool {
        self.enabled() && self.is_available() && self.provider.status == ProviderStatus::Ready
    }

    /// Shown in the picker rail (`isProviderInstancePickerVisible`).
    pub fn picker_visible(&self) -> bool {
        self.enabled()
    }

    /// Continuation group: instances sharing it can take over each other's threads.
    pub fn continuation_group(&self) -> Option<&'a str> {
        self.provider
            .continuation
            .as_ref()
            .map(|continuation| continuation.group_key.as_str())
    }

    /// The label pickers and tooltips use (`resolveInstanceDisplayName`).
    pub fn display_name(&self) -> String {
        let kind_label = driver_label(self.driver());
        let snapshot = self
            .provider
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        if let Some(name) = snapshot
            && name != kind_label
        {
            return name.to_owned();
        }
        if !self.is_default {
            let humanized = humanize_instance_id(self.instance_id().as_str());
            if !humanized.is_empty() {
                return humanized;
            }
        }
        snapshot.map_or(kind_label, str::to_owned)
    }

    /// `#rrggbb` accent, if valid.
    pub fn accent_color(&self) -> Option<&'a str> {
        let accent = self.provider.accent_color.as_deref()?.trim();
        (accent.len() == 7
            && accent.starts_with('#')
            && accent[1..].chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(accent)
    }

    /// Rail tooltip for an instance that cannot be picked (`describeUnavailableInstance`).
    pub fn unavailable_reason(&self) -> String {
        let label = self.display_name();
        if !self.enabled() || self.provider.status == ProviderStatus::Disabled {
            return format!("{label} — Disabled in settings.");
        }
        if self.provider.status == ProviderStatus::Ready && self.is_available() {
            return label;
        }
        let kind = match self.provider.status {
            ProviderStatus::Error => "Unavailable",
            ProviderStatus::Warning => "Limited",
            _ => "Not ready",
        };
        match self.provider.message.as_deref().map(str::trim) {
            Some(message) if !message.is_empty() => format!("{label} — {kind}. {message}"),
            _ => format!("{label} — {kind}."),
        }
    }
}

/// Entries in server order.
pub fn provider_entries(providers: &[ServerProvider]) -> Vec<ProviderEntry<'_>> {
    providers
        .iter()
        .map(|provider| ProviderEntry {
            provider,
            is_default: provider.instance_id.as_str() == provider.driver,
        })
        .collect()
}

/// Brand label of a driver (`PROVIDER_DISPLAY_NAMES`), else the kind title-cased.
pub fn driver_label(driver: &str) -> String {
    match driver {
        "codex" => "Codex".into(),
        "claudeAgent" => "Claude".into(),
        "cursor" => "Cursor".into(),
        "grok" => "Grok".into(),
        "opencode" => "OpenCode".into(),
        other => humanize_instance_id(other),
    }
}

/// `codex_personal` → "Codex Personal", `myInstance` → "My Instance".
fn humanize_instance_id(id: &str) -> String {
    let mut spaced = String::with_capacity(id.len() + 4);
    let mut previous_lower = false;
    for ch in id.chars() {
        if ch == '_' || ch == '-' {
            spaced.push(' ');
            previous_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && previous_lower {
            spaced.push(' ');
        }
        previous_lower = ch.is_ascii_lowercase();
        spaced.push(ch);
    }
    spaced
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Two-letter initials for unknown drivers and instance badges (`providerInstanceInitials`).
pub fn instance_initials(label: &str) -> String {
    let words: Vec<&str> = label
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|word| !word.is_empty())
        .collect();
    match words.as_slice() {
        [] => String::new(),
        [word] => word.chars().take(2).collect::<String>().to_uppercase(),
        [first, second, ..] => [first, second]
            .iter()
            .filter_map(|word| word.chars().next())
            .collect::<String>()
            .to_uppercase(),
    }
}

/// A model's label: `shortName` when preferred, minus a leading `subProvider` qualifier
/// (`getDisplayModelName`).
pub fn model_display_name(model: &ServerProviderModel, prefer_short: bool) -> String {
    let name = match (&model.short_name, prefer_short) {
        (Some(short), true) => short.as_str(),
        _ => model.name.as_str(),
    };
    let Some(qualifier) = model
        .sub_provider
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return name.to_owned();
    };
    let lower = name.to_lowercase();
    if !lower.starts_with(&qualifier.to_lowercase()) || !name.is_char_boundary(qualifier.len()) {
        return name.to_owned();
    }
    let rest = &name[qualifier.len()..];
    let trimmed = rest.trim_start();
    let separated = trimmed.len() < rest.len();
    let after = match trimmed.chars().next() {
        Some(c @ ('.' | ':' | '/' | '-')) => trimmed[c.len_utf8()..].trim_start(),
        _ if separated => trimmed,
        _ => return name.to_owned(),
    };
    let stripped = after.trim();
    if stripped.is_empty() {
        name.to_owned()
    } else {
        stripped.to_owned()
    }
}

/// The model a slug (or a model name typed in a draft) resolves to in `models`
/// (`resolveSelectableModel`): exact slug, then case-insensitive name, then alias.
pub fn resolve_model<'a>(
    models: &'a [ServerProviderModel],
    value: &str,
) -> Option<&'a ServerProviderModel> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    models
        .iter()
        .find(|model| model.slug == value)
        .or_else(|| {
            models
                .iter()
                .find(|model| model.name.eq_ignore_ascii_case(value))
        })
        .or_else(|| {
            models.iter().find(|model| {
                model
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(value))
            })
        })
}

/// What the composer's model resolution reads.
#[derive(Clone, Debug, Default)]
pub struct ModelContext<'a> {
    /// `activeProvider` saved in the draft.
    pub draft_active_instance: Option<&'a ProviderInstanceId>,
    /// Draft selections keyed by instance.
    pub draft_selections: &'a [ModelSelection],
    /// The live session's instance.
    pub session_instance: Option<&'a ProviderInstanceId>,
    /// The thread's persisted selection.
    pub thread_selection: Option<&'a ModelSelection>,
    /// The project's default selection.
    pub project_selection: Option<&'a ModelSelection>,
    /// Sticky last-picked selections across drafts, keyed by instance.
    pub sticky_selections: &'a [ModelSelection],
    /// Sticky last-picked instance.
    pub sticky_instance: Option<&'a ProviderInstanceId>,
}

/// The resolved composer target: which instance, which model, and the option picks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedModel {
    pub instance_id: ProviderInstanceId,
    /// Index of the instance in the provider list, when it exists.
    pub provider_index: Option<usize>,
    pub model: String,
    /// The option picks as the composer last saved them (may be partial).
    pub options: Vec<ProviderOptionSelection>,
}

/// Resolves the composer's instance and model (`ChatComposer.tsx:556-619` and
/// `deriveEffectiveComposerModelState`).
///
/// Instance: the draft's pick, then the session's, the thread's, the project default's, the
/// sticky pick, each only if that instance is enabled; then the first enabled instance; then
/// `codex`. Model: the draft's saved model for the instance, then the thread / project / sticky
/// model, each resolved against the instance's models, then the instance's first model.
pub fn resolve_model_selection(
    providers: &[ServerProvider],
    context: &ModelContext<'_>,
) -> ResolvedModel {
    let entries = provider_entries(providers);
    let enabled = |id: &ProviderInstanceId| {
        entries
            .iter()
            .position(|entry| entry.instance_id() == id && entry.enabled())
    };
    let candidates = [
        context.draft_active_instance,
        context.session_instance,
        context
            .thread_selection
            .map(|selection| &selection.instance_id),
        context
            .project_selection
            .map(|selection| &selection.instance_id),
        context.sticky_instance,
    ];
    let provider_index = candidates
        .into_iter()
        .flatten()
        .find_map(enabled)
        .or_else(|| entries.iter().position(ProviderEntry::enabled))
        .or_else(|| (!entries.is_empty()).then_some(0));
    let instance_id = provider_index.map_or_else(
        || {
            context
                .draft_active_instance
                .or(context.thread_selection.map(|s| &s.instance_id))
                .cloned()
                .unwrap_or_else(|| ProviderInstanceId::from(FALLBACK_INSTANCE))
        },
        |index| entries[index].instance_id().clone(),
    );
    let models = provider_index.map_or(&[][..], |index| entries[index].models());

    let saved = |list: &[ModelSelection]| {
        list.iter()
            .find(|selection| selection.instance_id == instance_id)
            .cloned()
    };
    let draft = saved(context.draft_selections);
    let same_instance = |selection: Option<&ModelSelection>| {
        selection
            .filter(|selection| selection.instance_id == instance_id)
            .cloned()
    };
    let base = [
        draft.clone(),
        same_instance(context.thread_selection),
        same_instance(context.project_selection),
        saved(context.sticky_selections),
        context.thread_selection.cloned(),
        context.project_selection.cloned(),
    ];
    let mut chosen: Option<(String, Vec<ProviderOptionSelection>)> = None;
    for selection in base.into_iter().flatten() {
        if let Some(model) = resolve_model(models, &selection.model) {
            let options = if selection.instance_id == instance_id {
                selection.options.clone()
            } else {
                Vec::new()
            };
            chosen = Some((model.slug.clone(), options));
            break;
        }
    }
    let (model, options) = chosen.unwrap_or_else(|| {
        let fallback = models
            .first()
            .map(|model| model.slug.clone())
            .or_else(|| context.thread_selection.map(|s| s.model.clone()))
            .unwrap_or_default();
        (fallback, Vec::new())
    });
    ResolvedModel {
        instance_id,
        provider_index,
        model,
        options,
    }
}

/// The model's option descriptors with `selections` applied as current values
/// (`getProviderOptionDescriptors`). Unknown descriptor types are dropped.
pub fn option_descriptors(
    model: Option<&ServerProviderModel>,
    selections: &[ProviderOptionSelection],
) -> Vec<ProviderOptionDescriptor> {
    let Some(capabilities) = model.and_then(|model| model.capabilities.as_ref()) else {
        return Vec::new();
    };
    capabilities
        .option_descriptors
        .iter()
        .filter_map(|descriptor| {
            let mut descriptor = descriptor.clone();
            let picked = selections
                .iter()
                .find(|selection| Some(selection.id.as_str()) == descriptor_id(&descriptor));
            match (&mut descriptor, picked.map(|selection| &selection.value)) {
                (ProviderOptionDescriptor::Unknown, _) => return None,
                (
                    ProviderOptionDescriptor::Select {
                        current_value,
                        options,
                        ..
                    },
                    Some(ProviderOptionValue::String(value)),
                ) if options.iter().any(|option| &option.id == value) => {
                    *current_value = Some(value.clone());
                }
                (
                    ProviderOptionDescriptor::Boolean { current_value, .. },
                    Some(ProviderOptionValue::Bool(value)),
                ) => *current_value = Some(*value),
                _ => {}
            }
            Some(descriptor)
        })
        .collect()
}

/// A descriptor's id.
pub fn descriptor_id(descriptor: &ProviderOptionDescriptor) -> Option<&str> {
    match descriptor {
        ProviderOptionDescriptor::Select { id, .. }
        | ProviderOptionDescriptor::Boolean { id, .. } => Some(id),
        ProviderOptionDescriptor::Unknown => None,
    }
}

/// The current value of a select: its `currentValue`, else the default option
/// (`getProviderOptionCurrentValue`).
pub fn select_current<'a>(
    current_value: &'a Option<String>,
    options: &'a [ProviderOptionChoice],
) -> Option<&'a str> {
    current_value.as_deref().or_else(|| {
        options
            .iter()
            .find(|option| option.is_default)
            .map(|o| o.id.as_str())
    })
}

/// Every descriptor's current value as dispatchable picks
/// (`buildProviderOptionSelectionsFromDescriptors`).
pub fn selections_from_descriptors(
    descriptors: &[ProviderOptionDescriptor],
) -> Vec<ProviderOptionSelection> {
    descriptors
        .iter()
        .filter_map(|descriptor| match descriptor {
            ProviderOptionDescriptor::Select {
                id,
                current_value,
                options,
                ..
            } => select_current(current_value, options).map(|value| ProviderOptionSelection {
                id: id.clone(),
                value: ProviderOptionValue::String(value.to_owned()),
            }),
            ProviderOptionDescriptor::Boolean {
                id, current_value, ..
            } => current_value.map(|value| ProviderOptionSelection {
                id: id.clone(),
                value: ProviderOptionValue::Bool(value),
            }),
            ProviderOptionDescriptor::Unknown => None,
        })
        .collect()
}

/// Whether a select descriptor is the reasoning effort (`isReasoningDescriptor`).
pub fn is_reasoning(id: &str, label: &str) -> bool {
    id == "effort" || label.eq_ignore_ascii_case("reasoning")
}

/// The label of an option in the traits menu and trigger (`formatTraitsOptionLabel`): reasoning
/// `low` reads "Light".
pub fn trait_option_label(
    descriptor_id: &str,
    descriptor_label: &str,
    option: &ProviderOptionChoice,
) -> String {
    if is_reasoning(descriptor_id, descriptor_label) {
        match option.id.as_str() {
            "low" => return "Light".into(),
            "max" => return "Max".into(),
            "ultra" => return "Ultra".into(),
            _ => {}
        }
    }
    option.label.clone()
}

/// One part of the traits trigger label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitLabelPart {
    pub label: String,
    /// Reasoning "Ultra" renders purple.
    pub ultra: bool,
}

/// The traits trigger label parts, joined with " · " (`TraitsPicker` trigger).
/// `ultrathink` is true when the prompt controls the primary effort.
pub fn traits_label(
    descriptors: &[ProviderOptionDescriptor],
    ultrathink: bool,
) -> Vec<TraitLabelPart> {
    let primary = descriptors
        .iter()
        .find(|descriptor| matches!(descriptor, ProviderOptionDescriptor::Select { .. }))
        .and_then(descriptor_id);
    descriptors
        .iter()
        .filter_map(|descriptor| {
            let part = match descriptor {
                ProviderOptionDescriptor::Select { id, .. }
                    if ultrathink && Some(id.as_str()) == primary =>
                {
                    TraitLabelPart {
                        label: "Ultrathink".into(),
                        ultra: false,
                    }
                }
                ProviderOptionDescriptor::Select {
                    id,
                    label,
                    options,
                    current_value,
                    ..
                } => {
                    let current = select_current(current_value, options)?;
                    let option = options.iter().find(|option| option.id == current)?;
                    TraitLabelPart {
                        label: trait_option_label(id, label, option),
                        ultra: is_reasoning(id, label) && current == "ultra",
                    }
                }
                ProviderOptionDescriptor::Boolean {
                    id,
                    label,
                    current_value,
                    ..
                } => TraitLabelPart {
                    label: if id == "fastMode" {
                        if *current_value == Some(true) {
                            "Fast"
                        } else {
                            "Normal"
                        }
                        .into()
                    } else {
                        format!(
                            "{label} {}",
                            if *current_value == Some(true) {
                                "On"
                            } else {
                                "Off"
                            }
                        )
                    },
                    ultra: false,
                },
                ProviderOptionDescriptor::Unknown => return None,
            };
            (!part.label.is_empty()).then_some(part)
        })
        .collect()
}

/// `descriptors` with one value replaced (`replaceDescriptorCurrentValue`).
pub fn with_descriptor_value(
    descriptors: &[ProviderOptionDescriptor],
    target: &str,
    value: &ProviderOptionValue,
) -> Vec<ProviderOptionDescriptor> {
    descriptors
        .iter()
        .map(|descriptor| {
            let mut descriptor = descriptor.clone();
            match (&mut descriptor, value) {
                (
                    ProviderOptionDescriptor::Select {
                        id, current_value, ..
                    },
                    ProviderOptionValue::String(v),
                ) if id == target => {
                    *current_value = Some(v.clone());
                }
                (
                    ProviderOptionDescriptor::Boolean {
                        id, current_value, ..
                    },
                    ProviderOptionValue::Bool(v),
                ) if id == target => {
                    *current_value = Some(*v);
                }
                _ => {}
            }
            descriptor
        })
        .collect()
}

/// A model-picker row.
#[derive(Clone, Debug)]
pub struct PickerModel<'a> {
    pub entry: ProviderEntry<'a>,
    pub model: &'a ServerProviderModel,
    pub favorite: bool,
}

impl PickerModel<'_> {
    /// `<instance>:<slug>`, the row identity.
    pub fn key(&self) -> String {
        format!("{}:{}", self.entry.instance_id(), self.model.slug)
    }
}

/// Which rail item is selected in the model picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickerRail {
    Favorites,
    Instance(ProviderInstanceId),
}

/// Initial rail selection: favorites when there are any, else the active instance.
pub fn initial_rail(favorites: &[ModelFavorite], active: &ProviderInstanceId) -> PickerRail {
    if favorites.is_empty() {
        PickerRail::Instance(active.clone())
    } else {
        PickerRail::Favorites
    }
}

/// The rows the model picker lists (`ModelPickerContent` `filteredModels`).
///
/// Only picker-ready instances contribute. With a query, every instance's models are ranked by
/// [`score_model`] (favorites, then the combined text, break ties). Without one, the rail
/// selection filters, favorites float to the top of an instance list, and the favorites view
/// keeps instance order.
pub fn picker_models<'a>(
    providers: &'a [ServerProvider],
    favorites: &[ModelFavorite],
    rail: &PickerRail,
    query: &str,
) -> Vec<PickerModel<'a>> {
    let favorite_keys: BTreeSet<(&str, &str)> = favorites
        .iter()
        .map(|favorite| (favorite.provider.as_str(), favorite.model.as_str()))
        .collect();
    let all: Vec<PickerModel<'a>> = provider_entries(providers)
        .into_iter()
        .filter(ProviderEntry::picker_ready)
        .flat_map(|entry| {
            let favorite_keys = &favorite_keys;
            entry.models().iter().map(move |model| PickerModel {
                entry,
                model,
                favorite: favorite_keys
                    .contains(&(entry.instance_id().as_str(), model.slug.as_str())),
            })
        })
        .collect();

    if !query.trim().is_empty() {
        let mut ranked: Vec<(i64, bool, String, PickerModel<'a>)> = all
            .into_iter()
            .filter_map(|row| {
                let display = row.entry.display_name();
                let fields = ModelSearchFields {
                    name: &row.model.name,
                    short_name: row.model.short_name.as_deref(),
                    sub_provider: row.model.sub_provider.as_deref(),
                    driver: row.entry.driver(),
                    provider_display_name: &display,
                    is_favorite: row.favorite,
                };
                let score = score_model(&fields, query)?;
                let tie = fields.search_text();
                Some((score, row.favorite, tie, row))
            })
            .collect();
        ranked.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        return ranked.into_iter().map(|(_, _, _, row)| row).collect();
    }

    match rail {
        PickerRail::Favorites => all.into_iter().filter(|row| row.favorite).collect(),
        PickerRail::Instance(instance) => {
            let mut rows: Vec<PickerModel<'a>> = all
                .into_iter()
                .filter(|row| row.entry.instance_id() == instance)
                .collect();
            // Stable: favorites first, server order otherwise.
            rows.sort_by_key(|row| !row.favorite);
            rows
        }
    }
}

/// Toggles `(instance, model)` in the favorites list.
pub fn toggle_favorite(favorites: &mut Vec<ModelFavorite>, instance: &str, model: &str) {
    if let Some(index) = favorites
        .iter()
        .position(|favorite| favorite.provider == instance && favorite.model == model)
    {
        favorites.remove(index);
    } else {
        favorites.push(ModelFavorite {
            provider: instance.to_owned(),
            model: model.to_owned(),
        });
    }
}
