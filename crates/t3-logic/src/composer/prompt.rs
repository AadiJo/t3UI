//! The composer prompt as a plain string, like the web composer keeps it
//! (`composer-editor-mentions.ts`, `shared/composerInlineTokens.ts`, `composer-logic.ts`).
//!
//! The editor shows some substrings as atomic chips, but the source of truth stays a string:
//!
//! - a file mention is `[basename](percent-encoded path)` (or the older `@path` / `@"quoted"`),
//! - a skill is `$name`,
//! - a terminal context is one U+FFFC per context, in draft order.
//!
//! A token only becomes a chip once whitespace follows it, so a half-typed `$ski` stays text.
//! Every offset here is a UTF-8 byte offset into the prompt; the editor works in the same units.

use std::ops::Range;

/// Where a terminal context chip sits in the prompt (`INLINE_TERMINAL_CONTEXT_PLACEHOLDER`).
pub const TERMINAL_CONTEXT_PLACEHOLDER: char = '\u{FFFC}';

/// What a chip refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineTokenKind {
    /// A workspace path (decoded).
    Mention { path: String },
    /// A provider skill name, without the `$`.
    Skill { name: String },
}

/// One recognized chip and the bytes of the prompt it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineToken {
    pub kind: InlineTokenKind,
    pub range: Range<usize>,
}

/// Every mention and skill chip in `text`, sorted by start (`collectComposerInlineTokens`).
/// Terminal placeholders are not included; find them with [`terminal_placeholder_offsets`].
pub fn collect_inline_tokens(text: &str) -> Vec<InlineToken> {
    let mut tokens = Vec::new();
    scan(text, &mut tokens, parse_file_link);
    scan(text, &mut tokens, parse_at_mention);
    scan(text, &mut tokens, parse_skill);
    tokens.sort_by_key(|token| token.range.start);
    tokens
}

/// Byte offsets of every terminal context placeholder, in order.
pub fn terminal_placeholder_offsets(text: &str) -> Vec<usize> {
    text.match_indices(TERMINAL_CONTEXT_PLACEHOLDER)
        .map(|(offset, _)| offset)
        .collect()
}

/// Runs one token grammar over `text` the way a global JS regex with a `(^|\s)` prefix and a
/// `(?=\s)` lookahead does: a token starts at the beginning or right after whitespace that is
/// not part of the previous match, and must be followed by whitespace.
fn scan(
    text: &str,
    tokens: &mut Vec<InlineToken>,
    parse: fn(&str, usize) -> Option<(usize, InlineTokenKind)>,
) {
    let mut resume = 0;
    let mut previous: Option<char> = None;
    for (start, ch) in text.char_indices() {
        let preceded_ok = match previous {
            None => true,
            // The prefix whitespace must lie at or after the end of the previous match.
            Some(prev) => prev.is_whitespace() && start - prev.len_utf8() >= resume,
        };
        previous = Some(ch);
        if start < resume || !preceded_ok {
            continue;
        }
        let Some((end, kind)) = parse(text, start) else {
            continue;
        };
        if !text[end..].chars().next().is_some_and(char::is_whitespace) {
            continue;
        }
        tokens.push(InlineToken {
            kind,
            range: start..end,
        });
        resume = end;
    }
}

/// `[label](path)` where `label` is the path's basename and the path has no URI scheme.
fn parse_file_link(text: &str, start: usize) -> Option<(usize, InlineTokenKind)> {
    let rest = text[start..].strip_prefix('[')?;
    let (label, after_label) = take_escaped(rest, ']')?;
    let after_paren = after_label.strip_prefix('(')?;
    let encoded_len = after_paren
        .find(|c: char| c == ')' || c.is_whitespace())
        .filter(|&len| len > 0)?;
    if !after_paren[encoded_len..].starts_with(')') {
        return None;
    }
    let encoded = &after_paren[..encoded_len];
    let path = percent_decode(encoded).unwrap_or_else(|| encoded.to_owned());
    if path.is_empty() || has_external_scheme(&path) || unescape(&label) != basename(&path) {
        return None;
    }
    let end = text.len() - after_paren.len() + encoded_len + 1;
    Some((end, InlineTokenKind::Mention { path }))
}

/// `@path` (no whitespace, `@` or `"`) or `@"quoted path"` with backslash escapes.
fn parse_at_mention(text: &str, start: usize) -> Option<(usize, InlineTokenKind)> {
    let rest = text[start..].strip_prefix('@')?;
    if let Some(quoted) = rest.strip_prefix('"') {
        let (raw, after) = take_escaped(quoted, '"')?;
        let path = unescape(&raw);
        return (!path.is_empty())
            .then(|| (text.len() - after.len(), InlineTokenKind::Mention { path }));
    }
    let len = rest
        .find(|c: char| c.is_whitespace() || c == '@' || c == '"')
        .unwrap_or(rest.len());
    (len > 0).then(|| {
        (
            start + 1 + len,
            InlineTokenKind::Mention {
                path: rest[..len].to_owned(),
            },
        )
    })
}

/// `$name` with `name` matching `[a-zA-Z][a-zA-Z0-9:_-]*`.
fn parse_skill(text: &str, start: usize) -> Option<(usize, InlineTokenKind)> {
    let rest = text[start..].strip_prefix('$')?;
    if !rest.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-')))
        .unwrap_or(rest.len());
    Some((
        start + 1 + len,
        InlineTokenKind::Skill {
            name: rest[..len].to_owned(),
        },
    ))
}

