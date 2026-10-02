//! Git actions in the chat header (web `GitActionsControl.logic.ts`, the disabled-reason and
//! progress helpers in `GitActionsControl.tsx`, `shared/sourceControl.ts`; fork fe7d3092c) and the pull
//! request reference parser of the PR checkout dialog (`pullRequestReference.ts`).
//!
//! Everything reads a [`GitStatus`]: the merged `subscribeVcsStatus` halves, where a missing
//! remote half counts as no upstream, nothing ahead or behind, and no PR (`mergeGitStatusParts`).

use std::sync::LazyLock;

use regex::Regex;
use t3_protocol::{
    orchestration::PullRequestState,
    vcs::{
        GitStackedAction, SourceControlProviderInfo, VcsPullRequestSummary, VcsStatusLocal,
        VcsStatusRemote,
    },
};

/// The merged VCS status of one working copy.
#[derive(Clone, Copy, Debug)]
pub struct GitStatus<'a> {
    pub local: &'a VcsStatusLocal,
    pub remote: Option<&'a VcsStatusRemote>,
}

impl<'a> GitStatus<'a> {
    pub fn new(local: &'a VcsStatusLocal, remote: Option<&'a VcsStatusRemote>) -> Self {
        Self { local, remote }
    }

    pub fn ref_name(&self) -> Option<&'a str> {
        self.local.ref_name.as_deref()
    }

    pub fn has_changes(&self) -> bool {
        self.local.has_working_tree_changes
    }

    pub fn has_upstream(&self) -> bool {
        self.remote.is_some_and(|remote| remote.has_upstream)
    }

    pub fn ahead(&self) -> u64 {
        self.remote.map_or(0, |remote| remote.ahead_count)
    }

    pub fn behind(&self) -> u64 {
        self.remote.map_or(0, |remote| remote.behind_count)
    }

    /// Commits ahead of the default ref, falling back to `ahead` when the server omits it.
    pub fn ahead_of_default(&self) -> u64 {
        self.remote.map_or(0, |remote| {
            remote.ahead_of_default_count.unwrap_or(remote.ahead_count)
        })
    }

    /// The branch's pull request when it is open.
    pub fn open_pr(&self) -> Option<&'a VcsPullRequestSummary> {
        self.remote
            .and_then(|remote| remote.pr.as_ref())
            .filter(|pr| pr.state == PullRequestState::Open)
    }

    pub fn is_default_ref(&self) -> bool {
        self.local.is_default_ref
    }

    pub fn has_primary_remote(&self) -> bool {
        self.local.has_primary_remote
    }

    pub fn presentation(&self) -> ChangeRequestPresentation {
        ChangeRequestPresentation::for_provider(self.local.source_control_provider.as_ref())
    }
}

/// Which brand a change request belongs to; picks icons and words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    GitHub,
    GitLab,
    Forgejo,
    AzureDevOps,
    Bitbucket,
    /// `unknown` or a kind this client does not know: lucide `GitPullRequest`.
    Generic,
}

/// Words and icon for a provider's change requests (`ChangeRequestPresentation` plus the
/// `SourceControlPresentation` provider name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeRequestPresentation {
    pub kind: ProviderKind,
    /// "GitHub", or the server's provider name.
    pub provider_name: String,
    /// `PR` / `MR` / `change request`.
    pub short_label: &'static str,
    /// `pull request` / `merge request` / `change request`.
    pub singular: &'static str,
}

