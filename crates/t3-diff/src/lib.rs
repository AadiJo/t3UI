//! The diff viewer and changed-files tree of the right panel.
//!
//! GPUI-free core (always built, tested with `cargo test -p t3-diff --no-default-features`):
//! - [`patch`]: parses unified git patches into files, hunks and lines.
//! - [`color`], [`palette`]: CSS `color-mix` and the resolved diff/tree colors.
//! - [`rows`]: unified/split row model of a file.
//! - [`tree`]: the changed-files tree model and compact counts.
//!
//! GPUI views (default `gpui` feature) build on the core.

pub mod color;
pub mod palette;
pub mod patch;
pub mod rows;
pub mod tree;
