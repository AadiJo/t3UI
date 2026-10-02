//! The pull requests page's GPU-free logic, ported from the fork's `components/pullRequest/*`
//! logic files (spec: `docs/spec/pull-requests.md`). `t3-app::pull_requests` owns the queries
//! and renders; everything that decides what a row says or where it goes lives here.
//!
//! - [`query`]: the search box (`label:` qualifiers, matching, relevance).
//! - [`list`]: merging environments, involvement, grouping, overrides, facets.
//! - [`sort`]: the Sort menu's orders.
//! - [`scope`]: which servers and projects a listing asks about.
//! - [`stats`]: the deferred `pullRequests.listStats` reads.
//! - [`preferences`]: the remembered list controls.
//! - [`presentation`]: labels, tones and messages shared by the list and the detail panel.
//!
//! Rows are `Arc<EnvironmentEntry>` so a refresh that changes one row reallocates one row.

pub mod list;
pub mod preferences;
pub mod presentation;
pub mod query;
pub mod scope;
pub mod sort;
pub mod stats;

#[cfg(test)]
mod tests;

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use t3_protocol::{EnvironmentId, pull_requests::PullRequestListEntry};

pub use list::*;
pub use preferences::*;
pub use presentation::*;
pub use query::*;
pub use scope::*;
pub use sort::*;
pub use stats::*;
pub use t3_protocol::pull_requests::{Involvement, ListState};

/// A listed pull request with the environment that read it. The listing itself does not say
/// which machine a row came from, and the page unions every connected one, so acting on a row,
/// refreshing it, or opening its detail all need this tag (`EnvironmentPullRequestEntry`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentEntry {
    pub environment_id: EnvironmentId,
    #[serde(flatten)]
    pub entry: PullRequestListEntry,
}

impl Deref for EnvironmentEntry {
    type Target = PullRequestListEntry;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}
