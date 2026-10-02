//! Project path helpers (`packages/client-runtime/src/state/projects.ts`,
//! `packages/shared/src/path.ts`).

fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\'))
}

fn is_unc_path(value: &str) -> bool {
    value.starts_with("\\\\")
}

fn is_root_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    value == "/"
        || value == "\\"
        || (bytes.len() >= 2
            && bytes.len() <= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes.len() == 2 || matches!(bytes[2], b'/' | b'\\')))
}

fn trim_trailing_separators(value: &str) -> String {
    if value.is_empty() || is_root_path(value) {
        return value.to_owned();
    }
    let is_unix = !(is_windows_drive_path(value) || is_unc_path(value)) && value.starts_with('/');
    let trimmed = if is_unix {
        value.trim_end_matches('/')
    } else {
        value.trim_end_matches(['/', '\\'])
    };
    if trimmed.is_empty() {
        return value.to_owned();
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return format!("{trimmed}\\");
    }
    trimmed.to_owned()
}

/// Normalizes a workspace path for comparison and keys: trimmed, trailing separators removed, and
/// Windows drive or UNC paths lowercased with `\` separators.
pub fn normalize_for_comparison(value: &str) -> String {
    let normalized = trim_trailing_separators(value.trim());
    if is_windows_drive_path(&normalized) || is_unc_path(&normalized) {
        return normalized.replace('/', "\\").to_lowercase();
    }
    normalized
}

/// Last path segment for display (`formatWorktreePathForDisplay`): `/a/b/feature/` shows
/// `feature`.
pub fn display_basename(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return path.to_owned();
    }
    let normalized = trimmed.replace('\\', "/");
    let last = normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim();
    if last.is_empty() {
        trimmed.to_owned()
    } else {
        last.to_owned()
    }
}

/// The path of `project` relative to its repository root (`""` at the root), or `None` when the
/// project is outside the root or there is no root.
pub fn repository_relative_path(
    workspace_root: &str,
    repository_root: Option<&str>,
) -> Option<String> {
    let root = repository_root?.trim();
    if root.is_empty() {
        return None;
    }
    let project = normalize_for_comparison(workspace_root);
    let root = normalize_for_comparison(root);
    if project.is_empty() || root.is_empty() {
        return None;
    }
    if project == root {
        return Some(String::new());
    }
    let separator = if root.contains('\\') { '\\' } else { '/' };
    let prefix = format!("{root}{separator}");
    project
        .strip_prefix(&prefix)
        .map(|relative| relative.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    //! Failure modes: root paths losing their separator, Windows paths keyed case-sensitively,
    //! and relative paths matching sibling directories that share a prefix.
    use super::*;

    #[test]
    fn normalizes_like_the_web() {
        assert_eq!(normalize_for_comparison(" /home/a/repo/ "), "/home/a/repo");
        assert_eq!(normalize_for_comparison("/"), "/");
        assert_eq!(
            normalize_for_comparison("C:/Users/A/Repo/"),
            "c:\\users\\a\\repo"
        );
        assert_eq!(normalize_for_comparison("C:\\"), "c:\\");
        assert_eq!(
            normalize_for_comparison("\\\\server\\Share\\x\\"),
            "\\\\server\\share\\x"
        );
    }

    #[test]
    fn relative_paths_need_a_separator_boundary() {
        assert_eq!(
            repository_relative_path("/r/app", Some("/r")),
            Some("app".into())
        );
        assert_eq!(
            repository_relative_path("/r", Some("/r/")),
            Some(String::new())
        );
        assert_eq!(repository_relative_path("/rx/app", Some("/r")), None);
        assert_eq!(repository_relative_path("/r/app", None), None);
        assert_eq!(
            repository_relative_path("C:\\r\\pkg\\a", Some("c:/r")),
            Some("pkg/a".into())
        );
    }

    #[test]
    fn basenames() {
        assert_eq!(display_basename("/a/b/feature/"), "feature");
        assert_eq!(display_basename("C:\\w\\tree"), "tree");
        assert_eq!(display_basename("  "), "  ");
    }
}
