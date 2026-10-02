//! Terminal drawer layout per thread (web `terminalUiStateStore.ts`, `terminalLabels.ts`):
//! which terminals exist, how they are grouped into splits, which one is active, whether the
//! drawer is open and how tall it is. Persisted like the web's `t3code:terminal-state:v1`.
//!
//! Every transition normalizes first (dedupes ids, drops groups that lost all members, picks a
//! valid active terminal and group) and returns whether anything changed, so the owner only
//! notifies and writes on real changes.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Drawer height before the user resizes it (`DEFAULT_THREAD_TERMINAL_HEIGHT`).
pub const DEFAULT_TERMINAL_HEIGHT: f32 = 280.;
/// Smallest drawer height a drag can reach.
pub const MIN_DRAWER_HEIGHT: f32 = 180.;
/// The drawer never grows past this fraction of the window height.
pub const MAX_DRAWER_HEIGHT_RATIO: f32 = 0.75;
/// A split beyond this many terminals in one group is a no-op.
pub const MAX_TERMINALS_PER_GROUP: usize = 4;
/// The terminal created when the drawer opens with none.
pub const DEFAULT_TERMINAL_ID: &str = "term-1";

/// How a group lays out its terminals: `Horizontal` is side-by-side columns, `Vertical` is
/// stacked rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    #[default]
    Horizontal,
    Vertical,
}

impl SplitDirection {
    fn is_horizontal(&self) -> bool {
        *self == Self::Horizontal
    }
}

/// Terminals shown together in a split.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalGroup {
    pub id: String,
    pub terminal_ids: Vec<String>,
    /// Only `vertical` is written; a missing value means horizontal.
    #[serde(default, skip_serializing_if = "SplitDirection::is_horizontal")]
    pub split_direction: SplitDirection,
}

/// One thread's drawer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadTerminalLayout {
    pub terminal_open: bool,
    pub terminal_height: f32,
    /// Creation order; the sidebar lists groups by their earliest member in this order.
    pub terminal_ids: Vec<String>,
    pub active_terminal_id: String,
    pub terminal_groups: Vec<TerminalGroup>,
    pub active_terminal_group_id: String,
}

impl Default for ThreadTerminalLayout {
    fn default() -> Self {
        Self {
            terminal_open: false,
            terminal_height: DEFAULT_TERMINAL_HEIGHT,
            terminal_ids: Vec::new(),
            active_terminal_id: String::new(),
            terminal_groups: Vec::new(),
            active_terminal_group_id: String::new(),
        }
    }
}

impl ThreadTerminalLayout {
    /// The active group, the one the drawer shows.
    pub fn active_group(&self) -> Option<&TerminalGroup> {
        self.terminal_groups
            .iter()
            .find(|group| group.id == self.active_terminal_group_id)
            .or_else(|| {
                self.terminal_groups
                    .iter()
                    .find(|group| group.terminal_ids.contains(&self.active_terminal_id))
            })
            .or(self.terminal_groups.first())
    }

    /// Terminals rendered side by side (or stacked) right now.
    pub fn visible_terminal_ids(&self) -> &[String] {
        self.active_group()
            .map_or(&[][..], |group| group.terminal_ids.as_slice())
    }

    /// Splitting is disabled once the active group is full.
    pub fn split_limit_reached(&self) -> bool {
        self.visible_terminal_ids().len() >= MAX_TERMINALS_PER_GROUP
    }

    /// Group headers show once there is more than one group or any split.
    pub fn shows_group_headers(&self) -> bool {
        self.terminal_groups.len() > 1
            || self
                .terminal_groups
                .iter()
                .any(|group| group.terminal_ids.len() > 1)
    }

