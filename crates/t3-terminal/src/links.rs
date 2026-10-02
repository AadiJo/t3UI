//! URL and file path detection in terminal text, ported from the fork's
//! `apps/web/src/terminal-links.ts`. Offsets are byte offsets into the scanned line (the
//! reference uses UTF-16 indices; the view maps bytes back to cells itself).

use std::sync::LazyLock;

use regex::Regex;

/// What a detected link points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalLinkKind {
    /// `http://` or `https://` URL.
    Url,
    /// Absolute, home-relative, or relative file path, optionally with `:line[:column]`.
    Path,
}

/// A link found in a line of terminal text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkMatch {
    pub kind: TerminalLinkKind,
    pub text: String,
    /// Byte range of `text` in the scanned line.
    pub start: usize,
    pub end: usize,
}

static URL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s"'`<>]+"#).expect("url pattern"));
static FILE_PATH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:~/|\.{1,2}/|/|[A-Za-z]:[\\/]|\\\\)[^\s"'`<>]+|[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)+(?::[0-9]+){0,2}"#,
    )
    .expect("path pattern")
});

/// Finds URLs and paths in one logical line (soft-wrapped rows already joined). URLs win over
/// overlapping paths; results are sorted by start offset.
pub(crate) fn extract_links(line: &str) -> Vec<LinkMatch> {
    let urls = collect_matches(line, TerminalLinkKind::Url, &URL_PATTERN, &[]);
    let paths = collect_matches(line, TerminalLinkKind::Path, &FILE_PATH_PATTERN, &urls);
    let mut links = urls;
    links.extend(paths);
    links.sort_by_key(|link| link.start);
    links
}

fn collect_matches(
    line: &str,
    kind: TerminalLinkKind,
    pattern: &Regex,
    existing: &[LinkMatch],
) -> Vec<LinkMatch> {
    let mut matches: Vec<LinkMatch> = Vec::new();
    for found in pattern.find_iter(line) {
        let trimmed = trim_closing_delimiters(found.as_str());
        if trimmed.is_empty() {
            continue;
        }
        if kind == TerminalLinkKind::Path && starts_with_http_scheme(trimmed) {
            continue;
        }
        let candidate = LinkMatch {
            kind,
            text: trimmed.to_owned(),
            start: found.start(),
            end: found.start() + trimmed.len(),
        };
        let collides = existing
            .iter()
            .chain(&matches)
            .any(|other| candidate.start < other.end && other.start < candidate.end);
        if !collides {
            matches.push(candidate);
        }
    }
    matches
}

fn starts_with_http_scheme(text: &str) -> bool {
    let lower = text.get(..8).unwrap_or(text).to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Drops trailing `.,;!?`, then unbalanced closing `)`, `]`, `}`.
fn trim_closing_delimiters(value: &str) -> &str {
    let mut output = value.trim_end_matches(['.', ',', ';', '!', '?']);
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        while output.ends_with(close) {
            let opens = output.matches(open).count();
            let closes = output.matches(close).count();
            if opens >= closes {
                break;
            }
            output = &output[..output.len() - close.len_utf8()];
        }
    }
    output
}

/// Splits a trailing `:line[:column]` off a path.
pub(crate) fn split_path_and_position(value: &str) -> (&str, Option<&str>, Option<&str>) {
    let Some((path, column)) = split_trailing_number(value) else {
        return (value, None, None);
    };
    match split_trailing_number(path) {
        Some((path, line)) => (path, Some(line), Some(column)),
        None => (path, Some(column), None),
    }
}

/// `"a:12"` → `("a", "12")`; `None` unless the value ends with `:` and ASCII digits.
fn split_trailing_number(value: &str) -> Option<(&str, &str)> {
    let digits_start = value.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits_start == value.len() {
        return None;
    }
    let head = value[..digits_start].strip_suffix(':')?;
    Some((head, &value[digits_start..]))
}

