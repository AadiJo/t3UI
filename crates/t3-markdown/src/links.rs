//! Link classification, ported from the fork's `markdown-links.ts`, `terminal-links.ts`
//! (`splitPathAndPosition`, `resolvePathLinkTarget`) and `filePathDisplay.ts`.
//!
//! A markdown link whose destination looks like a filesystem path renders as a file chip instead
//! of a text link. Everything here is string logic, so it is tested without GPUI.

use std::collections::HashMap;

/// A link destination that resolved to a file (`resolveMarkdownFileLinkMeta`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileLink {
    /// Absolute path without the `:line:col` suffix (used for the icon and grouping).
    pub file_path: String,
    /// Absolute path with the position suffix; what "open in editor" receives.
    pub target_path: String,
    /// Tooltip text: workspace-label-relative path with the position suffix.
    pub display_path: String,
    /// Path relative to the workspace root, when the file is inside it.
    pub workspace_relative_path: Option<String>,
    pub basename: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

const POSIX_FILE_ROOT_PREFIXES: &[&str] = &[
    "/Users/",
    "/home/",
    "/tmp/",
    "/var/",
    "/etc/",
    "/opt/",
    "/mnt/",
    "/Volumes/",
    "/private/",
    "/root/",
];

/// `value.trim()` without surrounding `<...>` (`normalizeMarkdownLinkDestination`).
pub fn normalize_destination(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(value)
}

/// Percent-decodes `value`, returning it unchanged when it is not valid (`safeDecode`).
fn safe_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            match hex {
                Some(byte) => {
                    out.push(byte);
                    i += 3;
                    continue;
                }
                None => return value.to_string(),
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_string())
}

fn is_windows_drive_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

fn is_windows_unc_path(path: &str) -> bool {
    path.starts_with("\\\\")
}

/// `/C:/x` -> `C:/x` (browsers report Windows file URLs with a leading slash).
fn normalize_windows_drive_path(path: &str) -> &str {
    match path.strip_prefix('/') {
        Some(rest) if is_windows_drive_path(rest) => rest,
        _ => path,
    }
}

/// Splits `file://host/path?query#hash` into (path, hash), like `new URL(href)`.
fn parse_file_url(href: &str, decode: bool) -> Option<(String, String)> {
    let rest = href
        .get(..5)?
        .eq_ignore_ascii_case("file:")
        .then(|| &href[5..])?;
    let rest = rest.strip_prefix("//").map_or(rest, |after| {
        // Skip the (usually empty) host.
        after.find('/').map_or("", |slash| &after[slash..])
    });
    let (before_hash, hash) = rest.split_at(rest.find('#').unwrap_or(rest.len()));
    let path = &before_hash[..before_hash.find('?').unwrap_or(before_hash.len())];
    if path.is_empty() {
        return None;
    }
    let path = normalize_windows_drive_path(path);
    let path = if decode {
        safe_decode(path)
    } else {
        path.to_string()
    };
    Some((path, hash.to_string()))
}

/// Rewrites `file://` hrefs to plain paths, keeping the hash (`rewriteMarkdownFileUriHref`).
pub fn rewrite_file_uri(href: &str) -> Option<String> {
    let (path, hash) = parse_file_url(normalize_destination(href), false)?;
    Some(format!("{path}{hash}"))
}

/// Splits `path:line:col` (`splitPathAndPosition`).
pub fn split_path_and_position(value: &str) -> (&str, Option<&str>, Option<&str>) {
    fn trailing_number(value: &str) -> Option<(&str, &str)> {
        let colon = value.rfind(':')?;
        let digits = &value[colon + 1..];
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| (&value[..colon], digits))
    }
    let Some((rest, last)) = trailing_number(value) else {
        return (value, None, None);
    };
    match trailing_number(rest) {
        Some((path, line)) => (path, Some(line), Some(last)),
        None => (rest, Some(last), None),
    }
}

fn has_position_suffix(path: &str) -> bool {
    split_path_and_position(path).1.is_some()
}

