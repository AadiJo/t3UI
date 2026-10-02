//! Project scripts ("actions") in the chat header (web `projectScripts.ts`,
//! `shared/projectScripts.ts`, `lib/projectScriptKeybindings.ts`, and the shortcut capture of
//! `KeybindingsSettings.logic.ts`).

use std::collections::{BTreeMap, HashSet};

use t3_protocol::{orchestration::ProjectScript, server::ResolvedKeybindingRule};

use crate::keybindings::{Platform, ShortcutEvent, parse_shortcut};

/// Script ids are at most this long (`MAX_SCRIPT_ID_LENGTH`).
pub const MAX_SCRIPT_ID_LENGTH: usize = 24;

/// The keybinding command that runs a script.
pub fn script_command(script_id: &str) -> String {
    format!("script.{script_id}.run")
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
    //! pushing ids past the limit, the setup script flag left on two scripts, an edited script
    //! appended instead of replaced, shortcuts captured from bare modifiers or with `mod` mapped
    //! to the wrong physical key per platform, and the worktree env var set for a blank path.
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
        assert_eq!(script_command("lint"), "script.lint.run");
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
