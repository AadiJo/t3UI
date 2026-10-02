//! The per-thread right panel model (spec 1.2, `rightPanelStore.ts` @ fe7d3092c): which
//! surfaces (tabs) a thread has, which one is active, and whether the panel is open. Pure data;
//! [`super::store`] wraps it in an entity.

/// A pull request shown in a tab (`pull-request:{ref}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestRef {
    /// Set when the tab was opened from the pull-request list (which spans servers); a tab
    /// beside a thread takes the thread's environment.
    pub environment_id: Option<String>,
    pub project_id: String,
    /// Lowercased host (`github.com`).
    pub host: Option<String>,
    pub repository: String,
    pub number: u64,
    pub url: Option<String>,
}

impl PullRequestRef {
    /// `pullRequestSurfaceId`: environment, project, host, repository and number.
    fn id(&self) -> String {
        let scope = self
            .environment_id
            .as_deref()
            .map_or_else(String::new, |environment| {
                format!("{}:", encode(environment))
            });
        let host = self.host.as_deref().map_or_else(String::new, |host| {
            format!("{}:", encode(&host.to_lowercase()))
        });
        format!(
            "{scope}{}:{host}{}:{}",
            encode(&self.project_id),
            encode(&self.repository),
            self.number
        )
    }
}

/// A simulator or emulator streamed into a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceTarget {
    pub host_id: String,
    pub device_id: String,
    pub platform: DevicePlatform,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevicePlatform {
    Ios,
    Android,
}

/// A tab of the right panel. Terminal, preview, pull-request, agents and device are typed
/// slots their owners fill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Surface {
    Diff,
    /// The workspace file browser.
    Files,
    /// One file open for reading. `reveal_request` bumps on every `open_file` so the preview
    /// scrolls to `reveal_line` again even when the path is already open. Attachment files
    /// (`attachment:{id}`) live in the thread's attachment store instead of the workspace.
    File {
        path: String,
        reveal_line: Option<u32>,
        reveal_request: u64,
        attachment_id: Option<String>,
    },
    /// Terminal tab (`terminal:{first id}`); owned by the terminal surface.
    Terminal {
        terminal_ids: Vec<String>,
        active_terminal_id: String,
        split_vertical: bool,
    },
    /// Browser preview (`browser:{tab}`, or `browser:new` before `preview.open` returns).
    Preview {
        tab_id: Option<String>,
    },
    PullRequest(PullRequestRef),
    /// The thread's linked pull requests.
    PullRequests,
    Agents,
    /// A device stream; `target` is unset for the device picker tab.
    Device {
        target: Option<DeviceTarget>,
        title: Option<String>,
    },
}

/// Stable identity of a surface (the web `id` string without its kind prefix).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceId {
    Diff,
    Files,
    File(String),
    Attachment(String),
    Terminal(String),
    Preview(Option<String>),
    PullRequest(String),
    PullRequests,
    Agents,
    /// `device` (picker) or `device:{host}:{device}`.
    Device(Option<String>),
}

/// Kinds that `open` / `toggle` address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceKind {
    Diff,
    Files,
    File,
    Terminal,
    Preview,
    PullRequest,
    PullRequests,
    Agents,
    Device,
}

impl Surface {
    pub fn id(&self) -> SurfaceId {
        match self {
            Self::Diff => SurfaceId::Diff,
            Self::Files => SurfaceId::Files,
            Self::File {
                attachment_id: Some(id),
                ..
            } => SurfaceId::Attachment(id.clone()),
            Self::File { path, .. } => SurfaceId::File(path.clone()),
            Self::Terminal { terminal_ids, .. } => {
                SurfaceId::Terminal(terminal_ids.first().cloned().unwrap_or_default())
            }
            Self::Preview { tab_id } => SurfaceId::Preview(tab_id.clone()),
            Self::PullRequest(pull_request) => SurfaceId::PullRequest(pull_request.id()),
            Self::PullRequests => SurfaceId::PullRequests,
            Self::Agents => SurfaceId::Agents,
            Self::Device { target, .. } => SurfaceId::Device(target.as_ref().map(|target| {
                format!("{}:{}", encode(&target.host_id), encode(&target.device_id))
            })),
        }
    }

