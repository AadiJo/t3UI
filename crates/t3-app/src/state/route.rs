//! Which main view is showing. Mirrors the web router (spec 1.2); the route is not persisted, so
//! a relaunch starts at [`Route::Index`].

use gpui_kit::SharedString;
use t3_logic::ThreadRef;

/// Id of a composer draft (a thread that has not started yet). Drafts live in the composer's
/// draft store, keyed by this id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DraftId(pub SharedString);

/// A settings page (`/settings/<page>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    #[default]
    General,
    Keybindings,
    Providers,
    SourceControl,
    Connections,
    /// "Archive" in the nav (`/settings/archived`).
    Archived,
    /// Reached from General > About; no nav item.
    Diagnostics,
}

impl SettingsPage {
    /// Nav label.
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Keybindings => "Keybindings",
            Self::Providers => "Providers",
            Self::SourceControl => "Source Control",
            Self::Connections => "Connections",
            Self::Archived => "Archive",
            Self::Diagnostics => "Diagnostics",
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
