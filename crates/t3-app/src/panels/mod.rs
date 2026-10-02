//! The right panel (spec 1, 2, 4.1, 4.2): per-thread tabs of surfaces beside the chat column.
//!
//! - [`model`]: the per-thread tab model (pure).
//! - [`store::RightPanels`]: global state; the chat header toggles and keybindings call it.
//! - [`context::PanelContext`]: what a surface knows (thread, cwd, RPC helper).
//! - [`thread_detail::ThreadDetail`]: the open thread's full state for surfaces.
//! - Surfaces: [`files`] (browser + preview), [`plan`]; diff and the panel view follow.

pub mod context;
pub mod files;
pub mod model;
pub mod plan;
pub mod store;
pub mod thread_detail;

pub use context::{FixtureResponder, PanelContext};
pub use model::{Surface, SurfaceId, SurfaceKind, ThreadPanel};
pub use store::RightPanels;
pub use thread_detail::ThreadDetail;
