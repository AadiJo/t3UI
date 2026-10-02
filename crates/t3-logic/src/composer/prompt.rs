//! The composer prompt as a plain string, like the web composer keeps it
//! (`composer-editor-mentions.ts`, `shared/composerInlineTokens.ts`,
//! `shared/composerContextReferences.ts`, `shared/assistantCitations.ts`, `composer-logic.ts`).
//!
//! The editor shows some substrings as atomic chips, but the source of truth stays a string:
//!
//! - a file mention is `[basename](percent-encoded path)` (or the older `@path` / `@"quoted"`),
//! - a skill is `$name` (any currency sign works as the sigil),
//! - a context (image, file, terminal selection, element, annotation, review comment, pull
//!   request) is `[label](t3-context://v1/<kind>/<id>)`, `![label](...)` for images; its payload
//!   lives in the draft's context records,
//! - a quote of an assistant message is `[Assistant quote](t3-citation://v1/...)`.
//!
//! Mentions and skills only become chips once whitespace follows them, so a half-typed `$ski`
//! stays text; context links and citations are chips wherever they appear. Every offset here is
//! a UTF-8 byte offset into the prompt; the editor works in the same units.

use std::ops::Range;

/// What a chip refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InlineTokenKind {
    /// A workspace path (decoded).
    Mention { path: String },
    /// A provider skill name, without the sigil.
    Skill { name: String },
    /// A context record referenced by id. `image` for the `![...]` form.
    Context {
        kind: String,
        context_id: String,
        label: String,
        image: bool,
    },
    /// A quoted assistant message, with the user's optional comment.
    Citation {
        text: String,
        comment: Option<String>,
    },
}

/// One recognized chip and the bytes of the prompt it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineToken {
    pub kind: InlineTokenKind,
    pub range: Range<usize>,
}

/// Every chip in `text`, sorted by start (`collectComposerPromptInlineTokens`). Context links
/// and citations win over a mention or skill they overlap (an unfinished `@` must not eat the
/// start of a link label).
pub fn collect_inline_tokens(text: &str) -> Vec<InlineToken> {
    let mut tokens = Vec::new();
    scan(text, &mut tokens, parse_file_link);
    scan(text, &mut tokens, parse_at_mention);
    scan(text, &mut tokens, parse_skill);
    let links = collect_links(text);
    if !links.is_empty() {
        tokens.retain(|token| {
            !links.iter().any(|link| {
                token.range.start < link.range.end && token.range.end > link.range.start
            })
        });
        tokens.extend(links);
    }
    tokens.sort_by_key(|token| token.range.start);
    tokens
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

/// Longest label a file or context link may have (bounds the scan on `" [[[[..."`).
const MAX_LINK_LABEL_CHARS: usize = 512;

/// `[label](path)` where `label` is the path's basename and the path has no URI scheme.
fn parse_file_link(text: &str, start: usize) -> Option<(usize, InlineTokenKind)> {
    let rest = text[start..].strip_prefix('[')?;
    let (label, after_label) = take_escaped(rest, ']')?;
    if label.chars().filter(|c| *c != '\\').count() > MAX_LINK_LABEL_CHARS {
        return None;
    }
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

/// `@path` (no whitespace, `@` or `"`) or `@"quoted path"` with backslash escapes. A bare
/// `@scope/package` stays text: autocomplete writes file links, so ambiguous package names are
/// left alone.
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
    let path = &rest[..len];
    (len > 0 && !is_scoped_package_reference(path)).then(|| {
        (
            start + 1 + len,
            InlineTokenKind::Mention {
                path: path.to_owned(),
            },
        )
    })
}

/// `^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._-]*(?:/[^\s@"]+)*$` (`SCOPED_PACKAGE_REFERENCE_REGEX`).
fn is_scoped_package_reference(path: &str) -> bool {
    let segment = |part: &str| {
        let mut chars = part.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && chars.all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
            })
    };
    let mut parts = path.split('/');
    let (Some(scope), Some(name)) = (parts.next(), parts.next()) else {
        return false;
    };
    // The rest is `[^\s@"]+` per segment; the caller already stopped at whitespace, `@`, `"`.
    segment(scope) && segment(name) && parts.all(|part| !part.is_empty())
}

