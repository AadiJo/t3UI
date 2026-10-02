//! Composer logic without GPUI: the prompt string and its chips, the `@` / `$` / `/` command
//! menu, provider and model resolution, traits, pending approvals and questions, send rules, and
//! the persisted draft shape. `t3_app::composer` renders these.

pub mod draft;
pub mod menu;
pub mod pending;
pub mod prompt;
pub mod providers;
pub mod search;
pub mod send;

#[cfg(test)]
mod tests;
