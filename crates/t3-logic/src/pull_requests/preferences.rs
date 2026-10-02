//! The remembered list controls (`pullRequestListPreferences.ts`). Every list-control change
//! writes the whole scope; opening the page reads it back. The selected row is not remembered.

use serde::{Deserialize, Serialize};
use t3_protocol::{
    EnvironmentId, ProjectId,
    pull_requests::{ChecksState, Involvement, ListState},
};

use super::ListSort;

const MAX_LABELS: usize = 10;
const MAX_TEXT: usize = 200;

/// The page's list scope as stored (`t3.pullRequests.preferences`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListPreferences {
    pub involvement: Involvement,
    pub state: ListState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<EnvironmentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    /// `only` or `hide`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// `approved`, `changes-requested`, `review-required` or `none`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<ChecksState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Absent means [`ListSort::Ready`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<ListSort>,
}

impl Default for ListPreferences {
    fn default() -> Self {
        Self {
            involvement: Involvement::All,
            state: ListState::Open,
            environment_id: None,
            project_id: None,
            host: None,
            q: None,
            draft: None,
            review: None,
            checks: None,
            author: None,
            labels: Vec::new(),
            sort: None,
        }
    }
}

impl ListPreferences {
    /// Decodes stored preferences; anything malformed reads as the defaults.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Self>(json)
            .map(Self::normalized)
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.clone().normalized()).unwrap_or_default()
    }

    /// The effective sort.
    pub fn sort(&self) -> ListSort {
        self.sort.unwrap_or_default()
    }

    /// Bounds and drops values the page would never send: empty strings, unknown enum values,
    /// `sort: ready`, more than ten labels.
    pub fn normalized(mut self) -> Self {
        if matches!(self.involvement, Involvement::Other(_)) {
            self.involvement = Involvement::All;
        }
        if matches!(self.state, ListState::Other(_)) {
            self.state = ListState::Open;
        }
        let bound = |value: Option<String>| {
            value
                .map(|value| value.trim().chars().take(MAX_TEXT).collect::<String>())
                .filter(|value| !value.is_empty())
        };
        self.host = bound(self.host);
        self.q = bound(self.q);
        self.author = bound(self.author);
        self.draft = self
            .draft
            .filter(|draft| draft == "only" || draft == "hide");
        self.review = self.review.filter(|review| {
            matches!(
                review.as_str(),
                "approved" | "changes-requested" | "review-required" | "none"
            )
        });
        self.checks = self
            .checks
            .filter(|checks| matches!(checks, ChecksState::Passing | ChecksState::Failing));
        let mut labels: Vec<String> = Vec::new();
        for label in self.labels {
            let label: String = label.trim().chars().take(MAX_TEXT).collect();
            if !label.is_empty()
                && !labels.iter().any(|held| held.eq_ignore_ascii_case(&label))
                && labels.len() < MAX_LABELS
            {
                labels.push(label);
            }
        }
        self.labels = labels;
        if self.sort == Some(ListSort::Ready) {
            self.sort = None;
        }
        self
    }
}
