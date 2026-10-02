//! How a pull request reads wherever it appears: state, conflicts, checks, review verdicts,
//! label colors, provider names and the server's error sentences (`pullRequestPresentation.tsx`,
//! `pullRequestIcons.tsx`, `packages/shared/src/sourceControl.ts`,
//! `packages/contracts/src/pullRequest.ts:1244-1411`). Colors and icons live in `t3-app`.

use t3_protocol::{
    orchestration::PullRequestState,
    pull_requests::{
        CheckStatus, ChecksState, Mergeability, PullRequestCheck, ReviewDecision,
        SourceControlProviderKind,
    },
};

/// The four looks a pull request's lifecycle has. Closed and merged win over a stale draft flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateKey {
    Open,
    Draft,
    Closed,
    Merged,
}

impl StateKey {
    pub fn resolve(state: &PullRequestState, is_draft: bool) -> Self {
        match state {
            PullRequestState::Closed => Self::Closed,
            PullRequestState::Merged => Self::Merged,
            _ if is_draft => Self::Draft,
            _ => Self::Open,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Draft => "Draft",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
        }
    }
}

/// The conflict badge's tooltip, only for open, ready, conflicting pull requests.
pub fn conflict_label(
    state: &PullRequestState,
    is_draft: bool,
    mergeability: &Mergeability,
    base_branch: Option<&str>,
) -> Option<String> {
    if *state != PullRequestState::Open || is_draft || *mergeability != Mergeability::Conflicting {
        return None;
    }
    Some(match base_branch {
        Some(base) if !base.is_empty() => format!("Conflicts with {base}"),
        _ => "Has conflicts".to_owned(),
    })
}

/// A list row's checks glyph tooltip (GitHub's own wording).
pub fn checks_state_label(state: &ChecksState) -> &'static str {
    match state {
        ChecksState::Passing => "All checks have passed",
        ChecksState::Failing => "Some checks were not successful",
        _ => "Some checks haven't completed yet",
    }
}

/// The review glyph's tooltip.
pub fn review_decision_label(decision: &ReviewDecision) -> &'static str {
    match decision {
        ReviewDecision::Approved => "Approved",
        ReviewDecision::ChangesRequested => "Changes requested",
        _ => "Awaiting review",
    }
}

/// The rollup a list row carries, worked out from a detail's checks; `None` with no checks.
pub fn checks_state(checks: &[PullRequestCheck]) -> Option<ChecksState> {
    let has = |status: CheckStatus| checks.iter().any(|check| check.status == status);
    if checks.is_empty() {
        None
    } else if has(CheckStatus::Failure) || has(CheckStatus::Cancelled) {
        Some(ChecksState::Failing)
    } else if has(CheckStatus::Pending) || has(CheckStatus::ActionRequired) {
        Some(ChecksState::Pending)
    } else if has(CheckStatus::Success) {
        Some(ChecksState::Passing)
    } else {
        None
    }
}

/// An Actions run waiting for a maintainer to approve it.
fn is_workflow_approval(check: &PullRequestCheck) -> bool {
    check.status == CheckStatus::ActionRequired
        && check.url.as_deref().is_some_and(|url| {
            url.split("/actions/runs/").nth(1).is_some_and(|rest| {
                let digits = rest.chars().take_while(char::is_ascii_digit).count();
                digits > 0 && matches!(rest[digits..].chars().next(), None | Some('/'))
            })
        })
}

