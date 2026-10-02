//! The composer command menu: items for the active trigger, the highlighted row, and the text a
//! selection writes (`ChatComposer.tsx` `composerMenuItems`, `onSelectComposerItem`,
//! `ComposerCommandMenu.tsx`, `composerSlashCommandSearch.ts`, `composerMenuHighlight.ts`).

use t3_protocol::{
    projects::{EntryKind, ProjectEntry},
    server::{ProviderSkill, ProviderSlashCommand},
};

use super::{
    prompt::{Trigger, TriggerKind, basename, parent_dir, serialize_file_link, serialize_skill},
    search::{
        SkillSource, invocable_skills, is_slash, normalize_query, rank, score_slash_command,
        score_slash_skill, search_skills, skill_display_name,
    },
};

/// `projects.searchEntries` page size for `@` mentions.
pub const PATH_SEARCH_LIMIT: u32 = 80;
/// Debounce between typing an `@` query and searching.
pub const PATH_SEARCH_DEBOUNCE_MS: u64 = 120;

/// One menu row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable id: `path:<kind>:<path>`, `slash:<command>`,
    /// `provider-slash-command:<instance>:<name>`, `skill:<instance>:<name>`.
    pub id: String,
    pub action: MenuAction,
    /// 12px medium text, at most 45% of the row. `/` skill rows render a muted `/skill:` prefix
    /// before [`MenuItem::label`].
    pub label: String,
    /// Muted text after the label.
    pub description: String,
    /// Right-aligned badge for skills.
    pub source: Option<SkillSource>,
}

/// What selecting a row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// Insert a file chip.
    Path { path: String, kind: EntryKind },
    /// Built-in `/model`: clear the trigger and open the model picker.
    OpenModelPicker,
    /// Built-in `/plan` or `/default` (only with plan mode enabled): switch interaction mode.
    SetPlanMode { plan: bool },
    /// Insert `/name `.
    ProviderCommand { name: String },
    /// Insert a skill chip. `slash` when picked from the `/` menu.
    Skill { name: String, slash: bool },
}

impl MenuItem {
    /// Leading glyph of the row: only paths (Pierre icon) and pull requests have one now.
    pub fn entry_icon(&self) -> Option<(&str, bool)> {
        match &self.action {
            MenuAction::Path { path, kind } => Some((path, *kind == EntryKind::Directory)),
            _ => None,
        }
    }

