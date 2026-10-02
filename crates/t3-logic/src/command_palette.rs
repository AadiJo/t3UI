//! Command palette search (`web/components/CommandPalette.logic.ts`, spec 4.4).
//!
//! The palette's items are app-specific (they carry icons and run closures), so the search is
//! generic over anything that exposes [`PaletteSearch::search_terms`]:
//!
//! ```ignore
//! let groups = filter_groups(&root_groups, &query, false, &project_items, &thread_items);
//! ```

/// Number of rows in "Recent Threads" (`RECENT_THREAD_LIMIT`).
pub const RECENT_THREAD_LIMIT: usize = 12;

/// Group ids the search treats specially.
pub mod group {
    pub const ACTIONS: &str = "actions";
    pub const RECENT_THREADS: &str = "recent-threads";
    pub const PROJECTS_SEARCH: &str = "projects-search";
    pub const THREADS_SEARCH: &str = "threads-search";
}

/// What the search reads from an item.
pub trait PaletteSearch {
    /// Ranked fields, best first (e.g. title, then path). Empty strings are skipped.
    fn search_terms(&self) -> &[String];
}

/// A labeled list of items.
#[derive(Clone, Debug)]
pub struct PaletteGroup<T> {
    /// Stable id ("actions", "recent-threads", ...).
    pub value: &'static str,
    pub label: &'static str,
    pub items: Vec<T>,
}

/// Which view the palette shows, for the input placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Root,
    RootBrowse,
    Submenu,
    SubmenuBrowse,
}

impl PaletteMode {
    pub fn new(in_submenu: bool, browsing: bool) -> Self {
        match (in_submenu, browsing) {
            (false, false) => Self::Root,
            (false, true) => Self::RootBrowse,
            (true, false) => Self::Submenu,
            (true, true) => Self::SubmenuBrowse,
        }
    }

    /// The input placeholder (`getCommandPaletteInputPlaceholder`).
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Root => "Search commands, projects, and threads...",
            Self::RootBrowse => "Enter project path (e.g. ~/projects/my-app)",
            Self::Submenu => "Search...",
            Self::SubmenuBrowse => "Enter path (e.g. ~/projects/my-app)",
        }
    }
}

/// Trim, lowercase, collapse whitespace (`normalizeSearchText`).
pub fn normalize_search_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// True when the query starts with `>`: only "Actions" are searched.
pub fn is_actions_filter(query: &str) -> bool {
    query.starts_with('>')
}

/// The empty-results copy for `query`.
pub fn empty_text(query: &str) -> &'static str {
    if is_actions_filter(query) {
        "No matching actions."
    } else {
        "No matching commands, projects, or threads."
    }
}

/// Filters and ranks the groups for `query` (`filterCommandPaletteGroups`):
///
/// - `>` limits results to "Actions" (an empty remainder lists every action).
/// - An empty query returns `active` unchanged.
/// - At the root, a query drops "Recent Threads" and appends "Projects" and "Threads" built from
///   `project_items` / `thread_items`.
/// - An item matches when the query is a substring of its terms joined by spaces. Within a group,
///   items sort by rank (the first term containing the query scores `1000 - index*100` plus 3
///   exact / 2 prefix / 1 contains), then by original order. Empty groups are dropped.
pub fn filter_groups<T: PaletteSearch + Clone>(
    active: &[PaletteGroup<T>],
    query: &str,
    in_submenu: bool,
    project_items: &[T],
    thread_items: &[T],
) -> Vec<PaletteGroup<T>> {
    let actions_only = is_actions_filter(query);
    let search = if actions_only { &query[1..] } else { query };
    let normalized = normalize_search_text(search);
    let keep = |group: &&PaletteGroup<T>| {
        if actions_only {
            group.value == group::ACTIONS
        } else {
            in_submenu || normalized.is_empty() || group.value != group::RECENT_THREADS
        }
    };
    let mut groups: Vec<PaletteGroup<T>> = active.iter().filter(keep).cloned().collect();
    if normalized.is_empty() {
        return groups;
    }
    if !in_submenu && !actions_only {
        for (value, label, items) in [
            (group::PROJECTS_SEARCH, "Projects", project_items),
            (group::THREADS_SEARCH, "Threads", thread_items),
        ] {
            if !items.is_empty() {
                groups.push(PaletteGroup {
                    value,
                    label,
                    items: items.to_vec(),
                });
            }
        }
    }
    groups
        .into_iter()
        .filter_map(|group| {
            let mut ranked: Vec<(i64, usize, T)> = group
                .items
                .into_iter()
                .enumerate()
                .filter(|(_, item)| {
                    normalize_search_text(&item.search_terms().join(" ")).contains(&normalized)
                })
                .map(|(index, item)| (rank_item(&item, &normalized), index, item))
                .collect();
            ranked.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
            (!ranked.is_empty()).then(|| PaletteGroup {
                value: group.value,
                label: group.label,
                items: ranked.into_iter().map(|(_, _, item)| item).collect(),
            })
        })
        .collect()
}

