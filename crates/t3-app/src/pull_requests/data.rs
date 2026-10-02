//! The page's reads. Connected environments answer through their client; snapshot scenes, whose
//! environments have none, install a [`Fixture`] global that answers instead, shaped exactly like
//! the server's responses.

use std::collections::HashMap;

use gpui_kit::{App, AppContext as _, Entity, Global, Task};
use t3_client::RpcError;
use t3_logic::pull_requests::{self as logic, Viewers};
use t3_protocol::{
    EnvironmentId,
    errors::ServerError,
    methods::{PullRequestsInvalidate, PullRequestsList, PullRequestsListStats},
    pull_requests::{
        Involvement, PullRequestDiffStat, PullRequestInvalidateInput, PullRequestListInput,
        PullRequestListResult, PullRequestListStatsInput, SourceControlProviderKind,
    },
};

use crate::state::Environment;

/// What a failed read says (`formatEnvironmentQueryError`): the server error's own sentence.
pub type ReadError = String;

/// Recorded answers for environments without a client.
#[derive(Clone, Default)]
pub struct Fixture {
    /// The full listing each environment would give for `state: all`; reads narrow it by state
    /// and involvement the way the server does.
    pub lists: HashMap<EnvironmentId, Result<PullRequestListResult, ReadError>>,
    /// Line counts `pullRequests.listStats` answers with.
    pub stats: HashMap<EnvironmentId, Vec<PullRequestDiffStat>>,
    /// Listings never answer (the loading ghost).
    pub pending: bool,
}

impl Global for Fixture {}

/// Answers this app's reads from `fixture` wherever an environment has no client.
pub fn install_fixture(fixture: Fixture, cx: &mut App) {
    cx.set_global(fixture);
}

/// `pullRequests.list` on one environment.
pub fn list(
    environment: &Entity<Environment>,
    input: PullRequestListInput,
    cx: &App,
) -> Task<Result<PullRequestListResult, ReadError>> {
    let environment = environment.read(cx);
    let Some(client) = environment.client().cloned() else {
        if cx
            .try_global::<Fixture>()
            .is_some_and(|fixture| fixture.pending)
        {
            return cx.background_spawn(std::future::pending());
        }
        let answer = cx
            .try_global::<Fixture>()
            .and_then(|fixture| fixture.lists.get(environment.id()).cloned())
            .map(|answer| answer.map(|list| narrow_fixture(list, &input)))
            .unwrap_or_else(|| Err(not_connected(environment.label())));
        return Task::ready(answer);
    };
    let label = environment.label().to_string();
    cx.background_spawn(async move {
        client
            .request::<PullRequestsList>(&input)
            .await
            .map_err(|error| read_error(error, &label))
    })
}

/// `pullRequests.listStats` on one environment.
pub fn list_stats(
    environment: &Entity<Environment>,
    input: PullRequestListStatsInput,
    cx: &App,
) -> Task<Result<Vec<PullRequestDiffStat>, ReadError>> {
    let environment = environment.read(cx);
    let Some(client) = environment.client().cloned() else {
        let stats = cx
            .try_global::<Fixture>()
            .and_then(|fixture| fixture.stats.get(environment.id()).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|stat| {
                input.refs.iter().any(|reference| {
                    reference.project_id == stat.project_id && reference.number == stat.number
                })
            })
            .collect();
        return Task::ready(Ok(stats));
    };
    let label = environment.label().to_string();
    cx.background_spawn(async move {
        client
            .request::<PullRequestsListStats>(&input)
            .await
            .map(|result| result.stats)
            .map_err(|error| read_error(error, &label))
    })
}

/// `pullRequests.invalidate {}`: forget cached listings so the next read asks the hosts.
pub fn invalidate(environment: &Entity<Environment>, cx: &App) -> Task<()> {
    let Some(client) = environment.read(cx).client().cloned() else {
        return Task::ready(());
    };
    cx.background_spawn(async move {
        let input = PullRequestInvalidateInput {
            reference: None,
            files_viewed_only: None,
        };
        client.request::<PullRequestsInvalidate>(&input).await.ok();
    })
}

fn not_connected(label: &str) -> ReadError {
    format!("{label} is not connected.")
}

/// The sentence a failed read shows: the tagged pull request errors' own messages, else the
/// server's message, else the web's generic fallback.
pub fn read_error(error: RpcError<ServerError>, label: &str) -> ReadError {
    match error {
        RpcError::Failed(error) => server_error_message(&error),
        RpcError::Disconnected(_) => not_connected(label),
        _ => "The environment request failed.".to_owned(),
    }
}

/// `PullRequestUnavailableError.message`, `PullRequestOperationError.message`, or the error's
/// own text.
pub fn server_error_message(error: &ServerError) -> String {
    match error.tag.as_str() {
        ServerError::PULL_REQUEST_UNAVAILABLE => {
            let provider = error.provider().map(SourceControlProviderKind::from);
            logic::unavailable_message(error.reason().unwrap_or_default(), provider.as_ref())
        }
        ServerError::PULL_REQUEST_OPERATION => logic::operation_message(
            error.operation().unwrap_or_default(),
            error.detail.as_deref().unwrap_or_default(),
        ),
        _ => {
            let message = error.display_message().trim();
            if message.is_empty() {
                "The environment request failed.".to_owned()
            } else {
                message.to_owned()
            }
        }
    }
}

/// What the server would answer for `input` from a recorded `state: all` listing.
fn narrow_fixture(
    mut list: PullRequestListResult,
    input: &PullRequestListInput,
) -> PullRequestListResult {
    let viewers: Viewers = list.viewers.clone();
    list.entries.retain(|entry| {
        let viewer = viewers.get(&entry.host).map(|login| login.to_lowercase());
        let authored = entry
            .author
            .as_ref()
            .is_some_and(|author| Some(author.login.to_lowercase()) == viewer);
        logic::state_matches(&input.state, &entry.state)
            && match input.involvement.as_ref() {
                Some(Involvement::Reviewing) => entry.viewer_review_requested,
                Some(Involvement::Authored) => authored,
                _ => true,
            }
            && input
                .project_id
                .as_ref()
                .is_none_or(|project| &entry.project_id == project)
            && input.host.as_ref().is_none_or(|host| &entry.host == host)
    });
    list
}