    pub fn kind(&self) -> SurfaceKind {
        match self {
            Self::Diff => SurfaceKind::Diff,
            Self::Files => SurfaceKind::Files,
            Self::File { .. } => SurfaceKind::File,
            Self::Terminal { .. } => SurfaceKind::Terminal,
            Self::Preview { .. } => SurfaceKind::Preview,
            Self::PullRequest(_) => SurfaceKind::PullRequest,
            Self::PullRequests => SurfaceKind::PullRequests,
            Self::Agents => SurfaceKind::Agents,
            Self::Device { .. } => SurfaceKind::Device,
        }
    }
}

/// `encodeURIComponent` for the characters ids can contain.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// One thread's panel (`ThreadRightPanelState`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadPanel {
    pub is_open: bool,
    pub active: Option<SurfaceId>,
    pub surfaces: Vec<Surface>,
}

impl ThreadPanel {
    /// The active surface while open (`selectActiveRightPanelSurface`).
    pub fn active_surface(&self) -> Option<&Surface> {
        if !self.is_open {
            return None;
        }
        let active = self.active.as_ref()?;
        self.surfaces.iter().find(|surface| &surface.id() == active)
    }

    /// Nothing to remember: closed, no tabs (the store drops such entries).
    pub fn is_empty(&self) -> bool {
        !self.is_open && self.active.is_none() && self.surfaces.is_empty()
    }

    /// `upsertSurface`: appends `surface` if its id is new (an existing tab is kept as is),
    /// opens the panel, and activates it.
    pub fn upsert(&mut self, surface: Surface) {
        let id = surface.id();
        if !self.surfaces.iter().any(|existing| existing.id() == id) {
            self.surfaces.push(surface);
        }
        self.active = Some(id);
        self.is_open = true;
    }

    /// `open(kind)`: singletons upsert; preview reuses the first preview tab or adds
    /// `browser:new`. Files and terminals need a path or id (`open_file` / `open_terminal`).
    pub fn open(&mut self, kind: SurfaceKind) {
        let surface = match kind {
            SurfaceKind::Diff => Surface::Diff,
            SurfaceKind::Files => Surface::Files,
            SurfaceKind::PullRequests => Surface::PullRequests,
            SurfaceKind::Agents => Surface::Agents,
            SurfaceKind::Device => Surface::Device {
                target: None,
                title: None,
            },
            SurfaceKind::Preview => self
                .surfaces
                .iter()
                .find(|surface| surface.kind() == SurfaceKind::Preview)
                .cloned()
                .unwrap_or(Surface::Preview { tab_id: None }),
            SurfaceKind::File | SurfaceKind::Terminal | SurfaceKind::PullRequest => {
                self.is_open = true;
                return;
            }
        };
        self.upsert(surface);
    }

    /// `openFile(path, line)`: `.` opens the file browser; otherwise drops the standalone
    /// browser, reuses or appends `file:{path}` (trailing slashes trimmed), and bumps its
    /// reveal request. Lines below 1 are ignored.
    pub fn open_file(&mut self, path: &str, line: Option<u32>) {
        if path == "." {
            self.upsert(Surface::Files);
            return;
        }
        let trimmed = path.trim_end_matches('/');
        let path = if trimmed.is_empty() { path } else { trimmed };
        self.surfaces
            .retain(|surface| surface.kind() != SurfaceKind::Files);
        let id = SurfaceId::File(path.to_owned());
        let reveal_request = self
            .surfaces
            .iter()
            .find_map(|surface| match surface {
                Surface::File { reveal_request, .. } if surface.id() == id => Some(*reveal_request),
                _ => None,
            })
            .unwrap_or(0)
            + 1;
        let surface = Surface::File {
            path: path.to_owned(),
            reveal_line: line.filter(|line| *line >= 1),
            reveal_request,
            attachment_id: None,
        };
        match self
            .surfaces
            .iter_mut()
            .find(|existing| existing.id() == id)
        {
            Some(existing) => *existing = surface,
            None => self.surfaces.push(surface),
        }
        self.active = Some(id);
        self.is_open = true;
    }

