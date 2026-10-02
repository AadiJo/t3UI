//! General page (`SettingsPanels.tsx:507-1069`, spec 3.4) and "Restore defaults"
//! (`useSettingsRestore`, spec 3.1).
//!
//! Client settings live in [`AppState`]; server settings are read from the primary
//! environment's config and written with [`server::update_server_settings`]. Fork rows map to
//! upstream as follows:
//!
//! - "Assistant output" toggles `responseStreamingMode` between `token` (on) and the default
//!   `paragraph` (off); upstream replaced `enableAssistantStreaming` with it.
//! - "Commit generation instructions" / "PR generation instructions" have no upstream setting:
//!   shown, disabled.
//! - Reset buttons compare against upstream's defaults (`packages/contracts/src/settings.ts`).

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window,
    component::input::{InputEvent, InputState, TextareaState},
    div, px,
};
use t3_logic::{settings::TimestampFormat, ui_state::ThemePreference};
use t3_protocol::{
    ProviderInstanceId,
    orchestration::{ModelSelection, ProviderOptionSelection, ProviderOptionValue, ThreadEnvMode},
    server::{
        ProviderOptionDescriptor, ResponseStreamingMode, ServerConfig, ServerSettings,
        ServerSettingsPatch,
    },
};
use t3_ui::{
    ActiveColors as _, Button, ButtonSize, ButtonVariant, Input, Select, Switch, Textarea,
};

use super::{
    layout::{
        DRAFT_INPUT_WIDTH, PAGE_MAX_WIDTH, SELECT_WIDE_WIDTH, SELECT_WIDTH, SettingsRow,
        SettingsSection, page, reset_button,
    },
    server::{primary_config, update_server_settings},
};
use crate::{
    chrome::{TypeScale as _, under_xs},
    state::{AppState, Route, SettingsPage},
};

/// Upstream `DEFAULT_TEXT_GENERATION_MODEL` / `_REASONING_EFFORT`.
const DEFAULT_TEXT_MODEL: &str = "gpt-6-luna";
const DEFAULT_TEXT_EFFORT: &str = "low";
/// Upstream `DEFAULT_AUTOMATIC_GIT_FETCH_INTERVAL` in ms.
const DEFAULT_GIT_FETCH_INTERVAL_MS: f64 = 30_000.;
/// Separates instance id and model in a model select value.
const MODEL_VALUE_SEPARATOR: char = '\u{1f}';

/// The General page.
pub struct GeneralPage {
    app_state: Entity<AppState>,
    /// "Add project starts in": commits on blur or Enter (`DraftInput`).
    add_project_dir: Entity<InputState>,
    commit_instructions: Entity<TextareaState>,
    pr_instructions: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPage {
    pub fn new(app_state: Entity<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current_dir = settings_of(cx)
            .and_then(|settings| settings.add_project_base_directory.clone())
            .unwrap_or_default();
        let add_project_dir = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("~/")
                .default_value(current_dir)
        });
        let commit_instructions = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "For example: use lowercase conventional commit prefixes and keep the body concise.",
            )
        });
        let pr_instructions = cx.new(|cx| {
            TextareaState::new(window, cx).placeholder(
                "For example: include rollout notes and group testing by automated and manual checks.",
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &add_project_dir,
                window,
                |this, input, event, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.commit_add_project_dir(input, window, cx);
                    }
                },
            ),
            // Keep the draft in sync with the server value while the field is not being edited.
            cx.observe_in(&app_state, window, |this, _, window, cx| {
                let server = settings_of(cx)
                    .and_then(|settings| settings.add_project_base_directory.clone())
                    .unwrap_or_default();
                this.add_project_dir.update(cx, |input, cx| {
                    if !input.focus_handle(cx).is_focused(window) && input.value() != server {
                        input.set_value(server, window, cx);
                    }
                });
                cx.notify();
            }),
        ];
        Self {
            app_state,
            add_project_dir,
            commit_instructions,
            pr_instructions,
            _subscriptions: subscriptions,
        }
    }

    fn commit_add_project_dir(
        &mut self,
        input: &Entity<InputState>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = input.read(cx).value().trim().to_owned();
        let current = settings_of(cx)
            .and_then(|settings| settings.add_project_base_directory.clone())
            .unwrap_or_default();
        if next != current {
            update_server_settings(
                ServerSettingsPatch {
                    add_project_base_directory: Some(next),
                    ..Default::default()
                },
                cx,
            );
        }
    }
}

