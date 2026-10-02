//! Networking and environment management for T3 Code servers.
//!
//! Runs on its own tokio runtime and is GPUI-free. The app talks to it through handles whose
//! futures and channels are executor-agnostic, so GPUI tasks can await them directly.
