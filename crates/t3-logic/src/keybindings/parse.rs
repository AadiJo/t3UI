//! Shortcut and `when` expression parsing (`packages/shared/src/keybindings.ts`), and the fork's
//! default bindings compiled with it.

use super::{KeybindingShortcut, KeybindingWhenNode, ResolvedKeybindingRule};

/// Nesting limit for `when` expressions (`MAX_WHEN_EXPRESSION_DEPTH`).
const MAX_WHEN_DEPTH: usize = 64;
/// The server keeps at most this many rules (`MAX_KEYBINDINGS_COUNT`).
const MAX_KEYBINDINGS: usize = 256;

/// The fork's default rules (`shared/keybindings.ts` `DEFAULT_KEYBINDINGS`), as
/// `(key, command, when)`, in source order (later rules shadow earlier ones).
/// `thread.jump.N` / `modelPicker.jump.N` and the usage page rules are appended by [`default_keybindings`] in the same positions.
const DEFAULT_RULES: &[(&str, &str, Option<&str>)] = &[
    ("mod+b", "sidebar.toggle", None),
    ("mod+[", "navigation.back", Some("!terminalFocus")),
    ("mod+]", "navigation.forward", Some("!terminalFocus")),
    ("mod+j", "terminal.toggle", None),
    ("mod+alt+b", "rightPanel.toggle", None),
    ("mod+d", "terminal.split", Some("terminalFocus")),
    (
        "mod+shift+d",
        "terminal.splitVertical",
        Some("terminalFocus"),
    ),
    ("mod+n", "terminal.new", Some("terminalFocus")),
    ("mod+w", "terminal.close", Some("terminalFocus")),
    ("mod+w", "rightPanel.close", Some("!terminalFocus")),
    ("mod+d", "diff.toggle", Some("!terminalFocus")),
    ("mod+shift+j", "preview.toggle", None),
    ("mod+r", "preview.refresh", Some("previewFocus")),
    ("mod+l", "preview.focusUrl", Some("previewFocus")),
    ("mod+=", "preview.zoomIn", Some("previewFocus")),
    ("mod++", "preview.zoomIn", Some("previewFocus")),
    ("mod+-", "preview.zoomOut", Some("previewFocus")),
    ("mod+0", "preview.resetZoom", Some("previewFocus")),
    ("mod+k", "commandPalette.toggle", Some("!terminalFocus")),
    ("mod+p", "filePicker.toggle", Some("!terminalFocus")),
    (
        "mod+shift+f",
        "projectSearch.toggle",
        Some("!terminalFocus"),
    ),
    ("mod+u", "usage.open", Some("!terminalFocus")),
    ("mod+alt+a", "theme.select", Some("!terminalFocus")),
    (
        "mod+alt+shift+a",
        "appearance.cycle",
        Some("!terminalFocus"),
    ),
    ("mod+alt+shift+t", "themeEditor.toggle", None),
    ("mod+s", "composer.stash", Some("!terminalFocus")),
    (
        "mod+shift+enter",
        "thread.steerQueuedMessage",
        Some("!terminalFocus"),
    ),
    ("mod+n", "chat.new", Some("!terminalFocus")),
    ("mod+shift+o", "chat.new", Some("!terminalFocus")),
    ("mod+shift+n", "chat.newLocal", Some("!terminalFocus")),
    (
        "mod+alt+n",
        "chat.newWithoutProject",
        Some("!terminalFocus"),
    ),
    ("mod+shift+m", "modelPicker.toggle", Some("!terminalFocus")),
    ("mod+shift+h", "composer.host", Some("!terminalFocus")),
    ("mod+shift+e", "composer.effort", Some("!terminalFocus")),
    ("mod+shift+a", "composer.mode", Some("!terminalFocus")),
    ("mod+shift+x", "composer.workspace", Some("!terminalFocus")),
    ("mod+shift+g", "composer.branch", Some("!terminalFocus")),
    (
        "mod+shift+l",
        "composer.previousWorktree",
        Some("!terminalFocus"),
    ),
    (
        "mod+shift+k",
        "pullRequest.copyNumber",
        Some("!terminalFocus"),
    ),
    (
        "mod+shift+arrowup",
        "modelPicker.previousProvider",
        Some("modelPickerOpen"),
    ),
    (
        "mod+shift+arrowdown",
        "modelPicker.nextProvider",
        Some("modelPickerOpen"),
    ),
    ("mod+o", "editor.openFavorite", None),
    ("mod+shift+[", "thread.previous", None),
    ("mod+shift+]", "thread.next", None),
    (
        "mod+shift+c",
        "thread.copyReference",
        Some("!terminalFocus"),
    ),
    ("mod+shift+s", "thread.settle", Some("!terminalFocus")),
    ("mod+shift+p", "thread.pin", Some("!terminalFocus")),
    (
        "mod+z",
        "thread.undo",
        Some("!terminalFocus && !editableFocus"),
    ),
];

