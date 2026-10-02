//! GPUI views over the core model (default `gpui` feature).
//!
//! - [`DiffView`]: the diff panel body; emits [`DiffViewEvent`].
//! - [`ChangedFilesTree`]: the changed-files tree; emits [`ChangedFilesTreeEvent`].

mod diff_view;
mod highlight;
mod icons;
mod rows;
mod style;
mod tree;

pub use diff_view::{DiffView, DiffViewEvent};
pub use tree::{ChangedFilesTree, ChangedFilesTreeEvent};