    /// `openTerminal(id)`.
    pub fn open_terminal(&mut self, terminal_id: &str) {
        self.upsert(Surface::Terminal {
            terminal_ids: vec![terminal_id.to_owned()],
            active_terminal_id: terminal_id.to_owned(),
            split_vertical: false,
        });
    }

    /// `openBrowser(tabId)`: a real tab replaces the `browser:new` placeholder.
    pub fn open_browser(&mut self, tab_id: Option<&str>) {
        if tab_id.is_some() {
            self.surfaces
                .retain(|surface| surface.id() != SurfaceId::Preview(None));
        }
        self.upsert(Surface::Preview {
            tab_id: tab_id.map(str::to_owned),
        });
    }

    /// `openPullRequest(target)`: a URL refreshes the stored reference.
    pub fn open_pull_request(&mut self, target: PullRequestRef) {
        let has_url = target.url.is_some();
        let surface = Surface::PullRequest(target);
        let id = surface.id();
        if has_url
            && let Some(existing) = self
                .surfaces
                .iter_mut()
                .find(|existing| existing.id() == id)
        {
            *existing = surface.clone();
        }
        self.upsert(surface);
    }

    pub fn activate(&mut self, id: &SurfaceId) {
        if self.surfaces.iter().any(|surface| &surface.id() == id) {
            self.active = Some(id.clone());
            self.is_open = true;
        }
    }

    /// `closeSurface(id)`: the next active tab is the one now at the closed index (or the last);
    /// the panel stays open only while tabs remain.
    pub fn close_surface(&mut self, id: &SurfaceId) {
        let Some(index) = self.surfaces.iter().position(|surface| &surface.id() == id) else {
            return;
        };
        self.surfaces.remove(index);
        if self.active.as_ref() == Some(id) {
            self.active = self
                .surfaces
                .get(index.min(self.surfaces.len().saturating_sub(1)))
                .map(Surface::id);
        }
        self.is_open = self.is_open && !self.surfaces.is_empty();
    }

    /// `closeOtherSurfaces(id)`; no-op with a single tab.
    pub fn close_others(&mut self, id: &SurfaceId) {
        if self.surfaces.len() > 1 && self.surfaces.iter().any(|surface| &surface.id() == id) {
            self.surfaces.retain(|surface| &surface.id() == id);
            self.active = Some(id.clone());
            self.is_open = true;
        }
    }

    /// `closeSurfacesToRight(id)`.
    pub fn close_to_right(&mut self, id: &SurfaceId) {
        let Some(index) = self.surfaces.iter().position(|surface| &surface.id() == id) else {
            return;
        };
        self.surfaces.truncate(index + 1);
        let active_kept = self
            .active
            .as_ref()
            .is_some_and(|active| self.surfaces.iter().any(|surface| &surface.id() == active));
        if !active_kept {
            self.active = Some(id.clone());
        }
    }

    /// `closeAllSurfaces`.
    pub fn close_all(&mut self) {
        if !self.surfaces.is_empty() {
            self.surfaces.clear();
            self.active = None;
            self.is_open = false;
        }
    }

    /// `toggle(kind)`: closes the panel when it shows that kind, else opens it.
    pub fn toggle(&mut self, kind: SurfaceKind) {
        if self.active_surface().map(Surface::kind) == Some(kind) {
            self.is_open = false;
        } else {
            self.open(kind);
        }
    }

    /// `toggleVisibility`: flips `is_open` and keeps the tabs.
    pub fn toggle_visibility(&mut self) {
        self.is_open = !self.is_open;
    }
}

#[cfg(test)]
mod tests {
    //! Failure modes: duplicate singleton tabs; an existing tab replaced by `open` (the fork
    //! keeps it); `open_file` leaving the file browser around, not re-revealing an already open
    //! file, keeping trailing slashes, or opening a `.` tab instead of the browser; zero reveal
    //! lines kept; closing the active tab activating the wrong neighbour or leaving the panel
    //! open with no tabs; "close others" acting on a lone tab; "close to the right" losing the
    //! active tab; `toggle` closing when another kind is shown; hidden panels reporting an
    //! active surface; pull-request ids not distinguishing hosts or environments.
    use super::*;

