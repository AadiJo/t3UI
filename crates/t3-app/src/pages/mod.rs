//! Standalone workspace pages: Usage, Welcome/onboarding, the "no projects" hero, and the
//! shared page chrome other full-page views (Pull Requests, settings) build on.
//!
//! - [`chrome`]: `WorkspacePageHeader`, `WorkspacePageContainer`, `WorkspaceBreadcrumb` and the
//!   topbar scroll fade, as builders that return plain `Div`s.
//! - [`controls`]: the segmented toggle group, `InlineButton`, tabular numerals.
//! - [`usage`]: `/usage` ([`usage::UsageView`]).

pub mod chrome;
pub mod controls;
pub mod usage;
