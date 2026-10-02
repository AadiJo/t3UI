//! The settings page registry: nav order, nav icons, and the view each page shows.
//!
//! To add a page: create `settings/<page>.rs` with a view entity, declare the module in
//! `settings/mod.rs`, and add an arm to [`build`]. Pages without an arm show
//! [`PendingPage`], so the nav and routing work before the page exists. A page is built the
//! first time it shows and kept while settings stays open (see `SettingsView`).

use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, div,
};
use t3_ui::{ActiveColors as _, IconName, tokens::text};

use super::{archived, connections, general, keybindings, layout, providers, source_control};
use crate::{
    chrome::TypeScale as _,
    state::{AppState, SettingsPage},
};

/// Nav items in order (`SETTINGS_SECTION_LABELS` order in `settingsSearch.ts`).
/// Diagnostics and Open source licenses have no item.
///
/// Icons marked "stand-in" are not in the bundled lucide set yet: Appearance is `Palette`,
/// Project `PanelsTopLeft`, SnapShots a custom frame icon, Integrations `Blocks`, Storage
/// `HardDrive` in the fork.
pub const NAV: [(SettingsPage, IconName); 11] = [
    (SettingsPage::Projects, IconName::FolderTree), // stand-in
    (SettingsPage::General, IconName::Settings2),
    (SettingsPage::Appearance, IconName::Paintbrush), // stand-in
    (SettingsPage::Keybindings, IconName::Keyboard),
    (SettingsPage::SnapShot, IconName::Camera), // stand-in
    (SettingsPage::Providers, IconName::Bot),
    (SettingsPage::Integrations, IconName::Zap), // stand-in
    (SettingsPage::SourceControl, IconName::GitBranch),
    (SettingsPage::Storage, IconName::Container), // stand-in
    (SettingsPage::Connections, IconName::Link2),
    (SettingsPage::Archived, IconName::Archive),
];

/// The nav item highlighted for `page` (detail pages highlight their parent).
pub fn nav_item_for(page: SettingsPage) -> SettingsPage {
    match page {
        SettingsPage::OpenSourceLicenses | SettingsPage::Diagnostics => SettingsPage::General,
        page => page,
    }
}

/// Builds the view for `page`.
pub fn build(
    page: SettingsPage,
    app_state: Entity<AppState>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    match page {
        SettingsPage::General => cx
            .new(|cx| general::GeneralPage::new(app_state, window, cx))
            .into(),
        SettingsPage::Keybindings => cx
            .new(|cx| keybindings::KeybindingsPage::new(app_state, cx))
            .into(),
        SettingsPage::Providers => cx
            .new(|cx| providers::ProvidersPage::new(app_state, cx))
            .into(),
        SettingsPage::SourceControl => cx
            .new(|cx| source_control::SourceControlPage::new(app_state, cx))
            .into(),
        SettingsPage::Connections => cx
            .new(|cx| connections::ConnectionsPage::new(app_state, window, cx))
            .into(),
        SettingsPage::Archived => cx
            .new(|cx| archived::ArchivedPage::new(app_state, cx))
            .into(),
        page => cx.new(|cx| PendingPage::new(page, app_state, cx)).into(),
    }
}

/// Stand-in for a page that is not built yet: its title in a page container.
pub struct PendingPage {
    page: SettingsPage,
    _app_state: Subscription,
}

impl PendingPage {
    pub fn new(page: SettingsPage, app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            page,
            _app_state: cx.observe(&app_state, |_, _, cx| cx.notify()),
        }
    }
}

impl Render for PendingPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.colors();
        layout::page(
            "settings-pending",
            layout::PAGE_MAX_WIDTH,
            [div()
                .type_scale(text::SM)
                .text_color(colors.muted_foreground)
                .child(format!(
                    "{} settings are not available yet.",
                    self.page.label()
                ))
                .into_any_element()],
        )
    }
}