    /// Deduped ids, groups holding only known terminals (each once), an active terminal and
    /// group that exist, and a positive height (`normalizeThreadTerminalUiState`).
    fn normalized(&self) -> Self {
        let terminal_ids = dedupe(&self.terminal_ids);
        let active_terminal_id = if terminal_ids.contains(&self.active_terminal_id) {
            self.active_terminal_id.clone()
        } else {
            terminal_ids.first().cloned().unwrap_or_default()
        };
        let terminal_groups = normalize_groups(&self.terminal_groups, &terminal_ids);
        let active_terminal_group_id = terminal_groups
            .iter()
            .find(|group| group.id == self.active_terminal_group_id)
            .or_else(|| {
                terminal_groups
                    .iter()
                    .find(|group| group.terminal_ids.contains(&active_terminal_id))
            })
            .or(terminal_groups.first())
            .map(|group| group.id.clone())
            .unwrap_or_default();
        Self {
            terminal_open: self.terminal_open,
            terminal_height: if self.terminal_height.is_finite() && self.terminal_height > 0. {
                self.terminal_height
            } else {
                DEFAULT_TERMINAL_HEIGHT
            },
            terminal_ids,
            active_terminal_id,
            terminal_groups,
            active_terminal_group_id,
        }
    }

    fn is_default(&self) -> bool {
        self.normalized() == Self::default()
    }

    /// `upsertTerminalIntoGroups`: `new` adds a group, `split` inserts after the active
    /// terminal of the active group (no-op when that group is full). Opens the drawer and
    /// activates `terminal_id`.
    fn upsert(&self, terminal_id: &str, split: Option<SplitDirection>) -> Self {
        let normalized = self.normalized();
        let terminal_id = terminal_id.trim();
        if terminal_id.is_empty() {
            return normalized;
        }
        let split = if normalized.terminal_ids.is_empty() {
            None
        } else {
            split
        };
        let is_new = !normalized.terminal_ids.iter().any(|id| id == terminal_id);
        let mut terminal_ids = normalized.terminal_ids.clone();
        if is_new {
            terminal_ids.push(terminal_id.to_owned());
        }
        let mut groups = normalized.terminal_groups.clone();
        // A terminal lives in one group: take it out of its current one first.
        if let Some(index) = group_index(&groups, terminal_id) {
            groups[index].terminal_ids.retain(|id| id != terminal_id);
            if groups[index].terminal_ids.is_empty() {
                groups.remove(index);
            }
        }

        let Some(direction) = split else {
            let mut used: HashSet<String> = groups.iter().map(|group| group.id.clone()).collect();
            let group_id = unique_group_id(&fallback_group_id(terminal_id), &mut used);
            groups.push(TerminalGroup {
                id: group_id.clone(),
                terminal_ids: vec![terminal_id.to_owned()],
                split_direction: SplitDirection::Horizontal,
            });
            return Self {
                terminal_open: true,
                terminal_ids,
                active_terminal_id: terminal_id.to_owned(),
                terminal_groups: groups,
                active_terminal_group_id: group_id,
                ..normalized
            }
            .normalized();
        };

        let mut active_index = groups
            .iter()
            .position(|group| group.id == normalized.active_terminal_group_id)
            .or_else(|| group_index(&groups, &normalized.active_terminal_id));
        if active_index.is_none() {
            let mut used: HashSet<String> = groups.iter().map(|group| group.id.clone()).collect();
            groups.push(TerminalGroup {
                id: unique_group_id(
                    &fallback_group_id(&normalized.active_terminal_id),
                    &mut used,
                ),
                terminal_ids: vec![normalized.active_terminal_id.clone()],
                split_direction: SplitDirection::Horizontal,
            });
            active_index = Some(groups.len() - 1);
        }
        let Some(destination) = active_index.and_then(|index| groups.get_mut(index)) else {
            return normalized;
        };
        let contains = destination.terminal_ids.iter().any(|id| id == terminal_id);
        if is_new && !contains && destination.terminal_ids.len() >= MAX_TERMINALS_PER_GROUP {
            return normalized;
        }
        if !contains {
            match destination
                .terminal_ids
                .iter()
                .position(|id| *id == normalized.active_terminal_id)
            {
                Some(anchor) => destination
                    .terminal_ids
                    .insert(anchor + 1, terminal_id.to_owned()),
                None => destination.terminal_ids.push(terminal_id.to_owned()),
            }
        }
        destination.split_direction = direction;
        let group_id = destination.id.clone();
        Self {
            terminal_open: true,
            terminal_ids,
            active_terminal_id: terminal_id.to_owned(),
            terminal_groups: groups,
            active_terminal_group_id: group_id,
            ..normalized
        }
        .normalized()
    }

