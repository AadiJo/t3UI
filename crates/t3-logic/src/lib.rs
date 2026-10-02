//! GPUI-free UI logic ported from the web client, so it can be unit-tested without a window:
//! keyboard shortcut resolution, sidebar grouping/sorting/status, client settings, persisted UI
//! state, timestamp formatting, and the chat timeline rows. `t3-app` wraps these in entities and renders them.

pub mod composer;
pub mod keybindings;
pub mod paths;
pub mod refs;
pub mod settings;
pub mod sidebar;
pub mod time;
pub mod timeline;
pub mod ui_state;

pub use refs::{ProjectRef, ThreadRef};
