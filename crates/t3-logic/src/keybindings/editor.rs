//! Settings > Keybindings table rows (`KeybindingsSettings.logic.ts`): wire strings for keys and
//! `when` clauses, the row source (Default / Custom / Project), conflicts, sorting, and search.

use t3_protocol::server::{KeybindingShortcut, KeybindingWhenNode, ResolvedKeybindingRule};

use super::default_keybindings;

/// Where a binding comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeybindingSource {
    Default,
    Custom,
    /// `script.<id>.run`.
    Project,
}

impl KeybindingSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Custom => "Custom",
            Self::Project => "Project",
        }
    }
}

/// One table row.
#[derive(Clone, Debug, PartialEq)]
pub struct KeybindingRow {
    pub command: String,
    /// `mod+shift+o`.
    pub key: String,
    /// `!terminalFocus`, or empty for always.
    pub when: String,
    pub source: KeybindingSource,
    /// The default this row resets to, if any.
    pub default_key: Option<String>,
    pub default_when: String,
    /// Labels of other commands bound to the same key in an overlapping context.
    pub conflicts: Vec<String>,
    pub rule: ResolvedKeybindingRule,
}

/// `shortcutToKeybindingInput`: `mod+shift+o`.
pub fn shortcut_to_input(shortcut: &KeybindingShortcut) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for (on, name) in [
        (shortcut.mod_key, "mod"),
        (shortcut.meta_key, "meta"),
        (shortcut.ctrl_key, "ctrl"),
        (shortcut.alt_key, "alt"),
        (shortcut.shift_key, "shift"),
    ] {
        if on {
            parts.push(name);
        }
    }
    let key = match shortcut.key.as_str() {
        " " => "space",
        "escape" => "esc",
        key => key,
    };
    parts.push(key);
    parts.join("+")
}

/// `whenAstToExpression`: parenthesizes `&&` / `||` operands.
pub fn when_to_expression(node: Option<&KeybindingWhenNode>) -> String {
    fn wrap(node: &KeybindingWhenNode) -> String {
        match node {
            KeybindingWhenNode::Identifier { .. } | KeybindingWhenNode::Not { .. } => {
                expression(node)
            }
            _ => format!("({})", expression(node)),
        }
    }
    fn expression(node: &KeybindingWhenNode) -> String {
        match node {
            KeybindingWhenNode::Identifier { name } => name.clone(),
            KeybindingWhenNode::Not { node } => format!("!{}", wrap(node)),
            KeybindingWhenNode::And { left, right } => format!("{} && {}", wrap(left), wrap(right)),
            KeybindingWhenNode::Or { left, right } => format!("{} || {}", wrap(left), wrap(right)),
            KeybindingWhenNode::Unknown => String::new(),
        }
    }
    node.map(expression).unwrap_or_default()
}

/// `commandLabel`: `thread.jump.1` reads "Thread: Jump: 1"; `script.lint.run` reads
/// "Run Script: Lint".
pub fn command_label(command: &str) -> String {
    if let Some(id) = command
        .strip_prefix("script.")
        .and_then(|rest| rest.strip_suffix(".run"))
    {
        return format!("Run Script: {}", title_case(id));
    }
    command
        .split('.')
        .map(title_case)
        .collect::<Vec<_>>()
        .join(": ")
}

/// Splits camelCase and `-`/`_` words, capitalizing each.
fn title_case(segment: &str) -> String {
    let mut spaced = String::new();
    let mut previous: Option<char> = None;
    for char in segment.chars() {
        if char.is_ascii_uppercase()
            && previous
                .is_some_and(|previous| previous.is_ascii_lowercase() || previous.is_ascii_digit())
        {
            spaced.push(' ');
        }
        spaced.push(char);
        previous = Some(char);
    }
    spaced
        .split(|char: char| char == '-' || char == '_' || char.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn wire(rule: &ResolvedKeybindingRule) -> (String, String) {
    (
        shortcut_to_input(&rule.shortcut),
        when_to_expression(rule.when_ast.as_ref()),
    )
}

/// Builds the sorted rows (command, then key) and keeps those matching `query` in the command
/// id, key, when clause, or source.
pub fn build_rows(rules: &[ResolvedKeybindingRule], query: &str) -> Vec<KeybindingRow> {
    let defaults: Vec<(String, String, String)> = default_keybindings()
        .iter()
        .map(|rule| {
            let (key, when) = wire(rule);
            (rule.command.clone(), key, when)
        })
        .collect();
    let mut rows: Vec<KeybindingRow> = rules
        .iter()
        .map(|rule| {
            let (key, when) = wire(rule);
            let exact = defaults.iter().any(|(command, default_key, default_when)| {
                *command == rule.command && *default_key == key && *default_when == when
            });
            let source = if rule.command.starts_with("script.") {
                KeybindingSource::Project
            } else if exact {
                KeybindingSource::Default
            } else {
                KeybindingSource::Custom
            };
            let default = defaults
                .iter()
                .find(|(command, default_key, default_when)| {
                    *command == rule.command && *default_key == key && *default_when == when
                })
                .or_else(|| {
                    defaults.iter().find(|(command, _, default_when)| {
                        *command == rule.command && *default_when == when
                    })
                })
                .or_else(|| {
                    defaults
                        .iter()
                        .find(|(command, _, _)| *command == rule.command)
                });
            KeybindingRow {
                command: rule.command.clone(),
                key,
                when,
                source,
                default_key: default.map(|(_, key, _)| key.clone()),
                default_when: default.map(|(_, _, when)| when.clone()).unwrap_or_default(),
                conflicts: Vec::new(),
                rule: rule.clone(),
            }
        })
        .collect();
    let snapshot: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| (row.command.clone(), row.key.clone(), row.when.clone()))
        .collect();
    for (index, row) in rows.iter_mut().enumerate() {
        let mut conflicts: Vec<String> = snapshot
            .iter()
            .enumerate()
            .filter(|(other, (_, key, when))| {
                *other != index
                    && *key == row.key
                    && (when.is_empty() || row.when.is_empty() || *when == row.when)
            })
            .map(|(_, (command, _, _))| command_label(command))
            .collect();
        conflicts.sort();
        conflicts.dedup();
        row.conflicts = conflicts;
    }
    rows.sort_by(|left, right| {
        left.command
            .cmp(&right.command)
            .then_with(|| left.key.cmp(&right.key))
    });
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|row| {
            row.command.to_lowercase().contains(&query)
                || row.key.to_lowercase().contains(&query)
                || row.when.to_lowercase().contains(&query)
                || row.source.label().to_lowercase().contains(&query)
        })
        .collect()
}

