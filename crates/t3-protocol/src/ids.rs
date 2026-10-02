//! Branded ids from `packages/contracts/src/baseSchemas.ts`. On the wire they are plain
//! strings; here each is its own type so a `ThreadId` cannot be passed where a `ProjectId` is
//! expected.

use crate::id;

id!(
    /// Stable server identity from `/.well-known/t3/environment`. Survives restarts and URL changes.
    EnvironmentId
);
id!(ProjectId);
id!(ThreadId);
id!(
    /// Message id. User messages use the client's UUID; assistant ids look like `assistant:<key>`.
    MessageId
);
id!(TurnId);
id!(
    /// Per-command id. Re-dispatching the same id is idempotent on the server.
    CommandId
);
id!(EventId);
id!(ActivityId);
id!(PlanId);
id!(
    /// Id of a pending approval or user-input request (`payload.requestId` on activities).
    ApprovalRequestId
);
id!(AttachmentId);
id!(
    /// Routing key of a configured provider instance (`ModelSelection.instanceId`).
    ProviderInstanceId
);
id!(
    /// Terminal id within a thread, e.g. `"default"` or `"terminal-2"`.
    TerminalId
);