fn rank_item<T: PaletteSearch>(item: &T, normalized_query: &str) -> i64 {
    item.search_terms()
        .iter()
        .filter(|term| !term.is_empty())
        .enumerate()
        .find_map(|(index, term)| {
            rank_field(term, normalized_query).map(|rank| 1000 - index as i64 * 100 + rank)
        })
        .unwrap_or(0)
}

fn rank_field(field: &str, normalized_query: &str) -> Option<i64> {
    let field = normalize_search_text(field);
    if field.is_empty() || !field.contains(normalized_query) {
        return None;
    }
    Some(if field == normalized_query {
        3
    } else if field.starts_with(normalized_query) {
        2
    } else {
        1
    })
}

/// Add-project browsing over Unix-style paths (`packages/client-runtime/src/state/projects.ts`).
/// The native client targets macOS and Linux servers, so `\\` separators and drive letters are
/// not handled.
pub mod browse {
    /// A query that switches the palette into browse mode (`isFilesystemBrowseQuery`).
    pub fn is_browse_query(value: &str) -> bool {
        ["./", "../", "/", "~/"]
            .iter()
            .any(|prefix| value.starts_with(prefix))
    }

    pub fn has_trailing_separator(value: &str) -> bool {
        value.ends_with('/')
    }

    /// The directory to list: the query itself when it ends in `/`, else up to the last `/`.
    pub fn directory_path(value: &str) -> &str {
        if has_trailing_separator(value) {
            return value;
        }
        value.rfind('/').map_or(value, |index| &value[..=index])
    }

    /// The partial name after the last `/`, used to filter entries.
    pub fn leaf(value: &str) -> &str {
        value.rfind('/').map_or(value, |index| &value[index + 1..])
    }

    /// The parent directory with a trailing `/` (`getBrowseParentPath`); `None` at the root or
    /// for a bare name.
    pub fn parent_path(value: &str) -> Option<String> {
        let trimmed = value.trim_end_matches('/');
        if trimmed.is_empty() {
            return None;
        }
        let index = trimmed.rfind('/')?;
        Some(trimmed[..=index].to_owned())
    }

    /// `query` as a directory: appends `/` unless empty or already there.
    pub fn ensure_directory(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() || has_trailing_separator(trimmed) {
            trimmed.to_owned()
        } else {
            format!("{trimmed}/")
        }
    }