/// Key pill parts (`mod` reads ⌘ on macOS, Ctrl elsewhere).
pub fn key_parts(key: &str, mac: bool) -> Vec<String> {
    key.split('+')
        .filter(|part| !part.is_empty())
        .map(|part| match part {
            "mod" => (if mac { "⌘" } else { "Ctrl" }).to_owned(),
            "shift" => "⇧".to_owned(),
            "alt" => (if mac { "⌥" } else { "Alt" }).to_owned(),
            "ctrl" => "⌃".to_owned(),
            part if part.chars().count() == 1 => part.to_uppercase(),
            part => part.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Failure modes:
    //! 1. Wire strings differ from the web (`mod+shift+o`, `esc`, parenthesized `when`), so
    //!    "Default" detection and server upserts break.
    //! 2. A default binding reads "Custom", or a script binding is not "Project".
    //! 3. Conflicts miss an always-on binding or flag different `when` clauses.
    //! 4. Command labels split camelCase or script ids wrong.
    use super::*;
    use crate::keybindings::parse::{parse_shortcut, parse_when};

    fn rule(command: &str, key: &str, when: Option<&str>) -> ResolvedKeybindingRule {
        ResolvedKeybindingRule {
            command: command.into(),
            shortcut: parse_shortcut(key).unwrap(),
            when_ast: when.and_then(parse_when),
        }
    }

    #[test]
    fn wire_strings() {
        assert_eq!(
            shortcut_to_input(&parse_shortcut("mod+shift+o").unwrap()),
            "mod+shift+o"
        );
        assert_eq!(
            shortcut_to_input(&parse_shortcut("mod+escape").unwrap()),
            "mod+esc"
        );
        assert_eq!(
            when_to_expression(parse_when("!terminalFocus && (a || b)").as_ref()),
            "!terminalFocus && (a || b)"
        );
        assert_eq!(when_to_expression(None), "");
    }

    #[test]
    fn labels() {
        assert_eq!(command_label("thread.jump.1"), "Thread: Jump: 1");
        assert_eq!(
            command_label("commandPalette.toggle"),
            "Command Palette: Toggle"
        );
        assert_eq!(command_label("script.lint-all.run"), "Run Script: Lint All");
        assert_eq!(key_parts("mod+shift+o", true), ["⌘", "⇧", "O"]);
        assert_eq!(key_parts("mod+k", false), ["Ctrl", "K"]);
    }

    #[test]
    fn rows_sources_and_conflicts() {
        let rules = vec![
            rule("commandPalette.toggle", "mod+k", Some("!terminalFocus")),
            rule("chat.new", "mod+k", None),
            rule("terminal.toggle", "mod+j", Some("terminalFocus")),
            rule("script.lint.run", "mod+l", None),
        ];
        let rows = build_rows(&rules, "");
        let commands: Vec<&str> = rows.iter().map(|row| row.command.as_str()).collect();
        assert_eq!(
            commands,
            [
                "chat.new",
                "commandPalette.toggle",
                "script.lint.run",
                "terminal.toggle"
            ]
        );
        let palette = &rows[1];
        assert_eq!(palette.source, KeybindingSource::Default);
        assert_eq!(palette.conflicts, ["Chat: New"]);
        assert_eq!(rows[0].source, KeybindingSource::Custom);
        assert_eq!(rows[2].source, KeybindingSource::Project);
        assert!(rows[3].conflicts.is_empty());
        assert_eq!(build_rows(&rules, "PROJECT").len(), 1);
        assert_eq!(build_rows(&rules, "mod+j")[0].command, "terminal.toggle");
    }
}
