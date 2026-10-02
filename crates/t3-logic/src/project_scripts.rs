//! Project scripts ("actions") in the chat header (web `projectScripts.ts`,
//! `shared/projectScripts.ts`, `lib/projectScriptKeybindings.ts`, the editor input of
//! `projectScriptEditor.tsx`, and the shortcut capture of `KeybindingsSettings.logic.ts`).
//!
//! Scripts live in the server settings, not on the project: a project's
//! `projectSettingsOverrides[id].defaultProjectScripts` wins, then the environment's
//! `defaultProjectScripts` ([`resolve_project_scripts`]). Saving writes the whole list back with
//! `server.updateSettings` ([`scripts_patch`]).

use std::collections::{BTreeMap, HashSet};

use serde_json::{Map, Value};
use t3_protocol::{
    orchestration::{ProjectScript, ProjectScriptIcon},
    server::{ResolvedKeybindingRule, ServerSettings},
};

use crate::keybindings::{Platform, ShortcutEvent, parse_shortcut};

/// Script ids are at most this long (`MAX_SCRIPT_ID_LENGTH`).
pub const MAX_SCRIPT_ID_LENGTH: usize = 24;

/// The keybinding command that runs a script, or `None` for legacy ids the command pattern
/// (`script.[a-z0-9][a-z0-9-]*.run`, id at most 24 chars) rejects; those scripts still run but
/// cannot have a shortcut.
pub fn script_command(script_id: &str) -> Option<String> {
    let valid = !script_id.is_empty()
        && script_id.len() <= MAX_SCRIPT_ID_LENGTH
        && script_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !script_id.starts_with('-');
    valid.then(|| format!("script.{script_id}.run"))
}

/// What the add/edit dialog submits (`NewProjectScriptInput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptInput {
    pub name: String,
    pub command: String,
    pub icon: ProjectScriptIcon,
    pub run_on_worktree_create: bool,
    /// Setup scripts only: hold the agent until the script exits (`async: false`).
    pub wait_for_setup: bool,
    pub keybinding: Option<String>,
    pub preview_url: Option<String>,
    pub auto_open_preview: bool,
}

impl ScriptInput {
    /// The dialog's fields for an existing script (`editorRequestForScript`).
    pub fn from_script(script: &ProjectScript, rules: &[ResolvedKeybindingRule]) -> Self {
        Self {
            name: script.name.clone(),
            command: script.command.clone(),
            icon: script.icon.clone(),
            run_on_worktree_create: script.run_on_worktree_create,
            wait_for_setup: script.run_on_worktree_create && script.run_async == Some(false),
            keybinding: script_command(&script.id)
                .and_then(|command| keybinding_value_for_command(rules, &command)),
            preview_url: script.preview_url.clone(),
            auto_open_preview: script.auto_open_preview.unwrap_or(false),
        }
    }
}

impl Default for ScriptInput {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            icon: ProjectScriptIcon::Play,
            run_on_worktree_create: false,
            wait_for_setup: false,
            keybinding: None,
            preview_url: None,
            auto_open_preview: false,
        }
    }
}

/// `buildProjectScript`: `async: false` only for a setup script that waits; the preview
/// fields only with a preview URL.
pub fn build_script(id: String, input: &ScriptInput) -> ProjectScript {
    ProjectScript {
        id,
        name: input.name.clone(),
        command: input.command.clone(),
        icon: input.icon.clone(),
        run_on_worktree_create: input.run_on_worktree_create,
        run_async: (input.run_on_worktree_create && input.wait_for_setup).then_some(false),
        preview_url: input.preview_url.clone(),
        auto_open_preview: input.preview_url.as_ref().map(|_| input.auto_open_preview),
    }
}

/// `resolveProjectScripts`: the project's override, then the environment defaults. Before the
/// server folds the legacy map, `projectScriptOverrides[id]` (null meaning "use defaults") and
/// the project's own scripts still count.
pub fn resolve_project_scripts(
    settings: &ServerSettings,
    project_id: &str,
    project_scripts: &[ProjectScript],
) -> Vec<ProjectScript> {
    let defaults = || settings.default_project_scripts.clone().unwrap_or_default();
    let decode = |value: &Value| serde_json::from_value::<Vec<ProjectScript>>(value.clone()).ok();
    let override_scripts = settings
        .other
        .get("projectSettingsOverrides")
        .and_then(|overrides| overrides.get(project_id))
        .and_then(|project| project.get("defaultProjectScripts"))
        .and_then(decode);
    if let Some(scripts) = override_scripts {
        return scripts;
    }
    if settings
        .other
        .get("projectSettingsFolded")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return defaults();
    }
    match settings
        .other
        .get("projectScriptOverrides")
        .and_then(|legacy| legacy.get(project_id))
    {
        Some(Value::Null) => defaults(),
        Some(value) => decode(value).unwrap_or_else(defaults),
        None if !project_scripts.is_empty() => project_scripts.to_vec(),
        None => defaults(),
    }
}