/// The usage page's rules, after the jump rules (`DEFAULT_KEYBINDINGS` tail).
const USAGE_RULES: &[(&str, &str, Option<&str>)] = &[
    ("c", "usage.cost", Some("usagePageOpen")),
    ("t", "usage.tokens", Some("usagePageOpen")),
    ("l", "usage.limits", Some("usagePageOpen")),
    ("mod+shift+1", "usage.period.day", Some("usagePageOpen")),
    ("mod+shift+2", "usage.period.week", Some("usagePageOpen")),
    ("mod+shift+3", "usage.period.month", Some("usagePageOpen")),
    ("mod+shift+4", "usage.period.quarter", Some("usagePageOpen")),
];

/// The built-in bindings (web `DEFAULT_RESOLVED_KEYBINDINGS`). The client always merges the
/// server's list over these with [`merge_with_default_keybindings`].
pub fn default_keybindings() -> Vec<ResolvedKeybindingRule> {
    let owned = |rules: &'static [(&str, &str, Option<&str>)]| {
        rules
            .iter()
            .map(|&(key, command, when)| (key.to_owned(), command.to_owned(), when))
    };
    let jumps = (1..=9).map(|index| {
        (
            format!("mod+{index}"),
            format!("thread.jump.{index}"),
            Some("isDesktop"),
        )
    });
    let picker_jumps = (1..=9).map(|index| {
        (
            format!("mod+{index}"),
            format!("modelPicker.jump.{index}"),
            Some("modelPickerOpen && isDesktop"),
        )
    });
    let rules: Vec<_> = owned(DEFAULT_RULES)
        .chain(jumps)
        .chain(picker_jumps)
        .chain(owned(USAGE_RULES))
        .filter_map(|(key, command, when)| compile_rule(&key, command, when))
        .collect();
    let skip = rules.len().saturating_sub(MAX_KEYBINDINGS);
    rules.into_iter().skip(skip).collect()
}

/// `mergeWithDefaultKeybindings`: the server's rules win per command. An empty server list
/// means the defaults; otherwise the defaults for commands the server did not mention come
/// first, then the server's rules, keeping the last 256.
pub fn merge_with_default_keybindings(
    server: &[ResolvedKeybindingRule],
) -> Vec<ResolvedKeybindingRule> {
    let defaults = default_keybindings();
    if server.is_empty() {
        return defaults;
    }
    let overridden: std::collections::HashSet<&str> =
        server.iter().map(|rule| rule.command.as_str()).collect();
    let merged: Vec<_> = defaults
        .into_iter()
        .filter(|rule| !overridden.contains(rule.command.as_str()))
        .chain(server.iter().cloned())
        .collect();
    let skip = merged.len().saturating_sub(MAX_KEYBINDINGS);
    merged.into_iter().skip(skip).collect()
}

fn compile_rule(key: &str, command: String, when: Option<&str>) -> Option<ResolvedKeybindingRule> {
    let shortcut = parse_shortcut(key)?;
    let when_ast = match when {
        Some(expression) => Some(parse_when(expression)?),
        None => None,
    };
    Some(ResolvedKeybindingRule {
        command,
        shortcut,
        when_ast,
    })
}

