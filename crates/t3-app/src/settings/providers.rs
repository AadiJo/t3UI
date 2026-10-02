//! Providers page (`SettingsPanels.tsx:1071-1514`, `ProviderInstanceCard.tsx`, spec 3.6).
//!
//! One row per provider instance: the default instance of each fork driver (Codex, Claude,
//! Cursor when the server reports it, Grok, OpenCode), then custom instances. The header shows
//! the live status (dot, authentication, version); the collapsible body edits the display name,
//! accent color, the driver's settings fields (upstream's field list), and lists models.
//!
//! Writes follow `buildProviderInstanceUpdatePatch`: the whole `providerInstances` map is sent,
//! and editing a default instance resets its legacy `providers[driver]` blob to the default.

use std::collections::{BTreeMap, HashMap, HashSet};

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, Window,
    component::input::{InputEvent, InputState},
    div,
    prelude::FluentBuilder as _,
    px,
};
use serde_json::{Map, Value, json};
use t3_logic::time::format_relative_time;
use t3_protocol::{
    methods::ServerRefreshProviders,
    server::{
        ProviderAuthStatus, ProviderInstanceConfig, ProviderStatus, RefreshProvidersInput,
        ServerConfig, ServerProvider, ServerSettingsPatch,
    },
};
use t3_ui::{
    ActiveColors as _, Badge, BadgeSize, BadgeVariant, Button, ButtonSize, ButtonVariant, Colors,
    Icon, IconName, Input, Logo, Switch, TooltipExt as _, logo,
    tokens::{hex, text},
};

use super::{
    layout::{PAGE_MAX_WIDTH, SettingsSection, page, reset_button},
    server::{primary, primary_config, update_server_settings},
};
use crate::{
    chrome::{TypeScale as _, under_xs},
    state::AppState,
};

/// `bg-amber-400`, the dot of a disabled provider (`tokens.json` palette).
const AMBER_400: Hsla = hex(0xFFB900FF);
/// The accent color presets (`ProviderAccentColorPicker.tsx`).
const ACCENT_PRESETS: [&str; 6] = [
    "#2563eb", "#16a34a", "#ea580c", "#dc2626", "#7c3aed", "#0891b2",
];

/// A driver the client knows: label, logo, badge, settings fields, legacy defaults.
struct Driver {
    id: &'static str,
    label: &'static str,
    logo: Logo,
    badge: Option<&'static str>,
    fields: &'static [Field],
    /// Upstream's decoding defaults for `providers[driver]`.
    legacy_default: fn() -> Value,
}

/// One settings field (upstream schema annotations).
struct Field {
    key: &'static str,
    title: &'static str,
    description: &'static str,
    placeholder: &'static str,
    password: bool,
}

const fn field(
    key: &'static str,
    title: &'static str,
    description: &'static str,
    placeholder: &'static str,
) -> Field {
    Field {
        key,
        title,
        description,
        placeholder,
        password: false,
    }
}

