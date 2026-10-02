//! Chat markdown for T3UI: parses messages the way the fork's `ChatMarkdown` does and renders
//! them with GPUI.
//!
//! The parsing side ([`parse`], [`streaming`], [`links`], [`document`]) is plain Rust and builds
//! without GPUI (`--no-default-features`), so it is unit-tested on any host.

pub mod document;
pub mod links;
pub mod parse;
pub mod streaming;

pub use document::Document;
pub use parse::{ParseOptions, parse};