fn settings_of(cx: &App) -> Option<ServerSettings> {
    primary_config(cx).map(|config| config.settings.clone())
}

fn patch(edit: impl FnOnce(&mut ServerSettingsPatch)) -> ServerSettingsPatch {
    let mut patch = ServerSettingsPatch::default();
    edit(&mut patch);
    patch
}

fn default_text_model() -> ModelSelection {
    ModelSelection {
        instance_id: ProviderInstanceId::from("codex"),
        model: DEFAULT_TEXT_MODEL.to_owned(),
        options: vec![ProviderOptionSelection {
            id: "reasoningEffort".to_owned(),
            value: ProviderOptionValue::String(DEFAULT_TEXT_EFFORT.to_owned()),
        }],
    }
}

/// Server values with upstream's decoding defaults applied.
struct Effective {
    streaming: bool,
    update_checks: bool,
    worktree_mode: bool,
    start_from_origin: bool,
    add_project_dir: String,
    text_model: ModelSelection,
    git_fetch_interval_ms: f64,
}

impl Effective {
    fn of(settings: Option<&ServerSettings>) -> Self {
        let settings = settings.cloned().unwrap_or_default();
        Self {
            streaming: settings.response_streaming_mode == Some(ResponseStreamingMode::Token),
            update_checks: settings.enable_provider_update_checks.unwrap_or(true),
            worktree_mode: settings.default_thread_env_mode == Some(ThreadEnvMode::Worktree),
            start_from_origin: settings.new_worktrees_start_from_origin.unwrap_or(true),
            add_project_dir: settings.add_project_base_directory.unwrap_or_default(),
            text_model: settings
                .text_generation_model_selection
                .unwrap_or_else(default_text_model),
            git_fetch_interval_ms: settings
                .other
                .get("automaticGitFetchInterval")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(DEFAULT_GIT_FETCH_INTERVAL_MS),
        }
    }
}

/// Labels of everything "Restore defaults" would reset, in the fork's order. Rows with no
/// upstream setting (generation instructions) never differ.
pub fn changed_labels(cx: &App) -> Vec<&'static str> {
    let app_state = AppState::global(cx);
    let state = app_state.read(cx);
    let client = state.settings();
    let defaults = t3_logic::settings::ClientSettings::default();
    let server = Effective::of(settings_of(cx).as_ref());
    let mut labels = Vec::new();
    let mut push = |changed: bool, label| {
        if changed {
            labels.push(label);
        }
    };
    push(state.ui().theme != ThemePreference::System, "Theme");
    push(
        client.timestamp_format != defaults.timestamp_format,
        "Time format",
    );
    push(
        client.sidebar_thread_preview_count != defaults.sidebar_thread_preview_count,
        "Visible threads",
    );
    push(client.word_wrap != defaults.word_wrap, "Word wrap");
    push(
        client.diff_ignore_whitespace != defaults.diff_ignore_whitespace,
        "Diff whitespace changes",
    );
    push(
        client.auto_open_plan_sidebar != defaults.auto_open_plan_sidebar,
        "Auto-open task panel",
    );
    push(server.streaming, "Assistant output");
    push(
        server.git_fetch_interval_ms != DEFAULT_GIT_FETCH_INTERVAL_MS,
        "Automatic Git fetch interval",
    );
    push(server.worktree_mode, "New thread mode");
    push(!server.start_from_origin, "New worktrees start from origin");
    push(
        !server.add_project_dir.is_empty(),
        "Add project base directory",
    );
    push(
        client.confirm_thread_archive != defaults.confirm_thread_archive,
        "Archive confirmation",
    );
    push(
        client.confirm_thread_delete != defaults.confirm_thread_delete,
        "Delete confirmation",
    );
    push(
        server.text_model != default_text_model(),
        "Git writing model",
    );
    labels
}