    fn with_open(&self, open: bool) -> Self {
        let normalized = self.normalized();
        if open && normalized.terminal_ids.is_empty() {
            return normalized.upsert(DEFAULT_TERMINAL_ID, None);
        }
        Self {
            terminal_open: open,
            ..normalized
        }
    }

    fn with_active(&self, terminal_id: &str) -> Self {
        let normalized = self.normalized();
        if !normalized.terminal_ids.iter().any(|id| id == terminal_id) {
            return normalized;
        }
        let group_id = group_index(&normalized.terminal_groups, terminal_id).map_or_else(
            || normalized.active_terminal_group_id.clone(),
            |index| normalized.terminal_groups[index].id.clone(),
        );
        Self {
            active_terminal_id: terminal_id.to_owned(),
            active_terminal_group_id: group_id,
            ..normalized
        }
    }

    /// `closeThreadTerminal`: the next active terminal is the one now at the closed index,
    /// clamped. Closing the last terminal resets the drawer (closed, default height kept).
    fn without(&self, terminal_id: &str) -> Self {
        let normalized = self.normalized();
        let Some(closed_index) = normalized
            .terminal_ids
            .iter()
            .position(|id| id == terminal_id)
        else {
            return normalized;
        };
        let remaining: Vec<String> = normalized
            .terminal_ids
            .iter()
            .filter(|id| *id != terminal_id)
            .cloned()
            .collect();
        if remaining.is_empty() {
            return Self::default();
        }
        let active_terminal_id = if normalized.active_terminal_id == terminal_id {
            remaining[closed_index.min(remaining.len() - 1)].clone()
        } else {
            normalized.active_terminal_id.clone()
        };
        let groups: Vec<TerminalGroup> = normalized
            .terminal_groups
            .iter()
            .filter_map(|group| {
                let ids: Vec<String> = group
                    .terminal_ids
                    .iter()
                    .filter(|id| *id != terminal_id)
                    .cloned()
                    .collect();
                (!ids.is_empty()).then(|| TerminalGroup {
                    terminal_ids: ids,
                    ..group.clone()
                })
            })
            .collect();
        let active_terminal_group_id = groups
            .iter()
            .find(|group| group.terminal_ids.contains(&active_terminal_id))
            .or(groups.first())
            .map_or_else(
                || fallback_group_id(&active_terminal_id),
                |group| group.id.clone(),
            );
        Self {
            terminal_open: normalized.terminal_open,
            terminal_height: normalized.terminal_height,
            terminal_ids: remaining,
            active_terminal_id,
            terminal_groups: groups,
            active_terminal_group_id,
        }
        .normalized()
    }

    /// `reconcileThreadTerminalSessionIds`: adopt the server's id list, keeping groups.
    fn reconciled(&self, ids: &[String]) -> Self {
        let normalized = self.normalized();
        if normalized.terminal_ids == ids {
            return normalized;
        }
        let active_terminal_id = if ids.contains(&normalized.active_terminal_id) {
            normalized.active_terminal_id.clone()
        } else {
            ids.first().cloned().unwrap_or_default()
        };
        let groups = normalize_groups(&normalized.terminal_groups, ids);
        let active_terminal_group_id = groups
            .iter()
            .find(|group| group.terminal_ids.contains(&active_terminal_id))
            .or(groups.first())
            .map(|group| group.id.clone())
            .unwrap_or_default();
        Self {
            terminal_ids: ids.to_vec(),
            active_terminal_id,
            terminal_groups: groups,
            active_terminal_group_id,
            ..normalized
        }
        .normalized()
    }
}

/// Every thread's drawer layout, keyed by `ThreadRef::key()`, plus the ids closed this session
/// that stale server metadata must not bring back (not persisted).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalLayouts {
    by_thread: BTreeMap<String, ThreadTerminalLayout>,
    suppressed: BTreeMap<String, Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRef<'a> {
    terminal_ui_state_by_thread_key: &'a BTreeMap<String, ThreadTerminalLayout>,
}