/// The `server.updateSettings` patch that stores `scripts` for a project. Servers with the
/// `projectSettingsOverrides` capability get the canonical key (merged over the project's
/// other overrides); older ones the legacy per-project map.
pub fn scripts_patch(
    settings: &ServerSettings,
    project_id: &str,
    scripts: &[ProjectScript],
    supports_project_overrides: bool,
) -> Map<String, Value> {
    let scripts = serde_json::to_value(scripts).unwrap_or(Value::Array(Vec::new()));
    let mut patch = Map::new();
    if supports_project_overrides {
        let mut project = settings
            .other
            .get("projectSettingsOverrides")
            .and_then(|overrides| overrides.get(project_id))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        project.insert("defaultProjectScripts".into(), scripts);
        let mut overrides = Map::new();
        overrides.insert(project_id.to_owned(), Value::Object(project));
        patch.insert("projectSettingsOverrides".into(), Value::Object(overrides));
    } else {
        let mut overrides = Map::new();
        overrides.insert(project_id.to_owned(), scripts);
        patch.insert("projectScriptOverrides".into(), Value::Object(overrides));
    }
    patch
}

/// The header's primary script: `preferred` (last run) when it still exists, else the first
/// script that is not the worktree setup script, else the first script.
pub fn primary_script<'a>(
    scripts: &'a [ProjectScript],
    preferred: Option<&str>,
) -> Option<&'a ProjectScript> {
    preferred
        .and_then(|id| scripts.iter().find(|script| script.id == id))
        .or_else(|| scripts.iter().find(|script| !script.run_on_worktree_create))
        .or_else(|| scripts.first())
}

/// A new script's id from its name: lowercased, runs of other characters become `-`, at most
/// 24 characters, with `-2`, `-3`… on collision (`nextProjectScriptId`).
pub fn next_script_id<'a>(name: &str, existing: impl IntoIterator<Item = &'a str>) -> String {
    let taken: HashSet<&str> = existing.into_iter().collect();
    let base = normalize_script_id(name);
    if !taken.contains(base.as_str()) {
        return base;
    }
    for suffix in 2..10_000 {
        let candidate = format!("{base}-{suffix}");
        let candidate = if candidate.len() <= MAX_SCRIPT_ID_LENGTH {
            candidate
        } else {
            let keep = MAX_SCRIPT_ID_LENGTH
                .saturating_sub(suffix.to_string().len() + 1)
                .max(1);
            format!("{}-{suffix}", &base[..keep.min(base.len())])
        };
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
    }
    base
}

fn normalize_script_id(value: &str) -> String {
    let lowered = value.trim().to_lowercase();
    let mut cleaned = String::new();
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            cleaned.push(ch);
        } else if !cleaned.ends_with('-') {
            cleaned.push('-');
        }
    }
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        return "script".into();
    }
    if cleaned.len() <= MAX_SCRIPT_ID_LENGTH {
        return cleaned.to_owned();
    }
    let cut = cleaned[..MAX_SCRIPT_ID_LENGTH].trim_end_matches('-');
    if cut.is_empty() {
        "script".into()
    } else {
        cut.to_owned()
    }
}

/// Saves `script` into `scripts`: replaces the entry with `previous_id` (editing) or appends.
/// Only one script may run on worktree creation, so setting it clears the others.
pub fn upsert_script(
    scripts: &[ProjectScript],
    previous_id: Option<&str>,
    script: ProjectScript,
) -> Vec<ProjectScript> {
    let setup = script.run_on_worktree_create;
    let mut next: Vec<ProjectScript> = match previous_id {
        Some(id) if scripts.iter().any(|existing| existing.id == id) => scripts
            .iter()
            .map(|existing| {
                if existing.id == id {
                    script.clone()
                } else {
                    existing.clone()
                }
            })
            .collect(),
        _ => scripts.iter().cloned().chain([script.clone()]).collect(),
    };
    if setup {
        for existing in &mut next {
            if existing.id != script.id {
                existing.run_on_worktree_create = false;
            }
        }
    }
    next
}

/// The shortcut text the scripts dialog captures from a key press (`keybindingFromKeyboardEvent`),
/// e.g. `mod+shift+t`. `None` for bare modifier presses, unsupported keys, or a key without any
/// modifier.
pub fn keybinding_from_event(event: &ShortcutEvent, platform: Platform) -> Option<String> {
    let key = key_token(event.key())?;
    let modifiers = event.modifiers();
    let mut parts: Vec<&str> = Vec::new();
    match platform {
        Platform::Mac => {
            if modifiers.meta {
                parts.push("mod");
            }
            if modifiers.ctrl {
                parts.push("ctrl");
            }
        }
        Platform::Other => {
            if modifiers.ctrl {
                parts.push("mod");
            }
            if modifiers.meta {
                parts.push("meta");
            }
        }
    }
    if modifiers.alt {
        parts.push("alt");
    }
    if modifiers.shift {
        parts.push("shift");
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!("{}+{key}", parts.join("+")))
}