impl ChangeRequestPresentation {
    /// No provider reads as GitHub (`resolveChangeRequestPresentation(undefined)`).
    pub fn for_provider(provider: Option<&SourceControlProviderInfo>) -> Self {
        let (kind, name, short_label, singular) = match provider.map(|p| p.kind.as_str()) {
            None | Some("github") => (ProviderKind::GitHub, "GitHub", "PR", "pull request"),
            Some("gitlab") => (ProviderKind::GitLab, "GitLab", "MR", "merge request"),
            Some("forgejo") => (ProviderKind::Forgejo, "Forgejo", "PR", "pull request"),
            Some("azure-devops") => (
                ProviderKind::AzureDevOps,
                "Azure DevOps",
                "PR",
                "pull request",
            ),
            Some("bitbucket") => (ProviderKind::Bitbucket, "Bitbucket", "PR", "pull request"),
            Some(_) => (
                ProviderKind::Generic,
                "source control",
                "change request",
                "change request",
            ),
        };
        let provider_name = provider
            .map(|p| p.name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or(name)
            .to_owned();
        Self {
            kind,
            provider_name,
            short_label,
            singular,
        }
    }
}

/// What the quick action button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickActionKind {
    RunAction(GitStackedAction),
    /// `vcs.pull`.
    RunPull,
    OpenPr,
    OpenPublish,
    /// Disabled; hovering shows the hint.
    ShowHint(String),
}

/// The left half of the Git actions group (`GitQuickAction`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickAction {
    pub label: String,
    pub disabled: bool,
    pub kind: QuickActionKind,
}

impl QuickAction {
    fn run(label: impl Into<String>, action: GitStackedAction) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            kind: QuickActionKind::RunAction(action),
        }
    }

    fn hint(label: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            disabled: true,
            kind: QuickActionKind::ShowHint(hint.into()),
        }
    }

    fn open(label: impl Into<String>, kind: QuickActionKind) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            kind,
        }
    }

    /// Hover text while disabled.
    pub fn disabled_reason(&self) -> Option<&str> {
        match &self.kind {
            QuickActionKind::ShowHint(hint) if self.disabled => Some(hint),
            _ if self.disabled => Some("This action is currently unavailable."),
            _ => None,
        }
    }
}

/// `resolveQuickAction`: the first matching row of the decision table in panels.md 4.5.
pub fn resolve_quick_action(status: Option<GitStatus<'_>>, busy: bool) -> QuickAction {
    if busy {
        return QuickAction::hint("Commit", "Git action in progress.");
    }
    let Some(status) = status else {
        return QuickAction::hint("Commit", "Git status is unavailable.");
    };
    let words = status.presentation();
    let short = words.short_label;
    let has_open_pr = status.open_pr().is_some();
    let is_ahead = status.ahead() > 0;
    let is_behind = status.behind() > 0;
    let is_default_ref = status.is_default_ref();
    let has_primary_remote = status.has_primary_remote();

    if status.ref_name().is_none() {
        return QuickAction::hint(
            "Commit",
            format!(
                "Create and checkout a ref before pushing or opening a {}.",
                words.singular
            ),
        );
    }
    if status.has_changes() {
        if !status.has_upstream() && !has_primary_remote {
            return QuickAction::run("Commit", GitStackedAction::Commit);
        }
        if has_open_pr || is_default_ref {
            return QuickAction::run("Commit & push", GitStackedAction::CommitPush);
        }
        return QuickAction::run(
            format!("Commit, push & {short}"),
            GitStackedAction::CommitPushPr,
        );
    }
    let push = || {
        QuickAction::run(
            "Push",
            if is_default_ref {
                GitStackedAction::CommitPush
            } else {
                GitStackedAction::Push
            },
        )
    };
    if !status.has_upstream() {
        if !has_primary_remote {
            if has_open_pr && !is_ahead {
                return QuickAction::open(format!("View {short}"), QuickActionKind::OpenPr);
            }
            return QuickAction::open("Publish repository", QuickActionKind::OpenPublish);
        }
        if !is_ahead {
            if has_open_pr {
                return QuickAction::open(format!("View {short}"), QuickActionKind::OpenPr);
            }
            return QuickAction::hint("Push", "No local commits to push.");
        }
        if has_open_pr || is_default_ref {
            return push();
        }
        return QuickAction::run(format!("Push & create {short}"), GitStackedAction::CreatePr);
    }
    if is_ahead && is_behind {
        return QuickAction::hint(
            "Sync ref",
            "Branch has diverged from upstream. Rebase/merge first.",
        );
    }
    if is_behind {
        return QuickAction::open("Pull", QuickActionKind::RunPull);
    }
    if is_ahead {
        if has_open_pr || is_default_ref {
            return push();
        }
        return QuickAction::run(format!("Push & create {short}"), GitStackedAction::CreatePr);
    }
    if has_open_pr {
        return QuickAction::open(format!("View {short}"), QuickActionKind::OpenPr);
    }
    if status.ahead_of_default() > 0 && !is_default_ref {
        return QuickAction::run(format!("Create {short}"), GitStackedAction::CreatePr);
    }
    QuickAction::hint("Commit", "Branch is up to date. No action needed.")
}

