//! Shortcut and `when` expression parsing (`packages/shared/src/keybindings.ts`), and the fork's
//! default bindings compiled with it.

use super::{KeybindingShortcut, KeybindingWhenNode, ResolvedKeybindingRule};

/// Nesting limit for `when` expressions (`MAX_WHEN_EXPRESSION_DEPTH`).
const MAX_WHEN_DEPTH: usize = 64;
/// The server keeps at most this many rules (`MAX_KEYBINDINGS_COUNT`).
const MAX_KEYBINDINGS: usize = 256;

/// The fork's default rules (`DEFAULT_KEYBINDINGS`), as `(key, command, when)`.
const DEFAULT_RULES: &[(&str, &str, Option<&str>)] = &[
    ("mod+b", "sidebar.toggle", None),
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
    ("mod+d", "diff.toggle", Some("!terminalFocus")),
    ("mod+shift+j", "preview.toggle", None),
    ("mod+r", "preview.refresh", Some("previewFocus")),
    ("mod+l", "preview.focusUrl", Some("previewFocus")),
    ("mod+=", "preview.zoomIn", Some("previewFocus")),
    ("mod++", "preview.zoomIn", Some("previewFocus")),
    ("mod+-", "preview.zoomOut", Some("previewFocus")),
    ("mod+0", "preview.resetZoom", Some("previewFocus")),
    ("mod+k", "commandPalette.toggle", Some("!terminalFocus")),
    ("mod+n", "chat.new", Some("!terminalFocus")),
    ("mod+shift+o", "chat.new", Some("!terminalFocus")),
    ("mod+shift+n", "chat.newLocal", Some("!terminalFocus")),
    ("mod+shift+m", "modelPicker.toggle", Some("!terminalFocus")),
    ("mod+o", "editor.openFavorite", None),
    ("mod+shift+[", "thread.previous", None),
    ("mod+shift+]", "thread.next", None),
];

/// The bindings used until the server's config arrives (web `DEFAULT_RESOLVED_KEYBINDINGS`).
pub fn default_keybindings() -> Vec<ResolvedKeybindingRule> {
    let jumps = (1..=9).map(|index| (format!("mod+{index}"), format!("thread.jump.{index}"), None));
    let picker_jumps = (1..=9).map(|index| {
        (
            format!("mod+{index}"),
            format!("modelPicker.jump.{index}"),
            Some("modelPickerOpen"),
        )
    });
    let rules: Vec<_> = DEFAULT_RULES
        .iter()
        .map(|&(key, command, when)| (key.to_owned(), command.to_owned(), when))
        .chain(jumps)
        .chain(picker_jumps)
        .filter_map(|(key, command, when)| compile_rule(&key, command, when))
        .collect();
    let skip = rules.len().saturating_sub(MAX_KEYBINDINGS);
    rules.into_iter().skip(skip).collect()
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