/// Resets the theme, the listed client settings, and the server settings to their defaults.
/// "Provider update checks" is not included, like the fork.
pub fn restore_defaults(cx: &mut App) {
    let app_state = AppState::global(cx);
    set_theme(ThemePreference::System, cx);
    app_state.update(cx, |state, cx| {
        state.update_settings(
            |settings| {
                let defaults = t3_logic::settings::ClientSettings::default();
                settings.timestamp_format = defaults.timestamp_format;
                settings.word_wrap = defaults.word_wrap;
                settings.diff_ignore_whitespace = defaults.diff_ignore_whitespace;
                settings.sidebar_thread_preview_count = defaults.sidebar_thread_preview_count;
                settings.auto_open_plan_sidebar = defaults.auto_open_plan_sidebar;
                settings.confirm_thread_archive = defaults.confirm_thread_archive;
                settings.confirm_thread_delete = defaults.confirm_thread_delete;
            },
            cx,
        )
    });
    let mut patch = patch(|patch| {
        patch.response_streaming_mode = Some(ResponseStreamingMode::Paragraph);
        patch.default_thread_env_mode = Some(None);
        patch.new_worktrees_start_from_origin = Some(true);
        patch.add_project_base_directory = Some(String::new());
        patch.text_generation_model_selection = Some(default_text_model());
    });
    patch.other.insert(
        "automaticGitFetchInterval".into(),
        DEFAULT_GIT_FETCH_INTERVAL_MS.into(),
    );
    update_server_settings(patch, cx);
}

/// Applies and persists the theme preference.
fn set_theme(theme: ThemePreference, cx: &mut App) {
    AppState::global(cx).update(cx, |state, cx| state.set_theme(theme, cx));
}

fn theme_value(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::System => "system",
        ThemePreference::Light => "light",
        ThemePreference::Dark => "dark",
    }
}

fn timestamp_value(format: TimestampFormat) -> &'static str {
    match format {
        TimestampFormat::Locale => "locale",
        TimestampFormat::TwelveHour => "12-hour",
        TimestampFormat::TwentyFourHour => "24-hour",
    }
}

/// Edits client settings from a control callback.
fn edit_client(cx: &mut App, edit: impl FnOnce(&mut t3_logic::settings::ClientSettings)) {
    AppState::global(cx).update(cx, |state, cx| state.update_settings(edit, cx));
}

/// A fixed-width wrapper so selects get the fork's `sm:w-40` / `sm:w-44`.
fn sized(width: gpui_kit::Pixels, child: impl IntoElement) -> impl IntoElement {
    div().w(width).child(child)
}

/// The text generation model pickers: model (enabled providers that generate text) and, when
/// the model has one, its first select option such as reasoning effort.
fn text_model_controls(config: Option<&ServerConfig>, selection: &ModelSelection) -> AnyElement {
    let providers = config
        .map(|config| config.providers.as_slice())
        .unwrap_or(&[]);
    let mut models: Vec<(SharedString, SharedString)> = Vec::new();
    for provider in providers
        .iter()
        .filter(|provider| provider.enabled && provider.supports_text_generation)
    {
        for model in &provider.models {
            models.push((
                format!(
                    "{}{MODEL_VALUE_SEPARATOR}{}",
                    provider.instance_id, model.slug
                )
                .into(),
                model.name.clone().into(),
            ));
        }
    }
    let value: SharedString = format!(
        "{}{MODEL_VALUE_SEPARATOR}{}",
        selection.instance_id, selection.model
    )
    .into();
    if !models.iter().any(|(item, _)| *item == value) {
        models.push((value.clone(), selection.model.clone().into()));
    }
    let effort = providers
        .iter()
        .find(|provider| provider.instance_id == selection.instance_id)
        .and_then(|provider| {
            provider
                .models
                .iter()
                .find(|model| model.slug == selection.model)
        })
        .and_then(|model| model.capabilities.as_ref())
        .and_then(|capabilities| {
            capabilities
                .option_descriptors
                .iter()
                .find_map(|descriptor| match descriptor {
                    ProviderOptionDescriptor::Select { id, options, .. } => {
                        Some((id.clone(), options.clone()))
                    }
                    _ => None,
                })
        });
    let model_select = Select::new("text-model")
        .items(models)
        .value(Some(value))
        .on_change(|value, _, cx| {
            let Some((instance, model)) = value.split_once(MODEL_VALUE_SEPARATOR) else {
                return;
            };
            update_server_settings(
                patch(|patch| {
                    patch.text_generation_model_selection = Some(ModelSelection {
                        instance_id: ProviderInstanceId::from(instance),
                        model: model.to_owned(),
                        options: Vec::new(),
                    })
                }),
                cx,
            );
        });
    let effort_select = effort.map(|(option_id, choices)| {
        let current = selection
            .options
            .iter()
            .find(|option| option.id == option_id)
            .and_then(|option| match &option.value {
                ProviderOptionValue::String(value) => Some(value.clone()),
                ProviderOptionValue::Bool(_) => None,
            })
            .or_else(|| {
                choices
                    .iter()
                    .find(|choice| choice.is_default)
                    .map(|choice| choice.id.clone())
            });
        let selection = selection.clone();
        Select::new("text-model-effort")
            .items(
                choices
                    .iter()
                    .map(|choice| (choice.id.clone(), choice.label.clone())),
            )
            .value(current.map(SharedString::from))
            .on_change(move |value, _, cx| {
                let mut next = selection.clone();
                next.options.retain(|option| option.id != option_id);
                next.options.push(ProviderOptionSelection {
                    id: option_id.clone(),
                    value: ProviderOptionValue::String(value.to_string()),
                });
                update_server_settings(
                    patch(|patch| patch.text_generation_model_selection = Some(next)),
                    cx,
                );
            })
    });
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_end()
        .gap(px(6.))
        .child(sized(SELECT_WIDE_WIDTH, model_select))
        .children(effort_select.map(|select| sized(px(144.), select)))
        .into_any_element()
}