    /// Entries whose name starts with the leaf (case-insensitive); dot entries only when the
    /// leaf starts with `.` (`filterBrowseEntries`).
    pub fn matches(name: &str, leaf: &str) -> bool {
        name.to_lowercase().starts_with(&leaf.to_lowercase())
            && (leaf.starts_with('.') || !name.starts_with('.'))
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes:
    //! 1. `>` does not restrict to actions, or `>` alone hides them.
    //! 2. Recent Threads still shows while searching at the root, or Projects/Threads are not
    //!    appended (or are appended inside a submenu).
    //! 3. Ranking prefers a later term over an earlier one, or exact < prefix < contains breaks.
    //! 4. Ties do not keep the original order.
    //! 5. Whitespace and case in the query or terms change the match.
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Item(&'static str, Vec<String>);

    impl PaletteSearch for Item {
        fn search_terms(&self) -> &[String] {
            &self.1
        }
    }

    fn item(id: &'static str, terms: &[&str]) -> Item {
        Item(id, terms.iter().map(|term| term.to_string()).collect())
    }

    fn ids(groups: &[PaletteGroup<Item>]) -> Vec<(&'static str, Vec<&'static str>)> {
        groups
            .iter()
            .map(|group| (group.value, group.items.iter().map(|item| item.0).collect()))
            .collect()
    }

    fn root() -> Vec<PaletteGroup<Item>> {
        vec![
            PaletteGroup {
                value: group::ACTIONS,
                label: "Actions",
                items: vec![
                    item("new-thread", &["new thread", "chat", "aurora-web"]),
                    item("add-project", &["add project", "folder", "clone"]),
                    item("settings", &["open settings", "preferences"]),
                ],
            },
            PaletteGroup {
                value: group::RECENT_THREADS,
                label: "Recent Threads",
                items: vec![item("t-recent", &["Repo tour", "aurora-web", "main"])],
            },
        ]
    }

    #[test]
    fn empty_query_keeps_groups() {
        let groups = filter_groups(&root(), "  ", false, &[], &[]);
        assert_eq!(ids(&groups).len(), 2);
    }

    #[test]
    fn actions_filter() {
        let groups = filter_groups(&root(), ">", false, &[], &[]);
        assert_eq!(
            ids(&groups),
            vec![("actions", vec!["new-thread", "add-project", "settings"])]
        );
        let groups = filter_groups(&root(), ">set", false, &[item("p", &["settings"])], &[]);
        assert_eq!(ids(&groups), vec![("actions", vec!["settings"])]);
        assert_eq!(empty_text(">zzz"), "No matching actions.");
    }

    #[test]
    fn root_search_swaps_recent_for_projects_and_threads() {
        let projects = [item("p-aurora", &["aurora-web", "/repos/aurora-web"])];
        let threads = [item("t-tour", &["Repo tour", "aurora-web", "main"])];
        let groups = filter_groups(&root(), "aurora", false, &projects, &threads);
        assert_eq!(
            ids(&groups),
            vec![
                ("actions", vec!["new-thread"]),
                ("projects-search", vec!["p-aurora"]),
                ("threads-search", vec!["t-tour"]),
            ]
        );
        // Inside a submenu the active groups are searched as they are.
        let groups = filter_groups(&root(), "tour", true, &projects, &threads);
        assert_eq!(ids(&groups), vec![("recent-threads", vec!["t-recent"])]);
    }

    #[test]
    fn ranks_by_term_then_match_kind_then_order() {
        let group = PaletteGroup {
            value: group::ACTIONS,
            label: "Actions",
            items: vec![
                item("in-path", &["zeta", "/x/alpha"]),
                item("contains", &["the alpha one"]),
                item("prefix", &["alphabet"]),
                item("exact", &["Alpha"]),
                item("contains-2", &["an alpha"]),
            ],
        };
        let groups = filter_groups(&[group], "  ALPHA ", true, &[], &[]);
        assert_eq!(
            ids(&groups),
            vec![(
                "actions",
                vec!["exact", "prefix", "contains", "contains-2", "in-path"]
            )]
        );
    }

    #[test]
    fn matches_across_joined_terms_and_whitespace() {
        let group = PaletteGroup {
            value: group::ACTIONS,
            label: "Actions",
            items: vec![item("split", &["new", "thread"])],
        };
        let groups = filter_groups(&[group], "new   thread", true, &[], &[]);
        assert_eq!(ids(&groups), vec![("actions", vec!["split"])]);
        assert_eq!(normalize_search_text("  A\tB  c "), "a b c");
    }

    #[test]
    fn browse_paths() {
        use super::browse::*;
        assert!(is_browse_query("~/") && is_browse_query("/tmp") && !is_browse_query("tmp"));
        assert_eq!(directory_path("~/code/aur"), "~/code/");
        assert_eq!(directory_path("~/code/"), "~/code/");
        assert_eq!(leaf("~/code/aur"), "aur");
        assert_eq!(parent_path("~/code/aurora/"), Some("~/code/".into()));
        assert_eq!(parent_path("/tmp/"), Some("/".into()));
        assert_eq!(parent_path("/"), None);
        assert_eq!(ensure_directory(" ~/code "), "~/code/");
        assert!(matches("Aurora", "au") && !matches(".git", "") && matches(".git", ".g"));
    }

    #[test]
    fn placeholders() {
        assert_eq!(
            PaletteMode::new(false, false).placeholder(),
            "Search commands, projects, and threads..."
        );
        assert_eq!(PaletteMode::new(true, false).placeholder(), "Search...");
    }
}
