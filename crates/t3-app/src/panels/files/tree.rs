//! The workspace file tree as `FileBrowserPanel` configures `@pierre/trees` 1.0.0-beta.4:
//! directories before files with natural, case-insensitive names (`path-store/src/sort.ts`),
//! empty directory chains flattened into one `a / b` row, top-level directories open at first
//! (`initialExpansion: 1`), and `hide-non-matches` search (`FileTreeController.ts`). Pure data;
//! [`super::browser::FileBrowser`] renders [`FileTree::rows`].

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use t3_protocol::projects::{EntryKind, ProjectEntry};

/// Whether a row is a directory or a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Directory,
    File,
}

/// One visible row of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// The entry the row stands for: the last directory of a flattened chain.
    pub path: String,
    /// The entry name, or the names of a flattened chain joined with ` / `.
    pub name: String,
    pub kind: RowKind,
    /// 0 for top-level rows; one indent guide per level.
    pub depth: usize,
    pub expanded: bool,
    /// Paths of the enclosing rows, outermost first (the indent guides).
    pub ancestors: Vec<String>,
    /// Matches the active search (drawn semibold).
    pub search_match: bool,
}

struct Node {
    name: String,
    path: String,
    directory: bool,
    /// Path segments; top-level entries are 1.
    depth: usize,
    children: Vec<usize>,
}

/// The active search: what matches, what is visible and opened for it, and the expansion to
/// restore when it closes.
struct Search {
    query: String,
    saved: HashMap<String, bool>,
    matches: HashSet<String>,
    /// `None` when nothing matches: every row stays visible (but collapsed).
    visible: Option<HashSet<String>>,
    expanded: HashSet<String>,
}

/// The tree of a `projects.listEntries` result plus its expansion and search state.
#[derive(Default)]
pub struct FileTree {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    by_path: HashMap<String, usize>,
    /// Expansion the user toggled, over the default (top-level directories open).
    overrides: HashMap<String, bool>,
    search: Option<Search>,
    rows: Vec<TreeRow>,
}

impl FileTree {
    /// Replaces the entries (`model.resetPaths`), keeping toggled directories and the search.
    pub fn set_entries(&mut self, entries: &[ProjectEntry]) {
        self.nodes.clear();
        self.roots.clear();
        self.by_path.clear();
        for entry in entries {
            let path = entry.path.trim_matches('/');
            if path.is_empty() {
                continue;
            }
            let directory = entry.kind == EntryKind::Directory;
            let mut parent: Option<usize> = None;
            let segments: Vec<&str> = path.split('/').collect();
            for (index, _) in segments.iter().enumerate() {
                let prefix = segments[..=index].join("/");
                let last = index + 1 == segments.len();
                let node = match self.by_path.get(&prefix) {
                    Some(&node) => node,
                    None => {
                        let node = self.nodes.len();
                        self.nodes.push(Node {
                            name: segments[index].to_owned(),
                            path: prefix.clone(),
                            directory: !last || directory,
                            depth: index + 1,
                            children: Vec::new(),
                        });
                        self.by_path.insert(prefix, node);
                        match parent {
                            Some(parent) => self.nodes[parent].children.push(node),
                            None => self.roots.push(node),
                        }
                        node
                    }
                };
                if !last {
                    self.nodes[node].directory = true;
                }
                parent = Some(node);
            }
        }
        let mut roots = std::mem::take(&mut self.roots);
        self.sort(&mut roots);
        self.roots = roots;
        for node in 0..self.nodes.len() {
            let mut children = std::mem::take(&mut self.nodes[node].children);
            self.sort(&mut children);
            self.nodes[node].children = children;
        }
        if let Some(query) = self.search.as_ref().map(|search| search.query.clone()) {
            let saved = self.search.take().map(|search| search.saved);
            self.search = Some(self.compute_search(query, saved.unwrap_or_default()));
        }
        self.rebuild();
    }

    /// The visible rows, top to bottom.
    pub fn rows(&self) -> &[TreeRow] {
        &self.rows
    }

    pub fn is_directory(&self, path: &str) -> bool {
        self.by_path
            .get(path)
            .is_some_and(|&node| self.nodes[node].directory)
    }

    /// Opens or closes a directory row.
    pub fn toggle(&mut self, path: &str) {
        if !self.is_directory(path) {
            return;
        }
        let expanded = self.is_expanded(path);
        match &mut self.search {
            Some(search) => {
                if expanded {
                    search.expanded.remove(path);
                } else {
                    search.expanded.insert(path.to_owned());
                }
            }
            None => {
                self.overrides.insert(path.to_owned(), !expanded);
            }
        }
        self.rebuild();
    }

    /// The active search text, if the search is open.
    pub fn search_query(&self) -> Option<&str> {
        self.search.as_ref().map(|search| search.query.as_str())
    }

