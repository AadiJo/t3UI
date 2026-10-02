//! Networking and environment management for T3 Code servers.
//!
//! Runs on its own tokio runtime ([`runtime`]) and is GPUI-free. The app talks to it through
//! handles whose futures and channels are executor-agnostic, so GPUI tasks can await them.
//!
//! - [`Environment`]: one server. Status, server config, and shell state as `watch`
//!   receivers; [`Environment::open_thread`] for thread detail; [`Environment::dispatch`] and
//!   [`Environment::request`] for RPCs. Reconnects on its own.
//! - [`pairing`] + [`auth::pair`]: turn a pasted pairing link into a saved environment.
//! - [`store`]: `environments.json` and the secret store.
//! - [`ShellState`] / [`ThreadState`]: pure reducers over protocol stream items.
//! - [`commands`]: builders for `orchestration.dispatchCommand`.

pub mod auth;
pub mod commands;
pub mod connection;
pub mod environment;
pub mod http;
pub mod pairing;
pub mod rpc;
pub mod runtime;
pub mod shell;
pub mod store;
pub mod thread;

pub use auth::{BearerEndpoint, ClientInfo, Endpoint, PairedEnvironment, pair};
pub use connection::{ConnectStage, ConnectionFailure, ConnectionStatus};
pub use environment::{Environment, EnvironmentOptions, Session, ThreadHandle};
pub use http::{EnvironmentHttp, HttpAuth, HttpError};
pub use rpc::{CloseReason, ConnectError, RpcConnection, RpcError, Subscription};
pub use shell::{ShellState, SyncStatus};
pub use thread::ThreadState;