/// Resolves a clicked path link against the terminal's cwd for `shell.openInEditor`, keeping
/// any `:line[:column]` suffix (`terminal-links.ts` `resolvePathLinkTarget`). `~/` resolves to a
/// home directory inferred from `cwd` (`/Users/x`, `/home/x`, `C:\Users\x`).
pub fn resolve_path_link_target(raw_path: &str, cwd: &str) -> String {
    let (path, line, column) = split_path_and_position(raw_path);
    let resolved = if let Some(rest) = path.strip_prefix("~/") {
        match infer_home_from_cwd(cwd) {
            Some(home) => join_path(home, rest, is_windows_path_style(home)),
            None => path.to_owned(),
        }
    } else if !is_absolute_path(path) {
        join_path(cwd, path, is_windows_path_style(cwd))
    } else {
        path.to_owned()
    };
    match (line, column) {
        (Some(line), Some(column)) => format!("{resolved}:{line}:{column}"),
        (Some(line), None) => format!("{resolved}:{line}"),
        _ => resolved,
    }
}

fn join_path(base: &str, next: &str, windows: bool) -> String {
    let base = base.trim_end_matches(['/', '\\']);
    if windows {
        format!("{base}\\{}", next.replace('/', "\\"))
    } else {
        format!("{base}/{}", next.trim_start_matches('/'))
    }
}

fn infer_home_from_cwd(cwd: &str) -> Option<&str> {
    for prefix in ["/Users/", "/home/"] {
        if let Some(rest) = cwd.strip_prefix(prefix) {
            let user_len = rest.find('/').unwrap_or(rest.len());
            return (user_len > 0).then(|| &cwd[..prefix.len() + user_len]);
        }
    }
    let bytes = cwd.as_bytes();
    if bytes.len() > 9 && bytes[0].is_ascii_alphabetic() && cwd[1..].starts_with(":\\Users\\") {
        let rest = &cwd[9..];
        let user_len = rest.find('\\').unwrap_or(rest.len());
        return (user_len > 0).then(|| &cwd[..9 + user_len]);
    }
    None
}

fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
        || value.starts_with("\\\\")
}

fn is_absolute_path(value: &str) -> bool {
    value.starts_with('/') || is_windows_absolute_path(value)
}

fn is_windows_path_style(value: &str) -> bool {
    is_windows_absolute_path(value)
        || value
            .as_bytes()
            .windows(3)
            .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && w[2] == b'\\')
}

#[cfg(test)]
mod tests {
    //! Failure modes covered, listed before the tests were written:
    //! 1. Trailing sentence punctuation or an unbalanced `)`/`]`/`}` stays in the link, or a
    //!    balanced `(...)` inside a URL gets cut.
    //! 2. Quotes, backticks, angle brackets or whitespace do not end a link.
    //! 3. A path regex match inside a URL is reported as a second link, or a URL is reported as
    //!    a path.
    //! 4. Path forms are missed: `/abs`, `~/x`, `./x`, `../x`, `a/b`, `a/b:12:4`, `C:\x`,
    //!    `\\server\share`; a bare word without a slash is reported.
    //! 5. Results are not sorted by position when paths precede URLs.
    //! 6. Non-ASCII text before a link shifts offsets so they no longer slice the link.
    //! 7. Resolution: relative paths ignore cwd, `~/` ignores the inferred home, absolute paths
    //!    get joined, `:line[:col]` is dropped, Windows cwd joins with `/`, a trailing slash on
    //!    cwd doubles the separator.
    use super::*;

    fn texts(line: &str) -> Vec<(TerminalLinkKind, String)> {
        extract_links(line)
            .into_iter()
            .map(|link| (link.kind, link.text))
            .collect()
    }

    use TerminalLinkKind::{Path, Url};

    #[test]
    fn trims_trailing_punctuation_and_unbalanced_closers() {
        assert_eq!(
            texts("see https://t3.chat/docs."),
            [(Url, "https://t3.chat/docs".into())]
        );
        assert_eq!(
            texts("(https://t3.chat/a)"),
            [(Url, "https://t3.chat/a".into())]
        );
        assert_eq!(
            texts("https://en.wikipedia.org/wiki/Rust_(language)!"),
            [(Url, "https://en.wikipedia.org/wiki/Rust_(language)".into())]
        );
        assert_eq!(texts("[./src/main.rs]"), [(Path, "./src/main.rs".into())]);
        assert_eq!(texts("http://a.b/?x=1;"), [(Url, "http://a.b/?x=1".into())]);
    }

