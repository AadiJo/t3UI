//! Environment-scoped references. Ids are only unique within one environment (server), so the
//! client keys projects and threads by `(environment, id)`. The string form
//! `<environmentId>:<id>` is the web's `scopedRefKey` and keys persisted state.

use std::fmt;

use t3_protocol::{EnvironmentId, ProjectId, ThreadId};

/// A thread in a specific environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadRef {
    pub environment_id: EnvironmentId,
    pub thread_id: ThreadId,
}

impl ThreadRef {
    pub fn new(environment_id: EnvironmentId, thread_id: ThreadId) -> Self {
        Self {
            environment_id,
            thread_id,
        }
    }

    /// `<environmentId>:<threadId>`, the key of per-thread persisted state.
    pub fn key(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for ThreadRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.environment_id, self.thread_id)
    }
}

/// A project in a specific environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectRef {
    pub environment_id: EnvironmentId,
    pub project_id: ProjectId,
}

impl ProjectRef {
    pub fn new(environment_id: EnvironmentId, project_id: ProjectId) -> Self {
        Self {
            environment_id,
            project_id,
        }
    }

    /// `<environmentId>:<projectId>`.
    pub fn key(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for ProjectRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.environment_id, self.project_id)
    }
}