impl TerminalLayouts {
    /// Decodes the state file. Unreadable files and malformed entries are skipped, never fatal.
    /// Accepts the web's older `terminalStateByThreadKey` name too.
    pub fn from_json(json: &str) -> Self {
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(json) else {
            return Self::default();
        };
        let entries = root
            .get("terminalUiStateByThreadKey")
            .or_else(|| root.get("terminalStateByThreadKey"));
        let by_thread = match entries {
            Some(Value::Object(entries)) => entries
                .iter()
                .filter(|(key, _)| key.contains(':'))
                .filter_map(|(key, value)| {
                    let layout: ThreadTerminalLayout =
                        serde_json::from_value(value.clone()).ok()?;
                    let layout = layout.normalized();
                    (!layout.is_default()).then(|| (key.clone(), layout))
                })
                .collect(),
            _ => BTreeMap::new(),
        };
        Self {
            by_thread,
            suppressed: BTreeMap::new(),
        }
    }

    /// The persisted form (`{terminalUiStateByThreadKey: {...}}`).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&PersistedRef {
            terminal_ui_state_by_thread_key: &self.by_thread,
        })
        .unwrap_or_default()
    }

    /// A thread's layout, or the default (closed, no terminals).
    pub fn layout(&self, thread_key: &str) -> ThreadTerminalLayout {
        self.by_thread.get(thread_key).cloned().unwrap_or_default()
    }

    /// Opens or closes the drawer. Opening with no terminals creates `term-1`.
    pub fn set_open(&mut self, thread_key: &str, open: bool) -> bool {
        if open && self.layout(thread_key).terminal_ids.is_empty() {
            self.unsuppress(thread_key, DEFAULT_TERMINAL_ID);
        }
        self.update(thread_key, |layout| layout.with_open(open))
    }

    /// Stores the height a drag ended at. Ignores non-positive or non-finite values.
    pub fn set_height(&mut self, thread_key: &str, height: f32) -> bool {
        if !height.is_finite() || height <= 0. {
            return false;
        }
        self.update(thread_key, |layout| ThreadTerminalLayout {
            terminal_height: height,
            ..layout.normalized()
        })
    }

    /// Adds `terminal_id` to the active group, after the active terminal.
    pub fn split(
        &mut self,
        thread_key: &str,
        terminal_id: &str,
        direction: SplitDirection,
    ) -> bool {
        self.unsuppress(thread_key, terminal_id);
        self.update(thread_key, |layout| {
            layout.upsert(terminal_id, Some(direction))
        })
    }

    /// Adds `terminal_id` in a new group of its own.
    pub fn new_terminal(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        self.unsuppress(thread_key, terminal_id);
        self.update(thread_key, |layout| layout.upsert(terminal_id, None))
    }

    /// `ensureTerminal`: adds `terminal_id` if missing (a new group, which opens the drawer
    /// like the web does), activates it unless `activate` is false, and opens the drawer when
    /// `open`.
    pub fn ensure(
        &mut self,
        thread_key: &str,
        terminal_id: &str,
        open: bool,
        activate: bool,
    ) -> bool {
        self.unsuppress(thread_key, terminal_id);
        self.update(thread_key, |layout| {
            let mut next = layout.clone();
            // Like the web, adding a terminal opens the drawer even without `open`.
            if !layout.terminal_ids.iter().any(|id| id == terminal_id) {
                next = next.upsert(terminal_id, None);
            }
            if activate {
                next = next.with_active(terminal_id);
            } else {
                next.active_terminal_id = layout.active_terminal_id.clone();
                next.active_terminal_group_id = layout.active_terminal_group_id.clone();
            }
            if open {
                next = next.with_open(true);
            }
            next.normalized()
        })
    }

    pub fn set_active(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        self.update(thread_key, |layout| layout.with_active(terminal_id))
    }

    /// Removes a terminal and keeps stale metadata from re-adding it.
    pub fn close(&mut self, thread_key: &str, terminal_id: &str) -> bool {
        let changed = self.update(thread_key, |layout| layout.without(terminal_id));
        let ids = self.suppressed.entry(thread_key.to_owned()).or_default();
        if !ids.iter().any(|id| id == terminal_id) {
            ids.push(terminal_id.to_owned());
        }
        changed
    }

    /// Adopts the server's terminal list for a thread (ChatView's reconcile effect): skipped when
    /// the lists hold the same ids, or when the server knows a strict subset of ours (it has
    /// not reported terminals we just opened yet). Suppressed ids are ignored.
    pub fn reconcile(&mut self, thread_key: &str, server_ids: &[String]) -> bool {
        let client_ids = self.layout(thread_key).terminal_ids;
        if same_ids(server_ids, &client_ids) || strict_subset(server_ids, &client_ids) {
            return false;
        }
        let suppressed = self.suppressed.get(thread_key).cloned().unwrap_or_default();
        let ids: Vec<String> = server_ids
            .iter()
            .filter(|id| !suppressed.contains(id))
            .cloned()
            .collect();
        self.update(thread_key, |layout| layout.reconciled(&ids))
    }

    /// Forgets a thread (deleted or archived).
    pub fn remove(&mut self, thread_key: &str) -> bool {
        self.suppressed.remove(thread_key);
        self.by_thread.remove(thread_key).is_some()
    }

    fn unsuppress(&mut self, thread_key: &str, terminal_id: &str) {
        if let Some(ids) = self.suppressed.get_mut(thread_key) {
            ids.retain(|id| id != terminal_id);
            if ids.is_empty() {
                self.suppressed.remove(thread_key);
            }
        }
    }

    /// Applies `edit` to a thread's layout; default layouts are dropped from the map.
    fn update(
        &mut self,
        thread_key: &str,
        edit: impl FnOnce(&ThreadTerminalLayout) -> ThreadTerminalLayout,
    ) -> bool {
        if thread_key.is_empty() {
            return false;
        }
        let current = self.layout(thread_key);
        let next = edit(&current);
        if next == current {
            return false;
        }
        if next.is_default() {
            return self.by_thread.remove(thread_key).is_some();
        }
        self.by_thread.insert(thread_key.to_owned(), next);
        true
    }
}

