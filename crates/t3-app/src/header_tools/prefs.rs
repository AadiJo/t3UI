//! Header control preferences the web keeps in localStorage: the last editor used
//! (`t3code:last-editor`) and the last script run per project
//! (`LAST_INVOKED_SCRIPT_BY_PROJECT_KEY`). Written to `header-tools.json`.

use std::collections::BTreeMap;

use gpui_kit::{App, AppContext as _, Context, Entity, Global};
use serde::{Deserialize, Serialize};
use t3_logic::ProjectRef;
use t3_protocol::server::EditorId;

use crate::state::AppState;

const PREFS_FILE: &str = "header-tools.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    last_editor: Option<EditorId>,
    /// Script id by `ProjectRef::key()`.
    last_invoked_script_by_project: BTreeMap<String, String>,
}

/// The persisted preferences. Read with [`HeaderPrefs::global`] and `cx.observe` it.
pub struct HeaderPrefs {
    stored: Stored,
}

struct GlobalPrefs(Entity<HeaderPrefs>);

impl Global for GlobalPrefs {}

impl HeaderPrefs {
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(prefs) = cx.try_global::<GlobalPrefs>() {
            return prefs.0.clone();
        }
        let stored = AppState::global(cx)
            .read(cx)
            .store()
            .read(PREFS_FILE)
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        let prefs = cx.new(|_| Self { stored });
        cx.set_global(GlobalPrefs(prefs.clone()));
        prefs
    }

    /// The preferred editor: the last one used if it is still available, else the first
    /// available editor in the server's order (`usePreferredEditor`).
    pub fn preferred_editor(&self, available: &[EditorId]) -> Option<EditorId> {
        self.stored
            .last_editor
            .as_ref()
            .filter(|editor| available.contains(editor))
            .or_else(|| {
                super::open_in::EDITORS
                    .iter()
                    .map(|option| &option.id)
                    .find(|id| available.contains(id))
            })
            .cloned()
    }

    pub fn set_last_editor(&mut self, editor: EditorId, cx: &mut Context<Self>) {
        if self.stored.last_editor.as_ref() != Some(&editor) {
            self.stored.last_editor = Some(editor);
            self.save(cx);
        }
    }

    /// The script last run in `project`, if any.
    pub fn last_script(&self, project: &ProjectRef) -> Option<&str> {
        self.stored
            .last_invoked_script_by_project
            .get(&project.key())
            .map(String::as_str)
    }

    pub fn set_last_script(&mut self, project: &ProjectRef, script_id: &str, cx: &mut Context<Self>) {
        let key = project.key();
        if self.stored.last_invoked_script_by_project.get(&key).map(String::as_str)
            != Some(script_id)
        {
            self.stored
                .last_invoked_script_by_project
                .insert(key, script_id.to_owned());
            self.save(cx);
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if let Ok(json) = serde_json::to_string_pretty(&self.stored) {
            let store = AppState::global(cx).read(cx).store().clone();
            store.write(PREFS_FILE, json, cx).detach();
        }
    }
}
