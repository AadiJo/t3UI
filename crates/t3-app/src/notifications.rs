//! App-level notifications raised from server state (spec 1.5 `EventRouter`, 5.2).
//!
//! [`KeybindingsNotifier`] watches the primary environment's config: when its keybindings are
//! reloaded it shows "Keybindings updated" (at most once per 2s), or "Invalid keybindings
//! configuration" with an action that opens `keybindings.json` in the preferred editor.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui_kit::App;
use t3_protocol::{
    methods::ShellOpenInEditor,
    projects::LaunchEditorInput,
    server::{EditorId, ServerConfig},
};

use crate::{
    state::AppState,
    toast::{self, Toast, ToastActionStyle},
};

/// `KEYBINDINGS_SUCCESS_TOAST_COOLDOWN_MS`.
const SUCCESS_COOLDOWN: Duration = Duration::from_secs(2);

/// Remembers the last keybindings seen so a reload can be told apart from other config
/// changes. Call [`KeybindingsNotifier::check`] whenever app state changes.
#[derive(Default)]
pub struct KeybindingsNotifier {
    config: Option<Arc<ServerConfig>>,
    last_success: Option<Instant>,
}

impl KeybindingsNotifier {
    pub fn check(&mut self, cx: &mut App) {
        let Some(config) = AppState::global(cx)
            .read(cx)
            .primary_environment()
            .and_then(|environment| environment.read(cx).config().cloned())
        else {
            return;
        };
        let previous = self.config.replace(config.clone());
        let Some(previous) = previous else {
            // The first config of a session is not a reload.
            return;
        };
        if Arc::ptr_eq(&previous, &config)
            || (previous.keybindings == config.keybindings && previous.issues == config.issues)
        {
            return;
        }
        if let Some(issue) = config
            .issues
            .iter()
            .find(|issue| issue.kind.starts_with("keybindings."))
        {
            let path = config.keybindings_config_path.clone();
            toast::show(
                Toast::warning("Invalid keybindings configuration")
                    .description(issue.message.clone())
                    .stacked()
                    .action(
                        "Open keybindings.json",
                        ToastActionStyle::Outline,
                        move |_, cx| open_in_preferred_editor(path.clone(), cx),
                    ),
                cx,
            );
            return;
        }
        if self
            .last_success
            .is_some_and(|at| at.elapsed() < SUCCESS_COOLDOWN)
        {
            return;
        }
        self.last_success = Some(Instant::now());
        toast::show(
            Toast::success("Keybindings updated")
                .description("Keybindings configuration reloaded successfully."),
            cx,
        );
    }
}

/// Opens `path` with `shell.openInEditor` in the primary environment's preferred editor
/// (the remembered one if still available, else the first available). Failures toast
/// "Unable to open keybindings file".
fn open_in_preferred_editor(path: String, cx: &mut App) {
    let state = AppState::global(cx);
    let Some(environment) = state.read(cx).primary_environment().cloned() else {
        return;
    };
    let environment = environment.read(cx);
    let available: Vec<EditorId> = environment
        .config()
        .map(|config| config.available_editors.clone())
        .unwrap_or_default();
    let remembered = state
        .read(cx)
        .ui()
        .last_editor
        .as_deref()
        .map(EditorId::from);
    let editor = remembered
        .filter(|editor| available.contains(editor))
        .or_else(|| available.first().cloned());
    let (Some(editor), Some(client)) = (editor, environment.client().cloned()) else {
        toast::show(
            Toast::error("Unable to open keybindings file")
                .description("Unknown error opening file.")
                .stacked(),
            cx,
        );
        return;
    };
    let task = gpui_kit::AppContext::background_spawn(cx, async move {
        client
            .request::<ShellOpenInEditor>(&LaunchEditorInput {
                cwd: path,
                editor,
                reveal: None,
            })
            .await
            .map_err(|error| error.to_string())
    });
    cx.spawn(async move |cx| {
        if let Err(message) = task.await {
            cx.update(|cx| {
                toast::show(
                    Toast::error("Unable to open keybindings file")
                        .description(message)
                        .stacked(),
                    cx,
                );
            });
        }
    })
    .detach();
}
