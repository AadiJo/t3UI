//! The diff viewer and changed-files tree of the right panel.
//!
//! GPUI-free core (always built, tested with `cargo test -p t3-diff --no-default-features`):
//! - [`patch`]: parses unified git patches into files, hunks and lines.
//! - [`color`], [`palette`]: CSS `color-mix` and the resolved diff/tree colors.
//! - [`rows`]: unified/split row model of a file.
//! - [`review`]: review comments on line ranges and their prompt serialization.
//! - [`tree`]: the changed-files tree model and compact counts.
//! - [`word_diff`]: word-level emphasis of paired changed lines (jsdiff + Pierre `word-alt`).
//!
//! GPUI views (default `gpui` feature) build on the core: [`DiffView`] for the diff panel body
//! and [`ChangedFilesTree`] for the changed-files tree.

pub mod color;
pub mod palette;
pub mod patch;
pub mod review;
pub mod rows;
pub mod tree;
pub mod word_diff;

#[cfg(feature = "gpui")]
mod view;

#[cfg(feature = "gpui")]
pub use view::{ChangedFilesTree, ChangedFilesTreeEvent, DiffView, DiffViewEvent};