    /// Sets the search (`setSearch`); `None` closes it and restores the expansion from before
    /// it opened, keeping `keep_open`'s ancestors open (`closeSearch`). An empty query only
    /// restores the expansion while staying open.
    pub fn set_search(&mut self, query: Option<&str>, keep_open: Option<&str>) {
        let saved = match self.search.take() {
            Some(search) => search.saved,
            None => self.overrides.clone(),
        };
        let Some(query) = query.map(normalize_query) else {
            self.overrides = saved;
            if let Some(path) = keep_open {
                for ancestor in ancestor_paths(path) {
                    self.overrides.insert(ancestor, true);
                }
            }
            self.rebuild();
            return;
        };
        self.overrides = saved.clone();
        self.search = Some(self.compute_search(query, saved));
        self.rebuild();
    }

    fn compute_search(&self, query: String, saved: HashMap<String, bool>) -> Search {
        let mut search = Search {
            query,
            saved,
            matches: HashSet::new(),
            visible: None,
            expanded: HashSet::new(),
        };
        if search.query.is_empty() {
            return search;
        }
        let mut visible = HashSet::new();
        for node in &self.nodes {
            let candidate = if node.directory {
                format!("{}/", node.path.to_lowercase())
            } else {
                node.path.to_lowercase()
            };
            if !candidate.contains(&search.query) {
                continue;
            }
            search.matches.insert(node.path.clone());
            visible.insert(node.path.clone());
            if node.directory {
                search.expanded.insert(node.path.clone());
            }
            for ancestor in ancestor_paths(&node.path) {
                search.expanded.insert(ancestor.clone());
                visible.insert(ancestor);
            }
        }
        if !search.matches.is_empty() {
            search.visible = Some(visible);
        }
        search
    }

    fn is_expanded(&self, path: &str) -> bool {
        if let Some(search) = &self.search
            && !search.query.is_empty()
        {
            return search.expanded.contains(path);
        }
        self.overrides.get(path).copied().unwrap_or_else(|| {
            self.by_path
                .get(path)
                .is_some_and(|&node| self.nodes[node].depth <= 1)
        })
    }

    fn sort(&self, nodes: &mut [usize]) {
        nodes.sort_by(|&a, &b| {
            let (a, b) = (&self.nodes[a], &self.nodes[b]);
            b.directory
                .cmp(&a.directory)
                .then_with(|| compare_names(&a.name, &b.name))
        });
    }

    fn rebuild(&mut self) {
        let mut rows = Vec::new();
        let roots = self.roots.clone();
        self.push_rows(&roots, 0, &mut Vec::new(), &mut rows);
        if let Some(visible) = self
            .search
            .as_ref()
            .and_then(|search| search.visible.as_ref())
        {
            rows.retain(|row| visible.contains(&row.path));
        }
        self.rows = rows;
    }

    fn push_rows(
        &self,
        nodes: &[usize],
        depth: usize,
        ancestors: &mut Vec<String>,
        rows: &mut Vec<TreeRow>,
    ) {
        for &head in nodes {
            // `flattenEmptyDirectories`: follow single-directory children to the chain's end.
            let mut terminal = head;
            let mut names = vec![self.nodes[head].name.clone()];
            while self.nodes[terminal].directory
                && let [only] = self.nodes[terminal].children[..]
                && self.nodes[only].directory
            {
                terminal = only;
                names.push(self.nodes[only].name.clone());
            }
            let node = &self.nodes[terminal];
            let expanded = node.directory && self.is_expanded(&node.path);
            rows.push(TreeRow {
                path: node.path.clone(),
                name: names.join(" / "),
                kind: if node.directory {
                    RowKind::Directory
                } else {
                    RowKind::File
                },
                depth,
                expanded,
                ancestors: ancestors.clone(),
                search_match: self
                    .search
                    .as_ref()
                    .is_some_and(|search| search.matches.contains(&node.path)),
            });
            if expanded {
                ancestors.push(node.path.clone());
                self.push_rows(&node.children, depth + 1, ancestors, rows);
                ancestors.pop();
            }
        }
    }
}

/// `normalizeSearchQuery`: trimmed, lowercase, backslashes as slashes.
fn normalize_query(query: &str) -> String {
    query.trim().replace('\\', "/").to_lowercase()
}

/// `a/b/c` -> `a`, `a/b`.
fn ancestor_paths(path: &str) -> Vec<String> {
    let segments: Vec<&str> = path.split('/').collect();
    (1..segments.len())
        .map(|end| segments[..end].join("/"))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    Number(u128),
}

impl Token {
    fn as_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Number(number) => number.to_string(),
        }
    }
}