/// `normalizeShortcutKeyToken`, on DOM key names.
fn key_token(key: &str) -> Option<String> {
    let key = key.to_lowercase();
    let token = match key.as_str() {
        "meta" | "control" | "ctrl" | "shift" | "alt" | "option" => return None,
        " " => "space".to_owned(),
        "escape" => "esc".to_owned(),
        "arrowup" | "arrowdown" | "arrowleft" | "arrowright" | "enter" | "tab" | "backspace"
        | "delete" | "home" | "end" | "pageup" | "pagedown" => key,
        _ if key.chars().count() == 1 => key,
        _ if key.len() <= 3
            && key.starts_with('f')
            && key.len() > 1
            && key[1..].bytes().all(|b| b.is_ascii_digit()) =>
        {
            key
        }
        _ => return None,
    };
    Some(token)
}

/// Whether captured shortcut text is a valid keybinding key (`decodeProjectScriptKeybindingRule`
/// fails with "Invalid keybinding." otherwise). Blank text means no keybinding and is valid.
pub fn is_valid_keybinding(key: &str) -> bool {
    key.trim().is_empty() || parse_shortcut(key.trim()).is_some()
}

/// The editable text of the last rule bound to `command` (`keybindingValueForCommand`).
pub fn keybinding_value_for_command(
    rules: &[ResolvedKeybindingRule],
    command: &str,
) -> Option<String> {
    let rule = rules.iter().rev().find(|rule| rule.command == command)?;
    let shortcut = &rule.shortcut;
    let mut parts: Vec<String> = Vec::new();
    for (held, name) in [
        (shortcut.mod_key, "mod"),
        (shortcut.ctrl_key, "ctrl"),
        (shortcut.meta_key, "meta"),
        (shortcut.alt_key, "alt"),
        (shortcut.shift_key, "shift"),
    ] {
        if held {
            parts.push(name.into());
        }
    }
    parts.push(match shortcut.key.as_str() {
        " " => "space".into(),
        "escape" => "esc".into(),
        other => other.into(),
    });
    Some(parts.join("+"))
}

/// `projectScriptRuntimeEnv`: what terminals and scripts of a project get in their environment.
pub fn runtime_env(project_root: &str, worktree_path: Option<&str>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([("T3CODE_PROJECT_ROOT".to_owned(), project_root.to_owned())]);
    if let Some(path) = worktree_path.filter(|path| !path.is_empty()) {
        env.insert("T3CODE_WORKTREE_PATH".into(), path.to_owned());
    }
    env
}

#[cfg(test)]
mod tests {
    //! Failure modes: ids with leading/trailing dashes or over 24 chars, collision suffixes
    //! pushing ids past the limit, legacy ids given a shortcut command the server rejects, the
    //! setup script flag left on two scripts, an edited script appended instead of replaced,
    //! `async: false` written for non-setup scripts, preview fields kept without a URL, the
    //! wrong settings source winning (override vs folded defaults vs legacy null), the patch
    //! dropping a project's other overrides, shortcuts captured from bare modifiers or with
    //! `mod` mapped to the wrong physical key per platform, and the worktree env var set for a
    //! blank path.
    use super::*;
    use crate::keybindings::Modifiers;
    use t3_protocol::orchestration::ProjectScriptIcon;

    fn script(id: &str, setup: bool) -> ProjectScript {
        ProjectScript {
            id: id.into(),
            name: id.into(),
            command: "true".into(),
            icon: ProjectScriptIcon::Play,
            run_on_worktree_create: setup,
            run_async: None,
            preview_url: None,
            auto_open_preview: None,
        }
    }

    #[test]
    fn ids() {
        assert_eq!(next_script_id(" Run Tests! ", []), "run-tests");
        assert_eq!(next_script_id("!!!", []), "script");
        assert_eq!(next_script_id("test", ["test", "test-2"]), "test-3");
        let long = "a very long action name that keeps going";
        let id = next_script_id(long, []);
        assert_eq!(id, "a-very-long-action-name");
        let next = next_script_id(long, [id.as_str()]);
        assert!(next.len() <= MAX_SCRIPT_ID_LENGTH, "{next}");
        assert!(next.ends_with("-2"));
        assert_eq!(script_command("lint").as_deref(), Some("script.lint.run"));
        assert_eq!(script_command("Legacy_ID"), None);
        assert_eq!(script_command("-x"), None);
    }

