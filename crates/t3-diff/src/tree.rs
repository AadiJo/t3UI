//! The changed-files tree model, ported from the web client's `lib/turnDiffTree.ts` and
//! `DiffStatLabel.tsx`: directories first, natural name order, single-child directory chains
//! compacted into `a/b/c`, and directory stats summed from their files.

use std::collections::BTreeMap;

use crate::patch::compare_natural;

/// Added and deleted line counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffStat {
    pub additions: u64,
    pub deletions: u64,
}

impl DiffStat {
    pub fn is_zero(self) -> bool {
        self.additions == 0 && self.deletions == 0
    }

    fn add(&mut self, other: Self) {
        self.additions += other.additions;
        self.deletions += other.deletions;
    }
}

/// A changed file as the server reports it (checkpoint `files[]`). Stats may be missing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub stat: Option<DiffStat>,
}

/// A node of the changed-files tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeNode {
    Directory {
        /// Display name, `a/b/c` when single-child directories were compacted.
        name: String,
        /// Full path of the deepest compacted directory; the key for expansion state.
        path: String,
        stat: DiffStat,
        children: Vec<TreeNode>,
    },
    File {
        name: String,
        path: String,
        stat: Option<DiffStat>,
    },
}

impl TreeNode {
    pub fn path(&self) -> &str {
        match self {
            Self::Directory { path, .. } | Self::File { path, .. } => path,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Directory { name, .. } | Self::File { name, .. } => name,
        }
    }
}

/// Directories with at most this many children start expanded
/// (`CHANGED_FILES_AUTO_EXPAND_MAX_ITEMS`).
pub const AUTO_EXPAND_MAX_CHILDREN: usize = 3;

/// Whether a directory starts expanded when no expand/collapse-all override is set.
pub fn expanded_by_default(children: &[TreeNode]) -> bool {
    children.len() <= AUTO_EXPAND_MAX_CHILDREN
}

/// Whether every directory starts expanded (`changedFilesExpandAllByDefault`); drives the
/// card's "Collapse all" / "Expand all" label.
pub fn all_expanded_by_default(nodes: &[TreeNode]) -> bool {
    nodes.iter().all(|node| match node {
        TreeNode::Directory { children, .. } => {
            expanded_by_default(children) && all_expanded_by_default(children)
        }
        TreeNode::File { .. } => true,
    })
}

/// Total of every file's stat (`summarizeTurnDiffStats`); files without stats count as zero.
pub fn summarize(files: &[ChangedFile]) -> DiffStat {
    let mut total = DiffStat::default();
    for stat in files.iter().filter_map(|file| file.stat) {
        total.add(stat);
    }
    total
}

#[derive(Default)]
struct DirectoryBuilder {
    path: String,
    stat: DiffStat,
    directories: BTreeMap<String, DirectoryBuilder>,
    files: Vec<TreeNode>,
}

/// Builds the tree for `files` (`buildTurnDiffTree`).
pub fn build_tree(files: &[ChangedFile]) -> Vec<TreeNode> {
    let mut root = DirectoryBuilder::default();
    for file in files {
        let normalized = file.path.replace('\\', "/");
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect();
        let Some((file_name, directories)) = segments.split_last() else {
            continue;
        };
        let mut directory = &mut root;
        if let Some(stat) = file.stat {
            directory.stat.add(stat);
        }
        for segment in directories {
            let path = if directory.path.is_empty() {
                (*segment).to_owned()
            } else {
                format!("{}/{segment}", directory.path)
            };
            directory = directory
                .directories
                .entry((*segment).to_owned())
                .or_insert_with(|| DirectoryBuilder {
                    path,
                    ..DirectoryBuilder::default()
                });
            if let Some(stat) = file.stat {
                directory.stat.add(stat);
            }
        }
        directory.files.push(TreeNode::File {
            name: (*file_name).to_owned(),
            path: segments.join("/"),
            stat: file.stat,
        });
    }
    into_nodes(root)
}

fn into_nodes(directory: DirectoryBuilder) -> Vec<TreeNode> {
    let mut directories: Vec<TreeNode> = directory
        .directories
        .into_iter()
        .map(|(name, builder)| {
            compact(TreeNode::Directory {
                name,
                path: builder.path.clone(),
                stat: builder.stat,
                children: into_nodes(builder),
            })
        })
        .collect();
    directories.sort_by(|a, b| compare_natural(a.name(), b.name()));
    let mut files = directory.files;
    files.sort_by(|a, b| compare_natural(a.name(), b.name()));
    directories.extend(files);
    directories
}

/// Merges a directory whose only child is a directory into `parent/child`.
fn compact(node: TreeNode) -> TreeNode {
    let TreeNode::Directory {
        mut name,
        mut path,
        mut stat,
        mut children,
    } = node
    else {
        return node;
    };
    while let [TreeNode::Directory { .. }] = children.as_slice() {
        let Some(TreeNode::Directory {
            name: child_name,
            path: child_path,
            stat: child_stat,
            children: grandchildren,
        }) = children.pop()
        else {
            break;
        };
        name = format!("{name}/{child_name}");
        path = child_path;
        stat = child_stat;
        children = grandchildren;
    }
    TreeNode::Directory {
        name,
        path,
        stat,
        children,
    }
}