/// `splitIntoNaturalTokens`: runs of digits become numbers.
fn natural_tokens(value: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut number: Option<u128> = None;
    for character in value.chars() {
        match (character.to_digit(10), number) {
            (Some(digit), Some(current)) => {
                number = Some(current.saturating_mul(10).saturating_add(digit.into()))
            }
            (Some(digit), None) => {
                if !text.is_empty() {
                    tokens.push(Token::Text(std::mem::take(&mut text)));
                }
                number = Some(digit.into());
            }
            (None, _) => {
                if let Some(current) = number.take() {
                    tokens.push(Token::Number(current));
                }
                text.push(character);
            }
        }
    }
    match number {
        Some(current) => tokens.push(Token::Number(current)),
        None => tokens.push(Token::Text(text)),
    }
    tokens
}

/// `compareSegmentValues`: case-insensitive natural order, then the raw names.
fn compare_names(a: &str, b: &str) -> Ordering {
    let (lower_a, lower_b) = (a.to_lowercase(), b.to_lowercase());
    let (tokens_a, tokens_b) = (natural_tokens(&lower_a), natural_tokens(&lower_b));
    let plain = |tokens: &[Token]| matches!(tokens, [Token::Text(_)]);
    let order = if plain(&tokens_a) && plain(&tokens_b) {
        lower_a.cmp(&lower_b)
    } else {
        compare_tokens(&tokens_a, &tokens_b).then_with(|| lower_a.cmp(&lower_b))
    };
    order.then_with(|| a.cmp(b))
}

fn compare_tokens(a: &[Token], b: &[Token]) -> Ordering {
    for (left, right) in a.iter().zip(b) {
        if left == right {
            continue;
        }
        let order = match (left, right) {
            (Token::Number(left), Token::Number(right)) => left.cmp(right),
            _ => left.as_text().cmp(&right.as_text()),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

#[cfg(test)]
mod tests {
    //! Failure modes: directories only implied by file paths missing from the tree; files
    //! listed before directories, case-sensitive order, or `file10` before `file9`; a
    //! directory flattened into its only file, or a single-directory chain not flattened;
    //! nested directories open at first or top-level ones closed; toggling a flattened row
    //! toggling its head instead of the directory it shows; search showing non-matches,
    //! hiding or collapsing ancestors of matches, hiding everything when nothing matches, or
    //! losing the pre-search expansion when it closes.
    use super::*;

    fn entry(path: &str, directory: bool) -> ProjectEntry {
        ProjectEntry {
            path: path.into(),
            kind: if directory {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            ignored: None,
        }
    }

    fn tree(paths: &[&str]) -> FileTree {
        let entries: Vec<ProjectEntry> = paths
            .iter()
            .map(|path| match path.strip_suffix('/') {
                Some(path) => entry(path, true),
                None => entry(path, false),
            })
            .collect();
        let mut tree = FileTree::default();
        tree.set_entries(&entries);
        tree
    }

    fn names(tree: &FileTree) -> Vec<String> {
        tree.rows()
            .iter()
            .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
            .collect()
    }

    #[test]
    fn sorts_directories_first_naturally() {
        let tree = tree(&[
            "b.ts",
            "README.md",
            "file10.ts",
            "file9.ts",
            "src/a.ts",
            "Zeta/x.ts",
            "src/lib/b.ts",
        ]);
        assert_eq!(
            names(&tree),
            [
                "src",
                "  lib",
                "  a.ts",
                "Zeta",
                "  x.ts",
                "b.ts",
                "file9.ts",
                "file10.ts",
                "README.md"
            ]
        );
        // `src/lib` is a nested directory: closed at first.
        assert!(!tree.rows()[1].expanded);
        assert_eq!(tree.rows()[2].ancestors, ["src"]);
    }

    #[test]
    fn flattens_single_directory_chains_only() {
        let mut tree = tree(&["apps/web/src/main.ts", "docs/guide.md", "lone/"]);
        assert_eq!(
            names(&tree),
            ["apps / web / src", "docs", "  guide.md", "lone"]
        );
        // The chain's last directory is nested, so it starts closed; toggling opens it.
        assert_eq!(tree.rows()[0].path, "apps/web/src");
        tree.toggle("apps/web/src");
        assert_eq!(names(&tree)[1], "  main.ts");
    }

    #[test]
    fn search_hides_non_matches_and_restores_expansion() {
        let mut tree = tree(&[
            "src/format.ts",
            "src/index.ts",
            "test/format.test.ts",
            "a.md",
        ]);
        tree.toggle("test");
        tree.set_search(Some(" Format "), None);
        assert_eq!(
            names(&tree),
            ["src", "  format.ts", "test", "  format.test.ts"]
        );
        assert!(tree.rows()[1].search_match && !tree.rows()[0].search_match);
        tree.set_search(Some("zzz"), None);
        assert_eq!(names(&tree), ["src", "test", "a.md"]);
        tree.set_search(None, Some("src/index.ts"));
        assert_eq!(
            names(&tree),
            ["src", "  format.ts", "  index.ts", "test", "a.md"]
        );
    }
}
