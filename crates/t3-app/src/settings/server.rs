//! The primary environment's server config, and writing its settings (`server.updateSettings`).
//! The web routes every setting key found in `ServerSettings` to the primary environment
//! (`web/hooks/useSettings.ts:146-163`); the result comes back on the config stream, which
//! re-renders the page.

use std::sync::Arc;

use gpui_kit::App;
use t3_protocol::{
    methods::ServerUpdateSettings,
    server::{ServerConfig, ServerSettingsPatch, UpdateSettingsInput},
};

use crate::{
    state::{AppState, Environment},
    toast::{self, Toast},
};

/// The primary environment, if any.
pub fn primary(cx: &App) -> Option<gpui_kit::Entity<Environment>> {
    AppState::global(cx).read(cx).primary_environment().cloned()
}

/// The primary environment's server config, once received.
pub fn primary_config(cx: &App) -> Option<Arc<ServerConfig>> {
    primary(cx).and_then(|environment| environment.read(cx).config().cloned())
}

/// Sends `patch` to the primary environment. Failures raise an error toast; success shows up
/// through the config stream.
pub fn update_server_settings(patch: ServerSettingsPatch, cx: &mut App) {
    let Some(client) = primary(cx).and_then(|environment| environment.read(cx).client().cloned())
    else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = client
            .request::<ServerUpdateSettings>(&UpdateSettingsInput { patch })
            .await;
        if let Err(error) = result {
            cx.update(|cx| {
                toast::show(
                    Toast::error("Could not save settings")
                        .description(error.to_string())
                        .stacked(),
                    cx,
                );
            });
        }
    })
    .detach();
}