/// The lowest unused `term-N` (`nextTerminalId`). Ids are always chosen by the client.
pub fn next_terminal_id<'a>(existing: impl IntoIterator<Item = &'a str>) -> String {
    let used: HashSet<&str> = existing.into_iter().map(str::trim).collect();
    (1..)
        .map(|index| format!("term-{index}"))
        .find(|id| !used.contains(id.as_str()))
        .unwrap_or_else(|| DEFAULT_TERMINAL_ID.to_owned())
}

/// `term-3` / `terminal-3` read "Terminal 3"; other ids show as-is (`getTerminalLabel`).
pub fn terminal_label(terminal_id: &str) -> String {
    let lower = terminal_id.to_ascii_lowercase();
    let suffix = lower
        .strip_prefix("terminal-")
        .or_else(|| lower.strip_prefix("term-"));
    match suffix {
        Some(digits) if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) => {
            format!(
                "Terminal {}",
                &terminal_id[terminal_id.len() - digits.len()..]
            )
        }
        _ => terminal_id.to_owned(),
    }
}

/// The server's label (e.g. the running command) when it has one, else [`terminal_label`].
pub fn session_label(terminal_id: &str, server_label: Option<&str>) -> String {
    match server_label.map(str::trim) {
        Some(label) if !label.is_empty() => label.to_owned(),
        _ => terminal_label(terminal_id),
    }
}

/// Clamps a drawer height to `[180, max(180, floor(0.75 × window))]`, rounding like the web.
/// Non-finite heights fall back to the default.
pub fn clamp_drawer_height(height: f32, window_height: f32) -> f32 {
    let height = if height.is_finite() {
        height
    } else {
        DEFAULT_TERMINAL_HEIGHT
    };
    let max = MIN_DRAWER_HEIGHT.max((window_height * MAX_DRAWER_HEIGHT_RATIO).floor());
    height.round().max(MIN_DRAWER_HEIGHT).min(max)
}

fn dedupe(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && seen.insert(*id))
        .map(str::to_owned)
        .collect()
}

fn fallback_group_id(terminal_id: &str) -> String {
    format!("group-{terminal_id}")
}