    fn ids(panel: &ThreadPanel) -> Vec<SurfaceId> {
        panel.surfaces.iter().map(Surface::id).collect()
    }

    #[test]
    fn singletons_are_not_duplicated() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Agents);
        panel.open(SurfaceKind::Diff);
        assert_eq!(ids(&panel), [SurfaceId::Diff, SurfaceId::Agents]);
        assert_eq!(panel.active, Some(SurfaceId::Diff));
        assert!(panel.is_open);
    }

    #[test]
    fn open_file_replaces_the_browser_and_rereveals() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Files);
        panel.open_file("src/a.ts/", Some(3));
        panel.open_file("src/a.ts", Some(0));
        assert_eq!(ids(&panel), [SurfaceId::File("src/a.ts".into())]);
        assert_eq!(
            panel.surfaces[0],
            Surface::File {
                path: "src/a.ts".into(),
                reveal_line: None,
                reveal_request: 2,
                attachment_id: None,
            }
        );
        panel.open_file(".", None);
        assert_eq!(panel.active, Some(SurfaceId::Files));
    }

    #[test]
    fn closing_picks_the_neighbour_and_closes_when_empty() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Agents);
        panel.open_file("a", None);
        panel.activate(&SurfaceId::Agents);
        panel.close_surface(&SurfaceId::Agents);
        assert_eq!(panel.active, Some(SurfaceId::File("a".into())));
        panel.close_others(&SurfaceId::File("a".into()));
        assert_eq!(ids(&panel), [SurfaceId::File("a".into())]);
        let lone = panel.clone();
        panel.close_others(&SurfaceId::File("a".into()));
        assert_eq!(panel, lone);
        panel.close_surface(&SurfaceId::File("a".into()));
        assert!(!panel.is_open);
        assert!(panel.is_empty());
    }

    #[test]
    fn close_to_the_right_keeps_or_moves_active() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Diff);
        panel.open(SurfaceKind::Agents);
        panel.open(SurfaceKind::Files);
        panel.close_to_right(&SurfaceId::Diff);
        assert_eq!(ids(&panel), [SurfaceId::Diff]);
        assert_eq!(panel.active, Some(SurfaceId::Diff));
    }

    #[test]
    fn toggle_and_visibility() {
        let mut panel = ThreadPanel::default();
        panel.toggle(SurfaceKind::Diff);
        assert!(panel.is_open);
        panel.toggle(SurfaceKind::Files);
        assert_eq!(panel.active, Some(SurfaceId::Files));
        panel.toggle(SurfaceKind::Files);
        assert!(!panel.is_open);
        assert_eq!(panel.active_surface(), None);
        panel.toggle_visibility();
        assert_eq!(panel.active_surface(), Some(&Surface::Files));
    }

    #[test]
    fn preview_placeholder_is_replaced() {
        let mut panel = ThreadPanel::default();
        panel.open(SurfaceKind::Preview);
        assert_eq!(ids(&panel), [SurfaceId::Preview(None)]);
        panel.open_browser(Some("tab-1"));
        assert_eq!(ids(&panel), [SurfaceId::Preview(Some("tab-1".into()))]);
    }

    #[test]
    fn pull_request_ids_match_the_fork() {
        let pull_request = PullRequestRef {
            environment_id: Some("env 1".into()),
            project_id: "project-aurora".into(),
            host: Some("GitHub.com".into()),
            repository: "acme/aurora".into(),
            number: 42,
            url: None,
        };
        assert_eq!(
            Surface::PullRequest(pull_request.clone()).id(),
            SurfaceId::PullRequest("env%201:project-aurora:github.com:acme%2Faurora:42".into())
        );
        let beside_thread = PullRequestRef {
            environment_id: None,
            host: None,
            ..pull_request
        };
        assert_eq!(
            Surface::PullRequest(beside_thread).id(),
            SurfaceId::PullRequest("project-aurora:acme%2Faurora:42".into())
        );
    }
}
