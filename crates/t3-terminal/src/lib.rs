//! Terminal emulator view for T3 Code's terminal drawer and terminal panel.
//!
//! `alacritty_terminal` holds the grid and parses output; a custom GPUI element paints it to
//! match the fork's xterm.js 6 setup (`ThreadTerminalDrawer.tsx:389-398`, DOM renderer).
// Without the view, the pure modules below are only exercised by their tests.
#![cfg_attr(not(feature = "gpui"), allow(dead_code))]

#[cfg(feature = "gpui")]
mod element;
mod input;
mod links;
mod session;
#[cfg(feature = "gpui")]
mod theme;
#[cfg(feature = "gpui")]
mod view;

pub use links::{TerminalLinkKind, resolve_path_link_target};
#[cfg(feature = "gpui")]
pub use view::{TerminalEvent, TerminalView};