    /// The text that replaces the trigger. `None` for built-ins, which only clear it.
    pub fn replacement(&self) -> Option<Replacement> {
        Some(match &self.action {
            MenuAction::Path { path, .. } => Replacement::Chip {
                token: serialize_file_link(path),
            },
            MenuAction::Skill { name, .. } => Replacement::Chip {
                token: serialize_skill(name),
            },
            MenuAction::ProviderCommand { name } => Replacement::Text(format!("/{name} ")),
            MenuAction::OpenModelPicker | MenuAction::SetPlanMode { .. } => return None,
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

/// What the menu reads besides the trigger.
#[derive(Clone, Copy, Debug, Default)]
pub struct MenuSources<'a> {
    /// The selected instance's id (item ids).
    pub instance: &'a str,
    /// Its slash commands and skills for the thread's workspace.
    pub slash_commands: &'a [ProviderSlashCommand],
    pub skills: &'a [ProviderSkill],
    /// `projects.searchEntries` results for a path trigger.
    pub entries: &'a [ProjectEntry],
    /// Client setting `planModeEnabled` and the provider allows the toggle.
    pub plan_mode: bool,
    /// Client setting `showSkillsInSlashMenu` (default on).
    pub skills_in_slash_menu: bool,
    /// `/compact` applies: the trigger is the whole prompt, nothing attached, and the thread
    /// has a conversation to compact.
    pub compact_available: bool,
}

/// Builds the rows for `trigger`.
pub fn menu_items(trigger: &Trigger, sources: MenuSources<'_>) -> Vec<MenuItem> {
    let instance = sources.instance;
    match trigger.kind {
        TriggerKind::Path => sources
            .entries
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
        TriggerKind::SlashCommand => slash_items(trigger, sources),
        TriggerKind::Skill => search_skills(sources.skills, &trigger.query)
            .into_iter()
            .map(|skill| MenuItem {
                id: format!("skill:{instance}:{}", skill.name),
                action: MenuAction::Skill {
                    name: skill.name.clone(),
                    slash: false,
                },
                label: skill_display_name(skill),
                description: skill_description(skill, "Run provider skill"),
                source: Some(SkillSource::of(skill)),
            })
            .collect(),
        // Pull request search needs the PR index; not wired yet.
        TriggerKind::PullRequest => Vec::new(),
    }
}

fn skill_description(skill: &ProviderSkill, fallback: &str) -> String {
    skill
        .short_description
        .clone()
        .or_else(|| skill.description.clone())
        .or_else(|| skill.scope.as_ref().map(|scope| format!("{scope} skill")))
        .unwrap_or_else(|| fallback.to_owned())
}

/// `/model` (+ `/plan`, `/default`), provider commands (not ones a visible skill shadows,
/// `/compact` only when it applies, and only at the start of the prompt), then `/skill:` rows;
/// ranked when there is a query.
fn slash_items(trigger: &Trigger, sources: MenuSources<'_>) -> Vec<MenuItem> {
    let instance = sources.instance;
    let mut items = vec![MenuItem {
        id: "slash:model".into(),
        action: MenuAction::OpenModelPicker,
        label: "/model".into(),
        description: "Switch response model for this thread".into(),
        source: None,
    }];
    if sources.plan_mode {
        items.push(MenuItem {
            id: "slash:plan".into(),
            action: MenuAction::SetPlanMode { plan: true },
            label: "/plan".into(),
            description: "Switch this thread into plan mode".into(),
            source: None,
        });
        items.push(MenuItem {
            id: "slash:default".into(),
            action: MenuAction::SetPlanMode { plan: false },
            label: "/default".into(),
            description: "Switch this thread back to normal build mode".into(),
            source: None,
        });
    }
    let skills = if sources.skills_in_slash_menu {
        invocable_skills(sources.skills)
    } else {
        Vec::new()
    };
    let shadowed: Vec<String> = skills
        .iter()
        .map(|skill| skill.name.trim().to_lowercase())
        .collect();
    let at_prompt_start = trigger.range.start == 0;
    if at_prompt_start {
        items.extend(
            sources
                .slash_commands
                .iter()
                .filter(|command| !shadowed.contains(&command.name.trim().to_lowercase()))
                .filter(|command| command.name != "compact" || sources.compact_available)
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
                }),
        );
    }
    let skill_rows: Vec<(&ProviderSkill, MenuItem)> = skills
        .into_iter()
        .map(|skill| {
            (
                skill,
                MenuItem {
                    id: format!("skill:{instance}:{}", skill.name),
                    action: MenuAction::Skill {
                        name: skill.name.clone(),
                        slash: true,
                    },
                    label: skill_display_name(skill),
                    description: skill_description(skill, ""),
                    source: Some(SkillSource::of(skill)),
                },
            )
        })
        .collect();

    let query = normalize_query(&trigger.query, Some(is_slash));
    if query.is_empty() {
        items.extend(skill_rows.into_iter().map(|(_, item)| item));
        return items;
    }
    let mut scored: Vec<(i64, String, MenuItem)> = items
        .into_iter()
        .filter_map(|item| {
            let (name, tie) = match &item.action {
                MenuAction::OpenModelPicker => ("model".to_owned(), "0\u{0}model".to_owned()),
                MenuAction::SetPlanMode { plan } => {
                    let name = if *plan { "plan" } else { "default" };
                    (name.to_owned(), format!("0\u{0}{name}"))
                }
                MenuAction::ProviderCommand { name } => {
                    (name.clone(), format!("1\u{0}{name}\u{0}{instance}"))
                }
                _ => return None,
            };
            let score = score_slash_command(&name, &item.description, &query)?;
            Some((score, tie, item))
        })
        .collect();
    scored.extend(skill_rows.into_iter().filter_map(|(skill, item)| {
        let score = score_slash_skill(skill, &query)?;
        Some((score, format!("2\u{0}{}\u{0}{instance}", skill.name), item))
    }));
    rank(scored)
}

/// Text shown when there are no rows.
pub fn empty_text(kind: TriggerKind, loading: bool) -> &'static str {
    match (kind, loading) {
        (TriggerKind::Skill, true) => "Searching workspace skills...",
        (TriggerKind::PullRequest, true) => "Finding pull request...",
        (_, true) => "Searching workspace files...",
        (TriggerKind::Skill, false) => "No skills found. Try / to browse provider commands.",
        (TriggerKind::Path, false) => "No matching files or folders.",
        (TriggerKind::SlashCommand | TriggerKind::PullRequest, false) => "No matching command.",
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
