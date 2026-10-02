//! The right panel (spec 1, 2, 4.1, 4.2): per-thread tabs of surfaces beside the chat column.
//!
//! - [`model`]: the per-thread tab model (pure).
//! - [`store::RightPanels`]: global state; the chat header toggles and keybindings call it.
//! - [`context::PanelContext`]: what a surface knows (thread, cwd, RPC helper).
//! - [`thread_detail::ThreadDetail`]: the open thread's full state for surfaces.
//! - [`view::RightPanel`]: the panel view the workspace mounts beside the chat column.
//! - Surfaces: [`diff_panel`], [`files`] (browser + preview), [`plan`].

pub mod context;
pub mod diff_panel;
pub mod files;
pub mod model;
pub mod plan;
pub mod store;
pub mod thread_detail;
pub mod view;

pub use context::{FixtureResponder, PanelContext};
pub use model::{Surface, SurfaceId, SurfaceKind, ThreadPanel};
pub use store::RightPanels;
pub use thread_detail::ThreadDetail;
pub use view::RightPanel;