/// Parses `mod+shift+k` style shortcuts. Tokens are case-insensitive; `cmd`/`meta`, `ctrl`/
/// `control`, `alt`/`option`, `shift`, and `mod` are modifiers; exactly one other token is the key
/// (`space` and `esc` are normalized). A trailing `+` is the plus key (`mod++`).
pub fn parse_shortcut(value: &str) -> Option<KeybindingShortcut> {
    let lowered = value.to_lowercase();
    let mut tokens: Vec<&str> = lowered.split('+').map(str::trim).collect();
    let mut trailing_empty = 0;
    while tokens.last() == Some(&"") {
        trailing_empty += 1;
        tokens.pop();
    }
    if trailing_empty > 0 {
        tokens.push("+");
    }
    if tokens.is_empty() || tokens.iter().any(|token| token.is_empty()) {
        return None;
    }

    let mut shortcut = KeybindingShortcut {
        key: String::new(),
        meta_key: false,
        ctrl_key: false,
        shift_key: false,
        alt_key: false,
        mod_key: false,
    };
    let mut key = None;
    for token in tokens {
        match token {
            "cmd" | "meta" => shortcut.meta_key = true,
            "ctrl" | "control" => shortcut.ctrl_key = true,
            "shift" => shortcut.shift_key = true,
            "alt" | "option" => shortcut.alt_key = true,
            "mod" => shortcut.mod_key = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Some(match other {
                    "space" => " ",
                    "esc" => "escape",
                    other => other,
                });
            }
        }
    }
    shortcut.key = key?.to_owned();
    Some(shortcut)
}

#[derive(Debug, PartialEq)]
enum Token<'a> {
    Identifier(&'a str),
    Not,
    And,
    Or,
    LParen,
    RParen,
}

fn tokenize(expression: &str) -> Option<Vec<Token<'_>>> {
    let mut tokens = Vec::new();
    let mut rest = expression;
    while let Some(first) = rest.chars().next() {
        if first.is_whitespace() {
            rest = &rest[first.len_utf8()..];
            continue;
        }
        let (token, len) = if rest.starts_with("&&") {
            (Token::And, 2)
        } else if rest.starts_with("||") {
            (Token::Or, 2)
        } else if first == '!' {
            (Token::Not, 1)
        } else if first == '(' {
            (Token::LParen, 1)
        } else if first == ')' {
            (Token::RParen, 1)
        } else if first.is_ascii_alphabetic() || first == '_' {
            let len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
                .unwrap_or(rest.len());
            (Token::Identifier(&rest[..len]), len)
        } else {
            return None;
        };
        tokens.push(token);
        rest = &rest[len..];
    }
    Some(tokens)
}

/// Parses a `when` expression (`terminalFocus && !previewOpen`). `!` binds tighter than `&&`,
/// which binds tighter than `||`. Returns `None` for empty, malformed, or too deeply nested input.
pub fn parse_when(expression: &str) -> Option<KeybindingWhenNode> {
    let tokens = tokenize(expression)?;
    if tokens.is_empty() {
        return None;
    }
    let mut parser = WhenParser { tokens, index: 0 };
    let ast = parser.or(0)?;
    (parser.index == parser.tokens.len()).then_some(ast)
}

struct WhenParser<'a> {
    tokens: Vec<Token<'a>>,
    index: usize,
}

impl WhenParser<'_> {
    fn peek(&self) -> Option<&Token<'_>> {
        self.tokens.get(self.index)
    }

    fn or(&mut self, depth: usize) -> Option<KeybindingWhenNode> {
        let mut left = self.and(depth)?;
        while self.peek() == Some(&Token::Or) {
            self.index += 1;
            let right = self.and(depth)?;
            left = KeybindingWhenNode::Or {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn and(&mut self, depth: usize) -> Option<KeybindingWhenNode> {
        let mut left = self.unary(depth)?;
        while self.peek() == Some(&Token::And) {
            self.index += 1;
            let right = self.unary(depth)?;
            left = KeybindingWhenNode::And {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Some(left)
    }

    fn unary(&mut self, depth: usize) -> Option<KeybindingWhenNode> {
        let mut nots = 0;
        while self.peek() == Some(&Token::Not) {
            self.index += 1;
            nots += 1;
            if nots > MAX_WHEN_DEPTH {
                return None;
            }
        }
        let mut node = self.primary(depth)?;
        for _ in 0..nots {
            node = KeybindingWhenNode::Not {
                node: Box::new(node),
            };
        }
        Some(node)
    }

    fn primary(&mut self, depth: usize) -> Option<KeybindingWhenNode> {
        if depth > MAX_WHEN_DEPTH {
            return None;
        }
        match self.peek()? {
            Token::Identifier(name) => {
                let name = (*name).to_owned();
                self.index += 1;
                Some(KeybindingWhenNode::Identifier { name })
            }
            Token::LParen => {
                self.index += 1;
                let node = self.or(depth + 1)?;
                if self.peek() != Some(&Token::RParen) {
                    return None;
                }
                self.index += 1;
                Some(node)
            }
            _ => None,
        }
    }
}
