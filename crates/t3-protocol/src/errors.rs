//! Typed RPC failures (`Schema.TaggedError`).
//!
//! Every method's error union is a set of tagged errors, and every union also contains
//! `EnvironmentAuthorizationError`. The unions are large and grow, so all methods share one
//! decoded shape: the `_tag`, the common text fields, and everything else in `fields`.

use serde::Deserialize;
use serde_json::{Map, Value};

/// A typed failure returned by any RPC method.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerError {
    #[serde(rename = "_tag")]
    pub tag: String,
    pub message: Option<String>,
    pub detail: Option<String>,
    /// Set on `EnvironmentAuthorizationError`: the scope the caller lacks.
    pub required_scope: Option<String>,
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

impl ServerError {
    pub const AUTHORIZATION: &'static str = "EnvironmentAuthorizationError";
    /// Provider CLI missing or signed out, or the host is unsupported. See [`reason`](Self::reason)
    /// (`cli-missing`, `cli-unauthenticated`, `provider-unsupported`) and
    /// [`provider`](Self::provider).
    pub const PULL_REQUEST_UNAVAILABLE: &'static str = "PullRequestUnavailableError";
    /// A host call failed. See [`operation`](Self::operation) and `detail`.
    pub const PULL_REQUEST_OPERATION: &'static str = "PullRequestOperationError";
    pub const DISPATCH: &'static str = "OrchestrationDispatchCommandError";
    pub const GET_SNAPSHOT: &'static str = "OrchestrationGetSnapshotError";

    /// The caller's credential lacks [`required_scope`](Self::required_scope).
    pub fn is_authorization(&self) -> bool {
        self.tag == Self::AUTHORIZATION
    }

    /// Text to show the user: `message`, else `detail`, else `reason`, else the tag.
    pub fn display_message(&self) -> &str {
        self.message
            .as_deref()
            .or(self.detail.as_deref())
            .or_else(|| self.reason())
            .unwrap_or(&self.tag)
    }

    fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }

    /// `reason` of errors that carry one (`PullRequestUnavailableError`, `UsageReadError`, ...).
    pub fn reason(&self) -> Option<&str> {
        self.field("reason")
    }

    /// `provider` of errors that carry one (source control kind or provider driver).
    pub fn provider(&self) -> Option<&str> {
        self.field("provider")
    }

    /// `operation` of errors that carry one (`PullRequestOperationError`, git errors, ...).
    pub fn operation(&self) -> Option<&str> {
        self.field("operation")
    }

    /// For a failed `thread.turn.start` with `bootstrap.create_thread`: `"not-created"` or
    /// `"deleted"` means the thread does not exist, so a retry cannot create a duplicate.
    pub fn bootstrap_thread_disposition(&self) -> Option<&str> {
        self.fields
            .get("bootstrapThreadDisposition")
            .and_then(Value::as_str)
    }
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.tag, self.display_message())
    }
}

impl std::error::Error for ServerError {}
