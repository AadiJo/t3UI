//! The composer command menu: items for the active trigger, their grouping, the highlighted row,
//! and the text a selection writes (`ChatComposer.tsx` `composerMenuItems`, `onSelectComposerItem`,
//! `ComposerCommandMenu.tsx`, `composerMenuHighlight.ts`).

use t3_protocol::{
    projects::{EntryKind, ProjectEntry},
    server::ServerProvider,
};

use super::{
    prompt::{Trigger, TriggerKind, basename, parent_dir, serialize_file_link, serialize_skill},
    search::{
        normalize_query, rank, score_slash_command, search_skills, skill_display_name,
        skill_install_source,
    },
};

/// `projects.searchEntries` page size for `@` mentions.
pub const PATH_SEARCH_LIMIT: u32 = 80;
/// Debounce between typing an `@` query and searching.
pub const PATH_SEARCH_DEBOUNCE_MS: u64 = 120;

/// One menu row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable id: `path:<kind>:<path>`, `slash:model`, `provider-slash-command:<instance>:<name>`,
    /// `skill:<instance>:<name>`.
    pub id: String,
    pub action: MenuAction,
    pub label: String,
    /// Muted text after the label.
    pub description: String,
    /// Right-aligned source label for skills ("App", "Personal", ...).
    pub source: Option<String>,
}

/// What selecting a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Insert a file chip.
    Path { path: String, kind: EntryKind },
    /// Built-in `/model`: clear the trigger and open the model picker.
    OpenModelPicker,
    /// Insert `/name `.
    ProviderCommand { name: String },
    /// Insert a skill chip.
    Skill { name: String },
}

impl MenuItem {
    /// Leading glyph of the row.
    pub fn icon(&self) -> MenuIcon {
        match &self.action {
            MenuAction::Path { path, kind } => MenuIcon::Entry {
                path: path.clone(),
                directory: *kind == EntryKind::Directory,
            },
            MenuAction::OpenModelPicker => MenuIcon::Bot,
            MenuAction::ProviderCommand { .. } | MenuAction::Skill { .. } => MenuIcon::Cube,
        }
    }

    /// The text that replaces the trigger, and whether it ends in a chip. `None` for `/model`,
    /// which only clears the trigger.
    pub fn replacement(&self) -> Option<Replacement> {
        Some(match &self.action {
            MenuAction::Path { path, .. } => Replacement::Chip {
                token: serialize_file_link(path),
            },
            MenuAction::Skill { name } => Replacement::Chip {
                token: serialize_skill(name),
            },
            MenuAction::ProviderCommand { name } => Replacement::Text(format!("/{name} ")),
            MenuAction::OpenModelPicker => return None,
        })
    }
}

/// How a selection rewrites the trigger range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Replacement {
    /// An atomic chip followed by one space (an existing space after the trigger is reused).
    Chip { token: String },
    /// Plain text.
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuIcon {
    /// Pierre file or folder icon for `path`.
    Entry { path: String, directory: bool },
    /// Lucide `Bot`.
    Bot,
    /// The cube glyph of provider commands and skills.
    Cube,
}

/// Builds the rows for `trigger`. `provider` is the composer's selected instance; `entries` is
/// the latest `projects.searchEntries` result for a path trigger.
pub fn menu_items(
    trigger: &Trigger,
    provider: Option<&ServerProvider>,
    entries: &[ProjectEntry],
) -> Vec<MenuItem> {
    let instance = provider.map_or("", |provider| provider.instance_id.as_str());
    match trigger.kind {
        TriggerKind::Path => entries
            .iter()
            .map(|entry| MenuItem {
                id: format!("path:{}:{}", entry.kind.as_str(), entry.path),
                action: MenuAction::Path {
                    path: entry.path.clone(),
                    kind: entry.kind.clone(),
                },
                label: basename(&entry.path).to_owned(),
                description: parent_dir(&entry.path).to_owned(),
                source: None,
            })
            .collect(),
        TriggerKind::SlashCommand => {
            let builtin = MenuItem {
                id: "slash:model".into(),
                action: MenuAction::OpenModelPicker,
                label: "/model".into(),
                description: "Switch response model for this thread".into(),
                source: None,
            };
            let commands = provider
                .map(|provider| provider.slash_commands.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|command| MenuItem {
                    id: format!("provider-slash-command:{instance}:{}", command.name),
                    action: MenuAction::ProviderCommand {
                        name: command.name.clone(),
                    },
                    label: format!("/{}", command.name),
                    description: command
                        .description
                        .clone()
                        .or_else(|| command.input.as_ref().map(|input| input.hint.clone()))
                        .unwrap_or_else(|| "Run provider command".into()),
                    source: None,
                });
            let items: Vec<MenuItem> = std::iter::once(builtin).chain(commands).collect();
            let query = normalize_query(&trigger.query, Some('/'));
            if query.is_empty() {
                return items;
            }
            let scored = items
                .into_iter()
                .filter_map(|item| {
                    let (name, tie) = match &item.action {
                        MenuAction::OpenModelPicker => ("model".to_owned(), "0\u{0}model".into()),
                        MenuAction::ProviderCommand { name } => {
                            (name.clone(), format!("1\u{0}{name}\u{0}{instance}"))
                        }
                        _ => return None,
                    };
                    let score = score_slash_command(&name, &item.description, &query)?;
                    Some((score, tie, item))
                })
                .collect();
            rank(scored)
        }
        TriggerKind::Skill => {
            let skills = provider
                .map(|provider| provider.skills.as_slice())
                .unwrap_or_default();
            search_skills(skills, &trigger.query)
                .into_iter()
                .map(|skill| MenuItem {
                    id: format!("skill:{instance}:{}", skill.name),
                    action: MenuAction::Skill {
                        name: skill.name.clone(),
                    },
                    label: skill_display_name(skill),
                    description: skill
                        .short_description
                        .clone()
                        .or_else(|| skill.description.clone())
                        .or_else(|| skill.scope.as_ref().map(|scope| format!("{scope} skill")))
                        .unwrap_or_else(|| "Run provider skill".into()),
                    source: skill_install_source(skill),
                })
                .collect()
        }
    }
}