/// One check's status word in the Checks section.
pub fn check_status_label(check: &PullRequestCheck) -> &'static str {
    if is_workflow_approval(check) {
        return "Awaiting approval";
    }
    match check.status {
        CheckStatus::Pending => "Running",
        CheckStatus::ActionRequired => "Awaiting action",
        CheckStatus::Success => "Passed",
        CheckStatus::Failure => "Failed",
        CheckStatus::Cancelled => "Cancelled",
        CheckStatus::Skipped => "Skipped",
        _ => "Neutral",
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The detail header's checks line (`summarizePullRequestChecks`).
pub fn summarize_checks(checks: &[PullRequestCheck]) -> String {
    if checks.is_empty() {
        return "No checks reported".to_owned();
    }
    let total = checks.len();
    let count = |status: CheckStatus| checks.iter().filter(|check| check.status == status).count();
    let approvals = checks
        .iter()
        .filter(|check| is_workflow_approval(check))
        .count();
    let other_action = count(CheckStatus::ActionRequired) - approvals;
    let failed = count(CheckStatus::Failure) + count(CheckStatus::Cancelled);
    let pending = count(CheckStatus::Pending);
    let passed = count(CheckStatus::Success);
    if failed > 0 {
        format!("{failed} of {total} failing")
    } else if approvals > 0 && other_action > 0 {
        format!(
            "{} and {} awaiting action",
            plural(approvals, "workflow", "workflows"),
            plural(other_action, "check", "checks")
        )
    } else if approvals > 0 {
        format!(
            "{} awaiting approval",
            plural(approvals, "workflow", "workflows")
        )
    } else if other_action > 0 {
        format!(
            "{} awaiting action",
            plural(other_action, "check", "checks")
        )
    } else if pending > 0 {
        format!("{pending} of {total} running")
    } else if passed == total {
        "All checks passed".to_owned()
    } else {
        format!("{passed} of {total} passing")
    }
}

/// A 6-digit hex label color as `0xRRGGBB`; anything else falls back to the muted chip.
pub fn label_color(color: Option<&str>) -> Option<u32> {
    let hex = color?.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// The provider's own name ("GitHub"), the row's provider tooltip and host menu label.
pub fn provider_name(kind: &SourceControlProviderKind) -> &'static str {
    match kind {
        SourceControlProviderKind::Github => "GitHub",
        SourceControlProviderKind::Gitlab => "GitLab",
        SourceControlProviderKind::Forgejo => "Forgejo",
        SourceControlProviderKind::AzureDevops => "Azure DevOps",
        SourceControlProviderKind::Bitbucket => "Bitbucket",
        _ => "source control",
    }
}

/// "Open on GitHub" and friends (`openOnHostLabel`).
pub fn open_on_host_label(kind: &SourceControlProviderKind) -> String {
    match kind {
        SourceControlProviderKind::Github
        | SourceControlProviderKind::Gitlab
        | SourceControlProviderKind::Forgejo
        | SourceControlProviderKind::AzureDevops
        | SourceControlProviderKind::Bitbucket => format!("Open on {}", provider_name(kind)),
        _ => "Open on host".to_owned(),
    }
}

/// What to call a host: the provider's name, unless two hosts share a kind
/// (`pullRequestHostLabel`).
pub fn host_label(
    hosts: &[(String, SourceControlProviderKind)],
    host: &str,
    kind: &SourceControlProviderKind,
) -> String {
    let sharing = hosts.iter().filter(|(_, other)| other == kind).count();
    if sharing > 1 {
        host.to_owned()
    } else {
        provider_name(kind).to_owned()
    }
}

/// What a host needs before it can be read (`PROVIDER_REQUIREMENT`), or `None`.
pub fn provider_requirement(
    kind: &SourceControlProviderKind,
    reason: &str,
) -> Option<&'static str> {
    let (missing, unauthenticated) = match kind {
        SourceControlProviderKind::Github => (
            "GitHub CLI (`gh`) is required to browse change requests on this host. Install it from https://cli.github.com/ and reload.",
            "GitHub CLI is not authenticated. Run `gh auth login` and retry.",
        ),
        SourceControlProviderKind::Forgejo => (
            "Install Forgejo CLI (`fj` 0.6 or later) from https://codeberg.org/forgejo-contrib/forgejo-cli or Gitea CLI (`tea` 0.16 or later) from https://gitea.com/gitea/tea to browse Forgejo pull requests.",
            "Authenticate your Forgejo or Gitea server with `fj --host <server-url> auth add-token` on the T3 Code server. If fj is missing or unconfigured for that server, use `tea login add`. A configured fj account must be repaired with fj.",
        ),
        SourceControlProviderKind::Gitlab => (
            "GitLab CLI (`glab`) is required to browse change requests on this host. Install it from https://gitlab.com/gitlab-org/cli and reload.",
            "GitLab CLI is not authenticated. Run `glab auth login` and retry.",
        ),
        SourceControlProviderKind::AzureDevops => (
            "Azure CLI (`az`) with the Azure DevOps extension is required. Install `az`, then run `az extension add --name azure-devops`.",
            "Azure CLI is not signed in. Run `az login` and retry.",
        ),
        SourceControlProviderKind::Bitbucket => (
            "Bitbucket needs API credentials on the server. Add them in Settings → Source Control.",
            "Bitbucket rejected the configured credentials. Check them in Settings → Source Control.",
        ),
        _ => return None,
    };
    match reason {
        "cli-missing" => Some(missing),
        "cli-unauthenticated" => Some(unauthenticated),
        _ => None,
    }
}

/// `PullRequestUnavailableError.message`.
pub fn unavailable_message(reason: &str, provider: Option<&SourceControlProviderKind>) -> String {
    let requirement = provider.and_then(|kind| provider_requirement(kind, reason));
    match (reason, requirement) {
        ("cli-missing" | "cli-unauthenticated", Some(requirement)) => requirement.to_owned(),
        ("cli-missing", None) => {
            "The tool this host is read through is not installed or set up.".to_owned()
        }
        ("cli-unauthenticated", None) => "This host has no working credentials.".to_owned(),
        _ => "Change requests cannot be browsed for this project's host yet.".to_owned(),
    }
}

/// `PullRequestOperationError.message`.
pub fn operation_message(operation: &str, detail: &str) -> String {
    format!("Pull request operation {operation} failed: {detail}")
}

/// `toLocaleString()` for counts: `1,234`.
pub fn format_count(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}
