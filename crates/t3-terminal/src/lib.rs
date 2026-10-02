//! Terminal emulator view for T3 Code's terminal drawer and terminal panel.
//!
//! `alacritty_terminal` holds the grid and parses output; a custom GPUI element paints it to
//! match the fork's xterm.js 6 setup (`ThreadTerminalDrawer.tsx:389-398`, DOM renderer).
//! Colors and the mono family come from the active `t3-ui` theme.
//!
//! Wiring one terminal to the server (`docs/spec/panels.md` §3.6-3.7):
//!
//! ```ignore
//! t3_terminal::init(cx); // once, after gpui_kit::init and t3_ui::init
//! let view = cx.new(|cx| TerminalView::new(window, cx));
//! cx.subscribe(&view, |_, _, event, cx| match event {
//!     TerminalEvent::Input(data) => { /* terminal.write {threadId, terminalId, data} */ }
//!     TerminalEvent::Resize { cols, rows } => { /* terminal.resize, latest wins */ }
//!     TerminalEvent::LinkActivated { kind, text, position } => { /* preview menu or editor */ }
//!     TerminalEvent::SelectionMenuRequested { .. } => { /* native "Add to chat" menu */ }
//! });
//! // terminal.attach stream (Ack every chunk):
//! //   snapshot / restarted -> view.feed_snapshot(&snapshot.history, cx)
//! //   output               -> view.feed_output(&data, cx)
//! //   cleared              -> view.reset(cx)
//! //   error                -> view.write_system_message(&message, cx)
//! //   exited / closed      -> write_system_message("Process exited" / "Terminal closed"),
//! //                           then close the tab (terminal.close {deleteHistory: true})
//! ```
//!
//! Keep the entity alive while its drawer is hidden so scrollback survives. Lay the view out at
//! its final size during the drawer's open animation (clip, don't shrink): every size change
//! reflows the grid and emits a resize.
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
pub use view::{KEY_CONTEXT, TerminalEvent, TerminalView, init};