/// A labeled run of rows. `label` is `None` for an ungrouped list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuGroup<'a> {
    pub label: Option<&'static str>,
    pub items: Vec<&'a MenuItem>,
}

/// Groups rows for display: "Skills" for `$`, "Built-in" / "Provider" for an empty `/` query,
/// one unlabeled group otherwise.
pub fn group_items<'a>(items: &'a [MenuItem], trigger: &Trigger) -> Vec<MenuGroup<'a>> {
    match trigger.kind {
        TriggerKind::Skill if items.is_empty() => Vec::new(),
        TriggerKind::Skill => vec![MenuGroup {
            label: Some("Skills"),
            items: items.iter().collect(),
        }],
        TriggerKind::SlashCommand if trigger.query.trim().is_empty() => {
            let (builtin, provider): (Vec<_>, Vec<_>) = items
                .iter()
                .partition(|item| item.action == MenuAction::OpenModelPicker);
            [("Built-in", builtin), ("Provider", provider)]
                .into_iter()
                .filter(|(_, items)| !items.is_empty())
                .map(|(label, items)| MenuGroup {
                    label: Some(label),
                    items,
                })
                .collect()
        }
        _ => vec![MenuGroup {
            label: None,
            items: items.iter().collect(),
        }],
    }
}

/// Text shown when there are no rows.
pub fn empty_text(kind: TriggerKind, loading: bool) -> &'static str {
    match (kind, loading) {
        (TriggerKind::Skill, true) => "Searching workspace skills...",
        (TriggerKind::Skill, false) => "No skills found. Try / to browse provider commands.",
        (_, true) => "Searching workspace files...",
        (TriggerKind::Path, false) => "No matching files or folders.",
        (TriggerKind::SlashCommand, false) => "No matching command.",
    }
}

/// The key a highlight belongs to: the trigger kind plus its normalized query. When it changes,
/// the highlight resets to the first row.
pub fn search_key(trigger: &Trigger) -> (TriggerKind, String) {
    (trigger.kind, trigger.query.trim().to_lowercase())
}

/// The highlighted row (`resolveComposerMenuActiveItemId`): the remembered one if it belongs to
/// the current search and still exists, else the first.
pub fn active_item<'a>(
    items: &'a [MenuItem],
    highlighted: Option<&str>,
    highlight_is_current: bool,
) -> Option<&'a MenuItem> {
    highlighted
        .filter(|_| highlight_is_current)
        .and_then(|id| items.iter().find(|item| item.id == id))
        .or_else(|| items.first())
}

/// The row ↑/↓ moves to, wrapping around (`nudgeComposerMenuHighlight`).
pub fn nudge<'a>(items: &'a [MenuItem], active: Option<&str>, down: bool) -> Option<&'a MenuItem> {
    if items.is_empty() {
        return None;
    }
    let len = items.len() as isize;
    let current = active
        .and_then(|id| items.iter().position(|item| item.id == id))
        .map_or(if down { -1 } else { 0 }, |index| index as isize);
    let next = (current + if down { 1 } else { -1 }).rem_euclid(len);
    items.get(next as usize)
}