impl Render for GeneralPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        let state = self.app_state.read(cx);
        let client = state.settings().clone();
        let theme = state.ui().theme;
        let defaults = t3_logic::settings::ClientSettings::default();
        let config = primary_config(cx);
        let server = Effective::of(config.as_ref().map(|config| &config.settings));

        let theme_row = SettingsRow::new("Theme", "Choose how T3 Code looks across the app.")
            .reset((theme != ThemePreference::System).then(|| {
                reset_button("reset-theme", colors, |_, cx| {
                    set_theme(ThemePreference::System, cx)
                })
            }))
            .control(sized(
                SELECT_WIDTH,
                Select::new("theme")
                    .items([("system", "System"), ("light", "Light"), ("dark", "Dark")])
                    .value(Some(theme_value(theme).into()))
                    .on_change(|value, _, cx| {
                        let theme = match value.as_ref() {
                            "light" => ThemePreference::Light,
                            "dark" => ThemePreference::Dark,
                            _ => ThemePreference::System,
                        };
                        set_theme(theme, cx);
                    }),
            ));

        let time_row = SettingsRow::new(
            "Time format",
            "System default follows your browser or OS clock preference.",
        )
        .reset(
            (client.timestamp_format != defaults.timestamp_format).then(|| {
                reset_button("reset-time-format", colors, |_, cx| {
                    edit_client(cx, |settings| {
                        settings.timestamp_format = TimestampFormat::default()
                    })
                })
            }),
        )
        .control(sized(
            SELECT_WIDTH,
            Select::new("time-format")
                .items([
                    ("locale", "System default"),
                    ("12-hour", "12-hour"),
                    ("24-hour", "24-hour"),
                ])
                .value(Some(timestamp_value(client.timestamp_format).into()))
                .on_change(|value, _, cx| {
                    let format = match value.as_ref() {
                        "12-hour" => TimestampFormat::TwelveHour,
                        "24-hour" => TimestampFormat::TwentyFourHour,
                        _ => TimestampFormat::Locale,
                    };
                    edit_client(cx, |settings| settings.timestamp_format = format);
                }),
        ));

        /// A client boolean setting row.
        fn client_switch(
            id: &'static str,
            title: &'static str,
            description: &'static str,
            value: bool,
            default: bool,
            colors: &t3_ui::Colors,
            set: fn(&mut t3_logic::settings::ClientSettings, bool),
        ) -> SettingsRow {
            SettingsRow::new(title, description)
                .reset((value != default).then(|| {
                    reset_button(
                        SharedString::from(format!("reset-{id}")),
                        colors,
                        move |_, cx| edit_client(cx, |settings| set(settings, default)),
                    )
                }))
                .control(
                    Switch::new(id)
                        .checked(value)
                        .on_change(move |checked, _, cx| {
                            let checked = *checked;
                            edit_client(cx, |settings| set(settings, checked))
                        }),
                )
        }

        let word_wrap = client_switch(
            "word-wrap",
            "Word wrap",
            "Wrap long lines in code blocks, tables, diffs, and file previews by default.",
            client.word_wrap,
            defaults.word_wrap,
            colors,
            |settings, value| settings.word_wrap = value,
        );
        let whitespace = client_switch(
            "diff-whitespace",
            "Hide whitespace changes",
            "Set whether the diff panel ignores whitespace-only edits by default.",
            client.diff_ignore_whitespace,
            defaults.diff_ignore_whitespace,
            colors,
            |settings, value| settings.diff_ignore_whitespace = value,
        );
        let streaming = SettingsRow::new(
            "Assistant output",
            "Show smoothly paced live output while a response is in progress.",
        )
        .reset(server.streaming.then(|| {
            reset_button("reset-streaming", colors, |_, cx| {
                update_server_settings(
                    patch(|patch| {
                        patch.response_streaming_mode = Some(ResponseStreamingMode::Paragraph)
                    }),
                    cx,
                )
            })
        }))
        .control(
            Switch::new("streaming")
                .checked(server.streaming)
                .on_change(|checked, _, cx| {
                    let mode = if *checked {
                        ResponseStreamingMode::Token
                    } else {
                        ResponseStreamingMode::Paragraph
                    };
                    update_server_settings(
                        patch(|patch| patch.response_streaming_mode = Some(mode)),
                        cx,
                    )
                }),
        );
        let update_checks = SettingsRow::new(
            "Provider update checks",
            "Check installed provider CLIs for newer available versions.",
        )
        .reset((!server.update_checks).then(|| {
            reset_button("reset-update-checks", colors, |_, cx| {
                update_server_settings(
                    patch(|patch| patch.enable_provider_update_checks = Some(true)),
                    cx,
                )
            })
        }))
        .control(
            Switch::new("update-checks")
                .checked(server.update_checks)
                .on_change(|checked, _, cx| {
                    let checked = *checked;
                    update_server_settings(
                        patch(|patch| patch.enable_provider_update_checks = Some(checked)),
                        cx,
                    )
                }),
        );
        let task_panel = client_switch(
            "auto-open-task-panel",
            "Auto-open task panel",
            "Open the right-side plan and task panel automatically when steps appear.",
            client.auto_open_plan_sidebar,
            defaults.auto_open_plan_sidebar,
            colors,
            |settings, value| settings.auto_open_plan_sidebar = value,
        );
        let new_threads = SettingsRow::new(
            "New threads",
            "Pick the default workspace mode for newly created draft threads.",
        )
        .reset(
            (server.worktree_mode || !server.start_from_origin).then(|| {
                reset_button("reset-new-threads", colors, |_, cx| {
                    update_server_settings(
                        patch(|patch| {
                            patch.default_thread_env_mode = Some(None);
                            patch.new_worktrees_start_from_origin = Some(true);
                        }),
                        cx,
                    )
                })
            }),
        )
        .control(sized(
            SELECT_WIDE_WIDTH,
            Select::new("new-thread-mode")
                .items([("local", "Local"), ("worktree", "New worktree")])
                .value(Some(
                    if server.worktree_mode {
                        "worktree"
                    } else {
                        "local"
                    }
                    .into(),
                ))
                .on_change(|value, _, cx| {
                    let mode = if value.as_ref() == "worktree" {
                        ThreadEnvMode::Worktree
                    } else {
                        ThreadEnvMode::Local
                    };
                    update_server_settings(
                        patch(|patch| patch.default_thread_env_mode = Some(Some(mode))),
                        cx,
                    )
                }),
        ));
        let start_from_origin = server.worktree_mode.then(|| {
            SettingsRow::new(
                "Start from origin",
                "Creates the worktree from the latest matching branch on origin instead of your local branch.",
            )
            .nested()
            .reset((!server.start_from_origin).then(|| {
                reset_button("reset-start-from-origin", colors, |_, cx| {
                    update_server_settings(
                        patch(|patch| patch.new_worktrees_start_from_origin = Some(true)),
                        cx,
                    )
                })
            }))
            .control(
                Switch::new("start-from-origin")
                    .checked(server.start_from_origin)
                    .on_change(|checked, _, cx| {
                        let checked = *checked;
                        update_server_settings(
                            patch(|patch| patch.new_worktrees_start_from_origin = Some(checked)),
                            cx,
                        )
                    }),
            )
        });
        let add_project = SettingsRow::new(
            "Add project starts in",
            "Leave empty to use \"~/\" when the Add Project browser opens.",
        )
        .reset((!server.add_project_dir.is_empty()).then(|| {
            reset_button("reset-add-project-dir", colors, |_, cx| {
                update_server_settings(
                    patch(|patch| patch.add_project_base_directory = Some(String::new())),
                    cx,
                )
            })
        }))
        .control(
            div()
                .w(DRAFT_INPUT_WIDTH)
                .child(Input::new(&self.add_project_dir)),
        );
        let archive_confirm = client_switch(
            "confirm-archive",
            "Archive confirmation",
            "Require a second click on the inline archive action before a thread is archived.",
            client.confirm_thread_archive,
            defaults.confirm_thread_archive,
            colors,
            |settings, value| settings.confirm_thread_archive = value,
        );
        let delete_confirm = client_switch(
            "confirm-delete",
            "Delete confirmation",
            "Ask before deleting a thread and its chat history.",
            client.confirm_thread_delete,
            defaults.confirm_thread_delete,
            colors,
            |settings, value| settings.confirm_thread_delete = value,
        );
        let text_model = SettingsRow::new(
            "Text generation model",
            "Configure the model used for generated commit messages, PR titles, and similar Git text.",
        )
        .reset((server.text_model != default_text_model()).then(|| {
            reset_button("reset-text-model", colors, |_, cx| {
                update_server_settings(
                    patch(|patch| patch.text_generation_model_selection = Some(default_text_model())),
                    cx,
                )
            })
        }))
        .control(text_model_controls(config.as_deref(), &server.text_model));
        // No upstream setting: shown for parity, disabled.
        let commit_instructions = SettingsRow::new(
            "Commit generation instructions",
            "Additional guidance for generated commit subjects and bodies. Applied to every connected environment.",
        )
        .child(
            div()
                .mt_3()
                .pb_4()
                .child(Textarea::new(&self.commit_instructions).disabled(true)),
        );
        let pr_instructions = SettingsRow::new(
            "PR generation instructions",
            "Additional guidance for generated pull request titles and descriptions. Applied to every connected environment, including WSL on Windows.",
        )
        .child(
            div()
                .mt_3()
                .pb_4()
                .child(Textarea::new(&self.pr_instructions).disabled(true)),
        );

        let general = SettingsSection::new("General")
            .child(theme_row)
            .child(time_row)
            .child(word_wrap)
            .child(whitespace)
            .child(streaming)
            .child(update_checks)
            .child(task_panel)
            .child(new_threads)
            .children(start_from_origin)
            .child(add_project)
            .child(archive_confirm)
            .child(delete_confirm)
            .child(text_model)
            .child(commit_instructions)
            .child(pr_instructions);

        let version_title = div().flex().items_center().gap_2().child("Version").child(
            div()
                .font_family(t3_ui::theme::Theme::global(cx).mono_family().clone())
                .type_scale(under_xs(11.))
                .font_weight(gpui_kit::FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(env!("CARGO_PKG_VERSION")),
        );
        let diagnostics_description = config
            .as_ref()
            .and_then(|config| config.observability.as_ref())
            .map(diagnostics_description)
            .unwrap_or_else(|| "Terminal logs only.".to_owned());
        let app_state = self.app_state.clone();
        let about = SettingsSection::new("About")
            .child(SettingsRow::new(
                version_title,
                "Current version of the application.",
            ))
            .child(
                SettingsRow::new("Diagnostics", diagnostics_description).control(
                    Button::new("view-diagnostics")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Xs)
                        .label("View diagnostics")
                        .on_click(move |_, _, cx| {
                            app_state.update(cx, |state, cx| {
                                state.navigate(Route::Settings(SettingsPage::Diagnostics), cx)
                            })
                        }),
                ),
            );

        page(
            "settings-general",
            PAGE_MAX_WIDTH,
            [general.into_any_element(), about.into_any_element()],
        )
    }
}