/// A skill: a currency sign, then `[a-zA-Z0-9][a-zA-Z0-9:_-]*` with at least one letter.
/// Amounts stay prose: `$20`, `$20k`, `$100M` and `$1e6` are not skills.
fn parse_skill(text: &str, start: usize) -> Option<(usize, InlineTokenKind)> {
    let sigil = text[start..].chars().next()?;
    if !is_currency_symbol(sigil) {
        return None;
    }
    let rest = &text[start + sigil.len_utf8()..];
    let len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-')))
        .unwrap_or(rest.len());
    let name = &rest[..len];
    if !name.chars().next()?.is_ascii_alphanumeric()
        || !name.chars().any(|c| c.is_ascii_alphabetic())
        || is_amount(rest)
    {
        return None;
    }
    Some((
        start + sigil.len_utf8() + len,
        InlineTokenKind::Skill {
            name: name.to_owned(),
        },
    ))
}

/// `[0-9][0-9_]*(?:[kKmMbBtT]|[eE][0-9]+)?` followed by whitespace or the end.
fn is_amount(rest: &str) -> bool {
    let bytes = rest.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    let mut index = 1;
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
        index += 1;
    }
    let at_boundary = |index: usize| rest[index..].chars().next().is_none_or(char::is_whitespace);
    if at_boundary(index) {
        return true;
    }
    if index < bytes.len() && b"kKmMbBtT".contains(&bytes[index]) && at_boundary(index + 1) {
        return true;
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let mut exponent = index + 1;
        while exponent < bytes.len() && bytes[exponent].is_ascii_digit() {
            exponent += 1;
        }
        return exponent > index + 1 && at_boundary(exponent);
    }
    false
}

/// Unicode general category `Sc` (currency symbols), which the web matches with `\p{Sc}`.
pub fn is_currency_symbol(c: char) -> bool {
    matches!(c,
        '$' | '\u{A2}'..='\u{A5}' | '\u{58F}' | '\u{60B}' | '\u{7FE}' | '\u{7FF}'
        | '\u{9F2}' | '\u{9F3}' | '\u{9FB}' | '\u{AF1}' | '\u{BF9}' | '\u{E3F}' | '\u{17DB}'
        | '\u{20A0}'..='\u{20C0}' | '\u{A838}' | '\u{FDFC}' | '\u{FE69}' | '\u{FF04}'
        | '\u{FFE0}' | '\u{FFE1}' | '\u{FFE5}' | '\u{FFE6}' | '\u{11FDD}'..='\u{11FE0}'
        | '\u{1E2FF}' | '\u{1ECB0}')
}

const CONTEXT_HREF_PREFIX: &str = "t3-context://v1/";
const CITATION_HREF_PREFIX: &str = "t3-citation://v1/";
/// Longest context label (`COMPOSER_CONTEXT_LABEL_MAX_CHARS`).
pub const CONTEXT_LABEL_MAX_CHARS: usize = 200;

/// Context links (`(!?)\[([^\]\n]{0,512})\]\((t3-context://v1/[^\s)]{1,200})\)`) and citations
/// (`\[Assistant quote\]\((t3-citation://v1/[^\s)]+)\)`), anywhere in the text.
fn collect_links(text: &str) -> Vec<InlineToken> {
    let mut links = Vec::new();
    if !text.contains("](t3-") {
        return links;
    }
    let mut search = 0;
    while let Some(offset) = text[search..].find('[') {
        let open = search + offset;
        search = open + 1;
        let Some(close) = text[open + 1..].find(']').map(|i| open + 1 + i) else {
            break;
        };
        let label = &text[open + 1..close];
        if label.contains('\n') || label.chars().count() > MAX_LINK_LABEL_CHARS {
            continue;
        }
        let Some(after) = text[close + 1..].strip_prefix('(') else {
            continue;
        };
        let href_len = after
            .find(|c: char| c == ')' || c.is_whitespace())
            .unwrap_or(after.len());
        if !after[href_len..].starts_with(')') {
            continue;
        }
        let href = &after[..href_len];
        let end = close + 2 + href_len + 1;
        let image = open > 0 && text.as_bytes()[open - 1] == b'!';
        let kind = if let Some(rest) = href.strip_prefix(CONTEXT_HREF_PREFIX) {
            if href_len > CONTEXT_HREF_PREFIX.len() + 200 {
                continue;
            }
            let Some((kind, context_id)) = parse_context_path(rest) else {
                continue;
            };
            InlineTokenKind::Context {
                label: sanitize_context_label(label, &kind),
                kind,
                context_id,
                image,
            }
        } else if label == "Assistant quote" && href.starts_with(CITATION_HREF_PREFIX) {
            let Some((text, comment)) = parse_citation(href) else {
                continue;
            };
            InlineTokenKind::Citation { text, comment }
        } else {
            continue;
        };
        let start = if image && matches!(kind, InlineTokenKind::Context { .. }) {
            open - 1
        } else {
            open
        };
        links.push(InlineToken {
            kind,
            range: start..end,
        });
        search = end;
    }
    links
}