    #[test]
    fn primary_and_upsert() {
        let scripts = vec![script("setup", true), script("dev", false)];
        assert_eq!(
            primary_script(&scripts, None).map(|s| s.id.as_str()),
            Some("dev")
        );
        assert_eq!(
            primary_script(&scripts, Some("setup")).map(|s| s.id.as_str()),
            Some("setup")
        );
        assert_eq!(
            primary_script(&scripts, Some("gone")).map(|s| s.id.as_str()),
            Some("dev")
        );
        let next = upsert_script(&scripts, None, script("install", true));
        assert_eq!(next.len(), 3);
        assert_eq!(
            next.iter().filter(|s| s.run_on_worktree_create).count(),
            1,
            "only the new setup script keeps the flag"
        );
        let mut renamed = script("dev", false);
        renamed.name = "Dev server".into();
        let edited = upsert_script(&scripts, Some("dev"), renamed);
        assert_eq!(edited.len(), 2);
        assert_eq!(edited[1].name, "Dev server");
    }

    #[test]
    fn shortcut_capture() {
        let press = |key: &str, modifiers: Modifiers| ShortcutEvent::from_gpui(key, modifiers);
        let cmd_shift = Modifiers {
            meta: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            keybinding_from_event(&press("t", cmd_shift), Platform::Mac).as_deref(),
            Some("mod+shift+t")
        );
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(
            keybinding_from_event(&press("f5", ctrl), Platform::Other).as_deref(),
            Some("mod+f5")
        );
        assert_eq!(
            keybinding_from_event(&press("t", Modifiers::default()), Platform::Mac),
            None
        );
        assert_eq!(
            keybinding_from_event(&press("shift", cmd_shift), Platform::Mac),
            None
        );
        assert!(is_valid_keybinding("mod+shift+t"));
        assert!(is_valid_keybinding(""));
        assert!(!is_valid_keybinding("mod+shift"));
    }

    #[test]
    fn build_and_resolve() {
        let input = ScriptInput {
            name: "Dev".into(),
            command: "bun dev".into(),
            wait_for_setup: true,
            auto_open_preview: true,
            ..Default::default()
        };
        let dev = build_script("dev".into(), &input);
        assert_eq!(dev.run_async, None, "only setup scripts wait");
        assert_eq!(dev.auto_open_preview, None, "no preview without a URL");
        let setup = build_script(
            "setup".into(),
            &ScriptInput {
                run_on_worktree_create: true,
                preview_url: Some("http://localhost:5173".into()),
                ..input.clone()
            },
        );
        assert_eq!(setup.run_async, Some(false));
        assert_eq!(setup.auto_open_preview, Some(true));

        let settings = |other: serde_json::Value| ServerSettings {
            default_project_scripts: Some(vec![script("default", false)]),
            other: other.as_object().cloned().unwrap_or_default(),
            ..ServerSettings::default()
        };
        let own = vec![script("own", false)];
        let ids = |scripts: Vec<ProjectScript>| -> Vec<String> {
            scripts.into_iter().map(|s| s.id).collect()
        };
        let none = settings(serde_json::json!({}));
        assert_eq!(ids(resolve_project_scripts(&none, "p", &own)), ["own"]);
        assert_eq!(ids(resolve_project_scripts(&none, "p", &[])), ["default"]);
        let legacy_null = settings(serde_json::json!({"projectScriptOverrides": {"p": null}}));
        assert_eq!(ids(resolve_project_scripts(&legacy_null, "p", &own)), ["default"]);
        let folded = settings(serde_json::json!({"projectSettingsFolded": true}));
        assert_eq!(ids(resolve_project_scripts(&folded, "p", &own)), ["default"]);
        let overridden = settings(serde_json::json!({
            "projectSettingsFolded": true,
            "projectSettingsOverrides": {"p": {"defaultProjectScripts": [], "autoPull": true}}
        }));
        assert!(resolve_project_scripts(&overridden, "p", &own).is_empty());

        let patch = scripts_patch(&overridden, "p", &own, true);
        let project = &patch["projectSettingsOverrides"]["p"];
        assert_eq!(project["autoPull"], serde_json::json!(true));
        assert_eq!(project["defaultProjectScripts"][0]["id"], "own");
        let legacy = scripts_patch(&none, "p", &own, false);
        assert_eq!(legacy["projectScriptOverrides"]["p"][0]["id"], "own");
    }

    #[test]
    fn env() {
        assert_eq!(runtime_env("/repo", Some("")).len(), 1);
        assert_eq!(
            runtime_env("/repo", Some("/wt"))
                .get("T3CODE_WORKTREE_PATH")
                .map(String::as_str),
            Some("/wt")
        );
    }
}