/// `formatDiagnosticsDescription` (`SettingsPanels.logic.ts:29-56`).
fn diagnostics_description(observability: &t3_protocol::server::ServerObservability) -> String {
    let mode = if observability.local_tracing_enabled {
        "Local trace file"
    } else {
        "Terminal logs only"
    };
    let traces = observability
        .otlp_traces_enabled
        .then_some(observability.otlp_traces_url.as_deref())
        .flatten();
    let metrics = observability
        .otlp_metrics_enabled
        .then_some(observability.otlp_metrics_url.as_deref())
        .flatten();
    match (traces, metrics) {
        (Some(traces), Some(metrics)) => {
            let collapsed = traces
                .strip_suffix("/traces")
                .zip(metrics.strip_suffix("/metrics"))
                .filter(|(left, right)| left == right)
                .map(|(base, _)| format!("{base}/{{traces,metrics}}"));
            match collapsed {
                Some(url) => format!("{mode}. Exporting OTEL to {url}."),
                None => {
                    format!("{mode}. Exporting OTEL traces to {traces} and metrics to {metrics}.")
                }
            }
        }
        (Some(traces), None) => format!("{mode}. Exporting OTEL traces to {traces}."),
        (None, Some(metrics)) => format!("{mode}. Exporting OTEL metrics to {metrics}."),
        (None, None) => format!("{mode}."),
    }
}
