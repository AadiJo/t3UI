//! The thread row's pull request badge (`web/components/ThreadStatusIndicators.tsx`,
//! `packages/shared/src/sourceControl.ts`).

use t3_protocol::{
    orchestration::PullRequestState,
    vcs::{VcsStatusLocal, VcsStatusRemote},
};

/// What the badge shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestBadge {
    pub state: PullRequestState,
    /// `#12 PR open: Fix the thing`.
    pub tooltip: String,
    pub url: String,
}

/// "PR" for GitHub (and an unknown provider), Azure DevOps and Bitbucket, "MR" for GitLab,
/// "change request" for anything else.
pub fn change_request_short_name(provider_kind: Option<&str>) -> &'static str {
    match provider_kind {
        None | Some("github" | "azure-devops" | "bitbucket") => "PR",
        Some("gitlab") => "MR",
        Some(_) => "change request",
    }
}

/// The badge for a thread on `branch`: only when the status is for that branch and it has an
/// open, closed, or merged pull request.
pub fn pull_request_badge(
    branch: Option<&str>,
    local: Option<&VcsStatusLocal>,
    remote: Option<&VcsStatusRemote>,
) -> Option<PullRequestBadge> {
    let branch = branch?;
    let local = local?;
    if local.ref_name.as_deref() != Some(branch) {
        return None;
    }
    let pr = remote?.pr.as_ref()?;
    let state = match pr.state {
        PullRequestState::Open => "open",
        PullRequestState::Closed => "closed",
        PullRequestState::Merged => "merged",
        _ => return None,
    };
    let short = change_request_short_name(
        local
            .source_control_provider
            .as_ref()
            .map(|provider| provider.kind.as_str()),
    );
    Some(PullRequestBadge {
        state: pr.state.clone(),
        tooltip: format!("#{} {short} {state}: {}", pr.number, pr.title),
        url: pr.url.clone(),
    })
}

#[cfg(test)]
mod tests {
    //! Failure modes: showing a PR for another branch (stale status after a checkout), showing
    //! one for a thread without a branch, wrong GitLab wording, unknown PR states.
    use serde_json::json;

    use super::*;

    fn local(branch: &str, provider: Option<&str>) -> VcsStatusLocal {
        serde_json::from_value(json!({
            "isRepo": true,
            "refName": branch,
            "sourceControlProvider": provider.map(|kind| json!({"kind": kind, "name": kind, "baseUrl": ""})),
        }))
        .unwrap()
    }

    fn remote(state: &str) -> VcsStatusRemote {
        serde_json::from_value(json!({
            "pr": {"number": 12, "title": "Fix it", "url": "https://x/pr/12", "baseRef": "main",
                   "headRef": "fix", "state": state},
        }))
        .unwrap()
    }

    #[test]
    fn badge_needs_matching_branch_and_pr() {
        let badge = pull_request_badge(
            Some("fix"),
            Some(&local("fix", None)),
            Some(&remote("open")),
        );
        assert_eq!(badge.unwrap().tooltip, "#12 PR open: Fix it");
        assert_eq!(
            pull_request_badge(
                Some("main"),
                Some(&local("fix", None)),
                Some(&remote("open"))
            ),
            None
        );
        assert_eq!(
            pull_request_badge(None, Some(&local("fix", None)), Some(&remote("open"))),
            None
        );
        assert_eq!(
            pull_request_badge(Some("fix"), Some(&local("fix", None)), None),
            None
        );
        assert_eq!(
            pull_request_badge(
                Some("fix"),
                Some(&local("fix", None)),
                Some(&remote("draft-ish"))
            ),
            None
        );
    }

    #[test]
    fn provider_wording() {
        let badge = pull_request_badge(
            Some("fix"),
            Some(&local("fix", Some("gitlab"))),
            Some(&remote("merged")),
        );
        assert_eq!(badge.unwrap().tooltip, "#12 MR merged: Fix it");
        assert_eq!(change_request_short_name(Some("gitea")), "change request");
        assert_eq!(change_request_short_name(Some("bitbucket")), "PR");
    }
}