const DRIVERS: [Driver; 5] = [
    Driver {
        id: "codex",
        label: "Codex",
        logo: Logo::OpenAI,
        badge: None,
        fields: &[
            field(
                "binaryPath",
                "Binary path",
                "Path to the Codex binary used by this instance.",
                "codex",
            ),
            field(
                "homePath",
                "CODEX_HOME path",
                "Custom Codex home and config directory.",
                "~/.codex",
            ),
            field(
                "shadowHomePath",
                "Shadow home path",
                "Account-specific Codex home. Keeps auth.json separate while sharing state from CODEX_HOME.",
                "~/.codex-t3/personal",
            ),
            field(
                "launchArgs",
                "Launch arguments",
                "Additional CLI arguments passed to codex app-server on session start.",
                "",
            ),
        ],
        legacy_default: || json!({"enabled": true, "binaryPath": "codex", "homePath": "", "shadowHomePath": "", "launchArgs": "", "customModels": []}),
    },
    Driver {
        id: "claudeAgent",
        label: "Claude",
        logo: Logo::ClaudeAI,
        badge: None,
        fields: &[
            field(
                "binaryPath",
                "Binary path",
                "Path to the Claude binary used by this instance.",
                "claude",
            ),
            field(
                "homePath",
                "CLAUDE_CONFIG_DIR path",
                "Custom Claude home and config directory. Keeps .claude.json and .claude separate.",
                "~/.claude",
            ),
            field(
                "autoCompactWindow",
                "Auto-compact after",
                "Compact after 100,000 to 1,000,000 tokens. Leave empty to use Claude's default.",
                "e.g. 300000",
            ),
            field(
                "launchArgs",
                "Launch arguments",
                "Additional CLI arguments passed on session start.",
                "e.g. --chrome",
            ),
        ],
        legacy_default: || json!({"enabled": true, "binaryPath": "claude", "homePath": "", "autoCompactWindow": "", "launchArgs": "", "customModels": []}),
    },
    Driver {
        id: "cursor",
        label: "Cursor",
        logo: Logo::CursorIcon,
        badge: Some("Early Access"),
        fields: &[
            field(
                "binaryPath",
                "Binary path",
                "Path to the Cursor agent binary.",
                "cursor-agent",
            ),
            field(
                "apiEndpoint",
                "API endpoint",
                "Override the Cursor API endpoint for this instance.",
                "https://...",
            ),
        ],
        legacy_default: || json!({"enabled": false, "binaryPath": "cursor-agent", "apiEndpoint": "", "customModels": []}),
    },
    Driver {
        id: "grok",
        label: "Grok",
        logo: Logo::GrokIcon,
        badge: Some("Early Access"),
        fields: &[field(
            "binaryPath",
            "Binary path",
            "Path to the Grok CLI binary.",
            "grok",
        )],
        legacy_default: || json!({"enabled": false, "binaryPath": "grok", "customModels": []}),
    },
    Driver {
        id: "opencode",
        label: "OpenCode",
        logo: Logo::OpenCodeIcon,
        badge: None,
        fields: &[
            field(
                "binaryPath",
                "Binary path",
                "Path to the OpenCode binary.",
                "opencode",
            ),
            field(
                "serverUrl",
                "Server URL",
                "Leave blank to let T3 Code spawn the server when needed.",
                "http://127.0.0.1:4096",
            ),
            Field {
                key: "serverPassword",
                title: "Server password",
                description: "Stored in plain text on disk.",
                placeholder: "Optional",
                password: true,
            },
        ],
        legacy_default: || json!({"enabled": false, "binaryPath": "opencode", "serverUrl": "", "serverPassword": "", "customModels": []}),
    },
];

fn driver(id: &str) -> Option<&'static Driver> {
    DRIVERS.iter().find(|driver| driver.id == id)
}

/// One card's data.
#[derive(Clone)]
struct Row {
    instance_id: String,
    instance: ProviderInstanceConfig,
    driver: String,
    is_default: bool,
    dirty: bool,
}