/// Formats a count compactly like `formatCompactDiffCount`: `999`, `1.2k`, `12k`, `1.2m`, `3b`.
/// One decimal below 10 of a unit (dropping `.0`), rounded halves up as JavaScript does.
pub fn format_count(value: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(1_000_000_000, "b"), (1_000_000, "m"), (1_000, "k")];
    if value < 1_000 {
        return value.to_string();
    }
    let (unit, suffix) = if value < 1_000_000 {
        UNITS[2]
    } else if value < 1_000_000_000 {
        UNITS[1]
    } else {
        UNITS[0]
    };
    if value < unit * 10 {
        let tenths = (value + unit / 20) / (unit / 10);
        if tenths.is_multiple_of(10) {
            format!("{}{suffix}", tenths / 10)
        } else {
            format!("{}.{}{suffix}", tenths / 10, tenths % 10)
        }
    } else {
        format!("{}{suffix}", (value + unit / 2) / unit)
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: directories not sorted before files, plain string order instead of
    //! natural order, compaction stopping early or merging a directory that also holds files,
    //! stats double counted or missing for compacted chains, backslash or doubled separators
    //! creating phantom directories, files without stats breaking sums, auto-expansion off by
    //! one, and compact counts rounding like Rust (`1.25` -> `1.2`) instead of JS (`1.3`).
    use super::*;

    fn file(path: &str, additions: u64, deletions: u64) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            stat: Some(DiffStat {
                additions,
                deletions,
            }),
        }
    }

    fn names(nodes: &[TreeNode]) -> Vec<&str> {
        nodes.iter().map(TreeNode::name).collect()
    }

    fn fixture() -> Vec<ChangedFile> {
        vec![
            file("apps/web/src/components/ChatView.tsx", 9, 4),
            file("apps/web/src/components/DiffPanel.tsx", 120, 38),
            file("apps/web/src/components/chat/ChangedFilesTree.tsx", 42, 7),
            file("apps/web/src/lib/diffStats.ts", 14, 0),
            file("apps/server/src/legacy/checkpointPaths.ts", 0, 7),
            file("apps/server/src/orchestration/turnDiff.ts", 23, 11),
            file("README.md", 1, 1),
            file("package.json", 2, 1),
            file("crates/t3-diff/src/lib.rs", 1250, 0),
        ]
    }

    #[test]
    fn directories_first_in_natural_order_with_compaction() {
        let tree = build_tree(&fixture());
        assert_eq!(
            names(&tree),
            ["apps", "crates/t3-diff/src", "package.json", "README.md"]
        );
        let TreeNode::Directory { children, .. } = &tree[0] else {
            panic!()
        };
        assert_eq!(names(children), ["server/src", "web/src"]);
        let TreeNode::Directory {
            children: web,
            path,
            ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(path, "apps/web/src");
        assert_eq!(names(web), ["components", "lib"]);
        let TreeNode::Directory {
            children: components,
            ..
        } = &web[0]
        else {
            panic!()
        };
        assert_eq!(names(components), ["chat", "ChatView.tsx", "DiffPanel.tsx"]);
    }

    #[test]
    fn stats_sum_through_compacted_chains() {
        let tree = build_tree(&fixture());
        let TreeNode::Directory { stat, children, .. } = &tree[0] else {
            panic!()
        };
        assert_eq!(
            *stat,
            DiffStat {
                additions: 208,
                deletions: 67
            }
        );
        let TreeNode::Directory { stat, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(
            *stat,
            DiffStat {
                additions: 23,
                deletions: 18
            }
        );
        assert_eq!(
            summarize(&fixture()),
            DiffStat {
                additions: 1461,
                deletions: 69
            }
        );
    }

    #[test]
    fn directory_with_files_is_not_compacted() {
        let tree = build_tree(&[file("a/b/x.ts", 1, 0), file("a/y.ts", 1, 0)]);
        assert_eq!(names(&tree), ["a"]);
        let TreeNode::Directory { children, .. } = &tree[0] else {
            panic!()
        };
        assert_eq!(names(children), ["b", "y.ts"]);
    }

    #[test]
    fn odd_separators_and_missing_stats() {
        let files = [
            ChangedFile {
                path: "src\\win\\path.rs".into(),
                stat: None,
            },
            file("src//double///slash.rs", 2, 2),
            file("", 5, 5),
        ];
        let tree = build_tree(&files);
        assert_eq!(names(&tree), ["src"]);
        let TreeNode::Directory { children, stat, .. } = &tree[0] else {
            panic!()
        };
        assert_eq!(names(children), ["double", "win"]);
        assert_eq!(
            *stat,
            DiffStat {
                additions: 2,
                deletions: 2
            }
        );
        assert_eq!(
            summarize(&files),
            DiffStat {
                additions: 7,
                deletions: 7
            }
        );
        let TreeNode::Directory { children: win, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(
            win[0],
            TreeNode::File {
                name: "path.rs".into(),
                path: "src/win/path.rs".into(),
                stat: None
            }
        );
    }

    #[test]
    fn auto_expansion_threshold() {
        let tree = build_tree(&fixture());
        let TreeNode::Directory { children, .. } = &tree[0] else {
            panic!()
        };
        assert!(expanded_by_default(children));
        assert!(all_expanded_by_default(&tree));
        let four = build_tree(&[
            file("d/a.ts", 1, 0),
            file("d/b.ts", 1, 0),
            file("d/c.ts", 1, 0),
            file("d/e.ts", 1, 0),
        ]);
        assert!(!all_expanded_by_default(&four));
    }

    #[test]
    fn compact_counts_round_like_javascript() {
        let cases = [
            (0, "0"),
            (999, "999"),
            (1000, "1k"),
            (1249, "1.2k"),
            (1250, "1.3k"),
            (9960, "10k"),
            (12_499, "12k"),
            (12_500, "13k"),
            (999_600, "1000k"),
            (1_200_000, "1.2m"),
            (3_000_000_000, "3b"),
        ];
        for (value, expected) in cases {
            assert_eq!(format_count(value), expected, "{value}");
        }
    }
}