/// Which menu row this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItemId {
    Commit,
    Push,
    Pr,
}

/// What choosing a menu row does: the commit dialog, a direct push / create-PR run, or opening
/// the existing PR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItemKind {
    OpenCommitDialog,
    Run(GitStackedAction),
    OpenPr,
}

/// A row of the Git actions menu (`GitActionMenuItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: MenuItemId,
    pub label: String,
    pub disabled: bool,
    pub kind: MenuItemKind,
    /// Hover text when disabled (`getMenuActionDisabledReason`).
    pub disabled_reason: Option<String>,
}

/// `buildMenuItems`: Commit, then Push and View/Create PR when there is a primary remote.
pub fn menu_items(status: Option<GitStatus<'_>>, busy: bool) -> Vec<MenuItem> {
    let Some(status) = status else {
        return Vec::new();
    };
    let words = status.presentation();
    let has_branch = status.ref_name().is_some();
    let has_changes = status.has_changes();
    let has_open_pr = status.open_pr().is_some();
    let is_behind = status.behind() > 0;
    let has_primary_remote = status.has_primary_remote();
    let can_push_without_upstream = has_primary_remote && !status.has_upstream();
    let can_reach_remote = status.has_upstream() || can_push_without_upstream;
    let can_commit = !busy && has_changes;
    let can_push = !busy && has_branch && !is_behind && status.ahead() > 0 && can_reach_remote;
    let can_create_pr = !busy
        && has_branch
        && !has_changes
        && !has_open_pr
        && status.ahead_of_default() > 0
        && !is_behind
        && can_reach_remote;

    let mut items = vec![MenuItem {
        id: MenuItemId::Commit,
        label: "Commit".into(),
        disabled: !can_commit,
        kind: MenuItemKind::OpenCommitDialog,
        disabled_reason: None,
    }];
    if has_primary_remote {
        items.push(MenuItem {
            id: MenuItemId::Push,
            label: "Push".into(),
            disabled: !can_push,
            kind: MenuItemKind::Run(GitStackedAction::Push),
            disabled_reason: None,
        });
        items.push(if has_open_pr {
            MenuItem {
                id: MenuItemId::Pr,
                label: format!("View {}", words.short_label),
                disabled: busy,
                kind: MenuItemKind::OpenPr,
                disabled_reason: None,
            }
        } else {
            MenuItem {
                id: MenuItemId::Pr,
                label: format!("Create {}", words.short_label),
                disabled: !can_create_pr,
                kind: MenuItemKind::Run(GitStackedAction::CreatePr),
                disabled_reason: None,
            }
        });
    }
    for item in &mut items {
        if item.disabled {
            item.disabled_reason = Some(disabled_reason(item.id, status, busy));
        }
    }
    items
}