fn legacy_providers(config: &ServerConfig) -> Map<String, Value> {
    config
        .settings
        .other
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Builds the rows (`SettingsPanels.tsx:1191-1264`).
fn build_rows(config: &ServerConfig) -> Vec<Row> {
    let instances = config
        .settings
        .provider_instances
        .clone()
        .unwrap_or_default();
    let legacy = legacy_providers(config);
    let reports_cursor = config
        .providers
        .iter()
        .any(|provider| provider.instance_id.as_str() == "cursor");
    let visible: Vec<&Driver> = DRIVERS
        .iter()
        .filter(|driver| driver.id != "cursor" || reports_cursor)
        .collect();
    let mut rows = Vec::new();
    for driver in &visible {
        let explicit = instances.get(driver.id);
        let legacy_config = legacy
            .get(driver.id)
            .cloned()
            .unwrap_or_else(driver.legacy_default);
        let instance = explicit.cloned().unwrap_or_else(|| ProviderInstanceConfig {
            driver: driver.id.to_owned(),
            display_name: None,
            accent_color: None,
            environment: None,
            enabled: legacy_config.get("enabled").and_then(Value::as_bool),
            config: Some(legacy_config.clone()),
        });
        let dirty = explicit.is_some() || !legacy_matches_default(&legacy_config, driver);
        rows.push(Row {
            instance_id: driver.id.to_owned(),
            instance,
            driver: driver.id.to_owned(),
            is_default: true,
            dirty,
        });
        for (id, instance) in &instances {
            if id != driver.id && instance.driver == driver.id {
                rows.push(Row {
                    instance_id: id.clone(),
                    instance: instance.clone(),
                    driver: instance.driver.clone(),
                    is_default: false,
                    dirty: false,
                });
            }
        }
    }
    for (id, instance) in &instances {
        if visible.iter().any(|driver| driver.id == instance.driver) {
            continue;
        }
        rows.push(Row {
            instance_id: id.clone(),
            instance: instance.clone(),
            driver: instance.driver.clone(),
            is_default: visible.iter().any(|driver| driver.id == id),
            dirty: false,
        });
    }
    rows
}

/// The server fills in defaults it knows; compare only the keys upstream's default has.
fn legacy_matches_default(value: &Value, driver: &Driver) -> bool {
    let default = (driver.legacy_default)();
    let Some(default) = default.as_object() else {
        return true;
    };
    default
        .iter()
        .all(|(key, expected)| value.get(key).is_none_or(|actual| actual == expected))
}

/// Status dot color (`providerStatus.ts:7-20`).
fn status_dot(colors: &Colors, provider: Option<&ServerProvider>, enabled: bool) -> Hsla {
    match provider.map(|provider| &provider.status) {
        Some(ProviderStatus::Ready) => colors.success,
        Some(ProviderStatus::Error) => colors.destructive,
        Some(ProviderStatus::Warning) => colors.warning,
        Some(ProviderStatus::Disabled) => AMBER_400,
        _ if enabled => colors.warning,
        _ => AMBER_400,
    }
}

/// Headline and detail under the name (`getProviderSummary`).
fn summary(provider: Option<&ServerProvider>) -> (String, Option<String>) {
    let Some(provider) = provider else {
        return (
            "Checking provider status".into(),
            Some(
                "Waiting for the server to report installation and authentication details.".into(),
            ),
        );
    };
    let message = provider.message.clone();
    let or = |fallback: &str| message.clone().or_else(|| Some(fallback.to_owned()));
    if !provider.enabled {
        return (
            "Disabled".into(),
            or("This provider is installed but disabled for new sessions in T3 Code."),
        );
    }
    if !provider.installed {
        return ("Not found".into(), or("CLI not detected on PATH."));
    }
    match provider.auth.status {
        ProviderAuthStatus::Authenticated => {
            let label = provider.auth.label.clone().or(provider.auth.kind.clone());
            return (
                label.map_or("Authenticated".into(), |label| {
                    format!("Authenticated · {label}")
                }),
                message,
            );
        }
        ProviderAuthStatus::Unauthenticated => return ("Not authenticated".into(), message),
        _ => {}
    }
    match provider.status {
        ProviderStatus::Warning => (
            "Needs attention".into(),
            or("The provider is installed, but the server could not fully verify it."),
        ),
        ProviderStatus::Error => (
            "Unavailable".into(),
            or("The provider failed its startup checks."),
        ),
        _ => (
            "Available".into(),
            or("Installed and ready, but authentication could not be verified."),
        ),
    }
}

/// `redactedPlaceholder`: a deterministic scramble that keeps `@ . - _`.
fn redacted(value: &str) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut state: u32 = 0x811c9dc5;
    for unit in value.encode_utf16() {
        state ^= u32::from(unit);
        state = state.wrapping_mul(0x01000193);
    }
    let mut next = || {
        state = (state ^ (state >> 13)).wrapping_mul(0x85ebca6b);
        state = (state ^ (state >> 16)).wrapping_mul(0xc2b2ae35);
        let signed = state as i32;
        ALPHABET[(signed.unsigned_abs() as usize) % ALPHABET.len()] as char
    };
    value
        .chars()
        .map(|char| match char {
            '@' | '.' | '-' | '_' => char,
            _ => next(),
        })
        .collect()
}

/// `v1.2.3` for a bare version.
fn version_label(version: &str) -> String {
    let version = version.trim();
    if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    }
}

fn instances_value(instances: &BTreeMap<String, ProviderInstanceConfig>) -> Value {
    serde_json::to_value(instances).unwrap_or(Value::Object(Map::new()))
}

/// Sends `providerInstances[id] = next` (and resets the legacy blob of a default instance).
fn write_instance(config: &ServerConfig, row: &Row, next: ProviderInstanceConfig, cx: &mut App) {
    let mut instances = config
        .settings
        .provider_instances
        .clone()
        .unwrap_or_default();
    instances.insert(row.instance_id.clone(), next.clone());
    let mut patch = ServerSettingsPatch::default();
    if row.is_default
        && let Some(driver) = driver(&row.driver)
    {
        let mut providers = legacy_providers(config);
        providers.insert(driver.id.to_owned(), (driver.legacy_default)());
        patch
            .other
            .insert("providers".into(), Value::Object(providers));
    }
    // Disabling the text generation instance resets that selection to its default.
    let text_instance = config
        .settings
        .text_generation_model_selection
        .as_ref()
        .map_or("codex", |selection| selection.instance_id.as_str());
    let disabling = row.instance.enabled.unwrap_or(true) && next.enabled == Some(false);
    if disabling && text_instance == row.instance_id {
        patch.other.insert(
            "textGenerationModelSelection".into(),
            json!({"instanceId": "codex", "model": "gpt-6-luna", "options": [{"id": "reasoningEffort", "value": "low"}]}),
        );
    }
    patch
        .other
        .insert("providerInstances".into(), instances_value(&instances));
    update_server_settings(patch, cx);
}

