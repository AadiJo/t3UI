//! Persisted UI state (`web/uiStateStore.ts`, localStorage `t3code:ui-state:v1`), plus the
//! sidebar width and theme preference the web keeps under their own keys. The app writes it to
//! `ui-state.json`, debounced.
//!
//! Transitions return `true` when they changed something, so the owner only notifies and persists
//! on real changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::time::{format_timestamp, parse_timestamp};

/// Fallback key the web migrated old "all collapsed" state into. Read, never written.
const LEGACY_EXPANSION_DEFAULT_KEY: &str = "legacy-project-expansion-default";

/// Theme preference (`t3code:theme`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

/// UI state that survives restarts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiState {
    /// Expansion per preference key (logical, physical, or `legacy-project-cwd:<path>` key).
    pub project_expanded_by_id: BTreeMap<String, bool>,
    /// Manual project order, as physical project keys.
    pub project_order: Vec<String>,
    /// When each thread (`<environmentId>:<threadId>`) was last visited. Drives "Completed".
    pub thread_last_visited_at_by_id: BTreeMap<String, String>,
    /// Collapsed changed-files sections per thread and turn (only `false` entries are stored).
    pub thread_changed_files_expanded_by_id: BTreeMap<String, BTreeMap<String, bool>>,
    pub default_advertised_endpoint_key: Option<String>,
    /// Sidebar width in px (`chat_thread_sidebar_width`). `None` uses the default width.
    pub sidebar_width: Option<f32>,
    /// Right panel width in px, shared by every thread (`t3code:preview-panel-width`). `None`
    /// uses the default width.
    pub right_panel_width: Option<f32>,
    pub theme: ThemePreference,
}

impl UiState {
    /// Decodes the state file, or the default state when it is unreadable.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Whether a project is expanded: the first preference key with a stored value wins, else
    /// expanded (web `resolveProjectExpanded`).
    pub fn project_expanded(&self, preference_keys: &[String]) -> bool {
        preference_keys
            .iter()
            .find_map(|key| self.project_expanded_by_id.get(key).copied())
            .or_else(|| {
                self.project_expanded_by_id
                    .get(LEGACY_EXPANSION_DEFAULT_KEY)
                    .copied()
            })
            .unwrap_or(true)
    }

    /// Writes `expanded` to every preference key of a project.
    pub fn set_project_expanded(&mut self, preference_keys: &[String], expanded: bool) -> bool {
        let mut changed = false;
        for key in preference_keys {
            if self.project_expanded_by_id.get(key) != Some(&expanded) {
                self.project_expanded_by_id.insert(key.clone(), expanded);
                changed = true;
            }
        }
        changed
    }

    /// Last visit time of a thread key.
    pub fn last_visited_at(&self, thread_key: &str) -> Option<&str> {
        self.thread_last_visited_at_by_id
            .get(thread_key)
            .map(String::as_str)
    }

    /// Records a visit at `visited_at` unless a later visit is already stored.
    pub fn mark_thread_visited(&mut self, thread_key: &str, visited_at: &str) -> bool {
        let Some(visited) = parse_timestamp(visited_at) else {
            return false;
        };
        let previous = self.last_visited_at(thread_key).and_then(parse_timestamp);
        if previous.is_some_and(|previous| previous >= visited) {
            return false;
        }
        self.thread_last_visited_at_by_id
            .insert(thread_key.to_owned(), visited_at.to_owned());
        true
    }

    /// Marks a thread unread by storing a visit 1ms before its latest completion. No-op without a
    /// completed turn.
    pub fn mark_thread_unread(&mut self, thread_key: &str, completed_at: Option<&str>) -> bool {
        let Some(unread_at) = completed_at
            .and_then(parse_timestamp)
            .and_then(|completed| format_timestamp(completed - 1))
        else {
            return false;
        };
        if self.last_visited_at(thread_key) == Some(unread_at.as_str()) {
            return false;
        }
        self.thread_last_visited_at_by_id
            .insert(thread_key.to_owned(), unread_at);
        true
    }

    /// Whether a turn's changed-files section is expanded (default true).
    pub fn changed_files_expanded(&self, thread_key: &str, turn_id: &str) -> bool {
        self.thread_changed_files_expanded_by_id
            .get(thread_key)
            .and_then(|turns| turns.get(turn_id))
            .copied()
            .unwrap_or(true)
    }

    /// Stores only collapsed sections; expanding removes the entry.
    pub fn set_changed_files_expanded(
        &mut self,
        thread_key: &str,
        turn_id: &str,
        expanded: bool,
    ) -> bool {
        if self.changed_files_expanded(thread_key, turn_id) == expanded {
            return false;
        }
        if expanded {
            if let Some(turns) = self.thread_changed_files_expanded_by_id.get_mut(thread_key) {
                turns.remove(turn_id);
                if turns.is_empty() {
                    self.thread_changed_files_expanded_by_id.remove(thread_key);
                }
            }
        } else {
            self.thread_changed_files_expanded_by_id
                .entry(thread_key.to_owned())
                .or_default()
                .insert(turn_id.to_owned(), false);
        }
        true
    }