fn disabled_reason(id: MenuItemId, status: GitStatus<'_>, busy: bool) -> String {
    if busy {
        return "Git action in progress.".into();
    }
    let singular = status.presentation().singular;
    let has_branch = status.ref_name().is_some();
    let has_changes = status.has_changes();
    let is_ahead = status.ahead() > 0;
    let is_behind = status.behind() > 0;
    let no_remote = !status.has_upstream() && !status.has_primary_remote();
    let reason = match id {
        MenuItemId::Commit if !has_changes => "Worktree is clean. Make changes before committing.",
        MenuItemId::Commit => "Commit is currently unavailable.",
        MenuItemId::Push if !has_branch => "Detached HEAD: check out a branch before pushing.",
        MenuItemId::Push if has_changes => "Commit or stash local changes before pushing.",
        MenuItemId::Push if is_behind => "Branch is behind upstream. Pull/rebase before pushing.",
        MenuItemId::Push if no_remote => "Add an \"origin\" remote before pushing.",
        MenuItemId::Push if !is_ahead => "No local commits to push.",
        MenuItemId::Push => "Push is currently unavailable.",
        MenuItemId::Pr => {
            return if status.open_pr().is_some() {
                format!("View {singular} is currently unavailable.")
            } else if !has_branch {
                format!("Detached HEAD: check out a branch before creating a {singular}.")
            } else if has_changes {
                format!("Commit local changes before creating a {singular}.")
            } else if no_remote {
                format!("Add an \"origin\" remote before creating a {singular}.")
            } else if !is_ahead {
                format!("No local commits to include in a {singular}.")
            } else if is_behind {
                format!("Branch is behind upstream. Pull/rebase before creating a {singular}.")
            } else {
                format!("Create {singular} is currently unavailable.")
            };
        }
    };
    reason.into()
}

/// Warnings under the menu rows.
pub fn menu_warnings(status: Option<GitStatus<'_>>) -> Vec<&'static str> {
    let Some(status) = status else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if status.ref_name().is_none() {
        warnings.push(
            "Detached HEAD: create and check out a branch to enable push and pull request actions.",
        );
    } else if !status.has_changes() && status.behind() > 0 && status.ahead() == 0 {
        warnings.push("Behind upstream. Pull/rebase first.");
    }
    warnings
}

/// Inputs to [`progress_stages`].
#[derive(Clone, Copy, Debug)]
pub struct ProgressInput<'a> {
    pub action: &'a GitStackedAction,
    pub has_custom_commit_message: bool,
    pub has_working_tree_changes: bool,
    pub feature_branch: bool,
    pub should_push_before_pr: bool,
}

/// `buildGitActionProgressStages`: the expected phase labels; the first one titles the progress
/// toast before the server reports anything.
pub fn progress_stages(input: ProgressInput<'_>, words: &ChangeRequestPresentation) -> Vec<String> {
    let push = "Pushing...".to_owned();
    let pr = [
        format!("Preparing {}...", words.short_label),
        format!("Generating {} content...", words.short_label),
        format!("Creating {}...", words.singular),
    ];
    let mut stages = Vec::new();
    match input.action {
        GitStackedAction::Push => return vec![push],
        GitStackedAction::CreatePr => {
            if input.should_push_before_pr {
                stages.push(push);
            }
            stages.extend(pr);
            return stages;
        }
        _ => {}
    }
    if input.feature_branch {
        stages.push("Preparing feature ref...".into());
    }
    if *input.action == GitStackedAction::Commit || input.has_working_tree_changes {
        if !input.has_custom_commit_message {
            stages.push("Generating commit message...".into());
        }
        stages.push("Committing...".into());
    }
    match input.action {
        GitStackedAction::CommitPush => stages.push(push),
        GitStackedAction::CommitPushPr => {
            stages.push(push);
            stages.extend(pr);
        }
        _ => {}
    }
    stages
}

/// Whether running `action` on the default ref asks for confirmation first.
pub fn requires_default_branch_confirmation(
    action: &GitStackedAction,
    is_default_ref: bool,
) -> bool {
    is_default_ref
        && matches!(
            action,
            GitStackedAction::Push
                | GitStackedAction::CreatePr
                | GitStackedAction::CommitPush
                | GitStackedAction::CommitPushPr
        )
}

/// Copy of the default-ref confirmation dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultBranchDialogCopy {
    pub title: String,
    pub description: String,
    pub continue_label: String,
}