    #[test]
    fn delimiters_end_links() {
        assert_eq!(
            texts("\"https://a.dev/x\" next"),
            [(Url, "https://a.dev/x".into())]
        );
        assert_eq!(texts("<https://a.dev>`b`"), [(Url, "https://a.dev".into())]);
        assert_eq!(texts("'/tmp/x' y"), [(Path, "/tmp/x".into())]);
    }

    #[test]
    fn url_wins_over_paths() {
        assert_eq!(
            texts("open https://github.com/a/b/pull/1 now"),
            [(Url, "https://github.com/a/b/pull/1".into())]
        );
        // The URL pattern is case-sensitive, so this falls through to the path pattern.
        assert_eq!(texts("HTTPS://X.COM/a/b"), [(Path, "S://X.COM/a/b".into())]);
    }

    #[test]
    fn path_forms() {
        assert_eq!(texts("/usr/local/bin"), [(Path, "/usr/local/bin".into())]);
        assert_eq!(
            texts("cat ~/notes/todo.md"),
            [(Path, "~/notes/todo.md".into())]
        );
        assert_eq!(texts("./run.sh"), [(Path, "./run.sh".into())]);
        assert_eq!(texts("../lib/a.ts"), [(Path, "../lib/a.ts".into())]);
        assert_eq!(
            texts("error in src/app.rs:12:4"),
            [(Path, "src/app.rs:12:4".into())]
        );
        assert_eq!(
            texts(r"C:\Users\me\x.txt"),
            [(Path, r"C:\Users\me\x.txt".into())]
        );
        assert_eq!(texts(r"\\server\share"), [(Path, r"\\server\share".into())]);
        assert_eq!(texts("README.md and words"), []);
    }

    #[test]
    fn sorted_with_valid_offsets() {
        let line = "→ ✓ src/a.rs then https://x.dev and /tmp/b";
        let links = extract_links(line);
        assert_eq!(
            links.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["src/a.rs", "https://x.dev", "/tmp/b"]
        );
        for link in &links {
            assert_eq!(&line[link.start..link.end], link.text);
        }
    }

    #[test]
    fn splits_positions() {
        assert_eq!(
            split_path_and_position("a.rs:12:4"),
            ("a.rs", Some("12"), Some("4"))
        );
        assert_eq!(
            split_path_and_position("a.rs:12"),
            ("a.rs", Some("12"), None)
        );
        assert_eq!(split_path_and_position("a.rs"), ("a.rs", None, None));
        assert_eq!(split_path_and_position("a.rs:"), ("a.rs:", None, None));
    }

    #[test]
    fn resolves_paths() {
        assert_eq!(
            resolve_path_link_target("src/a.rs:3", "/Users/me/p/"),
            "/Users/me/p/src/a.rs:3"
        );
        assert_eq!(
            resolve_path_link_target("./a", "/home/me/p"),
            "/home/me/p/./a"
        );
        assert_eq!(
            resolve_path_link_target("~/x/y:1:2", "/Users/me/p"),
            "/Users/me/x/y:1:2"
        );
        assert_eq!(resolve_path_link_target("~/x", "/home/me"), "/home/me/x");
        assert_eq!(resolve_path_link_target("~/x", "/srv/app"), "~/x");
        assert_eq!(
            resolve_path_link_target("/etc/hosts", "/Users/me"),
            "/etc/hosts"
        );
        assert_eq!(
            resolve_path_link_target("src/a.rs", r"C:\work\p"),
            r"C:\work\p\src\a.rs"
        );
        assert_eq!(
            resolve_path_link_target("~/a/b", r"C:\Users\me\p"),
            r"C:\Users\me\a\b"
        );
    }
}
