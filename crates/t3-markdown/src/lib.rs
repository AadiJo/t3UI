//! Chat markdown for T3UI: parses messages the way the fork's `ChatMarkdown` does and renders
//! them with GPUI.
//!
//! The parsing side ([`parse`], [`streaming`], [`links`], [`document`]) is plain Rust and builds
//! without GPUI (`--no-default-features`), so it is unit-tested on any host. The renderer
//! ([`Markdown`], feature `gpui`, on by default) turns parsed chunks into elements that match the
//! fork's typography, code block and table chrome, and joins the window text selection.

pub mod copy;
pub mod document;
pub mod links;
pub mod parse;
pub mod streaming;

#[cfg(feature = "gpui")]
mod inline_text;
#[cfg(feature = "gpui")]
mod render;
#[cfg(feature = "gpui")]
pub mod style;
#[cfg(feature = "gpui")]
mod view;

pub use document::Document;
pub use parse::{ParseOptions, parse};

#[cfg(feature = "gpui")]
pub use style::{MarkdownColors, MarkdownStyle};
#[cfg(feature = "gpui")]
pub use view::{Markdown, MarkdownEvent, MarkdownOptions};
