//! Wire types for the T3 Code server protocol (upstream `packages/contracts`).
//!
//! GPUI-free and IO-free: only serde types and the Effect RPC frame codec. The upstream
//! server ships nightly, so decoders tolerate unknown fields and unknown `_tag` variants.

pub mod method;
pub mod rpc;

pub use method::{Stream, Unary};