/// `resolveDefaultBranchActionDialogCopy`.
pub fn default_branch_dialog_copy(
    action: &GitStackedAction,
    branch: &str,
    includes_commit: bool,
    words: &ChangeRequestPresentation,
) -> DefaultBranchDialogCopy {
    let suffix = format!(
        " on \"{branch}\". You can continue on this ref or create a feature ref and run the same action there."
    );
    let short = words.short_label;
    let singular = words.singular;
    let (title, description, continue_label) = match (action, includes_commit) {
        (GitStackedAction::Push | GitStackedAction::CommitPush, true) => (
            "Commit & push to default ref?".to_owned(),
            format!("This action will commit and push changes{suffix}"),
            format!("Commit & push to {branch}"),
        ),
        (GitStackedAction::Push | GitStackedAction::CommitPush, false) => (
            "Push to default ref?".to_owned(),
            format!("This action will push local commits{suffix}"),
            format!("Push to {branch}"),
        ),
        (_, true) => (
            format!("Commit, push & create {short} from default ref?"),
            format!("This action will commit, push, and create a {singular}{suffix}"),
            format!("Commit, push & create {short}"),
        ),
        (_, false) => (
            format!("Push & create {short} from default ref?"),
            format!("This action will push local commits and create a {singular}{suffix}"),
            format!("Push & create {short}"),
        ),
    };
    DefaultBranchDialogCopy {
        title,
        description,
        continue_label,
    }
}

/// The progress toast's elapsed description: `Running for 12s`, `Running for 2m 5s`.
pub fn format_elapsed(seconds: u64) -> String {
    if seconds < 60 {
        format!("Running for {seconds}s")
    } else {
        format!("Running for {}m {}s", seconds / 60, seconds % 60)
    }
}

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("pull request reference pattern compiles")
}

static GITHUB_URL: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^https://github\.com/[^/\s]+/[^/\s]+/pull/(\d+)(?:[/?#].*)?$"));
static GITLAB_URL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)^https://[^/\s]*gitlab[^/\s]*/.+/-/merge_requests/(\d+)(?:[/?#].*)?$")
});
static AZURE_URL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)^https://(?:dev\.azure\.com/[^/\s]+/[^/\s]+|[^/\s]+\.visualstudio\.com/[^/\s]+)/_git/[^/\s]+/pullrequest/(\d+)(?:[/?#].*)?$",
    )
});
static FORGEJO_URL: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)^https?://[^/\s]+/(?:[^/\s]+/)+[^/\s]+/pulls/(\d+)(?:[/?#].*)?$")
});
static NUMBER: LazyLock<Regex> = LazyLock::new(|| pattern(r"^#?(\d+)$"));
static GH_CHECKOUT: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?i)^gh\s+pr\s+checkout\s+(.+)$"));
static GLAB_CHECKOUT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^glab\s+mr\s+checkout\s+(.+)$"));
static AZ_CHECKOUT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^az\s+repos\s+pr\s+checkout\s+(.+)$"));
static TEA_CHECKOUT: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^tea\s+(?:pr|pulls)\s+checkout\s+(.+)$"));

/// `parsePullRequestReference`: what to send as `reference` to `git.resolvePullRequest`, from
/// a PR/MR URL, a `gh`/`glab`/`tea`/`az` checkout command, `42` or `#42`. URLs pass through whole;
/// numbers lose the `#`. `None` when the input is none of those.
pub fn parse_pull_request_reference(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let capture = |regex: &Regex| {
        regex
            .captures(trimmed)
            .and_then(|captures| captures.get(1))
            .map(|m| m.as_str().trim().to_owned())
    };
    let normalized = capture(&GH_CHECKOUT)
        .or_else(|| capture(&GLAB_CHECKOUT))
        .or_else(|| capture(&TEA_CHECKOUT))
        .or_else(|| capture(&AZ_CHECKOUT).and_then(|args| azure_checkout_reference(&args)))
        .unwrap_or_else(|| trimmed.to_owned());
    if normalized.is_empty() {
        return None;
    }
    if [&*GITHUB_URL, &*GITLAB_URL, &*FORGEJO_URL, &*AZURE_URL]
        .iter()
        .any(|regex| regex.is_match(&normalized))
    {
        return Some(normalized);
    }
    NUMBER
        .captures(&normalized)
        .and_then(|captures| captures.get(1))
        .map(|m| m.as_str().to_owned())
}