/// Reset of a default instance: legacy blob to default, explicit instance removed.
fn reset_default(config: &ServerConfig, row: &Row, cx: &mut App) {
    let Some(driver) = driver(&row.driver) else {
        return;
    };
    let mut instances = config
        .settings
        .provider_instances
        .clone()
        .unwrap_or_default();
    instances.remove(&row.instance_id);
    let mut providers = legacy_providers(config);
    providers.insert(driver.id.to_owned(), (driver.legacy_default)());
    let mut patch = ServerSettingsPatch::default();
    patch
        .other
        .insert("providers".into(), Value::Object(providers));
    patch
        .other
        .insert("providerInstances".into(), instances_value(&instances));
    update_server_settings(patch, cx);
}

/// Deletes a custom instance (`deleteProviderInstance`).
fn delete_instance(config: &ServerConfig, id: &str, cx: &mut App) {
    let mut instances = config
        .settings
        .provider_instances
        .clone()
        .unwrap_or_default();
    instances.remove(id);
    let mut patch = ServerSettingsPatch::default();
    patch
        .other
        .insert("providerInstances".into(), instances_value(&instances));
    update_server_settings(patch, cx);
    AppState::global(cx).update(cx, |state, cx| {
        state.update_settings(
            |settings| {
                settings.provider_model_preferences.remove(id);
                settings
                    .favorites
                    .retain(|favorite| favorite.provider != id);
            },
            cx,
        )
    });
}

/// Applies `edit` to an instance's current config and writes it when `edit` changed it.
fn commit_instance(
    instance_id: &str,
    cx: &mut App,
    edit: impl FnOnce(&mut ProviderInstanceConfig) -> bool,
) {
    let Some(config) = primary_config(cx) else {
        return;
    };
    let Some(row) = build_rows(&config)
        .into_iter()
        .find(|row| row.instance_id == instance_id)
    else {
        return;
    };
    let mut next = row.instance.clone();
    if edit(&mut next) {
        write_instance(&config, &row, next, cx);
    }
}

/// `#rrggbb` to a color.
fn parse_hex(value: &str) -> Option<Hsla> {
    let digits = value.strip_prefix('#')?;
    if digits.len() != 6 {
        return None;
    }
    u32::from_str_radix(digits, 16)
        .ok()
        .map(|rgb| hex((rgb << 8) | 0xFF))
}