    /// Moves every dragged key to the target's position (web `reorderProjects`). `current_order`
    /// is the physical key of every project in its current order.
    pub fn reorder_projects(
        &mut self,
        current_order: &[String],
        dragged: &[String],
        target: &[String],
    ) -> bool {
        if dragged.is_empty() || dragged.iter().all(|key| target.contains(key)) {
            return false;
        }
        let Some(target_index) = current_order.iter().position(|key| target.contains(key)) else {
            return false;
        };
        let mut order = current_order.to_vec();
        let mut removed = Vec::new();
        let mut dragged_before_target: usize = 0;
        for index in (0..order.len()).rev() {
            if dragged.contains(&order[index]) {
                removed.insert(0, order.remove(index));
                if index < target_index {
                    dragged_before_target += 1;
                }
            }
        }
        if removed.is_empty() {
            return false;
        }
        let insert_at = target_index - dragged_before_target.saturating_sub(1);
        order.splice(insert_at..insert_at, removed);
        self.project_order = order;
        true
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: expansion falling through preference keys in the wrong order, visits moving
    //! backwards in time, "mark unread" not landing exactly 1ms before completion, changed-files
    //! entries leaking `true` values, and reorder off-by-one when dragging down vs up.
    use super::*;

    fn keys(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn expansion_uses_the_first_stored_key() {
        let mut state = UiState::default();
        let preference = keys(&["logical", "env:/a", "legacy-project-cwd:/a"]);
        assert!(state.project_expanded(&preference));
        state.project_expanded_by_id.insert("env:/a".into(), false);
        assert!(!state.project_expanded(&preference));
        state.project_expanded_by_id.insert("logical".into(), true);
        assert!(state.project_expanded(&preference));
        assert!(state.set_project_expanded(&preference, false));
        assert!(!state.set_project_expanded(&preference, false));
        assert!(
            state
                .project_expanded_by_id
                .values()
                .all(|expanded| !expanded)
        );
    }

    #[test]
    fn visits_only_move_forward_and_unread_is_one_ms_back() {
        let mut state = UiState::default();
        assert!(state.mark_thread_visited("e:t", "2026-10-01T12:00:00.000Z"));
        assert!(!state.mark_thread_visited("e:t", "2026-10-01T11:00:00.000Z"));
        assert!(!state.mark_thread_visited("e:t", "nonsense"));
        assert!(state.mark_thread_unread("e:t", Some("2026-10-01T12:30:00.000Z")));
        assert_eq!(
            state.last_visited_at("e:t"),
            Some("2026-10-01T12:29:59.999Z")
        );
        assert!(!state.mark_thread_unread("e:t", Some("2026-10-01T12:30:00.000Z")));
        assert!(!state.mark_thread_unread("e:t", None));
    }

    #[test]
    fn changed_files_store_only_collapsed_entries() {
        let mut state = UiState::default();
        assert!(!state.set_changed_files_expanded("t", "turn", true));
        assert!(state.set_changed_files_expanded("t", "turn", false));
        assert!(!state.changed_files_expanded("t", "turn"));
        assert!(state.set_changed_files_expanded("t", "turn", true));
        assert!(state.thread_changed_files_expanded_by_id.is_empty());
    }

    #[test]
    fn reorder_matches_the_web() {
        let order = keys(&["a", "b", "c", "d"]);
        let mut state = UiState::default();
        // Drag down: a onto c lands where c was.
        assert!(state.reorder_projects(&order, &keys(&["a"]), &keys(&["c"])));
        assert_eq!(state.project_order, keys(&["b", "c", "a", "d"]));
        // Drag up: d onto b lands before b.
        assert!(state.reorder_projects(&order, &keys(&["d"]), &keys(&["b"])));
        assert_eq!(state.project_order, keys(&["a", "d", "b", "c"]));
        // Grouped members move together and land after the target when dragged down.
        assert!(state.reorder_projects(&order, &keys(&["a", "b"]), &keys(&["d"])));
        assert_eq!(state.project_order, keys(&["c", "d", "a", "b"]));
        assert!(!state.reorder_projects(&order, &keys(&["a"]), &keys(&["a"])));
        assert!(!state.reorder_projects(&order, &keys(&["a"]), &keys(&["zz"])));
    }

    #[test]
    fn decodes_partial_files() {
        let state =
            UiState::from_json(r#"{"projectOrder":["x"],"sidebarWidth":300,"theme":"dark"}"#);
        assert_eq!(state.project_order, keys(&["x"]));
        assert_eq!(state.sidebar_width, Some(300.));
        assert_eq!(state.theme, ThemePreference::Dark);
        assert_eq!(UiState::from_json("{"), UiState::default());
    }
}