fn looks_like_posix_filesystem_path(path: &str) -> bool {
    if !path.starts_with('/') {
        return false;
    }
    if POSIX_FILE_ROOT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return true;
    }
    if has_position_suffix(path) {
        return true;
    }
    let basename = &path[path.rfind('/').map_or(0, |slash| slash + 1)..];
    has_extension(basename)
}

/// `/\.[A-Za-z0-9_-]+$/`
fn has_extension(name: &str) -> bool {
    name.rfind('.').is_some_and(|dot| {
        let extension = &name[dot + 1..];
        !extension.is_empty()
            && extension
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    })
}

fn is_path_segment_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// `RELATIVE_FILE_PATH_PATTERN` / `RELATIVE_FILE_NAME_PATTERN`.
fn is_relative_file_like(path: &str) -> bool {
    // Strip up to two `:digits` suffixes.
    let mut core = path;
    for _ in 0..2 {
        if let Some(colon) = core.rfind(':') {
            let digits = &core[colon + 1..];
            if !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()) {
                core = &core[..colon];
            }
        }
    }
    if core.is_empty() || core.contains("::") {
        return false;
    }
    let segments: Vec<&str> = core.split('/').collect();
    let all_valid = segments
        .iter()
        .all(|segment| !segment.is_empty() && segment.bytes().all(is_path_segment_byte));
    if !all_valid {
        return false;
    }
    if segments.len() >= 2 {
        return true;
    }
    // A single segment needs `name.ext` with an extension of [A-Za-z0-9_-].
    core.rfind('.').is_some_and(|dot| {
        dot > 0
            && core[dot + 1..]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            && dot + 1 < core.len()
    })
}

fn has_relative_prefix(path: &str) -> bool {
    path.starts_with("~/") || path.starts_with("./") || path.starts_with("../")
}

fn is_likely_path_candidate(path: &str) -> bool {
    if is_windows_drive_path(path) || is_windows_unc_path(path) || has_relative_prefix(path) {
        return true;
    }
    if path.starts_with('/') {
        return looks_like_posix_filesystem_path(path);
    }
    is_relative_file_like(path)
}

fn is_relative_path(path: &str) -> bool {
    has_relative_prefix(path)
        || (!path.starts_with('/') && !is_windows_drive_path(path) && !is_windows_unc_path(path))
}

/// `scheme:rest` where rest starts with `//` or is not a bare `line[:col]`.
fn has_external_scheme(path: &str) -> bool {
    let Some(colon) = path.find(':') else {
        return false;
    };
    let scheme = &path[..colon];
    let valid_scheme = scheme
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'));
    if !valid_scheme {
        return false;
    }
    let rest = &path[colon + 1..];
    if rest.starts_with("//") {
        return true;
    }
    let position_only = {
        let (line, column) = rest.split_once(':').unwrap_or((rest, ""));
        !line.is_empty()
            && line.bytes().all(|byte| byte.is_ascii_digit())
            && (column.is_empty() && !rest.ends_with(':')
                || !column.is_empty() && column.bytes().all(|byte| byte.is_ascii_digit()))
    };
    !position_only
}

fn append_line_column_from_hash(path: &str, hash: &str) -> String {
    if hash.is_empty() || has_position_suffix(path) {
        return path.to_string();
    }
    let parsed = hash
        .strip_prefix('#')
        .and_then(|rest| rest.strip_prefix(['L', 'l']))
        .and_then(|rest| {
            let line_end = rest
                .find(|ch: char| !ch.is_ascii_digit())
                .unwrap_or(rest.len());
            let line = &rest[..line_end];
            if line.is_empty() {
                return None;
            }
            let after = &rest[line_end..];
            if after.is_empty() {
                return Some((line, None));
            }
            let column = after.strip_prefix(['C', 'c'])?;
            (!column.is_empty() && column.bytes().all(|byte| byte.is_ascii_digit()))
                .then_some((line, Some(column)))
        });
    match parsed {
        Some((line, Some(column))) => format!("{path}:{line}:{column}"),
        Some((line, None)) => format!("{path}:{line}"),
        None => path.to_string(),
    }
}

fn is_windows_path_style(value: &str) -> bool {
    is_windows_drive_path(value)
        || is_windows_unc_path(value)
        || value.as_bytes().windows(3).any(|window| {
            window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\'
        })
}