/// `<kind>/<contextId>` with kind `[a-z][a-z0-9-]{0,39}` and id `[a-z0-9_-]{1,128}` (any case).
fn parse_context_path(rest: &str) -> Option<(String, String)> {
    let mut parts = rest.split('/');
    let (kind, id) = (parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let kind_ok = kind.len() <= 40
        && kind.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && kind
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let id_ok = (1..=128).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    (kind_ok && id_ok).then(|| (kind.to_owned(), id.to_owned()))
}

/// Labels survive a Markdown link: brackets, backslashes and line breaks become spaces,
/// whitespace collapses, at most 200 chars, never empty (`sanitizeComposerContextLabel`).
pub fn sanitize_context_label(label: &str, kind: &str) -> String {
    let replaced: String = label
        .chars()
        .map(|c| {
            if matches!(c, '[' | ']' | '\\' | '\r' | '\n') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned: String = replaced
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(CONTEXT_LABEL_MAX_CHARS)
        .collect();
    if cleaned.is_empty() {
        kind.to_owned()
    } else {
        cleaned
    }
}

/// The prompt text for a context chip (`formatComposerContextReference`).
pub fn serialize_context_reference(kind: &str, context_id: &str, label: &str) -> String {
    let label = sanitize_context_label(label, kind);
    let bang = if kind == "image" { "!" } else { "" };
    format!("{bang}[{label}]({CONTEXT_HREF_PREFIX}{kind}/{context_id})")
}

/// The quoted text and comment of a citation href (`parseAssistantCitationHref`): three path
/// parts (environment, thread, message) and exactly `text start end prefix suffix [comment]`.
fn parse_citation(href: &str) -> Option<(String, Option<String>)> {
    let rest = href.strip_prefix(CITATION_HREF_PREFIX)?;
    let (path, query) = rest.split_once('?')?;
    if path.split('/').count() != 3 || query.contains('#') {
        return None;
    }
    let mut fields: Vec<(String, String)> = Vec::new();
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        fields.push((form_decode(key)?, form_decode(value)?));
    }
    let count = |key: &str| fields.iter().filter(|(k, _)| k == key).count();
    let required = ["text", "start", "end", "prefix", "suffix"];
    let has_comment = count("comment") == 1;
    if required.iter().any(|key| count(key) != 1)
        || fields.len() != required.len() + usize::from(has_comment)
    {
        return None;
    }
    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    let numeric = |value: Option<String>| {
        value.is_some_and(|v| (1..=16).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit()))
    };
    if !numeric(get("start")) || !numeric(get("end")) {
        return None;
    }
    Some((get("text")?, get("comment")))
}

/// `application/x-www-form-urlencoded` decoding: `+` is a space, then percent escapes.
fn form_decode(text: &str) -> Option<String> {
    percent_decode(&text.replace('+', " "))
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
    /// `#query`: pull requests of the project's repository.
    PullRequest,
    /// `/query` at the start of a line.
    SlashCommand,
    /// `$query` (any currency sign): provider skills.
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
        .find(|&(_, c)| matches!(c, ' ' | '\n' | '\t' | '\r'))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let token = &text[token_start..cursor];
    let first = token.chars().next()?;
    let query = &token[first.len_utf8()..];
    let kind = match first {
        // `#` then nothing, or a letter/digit followed by letters, digits, `_`, `-`.
        '#' if query.is_empty()
            || (query.chars().next().is_some_and(char::is_alphanumeric)
                && query
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))) =>
        {
            TriggerKind::PullRequest
        }
        c if is_currency_symbol(c) => TriggerKind::Skill,
        '@' => TriggerKind::Path,
        _ => return None,
    };
    Some(Trigger {
        kind,
        query: query.to_owned(),
        range: token_start..cursor,
    })
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