/// `base`, or `base-2`, `base-3`… when taken. Records the result in `used`.
fn unique_group_id(base: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = base.to_owned();
    let mut index = 2;
    while used.contains(&candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn group_index(groups: &[TerminalGroup], terminal_id: &str) -> Option<usize> {
    groups
        .iter()
        .position(|group| group.terminal_ids.iter().any(|id| id == terminal_id))
}

/// `normalizeTerminalGroups`: each known terminal in exactly one group, unknown ids dropped,
/// ungrouped terminals in groups of their own, unique group ids.
fn normalize_groups(groups: &[TerminalGroup], terminal_ids: &[String]) -> Vec<TerminalGroup> {
    if terminal_ids.is_empty() {
        return Vec::new();
    }
    let valid: HashSet<&str> = terminal_ids.iter().map(String::as_str).collect();
    let mut assigned: HashSet<String> = HashSet::new();
    let mut used = HashSet::new();
    let mut next = Vec::new();
    for group in groups {
        let ids: Vec<String> = dedupe(&group.terminal_ids)
            .into_iter()
            .filter(|id| valid.contains(id.as_str()) && !assigned.contains(id))
            .collect();
        let Some(first) = ids.first() else {
            continue;
        };
        let base = if group.id.trim().is_empty() {
            fallback_group_id(first)
        } else {
            group.id.trim().to_owned()
        };
        assigned.extend(ids.iter().cloned());
        next.push(TerminalGroup {
            id: unique_group_id(&base, &mut used),
            terminal_ids: ids,
            split_direction: group.split_direction,
        });
    }
    for id in terminal_ids {
        if !assigned.contains(id) {
            next.push(TerminalGroup {
                id: unique_group_id(&fallback_group_id(id), &mut used),
                terminal_ids: vec![id.clone()],
                split_direction: SplitDirection::Horizontal,
            });
        }
    }
    next
}

fn same_ids(left: &[String], right: &[String]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort();
    right.sort();
    left == right
}

fn strict_subset(server: &[String], client: &[String]) -> bool {
    !client.is_empty() && server.len() < client.len() && server.iter().all(|id| client.contains(id))
}

#[cfg(test)]
mod tests {
    //! Failure modes: opening an empty drawer without creating `term-1`; a split growing a full
    //! group past 4 or landing at the end instead of after the active terminal; a vertical
    //! split direction surviving a later horizontal split; closing the active terminal picking
    //! the wrong neighbor or leaving empty groups; closing the last terminal leaving an open,
    //! empty drawer; colliding group ids; reconcile dropping the active terminal or re-adding
    //! closed ones from stale metadata; the web's JSON shape not round-tripping or a malformed
    //! entry poisoning the file; `next_terminal_id` skipping gaps; heights escaping the clamp.
    use super::*;

    const KEY: &str = "env:thread";

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn group(layouts: &TerminalLayouts, index: usize) -> TerminalGroup {
        layouts.layout(KEY).terminal_groups[index].clone()
    }

    #[test]
    fn opening_an_empty_drawer_creates_term_1() {
        let mut layouts = TerminalLayouts::default();
        assert!(layouts.set_open(KEY, true));
        let layout = layouts.layout(KEY);
        assert!(layout.terminal_open);
        assert_eq!(layout.terminal_ids, ids(&["term-1"]));
        assert_eq!(layout.active_terminal_id, "term-1");
        assert_eq!(layout.active_terminal_group_id, "group-term-1");
        assert!(!layouts.set_open(KEY, true), "already open");
        assert!(layouts.set_open(KEY, false));
        assert!(!layouts.layout(KEY).terminal_open);
        assert_eq!(layouts.layout(KEY).terminal_ids, ids(&["term-1"]));
    }

    #[test]
    fn split_inserts_after_the_active_terminal_and_stops_at_four() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.split(KEY, "term-2", SplitDirection::Horizontal);
        layouts.set_active(KEY, "term-1");
        layouts.split(KEY, "term-3", SplitDirection::Horizontal);
        assert_eq!(
            group(&layouts, 0).terminal_ids,
            ids(&["term-1", "term-3", "term-2"])
        );
        assert_eq!(layouts.layout(KEY).active_terminal_id, "term-3");
        layouts.split(KEY, "term-4", SplitDirection::Horizontal);
        assert!(layouts.layout(KEY).split_limit_reached());
        assert!(!layouts.split(KEY, "term-5", SplitDirection::Horizontal));
        assert_eq!(layouts.layout(KEY).terminal_ids.len(), 4);
        assert_eq!(layouts.layout(KEY).terminal_groups.len(), 1);
    }

    #[test]
    fn split_direction_follows_the_latest_split() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.split(KEY, "term-2", SplitDirection::Vertical);
        assert_eq!(group(&layouts, 0).split_direction, SplitDirection::Vertical);
        layouts.split(KEY, "term-3", SplitDirection::Horizontal);
        assert_eq!(
            group(&layouts, 0).split_direction,
            SplitDirection::Horizontal
        );
    }

    #[test]
    fn new_terminal_gets_its_own_group_and_headers_show() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        assert!(!layouts.layout(KEY).shows_group_headers());
        layouts.new_terminal(KEY, "term-2");
        let layout = layouts.layout(KEY);
        assert_eq!(layout.terminal_groups.len(), 2);
        assert_eq!(layout.active_terminal_group_id, "group-term-2");
        assert_eq!(layout.visible_terminal_ids(), &ids(&["term-2"])[..]);
        assert!(layout.shows_group_headers());
    }

    #[test]
    fn closing_the_active_terminal_activates_its_neighbor() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.new_terminal(KEY, "term-2");
        layouts.new_terminal(KEY, "term-3");
        layouts.set_active(KEY, "term-2");
        assert!(layouts.close(KEY, "term-2"));
        let layout = layouts.layout(KEY);
        assert_eq!(layout.terminal_ids, ids(&["term-1", "term-3"]));
        assert_eq!(layout.active_terminal_id, "term-3");
        assert_eq!(layout.active_terminal_group_id, "group-term-3");
        assert_eq!(layout.terminal_groups.len(), 2, "empty group removed");
        layouts.close(KEY, "term-3");
        assert_eq!(layouts.layout(KEY).active_terminal_id, "term-1");
    }

    #[test]
    fn closing_the_last_terminal_resets_and_closes_the_drawer() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.set_height(KEY, 400.);
        assert!(layouts.close(KEY, "term-1"));
        assert_eq!(layouts.layout(KEY), ThreadTerminalLayout::default());
        assert_eq!(layouts.to_json().matches("env:thread").count(), 0);
    }

    #[test]
    fn group_ids_stay_unique() {
        let mut layouts = TerminalLayouts::default();
        layouts.by_thread.insert(
            KEY.into(),
            ThreadTerminalLayout {
                terminal_ids: ids(&["term-1", "term-2"]),
                terminal_groups: vec![
                    TerminalGroup {
                        id: "group-term-1".into(),
                        terminal_ids: ids(&["term-1"]),
                        split_direction: SplitDirection::Horizontal,
                    },
                    TerminalGroup {
                        id: "group-term-1".into(),
                        terminal_ids: ids(&["term-2", "term-1"]),
                        split_direction: SplitDirection::Vertical,
                    },
                ],
                ..Default::default()
            },
        );
        layouts.set_active(KEY, "term-2");
        let layout = layouts.layout(KEY);
        assert_eq!(layout.terminal_groups[0].id, "group-term-1");
        assert_eq!(layout.terminal_groups[1].id, "group-term-1-2");
        assert_eq!(layout.terminal_groups[1].terminal_ids, ids(&["term-2"]));
    }

    #[test]
    fn reconcile_adopts_new_server_ids_but_not_closed_ones() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.new_terminal(KEY, "term-2");
        // The server has not reported term-2 yet: keep ours.
        assert!(!layouts.reconcile(KEY, &ids(&["term-1"])));
        // Nothing reported at all (fresh server): keep ours, attach reopens them.
        assert!(!layouts.reconcile(KEY, &[]));
        // The server knows a terminal we don't: adopt the server list.
        assert!(layouts.reconcile(KEY, &ids(&["term-1", "term-2", "term-7"])));
        assert_eq!(
            layouts.layout(KEY).terminal_ids,
            ids(&["term-1", "term-2", "term-7"])
        );
        assert_eq!(layouts.layout(KEY).active_terminal_id, "term-2");
        layouts.close(KEY, "term-7");
        assert!(!layouts.reconcile(KEY, &ids(&["term-1", "term-2", "term-7"])));
        assert_eq!(layouts.layout(KEY).terminal_ids, ids(&["term-1", "term-2"]));
        // Opening the id again lifts the suppression.
        layouts.new_terminal(KEY, "term-7");
        layouts.close(KEY, "term-2");
        assert!(layouts.reconcile(KEY, &ids(&["term-1", "term-2", "term-3", "term-7"])));
        assert_eq!(
            layouts.layout(KEY).terminal_ids,
            ids(&["term-1", "term-3", "term-7"])
        );
    }

    #[test]
    fn reconcile_replaces_a_vanished_active_terminal() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.new_terminal(KEY, "term-2");
        assert!(layouts.reconcile(KEY, &ids(&["term-3", "term-4"])));
        let layout = layouts.layout(KEY);
        assert_eq!(layout.active_terminal_id, "term-3");
        assert_eq!(layout.active_terminal_group_id, "group-term-3");
    }

    #[test]
    fn json_round_trips_the_web_shape_and_skips_bad_entries() {
        let json = r#"{
            "terminalUiStateByThreadKey": {
                "env:a": {
                    "terminalOpen": true, "terminalHeight": 320,
                    "terminalIds": ["term-1", "term-2"], "activeTerminalId": "term-2",
                    "terminalGroups": [{"id": "group-term-1", "terminalIds": ["term-1", "term-2"], "splitDirection": "vertical"}],
                    "activeTerminalGroupId": "group-term-1"
                },
                "env:b": {"terminalIds": 7},
                "no-colon": {"terminalOpen": true, "terminalIds": ["term-1"]},
                "env:c": {"terminalOpen": false}
            }
        }"#;
        let layouts = TerminalLayouts::from_json(json);
        let layout = layouts.layout("env:a");
        assert_eq!(layout.terminal_height, 320.);
        assert_eq!(
            layout.terminal_groups[0].split_direction,
            SplitDirection::Vertical
        );
        assert_eq!(layouts.layout("env:b"), ThreadTerminalLayout::default());
        assert_eq!(layouts.layout("no-colon"), ThreadTerminalLayout::default());
        let written = layouts.to_json();
        assert!(written.contains("\"splitDirection\": \"vertical\""));
        assert!(!written.contains("env:c"));
        assert_eq!(TerminalLayouts::from_json(&written), layouts);
        assert_eq!(
            TerminalLayouts::from_json("not json"),
            TerminalLayouts::default()
        );
    }

    #[test]
    fn ensure_adds_without_stealing_focus_when_asked() {
        let mut layouts = TerminalLayouts::default();
        layouts.set_open(KEY, true);
        layouts.ensure(KEY, "term-2", false, false);
        let layout = layouts.layout(KEY);
        assert_eq!(layout.active_terminal_id, "term-1");
        assert_eq!(layout.terminal_ids, ids(&["term-1", "term-2"]));
        layouts.set_open(KEY, false);
        assert!(!layouts.ensure(KEY, "term-2", false, true) || !layouts.layout(KEY).terminal_open);
        layouts.ensure(KEY, "term-2", true, true);
        assert!(layouts.layout(KEY).terminal_open);
        assert_eq!(layouts.layout(KEY).active_terminal_id, "term-2");
    }

    #[test]
    fn ids_labels_and_heights() {
        assert_eq!(next_terminal_id(["term-1", "term-3"]), "term-2");
        assert_eq!(next_terminal_id(Vec::<&str>::new()), "term-1");
        assert_eq!(terminal_label("term-12"), "Terminal 12");
        assert_eq!(terminal_label("Terminal-4"), "Terminal 4");
        assert_eq!(terminal_label("term-x"), "term-x");
        assert_eq!(session_label("term-2", Some("  ")), "Terminal 2");
        assert_eq!(session_label("term-2", Some("vim")), "vim");
        assert_eq!(clamp_drawer_height(100., 900.), 180.);
        assert_eq!(clamp_drawer_height(1000., 900.), 675.);
        assert_eq!(clamp_drawer_height(300.4, 900.), 300.);
        assert_eq!(clamp_drawer_height(300., 200.), 180.);
        assert_eq!(clamp_drawer_height(f32::NAN, 900.), 280.);
    }
}
