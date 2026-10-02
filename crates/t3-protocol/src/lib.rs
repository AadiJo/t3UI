//! Wire types for the T3 Code server protocol (upstream `packages/contracts`).
//!
//! GPUI-free and IO-free: serde types, typed method descriptors, and the Effect RPC frame
//! codec. The upstream server ships nightly, so decoders ignore unknown fields, string unions
//! keep an `Other(String)` variant, and tagged unions keep an `Unknown` variant.
//!
//! - [`methods`]: one type per RPC (`ServerGetConfig`, `SubscribeThread`, `DispatchCommand`, ...).
//! - [`orchestration`]: the read model (projects, threads, messages, activities), events, and
//!   stream items. [`commands`]: what `orchestration.dispatchCommand` accepts.
//! - [`environment`]: descriptor and HTTP auth shapes. [`server`]: config, providers, settings.
//! - [`pull_requests`]: the `/pull-requests` route. [`usage`]: token usage, cost, rate limits.
//! - [`providers`]: provider sign-in and installs. [`workspace`]: clone, create, import,
//!   worktree setup. [`server_ops`]: self-update, diagnostics, telemetry, relay client.
//!   [`preview`]: the in-app browser. [`device`]: simulators and emulators.
//! - [`terminal`], [`vcs`], [`projects`]: the remaining feature RPCs.

pub mod commands;
pub mod device;
pub mod environment;
pub mod errors;
pub mod ids;
pub mod method;
pub mod methods;
pub mod orchestration;
pub mod preview;
pub mod projects;
pub mod providers;
pub mod pull_requests;
pub mod rpc;
pub mod schema;
pub mod server;
pub mod server_ops;
pub mod terminal;
pub mod usage;
pub mod vcs;
pub mod workspace;

pub use errors::ServerError;
pub use ids::*;
pub use method::{Stream, Unary};