/// Text inputs of one expanded card, created when it first opens.
struct CardInputs {
    display_name: Entity<InputState>,
    fields: Vec<(&'static str, Entity<InputState>)>,
    _subscriptions: Vec<Subscription>,
}

/// The Providers page.
pub struct ProvidersPage {
    app_state: Entity<AppState>,
    expanded: HashSet<String>,
    inputs: HashMap<String, CardInputs>,
    revealed_emails: HashSet<String>,
    refreshing: bool,
    _tick: Task<()>,
    _app_state: Subscription,
}

impl ProvidersPage {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let live_clock = app_state.read(cx).clock_is_live();
        // "Checked 3m ago" re-renders every second (`useRelativeTimeTick`).
        let tick = cx.spawn(async move |this, cx| {
            if !live_clock {
                return;
            }
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        Self {
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
            app_state,
            expanded: HashSet::new(),
            inputs: HashMap::new(),
            revealed_emails: HashSet::new(),
            refreshing: false,
            _tick: tick,
        }
    }

    /// Opens or closes a card's details (also used by snapshot scenes).
    pub fn set_expanded(
        &mut self,
        instance_id: &str,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if open {
            self.expanded.insert(instance_id.to_owned());
            self.ensure_inputs(instance_id, window, cx);
        } else {
            self.expanded.remove(instance_id);
        }
        cx.notify();
    }

    fn ensure_inputs(&mut self, instance_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.inputs.contains_key(instance_id) {
            return;
        }
        let Some(config) = primary_config(cx) else {
            return;
        };
        let Some(row) = build_rows(&config)
            .into_iter()
            .find(|row| row.instance_id == instance_id)
        else {
            return;
        };
        let label = driver(&row.driver).map_or(row.driver.clone(), |driver| driver.label.into());
        let display_name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(label)
                .default_value(row.instance.display_name.clone().unwrap_or_default())
        });
        let mut subscriptions = Vec::new();
        let id = instance_id.to_owned();
        subscriptions.push(cx.subscribe_in(
            &display_name,
            window,
            move |_, input, event, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    let value = input.read(cx).value().trim().to_owned();
                    let next = (!value.is_empty()).then_some(value);
                    commit_instance(&id, cx, |instance| {
                        let changed = instance.display_name != next;
                        instance.display_name = next;
                        changed
                    });
                }
            },
        ));
        let mut fields = Vec::new();
        for field in driver(&row.driver).map_or(&[][..], |driver| driver.fields) {
            let current = row
                .instance
                .config
                .as_ref()
                .and_then(|config| config.get(field.key))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(field.placeholder)
                    .masked(field.password)
                    .default_value(current)
            });
            let id = instance_id.to_owned();
            let key = field.key;
            subscriptions.push(
                cx.subscribe_in(&input, window, move |_, input, event, _, cx| {
                    if !matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        return;
                    }
                    let value = input.read(cx).value().trim().to_owned();
                    commit_instance(&id, cx, |instance| {
                        let mut config = instance
                            .config
                            .clone()
                            .and_then(|config| config.as_object().cloned())
                            .unwrap_or_default();
                        let before = config.get(key).cloned();
                        // Empty strings are omitted (`clearWhenEmpty: "omit"`).
                        if value.is_empty() {
                            config.remove(key);
                        } else {
                            config.insert(key.to_owned(), Value::String(value.clone()));
                        }
                        let changed = config.get(key) != before.as_ref();
                        instance.config = Some(Value::Object(config));
                        changed
                    });
                }),
            );
            fields.push((field.key, input));
        }
        self.inputs.insert(
            instance_id.to_owned(),
            CardInputs {
                display_name,
                fields,
                _subscriptions: subscriptions,
            },
        );
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(client) =
            primary(cx).and_then(|environment| environment.read(cx).client().cloned())
        else {
            return;
        };
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client
                .request::<ServerRefreshProviders>(&RefreshProvidersInput::default())
                .await;
            if let Err(error) = result {
                tracing::warn!("failed to refresh providers: {error}");
            }
            this.update(cx, |this, cx| {
                this.refreshing = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_card(&self, config: &ServerConfig, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.colors();
        let mono = t3_ui::theme::Theme::global(cx).mono_family().clone();
        let meta = driver(&row.driver);
        let live = config
            .providers
            .iter()
            .find(|provider| provider.instance_id.as_str() == row.instance_id);
        let enabled = row.instance.enabled.unwrap_or(true);
        let display_name: SharedString = row
            .instance
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| meta.map(|driver| driver.label.to_owned()))
            .unwrap_or_else(|| row.driver.clone())
            .into();
        let expanded = self.expanded.contains(&row.instance_id);
        let icon = div()
            .relative()
            .size_5()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .map(|this| match meta {
                Some(meta) => this.child(logo(meta.logo, colors.is_dark, px(16.)).opacity(0.8)),
                None => this.child(
                    div()
                        .type_scale(under_xs(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(display_name.chars().take(2).collect::<String>()),
                ),
            })
            // Status dot: 8px at (-2, -2) with a 2px `card` ring.
            .child(
                div()
                    .absolute()
                    .top(px(-4.))
                    .left(px(-4.))
                    .size(px(12.))
                    .rounded_full()
                    .border_2()
                    .border_color(colors.card)
                    .bg(status_dot(colors, live, enabled)),
            );
        let reset = (row.is_default && row.dirty).then(|| {
            let config = config.clone();
            let row = row.clone();
            reset_button(
                SharedString::from(format!("reset-provider-{}", row.instance_id)),
                colors,
                move |_, cx| reset_default(&config, &row, cx),
            )
        });
        let delete = (!row.is_default).then(|| {
            let config = config.clone();
            let id = row.instance_id.clone();
            let destructive = colors.destructive;
            div()
                .id(SharedString::from(format!(
                    "delete-provider-{}",
                    row.instance_id
                )))
                .size_5()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(colors.muted_foreground)
                .hover(move |style| style.text_color(destructive))
                .tooltip_text("Delete instance")
                .on_click(move |_, _, cx| delete_instance(&config, &id, cx))
                .child(Icon::new(IconName::Trash2).size(px(12.)))
        });
        let (headline, detail) = summary(live);
        let email = live
            .filter(|provider| provider.auth.status == ProviderAuthStatus::Authenticated)
            .and_then(|provider| provider.auth.email.clone())
            .filter(|email| !email.trim().is_empty());
        let auth_line = div()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x_1()
            .type_scale(text::XS)
            .text_color(colors.muted_foreground_80)
            .map(|this| match email {
                Some(email) => {
                    let label = live.and_then(|provider| {
                        provider.auth.label.clone().or(provider.auth.kind.clone())
                    });
                    let revealed = self.revealed_emails.contains(&row.instance_id);
                    let id = row.instance_id.clone();
                    let hover = colors.foreground;
                    this.child("Authenticated as")
                        .child(
                            div()
                                .id(SharedString::from(format!("email-{}", row.instance_id)))
                                .cursor_pointer()
                                .font_family(mono.clone())
                                .text_size(px(11.))
                                .line_height(px(11.))
                                .text_color(colors.muted_foreground)
                                .hover(move |style| style.text_color(hover))
                                .tooltip_text(if revealed {
                                    "Click to hide email"
                                } else {
                                    "Click to reveal email"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.revealed_emails.remove(&id) {
                                        this.revealed_emails.insert(id.clone());
                                    }
                                    cx.notify();
                                }))
                                .child(if revealed {
                                    email.clone()
                                } else {
                                    redacted(&email)
                                }),
                        )
                        .children(label.map(|label| format!("· {label}")))
                }
                None => this.child(headline),
            })
            .children(detail.map(|detail| format!("- {detail}")));
        let toggle_id = row.instance_id.clone();
        let switch = {
            let config = config.clone();
            let row = row.clone();
            Switch::new(SharedString::from(format!(
                "enable-provider-{}",
                row.instance_id
            )))
            .checked(enabled)
            .on_change(move |checked, _, cx| {
                let mut next = row.instance.clone();
                next.enabled = Some(*checked);
                write_instance(&config, &row, next, cx);
            })
        };
        let header = div()
            .px_5()
            .py(px(14.))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap_2()
                            .child(icon)
                            .child(
                                div()
                                    .truncate()
                                    .type_scale(under_xs(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(colors.foreground)
                                    .child(display_name.clone()),
                            )
                            .when(row.instance_id != row.driver, |this| {
                                this.child(
                                    div()
                                        .rounded(px(4.))
                                        .bg(colors.muted.opacity(0.6))
                                        .px_1()
                                        .py(px(2.))
                                        .type_scale(under_xs(10.))
                                        .text_color(colors.muted_foreground)
                                        .child(row.instance_id.clone()),
                                )
                            })
                            .children(meta.and_then(|meta| meta.badge).map(|badge| {
                                Badge::new(badge)
                                    .variant(BadgeVariant::Warning)
                                    .size(BadgeSize::Sm)
                            }))
                            .children(live.and_then(|provider| provider.version.as_deref()).map(
                                |version| {
                                    div()
                                        .font_family(mono.clone())
                                        .type_scale(text::XS)
                                        .text_color(colors.muted_foreground)
                                        .child(version_label(version))
                                },
                            ))
                            .children(reset.map(|reset| div().size_5().flex_none().child(reset)))
                            .children(delete),
                    )
                    .child(auth_line),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!(
                            "toggle-provider-{}",
                            row.instance_id
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconSm)
                        .icon(if expanded {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let open = !this.expanded.contains(&toggle_id);
                                this.set_expanded(&toggle_id, open, window, cx);
                            },
                        )),
                    )
                    .child(switch),
            );
        div()
            .flex()
            .flex_col()
            .child(header)
            .when(expanded, |this| {
                this.child(self.render_body(config, row, live, cx))
            })
            .into_any_element()
    }

    fn render_body(
        &self,
        config: &ServerConfig,
        row: &Row,
        live: Option<&ServerProvider>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.colors();
        let block = || {
            div()
                .border_t_1()
                .border_color(colors.border_60)
                .px_5()
                .py_3()
        };
        let label = |value: &'static str| {
            div()
                .type_scale(text::XS)
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.foreground)
                .child(value)
        };
        let help = |value: SharedString| {
            div()
                .mt_1()
                .type_scale(text::XS)
                .text_color(colors.muted_foreground)
                .child(value)
        };
        let Some(meta) = driver(&row.driver) else {
            return block()
                .child(help(
                    format!(
                        "This instance uses a driver ({}) that is not shipped with the current build. Configuration values are preserved but cannot be edited from this surface.",
                        row.driver
                    )
                    .into(),
                ))
                .into_any_element();
        };
        let inputs = self.inputs.get(&row.instance_id);
        let display_name = inputs.map(|inputs| {
            block()
                .child(label("Display name"))
                .child(div().mt(px(6.)).child(Input::new(&inputs.display_name)))
                .child(help("Optional label shown in the provider list.".into()))
        });
        let accent = row.instance.accent_color.clone();
        let swatches: Vec<_> = ACCENT_PRESETS
            .iter()
            .map(|preset| {
                let selected = accent.as_deref() == Some(*preset);
                let color = parse_hex(preset).unwrap_or(colors.primary);
                let config = config.clone();
                let row = row.clone();
                let preset = (*preset).to_owned();
                div()
                    .id(SharedString::from(format!(
                        "accent-{}-{preset}",
                        row.instance_id
                    )))
                    .size_6()
                    .rounded_full()
                    .cursor_pointer()
                    .bg(color)
                    .when(selected, |this| {
                        // Inset 2px `card` ring plus an outer 2px ring in the swatch color.
                        this.border_2().border_color(colors.card).shadow(vec![
                            gpui_kit::BoxShadow {
                                color,
                                offset: gpui_kit::point(px(0.), px(0.)),
                                blur_radius: px(0.),
                                spread_radius: px(2.),
                                inset: false,
                            },
                        ])
                    })
                    .on_click(move |_, _, cx| {
                        let mut next = row.instance.clone();
                        next.accent_color = Some(preset.clone());
                        write_instance(&config, &row, next, cx);
                    })
            })
            .collect();
        let clear_accent = accent.as_ref().map(|_| {
            let config = config.clone();
            let row = row.clone();
            Button::new(SharedString::from(format!(
                "accent-clear-{}",
                row.instance_id
            )))
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::IconSm)
            .icon(IconName::X)
            .on_click(move |_, _, cx| {
                let mut next = row.instance.clone();
                next.accent_color = None;
                write_instance(&config, &row, next, cx);
            })
        });
        let accent_block = block()
            .child(label("Accent color"))
            .child(
                div()
                    .mt_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size_6()
                            .rounded_full()
                            .border_1()
                            .border_color(colors.border)
                            .bg(accent
                                .as_deref()
                                .and_then(parse_hex)
                                .unwrap_or(colors.muted))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Pipette)
                                    .size(px(12.))
                                    .color(colors.foreground.opacity(0.25)),
                            ),
                    )
                    .children(swatches)
                    .children(clear_accent),
            )
            .child(help(
                "Used to distinguish this instance in picker rails and model lists.".into(),
            ));
        let environment_count = row.instance.environment.as_ref().map_or(0, Vec::len);
        let environment_block =
            block()
                .child(label("Environment variables"))
                .child(help(if environment_count == 0 {
                    "Add variables to pass API keys, base URLs, or other per-instance CLI settings."
                        .into()
                } else {
                    format!(
                        "{environment_count} variable{} configured.",
                        if environment_count == 1 { "" } else { "s" }
                    )
                    .into()
                }));
        let fields: Vec<_> = inputs
            .map(|inputs| {
                meta.fields
                    .iter()
                    .filter_map(|field| {
                        let input = inputs
                            .fields
                            .iter()
                            .find(|(key, _)| *key == field.key)
                            .map(|(_, input)| input)?;
                        Some(
                            block()
                                .child(label(field.title))
                                .child(div().mt(px(6.)).child(Input::new(input)))
                                .child(help(field.description.into())),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let models = live.map_or(&[][..], |provider| provider.models.as_slice());
        let state = self.app_state.read(cx);
        let hidden_models = state
            .settings()
            .provider_model_preferences
            .get(&row.instance_id)
            .map(|preferences| preferences.hidden_models.clone())
            .unwrap_or_default();
        let favorites: Vec<String> = state
            .settings()
            .favorites
            .iter()
            .filter(|favorite| favorite.provider == row.instance_id)
            .map(|favorite| favorite.model.clone())
            .collect();
        let model_rows: Vec<_> = models
            .iter()
            .map(|model| {
                let hidden = hidden_models.contains(&model.slug);
                let favorite = favorites.contains(&model.slug);
                let (instance, slug) = (row.instance_id.clone(), model.slug.clone());
                let (instance_fav, slug_fav) = (row.instance_id.clone(), model.slug.clone());
                let tag = |value: &'static str| {
                    div()
                        .type_scale(under_xs(10.))
                        .text_color(colors.muted_foreground)
                        .child(value)
                };
                div()
                    .min_h_7()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .type_scale(text::XS)
                            .map(|this| {
                                if hidden {
                                    this.text_color(colors.muted_foreground).line_through()
                                } else {
                                    this.text_color(colors.foreground.opacity(0.9))
                                }
                            })
                            .child(model.name.clone()),
                    )
                    .when(hidden, |this| this.child(tag("hidden")))
                    .when(model.is_custom, |this| this.child(tag("custom")))
                    .child(
                        Button::new(SharedString::from(format!(
                            "fav-{}-{}",
                            row.instance_id, model.slug
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .icon(IconName::Star)
                        .tooltip(if favorite {
                            "Remove from favorites"
                        } else {
                            "Add to favorites"
                        })
                        .on_click(move |_, _, cx| toggle_favorite(&instance_fav, &slug_fav, cx)),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "hide-{}-{}",
                            row.instance_id, model.slug
                        )))
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::IconXs)
                        .icon(if hidden {
                            IconName::EyeOff
                        } else {
                            IconName::Eye
                        })
                        .tooltip(if hidden {
                            "Show in picker"
                        } else {
                            "Hide from picker"
                        })
                        .on_click(move |_, _, cx| toggle_hidden(&instance, &slug, cx)),
                    )
            })
            .collect();
        let count = models.len();
        let models_block = block()
            .child(label("Models"))
            .child(help(
                format!(
                    "{count} model{} available.",
                    if count == 1 { "" } else { "s" }
                )
                .into(),
            ))
            .child(
                div()
                    .id(SharedString::from(format!("models-{}", row.instance_id)))
                    .mt_2()
                    .max_h(px(160.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(model_rows),
            );
        div()
            .flex()
            .flex_col()
            .children(display_name)
            .child(accent_block)
            .child(environment_block)
            .children(fields)
            .child(models_block)
            .into_any_element()
    }
}

fn toggle_favorite(instance: &str, slug: &str, cx: &mut App) {
    AppState::global(cx).update(cx, |state, cx| {
        state.update_settings(
            |settings| {
                let before = settings.favorites.len();
                settings
                    .favorites
                    .retain(|entry| !(entry.provider == instance && entry.model == slug));
                if settings.favorites.len() == before {
                    settings.favorites.push(t3_logic::settings::ModelFavorite {
                        provider: instance.to_owned(),
                        model: slug.to_owned(),
                    });
                }
            },
            cx,
        )
    });
}

fn toggle_hidden(instance: &str, slug: &str, cx: &mut App) {
    AppState::global(cx).update(cx, |state, cx| {
        state.update_settings(
            |settings| {
                let preferences = settings
                    .provider_model_preferences
                    .entry(instance.to_owned())
                    .or_default();
                match preferences
                    .hidden_models
                    .iter()
                    .position(|entry| entry == slug)
                {
                    Some(index) => {
                        preferences.hidden_models.remove(index);
                    }
                    None => preferences.hidden_models.push(slug.to_owned()),
                }
            },
            cx,
        )
    });
}

impl Render for ProvidersPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let Some(config) = primary_config(cx) else {
            return page(
                "settings-providers",
                PAGE_MAX_WIDTH,
                [SettingsSection::new("Providers").into_any_element()],
            )
            .into_any_element();
        };
        let now = self.app_state.read(cx).now_millis();
        let last_checked = config
            .providers
            .iter()
            .map(|provider| provider.checked_at.as_str())
            .max()
            .map(|checked| format_relative_time(checked, now));
        let mono = t3_ui::theme::Theme::global(cx).mono_family().clone();
        let checked = last_checked.map(|relative| {
            let line = div()
                .flex()
                .items_center()
                .type_scale(under_xs(11.))
                .text_color(colors.muted_foreground_60);
            match relative.strip_suffix(" ago") {
                Some(value) => line
                    .child("Checked\u{a0}")
                    .child(div().font_family(mono).child(value.to_owned()))
                    .child("\u{a0}ago"),
                None => line.child(format!("Checked {relative}")),
            }
        });
        let icon_action = |id: &'static str, icon: IconName, tooltip: &'static str| {
            let hover = colors.foreground;
            div()
                .id(id)
                .size_5()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_color(colors.muted_foreground)
                .hover(move |style| style.text_color(hover))
                .tooltip_text(tooltip)
                .child(Icon::new(icon).size(px(12.)))
        };
        let header_action = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .children(checked)
            .child(icon_action(
                "add-provider-instance",
                IconName::Plus,
                "Add provider instance",
            ))
            .child(
                icon_action(
                    "refresh-providers",
                    if self.refreshing {
                        IconName::Loader
                    } else {
                        IconName::RefreshCw
                    },
                    "Refresh provider status",
                )
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            );
        let cards: Vec<AnyElement> = build_rows(&config)
            .iter()
            .map(|row| self.render_card(&config, row, cx))
            .collect();
        page(
            "settings-providers",
            PAGE_MAX_WIDTH,
            [SettingsSection::new("Providers")
                .header_action(header_action)
                .children(cards)
                .into_any_element()],
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: separators scrambled (the fork keeps `@ . - _` so the shape reads as an
    //! email), output length differs, or the scramble is not deterministic.
    use super::redacted;

    #[test]
    fn redaction_keeps_shape() {
        let value = redacted("fake-codex@t3ui.invalid");
        assert_eq!(value.chars().count(), "fake-codex@t3ui.invalid".len());
        assert_eq!(value.chars().nth(4), Some('-'));
        assert_eq!(value.chars().nth(10), Some('@'));
        assert_eq!(value, redacted("fake-codex@t3ui.invalid"));
        assert_ne!(value, "fake-codex@t3ui.invalid");
    }
}