/// The id in `az repos pr checkout` arguments: `--id N`, `-i N`, `--id=N`, else the first
/// non-flag argument.
fn azure_checkout_reference(args: &str) -> Option<String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    for (index, part) in parts.iter().enumerate() {
        if *part == "--id" || *part == "-i" {
            return parts.get(index + 1).map(|value| (*value).to_owned());
        }
        if let Some(value) = part.strip_prefix("--id=") {
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    parts
        .iter()
        .find(|part| !part.starts_with('-'))
        .map(|part| (*part).to_owned())
}

#[cfg(test)]
mod tests {
    //! Failure modes: a decision-table row shadowing an earlier one (busy, detached, diverged,
    //! default ref turning push into commit_push); a missing remote half treated as "ahead";
    //! `aheadOfDefaultCount` absent not falling back to `aheadCount`; menu rows offered without
    //! a primary remote; disabled rows without a reason or with the wrong one; GitLab words
    //! leaking "PR"; progress stages for a custom message still generating one; the PR parser
    //! accepting junk, dropping the URL, or keeping the `#`.
    use super::*;
    use t3_protocol::vcs::WorkingTree;

    fn local(ref_name: Option<&str>, changes: bool) -> VcsStatusLocal {
        VcsStatusLocal {
            is_repo: true,
            source_control_provider: None,
            has_primary_remote: true,
            is_default_ref: false,
            ref_name: ref_name.map(str::to_owned),
            has_working_tree_changes: changes,
            working_tree: WorkingTree::default(),
        }
    }

    fn remote(upstream: bool, ahead: u64, behind: u64) -> VcsStatusRemote {
        VcsStatusRemote {
            has_upstream: upstream,
            ahead_count: ahead,
            behind_count: behind,
            ahead_of_default_count: None,
            pr: None,
        }
    }

    fn open_pr() -> VcsPullRequestSummary {
        VcsPullRequestSummary {
            number: 7,
            title: "Fix".into(),
            url: "https://github.com/o/r/pull/7".into(),
            base_ref: "main".into(),
            head_ref: "fix".into(),
            state: PullRequestState::Open,
            is_draft: None,
            updated_at: None,
        }
    }

    fn quick(local: &VcsStatusLocal, remote: Option<&VcsStatusRemote>) -> QuickAction {
        resolve_quick_action(Some(GitStatus::new(local, remote)), false)
    }

    #[test]
    fn early_rows_win() {
        let feature = local(Some("feature"), true);
        assert_eq!(
            resolve_quick_action(Some(GitStatus::new(&feature, None)), true).disabled_reason(),
            Some("Git action in progress.")
        );
        assert_eq!(
            resolve_quick_action(None, false).disabled_reason(),
            Some("Git status is unavailable.")
        );
        let detached = local(None, true);
        assert_eq!(
            quick(&detached, None).disabled_reason(),
            Some("Create and checkout a ref before pushing or opening a pull request.")
        );
    }

    #[test]
    fn changes_pick_commit_variants() {
        let mut no_remote = local(Some("feature"), true);
        no_remote.has_primary_remote = false;
        assert_eq!(
            quick(&no_remote, None).kind,
            QuickActionKind::RunAction(GitStackedAction::Commit)
        );
        let feature = local(Some("feature"), true);
        let upstream = remote(true, 0, 0);
        let action = quick(&feature, Some(&upstream));
        assert_eq!(action.label, "Commit, push & PR");
        let mut with_pr = upstream.clone();
        with_pr.pr = Some(open_pr());
        assert_eq!(quick(&feature, Some(&with_pr)).label, "Commit & push");
        let mut default = feature.clone();
        default.is_default_ref = true;
        assert_eq!(
            quick(&default, Some(&upstream)).kind,
            QuickActionKind::RunAction(GitStackedAction::CommitPush)
        );
    }

    #[test]
    fn clean_branches_without_upstream() {
        let clean = local(Some("feature"), false);
        // A missing remote half is "no upstream, nothing ahead".
        assert_eq!(
            quick(&clean, None).disabled_reason(),
            Some("No local commits to push.")
        );
        let ahead = remote(false, 2, 0);
        assert_eq!(quick(&clean, Some(&ahead)).label, "Push & create PR");
        let mut default = clean.clone();
        default.is_default_ref = true;
        assert_eq!(
            quick(&default, Some(&ahead)).kind,
            QuickActionKind::RunAction(GitStackedAction::CommitPush)
        );
        let mut unpublished = clean.clone();
        unpublished.has_primary_remote = false;
        assert_eq!(quick(&unpublished, None).kind, QuickActionKind::OpenPublish);
    }

    #[test]
    fn upstream_rows() {
        let clean = local(Some("feature"), false);
        assert_eq!(quick(&clean, Some(&remote(true, 1, 1))).label, "Sync ref");
        assert_eq!(
            quick(&clean, Some(&remote(true, 0, 3))).kind,
            QuickActionKind::RunPull
        );
        assert_eq!(
            quick(&clean, Some(&remote(true, 2, 0))).kind,
            QuickActionKind::RunAction(GitStackedAction::CreatePr)
        );
        let mut with_pr = remote(true, 0, 0);
        with_pr.pr = Some(open_pr());
        assert_eq!(quick(&clean, Some(&with_pr)).label, "View PR");
        let mut delta = remote(true, 0, 0);
        delta.ahead_of_default_count = Some(3);
        assert_eq!(quick(&clean, Some(&delta)).label, "Create PR");
        assert_eq!(
            quick(&clean, Some(&remote(true, 0, 0))).disabled_reason(),
            Some("Branch is up to date. No action needed.")
        );
    }

    #[test]
    fn gitlab_words() {
        let mut clean = local(Some("feature"), false);
        clean.source_control_provider = Some(SourceControlProviderInfo {
            kind: "gitlab".into(),
            name: String::new(),
            base_url: "https://gitlab.com".into(),
        });
        assert_eq!(
            quick(&clean, Some(&remote(false, 1, 0))).label,
            "Push & create MR"
        );
        let words = GitStatus::new(&clean, None).presentation();
        assert_eq!(words.provider_name, "GitLab");
        assert_eq!(words.kind, ProviderKind::GitLab);
        let other = ChangeRequestPresentation::for_provider(Some(&SourceControlProviderInfo {
            kind: "gitea".into(),
            name: "Gitea".into(),
            base_url: String::new(),
        }));
        assert_eq!(other.kind, ProviderKind::Generic);
        assert_eq!(other.provider_name, "Gitea");
    }

    #[test]
    fn menu_rows_and_reasons() {
        let clean = local(Some("feature"), false);
        let up_to_date = remote(true, 0, 0);
        let items = menu_items(Some(GitStatus::new(&clean, Some(&up_to_date))), false);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, ["Commit", "Push", "Create PR"]);
        assert_eq!(
            items[0].disabled_reason.as_deref(),
            Some("Worktree is clean. Make changes before committing.")
        );
        assert_eq!(
            items[1].disabled_reason.as_deref(),
            Some("No local commits to push.")
        );
        assert_eq!(
            items[2].disabled_reason.as_deref(),
            Some("No local commits to include in a pull request.")
        );
        let ahead = remote(true, 1, 0);
        let items = menu_items(Some(GitStatus::new(&clean, Some(&ahead))), false);
        assert!(!items[1].disabled && items[1].disabled_reason.is_none());
        assert_eq!(items[1].kind, MenuItemKind::Run(GitStackedAction::Push));
        let mut unpublished = clean.clone();
        unpublished.has_primary_remote = false;
        assert_eq!(
            menu_items(Some(GitStatus::new(&unpublished, None)), false).len(),
            1
        );
        let busy = menu_items(Some(GitStatus::new(&clean, Some(&ahead))), true);
        assert!(
            busy.iter()
                .all(|item| { item.disabled_reason.as_deref() == Some("Git action in progress.") })
        );
        assert_eq!(
            menu_warnings(Some(GitStatus::new(&clean, Some(&remote(true, 0, 2))))),
            ["Behind upstream. Pull/rebase first."]
        );
    }

    #[test]
    fn progress_stages_and_dialog_copy() {
        let words = ChangeRequestPresentation::for_provider(None);
        let stages = |action: GitStackedAction, custom: bool, changes: bool, feature: bool| {
            progress_stages(
                ProgressInput {
                    action: &action,
                    has_custom_commit_message: custom,
                    has_working_tree_changes: changes,
                    feature_branch: feature,
                    should_push_before_pr: true,
                },
                &words,
            )
        };
        assert_eq!(
            stages(GitStackedAction::CommitPushPr, false, true, true),
            [
                "Preparing feature ref...",
                "Generating commit message...",
                "Committing...",
                "Pushing...",
                "Preparing PR...",
                "Generating PR content...",
                "Creating pull request...",
            ]
        );
        assert_eq!(
            stages(GitStackedAction::Commit, true, false, false),
            ["Committing..."]
        );
        assert_eq!(
            stages(GitStackedAction::CommitPush, false, false, false),
            ["Pushing..."]
        );
        assert_eq!(
            stages(GitStackedAction::CreatePr, false, false, false)[0],
            "Pushing..."
        );
        let copy = default_branch_dialog_copy(&GitStackedAction::CommitPush, "main", true, &words);
        assert_eq!(copy.title, "Commit & push to default ref?");
        assert_eq!(copy.continue_label, "Commit & push to main");
        assert!(copy.description.ends_with("run the same action there."));
        assert!(requires_default_branch_confirmation(
            &GitStackedAction::Push,
            true
        ));
        assert!(!requires_default_branch_confirmation(
            &GitStackedAction::Commit,
            true
        ));
        assert_eq!(format_elapsed(59), "Running for 59s");
        assert_eq!(format_elapsed(125), "Running for 2m 5s");
    }

    #[test]
    fn pull_request_references() {
        let parse = parse_pull_request_reference;
        assert_eq!(parse("#42").as_deref(), Some("42"));
        assert_eq!(parse(" 42 ").as_deref(), Some("42"));
        assert_eq!(parse("gh pr checkout 42").as_deref(), Some("42"));
        assert_eq!(
            parse("gh pr checkout https://github.com/o/r/pull/42").as_deref(),
            Some("https://github.com/o/r/pull/42")
        );
        assert_eq!(
            parse("https://GitHub.com/o/r/pull/42/files").as_deref(),
            Some("https://GitHub.com/o/r/pull/42/files")
        );
        assert_eq!(
            parse("https://gitlab.example.com/g/sub/p/-/merge_requests/9").as_deref(),
            Some("https://gitlab.example.com/g/sub/p/-/merge_requests/9")
        );
        assert_eq!(parse("az repos pr checkout --id 17").as_deref(), Some("17"));
        assert_eq!(parse("az repos pr checkout --id=18").as_deref(), Some("18"));
        assert_eq!(parse("glab mr checkout #5").as_deref(), Some("5"));
        assert_eq!(parse("tea pr checkout 12").as_deref(), Some("12"));
        assert_eq!(
            parse("https://codeberg.org/owner/repo/pulls/42").as_deref(),
            Some("https://codeberg.org/owner/repo/pulls/42")
        );
        assert_eq!(parse("feature-branch"), None);
        assert_eq!(parse("https://github.com/o/r/issues/4"), None);
        assert_eq!(parse("   "), None);
    }
}