fn join_path(base: &str, next: &str, windows: bool) -> String {
    let base = base.trim_end_matches(['/', '\\']);
    if windows {
        format!("{base}\\{}", next.replace('/', "\\"))
    } else {
        format!("{base}/{}", next.trim_start_matches('/'))
    }
}

fn infer_home_from_cwd(cwd: &str) -> Option<String> {
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = cwd.strip_prefix(prefix) {
            let user = rest.split('/').next().unwrap_or("");
            if !user.is_empty() {
                return Some(format!("{prefix}{user}"));
            }
        }
    }
    if is_windows_drive_path(cwd) && cwd[3..].to_ascii_lowercase().starts_with("users\\") {
        let user = cwd[9..].split('\\').next().unwrap_or("");
        if !user.is_empty() {
            return Some(format!("{}{user}", &cwd[..9]));
        }
    }
    None
}

/// `resolvePathLinkTarget`: makes a relative path absolute against `cwd`, keeping the position.
fn resolve_path_link_target(raw_path: &str, cwd: &str) -> String {
    let (path, line, column) = split_path_and_position(raw_path);
    let resolved = if let Some(rest) = path.strip_prefix("~/") {
        match infer_home_from_cwd(cwd) {
            Some(home) => join_path(&home, rest, is_windows_path_style(&home)),
            None => path.to_string(),
        }
    } else if !(path.starts_with('/') || is_windows_drive_path(path) || is_windows_unc_path(path)) {
        join_path(cwd, path, is_windows_path_style(cwd))
    } else {
        path.to_string()
    };
    match (line, column) {
        (Some(line), Some(column)) => format!("{resolved}:{line}:{column}"),
        (Some(line), None) => format!("{resolved}:{line}"),
        _ => resolved,
    }
}

/// `resolveMarkdownFileLinkTarget`: the absolute `path[:line[:col]]` for a file link, or `None`.
pub fn resolve_file_link_target(href: &str, cwd: Option<&str>) -> Option<String> {
    let raw = normalize_destination(href);
    if raw.is_empty() || raw.starts_with('#') {
        return None;
    }
    let (path, hash, from_file_url) = match parse_file_url(raw, true) {
        Some((path, hash)) => (path.trim().to_string(), hash, true),
        None => {
            let (before_hash, hash) = raw.split_at(raw.find('#').unwrap_or(raw.len()));
            let path = &before_hash[..before_hash.find('?').unwrap_or(before_hash.len())];
            (path.to_string(), hash.to_string(), false)
        }
    };
    let decoded = if from_file_url {
        path
    } else {
        safe_decode(path.trim())
    };
    let decoded = normalize_windows_drive_path(&decoded).to_string();
    let hash = safe_decode(hash.trim());
    if decoded.is_empty() {
        return None;
    }
    if !is_windows_drive_path(&decoded)
        && !is_windows_unc_path(&decoded)
        && has_external_scheme(&decoded)
    {
        return None;
    }
    if !is_likely_path_candidate(&decoded) {
        return None;
    }
    let with_position = append_line_column_from_hash(&decoded, &hash);
    if !is_relative_path(&with_position) {
        return Some(with_position);
    }
    cwd.map(|cwd| resolve_path_link_target(&with_position, cwd))
}

fn basename(path: &str) -> &str {
    &path[path.rfind(['/', '\\']).map_or(0, |separator| separator + 1)..]
}

fn workspace_relative_path(path: &str, root: Option<&str>) -> Option<String> {
    let root = root?;
    let path = path.replace('\\', "/");
    let path = normalize_windows_drive_path(&path);
    let root = root.replace('\\', "/");
    let root = normalize_windows_drive_path(&root).trim_end_matches('/');
    let prefix = format!("{}/", root.to_lowercase());
    path.to_lowercase()
        .starts_with(&prefix)
        .then(|| path[root.len() + 1..].to_string())
}