/// Reads up to an unescaped `close`, keeping escapes (`\x`) intact. Returns the raw body and
/// the text after `close`.
fn take_escaped(text: &str, close: char) -> Option<(String, &str)> {
    let mut body = String::new();
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        if ch == '\\' {
            let (_, escaped) = chars.next()?;
            body.push('\\');
            body.push(escaped);
        } else if ch == close {
            return Some((body, &text[index + close.len_utf8()..]));
        } else {
            body.push(ch);
        }
    }
    None
}

/// `\x` → `x`.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The last path segment, splitting on `/` and `\`.
pub fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The parent directory (`a/b/c.ts` → `a/b`), or empty for a bare name. Splits on `/` only,
/// like the menu's description.
pub fn parent_dir(path: &str) -> &str {
    path.rfind('/').map_or("", |index| &path[..index])
}

fn has_external_scheme(path: &str) -> bool {
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if windows_drive {
        return false;
    }
    let Some(colon) = path.find(':') else {
        return false;
    };
    let scheme = &path[..colon];
    scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// `decodeURIComponent`; `None` when the escapes are malformed or not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut iter = text.bytes();
    while let Some(byte) = iter.next() {
        if byte == b'%' {
            let hi = (iter.next()? as char).to_digit(16)?;
            let lo = (iter.next()? as char).to_digit(16)?;
            bytes.push((hi * 16 + lo) as u8);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).ok()
}

/// The chip text for a workspace path: `[basename](encoded path)` (`serializeComposerFileLink`).
pub fn serialize_file_link(path: &str) -> String {
    let label = basename(path)
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    format!("[{label}]({})", encode_link_destination(path))
}

/// `encodeURI(path)` plus `( ) # ? \` escaped, so the destination never ends the link early.
fn encode_link_destination(path: &str) -> String {
    const KEEP: &[u8] = b";,/:@&=+$-_.!~*'";
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || KEEP.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The text a skill chip stands for.
pub fn serialize_skill(name: &str) -> String {
    format!("${name}")
}

/// What the cursor is typing, if anything opens the command menu (`ComposerTrigger`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub kind: TriggerKind,
    /// The text after the sigil.
    pub query: String,
    /// Bytes the selected item replaces (sigil included).
    pub range: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TriggerKind {
    /// `@query`: workspace files and folders.
    Path,
    /// `/query` at the start of a line.
    SlashCommand,
    /// `$query`: provider skills.
    Skill,
}

/// Detects the trigger at `cursor` (`detectComposerTrigger`). `cursor` is clamped and snapped
/// down to a char boundary.
pub fn detect_trigger(text: &str, cursor: usize) -> Option<Trigger> {
    let mut cursor = cursor.min(text.len());
    while !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    let line_start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line_prefix = &text[line_start..cursor];
    if let Some(command) = line_prefix.strip_prefix('/')
        && !command.chars().any(char::is_whitespace)
    {
        return Some(Trigger {
            kind: TriggerKind::SlashCommand,
            query: command.to_owned(),
            range: line_start..cursor,
        });
    }

    let token_start = text[..cursor]
        .char_indices()
        .rev()
        .find(|&(_, c)| is_token_break(c))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let token = &text[token_start..cursor];
    let kind = match token.chars().next()? {
        '$' => TriggerKind::Skill,
        '@' => TriggerKind::Path,
        _ => return None,
    };
    Some(Trigger {
        kind,
        query: token[1..].to_owned(),
        range: token_start..cursor,
    })
}

fn is_token_break(c: char) -> bool {
    matches!(c, ' ' | '\n' | '\t' | '\r' | TERMINAL_CONTEXT_PLACEHOLDER)
}

/// End of the range a menu selection replaces: a replacement ending in a space swallows one
/// space that already follows the trigger (`extendReplacementRangeForTrailingSpace`).
pub fn replacement_end(text: &str, range_end: usize, replacement: &str) -> usize {
    if replacement.ends_with(' ') && text[range_end..].starts_with(' ') {
        range_end + 1
    } else {
        range_end
    }
}

/// Whether the prompt mentions "ultrathink" as a word (`isClaudeUltrathinkPrompt`).
pub fn mentions_ultrathink(text: &str) -> bool {
    text.match_indices(['u', 'U']).any(|(index, _)| {
        let candidate = &text[index..];
        let word_start = text[..index]
            .chars()
            .next_back()
            .is_none_or(|c| !is_word_char(c));
        word_start
            && candidate.len() >= 10
            && candidate.is_char_boundary(10)
            && candidate[..10].eq_ignore_ascii_case("ultrathink")
            && candidate[10..]
                .chars()
                .next()
                .is_none_or(|c| !is_word_char(c))
    })
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The prefix the Claude "ultrathink" effort writes into the prompt.
pub const ULTRATHINK_PREFIX: &str = "Ultrathink:\n";

/// Removes a leading `Ultrathink:` prefix (and the whitespace after it).
pub fn strip_ultrathink_prefix(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 11
        && text.is_char_boundary(11)
        && text[..11].eq_ignore_ascii_case("ultrathink:")
    {
        text[11..].trim_start()
    } else {
        text
    }
}
