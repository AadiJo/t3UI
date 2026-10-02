//! Which main view is showing. Mirrors the web router (spec 1.2); the route is not persisted, so
//! a relaunch starts at [`Route::Index`].

use gpui_kit::SharedString;
use t3_logic::ThreadRef;

/// Id of a composer draft (a thread that has not started yet). Drafts live in the composer's
/// draft store, keyed by this id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DraftId(pub SharedString);

/// A settings page (`/settings/<page>`). Nav order, icons, and the view per page live in
/// `settings::pages`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    /// "Project" in the nav; shown only when a project is in scope.
    Projects,
    #[default]
    General,
    Appearance,
    Keybindings,
    /// "SnapShots" in the nav (`/settings/snap-shot`).
    SnapShot,
    Providers,
    Integrations,
    SourceControl,
    Storage,
    Connections,
    /// "Archive" in the nav (`/settings/archived`).
    Archived,
    /// No nav item; reached from General.
    Diagnostics,
    /// No nav item; reached from General (the nav highlights General).
    OpenSourceLicenses,
}

impl SettingsPage {
    /// Nav and breadcrumb label (`SETTINGS_SECTION_LABELS`, `SettingsBreadcrumb.tsx`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Projects => "Project",
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Keybindings => "Keybindings",
            Self::SnapShot => "SnapShots",
            Self::Providers => "Providers",
            Self::Integrations => "Integrations",
            Self::SourceControl => "Source Control",
            Self::Storage => "Storage",
            Self::Connections => "Connections",
            Self::Archived => "Archive",
            Self::Diagnostics => "Diagnostics",
            Self::OpenSourceLicenses => "Open source licenses",
        }
    }

    /// The web path, e.g. `/settings/snap-shot`.
    pub fn path(self) -> &'static str {
        match self {
            Self::Projects => "/settings/projects",
            Self::General => "/settings/general",
            Self::Appearance => "/settings/appearance",
            Self::Keybindings => "/settings/keybindings",
            Self::SnapShot => "/settings/snap-shot",
            Self::Providers => "/settings/providers",
            Self::Integrations => "/settings/integrations",
            Self::SourceControl => "/settings/source-control",
            Self::Storage => "/settings/storage",
            Self::Connections => "/settings/connections",
            Self::Archived => "/settings/archived",
            Self::Diagnostics => "/settings/diagnostics",
            Self::OpenSourceLicenses => "/settings/open-source-licenses",
        }
    }
}

/// The main view's route.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Route {
    /// `/`: "No active thread".
    #[default]
    Index,
    /// `/$environmentId/$threadId`: a server thread.
    Thread(ThreadRef),
    /// `/draft/$draftId`: a new thread that has not started.
    Draft(DraftId),
    /// `/settings/<page>`.
    Settings(SettingsPage),
    /// `/pair`: connect to an environment.
    Pair,
}

impl Route {
    /// The server thread this route shows.
    pub fn thread(&self) -> Option<&ThreadRef> {
        match self {
            Self::Thread(thread) => Some(thread),
            _ => None,
        }
    }

    /// The draft this route shows.
    pub fn draft(&self) -> Option<&DraftId> {
        match self {
            Self::Draft(draft) => Some(draft),
            _ => None,
        }
    }

    /// True on any settings page; the sidebar then shows the settings nav.
    pub fn is_settings(&self) -> bool {
        matches!(self, Self::Settings(_))
    }
}