/// `formatWorkspaceRelativePath`: the tooltip path shown for a chip.
pub fn format_workspace_relative_path(path_with_position: &str, root: Option<&str>) -> String {
    let (path, line, column) = split_path_and_position(path_with_position);
    let normalized = path.replace('\\', "/");
    let normalized = normalize_windows_drive_path(&normalized).to_string();
    let mut display = normalized.clone();
    if let Some(root) = root {
        let root = root.trim_end_matches(['/', '\\']).replace('\\', "/");
        let root = normalize_windows_drive_path(&root).to_string();
        let label = basename(&root).to_string();
        let path_lower = normalized.to_lowercase();
        let root_lower = root.to_lowercase();
        if path_lower == root_lower {
            display = label;
        } else if path_lower.starts_with(&format!("{root_lower}/")) {
            display = format!("{label}/{}", &normalized[root.len() + 1..]);
        } else if !normalized.starts_with('/') {
            let relative = normalized.trim_start_matches("./").trim_start_matches('/');
            display = if path_lower.starts_with(&format!("{}/", label.to_lowercase())) {
                normalized.clone()
            } else {
                format!("{label}/{relative}")
            };
        }
    }
    match (line, column) {
        (Some(line), Some(column)) => format!("{display}:{line}:{column}"),
        (Some(line), None) => format!("{display}:{line}"),
        _ => display,
    }
}

/// `resolveMarkdownFileLinkMeta`.
pub fn resolve_file_link(href: &str, cwd: Option<&str>) -> Option<FileLink> {
    let target_path = resolve_file_link_target(href, cwd)?;
    let (path, line, column) = split_path_and_position(&target_path);
    Some(FileLink {
        file_path: path.to_string(),
        display_path: format_workspace_relative_path(&target_path, cwd),
        workspace_relative_path: workspace_relative_path(path, cwd),
        basename: basename(path).to_string(),
        line: line.and_then(|line| line.parse().ok()),
        column: column.and_then(|column| column.parse().ok()),
        target_path: target_path.clone(),
    })
}

/// `buildFileLinkParentSuffixByPath`: for file chips whose basenames collide, the shortest
/// unique parent path (at least two segments) to show after the name.
pub fn parent_suffixes<'a>(paths: impl IntoIterator<Item = &'a str>) -> HashMap<String, String> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for path in paths {
        let normalized = path.replace('\\', "/");
        let Some(name) = normalized.split('/').rfind(|part| !part.is_empty()) else {
            continue;
        };
        let group = groups.entry(name.to_string()).or_default();
        if !group.iter().any(|existing| existing == path) {
            group.push(path.to_string());
        }
    }
    let parents = |path: &str| -> Vec<String> {
        let segments: Vec<String> = path
            .replace('\\', "/")
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        segments[..segments.len().saturating_sub(1)].to_vec()
    };
    let suffix = |segments: &[String], depth: usize| segments[segments.len() - depth..].join("/");
    let mut out = HashMap::new();
    for group in groups.values().filter(|group| group.len() >= 2) {
        for path in group {
            let segments = parents(path);
            if segments.is_empty() {
                continue;
            }
            let unique_depth = (1..=segments.len())
                .find(|&depth| {
                    let candidate = suffix(&segments, depth);
                    !group.iter().any(|other| {
                        let other_segments = parents(other);
                        other != path
                            && other_segments.len() >= depth
                            && suffix(&other_segments, depth) == candidate
                    })
                })
                .unwrap_or(segments.len());
            let depth = segments.len().min(unique_depth.max(2));
            out.insert(path.clone(), suffix(&segments, depth));
        }
    }
    out
}

/// react-markdown's `defaultUrlTransform`: keeps relative URLs and safe protocols, drops the rest.
pub fn is_safe_url(url: &str) -> bool {
    let colon = url.find(':');
    let question = url.find('?');
    let hash = url.find('#');
    let slash = url.find('/');
    let Some(colon) = colon else {
        return true;
    };
    // A colon after a path/query/hash separator is not a protocol.
    if slash.is_some_and(|slash| colon > slash)
        || question.is_some_and(|question| colon > question)
        || hash.is_some_and(|hash| colon > hash)
    {
        return true;
    }
    let protocol = url[..colon].to_ascii_lowercase();
    matches!(
        protocol.as_str(),
        "http" | "https" | "mailto" | "xmpp" | "irc" | "ircs"
    )
}
