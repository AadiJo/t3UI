//! Connection status and failure types shown to the UI (`packages/client-runtime/src/
//! connection/model.ts`, `presentation.ts`; connections.md 1.6).

use std::time::Instant;

/// Where an environment's connection stands. Published on [`Environment::status`].
///
/// [`Environment::status`]: crate::Environment::status
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    /// Saved but not connecting (disabled, or disconnected by the user).
    Available,
    /// The app reported the network as offline.
    Offline,
    /// An attempt is running. `attempt > 1` or a `last_failure` means this is a retry.
    Connecting {
        attempt: u32,
        stage: ConnectStage,
        last_failure: Option<ConnectionFailure>,
    },
    /// The session is ready (first server config received). `generation` increments per session.
    Connected { generation: u64 },
    /// Waiting before the next attempt (3 s, 4 s, 8 s, then 16 s).
    Reconnecting {
        attempt: u32,
        retry_at: Instant,
        failure: ConnectionFailure,
    },
    /// Will not retry on its own (bad credential, wrong server, unsupported protocol). Any
    /// signal (retry, app activation) tries again.
    Blocked { failure: ConnectionFailure },
}

impl ConnectionStatus {
    pub fn is_connected(&self) -> bool {
        matches!(self, ConnectionStatus::Connected { .. })
    }

    /// Upstream's `connectionStatusText` (`presentation.ts:28-87`).
    pub fn status_text(&self) -> String {
        match self {
            ConnectionStatus::Available => "Available".into(),
            ConnectionStatus::Offline => "Offline".into(),
            ConnectionStatus::Connecting {
                attempt,
                last_failure: None,
                ..
            } if *attempt <= 1 => "Connecting...".into(),
            ConnectionStatus::Connecting {
                last_failure: Some(failure),
                ..
            }
            | ConnectionStatus::Reconnecting { failure, .. } => {
                format!("Failed to connect. Reconnecting... Reason: {}", failure.detail)
            }
            ConnectionStatus::Connecting { .. } => "Reconnecting...".into(),
            ConnectionStatus::Connected { .. } => "Connected".into(),
            ConnectionStatus::Blocked { failure } => match failure.kind {
                FailureKind::Blocked(BlockedReason::Unsupported) => "Client not supported".into(),
                _ if failure.detail.is_empty() => "Connection failed".into(),
                _ => format!("Connection failed. Reason: {}", failure.detail),
            },
        }
    }
}

/// Progress within one connection attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectStage {
    /// Descriptor check and credential (ticket) fetch.
    Preparing,
    /// Opening the WebSocket.
    Opening,
    /// Waiting for the first server config.
    Synchronizing,
}

/// Why an attempt failed. `detail` is user-facing copy.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct ConnectionFailure {
    pub kind: FailureKind,
    pub detail: String,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Retry with backoff.
    Transient(TransientReason),
    /// Wait for the user or app activation.
    Blocked(BlockedReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransientReason {
    Network,
    Timeout,
    Transport,
    EndpointUnavailable,
    RelayUnavailable,
    RemoteUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedReason {
    Authentication,
    Configuration,
    Permission,
    Unsupported,
}

impl ConnectionFailure {
    pub fn transient(reason: TransientReason, detail: impl Into<String>) -> Self {
        ConnectionFailure {
            kind: FailureKind::Transient(reason),
            detail: detail.into(),
            trace_id: None,
        }
    }

    pub fn blocked(reason: BlockedReason, detail: impl Into<String>) -> Self {
        ConnectionFailure {
            kind: FailureKind::Blocked(reason),
            detail: detail.into(),
            trace_id: None,
        }
    }

    pub fn with_trace_id(mut self, trace_id: Option<String>) -> Self {
        self.trace_id = trace_id;
        self
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self.kind, FailureKind::Blocked(_))
    }
}
